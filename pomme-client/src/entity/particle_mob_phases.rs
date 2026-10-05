//! Client-local phase and metadata-edge particle sources for synchronized mobs.
use azalea_registry::builtin::EntityKind;
use glam::{DVec3, dvec3};

use super::{EntityPose, EntityStore, LivingEntity};
use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
use crate::world::block;
use crate::world::chunk::ChunkStore;
use crate::world::particle_tick::ParticleSpawnRequest;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct State {
    pose: EntityPose,
    pose_initialized: bool,
    warden_start: Option<i64>,
    warden_complete: bool,
    breeze_jump_ticks: u8,
    cube_size: u8,
    cube_size_initialized: bool,
    cube_splash_pending: bool,
    was_on_ground: bool,
    ground_initialized: bool,
    ravager_stun_ticks: u8,
    ravager_attack_ticks: u8,
    illusion_ticks: u8,
    illusion_offsets: [[DVec3; 4]; 2],
}

fn req(kind: Kind, options: Options, position: DVec3, velocity: DVec3) -> ParticleSpawnRequest {
    ParticleSpawnRequest {
        kind,
        options,
        position,
        velocity,
        always_visible: false,
    }
}

fn block_req(state: azalea_block::BlockState, position: DVec3) -> ParticleSpawnRequest {
    req(Kind::Block, Options::Block(state), position, DVec3::ZERO)
}

fn support(chunks: &ChunkStore, e: &LivingEntity) -> Option<azalea_block::BlockState> {
    let (x, y, z) = (
        e.position.x.floor() as i32,
        e.position.y.floor() as i32,
        e.position.z.floor() as i32,
    );
    let in_block = chunks.get_block_state(x, y, z);
    if !block::is_air(in_block) {
        Some(in_block)
    } else {
        let below = chunks.get_block_state(x, y - 1, z);
        (!block::is_air(below)).then_some(below)
    }
}

impl EntityStore {
    /// Apply on synchronized DATA_POSE; repeated same-pose metadata is inert.
    pub(crate) fn on_particle_mob_pose(&mut self, id: i32, pose: EntityPose) {
        let Some(e) = self.living.get(&id) else {
            return;
        };
        let kind = e.entity_type;
        if !matches!(kind, EntityKind::Warden | EntityKind::Breeze) {
            return;
        }
        let s = self.phase_particle_state.entry(id).or_default();
        if s.pose_initialized && pose == s.pose {
            return;
        }
        if kind == EntityKind::Warden {
            s.warden_start =
                matches!(pose, EntityPose::Digging | EntityPose::Emerging).then_some(i64::MIN);
            s.warden_complete = false;
        }
        if kind == EntityKind::Breeze {
            s.breeze_jump_ticks = 0;
        }
        s.pose = pose;
        s.pose_initialized = true;
    }

    /// Hook from entity events. Ravager 39 starts the 40-tick stun; 69 returns
    /// the one-shot roar burst. Attack event 4 deliberately emits no particle.
    pub(crate) fn handle_particle_mob_event(
        &mut self,
        id: i32,
        event_id: u8,
    ) -> Vec<ParticleSpawnRequest> {
        let Some(e) = self.living.get(&id) else {
            return Vec::new();
        };
        if e.entity_type != EntityKind::Ravager {
            return Vec::new();
        }
        if event_id == 4 {
            self.phase_particle_state
                .entry(id)
                .or_default()
                .ravager_attack_ticks = 10;
        } else if event_id == 39 {
            self.phase_particle_state
                .entry(id)
                .or_default()
                .ravager_stun_ticks = 40;
        } else if event_id == 69 {
            let height =
                f64::from(azalea_entity::dimensions::EntityDimensions::from(e.entity_type).height);
            let center = DVec3::from(e.position) + dvec3(0.0, height * 0.5, 0.0);
            let mut rng = fastrand::Rng::with_seed(
                (id as u64).wrapping_mul(0x9e37_79b9) ^ u64::from(e.age_in_ticks),
            );
            return (0..40)
                .map(|_| {
                    req(
                        Kind::Poof,
                        Options::Simple,
                        center,
                        dvec3(
                            gaussian(&mut rng) * 0.2,
                            gaussian(&mut rng) * 0.2,
                            gaussian(&mut rng) * 0.2,
                        ),
                    )
                })
                .collect();
        }
        Vec::new()
    }

