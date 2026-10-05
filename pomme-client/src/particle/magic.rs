//! Java 26.2 combat, magic and animated particle providers.
use std::collections::HashMap;

use glam::{DVec3, Quat};

use super::{
    Appearance, Kind, Particle, ParticleStore, ServerParticleKind, ServerParticleOptions,
    world_brightness,
};
use crate::renderer::chunk::atlas::AtlasUVMap;
use crate::renderer::chunk::mesher::BiomeClimate;
use crate::world::block::registry::BlockRegistry;
use crate::world::chunk::ChunkStore;

#[derive(Clone, Copy, Debug)]
pub(super) struct State {
    pub(super) kind: ServerParticleKind,
    start: DVec3,
    velocity: DVec3,
    roll: f32,
    frame_count: usize,
    pub(super) firework_trail: bool,
    pub(super) firework_twinkle: bool,
    pub(super) firework_fade: Option<[u32; 3]>,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.start.to_array().map(f64::to_bits) == other.start.to_array().map(f64::to_bits)
            && self.velocity.to_array().map(f64::to_bits)
                == other.velocity.to_array().map(f64::to_bits)
            && self.roll.to_bits() == other.roll.to_bits()
            && self.frame_count == other.frame_count
            && self.firework_trail == other.firework_trail
            && self.firework_twinkle == other.firework_twinkle
            && self.firework_fade == other.firework_fade
    }
}

impl Eq for State {}

/// Java `SingleQuadParticle.Layer` ownership for the dispatcher seam.
pub(super) fn translucent(kind: ServerParticleKind) -> bool {
    matches!(
        kind,
        ServerParticleKind::Firework
            | ServerParticleKind::Flash
            | ServerParticleKind::SculkCharge
            | ServerParticleKind::SculkChargePop
    )
}

pub(super) fn supports(kind: ServerParticleKind) -> bool {
    matches!(
        kind,
        ServerParticleKind::Crit
            | ServerParticleKind::EnchantedHit
            | ServerParticleKind::DamageIndicator
            | ServerParticleKind::Enchant
            | ServerParticleKind::Note
            | ServerParticleKind::Portal
            | ServerParticleKind::ReversePortal
            | ServerParticleKind::Glow
            | ServerParticleKind::WaxOn
            | ServerParticleKind::WaxOff
            | ServerParticleKind::ElectricSpark
            | ServerParticleKind::Scrape
            | ServerParticleKind::EggCrack
            | ServerParticleKind::DustPlume
            | ServerParticleKind::TrialSpawnerDetection
            | ServerParticleKind::TrialSpawnerDetectionOminous
            | ServerParticleKind::VaultConnection
            | ServerParticleKind::OminousSpawning
            | ServerParticleKind::PauseMobGrowth
            | ServerParticleKind::ResetMobGrowth
            | ServerParticleKind::Firework
            | ServerParticleKind::Flash
            | ServerParticleKind::SculkCharge
            | ServerParticleKind::SculkChargePop
            | ServerParticleKind::SonicBoom
            | ServerParticleKind::SweepAttack
            | ServerParticleKind::Gust
            | ServerParticleKind::SmallGust
    )
}

fn descriptor(kind: ServerParticleKind) -> &'static str {
    match kind {
        ServerParticleKind::Crit => "minecraft:crit",
        ServerParticleKind::EnchantedHit => "minecraft:enchanted_hit",
        ServerParticleKind::DamageIndicator => "minecraft:damage_indicator",
        ServerParticleKind::Enchant => "minecraft:enchant",
        ServerParticleKind::Note => "minecraft:note",
        ServerParticleKind::Portal => "minecraft:portal",
        ServerParticleKind::ReversePortal => "minecraft:reverse_portal",
        ServerParticleKind::Glow => "minecraft:glow",
        ServerParticleKind::WaxOn => "minecraft:wax_on",
        ServerParticleKind::WaxOff => "minecraft:wax_off",
        ServerParticleKind::ElectricSpark => "minecraft:electric_spark",
        ServerParticleKind::Scrape => "minecraft:scrape",
        ServerParticleKind::EggCrack => "minecraft:egg_crack",
        ServerParticleKind::DustPlume => "minecraft:dust_plume",
        ServerParticleKind::TrialSpawnerDetection => "minecraft:trial_spawner_detection",
        ServerParticleKind::TrialSpawnerDetectionOminous => {
            "minecraft:trial_spawner_detection_ominous"
        }
        ServerParticleKind::VaultConnection => "minecraft:vault_connection",
        ServerParticleKind::OminousSpawning => "minecraft:ominous_spawning",
        ServerParticleKind::PauseMobGrowth => "minecraft:pause_mob_growth",
        ServerParticleKind::ResetMobGrowth => "minecraft:reset_mob_growth",
        ServerParticleKind::Firework => "minecraft:firework",
        ServerParticleKind::Flash => "minecraft:flash",
        ServerParticleKind::SculkCharge => "minecraft:sculk_charge",
        ServerParticleKind::SculkChargePop => "minecraft:sculk_charge_pop",
        ServerParticleKind::SonicBoom => "minecraft:sonic_boom",
        ServerParticleKind::SweepAttack => "minecraft:sweep_attack",
        ServerParticleKind::Gust => "minecraft:gust",
        ServerParticleKind::SmallGust => "minecraft:small_gust",
        _ => unreachable!("not a magic particle"),
    }
}

