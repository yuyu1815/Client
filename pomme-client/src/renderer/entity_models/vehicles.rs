//! Static 26.2 vehicle rest meshes, transcribed from mapped client model
//! layers. Dynamic rowing, damage, yaw, cargo block rendering and texture
//! selection are renderer-state responsibilities (see
//! `docs/report-audit/vehicle-models.md`).
use glam::Vec3;

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

const BOAT_TEXTURE_SIZE: (u32, u32) = (128, 64);
const CHEST_BOAT_TEXTURE_SIZE: (u32, u32) = (128, 128);
const RAFT_TEXTURE_SIZE: (u32, u32) = (128, 64);
const CHEST_RAFT_TEXTURE_SIZE: (u32, u32) = (128, 128);
const MINECART_TEXTURE_SIZE: (u32, u32) = (64, 32);

fn cube(origin: [f32; 3], size: [f32; 3], tex_offset: (i32, i32)) -> ModelCube {
    ModelCube {
        origin: Vec3::from_array(origin),
        size: Vec3::from_array(size),
        tex_offset,
        deformation: 0.0,
        mirror: false,
    }
}

fn part(name: &str, offset: [f32; 3], rotation: [f32; 3], cubes: Vec<ModelCube>) -> EntityPart {
    EntityPart {
        name: name.into(),
        offset: Vec3::from_array(offset),
        default_rotation: Vec3::from_array(rotation),
        cubes,
        parent: None,
    }
}

/// Shared vanilla boat hull. `chest` selects the independent 128x128 chest
/// boat layer; wood species select textures outside this geometry bake.
pub fn bake_boat_model(chest: bool) -> BakedEntityModel {
    let mut parts = vec![
        part(
            "bottom",
            [0.0, 3.0, 1.0],
            [std::f32::consts::FRAC_PI_2, 0.0, 0.0],
            vec![cube([-14.0, -9.0, -3.0], [28.0, 16.0, 3.0], (0, 0))],
        ),
        part(
            "back",
            [-15.0, 4.0, 4.0],
            [0.0, 4.712389, 0.0],
            vec![cube([-13.0, -7.0, -1.0], [18.0, 6.0, 2.0], (0, 19))],
        ),
        part(
            "front",
            [15.0, 4.0, 0.0],
            [0.0, std::f32::consts::FRAC_PI_2, 0.0],
            vec![cube([-8.0, -7.0, -1.0], [16.0, 6.0, 2.0], (0, 27))],
        ),
        part(
            "right",
            [0.0, 4.0, -9.0],
            [0.0, std::f32::consts::PI, 0.0],
            vec![cube([-14.0, -7.0, -1.0], [28.0, 6.0, 2.0], (0, 35))],
        ),
        part(
            "left",
            [0.0, 4.0, 9.0],
            [0.0, 0.0, 0.0],
            vec![cube([-14.0, -7.0, -1.0], [28.0, 6.0, 2.0], (0, 43))],
        ),
        part(
            "left_paddle",
            [3.0, -5.0, 9.0],
            [0.0, 0.0, 0.19634955],
            vec![
                cube([-1.0, 0.0, -5.0], [2.0, 2.0, 18.0], (62, 0)),
                cube([-1.001, -3.0, 8.0], [1.0, 6.0, 7.0], (62, 0)),
            ],
        ),
        part(
            "right_paddle",
            [3.0, -5.0, -9.0],
            [0.0, std::f32::consts::PI, 0.19634955],
            vec![
                cube([-1.0, 0.0, -5.0], [2.0, 2.0, 18.0], (62, 20)),
                cube([0.001, -3.0, 8.0], [1.0, 6.0, 7.0], (62, 20)),
            ],
        ),
    ];
    let texture_size = if chest {
        parts.extend([
            part(
                "chest_bottom",
                [-2.0, -5.0, -6.0],
                [0.0, -std::f32::consts::FRAC_PI_2, 0.0],
                vec![cube([0.0, 0.0, 0.0], [12.0, 8.0, 12.0], (0, 76))],
            ),
            part(
                "chest_lid",
                [-2.0, -9.0, -6.0],
                [0.0, -std::f32::consts::FRAC_PI_2, 0.0],
                vec![cube([0.0, 0.0, 0.0], [12.0, 4.0, 12.0], (0, 59))],
            ),
            part(
                "chest_lock",
                [-1.0, -6.0, -1.0],
                [0.0, -std::f32::consts::FRAC_PI_2, 0.0],
                vec![cube([0.0, 0.0, 0.0], [2.0, 4.0, 1.0], (0, 59))],
            ),
        ]);
        CHEST_BOAT_TEXTURE_SIZE
    } else {
        BOAT_TEXTURE_SIZE
    };
    bake_model(parts, texture_size.0, texture_size.1)
}

