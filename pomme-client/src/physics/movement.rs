// Fall distance is tracked locally for movement parity; health damage remains
// server-authoritative and is intentionally not predicted here.

use glam::{DVec3, dvec3};
use winit::keyboard::KeyCode;

use super::aabb::Aabb;
use super::collision::no_collision_for_player;
use crate::app::input::{self, InputState};
use crate::entity::EntityPose;
use crate::player::LocalPlayer;
use crate::world::chunk::ChunkStore;

const GRAVITY: f64 = 0.08;
// Vanilla mixes float and double physics values. Keep float values as f32 until
// the exact point where vanilla widens them into Vec3/AABB doubles.
const JUMP_VELOCITY: f32 = 0.42;
const VERTICAL_DRAG: f32 = 0.98;
const HORIZONTAL_DRAG: f32 = 0.91;
const BLOCK_FRICTION: f32 = 0.6;
const GROUND_FRICTION: f32 = BLOCK_FRICTION * HORIZONTAL_DRAG;
const GROUND_ACCEL_FACTOR: f32 = 0.216_000_02;
// Player.createAttributes receives 0.1f; the sprint modifier amount is 0.3f.
// Both are widened into the double-backed attribute system before
// Player.getSpeed casts the final value back to float.
const MOVEMENT_SPEED_ATTRIBUTE: f64 = 0.1_f32 as f64;
// SNEAKING_SPEED is a double attribute (default 0.3), then LocalPlayer casts it
// to float before scaling its Vec2 input.
const SNEAKING_SPEED: f32 = 0.3_f64 as f32;
const INPUT_DAMPING: f32 = 0.98;
const AIR_ACCELERATION: f32 = 0.02;
// Vanilla WATER_MOVEMENT_EFFICIENCY blends water drag toward 0.54600006f and
// acceleration toward land speed.
const WATER_ACCELERATION: f32 = 0.02;
const WATER_HORIZONTAL_DRAG: f32 = 0.8;
const WATER_HORIZONTAL_DRAG_SPRINT: f32 = 0.9;
const WATER_VERTICAL_DRAG: f32 = 0.8;
// Vanilla `travelInWater` applies fluid drag before gravity / 16 and the
// -0.003 falling clamp in `getFluidFallingAdjustedMovement`.
const LAVA_DRAG: f64 = 0.5;
const LAVA_VERTICAL_DRAG: f64 = 0.8;
// STEP_HEIGHT is a double attribute, but LivingEntity.maxUpStep casts it to
// float.
const STEP_HEIGHT: f32 = 0.6_f64 as f32;
const SPRINT_JUMP_BOOST: f64 = 0.2;
const FLYING_VERTICAL_FRICTION: f64 = 0.6;
// Vanilla Player.getFlyingSpeed values are floats.
const SPRINT_AIR_ACCELERATION: f32 = 0.025_999_999;
const SPRINT_HUNGER_THRESHOLD: u32 = 6;
const JUMP_DELAY_TICKS: u32 = 10;
// Vanilla `Entity.getFluidJumpThreshold`; always 0.4 for the player.
const FLUID_JUMP_THRESHOLD: f64 = 0.4;
// Vanilla `LivingEntity.jumpInLiquid` / `goDownInWater` add/subtract 0.04f.
const LIQUID_JUMP_ACCELERATION: f32 = 0.04;
const DEFAULT_SPRINT_WINDOW: u32 = 7;
const FLY_TOGGLE_WINDOW: u32 = 7;
// Mth.equal(double, double) widens the float EPSILON constant to double.
const MTH_EQUAL_EPSILON: f64 = 1.0e-5_f32 as f64;
const MINOR_COLLISION_ANGLE: f64 = 0.139_626_339_077_949_52;
const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0_f32;
const SIN_SCALE: f64 = 10_430.378_350_470_453;

/// Constant-size diagnostic copies only. Null means this path did not use it.
#[derive(Clone, Copy, Debug, Default)]
pub struct TravelObservation {
    pub friction: Option<f32>,
    pub friction_pos: Option<azalea_core::position::BlockPos>,
    pub ground_drag: Option<f32>,
    pub on_ground_at_start: bool,
    pub block_speed_factor: Option<f32>,
    pub block_jump_factor: Option<f32>,
    pub jump_power: Option<f32>,
    pub step_height: Option<f64>,
    pub bbox_before: Option<Aabb>,
    pub bbox_after: Option<Aabb>,
    pub support_before: Option<azalea_core::position::BlockPos>,
    pub support_after: Option<azalea_core::position::BlockPos>,
    pub pose_at_move: Option<EntityPose>,
    pub descending: bool,
    pub leather_boots: bool,
    pub fall_distance: f64,
    pub entity_collider_count: usize,
    pub entity_shapes_captured: bool,
    pub entity_shapes: [Option<Aabb>; 8],
    pub border_bounds: Option<[f64; 4]>,
    pub ground_decision: Option<bool>,
}

pub fn tick(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    use_speed_multiplier: f32,
    slow_due_to_using_item: bool,
) {
    tick_with_context(
        player,
        input,
        chunk_store,
        &[],
        None,
        use_speed_multiplier,
        slow_due_to_using_item,
    );
}

pub fn tick_with_context(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    use_speed_multiplier: f32,
    slow_due_to_using_item: bool,
) {
    player.last_travel_observation = TravelObservation::default();
    let jump_held = input.performing_action(input::Action::Jump);

    // Vanilla `LivingEntity.aiStep`.
    if player.no_jump_delay > 0 {
        player.no_jump_delay -= 1;
    }

    player.update_water_state(chunk_store);
    apply_fluid_currents(player, chunk_store);
    reset_fall_distance_for_tick(player);
    update_crouch_state(player, input, chunk_store, entity_aabbs, border_bounds);
    player.tick_eye_height();

    // Vanilla `LocalPlayer.modifyInput` keeps the entire input pipeline in
    // float: damping, item-use slowdown, sneaking slowdown, then square remap.
    let sneak_speed = player.attribute_value("minecraft:sneaking_speed", 0.3) as f32;
    let (forward, strafe) = movement_input_with_sneaking_speed(
        input,
        player.crouching,
        use_speed_multiplier,
        sneak_speed,
    );
    let forward_pressed = input.key_pressed(KeyCode::KeyW)
        || input
            .get_gamepad_movement_axes()
            .map(|vec| vec.y > input::STICK_MOVEMENT_THRESHOLD)
            .unwrap_or(false);

    update_sprint_state(
        player,
        input,
        forward,
        forward_pressed,
        slow_due_to_using_item,
    );

    let (sin_y_rot, cos_y_rot) = vanilla_yaw_sin_cos(player.look_dir.y_rot_deg());

    update_fly_state(player, input, chunk_store, sin_y_rot, cos_y_rot);

    if player.flying {
        let mut input_ya = 0.0f32;
        if input.performing_action(input::Action::Sneak) {
            input_ya -= 1.0;
        }
        if jump_held {
            input_ya += 1.0;
        }
        if input_ya != 0.0 {
            // Vanilla does this math in f32 before widening.
            player.velocity.y += f64::from(input_ya * player.fly_speed * 3.0);
        }
    }

    // LivingEntity.aiStep, after Entity's fluid impulses and LocalPlayer's
    // flight input, before jumping and travel (also water/lava/gliding).
    zero_small_velocity(player);

    // Climbable blocks use the jump input for upward movement, independently
    // of the grounded/fluid jump path below.
    if jump_held && is_on_climbable(chunk_store, player.position.into()) {
        player.velocity.y = 0.2;
    }

    // Vanilla `LivingEntity.aiStep`: swim upward when submerged past the jump
    // threshold, otherwise a full jump off the ground or the shallow-fluid floor.
    if jump_held {
        let in_water = player.in_water && player.fluid_height > 0.0;
        if in_water && (!player.on_ground || player.fluid_height > FLUID_JUMP_THRESHOLD) {
            player.velocity.y += f64::from(LIQUID_JUMP_ACCELERATION);
        } else if (player.on_ground || (in_water && player.fluid_height <= FLUID_JUMP_THRESHOLD))
            && player.no_jump_delay == 0
        {
            jump_from_ground(player, chunk_store, sin_y_rot, cos_y_rot);
            player.no_jump_delay = JUMP_DELAY_TICKS;
        }
    } else {
        player.no_jump_delay = 0;
    }

    travel(
        player,
        input,
        chunk_store,
        entity_aabbs,
        border_bounds,
        forward,
        strafe,
        sin_y_rot,
        cos_y_rot,
    );

    update_player_pose(player, input, chunk_store, entity_aabbs, border_bounds);
    player.tick_air_supply();
    stop_flying_on_ground(player);

    player.was_forward_pressed = forward_pressed;
    player.was_jump_pressed = jump_held;
}

/// Vanilla `LivingEntity.travel`: the water or air routine for this tick.
fn travel(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    if player.in_water {
        tick_water(
            player,
            input,
            chunk_store,
            entity_aabbs,
            border_bounds,
            forward,
            strafe,
            sin_y_rot,
            cos_y_rot,
        );
    } else if player.in_lava {
        tick_lava(
            player,
            input,
            chunk_store,
            entity_aabbs,
            border_bounds,
            forward,
            strafe,
            sin_y_rot,
            cos_y_rot,
        );
    } else if player.fall_flying {
        tick_fall_flying(
            player,
            input,
            chunk_store,
            entity_aabbs,
            border_bounds,
            forward,
            strafe,
            sin_y_rot,
            cos_y_rot,
        );
    } else {
        tick_land(
            player,
            input,
            chunk_store,
            entity_aabbs,
            border_bounds,
            forward,
            strafe,
            sin_y_rot,
            cos_y_rot,
        );
    }
}

/// Touching down cancels flight, even in creative.
fn stop_flying_on_ground(player: &mut LocalPlayer) {
    if player.on_ground && player.flying && player.game_mode != 3 {
        player.flying = false;
        player.abilities_dirty = true;
    }
}

/// Vanilla dead-player `LivingEntity.aiStep`: input is immobile, but travel
/// still applies existing velocity, gravity, collision, and drag until tick-20
/// removal.
pub fn tick_dead(player: &mut LocalPlayer, chunk_store: &ChunkStore) {
    tick_dead_with_context(player, chunk_store, &[], None);
}

pub fn tick_dead_with_context(
    player: &mut LocalPlayer,
    chunk_store: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
) {
    player.last_travel_observation = TravelObservation::default();
    player.no_jump_delay = 0;
    player.set_sprinting(false);

    // Local players enter death through SetHealth; entity event 3 intentionally
    // skips LivingEntity.die for players, so the current ordinary player pose
    // remains authoritative until Player.updatePlayerPose runs at tick end.
    let neutral = InputState::released();
    player.update_water_state(chunk_store);
    apply_fluid_currents(player, chunk_store);
    reset_fall_distance_for_tick(player);
    player.tick_eye_height();

    zero_small_velocity(player);
    let (sin_y_rot, cos_y_rot) = vanilla_yaw_sin_cos(player.look_dir.y_rot_deg());
    travel(
        player,
        &neutral,
        chunk_store,
        entity_aabbs,
        border_bounds,
        0.0,
        0.0,
        sin_y_rot,
        cos_y_rot,
    );

    // Player.updatePlayerPose runs after LivingEntity.tick in vanilla. With
    // death-screen input released, this becomes standing unless clearance keeps
    // the player in the crouching pose for the following tick.
    update_crouch_state(player, &neutral, chunk_store, entity_aabbs, border_bounds);
    update_player_pose(player, &neutral, chunk_store, entity_aabbs, border_bounds);

    stop_flying_on_ground(player);
    player.was_forward_pressed = false;
    player.was_jump_pressed = false;
}

// Vanilla `LocalPlayer.aiStep`: a fresh jump press arms the toggle window;
// a second one inside it toggles flight.
fn update_fly_state(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    if player.may_fly {
        if player.game_mode == 3 {
            // Spectator flight is forced on. TODO: spectator noclip
            if !player.flying {
                player.flying = true;
                player.abilities_dirty = true;
            }
        } else if !player.was_jump_pressed && input.performing_action(input::Action::Jump) {
            if player.jump_trigger_time == 0 {
                player.jump_trigger_time = FLY_TOGGLE_WINDOW;
            } else if !player.swimming {
                player.flying = !player.flying;
                if player.flying && player.on_ground {
                    jump_from_ground(player, chunk_store, sin_y_rot, cos_y_rot);
                }
                player.abilities_dirty = true;
                player.jump_trigger_time = 0;
            }
        }
    }
    // Vanilla decrements after the toggle check (unlike sprint_toggle_timer).
    if player.jump_trigger_time > 0 {
        player.jump_trigger_time -= 1;
    }
}

fn zero_small_velocity(player: &mut LocalPlayer) {
    // 26.2 LivingEntity.aiStep: PLAYER uses the horizontal vector magnitude,
    // not a per-axis epsilon. Vertical has its own strict .003 threshold.
    if player.velocity.x * player.velocity.x + player.velocity.z * player.velocity.z < 9.0e-6 {
        player.velocity.x = 0.0;
        player.velocity.z = 0.0;
    }
    if player.velocity.y.abs() < 0.003 {
        player.velocity.y = 0.0;
    }
    player.collision_delta = [DVec3::ZERO; 2];
}

fn jump_from_ground(player: &mut LocalPlayer, chunks: &ChunkStore, sin_y_rot: f32, cos_y_rot: f32) {
    let factor = block_movement_factor(player, chunks, true);
    let boost = player
        .effects
        .sorted_desc()
        .iter()
        .find(|effect| {
            crate::mob_effect::info(effect.effect_id).is_some_and(|info| info.name == "jump_boost")
        })
        .map_or(0.0, |effect| 0.1_f32 * (f32::from(effect.amplifier) + 1.0));
    let jump = player.attribute_value("minecraft:jump_strength", f64::from(JUMP_VELOCITY)) as f32
        * factor
        + boost;
    player.last_travel_observation.block_jump_factor = Some(factor);
    player.last_travel_observation.jump_power = Some(jump);
    if jump <= 1.0e-5_f32 {
        return;
    }
    player.velocity.y = f64::from(jump).max(player.velocity.y);

    if player.sprinting {
        player.velocity.x += f64::from(-sin_y_rot) * SPRINT_JUMP_BOOST;
        player.velocity.z += f64::from(cos_y_rot) * SPRINT_JUMP_BOOST;
    }
}

