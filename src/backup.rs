use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, Utc};

use crate::steam::{copy_shared_files_stamped, is_valid_steam_id3, list_shared_files};
use crate::win_platform;

pub const MAX_BACKUPS_PER_ACCOUNT: usize = 10;

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub folder_name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupMetadata {
    pub steam_id3: String,
    pub account_name: String,
    pub persona_name: String,
    pub created_utc: String,
}

#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub name: String,
    pub created_display: String,
    pub size_formatted: String,
    pub target_steam_id3: Option<String>,
    pub target_account_name: Option<String>,
    sort_time: SystemTime,
}

/// Validates a backup folder name to prevent path traversal.
pub fn is_valid_backup_folder(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

fn sanitize_backup_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.contains("..") {
        "account".to_string()
    } else {
        cleaned
    }
}

/// Resolves and ensures the Switcheroo backups directory exists.
/// Uses `PORTABLE_EXECUTABLE_DIR` if set, repo `backups/` for Cargo dev/test runs,
/// `<exe_dir>\backups` in portable mode, and `%APPDATA%\Switcheroo\backups` when installed.
pub fn backups_dir() -> PathBuf {
    if let Some(portable_dir) = std::env::var_os("PORTABLE_EXECUTABLE_DIR")
        && !portable_dir.is_empty()
    {
        let dir = PathBuf::from(portable_dir).join("backups");
        if fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));

    if let Some(ref dir) = exe_dir {
        if let Some(repo_root) = dir
            .ancestors()
            .take(5)
            .find(|a| a.join("Cargo.toml").is_file() && a.join("target").is_dir())
        {
            let dev_backups = repo_root.join("backups");
            let _ = fs::create_dir_all(&dev_backups);
            return dev_backups;
        }

        if !dir.join("Uninstall Switcheroo.exe").exists() {
            let exe_backups = dir.join("backups");
            if fs::create_dir_all(&exe_backups).is_ok() {
                return exe_backups;
            }
        }
    }

    if let Some(appdata) = std::env::var_os("APPDATA")
        && !appdata.is_empty()
    {
        let appdata_backups = PathBuf::from(appdata).join("Switcheroo").join("backups");
        let _ = fs::create_dir_all(&appdata_backups);
        return appdata_backups;
    }

    let fallback = exe_dir
        .unwrap_or_else(|| PathBuf::from("."))
        .join("backups");
    let _ = fs::create_dir_all(&fallback);
    fallback
}

/// Recursively copies `src` directory into `dest` and updates timestamps to `now`.
fn read_backup_metadata(backup_dir: &Path) -> Option<BackupMetadata> {
    let bytes = fs::read(backup_dir.join("backup-meta.json")).ok()?;
    serde_json::from_slice::<BackupMetadata>(&bytes).ok()
}

