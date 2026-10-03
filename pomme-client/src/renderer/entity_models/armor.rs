use azalea_inventory::components::EquipmentSlot;
use glam::{Vec3, vec3};

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

/// Bake the vanilla 64x32 humanoid armor layer; `leggings` selects the inner
/// layer deformation used by the leggings texture.
pub fn bake_humanoid_armor_model(leggings: bool) -> BakedEntityModel {
    let d = if leggings { 0.5 } else { 1.0 };
    let cube = |origin, size, tex_offset, mirror| ModelCube {
        origin,
        size,
        tex_offset,
        deformation: d,
        mirror,
    };
    let part = |name: &str, offset, cubes| EntityPart {
        name: name.into(),
        offset,
        default_rotation: Vec3::ZERO,
        cubes,
        parent: None,
    };
    bake_model(
        vec![
            part(
                "head",
                Vec3::ZERO,
                vec![cube(vec3(-4., -8., -4.), vec3(8., 8., 8.), (0, 0), false)],
            ),
            part(
                "body",
                Vec3::ZERO,
                vec![cube(vec3(-4., 0., -2.), vec3(8., 12., 4.), (16, 16), false)],
            ),
            part(
                "right_arm",
                vec3(-5., 2., 0.),
                vec![cube(
                    vec3(-3., -2., -2.),
                    vec3(4., 12., 4.),
                    (40, 16),
                    false,
                )],
            ),
            part(
                "left_arm",
                vec3(5., 2., 0.),
                vec![cube(vec3(-1., -2., -2.), vec3(4., 12., 4.), (40, 16), true)],
            ),
            part(
                "right_leg",
                vec3(-1.9, 12., 0.),
                vec![cube(vec3(-2., 0., -2.), vec3(4., 12., 4.), (0, 16), false)],
            ),
            part(
                "left_leg",
                vec3(1.9, 12., 0.),
                vec![cube(vec3(-2., 0., -2.), vec3(4., 12., 4.), (0, 16), true)],
            ),
        ],
        64,
        32,
    )
}

/// Parts rendered for one armor equipment slot.
pub fn armor_part_names(slot: EquipmentSlot) -> &'static [&'static str] {
    match slot {
        EquipmentSlot::Head => &["head"],
        EquipmentSlot::Chest => &["body", "right_arm", "left_arm"],
        EquipmentSlot::Legs => &["body", "right_leg", "left_leg"],
        EquipmentSlot::Feet => &["right_leg", "left_leg"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::entity_model::PartAnim;

    #[test]
    fn armor_mesh_has_standard_parts_ranges_uvs_deformation_and_slot_visibility() {
        let outer = bake_humanoid_armor_model(false);
        let inner = bake_humanoid_armor_model(true);
        assert_eq!(outer.parts.len(), 6);
        assert_eq!(outer.vertices.len(), 216);
        assert_eq!(outer.part_ranges.len(), outer.parts.len());
        assert_ne!(outer.vertices, inner.vertices);
        for (i, part) in outer.parts.iter().enumerate() {
            assert_eq!(
                outer.part_ranges[i].0 as usize + outer.part_ranges[i].1 as usize,
                (i + 1) * 36
            );
            assert_eq!(outer.part_ranges[i].1, 36);
            assert_eq!(part.cubes.len(), 1);
        }
        let find = |model: &BakedEntityModel, name: &str| {
            model.parts.iter().position(|p| p.name == name).unwrap()
        };
        for (name, uv) in [
            ("head", (0, 0)),
            ("body", (16, 16)),
            ("right_arm", (40, 16)),
            ("left_arm", (40, 16)),
            ("right_leg", (0, 16)),
            ("left_leg", (0, 16)),
        ] {
            let i = find(&outer, name);
            assert_eq!(outer.parts[i].cubes[0].tex_offset, uv, "{name}");
            let (start, count) = outer.part_ranges[i];
            assert_eq!(count, 36);
            let vertices = &outer.vertices[start as usize..(start + count) as usize];
            assert!(
                vertices
                    .iter()
                    .all(|v| v.tex_coords[0] <= u16::MAX && v.tex_coords[1] <= u16::MAX),
                "{name} UV outside normalized 64x32 texture"
            );
            assert!(
                vertices.iter().any(|v| v.tex_coords != [0, 0]),
                "{name} UVs were not baked"
            );
            let inner_range = inner.part_ranges[i];
            let inner_vertices =
                &inner.vertices[inner_range.0 as usize..(inner_range.0 + inner_range.1) as usize];
            assert!(
                vertices
                    .iter()
                    .zip(inner_vertices)
                    .all(|(outer, inner)| outer.tex_coords == inner.tex_coords)
            );
        }
        assert_eq!(armor_part_names(EquipmentSlot::Head), &["head"]);
        assert_eq!(
            armor_part_names(EquipmentSlot::Chest),
            &["body", "right_arm", "left_arm"]
        );
        assert_eq!(
            armor_part_names(EquipmentSlot::Legs),
            &["body", "right_leg", "left_leg"]
        );
        assert_eq!(
            armor_part_names(EquipmentSlot::Feet),
            &["right_leg", "left_leg"]
        );
        assert!(armor_part_names(EquipmentSlot::Mainhand).is_empty());
        let outer_head = outer.parts[find(&outer, "head")].cubes[0];
        let inner_head = inner.parts[find(&inner, "head")].cubes[0];
        assert_eq!(outer_head.deformation, 1.0);
        assert_eq!(inner_head.deformation, 0.5);
        assert_eq!(outer_head.size, inner_head.size);
    }

    #[test]
    fn baked_vertices_follow_non_identity_head_and_arm_pose() {
        let model = bake_humanoid_armor_model(false);
        let head = model.parts.iter().position(|p| p.name == "head").unwrap();
        let arm = model
            .parts
            .iter()
            .position(|p| p.name == "right_arm")
            .unwrap();
        let anim = PartAnim {
            rotation: vec![
                (head, Vec3::new(0.4, -0.3, 0.2)),
                (arm, Vec3::new(-0.6, 0.1, 0.4)),
            ],
            ..Default::default()
        };
        let posed = model.compute_part_transforms(&anim);
        let base = model.compute_part_transforms(&PartAnim::default());
        for part in [head, arm] {
            let (start, count) = model.part_ranges[part];
            assert_eq!(count, 36);
            let vertex = glam::Vec3::from_array(model.vertices[start as usize].position);
            assert_ne!(
                posed[part].transform_point3(vertex),
                base[part].transform_point3(vertex)
            );
        }
    }
}
