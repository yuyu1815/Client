use glam::{Mat4, Vec3};

use super::entity_model::{
    PLAYER_CAPE_TEXTURE_SIZE, PartAnim, bake_player_cape_model, player_cape_attachment_matrix,
};

#[test]
fn cape_uv_uses_vanilla_effective_height_without_changing_image_size() {
    let model = bake_player_cape_model();
    let v_min = model
        .vertices
        .iter()
        .map(|vertex| vertex.tex_coords[1])
        .min()
        .unwrap();
    let v_max = model
        .vertices
        .iter()
        .map(|vertex| vertex.tex_coords[1])
        .max()
        .unwrap();

    assert_eq!(PLAYER_CAPE_TEXTURE_SIZE, (64, 64));
    assert_eq!(v_min, 0);
    assert_eq!(
        v_max,
        crate::renderer::chunk::mesher::pack_uv(0.0, 17.0 / 32.0)[1]
    );
}

#[test]
fn cape_v_uses_the_same_normalized_uv_with_legacy_and_modern_image_heights() {
    let model = bake_player_cape_model();
    let max_v = model
        .vertices
        .iter()
        .map(|vertex| vertex.tex_coords[1])
        .max()
        .unwrap() as f32
        / u16::MAX as f32;

    for (image_width, image_height, expected_v_texel) in [(64, 32, 17.0), (64, 64, 34.0)] {
        assert_eq!(image_width, PLAYER_CAPE_TEXTURE_SIZE.0);
        assert!((max_v * image_height as f32 - expected_v_texel).abs() < 0.001);
    }
}

#[test]
fn cape_attachment_keeps_the_existing_body_parent_transform() {
    let model = bake_player_cape_model();
    let body = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
    let expected = body * model.compute_part_transforms(&PartAnim::default())[0];
    assert_eq!(
        player_cape_attachment_matrix(&model, body, &PartAnim::default()),
        expected
    );
}
