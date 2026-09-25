use glam::{DVec3, dvec3};

use super::aabb::Aabb;
use super::block_shape;
use crate::entity::components::Velocity;
use crate::world::block::has_collision;
use crate::world::chunk::ChunkStore;

const COLLISION_EPSILON: f64 = 1.0e-7;

pub fn collect_block_aabbs(chunk_store: &ChunkStore, region: &Aabb) -> Vec<Aabb> {
    let mut aabbs = Vec::new();

    let min_x = region.min.x.floor() as i32;
    let min_y = region.min.y.floor() as i32;
    let min_z = region.min.z.floor() as i32;
    let max_x = region.max.x.ceil() as i32;
    let max_y = region.max.y.ceil() as i32;
    let max_z = region.max.z.ceil() as i32;

    for by in min_y..max_y {
        for bz in min_z..max_z {
            for bx in min_x..max_x {
                let state = chunk_store.get_block_state(bx, by, bz);
                if crate::world::block::block_id(state) == "moving_piston" {
                    let pos = azalea_core::position::BlockPos::new(bx, by, bz);
                    if let Some((moved, progress_offset)) =
                        chunk_store.block_entities.get(&pos).and_then(|entity| {
                            crate::world::block_entity::moving_block_collision(&entity.nbt)
                        })
                        && has_collision(moved)
                    {
                        let origin = dvec3(bx as f64, by as f64, bz as f64) + progress_offset;
                        match block_shape::partial_shape(moved) {
                            Some(boxes) => {
                                aabbs.extend(boxes.iter().map(|b| Aabb::from_local(*b, origin)))
                            }
                            None => aabbs.push(Aabb::block(bx, by, bz).offset(progress_offset)),
                        }
                    }
                    continue;
                }
                if !has_collision(state) {
                    continue;
                }
                match block_shape::partial_shape(state) {
                    Some(boxes) => {
                        let offset = dvec3(bx as f64, by as f64, bz as f64);
                        aabbs.extend(boxes.iter().map(|&b| Aabb::from_local(b, offset)));
                    }
                    None => aabbs.push(Aabb::block(bx, by, bz)),
                }
            }
        }
    }

    aabbs
}

pub fn no_collision(chunk_store: &ChunkStore, aabb: &Aabb) -> bool {
    collect_block_aabbs(chunk_store, aabb)
        .iter()
        .all(|block| !block.intersects(aabb))
}

fn collide_along_axes(
    block_aabbs: &[Aabb],
    player_aabb: Aabb,
    mut velocity: Velocity,
) -> (DVec3, bool) {
    let original_y = velocity.y;

    for block in block_aabbs {
        if velocity.y.abs() < COLLISION_EPSILON {
            velocity.y = 0.0;
            break;
        }
        velocity.y = block.clip_y_collide(&player_aabb, velocity.y);
    }
    let mut resolved = player_aabb.offset(dvec3(0.0, velocity.y, 0.0));

    let x_first = velocity.x.abs() >= velocity.z.abs();

    if x_first {
        for block in block_aabbs {
            if velocity.x.abs() < COLLISION_EPSILON {
                velocity.x = 0.0;
                break;
            }
            velocity.x = block.clip_x_collide(&resolved, velocity.x);
        }
        resolved = resolved.offset(dvec3(velocity.x, 0.0, 0.0));

        for block in block_aabbs {
            if velocity.z.abs() < COLLISION_EPSILON {
                velocity.z = 0.0;
                break;
            }
            velocity.z = block.clip_z_collide(&resolved, velocity.z);
        }
    } else {
        for block in block_aabbs {
            if velocity.z.abs() < COLLISION_EPSILON {
                velocity.z = 0.0;
                break;
            }
            velocity.z = block.clip_z_collide(&resolved, velocity.z);
        }
        resolved = resolved.offset(dvec3(0.0, 0.0, velocity.z));

        for block in block_aabbs {
            if velocity.x.abs() < COLLISION_EPSILON {
                velocity.x = 0.0;
                break;
            }
            velocity.x = block.clip_x_collide(&resolved, velocity.x);
        }
    }

    let on_ground = original_y < 0.0 && velocity.y != original_y;

    (*velocity, on_ground)
}

pub fn resolve_collision(
    chunk_store: &ChunkStore,
    player_aabb: Aabb,
    velocity: Velocity,
    step_height: f64,
) -> (DVec3, bool) {
    resolve_collision_with_context(
        chunk_store,
        player_aabb,
        velocity,
        step_height,
        false,
        &[],
        None,
    )
}

pub fn resolve_collision_with_grounded(
    chunk_store: &ChunkStore,
    player_aabb: Aabb,
    velocity: Velocity,
    step_height: f64,
    was_grounded: bool,
) -> (DVec3, bool) {
    resolve_collision_with_context(
        chunk_store,
        player_aabb,
        velocity,
        step_height,
        was_grounded,
        &[],
        None,
    )
}

