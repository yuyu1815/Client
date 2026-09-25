// Fall distance is tracked locally for movement parity; health damage remains
// server-authoritative and is intentionally not predicted here.

use glam::{DVec3, dvec3};
use winit::keyboard::KeyCode;

use super::aabb::Aabb;
use super::collision::no_collision;
use crate::app::input::{self, InputState};
use crate::player::{CROUCH_HEIGHT, LocalPlayer, PLAYER_HALF_WIDTH, STANDING_HEIGHT};
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
const SPRINT_SPEED_MODIFIER: f64 = 0.3_f32 as f64;
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
    let jump_held = input.performing_action(input::Action::Jump);

    // Vanilla `LivingEntity.aiStep`.
    if player.no_jump_delay > 0 {
        player.no_jump_delay -= 1;
    }

    player.update_water_state(chunk_store);
    apply_fluid_currents(player, chunk_store);
    reset_fall_distance_for_tick(player);
    update_crouch_state(player, input, chunk_store);
    player.tick_eye_height();

    // Vanilla `LocalPlayer.modifyInput` keeps the entire input pipeline in
    // float: damping, item-use slowdown, sneaking slowdown, then square remap.
    let (forward, strafe) = movement_input(input, player.crouching, use_speed_multiplier);
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

    update_fly_state(player, input, sin_y_rot, cos_y_rot);

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
            jump_from_ground(player, sin_y_rot, cos_y_rot);
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
    player.no_jump_delay = 0;
    player.sprinting = false;

    // Local players enter death through SetHealth; entity event 3 intentionally
    // skips LivingEntity.die for players, so the current ordinary player pose
    // remains authoritative until Player.updatePlayerPose runs at tick end.
    let neutral = InputState::released();
    player.update_water_state(chunk_store);
    apply_fluid_currents(player, chunk_store);
    reset_fall_distance_for_tick(player);
    player.tick_eye_height();

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
    update_crouch_state(player, &neutral, chunk_store);

    stop_flying_on_ground(player);
    player.was_forward_pressed = false;
    player.was_jump_pressed = false;
}

// Vanilla `LocalPlayer.aiStep`: a fresh jump press arms the toggle window;
// a second one inside it toggles flight.
fn update_fly_state(player: &mut LocalPlayer, input: &InputState, sin_y_rot: f32, cos_y_rot: f32) {
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
                    jump_from_ground(player, sin_y_rot, cos_y_rot);
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

fn jump_from_ground(player: &mut LocalPlayer, sin_y_rot: f32, cos_y_rot: f32) {
    let jump = player.attribute_value("minecraft:generic.jump_strength", f64::from(JUMP_VELOCITY));
    player.velocity.y = jump.max(player.velocity.y);

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
    let friction = block_friction(chunk_store, player.position);
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

    player.velocity.y -= effective_gravity(player);
    player.velocity.y *= f64::from(VERTICAL_DRAG);

    let h_friction = if on_ground_at_start {
        friction * HORIZONTAL_DRAG
    } else {
        HORIZONTAL_DRAG
    };
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
        player.attribute_value("minecraft:generic.water_movement_efficiency", 0.0) as f32;
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
        player.position += *player.velocity;
        player.on_ground = false;
        player.horizontal_collision = false;
        return;
    }

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
    delta = back_off_from_edge(
        chunk_store,
        &aabb,
        delta,
        input.performing_action(input::Action::Sneak),
        player.on_ground,
        player.flying,
    );
    let step_height =
        player.attribute_value("minecraft:generic.step_height", f64::from(STEP_HEIGHT));
    let (resolved, on_ground) = super::collision::resolve_collision_for_player(
        chunk_store,
        aabb,
        delta.into(),
        step_height,
        player.on_ground,
        entity_aabbs,
        border_bounds,
        Some((
            input.performing_action(input::Action::Sneak),
            has_leather_boots(player),
            player.fall_distance,
        )),
    );

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
        let landed_on_slime = delta.y < 0.0
            && !input.performing_action(input::Action::Sneak)
            && crate::world::block::block_id(chunk_store.get_block_state(
                player.position.x.floor() as i32,
                (player.bounding_box().min.y - f64::from(0.2_f32)).floor() as i32,
                player.position.z.floor() as i32,
            )) == "slime_block";
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

    if (horizontal_collision || input.performing_action(input::Action::Jump))
        && intersects_block_id(chunk_store, &aabb, "powder_snow")
        && has_leather_boots(player)
    {
        // LivingEntity.handleRelativeFrictionAndCalculateMovement lets a
        // powder-snow walker climb with the same 0.2 upward impulse.
        player.velocity.y = 0.2;
    }

    if player.sprinting
        && horizontal_collision
        && forward > 0.0
        && !is_minor_horizontal_collision(forward, strafe, sin_y_rot, cos_y_rot, resolved)
    {
        player.sprinting = false;
    }
}

