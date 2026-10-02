//! Vanilla 26.2 aquatic model layers, transcribed from mapped jar bytecode.
//! Source names/UV/box/pose evidence is in
//! `docs/report-audit/aquatic-models.md`.
use glam::Vec3;

use crate::renderer::entity_model::{
    BakedEntityModel, EntityPart, ModelCube, bake_independent_roots_scaled, bake_model,
};

fn cube(uv: (i32, i32), origin: (f32, f32, f32), size: (f32, f32, f32)) -> ModelCube {
    ModelCube {
        origin: Vec3::new(origin.0, origin.1, origin.2),
        size: Vec3::new(size.0, size.1, size.2),
        tex_offset: uv,
        deformation: 0.0,
        mirror: false,
    }
}

fn part(
    name: &str,
    parent: Option<usize>,
    offset: (f32, f32, f32),
    rotation: (f32, f32, f32),
    cubes: Vec<ModelCube>,
) -> EntityPart {
    EntityPart {
        name: name.into(),
        offset: Vec3::new(offset.0, offset.1, offset.2),
        default_rotation: Vec3::new(rotation.0, rotation.1, rotation.2),
        cubes,
        parent,
    }
}

fn one(
    name: &str,
    parent: Option<usize>,
    offset: (f32, f32, f32),
    uv: (i32, i32),
    origin: (f32, f32, f32),
    size: (f32, f32, f32),
) -> EntityPart {
    part(
        name,
        parent,
        offset,
        (0.0, 0.0, 0.0),
        vec![cube(uv, origin, size)],
    )
}

/// `AdultAxolotlModel.createBodyLayer` (64x64): head and gills are children of
/// body, legs/tail are body children. Cube deformation on head/gills is .001.
pub fn bake_axolotl_model() -> BakedEntityModel {
    let mut head = one(
        "head",
        Some(0),
        (0.0, 0.0, -9.0),
        (0, 1),
        (-4.0, -3.0, -5.0),
        (8.0, 5.0, 5.0),
    );
    head.cubes[0].deformation = 0.001;
    let mut top_gills = one(
        "top_gills",
        Some(1),
        (0.0, -3.0, -1.0),
        (3, 37),
        (-4.0, -3.0, 0.0),
        (8.0, 3.0, 0.0),
    );
    top_gills.cubes[0].deformation = 0.001;
    let mut left_gills = one(
        "left_gills",
        Some(1),
        (-4.0, 0.0, -1.0),
        (0, 40),
        (-3.0, -5.0, 0.0),
        (3.0, 7.0, 0.0),
    );
    left_gills.cubes[0].deformation = 0.001;
    let mut right_gills = one(
        "right_gills",
        Some(1),
        (4.0, 0.0, -1.0),
        (11, 40),
        (0.0, -5.0, 0.0),
        (3.0, 7.0, 0.0),
    );
    right_gills.cubes[0].deformation = 0.001;
    let mut parts = vec![
        part(
            "body",
            None,
            (0.0, 19.5, 5.0),
            (0.0, 0.0, 0.0),
            vec![
                cube((0, 11), (-4.0, -2.0, -9.0), (8.0, 4.0, 10.0)),
                cube((2, 17), (0.0, -3.0, -8.0), (0.0, 5.0, 9.0)),
            ],
        ),
        head,
        top_gills,
        left_gills,
        right_gills,
        one(
            "right_hind_leg",
            Some(0),
            (-3.5, 1.0, -1.0),
            (2, 13),
            (-2.0, 0.0, 0.0),
            (3.0, 5.0, 0.0),
        ),
        one(
            "left_hind_leg",
            Some(0),
            (3.5, 1.0, -1.0),
            (2, 13),
            (-1.0, 0.0, 0.0),
            (3.0, 5.0, 0.0),
        ),
        one(
            "right_front_leg",
            Some(0),
            (-3.5, 1.0, -8.0),
            (2, 13),
            (-2.0, 0.0, 0.0),
            (3.0, 5.0, 0.0),
        ),
        one(
            "left_front_leg",
            Some(0),
            (3.5, 1.0, -8.0),
            (2, 13),
            (-1.0, 0.0, 0.0),
            (3.0, 5.0, 0.0),
        ),
        one(
            "tail",
            Some(0),
            (0.0, 0.0, 1.0),
            (2, 19),
            (0.0, -3.0, 0.0),
            (0.0, 5.0, 12.0),
        ),
    ];
    for p in &mut parts[1..9] {
        for c in &mut p.cubes {
            c.deformation = 0.001;
        }
    }
    bake_model(parts, 64, 64)
}

