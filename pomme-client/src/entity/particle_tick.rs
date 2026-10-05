//! Common Java `Entity` / `LivingEntity` client-side particle helpers.
//!
//! This owns per-entity fluid-edge state. Entity events (poof/honey/drown,
//! damage, item-break) and living-effect particles have separate owners.
use std::collections::HashSet;

use azalea_core::position::{BlockPos, ChunkPos};
use azalea_registry::builtin::EntityKind;
use glam::{DVec3, dvec3};

use crate::entity::{EntityStore, LivingEntity, VehicleState};
use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
use crate::physics::aabb::Aabb;
use crate::world::block::{self, FluidKind};
use crate::world::chunk::ChunkStore;
use crate::world::particle_tick::ParticleSpawnRequest;

fn moving_bounds_intersect_cell(start: Aabb, movement: DVec3, cell: Aabb) -> bool {
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    for (start_min, start_max, cell_min, cell_max, delta) in [
        (start.min.x, start.max.x, cell.min.x, cell.max.x, movement.x),
        (start.min.y, start.max.y, cell.min.y, cell.max.y, movement.y),
        (start.min.z, start.max.z, cell.min.z, cell.max.z, movement.z),
    ] {
        if delta == 0.0 {
            if start_min >= cell_max || start_max <= cell_min {
                return false;
            }
            continue;
        }
        let (axis_enter, axis_exit) = if delta > 0.0 {
            (
                (cell_min - start_max) / delta,
                (cell_max - start_min) / delta,
            )
        } else {
            (
                (cell_max - start_min) / delta,
                (cell_min - start_max) / delta,
            )
        };
        enter = enter.max(axis_enter);
        exit = exit.min(axis_exit);
        if enter >= exit {
            return false;
        }
    }
    exit > 0.0 && enter < 1.0
}

/// PowderSnowBlock.entityInside's client-local emission. The cell list is the
/// block-contact callback set for this client tick, deduplicated like Entity's
/// visitedBlocks set; callers provide the real previous/current entity poses.
pub(super) fn powder_snow_requests(
    chunks: &ChunkStore,
    position: DVec3,
    previous_position: DVec3,
    bounds: Aabb,
    living: bool,
    affected_by_blocks: bool,
    rng: &mut fastrand::Rng,
) -> Vec<ParticleSpawnRequest> {
    if !affected_by_blocks || position.x == previous_position.x && position.z == previous_position.z
    {
        return Vec::new();
    }
    // Java PowderSnowBlock.entityInside uses:
    // `!(entity instanceof LivingEntity) || entity.getInBlockState().is(this)`.
    // Spectator is not a particle-specific condition; its usual exclusion is
    // Entity.isAffectedByBlocks()'s `!noPhysics` gate.
    if living
        && block::block_id(chunks.get_block_state(
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        )) != "powder_snow"
    {
        return Vec::new();
    }

    let previous_bounds = bounds.offset(previous_position - position);
    let swept = Aabb::new(
        previous_bounds.min.min(bounds.min),
        previous_bounds.max.max(bounds.max),
    );
    let min_x = swept.min.x.floor() as i32;
    let min_y = swept.min.y.floor() as i32;
    let min_z = swept.min.z.floor() as i32;
    let max_x = swept.max.x.ceil() as i32 - 1;
    let max_y = swept.max.y.ceil() as i32 - 1;
    let max_z = swept.max.z.ceil() as i32 - 1;
    let mut requests = Vec::new();
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                let cell = Aabb::block(x, y, z);
                if !swept.intersects(&cell)
                    || !moving_bounds_intersect_cell(
                        previous_bounds,
                        position - previous_position,
                        cell,
                    )
                    || block::block_id(chunks.get_block_state(x, y, z)) != "powder_snow"
                {
                    continue;
                }
                // Java calls RandomSource.nextBoolean separately for each
                // entityInside callback cell, after checking horizontal motion.
                if rng.bool() {
                    let unit = 0.083333336_f32;
                    let vx = (rng.f32() * 2.0 - 1.0) * unit;
                    let vz = (rng.f32() * 2.0 - 1.0) * unit;
                    requests.push(ParticleSpawnRequest {
                        kind: Kind::Snowflake,
                        options: Options::Simple,
                        position: dvec3(position.x, f64::from(y + 1), position.z),
                        velocity: dvec3(f64::from(vx), f64::from(0.05_f32), f64::from(vz)),
                        always_visible: false,
                    });
                }
            }
        }
    }
    requests
}

/// Fixed-tick adapter for GameState.player, which is not owned by EntityStore.
pub(crate) fn living_teleport_particle_requests(
    store: &EntityStore,
    id: i32,
    seed: u64,
) -> Vec<ParticleSpawnRequest> {
    let Some(entity) = store.living.get(&id) else {
        return Vec::new();
    };
    let (width, height, _) = dimensions(entity);
    teleport_particle_requests(
        DVec3::from(entity.prev_position),
        DVec3::from(entity.position),
        width,
        height,
        seed,
    )
}

pub(crate) fn local_powder_snow_requests(
    chunks: &ChunkStore,
    position: DVec3,
    previous_position: DVec3,
    bounds: Aabb,
    affected_by_blocks: bool,
    rng: &mut fastrand::Rng,
) -> Vec<ParticleSpawnRequest> {
    powder_snow_requests(
        chunks,
        position,
        previous_position,
        bounds,
        true,
        affected_by_blocks,
        rng,
    )
}

pub(crate) fn local_block_effect_particle_requests(
    chunks: &ChunkStore,
    position: DVec3,
    previous_position: DVec3,
    bounds: Aabb,
    affected_by_blocks: bool,
    on_ground: bool,
    stepping_carefully: bool,
    tick_seed: u64,
    rng: &mut fastrand::Rng,
) -> Vec<ParticleSpawnRequest> {
    if !affected_by_blocks {
        return Vec::new();
    }
    let mut requests =
        local_powder_snow_requests(chunks, position, previous_position, bounds, true, rng);
    if on_ground && !stepping_carefully {
        requests.extend(
            crate::world::particle_tick::redstone_ore_interaction_requests(
                chunks,
                BlockPos::new(
                    position.x.floor() as i32,
                    (position.y - 0.2).floor() as i32,
                    position.z.floor() as i32,
                ),
                tick_seed,
            ),
        );
    }
    requests
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct FluidContact {
    water_height: f64,
    lava_height: f64,
    eye_in_water: bool,
    eye_in_lava: bool,
}

fn fluid_height(chunks: &ChunkStore, x: i32, y: i32, z: i32, kind: FluidKind) -> Option<f64> {
    let fluid = block::fluid(chunks.get_block_state(x, y, z));
    if fluid.kind != kind {
        return None;
    }
    let same_fluid_above = block::fluid(chunks.get_block_state(x, y + 1, z)).kind == kind;
    let height = if fluid.is_source() || fluid.falling || same_fluid_above {
        1.0
    } else {
        f64::from(fluid.amount) / 9.0
    };
    Some(f64::from(y) + height)
}

/// Mirrors `EntityFluidInteraction.update`: inspect every fluid cell touched by
/// the deflated entity box, use actual fluid tops, and separately test eye
/// height. Missing surrounding chunks invalidate the sample as in Java.
fn fluid_contact(chunks: &ChunkStore, bounds: Aabb, eye_y: f64, center: DVec3) -> FluidContact {
    let box_ = bounds.deflate(0.001);
    let min_x = box_.min.x.floor() as i32;
    let min_y = box_.min.y.floor() as i32;
    let min_z = box_.min.z.floor() as i32;
    let max_x = box_.max.x.ceil() as i32 - 1;
    let max_y = box_.max.y.ceil() as i32 - 1;
    let max_z = box_.max.z.ceil() as i32 - 1;
    for cz in (min_z - 1).div_euclid(16)..=(max_z + 1).div_euclid(16) {
        for cx in (min_x - 1).div_euclid(16)..=(max_x + 1).div_euclid(16) {
            if chunks.get_chunk(&ChunkPos::new(cx, cz)).is_none() {
                return FluidContact::default();
            }
        }
    }

    let mut contact = FluidContact::default();
    let eye_x = center.x.floor() as i32;
    let eye_z = center.z.floor() as i32;
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                let fluid = block::fluid(chunks.get_block_state(x, y, z));
                let Some(top) = fluid_height(chunks, x, y, z, fluid.kind) else {
                    continue;
                };
                if top < box_.min.y {
                    continue;
                }
                let height = top - bounds.min.y;
                let eye_inside = x == eye_x && z == eye_z && eye_y >= f64::from(y) && eye_y <= top;
                match fluid.kind {
                    FluidKind::Water => {
                        contact.water_height = contact.water_height.max(height);
                        contact.eye_in_water |= eye_inside;
                    }
                    FluidKind::Lava => {
                        contact.lava_height = contact.lava_height.max(height);
                        contact.eye_in_lava |= eye_inside;
                    }
                    FluidKind::Empty => {}
                }
            }
        }
    }
    contact
}

