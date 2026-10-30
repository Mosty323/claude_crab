use crate::art::{
    self, Frame, ItemKind, BUBBLE_BG, BUBBLE_EDGE, BUBBLE_FG, BUBBLE_LIGHT, CLOSED_EYE, CRAB, CRAB_SHADE, EYE, GUITAR_BLUE, GUITAR_DARK, GUITAR_LIGHT, GUITAR_SPARK, NOTE, TAIL,
    PETAL, ROPE_DARK, ROPE_KNOT, ROPE_LIGHT, SLASH, SLASH_DIM, SPARK, SPARK_WHITE, STEAM,
    STEAM_FAINT,
};
use crate::sprite::Sprite;
use egui::{Color32, Painter, Pos2, Rect, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h],
        }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        if w != self.w || h != self.h {
            self.w = w;
            self.h = h;
            self.px = vec![0; w * h];
        }
    }

    pub fn clear(&mut self) {
        self.px.fill(0);
    }

    pub fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color32) {
        let (x0, x1) = (x0.max(0) as usize, (x1.max(0) as usize).min(self.w));
        let (y0, y1) = (y0.max(0) as usize, (y1.max(0) as usize).min(self.h));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let v = (c.a() as u32) << 24 | (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32;
        if c.a() == 255 {
            for y in y0..y1 {
                self.px[y * self.w + x0..y * self.w + x1].fill(v);
            }
            return;
        }
        let inv = 255 - c.a() as u32;
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let d = *p;
                let ch = |sh: u32| ((v >> sh) & 255) + ((d >> sh) & 255) * inv / 255;
                *p = ch(24) << 24 | ch(16) << 16 | ch(8) << 8 | ch(0);
            }
        }
    }
}

#[derive(Clone)]
pub enum Target {
    Painter(Painter),
    Canvas(Rc<RefCell<Canvas>>),
}

#[derive(Clone)]
pub struct Brush {
    pub target: Target,
    pub origin: Pos2,
    pub ppp: f32,
}

impl Brush {
    pub fn painter(painter: Painter, origin: Pos2, ppp: f32) -> Self {
        Self {
            target: Target::Painter(painter),
            origin,
            ppp,
        }
    }

    pub fn canvas(canvas: Rc<RefCell<Canvas>>, ppp: f32) -> Self {
        Self {
            target: Target::Canvas(canvas),
            origin: Pos2::ZERO,
            ppp,
        }
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        let px = |v: f32| (v * self.ppp).round();
        let (x0, y0) = (px(self.origin.x + x), px(self.origin.y + y));
        let (x1, y1) = (px(self.origin.x + x + w), px(self.origin.y + y + h));
        match &self.target {
            Target::Painter(p) => {
                let r = Rect::from_min_max(
                    Pos2::new((x0 - 0.5) / self.ppp, (y0 - 0.5) / self.ppp),
                    Pos2::new((x1 + 0.5) / self.ppp, (y1 + 0.5) / self.ppp),
                );
                p.rect_filled(r, 0.0, c);
            }
            Target::Canvas(cv) => cv
                .borrow_mut()
                .fill(x0 as i32, y0 as i32, x1 as i32, y1 as i32, c),
        }
    }
}