    /// Invoke after normalized cube size metadata. The first value is baseline.
    pub(crate) fn on_particle_cube_size(&mut self, id: i32, size: u8) {
        let Some(e) = self.living.get(&id) else {
            return;
        };
        if !matches!(
            e.entity_type,
            EntityKind::Slime | EntityKind::MagmaCube | EntityKind::SulfurCube
        ) {
            return;
        }
        let (in_water, age) = (e.is_in_water, e.age_in_ticks);
        let s = self.phase_particle_state.entry(id).or_default();
        let changed = s.cube_size_initialized && s.cube_size != size;
        s.cube_size = size;
        s.cube_size_initialized = true;
        if changed && in_water {
            let mut rng = fastrand::Rng::with_seed(
                (id as u64).wrapping_mul(0xd6e8_feb8_6659_fd93) ^ u64::from(size) ^ u64::from(age),
            );
            if rng.u32(..20) == 0 {
                s.cube_splash_pending = true;
            }
        }
    }

    /// Aggregate entry: call once per client tick with the owned
    /// store/chunks/time.
    pub(super) fn tick_mob_phase_particles(
        store: &mut EntityStore,
        chunks: &ChunkStore,
        game_time: i64,
    ) -> Vec<ParticleSpawnRequest> {
        let mut out = Vec::new();
        for (&id, e) in &store.living {
            if !matches!(
                e.entity_type,
                EntityKind::Warden
                    | EntityKind::Breeze
                    | EntityKind::Illusioner
                    | EntityKind::Ravager
                    | EntityKind::Slime
                    | EntityKind::MagmaCube
                    | EntityKind::SulfurCube
            ) {
                continue;
            }
            let s = store.phase_particle_state.entry(id).or_default();
            match e.entity_type {
                EntityKind::Warden => warden(&mut out, id, e, s, chunks, game_time),
                EntityKind::Breeze => {
                    breeze(&mut out, e, s, chunks, store.vehicle_of.contains_key(&id))
                }
                EntityKind::Illusioner => illusioner(&mut out, id, e, s, game_time),
                EntityKind::Ravager => ravager(&mut out, id, e, s, game_time),
                EntityKind::Slime | EntityKind::MagmaCube | EntityKind::SulfurCube => {
                    cube(&mut out, id, e, s, game_time)
                }
                _ => {}
            }
        }
        out
    }
}

