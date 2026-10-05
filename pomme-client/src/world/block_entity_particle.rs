//! Client-local block-entity particle ticks. Network `LevelParticles` events
//! stay packet-owned; this module only reproduces visuals created by Java
//! clientTick methods.
use azalea_core::position::BlockPos;
use glam::{DVec3, dvec3};

use crate::entity::EntityStore;
use crate::particle::{ServerParticleKind, ServerParticleOptions};
use crate::world::block_entity::{
    BellResonanceState, ConduitVisualState, SpawnerVisualState, VaultVisualState,
};
use crate::world::chunk::ChunkStore;
use crate::world::particle_tick::ParticleSpawnRequest;

#[derive(Clone, Copy, Debug)]
pub struct ClientPlayerVisual {
    pub uuid: uuid::Uuid,
    pub position: DVec3,
    pub block_pos: BlockPos,
    pub height: f64,
    pub spectator: bool,
}

/// Java `BellBlockEntity.triggerEvent(1, direction)`: the packet block and
/// current block state must both identify the bell before touching its BE.
pub fn on_bell_block_event(
    chunks: &mut ChunkStore,
    entities: &EntityStore,
    spectator_uuids: &std::collections::HashSet<uuid::Uuid>,
    pos: BlockPos,
    packet_block: azalea_registry::builtin::BlockKind,
    action_id: u8,
    direction: u8,
    game_time: i64,
    local_player: Option<&ClientPlayerVisual>,
) -> bool {
    if packet_block != azalea_registry::builtin::BlockKind::Bell
        || action_id != 1
        || crate::world::block::block_id(chunks.get_block_state(pos.x, pos.y, pos.z)) != "bell"
    {
        return false;
    }
    let Some(entity) = chunks.block_entities.get_mut(&pos) else {
        return false;
    };
    if entity.kind != azalea_registry::builtin::BlockEntityKind::Bell
        || !entity.start_bell_swing(direction)
    {
        return false;
    }
    let Some(resonance) = entity.bell_resonance.as_mut() else {
        return false;
    };
    resonance.resonation_ticks = 0;
    if game_time > resonance.last_ring_timestamp + 60 || resonance.nearby_entities.is_none() {
        resonance.last_ring_timestamp = game_time;
        let mut nearby_entities = entities
            .living
            .iter()
            .filter_map(|(&id, e)| {
                let p = DVec3::from(*e.position);
                let dimensions = azalea_entity::dimensions::EntityDimensions::from(e.entity_type);
                let dimensions = EntityStore::dimensions_for_pose(
                    crate::entity::EntityDimensions {
                        width: f64::from(dimensions.width),
                        height: f64::from(dimensions.height),
                    },
                    e.pose,
                );
                let half_width = dimensions.width * 0.5;
                let aabb_intersects = p.x + half_width > pos.x as f64 - 48.0
                    && p.x - half_width < pos.x as f64 + 49.0
                    && p.y + dimensions.height > pos.y as f64 - 48.0
                    && p.y < pos.y as f64 + 49.0
                    && p.z + half_width > pos.z as f64 - 48.0
                    && p.z - half_width < pos.z as f64 + 49.0;
                (aabb_intersects
                    && !e.player_uuid.is_some_and(|uuid| {
                        spectator_uuids.contains(&uuid)
                            || local_player.is_some_and(|player| player.uuid == uuid)
                    }))
                .then_some(id)
            })
            .collect::<Vec<_>>();
        nearby_entities.sort_unstable();
        resonance.nearby_entities = Some(nearby_entities);
        resonance.local_player_uuid = local_player
            .filter(|player| {
                let half_width = 0.3;
                player.position.x + half_width > pos.x as f64 - 48.0
                    && player.position.x - half_width < pos.x as f64 + 49.0
                    && player.position.y + player.height > pos.y as f64 - 48.0
                    && player.position.y < pos.y as f64 + 49.0
                    && player.position.z + half_width > pos.z as f64 - 48.0
                    && player.position.z - half_width < pos.z as f64 + 49.0
                    && !player.spectator
            })
            .map(|player| player.uuid);
    }
    true
}

/// Record the Java `PotentSulfurBlock.triggerEvent(..., 0, 0)` client-side
/// `eruptionTick` update. Call only from the clientbound block-event owner.
pub fn on_potent_sulfur_block_event(
    chunks: &mut ChunkStore,
    pos: BlockPos,
    action_id: u8,
    game_time: i64,
) -> bool {
    if action_id != 0
        || crate::world::block::block_id(chunks.get_block_state(pos.x, pos.y, pos.z))
            != "potent_sulfur"
    {
        return false;
    }
    let Some(entity) = chunks.block_entities.get_mut(&pos) else {
        return false;
    };
    let Some(visual) = entity.potent_sulfur_visual.as_mut() else {
        return false;
    };
    visual.eruption_tick = Some(game_time);
    true
}

