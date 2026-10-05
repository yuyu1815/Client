//! Client-local particle sources for nonliving entities whose lifecycle is
//! synchronized by AddEntity/metadata rather than by a living AI model.
use std::collections::{HashMap, HashSet};

use azalea_registry::builtin::EntityKind as K;
use glam::{DVec3, dvec3};

use super::{LivingEntity, MetaValue, VehicleState};
use crate::particle::{ServerParticleKind as P, ServerParticleOptions as O};
use crate::physics::aabb::Aabb;
use crate::world::chunk::ChunkStore;

#[derive(Clone, Copy, Debug)]
pub struct LocalPlayerProjectileView {
    pub entity_id: i32,
    pub position: DVec3,
    pub bbox_height: f64,
    pub bounds: Aabb,
    pub is_spectator: bool,
    pub is_alive: bool,
    pub holds_carrot_on_a_stick: bool,
    pub holds_warped_fungus_on_a_stick: bool,
}

enum PearlTraceOutcome {
    Miss,
    Hit(DVec3),
    Deflected,
}
use crate::world::particle_tick::ParticleSpawnRequest;

#[derive(Clone, Debug)]
pub(crate) struct State {
    kind: K,
    age: u32,
    waiting: bool,
    radius: f32,
    cloud_particle: P,
    cloud_options: O,
    arrow_crit: bool,
    ender_pearl_impact_emitted: bool,
    pub(crate) owner_id: Option<i32>,
    left_owner: bool,
    fangs_attack_countdown: Option<u8>,
    minecart_fuse: i32,
}

impl State {
    pub(super) fn new(kind: K) -> Self {
        Self {
            kind,
            age: 0,
            waiting: false,
            radius: 3.0,
            cloud_particle: P::EntityEffect,
            cloud_options: O::EntityEffect { color: u32::MAX },
            arrow_crit: false,
            ender_pearl_impact_emitted: false,
            owner_id: None,
            left_owner: false,
            fangs_attack_countdown: None,
            minecart_fuse: -1,
        }
    }
}

fn gaussian(rng: &mut fastrand::Rng) -> f64 {
    let u = (1.0 - rng.f64()).max(f64::MIN_POSITIVE);
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * rng.f64()).cos()
}

fn push(out: &mut Vec<ParticleSpawnRequest>, kind: P, pos: DVec3, velocity: DVec3) {
    out.push(ParticleSpawnRequest {
        kind,
        options: O::Simple,
        position: pos,
        velocity,
        always_visible: false,
    });
}

fn entity_box(position: DVec3, kind: K) -> Aabb {
    let dimensions = azalea_entity::dimensions::EntityDimensions::from(kind);
    let width = f64::from(dimensions.width);
    let height = f64::from(dimensions.height);
    Aabb::new(
        dvec3(
            position.x - width * 0.5,
            position.y,
            position.z - width * 0.5,
        ),
        dvec3(
            position.x + width * 0.5,
            position.y + height,
            position.z + width * 0.5,
        ),
    )
}

fn expand_box(bounds: Aabb, amount: f64) -> Aabb {
    Aabb::new(
        bounds.min - DVec3::splat(amount),
        bounds.max + DVec3::splat(amount),
    )
}

fn swept_box(bounds: Aabb, movement: DVec3) -> Aabb {
    Aabb::new(
        bounds.min.min(bounds.min + movement),
        bounds.max.max(bounds.max + movement),
    )
}

fn fluid_height(chunks: &ChunkStore, x: i32, y: i32, z: i32) -> Option<f64> {
    let fluid = crate::world::block::fluid(chunks.get_block_state(x, y, z));
    if fluid.kind != crate::world::block::FluidKind::Water {
        return None;
    }
    let same_above = crate::world::block::fluid(chunks.get_block_state(x, y + 1, z)).kind
        == crate::world::block::FluidKind::Water;
    Some(if fluid.is_source() || fluid.falling || same_above {
        1.0
    } else {
        f64::from(fluid.amount) / 9.0
    })
}