/// Dedicated `BabyAxolotlModel.createBodyLayer` (32x32), not a scaled adult.
pub fn bake_baby_axolotl_model() -> BakedEntityModel {
    let mut body = part(
        "body",
        Some(0),
        (0.0, -1.25, 1.75),
        (0.0, 0.0, 0.0),
        vec![
            cube((0, 0), (-2.0, -0.75, -2.75), (4.0, 2.0, 6.0)),
            cube((0, 12), (0.0, -1.75, -2.75), (0.0, 3.0, 5.0)),
        ],
    );
    let mut right_hind = part(
        "right_hind_leg",
        Some(1),
        (-2.0, 0.25, 1.75),
        (0.0, 1.5708, 1.5708),
        vec![],
    );
    let right_hind_inner = part(
        "right_leg_r1",
        Some(3),
        (0.0, 0.0, 0.0),
        (-1.5708, 0.0, 1.5708),
        vec![cube((20, 14), (0.0, 0.0, -0.5), (3.0, 0.0, 1.0))],
    );
    right_hind.name = "right_hind_leg".into();
    let mut head = one(
        "head",
        Some(1),
        (0.0, 0.25, -2.75),
        (0, 8),
        (-3.0, -2.0, -4.0),
        (6.0, 3.0, 4.0),
    );
    let left_gills = one(
        "left_gills",
        Some(8),
        (3.0, -0.5, -2.0),
        (20, 8),
        (0.0, -3.5, 0.0),
        (3.0, 5.0, 0.0),
    );
    let right_gills = one(
        "right_gills",
        Some(8),
        (-3.0, -0.5, -2.0),
        (20, 3),
        (-3.0, -3.5, 0.0),
        (3.0, 5.0, 0.0),
    );
    let top_gills = one(
        "top_gills",
        Some(8),
        (0.0, -2.0, -2.0),
        (20, 0),
        (-3.0, -3.0, 0.0),
        (6.0, 3.0, 0.0),
    );
    head.cubes[0].deformation = 0.0;
    body.name = "body".into();
    bake_model(
        vec![
            part("root", None, (0.0, 24.0, 0.0), (0.0, 0.0, 0.0), vec![]),
            body,
            one(
                "right_front_leg",
                Some(1),
                (-2.0, 0.25, -1.25),
                (20, 16),
                (-3.0, 0.0, -0.5),
                (3.0, 0.0, 1.0),
            ),
            right_hind,
            right_hind_inner,
            one(
                "left_front_leg",
                Some(1),
                (2.0, 0.25, -1.25),
                (20, 13),
                (0.0, 0.0, -0.5),
                (3.0, 0.0, 1.0),
            ),
            one(
                "left_hind_leg",
                Some(1),
                (2.0, 0.25, 1.75),
                (20, 14),
                (0.0, 0.0, -0.5),
                (3.0, 0.0, 1.0),
            ),
            part(
                "tail",
                Some(1),
                (0.0, -0.25, 3.25),
                (0.0, 0.0, 0.0),
                vec![cube((10, 9), (0.0, -1.5, -1.0), (0.0, 3.0, 8.0))],
            ),
            head,
            left_gills,
            right_gills,
            top_gills,
        ],
        32,
        32,
    )
}