static CRAB_PAL: [std::sync::atomic::AtomicU32; 3] = [const { std::sync::atomic::AtomicU32::new(0) }; 3];
static RAINBOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_crab_colors(body: Option<Color32>, claw: Option<Color32>, eye: Option<Color32>, rainbow: bool) {
    for (slot, c) in CRAB_PAL.iter().zip([body, claw, eye]) {
        slot.store(c.map_or(0, |c| 1 << 24 | (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32), std::sync::atomic::Ordering::Relaxed);
    }
    RAINBOW.store(rainbow, std::sync::atomic::Ordering::Relaxed);
}

pub fn apply_crab_colors(s: &crate::sprite::Settings) {
    use crate::widgets::parse_hex;
    set_crab_colors(parse_hex(&s.crab_body), parse_hex(&s.crab_claw), parse_hex(&s.crab_eye), s.crab_rainbow);
}

fn pal(i: usize, default: Color32) -> Color32 {
    let v = CRAB_PAL[i].load(std::sync::atomic::Ordering::Relaxed);
    if v == 0 {
        return default;
    }
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn rainbow(col: usize) -> Option<Color32> {
    if !RAINBOW.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32();
    let h = (col as f32 / 24.0 + t * 0.2).fract();
    Some(egui::ecolor::Hsva::new(h, 0.62, 0.95, 1.0).into())
}

pub fn body_color(col: usize) -> Color32 {
    rainbow(col).unwrap_or_else(|| pal(0, CRAB))
}

pub fn claw_color(col: usize) -> Color32 {
    rainbow(col).unwrap_or_else(|| pal(1, pal(0, CRAB)))
}

fn shade(c: Color32) -> Color32 {
    if c == CRAB {
        return CRAB_SHADE;
    }
    let k = |v: u8| (v as f32 * 0.84) as u8;
    Color32::from_rgb(k(c.r()), k(c.g()), k(c.b()))
}

pub fn eye_color(closed: bool) -> Color32 {
    if closed {
        let c = body_color(12);
        let k = |v: u8| (v as f32 * 0.63) as u8;
        return if CRAB_PAL[0].load(std::sync::atomic::Ordering::Relaxed) == 0 && !RAINBOW.load(std::sync::atomic::Ordering::Relaxed) { CLOSED_EYE } else { Color32::from_rgb(k(c.r()), k(c.g()), k(c.b())) };
    }
    pal(2, EYE)
}

fn body_span(rows: &[&str]) -> (usize, usize, usize) {
    rows.iter()
        .enumerate()
        .find_map(|(r, row)| {
            let cols: Vec<usize> = row.char_indices().filter(|(_, c)| matches!(c, '#' | 'd' | 'E')).map(|(i, _)| i).collect();
            (cols.len() >= 8).then(|| (r, cols[0], cols[cols.len() - 1]))
        })
        .unwrap_or((0, 4, 19))
}

static OUTFIT: std::sync::Mutex<Vec<(Sprite, bool)>> = std::sync::Mutex::new(Vec::new());

pub fn set_outfit(layers: Vec<(Sprite, bool)>) {
    if let Ok(mut o) = OUTFIT.lock() {
        *o = layers;
    }
}

fn outfit() -> Vec<(Sprite, bool)> {
    OUTFIT.lock().map(|o| o.clone()).unwrap_or_default()
}

fn cloth(layers: &[(Sprite, bool)], back: bool, x: i32, y: i32) -> Option<Color32> {
    layers.iter().rev().filter(|l| l.1 == back).find_map(|(s, _)| {
        if x < 0 || y < 0 || x >= s.w as i32 || y >= s.h as i32 {
            return None;
        }
        let p = s.get(x as usize, y as usize);
        (p[3] > 0).then(|| if back { Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]) } else { Color32::from_rgb(p[0], p[1], p[2]) })
    })
}

pub fn crab_look_key(core: &crate::sprite::Core) -> u64 {
    use std::hash::{Hash, Hasher};
    let s = &core.settings;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (&s.crab_body, &s.crab_claw, &s.crab_eye, s.crab_rainbow, s.hat_visible, &s.clothes, &s.clothes_hidden).hash(&mut h);
    core.hat.and_then(|i| core.slots.get(i)).map(|x| &x.name).hash(&mut h);
    h.finish()
}

pub fn crab_icon(core: &crate::sprite::Core, size: usize) -> Vec<u8> {
    apply_crab_colors(&core.settings);
    set_outfit(core.outfit_layers());
    let hat = core.hat_sprite().filter(|_| core.settings.hat_visible);
    let rows = if hat.is_some() { 14.0 } else { 8.0 };
    let blk = (size as f32 / 16.0).floor().max(1.0);
    let x = ((size as f32 - 12.0 * blk) / 2.0).floor();
    let top = ((size as f32 - rows * blk) / 2.0).floor();
    let y = top + if hat.is_some() { 6.0 * blk } else { 0.0 };
    let canvas = Rc::new(RefCell::new(Canvas::new(size, size)));
    let brush = Brush::canvas(canvas.clone(), 1.0);
    draw_crab(&brush, x, y, blk, &crate::art::IDLE_STAND, 0.0);
    if let Some(h) = hat {
        draw_hat(&brush, h, x, y, blk, core.hat_drop() * blk);
    }
    let body = body_color(12);
    let dark = (0.299 * body.r() as f32 + 0.587 * body.g() as f32 + 0.114 * body.b() as f32) < 70.0;
    let bg: [u32; 3] = if dark { [214, 208, 200] } else { [33, 30, 28] };
    let r = (size / 6) as i32;
    let si = size as i32;
    let c = canvas.borrow();
    c.px.iter()
        .enumerate()
        .flat_map(|(i, &p)| {
            let (x, y) = ((i % size) as i32, (i / size) as i32);
            let (cx, cy) = (x.clamp(r, si - 1 - r), y.clamp(r, si - 1 - r));
            let inside = (x - cx).pow(2) + (y - cy).pow(2) <= r * r;
            let a = p >> 24;
            if inside {
                let ch = |sh: u32, b: u32| (((p >> sh) & 255) + b * (255 - a) / 255).min(255) as u8;
                [ch(16, bg[0]), ch(8, bg[1]), ch(0, bg[2]), 255]
            } else {
                let un = |v: u32| if a == 0 { 0 } else { (v * 255 / a).min(255) as u8 };
                [un((p >> 16) & 255), un((p >> 8) & 255), un(p & 255), a as u8]
            }
        })
        .collect()
}

pub fn save_desktop_icon(core: &crate::sprite::Core) {
    let path = crate::sprite::data_path("claude_crab.ico");
    if !path.exists() {
        return;
    }
    let frames: Vec<image::codecs::ico::IcoFrame> = [256u32, 48, 32, 16]
        .iter()
        .filter_map(|&s| image::codecs::ico::IcoFrame::as_png(&crab_icon(core, s as usize), s, s, image::ExtendedColorType::Rgba8).ok())
        .collect();
    let Ok(file) = std::fs::File::create(&path) else {
        return;
    };
    if image::codecs::ico::IcoEncoder::new(file).encode_images(&frames).is_ok() {
        std::thread::spawn(|| unsafe {
            use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
            SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null());
        });
    }
}

