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

fn child(name: &str, pivot: Vec3, cubes: Vec<ModelCube>, parent: usize) -> EntityPart {
    EntityPart {
        parent: Some(parent),
        ..part(name, pivot, cubes)
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
                default_rotation: vec3(-0.75, 0.0, 0.0),
                cubes: vec![
                    cube(vec3(-8.0, -2.0, -2.0), vec3(4.0, 8.0, 4.0), (44, 22)),
                    cube(vec3(-4.0, 2.0, -2.0), vec3(8.0, 4.0, 4.0), (40, 38)),
                    ModelCube {
                        mirror: true,
                        ..cube(vec3(4.0, -2.0, -2.0), vec3(4.0, 8.0, 4.0), (44, 22))
                    },
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
                vec![ModelCube {
                    mirror: true,
                    ..cube(vec3(-2.0, 0.0, -2.0), vec3(4.0, 12.0, 4.0), (0, 22))
                }],
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
                vec3(-5.0, 2.0, 0.0),
                vec![cube(vec3(-3.0, -2.0, -2.0), vec3(4.0, 12.0, 4.0), (40, 16))],
            ),
            part(
                "left_arm",
                vec3(5.0, 2.0, 0.0),
                vec![ModelCube {
                    mirror: true,
                    ..cube(vec3(-1.0, -2.0, -2.0), vec3(4.0, 12.0, 4.0), (32, 48))
                }],
            ),
            part(
                "right_leg",
                vec3(-1.9, 12.0, 0.0),
                vec![cube(vec3(-2.0, 0.0, -2.0), vec3(4.0, 12.0, 4.0), (0, 16))],
            ),
            part(
                "left_leg",
                vec3(1.9, 12.0, 0.0),
                vec![ModelCube {
                    mirror: true,
                    ..cube(vec3(-2.0, 0.0, -2.0), vec3(4.0, 12.0, 4.0), (16, 48))
                }],
            ),
        ],
        64,
        64,
    )
}

/// Same cube declaration helper with vanilla's signed deformation retained.
fn cube_d(origin: Vec3, size: Vec3, uv: (i32, i32), deformation: f32) -> ModelCube {
    ModelCube {
        deformation,
        ..cube(origin, size, uv)
    }
}

/// Vanilla EndermiteModel: four independently bobbing segments, with the exact
/// 64x32 texture atlas coordinates and body sizes from BODY_SIZES/BODY_TEXS.
pub fn bake_endermite_model() -> BakedEntityModel {
    let sizes = [(4., 3., 2.), (6., 4., 5.), (3., 3., 1.), (1., 2., 1.)];
    let tex = [(0, 0), (0, 5), (0, 14), (0, 18)];
    let mut x = -3.5;
    let parts = sizes
        .into_iter()
        .enumerate()
        .map(|(i, (w, h, d))| {
            let p = part(
                &format!("segment_{i}"),
                vec3(0.0, 24.0 - h, x),
                vec![cube(vec3(-w / 2.0, 0.0, -d / 2.0), vec3(w, h, d), tex[i])],
            );
            if i < 3 {
                x += (d + sizes[i + 1].2) * 0.5;
            }
            p
        })
        .collect();
    bake_model(parts, 64, 32)
}

/// Vanilla SilverfishModel: seven overlapping segmented plates and paired
/// gill-like lateral layers. Atlas is 64x32, matching BODY_SIZES/BODY_TEXS.
pub fn bake_silverfish_model() -> BakedEntityModel {
    let sizes = [
        (3., 2., 2.),
        (4., 3., 2.),
        (6., 4., 3.),
        (3., 3., 3.),
        (2., 2., 3.),
        (2., 1., 2.),
        (1., 1., 2.),
    ];
    let tex = [(0, 0), (0, 4), (0, 9), (0, 16), (0, 22), (11, 0), (13, 4)];
    let mut x = -3.5;
    let mut parts = Vec::new();
    for (i, (w, h, d)) in sizes.into_iter().enumerate() {
        parts.push(part(
            &format!("segment_{i}"),
            vec3(0.0, 24.0 - h, x),
            vec![cube(vec3(-w / 2.0, 0.0, -d / 2.0), vec3(w, h, d), tex[i])],
        ));
        if i < 6 {
            x += (d + sizes[i + 1].2) * 0.5;
        }
    }
    for (name, origin, size, pivot, uv) in [
        (
            "layer_0",
            vec3(-5.0, 0.0, -1.5),
            vec3(10.0, 8.0, 3.0),
            vec3(0.0, 16.0, -1.5),
            (20, 0),
        ),
        (
            "layer_1",
            vec3(-3.0, 0.0, -1.5),
            vec3(6.0, 4.0, 3.0),
            vec3(0.0, 20.0, -1.5),
            (20, 11),
        ),
        (
            "layer_2",
            vec3(-3.0, 0.0, -1.0),
            vec3(6.0, 5.0, 2.0),
            vec3(0.0, 19.0, -1.0),
            (20, 18),
        ),
    ] {
        parts.push(part(
            name,
            vec3(pivot.x, 24.0 - pivot.y, pivot.z),
            vec![cube(origin, size, uv)],
        ));
    }
    bake_model(parts, 64, 32)
}

