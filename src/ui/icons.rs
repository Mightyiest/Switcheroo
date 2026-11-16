use std::f32::consts::TAU;

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

/// Draws a crisp clock icon (Backups button & modal header).
pub fn paint_clock(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.42;
    let stroke = Stroke::new(1.5, color);
    painter.circle_stroke(center, r, stroke);
    painter.line_segment([center, pos2(center.x, center.y - r * 0.55)], stroke);
    painter.line_segment(
        [center, pos2(center.x + r * 0.45, center.y + r * 0.25)],
        stroke,
    );
}

/// Draws a crisp lightning bolt icon (Sync All, Switch & Play, Sync Only).
pub fn paint_lightning(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let s = size * 0.48;
    let pts = [
        pos2(center.x + s * 0.12, center.y - s),
        pos2(center.x - s * 0.65, center.y + s * 0.12),
        pos2(center.x - s * 0.05, center.y + s * 0.12),
        pos2(center.x - s * 0.18, center.y + s),
        pos2(center.x + s * 0.65, center.y - s * 0.12),
        pos2(center.x + s * 0.05, center.y - s * 0.12),
    ];
    painter.add(Shape::convex_polygon(
        vec![pts[0], pts[1], pts[2], pts[5]],
        color,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![pts[5], pts[2], pts[3], pts[4]],
        color,
        Stroke::NONE,
    ));
}

/// Draws a circular refresh icon rotated by `angle_rad`.
pub fn paint_refresh(painter: &Painter, center: Pos2, size: f32, color: Color32, angle_rad: f32) {
    let r = size * 0.36;
    let stroke = Stroke::new(1.5, color);

    let rot = |v: Vec2| -> Pos2 {
        let (sin, cos) = angle_rad.sin_cos();
        pos2(center.x + v.x * cos - v.y * sin, center.y + v.x * sin + v.y * cos)
    };

    for arc_idx in 0..2 {
        let base = arc_idx as f32 * std::f32::consts::PI;
        let steps = 10;
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = base + 0.25 + (i as f32 / steps as f32) * 2.25;
            pts.push(rot(vec2(t.cos() * r, t.sin() * r)));
        }
        for pair in pts.windows(2) {
            painter.line_segment([pair[0], pair[1]], stroke);
        }

        // Arrowhead at end of arc
        if let Some(&tip) = pts.last() {
            let end_t = base + 2.5;
            let head_a = rot(vec2((end_t - 0.35).cos() * (r + 2.6), (end_t - 0.35).sin() * (r + 2.6)));
            let head_b = rot(vec2((end_t - 0.35).cos() * (r - 2.6), (end_t - 0.35).sin() * (r - 2.6)));
            painter.line_segment([tip, head_a], stroke);
            painter.line_segment([tip, head_b], stroke);
        }
    }
}

/// Draws a 5-point star (filled or outlined) for Master badges.
pub fn paint_star(painter: &Painter, center: Pos2, size: f32, color: Color32, filled: bool) {
    let outer_r = size * 0.46;
    let inner_r = outer_r * 0.42;
    let mut pts = Vec::with_capacity(10);
    for i in 0..10 {
        let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * (TAU / 10.0);
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        pts.push(pos2(center.x + angle.cos() * r, center.y + angle.sin() * r));
    }

    if filled {
        // Fan triangles from center so concave star fills cleanly without convex hull issues
        for i in 0..10 {
            let next = (i + 1) % 10;
            painter.add(Shape::convex_polygon(
                vec![center, pts[i], pts[next]],
                color,
                Stroke::NONE,
            ));
        }
    } else {
        let stroke = Stroke::new(1.25, color);
        for i in 0..10 {
            let next = (i + 1) % 10;
            painter.line_segment([pts[i], pts[next]], stroke);
        }
    }
}

/// Draws crossed Dota blades emblem for Launch Dota 2.
pub fn paint_swords(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let s = size * 0.42;
    let blade = Stroke::new(1.8, color);
    let guard = Stroke::new(1.4, color);

    // Sword 1 (bottom-left to top-right)
    painter.line_segment([pos2(center.x - s, center.y + s), pos2(center.x + s, center.y - s)], blade);
    painter.line_segment(
        [
            pos2(center.x - s * 0.85, center.y + s * 0.25),
            pos2(center.x - s * 0.25, center.y + s * 0.85),
        ],
        guard,
    );

    // Sword 2 (bottom-right to top-left)
    painter.line_segment([pos2(center.x + s, center.y + s), pos2(center.x - s, center.y - s)], blade);
    painter.line_segment(
        [
            pos2(center.x + s * 0.85, center.y + s * 0.25),
            pos2(center.x + s * 0.25, center.y + s * 0.85),
        ],
        guard,
    );
}

