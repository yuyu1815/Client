//! 26.2 nonliving projectile model meshes, transcribed from mapped client model
//! layer definitions. Entity transforms/material selection remain the caller's
//! job.
use glam::Vec3;

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

/// DragonFireballRenderer: one camera-facing 16x16 plane, offset to y=-4..12px.
pub fn bake_dragon_fireball_model() -> BakedEntityModel {
    let mut model = bake_model(
        vec![EntityPart {
            name: "quad".into(),
            offset: Vec3::new(0.0, 24.016, 0.0),
            default_rotation: Vec3::ZERO,
            cubes: vec![ModelCube {
                origin: Vec3::new(-8.0, -12.0, 0.0),
                size: Vec3::new(16.0, 16.0, 0.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        }],
        16,
        16,
    );
    // Cube baking emits both coplanar Z faces; DragonFireballRenderer emits one
    // quad.
    model.vertices.truncate(6);
    model.part_ranges[0].1 = 6;
    model
}

/// Vanilla `TridentModel.createLayer` (32x32): pole with three fork tines.
pub fn bake_trident_model() -> BakedEntityModel {
    let root = EntityPart {
        name: "pole".into(),
        offset: Vec3::ZERO,
        default_rotation: Vec3::ZERO,
        cubes: vec![ModelCube {
            origin: Vec3::new(-0.5, 2.0, -0.5),
            size: Vec3::new(1.0, 25.0, 1.0),
            tex_offset: (0, 6),
            deformation: 0.0,
            mirror: false,
        }],
        parent: None,
    };
    let child = |name: &str, origin: Vec3, size: Vec3, uv, mirror| EntityPart {
        name: name.into(),
        offset: Vec3::ZERO,
        default_rotation: Vec3::ZERO,
        cubes: vec![ModelCube {
            origin,
            size,
            tex_offset: uv,
            deformation: 0.0,
            mirror,
        }],
        parent: Some(0),
    };
    bake_model(
        vec![
            root,
            child(
                "base",
                Vec3::new(-1.5, 0.0, -0.5),
                Vec3::new(3.0, 2.0, 1.0),
                (4, 0),
                false,
            ),
            child(
                "left_spike",
                Vec3::new(-2.5, -3.0, -0.5),
                Vec3::new(1.0, 4.0, 1.0),
                (4, 3),
                false,
            ),
            child(
                "middle_spike",
                Vec3::new(-0.5, -4.0, -0.5),
                Vec3::new(1.0, 4.0, 1.0),
                (0, 0),
                false,
            ),
            child(
                "right_spike",
                Vec3::new(1.5, -3.0, -0.5),
                Vec3::new(1.0, 4.0, 1.0),
                (4, 3),
                true,
            ),
        ],
        32,
        32,
    )
}

/// Vanilla `ShulkerBulletModel.createBodyLayer`: three crossing 8x8x2 plates.
pub fn bake_shulker_bullet_model() -> BakedEntityModel {
    let cubes = [
        (
            Vec3::new(-4.0, -4.0, -1.0),
            Vec3::new(8.0, 8.0, 2.0),
            (0, 0),
        ),
        (
            Vec3::new(-1.0, -4.0, -4.0),
            Vec3::new(2.0, 8.0, 8.0),
            (0, 10),
        ),
        (
            Vec3::new(-4.0, -1.0, -4.0),
            Vec3::new(8.0, 2.0, 8.0),
            (20, 0),
        ),
    ];
    bake_model(
        vec![EntityPart {
            name: "main".into(),
            offset: Vec3::ZERO,
            default_rotation: Vec3::ZERO,
            cubes: cubes
                .into_iter()
                .map(|(origin, size, tex_offset)| ModelCube {
                    origin,
                    size,
                    tex_offset,
                    deformation: 0.0,
                    mirror: false,
                })
                .collect(),
            parent: None,
        }],
        64,
        64,
    )
}

/// Vanilla `SkullModel.createHeadModel`: the projectile's single 8-cube skull.
pub fn bake_wither_skull_model() -> BakedEntityModel {
    bake_model(
        vec![EntityPart {
            name: "head".into(),
            offset: Vec3::ZERO,
            default_rotation: Vec3::ZERO,
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, -8.0, -4.0),
                size: Vec3::new(8.0, 8.0, 8.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        }],
        64,
        64,
    )
}

/// Vanilla `LlamaSpitModel.createBodyLayer`: seven overlapping 2-cubes form
/// the small faceted spit ball (the renderer supplies its flight orientation).
pub fn bake_llama_spit_model() -> BakedEntityModel {
    let origins = [
        Vec3::new(-4.0, 0.0, 0.0),
        Vec3::new(0.0, -4.0, 0.0),
        Vec3::new(0.0, 0.0, -4.0),
        Vec3::ZERO,
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.0, 0.0, 2.0),
    ];
    bake_model(
        vec![EntityPart {
            name: "main".into(),
            offset: Vec3::ZERO,
            default_rotation: Vec3::ZERO,
            cubes: origins
                .into_iter()
                .map(|origin| ModelCube {
                    origin,
                    size: Vec3::splat(2.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                })
                .collect(),
            parent: None,
        }],
        64,
        32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projectile_meshes_have_expected_geometry_and_uv_coverage() {
        let fireball = bake_dragon_fireball_model();
        assert_eq!(fireball.vertices.len(), 6);
        assert_eq!(fireball.part_ranges, [(0, 6)]);
        assert!(fireball.vertices.iter().all(|v| v.position[2] == 0.0));

        let models = [
            (bake_trident_model(), 32, 4),
            (bake_shulker_bullet_model(), 32, 1),
            (bake_wither_skull_model(), 64, 1),
            (bake_llama_spit_model(), 64, 1),
        ];
        for (model, texture_width, parts) in models {
            assert!(!model.vertices.is_empty());
            assert_eq!(model.parts.len(), parts);
            assert!(
                model
                    .vertices
                    .iter()
                    .all(|v| v.position.iter().all(|p| p.is_finite()))
            );
            let min_u = model
                .vertices
                .iter()
                .map(|v| v.tex_coords[0])
                .min()
                .unwrap();
            let max_u = model
                .vertices
                .iter()
                .map(|v| v.tex_coords[0])
                .max()
                .unwrap();
            let min_v = model
                .vertices
                .iter()
                .map(|v| v.tex_coords[1])
                .min()
                .unwrap();
            let max_v = model
                .vertices
                .iter()
                .map(|v| v.tex_coords[1])
                .max()
                .unwrap();
            assert!(
                min_u < max_u && min_v < max_v,
                "UV coverage on {texture_width}px texture"
            );
        }

        let trident = bake_trident_model();
        assert_eq!(trident.parts[0].cubes[0].tex_offset, (0, 6));
        assert!(trident.parts[4].cubes[0].mirror);
        let shulker = bake_shulker_bullet_model();
        assert_eq!(shulker.parts[0].cubes.len(), 3);
        assert_eq!(shulker.parts[0].cubes[1].tex_offset, (0, 10));
        assert_eq!(shulker.parts[0].cubes[2].tex_offset, (20, 0));
        let skull = bake_wither_skull_model().parts[0].cubes[0];
        assert_eq!(skull.origin, Vec3::new(-4.0, -8.0, -4.0));
        assert_eq!(skull.tex_offset, (0, 0));
        assert_eq!(bake_llama_spit_model().parts[0].cubes.len(), 7);
    }
}
