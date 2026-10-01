//! Vanilla 26.2 flying and insect meshes transcribed from client model-layer
//! bytecode. Animation/state updates remain the renderer's responsibility.
use glam::Vec3;

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

fn cube(uv: (i32, i32), pos: (f32, f32, f32), size: (f32, f32, f32)) -> ModelCube {
    cube_d(uv, pos, size, 0.0, false)
}

fn cube_d(
    uv: (i32, i32),
    pos: (f32, f32, f32),
    size: (f32, f32, f32),
    deformation: f32,
    mirror: bool,
) -> ModelCube {
    ModelCube {
        origin: Vec3::new(pos.0, pos.1, pos.2),
        size: Vec3::new(size.0, size.1, size.2),
        tex_offset: uv,
        deformation,
        mirror,
    }
}

fn part(
    name: &str,
    parent: Option<usize>,
    pivot: (f32, f32, f32),
    cubes: Vec<ModelCube>,
) -> EntityPart {
    part_rot(name, parent, pivot, (0.0, 0.0, 0.0), cubes)
}

fn part_rot(
    name: &str,
    parent: Option<usize>,
    pivot: (f32, f32, f32),
    rotation: (f32, f32, f32),
    cubes: Vec<ModelCube>,
) -> EntityPart {
    EntityPart {
        name: name.into(),
        offset: Vec3::new(pivot.0, pivot.1, pivot.2),
        default_rotation: Vec3::new(rotation.0, rotation.1, rotation.2),
        cubes,
        parent,
    }
}

fn bake(parts: Vec<EntityPart>, w: u32, h: u32) -> BakedEntityModel {
    bake_model(parts, w, h)
}

// `Mth.sin/cos(double)` samples its 65,536-entry table at this radians index.
fn vanilla_sin(angle: f64) -> f32 {
    let index = (angle * 10430.378350470453).trunc() as i64 & 65535;
    ((index as f64 / 10430.378350470453).sin()) as f32
}

fn vanilla_cos(angle: f64) -> f32 {
    let index = (angle * 10430.378350470453 + 16384.0).trunc() as i64 & 65535;
    ((index as f64 / 10430.378350470453).sin()) as f32
}

// MeshTransformer.scaling scales model coordinates, not UVs. Keep cube sizes
// unchanged during UV baking, then scale the complete hierarchy at its root.
fn bake_scaled(mut parts: Vec<EntityPart>, scale: f32, w: u32, h: u32) -> BakedEntityModel {
    const MODEL_REBASE_Y: f32 = 24.016;
    for p in &mut parts {
        p.parent = p.parent.map(|i| i + 1).or(Some(0));
    }
    let root_y = MODEL_REBASE_Y * (1.0 - scale);
    parts.insert(0, part("mesh_scale", None, (0.0, root_y, 0.0), vec![]));
    let mut model = bake(parts, w, h);
    model.part_scales[0] = scale;
    model
}

/// `AllayModel.createBodyLayer`, 32x32, `AllayRenderer` shadow radius 0.4.
pub fn bake_allay_model() -> BakedEntityModel {
    let mut p = vec![part("root", None, (0.0, 23.5, 0.0), vec![])];
    p.push(part(
        "head",
        Some(0),
        (0.0, -3.99, 0.0),
        vec![cube((0, 0), (-2.5, -5.0, -2.5), (5.0, 5.0, 5.0))],
    ));
    p.push(part(
        "body",
        Some(0),
        (0.0, -4.0, 0.0),
        vec![
            cube((0, 10), (-1.5, 0.0, -1.0), (3.0, 4.0, 2.0)),
            cube_d((0, 16), (-1.5, 0.0, -1.0), (3.0, 5.0, 2.0), -0.2, false),
        ],
    ));
    p.push(part(
        "right_arm",
        Some(2),
        (-1.75, 0.5, 0.0),
        vec![cube_d(
            (23, 0),
            (-0.75, -0.5, -1.0),
            (1.0, 4.0, 2.0),
            -0.01,
            false,
        )],
    ));
    p.push(part(
        "left_arm",
        Some(2),
        (1.75, 0.5, 0.0),
        vec![cube_d(
            (23, 6),
            (-0.25, -0.5, -1.0),
            (1.0, 4.0, 2.0),
            -0.01,
            false,
        )],
    ));
    p.push(part(
        "right_wing",
        Some(2),
        (-0.5, 0.0, 0.6),
        vec![cube((16, 14), (0.0, 1.0, 0.0), (0.0, 5.0, 8.0))],
    ));
    p.push(part(
        "left_wing",
        Some(2),
        (0.5, 0.0, 0.6),
        vec![cube((16, 14), (0.0, 1.0, 0.0), (0.0, 5.0, 8.0))],
    ));
    bake(p, 32, 32)
}

