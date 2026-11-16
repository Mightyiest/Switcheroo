use std::collections::HashMap;
use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha1::{Digest, Sha1};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

use crate::backup::{BackupInfo, create_backup_in_with_meta, create_backup_with_meta};
use crate::config::atomic_write_bytes;
use crate::vdf::{parse_library_folders, parse_login_users, rewrite_login_users_autologin};
use crate::win_platform;

pub const STEAM_ID_BASE: u64 = 76_561_197_960_265_728;

/// Allowlisted Dota 2 remote settings files safe to copy between accounts.
/// Deliberately excludes `remotecache.vdf` (Steam Cloud sync state) and account-specific files.
pub const SHARED_REMOTE_FILES: &[&str] = &[
    "remote/cfg/dotakeys_personal.lst",
    "remote/cfg/herobuilds.cfg",
    "remote/cfg/hero_facet_config.cfg",
    "remote/user_keys.vcfg",
    "remote/user_convars.vcfg",
    "remote/scripts/control_groups.txt",
    "remote/scripts/item_suggest_preference.txt",
];

#[derive(Debug, Clone, Default)]
pub struct SteamDetection {
    pub found: bool,
    pub is_running: bool,
    pub steam_path: Option<PathBuf>,
    pub steam_exe: Option<PathBuf>,
    pub current_auto_login_user: Option<String>,
    pub active_steam_id3: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DotaDetection {
    pub installed: bool,
    pub exe_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DecodedAvatar {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub fingerprint: (u64, u64),
}

#[derive(Debug, Clone)]
pub struct SteamAccount {
    pub steam_id64: String,
    pub steam_id3: String,
    pub account_name: String,
    pub persona_name: String,
    pub is_active: bool,
    pub decoded_avatar: Option<DecodedAvatar>,
    pub has_dota_config: bool,
    pub hotkeys_hash: Option<String>,
    pub dota_files_count: usize,
}

#[derive(Debug, Clone)]
pub struct SwitchOptions {
    pub target_account_name: String,
    pub target_steam_id3: String,
    pub target_persona_name: String,
    pub source_steam_id3: Option<String>,
    pub auto_sync_settings: bool,
    pub launch_dota: bool,
    pub launch_options: String,
}

#[derive(Debug, Clone)]
pub struct SwitchResult {
    pub launched_dota: bool,
    pub synced: bool,
    pub already_active: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub skipped: bool,
    pub backup_created: Option<BackupInfo>,
    pub files_copied: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SyncAccountDetail {
    pub persona_name: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncAllResult {
    pub synced_count: usize,
    pub skipped_count: usize,
    pub details: Vec<SyncAccountDetail>,
}

/// Validates that a SteamID3 is 1..=16 ASCII digits.
pub fn is_valid_steam_id3(id: &str) -> bool {
    (1..=16).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

/// Converts a 64-bit SteamID string into its 32-bit AccountID (SteamID3) string.
pub fn steam_id64_to_id3(steam_id64: &str) -> String {
    steam_id64
        .trim()
        .parse::<u64>()
        .ok()
        .and_then(|v| v.checked_sub(STEAM_ID_BASE))
        .map(|v| v.to_string())
        .unwrap_or_else(|| steam_id64.to_string())
}

/// Matches allowlisted local Dota 2 config filenames (`local/cfg/*`).
pub fn is_shared_local_cfg_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "machine_convars.vcfg" || lower == "video.txt" {
        return true;
    }

    let stem = lower.strip_suffix("_lastclouded").unwrap_or(&lower);
    let Some(body) = stem.strip_suffix(".vcfg") else {
        return false;
    };
    let rest = if let Some(r) = body.strip_prefix("user_keys_") {
        r
    } else if let Some(r) = body.strip_prefix("user_convars_") {
        r
    } else {
        return false;
    };
    let Some((acct, slot)) = rest.split_once("_slot") else {
        return false;
    };

    !acct.is_empty()
        && acct.bytes().all(|b| b.is_ascii_digit())
        && !slot.is_empty()
        && slot.bytes().all(|b| b.is_ascii_digit())
}

fn to_hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Computes the lowercase SHA-1 hex digest of an in-memory byte slice.
pub fn sha1_bytes(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    to_hex_lower(&hasher.finalize())
}

/// Computes the lowercase SHA-1 hex digest of a file.
pub fn sha1_file(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(sha1_bytes(&data))
}

/// Clears the Windows read-only attribute on `path` if present (best-effort).
pub fn clear_readonly(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
    }
}

/// Computes combined SHA-1 hash over all synced settings files inside `dota_dir`
/// (excluding machine-wide `machine_convars.vcfg` and `video.txt`),
/// separating each entry by relative path, a NUL byte, and little-endian byte length.
pub fn get_hotkeys_hash(dota_dir: &Path) -> Option<String> {
    let mut hasher = Sha1::new();
    let mut found = false;
    for rel in list_shared_files(dota_dir) {
        let lower = rel.to_ascii_lowercase();
        if lower == "local/cfg/machine_convars.vcfg" || lower == "local/cfg/video.txt" {
            continue;
        }
        let f = dota_dir.join(&rel);
        if f.is_file()
            && let Ok(bytes) = fs::read(&f)
        {
            hasher.update(rel.as_bytes());
            hasher.update([0u8]);
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(&bytes);
            found = true;
        }
    }
    if found {
        Some(to_hex_lower(&hasher.finalize()))
    } else {
        None
    }
}

/// Sets both accessed and modified timestamps on `path` (best-effort, matching JS behavior).
pub fn stamp_file_times(path: &Path, time: SystemTime) {
    if let Ok(file) = File::options().write(true).open(path) {
        let times = FileTimes::new().set_accessed(time).set_modified(time);
        let _ = file.set_times(times);
    }
}

/// Returns `true` when every relative path in `files` exists in both `dir_a` and `dir_b`
/// with identical byte contents.
pub fn are_shared_files_identical(dir_a: &Path, dir_b: &Path, files: &[String]) -> bool {
    for rel in files {
        let a = dir_a.join(rel);
        let b = dir_b.join(rel);
        let Ok(bytes_a) = fs::read(&a) else {
            return false;
        };
        let Ok(bytes_b) = fs::read(&b) else {
            return false;
        };
        if bytes_a != bytes_b {
            return false;
        }
    }
    true
}

/// Lists all allowlisted settings files relative to a `570` directory (sorted deterministically).
pub fn list_shared_files(source_dota_dir: &Path) -> Vec<String> {
    let mut files: Vec<String> = SHARED_REMOTE_FILES
        .iter()
        .filter(|rel| source_dota_dir.join(rel).is_file())
        .map(|s| (*s).to_string())
        .collect();

    let local_cfg = source_dota_dir.join("local").join("cfg");
    if local_cfg.is_dir()
        && let Ok(entries) = fs::read_dir(&local_cfg)
    {
        let mut local_files = Vec::new();
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type()
                && ft.is_file()
                && let Some(name) = entry.file_name().to_str()
                && is_shared_local_cfg_file(name)
            {
                local_files.push(format!("local/cfg/{name}"));
            }
        }
        local_files.sort();
        files.extend(local_files);
    }
    files
}

/// Copies `files` from `src_dota_dir` into `dst_dota_dir`, reading each source once,
/// clearing any destination read-only flag, updating timestamps, and verifying SHA-1.
pub fn copy_shared_files_stamped(
    src_dota_dir: &Path,
    dst_dota_dir: &Path,
    files: &[String],
) -> Result<(), String> {
    let now = SystemTime::now();
    for rel in files {
        let src = src_dota_dir.join(rel);
        let dst = dst_dota_dir.join(rel);
        let src_bytes =
            fs::read(&src).map_err(|e| format!("Failed to read {}: {e}", src.display()))?;
        let expected_sha1 = sha1_bytes(&src_bytes);

        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir {}: {e}", parent.display()))?;
        }
        if dst.exists() {
            clear_readonly(&dst);
        }
        fs::write(&dst, &src_bytes).map_err(|e| {
            format!(
                "Failed to copy {} to {}: {e}",
                src.display(),
                dst.display()
            )
        })?;
        clear_readonly(&dst);
        stamp_file_times(&dst, now);

        let actual_sha1 = sha1_file(&dst)?;
        if actual_sha1 != expected_sha1 {
            return Err(format!(
                "Verification failed for {rel}: expected {expected_sha1}, got {actual_sha1}"
            ));
        }
    }
    Ok(())
}

fn normalize_windows_path(raw: &str) -> PathBuf {
    let cleaned = raw.trim().replace('/', "\\");
    PathBuf::from(cleaned)
}

/// Automatically detects Steam installation via `HKCU\Software\Valve\Steam` or common directories.
pub fn detect_steam() -> SteamDetection {
    let mut steam_path: Option<PathBuf> = None;
    let mut steam_exe: Option<PathBuf> = None;
    let mut current_auto_login: Option<String> = None;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(r"Software\Valve\Steam") {
        if let Ok(val) = key.get_value::<String, _>("SteamPath")
            && !val.trim().is_empty()
        {
            steam_path = Some(normalize_windows_path(&val));
        }
        if let Ok(val) = key.get_value::<String, _>("SteamExe")
            && !val.trim().is_empty()
        {
            steam_exe = Some(normalize_windows_path(&val));
        }
        if let Ok(val) = key.get_value::<String, _>("AutoLoginUser") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                current_auto_login = Some(trimmed.to_string());
            }
        }
    }

    if steam_path.as_ref().is_none_or(|p| !p.exists()) {
        const COMMON_PATHS: &[&str] = &[
            r"C:\Program Files (x86)\Steam",
            r"C:\Program Files\Steam",
            r"D:\Steam",
            r"D:\Application\Steam",
            r"E:\Steam",
            r"E:\Application\Steam",
        ];
        for candidate in COMMON_PATHS {
            let p = PathBuf::from(candidate);
            let exe = p.join("steam.exe");
            if p.exists() && exe.exists() {
                steam_path = Some(p);
                steam_exe = Some(exe);
                break;
            }
        }
    }

    if let Some(ref sp) = steam_path
        && steam_exe.as_ref().is_none_or(|e| !e.exists())
    {
        let default_exe = sp.join("steam.exe");
        if default_exe.exists() {
            steam_exe = Some(default_exe);
        }
    }

    let is_running = win_platform::is_steam_running();
    let active_steam_id3 = if is_running {
        hkcu.open_subkey(r"Software\Valve\Steam\ActiveProcess")
            .ok()
            .and_then(|ap| ap.get_value::<u32, _>("ActiveUser").ok())
            .filter(|&uid| uid > 0)
            .map(|uid| uid.to_string())
    } else {
        None
    };

    let found = steam_path.as_ref().is_some_and(|p| p.exists())
        && steam_exe.as_ref().is_some_and(|e| e.exists());

    SteamDetection {
        found,
        is_running,
        steam_path,
        steam_exe,
        current_auto_login_user: current_auto_login,
        active_steam_id3,
    }
}