fn gaussian(rng: &mut fastrand::Rng) -> f64 {
    let a = rng.f64().max(f64::MIN_POSITIVE);
    let b = rng.f64();
    (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
}

fn warden(
    out: &mut Vec<ParticleSpawnRequest>,
    id: i32,
    e: &LivingEntity,
    s: &mut State,
    chunks: &ChunkStore,
    now: i64,
) {
    if !matches!(e.pose, EntityPose::Digging | EntityPose::Emerging) {
        s.warden_start = None;
        s.warden_complete = false;
        return;
    }
    if s.warden_complete {
        return;
    }
    if s.warden_start == Some(i64::MIN) {
        s.warden_start = Some(now);
    }
    let start = *s.warden_start.get_or_insert(now);
    if now.saturating_sub(start) >= 90 {
        s.warden_complete = true;
        return;
    }
    let Some(state) = support(chunks, e) else {
        return;
    };
    if crate::world::block_entity::is_invisible_block(block::block_id(state)) {
        return;
    }
    let mut rng = fastrand::Rng::with_seed((id as u64).wrapping_mul(0x9e37_79b9) ^ now as u64);
    for _ in 0..30 {
        out.push(block_req(
            state,
            dvec3(
                e.position.x + rng.f64() * 1.4 - 0.7,
                e.position.y,
                e.position.z + rng.f64() * 1.4 - 0.7,
            ),
        ));
    }
}

fn breeze(
    out: &mut Vec<ParticleSpawnRequest>,
    e: &LivingEntity,
    s: &mut State,
    chunks: &ChunkStore,
    passenger: bool,
) {
    let ground = support(chunks, e)
        .filter(|state| !crate::world::block_entity::is_invisible_block(block::block_id(*state)));
    match e.pose {
        EntityPose::LongJumping if s.breeze_jump_ticks < 5 => {
            s.breeze_jump_ticks += 1;
            if let Some(ground) = ground {
                let p = DVec3::from(e.position) + e.velocity + dvec3(0.0, 0.1, 0.0);
                for _ in 0..3 {
                    out.push(block_req(ground, p));
                }
            }
        }
        EntityPose::LongJumping => {}
        EntityPose::Sliding => {
            s.breeze_jump_ticks = 0;
            if !passenger && let Some(ground) = ground {
                for _ in 0..20 {
                    out.push(block_req(ground, DVec3::from(e.position)));
                }
            }
        }
        EntityPose::Shooting | EntityPose::Inhaling | EntityPose::Standing => {
            s.breeze_jump_ticks = 0;
            if !passenger && let Some(ground) = ground {
                out.push(block_req(ground, DVec3::from(e.position)));
            }
        }
        _ => s.breeze_jump_ticks = 0,
    }
}

fn illusioner(
    out: &mut Vec<ParticleSpawnRequest>,
    id: i32,
    e: &LivingEntity,
    s: &mut State,
    now: i64,
) {
    if !e.flags.invisible {
        s.illusion_ticks = 0;
        return;
    }
    s.illusion_ticks = s.illusion_ticks.saturating_sub(1);
    let client_tick = i64::from(e.age_in_ticks);
    if e.hurt_time == 1 || (client_tick != 0 && client_tick.rem_euclid(1200) == 0) {
        s.illusion_ticks = 3;
        s.illusion_offsets[0] = s.illusion_offsets[1];
        let mut rng = fastrand::Rng::with_seed((id as u64).wrapping_mul(0x1656_67b1) ^ now as u64);
        for offset in &mut s.illusion_offsets[1] {
            *offset = dvec3(
                rng.i32(-6..7) as f64 * 0.5,
                rng.i32(0..6).saturating_sub(4).max(0) as f64,
                rng.i32(-6..7) as f64 * 0.5,
            );
        }
        let dim = azalea_entity::dimensions::EntityDimensions::from(e.entity_type);
        for _ in 0..16 {
            out.push(req(
                Kind::Cloud,
                Options::Simple,
                dvec3(
                    e.position.x + (rng.f64() - 0.5) * f64::from(dim.width),
                    e.position.y + rng.f64() * f64::from(dim.height),
                    e.position.z + (rng.f64() - 0.5) * f64::from(dim.width),
                ),
                DVec3::ZERO,
            ));
        }
    } else if e.hurt_time == super::HURT_DURATION - 1 {
        s.illusion_ticks = 3;
        s.illusion_offsets = [[DVec3::ZERO; 4]; 2];
    }
}

fn ravager(
    out: &mut Vec<ParticleSpawnRequest>,
    id: i32,
    e: &LivingEntity,
    s: &mut State,
    now: i64,
) {
    s.ravager_attack_ticks = s.ravager_attack_ticks.saturating_sub(1);
    if s.ravager_stun_ticks == 0 {
        return;
    }
    s.ravager_stun_ticks -= 1;
    let mut rng = fastrand::Rng::with_seed((id as u64).wrapping_mul(0x517c_c1b7) ^ now as u64);
    if rng.u32(..6) == 0 {
        let dim = azalea_entity::dimensions::EntityDimensions::from(e.entity_type);
        let yaw = f64::from(e.body_y_rot_deg).to_radians();
        let width = f64::from(dim.width);
        let pos = dvec3(
            e.position.x - width * yaw.sin() + rng.f64() * 0.6 - 0.3,
            e.position.y + f64::from(dim.height) - 0.3,
            e.position.z + width * yaw.cos() + rng.f64() * 0.6 - 0.3,
        );
        out.push(req(
            Kind::EntityEffect,
            Options::EntityEffect { color: 0xff7f_838f },
            pos,
            DVec3::ZERO,
        ));
    }
}

fn cube(out: &mut Vec<ParticleSpawnRequest>, id: i32, e: &LivingEntity, s: &mut State, now: i64) {
    if s.cube_splash_pending {
        s.cube_splash_pending = false;
        water_splash(out, id, e, now);
    }
    if s.ground_initialized && e.on_ground && !s.was_on_ground {
        let width = f64::from(e.slime_size.max(1)) * 0.51;
        let count = (width * 32.0).ceil() as usize;
        let kind = match e.entity_type {
            EntityKind::Slime => Kind::ItemSlime,
            EntityKind::MagmaCube => Kind::Flame,
            EntityKind::SulfurCube => Kind::SulfurCubeGoo,
            _ => unreachable!(),
        };
        let mut rng = fastrand::Rng::with_seed((id as u64).wrapping_mul(0x94d0_49bb) ^ now as u64);
        for _ in 0..count {
            let angle = rng.f64() * std::f64::consts::TAU;
            let radius = rng.f64() * 0.5 + 0.5;
            out.push(req(
                kind,
                Options::Simple,
                dvec3(
                    e.position.x + angle.sin() * width * radius,
                    e.position.y,
                    e.position.z + angle.cos() * width * radius,
                ),
                DVec3::ZERO,
            ));
        }
    }
    s.was_on_ground = e.on_ground;
    s.ground_initialized = true;
}

fn water_splash(out: &mut Vec<ParticleSpawnRequest>, id: i32, e: &LivingEntity, now: i64) {
    let width = f64::from(e.slime_size.max(1)) * 0.51;
    let count = (1.0 + width * 20.0).ceil() as usize;
    let mut rng = fastrand::Rng::with_seed((id as u64).wrapping_mul(0xa076_1d64) ^ now as u64);
    for _ in 0..count {
        out.push(req(
            Kind::Bubble,
            Options::Simple,
            dvec3(
                e.position.x + (rng.f64() * 2.0 - 1.0) * width,
                e.position.y.floor() + 1.0,
                e.position.z + (rng.f64() * 2.0 - 1.0) * width,
            ),
            dvec3(e.velocity.x, e.velocity.y - rng.f64() * 0.2, e.velocity.z),
        ));
    }
    for _ in 0..count {
        out.push(req(
            Kind::Splash,
            Options::Simple,
            dvec3(
                e.position.x + (rng.f64() * 2.0 - 1.0) * width,
                e.position.y.floor() + 1.0,
                e.position.z + (rng.f64() * 2.0 - 1.0) * width,
            ),
            e.velocity,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::components::LookDirection;
    use crate::entity::{MetaValue, Position};

    fn spawn(store: &mut EntityStore, kind: EntityKind, id: i32) {
        store.spawn_living(
            id,
            kind,
            Position::new(0.5, 1.0, 0.5),
            LookDirection::default(),
            0.0,
            None,
        );
    }

    fn stone_world() -> ChunkStore {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        let mut chunk = azalea_world::chunk::Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), chunk);
        chunks.set_block_state(
            0,
            0,
            0,
            crate::world::block::default_state_of("stone").unwrap(),
        );
        chunks
    }

    #[test]
    fn warden_only_digging_and_emerging_phases_emit_for_90_ticks() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = stone_world();
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Warden, 7);
        store.set_pose(7, EntityPose::Roaring);
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 1).is_empty());
        store.set_pose(7, EntityPose::Digging);
        assert_eq!(
            EntityStore::tick_mob_phase_particles(&mut store, &chunks, 10).len(),
            30
        );
        assert_eq!(
            EntityStore::tick_mob_phase_particles(&mut store, &chunks, 99).len(),
            30
        );
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 100).is_empty());
        store.set_pose(7, EntityPose::Standing);
        store.set_pose(7, EntityPose::Emerging);
        assert_eq!(
            EntityStore::tick_mob_phase_particles(&mut store, &chunks, 101).len(),
            30
        );
        store.set_pose(7, EntityPose::Roaring);
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 102).is_empty());
        store.remove_entity(7);
        assert!(!store.phase_particle_state.contains_key(&7));
    }

    #[test]
    fn breeze_jump_trail_caps_at_five_samples_and_ground_pose_bursts() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = stone_world();
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Breeze, 1);
        store.set_living_motion(1, dvec3(0.25, 0.5, -0.25));
        store.set_pose(1, EntityPose::LongJumping);
        for t in 1..=5 {
            let batch = EntityStore::tick_mob_phase_particles(&mut store, &chunks, t);
            assert_eq!(batch.len(), 3);
            assert!(
                batch
                    .iter()
                    .all(|p| p.kind == Kind::Block && matches!(&p.options, &Options::Block(_)))
            );
        }
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 6).is_empty());
        store.set_pose(1, EntityPose::Sliding);
        assert_eq!(
            EntityStore::tick_mob_phase_particles(&mut store, &chunks, 7).len(),
            20
        );
        store.set_pose(1, EntityPose::Standing);
        assert_eq!(
            EntityStore::tick_mob_phase_particles(&mut store, &chunks, 8).len(),
            1
        );
    }

    #[test]
    fn illusioner_clouds_require_invisibility_and_actual_hurt_or_clock_edge() {
        let chunks = ChunkStore::new(1);
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Illusioner, 2);
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 1).is_empty());
        store.living.get_mut(&2).unwrap().flags.invisible = true;
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 1).is_empty());
        store.living.get_mut(&2).unwrap().hurt_time = 1;
        let clouds = EntityStore::tick_mob_phase_particles(&mut store, &chunks, 2);
        assert_eq!(clouds.len(), 16);
        assert_eq!(store.phase_particle_state[&2].illusion_ticks, 3);
        assert!(clouds.iter().all(|p| p.kind == Kind::Cloud));
        store.living.get_mut(&2).unwrap().hurt_time = 0;
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 3).is_empty());
        store.living.get_mut(&2).unwrap().flags.invisible = false;
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 1200).is_empty());
    }

    #[test]
    fn ravager_event_burst_is_one_shot_and_stun_window_expires() {
        let chunks = ChunkStore::new(1);
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Ravager, 3);
        assert!(store.handle_particle_mob_event(3, 4).is_empty());
        assert_eq!(store.phase_particle_state[&3].ravager_attack_ticks, 10);
        let roar = store.handle_particle_mob_event(3, 69);
        assert_eq!(roar.len(), 40);
        assert!(roar.iter().all(|p| p.kind == Kind::Poof));
        assert!(
            EntityStore::tick_mob_phase_particles(&mut store, &chunks, 0)
                .iter()
                .all(|p| p.kind != Kind::Poof)
        );
        assert!(store.handle_particle_mob_event(3, 39).is_empty());
        let mut stun = Vec::new();
        for now in 1..=40 {
            stun.extend(EntityStore::tick_mob_phase_particles(
                &mut store, &chunks, now,
            ));
        }
        assert!(stun.len() <= 40);
        assert!(stun.iter().all(|p| p.kind == Kind::EntityEffect
            && matches!(&p.options, &Options::EntityEffect { color: 0xff7f_838f })));
        assert_eq!(store.phase_particle_state[&3].ravager_stun_ticks, 0);
        assert_eq!(store.phase_particle_state[&3].ravager_attack_ticks, 0);
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 41).is_empty());
    }

    #[test]
    fn cube_size_metadata_baselines_edges_and_water_splash_is_one_shot() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Slime, 0);
        store.living.get_mut(&0).unwrap().is_in_water = true;
        store.apply_entity_data(0, 18, MetaValue::Int(2));
        store.apply_entity_data(0, 18, MetaValue::Int(2));
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 1).is_empty());
        // Search deterministic visual RNG seeds until the Java one-in-twenty edge
        // succeeds.
        let mut selected = None;
        for id in 0..200 {
            let mut candidate = EntityStore::new();
            spawn(&mut candidate, EntityKind::Slime, id);
            candidate.living.get_mut(&id).unwrap().is_in_water = true;
            candidate.apply_entity_data(id, 18, MetaValue::Int(1)); // baseline snapshot
            candidate.apply_entity_data(id, 18, MetaValue::Int(2));
            let particles = EntityStore::tick_mob_phase_particles(&mut candidate, &chunks, 5);
            if !particles.is_empty() {
                selected = Some(particles);
                break;
            }
        }
        let particles = selected.expect("at least one 1/20 seeded metadata edge");
        let width: f64 = 2.0 * 0.51;
        let count = (1.0 + width * 20.0).ceil() as usize;
        assert_eq!(
            particles.iter().filter(|p| p.kind == Kind::Bubble).count(),
            count
        );
        assert_eq!(
            particles.iter().filter(|p| p.kind == Kind::Splash).count(),
            count
        );
        assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 6).is_empty());
    }

    #[test]
    fn cube_landing_uses_each_subclass_particle_and_synchronized_size_count() {
        let chunks = ChunkStore::new(1);
        for (kind, expected) in [
            (EntityKind::Slime, Kind::ItemSlime),
            (EntityKind::MagmaCube, Kind::Flame),
            (EntityKind::SulfurCube, Kind::SulfurCubeGoo),
        ] {
            let mut store = EntityStore::new();
            spawn(&mut store, kind, 9);
            store.living.get_mut(&9).unwrap().slime_size = 2;
            store.living.get_mut(&9).unwrap().on_ground = true;
            assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 1).is_empty()); // first sample baselines ground
            store.living.get_mut(&9).unwrap().on_ground = false;
            assert!(EntityStore::tick_mob_phase_particles(&mut store, &chunks, 2).is_empty());
            store.living.get_mut(&9).unwrap().on_ground = true;
            let landing = EntityStore::tick_mob_phase_particles(&mut store, &chunks, 3);
            assert_eq!(landing.len(), (2.0_f64 * 0.51 * 32.0).ceil() as usize);
            assert!(landing.iter().all(|p| p.kind == expected));
        }
    }
}