pub fn idle_motion(style: usize, t: f32) -> (f32, f32, f32) {
    match style {
        0 => (0.0, 0.0, if (t * 1.1).fract() > 0.7 { 1.0 } else { 0.0 }),
        1 => (((t * 1.8).sin() * 1.2).round() * 0.5, 0.0, 0.0),
        2 => (0.0, -((t * 3.0).sin().abs() * 2.0).floor(), 0.0),
        _ => (0.0, 0.0, 0.0),
    }
}

pub fn draw_crab(b: &Brush, x: f32, y: f32, block: f32, frame: &Frame, body_dy: f32) {
    draw_crab_posed(b, x, y, block, frame, body_dy, 0.0, [(0, 0); 2], 0);
}

pub fn draw_crab_posed(b: &Brush, x: f32, y: f32, block: f32, frame: &Frame, body_dy: f32, upper_dy: f32, hands: [(i32, i32); 2], knee: i32) {
    let px = block / 2.0;
    let moved = |c: usize, r: usize| (4..8).contains(&r) && ((c < 4 && hands[0] != (0, 0)) || (c >= 20 && hands[1] != (0, 0)));
    let layers = outfit();
    let (head, left, _) = body_span(&frame[..]);
    let (ox, oy) = (left as i32 - 4, head as i32);
    let row_dy = |r: usize| match r {
        0..=7 => body_dy + upper_dy,
        8..=11 => body_dy,
        _ => 0.0,
    };
    let knee_of = |r: usize, c: i32| match (r, c) {
        (12.., 0..=5) => -knee,
        (12.., 18..) => knee,
        _ => 0,
    };
    if layers.iter().any(|l| l.1) {
        for cy in 0..crate::sprite::CLOTH_H as i32 {
            for cx in 0..crate::sprite::CLOTH_W as i32 {
                let (c, r) = (cx + ox, cy + oy);
                let Some(col) = cloth(&layers, true, cx, cy) else { continue };
                if r < 0 || r >= frame.len() as i32 {
                    continue;
                }
                let r = r as usize;
                b.rect(x + (c + knee_of(r, c)) as f32 * px, y + r as f32 * px + row_dy(r), px, px, col);
            }
        }
    }
    for (r, row) in frame.iter().enumerate() {
        let dy = match r {
            0..=7 => body_dy + upper_dy,
            8..=11 => body_dy,
            _ => 0.0,
        };
        for (c, ch) in row.chars().enumerate() {
            if ch == '#' && moved(c, r) {
                continue;
            }
            let arm = (4..8).contains(&r) && (c < 4 || c >= 20);
            let color = match ch {
                '#' if arm => claw_color(c),
                '#' => body_color(c),
                'E' | 'O' => eye_color(false),
                '-' => eye_color(true),
                _ => continue,
            };
            let color = cloth(&layers, false, c as i32 - ox, r as i32 - oy).unwrap_or(color);
            let kx = match (r, c) {
                (12.., 0..=5) => -knee,
                (12.., 18..) => knee,
                _ => 0,
            };
            let cx = x + (c as i32 + kx) as f32 * px;
            b.rect(cx, y + r as f32 * px + dy, px, px, color);
            if r == 12 && body_dy < 0.0 {
                b.rect(cx, y + r as f32 * px + body_dy, px, -body_dy, color);
            }
        }
    }
    for (i, (dx, dy)) in hands.into_iter().enumerate() {
        if (dx, dy) == (0, 0) {
            continue;
        }
        let col = if i == 0 { 0 } else { 20 };
        let steps = dx.abs().max(dy.abs());
        for k in 0..=steps {
            let (sx, sy) = (col + dx * k / steps, 4 + dy * k / steps);
            if k < steps && (sx - col).abs() < 4 && (sy - 4).abs() < 4 {
                continue;
            }
            b.rect(x + sx as f32 * px, y + sy as f32 * px + body_dy + upper_dy, 4.0 * px, 4.0 * px, claw_color(sx.max(0) as usize));
        }
    }
}

