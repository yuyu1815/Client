use glam::Vec3;

use super::entity_model::{
    BakedEntityModel, EntityPart, FACE_ALL, FACE_NEG_X, FACE_POS_X, ModelConvention, ModelCube,
    bake_model, generate_cube_vertices, generate_cube_vertices_faces,
};

/// Inactive conduit shell and default player head idle geometry.
pub fn bake_conduit_model() -> BakedEntityModel {
    let cube = ModelCube {
        origin: Vec3::new(-3.0, -3.0, -3.0),
        size: Vec3::splat(6.0),
        tex_offset: (0, 0),
        deformation: 0.0,
        mirror: false,
    };
    let mut vertices = Vec::new();
    generate_cube_vertices(&cube, 32, 16, FACE_ALL, false, &mut vertices);
    BakedEntityModel::new(
        vec![EntityPart {
            name: "shell".into(),
            offset: Vec3::new(8.0, 8.0, 8.0),
            default_rotation: Vec3::ZERO,
            cubes: Vec::new(),
            parent: None,
        }],
        vertices,
        vec![(0, 36)],
    )
}

/// Vanilla's flat-texture skull model: an 8x8x8 head on the mob's skin sheet.
pub fn bake_skull_model(texture_height: u32) -> BakedEntityModel {
    let head = ModelCube {
        origin: Vec3::new(-4.0, -8.0, -4.0),
        size: Vec3::splat(8.0),
        tex_offset: (0, 0),
        deformation: 0.0,
        mirror: false,
    };
    let mut vertices = Vec::new();
    generate_cube_vertices(&head, 64, texture_height, FACE_ALL, false, &mut vertices);
    BakedEntityModel::new(
        vec![EntityPart {
            name: "head".into(),
            offset: Vec3::ZERO,
            default_rotation: Vec3::ZERO,
            cubes: Vec::new(),
            parent: None,
        }],
        vertices,
        vec![(0, 36)],
    )
}

/// No faithful block-head mesh is implemented for dragon or piglin yet. An
/// empty model keeps those variants from silently rendering as a Steve head.
pub fn bake_unsupported_skull_model() -> BakedEntityModel {
    BakedEntityModel::new(Vec::new(), Vec::new(), Vec::new())
}

pub fn bake_player_head_model() -> BakedEntityModel {
    let head = ModelCube {
        origin: Vec3::new(-4.0, -8.0, -4.0),
        size: Vec3::splat(8.0),
        tex_offset: (0, 0),
        deformation: 0.0,
        mirror: false,
    };
    let hat = ModelCube {
        tex_offset: (32, 0),
        deformation: 0.25,
        ..head
    };
    let mut vertices = Vec::new();
    generate_cube_vertices(&head, 64, 64, FACE_ALL, false, &mut vertices);
    generate_cube_vertices(&hat, 64, 64, FACE_ALL, false, &mut vertices);
    BakedEntityModel::new(
        vec![EntityPart {
            name: "head".into(),
            offset: Vec3::ZERO,
            default_rotation: Vec3::ZERO,
            cubes: Vec::new(),
            parent: None,
        }],
        vertices,
        vec![(0, 72)],
    )
}

/// Shulker box, closed state. Matches vanilla `ShulkerModel`: a 16x12x16 lid
/// stacked on a 16x8x16 base, with the lid's bottom flush against the base's
/// top. Texture is 64x64 `entity/shulker/shulker_<color>.png`.
pub fn bake_shulker_box_model() -> BakedEntityModel {
    let base = EntityPart {
        name: "base".into(),
        offset: Vec3::new(0.0, 8.0, 0.0),
        default_rotation: Vec3::ZERO,
        cubes: vec![ModelCube {
            origin: Vec3::new(-8.0, 8.0, -8.0),
            size: Vec3::new(16.0, 8.0, 16.0),
            tex_offset: (0, 28),
            deformation: 0.0,
            mirror: false,
        }],
        parent: None,
    };
    let lid = EntityPart {
        name: "lid".into(),
        offset: Vec3::new(0.0, 24.0, 0.0),
        default_rotation: Vec3::ZERO,
        cubes: vec![ModelCube {
            origin: Vec3::new(-8.0, -16.0, -8.0),
            size: Vec3::new(16.0, 12.0, 16.0),
            tex_offset: (0, 0),
            deformation: 0.0,
            mirror: false,
        }],
        parent: None,
    };
    bake_model(vec![lid, base], 64, 64)
}