pub(super) fn touches_water(chunks: &ChunkStore, bounds: Aabb) -> bool {
    let center = (bounds.min + bounds.max) * 0.5;
    fluid_contact(chunks, bounds, center.y, center).water_height > 0.0
}

pub(super) fn dimensions(entity: &LivingEntity) -> (f64, f64, f64) {
    let native = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let scale = entity
        .attributes
        .get("minecraft:scale")
        .copied()
        .unwrap_or(1.0);
    let mut width = f64::from(native.width) * scale;
    let mut height = f64::from(native.height) * scale;
    let mut eye = f64::from(native.eye_height) * scale;
    match entity.entity_type {
        EntityKind::Squid | EntityKind::GlowSquid if entity.is_baby => {
            width = 0.5 * scale;
            height = 0.5 * scale;
            eye = 0.37 * scale;
        }
        EntityKind::Salmon => {
            let variant_scale = match entity.variant {
                0 => 0.5,
                2 => 1.5,
                _ => 1.0,
            };
            width = f64::from(native.width) * scale * variant_scale;
            height = f64::from(native.height) * scale * variant_scale;
            eye = f64::from(native.eye_height) * scale * variant_scale;
            if entity.is_baby {
                width *= 0.5;
                height *= 0.5;
                eye *= 0.5;
            }
        }
        EntityKind::Pufferfish => {
            let variant_scale = match entity.puff_state {
                0 => 0.5,
                1 => 0.7,
                _ => 1.0,
            };
            width *= variant_scale;
            height *= variant_scale;
            eye *= variant_scale;
        }
        EntityKind::Slime | EntityKind::MagmaCube | EntityKind::SulfurCube => {
            let size = if entity.entity_type == EntityKind::SulfurCube {
                entity.sulfur_cube_size
            } else {
                i32::from(entity.slime_size)
            };
            let size = size.max(1) as f64;
            width *= size;
            height *= size;
            eye *= size;
        }
        _ if entity.is_baby => {
            width *= 0.5;
            height *= 0.5;
            eye *= 0.5;
        }
        _ => {}
    }
    if entity.entity_type == EntityKind::Shulker
        && entity.shulker_attach_face == azalea_core::direction::Direction::Down
        && entity.shulker_peek_amount > 0.0
    {
        height *= 1.0 + f64::from(entity.shulker_peek_amount);
        eye *= 1.0 + f64::from(entity.shulker_peek_amount);
    }
    if entity.pose == crate::entity::EntityPose::Sleeping {
        (0.2 * scale, 0.2 * scale, 0.2 * scale)
    } else {
        (width, height, eye)
    }
}

fn living_bounds(entity: &LivingEntity, position: DVec3, width: f64, height: f64) -> Aabb {
    if entity.entity_type != EntityKind::Shulker {
        return bounds(position, width, height);
    }
    let scale = entity
        .attributes
        .get("minecraft:scale")
        .copied()
        .unwrap_or(1.0);
    let size = scale;
    let mut result = bounds(position, size, size);
    let angle = (0.5 + f64::from(entity.shulker_peek_amount)) * std::f64::consts::PI;
    let peek = 0.5 - angle.sin() * 0.5;
    let direction = entity.shulker_attach_face.opposite().normal_vec3();
    let extension = peek * size;
    if direction.x > 0.0 {
        result.max.x += extension;
    } else if direction.x < 0.0 {
        result.min.x -= extension;
    }
    if direction.y > 0.0 {
        result.max.y += extension;
    } else if direction.y < 0.0 {
        result.min.y -= extension;
    }
    if direction.z > 0.0 {
        result.max.z += extension;
    } else if direction.z < 0.0 {
        result.min.z -= extension;
    }
    result
}

fn vehicle_dimensions(kind: EntityKind) -> (f64, f64, f64) {
    let native = azalea_entity::dimensions::EntityDimensions::from(kind);
    let width = f64::from(native.width);
    let height = f64::from(native.height);
    (width, height, f64::from(native.eye_height))
}

