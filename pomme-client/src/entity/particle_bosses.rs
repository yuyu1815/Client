use std::collections::VecDeque;

use glam::{DVec3, dvec3};

use super::EntityStore;
use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
use crate::world::chunk::ChunkStore;
use crate::world::particle_tick::ParticleSpawnRequest;

const DRAGON_LANDING: i32 = 3;
const DRAGON_SITTING_FLAMING: i32 = 5;
const DRAGON_DYING: i32 = 9;

#[derive(Debug)]
pub(super) struct BossParticleState {
    pub(super) dragon_phase: i32,
    pub(super) dragon_phase_ticks: u32,
    pub(super) dragon_death_ticks: u32,
    /// `EnderDragon.dragonDeathTime`, separate from its synchronized dragon
    /// phase.
    pub(super) dragon_death_time: u32,
    dragon_history: VecDeque<(f64, f32)>,

    pub(super) wither_targets: [i32; 3],
    wither_head_yaw: [f32; 2],
    wither_head_pitch: [f32; 2],
    last_tick: Option<i64>,
}

impl Default for BossParticleState {
    fn default() -> Self {
        Self {
            dragon_phase: 10,
            dragon_phase_ticks: 0,
            dragon_death_ticks: 0,
            dragon_death_time: 0,
            dragon_history: VecDeque::new(),

            wither_targets: [0; 3],
            wither_head_yaw: [0.0; 2],
            wither_head_pitch: [0.0; 2],
            last_tick: None,
        }
    }
}

fn request(kind: Kind, options: Options, position: DVec3, velocity: DVec3) -> ParticleSpawnRequest {
    ParticleSpawnRequest {
        kind,
        options,
        position,
        velocity,
        always_visible: false,
    }
}

fn gaussian(rng: &mut fastrand::Rng) -> f64 {
    // Sum of 12 uniform variates approximates Java Random.nextGaussian at particle
    // scale.
    (0..12).map(|_| rng.f64()).sum::<f64>() - 6.0
}

fn dragon_head(state: &BossParticleState, entity: &super::LivingEntity) -> DVec3 {
    let sample = |delay: usize| {
        state
            .dragon_history
            .get(state.dragon_history.len().saturating_sub(1 + delay))
            .copied()
            .unwrap_or((entity.position.y, entity.look_dir.y_rot_deg()))
    };
    let (y5, _) = sample(5);
    let (y10, _) = sample(10);
    let (y0, _) = sample(0);
    let tilt = (y5 - y10) * 10.0_f64.to_radians();
    let (sin_tilt, cos_tilt) = tilt.sin_cos();
    let (_, look_yaw) = sample(0); // yRotA is server-flight state and is not synchronized.
    let (s2, c2) = (look_yaw.to_radians().sin(), look_yaw.to_radians().cos());
    let y_offset = if matches!(state.dragon_phase, 5 | 6 | 7) {
        -1.0
    } else {
        y5 - y0
    };
    dvec3(
        entity.position.x + s2 as f64 * 6.5 * cos_tilt,
        entity.position.y + y_offset + sin_tilt * 6.5,
        entity.position.z - c2 as f64 * 6.5 * cos_tilt,
    )
}

fn dragon_breath(
    out: &mut Vec<ParticleSpawnRequest>,
    state: &BossParticleState,
    entity: &super::LivingEntity,
    rng: &mut fastrand::Rng,
    sitting: bool,
) {
    let head = dragon_head(state, entity) + dvec3(0.0, 0.5, 0.0);
    let mut yaw = entity.look_dir.y_rot_deg().to_radians();
    let pitch = if sitting {
        -45.0_f32.to_radians()
    } else {
        entity.look_dir.x_rot_deg().to_radians()
    };
    let cp = pitch.cos() as f64;
    let sp = pitch.sin() as f64;
    for _ in 0..8 {
        let mut look = dvec3(-yaw.sin() as f64 * cp, -sp, yaw.cos() as f64 * cp);
        let (sin, cos) = (-std::f32::consts::FRAC_PI_4).sin_cos();
        look = dvec3(
            look.x * cos as f64 + look.z * sin as f64,
            look.y,
            look.z * cos as f64 - look.x * sin as f64,
        );
        let pos = head
            + dvec3(
                gaussian(rng) * 0.5,
                gaussian(rng) * 0.5,
                gaussian(rng) * 0.5,
            );
        let movement = entity.velocity;
        let mut emit = |velocity| {
            out.push(request(
                Kind::DragonBreath,
                Options::Power { power: 1.0 },
                pos,
                velocity,
            ))
        };
        if sitting {
            for j in 0..6 {
                emit(dvec3(
                    -look.x * 0.08 * j as f64,
                    -look.y * 0.6,
                    -look.z * 0.08 * j as f64,
                ));
            }
        } else {
            emit(dvec3(
                -look.x * 0.08 + movement.x,
                -look.y * 0.3 + movement.y,
                -look.z * 0.08 + movement.z,
            ));
        }
        yaw += 0.19634955;
    }
}

