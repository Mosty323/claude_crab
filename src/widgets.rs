use egui::{
    self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Response, RichText, Rounding,
    Sense, Stroke, TextStyle, Ui, Vec2,
};
use std::ops::RangeInclusive;

pub const BG: Color32 = Color32::from_rgb(24, 22, 21);
pub const PANEL: Color32 = Color32::from_rgb(33, 30, 28);
pub const ROW: Color32 = Color32::from_rgb(42, 38, 35);
pub const ROW_DARK: Color32 = Color32::from_rgb(30, 27, 25);
pub const LINE: Color32 = Color32::from_rgb(66, 59, 52);
pub const GOLD: Color32 = Color32::from_rgb(226, 188, 116);
pub const TEXT: Color32 = Color32::from_rgb(250, 250, 250);
pub const MUTED: Color32 = Color32::from_rgb(190, 190, 190);

pub const PALETTE: [(u8, u8, u8); 16] = [
    (20, 20, 22),
    (74, 70, 66),
    (140, 134, 126),
    (240, 236, 228),
    (205, 70, 62),
    (222, 128, 56),
    (226, 188, 116),
    (240, 214, 92),
    (140, 184, 76),
    (62, 144, 92),
    (60, 150, 150),
    (66, 112, 196),
    (96, 86, 176),
    (150, 86, 168),
    (214, 110, 150),
    (128, 84, 56),
];

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

pub const GLASS_FPS: u32 = 60;

pub const ACCENTS: [Color32; 6] = [
    GOLD,
    Color32::from_rgb(110, 168, 255),
    Color32::from_rgb(112, 204, 140),
    Color32::from_rgb(238, 112, 104),
    Color32::from_rgb(178, 140, 250),
    Color32::from_rgb(244, 140, 196),
];
static ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0xE2BC74);

