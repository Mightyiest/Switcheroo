pub mod components;
pub mod icons;
pub mod modals;
pub mod state;
pub mod theme;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{ColorImage, Frame, Margin, ScrollArea, TextureHandle, TextureOptions, ViewportCommand, vec2};

use crate::backup::{self, BackupEntry};
use crate::config::{self, AppSettings};
use crate::steam::{
    self, DotaDetection, SteamAccount, SteamDetection, SwitchOptions, SwitchResult, SyncAllResult,
    SyncResult,
};
use crate::ui::state::{
    ConfirmAction, ConfirmPrompt, FOOTER_HEIGHT, HEADER_HEIGHT, MIN_WINDOW_HEIGHT, TOOLBAR_HEIGHT,
    Toast, ToastKind, UiAction, WINDOW_WIDTH,
};
use crate::win_platform;

pub enum WorkerEvent {
    Refreshed {
        seq: u64,
        steam: SteamDetection,
        dota: DotaDetection,
        accounts: Vec<SteamAccount>,
    },
    BackupsListed(Vec<BackupEntry>),
    SyncSingleFinished {
        target_persona: String,
        result: Result<SyncResult, String>,
    },
    SyncAllFinished(Result<SyncAllResult, String>),
    SwitchFinished {
        target_account_name: String,
        result: Result<SwitchResult, String>,
    },
    RestoreFinished {
        target_persona: String,
        result: Result<Vec<String>, String>,
    },
}

pub struct SwitcherooApp {
    settings: AppSettings,
    steam: SteamDetection,
    dota: DotaDetection,
    accounts: Vec<SteamAccount>,
    backups: Vec<BackupEntry>,
    avatars: HashMap<String, TextureHandle>,
    avatar_fingerprints: HashMap<String, (u64, u64)>,
    logo_texture: Option<TextureHandle>,

    initial_loading: bool,
    is_busy: bool,
    backups_open: bool,
    about_open: bool,
    confirm_prompt: Option<ConfirmPrompt>,
    toasts: Vec<Toast>,

    next_refresh_seq: u64,
    applied_refresh_seq: u64,
    refresh_spin_until: Option<f64>,
    delayed_refresh_at: Option<Instant>,
    launch_save_at: Option<Instant>,
    last_window_height: f32,

    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
}