/// Draws a compact folder icon.
pub fn paint_folder(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let w = size * 0.84;
    let h = size * 0.64;
    let left = center.x - w * 0.5;
    let top = center.y - h * 0.5;
    let body = Rect::from_min_size(pos2(left, top + h * 0.2), vec2(w, h * 0.8));
    let tab = Rect::from_min_size(pos2(left, top), vec2(w * 0.44, h * 0.28));
    let stroke = Stroke::new(1.25, color);
    painter.rect_stroke(tab, 1.5, stroke, egui::StrokeKind::Inside);
    painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
}

/// Draws a crisp checkmark icon.
pub fn paint_check(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let s = size * 0.38;
    let stroke = Stroke::new(1.6, color);
    let p1 = pos2(center.x - s, center.y + s * 0.05);
    let p2 = pos2(center.x - s * 0.2, center.y + s * 0.78);
    let p3 = pos2(center.x + s, center.y - s * 0.7);
    painter.line_segment([p1, p2], stroke);
    painter.line_segment([p2, p3], stroke);
}

/// Draws a compact warning triangle with exclamation mark.
pub fn paint_warning(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.42;
    let top = pos2(center.x, center.y - r);
    let bl = pos2(center.x - r * 0.95, center.y + r * 0.78);
    let br = pos2(center.x + r * 0.95, center.y + r * 0.78);
    let stroke = Stroke::new(1.3, color);
    painter.line_segment([top, bl], stroke);
    painter.line_segment([bl, br], stroke);
    painter.line_segment([br, top], stroke);
    painter.line_segment(
        [pos2(center.x, center.y - r * 0.35), pos2(center.x, center.y + r * 0.2)],
        stroke,
    );
    painter.circle_filled(pos2(center.x, center.y + r * 0.48), 1.0, color);
}

/// Draws a glowing lightbulb / tip icon for the footer.
pub fn paint_tip_bulb(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.3;
    painter.circle_filled(
        pos2(center.x, center.y - size * 0.08),
        r * 1.4,
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 36),
    );
    painter.circle_stroke(pos2(center.x, center.y - size * 0.08), r, Stroke::new(1.3, color));
    painter.line_segment(
        [
            pos2(center.x - r * 0.55, center.y + r * 0.95),
            pos2(center.x + r * 0.55, center.y + r * 0.95),
        ],
        Stroke::new(1.4, color),
    );
}

/// Draws the window minimize line.
pub fn paint_minimize(painter: &Painter, center: Pos2, color: Color32) {
    painter.line_segment(
        [pos2(center.x - 5.0, center.y + 0.5), pos2(center.x + 5.0, center.y + 0.5)],
        Stroke::new(1.3, color),
    );
}

/// Draws the window close X icon.
pub fn paint_close(painter: &Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.35, color);
    painter.line_segment(
        [pos2(center.x - 4.0, center.y - 4.0), pos2(center.x + 4.0, center.y + 4.0)],
        stroke,
    );
    painter.line_segment(
        [pos2(center.x + 4.0, center.y - 4.0), pos2(center.x - 4.0, center.y + 4.0)],
        stroke,
    );
}

/// Draws an info badge icon for toasts.
pub fn paint_info_circle(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.44;
    painter.circle_stroke(center, r, Stroke::new(1.4, color));
    painter.circle_filled(pos2(center.x, center.y - r * 0.42), 1.1, color);
    painter.line_segment(
        [pos2(center.x, center.y - r * 0.1), pos2(center.x, center.y + r * 0.5)],
        Stroke::new(1.4, color),
    );
}

/// Draws an error X badge icon for toasts.
pub fn paint_error_circle(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    let r = size * 0.44;
    let d = r * 0.45;
    painter.circle_stroke(center, r, Stroke::new(1.4, color));
    let stroke = Stroke::new(1.4, color);
    painter.line_segment([pos2(center.x - d, center.y - d), pos2(center.x + d, center.y + d)], stroke);
    painter.line_segment([pos2(center.x + d, center.y - d), pos2(center.x - d, center.y + d)], stroke);
}

/// Draws a small spinner arc when an async background operation is running.
pub fn paint_spinner(painter: &Painter, center: Pos2, radius: f32, color: Color32, time_sec: f64) {
    let start = (time_sec * 6.0) as f32;
    let steps = 12;
    let span = TAU * 0.68;
    let stroke = Stroke::new(2.0, color);
    let mut prev = None;
    for i in 0..=steps {
        let a = start + (i as f32 / steps as f32) * span;
        let pt = pos2(center.x + a.cos() * radius, center.y + a.sin() * radius);
        if let Some(p) = prev {
            painter.line_segment([p, pt], stroke);
        }
        prev = Some(pt);
    }
}
