#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egui::{
    Align, Color32, ColorImage, CornerRadius, CursorIcon, FontId, Frame, Label, Layout,
    Margin, Pos2, RichText, Sense, Stroke, StrokeKind, TextEdit, TextureHandle, TextureOptions,
    Vec2, ViewportBuilder, ViewportCommand, pos2, vec2,
};
use flate2::read::GzDecoder;
use windows::Win32::Foundation::GENERIC_WRITE;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IPersistFile,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, KNOWN_FOLDER_FLAG, SHGetKnownFolderPath,
    ShellLink,
};
use windows::core::{GUID, HSTRING, Interface};
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const PAYLOAD_GZ: &[u8] = include_bytes!(env!("SWITCHEROO_PAYLOAD_FILE"));

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const SETUP_W: f32 = 480.0;
const SETUP_H: f32 = 380.0;

const BG_MAIN: Color32 = Color32::from_rgb(9, 10, 16);
const BG_HEADER: Color32 = Color32::from_rgb(11, 12, 19);
const BG_CARD: Color32 = Color32::from_rgb(17, 18, 29);
const BG_INPUT: Color32 = Color32::from_rgb(9, 10, 16);
const BORDER_SUBTLE: Color32 = Color32::from_rgba_premultiplied(18, 18, 18, 18);
const BORDER_HIGHLIGHT: Color32 = Color32::from_rgba_premultiplied(53, 24, 71, 75);
const DOTA_RED: Color32 = Color32::from_rgb(239, 68, 68);
const DOTA_RED_HOVER: Color32 = Color32::from_rgb(248, 113, 113);
const PLAY_GREEN: Color32 = Color32::from_rgb(16, 185, 129);
const PLAY_GREEN_HOVER: Color32 = Color32::from_rgb(52, 211, 153);
const NULL_PURPLE: Color32 = Color32::from_rgb(168, 85, 247);
const NULL_PURPLE_LIGHT: Color32 = Color32::from_rgb(192, 132, 252);
const TEXT_MAIN: Color32 = Color32::from_rgb(243, 244, 246);
const TEXT_MUTED: Color32 = Color32::from_rgb(156, 163, 175);
const TEXT_DIM: Color32 = Color32::from_rgb(107, 114, 128);

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliArgs {
    silent: bool,
    uninstall: bool,
    extract_only: bool,
    dir: Option<PathBuf>,
}

fn parse_cli_args<I, S>(iter: I) -> CliArgs
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = CliArgs {
        silent: false,
        uninstall: false,
        extract_only: false,
        dir: None,
    };
    let mut it = iter.into_iter().skip(1);
    while let Some(arg) = it.next() {
        let a = arg.as_ref();
        if a.eq_ignore_ascii_case("--silent") || a.eq_ignore_ascii_case("/S") {
            out.silent = true;
        } else if a.eq_ignore_ascii_case("--uninstall") {
            out.uninstall = true;
        } else if a.eq_ignore_ascii_case("--extract-only") {
            out.extract_only = true;
        } else if a.eq_ignore_ascii_case("--dir") {
            if let Some(val) = it.next() {
                let s = val.as_ref().trim();
                if !s.is_empty() {
                    out.dir = Some(PathBuf::from(s));
                }
            }
        } else if let Some(val) = a.strip_prefix("--dir=") {
            let s = val.trim();
            if !s.is_empty() {
                out.dir = Some(PathBuf::from(s));
            }
        }
    }
    out
}

fn default_install_dir() -> PathBuf {
    if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
        let p = PathBuf::from(local_appdata);
        if !p.as_os_str().is_empty() {
            return p.join("Programs").join("Switcheroo");
        }
    }
    PathBuf::from(r"C:\Program Files\Switcheroo")
}

#[derive(Debug, Clone)]
struct InstallConfig {
    install_dir: PathBuf,
    desktop_shortcut: bool,
    start_menu_shortcut: bool,
    extract_only: bool,
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn close_running_switcheroo() {
    let title = wide("Switcheroo");
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if hwnd.is_null() {
        return;
    }

    unsafe {
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }

    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(150));
        let check = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if check.is_null() {
            break;
        }
    }
}