/// Eight one-pixel magma membranes over the 4x4 core, exactly the vanilla
/// MagmaCubeModel 64x64 layer. Squish animation remains a renderer concern.
pub fn bake_magma_cube_model() -> BakedEntityModel {
    let mut parts: Vec<_> = (0..8)
        .map(|i| {
            let (u, v) = if i == 0 {
                (0, 0)
            } else if i < 4 {
                (0, 9 * i)
            } else {
                (32, 9 * i - 36)
            };
            part(
                &format!("cube_{i}"),
                Vec3::ZERO,
                vec![cube(
                    vec3(-4.0, 16.0 + i as f32, -4.0),
                    vec3(8.0, 1.0, 8.0),
                    (u, v),
                )],
            )
        })
        .collect();
    parts.push(part(
        "inside_cube",
        Vec3::ZERO,
        vec![cube(vec3(-2.0, 18.0, -2.0), vec3(4.0, 4.0, 4.0), (24, 40))],
    ));
    bake_model(parts, 64, 64)
}

/// Sulfur Cube's independent 18px outer and 16px inner meshes (both use
/// 128x128 UVs). They must be submitted as separate pipeline variants/layers.
pub fn bake_sulfur_cube_outer_model() -> BakedEntityModel {
    bake_model(
        vec![part(
            "cube",
            Vec3::ZERO,
            vec![cube(vec3(-9.0, -9.0, -9.0), vec3(18.0, 18.0, 18.0), (0, 0))],
        )],
        128,
        128,
    )
}
pub fn bake_sulfur_cube_inner_model() -> BakedEntityModel {
    bake_model(
        vec![part(
            "cube",
            Vec3::ZERO,
            vec![cube(
                vec3(-8.0, -8.0, -8.0),
                vec3(16.0, 16.0, 16.0),
                (0, 36),
            )],
        )],
        128,
        128,
    )
}
pub fn bake_sulfur_cube_small_outer_model() -> BakedEntityModel {
    bake_model(
        vec![part(
            "cube",
            Vec3::ZERO,
            vec![cube(vec3(-5.0, -5.0, -5.0), vec3(10.0, 10.0, 10.0), (0, 0))],
        )],
        64,
        64,
    )
}
pub fn bake_sulfur_cube_small_inner_model() -> BakedEntityModel {
    bake_model(
        vec![part(
            "cube",
            Vec3::ZERO,
            vec![cube(vec3(-4.0, -4.0, -4.0), vec3(8.0, 8.0, 8.0), (0, 20))],
        )],
        64,
        64,
    )
}

/// ShulkerModel.createBodyLayer: openable lid, fixed lower shell, and exposed
/// head. The draw path must animate lid offset from (0,24,0) when peeking.
pub fn bake_shulker_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "lid",
                vec3(0.0, 24.0, 0.0),
                vec![cube(
                    vec3(-8.0, -16.0, -8.0),
                    vec3(16.0, 12.0, 16.0),
                    (0, 0),
                )],
            ),
            part(
                "base",
                vec3(0.0, 24.0, 0.0),
                vec![cube(vec3(-8.0, -8.0, -8.0), vec3(16.0, 8.0, 16.0), (0, 28))],
            ),
            part(
                "head",
                vec3(0.0, 12.0, 0.0),
                vec![cube(vec3(-3.0, 0.0, -3.0), vec3(6.0, 6.0, 6.0), (0, 52))],
            ),
        ],
        64,
        64,
    )
}

/// SnowGolemModel.createBodyLayer: carved head, narrow stick arms and the
/// distinct 10x10 upper/12x12 lower snow sections. Pumpkin is a separate item
/// layer.
pub fn bake_snow_golem_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "head",
                vec3(0.0, 4.0, 0.0),
                vec![cube_d(
                    vec3(-4.0, -8.0, -4.0),
                    vec3(8.0, 8.0, 8.0),
                    (0, 0),
                    -0.5,
                )],
            ),
            part(
                "left_arm",
                vec3(5.0, 6.0, 1.0),
                vec![cube_d(
                    vec3(-1.0, 0.0, -1.0),
                    vec3(12.0, 2.0, 2.0),
                    (32, 0),
                    -0.5,
                )],
            ),
            part(
                "right_arm",
                vec3(-5.0, 6.0, -1.0),
                vec![cube_d(
                    vec3(-11.0, 0.0, -1.0),
                    vec3(12.0, 2.0, 2.0),
                    (32, 0),
                    -0.5,
                )],
            ),
            part(
                "upper_body",
                vec3(0.0, 13.0, 0.0),
                vec![cube_d(
                    vec3(-5.0, -10.0, -5.0),
                    vec3(10.0, 10.0, 10.0),
                    (16, 0),
                    -0.5,
                )],
            ),
            part(
                "lower_body",
                vec3(0.0, 24.0, 0.0),
                vec![cube_d(
                    vec3(-6.0, -12.0, -6.0),
                    vec3(12.0, 12.0, 12.0),
                    (36, 0),
                    -0.5,
                )],
            ),
        ],
        64,
        64,
    )
}

