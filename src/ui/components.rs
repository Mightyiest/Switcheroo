use std::collections::HashMap;

use egui::{
    Align, Color32, CornerRadius, CursorIcon, FontId, Frame, Label, Layout, Margin, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, TextEdit, TextureHandle, Ui, ViewportCommand, pos2,
    vec2,
};

use crate::steam::{DotaDetection, SteamAccount, SteamDetection};
use crate::ui::icons;
use crate::ui::state::{
    FOOTER_HEIGHT, HEADER_HEIGHT, LAUNCH_CHIP_PRESETS, TOOLBAR_HEIGHT, UiAction,
};
use crate::ui::theme::*;

/// Paints the subtle radial purple/red ambient background glow behind the main content.
pub fn paint_ambient_background(painter: &egui::Painter, rect: Rect) {
    let top_left = pos2(rect.left() + rect.width() * 0.1, rect.top());
    for (r, alpha) in [(320.0, 7), (220.0, 9), (130.0, 10)] {
        painter.circle_filled(
            top_left,
            r,
            Color32::from_rgba_unmultiplied(NULL_PURPLE.r(), NULL_PURPLE.g(), NULL_PURPLE.b(), alpha),
        );
    }

    let bottom_right = pos2(rect.left() + rect.width() * 0.9, rect.bottom());
    for (r, alpha) in [(340.0, 6), (220.0, 8), (120.0, 8)] {
        painter.circle_filled(
            bottom_right,
            r,
            Color32::from_rgba_unmultiplied(DOTA_RED.r(), DOTA_RED.g(), DOTA_RED.b(), alpha),
        );
    }
}

