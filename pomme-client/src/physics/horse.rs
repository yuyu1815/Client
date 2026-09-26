use glam::{DVec3, dvec3};

use super::aabb::Aabb;
use super::collision::resolve_collision_with_context;
use super::movement::block_friction;
use crate::entity::LivingEntity;
use crate::entity::components::LookDirection;
use crate::world::block::{self, FluidKind};
use crate::world::chunk::ChunkStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorseTickResult {
    Applied,
    /// No state, including the pending jump, was changed. Fluid travel is not
    /// implemented.
    UnsupportedFluid,
}

pub(crate) fn horse_aabb(horse: &LivingEntity) -> Aabb {
    Aabb::from_center(
        horse.position.into(),
        f64::from(1.396_484_4_f32) * 0.5,
        f64::from(1.6_f32) * 0.5,
    )
}

/// One locally authoritative adult-horse land/air tick. `input` is the rider's
/// (forward, strafe) from movement_input(input, false, 1.0), NOT raw key axes.
/// Callers resolve authority and collect entity/border collisions before
/// borrowing the horse mutably. This does not send packets or integrate the
/// rider as a player.
pub fn tick_controlled_horse_land(
    horse: &mut LivingEntity,
    chunks: &ChunkStore,
    rider_look: LookDirection,
    input: (f32, f32),
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
) -> HorseTickResult {
    let bb = horse_aabb(horse);
    let fluid_bb = bb.deflate(0.001);
    for y in fluid_bb.min.y.floor() as i32..fluid_bb.max.y.ceil() as i32 {
        for z in fluid_bb.min.z.floor() as i32..fluid_bb.max.z.ceil() as i32 {
            for x in fluid_bb.min.x.floor() as i32..fluid_bb.max.x.ceil() as i32 {
                let fluid = block::fluid(chunks.get_block_state(x, y, z));
                if matches!(fluid.kind, FluidKind::Water | FluidKind::Lava) {
                    let above = block::fluid(chunks.get_block_state(x, y + 1, z));
                    let height = if above.kind == fluid.kind {
                        1.0
                    } else {
                        f64::from(fluid.height())
                    };
                    if f64::from(y) + height >= fluid_bb.min.y {
                        return HorseTickResult::UnsupportedFluid;
                    }
                }
            }
        }
    }

    horse.look_dir = LookDirection::new(rider_look.y_rot_deg(), rider_look.x_rot_deg() * 0.5);
    horse.head_y_rot_deg = rider_look.y_rot_deg();
    horse.body_y_rot_deg = rider_look.y_rot_deg();
    let grounded = horse.on_ground;
    let friction = if grounded {
        block_friction(chunks, horse.position)
    } else {
        1.0
    };
    let speed = horse
        .attributes
        .get("minecraft:movement_speed")
        .copied()
        .unwrap_or(0.225);
    let step = horse
        .attributes
        .get("minecraft:step_height")
        .copied()
        .unwrap_or(1.0);
    // LivingEntity.aiStep zeros each horse axis independently, before acceleration.
    for axis in 0..3 {
        if horse.velocity[axis].abs() < 0.003 {
            horse.velocity[axis] = 0.0;
        }
    }
    if grounded {
        let scale = horse.horse_jump_pending_scale;
        if scale > 0.0 {
            let strength = horse
                .attributes
                .get("minecraft:jump_strength")
                .copied()
                .unwrap_or(0.7);
            let jump_factor = [
                horse.position.y,
                horse.position.y - f64::from(0.500_001_f32),
            ]
            .iter()
            .any(|y| {
                block::block_id(chunks.get_block_state(
                    horse.position.x.floor() as i32,
                    y.floor() as i32,
                    horse.position.z.floor() as i32,
                )) == "honey_block"
            });
            let boost = horse
                .effects
                .values()
                .find(|effect| {
                    crate::mob_effect::info(effect.effect_id)
                        .is_some_and(|info| info.name == "jump_boost")
                })
                .map_or(0.0, |effect| {
                    f64::from(0.1_f32 * (f32::from(effect.amplifier) + 1.0))
                });
            horse.velocity = riding_jump_impulse(
                horse.velocity,
                horse.look_dir.y_rot_deg(),
                strength,
                if jump_factor { 0.5 } else { 1.0 },
                boost,
                f64::from(scale),
                input.0,
                true,
            );
        }
        horse.horse_jump_pending_scale = 0.0;
    }
    horse.velocity += ridden_acceleration(
        horse.look_dir.y_rot_deg(),
        input.0,
        input.1,
        speed,
        grounded,
        f64::from(friction),
    );
    let delta = horse.velocity;
    let (resolved, on_ground) = resolve_collision_with_context(
        chunks,
        bb,
        delta.into(),
        f64::from((step as f32).max(1.0)),
        grounded,
        entity_aabbs,
        border_bounds,
    );
    horse.position += resolved;
    horse.on_ground = on_ground;
    if (delta.x - resolved.x).abs() >= f64::from(1.0e-5_f32) {
        horse.velocity.x = 0.0;
    }
    if (delta.z - resolved.z).abs() >= f64::from(1.0e-5_f32) {
        horse.velocity.z = 0.0;
    }
    if delta.y != resolved.y {
        horse.velocity.y = 0.0;
    }
    // Horse/LivingEntity constants, never the rider's gravity, fly speed or sprint
    // state.
    horse.velocity.y = (horse.velocity.y - 0.08) * f64::from(0.98_f32);
    let drag = f64::from(friction * 0.91_f32);
    horse.velocity.x *= drag;
    horse.velocity.z *= drag;
    HorseTickResult::Applied
}