/// TadpoleModel: the 3x2x3 body and 0x2x7 tail in the 16x16 atlas.
pub fn bake_tadpole_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "body",
                vec3(0.0, 22.0, -3.0),
                vec![cube(vec3(-1.5, -1.0, 0.0), vec3(3.0, 2.0, 3.0), (0, 0))],
            ),
            part(
                "tail",
                vec3(0.0, 22.0, 0.0),
                vec![cube(vec3(0.0, -1.0, 0.0), vec3(0.0, 2.0, 7.0), (0, 0))],
            ),
        ],
        16,
        16,
    )
}

/// Parched uses SkeletonModel.createSingleModelDualBodyLayer (64x64): the
/// dehydrated-specific layer adds a body shell and enlarged head shell, so this
/// must not be flattened to the regular 64x32 SkeletonModel mesh.
pub fn bake_parched_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "body",
                Vec3::ZERO,
                vec![
                    cube(vec3(-4.0, 0.0, -2.0), vec3(8.0, 12.0, 4.0), (16, 16)),
                    cube(vec3(-4.0, 10.0, -2.0), vec3(8.0, 1.0, 4.0), (28, 0)),
                    cube_d(vec3(-4.0, 0.0, -2.0), vec3(8.0, 12.0, 4.0), (16, 48), 0.025),
                ],
            ),
            part(
                "head",
                Vec3::ZERO,
                vec![
                    cube(vec3(-4.0, -8.0, -4.0), vec3(8.0, 8.0, 8.0), (0, 0)),
                    cube_d(vec3(-4.0, -8.0, -4.0), vec3(8.0, 8.0, 8.0), (0, 32), 0.2),
                ],
            ),
            part(
                "right_arm",
                vec3(-5.5, 2.0, 0.0),
                vec![
                    cube(vec3(-1.0, -2.0, -1.0), vec3(2.0, 12.0, 2.0), (40, 16)),
                    cube(vec3(-1.55, -2.025, -1.5), vec3(3.0, 12.0, 3.0), (42, 33)),
                ],
            ),
            part(
                "left_arm",
                vec3(5.5, 2.0, 0.0),
                vec![
                    cube(vec3(-1.0, -2.0, -1.0), vec3(2.0, 12.0, 2.0), (56, 16)),
                    cube(vec3(-1.45, -2.025, -1.5), vec3(3.0, 12.0, 3.0), (40, 48)),
                ],
            ),
            part(
                "right_leg",
                vec3(-2.0, 12.0, 0.0),
                vec![
                    cube(vec3(-1.0, 0.0, -1.0), vec3(2.0, 12.0, 2.0), (0, 16)),
                    cube(vec3(-1.5, 0.0, -1.5), vec3(3.0, 12.0, 3.0), (0, 49)),
                ],
            ),
            part(
                "left_leg",
                vec3(2.0, 12.0, 0.0),
                vec![
                    cube(vec3(-1.0, 0.0, -1.0), vec3(2.0, 12.0, 2.0), (0, 16)),
                    cube(vec3(-1.5, 0.0, -1.5), vec3(3.0, 12.0, 3.0), (4, 49)),
                ],
            ),
        ],
        64,
        64,
    )
}
pub fn bake_wither_skeleton_model() -> BakedEntityModel {
    // MeshTransformer scales geometry and pivots, not the skeleton's 64x32 UV
    // atlas. Keep the existing part indices used by animation and armor
    // extraction.
    crate::renderer::entity_model::bake_independent_roots_scaled(
        crate::renderer::entity_model::bake_skeleton_model().parts,
        1.2,
        64,
        32,
    )
}

/// WanderingTraderRenderer uses VillagerModel's nose/robe layer with its own
/// 64x64 trader texture; no profession clothing layer is appropriate.
pub fn bake_wandering_trader_model() -> BakedEntityModel {
    crate::renderer::entity_model::bake_villager_model(false)
}