fn decompress_payload(compressed: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = GzDecoder::new(compressed);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("Failed to decompress payload: {e}"))?;
    if out.len() < 2 || &out[..2] != b"MZ" {
        return Err("Embedded payload is not a valid Windows executable.".to_string());
    }
    Ok(out)
}

fn extract_app_bytes() -> Result<Vec<u8>, String> {
    if !PAYLOAD_GZ.is_empty() {
        return decompress_payload(PAYLOAD_GZ);
    }

    // Dev fallback when built directly without SWITCHEROO_PAYLOAD_GZ
    if let Ok(cur) = std::env::current_exe()
        && let Some(dir) = cur.parent()
    {
        let sibling = dir.join("Switcheroo.exe");
        if sibling.is_file() {
            let bytes = fs::read(&sibling)
                .map_err(|e| format!("Failed to read {}: {e}", sibling.display()))?;
            if bytes.len() >= 2 && &bytes[..2] == b"MZ" {
                return Ok(bytes);
            }
        }
    }

    Err("No embedded Switcheroo payload found. Build via Build-Release.ps1.".to_string())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
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
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("payload.tmp");
    let tmp = path.with_file_name(format!("{file_name}.{nanos}-{}.tmp", std::process::id()));

    if let Err(err) = fs::write(&tmp, bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to write {}: {err}", tmp.display()));
    }
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to replace {}: {err}", path.display()));
    }
    Ok(())
}

fn known_folder_path(folder_id: &GUID) -> Option<PathBuf> {
    unsafe {
        let pwstr = SHGetKnownFolderPath(folder_id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let res = pwstr.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(pwstr.as_ptr().cast()));
        res
    }
}

struct ComGuard;

impl ComGuard {
    fn init() -> Result<Self, String> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_ok() {
            Ok(Self)
        } else {
            Err(format!("Failed to initialize COM: {hr:?}"))
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn create_shortcut(
    lnk_path: &Path,
    target_exe: &Path,
    work_dir: &Path,
    description: &str,
) -> Result<(), String> {
    let _com = ComGuard::init()?;
    let _ = GENERIC_WRITE;

    if let Some(parent) = lnk_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create shortcut dir {}: {e}", parent.display()))?;
    }

    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("CoCreateInstance(ShellLink) failed: {e}"))?;

        let target_h = HSTRING::from(target_exe.as_os_str());
        let dir_h = HSTRING::from(work_dir.as_os_str());
        let desc_h = HSTRING::from(description);
        let lnk_h = HSTRING::from(lnk_path.as_os_str());

        link.SetPath(&target_h)
            .map_err(|e| format!("IShellLinkW::SetPath failed: {e}"))?;
        link.SetWorkingDirectory(&dir_h)
            .map_err(|e| format!("IShellLinkW::SetWorkingDirectory failed: {e}"))?;
        link.SetDescription(&desc_h)
            .map_err(|e| format!("IShellLinkW::SetDescription failed: {e}"))?;
        link.SetIconLocation(&target_h, 0)
            .map_err(|e| format!("IShellLinkW::SetIconLocation failed: {e}"))?;

        let persist: IPersistFile = link
            .cast()
            .map_err(|e| format!("Query IPersistFile failed: {e}"))?;
        persist
            .Save(&lnk_h, true)
            .map_err(|e| format!("IPersistFile::Save failed: {e}"))?;
    }

    Ok(())
}

const UNINSTALL_REG_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Switcheroo";