/// `AdultBeeModel.createBodyLayer`, 64x64: source bone/body, antennas, wings,
/// stinger, and three separately UV'd leg planes.
pub fn bake_bee_model() -> BakedEntityModel {
    let p = vec![
        part("bone", None, (0.0, 19.0, 0.0), vec![]),
        part(
            "body",
            Some(0),
            (0.0, 0.0, 0.0),
            vec![cube((0, 0), (-3.5, -4.0, -5.0), (7.0, 7.0, 10.0))],
        ),
        part(
            "stinger",
            Some(1),
            (0.0, 0.0, 0.0),
            vec![cube((26, 7), (0.0, -1.0, 5.0), (0.0, 1.0, 2.0))],
        ),
        part(
            "left_antenna",
            Some(1),
            (0.0, -2.0, -5.0),
            vec![cube((2, 0), (1.5, -2.0, -3.0), (1.0, 2.0, 3.0))],
        ),
        part(
            "right_antenna",
            Some(1),
            (0.0, -2.0, -5.0),
            vec![cube((2, 3), (-2.5, -2.0, -3.0), (1.0, 2.0, 3.0))],
        ),
        part_rot(
            "right_wing",
            Some(0),
            (-1.5, -4.0, -3.0),
            (0.0, -0.2618, 0.0),
            vec![cube_d(
                (0, 18),
                (-9.0, 0.0, 0.0),
                (9.0, 0.0, 6.0),
                0.001,
                false,
            )],
        ),
        part_rot(
            "left_wing",
            Some(0),
            (1.5, -4.0, -3.0),
            (0.0, 0.2618, 0.0),
            vec![cube_d(
                (0, 18),
                (0.0, 0.0, 0.0),
                (9.0, 0.0, 6.0),
                0.001,
                true,
            )],
        ),
        part(
            "front_legs",
            Some(0),
            (1.5, 3.0, -2.0),
            vec![cube((26, 1), (-5.0, 0.0, 0.0), (7.0, 2.0, 0.0))],
        ),
        part(
            "middle_legs",
            Some(0),
            (1.5, 3.0, 0.0),
            vec![cube((26, 3), (-5.0, 0.0, 0.0), (7.0, 2.0, 0.0))],
        ),
        part(
            "back_legs",
            Some(0),
            (1.5, 3.0, 2.0),
            vec![cube((26, 5), (-5.0, 0.0, 0.0), (7.0, 2.0, 0.0))],
        ),
    ];
    bake(p, 64, 64)
}

/// `BabyBeeModel.createBodyLayer`, dedicated 32x32 baby mesh (not the adult
/// mesh at a generic scale).
pub fn bake_baby_bee_model() -> BakedEntityModel {
    let p = vec![
        part(
            "bone",
            None,
            (0.0, 19.6667, -1.8567),
            vec![
                cube((6, 12), (1.0, -1.6667, -2.1633), (1.0, 2.0, 2.0)),
                cube((0, 12), (-2.0, -1.6667, -2.1933), (1.0, 2.0, 2.0)),
            ],
        ),
        part(
            "body",
            Some(0),
            (0.0, 1.3333, 2.3567),
            vec![cube((0, 0), (-2.0, -2.0, -2.5), (4.0, 4.0, 5.0))],
        ),
        part(
            "stinger",
            Some(1),
            (0.0, 0.5, 2.5),
            vec![cube((13, 2), (0.0, -0.5, 0.0), (0.0, 1.0, 1.0))],
        ),
        part_rot(
            "right_wing",
            Some(0),
            (-1.0, -0.6667, 0.8567),
            (0.2182, 0.3491, 0.0),
            vec![cube((3, 9), (-3.0, 0.0, 0.0), (3.0, 0.0, 3.0))],
        ),
        part_rot(
            "left_wing",
            Some(0),
            (1.0, -0.6667, 0.8567),
            (0.2182, -0.3491, 0.0),
            vec![cube((-3, 9), (0.0, 0.0, 0.0), (3.0, 0.0, 3.0))],
        ),
        part(
            "front_legs",
            Some(0),
            (1.5, 3.3333, 1.8567),
            vec![cube((13, 0), (-1.5, 0.0, 0.0), (3.0, 1.0, 0.0))],
        ),
        part(
            "middle_legs",
            Some(0),
            (1.5, 3.3333, 2.8567),
            vec![cube((13, 1), (-1.5, 0.0, 0.0), (3.0, 1.0, 0.0))],
        ),
        part(
            "back_legs",
            Some(0),
            (1.5, 3.3333, 3.8567),
            vec![cube((13, 2), (-1.5, 0.0, 0.0), (3.0, 1.0, 0.0))],
        ),
    ];
    bake(p, 32, 32)
}

