use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM,
    RECT,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_TERMINATE, TerminateProcess,
};
use windows_sys::Win32::UI::Shell::{SetCurrentProcessExplicitAppUserModelID, ShellExecuteW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetWindowThreadProcessId, IsIconic, PostMessageW, SPI_GETWORKAREA,
    SW_RESTORE, SW_SHOW, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, SystemParametersInfoW,
    WM_CLOSE,
};

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Sets the Windows taskbar AppUserModelID to match the original application identity.
pub fn set_app_user_model_id() {
    let app_id = wide("Switcheroo.Dota2.Switcher");
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr());
    }
}

/// Holds the named mutex enforcing a single running instance of Switcheroo.
pub struct SingleInstanceGuard {
    handle: HANDLE,
}

impl SingleInstanceGuard {
    /// Acquires the single-instance mutex, or focuses the existing Switcheroo window and returns `None`.
    pub fn acquire() -> Option<Self> {
        let mutex_name = wide("Local\\Switcheroo_SingleInstance_Mutex");
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
        if !handle.is_null() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            focus_existing_window();
            unsafe {
                CloseHandle(handle);
            }
            return None;
        }
        Some(Self { handle })
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

fn focus_existing_window() {
    let title = wide("Switcheroo");
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if !hwnd.is_null() {
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            } else {
                ShowWindow(hwnd, SW_SHOW);
            }
            SetForegroundWindow(hwnd);
        }
    }
}

/// Lists process IDs whose executable filename matches `exe_name` case-insensitively.
pub fn find_pids_by_name(exe_name: &str) -> Vec<u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return Vec::new();
    }

    let mut pids = Vec::new();
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    if unsafe { Process32FirstW(snapshot, &mut entry) } != 0 {
        loop {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case(exe_name) {
                pids.push(entry.th32ProcessID);
            }
            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }

    unsafe {
        CloseHandle(snapshot);
    }
    pids
}

/// Returns true if `steam.exe` is currently running.
pub fn is_steam_running() -> bool {
    !find_pids_by_name("steam.exe").is_empty()
}

/// Returns true if `dota2.exe` is currently running.
pub fn is_dota_running() -> bool {
    !find_pids_by_name("dota2.exe").is_empty()
}

unsafe extern "system" fn enum_close_steam_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    let target_pids = unsafe { &*(lparam as *const Vec<u32>) };
    if target_pids.contains(&pid) {
        unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
    1
}

/// Gracefully closes Steam (via `steam.exe -shutdown` when available, otherwise `WM_CLOSE`),
/// falling back to `TerminateProcess` if needed, and verifies Steam actually exited.
pub fn kill_steam(steam_exe: Option<&Path>) -> Result<(), String> {
    if !is_steam_running() {
        return Ok(());
    }

    let mut shutdown_sent = false;
    if let Some(exe) = steam_exe.filter(|p| p.is_file()) {
        let args = vec!["-shutdown".to_string()];
        if spawn_detached(exe, &args).is_ok() {
            shutdown_sent = true;
            std::thread::sleep(Duration::from_millis(300));
            for _ in 0..24 {
                if !is_steam_running() {
                    std::thread::sleep(Duration::from_millis(250));
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }

    if !shutdown_sent {
        let pids = find_pids_by_name("steam.exe");
        if pids.is_empty() {
            return Ok(());
        }

        unsafe {
            EnumWindows(
                Some(enum_close_steam_window),
                (&raw const pids) as LPARAM,
            );
        }

        for _ in 0..6 {
            std::thread::sleep(Duration::from_millis(400));
            if !is_steam_running() {
                std::thread::sleep(Duration::from_millis(250));
                return Ok(());
            }
        }
    }

    let remaining = find_pids_by_name("steam.exe");
    if remaining.is_empty() {
        std::thread::sleep(Duration::from_millis(250));
        return Ok(());
    }

    for pid in remaining {
        let proc_handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if !proc_handle.is_null() {
            unsafe {
                TerminateProcess(proc_handle, 1);
                CloseHandle(proc_handle);
            }
        }
    }

    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(200));
        if !is_steam_running() {
            std::thread::sleep(Duration::from_millis(250));
            return Ok(());
        }
    }

    Err("Steam did not exit after graceful shutdown and force termination.".to_string())
}

/// Spawns an external executable completely detached with no console window.
pub fn spawn_detached(exe: &Path, args: &[String]) -> Result<(), String> {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Failed to launch {}: {err}", exe.display()))
}

fn shell_execute_open(target: &str) -> Result<(), String> {
    let verb = wide("open");
    let file = wide(target);
    let ret = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (ret as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "Failed to open '{target}' (ShellExecuteW code {})",
            ret as isize
        ))
    }
}

/// Opens a local directory in Windows File Explorer.
pub fn open_folder(path: &Path) -> Result<(), String> {
    if !path.exists() || !path.is_dir() {
        return Err("Target directory does not exist or is not a folder.".to_string());
    }
    let normalized = path.to_string_lossy().replace('/', "\\");
    shell_execute_open(&normalized)
}

/// Opens an external `https://`, `http://`, or `steam://` URL in the default handler.
pub fn open_external_url(url: &str) -> Result<(), String> {
    if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("steam://") {
        shell_execute_open(url)
    } else {
        Err("Unsupported URL scheme.".to_string())
    }
}

/// Returns the primary display work-area height in logical `egui` points.
pub fn get_primary_work_area_height(pixels_per_point: f32) -> f32 {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&raw mut rect).cast(), 0)
    };
    if ok != 0 && rect.bottom > rect.top {
        (rect.bottom - rect.top) as f32 / pixels_per_point.max(0.5)
    } else {
        900.0
    }
}
