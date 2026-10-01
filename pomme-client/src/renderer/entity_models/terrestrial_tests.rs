use super::*;

#[test]
fn terrestrial_bakes_have_geometry_and_in_atlas_uv_origins() {
    let cases: [(BakedEntityModel, i32, i32); 15] = [
        (bake_armadillo_model(), 64, 64),
        (bake_camel_model(), 128, 128),
        (bake_camel_husk_model(), 128, 128),
        (bake_fox_model(), 48, 32),
        (bake_frog_model(), 48, 48),
        (bake_goat_model(), 64, 64),
        (bake_hoglin_model(), 128, 64),
        (bake_zoglin_model(), 128, 64),
        (bake_mooshroom_mushrooms_model(), 64, 32),
        (bake_panda_model(), 64, 64),
        (bake_polar_bear_model(), 128, 64),
        (bake_ravager_model(), 128, 128),
        (bake_sniffer_model(), 192, 192),
        (bake_strider_model(), 64, 128),
        (bake_llama_model(false), 128, 64),
    ];
    for (model, width, height) in cases {
        assert!(!model.vertices.is_empty());
        for part in model.parts {
            for cube in part.cubes {
                assert!(cube.tex_offset.0 >= 0 && cube.tex_offset.0 < width);
                assert!(cube.tex_offset.1 >= 0 && cube.tex_offset.1 < height);
            }
        }
    }
    assert!(!bake_llama_model(true).vertices.is_empty());
}
