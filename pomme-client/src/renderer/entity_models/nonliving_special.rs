//! Vanilla 26.2 geometry for the nonliving special renderers that use entity
//! models. Coordinates and UV origins were transcribed from the mapped client
//! JAR.

use glam::{Vec3, vec3};

use crate::renderer::entity_model::{
    BakedEntityModel, EntityPart, ModelConvention, ModelCube, bake_model, generate_cube_vertices,
};

fn cube(origin: Vec3, size: Vec3, uv: (i32, i32)) -> ModelCube {
    ModelCube {
        origin,
        size,
        tex_offset: uv,
        deformation: 0.0,
        mirror: false,
    }
}

fn part(name: &str, pivot: Vec3, cubes: Vec<ModelCube>) -> EntityPart {
    EntityPart {
        name: name.into(),
        offset: pivot,
        default_rotation: Vec3::ZERO,
        cubes,
        parent: None,
    }
}

fn bake_y_up_model(parts: Vec<EntityPart>, tex_w: u32, tex_h: u32) -> BakedEntityModel {
    let mut vertices = Vec::new();
    let mut part_ranges = Vec::with_capacity(parts.len());
    for part in &parts {
        let start = vertices.len() as u32;
        for cube in &part.cubes {
            generate_cube_vertices(cube, tex_w, tex_h, 0b11_1111, false, &mut vertices);
        }
        part_ranges.push((start, vertices.len() as u32 - start));
    }
    BakedEntityModel::new(parts, vertices, part_ranges).with_convention(ModelConvention::BlockYUp)
}

/// `ArmorStandModel.createBodyLayer` and its `HumanoidModel.createMesh` parent
/// (26.2), at the adult scale. Armor/hand items are separate item-mesh draws.
pub fn bake_armor_stand_model() -> BakedEntityModel {
    let model = bake_model(
        vec![
            part(
                "head",
                vec3(0.0, 1.0, 0.0),
                vec![cube(vec3(-1.0, -7.0, -1.0), vec3(2.0, 7.0, 2.0), (0, 0))],
            ),
            part(
                "body",
                Vec3::ZERO,
                vec![cube(vec3(-6.0, 0.0, -1.5), vec3(12.0, 3.0, 3.0), (0, 26))],
            ),
            part(
                "right_arm",
                vec3(-5.0, 2.0, 0.0),
                vec![cube(vec3(-2.0, -2.0, -1.0), vec3(2.0, 12.0, 2.0), (24, 0))],
            ),
            part(
                "left_arm",
                vec3(5.0, 2.0, 0.0),
                vec![ModelCube {
                    mirror: true,
                    ..cube(vec3(0.0, -2.0, -1.0), vec3(2.0, 12.0, 2.0), (32, 16))
                }],
            ),
            part(
                "right_leg",
                vec3(-1.9, 12.0, 0.0),
                vec![cube(vec3(-1.0, 0.0, -1.0), vec3(2.0, 11.0, 2.0), (8, 0))],
            ),
            part(
                "left_leg",
                vec3(1.9, 12.0, 0.0),
                vec![ModelCube {
                    mirror: true,
                    ..cube(vec3(-1.0, 0.0, -1.0), vec3(2.0, 11.0, 2.0), (40, 16))
                }],
            ),
            part(
                "right_body_stick",
                Vec3::ZERO,
                vec![cube(vec3(-3.0, 3.0, -1.0), vec3(2.0, 7.0, 2.0), (16, 0))],
            ),
            part(
                "left_body_stick",
                Vec3::ZERO,
                vec![cube(vec3(1.0, 3.0, -1.0), vec3(2.0, 7.0, 2.0), (48, 16))],
            ),
            part(
                "shoulder_stick",
                Vec3::ZERO,
                vec![cube(vec3(-4.0, 10.0, -1.0), vec3(8.0, 2.0, 2.0), (0, 48))],
            ),
            part(
                "base_plate",
                vec3(0.0, 12.0, 0.0),
                vec![cube(vec3(-6.0, 11.0, -6.0), vec3(12.0, 1.0, 12.0), (0, 32))],
            ),
        ],
        64,
        64,
    );
    model
}

