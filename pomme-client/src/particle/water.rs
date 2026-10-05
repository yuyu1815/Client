//! Java 26.2 water, fluid, squid-ink and drip particle providers.
use std::collections::HashMap;

use glam::{DVec3, dvec3};

use super::{
    Appearance, Kind, Particle, ParticleStore, ServerParticleKind as K, ServerParticleOptions,
    descriptor_age_frame, descriptor_frame, descriptor_random_frame,
};
use crate::physics::aabb::Aabb;
use crate::physics::block_shape::{self, LocalBox};
use crate::physics::collision::resolve_collision;
use crate::renderer::chunk::atlas::AtlasUVMap;
use crate::renderer::chunk::mesher::{BiomeClimate, world_brightness};
use crate::world::block::registry::BlockRegistry;
use crate::world::block::{self, FluidKind};
use crate::world::chunk::ChunkStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct State {
    pub(super) kind: K,
}

pub(super) fn translucent(state: State) -> bool {
    matches!(state.kind, K::SquidInk | K::GlowSquidInk)
}

pub(super) fn supports(kind: K) -> bool {
    matches!(
        kind,
        K::Bubble
            | K::SulfurBubbles
            | K::Fishing
            | K::Rain
            | K::Splash
            | K::BubblePop
            | K::CurrentDown
            | K::BubbleColumnUp
            | K::Nautilus
            | K::Dolphin
            | K::SquidInk
            | K::GlowSquidInk
            | K::Lava
            | K::DrippingLava
            | K::FallingLava
            | K::LandingLava
            | K::DrippingWater
            | K::FallingWater
            | K::DrippingHoney
            | K::FallingHoney
            | K::LandingHoney
            | K::FallingNectar
            | K::DrippingObsidianTear
            | K::FallingObsidianTear
            | K::LandingObsidianTear
            | K::DrippingDripstoneLava
            | K::FallingDripstoneLava
            | K::DrippingDripstoneWater
            | K::FallingDripstoneWater
            | K::Spit
    )
}

fn kind_name(kind: K) -> &'static str {
    match kind {
        K::Bubble => "bubble",
        K::SulfurBubbles => "sulfur_bubbles",
        K::Fishing => "fishing",
        K::Rain => "rain",
        K::Splash => "splash",
        K::BubblePop => "bubble_pop",
        K::CurrentDown => "current_down",
        K::BubbleColumnUp => "bubble_column_up",
        K::Nautilus => "nautilus",
        K::Dolphin => "dolphin",
        K::SquidInk => "squid_ink",
        K::GlowSquidInk => "glow_squid_ink",
        K::Lava => "lava",
        K::DrippingLava => "dripping_lava",
        K::FallingLava => "falling_lava",
        K::LandingLava => "landing_lava",
        K::DrippingWater => "dripping_water",
        K::FallingWater => "falling_water",
        K::DrippingHoney => "dripping_honey",
        K::FallingHoney => "falling_honey",
        K::LandingHoney => "landing_honey",
        K::FallingNectar => "falling_nectar",
        K::DrippingObsidianTear => "dripping_obsidian_tear",
        K::FallingObsidianTear => "falling_obsidian_tear",
        K::LandingObsidianTear => "landing_obsidian_tear",
        K::DrippingDripstoneLava => "dripping_dripstone_lava",
        K::FallingDripstoneLava => "falling_dripstone_lava",
        K::DrippingDripstoneWater => "dripping_dripstone_water",
        K::FallingDripstoneWater => "falling_dripstone_water",
        K::Spit => "spit",
        _ => unreachable!("water family kind"),
    }
}

fn color(kind: K) -> [f32; 3] {
    match kind {
        K::DrippingDripstoneWater | K::FallingDripstoneWater => [0.2, 0.3, 1.0],
        K::DrippingDripstoneLava
        | K::FallingDripstoneLava
        | K::DrippingLava
        | K::FallingLava
        | K::LandingLava => [1.0, 0.2857143, 0.083333336],
        K::DrippingHoney => [0.622, 0.508, 0.082],
        K::FallingHoney => [0.582, 0.448, 0.082],
        K::LandingHoney => [0.522, 0.408, 0.082],
        K::FallingNectar => [0.92, 0.782, 0.72],
        K::DrippingObsidianTear | K::FallingObsidianTear | K::LandingObsidianTear => {
            [0.51171875, 0.03125, 0.890625]
        }
        K::Dolphin => [0.3, 0.5, 1.0],
        K::SquidInk => [0.0; 3],
        K::GlowSquidInk => [0.2, 0.8, 0.6],
        _ => [1.0; 3],
    }
}

fn lifetime(kind: K) -> i32 {
    let r = fastrand::f32();
    match kind {
        K::Bubble => (8.0 / (r * 0.8 + 0.2)) as i32,
        K::BubbleColumnUp => (40.0 / (r * 0.8 + 0.2)) as i32,
        K::SulfurBubbles => i32::MAX,
        K::Fishing | K::Rain | K::Splash => (8.0 / (r * 0.8 + 0.2)) as i32,
        K::BubblePop => 4,
        K::CurrentDown => (r * 60.0) as i32 + 30,
        K::Nautilus => (r * 10.0) as i32 + 30,
        K::Dolphin => ((20.0 / (r * 0.8 + 0.2)) as i32) / 2,
        K::SquidInk | K::GlowSquidInk => (6.0 / (r * 0.8 + 0.2)) as i32,
        K::Lava => (16.0 / (r * 0.8 + 0.2)) as i32,
        K::DrippingWater
        | K::DrippingLava
        | K::DrippingDripstoneWater
        | K::DrippingDripstoneLava => 40,
        K::DrippingHoney | K::DrippingObsidianTear => 100,
        K::FallingHoney
        | K::FallingLava
        | K::FallingObsidianTear
        | K::FallingDripstoneLava
        | K::FallingDripstoneWater => (64.0 / (r * 0.8 + 0.2)) as i32,
        K::FallingWater => (4.0 / (r * 0.9 + 0.1)) as i32,
        K::LandingHoney => (128.0 / (r * 0.8 + 0.2)) as i32,
        K::FallingNectar => (16.0 / (r * 0.8 + 0.2)) as i32,
        K::LandingObsidianTear => (28.0 / (r * 0.8 + 0.2)) as i32,
        K::LandingLava => (16.0 / (r * 0.8 + 0.2)) as i32,
        K::Spit => (16.0 / (r * 0.8 + 0.2)) as i32 + 2,
        _ => 30,
    }
}