fn register_uninstaller(install_dir: &Path, exe_size_bytes: u64) -> Result<(), String> {
    let app_exe = install_dir.join("Switcheroo.exe");
    let uninst_exe = install_dir.join("Uninstall Switcheroo.exe");

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(UNINSTALL_REG_PATH)
        .map_err(|e| format!("Failed to register uninstaller in registry: {e}"))?;

    let size_kb = (exe_size_bytes / 1024).max(1) as u32;
    let uninstall_cmd = format!("\"{}\" --uninstall", uninst_exe.display());
    let quiet_cmd = format!("\"{}\" --uninstall --silent", uninst_exe.display());

    key.set_value("DisplayName", &"Switcheroo")
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("Publisher", &"Mightyiest")
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("DisplayIcon", &app_exe.to_string_lossy().to_string())
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("InstallLocation", &install_dir.to_string_lossy().to_string())
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("UninstallString", &uninstall_cmd)
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("QuietUninstallString", &quiet_cmd)
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("URLInfoAbout", &"https://github.com/Mightyiest/Switcheroo")
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("NoModify", &1u32)
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("NoRepair", &1u32)
        .map_err(|e| format!("Registry write failed: {e}"))?;
    key.set_value("EstimatedSize", &size_kb)
        .map_err(|e| format!("Registry write failed: {e}"))?;

    Ok(())
}

fn perform_install(cfg: &InstallConfig) -> Result<PathBuf, String> {
    if cfg.install_dir.as_os_str().is_empty() {
        return Err("Installation directory cannot be empty.".to_string());
    }

    close_running_switcheroo();

    let app_bytes = extract_app_bytes()?;
    fs::create_dir_all(&cfg.install_dir)
        .map_err(|e| format!("Failed to create {}: {e}", cfg.install_dir.display()))?;

    let target_exe = cfg.install_dir.join("Switcheroo.exe");
    atomic_write(&target_exe, &app_bytes)?;

    if !cfg.extract_only {
        if let Ok(cur_exe) = std::env::current_exe() {
            let uninst_exe = cfg.install_dir.join("Uninstall Switcheroo.exe");
            if cur_exe != uninst_exe {
                let uninst_bytes = fs::read(&cur_exe)
                    .map_err(|e| format!("Failed to read installer binary: {e}"))?;
                atomic_write(&uninst_exe, &uninst_bytes)?;
            }
        }

        let desc = "Switcheroo - Dota 2 & Steam Account Switcher";
        if cfg.desktop_shortcut
            && let Some(desktop) = known_folder_path(&FOLDERID_Desktop)
        {
            create_shortcut(&desktop.join("Switcheroo.lnk"), &target_exe, &cfg.install_dir, desc)?;
        }
        if cfg.start_menu_shortcut
            && let Some(programs) = known_folder_path(&FOLDERID_Programs)
        {
            create_shortcut(&programs.join("Switcheroo.lnk"), &target_exe, &cfg.install_dir, desc)?;
        }

        register_uninstaller(&cfg.install_dir, app_bytes.len() as u64)?;
    }

    Ok(target_exe)
}

fn perform_uninstall() -> Result<(), String> {
    close_running_switcheroo();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_install_dir = hkcu
        .open_subkey(UNINSTALL_REG_PATH)
        .ok()
        .and_then(|k| k.get_value::<String, _>("InstallLocation").ok())
        .map(PathBuf::from);

    let install_dir = reg_install_dir.or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
    });

    if let Some(desktop) = known_folder_path(&FOLDERID_Desktop) {
        let _ = fs::remove_file(desktop.join("Switcheroo.lnk"));
    }
    if let Some(programs) = known_folder_path(&FOLDERID_Programs) {
        let _ = fs::remove_file(programs.join("Switcheroo.lnk"));
    }

    let _ = hkcu.delete_subkey_all(UNINSTALL_REG_PATH);

    if let Some(dir) = install_dir {
        let app_exe = dir.join("Switcheroo.exe");
        if app_exe.exists() {
            let _ = fs::remove_file(&app_exe);
        }

        if let Ok(cur_exe) = std::env::current_exe() {
            let script = format!(
                "ping 127.0.0.1 -n 2 > nul & del /f /q \"{}\" & rmdir \"{}\"",
                cur_exe.display(),
                dir.display()
            );
            let _ = Command::new("cmd.exe")
                .args(["/C", &script])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
                .spawn();
        }
    }

    Ok(())
}