/// Tick block-entity client visuals once per fixed client tick using retained
/// players, entity UUID lookup and actual loaded block states.
pub fn tick_block_entity_particles(
    chunks: &mut ChunkStore,
    players: &[ClientPlayerVisual],
    entities: &EntityStore,
    game_time: i64,
) -> Vec<ParticleSpawnRequest> {
    let mut out = Vec::new();
    let snapshots = chunks
        .block_entities
        .iter()
        .map(|(&pos, entity)| {
            (
                pos,
                chunks.get_block_state(pos.x, pos.y, pos.z),
                entity.spawner_visual,
                entity.vault_visual.clone(),
                entity.conduit_visual.clone(),
                entity.potent_sulfur_visual,
                entity.bell_resonance.clone(),
            )
        })
        .collect::<Vec<_>>();
    for (pos, block_state, mut spawner, vault, mut conduit, mut sulfur, mut bell) in snapshots {
        let block_id = crate::world::block::block_id(block_state);
        if block_id == "bell" {
            if let Some(state) = bell.as_mut() {
                let swing = chunks.block_entities.get(&pos).and_then(|be| be.bell_swing);
                out.extend(bell_resonance_particles(
                    pos, swing, state, entities, players,
                ));
                if let Some(entity) = chunks.block_entities.get_mut(&pos) {
                    entity.bell_resonance = bell;
                }
            }
            continue;
        }
        if block_id == "potent_sulfur" {
            if let Some(state) = sulfur.as_mut() {
                out.extend(potent_sulfur_particles(
                    chunks,
                    pos,
                    block_state,
                    state,
                    game_time,
                ));
                if let Some(entity) = chunks.block_entities.get_mut(&pos) {
                    entity.potent_sulfur_visual = sulfur;
                }
            }
            continue;
        }
        if block_id == "conduit" {
            if let Some(state) = conduit.as_mut() {
                out.extend(conduit_particles(chunks, pos, state, entities, game_time));
                if let Some(entity) = chunks.block_entities.get_mut(&pos) {
                    entity.conduit_visual = conduit;
                }
            }
            continue;
        }
        if block_id == "trial_spawner" {
            let properties = crate::world::block::block_properties(block_state);
            out.extend(trial_spawner_particles(
                pos,
                properties.get("trial_spawner_state").unwrap_or("inactive"),
                properties.get("ominous") == Some("true"),
                game_time,
            ));
            continue;
        }
        if block_id == "vault" {
            if let Some(vault) = vault {
                if fastrand::f32() <= 0.5 {
                    out.extend(vault_idle_particles(
                        pos,
                        vault.has_display_item,
                        crate::world::block::block_properties(block_state).get("ominous")
                            == Some("true"),
                    ));
                }
                if game_time.rem_euclid(20) == 0 {
                    out.extend(vault_connection_particles(
                        pos,
                        block_state,
                        &vault,
                        players,
                        &mut fastrand::Rng::new(),
                    ));
                }
            }
            continue;
        }
        if block_id != "spawner"
            || !spawner.is_some_and(|state| {
                let range = f64::from(state.required_player_range);
                range >= 0.0
                    && players.iter().any(|player| {
                        let distance = player.position.distance_squared(pos_center(pos));
                        !player.spectator && distance < range * range
                    })
            })
        {
            continue;
        }
        if let Some(state) = spawner.as_mut() {
            out.extend(spawner_particles(pos, state));
            if let Some(entity) = chunks.block_entities.get_mut(&pos) {
                entity.spawner_visual = spawner;
            }
        }
    }
    out
}

fn bell_resonance_particles(
    pos: BlockPos,
    swing: Option<crate::world::block_entity::BellSwing>,
    state: &mut BellResonanceState,
    entities: &EntityStore,
    players: &[ClientPlayerVisual],
) -> Vec<ParticleSpawnRequest> {
    let Some(cached) = state.nearby_entities.as_ref() else {
        return Vec::new();
    };
    let player_at = |uuid: uuid::Uuid| players.iter().find(|p| p.uuid == uuid);
    let near = |p: DVec3, radius: f64| {
        let center = pos_center(pos);
        p.distance_squared(center) < radius * radius
    };
    let raiders = cached
        .iter()
        .filter_map(|id| entities.living.get(id))
        .filter(|entity| {
            entity.health > 0.0
                && near(DVec3::from(*entity.position), 48.0)
                && entities.is_raider_type(entity.entity_type)
        })
        .collect::<Vec<_>>();
    if swing.is_some_and(|swing| swing.ticks >= 5)
        && state.resonation_ticks == 0
        && cached
            .iter()
            .filter_map(|id| entities.living.get(id))
            .any(|entity| {
                entity.health > 0.0
                    && near(DVec3::from(*entity.position), 32.0)
                    && entities.is_raider_type(entity.entity_type)
            })
    {
        state.resonating = true;
    }
    if !state.resonating {
        return Vec::new();
    }
    if state.resonation_ticks < 40 {
        state.resonation_ticks += 1;
        return Vec::new();
    }
    state.resonating = false;
    let local = state.local_player_uuid.and_then(player_at);
    let nearby_count = cached
        .iter()
        .filter_map(|id| entities.living.get(id))
        .filter(|entity| near(DVec3::from(*entity.position), 48.0))
        .count()
        + usize::from(local.is_some_and(|player| near(player.position, 48.0)));
    let count = (((nearby_count as i32 - 21) / -2).clamp(3, 15)) as usize;
    let mut color = 16_700_985i32;
    let mut out = Vec::new();
    for entity in raiders {
        let p = DVec3::from(*entity.position);
        let dist = ((p.x - pos.x as f64).powi(2) + (p.z - pos.z as f64).powi(2)).sqrt();
        let particle_pos = dvec3(
            f64::from(pos.x as f32 + 0.5) + (p.x - pos.x as f64) / dist,
            f64::from(pos.y as f32 + 0.5),
            f64::from(pos.z as f32 + 0.5) + (p.z - pos.z as f64) / dist,
        );
        for _ in 0..count {
            color += 5;
            out.push(ParticleSpawnRequest {
                kind: ServerParticleKind::EntityEffect,
                options: ServerParticleOptions::EntityEffect {
                    color: color as u32,
                },
                position: particle_pos,
                velocity: DVec3::ZERO,
                always_visible: false,
            });
        }
    }
    out
}