fn tick_land(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    // Vanilla `travelInAir` samples on-ground once before the move and reuses
    // it for the end-of-tick drag, so a jump launches with ground friction.
    let on_ground_at_start = player.on_ground;

    let saved_vy = player.velocity.y;
    let speed = movement_speed(player);
    let (friction, friction_pos) = player_block_friction(chunk_store, player);
    player.last_travel_observation.friction = Some(friction);
    player.last_travel_observation.friction_pos = Some(friction_pos);
    player.last_travel_observation.on_ground_at_start = on_ground_at_start;
    let climbing = is_on_climbable(chunk_store, player.position.into());
    let accel = friction_influenced_speed(speed, player, friction);
    let (move_x, move_z) = movement_delta(forward, strafe, accel, sin_y_rot, cos_y_rot);
    player.velocity.x += move_x;
    player.velocity.z += move_z;
    if climbing {
        player.velocity.x = player.velocity.x.clamp(-0.15, 0.15);
        player.velocity.z = player.velocity.z.clamp(-0.15, 0.15);
        player.velocity.y = player.velocity.y.max(-0.15);
        if input.performing_action(input::Action::Sneak) && player.velocity.y < 0.0 {
            player.velocity.y = 0.0;
        }
    }

    apply_collision_with_context(
        player,
        input,
        chunk_store,
        entity_aabbs,
        border_bounds,
        forward,
        strafe,
        sin_y_rot,
        cos_y_rot,
    );

    if !player.flying {
        if let Some(delta_y) = levitation_travel_y_delta(player, player.velocity.y) {
            player.velocity.y += delta_y;
        } else {
            player.velocity.y -= effective_gravity(player);
        }
    } else {
        player.velocity.y -= effective_gravity(player);
    }
    player.velocity.y *= f64::from(VERTICAL_DRAG);

    let h_friction = if on_ground_at_start {
        friction * HORIZONTAL_DRAG
    } else {
        HORIZONTAL_DRAG
    };
    player.last_travel_observation.ground_drag = Some(h_friction);
    player.velocity.x *= f64::from(h_friction);
    player.velocity.z *= f64::from(h_friction);

    overwrite_flying_vy(player, saved_vy);
}

fn tick_water(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    if input.performing_action(input::Action::Sneak) {
        player.velocity.y -= f64::from(LIQUID_JUMP_ACCELERATION);
    }

    let mut water_walker =
        player.attribute_value("minecraft:water_movement_efficiency", 0.0) as f32;
    if !player.on_ground {
        water_walker *= 0.5;
    }
    let water_speed =
        WATER_ACCELERATION + (movement_speed(player) - WATER_ACCELERATION) * water_walker;
    let (move_x, move_z) = movement_delta(forward, strafe, water_speed, sin_y_rot, cos_y_rot);
    player.velocity.x += move_x;
    player.velocity.z += move_z;

    if player.swimming {
        let target_vy = vanilla_look_y(player.look_dir.x_rot_deg());
        let boost = if target_vy < -0.2 { 0.085 } else { 0.06 };
        player.velocity.y += (target_vy - player.velocity.y) * boost;
    }

    let saved_vy = player.velocity.y;

    apply_collision_with_context(
        player,
        input,
        chunk_store,
        entity_aabbs,
        border_bounds,
        forward,
        strafe,
        sin_y_rot,
        cos_y_rot,
    );

    let base_drag = if player.sprinting {
        WATER_HORIZONTAL_DRAG_SPRINT
    } else {
        WATER_HORIZONTAL_DRAG
    };
    let h_drag = base_drag + (0.546_000_06 - base_drag) * water_walker;
    player.velocity.x *= f64::from(h_drag);
    player.velocity.z *= f64::from(h_drag);
    player.velocity.y *= f64::from(WATER_VERTICAL_DRAG);
    player.velocity.y = fluid_falling_adjusted(
        player.velocity.y,
        effective_gravity(player),
        saved_vy <= 0.0,
        player.sprinting,
    );

    overwrite_flying_vy(player, saved_vy);
}

/// 26.2 LivingEntity.updateFallFlyingMovement + client-side Entity.move.
fn tick_fall_flying(
    player: &mut LocalPlayer,
    input: &InputState,
    chunks: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    let velocity = *player.velocity;
    let pitch_f = player.look_dir.x_rot_deg() * DEG_TO_RAD;
    let yaw_f = player.look_dir.y_rot_deg() * DEG_TO_RAD;
    let look = dvec3(
        -f64::from(mth_sin(yaw_f) * mth_cos(pitch_f)),
        -f64::from(mth_sin(pitch_f)),
        f64::from(mth_cos(yaw_f) * mth_cos(pitch_f)),
    );
    let look_h = (look.x * look.x + look.z * look.z).sqrt();
    let speed_h = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
    let pitch = f64::from(pitch_f);
    let lift = pitch.cos().powi(2);
    let mut next = velocity + dvec3(0.0, effective_gravity(player) * (-1.0 + lift * 0.75), 0.0);
    if next.y < 0.0 && look_h > 0.0 {
        let convert = next.y * -0.1 * lift;
        next += dvec3(
            look.x * convert / look_h,
            convert,
            look.z * convert / look_h,
        );
    }
    if pitch < 0.0 && look_h > 0.0 {
        let convert = speed_h * -f64::from(mth_sin(pitch_f)) * 0.04;
        next += dvec3(
            -look.x * convert / look_h,
            convert * 3.2,
            -look.z * convert / look_h,
        );
    }
    if look_h > 0.0 {
        next += dvec3(
            (look.x / look_h * speed_h - next.x) * 0.1,
            0.0,
            (look.z / look_h * speed_h - next.z) * 0.1,
        );
    }
    player.velocity = (next * dvec3(0.99, 0.98, 0.99)).into();
    apply_collision_with_context(
        player,
        input,
        chunks,
        entity_aabbs,
        border_bounds,
        forward,
        strafe,
        sin_y_rot,
        cos_y_rot,
    );
}

fn tick_lava(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    let (move_x, move_z) =
        movement_delta(forward, strafe, WATER_ACCELERATION, sin_y_rot, cos_y_rot);
    player.velocity.x += move_x;
    player.velocity.z += move_z;

    let saved_vy = player.velocity.y;
    apply_collision_with_context(
        player,
        input,
        chunk_store,
        entity_aabbs,
        border_bounds,
        forward,
        strafe,
        sin_y_rot,
        cos_y_rot,
    );

    if player.lava_height <= FLUID_JUMP_THRESHOLD {
        player.velocity.x *= LAVA_DRAG;
        player.velocity.y *= LAVA_VERTICAL_DRAG;
        player.velocity.z *= LAVA_DRAG;
        player.velocity.y = fluid_falling_adjusted(
            player.velocity.y,
            effective_gravity(player),
            saved_vy <= 0.0,
            player.sprinting,
        );
    } else {
        player.velocity.x *= LAVA_DRAG;
        player.velocity.y *= LAVA_DRAG;
        player.velocity.z *= LAVA_DRAG;
    }
    player.velocity.y -= effective_gravity(player) / 4.0;
    overwrite_flying_vy(player, saved_vy);
}

fn fluid_falling_adjusted(movement_y: f64, gravity: f64, is_falling: bool, sprinting: bool) -> f64 {
    if gravity == 0.0 || sprinting {
        return movement_y;
    }
    if is_falling
        && (movement_y - 0.005).abs() >= 0.003
        && (movement_y - gravity / 16.0).abs() < 0.003
    {
        -0.003
    } else {
        movement_y - gravity / 16.0
    }
}

// Vanilla Player.travel: while flying the travel step runs normally (gravity
// and water physics included) but its vertical result is discarded, replaced
// with the pre-travel vy decayed by 0.6.
fn overwrite_flying_vy(player: &mut LocalPlayer, saved_vy: f64) {
    if player.flying {
        player.velocity.y = saved_vy * FLYING_VERTICAL_FRICTION;
    }
}

fn apply_collision(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    apply_collision_with_context(
        player,
        input,
        chunk_store,
        &[],
        None,
        forward,
        strafe,
        sin_y_rot,
        cos_y_rot,
    );
}

fn apply_collision_with_context(
    player: &mut LocalPlayer,
    input: &InputState,
    chunk_store: &ChunkStore,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) {
    if player.game_mode == 3 {
        player.collision_delta = [*player.velocity; 2];
        player.position += *player.velocity;
        player.on_ground = false;
        player.main_supporting_block_pos = None;
        player.on_ground_no_blocks = false;
        player.horizontal_collision = false;
        player.minor_horizontal_collision = false;
        return;
    }

    let move_from = *player.position;
    let aabb = player.bounding_box();
    let mut delta = *player.velocity;
    if intersects_block_id(chunk_store, &aabb, "cobweb") {
        // WebBlock sets a one-move multiplier and clears delta movement.
        let multiplier = if has_effect_named(player, "weaving") {
            dvec3(0.5, 0.25, 0.5)
        } else {
            dvec3(0.25, 0.05_f32 as f64, 0.25)
        };
        delta *= multiplier;
        *player.velocity = dvec3(0.0, 0.0, 0.0);
        player.fall_distance = 0.0;
    }
    if intersects_block_id(chunk_store, &aabb, "powder_snow") {
        // PowderSnowBlock.entityInside schedules this multiplier for the next
        // Entity.move; the inside-block pass has no persistent state here.
        delta *= dvec3(0.9_f32 as f64, 1.5_f32 as f64, 0.9_f32 as f64);
        *player.velocity = dvec3(0.0, 0.0, 0.0);
        player.fall_distance = 0.0;
    }
    if is_on_climbable(chunk_store, player.position.into()) {
        player.fall_distance = 0.0;
        delta.x = delta.x.clamp(-0.15, 0.15);
        delta.z = delta.z.clamp(-0.15, 0.15);
        delta.y = delta.y.max(-0.15);
    }
    let step_height =
        f64::from(player.attribute_value("minecraft:step_height", f64::from(STEP_HEIGHT)) as f32);
    let context = (
        input.performing_action(input::Action::Sneak),
        has_leather_boots(player),
        player.fall_distance,
    );
    player.last_travel_observation.bbox_before = Some(aabb);
    player.last_travel_observation.support_before = player.main_supporting_block_pos;
    player.last_travel_observation.pose_at_move = Some(player.pose);
    player.last_travel_observation.step_height = Some(step_height);
    player.last_travel_observation.descending = context.0;
    player.last_travel_observation.leather_boots = context.1;
    player.last_travel_observation.fall_distance = context.2;
    player.last_travel_observation.entity_collider_count = entity_aabbs.len();
    if player.observe_collision_shapes {
        player.last_travel_observation.entity_shapes_captured = true;
        for (slot, aabb) in player
            .last_travel_observation
            .entity_shapes
            .iter_mut()
            .zip(entity_aabbs)
        {
            *slot = Some(*aabb);
        }
    }
    player.last_travel_observation.border_bounds = border_bounds;
    delta = back_off_from_edge(
        chunk_store,
        player,
        delta,
        context,
        step_height,
        entity_aabbs,
        border_bounds,
    );
    let (resolved, on_ground) = super::collision::resolve_collision_for_player(
        chunk_store,
        aabb,
        delta.into(),
        step_height,
        player.on_ground,
        entity_aabbs,
        border_bounds,
        Some(context),
    );

    player.collision_delta = [delta, resolved];

    // Vanilla horizontal collision flags use Mth.equal(double, double), whose
    // epsilon is the widened float constant 1.0E-5f.
    let collided_x = !mth_equal(delta.x, resolved.x);
    // Vanilla only applies Mth.equal to the horizontal axes. Vertical
    // collision is an exact double comparison.
    let collided_y = delta.y != resolved.y;
    let collided_z = !mth_equal(delta.z, resolved.z);
    let horizontal_collision = collided_x || collided_z;

    player.position += resolved;
    player.on_ground = on_ground;
    player.horizontal_collision = horizontal_collision;
    player.minor_horizontal_collision = horizontal_collision
        && is_minor_horizontal_collision(forward, strafe, sin_y_rot, cos_y_rot, resolved);
    check_supporting_block(player, chunk_store, resolved, context);
    player.last_travel_observation.ground_decision = Some(on_ground);
    player.last_travel_observation.bbox_after = Some(player.bounding_box());
    player.last_travel_observation.support_after = player.main_supporting_block_pos;
    update_fall_distance(
        &mut player.fall_distance,
        resolved.y,
        on_ground,
        player.in_water,
    );
    if horizontal_collision
        && delta.y < 0.0
        && touches_block_id(chunk_store, &player.bounding_box(), "honey_block")
    {
        // HoneyBlock.doSlideMovement throttles the fall to
        // ((-0.05 - 0.08) * 0.98), with extra horizontal damping at speed.
        let (horizontal_scale, slide_y) = honey_slide_movement(delta.y);
        player.velocity.x *= horizontal_scale;
        player.velocity.z *= horizontal_scale;
        player.velocity.y = slide_y;
        player.fall_distance = 0.0;
    }

    if collided_x {
        player.velocity.x = 0.0;
    }
    if collided_z {
        player.velocity.z = 0.0;
    }
    // Zero the vertical velocity on ground/ceiling contact (vanilla does this in
    // move()). Gravity is re-applied after the move, leaving vy slightly
    // negative so the next tick's move always probes downward and keeps
    // `on_ground` stable instead of flickering.
    if collided_y {
        let landed_on_slime = delta.y < 0.0 && !input.performing_action(input::Action::Sneak) && {
            let pos = player.on_pos(chunk_store, 0.2_f32);
            crate::world::block::block_id(chunk_store.get_block_state(pos.x, pos.y, pos.z))
                == "slime_block"
        };
        // Entity.restituteMovementAfterCollisions compensates gravity and
        // blends air drag by the fraction of the downward move completed.
        player.velocity.y = if landed_on_slime && -delta.y >= GRAVITY {
            let portion = (resolved.y / delta.y).clamp(0.0, 1.0);
            let air_drag = f64::from(VERTICAL_DRAG);
            (portion * GRAVITY - delta.y) * (1.0 + (air_drag - 1.0) * portion)
        } else {
            0.0
        };
    }

    if !player.flying
        && (horizontal_collision || input.performing_action(input::Action::Jump))
        && is_on_climbable(chunk_store, player.position.into())
    {
        // LivingEntity.handleRelativeFrictionAndCalculateMovement applies this
        // after move for climbables, including a wall collision while walking.
        player.velocity.y = 0.2;
    }

    if (horizontal_collision || input.performing_action(input::Action::Jump))
        && intersects_block_id(chunk_store, &aabb, "powder_snow")
        && has_leather_boots(player)
    {
        // Preserve the independent powder-snow/leather-boots climb condition.
        player.velocity.y = 0.2;
    }

    apply_bubble_column_effect(player, chunk_store, move_from, *player.position);

    let speed_factor = f64::from(block_movement_factor(player, chunk_store, false));
    player.last_travel_observation.block_speed_factor = Some(speed_factor as f32);
    player.velocity.x *= speed_factor;
    player.velocity.z *= speed_factor;

}