/// CopperGolemModel.createBodyLayer: stock body, offset head with nose/ears,
/// two arms and five-unit legs on the 64x64 atlas. Pose/oxidation layers stay
/// external.
pub fn bake_copper_golem_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "body",
                vec3(0.0, 19.0, 0.0),
                vec![cube(vec3(-4.0, -6.0, -3.0), vec3(8.0, 6.0, 6.0), (0, 15))],
            ),
            child(
                "head",
                vec3(0.0, -6.0, 0.0),
                vec![
                    cube_d(vec3(-4.0, -5.0, -5.0), vec3(8.0, 5.0, 10.0), (0, 0), 0.015),
                    cube(vec3(-1.0, -2.0, -6.0), vec3(2.0, 3.0, 2.0), (56, 0)),
                    cube_d(vec3(-1.0, -9.0, -1.0), vec3(2.0, 4.0, 2.0), (37, 8), -0.015),
                    cube_d(
                        vec3(-2.0, -13.0, -2.0),
                        vec3(4.0, 4.0, 4.0),
                        (37, 0),
                        -0.015,
                    ),
                ],
                0,
            ),
            child(
                "right_arm",
                vec3(-4.0, -6.0, 0.0),
                vec![cube(vec3(-3.0, -1.0, -2.0), vec3(3.0, 10.0, 4.0), (36, 16))],
                0,
            ),
            child(
                "left_arm",
                vec3(4.0, -6.0, 0.0),
                vec![cube(vec3(0.0, -1.0, -2.0), vec3(3.0, 10.0, 4.0), (50, 16))],
                0,
            ),
            part(
                "right_leg",
                vec3(0.0, 19.0, 0.0),
                vec![cube(vec3(-4.0, 0.0, -2.0), vec3(4.0, 5.0, 4.0), (0, 27))],
            ),
            part(
                "left_leg",
                vec3(0.0, 19.0, 0.0),
                vec![cube(vec3(0.0, 0.0, -2.0), vec3(4.0, 5.0, 4.0), (16, 27))],
            ),
        ],
        64,
        64,
    )
}
/// CreakingModel.createMesh: bark trunk, split ribcage, oversized carved head,
/// long twig arms/legs, and separate hanging tendrils; 64x64 UV atlas.
pub fn bake_creaking_model() -> BakedEntityModel {
    bake_model(
        vec![
            part("root", vec3(0.0, 24.0, 0.0), vec![]),
            child("upper_body", vec3(-1.0, -19.0, 0.0), vec![], 0),
            child(
                "head",
                vec3(-3.0, -11.0, 0.0),
                vec![
                    cube(vec3(-3.0, -10.0, -3.0), vec3(6.0, 10.0, 6.0), (0, 0)),
                    cube(vec3(-3.0, -13.0, -3.0), vec3(6.0, 3.0, 6.0), (28, 31)),
                    cube(vec3(3.0, -13.0, 0.0), vec3(9.0, 14.0, 0.0), (12, 40)),
                    cube(vec3(-12.0, -14.0, 0.0), vec3(9.0, 14.0, 0.0), (34, 12)),
                ],
                1,
            ),
            child(
                "body",
                vec3(0.0, -7.0, 1.0),
                vec![
                    cube(vec3(0.0, -3.0, -3.0), vec3(6.0, 13.0, 5.0), (0, 16)),
                    cube(vec3(-6.0, -4.0, -3.0), vec3(6.0, 7.0, 5.0), (24, 0)),
                ],
                1,
            ),
            child(
                "right_arm",
                vec3(-7.0, -9.5, 1.5),
                vec![
                    cube(vec3(-2.0, -1.5, -1.5), vec3(3.0, 21.0, 3.0), (22, 13)),
                    cube(vec3(-2.0, 19.5, -1.5), vec3(3.0, 4.0, 3.0), (46, 0)),
                ],
                1,
            ),
            child(
                "left_arm",
                vec3(6.0, -9.0, 0.5),
                vec![
                    cube(vec3(0.0, -1.0, -1.5), vec3(3.0, 16.0, 3.0), (30, 40)),
                    cube(vec3(0.0, -5.0, -1.5), vec3(3.0, 4.0, 3.0), (52, 12)),
                    cube(vec3(0.0, 15.0, -1.5), vec3(3.0, 4.0, 3.0), (52, 19)),
                ],
                1,
            ),
            child(
                "left_leg",
                vec3(1.5, -16.0, 0.5),
                vec![
                    cube(vec3(-1.5, 0.0, -1.5), vec3(3.0, 16.0, 3.0), (42, 40)),
                    cube(vec3(-1.5, 15.7, -4.5), vec3(5.0, 0.0, 9.0), (45, 55)),
                ],
                0,
            ),
            child(
                "right_leg",
                vec3(-1.0, -17.5, 0.5),
                vec![
                    cube(vec3(-3.0, -1.5, -1.5), vec3(3.0, 19.0, 3.0), (0, 34)),
                    cube(vec3(-5.0, 17.2, -4.5), vec3(5.0, 0.0, 9.0), (45, 46)),
                    cube(vec3(-3.0, -4.5, -1.5), vec3(3.0, 3.0, 3.0), (12, 34)),
                ],
                0,
            ),
        ],
        64,
        64,
    )
}

/// Creaking eye pass retains the vanilla head cuboid as its own geometry.
pub fn bake_creaking_eyes_model() -> BakedEntityModel {
    bake_model(
        vec![
            part("root", vec3(0.0, 24.0, 0.0), vec![]),
            child("upper_body", vec3(-1.0, -19.0, 0.0), vec![], 0),
            child(
                "head",
                vec3(-3.0, -11.0, 0.0),
                vec![
                    cube(vec3(-3.0, -10.0, -3.0), vec3(6.0, 10.0, 6.0), (0, 0)),
                    cube(vec3(-3.0, -13.0, -3.0), vec3(6.0, 3.0, 6.0), (28, 31)),
                    cube(vec3(3.0, -13.0, 0.0), vec3(9.0, 14.0, 0.0), (12, 40)),
                    cube(vec3(-12.0, -14.0, 0.0), vec3(9.0, 14.0, 0.0), (34, 12)),
                ],
                1,
            ),
        ],
        64,
        64,
    )
}