fn bounds(position: DVec3, width: f64, height: f64) -> Aabb {
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

pub(crate) fn teleport_particle_requests(
    previous_position: DVec3,
    position: DVec3,
    width: f64,
    height: f64,
    seed: u64,
) -> Vec<ParticleSpawnRequest> {
    let mut rng = fastrand::Rng::with_seed(seed);
    (0..128)
        .map(|index| {
            let progress = f64::from(index) / 127.0;
            let interpolated = previous_position.lerp(position, progress);
            ParticleSpawnRequest {
                kind: Kind::Portal,
                options: Options::Simple,
                position: dvec3(
                    interpolated.x + (rng.f64() - 0.5) * width * 2.0,
                    interpolated.y + rng.f64() * height,
                    interpolated.z + (rng.f64() - 0.5) * width * 2.0,
                ),
                velocity: dvec3(
                    f64::from((rng.f32() - 0.5) * 0.2_f32),
                    f64::from((rng.f32() - 0.5) * 0.2_f32),
                    f64::from((rng.f32() - 0.5) * 0.2_f32),
                ),
                always_visible: false,
            }
        })
        .collect()
}

fn push(
    out: &mut Vec<ParticleSpawnRequest>,
    kind: Kind,
    options: Options,
    position: DVec3,
    velocity: DVec3,
) {
    out.push(ParticleSpawnRequest {
        kind,
        options,
        position,
        velocity,
        always_visible: false,
    });
}

fn sprint_particle(
    out: &mut Vec<ParticleSpawnRequest>,
    chunks: &ChunkStore,
    position: DVec3,
    velocity: DVec3,
    width: f64,
    rng: &mut fastrand::Rng,
) {
    let block_pos = azalea_core::position::BlockPos::new(
        position.x.floor() as i32,
        (position.y - 0.2).floor() as i32,
        position.z.floor() as i32,
    );
    let state = chunks.get_block_state(block_pos.x, block_pos.y, block_pos.z);
    if block::is_air(state)
        || crate::world::block_entity::is_fluid_block(block::block_id(state))
        || crate::world::block_entity::is_invisible_block(block::block_id(state))
    {
        return;
    }
    let entity_block_x = position.x.floor() as i32;
    let entity_block_z = position.z.floor() as i32;
    let mut x = position.x + (rng.f64() - 0.5) * width;
    let mut z = position.z + (rng.f64() - 0.5) * width;
    if entity_block_x != block_pos.x {
        x = x.clamp(f64::from(block_pos.x), f64::from(block_pos.x + 1));
    }
    if entity_block_z != block_pos.z {
        z = z.clamp(f64::from(block_pos.z), f64::from(block_pos.z + 1));
    }
    push(
        out,
        Kind::Block,
        Options::Block(state),
        dvec3(x, position.y + 0.1, z),
        dvec3(velocity.x * -4.0, 1.5, velocity.z * -4.0),
    );
}

fn splash_particles(
    out: &mut Vec<ParticleSpawnRequest>,
    position: DVec3,
    velocity: DVec3,
    width: f64,
    rng: &mut fastrand::Rng,
) {
    let count = (1.0 + width * 20.0).ceil() as usize;
    let y = position.y.floor() + 1.0;
    for _ in 0..count {
        let x = position.x + (rng.f64() * 2.0 - 1.0) * width;
        let z = position.z + (rng.f64() * 2.0 - 1.0) * width;
        push(
            out,
            Kind::Bubble,
            Options::Simple,
            dvec3(x, y, z),
            dvec3(velocity.x, velocity.y - rng.f64() * 0.2, velocity.z),
        );
    }
    for _ in 0..count {
        let x = position.x + (rng.f64() * 2.0 - 1.0) * width;
        let z = position.z + (rng.f64() * 2.0 - 1.0) * width;
        push(out, Kind::Splash, Options::Simple, dvec3(x, y, z), velocity);
    }
}

fn tick_living(
    out: &mut Vec<ParticleSpawnRequest>,
    id: i32,
    entity: &mut LivingEntity,
    chunks: &ChunkStore,
    is_spectator: bool,
    block_effects_authoritative: bool,
) {
    let (width, height, eye_height) = dimensions(entity);
    let position = DVec3::from(entity.position);
    let contact = fluid_contact(
        chunks,
        living_bounds(entity, position, width, height),
        entity.position.y + eye_height,
        position,
    );
    let mut rng = fastrand::Rng::with_seed(
        (id as u64).wrapping_mul(0x517c_c1b7) ^ u64::from(entity.age_in_ticks),
    );

    // Entity.applyEffectsFromBlocks invokes stepOn only for grounded entities
    // affected by blocks; RedStoneOreBlock additionally suppresses crouching.
    if block_effects_authoritative && entity.on_ground && !entity.is_crouching && !is_spectator {
        let on_pos = BlockPos::new(
            position.x.floor() as i32,
            (position.y - 0.2).floor() as i32,
            position.z.floor() as i32,
        );
        out.extend(
            crate::world::particle_tick::redstone_ore_interaction_requests(
                chunks,
                on_pos,
                (id as u64).wrapping_mul(0x9e37_79b9) ^ u64::from(entity.age_in_ticks),
            ),
        );
    }

    if block_effects_authoritative {
        out.extend(powder_snow_requests(
            chunks,
            position,
            DVec3::from(entity.prev_position),
            living_bounds(entity, position, width, height),
            true,
            true,
            &mut rng,
        ));
    }

    // Entity.baseTick asks this before updating fluid interaction, so use the
    // previous tick's water/lava flags for sprint eligibility.
    let can_sprint = entity.is_sprinting
        && !entity.client_particle_state.was_touching_water
        && !entity.client_particle_state.was_touching_lava
        && !is_spectator
        && entity.pose != crate::entity::EntityPose::Crouching
        && entity.health > 0.0;
    let special_golem_sprint = entity.entity_type == EntityKind::IronGolem
        && entity.velocity.x * entity.velocity.x + entity.velocity.z * entity.velocity.z
            > 2.500000277905201e-7
        && rng.u32(0..5) == 0;
    let rabbit_never_sprints = entity.entity_type == EntityKind::Rabbit;
    if !rabbit_never_sprints && (can_sprint || special_golem_sprint) {
        sprint_particle(out, chunks, position, entity.velocity, width, &mut rng);
    }

    if entity.client_particle_state.initialized
        && !entity.client_particle_state.was_touching_water
        && contact.water_height > 0.0
        && !(entity.entity_type == EntityKind::Player && is_spectator)
    {
        splash_particles(out, position, entity.velocity, width, &mut rng);
    }
    entity.client_particle_state.was_touching_water = contact.water_height > 0.0;
    entity.client_particle_state.was_touching_lava = contact.lava_height > 0.0;
    entity.client_particle_state.eye_in_water = contact.eye_in_water;
    entity.client_particle_state.eye_in_lava = contact.eye_in_lava;
    entity.client_particle_state.initialized = true;
    entity.is_in_water = contact.water_height > 0.0;
}

fn tick_vehicle(
    out: &mut Vec<ParticleSpawnRequest>,
    id: i32,
    entity: &mut VehicleState,
    kind: EntityKind,
    chunks: &ChunkStore,
    tick: u32,
    block_effects_eligible: bool,
) {
    let position = DVec3::from(entity.position);
    if block_effects_eligible && entity.on_ground {
        let on_pos = BlockPos::new(
            position.x.floor() as i32,
            (position.y - 0.2).floor() as i32,
            position.z.floor() as i32,
        );
        out.extend(
            crate::world::particle_tick::redstone_ore_interaction_requests(
                chunks,
                on_pos,
                (id as u64).wrapping_mul(0x9e37_79b9) ^ u64::from(tick),
            ),
        );
    }
    if kind == EntityKind::ExperienceOrb {
        entity.client_particle_state.initialized = true;
        return;
    }
    let (width, height, eye_height) = vehicle_dimensions(kind);
    let contact = fluid_contact(
        chunks,
        bounds(position, width, height),
        entity.position.y + eye_height,
        position,
    );
    if entity.client_particle_state.initialized
        && !entity.client_particle_state.was_touching_water
        && contact.water_height > 0.0
    {
        let mut rng =
            fastrand::Rng::with_seed((id as u64).wrapping_mul(0x517c_c1b7) ^ u64::from(tick));
        splash_particles(out, position, entity.velocity, width, &mut rng);
    }
    entity.client_particle_state.was_touching_water = contact.water_height > 0.0;
    entity.client_particle_state.was_touching_lava = contact.lava_height > 0.0;
    entity.client_particle_state.eye_in_water = contact.eye_in_water;
    entity.client_particle_state.eye_in_lava = contact.eye_in_lava;
    entity.client_particle_state.initialized = true;
}

fn invokes_client_block_effects(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::AcaciaBoat
            | EntityKind::AcaciaChestBoat
            | EntityKind::BambooRaft
            | EntityKind::BambooChestRaft
            | EntityKind::BirchBoat
            | EntityKind::BirchChestBoat
            | EntityKind::CherryBoat
            | EntityKind::CherryChestBoat
            | EntityKind::DarkOakBoat
            | EntityKind::DarkOakChestBoat
            | EntityKind::JungleBoat
            | EntityKind::JungleChestBoat
            | EntityKind::MangroveBoat
            | EntityKind::MangroveChestBoat
            | EntityKind::OakBoat
            | EntityKind::OakChestBoat
            | EntityKind::PaleOakBoat
            | EntityKind::PaleOakChestBoat
            | EntityKind::SpruceBoat
            | EntityKind::SpruceChestBoat
            | EntityKind::Minecart
            | EntityKind::ChestMinecart
            | EntityKind::FurnaceMinecart
            | EntityKind::TntMinecart
            | EntityKind::HopperMinecart
            | EntityKind::CommandBlockMinecart
            | EntityKind::SpawnerMinecart
            | EntityKind::Arrow
            | EntityKind::SpectralArrow
            | EntityKind::Trident
            | EntityKind::Egg
            | EntityKind::Snowball
            | EntityKind::EnderPearl
            | EntityKind::ExperienceBottle
            | EntityKind::FishingBobber
            | EntityKind::LlamaSpit
            | EntityKind::Fireball
            | EntityKind::SmallFireball
            | EntityKind::DragonFireball
            | EntityKind::WitherSkull
            | EntityKind::BreezeWindCharge
            | EntityKind::WindCharge
            | EntityKind::FireworkRocket
            | EntityKind::ExperienceOrb
            | EntityKind::FallingBlock
            | EntityKind::Tnt
            | EntityKind::EndCrystal
            | EntityKind::ShulkerBullet
    )
}