pub fn draw_sprite_frame(b: &Brush, x: f32, y: f32, block: f32, rows: &[&str]) {
    let px = block / 2.0;
    let pad = rows.len() as f32 - 16.0;
    let (head, left, right) = body_span(rows);
    let layers = outfit();
    let (ox, oy) = (left as i32 - 4, head as i32);
    if layers.iter().any(|l| l.1) {
        for cy in 0..crate::sprite::CLOTH_H as i32 {
            for cx in 0..crate::sprite::CLOTH_W as i32 {
                let Some(col) = cloth(&layers, true, cx, cy) else { continue };
                let (c, r) = (cx + ox, cy + oy);
                b.rect(x + (c as f32 - 2.0) * px, y + (r as f32 - pad) * px, px, px, col);
            }
        }
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            let arm = r > head + 1 && (c > right || (c < left && r + 4 < rows.len()));
            let color = match ch {
                '#' if arm => claw_color(c),
                '#' => body_color(c),
                'd' if arm => shade(claw_color(c)),
                'd' => shade(body_color(c)),
                'E' => eye_color(false),
                'g' => TAIL,
                'k' => GUITAR_DARK,
                'b' => GUITAR_BLUE,
                'w' => GUITAR_LIGHT,
                'y' => GUITAR_SPARK,
                'n' => NOTE,
                _ => continue,
            };
            let color = match ch {
                '#' | 'd' | 'E' => cloth(&layers, false, c as i32 - ox, r as i32 - oy).map_or(color, |cc| if ch == 'd' { shade(cc) } else { cc }),
                _ => color,
            };
            b.rect(x + (c as f32 - 2.0) * px, y + (r as f32 - pad) * px, px, px, color);
        }
    }
}

pub fn draw_hat(b: &Brush, hat: &Sprite, x: f32, y: f32, block: f32, extra_dy: f32) {
    let base = y - 6.0 * block + extra_dy;
    let cell = block * crate::sprite::HAT_W as f32 / hat.w as f32;
    for r in 0..hat.h {
        for c in 0..hat.w {
            let p = hat.get(c, r);
            if p[3] == 0 {
                continue;
            }
            b.rect(
                x + c as f32 * cell,
                base + r as f32 * cell,
                cell,
                cell,
                Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]),
            );
        }
    }
}

pub fn draw_buddy(
    b: &Brush,
    sprite: &Sprite,
    x: f32,
    y: f32,
    size: f32,
    flip: bool,
    squash: f32,
) {
    let cw = size / sprite.w as f32;
    let ch = (size - squash) / sprite.h as f32;
    for r in 0..sprite.h {
        for c in 0..sprite.w {
            let p = sprite.get(c, r);
            if p[3] == 0 {
                continue;
            }
            let cx = if flip { sprite.w - 1 - c } else { c };
            b.rect(
                x + cx as f32 * cw,
                y + squash + r as f32 * ch,
                cw,
                ch,
                Color32::from_rgb(p[0], p[1], p[2]),
            );
        }
    }
}

pub fn draw_lasso(b: &Brush, x: f32, y: f32, bsz: f32, tick: u64) {
    let rope: [(f32, f32); 6] = [
        (12.0, 2.0),
        (12.0, 1.0),
        (13.0, 0.0),
        (14.0, -1.0),
        (14.0, -2.0),
        (15.0, -3.0),
    ];
    for (i, (cx, cy)) in rope.iter().enumerate() {
        let c = if i == rope.len() - 1 {
            ROPE_KNOT
        } else if i % 2 == 0 {
            ROPE_LIGHT
        } else {
            ROPE_DARK
        };
        b.rect(x + cx * bsz, y + cy * bsz, bsz, bsz, c);
    }
    let m = art::LASSO[((tick / 4) % 4) as usize];
    let bottom = m.last().copied().unwrap_or("");
    let anchor = bottom.find('#').unwrap_or(0) as f32;
    let left = 15.0 - anchor;
    let top = -4.0 - (m.len() as f32 - 1.0);
    for (r, row) in m.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            if ch == '#' {
                let col = if (r + c) % 2 == 0 {
                    ROPE_LIGHT
                } else {
                    ROPE_DARK
                };
                b.rect(
                    x + (left + c as f32) * bsz,
                    y + (top + r as f32) * bsz,
                    bsz,
                    bsz,
                    col,
                );
            }
        }
    }
}