/// Bamboo raft geometry; the chest raft uses its own 128x128 layer and raised
/// chest pivots from `RaftModel.createChestRaftModel`.
pub fn bake_raft_model(chest: bool) -> BakedEntityModel {
    let mut parts = vec![
        part(
            "bottom",
            [0.0, -2.1, 1.0],
            [1.5708, 0.0, 0.0],
            vec![
                cube([-14.0, -11.0, -4.0], [28.0, 20.0, 4.0], (0, 0)),
                cube([-14.0, -9.0, -8.0], [28.0, 16.0, 4.0], (0, 0)),
            ],
        ),
        part(
            "left_paddle",
            [3.0, -4.0, 9.0],
            [0.0, 0.0, 0.19634955],
            vec![
                cube([-1.0, 0.0, -5.0], [2.0, 2.0, 18.0], (0, 24)),
                cube([-1.001, -3.0, 8.0], [1.0, 6.0, 7.0], (0, 24)),
            ],
        ),
        part(
            "right_paddle",
            [3.0, -4.0, -9.0],
            [0.0, std::f32::consts::PI, 0.19634955],
            vec![
                cube([-1.0, 0.0, -5.0], [2.0, 2.0, 18.0], (40, 24)),
                cube([0.001, -3.0, 8.0], [1.0, 6.0, 7.0], (40, 24)),
            ],
        ),
    ];
    let texture_size = if chest {
        parts.extend([
            part(
                "chest_bottom",
                [-2.0, -10.1, -6.0],
                [0.0, -1.5707964, 0.0],
                vec![cube([0.0, 0.0, 0.0], [12.0, 8.0, 12.0], (0, 76))],
            ),
            part(
                "chest_lid",
                [-2.0, -14.1, -6.0],
                [0.0, -1.5707964, 0.0],
                vec![cube([0.0, 0.0, 0.0], [12.0, 4.0, 12.0], (0, 59))],
            ),
            part(
                "chest_lock",
                [-1.0, -11.1, -1.0],
                [0.0, -1.5707964, 0.0],
                vec![cube([0.0, 0.0, 0.0], [2.0, 4.0, 1.0], (0, 59))],
            ),
        ]);
        CHEST_RAFT_TEXTURE_SIZE
    } else {
        RAFT_TEXTURE_SIZE
    };
    bake_model(parts, texture_size.0, texture_size.1)
}

