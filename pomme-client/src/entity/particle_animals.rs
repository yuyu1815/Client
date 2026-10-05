//! Java 26.2 client-local particle calls owned by animal/entity tick logic.
//!
//! Server AI goals and `sendParticles` are intentionally not replayed here.
use azalea_registry::Registry;
use azalea_registry::builtin::EntityKind;
use glam::{DVec3, dvec3};

use crate::entity::{EntityStore, LivingEntity};
use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
use crate::world::chunk::ChunkStore;
use crate::world::particle_tick::ParticleSpawnRequest;

#[derive(Default)]
pub(super) struct AnimalParticleState {
    forced_age_timer: u8,
    age_lock_particle_timer: u8,
    age_lock_particle_locked: bool,
    mooshroom_stew_effects: bool,
    wolf_shake: bool,
    sniffer_digging_start: Option<u32>,
}

fn push(out: &mut Vec<ParticleSpawnRequest>, kind: Kind, position: DVec3, velocity: DVec3) {
    out.push(ParticleSpawnRequest {
        kind,
        options: Options::Simple,
        position,
        velocity,
        always_visible: false,
    });
}

fn dimensions(entity: &LivingEntity) -> (f64, f64) {
    let dimensions = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let scale = entity
        .attributes
        .get("minecraft:scale")
        .copied()
        .unwrap_or(1.0);
    let baby_scale = if entity.is_baby { 0.5 } else { 1.0 };
    (
        f64::from(dimensions.width) * scale * baby_scale,
        f64::from(dimensions.height) * scale * baby_scale,
    )
}

fn next_gaussian(rng: &mut fastrand::Rng) -> f64 {
    let u = rng.f64().max(f64::MIN_POSITIVE);
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * rng.f64()).cos()
}

fn random_xz(position: DVec3, width: f64, rng: &mut fastrand::Rng) -> (f64, f64) {
    (
        position.x + (rng.f64() - 0.5) * width,
        position.z + (rng.f64() - 0.5) * width,
    )
}

fn tick_ageable(
    out: &mut Vec<ParticleSpawnRequest>,
    id: i32,
    entity: &LivingEntity,
    state: &mut AnimalParticleState,
) {
    if state.forced_age_timer > 0 {
        if state.forced_age_timer % 4 == 0 {
            let mut rng = fastrand::Rng::with_seed(
                (id as u64).wrapping_mul(0x9e37_79b9) ^ u64::from(entity.age_in_ticks),
            );
            let (width, height) = dimensions(entity);
            let position = DVec3::from(entity.position);
            let (x, z) = random_xz(position, width, &mut rng);
            push(
                out,
                Kind::HappyVillager,
                dvec3(x, position.y + rng.f64() * height + 0.5, z),
                DVec3::ZERO,
            );
        }
        state.forced_age_timer -= 1;
    }
    if state.age_lock_particle_timer > 0 {
        if state.age_lock_particle_timer % 2 == 0 {
            let mut rng = fastrand::Rng::with_seed(
                (id as u64).wrapping_mul(0x85eb_ca6b) ^ u64::from(entity.age_in_ticks),
            );
            let (width, height) = dimensions(entity);
            let position = DVec3::from(entity.position);
            let (x, z) = random_xz(position, width, &mut rng);
            let is_locked = state.age_lock_particle_locked;
            let y_offset = if is_locked { 0.2 } else { 0.0 };
            let y = position.y + rng.f64() * height * 0.2 + height + y_offset;
            push(
                out,
                if is_locked {
                    Kind::PauseMobGrowth
                } else {
                    Kind::ResetMobGrowth
                },
                dvec3(x, y, z),
                DVec3::ZERO,
            );
        }
        state.age_lock_particle_timer -= 1;
    }
}

fn tick_bee(out: &mut Vec<ParticleSpawnRequest>, id: i32, entity: &LivingEntity) {
    // Java initializes this unsynchronized server-only counter to 0. A remote
    // Bee is created from AddEntity and only its synced nectar flag is sent.
    if entity.bee_flags & 0x08 == 0 {
        return;
    }
    let mut rng = fastrand::Rng::with_seed(
        (id as u64).wrapping_mul(0xd6e8_feb8_6659_fd93) ^ u64::from(entity.age_in_ticks),
    );
    if rng.f32() >= 0.05 {
        return;
    }
    let count = rng.u32(1..3);
    let position = DVec3::from(entity.position);
    let (_, height) = dimensions(entity);
    for _ in 0..count {
        let x = position.x - 0.3 + rng.f64() * 0.6;
        let z = position.z - 0.3 + rng.f64() * 0.6;
        push(
            out,
            Kind::FallingNectar,
            dvec3(x, position.y + height * 0.5, z),
            DVec3::ZERO,
        );
    }
}

fn tick_dolphin(out: &mut Vec<ParticleSpawnRequest>, id: i32, entity: &LivingEntity) {
    if !entity.is_in_water || entity.velocity.length_squared() <= 0.03 {
        return;
    }
    let view = entity.look_dir.as_vec().as_dvec3();
    let yaw = entity.look_dir.y_rot_rad();
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let c = f64::from(cos_yaw * 0.3);
    let s = f64::from(sin_yaw * 0.3);
    let position = DVec3::from(entity.position);
    let mut rng = fastrand::Rng::with_seed(
        (id as u64).wrapping_mul(0xc2b2_ae35) ^ u64::from(entity.age_in_ticks),
    );
    for _ in 0..2 {
        let multiplier = 1.2 - rng.f64() * 0.7;
        for sign in [1.0, -1.0] {
            let p = dvec3(
                position.x - view.x * multiplier + c * sign,
                position.y - view.y,
                position.z - view.z * multiplier + s * sign,
            );
            push(out, Kind::Dolphin, p, DVec3::ZERO);
        }
    }
}

