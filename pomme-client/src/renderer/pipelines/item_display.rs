use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};

use crate::world::block::model::{first_item_model_ref, parse_vec3, strip_mc_prefix};

const MODEL_PARENT_LIMIT: u32 = 16;

#[derive(Debug, Clone, Copy)]
pub struct DisplayTransform {
    pub rotation: Vec3,
    pub translation: Vec3,
    pub scale: Vec3,
}

impl DisplayTransform {
    pub const IDENTITY: Self = Self {
        rotation: Vec3::ZERO,
        translation: Vec3::ZERO,
        scale: Vec3::ONE,
    };

    pub fn to_matrix(self) -> Mat4 {
        let t = Mat4::from_translation(self.translation);
        let r = Mat4::from_rotation_x(self.rotation.x.to_radians())
            * Mat4::from_rotation_y(self.rotation.y.to_radians())
            * Mat4::from_rotation_z(self.rotation.z.to_radians());
        let s = Mat4::from_scale(self.scale);
        t * r * s
    }
}

/// Per-item cache of one `display.<key>` transform, resolved from the item's
/// model JSON parent chain.
pub struct DisplayResolver {
    key: &'static str,
    cache: RefCell<HashMap<String, DisplayTransform>>,
    jar_assets_dir: PathBuf,
    asset_index: Option<crate::assets::AssetIndex>,
    pack_dirs: Vec<PathBuf>,
}

impl DisplayResolver {
    pub fn new(jar_assets_dir: &Path, key: &'static str) -> Self {
        Self {
            key,
            cache: RefCell::new(HashMap::new()),
            jar_assets_dir: jar_assets_dir.to_path_buf(),
            asset_index: None,
            pack_dirs: Vec::new(),
        }
    }

