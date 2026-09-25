//! Particles: a port of vanilla `TerrainParticle`, `BreakingItemParticle`,
//! `EndRodParticle`, `Particle`, `ClientLevel.addDestroyBlockEffect`, and the
//! `ClientboundLevelParticles` spawn path.

use std::collections::HashMap;
use std::sync::Arc;

use azalea_block::BlockState;
use azalea_core::position::BlockPos;
use azalea_entity::particle::Particle as ParticleOptions;
use azalea_protocol::packets::game::c_explode::{ExplosionParticleInfo, Weighted};
use glam::{DVec3, EulerRot, Quat, dvec3};

use crate::physics::aabb::Aabb;
use crate::physics::block_shape::{self, LocalBox};
use crate::physics::collision::resolve_collision;
use crate::renderer::ParticleQuad;
use crate::renderer::chunk::atlas::{AtlasRegion, AtlasUVMap};
use crate::renderer::chunk::mesher::{
    BiomeClimate, Colormap, blend_color, dry_foliage_color, foliage_color, grass_color,
    world_brightness,
};
use crate::renderer::pipelines::particle::MAX_PARTICLE_QUADS as MAX_PARTICLES;
use crate::world::block::registry::{BlockRegistry, Tint};
use crate::world::block::{block_id, is_air};
use crate::world::chunk::ChunkStore;

/// Vanilla `ParticleGroup.RESERVOIR_START` — above this, new particles are
/// probabilistically dropped.
const RESERVOIR_START: usize = 12288;
/// Vanilla `Particle.MAXIMUM_COLLISION_VELOCITY_SQUARED` (100²).
const MAX_COLLISION_VELOCITY_SQ: f64 = 10000.0;
/// Terrain particles use the default 0.2-wide, 0.2-tall bounding box.
const HALF_WIDTH: f64 = 0.1;

#[derive(PartialEq, Eq)]
enum Kind {
    /// `TerrainParticle` / `BreakingItemParticle`: collision physics,
    /// world-lit, fixed sprite, opaque layer.
    Terrain,
    /// `BreakingItemParticle`: terrain-like physics with an item-model particle
    /// icon.
    Item,
    ItemTranslucent,
    /// `EndRodParticle` (a `SimpleAnimatedParticle`): no collision,
    /// full-bright, 8-frame animation, fades after half-life, translucent
    /// layer.
    EndRod,
    /// `TotemParticle` (`SimpleAnimatedParticle`), independently parameterized
    /// from EndRod despite sharing its official `glitter_7..0` sprite frames.
    Totem,
    /// `POOF` uses vanilla `ExplodeParticle`'s short velocity-driven animation.
    Poof,
    /// `EXPLOSION` uses vanilla `HugeExplosionParticle`, a large stationary
    /// quad.
    Explosion,
    /// Standard explosion block effect `SMOKE`.
    Smoke,
    Crit,
    Dust,
    Shriek,
    Trail,
    Vibration,
}

impl Kind {
    /// Vanilla `SingleQuadParticle.getLayer`.
    fn translucent(&self) -> bool {
        matches!(
            self,
            Kind::ItemTranslucent | Kind::EndRod | Kind::Totem | Kind::Shriek | Kind::Vibration
        )
    }
}

pub struct Particle {
    kind: Kind,
    /// Bounding-box bottom-center, like vanilla `Particle.setPos`.
    pos: DVec3,
    prev_pos: DVec3,
    vel: DVec3,
    age: i32,
    lifetime: i32,
    on_ground: bool,
    stopped_by_collision: bool,
    /// Vanilla `Particle.gravity` and `friction`.
    gravity: f64,
    friction: f64,
    /// Vanilla `quadSize`; the billboard spans twice this.
    size: f32,
    base_size: f32,
    u0: f32,
    u1: f32,
    v0: f32,
    v1: f32,
    color: [f32; 3],
    alpha: f32,
    light: f32,
    target: Option<DVec3>,
    entity_target: Option<(i32, f32)>,
    delay: i32,
    rotation: Quat,
    second_rotation: Option<Quat>,
    rot: f32,
    rot_o: f32,
    pitch: f32,
    pitch_o: f32,
}

impl Particle {
    /// Vanilla `TerrainParticle` constructor chain: velocity jitter +
    /// normalization from `Particle(level, x, y, z, xa, ya, za)`, then the
    /// terrain quad size and random quarter sub-tile.
    fn terrain(
        pos: DVec3,
        velocity_arg: DVec3,
        region: AtlasRegion,
        color: [f32; 3],
        light: f32,
    ) -> Self {
        let jitter = || (fastrand::f64() * 2.0 - 1.0) * 0.4;
        let dir = velocity_arg + DVec3::new(jitter(), jitter(), jitter());
        let speed = (fastrand::f64() + fastrand::f64() + 1.0) * 0.15;
        let mut vel = dir / dir.length() * speed * 0.4;
        vel.y += 0.1;

        let lifetime = (4.0 / (fastrand::f32() * 0.9 + 0.1)) as i32;
        // SingleQuadParticle base size, halved by TerrainParticle.
        let size = 0.1 * (fastrand::f32() * 0.5 + 0.5) * 2.0 / 2.0;

        // Random quarter sub-tile of the block sprite. The u0/u1 flip is
        // vanilla (`getU0` samples uo+1, `getU1` samples uo).
        let sprite_u = |f: f32| region.u_min + f * (region.u_max - region.u_min);
        let sprite_v = |f: f32| region.v_min + f * (region.v_max - region.v_min);
        let uo = fastrand::f32() * 3.0;
        let vo = fastrand::f32() * 3.0;

        Self {
            kind: Kind::Terrain,
            pos,
            prev_pos: pos,
            vel,
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: 1.0,
            friction: 0.98,
            size,
            base_size: size,
            u0: sprite_u((uo + 1.0) / 4.0),
            u1: sprite_u(uo / 4.0),
            v0: sprite_v(vo / 4.0),
            v1: sprite_v((vo + 1.0) / 4.0),
            color,
            alpha: 1.0,
            light,
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        }
    }