/// ParrotModel's blue/green/grey/red-blue/yellow-blue variants share this exact
/// 32x32 topology; animated posing of wings/head/tail is deliberately static.
pub fn bake_parrot_model() -> BakedEntityModel {
    bake_model(
        vec![
            EntityPart {
                name: "body".into(),
                offset: vec3(0.0, 16.5, -3.0),
                default_rotation: vec3(0.4937, 0.0, 0.0),
                cubes: vec![cube(vec3(-1.5, 0.0, -1.5), vec3(3.0, 6.0, 3.0), (2, 8))],
                parent: None,
            },
            EntityPart {
                name: "tail".into(),
                offset: vec3(0.0, 21.07, 1.16),
                default_rotation: vec3(1.015, 0.0, 0.0),
                cubes: vec![cube(vec3(-1.5, -1.0, -1.0), vec3(3.0, 4.0, 1.0), (22, 1))],
                parent: None,
            },
            EntityPart {
                name: "left_wing".into(),
                offset: vec3(1.5, 16.94, -2.76),
                default_rotation: vec3(-0.6981, -std::f32::consts::PI, 0.0),
                cubes: vec![cube(vec3(-0.5, 0.0, -1.5), vec3(1.0, 5.0, 3.0), (19, 8))],
                parent: None,
            },
            EntityPart {
                name: "right_wing".into(),
                offset: vec3(-1.5, 16.94, -2.76),
                default_rotation: vec3(-0.6981, -std::f32::consts::PI, 0.0),
                cubes: vec![cube(vec3(-0.5, 0.0, -1.5), vec3(1.0, 5.0, 3.0), (19, 8))],
                parent: None,
            },
            EntityPart {
                name: "head".into(),
                offset: vec3(0.0, 15.69, -2.76),
                default_rotation: Vec3::ZERO,
                cubes: vec![cube(vec3(-1.0, -1.5, -1.0), vec3(2.0, 3.0, 2.0), (2, 2))],
                parent: None,
            },
            child(
                "head2",
                vec3(0.0, -2.0, -1.0),
                vec![cube(vec3(-1.0, -0.5, -2.0), vec3(2.0, 1.0, 4.0), (10, 0))],
                4,
            ),
            child(
                "beak1",
                vec3(0.0, -0.5, -1.5),
                vec![cube(vec3(-0.5, -1.0, -0.5), vec3(1.0, 2.0, 1.0), (11, 7))],
                5,
            ),
            child(
                "beak2",
                vec3(0.0, -1.75, -2.45),
                vec![cube(vec3(-0.5, 0.0, -0.5), vec3(1.0, 2.0, 1.0), (16, 7))],
                5,
            ),
            EntityPart {
                name: "feather".into(),
                offset: vec3(0.0, -2.15, 0.15),
                default_rotation: vec3(-0.2214, 0.0, 0.0),
                cubes: vec![cube(vec3(0.0, -4.0, -2.0), vec3(0.0, 5.0, 4.0), (2, 18))],
                parent: Some(4),
            },
            EntityPart {
                name: "left_leg".into(),
                offset: vec3(1.0, 22.0, -1.05),
                default_rotation: vec3(0.0, -1.05, -0.0299),
                cubes: vec![cube(vec3(-0.5, 0.0, -0.5), vec3(1.0, 2.0, 1.0), (14, 18))],
                parent: None,
            },
            EntityPart {
                name: "right_leg".into(),
                offset: vec3(-1.0, 22.0, -1.05),
                default_rotation: vec3(0.0, 1.05, -0.0299),
                cubes: vec![cube(vec3(-0.5, 0.0, -0.5), vec3(1.0, 2.0, 1.0), (14, 18))],
                parent: None,
            },
        ],
        32,
        32,
    )
}