fn parse_backup_time(meta: Option<&BackupMetadata>, folder_name: &str, mtime: SystemTime) -> SystemTime {
    if let Some(m) = meta
        && let Ok(dt) = DateTime::parse_from_rfc3339(&m.created_utc)
    {
        return SystemTime::from(dt.with_timezone(&Utc));
    }

    if let Some((_, ts_part)) = folder_name.rsplit_once("_backup_") {
        // Strip optional `-NN` collision suffix (e.g. `2026-11-16T03-00-00-000Z-01`)
        let base_ts = if ts_part.ends_with('Z') {
            ts_part
        } else if let Some((head, tail)) = ts_part.rsplit_once('-')
            && tail.len() == 2
            && tail.bytes().all(|b| b.is_ascii_digit())
            && head.ends_with('Z')
        {
            head
        } else {
            ts_part
        };

        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(base_ts, "%Y-%m-%dT%H-%M-%S-%3fZ")
        {
            return SystemTime::from(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }

    mtime
}

fn account_matches_backup(
    folder_name: &str,
    meta: Option<&BackupMetadata>,
    steam_id3: &str,
    sanitized_label: &str,
) -> bool {
    if let Some(m) = meta
        && !m.steam_id3.is_empty()
        && !steam_id3.is_empty()
    {
        return m.steam_id3 == steam_id3;
    }

    if let Some(m) = meta
        && !m.account_name.is_empty()
        && !sanitized_label.is_empty()
    {
        return sanitize_backup_label(&m.account_name).eq_ignore_ascii_case(sanitized_label);
    }

    folder_name
        .rsplit_once("_backup_")
        .is_some_and(|(prefix, _)| prefix.eq_ignore_ascii_case(sanitized_label))
}

fn prune_account_backups(
    backups_root: &Path,
    steam_id3: &str,
    sanitized_label: &str,
    max_keep: usize,
) {
    let Ok(entries) = fs::read_dir(backups_root) else {
        return;
    };

    let mut matching: Vec<(SystemTime, String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_valid_backup_folder(&name) {
            continue;
        }
        let path = entry.path();
        let meta = read_backup_metadata(&path);
        if !account_matches_backup(&name, meta.as_ref(), steam_id3, sanitized_label) {
            continue;
        }
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(UNIX_EPOCH);
        let sort_time = parse_backup_time(meta.as_ref(), &name, mtime);
        matching.push((sort_time, name, path));
    }

    if matching.len() <= max_keep {
        return;
    }

    matching.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_, _, old_path) in matching.into_iter().skip(max_keep) {
        let _ = fs::remove_dir_all(old_path);
    }
}

/// Creates a selective, timestamped backup snapshot of shared Dota 2 settings for `target_dota_dir`.
pub fn create_backup_with_meta(
    target_dota_dir: &Path,
    steam_id3: &str,
    account_name: &str,
    persona_name: &str,
) -> Result<BackupInfo, String> {
    let dir = backups_dir();
    create_backup_in_with_meta(target_dota_dir, steam_id3, account_name, persona_name, &dir)
}

pub fn create_backup_in_with_meta(
    target_dota_dir: &Path,
    steam_id3: &str,
    account_name: &str,
    persona_name: &str,
    backups_root: &Path,
) -> Result<BackupInfo, String> {
    if !target_dota_dir.is_dir() {
        return Err(format!(
            "Target Dota 2 settings folder not found: {}",
            target_dota_dir.display()
        ));
    }

    let now_utc = Utc::now();
    let ts_slug = now_utc.format("%Y-%m-%dT%H-%M-%S-%3fZ").to_string();
    let label = sanitize_backup_label(account_name);
    let base_name = format!("{label}_backup_{ts_slug}");

    let mut folder_name = base_name.clone();
    let mut suffix = 1u32;
    while backups_root.join(&folder_name).exists() {
        folder_name = format!("{base_name}-{suffix:02}");
        suffix += 1;
    }

    let backup_path = backups_root.join(&folder_name);
    fs::create_dir_all(&backup_path)
        .map_err(|e| format!("Failed to create backup dir {}: {e}", backup_path.display()))?;

    let mut files = list_shared_files(target_dota_dir);
    if target_dota_dir.join("remotecache.vdf").is_file() {
        files.push("remotecache.vdf".to_string());
    }
    if !files.is_empty() {
        copy_shared_files_stamped(target_dota_dir, &backup_path, &files)?;
    }

    let meta = BackupMetadata {
        steam_id3: steam_id3.to_string(),
        account_name: account_name.to_string(),
        persona_name: persona_name.to_string(),
        created_utc: now_utc.to_rfc3339(),
    };
    let meta_bytes = serde_json::to_vec_pretty(&meta)
        .map_err(|e| format!("Failed to serialize backup metadata: {e}"))?;
    crate::config::atomic_write_bytes(&backup_path.join("backup-meta.json"), &meta_bytes)?;

    prune_account_backups(backups_root, steam_id3, &label, MAX_BACKUPS_PER_ACCOUNT);

    Ok(BackupInfo {
        folder_name,
        path: backup_path,
    })
}

fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            total = total.saturating_add(dir_size_bytes(&p));
        } else if ft.is_file()
            && let Ok(meta) = entry.metadata()
        {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Lists all backup snapshots sorted newest first.
pub fn list_backups() -> Vec<BackupEntry> {
    let dir = backups_dir();
    list_backups_in(&dir)
}

pub fn list_backups_in(backups_root: &Path) -> Vec<BackupEntry> {
    let Ok(entries) = fs::read_dir(backups_root) else {
        return Vec::new();
    };

    let mut backups = Vec::new();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let full_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_valid_backup_folder(&name) {
            continue;
        }

        let backup_meta = read_backup_metadata(&full_path);
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(UNIX_EPOCH);
        let sort_time = parse_backup_time(backup_meta.as_ref(), &name, mtime);
        let local_dt: DateTime<Local> = DateTime::from(sort_time);
        let bytes = dir_size_bytes(&full_path);

        let target_steam_id3 = backup_meta
            .as_ref()
            .map(|m| m.steam_id3.clone())
            .filter(|s| !s.is_empty());
        let target_account_name = backup_meta
            .as_ref()
            .map(|m| m.account_name.clone())
            .filter(|s| !s.is_empty());

        backups.push(BackupEntry {
            name,
            created_display: local_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            size_formatted: format!("{:.1} KB", bytes as f64 / 1024.0),
            target_steam_id3,
            target_account_name,
            sort_time,
        });
    }

    backups.sort_by(|a, b| b.sort_time.cmp(&a.sort_time).then_with(|| b.name.cmp(&a.name)));
    backups
}