fn potent_sulfur_particles(
    chunks: &ChunkStore,
    origin: BlockPos,
    block_state: azalea_block::BlockState,
    visual: &mut crate::world::block_entity::PotentSulfurVisualState,
    game_time: i64,
) -> Vec<ParticleSpawnRequest> {
    let start = *visual.eruption_tick.get_or_insert(game_time);
    let props = crate::world::block::block_properties(block_state);
    let sulfur_state = props.get("potent_sulfur_state").unwrap_or("dry");
    let Some(source) = noxious_gas_source(chunks, origin) else {
        return Vec::new();
    };
    let mut requests = Vec::new();
    if matches!(sulfur_state, "wet" | "dormant") && game_time.rem_euclid(20) == 0 {
        requests.push(simple(
            ServerParticleKind::NoxiousGasCloud,
            pos_center(source),
        ));
    }
    if matches!(sulfur_state, "erupting" | "continuous") && (game_time - start).rem_euclid(20) == 0
    {
        let water_blocks = source.y - origin.y - 1;
        requests.push(ParticleSpawnRequest {
            kind: ServerParticleKind::Geyser,
            options: ServerParticleOptions::Geyser { water_blocks },
            position: dvec3(
                source.x as f64 + 0.5,
                source.y as f64,
                source.z as f64 + 0.5,
            ),
            velocity: DVec3::ZERO,
            always_visible: false,
        });
    }
    requests
}

fn noxious_gas_source(chunks: &ChunkStore, origin: BlockPos) -> Option<BlockPos> {
    for y in (origin.y + 1)..=(origin.y + 5) {
        let pos = BlockPos::new(origin.x, y, origin.z);
        let state = chunks.get_block_state(pos.x, pos.y, pos.z);
        let fluid = crate::world::block::fluid(state);
        let source_water = fluid.kind == crate::world::block::FluidKind::Water && fluid.amount == 8;
        let is_water = crate::world::block::block_id(state) == "water";
        let passable = crate::world::block::noxious_gas_collision_empty(state, pos, Some(origin.y));
        if !source_water || (!is_water && !passable) {
            if !crate::world::block::is_air(state) && !passable {
                return None;
            }
            return Some(pos);
        }
    }
    None
}

/// Java `CollisionContext.positionContext(origin.y)` handling used by
/// `isGeyserPassableBlock`. The only context-dependent collision shapes in
/// these source columns are powder snow and scaffolding.
fn conduit_particles(
    chunks: &ChunkStore,
    origin: BlockPos,
    state: &mut ConduitVisualState,
    entities: &EntityStore,
    game_time: i64,
) -> Vec<ParticleSpawnRequest> {
    state.tick_count = state.tick_count.wrapping_add(1);
    if game_time.rem_euclid(40) == 0 {
        state.effect_blocks.clear();
        let mut fully_waterlogged = true;
        'water_cube: for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let at = BlockPos::new(origin.x + x, origin.y + y, origin.z + z);
                    if crate::world::block::fluid(chunks.get_block_state(at.x, at.y, at.z)).kind
                        != crate::world::block::FluidKind::Water
                    {
                        fully_waterlogged = false;
                        break 'water_cube;
                    }
                }
            }
        }
        if !fully_waterlogged {
            state.effect_blocks.clear();
        } else {
            for x in -2i32..=2 {
                for y in -2i32..=2 {
                    for z in -2i32..=2 {
                        let ax = x.abs();
                        let ay = y.abs();
                        let az = z.abs();
                        if ax <= 1 && ay <= 1 && az <= 1
                            || !((x != 0 || ay != 2 && az != 2)
                                && (y != 0 || ax != 2 && az != 2)
                                && (z != 0 || ax != 2 && ay != 2))
                        {
                            continue;
                        }
                        let at = BlockPos::new(origin.x + x, origin.y + y, origin.z + z);
                        if matches!(
                            crate::world::block::block_id(chunks.get_block_state(at.x, at.y, at.z)),
                            "prismarine" | "prismarine_bricks" | "sea_lantern" | "dark_prismarine"
                        ) {
                            state.effect_blocks.push(at);
                        }
                    }
                }
            }
        }
    }
    let bob = ((state.tick_count as f32 + 35.0) * 0.1).sin() * 0.5 + 0.5;
    let bob = (bob * bob + bob) * 0.3;
    let end = dvec3(
        origin.x as f64 + 0.5,
        origin.y as f64 + 1.5 + f64::from(bob),
        origin.z as f64 + 0.5,
    );
    let mut requests = state
        .effect_blocks
        .iter()
        .filter_map(|block| {
            if fastrand::u32(0..50) != 0 {
                return None;
            }
            let delta = dvec3(
                f64::from(block.x - origin.x),
                f64::from(block.y - origin.y),
                f64::from(block.z - origin.z),
            );
            let velocity = dvec3(
                -0.5 + fastrand::f64() + delta.x,
                -2.0 + fastrand::f64() + delta.y,
                -0.5 + fastrand::f64() + delta.z,
            );
            Some(simple_with_velocity(
                ServerParticleKind::Nautilus,
                end,
                velocity,
            ))
        })
        .collect::<Vec<_>>();
    if let Some(target) = state.target_uuid
        && let Some(particle) = conduit_target_particle(target, entities)
    {
        requests.push(particle);
    }
    requests
}