/// Renders the custom frameless 44px header with brand, status chips, actions, and window buttons.
#[allow(clippy::too_many_arguments)]
pub fn render_header(
    ui: &mut Ui,
    logo: Option<&TextureHandle>,
    steam: &SteamDetection,
    dota: &DotaDetection,
    is_busy: bool,
    modal_open: bool,
    refresh_spin_until: Option<f64>,
    actions: &mut Vec<UiAction>,
) {
    let (header_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), HEADER_HEIGHT),
        Sense::hover(),
    );
    ui.painter().rect_filled(header_rect, 0.0, BG_HEADER);
    ui.painter().line_segment(
        [
            pos2(header_rect.left(), header_rect.bottom()),
            pos2(header_rect.right(), header_rect.bottom()),
        ],
        Stroke::new(1.0, BORDER_SUBTLE),
    );

    let mut any_interactive_hovered = false;
    let inner_rect = header_rect.shrink2(vec2(12.0, 0.0));

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    // 1. Left: Brand (clickable -> About)
    let brand_resp = child
        .scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if let Some(tex) = logo {
                    let (img_rect, _) =
                        ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                    ui.painter().rect_filled(
                        img_rect,
                        radius_sm(),
                        Color32::from_rgba_unmultiplied(168, 85, 247, 35),
                    );
                    ui.put(
                        img_rect,
                        egui::Image::new(tex)
                            .fit_to_exact_size(vec2(26.0, 26.0))
                            .corner_radius(6.0),
                    );
                    ui.painter().rect_stroke(
                        img_rect,
                        radius_sm(),
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(168, 85, 247, 90)),
                        StrokeKind::Inside,
                    );
                }

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add(
                        Label::new(
                            RichText::new("SWITCHEROO")
                                .size(15.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .selectable(false),
                    );
                    ui.add(
                        Label::new(
                            RichText::new("DOTA 2 SWITCHER")
                                .size(9.0)
                                .strong()
                                .color(NULL_PURPLE),
                        )
                        .selectable(false),
                    );
                });
            });
        })
        .response;

    if brand_resp.hovered() {
        any_interactive_hovered = true;
        child.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if !is_busy && brand_resp.clicked() {
        actions.push(UiAction::OpenAbout);
    }
    let brand_right = brand_resp.rect.right();
    brand_resp.on_hover_text("Click to view About Switcheroo");

    // 2. Right: Actions + Window Controls (track leftmost edge to center status chips)
    let mut actions_left = inner_rect.right();
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // Close button (always active)
        let (close_resp, close_hovered) = draw_win_button(ui, true, "Close");
        if close_hovered {
            any_interactive_hovered = true;
        }
        if close_resp.clicked() {
            actions.push(UiAction::Close);
        }

        // Minimize button (always active)
        let (min_resp, min_hovered) = draw_win_button(ui, false, "Minimize");
        if min_hovered {
            any_interactive_hovered = true;
        }
        if min_resp.clicked() {
            actions.push(UiAction::Minimize);
        }

        // Divider between app actions & window controls
        ui.add_space(4.0);
        let (div_rect, _) = ui.allocate_exact_size(vec2(1.0, 22.0), Sense::hover());
        ui.painter().rect_filled(div_rect, 0.0, BORDER_SUBTLE);
        ui.add_space(4.0);

        ui.add_enabled_ui(!is_busy, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;

            // Refresh icon button
            let now = ui.input(|i| i.time);
            let spin_angle = match refresh_spin_until {
                Some(until) if now < until => {
                    let progress = 1.0 - ((until - now) / 0.5).clamp(0.0, 1.0);
                    (progress as f32) * std::f32::consts::TAU
                }
                _ => 0.0,
            };
            if refresh_spin_until.is_some_and(|u| now < u) {
                ui.ctx().request_repaint();
            }

            let (ref_rect, ref_resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            let ref_hov = ref_resp.hovered();
            if ref_hov {
                any_interactive_hovered = true;
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            let bg = if ref_hov {
                Color32::from_rgba_unmultiplied(255, 255, 255, 22)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 9)
            };
            ui.painter().rect_filled(ref_rect, radius_sm(), bg);
            ui.painter().rect_stroke(
                ref_rect,
                radius_sm(),
                Stroke::new(1.0, BORDER_SUBTLE),
                StrokeKind::Inside,
            );
            icons::paint_refresh(
                ui.painter(),
                ref_rect.center(),
                14.0,
                if ref_hov { TEXT_MAIN } else { TEXT_MUTED },
                spin_angle,
            );
            if ref_resp.clicked() {
                actions.push(UiAction::Refresh);
            }
            ref_resp.on_hover_text("Refresh accounts");

            // Sync All primary red button
            let (sync_resp, sync_hov) =
                draw_header_button(ui, "Sync All", true, |p, c, col| {
                    icons::paint_lightning(p, c, 12.0, col);
                });
            if sync_hov {
                any_interactive_hovered = true;
            }
            if sync_resp.clicked() {
                actions.push(UiAction::SyncAllClick);
            }
            sync_resp.on_hover_text("Sync Master settings to all accounts");

            // Backups secondary button
            let (bak_resp, bak_hov) =
                draw_header_button(ui, "Backups", false, |p, c, col| {
                    icons::paint_clock(p, c, 12.0, col);
                });
            actions_left = actions_left.min(bak_resp.rect.left());
            if bak_hov {
                any_interactive_hovered = true;
            }
            if bak_resp.clicked() {
                actions.push(UiAction::OpenBackups);
            }
            bak_resp.on_hover_text("View automatic backups");
        });
    });

    // 3. Center: Compact Steam & Dota 2 status chips centered between Brand and Actions
    let steam_w = status_chip_width(ui, "Steam");
    let dota_w = status_chip_width(ui, "Dota 2");
    let group_w = steam_w + 6.0 + dota_w;
    let gap_mid = (brand_right + actions_left) * 0.5;
    let start_x = (gap_mid - group_w * 0.5).max(brand_right + 8.0);
    let center_rect = Rect::from_min_max(
        pos2(start_x, inner_rect.top()),
        pos2((start_x + group_w + 4.0).min(actions_left - 4.0), inner_rect.bottom()),
    );
    let mut center_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(center_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    center_ui.spacing_mut().item_spacing.x = 6.0;

    let steam_tip = steam
        .steam_path
        .as_ref()
        .filter(|_| steam.found)
        .map(|p| {
            let state = if steam.is_running {
                "Running"
            } else {
                "Not running"
            };
            match steam.current_auto_login_user.as_deref() {
                Some(user) => format!("Steam ({state}, AutoLogin: @{user}): {}", p.display()),
                None => format!("Steam ({state}): {}", p.display()),
            }
        })
        .unwrap_or_else(|| "Steam installation not found in registry or default paths".to_string());
    if draw_status_chip(&mut center_ui, "Steam", steam.found, &steam_tip) {
        any_interactive_hovered = true;
    }

    let dota_tip = dota
        .exe_path
        .as_ref()
        .filter(|_| dota.installed)
        .map(|p| format!("Dota 2 ready: {}", p.display()))
        .unwrap_or_else(|| "Dota 2 not detected in Steam library folders".to_string());
    if draw_status_chip(&mut center_ui, "Dota 2", dota.installed, &dota_tip) {
        any_interactive_hovered = true;
    }

    if !modal_open
        && !any_interactive_hovered
        && ui.rect_contains_pointer(header_rect)
        && ui.input(|i| i.pointer.primary_pressed())
    {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

fn status_chip_width(ui: &Ui, label: &str) -> f32 {
    let text_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(11.2),
        TEXT_MUTED,
    );
    10.0 + 6.0 + 5.0 + text_galley.size().x + 10.0
}

fn draw_status_chip(ui: &mut Ui, label: &str, active: bool, tooltip: &str) -> bool {
    let text_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(11.2),
        TEXT_MUTED,
    );
    let width = 10.0 + 6.0 + 5.0 + text_galley.size().x + 10.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());

    ui.painter().rect_filled(
        rect,
        radius_pill(),
        Color32::from_rgba_unmultiplied(255, 255, 255, 8),
    );
    ui.painter().rect_stroke(
        rect,
        radius_pill(),
        Stroke::new(1.0, BORDER_SUBTLE),
        StrokeKind::Inside,
    );

    let dot_center = pos2(rect.left() + 11.0, rect.center().y);
    if active {
        ui.painter().circle_filled(
            dot_center,
            4.5,
            Color32::from_rgba_unmultiplied(PLAY_GREEN.r(), PLAY_GREEN.g(), PLAY_GREEN.b(), 65),
        );
        ui.painter().circle_filled(dot_center, 3.0, PLAY_GREEN);
    } else {
        let pulse = ((ui.input(|i| i.time) * 3.2).sin() * 0.5 + 0.5) as f32;
        let alpha = (150.0 + pulse * 105.0) as u8;
        ui.painter().circle_filled(
            dot_center,
            3.0,
            Color32::from_rgba_unmultiplied(DOTA_GOLD.r(), DOTA_GOLD.g(), DOTA_GOLD.b(), alpha),
        );
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
    }

    ui.painter().galley(
        pos2(
            dot_center.x + 8.0,
            rect.center().y - text_galley.size().y * 0.5,
        ),
        text_galley,
        TEXT_MUTED,
    );

    let hovered = resp.hovered();
    resp.on_hover_text(tooltip);
    hovered
}