/// Java AbstractBoat.isUnderwater: only water whose surface is above the boat
/// top counts; source water is UNDER_WATER, flowing water is
/// UNDER_FLOWING_WATER.
fn boat_is_underwater(bounds: Aabb, chunks: &ChunkStore) -> bool {
    let max_y = bounds.max.y + 0.001;
    for x in bounds.min.x.floor() as i32..bounds.max.x.ceil() as i32 {
        for y in bounds.max.y.floor() as i32..max_y.ceil() as i32 {
            for z in bounds.min.z.floor() as i32..bounds.max.z.ceil() as i32 {
                if let Some(height) = fluid_height(chunks, x, y, z)
                    && max_y < f64::from(y) + height
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Java BubbleColumnBlock.entityInside calls the boat callback for each
/// intersected, precise bubble-column block whose block above has no collision
/// and no fluid. Each cell is retained as a distinct callback.
fn boat_above_bubble_columns(bounds: Aabb, chunks: &ChunkStore) -> Vec<(i32, i32, i32)> {
    let Some(bubble_default) = crate::world::block::default_state_of("bubble_column") else {
        return Vec::new();
    };
    let bubble_id = crate::world::block::block_id(bubble_default);
    let mut cells = Vec::new();
    let precise_bounds = bounds.deflate(1.0e-5);
    for x in precise_bounds.min.x.floor() as i32..precise_bounds.max.x.ceil() as i32 {
        for y in precise_bounds.min.y.floor() as i32..precise_bounds.max.y.ceil() as i32 {
            for z in precise_bounds.min.z.floor() as i32..precise_bounds.max.z.ceil() as i32 {
                let state = chunks.get_block_state(x, y, z);
                if crate::world::block::block_id(state) != bubble_id
                    || !precise_bounds.intersects(&Aabb::block(x, y, z))
                {
                    continue;
                }
                let above = chunks.get_block_state(x, y + 1, z);
                let above_has_empty_collision = crate::physics::block_shape::partial_shape(above)
                    .is_some_and(|shape| shape.is_empty());
                let above_has_fluid =
                    crate::world::block::fluid(above).kind != crate::world::block::FluidKind::Empty;
                if above_has_empty_collision && !above_has_fluid {
                    cells.push((x, y, z));
                }
            }
        }
    }
    cells
}

fn boat_bubble_particles(
    id: i32,
    state: &State,
    position: DVec3,
    chunks: &ChunkStore,
) -> Vec<ParticleSpawnRequest> {
    let bounds = entity_box(position, state.kind);
    if boat_is_underwater(bounds, chunks) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (x, y, z) in boat_above_bubble_columns(bounds, chunks) {
        // AbstractBoat.onAboveBubbleColumn has one shared 1/100 branch for
        // its local sound and SPLASH particle, per callback/cell. Consume the
        // sound pitch draw before the two particle-position draws.
        let seed = (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ u64::from(state.age).rotate_left(19)
            ^ (x as u64).rotate_left(7)
            ^ (y as u64).rotate_left(23)
            ^ (z as u64).rotate_left(41);
        let mut rng = fastrand::Rng::with_seed(seed);
        if rng.u32(0..100) == 0 {
            let _sound_pitch = rng.f64();
            push(
                &mut out,
                P::Splash,
                position + dvec3(rng.f64(), 0.7, rng.f64()),
                DVec3::ZERO,
            );
        }
    }
    out
}

fn root_vehicle(id: i32, vehicle_of: &HashMap<i32, i32>) -> i32 {
    let mut root = id;
    let mut hops = 0;
    while let Some(&parent) = vehicle_of.get(&root) {
        root = parent;
        hops += 1;
        if hops > vehicle_of.len() {
            break;
        }
    }
    root
}

fn living_box(entity: &LivingEntity) -> Aabb {
    let dimensions = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let scale = if entity.is_baby { 0.5 } else { 1.0 };
    let width = f64::from(dimensions.width) * scale;
    let height = f64::from(dimensions.height) * scale;
    let position = DVec3::from(entity.position);
    Aabb::new(
        dvec3(
            position.x - width * 0.5,
            position.y,
            position.z - width * 0.5,
        ),
        dvec3(
            position.x + width * 0.5,
            position.y + height,
            position.z + width * 0.5,
        ),
    )
}

fn vehicle_box(entity: &VehicleState) -> Option<Aabb> {
    let kind = entity.kind?;
    // Nonliving entities that override Entity.isPickable in Java. Ordinary
    // projectiles are intentionally not targets; item entities are stored by
    // another owner and do not have a collision model here.
    if !matches!(
        kind,
        K::Minecart
            | K::ChestMinecart
            | K::FurnaceMinecart
            | K::TntMinecart
            | K::HopperMinecart
            | K::CommandBlockMinecart
            | K::SpawnerMinecart
            | K::Tnt
            | K::FallingBlock
            | K::EndCrystal
            | K::Interaction
            | K::ShulkerBullet
            | K::OakBoat
            | K::SpruceBoat
            | K::BirchBoat
            | K::JungleBoat
            | K::AcaciaBoat
            | K::DarkOakBoat
            | K::MangroveBoat
            | K::CherryBoat
            | K::PaleOakBoat
            | K::BambooRaft
            | K::OakChestBoat
            | K::SpruceChestBoat
            | K::BirchChestBoat
            | K::JungleChestBoat
            | K::AcaciaChestBoat
            | K::DarkOakChestBoat
            | K::MangroveChestBoat
            | K::CherryChestBoat
            | K::PaleOakChestBoat
            | K::BambooChestRaft
            | K::ItemFrame
            | K::GlowItemFrame
            | K::Painting
    ) {
        return None;
    }
    if kind == K::ArmorStand && entity.armor_stand_flags & 0x10 != 0 {
        return None;
    }
    Some(entity_box(DVec3::from(entity.position), kind))
}

fn line_aabb_hit(from: DVec3, to: DVec3, bounds: Aabb) -> Option<DVec3> {
    if bounds.contains(from) {
        // Java ProjectileUtil only treats an inside-origin hit as selectable
        // for entities overriding canBePickedFromInside; tracked living and
        // ordinary vehicle targets do not opt into it.
        return None;
    }
    let delta = to - from;
    let mut low: f64 = 0.0;
    let mut high: f64 = 1.0;
    for (start, step, min, max) in [
        (from.x, delta.x, bounds.min.x, bounds.max.x),
        (from.y, delta.y, bounds.min.y, bounds.max.y),
        (from.z, delta.z, bounds.min.z, bounds.max.z),
    ] {
        if step.abs() < 1.0e-12 {
            if start < min || start > max {
                return None;
            }
            continue;
        }
        let a = (min - start) / step;
        let b = (max - start) / step;
        low = low.max(a.min(b));
        high = high.min(a.max(b));
        if low > high {
            return None;
        }
    }
    Some(from + delta * low)
}

fn owner_group_intersects(
    owner_id: i32,
    projectile_sweep: Aabb,
    living: &HashMap<i32, LivingEntity>,
    vehicles: &HashMap<i32, VehicleState>,
    vehicle_of: &HashMap<i32, i32>,
    local_player: Option<LocalPlayerProjectileView>,
    spectator_uuids: &HashSet<uuid::Uuid>,
) -> bool {
    let root = root_vehicle(owner_id, vehicle_of);
    living.iter().any(|(&id, entity)| {
        entity.health > 0.0
            && entity.entity_type != K::EnderDragon
            && !(entity.entity_type == K::Player
                && entity
                    .player_uuid
                    .is_some_and(|uuid| spectator_uuids.contains(&uuid)))
            && root_vehicle(id, vehicle_of) == root
            && projectile_sweep.intersects(&living_box(entity))
    }) || vehicles.iter().any(|(&id, entity)| {
        root_vehicle(id, vehicle_of) == root
            && vehicle_box(entity).is_some_and(|bounds| projectile_sweep.intersects(&bounds))
    }) || local_player.is_some_and(|player| {
        player.is_alive
            && !player.is_spectator
            && root_vehicle(player.entity_id, vehicle_of) == root
            && projectile_sweep.intersects(&player.bounds)
    })
}

/// Classifies the next client-local segment without moving the pearl. Java's
/// entity deflection result is terminal for this segment, not a transparent
/// target that lets a later wall become the hit.
fn ender_pearl_hit(
    id: i32,
    state: &mut State,
    entity: &VehicleState,
    vehicles: &HashMap<i32, VehicleState>,
    living: &HashMap<i32, LivingEntity>,
    vehicle_of: &HashMap<i32, i32>,
    chunks: &ChunkStore,
    spectator_uuids: &HashSet<uuid::Uuid>,
    local_player: Option<LocalPlayerProjectileView>,
) -> PearlTraceOutcome {
    let from = DVec3::from(entity.position);
    let box_ = entity_box(from, K::EnderPearl);
    let in_water = {
        let mut found = false;
        'cells: for x in box_.min.x.floor() as i32..box_.max.x.ceil() as i32 {
            for y in box_.min.y.floor() as i32..box_.max.y.ceil() as i32 {
                for z in box_.min.z.floor() as i32..box_.max.z.ceil() as i32 {
                    if crate::world::block::fluid(chunks.get_block_state(x, y, z)).kind
                        == crate::world::block::FluidKind::Water
                    {
                        found = true;
                        break 'cells;
                    }
                }
            }
        }
        found
    };
    let mut movement = entity.velocity - DVec3::Y * 0.03;
    movement *= if in_water { 0.8 } else { 0.99 };
    if !movement.is_finite() || movement.length_squared() < 1.0e-16 {
        return PearlTraceOutcome::Miss;
    }
    let owner_present = state.owner_id.is_some_and(|owner| {
        living.contains_key(&owner)
            || vehicles.contains_key(&owner)
            || local_player.is_some_and(|player| player.entity_id == owner)
    });
    if !state.left_owner {
        if let Some(owner) = state.owner_id {
            let sweep = expand_box(swept_box(box_, movement), 1.0);
            if !owner_present
                || !owner_group_intersects(
                    owner,
                    sweep,
                    living,
                    vehicles,
                    vehicle_of,
                    local_player,
                    spectator_uuids,
                )
            {
                state.left_owner = true;
            }
        } else {
            state.left_owner = true;
        }
    }

    let border = crate::world::border::WorldBorder::default();
    let length = movement.length();
    let block_hit = crate::player::interaction::raycast_collision(
        from,
        (movement / length).as_vec3(),
        length as f32,
        chunks,
        &border,
        box_.min.y,
        false,
    );
    let block_distance = block_hit.map_or(length, |hit| hit.hit_point.distance(from));
    let entity_end = from + movement * (block_distance / length);
    // ProjectileUtil.computeMargin: clamp((tickCount - 2) / 20, 0, 0.3).
    let target_margin = f64::from(state.age.saturating_sub(2).min(6)) / 20.0;
    let owner_root = state.owner_id.map(|owner| root_vehicle(owner, vehicle_of));
    let mut nearest_distance = f64::INFINITY;
    let mut nearest_hit = None;
    let mut nearest_deflects = false;

    for (&target_id, target) in living {
        if target_id == id
            || local_player.is_some_and(|player| player.entity_id == target_id)
            || target.health <= 0.0
            || target.entity_type == K::EnderDragon
            || (target.entity_type == K::Player
                && target
                    .player_uuid
                    .is_some_and(|uuid| spectator_uuids.contains(&uuid)))
        {
            continue;
        }
        if !state.left_owner
            && owner_root.is_some_and(|root| root_vehicle(target_id, vehicle_of) == root)
        {
            continue;
        }
        if let Some(hit) = line_aabb_hit(
            from,
            entity_end,
            expand_box(living_box(target), target_margin),
        ) {
            let distance = hit.distance_squared(from);
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_hit = Some(hit);
                nearest_deflects = target.entity_type == K::Breeze;
            }
        }
    }
    for (&target_id, target) in vehicles {
        if target_id == id {
            continue;
        }
        if !state.left_owner
            && owner_root.is_some_and(|root| root_vehicle(target_id, vehicle_of) == root)
        {
            continue;
        }
        if let Some(bounds) = vehicle_box(target)
            && let Some(hit) = line_aabb_hit(from, entity_end, expand_box(bounds, target_margin))
        {
            let distance = hit.distance_squared(from);
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_hit = Some(hit);
                nearest_deflects = false;
            }
        }
    }
    if let Some(player) = local_player
        .filter(|player| player.is_alive && !player.is_spectator && player.entity_id != id)
        && (state.left_owner
            || !owner_root.is_some_and(|root| root_vehicle(player.entity_id, vehicle_of) == root))
        && let Some(hit) = line_aabb_hit(from, entity_end, expand_box(player.bounds, target_margin))
    {
        let distance = hit.distance_squared(from);
        if distance < nearest_distance {
            nearest_distance = distance;
            nearest_hit = Some(hit);
            nearest_deflects = false;
        }
    }
    if nearest_hit.is_some() && nearest_distance.sqrt() < block_distance {
        return if nearest_deflects {
            PearlTraceOutcome::Deflected
        } else {
            PearlTraceOutcome::Hit(nearest_hit.unwrap())
        };
    }
    block_hit.map_or(PearlTraceOutcome::Miss, |hit| {
        PearlTraceOutcome::Hit(hit.hit_point)
    })
}

/// AddEntity hook. LlamaSpit's Java `recreateFromPacket` emits exactly one
/// seven-particle burst; it is not a per-tick trail.
pub(crate) fn on_spawn(
    states: &mut HashMap<i32, State>,
    id: i32,
    kind: K,
    position: DVec3,
    velocity: DVec3,
    spawn_data: Option<i32>,
) -> Vec<ParticleSpawnRequest> {
    let mut state = State::new(kind);
    if kind == K::EnderPearl {
        // Projectile AddEntity data is the owner entity id; zero is Java's
        // explicit no-owner sentinel, while unresolved nonzero ids stay Some.
        state.owner_id = spawn_data.filter(|&owner_id| owner_id != 0);
    }
    states.insert(id, state);
    if kind == K::LlamaSpit {
        return (0..7)
            .map(|i| {
                let scale = 0.4 + 0.1 * f64::from(i);
                let mut out = Vec::new();
                push(
                    &mut out,
                    P::Spit,
                    position,
                    dvec3(velocity.x * scale, velocity.y, velocity.z * scale),
                );
                out.pop().unwrap()
            })
            .collect();
    }
    Vec::new()
}

/// Preserve only metadata that drives client-local particle decisions. Indexes
/// are Java 26.2 entity-data indexes (after protocol translation).
pub(crate) fn on_metadata(state: &mut State, index: u8, value: MetaValue) {
    match (state.kind, index, value) {
        (K::AreaEffectCloud, 8, MetaValue::Float(radius)) => state.radius = radius.clamp(0.0, 32.0),
        (K::AreaEffectCloud, 9, MetaValue::Bool(waiting)) => state.waiting = waiting,
        (K::Arrow | K::SpectralArrow, 8, MetaValue::Byte(flags)) => {
            state.arrow_crit = flags & 1 != 0
        }
        _ => {}
    }
}

pub(crate) fn set_cloud_particle(state: &mut State, kind: P, options: O) -> bool {
    if state.kind != K::AreaEffectCloud {
        return false;
    }
    state.cloud_particle = kind;
    state.cloud_options = options;
    true
}

/// Render the one-shot client-local particle branch after a confirmed trace
/// hit.
fn impact_for_state(state: &mut State, id: i32, position: DVec3) -> Vec<ParticleSpawnRequest> {
    if state.kind != K::EnderPearl || state.ender_pearl_impact_emitted {
        return Vec::new();
    }
    state.ender_pearl_impact_emitted = true;
    let mut rng = fastrand::Rng::with_seed(id as u64 ^ state.age as u64);
    (0..32)
        .map(|_| {
            let y = rng.f64() * 2.0;
            let velocity = dvec3(gaussian(&mut rng), 0.0, gaussian(&mut rng));
            let mut out = Vec::new();
            push(&mut out, P::Portal, position + dvec3(0.0, y, 0.0), velocity);
            out.pop().unwrap()
        })
        .collect()
}

pub(crate) fn on_event(
    states: &mut HashMap<i32, State>,
    id: i32,
    event: u8,
    _position: DVec3,
    _width: f64,
) -> Vec<ParticleSpawnRequest> {
    let Some(state) = states.get_mut(&id) else {
        return Vec::new();
    };
    if state.kind == K::TntMinecart && event == 70 {
        state.minecart_fuse = 80;
        return Vec::new();
    }
    if state.kind != K::EvokerFangs || event != 4 {
        return Vec::new();
    }
    state.fangs_attack_countdown = Some(22);
    Vec::new()
}

/// Tick synchronized visual sources. Position/velocity and entity targets come
/// from existing client stores; this only traces the ender pearl's next
/// segment.
pub(crate) fn tick(
    states: &mut HashMap<i32, State>,
    vehicles: &HashMap<i32, VehicleState>,
    living: &HashMap<i32, LivingEntity>,
    vehicle_of: &HashMap<i32, i32>,
    chunks: &ChunkStore,
    game_time: i64,
    spectator_uuids: &HashSet<uuid::Uuid>,
    local_player: Option<LocalPlayerProjectileView>,
) -> Vec<ParticleSpawnRequest> {
    states.retain(|id, _| vehicles.contains_key(id));
    let mut out = Vec::new();
    for (&id, state) in states.iter_mut() {
        let Some(entity) = vehicles.get(&id) else {
            continue;
        };
        let pos = DVec3::from(entity.position);
        let velocity = entity.velocity;
        match state.kind {
            K::AreaEffectCloud => {
                let waiting = state.waiting;
                let mut rng = fastrand::Rng::with_seed(
                    (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ u64::from(state.age),
                );
                if waiting && rng.bool() {
                    state.age = state.age.wrapping_add(1);
                    continue;
                }
                if waiting {
                    for _ in 0..2 {
                        let angle = rng.f64() * std::f64::consts::TAU;
                        let distance = rng.f64().sqrt() * 0.2;
                        let p = pos + dvec3(angle.cos() * distance, 0.0, angle.sin() * distance);
                        let (kind, options) =
                            if state.cloud_particle == P::EntityEffect && rng.bool() {
                                (P::EntityEffect, O::EntityEffect { color: u32::MAX })
                            } else {
                                (state.cloud_particle, state.cloud_options.clone())
                            };
                        out.push(ParticleSpawnRequest {
                            kind,
                            options,
                            position: p,
                            velocity: DVec3::ZERO,
                            always_visible: true,
                        });
                    }
                } else {
                    let count =
                        (std::f32::consts::PI * state.radius * state.radius).ceil() as usize;
                    for _ in 0..count {
                        let angle = rng.f64() * std::f64::consts::TAU;
                        let radius = rng.f64().sqrt() * f64::from(state.radius);
                        let p = pos + dvec3(angle.cos() * radius, 0.0, angle.sin() * radius);
                        out.push(ParticleSpawnRequest {
                            kind: state.cloud_particle,
                            options: state.cloud_options.clone(),
                            position: p,
                            velocity: dvec3(
                                (0.5 - rng.f64()) * 0.15,
                                0.01,
                                (0.5 - rng.f64()) * 0.15,
                            ),
                            always_visible: true,
                        });
                    }
                }
            }
            K::OminousItemSpawner if game_time.rem_euclid(5) == 0 => {
                let mut rng = fastrand::Rng::with_seed((id as u64) ^ game_time as u64);
                for _ in 0..rng.u32(1..=3) {
                    let from = dvec3(
                        pos.x + 0.4 * (gaussian(&mut rng) - gaussian(&mut rng)),
                        pos.y + 0.4 * (gaussian(&mut rng) - gaussian(&mut rng)),
                        pos.z + 0.4 * (gaussian(&mut rng) - gaussian(&mut rng)),
                    );
                    push(&mut out, P::OminousSpawning, pos, from - pos);
                }
            }
            K::EyeOfEnder => {
                let mut rng = fastrand::Rng::with_seed(
                    (id as u64).wrapping_mul(0x517c_c1b7) ^ u64::from(state.age),
                );
                if crate::world::block::fluid(chunks.get_block_state(
                    pos.x.floor() as i32,
                    pos.y.floor() as i32,
                    pos.z.floor() as i32,
                ))
                .kind
                    == crate::world::block::FluidKind::Water
                {
                    for _ in 0..4 {
                        push(&mut out, P::Bubble, pos + velocity * 0.75, velocity);
                    }
                } else {
                    push(
                        &mut out,
                        P::Portal,
                        pos + velocity * 0.75
                            + dvec3(rng.f64() * 0.6 - 0.3, -0.5, rng.f64() * 0.6 - 0.3),
                        velocity,
                    );
                }
            }
            K::EnderPearl if !state.ender_pearl_impact_emitted => {
                match ender_pearl_hit(
                    id,
                    state,
                    entity,
                    vehicles,
                    living,
                    vehicle_of,
                    chunks,
                    spectator_uuids,
                    local_player,
                ) {
                    PearlTraceOutcome::Hit(hit) => out.extend(impact_for_state(state, id, hit)),
                    PearlTraceOutcome::Deflected | PearlTraceOutcome::Miss => {}
                }
            }
            K::Arrow | K::SpectralArrow if state.arrow_crit => {
                let start = DVec3::from(entity.prev_position);
                for i in 0..4 {
                    let t = f64::from(i) / 4.0;
                    push(
                        &mut out,
                        P::Crit,
                        start + velocity * t,
                        -velocity + DVec3::Y * 0.2,
                    );
                }
            }
            K::SmallFireball | K::Fireball | K::WitherSkull => {
                push(&mut out, P::Smoke, pos + DVec3::Y * 0.5, DVec3::ZERO);
            }
            K::DragonFireball => {
                out.push(ParticleSpawnRequest {
                    kind: P::DragonBreath,
                    options: O::Power { power: 1.0 },
                    position: pos + DVec3::Y * 0.5,
                    velocity: DVec3::ZERO,
                    always_visible: false,
                });
            }
            K::TntMinecart if state.minecart_fuse > 0 => {
                push(&mut out, P::Smoke, pos + DVec3::Y * 0.5, DVec3::ZERO);
                state.minecart_fuse -= 1;
            }
            K::FurnaceMinecart if entity.minecart_furnace_has_fuel => {
                let mut rng = fastrand::Rng::with_seed(
                    (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ u64::from(state.age),
                );
                if rng.u32(0..4) == 0 {
                    push(&mut out, P::LargeSmoke, pos + DVec3::Y * 0.8, DVec3::ZERO);
                }
            }
            K::OakBoat
            | K::SpruceBoat
            | K::BirchBoat
            | K::JungleBoat
            | K::AcaciaBoat
            | K::DarkOakBoat
            | K::MangroveBoat
            | K::CherryBoat
            | K::PaleOakBoat
            | K::BambooRaft
            | K::OakChestBoat
            | K::SpruceChestBoat
            | K::BirchChestBoat
            | K::JungleChestBoat
            | K::AcaciaChestBoat
            | K::DarkOakChestBoat
            | K::MangroveChestBoat
            | K::CherryChestBoat
            | K::PaleOakChestBoat
            | K::BambooChestRaft => {
                out.extend(boat_bubble_particles(id, state, pos, chunks));
            }
            K::EvokerFangs => {
                if let Some(ticks) = state.fangs_attack_countdown.as_mut() {
                    *ticks = ticks.saturating_sub(1);
                    if *ticks == 14 {
                        let mut rng = fastrand::Rng::with_seed(id as u64 ^ u64::from(state.age));
                        for _ in 0..12 {
                            let p = dvec3(
                                pos.x + (rng.f64() * 2.0 - 1.0) * 0.25,
                                pos.y + 1.05 + rng.f64(),
                                pos.z + (rng.f64() * 2.0 - 1.0) * 0.25,
                            );
                            let v = dvec3(
                                (rng.f64() * 2.0 - 1.0) * 0.3,
                                0.3 + rng.f64() * 0.3,
                                (rng.f64() * 2.0 - 1.0) * 0.3,
                            );
                            push(&mut out, P::Crit, p + DVec3::Y, v);
                        }
                        state.fangs_attack_countdown = None;
                    }
                }
            }
            _ => {}
        }
        state.age = state.age.wrapping_add(1);
    }
    out
}

#[cfg(test)]
mod tests {
    use azalea_core::position::ChunkPos;

    use super::*;
    use crate::entity::EntityStore;
    use crate::entity::components::LookDirection;

    fn test_world() -> ChunkStore {
        let mut chunks = ChunkStore::new(1);
        for x in -1..=1 {
            for z in -1..=1 {
                chunks
                    .load_decoded_chunk(ChunkPos::new(x, z), azalea_world::chunk::Chunk::default());
            }
        }
        chunks
    }

    fn store(id: i32, kind: K, pos: DVec3, velocity: DVec3, data: i32) -> EntityStore {
        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(id, pos.into(), velocity, LookDirection::default());
        store.set_vehicle_kind(id, kind);
        store.set_vehicle_spawn_data(id, data);
        store
    }

    fn spawn_state(
        store: &EntityStore,
        id: i32,
        kind: K,
        states: &mut HashMap<i32, State>,
    ) -> Vec<ParticleSpawnRequest> {
        let vehicle = &store.vehicles[&id];
        on_spawn(
            states,
            id,
            kind,
            DVec3::from(vehicle.position),
            vehicle.velocity,
            vehicle.spawn_data,
        )
    }

    fn requests(
        states: &mut HashMap<i32, State>,
        store: &EntityStore,
        chunks: &ChunkStore,
        game_time: i64,
    ) -> Vec<ParticleSpawnRequest> {
        requests_with_player(states, store, chunks, game_time, &HashSet::new(), None)
    }

    fn requests_with_player(
        states: &mut HashMap<i32, State>,
        store: &EntityStore,
        chunks: &ChunkStore,
        game_time: i64,
        spectators: &HashSet<uuid::Uuid>,
        local_player: Option<LocalPlayerProjectileView>,
    ) -> Vec<ParticleSpawnRequest> {
        tick(
            states,
            &store.vehicles,
            &store.living,
            &store.vehicle_of,
            chunks,
            game_time,
            spectators,
            local_player,
        )
    }

    #[test]
    fn llama_spit_emits_seven_packet_spawn_particles_with_velocity_scale() {
        let mut states = HashMap::new();
        let particles = on_spawn(
            &mut states,
            2,
            K::LlamaSpit,
            DVec3::ZERO,
            DVec3::X,
            Some(123),
        );
        assert_eq!(particles.len(), 7);
        for (index, particle) in particles.iter().enumerate() {
            assert_eq!(particle.kind, P::Spit);
            assert_eq!(particle.velocity, DVec3::X * (0.4 + 0.1 * index as f64));
        }
    }

    #[test]
    fn area_cloud_radius_and_waiting_metadata_drive_particle_phase() {
        let mut states = HashMap::new();
        on_spawn(
            &mut states,
            3,
            K::AreaEffectCloud,
            DVec3::ZERO,
            DVec3::ZERO,
            None,
        );
        on_metadata(states.get_mut(&3).unwrap(), 8, MetaValue::Float(0.5));
        assert!(set_cloud_particle(
            states.get_mut(&3).unwrap(),
            P::Dust,
            O::Dust {
                packed_color: 0xff12_3456u32 as i32,
                scale: 1.25,
            },
        ));
        let entities = store(3, K::AreaEffectCloud, DVec3::ZERO, DVec3::ZERO, 0);
        let particles = requests(&mut states, &entities, &test_world(), 1);
        assert_eq!(particles.len(), 1);
        assert!(particles.iter().all(|p| p.kind == P::Dust
            && matches!(&p.options, O::Dust { packed_color, scale } if *packed_color == 0xff12_3456u32 as i32 && *scale == 1.25)
            && p.always_visible));

        on_metadata(states.get_mut(&3).unwrap(), 8, MetaValue::Float(1.0));
        on_metadata(states.get_mut(&3).unwrap(), 9, MetaValue::Bool(true));
        let mut waiting_particles = Vec::new();
        for tick in 2..40 {
            waiting_particles.extend(requests(&mut states, &entities, &test_world(), tick));
        }
        assert!(!waiting_particles.is_empty());
        assert!(waiting_particles.iter().all(|p| p.kind == P::Dust));
        on_metadata(states.get_mut(&3).unwrap(), 9, MetaValue::Bool(false));
        let radius_particles = requests(&mut states, &entities, &test_world(), 41);
        assert_eq!(radius_particles.len(), 4);
    }

    #[test]
    fn fangs_event_has_a_one_shot_eight_tick_crit_phase() {
        let mut states = HashMap::new();
        on_spawn(
            &mut states,
            4,
            K::EvokerFangs,
            DVec3::ZERO,
            DVec3::ZERO,
            None,
        );
        assert!(on_event(&mut states, 4, 4, DVec3::ZERO, 0.5).is_empty());
        let entities = store(4, K::EvokerFangs, DVec3::ZERO, DVec3::ZERO, 0);
        let chunks = test_world();
        let mut particles = Vec::new();
        for _ in 0..8 {
            particles.extend(requests(&mut states, &entities, &chunks, 0));
        }
        assert_eq!(particles.iter().filter(|p| p.kind == P::Crit).count(), 12);
        assert!(requests(&mut states, &entities, &chunks, 0).is_empty());
    }

    #[test]
    fn ominous_item_spawner_ticks_every_five_game_ticks_with_one_to_three() {
        let mut states = HashMap::new();
        on_spawn(
            &mut states,
            5,
            K::OminousItemSpawner,
            DVec3::ZERO,
            DVec3::ZERO,
            None,
        );
        let entities = store(5, K::OminousItemSpawner, DVec3::ZERO, DVec3::ZERO, 0);
        let chunks = test_world();
        assert!(requests(&mut states, &entities, &chunks, 4).is_empty());
        let particles = requests(&mut states, &entities, &chunks, 5);
        assert!((1..=3).contains(&particles.len()));
        assert!(particles.iter().all(|p| p.kind == P::OminousSpawning));
    }

    #[test]
    fn minecart_tnt_event_70_starts_exactly_80_smoke_ticks() {
        let mut states = HashMap::new();
        on_spawn(
            &mut states,
            6,
            K::TntMinecart,
            DVec3::ZERO,
            DVec3::ZERO,
            None,
        );
        let entities = store(6, K::TntMinecart, DVec3::ZERO, DVec3::ZERO, 0);
        assert!(requests(&mut states, &entities, &test_world(), 1).is_empty());
        on_event(&mut states, 6, 70, DVec3::ZERO, 0.0);
        for _ in 0..80 {
            let particles = requests(&mut states, &entities, &test_world(), 1);
            assert_eq!(particles.len(), 1);
            assert_eq!(particles[0].kind, P::Smoke);
        }
        assert!(requests(&mut states, &entities, &test_world(), 1).is_empty());
    }

    #[test]
    fn ender_pearl_block_hit_emits_once_and_a_miss_emits_nothing() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = test_world();
        let stone = crate::world::block::default_state_of("stone").unwrap();
        chunks.set_block_state(2, 64, 0, stone);
        let pearl = store(
            7,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(2.0, 0.0, 0.0),
            0,
        );
        let mut states = HashMap::new();
        spawn_state(&pearl, 7, K::EnderPearl, &mut states);
        let burst = requests(&mut states, &pearl, &chunks, 1);
        assert_eq!(burst.len(), 32);
        assert!(burst.iter().all(|particle| particle.kind == P::Portal));
        assert!(requests(&mut states, &pearl, &chunks, 2).is_empty());

        let miss_chunks = test_world();
        let miss = store(8, K::EnderPearl, dvec3(0.5, 64.5, 0.5), DVec3::X, 0);
        let mut miss_states = HashMap::new();
        spawn_state(&miss, 8, K::EnderPearl, &mut miss_states);
        assert!(requests(&mut miss_states, &miss, &miss_chunks, 1).is_empty());
        assert!(!miss_states[&8].ender_pearl_impact_emitted);
    }

    #[test]
    fn pearl_collision_ray_passes_outline_only_blocks_and_water_but_hits_solid() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = test_world();
        chunks.set_block_state(
            1,
            64,
            0,
            crate::world::block::default_state_of("short_grass").unwrap(),
        );
        chunks.set_block_state(
            2,
            64,
            0,
            crate::world::block::default_state_of("poppy").unwrap(),
        );
        chunks.set_block_state(
            3,
            64,
            0,
            crate::world::block::default_state_of("powder_snow").unwrap(),
        );
        let pearl = store(
            71,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(3.0, 0.0, 0.0),
            0,
        );
        let mut states = HashMap::new();
        spawn_state(&pearl, 71, K::EnderPearl, &mut states);
        assert!(requests(&mut states, &pearl, &chunks, 1).is_empty());
        assert!(!states[&71].ender_pearl_impact_emitted);

        let wet = test_world();
        wet.set_block_state(
            1,
            64,
            0,
            crate::world::block::default_state_of("water").unwrap(),
        );
        wet.set_block_state(
            3,
            64,
            0,
            crate::world::block::default_state_of("stone").unwrap(),
        );
        let mut pearl = store(
            72,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(4.0, 0.0, 0.0),
            0,
        );
        let mut states = HashMap::new();
        spawn_state(&pearl, 72, K::EnderPearl, &mut states);
        assert_eq!(requests(&mut states, &pearl, &wet, 1).len(), 32);

        wet.set_block_state(3, 64, 0, azalea_block::BlockState::AIR);
        pearl.vehicles.get_mut(&72).unwrap().position.x = 0.5;
        let mut states = HashMap::new();
        spawn_state(&pearl, 72, K::EnderPearl, &mut states);
        assert!(requests(&mut states, &pearl, &wet, 1).is_empty());
    }

    #[test]
    fn pearl_spectators_miss_and_external_local_player_is_a_real_target() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = test_world();
        let spectator_uuid = uuid::Uuid::from_u128(700);
        let mut entities = store(
            73,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(2.0, 0.0, 0.0),
            0,
        );
        entities.spawn_living(
            700,
            K::Player,
            dvec3(1.0, 64.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            Some(spectator_uuid),
        );
        let spectators = HashSet::from([spectator_uuid]);
        let mut states = HashMap::new();
        spawn_state(&entities, 73, K::EnderPearl, &mut states);
        assert!(
            requests_with_player(&mut states, &entities, &chunks, 1, &spectators, None).is_empty()
        );

        let pearl = store(
            74,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(2.0, 0.0, 0.0),
            0,
        );
        let player_view = LocalPlayerProjectileView {
            entity_id: 701,
            position: dvec3(1.0, 64.0, 0.5),
            bbox_height: 1.8,
            bounds: Aabb::new(dvec3(0.7, 64.0, 0.2), dvec3(1.3, 65.8, 0.8)),
            is_spectator: true,
            is_alive: true,
            holds_carrot_on_a_stick: false,
            holds_warped_fungus_on_a_stick: false,
        };
        let mut spectator_states = HashMap::new();
        spawn_state(&pearl, 74, K::EnderPearl, &mut spectator_states);
        assert!(
            requests_with_player(
                &mut spectator_states,
                &pearl,
                &chunks,
                1,
                &HashSet::new(),
                Some(player_view),
            )
            .is_empty()
        );

        let mut states = HashMap::new();
        spawn_state(&pearl, 74, K::EnderPearl, &mut states);
        assert_eq!(
            requests_with_player(
                &mut states,
                &pearl,
                &chunks,
                1,
                &HashSet::new(),
                Some(LocalPlayerProjectileView {
                    is_spectator: false,
                    ..player_view
                }),
            )
            .len(),
            32
        );
    }

    #[test]
    fn pearl_owner_is_excluded_and_breeze_deflection_stops_before_wall() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = test_world();
        let mut owned = store(
            75,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(2.0, 0.0, 0.0),
            750,
        );
        owned.spawn_living(
            750,
            K::Player,
            dvec3(1.0, 64.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            Some(uuid::Uuid::from_u128(750)),
        );
        let mut states = HashMap::new();
        spawn_state(&owned, 75, K::EnderPearl, &mut states);
        assert!(requests(&mut states, &owned, &chunks, 1).is_empty());
        assert!(!states[&75].left_owner);

        let world = test_world();
        world.set_block_state(
            3,
            64,
            0,
            crate::world::block::default_state_of("stone").unwrap(),
        );
        let mut pearl = store(
            76,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(4.0, 0.0, 0.0),
            0,
        );
        pearl.spawn_living(
            760,
            K::Breeze,
            dvec3(1.0, 64.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            None,
        );
        let mut states = HashMap::new();
        spawn_state(&pearl, 76, K::EnderPearl, &mut states);
        assert!(requests(&mut states, &pearl, &world, 1).is_empty());
        assert!(!states[&76].ender_pearl_impact_emitted);
    }

    #[test]
    fn pearl_add_entity_owner_is_preserved_and_owner_vehicle_is_ignored_until_left() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = test_world();
        let mut entities = store(
            20,
            K::EnderPearl,
            dvec3(0.5, 64.5, 0.5),
            dvec3(3.0, 0.0, 0.0),
            10,
        );
        entities.spawn_living(
            10,
            K::Player,
            dvec3(1.0, 64.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            Some(uuid::Uuid::from_u128(10)),
        );
        entities.spawn_living(
            11,
            K::Cow,
            dvec3(2.0, 64.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            None,
        );
        let mut states = HashMap::new();
        spawn_state(&entities, 20, K::EnderPearl, &mut states);
        assert_eq!(states[&20].owner_id, Some(10));
        let burst = requests(&mut states, &entities, &chunks, 1);
        assert_eq!(
            burst.len(),
            32,
            "cow behind owner is hit, but same vehicle owner is ignored"
        );

        let unknown = store(21, K::EnderPearl, dvec3(0.5, 64.0, 0.5), DVec3::X, 999);
        let mut unknown_states = HashMap::new();
        spawn_state(&unknown, 21, K::EnderPearl, &mut unknown_states);
        assert_eq!(unknown_states[&21].owner_id, Some(999));
        assert!(requests(&mut unknown_states, &unknown, &chunks, 1).is_empty());
        assert!(
            unknown_states[&21].left_owner,
            "unresolved owner acts as Java's null client owner"
        );
    }

    #[test]
    fn removed_or_expired_pearl_state_never_fabricates_an_impact() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = test_world();
        let mut entities = store(30, K::EnderPearl, dvec3(0.5, 64.5, 0.5), DVec3::X, 0);
        let mut states = HashMap::new();
        spawn_state(&entities, 30, K::EnderPearl, &mut states);
        entities.remove_entity(30);
        assert!(requests(&mut states, &entities, &chunks, 1).is_empty());
        assert!(!states.contains_key(&30));
    }

    #[test]
    fn boat_bubble_callback_requires_intersection_top_air_and_not_underwater() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = test_world();
        let bounds = entity_box(dvec3(2.5, 64.6, 2.5), K::OakBoat);
        let bubble = crate::world::block::find_state("bubble_column", &[("drag", "false")]);
        chunks.set_block_state(2, 64, 2, bubble);
        assert_eq!(boat_above_bubble_columns(bounds, &chunks), vec![(2, 64, 2)]);
        assert!(!boat_is_underwater(bounds, &chunks));
        let state = State::new(K::OakBoat);
        let mut emitted = Vec::new();
        for age in 0..200 {
            let mut at_age = state.clone();
            at_age.age = age;
            emitted.extend(boat_bubble_particles(
                40,
                &at_age,
                dvec3(2.5, 64.6, 2.5),
                &chunks,
            ));
        }
        assert!(!emitted.is_empty());
        assert!(emitted.iter().all(|particle| particle.kind == P::Splash));

        let water = crate::world::block::default_state_of("water").unwrap();
        chunks.set_block_state(2, 65, 2, water);
        assert!(
            boat_above_bubble_columns(bounds, &chunks).is_empty(),
            "fluid above selects onInsideBubbleColumn"
        );
        assert!(boat_is_underwater(bounds, &chunks));
        assert!(boat_bubble_particles(40, &state, dvec3(2.5, 64.6, 2.5), &chunks).is_empty());

        chunks.set_block_state(2, 65, 2, azalea_block::BlockState::AIR);
        chunks.set_block_state(2, 64, 2, azalea_block::BlockState::AIR);
        chunks.set_block_state(4, 64, 2, bubble);
        assert!(
            boat_above_bubble_columns(bounds, &chunks).is_empty(),
            "adjacent non-intersected column does not callback"
        );
    }
}