fn append_context_aabbs(
    aabbs: &mut Vec<Aabb>,
    region: &Aabb,
    entities: &[Aabb],
    border: Option<[f64; 4]>,
) {
    aabbs.extend(
        entities
            .iter()
            .copied()
            .filter(|aabb| aabb.intersects(region)),
    );
    if let Some([min_x, max_x, min_z, max_z]) = border {
        let y = 30_000_000.0;
        for wall in [
            Aabb::new(
                dvec3(min_x - 0.5, -y, min_z - y),
                dvec3(min_x, y, max_z + y),
            ),
            Aabb::new(
                dvec3(max_x, -y, min_z - y),
                dvec3(max_x + 0.5, y, max_z + y),
            ),
            Aabb::new(
                dvec3(min_x - y, -y, min_z - 0.5),
                dvec3(max_x + y, y, min_z),
            ),
            Aabb::new(
                dvec3(min_x - y, -y, max_z),
                dvec3(max_x + y, y, max_z + 0.5),
            ),
        ] {
            if wall.intersects(region) {
                aabbs.push(wall);
            }
        }
    }
}

pub fn resolve_collision_with_context(
    chunk_store: &ChunkStore,
    player_aabb: Aabb,
    velocity: Velocity,
    step_height: f64,
    was_grounded: bool,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
) -> (DVec3, bool) {
    let expanded = player_aabb.expand(*velocity);
    let mut block_aabbs = collect_block_aabbs(chunk_store, &expanded);
    append_context_aabbs(&mut block_aabbs, &expanded, entity_aabbs, border_bounds);

    let (resolved, on_ground) = collide_along_axes(&block_aabbs, player_aabb, velocity);

    let horizontal_blocked = resolved.x != velocity.x || resolved.z != velocity.z;
    if step_height > 0.0 && (on_ground || was_grounded) && horizontal_blocked {
        let grounded_aabb = if on_ground {
            player_aabb.offset(dvec3(0.0, resolved.y, 0.0))
        } else {
            player_aabb
        };
        let step_region = grounded_aabb
            .expand(dvec3(velocity.x, step_height, velocity.z))
            .expand(dvec3(
                0.0,
                if on_ground { 0.0 } else { -COLLISION_EPSILON },
                0.0,
            ));
        let mut step_aabbs = collect_block_aabbs(chunk_store, &step_region);
        append_context_aabbs(&mut step_aabbs, &step_region, entity_aabbs, border_bounds);
        let skip_height = if on_ground { resolved.y as f32 } else { 0.0 };
        let mut candidates = Vec::new();
        for block in &step_aabbs {
            for y in [block.min.y, block.max.y] {
                let height = (y - grounded_aabb.min.y) as f32;
                if height >= 0.0 && height <= step_height as f32 && height != skip_height {
                    candidates.push(height);
                }
            }
        }
        candidates.sort_by(f32::total_cmp);
        candidates.dedup();
        for height in candidates {
            let step = dvec3(velocity.x, f64::from(height), velocity.z);
            let (candidate, _) = collide_along_axes(&step_aabbs, grounded_aabb, step.into());
            if candidate.x * candidate.x + candidate.z * candidate.z
                > resolved.x * resolved.x + resolved.z * resolved.z
            {
                let result = candidate + dvec3(0.0, grounded_aabb.min.y - player_aabb.min.y, 0.0);
                return (result, on_ground || was_grounded);
            }
        }
    }

    (resolved, on_ground)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_distance_below_vanilla_epsilon_is_zeroed() {
        let player = Aabb::from_center(dvec3(0.5, 0.0, 0.5), 0.3, 0.9);
        let block = Aabb::block(1, 0, 0);
        let (resolved, _) = collide_along_axes(&[block], player, Velocity::new(5.0e-8, 0.0, 0.0));
        assert_eq!(resolved.x, 0.0);
    }

    #[test]
    fn entity_aabb_stops_player_motion() {
        let chunks = ChunkStore::new(1);
        let player = Aabb::from_center(dvec3(0.5, 0.0, 0.5), 0.3, 0.9);
        let entity = Aabb::new(dvec3(1.0, 0.0, 0.0), dvec3(1.6, 1.8, 1.0));
        let (resolved, _) = resolve_collision_with_context(
            &chunks,
            player,
            Velocity::new(1.0, 0.0, 0.0),
            0.0,
            false,
            &[entity],
            None,
        );
        assert!((resolved.x - 0.2).abs() < 1.0e-9);
    }

    #[test]
    fn world_border_stops_player_at_boundary() {
        let chunks = ChunkStore::new(1);
        let player = Aabb::from_center(dvec3(4.5, 0.0, 0.0), 0.3, 0.9);
        let (resolved, _) = resolve_collision_with_context(
            &chunks,
            player,
            Velocity::new(1.0, 0.0, 0.0),
            0.0,
            false,
            &[],
            Some([-5.0, 5.0, -5.0, 5.0]),
        );
        assert!((resolved.x - 0.2).abs() < 1.0e-9);
    }
}
