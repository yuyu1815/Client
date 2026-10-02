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
    #[serde(default)]
    pub color_when_undyed: Option<u32>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equipment_layers_and_texture_paths_follow_vanilla_identifiers() {
        let temp = std::env::temp_dir().join(format!("equipment-{}", uuid::Uuid::new_v4()));
        let definition = temp.join("assets/example/equipment/harness.json");
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
    fn malformed_equipment_json_is_unknown_and_safe() {
        let temp = std::env::temp_dir().join(format!("equipment-{}", uuid::Uuid::new_v4()));
        let path = temp.join("assets/minecraft/equipment/test.json");
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
