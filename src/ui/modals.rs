use egui::{
    Align, Align2, Area, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Label,
    Layout, Margin, Order, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextureHandle, Ui,
    pos2, vec2,
};

use crate::backup::BackupEntry;
use crate::ui::icons;
use crate::ui::state::{ConfirmPrompt, Toast, ToastKind, UiAction};
use crate::ui::theme::*;

/// Renders the Backups, Confirm, and About modals (with Esc & outside-click dismissal).
#[allow(clippy::too_many_arguments)]
pub fn render_modals(
    ctx: &egui::Context,
    logo: Option<&TextureHandle>,
    backups_open: bool,
    about_open: bool,
    confirm_prompt: Option<&ConfirmPrompt>,
    backups: &[BackupEntry],
    is_busy: bool,
    actions: &mut Vec<UiAction>,
) {
    if confirm_prompt.is_none() && !backups_open && !about_open {
        return;
    }

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        if confirm_prompt.is_some() {
            actions.push(UiAction::ConfirmNo);
        } else if backups_open {
            actions.push(UiAction::CloseBackups);
        } else if about_open {
            actions.push(UiAction::CloseAbout);
        }
    }

    let screen_rect = ctx.content_rect();
    let backdrop_resp = Area::new(Id::unique("modal_backdrop"))
        .fixed_pos(screen_rect.min)
        .order(Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen_rect.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 185));
            resp
        })
        .inner;

    if backdrop_resp.clicked() {
        if confirm_prompt.is_some() {
            actions.push(UiAction::ConfirmNo);
        } else if backups_open {
            actions.push(UiAction::CloseBackups);
        } else if about_open {
            actions.push(UiAction::CloseAbout);
        }
    }

    if let Some(prompt) = confirm_prompt {
        render_confirm_modal(ctx, prompt, is_busy, actions);
    } else if backups_open {
        render_backups_modal(ctx, backups, is_busy, actions);
    } else if about_open {
        render_about_modal(ctx, logo, actions);
    }
}

struct ModalSpec<'a> {
    id: &'a str,
    width: f32,
    title: &'a str,
    on_close: UiAction,
}

fn draw_modal_shell(
    ctx: &egui::Context,
    spec: ModalSpec<'_>,
    header_icon: impl FnOnce(&mut Ui),
    actions: &mut Vec<UiAction>,
    body_fn: impl FnOnce(&mut Ui, &mut Vec<UiAction>),
    footer_fn: impl FnOnce(&mut Ui, &mut Vec<UiAction>),
) {
    let ModalSpec {
        id,
        width,
        title,
        on_close,
    } = spec;
    Area::new(Id::unique(id))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .order(Order::Foreground)
        .show(ctx, |ui| {
            let max_h = (ctx.content_rect().height() * 0.84).max(260.0);
            Frame::NONE
                .fill(BG_MODAL)
                .stroke(Stroke::new(1.0, BORDER_HIGHLIGHT))
                .corner_radius(radius_lg())
                .shadow(egui::Shadow {
                    offset: [0, 12],
                    blur: 32,
                    spread: 2,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                })
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.set_max_height(max_h);
                    ui.spacing_mut().item_spacing.y = 0.0;

                    // Header
                    Frame::NONE
                        .inner_margin(Margin::symmetric(16, 11))
                        .show(ui, |ui| {
                            ui.set_width(width - 32.0);
                            ui.horizontal(|ui| {
                                header_icon(ui);
                                ui.add(
                                    Label::new(
                                        RichText::new(title)
                                            .size(14.5)
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .selectable(false),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let (close_rect, close_resp) =
                                        ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                                    if close_resp.hovered() {
                                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                                    }
                                    icons::paint_close(
                                        ui.painter(),
                                        close_rect.center(),
                                        if close_resp.hovered() {
                                            Color32::WHITE
                                        } else {
                                            TEXT_DIM
                                        },
                                    );
                                    if close_resp.clicked() {
                                        actions.push(on_close);
                                    }
                                });
                            });
                        });

                    paint_divider(ui, width);

                    // Body
                    Frame::NONE
                        .inner_margin(Margin::symmetric(16, 14))
                        .show(ui, |ui| {
                            ui.set_width(width - 32.0);
                            body_fn(ui, actions);
                        });

                    paint_divider(ui, width);

                    // Footer
                    Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 52))
                        .corner_radius(CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: 12,
                            se: 12,
                        })
                        .inner_margin(Margin::symmetric(16, 10))
                        .show(ui, |ui| {
                            ui.set_width(width - 32.0);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                footer_fn(ui, actions);
                            });
                        });
                });
        });
}