pub(super) fn spawn(
    store: &mut ParticleStore,
    kind: K,
    options: ServerParticleOptions,
    pos: DVec3,
    vel: DVec3,
    _registry: &BlockRegistry,
    chunks: &ChunkStore,
    _biome_climate: &HashMap<u32, BiomeClimate>,
) -> bool {
    if !supports(kind) || !matches!(options, ServerParticleOptions::Simple) {
        return false;
    }
    let descriptor = format!("minecraft:{}", kind_name(kind));
    let count = store
        .uv_map
        .particle_sprite_names(&descriptor)
        .map_or(1, <[_]>::len)
        .max(1);
    let frame_index = if matches!(
        kind,
        K::Fishing | K::SquidInk | K::GlowSquidInk | K::BubblePop | K::Spit
    ) {
        0
    } else {
        descriptor_random_frame(count)
    };
    let sprite = descriptor_frame(&store.uv_map, &descriptor, frame_index)
        .unwrap_or_else(|| store.uv_map.missing_region());
    let mut p = Particle::special(
        Kind::Water(State { kind }),
        pos,
        vel,
        None,
        lifetime(kind),
        0.1,
        sprite,
    );
    p.delay = frame_index as i32;
    p.color = color(kind);
    p.gravity = gravity(kind);
    p.friction = friction(kind);
    p.alpha = match kind {
        K::Dolphin => 1.0 - fastrand::f32() * 0.7,
        _ => 1.0,
    };
    p.light = if matches!(
        kind,
        K::Lava
            | K::SquidInk
            | K::GlowSquidInk
            | K::DrippingObsidianTear
            | K::FallingObsidianTear
            | K::LandingObsidianTear
    ) {
        1.0
    } else {
        world_brightness(
            chunks,
            pos.x.floor() as i32,
            pos.y.floor() as i32,
            pos.z.floor() as i32,
        )
    };
    if is_drip(kind) {
        p.vel = DVec3::ZERO;
        p.size = 0.1;
        p.base_size = p.size;
    }
    match kind {
        K::Bubble | K::BubbleColumnUp => {
            p.size = 0.1 * (fastrand::f32() * 0.6 + 0.2);
            p.base_size = p.size;
            p.vel = vel * 0.2
                + dvec3(
                    rand_signed() * 0.02,
                    rand_signed() * 0.02,
                    rand_signed() * 0.02,
                );
        }
        K::SulfurBubbles => {
            p.gravity = -0.04;
            p.size = 0.02 + 0.02 * fastrand::f32();
            p.base_size = p.size;
            p.vel = dvec3(
                vel.x * 0.2 + rand_signed() * 0.02,
                0.0,
                vel.z * 0.2 + rand_signed() * 0.02,
            );
            p.target = Some(pos + dvec3(0.0, 3.0, 0.0));
            p.pitch = pos.y as f32;
        }
        K::Fishing => {
            p.vel = vel;
            p.gravity = 0.0;
            p.size = 0.1;
            p.base_size = p.size;
        }
        K::Rain => {
            p.vel = dvec3(0.0, f64::from(fastrand::f32() * 0.2 + 0.1), 0.0);
            p.gravity = 0.06;
            p.size = 0.1;
            p.base_size = p.size;
        }
        K::Splash => {
            p.gravity = 0.04;
            p.size = 0.1;
            p.base_size = p.size;
            if vel.y == 0.0 && (vel.x != 0.0 || vel.z != 0.0) {
                p.vel = dvec3(vel.x, 0.1, vel.z);
            } else {
                p.vel = dvec3(0.0, f64::from(fastrand::f32() * 0.2 + 0.1), 0.0);
            }
        }
        K::BubblePop => {
            p.gravity = 0.008;
            p.vel = vel;
        }
        K::CurrentDown => {
            p.vel = dvec3(0.0, -0.05, 0.0);
            p.gravity = 0.002;
            p.rot = 0.0;
            p.size = 0.1 * (fastrand::f32() * 0.6 + 0.2);
            p.base_size = p.size;
        }
        K::Nautilus => {
            p.prev_pos = pos + vel;
            p.pos = p.prev_pos;
            p.target = Some(pos);
            p.pitch = fastrand::f32() * 0.6 + 0.4;
            p.color = [0.9 * p.pitch, 0.9 * p.pitch, p.pitch];
            p.size = 0.1 * (fastrand::f32() * 0.5 + 0.2);
            p.base_size = p.size;
            p.gravity = 0.0;
            p.friction = 1.0;
        }
        K::Dolphin => {
            let mut direction = vel
                + dvec3(
                    rand_signed() * 0.4,
                    rand_signed() * 0.4,
                    rand_signed() * 0.4,
                );
            let speed = f64::from(fastrand::f32() + fastrand::f32() + 1.0) * 0.15;
            direction = direction / direction.length().max(f64::MIN_POSITIVE) * speed * 0.4;
            p.vel = direction * 0.02;
            p.size = 0.1 * (fastrand::f32() * 0.6 + 0.5);
            p.base_size = p.size;
            p.gravity = 0.0;
        }
        K::SquidInk | K::GlowSquidInk => {
            p.size = 0.5;
            p.base_size = 0.5;
            p.vel = vel;
            p.gravity = 0.0;
            p.friction = 0.92;
            p.alpha = 1.0;
        }
        K::Lava => {
            p.vel = dvec3(0.0, f64::from(fastrand::f32() * 0.4 + 0.05), 0.0);
            p.gravity = 0.75;
            p.friction = 0.999;
            p.size = 0.1 * (fastrand::f32() * 2.0 + 0.2);
            p.base_size = p.size;
        }
        K::DrippingHoney => {
            p.vel = DVec3::ZERO;
            p.gravity *= 0.0002;
        }
        K::DrippingObsidianTear => {
            p.vel = DVec3::ZERO;
            p.gravity *= 0.0002;
        }
        K::DrippingDripstoneWater
        | K::DrippingDripstoneLava
        | K::DrippingLava
        | K::DrippingWater => {
            p.vel = DVec3::ZERO;
            p.gravity *= 0.02;
        }
        K::FallingHoney => p.gravity = 0.01,
        K::FallingNectar => p.gravity = 0.007,
        K::FallingObsidianTear => p.gravity = 0.01,
        K::Spit => {
            p.gravity = 0.5;
            p.friction = 0.9;
            p.vel = vel
                + dvec3(
                    rand_signed() * 0.05,
                    rand_signed() * 0.05,
                    rand_signed() * 0.05,
                );
            let r = fastrand::f32() * 0.3 + 0.7;
            p.color = [r; 3];
            p.size = 0.1 * (fastrand::f32() * fastrand::f32() * 6.0 + 1.0);
            p.base_size = p.size;
        }
        _ => {}
    }
    if matches!(kind, K::DrippingWater | K::FallingWater) {
        p.color = [1.0; 3];
    }
    store.push(p);
    true
}

