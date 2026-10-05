//! Java 26.2 atmosphere and ambient particle providers.
use std::collections::HashMap;

use glam::{DVec3, Quat};

use super::{
    Appearance, Kind, Particle, ParticleStore, ServerParticleKind, ServerParticleOptions,
    descriptor_age_frame, descriptor_frame,
};
use crate::renderer::chunk::atlas::AtlasUVMap;
use crate::renderer::chunk::mesher::BiomeClimate;
use crate::world::block;
use crate::world::block::registry::BlockRegistry;
use crate::world::chunk::ChunkStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct State {
    pub(super) kind: ServerParticleKind,
    // Provider-specific deterministic parameters (leaves, fading gas and breath).
    mode: u8,
    seed: u16,
    frame: u16,
    translucent: bool,
    hit_ground: bool,
}

impl State {
    fn new(kind: ServerParticleKind, mode: u8, seed: u16, frame: u16, translucent: bool) -> Self {
        Self {
            kind,
            mode,
            seed,
            frame,
            translucent,
            hit_ground: false,
        }
    }
}

pub(super) fn translucent(state: State) -> bool {
    state.translucent
}

fn provider_translucency(
    kind: ServerParticleKind,
    sprite: crate::renderer::chunk::atlas::AtlasRegion,
) -> bool {
    matches!(
        kind,
        ServerParticleKind::Cloud
            | ServerParticleKind::Sneeze
            | ServerParticleKind::Firefly
            | ServerParticleKind::NoxiousGas
            | ServerParticleKind::Infested
            | ServerParticleKind::Soul
            | ServerParticleKind::SculkSoul
    ) || (kind == ServerParticleKind::SulfurCubeGoo && sprite.translucent)
}

pub(super) fn supports(kind: ServerParticleKind) -> bool {
    matches!(
        kind,
        ServerParticleKind::Cloud
            | ServerParticleKind::CopperFireFlame
            | ServerParticleKind::Flame
            | ServerParticleKind::SoulFireFlame
            | ServerParticleKind::SmallFlame
            | ServerParticleKind::LargeSmoke
            | ServerParticleKind::WhiteSmoke
            | ServerParticleKind::Sneeze
            | ServerParticleKind::Ash
            | ServerParticleKind::WhiteAsh
            | ServerParticleKind::CrimsonSpore
            | ServerParticleKind::WarpedSpore
            | ServerParticleKind::SporeBlossomAir
            | ServerParticleKind::FallingSporeBlossom
            | ServerParticleKind::Mycelium
            | ServerParticleKind::Underwater
            | ServerParticleKind::Firefly
            | ServerParticleKind::CherryLeaves
            | ServerParticleKind::PaleOakLeaves
            | ServerParticleKind::TintedLeaves
            | ServerParticleKind::AngryVillager
            | ServerParticleKind::HappyVillager
            | ServerParticleKind::Composter
            | ServerParticleKind::Heart
            | ServerParticleKind::Snowflake
            | ServerParticleKind::NoxiousGas
            | ServerParticleKind::SulfurCubeGoo
            | ServerParticleKind::DragonBreath
            | ServerParticleKind::Infested
            | ServerParticleKind::Soul
            | ServerParticleKind::SculkSoul
    )
}

fn rgb24(value: i32) -> [f32; 3] {
    let value = value as u32;
    [
        ((value >> 16) & 255) as f32 / 255.0,
        ((value >> 8) & 255) as f32 / 255.0,
        (value & 255) as f32 / 255.0,
    ]
}
fn rand() -> f32 {
    fastrand::f32()
}
fn unit() -> f64 {
    f64::from(rand())
}
fn move_without_collision(p: &mut Particle) {
    p.pos += p.vel;
}
fn base_velocity(input: DVec3) -> DVec3 {
    let mut v = input + DVec3::new(unit() * 0.8 - 0.4, unit() * 0.8 - 0.4, unit() * 0.8 - 0.4);
    v = v.normalize_or_zero() * (unit() + unit() + 1.0) * 0.06;
    v.y += 0.1;
    v
}
fn crimson_spore_velocity() -> DVec3 {
    DVec3::new(
        super::next_gaussian() * f64::from(1.0e-6_f32),
        super::next_gaussian() * f64::from(1.0e-4_f32),
        super::next_gaussian() * f64::from(1.0e-6_f32),
    )
}
fn base_quad() -> f32 {
    0.1 * (rand() * 0.5 + 0.5) * 2.0
}
fn random_lifetime(base: f32, spread: f32) -> i32 {
    (f64::from(base) / (unit() * f64::from(spread) + f64::from(1.0 - spread))) as i32
}
fn sprites_for(kind: ServerParticleKind) -> &'static str {
    match kind {
        ServerParticleKind::Cloud => "minecraft:cloud",
        ServerParticleKind::Sneeze => "minecraft:sneeze",
        ServerParticleKind::CopperFireFlame => "minecraft:copper_fire_flame",
        ServerParticleKind::Flame => "minecraft:flame",
        ServerParticleKind::SoulFireFlame => "minecraft:soul_fire_flame",
        ServerParticleKind::SmallFlame => "minecraft:small_flame",
        ServerParticleKind::LargeSmoke => "minecraft:large_smoke",
        ServerParticleKind::WhiteSmoke => "minecraft:white_smoke",
        ServerParticleKind::Ash => "minecraft:ash",
        ServerParticleKind::WhiteAsh => "minecraft:white_ash",
        ServerParticleKind::CrimsonSpore => "minecraft:crimson_spore",
        ServerParticleKind::WarpedSpore => "minecraft:warped_spore",
        ServerParticleKind::SporeBlossomAir => "minecraft:spore_blossom_air",
        ServerParticleKind::FallingSporeBlossom => "minecraft:falling_spore_blossom",
        ServerParticleKind::Mycelium => "minecraft:mycelium",
        ServerParticleKind::Underwater => "minecraft:underwater",
        ServerParticleKind::Firefly => "minecraft:firefly",
        ServerParticleKind::CherryLeaves => "minecraft:cherry_leaves",
        ServerParticleKind::PaleOakLeaves => "minecraft:pale_oak_leaves",
        ServerParticleKind::TintedLeaves => "minecraft:tinted_leaves",
        ServerParticleKind::AngryVillager => "minecraft:angry_villager",
        ServerParticleKind::HappyVillager => "minecraft:happy_villager",
        ServerParticleKind::Composter => "minecraft:composter",
        ServerParticleKind::Heart => "minecraft:heart",
        ServerParticleKind::Snowflake => "minecraft:snowflake",
        ServerParticleKind::NoxiousGas => "minecraft:noxious_gas",
        ServerParticleKind::SulfurCubeGoo => "minecraft:sulfur_cube_goo",
        ServerParticleKind::DragonBreath => "minecraft:dragon_breath",
        ServerParticleKind::Infested => "minecraft:infested",
        ServerParticleKind::Soul => "minecraft:soul",
        ServerParticleKind::SculkSoul => "minecraft:sculk_soul",
        _ => unreachable!("not an atmosphere provider"),
    }
}