/// WardenModel.createBodyLayer: 128x128 rib cage, skull horns, broad arms and
/// legs, with two modeled tendril bars. Overlay passes are texture variants.
pub fn bake_warden_model() -> BakedEntityModel {
    bake_model(
        vec![
            part("bone", vec3(0.0, 24.0, 0.0), vec![]),
            child(
                "body",
                vec3(0.0, -21.0, 0.0),
                vec![cube(
                    vec3(-9.0, -13.0, -4.0),
                    vec3(18.0, 21.0, 11.0),
                    (0, 0),
                )],
                0,
            ),
            child(
                "right_ribcage",
                vec3(-7.0, -2.0, -4.0),
                vec![cube(
                    vec3(-2.0, -11.0, -0.1),
                    vec3(9.0, 21.0, 0.0),
                    (90, 11),
                )],
                1,
            ),
            child(
                "left_ribcage",
                vec3(7.0, -2.0, -4.0),
                vec![cube(
                    vec3(-7.0, -11.0, -0.1),
                    vec3(9.0, 21.0, 0.0),
                    (90, 11),
                )],
                1,
            ),
            child(
                "head",
                vec3(0.0, -13.0, 0.0),
                vec![cube(
                    vec3(-8.0, -16.0, -5.0),
                    vec3(16.0, 16.0, 10.0),
                    (0, 32),
                )],
                1,
            ),
            child(
                "right_tendril",
                vec3(-8.0, -12.0, 0.0),
                vec![cube(
                    vec3(-16.0, -13.0, 0.0),
                    vec3(16.0, 16.0, 0.0),
                    (52, 32),
                )],
                4,
            ),
            child(
                "left_tendril",
                vec3(8.0, -12.0, 0.0),
                vec![cube(vec3(0.0, -13.0, 0.0), vec3(16.0, 16.0, 0.0), (58, 0))],
                4,
            ),
            child(
                "right_arm",
                vec3(-13.0, -13.0, 1.0),
                vec![cube(vec3(-4.0, 0.0, -4.0), vec3(8.0, 28.0, 8.0), (44, 50))],
                1,
            ),
            child(
                "left_arm",
                vec3(13.0, -13.0, 1.0),
                vec![cube(vec3(-4.0, 0.0, -4.0), vec3(8.0, 28.0, 8.0), (0, 58))],
                1,
            ),
            child(
                "right_leg",
                vec3(-5.9, -13.0, 0.0),
                vec![cube(vec3(-3.1, 0.0, -3.0), vec3(6.0, 13.0, 6.0), (76, 48))],
                1,
            ),
            child(
                "left_leg",
                vec3(5.9, -13.0, 0.0),
                vec![cube(vec3(-2.9, 0.0, -3.0), vec3(6.0, 13.0, 6.0), (76, 76))],
                1,
            ),
        ],
        128,
        128,
    )
}