fn rand_signed() -> f64 {
    f64::from(fastrand::f32() * 2.0 - 1.0)
}
fn gravity(k: K) -> f64 {
    if is_drip(k) { 0.06 } else { 0.0 }
}
fn friction(k: K) -> f64 {
    match k {
        K::Bubble | K::BubbleColumnUp | K::SulfurBubbles => 0.85,
        K::Fishing | K::Rain | K::Splash => 0.98,
        K::Dolphin => 0.99,
        K::SquidInk | K::GlowSquidInk => 0.92,
        K::Lava => 0.999,
        K::Spit => 0.9,
        _ => 0.98,
    }
}
fn is_drip(k: K) -> bool {
    matches!(
        k,
        K::DrippingLava
            | K::FallingLava
            | K::LandingLava
            | K::DrippingWater
            | K::FallingWater
            | K::DrippingHoney
            | K::FallingHoney
            | K::LandingHoney
            | K::FallingNectar
            | K::DrippingObsidianTear
            | K::FallingObsidianTear
            | K::LandingObsidianTear
            | K::DrippingDripstoneLava
            | K::FallingDripstoneLava
            | K::DrippingDripstoneWater
            | K::FallingDripstoneWater
    )
}

/// Java DripParticle's `type`: only water and lava providers participate in
/// same-fluid removal; honey, nectar, and obsidian-tear providers use EMPTY.
fn drip_fluid_type(k: K) -> Option<FluidKind> {
    match k {
        K::DrippingWater
        | K::FallingWater
        | K::DrippingDripstoneWater
        | K::FallingDripstoneWater => Some(FluidKind::Water),
        K::DrippingLava
        | K::FallingLava
        | K::LandingLava
        | K::DrippingDripstoneLava
        | K::FallingDripstoneLava => Some(FluidKind::Lava),
        K::DrippingHoney
        | K::FallingHoney
        | K::LandingHoney
        | K::FallingNectar
        | K::DrippingObsidianTear
        | K::FallingObsidianTear
        | K::LandingObsidianTear => None,
        _ => None,
    }
}