fn tick_nautilus(out: &mut Vec<ParticleSpawnRequest>, id: i32, entity: &LivingEntity) {
    if !entity.is_in_water {
        return;
    }
    let speed = entity.velocity.length();
    let mut rng = fastrand::Rng::with_seed(
        (id as u64).wrapping_mul(0x27d4_eb2f) ^ u64::from(entity.age_in_ticks),
    );
    if rng.f32() as f64 >= (speed * 2.0).clamp(0.15, 1.0) {
        return;
    }
    let look = crate::entity::components::LookDirection::new(
        entity.look_dir.y_rot_deg(),
        entity.look_dir.x_rot_deg().clamp(-10.0, 10.0),
    )
    .as_vec()
    .as_dvec3();
    let spread = rng.f64() * 0.8 * (1.0 + speed);
    let velocity = dvec3(
        (rng.f64() - 0.5) * spread,
        (rng.f64() - 0.5) * spread,
        (rng.f64() - 0.5) * spread,
    );
    let position = DVec3::from(entity.position);
    push(
        out,
        Kind::Bubble,
        dvec3(
            position.x - look.x * 1.1,
            position.y - look.y + 0.25,
            position.z - look.z * 1.1,
        ),
        velocity,
    );
}

fn tick_sniffer(
    out: &mut Vec<ParticleSpawnRequest>,
    entity: &LivingEntity,
    state: &mut AnimalParticleState,
    chunks: &ChunkStore,
) {
    if entity.sniffer_state != 5 {
        state.sniffer_digging_start = None;
        return;
    }
    let start = *state
        .sniffer_digging_start
        .get_or_insert(entity.age_in_ticks);
    let elapsed = entity.age_in_ticks.wrapping_sub(start);
    // AnimationState.getTimeInMillis(tickCount): strict 1700ms..6000ms.
    if elapsed <= 34 || elapsed >= 120 {
        return;
    }
    let position = DVec3::from(entity.position);
    let yaw = f64::from(entity.look_dir.y_rot_rad());
    let head_x = position.x - yaw.sin() * 2.25;
    let head_z = position.z + yaw.cos() * 2.25;
    let head_block = azalea_core::position::BlockPos::new(
        head_x.floor() as i32,
        (position.y + 0.2).floor() as i32,
        head_z.floor() as i32,
    );
    let below = chunks.get_block_state(head_block.x, head_block.y - 1, head_block.z);
    if crate::world::block::is_air(below)
        || crate::world::block_entity::is_invisible_block(crate::world::block::block_id(below))
    {
        return;
    }
    let centered = dvec3(
        f64::from(head_block.x) + 0.5,
        f64::from(head_block.y) - 0.15,
        f64::from(head_block.z) + 0.5,
    );
    for _ in 0..30 {
        out.push(ParticleSpawnRequest {
            kind: Kind::Block,
            options: Options::Block(below),
            position: centered,
            velocity: DVec3::ZERO,
            always_visible: false,
        });
    }
}

fn panda_eye_height(entity: &LivingEntity) -> f64 {
    let dimensions = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let scale = entity
        .attributes
        .get("minecraft:scale")
        .copied()
        .unwrap_or(1.0);
    if entity.is_baby {
        0.28125 * scale
    } else {
        f64::from(dimensions.eye_height) * scale
    }
}

fn panda_item_options(stack: &azalea_inventory::ItemStackData) -> Options {
    Options::Item {
        item_id: stack.kind.to_u32(),
        count: stack.count,
        components: stack.component_patch.clone(),
        raw_components: None,
    }
}

fn rotate_x(v: DVec3, angle: f32) -> DVec3 {
    let (sin, cos) = angle.sin_cos();
    dvec3(
        v.x,
        v.y * f64::from(cos) + v.z * f64::from(sin),
        v.z * f64::from(cos) - v.y * f64::from(sin),
    )
}

fn rotate_y(v: DVec3, angle: f32) -> DVec3 {
    let (sin, cos) = angle.sin_cos();
    dvec3(
        v.x * f64::from(cos) + v.z * f64::from(sin),
        v.y,
        v.z * f64::from(cos) - v.x * f64::from(sin),
    )
}