fn rand() -> f32 {
    fastrand::f32()
}
fn rand_range(min: f32, max: f32) -> f32 {
    min + rand() * (max - min)
}
fn rand_life(min: i32, max_exclusive: i32) -> i32 {
    min + fastrand::i32(0..(max_exclusive - min))
}
fn base_particle_velocity(input: DVec3) -> DVec3 {
    let direction = input
        + DVec3::new(
            rand_range(-0.4, 0.4) as f64,
            rand_range(-0.4, 0.4) as f64,
            rand_range(-0.4, 0.4) as f64,
        );
    let speed = ((fastrand::f64() + fastrand::f64() + 1.0) * 0.15 * 0.4) / direction.length();
    DVec3::new(
        direction.x * speed,
        direction.y * speed + 0.1,
        direction.z * speed,
    )
}

fn portal_progress(t: f64) -> f64 {
    1.0 + t - 2.0 * t * t
}
fn reverse_portal_progress(age: i32, lifetime: i32) -> f64 {
    let age = f64::from(age);
    age * (age + 1.0) / (2.0 * f64::from(lifetime))
}
fn enchant_position(start: DVec3, velocity: DVec3, t: f64) -> DVec3 {
    let progress = 1.0 - t;
    start + velocity * progress - DVec3::new(0.0, t.powi(4) * 1.2, 0.0)
}
fn crit_color_tick(color: &mut [f32; 3]) {
    color[1] *= 0.96;
    color[2] *= 0.9;
}
fn firework_alpha(age: i32, lifetime: i32) -> f32 {
    if age > lifetime / 2 {
        1.0 - ((age - lifetime / 2) as f32 / lifetime as f32)
    } else {
        0.99
    }
}
fn firework_fade_tick(color: &mut [f32; 3], fade: [f32; 3]) {
    for (channel, target) in color.iter_mut().zip(fade) {
        *channel += (target - *channel) * 0.2;
    }
}

/// Java `FireworkParticles.SparkParticle` constructor for the rocket entity's
/// composite explosion path. The `firework` LevelParticles provider uses the
/// same spark with trail/twinkle disabled.
pub(super) fn firework_flash(atlas: &AtlasUVMap, pos: DVec3, rgb: i32) -> Option<Particle> {
    let kind = ServerParticleKind::Flash;
    let name = descriptor(kind);
    let frame = super::descriptor_frame(atlas, name, 0)?;
    let state = State {
        kind,
        start: pos,
        velocity: DVec3::ZERO,
        roll: 0.0,
        frame_count: atlas.particle_sprite_names(name)?.len(),
        firework_trail: false,
        firework_twinkle: false,
        firework_fade: None,
    };
    let mut particle = Particle::special(Kind::Magic(state), pos, DVec3::ZERO, None, 4, 7.1, frame);
    particle.color = [
        ((rgb >> 16) & 255) as f32 / 255.0,
        ((rgb >> 8) & 255) as f32 / 255.0,
        (rgb & 255) as f32 / 255.0,
    ];
    particle.alpha = 1.0;
    Some(particle)
}

pub(super) fn firework_spark(
    atlas: &AtlasUVMap,
    pos: DVec3,
    velocity: DVec3,
    color: [f32; 3],
    fade_color: Option<[f32; 3]>,
    trail: bool,
    twinkle: bool,
) -> Option<Particle> {
    let name = descriptor(ServerParticleKind::Firework);
    let names = atlas.particle_sprite_names(name)?;
    let frame = super::descriptor_frame(atlas, name, 0)?;
    let state = State {
        kind: ServerParticleKind::Firework,
        start: pos,
        velocity,
        roll: 0.0,
        frame_count: names.len(),
        firework_trail: trail,
        firework_twinkle: twinkle,
        firework_fade: fade_color.map(|rgb| rgb.map(f32::to_bits)),
    };
    let mut particle = Particle::special(
        Kind::Magic(state),
        pos,
        velocity,
        None,
        rand_life(48, 60),
        rand_range(0.075, 0.15),
        frame,
    );
    particle.color = color;
    particle.alpha = 0.99;
    particle.friction = 0.91;
    particle.gravity = 0.1;
    Some(particle)
}

