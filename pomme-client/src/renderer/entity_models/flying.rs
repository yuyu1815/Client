//! Vanilla 26.2 flying / insect entity meshes. Geometry is in model pixels;
//! per-entity renderer scale and animation remain the caller's responsibility.
use glam::Vec3;

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

fn cube(uv: (i32, i32), pos: (f32, f32, f32), size: (f32, f32, f32)) -> ModelCube {
    ModelCube {
        origin: Vec3::new(pos.0, pos.1, pos.2),
        size: Vec3::new(size.0, size.1, size.2),
        tex_offset: uv,
        deformation: 0.0,
        mirror: false,
    }
}

fn part(
    name: &str,
    parent: Option<usize>,
    pivot: (f32, f32, f32),
    cubes: Vec<ModelCube>,
) -> EntityPart {
    EntityPart {
        name: name.into(),
        offset: Vec3::new(pivot.0, pivot.1, pivot.2),
        default_rotation: Vec3::ZERO,
        cubes,
        parent,
    }
}

fn bake(parts: Vec<EntityPart>, w: u32, h: u32) -> BakedEntityModel {
    bake_model(parts, w, h)
}

/// AllayModel.createBodyLayer, 32x32: oversized head, narrow body, arms and
/// two thin wings. Vanilla entity scale is 0.35 (applied by the renderer).
pub fn bake_allay_model() -> BakedEntityModel {
    bake(
        vec![
            part(
                "head",
                None,
                (0.0, 4.0, 0.0),
                vec![cube((0, 0), (-3.0, -4.0, -3.0), (6.0, 6.0, 6.0))],
            ),
            part(
                "body",
                None,
                (0.0, 4.0, 0.0),
                vec![cube((0, 16), (-2.0, 2.0, -1.0), (4.0, 5.0, 2.0))],
            ),
            part(
                "right_arm",
                Some(1),
                (-2.0, 3.0, 0.0),
                vec![cube((16, 16), (-1.0, 0.0, -1.0), (2.0, 4.0, 2.0))],
            ),
            part(
                "left_arm",
                Some(1),
                (2.0, 3.0, 0.0),
                vec![cube((16, 16), (-1.0, 0.0, -1.0), (2.0, 4.0, 2.0))],
            ),
            part(
                "right_wing",
                Some(1),
                (-2.0, 0.0, 0.0),
                vec![cube((0, 12), (-5.0, 0.0, 0.0), (5.0, 6.0, 0.0))],
            ),
            part(
                "left_wing",
                Some(1),
                (2.0, 0.0, 0.0),
                vec![cube((0, 12), (0.0, 0.0, 0.0), (5.0, 6.0, 0.0))],
            ),
        ],
        32,
        32,
    )
}

/// BeeModel.createBodyLayer, 64x64: segmented head/body, stinger, six legs,
/// and paired zero-depth wings (the wings animate around their body pivots).
pub fn bake_bee_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "body",
            None,
            (0.0, 19.0, 0.0),
            vec![cube((0, 0), (-3.5, -4.0, -5.0), (7.0, 5.0, 10.0))],
        ),
        part(
            "head",
            None,
            (0.0, 19.0, -5.0),
            vec![cube((0, 0), (-2.5, -3.0, -3.0), (5.0, 4.0, 4.0))],
        ),
        part(
            "stinger",
            Some(0),
            (0.0, -2.0, 5.0),
            vec![cube((26, 7), (-0.5, 0.0, 0.0), (1.0, 1.0, 2.0))],
        ),
    ];
    for (i, x) in [-2.0, 0.0, 2.0].into_iter().enumerate() {
        p.push(part(
            if i == 0 {
                "right_front_leg"
            } else if i == 1 {
                "right_middle_leg"
            } else {
                "right_hind_leg"
            },
            Some(0),
            (x, 1.0, -2.0 + i as f32 * 2.0),
            vec![cube((26, 0), (-0.5, 0.0, -1.0), (1.0, 3.0, 1.0))],
        ));
        p.push(part(
            if i == 0 {
                "left_front_leg"
            } else if i == 1 {
                "left_middle_leg"
            } else {
                "left_hind_leg"
            },
            Some(0),
            (-x, 1.0, -2.0 + i as f32 * 2.0),
            vec![cube((26, 0), (-0.5, 0.0, -1.0), (1.0, 3.0, 1.0))],
        ));
    }
    p.push(part(
        "right_wing",
        Some(0),
        (-2.0, -4.0, -1.0),
        vec![cube((0, 21), (-5.0, 0.0, -2.0), (5.0, 0.0, 7.0))],
    ));
    p.push(part(
        "left_wing",
        Some(0),
        (2.0, -4.0, -1.0),
        vec![cube((0, 21), (0.0, 0.0, -2.0), (5.0, 0.0, 7.0))],
    ));
    bake(p, 64, 64)
}

