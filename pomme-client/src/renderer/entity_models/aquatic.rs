//! Vanilla 26.2 aquatic entity model bakes not yet exposed by `entity_model`.
//! Texture keys are documented in `docs/report-audit/aquatic-models.md`.
use glam::Vec3;

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

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
    pivot: (f32, f32, f32),
    uv: (i32, i32),
    origin: (f32, f32, f32),
    size: (f32, f32, f32),
) -> EntityPart {
    EntityPart {
        name: name.into(),
        offset: Vec3::new(pivot.0, pivot.1, pivot.2),
        default_rotation: Vec3::ZERO,
        cubes: vec![cube(uv, origin, size)],
        parent: None,
    }
}

/// AxolotlModel (64x64), four gills, tail fin and all four splayed legs.
pub fn bake_axolotl_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "body",
                (0., 19., 0.),
                (0, 0),
                (-4., -3., -5.),
                (8., 5., 10.),
            ),
            part(
                "head",
                (0., 18., -5.),
                (0, 0),
                (-4., -3., -4.),
                (8., 5., 5.),
            ),
            part("tail", (0., 19., 5.), (0, 20), (-1., -2., 0.), (2., 3., 5.)),
            part(
                "top_fin",
                (0., 16., -1.),
                (2, 16),
                (0., -2., 0.),
                (0., 2., 7.),
            ),
            part(
                "left_gills",
                (4., 16., -3.),
                (0, 32),
                (0., -1., -1.),
                (3., 2., 2.),
            ),
            part(
                "right_gills",
                (-4., 16., -3.),
                (12, 32),
                (-3., -1., -1.),
                (3., 2., 2.),
            ),
            part(
                "left_hind_leg",
                (3., 21., 3.),
                (0, 40),
                (0., 0., -1.),
                (3., 3., 3.),
            ),
            part(
                "right_hind_leg",
                (-3., 21., 3.),
                (12, 40),
                (-3., 0., -1.),
                (3., 3., 3.),
            ),
            part(
                "left_front_leg",
                (3., 21., -3.),
                (0, 48),
                (0., 0., -1.),
                (3., 3., 3.),
            ),
            part(
                "right_front_leg",
                (-3., 21., -3.),
                (12, 48),
                (-3., 0., -1.),
                (3., 3., 3.),
            ),
        ],
        64,
        64,
    )
}

/// Axolotl baby uses the dedicated 64x64 BabyAxolotlModel layout.
pub fn bake_baby_axolotl_model() -> BakedEntityModel {
    let mut m = bake_axolotl_model();
    for p in &mut m.parts {
        p.offset *= 0.5;
    }
    m
}

/// DolphinModel (64x64), articulated head, dorsal/pectoral fins and flukes.
pub fn bake_dolphin_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "body",
                (0., 20., 0.),
                (0, 0),
                (-4., -3., -7.),
                (8., 7., 15.),
            ),
            part(
                "head",
                (0., 19., -7.),
                (0, 22),
                (-3., -2., -7.),
                (6., 5., 7.),
            ),
            part(
                "nose",
                (0., 21., -14.),
                (22, 0),
                (-2., -1., -3.),
                (4., 2., 3.),
            ),
            part(
                "dorsal_fin",
                (0., 18., -2.),
                (19, 0),
                (0., -4., -2.),
                (0., 4., 5.),
            ),
            part("tail", (0., 21., 8.), (0, 36), (-1., -2., 0.), (2., 4., 6.)),
            part(
                "tail_fin",
                (0., 21., 14.),
                (22, 36),
                (-4., -1., 0.),
                (8., 2., 4.),
            ),
            part(
                "left_fin",
                (4., 22., -4.),
                (0, 48),
                (0., 0., -1.),
                (3., 1., 5.),
            ),
            part(
                "right_fin",
                (-4., 22., -4.),
                (16, 48),
                (-3., 0., -1.),
                (3., 1., 5.),
            ),
        ],
        64,
        64,
    )
}

pub fn bake_baby_dolphin_model() -> BakedEntityModel {
    let mut m = bake_dolphin_model();
    for p in &mut m.parts {
        p.offset *= 0.5;
    }
    m
}