/// Non-living client entities are sampled only when their Java client ticker
/// actually invokes applyEffectsFromBlocks. Use owner movement, never
/// snapshots.
pub(crate) fn powder_snow_nonliving_requests(
    store: &mut EntityStore,
    items: &crate::entity::ItemEntityStore,
    chunks: &ChunkStore,
    tick: u64,
) -> Vec<ParticleSpawnRequest> {
    let mut requests = Vec::new();
    for (&id, entity) in &mut store.vehicles {
        if let Some(kind) = entity.kind.filter(|kind| {
            *kind != EntityKind::Item
                && invokes_client_block_effects(*kind)
                && entity_affected_by_blocks(*kind, entity)
                && !store.living.contains_key(&id)
        }) {
            let (width, height, _) = vehicle_dimensions(kind);
            let position = DVec3::from(entity.position);
            requests.extend(powder_snow_requests(
                chunks,
                position,
                DVec3::from(entity.particle_prev_position),
                bounds(position, width, height),
                false,
                true,
                &mut fastrand::Rng::with_seed((id as u64).wrapping_mul(0x517c_c1b7) ^ tick),
            ));
        }
        entity.particle_prev_position = entity.position;
    }
    for (id, position, previous) in items.powder_snow_positions() {
        let position = DVec3::from(position);
        let (width, height, _) = vehicle_dimensions(EntityKind::Item);
        requests.extend(powder_snow_requests(
            chunks,
            position,
            DVec3::from(previous),
            bounds(position, width, height),
            false,
            true,
            &mut fastrand::Rng::with_seed((id as u64).wrapping_mul(0x517c_c1b7) ^ tick),
        ));
    }
    requests
}

/// Mirrors Entity.isAffectedByBlocks for the contact-producing entity kinds.
/// Removal is handled by owner-store lifecycle; ShulkerBullet overrides the
/// Java default and remains affected by blocks despite noPhysics=true.
fn entity_affected_by_blocks(kind: EntityKind, vehicle: &VehicleState) -> bool {
    match kind {
        EntityKind::ShulkerBullet => true,
        EntityKind::Arrow | EntityKind::SpectralArrow | EntityKind::Trident => !vehicle
            .projectile
            .as_ref()
            .is_some_and(|projectile| projectile.no_physics),
        _ => true,
    }
}

fn local_authoritative_mounts(
    store: &EntityStore,
    local_player: Option<crate::entity::LocalPlayerProjectileView>,
) -> HashSet<i32> {
    let Some(player) = local_player else {
        return HashSet::new();
    };
    if store
        .living
        .get(&player.entity_id)
        .is_some_and(|entity| entity.entity_type != EntityKind::Player)
    {
        return HashSet::new();
    }
    store
        .living
        .iter()
        .filter_map(|(&mount_id, mount)| {
            let first = store.vehicles.get(&mount_id)?.passengers.first()?;
            if *first != player.entity_id {
                return None;
            }
            let saddled = mount.saddled;
            let controlled = match mount.entity_type {
                EntityKind::Horse
                | EntityKind::Donkey
                | EntityKind::Mule
                | EntityKind::SkeletonHorse
                | EntityKind::ZombieHorse
                | EntityKind::Camel
                | EntityKind::Nautilus => saddled,
                EntityKind::Pig => saddled && player.holds_carrot_on_a_stick,
                EntityKind::Strider => saddled && player.holds_warped_fungus_on_a_stick,
                EntityKind::HappyGhast => {
                    !mount.happy_ghast_stays_still
                        && mount
                            .equipment
                            .get(&azalea_inventory::components::EquipmentSlot::Body)
                            .is_some_and(|stack| !stack.is_empty())
                }
                _ => false,
            };
            controlled.then_some(mount_id)
        })
        .collect()
}

pub(super) fn tick_common(
    store: &mut EntityStore,
    chunks: &ChunkStore,
    spectator_uuids: &HashSet<uuid::Uuid>,
    local_player: Option<crate::entity::LocalPlayerProjectileView>,
) -> Vec<ParticleSpawnRequest> {
    let mut requests = Vec::new();
    let local_mounts = local_authoritative_mounts(store, local_player);
    let local_player_id = local_player.map(|player| player.entity_id);
    for (&id, entity) in &mut store.living {
        let spectator = entity.entity_type == EntityKind::Player
            && entity
                .player_uuid
                .is_some_and(|uuid| spectator_uuids.contains(&uuid));
        let block_effects_authoritative = Some(id) != local_player_id && local_mounts.contains(&id);
        tick_living(
            &mut requests,
            id,
            entity,
            chunks,
            spectator,
            block_effects_authoritative,
        );
    }
    for (&id, entity) in &mut store.vehicles {
        if let Some(kind) = entity.kind {
            let block_effects_eligible = !store.living.contains_key(&id)
                && invokes_client_block_effects(kind)
                && entity_affected_by_blocks(kind, entity);
            tick_vehicle(
                &mut requests,
                id,
                entity,
                kind,
                chunks,
                entity.projectile_age,
                block_effects_eligible,
            );
        }
    }
    requests
}

#[cfg(test)]
mod tests {
    use azalea_core::position::ChunkPos;

    use super::*;
    use crate::entity::MetaValue;
    use crate::entity::components::{LookDirection, Position};

    fn world() -> ChunkStore {
        let mut chunks = ChunkStore::new(1);
        for x in -1..=1 {
            for z in -1..=1 {
                chunks
                    .load_decoded_chunk(ChunkPos::new(x, z), azalea_world::chunk::Chunk::default());
            }
        }
        chunks
    }

    fn spawn(store: &mut EntityStore, kind: EntityKind, pos: DVec3) {
        store.spawn_living(
            1,
            kind,
            pos.into(),
            LookDirection::default(),
            0.0,
            (kind == EntityKind::Player).then(|| uuid::Uuid::from_u128(1)),
        );
    }

    fn powder_snow_world() -> ChunkStore {
        let chunks = world();
        let powder = crate::world::block::default_state_of("powder_snow").unwrap();
        chunks.set_block_state(0, 64, 0, powder);
        chunks
    }