/// Adult `DolphinModel.createBodyLayer` (64x64).
pub fn bake_dolphin_model() -> BakedEntityModel {
    let mut left_fin = one(
        "left_fin",
        Some(0),
        (2.0, -2.0, 4.0),
        (48, 20),
        (-0.5, -4.0, 0.0),
        (1.0, 4.0, 7.0),
    );
    left_fin.default_rotation = Vec3::new(1.0471976, 0.0, 2.0943952);
    left_fin.cubes[0].mirror = true;
    let mut right_fin = one(
        "right_fin",
        Some(0),
        (-2.0, -2.0, 4.0),
        (48, 20),
        (-0.5, -4.0, 0.0),
        (1.0, 4.0, 7.0),
    );
    right_fin.default_rotation = Vec3::new(1.0471976, 0.0, -2.0943952);
    let mut tail = one(
        "tail",
        Some(0),
        (0.0, -2.5, 11.0),
        (0, 19),
        (-2.0, -2.5, 0.0),
        (4.0, 5.0, 11.0),
    );
    tail.default_rotation.x = -0.10471976;
    let mut back_fin = one(
        "back_fin",
        Some(0),
        (0.0, 0.0, 0.0),
        (51, 0),
        (-0.5, 0.0, 8.0),
        (1.0, 4.0, 5.0),
    );
    back_fin.default_rotation.x = 1.0471976;
    bake_model(
        vec![
            one(
                "body",
                None,
                (0.0, 22.0, -5.0),
                (22, 0),
                (-4.0, -7.0, 0.0),
                (8.0, 7.0, 13.0),
            ),
            one(
                "head",
                Some(0),
                (0.0, -4.0, -3.0),
                (0, 0),
                (-4.0, -3.0, -3.0),
                (8.0, 7.0, 6.0),
            ),
            one(
                "nose",
                Some(1),
                (0.0, 0.0, 0.0),
                (0, 13),
                (-1.0, 2.0, -7.0),
                (2.0, 2.0, 4.0),
            ),
            left_fin,
            right_fin,
            tail,
            one(
                "tail_fin",
                Some(5),
                (0.0, 0.0, 9.0),
                (19, 20),
                (-5.0, -0.5, 0.0),
                (10.0, 1.0, 6.0),
            ),
            back_fin,
        ],
        64,
        64,
    )
}

/// `BabyDolphinModel.createBodyLayer` is an independent authored layer (not
/// merely a scaled adult). Its mesh uses the same 64x64 sheet.
pub fn bake_baby_dolphin_model() -> BakedEntityModel {
    let mut left_fin = one(
        "left_fin",
        Some(0),
        (1.8, 0.85, -2.6),
        (34, 18),
        (-0.5, -1.5, -0.5),
        (1.0, 3.0, 6.0),
    );
    left_fin.default_rotation = Vec3::new(0.8727, 0.0, 1.7017);
    let mut right_fin = one(
        "right_fin",
        Some(0),
        (-1.8, 0.85, -2.6),
        (48, 18),
        (-0.5, -1.5, -0.5),
        (1.0, 3.0, 6.0),
    );
    right_fin.default_rotation = Vec3::new(0.8727, 0.0, -1.7017);
    let mut tail = one(
        "tail",
        Some(0),
        (0.0, 1.0, 4.0),
        (0, 13),
        (-2.0, -1.5, 0.0),
        (4.0, 3.0, 7.0),
    );
    tail.default_rotation.x = -0.10471976;
    bake_model(
        vec![
            one(
                "body",
                None,
                (0.0, 21.5, 0.0),
                (20, 0),
                (-3.0, -2.5, -4.0),
                (6.0, 5.0, 8.0),
            ),
            one(
                "head",
                Some(0),
                (0.0, 1.0, -4.0),
                (0, 0),
                (-3.0, -3.5, -4.0),
                (6.0, 5.0, 4.0),
            ),
            one(
                "nose",
                Some(1),
                (0.0, 0.5, -4.0),
                (0, 9),
                (-1.0, -1.0, -2.0),
                (2.0, 2.0, 2.0),
            ),
            left_fin,
            right_fin,
            tail,
            one(
                "tail_fin",
                Some(5),
                (0.0, 0.0, 9.0),
                (22, 13),
                (-4.0, -0.5, -1.0),
                (8.0, 1.0, 4.0),
            ),
            one(
                "back_fin",
                Some(0),
                (0.0, -1.0, -2.7),
                (42, 0),
                (-0.5, -1.0, 1.0),
                (1.0, 3.0, 4.0),
            ),
        ],
        64,
        64,
    )
}