/// NautilusModel shared standard mesh; ZombieNautilus uses this base layer too.
/// The vanilla body layer consists of shell/back plate and articulated mouth;
/// no extra tentacle cubes are declared by this model's createBodyMesh.
pub fn bake_nautilus_model() -> BakedEntityModel {
    bake_model(
        vec![
            part("root", vec3(0.0, 29.0, -6.0), vec![]),
            child(
                "shell",
                vec3(0.0, -13.0, 5.0),
                vec![
                    cube(vec3(-7.0, -10.0, -7.0), vec3(14.0, 10.0, 16.0), (0, 0)),
                    cube(vec3(-7.0, 0.0, -7.0), vec3(14.0, 8.0, 20.0), (0, 26)),
                    cube(vec3(-7.0, 0.0, 6.0), vec3(14.0, 8.0, 0.0), (48, 26)),
                ],
                0,
            ),
            child(
                "body",
                vec3(0.0, -8.5, 12.3),
                vec![
                    cube(vec3(-5.0, -4.51, -3.0), vec3(10.0, 8.0, 14.0), (54, 54)),
                    cube(vec3(-5.0, -4.51, 7.0), vec3(10.0, 8.0, 0.0), (0, 76)),
                ],
                1,
            ),
            child(
                "upper_mouth",
                vec3(0.0, -2.51, 7.0),
                vec![cube_d(
                    vec3(-5.0, -2.0, 0.0),
                    vec3(10.0, 4.0, 4.0),
                    (54, 54),
                    -0.001,
                )],
                2,
            ),
            child(
                "inner_mouth",
                vec3(0.0, -0.51, 7.5),
                vec![cube(vec3(-3.0, -2.0, -0.5), vec3(6.0, 4.0, 4.0), (54, 70))],
                2,
            ),
            child(
                "lower_mouth",
                vec3(0.0, 1.49, 7.0),
                vec![cube_d(
                    vec3(-5.0, -1.98, 0.0),
                    vec3(10.0, 4.0, 4.0),
                    (54, 62),
                    -0.001,
                )],
                2,
            ),
        ],
        128,
        128,
    )
}
pub fn bake_baby_nautilus_model() -> BakedEntityModel {
    bake_model(
        vec![
            part("root", vec3(-0.5, 28.0, -0.5), vec![]),
            child(
                "shell",
                vec3(3.0, -8.0, -2.0),
                vec![
                    cube(vec3(-6.0, -4.0, -1.0), vec3(7.0, 4.0, 7.0), (0, 0)),
                    cube(vec3(-6.0, 0.0, -1.0), vec3(7.0, 4.0, 9.0), (0, 11)),
                    cube(vec3(-6.0, 0.0, 5.0), vec3(7.0, 4.0, 0.0), (23, 11)),
                ],
                0,
            ),
            child(
                "body",
                vec3(0.5, -5.0, 3.0),
                vec![
                    cube(vec3(-2.5, -3.01, -1.0), vec3(5.0, 4.0, 7.0), (24, 0)),
                    cube(vec3(-2.5, -3.01, 4.1), vec3(5.0, 4.0, 0.0), (0, 35)),
                ],
                1,
            ),
            child(
                "upper_mouth",
                vec3(0.0, -2.01, 3.9),
                vec![cube_d(
                    vec3(-2.5, -1.0, 0.0),
                    vec3(5.0, 2.0, 2.0),
                    (24, 24),
                    -0.001,
                )],
                2,
            ),
            child(
                "inner_mouth",
                vec3(0.0, -1.01, 4.9),
                vec![cube(vec3(-1.5, -1.0, -1.0), vec3(3.0, 2.0, 2.0), (24, 32))],
                2,
            ),
            child(
                "lower_mouth",
                vec3(0.0, -0.01, 3.9),
                vec![cube_d(
                    vec3(-2.5, -1.0, 0.0),
                    vec3(5.0, 2.0, 2.0),
                    (24, 28),
                    -0.001,
                )],
                2,
            ),
        ],
        64,
        64,
    )
}
pub fn bake_zombie_nautilus_model() -> BakedEntityModel {
    bake_nautilus_model()
}
/// ZombieNautilusCoralModel's separately posed coral branches. The vanilla
/// NautilusModel beneath it is baked by `bake_zombie_nautilus_model`.
pub fn bake_zombie_nautilus_coral_model() -> BakedEntityModel {
    let mut parts = bake_nautilus_model().parts;
    parts.push(child("corals", vec3(8.0, 4.5, -8.0), vec![], 1));
    parts.push(child("yellow_coral", vec3(0.0, -11.0, 11.0), vec![], 6));
    parts.push(child(
        "yellow_coral_second",
        vec3(0.0, 0.0, 2.0),
        vec![cube(vec3(-4.5, -3.5, 0.0), vec3(6.0, 8.0, 0.0), (0, 85))],
        7,
    ));
    {
        let mut p = child(
            "yellow_coral_first",
            Vec3::ZERO,
            vec![cube(vec3(-4.5, -3.5, 0.0), vec3(6.0, 8.0, 0.0), (0, 85))],
            7,
        );
        p.default_rotation.x = 0.7854;
        parts.push(p);
    }
    parts.push(child(
        "pink_coral",
        vec3(-12.5, -18.0, 11.0),
        vec![cube(vec3(-4.5, 4.5, 0.0), vec3(6.0, 0.0, 8.0), (-8, 94))],
        6,
    ));
    {
        let mut p = child(
            "pink_coral_second",
            vec3(-1.5, 4.5, 4.0),
            vec![cube(vec3(-3.0, 0.0, -4.0), vec3(6.0, 0.0, 8.0), (-8, 94))],
            10,
        );
        p.default_rotation.z = 1.5708;
        parts.push(p);
    }
    parts.push(child("blue_coral", vec3(-14.0, 0.0, 5.5), vec![], 6));
    {
        let mut p = child(
            "blue_second",
            vec3(0.0, 0.0, -2.0),
            vec![cube(vec3(-3.5, -5.5, 0.0), vec3(5.0, 10.0, 0.0), (0, 102))],
            12,
        );
        p.default_rotation.y = 0.7854;
        parts.push(p);
    }
    {
        let mut p = child(
            "blue_first",
            Vec3::ZERO,
            vec![cube(vec3(-3.5, -5.5, 0.0), vec3(5.0, 10.0, 0.0), (0, 102))],
            12,
        );
        p.default_rotation.y = -0.7854;
        parts.push(p);
    }
    parts.push(child("red_coral", Vec3::ZERO, vec![], 6));
    {
        let mut p = child(
            "red_coral_second",
            vec3(-0.5, -1.0, 1.5),
            vec![cube(vec3(-2.5, -5.5, 0.0), vec3(4.0, 10.0, 0.0), (0, 112))],
            15,
        );
        p.default_rotation.x = -0.829;
        parts.push(p);
    }
    {
        let mut p = child(
            "red_coral_first",
            Vec3::ZERO,
            vec![cube(vec3(-4.5, -5.5, 0.0), vec3(6.0, 10.0, 0.0), (0, 112))],
            15,
        );
        p.default_rotation.y = 0.7854;
        parts.push(p);
    }
    bake_model(parts, 128, 128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::entity_model::PartAnim;

    #[test]
    fn all_static_bakes_have_golden_parts_uv_and_nonempty_vertices() {
        let models = [
            bake_illager_model(),
            bake_piglin_model(),
            bake_copper_golem_model(),
            bake_creaking_model(),
            bake_creaking_eyes_model(),
            bake_endermite_model(),
            bake_silverfish_model(),
            bake_magma_cube_model(),
            bake_sulfur_cube_outer_model(),
            bake_sulfur_cube_inner_model(),
            bake_sulfur_cube_small_outer_model(),
            bake_sulfur_cube_small_inner_model(),
            bake_shulker_model(),
            bake_snow_golem_model(),
            bake_tadpole_model(),
            bake_parched_model(),
            bake_parrot_model(),
            bake_wandering_trader_model(),
            bake_warden_model(),
            bake_wither_skeleton_model(),
            bake_nautilus_model(),
            bake_baby_nautilus_model(),
            bake_zombie_nautilus_model(),
            bake_zombie_nautilus_coral_model(),
        ];
        for model in &models {
            assert!(!model.vertices.is_empty());
            assert_eq!(model.part_ranges.len(), model.parts.len());
            for (index, part) in model.parts.iter().enumerate() {
                assert!(part.parent.is_none_or(|parent| parent < index));
            }
        }

        let illager = bake_illager_model();
        assert_eq!(illager.parts.len(), 6);
        assert_eq!(illager.parts[1].cubes[0].tex_offset, (24, 0));
        assert_eq!(illager.parts[3].cubes.len(), 3);
        assert_eq!(illager.parts[4].cubes[0].size.y, 12.0);
        let piglin = bake_piglin_model();
        assert_eq!(piglin.parts.len(), 8);
        assert_eq!(piglin.parts[1].cubes[0].tex_offset, (39, 6));
        assert_eq!(piglin.parts[5].cubes[0].tex_offset, (32, 48));

        assert_eq!(bake_endermite_model().parts.len(), 4);
        assert_eq!(bake_silverfish_model().parts.len(), 10);
        let magma = bake_magma_cube_model();
        assert_eq!(magma.parts.len(), 9);
        assert_eq!(magma.parts[8].cubes[0].tex_offset, (24, 40));
        assert_eq!(bake_shulker_model().parts[0].cubes[0].tex_offset, (0, 0));
        assert_eq!(bake_warden_model().parts[5].cubes[0].tex_offset, (52, 32));
        assert_eq!(bake_nautilus_model().parts[1].cubes[1].tex_offset, (0, 26));
        assert_eq!(bake_parrot_model().parts[6].cubes[0].tex_offset, (11, 7));
        assert_eq!(
            bake_sulfur_cube_inner_model().parts[0].cubes[0].tex_offset,
            (0, 36)
        );
        assert_eq!(
            bake_sulfur_cube_small_outer_model().parts[0].cubes[0]
                .size
                .x,
            10.0
        );
        assert_eq!(bake_parched_model().parts[0].cubes.len(), 3);
        let skeleton = crate::renderer::entity_model::bake_skeleton_model();
        let wither = bake_wither_skeleton_model();
        assert_eq!(
            wither
                .parts
                .iter()
                .map(|part| part.name.as_str())
                .collect::<Vec<_>>(),
            skeleton
                .parts
                .iter()
                .map(|part| part.name.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(wither.parts.len(), 6);
        assert_eq!(wither.parts[2].name, "right_arm");
        assert_eq!(wither.parts[5].name, "left_leg");
        assert_eq!(wither.vertices.len(), skeleton.vertices.len());
        assert_eq!(
            wither
                .vertices
                .iter()
                .map(|v| v.tex_coords)
                .collect::<Vec<_>>(),
            skeleton
                .vertices
                .iter()
                .map(|v| v.tex_coords)
                .collect::<Vec<_>>(),
            "1.2 geometry scaling must preserve normalized 64x32 UVs"
        );
        let y_offset = 24.016 * (1.0 - 1.2);
        for (base, scaled) in skeleton.parts.iter().zip(&wither.parts) {
            assert_eq!(scaled.cubes[0].size, base.cubes[0].size);
            assert_eq!(scaled.cubes[0].tex_offset, base.cubes[0].tex_offset);
            assert_eq!(
                scaled.offset,
                base.offset * 1.2 + Vec3::new(0.0, y_offset, 0.0)
            );
        }
        assert!(wither.part_scales.iter().all(|&scale| scale == 1.2));
        let base_transforms = skeleton.compute_part_transforms(&PartAnim::default());
        let scaled_transforms = wither.compute_part_transforms(&PartAnim::default());
        for (part_index, &(start, count)) in skeleton.part_ranges.iter().enumerate() {
            let end = (start + count) as usize;
            for (base_vertex, scaled_vertex) in skeleton.vertices[start as usize..end]
                .iter()
                .zip(&wither.vertices[start as usize..end])
            {
                let base_point = base_transforms[part_index]
                    .transform_point3(Vec3::from_array(base_vertex.position));
                let scaled_point = scaled_transforms[part_index]
                    .transform_point3(Vec3::from_array(scaled_vertex.position));
                assert!((scaled_point - base_point * 1.2).abs().max_element() < 1e-5);
            }
        }
        // Vanilla's leg pivot at y=12 and the scaled 12px leg retain the
        // transformed 24.016px ground anchor (rather than adding a new root).
        let right_leg = &wither.parts[4];
        assert_eq!(right_leg.offset.y, 12.0 * 1.2 + y_offset);
        assert_eq!(
            right_leg.cubes[0].size.y * wither.part_scales[4],
            12.0 * 1.2
        );
    }
}