fn paint_divider(ui: &mut Ui, width: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, BORDER_SUBTLE);
}

fn modal_button(ui: &mut Ui, label: &str, primary: bool) -> egui::Response {
    let g = ui
        .painter()
        .layout_no_wrap(label.to_string(), FontId::proportional(11.8), Color32::WHITE);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 26.0), Sense::click());
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

    ui.painter().rect_filled(rect, radius_sm(), fill);
    if stroke != Color32::TRANSPARENT {
        ui.painter()
            .rect_stroke(rect, radius_sm(), Stroke::new(1.0, stroke), StrokeKind::Inside);
    }
    let g = ui
        .painter()
        .layout_no_wrap(label.to_string(), FontId::proportional(11.8), text_col);
    ui.painter().galley(
        pos2(rect.center().x - g.size().x * 0.5, rect.center().y - g.size().y * 0.5),
        g,
        text_col,
    );
    resp
}

fn render_backups_modal(
    ctx: &egui::Context,
    backups: &[BackupEntry],
    is_busy: bool,
    actions: &mut Vec<UiAction>,
) {
    draw_modal_shell(
        ctx,
        ModalSpec {
            id: "backups_modal",
            width: 470.0,
            title: "Dota 2 Settings Backups",
            on_close: UiAction::CloseBackups,
        },
        |ui| {
            let (icon_r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint_clock(ui.painter(), icon_r.center(), 15.0, TEXT_MAIN);
        },
        actions,
        |ui, actions| {
            if backups.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "No backups created yet. Backups are saved automatically before any sync operation.",
                    )
                    .size(12.2)
                    .color(TEXT_MUTED),
                );
                ui.add_space(8.0);
                return;
            }

            ScrollArea::vertical()
                .max_height(240.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for b in backups {
                        Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 5))
                            .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                            .corner_radius(radius_sm())
                            .inner_margin(Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        ui.set_max_width(ui.available_width() - 80.0);
                                        ui.add(
                                            Label::new(
                                                RichText::new(&b.name)
                                                    .size(12.0)
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            )
                                            .truncate()
                                            .selectable(false),
                                        );
                                        ui.add(
                                            Label::new(
                                                RichText::new(format!(
                                                    "Created: {} • {}",
                                                    b.created_display, b.size_formatted
                                                ))
                                                .size(10.5)
                                                .monospace()
                                                .color(TEXT_DIM),
                                            )
                                            .selectable(false),
                                        );
                                    });

                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add_enabled_ui(!is_busy, |ui| {
                                            if modal_button(ui, "Restore", false).clicked() {
                                                actions.push(UiAction::RestoreBackupClick(
                                                    b.name.clone(),
                                                ));
                                            }
                                        });
                                    });
                                });
                            });
                    }
                });
        },
        |ui, actions| {
            if modal_button(ui, "Done", true).clicked() {
                actions.push(UiAction::CloseBackups);
            }
            if modal_button(ui, "Open Folder", false).clicked() {
                actions.push(UiAction::OpenBackupsFolder);
            }
        },
    );
}