    pub fn update_resources(
        &mut self,
        jar_assets_dir: &Path,
        asset_index: &Option<crate::assets::AssetIndex>,
        pack_dirs: &[PathBuf],
    ) {
        self.jar_assets_dir = jar_assets_dir.to_path_buf();
        self.asset_index = asset_index.clone();
        self.pack_dirs = pack_dirs.to_vec();
        self.clear_cache();
    }

    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    pub fn resolve(&self, item_name: &str, default: DisplayTransform) -> DisplayTransform {
        if let Some(t) = self.cache.borrow().get(item_name) {
            return *t;
        }
        let resolved = resolve_item_model_path(
            item_name,
            &self.jar_assets_dir,
            &self.asset_index,
            &self.pack_dirs,
        )
        .and_then(|path| {
            resolve_display(
                &path,
                &self.jar_assets_dir,
                &self.asset_index,
                &self.pack_dirs,
                self.key,
            )
        })
        .unwrap_or(default);
        self.cache
            .borrow_mut()
            .insert(item_name.to_string(), resolved);
        resolved
    }
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn resolve_item_model_path(
    name: &str,
    jar: &Path,
    index: &Option<crate::assets::AssetIndex>,
    packs: &[PathBuf],
) -> Option<String> {
    let key = crate::assets::AssetId::parse(name).asset_key("items", ".json");
    let file = crate::assets::resolve_asset_path_with_pack_dirs(jar, index, &key, packs);
    let item_json = read_json(&file)?;
    first_item_model_ref(&item_json)
}

fn parse_display_transform(json: &serde_json::Value) -> Option<DisplayTransform> {
    let obj = json.as_object()?;
    let rotation = obj
        .get("rotation")
        .map(|v| parse_vec3(v, Vec3::ZERO))
        .unwrap_or(Vec3::ZERO);
    let translation = obj
        .get("translation")
        .map(|v| parse_vec3(v, Vec3::ZERO))
        .unwrap_or(Vec3::ZERO);
    let scale = obj
        .get("scale")
        .map(|v| parse_vec3(v, Vec3::ONE))
        .unwrap_or(Vec3::ONE);
    Some(DisplayTransform {
        rotation,
        translation: translation * (1.0 / 16.0),
        scale,
    })
}

/// First `display.<key>` transform found walking up the model parent chain.
fn resolve_display(
    start_path: &str,
    jar: &Path,
    index: &Option<crate::assets::AssetIndex>,
    packs: &[PathBuf],
    key: &str,
) -> Option<DisplayTransform> {
    let mut current = Some(start_path.to_string());
    let mut depth = 0u32;
    while let Some(path) = current.take() {
        if depth >= MODEL_PARENT_LIMIT {
            break;
        }
        depth += 1;

        let asset_key = crate::assets::AssetId::parse(&path).asset_key("models", ".json");
        let file = crate::assets::resolve_asset_path_with_pack_dirs(jar, index, &asset_key, packs);
        let json = read_json(&file)?;

        if let Some(entry) = json.get("display").and_then(|d| d.get(key))
            && let Some(t) = parse_display_transform(entry)
        {
            return Some(t);
        }

        current = json
            .get("parent")
            .and_then(|p| p.as_str())
            .map(|p| strip_mc_prefix(p).to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, key: &str, json: &str) {
        let path = root.join("assets").join(key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json).unwrap();
    }

    fn model(root: &Path, name: &str, body: &str) {
        write(root, &format!("minecraft/models/item/{name}.json"), body);
    }

    fn scale(resolver: &DisplayResolver) -> f32 {
        resolver
            .resolve("totem_of_undying", DisplayTransform::IDENTITY)
            .scale
            .x
    }

    #[test]
    fn reload_uses_pack_item_parent_priority_and_recreated_context() {
        let root = std::env::temp_dir().join(format!("display-reload-{}", uuid::Uuid::new_v4()));
        let jar = root.join("jar");
        let low = root.join("low");
        let high = root.join("high");
        write(
            &jar,
            "minecraft/items/totem_of_undying.json",
            r#"{"model":"minecraft:item/base"}"#,
        );
        model(&jar, "base", r#"{"display":{"fixed":{"scale":[1,1,1]}}}"#);
        let mut resolver = DisplayResolver::new(&jar.join("assets"), "fixed");
        assert_eq!(scale(&resolver), 1.0);

        write(
            &low,
            "minecraft/items/totem_of_undying.json",
            r#"{"model":"minecraft:item/child"}"#,
        );
        model(&low, "child", r#"{"parent":"minecraft:item/parent"}"#);
        model(&low, "parent", r#"{"display":{"fixed":{"scale":[2,2,2]}}}"#);
        model(&high, "parent", "{ invalid json");
        resolver.update_resources(&jar.join("assets"), &None, &[low.clone(), high.clone()]);
        assert_eq!(
            scale(&resolver),
            1.0,
            "invalid winning JSON does not fall through"
        );

        model(
            &high,
            "parent",
            r#"{"display":{"fixed":{"scale":[3,3,3]}}}"#,
        );
        resolver.update_resources(&jar.join("assets"), &None, &[low.clone(), high.clone()]);
        assert_eq!(scale(&resolver), 3.0, "higher-priority parent model wins");

        model(
            &high,
            "parent",
            r#"{"display":{"fixed":{"scale":[4,4,4]}}}"#,
        );
        resolver.update_resources(&jar.join("assets"), &None, &[low.clone(), high.clone()]);
        assert_eq!(
            scale(&resolver),
            4.0,
            "same-directory content changes clear cache"
        );

        resolver.update_resources(&jar.join("assets"), &None, &[low.clone()]);
        assert_eq!(
            scale(&resolver),
            2.0,
            "removing high-priority pack reveals lower pack"
        );
        resolver.update_resources(&jar.join("assets"), &None, &[]);
        assert_eq!(scale(&resolver), 1.0, "removing packs returns to base");

        let mut recreated = DisplayResolver::new(&jar.join("assets"), "fixed");
        recreated.update_resources(&jar.join("assets"), &None, &[low, high]);
        assert_eq!(
            scale(&recreated),
            4.0,
            "new pipeline can receive retained context"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