fn wrap_degrees(degrees: f32) -> f32 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

fn approach_angle(current: f32, target: f32, max_delta: f32) -> f32 {
    current + wrap_degrees(target - current).clamp(-max_delta, max_delta)
}

fn tick_wither(
    out: &mut Vec<ParticleSpawnRequest>,
    store: &EntityStore,
    id: i32,
    entity: &super::LivingEntity,
    state: &mut BossParticleState,
    rng: &mut fastrand::Rng,
) {
    let body_yaw = entity.body_y_rot_deg;
    for head in 0..3 {
        let (hx, hy, hz) = if head == 0 {
            (
                entity.position.x,
                entity.position.y + 3.0,
                entity.position.z,
            )
        } else {
            let angle = (body_yaw + (180 * (head - 1)) as f32).to_radians();
            (
                entity.position.x + angle.cos() as f64 * 1.3,
                entity.position.y + 2.2,
                entity.position.z + angle.sin() as f64 * 1.3,
            )
        };
        if head > 0 {
            let slot = head - 1;
            let target = store.living.get(&state.wither_targets[head]);
            if let Some(target) = target {
                let dx = target.position.x - hx;
                let dy = target.position.y + 1.62 - hy;
                let dz = target.position.z - hz;
                let horizontal = dx.hypot(dz);
                let yaw = dz.atan2(dx).to_degrees() as f32 - 90.0;
                let pitch = -(dy.atan2(horizontal).to_degrees() as f32);
                state.wither_head_pitch[slot] =
                    approach_angle(state.wither_head_pitch[slot], pitch, 40.0);
                state.wither_head_yaw[slot] =
                    approach_angle(state.wither_head_yaw[slot], yaw, 10.0);
            } else {
                state.wither_head_yaw[slot] =
                    approach_angle(state.wither_head_yaw[slot], body_yaw, 10.0);
            }
        }
        let radius = 0.3;
        let pos = dvec3(hx, hy, hz)
            + dvec3(
                gaussian(rng) * radius,
                gaussian(rng) * radius,
                gaussian(rng) * radius,
            );
        out.push(request(Kind::Smoke, Options::Simple, pos, DVec3::ZERO));
        if entity.health <= entity.max_health * 0.5 && rng.u32(0..4) == 0 {
            let pos = dvec3(hx, hy, hz)
                + dvec3(
                    gaussian(rng) * radius,
                    gaussian(rng) * radius,
                    gaussian(rng) * radius,
                );
            out.push(request(
                Kind::EntityEffect,
                Options::EntityEffect {
                    color: 0xff_b2_b2_7f,
                },
                pos,
                DVec3::ZERO,
            ));
        }
    }
    if entity.wither_invulnerability > 0 {
        for _ in 0..3 {
            let pos = dvec3(
                entity.position.x + gaussian(rng),
                entity.position.y + rng.f64() * 3.3,
                entity.position.z + gaussian(rng),
            );
            out.push(request(
                Kind::EntityEffect,
                Options::EntityEffect {
                    color: 0xff_b2_b2_e5,
                },
                pos,
                DVec3::ZERO,
            ));
        }
    }
    let _ = id;
}