/// Restores allowlisted settings files from a backup snapshot without touching `remotecache.vdf`.
pub fn restore_backup(
    backup_folder_name: &str,
    target_steam_id3: &str,
    target_account_name: &str,
    target_persona_name: &str,
    steam_path: &Path,
    steam_exe: Option<&Path>,
) -> Result<Vec<String>, String> {
    if !is_valid_backup_folder(backup_folder_name) {
        return Err("Invalid backup folder name format.".to_string());
    }
    if !is_valid_steam_id3(target_steam_id3) {
        return Err("Invalid Target Steam ID format.".to_string());
    }

    let backups_root = backups_dir();
    let backup_path = backups_root.join(backup_folder_name);
    if !backup_path.is_dir() {
        return Err("Backup folder not found".to_string());
    }

    let files = list_shared_files(&backup_path);
    if files.is_empty() {
        return Err("Selected backup contains no Dota 2 settings files to restore.".to_string());
    }

    if win_platform::is_dota_running() {
        return Err("Dota 2 is running. Close Dota 2 first, then restore.".to_string());
    }

    let was_steam_running = win_platform::is_steam_running();
    if was_steam_running {
        win_platform::kill_steam(steam_exe)?;
    }

    let target_dota_dir = steam_path
        .join("userdata")
        .join(target_steam_id3)
        .join("570");

    if target_dota_dir.is_dir()
        && !crate::steam::are_shared_files_identical(&backup_path, &target_dota_dir, &files)
    {
        create_backup_in_with_meta(
            &target_dota_dir,
            target_steam_id3,
            target_account_name,
            target_persona_name,
            &backups_root,
        )?;
    }

    let copy_result = copy_shared_files_stamped(&backup_path, &target_dota_dir, &files);
    if was_steam_running
        && let Some(exe) = steam_exe.filter(|p| p.is_file())
    {
        let _ = win_platform::spawn_detached(exe, &[]);
    }

    copy_result?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_folder_validation_blocks_traversal() {
        assert!(is_valid_backup_folder("main_acc_backup_2026-11-16T03-00-00-000Z"));
        assert!(!is_valid_backup_folder(""));
        assert!(!is_valid_backup_folder("../etc"));
        assert!(!is_valid_backup_folder("foo..bar"));
        assert!(!is_valid_backup_folder("foo/bar"));
        assert!(!is_valid_backup_folder("foo\\bar"));
    }

    #[test]
    fn create_and_list_backups_in_temp_dir() {
        let base = std::env::temp_dir().join(format!(
            "switcheroo_backup_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src = base.join("570");
        let cfg = src.join("remote").join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(cfg.join("dotakeys_personal.lst"), b"keys").unwrap();

        let backups_root = base.join("backups");
        fs::create_dir_all(&backups_root).unwrap();

        let created =
            create_backup_in_with_meta(&src, "", "my_user", "", &backups_root).unwrap();
        assert!(created.path.join("remote/cfg/dotakeys_personal.lst").exists());

        let listed = list_backups_in(&backups_root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, created.folder_name);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn selective_backup_excludes_unshared_files_and_writes_meta_with_backup_in_name() {
        let base = std::env::temp_dir().join(format!(
            "switcheroo_selective_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src = base.join("userdata").join("182260946").join("570");
        let cfg = src.join("remote").join("cfg");
        let guides = src.join("remote").join("guides");
        fs::create_dir_all(&cfg).unwrap();
        fs::create_dir_all(&guides).unwrap();

        fs::write(cfg.join("dotakeys_personal.lst"), b"keys").unwrap();
        fs::write(cfg.join("chat.cfg"), b"VBKV").unwrap();
        fs::write(cfg.join("config.cfg"), b"legacy").unwrap();
        fs::write(guides.join("hero.build"), b"guide").unwrap();
        fs::write(src.join("remotecache.vdf"), b"cache").unwrap();

        let backups_root = base.join("backups");
        fs::create_dir_all(&backups_root).unwrap();

        let created =
            create_backup_in_with_meta(&src, "182260946", "my_backup_user", "P4", &backups_root)
                .unwrap();

        assert!(created.path.join("remote/cfg/dotakeys_personal.lst").exists());
        assert!(created.path.join("remotecache.vdf").exists());
        assert!(created.path.join("backup-meta.json").exists());
        assert!(!created.path.join("remote/cfg/chat.cfg").exists());
        assert!(!created.path.join("remote/cfg/config.cfg").exists());
        assert!(!created.path.join("remote/guides/hero.build").exists());
        assert_eq!(
            created.folder_name.rsplit_once("_backup_").map(|(p, _)| p),
            Some("my_backup_user")
        );

        let listed = list_backups_in(&backups_root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].target_steam_id3.as_deref(), Some("182260946"));
        assert_eq!(
            listed[0].target_account_name.as_deref(),
            Some("my_backup_user")
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn prunes_old_backups_per_account_to_limit() {
        let base = std::env::temp_dir().join(format!(
            "switcheroo_prune_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src_a = base.join("userdata").join("111").join("570");
        let src_b = base.join("userdata").join("222").join("570");
        fs::create_dir_all(src_a.join("remote/cfg")).unwrap();
        fs::create_dir_all(src_b.join("remote/cfg")).unwrap();
        fs::write(src_a.join("remote/cfg/dotakeys_personal.lst"), b"a").unwrap();
        fs::write(src_b.join("remote/cfg/dotakeys_personal.lst"), b"b").unwrap();

        let backups_root = base.join("backups");
        fs::create_dir_all(&backups_root).unwrap();

        let mut created_a = Vec::new();
        for _ in 0..12 {
            let info =
                create_backup_in_with_meta(&src_a, "111", "acc_a", "AccA", &backups_root).unwrap();
            created_a.push(info.folder_name);
        }
        for _ in 0..2 {
            create_backup_in_with_meta(&src_b, "222", "acc_b", "AccB", &backups_root).unwrap();
        }

        let listed = list_backups_in(&backups_root);
        let count_a = listed
            .iter()
            .filter(|e| e.target_steam_id3.as_deref() == Some("111"))
            .count();
        let count_b = listed
            .iter()
            .filter(|e| e.target_steam_id3.as_deref() == Some("222"))
            .count();
        assert_eq!(count_a, MAX_BACKUPS_PER_ACCOUNT);
        assert_eq!(count_b, 2);
        assert!(!backups_root.join(&created_a[0]).exists());
        assert!(!backups_root.join(&created_a[1]).exists());
        assert!(backups_root.join(&created_a[11]).exists());

        let _ = fs::remove_dir_all(&base);
    }
}