fn apply_bubble_column_effect(
    player: &mut LocalPlayer,
    chunks: &ChunkStore,
    from: DVec3,
    to: DVec3,
) {
    if player.flying {
        return;
    }
    let aabb = player.bounding_box();
    let half = (aabb.max - aabb.min) * 0.5;
    let center_offset = (aabb.max + aabb.min) * 0.5 - *player.position;
    let start = from + center_offset;
    let end = to + center_offset;
    let movement = to - from;
    // `aabb` is at the post-move position; extend it backward to cover the
    // starting box as well as the end box.
    let min = (aabb.min.min(aabb.min - movement)).floor().as_ivec3();
    let max = (aabb.max.max(aabb.max - movement)).ceil().as_ivec3();
    let mut visited = Vec::new();
    for x in min.x..max.x {
        for y in min.y..max.y {
            for z in min.z..max.z {
                if let Some(t) = swept_aabb_entry(start, end, half, [x, y, z]) {
                    visited.push((t, x, y, z));
                }
            }
        }
    }
    visited.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then((a.1, a.2, a.3).cmp(&(b.1, b.2, b.3)))
    });
    for (_, x, y, z) in visited {
        let state = chunks.get_block_state(x, y, z);
        if crate::world::block::block_id(state) != "bubble_column" {
            continue;
        }
        let drag = crate::world::block::block_properties(state).get("drag") == Some("true");
        let above = chunks.get_block_state(x, y + 1, z);
        let open_above = crate::physics::block_shape::partial_shape(above)
            .is_some_and(|shape| shape.is_empty())
            && crate::world::block::fluid(above).kind == crate::world::block::FluidKind::Empty;
        player.velocity.y = bubble_column_velocity(player.velocity.y, drag, open_above);
        if !open_above {
            player.fall_distance = 0.0;
        }
    }
}

fn swept_aabb_entry(start: DVec3, end: DVec3, half: DVec3, block: [i32; 3]) -> Option<f64> {
    let block_min = dvec3(
        f64::from(block[0]),
        f64::from(block[1]),
        f64::from(block[2]),
    ) - half;
    let block_max = block_min + DVec3::ONE + half * 2.0;
    let movement = end - start;
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    for axis in 0..3 {
        if movement[axis] == 0.0 {
            if start[axis] <= block_min[axis] || start[axis] >= block_max[axis] {
                return None;
            }
            continue;
        }
        let a = (block_min[axis] - start[axis]) / movement[axis];
        let b = (block_max[axis] - start[axis]) / movement[axis];
        enter = enter.max(a.min(b));
        exit = exit.min(a.max(b));
        if enter >= exit {
            return None;
        }
    }
    (enter < 1.0 && exit > 0.0).then_some(enter.max(0.0))
}

fn bubble_column_velocity(vy: f64, drag: bool, open_above: bool) -> f64 {
    match (drag, open_above) {
        (false, false) => (vy + 0.06).min(0.7),
        (true, false) => (vy - 0.03).max(-0.3),
        (false, true) => (vy + 0.1).min(1.8),
        (true, true) => (vy - 0.03).max(-0.9),
    }
}

fn levitation_travel_y_delta(player: &LocalPlayer, movement_y: f64) -> Option<f64> {
    player.effects.sorted_desc().iter().find_map(|effect| {
        crate::mob_effect::info(effect.effect_id)
            .is_some_and(|info| info.name == "levitation")
            .then(|| (0.05 * (f64::from(effect.amplifier) + 1.0) - movement_y) * 0.2)
    })
}

fn honey_slide_movement(delta_y: f64) -> (f64, f64) {
    let old_y = delta_y / f64::from(VERTICAL_DRAG) + 0.08;
    let horizontal_scale = if old_y < -0.13 { -0.05 / old_y } else { 1.0 };
    (horizontal_scale, (-0.05 - 0.08) * f64::from(VERTICAL_DRAG))
}

fn update_fall_distance(fall_distance: &mut f64, resolved_y: f64, on_ground: bool, in_water: bool) {
    if on_ground || in_water {
        *fall_distance = 0.0;
    } else if resolved_y < 0.0 {
        *fall_distance -= f64::from(resolved_y as f32);
    }
}

fn reset_fall_distance_for_tick(player: &mut LocalPlayer) {
    if player.in_water
        || has_effect_named(player, "slow_falling")
        || has_effect_named(player, "levitation")
    {
        player.fall_distance = 0.0;
    } else if player.fall_flying && player.velocity.y > -0.5 && player.fall_distance > 1.0 {
        // LivingEntity.updateFallFlying calls checkFallDistanceAccumulation;
        // this limits stale accumulated distance, it does not erase it.
        player.fall_distance = 1.0;
    }
}

pub(crate) fn has_leather_boots(player: &LocalPlayer) -> bool {
    matches!(
        player.inventory.slot(8),
        azalea_inventory::ItemStack::Present(stack)
            if stack.kind == azalea_registry::builtin::ItemKind::LeatherBoots
    )
}

fn has_effect_named(player: &LocalPlayer, name: &str) -> bool {
    player.effects.sorted_desc().iter().any(|effect| {
        crate::mob_effect::info(effect.effect_id).is_some_and(|info| info.name == name)
    })
}

fn update_sprint_state(
    player: &mut LocalPlayer,
    input: &InputState,
    forward: f32,
    forward_pressed: bool,
    slow_due_to_using_item: bool,
) {
    if player.sprint_toggle_timer > 0 {
        player.sprint_toggle_timer -= 1;
    }
    if input.performing_action(input::Action::Sneak) || slow_due_to_using_item {
        player.sprint_toggle_timer = 0;
    }

    // Crouching blocks starting a sprint but doesn't stop one in progress.
    // Vanilla `canStartSprinting` also denies it while slowed by an item use,
    // and the slowed input impulse (< 0.8) stops a sprint in progress.
    let can_sprint = !player.sprinting
        && forward > 0.0
        && player.food > SPRINT_HUNGER_THRESHOLD
        && !player.crouching
        && !slow_due_to_using_item;

    if input.performing_action(input::Action::Sprint) && can_sprint {
        player.set_sprinting(true);
    }

    if !player.was_forward_pressed && forward_pressed && can_sprint {
        if player.sprint_toggle_timer > 0 {
            player.set_sprinting(true);
        }
        player.sprint_toggle_timer = DEFAULT_SPRINT_WINDOW;
    }

    if player.sprinting
        && (forward <= 0.0
            || player.food <= SPRINT_HUNGER_THRESHOLD
            || slow_due_to_using_item
            || (player.horizontal_collision
                && !player.minor_horizontal_collision
                && !player.swimming))
    {
        player.set_sprinting(false);
    }
}

fn can_fit_pose(
    player: &LocalPlayer,
    chunks: &ChunkStore,
    pose: EntityPose,
    entities: &[Aabb],
    border: Option<[f64; 4]>,
    descending: bool,
) -> bool {
    no_collision_for_player(
        chunks,
        &player.bounding_box_for_pose(pose).deflate(1.0e-7),
        &player.bounding_box(),
        entities,
        border,
        (descending, has_leather_boots(player), player.fall_distance),
    )
}

fn update_crouch_state(
    player: &mut LocalPlayer,
    input: &InputState,
    chunks: &ChunkStore,
    entities: &[Aabb],
    border: Option<[f64; 4]>,
) {
    let shift = input.performing_action(input::Action::Sneak);
    player.crouching = player.game_mode != 3
        && !player.flying
        && !player.swimming
        && can_fit_pose(
            player,
            chunks,
            EntityPose::Crouching,
            entities,
            border,
            shift,
        )
        && (shift
            || !can_fit_pose(
                player,
                chunks,
                EntityPose::Standing,
                entities,
                border,
                shift,
            ));
}

fn update_player_pose(
    player: &mut LocalPlayer,
    input: &InputState,
    chunks: &ChunkStore,
    entities: &[Aabb],
    border: Option<[f64; 4]>,
) {
    let shift = input.performing_action(input::Action::Sneak);
    if !can_fit_pose(
        player,
        chunks,
        EntityPose::Swimming,
        entities,
        border,
        shift,
    ) {
        return;
    }
    let desired = if player.is_sleeping() {
        EntityPose::Sleeping
    } else if player.swimming {
        EntityPose::Swimming
    } else if player.fall_flying {
        EntityPose::FallFlying
    } else if shift && !player.flying {
        EntityPose::Crouching
    } else {
        EntityPose::Standing
    };
    player.pose = if player.game_mode == 3
        || can_fit_pose(player, chunks, desired, entities, border, shift)
    {
        desired
    } else if can_fit_pose(
        player,
        chunks,
        EntityPose::Crouching,
        entities,
        border,
        shift,
    ) {
        EntityPose::Crouching
    } else {
        EntityPose::Swimming
    };
}

fn check_supporting_block(
    player: &mut LocalPlayer,
    chunks: &ChunkStore,
    movement: DVec3,
    context: (bool, bool, f64),
) {
    if !player.on_ground {
        player.main_supporting_block_pos = None;
        player.on_ground_no_blocks = false;
        return;
    }
    let bb = player.bounding_box();
    let area = Aabb::new(
        bb.min - dvec3(0.0, 1.0e-6, 0.0),
        dvec3(bb.max.x, bb.min.y, bb.max.z),
    );
    let context = (bb.min.y, context.0, context.1, context.2);
    let mut support =
        super::collision::find_supporting_block(chunks, &area, player.position.into(), context);
    if support.is_none() && !player.on_ground_no_blocks {
        support = super::collision::find_supporting_block(
            chunks,
            &area.offset(dvec3(-movement.x, 0.0, -movement.z)),
            player.position.into(),
            context,
        );
    }
    player.main_supporting_block_pos = support;
    player.on_ground_no_blocks = support.is_none();
}

// While holding shift on the ground, clamp the horizontal move so the player
// can't fall further than the step height.
fn back_off_from_edge(
    chunk_store: &ChunkStore,
    player: &LocalPlayer,
    delta: DVec3,
    context: (bool, bool, f64),
    step_height: f64,
    entities: &[Aabb],
    border: Option<[f64; 4]>,
) -> DVec3 {
    let bb = player.bounding_box();
    let can_fall = |dx, dz, height| {
        can_fall_at_least(chunk_store, &bb, dx, dz, height, entities, border, context)
    };
    if !context.0 || player.flying || delta.y > 0.0 {
        return delta;
    }
    let fall = player.fall_distance;
    let above_ground =
        player.on_ground || (fall < step_height && !can_fall(0.0, 0.0, step_height - fall));
    if !above_ground {
        return delta;
    }

    let mut dx = delta.x;
    let mut dz = delta.z;
    let step_x = dx.signum() * 0.05;
    let step_z = dz.signum() * 0.05;

    while dx != 0.0 && can_fall(dx, 0.0, step_height) {
        if dx.abs() <= 0.05 {
            dx = 0.0;
            break;
        }
        dx -= step_x;
    }
    while dz != 0.0 && can_fall(0.0, dz, step_height) {
        if dz.abs() <= 0.05 {
            dz = 0.0;
            break;
        }
        dz -= step_z;
    }
    while dx != 0.0 && dz != 0.0 && can_fall(dx, dz, step_height) {
        dx = if dx.abs() <= 0.05 { 0.0 } else { dx - step_x };
        if dz.abs() <= 0.05 {
            dz = 0.0;
            continue;
        }
        dz -= step_z;
    }

    dvec3(dx, delta.y, dz)
}

fn is_on_climbable(chunks: &ChunkStore, position: DVec3) -> bool {
    let (x, y, z) = (
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    );
    let state = chunks.get_block_state(x, y, z);
    let id = crate::world::block::block_id(state);
    if matches!(
        id,
        "ladder"
            | "vine"
            | "scaffolding"
            | "weeping_vines"
            | "weeping_vines_plant"
            | "twisting_vines"
            | "twisting_vines_plant"
    ) {
        return true;
    }
    let props = crate::world::block::block_properties(state);
    if id.ends_with("_trapdoor") && props.get("open") == Some("true") {
        let below = chunks.get_block_state(x, y - 1, z);
        return crate::world::block::block_id(below) == "ladder"
            && crate::world::block::block_properties(below).get("facing") == props.get("facing");
    }
    false
}