/// `BlazeModel.createBodyLayer`, 64x32. Four upper, four middle and four
/// lower rods are placed by the exact source trigonometric layout.
pub fn bake_blaze_model() -> BakedEntityModel {
    let mut p = vec![part(
        "head",
        None,
        (0.0, 0.0, 0.0),
        vec![cube((0, 0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))],
    )];
    for i in 0..12 {
        let (angle, radius, y) = if i < 4 {
            let a = (i as f32) * std::f32::consts::FRAC_PI_2;
            (a, 9.0, -2.0 + vanilla_cos((i as f32 * 0.5) as f64))
        } else if i < 8 {
            let a = std::f32::consts::FRAC_PI_4 + ((i - 4) as f32) * std::f32::consts::FRAC_PI_2;
            (a, 7.0, 2.0 + vanilla_cos((i as f32 * 0.5) as f64))
        } else {
            let a = 0.47123894 + ((i - 8) as f32) * std::f32::consts::FRAC_PI_2;
            (a, 5.0, 11.0 + vanilla_cos((i as f32 * 0.75) as f64))
        };
        p.push(part(
            &format!("part{i}"),
            None,
            (
                radius * vanilla_cos(angle as f64),
                y,
                radius * vanilla_sin(angle as f64),
            ),
            vec![cube((0, 16), (0.0, 0.0, 0.0), (2.0, 8.0, 2.0))],
        ));
    }
    bake(p, 64, 32)
}

/// `BreezeModel.createBodyLayer`, base skin geometry (32x32). The wind and
/// eyes meshes are separate vanilla layers and have their own bake functions.
pub fn bake_breeze_model() -> BakedEntityModel {
    let p = vec![
        part("body", None, (0.0, 0.0, 0.0), vec![]),
        part("rods", Some(0), (0.0, 8.0, 0.0), vec![]),
        part_rot(
            "rod_1",
            Some(1),
            (2.5981, -3.0, 1.5),
            (-2.7489, -1.0472, 3.1416),
            vec![cube((0, 17), (-1.0, 0.0, -3.0), (2.0, 8.0, 2.0))],
        ),
        part_rot(
            "rod_2",
            Some(1),
            (-2.5981, -3.0, 1.5),
            (-2.7489, 1.0472, 3.1416),
            vec![cube((0, 17), (-1.0, 0.0, -3.0), (2.0, 8.0, 2.0))],
        ),
        part_rot(
            "rod_3",
            Some(1),
            (0.0, -3.0, -3.0),
            (0.3927, 0.0, 0.0),
            vec![cube((0, 17), (-1.0, 0.0, -3.0), (2.0, 8.0, 2.0))],
        ),
        part(
            "head",
            Some(0),
            (0.0, 4.0, 0.0),
            vec![
                cube((4, 24), (-5.0, -5.0, -4.2), (10.0, 3.0, 4.0)),
                cube((0, 0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)),
            ],
        ),
        part(
            "eyes",
            Some(5),
            (0.0, 0.0, 0.0),
            vec![
                cube((4, 24), (-5.0, -5.0, -4.2), (10.0, 3.0, 4.0)),
                cube((0, 0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)),
            ],
        ),
    ];
    bake(p, 32, 32)
}

/// Separate `BreezeModel.createEyesLayer`; texture is `breeze_eyes.png`.
pub fn bake_breeze_eyes_model() -> BakedEntityModel {
    bake(
        vec![
            part("body", None, (0.0, 0.0, 0.0), vec![]),
            part("rods", Some(0), (0.0, 8.0, 0.0), vec![]),
            part_rot(
                "rod_1",
                Some(1),
                (2.5981, -3.0, 1.5),
                (-2.7489, -1.0472, 3.1416),
                vec![],
            ),
            part_rot(
                "rod_2",
                Some(1),
                (-2.5981, -3.0, 1.5),
                (-2.7489, 1.0472, 3.1416),
                vec![],
            ),
            part_rot(
                "rod_3",
                Some(1),
                (0.0, -3.0, -3.0),
                (0.3927, 0.0, 0.0),
                vec![],
            ),
            part("head", Some(0), (0.0, 4.0, 0.0), vec![]),
            part(
                "eyes",
                Some(5),
                (0.0, 0.0, 0.0),
                vec![
                    cube((4, 24), (-5.0, -5.0, -4.2), (10.0, 3.0, 4.0)),
                    cube((0, 0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)),
                ],
            ),
        ],
        32,
        32,
    )
}