/// Constructors follow ParticleResources registrations and each provider's Java
/// constructor.
pub(super) fn spawn(
    store: &mut ParticleStore,
    kind: ServerParticleKind,
    options: ServerParticleOptions,
    pos: DVec3,
    vel: DVec3,
    _registry: &BlockRegistry,
    _chunks: &ChunkStore,
    _biome_climate: &HashMap<u32, BiomeClimate>,
) -> bool {
    if !supports(kind) {
        return false;
    }
    let fallback = store.uv_map.missing_region();
    let frames = store.uv_map.particle_sprite_names(sprites_for(kind));
    let static_sprite = matches!(
        kind,
        ServerParticleKind::CrimsonSpore
            | ServerParticleKind::WarpedSpore
            | ServerParticleKind::SporeBlossomAir
            | ServerParticleKind::FallingSporeBlossom
            | ServerParticleKind::Mycelium
            | ServerParticleKind::Underwater
            | ServerParticleKind::Firefly
            | ServerParticleKind::CherryLeaves
            | ServerParticleKind::PaleOakLeaves
            | ServerParticleKind::TintedLeaves
            | ServerParticleKind::AngryVillager
            | ServerParticleKind::HappyVillager
            | ServerParticleKind::Composter
            | ServerParticleKind::Heart
    );
    let frame = frames.as_ref().filter(|f| !f.is_empty()).map_or(0, |f| {
        if static_sprite {
            fastrand::usize(..f.len())
        } else {
            0
        }
    });
    let sprite = descriptor_frame(&store.uv_map, sprites_for(kind), frame).unwrap_or(fallback);
    let translucent = provider_translucency(kind, sprite);
    let mode = match kind {
        ServerParticleKind::CherryLeaves => 1,
        ServerParticleKind::PaleOakLeaves | ServerParticleKind::TintedLeaves => 2,
        _ => 0,
    };
    let seed = fastrand::u16(..);
    let mut state = State::new(kind, mode, seed, frame as u16, translucent);
    let (life, size, color, alpha, gravity, friction, velocity, _light) = match kind {
        // PlayerCloudParticle: zero-velocity base ctor, scale 2.5, 8/(r*.8+.3)*2.5.
        ServerParticleKind::Cloud | ServerParticleKind::Sneeze => {
            let life = (8.0 / (unit() * 0.8 + 0.3) * 2.5) as i32;
            let cloud_vel = base_velocity(DVec3::ZERO) * 0.1 + vel;
            let shade = 1.0 - rand() * 0.3;
            (
                life,
                base_quad() * 1.875,
                if kind == ServerParticleKind::Sneeze {
                    [0.22, 1.0, 0.53]
                } else {
                    [shade; 3]
                },
                if kind == ServerParticleKind::Sneeze {
                    0.4
                } else {
                    1.0
                },
                0.0,
                0.96,
                cloud_vel,
                0.0,
            )
        }
        ServerParticleKind::CopperFireFlame
        | ServerParticleKind::Flame
        | ServerParticleKind::SoulFireFlame
        | ServerParticleKind::SmallFlame => {
            let base = random_lifetime(8.0, 0.8) + 4;
            (
                base,
                base_quad()
                    * if kind == ServerParticleKind::SmallFlame {
                        0.5
                    } else {
                        1.0
                    },
                [1.0; 3],
                1.0,
                0.0,
                0.96,
                base_velocity(vel) * 0.01 + vel,
                1.0,
            )
        }
        ServerParticleKind::LargeSmoke => (
            random_lifetime(20.0, 0.8),
            base_quad() * 0.75 * 2.5,
            [rand() * 0.3; 3],
            1.0,
            -0.1,
            0.96,
            base_velocity(DVec3::ZERO) * 0.1 + vel,
            0.0,
        ),
        ServerParticleKind::WhiteSmoke => (
            random_lifetime(8.0, 0.8).max(1),
            base_quad() * 0.75,
            [0.7294118, 0.69411767, 0.7607843],
            1.0,
            -0.1,
            0.96,
            base_velocity(DVec3::ZERO) * 0.1 + vel,
            0.0,
        ),
        ServerParticleKind::Ash => {
            let v = base_velocity(DVec3::ZERO);
            (
                random_lifetime(20.0, 0.8).max(1),
                base_quad() * 0.75,
                [rand() * 0.5; 3],
                1.0,
                0.1,
                0.96,
                DVec3::new(v.x * 0.1 + vel.x, v.y * -0.1 + vel.y, v.z * 0.1 + vel.z),
                0.0,
            )
        }
        ServerParticleKind::WhiteAsh => {
            let v = base_velocity(DVec3::ZERO);
            let extra = DVec3::new(
                -unit() * 1.9 * unit() * 0.1,
                -unit() * 0.5 * unit() * 0.5,
                -unit() * 1.9 * unit() * 0.1,
            );
            (
                random_lifetime(20.0, 0.8).max(1),
                base_quad() * 0.75,
                rgb24(12235202),
                1.0,
                0.0125,
                0.96,
                DVec3::new(
                    v.x * 0.1 + extra.x,
                    v.y * -0.1 + extra.y,
                    v.z * 0.1 + extra.z,
                ),
                0.0,
            )
        }
        ServerParticleKind::CrimsonSpore
        | ServerParticleKind::WarpedSpore
        | ServerParticleKind::SporeBlossomAir
        | ServerParticleKind::Underwater => {
            let life = if kind == ServerParticleKind::SporeBlossomAir {
                500 + fastrand::i32(0..501)
            } else {
                (16.0 / (unit() * 0.8 + 0.2)) as i32
            };
            let (v, c, _size_scale, g) = match kind {
                ServerParticleKind::CrimsonSpore => {
                    (crimson_spore_velocity(), [0.9, 0.4, 0.5], 0.1, 0.0)
                }
                ServerParticleKind::WarpedSpore => (
                    DVec3::new(0.0, -unit() * 1.9 * unit() * 0.1, 0.0),
                    [0.1, 0.1, 0.3],
                    0.001,
                    0.0,
                ),
                ServerParticleKind::SporeBlossomAir => {
                    (DVec3::new(0.0, -0.8, 0.0), [0.32, 0.5, 0.22], 0.1, 0.01)
                }
                _ => (DVec3::ZERO, [0.4, 0.4, 0.7], 0.1, 0.0),
            };
            let size = base_quad()
                * if kind == ServerParticleKind::Underwater {
                    0.2 + rand() * 0.6
                } else {
                    0.6 + rand() * 0.6
                };
            (life, size, c, 1.0, g, 1.0, v, 0.0)
        }
        ServerParticleKind::FallingSporeBlossom => (
            (64.0 / (0.1 + unit() * 0.8)) as i32,
            base_quad(),
            [0.32, 0.5, 0.22],
            1.0,
            0.005,
            0.98,
            DVec3::ZERO,
            0.0,
        ),
        ServerParticleKind::Mycelium => {
            let shade = 0.2 + rand() * 0.1;
            (
                random_lifetime(20.0, 0.8),
                base_quad() * (0.5 + rand() * 0.6),
                [shade; 3],
                1.0,
                0.0,
                0.99,
                base_velocity(vel) * 0.02,
                0.0,
            )
        }
        ServerParticleKind::Firefly => (
            200 + fastrand::i32(0..101),
            base_quad() * 0.75 * 1.5,
            [1.0; 3],
            0.0,
            0.0,
            0.96,
            base_velocity(DVec3::new(
                0.5 - unit(),
                if rand() < 0.5 { vel.y } else { -vel.y },
                0.5 - unit(),
            )) * 0.8,
            1.0,
        ),
        ServerParticleKind::CherryLeaves
        | ServerParticleKind::PaleOakLeaves
        | ServerParticleKind::TintedLeaves => {
            let tint = if kind == ServerParticleKind::TintedLeaves {
                match options {
                    ServerParticleOptions::Color { color } => rgb24(color),
                    _ => [1.0; 3],
                }
            } else {
                [1.0; 3]
            };
            state.seed = fastrand::u16(..);
            let size =
                (if mode == 1 { 1.0 } else { 2.0 }) * if rand() < 0.5 { 0.05 } else { 0.075 };
            (
                300,
                size,
                tint,
                1.0,
                if mode == 1 { 0.25 } else { 0.07 } * 1.2 * 0.0025,
                1.0,
                DVec3::new(0.0, if mode == 1 { 0.0 } else { -0.021 }, 0.0),
                0.0,
            )
        }
        ServerParticleKind::AngryVillager
        | ServerParticleKind::HappyVillager
        | ServerParticleKind::Composter
        | ServerParticleKind::Heart => {
            let heart = matches!(
                kind,
                ServerParticleKind::AngryVillager | ServerParticleKind::Heart
            );
            let town = base_velocity(if heart { DVec3::ZERO } else { vel });
            let v = if heart {
                DVec3::new(town.x * 0.01, town.y * 0.01 + 0.1, town.z * 0.01)
            } else {
                town * 0.02
            };
            let lifetime = match kind {
                ServerParticleKind::Composter => 3 + fastrand::i32(0..5),
                ServerParticleKind::HappyVillager => random_lifetime(20.0, 0.8),
                _ => 16,
            };
            (
                lifetime,
                base_quad() * if heart { 1.5 } else { 0.5 + rand() * 0.6 },
                [1.0; 3],
                1.0,
                0.0,
                if heart { 0.86 } else { 0.99 },
                v,
                0.0,
            )
        }
        ServerParticleKind::Snowflake => (
            random_lifetime(16.0, 0.8) + 2,
            0.1 * (unit() as f32 * unit() as f32 + 1.0),
            [0.923, 0.964, 0.999],
            1.0,
            0.225,
            1.0,
            vel + DVec3::new(
                (unit() * 2.0 - 1.0) * 0.05,
                (unit() * 2.0 - 1.0) * 0.05,
                (unit() * 2.0 - 1.0) * 0.05,
            ),
            0.0,
        ),
        ServerParticleKind::NoxiousGas => (
            random_lifetime(18.0, 0.5),
            base_quad() * 0.75 * 3.0,
            [1.0; 3],
            1.0,
            -0.02,
            0.96,
            base_velocity(DVec3::ZERO) * 0.1 + vel,
            0.0,
        ),
        ServerParticleKind::SulfurCubeGoo => (
            random_lifetime(4.0, 0.9),
            base_quad() * 0.5,
            [1.0; 3],
            1.0,
            1.0,
            0.98,
            base_velocity(DVec3::ZERO) * 0.1,
            0.0,
        ),
        ServerParticleKind::DragonBreath => {
            let life = (20.0 / (unit() * 0.8 + 0.2)) as i32;
            let power = match options {
                ServerParticleOptions::Power { power } => power,
                _ => 1.0,
            };
            let c = [
                0.7176471 + rand() * (0.8745098 - 0.7176471),
                0.0,
                0.8235294 + rand() * (0.9764706 - 0.8235294),
            ];
            (
                life,
                base_quad() * 0.75,
                c,
                1.0,
                0.0,
                0.96,
                DVec3::new(
                    vel.x * f64::from(power),
                    (vel.y - 0.1) * f64::from(power) + 0.1,
                    vel.z * f64::from(power),
                ),
                0.0,
            )
        }
        ServerParticleKind::Infested => {
            let mut v = base_velocity(DVec3::new(0.5 - unit(), vel.y, 0.5 - unit()));
            v.y *= 0.2;
            if vel.x == 0.0 && vel.z == 0.0 {
                v.x *= 0.1;
                v.z *= 0.1;
            }
            (
                random_lifetime(8.0, 0.8),
                base_quad() * 0.75,
                [1.0; 3],
                1.0,
                -0.1,
                0.96,
                v,
                0.0,
            )
        }
        ServerParticleKind::Soul | ServerParticleKind::SculkSoul => (
            random_lifetime(8.0, 0.8) + 4,
            base_quad() * 1.5,
            [1.0; 3],
            1.0,
            0.0,
            0.96,
            base_velocity(vel) * 0.01 + vel,
            1.0,
        ),
        _ => return false,
    };
    let particle_kind = Kind::Atmosphere(state);
    let mut p = Particle::special(
        particle_kind,
        pos,
        velocity,
        None,
        life.max(1),
        size,
        sprite,
    );
    p.color = color;
    p.alpha = alpha;
    p.gravity = gravity;
    p.friction = friction;
    // Retain the legacy scalar for provider-local assertions; frame extraction
    // now resolves Java packed light coordinates from the chunk at the particle.
    p.light = 1.0;
    if matches!(
        kind,
        ServerParticleKind::CopperFireFlame
            | ServerParticleKind::Flame
            | ServerParticleKind::SoulFireFlame
            | ServerParticleKind::SmallFlame
            | ServerParticleKind::Soul
            | ServerParticleKind::SculkSoul
    ) {
        p.pos += DVec3::new(
            (unit() - unit()) * 0.05,
            (unit() - unit()) * 0.05,
            (unit() - unit()) * 0.05,
        );
        p.prev_pos = p.pos;
    }
    if matches!(
        kind,
        ServerParticleKind::CrimsonSpore
            | ServerParticleKind::WarpedSpore
            | ServerParticleKind::SporeBlossomAir
            | ServerParticleKind::Underwater
            | ServerParticleKind::DragonBreath
    ) {
        p.vel = velocity;
    }
    if matches!(
        kind,
        ServerParticleKind::CherryLeaves
            | ServerParticleKind::PaleOakLeaves
            | ServerParticleKind::TintedLeaves
    ) {
        p.rot = 0.0;
        p.rot_o = 0.0;
        p.pitch = if state.seed & 1 == 0 {
            -30.0f32.to_radians()
        } else {
            30.0f32.to_radians()
        };
    }
    if kind == ServerParticleKind::AngryVillager {
        p.pos.y += 0.5;
        p.prev_pos.y += 0.5;
    }
    if matches!(
        kind,
        ServerParticleKind::CrimsonSpore
            | ServerParticleKind::WarpedSpore
            | ServerParticleKind::SporeBlossomAir
            | ServerParticleKind::Underwater
    ) {
        p.pos.y -= 0.125;
        p.prev_pos.y -= 0.125;
    }
    store.pending.push(p);
    true
}

