use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::storage;

static LAUNCHER_SETTINGS: LazyLock<RwLock<LauncherSettings>> =
    LazyLock::new(|| RwLock::new(LauncherSettings::load()));

#[derive(Serialize, Deserialize, Debug, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    #[serde(default = "default_language")]
    pub language: String,
    pub keep_launcher_open: bool,
    pub launch_with_console: bool,
}

fn default_language() -> String {
    "en".into()
}

fn normalize_language(language: &str) -> &'static str {
    if matches!(language, "ja" | "Japanese") {
        "ja"
    } else {
        "en"
    }
}

impl Default for LauncherSettings {
    fn default() -> Self {
        LauncherSettings {
            language: default_language(),
            keep_launcher_open: true,
            launch_with_console: false,
        }
    }
}

impl LauncherSettings {
    async fn save(&self) -> Result<(), String> {
        let path = storage::settings_file();
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        tokio::fs::write(path, json)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn load() -> Self {
        let path = storage::settings_file();

        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<LauncherSettings>(&content) {
                Ok(mut cfg) => {
                    cfg.language = normalize_language(&cfg.language).into();
                    return cfg;
                }
                Err(err) => {
                    log::warn!("Settings file invalid ({}), using defaults", err);
                }
            },
            Err(_) => {
                log::info!("Settings file not found, creating default settings");
            }
        }

        let default = LauncherSettings::default();
        if let Ok(json) = serde_json::to_string_pretty(&default) {
            let _ = std::fs::write(&path, json);
        }
        default
    }

    pub async fn get() -> RwLockReadGuard<'static, LauncherSettings> {
        LAUNCHER_SETTINGS.read().await
    }

    pub async fn update<F>(f: F) -> Result<(), String>
    where
        F: FnOnce(&mut LauncherSettings),
    {
        let cloned = {
            let mut settings = LAUNCHER_SETTINGS.write().await;
            f(&mut settings);
            settings.clone()
        };

        cloned.save().await
    }
}

#[cfg(test)]
mod tests {
    use super::LauncherSettings;

    #[test]
    fn locale_defaults_and_persists() {
        assert_eq!(LauncherSettings::default().language, "en");
        let settings = LauncherSettings {
            language: "ja".into(),
            ..LauncherSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: LauncherSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.language, "ja");
    }

    #[test]
    fn missing_or_unsupported_language_falls_back_to_english() {
        let settings: LauncherSettings =
            serde_json::from_str(r#"{"keepLauncherOpen":true,"launchWithConsole":false}"#).unwrap();
        assert_eq!(settings.language, "en");
        assert_eq!(super::normalize_language("fr"), "en");
        assert_eq!(super::normalize_language("English"), "en");
    }
}
