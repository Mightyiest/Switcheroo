use std::time::{Duration, Instant};

pub const WINDOW_WIDTH: f32 = 800.0;
pub const MIN_WINDOW_HEIGHT: f32 = 400.0;
pub const HEADER_HEIGHT: f32 = 44.0;
pub const TOOLBAR_HEIGHT: f32 = 38.0;
pub const FOOTER_HEIGHT: f32 = 30.0;

pub const LAUNCH_CHIP_PRESETS: &[&str] = &["-novid", "-high", "-map dota", "-dx11"];

#[derive(Debug, Clone)]
pub enum UiAction {
    Minimize,
    Close,
    OpenAbout,
    CloseAbout,
    OpenBackups,
    CloseBackups,
    Refresh,
    SyncAllClick,
    SetMaster(String),
    SingleSyncClick(usize),
    SwitchAndPlay(usize),
    OpenAccountFolder(String),
    OpenUserdataFolder,
    OpenBackupsFolder,
    OpenUrl(String),
    LaunchOptionsEdited,
    AppendLaunchChip(String),
    RestoreBackupClick(String),
    ConfirmYes,
    ConfirmNo,
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    SyncSingle {
        source_steam_id3: String,
        target_steam_id3: String,
        target_account_name: String,
        target_persona_name: String,
    },
    SyncAll {
        source_steam_id3: String,
    },
    RestoreBackup {
        backup_folder_name: String,
        target_steam_id3: String,
        target_account_name: String,
        target_persona_name: String,
    },
}

#[derive(Debug, Clone)]
pub struct ConfirmPrompt {
    pub title: String,
    pub body_before: String,
    pub highlight_a: String,
    pub body_mid: String,
    pub highlight_b: String,
    pub body_after: String,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            message: message.into(),
            kind,
            created_at: Instant::now(),
            duration: Duration::from_secs(4),
        }
    }

    pub fn opacity(&self) -> f32 {
        let elapsed = self.created_at.elapsed().as_secs_f32();
        let dur = self.duration.as_secs_f32();
        if elapsed <= dur {
            (elapsed / 0.15).clamp(0.0, 1.0)
        } else {
            (1.0 - (elapsed - dur) / 0.25).clamp(0.0, 1.0)
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration + Duration::from_millis(250)
    }
}
