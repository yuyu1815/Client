use azalea_block::BlockState;
use azalea_core::position::BlockPos;
use glam::{DVec3, dvec3};

use super::aabb::Aabb;
use super::block_shape;
use crate::entity::components::Velocity;
use crate::world::chunk::ChunkStore;

const COLLISION_EPSILON: f64 = 1.0e-7;
const MAX_BLOCK_SHAPE_OFFSET: f64 = 0.25;

pub fn collect_block_aabbs(chunk_store: &ChunkStore, region: &Aabb) -> Vec<Aabb> {
    collect_block_aabbs_for_player(chunk_store, region, None)
}

// EntityCollisionContext: feet height, shift-descending, equipment and fall
// distance are needed for the two blocks whose shapes depend on the mover.
fn collect_block_aabbs_for_player(
    chunk_store: &ChunkStore,
    region: &Aabb,
    player: Option<(f64, bool, bool, f64)>,
) -> Vec<Aabb> {
    collect_block_aabbs_with(region, player, |x, y, z| {
        collision_cell(chunk_store, x, y, z)
    })
}

fn collision_cell(
    chunks: &ChunkStore,
    x: i32,
    y: i32,
    z: i32,
) -> (BlockState, Option<(BlockState, DVec3)>) {
    let state = chunks.get_block_state(x, y, z);
    let piston = (crate::world::block::block_id(state) == "moving_piston")
        .then(|| chunks.block_entities.get(&BlockPos::new(x, y, z)))
        .flatten()
        .and_then(|entity| crate::world::block_entity::moving_block_collision(&entity.nbt));
    (state, piston)
}

fn collect_block_aabbs_with(
    region: &Aabb,
    player: Option<(f64, bool, bool, f64)>,
    cell: impl FnMut(i32, i32, i32) -> (BlockState, Option<(BlockState, DVec3)>),
) -> Vec<Aabb> {
    let mut aabbs = Vec::new();
    visit_block_aabbs_with(region, player, cell, |_, aabb| aabbs.push(aabb));
    aabbs
}

fn visit_block_aabbs_with(
    region: &Aabb,
    player: Option<(f64, bool, bool, f64)>,
    cell: impl FnMut(i32, i32, i32) -> (BlockState, Option<(BlockState, DVec3)>),
    visit: impl FnMut(BlockPos, Aabb),
) {
    visit_block_aabbs_bounded(region, player, usize::MAX, cell, visit);
}

