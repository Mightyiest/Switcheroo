use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub master_steam_id3: Option<String>,
    pub auto_sync_on_switch: bool,
    pub launch_options: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            master_steam_id3: None,
            auto_sync_on_switch: true,
            launch_options: "-novid -high -map dota".to_string(),
        }
    }
}

/// Resolves the settings file path with portable-mode awareness and `%APPDATA%\Switcheroo` fallback.
pub fn config_file_path() -> PathBuf {
    if let Some(portable_dir) = std::env::var_os("PORTABLE_EXECUTABLE_DIR") {
        let dir = PathBuf::from(portable_dir);
        if !dir.as_os_str().is_empty() {
            return dir.join("switcheroo-settings.json");
        }
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));

    if let Some(ref dir) = exe_dir {
        let portable_candidate = dir.join("switcheroo-settings.json");
        if portable_candidate.exists() {
            return portable_candidate;
        }
    }

    if let Some(appdata) = std::env::var_os("APPDATA") {
        let appdata_dir = PathBuf::from(appdata);
        if !appdata_dir.as_os_str().is_empty() {
            return appdata_dir.join("Switcheroo").join("switcheroo-settings.json");
        }
    }

    exe_dir
        .unwrap_or_else(|| PathBuf::from("."))
        .join("switcheroo-settings.json")
}

/// Writes `bytes` to `path` atomically via a temporary sibling file and rename.
pub fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("switcheroo.tmp");
    let tmp_path = path.with_file_name(format!("{file_name}.{nanos}-{pid}.tmp"));

    if let Err(err) = fs::write(&tmp_path, bytes) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Failed to write temp file {}: {err}", tmp_path.display()));
    }

    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Failed to rename temp file to {}: {err}", path.display()));
    }

    Ok(())
}

/// Loads `switcheroo-settings.json`, returning defaults on missing or unreadable files.
pub fn load_config() -> AppSettings {
    let path = config_file_path();
    load_config_from(&path)
}

pub fn load_config_from(path: &Path) -> AppSettings {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<AppSettings>(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// Atomically persists settings to `switcheroo-settings.json`.
pub fn save_config(settings: &AppSettings) -> Result<(), String> {
    let path = config_file_path();
    save_config_to(&path, settings)
}

pub fn save_config_to(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    atomic_write_bytes(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_camel_case_roundtrip_and_defaults() {
        let partial = r#"{"masterSteamId3":"123456"}"#;
        let parsed: AppSettings = serde_json::from_str(partial).unwrap();
        assert_eq!(parsed.master_steam_id3.as_deref(), Some("123456"));
        assert!(parsed.auto_sync_on_switch);
        assert_eq!(parsed.launch_options, "-novid -high -map dota");

        let serialized = serde_json::to_string(&parsed).unwrap();
        assert!(serialized.contains("\"masterSteamId3\":\"123456\""));
        assert!(serialized.contains("\"autoSyncOnSwitch\":true"));
        assert!(serialized.contains("\"launchOptions\":\"-novid -high -map dota\""));
    }

    #[test]
    fn atomic_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "switcheroo_cfg_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = dir.join("switcheroo-settings.json");
        let cfg = AppSettings {
            master_steam_id3: Some("987654321".to_string()),
            auto_sync_on_switch: false,
            launch_options: "-novid -dx11".to_string(),
        };

        save_config_to(&file, &cfg).unwrap();
        let loaded = load_config_from(&file);
        assert_eq!(loaded, cfg);

        let _ = fs::remove_dir_all(&dir);
    }
}