fn draw_header_button(
    ui: &mut Ui,
    label: &str,
    primary: bool,
    paint_icon: impl FnOnce(&egui::Painter, Pos2, Color32),
) -> (egui::Response, bool) {
    let font = FontId::proportional(11.5);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::WHITE);
    let width = 9.0 + 13.0 + 5.0 + galley.size().x + 9.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let (fill, stroke, text_col) = if primary {
        (
            if hovered { DOTA_RED_HOVER } else { DOTA_RED },
            Color32::TRANSPARENT,
            Color32::WHITE,
        )
    } else {
        (
            if hovered {
                Color32::from_rgba_unmultiplied(255, 255, 255, 20)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 10)
            },
            if hovered { BORDER_HIGHLIGHT } else { BORDER_SUBTLE },
            if hovered { TEXT_MAIN } else { TEXT_MUTED },
        )
    };

    if primary {
        ui.painter().rect_filled(
            rect.expand(1.0),
            radius_sm(),
            Color32::from_rgba_unmultiplied(DOTA_RED.r(), DOTA_RED.g(), DOTA_RED.b(), 35),
        );
    }
    ui.painter().rect_filled(rect, radius_sm(), fill);
    if stroke != Color32::TRANSPARENT {
        ui.painter()
            .rect_stroke(rect, radius_sm(), Stroke::new(1.0, stroke), StrokeKind::Inside);
    }

    let icon_center = pos2(rect.left() + 14.5, rect.center().y);
    paint_icon(ui.painter(), icon_center, text_col);

    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(11.5),
        text_col,
    );
    ui.painter().galley(
        pos2(rect.left() + 25.0, rect.center().y - galley.size().y * 0.5),
        galley,
        text_col,
    );

    (resp, hovered)
}

fn draw_win_button(ui: &mut Ui, is_close: bool, tooltip: &str) -> (egui::Response, bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    if hovered {
        let fill = if is_close {
            DOTA_RED
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 26)
        };
        ui.painter().rect_filled(rect, radius_xs(), fill);
    }

    let icon_col = if hovered { Color32::WHITE } else { TEXT_MUTED };
    if is_close {
        icons::paint_close(ui.painter(), rect.center(), icon_col);
    } else {
        icons::paint_minimize(ui.painter(), rect.center(), icon_col);
    }

    resp.clone().on_hover_text(tooltip);
    (resp, hovered)
}

