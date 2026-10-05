//! Client-local mob particle requests whose source state comes from synced
//! entity metadata. Discrete network events remain event-owned elsewhere.
use std::collections::HashMap;

use azalea_registry::builtin::EntityKind;
use glam::{DVec3, Vec3};

use super::{EntityStore, LivingEntity, MetaValue};
use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
use crate::world::chunk::ChunkStore;
use crate::world::particle_tick::ParticleSpawnRequest;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Spell {
    #[default]
    None,
    SummonVex,
    Fangs,
    Wololo,
    Disappear,
    Blindness,
}

impl Spell {
    fn from_id(id: u8) -> Self {
        match id {
            1 => Self::SummonVex,
            2 => Self::Fangs,
            3 => Self::Wololo,
            4 => Self::Disappear,
            5 => Self::Blindness,
            _ => Self::None,
        }
    }

    fn color(self) -> Option<u32> {
        let (r, g, b) = match self {
            Self::None => return None,
            Self::SummonVex => (0.7_f32, 0.7_f32, 0.8_f32),
            Self::Fangs => (0.4, 0.3, 0.35),
            Self::Wololo => (0.7, 0.5, 0.2),
            Self::Disappear => (0.3, 0.3, 0.8),
            Self::Blindness => (0.1, 0.1, 0.2),
        };
        // ColorParticleOption.create(float,float,float) uses ARGB.colorFromFloat,
        // whose channel conversion is floor(value * 255).
        Some(
            0xff00_0000
                | ((r * 255.0).floor() as u32) << 16
                | ((g * 255.0).floor() as u32) << 8
                | (b * 255.0).floor() as u32,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GuardianTargetSample {
    position: DVec3,
    bbox_height: f64,
}

/// A player target view supplied by GameState when its local player is not in
/// EntityStore. `bbox_height` is the current pose's actual bounding-box height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GuardianTargetView {
    pub entity_id: i32,
    pub position: DVec3,
    pub bbox_height: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MobParticleState {
    guardian_target_id: Option<i32>,
    guardian_attack_ticks: u16,
    guardian_cached_target: Option<GuardianTargetSample>,
    guardian_target_removed: bool,
    guardian_moving: bool,
    spell: Spell,
}

/// Apply only the class-specific native metadata fields verified against Java
/// 26.2: Guardian moving/target (16/17) and Spellcaster spell byte (17).
pub(super) fn on_metadata(
    state: &mut MobParticleState,
    kind: EntityKind,
    index: u8,
    value: MetaValue,
) {
    match (kind, index, value) {
        (EntityKind::Guardian | EntityKind::ElderGuardian, 16, MetaValue::Bool(moving)) => {
            state.guardian_moving = moving;
        }
        (EntityKind::Guardian | EntityKind::ElderGuardian, 17, MetaValue::Int(target)) => {
            let target = (target > 0).then_some(target);
            if state.guardian_target_id != target {
                state.guardian_target_id = target;
                state.guardian_attack_ticks = 0;
                state.guardian_cached_target = None;
                state.guardian_target_removed = false;
            }
        }
        (EntityKind::Evoker | EntityKind::Illusioner, 17, MetaValue::Byte(spell)) => {
            state.spell = Spell::from_id(spell);
        }
        _ => {}
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

fn range(rng: &mut fastrand::Rng, extent: f64) -> f64 {
    (rng.f64() - 0.5) * extent
}

fn portal_pair(
    out: &mut Vec<ParticleSpawnRequest>,
    entity: &LivingEntity,
    rng: &mut fastrand::Rng,
    enderman: bool,
) {
    let dimensions = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let (width, height) = (f64::from(dimensions.width), f64::from(dimensions.height));
    for _ in 0..2 {
        let position = DVec3::new(
            entity.position.x + range(rng, width * 0.5),
            entity.position.y + rng.f64() * height - if enderman { 0.25 } else { 0.0 },
            entity.position.z + range(rng, width * 0.5),
        );
        let velocity = DVec3::new(range(rng, 2.0), -rng.f64(), range(rng, 2.0));
        out.push(request(Kind::Portal, Options::Simple, position, velocity));
    }
}

fn scale(entity: &LivingEntity) -> f64 {
    entity
        .attributes
        .get("minecraft:scale")
        .copied()
        .unwrap_or(1.0)
}

fn target_sample(entity: &LivingEntity) -> GuardianTargetSample {
    let dimensions = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let dimensions = EntityStore::dimensions_for_pose(
        super::EntityDimensions {
            width: f64::from(dimensions.width),
            height: f64::from(dimensions.height),
        },
        entity.pose,
    );
    let baby_scale = if entity.is_baby { 0.5 } else { 1.0 };
    GuardianTargetSample {
        position: DVec3::from(entity.position),
        bbox_height: dimensions.height * scale(entity) * baby_scale,
    }
}

pub(super) fn on_entity_removed(
    states: &mut HashMap<i32, MobParticleState>,
    removed_id: i32,
    entity: &LivingEntity,
) {
    let sample = target_sample(entity);
    for state in states.values_mut() {
        if state.guardian_target_id == Some(removed_id) && state.guardian_cached_target.is_some() {
            // Java retains its cached LivingEntity reference until target
            // metadata changes; freeze its last position after removal.
            state.guardian_cached_target = Some(sample);
            state.guardian_target_removed = true;
        }
    }
}

fn guardian_beam(
    out: &mut Vec<ParticleSpawnRequest>,
    source: &LivingEntity,
    target: GuardianTargetSample,
    attack_ticks: u16,
    attack_duration: u16,
    rng: &mut fastrand::Rng,
) {
    let source_dimensions = azalea_entity::dimensions::EntityDimensions::from(source.entity_type);
    let source_scale = scale(source);
    let start = DVec3::new(
        source.position.x,
        source.position.y + f64::from(source_dimensions.eye_height) * source_scale,
        source.position.z,
    );
    // Guardian.java uses target.getY(0.5), i.e. the vertical bbox midpoint.
    let end = DVec3::new(
        target.position.x,
        target.position.y + target.bbox_height * 0.5,
        target.position.z,
    );
    let delta = end - start;
    let distance = delta.length();
    if distance <= f64::EPSILON {
        return;
    }
    let direction = delta / distance;
    let attack_scale = f64::from(attack_ticks) / f64::from(attack_duration);
    let mut step = rng.f64();
    while step < distance {
        step += 1.8 - attack_scale + rng.f64() * (1.7 - attack_scale);
        out.push(request(
            Kind::Bubble,
            Options::Simple,
            start + direction * step,
            DVec3::ZERO,
        ));
    }
}

fn spell_hands(out: &mut Vec<ParticleSpawnRequest>, entity: &LivingEntity, spell: Spell) {
    let Some(color) = spell.color() else { return };
    let angle =
        entity.body_y_rot_deg.to_radians() + (entity.age_in_ticks as f32 * 0.6662).cos() * 0.25;
    let offset = DVec3::new(
        f64::from(angle.cos()) * 0.6 * scale(entity),
        1.8 * scale(entity),
        f64::from(angle.sin()) * 0.6 * scale(entity),
    );
    for sign in [1.0, -1.0] {
        out.push(request(
            Kind::EntityEffect,
            Options::EntityEffect { color },
            DVec3::from(entity.position) + offset * sign,
            DVec3::ZERO,
        ));
    }
}

/// The local player lives in `GameState`, not `EntityStore`; integration passes
/// its live bbox view here when Guardian's synced target ID equals local ID.
pub(super) fn tick_with_target_view(
    store: &mut EntityStore,
    _chunks: &ChunkStore,
    game_time: i64,
    local_player: Option<GuardianTargetView>,
) -> Vec<ParticleSpawnRequest> {
    let mut out = Vec::new();
    for (&id, entity) in &store.living {
        let seed = (id as u64).wrapping_mul(0x9e37_79b9) ^ game_time as u64;
        let mut rng = fastrand::Rng::with_seed(seed);
        match entity.entity_type {
            // Enderman.aiStep and Endermite.aiStep each add exactly two PORTAL
            // particles per client tick; neither is teleport-gated.
            EntityKind::Enderman => portal_pair(&mut out, entity, &mut rng, true),
            EntityKind::Endermite => portal_pair(&mut out, entity, &mut rng, false),
            EntityKind::Guardian | EntityKind::ElderGuardian if entity.health > 0.0 => {
                let state = store.mob_particle_state.entry(id).or_default();
                if entity.is_in_water && state.guardian_moving {
                    let dimensions =
                        azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
                    let yaw = entity.look_dir.y_rot_deg().to_radians();
                    let pitch = entity.look_dir.x_rot_deg().to_radians();
                    let look = Vec3::new(
                        -yaw.sin() * pitch.cos(),
                        -pitch.sin(),
                        yaw.cos() * pitch.cos(),
                    );
                    let mob_scale = scale(entity);
                    for _ in 0..2 {
                        let position = DVec3::new(
                            entity.position.x
                                + range(&mut rng, f64::from(dimensions.width) * mob_scale * 0.5)
                                - f64::from(look.x) * 1.5,
                            entity.position.y
                                + rng.f64() * f64::from(dimensions.height) * mob_scale
                                - f64::from(look.y) * 1.5,
                            entity.position.z
                                + range(&mut rng, f64::from(dimensions.width) * mob_scale * 0.5)
                                - f64::from(look.z) * 1.5,
                        );
                        out.push(request(
                            Kind::Bubble,
                            Options::Simple,
                            position,
                            DVec3::ZERO,
                        ));
                    }
                }
                let duration = if entity.entity_type == EntityKind::ElderGuardian {
                    60
                } else {
                    80
                };
                if let Some(target_id) = state.guardian_target_id {
                    // Java hasActiveAttackTarget gates the clock on the synced
                    // ID, not successful resolution of its cached entity.
                    state.guardian_attack_ticks =
                        state.guardian_attack_ticks.saturating_add(1).min(duration);
                    if !state.guardian_target_removed {
                        let resolved = if let Some(player) =
                            local_player.filter(|player| player.entity_id == target_id)
                        {
                            Some(GuardianTargetSample {
                                position: player.position,
                                bbox_height: player.bbox_height,
                            })
                        } else {
                            store.living.get(&target_id).map(target_sample)
                        };
                        if let Some(resolved) = resolved {
                            state.guardian_cached_target = Some(resolved);
                        }
                    }
                    if let Some(target) = state.guardian_cached_target {
                        guardian_beam(
                            &mut out,
                            entity,
                            target,
                            state.guardian_attack_ticks,
                            duration,
                            &mut rng,
                        );
                    }
                } else {
                    state.guardian_attack_ticks = 0;
                }
            }
            EntityKind::Evoker | EntityKind::Illusioner => {
                let spell = store.mob_particle_state.entry(id).or_default().spell;
                spell_hands(&mut out, entity, spell);
            }
            _ => {}
        }
    }
    // If an attacker was removed from the entity collection, retire its visual
    // state. Cached target snapshots remain owned by other live Guardians.
    store
        .mob_particle_state
        .retain(|id, _| store.living.contains_key(id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Position;
    use crate::entity::components::LookDirection;

    fn entity(kind: EntityKind, position: Position) -> LivingEntity {
        LivingEntity::new(kind, position, LookDirection::default(), 0.0, 0.0, None)
    }

    fn spawn(store: &mut EntityStore, id: i32, kind: EntityKind, position: Position) {
        store.spawn_living(id, kind, position, LookDirection::default(), 0.0, None);
    }

    fn tick(
        store: &mut EntityStore,
        chunks: &ChunkStore,
        game_time: i64,
    ) -> Vec<ParticleSpawnRequest> {
        tick_with_target_view(store, chunks, game_time, None)
    }

    #[test]
    fn enderman_and_endermite_emit_two_portal_particles_each_tick() {
        let mut store = EntityStore::new();
        spawn(&mut store, 1, EntityKind::Enderman, Position::default());
        spawn(&mut store, 2, EntityKind::Endermite, Position::default());
        let requests = tick(&mut store, &ChunkStore::new(1), 15);
        assert_eq!(requests.len(), 4);
        assert!(requests.iter().all(|r| r.kind == Kind::Portal));
        assert_eq!(requests.iter().filter(|r| r.velocity.y <= 0.0).count(), 4);
    }

    #[test]
    fn all_spell_ids_use_java_entity_effect_colors() {
        let expected = [
            (1, 0xffb2_b2_cc),
            (2, 0xff66_4c_59),
            (3, 0xffb2_7f_33),
            (4, 0xff4c_4c_cc),
            (5, 0xff19_19_33),
        ];
        for (id, color) in expected {
            assert_eq!(Spell::from_id(id).color(), Some(color));
        }
        assert_eq!(Spell::from_id(0), Spell::None);
        assert_eq!(Spell::from_id(255), Spell::None);
    }

    #[test]
    fn guardian_target_metadata_drives_timed_beam_and_resets_on_target_changes() {
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::Guardian,
            Position::new(0.0, 0.0, 0.0),
        );
        spawn(
            &mut store,
            2,
            EntityKind::Player,
            Position::new(0.0, 2.0, 12.0),
        );
        store.apply_entity_data(1, 17, MetaValue::Int(2));
        let first = tick(&mut store, &ChunkStore::new(1), 1);
        assert!(!first.is_empty());
        assert!(first.iter().all(|r| r.kind == Kind::Bubble));
        let source_eye = f64::from(
            azalea_entity::dimensions::EntityDimensions::from(EntityKind::Guardian).eye_height,
        );
        let target_mid = 2.0
            + f64::from(
                azalea_entity::dimensions::EntityDimensions::from(EntityKind::Player).height,
            ) * 0.5;
        let slope = (target_mid - source_eye) / 12.0;
        assert!(first.iter().all(|r| {
            r.position.z > 0.0 && (r.position.y - source_eye - r.position.z * slope).abs() < 1e-6
        }));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 1);
        tick(&mut store, &ChunkStore::new(1), 2);
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 2);
        store.apply_entity_data(1, 17, MetaValue::Int(2));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 2);

        store.living.get_mut(&2).unwrap().health = 0.0;
        assert!(!tick(&mut store, &ChunkStore::new(1), 3).is_empty());
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 3);

        store.remove_entity(2);
        assert!(!store.mob_particle_state.contains_key(&2));
        assert!(!tick(&mut store, &ChunkStore::new(1), 4).is_empty());
        assert_eq!(store.mob_particle_state[&1].guardian_target_id, Some(2));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 4);
        assert!(store.mob_particle_state[&1].guardian_target_removed);

        spawn(
            &mut store,
            3,
            EntityKind::Player,
            Position::new(0.0, 2.0, 12.0),
        );
        store.apply_entity_data(1, 17, MetaValue::Int(3));
        assert!(!tick(&mut store, &ChunkStore::new(1), 5).is_empty());
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 1);
        store.apply_entity_data(1, 17, MetaValue::Int(2));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 0);
        assert!(tick(&mut store, &ChunkStore::new(1), 6).is_empty());
        assert_eq!(store.mob_particle_state[&1].guardian_target_id, Some(2));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 1);
    }

    #[test]
    fn guardian_pending_target_id_retries_lookup_when_entity_loads_later() {
        let mut store = EntityStore::new();
        spawn(&mut store, 1, EntityKind::Guardian, Position::default());
        store.apply_entity_data(1, 17, MetaValue::Int(2));
        assert!(tick(&mut store, &ChunkStore::new(1), 1).is_empty());
        assert_eq!(store.mob_particle_state[&1].guardian_target_id, Some(2));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 1);
        assert_eq!(store.mob_particle_state[&1].guardian_cached_target, None);

        spawn(
            &mut store,
            2,
            EntityKind::Player,
            Position::new(0.0, 2.0, 12.0),
        );
        assert!(!tick(&mut store, &ChunkStore::new(1), 2).is_empty());
        assert_eq!(store.mob_particle_state[&1].guardian_target_id, Some(2));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 2);
        let cached = store.mob_particle_state[&1].guardian_cached_target.unwrap();
        store.remove_entity(2);
        spawn(
            &mut store,
            2,
            EntityKind::Player,
            Position::new(20.0, 5.0, -4.0),
        );
        assert!(!tick(&mut store, &ChunkStore::new(1), 3).is_empty());
        assert_eq!(store.mob_particle_state[&1].guardian_target_id, Some(2));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 3);
        assert_eq!(
            store.mob_particle_state[&1].guardian_cached_target,
            Some(cached)
        );
        assert!(store.mob_particle_state[&1].guardian_target_removed);
    }

    #[test]
    fn guardian_resolves_local_player_from_explicit_target_view() {
        let mut store = EntityStore::new();
        spawn(&mut store, 1, EntityKind::Guardian, Position::default());
        store.apply_entity_data(1, 17, MetaValue::Int(99));
        let requests = tick_with_target_view(
            &mut store,
            &ChunkStore::new(1),
            1,
            Some(GuardianTargetView {
                entity_id: 99,
                position: DVec3::new(0.0, 2.0, 12.0),
                bbox_height: 1.8,
            }),
        );
        assert!(!requests.is_empty());
        assert!(requests.iter().all(|request| request.kind == Kind::Bubble));
        assert_eq!(store.mob_particle_state[&1].guardian_target_id, Some(99));
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 1);
    }

    #[test]
    fn guardian_swimming_bubbles_are_two_additional_requests() {
        let mut swimming = EntityStore::new();
        spawn(&mut swimming, 1, EntityKind::Guardian, Position::default());
        swimming.living.get_mut(&1).unwrap().is_in_water = true;
        swimming.apply_entity_data(1, 16, MetaValue::Bool(true));
        let requests = tick(&mut swimming, &ChunkStore::new(1), 7);
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| r.kind == Kind::Bubble));
    }

    #[test]
    fn elder_guardian_uses_sixty_tick_beam_duration() {
        let mut store = EntityStore::new();
        spawn(
            &mut store,
            1,
            EntityKind::ElderGuardian,
            Position::default(),
        );
        spawn(
            &mut store,
            2,
            EntityKind::Player,
            Position::new(0.0, 1.0, 15.0),
        );
        store.apply_entity_data(1, 17, MetaValue::Int(2));
        for tick_id in 0..61 {
            tick(&mut store, &ChunkStore::new(1), tick_id);
        }
        assert_eq!(store.mob_particle_state[&1].guardian_attack_ticks, 60);
    }

    #[test]
    fn mob_visual_state_is_removed_and_reset_on_entity_id_reuse() {
        let mut store = EntityStore::new();
        spawn(&mut store, 8, EntityKind::Evoker, Position::default());
        store.apply_entity_data(8, 17, MetaValue::Byte(1));
        assert_eq!(store.mob_particle_state[&8].spell, Spell::SummonVex);
        store.remove_entity(8);
        assert!(!store.mob_particle_state.contains_key(&8));
        spawn(&mut store, 8, EntityKind::Evoker, Position::default());
        assert_eq!(store.mob_particle_state[&8].spell, Spell::None);
    }

    #[test]
    fn spell_metadata_emits_colored_particles_at_both_hands_and_none_stops() {
        let mut store = EntityStore::new();
        let mut evoker = entity(EntityKind::Evoker, Position::new(4.0, 10.0, 8.0));
        evoker.body_y_rot_deg = 90.0;
        evoker.age_in_ticks = 7;
        store.living.insert(5, evoker);
        store
            .mob_particle_state
            .insert(5, MobParticleState::default());
        store.apply_entity_data(5, 17, MetaValue::Byte(1));
        let requests = tick(&mut store, &ChunkStore::new(1), 10);
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| {
            r.kind == Kind::EntityEffect
                && matches!(
                    &r.options,
                    Options::EntityEffect {
                        color: 0xffb2_b2_cc
                    }
                )
        }));
        assert!((requests[0].position.x + requests[1].position.x - 8.0).abs() < 1e-5);
        assert!((requests[0].position.y - 11.8).abs() < 1e-6);
        assert!((requests[1].position.y - 8.2).abs() < 1e-6);
        assert!((requests[0].position.z - 8.6).abs() < 1e-3);
        assert!((requests[1].position.z - 7.4).abs() < 1e-3);

        store.apply_entity_data(5, 17, MetaValue::Byte(0));
        assert!(tick(&mut store, &ChunkStore::new(1), 11).is_empty());
    }
}