pub fn accent() -> Color32 {
    let c = ACCENT.load(std::sync::atomic::Ordering::Relaxed);
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

pub fn set_accent(ctx: &egui::Context, c: Color32) {
    let v = (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32;
    if ACCENT.swap(v, std::sync::atomic::Ordering::Relaxed) != v {
        apply_style(ctx);
    }
}

pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6)?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn hex_of(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

pub const CRAB_PRESETS: [(&str, &str, &str, &str, bool); 6] = [
    ("Original", "", "", "", false),
    ("Rot", "#D9473C", "#B53A31", "#1A1A1A", false),
    ("Blau", "#4F8EE6", "#3B6FBD", "#0F1830", false),
    ("Gold", "#E8B940", "#C99A26", "#3A2810", false),
    ("Neon", "#3DFF8C", "#FF3FD6", "#101010", false),
    ("Regenbogen", "", "", "#1A1A1A", true),
];

pub fn color_field(ui: &mut Ui, hex: &mut String, default: Color32) {
    if !hex.is_empty() && button(ui, "x", 22.0, false).clicked() {
        hex.clear();
    }
    let bad = !hex.is_empty() && parse_hex(hex).is_none();
    ui.add(egui::TextEdit::singleline(hex).char_limit(7).desired_width(62.0).hint_text(hex_of(default)).text_color(if bad { MUTED } else { TEXT }));
    let mut c = parse_hex(hex).unwrap_or(default);
    if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
        *hex = hex_of(c);
    }
}

pub fn accent_picker(ui: &mut Ui, sel: &mut usize, hex: &mut String) {
    let bad = !hex.is_empty() && parse_hex(hex).is_none();
    ui.add(egui::TextEdit::singleline(hex).char_limit(7).desired_width(62.0).hint_text("#RRGGBB").text_color(if bad { MUTED } else { TEXT }));
    let cur = accent();
    for i in (0..ACCENTS.len()).rev() {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
        if resp.clicked() {
            *sel = i;
            *hex = hex_of(ACCENTS[i]);
        }
        let on = ACCENTS[i] == cur;
        let p = ui.painter();
        p.circle_filled(rect.center(), 6.5, ACCENTS[i]);
        if on || resp.hovered() {
            p.circle_stroke(rect.center(), 8.5, Stroke::new(1.5f32, if on { TEXT } else { MUTED }));
        }
    }
}

static THEME: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

pub fn glass() -> bool {
    THEME.load(std::sync::atomic::Ordering::Relaxed) == 0
}

pub fn black() -> bool {
    THEME.load(std::sync::atomic::Ordering::Relaxed) == 2
}

pub fn set_theme(ctx: &egui::Context, t: usize) {
    if THEME.swap(t, std::sync::atomic::Ordering::Relaxed) != t {
        apply_style(ctx);
    }
}

pub fn bg_col() -> Color32 {
    if black() { Color32::BLACK } else { BG }
}

pub fn panel_col() -> Color32 {
    if glass() { Color32::from_black_alpha(170) } else if black() { Color32::from_gray(10) } else { PANEL }
}

pub fn row_col() -> Color32 {
    if black() { Color32::from_gray(22) } else { ROW }
}

pub fn row_dark_col() -> Color32 {
    if glass() { Color32::from_black_alpha(110) } else if black() { Color32::from_gray(14) } else { ROW_DARK }
}

pub fn line_col() -> Color32 {
    if glass() { Color32::from_white_alpha(40) } else if black() { Color32::from_gray(44) } else { LINE }
}

pub fn glass_fill(a: u8) -> Color32 {
    Color32::from_white_alpha(a)
}

thread_local! {
    static GLASS_RECTS: std::cell::RefCell<Vec<(Rect, f32, Rect)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn glass_rect(rect: Rect, radius: f32, clip: Rect) {
    GLASS_RECTS.with(|g| g.borrow_mut().push((rect, radius, clip)));
}

pub fn take_glass_rects() -> Vec<(Rect, f32, Rect)> {
    GLASS_RECTS.with(|g| std::mem::take(&mut *g.borrow_mut()))
}

pub fn panel_frame(margin: f32, solid: Color32) -> egui::Frame {
    if !glass() {
        return egui::Frame::none().fill(solid).inner_margin(egui::Margin::same(margin));
    }
    egui::Frame::none().outer_margin(egui::Margin::same(6.0)).inner_margin(egui::Margin::same(margin))
}

pub fn apply_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = bg_col();
    v.window_fill = panel_col();
    v.extreme_bg_color = row_dark_col();
    v.faint_bg_color = row_col();
    v.window_stroke = Stroke::new(1.0f32, line_col());
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.selection.bg_fill = accent().gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0f32, accent());
    let (hover, press) = if black() { (Color32::from_gray(34), Color32::from_gray(46)) } else { (Color32::from_rgb(58, 52, 46), Color32::from_rgb(70, 62, 52)) };
    let r = Rounding::same(4.0);

    v.widgets.noninteractive.bg_fill = row_col();
    v.widgets.noninteractive.weak_bg_fill = row_col();
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, line_col());
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0f32, TEXT);
    v.widgets.noninteractive.rounding = r;

    v.widgets.inactive.bg_fill = row_col();
    v.widgets.inactive.weak_bg_fill = row_col();
    v.widgets.inactive.bg_stroke = Stroke::new(1.0f32, line_col());
    v.widgets.inactive.fg_stroke = Stroke::new(1.0f32, TEXT);
    v.widgets.inactive.rounding = r;

    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0f32, accent().gamma_multiply(0.6));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0f32, accent());
    v.widgets.hovered.rounding = r;

    v.widgets.active.bg_fill = press;
    v.widgets.active.weak_bg_fill = press;
    v.widgets.active.bg_stroke = Stroke::new(1.0f32, accent());
    v.widgets.active.fg_stroke = Stroke::new(1.0f32, accent());
    v.widgets.active.rounding = r;

    v.widgets.open.bg_fill = hover;
    v.widgets.open.weak_bg_fill = hover;
    v.widgets.open.bg_stroke = Stroke::new(1.0f32, accent().gamma_multiply(0.6));
    v.widgets.open.fg_stroke = Stroke::new(1.0f32, TEXT);
    v.widgets.open.rounding = r;

    if glass() {
        let (idle, hover, press) = (Color32::from_black_alpha(90), Color32::from_black_alpha(120), Color32::from_black_alpha(150));
        let rim = Stroke::NONE;
        let r = Rounding::same(10.0);
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = Color32::from_rgba_unmultiplied(40, 40, 56, 220);
        v.window_stroke = rim;
        v.extreme_bg_color = Color32::from_black_alpha(70);
        v.faint_bg_color = idle;
        for (w, fill) in [
            (&mut v.widgets.noninteractive, idle),
            (&mut v.widgets.inactive, idle),
            (&mut v.widgets.hovered, hover),
            (&mut v.widgets.active, press),
            (&mut v.widgets.open, hover),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.rounding = r;
            w.bg_stroke = Stroke::NONE;
        }
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.interact_size.y = 24.0;
        s.text_styles = [
            (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        ]
        .into();
    });
}