pub(crate) fn tick_boss_particles(
    store: &mut EntityStore,
    _chunks: &ChunkStore,
    game_time: i64,
) -> Vec<ParticleSpawnRequest> {
    let mut out = Vec::new();
    let ids: Vec<i32> = store
        .living
        .iter()
        .filter_map(|(&id, e)| {
            matches!(
                e.entity_type,
                azalea_registry::builtin::EntityKind::EnderDragon
                    | azalea_registry::builtin::EntityKind::Wither
            )
            .then_some(id)
        })
        .collect();
    for id in ids {
        let Some(entity) = store.living.get(&id) else {
            continue;
        };
        let mut state = store.boss_particle_state.remove(&id).unwrap_or_default();
        if state.last_tick == Some(game_time) {
            store.boss_particle_state.insert(id, state);
            continue;
        }
        state.last_tick = Some(game_time);
        let seed = (id as u64).wrapping_mul(0x9e37_79b9) ^ game_time as u64;
        let mut rng = fastrand::Rng::with_seed(seed);
        match entity.entity_type {
            azalea_registry::builtin::EntityKind::EnderDragon => {
                let phase = state.dragon_phase;
                let position = entity.position;
                let yaw = entity.look_dir.y_rot_deg();
                if state.dragon_history.is_empty() {
                    state.dragon_history.resize(64, (position.y, yaw));
                } else {
                    state.dragon_history.push_back((position.y, yaw));
                    state.dragon_history.pop_front();
                }
                state.dragon_phase_ticks = state.dragon_phase_ticks.saturating_add(1);
                if phase == DRAGON_DYING {
                    state.dragon_death_ticks = state.dragon_death_ticks.saturating_add(1);
                    if state.dragon_death_ticks % 10 == 1 {
                        let p = dvec3(
                            position.x + (rng.f64() - 0.5) * 8.0,
                            position.y + 2.0 + (rng.f64() - 0.5) * 4.0,
                            position.z + (rng.f64() - 0.5) * 8.0,
                        );
                        out.push(request(
                            Kind::ExplosionEmitter,
                            Options::Simple,
                            p,
                            DVec3::ZERO,
                        ));
                    }
                } else {
                    state.dragon_death_ticks = 0;
                }
                if entity.health <= 0.0 {
                    state.dragon_death_time = state.dragon_death_time.saturating_add(1);
                    let p = dvec3(
                        position.x + (rng.f64() - 0.5) * 8.0,
                        position.y + 2.0 + (rng.f64() - 0.5) * 4.0,
                        position.z + (rng.f64() - 0.5) * 8.0,
                    );
                    out.push(request(Kind::Explosion, Options::Simple, p, DVec3::ZERO));
                    if (180..=200).contains(&state.dragon_death_time) {
                        let p = dvec3(
                            position.x + (rng.f32() as f64 - 0.5) * 8.0,
                            position.y + 2.0 + (rng.f32() as f64 - 0.5) * 4.0,
                            position.z + (rng.f32() as f64 - 0.5) * 8.0,
                        );
                        out.push(request(
                            Kind::ExplosionEmitter,
                            Options::Simple,
                            p,
                            DVec3::ZERO,
                        ));
                    }
                } else {
                    state.dragon_death_time = 0;
                }
                if phase == DRAGON_LANDING
                    || (phase == DRAGON_SITTING_FLAMING
                        && state.dragon_phase_ticks % 2 == 0
                        && state.dragon_phase_ticks < 10)
                {
                    dragon_breath(
                        &mut out,
                        &state,
                        entity,
                        &mut rng,
                        phase == DRAGON_SITTING_FLAMING,
                    );
                }
            }
            azalea_registry::builtin::EntityKind::Wither => {
                tick_wither(&mut out, store, id, entity, &mut state, &mut rng)
            }
            _ => {}
        }
        store.boss_particle_state.insert(id, state);
    }
    out
}

#[cfg(test)]
mod tests {
    use azalea_registry::builtin::EntityKind;

    use super::*;
    use crate::entity::components::{LookDirection, Position};

    fn spawn(store: &mut EntityStore, id: i32, kind: EntityKind, p: Position) {
        store.spawn_living(id, kind, p, LookDirection::new(0.0, 0.0), 0.0, None);
    }

