//! Particles: a port of vanilla `TerrainParticle`, `BreakingItemParticle`,
//! `EndRodParticle`, `Particle`, `ClientLevel.addDestroyBlockEffect`, and the
//! `ClientboundLevelParticles` spawn path.

use std::collections::HashMap;
use std::sync::Arc;

use azalea_block::BlockState;
use azalea_buf::AzBuf;
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

mod atmosphere;
mod emitters;
mod magic;
mod terrain_extra;
mod water;

pub(crate) use emitters::RenderRequest as SpecialParticleRenderRequest;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParticleMode {
    #[default]
    All,
    Decreased,
    Minimal,
}

impl ParticleMode {
    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Decreased,
            2 => Self::Minimal,
            _ => Self::All,
        }
    }

    pub const fn to_u8(self) -> u8 {
        match self {
            Self::All => 0,
            Self::Decreased => 1,
            Self::Minimal => 2,
        }
    }

    pub const fn cycle(self) -> Self {
        match self {
            Self::All => Self::Decreased,
            Self::Decreased => Self::Minimal,
            Self::Minimal => Self::All,
        }
    }
}

/// Vanilla `ParticleGroup.RESERVOIR_START` — above this, new particles are
/// probabilistically dropped.
const RESERVOIR_START: usize = 12288;
/// Vanilla `Particle.MAXIMUM_COLLISION_VELOCITY_SQUARED` (100²).
const MAX_COLLISION_VELOCITY_SQ: f64 = 10000.0;
/// Terrain particles use the default 0.2-wide, 0.2-tall bounding box.
const HALF_WIDTH: f64 = 0.1;

fn accept_particle(
    mode: ParticleMode,
    bypass: bool,
    always_visible: bool,
    distance_squared: f64,
    rng: &mut impl FnMut() -> u32,
) -> bool {
    // Java calculates the effective ParticleStatus before either limiter check,
    // so these RNG draws also occur for distant and override-limiter particles.
    let mode = if mode == ParticleMode::Minimal && always_visible && rng() % 10 == 0 {
        ParticleMode::Decreased
    } else {
        mode
    };
    let mode = if mode == ParticleMode::Decreased && rng() % 3 == 0 {
        ParticleMode::Minimal
    } else {
        mode
    };
    bypass || (distance_squared <= 1024.0 && mode != ParticleMode::Minimal)
}

#[derive(PartialEq, Eq)]
enum Kind {
    /// `TerrainParticle` / `BreakingItemParticle`: collision physics,
    /// world-lit, fixed sprite, opaque layer.
    Terrain,
    /// Breaking-hit terrain after Java `scale(0.6)`, which narrows collision
    /// bounds without changing the rendered quad size.
    TerrainScaled,
    /// `BreakingItemParticle`: terrain-like physics with an item-model particle
    /// icon.
    Item,
    ItemTranslucent,
    /// `EndRodParticle` (a `SimpleAnimatedParticle`): no collision,
    /// full-bright, 8-frame animation, fades after half-life, translucent
    /// layer.
    EndRod,
    /// `SpellParticle.MobEffectProvider`: rising, animated, translucent.
    EntityEffect,
    SpellEffect,
    FixedEffect,
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
    CampfireCosySmoke,
    CampfireSignalSmoke,
    Crit,
    Dust,
    DustColorTransition,
    Shriek,
    Trail,
    Vibration,
    Atmosphere(atmosphere::State),
    Water(water::State),
    Magic(magic::State),
    TerrainExtra(terrain_extra::State),
    Emitters(emitters::State),
}

impl Kind {
    /// Java `TrialSpawnerDetectionParticle.getFacingCameraMode`.
    fn look_at_y(&self) -> bool {
        matches!(self, Kind::Magic(state) if look_at_y(state.kind))
    }

    /// Vanilla `SingleQuadParticle.getLayer`.
    fn translucent(&self) -> bool {
        if let Kind::TerrainExtra(state) = self {
            return terrain_extra::translucent(*state);
        }
        if let Kind::Atmosphere(state) = self {
            return atmosphere::translucent(*state);
        }
        if let Kind::Water(state) = self {
            return water::translucent(*state);
        }
        if let Kind::Magic(state) = self {
            return magic::translucent(state.kind);
        }
        if let Kind::Emitters(state) = self {
            return emitters::translucent(state.kind);
        }
        matches!(
            self,
            Kind::ItemTranslucent
                | Kind::EndRod
                | Kind::EntityEffect
                | Kind::SpellEffect
                | Kind::FixedEffect
                | Kind::Totem
                | Kind::CampfireCosySmoke
                | Kind::CampfireSignalSmoke
                | Kind::Shriek
                | Kind::Vibration
        )
    }
}

fn look_at_y(kind: ServerParticleKind) -> bool {
    matches!(
        kind,
        ServerParticleKind::TrialSpawnerDetection
            | ServerParticleKind::TrialSpawnerDetectionOminous
    )
}

fn provider_light_uv(
    kind: ServerParticleKind,
    sky: u8,
    block: u8,
    age: i32,
    lifetime: i32,
    partial: f32,
) -> Option<u32> {
    let progress = ((age as f32 + partial) / lifetime.max(1) as f32).clamp(0.0, 1.0);
    let block = block.saturating_mul(16);
    let sky = sky.saturating_mul(16);
    let with_block_15 = || Some(240 | (u32::from(sky) << 8));
    let add_emission = |emission: f32| {
        Some(
            u32::from(
                block
                    .saturating_add((emission.clamp(0.0, 1.0) * 240.0) as u8)
                    .min(240),
            ) | (u32::from(sky) << 8),
        )
    };
    match kind {
        ServerParticleKind::CopperFireFlame
        | ServerParticleKind::Flame
        | ServerParticleKind::SoulFireFlame
        | ServerParticleKind::SmallFlame => add_emission(progress),
        ServerParticleKind::Soul
        | ServerParticleKind::SculkSoul
        | ServerParticleKind::Lava
        | ServerParticleKind::DrippingObsidianTear
        | ServerParticleKind::FallingObsidianTear
        | ServerParticleKind::LandingObsidianTear
        | ServerParticleKind::TrialSpawnerDetection
        | ServerParticleKind::TrialSpawnerDetectionOminous
        | ServerParticleKind::SculkCharge
        | ServerParticleKind::SculkChargePop
        | ServerParticleKind::VaultConnection
        | ServerParticleKind::OminousSpawning => with_block_15(),
        ServerParticleKind::Firefly => {
            let fade = if progress >= 0.9 {
                (1.0 - progress) / 0.1
            } else if progress <= 0.3 {
                progress / 0.3
            } else {
                1.0
            };
            Some((255.0 * fade) as u32)
        }
        ServerParticleKind::Glow
        | ServerParticleKind::WaxOn
        | ServerParticleKind::WaxOff
        | ServerParticleKind::ElectricSpark
        | ServerParticleKind::Scrape => add_emission(progress),
        ServerParticleKind::Portal | ServerParticleKind::ReversePortal => add_emission(
            (age.max(0) as f32 / lifetime.max(1) as f32)
                .clamp(0.0, 1.0)
                .powi(8),
        ),
        ServerParticleKind::Enchant | ServerParticleKind::Nautilus => {
            add_emission(progress.powi(8))
        }
        _ => None,
    }
}

fn particle_light_uv(
    kind: &Kind,
    sky: u8,
    block: u8,
    age: i32,
    lifetime: i32,
    partial: f32,
) -> u32 {
    let provider = match kind {
        Kind::Atmosphere(state) => Some(state.kind),
        Kind::Water(state) => Some(state.kind),
        Kind::Magic(state) => Some(state.kind),
        Kind::TerrainExtra(state) => Some(state.kind),
        Kind::Emitters(state) => Some(state.kind),
        _ => None,
    };
    let fullbright = matches!(
        kind,
        Kind::EndRod
            | Kind::Totem
            | Kind::Explosion
            | Kind::Trail
            | Kind::Magic(magic::State {
                kind: ServerParticleKind::Firework
                    | ServerParticleKind::SonicBoom
                    | ServerParticleKind::SweepAttack
                    | ServerParticleKind::Gust
                    | ServerParticleKind::SmallGust,
                ..
            })
            | Kind::Water(water::State {
                kind: ServerParticleKind::SquidInk | ServerParticleKind::GlowSquidInk,
                ..
            })
            | Kind::Emitters(emitters::State {
                kind: ServerParticleKind::ElderGuardian,
                ..
            })
    );
    if fullbright {
        return 0xF0F0;
    }
    if matches!(kind, Kind::Shriek | Kind::Vibration) {
        return 240 | (u32::from(sky.saturating_mul(16)) << 8);
    }
    provider
        .and_then(|provider| provider_light_uv(provider, sky, block, age, lifetime, partial))
        .unwrap_or_else(|| {
            u32::from(block.saturating_mul(16)) | (u32::from(sky.saturating_mul(16)) << 8)
        })
}

#[derive(Clone, Copy)]
pub(super) struct Appearance {
    pub(super) size: f32,
    pub(super) color: [f32; 3],
    pub(super) alpha: f32,
    pub(super) rotation: Quat,
    pub(super) second_rotation: Option<Quat>,
}

pub(crate) struct ParticleSoundRequest {
    pub(crate) event: &'static str,
    pub(crate) pos: DVec3,
    pub(crate) volume: f32,
    pub(crate) pitch: f32,
    pub(crate) seed: u64,
}

/// Typed client-local composite effect from `ClientLevel.createFireworks`.
#[derive(Clone, Debug)]
pub(crate) struct FireworkStarterRequest {
    pub(crate) position: DVec3,
    pub(crate) velocity: DVec3,
    pub(crate) far_effect: bool,
    pub(crate) explosions: Vec<azalea_inventory::components::FireworkExplosion>,
}