/// Automatically detects Dota 2 installation in Steam or library folders.
pub fn detect_dota(steam_path: &Path) -> DotaDetection {
    if !steam_path.exists() {
        return DotaDetection::default();
    }

    let dota_rel = Path::new("steamapps")
        .join("common")
        .join("dota 2 beta")
        .join("game")
        .join("bin")
        .join("win64")
        .join("dota2.exe");

    let standard_exe = steam_path.join(&dota_rel);
    if standard_exe.exists() {
        return DotaDetection {
            installed: true,
            exe_path: Some(standard_exe),
        };
    }

    let library_folders_path = steam_path.join("config").join("libraryfolders.vdf");
    if let Ok(content) = fs::read_to_string(&library_folders_path) {
        for lib_path in parse_library_folders(&content) {
            let test_exe = lib_path.join(&dota_rel);
            if test_exe.exists() {
                return DotaDetection {
                    installed: true,
                    exe_path: Some(test_exe),
                };
            }
        }
    }

    DotaDetection::default()
}

fn count_files_recursive(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            count += count_files_recursive(&entry.path());
        } else if ft.is_file() {
            count += 1;
        }
    }
    count
}

fn file_fingerprint(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let len = meta.len();
    let mtime_nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0);
    Some((len, mtime_nanos))
}