/// Renders the 38px toolbar containing the Master badge, Launch options input, and preset chips.
pub fn render_toolbar(
    ui: &mut Ui,
    master_account: Option<&SteamAccount>,
    launch_options: &mut String,
    is_busy: bool,
    actions: &mut Vec<UiAction>,
) {
    let (bar_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), TOOLBAR_HEIGHT),
        Sense::hover(),
    );
    ui.painter().rect_filled(bar_rect, 0.0, BG_TOOLBAR);
    ui.painter().line_segment(
        [
            pos2(bar_rect.left(), bar_rect.bottom()),
            pos2(bar_rect.right(), bar_rect.bottom()),
        ],
        Stroke::new(1.0, BORDER_SUBTLE),
    );

    let inner = bar_rect.shrink2(vec2(14.0, 0.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    // Left: Master badge
    let master_name = master_account
        .map(|a| a.persona_name.as_str())
        .unwrap_or("None Selected");
    let master_tooltip = master_account
        .map(|a| {
            format!(
                "Master Template Account: {} (@{}) • Dota 2 hotkeys & settings source",
                a.persona_name, a.account_name
            )
        })
        .unwrap_or_else(|| {
            "No master template selected. Click \"Make Master\" on an account card.".to_string()
        });

    let master_label_prefix = child.painter().layout_no_wrap(
        "Master: ".to_string(),
        FontId::proportional(11.5),
        TEXT_MAIN,
    );
    let max_name_w = 135.0;
    let master_name_galley = child.painter().layout(
        master_name.to_string(),
        FontId::proportional(11.5),
        DOTA_GOLD,
        f32::INFINITY,
    );
    let chip_w = (9.0 + 12.0 + 5.0 + master_label_prefix.size().x + master_name_galley.size().x.min(max_name_w) + 10.0)
        .min(220.0);
    let (chip_rect, chip_resp) = child.allocate_exact_size(vec2(chip_w, 24.0), Sense::hover());
    child.painter().rect_filled(
        chip_rect,
        radius_sm(),
        Color32::from_rgba_unmultiplied(DOTA_GOLD.r(), DOTA_GOLD.g(), DOTA_GOLD.b(), 20),
    );
    child.painter().rect_stroke(
        chip_rect,
        radius_sm(),
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(
                DOTA_GOLD.r(),
                DOTA_GOLD.g(),
                DOTA_GOLD.b(),
                if chip_resp.hovered() { 130 } else { 65 },
            ),
        ),
        StrokeKind::Inside,
    );

    icons::paint_star(
        child.painter(),
        pos2(chip_rect.left() + 14.0, chip_rect.center().y),
        11.0,
        DOTA_GOLD,
        true,
    );
    let text_start_x = chip_rect.left() + 24.0;
    child.painter().galley(
        pos2(
            text_start_x,
            chip_rect.center().y - master_label_prefix.size().y * 0.5,
        ),
        master_label_prefix.clone(),
        TEXT_MAIN,
    );
    let name_x = text_start_x + master_label_prefix.size().x;
    let name_clip = Rect::from_min_max(
        pos2(name_x, chip_rect.top()),
        pos2(chip_rect.right() - 8.0, chip_rect.bottom()),
    );
    let name_painter = child.painter().with_clip_rect(name_clip);
    name_painter.galley(
        pos2(name_x, chip_rect.center().y - master_name_galley.size().y * 0.5),
        master_name_galley,
        DOTA_GOLD,
    );
    chip_resp.on_hover_text(master_tooltip);

    // Right: Launch label + monospace input + preset chips
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        ui.add_enabled_ui(!is_busy, |ui| {
            for &preset in LAUNCH_CHIP_PRESETS.iter().rev() {
                let already_present = has_launch_option(launch_options, preset);
                let galley = ui.painter().layout_no_wrap(
                    preset.to_string(),
                    FontId::monospace(10.5),
                    TEXT_MUTED,
                );
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(galley.size().x + 12.0, 21.0), Sense::click());
                let hovered = resp.hovered();
                if hovered {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }

                let fill = if hovered {
                    Color32::from_rgba_unmultiplied(NULL_PURPLE.r(), NULL_PURPLE.g(), NULL_PURPLE.b(), 46)
                } else if already_present {
                    Color32::from_rgba_unmultiplied(NULL_PURPLE.r(), NULL_PURPLE.g(), NULL_PURPLE.b(), 22)
                } else {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 10)
                };
                let stroke_col = if hovered {
                    NULL_PURPLE
                } else if already_present {
                    Color32::from_rgba_unmultiplied(NULL_PURPLE.r(), NULL_PURPLE.g(), NULL_PURPLE.b(), 95)
                } else {
                    BORDER_SUBTLE
                };
                let text_col = if hovered {
                    Color32::WHITE
                } else if already_present {
                    NULL_PURPLE_LIGHT
                } else {
                    TEXT_MUTED
                };

                ui.painter().rect_filled(rect, radius_xs(), fill);
                ui.painter().rect_stroke(
                    rect,
                    radius_xs(),
                    Stroke::new(1.0, stroke_col),
                    StrokeKind::Inside,
                );
                let g = ui.painter().layout_no_wrap(
                    preset.to_string(),
                    FontId::monospace(10.5),
                    text_col,
                );
                ui.painter().galley(
                    pos2(rect.center().x - g.size().x * 0.5, rect.center().y - g.size().y * 0.5),
                    g,
                    text_col,
                );

                if resp.clicked() {
                    actions.push(UiAction::AppendLaunchChip(preset.to_string()));
                }
            }

            ui.add_space(4.0);

            let edit_resp = ui.add(
                TextEdit::singleline(launch_options)
                    .desired_width(195.0)
                    .font(FontId::monospace(11.2))
                    .hint_text("-novid -high -map dota")
                    .margin(Margin::symmetric(8, 3)),
            );
            if edit_resp.changed() {
                actions.push(UiAction::LaunchOptionsEdited);
            }

            ui.add_space(4.0);
            ui.add(
                Label::new(
                    RichText::new("Launch:")
                        .size(11.2)
                        .strong()
                        .color(STEAM_BLUE),
                )
                .selectable(false),
            );
        });
    });
}

pub fn has_launch_option(full_options: &str, option: &str) -> bool {
    if full_options.trim().is_empty() || option.is_empty() {
        return false;
    }
    let haystack = full_options.trim();
    if !option.contains(char::is_whitespace) {
        return haystack.split_whitespace().any(|t| t == option);
    }
    haystack.match_indices(option).any(|(idx, _)| {
        let before_ok = idx == 0 || haystack[..idx].chars().next_back().is_some_and(char::is_whitespace);
        let after_idx = idx + option.len();
        let after_ok = after_idx == haystack.len()
            || haystack[after_idx..].chars().next().is_some_and(char::is_whitespace);
        before_ok && after_ok
    })
}