/// Horse `LivingEntity.travelRidden` input acceleration for the land/air path.
/// Water and lava travel are deliberately outside this phase.
pub fn ridden_acceleration(
    yaw_degrees: f32,
    forward: f32,
    strafe: f32,
    movement_speed: f64,
    on_ground: bool,
    block_friction: f64,
) -> DVec3 {
    let mut forward = forward;
    let strafe = strafe * 0.5;
    if forward <= 0.0 {
        forward *= 0.25;
    }
    let friction = block_friction as f32;
    let speed = if on_ground {
        movement_speed as f32
            * if f64::from(friction) <= 0.6 {
                1.0
            } else {
                0.216_000_02_f32 / (friction * friction * friction)
            }
    } else {
        movement_speed as f32 * 0.1_f32
    };
    let (mut x, mut z) = (f64::from(strafe), f64::from(forward));
    let length_squared = x * x + z * z;
    if length_squared < 1.0e-7 {
        return DVec3::ZERO;
    }
    if length_squared > 1.0 {
        let length = length_squared.sqrt();
        x /= length;
        z /= length;
    }
    x *= f64::from(speed);
    z *= f64::from(speed);
    let (sin, cos) = horse_yaw_sin_cos(yaw_degrees);
    dvec3(
        x * f64::from(cos) - z * f64::from(sin),
        0.0,
        z * f64::from(cos) + x * f64::from(sin),
    )
}

// 26.2 Mth takes a double table index; azalea's sin/cos still multiply in
// float.
fn horse_yaw_sin_cos(yaw_degrees: f32) -> (f32, f32) {
    let angle = f64::from(yaw_degrees * (std::f32::consts::PI / 180.0));
    let index = angle * 10_430.378_350_470_453;
    (
        azalea_core::math::SIN[(index as i64 & 0xFFFF) as usize],
        azalea_core::math::SIN[((index + 16_384.0) as i64 & 0xFFFF) as usize],
    )
}

/// Apply the vanilla horse's pending riding-jump impulse to its current
/// velocity.
pub fn riding_jump_impulse(
    velocity: DVec3,
    yaw_degrees: f32,
    jump_strength: f64,
    block_jump_factor: f64,
    jump_boost: f64,
    scale: f64,
    forward: f32,
    on_ground: bool,
) -> DVec3 {
    if !on_ground {
        return velocity;
    }
    let mut velocity = velocity;
    // AbstractHorse.executeRidersJump replaces vy (unlike Player.jumpFromGround).
    velocity.y = f64::from(
        jump_strength as f32 * scale as f32 * block_jump_factor as f32 + jump_boost as f32,
    );
    if forward > 0.0 {
        let (sin, cos) = horse_yaw_sin_cos(yaw_degrees);
        velocity.x += f64::from(-0.4_f32 * sin * scale as f32);
        velocity.z += f64::from(0.4_f32 * cos * scale as f32);
    }
    velocity
}