fn tick_panda_eating(out: &mut Vec<ParticleSpawnRequest>, id: i32, entity: &mut LivingEntity) {
    if entity.panda_eat_counter <= 0 {
        return;
    }
    let Some(stack) = entity
        .equipment
        .get(&azalea_inventory::components::EquipmentSlot::Mainhand)
        .and_then(azalea_inventory::ItemStack::as_present)
    else {
        entity.panda_eat_counter = 0;
        return;
    };
    if entity.panda_flags & 0x08 == 0 {
        entity.panda_eat_counter = 0;
        return;
    }
    if entity.panda_eat_counter % 5 == 0 {
        let options = panda_item_options(stack);
        let mut rng = fastrand::Rng::with_seed(
            (id as u64).wrapping_mul(0xa076_1d64_78bd_642f) ^ u64::from(entity.age_in_ticks),
        );
        let x_rot = -entity.look_dir.x_rot_rad();
        let y_rot = -entity.look_dir.y_rot_rad();
        let body_y_rot = -entity.body_y_rot_deg.to_radians();
        let origin = DVec3::from(entity.position);
        for _ in 0..6 {
            let local_velocity = dvec3(
                f64::from((rng.f32() - 0.5) * 0.1),
                f64::from(rng.f32() * 0.1 + 0.1),
                f64::from((rng.f32() - 0.5) * 0.1),
            );
            let velocity = rotate_y(rotate_x(local_velocity, x_rot), y_rot) + DVec3::Y * 0.05;
            let local_position = dvec3(
                f64::from((rng.f32() - 0.5) * 0.8),
                f64::from(-rng.f32() * 0.6 - 0.3),
                f64::from(1.0 + (rng.f32() - 0.5) * 0.4),
            );
            out.push(ParticleSpawnRequest {
                kind: Kind::Item,
                options: options.clone(),
                position: origin
                    + rotate_y(local_position, body_y_rot)
                    + DVec3::Y * (panda_eye_height(entity) + 1.0),
                velocity,
                always_visible: false,
            });
        }
    }
    entity.panda_eat_counter += 1;
}

fn tick_panda(out: &mut Vec<ParticleSpawnRequest>, entity: &mut LivingEntity) {
    if entity.panda_flags & 0x02 == 0 {
        return;
    }
    entity.panda_sneeze_counter += 1;
    if entity.panda_sneeze_counter <= 20 {
        return;
    }
    let dimensions = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
    let scale = entity
        .attributes
        .get("minecraft:scale")
        .copied()
        .unwrap_or(1.0)
        * if entity.is_baby { 0.5 } else { 1.0 };
    let width = f64::from(dimensions.width) * scale;
    let eye_height = panda_eye_height(entity);
    let yaw = f64::from(entity.body_y_rot_deg.to_radians());
    let position = DVec3::from(entity.position);
    let x = position.x - (width + 1.0) * 0.5 * yaw.sin();
    let z = position.z + (width + 1.0) * 0.5 * yaw.cos();
    push(
        out,
        Kind::Sneeze,
        dvec3(x, position.y + eye_height - 0.1, z),
        dvec3(entity.velocity.x, 0.0, entity.velocity.z),
    );
    entity.panda_flags &= !0x02;
    entity.panda_sneeze_counter = 0;
}

fn tick_wolf(
    out: &mut Vec<ParticleSpawnRequest>,
    id: i32,
    entity: &mut LivingEntity,
    state: &mut AnimalParticleState,
) {
    if !state.wolf_shake || entity.health <= 0.0 {
        return;
    }
    entity.prev_shake_anim = entity.shake_anim;
    if entity.prev_shake_anim >= 2.0 {
        state.wolf_shake = false;
        entity.shake_anim = 0.0;
        entity.prev_shake_anim = 0.0;
        return;
    }
    entity.shake_anim += 0.05;
    if entity.shake_anim > 0.4 {
        let count = (((entity.shake_anim - 0.4) * std::f32::consts::PI).sin() * 7.0) as usize;
        let width = dimensions(entity).0;
        let mut rng = fastrand::Rng::with_seed(
            (id as u64).wrapping_mul(0x1656_67b1) ^ u64::from(entity.age_in_ticks),
        );
        let position = DVec3::from(entity.position);
        for _ in 0..count {
            let x = position.x + (rng.f64() * 2.0 - 1.0) * width * 0.5;
            let z = position.z + (rng.f64() * 2.0 - 1.0) * width * 0.5;
            push(
                out,
                Kind::Splash,
                dvec3(x, position.y + 0.8, z),
                entity.velocity,
            );
        }
    }
}

/// Client-only state transition for vanilla Wolf EntityEvent 8/56.
/// The parent event dispatcher should call this instead of replaying the event
/// as particles.
pub(crate) fn handle_entity_event(store: &mut EntityStore, id: i32, event_id: u8) -> bool {
    let Some(entity) = store.living.get(&id) else {
        return false;
    };
    if entity.entity_type != EntityKind::Wolf || !matches!(event_id, 8 | 56) {
        return false;
    }
    let state = store.animals_particle_state.entry(id).or_default();
    let entity = store.living.get_mut(&id).expect("checked above");
    if event_id == 8 {
        state.wolf_shake = true;
        entity.shake_anim = 0.0;
        entity.prev_shake_anim = 0.0;
    } else {
        state.wolf_shake = false;
        entity.shake_anim = 0.0;
        entity.prev_shake_anim = 0.0;
    }
    true
}

fn taming_particle_requests(
    entity: &LivingEntity,
    success: bool,
    seed: u64,
) -> Vec<ParticleSpawnRequest> {
    let (width, height) = dimensions(entity);
    let position = DVec3::from(entity.position);
    let mut rng = fastrand::Rng::with_seed(seed);
    let kind = if success { Kind::Heart } else { Kind::Smoke };
    (0..7)
        .map(|_| ParticleSpawnRequest {
            kind,
            options: Options::Simple,
            position: dvec3(
                position.x + (rng.f64() * 2.0 - 1.0) * width,
                position.y + rng.f64() * height + 0.5,
                position.z + (rng.f64() * 2.0 - 1.0) * width,
            ),
            velocity: dvec3(
                next_gaussian(&mut rng) * 0.02,
                next_gaussian(&mut rng) * 0.02,
                next_gaussian(&mut rng) * 0.02,
            ),
            always_visible: false,
        })
        .collect()
}