/// Standing sign, matching vanilla `block/template_sign_rot_0`: a 16x8x1.33
/// board (one block wide, centered) raised on a 1.33x9.33x1.33 post. Geometry
/// and UVs are in block-model units (16 = one block); UVs are in 0-16 space so
/// the model bakes against a 16x16 reference even though the texture
/// (`block/<wood>_sign.png`) is 32x32. Face order: -Z, +Z, top, bottom,
/// -X, +X.
pub fn bake_sign_model() -> BakedEntityModel {
    // Face order -Z, +Z, top, bottom, -X, +X; the render-space X flip puts
    // the model's -X face on the world's +X side, so the side rects are
    // assigned crosswise.
    const BOARD_UVS: [[f32; 4]; 6] = [
        [0.0, 8.0, 12.0, 14.0],  // -Z (back)
        [0.0, 1.0, 12.0, 7.0],   // +Z (front)
        [0.0, 0.0, 12.0, 1.0],   // top
        [0.0, 14.0, 12.0, 15.0], // bottom
        [12.0, 1.0, 13.0, 7.0],  // -X
        [12.0, 8.0, 13.0, 14.0], // +X
    ];
    // The post's top is hidden under the board, so its top face reuses the
    // bottom rect rather than claiming texture vanilla never assigns it.
    const POST_UVS: [[f32; 4]; 6] = [
        [14.0, 8.0, 15.0, 15.0],  // -Z
        [14.0, 0.0, 15.0, 7.0],   // +Z
        [14.0, 15.0, 15.0, 16.0], // top (hidden)
        [14.0, 15.0, 15.0, 16.0], // bottom
        [15.0, 0.0, 16.0, 7.0],   // -X
        [15.0, 8.0, 16.0, 15.0],  // +X
    ];

    let board = ModelCube {
        origin: Vec3::new(-8.0, -52.0 / 3.0, -2.0 / 3.0),
        size: Vec3::new(16.0, 8.0, 4.0 / 3.0),
        tex_offset: (0, 0),
        deformation: 0.0,
        mirror: false,
    };
    let post = ModelCube {
        origin: Vec3::new(-2.0 / 3.0, -28.0 / 3.0, -2.0 / 3.0),
        size: Vec3::new(4.0 / 3.0, 28.0 / 3.0, 4.0 / 3.0),
        tex_offset: (0, 0),
        deformation: 0.0,
        mirror: false,
    };

    let mut vertices = Vec::new();
    let mut part_ranges = Vec::new();
    let mut parts = Vec::new();
    for (name, cube, uvs) in [("sign", board, &BOARD_UVS), ("stick", post, &POST_UVS)] {
        let start = vertices.len() as u32;
        generate_cube_vertices_faces(&cube, uvs, 16, 16, &mut vertices);
        part_ranges.push((start, vertices.len() as u32 - start));
        parts.push(EntityPart {
            name: name.into(),
            offset: Vec3::new(0.0, 24.0, 0.0),
            default_rotation: Vec3::ZERO,
            // Vertices were emitted above with explicit UVs, so no cubes to bake.
            cubes: Vec::new(),
            parent: None,
        });
    }
    BakedEntityModel::new(parts, vertices, part_ranges)
}

