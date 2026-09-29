use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use azalea_registry::builtin::ItemKind;

use crate::assets::{AssetIndex, resolve_asset_path};
use crate::player::inventory::item_resource_name;

struct Catalogs {
    english: HashMap<String, String>,
    japanese: HashMap<String, String>,
}

static LANG: OnceLock<Catalogs> = OnceLock::new();
// The maps are immutable after loading; changing this flag never invalidates a
// returned &'static str.
static LOCALE: AtomicU8 = AtomicU8::new(0);

fn read_catalog(path: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn catalogs(jar_assets_dir: &Path, asset_index: &Option<AssetIndex>) -> Catalogs {
    let english_jar = jar_assets_dir.join("minecraft/lang/en_us.json");
    let english = if english_jar.is_file() {
        read_catalog(&english_jar)
    } else {
        read_catalog(&resolve_asset_path(
            jar_assets_dir,
            asset_index,
            "minecraft/lang/en_us.json",
        ))
    };
    let japanese = read_catalog(&resolve_asset_path(
        jar_assets_dir,
        asset_index,
        "minecraft/lang/ja_jp.json",
    ));
    Catalogs { english, japanese }
}

/// Legacy jar-only bootstrap (also used by tests). The first bootstrap wins.
pub fn load(jar_assets_dir: &Path) {
    load_with_index(jar_assets_dir, &None);
}

/// Load immutable catalogs once; Japanese is normally supplied by the game
/// asset index.
pub fn load_with_index(jar_assets_dir: &Path, asset_index: &Option<AssetIndex>) {
    LANG.get_or_init(|| catalogs(jar_assets_dir, asset_index));
}

/// Select `en_us` or `ja_jp`. Unsupported names leave the selection unchanged.
/// Safe before bootstrap; missing Japanese keys (or the entire file) use
/// English.
pub fn set_locale(locale: &str) -> bool {
    let selected = match locale {
        "en_us" => 0,
        "ja_jp" => 1,
        _ => return false,
    };
    LOCALE.store(selected, Ordering::Relaxed);
    true
}

pub fn locale() -> &'static str {
    if LOCALE.load(Ordering::Relaxed) == 1 {
        "ja_jp"
    } else {
        "en_us"
    }
}

fn lookup<'a>(catalogs: &'a Catalogs, key: &str, selected: u8) -> Option<&'a str> {
    if selected == 1 {
        if let Some(text) = catalogs.japanese.get(key) {
            return Some(text);
        }
    }
    catalogs.english.get(key).map(String::as_str)
}

pub fn translate(key: &str) -> Option<&'static str> {
    lookup(LANG.get()?, key, LOCALE.load(Ordering::Relaxed))
}

pub fn item_display_name(kind: ItemKind) -> String {
    let bare = item_resource_name(kind);
    let block_key = format!("block.minecraft.{bare}");
    if let Some(name) = translate(&block_key) {
        return name.to_string();
    }
    let item_key = format!("item.minecraft.{bare}");
    if let Some(name) = translate(&item_key) {
        return name.to_string();
    }
    title_case_snake(&bare)
}

pub(crate) fn title_case_snake(s: &str) -> String {
    s.split('_')
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(first) => first.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_japanese_and_english_fallback_preserve_templates() {
        let root = std::env::temp_dir().join(format!("pomme-lang-{}", uuid::Uuid::new_v4()));
        let jar = root.join("jar");
        let indexes = root.join("indexes");
        let objects = root.join("objects");
        let hash = "1234567890123456789012345678901234567890";
        std::fs::create_dir_all(jar.join("minecraft/lang")).unwrap();
        std::fs::create_dir_all(objects.join("12")).unwrap();
        std::fs::create_dir_all(&indexes).unwrap();
        std::fs::write(
            jar.join("minecraft/lang/en_us.json"),
            r#"{"greeting":"Hello %s","english_only":"English","empty":""}"#,
        )
        .unwrap();
        std::fs::write(
            objects.join("12").join(hash),
            r#"{"greeting":"こんにちは %s","empty":""}"#,
        )
        .unwrap();
        std::fs::write(
            indexes.join("test.json"),
            format!(r#"{{"objects":{{"minecraft/lang/ja_jp.json":{{"hash":"{hash}"}}}}}}"#),
        )
        .unwrap();
        let index = AssetIndex::load(&indexes, &objects, "test");
        let maps = catalogs(&jar, &index);
        assert_eq!(lookup(&maps, "greeting", 1), Some("こんにちは %s"));
        assert_eq!(lookup(&maps, "greeting", 0), Some("Hello %s"));
        assert_eq!(lookup(&maps, "english_only", 1), Some("English"));
        assert_eq!(lookup(&maps, "empty", 1), Some(""));
        assert_eq!(lookup(&maps, "absent", 1), None);
        std::fs::remove_file(objects.join("12").join(hash)).unwrap();
        let missing = catalogs(&jar, &index);
        assert!(missing.japanese.is_empty());
        assert_eq!(lookup(&missing, "greeting", 1), Some("Hello %s"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
