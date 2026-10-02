use glam::Vec3;

use super::entity_model::{PartAnim, bake_player_model};

#[test]
fn player_overlay_meshes_match_native_layers_and_follow_their_bones() {
    for slim in [false, true] {
        let model = bake_player_model(slim);
        assert_eq!(model.parts.len(), 12);
        let find = |name: &str| {
            model
                .parts
                .iter()
                .position(|part| part.name == name)
                .unwrap()
        };
        for (name, parent, uv, deform) in [
            ("hat", "head", (32, 0), 0.5),
            ("jacket", "body", (16, 32), 0.25),
            ("right_sleeve", "right_arm", (40, 32), 0.25),
            ("left_sleeve", "left_arm", (48, 48), 0.25),
            ("right_pants", "right_leg", (0, 32), 0.25),
            ("left_pants", "left_leg", (0, 48), 0.25),
        ] {
            let i = find(name);
            let part = &model.parts[i];
            assert_eq!(part.parent, Some(find(parent)), "{name}");
            assert_eq!(part.cubes[0].tex_offset, uv, "{name}");
            assert_eq!(part.cubes[0].deformation, deform, "{name}");
            assert_eq!(part.offset, Vec3::ZERO, "{name}");
        }
        assert_eq!(model.parts[find("right_arm")].cubes[0].tex_offset, (40, 16));
        assert_eq!(model.parts[find("left_arm")].cubes[0].tex_offset, (32, 48));
        assert_eq!(
            model.parts[find("right_arm")].cubes[0].size.x,
            if slim { 3.0 } else { 4.0 }
        );

        for rotation in [
            Vec3::ZERO,
            Vec3::new(0.4, -0.7, 0.2),
            Vec3::new(-1.1, 0.3, 0.8),
        ] {
            let anim = PartAnim {
                rotation: (0..6).map(|i| (i, rotation)).collect(),
                ..Default::default()
            };
            let transforms = model.compute_part_transforms(&anim);
            for (overlay, parent) in [
                ("hat", "head"),
                ("jacket", "body"),
                ("right_sleeve", "right_arm"),
                ("left_sleeve", "left_arm"),
                ("right_pants", "right_leg"),
                ("left_pants", "left_leg"),
            ] {
                assert_eq!(
                    transforms[find(overlay)],
                    transforms[find(parent)],
                    "{overlay}"
                );
            }
        }
    }
}

#[test]
fn skin_bits_only_control_overlay_parts() {
    use azalea_registry::builtin::EntityKind;

    use super::pipelines::entity_renderer::player_model_part_visible as visible;

    let overlays = [
        ("jacket", 2),
        ("left_sleeve", 4),
        ("right_sleeve", 8),
        ("left_pants", 16),
        ("right_pants", 32),
        ("hat", 64),
    ];
    for mask in 0..=127 {
        for base in [
            "head",
            "body",
            "right_arm",
            "left_arm",
            "right_leg",
            "left_leg",
        ] {
            assert!(visible(EntityKind::Player, base, mask));
        }
        for (part, bit) in overlays {
            assert_eq!(visible(EntityKind::Player, part, mask), mask & bit != 0);
        }
    }
    assert!(visible(EntityKind::Zombie, "hat", 0));
    assert!(!visible(EntityKind::Mannequin, "hat", 0));
    assert!(visible(EntityKind::Mannequin, "hat", 64));
}