/// Renders the 2-column accounts grid and returns its total layout height.
pub fn render_accounts_grid(
    ui: &mut Ui,
    accounts: &[SteamAccount],
    master_steam_id3: Option<&str>,
    avatars: &HashMap<String, TextureHandle>,
    initial_loading: bool,
    is_busy: bool,
    actions: &mut Vec<UiAction>,
) -> f32 {
    if initial_loading {
        let resp = ui
            .allocate_ui(vec2(ui.available_width(), 160.0), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(44.0);
                        let (spin_rect, _) =
                            ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                        icons::paint_spinner(
                            ui.painter(),
                            spin_rect.center(),
                            13.0,
                            NULL_PURPLE,
                            ui.input(|i| i.time),
                        );
                        ui.ctx().request_repaint();
                        ui.add_space(10.0);
                        ui.label(RichText::new("Loading accounts...").size(13.0).color(TEXT_MUTED));
                    });
                });
            })
            .response;
        return resp.rect.height();
    }

    if accounts.is_empty() {
        let resp = ui
            .allocate_ui(vec2(ui.available_width(), 150.0), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(44.0);
                    ui.label(
                        RichText::new("No Steam accounts found in loginusers.vdf.")
                            .size(13.5)
                            .strong()
                            .color(TEXT_MAIN),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Log into Steam on this computer at least once to register an account.",
                        )
                        .size(11.5)
                        .color(TEXT_MUTED),
                    );
                });
            })
            .response;
        return resp.rect.height();
    }

    let master_acc = master_steam_id3.and_then(|id| accounts.iter().find(|a| a.steam_id3 == id));
    let col_gap = 8.0;
    let row_gap = 8.0;
    let total_w = ui.available_width();
    let card_w = ((total_w - col_gap) * 0.5).floor().max(200.0);

    let grid_resp = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = row_gap;

        for (row_idx, pair) in accounts.chunks(2).enumerate() {
            let row_nat_id = egui::Id::unique(("acct_row_nat_h", row_idx));
            let target_nat_h = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(row_nat_id))
                .unwrap_or(0.0);
            let mut row_max_nat_h = 0.0_f32;

            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = col_gap;
                for (col_idx, acc) in pair.iter().enumerate() {
                    let global_idx = row_idx * 2 + col_idx;
                    let nat_h = render_account_card(
                        ui,
                        card_w,
                        target_nat_h,
                        global_idx,
                        acc,
                        master_steam_id3,
                        master_acc,
                        avatars.get(&acc.steam_id64),
                        is_busy,
                        actions,
                    );
                    row_max_nat_h = row_max_nat_h.max(nat_h);
                }
            });

            if pair.len() == 2 && row_max_nat_h > 0.0 {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(row_nat_id, row_max_nat_h));
            } else {
                ui.ctx().data_mut(|d| d.remove::<f32>(row_nat_id));
            }
        }
    });

    grid_resp.response.rect.height()
}