/// Apply Java `PlayerCloudParticle.tick` attraction after the normal particle
/// tick. Inputs are `(player position, player delta movement)` for nonspectator
/// players.
pub(super) fn apply_player_attraction(p: &mut Particle, player_positions: &[(DVec3, DVec3)]) {
    if !matches!(
        &p.kind,
        Kind::Atmosphere(State {
            kind: ServerParticleKind::Cloud | ServerParticleKind::Sneeze,
            ..
        })
    ) {
        return;
    }
    let mut nearest: Option<(f64, DVec3, DVec3)> = None;
    for (position, velocity) in player_positions {
        let distance_squared = p.pos.distance_squared(*position);
        if distance_squared < 4.0 && nearest.is_none_or(|(best, _, _)| distance_squared < best) {
            nearest = Some((distance_squared, *position, *velocity));
        }
    }
    if let Some((_, position, velocity)) = nearest {
        if p.pos.y > position.y {
            p.pos.y += (position.y - p.pos.y) * 0.2;
            p.vel.y += (velocity.y - p.vel.y) * 0.2;
        }
    }
}

/// Full family-owned Java update. Collision is used by default; Flame/cloud,
/// suspended particles, dragon breath and leaf provider behavior override it as
/// Java does.
pub(super) fn tick(
    p: &mut Particle,
    chunks: &ChunkStore,
    _atlas: &AtlasUVMap,
    _children: &mut Vec<Particle>,
) -> bool {
    let Kind::Atmosphere(mut state) = p.kind else {
        return false;
    };
    p.prev_pos = p.pos;
    if p.age >= p.lifetime {
        return false;
    }
    p.age += 1;
    let k = state.kind;
    match k {
        ServerParticleKind::CherryLeaves
        | ServerParticleKind::PaleOakLeaves
        | ServerParticleKind::TintedLeaves => {
            let alive = (300 - (p.lifetime - p.age)) as f64 / 300.0;
            let wind = if state.mode == 1 { 2.0 } else { 10.0 };
            let phase = f64::from(state.seed) / 65535.0 * std::f64::consts::PI / 3.0;
            let swirl = 1000.0 + f64::from(state.seed) / 65535.0 * 3000.0;
            let (mut ax, mut az) = (0.0, 0.0);
            if state.mode == 1 {
                ax += wind * phase.cos() * alive.powf(1.25);
                az += wind * phase.sin() * alive.powf(1.25);
            } else {
                ax += alive * (alive * swirl).cos() * wind;
                az += alive * (alive * swirl).sin() * wind;
            }
            p.vel.x += ax * 0.0025;
            p.vel.z += az * 0.0025;
            p.vel.y -= p.gravity;
            p.rot_o = p.rot;
            p.pitch += if state.seed & 2 == 0 {
                -5.0f32.to_radians() / 20.0
            } else {
                5.0f32.to_radians() / 20.0
            };
            p.rot += p.pitch / 20.0;
            p.move_with_collision(chunks);
            if p.on_ground || (p.age > 1 && (p.vel.x == 0.0 || p.vel.z == 0.0)) {
                return false;
            }
            p.vel *= p.friction;
        }
        ServerParticleKind::DragonBreath => {
            if state.hit_ground {
                p.vel.y += 0.002;
            }
            let before = p.pos.y;
            move_without_collision(p);
            if p.pos.y == before {
                p.vel.x *= 1.1;
                p.vel.z *= 1.1;
            }
            p.vel.x *= p.friction;
            p.vel.z *= p.friction;
            if state.hit_ground {
                p.vel.y *= p.friction;
            }
            if p.on_ground {
                state.hit_ground = true;
                p.vel.y = 0.0;
            }
            p.kind = Kind::Atmosphere(state);
        }
        ServerParticleKind::CopperFireFlame
        | ServerParticleKind::Flame
        | ServerParticleKind::SoulFireFlame
        | ServerParticleKind::SmallFlame => {
            p.vel.y -= 0.04 * p.gravity;
            move_without_collision(p);
            p.vel *= p.friction;
        }
        ServerParticleKind::Cloud | ServerParticleKind::Sneeze => {
            p.vel.y -= 0.04 * p.gravity;
            move_without_collision(p);
            p.vel *= p.friction;
            p.size = p.base_size * (((p.age as f32) / p.lifetime as f32 * 32.0).clamp(0.0, 1.0));
        }
        ServerParticleKind::CrimsonSpore
        | ServerParticleKind::WarpedSpore
        | ServerParticleKind::SporeBlossomAir
        | ServerParticleKind::Underwater
        | ServerParticleKind::Mycelium => {
            p.vel.y -= 0.04 * p.gravity;
            move_without_collision(p);
            p.vel *= p.friction;
        }
        ServerParticleKind::Heart
        | ServerParticleKind::AngryVillager
        | ServerParticleKind::HappyVillager
        | ServerParticleKind::Composter => {
            p.vel.y -= 0.04 * p.gravity;
            move_without_collision(p);
            p.vel *= p.friction;
            if matches!(
                k,
                ServerParticleKind::Heart | ServerParticleKind::AngryVillager
            ) {
                p.size = p.base_size * ((p.age as f32 / p.lifetime as f32 * 32.0).clamp(0.0, 1.0));
            }
        }
        ServerParticleKind::Firefly => {
            p.vel.y -= 0.04 * p.gravity;
            p.move_with_collision(chunks);
            p.vel *= p.friction;
            let (x, y, z) = (
                p.pos.x.floor() as i32,
                p.pos.y.floor() as i32,
                p.pos.z.floor() as i32,
            );
            if !block::is_air(chunks.get_block_state(x, y, z)) {
                return false;
            }
            let progress = (p.age as f32 / p.lifetime as f32).clamp(0.0, 1.0);
            p.alpha = if progress >= 0.7 {
                (1.0 - progress) / 0.3
            } else if progress <= 0.5 {
                progress / 0.5
            } else {
                1.0
            };
            if p.age == 1 || rand() > 0.95 {
                p.vel = DVec3::new(
                    f64::from(rand() * 0.1 - 0.05),
                    f64::from(rand() * 0.1 - 0.05),
                    f64::from(rand() * 0.1 - 0.05),
                );
            }
        }
        ServerParticleKind::Snowflake => {
            p.vel.y -= 0.04 * p.gravity;
            p.move_with_collision(chunks);
            p.vel.x *= 0.95;
            p.vel.y *= 0.9;
            p.vel.z *= 0.95;
        }
        ServerParticleKind::Infested => {
            p.vel.y -= 0.04 * p.gravity;
            move_without_collision(p);
            if p.pos.y == p.prev_pos.y {
                p.vel.x *= 1.1;
                p.vel.z *= 1.1;
            }
            p.vel *= p.friction;
        }
        ServerParticleKind::FallingSporeBlossom => {
            p.vel.y -= p.gravity;
            p.move_with_collision(chunks);
            p.vel *= p.friction;
        }
        ServerParticleKind::Ash | ServerParticleKind::WhiteAsh => {
            p.vel.y -= 0.04 * p.gravity;
            move_without_collision(p);
            if p.pos.y == p.prev_pos.y {
                p.vel.x *= 1.1;
                p.vel.z *= 1.1;
            }
            p.vel *= p.friction;
        }
        _ => {
            p.vel.y -= 0.04 * p.gravity;
            p.move_with_collision(chunks);
            if matches!(
                k,
                ServerParticleKind::LargeSmoke
                    | ServerParticleKind::WhiteSmoke
                    | ServerParticleKind::NoxiousGas
            ) && p.pos.y == p.prev_pos.y
            {
                p.vel.x *= 1.1;
                p.vel.z *= 1.1;
            }
            p.vel *= p.friction;
        }
    }
    // Vanilla BaseAshSmoke and animated ambient providers update sprites by age.
    let atlas = _atlas;
    if let Some(frames) = atlas.particle_sprite_names(sprites_for(k)) {
        if !frames.is_empty() {
            let ix = if matches!(
                k,
                ServerParticleKind::Cloud
                    | ServerParticleKind::Sneeze
                    | ServerParticleKind::DragonBreath
                    | ServerParticleKind::LargeSmoke
                    | ServerParticleKind::WhiteSmoke
                    | ServerParticleKind::Ash
                    | ServerParticleKind::WhiteAsh
                    | ServerParticleKind::NoxiousGas
                    | ServerParticleKind::Snowflake
                    | ServerParticleKind::Soul
                    | ServerParticleKind::SculkSoul
                    | ServerParticleKind::Infested
            ) {
                descriptor_age_frame(p.age, p.lifetime, frames.len())
            } else {
                usize::from(state.frame) % frames.len()
            };
            if let Some(region) = descriptor_frame(atlas, sprites_for(k), ix) {
                p.set_sprite(&region);
            }
        }
    }
    if matches!(
        k,
        ServerParticleKind::Flame
            | ServerParticleKind::CopperFireFlame
            | ServerParticleKind::SoulFireFlame
            | ServerParticleKind::SmallFlame
    ) {
        p.size = p.base_size * (1.0 - (p.age as f32 / p.lifetime as f32).powi(2) * 0.5);
    }
    if matches!(
        k,
        ServerParticleKind::NoxiousGas
            | ServerParticleKind::LargeSmoke
            | ServerParticleKind::WhiteSmoke
            | ServerParticleKind::Ash
            | ServerParticleKind::WhiteAsh
    ) {
        p.size = p.base_size * ((p.age as f32 / p.lifetime as f32 * 32.0).clamp(0.0, 1.0));
    }
    if k == ServerParticleKind::NoxiousGas && p.age > p.lifetime / 2 {
        p.alpha = ((p.lifetime - (p.age - p.lifetime / 2)) as f32 / p.lifetime as f32).max(0.0);
    }
    if matches!(k, ServerParticleKind::Soul | ServerParticleKind::SculkSoul) {
        p.size = p.base_size * 1.5;
    }
    if k == ServerParticleKind::Infested {
        p.size = p.base_size * 0.75;
    }
    true
}