impl SwitcherooApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::configure_fonts(&cc.egui_ctx);
        theme::apply_theme(&cc.egui_ctx);

        let logo_texture = decode_texture(
            &cc.egui_ctx,
            "switcheroo_logo",
            include_bytes!("../../icon.png"),
        );

        let settings = config::load_config();
        let (tx, rx) = mpsc::channel();

        let mut app = Self {
            settings,
            steam: SteamDetection::default(),
            dota: DotaDetection::default(),
            accounts: Vec::new(),
            backups: Vec::new(),
            avatars: HashMap::new(),
            avatar_fingerprints: HashMap::new(),
            logo_texture,
            initial_loading: true,
            is_busy: false,
            backups_open: false,
            about_open: false,
            confirm_prompt: None,
            toasts: Vec::new(),
            next_refresh_seq: 0,
            applied_refresh_seq: 0,
            refresh_spin_until: None,
            delayed_refresh_at: None,
            launch_save_at: None,
            last_window_height: 520.0,
            tx,
            rx,
        };

        app.spawn_refresh(&cc.egui_ctx);
        app
    }

    fn push_toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast::new(message, kind));
        if self.toasts.len() > 4 {
            let drain_count = self.toasts.len() - 4;
            self.toasts.drain(0..drain_count);
        }
    }

    fn spawn_refresh(&mut self, ctx: &egui::Context) {
        self.next_refresh_seq = self.next_refresh_seq.saturating_add(1);
        let seq = self.next_refresh_seq;
        let master_id = self.settings.master_steam_id3.clone();
        let fp_cache = self.avatar_fingerprints.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let steam = steam::detect_steam();
            let dota = steam
                .steam_path
                .as_deref()
                .map(steam::detect_dota)
                .unwrap_or_default();
            let accounts = steam
                .steam_path
                .as_deref()
                .filter(|_| steam.found)
                .map(|p| {
                    steam::get_accounts_cached(
                        p,
                        steam.active_steam_id3.as_deref(),
                        master_id.as_deref(),
                        &fp_cache,
                    )
                })
                .unwrap_or_default();

            let _ = tx.send(WorkerEvent::Refreshed {
                seq,
                steam,
                dota,
                accounts,
            });
            ctx.request_repaint();
        });
    }

    fn spawn_list_backups(&self, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let backups = backup::list_backups();
            let _ = tx.send(WorkerEvent::BackupsListed(backups));
            ctx.request_repaint();
        });
    }

    fn poll_worker_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WorkerEvent::Refreshed {
                    seq,
                    steam,
                    dota,
                    accounts,
                } => {
                    if seq < self.applied_refresh_seq {
                        continue;
                    }
                    self.applied_refresh_seq = seq;

                    for acc in &accounts {
                        if let Some(ref av) = acc.decoded_avatar {
                            let image =
                                ColorImage::from_rgba_unmultiplied([av.width, av.height], &av.rgba);
                            let tex = ctx.load_texture(
                                format!("avatar_{}", acc.steam_id64),
                                image,
                                TextureOptions::LINEAR,
                            );
                            self.avatars.insert(acc.steam_id64.clone(), tex);
                            self.avatar_fingerprints
                                .insert(acc.steam_id64.clone(), av.fingerprint);
                        }
                    }

                    self.steam = steam;
                    self.dota = dota;
                    self.accounts = accounts;
                    self.initial_loading = false;

                    let master_valid = self
                        .settings
                        .master_steam_id3
                        .as_deref()
                        .is_some_and(|id| self.accounts.iter().any(|a| a.steam_id3 == id));

                    if !master_valid && !self.accounts.is_empty() {
                        let chosen = self
                            .accounts
                            .iter()
                            .find(|a| a.is_active)
                            .unwrap_or(&self.accounts[0])
                            .steam_id3
                            .clone();
                        self.settings.master_steam_id3 = Some(chosen);
                        let _ = config::save_config(&self.settings);
                    }
                }
                WorkerEvent::BackupsListed(backups) => {
                    self.backups = backups;
                    self.backups_open = true;
                }
                WorkerEvent::SyncSingleFinished {
                    target_persona,
                    result,
                } => {
                    self.is_busy = false;
                    match result {
                        Ok(res) => {
                            if res.skipped {
                                self.push_toast(
                                    format!(
                                        "{target_persona} is already in sync with Master (no changes needed)."
                                    ),
                                    ToastKind::Info,
                                );
                            } else {
                                let backup_note = res
                                    .backup_created
                                    .as_ref()
                                    .map(|b| format!(" (Backup: {})", b.folder_name))
                                    .unwrap_or_default();
                                let _ = res.backup_created.as_ref().map(|b| &b.path);
                                self.push_toast(
                                    format!(
                                        "Synced {} Dota 2 files to {target_persona}!{backup_note}",
                                        res.files_copied.len()
                                    ),
                                    ToastKind::Success,
                                );
                                self.spawn_refresh(ctx);
                            }
                        }
                        Err(err) => {
                            self.push_toast(format!("Sync failed: {err}"), ToastKind::Error);
                        }
                    }
                }
                WorkerEvent::SyncAllFinished(result) => {
                    self.is_busy = false;
                    match result {
                        Ok(res) => {
                            let failures: Vec<_> = res
                                .details
                                .iter()
                                .filter(|d| !d.success)
                                .collect();
                            if failures.is_empty() {
                                if res.synced_count == 0 && res.skipped_count > 0 {
                                    self.push_toast(
                                        format!(
                                            "All {} account(s) are already in sync with Master.",
                                            res.skipped_count
                                        ),
                                        ToastKind::Info,
                                    );
                                } else if res.skipped_count > 0 {
                                    self.push_toast(
                                        format!(
                                            "Synced {} account(s); {} already in sync.",
                                            res.synced_count, res.skipped_count
                                        ),
                                        ToastKind::Success,
                                    );
                                } else {
                                    self.push_toast(
                                        format!(
                                            "Successfully synced to {} account(s)! Backups created.",
                                            res.synced_count
                                        ),
                                        ToastKind::Success,
                                    );
                                }
                            } else {
                                let err_msg = failures[0]
                                    .error
                                    .as_deref()
                                    .unwrap_or("Unknown error");
                                self.push_toast(
                                    format!(
                                        "Synced {} account(s); {} failed ({}: {})",
                                        res.synced_count,
                                        failures.len(),
                                        failures[0].persona_name,
                                        err_msg
                                    ),
                                    ToastKind::Error,
                                );
                            }
                            if res.synced_count > 0 || !failures.is_empty() {
                                self.spawn_refresh(ctx);
                            }
                        }
                        Err(err) => {
                            self.push_toast(format!("Sync all failed: {err}"), ToastKind::Error);
                        }
                    }
                }
                WorkerEvent::SwitchFinished {
                    target_account_name,
                    result,
                } => {
                    self.is_busy = false;
                    match result {
                        Ok(res) => {
                            let msg = if res.already_active && res.launched_dota {
                                format!("Launched Dota 2 directly on @{target_account_name}!")
                            } else if res.already_active {
                                format!("@{target_account_name} is already active.")
                            } else if res.synced && res.launched_dota {
                                format!(
                                    "Settings synced! Switched to @{target_account_name} and launched Dota 2!"
                                )
                            } else if res.launched_dota {
                                format!(
                                    "Switched to @{target_account_name} and launched Dota 2!"
                                )
                            } else {
                                format!("Switched Steam to @{target_account_name}!")
                            };
                            self.push_toast(msg, ToastKind::Success);
                            for warn in res.warnings {
                                self.push_toast(warn, ToastKind::Info);
                            }
                            self.delayed_refresh_at =
                                Some(Instant::now() + Duration::from_millis(2500));
                        }
                        Err(err) => {
                            self.push_toast(format!("Failed: {err}"), ToastKind::Error);
                        }
                    }
                }
                WorkerEvent::RestoreFinished {
                    target_persona,
                    result,
                } => {
                    self.is_busy = false;
                    match result {
                        Ok(_) => {
                            self.push_toast(
                                format!("Settings restored to {target_persona}!"),
                                ToastKind::Success,
                            );
                            self.backups_open = false;
                            self.spawn_refresh(ctx);
                        }
                        Err(err) => {
                            self.push_toast(format!("Restore failed: {err}"), ToastKind::Error);
                        }
                    }
                }
            }
        }
    }

    fn flush_timers(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some(save_at) = self.launch_save_at {
            if now >= save_at {
                self.launch_save_at = None;
                let _ = config::save_config(&self.settings);
            } else {
                ctx.request_repaint_after(save_at - now);
            }
        }

        if let Some(refresh_at) = self.delayed_refresh_at {
            if now >= refresh_at {
                self.delayed_refresh_at = None;
                self.spawn_refresh(ctx);
            } else {
                ctx.request_repaint_after(refresh_at - now);
            }
        }

        self.toasts.retain(|t| !t.is_expired());
    }

    fn process_actions(&mut self, ctx: &egui::Context, actions: Vec<UiAction>) {
        for action in actions {
            match action {
                UiAction::Minimize => {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
                UiAction::Close => {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                UiAction::OpenAbout => {
                    self.about_open = true;
                }
                UiAction::CloseAbout => {
                    self.about_open = false;
                }
                UiAction::OpenBackups => {
                    self.spawn_list_backups(ctx);
                }
                UiAction::CloseBackups => {
                    self.backups_open = false;
                }
                UiAction::Refresh => {
                    self.refresh_spin_until = Some(ctx.input(|i| i.time) + 0.5);
                    self.spawn_refresh(ctx);
                    self.push_toast("Accounts and system status refreshed", ToastKind::Info);
                }
                UiAction::SetMaster(steam_id3) => {
                    self.settings.master_steam_id3 = Some(steam_id3.clone());
                    let _ = config::save_config(&self.settings);
                    let display = self
                        .accounts
                        .iter()
                        .find(|a| a.steam_id3 == steam_id3)
                        .map(|a| a.persona_name.clone())
                        .unwrap_or(steam_id3);
                    self.spawn_refresh(ctx);
                    self.push_toast(
                        format!("Template master set to {display}"),
                        ToastKind::Info,
                    );
                }
                UiAction::LaunchOptionsEdited => {
                    self.launch_save_at = Some(Instant::now() + Duration::from_millis(300));
                }
                UiAction::AppendLaunchChip(val) => {
                    if !components::has_launch_option(&self.settings.launch_options, &val) {
                        let trimmed = self.settings.launch_options.trim();
                        self.settings.launch_options = if trimmed.is_empty() {
                            val.clone()
                        } else {
                            format!("{trimmed} {val}")
                        };
                        self.launch_save_at = None;
                        let _ = config::save_config(&self.settings);
                        self.push_toast(
                            format!("Added {val} to launch options"),
                            ToastKind::Info,
                        );
                    }
                }
                UiAction::SingleSyncClick(idx) => {
                    let Some(target) = self.accounts.get(idx).cloned() else {
                        continue;
                    };
                    let Some(master) = self
                        .settings
                        .master_steam_id3
                        .as_deref()
                        .and_then(|id| self.accounts.iter().find(|a| a.steam_id3 == id))
                        .cloned()
                    else {
                        self.push_toast(
                            "Please select a Master Template account first.",
                            ToastKind::Error,
                        );
                        continue;
                    };

                    let restart_note = if self.steam.is_running {
                        " Steam will close cleanly and restart automatically afterwards."
                    } else {
                        ""
                    };
                    self.confirm_prompt = Some(ConfirmPrompt {
                        title: "Sync Dota 2 Settings".to_string(),
                        body_before: "Clone hotkeys, quickcast, chat wheels and control groups from ".to_string(),
                        highlight_a: master.persona_name,
                        body_mid: " to ".to_string(),
                        highlight_b: target.persona_name.clone(),
                        body_after: format!(
                            "?\n\nA full backup of existing settings will be created automatically before overwriting.{restart_note}"
                        ),
                        action: ConfirmAction::SyncSingle {
                            source_steam_id3: master.steam_id3,
                            target_steam_id3: target.steam_id3,
                            target_account_name: target.account_name,
                            target_persona_name: target.persona_name,
                        },
                    });
                }
                UiAction::SyncAllClick => {
                    let Some(master) = self
                        .settings
                        .master_steam_id3
                        .as_deref()
                        .and_then(|id| self.accounts.iter().find(|a| a.steam_id3 == id))
                        .cloned()
                    else {
                        self.push_toast(
                            "Please select a Master Template account first.",
                            ToastKind::Error,
                        );
                        continue;
                    };

                    let target_count = self
                        .accounts
                        .iter()
                        .filter(|a| a.steam_id3 != master.steam_id3)
                        .count();
                    if target_count == 0 {
                        self.push_toast("No other accounts to sync.", ToastKind::Info);
                        continue;
                    }

                    let restart_note = if self.steam.is_running {
                        " Steam will close cleanly and restart automatically afterwards."
                    } else {
                        ""
                    };
                    self.confirm_prompt = Some(ConfirmPrompt {
                        title: "Sync Master to All Accounts".to_string(),
                        body_before: "Clone all Dota 2 hotkeys & settings from Master ".to_string(),
                        highlight_a: master.persona_name,
                        body_mid: " to all ".to_string(),
                        highlight_b: format!("{target_count} other account(s)"),
                        body_after: format!(
                            "?\n\nBackups of all accounts will be created automatically before syncing.{restart_note}"
                        ),
                        action: ConfirmAction::SyncAll {
                            source_steam_id3: master.steam_id3,
                        },
                    });
                }
                UiAction::SwitchAndPlay(idx) => {
                    if self.is_busy {
                        continue;
                    }
                    let Some(acc) = self.accounts.get(idx).cloned() else {
                        continue;
                    };
                    if !self.steam.found
                        || self.steam.steam_path.is_none()
                        || self.steam.steam_exe.is_none()
                    {
                        self.push_toast("Steam not detected on system.", ToastKind::Error);
                        continue;
                    }

                    if self.launch_save_at.take().is_some() {
                        let _ = config::save_config(&self.settings);
                    }

                    self.is_busy = true;
                    let msg = if acc.is_active {
                        "Launching Dota 2 directly..."
                    } else if self.settings.auto_sync_on_switch
                        && self.settings.master_steam_id3.as_deref()
                            != Some(acc.steam_id3.as_str())
                    {
                        "Syncing settings, switching account & launching Dota 2..."
                    } else {
                        "Switching account & launching Dota 2..."
                    };
                    self.push_toast(msg, ToastKind::Info);

                    let steam_path = self.steam.steam_path.clone().unwrap();
                    let steam_exe = self.steam.steam_exe.clone().unwrap();
                    let opts = SwitchOptions {
                        target_account_name: acc.account_name.clone(),
                        target_steam_id3: acc.steam_id3.clone(),
                        target_persona_name: acc.persona_name.clone(),
                        source_steam_id3: self.settings.master_steam_id3.clone(),
                        auto_sync_settings: self.settings.auto_sync_on_switch,
                        launch_dota: true,
                        launch_options: self.settings.launch_options.trim().to_string(),
                    };

                    let tx = self.tx.clone();
                    let ctx_bg = ctx.clone();
                    let target_name = acc.account_name;
                    std::thread::spawn(move || {
                        let result = steam::switch_account(&steam_path, &steam_exe, &opts);
                        let _ = tx.send(WorkerEvent::SwitchFinished {
                            target_account_name: target_name,
                            result,
                        });
                        ctx_bg.request_repaint();
                    });
                }
                UiAction::OpenAccountFolder(steam_id3) => {
                    if let Some(ref sp) = self.steam.steam_path {
                        let target = sp.join("userdata").join(&steam_id3).join("570");
                        if let Err(err) = win_platform::open_folder(&target) {
                            self.push_toast(
                                format!("Cannot open folder: {err}"),
                                ToastKind::Error,
                            );
                        }
                    } else {
                        self.push_toast("Steam installation not detected.", ToastKind::Error);
                    }
                }
                UiAction::OpenUserdataFolder => {
                    if let Some(ref sp) = self.steam.steam_path {
                        let target = sp.join("userdata");
                        if let Err(err) = win_platform::open_folder(&target) {
                            self.push_toast(
                                format!("Could not open userdata folder: {err}"),
                                ToastKind::Error,
                            );
                        }
                    } else {
                        self.push_toast("Steam installation not detected.", ToastKind::Error);
                    }
                }
                UiAction::OpenBackupsFolder => {
                    let dir = backup::backups_dir();
                    if let Err(err) = win_platform::open_folder(&dir) {
                        self.push_toast(
                            format!("Could not open backups folder: {err}"),
                            ToastKind::Error,
                        );
                    }
                }
                UiAction::OpenUrl(url) => {
                    if let Err(err) = win_platform::open_external_url(&url) {
                        self.push_toast(format!("Failed to open URL: {err}"), ToastKind::Error);
                    }
                }
                UiAction::RestoreBackupClick(backup_name) => {
                    let entry = self
                        .backups
                        .iter()
                        .find(|b| b.name == backup_name)
                        .cloned();
                    let prefix = backup_name
                        .rsplit_once("_backup_")
                        .map(|(p, _)| p)
                        .unwrap_or("");
                    let target = entry
                        .as_ref()
                        .and_then(|e| e.target_steam_id3.as_deref())
                        .and_then(|id3| self.accounts.iter().find(|a| a.steam_id3 == id3))
                        .or_else(|| {
                            entry
                                .as_ref()
                                .and_then(|e| e.target_account_name.as_deref())
                                .and_then(|acc_name| {
                                    self.accounts
                                        .iter()
                                        .find(|a| a.account_name.eq_ignore_ascii_case(acc_name))
                                })
                        })
                        .or_else(|| {
                            self.accounts.iter().find(|a| {
                                a.account_name.eq_ignore_ascii_case(prefix)
                                    || a.steam_id3 == prefix
                            })
                        });
                    let Some(target_acc) = target.cloned() else {
                        self.push_toast(
                            format!("Could not match target account for {backup_name}"),
                            ToastKind::Error,
                        );
                        continue;
                    };

                    let restart_note = if self.steam.is_running {
                        "\n\nA safety pre-restore backup will be created and Steam will restart afterwards."
                    } else {
                        "\n\nA safety pre-restore backup will be created before overwriting."
                    };
                    self.confirm_prompt = Some(ConfirmPrompt {
                        title: "Restore Settings Backup".to_string(),
                        body_before: "Restore settings snapshot ".to_string(),
                        highlight_a: backup_name.clone(),
                        body_mid: " back to ".to_string(),
                        highlight_b: target_acc.persona_name.clone(),
                        body_after: format!("?{restart_note}"),
                        action: ConfirmAction::RestoreBackup {
                            backup_folder_name: backup_name,
                            target_steam_id3: target_acc.steam_id3,
                            target_account_name: target_acc.account_name,
                            target_persona_name: target_acc.persona_name,
                        },
                    });
                }
                UiAction::ConfirmNo => {
                    self.confirm_prompt = None;
                }
                UiAction::ConfirmYes => {
                    let Some(prompt) = self.confirm_prompt.take() else {
                        continue;
                    };
                    if self.is_busy {
                        continue;
                    }
                    let Some(ref steam_path) = self.steam.steam_path else {
                        self.push_toast("Steam not detected on system.", ToastKind::Error);
                        continue;
                    };
                    let steam_path = steam_path.clone();
                    let steam_exe = self.steam.steam_exe.clone();
                    let tx = self.tx.clone();
                    let ctx_bg = ctx.clone();

                    match prompt.action {
                        ConfirmAction::SyncSingle {
                            source_steam_id3,
                            target_steam_id3,
                            target_account_name,
                            target_persona_name,
                        } => {
                            self.is_busy = true;
                            self.push_toast(
                                format!("Syncing Dota 2 settings to {target_persona_name}..."),
                                ToastKind::Info,
                            );
                            std::thread::spawn(move || {
                                let result = steam::sync_dota_settings(
                                    &steam_path,
                                    steam_exe.as_deref(),
                                    &source_steam_id3,
                                    &target_steam_id3,
                                    &target_account_name,
                                    &target_persona_name,
                                );
                                let _ = tx.send(WorkerEvent::SyncSingleFinished {
                                    target_persona: target_persona_name,
                                    result,
                                });
                                ctx_bg.request_repaint();
                            });
                        }
                        ConfirmAction::SyncAll { source_steam_id3 } => {
                            let target_count = self
                                .accounts
                                .iter()
                                .filter(|a| a.steam_id3 != source_steam_id3)
                                .count();
                            self.is_busy = true;
                            self.push_toast(
                                format!("Syncing to {target_count} accounts..."),
                                ToastKind::Info,
                            );
                            std::thread::spawn(move || {
                                let result = steam::sync_to_all(
                                    &steam_path,
                                    steam_exe.as_deref(),
                                    &source_steam_id3,
                                );
                                let _ = tx.send(WorkerEvent::SyncAllFinished(result));
                                ctx_bg.request_repaint();
                            });
                        }
                        ConfirmAction::RestoreBackup {
                            backup_folder_name,
                            target_steam_id3,
                            target_account_name,
                            target_persona_name,
                        } => {
                            self.is_busy = true;
                            std::thread::spawn(move || {
                                let result = backup::restore_backup(
                                    &backup_folder_name,
                                    &target_steam_id3,
                                    &target_account_name,
                                    &target_persona_name,
                                    &steam_path,
                                    steam_exe.as_deref(),
                                );
                                let _ = tx.send(WorkerEvent::RestoreFinished {
                                    target_persona: target_persona_name,
                                    result,
                                });
                                ctx_bg.request_repaint();
                            });
                        }
                    }
                }
            }
        }
    }

    fn auto_fit_window_height(&mut self, ctx: &egui::Context, grid_height: f32) {
        if self.initial_loading {
            return;
        }
        let work_h = win_platform::get_primary_work_area_height(ctx.pixels_per_point());
        let max_h = (work_h - 40.0).max(MIN_WINDOW_HEIGHT);
        let desired = HEADER_HEIGHT + TOOLBAR_HEIGHT + FOOTER_HEIGHT + 16.0 + grid_height;
        let target = desired.clamp(MIN_WINDOW_HEIGHT, max_h).ceil();

        if (target - self.last_window_height).abs() >= 2.0 {
            self.last_window_height = target;
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(WINDOW_WIDTH, target)));
        }
    }
}