fn conduit_target_particle(
    target_uuid: uuid::Uuid,
    entities: &EntityStore,
) -> Option<ParticleSpawnRequest> {
    let entity = entities.living_by_uuid(&target_uuid)?;
    if entity.health <= 0.0 {
        return None;
    }
    let base = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let dimensions = EntityStore::dimensions_for_pose(
        crate::entity::EntityDimensions {
            width: f64::from(base.width),
            height: f64::from(base.height),
        },
        entity.pose,
    );
    let eye_height = if entity.entity_type == azalea_registry::builtin::EntityKind::Player {
        match entity.pose {
            crate::entity::EntityPose::Sleeping => f64::from(crate::player::SLEEPING_EYE_HEIGHT),
            crate::entity::EntityPose::Crouching => f64::from(crate::player::CROUCH_EYE_HEIGHT),
            crate::entity::EntityPose::Swimming
            | crate::entity::EntityPose::FallFlying
            | crate::entity::EntityPose::SpinAttack => {
                f64::from(crate::player::SWIMMING_EYE_HEIGHT)
            }
            _ => f64::from(crate::player::STANDING_EYE_HEIGHT),
        }
    } else {
        f64::from(base.eye_height)
    };
    let position = DVec3::from(entity.position);
    let target_position = position + dvec3(0.0, eye_height, 0.0);
    let width = dimensions.width;
    let height = dimensions.height;
    let velocity = dvec3(
        (fastrand::f32() as f64 - 0.5) * (3.0 + width),
        -1.0 + fastrand::f64() * height,
        (fastrand::f32() as f64 - 0.5) * (3.0 + width),
    );
    Some(simple_with_velocity(
        ServerParticleKind::Nautilus,
        target_position,
        velocity,
    ))
}

fn trial_spawner_particles(
    pos: BlockPos,
    state: &str,
    ominous: bool,
    game_time: i64,
) -> Vec<ParticleSpawnRequest> {
    let center = pos_center(pos);
    let flame = if ominous {
        ServerParticleKind::SoulFireFlame
    } else {
        ServerParticleKind::SmallFlame
    };
    let mut requests = Vec::new();
    match state {
        "waiting_for_players" | "waiting_for_reward_ejection" | "ejecting_reward" => {
            if fastrand::u32(0..2) == 0 {
                requests.push(simple(
                    flame,
                    center
                        + dvec3(
                            (fastrand::f32() as f64 - 0.5) * 0.9,
                            (fastrand::f32() as f64 - 0.5) * 0.9,
                            (fastrand::f32() as f64 - 0.5) * 0.9,
                        ),
                ));
            }
        }
        "active" => {
            let point = center
                + dvec3(
                    fastrand::f32() as f64 - 0.5,
                    fastrand::f32() as f64 - 0.5,
                    fastrand::f32() as f64 - 0.5,
                );
            requests.push(simple(ServerParticleKind::Smoke, point));
            requests.push(simple(
                if ominous {
                    ServerParticleKind::SoulFireFlame
                } else {
                    ServerParticleKind::Flame
                },
                point,
            ));
        }
        "cooldown" => {
            let point = center
                + dvec3(
                    (fastrand::f32() as f64 - 0.5) * 0.9,
                    (fastrand::f32() as f64 - 0.5) * 0.9,
                    (fastrand::f32() as f64 - 0.5) * 0.9,
                );
            if fastrand::u32(0..3) == 0 {
                requests.push(simple(ServerParticleKind::Smoke, point));
            }
            if game_time.rem_euclid(20) == 0 {
                for _ in 0..(20 + fastrand::u32(0..4)) {
                    requests.push(simple(
                        ServerParticleKind::Smoke,
                        center + dvec3(0.0, 0.5, 0.0),
                    ));
                }
            }
        }
        _ => {}
    }
    requests
}

fn spawner_particles(pos: BlockPos, state: &mut SpawnerVisualState) -> Vec<ParticleSpawnRequest> {
    if !state.has_display_entity {
        return Vec::new();
    }
    let point = dvec3(
        pos.x as f64 + fastrand::f64(),
        pos.y as f64 + fastrand::f64(),
        pos.z as f64 + fastrand::f64(),
    );
    if state.spawn_delay > 0 {
        state.spawn_delay -= 1;
    }
    vec![
        simple(ServerParticleKind::Smoke, point),
        simple(ServerParticleKind::Flame, point),
    ]
}

fn vault_idle_particles(
    pos: BlockPos,
    has_display_item: bool,
    ominous: bool,
) -> Vec<ParticleSpawnRequest> {
    let point = dvec3(
        pos.x as f64 + fastrand::f64() * 0.8 + 0.1,
        pos.y as f64 + fastrand::f64() * 0.5 + 0.25,
        pos.z as f64 + fastrand::f64() * 0.8 + 0.1,
    );
    let mut requests = vec![simple(ServerParticleKind::Smoke, point)];
    if has_display_item {
        requests.push(simple(
            if ominous {
                ServerParticleKind::SoulFireFlame
            } else {
                ServerParticleKind::SmallFlame
            },
            point,
        ));
    }
    requests
}

pub(crate) fn vault_connection_particles(
    pos: BlockPos,
    block_state: azalea_block::BlockState,
    vault: &VaultVisualState,
    players: &[ClientPlayerVisual],
    rng: &mut fastrand::Rng,
) -> Vec<ParticleSpawnRequest> {
    let facing = crate::world::block::block_properties(block_state)
        .get("facing")
        .unwrap_or("north");
    let (step_x, step_z) = match facing {
        "east" => (1.0, 0.0),
        "south" => (0.0, 1.0),
        "west" => (-1.0, 0.0),
        _ => (0.0, -1.0),
    };
    let keyhole = dvec3(
        pos.x as f64 + 0.5 + step_x * 0.5,
        pos.y as f64 + 1.75,
        pos.z as f64 + 0.5 + step_z * 0.5,
    );
    let range_squared = vault.connected_particles_range * vault.connected_particles_range;
    let mut requests = Vec::new();
    for uuid in &vault.connected_players {
        let Some(player) = players.iter().find(|player| player.uuid == *uuid) else {
            continue;
        };
        let dx = i64::from(player.block_pos.x) - i64::from(pos.x);
        let dy = i64::from(player.block_pos.y) - i64::from(pos.y);
        let dz = i64::from(player.block_pos.z) - i64::from(pos.z);
        let distance_squared = (dx * dx + dy * dy + dz * dz) as f64;
        if distance_squared > range_squared {
            continue;
        }
        let direction = player.position + dvec3(0.0, player.height * 0.5, 0.0) - keyhole;
        for _ in 0..rng.u32(2..=5) {
            let velocity = direction
                + dvec3(
                    f64::from(rng.f32() - 0.5),
                    f64::from(rng.f32() - 0.5),
                    f64::from(rng.f32() - 0.5),
                );
            requests.push(simple_with_velocity(
                ServerParticleKind::VaultConnection,
                keyhole,
                velocity,
            ));
        }
    }
    requests
}