fn spawn_detached_exe(exe: &Path) -> Result<(), String> {
    Command::new(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to launch {}: {e}", exe.display()))
}

enum Stage {
    Ready,
    Working,
    Completed(PathBuf),
    UninstallPrompt,
    Uninstalled,
}

enum WorkerMsg {
    Installed(Result<PathBuf, String>),
    Uninstalled(Result<(), String>),
}

struct InstallerApp {
    stage: Stage,
    install_dir_str: String,
    desktop_shortcut: bool,
    start_menu_shortcut: bool,
    launch_after: bool,
    error_msg: Option<String>,
    logo: Option<TextureHandle>,
    tx: Sender<WorkerMsg>,
    rx: Receiver<WorkerMsg>,
}

impl InstallerApp {
    fn new(cc: &eframe::CreationContext<'_>, args: CliArgs) -> Self {
        configure_fonts(&cc.egui_ctx);
        apply_dark_theme(&cc.egui_ctx);

        let logo = decode_logo(&cc.egui_ctx);
        let dir = args.dir.unwrap_or_else(default_install_dir);
        let (tx, rx) = mpsc::channel();

        Self {
            stage: if args.uninstall {
                Stage::UninstallPrompt
            } else {
                Stage::Ready
            },
            install_dir_str: dir.to_string_lossy().to_string(),
            desktop_shortcut: true,
            start_menu_shortcut: true,
            launch_after: true,
            error_msg: None,
            logo,
            tx,
            rx,
        }
    }

    fn start_install(&mut self, ctx: &egui::Context) {
        self.error_msg = None;
        self.stage = Stage::Working;
        let cfg = InstallConfig {
            install_dir: PathBuf::from(self.install_dir_str.trim()),
            desktop_shortcut: self.desktop_shortcut,
            start_menu_shortcut: self.start_menu_shortcut,
            extract_only: false,
        };
        let tx = self.tx.clone();
        let ctx_bg = ctx.clone();
        std::thread::spawn(move || {
            let res = perform_install(&cfg);
            let _ = tx.send(WorkerMsg::Installed(res));
            ctx_bg.request_repaint();
        });
    }

    fn start_uninstall(&mut self, ctx: &egui::Context) {
        self.error_msg = None;
        self.stage = Stage::Working;
        let tx = self.tx.clone();
        let ctx_bg = ctx.clone();
        std::thread::spawn(move || {
            let res = perform_uninstall();
            let _ = tx.send(WorkerMsg::Uninstalled(res));
            ctx_bg.request_repaint();
        });
    }
}

impl eframe::App for InstallerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::Installed(Ok(exe_path)) => {
                    self.stage = Stage::Completed(exe_path);
                }
                WorkerMsg::Installed(Err(err)) => {
                    self.error_msg = Some(err);
                    self.stage = Stage::Ready;
                }
                WorkerMsg::Uninstalled(Ok(())) => {
                    self.stage = Stage::Uninstalled;
                }
                WorkerMsg::Uninstalled(Err(err)) => {
                    self.error_msg = Some(err);
                    self.stage = Stage::UninstallPrompt;
                }
            }
        }

        egui::Panel::top("setup_header")
            .exact_size(40.0)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(BG_HEADER))
            .show(ui, |ui| {
                let (header_rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                ui.painter().rect_filled(header_rect, 0.0, BG_HEADER);
                ui.painter().line_segment(
                    [
                        pos2(header_rect.left(), header_rect.bottom()),
                        pos2(header_rect.right(), header_rect.bottom()),
                    ],
                    Stroke::new(1.0, BORDER_SUBTLE),
                );

                let inner = header_rect.shrink2(vec2(12.0, 0.0));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(Layout::left_to_right(Align::Center)),
                );

                if let Some(ref tex) = self.logo {
                    let (r, _) = child.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    child.put(
                        r,
                        egui::Image::new(tex)
                            .fit_to_exact_size(vec2(20.0, 20.0))
                            .corner_radius(4.0),
                    );
                    child.add_space(8.0);
                }
                child.add(
                    Label::new(
                        RichText::new("SWITCHEROO SETUP")
                            .size(12.5)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .selectable(false),
                );

                let mut btn_hovered = false;
                child.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (close_r, close_resp) =
                        ui.allocate_exact_size(vec2(28.0, 26.0), Sense::click());
                    if close_resp.hovered() {
                        btn_hovered = true;
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                        ui.painter().rect_filled(close_r, CornerRadius::same(4), DOTA_RED);
                    }
                    paint_close(
                        ui.painter(),
                        close_r.center(),
                        if close_resp.hovered() {
                            Color32::WHITE
                        } else {
                            TEXT_MUTED
                        },
                    );
                    if close_resp.clicked() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                    }

                    let (min_r, min_resp) =
                        ui.allocate_exact_size(vec2(28.0, 26.0), Sense::click());
                    if min_resp.hovered() {
                        btn_hovered = true;
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                        ui.painter().rect_filled(
                            min_r,
                            CornerRadius::same(4),
                            Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                        );
                    }
                    ui.painter().line_segment(
                        [
                            pos2(min_r.center().x - 5.0, min_r.center().y + 0.5),
                            pos2(min_r.center().x + 5.0, min_r.center().y + 0.5),
                        ],
                        Stroke::new(
                            1.3,
                            if min_resp.hovered() {
                                Color32::WHITE
                            } else {
                                TEXT_MUTED
                            },
                        ),
                    );
                    if min_resp.clicked() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                    }
                });

                if !btn_hovered
                    && ui.rect_contains_pointer(header_rect)
                    && ui.input(|i| i.pointer.primary_pressed())
                {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(BG_MAIN).inner_margin(Margin::symmetric(20, 16)))
            .show(ui, |ui| {
                let bg_rect = ctx.content_rect();
                ui.painter().circle_filled(
                    pos2(bg_rect.left() + 40.0, bg_rect.top() + 50.0),
                    160.0,
                    Color32::from_rgba_unmultiplied(168, 85, 247, 10),
                );
                ui.painter().circle_filled(
                    pos2(bg_rect.right() - 40.0, bg_rect.bottom() - 30.0),
                    150.0,
                    Color32::from_rgba_unmultiplied(239, 68, 68, 8),
                );

                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 12.0;

                    // Hero row
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if let Some(ref tex) = self.logo {
                            let (r, _) =
                                ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                            ui.painter().rect_filled(
                                r.expand(2.0),
                                CornerRadius::same(10),
                                Color32::from_rgba_unmultiplied(168, 85, 247, 40),
                            );
                            ui.put(
                                r,
                                egui::Image::new(tex)
                                    .fit_to_exact_size(vec2(44.0, 44.0))
                                    .corner_radius(8.0),
                            );
                            ui.painter().rect_stroke(
                                r,
                                CornerRadius::same(8),
                                Stroke::new(
                                    1.2,
                                    Color32::from_rgba_unmultiplied(168, 85, 247, 100),
                                ),
                                StrokeKind::Inside,
                            );
                        }

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            ui.add(
                                Label::new(
                                    RichText::new(format!(
                                        "Switcheroo v{}",
                                        env!("CARGO_PKG_VERSION")
                                    ))
                                    .size(17.5)
                                    .strong()
                                    .color(Color32::WHITE),
                                )
                                .selectable(false),
                            );

                            let badge_txt = "100% Native Rust • Free & Open Source";
                            let bg_g = ui.painter().layout_no_wrap(
                                badge_txt.to_string(),
                                FontId::proportional(10.5),
                                NULL_PURPLE_LIGHT,
                            );
                            let (br, _) = ui.allocate_exact_size(
                                vec2(bg_g.size().x + 14.0, 19.0),
                                Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                br,
                                CornerRadius::same(10),
                                Color32::from_rgba_unmultiplied(168, 85, 247, 28),
                            );
                            ui.painter().rect_stroke(
                                br,
                                CornerRadius::same(10),
                                Stroke::new(
                                    1.0,
                                    Color32::from_rgba_unmultiplied(168, 85, 247, 80),
                                ),
                                StrokeKind::Inside,
                            );
                            ui.painter().galley(
                                pos2(
                                    br.center().x - bg_g.size().x * 0.5,
                                    br.center().y - bg_g.size().y * 0.5,
                                ),
                                bg_g,
                                NULL_PURPLE_LIGHT,
                            );
                        });
                    });

                    if let Some(ref err) = self.error_msg {
                        Frame::NONE
                            .fill(Color32::from_rgb(39, 18, 18))
                            .stroke(Stroke::new(
                                1.0,
                                Color32::from_rgba_unmultiplied(239, 68, 68, 140),
                            ))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(10, 7))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(
                                    Label::new(
                                        RichText::new(err).size(11.5).color(Color32::WHITE),
                                    )
                                    .wrap()
                                    .selectable(false),
                                );
                            });
                    }

                    match &self.stage {
                        Stage::Ready => {
                            Frame::NONE
                                .fill(BG_CARD)
                                .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(14, 12))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 8.0;

                                        ui.label(
                                            RichText::new("Installation Directory")
                                                .size(12.0)
                                                .strong()
                                                .color(TEXT_MAIN),
                                        );

                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 8.0;
                                            let input_w = (ui.available_width() - 62.0).max(160.0);
                                            ui.add(
                                                TextEdit::singleline(&mut self.install_dir_str)
                                                    .desired_width(input_w)
                                                    .font(FontId::monospace(11.0))
                                                    .margin(Margin::symmetric(8, 5)),
                                            );
                                            if setup_btn(ui, "Reset", 54.0, false).clicked() {
                                                self.install_dir_str = default_install_dir()
                                                    .to_string_lossy()
                                                    .to_string();
                                            }
                                        });

                                        ui.add_space(2.0);
                                        ui.checkbox(
                                            &mut self.desktop_shortcut,
                                            RichText::new("Create Desktop shortcut")
                                                .size(12.0)
                                                .color(TEXT_MUTED),
                                        );
                                        ui.checkbox(
                                            &mut self.start_menu_shortcut,
                                            RichText::new("Create Start Menu shortcut")
                                                .size(12.0)
                                                .color(TEXT_MUTED),
                                        );
                                        ui.checkbox(
                                            &mut self.launch_after,
                                            RichText::new("Launch Switcheroo after installation")
                                                .size(12.0)
                                                .color(TEXT_MUTED),
                                        );
                                    });
                                });

                            let w = ui.available_width();
                            if primary_setup_btn(ui, w, "Install Switcheroo", DOTA_RED, DOTA_RED_HOVER)
                                .clicked()
                            {
                                self.start_install(&ctx);
                            }
                        }
                        Stage::Working => {
                            Frame::NONE
                                .fill(BG_CARD)
                                .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(16, 36))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new("Installing Switcheroo...")
                                                .size(14.0)
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                        ui.add_space(6.0);
                                        ui.label(
                                            RichText::new(
                                                "Extracting native executable and configuring shortcuts.",
                                            )
                                            .size(11.5)
                                            .color(TEXT_MUTED),
                                        );
                                    });
                                });
                        }
                        Stage::Completed(exe_path) => {
                            let exe_path = exe_path.clone();
                            Frame::NONE
                                .fill(BG_CARD)
                                .stroke(Stroke::new(
                                    1.0,
                                    Color32::from_rgba_unmultiplied(16, 185, 129, 100),
                                ))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(14, 16))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 8.0;
                                        ui.label(
                                            RichText::new("Switcheroo Installed Successfully!")
                                                .size(14.5)
                                                .strong()
                                                .color(PLAY_GREEN),
                                        );
                                        ui.label(
                                            RichText::new(format!(
                                                "Installed to: {}",
                                                exe_path.display()
                                            ))
                                            .size(11.2)
                                            .monospace()
                                            .color(TEXT_MUTED),
                                        );
                                        ui.label(
                                            RichText::new(
                                                "Registered in Windows Installed Apps for clean 1-click uninstall.",
                                            )
                                            .size(11.2)
                                            .color(TEXT_DIM),
                                        );
                                    });
                                });

                            let w = ui.available_width();
                            if primary_setup_btn(ui, w, "Finish", PLAY_GREEN, PLAY_GREEN_HOVER)
                                .clicked()
                            {
                                if self.launch_after {
                                    let _ = spawn_detached_exe(&exe_path);
                                }
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                        }
                        Stage::UninstallPrompt => {
                            Frame::NONE
                                .fill(BG_CARD)
                                .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(14, 16))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 8.0;
                                        ui.label(
                                            RichText::new("Uninstall Switcheroo?")
                                                .size(14.5)
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                        ui.label(
                                            RichText::new(
                                                "This removes the Switcheroo executable, shortcuts, and Windows uninstall entry. Your Steam accounts and Dota 2 backups are untouched.",
                                            )
                                            .size(11.8)
                                            .color(TEXT_MUTED),
                                        );
                                    });
                                });

                            ui.horizontal(|ui| {
                                let w = (ui.available_width() - 8.0) * 0.5;
                                if setup_btn(ui, "Cancel", w, false).clicked() {
                                    ctx.send_viewport_cmd(ViewportCommand::Close);
                                }
                                if primary_setup_btn(ui, w, "Uninstall", DOTA_RED, DOTA_RED_HOVER)
                                    .clicked()
                                {
                                    self.start_uninstall(&ctx);
                                }
                            });
                        }
                        Stage::Uninstalled => {
                            Frame::NONE
                                .fill(BG_CARD)
                                .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(14, 16))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 6.0;
                                        ui.label(
                                            RichText::new("Switcheroo Uninstalled")
                                                .size(14.5)
                                                .strong()
                                                .color(PLAY_GREEN),
                                        );
                                        ui.label(
                                            RichText::new("All application files and shortcuts were removed.")
                                                .size(11.8)
                                                .color(TEXT_MUTED),
                                        );
                                    });
                                });

                            let w = ui.available_width();
                            if primary_setup_btn(ui, w, "Close", PLAY_GREEN, PLAY_GREEN_HOVER)
                                .clicked()
                            {
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                        }
                    }
                });
            });

        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("setup_border"),
        ))
        .rect_stroke(
            ctx.content_rect(),
            0.0,
            Stroke::new(1.0, BORDER_SUBTLE),
            StrokeKind::Inside,
        );
    }
}