fn decode_avatar_if_needed(
    avatar_path: &Path,
    steam_id64: &str,
    avatar_cache: &HashMap<String, (u64, u64)>,
) -> Option<DecodedAvatar> {
    let fp = file_fingerprint(avatar_path)?;
    if avatar_cache.get(steam_id64) == Some(&fp) {
        return None;
    }
    let bytes = fs::read(avatar_path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba_img = img.to_rgba8();
    let (w, h) = rgba_img.dimensions();
    Some(DecodedAvatar {
        width: w as usize,
        height: h as usize,
        rgba: rgba_img.into_raw(),
        fingerprint: fp,
    })
}

/// Parses `loginusers.vdf` and enriches each account with SteamID3, cached avatar decoding,
/// active state (`ActiveProcess\ActiveUser`), and Dota 2 status.
pub fn get_accounts_cached(
    steam_path: &Path,
    active_steam_id3: Option<&str>,
    master_steam_id3: Option<&str>,
    avatar_cache: &HashMap<String, (u64, u64)>,
) -> Vec<SteamAccount> {
    let login_users_path = steam_path.join("config").join("loginusers.vdf");
    let Ok(raw) = fs::read_to_string(&login_users_path) else {
        return Vec::new();
    };

    let raw_users = parse_login_users(&raw);
    let avatar_cache_dir = steam_path.join("config").join("avatarcache");
    let userdata_dir = steam_path.join("userdata");

    let master_known = master_steam_id3
        .is_some_and(|m| raw_users.iter().any(|u| steam_id64_to_id3(&u.steam_id64) == m));

    raw_users
        .into_iter()
        .map(|acc| {
            let steam_id3 = steam_id64_to_id3(&acc.steam_id64);
            let avatar_file = avatar_cache_dir.join(format!("{}.png", acc.steam_id64));
            let decoded_avatar = decode_avatar_if_needed(&avatar_file, &acc.steam_id64, avatar_cache);

            let dota_userdata_dir = userdata_dir.join(&steam_id3).join("570");
            let has_dota_config = dota_userdata_dir.is_dir();
            let hotkeys_hash = if has_dota_config {
                get_hotkeys_hash(&dota_userdata_dir)
            } else {
                None
            };
            let should_count = has_dota_config
                && (!master_known || master_steam_id3 == Some(steam_id3.as_str()));
            let dota_files_count = if should_count {
                count_files_recursive(&dota_userdata_dir)
            } else {
                0
            };

            let is_active = active_steam_id3 == Some(steam_id3.as_str());

            let persona_name = if acc.persona_name.is_empty() {
                if acc.account_name.is_empty() {
                    "Steam User".to_string()
                } else {
                    acc.account_name.clone()
                }
            } else {
                acc.persona_name
            };

            SteamAccount {
                steam_id64: acc.steam_id64,
                steam_id3,
                account_name: acc.account_name,
                persona_name,
                is_active,
                decoded_avatar,
                has_dota_config,
                hotkeys_hash,
                dota_files_count,
            }
        })
        .collect()
}

/// Splits launch options on whitespace while preserving `"quoted"` tokens and stripping quotes.
pub fn parse_launch_options(opts: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;

    for ch in opts.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token && !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                has_token = false;
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }

    if has_token && !current.is_empty() {
        args.push(current);
    }

    args
}