#[cfg(test)]
mod tests {
    use azalea_registry::builtin::EntityKind;

    use super::*;

    fn in_world(test: impl FnOnce(&ChunkStore)) {
        block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        let _chunk = chunks.chunk_storage.upsert(
            azalea_core::position::ChunkPos::new(0, 0),
            azalea_world::chunk::Chunk::default(),
        );
        let stone = block::first_state_of("stone").unwrap();
        for x in 0..16 {
            for z in 0..16 {
                chunks.set_block_state(x, 60, z, stone);
            }
        }
        test(&chunks);
    }

    fn horse() -> LivingEntity {
        let mut horse = LivingEntity::new(
            EntityKind::Horse,
            dvec3(14.0, 61.0, 2.0).into(),
            LookDirection::new(0.0, 0.0),
            0.0,
            0.0,
            None,
        );
        horse.on_ground = true;
        horse.velocity.y = -0.08 * f64::from(0.98_f32);
        horse
    }

    fn tick(horse: &mut LivingEntity, chunks: &ChunkStore, yaw: f32, forward: f32) {
        assert_eq!(
            tick_controlled_horse_land(
                horse,
                chunks,
                LookDirection::new(yaw, 40.0),
                (forward, 0.0),
                &[],
                None
            ),
            HorseTickResult::Applied
        );
    }

    #[test]
    fn twenty_ticks_integrate_displacement_yaw_and_speed_attributes() {
        in_world(|chunks| {
            for (speed, expected_distance) in [
                (0.1, 4.057_581_306),
                (0.225, 9.129_557_561),
                (0.3, 12.172_744_221),
            ] {
                for yaw in [0.0, 90.0] {
                    let mut horse = horse();
                    horse
                        .attributes
                        .insert("minecraft:movement_speed".into(), speed);
                    let origin = horse.position;
                    let friction = 0.6_f32;
                    let accel = f64::from(speed as f32 * (0.216_000_02_f32 / friction.powi(3)))
                        * f64::from(0.98_f32);
                    let drag = f64::from(friction * 0.91_f32);
                    let (mut velocity, mut distance) = (0.0, 0.0);
                    for _ in 0..20 {
                        velocity += accel;
                        distance += velocity;
                        velocity *= drag;
                        tick(&mut horse, chunks, yaw, 0.98);
                    }
                    let displacement = horse.position - origin;
                    assert!((distance - expected_distance).abs() < 1e-5);
                    if yaw == 0.0 {
                        assert!(displacement.x.abs() < 1e-7);
                        assert!(
                            (displacement.z - distance).abs() < 1e-5,
                            "{displacement:?} {distance}"
                        );
                    } else {
                        assert!((displacement.x + distance).abs() < 1e-5);
                        assert!(displacement.z.abs() < 1e-7);
                    }
                    assert_eq!(horse.position.y, 61.0);
                    assert!(horse.on_ground);
                    assert_eq!(horse.look_dir.y_rot_deg(), yaw);
                    assert_eq!(horse.look_dir.x_rot_deg(), 20.0);
                    assert_eq!(horse.head_y_rot_deg, yaw);
                    assert_eq!(horse.body_y_rot_deg, yaw);
                }
            }
        });
    }

    #[test]
    fn wall_clips_displacement_and_resets_velocity() {
        in_world(|chunks| {
            let stone = block::first_state_of("stone").unwrap();
            for x in 0..16 {
                for y in 61..65 {
                    chunks.set_block_state(x, y, 5, stone);
                }
            }
            let mut horse = horse();
            for _ in 0..20 {
                tick(&mut horse, chunks, 0.0, 0.98);
            }
            assert!((horse_aabb(&horse).max.z - 5.0).abs() < 1e-7);
            assert_eq!(horse.velocity.z, 0.0);
            assert_eq!(horse.position.y, 61.0);
            assert!(horse.on_ground);
            assert_eq!(horse.velocity.y, -0.08 * f64::from(0.98_f32));
        });
    }