#[allow(clippy::too_many_arguments)]
fn render_account_card(
    ui: &mut Ui,
    card_w: f32,
    target_nat_h: f32,
    idx: usize,
    acc: &SteamAccount,
    master_steam_id3: Option<&str>,
    master_acc: Option<&SteamAccount>,
    avatar_tex: Option<&TextureHandle>,
    is_busy: bool,
    actions: &mut Vec<UiAction>,
) -> f32 {
    let is_master = master_steam_id3 == Some(acc.steam_id3.as_str());
    let card_id = ui.make_persistent_id(("acct_card", &acc.steam_id3));
    let nat_h_id = ui.make_persistent_id(("acct_card_nat_h", &acc.steam_id3));
    let was_hovered = ui.ctx().data(|d| d.get_temp::<bool>(card_id)).unwrap_or(false);
    let prev_nat_h = ui.ctx().data(|d| d.get_temp::<f32>(nat_h_id)).unwrap_or(0.0);

    let fill = if acc.is_active {
        BG_CARD_ACTIVE
    } else if was_hovered {
        BG_CARD_HOVER
    } else {
        BG_CARD
    };

    let stroke = if acc.is_active {
        Stroke::new(1.2, Color32::from_rgba_unmultiplied(0, 229, 255, 115))
    } else if is_master {
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(245, 158, 11, 102))
    } else if was_hovered {
        Stroke::new(1.0, BORDER_HIGHLIGHT)
    } else {
        Stroke::new(1.0, BORDER_SUBTLE)
    };

    let inner_w = (card_w - 22.0 - stroke.width * 2.0).max(160.0);
    let spacer = if prev_nat_h > 0.0 && target_nat_h > prev_nat_h {
        (target_nat_h - prev_nat_h).max(0.0)
    } else {
        0.0
    };

    let card_resp = ui
        .allocate_ui_with_layout(vec2(card_w, 0.0), Layout::top_down(Align::Min), |ui| {
            Frame::NONE
                .fill(fill)
                .stroke(stroke)
                .corner_radius(radius_md())
                .inner_margin(Margin::symmetric(11, 9))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(inner_w);
                        ui.set_max_width(inner_w);
                        ui.spacing_mut().item_spacing.y = 7.0;

                        // 1. Account header: Avatar + Persona / @account (id3)
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 9.0;
                            let (av_rect, _) =
                                ui.allocate_exact_size(vec2(38.0, 38.0), Sense::hover());
                            let av_border = if acc.is_active {
                                BORDER_ACTIVE_CYAN
                            } else if is_master {
                                DOTA_GOLD
                            } else {
                                BORDER_SUBTLE
                            };

                            if let Some(tex) = avatar_tex {
                                ui.painter().rect_filled(av_rect, radius_sm(), Color32::BLACK);
                                ui.put(
                                    av_rect,
                                    egui::Image::new(tex)
                                        .fit_to_exact_size(vec2(38.0, 38.0))
                                        .corner_radius(6.0),
                                );
                            } else {
                                ui.painter().rect_filled(
                                    av_rect,
                                    radius_sm(),
                                    Color32::from_rgb(34, 26, 54),
                                );
                                let initial = acc
                                    .persona_name
                                    .chars()
                                    .next()
                                    .or_else(|| acc.account_name.chars().next())
                                    .unwrap_or('S')
                                    .to_uppercase()
                                    .to_string();
                                let g = ui.painter().layout_no_wrap(
                                    initial,
                                    FontId::proportional(17.0),
                                    NULL_PURPLE_LIGHT,
                                );
                                ui.painter().galley(
                                    pos2(
                                        av_rect.center().x - g.size().x * 0.5,
                                        av_rect.center().y - g.size().y * 0.5,
                                    ),
                                    g,
                                    NULL_PURPLE_LIGHT,
                                );
                            }
                            ui.painter().rect_stroke(
                                av_rect,
                                radius_sm(),
                                Stroke::new(1.5, av_border),
                                StrokeKind::Inside,
                            );

                            let id_w = (inner_w - 38.0 - 9.0).max(80.0);
                            ui.allocate_ui_with_layout(
                                vec2(id_w, 38.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    ui.set_max_width(id_w);
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    let name_resp = ui.add(
                                        Label::new(
                                            RichText::new(&acc.persona_name)
                                                .size(14.5)
                                                .strong()
                                                .color(Color32::WHITE),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                    name_resp.on_hover_text(&acc.persona_name);

                                    let id3_label = format!("({})", acc.steam_id3);
                                    let id3_galley = ui.painter().layout_no_wrap(
                                        id3_label.clone(),
                                        FontId::monospace(10.0),
                                        TEXT_DIM,
                                    );
                                    let max_acct_w = (id_w - id3_galley.size().x - 6.0).max(40.0);
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.scope(|ui| {
                                            ui.set_max_width(max_acct_w);
                                            ui.add(
                                                Label::new(
                                                    RichText::new(format!("@{}", acc.account_name))
                                                        .size(11.2)
                                                        .monospace()
                                                        .color(TEXT_MUTED),
                                                )
                                                .truncate()
                                                .selectable(false),
                                            );
                                        });
                                        ui.add(
                                            Label::new(
                                                RichText::new(id3_label)
                                                    .size(10.0)
                                                    .monospace()
                                                    .color(TEXT_DIM),
                                            )
                                            .selectable(false),
                                        );
                                    });
                                },
                            );
                        });

                        // 2. Meta Pills Row
                        ui.horizontal_wrapped(|ui| {
                            ui.set_max_width(inner_w);
                            ui.spacing_mut().item_spacing = vec2(6.0, 4.0);

                            if acc.is_active {
                                draw_meta_pill(
                                    ui,
                                    "Active",
                                    BORDER_ACTIVE_CYAN,
                                    Color32::from_rgba_unmultiplied(0, 229, 255, 20),
                                    Color32::from_rgba_unmultiplied(0, 229, 255, 77),
                                    Some("Account currently active in Steam"),
                                    |p, c, col| {
                                        p.circle_filled(c, 2.8, col);
                                    },
                                );
                            }

                            if is_master {
                                draw_meta_pill(
                                    ui,
                                    "Master",
                                    DOTA_GOLD,
                                    Color32::from_rgba_unmultiplied(245, 158, 11, 14),
                                    Color32::from_rgba_unmultiplied(245, 158, 11, 77),
                                    Some("Selected Master Template account"),
                                    |p, c, col| icons::paint_star(p, c, 9.5, col, true),
                                );

                                let truth_label =
                                    format!("Source of Truth ({} files)", acc.dota_files_count);
                                draw_meta_pill(
                                    ui,
                                    &truth_label,
                                    PLAY_GREEN,
                                    Color32::from_rgba_unmultiplied(16, 185, 129, 14),
                                    Color32::from_rgba_unmultiplied(16, 185, 129, 64),
                                    Some("Master template source"),
                                    |p, c, col| icons::paint_star(p, c, 9.5, col, true),
                                );
                            } else if let Some(master) = master_acc
                                && acc.has_dota_config
                            {
                                let in_sync = acc.hotkeys_hash.is_some()
                                    && acc.hotkeys_hash == master.hotkeys_hash;
                                if in_sync {
                                    draw_meta_pill(
                                        ui,
                                        "Hotkeys In Sync",
                                        PLAY_GREEN,
                                        Color32::from_rgba_unmultiplied(16, 185, 129, 14),
                                        Color32::from_rgba_unmultiplied(16, 185, 129, 64),
                                        Some("Hotkeys match Master profile exactly"),
                                        |p, c, col| icons::paint_check(p, c, 9.5, col),
                                    );
                                } else {
                                    draw_meta_pill(
                                        ui,
                                        "Unsynced Binds",
                                        DOTA_GOLD,
                                        Color32::from_rgba_unmultiplied(245, 158, 11, 14),
                                        Color32::from_rgba_unmultiplied(245, 158, 11, 64),
                                        Some("Keybind files differ from Master"),
                                        |p, c, col| icons::paint_warning(p, c, 9.5, col),
                                    );
                                }
                            } else {
                                draw_meta_pill(
                                    ui,
                                    "No Dota Config",
                                    DOTA_GOLD,
                                    Color32::from_rgba_unmultiplied(245, 158, 11, 14),
                                    Color32::from_rgba_unmultiplied(245, 158, 11, 64),
                                    Some("No local Dota 2 userdata directory found yet"),
                                    |p, c, col| icons::paint_warning(p, c, 9.5, col),
                                );
                            }
                        });

                        if spacer > 0.0 {
                            ui.add_space(spacer);
                        }

                        // 3. Primary + Sub Actions
                        ui.add_enabled_ui(!is_busy, |ui| {
                            ui.vertical(|ui| {
                                ui.set_min_width(inner_w);
                                ui.set_max_width(inner_w);
                                ui.spacing_mut().item_spacing.y = 6.0;

                                if acc.is_active {
                                    let resp = draw_primary_card_button(
                                        ui,
                                        inner_w,
                                        "Launch Dota 2",
                                        PLAY_GREEN,
                                        PLAY_GREEN_HOVER,
                                        PLAY_GREEN_DARK,
                                        |p, c, col| icons::paint_swords(p, c, 12.0, col),
                                    );
                                    if resp.clicked() {
                                        actions.push(UiAction::SwitchAndPlay(idx));
                                    }
                                    resp.on_hover_text(
                                        "Launch Dota 2 immediately without restarting Steam",
                                    );
                                } else {
                                    let resp = draw_primary_card_button(
                                        ui,
                                        inner_w,
                                        "Switch & Play Dota",
                                        DOTA_RED,
                                        DOTA_RED_HOVER,
                                        DOTA_RED_DARK,
                                        |p, c, col| icons::paint_lightning(p, c, 12.0, col),
                                    );
                                    if resp.clicked() {
                                        actions.push(UiAction::SwitchAndPlay(idx));
                                    }
                                    resp.on_hover_text(
                                        "Sync Master settings, switch to this account, and launch Dota 2 in 1 click",
                                    );
                                }

                                // Sub-action buttons (equal width row)
                                let gap = 4.0;
                                let btn_count = if is_master { 2.0 } else { 3.0 };
                                let sub_w = (inner_w - gap * (btn_count - 1.0)) / btn_count;

                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;

                                    let master_btn_label = if is_master {
                                        "Master Template"
                                    } else {
                                        "Make Master"
                                    };
                                    let master_resp = draw_sub_card_button(
                                        ui,
                                        sub_w,
                                        master_btn_label,
                                        is_master,
                                        |p, c, col| icons::paint_star(p, c, 9.5, col, is_master),
                                    );
                                    if master_resp.clicked() {
                                        actions.push(UiAction::SetMaster(acc.steam_id3.clone()));
                                    }
                                    master_resp.on_hover_text("Designate as Master template");

                                    if !is_master {
                                        let sync_resp = draw_sub_card_button(
                                            ui,
                                            sub_w,
                                            "Sync Only",
                                            false,
                                            |p, c, col| icons::paint_lightning(p, c, 9.5, col),
                                        );
                                        if sync_resp.clicked() {
                                            actions.push(UiAction::SingleSyncClick(idx));
                                        }
                                        sync_resp.on_hover_text("Sync settings without launching");
                                    }

                                    let folder_resp = draw_sub_card_button(
                                        ui,
                                        sub_w,
                                        "Folder",
                                        false,
                                        |p, c, col| icons::paint_folder(p, c, 9.5, col),
                                    );
                                    if folder_resp.clicked() {
                                        actions.push(UiAction::OpenAccountFolder(
                                            acc.steam_id3.clone(),
                                        ));
                                    }
                                    folder_resp.on_hover_text("Open userdata folder in Explorer");
                                });
                            });
                        });
                    });
                })
                .response
        })
        .inner;

    if acc.is_active {
        ui.painter().rect_stroke(
            card_resp.rect.expand(1.5),
            CornerRadius::same(9),
            Stroke::new(2.0, Color32::from_rgba_unmultiplied(0, 229, 255, 22)),
            StrokeKind::Outside,
        );
    }

    let natural_h = (card_resp.rect.height() - spacer).max(0.0);
    let hovered_now = ui.rect_contains_pointer(card_resp.rect);
    ui.ctx().data_mut(|d| {
        d.insert_temp(card_id, hovered_now);
        d.insert_temp(nat_h_id, natural_h);
    });
    natural_h
}