pub const GRIP: (f32, f32) = (1.5, 5.5);

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct ItemPose {
    pub dx: f32,
    pub dy: f32,
    pub angle: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fx {
    None,
    Ambient,
    Code,
    Bubbles,
    Steam,
    Hearts,
    Sparkles,
    Burst,
    Slash,
}

pub fn rot(v: (f32, f32), a: f32) -> (f32, f32) {
    let (s, c) = a.sin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

pub fn item_pivot(x: f32, y: f32, blk: f32, pose: ItemPose) -> (f32, f32) {
    (x + (12.5 + pose.dx) * blk, y + (3.5 + pose.dy) * blk)
}

pub fn draw_item(b: &Brush, item: &Sprite, x: f32, y: f32, blk: f32, pose: ItemPose) {
    draw_cells(b, item, 1.0, x, y, blk, pose);
}

pub fn draw_item_of(b: &Brush, kind: ItemKind, item: &Sprite, x: f32, y: f32, blk: f32, pose: ItemPose) {
    static FINE: std::sync::OnceLock<Sprite> = std::sync::OnceLock::new();
    if kind == ItemKind::Laptop {
        draw_cells(b, FINE.get_or_init(|| crate::sprite::sprite_from_def(&art::LAPTOP_FINE)), 2.0, x, y, blk, pose);
    } else {
        draw_item(b, item, x, y, blk, pose);
    }
}

fn draw_cells(b: &Brush, item: &Sprite, k: f32, x: f32, y: f32, blk: f32, pose: ItemPose) {
    let (px, py) = item_pivot(x, y, blk, pose);
    let unit = blk / k;
    let grip = (GRIP.0 * k, GRIP.1 * k);
    let opaque = |p: [u8; 4]| Color32::from_rgb(p[0], p[1], p[2]);
    let a = pose.angle.rem_euclid(std::f32::consts::TAU);
    if a < 0.01 || a > std::f32::consts::TAU - 0.01 {
        let ox = px - grip.0 * unit;
        let oy = py - grip.1 * unit;
        for r in 0..item.h {
            for c in 0..item.w {
                let p = item.get(c, r);
                if p[3] != 0 {
                    b.rect(ox + c as f32 * unit, oy + r as f32 * unit, unit, unit, opaque(p));
                }
            }
        }
        return;
    }
    let (s, c) = (-pose.angle).sin_cos();
    let h = 0.5 - grip.0.fract();
    let reach = item.w.max(item.h) as i32 + 2;
    for j in -reach..=reach {
        for i in -reach..=reach {
            let (fx, fy) = (i as f32 + h, j as f32 + h);
            let sx = fx * c - fy * s + grip.0;
            let sy = fx * s + fy * c + grip.1;
            if sx < 0.0 || sy < 0.0 {
                continue;
            }
            let (cx, cy) = (sx as usize, sy as usize);
            if cx >= item.w || cy >= item.h {
                continue;
            }
            let p = item.get(cx, cy);
            if p[3] != 0 {
                b.rect(px + (fx - 0.5) * unit, py + (fy - 0.5) * unit, unit, unit, opaque(p));
            }
        }
    }
}

fn hash(n: u32) -> u32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

struct Fxp {
    angle: f32,
    px: Vec<(i32, i32, Color32)>,
}

impl Fxp {
    fn turn(&self, v: (i32, i32)) -> (i32, i32) {
        if self.angle.abs() < 0.01 {
            return v;
        }
        let r = rot((v.0 as f32, v.1 as f32), self.angle);
        (r.0.round() as i32, r.1.round() as i32)
    }

    fn local(&mut self, c: i32, r: i32, col: Color32) {
        let (x, y) = self.turn((c - 1, r - 5));
        self.px.push((x, y, col));
    }

    fn at(&mut self, c: i32, r: i32, dx: i32, dy: i32, col: Color32) {
        let (x, y) = self.turn((c - 1, r - 5));
        self.px.push((x + dx, y + dy, col));
    }

    fn star(&mut self, c: i32, r: i32, dx: i32, dy: i32, center: Color32, rays: Option<Color32>) {
        self.at(c, r, dx, dy, center);
        if let Some(rc) = rays {
            for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                self.at(c, r, dx + ox, dy + oy, rc);
            }
        }
    }
}

fn steam(e: &mut Fxp, c: i32, r: i32, f: i32) {
    const ZIG: [i32; 4] = [0, 0, 1, 1];
    for w in 0..2 {
        let base = w * 2 - 1;
        for k in 0..4 {
            let q = (f - k + w * 3).rem_euclid(12);
            if q % 6 == 5 || (k == 3 && q % 2 == 1) {
                continue;
            }
            let col = if k <= 1 { STEAM } else { STEAM_FAINT };
            e.at(c, r, base + ZIG[(q % 4) as usize], -1 - k, col);
        }
    }
}

fn ambient(e: &mut Fxp, kind: ItemKind, f: i32) {
    match kind {
        ItemKind::Coffee => steam(e, 3, 2, f),
        ItemKind::Bottle => {
            const BUBBLE: [(i32, i32); 4] = [(3, 6), (3, 3), (3, 2), (3, 1)];
            const GLINT: [(i32, i32); 3] = [(2, 2), (2, 3), (2, 6)];
            let p = f % 12;
            if p < 4 {
                let (c, r) = BUBBLE[p as usize];
                e.local(c, r, BUBBLE_LIGHT);
            }
            let g = f % 20;
            if g < 3 {
                let (c, r) = GLINT[g as usize];
                e.local(c, r, SPARK_WHITE);
            }
        }
        ItemKind::Flower => {
            const PATH: [(i32, i32); 6] = [(0, 0), (1, 1), (1, 2), (2, 3), (2, 4), (3, 5)];
            let p = f % 16;
            if p < 6 {
                let (dx, dy) = PATH[p as usize];
                e.at(4, 2, dx, dy, PETAL);
            }
            if (9..11).contains(&p) {
                e.local(2, -1, SPARK_WHITE);
            }
        }
        ItemKind::Wand => {
            let p = f % 10;
            if p < 2 {
                for (c, r) in [(5, -1), (5, 3), (3, 1), (7, 1)] {
                    e.local(c, r, SPARK_WHITE);
                }
            } else if p == 2 {
                for (c, r) in [(4, 0), (6, 0), (4, 2), (6, 2)] {
                    e.local(c, r, SPARK);
                }
            } else if (4..8).contains(&p) {
                let k = p - 4;
                e.at(5, 1, 1 + k / 2, -1 - k, if k % 2 == 0 { SPARK } else { SPARK_WHITE });
            }
        }
        ItemKind::Sword => {
            const PATH: [(i32, i32); 4] = [(3, 3), (4, 2), (5, 1), (6, 0)];
            let p = f % 16;
            if p < 4 {
                let (c, r) = PATH[p as usize];
                e.local(c, r, SPARK_WHITE);
            } else if p == 4 {
                e.star(7, -1, 0, 0, SPARK_WHITE, Some(SLASH));
            } else if p == 5 {
                e.local(7, -1, SPARK_WHITE);
            }
        }
        ItemKind::Laptop if f % 12 < 6 => code(e, f, None),
        ItemKind::Laptop => {}
        ItemKind::Guitar if f % 14 < 5 => note(e, 1, -1 - f % 14, STEAM),
        ItemKind::Guitar => {}
        ItemKind::Custom => {}
    }
}

fn note(e: &mut Fxp, dx: i32, dy: i32, col: Color32) {
    for (ox, oy) in [(0, 0), (1, 0), (1, -1), (1, -2), (2, -1)] {
        e.at(3, 3, dx + ox, dy + oy, col);
    }
}

fn code(e: &mut Fxp, f: i32, key: Option<i32>) {
    for k in 0..2 {
        let q = (f + k * 5).rem_euclid(10);
        if q < 4 {
            let pick = hash(((f + k * 5).div_euclid(10) * 2 + k) as u32);
            let col = art::CODE_COLORS[(pick % 4) as usize];
            e.at(5, 4, k - 1 - (q % 2), -1 - q, col);
        }
    }
    if let Some(c) = key {
        e.local(c, 6, SPARK_WHITE);
    }
}

pub fn item_bottom(item: &Sprite, angle: f32) -> f32 {
    let (s, c) = angle.sin_cos();
    let mut low = f32::MIN;
    for r in 0..item.h {
        for col in 0..item.w {
            if item.get(col, r)[3] == 0 {
                continue;
            }
            for (cx, cy) in [(col as f32, r as f32 + 1.0), (col as f32 + 1.0, r as f32 + 1.0), (col as f32, r as f32), (col as f32 + 1.0, r as f32)] {
                let (vx, vy) = (cx - GRIP.0, cy - GRIP.1);
                low = low.max(vx * s + vy * c);
            }
        }
    }
    if low == f32::MIN {
        0.0
    } else {
        low
    }
}

pub fn draw_item_fx(b: &Brush, kind: ItemKind, fx: Fx, x: f32, y: f32, blk: f32, pose: ItemPose, tick: u32) {
    let mut e = Fxp {
        angle: pose.angle,
        px: Vec::new(),
    };
    let t = tick as i32;
    let f = t / 3;
    match fx {
        Fx::None => {}
        Fx::Ambient => ambient(&mut e, kind, t / 5),
        Fx::Steam => steam(&mut e, 3, 2, f),
        Fx::Code => code(&mut e, f, None),
        Fx::Bubbles => {
            for i in 0..3 {
                let h = (f + i * 2) % 6;
                if h == 5 {
                    continue;
                }
                let wob = if (h + i) % 2 == 0 { 0 } else { 1 };
                let col = if i % 2 == 0 { BUBBLE_LIGHT } else { SPARK_WHITE };
                e.at(2, 0, i - 1 + wob, -1 - h, col);
            }
        }
        Fx::Hearts => {
            const HEART: [(i32, i32); 6] = [(-1, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (0, 1)];
            for i in 0..2 {
                let h = (f + i * 4) % 8;
                if h == 7 {
                    continue;
                }
                let bx = i * 4 - 2 + (h / 2) % 2;
                for (ox, oy) in HEART {
                    e.at(2, 0, bx + ox, -2 - h + oy, art::HEART);
                }
            }
        }
        Fx::Sparkles => {
            for i in 0..3u32 {
                let s = hash((f / 2) as u32 * 31 + i * 7919);
                if s % 3 == 0 {
                    continue;
                }
                let dx = (s % 7) as i32 - 3;
                let dy = ((s / 7) % 6) as i32 - 4;
                if (f as u32 + i) % 2 == 0 {
                    e.star(5, 1, dx, dy, SPARK_WHITE, Some(SPARK));
                } else {
                    e.at(5, 1, dx, dy, SPARK);
                }
            }
        }
        Fx::Burst => {
            let r = (1 + f).min(4);
            let d = (r * 2 / 3).max(1);
            e.at(5, 1, 0, 0, SPARK_WHITE);
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                e.at(5, 1, ox * r, oy * r, SPARK_WHITE);
            }
            for (ox, oy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                e.at(5, 1, ox * d, oy * d, SPARK);
            }
        }
        Fx::Slash => {
            for k in 1..5 {
                let r = rot((5.0, -5.0), pose.angle - k as f32 * 0.22);
                let col = if k < 3 { SLASH } else { SLASH_DIM };
                e.px.push((r.0.round() as i32, r.1.round() as i32, col));
            }
        }
    }
    let (px, py) = item_pivot(x, y, blk, pose);
    for (dx, dy, col) in e.px {
        b.rect(px + (dx as f32 - 0.5) * blk, py + (dy as f32 - 0.5) * blk, blk, blk, col);
    }
}

const Z_GLYPHS: [&[&str]; 3] = [
    &["####", "..#.", ".#..", "####"],
    &["#####", "...#.", "..#..", ".#...", "#####"],
    &["######", "....#.", "...#..", "..#...", ".#....", "######"],
];
const Z_SPAWN: u64 = 40;
const Z_LIFE: u64 = 120;

pub fn zzz_cells(tick: u64) -> Vec<(i32, i32, bool)> {
    let mut cells = Vec::new();
    for i in 0..Z_LIFE / Z_SPAWN {
        let age = (tick + i * Z_SPAWN) % Z_LIFE;
        let glyph = Z_GLYPHS[(age * 3 / Z_LIFE) as usize];
        let size = glyph.len() as i32;
        let dx = (age / 6) as i32 + ((age as f32 * 0.15).sin() * 1.5).round() as i32;
        let dy = -((age / 4) as i32) - size;
        for (r, row) in glyph.iter().enumerate() {
            for (c, ch) in row.bytes().enumerate() {
                if ch != b'#' {
                    continue;
                }
                let (px, py) = (dx + c as i32, dy + r as i32);
                for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                    cells.push((px + ox, py + oy, true));
                }
                cells.push((px, py, false));
            }
        }
    }
    cells.sort_by_key(|c| !c.2);
    cells
}

