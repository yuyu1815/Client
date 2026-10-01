use glam::Vec3;

use super::entity_model::{
    BakedEntityModel, EntityPart, FACE_ALL, FACE_NEG_X, FACE_POS_X, ModelConvention, ModelCube,
    bake_model, generate_cube_vertices, generate_cube_vertices_faces,
};

/// Bell body from vanilla `BellModel`: two cubes with their own official
/// `bell_body.png` UV regions; blockstate models retain the supports.
pub fn bake_bell_model() -> BakedEntityModel {
    let cubes = vec![
        ModelCube {
            origin: Vec3::new(-3.0, -6.0, -3.0),
            size: Vec3::new(6.0, 7.0, 6.0),
            tex_offset: (0, 0),
            deformation: 0.0,
            mirror: false,
        },
        ModelCube {
            origin: Vec3::new(-8.0, -12.0, -8.0),
            size: Vec3::new(8.0, 2.0, 8.0),
            tex_offset: (0, 13),
            deformation: 0.0,
            mirror: false,
        },
    ];
    let mut vertices = Vec::new();
    for cube in &cubes {
        generate_cube_vertices(cube, 32, 32, FACE_ALL, true, &mut vertices);
    }
    BakedEntityModel::new(
        vec![EntityPart {
            name: "bell".into(),
            offset: Vec3::new(8.0, 12.0, 8.0),
            default_rotation: Vec3::ZERO,
            cubes,
            parent: None,
        }],
        vertices,
        vec![(0, 72)],
    )
}

/// 26.2 BannerModel and BannerFlagModel geometry (64x64 `banner_base` sheet).
/// Wall variants omit the freestanding pole and move the bar/cloth toward the
/// wall.
pub fn bake_banner_model(wall: bool) -> BakedEntityModel {
    let mut parts = Vec::new();
    if !wall {
        parts.push(EntityPart {
            name: "pole".into(),
            offset: Vec3::ZERO,
            default_rotation: Vec3::ZERO,
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -42.0, -1.0),
                size: Vec3::new(2.0, 42.0, 2.0),
                tex_offset: (44, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        });
    }
    parts.push(EntityPart {
        name: "bar".into(),
        offset: Vec3::ZERO,
        default_rotation: Vec3::ZERO,
        cubes: vec![ModelCube {
            origin: Vec3::new(
                -10.0,
                if wall { -20.5 } else { -44.0 },
                if wall { 9.5 } else { -1.0 },
            ),
            size: Vec3::new(20.0, 2.0, 2.0),
            tex_offset: (0, 42),
            deformation: 0.0,
            mirror: false,
        }],
        parent: None,
    });
    parts.push(EntityPart {
        name: "flag".into(),
        offset: Vec3::new(
            0.0,
            if wall { -20.5 } else { -44.0 },
            if wall { 10.5 } else { 0.0 },
        ),
        default_rotation: Vec3::ZERO,
        cubes: vec![ModelCube {
            origin: Vec3::new(-10.0, 0.0, -2.0),
            size: Vec3::new(20.0, 40.0, 1.0),
            tex_offset: (0, 0),
            deformation: 0.0,
            mirror: false,
        }],
        parent: None,
    });
    let mut vertices = Vec::new();
    let mut ranges = Vec::new();
    for part in &parts {
        let start = vertices.len() as u32;
        for cube in &part.cubes {
            generate_cube_vertices(cube, 64, 64, FACE_ALL, false, &mut vertices);
        }
        ranges.push((start, vertices.len() as u32 - start));
    }
    BakedEntityModel::new(parts, vertices, ranges).with_convention(ModelConvention::BlockYUp)
}