fn draw_meta_pill(
    ui: &mut Ui,
    label: &str,
    fg: Color32,
    bg: Color32,
    border: Color32,
    tooltip: Option<&str>,
    paint_icon: impl FnOnce(&egui::Painter, Pos2, Color32),
) {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), FontId::proportional(10.5), fg);
    let w = 7.0 + 10.0 + 4.0 + galley.size().x + 7.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, radius_xs(), bg);
    ui.painter()
        .rect_stroke(rect, radius_xs(), Stroke::new(1.0, border), StrokeKind::Inside);
    paint_icon(
        ui.painter(),
        pos2(rect.left() + 11.5, rect.center().y),
        fg,
    );
    ui.painter().galley(
        pos2(rect.left() + 20.5, rect.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
    if let Some(tip) = tooltip {
        resp.on_hover_text(tip);
    }
}

fn draw_primary_card_button(
    ui: &mut Ui,
    width: f32,
    label: &str,
    base_col: Color32,
    hover_col: Color32,
    active_col: Color32,
    paint_icon: impl FnOnce(&egui::Painter, Pos2, Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let hovered = resp.hovered();
    let pressed = resp.is_pointer_button_down_on();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    ui.painter().rect_filled(
        rect.expand(1.0),
        radius_sm(),
        Color32::from_rgba_unmultiplied(
            base_col.r(),
            base_col.g(),
            base_col.b(),
            if hovered { 55 } else { 32 },
        ),
    );
    let fill = if pressed {
        active_col
    } else if hovered {
        hover_col
    } else {
        base_col
    };
    ui.painter().rect_filled(rect, radius_sm(), fill);

    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(12.5),
        Color32::WHITE,
    );
    let content_w = 13.0 + 6.0 + galley.size().x;
    let start_x = rect.center().x - content_w * 0.5;
    paint_icon(
        ui.painter(),
        pos2(start_x + 6.0, rect.center().y),
        Color32::WHITE,
    );
    ui.painter().galley(
        pos2(start_x + 18.0, rect.center().y - galley.size().y * 0.5),
        galley,
        Color32::WHITE,
    );
    resp
}

fn draw_sub_card_button(
    ui: &mut Ui,
    width: f32,
    label: &str,
    is_active_master: bool,
    paint_icon: impl FnOnce(&egui::Painter, Pos2, Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 23.0), Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let (fill, stroke, text_col) = if is_active_master {
        (
            Color32::from_rgba_unmultiplied(DOTA_GOLD.r(), DOTA_GOLD.g(), DOTA_GOLD.b(), 16),
            Color32::from_rgba_unmultiplied(DOTA_GOLD.r(), DOTA_GOLD.g(), DOTA_GOLD.b(), 77),
            DOTA_GOLD,
        )
    } else if hovered {
        (
            Color32::from_rgba_unmultiplied(255, 255, 255, 16),
            BORDER_HIGHLIGHT,
            TEXT_MAIN,
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(255, 255, 255, 5),
            BORDER_SUBTLE,
            TEXT_DIM,
        )
    };

    ui.painter().rect_filled(rect, radius_xs(), fill);
    ui.painter()
        .rect_stroke(rect, radius_xs(), Stroke::new(1.0, stroke), StrokeKind::Inside);

    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), FontId::proportional(10.5), text_col);
    let content_w = 10.0 + 4.0 + galley.size().x;
    let start_x = rect.center().x - content_w * 0.5;
    paint_icon(ui.painter(), pos2(start_x + 5.0, rect.center().y), text_col);
    ui.painter().galley(
        pos2(start_x + 14.0, rect.center().y - galley.size().y * 0.5),
        galley,
        text_col,
    );

    resp
}

