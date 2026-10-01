use glam::{DVec3, dvec3};

use super::block_shape::LocalBox;

/// Vanilla's double `1.0E-7` slop used by `AABB.clip`/voxel collision math.
/// This is intentionally not `Mth.EPSILON`, which is the float `1.0E-5f`.
const EPSILON: f64 = 1.0e-7;

/// `VoxelShape.collideX` tolerance; kept separate from ray clipping.
const COLLISION_EPSILON: f64 = 1.0e-7;

#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    pub fn new(min: DVec3, max: DVec3) -> Self {
        Self { min, max }
    }

    /// Unit cube occupying the block at the given coordinates.
    pub fn block(x: i32, y: i32, z: i32) -> Self {
        Self::new(
            dvec3(x as f64, y as f64, z as f64),
            dvec3((x + 1) as f64, (y + 1) as f64, (z + 1) as f64),
        )
    }

    /// A block-local box placed at `offset`, usually the block position.
    pub fn from_local([min_x, min_y, min_z, max_x, max_y, max_z]: LocalBox, offset: DVec3) -> Self {
        Self::new(
            offset + dvec3(min_x, min_y, min_z),
            offset + dvec3(max_x, max_y, max_z),
        )
    }

    pub fn from_center(center: DVec3, half_width: f64, half_height: f64) -> Self {
        Self {
            min: dvec3(center.x - half_width, center.y, center.z - half_width),
            max: dvec3(
                center.x + half_width,
                center.y + half_height * 2.0,
                center.z + half_width,
            ),
        }
    }

    /// Vanilla `AABB.contains`: half-open, so a point on a max face is outside.
    pub fn contains(&self, point: DVec3) -> bool {
        point.x >= self.min.x
            && point.x < self.max.x
            && point.y >= self.min.y
            && point.y < self.max.y
            && point.z >= self.min.z
            && point.z < self.max.z
    }

    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }

    pub fn offset(self, offset: DVec3) -> Self {
        Self {
            min: self.min + offset,
            max: self.max + offset,
        }
    }

    pub fn deflate(self, amount: f64) -> Self {
        Self {
            min: self.min + amount,
            max: self.max - amount,
        }
    }

    pub fn expand(self, delta: DVec3) -> Self {
        let mut min = self.min;
        let mut max = self.max;

        if delta.x < 0.0 {
            min.x += delta.x;
        } else {
            max.x += delta.x;
        }
        if delta.y < 0.0 {
            min.y += delta.y;
        } else {
            max.y += delta.y;
        }
        if delta.z < 0.0 {
            min.z += delta.z;
        } else {
            max.z += delta.z;
        }

        Self { min, max }
    }

    pub fn clip_x_collide(&self, other: &Aabb, dx: f64) -> f64 {
        self.clip_axis(other, dx, Axis::X)
    }

    pub fn clip_y_collide(&self, other: &Aabb, dy: f64) -> f64 {
        self.clip_axis(other, dy, Axis::Y)
    }

    pub fn clip_z_collide(&self, other: &Aabb, dz: f64) -> f64 {
        self.clip_axis(other, dz, Axis::Z)
    }

    fn clip_axis(&self, other: &Aabb, mut delta: f64, axis: Axis) -> f64 {
        if delta.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        let (c1, c2) = axis.cross_axes();
        for cross in [c1, c2] {
            // Vanilla findIndex uses half-open cells: max - epsilon exactly
            // on the obstacle's min still includes that cell, but min +
            // epsilon exactly on its max excludes it. Do not symmetrize.
            if component(other.max, cross) - COLLISION_EPSILON < component(self.min, cross)
                || component(other.min, cross) + COLLISION_EPSILON >= component(self.max, cross)
            {
                return delta;
            }
        }

        // collideX scans from aMax + 1 / aMin - 1, then checks the signed
        // distance. Both checks matter at exact epsilon and large coordinates.
        if delta > 0.0 && component(other.max, axis) - COLLISION_EPSILON < component(self.min, axis)
        {
            let clip = component(self.min, axis) - component(other.max, axis);
            if clip >= -COLLISION_EPSILON {
                delta = delta.min(clip);
            }
        } else if delta < 0.0
            && component(other.min, axis) + COLLISION_EPSILON >= component(self.max, axis)
        {
            let clip = component(self.max, axis) - component(other.min, axis);
            if clip <= COLLISION_EPSILON {
                delta = delta.max(clip);
            }
        }

        delta
    }
}