fn visit_block_aabbs_bounded(
    region: &Aabb,
    player: Option<(f64, bool, bool, f64)>,
    max_cells: usize,
    mut cell: impl FnMut(i32, i32, i32) -> (BlockState, Option<(BlockState, DVec3)>),
    mut visit: impl FnMut(BlockPos, Aabb),
) {
    // Include neighboring cells whose offset shapes can protrude into the query.
    let min_x = (region.min.x - MAX_BLOCK_SHAPE_OFFSET).floor() as i32;
    // Fences, walls and closed gates extend 0.5 blocks above their cell.
    let min_y = region.min.y.floor() as i32 - 1;
    let min_z = (region.min.z - MAX_BLOCK_SHAPE_OFFSET).floor() as i32;
    let max_x = (region.max.x + MAX_BLOCK_SHAPE_OFFSET).ceil() as i32;
    let max_y = region.max.y.ceil() as i32;
    let max_z = (region.max.z + MAX_BLOCK_SHAPE_OFFSET).ceil() as i32;

    let mut visited = 0usize;
    for by in min_y..max_y {
        for bz in min_z..max_z {
            for bx in min_x..max_x {
                if visited == max_cells {
                    return;
                }
                visited += 1;
                let (state, piston) = cell(bx, by, bz);
                let mut push = |aabb| visit(BlockPos::new(bx, by, bz), aabb);
                if crate::world::block::block_id(state) == "moving_piston" {
                    if let Some((moved, progress_offset)) = piston {
                        let origin = dvec3(bx as f64, by as f64, bz as f64) + progress_offset;
                        match block_shape::partial_shape(moved) {
                            Some(boxes) => boxes
                                .iter()
                                .for_each(|b| push(Aabb::from_local(*b, origin))),
                            None => push(Aabb::block(bx, by, bz).offset(progress_offset)),
                        }
                    }
                    continue;
                }
                let id = crate::world::block::block_id(state);
                if id == "powder_snow" {
                    if let Some((feet, descending, leather_boots, fall_distance)) = player {
                        if fall_distance > 2.5 {
                            push(Aabb::from_local(
                                [0.0, 0.0, 0.0, 1.0, 0.9_f32 as f64, 1.0],
                                dvec3(bx as f64, by as f64, bz as f64),
                            ));
                        } else if leather_boots
                            && feet > by as f64 + 1.0 - 1.0e-5_f32 as f64
                            && !descending
                        {
                            push(Aabb::block(bx, by, bz));
                        }
                    }
                    continue;
                }
                if id == "scaffolding" {
                    if let Some((feet, descending, _, _)) = player {
                        if !descending && feet > by as f64 + 1.0 - 1.0e-5_f32 as f64 {
                            // Stable frame: upper deck and four corner posts.
                            let origin = dvec3(bx as f64, by as f64, bz as f64);
                            push(Aabb::from_local([0.0, 0.875, 0.0, 1.0, 1.0, 1.0], origin));
                            for x in [0.0, 0.875] {
                                for z in [0.0, 0.875] {
                                    push(Aabb::from_local(
                                        [x, 0.0, z, x + 0.125, 1.0, z + 0.125],
                                        origin,
                                    ));
                                }
                            }
                        } else if feet > by as f64 - 1.0 - 1.0e-5_f32 as f64 {
                            let props = crate::world::block::block_properties(state);
                            if props.get("bottom") == Some("true")
                                && props.get("distance") != Some("0")
                            {
                                push(Aabb::from_local(
                                    [0.0, 0.0, 0.0, 1.0, 0.125, 1.0],
                                    dvec3(bx as f64, by as f64, bz as f64),
                                ));
                            }
                        }
                    }
                    continue;
                }
                match block_shape::partial_shape(state) {
                    Some(boxes) => {
                        let pos = BlockPos::new(bx, by, bz);
                        let offset = dvec3(bx as f64, by as f64, bz as f64)
                            + crate::world::block::block_offset(state, pos);
                        boxes
                            .iter()
                            .for_each(|&b| push(Aabb::from_local(b, offset)));
                    }
                    None => push(Aabb::block(bx, by, bz)),
                }
            }
        }
    }
}

/// Active-recorder requery of the same shape visitor, not resolver evidence.
/// ponytail: first 32 cells / 128 boxes; actual resolver tracing if this
/// ceiling hides the contact.
pub(crate) fn diagnostic_block_shapes(
    chunks: &ChunkStore,
    region: &Aabb,
    context: (f64, bool, bool, f64),
) -> (
    Vec<(BlockPos, BlockState)>,
    Vec<(BlockPos, Aabb)>,
    u64,
    usize,
) {
    let mut cells = Vec::new();
    let mut boxes = Vec::new();
    let mut omitted_boxes = 0;
    visit_block_aabbs_bounded(
        region,
        Some(context),
        32,
        |x, y, z| {
            let result = collision_cell(chunks, x, y, z);
            cells.push((BlockPos::new(x, y, z), result.0));
            result
        },
        |pos, shape| {
            if boxes.len() < 128 {
                boxes.push((pos, shape));
            } else {
                omitted_boxes += 1;
            }
        },
    );
    let extent = |min: f64, max: f64, extra: i64| {
        ((max.ceil() as i64)
            .saturating_sub(min.floor() as i64)
            .saturating_add(extra))
        .max(0) as u64
    };
    let total = extent(region.min.x, region.max.x, 0)
        .saturating_mul(extent(region.min.y, region.max.y, 1))
        .saturating_mul(extent(region.min.z, region.max.z, 0));
    let omitted_cells = total.saturating_sub(cells.len() as u64);
    (cells, boxes, omitted_cells, omitted_boxes)
}

pub fn find_supporting_block(
    chunks: &ChunkStore,
    region: &Aabb,
    position: DVec3,
    context: (f64, bool, bool, f64),
) -> Option<BlockPos> {
    let mut closest: Option<BlockPos> = None;
    let mut distance = f64::MAX;
    visit_block_aabbs_with(
        region,
        Some(context),
        |x, y, z| collision_cell(chunks, x, y, z),
        |pos, shape| {
            if !shape.intersects(region) {
                return;
            }
            let next = (dvec3(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5)
                - position)
                .length_squared();
            // Vec3i.compareTo orders Y, Z, X; equal-distance selection keeps the greatest.
            if next < distance
                || (next == distance
                    && closest.is_none_or(|old| (old.y, old.z, old.x) < (pos.y, pos.z, pos.x)))
            {
                closest = Some(pos);
                distance = next;
            }
        },
    );
    closest
}