    #[test]
    fn ridden_step_height_has_one_block_minimum_and_uses_attribute() {
        for (height, attribute, climbs) in [(1, 0.0, true), (2, 1.0, false), (2, 2.0, true)] {
            in_world(|chunks| {
                let stone = block::first_state_of("stone").unwrap();
                for x in 0..16 {
                    for z in 5..16 {
                        for y in 61..61 + height {
                            chunks.set_block_state(x, y, z, stone);
                        }
                    }
                }
                let mut horse = horse();
                horse
                    .attributes
                    .insert("minecraft:step_height".into(), attribute);
                for _ in 0..20 {
                    tick(&mut horse, chunks, 0.0, 0.98);
                }
                assert_eq!(
                    horse.position.y,
                    if climbs {
                        61.0 + f64::from(height)
                    } else {
                        61.0
                    }
                );
                assert_eq!(horse.position.z > 5.0, climbs);
                assert!(horse.on_ground);
            });
        }
    }

    #[test]
    fn ledge_enters_air_and_uses_horse_not_player_acceleration_or_gravity() {
        in_world(|chunks| {
            for x in 0..16 {
                for z in 5..16 {
                    chunks.set_block_state(x, 60, z, azalea_block::BlockState::AIR);
                }
            }
            let mut horse = horse();
            for _ in 0..20 {
                tick(&mut horse, chunks, 0.0, 0.98);
            }
            assert!(!horse.on_ground);
            assert!(horse.position.z > 5.7 && horse.position.y < 60.0);
            let old_y = horse.position.y;
            let old_velocity = horse.velocity;
            tick(&mut horse, chunks, 0.0, 0.98);
            assert!((horse.position.y - old_y - old_velocity.y).abs() < 1e-10);
            assert_eq!(
                horse.velocity.y,
                (old_velocity.y - 0.08) * f64::from(0.98_f32)
            );
            let acceleration = f64::from(0.225_f32 * 0.1_f32) * f64::from(0.98_f32);
            assert!(
                (horse.velocity.z - (old_velocity.z + acceleration) * f64::from(0.91_f32)).abs()
                    < 1e-10
            );
        });
    }

    #[test]
    fn neutral_air_tick_zeros_axes_individually_and_does_not_tick_player() {
        in_world(|chunks| {
            let mut rider = crate::player::LocalPlayer::new();
            rider.set_attribute_value("minecraft:generic.gravity", 2.0);
            let rider_position = rider.position;
            let rider_velocity = rider.velocity;
            let mut horse = horse();
            horse.position.y = 70.0;
            horse.on_ground = false;
            horse.velocity = dvec3(0.0029, 0.0, 0.0029);
            let input = crate::app::input::InputState::released();
            assert_eq!(
                tick_controlled_horse_land(
                    &mut horse,
                    chunks,
                    rider.look_dir,
                    super::super::movement::movement_input(&input, false, 1.0),
                    &[],
                    None
                ),
                HorseTickResult::Applied
            );
            assert_eq!(horse.position.y, 70.0);
            assert_eq!(horse.velocity, dvec3(0.0, -0.08 * f64::from(0.98_f32), 0.0));
            assert_eq!(rider.position, rider_position);
            assert_eq!(rider.velocity, rider_velocity);
            tick(&mut horse, chunks, 0.0, 0.0);
            assert!((horse.position.y - (70.0 - 0.08 * f64::from(0.98_f32))).abs() < 1e-12);
        });
    }

    #[test]
    fn pending_jump_uses_attribute_and_is_consumed_only_on_ground() {
        in_world(|chunks| {
            let mut horse = horse();
            horse
                .attributes
                .insert("minecraft:jump_strength".into(), 0.9);
            horse.horse_jump_pending_scale = 0.5;
            tick(&mut horse, chunks, 0.0, 0.98);
            assert!((horse.position.y - 61.45).abs() < 1e-6);
            assert!(!horse.on_ground);
            assert_eq!(horse.horse_jump_pending_scale, 0.0);
            horse.horse_jump_pending_scale = 1.0;
            tick(&mut horse, chunks, 0.0, 0.98);
            assert_eq!(horse.horse_jump_pending_scale, 1.0);
        });
    }