/// Separate `BreezeModel.createWindLayer`, 128x128 `breeze_wind.png` geometry.
pub fn bake_breeze_wind_model() -> BakedEntityModel {
    bake(
        vec![
            part("wind_body", None, (0.0, 0.0, 0.0), vec![]),
            part(
                "wind_bottom",
                Some(0),
                (0.0, 24.0, 0.0),
                vec![cube((1, 83), (-2.5, -7.0, -2.5), (5.0, 7.0, 5.0))],
            ),
            part(
                "wind_mid",
                Some(1),
                (0.0, -7.0, 0.0),
                vec![
                    cube((74, 28), (-6.0, -6.0, -6.0), (12.0, 6.0, 12.0)),
                    cube((78, 32), (-4.0, -6.0, -4.0), (8.0, 6.0, 8.0)),
                    cube((49, 71), (-2.5, -6.0, -2.5), (5.0, 6.0, 5.0)),
                ],
            ),
            part(
                "wind_top",
                Some(2),
                (0.0, -6.0, 0.0),
                vec![
                    cube((0, 0), (-9.0, -8.0, -9.0), (18.0, 8.0, 18.0)),
                    cube((6, 6), (-6.0, -8.0, -6.0), (12.0, 8.0, 12.0)),
                    cube((105, 57), (-2.5, -8.0, -2.5), (5.0, 8.0, 5.0)),
                ],
            ),
        ],
        128,
        128,
    )
}

/// `GhastModel.createBodyLayer`: vanilla 4.5x MeshTransformer and seed-1660
/// nine tentacle placements. Exact seeded lengths are stored in source order.
pub fn bake_ghast_model() -> BakedEntityModel {
    let lengths = [8.0, 13.0, 9.0, 11.0, 11.0, 10.0, 12.0, 9.0, 12.0];
    let mut p = vec![part(
        "body",
        None,
        (0.0, 17.6, 0.0),
        vec![cube((0, 0), (-8.0, -8.0, -8.0), (16.0, 16.0, 16.0))],
    )];
    for i in 0..9 {
        let x = (((i % 3) as f32 - (i / 3 % 2) as f32 * 0.5 + 0.25) / 2.0) * 2.0 - 1.0;
        let z = (i as f32 / 3.0 / 2.0 * 2.0 - 1.0) * 5.0;
        p.push(part(
            &format!("tentacle{i}"),
            None,
            (x * 5.0, 24.6, z),
            vec![cube((0, 0), (-1.0, 0.0, -1.0), (2.0, lengths[i], 2.0))],
        ));
    }
    bake_scaled(p, 4.5, 64, 32)
}

fn happy_ghast_parts(baby: bool) -> Vec<EntityPart> {
    let mut p = vec![part(
        "body",
        None,
        (0.0, 16.0, 0.0),
        vec![cube((0, 0), (-8.0, -8.0, -8.0), (16.0, 16.0, 16.0))],
    )];
    if baby {
        p.push(part(
            "inner_body",
            Some(0),
            (0.0, 8.0, 0.0),
            vec![cube_d(
                (0, 32),
                (-8.0, -16.0, -8.0),
                (16.0, 16.0, 16.0),
                -0.5,
                false,
            )],
        ));
    }
    let data = [
        (-3.75, 5.0, -5.0),
        (1.25, 7.0, -5.0),
        (6.25, 4.0, -5.0),
        (-6.25, 5.0, 0.0),
        (-1.25, 5.0, 0.0),
        (3.75, 7.0, 0.0),
        (-3.75, 8.0, 5.0),
        (1.25, 8.0, 5.0),
        (6.25, 5.0, 5.0),
    ];
    for (i, (x, len, z)) in data.into_iter().enumerate() {
        p.push(part(
            &format!("tentacle{i}"),
            Some(0),
            (x, 7.0, z),
            vec![cube((0, 0), (-1.0, 0.0, -1.0), (2.0, len, 2.0))],
        ));
    }
    p
}

/// `HappyGhastModel.createBodyLayer(false, deformation)`, vanilla 4x mesh
/// scale.
pub fn bake_happy_ghast_model() -> BakedEntityModel {
    bake_scaled(happy_ghast_parts(false), 4.0, 64, 64)
}

/// Baby Happy Ghast has a separate body-inner cube layer and texture.
pub fn bake_baby_happy_ghast_model() -> BakedEntityModel {
    bake_scaled(happy_ghast_parts(true), 0.95, 64, 64)
}