/// Renders the compact 30px footer with Cloud Conflict tip and quick links.
pub fn render_footer(ui: &mut Ui, is_busy: bool, actions: &mut Vec<UiAction>) {
    let (footer_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), FOOTER_HEIGHT),
        Sense::hover(),
    );
    ui.painter().rect_filled(footer_rect, 0.0, BG_HEADER);
    ui.painter().line_segment(
        [
            pos2(footer_rect.left(), footer_rect.top()),
            pos2(footer_rect.right(), footer_rect.top()),
        ],
        Stroke::new(1.0, BORDER_SUBTLE),
    );

    let inner = footer_rect.shrink2(vec2(16.0, 0.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );

    // Left tip
    let (bulb_rect, _) = child.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
    icons::paint_tip_bulb(child.painter(), bulb_rect.center(), 11.0, DOTA_GOLD);
    child.add_space(4.0);
    child.add(
        Label::new(
            RichText::new("If Steam shows Cloud Conflict: pick ")
                .size(10.8)
                .color(TEXT_DIM),
        )
        .selectable(false),
    );
    child.add(
        Label::new(
            RichText::new("Local Files.")
                .size(10.8)
                .strong()
                .color(NULL_PURPLE_LIGHT),
        )
        .selectable(false),
    );

    // Right links
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.add_enabled_ui(!is_busy, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if draw_footer_link(ui, "About").clicked() {
                actions.push(UiAction::OpenAbout);
            }
            ui.add(Label::new(RichText::new("•").size(10.5).color(TEXT_DIM)).selectable(false));
            if draw_footer_link(ui, "Backups").clicked() {
                actions.push(UiAction::OpenBackups);
            }
            ui.add(Label::new(RichText::new("•").size(10.5).color(TEXT_DIM)).selectable(false));
            if draw_footer_link(ui, "Userdata").clicked() {
                actions.push(UiAction::OpenUserdataFolder);
            }
        });
    });
}

fn draw_footer_link(ui: &mut Ui, label: &str) -> egui::Response {
    let resp = ui.add(
        Label::new(RichText::new(label).size(10.8).color(TEXT_MUTED))
            .sense(Sense::click())
            .selectable(false),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        let rect = resp.rect;
        let galley = ui.painter().layout_no_wrap(
            label.to_string(),
            FontId::proportional(10.8),
            NULL_PURPLE_LIGHT,
        );
        ui.painter().rect_filled(rect, 0.0, BG_HEADER);
        ui.painter().galley(rect.min, galley, NULL_PURPLE_LIGHT);
        ui.painter().line_segment(
            [pos2(rect.left(), rect.bottom()), pos2(rect.right(), rect.bottom())],
            Stroke::new(1.0, NULL_PURPLE_LIGHT),
        );
    }
    resp
}