fn setup_btn(ui: &mut egui::Ui, label: &str, width: f32, primary: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let fill = if primary {
        if hovered { DOTA_RED_HOVER } else { DOTA_RED }
    } else if hovered {
        Color32::from_rgba_unmultiplied(255, 255, 255, 22)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 10)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    if !primary {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.0, if hovered { BORDER_HIGHLIGHT } else { BORDER_SUBTLE }),
            StrokeKind::Inside,
        );
    }
    let col = if primary || hovered {
        Color32::WHITE
    } else {
        TEXT_MUTED
    };
    let g = ui
        .painter()
        .layout_no_wrap(label.to_string(), FontId::proportional(11.5), col);
    ui.painter().galley(
        pos2(
            rect.center().x - g.size().x * 0.5,
            rect.center().y - g.size().y * 0.5,
        ),
        g,
        col,
    );
    resp
}

fn primary_setup_btn(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    base_col: Color32,
    hover_col: Color32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    ui.painter().rect_filled(
        rect.expand(1.0),
        CornerRadius::same(6),
        Color32::from_rgba_unmultiplied(
            base_col.r(),
            base_col.g(),
            base_col.b(),
            if hovered { 55 } else { 30 },
        ),
    );
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(6),
        if hovered { hover_col } else { base_col },
    );
    let g = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(13.0),
        Color32::WHITE,
    );
    ui.painter().galley(
        pos2(
            rect.center().x - g.size().x * 0.5,
            rect.center().y - g.size().y * 0.5,
        ),
        g,
        Color32::WHITE,
    );
    resp
}