fn pos_center(pos: BlockPos) -> DVec3 {
    dvec3(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5)
}

fn simple(kind: ServerParticleKind, position: DVec3) -> ParticleSpawnRequest {
    simple_with_velocity(kind, position, DVec3::ZERO)
}

fn simple_with_velocity(
    kind: ServerParticleKind,
    position: DVec3,
    velocity: DVec3,
) -> ParticleSpawnRequest {
    ParticleSpawnRequest {
        kind,
        options: ServerParticleOptions::Simple,
        position,
        velocity,
        always_visible: false,
    }
}

#[cfg(test)]
mod tests {
    use azalea_core::position::ChunkPos;
    use azalea_registry::builtin::{BlockEntityKind, EntityKind};
    use simdnbt::owned::NbtCompound;

    use super::*;

    fn world_with_block(pos: BlockPos, state: azalea_block::BlockState) -> ChunkStore {
        use azalea_world::chunk::Chunk;

        let mut chunks = ChunkStore::new(2);
        let mut chunk = Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        let column = ChunkPos::new(pos.x.div_euclid(16), pos.z.div_euclid(16));
        chunks.load_decoded_chunk(column, chunk);
        chunks.set_block_state(pos.x, pos.y, pos.z, state);
        chunks
    }

    fn int_array_uuid(uuid: uuid::Uuid) -> Vec<i32> {
        uuid.as_bytes()
            .chunks_exact(4)
            .map(|chunk| i32::from_be_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    #[test]
    fn mob_spawner_chunk_nbt_reaches_nearby_player_tick_and_uses_required_range() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(1, 64, 1);
        let state = crate::world::block::first_state_of("spawner").unwrap();
        let mut chunks = world_with_block(pos, state);
        let mut nbt = NbtCompound::new();
        nbt.insert("Delay", 2i16);
        nbt.insert("RequiredPlayerRange", 3i16);
        let mut entity = NbtCompound::new();
        entity.insert("id", "minecraft:zombie");
        let mut spawn_data = NbtCompound::new();
        spawn_data.insert("entity", entity);
        nbt.insert("SpawnData", spawn_data);
        chunks.block_entities.insert(
            pos,
            crate::world::block_entity::StoredBlockEntity::new(BlockEntityKind::MobSpawner, nbt),
        );
        let players = [ClientPlayerVisual {
            uuid: uuid::Uuid::nil(),
            position: dvec3(1.5, 64.5, 1.5),
            block_pos: pos,
            height: 1.8,
            spectator: false,
        }];
        let mut entities = EntityStore::new();
        let emitted = tick_block_entity_particles(&mut chunks, &players, &entities, 40);
        assert_eq!(
            emitted
                .iter()
                .map(|request| request.kind)
                .collect::<Vec<_>>(),
            [ServerParticleKind::Smoke, ServerParticleKind::Flame]
        );
        assert_eq!(
            chunks.block_entities[&pos]
                .spawner_visual
                .unwrap()
                .spawn_delay,
            1
        );

        entities.spawn_living(
            1,
            EntityKind::Player,
            crate::entity::components::Position::new(5.6, 64.5, 1.5),
            crate::entity::components::LookDirection::default(),
            0.0,
            Some(uuid::Uuid::nil()),
        );
        let far_player = [ClientPlayerVisual {
            uuid: uuid::Uuid::nil(),
            position: dvec3(5.6, 64.5, 1.5),
            block_pos: BlockPos::new(5, 64, 1),
            height: 1.8,
            spectator: false,
        }];
        assert!(tick_block_entity_particles(&mut chunks, &far_player, &entities, 41).is_empty());
        assert_eq!(
            chunks.block_entities[&pos]
                .spawner_visual
                .unwrap()
                .spawn_delay,
            1
        );
    }

    #[test]
    fn vault_connection_particle_resolves_synced_uuid_and_range() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        let uuid = uuid::Uuid::from_u64_pair(1, 2);
        let mut shared = NbtCompound::new();
        shared.insert(
            "connected_players",
            simdnbt::owned::NbtList::IntArray(vec![int_array_uuid(uuid)]),
        );
        shared.insert("connected_particles_range", 4.5f64);
        let mut nbt = NbtCompound::new();
        nbt.insert("shared_data", shared);
        let mut chunks =
            world_with_block(pos, crate::world::block::first_state_of("vault").unwrap());
        chunks.block_entities.insert(
            pos,
            crate::world::block_entity::StoredBlockEntity::new(BlockEntityKind::Vault, nbt),
        );
        let player = ClientPlayerVisual {
            uuid,
            position: dvec3(2.5, 64.0, 0.5),
            block_pos: BlockPos::new(2, 64, 0),
            height: 1.8,
            spectator: false,
        };
        let requests = tick_block_entity_particles(&mut chunks, &[player], &EntityStore::new(), 20);
        let connection = requests
            .iter()
            .filter(|request| request.kind == ServerParticleKind::VaultConnection)
            .collect::<Vec<_>>();
        assert!((2..=5).contains(&connection.len()));
        assert!(
            connection
                .iter()
                .all(|r| r.position == dvec3(0.5, 65.75, 0.0))
        );

        let too_far = ClientPlayerVisual {
            block_pos: BlockPos::new(6, 64, 0),
            ..player
        };
        let requests =
            tick_block_entity_particles(&mut chunks, &[too_far], &EntityStore::new(), 40);
        assert!(
            !requests
                .iter()
                .any(|r| r.kind == ServerParticleKind::VaultConnection)
        );
    }

