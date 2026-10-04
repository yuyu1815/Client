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

    pub fn to_left_hand_matrix(mut self) -> Mat4 {
        self.translation.x = -self.translation.x;
        self.rotation.y = -self.rotation.y;
        self.rotation.z = -self.rotation.z;
        self.to_matrix()
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

    /// Resolve a concrete model (used by condition-selected special item
    /// display bases such as shield_blocking) through the same pack/cache path.
    pub fn resolve_model_path(
        &self,
        model_path: &str,
        default: DisplayTransform,
    ) -> DisplayTransform {
        let key = format!("model:{model_path}");
        if let Some(t) = self.cache.borrow().get(&key) {
            return *t;
        }
        let resolved = resolve_display(
            model_path,
            &self.jar_assets_dir,
            &self.asset_index,
            &self.pack_dirs,
            self.key,
        )
        .unwrap_or(default);
        self.cache.borrow_mut().insert(key, resolved);
        resolved
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

    pub fn resolve_left_hand(
        &self,
        item_name: &str,
        right_hand: &DisplayResolver,
        default: DisplayTransform,
    ) -> DisplayTransform {
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
            resolve_left_display_optional(
                &path,
                &self.jar_assets_dir,
                &self.asset_index,
                &self.pack_dirs,
                self.key,
                right_hand.key,
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
        if current.is_none() {
            // A successfully resolved model with no entry uses NO_TRANSFORM.
            // In particular template_skull has no first-person entries: the
            // missing-asset block fallback would incorrectly shrink/turn it.
            return Some(DisplayTransform::IDENTITY);
        }
    }

    None
}

fn resolve_left_display_optional(
    start_path: &str,
    jar: &Path,
    index: &Option<crate::assets::AssetIndex>,
    packs: &[PathBuf],
    left_key: &str,
    right_key: &str,
) -> Option<DisplayTransform> {
    let mut current = Some(start_path.to_string());
    for _ in 0..MODEL_PARENT_LIMIT {
        let path = current.take()?;
        let asset_key = crate::assets::AssetId::parse(&path).asset_key("models", ".json");
        let file = crate::assets::resolve_asset_path_with_pack_dirs(jar, index, &asset_key, packs);
        let json = read_json(&file)?;
        let display = json.get("display");
        if let Some(transform) = display
            .and_then(|display| display.get(left_key))
            .and_then(parse_display_transform)
            .or_else(|| {
                display
                    .and_then(|display| display.get(right_key))
                    .and_then(parse_display_transform)
            })
        {
            return Some(transform);
        }
        current = json
            .get("parent")
            .and_then(|parent| parent.as_str())
            .map(|parent| strip_mc_prefix(parent).to_string());
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
    fn third_person_left_display_falls_back_then_mirrors_official_axes() {
        let root = std::env::temp_dir().join(format!("held-display-{}", uuid::Uuid::new_v4()));
        write(
            &root,
            "minecraft/items/stick.json",
            r#"{"model":{"type":"minecraft:model","model":"minecraft:item/stick"}}"#,
        );
        model(
            &root,
            "stick",
            r#"{"display":{"thirdperson_righthand":{"rotation":[10,20,30],"translation":[4,5,6],"scale":[2,2,2]}}}"#,
        );
        let right = DisplayResolver::new(&root.join("assets"), "thirdperson_righthand");
        let left = DisplayResolver::new(&root.join("assets"), "thirdperson_lefthand");
        let fallback = left.resolve_left_hand("stick", &right, DisplayTransform::IDENTITY);
        assert_eq!(fallback.translation, Vec3::new(4.0, 5.0, 6.0) / 16.0);
        let matrix = fallback.to_left_hand_matrix();
        let expected = Mat4::from_translation(Vec3::new(-4.0, 5.0, 6.0) / 16.0)
            * Mat4::from_rotation_x(10.0_f32.to_radians())
            * Mat4::from_rotation_y(-20.0_f32.to_radians())
            * Mat4::from_rotation_z(-30.0_f32.to_radians())
            * Mat4::from_scale(Vec3::splat(2.0));
        assert!(matrix.abs_diff_eq(expected, 1.0e-6));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn left_hand_fallback_uses_each_models_right_before_parent_left() {
        let root = std::env::temp_dir().join(format!("held-left-parent-{}", uuid::Uuid::new_v4()));
        write(
            &root,
            "minecraft/items/stick.json",
            r#"{"model":"minecraft:item/child"}"#,
        );
        model(
            &root,
            "child",
            r#"{"parent":"minecraft:item/parent","display":{"thirdperson_righthand":{"translation":[1,2,3]}}}"#,
        );
        model(
            &root,
            "parent",
            r#"{"display":{"thirdperson_lefthand":{"translation":[9,8,7]}}}"#,
        );
        let right = DisplayResolver::new(&root.join("assets"), "thirdperson_righthand");
        let left = DisplayResolver::new(&root.join("assets"), "thirdperson_lefthand");
        let resolved = left.resolve_left_hand("stick", &right, DisplayTransform::IDENTITY);
        assert_eq!(resolved.translation, Vec3::new(1.0, 2.0, 3.0) / 16.0);
        assert_eq!(
            resolved.to_left_hand_matrix().w_axis.truncate(),
            Vec3::new(-1.0, 2.0, 3.0) / 16.0
        );

        model(
            &root,
            "child",
            r#"{"parent":"minecraft:item/parent","display":{"thirdperson_lefthand":{"translation":[4,5,6]},"thirdperson_righthand":{"translation":[1,2,3]}}}"#,
        );
        left.clear_cache();
        assert_eq!(
            left.resolve_left_hand("stick", &right, DisplayTransform::IDENTITY)
                .translation,
            Vec3::new(4.0, 5.0, 6.0) / 16.0,
            "explicit local left wins"
        );

        model(&root, "child", r#"{"parent":"minecraft:item/parent"}"#);
        left.clear_cache();
        assert_eq!(
            left.resolve_left_hand("stick", &right, DisplayTransform::IDENTITY)
                .translation,
            Vec3::new(9.0, 8.0, 7.0) / 16.0,
            "parent transform is used only after local left and right are absent"
        );

        write(
            &root,
            "minecraft/items/stick.json",
            r#"{"model":"minecraft:item/identity"}"#,
        );
        model(&root, "identity", "{}");
        left.clear_cache();
        assert_eq!(
            left.resolve_left_hand("stick", &right, DisplayTransform::IDENTITY)
                .to_matrix(),
            Mat4::IDENTITY,
            "resolved model without either hand transform is identity"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shield_gui_and_blocking_hand_displays_resolve_the_selected_base() {
        let root = std::env::temp_dir().join(format!("shield-display-{}", uuid::Uuid::new_v4()));
        write(
            &root,
            "minecraft/items/shield.json",
            r#"{"model":{"type":"minecraft:condition","on_false":{"type":"minecraft:special","base":"minecraft:item/shield","model":{"type":"minecraft:shield"}},"on_true":{"type":"minecraft:special","base":"minecraft:item/shield_blocking","model":{"type":"minecraft:shield"}},"property":"minecraft:using_item"}}"#,
        );
        model(
            &root,
            "shield",
            r#"{"display":{"gui":{"rotation":[15,-25,-5],"translation":[2,3,0],"scale":[0.65,0.65,0.65]}}}"#,
        );
        model(
            &root,
            "shield_blocking",
            r#"{"display":{"firstperson_righthand":{"rotation":[0,180,-5],"translation":[-15,3.25,-11],"scale":[1.25,1.25,1.25]},"firstperson_lefthand":{"rotation":[0,180,-5],"translation":[5,5,-11],"scale":[1.25,1.25,1.25]}}}"#,
        );
        let gui = DisplayResolver::new(&root.join("assets"), "gui")
            .resolve("shield", DisplayTransform::IDENTITY);
        assert_eq!(gui.rotation, Vec3::new(15.0, -25.0, -5.0));
        assert_eq!(gui.translation, Vec3::new(2.0, 3.0, 0.0) / 16.0);
        for (key, expected) in [
            ("firstperson_righthand", Vec3::new(-15.0, 3.25, -11.0)),
            ("firstperson_lefthand", Vec3::new(5.0, 5.0, -11.0)),
        ] {
            let resolver = DisplayResolver::new(&root.join("assets"), key);
            let blocked = resolver
                .resolve_model_path("minecraft:item/shield_blocking", DisplayTransform::IDENTITY);
            assert_eq!(blocked.translation, expected / 16.0);
            assert_eq!(blocked.scale, Vec3::splat(1.25));
            assert_eq!(
                resolver.resolve("shield", DisplayTransform::IDENTITY).scale,
                Vec3::ONE
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn player_head_missing_display_is_identity_but_missing_asset_keeps_fallback() {
        let root = std::env::temp_dir().join(format!("head-display-{}", uuid::Uuid::new_v4()));
        write(
            &root,
            "minecraft/items/player_head.json",
            r#"{"model":{
            "type":"minecraft:special","base":"minecraft:item/child",
            "model":{"type":"minecraft:player_head"}
        }}"#,
        );
        model(
            &root,
            "child",
            r#"{"parent":"minecraft:item/template_skull"}"#,
        );
        model(
            &root,
            "template_skull",
            r#"{"display":{"gui":{"rotation":[30,45,0]}}}"#,
        );
        let fallback = DisplayTransform {
            scale: Vec3::splat(0.4),
            ..DisplayTransform::IDENTITY
        };
        for key in ["firstperson_righthand", "firstperson_lefthand"] {
            let resolver = DisplayResolver::new(&root.join("assets"), key);
            assert_eq!(
                resolver.resolve("player_head", fallback).to_matrix(),
                Mat4::IDENTITY
            );
            assert_eq!(
                resolver.resolve("missing_item", fallback).to_matrix(),
                fallback.to_matrix()
            );
        }
        std::fs::remove_dir_all(root).unwrap();
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