    #[test]
    fn ore_contact_requires_local_block_effect_authority_and_includes_local_controlled_mounts() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = world();
        let ore = crate::world::block::find_state("deepslate_redstone_ore", &[("lit", "false")]);
        chunks.set_block_state(0, 64, 0, ore);
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Player, dvec3(0.5, 65.0, 0.5));
        store.living.get_mut(&1).unwrap().on_ground = true;
        store.spawn_living(
            12,
            EntityKind::Zombie,
            dvec3(0.5, 65.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.living.get_mut(&12).unwrap().on_ground = true;
        assert!(
            tick_common(&mut store, &chunks, &HashSet::new(), None).is_empty(),
            "remote player and mob do not execute client applyEffectsFromBlocks"
        );

        store.set_vehicle_kind(2, EntityKind::Horse);
        store.spawn_living(
            2,
            EntityKind::Horse,
            dvec3(0.5, 65.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.living.get_mut(&2).unwrap().saddled = true;
        store.living.get_mut(&2).unwrap().on_ground = true;
        store.set_passengers(2, &[10]);
        let player_view = crate::entity::LocalPlayerProjectileView {
            entity_id: 10,
            position: dvec3(0.5, 65.0, 0.5),
            bbox_height: 1.8,
            bounds: Aabb::from_center(dvec3(0.5, 65.0, 0.5), 0.6, 1.8),
            is_spectator: false,
            is_alive: true,
            holds_carrot_on_a_stick: false,
            holds_warped_fungus_on_a_stick: false,
        };
        let requests = store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view));
        assert_eq!(requests.iter().filter(|r| r.kind == Kind::Dust).count(), 6);
        assert!(requests.iter().all(|request| {
            request.kind == Kind::Dust
                && matches!(request.options, Options::Dust { packed_color: 0xff0000, scale } if scale == 1.0)
        }));
        store.living.get_mut(&2).unwrap().is_crouching = true;
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view)).is_empty());
        store.living.get_mut(&2).unwrap().is_crouching = false;
        store.living.get_mut(&2).unwrap().on_ground = false;
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view)).is_empty());
        store.living.get_mut(&2).unwrap().on_ground = true;
        store.living.get_mut(&2).unwrap().saddled = false;
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view)).is_empty());
        store.living.get_mut(&2).unwrap().saddled = true;
        store.spawn_living(
            11,
            EntityKind::Player,
            dvec3(0.5, 65.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            Some(uuid::Uuid::from_u128(11)),
        );
        store.set_passengers(2, &[11, 10]);
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view)).is_empty());
        store.set_passengers(2, &[10, 11]);
        assert_eq!(
            store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view))
                .iter()
                .filter(|r| r.kind == Kind::Dust)
                .count(),
            6,
            "first local player passenger controls the saddled horse"
        );
        store.remove_entity(10);
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view)).is_empty());
        store.spawn_living(
            10,
            EntityKind::Player,
            dvec3(0.5, 65.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            Some(uuid::Uuid::from_u128(10)),
        );
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player_view)).is_empty());
    }

    #[test]
    fn ore_contact_uses_java_is_affected_by_blocks_override_and_arrow_flags() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = world();
        chunks.set_block_state(
            0,
            64,
            0,
            crate::world::block::find_state("redstone_ore", &[("lit", "false")]),
        );
        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(
            2,
            Position::new(0.5, 65.0, 0.5),
            DVec3::ZERO,
            LookDirection::default(),
        );
        store.set_vehicle_kind(2, EntityKind::Arrow);
        store.set_vehicle_on_ground(2, true);
        store.apply_vehicle_metadata(2, 0, MetaValue::Byte(0x04));
        assert_eq!(
            store.client_particle_requests(&chunks, &HashSet::new(), None)
                .iter()
                .filter(|request| request.kind == Kind::Dust)
                .count(),
            6,
            "shared flag 0x04 does not disable an arrow's block effect"
        );
        store.set_projectile_metadata(2, 8, MetaValue::Byte(0x02));
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), None).is_empty());
        store.set_projectile_metadata(2, 8, MetaValue::Byte(0x00));
        assert_eq!(
            store.client_particle_requests(&chunks, &HashSet::new(), None)
                .iter()
                .filter(|request| request.kind == Kind::Dust)
                .count(),
            6
        );

        let mut bullet = EntityStore::new();
        bullet.set_vehicle_spawn_transform(
            3,
            Position::new(0.5, 65.0, 0.5),
            DVec3::ZERO,
            LookDirection::default(),
        );
        bullet.set_vehicle_kind(3, EntityKind::ShulkerBullet);
        bullet.set_vehicle_on_ground(3, true);
        assert_eq!(
            bullet
                .client_particle_requests(&chunks, &HashSet::new(), None)
                .iter()
                .filter(|request| request.kind == Kind::Dust)
                .count(),
            6,
            "ShulkerBullet overrides noPhysics in isAffectedByBlocks and reaches stepOn"
        );
        bullet.remove_entity(3);
        assert!(
            bullet
                .client_particle_requests(&chunks, &HashSet::new(), None)
                .iter()
                .all(|request| request.kind != Kind::Dust),
            "removed entities are absent through owner lifecycle"
        );
    }

    #[test]
    fn redstone_ore_step_on_uses_grounded_item_entity_contact() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = world();
        chunks.set_block_state(
            0,
            64,
            0,
            crate::world::block::find_state("redstone_ore", &[("lit", "false")]),
        );
        let mut items = crate::entity::ItemEntityStore::new();
        items.spawn_item(
            3,
            uuid::Uuid::from_u128(3),
            Position::new(0.5, 65.0, 0.5),
            DVec3::ZERO,
        );
        items.teleport(3, Position::new(0.5, 65.0, 0.5), None, true);
        assert_eq!(items.block_contact_particle_requests(&chunks, 7).len(), 6);
        items.teleport(3, Position::new(0.5, 65.0, 0.5), None, false);
        assert!(items.block_contact_particle_requests(&chunks, 7).is_empty());
    }

    #[test]
    fn redstone_ore_step_on_uses_network_grounded_state_for_vehicle_entities() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = world();
        chunks.set_block_state(
            0,
            64,
            0,
            crate::world::block::find_state("redstone_ore", &[("lit", "false")]),
        );
        let mut store = EntityStore::new();
        store.set_vehicle_transform(2, Position::new(0.5, 65.0, 0.5), DVec3::ZERO);
        store.set_vehicle_kind(2, EntityKind::OakBoat);
        store.set_vehicle_on_ground(2, true);
        assert_eq!(tick_common(&mut store, &chunks, &HashSet::new(), None).len(), 6);
        store.set_vehicle_on_ground(2, false);
        assert!(tick_common(&mut store, &chunks, &HashSet::new(), None).is_empty());
    }

    #[test]
    fn living_teleport_event_emits_128_interpolated_portal_particles() {
        let start = dvec3(-3.0, 64.0, 8.0);
        let end = dvec3(21.0, 72.0, -4.0);
        let width = 0.6;
        let height = 1.95;
        let a = teleport_particle_requests(start, end, width, height, 46);
        let b = teleport_particle_requests(start, end, width, height, 46);
        assert_eq!(a.len(), 128);
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(left, right)| {
            left.kind == right.kind
                && left.position == right.position
                && left.velocity == right.velocity
                && left.always_visible == right.always_visible
        }), "seeded request values are deterministic");
        assert!(a.iter().all(|request| {
            request.kind == Kind::Portal
                && matches!(request.options, Options::Simple)
                && request.position.x >= start.x - width
                && request.position.x <= end.x + width
                && request.position.y >= start.y
                && request.position.y <= end.y + height
                && request.position.z >= end.z - width
                && request.position.z <= start.z + width
                && request.velocity.abs().cmple(dvec3(0.1, 0.1, 0.1)).all()
                && !request.always_visible
        }));
    }

    #[test]
    fn powder_snow_matches_java_living_or_nonliving_condition_and_payload() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let current = dvec3(0.5, 64.0, 0.5);
        let previous = dvec3(0.4, 64.0, 0.5);
        let bounds = Aabb::from_center(current, 0.3, 0.9);
        let seed = (0..1000)
            .find(|seed| fastrand::Rng::with_seed(*seed).bool())
            .expect("seed taking Java's emission branch");
        let mut rng = fastrand::Rng::with_seed(seed);
        let mut expected_rng = fastrand::Rng::with_seed(seed);
        let expected_emit = expected_rng.bool();
        let expected_vx = (expected_rng.f32() * 2.0 - 1.0) * 0.083333336_f32;
        let expected_vz = (expected_rng.f32() * 2.0 - 1.0) * 0.083333336_f32;
        let requests =
            powder_snow_requests(&chunks, current, previous, bounds, true, true, &mut rng);
        assert_eq!(requests.len(), usize::from(expected_emit));
        if let Some(request) = requests.first() {
            assert_eq!(request.kind, Kind::Snowflake);
            assert!(matches!(request.options, Options::Simple));
            assert_eq!(request.position, dvec3(0.5, 65.0, 0.5));
            assert_eq!(
                request.velocity,
                dvec3(
                    f64::from(expected_vx),
                    f64::from(0.05_f32),
                    f64::from(expected_vz),
                )
            );
            assert!((request.velocity.x.abs() <= f64::from(0.083333336_f32)));
            assert_eq!(request.velocity.y, f64::from(0.05_f32));
        }
        assert!(
            powder_snow_requests(
                &chunks,
                current,
                current,
                bounds,
                true,
                false,
                &mut fastrand::Rng::with_seed(4),
            )
            .is_empty(),
            "stationary callback emits nothing"
        );
        let adjacent = dvec3(1.01, 64.0, 0.5);
        let nonliving = powder_snow_requests(
            &chunks,
            adjacent,
            dvec3(0.99, 64.0, 0.5),
            Aabb::from_center(adjacent, 0.1, 0.9),
            false,
            true,
            &mut fastrand::Rng::with_seed(seed),
        );
        assert_eq!(
            nonliving.len(),
            1,
            "non-living entities take the OR branch even when inBlockState is air"
        );
        assert!(
            powder_snow_requests(
                &chunks,
                current,
                previous,
                bounds,
                true,
                false,
                &mut fastrand::Rng::with_seed(4),
            )
            .is_empty(),
            "isAffectedByBlocks caller gate excludes noPhysics entities"
        );
        assert_eq!(
            powder_snow_requests(
                &chunks,
                current,
                previous,
                bounds,
                true,
                true,
                &mut fastrand::Rng::with_seed(seed),
            )
            .len(),
            1,
            "spectator status alone is not a block-method guard"
        );
    }

    #[test]
    fn powder_snow_contact_bounds_include_edges_but_not_adjacent_or_outside_cells() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let powder = crate::world::block::default_state_of("powder_snow").unwrap();
        let current = dvec3(0.79, 64.0, 0.5);
        let previous = dvec3(0.69, 64.0, 0.5);
        // The bounding box reaches into x=1; an actually adjacent x=1 cell is
        // included only when its volume overlaps the entity contact volume.
        chunks.set_block_state(1, 64, 0, powder);
        let seed = (0..10_000)
            .find(|seed| {
                let mut rng = fastrand::Rng::with_seed(*seed);
                if !rng.bool() {
                    return false;
                }
                let _ = rng.f32();
                let _ = rng.f32();
                rng.bool()
            })
            .expect("seed emits in both overlapped cells");
        let edge = powder_snow_requests(
            &chunks,
            current,
            previous,
            Aabb::from_center(current, 0.22, 0.9),
            true,
            true,
            &mut fastrand::Rng::with_seed(seed),
        );
        assert_eq!(edge.len(), 2, "x=1 callback requires actual edge overlap");
        let outside = powder_snow_requests(
            &chunks,
            current,
            previous,
            Aabb::from_center(current, 0.1, 0.9),
            true,
            true,
            &mut fastrand::Rng::with_seed(seed),
        );
        assert_eq!(
            outside.len(),
            1,
            "same seed emits only for x=0; x=1 is outside the smaller bbox"
        );
        chunks.set_block_state(
            1,
            64,
            0,
            crate::world::block::default_state_of("air").unwrap(),
        );
        let adjacent_only = powder_snow_requests(
            &chunks,
            dvec3(1.01, 64.0, 0.5),
            dvec3(1.0, 64.0, 0.5),
            Aabb::from_center(dvec3(1.01, 64.0, 0.5), 0.025, 0.9),
            true,
            true,
            &mut fastrand::Rng::with_seed(2),
        );
        assert!(
            adjacent_only.is_empty(),
            "living entity needs getInBlockState() even when its bbox touches snow"
        );
    }

    #[test]
    fn powder_snow_random_boolean_is_per_contact_cell_and_velocity_is_bounded() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let powder = crate::world::block::default_state_of("powder_snow").unwrap();
        chunks.set_block_state(1, 64, 0, powder);
        let current = dvec3(0.8, 64.0, 0.5);
        let previous = dvec3(0.7, 64.0, 0.5);
        let bounds = Aabb::from_center(current, 0.25, 0.9);
        let seed = (0..1000)
            .find(|seed| {
                let mut rng = fastrand::Rng::with_seed(*seed);
                for _ in 0..2 {
                    if !rng.bool() {
                        return false;
                    }
                    let _ = rng.f32();
                    let _ = rng.f32();
                }
                true
            })
            .expect("seed emitting both callback cells");
        let mut rng = fastrand::Rng::with_seed(seed);
        let requests =
            powder_snow_requests(&chunks, current, previous, bounds, true, true, &mut rng);
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert_eq!(request.kind, Kind::Snowflake);
            assert!(request.velocity.x.abs() <= f64::from(0.083333336_f32));
            assert!(request.velocity.z.abs() <= f64::from(0.083333336_f32));
            assert_eq!(request.velocity.y, f64::from(0.05_f32));
        }
    }

    #[test]
    fn remote_living_entities_do_not_run_client_block_effects() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Zombie, dvec3(0.5, 64.0, 0.5));
        store.living.get_mut(&1).unwrap().prev_position = dvec3(0.4, 64.0, 0.5).into();
        let requests = tick_common(&mut store, &chunks, &HashSet::new(), None);
        assert!(
            requests.iter().all(|request| request.kind != Kind::Snowflake),
            "Java LivingEntity.tick skips applyEffectsFromBlocks for non-authoritative remote entities"
        );
    }

    #[test]
    fn local_mount_powder_snow_uses_the_same_authority_and_steering_rules() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let mut store = EntityStore::new();
        store.set_vehicle_kind(2, EntityKind::Horse);
        store.spawn_living(
            2,
            EntityKind::Horse,
            dvec3(0.5, 64.0, 0.5).into(),
            LookDirection::default(),
            0.0,
            None,
        );
        let age = (0..1000)
            .find(|age| fastrand::Rng::with_seed((2_u64).wrapping_mul(0x517c_c1b7) ^ u64::from(*age)).bool())
            .expect("seed taking Java's emission branch");
        store.living.get_mut(&2).unwrap().age_in_ticks = age;
        store.living.get_mut(&2).unwrap().prev_position = dvec3(0.4, 64.0, 0.5).into();
        store.set_passengers(2, &[10]);
        let player = crate::entity::LocalPlayerProjectileView {
            entity_id: 10,
            position: dvec3(0.5, 64.0, 0.5),
            bbox_height: 1.8,
            bounds: Aabb::from_center(dvec3(0.5, 64.0, 0.5), 0.6, 1.8),
            is_spectator: false,
            is_alive: true,
            holds_carrot_on_a_stick: false,
            holds_warped_fungus_on_a_stick: false,
        };
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player)).is_empty());
        store.living.get_mut(&2).unwrap().saddled = true;
        let requests = store.client_particle_requests(&chunks, &HashSet::new(), Some(player));
        assert_eq!(requests.iter().filter(|r| r.kind == Kind::Snowflake).count(), 1);

        store.living.get_mut(&2).unwrap().entity_type = EntityKind::Pig;
        store.living.get_mut(&2).unwrap().prev_position = dvec3(0.5, 64.0, 0.5).into();
        store.living.get_mut(&2).unwrap().position = dvec3(0.6, 64.0, 0.5).into();
        assert!(store.client_particle_requests(&chunks, &HashSet::new(), Some(player)).is_empty());
        let pig_input = crate::entity::LocalPlayerProjectileView {
            holds_carrot_on_a_stick: true,
            ..player
        };
        assert!(local_authoritative_mounts(&store, Some(pig_input)).contains(&2));
        assert!(!local_authoritative_mounts(&store, Some(player)).contains(&2));
        store.living.get_mut(&2).unwrap().entity_type = EntityKind::Strider;
        let strider_input = crate::entity::LocalPlayerProjectileView {
            holds_warped_fungus_on_a_stick: true,
            ..player
        };
        assert!(!local_authoritative_mounts(&store, Some(player)).contains(&2));
        assert!(local_authoritative_mounts(&store, Some(strider_input)).contains(&2));
        store.living.get_mut(&2).unwrap().entity_type = EntityKind::Nautilus;
        assert!(local_authoritative_mounts(&store, Some(player)).contains(&2));
        store.living.get_mut(&2).unwrap().saddled = false;
        assert!(!local_authoritative_mounts(&store, Some(player)).contains(&2));
        store.living.get_mut(&2).unwrap().entity_type = EntityKind::HappyGhast;
        store.set_passengers(2, &[10]);
        assert!(!local_authoritative_mounts(&store, Some(player)).contains(&2));
        store.set_armor_stand_equipment(
            2,
            vec![(
                azalea_inventory::components::EquipmentSlot::Body,
                azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
                    azalea_registry::builtin::ItemKind::WhiteHarness,
                    1,
                )),
            )],
        );
        assert!(local_authoritative_mounts(&store, Some(player)).contains(&2));
        store.apply_entity_data(2, 19, MetaValue::Bool(true));
        assert!(!local_authoritative_mounts(&store, Some(player)).contains(&2));
        store.apply_entity_data(2, 19, MetaValue::Bool(false));
        assert!(local_authoritative_mounts(&store, Some(player)).contains(&2));
    }

    #[test]
    fn local_player_block_effect_gate_is_shared_by_powder_and_ore_sources() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = powder_snow_world();
        chunks.set_block_state(
            1,
            64,
            0,
            crate::world::block::find_state("redstone_ore", &[("lit", "false")]),
        );
        let powder_position = dvec3(0.5, 64.0, 0.5);
        let bounds = Aabb::from_center(powder_position, 0.6, 1.8);
        let seed = (0..1000)
            .find(|seed| fastrand::Rng::with_seed(*seed).bool())
            .expect("seed taking PowderSnow's emission branch");
        let powder = local_block_effect_particle_requests(
            &chunks,
            powder_position,
            dvec3(0.4, 64.0, 0.5),
            bounds,
            true,
            false,
            false,
            1,
            &mut fastrand::Rng::with_seed(seed),
        );
        assert_eq!(powder.len(), 1);
        assert_eq!(powder[0].kind, Kind::Snowflake);

        let ore_position = dvec3(1.5, 65.0, 0.5);
        let ore = local_block_effect_particle_requests(
            &chunks,
            ore_position,
            ore_position,
            Aabb::from_center(ore_position, 0.6, 1.8),
            true,
            true,
            false,
            2,
            &mut fastrand::Rng::with_seed(2),
        );
        assert_eq!(ore.len(), 6);
        assert!(ore.iter().all(|request| request.kind == Kind::Dust));
        assert!(local_block_effect_particle_requests(
            &chunks,
            powder_position,
            dvec3(0.4, 64.0, 0.5),
            bounds,
            false,
            true,
            false,
            3,
            &mut fastrand::Rng::with_seed(seed),
        )
        .is_empty());
        assert!(local_block_effect_particle_requests(
            &chunks,
            ore_position,
            ore_position,
            Aabb::from_center(ore_position, 0.6, 1.8),
            true,
            true,
            true,
            4,
            &mut fastrand::Rng::with_seed(4),
        )
        .is_empty());
    }

    #[test]
    fn local_player_fixed_tick_adapter_uses_same_typed_contact_source() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let current = dvec3(0.5, 64.0, 0.5);
        let bounds = Aabb::from_center(current, 0.3, 0.9);
        let seed = (0..1000)
            .find(|seed| fastrand::Rng::with_seed(*seed).bool())
            .expect("seed taking Java's 1/2 emission branch");
        let requests = local_powder_snow_requests(
            &chunks,
            current,
            dvec3(0.4, 64.0, 0.5),
            bounds,
            true,
            &mut fastrand::Rng::with_seed(seed),
        );
        assert!(
            !requests.is_empty(),
            "fixture seed takes the emitting branch"
        );
        assert!(
            requests
                .iter()
                .all(|request| request.kind == Kind::Snowflake)
        );
        assert!(
            local_powder_snow_requests(
                &chunks,
                current,
                current,
                bounds,
                true,
                &mut fastrand::Rng::with_seed(0),
            )
            .is_empty()
        );
    }

    #[test]
    fn moving_shulker_bullet_gets_powder_snow_contact_despite_no_physics() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let items = crate::entity::ItemEntityStore::new();
        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(
            9,
            crate::entity::components::Position::new(-0.2, 64.0, 0.5),
            DVec3::ZERO,
            LookDirection::default(),
        );
        store.set_vehicle_kind(9, EntityKind::ShulkerBullet);
        let tick = (0..1000)
            .find(|tick| fastrand::Rng::with_seed((9_u64).wrapping_mul(0x517c_c1b7) ^ *tick).bool())
            .expect("seed taking Java's emission branch");
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());
        store.set_vehicle_transform(
            9,
            crate::entity::components::Position::new(0.1, 64.0, 0.5),
            DVec3::ZERO,
        );
        let requests = powder_snow_nonliving_requests(&mut store, &items, &chunks, tick);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].kind, Kind::Snowflake);
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());
        store.remove_entity(9);
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());
    }

    #[test]
    fn nonliving_owner_sources_use_real_moves_and_ignore_spawn_teleport_and_removed_entities() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let mut store = EntityStore::new();
        let spawn = crate::entity::components::Position::new(0.5, 64.0, 0.5);
        store.set_vehicle_spawn_transform(2, spawn, DVec3::ZERO, LookDirection::default());
        store.set_vehicle_kind(2, EntityKind::OakBoat);
        let items = crate::entity::ItemEntityStore::new();
        let tick = (0..1000)
            .find(|tick| fastrand::Rng::with_seed((2_u64).wrapping_mul(0x517c_c1b7) ^ *tick).bool())
            .expect("seed taking Java's emission branch");
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());

        store.set_vehicle_transform(
            2,
            crate::entity::components::Position::new(0.6, 64.0, 0.5),
            DVec3::ZERO,
        );
        assert_eq!(
            powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).len(),
            1,
            "actual vehicle transform movement reaches the non-living source"
        );
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());

        store.set_vehicle_transform(3, spawn, DVec3::ZERO);
        store.set_vehicle_kind(3, EntityKind::Arrow);
        let arrow_tick = (0..1000)
            .find(|tick| fastrand::Rng::with_seed((3_u64).wrapping_mul(0x517c_c1b7) ^ *tick).bool())
            .expect("seed taking Arrow's emission branch");
        let arrow_move = |store: &mut EntityStore, x: f64| {
            store.set_vehicle_transform(
                3,
                crate::entity::components::Position::new(x, 64.0, 0.5),
                DVec3::ZERO,
            );
            powder_snow_nonliving_requests(store, &items, &chunks, arrow_tick)
        };
        store.apply_vehicle_metadata(3, 0, MetaValue::Byte(0x04));
        assert_eq!(
            arrow_move(&mut store, 0.6).len(),
            1,
            "Entity shared flag 0x04 is not AbstractArrow noPhysics"
        );
        store.set_projectile_metadata(3, 8, MetaValue::Byte(0x01));
        assert!(store.vehicles[&3].projectile.as_ref().unwrap().critical);
        assert!(!store.vehicles[&3].projectile.as_ref().unwrap().no_physics);
        assert_eq!(arrow_move(&mut store, 0.7).len(), 1, "flags 0x01 keeps crit without noPhysics");
        store.set_projectile_metadata(3, 8, MetaValue::Byte(0x02));
        assert!(!store.vehicles[&3].projectile.as_ref().unwrap().critical);
        assert!(store.vehicles[&3].projectile.as_ref().unwrap().no_physics);
        assert!(arrow_move(&mut store, 0.8).is_empty(), "flags 0x02 disables block effects");
        store.set_projectile_metadata(3, 8, MetaValue::Byte(0x03));
        assert!(store.vehicles[&3].projectile.as_ref().unwrap().critical);
        assert!(store.vehicles[&3].projectile.as_ref().unwrap().no_physics);
        assert!(arrow_move(&mut store, 0.9).is_empty(), "flags 0x03 preserve both bits");
        store.set_projectile_metadata(3, 8, MetaValue::Byte(0x01));
        assert_eq!(arrow_move(&mut store, 0.95).len(), 1, "clearing noPhysics re-enables effects");

        store.set_vehicle_teleport_transform(
            2,
            crate::entity::components::Position::new(0.7, 64.0, 0.5),
            DVec3::ZERO,
        );
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());
        store.remove_entity(2);
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());

        let mut items = crate::entity::ItemEntityStore::new();
        items.spawn_item(4, uuid::Uuid::from_u128(4), spawn, DVec3::ZERO);
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());
        items.teleport(
            4,
            crate::entity::components::Position::new(0.6, 64.0, 0.5),
            None,
            false,
        );
        assert!(
            powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty(),
            "item teleport is not invented horizontal movement"
        );
        items.set_motion(4, dvec3(0.1, 0.0, 0.0));
        items.tick(&chunks);
        let item_tick = (0..1000)
            .find(|tick| fastrand::Rng::with_seed((4_u64).wrapping_mul(0x517c_c1b7) ^ *tick).bool())
            .expect("seed taking Java's emission branch");
        assert_eq!(
            powder_snow_nonliving_requests(&mut store, &items, &chunks, item_tick).len(),
            1,
            "item physics movement reaches the actual item entity owner"
        );
        items.remove(&[4]);
        assert!(powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).is_empty());
    }

    #[test]
    fn abstract_arrow_no_physics_flags_gate_powder_for_arrow_spectral_and_trident() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = powder_snow_world();
        let items = crate::entity::ItemEntityStore::new();
        let spawn = crate::entity::components::Position::new(0.5, 64.0, 0.5);
        for (id, kind) in [
            (20, EntityKind::Arrow),
            (21, EntityKind::SpectralArrow),
            (22, EntityKind::Trident),
        ] {
            let mut store = EntityStore::new();
            store.set_vehicle_spawn_transform(id, spawn, DVec3::ZERO, LookDirection::default());
            store.set_vehicle_kind(id, kind);
            store.apply_vehicle_metadata(id, 0, MetaValue::Byte(0x04));
            let tick = (0..1000)
                .find(|tick| {
                    fastrand::Rng::with_seed((id as u64).wrapping_mul(0x517c_c1b7) ^ *tick).bool()
                })
                .expect("seed taking Java's emission branch");
            for (index, (flags, expected)) in [(0x00, 1), (0x01, 1), (0x02, 0), (0x03, 0)]
                .into_iter()
                .enumerate()
            {
                store.set_projectile_metadata(id, 8, MetaValue::Byte(flags));
                let projectile = store.vehicles[&id].projectile.as_ref().unwrap();
                assert_eq!(projectile.critical, flags & 0x01 != 0);
                assert_eq!(projectile.no_physics, flags & 0x02 != 0);
                store.set_vehicle_transform(
                    id,
                    crate::entity::components::Position::new(
                        0.6 + f64::from(index as u8) * 0.1,
                        64.0,
                        0.5,
                    ),
                    DVec3::ZERO,
                );
                assert_eq!(
                    powder_snow_nonliving_requests(&mut store, &items, &chunks, tick).len(),
                    expected,
                    "{kind:?} ID_FLAGS {flags:#04x}, shared flag 0x04"
                );
            }
            store.remove_entity(id);
            store.set_vehicle_spawn_transform(id, spawn, DVec3::ZERO, LookDirection::default());
            store.set_vehicle_kind(id, kind);
            let projectile = store.vehicles[&id].projectile.as_ref().unwrap();
            assert!(!projectile.no_physics && !projectile.critical, "entity ID reuse resets arrow flags");
        }
    }

    #[test]
    fn sprint_particle_uses_synced_flag_ground_block_native_state_and_velocity() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = world();
        let state = crate::world::block::default_state_of("stone").unwrap();
        chunks.set_block_state(0, 0, 0, state);
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Zombie, dvec3(0.5, 1.0, 0.5));
        store.set_living_motion(1, dvec3(0.25, 0.0, -0.5));
        store.apply_entity_data(1, 0, MetaValue::Byte(0x08));
        let requests = store.client_particle_requests(&chunks, &HashSet::new(), None);
        let [request] = requests.as_slice() else {
            panic!("one sprint block request")
        };
        assert_eq!(request.kind, Kind::Block);
        assert!(matches!(request.options, Options::Block(block) if block == state));
        assert_eq!(request.position.y, 1.1);
        assert_eq!(request.velocity, dvec3(-1.0, 1.5, 2.0));
    }

    #[test]
    fn water_entry_uses_full_bbox_and_emits_java_splash_count_once() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = world();
        // Zombie at x=.9 has its center in column 0, while its bbox overlaps
        // this source column at x=1: point-sampling would miss the entry.
        let water = crate::world::block::default_state_of("water").unwrap();
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Zombie, dvec3(0.9, 1.0, 0.5));
        let spectators = HashSet::new();
        assert!(
            store
                .client_particle_requests(&chunks, &spectators, None)
                .is_empty()
        );
        chunks.set_block_state(1, 1, 0, water);
        let requests = store.client_particle_requests(&chunks, &spectators, None);
        let width =
            f64::from(azalea_entity::dimensions::EntityDimensions::from(EntityKind::Zombie).width);
        let count = (1.0 + width * 20.0).ceil() as usize;
        assert_eq!(requests.len(), count * 2);
        assert_eq!(
            requests.iter().filter(|p| p.kind == Kind::Bubble).count(),
            count
        );
        assert_eq!(
            requests.iter().filter(|p| p.kind == Kind::Splash).count(),
            count
        );
        assert!(
            store
                .client_particle_requests(&chunks, &spectators, None)
                .is_empty()
        );
    }

    #[test]
    fn dynamic_dimensions_follow_synced_scale_and_fish_variants() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Pufferfish, DVec3::ZERO);
        store.apply_entity_data(1, 17, MetaValue::Int(0));
        let base = azalea_entity::dimensions::EntityDimensions::from(EntityKind::Pufferfish);
        assert_eq!(dimensions(&store.living[&1]).0, f64::from(base.width) * 0.5);
        assert_eq!(
            dimensions(&store.living[&1]).1,
            f64::from(base.height) * 0.5
        );
        store.set_attribute(1, "minecraft:scale", 2.0);
        assert_eq!(dimensions(&store.living[&1]).0, f64::from(base.width));

        store.spawn_living(
            2,
            EntityKind::Salmon,
            DVec3::ZERO.into(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.apply_entity_data(2, 17, MetaValue::Int(0));
        let salmon = azalea_entity::dimensions::EntityDimensions::from(EntityKind::Salmon);
        assert_eq!(
            dimensions(&store.living[&2]).0,
            f64::from(salmon.width) * 0.5
        );
    }

    #[test]
    fn spectator_does_not_emit_sprint_particles_in_common_tick() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = world();
        let stone = crate::world::block::default_state_of("stone").unwrap();
        chunks.set_block_state(0, 0, 0, stone);
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Player, dvec3(0.5, 1.0, 0.5));
        store.set_living_motion(1, dvec3(0.2, 0.0, 0.0));
        store.apply_entity_data(1, 0, MetaValue::Byte(0x08));
        let spectators = HashSet::from([uuid::Uuid::from_u128(1)]);
        assert!(
            store
                .client_particle_requests(&chunks, &spectators, None)
                .is_empty()
        );
    }
}
