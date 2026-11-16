#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod backup;
mod config;
mod steam;
mod ui;
mod vdf;
mod win_platform;

use std::sync::Arc;

use ui::SwitcherooApp;
use ui::state::WINDOW_WIDTH;

fn load_window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../icon.png")).ok()?;
    let rgba = img.to_rgba8();
    Some(egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

fn main() -> eframe::Result<()> {
    win_platform::set_app_user_model_id();

    let Some(_instance_guard) = win_platform::SingleInstanceGuard::acquire() else {
        return Ok(());
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Switcheroo")
        .with_inner_size([WINDOW_WIDTH, 520.0])
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
        "Switcheroo",
        options,
        Box::new(|cc| Ok(Box::new(SwitcherooApp::new(cc)))),
    )
}