pub(super) fn appearance(p: &Particle, partial_tick: f32) -> Option<Appearance> {
    let Kind::Atmosphere(state) = p.kind else {
        return None;
    };
    let age = p.age as f32 + partial_tick;
    let mut size = p.size;
    let rotation = if matches!(
        state.kind,
        ServerParticleKind::CherryLeaves
            | ServerParticleKind::PaleOakLeaves
            | ServerParticleKind::TintedLeaves
    ) {
        Quat::from_rotation_z(p.rot)
    } else {
        p.rotation
    };
    let mut color = p.color;
    let mut alpha = p.alpha;
    match state.kind {
        ServerParticleKind::CopperFireFlame
        | ServerParticleKind::Flame
        | ServerParticleKind::SoulFireFlame
        | ServerParticleKind::SmallFlame => {
            size = p.base_size * (1.0 - (age / p.lifetime as f32).powi(2) * 0.5);
        }
        ServerParticleKind::Cloud
        | ServerParticleKind::Sneeze
        | ServerParticleKind::DragonBreath => {
            size = p.base_size * ((age / p.lifetime as f32 * 32.0).clamp(0.0, 1.0))
        }
        ServerParticleKind::CherryLeaves
        | ServerParticleKind::PaleOakLeaves
        | ServerParticleKind::TintedLeaves => {
            let roll = p.rot_o + (p.rot - p.rot_o) * partial_tick;
            return Some(Appearance {
                size,
                color,
                alpha,
                rotation: Quat::from_rotation_z(roll),
                second_rotation: None,
            });
        }
        ServerParticleKind::NoxiousGas => {
            let half = p.lifetime as f32 * 0.5;
            alpha = if age > half {
                ((p.lifetime as f32 - (age - half)) / p.lifetime as f32).max(0.0)
            } else {
                1.0
            };
        }
        ServerParticleKind::Firefly => {
            let progress = (age / p.lifetime as f32).clamp(0.0, 1.0);
            let fade = |in_time: f32, out_time: f32| {
                if progress >= 1.0 - in_time {
                    (1.0 - progress) / in_time
                } else if progress <= out_time {
                    progress / out_time
                } else {
                    1.0
                }
            };
            alpha = fade(0.3, 0.5);
        }
        ServerParticleKind::Heart | ServerParticleKind::AngryVillager => {
            size = p.base_size * ((age / p.lifetime as f32 * 32.0).clamp(0.0, 1.0))
        }
        _ => {}
    }
    if state.kind == ServerParticleKind::DragonBreath {
        color = p.color;
    }
    Some(Appearance {
        size,
        color,
        alpha,
        rotation,
        second_rotation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_31_registered_java_providers_have_an_owner() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let kinds = [
            ServerParticleKind::Cloud,
            ServerParticleKind::CopperFireFlame,
            ServerParticleKind::Flame,
            ServerParticleKind::SoulFireFlame,
            ServerParticleKind::SmallFlame,
            ServerParticleKind::LargeSmoke,
            ServerParticleKind::WhiteSmoke,
            ServerParticleKind::Sneeze,
            ServerParticleKind::Ash,
            ServerParticleKind::WhiteAsh,
            ServerParticleKind::CrimsonSpore,
            ServerParticleKind::WarpedSpore,
            ServerParticleKind::SporeBlossomAir,
            ServerParticleKind::FallingSporeBlossom,
            ServerParticleKind::Mycelium,
            ServerParticleKind::Underwater,
            ServerParticleKind::Firefly,
            ServerParticleKind::CherryLeaves,
            ServerParticleKind::PaleOakLeaves,
            ServerParticleKind::TintedLeaves,
            ServerParticleKind::AngryVillager,
            ServerParticleKind::HappyVillager,
            ServerParticleKind::Composter,
            ServerParticleKind::Heart,
            ServerParticleKind::Snowflake,
            ServerParticleKind::NoxiousGas,
            ServerParticleKind::SulfurCubeGoo,
            ServerParticleKind::DragonBreath,
            ServerParticleKind::Infested,
            ServerParticleKind::Soul,
            ServerParticleKind::SculkSoul,
        ];
        assert_eq!(kinds.len(), 31);
        let opaque_sprite =
            crate::renderer::chunk::atlas::AtlasUVMap::test_empty().missing_region();
        let translucent_sprite = crate::renderer::chunk::atlas::AtlasRegion {
            translucent: true,
            ..opaque_sprite
        };
        assert!(!provider_translucency(
            ServerParticleKind::SulfurCubeGoo,
            opaque_sprite
        ));
        assert!(provider_translucency(
            ServerParticleKind::SulfurCubeGoo,
            translucent_sprite
        ));
        let colors = std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        let mut store = ParticleStore::new(
            AtlasUVMap::test_empty(),
            colors.clone(),
            colors.clone(),
            colors,
        );
        let registry = BlockRegistry::test_empty();
        let chunks = ChunkStore::new(2);
        for (index, kind) in kinds.into_iter().enumerate() {
            assert!(supports(kind));
            let options = match kind {
                ServerParticleKind::TintedLeaves => {
                    ServerParticleOptions::Color { color: 0x35a4d0 }
                }
                ServerParticleKind::DragonBreath => ServerParticleOptions::Power { power: 1.75 },
                _ => ServerParticleOptions::Simple,
            };
            assert!(spawn(
                &mut store,
                kind,
                options,
                DVec3::new(1.0, 2.0, 3.0),
                DVec3::new(0.1, 0.2, 0.3),
                &registry,
                &chunks,
                &HashMap::new()
            ));
            assert_eq!(
                store.pending.len(),
                index + 1,
                "{kind:?} must create a family particle"
            );
            match store.pending[index].kind {
                Kind::Atmosphere(state) => {
                    assert_eq!(state.kind, kind);
                    let expected_layer = matches!(
                        kind,
                        ServerParticleKind::Cloud
                            | ServerParticleKind::Sneeze
                            | ServerParticleKind::Firefly
                            | ServerParticleKind::NoxiousGas
                            | ServerParticleKind::Infested
                            | ServerParticleKind::Soul
                            | ServerParticleKind::SculkSoul
                    );
                    assert_eq!(
                        translucent(state),
                        expected_layer,
                        "{kind:?} Java render layer"
                    );
                    assert_eq!(
                        store.pending[index].kind.translucent(),
                        expected_layer,
                        "Kind::translucent for {kind:?}"
                    );
                }
                _ => panic!("{kind:?} did not construct its atmosphere state"),
            }
            if kind == ServerParticleKind::DragonBreath {
                let v = store.pending[index].vel;
                assert!((v - DVec3::new(0.175, 0.275, 0.525)).length() < 1.0e-12);
            }
            if kind == ServerParticleKind::TintedLeaves {
                assert_eq!(
                    store.pending[index].color,
                    [
                        0x35 as f32 / 255.0,
                        0xa4 as f32 / 255.0,
                        0xd0 as f32 / 255.0
                    ]
                );
            }
        }
    }

    #[test]
    fn crimson_spore_velocity_uses_java_gaussian_coefficients() {
        const SEED: u64 = 0x4352_494d_534f_4e;
        fastrand::seed(SEED);
        let expected = DVec3::new(
            super::super::next_gaussian() * f64::from(1.0e-6_f32),
            super::super::next_gaussian() * f64::from(1.0e-4_f32),
            super::super::next_gaussian() * f64::from(1.0e-6_f32),
        );
        fastrand::seed(SEED);
        assert_eq!(crimson_spore_velocity(), expected);

        fastrand::seed(SEED);
        let mut sum = [0.0; 3];
        let mut squares = [0.0; 3];
        let n = 8192;
        let sample_count = n as f64;
        for _ in 0..n {
            let v = crimson_spore_velocity();
            let normalized = [v.x / 1.0e-6, v.y / 1.0e-4, v.z / 1.0e-6];
            for axis in 0..3 {
                sum[axis] += normalized[axis];
                squares[axis] += normalized[axis] * normalized[axis];
            }
        }
        for axis in 0..3 {
            let mean = sum[axis] / sample_count;
            let variance = squares[axis] / sample_count - mean * mean;
            assert!(mean.abs() < 0.04, "axis {axis} mean={mean}");
            assert!(
                (0.94..1.06).contains(&variance),
                "axis {axis} variance={variance}"
            );
        }
    }

    #[test]
    fn player_cloud_attraction_matches_java_nearest_player_y_interpolation() {
        let mut p = Particle::special(
            Kind::Atmosphere(State::new(ServerParticleKind::Cloud, 0, 0, 0, true)),
            DVec3::new(0.0, 5.0, 0.0),
            DVec3::new(0.0, 1.0, 0.0),
            None,
            20,
            0.2,
            AtlasUVMap::test_empty().missing_region(),
        );
        let players = [
            (DVec3::new(0.0, 4.0, 1.0), DVec3::new(0.0, -1.0, 0.0)),
            (DVec3::new(0.0, 3.0, 0.0), DVec3::ZERO),
        ];
        apply_player_attraction(&mut p, &players);
        assert_eq!(p.pos.y, 4.8);
        assert!((p.vel.y - 0.6).abs() < 1.0e-12);
        p.pos.y = 3.0;
        p.vel.y = 0.25;
        apply_player_attraction(&mut p, &players);
        assert_eq!(p.pos.y, 3.0);
        assert_eq!(p.vel.y, 0.25);
    }

    #[test]
    fn cherry_leaf_tick_matches_seeded_first_wind_acceleration() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut p = Particle::special(
            Kind::Atmosphere(State::new(ServerParticleKind::CherryLeaves, 1, 0, 0, false)),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            300,
            0.05,
            AtlasUVMap::test_empty().missing_region(),
        );
        p.gravity = 0.00075;
        p.friction = 1.0;
        let chunks = ChunkStore::new(2);
        let atlas = AtlasUVMap::test_empty();
        assert!(tick(&mut p, &chunks, &atlas, &mut Vec::new()));
        let relative: f64 = 1.0 / 300.0;
        let expected = 2.0 * relative.powf(1.25) * 0.0025;
        assert!((p.vel.x - expected).abs() < 1.0e-12);
        assert_eq!(p.age, 1);
        assert_eq!(descriptor_age_frame(5, 10, 7), 3);
    }
}