/// 26.2 DecoratedPotRenderer.createBaseLayer/createSidesLayer geometry.
/// Body planes are intentionally one-sided like the vanilla ModelPart cubes.
pub fn bake_decorated_pot_model() -> BakedEntityModel {
    let mut parts = Vec::new();
    let mut vertices = Vec::new();
    let mut ranges = Vec::new();
    let mut add = |name: &str,
                   offset: Vec3,
                   rotation: Vec3,
                   cubes: &[ModelCube],
                   faces: u8,
                   size: (u32, u32)| {
        let start = vertices.len() as u32;
        for cube in cubes {
            generate_cube_vertices(cube, size.0, size.1, faces, false, &mut vertices);
        }
        ranges.push((start, vertices.len() as u32 - start));
        parts.push(EntityPart {
            name: name.into(),
            offset,
            default_rotation: rotation,
            cubes: Vec::new(),
            parent: None,
        });
    };
    let neck = [
        ModelCube {
            origin: Vec3::new(4.0, 17.0, 4.0),
            size: Vec3::new(8.0, 3.0, 8.0),
            tex_offset: (0, 0),
            deformation: -0.1,
            mirror: false,
        },
        ModelCube {
            origin: Vec3::new(5.0, 20.0, 5.0),
            size: Vec3::new(6.0, 1.0, 6.0),
            tex_offset: (0, 5),
            deformation: 0.2,
            mirror: false,
        },
    ];
    add(
        "neck",
        Vec3::new(0.0, 37.0, 16.0),
        Vec3::new(std::f32::consts::PI, 0.0, 0.0),
        &neck,
        FACE_ALL,
        (32, 32),
    );
    let slab = ModelCube {
        origin: Vec3::ZERO,
        size: Vec3::new(14.0, 0.0, 14.0),
        tex_offset: (-14, 13),
        deformation: 0.0,
        mirror: false,
    };
    add(
        "top",
        Vec3::new(1.0, 16.0, 1.0),
        Vec3::ZERO,
        &[slab],
        1 << 3,
        (32, 32),
    );
    add(
        "bottom",
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::ZERO,
        &[slab],
        1 << 2,
        (32, 32),
    );
    let side = ModelCube {
        origin: Vec3::ZERO,
        size: Vec3::new(14.0, 16.0, 0.0),
        tex_offset: (1, 0),
        deformation: 0.0,
        mirror: false,
    };
    add(
        "front",
        Vec3::new(1.0, 16.0, 15.0),
        Vec3::new(std::f32::consts::PI, 0.0, 0.0),
        &[side],
        1,
        (16, 16),
    );
    add(
        "back",
        Vec3::new(15.0, 16.0, 1.0),
        Vec3::new(0.0, 0.0, std::f32::consts::PI),
        &[side],
        1,
        (16, 16),
    );
    add(
        "left",
        Vec3::new(1.0, 16.0, 1.0),
        Vec3::new(0.0, -std::f32::consts::FRAC_PI_2, std::f32::consts::PI),
        &[side],
        1,
        (16, 16),
    );
    add(
        "right",
        Vec3::new(15.0, 16.0, 15.0),
        Vec3::new(0.0, std::f32::consts::FRAC_PI_2, std::f32::consts::PI),
        &[side],
        1,
        (16, 16),
    );
    BakedEntityModel::new(parts, vertices, ranges).with_convention(ModelConvention::BlockYUp)
}