pub fn heading(ui: &mut Ui, t: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(t).size(12.0).color(MUTED));
    ui.add_space(2.0);
}

pub fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    let glass = glass();
    let shown = egui::Frame::none()
        .fill(if glass { Color32::TRANSPARENT } else { row_col() })
        .rounding(Rounding::same(if glass { 10.0 } else { 5.0 }))
        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
        .show(ui, |ui| {
            ui.set_width(w - 24.0);
            ui.set_min_height(22.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).color(TEXT).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add(ui);
                });
            });
        });
    if glass {
        glass_rect(shown.response.rect, 10.0, ui.clip_rect());
    }
    ui.add_space(2.0);
}

pub fn check(ui: &mut Ui, on: &mut bool) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.14);
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.12);
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(4.0), mix(row_dark_col(), accent(), t));
    p.rect_stroke(
        rect,
        Rounding::same(4.0),
        Stroke::new(1.0f32, mix(line_col(), accent(), h.max(t))),
    );
    if t > 0.01 {
        let c = rect.center();
        let a = c + Vec2::new(-4.5, 0.5);
        let b = c + Vec2::new(-1.5, 3.5);
        let d = c + Vec2::new(4.5, -3.5);
        let s1 = (t * 2.0).min(1.0);
        let s2 = (t * 2.0 - 1.0).max(0.0);
        let k = Stroke::new(2.0f32, bg_col());
        p.line_segment([a, a + (b - a) * s1], k);
        if s2 > 0.0 {
            p.line_segment([b, b + (d - b) * s2], k);
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
}

pub fn slim(ui: &mut Ui, v: &mut f32, r: RangeInclusive<f32>, w: f32, text: impl Fn(f32) -> String) {
    let (lo, hi) = (*r.start(), *r.end());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::click_and_drag());
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left() - 7.0) / (rect.width() - 14.0)).clamp(0.0, 1.0);
            *v = lo + (hi - lo) * t;
        }
    }
    let t = ((*v - lo) / (hi - lo)).clamp(0.0, 1.0);
    let ts = ui
        .ctx()
        .animate_value_with_time(resp.id.with("t"), t, 0.08);
    let h = ui.ctx().animate_bool_with_time(
        resp.id.with("h"),
        resp.hovered() || resp.dragged(),
        0.12,
    );
    let cy = rect.center().y;
    let x0 = rect.left() + 7.0;
    let x1 = rect.right() - 7.0;
    let kx = x0 + (x1 - x0) * ts;
    let p = ui.painter();
    p.rect_filled(
        Rect::from_min_max(Pos2::new(x0, cy - 2.0), Pos2::new(x1, cy + 2.0)),
        Rounding::same(2.0),
        line_col(),
    );
    p.rect_filled(
        Rect::from_min_max(Pos2::new(x0, cy - 2.0), Pos2::new(kx, cy + 2.0)),
        Rounding::same(2.0),
        accent(),
    );
    p.circle_filled(Pos2::new(kx, cy), 5.0 + 1.5 * h, mix(TEXT, Color32::WHITE, h));
    ui.label(RichText::new(text(*v)).size(12.0).color(MUTED));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
}

pub fn combo(ui: &mut Ui, id: &str, sel: &mut usize, items: &[&str], w: f32) {
    egui::ComboBox::from_id_salt(id)
        .width(w)
        .selected_text(items[*sel])
        .show_ui(ui, |ui| {
            for (i, it) in items.iter().enumerate() {
                ui.selectable_value(sel, i, *it);
            }
        });
}