/// Maps each kind to its Java 26.2 `ParticleResources.registerProviders`
/// provider.
pub(super) fn spawn(
    store: &mut ParticleStore,
    kind: ServerParticleKind,
    options: ServerParticleOptions,
    pos: DVec3,
    vel: DVec3,
    _registry: &BlockRegistry,
    chunks: &ChunkStore,
    _biome_climate: &HashMap<u32, BiomeClimate>,
) -> bool {
    if !supports(kind) {
        return false;
    }
    let name = descriptor(kind);
    let Some(names) = store.uv_map.particle_sprite_names(name) else {
        return false;
    };
    if names.is_empty() {
        return false;
    }
    let frame_count = names.len();
    let roll = match options {
        ServerParticleOptions::SculkCharge { roll } => roll,
        _ => 0.0,
    };
    let state = State {
        kind,
        start: pos,
        velocity: vel,
        roll,
        frame_count,
        firework_trail: false,
        firework_twinkle: false,
        firework_fade: None,
    };
    let (lifetime, size, color, alpha, velocity, _provider_light) = match kind {
        // CritParticle: velocity is damped then packet velocity contributes 0.4;
        // constructor performs one initial tick in Java, reflected in initial state.
        ServerParticleKind::Crit
        | ServerParticleKind::EnchantedHit
        | ServerParticleKind::DamageIndicator => {
            let input = if kind == ServerParticleKind::DamageIndicator {
                vel + DVec3::Y
            } else {
                vel
            };
            let v = base_particle_velocity(DVec3::ZERO) * 0.1 + input * 0.4;
            let life = (6.0 / rand_range(0.6, 1.4)) as i32;
            let life = life.max(1);
            let c = rand_range(0.6, 0.9);
            let color = if kind == ServerParticleKind::EnchantedHit {
                [c * 0.3, c * 0.8, c]
            } else {
                [c; 3]
            };
            (
                if kind == ServerParticleKind::DamageIndicator {
                    20
                } else {
                    life
                },
                rand_range(0.075, 0.15),
                color,
                1.0,
                v,
                0.0,
            )
        }
        ServerParticleKind::Note => {
            let t = vel.x as f32;
            let rgb = [0.0, 1.0 / 3.0, 2.0 / 3.0].map(|offset| {
                ((t + offset) * std::f32::consts::TAU)
                    .sin()
                    .mul_add(0.65, 0.35)
                    .max(0.0)
            });
            let note_velocity =
                base_particle_velocity(DVec3::ZERO) * 0.01 + DVec3::new(0.0, 0.2, 0.0);
            (6, rand_range(0.15, 0.3), rgb, 1.0, note_velocity, 0.0)
        }
        ServerParticleKind::Portal => {
            let br = rand_range(0.4, 1.0);
            (
                rand_life(40, 50),
                rand_range(0.05, 0.07),
                [0.9 * br, 0.3 * br, br],
                1.0,
                vel,
                0.0,
            )
        }
        ServerParticleKind::ReversePortal => {
            let br = rand_range(0.4, 1.0);
            (
                rand_life(60, 62),
                rand_range(0.075, 0.105),
                [0.9 * br, 0.3 * br, br],
                1.0,
                vel,
                0.0,
            )
        }
        ServerParticleKind::Enchant | ServerParticleKind::VaultConnection => {
            let b = rand_range(0.4, 1.0);
            let v = vel;
            let alpha = if kind == ServerParticleKind::VaultConnection {
                0.0
            } else {
                1.0
            };
            let size = if kind == ServerParticleKind::VaultConnection {
                0.1 * rand_range(0.2, 0.7) * 1.5
            } else {
                0.1 * rand_range(0.2, 0.7)
            };
            (
                rand_life(30, 40),
                size,
                [b * 0.9, b * 0.9, b],
                alpha,
                v,
                if kind == ServerParticleKind::VaultConnection {
                    1.0
                } else {
                    0.0
                },
            )
        }
        ServerParticleKind::Glow
        | ServerParticleKind::WaxOn
        | ServerParticleKind::WaxOff
        | ServerParticleKind::ElectricSpark
        | ServerParticleKind::Scrape => {
            let (c, factor, life) = match kind {
                ServerParticleKind::ElectricSpark => ([1.0, 0.9, 1.0], 0.25, rand_life(2, 4)),
                ServerParticleKind::Scrape => (
                    if fastrand::bool() {
                        [0.29, 0.58, 0.51]
                    } else {
                        [0.43, 0.77, 0.62]
                    },
                    0.01,
                    rand_life(10, 40),
                ),
                ServerParticleKind::WaxOn => ([0.91, 0.55, 0.08], 0.005, rand_life(10, 40)),
                ServerParticleKind::WaxOff => ([1.0, 0.9, 1.0], 0.005, rand_life(10, 40)),
                _ => (
                    if fastrand::bool() {
                        [0.6, 1.0, 0.8]
                    } else {
                        [0.08, 0.4, 0.4]
                    },
                    0.0,
                    (8.0 / rand_range(0.2, 1.0)) as i32,
                ),
            };
            let scale = if matches!(kind, ServerParticleKind::WaxOn | ServerParticleKind::WaxOff) {
                DVec3::new(0.5, 1.0, 0.5)
            } else {
                DVec3::ONE
            };
            (
                life,
                rand_range(0.075, 0.15),
                c,
                1.0,
                if kind == ServerParticleKind::Glow {
                    let mut v = base_particle_velocity(DVec3::new(
                        0.5 - fastrand::f64(),
                        vel.y,
                        0.5 - fastrand::f64(),
                    ));
                    v.y *= 0.2;
                    if vel.x == 0.0 && vel.z == 0.0 {
                        v.x *= 0.1;
                        v.z *= 0.1;
                    }
                    v
                } else {
                    DVec3::new(
                        vel.x * factor * scale.x,
                        vel.y
                            * if matches!(
                                kind,
                                ServerParticleKind::WaxOn | ServerParticleKind::WaxOff
                            ) {
                                0.01
                            } else {
                                factor
                            },
                        vel.z * factor * scale.z,
                    )
                },
                1.0,
            )
        }
        ServerParticleKind::EggCrack => (
            (20.0 / rand_range(0.2, 1.0)) as i32,
            rand_range(0.1, 0.2) * rand_range(0.5, 1.1),
            [1.0; 3],
            1.0,
            base_particle_velocity(vel) * 0.02,
            0.0,
        ),
        ServerParticleKind::DustPlume => {
            let shade = rand_range(0.0, 0.2);
            (
                (7.0 / rand_range(0.2, 1.0)) as i32,
                rand_range(0.075, 0.15),
                [
                    122.0 / 255.0 - shade,
                    53.0 / 255.0 - shade,
                    2.0 / 255.0 - shade,
                ],
                1.0,
                {
                    let base = base_particle_velocity(DVec3::ZERO);
                    DVec3::new(
                        base.x * 0.7 + vel.x,
                        base.y * 0.6 + vel.y + 0.15,
                        base.z * 0.7 + vel.z,
                    )
                },
                0.0,
            )
        }
        ServerParticleKind::TrialSpawnerDetection
        | ServerParticleKind::TrialSpawnerDetectionOminous => (
            (8.0 / rand_range(0.5, 1.0) * 1.5) as i32,
            rand_range(0.1125, 0.225),
            [1.0; 3],
            1.0,
            {
                let base = base_particle_velocity(DVec3::ZERO);
                DVec3::new(vel.x, vel.y + base.y * 0.9, vel.z)
            },
            1.0,
        ),
        ServerParticleKind::OminousSpawning => (
            rand_life(25, 30),
            rand_range(0.02, 0.07) * rand_range(3.0, 5.0),
            unpack_color(-12210434),
            1.0,
            vel,
            1.0,
        ),
        ServerParticleKind::PauseMobGrowth => (
            8,
            rand_range(0.05, 0.22),
            [1.0; 3],
            1.0,
            vel + DVec3::new(0.0, -0.03, 0.0),
            0.0,
        ),
        ServerParticleKind::ResetMobGrowth => (
            8,
            rand_range(0.05, 0.22),
            [1.0; 3],
            1.0,
            vel + DVec3::new(0.0, 0.03, 0.0),
            0.0,
        ),
        ServerParticleKind::Firework => (
            rand_life(48, 60),
            rand_range(0.075, 0.15),
            [1.0; 3],
            0.99,
            vel,
            0.0,
        ),
        ServerParticleKind::Flash => {
            let (c, a) = match options {
                ServerParticleOptions::Color { color } => (
                    unpack_color(color),
                    ((color as u32 >> 24) & 255) as f32 / 255.0,
                ),
                _ => ([1.0; 3], 1.0),
            };
            (4, 7.1, c, a, DVec3::ZERO, 0.0)
        }
        ServerParticleKind::SculkCharge => (
            rand_life(8, 20),
            rand_range(0.15, 0.3),
            [1.0; 3],
            1.0,
            vel,
            1.0,
        ),
        ServerParticleKind::SculkChargePop => (
            rand_life(6, 10),
            rand_range(0.1, 0.2),
            [1.0; 3],
            1.0,
            vel,
            1.0,
        ),
        ServerParticleKind::SonicBoom => (16, 1.5, [1.0; 3], 1.0, DVec3::ZERO, 1.0),
        ServerParticleKind::SweepAttack => (
            4,
            (1.0 - vel.x as f32 * 0.5).max(0.0),
            [rand_range(0.4, 1.0); 3],
            1.0,
            DVec3::ZERO,
            1.0,
        ),
        ServerParticleKind::Gust => (rand_life(12, 16), 1.0, [1.0; 3], 1.0, DVec3::ZERO, 1.0),
        ServerParticleKind::SmallGust => (rand_life(12, 16), 0.15, [1.0; 3], 1.0, DVec3::ZERO, 1.0),
        _ => return false,
    };
    let random_frame = matches!(
        kind,
        ServerParticleKind::Crit
            | ServerParticleKind::EnchantedHit
            | ServerParticleKind::DamageIndicator
            | ServerParticleKind::Enchant
            | ServerParticleKind::Note
            | ServerParticleKind::Portal
            | ServerParticleKind::ReversePortal
            | ServerParticleKind::EggCrack
            | ServerParticleKind::VaultConnection
            | ServerParticleKind::OminousSpawning
            | ServerParticleKind::PauseMobGrowth
            | ServerParticleKind::ResetMobGrowth
            | ServerParticleKind::Flash
    );
    let frame_index = if random_frame {
        fastrand::usize(0..frame_count)
    } else {
        0
    };
    let frame = super::descriptor_frame(&store.uv_map, name, frame_index)
        .expect("descriptor frame checked");
    let mut particle = Particle::special(
        Kind::Magic(state),
        pos,
        velocity,
        None,
        lifetime,
        size,
        frame,
    );
    particle.color = color;
    particle.alpha = alpha;
    let world_light = world_brightness(
        chunks,
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    particle.light = if matches!(
        kind,
        ServerParticleKind::TrialSpawnerDetection
            | ServerParticleKind::TrialSpawnerDetectionOminous
            | ServerParticleKind::OminousSpawning
            | ServerParticleKind::VaultConnection
            | ServerParticleKind::SculkCharge
            | ServerParticleKind::SculkChargePop
            | ServerParticleKind::SonicBoom
            | ServerParticleKind::SweepAttack
            | ServerParticleKind::Gust
            | ServerParticleKind::SmallGust
            | ServerParticleKind::Firework
    ) {
        1.0
    } else {
        world_light
    };
    particle.friction = match kind {
        ServerParticleKind::Crit
        | ServerParticleKind::EnchantedHit
        | ServerParticleKind::DamageIndicator => 0.7,
        ServerParticleKind::Note => 0.66,
        ServerParticleKind::PauseMobGrowth | ServerParticleKind::ResetMobGrowth => 0.98,
        ServerParticleKind::EggCrack => 0.99,
        ServerParticleKind::TrialSpawnerDetection
        | ServerParticleKind::TrialSpawnerDetectionOminous => 0.96,
        ServerParticleKind::Firework => 0.91,
        ServerParticleKind::Glow
        | ServerParticleKind::WaxOn
        | ServerParticleKind::WaxOff
        | ServerParticleKind::ElectricSpark
        | ServerParticleKind::Scrape
        | ServerParticleKind::SculkCharge
        | ServerParticleKind::SculkChargePop => 0.96,
        _ => 1.0,
    };
    particle.gravity = match kind {
        ServerParticleKind::Crit
        | ServerParticleKind::EnchantedHit
        | ServerParticleKind::DamageIndicator => 0.5,
        ServerParticleKind::DustPlume => 0.5,
        ServerParticleKind::TrialSpawnerDetection
        | ServerParticleKind::TrialSpawnerDetectionOminous => -0.1,
        ServerParticleKind::Firework => 0.1,
        _ => 0.0,
    };
    particle.rotation = Quat::from_rotation_z(roll);
    if kind == ServerParticleKind::Crit
        || kind == ServerParticleKind::EnchantedHit
        || kind == ServerParticleKind::DamageIndicator
    {
        particle.age = 1;
        particle.vel.y -= 0.02;
        particle.pos += particle.vel;
        particle.vel *= 0.7;
        particle.color[1] *= 0.96;
        particle.color[2] *= 0.9;
    }
    if matches!(
        kind,
        ServerParticleKind::Enchant
            | ServerParticleKind::VaultConnection
            | ServerParticleKind::OminousSpawning
    ) {
        particle.prev_pos += velocity;
        particle.pos += velocity;
    }
    if matches!(
        kind,
        ServerParticleKind::SculkCharge | ServerParticleKind::SculkChargePop
    ) {
        particle.rot = roll;
        particle.rot_o = roll;
    }
    store.push(particle);
    true
}

fn unpack_color(color: i32) -> [f32; 3] {
    [
        ((color >> 16) & 255) as f32 / 255.0,
        ((color >> 8) & 255) as f32 / 255.0,
        (color & 255) as f32 / 255.0,
    ]
}

pub(super) fn tick(
    p: &mut Particle,
    chunks: &ChunkStore,
    atlas: &AtlasUVMap,
    children: &mut Vec<Particle>,
) -> bool {
    let Kind::Magic(state) = p.kind else {
        return false;
    };
    if p.age >= p.lifetime {
        return false;
    }
    p.prev_pos = p.pos;
    p.age += 1;
    let t = p.age as f32 / p.lifetime.max(1) as f32;
    let k = state.kind;
    match k {
        ServerParticleKind::Portal => {
            // PortalParticle: pos = 1 - (-t + 2t²), plus the one-block vertical arc.
            let q = t as f64;
            let path = portal_progress(q);
            p.pos = state.start + state.velocity * path + DVec3::new(0.0, 1.0 - q, 0.0);
        }
        ServerParticleKind::ReversePortal => {
            p.pos = state.start + state.velocity * reverse_portal_progress(p.age, p.lifetime);
        }
        ServerParticleKind::Enchant
        | ServerParticleKind::VaultConnection
        | ServerParticleKind::OminousSpawning => {
            let progress = 1.0 - t;
            if k == ServerParticleKind::OminousSpawning {
                p.pos = state.start + state.velocity * progress as f64;
                let start = unpack_color(-12210434);
                p.color = std::array::from_fn(|i| start[i] + (1.0 - start[i]) * t);
            } else {
                p.pos = enchant_position(state.start, state.velocity, f64::from(t));
            }
            if k == ServerParticleKind::VaultConnection {
                p.alpha = (t * 0.6 / 0.25).min(0.6);
            }
        }
        ServerParticleKind::Gust
        | ServerParticleKind::SmallGust
        | ServerParticleKind::SonicBoom
        | ServerParticleKind::Flash
        | ServerParticleKind::SweepAttack => {}
        _ => {
            if k == ServerParticleKind::DustPlume {
                p.gravity *= 0.88;
                p.friction *= 0.92;
            }
            p.vel.y -= 0.04 * p.gravity;
            if matches!(
                k,
                ServerParticleKind::Note
                    | ServerParticleKind::DustPlume
                    | ServerParticleKind::TrialSpawnerDetection
                    | ServerParticleKind::TrialSpawnerDetectionOminous
                    | ServerParticleKind::PauseMobGrowth
                    | ServerParticleKind::ResetMobGrowth
                    | ServerParticleKind::Firework
            ) {
                p.move_with_collision(chunks);
            } else {
                p.pos += p.vel;
            }
            p.vel *= p.friction;
        }
    }
    if matches!(
        k,
        ServerParticleKind::Crit
            | ServerParticleKind::EnchantedHit
            | ServerParticleKind::DamageIndicator
    ) {
        crit_color_tick(&mut p.color);
    }
    if matches!(
        k,
        ServerParticleKind::DustPlume
            | ServerParticleKind::Glow
            | ServerParticleKind::WaxOn
            | ServerParticleKind::WaxOff
            | ServerParticleKind::ElectricSpark
            | ServerParticleKind::Scrape
            | ServerParticleKind::SculkCharge
            | ServerParticleKind::SculkChargePop
            | ServerParticleKind::TrialSpawnerDetection
            | ServerParticleKind::TrialSpawnerDetectionOminous
            | ServerParticleKind::Firework
            | ServerParticleKind::SonicBoom
            | ServerParticleKind::SweepAttack
            | ServerParticleKind::Gust
            | ServerParticleKind::SmallGust
    ) {
        if let Some(frame) = super::descriptor_frame(
            atlas,
            descriptor(k),
            super::descriptor_age_frame(p.age, p.lifetime, state.frame_count),
        ) {
            p.set_sprite(&frame);
        }
    }
    if k == ServerParticleKind::Crit
        || k == ServerParticleKind::EnchantedHit
        || k == ServerParticleKind::DamageIndicator
        || k == ServerParticleKind::Note
        || k == ServerParticleKind::EggCrack
        || k == ServerParticleKind::SweepAttack
    {
        p.size = p.base_size * (t * 32.0).clamp(0.0, 1.0);
    }
    if matches!(
        k,
        ServerParticleKind::Crit
            | ServerParticleKind::EnchantedHit
            | ServerParticleKind::DamageIndicator
            | ServerParticleKind::Note
            | ServerParticleKind::DustPlume
    ) {
        p.light = 1.0;
    }
    if k == ServerParticleKind::Flash {
        p.alpha = (0.6 - (p.age as f32 - 1.0) * 0.125).max(0.0);
        p.size = 7.1 * ((p.age as f32 - 1.0) * 0.25 * std::f32::consts::PI).sin();
    }
    let world_light = world_brightness(
        chunks,
        p.pos.x.floor() as i32,
        p.pos.y.floor() as i32,
        p.pos.z.floor() as i32,
    );
    p.light = match k {
        ServerParticleKind::TrialSpawnerDetection
        | ServerParticleKind::TrialSpawnerDetectionOminous
        | ServerParticleKind::OminousSpawning
        | ServerParticleKind::VaultConnection
        | ServerParticleKind::SculkCharge
        | ServerParticleKind::SculkChargePop
        | ServerParticleKind::SonicBoom
        | ServerParticleKind::SweepAttack
        | ServerParticleKind::Gust
        | ServerParticleKind::SmallGust
        | ServerParticleKind::Firework => 1.0,
        ServerParticleKind::Glow
        | ServerParticleKind::WaxOn
        | ServerParticleKind::WaxOff
        | ServerParticleKind::ElectricSpark
        | ServerParticleKind::Scrape => world_light.max(t),
        ServerParticleKind::Portal | ServerParticleKind::Enchant => world_light.max(t.powi(4)),
        _ => world_light,
    };
    if k == ServerParticleKind::Firework {
        p.alpha = firework_alpha(p.age, p.lifetime);
        if p.age > p.lifetime / 2 {
            if let Some(fade) = state.firework_fade {
                firework_fade_tick(&mut p.color, fade.map(f32::from_bits));
            }
        }
        if state.firework_trail && p.age < p.lifetime / 2 && (p.age + p.lifetime) % 2 == 0 {
            if let Some(mut child) = firework_spark(
                atlas,
                p.pos,
                DVec3::ZERO,
                p.color,
                state.firework_fade.map(|rgb| rgb.map(f32::from_bits)),
                false,
                state.firework_twinkle,
            ) {
                child.age = child.lifetime / 2;
                if let Kind::Magic(child_state) = child.kind {
                    let frame_index = super::descriptor_age_frame(
                        child.age,
                        child.lifetime,
                        child_state.frame_count,
                    );
                    if let Some(frame) = super::descriptor_frame(atlas, descriptor(k), frame_index)
                    {
                        child.set_sprite(&frame);
                    }
                }
                children.push(child);
            }
        }
    }
    true
}

pub(super) fn appearance(p: &Particle, partial_tick: f32) -> Option<Appearance> {
    let Kind::Magic(state) = p.kind else {
        return None;
    };
    let k = state.kind;
    if k == ServerParticleKind::Firework
        && state.firework_twinkle
        && p.age >= p.lifetime / 3
        && (p.age + p.lifetime) / 3 % 2 != 0
    {
        return None;
    }
    let t = (p.age as f32 + partial_tick) / p.lifetime.max(1) as f32;
    let size = match k {
        ServerParticleKind::Portal => p.base_size * (1.0 - (1.0 - t).powi(2)),
        ServerParticleKind::ReversePortal => p.base_size * (1.0 - t / 1.5),
        ServerParticleKind::Enchant | ServerParticleKind::VaultConnection => p.base_size,
        ServerParticleKind::Crit
        | ServerParticleKind::EnchantedHit
        | ServerParticleKind::DamageIndicator
        | ServerParticleKind::Note => {
            p.base_size
                * ((p.age as f32 + partial_tick) / p.lifetime.max(1) as f32 * 32.0).clamp(0.0, 1.0)
        }
        ServerParticleKind::SweepAttack => p.base_size,
        ServerParticleKind::Gust
        | ServerParticleKind::SmallGust
        | ServerParticleKind::SonicBoom => p.base_size,
        _ => p.size,
    };
    let rotation = if k == ServerParticleKind::SculkCharge {
        Quat::from_rotation_z(state.roll)
    } else {
        p.rotation
    };
    let alpha = if k == ServerParticleKind::VaultConnection {
        0.6 * ((t - 0.25) / 0.75).clamp(0.0, 1.0)
    } else if k == ServerParticleKind::Flash {
        (0.6 - (p.age as f32 + partial_tick - 1.0) * 0.125).max(0.0)
    } else {
        p.alpha
    };
    let size = if k == ServerParticleKind::Flash {
        7.1 * ((p.age as f32 + partial_tick - 1.0) * 0.25 * std::f32::consts::PI).sin()
    } else {
        size
    };
    Some(Appearance {
        size,
        color: p.color,
        alpha,
        rotation,
        second_rotation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_twenty_eight_provider_kinds_are_owned() {
        let kinds = [
            ServerParticleKind::Crit,
            ServerParticleKind::EnchantedHit,
            ServerParticleKind::DamageIndicator,
            ServerParticleKind::Enchant,
            ServerParticleKind::Note,
            ServerParticleKind::Portal,
            ServerParticleKind::ReversePortal,
            ServerParticleKind::Glow,
            ServerParticleKind::WaxOn,
            ServerParticleKind::WaxOff,
            ServerParticleKind::ElectricSpark,
            ServerParticleKind::Scrape,
            ServerParticleKind::EggCrack,
            ServerParticleKind::DustPlume,
            ServerParticleKind::TrialSpawnerDetection,
            ServerParticleKind::TrialSpawnerDetectionOminous,
            ServerParticleKind::VaultConnection,
            ServerParticleKind::OminousSpawning,
            ServerParticleKind::PauseMobGrowth,
            ServerParticleKind::ResetMobGrowth,
            ServerParticleKind::Firework,
            ServerParticleKind::Flash,
            ServerParticleKind::SculkCharge,
            ServerParticleKind::SculkChargePop,
            ServerParticleKind::SonicBoom,
            ServerParticleKind::SweepAttack,
            ServerParticleKind::Gust,
            ServerParticleKind::SmallGust,
        ];
        assert_eq!(kinds.len(), 28);
        assert!(kinds.into_iter().all(supports));
        assert_eq!(
            kinds
                .map(descriptor)
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            28
        );
    }

    #[test]
    fn note_velocity_x_is_the_vanilla_color_parameter() {
        let phase = 0.25_f32;
        let color = [0.0, 1.0 / 3.0, 2.0 / 3.0].map(|o| {
            ((phase + o) * std::f32::consts::TAU)
                .sin()
                .mul_add(0.65, 0.35)
                .max(0.0)
        });
        assert!((color[0] - 1.0).abs() < 1e-6);
        assert!(color[1] < color[0] && color[2] < color[0]);
    }

    #[test]
    fn java_provider_tick_formulas_are_preserved() {
        assert_eq!(portal_progress(0.0), 1.0);
        assert_eq!(portal_progress(0.5), 1.0);
        assert_eq!(portal_progress(1.0), 0.0);
        assert_eq!(reverse_portal_progress(1, 10), 0.1);
        assert_eq!(reverse_portal_progress(10, 10), 5.5);
        assert_eq!(
            enchant_position(DVec3::ZERO, DVec3::new(2.0, 4.0, 0.0), 0.0),
            DVec3::new(2.0, 4.0, 0.0)
        );
        assert_eq!(
            enchant_position(DVec3::ZERO, DVec3::new(2.0, 4.0, 0.0), 1.0),
            DVec3::new(0.0, -1.2, 0.0)
        );
        let mut crit = [0.8, 0.8, 0.8];
        crit_color_tick(&mut crit);
        assert!((crit[0] - 0.8).abs() < 1e-6);
        assert!((crit[1] - 0.768).abs() < 1e-6);
        assert!((crit[2] - 0.72).abs() < 1e-6);
        assert_eq!(firework_alpha(24, 48), 0.99);
        assert!((firework_alpha(36, 48) - 0.75).abs() < 1e-6);
        let mut spark = [0.0, 0.5, 1.0];
        firework_fade_tick(&mut spark, [1.0, 0.5, 0.0]);
        assert_eq!(spark, [0.2, 0.5, 0.8]);
    }

    #[test]
    fn java_render_layers_match_provider_groups() {
        for kind in [
            ServerParticleKind::Firework,
            ServerParticleKind::Flash,
            ServerParticleKind::SculkCharge,
            ServerParticleKind::SculkChargePop,
        ] {
            assert!(translucent(kind), "{kind:?} is translucent in Java");
        }
        for kind in [
            ServerParticleKind::Crit,
            ServerParticleKind::EnchantedHit,
            ServerParticleKind::DamageIndicator,
            ServerParticleKind::Enchant,
            ServerParticleKind::Note,
            ServerParticleKind::Portal,
            ServerParticleKind::ReversePortal,
            ServerParticleKind::Glow,
            ServerParticleKind::WaxOn,
            ServerParticleKind::WaxOff,
            ServerParticleKind::ElectricSpark,
            ServerParticleKind::Scrape,
            ServerParticleKind::EggCrack,
            ServerParticleKind::DustPlume,
        ] {
            assert!(!translucent(kind), "{kind:?} is opaque in Java");
        }
    }

    #[test]
    fn animated_sprite_index_uses_ordered_descriptor_frames() {
        assert_eq!(super::super::descriptor_age_frame(0, 8, 4), 0);
        assert_eq!(super::super::descriptor_age_frame(4, 8, 4), 2);
        assert_eq!(super::super::descriptor_age_frame(7, 8, 4), 3);
    }

    #[test]
    fn sculk_charge_roll_is_the_provider_rotation() {
        let roll = std::f32::consts::FRAC_PI_2;
        let q = Quat::from_rotation_z(roll);
        let rotated = q * glam::Vec3::X;
        assert!(rotated.distance(glam::Vec3::Y) < 1e-6);
    }
}