fn paint_close(painter: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.35, color);
    painter.line_segment(
        [
            pos2(center.x - 4.0, center.y - 4.0),
            pos2(center.x + 4.0, center.y + 4.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(center.x + 4.0, center.y - 4.0),
            pos2(center.x - 4.0, center.y + 4.0),
        ],
        stroke,
    );
}

fn apply_dark_theme(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = BG_CARD;
    visuals.panel_fill = BG_MAIN;
    visuals.extreme_bg_color = BG_INPUT;
    visuals.override_text_color = Some(TEXT_MAIN);
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(168, 85, 247, 70);
    visuals.selection.stroke = Stroke::new(1.0, NULL_PURPLE);
    visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 10);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_HIGHLIGHT);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, NULL_PURPLE);
    style.visuals = visuals;
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(segoe_bytes) = fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert(
            "segoe_ui".to_owned(),
            Arc::new(egui::FontData::from_owned(segoe_bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "segoe_ui".to_owned());
        }
    }
    if let Ok(consola_bytes) = fs::read(r"C:\Windows\Fonts\consola.ttf") {
        fonts.font_data.insert(
            "consolas".to_owned(),
            Arc::new(egui::FontData::from_owned(consola_bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.insert(0, "consolas".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

fn decode_logo(ctx: &egui::Context) -> Option<TextureHandle> {
    let img = image::load_from_memory(include_bytes!("../../icon.png")).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_img = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture("setup_logo", color_img, TextureOptions::LINEAR))
}

fn load_window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../../icon.png")).ok()?;
    let rgba = img.to_rgba8();
    Some(egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

fn main() -> eframe::Result<()> {
    let args = parse_cli_args(std::env::args());

    if args.silent {
        if args.uninstall {
            if let Err(err) = perform_uninstall() {
                eprintln!("{err}");
                std::process::exit(1);
            }
            return Ok(());
        }

        let cfg = InstallConfig {
            install_dir: args.dir.unwrap_or_else(default_install_dir),
            desktop_shortcut: !args.extract_only,
            start_menu_shortcut: !args.extract_only,
            extract_only: args.extract_only,
        };
        if let Err(err) = perform_install(&cfg) {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return Ok(());
    }

    let mut viewport = ViewportBuilder::default()
        .with_title("Switcheroo Setup")
        .with_inner_size(Vec2::new(SETUP_W, SETUP_H))
        .with_decorations(false)
        .with_resizable(false)
        .with_maximize_button(false);

    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Switcheroo Setup",
        options,
        Box::new(|cc| Ok(Box::new(InstallerApp::new(cc, args)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    #[test]
    fn parses_cli_flags_and_quoted_dir() {
        let parsed = parse_cli_args([
            "Switcheroo-Setup.exe",
            "--silent",
            "--extract-only",
            "--dir",
            r"C:\Test Path\Switcheroo",
        ]);
        assert!(parsed.silent);
        assert!(parsed.extract_only);
        assert!(!parsed.uninstall);
        assert_eq!(
            parsed.dir,
            Some(PathBuf::from(r"C:\Test Path\Switcheroo"))
        );

        let uninst = parse_cli_args(["Uninstall.exe", "--uninstall", "--dir=D:\\Apps"]);
        assert!(uninst.uninstall);
        assert!(!uninst.silent);
        assert_eq!(uninst.dir, Some(PathBuf::from(r"D:\Apps")));
    }

    #[test]
    fn gzip_payload_round_trip_validates_mz_header() {
        let mut fake_pe = b"MZ".to_vec();
        fake_pe.extend_from_slice(&[0x90, 0x00, 0x03, 0x00, 0xAA, 0xBB]);

        let mut enc = GzEncoder::new(Vec::new(), Compression::best());
        enc.write_all(&fake_pe).unwrap();
        let compressed = enc.finish().unwrap();

        let restored = decompress_payload(&compressed).unwrap();
        assert_eq!(restored, fake_pe);

        let mut bad_enc = GzEncoder::new(Vec::new(), Compression::fast());
        bad_enc.write_all(b"NOT_A_PE").unwrap();
        let bad_gz = bad_enc.finish().unwrap();
        assert!(decompress_payload(&bad_gz).is_err());
    }
}
