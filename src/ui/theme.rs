use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

pub const BG_MAIN: Color32 = Color32::from_rgb(9, 10, 16);
pub const BG_HEADER: Color32 = Color32::from_rgb(11, 12, 19);
pub const BG_TOOLBAR: Color32 = Color32::from_rgb(16, 17, 28);
pub const BG_CARD: Color32 = Color32::from_rgb(17, 18, 29);
pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(22, 24, 39);
pub const BG_CARD_ACTIVE: Color32 = Color32::from_rgb(19, 23, 40);
pub const BG_SURFACE: Color32 = Color32::from_rgb(24, 26, 43);
pub const BG_INPUT: Color32 = Color32::from_rgb(9, 10, 16);
pub const BG_MODAL: Color32 = Color32::from_rgb(17, 18, 29);

pub const BORDER_SUBTLE: Color32 = Color32::from_rgba_premultiplied(18, 18, 18, 18);
pub const BORDER_HIGHLIGHT: Color32 = Color32::from_rgba_premultiplied(53, 24, 71, 75);
pub const BORDER_ACTIVE_CYAN: Color32 = Color32::from_rgb(0, 229, 255);
pub const DOTA_GOLD: Color32 = Color32::from_rgb(245, 158, 11);
pub const DOTA_RED: Color32 = Color32::from_rgb(239, 68, 68);
pub const DOTA_RED_DARK: Color32 = Color32::from_rgb(185, 28, 28);
pub const DOTA_RED_HOVER: Color32 = Color32::from_rgb(248, 113, 113);

pub const PLAY_GREEN: Color32 = Color32::from_rgb(16, 185, 129);
pub const PLAY_GREEN_DARK: Color32 = Color32::from_rgb(5, 150, 105);
pub const PLAY_GREEN_HOVER: Color32 = Color32::from_rgb(52, 211, 153);

pub const NULL_PURPLE: Color32 = Color32::from_rgb(168, 85, 247);
pub const NULL_PURPLE_LIGHT: Color32 = Color32::from_rgb(192, 132, 252);
pub const STEAM_BLUE: Color32 = Color32::from_rgb(56, 189, 248);
pub const STEAM_BLUE_HOVER: Color32 = Color32::from_rgb(125, 211, 252);

pub const TEXT_MAIN: Color32 = Color32::from_rgb(243, 244, 246);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(156, 163, 175);
pub const TEXT_DIM: Color32 = Color32::from_rgb(107, 114, 128);

pub const TOAST_INFO_BG: Color32 = Color32::from_rgb(28, 19, 42);
pub const TOAST_SUCCESS_BG: Color32 = Color32::from_rgb(15, 36, 28);
pub const TOAST_ERROR_BG: Color32 = Color32::from_rgb(39, 18, 18);

pub fn radius_xs() -> CornerRadius {
    CornerRadius::same(4)
}

pub fn radius_sm() -> CornerRadius {
    CornerRadius::same(6)
}

pub fn radius_md() -> CornerRadius {
    CornerRadius::same(8)
}

pub fn radius_lg() -> CornerRadius {
    CornerRadius::same(12)
}

pub fn radius_pill() -> CornerRadius {
    CornerRadius::same(12)
}

/// Applies Switcheroo's dark theme visuals and widget styling to the `egui::Context`.
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = Style::default();
    let mut visuals = Visuals::dark();

    visuals.window_fill = BG_MODAL;
    visuals.panel_fill = BG_MAIN;
    visuals.extreme_bg_color = BG_INPUT;
    visuals.faint_bg_color = BG_SURFACE;
    visuals.override_text_color = Some(TEXT_MAIN);
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(168, 85, 247, 70);
    visuals.selection.stroke = Stroke::new(1.0, NULL_PURPLE);

    visuals.widgets.noninteractive.bg_fill = BG_CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
    visuals.widgets.noninteractive.corner_radius = radius_sm();

    visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 10);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 10);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
    visuals.widgets.inactive.corner_radius = radius_sm();

    visuals.widgets.hovered.bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 22);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 22);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_HIGHLIGHT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_MAIN);
    visuals.widgets.hovered.corner_radius = radius_sm();

    visuals.widgets.active.bg_fill = Color32::from_rgba_unmultiplied(168, 85, 247, 45);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgba_unmultiplied(168, 85, 247, 45);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, NULL_PURPLE);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.corner_radius = radius_sm();

    style.visuals = visuals;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.bar_inner_margin = 2.0;
    style.spacing.scroll.bar_outer_margin = 2.0;
    style.spacing.tooltip_width = 360.0;

    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}

/// Configures proportional and monospace Windows system fonts while keeping built-in/CJK fallbacks.
pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Ok(segoe_bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert(
            "segoe_ui".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(segoe_bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "segoe_ui".to_owned());
        }
    }

    if let Ok(consola_bytes) = std::fs::read(r"C:\Windows\Fonts\consola.ttf") {
        fonts.font_data.insert(
            "consolas".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(consola_bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.insert(0, "consolas".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}