/// Synchronizes allowlisted Dota 2 settings from `source_steam_id3` to `target_steam_id3`,
/// restarting Steam automatically afterwards if it had been running.
pub fn sync_dota_settings(
    steam_path: &Path,
    steam_exe: Option<&Path>,
    source_steam_id3: &str,
    target_steam_id3: &str,
    target_account_name: &str,
    target_persona_name: &str,
) -> Result<SyncResult, String> {
    if !is_valid_steam_id3(source_steam_id3) || !is_valid_steam_id3(target_steam_id3) {
        return Err("Invalid Steam ID format.".to_string());
    }
    if source_steam_id3 == target_steam_id3 {
        return Ok(SyncResult {
            skipped: true,
            backup_created: None,
            files_copied: Vec::new(),
        });
    }

    if win_platform::is_dota_running() {
        return Err("Dota 2 is running. Close Dota 2 first, then sync.".to_string());
    }

    let userdata_dir = steam_path.join("userdata");
    let source_dota_dir = userdata_dir.join(source_steam_id3).join("570");
    let target_dota_dir = userdata_dir.join(target_steam_id3).join("570");
    if !source_dota_dir.is_dir() {
        return Err(format!(
            "Master account does not have Dota 2 settings in: {}",
            source_dota_dir.display()
        ));
    }

    let files = list_shared_files(&source_dota_dir);
    if files.is_empty() {
        return Err("Master account has no allowlisted Dota 2 settings files to sync.".to_string());
    }
    if target_dota_dir.is_dir()
        && are_shared_files_identical(&source_dota_dir, &target_dota_dir, &files)
    {
        return Ok(SyncResult {
            skipped: true,
            backup_created: None,
            files_copied: Vec::new(),
        });
    }

    let was_steam_running = win_platform::is_steam_running();
    if was_steam_running {
        win_platform::kill_steam(steam_exe)?;
    }

    let result = sync_dota_settings_dirs(
        steam_path,
        source_steam_id3,
        target_steam_id3,
        target_account_name,
        target_persona_name,
        None,
    );

    if was_steam_running
        && let Some(exe) = steam_exe.filter(|p| p.is_file())
    {
        let _ = win_platform::spawn_detached(exe, &[]);
    }

    result
}