fn render_confirm_modal(
    ctx: &egui::Context,
    prompt: &ConfirmPrompt,
    is_busy: bool,
    actions: &mut Vec<UiAction>,
) {
    draw_modal_shell(
        ctx,
        ModalSpec {
            id: "confirm_modal",
            width: 380.0,
            title: &prompt.title,
            on_close: UiAction::ConfirmNo,
        },
        |_| {},
        actions,
        |ui, _| {
            let mut job = egui::text::LayoutJob::default();
            let normal = egui::TextFormat {
                font_id: FontId::proportional(12.5),
                color: TEXT_MUTED,
                ..Default::default()
            };
            let bold = egui::TextFormat {
                font_id: FontId::proportional(12.5),
                color: Color32::WHITE,
                ..Default::default()
            };

            if !prompt.body_before.is_empty() {
                job.append(&prompt.body_before, 0.0, normal.clone());
            }
            if !prompt.highlight_a.is_empty() {
                job.append(&prompt.highlight_a, 0.0, bold.clone());
            }
            if !prompt.body_mid.is_empty() {
                job.append(&prompt.body_mid, 0.0, normal.clone());
            }
            if !prompt.highlight_b.is_empty() {
                job.append(&prompt.highlight_b, 0.0, bold);
            }
            if !prompt.body_after.is_empty() {
                job.append(&prompt.body_after, 0.0, normal);
            }

            ui.add(Label::new(job).wrap().selectable(false));
        },
        |ui, actions| {
            ui.add_enabled_ui(!is_busy, |ui| {
                if modal_button(ui, "Proceed", true).clicked() {
                    actions.push(UiAction::ConfirmYes);
                }
            });
            if modal_button(ui, "Cancel", false).clicked() {
                actions.push(UiAction::ConfirmNo);
            }
        },
    );
}

fn render_about_modal(
    ctx: &egui::Context,
    logo: Option<&TextureHandle>,
    actions: &mut Vec<UiAction>,
) {
    draw_modal_shell(
        ctx,
        ModalSpec {
            id: "about_modal",
            width: 380.0,
            title: "About Switcheroo",
            on_close: UiAction::CloseAbout,
        },
        |ui| {
            if let Some(tex) = logo {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                ui.put(
                    r,
                    egui::Image::new(tex)
                        .fit_to_exact_size(vec2(16.0, 16.0))
                        .corner_radius(3.0),
                );
            }
        },
        actions,
        |ui, actions| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;

                if let Some(tex) = logo {
                    let (r, _) = ui.allocate_exact_size(vec2(52.0, 52.0), Sense::hover());
                    ui.painter().rect_filled(
                        r.expand(2.0),
                        radius_md(),
                        Color32::from_rgba_unmultiplied(168, 85, 247, 45),
                    );
                    ui.put(
                        r,
                        egui::Image::new(tex)
                            .fit_to_exact_size(vec2(52.0, 52.0))
                            .corner_radius(8.0),
                    );
                    ui.painter().rect_stroke(
                        r,
                        radius_md(),
                        Stroke::new(1.5, Color32::from_rgba_unmultiplied(168, 85, 247, 102)),
                        StrokeKind::Inside,
                    );
                }

                ui.add_space(2.0);
                ui.add(
                    Label::new(
                        RichText::new("SWITCHEROO")
                            .size(18.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .selectable(false),
                );

                // Version pill
                let ver_text = format!("v{}", env!("CARGO_PKG_VERSION"));
                let vg = ui.painter().layout_no_wrap(
                    ver_text.clone(),
                    FontId::monospace(10.8),
                    NULL_PURPLE_LIGHT,
                );
                let (vr, _) = ui.allocate_exact_size(vec2(vg.size().x + 16.0, 20.0), Sense::hover());
                ui.painter().rect_filled(
                    vr,
                    radius_pill(),
                    Color32::from_rgba_unmultiplied(168, 85, 247, 30),
                );
                ui.painter().rect_stroke(
                    vr,
                    radius_pill(),
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(168, 85, 247, 77)),
                    StrokeKind::Inside,
                );
                ui.painter().galley(
                    pos2(vr.center().x - vg.size().x * 0.5, vr.center().y - vg.size().y * 0.5),
                    vg,
                    NULL_PURPLE_LIGHT,
                );

                ui.add(
                    Label::new(
                        RichText::new(
                            "1-Click Steam Account Switcher & Dota 2 Settings Synchronizer (100% Native Rust)",
                        )
                        .size(11.5)
                        .color(TEXT_MUTED),
                    )
                    .wrap()
                    .selectable(false),
                );

                ui.add_space(4.0);

                Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 5))
                    .stroke(Stroke::new(1.0, BORDER_SUBTLE))
                    .corner_radius(radius_sm())
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 8.0;

                            about_link_row(
                                ui,
                                "Author",
                                "Mightyiest ↗",
                                "https://github.com/Mightyiest",
                                actions,
                            );
                            about_link_row(
                                ui,
                                "Repository",
                                "Mightyiest/Switcheroo ↗",
                                "https://github.com/Mightyiest/Switcheroo",
                                actions,
                            );
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("License").size(11.8).color(TEXT_DIM));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new("ISC (Open Source)")
                                            .size(11.2)
                                            .monospace()
                                            .color(TEXT_MAIN),
                                    );
                                });
                            });
                        });
                    });
            });
        },
        |ui, actions| {
            if modal_button(ui, "Done", true).clicked() {
                actions.push(UiAction::CloseAbout);
            }
        },
    );
}