fn fluid_at(chunks: &ChunkStore, p: DVec3) -> (FluidKind, f64, bool) {
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let state = chunks.get_block_state(x, y, z);
    let f = block::fluid(state);
    (f.kind, y as f64 + f64::from(f.height()), f.is_source())
}
fn collision_height(state: azalea_block::BlockState, x: f64, z: f64) -> f64 {
    let boxes: &[LocalBox] =
        block_shape::partial_shape(state).unwrap_or(&[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]);
    boxes
        .iter()
        .filter(|b| x >= b[0] && x <= b[3] && z >= b[2] && z <= b[5])
        .map(|b| b[4])
        .fold(0.0, f64::max)
}
fn reached_surface(chunks: &ChunkStore, pos: DVec3) -> bool {
    let (x, y, z) = (
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    let state = chunks.get_block_state(x, y, z);
    let f = block::fluid(state);
    let solid = collision_height(state, pos.x - x as f64, pos.z - z as f64);
    let top = solid.max(if f.kind == FluidKind::Empty {
        0.0
    } else {
        f64::from(f.height())
    });
    top > 0.0 && pos.y < y as f64 + top
}
fn move_particle(p: &mut Particle, chunks: &ChunkStore, physics: bool) {
    p.on_ground = false;
    if !physics {
        p.pos += p.vel;
        return;
    }
    let orig = p.vel;
    let mut delta = orig;
    if orig != DVec3::ZERO && orig.length_squared() < 10000.0 {
        let half_width = match p.kind {
            Kind::Water(State {
                kind: K::Spit | K::BubblePop | K::Lava,
            }) => 0.1,
            Kind::Water(State {
                kind: K::Bubble | K::BubbleColumnUp | K::CurrentDown | K::SulfurBubbles,
            }) => 0.01,
            _ => 0.005,
        };
        (delta, _) = resolve_collision(
            chunks,
            Aabb::from_center(p.pos, half_width, half_width),
            orig.into(),
            0.0,
        );
    }
    p.pos += delta;
    p.on_ground = orig.y != delta.y && orig.y < 0.0;
    if (orig.x - delta.x).abs() > 1e-8 {
        p.vel.x = 0.0;
    }
    if (orig.y - delta.y).abs() > 1e-8 {
        p.vel.y = 0.0;
    }
    if (orig.z - delta.z).abs() > 1e-8 {
        p.vel.z = 0.0;
    }
}

pub(super) fn tick(
    p: &mut Particle,
    chunks: &ChunkStore,
    atlas: &AtlasUVMap,
    children: &mut Vec<Particle>,
    sounds: &mut Vec<super::ParticleSoundRequest>,
) -> bool {
    let Kind::Water(state) = p.kind else {
        return false;
    };
    let k = state.kind;
    p.prev_pos = p.pos;
    if p.age >= p.lifetime {
        let falling = match k {
            K::DrippingWater => Some(K::FallingWater),
            K::DrippingLava => Some(K::FallingLava),
            K::DrippingHoney => Some(K::FallingHoney),
            K::DrippingObsidianTear => Some(K::FallingObsidianTear),
            K::DrippingDripstoneWater => Some(K::FallingDripstoneWater),
            K::DrippingDripstoneLava => Some(K::FallingDripstoneLava),
            _ => None,
        };
        if let Some(next) = falling {
            p.kind = Kind::Water(State { kind: next });
            let name = format!("minecraft:{}", kind_name(next));
            let count = atlas.particle_sprite_names(&name).map_or(1, <[_]>::len);
            let frame = descriptor_random_frame(count);
            p.delay = frame as i32;
            p.set_sprite(
                &descriptor_frame(atlas, &name, frame).unwrap_or_else(|| atlas.missing_region()),
            );
            p.color = color(next);
            p.age = 0;
            p.lifetime = lifetime(next);
            p.gravity = gravity(next);
            if next == K::FallingHoney {
                p.gravity = 0.01;
            }
            if next == K::FallingObsidianTear {
                p.gravity = 0.01;
            }
            if next == K::FallingNectar {
                p.gravity = 0.007;
            }
            p.vel = DVec3::ZERO;
            p.size = 0.1;
            return true;
        }
        return false;
    }
    p.age += 1;
    if k == K::Nautilus {
        let launch = p.target.unwrap_or(p.pos);
        let t = (p.age as f32 / p.lifetime as f32).clamp(0.0, 1.0);
        let (dx, dy, dz) = (p.vel.x, p.vel.y, p.vel.z);
        p.pos = launch
            + dvec3(
                dx * f64::from(1.0 - t),
                dy * f64::from(1.0 - t) - f64::from(t.powi(4) * 1.2),
                dz * f64::from(1.0 - t),
            );
        p.light = if k == K::Nautilus { 0.0 } else { p.light };
        return true;
    }
    if k == K::SulfurBubbles {
        p.vel.y += 0.0016;
        p.vel.x += rand_signed() * 0.0015;
        p.vel.z += rand_signed() * 0.0015;
        move_particle(p, chunks, true);
        p.vel *= 0.85;
        let (_, _, source) = fluid_at(chunks, p.pos);
        if !source || p.pos.y >= p.target.map_or(p.pos.y, |end| end.y) || p.pos.y <= p.prev_pos.y {
            return false;
        }
        let start = f64::from(p.pitch);
        let end = p.target.map_or(start + 3.0, |v| v.y);
        let t = ((p.pos.y - start) / (end - start)).clamp(0.0, 1.0) as f32;
        p.size = p.base_size + (0.15 - p.base_size) * t;
        return true;
    }
    match k {
        K::Bubble | K::BubbleColumnUp => {
            p.vel.y += if k == K::Bubble { 0.002 } else { 0.005 };
            move_particle(p, chunks, true);
            p.vel *= p.friction;
            if k == K::BubbleColumnUp && p.on_ground {
                p.vel.x *= 0.7;
                p.vel.z *= 0.7;
            }
            if fluid_at(chunks, p.pos).0 != FluidKind::Water {
                return false;
            }
        }
        K::CurrentDown => {
            p.vel.x = (p.vel.x + 0.6 * f64::from(p.rot.cos())) * 0.07;
            p.vel.z = (p.vel.z + 0.6 * f64::from(p.rot.sin())) * 0.07;
            move_particle(p, chunks, false);
            p.rot += 0.08;
            if fluid_at(chunks, p.pos).0 != FluidKind::Water || p.on_ground {
                return false;
            }
        }
        K::Fishing | K::Dolphin | K::SquidInk | K::GlowSquidInk | K::Nautilus => {
            if k == K::SquidInk || k == K::GlowSquidInk {
                move_particle(p, chunks, false);
                p.vel *= p.friction;
                if p.age > p.lifetime / 2 {
                    p.alpha = 1.0 - (p.age - p.lifetime / 2) as f32 / p.lifetime as f32;
                }
                if block::is_air(chunks.get_block_state(
                    p.pos.x.floor() as i32,
                    p.pos.y.floor() as i32,
                    p.pos.z.floor() as i32,
                )) {
                    p.vel.y -= 0.0074;
                }
            } else {
                move_particle(p, chunks, k == K::Fishing);
                p.vel *= if k == K::Dolphin { 0.99 } else { 0.98 };
            }
        }
        K::Rain | K::Splash => {
            p.vel.y -= 0.04 * p.gravity;
            move_particle(p, chunks, true);
            p.vel *= 0.98;
            if p.on_ground {
                if fastrand::f32() < 0.5 {
                    return false;
                }
                p.vel.x *= 0.7;
                p.vel.z *= 0.7;
            }
            if reached_surface(chunks, p.pos) {
                return false;
            }
        }
        K::BubblePop => {
            p.vel.y -= 0.04 * p.gravity;
            move_particle(p, chunks, true);
        }
        K::Lava => {
            p.vel.y -= p.gravity * 0.04;
            move_particle(p, chunks, true);
            p.vel *= p.friction;
            if p.on_ground {
                p.vel.x *= 0.7;
                p.vel.z *= 0.7;
            }
            push_lava_smoke(
                children,
                p.pos,
                p.vel,
                atlas,
                p.age,
                p.lifetime,
                fastrand::f32(),
            );
            p.size = p.base_size * (1.0 - (p.age as f32 / p.lifetime as f32).powi(2));
        }
        _ if is_drip(k) => {
            let hanging = matches!(
                k,
                K::DrippingWater
                    | K::DrippingLava
                    | K::DrippingHoney
                    | K::DrippingObsidianTear
                    | K::DrippingDripstoneWater
                    | K::DrippingDripstoneLava
            );
            let falling = matches!(
                k,
                K::FallingLava
                    | K::FallingWater
                    | K::FallingHoney
                    | K::FallingObsidianTear
                    | K::FallingNectar
                    | K::FallingDripstoneWater
                    | K::FallingDripstoneLava
            );
            let land_kind = matches!(
                k,
                K::FallingLava
                    | K::FallingHoney
                    | K::FallingObsidianTear
                    | K::FallingDripstoneWater
                    | K::FallingDripstoneLava
            );
            if hanging {
                if k == K::DrippingLava || k == K::DrippingDripstoneLava {
                    p.color = [1.0, 16.0 / (p.age + 15) as f32, 4.0 / (p.age + 7) as f32];
                }
                p.vel.y -= 0.04 * p.gravity;
                move_particle(p, chunks, true);
                p.vel *= 0.02 * 0.98;
                let (fluid, surface, _) = fluid_at(chunks, p.pos);
                if drip_fluid_type(k).is_some_and(|wanted| fluid == wanted && p.pos.y < surface) {
                    return false;
                }
            } else {
                p.vel.y -= 0.04 * p.gravity;
                move_particle(p, chunks, true);
                p.vel *= 0.98;
                if let Some(wanted) = drip_fluid_type(k) {
                    let (fluid, top, _) = fluid_at(chunks, p.pos);
                    if fluid == wanted && p.pos.y < top {
                        return false;
                    }
                }
                if falling && p.on_ground {
                    if let Some(sound) = landing_sound(k, p.pos) {
                        sounds.push(sound);
                    }
                    if land_kind {
                        let child = match k {
                            K::FallingLava | K::FallingDripstoneLava => Some(K::LandingLava),
                            K::FallingHoney => Some(K::LandingHoney),
                            K::FallingObsidianTear => Some(K::LandingObsidianTear),
                            K::FallingDripstoneWater => Some(K::Splash),
                            _ => None,
                        };
                        if let Some(child) = child {
                            children.push(make_child(child, p.pos, atlas));
                        }
                    }
                    return false;
                }
            }
        }
        K::Spit => {
            p.vel.y -= p.gravity * 0.04;
            move_particle(p, chunks, true);
            p.vel *= p.friction;
            if p.on_ground {
                p.vel.x *= 0.7;
                p.vel.z *= 0.7;
            }
        }
        _ => {}
    }
    if matches!(k, K::Fishing) {
        let frame = ((60 - p.lifetime + p.age - 1).rem_euclid(4)) as usize;
        p.set_sprite(
            &descriptor_frame(atlas, "minecraft:fishing", frame)
                .unwrap_or_else(|| atlas.missing_region()),
        );
    } else if matches!(k, K::BubblePop | K::SquidInk | K::GlowSquidInk | K::Spit) {
        let name = format!("minecraft:{}", kind_name(k));
        let count = atlas.particle_sprite_names(&name).map_or(1, <[_]>::len);
        p.set_sprite(
            &descriptor_frame(atlas, &name, descriptor_age_frame(p.age, p.lifetime, count))
                .unwrap_or_else(|| atlas.missing_region()),
        );
    }
    if !matches!(
        k,
        K::Fishing | K::BubblePop | K::SquidInk | K::GlowSquidInk | K::Spit
    ) {
        let name = format!("minecraft:{}", kind_name(k));
        let count = atlas
            .particle_sprite_names(&name)
            .map_or(1, <[_]>::len)
            .max(1);
        let frame = p.delay.max(0) as usize % count;
        p.set_sprite(
            &descriptor_frame(atlas, &name, frame).unwrap_or_else(|| atlas.missing_region()),
        );
    }
    if matches!(
        k,
        K::Lava
            | K::SquidInk
            | K::GlowSquidInk
            | K::DrippingObsidianTear
            | K::FallingObsidianTear
            | K::LandingObsidianTear
    ) {
        p.light = 1.0;
    } else {
        p.light = world_brightness(
            chunks,
            p.pos.x.floor() as i32,
            p.pos.y.floor() as i32,
            p.pos.z.floor() as i32,
        );
    }
    true
}
fn push_lava_smoke(
    children: &mut Vec<Particle>,
    pos: DVec3,
    velocity: DVec3,
    atlas: &AtlasUVMap,
    age: i32,
    lifetime: i32,
    sample: f32,
) -> bool {
    if sample <= age as f32 / lifetime.max(1) as f32 {
        return false;
    }
    let frames: [crate::renderer::chunk::atlas::AtlasRegion; 8] = std::array::from_fn(|i| {
        descriptor_frame(atlas, "minecraft:smoke", i).unwrap_or_else(|| atlas.missing_region())
    });
    children.push(Particle::smoke(pos, velocity, &frames));
    true
}

fn landing_sound(kind: K, pos: DVec3) -> Option<super::ParticleSoundRequest> {
    let (event, volume) = match kind {
        K::FallingHoney => ("block.beehive.drip", 0.3 + fastrand::f32() * 0.7),
        K::FallingDripstoneWater => (
            "block.pointed_dripstone.drip_water",
            0.3 + fastrand::f32() * 0.7,
        ),
        K::FallingDripstoneLava => (
            "block.pointed_dripstone.drip_lava",
            0.3 + fastrand::f32() * 0.7,
        ),
        _ => return None,
    };
    Some(super::ParticleSoundRequest {
        event,
        pos,
        volume,
        pitch: 1.0,
        seed: fastrand::u64(..),
    })
}

fn make_child(kind: K, pos: DVec3, atlas: &AtlasUVMap) -> Particle {
    let name = format!("minecraft:{}", kind_name(kind));
    let frame = descriptor_random_frame(atlas.particle_sprite_names(&name).map_or(1, <[_]>::len));
    let sprite = descriptor_frame(atlas, &name, frame).unwrap_or_else(|| atlas.missing_region());
    let mut p = Particle::special(
        Kind::Water(State { kind }),
        pos,
        DVec3::ZERO,
        None,
        lifetime(kind),
        0.1,
        sprite,
    );
    p.delay = frame as i32;
    p.color = color(kind);
    p.gravity = gravity(kind);
    match kind {
        K::Splash => {
            p.gravity = 0.04;
            p.vel.y = f64::from(fastrand::f32() * 0.2 + 0.1);
            p.size = 0.1;
            p.base_size = p.size;
        }
        K::LandingLava | K::LandingHoney | K::LandingObsidianTear => p.gravity = 0.06,
        _ => {}
    }
    p
}

pub(super) fn appearance(p: &Particle, partial_tick: f32) -> Option<Appearance> {
    let Kind::Water(state) = p.kind else {
        return None;
    };
    let k = state.kind;
    let (size, color, alpha) = match k {
        K::Nautilus => (p.size, p.color, p.alpha),
        K::Lava => {
            let t = (p.age as f32 + partial_tick) / p.lifetime as f32;
            (p.base_size * (1.0 - t * t), p.color, p.alpha)
        }
        _ => (p.size, p.color, p.alpha),
    };
    Some(Appearance {
        size,
        color,
        alpha,
        rotation: p.rotation,
        second_rotation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_coverage_is_exactly_the_java_water_family_30() {
        let kinds = [
            K::Bubble,
            K::SulfurBubbles,
            K::Fishing,
            K::Rain,
            K::Splash,
            K::BubblePop,
            K::CurrentDown,
            K::BubbleColumnUp,
            K::Nautilus,
            K::Dolphin,
            K::SquidInk,
            K::GlowSquidInk,
            K::Lava,
            K::DrippingLava,
            K::FallingLava,
            K::LandingLava,
            K::DrippingWater,
            K::FallingWater,
            K::DrippingHoney,
            K::FallingHoney,
            K::LandingHoney,
            K::FallingNectar,
            K::DrippingObsidianTear,
            K::FallingObsidianTear,
            K::LandingObsidianTear,
            K::DrippingDripstoneLava,
            K::FallingDripstoneLava,
            K::DrippingDripstoneWater,
            K::FallingDripstoneWater,
            K::Spit,
        ];
        assert_eq!(kinds.len(), 30);
        assert!(kinds.into_iter().all(supports));
        for kind in kinds {
            assert_eq!(
                Kind::Water(State { kind }).translucent(),
                matches!(kind, K::SquidInk | K::GlowSquidInk),
                "{} layer",
                kind_name(kind)
            );
        }
    }
    #[test]
    fn drip_stages_and_native_lifetimes_are_distinct() {
        assert_eq!(lifetime(K::DrippingWater), 40);
        assert_eq!(lifetime(K::DrippingHoney), 100);
        assert!((64..=320).contains(&lifetime(K::FallingLava)));
        assert!((28..=140).contains(&lifetime(K::LandingObsidianTear)));
        assert!((4..=40).contains(&lifetime(K::FallingWater)));
        assert!(is_drip(K::DrippingDripstoneWater));
        assert!(!is_drip(K::Bubble));
    }

    #[test]
    fn java_drip_providers_use_water_lava_or_empty_fluid_types() {
        assert_eq!(drip_fluid_type(K::DrippingWater), Some(FluidKind::Water));
        assert_eq!(drip_fluid_type(K::FallingWater), Some(FluidKind::Water));
        assert_eq!(
            drip_fluid_type(K::DrippingDripstoneWater),
            Some(FluidKind::Water)
        );
        assert_eq!(
            drip_fluid_type(K::FallingDripstoneWater),
            Some(FluidKind::Water)
        );
        for kind in [
            K::DrippingLava,
            K::FallingLava,
            K::LandingLava,
            K::DrippingDripstoneLava,
            K::FallingDripstoneLava,
        ] {
            assert_eq!(drip_fluid_type(kind), Some(FluidKind::Lava), "{kind:?}");
        }
        for kind in [
            K::DrippingHoney,
            K::FallingHoney,
            K::LandingHoney,
            K::FallingNectar,
            K::DrippingObsidianTear,
            K::FallingObsidianTear,
            K::LandingObsidianTear,
        ] {
            assert_eq!(drip_fluid_type(kind), None, "{kind:?}");
        }
    }

    #[test]
    fn drip_removal_uses_real_world_fluid_type_and_skips_empty_type() {
        let _protocol = crate::world::block::test_protocol_guard();
        block::init("26.2");
        let atlas = AtlasUVMap::test_empty();
        let mut water_world = ChunkStore::new(1);
        let mut chunk = azalea_world::chunk::Chunk::default();
        chunk.sections = vec![Default::default(); water_world.section_count() as usize].into();
        water_world.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), chunk);
        water_world.set_block_state(1, 5, 1, block::find_state("water", &[("level", "0")]));
        let mut lava_world = ChunkStore::new(1);
        let mut chunk = azalea_world::chunk::Chunk::default();
        chunk.sections = vec![Default::default(); lava_world.section_count() as usize].into();
        lava_world.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), chunk);
        lava_world.set_block_state(1, 5, 1, block::find_state("lava", &[("level", "0")]));

        for (kind, native_fluid) in [
            (K::DrippingWater, FluidKind::Water),
            (K::FallingWater, FluidKind::Water),
            (K::DrippingLava, FluidKind::Lava),
            (K::FallingLava, FluidKind::Lava),
        ] {
            for (world, fluid) in [
                (&water_world, FluidKind::Water),
                (&lava_world, FluidKind::Lava),
            ] {
                let mut particle = Particle::special(
                    Kind::Water(State { kind }),
                    dvec3(1.5, 5.5, 1.5),
                    DVec3::ZERO,
                    None,
                    100,
                    0.1,
                    atlas.missing_region(),
                );
                particle.gravity = 0.0;
                let mut children = Vec::new();
                let mut sounds = Vec::new();
                let alive = tick(&mut particle, world, &atlas, &mut children, &mut sounds);
                assert_eq!(alive, native_fluid != fluid, "{kind:?} in {fluid:?}");
            }
        }

        for kind in [
            K::DrippingHoney,
            K::FallingHoney,
            K::DrippingObsidianTear,
            K::FallingObsidianTear,
        ] {
            for world in [&water_world, &lava_world] {
                let mut particle = Particle::special(
                    Kind::Water(State { kind }),
                    dvec3(1.5, 5.5, 1.5),
                    DVec3::ZERO,
                    None,
                    100,
                    0.1,
                    atlas.missing_region(),
                );
                particle.gravity = 0.0;
                let mut children = Vec::new();
                let mut sounds = Vec::new();
                assert!(
                    tick(&mut particle, world, &atlas, &mut children, &mut sounds),
                    "{kind:?}"
                );
            }
        }
    }

    #[test]
    fn all_30_providers_spawn_and_reject_wrong_option_payloads() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let chunks = ChunkStore::new(1);
        let registry = BlockRegistry::test_empty();
        let climate = HashMap::new();
        let kinds = [
            K::Bubble,
            K::SulfurBubbles,
            K::Fishing,
            K::Rain,
            K::Splash,
            K::BubblePop,
            K::CurrentDown,
            K::BubbleColumnUp,
            K::Nautilus,
            K::Dolphin,
            K::SquidInk,
            K::GlowSquidInk,
            K::Lava,
            K::DrippingLava,
            K::FallingLava,
            K::LandingLava,
            K::DrippingWater,
            K::FallingWater,
            K::DrippingHoney,
            K::FallingHoney,
            K::LandingHoney,
            K::FallingNectar,
            K::DrippingObsidianTear,
            K::FallingObsidianTear,
            K::LandingObsidianTear,
            K::DrippingDripstoneLava,
            K::FallingDripstoneLava,
            K::DrippingDripstoneWater,
            K::FallingDripstoneWater,
            K::Spit,
        ];
        for kind in kinds {
            assert!(!spawn(
                &mut store,
                kind,
                ServerParticleOptions::Dust {
                    packed_color: 0,
                    scale: 1.0
                },
                DVec3::ZERO,
                DVec3::ZERO,
                &registry,
                &chunks,
                &climate
            ));
            assert!(
                spawn(
                    &mut store,
                    kind,
                    ServerParticleOptions::Simple,
                    DVec3::ZERO,
                    DVec3::ZERO,
                    &registry,
                    &chunks,
                    &climate
                ),
                "{}",
                kind_name(kind)
            );
        }
        assert_eq!(store.pending.len(), 30);
        assert!(
            store
                .pending
                .iter()
                .all(|p| matches!(p.kind, Kind::Water(_)))
        );
    }

    #[test]
    fn lava_smoke_children_follow_the_java_age_probability() {
        let atlas = AtlasUVMap::test_empty();
        let mut children = Vec::new();
        assert!(push_lava_smoke(
            &mut children,
            DVec3::ZERO,
            DVec3::ZERO,
            &atlas,
            0,
            100,
            0.5
        ));
        assert!(!push_lava_smoke(
            &mut children,
            DVec3::ZERO,
            DVec3::ZERO,
            &atlas,
            5,
            10,
            0.5
        ));
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0].kind, Kind::Smoke));
    }

    #[test]
    fn animated_frames_and_fullbright_ink_follow_provider_rules() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        assert_eq!(descriptor_age_frame(0, 10, 4), 0);
        assert_eq!(descriptor_age_frame(9, 10, 4), 3);
        let atlas = AtlasUVMap::test_empty();
        let chunks = ChunkStore::new(1);
        let mut children = Vec::new();
        let mut sounds = Vec::new();
        let mut ink = Particle::special(
            Kind::Water(State {
                kind: K::GlowSquidInk,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            10,
            0.5,
            atlas.missing_region(),
        );
        ink.age = 5;
        assert!(tick(&mut ink, &chunks, &atlas, &mut children, &mut sounds));
        assert!(ink.alpha < 1.0);
    }

    #[test]
    fn landing_sounds_match_java_events_and_fire_once_only_after_collision() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        let mut chunk = azalea_world::chunk::Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), chunk);
        let stone = block::first_state_of("stone").unwrap();
        for x in 0..4 {
            chunks.set_block_state(x, 5, 0, stone);
        }
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let atlas = AtlasUVMap::test_empty();
        let mut store = ParticleStore::new(atlas.clone(), colors.clone(), colors.clone(), colors);
        for (x, kind, gravity) in [
            (0.5, K::FallingHoney, 0.01),
            (1.5, K::FallingDripstoneWater, 0.06),
            (2.5, K::FallingDripstoneLava, 0.06),
        ] {
            let mut particle = Particle::special(
                Kind::Water(State { kind }),
                dvec3(x, 6.2, 0.5),
                dvec3(0.0, -0.5, 0.0),
                None,
                100,
                0.1,
                atlas.missing_region(),
            );
            particle.gravity = gravity;
            store.particles.push(particle);
        }
        let mut falling_in_air = Particle::special(
            Kind::Water(State {
                kind: K::FallingHoney,
            }),
            dvec3(3.5, 20.0, 0.5),
            dvec3(0.0, -0.01, 0.0),
            None,
            100,
            0.1,
            atlas.missing_region(),
        );
        falling_in_air.gravity = 0.01;
        store.particles.push(falling_in_air);

        store.tick_with_entity_lookup(&chunks, |_| None);
        let sounds = store.drain_sound_requests();
        assert_eq!(sounds.len(), 3);
        for ((sound, expected), x) in sounds
            .iter()
            .zip([
                "block.beehive.drip",
                "block.pointed_dripstone.drip_water",
                "block.pointed_dripstone.drip_lava",
            ])
            .zip([0.5, 1.5, 2.5])
        {
            assert_eq!(sound.event, expected);
            assert!((sound.pos.x - x).abs() < f64::EPSILON);
            assert!((6.0..6.2).contains(&sound.pos.y));
            assert!((0.3..=1.0).contains(&sound.volume));
            assert_eq!(sound.pitch, 1.0);
        }
        store.tick_with_entity_lookup(&chunks, |_| None);
        assert!(store.drain_sound_requests().is_empty());
    }

    #[test]
    fn particle_mode_and_distance_culling_cannot_enqueue_landing_sounds() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let registry = BlockRegistry::test_empty();
        let climate = HashMap::new();
        store.set_mode(super::super::ParticleMode::Minimal);
        store.add_particles_from_packet(
            K::FallingHoney,
            ServerParticleOptions::Simple,
            false,
            false,
            dvec3(0.5, 6.2, 0.5),
            dvec3(0.0, -1.0, 0.0),
            1.0,
            0,
            DVec3::ZERO,
            &registry,
            &chunks,
            &climate,
        );
        store.set_mode(super::super::ParticleMode::All);
        store.add_particles_from_packet(
            K::FallingHoney,
            ServerParticleOptions::Simple,
            false,
            false,
            dvec3(40.0, 6.2, 0.5),
            dvec3(0.0, -1.0, 0.0),
            1.0,
            0,
            DVec3::ZERO,
            &registry,
            &chunks,
            &climate,
        );
        store.tick_with_entity_lookup(&chunks, |_| None);
        store.tick_with_entity_lookup(&chunks, |_| None);
        assert!(store.drain_sound_requests().is_empty());
        assert!(store.particles.is_empty());
    }

    #[test]
    fn water_queries_use_fluid_and_collision_surfaces() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        let mut chunk = azalea_world::chunk::Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), chunk);
        let water = block::first_state_of("water").unwrap();
        let stone = block::first_state_of("stone").unwrap();
        chunks.set_block_state(1, 5, 1, water);
        chunks.set_block_state(2, 5, 1, stone);
        assert_eq!(fluid_at(&chunks, dvec3(1.5, 5.5, 1.5)).0, FluidKind::Water);
        assert!(reached_surface(&chunks, dvec3(2.5, 5.5, 1.5)));
        assert!(!reached_surface(&chunks, dvec3(2.5, 4.5, 1.5)));
        let mut falling = Particle::special(
            Kind::Water(State {
                kind: K::FallingWater,
            }),
            dvec3(2.5, 6.2, 1.5),
            dvec3(0.0, -0.5, 0.0),
            None,
            20,
            0.1,
            AtlasUVMap::test_empty().missing_region(),
        );
        move_particle(&mut falling, &chunks, true);
        assert!(falling.on_ground);
        assert!(falling.pos.y >= 6.0);
    }

    #[test]
    fn bubbles_exit_when_not_in_water_and_hanging_drips_transition() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let atlas = AtlasUVMap::test_empty();
        let mut children = Vec::new();
        let mut sounds = Vec::new();
        let sprite = atlas.missing_region();
        let mut bubble = Particle::special(
            Kind::Water(State { kind: K::Bubble }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            20,
            0.02,
            sprite,
        );
        assert!(!tick(
            &mut bubble,
            &chunks,
            &atlas,
            &mut children,
            &mut sounds
        ));

        let mut drip = Particle::special(
            Kind::Water(State {
                kind: K::DrippingWater,
            }),
            dvec3(0.5, 8.0, 0.5),
            DVec3::ZERO,
            None,
            40,
            0.01,
            sprite,
        );
        drip.age = 40;
        assert!(tick(&mut drip, &chunks, &atlas, &mut children, &mut sounds));
        assert!(matches!(
            drip.kind,
            Kind::Water(State {
                kind: K::FallingWater
            })
        ));
        assert_eq!(drip.age, 0);
        assert!((4..=40).contains(&drip.lifetime));
    }
}