/// Fluid flow follows FlowingFluid.getFlow and EntityFluidInteraction's
/// player-specific current averaging.
fn apply_fluid_currents(player: &mut LocalPlayer, chunks: &ChunkStore) {
    use crate::world::block::{FluidKind, fluid};

    let bb = player.bounding_box();
    for (kind, strength, touching) in [
        (FluidKind::Water, 0.014, player.in_water),
        (FluidKind::Lava, 0.002_333_333_333_333_333_5, player.in_lava),
    ] {
        if !touching {
            continue;
        }
        let mut accumulated = dvec3(0.0, 0.0, 0.0);
        let mut sample_count = 0u32;
        let fluid_height = match kind {
            FluidKind::Water => player.fluid_height,
            FluidKind::Lava => player.lava_height,
            FluidKind::Empty => 0.0,
        };
        let (x0, x1) = (bb.min.x.floor() as i32, bb.max.x.ceil() as i32 - 1);
        let (y0, y1) = (bb.min.y.floor() as i32, bb.max.y.ceil() as i32 - 1);
        let (z0, z1) = (bb.min.z.floor() as i32, bb.max.z.ceil() as i32 - 1);
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let current = fluid(chunks.get_block_state(x, y, z));
                    if current.kind != kind {
                        continue;
                    }
                    let above_fluid = fluid(chunks.get_block_state(x, y + 1, z));
                    let surface = if above_fluid.kind == kind {
                        1.0
                    } else {
                        f64::from(current.height())
                    };
                    if y as f64 + surface < bb.min.y + 0.001 {
                        continue;
                    }
                    let mut cell_flow = dvec3(0.0, 0.0, 0.0);
                    for (dx, dz) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                        let neighbor_state = chunks.get_block_state(x + dx, y, z + dz);
                        let neighbor = fluid(neighbor_state);
                        // FlowingFluid.getFlow uses getOwnHeight (even for falling
                        // fluid), and only probes below an empty neighbor.
                        if neighbor.kind == kind {
                            let difference = f64::from(current.height() - neighbor.height());
                            cell_flow.x += dx as f64 * difference;
                            cell_flow.z += dz as f64 * difference;
                        } else if neighbor.kind == FluidKind::Empty
                            && !crate::world::block::blocks_motion(neighbor_state)
                        {
                            let below = fluid(chunks.get_block_state(x + dx, y - 1, z + dz));
                            if below.kind == kind && below.amount > 0 {
                                let difference = f64::from(
                                    current.height() - (below.height() - 0.888_888_9_f32),
                                );
                                cell_flow.x += dx as f64 * difference;
                                cell_flow.z += dz as f64 * difference;
                            }
                        }
                    }
                    if current.falling {
                        let has_solid_side =
                            [(0, -1), (0, 1), (-1, 0), (1, 0)]
                                .into_iter()
                                .any(|(dx, dz)| {
                                    let side = chunks.get_block_state(x + dx, y, z + dz);
                                    let above = chunks.get_block_state(x + dx, y + 1, z + dz);
                                    (fluid(side).kind != kind
                                        && !matches!(
                                            crate::world::block::block_id(side),
                                            "ice" | "frosted_ice"
                                        )
                                        && crate::world::block::has_full_horizontal_sturdy_face(
                                            side, dx, dz,
                                        ))
                                        || (fluid(above).kind != kind
                                            && !matches!(
                                                crate::world::block::block_id(above),
                                                "ice" | "frosted_ice"
                                            )
                                            && crate::world::block::has_full_horizontal_sturdy_face(
                                                above, dx, dz,
                                            ))
                                });
                        if has_solid_side {
                            let horizontal = cell_flow.normalize_or_zero();
                            cell_flow = (horizontal + dvec3(0.0, -6.0, 0.0)).normalize_or_zero();
                        }
                    }
                    let cell_length = cell_flow.length();
                    if cell_length > 0.0 {
                        let mut flow = cell_flow / cell_length;
                        if fluid_height < 0.4 {
                            flow *= fluid_height;
                        }
                        accumulated += flow;
                    }
                    sample_count += 1;
                }
            }
        }
        let accumulated_length_sqr = accumulated.length_squared();
        if sample_count > 0 && accumulated_length_sqr >= 1.0e-5 {
            let mut impulse = accumulated / f64::from(sample_count) * strength;
            if player.velocity.x.abs() < 0.003
                && player.velocity.z.abs() < 0.003
                && impulse.length() < 0.0045
            {
                impulse = impulse.normalize() * 0.0045;
            }
            player.velocity = (*player.velocity + impulse).into();
        }
    }
}

fn touches_block_id(chunks: &ChunkStore, aabb: &Aabb, id: &str) -> bool {
    const EPSILON: f64 = 1.0e-7;
    intersects_block_id(
        chunks,
        &Aabb::new(
            aabb.min - dvec3(EPSILON, EPSILON, EPSILON),
            aabb.max + dvec3(EPSILON, EPSILON, EPSILON),
        ),
        id,
    )
}

fn intersects_block_id(chunks: &ChunkStore, aabb: &Aabb, id: &str) -> bool {
    let min_x = aabb.min.x.floor() as i32;
    let min_y = aabb.min.y.floor() as i32;
    let min_z = aabb.min.z.floor() as i32;
    let max_x = aabb.max.x.ceil() as i32;
    let max_y = aabb.max.y.ceil() as i32;
    let max_z = aabb.max.z.ceil() as i32;
    (min_x..max_x).any(|x| {
        (min_y..max_y).any(|y| {
            (min_z..max_z).any(|z| {
                crate::world::block::block_id(chunks.get_block_state(x, y, z)) == id
                    && Aabb::block(x, y, z).intersects(aabb)
            })
        })
    })
}

fn can_fall_at_least(
    chunk_store: &ChunkStore,
    bb: &Aabb,
    dx: f64,
    dz: f64,
    min_height: f64,
    entities: &[Aabb],
    border: Option<[f64; 4]>,
    context: (bool, bool, f64),
) -> bool {
    no_collision_for_player(
        chunk_store,
        &Aabb::new(
            dvec3(
                bb.min.x + 1.0e-7 + dx,
                bb.min.y - min_height - 1.0e-7,
                bb.min.z + 1.0e-7 + dz,
            ),
            dvec3(bb.max.x - 1.0e-7 + dx, bb.min.y, bb.max.z - 1.0e-7 + dz),
        ),
        bb,
        entities,
        border,
        context,
    )
}

fn player_block_friction(
    chunks: &ChunkStore,
    player: &LocalPlayer,
) -> (f32, azalea_core::position::BlockPos) {
    let pos = player.on_pos(chunks, 0.500_001_f32);
    (
        friction_for_block_id(crate::world::block::block_id(
            chunks.get_block_state(pos.x, pos.y, pos.z),
        )),
        pos,
    )
}

fn block_movement_factor(player: &LocalPlayer, chunks: &ChunkStore, jump: bool) -> f32 {
    let here = chunks.get_block_state(
        player.position.x.floor() as i32,
        player.position.y.floor() as i32,
        player.position.z.floor() as i32,
    );
    let factor = |state| match crate::world::block::block_id(state) {
        "honey_block" if jump => 0.5_f32,
        "honey_block" | "soul_sand" if !jump => 0.4_f32,
        _ => 1.0_f32,
    };
    let here_factor = factor(here);
    if here_factor != 1.0
        || (!jump
            && matches!(
                crate::world::block::block_id(here),
                "water" | "bubble_column"
            ))
    {
        return here_factor;
    }
    let pos = player.on_pos(chunks, 0.500_001_f32);
    factor(chunks.get_block_state(pos.x, pos.y, pos.z))
}

/// Floor fallback retained for horse physics and historical-position
/// diagnostics.
pub(crate) fn block_friction(
    chunks: &ChunkStore,
    position: crate::entity::components::Position,
) -> f32 {
    let id = crate::world::block::block_id(chunks.get_block_state(
        position.x.floor() as i32,
        (position.y - f64::from(0.500_001_f32)).floor() as i32,
        position.z.floor() as i32,
    ));
    friction_for_block_id(id)
}

fn friction_for_block_id(id: &str) -> f32 {
    match id {
        "ice" | "packed_ice" | "frosted_ice" => 0.98,
        "blue_ice" => 0.989,
        "slime_block" => 0.8,
        _ => BLOCK_FRICTION,
    }
}

pub(crate) fn movement_speed(player: &LocalPlayer) -> f32 {
    player.attribute_value("minecraft:movement_speed", MOVEMENT_SPEED_ATTRIBUTE) as f32
}

pub(crate) fn effective_gravity(player: &LocalPlayer) -> f64 {
    let gravity = player.attribute_value("minecraft:gravity", GRAVITY);
    if player.velocity.y < 0.0
        && player.effects.sorted_desc().iter().any(|effect| {
            crate::mob_effect::info(effect.effect_id)
                .is_some_and(|info| info.name == "slow_falling")
        })
    {
        gravity.min(0.01)
    } else {
        gravity
    }
}

fn movement_delta(
    forward: f32,
    strafe: f32,
    speed: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
) -> (f64, f64) {
    // Vanilla constructs Vec3 from the float xxa/zza fields, then performs the
    // normalization and speed scaling in double precision.
    let mut x = f64::from(strafe);
    let mut z = f64::from(forward);
    let length_sq = x * x + z * z;
    if length_sq < 1.0e-7 {
        return (0.0, 0.0);
    }
    if length_sq > 1.0 {
        // `Vec3.normalize` divides; a reciprocal multiply lands an ULP off.
        let length = length_sq.sqrt();
        x /= length;
        z /= length;
    }
    let speed = f64::from(speed);
    x *= speed;
    z *= speed;

    let sin = f64::from(sin_y_rot);
    let cos = f64::from(cos_y_rot);
    (x * cos - z * sin, z * cos + x * sin)
}

fn world_input_direction(forward: f32, strafe: f32, sin_y_rot: f32, cos_y_rot: f32) -> (f64, f64) {
    let forward = f64::from(forward);
    let strafe = f64::from(strafe);
    let sin = f64::from(sin_y_rot);
    let cos = f64::from(cos_y_rot);
    (strafe * cos - forward * sin, forward * cos + strafe * sin)
}

fn friction_influenced_speed(speed: f32, player: &LocalPlayer, block_friction: f32) -> f32 {
    if player.on_ground {
        speed * (GROUND_ACCEL_FACTOR / (block_friction * block_friction * block_friction))
    } else if player.flying {
        // Vanilla Player.getFlyingSpeed.
        if player.sprinting {
            player.fly_speed * 2.0_f32
        } else {
            player.fly_speed
        }
    } else if player.sprinting {
        SPRINT_AIR_ACCELERATION
    } else {
        AIR_ACCELERATION
    }
}

fn is_minor_horizontal_collision(
    forward: f32,
    strafe: f32,
    sin_y_rot: f32,
    cos_y_rot: f32,
    resolved: DVec3,
) -> bool {
    let (intent_x, intent_z) = world_input_direction(forward, strafe, sin_y_rot, cos_y_rot);
    let intent_len_sq = intent_x * intent_x + intent_z * intent_z;
    let resolved_len_sq = resolved.x * resolved.x + resolved.z * resolved.z;
    if intent_len_sq < f64::from(1.0e-5_f32) || resolved_len_sq < f64::from(1.0e-5_f32) {
        return false;
    }
    let dot = intent_x * resolved.x + intent_z * resolved.z;
    let angle = (dot / (intent_len_sq * resolved_len_sq).sqrt()).acos();
    angle < MINOR_COLLISION_ANGLE
}

pub(crate) fn movement_input(
    input: &InputState,
    crouching: bool,
    use_speed_multiplier: f32,
) -> (f32, f32) {
    movement_input_with_sneaking_speed(input, crouching, use_speed_multiplier, SNEAKING_SPEED)
}

fn movement_input_with_sneaking_speed(
    input: &InputState,
    crouching: bool,
    use_speed_multiplier: f32,
    sneaking_speed: f32,
) -> (f32, f32) {
    // Keep the LocalPlayer input pipeline in float exactly like vanilla. Pomme's
    // analog stick is already clamped to unit length; keyboard input is first
    // normalized just like KeyboardInput.tick(). `strafe` follows vanilla xxa:
    // positive is left, negative is right.
    let (mut strafe, mut forward) = if let Some(analog) = input.get_gamepad_movement_axes() {
        (analog.x, analog.y)
    } else {
        let mut forward = 0.0_f32;
        let mut strafe = 0.0_f32;
        if input.key_pressed(KeyCode::KeyW) {
            forward += 1.0;
        }
        if input.key_pressed(KeyCode::KeyS) {
            forward -= 1.0;
        }
        if input.key_pressed(KeyCode::KeyA) {
            strafe += 1.0;
        }
        if input.key_pressed(KeyCode::KeyD) {
            strafe -= 1.0;
        }
        normalize_vec2(strafe, forward)
    };

    if strafe == 0.0 && forward == 0.0 {
        return (forward, strafe);
    }

    strafe *= INPUT_DAMPING;
    forward *= INPUT_DAMPING;
    strafe *= use_speed_multiplier;
    forward *= use_speed_multiplier;

    if crouching {
        strafe *= sneaking_speed;
        forward *= sneaking_speed;
    }

    let (strafe, forward) = square_movement(strafe, forward);
    (forward, strafe)
}

fn normalize_vec2(x: f32, y: f32) -> (f32, f32) {
    let length = mth_sqrt(x * x + y * y);
    if length < 1.0e-4_f32 {
        return (0.0, 0.0);
    }
    (x / length, y / length)
}

fn square_movement(x: f32, y: f32) -> (f32, f32) {
    let length = mth_sqrt(x * x + y * y);
    if length <= 0.0 {
        return (x, y);
    }
    let inv_len = 1.0_f32 / length;
    let dir_x = x * inv_len;
    let dir_y = y * inv_len;
    let abs_x = dir_x.abs();
    let abs_y = dir_y.abs();
    let tan = if abs_y > abs_x {
        abs_x / abs_y
    } else {
        abs_y / abs_x
    };
    let distance_to_square = mth_sqrt(1.0_f32 + tan * tan);
    let modified_length = (length * distance_to_square).min(1.0_f32);
    (dir_x * modified_length, dir_y * modified_length)
}

fn mth_equal(a: f64, b: f64) -> bool {
    (b - a).abs() < MTH_EQUAL_EPSILON
}

fn mth_sqrt(value: f32) -> f32 {
    f64::from(value).sqrt() as f32
}

fn mth_sin(angle: f32) -> f32 {
    let index = ((f64::from(angle) * SIN_SCALE) as i64 & 0xFFFF) as usize;
    azalea_core::math::SIN[index]
}

fn mth_cos(angle: f32) -> f32 {
    let index = ((f64::from(angle) * SIN_SCALE + 16_384.0) as i64 & 0xFFFF) as usize;
    azalea_core::math::SIN[index]
}

fn vanilla_yaw_sin_cos(yaw_degrees: f32) -> (f32, f32) {
    let angle = yaw_degrees * DEG_TO_RAD;
    (mth_sin(angle), mth_cos(angle))
}