    /// Vanilla `EndRodParticle`: velocity taken verbatim from the spawn call
    /// (the 3-arg `Particle` constructor adds no jitter), tiny gravity, no
    /// collision, warm fade color.
    fn end_rod(pos: DVec3, vel: DVec3, frames: &[AtlasRegion; 8]) -> Self {
        // SingleQuadParticle base quadSize, then EndRodParticle *= 0.75.
        let size = 0.1 * (fastrand::f32() * 0.5 + 0.5) * 2.0 * 0.75;
        let mut p = Self {
            kind: Kind::EndRod,
            pos,
            prev_pos: pos,
            vel,
            age: 0,
            lifetime: 60 + fastrand::i32(0..12),
            on_ground: false,
            stopped_by_collision: false,
            gravity: f64::from(0.0125f32),
            friction: f64::from(0.91f32),
            size,
            base_size: size,
            u0: 0.0,
            u1: 0.0,
            v0: 0.0,
            v1: 0.0,
            color: [1.0; 3],
            alpha: 1.0,
            // SimpleAnimatedParticle.getLightCoords is always full-bright.
            light: 1.0,
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        p.set_sprite(&frames[0]);
        p
    }

    /// `POOF` provider: official 26.2 `ExplodeParticle` behavior.
    fn poof(pos: DVec3, velocity_arg: DVec3, frames: &[AtlasRegion; 8]) -> Self {
        let jitter = || ((fastrand::f32() * 2.0 - 1.0) * 0.05) as f64;
        let vel = velocity_arg + dvec3(jitter(), jitter(), jitter());
        let shade = fastrand::f32() * 0.3 + 0.7;
        let size = 0.1 * (fastrand::f32() * fastrand::f32() * 6.0 + 1.0);
        let lifetime = (16.0 / (f64::from(fastrand::f32()) * 0.8 + 0.2)) as i32 + 2;
        let mut p = Self {
            kind: Kind::Poof,
            pos,
            prev_pos: pos,
            vel,
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: f64::from(-0.1f32),
            friction: f64::from(0.9f32),
            size,
            base_size: size,
            u0: 0.0,
            u1: 0.0,
            v0: 0.0,
            v1: 0.0,
            color: [shade; 3],
            alpha: 1.0,
            // Until the first tick samples the world, never render this as full-bright.
            light: 0.0,
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        p.set_sprite(&frames[0]);
        p
    }

    /// `EXPLOSION` provider: `HugeExplosionParticle` uses xAux as size and
    /// ignores velocity for its stationary, full-bright quad.
    fn huge_explosion(pos: DVec3, auxiliary: DVec3, frames: &[AtlasRegion; 16]) -> Self {
        let lifetime = 6 + fastrand::i32(0..4);
        let shade = fastrand::f32() * 0.6 + 0.4;
        let size = 2.0 * (1.0 - auxiliary.x as f32 * 0.5);
        let mut p = Self {
            kind: Kind::Explosion,
            pos,
            prev_pos: pos,
            vel: DVec3::ZERO,
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: 0.0,
            friction: 0.98,
            size,
            base_size: size,
            u0: 0.0,
            u1: 0.0,
            v0: 0.0,
            v1: 0.0,
            color: [shade; 3],
            alpha: 1.0,
            light: 1.0,
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        p.set_sprite(&frames[0]);
        p
    }

    /// Standard `SMOKE` block-particle provider based on
    /// `BaseAshSmokeParticle`.
    fn smoke(pos: DVec3, velocity: DVec3, frames: &[AtlasRegion; 8]) -> Self {
        let jitter = || (fastrand::f32() * 2.0 - 1.0) * 0.4;
        let mut base_velocity = dvec3(
            f64::from(jitter()),
            f64::from(jitter()),
            f64::from(jitter()),
        );
        let speed = (fastrand::f32() + fastrand::f32() + 1.0) * 0.15;
        base_velocity = base_velocity / base_velocity.length() * f64::from(speed * 0.4);
        base_velocity.y += 0.1;
        let velocity = base_velocity * 0.1 + velocity;
        let base_size = 0.1 * (fastrand::f32() * 0.5 + 0.5) * 2.0 * 0.75;
        let shade = fastrand::f32() * 0.3;
        let lifetime = (8.0 / (fastrand::f32() * 0.8 + 0.2)) as i32;
        let mut p = Self {
            kind: Kind::Smoke,
            pos,
            prev_pos: pos,
            vel: velocity,
            age: 0,
            lifetime: lifetime.max(1),
            on_ground: false,
            stopped_by_collision: false,
            gravity: f64::from(-0.1f32),
            friction: f64::from(0.96f32),
            size: 0.0,
            base_size,
            u0: 0.0,
            u1: 0.0,
            v0: 0.0,
            v1: 0.0,
            color: [shade; 3],
            alpha: 1.0,
            light: 1.0,
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        p.set_sprite(&frames[0]);
        p
    }

    fn dust(
        pos: DVec3,
        velocity: DVec3,
        color: [f32; 3],
        scale: f32,
        sprite: AtlasRegion,
        rng: &mut fastrand::Rng,
    ) -> Self {
        let scale = scale.clamp(0.01, 4.0);
        let base_size = 0.1 * (0.75 * scale);
        let base_lifetime = (8.0 / (rng.f64() * 0.8 + 0.2)) as i32;
        let lifetime = ((base_lifetime as f32 * scale).max(1.0)) as i32;
        let base_factor = rng.f32() * 0.4 + 0.6;
        let color = color.map(|channel| (rng.f32() * 0.2 + 0.8) * channel * base_factor);
        let mut particle = Self {
            kind: Kind::Dust,
            pos,
            prev_pos: pos,
            vel: velocity * 0.1,
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: 0.0,
            friction: 0.96,
            size: base_size,
            base_size,
            u0: sprite.u_min,
            u1: sprite.u_max,
            v0: sprite.v_min,
            v1: sprite.v_max,
            color,
            alpha: 1.0,
            light: 0.0,
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        particle.set_sprite(&sprite);
        particle
    }

    /// `CritParticle` provider; constructor performs one synchronous particle
    /// tick.
    fn crit(
        pos: DVec3,
        velocity: DVec3,
        magic: bool,
        sprite: AtlasRegion,
        chunks: &ChunkStore,
    ) -> Self {
        let size = 0.1 * (fastrand::f32() * 0.5 + 0.5) * 2.0 * 0.75;
        let shade = fastrand::f32() * 0.3 + 0.6;
        let lifetime = (6.0 / (fastrand::f32() * 0.8 + 0.6)) as i32;
        let mut p = Self {
            kind: Kind::Crit,
            pos,
            prev_pos: pos,
            vel: velocity * 0.4,
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: 0.5,
            friction: 0.7,
            size,
            base_size: size,
            u0: sprite.u_min,
            u1: sprite.u_max,
            v0: sprite.v_min,
            v1: sprite.v_max,
            color: [shade, shade, shade],
            alpha: 1.0,
            light: world_brightness(
                chunks,
                pos.x.floor() as i32,
                pos.y.floor() as i32,
                pos.z.floor() as i32,
            ),
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        if magic {
            p.color[0] *= 0.3;
            p.color[1] *= 0.8;
        }
        p.tick(chunks, &[sprite; 8], &[sprite; 8], &[sprite; 16]);
        p
    }

    /// Vanilla `TotemParticle`: verbatim emitter velocity, full-bright animated
    /// sprite, and no synchronous constructor tick.
    fn totem(pos: DVec3, velocity: DVec3, frames: &[AtlasRegion; 8]) -> Self {
        let size = 0.1 * (fastrand::f32() * 0.5 + 0.5) * 2.0 * 0.75;
        let lifetime = 60 + fastrand::i32(0..12);
        let rare = fastrand::u8(0..4) == 0;
        let color = totem_color(rare, [fastrand::f32(), fastrand::f32(), fastrand::f32()]);
        let mut p = Self {
            kind: Kind::Totem,
            pos,
            prev_pos: pos,
            vel: velocity,
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: f64::from(1.25f32),
            friction: f64::from(0.6f32),
            size,
            base_size: size,
            u0: 0.0,
            u1: 0.0,
            v0: 0.0,
            v1: 0.0,
            color,
            alpha: 1.0,
            light: 1.0,
            target: None,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        p.set_sprite(&frames[0]);
        p
    }

    fn shriek(pos: DVec3, delay: i32, sprite: AtlasRegion) -> Self {
        let mut p = Self::special(Kind::Shriek, pos, DVec3::ZERO, None, 30, 0.85, sprite);
        p.delay = delay.max(0);
        p.vel.y = 0.1;
        p.rotation = Quat::from_rotation_x(-1.0472);
        p.second_rotation = Some(Quat::from_euler(
            EulerRot::YXZ,
            -std::f32::consts::PI,
            1.0472,
            0.0,
        ));
        p
    }

    fn trail(
        pos: DVec3,
        velocity: DVec3,
        target: DVec3,
        color: i32,
        duration: i32,
        sprite: AtlasRegion,
    ) -> Self {
        let color = color as u32;
        let variation = || fastrand::f32() * 0.25 + 0.875;
        let c = [
            ((color >> 16) & 0xff) as f32 / 255.0 * variation(),
            ((color >> 8) & 0xff) as f32 / 255.0 * variation(),
            (color & 0xff) as f32 / 255.0 * variation(),
        ];
        Self::special(
            Kind::Trail,
            pos,
            velocity,
            Some(target),
            duration.max(1),
            0.26,
            sprite,
        )
        .with_color(c)
    }

    fn vibration(pos: DVec3, target: DVec3, arrival_ticks: i32, sprite: AtlasRegion) -> Self {
        let mut particle = Self::special(
            Kind::Vibration,
            pos,
            DVec3::ZERO,
            Some(target),
            arrival_ticks.max(1),
            0.3,
            sprite,
        );
        particle.point_vibration_at(target, true);
        particle
    }

    fn point_vibration_at(&mut self, target: DVec3, initial: bool) {
        let delta = self.pos - target;
        self.rot_o = self.rot;
        self.pitch_o = self.pitch;
        self.rot = delta.x.atan2(delta.z) as f32;
        self.pitch = delta.y.atan2(delta.x.hypot(delta.z)) as f32;
        if initial {
            self.rot_o = self.rot;
            self.pitch_o = self.pitch;
        }
    }

    fn vibration_entity(
        pos: DVec3,
        entity_id: i32,
        y_offset: f32,
        arrival_ticks: i32,
        sprite: AtlasRegion,
    ) -> Self {
        let mut particle = Self::special(
            Kind::Vibration,
            pos,
            DVec3::ZERO,
            None,
            arrival_ticks.max(1),
            0.3,
            sprite,
        );
        particle.entity_target = Some((entity_id, y_offset));
        particle
    }

    fn special(
        kind: Kind,
        pos: DVec3,
        velocity: DVec3,
        target: Option<DVec3>,
        lifetime: i32,
        size: f32,
        sprite: AtlasRegion,
    ) -> Self {
        let mut p = Self {
            kind,
            pos,
            prev_pos: pos,
            vel: velocity,
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: 0.0,
            friction: 1.0,
            size,
            base_size: size,
            u0: sprite.u_min,
            u1: sprite.u_max,
            v0: sprite.v_min,
            v1: sprite.v_max,
            color: [1.0; 3],
            alpha: 1.0,
            light: 1.0,
            target,
            entity_target: None,
            delay: 0,
            rotation: Quat::IDENTITY,
            second_rotation: None,
            rot: 0.0,
            rot_o: 0.0,
            pitch: 0.0,
            pitch_o: 0.0,
        };
        p.set_sprite(&sprite);
        p
    }

    fn with_color(mut self, color: [f32; 3]) -> Self {
        self.color = color;
        self
    }

    /// Vanilla `SingleQuadParticle.setSprite`.
    fn set_sprite(&mut self, frame: &AtlasRegion) {
        self.u0 = frame.u_min;
        self.u1 = frame.u_max;
        self.v0 = frame.v_min;
        self.v1 = frame.v_max;
    }

    /// Vanilla `BreakingItemParticle`: the base-constructor velocity (zero
    /// argument, jitter only) scaled to 10%, plus the spawn velocity. Shares
    /// the halved quad size and quarter sub-tile sampling with terrain.
    fn breaking_item(pos: DVec3, velocity: DVec3, region: AtlasRegion, light: f32) -> Self {
        let mut p = Self::terrain(pos, DVec3::ZERO, region, [1.0; 3], light);
        p.kind = if region.translucent {
            Kind::ItemTranslucent
        } else {
            Kind::Item
        };
        p.vel = p.vel * 0.1 + velocity;
        p
    }

    /// Vanilla `Particle.tick`. Returns false when the particle expires.
    fn tick(
        &mut self,
        chunks: &ChunkStore,
        end_rod_frames: &[AtlasRegion; 8],
        generic_frames: &[AtlasRegion; 8],
        explosion_frames: &[AtlasRegion; 16],
    ) -> bool {
        self.tick_with_entity_lookup(
            chunks,
            end_rod_frames,
            generic_frames,
            explosion_frames,
            &mut |_| None,
        )
    }

    fn tick_with_entity_lookup(
        &mut self,
        chunks: &ChunkStore,
        end_rod_frames: &[AtlasRegion; 8],
        generic_frames: &[AtlasRegion; 8],
        explosion_frames: &[AtlasRegion; 16],
        lookup: &mut impl FnMut(i32) -> Option<TrackingAttachment>,
    ) -> bool {
        self.prev_pos = self.pos;
        if self.age >= self.lifetime {
            return false;
        }
        if self.kind == Kind::Shriek && self.delay > 0 {
            self.delay -= 1;
            return true;
        }
        self.age += 1;
        self.vel.y -= 0.04 * self.gravity;
        match self.kind {
            Kind::Terrain
            | Kind::Item
            | Kind::ItemTranslucent
            | Kind::Smoke
            | Kind::Poof
            | Kind::Dust => self.move_with_collision(chunks),
            Kind::EndRod | Kind::Crit | Kind::Shriek => self.pos += self.vel,
            Kind::Totem => self.move_with_collision(chunks),
            Kind::Trail | Kind::Vibration => {
                if let Some((entity_id, y_offset)) = self.entity_target {
                    let Some(attachment) = lookup(entity_id) else {
                        return false;
                    };
                    self.target = Some(attachment.position + dvec3(0.0, f64::from(y_offset), 0.0));
                }
                if let Some(target) = self.target {
                    let remaining = self.lifetime - self.age;
                    if self.kind == Kind::Vibration && self.age == 1 && self.entity_target.is_some()
                    {
                        // EntityPositionSource resolves on first lookup; vanilla's
                        // constructor sets both previous and current orientation.
                        self.point_vibration_at(target, true);
                    }
                    self.pos = target_step(self.pos, target, remaining);
                    if self.kind == Kind::Vibration {
                        self.point_vibration_at(target, false);
                    }
                }
            }
            Kind::Explosion => {}
        }
        if self.kind == Kind::Smoke && self.pos.y == self.prev_pos.y {
            self.vel.x *= 1.1;
            self.vel.z *= 1.1;
        }
        self.vel *= self.friction;
        if self.on_ground {
            self.vel.x *= 0.7;
            self.vel.z *= 0.7;
        }
        match self.kind {
            Kind::Terrain
            | Kind::Item
            | Kind::ItemTranslucent
            | Kind::Smoke
            | Kind::Poof
            | Kind::Dust => {
                self.light = world_brightness(
                    chunks,
                    self.pos.x.floor() as i32,
                    self.pos.y.floor() as i32,
                    self.pos.z.floor() as i32,
                );
            }
            Kind::Explosion => {
                self.set_sprite(&explosion_frames[explosion_frame_index(self.age, self.lifetime)])
            }
            Kind::Crit => {
                self.color[1] *= 0.96;
                self.color[2] *= 0.9;
                self.light = world_brightness(
                    chunks,
                    self.pos.x.floor() as i32,
                    self.pos.y.floor() as i32,
                    self.pos.z.floor() as i32,
                );
            }
            // SimpleAnimatedParticle.tick: advance the sprite frame, then
            // after half-life fade alpha out and lerp toward the fade color.
            Kind::EndRod | Kind::Totem => {
                self.set_sprite(&end_rod_frames[(self.age * 7 / self.lifetime) as usize]);
                if self.age > self.lifetime / 2 {
                    self.alpha = 1.0 - (self.age - self.lifetime / 2) as f32 / self.lifetime as f32;
                    if self.kind == Kind::EndRod {
                        for (c, f) in self.color.iter_mut().zip(END_ROD_FADE) {
                            *c += (f - *c) * 0.2;
                        }
                    }
                }
            }
            Kind::Shriek | Kind::Trail | Kind::Vibration => self.light = 1.0,
        }
        if self.kind == Kind::Shriek {
            self.alpha = 1.0 - (self.age as f32 / self.lifetime as f32).clamp(0.0, 1.0);
        }
        if matches!(self.kind, Kind::Poof | Kind::Smoke) {
            let frame = animated_frame_index(self.age, self.lifetime, 8);
            self.set_sprite(&generic_frames[frame]);
            if self.kind == Kind::Smoke {
                self.size = self.base_size
                    * ((self.age as f32 / self.lifetime as f32) * 32.0).clamp(0.0, 1.0);
            }
        }
        true
    }

    /// Vanilla `Particle.move`.
    fn move_with_collision(&mut self, chunks: &ChunkStore) {
        if self.stopped_by_collision {
            return;
        }
        let orig = self.vel;
        let mut delta = orig;
        if delta != DVec3::ZERO && delta.length_squared() < MAX_COLLISION_VELOCITY_SQ {
            let aabb = Aabb::from_center(self.pos, HALF_WIDTH, HALF_WIDTH);
            (delta, _) = resolve_collision(chunks, aabb, orig.into(), 0.0);
        }
        self.pos += delta;
        if orig.y.abs() >= 1e-5 && delta.y.abs() < 1e-5 {
            self.stopped_by_collision = true;
        }
        self.on_ground = orig.y != delta.y && orig.y < 0.0;
        if orig.x != delta.x {
            self.vel.x = 0.0;
        }
        if orig.z != delta.z {
            self.vel.z = 0.0;
        }
    }
}

/// Vanilla `TotemParticle` RGB expressions; samples are branch-local random
/// floats.
fn totem_color(rare: bool, samples: [f32; 3]) -> [f32; 3] {
    let [red, green, blue] = samples;
    if rare {
        [0.6 + red * 0.2, 0.6 + green * 0.3, blue * 0.2]
    } else {
        [0.1 + red * 0.2, 0.4 + green * 0.3, blue * 0.2]
    }
}

/// `SimpleAnimatedParticle.setFadeColor(0xF2DEC9)` in `EndRodParticle`.
const END_ROD_FADE: [f32; 3] = [242.0 / 255.0, 222.0 / 255.0, 201.0 / 255.0];

/// Exact order in `assets/minecraft/particles/explosion.json`.
pub const EXPLOSION_SPRITES: [&str; 16] = [
    "particle/explosion_0",
    "particle/explosion_1",
    "particle/explosion_2",
    "particle/explosion_3",
    "particle/explosion_4",
    "particle/explosion_5",
    "particle/explosion_6",
    "particle/explosion_7",
    "particle/explosion_8",
    "particle/explosion_9",
    "particle/explosion_10",
    "particle/explosion_11",
    "particle/explosion_12",
    "particle/explosion_13",
    "particle/explosion_14",
    "particle/explosion_15",
];

fn supports_explosion_particle(option: &ParticleOptions) -> bool {
    matches!(
        option,
        ParticleOptions::Explosion
            | ParticleOptions::ExplosionEmitter
            | ParticleOptions::EndRod
            | ParticleOptions::Poof
            | ParticleOptions::Smoke
    )
}

fn explosion_frame_index(age: i32, lifetime: i32) -> usize {
    ((age.max(0) as usize * 15) / lifetime.max(1) as usize).min(15)
}

fn animated_frame_index(age: i32, lifetime: i32, frames: usize) -> usize {
    ((age.max(0) as usize * (frames - 1)) / lifetime.max(1) as usize).min(frames - 1)
}

fn dust_quad_size(base_size: f32, age: i32, lifetime: i32, partial_tick: f32) -> f32 {
    base_size * (((age as f32 + partial_tick) / lifetime as f32) * 32.0).clamp(0.0, 1.0)
}

/// `crit.json` and `enchanted_hit.json` each register one provider-specific
/// sprite.
pub const CRIT_SPRITE: &str = "particle/critical_hit";
pub const ENCHANTED_HIT_SPRITE: &str = "particle/enchanted_hit";

/// `POOF` and `SMOKE` use the first eight entries. Dust is included last so
/// the atlas builder also packs its static sprite.
pub const GENERIC_PARTICLE_SPRITES: [&str; 12] = [
    "particle/generic_7",
    "particle/generic_6",
    "particle/generic_5",
    "particle/generic_4",
    "particle/generic_3",
    "particle/generic_2",
    "particle/generic_1",
    "particle/generic_0",
    "particle/dust",
    "particle/shriek",
    "particle/generic_0",
    "particle/vibration",
];

const DUST_SPRITE: &str = "particle/dust";

const EXPLOSION_EMITTER_TICKS: u8 = 8;
const fn explosion_emitter_children(age: u8) -> u8 {
    if age < EXPLOSION_EMITTER_TICKS { 6 } else { 0 }
}

fn explosion_emitter_size(age: u8) -> f64 {
    f64::from(age as f32 / f32::from(EXPLOSION_EMITTER_TICKS))
}

fn explosion_emitter_child(
    origin: DVec3,
    age: u8,
    mut sample: impl FnMut() -> f64,
) -> (DVec3, DVec3) {
    let pos = origin + dvec3(sample() * 4.0, sample() * 4.0, sample() * 4.0);
    let velocity = dvec3(explosion_emitter_size(age), 0.0, 0.0);
    (pos, velocity)
}

/// Frame order from `assets/minecraft/particles/end_rod.json`: frame 0 is
/// `glitter_7` and the animation walks toward `glitter_0`.
pub const END_ROD_SPRITES: [&str; 8] = [
    "particle/glitter_7",
    "particle/glitter_6",
    "particle/glitter_5",
    "particle/glitter_4",
    "particle/glitter_3",
    "particle/glitter_2",
    "particle/glitter_1",
    "particle/glitter_0",
];

/// Server-sent particle types with implemented vanilla-like effects. Payload
/// codecs are decoded separately; unsupported types are dropped, never treated
/// as simple options.
#[derive(Clone, Copy, Debug)]
pub enum ServerParticleKind {
    EndRod,
    ExplosionEmitter,
    Explosion,
    Poof,
    Smoke,
    Totem,
    Dust,
    Block,
    Item,
    Shriek,
    Trail,
    Vibration,
}

/// Wire options retained with a server particle. Only known exact codecs are
/// represented; unknown payload layouts are never assumed to be empty.
#[derive(Clone, Debug)]
pub enum ServerParticleOptions {
    Simple,
    Dust {
        packed_color: i32,
        scale: f32,
    },
    Block(BlockState),
    Item {
        item_id: u32,
        count: i32,
        components: azalea_inventory::DataComponentPatch,
    },
    Shriek {
        delay: i32,
    },
    Trail {
        target: DVec3,
        color: i32,
        duration: i32,
    },
    VibrationBlock {
        target: BlockPos,
        arrival_ticks: i32,
    },
    VibrationEntity {
        entity_id: i32,
        y_offset: f32,
        arrival_ticks: i32,
    },
}

impl ServerParticleKind {
    /// Maps a particle registry id (`ParticleTypes` registration order in the
    /// 26.2 reference; ids shift between versions). Pomme owns this mapping
    /// because azalea's particle wire enum is out of sync with the registry.
    pub fn from_id(id: u32) -> Option<Self> {
        match id {
            27 => Some(Self::EndRod),
            29 => Some(Self::ExplosionEmitter),
            30 => Some(Self::Explosion),
            66 => Some(Self::Poof),
            69 => Some(Self::Smoke),
            75 => Some(Self::Totem),
            21 => Some(Self::Dust),
            1 => Some(Self::Block),
            54 => Some(Self::Item),
            55 => Some(Self::Vibration),
            56 => Some(Self::Trail),
            112 => Some(Self::Shriek),
            _ => None,
        }
    }

    /// Vanilla `ParticleType.getOverrideLimiter`.
    fn override_limiter(self) -> bool {
        matches!(
            self,
            Self::ExplosionEmitter | Self::Explosion | Self::Poof | Self::Vibration
        )
    }
}

#[derive(Clone)]
struct TrackedExplosion {
    center: DVec3,
    radius: f32,
    block_count: i32,
    block_particles: Vec<Weighted<ExplosionParticleInfo>>,
}

#[derive(Clone, Debug, PartialEq)]
struct PlannedExplosionParticle {
    particle: ParticleOptions,
    pos: DVec3,
    velocity: DVec3,
}

struct ExplosionEmitter {
    pos: DVec3,
    age: u8,
}

fn plan_explosion_particles(
    explosions: &[TrackedExplosion],
    rng: &mut fastrand::Rng,
    mut is_air: impl FnMut(i32, i32, i32) -> bool,
) -> Vec<PlannedExplosionParticle> {
    let total_blocks: i64 = explosions
        .iter()
        .map(|e| i64::from(e.block_count.max(0)))
        .sum();
    let count = total_blocks.clamp(0, 512) as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut pick = rng.i64(0..total_blocks);
        let explosion = explosions
            .iter()
            .find(|e| {
                let w = i64::from(e.block_count.max(0));
                if pick < w {
                    true
                } else {
                    pick -= w;
                    false
                }
            })
            .expect("positive block count has a weighted explosion");
        if !explosion.radius.is_finite() || explosion.radius <= 0.0 {
            continue;
        }
        let raw_dir = dvec3(
            f64::from(rng.f32() * 2.0 - 1.0),
            f64::from(rng.f32() * 2.0 - 1.0),
            f64::from(rng.f32() * 2.0 - 1.0),
        );
        let length = raw_dir.length();
        let dir = if length < 1.0e-4 {
            DVec3::ZERO
        } else {
            raw_dir / length
        };
        let radius = (f64::from(rng.f32()).cbrt() as f32) * explosion.radius;
        let local = dir * f64::from(radius);
        let pos = explosion.center + local;
        if !is_air(
            pos.x.floor() as i32,
            pos.y.floor() as i32,
            pos.z.floor() as i32,
        ) {
            continue;
        }
        let speed = 0.5f32 / (radius / explosion.radius + 0.1) * rng.f32() * rng.f32() + 0.3;
        let weight_total: i64 = explosion
            .block_particles
            .iter()
            .map(|w| i64::from(w.weight.max(0)))
            .sum();
        if weight_total <= 0 {
            continue;
        }
        let mut particle_pick = rng.i64(0..weight_total);
        let info = explosion
            .block_particles
            .iter()
            .find(|w| {
                let weight = i64::from(w.weight.max(0));
                if particle_pick < weight {
                    true
                } else {
                    particle_pick -= weight;
                    false
                }
            })
            .expect("positive particle weight has an entry");
        out.push(PlannedExplosionParticle {
            particle: info.value.particle.clone(),
            pos: explosion.center + local * f64::from(info.value.scaling),
            velocity: dir * f64::from(speed * info.value.speed),
        });
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrackingParticleKind {
    Crit,
    EnchantedHit,
    Totem,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TrackingAttachment {
    pub position: DVec3,
    pub width: f64,
    pub height: f64,
}

struct TrackingEmitter {
    entity_id: Option<i32>,
    attachment: TrackingAttachment,
    age: u8,
    kind: TrackingParticleKind,
}

impl TrackingEmitter {
    fn advance(
        &mut self,
        lookup: &mut impl FnMut(i32) -> Option<TrackingAttachment>,
    ) -> Option<(TrackingParticleKind, TrackingAttachment)> {
        if let Some(id) = self.entity_id
            && let Some(attachment) = lookup(id)
        {
            self.attachment = attachment;
        }
        let lifetime = match self.kind {
            TrackingParticleKind::Crit | TrackingParticleKind::EnchantedHit => 3,
            TrackingParticleKind::Totem => 30,
        };
        if self.age >= lifetime {
            return None;
        }
        self.age += 1;
        Some((self.kind, self.attachment))
    }
}

fn tracking_sample(
    attachment: TrackingAttachment,
    samples: impl IntoIterator<Item = [f64; 3]>,
) -> Vec<(DVec3, DVec3)> {
    samples
        .into_iter()
        .take(16)
        .filter_map(|[x, y, z]| {
            if x * x + y * y + z * z > 1.0 {
                return None;
            }
            Some((
                attachment.position
                    + dvec3(
                        attachment.width * x / 4.0,
                        attachment.height * (0.5 + y / 4.0),
                        attachment.width * z / 4.0,
                    ),
                dvec3(x, y + 0.2, z),
            ))
        })
        .collect()
}

pub struct ParticleStore {
    particles: Vec<Particle>,
    emitters: Vec<ExplosionEmitter>,
    pending_emitters: Vec<ExplosionEmitter>,
    tracked_explosions: Vec<TrackedExplosion>,
    tracking_emitters: Vec<TrackingEmitter>,
    crit_sprite: AtlasRegion,
    enchanted_hit_sprite: AtlasRegion,
    /// Spawned this tick; drained after live particles tick, so a particle's
    /// first physics tick is the tick after it spawns (vanilla
    /// `ParticleEngine.particlesToAdd`).
    pending: Vec<Particle>,
    uv_map: AtlasUVMap,
    end_rod_frames: [AtlasRegion; 8],
    generic_frames: [AtlasRegion; 8],
    explosion_frames: [AtlasRegion; 16],
    grass_colormap: Arc<Colormap>,
    foliage_colormap: Arc<Colormap>,
    dry_foliage_colormap: Arc<Colormap>,
}

impl ParticleStore {
    pub fn new(
        uv_map: AtlasUVMap,
        grass_colormap: Arc<Colormap>,
        foliage_colormap: Arc<Colormap>,
        dry_foliage_colormap: Arc<Colormap>,
    ) -> Self {
        let end_rod_frames = END_ROD_SPRITES.map(|k| uv_map.get_region(k));
        let generic_frames =
            std::array::from_fn(|i| uv_map.get_region(GENERIC_PARTICLE_SPRITES[i]));
        let explosion_frames = EXPLOSION_SPRITES.map(|k| uv_map.get_region(k));
        let crit_sprite = uv_map.get_region(CRIT_SPRITE);
        let enchanted_hit_sprite = uv_map.get_region(ENCHANTED_HIT_SPRITE);
        Self {
            particles: Vec::new(),
            pending: Vec::new(),
            emitters: Vec::new(),
            pending_emitters: Vec::new(),
            tracked_explosions: Vec::new(),
            tracking_emitters: Vec::new(),
            crit_sprite,
            enchanted_hit_sprite,
            uv_map,
            end_rod_frames,
            generic_frames,
            explosion_frames,
            grass_colormap,
            foliage_colormap,
            dry_foliage_colormap,
        }
    }

    pub(crate) fn add_tracking_emitter(
        &mut self,
        entity_id: i32,
        kind: TrackingParticleKind,
        attachment: TrackingAttachment,
        world: &ChunkStore,
    ) {
        self.emit_tracking_batch(kind, attachment, world);
        self.tracking_emitters.push(TrackingEmitter {
            entity_id: Some(entity_id),
            attachment,
            age: 1,
            kind,
        });
    }

    pub(crate) fn detach_tracking_emitter(
        &mut self,
        entity_id: i32,
        final_attachment: Option<TrackingAttachment>,
    ) {
        for emitter in &mut self.tracking_emitters {
            if emitter.entity_id == Some(entity_id) {
                if let Some(attachment) = final_attachment {
                    emitter.attachment = attachment;
                }
                emitter.entity_id = None;
            }
        }
    }

    fn emit_tracking_batch(
        &mut self,
        kind: TrackingParticleKind,
        attachment: TrackingAttachment,
        world: &ChunkStore,
    ) {
        let samples = (0..16).map(|_| {
            [
                fastrand::f64() * 2.0 - 1.0,
                fastrand::f64() * 2.0 - 1.0,
                fastrand::f64() * 2.0 - 1.0,
            ]
        });
        for (pos, velocity) in tracking_sample(attachment, samples) {
            match kind {
                TrackingParticleKind::Crit => {
                    self.push(Particle::crit(
                        pos,
                        velocity,
                        false,
                        self.crit_sprite,
                        world,
                    ));
                }
                TrackingParticleKind::EnchantedHit => {
                    self.push(Particle::crit(
                        pos,
                        velocity,
                        true,
                        self.enchanted_hit_sprite,
                        world,
                    ));
                }
                TrackingParticleKind::Totem => {
                    self.push(Particle::totem(pos, velocity, &self.end_rod_frames));
                }
            }
        }
    }

    /// Spawn supported primary explosion options; unsupported options are
    /// rejected, never remapped.
    pub fn add_explosion_particle(
        &mut self,
        option: &ParticleOptions,
        pos: DVec3,
        velocity: DVec3,
    ) -> bool {
        if !supports_explosion_particle(option) {
            return false;
        }
        match option {
            ParticleOptions::Explosion => self.push(Particle::huge_explosion(
                pos,
                velocity,
                &self.explosion_frames,
            )),

            ParticleOptions::ExplosionEmitter => {
                self.pending_emitters.push(ExplosionEmitter { pos, age: 0 })
            }
            ParticleOptions::EndRod => {
                self.push(Particle::end_rod(pos, velocity, &self.end_rod_frames))
            }
            ParticleOptions::Poof => self.push(Particle::poof(pos, velocity, &self.generic_frames)),
            ParticleOptions::Smoke => {
                self.push(Particle::smoke(pos, velocity, &self.generic_frames))
            }
            _ => return false,
        }
        true
    }

    /// Preserve and queue the complete weighted packet list for the next client
    /// tick.
    pub fn track_explosion_effects(
        &mut self,
        center: DVec3,
        radius: f32,
        block_count: i32,
        block_particles: Vec<Weighted<ExplosionParticleInfo>>,
    ) {
        if !block_particles.is_empty() {
            self.tracked_explosions.push(TrackedExplosion {
                center,
                radius,
                block_count,
                block_particles,
            });
        }
    }

    /// Vanilla `ClientLevel.addDestroyBlockEffect`: a grid of terrain
    /// particles across each box of the block's shape (4x4x4 for a full
    /// cube).
    pub fn add_destroy_block_effect(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        if is_air(state) {
            return;
        }
        let block_id = block_id(state);
        // The only vanilla `noTerrainParticles` blocks.
        if matches!(block_id, "barrier" | "structure_void") {
            return;
        }

        // TerrainParticle base grey, multiplied by the block's biome tint.
        // grass_block is exempt (vanilla `colorAsTerrainParticle` returns
        // white for it — its particle texture is dirt).
        let mut color = [0.6f32; 3];
        let faces = registry.get_textures(state);
        if let Some(faces) = faces
            && faces.tint != Tint::None
            && block_id != "grass_block"
        {
            let tint = match faces.tint {
                Tint::Redstone => crate::world::block::redstone_wire_rgb(state),
                Tint::Stem => crate::world::block::stem_rgb(state),
                _ => self.blend_tint(faces.tint, pos, chunks, biome_climate),
            };
            for (c, t) in color.iter_mut().zip(tint) {
                *c *= t;
            }
        }

        let region = match faces {
            Some(faces) => self
                .uv_map
                .get_region(faces.particle.as_deref().unwrap_or(&faces.top)),
            // No face textures at all: the missing-texture region.
            None => self.uv_map.get_region(""),
        };
        let light = world_brightness(chunks, pos.x, pos.y, pos.z);

        // Vanilla `ParticleEngine.destroy` scatters over the outline shape's
        // boxes, so a block with an empty outline emits nothing.
        let boxes: &[LocalBox] = block_shape::outline_shape(state);

        for b in boxes {
            let width_x = (b[3] - b[0]).min(1.0);
            let width_y = (b[4] - b[1]).min(1.0);
            let width_z = (b[5] - b[2]).min(1.0);
            let count_x = ((width_x / 0.25).ceil() as i32).max(2);
            let count_y = ((width_y / 0.25).ceil() as i32).max(2);
            let count_z = ((width_z / 0.25).ceil() as i32).max(2);
            for xx in 0..count_x {
                for yy in 0..count_y {
                    for zz in 0..count_z {
                        let rel_x = (xx as f64 + 0.5) / count_x as f64;
                        let rel_y = (yy as f64 + 0.5) / count_y as f64;
                        let rel_z = (zz as f64 + 0.5) / count_z as f64;
                        let spawn = DVec3::new(
                            pos.x as f64 + rel_x * width_x + b[0],
                            pos.y as f64 + rel_y * width_y + b[1],
                            pos.z as f64 + rel_z * width_z + b[2],
                        );
                        self.push(Particle::terrain(
                            spawn,
                            DVec3::new(rel_x - 0.5, rel_y - 0.5, rel_z - 0.5),
                            region,
                            color,
                            light,
                        ));
                    }
                }
            }
        }
    }

    /// Vanilla `LivingEntity.spawnItemParticles`: item crumbs thrown from the
    /// mouth along the look direction while eating.
    pub fn add_item_use_particles(
        &mut self,
        count: u32,
        texture: &str,
        eye_pos: DVec3,
        x_rot_deg: f32,
        y_rot_deg: f32,
        chunks: &ChunkStore,
    ) {
        if !self.uv_map.has_region(texture) {
            return;
        }
        let region = self.uv_map.get_region(texture);
        let x_rot = -(x_rot_deg as f64).to_radians();
        let y_rot = -(y_rot_deg as f64).to_radians();
        let light = world_brightness(
            chunks,
            eye_pos.x.floor() as i32,
            eye_pos.y.floor() as i32,
            eye_pos.z.floor() as i32,
        );
        for _ in 0..count {
            let d = dvec3(
                (fastrand::f64() - 0.5) * 0.1,
                fastrand::f64() * 0.1 + 0.1,
                0.0,
            );
            let d = rot_y(rot_x(d, x_rot), y_rot);
            let p = dvec3(
                (fastrand::f64() - 0.5) * 0.3,
                -fastrand::f64() * 0.6 - 0.3,
                0.6,
            );
            let p = rot_y(rot_x(p, x_rot), y_rot) + eye_pos;
            self.push(Particle::breaking_item(
                p,
                dvec3(d.x, d.y + 0.05, d.z),
                region,
                light,
            ));
        }
    }

    /// Vanilla `ClientPacketListener.handleParticleEvent`: count 0 is a
    /// single directional particle (velocity = dist * max_speed), otherwise a
    /// gaussian scatter of `count` particles around the position.
    #[allow(clippy::too_many_arguments)]
    pub fn add_particles_from_packet(
        &mut self,
        kind: ServerParticleKind,
        options: ServerParticleOptions,
        override_limiter: bool,
        always_show: bool,
        pos: DVec3,
        dist: DVec3,
        max_speed: f64,
        count: i32,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        let Some(count) = packet_particle_count(count) else {
            return;
        };
        let bypass_distance_limit = override_limiter || always_show;
        if count == 0 {
            self.add_server_particle(
                kind,
                options,
                bypass_distance_limit,
                pos,
                dist * max_speed,
                camera_pos,
                registry,
                chunks,
                biome_climate,
            );
            return;
        }
        for _ in 0..count {
            let scatter = dvec3(
                next_gaussian() * dist.x,
                next_gaussian() * dist.y,
                next_gaussian() * dist.z,
            );
            let vel = dvec3(next_gaussian(), next_gaussian(), next_gaussian()) * max_speed;
            self.add_server_particle(
                kind,
                options.clone(),
                bypass_distance_limit,
                pos + scatter,
                vel,
                camera_pos,
                registry,
                chunks,
                biome_climate,
            );
        }
    }

    /// Vanilla `ClientLevel.doAddParticle`. Pomme reports
    /// `ParticleStatus::All` in client information, so the MINIMAL/DECREASED
    /// branches are unreachable and only the 32-block camera cull applies.
    fn add_server_particle(
        &mut self,
        kind: ServerParticleKind,
        options: ServerParticleOptions,
        override_limiter: bool,
        pos: DVec3,
        vel: DVec3,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        if !(override_limiter || kind.override_limiter())
            && camera_pos.distance_squared(pos) > 1024.0
        {
            return;
        }
        match kind {
            ServerParticleKind::EndRod => {
                self.push(Particle::end_rod(pos, vel, &self.end_rod_frames));
            }
            ServerParticleKind::ExplosionEmitter => {
                self.pending_emitters.push(ExplosionEmitter { pos, age: 0 });
            }
            ServerParticleKind::Explosion => {
                self.push(Particle::huge_explosion(pos, vel, &self.explosion_frames));
            }
            ServerParticleKind::Poof => {
                self.push(Particle::poof(pos, vel, &self.generic_frames));
            }
            ServerParticleKind::Smoke => {
                self.push(Particle::smoke(pos, vel, &self.generic_frames));
            }
            ServerParticleKind::Totem => {
                self.push(Particle::totem(pos, vel, &self.end_rod_frames));
            }
            ServerParticleKind::Block => {
                let ServerParticleOptions::Block(state) = options else {
                    return;
                };
                let state_id = block_id(state);
                if is_air(state)
                    || matches!(state_id, "barrier" | "structure_void" | "moving_piston")
                {
                    return;
                }
                let Some(faces) = registry.get_textures(state) else {
                    return;
                };
                let mut color = [0.6; 3];
                if faces.tint != Tint::None && state_id != "grass_block" {
                    let tint = match faces.tint {
                        Tint::Redstone => crate::world::block::redstone_wire_rgb(state),
                        Tint::Stem => crate::world::block::stem_rgb(state),
                        tint => self.blend_tint(
                            tint,
                            BlockPos::new(
                                pos.x.floor() as i32,
                                pos.y.floor() as i32,
                                pos.z.floor() as i32,
                            ),
                            chunks,
                            biome_climate,
                        ),
                    };
                    for (channel, tint) in color.iter_mut().zip(tint) {
                        *channel *= tint;
                    }
                }
                let block_pos = BlockPos::new(
                    pos.x.floor() as i32,
                    pos.y.floor() as i32,
                    pos.z.floor() as i32,
                );
                self.push(Particle::terrain(
                    pos,
                    vel,
                    self.uv_map
                        .get_region(faces.particle.as_deref().unwrap_or(&faces.top)),
                    color,
                    world_brightness(chunks, block_pos.x, block_pos.y, block_pos.z),
                ));
            }
            ServerParticleKind::Shriek => {
                let ServerParticleOptions::Shriek { delay } = options else {
                    return;
                };
                self.push(Particle::shriek(
                    pos,
                    delay,
                    self.uv_map.get_region(GENERIC_PARTICLE_SPRITES[9]),
                ));
            }
            ServerParticleKind::Trail => {
                let ServerParticleOptions::Trail {
                    target,
                    color,
                    duration,
                } = options
                else {
                    return;
                };
                self.push(Particle::trail(
                    pos,
                    vel,
                    target,
                    color,
                    duration,
                    self.uv_map.get_region(GENERIC_PARTICLE_SPRITES[10]),
                ));
            }
            ServerParticleKind::Vibration => {
                let sprite = self.uv_map.get_region(GENERIC_PARTICLE_SPRITES[11]);
                match options {
                    ServerParticleOptions::VibrationBlock {
                        target,
                        arrival_ticks,
                    } => {
                        self.push(Particle::vibration(
                            pos,
                            dvec3(
                                target.x as f64 + 0.5,
                                target.y as f64 + 0.5,
                                target.z as f64 + 0.5,
                            ),
                            arrival_ticks,
                            sprite,
                        ));
                    }
                    ServerParticleOptions::VibrationEntity {
                        entity_id,
                        y_offset,
                        arrival_ticks,
                    } => {
                        self.push(Particle::vibration_entity(
                            pos,
                            entity_id,
                            y_offset,
                            arrival_ticks,
                            sprite,
                        ));
                    }
                    _ => return,
                }
            }
            ServerParticleKind::Item => {
                let ServerParticleOptions::Item {
                    item_id,
                    count,
                    components,
                } = options
                else {
                    return;
                };
                let Some(kind) =
                    <azalea_registry::builtin::ItemKind as azalea_registry::Registry>::from_u32(
                        item_id,
                    )
                else {
                    return;
                };
                if count <= 0 {
                    return;
                }
                let stack = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData {
                    kind,
                    count,
                    component_patch: components,
                });
                let Some(texture) = registry.get_item_particle_icon(&stack) else {
                    return;
                };
                if !self.uv_map.has_region(texture) {
                    return;
                }
                self.push(Particle::breaking_item(
                    pos,
                    vel,
                    self.uv_map.get_region(texture),
                    world_brightness(
                        chunks,
                        pos.x.floor() as i32,
                        pos.y.floor() as i32,
                        pos.z.floor() as i32,
                    ),
                ));
            }
            ServerParticleKind::Dust => {
                if let ServerParticleOptions::Dust {
                    packed_color,
                    scale,
                } = options
                {
                    let packed = packed_color as u32;
                    let color = [
                        ((packed >> 16) & 0xff) as f32 / 255.0,
                        ((packed >> 8) & 0xff) as f32 / 255.0,
                        (packed & 0xff) as f32 / 255.0,
                    ];
                    let mut rng = fastrand::Rng::new();
                    self.push(Particle::dust(
                        pos,
                        vel,
                        color,
                        scale,
                        self.uv_map.get_region(DUST_SPRITE),
                        &mut rng,
                    ));
                }
            }
        }
    }

    /// The block's biome tint averaged over the vanilla 5x5 biome blend.
    fn blend_tint(
        &self,
        tint: Tint,
        pos: BlockPos,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) -> [f32; 3] {
        blend_color(pos.x, pos.z, |x, z| {
            let climate = match chunks.biome_id_checked(x, pos.y, z) {
                Some(biome_id) => biome_climate.get(&biome_id).copied().unwrap_or_default(),
                None => {
                    // Particle tint has no safe world sample here; use the
                    // neutral default without fabricating biome id 0.
                    BiomeClimate::default()
                }
            };
            match tint {
                Tint::Grass => grass_color(&climate, &self.grass_colormap, x, z),
                Tint::Foliage => foliage_color(&climate, &self.foliage_colormap),
                Tint::DryFoliage => dry_foliage_color(&climate, &self.dry_foliage_colormap),
                Tint::Fixed(rgb) => [
                    rgb[0] as f32 / 255.0,
                    rgb[1] as f32 / 255.0,
                    rgb[2] as f32 / 255.0,
                ],
                // Redstone is state-derived, resolved by the caller.
                Tint::None | Tint::Redstone | Tint::Stem => [1.0; 3],
            }
        })
    }

    pub(crate) fn clear(&mut self) {
        self.particles.clear();
        self.pending.clear();
        self.emitters.clear();
        self.pending_emitters.clear();
        self.tracked_explosions.clear();
        self.tracking_emitters.clear();
    }

    /// Vanilla `ParticleGroup` caps: hard limit plus probabilistic rejection
    /// once the reservoir fills.
    fn push(&mut self, particle: Particle) {
        let count = self.particles.len() + self.pending.len();
        if count >= MAX_PARTICLES {
            return;
        }
        if count >= RESERVOIR_START {
            let free = (MAX_PARTICLES - count) as f32 / 4096.0;
            if fastrand::f32() >= free * free {
                return;
            }
        }
        self.pending.push(particle);
    }

    pub fn tick(&mut self, chunks: &ChunkStore) {
        self.tick_with_entity_lookup(chunks, |_| None);
    }

    pub(crate) fn tick_with_entity_lookup(
        &mut self,
        chunks: &ChunkStore,
        mut lookup: impl FnMut(i32) -> Option<TrackingAttachment>,
    ) {
        let tracking: Vec<_> = self
            .tracking_emitters
            .iter_mut()
            .filter_map(|e| e.advance(&mut lookup))
            .collect();
        self.tracking_emitters.retain(|e| {
            e.age
                < match e.kind {
                    TrackingParticleKind::Crit | TrackingParticleKind::EnchantedHit => 3,
                    TrackingParticleKind::Totem => 30,
                }
        });
        for (kind, attachment) in tracking {
            self.emit_tracking_batch(kind, attachment, chunks);
        }
        self.emitters.append(&mut self.pending_emitters);
        let mut children = Vec::with_capacity(self.emitters.len() * 6);
        for emitter in &mut self.emitters {
            for _ in 0..explosion_emitter_children(emitter.age) {
                children.push(explosion_emitter_child(emitter.pos, emitter.age, || {
                    fastrand::f64() - fastrand::f64()
                }));
            }
            emitter.age += 1;
        }
        self.emitters.retain(|e| e.age < 8);
        for (pos, velocity) in children {
            let _ = self.add_explosion_particle(&ParticleOptions::Explosion, pos, velocity);
        }
        if !self.tracked_explosions.is_empty() {
            let mut rng = fastrand::Rng::new();
            let spawns = plan_explosion_particles(&self.tracked_explosions, &mut rng, |x, y, z| {
                is_air(chunks.get_block_state(x, y, z))
            });
            for spawn in spawns {
                if !self.add_explosion_particle(&spawn.particle, spawn.pos, spawn.velocity) {
                    tracing::debug!(particle = ?spawn.particle, "skipping unsupported explosion block particle option");
                }
            }
            self.tracked_explosions.clear();
        }
        let end_frames = self.end_rod_frames;
        let generic_frames = self.generic_frames;
        let explosion_frames = self.explosion_frames;
        self.particles.retain_mut(|p| {
            p.tick_with_entity_lookup(
                chunks,
                &end_frames,
                &generic_frames,
                &explosion_frames,
                &mut lookup,
            )
        });
        self.particles.append(&mut self.pending);
    }

    /// Quad positions are anchor-relative, subtracted in f64 (see
    /// `Camera::anchor`).
    pub fn extract(&self, partial_tick: f32, anchor: DVec3) -> Vec<ParticleQuad> {
        self.particles
            .iter()
            .flat_map(|p| {
                if p.kind == Kind::Shriek && p.delay > 0 {
                    return Vec::new();
                }
                let pos = (p.prev_pos.lerp(p.pos, partial_tick as f64) - anchor).as_vec3();
                let channel = |c: f32| (c * p.light * 255.0).round() as u8;
                let t = (p.age as f32 + partial_tick) / p.lifetime as f32;
                let size = match p.kind {
                    Kind::Dust => dust_quad_size(p.base_size, p.age, p.lifetime, partial_tick),
                    Kind::Shriek => p.size * (t * 0.75).clamp(0.0, 1.0),
                    _ => p.size,
                };
                let alpha = if p.kind == Kind::Shriek {
                    1.0 - t.clamp(0.0, 1.0)
                } else {
                    p.alpha
                };
                let sway =
                    ((p.age as f32 + partial_tick - std::f32::consts::TAU) * 0.05).sin() * 2.0;
                let rot = p.rot_o + (p.rot - p.rot_o) * partial_tick;
                let pitch = p.pitch_o + (p.pitch - p.pitch_o) * partial_tick;
                let primary = if p.kind == Kind::Vibration {
                    Quat::from_rotation_y(rot)
                        * Quat::from_rotation_x(-pitch - std::f32::consts::FRAC_PI_2)
                        * Quat::from_rotation_y(sway)
                } else {
                    p.rotation
                };
                let quad = |rotation: Quat| ParticleQuad {
                    pos: pos.into(),
                    size,
                    u0: p.u0,
                    u1: p.u1,
                    v0: p.v0,
                    v1: p.v1,
                    color: u32::from_le_bytes([
                        channel(p.color[0]),
                        channel(p.color[1]),
                        channel(p.color[2]),
                        (alpha * 255.0).round() as u8,
                    ]),
                    translucent: p.kind.translucent(),
                    rotation: rotation.to_array(),
                };
                let second = p.second_rotation.or_else(|| {
                    (p.kind == Kind::Vibration).then(|| {
                        Quat::from_rotation_y(-std::f32::consts::PI + rot)
                            * Quat::from_rotation_x(pitch + std::f32::consts::FRAC_PI_2)
                            * Quat::from_rotation_y(sway)
                    })
                });
                let mut quads = vec![quad(primary)];
                if let Some(rotation) = second {
                    quads.push(quad(rotation));
                }
                quads
            })
            .collect()
    }
}

fn target_step(current: DVec3, target: DVec3, ticks_remaining: i32) -> DVec3 {
    let factor = if ticks_remaining <= 1 {
        1.0
    } else {
        1.0 / ticks_remaining as f64
    };
    current.lerp(target, factor)
}

/// `java.util.Random.nextGaussian` (Marsaglia polar method), minus the
/// second-sample cache.
pub(crate) fn packet_particle_count(count: i32) -> Option<u32> {
    u32::try_from(count).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        AtlasUVMap, ExplosionParticleInfo, Particle, ParticleOptions, ServerParticleKind,
        TrackedExplosion, Weighted, animated_frame_index, dust_quad_size, dvec3,
        explosion_emitter_child, explosion_frame_index, packet_particle_count,
        plan_explosion_particles, supports_explosion_particle,
    };
    use crate::world::chunk::ChunkStore;

    #[test]
    fn block_particle_id_is_typed_and_unhandled_payload_kinds_stay_unknown() {
        assert!(matches!(
            ServerParticleKind::from_id(1),
            Some(ServerParticleKind::Block)
        ));
        assert!(ServerParticleKind::from_id(43).is_none()); // item requires its own codec
        assert!(ServerParticleKind::from_id(44).is_none()); // vibration requires target data
        assert!(matches!(
            ServerParticleKind::from_id(54),
            Some(ServerParticleKind::Item)
        ));
        assert!(matches!(
            ServerParticleKind::from_id(55),
            Some(ServerParticleKind::Vibration)
        ));
        assert!(matches!(
            ServerParticleKind::from_id(56),
            Some(ServerParticleKind::Trail)
        ));
        assert!(matches!(
            ServerParticleKind::from_id(112),
            Some(ServerParticleKind::Shriek)
        ));
    }

    #[test]
    fn special_target_step_reaches_destination_on_final_tick() {
        assert_eq!(
            super::target_step(dvec3(0.0, 0.0, 0.0), dvec3(10.0, 0.0, 0.0), 2),
            dvec3(5.0, 0.0, 0.0)
        );
        assert_eq!(
            super::target_step(dvec3(5.0, 0.0, 0.0), dvec3(10.0, 0.0, 0.0), 1),
            dvec3(10.0, 0.0, 0.0)
        );
    }

    #[test]
    fn block_vibration_starts_facing_its_source_without_first_frame_spin() {
        let sprite = AtlasUVMap::test_empty().missing_region();
        let particle = Particle::vibration(dvec3(0.0, 0.0, 0.0), dvec3(4.0, 3.0, 0.0), 4, sprite);
        assert_eq!(particle.rot, particle.rot_o);
        assert_eq!(particle.pitch, particle.pitch_o);
        assert!((particle.rot + std::f32::consts::FRAC_PI_2).abs() < 1.0e-6);
    }

    #[test]
    fn entity_vibration_resolves_moving_target_and_expires_when_missing() {
        let sprite = AtlasUVMap::test_empty().missing_region();
        let frames = [sprite; 8];
        let mut particle = Particle::vibration_entity(dvec3(0.0, 0.0, 0.0), 17, 1.5, 4, sprite);
        let chunks = ChunkStore::new(2);
        assert!(particle.tick_with_entity_lookup(
            &chunks,
            &frames,
            &frames,
            &[sprite; 16],
            &mut |id| {
                assert_eq!(id, 17);
                Some(super::TrackingAttachment {
                    position: dvec3(9.0, 18.0, 27.0),
                    width: 1.0,
                    height: 2.0,
                })
            }
        ));
        assert_eq!(particle.target, Some(dvec3(9.0, 19.5, 27.0)));
        assert_eq!(particle.pos, dvec3(3.0, 6.5, 9.0));

        assert!(particle.tick_with_entity_lookup(
            &chunks,
            &frames,
            &frames,
            &[sprite; 16],
            &mut |_| {
                Some(super::TrackingAttachment {
                    position: dvec3(21.0, 42.0, 63.0),
                    width: 1.0,
                    height: 2.0,
                })
            }
        ));
        assert_eq!(particle.target, Some(dvec3(21.0, 43.5, 63.0)));
        assert_eq!(particle.pos, dvec3(12.0, 25.0, 36.0));
        assert!(!particle.tick_with_entity_lookup(
            &chunks,
            &frames,
            &frames,
            &[sprite; 16],
            &mut |_| None
        ));
    }

    #[test]
    fn item_breaking_particle_matches_vanilla_terrain_base() {
        let sprite = AtlasUVMap::test_empty().missing_region();
        let particle =
            Particle::breaking_item(dvec3(1.0, 2.0, 3.0), dvec3(0.2, 0.3, 0.4), sprite, 0.5);
        assert!(matches!(
            particle.kind,
            super::Kind::Item | super::Kind::ItemTranslucent
        ));
        assert_eq!(particle.gravity, 1.0);
        assert_eq!(particle.friction, 0.98);
        assert!((4..=40).contains(&particle.lifetime));
        assert!((0.05..=0.1).contains(&particle.size));
        assert!(particle.vel.distance(dvec3(0.2, 0.3, 0.4)) < 0.05);
        assert_eq!(particle.color, [1.0; 3]);
        assert_eq!(particle.light, 0.5);
    }

    #[test]
    fn specialized_particles_keep_options_and_vanilla_lifetimes() {
        let sprite = AtlasUVMap::test_empty().missing_region();
        let shriek = Particle::shriek(dvec3(1.0, 2.0, 3.0), 7, sprite);
        assert_eq!((shriek.delay, shriek.lifetime), (7, 30));
        assert_eq!(shriek.vel.y, 0.1);
        assert!(shriek.second_rotation.is_some());

        let trail = Particle::trail(
            dvec3(0.0, 0.0, 0.0),
            dvec3(1.0, 2.0, 3.0),
            dvec3(9.0, 0.0, 0.0),
            0x804020,
            12,
            sprite,
        );
        assert!(matches!(&trail.kind, super::Kind::Trail));
        assert_eq!(
            (trail.lifetime, trail.target),
            (12, Some(dvec3(9.0, 0.0, 0.0)))
        );
        assert_eq!(trail.light, 1.0);
        assert!(trail.color.iter().all(|c| (0.0..=1.0).contains(c)));

        let vibration = Particle::vibration(dvec3(0.0, 0.0, 0.0), dvec3(4.5, 5.5, 6.5), 20, sprite);
        assert!(matches!(&vibration.kind, super::Kind::Vibration));
        assert_eq!(vibration.lifetime, 20);
        assert_eq!(vibration.target, Some(dvec3(4.5, 5.5, 6.5)));
        assert_eq!(vibration.light, 1.0);
    }

    #[test]
    fn dust_constructor_matches_vanilla_scaling_and_seeded_randomness() {
        let sprite = AtlasUVMap::test_empty().missing_region();
        let mut first_rng = fastrand::Rng::with_seed(262);
        let mut second_rng = fastrand::Rng::with_seed(262);
        let mut expected_rng = fastrand::Rng::with_seed(262);
        let expected_lifetime = (8.0 / (expected_rng.f64() * 0.8 + 0.2)) as i32;
        let base_factor = expected_rng.f32() * 0.4 + 0.6;
        let expected_color = [
            (expected_rng.f32() * 0.2 + 0.8) * base_factor,
            (expected_rng.f32() * 0.2 + 0.8) * base_factor,
            (expected_rng.f32() * 0.2 + 0.8) * base_factor,
        ];
        let first = Particle::dust(
            dvec3(1.0, 2.0, 3.0),
            dvec3(2.0, -4.0, 6.0),
            [1.0; 3],
            1.0,
            sprite,
            &mut first_rng,
        );
        let second = Particle::dust(
            dvec3(1.0, 2.0, 3.0),
            dvec3(2.0, -4.0, 6.0),
            [1.0; 3],
            1.0,
            sprite,
            &mut second_rng,
        );
        assert_eq!(first.vel, dvec3(2.0, -4.0, 6.0) * 0.1);
        assert_eq!(first.base_size, 0.075);
        assert_eq!(first.lifetime, expected_lifetime);
        assert_eq!(first.lifetime, second.lifetime);
        assert_eq!(first.color, expected_color);
        assert_eq!(first.color, second.color);
    }

    #[test]
    fn dust_size_grows_during_first_thirty_second_of_lifetime() {
        assert_eq!(dust_quad_size(2.0, 0, 40, 0.0), 0.0);
        assert_eq!(dust_quad_size(2.0, 1, 40, 0.0), 1.6);
        assert_eq!(dust_quad_size(2.0, 40, 40, 0.0), 2.0);
    }

    fn explosion_fixture(block_count: i32, weight: i32) -> TrackedExplosion {
        TrackedExplosion {
            center: dvec3(0.5, 0.5, 0.5),
            radius: 3.0,
            block_count,
            block_particles: vec![Weighted {
                value: ExplosionParticleInfo {
                    particle: ParticleOptions::Explosion,
                    scaling: 1.0,
                    speed: 1.0,
                },
                weight,
            }],
        }
    }

    #[test]
    fn tracking_samples_are_bounded_and_use_entity_dimensions() {
        let attachment = super::TrackingAttachment {
            position: dvec3(10.0, 20.0, 30.0),
            width: 2.0,
            height: 4.0,
        };
        let samples = [[0.5, -0.25, 0.5], [1.0, 1.0, 1.0]];
        let out = super::tracking_sample(attachment, samples);
        assert_eq!(out.len(), 1);
        // Independent contract equations: pos + (width*x/4,
        // height*(0.5+y/4), width*z/4), aux = (x, y+0.2, z).
        assert_eq!(out[0].0, dvec3(10.25, 21.75, 30.25));
        assert!((out[0].1 - dvec3(0.5, -0.05, 0.5)).length() < 1.0e-12);
        let mut draws = 0;
        let _ = super::tracking_sample(
            attachment,
            std::iter::repeat_with(|| {
                draws += 1;
                [0.0; 3]
            }),
        );
        assert_eq!(draws, 16);
    }

    #[test]
    fn tracking_store_ticks_three_batches_and_detach_preserves_old_id_snapshot() {
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;
        use crate::world::chunk::ChunkStore;

        crate::world::block::init("26.2");
        let uv = AtlasUVMap::test_empty();
        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        let chunks = ChunkStore::new(2);
        let start = super::TrackingAttachment {
            position: dvec3(1.0, 2.0, 3.0),
            width: 0.6,
            height: 1.8,
        };
        let moved = super::TrackingAttachment {
            position: dvec3(4.0, 5.0, 6.0),
            width: 0.6,
            height: 1.8,
        };
        store.add_tracking_emitter(7, super::TrackingParticleKind::Crit, start, &chunks);
        assert!((1..=16).contains(&store.pending.len()));
        assert_eq!(store.tracking_emitters.len(), 1);

        let mut looked_up = Vec::new();
        store.tick_with_entity_lookup(&chunks, |id| {
            looked_up.push(id);
            (id == 7).then_some(moved)
        });
        assert_eq!(looked_up, [7]);
        assert_eq!(store.tracking_emitters[0].attachment, moved);
        let after_second_batch = store.particles.len();
        assert!(after_second_batch > 0);

        // Despawn/ID replacement snapshots the old entity before the numeric ID
        // can resolve to a different entity on the next simulation tick.
        store.detach_tracking_emitter(7, Some(moved));
        assert_eq!(store.tracking_emitters[0].entity_id, None);
        let respawned = super::TrackingAttachment {
            position: dvec3(20.0, 30.0, 40.0),
            width: 1.0,
            height: 2.0,
        };
        store.add_tracking_emitter(
            7,
            super::TrackingParticleKind::EnchantedHit,
            respawned,
            &chunks,
        );
        assert_eq!(store.tracking_emitters[0].attachment, moved);
        assert_eq!(store.tracking_emitters[1].attachment, respawned);
        let mut looked_up_again = Vec::new();
        store.tick_with_entity_lookup(&chunks, |id| {
            looked_up_again.push(id);
            (id == 7).then_some(respawned)
        });
        assert_eq!(looked_up_again, [7]); // only the new emitter resolves the reused ID.
        assert!(
            store
                .tracking_emitters
                .iter()
                .all(|e| e.entity_id == Some(7))
        );
        assert!(
            store
                .tracking_emitters
                .iter()
                .any(|e| e.attachment == respawned)
        );
        assert!(store.particles.len() > after_second_batch);
        assert!(store.particles.iter().any(|p| p.age == 2));
        assert!(store.particles.iter().any(|p| p.age == 1));

        // A clearing transition must also drop any remaining tracking state.
        store.add_tracking_emitter(8, super::TrackingParticleKind::EnchantedHit, start, &chunks);
        store.clear();
        assert!(store.tracking_emitters.is_empty());
        assert!(store.particles.is_empty());
        assert!(store.pending.is_empty());
    }

    #[test]
    fn tracking_crit_and_enchanted_emitters_keep_three_total_batches() {
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;
        use crate::world::chunk::ChunkStore;
        crate::world::block::init("26.2");
        let uv = AtlasUVMap::test_empty();
        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        let chunks = ChunkStore::new(2);
        let attachment = super::TrackingAttachment {
            position: dvec3(1.0, 2.0, 3.0),
            width: 0.6,
            height: 1.8,
        };
        for kind in [
            super::TrackingParticleKind::Crit,
            super::TrackingParticleKind::EnchantedHit,
        ] {
            store.add_tracking_emitter(4, kind, attachment, &chunks);
            assert_eq!(store.tracking_emitters.last().unwrap().age, 1);
            store.tick_with_entity_lookup(&chunks, |_| Some(attachment));
            assert_eq!(store.tracking_emitters.last().unwrap().age, 2);
            store.tick_with_entity_lookup(&chunks, |_| Some(attachment));
            assert!(store.tracking_emitters.is_empty());
        }
    }

    #[test]
    fn totem_tracking_emits_immediately_then_30_total_batches_and_removes() {
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;
        use crate::world::chunk::ChunkStore;
        crate::world::block::init("26.2");
        let uv = AtlasUVMap::test_empty();
        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        let chunks = ChunkStore::new(2);
        let attachment = super::TrackingAttachment {
            position: dvec3(1.0, 2.0, 3.0),
            width: 0.6,
            height: 1.8,
        };
        fastrand::seed(0x544f_5445_4d);
        store.add_tracking_emitter(4, super::TrackingParticleKind::Totem, attachment, &chunks);
        assert_eq!(store.tracking_emitters[0].age, 1);
        assert!((1..=16).contains(&store.pending.len()));
        let mut batches = 1;
        for _ in 0..29 {
            store.tick_with_entity_lookup(&chunks, |_| Some(attachment));
            batches += 1;
        }
        assert_eq!(batches, 30);
        assert!(store.tracking_emitters.is_empty());
        let totem_count = store
            .particles
            .iter()
            .filter(|p| p.kind == super::Kind::Totem)
            .count()
            + store
                .pending
                .iter()
                .filter(|p| p.kind == super::Kind::Totem)
                .count();
        let particles_before = store.particles.len() + store.pending.len();
        store.tick_with_entity_lookup(&chunks, |_| Some(attachment));
        assert_eq!(store.tracking_emitters.len(), 0);
        assert_eq!(
            store
                .particles
                .iter()
                .filter(|p| p.kind == super::Kind::Totem)
                .count()
                + store
                    .pending
                    .iter()
                    .filter(|p| p.kind == super::Kind::Totem)
                    .count(),
            totem_count
        );
        assert!(store.particles.len() + store.pending.len() <= particles_before);
    }

    #[test]
    fn totem_move_clips_against_loaded_floor_and_wall() {
        use crate::renderer::chunk::atlas::AtlasRegion;
        use crate::world::chunk::ChunkStore;
        crate::world::block::init("26.2");
        let frame = AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 1.0,
            v_max: 1.0,
            pixel_rect: [0; 4],
            sprite: 0,
            opaque: false,
            translucent: true,
            alpha_counts: [0; 3],
        };
        let frames = [frame; 8];
        let mut chunks = ChunkStore::new(1);
        let _loaded = chunks.chunk_storage.upsert(
            azalea_core::position::ChunkPos::new(0, 0),
            azalea_world::chunk::Chunk::default(),
        );
        let stone = crate::world::block::first_state_of("stone").unwrap();
        chunks.set_block_state(0, 60, 0, stone);
        chunks.set_block_state(1, 61, 0, stone);

        let mut floor =
            super::Particle::totem(dvec3(0.5, 61.2, 0.5), dvec3(0.2, -0.5, 0.0), &frames);
        assert!(floor.tick(&chunks, &frames, &frames, &[frame; 16]));
        assert_eq!(floor.pos, dvec3(0.7, 61.0, 0.5));
        assert!(floor.on_ground);
        assert!(!floor.stopped_by_collision);
        let friction = f64::from(0.6f32);
        let gravity = f64::from(1.25f32);
        let expected_floor_velocity = dvec3(
            0.2 * friction * 0.7,
            (-0.5 - 0.04 * gravity) * friction,
            0.0,
        );
        assert!((floor.vel - expected_floor_velocity).length() < 1e-6);

        let mut wall = super::Particle::totem(dvec3(0.8, 62.0, 0.5), dvec3(0.5, 0.0, 0.0), &frames);
        assert!(wall.tick(&chunks, &frames, &frames, &[frame; 16]));
        assert_eq!(wall.pos, dvec3(0.9, 61.95, 0.5));
        assert_eq!(wall.vel.x, 0.0);
        assert!((wall.vel.y - (-0.04 * gravity) * friction).abs() < 1e-6);
        assert!(!wall.on_ground);
    }

    #[test]
    fn totem_constructor_physics_render_and_sprite_tick_contract() {
        use crate::renderer::chunk::atlas::AtlasRegion;
        use crate::world::chunk::ChunkStore;
        crate::world::block::init("26.2");
        let frames = std::array::from_fn(|i| AtlasRegion {
            u_min: i as f32 / 8.0,
            v_min: 0.25,
            u_max: (i + 1) as f32 / 8.0,
            v_max: 0.5,
            pixel_rect: [0; 4],
            sprite: i as u16,
            opaque: false,
            translucent: true,
            alpha_counts: [0; 3],
        });
        let pos = dvec3(2.0, 3.0, 4.0);
        let velocity = dvec3(0.2, 0.4, -0.3);
        let mut p = super::Particle::totem(pos, velocity, &frames);
        assert_eq!(p.age, 0); // Totem constructor does not tick.
        assert!((60..=71).contains(&p.lifetime));
        assert!((0.075..=0.15).contains(&p.size));
        assert_eq!(p.vel, velocity);
        assert_eq!(
            (p.gravity, p.friction, p.alpha, p.light),
            (f64::from(1.25f32), f64::from(0.6f32), 1.0, 1.0)
        );
        assert_eq!(
            (p.u0, p.u1, p.v0, p.v1),
            (
                frames[0].u_min,
                frames[0].u_max,
                frames[0].v_min,
                frames[0].v_max
            )
        );
        assert!(p.kind.translucent());
        let original = p.pos;
        let chunks = ChunkStore::new(2);
        assert!(p.tick(&chunks, &frames, &frames, &[frames[0]; 16]));
        assert_eq!(p.age, 1);
        assert_eq!(
            p.pos,
            original + dvec3(velocity.x, velocity.y - 0.04 * 1.25, velocity.z)
        );
        let friction = f64::from(0.6f32);
        let gravity = f64::from(1.25f32);
        let expected_velocity =
            dvec3(velocity.x, velocity.y - 0.04 * gravity, velocity.z) * friction;
        assert!((p.vel - expected_velocity).length() < 1e-6);
        assert_eq!(p.light, 1.0);
        assert!(p.alpha <= 1.0 && p.alpha >= 0.0);
        assert!(p.color[0] >= 0.1 && p.color[0] < 0.8);
        assert!(p.color[1] >= 0.4 && p.color[1] < 0.9);
        assert!((0.0..0.2).contains(&p.color[2])); // Vanilla samples blue independently in both branches.
        assert_eq!(
            (p.u0, p.u1),
            (
                frames[(p.age * 7 / p.lifetime) as usize].u_min,
                frames[(p.age * 7 / p.lifetime) as usize].u_max
            )
        );
        p.age = p.lifetime / 2;
        p.alpha = 1.0;
        assert!(p.tick(&chunks, &frames, &frames, &[frames[0]; 16]));
        assert_eq!(p.alpha, 1.0 - 1.0 / p.lifetime as f32);
        p.age = p.lifetime - 1;
        assert!(p.tick(&chunks, &frames, &frames, &[frames[0]; 16]));
        assert_eq!(p.age, p.lifetime);
        assert_eq!((p.u0, p.u1), (frames[7].u_min, frames[7].u_max));
        assert!(!p.tick(&chunks, &frames, &frames, &[frames[0]; 16]));
    }

    #[test]
    fn totem_color_matches_both_vanilla_branches_including_blue() {
        let rare = super::totem_color(true, [0.5; 3]);
        let common = super::totem_color(false, [0.5; 3]);
        assert!((rare[0] - 0.7).abs() < f32::EPSILON);
        assert!((rare[1] - 0.75).abs() < f32::EPSILON);
        assert_eq!(rare[2], 0.1); // Independent expected blue = sample 0.5 * 0.2.
        assert!((common[0] - 0.2).abs() < f32::EPSILON);
        assert!((common[1] - 0.55).abs() < f32::EPSILON);
        assert_eq!(common[2], 0.1);
    }

    #[test]
    fn totem_tracking_detach_same_id_reuse_and_clear_use_store() {
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;
        use crate::world::chunk::ChunkStore;
        crate::world::block::init("26.2");
        fastrand::seed(0x544f_5445_4d);
        let uv = AtlasUVMap::test_empty();
        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        let chunks = ChunkStore::new(2);
        let old = super::TrackingAttachment {
            position: dvec3(1.0, 2.0, 3.0),
            width: 0.6,
            height: 1.8,
        };
        let reused = super::TrackingAttachment {
            position: dvec3(20.0, 30.0, 40.0),
            width: 1.0,
            height: 2.0,
        };
        store.add_tracking_emitter(9, super::TrackingParticleKind::Totem, old, &chunks);
        store.detach_tracking_emitter(9, Some(old));
        store.add_tracking_emitter(9, super::TrackingParticleKind::Totem, reused, &chunks);
        let mut lookups = 0;
        for _ in 0..28 {
            store.tick_with_entity_lookup(&chunks, |_| {
                lookups += 1;
                Some(reused)
            });
            assert_eq!(store.tracking_emitters[0].attachment, old);
        }
        assert_eq!(lookups, 28); // only the reused ID's new emitter is looked up.
        assert_eq!(store.tracking_emitters.len(), 2);
        assert_eq!(store.tracking_emitters[0].attachment, old);
        assert_eq!(store.tracking_emitters[1].attachment, reused);
        store.tick_with_entity_lookup(&chunks, |_| {
            lookups += 1;
            Some(reused)
        });
        assert_eq!(lookups, 29);
        assert!(store.tracking_emitters.is_empty());
        store.add_tracking_emitter(10, super::TrackingParticleKind::Totem, old, &chunks);
        store.clear();
        assert!(store.tracking_emitters.is_empty());
        assert!(store.particles.is_empty());
        assert!(store.pending.is_empty());
    }

    #[test]
    fn tracking_particle_ctor_and_sprite_contract() {
        assert_eq!(super::CRIT_SPRITE, "particle/critical_hit");
        assert_eq!(super::ENCHANTED_HIT_SPRITE, "particle/enchanted_hit");
        assert_eq!(
            super::END_ROD_SPRITES,
            [
                "particle/glitter_7",
                "particle/glitter_6",
                "particle/glitter_5",
                "particle/glitter_4",
                "particle/glitter_3",
                "particle/glitter_2",
                "particle/glitter_1",
                "particle/glitter_0"
            ]
        );
        let frame = super::AtlasRegion {
            u_min: 0.1,
            v_min: 0.2,
            u_max: 0.3,
            v_max: 0.4,
            pixel_rect: [0; 4],
            sprite: 0,
            opaque: true,
            translucent: false,
            alpha_counts: [0; 3],
        };
        crate::world::block::init("26.2");
        let chunks = crate::world::chunk::ChunkStore::new(2);
        let p = super::Particle::crit(
            dvec3(0.5, 10.0, 0.5),
            dvec3(0.5, 0.25, -0.5),
            true,
            frame,
            &chunks,
        );
        assert_eq!(p.age, 1); // Crit ctor ticks synchronously, unlike its TrackingEmitter.
        assert!((p.pos - dvec3(0.7, 10.08, 0.3)).length() < 1.0e-12);
        assert!((p.vel - dvec3(0.14, 0.056, -0.14)).length() < 1.0e-12);
        assert!(!p.kind.translucent());
        assert!((4..=10).contains(&p.lifetime));
        assert_eq!((p.gravity, p.friction), (0.5, 0.7));
        assert!((0.075..=0.15).contains(&p.size));
        assert!((0.18..0.27).contains(&p.color[0]));
        assert!((0.46..0.72).contains(&p.color[1]));
        assert!((0.48..0.81).contains(&p.color[2]));
        assert_eq!(
            (p.u0, p.u1, p.v0, p.v1),
            (frame.u_min, frame.u_max, frame.v_min, frame.v_max)
        );
        assert_eq!(
            p.light,
            super::world_brightness(
                &chunks,
                p.pos.x.floor() as i32,
                p.pos.y.floor() as i32,
                p.pos.z.floor() as i32
            )
        );
    }

    #[test]
    fn particle_lifetime_keeps_age_life_frame_then_removes_next_tick() {
        use crate::world::chunk::ChunkStore;

        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(2);
        let frame = super::AtlasRegion {
            u_min: 0.1,
            v_min: 0.2,
            u_max: 0.3,
            v_max: 0.4,
            pixel_rect: [0; 4],
            sprite: 0,
            opaque: true,
            translucent: false,
            alpha_counts: [0; 3],
        };
        let mut particle = super::Particle::crit(
            dvec3(0.5, 10.0, 0.5),
            dvec3(0.0, 0.0, 0.0),
            false,
            frame,
            &chunks,
        );
        particle.age = particle.lifetime - 1;
        assert!(particle.tick(&chunks, &[frame; 8], &[frame; 8], &[frame; 16]));
        assert_eq!(particle.age, particle.lifetime);
        assert!(!particle.tick(&chunks, &[frame; 8], &[frame; 8], &[frame; 16]));
    }

    #[test]
    fn explosion_frames_and_final_age_match_spriteset_selection() {
        assert_eq!(super::EXPLOSION_SPRITES.len(), 16);
        assert_eq!(super::EXPLOSION_SPRITES[0], "particle/explosion_0");
        assert_eq!(super::EXPLOSION_SPRITES[15], "particle/explosion_15");
        assert_eq!(explosion_frame_index(0, 20), 0);
        assert_eq!(explosion_frame_index(19, 20), 14);
        assert_eq!(explosion_frame_index(20, 20), 15);
        assert_eq!(animated_frame_index(19, 20, 8), 6);
        assert_eq!(animated_frame_index(20, 20, 8), 7);
    }

    #[test]
    fn emitter_has_eight_ticks_and_emits_six_each_tick() {
        let mut age = 0;
        let mut children = 0;
        while age < super::EXPLOSION_EMITTER_TICKS {
            children += usize::from(super::explosion_emitter_children(age));
            age += 1;
        }
        assert_eq!((age, children), (8, 48));
        assert_eq!(super::explosion_emitter_children(8), 0);
        assert_eq!(
            [0, 4, 7].map(super::explosion_emitter_size),
            [0.0, 0.5, 0.875],
        );
        let mut draws = [0.25, -0.125, 0.5].into_iter();
        let child = explosion_emitter_child(dvec3(10.0, 20.0, 30.0), 3, || draws.next().unwrap());
        assert_eq!(child, (dvec3(11.0, 19.5, 32.0), dvec3(0.375, 0.0, 0.0)));
    }

    #[test]
    fn huge_explosion_uses_only_x_aux_as_size_and_never_moves() {
        let frame = super::AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 1.0,
            v_max: 1.0,
            pixel_rect: [0; 4],
            sprite: 0,
            opaque: true,
            translucent: false,
            alpha_counts: [0, 0, 1],
        };
        let frames = [frame; 16];
        let pos = dvec3(3.0, 4.0, 5.0);
        let mut huge = super::Particle::huge_explosion(pos, dvec3(0.5, 100.0, -200.0), &frames);
        assert_eq!(huge.size, 1.5);
        assert_eq!(huge.vel, dvec3(0.0, 0.0, 0.0));
        assert!((6..=9).contains(&huge.lifetime));
        assert_eq!(huge.light, 1.0);
        let chunks = crate::world::chunk::ChunkStore::new(2);
        for _ in 0..huge.lifetime {
            assert!(huge.tick(&chunks, &[frame; 8], &[frame; 8], &frames));
        }
        assert_eq!(huge.pos, pos);

        let primary = super::Particle::huge_explosion(pos, dvec3(1.0, 0.0, 0.0), &frames);
        assert_eq!(primary.size, 1.0);
        assert!(supports_explosion_particle(&ParticleOptions::Explosion));
        assert!(supports_explosion_particle(&ParticleOptions::Poof));
        assert!(supports_explosion_particle(&ParticleOptions::Smoke));
    }

    #[test]
    fn poof_starts_not_fullbright_and_refreshes_world_light_on_tick() {
        let frame = super::AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 1.0,
            v_max: 1.0,
            pixel_rect: [0; 4],
            sprite: 0,
            opaque: true,
            translucent: false,
            alpha_counts: [0; 3],
        };
        let frames = [frame; 8];
        crate::world::block::init("26.2");
        let mut poof = super::Particle::poof(dvec3(3.0, 4.0, 5.0), dvec3(0.0, 0.0, 0.0), &frames);
        assert_eq!(poof.light, 0.0);
        let chunks = crate::world::chunk::ChunkStore::new(2);
        poof.tick(&chunks, &frames, &frames, &[frame; 16]);
        assert_eq!(poof.light, super::world_brightness(&chunks, 3, 4, 5),);
        assert_ne!(poof.light, 0.0);
    }

    #[test]
    fn poof_and_smoke_advance_frames_and_smoke_grows_during_tick() {
        let generic_frames = std::array::from_fn(|i| super::AtlasRegion {
            u_min: i as f32 / 8.0,
            u_max: (i + 1) as f32 / 8.0,
            v_min: 0.0,
            v_max: 1.0,
            pixel_rect: [0; 4],
            sprite: i as u16,
            opaque: true,
            translucent: false,
            alpha_counts: [0; 3],
        });
        crate::world::block::init("26.2");
        let chunks = crate::world::chunk::ChunkStore::new(2);
        let pos = dvec3(3.0, 4.0, 5.0);
        let mut poof = super::Particle::poof(pos, dvec3(0.0, 0.0, 0.0), &generic_frames);
        let mut smoke = super::Particle::smoke(pos, dvec3(0.0, 0.0, 0.0), &generic_frames);
        poof.lifetime = 16;
        smoke.lifetime = 16;
        poof.set_sprite(&generic_frames[0]);
        smoke.set_sprite(&generic_frames[0]);
        assert_eq!(smoke.size, 0.0);

        for _ in 0..4 {
            assert!(poof.tick(
                &chunks,
                &[generic_frames[0]; 8],
                &generic_frames,
                &[generic_frames[0]; 16],
            ));
            assert!(smoke.tick(
                &chunks,
                &[generic_frames[0]; 8],
                &generic_frames,
                &[generic_frames[0]; 16],
            ));
        }

        assert_eq!(poof.u0, generic_frames[1].u_min);
        assert_eq!(smoke.u0, generic_frames[1].u_min);
        assert!(smoke.size > 0.0);
    }

    #[test]
    fn standard_tnt_weighted_options_are_renderable_and_not_skipped() {
        let mut weighted = explosion_fixture(1, 1);
        weighted.block_particles[0].value = ExplosionParticleInfo {
            particle: ParticleOptions::Poof,
            scaling: 0.5,
            speed: 1.0,
        };
        weighted.block_particles.push(Weighted {
            value: ExplosionParticleInfo {
                particle: ParticleOptions::Smoke,
                scaling: 1.0,
                speed: 1.0,
            },
            weight: 1,
        });
        let mut rng = fastrand::Rng::with_seed(0x544e_54);
        let mut found_poof = false;
        let mut found_smoke = false;
        for _ in 0..32 {
            for spawn in
                plan_explosion_particles(std::slice::from_ref(&weighted), &mut rng, |_, _, _| true)
            {
                assert!(supports_explosion_particle(&spawn.particle));
                found_poof |= matches!(spawn.particle, ParticleOptions::Poof);
                found_smoke |= matches!(spawn.particle, ParticleOptions::Smoke);
            }
        }
        assert!(found_poof && found_smoke);
    }

    #[test]
    fn weighted_tracker_is_seeded_air_filtered_and_capped_without_mutation() {
        let effects = vec![explosion_fixture(700, 1)];
        let before = effects[0].center;
        let world = std::collections::HashMap::from([((0, 0, 0), 17u32)]);
        let world_before = world.clone();
        let mut air_reads = 0;
        let mut first_rng = fastrand::Rng::with_seed(42);
        let mut second_rng = fastrand::Rng::with_seed(42);
        let first = plan_explosion_particles(&effects, &mut first_rng, |x, y, z| {
            air_reads += 1;
            let _ = world.get(&(x, y, z));
            true
        });
        let mut false_reads = 0;
        let second = plan_explosion_particles(&effects, &mut second_rng, |_, _, _| {
            false_reads += 1;
            false
        });
        assert_eq!(first.len(), 512);
        assert_eq!(second.len(), 0);
        assert_eq!(effects[0].center, before);
        assert_eq!(world, world_before);
        assert_eq!(air_reads, 700.min(512));
        assert_eq!(false_reads, 700.min(512));
        let mut zero_rng = fastrand::Rng::with_seed(42);
        assert!(
            plan_explosion_particles(&[explosion_fixture(0, 1)], &mut zero_rng, |_, _, _| {
                panic!("zero candidates should not read the world")
            })
            .is_empty()
        );
    }

    #[test]
    fn seeded_tracker_applies_exact_scaling_speed_and_sampled_position() {
        let mut effect = explosion_fixture(1, 1);
        effect.block_particles[0].value.scaling = 1.25;
        effect.block_particles[0].value.speed = 0.75;
        let mut rng = fastrand::Rng::with_seed(0x26_02);
        let plan =
            plan_explosion_particles(std::slice::from_ref(&effect), &mut rng, |_, _, _| true);
        assert_eq!(plan.len(), 1);
        let particle = &plan[0];
        for (actual, expected) in [
            (particle.pos.x, -0.4607938680388368),
            (particle.pos.y, 1.4011020870320938),
            (particle.pos.z, -0.6507520787596177),
            (particle.velocity.x, -0.1254192857517548),
            (particle.velocity.y, 0.11762729124787932),
            (particle.velocity.z, -0.1502158877116651),
        ] {
            assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
        }

        effect.radius = 0.0;
        let mut rng = fastrand::Rng::with_seed(0x26_02);
        assert!(plan_explosion_particles(&[effect], &mut rng, |_, _, _| true).is_empty());
    }

    #[test]
    fn seeded_tracker_uses_block_count_and_inner_particle_weights() {
        let mut light = explosion_fixture(1, 1);
        light.block_particles[0].value.particle = ParticleOptions::Explosion;
        let mut heavy = explosion_fixture(3, 1);
        heavy.block_particles[0].value.particle = ParticleOptions::EndRod;
        let mut rng = fastrand::Rng::with_seed(0x26_02);
        let mut light_count = 0;
        let mut heavy_count = 0;
        for _ in 0..250 {
            let plan =
                plan_explosion_particles(&[light.clone(), heavy.clone()], &mut rng, |_, _, _| true);
            for spawn in plan {
                match spawn.particle {
                    ParticleOptions::Explosion => light_count += 1,
                    ParticleOptions::EndRod => heavy_count += 1,
                    _ => unreachable!(),
                }
            }
        }
        assert!(heavy_count > light_count * 2);

        let mut weighted = explosion_fixture(1, 1);
        weighted.block_particles.push(Weighted {
            value: ExplosionParticleInfo {
                particle: ParticleOptions::EndRod,
                scaling: 1.0,
                speed: 1.0,
            },
            weight: 3,
        });
        let mut rng = fastrand::Rng::with_seed(77);
        let mut end_rod_count = 0;
        for _ in 0..1000 {
            if matches!(
                plan_explosion_particles(std::slice::from_ref(&weighted), &mut rng, |_, _, _| {
                    true
                })[0]
                    .particle,
                ParticleOptions::EndRod
            ) {
                end_rod_count += 1;
            }
        }
        assert!((650..850).contains(&end_rod_count));
    }

    #[test]
    fn server_particle_count_keeps_negative_distinct_from_directional_zero() {
        assert_eq!(packet_particle_count(-1), None);
        assert_eq!(packet_particle_count(0), Some(0));
        assert_eq!(packet_particle_count(3), Some(3));
    }

    #[test]
    fn server_particle_ids_match_26_2_registry_for_supported_simple_options() {
        use super::ServerParticleKind as Kind;

        assert!(matches!(Kind::from_id(27), Some(Kind::EndRod)));
        assert!(matches!(Kind::from_id(29), Some(Kind::ExplosionEmitter)));
        assert!(matches!(Kind::from_id(30), Some(Kind::Explosion)));
        assert!(matches!(Kind::from_id(66), Some(Kind::Poof)));
        assert!(matches!(Kind::from_id(69), Some(Kind::Smoke)));
        assert!(matches!(Kind::from_id(75), Some(Kind::Totem)));
        assert!(matches!(Kind::from_id(21), Some(Kind::Dust))); // RGB + scale decoded separately.
        assert!(matches!(Kind::from_id(1), Some(Kind::Block))); // Block carries a block-state ID.
        assert!(matches!(Kind::from_id(54), Some(Kind::Item))); // item
        assert!(matches!(Kind::from_id(55), Some(Kind::Vibration))); // vibration
        assert!(matches!(Kind::from_id(56), Some(Kind::Trail))); // trail
        assert!(matches!(Kind::from_id(112), Some(Kind::Shriek))); // shriek
    }
}

pub(crate) fn next_gaussian() -> f64 {
    loop {
        let v1 = 2.0 * fastrand::f64() - 1.0;
        let v2 = 2.0 * fastrand::f64() - 1.0;
        let s = v1 * v1 + v2 * v2;
        if s < 1.0 && s != 0.0 {
            return v1 * (-2.0 * s.ln() / s).sqrt();
        }
    }
}

/// Vanilla `Vec3.xRot`: rotation about the X axis.
fn rot_x(v: DVec3, angle: f64) -> DVec3 {
    let (sin, cos) = angle.sin_cos();
    dvec3(v.x, v.y * cos - v.z * sin, v.y * sin + v.z * cos)
}

/// Vanilla `Vec3.yRot`: rotation about the Y axis.
fn rot_y(v: DVec3, angle: f64) -> DVec3 {
    let (sin, cos) = angle.sin_cos();
    dvec3(v.x * cos + v.z * sin, v.y, -v.x * sin + v.z * cos)
}