fn handles_animal_breeding_hearts(kind: EntityKind) -> bool {
    super::supports_horse_inventory(&kind)
        || matches!(
            kind,
            EntityKind::Pig
                | EntityKind::Cow
                | EntityKind::Mooshroom
                | EntityKind::Bee
                | EntityKind::HappyGhast
                | EntityKind::Sheep
                | EntityKind::Chicken
                | EntityKind::Wolf
                | EntityKind::Cat
                | EntityKind::Ocelot
                | EntityKind::Parrot
                | EntityKind::Rabbit
                | EntityKind::Armadillo
                | EntityKind::Axolotl
                | EntityKind::Fox
                | EntityKind::Frog
                | EntityKind::Goat
                | EntityKind::Nautilus
                | EntityKind::ZombieNautilus
                | EntityKind::Panda
                | EntityKind::PolarBear
                | EntityKind::Sniffer
                | EntityKind::Strider
                | EntityKind::Turtle
        )
}

impl EntityStore {
    pub(crate) fn animal_particle_event(
        &mut self,
        id: i32,
        event_id: u8,
    ) -> Option<Vec<ParticleSpawnRequest>> {
        let Some(entity) = self.living.get(&id) else {
            return None;
        };
        if event_id == 18 {
            let count = if entity.entity_type == EntityKind::Allay {
                3
            } else if handles_animal_breeding_hearts(entity.entity_type) {
                7
            } else {
                return None;
            };
            let (width, height) = dimensions(entity);
            let position = DVec3::from(entity.position);
            let mut rng = fastrand::Rng::with_seed(
                (id as u64).wrapping_mul(0x9e37_79b9)
                    ^ u64::from(entity.age_in_ticks)
                    ^ 0x6272_6565_6469_6e67,
            );
            return Some(
                (0..count)
                    .map(|_| ParticleSpawnRequest {
                        kind: Kind::Heart,
                        options: Options::Simple,
                        position: dvec3(
                            position.x + (rng.f64() * 2.0 - 1.0) * width,
                            position.y + rng.f64() * height + 0.5,
                            position.z + (rng.f64() * 2.0 - 1.0) * width,
                        ),
                        velocity: dvec3(
                            next_gaussian(&mut rng) * 0.02,
                            next_gaussian(&mut rng) * 0.02,
                            next_gaussian(&mut rng) * 0.02,
                        ),
                        always_visible: false,
                    })
                    .collect(),
            );
        }
        if entity.entity_type == EntityKind::Dolphin && event_id == 38 {
            let (width, height) = dimensions(entity);
            let position = DVec3::from(entity.position);
            let mut rng = fastrand::Rng::with_seed(
                (id as u64).wrapping_mul(0x9e37_79b9)
                    ^ u64::from(entity.age_in_ticks)
                    ^ 0x646f_6c70_6869_6e38,
            );
            return Some(
                (0..7)
                    .map(|_| ParticleSpawnRequest {
                        kind: Kind::HappyVillager,
                        options: Options::Simple,
                        position: dvec3(
                            position.x + (rng.f64() * 2.0 - 1.0) * width,
                            position.y + rng.f64() * height + 0.5,
                            position.z + (rng.f64() * 2.0 - 1.0) * width,
                        ),
                        velocity: dvec3(
                            next_gaussian(&mut rng) * 0.01,
                            next_gaussian(&mut rng) * 0.01,
                            next_gaussian(&mut rng) * 0.01,
                        ),
                        always_visible: false,
                    })
                    .collect(),
            );
        }
        if entity.entity_type == EntityKind::Fox && event_id == 45 {
            let Some(stack) = entity
                .equipment
                .get(&azalea_inventory::components::EquipmentSlot::Mainhand)
                .and_then(azalea_inventory::ItemStack::as_present)
                .filter(|stack| !stack.is_empty())
            else {
                return Some(Vec::new());
            };
            let position = DVec3::from(entity.position);
            let look = entity.look_dir.as_vec().as_dvec3();
            let mut rng = fastrand::Rng::with_seed(
                (id as u64).wrapping_mul(0x9e37_79b9)
                    ^ u64::from(entity.age_in_ticks)
                    ^ 0x666f_785f_6d6f_7574,
            );
            let rotation = glam::Quat::from_rotation_y(-entity.look_dir.y_rot_rad())
                * glam::Quat::from_rotation_x(-entity.look_dir.x_rot_rad());
            return Some(
                (0..8)
                    .map(|_| {
                        let raw = glam::Vec3::new(
                            (rng.f32() - 0.5) * 0.1,
                            rng.f32() * 0.1 + 0.1,
                            0.0,
                        );
                        let velocity = rotation.mul_vec3(raw);
                        ParticleSpawnRequest {
                            kind: Kind::Item,
                            options: panda_item_options(stack),
                            position: position + dvec3(look.x * 0.5, 0.0, look.z * 0.5),
                            velocity: dvec3(
                                f64::from(velocity.x),
                                f64::from(velocity.y + 0.05),
                                f64::from(velocity.z),
                            ),
                            always_visible: false,
                        }
                    })
                    .collect(),
            );
        }
        if matches!(event_id, 6 | 7)
            && matches!(
                entity.entity_type,
                EntityKind::Wolf
                    | EntityKind::Cat
                    | EntityKind::Parrot
                    | EntityKind::Nautilus
                    | EntityKind::ZombieNautilus
            )
        {
            return Some(taming_particle_requests(
                entity,
                event_id == 7,
                (id as u64).wrapping_mul(0x9e37_79b9)
                    ^ u64::from(entity.age_in_ticks)
                    ^ u64::from(event_id),
            ));
        }
        if entity.entity_type == EntityKind::Ocelot && matches!(event_id, 40 | 41) {
            return Some(taming_particle_requests(
                entity,
                event_id == 41,
                (id as u64).wrapping_mul(0x9e37_79b9)
                    ^ u64::from(entity.age_in_ticks)
                    ^ u64::from(event_id),
            ));
        }
        handle_entity_event(self, id, event_id).then(Vec::new)
    }