pub fn button(ui: &mut Ui, text: &str, w: f32, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
    let h = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let a = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("a"), active, 0.14);
    let down = resp.is_pointer_button_down_on();
    let mut fill = mix(mix(row_col(), if black() { Color32::from_gray(34) } else { Color32::from_rgb(58, 52, 46) }, h), accent().gamma_multiply(0.22), a);
    if down {
        fill = accent().gamma_multiply(0.28);
    }
    let stroke = mix(line_col(), accent(), (h * 0.7).max(a));
    let col = mix(mix(TEXT, accent(), h), accent(), a);
    let p = ui.painter();
    if glass() {
        glass_rect(rect, 14.0, ui.clip_rect());
        let fill = if down { accent().gamma_multiply(0.30) } else { mix(glass_fill((h * 22.0) as u8), accent().gamma_multiply(0.24), a) };
        p.rect_filled(rect, Rounding::same(14.0), fill);
    } else {
        p.rect_filled(rect, Rounding::same(5.0), fill);
        p.rect_stroke(rect, Rounding::same(5.0), Stroke::new(1.0f32, stroke));
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.5),
        col,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn icon_sliders(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6f32, col);
    for (dy, kx) in [(-7.0f32, -4.0f32), (0.0, 5.0), (7.0, -1.0)] {
        p.line_segment(
            [c + Vec2::new(-10.0, dy), c + Vec2::new(10.0, dy)],
            s,
        );
        let k = c + Vec2::new(kx, dy);
        p.circle_filled(k, 3.4, panel_col());
        p.circle_stroke(k, 3.4, s);
    }
}

pub fn icon_crab(p: &Painter, c: Pos2, col: Color32) {
    let k = 2.0;
    let o = c - Vec2::new(12.0, 8.0);
    for (r, row) in crate::art::IDLE_STAND.iter().step_by(2).enumerate() {
        for (q, ch) in row.bytes().step_by(2).enumerate() {
            if ch == b'#' {
                p.rect_filled(Rect::from_min_size(o + Vec2::new(q as f32 * k, r as f32 * k), Vec2::splat(k)), 0.0, col);
            }
        }
    }
}

pub fn icon_terminal(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6f32, col);
    p.rect_stroke(Rect::from_center_size(c, Vec2::new(22.0, 17.0)), Rounding::same(3.0), s);
    p.line_segment([c + Vec2::new(-6.0, -3.5), c + Vec2::new(-2.0, 0.0)], s);
    p.line_segment([c + Vec2::new(-2.0, 0.0), c + Vec2::new(-6.0, 3.5)], s);
    p.line_segment([c + Vec2::new(1.0, 4.0), c + Vec2::new(6.0, 4.0)], s);
}

pub fn icon_gear(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6f32, col);
    for i in 0..8 {
        let d = Vec2::angled(i as f32 / 8.0 * std::f32::consts::TAU);
        p.line_segment([c + d * 6.5, c + d * 9.5], Stroke::new(3.0f32, col));
    }
    p.circle_stroke(c, 6.5, s);
    p.circle_stroke(c, 2.6, s);
}

pub fn icon_pixels(p: &Painter, c: Pos2, col: Color32) {
    let cell = 5.5;
    let gap = 1.5;
    let total = cell * 3.0 + gap * 2.0;
    let start = c - Vec2::splat(total / 2.0);
    let on = [true, false, true, false, true, false, true, true, true];
    for r in 0..3 {
        for q in 0..3 {
            let min = start + Vec2::new(q as f32 * (cell + gap), r as f32 * (cell + gap));
            let rect = Rect::from_min_size(min, Vec2::splat(cell));
            if on[r * 3 + q] {
                p.rect_filled(rect, Rounding::same(1.0), col);
            } else {
                p.rect_stroke(
                    rect,
                    Rounding::same(1.0),
                    Stroke::new(1.0f32, col.gamma_multiply(0.5)),
                );
            }
        }
    }
}

pub fn icon_library(p: &Painter, c: Pos2, col: Color32) {
    let s = 8.5;
    let g = 3.5;
    let start = c - Vec2::splat(s + g / 2.0);
    for r in 0..2 {
        for q in 0..2 {
            let min = start + Vec2::new(q as f32 * (s + g), r as f32 * (s + g));
            let rect = Rect::from_min_size(min, Vec2::splat(s));
            if r == 0 && q == 0 {
                p.rect_filled(rect, Rounding::same(2.0), col);
            } else {
                p.rect_stroke(rect, Rounding::same(2.0), Stroke::new(1.6f32, col));
            }
        }
    }
}