/// `GuardianModel.createBodyLayer` (64x64). Elder layer applies the separate
/// `MeshTransformer.scaling(2.35f)`; its renderer additionally uses 1.2f.
pub fn bake_guardian_model(elder: bool) -> BakedEntityModel {
    let mut head = part(
        "head",
        None,
        (0.0, 0.0, 0.0),
        (0.0, 0.0, 0.0),
        vec![
            cube((0, 0), (-6.0, 10.0, -8.0), (12.0, 12.0, 16.0)),
            cube((0, 28), (-8.0, 10.0, -6.0), (2.0, 12.0, 12.0)),
            cube((0, 28), (6.0, 10.0, -6.0), (2.0, 12.0, 12.0)),
            cube((16, 40), (-6.0, 8.0, -6.0), (12.0, 2.0, 12.0)),
            cube((16, 40), (-6.0, 22.0, -6.0), (12.0, 2.0, 12.0)),
        ],
    );
    head.cubes[2].mirror = true;
    let spike_x = [
        8.0, -8.0, 0.0, 0.0, -8.0, -8.0, 8.0, 8.0, 8.0, -8.0, 0.0, 0.0,
    ];
    let spike_y = [
        -8.0, -8.0, -8.0, -8.0, 0.0, 0.0, 0.0, 0.0, 8.0, 8.0, 8.0, 8.0,
    ];
    let spike_z = [
        8.0, -8.0, 0.0, 0.0, -8.0, -8.0, 8.0, 8.0, 8.0, -8.0, 0.0, 0.0,
    ];
    let spike_rx = [
        1.75, 0.25, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.25, 0.75, 0.0, 0.0,
    ];
    let spike_ry = [
        0.0, 0.0, 0.0, 0.0, 0.25, 1.75, 1.25, 0.75, 0.0, 0.0, 0.0, 0.0,
    ];
    let spike_rz = [
        0.0, 0.0, 0.25, 1.75, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.75, 1.25,
    ];
    let mut parts = vec![head];
    for i in 0..12 {
        let offset = 1.0 + (i as f32).cos() * 0.01;
        let mut spike = one(
            &format!("spike_{i}"),
            Some(0),
            (
                spike_x[i] * offset,
                16.0 + spike_y[i] * offset,
                spike_z[i] * offset,
            ),
            (0, 0),
            (-1.0, -4.5, -1.0),
            (2.0, 9.0, 2.0),
        );
        spike.default_rotation = Vec3::new(
            spike_rx[i] * std::f32::consts::PI,
            spike_ry[i] * std::f32::consts::PI,
            spike_rz[i] * std::f32::consts::PI,
        );
        parts.push(spike);
    }
    parts.push(one(
        "eye",
        Some(0),
        (0.0, 0.0, -8.25),
        (8, 0),
        (-1.0, 15.0, 0.0),
        (2.0, 2.0, 1.0),
    ));
    parts.push(one(
        "tail0",
        None,
        (0.0, 0.0, 0.0),
        (40, 0),
        (-2.0, 14.0, 7.0),
        (4.0, 4.0, 8.0),
    ));
    parts.push(one(
        "tail1",
        Some(14),
        (-1.5, 0.5, 14.0),
        (0, 54),
        (0.0, 14.0, 0.0),
        (3.0, 3.0, 7.0),
    ));
    parts.push(part(
        "tail2",
        Some(15),
        (0.5, 0.5, 6.0),
        (0.0, 0.0, 0.0),
        vec![
            cube((41, 32), (0.0, 14.0, 0.0), (2.0, 2.0, 6.0)),
            cube((25, 19), (1.0, 10.5, 3.0), (1.0, 9.0, 9.0)),
        ],
    ));
    bake_independent_roots_scaled(parts, if elder { 2.35 } else { 1.0 }, 64, 64)
}