    /// Start vanilla AgeableMob's client-only age-up sparkle countdown after a
    /// successful forced feed interaction. Call only for the interaction that
    /// invokes Java `ageUp(seconds, true)`.
    pub(crate) fn start_age_up_particle_timer(&mut self, id: i32) -> bool {
        let Some(entity) = self.living.get(&id) else {
            return false;
        };
        if !is_ageable(entity.entity_type) || !entity.is_baby || entity.age_locked {
            return false;
        }
        let state = self.animals_particle_state.entry(id).or_default();
        if state.forced_age_timer == 0 {
            state.forced_age_timer = 40;
        }
        true
    }

    /// Start the 40-tick golden-dandelion age-lock transition particle timer
    /// after the successful interaction result; persistent metadata alone must
    /// never restart this effect.
    pub(crate) fn start_age_lock_particle_timer(&mut self, id: i32, is_locked: bool) -> bool {
        let Some(entity) = self.living.get(&id) else {
            return false;
        };
        if !is_ageable(entity.entity_type) || !entity.is_baby || entity.age_locked == is_locked {
            return false;
        }
        let state = self.animals_particle_state.entry(id).or_default();
        if state.age_lock_particle_timer > 0 {
            return false;
        }
        state.age_lock_particle_locked = is_locked;
        state.age_lock_particle_timer = 40;
        self.living.get_mut(&id).expect("checked above").age_locked = is_locked;
        true
    }

