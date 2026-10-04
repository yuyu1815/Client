use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::assets::{AssetId, AssetIndex, resolve_asset_path_with_packs};
use crate::resource_pack::ResourcePackManager;

const MAX_EQUIPMENT_JSON_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EquipmentLayer {
    pub texture: String,
    #[serde(default)]
    pub dyeable: Option<Dyeable>,
    #[serde(default)]
    pub use_player_texture: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Dyeable {
    #[serde(default, deserialize_with = "deserialize_rgb_color")]
    pub color_when_undyed: Option<i32>,
}

fn deserialize_rgb_color<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Color {
        Integer(i32),
        Components([f32; 3]),
    }
    Ok(
        Option::<Color>::deserialize(deserializer)?.map(|color| match color {
            Color::Integer(color) => color,
            Color::Components([r, g, b]) => {
                let channel = |value: f32| (value * 255.0).floor() as u32;
                ((channel(r) << 16) | (channel(g) << 8) | channel(b)) as i32
            }
        }),
    )
}

/// Frame-ready Happy Ghast layer input for the renderer/GPU owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedEquipmentLayer {
    pub texture_key: String,
    pub tint_rgb: [u8; 3],
}

/// Apply Native EquipmentLayerRenderer.getColorForLayer and resolve ordered
/// layer texture identifiers without loading GPU resources.
pub fn resolve_happy_ghast_layer_inputs(
    layers: &[EquipmentLayer],
    dyed_rgb: Option<i32>,
) -> Vec<ResolvedEquipmentLayer> {
    layers
        .iter()
        .filter_map(|layer| {
            let color = match &layer.dyeable {
                Some(dyeable) => {
                    let Some(color) = dyed_rgb.or(dyeable.color_when_undyed)
                    else {
                        return None;
                    };
                    (color as u32) | 0xff00_0000
                }
                None => u32::MAX,
            };
            Some(ResolvedEquipmentLayer {
                texture_key: equipment_layer_asset_key("happy_ghast_body", &layer.texture)?,
                tint_rgb: [(color >> 16) as u8, (color >> 8) as u8, color as u8],
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EquipmentDefinition {
    layers: std::collections::HashMap<String, Vec<EquipmentLayer>>,
}

/// Resolve one equipment asset layer list through the active resource-pack
/// stack. Unknown layer types and malformed/oversized definitions safely
/// produce None.
pub fn resolve_equipment_layers(
    jar_assets: &Path,
    index: &Option<AssetIndex>,
    packs: Option<&ResourcePackManager>,
    asset_id: &str,
    layer_type: &str,
) -> Option<Vec<EquipmentLayer>> {
    let id = AssetId::parse(asset_id);
    if !crate::assets::identifier_chars(id.namespace, false)
        || !crate::assets::identifier_chars(id.path, true)
        || id
            .path
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || !crate::assets::identifier_chars(layer_type, false)
    {
        return None;
    }
    let key = format!("{}/equipment/{}.json", id.namespace, id.path);
    let path = resolve_asset_path_with_packs(jar_assets, index, &key, packs);
    load_equipment_layers(&path, layer_type)
}

/// Resolve against Renderer’s active-pack snapshot for this frame.
pub fn resolve_equipment_layers_with_pack_dirs(
    jar_assets: &Path,
    index: &Option<AssetIndex>,
    pack_dirs: &[PathBuf],
    asset_id: &str,
    layer_type: &str,
) -> Option<Vec<EquipmentLayer>> {
    let id = AssetId::parse(asset_id);
    if !crate::assets::identifier_chars(id.namespace, false)
        || !crate::assets::identifier_chars(id.path, true)
        || id
            .path
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || !crate::assets::identifier_chars(layer_type, false)
    {
        return None;
    }
    let key = format!("{}/equipment/{}.json", id.namespace, id.path);
    let path = crate::assets::resolve_asset_path_with_pack_dirs(jar_assets, index, &key, pack_dirs);
    load_equipment_layers(&path, layer_type)
}

fn load_equipment_layers(path: &Path, layer_type: &str) -> Option<Vec<EquipmentLayer>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > MAX_EQUIPMENT_JSON_BYTES {
        return None;
    }
    let parsed: EquipmentDefinition = serde_json::from_slice(&bytes).ok()?;
    let layers = parsed.layers.get(layer_type)?.clone();
    (!layers.is_empty()).then_some(layers)
}

/// Vanilla EquipmentClientInfo.Layer.getTextureLocation: preserve texture
/// namespace, prepend type directory, and append .png.
pub fn equipment_layer_asset_key(layer_type: &str, texture: &str) -> Option<String> {
    let id = AssetId::parse(texture);
    if !crate::assets::identifier_chars(id.namespace, false)
        || !crate::assets::identifier_chars(id.path, true)
        || id
            .path
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || !crate::assets::identifier_chars(layer_type, false)
    {
        return None;
    }
    Some(format!(
        "{}/textures/entity/equipment/{layer_type}/{}.png",
        id.namespace, id.path
    ))
}

pub fn resolve_equipment_texture(
    jar_assets: &Path,
    index: &Option<AssetIndex>,
    packs: Option<&ResourcePackManager>,
    layer_type: &str,
    texture: &str,
) -> Option<PathBuf> {
    let key = equipment_layer_asset_key(layer_type, texture)?;
    let path = resolve_asset_path_with_packs(jar_assets, index, &key, packs);
    path.is_file().then_some(path)
}

/// Resolve the ordered humanoid texture layers for one equipped item.
pub fn resolve_humanoid_equipment_layers(
    slot: azalea_inventory::components::EquipmentSlot,
    stack: &azalea_inventory::ItemStack,
    jar_assets: &Path,
    index: &Option<AssetIndex>,
    pack_dirs: &[PathBuf],
) -> Vec<ResolvedEquipmentLayer> {
    let azalea_inventory::ItemStack::Present(stack) = stack else {
        return Vec::new();
    };
    let layer_type = match slot {
        azalea_inventory::components::EquipmentSlot::Head
        | azalea_inventory::components::EquipmentSlot::Chest
        | azalea_inventory::components::EquipmentSlot::Feet => "humanoid",
        azalea_inventory::components::EquipmentSlot::Legs => "humanoid_leggings",
        _ => return Vec::new(),
    };
    let Some(equippable) =
        crate::player::menu_click::component::<azalea_inventory::components::Equippable>(stack)
    else {
        return Vec::new();
    };
    if equippable.slot != slot {
        return Vec::new();
    }
    let Some(asset_id) = equippable.asset_id.map(|id| id.to_string()) else {
        return Vec::new();
    };
    let Some(layers) = resolve_equipment_layers_with_pack_dirs(
        jar_assets, index, pack_dirs, &asset_id, layer_type,
    ) else {
        return Vec::new();
    };
    let dyed_color =
        crate::player::menu_click::component::<azalea_inventory::components::DyedColor>(stack)
            .map(|color| color.rgb as i32);
    layers
        .iter()
        .filter_map(|layer| {
            let tint = match &layer.dyeable {
                Some(dyeable) => {
                    let Some(color) = dyed_color.or(dyeable.color_when_undyed) else {
                        return None;
                    };
                    color as u32
                }
                None => u32::MAX,
            };
            Some(ResolvedEquipmentLayer {
                texture_key: equipment_layer_asset_key(layer_type, &layer.texture)?,
                tint_rgb: [(tint >> 16) as u8, (tint >> 8) as u8, tint as u8],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_iron_helmet_resolves_vanilla_humanoid_texture() {
        use azalea_inventory::components::EquipmentSlot;
        use azalea_inventory::{ItemStack, ItemStackData};
        use azalea_registry::builtin::ItemKind;

        let temp =
            std::env::temp_dir().join(format!("equipment-standard-{}", uuid::Uuid::new_v4()));
        let definition = temp.join("minecraft/equipment/iron.json");
        std::fs::create_dir_all(definition.parent().unwrap()).unwrap();
        std::fs::write(
            &definition,
            r#"{"layers":{"humanoid":[{"texture":"minecraft:iron"}],"humanoid_leggings":[{"texture":"minecraft:iron"}]}}"#,
        )
        .unwrap();
        let helmet = ItemStack::Present(ItemStackData::new(ItemKind::IronHelmet, 1));
        let layers =
            resolve_humanoid_equipment_layers(EquipmentSlot::Head, &helmet, &temp, &None, &[]);
        assert_eq!(layers.len(), 1);
        assert_eq!(
            layers[0].texture_key,
            "minecraft/textures/entity/equipment/humanoid/iron.png"
        );
        assert_eq!(layers[0].tint_rgb, [255, 255, 255]);
        let leggings = ItemStack::Present(ItemStackData::new(ItemKind::IronLeggings, 1));
        let layers =
            resolve_humanoid_equipment_layers(EquipmentSlot::Legs, &leggings, &temp, &None, &[]);
        assert_eq!(
            layers[0].texture_key,
            "minecraft/textures/entity/equipment/humanoid_leggings/iron.png"
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn custom_asset_keeps_order_tint_and_highest_pack_priority() {
        use azalea_inventory::components::{DyedColor, EquipmentSlot, Equippable};
        use azalea_inventory::{ItemStack, ItemStackData};
        use azalea_registry::builtin::{DataComponentKind, ItemKind};

        let temp = std::env::temp_dir().join(format!("equipment-custom-{}", uuid::Uuid::new_v4()));
        let jar = temp.join("jar");
        let pack_a = temp.join("pack-a");
        let pack_b = temp.join("pack-b");
        let write = |root: &Path, texture: &str| {
            let path = root.join("assets/custom/equipment/armor.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!(r#"{{"layers":{{"humanoid":[{{"texture":"{texture}","dyeable":{{"color_when_undyed":1122867}}}},{{"texture":"other:plain"}}]}}}}"#)).unwrap();
        };
        write(&jar, "other:jar");
        write(&pack_a, "other:pack_a");
        write(&pack_b, "other:pack_b");
        let mut data = ItemStackData::new(ItemKind::LeatherChestplate, 1);
        let mut equippable = Equippable::new();
        equippable.slot = EquipmentSlot::Chest;
        equippable.asset_id = Some("custom:armor".parse().unwrap());
        unsafe {
            data.component_patch
                .unchecked_insert_component(DataComponentKind::Equippable, Some(equippable.into()));
            data.component_patch.unchecked_insert_component(
                DataComponentKind::DyedColor,
                Some(DyedColor { rgb: 0x123456 }.into()),
            );
        }
        let stack = ItemStack::Present(data);
        let layers = resolve_humanoid_equipment_layers(
            EquipmentSlot::Chest,
            &stack,
            &jar,
            &None,
            &[pack_a, pack_b],
        );
        assert_eq!(layers.len(), 2);
        assert_eq!(
            layers[0].texture_key,
            "other/textures/entity/equipment/humanoid/pack_b.png"
        );
        assert_eq!(layers[0].tint_rgb, [0x12, 0x34, 0x56]);
        assert_eq!(
            layers[1].texture_key,
            "other/textures/entity/equipment/humanoid/plain.png"
        );
        assert_eq!(layers[1].tint_rgb, [255, 255, 255]);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn explicit_equippable_removal_slot_mismatch_and_empty_stack_resolve_nothing() {
        use azalea_inventory::components::{EquipmentSlot, Equippable};
        use azalea_inventory::{ItemStack, ItemStackData};
        use azalea_registry::builtin::{DataComponentKind, ItemKind};

        let temp = std::env::temp_dir().join(format!("equipment-empty-{}", uuid::Uuid::new_v4()));
        let mut data = ItemStackData::new(ItemKind::IronHelmet, 1);
        let mut wrong_slot = Equippable::new();
        wrong_slot.slot = EquipmentSlot::Chest;
        unsafe {
            data.component_patch
                .unchecked_insert_component(DataComponentKind::Equippable, Some(wrong_slot.into()));
        }
        assert!(
            resolve_humanoid_equipment_layers(
                EquipmentSlot::Head,
                &ItemStack::Present(data.clone()),
                &temp,
                &None,
                &[]
            )
            .is_empty()
        );
        let mut no_asset = Equippable::new();
        no_asset.slot = EquipmentSlot::Head;
        unsafe {
            data.component_patch
                .unchecked_insert_component(DataComponentKind::Equippable, Some(no_asset.into()));
        }
        assert!(
            resolve_humanoid_equipment_layers(
                EquipmentSlot::Head,
                &ItemStack::Present(data.clone()),
                &temp,
                &None,
                &[]
            )
            .is_empty()
        );
        unsafe {
            data.component_patch
                .unchecked_insert_component(DataComponentKind::Equippable, None);
        }
        assert!(
            resolve_humanoid_equipment_layers(
                EquipmentSlot::Head,
                &ItemStack::Present(data),
                &temp,
                &None,
                &[]
            )
            .is_empty()
        );
        assert!(
            resolve_humanoid_equipment_layers(
                EquipmentSlot::Head,
                &ItemStack::Empty,
                &temp,
                &None,
                &[]
            )
            .is_empty()
        );
        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn equipment_layers_and_texture_paths_follow_vanilla_identifiers() {
        let temp = std::env::temp_dir().join(format!("equipment-{}", uuid::Uuid::new_v4()));
        let definition = temp.join("example/equipment/harness.json");
        std::fs::create_dir_all(definition.parent().unwrap()).unwrap();
        std::fs::write(
            &definition,
            r#"{"layers":{"happy_ghast_body":[{"texture":"example:entity/harness","dyeable":{"color_when_undyed":123},"use_player_texture":false}]}}"#,
        ).unwrap();
        let layers =
            resolve_equipment_layers(&temp, &None, None, "example:harness", "happy_ghast_body")
                .unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(
            layers[0].dyeable.as_ref().unwrap().color_when_undyed,
            Some(123)
        );
        assert_eq!(
            equipment_layer_asset_key("happy_ghast_body", &layers[0].texture).as_deref(),
            Some("example/textures/entity/equipment/happy_ghast_body/entity/harness.png")
        );
        assert!(equipment_layer_asset_key("happy_ghast_body", "../outside").is_none());
        assert!(
            resolve_equipment_layers(&temp, &None, None, "example:../outside", "happy_ghast_body")
                .is_none()
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn native_cape_predicate_uses_each_custom_assets_layer_list() {
        let temp = std::env::temp_dir().join(format!("equipment-cape-{}", uuid::Uuid::new_v4()));
        let path = temp.join("custom/equipment/cape.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let definition = |layers: &str| format!(r#"{{"layers":{{{layers}}}}}"#);
        std::fs::write(
            &path,
            definition(
                r#""wings":[{"texture":"custom:wings"}],"humanoid":[{"texture":"custom:body"}]"#,
            ),
        )
        .unwrap();
        let resolve = |kind| resolve_equipment_layers(&temp, &None, None, "custom:cape", kind);
        assert!(resolve("wings").is_some());
        assert!(resolve("humanoid").is_some());
        assert!(resolve("unknown").is_none());

        std::fs::write(&path, definition(r#""other":[{"texture":"custom:other"}]"#)).unwrap();
        assert!(resolve("wings").is_none());
        assert!(resolve("humanoid").is_none());
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn frame_resolver_uses_pack_override_and_observes_reload_paths() {
        let temp = std::env::temp_dir().join(format!("equipment-pack-{}", uuid::Uuid::new_v4()));
        let jar = temp.join("jar");
        let pack = temp.join("pack");
        let write = |path: &Path, texture: &str| {
            let path = path.join("example/equipment/harness.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                path,
                format!(r#"{{"layers":{{"happy_ghast_body":[{{"texture":"{texture}"}}]}}}}"#),
            )
            .unwrap();
        };
        write(&jar, "example:default");
        write(&pack.join("assets"), "custom:override");
        let base = resolve_equipment_layers_with_pack_dirs(
            &jar,
            &None,
            &[],
            "example:harness",
            "happy_ghast_body",
        )
        .unwrap();
        let overridden = resolve_equipment_layers_with_pack_dirs(
            &jar,
            &None,
            std::slice::from_ref(&pack),
            "example:harness",
            "happy_ghast_body",
        )
        .unwrap();
        assert_eq!(base[0].texture, "example:default");
        assert_eq!(overridden[0].texture, "custom:override");
        write(&pack.join("assets"), "custom:reloaded");
        let reloaded = resolve_equipment_layers_with_pack_dirs(
            &jar,
            &None,
            std::slice::from_ref(&pack),
            "example:harness",
            "happy_ghast_body",
        )
        .unwrap();
        assert_eq!(reloaded[0].texture, "custom:reloaded");
        assert!(resolve_happy_ghast_layer_inputs(&[], None).is_empty());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn happy_ghast_layer_inputs_keep_order_and_native_tints() {
        let colors = [
            0x000000, 0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0x123456, 0x654321, 0xabcdef,
            0x010203, 0x102030, 0x203040, 0x304050, 0x405060, 0x506070, 0x607080, 0x708090,
        ];
        let mut layers: Vec<_> = colors
            .iter()
            .map(|color| EquipmentLayer {
                texture: format!("custom:body/{color:06x}"),
                dyeable: Some(Dyeable {
                    color_when_undyed: Some(*color),
                }),
                use_player_texture: false,
            })
            .collect();
        layers.push(EquipmentLayer {
            texture: "minecraft:body/white".into(),
            dyeable: None,
            use_player_texture: false,
        });
        let resolved = resolve_happy_ghast_layer_inputs(&layers, None);
        assert_eq!(
            resolved.len(),
            17,
            "configured RGB black remains renderable"
        );
        let expected: Vec<_> = colors
            .iter()
            .map(|color| [(*color >> 16) as u8, (*color >> 8) as u8, *color as u8])
            .chain([[255, 255, 255]])
            .collect();
        assert_eq!(
            resolved
                .iter()
                .map(|layer| layer.tint_rgb)
                .collect::<Vec<_>>(),
            expected,
        );
        assert_eq!(resolved[0].tint_rgb, [0, 0, 0]);
        assert_eq!(resolved[1].tint_rgb, [255, 255, 255]);
        assert_eq!(
            resolved[1].texture_key,
            "custom/textures/entity/equipment/happy_ghast_body/body/ffffff.png"
        );
        assert_eq!(resolved[16].tint_rgb, [255, 255, 255]);
        assert_eq!(
            resolve_happy_ghast_layer_inputs(&layers[1..2], Some(0x123456))[0].tint_rgb,
            [0x12, 0x34, 0x56],
        );
        assert_eq!(
            resolve_happy_ghast_layer_inputs(&layers[0..1], None)[0].tint_rgb,
            [0, 0, 0]
        );
        assert_eq!(
            resolve_happy_ghast_layer_inputs(&layers[0..1], Some(0))[0].tint_rgb,
            [0, 0, 0],
            "explicit black dye overrides the undyed default"
        );
        let no_default = [EquipmentLayer {
            texture: "custom:no-default".into(),
            dyeable: Some(Dyeable {
                color_when_undyed: None,
            }),
            use_player_texture: false,
        }];
        assert!(resolve_happy_ghast_layer_inputs(&no_default, None).is_empty());
    }

    #[test]
    fn rgb_color_accepts_signed_integer_and_float_components() {
        let negative: EquipmentLayer = serde_json::from_str(
            r#"{"texture":"minecraft:leather","dyeable":{"color_when_undyed":-6265536}}"#,
        )
        .unwrap();
        assert_eq!(negative.dyeable.unwrap().color_when_undyed, Some(-6265536));
        let floats: EquipmentLayer = serde_json::from_str(
            r#"{"texture":"minecraft:leather","dyeable":{"color_when_undyed":[1.0,0.5,0.0]}}"#,
        )
        .unwrap();
        assert_eq!(floats.dyeable.unwrap().color_when_undyed, Some(0x00ff_7f00));
        let positive: EquipmentLayer = serde_json::from_str(
            r#"{"texture":"minecraft:leather","dyeable":{"color_when_undyed":1122867}}"#,
        )
        .unwrap();
        assert_eq!(positive.dyeable.unwrap().color_when_undyed, Some(1122867));
    }

    #[test]
    fn malformed_equipment_json_is_unknown_and_safe() {
        let temp = std::env::temp_dir().join(format!("equipment-{}", uuid::Uuid::new_v4()));
        let path = temp.join("minecraft/equipment/test.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"layers":{"happy_ghast_body":[{"texture":"a","unexpected":true}]}}"#,
        )
        .unwrap();
        assert!(resolve_equipment_layers(&temp, &None, None, "test", "happy_ghast_body").is_none());
        std::fs::remove_dir_all(temp).unwrap();
    }
}