/// `HappyGhastHarnessModel.createHarnessLayer`, base/equipment geometry.
/// Set `baby` to use the dedicated 0.95x harness layer; riding goggles are
/// repositioned by the renderer animation after baking.
pub fn bake_happy_ghast_harness_model(baby: bool) -> BakedEntityModel {
    let parts = vec![
        part(
            "harness",
            None,
            (0.0, 24.0, 0.0),
            vec![cube((0, 0), (-8.0, -16.0, -8.0), (16.0, 16.0, 16.0))],
        ),
        part(
            "goggles",
            None,
            (0.0, 14.0, -5.5),
            vec![cube_d(
                (0, 32),
                (-8.0, -2.5, -2.5),
                (16.0, 5.0, 5.0),
                0.15,
                false,
            )],
        ),
    ];
    bake_scaled(parts, if baby { 0.95 } else { 4.0 }, 64, 64)
}

/// `PhantomModel.createBodyLayer`, exact 64x64 part hierarchy/poses.
pub fn bake_phantom_model() -> BakedEntityModel {
    bake(
        vec![
            part_rot(
                "body",
                None,
                (0.0, 0.0, 0.0),
                (-0.1, 0.0, 0.0),
                vec![cube((0, 8), (-3.0, -2.0, -8.0), (5.0, 3.0, 9.0))],
            ),
            part(
                "tail_base",
                Some(0),
                (0.0, -2.0, 1.0),
                vec![cube((3, 20), (-2.0, 0.0, 0.0), (3.0, 2.0, 6.0))],
            ),
            part(
                "tail_tip",
                Some(1),
                (0.0, 0.5, 6.0),
                vec![cube((4, 29), (-1.0, 0.0, 0.0), (1.0, 1.0, 6.0))],
            ),
            part_rot(
                "left_wing_base",
                Some(0),
                (2.0, -2.0, -8.0),
                (0.0, 0.0, 0.1),
                vec![cube((23, 12), (0.0, 0.0, 0.0), (6.0, 2.0, 9.0))],
            ),
            part_rot(
                "left_wing_tip",
                Some(3),
                (6.0, 0.0, 0.0),
                (0.0, 0.0, 0.1),
                vec![cube((16, 24), (0.0, 0.0, 0.0), (13.0, 1.0, 9.0))],
            ),
            part_rot(
                "right_wing_base",
                Some(0),
                (-3.0, -2.0, -8.0),
                (0.0, 0.0, -0.1),
                vec![cube((23, 12), (-6.0, 0.0, 0.0), (6.0, 2.0, 9.0))],
            ),
            part_rot(
                "right_wing_tip",
                Some(5),
                (-6.0, 0.0, 0.0),
                (0.0, 0.0, -0.1),
                vec![cube((16, 24), (-13.0, 0.0, 0.0), (13.0, 1.0, 9.0))],
            ),
            part_rot(
                "head",
                None,
                (0.0, 1.0, -7.0),
                (0.2, 0.0, 0.0),
                vec![cube((0, 0), (-4.0, -2.0, -5.0), (7.0, 3.0, 5.0))],
            ),
        ],
        64,
        64,
    )
}

/// `VexModel.createBodyLayer`, exact 32x32 geometry. Charging-arm pose and
/// sword are renderer state/layers, not baked into its rest pose.
pub fn bake_vex_model() -> BakedEntityModel {
    bake(
        vec![
            part("root", None, (0.0, -2.5, 0.0), vec![]),
            part(
                "head",
                Some(0),
                (0.0, 20.0, 0.0),
                vec![cube((0, 0), (-2.5, -5.0, -2.5), (5.0, 5.0, 5.0))],
            ),
            part(
                "body",
                Some(0),
                (0.0, 20.0, 0.0),
                vec![
                    cube((0, 10), (-1.5, 0.0, -1.0), (3.0, 4.0, 2.0)),
                    cube_d((0, 16), (-1.5, 1.0, -1.0), (3.0, 5.0, 2.0), -0.2, false),
                ],
            ),
            part(
                "right_arm",
                Some(2),
                (-1.75, 0.25, 0.0),
                vec![cube_d(
                    (23, 0),
                    (-1.25, -0.5, -1.0),
                    (2.0, 4.0, 2.0),
                    -0.1,
                    false,
                )],
            ),
            part(
                "left_arm",
                Some(2),
                (1.75, 0.25, 0.0),
                vec![cube_d(
                    (23, 6),
                    (-0.75, -0.5, -1.0),
                    (2.0, 4.0, 2.0),
                    -0.1,
                    false,
                )],
            ),
            part(
                "left_wing",
                Some(2),
                (0.5, 1.0, 1.0),
                vec![cube_d(
                    (16, 14),
                    (0.0, 0.0, 0.0),
                    (0.0, 5.0, 8.0),
                    0.0,
                    true,
                )],
            ),
            part(
                "right_wing",
                Some(2),
                (-0.5, 1.0, 1.0),
                vec![cube((16, 14), (0.0, 0.0, 0.0), (0.0, 5.0, 8.0))],
            ),
        ],
        32,
        32,
    )
}