/// BlazeModel.createBodyLayer, 64x32: head/body and twelve orbiting rods.
pub fn bake_blaze_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            None,
            (0.0, 8.0, 0.0),
            vec![cube((0, 0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))],
        ),
        part(
            "body",
            None,
            (0.0, 8.0, 0.0),
            vec![cube((0, 16), (-4.0, 4.0, -4.0), (8.0, 4.0, 8.0))],
        ),
    ];
    for i in 0..12 {
        let band = i / 4;
        let a = (i % 4) as f32 * std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4;
        let (y, radius, len) = match band {
            0 => (0.0, 9.0, 2.0),
            1 => (4.0, 6.0, 4.0),
            _ => (8.0, 9.0, 2.0),
        };
        let center = Vec3::new(a.cos() * radius, y, a.sin() * radius);
        p.push(part(
            &format!("rod{i}"),
            None,
            (center.x, center.y, center.z),
            vec![cube((0, 16), (-1.0, -len / 2.0, -1.0), (2.0, len, 2.0))],
        ));
    }
    bake(p, 64, 32)
}

/// BreezeModel: 32x32 cuboid core with its characteristic separated wind curls.
pub fn bake_breeze_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            None,
            (0.0, 8.0, 0.0),
            vec![cube((0, 0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))],
        ),
        part(
            "body",
            None,
            (0.0, 8.0, 0.0),
            vec![cube((0, 16), (-4.0, 4.0, -4.0), (8.0, 8.0, 8.0))],
        ),
    ];
    for i in 0..4 {
        let a = i as f32 * std::f32::consts::FRAC_PI_2;
        let x = a.cos() * 5.0;
        let z = a.sin() * 5.0;
        p.push(part(
            &format!("wind{i}"),
            Some(1),
            (x, 1.0, z),
            vec![cube((0, 24), (-1.0, -3.0, -1.0), (2.0, 6.0, 2.0))],
        ));
    }
    bake(p, 32, 32)
}

fn ghast_parts(happy: bool) -> Vec<EntityPart> {
    let mut p = vec![part(
        "body",
        None,
        (0.0, 8.0, 0.0),
        vec![cube((0, 0), (-8.0, -8.0, -8.0), (16.0, 16.0, 16.0))],
    )];
    for i in 0..9 {
        let x = ((i % 3) as f32 - 1.0) * 5.0;
        let z = ((i / 3) as f32 - 1.0) * 5.0;
        let len = if happy {
            8.0 + (i % 3) as f32
        } else {
            8.0 + ((i * 5) % 7) as f32
        };
        p.push(part(
            &format!("tentacle{i}"),
            Some(0),
            (x, 8.0, z),
            vec![cube((0, 0), (-1.0, 0.0, -1.0), (2.0, len, 2.0))],
        ));
    }
    p
}

/// GhastModel.createBodyLayer, 128x64: 16-cube body and nine independent
/// tentacles. Tentacle motion / shooting texture state are not baked here.
pub fn bake_ghast_model() -> BakedEntityModel {
    bake(ghast_parts(false), 128, 64)
}

/// HappyGhast's 26.2 body and nine hanging tentacles; harness/ropes are
/// equipment overlays and deliberately not part of the base skin mesh.
pub fn bake_happy_ghast_model() -> BakedEntityModel {
    bake(ghast_parts(true), 128, 64)
}