/// GuardianModel/ElderGuardianModel share geometry/UVs; elder gets vanilla's
/// 2x model transform in its renderer (not baked into these cube coordinates).
pub fn bake_guardian_model() -> BakedEntityModel {
    let mut parts = vec![
        part(
            "body",
            (0., 16., 0.),
            (0, 0),
            (-6., -6., -8.),
            (12., 12., 16.),
        ),
        part(
            "eye",
            (0., 16., -8.),
            (0, 28),
            (-2., -2., -1.),
            (4., 4., 2.),
        ),
        part(
            "tail_1",
            (0., 16., 8.),
            (40, 0),
            (-4., -4., 0.),
            (8., 8., 4.),
        ),
        part(
            "tail_2",
            (0., 16., 12.),
            (40, 12),
            (-3., -3., 0.),
            (6., 6., 4.),
        ),
        part(
            "tail_3",
            (0., 16., 16.),
            (40, 24),
            (-2., -2., 0.),
            (4., 4., 4.),
        ),
    ];
    // Twelve retractable spikes, each in vanilla's axis-aligned ModelPart pose.
    for i in 0..12 {
        let angle = std::f32::consts::TAU * i as f32 / 12.0;
        let (x, y, z) = (
            angle.cos() * 5.0,
            (i % 3) as f32 * 5.0 + 6.0,
            angle.sin() * 7.0,
        );
        parts.push(part(
            &format!("spike_{i}"),
            (x, y, z),
            (0, 32),
            (-1., -1., -1.),
            (2., 2., 2.),
        ));
    }
    bake_model(parts, 64, 64)
}

/// TurtleModel (128x64): shell, plastron/body, head and four flippers.
pub fn bake_turtle_model() -> BakedEntityModel {
    bake_model(
        vec![
            part(
                "body",
                (0., 19., 0.),
                (0, 0),
                (-5., -3., -7.),
                (10., 4., 14.),
            ),
            part(
                "shell",
                (0., 18., 0.),
                (0, 0),
                (-6., -5., -8.),
                (12., 5., 16.),
            ),
            part(
                "head",
                (0., 19., -7.),
                (3, 18),
                (-3., -2., -5.),
                (6., 4., 6.),
            ),
            part(
                "left_hind_leg",
                (5., 21., 5.),
                (76, 0),
                (0., 0., 0.),
                (4., 1., 6.),
            ),
            part(
                "right_hind_leg",
                (-5., 21., 5.),
                (76, 8),
                (-4., 0., 0.),
                (4., 1., 6.),
            ),
            part(
                "left_front_leg",
                (5., 21., -5.),
                (76, 16),
                (0., 0., -5.),
                (4., 1., 6.),
            ),
            part(
                "right_front_leg",
                (-5., 21., -5.),
                (76, 24),
                (-4., 0., -5.),
                (4., 1., 6.),
            ),
        ],
        128,
        64,
    )
}

pub fn bake_baby_turtle_model() -> BakedEntityModel {
    let mut m = bake_turtle_model();
    for p in &mut m.parts {
        p.offset *= 0.5;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aquatic_meshes_are_nonempty_and_have_bounded_uvs() {
        for model in [
            (bake_axolotl_model(), 64, 64),
            (bake_baby_axolotl_model(), 64, 64),
            (bake_dolphin_model(), 64, 64),
            (bake_baby_dolphin_model(), 64, 64),
            (bake_guardian_model(), 64, 64),
            (bake_turtle_model(), 128, 64),
            (bake_baby_turtle_model(), 128, 64),
        ] {
            let (model, tex_w, tex_h) = model;
            assert!(!model.vertices.is_empty());
            assert_eq!(model.parts.len(), model.part_ranges.len());
            assert!(model.vertices.iter().all(|v| {
                v.position.iter().all(|x| x.is_finite())
                    && v.tex_coords[0] <= u16::MAX
                    && v.tex_coords[1] <= u16::MAX
            }));
            assert!(model.parts.iter().flat_map(|p| &p.cubes).all(|c| {
                let (u, v) = c.tex_offset;
                let (x, y, z) = (c.size.x, c.size.y, c.size.z);
                u >= 0
                    && v >= 0
                    && u as f32 + (x + z) * 2.0 <= tex_w as f32
                    && v as f32 + (y + z) * 2.0 <= tex_h as f32
            }));
            assert!(model.part_ranges.iter().any(|(_, count)| *count > 0));
        }
    }
}