    #[test]
    fn dragon_head_uses_synced_yaw_and_client_history_instead_of_entity_center() {
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::EnderDragon,
            Position::new(10.0, 64.0, -3.0),
        );
        let entity = store.living.get_mut(&1).unwrap();
        entity.look_dir = LookDirection::new(90.0, 0.0);
        let mut state = BossParticleState::default();
        state.dragon_history.resize(64, (64.0, 90.0));
        state.dragon_history[64 - 6] = (65.0, 90.0);
        state.dragon_history[64 - 11] = (64.0, 90.0);
        let head = dragon_head(&state, entity);
        assert!(head.x > 16.0);
        assert!((head.z + 3.0).abs() < 0.01);
        assert!(head.y > 65.0);
    }

    #[test]
    fn dragon_phase_reset_death_clock_and_breath_fan_are_local_client_particles() {
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::EnderDragon,
            Position::new(0.0, 64.0, 0.0),
        );
        store.apply_entity_data(1, 16, super::super::MetaValue::Int(DRAGON_SITTING_FLAMING));
        let first = tick_boss_particles(&mut store, &ChunkStore::new(8), 1);
        assert!(first.is_empty());
        let second = tick_boss_particles(&mut store, &ChunkStore::new(8), 2);
        assert_eq!(second.len(), 48);
        assert!(second.iter().all(|p| p.kind == Kind::DragonBreath
            && matches!(&p.options, Options::Power { power } if *power == 1.0)));
        store.apply_entity_data(1, 16, super::super::MetaValue::Int(DRAGON_DYING));
        assert_eq!(store.boss_particle_state[&1].dragon_phase_ticks, 0);
        let death = tick_boss_particles(&mut store, &ChunkStore::new(8), 3);
        assert_eq!(death.len(), 1);
        assert_eq!(death[0].kind, Kind::ExplosionEmitter);
        store.apply_entity_data(1, 16, super::super::MetaValue::Int(DRAGON_LANDING));
        assert_eq!(store.boss_particle_state[&1].dragon_death_ticks, 0);
        assert_eq!(store.boss_particle_state[&1].dragon_phase_ticks, 0);
    }

    fn aggregate_tick(store: &mut EntityStore, game_time: i64) -> Vec<ParticleSpawnRequest> {
        store.set_client_particle_game_time(game_time);
        store.client_particle_requests(&ChunkStore::new(8), &std::collections::HashSet::new(), None)
    }

    #[test]
    fn dragon_late_death_burst_uses_its_own_clock_and_exact_window() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::EnderDragon,
            Position::new(0.0, 64.0, 0.0),
        );
        store.apply_entity_data(1, 9, super::super::MetaValue::Float(0.0));
        store
            .boss_particle_state
            .entry(1)
            .or_default()
            .dragon_death_time = 178;

        let at_179 = aggregate_tick(&mut store, 1);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 179);
        assert_eq!(
            at_179.iter().filter(|p| p.kind == Kind::Explosion).count(),
            1
        );
        assert_eq!(
            at_179
                .iter()
                .filter(|p| p.kind == Kind::ExplosionEmitter)
                .count(),
            0
        );

        let at_180 = aggregate_tick(&mut store, 2);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 180);
        assert_eq!(
            at_180.iter().filter(|p| p.kind == Kind::Explosion).count(),
            1
        );
        let [_, emitter] = at_180.as_slice() else {
            panic!("health-zero Explosion and late-death emitter are separate requests")
        };
        assert_eq!(emitter.kind, Kind::ExplosionEmitter);
        assert_eq!(emitter.velocity, DVec3::ZERO);
        assert!((-4.0..4.0).contains(&emitter.position.x));
        assert!((64.0..68.0).contains(&emitter.position.y));
        assert!((-4.0..4.0).contains(&emitter.position.z));

        store
            .boss_particle_state
            .get_mut(&1)
            .unwrap()
            .dragon_death_time = 198;
        let at_199 = aggregate_tick(&mut store, 3);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 199);
        assert_eq!(
            at_199
                .iter()
                .filter(|p| p.kind == Kind::ExplosionEmitter)
                .count(),
            1
        );
        let at_200 = aggregate_tick(&mut store, 4);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 200);
        assert_eq!(
            at_200
                .iter()
                .filter(|p| p.kind == Kind::ExplosionEmitter)
                .count(),
            1
        );
        let at_201 = aggregate_tick(&mut store, 5);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 201);
        assert_eq!(
            at_201
                .iter()
                .filter(|p| p.kind == Kind::ExplosionEmitter)
                .count(),
            0
        );
        assert_eq!(
            at_201.iter().filter(|p| p.kind == Kind::Explosion).count(),
            1
        );
    }

    #[test]
    fn dragon_phase_burst_and_late_death_burst_remain_independent() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::EnderDragon,
            Position::new(0.0, 64.0, 0.0),
        );
        store
            .boss_particle_state
            .entry(1)
            .or_default()
            .dragon_death_time = 179;
        store.apply_entity_data(1, 16, super::super::MetaValue::Int(DRAGON_DYING));
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 179);
        store.apply_entity_data(1, 9, super::super::MetaValue::Float(0.0));

        let both = aggregate_tick(&mut store, 1);
        assert_eq!(both.iter().filter(|p| p.kind == Kind::Explosion).count(), 1);
        assert_eq!(
            both.iter()
                .filter(|p| p.kind == Kind::ExplosionEmitter)
                .count(),
            2
        );
        assert_eq!(store.boss_particle_state[&1].dragon_phase_ticks, 1);
        assert_eq!(store.boss_particle_state[&1].dragon_death_ticks, 1);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 180);

        // Re-reading the same aggregate tick cannot advance either clock or duplicate
        // bursts.
        let duplicate = aggregate_tick(&mut store, 1);
        assert!(duplicate.is_empty());
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 180);

        let mut phase_only = EntityStore::new();
        spawn(
            &mut phase_only,
            1,
            EntityKind::EnderDragon,
            Position::new(0.0, 64.0, 0.0),
        );
        phase_only.apply_entity_data(1, 16, super::super::MetaValue::Int(DRAGON_DYING));
        for tick in 1..=10 {
            let requests = aggregate_tick(&mut phase_only, tick);
            assert_eq!(
                requests
                    .iter()
                    .filter(|p| p.kind == Kind::ExplosionEmitter)
                    .count(),
                usize::from(tick == 1),
                "the living DragonDeathPhase cadence is still separate"
            );
        }
    }

    #[test]
    fn dragon_death_clock_resets_on_heal_and_entity_lifecycle() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::EnderDragon,
            Position::new(0.0, 64.0, 0.0),
        );
        store.apply_entity_data(1, 9, super::super::MetaValue::Float(0.0));
        aggregate_tick(&mut store, 1);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 1);
        store.apply_entity_data(1, 9, super::super::MetaValue::Float(10.0));
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 0);
        aggregate_tick(&mut store, 2);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 0);

        store.remove_entity(1);
        assert!(!store.boss_particle_state.contains_key(&1));
        spawn(
            &mut store,
            1,
            EntityKind::EnderDragon,
            Position::new(0.0, 64.0, 0.0),
        );
        assert!(!store.boss_particle_state.contains_key(&1));
        store.apply_entity_data(1, 9, super::super::MetaValue::Float(0.0));
        aggregate_tick(&mut store, 3);
        assert_eq!(store.boss_particle_state[&1].dragon_death_time, 1);
    }

    #[test]
    fn wither_heads_are_spatially_distinct_and_targets_are_resolved() {
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::Wither,
            Position::new(4.0, 10.0, 8.0),
        );
        spawn(
            &mut store,
            2,
            EntityKind::Player,
            Position::new(20.0, 12.0, 8.0),
        );
        store.apply_entity_data(1, 17, super::super::MetaValue::Int(2));
        store.apply_entity_data(1, 19, super::super::MetaValue::Int(40));
        let particles = tick_boss_particles(&mut store, &ChunkStore::new(8), 5);
        assert_eq!(particles.len(), 6);
        assert_eq!(
            particles.iter().filter(|p| p.kind == Kind::Smoke).count(),
            3
        );
        assert_eq!(
            particles
                .iter()
                .filter(|p| matches!(&p.options, Options::EntityEffect { .. }))
                .count(),
            3
        );
        assert_ne!(particles[0].position, particles[1].position);
        assert!(store.boss_particle_state[&1].wither_head_yaw[0].abs() > 0.0);
        let old_yaw = store.boss_particle_state[&1].wither_head_yaw[0];
        store.remove_entity(2);
        tick_boss_particles(&mut store, &ChunkStore::new(8), 6);
        assert_eq!(store.boss_particle_state[&1].wither_targets[1], 2);
        assert_ne!(store.boss_particle_state[&1].wither_head_yaw[0], old_yaw);
    }
}