impl eframe::App for SwitcherooApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_worker_events(&ctx);
        self.flush_timers(&ctx);

        let mut actions = Vec::new();
        let modal_open =
            self.confirm_prompt.is_some() || self.backups_open || self.about_open;
        let master_account = self
            .settings
            .master_steam_id3
            .as_deref()
            .and_then(|id| self.accounts.iter().find(|a| a.steam_id3 == id))
            .cloned();

        let mut grid_height = 0.0;

        egui::Panel::top("header_panel")
            .exact_size(HEADER_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(theme::BG_HEADER))
            .show(ui, |ui| {
                components::render_header(
                    ui,
                    self.logo_texture.as_ref(),
                    &self.steam,
                    &self.dota,
                    self.is_busy,
                    modal_open,
                    self.refresh_spin_until,
                    &mut actions,
                );
            });

        egui::Panel::top("toolbar_panel")
            .exact_size(TOOLBAR_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(theme::BG_TOOLBAR))
            .show(ui, |ui| {
                components::render_toolbar(
                    ui,
                    master_account.as_ref(),
                    &mut self.settings.launch_options,
                    self.is_busy,
                    &mut actions,
                );
            });

        egui::Panel::bottom("footer_panel")
            .exact_size(FOOTER_HEIGHT)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(theme::BG_HEADER))
            .show(ui, |ui| {
                components::render_footer(ui, self.is_busy, &mut actions);
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(theme::BG_MAIN))
            .show(ui, |ui| {
                components::paint_ambient_background(ui.painter(), ui.max_rect());

                Frame::NONE
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                grid_height = components::render_accounts_grid(
                                    ui,
                                    &self.accounts,
                                    self.settings.master_steam_id3.as_deref(),
                                    &self.avatars,
                                    self.initial_loading,
                                    self.is_busy,
                                    &mut actions,
                                );
                            });
                    });
            });

        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("window_border"),
        ))
        .rect_stroke(
            ctx.content_rect(),
            0.0,
            egui::Stroke::new(1.0, theme::BORDER_SUBTLE),
            egui::StrokeKind::Inside,
        );

        modals::render_modals(
            &ctx,
            self.logo_texture.as_ref(),
            self.backups_open,
            self.about_open,
            self.confirm_prompt.as_ref(),
            &self.backups,
            self.is_busy,
            &mut actions,
        );

        modals::render_toasts(&ctx, &self.toasts);

        self.process_actions(&ctx, actions);
        self.auto_fit_window_height(&ctx, grid_height);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.launch_save_at.take().is_some() {
            let _ = config::save_config(&self.settings);
        }
    }
}