pub fn draw_zzz(b: &Brush, x: f32, y: f32, blk: f32, tick: u64) {
    let px = blk / 2.0;
    for (cx, cy, edge) in zzz_cells(tick) {
        b.rect(x + cx as f32 * px, y + cy as f32 * px, px, px, if edge { BUBBLE_EDGE } else { BUBBLE_BG });
    }
}

const GLASS_TEXT_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 150);

fn glass_panel(b: &Brush, x: f32, y: f32, w: f32, h: f32, r: f32) {
    glass_shape(b, x, y, w, h, r, false);
}

fn glass_shape(b: &Brush, x: f32, y: f32, w: f32, h: f32, r: f32, cloud: bool) {
    if let Target::Canvas(cv) = &b.target {
        let k = b.ppp;
        let (px, py) = (((b.origin.x + x) * k).round() as i32, ((b.origin.y + y) * k).round() as i32);
        crate::glass::pet_panel(&mut cv.borrow_mut(), px, py, (w * k).round() as usize, (h * k).round() as usize, r * k, k * (h / 200.0).clamp(0.15, 1.0), crate::glass::RIM * k, cloud);
    }
}

pub fn draw_sign(b: &Brush, x: f32, y: f32, blk: f32, check: bool, glass: bool) {
    const BANG: [&str; 5] = [".#.", ".#.", ".#.", "...", ".#."];
    const TICK: [&str; 5] = ["...", "..#", "#.#", ".#.", "..."];
    let (glyph, mark) = if check {
        (TICK, Color32::from_rgb(80, 180, 100))
    } else {
        (BANG, art::PETAL)
    };
    if glass {
        glass_panel(b, x - blk, y - 7.0 * blk, 5.0 * blk, 7.0 * blk, 1.6 * blk);
    }
    for r in 0..7 {
        for c in 0..5 {
            let (cx, cy) = (x + (c as f32 - 1.0) * blk, y + (r as f32 - 7.0) * blk);
            let edge = r == 0 || r == 6 || c == 0 || c == 4;
            let mark_px = !edge && glyph[r - 1].as_bytes()[c - 1] == b'#';
            if glass {
                if mark_px {
                    b.rect(cx, cy, blk, blk, mark);
                }
                continue;
            }
            let col = if edge {
                BUBBLE_EDGE
            } else if mark_px {
                mark
            } else {
                BUBBLE_BG
            };
            b.rect(cx, cy, blk, blk, col);
        }
    }
}