/// The face of a box a ray entered through: the axis it lies on plus whether
/// it is the box's max side. Callers that need vanilla's `Direction` map it at
/// their own boundary, keeping this module free of wire types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub axis: Axis,
    pub max: bool,
}

/// Ports vanilla `AABB.clip(Iterable<AABB>, Vec3, Vec3, BlockPos)`: the nearest
/// entry across `boxes` (block-local, shifted by `offset`), as the fraction
/// along `from -> to` and the face entered. `None` if the ray misses them all.
pub fn clip_boxes(
    boxes: &[LocalBox],
    offset: DVec3,
    from: DVec3,
    to: DVec3,
) -> Option<(f64, Face)> {
    let delta = to - from;
    let mut nearest = 1.0;
    let mut face = None;

    for &local in boxes {
        let aabb = Aabb::from_local(local, offset);
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            // A ray only enters through the face it travels towards, so the
            // sign of its component picks which plane to test.
            let d = component(delta, axis);
            let max = if d > EPSILON {
                false
            } else if d < -EPSILON {
                true
            } else {
                continue;
            };
            let plane = component(if max { aabb.max } else { aabb.min }, axis);
            if let Some(t) = clip_point(&aabb, plane, axis, from, delta, nearest) {
                nearest = t;
                face = Some(Face { axis, max });
            }
        }
    }

    face.map(|face| (nearest, face))
}

/// Ports vanilla `AABB.clipPoint`: the fraction at which the ray crosses
/// `plane` on `axis`, if that lands inside the box's face rect and nearer than
/// `nearest`.
fn clip_point(
    aabb: &Aabb,
    plane: f64,
    axis: Axis,
    from: DVec3,
    delta: DVec3,
    nearest: f64,
) -> Option<f64> {
    let t = (plane - component(from, axis)) / component(delta, axis);
    if t <= 0.0 || t >= nearest {
        return None;
    }
    let (b, c) = axis.cross_axes();
    for cross in [b, c] {
        let p = component(from, cross) + t * component(delta, cross);
        if p <= component(aabb.min, cross) - EPSILON || p >= component(aabb.max, cross) + EPSILON {
            return None;
        }
    }
    Some(t)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn cross_axes(self) -> (Axis, Axis) {
        match self {
            Axis::X => (Axis::Y, Axis::Z),
            Axis::Y => (Axis::X, Axis::Z),
            Axis::Z => (Axis::X, Axis::Y),
        }
    }
}