/// The 26.2 BookModel.createBodyLayer geometry and pivots (64x32 sheet).
pub fn bake_enchanting_book_model() -> BakedEntityModel {
    let definitions = [
        (
            "left_lid",
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(-6.0, -5.0, -0.005),
            Vec3::new(6.0, 10.0, 0.005),
            (0, 0),
        ),
        (
            "right_lid",
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, -5.0, -0.005),
            Vec3::new(6.0, 10.0, 0.005),
            (16, 0),
        ),
        (
            "seam",
            Vec3::ZERO,
            Vec3::new(-1.0, -5.0, 0.0),
            Vec3::new(2.0, 10.0, 0.005),
            (12, 0),
        ),
        (
            "left_pages",
            Vec3::ZERO,
            Vec3::new(0.0, -4.0, -0.99),
            Vec3::new(5.0, 8.0, 1.0),
            (0, 10),
        ),
        (
            "right_pages",
            Vec3::ZERO,
            Vec3::new(0.0, -4.0, -0.01),
            Vec3::new(5.0, 8.0, 1.0),
            (12, 10),
        ),
        (
            "flip_page1",
            Vec3::ZERO,
            Vec3::new(0.0, -4.0, 0.0),
            Vec3::new(5.0, 8.0, 0.005),
            (24, 10),
        ),
        (
            "flip_page2",
            Vec3::ZERO,
            Vec3::new(0.0, -4.0, 0.0),
            Vec3::new(5.0, 8.0, 0.005),
            (24, 10),
        ),
    ];
    let mut parts = Vec::new();
    let mut vertices = Vec::new();
    let mut ranges = Vec::new();
    for (name, offset, origin, size, tex_offset) in definitions {
        let start = vertices.len() as u32;
        generate_cube_vertices(
            &ModelCube {
                origin,
                size,
                tex_offset,
                deformation: 0.0,
                mirror: false,
            },
            64,
            32,
            FACE_ALL,
            false,
            &mut vertices,
        );
        ranges.push((start, vertices.len() as u32 - start));
        parts.push(EntityPart {
            name: name.into(),
            offset,
            default_rotation: if name == "seam" {
                Vec3::new(0.0, std::f32::consts::FRAC_PI_2, 0.0)
            } else {
                Vec3::ZERO
            },
            cubes: Vec::new(),
            parent: None,
        });
    }
    BakedEntityModel::new(parts, vertices, ranges).with_convention(ModelConvention::BlockYUp)
}

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