/// Standard empty cart shell, transcribed from the 64x32 `MinecartModel` layer.
pub fn bake_minecart_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "bottom",
                [0.0, 4.0, 0.0],
                [std::f32::consts::FRAC_PI_2, 0.0, 0.0],
                vec![cube([-10.0, -8.0, -1.0], [20.0, 16.0, 2.0], (0, 10))],
            ),
            part(
                "front",
                [-9.0, 4.0, 0.0],
                [0.0, 4.712389, 0.0],
                vec![cube([-8.0, -9.0, -1.0], [16.0, 8.0, 2.0], (0, 0))],
            ),
            part(
                "back",
                [9.0, 4.0, 0.0],
                [0.0, std::f32::consts::FRAC_PI_2, 0.0],
                vec![cube([-8.0, -9.0, -1.0], [16.0, 8.0, 2.0], (0, 0))],
            ),
            part(
                "left",
                [0.0, 4.0, -7.0],
                [0.0, std::f32::consts::PI, 0.0],
                vec![cube([-8.0, -9.0, -1.0], [16.0, 8.0, 2.0], (0, 0))],
            ),
            part(
                "right",
                [0.0, 4.0, 7.0],
                [0.0, 0.0, 0.0],
                vec![cube([-8.0, -9.0, -1.0], [16.0, 8.0, 2.0], (0, 0))],
            ),
        ],
        MINECART_TEXTURE_SIZE.0,
        MINECART_TEXTURE_SIZE.1,
    )
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    fn assert_cube(
        part: &EntityPart,
        index: usize,
        origin: [f32; 3],
        size: [f32; 3],
        uv: (i32, i32),
    ) {
        let cube = &part.cubes[index];
        assert_eq!(cube.origin, Vec3::from_array(origin));
        assert_eq!(cube.size, Vec3::from_array(size));
        assert_eq!(cube.tex_offset, uv);
        assert_eq!(cube.deformation, 0.0);
        assert!(!cube.mirror);
    }

    fn assert_vertex_bounds(
        model: &BakedEntityModel,
        part_index: usize,
        min: [f32; 3],
        max: [f32; 3],
    ) {
        let (start, count) = model.part_ranges[part_index];
        let vertices = &model.vertices[start as usize..(start + count) as usize];
        let mut actual_min = [f32::INFINITY; 3];
        let mut actual_max = [f32::NEG_INFINITY; 3];
        for vertex in vertices {
            for axis in 0..3 {
                actual_min[axis] = actual_min[axis].min(vertex.position[axis]);
                actual_max[axis] = actual_max[axis].max(vertex.position[axis]);
            }
        }
        assert_eq!(actual_min, min);
        assert_eq!(actual_max, max);
    }

    #[test]
    fn boat_hull_oars_chest_layer_match_26_2_golden_parts() {
        assert_eq!(BOAT_TEXTURE_SIZE, (128, 64));
        assert_eq!(CHEST_BOAT_TEXTURE_SIZE, (128, 128));
        let boat = bake_boat_model(false);
        assert_eq!(
            boat.parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "bottom",
                "back",
                "front",
                "right",
                "left",
                "left_paddle",
                "right_paddle",
            ]
        );
        assert!(boat.parts.iter().all(|part| part.parent.is_none()));
        let floor = &boat.parts[0];
        assert_eq!(floor.offset, Vec3::new(0.0, 3.0, 1.0));
        assert_eq!(floor.default_rotation, Vec3::new(1.5707964, 0.0, 0.0));
        assert_eq!(floor.parent, None);
        assert_cube(floor, 0, [-14.0, -9.0, -3.0], [28.0, 16.0, 3.0], (0, 0));
        assert_vertex_bounds(&boat, 0, [-0.875, -0.4375, -0.1875], [0.875, 0.5625, 0.0]);

        let side = &boat.parts[3];
        assert_eq!(side.offset, Vec3::new(0.0, 4.0, -9.0));
        assert_eq!(
            side.default_rotation,
            Vec3::new(0.0, std::f32::consts::PI, 0.0)
        );
        assert_cube(side, 0, [-14.0, -7.0, -1.0], [28.0, 6.0, 2.0], (0, 35));
        let oar = &boat.parts[5];
        assert_eq!(oar.offset, Vec3::new(3.0, -5.0, 9.0));
        assert_eq!(oar.default_rotation, Vec3::new(0.0, 0.0, 0.19634955));
        assert_cube(oar, 0, [-1.0, 0.0, -5.0], [2.0, 2.0, 18.0], (62, 0));
        assert_cube(oar, 1, [-1.001, -3.0, 8.0], [1.0, 6.0, 7.0], (62, 0));

        let chest_boat = bake_boat_model(true);
        assert_eq!(chest_boat.parts.len(), 10);
        assert!(chest_boat.parts.iter().all(|part| part.parent.is_none()));
        let chest_floor = &chest_boat.parts[7];
        assert_eq!(chest_floor.name, "chest_bottom");
        assert_eq!(chest_floor.offset, Vec3::new(-2.0, -5.0, -6.0));
        assert_cube(chest_floor, 0, [0.0; 3], [12.0, 8.0, 12.0], (0, 76));
        assert_eq!(chest_boat.parts[8].name, "chest_lid");
        assert_cube(
            &chest_boat.parts[8],
            0,
            [0.0; 3],
            [12.0, 4.0, 12.0],
            (0, 59),
        );
        assert_eq!(chest_boat.parts[9].name, "chest_lock");
        assert_cube(&chest_boat.parts[9], 0, [0.0; 3], [2.0, 4.0, 1.0], (0, 59));
    }

    #[test]
    fn raft_planks_chest_pivots_and_texture_layers_match_26_2_golden_parts() {
        assert_eq!(RAFT_TEXTURE_SIZE, (128, 64));
        assert_eq!(CHEST_RAFT_TEXTURE_SIZE, (128, 128));
        let raft = bake_raft_model(false);
        assert_eq!(
            raft.parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["bottom", "left_paddle", "right_paddle"]
        );
        assert!(raft.parts.iter().all(|part| part.parent.is_none()));
        let bottom = &raft.parts[0];
        assert_eq!(bottom.name, "bottom");
        assert_eq!(bottom.offset, Vec3::new(0.0, -2.1, 1.0));
        assert_eq!(bottom.default_rotation, Vec3::new(1.5708, 0.0, 0.0));
        assert_cube(bottom, 0, [-14.0, -11.0, -4.0], [28.0, 20.0, 4.0], (0, 0));
        assert_cube(bottom, 1, [-14.0, -9.0, -8.0], [28.0, 16.0, 4.0], (0, 0));
        assert_vertex_bounds(&raft, 0, [-0.875, -0.5625, -0.5], [0.875, 0.6875, 0.0]);
        let left_oar = &raft.parts[1];
        assert_eq!(left_oar.offset, Vec3::new(3.0, -4.0, 9.0));
        assert_cube(left_oar, 0, [-1.0, 0.0, -5.0], [2.0, 2.0, 18.0], (0, 24));
        assert_cube(left_oar, 1, [-1.001, -3.0, 8.0], [1.0, 6.0, 7.0], (0, 24));

        let chest_raft = bake_raft_model(true);
        assert_eq!(
            chest_raft
                .parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "bottom",
                "left_paddle",
                "right_paddle",
                "chest_bottom",
                "chest_lid",
                "chest_lock",
            ]
        );
        assert!(chest_raft.parts.iter().all(|part| part.parent.is_none()));
        let chest_floor = &chest_raft.parts[3];
        assert_eq!(chest_floor.name, "chest_bottom");
        assert_eq!(chest_floor.offset, Vec3::new(-2.0, -10.1, -6.0));
        assert_cube(chest_floor, 0, [0.0; 3], [12.0, 8.0, 12.0], (0, 76));
        assert_eq!(chest_raft.parts[4].offset, Vec3::new(-2.0, -14.1, -6.0));
        assert_eq!(chest_raft.parts[5].offset, Vec3::new(-1.0, -11.1, -1.0));
        assert_cube(&chest_raft.parts[5], 0, [0.0; 3], [2.0, 4.0, 1.0], (0, 59));
    }

    #[test]
    fn minecart_shell_bottom_and_walls_match_26_2_golden_parts() {
        assert_eq!(MINECART_TEXTURE_SIZE, (64, 32));
        let cart = bake_minecart_model();
        assert_eq!(
            cart.parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["bottom", "front", "back", "left", "right"]
        );
        let bottom = &cart.parts[0];
        assert_eq!(bottom.offset, Vec3::new(0.0, 4.0, 0.0));
        assert_eq!(bottom.default_rotation, Vec3::new(1.5707964, 0.0, 0.0));
        assert_cube(bottom, 0, [-10.0, -8.0, -1.0], [20.0, 16.0, 2.0], (0, 10));
        assert_vertex_bounds(&cart, 0, [-0.625, -0.5, -0.0625], [0.625, 0.5, 0.0625]);
        let front = &cart.parts[1];
        assert_eq!(front.offset, Vec3::new(-9.0, 4.0, 0.0));
        assert_eq!(front.default_rotation, Vec3::new(0.0, 4.712389, 0.0));
        assert_cube(front, 0, [-8.0, -9.0, -1.0], [16.0, 8.0, 2.0], (0, 0));
        assert_eq!(cart.parts[2].offset, Vec3::new(9.0, 4.0, 0.0));
        assert_eq!(
            cart.parts[2].default_rotation,
            Vec3::new(0.0, 1.5707964, 0.0)
        );
        assert_eq!(cart.parts[3].offset, Vec3::new(0.0, 4.0, -7.0));
        assert_eq!(cart.parts[4].offset, Vec3::new(0.0, 4.0, 7.0));
        assert!(cart.parts.iter().all(|part| part.parent.is_none()));
    }
}