fn vanilla_look_y(pitch_degrees: f32) -> f64 {
    let angle = pitch_degrees * DEG_TO_RAD;
    -f64::from(mth_sin(angle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{
        CROUCH_EYE_HEIGHT, CROUCH_HEIGHT, PLAYER_HALF_WIDTH, STANDING_EYE_HEIGHT, STANDING_HEIGHT,
    };

    fn flat_floor() -> ChunkStore {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.partial_storage.set(
            &azalea_core::position::ChunkPos::new(0, 0),
            Some(azalea_world::chunk::Chunk::default()),
            &mut chunks.chunk_storage,
        );
        for x in 0..16 {
            for z in 0..16 {
                chunks.set_block_state(
                    x,
                    60,
                    z,
                    crate::world::block::first_state_of("stone").unwrap(),
                );
            }
        }
        chunks
    }

    fn sparse_world(blocks: &[(i32, i32, i32, &str)]) -> ChunkStore {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        let mut loaded = std::collections::HashSet::new();
        for &(x, y, z, id) in blocks {
            let pos = azalea_core::position::ChunkPos::new(x.div_euclid(16), z.div_euclid(16));
            if loaded.insert(pos) {
                chunks.partial_storage.set(
                    &pos,
                    Some(azalea_world::chunk::Chunk::default()),
                    &mut chunks.chunk_storage,
                );
            }
            chunks.set_block_state(x, y, z, crate::world::block::find_state(id, &[]));
        }
        chunks
    }

    #[test]
    fn bounded_entity_observation_is_opt_in_and_preserves_numeric_outcome() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = sparse_world(&[]);
        let entities = [Aabb::block(0, 60, 0); 10];
        for active in [false, true] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(0.5, 61.0, 0.5).into();
            player.velocity.y = -0.08;
            player.observe_collision_shapes = active;
            tick_with_context(
                &mut player,
                &InputState::released(),
                &chunks,
                &entities,
                None,
                1.0,
                false,
            );
            assert_eq!(*player.position, dvec3(0.5, 61.0, 0.5));
            assert_eq!(player.velocity.y, -0.08 * f64::from(VERTICAL_DRAG));
            assert!(player.on_ground);
            let o = player.last_travel_observation;
            assert_eq!(o.entity_collider_count, 10);
            assert_eq!(o.entity_shapes_captured, active);
            assert_eq!(
                o.entity_shapes.iter().flatten().count(),
                if active { 8 } else { 0 }
            );
        }
    }

    #[test]
    fn full_tick_pose_uses_block_entity_and_border_clearance_and_swimming_fallback() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = flat_floor();
        let neutral = InputState::released();
        for (ceiling, expected) in [
            (1.6, EntityPose::Crouching),
            (1.0, EntityPose::Swimming),
            (0.4, EntityPose::Standing),
        ] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(4.5, 61.0, 4.5).into();
            player.velocity.y = -0.08;
            let entity = Aabb::new(dvec3(4.0, 61.0 + ceiling, 4.0), dvec3(5.0, 64.0, 5.0));
            tick_with_context(&mut player, &neutral, &chunks, &[entity], None, 1.0, false);
            assert_eq!(player.pose, expected, "ceiling={ceiling}");
            assert_eq!(
                player.height(),
                LocalPlayer::dimensions_for_pose(expected).1
            );
            assert_eq!(
                player.swimming, false,
                "crawling does not set the swim movement flag"
            );
            if expected == EntityPose::Swimming {
                assert_eq!(
                    player.target_eye_height(),
                    crate::player::SWIMMING_EYE_HEIGHT
                );
                tick_with_context(&mut player, &neutral, &chunks, &[entity], None, 1.0, false);
                assert!(!player.bounding_box().intersects(&entity));
            }
        }
        let chunks = sparse_world(&[(4, 60, 4, "stone"), (4, 62, 4, "stone")]);
        let mut player = LocalPlayer::new();
        player.position = dvec3(4.5, 61.0, 4.5).into();
        player.velocity.y = -0.08;
        tick(&mut player, &neutral, &chunks, 1.0, false);
        assert_eq!(player.pose, EntityPose::Swimming);
        tick(&mut player, &neutral, &chunks, 1.0, false);
        assert!(player.on_ground);
        assert_eq!(player.height(), crate::player::SWIMMING_HEIGHT);

        let chunks = flat_floor();
        player.position = dvec3(4.9, 61.0, 4.5).into();
        player.pose = EntityPose::Crouching;
        player.velocity.y = -0.08;
        tick_with_context(
            &mut player,
            &neutral,
            &chunks,
            &[],
            Some([0.0, 5.0, 0.0, 16.0]),
            1.0,
            false,
        );
        assert_eq!(
            player.pose,
            EntityPose::Crouching,
            "blocked swimming retains the current metadata pose"
        );
        assert!(!can_fit_pose(
            &player,
            &chunks,
            EntityPose::Standing,
            &[],
            Some([0.0, 5.0, 0.0, 16.0]),
            false
        ));
    }

    #[test]
    fn full_tick_sneak_uses_remaining_fall_distance_attribute_step_and_entity_support() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = sparse_world(&[(0, 60, 0, "stone")]);
        let mut shift = InputState::released();
        shift.set_test_key(KeyCode::ShiftLeft, true);
        for (fall, ground, step, expected_x) in [
            (0.4, false, 0.6, 1.0),
            (0.1, false, 0.6, 0.75),
            (0.4, true, 0.6, 0.75),
            (0.1, false, 0.2, 1.0),
        ] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(0.5, 61.3, 0.5).into();
            player.velocity = crate::entity::components::Velocity::new(1.0, -0.01, 0.0);
            player.fall_distance = fall;
            player.on_ground = ground;
            player.set_attribute_value("step_height", step);
            tick(&mut player, &shift, &chunks, 1.0, false);
            assert!(
                (player.collision_delta[0].x - expected_x).abs() < 1.0e-12,
                "fall={fall} ground={ground} step={step}: {:?}",
                player.collision_delta
            );
        }
        // Entity.fallDistance is a double; rounding to float would erase the
        // just-below-maxDownStep case before isAboveGround can inspect it.
        for (fall, expected_x) in [
            (f64::from(STEP_HEIGHT) - 1.0e-9, 0.75),
            (f64::from(STEP_HEIGHT), 1.0),
        ] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(0.5, 61.0, 0.5).into();
            player.fall_distance = fall;
            player.velocity = crate::entity::components::Velocity::new(1.0, -0.08, 0.0);
            tick(&mut player, &shift, &chunks, 1.0, false);
            assert!((player.collision_delta[0].x - expected_x).abs() < 1.0e-12);
        }
        let chunks = sparse_world(&[]);
        let support = Aabb::block(0, 60, 0);
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.5, 61.0, 0.5).into();
        player.velocity = crate::entity::components::Velocity::new(1.0, -0.08, 0.0);
        player.on_ground = true;
        tick_with_context(&mut player, &shift, &chunks, &[support], None, 1.0, false);
        assert!((player.collision_delta[0].x - 0.75).abs() < 1.0e-12);
        assert!(player.on_ground);
        assert_eq!(
            player.main_supporting_block_pos, None,
            "entity ground must not invent a block support"
        );
    }

    #[test]
    fn full_tick_equipment_powder_snow_context_reaches_ground_and_support_queries() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = sparse_world(&[(0, 60, 0, "powder_snow")]);
        for boots in [false, true] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(0.5, 61.0, 0.5).into();
            player.velocity.y = -0.08;
            if boots {
                player.inventory.set_slot(
                    8,
                    azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
                        azalea_registry::builtin::ItemKind::LeatherBoots,
                        1,
                    )),
                );
            }
            let bb = player.bounding_box();
            assert_eq!(
                can_fall_at_least(&chunks, &bb, 0.0, 0.0, 0.2, &[], None, (false, boots, 0.0)),
                !boots
            );
            assert!(
                can_fall_at_least(&chunks, &bb, 0.0, 0.0, 0.2, &[], None, (true, boots, 0.0)),
                "descending disables boot support"
            );
            tick(&mut player, &InputState::released(), &chunks, 1.0, false);
            assert_eq!(player.on_ground, boots);
            assert_eq!(player.main_supporting_block_pos.is_some(), boots);
        }
    }

    #[test]
    fn full_tick_edge_support_selects_ice_friction_at_positive_and_negative_coordinates() {
        let _protocol = crate::world::block::test_protocol_guard();
        for (bx, x) in [(0, 1.05), (-1, -1.05)] {
            for (id, friction) in [("ice", 0.98_f32), ("stone", 0.6_f32)] {
                let chunks = sparse_world(&[(bx, 60, 0, id)]);
                let mut player = LocalPlayer::new();
                player.position = dvec3(x, 61.0, 0.5).into();
                player.velocity.y = -0.08;
                let neutral = InputState::released();
                tick(&mut player, &neutral, &chunks, 1.0, false);
                assert_eq!(
                    player.main_supporting_block_pos,
                    Some(azalea_core::position::BlockPos::new(bx, 60, 0))
                );
                assert_eq!(player_block_friction(&chunks, &player).0, friction);
                player.velocity.x = 0.1;
                tick(&mut player, &neutral, &chunks, 1.0, false);
                assert_eq!(
                    player.velocity.x,
                    0.1 * f64::from(friction * HORIZONTAL_DRAG)
                );
                let o = player.last_travel_observation;
                assert_eq!(o.friction, Some(friction));
                assert_eq!(o.ground_drag, Some(friction * HORIZONTAL_DRAG));
                assert_eq!(
                    o.friction_pos,
                    Some(azalea_core::position::BlockPos::new(bx, 60, 0))
                );
                assert_eq!(block_friction(&chunks, player.prev_position), 0.6);
                assert_eq!(o.ground_decision, Some(player.on_ground));
                assert_eq!(o.bbox_after.unwrap().min, player.bounding_box().min);
            }
        }
    }

    #[test]
    fn full_tick_support_uses_previous_horizontal_position_then_clears_when_airborne() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = sparse_world(&[(0, 60, 0, "ice")]);
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.5, 61.0, 0.5).into();
        player.velocity = crate::entity::components::Velocity::new(1.0, -0.08, 0.0);
        let neutral = InputState::released();
        tick(&mut player, &neutral, &chunks, 1.0, false);
        assert!(player.on_ground);
        assert_eq!(
            player.main_supporting_block_pos,
            Some(azalea_core::position::BlockPos::new(0, 60, 0))
        );
        assert_eq!(player_block_friction(&chunks, &player).0, 0.98);
        tick(&mut player, &neutral, &chunks, 1.0, false);
        assert!(!player.on_ground);
        assert_eq!(player.main_supporting_block_pos, None);
    }

    #[test]
    fn supporting_blocks_use_nearest_center_ties_and_no_block_fallback_gate() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = sparse_world(&[(0, 60, 0, "ice"), (1, 60, 0, "stone")]);
        let mut player = LocalPlayer::new();
        player.position = dvec3(1.0, 61.0, 0.5).into();
        player.on_ground = true;
        check_supporting_block(&mut player, &chunks, DVec3::ZERO, (false, false, 0.0));
        assert_eq!(
            player.main_supporting_block_pos,
            Some(azalea_core::position::BlockPos::new(1, 60, 0)),
            "equal-distance Vec3i ordering chooses greater X"
        );
        player.position.x = 0.95;
        check_supporting_block(&mut player, &chunks, DVec3::ZERO, (false, false, 0.0));
        assert_eq!(
            player.main_supporting_block_pos,
            Some(azalea_core::position::BlockPos::new(0, 60, 0))
        );
        player.position.x = 4.0;
        check_supporting_block(&mut player, &chunks, DVec3::ZERO, (false, false, 0.0));
        assert!(player.on_ground_no_blocks);
        check_supporting_block(
            &mut player,
            &chunks,
            dvec3(3.5, 0.0, 0.0),
            (false, false, 0.0),
        );
        assert_eq!(
            player.main_supporting_block_pos, None,
            "already on entity-only ground skips previous-block retry"
        );
    }

    #[test]
    fn full_tick_edge_honey_shares_support_for_speed_and_jump_not_friction() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = sparse_world(&[(0, 60, 0, "honey_block")]);
        let mut player = LocalPlayer::new();
        player.position = dvec3(1.05, 60.9375, 0.5).into();
        player.velocity.y = -0.08;
        tick(&mut player, &InputState::released(), &chunks, 1.0, false);
        assert!(player.main_supporting_block_pos.is_some());
        assert_eq!(player_block_friction(&chunks, &player).0, 0.6);
        assert_eq!(block_movement_factor(&player, &chunks, false), 0.4);
        player.velocity.x = 0.02;
        tick(&mut player, &InputState::released(), &chunks, 1.0, false);
        assert_eq!(
            player.velocity.x,
            0.02 * f64::from(0.4_f32) * f64::from(GROUND_FRICTION)
        );
        let mut jump = InputState::released();
        jump.set_test_key(KeyCode::Space, true);
        tick(&mut player, &jump, &chunks, 1.0, false);
        assert_eq!(player.collision_delta[0].y, f64::from(0.42_f32 * 0.5_f32));
    }

    #[test]
    fn soul_sand_support_shape_has_all_horizontal_faces_but_not_invalid_directions() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let state = crate::world::block::find_state("soul_sand", &[]);
        for direction in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            assert!(crate::world::block::has_full_horizontal_sturdy_face(
                state,
                direction.0,
                direction.1
            ));
        }
        for direction in [(0, 0), (1, 1), (2, 0)] {
            assert!(!crate::world::block::has_full_horizontal_sturdy_face(
                state,
                direction.0,
                direction.1
            ));
        }
        assert_eq!(
            super::super::block_shape::partial_shape(state).unwrap()[0][4],
            0.875
        );
    }

    #[test]
    fn authoritative_native_packet_event_local_player_tick_and_log() {
        let _protocol = crate::world::block::test_protocol_guard();
        use azalea_core::attribute_modifier_operation::AttributeModifierOperation as Op;
        use azalea_inventory::components::AttributeModifier;
        use azalea_protocol::packets::ProtocolPacket;
        use azalea_protocol::packets::game::ClientboundGamePacket as C;
        use azalea_protocol::packets::game::c_update_attributes::{
            AttributeSnapshot, ClientboundUpdateAttributes,
        };
        use azalea_registry::builtin::Attribute;
        let chunks = flat_floor();
        for sprint_id in ["sprinting", "662a6b8dda3e4c1c881396ea6097278d"] {
            let modifiers = vec![
                AttributeModifier {
                    id: "test:add".into(),
                    amount: 0.05,
                    operation: Op::AddValue,
                },
                AttributeModifier {
                    id: "test:base".into(),
                    amount: 0.2,
                    operation: Op::AddMultipliedBase,
                },
                AttributeModifier {
                    id: "test:total".into(),
                    amount: 0.1,
                    operation: Op::AddMultipliedTotal,
                },
                AttributeModifier {
                    id: sprint_id.into(),
                    amount: 0.3_f32 as f64,
                    operation: Op::AddMultipliedTotal,
                },
            ];
            let packet = C::UpdateAttributes(ClientboundUpdateAttributes {
                entity_id: azalea_core::entity_id::MinecraftEntityId(7),
                values: vec![
                    AttributeSnapshot {
                        attribute: Attribute::MovementSpeed,
                        base: 0.2,
                        modifiers: modifiers.clone(),
                    },
                    AttributeSnapshot {
                        attribute: Attribute::JumpStrength,
                        base: 0.6,
                        modifiers: vec![],
                    },
                    AttributeSnapshot {
                        attribute: Attribute::MaxHealth,
                        base: 40.0,
                        modifiers: vec![AttributeModifier {
                            id: "test:health".into(),
                            amount: 0.5,
                            operation: Op::AddMultipliedTotal,
                        }],
                    },
                    AttributeSnapshot {
                        attribute: Attribute::Armor,
                        base: 8.0,
                        modifiers: vec![AttributeModifier {
                            id: "test:armor".into(),
                            amount: 2.0,
                            operation: Op::AddValue,
                        }],
                    },
                ],
            });
            let bytes = azalea_protocol::write::serialize_packet(&packet).unwrap();
            let mut cursor = std::io::Cursor::new(&bytes[..]);
            use azalea_buf::AzBufVar;
            let id = u32::azalea_read_var(&mut cursor).unwrap();
            assert_eq!(id, packet.id());
            let C::UpdateAttributes(decoded) = C::read(id, &mut cursor).unwrap() else {
                panic!("packet")
            };
            let mut player = LocalPlayer::new();
            player.entity_id = 7;
            player.health = 11.0;
            let mut entities = crate::entity::EntityStore::new();
            for snapshot in decoded.values {
                let crate::net::NetworkEvent::EntityAttributeUpdate {
                    entity_id,
                    snapshot,
                } = crate::net::handler::attribute_event(decoded.entity_id.0, snapshot)
                else {
                    panic!("event")
                };
                crate::app::core::apply_entity_attribute(
                    &mut player,
                    &mut entities,
                    entity_id,
                    snapshot,
                );
            }
            assert_eq!(player.attributes["movement_speed"].modifiers, modifiers);
            assert_eq!(
                (player.health, player.max_health, player.armor),
                (11.0, 60.0, 10)
            );
            assert!(
                (player.attribute_value("minecraft:generic.movement_speed", 0.0)
                    - 0.33 * (1.0 + f64::from(0.3_f32)))
                .abs()
                    < 1e-14
            );
            let mut input = InputState::released();
            input.set_test_key(KeyCode::KeyW, true);
            input.set_test_key(KeyCode::ControlLeft, true);
            player.position = dvec3(4.5, 61.0, 4.5).into();
            player.on_ground = true;
            player.velocity.y = -GRAVITY * f64::from(VERTICAL_DRAG);
            tick(&mut player, &input, &chunks, 1.0, false);
            assert!(player.sprinting);
            let speed = (0.33 * (1.0 + 0.3_f32 as f64)) as f32;
            assert_eq!(movement_speed(&player), speed);
            let expected = f64::from(INPUT_DAMPING)
                * f64::from(speed * (GROUND_ACCEL_FACTOR / BLOCK_FRICTION.powi(3)));
            assert!((player.position.z - 4.5 - expected).abs() < 1e-14);
            let log = crate::movement_record::own_attributes(&player);
            assert_eq!(log["movement_speed"]["base"], 0.2);
            assert_eq!(
                log["movement_speed"]["modifiers"].as_array().unwrap().len(),
                4
            );
            assert_eq!(
                log["movement_speed"]["effective"].as_f64().unwrap() as f32,
                movement_speed(&player)
            );
            assert_eq!(
                log["movement_speed"]["modifiers"][0]["operation"],
                "add_value"
            );
            input.set_test_key(KeyCode::Space, true);
            tick(&mut player, &input, &chunks, 1.0, false);
            assert_eq!(player.collision_delta[0].y, f64::from(0.6_f32));
            // Other entity updates must never overwrite the local snapshot.
            crate::app::core::apply_entity_attribute(
                &mut player,
                &mut entities,
                99,
                AttributeSnapshot {
                    attribute: Attribute::MovementSpeed,
                    base: 3.0,
                    modifiers: vec![],
                },
            );
            assert_eq!(player.attributes["movement_speed"].base, 0.2);
            tick(&mut player, &InputState::released(), &chunks, 1.0, false);
            assert!(!player.sprinting);
            assert_eq!(movement_speed(&player), 0.33);
            assert_eq!(
                player.attributes["movement_speed"].modifiers,
                modifiers[..3]
            );
        }
    }

    #[test]
    fn canonical_aliases_and_sprint_state_do_not_stack_or_discard_other_modifiers() {
        let mut player = LocalPlayer::new();
        player.set_attribute_value("minecraft:generic.movement_speed", 0.2);
        assert_eq!(player.attribute_value("minecraft:movement_speed", 0.0), 0.2);
        player.set_attribute_value("minecraft:movement_speed", 0.3);
        assert_eq!(player.attributes.len(), 1);
        assert_eq!(
            player.attribute_value("minecraft:generic.movement_speed", 0.0),
            0.3
        );
        assert_eq!(
            crate::player::canonical_attribute("minecraft:horse.jump_strength"),
            "jump_strength"
        );
        assert_eq!(
            crate::player::canonical_attribute("custom:generic.movement_speed"),
            "custom:generic.movement_speed"
        );
        player.set_sprinting(true);
        assert_eq!(
            movement_speed(&player),
            (0.3 * (1.0 + 0.3_f32 as f64)) as f32
        );
        player.set_sprinting(false);
        assert_eq!(movement_speed(&player), 0.3);
    }

    #[test]
    fn jump_boost_honey_and_zero_power_match_float_jump() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = flat_floor();
        let mut player = LocalPlayer::new();
        player.position = dvec3(4.5, 61.0, 4.5).into();
        player.effects.update(crate::mob_effect::MobEffectInstance {
            effect_id: 7,
            amplifier: 0,
            duration: 200,
            ambient: false,
            show_particles: true,
            show_icon: true,
        });
        player.on_ground = true;
        let mut input = InputState::released();
        input.set_test_key(KeyCode::Space, true);
        tick(&mut player, &input, &chunks, 1.0, false);
        assert_eq!(player.collision_delta[0].y, f64::from(0.42_f32 + 0.1_f32));
        assert_eq!(
            player.last_travel_observation.jump_power,
            Some(0.42_f32 + 0.1_f32)
        );
        assert_eq!(player.last_travel_observation.block_jump_factor, Some(1.0));
        player.position = dvec3(4.5, 61.0, 4.5).into();
        let honey = crate::world::block::first_state_of("honey_block").unwrap();
        for y in [60, 61] {
            chunks.set_block_state(4, y, 4, honey);
            player.velocity = crate::entity::components::Velocity::default();
            jump_from_ground(&mut player, &chunks, 0.0, 1.0);
            assert_eq!(player.velocity.y, f64::from(0.42_f32 * 0.5_f32 + 0.1_f32));
            assert_eq!(player.last_travel_observation.block_jump_factor, Some(0.5));
            assert_eq!(
                f64::from(player.last_travel_observation.jump_power.unwrap()),
                player.velocity.y
            );
        }
        player.effects.clear();
        let stone = crate::world::block::first_state_of("stone").unwrap();
        chunks.set_block_state(4, 60, 4, stone);
        chunks.set_block_state(4, 61, 4, azalea_block::BlockState::AIR);
        player.set_sprinting(true);
        for power in [0.0, f64::from(1.0e-5_f32)] {
            player.set_attribute_value("jump_strength", power);
            player.velocity = crate::entity::components::Velocity::new(0.01, -0.02, 0.03);
            jump_from_ground(&mut player, &chunks, 0.0, 1.0);
            assert_eq!(*player.velocity, dvec3(0.01, -0.02, 0.03));
        }
    }

    #[test]
    fn server_sprint_snapshot_survives_idle_tick_until_set_sprinting() {
        let _protocol = crate::world::block::test_protocol_guard();
        use azalea_core::attribute_modifier_operation::AttributeModifierOperation as Op;
        use azalea_inventory::components::AttributeModifier;
        use azalea_protocol::packets::game::c_update_attributes::AttributeSnapshot;
        let mut player = LocalPlayer::new();
        player.apply_attribute(AttributeSnapshot {
            attribute: azalea_registry::builtin::Attribute::MovementSpeed,
            base: f64::from(0.1_f32),
            modifiers: vec![AttributeModifier {
                id: "minecraft:sprinting".into(),
                amount: f64::from(0.3_f32),
                operation: Op::AddMultipliedTotal,
            }],
        });
        assert!(!player.sprinting);
        let snapshot_speed = (f64::from(0.1_f32) * (1.0 + f64::from(0.3_f32))) as f32;
        assert_eq!(movement_speed(&player), snapshot_speed);
        tick(
            &mut player,
            &InputState::released(),
            &flat_floor(),
            1.0,
            false,
        );
        assert_eq!(movement_speed(&player), snapshot_speed);
        player.set_sprinting(false);
        assert_eq!(movement_speed(&player), 0.1_f32);
    }

    #[test]
    fn tiny_velocity_is_zeroed_before_travel_in_ground_air_water_and_flight() {
        let _protocol = crate::world::block::test_protocol_guard();
        let floor = flat_floor();
        let water = flat_floor();
        water.set_block_state(
            4,
            61,
            4,
            crate::world::block::find_state("water", &[("level", "0")]),
        );
        for (chunks, ground, flying) in [
            (&floor, true, false),
            (&floor, false, false),
            (&water, false, false),
            (&floor, false, true),
        ] {
            for (vx, vz, should_zero) in [
                (0.002193, 0.0, true),
                (0.002, 0.002, true),
                (0.003, 0.0, false),
                (0.0022, 0.0022, false),
            ] {
                let mut player = LocalPlayer::new();
                player.position = dvec3(4.5, 61.0, 4.5).into();
                player.on_ground = ground;
                player.flying = flying;
                player.velocity = crate::entity::components::Velocity::new(vx, 0.00299, vz);
                tick(&mut player, &InputState::released(), chunks, 1.0, false);
                let delta = *player.position - dvec3(4.5, 61.0, 4.5);
                if should_zero {
                    assert_eq!((delta.x, delta.z), (0.0, 0.0));
                } else {
                    assert!((delta.x - vx).abs() < 1e-14 && (delta.z - vz).abs() < 1e-14);
                }
                assert_eq!(player.collision_delta[0].y, 0.0);
            }
        }
        let mut dead = LocalPlayer::new();
        dead.position = dvec3(4.5, 61.0, 4.5).into();
        dead.velocity.x = 0.002193;
        tick_dead(&mut dead, &floor);
        assert_eq!(dead.position.x, 4.5);
        for y in [0.003, -0.003] {
            let mut player = LocalPlayer::new();
            player.velocity = crate::entity::components::Velocity::new(0.0, y, 0.0);
            zero_small_velocity(&mut player);
            assert_eq!(player.velocity.y, y);
        }
    }

    #[test]
    fn wall_collision_ends_sprint_before_the_following_tick_accelerates() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = flat_floor();
        let stone = crate::world::block::first_state_of("stone").unwrap();
        for x in 3..=6 {
            for y in [61, 62] {
                chunks.set_block_state(x, y, 5, stone);
            }
        }
        let mut player = LocalPlayer::new();
        player.position = dvec3(4.5, 61.0, 4.0).into();
        player.on_ground = true;
        let mut input = InputState::released();
        input.set_test_key(KeyCode::KeyW, true);
        input.set_test_key(KeyCode::KeyD, true);
        input.set_test_key(KeyCode::ControlLeft, true);

        let mut contacted_wall = false;
        let mut stopping_tick_speed = None;
        for _ in 0..12 {
            let previous_tick_collided = player.horizontal_collision;
            tick(&mut player, &input, &chunks, 1.0, false);
            contacted_wall |= player.horizontal_collision;
            if previous_tick_collided && !player.sprinting {
                stopping_tick_speed = Some(movement_speed(&player));
            }
        }

        assert!(contacted_wall, "diagonal movement reaches the wall");
        assert_eq!(stopping_tick_speed, Some(0.1_f32), "stop precedes the next tick's travel");
        assert_eq!(movement_speed(&player), 0.1_f32);
    }

    #[test]
    fn default_walk_and_sprint_keep_vanilla_steady_tick_displacement() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = flat_floor();
        for (sprint, expected_bps) in [(false, 4.317177), (true, 5.612330)] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(4.5, 61.0, 4.5).into();
            player.on_ground = true;
            let mut input = InputState::released();
            input.set_test_key(KeyCode::KeyW, true);
            input.set_test_key(KeyCode::ControlLeft, sprint);
            let mut delta = 0.0;
            for _ in 0..60 {
                // Stay over the same flat floor without changing momentum.
                player.position.z = 4.5;
                tick(&mut player, &input, &chunks, 1.0, false);
                delta = player.position.z - 4.5;
            }
            assert!(
                (delta * 20.0 - expected_bps).abs() < 1e-5,
                "{sprint}: {}; grounded={} friction={} vel={:?} pos={:?} speed={}",
                delta * 20.0,
                player.on_ground,
                block_friction(&chunks, player.position),
                player.velocity,
                player.position,
                movement_speed(&player)
            );
        }
    }

    #[test]
    fn stair_step_ground_uses_requested_y_and_next_tick_uses_air_physics_after_jump() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = flat_floor();
        chunks.set_block_state(
            4,
            61,
            5,
            crate::world::block::find_state(
                "stone_brick_stairs",
                &[
                    ("facing", "south"),
                    ("half", "bottom"),
                    ("shape", "straight"),
                    ("waterlogged", "false"),
                ],
            ),
        );
        for jump in [false, true] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(4.5, 61.0, 4.6).into();
            player.on_ground = true;
            player.velocity.y = -GRAVITY * f64::from(VERTICAL_DRAG);
            let mut input = InputState::released();
            input.set_test_key(KeyCode::KeyW, true);
            input.set_test_key(KeyCode::ControlLeft, true);
            input.set_test_key(KeyCode::Space, jump);

            tick(&mut player, &input, &chunks, 1.0, false);
            assert_eq!(
                player.collision_delta[0].y,
                if jump {
                    f64::from(JUMP_VELOCITY)
                } else {
                    -GRAVITY * f64::from(VERTICAL_DRAG)
                },
            );
            assert_eq!(player.collision_delta[1].y, 0.5);
            assert_eq!(player.position.y, 61.5);
            assert_eq!(player.on_ground, !jump);
            assert!(player.sprinting);
            assert!(!player.horizontal_collision);

            let previous_vz = player.velocity.z;
            let accel = if jump {
                SPRINT_AIR_ACCELERATION
            } else {
                movement_speed(&player)
                    * (GROUND_ACCEL_FACTOR / (BLOCK_FRICTION * BLOCK_FRICTION * BLOCK_FRICTION))
            };
            let drag = if jump {
                HORIZONTAL_DRAG
            } else {
                GROUND_FRICTION
            };
            let expected_z = previous_vz + f64::from(INPUT_DAMPING) * f64::from(accel);
            tick(&mut player, &input, &chunks, 1.0, false);
            assert!((player.collision_delta[0].z - expected_z).abs() < 1.0e-14);
            assert!((player.collision_delta[1].z - expected_z).abs() < 1.0e-14);
            assert!((player.velocity.z - expected_z * f64::from(drag)).abs() < 1.0e-14);
            assert_eq!(player.collision_delta[1].y, 0.0);
            assert!(player.on_ground);
        }
    }

    #[test]
    fn player_width_stops_at_negative_two_block_face() {
        let block = Aabb::block(0, 0, -2);
        let player = Aabb::from_center(
            dvec3(0.5, 0.0, block.min.z - PLAYER_HALF_WIDTH),
            PLAYER_HALF_WIDTH,
            STANDING_HEIGHT / 2.0,
        );

        assert_eq!(player.max.z, block.min.z);
        assert_eq!(block.clip_z_collide(&player, 0.1), 0.0);
    }

    #[test]
    fn float_derived_width_reconstructs_problem_block_faces_exactly() {
        // These are the sparse integer boundaries where the old direct f64
        // literal `0.3` could reconstruct one ULP inside the block face.
        for block_coord in [-2, -32, -512, -8192, -131_072, -2_097_152] {
            let face = f64::from(block_coord);
            assert_eq!((face - PLAYER_HALF_WIDTH) + PLAYER_HALF_WIDTH, face);
        }
        for block_coord in [2, 32, 512, 8192, 131_072, 2_097_152] {
            let face = f64::from(block_coord);
            assert_eq!((face + PLAYER_HALF_WIDTH) - PLAYER_HALF_WIDTH, face);
        }
    }

    #[test]
    fn player_dimensions_match_vanilla_float_widening() {
        assert_eq!(PLAYER_HALF_WIDTH.to_bits(), 0x3fd3333340000000);
        assert_eq!(STANDING_HEIGHT.to_bits(), 0x3ffcccccc0000000);
        assert_eq!(CROUCH_HEIGHT.to_bits(), 1.5_f64.to_bits());
        assert_eq!(STANDING_EYE_HEIGHT.to_bits(), 0x3fcf5c29);
        assert_eq!(CROUCH_EYE_HEIGHT.to_bits(), 0x3fa28f5c);
    }

    #[test]
    fn movement_constants_match_vanilla_value_types() {
        assert_eq!(JUMP_VELOCITY.to_bits(), 0x3ed70a3d);
        assert_eq!(HORIZONTAL_DRAG.to_bits(), 0x3f68f5c3);
        assert_eq!(BLOCK_FRICTION.to_bits(), 0x3f19999a);
        assert_eq!(GROUND_FRICTION.to_bits(), 0x3f0bc6a9);
        assert_eq!(GROUND_ACCEL_FACTOR.to_bits(), 0x3e5d2f1c);
        assert_eq!(fluid_falling_adjusted(0.0125, 0.2, true, false), -0.003);
        assert_eq!(fluid_falling_adjusted(0.0, GRAVITY, false, false), -0.005);
        assert_eq!(MTH_EQUAL_EPSILON.to_bits(), 0x3ee4f8b580000000);
        assert_eq!(MINOR_COLLISION_ANGLE.to_bits(), 0x3fc1df46a0000000);
    }

    #[test]
    fn movement_speed_matches_vanilla_attribute_rounding() {
        let mut player = LocalPlayer::new();
        assert_eq!(movement_speed(&player).to_bits(), 0x3dcccccd);
        player.set_sprinting(true);
        assert_eq!(movement_speed(&player).to_bits(), 0x3e051eb9);

        let mut player = LocalPlayer::new();
        player.on_ground = true;
        assert_eq!(
            friction_influenced_speed(movement_speed(&player), &player, BLOCK_FRICTION).to_bits(),
            (movement_speed(&player)
                * (GROUND_ACCEL_FACTOR / (BLOCK_FRICTION * BLOCK_FRICTION * BLOCK_FRICTION)))
                .to_bits()
        );
        player.sprinting = true;
        assert_eq!(
            friction_influenced_speed(movement_speed(&player), &player, BLOCK_FRICTION).to_bits(),
            (movement_speed(&player)
                * (GROUND_ACCEL_FACTOR / (BLOCK_FRICTION * BLOCK_FRICTION * BLOCK_FRICTION)))
                .to_bits()
        );
    }

    #[test]
    fn effective_movement_attributes_override_vanilla_defaults() {
        let _protocol = crate::world::block::test_protocol_guard();
        let mut player = LocalPlayer::new();
        player.set_attribute_value("minecraft:generic.movement_speed", 0.2);
        player.set_attribute_value("minecraft:generic.gravity", 0.04);
        player.set_attribute_value("minecraft:generic.jump_strength", 0.6);
        assert_eq!(movement_speed(&player), 0.2);
        player.set_sprinting(true);
        assert_eq!(movement_speed(&player), 0.26);
        assert_eq!(effective_gravity(&player), 0.04);
        let (sin, cos) = vanilla_yaw_sin_cos(0.0);
        jump_from_ground(&mut player, &flat_floor(), sin, cos);
        assert_eq!(player.velocity.y, f64::from(0.6_f32));
    }

    #[test]
    fn keyboard_input_math_matches_vanilla_float_pipeline() {
        let (left, forward) = normalize_vec2(1.0, 1.0);
        let (left, forward) = square_movement(left * INPUT_DAMPING, forward * INPUT_DAMPING);
        assert_eq!(left.to_bits(), 0x3f3504f2);
        assert_eq!(forward.to_bits(), 0x3f3504f2);

        let (left, forward) = normalize_vec2(0.0, 1.0);
        let (left, forward) = square_movement(left * INPUT_DAMPING, forward * INPUT_DAMPING);
        assert_eq!(left.to_bits(), 0x00000000);
        assert_eq!(forward.to_bits(), 0x3f7ae148);
    }

    #[test]
    fn gamepad_horizontal_axis_matches_vanilla_strafe_sign() {
        let analog = input::gamepad_movement_axes(glam::vec2(-1.0, 0.0));
        assert_eq!(analog, glam::vec2(1.0, 0.0));
        let (sin, cos) = vanilla_yaw_sin_cos(0.0);
        let (dx, dz) = movement_delta(analog.y, analog.x, 1.0, sin, cos);
        assert_eq!((dx, dz), (1.0, 0.0));

        let analog = input::gamepad_movement_axes(glam::vec2(1.0, 0.0));
        assert_eq!(analog, glam::vec2(-1.0, 0.0));
        let (dx, dz) = movement_delta(analog.y, analog.x, 1.0, sin, cos);
        assert_eq!((dx, dz), (-1.0, 0.0));

        assert_eq!(
            input::gamepad_movement_axes(glam::vec2(-0.6, 0.8)),
            glam::vec2(0.6, 0.8)
        );
    }

    /// A float direction that widens to just over unit length takes the
    /// normalize branch, where a reciprocal multiply lands one ULP off.
    #[test]
    fn over_unit_input_normalizes_like_vanilla() {
        let (sin, cos) = vanilla_yaw_sin_cos(0.0);
        let strafe = f32::from_bits(0x3f7ffb1c);
        let forward = f32::from_bits(0x3c4829d1);
        let (dx, dz) = movement_delta(forward, strafe, 1.0, sin, cos);
        assert_eq!(dx.to_bits(), 0x3fefff637d23f861);
        assert_eq!(dz.to_bits(), 0x3f89053a1dc39789);
    }

    #[test]
    fn vanilla_mth_trig_and_movement_rotation_match_java_bits() {
        let (sin, cos) = vanilla_yaw_sin_cos(30.0);
        assert_eq!(sin.to_bits(), 0x3efffc5f);
        assert_eq!(cos.to_bits(), 0x3f5db4e3);

        let (sin, cos) = vanilla_yaw_sin_cos(45.0);
        assert_eq!(sin.to_bits(), 0x3f3504f3);
        assert_eq!(cos.to_bits(), 0x3f3504f3);

        let forward = f32::from_bits(0x3f3504f2);
        let (dx, dz) = movement_delta(forward, 0.0, movement_speed(&LocalPlayer::new()), sin, cos);
        assert_eq!(dx.to_bits(), 0xbfa999996d18578d);
        assert_eq!(dz.to_bits(), 0x3fa999996d18578d);

        assert_eq!(vanilla_look_y(30.0).to_bits(), 0xbfdfff8be0000000);
    }

    #[test]
    fn lava_travel_uses_deep_fluid_drag_and_gravity() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.5, 10.0, 0.5).into();
        player.velocity = crate::entity::components::Velocity::new(0.2, 0.2, 0.0);
        player.in_lava = true;
        player.lava_height = 0.8;
        let chunks = ChunkStore::new(1);

        tick_lava(
            &mut player,
            &InputState::released(),
            &chunks,
            &[],
            None,
            0.0,
            0.0,
            0.0,
            1.0,
        );

        assert_eq!(player.position.x, 0.7);
        assert_eq!(player.velocity.x, 0.1);
        assert_eq!(player.velocity.y, 0.08);
    }

    #[test]
    fn friction_reads_vanilla_block_property_values() {
        for (id, expected) in [
            ("stone", 0.6),
            ("honey_block", 0.6),
            ("soul_sand", 0.6),
            ("ice", 0.98),
            ("packed_ice", 0.98),
            ("blue_ice", 0.989),
            ("slime_block", 0.8),
        ] {
            assert_eq!(friction_for_block_id(id), expected);
        }
        let mut player = LocalPlayer::new();
        player.on_ground = true;
        let speed = movement_speed(&player);
        assert!(friction_influenced_speed(speed, &player, 0.4) > speed);
    }

    #[test]
    fn fall_flying_travel_applies_official_glide_and_drag() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut player = LocalPlayer::new();
        player.look_dir = crate::entity::components::LookDirection::new(0.0, -30.0);
        player.velocity = crate::entity::components::Velocity::new(0.0, -0.1, 0.0);
        let before_y = player.velocity.y;
        tick_fall_flying(
            &mut player,
            &InputState::released(),
            &ChunkStore::new(1),
            &[],
            None,
            0.0,
            0.0,
            0.0,
            1.0,
        );
        assert!(
            player.velocity.y > before_y - GRAVITY,
            "upward-look lift must offset part of the gravity step"
        );
        assert_eq!(player.velocity.x, 0.0);
    }

    #[test]
    fn spectator_move_bypasses_block_collision() {
        let mut player = LocalPlayer::new();
        player.game_mode = 3;
        player.fall_distance = 4.0;
        player.position = dvec3(0.5, 0.0, 0.5).into();
        player.velocity = crate::entity::components::Velocity::new(0.0, 0.0, 1.0);
        let chunks = ChunkStore::new(1);

        apply_collision(
            &mut player,
            &InputState::released(),
            &chunks,
            0.0,
            0.0,
            0.0,
            1.0,
        );

        assert_eq!(player.position.z, 1.5);
        assert!(!player.on_ground);
        assert!(!player.horizontal_collision);
        assert_eq!(player.fall_distance, 4.0);
    }

    #[test]
    fn fluid_blocking_and_full_face_support_are_distinct_from_collision() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let stone = crate::world::block::find_state("stone", &[]);
        let cobweb = crate::world::block::find_state("cobweb", &[]);
        assert!(crate::world::block::blocks_motion(stone));
        assert!(!crate::world::block::blocks_motion(cobweb));
        assert!(crate::world::block::has_full_horizontal_sturdy_face(
            stone, 1, 0
        ));
        assert!(!crate::world::block::has_full_horizontal_sturdy_face(
            crate::world::block::find_state(
                "oak_slab",
                &[("type", "bottom"), ("waterlogged", "false")]
            ),
            1,
            0,
        ));
    }

    #[test]
    fn fall_distance_cap_is_only_for_fall_flying_and_effects_reset_it() {
        let mut player = LocalPlayer::new();
        player.velocity.y = 0.0;
        player.fall_distance = 4.0;
        reset_fall_distance_for_tick(&mut player);
        assert_eq!(player.fall_distance, 4.0);
        player.fall_flying = true;
        reset_fall_distance_for_tick(&mut player);
        assert_eq!(player.fall_distance, 1.0);
        player.effects.update(crate::mob_effect::MobEffectInstance {
            effect_id: crate::mob_effect::MOB_EFFECTS
                .iter()
                .position(|effect| effect.name == "slow_falling")
                .unwrap() as u32,
            amplifier: 0,
            duration: 10,
            ambient: false,
            show_particles: true,
            show_icon: true,
        });
        player.fall_distance = 4.0;
        reset_fall_distance_for_tick(&mut player);
        assert_eq!(player.fall_distance, 0.0);
    }

    #[test]
    fn honey_slide_matches_vanilla_speed_throttle() {
        let (horizontal_scale, vertical) = honey_slide_movement(-0.2744);
        assert!((horizontal_scale - 0.25).abs() < 1.0e-6);
        assert!((vertical - (-0.1274)).abs() < 1.0e-6);
        assert_eq!(honey_slide_movement(-0.1).0, 1.0);
    }

    #[test]
    fn fall_distance_helper_tracks_only_resolved_downward_movement() {
        let mut fall_distance = 1.0;
        update_fall_distance(&mut fall_distance, -0.75, false, false);
        assert_eq!(fall_distance, 1.75);
        let mut accumulated = 0.0;
        for _ in 0..10 {
            update_fall_distance(&mut accumulated, -0.01, false, false);
        }
        assert_eq!(accumulated, 10.0 * f64::from(0.01_f32));
        update_fall_distance(&mut fall_distance, 0.5, false, false);
        update_fall_distance(&mut fall_distance, 0.0, false, false);
        assert_eq!(fall_distance, 1.75);
        update_fall_distance(&mut fall_distance, 0.0, false, true);
        assert_eq!(fall_distance, 0.0);
        fall_distance = 2.0;
        update_fall_distance(&mut fall_distance, 0.0, true, false);
        assert_eq!(fall_distance, 0.0);
    }

    #[test]
    fn apply_collision_clips_fall_at_floor_and_resets_grounded_distance() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.5, 61.0, 0.5).into();
        player.fall_distance = 5.0;
        player.velocity = crate::entity::components::Velocity::new(0.0, -2.0, 0.0);
        let mut chunks = ChunkStore::new(1);
        let _floor_chunk = chunks.chunk_storage.upsert(
            azalea_core::position::ChunkPos::new(0, 0),
            azalea_world::chunk::Chunk::default(),
        );
        chunks.set_block_state(
            0,
            60,
            0,
            crate::world::block::first_state_of("stone").unwrap(),
        );

        apply_collision(
            &mut player,
            &InputState::released(),
            &chunks,
            0.0,
            0.0,
            0.0,
            1.0,
        );

        assert_eq!(player.position.y, 61.0);
        assert!(player.on_ground);
        assert_eq!(player.fall_distance, 0.0);
    }

    #[test]
    fn tick_and_dead_tick_preserve_fall_distance_during_flight_and_reset_for_effects() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let input = InputState::released();
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.5, 100.0, 0.5).into();
        player.flying = true;
        player.fall_distance = 8.0;
        tick(&mut player, &input, &chunks, 1.0, false);
        assert_eq!(player.fall_distance, 8.0);
        player.position = dvec3(0.5, 100.0, 0.5).into();
        player.velocity = crate::entity::components::Velocity::new(0.0, 0.0, 0.0);
        player.on_ground = false;
        player.fall_distance = 8.0;
        tick_dead(&mut player, &chunks);
        assert_eq!(player.fall_distance, 8.0);
        player.flying = false;

        for (name, duration) in [("slow_falling", 0), ("levitation", -1)] {
            let effect_id = crate::mob_effect::MOB_EFFECTS
                .iter()
                .position(|effect| effect.name == name)
                .unwrap() as u32;
            player.effects.update(crate::mob_effect::MobEffectInstance {
                effect_id,
                amplifier: 0,
                duration,
                ambient: false,
                show_particles: true,
                show_icon: true,
            });
            player.position = dvec3(0.5, 100.0, 0.5).into();
            player.velocity = crate::entity::components::Velocity::new(0.0, 0.0, 0.0);
            player.on_ground = false;
            player.fall_distance = 8.0;
            tick(&mut player, &input, &chunks, 1.0, false);
            assert_eq!(
                player.fall_distance, 0.0,
                "normal {name} duration={duration}"
            );
            player.position = dvec3(0.5, 100.0, 0.5).into();
            player.velocity = crate::entity::components::Velocity::new(0.0, 0.0, 0.0);
            player.on_ground = false;
            player.fall_distance = 8.0;
            tick_dead(&mut player, &chunks);
            assert_eq!(player.fall_distance, 0.0, "dead {name} duration={duration}");
            player.effects.remove(effect_id);
            player.position = dvec3(0.5, 100.0, 0.5).into();
            player.velocity = crate::entity::components::Velocity::new(0.0, 0.0, 0.0);
            player.on_ground = false;
            player.fall_distance = 8.0;
            tick(&mut player, &input, &chunks, 1.0, false);
            assert_eq!(player.fall_distance, 8.0, "removed {name}");
        }
    }

    #[test]
    fn climbable_post_move_impulse_matches_steel_even_while_sneaking() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = sparse_world(&[(0, 64, 0, "ladder"), (1, 64, 0, "stone")]);
        let ladder = crate::world::block::find_state("ladder", &[("facing", "north")]);
        let mut chunks = chunks;
        chunks.set_block_state(0, 64, 0, ladder);

        let mut colliding = LocalPlayer::new();
        colliding.position = dvec3(0.6, 64.0, 0.5).into();
        colliding.velocity.x = 1.0;
        apply_collision_with_context(
            &mut colliding,
            &InputState::released(),
            &chunks,
            &[],
            None,
            1.0,
            0.0,
            0.0,
            1.0,
        );
        assert!(colliding.horizontal_collision);
        assert_eq!(colliding.velocity.y, 0.2, "W-only wall contact climbs");

        let mut jumping = LocalPlayer::new();
        jumping.position = dvec3(0.5, 64.0, 0.5).into();
        let mut jump = InputState::released();
        jump.set_test_key(KeyCode::Space, true);
        apply_collision_with_context(&mut jumping, &jump, &chunks, &[], None, 0.0, 0.0, 0.0, 1.0);
        assert!(!jumping.horizontal_collision);
        assert_eq!(jumping.velocity.y, 0.2, "jump climbs without a wall");

        let mut no_wall = LocalPlayer::new();
        no_wall.position = dvec3(0.5, 64.0, 0.5).into();
        no_wall.velocity.x = 0.1;
        apply_collision_with_context(
            &mut no_wall,
            &InputState::released(),
            &chunks,
            &[],
            None,
            1.0,
            0.0,
            0.0,
            1.0,
        );
        assert!(!no_wall.horizontal_collision);
        assert_eq!(
            no_wall.velocity.y, 0.0,
            "walking without contact does not climb"
        );

        let mut sneaking = LocalPlayer::new();
        // Start one tenth of a block from the adjacent wall; ladder travel
        // clamps horizontal movement to 0.15 before collision resolution.
        sneaking.position = dvec3(0.6, 64.0, 0.5).into();
        sneaking.velocity.x = 1.0;
        let mut sneak = InputState::released();
        sneak.set_test_key(KeyCode::ShiftLeft, true);
        apply_collision_with_context(
            &mut sneaking,
            &sneak,
            &chunks,
            &[],
            None,
            1.0,
            0.0,
            0.0,
            1.0,
        );
        assert!(sneaking.horizontal_collision);
        assert_eq!(
            sneaking.velocity.y, 0.2,
            "Steel applies post-move climb impulse while sneaking"
        );
    }

    #[test]
    fn bubble_column_effect_visits_each_swept_cell_once_and_respects_velocity_limits() {
        let _protocol = crate::world::block::test_protocol_guard();
        assert_eq!(bubble_column_velocity(0.68, false, false), 0.7);
        assert_eq!(bubble_column_velocity(0.69, false, false), 0.7);
        assert_eq!(bubble_column_velocity(-0.28, true, false), -0.3);
        assert_eq!(bubble_column_velocity(1.75, false, true), 1.8);
        assert_eq!(bubble_column_velocity(-0.88, true, true), -0.9);
        assert_eq!(bubble_column_velocity(0.0, false, false), 0.06);
        assert_eq!(bubble_column_velocity(0.0, true, false), -0.03);
        assert_eq!(bubble_column_velocity(0.0, false, true), 0.1);
        assert_eq!(bubble_column_velocity(0.0, true, true), -0.03);

        crate::world::block::init("26.2");
        let mut chunks = sparse_world(&[
            (0, 64, 0, "bubble_column"),
            (0, 65, 0, "bubble_column"),
            (0, 66, 0, "bubble_column"),
        ]);
        let upward = crate::world::block::find_state("bubble_column", &[("drag", "false")]);
        for y in 64..=66 {
            chunks.set_block_state(0, y, 0, upward);
        }
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.5, 64.0, 0.5).into();
        player.velocity = crate::entity::components::Velocity::new(0.2, 0.0, -0.3);
        player.fall_distance = 5.0;
        let position = *player.position;
        apply_bubble_column_effect(&mut player, &chunks, position, position);
        assert_eq!(player.velocity.x, 0.2);
        assert_eq!(
            player.velocity.y, 0.12,
            "both intersected stacked cells apply"
        );
        assert_eq!(player.velocity.z, -0.3);
        assert_eq!(player.fall_distance, 0.0);

        player.position = dvec3(2.0, 64.0, 0.5).into();
        player.velocity.y = 0.0;
        player.fall_distance = 5.0;
        let position = *player.position;
        apply_bubble_column_effect(&mut player, &chunks, position, position);
        assert_eq!(player.velocity.y, 0.0, "nonintersecting AABB has no effect");
        assert_eq!(player.fall_distance, 5.0);

        player.position = dvec3(0.5, 64.0, 0.5).into();
        player.flying = true;
        let position = *player.position;
        apply_bubble_column_effect(&mut player, &chunks, position, position);
        assert_eq!(player.velocity.y, 0.0, "flying player is excluded");
        assert_eq!(player.fall_distance, 5.0);

        let mut mixed = sparse_world(&[
            (0, 64, 0, "bubble_column"),
            (0, 65, 0, "bubble_column"),
            (0, 66, 0, "bubble_column"),
        ]);
        mixed.set_block_state(0, 64, 0, upward);
        mixed.set_block_state(
            0,
            65,
            0,
            crate::world::block::find_state("bubble_column", &[("drag", "true")]),
        );
        mixed.set_block_state(0, 66, 0, upward);
        let mut traversing = LocalPlayer::new();
        traversing.position = dvec3(0.5, 64.0, 0.5).into();
        traversing.velocity.y = 4.5;
        traversing.fall_distance = 5.0;
        tick(&mut traversing, &InputState::released(), &mixed, 1.0, false);
        assert!(
            traversing.position.y > 68.0,
            "fast movement crosses the full column"
        );
        // First cell caps to 0.7, the drag cell lowers it to 0.67, and
        // the open-top cell raises it to 0.77 before water drag/gravity.
        let expected_vy = (0.7 - 0.03 + 0.1) * f64::from(WATER_VERTICAL_DRAG) - GRAVITY / 16.0;
        assert!(
            (traversing.velocity.y - expected_vy).abs() < 1.0e-12,
            "velocity={} expected={expected_vy}",
            traversing.velocity.y
        );
        assert_eq!(
            traversing.fall_distance, 0.0,
            "inside hooks reset fall distance"
        );

        assert!(
            swept_aabb_entry(
                dvec3(0.5, 64.0, 0.5),
                dvec3(2.5, 64.0, 2.5),
                dvec3(0.3, 0.9, 0.3),
                [0, 64, 2],
            )
            .is_none(),
            "diagonal swept bounds do not hit an untraversed corner"
        );
    }

    #[test]
    fn levitation_moves_before_adjusting_velocity_and_removal_restores_gravity() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let input = InputState::released();
        let levitation = crate::mob_effect::MOB_EFFECTS
            .iter()
            .position(|effect| effect.name == "levitation")
            .unwrap() as u32;
        for amplifier in [0, 1] {
            let mut player = LocalPlayer::new();
            player.position = dvec3(0.5, 100.0, 0.5).into();
            player.effects.update(crate::mob_effect::MobEffectInstance {
                effect_id: levitation,
                amplifier,
                duration: 100,
                ambient: false,
                show_particles: true,
                show_icon: true,
            });
            tick(&mut player, &input, &chunks, 1.0, false);
            assert_eq!(
                player.position.y, 100.0,
                "first-tick movement uses initial vy=0"
            );
            // Levitation changes velocity 20% toward its target before drag.
            let target_vy = 0.05 * (f64::from(amplifier) + 1.0) * 0.2 * f64::from(VERTICAL_DRAG);
            assert!(
                (player.velocity.y - target_vy).abs() < 1.0e-12,
                "amplifier={amplifier} velocity={} target={target_vy}",
                player.velocity.y
            );
            let first_vy = player.velocity.y;
            tick(&mut player, &input, &chunks, 1.0, false);
            assert!((player.position.y - 100.0 - first_vy).abs() < 1.0e-12);
            assert!(player.velocity.y > 0.0);
            player.effects.remove(levitation);
            player.velocity.y = 0.0;
            tick(&mut player, &input, &chunks, 1.0, false);
            assert!(player.velocity.y < 0.0, "gravity resumes after removal");
        }
    }

    #[test]
    fn dead_player_keeps_zero_input_air_travel() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.0, 80.0, 0.0).into();
        player.velocity = crate::entity::components::Velocity::new(0.25, 0.0, -0.1);
        player.sprinting = true;
        player.crouching = true;
        player.pose = EntityPose::Crouching;
        player.eye_height = 1.27;
        player.prev_eye_height = 1.27;
        let chunks = ChunkStore::new(2);

        player.death_time = 1;
        let starting_height = player.height();
        tick_dead(&mut player, &chunks);

        assert_eq!(
            starting_height, CROUCH_HEIGHT,
            "death must begin from the player's existing ordinary pose"
        );

        assert!(
            !player.crouching,
            "the first dead tick must end by selecting the neutral-input pose"
        );
        assert_eq!(
            player.eye_height, 1.27,
            "the first dead tick must still use the pre-tick crouching eye height"
        );
        player.death_time = 2;
        tick_dead(&mut player, &chunks);
        assert!(
            player.eye_height > 1.27,
            "the next dead tick must smooth toward the neutral standing pose"
        );
        assert!(
            player.position.x > 0.0,
            "dead-player momentum must still move the corpse"
        );
        assert!(
            player.position.z < 0.0,
            "dead-player momentum must still move the corpse"
        );
        assert!(
            player.position.y <= 80.0,
            "dead-player travel must continue applying gravity"
        );
        assert!(
            player.velocity.y < 0.0,
            "dead-player travel must retain downward gravity/drag"
        );
        assert!(
            !player.sprinting,
            "immobile dead-player input must stop sprinting"
        );
    }
}