/// `WitherBossModel.createBodyLayer(CubeDeformation.ZERO)`, exact 64x64 mesh.
pub fn bake_wither_model() -> BakedEntityModel {
    let angle = 0.2042035162448883_f64;
    let p = vec![
        part(
            "shoulders",
            None,
            (0.0, 0.0, 0.0),
            vec![cube((0, 16), (-10.0, 3.9, -0.5), (20.0, 3.0, 3.0))],
        ),
        part_rot(
            "ribcage",
            None,
            (-2.0, 6.9, -0.5),
            (0.20420352, 0.0, 0.0),
            vec![
                cube((0, 22), (0.0, 0.0, 0.0), (3.0, 10.0, 3.0)),
                cube((24, 22), (-4.0, 1.5, 0.5), (11.0, 2.0, 2.0)),
                cube((24, 22), (-4.0, 4.0, 0.5), (11.0, 2.0, 2.0)),
                cube((24, 22), (-4.0, 6.5, 0.5), (11.0, 2.0, 2.0)),
            ],
        ),
        part_rot(
            "tail",
            None,
            (
                -2.0,
                6.9 + vanilla_cos(angle) * 10.0,
                -0.5 + vanilla_sin(angle) * 10.0,
            ),
            (0.83252203, 0.0, 0.0),
            vec![cube((12, 22), (0.0, 0.0, 0.0), (3.0, 6.0, 3.0))],
        ),
        part(
            "center_head",
            None,
            (0.0, 0.0, 0.0),
            vec![cube((0, 0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))],
        ),
        part(
            "right_head",
            None,
            (-8.0, 4.0, 0.0),
            vec![cube((32, 0), (-4.0, -4.0, -4.0), (6.0, 6.0, 6.0))],
        ),
        part(
            "left_head",
            None,
            (10.0, 4.0, 0.0),
            vec![cube((32, 0), (-4.0, -4.0, -4.0), (6.0, 6.0, 6.0))],
        ),
    ];
    bake(p, 64, 64)
}