    /// Tick the direct client-local animal particle callsites represented here.
    pub(crate) fn tick_animal_particles(
        &mut self,
        _chunks: &ChunkStore,
        _game_time: i64,
    ) -> Vec<ParticleSpawnRequest> {
        let mut out = Vec::new();
        for (&id, entity) in &mut self.living {
            if is_ageable(entity.entity_type) {
                tick_ageable(
                    &mut out,
                    id,
                    entity,
                    self.animals_particle_state.entry(id).or_default(),
                );
            }
            match entity.entity_type {
                EntityKind::Bee => tick_bee(&mut out, id, entity),
                EntityKind::Dolphin => tick_dolphin(&mut out, id, entity),
                EntityKind::Nautilus | EntityKind::ZombieNautilus => {
                    tick_nautilus(&mut out, id, entity)
                }
                EntityKind::Panda => {
                    tick_panda(&mut out, entity);
                    tick_panda_eating(&mut out, id, entity);
                }
                EntityKind::Sniffer => tick_sniffer(
                    &mut out,
                    entity,
                    self.animals_particle_state.entry(id).or_default(),
                    _chunks,
                ),
                EntityKind::Wolf => tick_wolf(
                    &mut out,
                    id,
                    entity,
                    self.animals_particle_state.entry(id).or_default(),
                ),
                _ => {}
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InteractionParticle {
    AgeUpHappyVillager,
    BrownMooshroomStewRepeat,
    BrownMooshroomStewApply,
}

impl EntityStore {
    /// Java's Brown Mooshroom keeps this operation state locally even though
    /// it is not synchronized. The first effect-bearing flower applies stew
    /// effects and emits EFFECT; later flowers emit SMOKE instead.
    pub(crate) fn mooshroom_stew_item_requests(
        &mut self,
        id: i32,
        item: azalea_registry::builtin::ItemKind,
        seed: u64,
    ) -> Vec<ParticleSpawnRequest> {
        if !is_suspicious_stew_effect_item(item)
            || !self.living.get(&id).is_some_and(|entity| {
                entity.entity_type == EntityKind::Mooshroom
                    && entity.variant == 1
                    && !entity.is_baby
            })
        {
            return Vec::new();
        }
        let state = self.animals_particle_state.entry(id).or_default();
        let interaction = if state.mooshroom_stew_effects {
            InteractionParticle::BrownMooshroomStewRepeat
        } else {
            state.mooshroom_stew_effects = true;
            InteractionParticle::BrownMooshroomStewApply
        };
        self.interaction_particle_requests(id, interaction, seed)
    }

    /// Pure one-shot requests for Java interaction branches that directly call
    /// `Level.addParticle`. Call only after that branch's item/result predicate
    /// succeeds; `seed` makes the request deterministic for the caller.
    pub(crate) fn interaction_particle_requests(
        &self,
        id: i32,
        interaction: InteractionParticle,
        seed: u64,
    ) -> Vec<ParticleSpawnRequest> {
        let Some(entity) = self.living.get(&id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut rng = fastrand::Rng::with_seed(seed);
        let position = DVec3::from(entity.position);
        match interaction {
            InteractionParticle::AgeUpHappyVillager => {
                let supported = matches!(
                    entity.entity_type,
                    EntityKind::Camel
                        | EntityKind::CamelHusk
                        | EntityKind::Llama
                        | EntityKind::TraderLlama
                        | EntityKind::Tadpole
                ) || super::is_equine(&entity.entity_type);
                let age_eligible = if entity.entity_type == EntityKind::Tadpole {
                    !entity.age_locked
                } else {
                    entity.is_baby && !entity.age_locked
                };
                if !supported || !age_eligible {
                    return out;
                }
                let (width, height) = dimensions(entity);
                let (x, z) = random_xz(position, width, &mut rng);
                push(
                    &mut out,
                    Kind::HappyVillager,
                    dvec3(x, position.y + rng.f64() * height + 0.5, z),
                    DVec3::ZERO,
                );
            }
            InteractionParticle::BrownMooshroomStewRepeat
            | InteractionParticle::BrownMooshroomStewApply => {
                if entity.entity_type != EntityKind::Mooshroom
                    || entity.variant != 1
                    || entity.is_baby
                {
                    return out;
                }
                let (kind, count, options) = match interaction {
                    InteractionParticle::BrownMooshroomStewRepeat => {
                        (Kind::Smoke, 2, Options::Simple)
                    }
                    InteractionParticle::BrownMooshroomStewApply => (
                        Kind::Effect,
                        4,
                        Options::Spell {
                            color: -1,
                            power: 1.0,
                        },
                    ),
                    InteractionParticle::AgeUpHappyVillager => unreachable!(),
                };
                let (_, height) = dimensions(entity);
                for _ in 0..count {
                    out.push(ParticleSpawnRequest {
                        kind,
                        options: options.clone(),
                        position: dvec3(
                            position.x + rng.f64() * 0.5,
                            position.y + height * 0.5,
                            position.z + rng.f64() * 0.5,
                        ),
                        velocity: dvec3(0.0, rng.f64() / 5.0, 0.0),
                        always_visible: false,
                    });
                }
            }
        }
        out
    }
}

fn is_suspicious_stew_effect_item(item: azalea_registry::builtin::ItemKind) -> bool {
    matches!(
        item,
        azalea_registry::builtin::ItemKind::Dandelion
            | azalea_registry::builtin::ItemKind::GoldenDandelion
            | azalea_registry::builtin::ItemKind::Torchflower
            | azalea_registry::builtin::ItemKind::Poppy
            | azalea_registry::builtin::ItemKind::BlueOrchid
            | azalea_registry::builtin::ItemKind::Allium
            | azalea_registry::builtin::ItemKind::AzureBluet
            | azalea_registry::builtin::ItemKind::RedTulip
            | azalea_registry::builtin::ItemKind::OrangeTulip
            | azalea_registry::builtin::ItemKind::WhiteTulip
            | azalea_registry::builtin::ItemKind::PinkTulip
            | azalea_registry::builtin::ItemKind::OxeyeDaisy
            | azalea_registry::builtin::ItemKind::Cornflower
            | azalea_registry::builtin::ItemKind::WitherRose
            | azalea_registry::builtin::ItemKind::LilyOfTheValley
    )
}

fn is_ageable(kind: EntityKind) -> bool {
    super::is_ageable_mob(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::components::LookDirection;
    use crate::world::chunk::ChunkStore;

    fn spawn(store: &mut EntityStore, kind: EntityKind) {
        store.spawn_living(
            7,
            kind,
            dvec3(1.0, 2.0, 3.0).into(),
            LookDirection::new(0.0, 0.0),
            0.0,
            None,
        );
    }

    #[test]
    fn bee_uses_synced_nectar_flag_and_java_trial_count_and_bounds() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Bee);
        assert!(
            store
                .tick_animal_particles(&ChunkStore::new(1), 0)
                .is_empty()
        );
        let bee = store.living.get_mut(&7).unwrap();
        bee.bee_flags = 0x08;
        let mut requests = Vec::new();
        for id in 0..10_000 {
            tick_bee(&mut requests, id, bee);
            if !requests.is_empty() {
                break;
            }
        }
        assert!((1..=2).contains(&requests.len()));
        let (_, height) = dimensions(bee);
        for request in requests {
            assert_eq!(request.kind, Kind::FallingNectar);
            assert_eq!(request.position.y, bee.position.y + height * 0.5);
            assert!((bee.position.x - 0.3..=bee.position.x + 0.3).contains(&request.position.x));
            assert!((bee.position.z - 0.3..=bee.position.z + 0.3).contains(&request.position.z));
            assert_eq!(request.velocity, DVec3::ZERO);
        }
    }

    #[test]
    fn panda_eating_emits_six_component_preserving_item_particles_every_five_ticks() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Panda);
        let entity = store.living.get_mut(&7).unwrap();
        entity.panda_flags = 0x08;
        entity.panda_eat_counter = 4;
        entity.equipment.insert(
            azalea_inventory::components::EquipmentSlot::Mainhand,
            azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
                azalea_registry::builtin::ItemKind::Bamboo,
                3,
            )),
        );
        let chunks = ChunkStore::new(1);
        assert!(store.tick_animal_particles(&chunks, 0).is_empty());
        assert_eq!(store.living[&7].panda_eat_counter, 5);
        let particles = store.tick_animal_particles(&chunks, 1);
        assert_eq!(particles.len(), 6);
        assert!(particles.iter().all(|p| p.kind == Kind::Item));
        assert!(particles.iter().all(|p| p.position != dvec3(1.0, 2.0, 3.0)));
        for particle in particles {
            assert!(
                matches!(particle.options, Options::Item { item_id, count, raw_components: None, .. }
                if item_id == azalea_registry::builtin::ItemKind::Bamboo.to_u32() && count == 3)
            );
        }
    }