/// Inner directory-based sync implementation (also used directly in unit tests).
pub fn sync_dota_settings_dirs(
    steam_path: &Path,
    source_steam_id3: &str,
    target_steam_id3: &str,
    target_account_name: &str,
    target_persona_name: &str,
    backup_root_override: Option<&Path>,
) -> Result<SyncResult, String> {
    let userdata_dir = steam_path.join("userdata");
    let source_dota_dir = userdata_dir.join(source_steam_id3).join("570");
    let target_dota_dir = userdata_dir.join(target_steam_id3).join("570");

    if !source_dota_dir.is_dir() {
        return Err(format!(
            "Master account does not have Dota 2 settings in: {}",
            source_dota_dir.display()
        ));
    }

    let files = list_shared_files(&source_dota_dir);
    if files.is_empty() {
        return Err("Master account has no allowlisted Dota 2 settings files to sync.".to_string());
    }

    if target_dota_dir.is_dir()
        && are_shared_files_identical(&source_dota_dir, &target_dota_dir, &files)
    {
        return Ok(SyncResult {
            skipped: true,
            backup_created: None,
            files_copied: Vec::new(),
        });
    }

    let backup_created = if target_dota_dir.is_dir() {
        let label = if target_account_name.is_empty() {
            target_steam_id3
        } else {
            target_account_name
        };
        let info = match backup_root_override {
            Some(root) => create_backup_in_with_meta(
                &target_dota_dir,
                target_steam_id3,
                label,
                target_persona_name,
                root,
            )?,
            None => create_backup_with_meta(
                &target_dota_dir,
                target_steam_id3,
                label,
                target_persona_name,
            )?,
        };
        Some(info)
    } else {
        None
    };

    copy_shared_files_stamped(&source_dota_dir, &target_dota_dir, &files)?;

    Ok(SyncResult {
        skipped: false,
        backup_created,
        files_copied: files,
    })
}