    #[test]
    fn conduit_target_uuid_tracks_loaded_moved_dead_and_removed_mobs() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        let uuid = uuid::Uuid::from_u64_pair(3, 4);
        let mut nbt = NbtCompound::new();
        nbt.insert(
            "Target",
            simdnbt::owned::NbtTag::IntArray(int_array_uuid(uuid)),
        );
        let mut chunks =
            world_with_block(pos, crate::world::block::first_state_of("conduit").unwrap());
        chunks.block_entities.insert(
            pos,
            crate::world::block_entity::StoredBlockEntity::new(BlockEntityKind::Conduit, nbt),
        );
        let mut entities = EntityStore::new();
        entities.spawn_living(
            9,
            EntityKind::Zombie,
            crate::entity::components::Position::new(4.0, 66.0, -2.0),
            crate::entity::components::LookDirection::default(),
            0.0,
            None,
        );
        entities.set_living_uuid(9, uuid);
        let requests = tick_block_entity_particles(&mut chunks, &[], &entities, 1);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].kind, ServerParticleKind::Nautilus);
        let old_pos = requests[0].position;

        entities.teleport_living(
            9,
            crate::entity::components::Position::new(8.0, 70.0, 3.0),
            false,
        );
        entities.tick_living(
            &ChunkStore::new(2),
            crate::entity::components::Position::default(),
            10,
        );
        let moved = tick_block_entity_particles(&mut chunks, &[], &entities, 2);
        assert_eq!(moved.len(), 1);
        assert_ne!(moved[0].position, old_pos);
        entities.living.get_mut(&9).unwrap().health = 0.0;
        assert!(tick_block_entity_particles(&mut chunks, &[], &entities, 3).is_empty());
        entities.remove_entity(9);
        assert!(tick_block_entity_particles(&mut chunks, &[], &entities, 4).is_empty());
    }

    #[test]
    fn bell_block_event_is_identity_checked_and_resonance_emits_java_particles() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        let state = crate::world::block::first_state_of("bell").unwrap();
        let mut chunks = world_with_block(pos, state);
        chunks.block_entities.insert(
            pos,
            crate::world::block_entity::StoredBlockEntity::new(
                BlockEntityKind::Bell,
                NbtCompound::new(),
            ),
        );
        let mut entities = EntityStore::new();
        entities.spawn_living(
            7,
            EntityKind::Witch,
            crate::entity::components::Position::new(4.0, 64.0, 0.0),
            crate::entity::components::LookDirection::default(),
            0.0,
            None,
        );
        let local = ClientPlayerVisual {
            uuid: uuid::Uuid::nil(),
            position: dvec3(3.0, 64.0, 0.0),
            block_pos: BlockPos::new(3, 64, 0),
            height: 1.8,
            spectator: false,
        };
        assert!(!on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::NoteBlock,
            1,
            2,
            1,
            Some(&local),
        ));
        assert!(!on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            0,
            2,
            1,
            Some(&local),
        ));
        assert!(on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            1,
            2,
            1,
            Some(&local),
        ));
        let mut requests = Vec::new();
        for game_time in 1..=46 {
            chunks
                .block_entities
                .get_mut(&pos)
                .unwrap()
                .tick_bell_swing();
            requests.extend(tick_block_entity_particles(
                &mut chunks,
                &[local],
                &entities,
                game_time,
            ));
        }
        let particles = requests
            .iter()
            .filter(|r| r.kind == ServerParticleKind::EntityEffect)
            .collect::<Vec<_>>();
        // Nearby living count includes the local player and the raider: Java's
        // integer formula yields 9 particles for a count of two.
        assert_eq!(particles.len(), 9);
        assert_eq!(particles[0].position, dvec3(1.5, 64.5, 0.5));
        assert!(particles.iter().all(|r| r.velocity == DVec3::ZERO));
        assert!(matches!(
            particles[0].options,
            ServerParticleOptions::EntityEffect { color: 16_700_990 }
        ));
        assert!(
            !chunks.block_entities[&pos]
                .bell_resonance
                .as_ref()
                .unwrap()
                .resonating
        );
        entities.spawn_living(
            8,
            EntityKind::Pillager,
            crate::entity::components::Position::new(5.0, 64.0, 0.0),
            crate::entity::components::LookDirection::default(),
            0.0,
            None,
        );
        assert!(on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            1,
            2,
            61,
            Some(&local),
        ));
        let resonance = chunks.block_entities[&pos].bell_resonance.as_ref().unwrap();
        assert_eq!(resonance.resonation_ticks, 0);
        assert!(!resonance.nearby_entities.as_ref().unwrap().contains(&8));
        assert!(on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            1,
            2,
            62,
            Some(&local),
        ));
        assert!(
            chunks.block_entities[&pos]
                .bell_resonance
                .as_ref()
                .unwrap()
                .nearby_entities
                .as_ref()
                .unwrap()
                .contains(&8)
        );
    }

    #[test]
    fn bell_no_raider_ranges_tag_replacement_and_removal_are_respected() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        let mut chunks =
            world_with_block(pos, crate::world::block::first_state_of("bell").unwrap());
        chunks.block_entities.insert(
            pos,
            crate::world::block_entity::StoredBlockEntity::new(
                BlockEntityKind::Bell,
                NbtCompound::new(),
            ),
        );
        let mut entities = EntityStore::new();
        entities.spawn_living(
            1,
            EntityKind::Witch,
            crate::entity::components::Position::new(31.49, 64.5, 0.5),
            crate::entity::components::LookDirection::default(),
            0.0,
            None,
        );
        entities.set_raider_entity_types(std::collections::HashSet::new());
        assert!(on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            1,
            0,
            1,
            None
        ));
        let mut state = chunks.block_entities[&pos].bell_resonance.clone().unwrap();
        let swing = crate::world::block_entity::BellSwing {
            ticks: 5,
            direction: 0,
            shaking: true,
        };
        assert!(bell_resonance_particles(pos, Some(swing), &mut state, &entities, &[]).is_empty());
        entities.set_raider_entity_types([EntityKind::Witch].into_iter().collect());
        state.resonation_ticks = 0;
        assert!(bell_resonance_particles(pos, Some(swing), &mut state, &entities, &[]).is_empty());
        assert!(state.resonating);
        state.resonation_ticks = 40;
        let out = bell_resonance_particles(pos, Some(swing), &mut state, &entities, &[]);
        assert_eq!(out.len(), 10);
        let raider = DVec3::new(31.49, 64.5, 0.5);
        let distance = (raider.x * raider.x + raider.z * raider.z).sqrt();
        let expected_position = dvec3(0.5 + raider.x / distance, 64.5, 0.5 + raider.z / distance);
        assert_eq!(out[0].position, expected_position);
        assert!(out.last().unwrap().position == out[0].position);
        // A stale Bell BE under another block is never started; removal also
        // drops its cached references and timer.
        chunks.set_block_state(
            pos.x,
            pos.y,
            pos.z,
            crate::world::block::first_state_of("stone").unwrap(),
        );
        assert!(!on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            1,
            0,
            2,
            None,
        ));
        chunks.block_entities.remove(&pos);
        assert!(!on_bell_block_event(
            &mut chunks,
            &entities,
            &std::collections::HashSet::new(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            1,
            0,
            2,
            None
        ));
    }

    #[test]
    fn bell_radius_checks_are_strict_and_dead_or_spectator_raiders_do_not_resonate() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        let mut entities = EntityStore::new();
        entities.spawn_living(
            1,
            EntityKind::Pillager,
            crate::entity::components::Position::new(32.5, 64.5, 0.5),
            crate::entity::components::LookDirection::default(),
            0.0,
            None,
        );
        entities.set_raider_entity_types([EntityKind::Pillager].into_iter().collect());
        let mut state = BellResonanceState {
            nearby_entities: Some(vec![1]),
            ..Default::default()
        };
        let swing = crate::world::block_entity::BellSwing {
            ticks: 5,
            direction: 0,
            shaking: true,
        };
        bell_resonance_particles(pos, Some(swing), &mut state, &entities, &[]);
        assert!(!state.resonating); // exactly 32 from block center

        entities.living.get_mut(&1).unwrap().position =
            crate::entity::components::Position::new(31.5, 64.5, 0.5);
        state.resonation_ticks = 0;
        bell_resonance_particles(pos, Some(swing), &mut state, &entities, &[]);
        assert!(state.resonating);
        entities.living.get_mut(&1).unwrap().health = 0.0;
        state.resonating = false;
        state.resonation_ticks = 0;
        bell_resonance_particles(pos, Some(swing), &mut state, &entities, &[]);
        assert!(!state.resonating);

        entities.living.get_mut(&1).unwrap().health = 20.0;
        entities.living.get_mut(&1).unwrap().position =
            crate::entity::components::Position::new(48.5, 64.5, 0.5);
        state.resonation_ticks = 40;
        state.resonating = true;
        assert!(bell_resonance_particles(pos, Some(swing), &mut state, &entities, &[]).is_empty());
        assert!(!state.resonating); // exactly 48 is outside
    }

    #[test]
    fn bell_cache_uses_entity_type_identity_and_excludes_spectators_at_query_time() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        let mut chunks =
            world_with_block(pos, crate::world::block::first_state_of("bell").unwrap());
        chunks.block_entities.insert(
            pos,
            crate::world::block_entity::StoredBlockEntity::new(
                BlockEntityKind::Bell,
                NbtCompound::new(),
            ),
        );
        let mut entities = EntityStore::new();
        let spectator = uuid::Uuid::from_u64_pair(4, 5);
        entities.spawn_living(
            1,
            EntityKind::Player,
            crate::entity::components::Position::new(2.0, 64.0, 0.0),
            crate::entity::components::LookDirection::default(),
            0.0,
            Some(spectator),
        );
        entities.spawn_living(
            2,
            EntityKind::Zombie,
            crate::entity::components::Position::new(3.0, 64.0, 0.0),
            crate::entity::components::LookDirection::default(),
            0.0,
            None,
        );
        assert!(on_bell_block_event(
            &mut chunks,
            &entities,
            &[spectator].into_iter().collect(),
            pos,
            azalea_registry::builtin::BlockKind::Bell,
            1,
            0,
            100,
            None,
        ));
        assert_eq!(
            chunks.block_entities[&pos]
                .bell_resonance
                .as_ref()
                .unwrap()
                .nearby_entities,
            Some(vec![2])
        );
    }

    #[test]
    fn potent_sulfur_event_resets_eruption_timer_and_typed_geyser_request() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        let erupting = crate::world::block::find_state(
            "potent_sulfur",
            &[("potent_sulfur_state", "erupting")],
        );
        let water = crate::world::block::first_state_of("water").unwrap();
        let mut chunks = world_with_block(pos, erupting);
        chunks.set_block_state(pos.x, pos.y + 1, pos.z, water);
        chunks.block_entities.insert(
            pos,
            crate::world::block_entity::StoredBlockEntity::new(
                BlockEntityKind::PotentSulfur,
                NbtCompound::new(),
            ),
        );
        let requests = tick_block_entity_particles(&mut chunks, &[], &EntityStore::new(), 100);
        let geyser = requests
            .iter()
            .find(|r| r.kind == ServerParticleKind::Geyser)
            .unwrap();
        assert!(matches!(
            &geyser.options,
            ServerParticleOptions::Geyser { water_blocks } if *water_blocks == 1
        ));
        assert_eq!(geyser.position, dvec3(0.5, 66.0, 0.5));
        assert_eq!(
            chunks.block_entities[&pos]
                .potent_sulfur_visual
                .unwrap()
                .eruption_tick,
            Some(100)
        );
        assert!(!on_potent_sulfur_block_event(&mut chunks, pos, 1, 107));
        assert_eq!(
            chunks.block_entities[&pos]
                .potent_sulfur_visual
                .unwrap()
                .eruption_tick,
            Some(100)
        );
        assert!(on_potent_sulfur_block_event(&mut chunks, pos, 0, 107));
        assert_eq!(
            chunks.block_entities[&pos]
                .potent_sulfur_visual
                .unwrap()
                .eruption_tick,
            Some(107)
        );
        assert!(
            tick_block_entity_particles(&mut chunks, &[], &EntityStore::new(), 126)
                .iter()
                .all(|r| r.kind != ServerParticleKind::Geyser)
        );
        let reset = tick_block_entity_particles(&mut chunks, &[], &EntityStore::new(), 127);
        assert_eq!(
            reset
                .iter()
                .filter(|r| r.kind == ServerParticleKind::Geyser)
                .count(),
            1
        );
        let reset_geyser = reset
            .iter()
            .find(|r| r.kind == ServerParticleKind::Geyser)
            .unwrap();
        assert!(matches!(
            &reset_geyser.options,
            ServerParticleOptions::Geyser { water_blocks: 1 }
        ));

        let wet =
            crate::world::block::find_state("potent_sulfur", &[("potent_sulfur_state", "wet")]);
        chunks.set_block_state(pos.x, pos.y, pos.z, wet);
        let cloud = tick_block_entity_particles(&mut chunks, &[], &EntityStore::new(), 140);
        assert_eq!(
            cloud
                .iter()
                .filter(|r| r.kind == ServerParticleKind::NoxiousGasCloud)
                .count(),
            1
        );
        assert!(!cloud.iter().any(|r| r.kind == ServerParticleKind::Geyser));
    }

    #[test]
    fn position_context_matches_java_for_powder_snow_scaffolding_and_solids() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 65, 0);
        assert!(crate::world::block::noxious_gas_collision_empty(
            crate::world::block::first_state_of("powder_snow").unwrap(),
            pos,
            Some(64),
        ));
        let scaffolding = crate::world::block::find_state(
            "scaffolding",
            &[
                ("bottom", "true"),
                ("distance", "1"),
                ("waterlogged", "false"),
            ],
        );
        assert!(crate::world::block::noxious_gas_collision_empty(
            scaffolding,
            pos,
            Some(64),
        ));
        assert!(!crate::world::block::noxious_gas_collision_empty(
            crate::world::block::first_state_of("stone").unwrap(),
            pos,
            Some(64),
        ));
    }

    #[test]
    fn trial_spawner_state_selects_emission_kind_and_cooldown_count() {
        let active = trial_spawner_particles(BlockPos::new(0, 0, 0), "active", true, 1);
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].kind, ServerParticleKind::Smoke);
        assert_eq!(active[1].kind, ServerParticleKind::SoulFireFlame);
        assert!(
            active
                .iter()
                .all(|r| matches!(&r.options, ServerParticleOptions::Simple))
        );
        let cooldown = trial_spawner_particles(BlockPos::new(0, 0, 0), "cooldown", false, 20);
        assert!((20..=24).contains(&cooldown.len()));
        assert!(cooldown.iter().all(|r| r.kind == ServerParticleKind::Smoke));
        assert!(trial_spawner_particles(BlockPos::new(0, 0, 0), "inactive", false, 20).is_empty());
    }

    #[test]
    fn spawner_emits_pair_and_stops_delay_at_zero() {
        let mut state = SpawnerVisualState {
            spawn_delay: 1,
            required_player_range: 16,
            has_display_entity: true,
        };
        let requests = spawner_particles(BlockPos::new(0, 0, 0), &mut state);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].kind, ServerParticleKind::Smoke);
        assert_eq!(requests[1].kind, ServerParticleKind::Flame);
        assert!(
            requests
                .iter()
                .all(|r| matches!(&r.options, ServerParticleOptions::Simple))
        );
        assert_eq!(state.spawn_delay, 0);
        state.has_display_entity = false;
        assert!(spawner_particles(BlockPos::new(0, 0, 0), &mut state).is_empty());
    }

    #[test]
    fn vault_idle_particle_count_and_ominous_flame_follow_synced_item_state() {
        let pos = BlockPos::new(2, 4, 6);
        assert_eq!(vault_idle_particles(pos, false, false).len(), 1);
        let normal = vault_idle_particles(pos, true, false);
        assert_eq!(normal.len(), 2);
        assert_eq!(normal[0].kind, ServerParticleKind::Smoke);
        assert_eq!(normal[1].kind, ServerParticleKind::SmallFlame);
        let ominous = vault_idle_particles(pos, true, true);
        assert_eq!(ominous[1].kind, ServerParticleKind::SoulFireFlame);
        assert!(normal.iter().all(|r| r.velocity == DVec3::ZERO));
    }
}