fn honey_slide_movement(delta_y: f64) -> (f64, f64) {
    let old_y = delta_y / f64::from(VERTICAL_DRAG) + 0.08;
    let horizontal_scale = if old_y < -0.13 { -0.05 / old_y } else { 1.0 };
    (horizontal_scale, (-0.05 - 0.08) * f64::from(VERTICAL_DRAG))
}

fn update_fall_distance(fall_distance: &mut f32, resolved_y: f64, on_ground: bool, in_water: bool) {
    if on_ground || in_water {
        *fall_distance = 0.0;
    } else if resolved_y < 0.0 {
        *fall_distance -= resolved_y as f32;
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

fn has_leather_boots(player: &LocalPlayer) -> bool {
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
    let can_sprint = forward > 0.0
        && player.food > SPRINT_HUNGER_THRESHOLD
        && !player.crouching
        && !slow_due_to_using_item;

    if input.performing_action(input::Action::Sprint) && can_sprint {
        player.sprinting = true;
    }

    if !player.was_forward_pressed && forward_pressed && can_sprint {
        if player.sprint_toggle_timer > 0 {
            player.sprinting = true;
        }
        player.sprint_toggle_timer = DEFAULT_SPRINT_WINDOW;
    }

    if player.sprinting
        && (forward <= 0.0 || player.food <= SPRINT_HUNGER_THRESHOLD || slow_due_to_using_item)
    {
        player.sprinting = false;
    }
}

// Forces the crouch pose under ceilings too low to stand in; riding and
// sleeping aren't simulated.
fn update_crouch_state(player: &mut LocalPlayer, input: &InputState, chunk_store: &ChunkStore) {
    player.crouching = player.game_mode != 3
        && !player.flying
        && !player.swimming
        && can_fit_with_height(chunk_store, player.position.into(), CROUCH_HEIGHT)
        && (input.performing_action(input::Action::Sneak)
            || !can_fit_with_height(chunk_store, player.position.into(), STANDING_HEIGHT));
}

fn can_fit_with_height(chunk_store: &ChunkStore, pos: DVec3, height: f64) -> bool {
    no_collision(
        chunk_store,
        &Aabb::from_center(pos, PLAYER_HALF_WIDTH, height / 2.0).deflate(1.0e-7),
    )
}

// While holding shift on the ground, clamp the horizontal move so the player
// can't fall further than the step height.
fn back_off_from_edge(
    chunk_store: &ChunkStore,
    bb: &Aabb,
    delta: DVec3,
    shift_down: bool,
    on_ground: bool,
    flying: bool,
) -> DVec3 {
    if !shift_down || flying || delta.y > 0.0 {
        return delta;
    }
    // TODO: fall distance - falling less than the step height still counts
    // as above ground
    let above_ground =
        on_ground || !can_fall_at_least(chunk_store, bb, 0.0, 0.0, f64::from(STEP_HEIGHT));
    if !above_ground {
        return delta;
    }

    let mut dx = delta.x;
    let mut dz = delta.z;
    let step_x = dx.signum() * 0.05;
    let step_z = dz.signum() * 0.05;

    while dx != 0.0 && can_fall_at_least(chunk_store, bb, dx, 0.0, f64::from(STEP_HEIGHT)) {
        if dx.abs() <= 0.05 {
            dx = 0.0;
            break;
        }
        dx -= step_x;
    }
    while dz != 0.0 && can_fall_at_least(chunk_store, bb, 0.0, dz, f64::from(STEP_HEIGHT)) {
        if dz.abs() <= 0.05 {
            dz = 0.0;
            break;
        }
        dz -= step_z;
    }
    while dx != 0.0
        && dz != 0.0
        && can_fall_at_least(chunk_store, bb, dx, dz, f64::from(STEP_HEIGHT))
    {
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
) -> bool {
    no_collision(
        chunk_store,
        &Aabb::new(
            dvec3(
                bb.min.x + 1.0e-7 + dx,
                bb.min.y - min_height - 1.0e-7,
                bb.min.z + 1.0e-7 + dz,
            ),
            dvec3(bb.max.x - 1.0e-7 + dx, bb.min.y, bb.max.z - 1.0e-7 + dz),
        ),
    )
}

/// Vanilla's `Block.getFriction`; floor fallback mirrors
/// `Entity.getOnPos(0.500001F)`.
// ponytail: collision resolver has no mainSupportingBlockPos; use the official
// floor fallback.
fn block_friction(chunks: &ChunkStore, position: crate::entity::components::Position) -> f32 {
    let id = crate::world::block::block_id(chunks.get_block_state(
        position.x.floor() as i32,
        (position.y - f64::from(0.500_001_f32)).floor() as i32,
        position.z.floor() as i32,
    ));
    friction_for_block_id(id)
}

fn friction_for_block_id(id: &str) -> f32 {
    match id {
        "honey_block" | "soul_sand" => 0.4,
        "ice" | "packed_ice" | "frosted_ice" => 0.98,
        "blue_ice" => 0.989,
        "slime_block" => 0.8,
        _ => BLOCK_FRICTION,
    }
}

fn movement_speed(player: &LocalPlayer) -> f32 {
    let mut speed =
        player.attribute_value("minecraft:generic.movement_speed", MOVEMENT_SPEED_ATTRIBUTE);
    if player.sprinting {
        speed *= 1.0 + SPRINT_SPEED_MODIFIER;
    }
    speed as f32
}

fn effective_gravity(player: &LocalPlayer) -> f64 {
    let gravity = player.attribute_value("minecraft:generic.gravity", GRAVITY);
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

fn movement_input(input: &InputState, crouching: bool, use_speed_multiplier: f32) -> (f32, f32) {
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
        strafe *= SNEAKING_SPEED;
        forward *= SNEAKING_SPEED;
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
    use crate::player::{CROUCH_EYE_HEIGHT, STANDING_EYE_HEIGHT};

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
        player.sprinting = true;
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
        let mut player = LocalPlayer::new();
        player.set_attribute_value("minecraft:generic.movement_speed", 0.2);
        player.set_attribute_value("minecraft:generic.gravity", 0.04);
        player.set_attribute_value("minecraft:generic.jump_strength", 0.6);
        assert_eq!(movement_speed(&player), 0.2);
        player.sprinting = true;
        assert_eq!(movement_speed(&player), 0.26);
        assert_eq!(effective_gravity(&player), 0.04);
        let (sin, cos) = vanilla_yaw_sin_cos(0.0);
        jump_from_ground(&mut player, sin, cos);
        assert_eq!(player.velocity.y, 0.6);
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
            ("honey_block", 0.4),
            ("soul_sand", 0.4),
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
    fn dead_player_keeps_zero_input_air_travel() {
        crate::world::block::init("26.2");
        let mut player = LocalPlayer::new();
        player.position = dvec3(0.0, 80.0, 0.0).into();
        player.velocity = crate::entity::components::Velocity::new(0.25, 0.0, -0.1);
        player.sprinting = true;
        player.crouching = true;
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