fn about_link_row(
    ui: &mut Ui,
    label: &str,
    link_text: &str,
    url: &str,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.8).color(TEXT_DIM));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let resp = ui.add(
                Label::new(RichText::new(link_text).size(11.8).strong().color(STEAM_BLUE))
                    .sense(Sense::click())
                    .selectable(false),
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                let g = ui.painter().layout_no_wrap(
                    link_text.to_string(),
                    FontId::proportional(11.8),
                    STEAM_BLUE_HOVER,
                );
                ui.painter().rect_filled(resp.rect, 0.0, BG_MODAL);
                ui.painter().galley(resp.rect.min, g, STEAM_BLUE_HOVER);
            }
            if resp.clicked() {
                actions.push(UiAction::OpenUrl(url.to_string()));
            }
        });
    });
}

/// Renders active toast notifications stacked in the bottom-right corner.
pub fn render_toasts(ctx: &egui::Context, toasts: &[Toast]) {
    if toasts.is_empty() {
        return;
    }

    ctx.request_repaint_after(std::time::Duration::from_millis(50));

    Area::new(Id::unique("toast_stack"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -16.0))
        .order(Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            for toast in toasts {
                let alpha = (toast.opacity() * 255.0) as u8;
                if alpha == 0 {
                    continue;
                }
                let (bg_base, border_base, icon_col) = match toast.kind {
                    ToastKind::Info => (
                        TOAST_INFO_BG,
                        Color32::from_rgba_unmultiplied(168, 85, 247, 130),
                        NULL_PURPLE_LIGHT,
                    ),
                    ToastKind::Success => (
                        TOAST_SUCCESS_BG,
                        Color32::from_rgba_unmultiplied(16, 185, 129, 130),
                        PLAY_GREEN,
                    ),
                    ToastKind::Error => (
                        TOAST_ERROR_BG,
                        Color32::from_rgba_unmultiplied(239, 68, 68, 130),
                        DOTA_RED,
                    ),
                };

                let bg = Color32::from_rgba_unmultiplied(
                    bg_base.r(),
                    bg_base.g(),
                    bg_base.b(),
                    ((alpha as u16 * 245) / 255) as u8,
                );
                let border = Color32::from_rgba_unmultiplied(
                    border_base.r(),
                    border_base.g(),
                    border_base.b(),
                    ((alpha as u16 * border_base.a() as u16) / 255) as u8,
                );
                let fg = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
                let ic = Color32::from_rgba_unmultiplied(
                    icon_col.r(),
                    icon_col.g(),
                    icon_col.b(),
                    alpha,
                );

                Frame::NONE
                    .fill(bg)
                    .stroke(Stroke::new(1.0, border))
                    .corner_radius(radius_sm())
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let (ir, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                            match toast.kind {
                                ToastKind::Info => {
                                    icons::paint_info_circle(ui.painter(), ir.center(), 13.0, ic);
                                }
                                ToastKind::Success => {
                                    icons::paint_check(ui.painter(), ir.center(), 12.0, ic);
                                }
                                ToastKind::Error => {
                                    icons::paint_error_circle(ui.painter(), ir.center(), 13.0, ic);
                                }
                            }
                            ui.add(
                                Label::new(RichText::new(&toast.message).size(12.0).color(fg))
                                    .wrap()
                                    .selectable(false),
                            );
                        });
                    });
            }
        });
}