/// 26.2 DragonHeadModel: scaled head with the upper lip, skull, horns and
/// nostrils, plus its hinged 12x4x16 jaw. Texture sheet is 256x256.
pub fn bake_dragon_head_model() -> BakedEntityModel {
    let head = EntityPart {
        name: "head".into(),
        offset: Vec3::new(0.0, -7.986_666, 0.0),
        default_rotation: Vec3::ZERO,
        cubes: vec![
            ModelCube {
                origin: Vec3::new(-6.0, -1.0, -24.0),
                size: Vec3::new(12.0, 5.0, 16.0),
                tex_offset: (176, 44),
                deformation: 0.0,
                mirror: false,
            },
            ModelCube {
                origin: Vec3::new(-8.0, -8.0, -10.0),
                size: Vec3::splat(16.0),
                tex_offset: (112, 30),
                deformation: 0.0,
                mirror: true,
            },
            ModelCube {
                origin: Vec3::new(-5.0, -12.0, -4.0),
                size: Vec3::new(2.0, 4.0, 6.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            },
            ModelCube {
                origin: Vec3::new(3.0, -12.0, -4.0),
                size: Vec3::new(2.0, 4.0, 6.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: true,
            },
            ModelCube {
                origin: Vec3::new(-5.0, -3.0, -22.0),
                size: Vec3::new(2.0, 2.0, 4.0),
                tex_offset: (112, 0),
                deformation: 0.0,
                mirror: false,
            },
            ModelCube {
                origin: Vec3::new(3.0, -3.0, -22.0),
                size: Vec3::new(2.0, 2.0, 4.0),
                tex_offset: (112, 0),
                deformation: 0.0,
                mirror: false,
            },
        ],
        parent: None,
    };
    let jaw = EntityPart {
        name: "jaw".into(),
        offset: Vec3::new(0.0, 4.0, -8.0),
        default_rotation: Vec3::ZERO,
        cubes: vec![ModelCube {
            origin: Vec3::new(-6.0, 0.0, -16.0),
            size: Vec3::new(12.0, 4.0, 16.0),
            tex_offset: (176, 65),
            deformation: 0.0,
            mirror: false,
        }],
        parent: Some(0),
    };
    let mut model = bake_model(vec![head, jaw], 256, 256);
    model.part_scales[0] = 0.75;
    model
}

/// 26.2 PiglinHeadModel / AbstractPiglinModel.addHead: broad 10x8x8 head,
/// snout and nostrils, with separately posed 1x5x4 ears on the 64x64 skin.
pub fn bake_piglin_head_model() -> BakedEntityModel {
    let head = EntityPart {
        name: "head".into(),
        offset: Vec3::ZERO,
        default_rotation: Vec3::ZERO,
        cubes: vec![
            ModelCube {
                origin: Vec3::new(-5.0, -8.0, -4.0),
                size: Vec3::new(10.0, 8.0, 8.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            },
            ModelCube {
                origin: Vec3::new(-2.0, -4.0, -5.0),
                size: Vec3::new(4.0, 4.0, 1.0),
                tex_offset: (31, 1),
                deformation: 0.0,
                mirror: false,
            },
            ModelCube {
                origin: Vec3::new(2.0, -2.0, -5.0),
                size: Vec3::new(1.0, 2.0, 1.0),
                tex_offset: (2, 4),
                deformation: 0.0,
                mirror: false,
            },
            ModelCube {
                origin: Vec3::new(-3.0, -2.0, -5.0),
                size: Vec3::new(1.0, 2.0, 1.0),
                tex_offset: (2, 0),
                deformation: 0.0,
                mirror: false,
            },
        ],
        parent: None,
    };
    let left_ear = EntityPart {
        name: "left_ear".into(),
        offset: Vec3::new(4.5, -6.0, 0.0),
        default_rotation: Vec3::new(0.0, 0.0, -30.0_f32.to_radians()),
        cubes: vec![ModelCube {
            origin: Vec3::new(0.0, 0.0, -2.0),
            size: Vec3::new(1.0, 5.0, 4.0),
            tex_offset: (51, 6),
            deformation: 0.0,
            mirror: false,
        }],
        parent: Some(0),
    };
    let right_ear = EntityPart {
        name: "right_ear".into(),
        offset: Vec3::new(-4.5, -6.0, 0.0),
        default_rotation: Vec3::new(0.0, 0.0, 30.0_f32.to_radians()),
        cubes: vec![ModelCube {
            origin: Vec3::new(-1.0, 0.0, -2.0),
            size: Vec3::new(1.0, 5.0, 4.0),
            tex_offset: (39, 6),
            deformation: 0.0,
            mirror: false,
        }],
        parent: Some(0),
    };
    bake_model(vec![head, left_ear, right_ear], 64, 64)
}

/// Unknown skull blocks stay hidden rather than borrowing another type's mesh.
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
    fn decorated_pot_has_all_vanilla_parts_with_nonempty_uv_baked_geometry() {
        let model = bake_decorated_pot_model();
        assert_eq!(model.convention, ModelConvention::BlockYUp);
        assert_eq!(
            model
                .parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["neck", "top", "bottom", "front", "back", "left", "right"]
        );
        assert_eq!(
            model
                .part_ranges
                .iter()
                .map(|(_, count)| *count)
                .collect::<Vec<_>>(),
            [72, 6, 6, 6, 6, 6, 6]
        );
        let uv = crate::renderer::chunk::mesher::pack_uv;
        assert_eq!(
            model.vertices[84..90]
                .iter()
                .map(|v| v.tex_coords)
                .collect::<Vec<_>>(),
            [
                uv(15.0 / 16.0, 0.0),
                uv(1.0 / 16.0, 0.0),
                uv(1.0 / 16.0, 1.0),
                uv(15.0 / 16.0, 0.0),
                uv(1.0 / 16.0, 1.0),
                uv(15.0 / 16.0, 1.0)
            ]
        );
        assert!(model.vertices.iter().any(|v| v.position != [0.0; 3]));
    }

    #[test]
    fn enchanting_book_matches_the_seven_vanilla_book_parts_and_uv_sheet() {
        let model = bake_enchanting_book_model();
        assert_eq!(model.convention, ModelConvention::BlockYUp);
        assert_eq!(
            model
                .parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            [
                "left_lid",
                "right_lid",
                "seam",
                "left_pages",
                "right_pages",
                "flip_page1",
                "flip_page2"
            ]
        );
        assert_eq!(
            model.part_ranges,
            [
                (0, 36),
                (36, 36),
                (72, 36),
                (108, 36),
                (144, 36),
                (180, 36),
                (216, 36)
            ]
        );
        assert!(
            model
                .vertices
                .iter()
                .all(|v| (0..=65535).contains(&v.tex_coords[0])
                    && (0..=65535).contains(&v.tex_coords[1]))
        );
        assert!(model.vertices.iter().any(|v| v.position != [0.0; 3]));
    }

    #[test]
    fn bell_body_uses_the_verified_32_by_32_entity_sheet() {
        let model = bake_bell_model();
        assert_eq!(model.vertices.len(), 72);
        assert_eq!(model.part_ranges, [(0, 72)]);
        assert_eq!(model.convention, ModelConvention::EntityYDown);
        assert_eq!(model.parts[0].offset, Vec3::new(8.0, 12.0, 8.0));
        assert_eq!(model.parts[0].cubes[0].origin, Vec3::new(-3.0, -6.0, -3.0));
        assert_eq!(model.parts[0].cubes[0].size, Vec3::new(6.0, 7.0, 6.0));
        assert_eq!(model.parts[0].cubes[1].origin, Vec3::new(-8.0, -12.0, -8.0));
        assert_eq!(model.parts[0].cubes[1].size, Vec3::new(8.0, 2.0, 8.0));
        assert_eq!(model.parts[0].cubes[1].tex_offset, (0, 13));
        assert!(model.vertices.iter().all(|v| {
            (0.0..=1.0).contains(&(v.tex_coords[0] as f32 / 65535.0))
                && (0.0..=1.0).contains(&(v.tex_coords[1] as f32 / 65535.0))
        }));
    }

    #[test]
    fn dragon_and_piglin_heads_keep_their_vanilla_parts_uvs_and_bounds() {
        let dragon = bake_dragon_head_model();
        assert_eq!(
            dragon
                .parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["head", "jaw"]
        );
        assert_eq!(dragon.parts[1].parent, Some(0));
        assert_eq!(dragon.parts[1].cubes[0].size, Vec3::new(12.0, 4.0, 16.0));
        assert_eq!(dragon.parts[1].cubes[0].tex_offset, (176, 65));
        assert_eq!(dragon.part_scales[0], 0.75);
        assert_eq!(dragon.part_ranges, [(0, 216), (216, 36)]);
        assert!(dragon.vertices.iter().all(|v| {
            (0.0..=1.0).contains(&(v.tex_coords[0] as f32 / 65535.0))
                && (0.0..=1.0).contains(&(v.tex_coords[1] as f32 / 65535.0))
        }));

        let piglin = bake_piglin_head_model();
        assert_eq!(
            piglin
                .parts
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["head", "left_ear", "right_ear"]
        );
        assert_eq!(piglin.parts[0].cubes[0].size, Vec3::new(10.0, 8.0, 8.0));
        assert_eq!(piglin.parts[1].cubes[0].size, Vec3::new(1.0, 5.0, 4.0));
        assert_eq!(piglin.parts[2].cubes[0].size, Vec3::new(1.0, 5.0, 4.0));
        assert_eq!(piglin.parts[1].parent, Some(0));
        assert_eq!(piglin.parts[2].parent, Some(0));
        assert_eq!(piglin.part_ranges, [(0, 144), (144, 36), (180, 36)]);
        assert!(piglin.vertices.iter().all(|v| {
            (0.0..=1.0).contains(&(v.tex_coords[0] as f32 / 65535.0))
                && (0.0..=1.0).contains(&(v.tex_coords[1] as f32 / 65535.0))
        }));
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