fn decode_texture(
    ctx: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_img = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(name, color_img, TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_accounts() -> Vec<SteamAccount> {
        vec![
            SteamAccount {
                steam_id64: "76561198142526674".to_string(),
                steam_id3: "182260946".to_string(),
                account_name: "owningz2".to_string(),
                persona_name: "t1 fekar P4".to_string(),
                is_active: true,
                decoded_avatar: None,
                has_dota_config: true,
                hotkeys_hash: Some("aaa".to_string()),
                dota_files_count: 66,
            },
            SteamAccount {
                steam_id64: "76561198256934242".to_string(),
                steam_id3: "296668514".to_string(),
                account_name: "13lightning".to_string(),
                persona_name: "Rave".to_string(),
                is_active: false,
                decoded_avatar: None,
                has_dota_config: true,
                hotkeys_hash: Some("bbb".to_string()),
                dota_files_count: 32,
            },
            SteamAccount {
                steam_id64: "76561198242661728".to_string(),
                steam_id3: "282396000".to_string(),
                account_name: "pinkyoshino".to_string(),
                persona_name: "impasta".to_string(),
                is_active: false,
                decoded_avatar: None,
                has_dota_config: true,
                hotkeys_hash: Some("aaa".to_string()),
                dota_files_count: 32,
            },
            SteamAccount {
                steam_id64: "76561198170051426".to_string(),
                steam_id3: "209785698".to_string(),
                account_name: "masterdeadly1".to_string(),
                persona_name: "Yuēhàn 约翰".to_string(),
                is_active: false,
                decoded_avatar: None,
                has_dota_config: true,
                hotkeys_hash: Some("aaa".to_string()),
                dota_files_count: 32,
            },
        ]
    }

    #[test]
    fn ui_layout_fits_800px_width_without_horizontal_overflow() {
        let ctx = egui::Context::default();
        theme::apply_theme(&ctx);

        let steam = SteamDetection {
            found: true,
            is_running: true,
            steam_path: Some(std::path::PathBuf::from(r"C:\Program Files (x86)\Steam")),
            steam_exe: Some(std::path::PathBuf::from(r"C:\Program Files (x86)\Steam\steam.exe")),
            current_auto_login_user: Some("owningz2".to_string()),
            active_steam_id3: Some("182260946".to_string()),
        };
        let dota = DotaDetection {
            installed: true,
            exe_path: Some(std::path::PathBuf::from(r"C:\Steam\dota2.exe")),
        };
        let accounts = sample_accounts();
        let avatars = HashMap::new();
        let mut launch_opts = "-novid -high -map dota".to_string();

        let mut header_rect = egui::Rect::NOTHING;
        let mut toolbar_rect = egui::Rect::NOTHING;
        let mut footer_rect = egui::Rect::NOTHING;
        let mut content_min_rect = egui::Rect::NOTHING;
        let mut grid_h = 0.0_f32;
        let mut grid_w = 0.0_f32;

        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(WINDOW_WIDTH, 460.0),
                )),
                ..Default::default()
            };
            let mut full_output = ctx.run_ui(input, |ui| {
                let mut actions = Vec::new();

                header_rect = egui::Panel::top("header_panel")
                    .exact_size(HEADER_HEIGHT)
                    .show_separator_line(false)
                    .frame(Frame::NONE.fill(theme::BG_HEADER))
                    .show(ui, |ui| {
                        components::render_header(
                            ui,
                            None,
                            &steam,
                            &dota,
                            false,
                            false,
                            None,
                            &mut actions,
                        );
                    })
                    .response
                    .rect;

                toolbar_rect = egui::Panel::top("toolbar_panel")
                    .exact_size(TOOLBAR_HEIGHT)
                    .show_separator_line(false)
                    .frame(Frame::NONE.fill(theme::BG_TOOLBAR))
                    .show(ui, |ui| {
                        components::render_toolbar(
                            ui,
                            accounts.first(),
                            &mut launch_opts,
                            false,
                            &mut actions,
                        );
                    })
                    .response
                    .rect;

                footer_rect = egui::Panel::bottom("footer_panel")
                    .exact_size(FOOTER_HEIGHT)
                    .show_separator_line(false)
                    .frame(Frame::NONE.fill(theme::BG_HEADER))
                    .show(ui, |ui| {
                        components::render_footer(ui, false, &mut actions);
                    })
                    .response
                    .rect;

                egui::CentralPanel::default()
                    .frame(Frame::NONE.fill(theme::BG_MAIN))
                    .show(ui, |ui| {
                        let inner = Frame::NONE
                            .inner_margin(Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                grid_w = ui.available_width();
                                grid_h = components::render_accounts_grid(
                                    ui,
                                    &accounts,
                                    Some("182260946"),
                                    &avatars,
                                    false,
                                    false,
                                    &mut actions,
                                );
                                content_min_rect = ui.min_rect();
                            });
                        let _ = inner;
                    });
            });
            full_output.textures_delta.clear();
        }

        assert!(header_rect.right() <= WINDOW_WIDTH + 0.5);
        assert!(toolbar_rect.right() <= WINDOW_WIDTH + 0.5);
        assert!(footer_rect.right() <= WINDOW_WIDTH + 0.5);
        assert!(
            content_min_rect.right() <= WINDOW_WIDTH - 12.0 + 0.5,
            "Grid overflowed right margin: {:?}",
            content_min_rect
        );
        assert!((grid_w - (WINDOW_WIDTH - 24.0)).abs() <= 0.5);
        // 2 rows of vertical cards must be at least 260px tall (not collapsed horizontally)
        assert!(
            (260.0..=380.0).contains(&grid_h),
            "Unexpected grid height: {grid_h}"
        );
    }
}