fn component(v: DVec3, axis: Axis) -> f64 {
    match axis {
        Axis::X => v.x,
        Axis::Y => v.y,
        Axis::Z => v.z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTTOM_SLAB: LocalBox = [0.0, 0.0, 0.0, 1.0, 0.5, 1.0];

    fn collide(block: &Aabb, moving: &Aabb, distance: f64, axis: Axis) -> f64 {
        match axis {
            Axis::X => block.clip_x_collide(moving, distance),
            Axis::Y => block.clip_y_collide(moving, distance),
            Axis::Z => block.clip_z_collide(moving, distance),
        }
    }

    // Literal single-cell VoxelShape.collideX oracle, including findIndex's
    // half-open boundaries. Independent of clip_axis's distance predicates.
    fn voxel_collide(block: &Aabb, moving: &Aabb, distance: f64, axis: Axis) -> f64 {
        if distance.abs() < COLLISION_EPSILON {
            return 0.0;
        }
        let index = |axis, coord| {
            [component(block.min, axis), component(block.max, axis)]
                .into_iter()
                .take_while(|&bound| bound <= coord)
                .count() as i32
                - 1
        };
        let (b, c) = axis.cross_axes();
        for cross in [b, c] {
            let min = index(cross, component(moving.min, cross) + COLLISION_EPSILON).max(0);
            let max = (index(cross, component(moving.max, cross) - COLLISION_EPSILON) + 1).min(1);
            if min >= max {
                return distance;
            }
        }
        if distance > 0.0 {
            let start = index(axis, component(moving.max, axis) - COLLISION_EPSILON) + 1;
            for cell in start..1 {
                if cell == 0 {
                    let gap = component(block.min, axis) - component(moving.max, axis);
                    return if gap >= -COLLISION_EPSILON {
                        distance.min(gap)
                    } else {
                        distance
                    };
                }
            }
        } else if distance < 0.0 {
            let start = index(axis, component(moving.min, axis) + COLLISION_EPSILON) - 1;
            for cell in (0..=start).rev() {
                if cell == 0 {
                    let gap = component(block.max, axis) - component(moving.min, axis);
                    return if gap <= COLLISION_EPSILON {
                        distance.max(gap)
                    } else {
                        distance
                    };
                }
            }
        }
        distance
    }

    #[test]
    fn collision_matches_voxel_oracle_for_both_signs_all_axes_and_large_coordinates() {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for origin in [0.0, 1.0, -2.0, -30_000_000.0, 30_000_000.0] {
                let offset = DVec3::splat(origin);
                let block = Aabb::block(0, 0, 0).offset(offset);
                for penetration in [
                    -0.25,
                    -COLLISION_EPSILON,
                    0.0,
                    5.0e-8,
                    COLLISION_EPSILON,
                    2.0e-7,
                    0.25,
                ] {
                    for sign in [-1.0, 1.0] {
                        let mut moving = block;
                        let i = match axis {
                            Axis::X => 0,
                            Axis::Y => 1,
                            Axis::Z => 2,
                        };
                        if sign > 0.0 {
                            moving.max[i] = origin + penetration;
                            moving.min[i] = moving.max[i] - 0.6;
                        } else {
                            moving.min[i] = origin + 1.0 - penetration;
                            moving.max[i] = moving.min[i] + 0.6;
                        }
                        let actual = collide(&block, &moving, sign * 0.5, axis);
                        assert_eq!(
                            actual,
                            voxel_collide(&block, &moving, sign * 0.5, axis),
                            "{axis:?} origin={origin} penetration={penetration} sign={sign}"
                        );
                        if penetration == 0.25 {
                            assert_eq!(
                                actual,
                                sign * 0.5,
                                "do not teleport out of real penetration"
                            );
                        }
                    }
                }
            }
        }
        let wall = Aabb::block(1, 0, 0);
        let moving = Aabb::new(DVec3::ZERO, dvec3(1.0 + 5.0e-8, 1.0, 1.0));
        assert_eq!(wall.clip_x_collide(&moving, 0.1), 1.0 - moving.max.x);
        assert!(wall.clip_x_collide(&moving, 0.1) < 0.0);
    }

    #[test]
    fn collision_exact_epsilon_boundaries_follow_half_open_voxel_indices() {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let i = match axis {
                Axis::X => 0,
                Axis::Y => 1,
                Axis::Z => 2,
            };
            let block = Aabb::new(DVec3::splat(-1.0), DVec3::ZERO);
            let mut moving = block;
            moving.min[i] = -COLLISION_EPSILON;
            moving.max[i] = 0.5;
            assert_eq!(collide(&block, &moving, -0.1, axis), COLLISION_EPSILON);
            let block = Aabb::block(0, 0, 0);
            moving = block;
            moving.min[i] = -0.5;
            moving.max[i] = COLLISION_EPSILON;
            assert_eq!(collide(&block, &moving, 0.1, axis), 0.1);
            moving.max[i] = COLLISION_EPSILON.next_down();
            assert_eq!(collide(&block, &moving, 0.1, axis), -moving.max[i]);
            for cross in [axis.cross_axes().0, axis.cross_axes().1] {
                let j = match cross {
                    Axis::X => 0,
                    Axis::Y => 1,
                    Axis::Z => 2,
                };
                for overlap in [
                    0.0,
                    5.0e-8,
                    COLLISION_EPSILON,
                    COLLISION_EPSILON.next_up(),
                    2.0e-7,
                ] {
                    for near_min in [true, false] {
                        moving = block;
                        moving.min[i] = -1.0;
                        moving.max[i] = 0.0;
                        if near_min {
                            moving.min[j] = -1.0;
                            moving.max[j] = overlap;
                        } else {
                            moving.min[j] = 1.0 - overlap;
                            moving.max[j] = 2.0;
                        }
                        let actual = collide(&block, &moving, 0.1, axis);
                        assert_eq!(
                            actual,
                            voxel_collide(&block, &moving, 0.1, axis),
                            "{axis:?} cross={cross:?} overlap={overlap} near_min={near_min}"
                        );
                        if overlap < COLLISION_EPSILON {
                            assert_eq!(actual, 0.1, "ignore microscopic cross-axis overlap");
                        }
                        if near_min && overlap == COLLISION_EPSILON {
                            assert_eq!(actual, 0.0, "max-epsilon on min includes the cell");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn collision_zeroes_only_sub_epsilon_motion_and_preserves_full_floor() {
        let block = Aabb::block(0, 0, 0);
        let distant = block.offset(DVec3::splat(10.0));
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for distance in [0.0, -0.0, 5.0e-8, -5.0e-8] {
                assert_eq!(
                    collide(&block, &distant, distance, axis).to_bits(),
                    0.0_f64.to_bits()
                );
            }
            for distance in [COLLISION_EPSILON, -COLLISION_EPSILON, 0.1, -0.1] {
                assert_eq!(collide(&block, &distant, distance, axis), distance);
            }
        }
        let standing = Aabb::from_center(dvec3(0.5, 1.0, 0.5), 0.3, 0.9);
        assert_eq!(block.clip_y_collide(&standing, -0.08), 0.0);
        assert_eq!(block.clip_x_collide(&standing, 0.1), 0.1);
        let falling = standing.offset(DVec3::Y * 0.25);
        assert_eq!(block.clip_y_collide(&falling, -0.5), -0.25);
    }

    #[test]
    fn clip_hits_the_top_of_a_slab() {
        let from = dvec3(0.5, 2.0, 0.5);
        let to = dvec3(0.5, -1.0, 0.5);
        let (t, face) = clip_boxes(&[BOTTOM_SLAB], DVec3::ZERO, from, to).unwrap();

        assert_eq!(
            face,
            Face {
                axis: Axis::Y,
                max: true
            }
        );
        let hit = from + (to - from) * t;
        assert!((hit.y - 0.5).abs() < 1e-9, "hit {hit:?}");
    }

    /// A ray passing through the block cell but over the slab's shape misses.
    #[test]
    fn clip_misses_above_a_slab() {
        let from = dvec3(-1.0, 0.75, 0.5);
        let to = dvec3(2.0, 0.75, 0.5);
        assert!(clip_boxes(&[BOTTOM_SLAB], DVec3::ZERO, from, to).is_none());
    }

    /// Vanilla carries `t` across the whole iterable, so listing order can't
    /// change the answer.
    #[test]
    fn clip_keeps_the_nearest_of_several_boxes() {
        let far: LocalBox = [0.0, 0.0, 0.0, 1.0, 0.25, 1.0];
        let near: LocalBox = [0.0, 0.5, 0.0, 1.0, 0.75, 1.0];
        let from = dvec3(0.5, 2.0, 0.5);
        let to = dvec3(0.5, -1.0, 0.5);

        for boxes in [[far, near], [near, far]] {
            let (t, face) = clip_boxes(&boxes, DVec3::ZERO, from, to).unwrap();
            assert_eq!(
                face,
                Face {
                    axis: Axis::Y,
                    max: true
                }
            );
            let hit = from + (to - from) * t;
            assert!((hit.y - 0.75).abs() < 1e-9, "hit {hit:?}");
        }
    }

    #[test]
    fn clip_respects_the_block_offset() {
        let from = dvec3(3.5, 2.0, -4.5);
        let to = dvec3(3.5, -1.0, -4.5);
        let offset = dvec3(3.0, 0.0, -5.0);
        let (t, _) = clip_boxes(&[BOTTOM_SLAB], offset, from, to).unwrap();

        let hit = from + (to - from) * t;
        assert!((hit.y - 0.5).abs() < 1e-9, "hit {hit:?}");
        assert!(clip_boxes(&[BOTTOM_SLAB], DVec3::ZERO, from, to).is_none());
    }
}