struct FireworkStarter {
    request: FireworkStarterRequest,
    life: usize,
    lifetime: usize,
    twinkle_delay: bool,
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
    fn poof(pos: DVec3, velocity_arg: DVec3, frames: &[AtlasRegion]) -> Self {
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
    fn smoke(pos: DVec3, velocity: DVec3, frames: &[AtlasRegion]) -> Self {
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

    fn campfire_smoke(
        pos: DVec3,
        signal: bool,
        frames: &[AtlasRegion; 8],
        rng: &mut fastrand::Rng,
    ) -> Self {
        let size = 0.1 * (rng.f32() * 0.5 + 0.5) * 2.0 * 3.0;
        let lifetime = rng.i32(0..50) + if signal { 280 } else { 80 };
        let mut particle = Self {
            kind: if signal {
                Kind::CampfireSignalSmoke
            } else {
                Kind::CampfireCosySmoke
            },
            pos,
            prev_pos: pos,
            vel: dvec3(0.0, 0.07 + f64::from(rng.f32() / 500.0), 0.0),
            age: 0,
            lifetime,
            on_ground: false,
            stopped_by_collision: false,
            gravity: 3.0e-6,
            friction: 0.98,
            size,
            base_size: size,
            u0: 0.0,
            u1: 0.0,
            v0: 0.0,
            v1: 0.0,
            color: [1.0; 3],
            alpha: if signal { 0.95 } else { 0.9 },
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
        particle.set_sprite(&frames[rng.usize(0..frames.len())]);
        particle
    }

    fn dust(
        pos: DVec3,
        velocity: DVec3,
        color: [f32; 3],
        scale: f32,
        sprite: AtlasRegion,
        rng: &mut fastrand::Rng,
    ) -> Self {
        Self::dust_with_transition(pos, velocity, color, None, scale, sprite, rng)
    }

    fn dust_with_transition(
        pos: DVec3,
        velocity: DVec3,
        color: [f32; 3],
        to_color: Option<[f32; 3]>,
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
        let end_color =
            to_color.map(|to| to.map(|channel| (rng.f32() * 0.2 + 0.8) * channel * base_factor));
        let mut particle = Self {
            kind: if end_color.is_some() {
                Kind::DustColorTransition
            } else {
                Kind::Dust
            },
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
            target: end_color.map(|c| dvec3(c[0] as f64, c[1] as f64, c[2] as f64)),
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

    fn entity_effect(pos: DVec3, velocity: DVec3, color: u32, frames: &[AtlasRegion; 8]) -> Self {
        Self::spell_particle(
            Kind::EntityEffect,
            pos,
            velocity,
            [
                ((color >> 16) & 0xff) as f32 / 255.0,
                ((color >> 8) & 0xff) as f32 / 255.0,
                (color & 0xff) as f32 / 255.0,
            ],
            ((color >> 24) & 0xff) as f32 / 255.0,
            frames,
        )
    }

    fn spell_effect(pos: DVec3, velocity: DVec3, color: u32, frames: &[AtlasRegion; 8]) -> Self {
        Self::spell_particle(
            Kind::SpellEffect,
            pos,
            velocity,
            [
                ((color >> 16) & 0xff) as f32 / 255.0,
                ((color >> 8) & 0xff) as f32 / 255.0,
                (color & 0xff) as f32 / 255.0,
            ],
            1.0,
            frames,
        )
    }

    fn spell_particle(
        kind: Kind,
        pos: DVec3,
        velocity: DVec3,
        color: [f32; 3],
        alpha: f32,
        frames: &[AtlasRegion; 8],
    ) -> Self {
        // SpellParticle evaluates its two random horizontal base velocities
        // before Particle's lifetime and randomized velocity constructor.
        let xa = 0.5 - fastrand::f64();
        let za = 0.5 - fastrand::f64();
        let _base_lifetime = (4.0 / (fastrand::f32() * 0.9 + 0.1)) as i32;
        let jitter = || (fastrand::f32() * 2.0 - 1.0) * 0.4;
        let mut vel = dvec3(
            xa + f64::from(jitter()),
            velocity.y + f64::from(jitter()),
            za + f64::from(jitter()),
        );
        let speed = f64::from((fastrand::f32() + fastrand::f32() + 1.0) * 0.15);
        vel = vel / vel.length() * speed * 0.4;
        vel.y += 0.1;
        let base_size = 0.1 * (fastrand::f32() * 0.5 + 0.5) * 2.0;
        let lifetime = (8.0 / (fastrand::f32() * 0.8 + 0.2)) as i32;
        vel.y *= 0.2;
        if velocity.x == 0.0 && velocity.z == 0.0 {
            vel.x *= 0.1;
            vel.z *= 0.1;
        }
        let mut particle =
            Self::special(kind, pos, vel, None, lifetime, base_size * 0.75, frames[0]);
        particle.base_size = base_size * 0.75;
        particle.color = color;
        particle.alpha = alpha;
        particle.gravity = -0.1;
        particle.friction = 0.96;
        particle.light = 0.0;
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
        generic_frames: &[AtlasRegion],
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
        generic_frames: &[AtlasRegion],
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
        let campfire_smoke = matches!(
            self.kind,
            Kind::CampfireCosySmoke | Kind::CampfireSignalSmoke
        );
        if campfire_smoke {
            let jitter = || fastrand::f64() * 0.0002 * if fastrand::bool() { 1.0 } else { -1.0 };
            self.vel.x += jitter();
            self.vel.z += jitter();
            self.vel.y -= self.gravity;
        } else {
            self.vel.y -= 0.04 * self.gravity;
        }
        match self.kind {
            Kind::Terrain
            | Kind::TerrainScaled
            | Kind::Item
            | Kind::ItemTranslucent
            | Kind::Smoke
            | Kind::Poof
            | Kind::Dust
            | Kind::DustColorTransition => self.move_with_collision(chunks),
            Kind::CampfireCosySmoke | Kind::CampfireSignalSmoke => {
                self.move_with_collision_width(chunks, 0.125)
            }
            Kind::EndRod
            | Kind::EntityEffect
            | Kind::SpellEffect
            | Kind::FixedEffect
            | Kind::Crit
            | Kind::Shriek => self.pos += self.vel,
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
            Kind::Atmosphere(_)
            | Kind::Water(_)
            | Kind::Magic(_)
            | Kind::TerrainExtra(_)
            | Kind::Emitters(_) => {}
        }
        if self.kind == Kind::Smoke && self.pos.y == self.prev_pos.y {
            self.vel.x *= 1.1;
            self.vel.z *= 1.1;
        }
        if matches!(
            self.kind,
            Kind::CampfireCosySmoke | Kind::CampfireSignalSmoke
        ) && self.age >= self.lifetime - 60
            && self.alpha > 0.01
        {
            self.alpha -= 0.015;
        }
        if !campfire_smoke {
            self.vel *= self.friction;
        }
        if self.on_ground {
            self.vel.x *= 0.7;
            self.vel.z *= 0.7;
        }
        match self.kind {
            Kind::Terrain
            | Kind::TerrainScaled
            | Kind::Item
            | Kind::ItemTranslucent
            | Kind::Smoke
            | Kind::Poof
            | Kind::Dust
            | Kind::DustColorTransition => {
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
            Kind::EntityEffect | Kind::SpellEffect | Kind::FixedEffect => {
                self.light = world_brightness(
                    chunks,
                    self.pos.x.floor() as i32,
                    self.pos.y.floor() as i32,
                    self.pos.z.floor() as i32,
                );
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
            Kind::CampfireCosySmoke | Kind::CampfireSignalSmoke => {
                self.light = world_brightness(
                    chunks,
                    self.pos.x.floor() as i32,
                    self.pos.y.floor() as i32,
                    self.pos.z.floor() as i32,
                );
            }
            Kind::Atmosphere(_)
            | Kind::Water(_)
            | Kind::Magic(_)
            | Kind::TerrainExtra(_)
            | Kind::Emitters(_) => {}
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
        self.move_with_collision_width(chunks, collision_half_width(&self.kind));
    }

    fn move_with_collision_width(&mut self, chunks: &ChunkStore, half_width: f64) {
        if self.stopped_by_collision {
            return;
        }
        let orig = self.vel;
        let mut delta = orig;
        if delta != DVec3::ZERO && delta.length_squared() < MAX_COLLISION_VELOCITY_SQ {
            let aabb = Aabb::from_center(self.pos, half_width, half_width);
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

fn explosion_frame_index(age: i32, lifetime: i32) -> usize {
    ((age.max(0) as usize * 15) / lifetime.max(1) as usize).min(15)
}

fn animated_frame_index(age: i32, lifetime: i32, frames: usize) -> usize {
    ((age.max(0) as usize * (frames - 1)) / lifetime.max(1) as usize).min(frames - 1)
}

fn breaking_effect_position(
    pos: BlockPos,
    bounds: LocalBox,
    face: azalea_core::direction::Direction,
    random: [f64; 3],
) -> DVec3 {
    let sample = |random: f64, min: f64, max: f64| {
        random * (max - min - f64::from(0.2f32)) + f64::from(0.1f32) + min
    };
    let mut p = DVec3::new(
        pos.x as f64 + sample(random[0], bounds[0], bounds[3]),
        pos.y as f64 + sample(random[1], bounds[1], bounds[4]),
        pos.z as f64 + sample(random[2], bounds[2], bounds[5]),
    );
    let face_offset = f64::from(0.1f32);
    match face {
        azalea_core::direction::Direction::Down => p.y = pos.y as f64 + bounds[1] - face_offset,
        azalea_core::direction::Direction::Up => p.y = pos.y as f64 + bounds[4] + face_offset,
        azalea_core::direction::Direction::North => p.z = pos.z as f64 + bounds[2] - face_offset,
        azalea_core::direction::Direction::South => p.z = pos.z as f64 + bounds[5] + face_offset,
        azalea_core::direction::Direction::West => p.x = pos.x as f64 + bounds[0] - face_offset,
        azalea_core::direction::Direction::East => p.x = pos.x as f64 + bounds[3] + face_offset,
    }
    p
}

fn terrain_power(velocity: DVec3, power: f32) -> DVec3 {
    let power = f64::from(power);
    let y_offset = f64::from(0.1f32);
    DVec3::new(
        velocity.x * power,
        (velocity.y - y_offset) * power + y_offset,
        velocity.z * power,
    )
}

fn collision_half_width(kind: &Kind) -> f64 {
    if matches!(kind, Kind::TerrainScaled) {
        f64::from(0.2f32 * 0.6f32) * 0.5
    } else {
        HALF_WIDTH
    }
}

fn effect_particle_denominator(ambient: bool, invisible: bool) -> u32 {
    (if invisible { 15 } else { 4 }) * if ambient { 5 } else { 1 }
}

fn effect_particle_selected(sample: u32, ambient: bool, invisible: bool) -> bool {
    sample % effect_particle_denominator(ambient, invisible) == 0
}

fn spell_brightness(random: f32) -> f32 {
    random * 0.5 + 0.35
}

fn gaussian_sample() -> f64 {
    (-2.0 * fastrand::f64().max(f64::MIN_POSITIVE).ln()).sqrt()
        * (std::f64::consts::TAU * fastrand::f64()).cos()
}

fn color_particle_argb(color: &azalea_core::color::RgbColor) -> u32 {
    let mut bytes = [0u8; 4];
    let mut output = &mut bytes[..];
    color
        .azalea_write(&mut output)
        .expect("writing a particle color to a fixed byte array cannot fail");
    u32::from_be_bytes(bytes)
}

fn dust_quad_size(base_size: f32, age: i32, lifetime: i32, partial_tick: f32) -> f32 {
    base_size * (((age as f32 + partial_tick) / lifetime as f32) * 32.0).clamp(0.0, 1.0)
}

/// Java `DustColorTransitionParticle.lerpColors` uses `(age + partial) /
/// (lifetime + 1)`.
fn dust_transition_color(
    from: [f32; 3],
    to: [f32; 3],
    age: i32,
    lifetime: i32,
    partial_tick: f32,
) -> [f32; 3] {
    let t = ((age as f32 + partial_tick) / (lifetime as f32 + 1.0)).clamp(0.0, 1.0);
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t)
}

/// `crit.json` and `enchanted_hit.json` each register one provider-specific
/// sprite.
pub const CRIT_SPRITE: &str = "particle/critical_hit";
pub const ENCHANTED_HIT_SPRITE: &str = "particle/enchanted_hit";

/// `POOF` and `SMOKE` use the first eight entries. Dust is included last so
/// the atlas builder also packs its static sprite.
pub const CAMPFIRE_COSY_SMOKE_SPRITES: [&str; 8] = [
    "particle/big_smoke_0",
    "particle/big_smoke_1",
    "particle/big_smoke_2",
    "particle/big_smoke_3",
    "particle/big_smoke_4",
    "particle/big_smoke_5",
    "particle/big_smoke_6",
    "particle/big_smoke_7",
];
pub const CAMPFIRE_SIGNAL_SMOKE_SPRITES: [&str; 8] = CAMPFIRE_COSY_SMOKE_SPRITES;

/// Frame order from entity_effect.json / ambient_entity_effect.json.
pub const ENTITY_EFFECT_SPRITES: [&str; 8] = [
    "particle/effect_7",
    "particle/effect_6",
    "particle/effect_5",
    "particle/effect_4",
    "particle/effect_3",
    "particle/effect_2",
    "particle/effect_1",
    "particle/effect_0",
];
pub const SPELL_SPRITES: [&str; 8] = [
    "particle/spell_7",
    "particle/spell_6",
    "particle/spell_5",
    "particle/spell_4",
    "particle/spell_3",
    "particle/spell_2",
    "particle/spell_1",
    "particle/spell_0",
];
pub const RAID_OMEN_SPRITE: &str = "particle/raid_omen";
pub const TRIAL_OMEN_SPRITE: &str = "particle/trial_omen";

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

/// Every native Java 26.2 particle identity. ID resolution is delegated to
/// the native registry so this list never duplicates numeric registry IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerParticleKind {
    AngryVillager,
    Block,
    BlockMarker,
    Bubble,
    SulfurBubbles,
    NoxiousGas,
    NoxiousGasCloud,
    Geyser,
    GeyserBase,
    GeyserPoof,
    GeyserPlume,
    Cloud,
    CopperFireFlame,
    Crit,
    DamageIndicator,
    DragonBreath,
    DrippingLava,
    FallingLava,
    LandingLava,
    DrippingWater,
    FallingWater,
    Dust,
    DustColorTransition,
    Effect,
    ElderGuardian,
    EnchantedHit,
    Enchant,
    EndRod,
    EntityEffect,
    ExplosionEmitter,
    Explosion,
    Gust,
    SmallGust,
    GustEmitterLarge,
    GustEmitterSmall,
    SonicBoom,
    FallingDust,
    Firework,
    Fishing,
    Flame,
    Infested,
    CherryLeaves,
    PaleOakLeaves,
    TintedLeaves,
    SculkSoul,
    SculkCharge,
    SculkChargePop,
    SoulFireFlame,
    Soul,
    Flash,
    HappyVillager,
    Composter,
    Heart,
    InstantEffect,
    Item,
    Vibration,
    Trail,
    PauseMobGrowth,
    ResetMobGrowth,
    ItemSlime,
    ItemCobweb,
    ItemSnowball,
    LargeSmoke,
    Lava,
    Mycelium,
    Note,
    Poof,
    Portal,
    Rain,
    Smoke,
    WhiteSmoke,
    Sneeze,
    Spit,
    SquidInk,
    SweepAttack,
    Totem,
    Underwater,
    Splash,
    Witch,
    BubblePop,
    CurrentDown,
    BubbleColumnUp,
    Nautilus,
    Dolphin,
    CampfireCosySmoke,
    CampfireSignalSmoke,
    DrippingHoney,
    FallingHoney,
    LandingHoney,
    FallingNectar,
    FallingSporeBlossom,
    Ash,
    CrimsonSpore,
    WarpedSpore,
    SporeBlossomAir,
    DrippingObsidianTear,
    FallingObsidianTear,
    LandingObsidianTear,
    ReversePortal,
    WhiteAsh,
    SmallFlame,
    Snowflake,
    DrippingDripstoneLava,
    FallingDripstoneLava,
    DrippingDripstoneWater,
    FallingDripstoneWater,
    GlowSquidInk,
    Glow,
    WaxOn,
    WaxOff,
    ElectricSpark,
    Scrape,
    Shriek,
    EggCrack,
    DustPlume,
    TrialSpawnerDetection,
    TrialSpawnerDetectionOminous,
    VaultConnection,
    DustPillar,
    OminousSpawning,
    RaidOmen,
    TrialOmen,
    BlockCrumble,
    Firefly,
    SulfurCubeGoo,
}

/// Exact option payloads supported by the Java 26.2 particle codecs.
#[derive(Clone, Debug)]
pub enum ServerParticleOptions {
    Simple,
    EntityEffect {
        color: u32,
    },
    Spell {
        color: i32,
        power: f32,
    },
    Dust {
        packed_color: i32,
        scale: f32,
    },
    DustColorTransition {
        from_color: i32,
        to_color: i32,
        scale: f32,
    },
    Color {
        color: i32,
    },
    Power {
        power: f32,
    },
    SculkCharge {
        roll: f32,
    },
    Geyser {
        water_blocks: i32,
    },
    GeyserBase {
        water_blocks: i32,
        burst_impulse_base: f32,
    },
    Block(BlockState),
    Item {
        item_id: u32,
        count: i32,
        components: azalea_inventory::DataComponentPatch,
        raw_components: Option<std::sync::Arc<simdnbt::owned::NbtCompound>>,
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
    pub fn from_id(id: u32) -> Option<Self> {
        let name = pomme_protocol::registries::RegistryTable::native()
            .name_of(pomme_protocol::registries::ClientRegistry::ParticleType, id)?;
        Self::from_name(name)
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "angry_villager" => Self::AngryVillager,
            "block" => Self::Block,
            "block_marker" => Self::BlockMarker,
            "bubble" => Self::Bubble,
            "sulfur_bubbles" => Self::SulfurBubbles,
            "noxious_gas" => Self::NoxiousGas,
            "noxious_gas_cloud" => Self::NoxiousGasCloud,
            "geyser" => Self::Geyser,
            "geyser_base" => Self::GeyserBase,
            "geyser_poof" => Self::GeyserPoof,
            "geyser_plume" => Self::GeyserPlume,
            "cloud" => Self::Cloud,
            "copper_fire_flame" => Self::CopperFireFlame,
            "crit" => Self::Crit,
            "damage_indicator" => Self::DamageIndicator,
            "dragon_breath" => Self::DragonBreath,
            "dripping_lava" => Self::DrippingLava,
            "falling_lava" => Self::FallingLava,
            "landing_lava" => Self::LandingLava,
            "dripping_water" => Self::DrippingWater,
            "falling_water" => Self::FallingWater,
            "dust" => Self::Dust,
            "dust_color_transition" => Self::DustColorTransition,
            "effect" => Self::Effect,
            "elder_guardian" => Self::ElderGuardian,
            "enchanted_hit" => Self::EnchantedHit,
            "enchant" => Self::Enchant,
            "end_rod" => Self::EndRod,
            "entity_effect" => Self::EntityEffect,
            "explosion_emitter" => Self::ExplosionEmitter,
            "explosion" => Self::Explosion,
            "gust" => Self::Gust,
            "small_gust" => Self::SmallGust,
            "gust_emitter_large" => Self::GustEmitterLarge,
            "gust_emitter_small" => Self::GustEmitterSmall,
            "sonic_boom" => Self::SonicBoom,
            "falling_dust" => Self::FallingDust,
            "firework" => Self::Firework,
            "fishing" => Self::Fishing,
            "flame" => Self::Flame,
            "infested" => Self::Infested,
            "cherry_leaves" => Self::CherryLeaves,
            "pale_oak_leaves" => Self::PaleOakLeaves,
            "tinted_leaves" => Self::TintedLeaves,
            "sculk_soul" => Self::SculkSoul,
            "sculk_charge" => Self::SculkCharge,
            "sculk_charge_pop" => Self::SculkChargePop,
            "soul_fire_flame" => Self::SoulFireFlame,
            "soul" => Self::Soul,
            "flash" => Self::Flash,
            "happy_villager" => Self::HappyVillager,
            "composter" => Self::Composter,
            "heart" => Self::Heart,
            "instant_effect" => Self::InstantEffect,
            "item" => Self::Item,
            "vibration" => Self::Vibration,
            "trail" => Self::Trail,
            "pause_mob_growth" => Self::PauseMobGrowth,
            "reset_mob_growth" => Self::ResetMobGrowth,
            "item_slime" => Self::ItemSlime,
            "item_cobweb" => Self::ItemCobweb,
            "item_snowball" => Self::ItemSnowball,
            "large_smoke" => Self::LargeSmoke,
            "lava" => Self::Lava,
            "mycelium" => Self::Mycelium,
            "note" => Self::Note,
            "poof" => Self::Poof,
            "portal" => Self::Portal,
            "rain" => Self::Rain,
            "smoke" => Self::Smoke,
            "white_smoke" => Self::WhiteSmoke,
            "sneeze" => Self::Sneeze,
            "spit" => Self::Spit,
            "squid_ink" => Self::SquidInk,
            "sweep_attack" => Self::SweepAttack,
            "totem_of_undying" => Self::Totem,
            "underwater" => Self::Underwater,
            "splash" => Self::Splash,
            "witch" => Self::Witch,
            "bubble_pop" => Self::BubblePop,
            "current_down" => Self::CurrentDown,
            "bubble_column_up" => Self::BubbleColumnUp,
            "nautilus" => Self::Nautilus,
            "dolphin" => Self::Dolphin,
            "campfire_cosy_smoke" => Self::CampfireCosySmoke,
            "campfire_signal_smoke" => Self::CampfireSignalSmoke,
            "dripping_honey" => Self::DrippingHoney,
            "falling_honey" => Self::FallingHoney,
            "landing_honey" => Self::LandingHoney,
            "falling_nectar" => Self::FallingNectar,
            "falling_spore_blossom" => Self::FallingSporeBlossom,
            "ash" => Self::Ash,
            "crimson_spore" => Self::CrimsonSpore,
            "warped_spore" => Self::WarpedSpore,
            "spore_blossom_air" => Self::SporeBlossomAir,
            "dripping_obsidian_tear" => Self::DrippingObsidianTear,
            "falling_obsidian_tear" => Self::FallingObsidianTear,
            "landing_obsidian_tear" => Self::LandingObsidianTear,
            "reverse_portal" => Self::ReversePortal,
            "white_ash" => Self::WhiteAsh,
            "small_flame" => Self::SmallFlame,
            "snowflake" => Self::Snowflake,
            "dripping_dripstone_lava" => Self::DrippingDripstoneLava,
            "falling_dripstone_lava" => Self::FallingDripstoneLava,
            "dripping_dripstone_water" => Self::DrippingDripstoneWater,
            "falling_dripstone_water" => Self::FallingDripstoneWater,
            "glow_squid_ink" => Self::GlowSquidInk,
            "glow" => Self::Glow,
            "wax_on" => Self::WaxOn,
            "wax_off" => Self::WaxOff,
            "electric_spark" => Self::ElectricSpark,
            "scrape" => Self::Scrape,
            "shriek" => Self::Shriek,
            "egg_crack" => Self::EggCrack,
            "dust_plume" => Self::DustPlume,
            "trial_spawner_detection" => Self::TrialSpawnerDetection,
            "trial_spawner_detection_ominous" => Self::TrialSpawnerDetectionOminous,
            "vault_connection" => Self::VaultConnection,
            "dust_pillar" => Self::DustPillar,
            "ominous_spawning" => Self::OminousSpawning,
            "raid_omen" => Self::RaidOmen,
            "trial_omen" => Self::TrialOmen,
            "block_crumble" => Self::BlockCrumble,
            "firefly" => Self::Firefly,
            "sulfur_cube_goo" => Self::SulfurCubeGoo,
            _ => return None,
        })
    }

    /// Vanilla `ParticleType.getOverrideLimiter`.
    fn override_limiter(self) -> bool {
        matches!(
            self,
            Self::BlockMarker
                | Self::Geyser
                | Self::GeyserBase
                | Self::GeyserPoof
                | Self::GeyserPlume
                | Self::DamageIndicator
                | Self::ElderGuardian
                | Self::ExplosionEmitter
                | Self::Explosion
                | Self::Gust
                | Self::GustEmitterLarge
                | Self::GustEmitterSmall
                | Self::SonicBoom
                | Self::SculkCharge
                | Self::SculkChargePop
                | Self::Poof
                | Self::Spit
                | Self::SquidInk
                | Self::SweepAttack
                | Self::CampfireCosySmoke
                | Self::CampfireSignalSmoke
                | Self::GlowSquidInk
                | Self::Glow
                | Self::WaxOn
                | Self::WaxOff
                | Self::ElectricSpark
                | Self::Scrape
                | Self::TrialSpawnerDetection
                | Self::TrialSpawnerDetectionOminous
                | Self::VaultConnection
                | Self::OminousSpawning
                | Self::Vibration
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

fn firework_explosion_sparks(
    atlas: &AtlasUVMap,
    request: &FireworkStarterRequest,
    explosion: &azalea_inventory::components::FireworkExplosion,
) -> Vec<Particle> {
    use azalea_inventory::components::FireworkExplosionShape as Shape;
    const STAR: &[(f64, f64)] = &[
        (0.0, 1.0),
        (0.3455, 0.309),
        (0.9511, 0.309),
        (0.3795918367346939, -0.12653061224489795),
        (0.6122448979591837, -0.8040816326530612),
        (0.0, -0.35918367346938773),
    ];
    const CREEPER: &[(f64, f64)] = &[
        (0.0, 0.2),
        (0.2, 0.2),
        (0.2, 0.6),
        (0.6, 0.6),
        (0.6, 0.2),
        (0.2, 0.2),
        (0.2, 0.0),
        (0.4, 0.0),
        (0.4, -0.6),
        (0.2, -0.6),
        (0.2, -0.4),
        (0.0, -0.4),
    ];
    let colors = if explosion.colors.is_empty() {
        &[0x1e1b1b][..]
    } else {
        explosion.colors.as_slice()
    };
    let fade = || {
        (!explosion.fade_colors.is_empty())
            .then(|| rgb_f32(explosion.fade_colors[fastrand::usize(..explosion.fade_colors.len())]))
    };
    let color = || rgb_f32(colors[fastrand::usize(..colors.len())]);
    let mut result = Vec::new();
    let mut push = |velocity: DVec3| {
        if let Some(particle) = magic::firework_spark(
            atlas,
            request.position,
            velocity,
            color(),
            fade(),
            explosion.has_trail,
            explosion.has_twinkle,
        ) {
            result.push(particle);
        }
    };
    match explosion.shape {
        Shape::SmallBall | Shape::LargeBall => {
            let steps = if explosion.shape == Shape::SmallBall {
                2
            } else {
                4
            };
            let base_speed = if steps == 2 { 0.25 } else { 0.5 };
            for y in -steps..=steps {
                for x in -steps..=steps {
                    let mut z = -steps;
                    while z <= steps {
                        let direction = DVec3::new(
                            f64::from(x) + (fastrand::f64() - fastrand::f64()) * 0.5,
                            f64::from(y) + (fastrand::f64() - fastrand::f64()) * 0.5,
                            f64::from(z) + (fastrand::f64() - fastrand::f64()) * 0.5,
                        );
                        let speed = direction.length() / base_speed + gaussian_sample() * 0.05;
                        push(direction / speed);
                        if y != -steps && y != steps && x != -steps && x != steps {
                            z += steps * 2 - 1;
                        }
                        z += 1;
                    }
                }
            }
        }
        Shape::Star | Shape::Creeper => {
            let coords = if explosion.shape == Shape::Star {
                STAR
            } else {
                CREEPER
            };
            let flat = explosion.shape == Shape::Creeper;
            let (sx, sy) = coords[0];
            push(dvec3(sx * 0.5, sy * 0.5, 0.0));
            let base_angle = fastrand::f64() * std::f64::consts::PI;
            let angle_mod = if flat { 0.034 } else { 0.34 };
            for angle_step in 0..3 {
                let angle = base_angle + angle_step as f64 * std::f64::consts::PI * angle_mod;
                let (mut ox, mut oy) = coords[0];
                for &(tx, ty) in &coords[1..] {
                    for step in 1..=4 {
                        let t = f64::from(step) * 0.25;
                        let mut vx = (ox + (tx - ox) * t) * 0.5;
                        let vy = (oy + (ty - oy) * t) * 0.5;
                        let vz = vx * angle.sin();
                        vx *= angle.cos();
                        for flip in [-1.0, 1.0] {
                            push(dvec3(vx * flip, vy, vz * flip));
                        }
                    }
                    ox = tx;
                    oy = ty;
                }
            }
        }
        Shape::Burst => {
            let off_x = gaussian_sample() * 0.05;
            let off_z = gaussian_sample() * 0.05;
            for _ in 0..70 {
                push(dvec3(
                    request.velocity.x * 0.5 + gaussian_sample() * 0.15 + off_x,
                    request.velocity.y * 0.5 + fastrand::f64() * 0.5,
                    request.velocity.z * 0.5 + gaussian_sample() * 0.15 + off_z,
                ));
            }
        }
    }
    result
}

fn rgb_f32(rgb: i32) -> [f32; 3] {
    [
        ((rgb >> 16) & 255) as f32 / 255.0,
        ((rgb >> 8) & 255) as f32 / 255.0,
        (rgb & 255) as f32 / 255.0,
    ]
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
    mode: ParticleMode,
    particles: Vec<Particle>,
    emitters: Vec<ExplosionEmitter>,
    pending_emitters: Vec<ExplosionEmitter>,
    firework_starters: Vec<FireworkStarter>,
    pending_firework_starters: Vec<FireworkStarter>,
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
    entity_effect_frames: [AtlasRegion; 8],
    spell_frames: [AtlasRegion; 8],
    raid_omen_sprite: AtlasRegion,
    trial_omen_sprite: AtlasRegion,
    generic_frames: [AtlasRegion; 12],
    explosion_frames: [AtlasRegion; 16],
    campfire_cosy_frames: [AtlasRegion; 8],
    campfire_signal_frames: [AtlasRegion; 8],
    grass_colormap: Arc<Colormap>,
    foliage_colormap: Arc<Colormap>,
    dry_foliage_colormap: Arc<Colormap>,
    sound_requests: Vec<ParticleSoundRequest>,
    item_model_dimension: Option<String>,
    item_model_registries: Arc<azalea_core::registry_holder::RegistryHolder>,
}

/// Client `CampfireBlockEntity::particleTick` emits smoke only for lit, dry
/// fires; server `LevelParticles` packets remain on their typed network path.
pub(crate) fn campfire_smoke_enabled(lit: bool, waterlogged: bool) -> bool {
    lit && !waterlogged
}

pub(crate) fn campfire_smoke_count(chance: f32, amount: u32) -> Option<u32> {
    (chance < 0.11).then_some(amount % 2 + 2)
}

pub(crate) fn campfire_food_smoke_enabled(chance: f32) -> bool {
    chance < 0.2
}

pub(crate) fn campfire_slot_smoke_pos(pos: BlockPos, facing: &str, slot: usize) -> DVec3 {
    let rotation = match facing {
        "south" => 0,
        "west" => 1,
        "north" => 2,
        "east" => 3,
        _ => 0,
    };
    let (dx, dz) = match (slot + rotation) % 4 {
        0 => (0.0, 1.0),
        1 => (-1.0, 0.0),
        2 => (0.0, -1.0),
        _ => (1.0, 0.0),
    };
    let (clockwise_x, clockwise_z) = (-dz, dx);
    dvec3(
        pos.x as f64 + 0.5 - dx * 0.3125 + clockwise_x * 0.3125,
        pos.y as f64 + 0.5,
        pos.z as f64 + 0.5 - dz * 0.3125 + clockwise_z * 0.3125,
    )
}

impl ParticleStore {
    #[cfg(test)]
    pub(crate) fn test_pending(&self) -> &[Particle] {
        &self.pending
    }

    #[cfg(test)]
    pub(crate) fn test_pending_positions(&self) -> Vec<DVec3> {
        self.pending.iter().map(|particle| particle.pos).collect()
    }

    #[cfg(test)]
    pub(crate) fn test_pending_color_velocity(&self, index: usize) -> ([f32; 3], DVec3) {
        let particle = &self.pending[index];
        (particle.color, particle.vel)
    }

    pub fn set_mode(&mut self, mode: ParticleMode) {
        self.mode = mode;
    }

    /// The ParticleStore is the common item-particle spawn boundary. The
    /// GameState world lifecycle updates this snapshot before any packet or
    /// local request can spawn against a newly joined/switched level.
    pub(crate) fn set_item_model_world(
        &mut self,
        dimension: Option<&str>,
        registries: Arc<azalea_core::registry_holder::RegistryHolder>,
    ) {
        self.item_model_dimension = dimension.filter(|key| !key.is_empty()).map(str::to_owned);
        self.item_model_registries = registries;
    }

    pub(crate) fn reload_assets(
        &mut self,
        uv_map: AtlasUVMap,
        grass_colormap: Arc<Colormap>,
        foliage_colormap: Arc<Colormap>,
        dry_foliage_colormap: Arc<Colormap>,
    ) {
        let mode = self.mode;
        let dimension = self.item_model_dimension.clone();
        let registries = Arc::clone(&self.item_model_registries);
        *self = Self::new(
            uv_map,
            grass_colormap,
            foliage_colormap,
            dry_foliage_colormap,
        );
        self.mode = mode;
        self.item_model_dimension = dimension;
        self.item_model_registries = registries;
    }

    fn item_particle_region(
        &self,
        registry: &BlockRegistry,
        stack: &azalea_inventory::ItemStack,
        raw_components: Option<&simdnbt::owned::NbtCompound>,
    ) -> AtlasRegion {
        let context = crate::world::block::registry::ItemParticleModelContext {
            dimension: self.item_model_dimension.as_deref(),
            registries: Some(self.item_model_registries.as_ref()),
        };
        registry
            .get_item_particle_materials(stack, raw_components, context)
            .and_then(|materials| {
                (!materials.is_empty())
                    .then(|| {
                        materials
                            .get(fastrand::usize(..materials.len()))
                            .copied()
                            .flatten()
                    })
                    .flatten()
            })
            .filter(|texture| self.uv_map.has_region(texture))
            .map_or_else(
                || self.uv_map.missing_region(),
                |texture| self.uv_map.get_region(texture),
            )
    }

    pub(crate) fn drain_sound_requests(&mut self) -> Vec<ParticleSoundRequest> {
        std::mem::take(&mut self.sound_requests)
    }

    pub(crate) fn add_firework_starter(&mut self, request: FireworkStarterRequest) -> bool {
        if request.explosions.is_empty() {
            return false;
        }
        let twinkle_delay = request.explosions.iter().any(|e| e.has_twinkle);
        let lifetime = request.explosions.len() * 2 - 1 + usize::from(twinkle_delay) * 15;
        self.pending_firework_starters.push(FireworkStarter {
            request,
            life: 0,
            lifetime,
            twinkle_delay,
        });
        true
    }

    fn accepts_normal_spawn(&self, always_visible: bool, rng: &mut impl FnMut() -> u32) -> bool {
        accept_particle(self.mode, false, always_visible, 0.0, rng)
    }

    pub fn add_living_effect_particles(
        &mut self,
        pos: DVec3,
        width: f32,
        height: f32,
        particles: &[ParticleOptions],
        ambient: bool,
        invisible: bool,
        camera_pos: DVec3,
    ) {
        if particles.is_empty() {
            return;
        }
        let option = &particles[fastrand::usize(..particles.len())];
        if !effect_particle_selected(fastrand::u32(..), ambient, invisible) {
            return;
        }
        let raid_frames = [self.raid_omen_sprite; 8];
        let trial_frames = [self.trial_omen_sprite; 8];
        let (color, frames, spell_kind, power) = match option {
            ParticleOptions::EntityEffect(effect) => (
                color_particle_argb(&effect.color),
                &self.entity_effect_frames,
                false,
                1.0,
            ),
            ParticleOptions::Effect(effect) => (
                0xff00_0000 | (effect.color as u32 & 0x00ff_ffff),
                &self.entity_effect_frames,
                false,
                effect.power,
            ),
            ParticleOptions::InstantEffect(effect) => (
                0xff00_0000 | (effect.color as u32 & 0x00ff_ffff),
                &self.spell_frames,
                true,
                effect.power,
            ),
            ParticleOptions::Witch => (0xffff_ffff, &self.spell_frames, true, 1.0),
            ParticleOptions::RaidOmen => (0xffff_ffff, &raid_frames, false, 1.0),
            ParticleOptions::TrialOmen => (0xffff_ffff, &trial_frames, false, 1.0),
            _ => return,
        };
        let particle_pos = pos
            + dvec3(
                (fastrand::f64() - 0.5) * f64::from(width),
                fastrand::f64() * f64::from(height),
                (fastrand::f64() - 0.5) * f64::from(width),
            );
        if !accept_particle(
            self.mode,
            false,
            false,
            camera_pos.distance_squared(particle_pos),
            &mut || fastrand::u32(..),
        ) {
            return;
        }
        let mut particle = if spell_kind {
            Particle::spell_effect(particle_pos, DVec3::ONE, color, frames)
        } else {
            Particle::entity_effect(particle_pos, DVec3::ONE, color, frames)
        };
        if matches!(
            option,
            ParticleOptions::Effect(_) | ParticleOptions::InstantEffect(_)
        ) {
            let power = if power.is_finite() { power } else { 1.0 };
            particle.vel = dvec3(
                particle.vel.x * f64::from(power),
                (particle.vel.y - 0.1) * f64::from(power) + 0.1,
                particle.vel.z * f64::from(power),
            );
        }
        if matches!(option, ParticleOptions::Witch) {
            let brightness = spell_brightness(fastrand::f32());
            particle.color = [brightness, 0.0, brightness];
            particle.kind = Kind::SpellEffect;
        } else if !matches!(option, ParticleOptions::EntityEffect(_))
            && !matches!(
                option,
                ParticleOptions::RaidOmen | ParticleOptions::TrialOmen
            )
        {
            particle.kind = Kind::SpellEffect;
        }
        if matches!(
            option,
            ParticleOptions::RaidOmen | ParticleOptions::TrialOmen
        ) {
            particle.kind = Kind::FixedEffect;
        }
        self.push(particle);
    }

    /// Spawn the exact ParticleOptions carried by LivingEntity metadata. This
    /// is the same provider path as server particles, not an EntityEffect
    /// fallback.
    pub(crate) fn add_living_effect_server_particles(
        &mut self,
        pos: DVec3,
        width: f32,
        height: f32,
        particles: &[(ServerParticleKind, ServerParticleOptions)],
        ambient: bool,
        invisible: bool,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        if particles.is_empty() || !effect_particle_selected(fastrand::u32(..), ambient, invisible)
        {
            return;
        }
        let (kind, options) = &particles[fastrand::usize(..particles.len())];
        let particle_pos = pos
            + dvec3(
                (fastrand::f64() - 0.5) * f64::from(width),
                fastrand::f64() * f64::from(height),
                (fastrand::f64() - 0.5) * f64::from(width),
            );
        self.add_server_particle(
            *kind,
            options.clone(),
            false,
            false,
            particle_pos,
            DVec3::ZERO,
            camera_pos,
            registry,
            chunks,
            biome_climate,
        );
    }

    pub fn add_potion_break_particles(
        &mut self,
        pos: DVec3,
        color: u32,
        instant_effect: bool,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
    ) {
        let stack = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData {
            kind: azalea_registry::builtin::ItemKind::SplashPotion,
            count: 1,
            component_patch: azalea_inventory::DataComponentPatch::default(),
        });
        for _ in 0..8 {
            let sprite = self.item_particle_region(registry, &stack, None);
            let item_pos = pos;
            if accept_particle(
                self.mode,
                false,
                false,
                camera_pos.distance_squared(item_pos),
                &mut || fastrand::u32(..),
            ) {
                self.push(Particle::breaking_item(
                    item_pos,
                    dvec3(
                        gaussian_sample() * 0.15,
                        fastrand::f64() * 0.2,
                        gaussian_sample() * 0.15,
                    ),
                    sprite,
                    world_brightness(
                        chunks,
                        pos.x.floor() as i32,
                        pos.y.floor() as i32,
                        pos.z.floor() as i32,
                    ),
                ));
            }
        }
        let frames = self.potion_break_frames(instant_effect);
        for _ in 0..100 {
            let distance = fastrand::f64() * 4.0;
            let angle = fastrand::f64() * std::f64::consts::TAU;
            let velocity = dvec3(
                angle.cos() * distance,
                0.01 + fastrand::f64() * 0.5,
                angle.sin() * distance,
            );
            let brightness = 0.75 + fastrand::f32() * 0.25;
            let bright_color = 0xff00_0000
                | ((((color >> 16) & 255) as f32 * brightness) as u32) << 16
                | ((((color >> 8) & 255) as f32 * brightness) as u32) << 8
                | ((color & 255) as f32 * brightness) as u32;
            let particle_pos = pos + dvec3(velocity.x * 0.1, 0.3, velocity.z * 0.1);
            if accept_particle(
                self.mode,
                false,
                false,
                camera_pos.distance_squared(particle_pos),
                &mut || fastrand::u32(..),
            ) {
                let mut particle =
                    Particle::entity_effect(particle_pos, velocity, bright_color, &frames);
                if instant_effect {
                    particle.kind = Kind::SpellEffect;
                }
                particle.vel.x *= distance;
                particle.vel.y = (particle.vel.y - 0.1) * distance + 0.1;
                particle.vel.z *= distance;
                self.push(particle);
            }
        }
    }

    fn potion_break_frames(&self, instant_effect: bool) -> [AtlasRegion; 8] {
        if instant_effect {
            self.spell_frames
        } else {
            self.entity_effect_frames
        }
    }

    pub fn add_campfire_food_smoke(&mut self, pos: DVec3, camera_pos: DVec3) {
        if accept_particle(
            self.mode,
            false,
            false,
            camera_pos.distance_squared(pos),
            &mut || fastrand::u32(..),
        ) {
            self.push(Particle::smoke(
                pos,
                dvec3(0.0, 5.0e-4, 0.0),
                &self.generic_frames,
            ));
        }
    }

    pub fn add_campfire_smoke(&mut self, pos: DVec3, signal: bool) {
        // Vanilla `CampfireBlock.makeParticles` calls `addAlwaysVisibleParticle`;
        // preserve that bypass (including its 11% emitter chance in the caller).
        let frames = if signal {
            &self.campfire_signal_frames
        } else {
            &self.campfire_cosy_frames
        };
        self.push(Particle::campfire_smoke(
            pos,
            signal,
            frames,
            &mut fastrand::Rng::new(),
        ));
    }

    #[cfg(test)]
    pub(crate) fn test_particle_count(&self) -> usize {
        self.particles.len() + self.pending.len()
    }

    pub fn new(
        uv_map: AtlasUVMap,
        grass_colormap: Arc<Colormap>,
        foliage_colormap: Arc<Colormap>,
        dry_foliage_colormap: Arc<Colormap>,
    ) -> Self {
        let end_rod_frames = descriptor_frames(&uv_map, "minecraft:end_rod", END_ROD_SPRITES);
        let entity_effect_frames =
            descriptor_frames(&uv_map, "minecraft:entity_effect", ENTITY_EFFECT_SPRITES);
        let spell_frames = descriptor_frames(&uv_map, "minecraft:instant_effect", SPELL_SPRITES);
        let raid_omen_sprite = descriptor_frame(&uv_map, "minecraft:raid_omen", 0)
            .unwrap_or_else(|| uv_map.get_region(RAID_OMEN_SPRITE));
        let trial_omen_sprite = descriptor_frame(&uv_map, "minecraft:trial_omen", 0)
            .unwrap_or_else(|| uv_map.get_region(TRIAL_OMEN_SPRITE));
        let mut generic_frames =
            std::array::from_fn(|i| uv_map.get_region(GENERIC_PARTICLE_SPRITES[i]));
        generic_frames[8] =
            descriptor_frame(&uv_map, "minecraft:dust", 0).unwrap_or(generic_frames[8]);
        generic_frames[9] =
            descriptor_frame(&uv_map, "minecraft:shriek", 0).unwrap_or(generic_frames[9]);
        generic_frames[10] =
            descriptor_frame(&uv_map, "minecraft:trail", 0).unwrap_or(generic_frames[10]);
        generic_frames[11] =
            descriptor_frame(&uv_map, "minecraft:vibration", 0).unwrap_or(generic_frames[11]);
        let explosion_frames = descriptor_frames(&uv_map, "minecraft:explosion", EXPLOSION_SPRITES);
        let campfire_cosy_frames = descriptor_frames(
            &uv_map,
            "minecraft:campfire_cosy_smoke",
            CAMPFIRE_COSY_SMOKE_SPRITES,
        );
        let campfire_signal_frames = descriptor_frames(
            &uv_map,
            "minecraft:campfire_signal_smoke",
            CAMPFIRE_SIGNAL_SMOKE_SPRITES,
        );
        let crit_sprite = descriptor_frame(&uv_map, "minecraft:crit", 0)
            .unwrap_or_else(|| uv_map.get_region(CRIT_SPRITE));
        let enchanted_hit_sprite = descriptor_frame(&uv_map, "minecraft:enchanted_hit", 0)
            .unwrap_or_else(|| uv_map.get_region(ENCHANTED_HIT_SPRITE));
        Self {
            mode: ParticleMode::All,
            particles: Vec::new(),
            pending: Vec::new(),
            emitters: Vec::new(),
            pending_emitters: Vec::new(),
            firework_starters: Vec::new(),
            pending_firework_starters: Vec::new(),
            tracked_explosions: Vec::new(),
            tracking_emitters: Vec::new(),
            crit_sprite,
            enchanted_hit_sprite,
            uv_map,
            end_rod_frames,
            entity_effect_frames,
            spell_frames,
            raid_omen_sprite,
            trial_omen_sprite,
            generic_frames,
            explosion_frames,
            campfire_cosy_frames,
            campfire_signal_frames,
            grass_colormap,
            foliage_colormap,
            dry_foliage_colormap,
            sound_requests: Vec::new(),
            item_model_dimension: None,
            item_model_registries: Arc::new(azalea_core::registry_holder::RegistryHolder::default()),
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

    /// Spawn the engine's internally generated explosion effects.
    fn add_explosion_particle(&mut self, option: &ParticleOptions, pos: DVec3, velocity: DVec3) {
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
            _ => unreachable!("internal explosion particle kind"),
        }
    }

    /// Spawn any typed explosion option through the same provider boundary as
    /// ordinary particle packets. The option codec is shared with metadata.
    pub(crate) fn add_explosion_packet_particle(
        &mut self,
        option: &ParticleOptions,
        pos: DVec3,
        velocity: DVec3,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) -> bool {
        let Some((kind, options)) = crate::net::handler::particle_options_from_typed(option) else {
            return false;
        };
        self.add_particles_from_packet(
            kind,
            options,
            false,
            false,
            pos,
            velocity,
            1.0,
            0,
            camera_pos,
            registry,
            chunks,
            biome_climate,
        );
        true
    }

    /// Resolve queued block particles using the current world and route them
    /// through the ordinary typed particle provider.
    pub(crate) fn spawn_tracked_explosion_particles(
        &mut self,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        if self.tracked_explosions.is_empty() {
            return;
        }
        let mut rng = fastrand::Rng::new();
        let spawns = plan_explosion_particles(&self.tracked_explosions, &mut rng, |x, y, z| {
            is_air(chunks.get_block_state(x, y, z))
        });
        for spawn in spawns {
            if !self.add_explosion_packet_particle(
                &spawn.particle,
                spawn.pos,
                spawn.velocity,
                camera_pos,
                registry,
                chunks,
                biome_climate,
            ) {
                tracing::debug!(particle = ?spawn.particle, "skipping invalid typed explosion particle option");
            }
        }
        self.tracked_explosions.clear();
    }

    /// Queue both explosion packet particle paths without altering the packet's
    /// typed option or weighted entries.
    pub(crate) fn queue_explosion_packet_particles(
        &mut self,
        explosion: &crate::net::ExplosionPayload,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) -> bool {
        let center = dvec3(explosion.center.x, explosion.center.y, explosion.center.z);
        let accepted = self.add_explosion_packet_particle(
            &explosion.explosion_particle,
            center,
            dvec3(1.0, 0.0, 0.0),
            camera_pos,
            registry,
            chunks,
            biome_climate,
        );
        self.track_explosion_effects(
            center,
            explosion.radius,
            explosion.block_count,
            explosion.block_particles.clone(),
        );
        accepted
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

    /// Vanilla `ClientLevel.addBreakingBlockEffect`: one terrain particle on
    /// the hit face, sampled from the union bounds of the block outline shape.
    pub fn add_breaking_block_effect(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        face: azalea_core::direction::Direction,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        if is_air(state)
            || crate::world::block::has_invisible_render_shape(state)
            || !crate::world::block::should_spawn_terrain_particles(state)
        {
            return;
        }

        let boxes = block_shape::outline_shape(state);
        let mut bounds = [0.0; 6];
        if let Some(first) = boxes.first() {
            bounds = *first;
            for b in &boxes[1..] {
                bounds[0] = bounds[0].min(b[0]);
                bounds[1] = bounds[1].min(b[1]);
                bounds[2] = bounds[2].min(b[2]);
                bounds[3] = bounds[3].max(b[3]);
                bounds[4] = bounds[4].max(b[4]);
                bounds[5] = bounds[5].max(b[5]);
            }
        }

        let p = breaking_effect_position(
            pos,
            bounds,
            face,
            [fastrand::f64(), fastrand::f64(), fastrand::f64()],
        );

        let faces = registry.get_textures(state);
        let mut color = [0.6f32; 3];
        if let Some(faces) = faces
            && faces.tint != Tint::None
            && block_id(state) != "grass_block"
        {
            let tint = match faces.tint {
                Tint::Redstone => crate::world::block::redstone_wire_rgb(state),
                Tint::Stem => crate::world::block::stem_rgb(state),
                tint => self.blend_tint(tint, pos, chunks, biome_climate),
            };
            for (c, t) in color.iter_mut().zip(tint) {
                *c *= t;
            }
        }
        let region = match faces {
            Some(faces) => self
                .uv_map
                .get_region(faces.particle.as_deref().unwrap_or(&faces.top)),
            None => self.uv_map.get_region(""),
        };
        let light = world_brightness(chunks, pos.x, pos.y, pos.z);
        let mut particle = Particle::terrain(p, DVec3::ZERO, region, color, light);
        particle.vel = terrain_power(particle.vel, 0.2f32);
        particle.kind = Kind::TerrainScaled;
        self.push(particle);
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
        kind: azalea_registry::builtin::ItemKind,
        registry: &BlockRegistry,
        eye_pos: DVec3,
        x_rot_deg: f32,
        y_rot_deg: f32,
        chunks: &ChunkStore,
    ) {
        let stack = azalea_inventory::ItemStack::new(kind, 1);
        let x_rot = -(x_rot_deg as f64).to_radians();
        let y_rot = -(y_rot_deg as f64).to_radians();
        let light = world_brightness(
            chunks,
            eye_pos.x.floor() as i32,
            eye_pos.y.floor() as i32,
            eye_pos.z.floor() as i32,
        );
        for _ in 0..count {
            if !self.accepts_normal_spawn(false, &mut || fastrand::u32(..)) {
                continue;
            }
            let region = self.item_particle_region(registry, &stack, None);
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
        if count == 0 {
            self.add_server_particle(
                kind,
                options,
                override_limiter,
                always_show,
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
                override_limiter,
                always_show,
                pos + scatter,
                vel,
                camera_pos,
                registry,
                chunks,
                biome_climate,
            );
        }
    }

    /// Shared endpoint for locally sampled world/entity requests (ambient,
    /// animate-tick, and entity particles). These requests retain the same
    /// ParticleStore world context as packet-driven item particles.
    pub(crate) fn add_particle_spawn_request(
        &mut self,
        request: crate::world::particle_tick::ParticleSpawnRequest,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        self.add_particles_from_packet(
            request.kind,
            request.options,
            false,
            request.always_visible,
            request.position,
            request.velocity,
            1.0,
            0,
            camera_pos,
            registry,
            chunks,
            biome_climate,
        );
    }

    /// Vanilla `ClientLevel.doAddParticle`: ordinary particles are culled at
    /// 32 blocks and filtered by the selected status; override-limiter and
    /// long-distance packets bypass both checks.
    fn add_server_particle(
        &mut self,
        kind: ServerParticleKind,
        options: ServerParticleOptions,
        bypass_limiter: bool,
        always_show: bool,
        pos: DVec3,
        vel: DVec3,
        camera_pos: DVec3,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        biome_climate: &HashMap<u32, BiomeClimate>,
    ) {
        let bypass_limiter = bypass_limiter || kind.override_limiter();
        if !accept_particle(
            self.mode,
            bypass_limiter,
            always_show,
            camera_pos.distance_squared(pos),
            &mut || fastrand::u32(..),
        ) {
            return;
        }
        match kind {
            ServerParticleKind::Crit | ServerParticleKind::EnchantedHit => {
                let name = if kind == ServerParticleKind::Crit {
                    "minecraft:crit"
                } else {
                    "minecraft:enchanted_hit"
                };
                let sprite = descriptor_frame(&self.uv_map, name, 0)
                    .unwrap_or_else(|| self.uv_map.missing_region());
                self.push(Particle::crit(
                    pos,
                    vel,
                    kind == ServerParticleKind::EnchantedHit,
                    sprite,
                    chunks,
                ));
            }
            ServerParticleKind::EndRod => {
                self.push(Particle::end_rod(pos, vel, &self.end_rod_frames));
            }
            ServerParticleKind::EntityEffect => {
                if let ServerParticleOptions::EntityEffect { color } = options {
                    self.push(Particle::entity_effect(
                        pos,
                        vel,
                        color,
                        &self.entity_effect_frames,
                    ));
                }
            }
            ServerParticleKind::Effect | ServerParticleKind::InstantEffect => {
                let ServerParticleOptions::Spell { color, power } = options else {
                    return;
                };
                let instant = matches!(kind, ServerParticleKind::InstantEffect);
                let frames = if instant {
                    &self.spell_frames
                } else {
                    &self.entity_effect_frames
                };
                let mut particle = if instant {
                    Particle::spell_effect(
                        pos,
                        vel,
                        0xff00_0000 | (color as u32 & 0x00ff_ffff),
                        frames,
                    )
                } else {
                    Particle::entity_effect(
                        pos,
                        vel,
                        0xff00_0000 | (color as u32 & 0x00ff_ffff),
                        frames,
                    )
                };
                let power = if power.is_finite() { power } else { 1.0 };
                particle.vel = dvec3(
                    particle.vel.x * f64::from(power),
                    (particle.vel.y - 0.1) * f64::from(power) + 0.1,
                    particle.vel.z * f64::from(power),
                );
                self.push(particle);
            }
            ServerParticleKind::Witch => {
                let mut particle = Particle::spell_particle(
                    Kind::SpellEffect,
                    pos,
                    vel,
                    [1.0; 3],
                    1.0,
                    &self.spell_frames,
                );
                let brightness = spell_brightness(fastrand::f32());
                particle.color = [brightness, 0.0, brightness];
                self.push(particle);
            }
            ServerParticleKind::TrialOmen | ServerParticleKind::RaidOmen => {
                let sprite = if matches!(kind, ServerParticleKind::TrialOmen) {
                    self.trial_omen_sprite
                } else {
                    self.raid_omen_sprite
                };
                let mut particle = Particle::entity_effect(pos, vel, 0xffff_ffff, &[sprite; 8]);
                particle.kind = Kind::FixedEffect;
                self.push(particle);
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
            ServerParticleKind::CampfireCosySmoke | ServerParticleKind::CampfireSignalSmoke => {
                let signal = matches!(kind, ServerParticleKind::CampfireSignalSmoke);
                let frames = if signal {
                    &self.campfire_signal_frames
                } else {
                    &self.campfire_cosy_frames
                };
                self.push(Particle::campfire_smoke(
                    pos,
                    signal,
                    frames,
                    &mut fastrand::Rng::new(),
                ));
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
                self.push(Particle::shriek(pos, delay, self.generic_frames[9]));
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
                    self.generic_frames[10],
                ));
            }
            ServerParticleKind::Vibration => {
                let sprite = self.generic_frames[11];
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
                    raw_components,
                } = options
                else {
                    return;
                };
                let Some(registry_name) = pomme_protocol::registries::RegistryTable::native()
                    .name_of(pomme_protocol::registries::ClientRegistry::Item, item_id)
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
                if crate::player::inventory::item_resource_name(kind) != registry_name {
                    return;
                }
                if count <= 0 {
                    return;
                }
                let stack = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData {
                    kind,
                    count,
                    component_patch: components,
                });
                let sprite = self.item_particle_region(registry, &stack, raw_components.as_deref());
                self.push(Particle::breaking_item(
                    pos,
                    vel,
                    sprite,
                    world_brightness(
                        chunks,
                        pos.x.floor() as i32,
                        pos.y.floor() as i32,
                        pos.z.floor() as i32,
                    ),
                ));
            }
            ServerParticleKind::Dust | ServerParticleKind::DustColorTransition => {
                let (from, to, scale) = match options {
                    ServerParticleOptions::Dust {
                        packed_color,
                        scale,
                    } => (packed_color, None, scale),
                    ServerParticleOptions::DustColorTransition {
                        from_color,
                        to_color,
                        scale,
                    } => (from_color, Some(to_color), scale),
                    _ => return,
                };
                let rgb = |packed: i32| {
                    let packed = packed as u32;
                    [
                        ((packed >> 16) & 0xff) as f32 / 255.0,
                        ((packed >> 8) & 0xff) as f32 / 255.0,
                        (packed & 0xff) as f32 / 255.0,
                    ]
                };
                let mut rng = fastrand::Rng::new();
                self.push(Particle::dust_with_transition(
                    pos,
                    vel,
                    rgb(from),
                    to.map(rgb),
                    scale,
                    descriptor_frame(
                        &self.uv_map,
                        if to.is_some() {
                            "minecraft:dust_color_transition"
                        } else {
                            "minecraft:dust"
                        },
                        0,
                    )
                    .unwrap_or_else(|| self.uv_map.get_region(DUST_SPRITE)),
                    &mut rng,
                ));
            }
            // Family modules own provider behavior; keep this shared dispatch narrow.
            _ if atmosphere::supports(kind) => {
                let _ = atmosphere::spawn(
                    self,
                    kind,
                    options,
                    pos,
                    vel,
                    registry,
                    chunks,
                    biome_climate,
                );
            }
            _ if water::supports(kind) => {
                let _ = water::spawn(
                    self,
                    kind,
                    options,
                    pos,
                    vel,
                    registry,
                    chunks,
                    biome_climate,
                );
            }
            _ if magic::supports(kind) => {
                let _ = magic::spawn(
                    self,
                    kind,
                    options,
                    pos,
                    vel,
                    registry,
                    chunks,
                    biome_climate,
                );
            }
            _ if terrain_extra::supports(kind) => {
                let _ = terrain_extra::spawn(
                    self,
                    kind,
                    options,
                    pos,
                    vel,
                    registry,
                    chunks,
                    biome_climate,
                );
            }
            _ if emitters::supports(kind) => {
                let _ = emitters::spawn(
                    self,
                    kind,
                    options,
                    pos,
                    vel,
                    registry,
                    chunks,
                    biome_climate,
                );
            }
            // No generic substitute: unsupported providers remain invisible.
            _ => {}
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

    #[cfg(test)]
    pub(crate) fn pending_terrain_counts_for_test(&self) -> (usize, usize) {
        let particles = self.pending.iter().chain(&self.particles);
        let mut total = 0;
        let mut breaking = 0;
        for particle in particles {
            total += 1;
            breaking += usize::from(particle.kind == Kind::TerrainScaled);
        }
        (total, breaking)
    }

    pub(crate) fn clear(&mut self) {
        self.sound_requests.clear();
        self.particles.clear();
        self.pending.clear();
        self.emitters.clear();
        self.pending_emitters.clear();
        self.firework_starters.clear();
        self.pending_firework_starters.clear();
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
        self.firework_starters
            .append(&mut self.pending_firework_starters);
        let mut firework_sparks = Vec::new();
        let mut firework_flashes = Vec::new();
        for starter in &mut self.firework_starters {
            if starter.life == 0 {
                let far = starter.request.far_effect;
                let large = starter.request.explosions.len() >= 3
                    || starter.request.explosions.iter().any(|e| {
                        e.shape == azalea_inventory::components::FireworkExplosionShape::LargeBall
                    });
                let event = match (large, far) {
                    (true, true) => "entity.firework_rocket.large_blast_far",
                    (true, false) => "entity.firework_rocket.large_blast",
                    (false, true) => "entity.firework_rocket.blast_far",
                    (false, false) => "entity.firework_rocket.blast",
                };
                self.sound_requests.push(ParticleSoundRequest {
                    event,
                    pos: starter.request.position,
                    volume: 20.0,
                    pitch: 0.95 + fastrand::f32() * 0.1,
                    seed: fastrand::u64(..),
                });
            }
            if starter.life % 2 == 0 {
                let index = starter.life / 2;
                if let Some(explosion) = starter.request.explosions.get(index) {
                    let colors = if explosion.colors.is_empty() {
                        &[0x1e1b1b][..]
                    } else {
                        explosion.colors.as_slice()
                    };
                    firework_flashes.push((starter.request.position, colors[0]));
                    firework_sparks.extend(firework_explosion_sparks(
                        &self.uv_map,
                        &starter.request,
                        explosion,
                    ));
                }
            }
            starter.life += 1;
            if starter.life > starter.lifetime && starter.twinkle_delay {
                let far = starter.request.far_effect;
                self.sound_requests.push(ParticleSoundRequest {
                    event: if far {
                        "entity.firework_rocket.twinkle_far"
                    } else {
                        "entity.firework_rocket.twinkle"
                    },
                    pos: starter.request.position,
                    volume: 20.0,
                    pitch: 0.9 + fastrand::f32() * 0.15,
                    seed: fastrand::u64(..),
                });
                starter.twinkle_delay = false;
            }
        }
        self.firework_starters.retain(|s| s.life <= s.lifetime);
        for particle in firework_sparks {
            self.push(particle);
        }
        for (pos, color) in firework_flashes {
            if let Some(particle) = magic::firework_flash(&self.uv_map, pos, color) {
                self.push(particle);
            }
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
            self.add_explosion_particle(&ParticleOptions::Explosion, pos, velocity);
        }
        let end_frames = self.end_rod_frames;
        let generic_frames = self.generic_frames;
        let effect_frames = self.entity_effect_frames;
        let spell_frames = self.spell_frames;
        let explosion_frames = self.explosion_frames;
        let atlas = &self.uv_map;
        let mut family_children = Vec::new();
        let mut family_sounds = Vec::new();
        self.particles.retain_mut(|p| {
            let alive = match &p.kind {
                Kind::Atmosphere(_) => atmosphere::tick(p, chunks, atlas, &mut family_children),
                Kind::Water(_) => {
                    water::tick(p, chunks, atlas, &mut family_children, &mut family_sounds)
                }
                Kind::Magic(_) => magic::tick(p, chunks, atlas, &mut family_children),
                Kind::TerrainExtra(_) => {
                    terrain_extra::tick(p, chunks, atlas, &mut family_children)
                }
                Kind::Emitters(_) => emitters::tick(p, chunks, atlas, &mut family_children),
                _ => p.tick_with_entity_lookup(
                    chunks,
                    &end_frames,
                    &generic_frames,
                    &explosion_frames,
                    &mut lookup,
                ),
            };
            if alive && matches!(p.kind, Kind::EntityEffect | Kind::SpellEffect) {
                let frames = if p.kind == Kind::SpellEffect {
                    &spell_frames
                } else {
                    &effect_frames
                };
                p.set_sprite(&frames[animated_frame_index(p.age, p.lifetime, 8)]);
            }
            alive
        });
        // Child particles join only after the live iteration and bypass spawn-mode
        // filtering.
        self.particles.extend(family_children);
        self.particles.append(&mut self.pending);
        self.sound_requests.extend(family_sounds);
    }

    /// Apply Java's player attraction after family ticks to surviving
    /// particles. Only the atmosphere provider owns this post-tick
    /// transform.
    pub(crate) fn apply_player_attraction(&mut self, players: &[(DVec3, DVec3)]) {
        for particle in &mut self.particles {
            if matches!(particle.kind, Kind::Atmosphere(_)) {
                atmosphere::apply_player_attraction(particle, players);
            }
        }
    }

    /// Non-quad renderer work requested by active emitter particles.
    pub(crate) fn model_render_requests(&self, partial_tick: f32) -> Vec<emitters::RenderRequest> {
        self.particles
            .iter()
            .filter_map(|particle| emitters::render_request(particle, partial_tick))
            .collect()
    }

    /// Quad positions are anchor-relative, subtracted in f64 (see
    /// `Camera::anchor`).
    pub fn extract(
        &self,
        partial_tick: f32,
        anchor: DVec3,
        chunks: &ChunkStore,
    ) -> Vec<ParticleQuad> {
        self.particles
            .iter()
            .flat_map(|p| {
                if p.kind == Kind::Shriek && p.delay > 0 {
                    return Vec::new();
                }
                let family_appearance = match &p.kind {
                    Kind::Atmosphere(_) => atmosphere::appearance(p, partial_tick),
                    Kind::Water(_) => water::appearance(p, partial_tick),
                    Kind::Magic(_) => magic::appearance(p, partial_tick),
                    Kind::TerrainExtra(_) => terrain_extra::appearance(p, partial_tick),
                    Kind::Emitters(_) => emitters::appearance(p, partial_tick),
                    _ => None,
                };
                let is_family_particle = matches!(
                    &p.kind,
                    Kind::Atmosphere(_)
                        | Kind::Water(_)
                        | Kind::Magic(_)
                        | Kind::TerrainExtra(_)
                        | Kind::Emitters(_)
                );
                if is_family_particle && family_appearance.is_none() {
                    return Vec::new();
                }
                let world_pos = p.prev_pos.lerp(p.pos, partial_tick as f64);
                let pos = (world_pos - anchor).as_vec3();
                let sky = chunks.get_sky_light(
                    world_pos.x.floor() as i32,
                    world_pos.y.floor() as i32,
                    world_pos.z.floor() as i32,
                );
                let block_pos = [
                    world_pos.x.floor() as i32,
                    world_pos.y.floor() as i32,
                    world_pos.z.floor() as i32,
                ];
                let block = chunks.get_block_light(block_pos[0], block_pos[1], block_pos[2]);
                let light_uv =
                    particle_light_uv(&p.kind, sky, block, p.age, p.lifetime, partial_tick);
                // Java ARGB.colorFromFloat quantizes the base color before the
                // vertex shader multiplies by the sampled RGBA8 lightmap.
                let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).floor() as u8;
                let t = (p.age as f32 + partial_tick) / p.lifetime as f32;
                let size = family_appearance.map_or_else(
                    || match p.kind {
                        Kind::Dust | Kind::DustColorTransition => {
                            dust_quad_size(p.base_size, p.age, p.lifetime, partial_tick)
                        }
                        Kind::Shriek => p.size * (t * 0.75).clamp(0.0, 1.0),
                        _ => p.size,
                    },
                    |a| a.size,
                );
                let alpha = family_appearance.map_or_else(
                    || {
                        if p.kind == Kind::Shriek {
                            1.0 - t.clamp(0.0, 1.0)
                        } else {
                            p.alpha
                        }
                    },
                    |a| a.alpha,
                );
                let sway =
                    ((p.age as f32 + partial_tick - std::f32::consts::TAU) * 0.05).sin() * 2.0;
                let rot = p.rot_o + (p.rot - p.rot_o) * partial_tick;
                let pitch = p.pitch_o + (p.pitch - p.pitch_o) * partial_tick;
                let primary = family_appearance.map_or_else(
                    || {
                        if p.kind == Kind::Vibration {
                            Quat::from_rotation_y(rot)
                                * Quat::from_rotation_x(-pitch - std::f32::consts::FRAC_PI_2)
                                * Quat::from_rotation_y(sway)
                        } else {
                            p.rotation
                        }
                    },
                    |a| a.rotation,
                );
                let particle_color = family_appearance.map_or_else(
                    || {
                        if p.kind == Kind::DustColorTransition {
                            let end = p.target.unwrap_or(DVec3::ZERO).as_vec3().to_array();
                            dust_transition_color(p.color, end, p.age, p.lifetime, partial_tick)
                        } else {
                            p.color
                        }
                    },
                    |a| a.color,
                );
                let look_at_y = p.kind.look_at_y();
                let quad = |rotation: Quat| ParticleQuad {
                    pos: pos.into(),
                    size,
                    u0: p.u0,
                    u1: p.u1,
                    v0: p.v0,
                    v1: p.v1,
                    color: u32::from_le_bytes([
                        channel(particle_color[0]),
                        channel(particle_color[1]),
                        channel(particle_color[2]),
                        channel(alpha),
                    ]),
                    light_uv,
                    translucent: p.kind.translucent(),
                    look_at_y,
                    rotation: rotation.to_array(),
                };
                let second = family_appearance.map_or_else(
                    || {
                        p.second_rotation.or_else(|| {
                            (p.kind == Kind::Vibration).then(|| {
                                Quat::from_rotation_y(-std::f32::consts::PI + rot)
                                    * Quat::from_rotation_x(pitch + std::f32::consts::FRAC_PI_2)
                                    * Quat::from_rotation_y(sway)
                            })
                        })
                    },
                    |a| a.second_rotation,
                );
                let mut quads = vec![quad(primary)];
                if let Some(rotation) = second {
                    quads.push(quad(rotation));
                }
                quads
            })
            .collect()
    }
}

fn descriptor_frames<const N: usize>(
    atlas: &AtlasUVMap,
    particle: &str,
    fallback: [&str; N],
) -> [AtlasRegion; N] {
    std::array::from_fn(|index| {
        descriptor_frame(atlas, particle, index)
            .unwrap_or_else(|| atlas.get_region(fallback[index]))
    })
}

/// Look up a descriptor frame without allocating its ordered frame list.
pub(super) fn descriptor_frame(
    atlas: &AtlasUVMap,
    particle: &str,
    index: usize,
) -> Option<AtlasRegion> {
    let names = atlas.particle_sprite_names(particle)?;
    names.get(index).map(|name| atlas.get_region(name))
}

pub(super) fn descriptor_age_frame(age: i32, lifetime: i32, frame_count: usize) -> usize {
    if frame_count == 0 {
        return 0;
    }
    ((age.max(0) as usize * frame_count) / lifetime.max(1) as usize).min(frame_count - 1)
}

pub(super) fn descriptor_random_frame(frame_count: usize) -> usize {
    if frame_count == 0 {
        0
    } else {
        fastrand::usize(0..frame_count)
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
    use azalea_buf::AzBuf;

    use super::{
        AtlasRegion, AtlasUVMap, CAMPFIRE_COSY_SMOKE_SPRITES, CAMPFIRE_SIGNAL_SMOKE_SPRITES,
        END_ROD_SPRITES, EXPLOSION_SPRITES, ExplosionParticleInfo, GENERIC_PARTICLE_SPRITES, Kind,
        Particle, ParticleOptions, ServerParticleKind, TrackedExplosion, Weighted,
        animated_frame_index, dust_quad_size, dust_transition_color, dvec3,
        explosion_emitter_child, explosion_frame_index, packet_particle_count,
        plan_explosion_particles,
    };
    use crate::world::chunk::ChunkStore;

    #[test]
    fn breaking_block_effect_uses_hit_face_outline_bounds_and_real_terrain_quad() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut atlas = AtlasUVMap::test_empty();
        atlas.test_insert_region(
            "fixture/block_particle",
            AtlasRegion {
                u_min: 0.2,
                v_min: 0.3,
                u_max: 0.4,
                v_max: 0.5,
                pixel_rect: [1, 1, 2, 2],
                sprite: 1,
                opaque: true,
                translucent: false,
                alpha_counts: [0, 0, 4],
            },
        );
        let mut store = super::ParticleStore::new(atlas, colors.clone(), colors.clone(), colors);
        let mut registry = crate::world::block::registry::BlockRegistry::test_empty();
        registry.test_add_particle_fixture();
        let chunks = ChunkStore::new(2);
        let stone = crate::world::block::default_state_of("stone").unwrap();
        let origin = azalea_core::position::BlockPos::new(4, 20, -3);
        let cases = [
            (
                azalea_core::direction::Direction::Down,
                20.0 - 0.1f32 as f64,
                0,
            ),
            (
                azalea_core::direction::Direction::Up,
                21.0 + 0.1f32 as f64,
                1,
            ),
            (
                azalea_core::direction::Direction::North,
                -3.0 - 0.1f32 as f64,
                2,
            ),
            (
                azalea_core::direction::Direction::South,
                -2.0 + 0.1f32 as f64,
                3,
            ),
            (
                azalea_core::direction::Direction::West,
                4.0 - 0.1f32 as f64,
                4,
            ),
            (
                azalea_core::direction::Direction::East,
                5.0 + 0.1f32 as f64,
                5,
            ),
        ];
        for (face, expected, axis) in cases {
            store.clear();
            store.add_breaking_block_effect(
                origin,
                stone,
                face,
                &registry,
                &chunks,
                &std::collections::HashMap::new(),
            );
            assert_eq!(store.pending.len(), 1, "{face:?}");
            let particle = &store.pending[0];
            assert!(particle.kind == super::Kind::TerrainScaled);
            assert_eq!(
                particle.light,
                super::world_brightness(&chunks, origin.x, origin.y, origin.z)
            );
            assert_eq!(particle.color, [0.6; 3]);
            assert_eq!(
                super::collision_half_width(&particle.kind),
                f64::from(0.2f32 * 0.6f32) * 0.5
            );
            assert!(particle.vel.x.abs() <= 0.036 && particle.vel.z.abs() <= 0.036);
            assert!((0.06..=0.14).contains(&particle.vel.y));
            match axis {
                0 | 1 => {
                    assert!((particle.pos.y - expected).abs() < 1e-12);
                    assert!((4.1..=4.9).contains(&particle.pos.x));
                    assert!((-2.9..=-2.1).contains(&particle.pos.z));
                }
                2 | 3 => {
                    assert!((particle.pos.z - expected).abs() < 1e-12);
                    assert!((4.1..=4.9).contains(&particle.pos.x));
                    assert!((20.1..=20.9).contains(&particle.pos.y));
                }
                4 | 5 => {
                    assert!((particle.pos.x - expected).abs() < 1e-12);
                    assert!((20.1..=20.9).contains(&particle.pos.y));
                    assert!((-2.9..=-2.1).contains(&particle.pos.z));
                }
                _ => unreachable!(),
            }
            let quad_size = particle.size;
            assert!((0.05..=0.1).contains(&quad_size));
            store.particles.append(&mut store.pending);
            let quads = store.extract(0.0, glam::DVec3::ZERO, &chunks);
            assert_eq!(quads.len(), 1, "face {face:?} must reach the quad path");
            assert!((quads[0].size - quad_size).abs() < 1e-6);
            assert!((0.2..=0.4).contains(&quads[0].u0));
            assert!((0.2..=0.4).contains(&quads[0].u1));
            assert!((0.3..=0.5).contains(&quads[0].v0));
            assert!((0.3..=0.5).contains(&quads[0].v1));
        }

        let slab = crate::world::block::default_state_of("oak_slab").unwrap();
        store.clear();
        store.add_breaking_block_effect(
            origin,
            slab,
            azalea_core::direction::Direction::Up,
            &registry,
            &chunks,
            &std::collections::HashMap::new(),
        );
        assert!((store.pending[0].pos.y - (20.5 + 0.1f32 as f64)).abs() < 1e-12);
    }

    #[test]
    fn breaking_block_effect_respects_air_invisible_and_terrain_particle_opt_out() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let registry = crate::world::block::registry::BlockRegistry::test_empty();
        let chunks = ChunkStore::new(2);
        let pos = azalea_core::position::BlockPos::new(0, 64, 0);
        for name in ["air", "barrier", "structure_void", "moving_piston", "water"] {
            let state = crate::world::block::default_state_of(name).unwrap();
            store.add_breaking_block_effect(
                pos,
                state,
                azalea_core::direction::Direction::Up,
                &registry,
                &chunks,
                &std::collections::HashMap::new(),
            );
            assert!(
                store.pending.is_empty(),
                "unexpected terrain particle for {name}"
            );
        }
    }

    #[test]
    fn breaking_particle_power_uses_java_vertical_bias_formula() {
        let origin = azalea_core::position::BlockPos::new(4, 20, -3);
        let tiny = [0.4, 0.4, 0.4, 0.45, 0.45, 0.45];
        let tiny_up = super::breaking_effect_position(
            origin,
            tiny,
            azalea_core::direction::Direction::Up,
            [0.0; 3],
        );
        let tiny_up_expected = glam::DVec3::new(4.5, 20.45 + 0.1f32 as f64, -2.5);
        assert!((tiny_up - tiny_up_expected).length() < 3.0e-9);
        let tiny_down = super::breaking_effect_position(
            origin,
            tiny,
            azalea_core::direction::Direction::Down,
            [0.0; 3],
        );
        assert!((tiny_down.y - (20.4 - 0.1f32 as f64)).abs() < 1.0e-12);

        let velocity = glam::DVec3::new(0.25, 0.4, -0.5);
        let y_offset = f64::from(0.1f32);
        let power = f64::from(0.2f32);
        assert_eq!(
            super::terrain_power(velocity, 0.2f32),
            glam::DVec3::new(
                velocity.x * power,
                (velocity.y - y_offset) * power + y_offset,
                velocity.z * power,
            )
        );
    }

    #[test]
    fn java_provider_light_uvs_preserve_sky_emission_and_firefly_bytes() {
        let kind = ServerParticleKind::Flame;
        assert_eq!(
            super::provider_light_uv(kind, 12, 3, 5, 10, 0.0),
            Some(168 | (192 << 8))
        );
        assert_eq!(
            super::provider_light_uv(kind, 12, 15, 10, 10, 0.0),
            Some(240 | (192 << 8))
        );
        assert_eq!(
            super::provider_light_uv(ServerParticleKind::SculkSoul, 9, 2, 0, 10, 0.0),
            Some(240 | (144 << 8))
        );
        assert_eq!(
            super::provider_light_uv(ServerParticleKind::Firefly, 9, 2, 10, 100, 0.0),
            Some(84)
        );
        assert_eq!(
            super::provider_light_uv(ServerParticleKind::Firefly, 9, 2, 50, 100, 0.0),
            Some(255)
        );
        assert_eq!(
            super::provider_light_uv(ServerParticleKind::Crit, 9, 2, 0, 10, 0.0),
            None
        );
        assert_eq!(
            super::particle_light_uv(&super::Kind::EndRod, 2, 1, 0, 10, 0.0),
            0xF0F0
        );
        assert_eq!(
            super::particle_light_uv(&super::Kind::Vibration, 9, 2, 0, 10, 0.0),
            240 | (144 << 8)
        );
    }

    #[test]
    fn only_java_trial_spawner_detection_particles_use_look_at_y() {
        assert!(super::look_at_y(ServerParticleKind::TrialSpawnerDetection));
        assert!(super::look_at_y(
            ServerParticleKind::TrialSpawnerDetectionOminous
        ));
        assert!(!super::look_at_y(ServerParticleKind::VaultConnection));
        assert!(!super::look_at_y(ServerParticleKind::OminousSpawning));
        assert!(!super::look_at_y(ServerParticleKind::SculkCharge));
    }

    #[test]
    fn all_104_remaining_provider_kinds_have_one_family_owner() {
        let names = r#"
            cloud copper_fire_flame flame soul_fire_flame small_flame large_smoke white_smoke sneeze
            ash white_ash crimson_spore warped_spore spore_blossom_air falling_spore_blossom mycelium
            underwater firefly cherry_leaves pale_oak_leaves tinted_leaves angry_villager happy_villager
            composter heart snowflake noxious_gas sulfur_cube_goo dragon_breath infested soul sculk_soul
            bubble sulfur_bubbles fishing rain splash bubble_pop current_down bubble_column_up nautilus
            dolphin squid_ink glow_squid_ink lava dripping_lava falling_lava landing_lava dripping_water
            falling_water dripping_honey falling_honey landing_honey falling_nectar dripping_obsidian_tear
            falling_obsidian_tear landing_obsidian_tear dripping_dripstone_lava falling_dripstone_lava
            dripping_dripstone_water falling_dripstone_water spit crit enchanted_hit damage_indicator enchant
            note portal reverse_portal glow wax_on wax_off electric_spark scrape egg_crack dust_plume
            trial_spawner_detection trial_spawner_detection_ominous vault_connection ominous_spawning
            pause_mob_growth reset_mob_growth firework flash sculk_charge sculk_charge_pop sonic_boom
            sweep_attack gust small_gust block_marker falling_dust dust_pillar block_crumble item_slime
            item_cobweb item_snowball geyser geyser_base geyser_poof geyser_plume noxious_gas_cloud
            gust_emitter_large gust_emitter_small elder_guardian
        "#;
        let names: Vec<_> = names.split_whitespace().collect();
        let unique: std::collections::HashSet<_> = names.iter().copied().collect();
        assert_eq!(names.len(), 104);
        assert_eq!(
            unique.len(),
            104,
            "family partition contains duplicate names"
        );

        for name in names {
            let kind = ServerParticleKind::from_name(name).expect("native particle name");
            let owners = [
                super::atmosphere::supports(kind),
                super::water::supports(kind),
                super::magic::supports(kind),
                super::terrain_extra::supports(kind),
                super::emitters::supports(kind),
            ]
            .into_iter()
            .filter(|supported| *supported)
            .count();
            assert_eq!(owners, 1, "{name} must have exactly one family owner");
        }

        assert_eq!(names_in_family(super::atmosphere::supports), 31);
        assert_eq!(names_in_family(super::water::supports), 30);
        assert_eq!(names_in_family(super::magic::supports), 28);
        assert_eq!(names_in_family(super::terrain_extra::supports), 7);
        assert_eq!(names_in_family(super::emitters::supports), 8);
    }

    fn names_in_family(supports: impl Fn(ServerParticleKind) -> bool) -> usize {
        let mut count = 0;
        for id in 0..125 {
            if let Some(kind) = ServerParticleKind::from_id(id) {
                count += usize::from(supports(kind));
            }
        }
        count
    }

    fn firework_test_atlas() -> AtlasUVMap {
        let mut atlas = AtlasUVMap::test_empty();
        atlas.test_insert_particle_sprites("firework", vec!["particle/firework".to_owned()]);
        atlas.test_insert_particle_sprites("flash", vec!["particle/flash".to_owned()]);
        atlas
    }

    fn test_firework(
        shape: azalea_inventory::components::FireworkExplosionShape,
    ) -> azalea_inventory::components::FireworkExplosion {
        azalea_inventory::components::FireworkExplosion {
            shape,
            colors: vec![0x123456, 0xabcdef],
            fade_colors: vec![0x654321],
            has_trail: true,
            has_twinkle: true,
        }
    }

    #[test]
    fn firework_shapes_create_java_spark_counts_and_velocity_families() {
        use azalea_inventory::components::FireworkExplosionShape as S;
        let atlas = firework_test_atlas();
        let request = super::FireworkStarterRequest {
            position: dvec3(12.0, 30.0, -7.0),
            velocity: dvec3(0.2, 0.4, -0.1),
            far_effect: false,
            explosions: Vec::new(),
        };
        for (shape, expected) in [
            (S::SmallBall, 98),
            (S::LargeBall, 386),
            (S::Star, 121),
            (S::Creeper, 265),
            (S::Burst, 70),
        ] {
            fastrand::seed(
                262 + match shape {
                    S::SmallBall => 0,
                    S::LargeBall => 1,
                    S::Star => 2,
                    S::Creeper => 3,
                    S::Burst => 4,
                },
            );
            let sparks = super::firework_explosion_sparks(&atlas, &request, &test_firework(shape));
            assert_eq!(sparks.len(), expected, "{shape:?}");
            assert!(sparks.iter().all(|p| p.pos == request.position));
            assert!(sparks.iter().all(|p| p.vel.is_finite()));
            match shape {
                S::SmallBall | S::LargeBall => {
                    assert!(
                        sparks
                            .iter()
                            .all(|p| (0.15..0.65).contains(&p.vel.length()))
                    );
                }
                S::Star | S::Creeper => {
                    assert!(sparks.iter().all(|p| p.vel.length() <= 0.51), "{shape:?}");
                }
                S::Burst => {
                    assert!(sparks.iter().all(|p| p.vel.y >= 0.2 && p.vel.y < 0.7));
                }
            }
            assert!(sparks.iter().all(|p| matches!(p.kind, super::Kind::Magic(state)
                if state.firework_trail && state.firework_twinkle && state.firework_fade.is_some())));
        }
    }

    #[test]
    fn firework_starter_sequences_multiple_explosions_and_survives_entity_removal() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        use azalea_inventory::components::FireworkExplosionShape as S;
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            firework_test_atlas(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let pos = dvec3(12.0, 30.0, -7.0);
        let mut rocket = crate::entity::EntityStore::new();
        let rocket_position = crate::entity::components::Position::new(pos.x, pos.y, pos.z);
        rocket.set_vehicle_transform(7, rocket_position, dvec3(0.0, 0.2, 0.0));
        rocket.set_vehicle_kind(7, azalea_registry::builtin::EntityKind::FireworkRocket);
        let snapshot_position = glam::DVec3::from(rocket.vehicles[&7].position);
        let snapshot_velocity = rocket.vehicles[&7].velocity;
        rocket.remove_entity(7);
        let mut first = test_firework(S::SmallBall);
        first.has_trail = false;
        first.has_twinkle = false;
        let mut second = test_firework(S::Burst);
        second.has_trail = false;
        let request = super::FireworkStarterRequest {
            position: snapshot_position,
            velocity: snapshot_velocity,
            far_effect: true,
            explosions: vec![first, second],
        };
        assert!(store.add_firework_starter(request));
        store.tick(&ChunkStore::new(1));
        assert_eq!(store.particles.len(), 99); // 98 sparks + flash
        assert_eq!(store.particles.iter().filter(|p| p.pos == pos).count(), 99);
        store.tick(&ChunkStore::new(1));
        assert_eq!(store.particles.len(), 99);
        store.tick(&ChunkStore::new(1));
        assert_eq!(store.particles.len(), 170); // +70 sparks + flash
        assert_eq!(store.firework_starters[0].life, 3);
        assert!(
            store
                .drain_sound_requests()
                .iter()
                .any(|s| { s.event == "entity.firework_rocket.blast_far" && s.pos == pos })
        );
        for _ in 0..16 {
            store.tick(&ChunkStore::new(1));
        }
        assert!(
            store
                .drain_sound_requests()
                .iter()
                .any(|s| { s.event == "entity.firework_rocket.twinkle_far" && s.pos == pos })
        );
    }

    #[test]
    fn server_firework_particle_stays_plain_and_separate_from_rocket_starter() {
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            firework_test_atlas(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        assert!(super::magic::spawn(
            &mut store,
            super::ServerParticleKind::Firework,
            super::ServerParticleOptions::Simple,
            dvec3(1.0, 2.0, 3.0),
            glam::DVec3::Y,
            &crate::world::block::registry::BlockRegistry::test_empty(),
            &ChunkStore::new(1),
            &std::collections::HashMap::new(),
        ));
        assert_eq!(store.pending.len(), 1);
        assert!(matches!(store.pending[0].kind, super::Kind::Magic(state)
            if !state.firework_trail && !state.firework_twinkle && state.firework_fade.is_none()));
    }

    #[test]
    fn firework_spark_trail_inherits_fade_twinkle_and_color() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let atlas = firework_test_atlas();
        let mut particle = super::magic::firework_spark(
            &atlas,
            dvec3(1.0, 2.0, 3.0),
            glam::DVec3::ZERO,
            [0.2, 0.4, 0.6],
            Some([0.8, 0.7, 0.6]),
            true,
            true,
        )
        .unwrap();
        particle.age = 1;
        particle.lifetime = 48;
        let mut children = Vec::new();
        assert!(super::magic::tick(
            &mut particle,
            &ChunkStore::new(1),
            &atlas,
            &mut children,
        ));
        let [child] = children.as_slice() else {
            panic!("trail child emitted on Java's even-age cadence")
        };
        assert_eq!(child.pos, particle.pos);
        assert_eq!(child.color, [0.2, 0.4, 0.6]);
        assert_eq!(child.age, child.lifetime / 2);
        assert!(matches!(child.kind, super::Kind::Magic(state)
            if !state.firework_trail && state.firework_twinkle && state.firework_fade.is_some()));
    }

    #[test]
    fn living_effect_metadata_spawns_colored_particle() {
        use std::sync::Arc;

        use azalea_core::color::RgbColor;

        use crate::renderer::chunk::mesher::Colormap;

        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let packed_color = 0x2612_3456u32.to_be_bytes();
        let metadata_color =
            RgbColor::azalea_read(&mut std::io::Cursor::new(packed_color.as_slice())).unwrap();
        let metadata = vec![
            ParticleOptions::EntityEffect(azalea_entity::particle::ColorParticle {
                color: metadata_color,
            });
            75
        ];
        for seed in 0..256 {
            fastrand::seed(seed);
            store.add_living_effect_particles(
                dvec3(0.0, 0.0, 0.0),
                0.6,
                1.8,
                &metadata,
                true,
                false,
                dvec3(0.0, 0.0, 0.0),
            );
            assert!(
                store.pending.len() <= 1,
                "living update emits at most one particle"
            );
            if !store.pending.is_empty() {
                break;
            }
        }
        assert!(!store.pending.is_empty());
        assert!(store.pending.iter().all(|p| p.alpha == 38.0 / 255.0));
        let p = &store.pending[0];
        assert_eq!(
            p.color,
            [
                0x12 as f32 / 255.0,
                0x34 as f32 / 255.0,
                0x56 as f32 / 255.0
            ]
        );
        assert_eq!(p.alpha, 38.0 / 255.0);
        assert_eq!((p.gravity, p.friction), (-0.1, 0.96));
        assert!((0.075..=0.15).contains(&p.size));
        assert!(matches!(p.kind, super::Kind::EntityEffect));
        let spawned = store.pending.len();
        let chunks = ChunkStore::new(2);
        store.tick(&chunks);
        let quad = store.extract(0.0, dvec3(0.0, 0.0, 0.0), &chunks);
        assert_eq!(quad.len(), spawned);
        assert!(quad.iter().all(|q| q.translucent && q.color >> 24 == 38));
    }

    #[test]
    fn living_spell_power_scales_effect_and_instant_effect_velocities() {
        use azalea_entity::particle::ColorPowerParticle;

        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let options = [
            ParticleOptions::Effect(ColorPowerParticle {
                color: 0x123456,
                power: 1.0,
            }),
            ParticleOptions::InstantEffect(ColorPowerParticle {
                color: 0x123456,
                power: 1.0,
            }),
        ];
        let mut seed_with_spawn = None;
        for seed in 0..256 {
            store.clear();
            fastrand::seed(seed);
            store.add_living_effect_particles(
                dvec3(0.0, 0.0, 0.0),
                0.6,
                1.8,
                &options[..1],
                true,
                false,
                dvec3(0.0, 0.0, 0.0),
            );
            if !store.pending.is_empty() {
                seed_with_spawn = Some(seed);
                break;
            }
        }
        let seed = seed_with_spawn.expect("Effect living particle spawns within 256 seeds");
        let mut baseline = [dvec3(0.0, 0.0, 0.0); 2];
        for (index, option) in options.iter().enumerate() {
            for power in [1.0, 0.5, 0.0] {
                let option = match option {
                    ParticleOptions::Effect(effect) => {
                        ParticleOptions::Effect(ColorPowerParticle {
                            color: effect.color,
                            power,
                        })
                    }
                    ParticleOptions::InstantEffect(effect) => {
                        ParticleOptions::InstantEffect(ColorPowerParticle {
                            color: effect.color,
                            power,
                        })
                    }
                    _ => unreachable!(),
                };
                store.clear();
                fastrand::seed(seed);
                store.add_living_effect_particles(
                    dvec3(0.0, 0.0, 0.0),
                    0.6,
                    1.8,
                    &[option],
                    true,
                    false,
                    dvec3(0.0, 0.0, 0.0),
                );
                assert_eq!(store.pending.len(), 1);
                let velocity = store.pending[0].vel;
                assert_eq!(
                    store.pending[0].color,
                    [
                        0x12 as f32 / 255.0,
                        0x34 as f32 / 255.0,
                        0x56 as f32 / 255.0
                    ]
                );
                if power == 1.0 {
                    baseline[index] = velocity;
                    assert!(matches!(
                        store.pending[0].kind,
                        super::Kind::EntityEffect | super::Kind::SpellEffect
                    ));
                } else {
                    let raw = baseline[index];
                    assert_eq!(
                        velocity,
                        dvec3(
                            raw.x * f64::from(power),
                            (raw.y - 0.1) * f64::from(power) + 0.1,
                            raw.z * f64::from(power),
                        )
                    );
                    if power == 0.0 {
                        assert_eq!(velocity, dvec3(0.0, 0.1, 0.0));
                    }
                }
            }
        }
        assert_eq!(baseline[0], baseline[1]);
    }

    #[test]
    fn potion_break_spawns_use_radial_power_positions_and_effect_specific_frames() {
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        store.set_mode(super::ParticleMode::All);
        let registry = crate::world::block::registry::BlockRegistry::test_empty();
        let chunks = ChunkStore::new(2);
        for (instant, expected_kind) in [
            (false, super::Kind::EntityEffect),
            (true, super::Kind::SpellEffect),
        ] {
            store.pending.clear();
            fastrand::seed(if instant { 2007 } else { 2002 });
            store.add_potion_break_particles(
                dvec3(10.5, 20.0, 30.5),
                0xff80_40c0,
                instant,
                dvec3(10.5, 20.0, 30.5),
                &registry,
                &chunks,
            );
            assert_eq!(store.pending.len(), 108);
            assert!(store.pending[..8].iter().all(|particle| {
                matches!(
                    particle.kind,
                    super::Kind::Item | super::Kind::ItemTranslucent
                ) && particle.pos == dvec3(10.5, 20.0, 30.5)
            }));
            let effect_particles = &store.pending[8..];
            assert_eq!(effect_particles.len(), 100);
            let first = &effect_particles[0];
            let (expected_pos, expected_vel) = if instant {
                (
                    dvec3(10.42181369123755, 20.3, 30.61682287372779),
                    dvec3(
                        0.027405258023728847,
                        0.02163990108827317,
                        -0.1123468668768632,
                    ),
                )
            } else {
                (
                    dvec3(10.416956909616552, 20.3, 30.575336662981115),
                    dvec3(
                        -0.02579559487411549,
                        0.03152759990844002,
                        -0.06975067760901371,
                    ),
                )
            };
            assert!((first.pos - expected_pos).length() < 1.0e-12);
            assert!((first.vel - expected_vel).length() < 1.0e-12);
            assert!(effect_particles.iter().all(|p| {
                p.kind == expected_kind
                    && (p.pos.y - 20.3).abs() < 1e-12
                    && (p.pos.x - 10.5).abs() <= 0.4
                    && (p.pos.z - 30.5).abs() <= 0.4
            }));
            assert!(
                effect_particles
                    .iter()
                    .any(|p| (p.pos.x - 10.5).abs() > 0.1 || (p.pos.z - 30.5).abs() > 0.1)
            );
        }
    }

    #[test]
    fn potion_event_2002_and_2007_use_distinct_eight_frame_sprites() {
        assert_eq!(super::ENTITY_EFFECT_SPRITES.len(), 8);
        assert_eq!(super::SPELL_SPRITES.len(), 8);
        assert_ne!(super::ENTITY_EFFECT_SPRITES, super::SPELL_SPRITES);
        assert_ne!(super::RAID_OMEN_SPRITE, super::TRIAL_OMEN_SPRITE);
    }

    #[test]
    fn packet_effect_families_keep_their_own_animated_or_fixed_atlas_frames() {
        let mut uv = AtlasUVMap::test_empty();
        for (index, name) in super::ENTITY_EFFECT_SPRITES
            .iter()
            .chain(super::SPELL_SPRITES.iter())
            .chain([super::RAID_OMEN_SPRITE, super::TRIAL_OMEN_SPRITE].iter())
            .enumerate()
        {
            let mut region = uv.missing_region();
            region.u_min = index as f32 / 32.0;
            region.u_max = (index as f32 + 1.0) / 32.0;
            region.v_min = 0.25;
            region.v_max = 0.5;
            uv.test_insert_region(name, region);
        }
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        let registry = crate::world::block::registry::BlockRegistry::test_empty();
        let chunks = ChunkStore::new(2);
        let cases = [
            (
                ServerParticleKind::EntityEffect,
                super::ServerParticleOptions::EntityEffect { color: 0xffff_ffff },
                false,
            ),
            (
                ServerParticleKind::Effect,
                super::ServerParticleOptions::Spell {
                    color: 0x00ff_ffff,
                    power: 1.0,
                },
                false,
            ),
            (
                ServerParticleKind::InstantEffect,
                super::ServerParticleOptions::Spell {
                    color: 0x00ff_ffff,
                    power: 1.0,
                },
                true,
            ),
            (
                ServerParticleKind::Witch,
                super::ServerParticleOptions::Simple,
                true,
            ),
            (
                ServerParticleKind::RaidOmen,
                super::ServerParticleOptions::Simple,
                false,
            ),
            (
                ServerParticleKind::TrialOmen,
                super::ServerParticleOptions::Simple,
                false,
            ),
        ];
        for (kind, options, spell) in cases {
            store.clear();
            store.add_particles_from_packet(
                kind,
                options,
                true,
                false,
                dvec3(0.0, 0.0, 0.0),
                dvec3(0.0, 0.0, 0.0),
                0.0,
                0,
                dvec3(0.0, 0.0, 0.0),
                &registry,
                &chunks,
                &Default::default(),
            );
            assert_eq!(store.pending.len(), 1, "{kind:?}");
            if kind == ServerParticleKind::Witch {
                let color = store.pending[0].color;
                assert!((0.35..0.85).contains(&color[0]));
                assert_eq!(color[1], 0.0);
                assert_eq!(color[0], color[2]);
            }
            for _ in 0..3 {
                store.tick(&chunks);
            }
            let quad = store.extract(0.0, dvec3(0.0, 0.0, 0.0), &chunks);
            assert_eq!(quad.len(), 1, "{kind:?}");
            let expected_sprite = match kind {
                ServerParticleKind::EntityEffect | ServerParticleKind::Effect => {
                    let frame = super::animated_frame_index(
                        store.particles[0].age,
                        store.particles[0].lifetime,
                        8,
                    );
                    store.entity_effect_frames[frame]
                }
                ServerParticleKind::InstantEffect | ServerParticleKind::Witch => {
                    let frame = super::animated_frame_index(
                        store.particles[0].age,
                        store.particles[0].lifetime,
                        8,
                    );
                    store.spell_frames[frame]
                }
                ServerParticleKind::RaidOmen => store.raid_omen_sprite,
                ServerParticleKind::TrialOmen => store.trial_omen_sprite,
                _ => unreachable!(),
            };
            assert_eq!(quad[0].u0, expected_sprite.u_min, "{kind:?}");
            assert_eq!(quad[0].v0, expected_sprite.v_min, "{kind:?}");
            assert_eq!(quad[0].translucent, true, "{kind:?}");
            if matches!(
                kind,
                ServerParticleKind::RaidOmen | ServerParticleKind::TrialOmen
            ) {
                assert_eq!(quad[0].color, 0xffff_ffff, "{kind:?}");
            }
            assert_eq!(
                spell,
                matches!(
                    kind,
                    ServerParticleKind::InstantEffect | ServerParticleKind::Witch
                )
            );
        }
    }

    #[test]
    fn item_world_context_matches_packet_and_local_requests_across_switch_and_reload() {
        use std::sync::Arc;

        use azalea_registry::Registry;
        use glam::DVec3;

        use crate::renderer::chunk::mesher::Colormap;
        use crate::world::block::registry::BlockRegistry;

        fn region(sprite: u16, u_min: f32) -> AtlasRegion {
            AtlasRegion {
                u_min,
                v_min: 0.0,
                u_max: u_min + 0.1,
                v_max: 0.1,
                pixel_rect: [0; 4],
                sprite,
                opaque: true,
                translucent: false,
                alpha_counts: [0, 0, 1],
            }
        }
        fn expected_uv(particle: &Particle, u_min: f32) {
            assert!(
                particle.u0 >= u_min + 0.025 && particle.u0 <= u_min + 0.1,
                "u0={}",
                particle.u0
            );
        }

        let leaves = [
            ("item/context_overworld", "item/context_overworld_sprite"),
            ("item/context_nether", "item/context_nether_sprite"),
            ("item/context_custom", "item/context_custom_sprite"),
            ("item/context_fallback", "item/context_fallback_sprite"),
        ];
        let leaf = |model: &str| serde_json::json!({"type":"minecraft:model","model":format!("minecraft:{model}")});
        let definition = serde_json::json!({
            "type":"minecraft:select", "property":"minecraft:context_dimension",
            "cases":[
                {"when":"minecraft:overworld", "model":leaf("item/context_overworld")},
                {"when":"minecraft:the_nether", "model":leaf("item/context_nether")},
                {"when":"example:underdeep", "model":leaf("item/context_custom")},
            ],
            "fallback":leaf("item/context_fallback"),
        });
        let mut block_registry = BlockRegistry::test_empty();
        block_registry.test_set_item_particle_model("stone", definition, &leaves);

        let mut atlas = AtlasUVMap::test_empty();
        for (index, (_, texture)) in leaves.iter().enumerate() {
            atlas.test_insert_region(texture, region(index as u16 + 1, index as f32 * 0.2));
        }
        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(atlas, colors.clone(), colors.clone(), colors);
        let registries = Arc::new(azalea_core::registry_holder::RegistryHolder::default());
        let stack = azalea_registry::builtin::ItemKind::Stone;
        let item = || super::ServerParticleOptions::Item {
            item_id: stack.to_u32(),
            count: 1,
            components: azalea_inventory::DataComponentPatch::default(),
            raw_components: None,
        };
        let chunks = ChunkStore::new(1);
        let climate = std::collections::HashMap::new();
        let sources = ["level-packet", "ambient-request", "entity-request"];
        for (dimension, u_min) in [
            ("minecraft:overworld", 0.0),
            ("minecraft:the_nether", 0.2),
            ("example:underdeep", 0.4),
        ] {
            // Mirrors the world lifecycle: DimensionInfo invalidates the old
            // key, then DimensionName installs the actual level key (not type).
            store.set_item_model_world(None, Arc::clone(&registries));
            store.set_item_model_world(Some(dimension), Arc::clone(&registries));
            for source in sources.iter().copied() {
                store.pending.clear();
                if source == "level-packet" {
                    store.add_particles_from_packet(
                        ServerParticleKind::Item,
                        item(),
                        false,
                        false,
                        DVec3::ZERO,
                        DVec3::ZERO,
                        1.0,
                        0,
                        DVec3::ZERO,
                        &block_registry,
                        &chunks,
                        &climate,
                    );
                } else if source == "ambient-request" {
                    store.add_particle_spawn_request(
                        crate::world::environment_particles::AmbientSpawn {
                            kind: ServerParticleKind::Item,
                            options: item(),
                            position: DVec3::ZERO,
                            velocity: DVec3::ZERO,
                            always_visible: false,
                        },
                        DVec3::ZERO,
                        &block_registry,
                        &chunks,
                        &climate,
                    );
                } else {
                    store.add_particle_spawn_request(
                        crate::world::particle_tick::ParticleSpawnRequest {
                            kind: ServerParticleKind::Item,
                            options: item(),
                            position: DVec3::ZERO,
                            velocity: DVec3::ZERO,
                            always_visible: false,
                        },
                        DVec3::ZERO,
                        &block_registry,
                        &chunks,
                        &climate,
                    );
                }
                assert_eq!(store.pending.len(), 1, "{source} / {dimension}");
                expected_uv(&store.pending[0], u_min);
            }
        }

        let reloaded = {
            let mut atlas = AtlasUVMap::test_empty();
            for (index, (_, texture)) in leaves.iter().enumerate() {
                atlas.test_insert_region(texture, region(index as u16 + 10, index as f32 * 0.2));
            }
            atlas
        };
        let grass = Arc::clone(&store.grass_colormap);
        let foliage = Arc::clone(&store.foliage_colormap);
        let dry_foliage = Arc::clone(&store.dry_foliage_colormap);
        store.reload_assets(reloaded, grass, foliage, dry_foliage);
        assert!(Arc::ptr_eq(&store.item_model_registries, &registries));
        store.pending.clear();
        store.add_particle_spawn_request(
            crate::world::particle_tick::ParticleSpawnRequest {
                kind: ServerParticleKind::Item,
                options: item(),
                position: DVec3::ZERO,
                velocity: DVec3::ZERO,
                always_visible: false,
            },
            DVec3::ZERO,
            &block_registry,
            &chunks,
            &climate,
        );
        assert_eq!(store.pending.len(), 1);
        expected_uv(&store.pending[0], 0.4);
    }

    #[test]
    fn packet_effect_power_applies_to_effect_and_instant_effect() {
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let registry = crate::world::block::registry::BlockRegistry::test_empty();
        let chunks = ChunkStore::new(2);
        for kind in [
            ServerParticleKind::Effect,
            ServerParticleKind::InstantEffect,
        ] {
            store.clear();
            fastrand::seed(17);
            store.add_particles_from_packet(
                kind,
                super::ServerParticleOptions::Spell {
                    color: -1,
                    power: 1.0,
                },
                true,
                false,
                dvec3(0.0, 0.0, 0.0),
                dvec3(0.0, 0.0, 0.0),
                0.0,
                0,
                dvec3(0.0, 0.0, 0.0),
                &registry,
                &chunks,
                &Default::default(),
            );
            let baseline_vel = store.pending[0].vel;
            for power in [0.0, 0.5] {
                store.clear();
                fastrand::seed(17);
                store.add_particles_from_packet(
                    kind,
                    super::ServerParticleOptions::Spell { color: -1, power },
                    true,
                    false,
                    dvec3(0.0, 0.0, 0.0),
                    dvec3(0.0, 0.0, 0.0),
                    0.0,
                    0,
                    dvec3(0.0, 0.0, 0.0),
                    &registry,
                    &chunks,
                    &Default::default(),
                );
                assert_eq!(store.pending.len(), 1);
                let spawn_pos = store.pending[0].pos;
                let spawn_vel = store.pending[0].vel;
                let expected_vel = dvec3(
                    baseline_vel.x * f64::from(power),
                    (baseline_vel.y - 0.1) * f64::from(power) + 0.1,
                    baseline_vel.z * f64::from(power),
                );
                assert_eq!(spawn_vel, expected_vel, "{kind:?}, power={power}");
                store.tick(&chunks);
                store.tick(&chunks);
                assert_eq!(store.particles.len(), 1);
                let expected_delta = dvec3(spawn_vel.x, spawn_vel.y + 0.004, spawn_vel.z);
                assert!((store.particles[0].pos - spawn_pos - expected_delta).length() < 1e-12);
                assert_eq!(store.extract(0.0, dvec3(0.0, 0.0, 0.0), &chunks).len(), 1);
            }
        }
    }

    #[test]
    fn living_omen_effects_reach_the_filter_and_emit_at_most_one_particle() {
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let metadata = [const { ParticleOptions::RaidOmen }; 75];
        let mut spawned = false;
        for seed in 0..256 {
            fastrand::seed(seed);
            store.add_living_effect_particles(
                dvec3(0.0, 0.0, 0.0),
                0.6,
                1.8,
                &metadata,
                true,
                false,
                dvec3(0.0, 0.0, 0.0),
            );
            assert!(store.pending.len() <= 1);
            if !store.pending.is_empty() {
                spawned = true;
                break;
            }
        }
        assert!(spawned, "omen reaches the living-particle spawn path");
        assert_eq!(store.pending[0].color, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn living_witch_uses_red_blue_spell_brightness_and_advances_once_per_tick() {
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let metadata = [ParticleOptions::Witch];
        let mut chunks = ChunkStore::new(2);
        chunks.light_data.insert(
            (0, 0),
            std::sync::Arc::new(crate::world::chunk::ChunkLightData {
                sky_sections: Vec::new(),
                block_sections: Vec::new(),
                min_y: -64,
                has_sky: true,
                sky_top_section: None,
            }),
        );
        let mut spawned_seed = None;
        for seed in 0..256 {
            store.clear();
            fastrand::seed(seed);
            store.add_living_effect_particles(
                dvec3(0.0, 0.0, 0.0),
                0.6,
                1.8,
                &metadata,
                false,
                false,
                dvec3(0.0, 0.0, 0.0),
            );
            if !store.pending.is_empty() {
                spawned_seed = Some(seed);
                break;
            }
        }
        let seed = spawned_seed.expect("Witch living effect spawns within 256 seeds");
        let initial_pos = store.pending[0].pos;
        let expected_color = store.pending[0].color;
        fastrand::seed(seed);
        let _selected_option = fastrand::usize(..1);
        assert_eq!(fastrand::u32(..) % 4, 0);
        let expected_pos = dvec3(
            (fastrand::f64() - 0.5) * f64::from(0.6f32),
            fastrand::f64() * f64::from(1.8f32),
            (fastrand::f64() - 0.5) * f64::from(0.6f32),
        );
        assert_eq!(initial_pos, expected_pos);
        let expected_particle = Particle::entity_effect(
            expected_pos,
            glam::DVec3::ONE,
            0xffff_ffff,
            &store.entity_effect_frames,
        );
        let expected_brightness = super::spell_brightness(fastrand::f32());
        assert_eq!(store.pending[0].vel, expected_particle.vel);
        assert_eq!(store.pending[0].lifetime, expected_particle.lifetime);
        assert_eq!(
            expected_color,
            [expected_brightness, 0.0, expected_brightness]
        );
        assert_eq!(expected_color[1], 0.0);
        assert_eq!(expected_color[0], expected_color[2]);
        assert!((0.35..0.85).contains(&expected_color[0]));
        assert!(matches!(store.pending[0].kind, super::Kind::SpellEffect));
        assert!(store.pending.len() <= 1);
        store.tick(&chunks);
        assert!(store.pending.is_empty());
        assert_eq!(store.particles.len(), 1);
        store.tick(&chunks);
        assert_ne!(store.particles[0].pos, initial_pos);
        assert_eq!(store.particles[0].color, expected_color);
        assert!(store.particles[0].light > 0.0);
        let quad = store.extract(0.0, dvec3(0.0, 0.0, 0.0), &chunks);
        assert_eq!(quad.len(), 1);
        let red = quad[0].color & 0xff;
        let green = (quad[0].color >> 8) & 0xff;
        let blue = (quad[0].color >> 16) & 0xff;
        assert!(red > 0 && blue > 0);
        assert_eq!(red, blue);
        assert_eq!(green, 0);
        assert!((89..=217).contains(&red));
    }

    #[test]
    fn living_effect_metadata_respects_empty_mode_and_distance_filters() {
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let metadata = [ParticleOptions::EntityEffect(
            azalea_entity::particle::ColorParticle {
                color: azalea_core::color::RgbColor::new(10, 20, 30),
            },
        )];
        store.add_living_effect_particles(
            dvec3(0.0, 0.0, 0.0),
            0.6,
            1.8,
            &[],
            false,
            false,
            dvec3(0.0, 0.0, 0.0),
        );
        assert!(store.pending.is_empty());
        store.add_living_effect_particles(
            dvec3(0.0, 0.0, 0.0),
            0.6,
            1.8,
            &metadata,
            false,
            false,
            dvec3(100.0, 0.0, 0.0),
        );
        assert!(store.pending.is_empty());
        store.set_mode(super::ParticleMode::Minimal);
        store.add_living_effect_particles(
            dvec3(0.0, 0.0, 0.0),
            0.6,
            1.8,
            &metadata,
            false,
            false,
            dvec3(0.0, 0.0, 0.0),
        );
        assert!(store.pending.is_empty());

        store.set_mode(super::ParticleMode::All);
        for _ in 0..super::MAX_PARTICLES {
            store.particles.push(Particle::entity_effect(
                dvec3(0.0, 0.0, 0.0),
                dvec3(0.0, 0.0, 0.0),
                0xFF0A_141E,
                &store.entity_effect_frames,
            ));
        }
        store.add_living_effect_particles(
            dvec3(0.0, 0.0, 0.0),
            0.6,
            1.8,
            &metadata,
            false,
            false,
            dvec3(0.0, 0.0, 0.0),
        );
        assert_eq!(
            store.particles.len() + store.pending.len(),
            super::MAX_PARTICLES
        );
    }

    #[test]
    fn effect_spawn_probability_boundaries_and_alpha_are_exact() {
        for (ambient, invisible, denominator) in [
            (false, false, 4),
            (true, false, 20),
            (false, true, 15),
            (true, true, 75),
        ] {
            assert!(super::effect_particle_selected(0, ambient, invisible));
            assert!(!super::effect_particle_selected(
                denominator - 1,
                ambient,
                invisible
            ));
            assert!(super::effect_particle_selected(
                denominator,
                ambient,
                invisible
            ));
        }
        let frame = AtlasRegion {
            u_min: 0.1,
            v_min: 0.2,
            u_max: 0.3,
            v_max: 0.4,
            pixel_rect: [0; 4],
            sprite: 3,
            opaque: false,
            translucent: true,
            alpha_counts: [0; 3],
        };
        let particle = Particle::entity_effect(
            dvec3(0.0, 0.0, 0.0),
            dvec3(0.0, 0.0, 0.0),
            0x26123456,
            &[frame; 8],
        );
        assert_eq!(
            particle.color,
            [
                0x12 as f32 / 255.0,
                0x34 as f32 / 255.0,
                0x56 as f32 / 255.0
            ]
        );
        assert_eq!(particle.alpha, 38.0 / 255.0);
        assert_eq!(
            (particle.u0, particle.u1, particle.v0, particle.v1),
            (0.1, 0.3, 0.2, 0.4)
        );
        assert!((8..=40).contains(&particle.lifetime));
        assert!(particle.kind.translucent());
    }

    #[test]
    fn campfire_ambient_smoke_requires_lit_and_dry_state() {
        assert!(super::campfire_smoke_enabled(true, false));
        assert!(!super::campfire_smoke_enabled(false, false));
        assert!(!super::campfire_smoke_enabled(true, true));
        assert_eq!(super::campfire_smoke_count(0.0, 0), Some(2));
        assert_eq!(super::campfire_smoke_count(0.109, 1), Some(3));
        assert_eq!(super::campfire_smoke_count(0.11, 1), None);
        assert!(super::campfire_food_smoke_enabled(0.199));
        assert!(!super::campfire_food_smoke_enabled(0.2));
    }

    #[test]
    fn campfire_override_tick_uses_native_gravity_without_base_friction() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let frame = super::AtlasRegion {
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
        let mut particle = super::Particle::campfire_smoke(
            dvec3(0.0, 5.0, 0.0),
            false,
            &frames,
            &mut fastrand::Rng::with_seed(7),
        );
        particle.vel = dvec3(0.02, 0.1, -0.03);
        particle.gravity = 3.0e-6;
        particle.friction = 0.98;
        assert!(particle.tick(&ChunkStore::new(2), &frames, &frames, &[frame; 16],));
        assert!((particle.vel.x - 0.02).abs() <= 0.0002);
        assert!((particle.vel.z + 0.03).abs() <= 0.0002);
        assert!((particle.vel.y - (0.1 - 3.0e-6)).abs() < 1.0e-12);
    }

    #[test]
    fn campfire_food_smoke_uses_native_slot_rotation_and_generic_smoke_species() {
        use azalea_core::position::BlockPos;

        let pos = BlockPos::new(10, 20, -4);
        let south = super::campfire_slot_smoke_pos(pos, "south", 0);
        assert_eq!(south, dvec3(10.1875, 20.5, -3.8125));
        assert_eq!(
            super::campfire_slot_smoke_pos(pos, "west", 0),
            super::campfire_slot_smoke_pos(pos, "south", 1)
        );
        assert_eq!(
            super::campfire_slot_smoke_pos(pos, "north", 0),
            super::campfire_slot_smoke_pos(pos, "south", 2)
        );
        assert_eq!(
            super::campfire_slot_smoke_pos(pos, "east", 0),
            super::campfire_slot_smoke_pos(pos, "south", 3)
        );
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;

        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        for _ in 0..4 {
            store.add_campfire_food_smoke(south, dvec3(10.5, 20.5, -3.5));
        }
        assert_eq!(store.pending.len(), 4);
        assert!(
            store
                .pending
                .iter()
                .all(|particle| matches!(particle.kind, super::Kind::Smoke))
        );
    }

    #[test]
    fn campfire_species_keep_native_sprite_lifetime_and_alpha() {
        let frames = std::array::from_fn(|i| AtlasRegion {
            u_min: i as f32,
            v_min: i as f32,
            u_max: i as f32 + 1.0,
            v_max: i as f32 + 1.0,
            pixel_rect: [0; 4],
            sprite: i as u16,
            opaque: false,
            translucent: true,
            alpha_counts: [0; 3],
        });
        let mut cosy_rng = fastrand::Rng::with_seed(1);
        let mut signal_rng = fastrand::Rng::with_seed(1);
        let cosy = Particle::campfire_smoke(dvec3(0.0, 1.0, 0.0), false, &frames, &mut cosy_rng);
        let signal = Particle::campfire_smoke(dvec3(0.0, 1.0, 0.0), true, &frames, &mut signal_rng);
        assert!(matches!(cosy.kind, super::Kind::CampfireCosySmoke));
        assert!(matches!(signal.kind, super::Kind::CampfireSignalSmoke));
        assert!(frames.iter().any(|frame| frame.u_min == cosy.u0));
        assert!(frames.iter().any(|frame| frame.u_min == signal.u0));
        assert!((80..130).contains(&cosy.lifetime));
        assert!((280..330).contains(&signal.lifetime));
        assert_eq!(cosy.alpha, 0.9);
        assert_eq!(signal.alpha, 0.95);
        assert!((0.3..=0.6).contains(&cosy.size));
        assert!((0.3..=0.6).contains(&signal.size));
        assert_eq!(cosy.gravity, 3.0e-6);
        assert_eq!(cosy.friction, 0.98);
        assert_eq!(super::CAMPFIRE_COSY_SMOKE_SPRITES.len(), 8);
        assert_eq!(super::CAMPFIRE_SIGNAL_SMOKE_SPRITES.len(), 8);
        assert_eq!(
            super::CAMPFIRE_COSY_SMOKE_SPRITES,
            super::CAMPFIRE_SIGNAL_SMOKE_SPRITES
        );
        assert!(cosy.kind.translucent());
        assert!(signal.kind.translucent());
    }

    #[test]
    fn every_native_particle_registry_identity_resolves_by_name() {
        use pomme_protocol::registries::{ClientRegistry, RegistryTable};

        let registry = RegistryTable::native();
        for id in 0..125 {
            let name = registry
                .name_of(ClientRegistry::ParticleType, id)
                .expect("26.2 particle registry entry");
            let kind = ServerParticleKind::from_id(id).expect("known native particle");
            assert_eq!(
                ServerParticleKind::from_name(name),
                Some(kind),
                "{name} id={id}"
            );
        }
        assert_eq!(
            registry.name_of(ClientRegistry::ParticleType, 70),
            Some("white_smoke")
        );
        assert_eq!(
            registry.name_of(ClientRegistry::ParticleType, 71),
            Some("sneeze")
        );
        assert_eq!(
            ServerParticleKind::from_id(70),
            Some(ServerParticleKind::WhiteSmoke)
        );
        assert_eq!(
            ServerParticleKind::from_id(71),
            Some(ServerParticleKind::Sneeze)
        );
        assert_eq!(
            ServerParticleKind::from_id(84),
            Some(ServerParticleKind::CampfireCosySmoke)
        );
        assert_eq!(
            ServerParticleKind::from_id(85),
            Some(ServerParticleKind::CampfireSignalSmoke)
        );
        assert_eq!(ServerParticleKind::from_id(125), None);
    }

    #[test]
    fn every_native_particle_kind_spawns_from_a_valid_typed_packet_fixture() {
        use std::collections::{HashMap, HashSet};
        use std::sync::Arc;

        use azalea_core::position::{BlockPos, ChunkPos};
        use azalea_inventory::DataComponentPatch;
        use azalea_registry::Registry;
        use glam::DVec3;
        use pomme_protocol::registries::{ClientRegistry, RegistryTable};

        use super::{ParticleMode, ParticleStore, ServerParticleOptions};
        use crate::renderer::chunk::mesher::Colormap;
        use crate::world::block::registry::BlockRegistry;

        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let stone = crate::world::block::first_state_of("stone").unwrap();
        let air = crate::world::block::first_state_of("air").unwrap();
        let water = crate::world::block::first_state_of("water").unwrap();
        let mut atlas = AtlasUVMap::test_empty();
        let fixture_region = |index: usize| AtlasRegion {
            u_min: 0.01 + index as f32 * 0.0001,
            v_min: 0.2,
            u_max: 0.06 + index as f32 * 0.0001,
            v_max: 0.4,
            pixel_rect: [index as u16, 2, 2, 2],
            sprite: index as u16,
            opaque: true,
            translucent: false,
            alpha_counts: [0, 0, 4],
        };
        // Java 26.2 ParticleResources registrations intersect the client jar's
        // particle descriptors at 111 kinds. These 14 use terrain/item/model or
        // emitter-child consumers instead of owning a descriptor.
        const NO_PARTICLE_DESCRIPTOR: [&str; 14] = [
            "block",
            "block_crumble",
            "block_marker",
            "dust_pillar",
            "elder_guardian",
            "explosion_emitter",
            "geyser",
            "gust_emitter_large",
            "gust_emitter_small",
            "item",
            "item_cobweb",
            "item_slime",
            "item_snowball",
            "noxious_gas_cloud",
        ];
        let descriptor_frame_count = |name: &str| match name {
            "dragon_breath" => 3,
            "fishing" | "sculk_charge_pop" | "rain" | "splash" => 4,
            "bubble_pop" | "trial_spawner_detection" | "trial_spawner_detection_ominous" => 5,
            "small_gust" | "sculk_charge" => 7,
            "noxious_gas"
            | "geyser_base"
            | "geyser_poof"
            | "geyser_plume"
            | "cloud"
            | "dust"
            | "dust_color_transition"
            | "effect"
            | "end_rod"
            | "entity_effect"
            | "falling_dust"
            | "firework"
            | "instant_effect"
            | "large_smoke"
            | "poof"
            | "portal"
            | "smoke"
            | "white_smoke"
            | "sneeze"
            | "spit"
            | "squid_ink"
            | "sweep_attack"
            | "totem_of_undying"
            | "witch"
            | "reverse_portal"
            | "snowflake"
            | "glow_squid_ink"
            | "dust_plume" => 8,
            "sculk_soul" | "soul" => 11,
            "gust"
            | "cherry_leaves"
            | "pale_oak_leaves"
            | "tinted_leaves"
            | "campfire_cosy_smoke"
            | "campfire_signal_smoke" => 12,
            "explosion" | "sonic_boom" => 16,
            "enchant" => 26,
            _ => 1,
        };
        let mut descriptor_uvs = HashMap::<String, Vec<(f32, f32, f32, f32)>>::new();
        for id in 0..125 {
            let name = RegistryTable::native()
                .name_of(ClientRegistry::ParticleType, id)
                .unwrap();
            if NO_PARTICLE_DESCRIPTOR.contains(&name) {
                continue;
            }
            let particle = crate::assets::AssetId::parse(&format!("minecraft:{name}")).canonical();
            let frames = if matches!(name, "poof" | "smoke") {
                GENERIC_PARTICLE_SPRITES[..descriptor_frame_count(name)]
                    .iter()
                    .enumerate()
                    .map(|(frame, sprite)| ((*sprite).to_owned(), fixture_region(500 + frame)))
                    .collect::<Vec<_>>()
            } else {
                (0..descriptor_frame_count(name))
                    .map(|frame| {
                        let sprite_name = format!("particle/fixture_{id}_{frame}");
                        let region = fixture_region(id as usize * 8 + frame + 1);
                        atlas.test_insert_region(&sprite_name, region);
                        (sprite_name, region)
                    })
                    .collect::<Vec<_>>()
            };
            descriptor_uvs.insert(
                name.to_owned(),
                frames
                    .iter()
                    .map(|(_, r)| (r.u_min, r.u_max, r.v_min, r.v_max))
                    .collect(),
            );
            atlas.test_insert_particle_sprites(
                &particle,
                frames.into_iter().map(|(name, _)| name).collect(),
            );
        }
        assert_eq!(descriptor_uvs.len(), 111);
        for name in NO_PARTICLE_DESCRIPTOR {
            assert!(
                atlas
                    .particle_sprite_names(&format!("minecraft:{name}"))
                    .is_none(),
                "{name} must use its non-descriptor consumer"
            );
        }
        for (index, name) in GENERIC_PARTICLE_SPRITES
            .into_iter()
            .chain(END_ROD_SPRITES)
            .chain(EXPLOSION_SPRITES)
            .chain(CAMPFIRE_COSY_SMOKE_SPRITES)
            .chain(CAMPFIRE_SIGNAL_SMOKE_SPRITES)
            .chain(["fixture/block_particle", "fixture/item_particle"])
            .enumerate()
        {
            atlas.test_insert_region(name, fixture_region(500 + index));
        }
        let mut block_registry = BlockRegistry::test_empty();
        block_registry.test_add_particle_fixture();
        let mut chunks = ChunkStore::new(2);
        let mut column = azalea_world::chunk::Chunk::default();
        column.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(ChunkPos::new(0, 0), column);
        chunks.set_block_state(0, 63, 0, stone);
        let climate = HashMap::new();
        let colors = Arc::new(Colormap::test_empty());
        let mut store = ParticleStore::new(atlas, colors.clone(), colors.clone(), colors);
        store.set_mode(ParticleMode::All);
        let mut seen_names = HashSet::new();
        let mut seen_kinds = HashSet::new();
        let registry = RegistryTable::native();
        let mut fixture_count = 0;
        for id in 0..125 {
            let name = registry.name_of(ClientRegistry::ParticleType, id).unwrap();
            assert!(
                seen_names.insert(name),
                "duplicate native registry name {name}"
            );
            let kind = ServerParticleKind::from_id(id).unwrap();
            assert_eq!(
                ServerParticleKind::from_name(name),
                Some(kind),
                "id={id} {name}"
            );
            assert!(
                seen_kinds.insert(format!("{kind:?}")),
                "duplicate fixture kind {kind:?}"
            );
            store.clear();
            chunks.set_block_state(0, 64, 0, air);
            chunks.set_block_state(0, 63, 0, stone);
            if matches!(
                kind,
                ServerParticleKind::Bubble
                    | ServerParticleKind::SulfurBubbles
                    | ServerParticleKind::CurrentDown
                    | ServerParticleKind::BubbleColumnUp
            ) {
                chunks.set_block_state(0, 64, 0, water);
                if kind == ServerParticleKind::Bubble {
                    let fluid = crate::world::block::fluid(chunks.get_block_state(0, 64, 0));
                    assert_eq!(
                        chunks.get_block_state(0, 64, 0),
                        water,
                        "Bubble fixture water state"
                    );
                    assert_eq!(
                        fluid.kind,
                        crate::world::block::FluidKind::Water,
                        "Bubble fixture fluid kind"
                    );
                    assert_eq!(fluid.amount, 8, "Bubble fixture source amount");
                    assert!(!fluid.falling, "Bubble fixture must be still source water");
                    assert_eq!(
                        chunks.get_block_state(0, 63, 0),
                        stone,
                        "Bubble fixture support state"
                    );
                }
            } else if kind == ServerParticleKind::NoxiousGas {
                // The direct provider's origin is above its source water.
                chunks.set_block_state(0, 63, 0, water);
            } else if kind == ServerParticleKind::NoxiousGasCloud {
                // Its child sampler may pick any point within radius 3. Put the
                // source inside a full-water patch so every seeded sample has
                // valid loaded terrain, water below, and unobstructed air.
                for x in 5..=11 {
                    for z in 5..=11 {
                        chunks.set_block_state(x, 63, z, water);
                    }
                }
            }
            let options = match kind {
                ServerParticleKind::Block
                | ServerParticleKind::BlockMarker
                | ServerParticleKind::FallingDust
                | ServerParticleKind::DustPillar
                | ServerParticleKind::BlockCrumble => ServerParticleOptions::Block(stone),
                ServerParticleKind::Item => ServerParticleOptions::Item {
                    item_id: azalea_registry::builtin::ItemKind::Stone.to_u32(),
                    count: 3,
                    components: DataComponentPatch::default(),
                    raw_components: Some(Arc::new({
                        let mut raw = simdnbt::owned::NbtCompound::new();
                        raw.insert("minecraft:custom_name", "preserve through item particle");
                        raw
                    })),
                },
                ServerParticleKind::EntityEffect => {
                    ServerParticleOptions::EntityEffect { color: 0xff55_aa33 }
                }
                ServerParticleKind::Effect | ServerParticleKind::InstantEffect => {
                    ServerParticleOptions::Spell {
                        color: 0x55aa33,
                        power: 1.0,
                    }
                }
                ServerParticleKind::Dust => ServerParticleOptions::Dust {
                    packed_color: 0x55aa33,
                    scale: 1.0,
                },
                ServerParticleKind::DustColorTransition => {
                    ServerParticleOptions::DustColorTransition {
                        from_color: 0xff0000,
                        to_color: 0x0000ff,
                        scale: 1.0,
                    }
                }
                ServerParticleKind::TintedLeaves => {
                    ServerParticleOptions::Color { color: 0x55aa33 }
                }
                ServerParticleKind::DragonBreath => ServerParticleOptions::Power { power: 1.0 },
                ServerParticleKind::SculkCharge => {
                    ServerParticleOptions::SculkCharge { roll: 0.25 }
                }
                ServerParticleKind::Geyser | ServerParticleKind::GeyserPlume => {
                    ServerParticleOptions::Geyser { water_blocks: 2 }
                }
                ServerParticleKind::GeyserBase | ServerParticleKind::GeyserPoof => {
                    ServerParticleOptions::GeyserBase {
                        water_blocks: 2,
                        burst_impulse_base: 1.0,
                    }
                }
                ServerParticleKind::Shriek => ServerParticleOptions::Shriek { delay: 0 },
                ServerParticleKind::Trail => ServerParticleOptions::Trail {
                    target: dvec3(2.0, 65.0, 1.0),
                    color: 0x55aa33,
                    duration: 10,
                },
                ServerParticleKind::Vibration => ServerParticleOptions::VibrationBlock {
                    target: BlockPos::new(2, 65, 1),
                    arrival_ticks: 10,
                },
                _ => ServerParticleOptions::Simple,
            };

            let spawn_pos = if kind == ServerParticleKind::NoxiousGasCloud {
                dvec3(8.5, 64.5, 8.5)
            } else {
                dvec3(0.5, 64.5, 0.5)
            };
            let packet_max_speed = if kind == ServerParticleKind::Bubble {
                0.0
            } else {
                1.0
            };
            let before = store.pending.len()
                + store.pending_emitters.len()
                + store.pending_firework_starters.len();
            store.add_particles_from_packet(
                kind,
                options,
                false,
                false,
                spawn_pos,
                DVec3::ZERO,
                packet_max_speed,
                1,
                spawn_pos,
                &block_registry,
                &chunks,
                &climate,
            );
            let after = store.pending.len()
                + store.pending_emitters.len()
                + store.pending_firework_starters.len();
            assert!(
                after > before,
                "{name} (id {id}, {kind:?}) produced no pending/live particle, emitter, or model state"
            );
            if kind == ServerParticleKind::Bubble {
                assert!(
                    store.pending.iter().any(|particle| matches!(
                        particle.kind,
                        Kind::Water(state) if state.kind == ServerParticleKind::Bubble
                    )),
                    "Bubble packet max_speed={packet_max_speed}, state={:?}, pos={spawn_pos:?} produced no Bubble particle",
                    chunks.get_block_state(0, 64, 0)
                );
            }
            store.particles.append(&mut store.pending);
            if kind == ServerParticleKind::NoxiousGasCloud {
                fastrand::seed(0x4e4f_5849_4f55_53);
            }
            if kind != ServerParticleKind::SulfurBubbles {
                store.tick(&chunks);
            }
            if kind == ServerParticleKind::Bubble {
                assert!(
                    store.particles.iter().any(|particle| matches!(
                        particle.kind,
                        Kind::Water(state) if state.kind == ServerParticleKind::Bubble
                    )),
                    "Bubble packet max_speed={packet_max_speed}, state={:?}, pos={spawn_pos:?} did not survive one tick",
                    chunks.get_block_state(0, 64, 0)
                );
            }
            if kind == ServerParticleKind::NoxiousGasCloud {
                // Java's cloud samples for children every second tick.
                store.tick(&chunks);
            } else if kind == ServerParticleKind::GustEmitterSmall {
                // Small gusts emit their first child after the provider's 2-tick delay.
                store.tick(&chunks);
                store.tick(&chunks);
            }
            let quads = store.extract(0.5, DVec3::ZERO, &chunks);
            if kind == ServerParticleKind::ElderGuardian {
                assert!(
                    store
                        .model_render_requests(0.5)
                        .iter()
                        .any(|request| matches!(
                            request,
                            super::emitters::RenderRequest::ElderGuardianModel { .. }
                        )),
                    "{name} did not reach the ElderGuardian model consumer"
                );
            } else if matches!(
                kind,
                ServerParticleKind::NoxiousGasCloud | ServerParticleKind::GustEmitterSmall
            ) {
                assert!(
                    store.particles.iter().any(|particle| matches!(
                        particle.kind,
                        super::Kind::Emitters(super::emitters::State { kind: emitter_kind, .. })
                            if emitter_kind == kind
                    )),
                    "{name} emitter did not survive its initial tick"
                );
            } else {
                assert!(
                    !quads.is_empty(),
                    "{name} (id {id}, {kind:?}) did not reach quad extraction after provider tick; packet max_speed={packet_max_speed}, state={:?}, pos={spawn_pos:?}",
                    chunks.get_block_state(0, 64, 0)
                );
                let camera = crate::renderer::camera::Camera::new(16.0 / 9.0);
                let (vertices, _) =
                    crate::renderer::pipelines::particle::build_particle_vertices(&camera, &quads);
                assert_eq!(
                    vertices.len(),
                    quads.len() * 6,
                    "{name} CPU vertex conversion"
                );
                let material = match kind {
                    ServerParticleKind::Block
                    | ServerParticleKind::BlockMarker
                    | ServerParticleKind::BlockCrumble
                    | ServerParticleKind::DustPillar => Some("fixture/block_particle"),
                    ServerParticleKind::Item
                    | ServerParticleKind::ItemSlime
                    | ServerParticleKind::ItemCobweb
                    | ServerParticleKind::ItemSnowball => Some("fixture/item_particle"),
                    _ => None,
                };
                if let Some(material) = material {
                    let region = store.uv_map.get_region(material);
                    assert!(
                        quads.iter().any(|quad| {
                            quad.u0 >= region.u_min
                                && quad.u1 <= region.u_max
                                && quad.v0 >= region.v_min
                                && quad.v1 <= region.v_max
                        }),
                        "{name} lost its terrain/item material or sampled outside its atlas region"
                    );
                }
                let child_descriptor = match kind {
                    ServerParticleKind::ExplosionEmitter => Some("explosion"),
                    ServerParticleKind::Geyser => Some("geyser_plume"),
                    ServerParticleKind::NoxiousGasCloud => Some("noxious_gas"),
                    ServerParticleKind::GustEmitterLarge => Some("gust"),
                    ServerParticleKind::GustEmitterSmall => Some("small_gust"),
                    _ => None,
                };
                let has_native_material = matches!(
                    kind,
                    ServerParticleKind::Block
                        | ServerParticleKind::BlockMarker
                        | ServerParticleKind::DustPillar
                        | ServerParticleKind::BlockCrumble
                        | ServerParticleKind::Item
                        | ServerParticleKind::ItemSlime
                        | ServerParticleKind::ItemCobweb
                        | ServerParticleKind::ItemSnowball
                );
                let sprite_descriptor = match kind {
                    ServerParticleKind::EntityEffect | ServerParticleKind::Effect => {
                        "entity_effect"
                    }
                    ServerParticleKind::InstantEffect | ServerParticleKind::Witch => {
                        "instant_effect"
                    }
                    ServerParticleKind::Totem => "end_rod",
                    _ => name,
                };
                let expected_descriptor = child_descriptor.unwrap_or(sprite_descriptor);
                if !has_native_material && kind != ServerParticleKind::ElderGuardian {
                    let expected = descriptor_uvs.get(expected_descriptor).unwrap_or_else(|| {
                        panic!("{name} reached a texture consumer without its Java descriptor")
                    });
                    assert!(
                        quads.iter().any(|quad| {
                            expected.iter().any(|&(u0, u1, v0, v1)| {
                                quad.u0 == u0 && quad.u1 == u1 && quad.v0 == v0 && quad.v1 == v1
                            })
                        }),
                        "{name} did not use its own descriptor frame or expected child material"
                    );
                }
            }
            let child_descriptor = match kind {
                ServerParticleKind::ExplosionEmitter => Some("explosion"),
                ServerParticleKind::Geyser => Some("geyser_plume"),
                ServerParticleKind::NoxiousGasCloud => Some("noxious_gas"),
                ServerParticleKind::GustEmitterLarge => Some("gust"),
                ServerParticleKind::GustEmitterSmall => Some("small_gust"),
                _ => None,
            };
            if let Some(descriptor) = child_descriptor {
                assert!(!quads.is_empty(), "{name} emitted no child quad");
                let expected = descriptor_uvs
                    .get(descriptor)
                    .unwrap_or_else(|| panic!("missing child descriptor {descriptor}"));
                assert!(
                    quads.iter().any(|quad| {
                        expected.iter().any(|&(u0, u1, v0, v1)| {
                            quad.u0 == u0 && quad.u1 == u1 && quad.v0 == v0 && quad.v1 == v1
                        })
                    }),
                    "{name} child did not use {descriptor}'s real Java descriptor frame"
                );
                let expected_child = match kind {
                    ServerParticleKind::ExplosionEmitter => store
                        .particles
                        .iter()
                        .any(|child| matches!(child.kind, super::Kind::Explosion)),
                    ServerParticleKind::Geyser => store.particles.iter().any(|child| {
                        matches!(
                            child.kind,
                            super::Kind::Emitters(super::emitters::State {
                                kind: ServerParticleKind::GeyserPlume,
                                ..
                            })
                        )
                    }),
                    ServerParticleKind::NoxiousGasCloud => store.particles.iter().any(|child| {
                        matches!(
                            child.kind,
                            super::Kind::Emitters(super::emitters::State {
                                kind: ServerParticleKind::NoxiousGas,
                                ..
                            })
                        )
                    }),
                    ServerParticleKind::GustEmitterLarge | ServerParticleKind::GustEmitterSmall => {
                        let expected_kind = if kind == ServerParticleKind::GustEmitterLarge {
                            ServerParticleKind::Gust
                        } else {
                            ServerParticleKind::SmallGust
                        };
                        store.particles.iter().any(|child| matches!(
                            child.kind,
                            super::Kind::Emitters(super::emitters::State { kind: child_kind, .. })
                                if child_kind == expected_kind
                        ))
                    }
                    _ => unreachable!(),
                };
                assert!(
                    expected_child,
                    "{name} did not produce the expected child kind"
                );
            }
            fixture_count += 1;
        }
        assert_eq!(fixture_count, 125);
        assert_eq!(seen_names.len(), 125);
        assert_eq!(seen_kinds.len(), 125);

        assert_eq!(descriptor_uvs.len(), 111);
        for (name, frames) in &descriptor_uvs {
            assert_eq!(
                frames.len(),
                descriptor_frame_count(name),
                "Java descriptor frame count for {name}"
            );
            for &(u0, u1, v0, v1) in frames {
                assert!(0.0 <= u0 && u0 < u1 && u1 <= 1.0);
                assert!(0.0 <= v0 && v0 < v1 && v1 <= 1.0);
            }
        }
    }

    #[test]
    fn particle_asset_reload_replaces_transient_state_and_uses_new_ordered_frames() {
        use std::collections::HashMap;
        use std::sync::Arc;

        use glam::DVec3;

        use super::{ParticleMode, ParticleStore, ServerParticleOptions};
        use crate::renderer::chunk::mesher::Colormap;

        let region = |sprite: u16, u_min: f32| AtlasRegion {
            u_min,
            v_min: 0.1,
            u_max: u_min + 0.1,
            v_max: 0.2,
            pixel_rect: [u16::from(sprite), 0, 1, 1],
            sprite,
            opaque: true,
            translucent: false,
            alpha_counts: [0, 0, 1],
        };
        let mut old_atlas = AtlasUVMap::test_empty();
        for (name, sprite, u_min) in [("particle/old_a", 1, 0.1), ("particle/old_b", 2, 0.2)] {
            old_atlas.test_insert_region(name, region(sprite, u_min));
        }
        old_atlas.test_insert_particle_sprites(
            "end_rod",
            vec!["particle/old_a".into(), "particle/old_b".into()],
        );
        let mut new_atlas = AtlasUVMap::test_empty();
        for (name, sprite, u_min) in [
            ("particle/new_a", 3, 0.6),
            ("particle/new_b", 4, 0.7),
            ("particle/new_c", 5, 0.8),
        ] {
            new_atlas.test_insert_region(name, region(sprite, u_min));
        }
        new_atlas.test_insert_particle_sprites(
            "end_rod",
            vec![
                "particle/new_a".into(),
                "particle/new_b".into(),
                "particle/new_c".into(),
            ],
        );
        let old_colors = Arc::new(Colormap::test_empty());
        let new_colors = Arc::new(Colormap::test_empty());
        let mut store = ParticleStore::new(
            old_atlas,
            old_colors.clone(),
            old_colors.clone(),
            old_colors.clone(),
        );
        store.set_mode(ParticleMode::Decreased);
        store.particles.push(Particle::end_rod(
            DVec3::ZERO,
            DVec3::ZERO,
            &store.end_rod_frames,
        ));
        super::emitters::spawn(
            &mut store,
            ServerParticleKind::ElderGuardian,
            ServerParticleOptions::Simple,
            DVec3::ZERO,
            DVec3::ZERO,
            &crate::world::block::registry::BlockRegistry::test_empty(),
            &ChunkStore::new(2),
            &HashMap::new(),
        );
        store.particles.append(&mut store.pending);
        assert_eq!(store.model_render_requests(0.0).len(), 1);
        store.pending.push(Particle::end_rod(
            DVec3::ZERO,
            DVec3::ZERO,
            &store.end_rod_frames,
        ));
        store.emitters.push(super::ExplosionEmitter {
            pos: DVec3::ZERO,
            age: 0,
        });
        store.pending_emitters.push(super::ExplosionEmitter {
            pos: DVec3::ZERO,
            age: 0,
        });
        store.firework_starters.push(super::FireworkStarter {
            request: super::FireworkStarterRequest {
                position: DVec3::ZERO,
                velocity: DVec3::ZERO,
                far_effect: false,
                explosions: vec![],
            },
            life: 0,
            lifetime: 1,
            twinkle_delay: false,
        });
        store
            .pending_firework_starters
            .push(super::FireworkStarter {
                request: super::FireworkStarterRequest {
                    position: DVec3::ZERO,
                    velocity: DVec3::ZERO,
                    far_effect: false,
                    explosions: vec![],
                },
                life: 0,
                lifetime: 1,
                twinkle_delay: false,
            });
        store.tracked_explosions.push(TrackedExplosion {
            center: DVec3::ZERO,
            radius: 1.0,
            block_count: 1,
            block_particles: vec![],
        });
        store.tracking_emitters.push(super::TrackingEmitter {
            entity_id: Some(1),
            attachment: super::TrackingAttachment {
                position: DVec3::ZERO,
                width: 1.0,
                height: 1.0,
            },
            age: 1,
            kind: super::TrackingParticleKind::Crit,
        });
        store.sound_requests.push(super::ParticleSoundRequest {
            event: "entity.firework_rocket.blast",
            pos: DVec3::ZERO,
            volume: 1.0,
            pitch: 1.0,
            seed: 1,
        });

        store.reload_assets(
            new_atlas,
            new_colors.clone(),
            new_colors.clone(),
            new_colors.clone(),
        );

        assert!(Arc::ptr_eq(&store.grass_colormap, &new_colors));
        assert!(Arc::ptr_eq(&store.foliage_colormap, &new_colors));
        assert!(Arc::ptr_eq(&store.dry_foliage_colormap, &new_colors));
        assert!(!Arc::ptr_eq(&store.grass_colormap, &old_colors));
        assert_eq!(store.mode, ParticleMode::Decreased);
        assert!(store.particles.is_empty() && store.pending.is_empty());
        assert!(store.emitters.is_empty() && store.pending_emitters.is_empty());
        assert!(store.firework_starters.is_empty() && store.pending_firework_starters.is_empty());
        assert!(store.tracked_explosions.is_empty() && store.tracking_emitters.is_empty());
        assert!(store.sound_requests.is_empty());
        assert!(store.model_render_requests(0.0).is_empty());
        let frames = store.uv_map.particle_sprite_names("end_rod").unwrap();
        assert_eq!(
            frames.iter().map(String::as_str).collect::<Vec<_>>(),
            ["particle/new_a", "particle/new_b", "particle/new_c"]
        );
        assert_eq!(store.end_rod_frames[0].u_min, 0.6);
        assert_eq!(store.end_rod_frames[1].u_min, 0.7);

        let registry = crate::world::block::registry::BlockRegistry::test_empty();
        let chunks = ChunkStore::new(2);
        store.add_particles_from_packet(
            ServerParticleKind::EndRod,
            ServerParticleOptions::Simple,
            true,
            false,
            DVec3::ZERO,
            DVec3::ZERO,
            1.0,
            0,
            DVec3::ZERO,
            &registry,
            &chunks,
            &HashMap::new(),
        );
        let spawned = store.pending.last().unwrap();
        assert!((spawned.u0 - 0.6).abs() < f32::EPSILON);
        assert!((spawned.u1 - 0.7).abs() < f32::EPSILON);
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

    #[test]
    fn dust_color_transition_uses_java_lifetime_plus_one_interpolation() {
        assert_eq!(
            dust_transition_color([0.0, 0.25, 1.0], [1.0, 0.75, 0.0], 5, 9, 0.0),
            [0.5, 0.5, 0.5]
        );
        assert_eq!(
            dust_transition_color([0.0; 3], [1.0; 3], 9, 9, 0.0),
            [0.9; 3]
        );
    }

    #[test]
    fn dust_color_transition_packet_constructs_transition_particle() {
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut uv = AtlasUVMap::test_empty();
        uv.test_insert_region(
            super::DUST_SPRITE,
            AtlasRegion {
                u_min: 0.1,
                v_min: 0.1,
                u_max: 0.2,
                v_max: 0.2,
                pixel_rect: [0; 4],
                sprite: 1,
                opaque: true,
                translucent: false,
                alpha_counts: [0, 0, 1],
            },
        );
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        let chunks = ChunkStore::new(2);
        store.add_particles_from_packet(
            ServerParticleKind::DustColorTransition,
            super::ServerParticleOptions::DustColorTransition {
                from_color: 0x00ff_0000,
                to_color: 0x0000_00ff,
                scale: 1.0,
            },
            true,
            false,
            dvec3(0.0, 0.0, 0.0),
            dvec3(0.0, 0.0, 0.0),
            0.0,
            0,
            dvec3(0.0, 0.0, 0.0),
            &crate::world::block::registry::BlockRegistry::test_empty(),
            &chunks,
            &Default::default(),
        );
        assert_eq!(store.pending.len(), 1);
        assert!(matches!(
            store.pending[0].kind,
            super::Kind::DustColorTransition
        ));
        assert!(store.pending[0].target.is_some());
        assert_eq!(store.pending[0].u0, 0.1);
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
    fn store_mode_change_affects_the_next_filtered_spawn() {
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;

        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        store.set_mode(super::ParticleMode::Minimal);
        assert!(!store.accepts_normal_spawn(false, &mut || 1));
        store.set_mode(super::ParticleMode::All);
        assert!(store.accepts_normal_spawn(false, &mut || 1));
    }

    #[test]
    fn campfire_always_visible_spawns_bypass_the_selected_status() {
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;

        let uv = AtlasUVMap::test_empty();
        let colors = Arc::new(Colormap::test_empty());
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        store.set_mode(super::ParticleMode::Minimal);
        store.add_campfire_smoke(dvec3(0.0, 0.0, 0.0), false);
        assert_eq!(store.pending.len(), 1);
    }

    #[test]
    fn tracking_store_ticks_three_batches_and_detach_preserves_old_id_snapshot() {
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
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
    }

    #[test]
    fn explosion_packet_options_use_shared_typed_particle_spawn_boundary() {
        use azalea_core::color::RgbColor;
        use azalea_core::position::BlockPos;
        use azalea_entity::particle::{
            BlockParticle, ColorPowerParticle, DustParticle, ItemParticle, TrailParticle,
            VibrationParticle,
        };

        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut uv = AtlasUVMap::test_empty();
        uv.test_insert_particle_sprites("gust", vec!["particle/gust_test".into()]);
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = super::ParticleStore::new(uv, colors.clone(), colors.clone(), colors);
        let mut registry = crate::world::block::registry::BlockRegistry::test_empty();
        registry.test_add_particle_fixture();
        let chunks = ChunkStore::new(2);
        let climate = std::collections::HashMap::new();
        let pos = dvec3(2.0, 64.0, 3.0);
        let camera = pos;

        let cases = [
            (ParticleOptions::Gust, ServerParticleKind::Gust),
            (
                ParticleOptions::GustEmitterLarge,
                ServerParticleKind::GustEmitterLarge,
            ),
            (
                ParticleOptions::GustEmitterSmall,
                ServerParticleKind::GustEmitterSmall,
            ),
            (
                ParticleOptions::Dust(DustParticle {
                    color: RgbColor::new(20, 40, 60),
                    scale: 1.0,
                }),
                ServerParticleKind::Dust,
            ),
            (
                ParticleOptions::Effect(ColorPowerParticle {
                    color: 0x123456,
                    power: 1.0,
                }),
                ServerParticleKind::Effect,
            ),
            (
                ParticleOptions::Block(BlockParticle {
                    block_state: crate::world::block::find_state("stone", &[]),
                }),
                ServerParticleKind::Block,
            ),
            (
                ParticleOptions::Item(ItemParticle {
                    item: azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData {
                        kind: azalea_registry::builtin::ItemKind::Stone,
                        count: 3,
                        component_patch: azalea_inventory::DataComponentPatch::default(),
                    }),
                }),
                ServerParticleKind::Item,
            ),
            (
                ParticleOptions::Trail(Box::new(TrailParticle {
                    target: azalea_core::position::Vec3 {
                        x: 5.0,
                        y: 6.0,
                        z: 7.0,
                    },
                    color: 0x0012_3456,
                    duration: 17,
                })),
                ServerParticleKind::Trail,
            ),
            (
                ParticleOptions::Vibration(Box::new(VibrationParticle {
                    position: azalea_entity::particle::PositionSource::Block(BlockPos::new(
                        4, 65, 6,
                    )),
                    ticks: 23,
                })),
                ServerParticleKind::Vibration,
            ),
        ];
        for (option, expected) in cases {
            store.pending.clear();
            assert!(store.add_explosion_packet_particle(
                &option,
                pos,
                dvec3(1.0, 0.0, 0.0),
                camera,
                &registry,
                &chunks,
                &climate,
            ));
            assert_eq!(store.test_pending().len(), 1, "{expected:?}");
            assert_eq!(store.test_pending()[0].pos, pos, "{expected:?}");
        }

        // Existing packet options remain routed through the same providers.
        for option in [
            ParticleOptions::Explosion,
            ParticleOptions::EndRod,
            ParticleOptions::Poof,
            ParticleOptions::Smoke,
        ] {
            store.pending.clear();
            assert!(store.add_explosion_packet_particle(
                &option,
                pos,
                dvec3(0.0, 1.0, 0.0),
                camera,
                &registry,
                &chunks,
                &climate,
            ));
            assert_eq!(store.test_pending().len(), 1);
        }
        store.pending_emitters.clear();
        assert!(store.add_explosion_packet_particle(
            &ParticleOptions::ExplosionEmitter,
            pos,
            glam::DVec3::ZERO,
            camera,
            &registry,
            &chunks,
            &climate,
        ));
        assert_eq!(store.pending_emitters.len(), 1);

        store.pending.clear();
        store.track_explosion_effects(
            pos,
            1.0,
            1,
            vec![super::Weighted {
                value: super::ExplosionParticleInfo {
                    particle: ParticleOptions::Gust,
                    scaling: 1.0,
                    speed: 1.0,
                },
                weight: 1,
            }],
        );
        store.spawn_tracked_explosion_particles(camera, &registry, &chunks, &climate);
        assert_eq!(
            store.test_pending().len(),
            1,
            "weighted GUST reaches its provider"
        );

        store.track_explosion_effects(pos, 1.0, 1, Vec::new());
        assert!(
            store.tracked_explosions.is_empty(),
            "empty weighted lists are ignored"
        );
        store.track_explosion_effects(
            pos,
            1.0,
            1,
            vec![super::Weighted {
                value: super::ExplosionParticleInfo {
                    particle: ParticleOptions::Gust,
                    scaling: 1.0,
                    speed: 1.0,
                },
                weight: 0,
            }],
        );
        store.pending.clear();
        store.spawn_tracked_explosion_particles(camera, &registry, &chunks, &climate);
        assert!(
            store.test_pending().is_empty(),
            "zero-weight options are not selected"
        );
    }

    #[test]
    fn poof_starts_not_fullbright_and_refreshes_world_light_on_tick() {
        let _protocol = crate::world::block::test_protocol_guard();
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
        let _protocol = crate::world::block::test_protocol_guard();
        let generic_frames: [super::AtlasRegion; 12] =
            std::array::from_fn(|i| super::AtlasRegion {
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
    fn particle_mode_roundtrips_and_invalid_values_default_all() {
        for (value, mode) in [
            (0, super::ParticleMode::All),
            (1, super::ParticleMode::Decreased),
            (2, super::ParticleMode::Minimal),
        ] {
            assert_eq!(super::ParticleMode::from_u8(value), mode);
            assert_eq!(super::ParticleMode::from_u8(value).to_u8(), value);
        }
        assert_eq!(
            super::ParticleMode::from_u8(u8::MAX),
            super::ParticleMode::All
        );
    }

    #[test]
    fn native_status_filter_respects_sampling_bypass_and_distance() {
        use super::{ParticleMode as Mode, accept_particle};
        let mut rolls = [0, 1, 2].into_iter();
        assert!(accept_particle(
            Mode::All,
            false,
            false,
            1024.0,
            &mut || rolls.next().unwrap()
        ));
        assert!(!accept_particle(
            Mode::All,
            false,
            false,
            1024.01,
            &mut || 1
        ));
        assert!(!accept_particle(
            Mode::Minimal,
            false,
            false,
            0.0,
            &mut || 1
        ));
        // Decreased keeps two of each three deterministic rolls.
        assert!(accept_particle(
            Mode::Decreased,
            false,
            false,
            0.0,
            &mut || 1
        ));
        assert!(accept_particle(
            Mode::Decreased,
            false,
            false,
            0.0,
            &mut || 2
        ));
        assert!(!accept_particle(
            Mode::Decreased,
            false,
            false,
            0.0,
            &mut || 0
        ));
        // Override/long-distance bypasses both distance and status checks.
        let mut draws = 0;
        assert!(accept_particle(
            Mode::Minimal,
            true,
            false,
            4096.0,
            &mut || {
                draws += 1;
                0
            }
        ));
        assert_eq!(draws, 0);
        // Java computes particle status (and consumes its RNG) before bypassing.
        let mut rolls = [0, 1].into_iter();
        assert!(accept_particle(
            Mode::Minimal,
            true,
            true,
            4096.0,
            &mut || rolls.next().unwrap()
        ));
        assert_eq!(rolls.next(), None);
        // addAlwaysVisibleParticle grants the native 10% Minimal fallback to Decreased.
        let mut rolls = [0, 1].into_iter();
        assert!(accept_particle(
            Mode::Minimal,
            false,
            true,
            0.0,
            &mut || rolls.next().unwrap()
        ));
        let mut rolls = [1].into_iter();
        assert!(!accept_particle(
            Mode::Minimal,
            false,
            true,
            0.0,
            &mut || rolls.next().unwrap()
        ));
    }

    #[test]
    fn server_particle_count_keeps_negative_distinct_from_directional_zero() {
        assert_eq!(packet_particle_count(-1), None);
        assert_eq!(packet_particle_count(0), Some(0));
        assert_eq!(packet_particle_count(3), Some(3));
    }

    #[test]
    fn particle_override_limiter_matches_java_26_2_registry_flags() {
        use pomme_protocol::{ClientRegistry, RegistryTable};

        let registry = RegistryTable::native();
        let overridden = [
            "block_marker",
            "geyser",
            "geyser_base",
            "geyser_poof",
            "geyser_plume",
            "damage_indicator",
            "elder_guardian",
            "explosion_emitter",
            "explosion",
            "gust",
            "gust_emitter_large",
            "gust_emitter_small",
            "sonic_boom",
            "sculk_charge",
            "sculk_charge_pop",
            "poof",
            "spit",
            "squid_ink",
            "sweep_attack",
            "campfire_cosy_smoke",
            "campfire_signal_smoke",
            "glow_squid_ink",
            "glow",
            "wax_on",
            "wax_off",
            "electric_spark",
            "scrape",
            "trial_spawner_detection",
            "trial_spawner_detection_ominous",
            "vault_connection",
            "ominous_spawning",
            "vibration",
        ];
        for name in overridden {
            let id = registry.id_of(ClientRegistry::ParticleType, name).unwrap();
            assert!(
                ServerParticleKind::from_id(id).unwrap().override_limiter(),
                "{name}"
            );
        }
    }

    #[test]
    fn server_particle_ids_match_26_2_registry_for_supported_kinds() {
        use pomme_protocol::{ClientRegistry, RegistryTable};

        use super::ServerParticleKind as Kind;

        let registry = RegistryTable::native();
        let supported = [
            ("end_rod", Kind::EndRod),
            ("effect", Kind::Effect),
            ("entity_effect", Kind::EntityEffect),
            ("instant_effect", Kind::InstantEffect),
            ("witch", Kind::Witch),
            ("raid_omen", Kind::RaidOmen),
            ("trial_omen", Kind::TrialOmen),
            ("explosion_emitter", Kind::ExplosionEmitter),
            ("explosion", Kind::Explosion),
            ("poof", Kind::Poof),
            ("smoke", Kind::Smoke),
            ("campfire_cosy_smoke", Kind::CampfireCosySmoke),
            ("campfire_signal_smoke", Kind::CampfireSignalSmoke),
            ("totem_of_undying", Kind::Totem),
            ("dust", Kind::Dust),
            ("block", Kind::Block),
            ("item", Kind::Item),
            ("shriek", Kind::Shriek),
            ("trail", Kind::Trail),
            ("vibration", Kind::Vibration),
        ];

        for (name, expected_kind) in supported {
            let id = registry
                .id_of(ClientRegistry::ParticleType, name)
                .unwrap_or_else(|| panic!("native particle registry is missing {name}"));
            assert_eq!(
                Kind::from_id(id),
                Some(expected_kind),
                "native particle {name} at id {id}"
            );
        }

        for (id, expected_name) in [(70, "white_smoke"), (71, "sneeze")] {
            assert_eq!(
                registry.name_of(ClientRegistry::ParticleType, id),
                Some(expected_name)
            );
            assert!(!matches!(
                Kind::from_id(id),
                Some(Kind::CampfireCosySmoke | Kind::CampfireSignalSmoke)
            ));
        }
        assert!(Kind::CampfireCosySmoke.override_limiter());
        assert!(Kind::CampfireSignalSmoke.override_limiter());
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
