//! Entity-model parts for humanoids not covered by the existing player/skeleton
//! meshes. Meshes follow vanilla 26.2's 64x64 humanoid UV space.
//!
//! This module is intentionally not registered yet: `renderer::mod.rs` and the
//! entity-model pipeline are owned by the coordinating integration change.

use glam::{Vec3, vec3};

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

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

/// `IllagerModel.createBodyLayer` (26.2): head/nose, two-piece robe, the
/// crooked joined arms, and articulated legs. The hat child is deliberately
/// omitted because vanilla initializes it invisible.
pub fn bake_illager_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "head",
                vec3(0.0, 0.0, 0.0),
                vec![cube(vec3(-4.0, -10.0, -4.0), vec3(8.0, 10.0, 8.0), (0, 0))],
            ),
            EntityPart {
                name: "nose".into(),
                offset: vec3(0.0, -2.0, 0.0),
                default_rotation: Vec3::ZERO,
                cubes: vec![cube(vec3(-1.0, -1.0, -6.0), vec3(2.0, 4.0, 2.0), (24, 0))],
                parent: Some(0),
            },
            part(
                "body",
                Vec3::ZERO,
                vec![
                    cube(vec3(-4.0, 0.0, -3.0), vec3(8.0, 12.0, 6.0), (16, 20)),
                    cube(vec3(-4.0, 0.0, -3.0), vec3(8.0, 20.0, 6.0), (0, 38)),
                ],
            ),
            EntityPart {
                name: "arms".into(),
                offset: vec3(0.0, 3.0, -1.0),
                default_rotation: vec3(0.0, 0.0, -0.75),
                cubes: vec![
                    cube(vec3(-8.0, -2.0, -2.0), vec3(4.0, 8.0, 4.0), (44, 22)),
                    cube(vec3(-4.0, 2.0, -2.0), vec3(8.0, 4.0, 4.0), (40, 38)),
                ],
                parent: None,
            },
            part(
                "right_leg",
                vec3(-2.0, 12.0, 0.0),
                vec![cube(vec3(-2.0, 0.0, -2.0), vec3(4.0, 12.0, 4.0), (0, 22))],
            ),
            part(
                "left_leg",
                vec3(2.0, 12.0, 0.0),
                vec![cube(vec3(-2.0, 0.0, -2.0), vec3(4.0, 12.0, 4.0), (0, 22))],
            ),
        ],
        64,
        64,
    )
}

/// Piglin body: broad pig head with snout and ears, humanoid torso, folded
/// arms, and the short leg proportions used by the piglin family model.
pub fn bake_piglin_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "head",
                vec3(0.0, 0.0, 0.0),
                vec![
                    cube(vec3(-5.0, -8.0, -4.0), vec3(10.0, 8.0, 8.0), (0, 0)),
                    cube(vec3(-2.0, -4.0, -5.0), vec3(4.0, 4.0, 1.0), (31, 1)),
                    cube(vec3(2.0, -2.0, -5.0), vec3(1.0, 2.0, 1.0), (2, 4)),
                    cube(vec3(-3.0, -2.0, -5.0), vec3(1.0, 2.0, 1.0), (2, 0)),
                ],
            ),
            EntityPart {
                name: "right_ear".into(),
                offset: vec3(-4.5, -6.0, 0.0),
                default_rotation: vec3(0.0, 0.0, 0.5235988),
                cubes: vec![cube(vec3(-1.0, 0.0, -2.0), vec3(1.0, 5.0, 4.0), (39, 6))],
                parent: Some(0),
            },
            EntityPart {
                name: "left_ear".into(),
                offset: vec3(4.5, -6.0, 0.0),
                default_rotation: vec3(0.0, 0.0, -0.5235988),
                cubes: vec![cube(vec3(0.0, 0.0, -2.0), vec3(1.0, 5.0, 4.0), (51, 6))],
                parent: Some(0),
            },
            part(
                "body",
                Vec3::ZERO,
                vec![cube(vec3(-4.0, 0.0, -2.0), vec3(8.0, 12.0, 4.0), (16, 16))],
            ),
            part(
                "right_arm",
                vec3(-5.0, 0.0, 0.0),
                vec![cube(vec3(-3.0, -2.0, -2.0), vec3(4.0, 12.0, 4.0), (40, 16))],
            ),
            part(
                "left_arm",
                vec3(5.0, 0.0, 0.0),
                vec![cube(vec3(-1.0, -2.0, -2.0), vec3(4.0, 12.0, 4.0), (40, 16))],
            ),
            part(
                "right_leg",
                vec3(-2.0, 10.0, 0.0),
                vec![cube(vec3(-2.0, 0.0, -2.0), vec3(4.0, 12.0, 4.0), (0, 16))],
            ),
            part(
                "left_leg",
                vec3(2.0, 10.0, 0.0),
                vec![cube(vec3(-2.0, 0.0, -2.0), vec3(4.0, 12.0, 4.0), (0, 16))],
            ),
        ],
        64,
        64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanoid_meshes_have_real_geometry_and_vanilla_atlas_size() {
        let illager = bake_illager_model();
        assert_eq!(illager.parts.len(), 6);
        assert!(!illager.vertices.is_empty());
        assert_eq!(illager.parts[1].cubes[0].tex_offset, (24, 0));
        assert_eq!(illager.parts[2].cubes.len(), 2);

        let piglin = bake_piglin_model();
        assert_eq!(piglin.parts.len(), 8);
        assert!(!piglin.vertices.is_empty());
        assert_eq!(piglin.parts[0].cubes.len(), 4);
        assert_eq!(piglin.parts[1].cubes[0].tex_offset, (39, 6));
    }
}