/// PhantomModel: long head/body, articulated tapered wing segments, tail and
/// paired hind limbs. Vanilla uses a 64x64 skin and animated wing joints.
pub fn bake_phantom_model() -> BakedEntityModel {
    bake(
        vec![
            part(
                "head",
                None,
                (0.0, 4.0, -5.0),
                vec![cube((0, 0), (-2.0, -2.0, -3.0), (4.0, 4.0, 5.0))],
            ),
            part(
                "body",
                None,
                (0.0, 4.0, 0.0),
                vec![cube((0, 9), (-3.0, -2.0, -3.0), (6.0, 4.0, 12.0))],
            ),
            part(
                "tail",
                Some(1),
                (0.0, 0.0, 9.0),
                vec![cube((0, 25), (-1.0, -1.0, 0.0), (2.0, 2.0, 8.0))],
            ),
            part(
                "right_wing",
                Some(1),
                (-2.0, 0.0, 0.0),
                vec![cube((0, 32), (-9.0, 0.0, -2.0), (9.0, 1.0, 10.0))],
            ),
            part(
                "left_wing",
                Some(1),
                (2.0, 0.0, 0.0),
                vec![cube((0, 32), (0.0, 0.0, -2.0), (9.0, 1.0, 10.0))],
            ),
            part(
                "right_wing_tip",
                Some(3),
                (-8.0, 0.0, 7.0),
                vec![cube((0, 44), (-5.0, 0.0, -2.0), (5.0, 1.0, 8.0))],
            ),
            part(
                "left_wing_tip",
                Some(4),
                (8.0, 0.0, 7.0),
                vec![cube((0, 44), (0.0, 0.0, -2.0), (5.0, 1.0, 8.0))],
            ),
            part(
                "right_leg",
                Some(1),
                (-2.0, 1.0, 3.0),
                vec![cube((24, 0), (-1.0, 0.0, -1.0), (2.0, 3.0, 2.0))],
            ),
            part(
                "left_leg",
                Some(1),
                (2.0, 1.0, 3.0),
                vec![cube((24, 0), (-1.0, 0.0, -1.0), (2.0, 3.0, 2.0))],
            ),
        ],
        64,
        64,
    )
}

/// VexModel: small illager head/robe/arms with two thin wings, on a 32x32 skin.
pub fn bake_vex_model() -> BakedEntityModel {
    bake(
        vec![
            part(
                "head",
                None,
                (0.0, 4.0, 0.0),
                vec![cube((0, 0), (-3.0, -4.0, -3.0), (6.0, 6.0, 6.0))],
            ),
            part(
                "body",
                None,
                (0.0, 4.0, 0.0),
                vec![cube((0, 16), (-3.0, 2.0, -2.0), (6.0, 7.0, 4.0))],
            ),
            part(
                "right_wing",
                Some(1),
                (-3.0, 2.0, 0.0),
                vec![cube((0, 24), (-6.0, -1.0, 0.0), (6.0, 8.0, 0.0))],
            ),
            part(
                "left_wing",
                Some(1),
                (3.0, 2.0, 0.0),
                vec![cube((0, 24), (0.0, -1.0, 0.0), (6.0, 8.0, 0.0))],
            ),
            part(
                "right_arm",
                Some(1),
                (-3.0, 3.0, 0.0),
                vec![cube((16, 16), (-1.0, 0.0, -1.0), (2.0, 5.0, 2.0))],
            ),
            part(
                "left_arm",
                Some(1),
                (3.0, 3.0, 0.0),
                vec![cube((16, 16), (-1.0, 0.0, -1.0), (2.0, 5.0, 2.0))],
            ),
        ],
        32,
        32,
    )
}

/// WitherBossModel: three heads, central ribbed torso and lower body segment.
pub fn bake_wither_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "body",
            None,
            (0.0, 8.0, 0.0),
            vec![cube((0, 16), (-3.0, 0.0, -3.0), (6.0, 12.0, 6.0))],
        ),
        part(
            "rib",
            Some(0),
            (0.0, 3.0, 0.0),
            vec![cube((0, 32), (-10.0, 0.0, -2.0), (20.0, 3.0, 4.0))],
        ),
        part(
            "lower_body",
            Some(0),
            (0.0, 10.0, 0.0),
            vec![cube((0, 16), (-3.0, 0.0, -3.0), (6.0, 6.0, 6.0))],
        ),
    ];
    for (i, x) in [-8.0, 0.0, 8.0].into_iter().enumerate() {
        p.push(part(
            &format!("head{i}"),
            None,
            (x, 4.0, 0.0),
            vec![cube((0, 0), (-4.0, -4.0, -4.0), (8.0, 8.0, 8.0))],
        ));
    }
    bake(p, 64, 64)
}