pub fn draw_shadow(b: &Brush, x: f32, y: f32, w: f32, h: f32) {
    let cell = (h / 3.0).max(2.0);
    let cols = (w / cell).floor() as i32;
    let rows = (h / cell).ceil().max(1.0) as i32;
    let color = Color32::from_rgb(34, 34, 34);
    for r in 0..rows {
        for c in 0..cols {
            if (r + c) % 2 != 0 {
                continue;
            }
            let nx = (c as f32 + 0.5) / cols as f32 * 2.0 - 1.0;
            let ny = (r as f32 + 0.5) / rows as f32 * 2.0 - 1.0;
            if nx * nx + ny * ny <= 1.0 {
                b.rect(x + c as f32 * cell, y + r as f32 * cell, cell, cell, color);
            }
        }
    }
}

fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in text.split(' ') {
        if cur.is_empty() {
            cur = w.to_string();
        } else if cur.chars().count() + 1 + w.chars().count() <= max {
            cur.push(' ');
            cur.push_str(w);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn visible_lines(text: &str, typed: usize) -> Vec<String> {
    let mut left = typed;
    let mut out = Vec::new();
    for line in wrap(text, 16) {
        if left == 0 {
            break;
        }
        let n = line.chars().count();
        out.push(line.chars().take(n.min(left)).collect());
        left = left.saturating_sub(n + 1);
    }
    out
}

fn glyph_key(c: char) -> char {
    if c == 'ß' {
        c
    } else {
        c.to_uppercase().next().unwrap_or(c)
    }
}

pub fn draw_bubble(
    b: &Brush,
    text: &str,
    typed: usize,
    anchor_x: i32,
    anchor_y: i32,
    win_w: i32,
    glass: bool,
    cloud: bool,
) -> Option<Rect> {
    let cloud = cloud && glass;
    let lines = visible_lines(text, typed);
    if lines.is_empty() {
        return None;
    }
    let fp = 2;
    let char_w = 3 * fp;
    let char_h = 5 * fp;
    let spacing = 2 * fp;
    let line_gap = 3 * fp;
    let count = lines.len() as i32;
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32;
    let text_w = longest * (char_w + spacing) - spacing;
    let text_h = count * char_h + (count - 1) * line_gap;
    let (pad_x, pad_y) = if cloud { (22, 15) } else if glass { (14, 9) } else { (10, 8) };
    let ps = 3;
    let bw = ((text_w + pad_x * 2) / ps) * ps;
    let bh = ((text_h + pad_y * 2) / ps) * ps;
    let bx = ((anchor_x - (text_w + pad_x * 2) / 2).min(win_w - (text_w + pad_x * 2) - 8)).max(8);
    let by = (anchor_y - (text_h + pad_y * 2) - 10).max(6);
    let bx = (bx / ps) * ps;
    let by = (by / ps) * ps;

    let r = |x: i32, y: i32, w: i32, h: i32, c: Color32| {
        b.rect(x as f32, y as f32, w as f32, h as f32, c);
    };
    let tail_x = (anchor_x.max(bx + 16).min(bx + bw - 16) / ps) * ps;
    if cloud {
        glass_shape(b, bx as f32, by as f32, bw as f32, bh as f32, 0.0, true);
        let (tx, ty) = (tail_x as f32, (by + bh) as f32);
        glass_panel(b, tx - 12.0, ty + 1.0, 12.0, 12.0, 6.0);
        glass_panel(b, tx - 20.0, ty + 13.0, 7.0, 7.0, 3.5);
    } else if glass {
        glass_panel(b, bx as f32, by as f32, bw as f32, bh as f32, (bh as f32 / 2.0).min(18.0));
    } else {
        r(bx + ps, by, bw - 2 * ps, bh, BUBBLE_EDGE);
        r(bx, by + ps, bw, bh - 2 * ps, BUBBLE_EDGE);
        r(bx + 2 * ps, by + ps, bw - 4 * ps, bh - 2 * ps, BUBBLE_BG);
        r(bx + ps, by + 2 * ps, bw - 2 * ps, bh - 4 * ps, BUBBLE_BG);
        r(tail_x - ps, by + bh, 3 * ps, ps, BUBBLE_EDGE);
        r(tail_x, by + bh, ps, ps, BUBBLE_BG);
        r(tail_x - ps, by + bh + ps, 2 * ps, ps, BUBBLE_EDGE);
        r(tail_x, by + bh + ps, ps, ps, BUBBLE_BG);
        r(tail_x, by + bh + 2 * ps, ps, ps, BUBBLE_EDGE);
    }

    let mut ink = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let cy = by + pad_y + li as i32 * (char_h + line_gap);
        let mut cx = bx + pad_x;
        for ch in line.chars() {
            let g = art::glyph(glyph_key(ch));
            for (ri, grow) in g.iter().enumerate() {
                for (ci, bit) in grow.chars().enumerate() {
                    if bit == '#' {
                        ink.push((cx + ci as i32 * fp, cy + ri as i32 * fp));
                    }
                }
            }
            cx += char_w + spacing;
        }
    }
    if glass {
        let (ox, oy) = (bx + pad_x - 1, by + pad_y - 1);
        let (mw, mh) = ((text_w + 3) as usize, (text_h + 3) as usize);
        let mut halo = vec![false; mw * mh];
        for &(gx, gy) in &ink {
            for y in gy - 1..gy + fp + 2 {
                for x in gx - 1..gx + fp + 1 {
                    let (lx, ly) = ((x - ox) as usize, (y - oy) as usize);
                    if lx < mw && ly < mh {
                        halo[ly * mw + lx] = true;
                    }
                }
            }
        }
        for (y, row) in halo.chunks(mw).enumerate() {
            let mut x = 0;
            while x < mw {
                if !row[x] {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < mw && row[x] {
                    x += 1;
                }
                r(ox + start as i32, oy + y as i32, (x - start) as i32, 1, GLASS_TEXT_SHADOW);
            }
        }
    }
    for &(gx, gy) in &ink {
        r(gx, gy, fp, fp, if glass { Color32::WHITE } else { BUBBLE_FG });
    }

    Some(Rect::from_min_size(
        Pos2::new(bx as f32, by as f32),
        Vec2::new(bw as f32, (bh + 3 * ps) as f32),
    ))
}