/// `EnderDragonModel.createBodyLayer`, source-correct head/neck/tail/body,
/// wing, and articulated leg cuboids (256x256). Dynamic flight poses omitted.
pub fn bake_ender_dragon_model() -> BakedEntityModel {
    let mut p = vec![part("root", None, (0.0, 0.0, 0.0), vec![])];
    p.push(part(
        "head",
        Some(0),
        (0.0, 20.0, -62.0),
        vec![
            cube((176, 44), (-6.0, -1.0, -24.0), (12.0, 5.0, 16.0)),
            cube_d(
                (112, 30),
                (-8.0, -8.0, -10.0),
                (16.0, 16.0, 16.0),
                0.0,
                true,
            ),
            cube_d((0, 0), (-5.0, -12.0, -4.0), (2.0, 4.0, 6.0), 0.0, true),
            cube_d((112, 0), (-5.0, -3.0, -22.0), (2.0, 2.0, 4.0), 0.0, true),
            cube_d((0, 0), (3.0, -12.0, -4.0), (2.0, 4.0, 6.0), 0.0, true),
            cube_d((112, 0), (3.0, -3.0, -22.0), (2.0, 2.0, 4.0), 0.0, true),
        ],
    ));
    p.push(part(
        "jaw",
        Some(1),
        (0.0, 4.0, -8.0),
        vec![cube((176, 65), (-6.0, 0.0, -16.0), (12.0, 4.0, 16.0))],
    ));
    for i in 0..5 {
        p.push(part(
            &format!("neck{i}"),
            Some(0),
            (0.0, 20.0, -12.0 - i as f32 * 10.0),
            vec![
                cube((192, 104), (-5.0, -5.0, -5.0), (10.0, 10.0, 10.0)),
                cube((48, 0), (-1.0, -9.0, -3.0), (2.0, 4.0, 6.0)),
            ],
        ));
    }
    for i in 0..12 {
        p.push(part(
            &format!("tail{i}"),
            Some(0),
            (0.0, 10.0, 60.0 + i as f32 * 10.0),
            vec![
                cube((192, 104), (-5.0, -5.0, -5.0), (10.0, 10.0, 10.0)),
                cube((48, 0), (-1.0, -9.0, -3.0), (2.0, 4.0, 6.0)),
            ],
        ));
    }
    let body = p.len();
    p.push(part(
        "body",
        Some(0),
        (0.0, 3.0, 8.0),
        vec![
            cube((0, 0), (-12.0, 1.0, -16.0), (24.0, 24.0, 64.0)),
            cube((220, 53), (-1.0, -5.0, -10.0), (2.0, 6.0, 12.0)),
            cube((220, 53), (-1.0, -5.0, 10.0), (2.0, 6.0, 12.0)),
            cube((220, 53), (-1.0, -5.0, 30.0), (2.0, 6.0, 12.0)),
        ],
    ));
    for (side, x, mirror) in [("left", 12.0, true), ("right", -12.0, false)] {
        let wing = p.len();
        p.push(part(
            &format!("{side}_wing"),
            Some(body),
            (x, 2.0, -6.0),
            vec![
                cube_d((112, 88), (0.0, -4.0, -4.0), (56.0, 8.0, 8.0), 0.0, mirror),
                cube_d((-56, 88), (0.0, 0.0, 2.0), (56.0, 0.0, 56.0), 0.0, mirror),
            ],
        ));
        p.push(part(
            &format!("{side}_wing_tip"),
            Some(wing),
            (if mirror { -56.0 } else { 56.0 }, 0.0, 0.0),
            vec![
                cube_d((112, 136), (0.0, -2.0, -2.0), (56.0, 4.0, 4.0), 0.0, mirror),
                cube_d((-56, 144), (0.0, 0.0, 2.0), (56.0, 0.0, 56.0), 0.0, mirror),
            ],
        ));
    }
    for (side, x, mirror) in [("left", 12.0, false), ("right", -12.0, false)] {
        let front = p.len();
        p.push(part_rot(
            &format!("{side}_front_leg"),
            Some(body),
            (x, 17.0, -6.0),
            (1.3, 0.0, 0.0),
            vec![cube_d(
                (112, 104),
                (-4.0, -4.0, -4.0),
                (8.0, 24.0, 8.0),
                0.0,
                mirror,
            )],
        ));
        let tip = p.len();
        p.push(part_rot(
            &format!("{side}_front_leg_tip"),
            Some(front),
            (0.0, 20.0, -1.0),
            (-0.5, 0.0, 0.0),
            vec![cube_d(
                (226, 138),
                (-3.0, -1.0, -3.0),
                (6.0, 24.0, 6.0),
                0.0,
                mirror,
            )],
        ));
        p.push(part_rot(
            &format!("{side}_front_foot"),
            Some(tip),
            (0.0, 23.0, 0.0),
            (0.75, 0.0, 0.0),
            vec![cube_d(
                (144, 104),
                (-4.0, 0.0, -12.0),
                (8.0, 4.0, 16.0),
                0.0,
                mirror,
            )],
        ));
        let hind = p.len();
        p.push(part_rot(
            &format!("{side}_hind_leg"),
            Some(body),
            (if mirror { -16.0 } else { 16.0 }, 13.0, 34.0),
            (1.0, 0.0, 0.0),
            vec![cube_d(
                (0, 0),
                (-8.0, -4.0, -8.0),
                (16.0, 32.0, 16.0),
                0.0,
                mirror,
            )],
        ));
        let hind_tip = p.len();
        p.push(part_rot(
            &format!("{side}_hind_leg_tip"),
            Some(hind),
            (0.0, 32.0, -4.0),
            (0.5, 0.0, 0.0),
            vec![cube_d(
                (196, 0),
                (-6.0, -2.0, 0.0),
                (12.0, 32.0, 12.0),
                0.0,
                mirror,
            )],
        ));
        p.push(part_rot(
            &format!("{side}_hind_foot"),
            Some(hind_tip),
            (0.0, 31.0, 4.0),
            (0.75, 0.0, 0.0),
            vec![cube_d(
                (112, 0),
                (-9.0, 0.0, -20.0),
                (18.0, 6.0, 24.0),
                0.0,
                mirror,
            )],
        ));
    }
    bake(p, 256, 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_cube(
        model: &BakedEntityModel,
        part_name: &str,
        uv: (i32, i32),
        origin: Vec3,
        size: Vec3,
    ) {
        let part = model.parts.iter().find(|p| p.name == part_name).unwrap();
        assert!(
            part.cubes
                .iter()
                .any(|c| c.tex_offset == uv && c.origin == origin && c.size == size)
        );
    }

    #[test]
    fn vanilla_layer_golden_geometry() {
        let allay = bake_allay_model();
        assert_cube(
            &allay,
            "head",
            (0, 0),
            Vec3::new(-2.5, -5.0, -2.5),
            Vec3::splat(5.0),
        );
        let bee = bake_bee_model();
        assert_cube(
            &bee,
            "body",
            (0, 0),
            Vec3::new(-3.5, -4.0, -5.0),
            Vec3::new(7.0, 7.0, 10.0),
        );
        let blaze = bake_blaze_model();
        assert_eq!(blaze.parts.len(), 13);
        assert_cube(
            &blaze,
            "part0",
            (0, 16),
            Vec3::ZERO,
            Vec3::new(2.0, 8.0, 2.0),
        );
        let breeze = bake_breeze_model();
        assert_eq!(breeze.parts.len(), 7);
        assert_cube(
            &breeze,
            "rod_1",
            (0, 17),
            Vec3::new(-1.0, 0.0, -3.0),
            Vec3::new(2.0, 8.0, 2.0),
        );
        let breeze_eyes = bake_breeze_eyes_model();
        assert!(breeze_eyes.same_part_poses(&breeze));
        let breeze_wind = bake_breeze_wind_model();
        assert_cube(
            &breeze_wind,
            "wind_top",
            (0, 0),
            Vec3::new(-9.0, -8.0, -9.0),
            Vec3::new(18.0, 8.0, 18.0),
        );
        let ghast = bake_ghast_model();
        assert_eq!(ghast.parts.len(), 11);
        assert_cube(&ghast, "body", (0, 0), Vec3::splat(-8.0), Vec3::splat(16.0));
        assert_cube(
            &ghast,
            "tentacle0",
            (0, 0),
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(2.0, 8.0, 2.0),
        );
        assert_eq!(ghast.parts[2].offset, Vec3::new(-3.75, 24.6, -5.0));
        assert_eq!(ghast.part_scales[0], 4.5);
        assert_eq!(ghast.parts[0].offset.y, 24.016 * (1.0 - 4.5));
        let happy = bake_happy_ghast_model();
        assert_cube(&happy, "body", (0, 0), Vec3::splat(-8.0), Vec3::splat(16.0));
        assert_cube(
            &happy,
            "tentacle0",
            (0, 0),
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(2.0, 5.0, 2.0),
        );
        assert_eq!(happy.part_scales[0], 4.0);
        assert_eq!(happy.parts[0].offset.y, 0.0);
        let happy_baby = bake_baby_happy_ghast_model();
        assert_eq!(happy_baby.part_scales[0], 0.95);
        assert_cube(
            &happy_baby,
            "inner_body",
            (0, 32),
            Vec3::new(-8.0, -16.0, -8.0),
            Vec3::splat(16.0),
        );
        let harness = bake_happy_ghast_harness_model(false);
        assert_eq!(harness.part_scales[0], 4.0);
        assert_cube(
            &harness,
            "goggles",
            (0, 32),
            Vec3::new(-8.0, -2.5, -2.5),
            Vec3::new(16.0, 5.0, 5.0),
        );
        assert_eq!(bake_happy_ghast_harness_model(true).part_scales[0], 0.95);
        let phantom = bake_phantom_model();
        assert_cube(
            &phantom,
            "body",
            (0, 8),
            Vec3::new(-3.0, -2.0, -8.0),
            Vec3::new(5.0, 3.0, 9.0),
        );
        let vex = bake_vex_model();
        assert_cube(
            &vex,
            "right_arm",
            (23, 0),
            Vec3::new(-1.25, -0.5, -1.0),
            Vec3::new(2.0, 4.0, 2.0),
        );
        let wither = bake_wither_model();
        assert_cube(
            &wither,
            "shoulders",
            (0, 16),
            Vec3::new(-10.0, 3.9, -0.5),
            Vec3::new(20.0, 3.0, 3.0),
        );
        let dragon = bake_ender_dragon_model();
        assert_eq!(dragon.parts.len(), 37);
        assert_cube(
            &dragon,
            "jaw",
            (176, 65),
            Vec3::new(-6.0, 0.0, -16.0),
            Vec3::new(12.0, 4.0, 16.0),
        );
        assert_cube(
            &dragon,
            "left_wing",
            (112, 88),
            Vec3::new(0.0, -4.0, -4.0),
            Vec3::new(56.0, 8.0, 8.0),
        );
        assert!(
            dragon
                .parts
                .iter()
                .find(|p| p.name == "left_wing")
                .unwrap()
                .cubes[0]
                .mirror
        );
        assert!(
            !dragon
                .parts
                .iter()
                .find(|p| p.name == "right_wing")
                .unwrap()
                .cubes[0]
                .mirror
        );
        for model in [
            allay,
            bee,
            blaze,
            bake_baby_bee_model(),
            breeze,
            breeze_eyes,
            breeze_wind,
            ghast,
            happy,
            happy_baby,
            harness,
            bake_happy_ghast_harness_model(true),
            phantom,
            vex,
            wither,
            dragon,
        ] {
            assert!(!model.parts.is_empty());
            assert!(!model.vertices.is_empty());
            assert_eq!(model.parts.len(), model.part_ranges.len());
        }
    }
}