/// EnderDragonModel: long neck/tail segments, horned head, body, four legs,
/// and paired broad wings with articulated tips. 256x256 vanilla texture.
pub fn bake_ender_dragon_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            None,
            (0.0, 16.0, -8.0),
            vec![
                cube((0, 0), (-4.0, -3.0, -8.0), (8.0, 6.0, 8.0)),
                cube((56, 0), (-4.0, -3.0, -11.0), (8.0, 1.0, 3.0)),
            ],
        ),
        part(
            "body",
            None,
            (0.0, 16.0, 0.0),
            vec![cube((0, 32), (-6.0, -5.0, -10.0), (12.0, 10.0, 20.0))],
        ),
    ];
    for i in 0..5 {
        let z = 8.0 + i as f32 * 6.0;
        let s = 7.0 - i as f32;
        p.push(part(
            &format!("tail{i}"),
            Some(1),
            (0.0, 0.0, z),
            vec![cube((0, 64), (-s / 2.0, -s / 2.0, 0.0), (s, s, 7.0))],
        ));
        let neck_z = -8.0 - i as f32 * 3.0;
        p.push(part(
            &format!("neck{i}"),
            Some(1),
            (0.0, -1.0, neck_z),
            vec![cube(
                (0, 80),
                (-3.0 + i as f32 * 0.3, -3.0 + i as f32 * 0.3, -5.0),
                (6.0 - i as f32 * 0.6, 6.0 - i as f32 * 0.6, 6.0),
            )],
        ));
    }
    for (side, x) in [("right", -5.0), ("left", 5.0)] {
        p.push(part(
            &format!("{side}_wing"),
            Some(1),
            (x, -2.0, -1.0),
            vec![cube(
                (0, 112),
                (if x < 0.0 { -28.0 } else { 0.0 }, 0.0, -4.0),
                (28.0, 2.0, 24.0),
            )],
        ));
        p.push(part(
            &format!("{side}_wing_tip"),
            Some(if x < 0.0 { 12 } else { 14 }),
            (if x < 0.0 { -26.0 } else { 26.0 }, 0.0, 16.0),
            vec![cube(
                (0, 144),
                (if x < 0.0 { -18.0 } else { 0.0 }, 0.0, -3.0),
                (18.0, 1.0, 16.0),
            )],
        ));
        p.push(part(
            &format!("{side}_hind_leg"),
            Some(1),
            (x * 0.7, 4.0, 8.0),
            vec![cube((0, 176), (-2.0, 0.0, -2.0), (4.0, 10.0, 4.0))],
        ));
        p.push(part(
            &format!("{side}_front_leg"),
            Some(1),
            (x * 0.7, 4.0, -5.0),
            vec![cube((0, 192), (-2.0, 0.0, -2.0), (4.0, 9.0, 4.0))],
        ));
    }
    bake(p, 256, 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flying_bakes_have_geometry_uvs_and_bounded_cubes() {
        let models = [
            (bake_allay_model(), 32, 32),
            (bake_bee_model(), 64, 64),
            (bake_blaze_model(), 64, 32),
            (bake_breeze_model(), 32, 32),
            (bake_ghast_model(), 128, 64),
            (bake_happy_ghast_model(), 128, 64),
            (bake_phantom_model(), 64, 64),
            (bake_vex_model(), 32, 32),
            (bake_wither_model(), 64, 64),
            (bake_ender_dragon_model(), 256, 256),
        ];
        for (model, tex_w, tex_h) in models {
            assert!(!model.parts.is_empty());
            assert!(!model.vertices.is_empty());
            assert_eq!(model.parts.len(), model.part_ranges.len());
            for part in model.parts {
                for c in part.cubes {
                    assert!(c.size.min_element() >= 0.0 && c.size.max_element() > 0.0);
                    let u = c.tex_offset.0 as f32;
                    let v = c.tex_offset.1 as f32;
                    assert!(u + 2.0 * c.size.z + 2.0 * c.size.x <= tex_w as f32);
                    assert!(v + c.size.z + c.size.y <= tex_h as f32);
                }
            }
        }
    }
}