pub fn no_collision_for_player(
    chunks: &ChunkStore,
    aabb: &Aabb,
    source: &Aabb,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    context: (bool, bool, f64),
) -> bool {
    let (descending, boots, fall) = context;
    let boxes =
        collect_block_aabbs_for_player(chunks, aabb, Some((source.min.y, descending, boots, fall)));
    if boxes.iter().any(|shape| shape.intersects(aabb))
        || entity_aabbs.iter().any(|shape| shape.intersects(aabb))
    {
        return false;
    }
    // CollisionGetter.noBorderCollision uses the exterior voxel shape, not
    // the resolver's thin walls: queries can be entirely beyond a wall.
    !border_bounds.is_some_and(|[min_x, max_x, min_z, max_z]| {
        let x = (source.min.x + source.max.x) * 0.5;
        let z = (source.min.z + source.max.z) * 0.5;
        let margin = (aabb.max.x - aabb.min.x)
            .max(aabb.max.z - aabb.min.z)
            .max(1.0);
        let distance = (x - min_x).min(max_x - x).min(z - min_z).min(max_z - z);
        distance < margin * 2.0
            && x >= min_x - margin
            && x < max_x + margin
            && z >= min_z - margin
            && z < max_z + margin
            && (aabb.min.x < min_x
                || aabb.max.x > max_x
                || aabb.min.z < min_z
                || aabb.max.z > max_z)
    })
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
    source: &Aabb,
    entities: &[Aabb],
    border: Option<[f64; 4]>,
) {
    aabbs.extend(
        entities
            .iter()
            .copied()
            .filter(|aabb| aabb.intersects(region)),
    );
    if let Some([min_x, max_x, min_z, max_z]) = border.filter(|bounds| {
        // Entity.collectCollidersIgnoringWorldBorder only includes the border
        // when its source is still inside and near it; an entity already
        // outside must be able to re-enter, not be trapped by a wall.
        let [min_x, max_x, min_z, max_z] = *bounds;
        let x = (source.min.x + source.max.x) * 0.5;
        let z = (source.min.z + source.max.z) * 0.5;
        let margin = (region.max.x - region.min.x)
            .max(region.max.z - region.min.z)
            .max(1.0);
        let distance = (x - min_x).min(max_x - x).min(z - min_z).min(max_z - z);
        // The actual voxel shape fills the exterior. Our thin AABB walls
        // cannot represent starting inside that exterior; skip walls there
        // so crossing back into the playable area stays possible.
        distance >= 0.0
            && distance < margin * 2.0
            && x >= min_x
            && x < max_x
            && z >= min_z
            && z < max_z
    }) {
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
    resolve_collision_for_player(
        chunk_store,
        player_aabb,
        velocity,
        step_height,
        was_grounded,
        entity_aabbs,
        border_bounds,
        None,
    )
}

pub fn resolve_collision_for_player(
    chunk_store: &ChunkStore,
    player_aabb: Aabb,
    velocity: Velocity,
    step_height: f64,
    was_grounded: bool,
    entity_aabbs: &[Aabb],
    border_bounds: Option<[f64; 4]>,
    context: Option<(bool, bool, f64)>,
) -> (DVec3, bool) {
    let player =
        context.map(|(descending, boots, fall)| (player_aabb.min.y, descending, boots, fall));
    let expanded = player_aabb.expand(*velocity);
    let mut block_aabbs = collect_block_aabbs_for_player(chunk_store, &expanded, player);
    append_context_aabbs(
        &mut block_aabbs,
        &expanded,
        &player_aabb,
        entity_aabbs,
        border_bounds,
    );

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
        let mut step_aabbs = collect_block_aabbs_for_player(chunk_store, &step_region, player);
        append_context_aabbs(
            &mut step_aabbs,
            &step_region,
            &player_aabb,
            entity_aabbs,
            border_bounds,
        );
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
                // Entity.move: ground is final downward clipping, not step eligibility.
                return (result, velocity.y < 0.0 && result.y != velocity.y);
            }
        }
    }

    (resolved, on_ground)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_bounds_and_requery_do_not_change_collision() {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.partial_storage.set(
            &azalea_core::position::ChunkPos::new(0, 0),
            Some(azalea_world::chunk::Chunk::default()),
            &mut chunks.chunk_storage,
        );
        let scaffold = crate::world::block::find_state("scaffolding", &[]);
        for x in 0..4 {
            for z in 0..2 {
                for y in 0..4 {
                    chunks.set_block_state(x, y, z, scaffold);
                }
            }
        }
        let region = Aabb::new(dvec3(0.0, 1.0, 0.0), dvec3(4.0, 4.0, 2.0));
        let (cells, boxes, omitted_cells, omitted_boxes) =
            diagnostic_block_shapes(&chunks, &region, (10.0, false, false, 0.0));
        assert_eq!(cells.len(), 32);
        assert_eq!(boxes.len(), 128);
        assert_eq!(omitted_cells, 0);
        assert_eq!(omitted_boxes, 32);
        let huge = Aabb::new(DVec3::ZERO, dvec3(100.0, 100.0, 100.0));
        let (cells, _, omitted_cells, _) =
            diagnostic_block_shapes(&chunks, &huge, (10.0, false, false, 0.0));
        assert_eq!(cells.len(), 32);
        assert_eq!(omitted_cells, 1_010_000 - 32);
        let player = Aabb::from_center(dvec3(0.5, 5.0, 0.5), 0.3, 0.9);
        let resolve = || {
            resolve_collision_for_player(
                &chunks,
                player,
                Velocity::new(0.0, -2.0, 0.0),
                0.6,
                false,
                &[],
                None,
                Some((false, false, 0.0)),
            )
        };
        let old = resolve();
        diagnostic_block_shapes(
            &chunks,
            &player.expand(dvec3(0.0, -2.0, 0.0)),
            (5.0, false, false, 0.0),
        );
        assert_eq!(resolve(), old);
        assert_eq!(old, (dvec3(0.0, -1.0, 0.0), true));
    }

    #[test]
    fn explicit_wall_hanging_crossbar_collides_even_when_has_collision_is_false() {
        crate::world::block::init("26.2");
        let air = crate::world::block::find_state("air", &[]);
        let piston = crate::world::block::find_state("moving_piston", &[]);
        for facing in ["north", "south", "east", "west"] {
            let sign =
                crate::world::block::find_state("oak_wall_hanging_sign", &[("facing", facing)]);
            assert!(!crate::world::block::has_collision(sign));
            let bar = if matches!(facing, "north" | "south") {
                [0.0, 0.875, 0.375, 1.0, 1.0, 0.625]
            } else {
                [0.375, 0.875, 0.0, 0.625, 1.0, 1.0]
            };
            for moving in [false, true] {
                let offset = if moving {
                    dvec3(0.25, 0.0, 0.0)
                } else {
                    DVec3::ZERO
                };
                let boxes = collect_block_aabbs_with(&Aabb::block(0, 0, 0), None, |x, y, z| {
                    if (x, y, z) == (0, 0, 0) {
                        if moving {
                            (piston, Some((sign, offset)))
                        } else {
                            (sign, None)
                        }
                    } else {
                        (air, None)
                    }
                });
                assert_eq!(boxes.len(), 1, "{facing} moving={moving}");
                let expected = Aabb::from_local(bar, offset);
                assert_eq!(boxes[0].min, expected.min);
                assert_eq!(boxes[0].max, expected.max);
                let board = Aabb::from_local([0.45, 0.1, 0.45, 0.55, 0.6, 0.55], offset);
                assert!(!boxes[0].intersects(&board), "board must not collide");
                let falling = Aabb::from_local([0.45, 1.25, 0.45, 0.55, 1.5, 0.55], offset);
                let (resolved, grounded) =
                    collide_along_axes(&boxes, falling, Velocity::new(0.0, -0.5, 0.0));
                assert_eq!(resolved.y, -0.25);
                assert!(grounded);
            }
        }
    }

    #[test]
    fn collector_keeps_undefined_noncollidable_blocks_empty_including_moving_pistons() {
        crate::world::block::init("26.2");
        let air = crate::world::block::find_state("air", &[]);
        let piston = crate::world::block::find_state("moving_piston", &[]);
        for id in [
            "air",
            "torch",
            "oak_sign",
            "oak_wall_sign",
            "oak_hanging_sign",
            "stone",
            "iron_chain",
        ] {
            let state = crate::world::block::find_state(id, &[]);
            for moving in [false, true] {
                let offset = if moving {
                    dvec3(0.25, 0.0, 0.0)
                } else {
                    DVec3::ZERO
                };
                let boxes = collect_block_aabbs_with(&Aabb::block(0, 0, 0), None, |x, y, z| {
                    if (x, y, z) == (0, 0, 0) {
                        if moving {
                            (piston, Some((state, offset)))
                        } else {
                            (state, None)
                        }
                    } else {
                        (air, None)
                    }
                });
                if matches!(id, "stone" | "iron_chain") {
                    assert_eq!(boxes.len(), 1, "{id} moving={moving}");
                    let local = block_shape::partial_shape(state)
                        .map(|b| b[0])
                        .unwrap_or([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
                    let expected = Aabb::from_local(local, offset);
                    assert_eq!(boxes[0].min, expected.min);
                    assert_eq!(boxes[0].max, expected.max);
                } else {
                    assert!(boxes.is_empty(), "{id} moving={moving}");
                }
            }
        }
    }

    #[test]
    fn tall_shapes_in_cell_below_region_collide_at_positive_and_negative_heights() {
        crate::world::block::init("26.2");
        let air = crate::world::block::find_state("air", &[]);
        for (id, props, min_x) in [
            (
                "oak_fence",
                &[
                    ("north", "true"),
                    ("east", "false"),
                    ("south", "false"),
                    ("west", "false"),
                ][..],
                0.375,
            ),
            (
                "cobblestone_wall",
                &[
                    ("up", "false"),
                    ("north", "low"),
                    ("east", "none"),
                    ("south", "none"),
                    ("west", "none"),
                ][..],
                0.3125,
            ),
            (
                "oak_fence_gate",
                &[("facing", "east"), ("open", "false")][..],
                0.375,
            ),
        ] {
            let state = crate::world::block::find_state(id, props);
            for by in [0, -1, -2] {
                let origin = dvec3(-2.0, by as f64, -3.0);
                let collect = |region: &Aabb| {
                    collect_block_aabbs_with(region, None, |x, y, z| {
                        (
                            if (x, y, z) == (-2, by, -3) {
                                state
                            } else {
                                air
                            },
                            None,
                        )
                    })
                };
                // North connection only: this region does not touch the central post.
                let overlap = Aabb::from_local([0.4, 1.25, 0.05, 0.6, 1.4, 0.2], origin);
                assert!(
                    collect(&overlap).iter().any(|b| b.intersects(&overlap)),
                    "{id} y={by}"
                );

                let player = overlap.offset(dvec3(-0.8, 0.0, 0.0));
                let velocity = Velocity::new(1.0, 0.0, 0.0);
                let (resolved, _) =
                    collide_along_axes(&collect(&player.expand(*velocity)), player, velocity);
                assert!((resolved.x - (min_x + 0.2)).abs() < 1.0e-9, "{id} y={by}");

                let falling = overlap.offset(dvec3(0.0, 0.5, 0.0));
                let velocity = Velocity::new(0.0, -0.5, 0.0);
                let (resolved, grounded) =
                    collide_along_axes(&collect(&falling.expand(*velocity)), falling, velocity);
                assert_eq!(resolved.y, -0.25, "{id} y={by}");
                assert!(grounded, "{id} y={by}");

                let above = overlap.offset(dvec3(0.0, 0.25, 0.0));
                assert!(
                    collect(&above).iter().all(|b| !b.intersects(&above)),
                    "{id} y={by}"
                );
            }
        }
    }

    #[test]
    fn collision_distance_below_vanilla_epsilon_is_zeroed() {
        let player = Aabb::from_center(dvec3(0.5, 0.0, 0.5), 0.3, 0.9);
        let block = Aabb::block(1, 0, 0);
        let (resolved, _) = collide_along_axes(&[block], player, Velocity::new(5.0e-8, 0.0, 0.0));
        assert_eq!(resolved.x, 0.0);
    }

    #[test]
    fn entity_aabb_stops_player_motion() {
        crate::world::block::init("26.2");
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
        crate::world::block::init("26.2");
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

    #[test]
    fn border_does_not_trap_entity_already_outside() {
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let outside = Aabb::from_center(dvec3(5.5, 0.0, 0.0), 0.3, 0.9);
        let (resolved, _) = resolve_collision_with_context(
            &chunks,
            outside,
            Velocity::new(-1.0, 0.0, 0.0),
            0.0,
            false,
            &[],
            Some([-5.0, 5.0, -5.0, 5.0]),
        );
        assert_eq!(resolved.x, -1.0);
    }
}