/// Synchronizes Dota 2 settings from `source_steam_id3` to all other accounts in a single pass
/// (killing and restarting Steam at most once).
pub fn sync_to_all(
    steam_path: &Path,
    steam_exe: Option<&Path>,
    source_steam_id3: &str,
) -> Result<SyncAllResult, String> {
    if !is_valid_steam_id3(source_steam_id3) {
        return Err("Invalid Source Steam ID format.".to_string());
    }

    if win_platform::is_dota_running() {
        return Err("Dota 2 is running. Close Dota 2 first, then sync all.".to_string());
    }

    let source_dota_dir = steam_path.join("userdata").join(source_steam_id3).join("570");
    if !source_dota_dir.is_dir() {
        return Err(format!(
            "Master account does not have Dota 2 settings in: {}",
            source_dota_dir.display()
        ));
    }
    let master_files = list_shared_files(&source_dota_dir);
    if master_files.is_empty() {
        return Err("Master account has no allowlisted Dota 2 settings files to sync.".to_string());
    }

    let login_users_path = steam_path.join("config").join("loginusers.vdf");
    let raw = fs::read_to_string(&login_users_path)
        .map_err(|e| format!("Failed to read loginusers.vdf: {e}"))?;

    struct SyncTarget {
        steam_id3: String,
        account_name: String,
        persona_name: String,
    }

    let targets: Vec<SyncTarget> = parse_login_users(&raw)
        .into_iter()
        .filter_map(|acc| {
            let steam_id3 = steam_id64_to_id3(&acc.steam_id64);
            if !is_valid_steam_id3(&steam_id3) || steam_id3 == source_steam_id3 {
                return None;
            }
            let persona_name = if acc.persona_name.is_empty() {
                if acc.account_name.is_empty() {
                    "Steam User".to_string()
                } else {
                    acc.account_name.clone()
                }
            } else {
                acc.persona_name
            };
            Some(SyncTarget {
                steam_id3,
                account_name: acc.account_name,
                persona_name,
            })
        })
        .collect();

    if targets.is_empty() {
        return Ok(SyncAllResult {
            synced_count: 0,
            skipped_count: 0,
            details: Vec::new(),
        });
    }

    let userdata_dir = steam_path.join("userdata");
    let all_identical = targets.iter().all(|t| {
        let target_dir = userdata_dir.join(&t.steam_id3).join("570");
        target_dir.is_dir() && are_shared_files_identical(&source_dota_dir, &target_dir, &master_files)
    });

    if all_identical {
        let details = targets
            .into_iter()
            .map(|t| SyncAccountDetail {
                persona_name: t.persona_name,
                success: true,
                error: None,
            })
            .collect::<Vec<_>>();
        let skipped_count = details.len();
        return Ok(SyncAllResult {
            synced_count: 0,
            skipped_count,
            details,
        });
    }

    let was_steam_running = win_platform::is_steam_running();
    if was_steam_running {
        win_platform::kill_steam(steam_exe)?;
    }

    let mut details = Vec::with_capacity(targets.len());
    let mut synced_count = 0usize;
    let mut skipped_count = 0usize;

    for target in targets {
        match sync_dota_settings_dirs(
            steam_path,
            source_steam_id3,
            &target.steam_id3,
            &target.account_name,
            &target.persona_name,
            None,
        ) {
            Ok(res) => {
                if res.skipped {
                    skipped_count += 1;
                } else {
                    synced_count += 1;
                }
                details.push(SyncAccountDetail {
                    persona_name: target.persona_name,
                    success: true,
                    error: None,
                });
            }
            Err(err) => {
                details.push(SyncAccountDetail {
                    persona_name: target.persona_name,
                    success: false,
                    error: Some(err),
                });
            }
        }
    }

    if was_steam_running
        && let Some(exe) = steam_exe.filter(|p| p.is_file())
    {
        let _ = win_platform::spawn_detached(exe, &[]);
    }

    Ok(SyncAllResult {
        synced_count,
        skipped_count,
        details,
    })
}