    #[test]
    fn interaction_particle_requests_match_successful_ageup_and_mooshroom_branches() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Camel);
        store.living.get_mut(&7).unwrap().is_baby = true;
        assert_eq!(
            store
                .interaction_particle_requests(7, InteractionParticle::AgeUpHappyVillager, 1)
                .len(),
            1
        );
        store.living.get_mut(&7).unwrap().age_locked = true;
        assert!(
            store
                .interaction_particle_requests(7, InteractionParticle::AgeUpHappyVillager, 1)
                .is_empty()
        );
        store.spawn_living(
            8,
            EntityKind::Mooshroom,
            dvec3(1.0, 2.0, 3.0).into(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.living.get_mut(&8).unwrap().variant = 1;
        let repeats = store.interaction_particle_requests(
            8,
            InteractionParticle::BrownMooshroomStewRepeat,
            2,
        );
        assert_eq!(repeats.len(), 2);
        assert!(repeats.iter().all(|p| p.kind == Kind::Smoke));
        let apply =
            store.interaction_particle_requests(8, InteractionParticle::BrownMooshroomStewApply, 3);
        assert_eq!(apply.len(), 4);
        assert!(apply.iter().all(|p| p.kind == Kind::Effect
            && matches!(p.options, Options::Spell { color: -1, power } if power == 1.0)));

        let flower = azalea_registry::builtin::ItemKind::Poppy;
        let first = store.mooshroom_stew_item_requests(8, flower, 4);
        assert_eq!(first.len(), 4);
        assert!(first.iter().all(|p| p.kind == Kind::Effect));
        let repeat = store.mooshroom_stew_item_requests(8, flower, 5);
        assert_eq!(repeat.len(), 2);
        assert!(repeat.iter().all(|p| p.kind == Kind::Smoke));
        assert!(
            store
                .mooshroom_stew_item_requests(8, azalea_registry::builtin::ItemKind::Cactus, 6)
                .is_empty()
        );
        store.living.get_mut(&8).unwrap().variant = 0;
        assert!(store.mooshroom_stew_item_requests(8, flower, 7).is_empty());
    }

    #[test]
    fn age_lock_interaction_snapshots_transition_and_cooldown_resets_on_remove() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Cow);
        store.living.get_mut(&7).unwrap().is_baby = true;
        let chunks = ChunkStore::new(1);
        assert!(store.start_age_lock_particle_timer(7, true));
        assert!(store.living[&7].age_locked);
        assert!(!store.start_age_lock_particle_timer(7, false));
        let particles = store.tick_animal_particles(&chunks, 0);
        assert_eq!(particles.len(), 1);
        assert_eq!(particles[0].kind, Kind::PauseMobGrowth);
        assert!(!store.start_age_lock_particle_timer(7, false));
        store.remove_entity(7);
        assert!(!store.animals_particle_state.contains_key(&7));
    }

    #[test]
    fn dolphin_emits_paired_trail_only_while_moving_in_water() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Dolphin);
        store.living.get_mut(&7).unwrap().is_in_water = true;
        store.set_living_motion(7, dvec3(0.2, 0.0, 0.0));
        assert_eq!(
            store
                .tick_animal_particles(&ChunkStore::new(1), 0)
                .iter()
                .filter(|p| p.kind == Kind::Dolphin)
                .count(),
            4
        );
        store.living.get_mut(&7).unwrap().is_in_water = false;
        assert!(
            store
                .tick_animal_particles(&ChunkStore::new(1), 1)
                .is_empty()
        );
    }

    #[test]
    fn ageable_timers_require_interaction_hook_and_do_not_repeat_after_expiry() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Cow);
        store.living.get_mut(&7).unwrap().is_baby = true;
        let chunks = ChunkStore::new(1);
        assert!(store.tick_animal_particles(&chunks, 0).is_empty());
        assert!(store.start_age_up_particle_timer(7));
        let particles = store.tick_animal_particles(&chunks, 1);
        assert_eq!(particles.len(), 1);
        assert_eq!(particles[0].kind, Kind::HappyVillager);
        for tick in 2..41 {
            let _ = store.tick_animal_particles(&chunks, tick);
        }
        assert!(store.tick_animal_particles(&chunks, 41).is_empty());
    }

    #[test]
    fn sniffer_digging_uses_synced_state_elapsed_clock_and_head_support_block() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.load_decoded_chunk(
            azalea_core::position::ChunkPos::new(0, 0),
            azalea_world::chunk::Chunk::default(),
        );
        let stone = crate::world::block::default_state_of("stone").unwrap();
        chunks.set_block_state(1, 1, 5, stone);
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Sniffer);
        store.living.get_mut(&7).unwrap().sniffer_state = 5;
        assert!(store.tick_animal_particles(&chunks, 0).is_empty());
        store.living.get_mut(&7).unwrap().age_in_ticks = 35;
        let particles = store.tick_animal_particles(&chunks, 1);
        assert_eq!(particles.len(), 30);
        assert!(
            particles
                .iter()
                .all(|p| p.kind == Kind::Block && p.position == dvec3(1.5, 1.85, 5.5))
        );
    }

    #[test]
    fn panda_sneeze_uses_synced_flag_body_yaw_bbox_and_velocity() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Panda);
        let entity = store.living.get_mut(&7).unwrap();
        entity.panda_flags = 0x02;
        entity.panda_sneeze_counter = 20;
        entity.body_y_rot_deg = 90.0;
        entity.velocity = dvec3(0.25, 0.4, -0.5);
        let particles = store.tick_animal_particles(&ChunkStore::new(1), 0);
        let [particle] = particles.as_slice() else {
            panic!("one sneeze particle")
        };
        assert_eq!(particle.kind, Kind::Sneeze);
        assert_eq!(particle.velocity, dvec3(0.25, 0.0, -0.5));
        assert_eq!(store.living[&7].panda_flags & 0x02, 0);
    }

    #[test]
    fn animal_breeding_event_emits_seven_hearts_and_allay_override_emits_three() {
        for (kind, expected_count) in [(EntityKind::Cow, 7), (EntityKind::Allay, 3)] {
            let mut store = EntityStore::new();
            spawn(&mut store, kind);
            let (width, height) = dimensions(&store.living[&7]);
            let requests = store.animal_particle_event(7, 18).expect("Java event 18 owner");
            assert_eq!(requests.len(), expected_count);
            assert!(requests.iter().all(|request| {
                request.kind == Kind::Heart
                    && matches!(request.options, Options::Simple)
                    && request.position.x >= 1.0 - width
                    && request.position.x <= 1.0 + width
                    && request.position.y >= 2.5
                    && request.position.y <= 2.5 + height
                    && request.position.z >= 3.0 - width
                    && request.position.z <= 3.0 + width
                    && request.velocity.is_finite()
                    && !request.always_visible
            }));
        }
        let mut unrelated = EntityStore::new();
        spawn(&mut unrelated, EntityKind::Zombie);
        assert!(unrelated.animal_particle_event(7, 18).is_none());
    }

    #[test]
    fn dolphin_event_38_emits_seven_happy_villager_particles() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Dolphin);
        let requests = store.animal_particle_event(7, 38).unwrap();
        assert_eq!(requests.len(), 7);
        assert!(requests.iter().all(|request| {
            request.kind == Kind::HappyVillager
                && matches!(request.options, Options::Simple)
                && request.position.y >= 2.5
                && request.velocity.is_finite()
                && request.velocity.abs().cmple(dvec3(0.1, 0.1, 0.1)).all()
                && !request.always_visible
        }));
    }

    #[test]
    fn tamable_and_ocelot_events_emit_their_authoritative_seven_particle_bursts() {
        for (kind, success_id, failure_id) in [
            (EntityKind::Wolf, 7, 6),
            (EntityKind::Cat, 7, 6),
            (EntityKind::Parrot, 7, 6),
            (EntityKind::Nautilus, 7, 6),
            (EntityKind::ZombieNautilus, 7, 6),
            (EntityKind::Ocelot, 41, 40),
        ] {
            for (event_id, expected_kind) in [
                (success_id, Kind::Heart),
                (failure_id, Kind::Smoke),
            ] {
                let mut store = EntityStore::new();
                spawn(&mut store, kind);
                let requests = store.animal_particle_event(7, event_id).unwrap();
                assert_eq!(requests.len(), 7, "{kind:?} event {event_id}");
                assert!(requests.iter().all(|request| {
                    request.kind == expected_kind
                        && matches!(request.options, Options::Simple)
                        && request.velocity.is_finite()
                        && !request.always_visible
                }));
            }
        }
        let mut unrelated = EntityStore::new();
        spawn(&mut unrelated, EntityKind::Cow);
        assert!(unrelated.animal_particle_event(7, 6).is_none());
    }

    #[test]
    fn fox_event_45_emits_eight_mouth_item_particles_from_synced_stack() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Fox);
        assert!(store.animal_particle_event(7, 45).unwrap().is_empty());
        let stack = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
            azalea_registry::builtin::ItemKind::Feather,
            2,
        ));
        store.living.get_mut(&7).unwrap().equipment.insert(
            azalea_inventory::components::EquipmentSlot::Mainhand,
            stack,
        );
        let requests = store.animal_particle_event(7, 45).unwrap();
        assert_eq!(requests.len(), 8);
        assert!(requests.iter().all(|request| {
            request.kind == Kind::Item
                && matches!(
                    &request.options,
                    Options::Item { item_id, count, .. }
                        if *item_id == azalea_registry::builtin::ItemKind::Feather.to_u32()
                            && *count == 2
                )
                && request.position == dvec3(1.0, 2.0, 3.5)
                && request.velocity.x.abs() <= 0.05
                && (0.15..=0.25).contains(&request.velocity.y)
                && request.velocity.z == 0.0
        }));
    }

    #[test]
    fn wolf_shake_is_event_owned_and_splashes_only_during_client_animation() {
        let mut store = EntityStore::new();
        spawn(&mut store, EntityKind::Wolf);
        let chunks = ChunkStore::new(1);
        assert!(store.tick_animal_particles(&chunks, 0).is_empty());
        assert!(handle_entity_event(&mut store, 7, 8));
        for tick in 0..9 {
            let _ = store.tick_animal_particles(&chunks, tick);
        }
        assert!(store.living[&7].shake_anim > 0.4);
        assert!(handle_entity_event(&mut store, 7, 56));
        assert!(!store.animals_particle_state[&7].wolf_shake);
    }
}