/// `AdultTurtleModel.createBodyLayer` (128x64), egg and belly are separate
/// body cubes with their own vanilla UV offsets.
pub fn bake_turtle_model() -> BakedEntityModel {
    bake_model(
        vec![
            one(
                "head",
                None,
                (0.0, 19.0, -10.0),
                (3, 0),
                (-3.0, -1.0, -3.0),
                (6.0, 5.0, 6.0),
            ),
            part(
                "body",
                None,
                (0.0, 11.0, -10.0),
                (std::f32::consts::FRAC_PI_2, 0.0, 0.0),
                vec![
                    cube((7, 37), (-9.5, 3.0, -10.0), (19.0, 20.0, 6.0)),
                    cube((31, 1), (-5.5, 3.0, -13.0), (11.0, 18.0, 3.0)),
                ],
            ),
            one(
                "egg_belly",
                None,
                (0.0, 11.0, -10.0),
                (70, 33),
                (-4.5, 3.0, -14.0),
                (9.0, 18.0, 1.0),
            ),
            one(
                "right_hind_leg",
                None,
                (-3.5, 22.0, 11.0),
                (1, 23),
                (-2.0, 0.0, 0.0),
                (4.0, 1.0, 10.0),
            ),
            one(
                "left_hind_leg",
                None,
                (3.5, 22.0, 11.0),
                (1, 12),
                (-2.0, 0.0, 0.0),
                (4.0, 1.0, 10.0),
            ),
            one(
                "right_front_leg",
                None,
                (-5.0, 21.0, -4.0),
                (27, 30),
                (-13.0, 0.0, -2.0),
                (13.0, 1.0, 5.0),
            ),
            one(
                "left_front_leg",
                None,
                (5.0, 21.0, -4.0),
                (27, 24),
                (0.0, 0.0, -2.0),
                (13.0, 1.0, 5.0),
            ),
        ],
        128,
        64,
    )
}