/// Switches active Steam account (or launches Dota 2 directly if already active & running).
pub fn switch_account(
    steam_path: &Path,
    steam_exe: &Path,
    opts: &SwitchOptions,
) -> Result<SwitchResult, String> {
    if !is_valid_steam_id3(&opts.target_steam_id3) {
        return Err("Invalid Target Steam ID format.".to_string());
    }
    if let Some(ref src_id) = opts.source_steam_id3
        && !is_valid_steam_id3(src_id)
    {
        return Err("Invalid Source Steam ID format.".to_string());
    }
    if !steam_exe.is_file() {
        return Err(format!("Steam executable not found: {}", steam_exe.display()));
    }

    let mut dota_args = vec!["-applaunch".to_string(), "570".to_string()];
    dota_args.extend(parse_launch_options(&opts.launch_options));

    let current_steam = detect_steam();
    let is_already_active = current_steam.is_running
        && current_steam
            .active_steam_id3
            .as_deref()
            .is_some_and(|active_id3| active_id3 == opts.target_steam_id3);

    if is_already_active {
        if opts.launch_dota {
            win_platform::spawn_detached(steam_exe, &dota_args)?;
        }
        return Ok(SwitchResult {
            launched_dota: opts.launch_dota,
            synced: false,
            already_active: true,
            warnings: Vec::new(),
        });
    }

    if win_platform::is_dota_running() {
        return Err("Dota 2 is running. Close Dota 2 first, then switch accounts.".to_string());
    }

    let mut warnings = Vec::new();
    let mut can_sync_master: Option<String> = None;
    if opts.auto_sync_settings
        && let Some(ref src_id) = opts.source_steam_id3
        && src_id != &opts.target_steam_id3
    {
        let master_dota_dir = steam_path.join("userdata").join(src_id).join("570");
        if master_dota_dir.is_dir() {
            can_sync_master = Some(src_id.clone());
        } else {
            warnings.push(format!(
                "Auto-sync skipped: Master account has no Dota 2 settings in {}.",
                master_dota_dir.display()
            ));
        }
    }

    win_platform::kill_steam(Some(steam_exe))?;

    let mut synced = false;
    if let Some(ref src_id) = can_sync_master {
        match sync_dota_settings_dirs(
            steam_path,
            src_id,
            &opts.target_steam_id3,
            &opts.target_account_name,
            &opts.target_persona_name,
            None,
        ) {
            Ok(res) => {
                synced = !res.skipped;
            }
            Err(err) => {
                warnings.push(format!("Auto-sync warning: {err}"));
            }
        }
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (steam_key, _) = hkcu
        .create_subkey(r"Software\Valve\Steam")
        .map_err(|e| format!("Failed to update Windows Registry: {e}"))?;
    steam_key
        .set_value("AutoLoginUser", &opts.target_account_name)
        .map_err(|e| format!("Failed to update Windows Registry AutoLoginUser: {e}"))?;
    steam_key
        .set_value("RememberPassword", &1u32)
        .map_err(|e| format!("Failed to update Windows Registry RememberPassword: {e}"))?;

    let login_users_path = steam_path.join("config").join("loginusers.vdf");
    if login_users_path.is_file() {
        match fs::read_to_string(&login_users_path) {
            Ok(content) => {
                let updated = rewrite_login_users_autologin(&content, &opts.target_account_name);
                if let Err(err) = atomic_write_bytes(&login_users_path, updated.as_bytes()) {
                    warnings.push(format!("Could not update loginusers.vdf: {err}"));
                }
            }
            Err(err) => {
                warnings.push(format!("Could not read loginusers.vdf: {err}"));
            }
        }
    }

    let launch_args: Vec<String> = if opts.launch_dota {
        dota_args
    } else {
        Vec::new()
    };
    win_platform::spawn_detached(steam_exe, &launch_args)?;

    Ok(SwitchResult {
        launched_dota: opts.launch_dota,
        synced,
        already_active: false,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn converts_steam_id64_to_id3() {
        assert_eq!(steam_id64_to_id3("76561198000000000"), "39734272");
        assert_eq!(steam_id64_to_id3("76561197960265728"), "0");
    }

    #[test]
    fn parses_launch_options_with_quotes() {
        let parsed = parse_launch_options(r#"-novid -high -map dota "+exec autoexec.cfg""#);
        assert_eq!(
            parsed,
            vec!["-novid", "-high", "-map", "dota", "+exec autoexec.cfg"]
        );
    }

    #[test]
    fn matches_only_allowlisted_local_cfg_files() {
        assert!(is_shared_local_cfg_file("user_keys_0_slot0.vcfg"));
        assert!(is_shared_local_cfg_file("user_convars_0_slot0.vcfg_lastclouded"));
        assert!(is_shared_local_cfg_file("MACHINE_CONVARS.VCFG"));
        assert!(is_shared_local_cfg_file("video.txt"));

        assert!(!is_shared_local_cfg_file("remotecache.vdf"));
        assert!(!is_shared_local_cfg_file("stats.dat"));
        assert!(!is_shared_local_cfg_file("user_keys_abc_slot0.vcfg"));
        assert!(!is_shared_local_cfg_file("user_keys_0_slot.vcfg"));
    }

    #[test]
    fn sync_copies_allowlisted_files_and_leaves_remotecache_untouched() {
        let base = std::env::temp_dir().join(format!(
            "switcheroo_sync_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let src_570 = base.join("userdata").join("111").join("570");
        let dst_570 = base.join("userdata").join("222").join("570");
        let backups_root = base.join("backups");

        fs::create_dir_all(src_570.join("remote").join("cfg")).unwrap();
        fs::create_dir_all(src_570.join("local").join("cfg")).unwrap();
        fs::create_dir_all(dst_570.join("remote").join("cfg")).unwrap();

        fs::write(
            src_570.join("remote/cfg/dotakeys_personal.lst"),
            b"master_hotkeys_v1",
        )
        .unwrap();
        fs::write(src_570.join("remote/user_keys.vcfg"), b"master_console_keys").unwrap();
        fs::write(
            src_570.join("local/cfg/user_keys_0_slot0.vcfg"),
            b"slot0_keys",
        )
        .unwrap();
        fs::write(src_570.join("remotecache.vdf"), b"MASTER_REMOTECACHE").unwrap();
        fs::write(src_570.join("stats.dat"), b"MASTER_STATS").unwrap();

        fs::write(
            dst_570.join("remote/cfg/dotakeys_personal.lst"),
            b"old_alt_hotkeys",
        )
        .unwrap();
        fs::write(dst_570.join("remotecache.vdf"), b"ALT_REMOTECACHE_KEEP_ME").unwrap();

        let result = sync_dota_settings_dirs(
            &base,
            "111",
            "222",
            "alt_user",
            "Alt User",
            Some(&backups_root),
        )
        .unwrap();

        assert!(!result.skipped);
        assert!(result.backup_created.is_some());
        assert_eq!(result.files_copied.len(), 3);

        // Target remotecache.vdf must remain untouched and stats.dat must not be copied
        assert_eq!(
            fs::read(dst_570.join("remotecache.vdf")).unwrap(),
            b"ALT_REMOTECACHE_KEEP_ME"
        );
        assert!(!dst_570.join("stats.dat").exists());

        // Hotkeys hash must match source
        assert_eq!(get_hotkeys_hash(&src_570), get_hotkeys_hash(&dst_570));

        // Backup must contain the old target hotkeys
        let backup = result.backup_created.unwrap();
        assert_eq!(
            fs::read(backup.path.join("remote/cfg/dotakeys_personal.lst")).unwrap(),
            b"old_alt_hotkeys"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn copy_shared_files_overwrites_readonly_destination() {
        let base = std::env::temp_dir().join(format!(
            "switcheroo_ro_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src_570 = base.join("src/570");
        let dst_570 = base.join("dst/570");
        fs::create_dir_all(src_570.join("local/cfg")).unwrap();
        fs::create_dir_all(dst_570.join("local/cfg")).unwrap();

        let src_video = src_570.join("local/cfg/video.txt");
        let dst_video = dst_570.join("local/cfg/video.txt");
        fs::write(&src_video, b"new_video_config").unwrap();
        fs::write(&dst_video, b"old_locked_config").unwrap();

        let mut perms = fs::metadata(&dst_video).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&dst_video, perms).unwrap();

        let files = vec!["local/cfg/video.txt".to_string()];
        copy_shared_files_stamped(&src_570, &dst_570, &files).unwrap();

        assert_eq!(fs::read(&dst_video).unwrap(), b"new_video_config");
        assert!(!fs::metadata(&dst_video).unwrap().permissions().readonly());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_skips_when_shared_files_are_already_identical() {
        let base = std::env::temp_dir().join(format!(
            "switcheroo_skip_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src_570 = base.join("userdata/111/570");
        let dst_570 = base.join("userdata/222/570");
        let backups_root = base.join("backups");
        fs::create_dir_all(src_570.join("remote/cfg")).unwrap();
        fs::create_dir_all(dst_570.join("remote/cfg")).unwrap();

        fs::write(src_570.join("remote/cfg/dotakeys_personal.lst"), b"keys_v1").unwrap();
        fs::write(dst_570.join("remote/cfg/dotakeys_personal.lst"), b"keys_v2").unwrap();

        let first =
            sync_dota_settings_dirs(&base, "111", "222", "alt", "Alt", Some(&backups_root))
                .unwrap();
        assert!(!first.skipped);
        assert!(first.backup_created.is_some());

        let second =
            sync_dota_settings_dirs(&base, "111", "222", "alt", "Alt", Some(&backups_root))
                .unwrap();
        assert!(second.skipped);
        assert!(second.backup_created.is_none());
        assert!(second.files_copied.is_empty());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn hotkeys_hash_is_domain_separated_across_files() {
        let base = std::env::temp_dir().join(format!(
            "switcheroo_hash_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dir_a = base.join("a");
        let dir_b = base.join("b");
        fs::create_dir_all(dir_a.join("remote/cfg")).unwrap();
        fs::create_dir_all(dir_b.join("remote/cfg")).unwrap();

        fs::write(dir_a.join("remote/cfg/dotakeys_personal.lst"), b"AB").unwrap();
        fs::write(dir_a.join("remote/user_keys.vcfg"), b"C").unwrap();

        fs::write(dir_b.join("remote/cfg/dotakeys_personal.lst"), b"A").unwrap();
        fs::write(dir_b.join("remote/user_keys.vcfg"), b"BC").unwrap();

        assert_ne!(get_hotkeys_hash(&dir_a), get_hotkeys_hash(&dir_b));

        let _ = fs::remove_dir_all(&base);
    }
}