    #[test]
    fn water_and_lava_return_explicitly_without_mutating_horse() {
        for fluid in ["water", "lava"] {
            in_world(|chunks| {
                // Touch the horse's flank, not just its feet/center cell.
                chunks.set_block_state(13, 61, 2, block::first_state_of(fluid).unwrap());
                let mut horse = horse();
                horse.horse_jump_pending_scale = 0.8;
                let (position, velocity, look) = (horse.position, horse.velocity, horse.look_dir);
                assert_eq!(
                    tick_controlled_horse_land(
                        &mut horse,
                        chunks,
                        LookDirection::new(90.0, 40.0),
                        (0.98, 0.0),
                        &[],
                        None
                    ),
                    HorseTickResult::UnsupportedFluid
                );
                assert_eq!(horse.position, position);
                assert_eq!(horse.velocity, velocity);
                assert_eq!(horse.look_dir.y_rot_deg(), look.y_rot_deg());
                assert_eq!(horse.horse_jump_pending_scale, 0.8);
                assert!(horse.on_ground);
            });
        }
    }

    #[test]
    fn entity_and_border_context_clip_the_horse() {
        in_world(|chunks| {
            for border in [false, true] {
                let mut horse = horse();
                let wall = Aabb::new(dvec3(0.0, 60.0, 5.0), dvec3(16.0, 66.0, 6.0));
                for _ in 0..20 {
                    assert_eq!(
                        tick_controlled_horse_land(
                            &mut horse,
                            chunks,
                            LookDirection::new(0.0, 0.0),
                            (0.98, 0.0),
                            if border {
                                &[]
                            } else {
                                std::slice::from_ref(&wall)
                            },
                            border.then_some([0.0, 16.0, 0.0, 5.0])
                        ),
                        HorseTickResult::Applied
                    );
                }
                assert!((horse_aabb(&horse).max.z - 5.0).abs() < 1e-7);
                assert_eq!(horse.velocity.z, 0.0);
            }
        });
    }

    #[test]
    fn riding_jump_impulse_is_grounded_and_forward_only() {
        let v = riding_jump_impulse(DVec3::ZERO, 0.0, 0.8, 1.0, 0.0, 1.0, 1.0, true);
        assert!((v - DVec3::new(0.0, 0.8, 0.4)).length() < 1e-7);
        assert!(
            (riding_jump_impulse(DVec3::ZERO, 0.0, 0.8, 1.0, 0.0, 1.0, 0.0, true)
                - DVec3::new(0.0, 0.8, 0.0))
            .length()
                < 1e-7
        );
        assert_eq!(
            riding_jump_impulse(DVec3::ZERO, 0.0, 0.8, 1.0, 0.0, 1.0, 1.0, false),
            DVec3::ZERO
        );
    }

    #[test]
    fn ridden_acceleration_obeys_yaw_and_direction_scaling() {
        let forward = ridden_acceleration(0.0, 1.0, 0.0, 0.2, true, 0.6);
        assert!(forward.x.abs() < 1e-7 && (forward.z - 0.2).abs() < 1e-7);
        let left = ridden_acceleration(0.0, 0.0, 1.0, 0.2, true, 0.6);
        assert!((left.x - 0.1).abs() < 1e-7);
        let west = ridden_acceleration(90.0, 1.0, 0.0, 0.2, true, 0.6);
        assert!((west.x + 0.2).abs() < 1e-7);
        // Backward input is quarter speed, not the inverse of forward input.
        assert!((ridden_acceleration(0.0, -1.0, 0.0, 0.2, true, 0.6).z + 0.05).abs() < 1e-7);
        let diagonal = ridden_acceleration(0.0, 1.0, 1.0, 0.2, true, 0.6);
        assert!((diagonal.length() - forward.length()).abs() < 1e-7);
    }
}