/// One chest layer as parts [bottom, lid, lock], matching vanilla `ChestModel`
/// (single/double-left/double-right differ only in body/lock x extents and the
/// culled seam face). Texture 64x64; lid and lock pivot at offset (0, 9, 1).
/// Baked in literal y-up block space (`y_down: false`).
// TODO: full-bright; vanilla samples the lightmap at the block (pending
// lighting support in the entity pipeline).
fn bake_chest_layer(
    body_x0: f32,
    body_w: f32,
    lock_x0: f32,
    lock_w: f32,
    faces: u8,
) -> BakedEntityModel {
    let cubes = [
        (
            "bottom",
            Vec3::ZERO,
            Vec3::new(body_x0, 0.0, 1.0),
            Vec3::new(body_w, 10.0, 14.0),
            (0, 19),
        ),
        (
            "lid",
            Vec3::new(0.0, 9.0, 1.0),
            Vec3::new(body_x0, 0.0, 0.0),
            Vec3::new(body_w, 5.0, 14.0),
            (0, 0),
        ),
        (
            "lock",
            Vec3::new(0.0, 9.0, 1.0),
            Vec3::new(lock_x0, -2.0, 14.0),
            Vec3::new(lock_w, 4.0, 1.0),
            (0, 0),
        ),
    ];

    let mut vertices = Vec::new();
    let mut part_ranges = Vec::new();
    let mut parts = Vec::new();
    for (name, offset, origin, size, tex_offset) in cubes {
        let start = vertices.len() as u32;
        let cube = ModelCube {
            origin,
            size,
            tex_offset,
            deformation: 0.0,
            mirror: false,
        };
        generate_cube_vertices(&cube, 64, 64, faces, false, &mut vertices);
        part_ranges.push((start, vertices.len() as u32 - start));
        parts.push(EntityPart {
            name: name.into(),
            offset,
            default_rotation: Vec3::ZERO,
            cubes: Vec::new(),
            parent: None,
        });
    }
    BakedEntityModel::new(parts, vertices, part_ranges).with_convention(ModelConvention::BlockYUp)
}

/// Chest models in variant order [single, double-left, double-right], from
/// vanilla `ChestModel::createSingleBodyLayer` / `createDoubleBodyLeftLayer` /
/// `createDoubleBodyRightLayer`.
// TODO: copper chest variants (26.2) are not rendered yet.
pub fn bake_chest_models() -> Vec<BakedEntityModel> {
    vec![
        bake_chest_layer(1.0, 14.0, 7.0, 2.0, FACE_ALL),
        bake_chest_layer(0.0, 15.0, 0.0, 1.0, FACE_ALL & !FACE_NEG_X),
        bake_chest_layer(1.0, 15.0, 15.0, 1.0, FACE_ALL & !FACE_POS_X),
    ]
}

#[cfg(test)]
mod conduit_tests {
    use super::*;

    #[test]
    fn shell_uvs_use_the_official_32_by_16_sheet() {
        let cube = ModelCube {
            origin: Vec3::splat(-3.0),
            size: Vec3::splat(6.0),
            tex_offset: (0, 0),
            deformation: 0.0,
            mirror: false,
        };
        let mut shell = Vec::new();
        generate_cube_vertices(&cube, 32, 16, FACE_ALL, false, &mut shell);
        let mut wrong_sheet = Vec::new();
        generate_cube_vertices(&cube, 64, 64, FACE_ALL, false, &mut wrong_sheet);
        assert_eq!(shell.len(), 36);
        assert_ne!(
            shell.iter().map(|v| v.tex_coords).collect::<Vec<_>>(),
            wrong_sheet.iter().map(|v| v.tex_coords).collect::<Vec<_>>()
        );
        assert_eq!(shell, bake_conduit_model().vertices);
    }

    #[test]
    fn block_skull_uses_the_flat_64_by_32_head_mesh() {
        let cube = ModelCube {
            origin: Vec3::new(-4.0, -8.0, -4.0),
            size: Vec3::splat(8.0),
            tex_offset: (0, 0),
            deformation: 0.0,
            mirror: false,
        };
        let mut expected = Vec::new();
        generate_cube_vertices(&cube, 64, 32, FACE_ALL, false, &mut expected);
        let model = bake_skull_model(32);
        assert_eq!(model.vertices, expected);
        assert_eq!(model.part_ranges, [(0, 36)]);
        assert_eq!(bake_unsupported_skull_model().vertices.len(), 0);
    }
}
