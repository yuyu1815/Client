use super::*;

fn verify(model: BakedEntityModel, width: u32, height: u32, parts: usize, cubes: usize) {
    assert_eq!(model.parts.len(), parts);
    assert!(!model.vertices.is_empty());
    let mut actual_cubes = 0;
    for part in &model.parts {
        actual_cubes += part.cubes.len();
        for cube in &part.cubes {
            assert!(cube.tex_offset.0 >= 0 && cube.tex_offset.0 < width as i32);
            assert!(cube.tex_offset.1 >= 0 && cube.tex_offset.1 < height as i32);
        }
    }
    assert_eq!(actual_cubes, cubes);
}

#[test]
fn mapped_26_2_layer_bakes_have_source_part_counts_and_atlas_uvs() {
    let cases: [(BakedEntityModel, u32, u32, usize, usize); 23] = [
        (bake_armadillo_model(), 64, 64, 13, 11),
        (bake_baby_armadillo_model(), 64, 64, 12, 11),
        (bake_camel_model(), 128, 128, 10, 12),
        (bake_baby_camel_model(), 64, 64, 9, 11),
        (bake_fox_model(), 48, 32, 10, 10),
        (bake_baby_fox_model(), 32, 32, 7, 10),
        (bake_frog_model(), 48, 48, 16, 16),
        (bake_goat_model(), 64, 64, 9, 12),
        (bake_baby_goat_model(), 64, 64, 11, 12),
        (bake_hoglin_model(), 128, 64, 11, 11),
        (bake_baby_hoglin_model(), 64, 64, 8, 11),
        (bake_panda_model(), 64, 64, 6, 9),
        (bake_baby_panda_model(), 64, 64, 6, 9),
        (bake_polar_bear_model(), 128, 64, 6, 10),
        (bake_baby_polar_bear_model(), 64, 64, 6, 9),
        (bake_ravager_model(), 128, 128, 10, 12),
        (bake_sniffer_model(), 192, 192, 13, 15),
        (bake_baby_sniffer_model(), 128, 128, 13, 15),
        (bake_strider_model(), 64, 128, 9, 9),
        (bake_baby_strider_model(), 32, 32, 6, 6),
        (bake_llama_model(), 128, 64, 8, 11),
        (bake_baby_llama_model(), 64, 64, 8, 11),
        (bake_llama_decor_model(), 128, 64, 8, 11),
    ];
    for (model, width, height, parts, cubes) in cases {
        verify(model, width, height, parts, cubes);
    }
    verify(bake_baby_llama_decor_model(), 64, 64, 8, 11);
    verify(bake_camel_husk_model(), 128, 128, 10, 12);
    verify(bake_zoglin_model(), 128, 64, 11, 11);
    verify(bake_baby_zoglin_model(), 64, 64, 8, 11);
}

#[test]
fn llama_decoration_layers_use_vanilla_cube_deformation_values() {
    assert!(
        bake_llama_model()
            .parts
            .iter()
            .flat_map(|p| &p.cubes)
            .all(|c| c.deformation == 0.0)
    );
    assert!(
        bake_llama_decor_model()
            .parts
            .iter()
            .flat_map(|p| &p.cubes)
            .all(|c| c.deformation == 0.5)
    );
    assert!(
        bake_baby_llama_decor_model()
            .parts
            .iter()
            .flat_map(|p| &p.cubes)
            .all(|c| c.deformation == 0.2)
    );
}