/// Dedicated `BabyTurtleModel.createBodyLayer` (16x16), independent of adult.
pub fn bake_baby_turtle_model() -> BakedEntityModel {
    bake_model(
        vec![
            one(
                "body",
                None,
                (0.0, 22.9, 1.0),
                (0, 0),
                (-2.0, -1.0, -2.0),
                (4.0, 2.0, 4.0),
            ),
            one(
                "head",
                None,
                (0.0, 22.9, -1.0),
                (0, 6),
                (-1.5, -2.0, -3.0),
                (3.0, 3.0, 3.0),
            ),
            one(
                "right_hind_leg",
                None,
                (-2.0, 23.9, 2.5),
                (-1, 0),
                (-2.0, 0.0, -0.5),
                (2.0, 0.0, 1.0),
            ),
            one(
                "left_hind_leg",
                None,
                (2.0, 23.9, 2.5),
                (-1, 1),
                (0.0, 0.0, -0.5),
                (2.0, 0.0, 1.0),
            ),
            one(
                "right_front_leg",
                None,
                (-2.0, 23.9, -0.5),
                (8, 6),
                (-2.0, 0.0, -0.5),
                (2.0, 0.0, 1.0),
            ),
            one(
                "left_front_leg",
                None,
                (2.0, 23.9, -0.5),
                (8, 7),
                (0.0, 0.0, -0.5),
                (2.0, 0.0, 1.0),
            ),
        ],
        16,
        16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_layer(model: BakedEntityModel, tex: (u32, u32), cubes: usize) {
        let cubes_in_model: Vec<_> = model.parts.iter().flat_map(|p| &p.cubes).collect();
        assert!(!model.vertices.is_empty());
        assert_eq!(cubes_in_model.len(), cubes);
        assert_eq!(model.parts.len(), model.part_ranges.len());
        for c in cubes_in_model {
            let (u, v) = (c.tex_offset.0 as f32, c.tex_offset.1 as f32);
            let (w, h, d) = (c.size.x.abs(), c.size.y.abs(), c.size.z.abs());
            let faces = [
                [u + d, v + d, u + d + w, v + d + h],
                [u + 2.0 * d + w, v + d, u + 2.0 * d + 2.0 * w, v + d + h],
                [u + d, v, u + d + w, v + d],
                [u + d + w, v + d, u + d + 2.0 * w, v],
                [u, v + d, u, v + d + h],
                [u + d + w, v + d, u + 2.0 * d + w, v + d + h],
            ];
            for face in faces {
                for (lo, hi, extent) in [
                    (face[0].min(face[2]), face[0].max(face[2]), tex.0 as f32),
                    (face[1].min(face[3]), face[1].max(face[3]), tex.1 as f32),
                ] {
                    let shift = -(lo / extent).floor() * extent;
                    let (lo, hi) = if shift != 0.0 && hi + shift <= extent {
                        (lo + shift, hi + shift)
                    } else {
                        (lo, hi)
                    };
                    assert!(
                        lo >= 0.0 && hi <= extent,
                        "face UV {lo}..{hi} outside 0..{extent}"
                    );
                }
            }
        }
    }

    fn assert_cube(
        model: &BakedEntityModel,
        part_name: &str,
        cube_index: usize,
        uv: (i32, i32),
        origin: (f32, f32, f32),
        size: (f32, f32, f32),
    ) {
        let p = model.parts.iter().find(|p| p.name == part_name).unwrap();
        let c = &p.cubes[cube_index];
        assert_eq!(c.tex_offset, uv);
        assert_eq!(c.origin, Vec3::new(origin.0, origin.1, origin.2));
        assert_eq!(c.size, Vec3::new(size.0, size.1, size.2));
    }

    #[test]
    fn vanilla_layer_meshes_are_nonempty_uv_bounded_and_match_layer_cubes() {
        let axolotl = bake_axolotl_model();
        let baby_axolotl = bake_baby_axolotl_model();
        let dolphin = bake_dolphin_model();
        let baby_dolphin = bake_baby_dolphin_model();
        let guardian = bake_guardian_model(false);
        let elder_guardian = bake_guardian_model(true);
        let turtle = bake_turtle_model();
        let baby_turtle = bake_baby_turtle_model();

        assert_layer(axolotl.clone(), (64, 64), 11);
        assert_layer(baby_axolotl.clone(), (32, 32), 11);
        assert_layer(dolphin.clone(), (64, 64), 8);
        assert_layer(baby_dolphin.clone(), (64, 64), 8);
        assert_layer(guardian.clone(), (64, 64), 22);
        assert_layer(elder_guardian.clone(), (64, 64), 22);
        assert_layer(turtle.clone(), (128, 64), 8);
        assert_layer(baby_turtle.clone(), (16, 16), 6);

        assert_cube(
            &axolotl,
            "body",
            0,
            (0, 11),
            (-4.0, -2.0, -9.0),
            (8.0, 4.0, 10.0),
        );
        assert_cube(
            &baby_axolotl,
            "body",
            0,
            (0, 0),
            (-2.0, -0.75, -2.75),
            (4.0, 2.0, 6.0),
        );
        assert_cube(
            &dolphin,
            "body",
            0,
            (22, 0),
            (-4.0, -7.0, 0.0),
            (8.0, 7.0, 13.0),
        );
        assert_cube(
            &baby_dolphin,
            "body",
            0,
            (20, 0),
            (-3.0, -2.5, -4.0),
            (6.0, 5.0, 8.0),
        );
        assert_cube(
            &guardian,
            "head",
            0,
            (0, 0),
            (-6.0, 10.0, -8.0),
            (12.0, 12.0, 16.0),
        );
        assert!(guardian.parts[0].cubes[2].mirror);
        assert_cube(
            &elder_guardian,
            "head",
            0,
            (0, 0),
            (-6.0, 10.0, -8.0),
            (12.0, 12.0, 16.0),
        );
        assert_eq!(elder_guardian.part_scales[0], 2.35);
        assert_cube(
            &turtle,
            "body",
            0,
            (7, 37),
            (-9.5, 3.0, -10.0),
            (19.0, 20.0, 6.0),
        );
        assert_cube(
            &baby_turtle,
            "body",
            0,
            (0, 0),
            (-2.0, -1.0, -2.0),
            (4.0, 2.0, 4.0),
        );
        let adult_ax_head = &axolotl
            .parts
            .iter()
            .find(|p| p.name == "head")
            .unwrap()
            .cubes[0];
        assert_eq!(adult_ax_head.deformation, 0.001);
    }
}