/// `EndCrystalModel.createBodyLayer` (26.2), represented in the renderer's
/// literal y-up coordinates. The inner glass and core retain vanilla's nested
/// part hierarchy and scales (0.875 and 0.765625) for transform evaluation.
pub fn bake_end_crystal_model() -> BakedEntityModel {
    let mut model = bake_y_up_model(
        vec![
            part(
                "outer_glass",
                vec3(0.0, 24.0, 0.0),
                vec![cube(vec3(-4.0, -4.0, -4.0), vec3(8.0, 8.0, 8.0), (0, 0))],
            ),
            EntityPart {
                parent: Some(0),
                ..part(
                    "inner_glass",
                    Vec3::ZERO,
                    vec![cube(vec3(-4.0, -4.0, -4.0), vec3(8.0, 8.0, 8.0), (0, 0))],
                )
            },
            EntityPart {
                parent: Some(1),
                ..part(
                    "cube",
                    Vec3::ZERO,
                    vec![cube(vec3(-4.0, -4.0, -4.0), vec3(8.0, 8.0, 8.0), (32, 0))],
                )
            },
            part(
                "base",
                Vec3::ZERO,
                vec![cube(vec3(-6.0, 0.0, -6.0), vec3(12.0, 4.0, 12.0), (0, 16))],
            ),
        ],
        64,
        32,
    );
    model.part_scales[1] = 0.875;
    model.part_scales[2] = 0.765625;
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify(model: BakedEntityModel, dims: (i32, i32), parts: usize, cubes: usize) {
        assert_eq!(model.parts.len(), parts);
        assert!(!model.vertices.is_empty());
        let all = model
            .parts
            .iter()
            .flat_map(|part| &part.cubes)
            .collect::<Vec<_>>();
        assert_eq!(all.len(), cubes);
        assert!(all.iter().all(|c| c.size.min_element() > 0.0
            && c.tex_offset.0 >= 0
            && c.tex_offset.1 >= 0
            && c.tex_offset.0 < dims.0
            && c.tex_offset.1 < dims.1));
    }

    #[test]
    fn vanilla_special_geometries_bake_nonempty_with_in_atlas_uv_origins() {
        let stand = bake_armor_stand_model();
        verify(stand.clone(), (64, 64), 10, 10);
        let stand_cubes = stand
            .parts
            .iter()
            .flat_map(|p| &p.cubes)
            .collect::<Vec<_>>();
        assert_eq!(
            stand_cubes.iter().map(|c| c.tex_offset).collect::<Vec<_>>(),
            vec![
                (0, 0),
                (0, 26),
                (24, 0),
                (32, 16),
                (8, 0),
                (40, 16),
                (16, 0),
                (48, 16),
                (0, 48),
                (0, 32)
            ],
        );
        assert_eq!(
            stand_cubes
                .iter()
                .map(|c| (c.origin.to_array(), c.size.to_array()))
                .collect::<Vec<_>>(),
            vec![
                ([-1.0, -7.0, -1.0], [2.0, 7.0, 2.0]),
                ([-6.0, 0.0, -1.5], [12.0, 3.0, 3.0]),
                ([-2.0, -2.0, -1.0], [2.0, 12.0, 2.0]),
                ([0.0, -2.0, -1.0], [2.0, 12.0, 2.0]),
                ([-1.0, 0.0, -1.0], [2.0, 11.0, 2.0]),
                ([-1.0, 0.0, -1.0], [2.0, 11.0, 2.0]),
                ([-3.0, 3.0, -1.0], [2.0, 7.0, 2.0]),
                ([1.0, 3.0, -1.0], [2.0, 7.0, 2.0]),
                ([-4.0, 10.0, -1.0], [8.0, 2.0, 2.0]),
                ([-6.0, 11.0, -6.0], [12.0, 1.0, 12.0]),
            ],
        );

        let crystal = bake_end_crystal_model();
        verify(crystal.clone(), (64, 32), 4, 4);
        let crystal_cubes = crystal
            .parts
            .iter()
            .flat_map(|p| &p.cubes)
            .collect::<Vec<_>>();
        assert_eq!(
            crystal_cubes
                .iter()
                .map(|c| c.tex_offset)
                .collect::<Vec<_>>(),
            vec![(0, 0), (0, 0), (32, 0), (0, 16)],
        );
        assert_eq!(
            crystal_cubes
                .iter()
                .map(|c| c.size.to_array())
                .collect::<Vec<_>>(),
            vec![
                [8.0, 8.0, 8.0],
                [8.0, 8.0, 8.0],
                [8.0, 8.0, 8.0],
                [12.0, 4.0, 12.0]
            ],
        );
        assert_eq!(crystal.parts[1].parent, Some(0));
        assert_eq!(crystal.parts[2].parent, Some(1));
        assert_eq!(crystal.part_scales[1], 0.875);
        assert_eq!(crystal.part_scales[2], 0.765625);
    }
}
