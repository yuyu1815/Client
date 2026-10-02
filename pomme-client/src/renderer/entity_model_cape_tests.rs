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
fn cape_attachment_keeps_the_existing_body_parent_transform() {
    let model = bake_player_cape_model();
    let body = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
    let expected = body * model.compute_part_transforms(&PartAnim::default())[0];
    assert_eq!(
        player_cape_attachment_matrix(&model, body, &PartAnim::default()),
        expected
    );
}
