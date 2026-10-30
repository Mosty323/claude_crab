use egui::epaint::{ImageDelta, Primitive, Vertex};
use egui::{ClippedPrimitive, Color32, ImageData, TextureId, ViewportCommand, ViewportId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Icon, Window, WindowButtons, WindowId};

pub trait SoftApp {
    fn update(&mut self, ctx: &egui::Context);
    fn on_exit(&mut self) {}
    fn base(&mut self, _hwnd: isize, _w: usize, _h: usize, _ppp: f32, _fresh: bool) -> Option<(&[u32], u64)> {
        None
    }
}

pub struct Options {
    pub title: String,
    pub size: [f32; 2],
    pub min_size: Option<[f32; 2]>,
    pub resizable: bool,
    pub icon: Option<(Vec<u8>, u32)>,
    pub custom_frame: bool,
}

pub const TITLE_H: f32 = 32.0;
pub const CAPTION_BUTTON_W: f32 = 46.0;
pub const DOTS_W: f32 = 80.0;
const CORNER: f32 = 10.0;
static ICON: std::sync::Mutex<Option<(Vec<u8>, u32)>> = std::sync::Mutex::new(None);


#[allow(dead_code)]
pub fn set_icon(rgba: Vec<u8>, size: u32) {
    if let Ok(mut i) = ICON.lock() {
        *i = Some((rgba, size));
    }
}

static CUSTOM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);

#[derive(Default)]
struct Layer {
    dc: isize,
    bmp: isize,
    bits: usize,
    w: usize,
    h: usize,
}

impl Layer {
    fn present(&mut self, hwnd: isize, buf: &[u32], w: usize, h: usize, pos: Option<(i32, i32)>) {
        use windows_sys::Win32::Foundation::{POINT, SIZE};
        use windows_sys::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
            BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};
        if w == 0 || h == 0 || buf.len() < w * h {
            return;
        }
        unsafe {
            use windows_sys::Win32::Foundation::RECT;
            use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
            use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsZoomed};
            let zoomed = IsZoomed(hwnd as _) != 0;
            let mut wr: RECT = std::mem::zeroed();
            GetWindowRect(hwnd as _, &mut wr);
            let mut o = POINT { x: 0, y: 0 };
            ClientToScreen(hwnd as _, &mut o);
            let (ww, wh) = ((wr.right - wr.left).max(0) as usize, (wr.bottom - wr.top).max(0) as usize);
            let (ox, oy) = ((o.x - wr.left).max(0) as usize, (o.y - wr.top).max(0) as usize);
            let framed = zoomed && (ww, wh) != (w, h) && ox + w <= ww && oy + h <= wh;
            let (fw, fh, ox, oy) = if framed { (ww, wh, ox, oy) } else { (w, h, 0, 0) };
            if (self.w, self.h) != (fw, fh) || self.dc == 0 {
                if self.dc != 0 {
                    DeleteObject(self.bmp as _);
                    DeleteDC(self.dc as _);
                }
                let dc = CreateCompatibleDC(std::ptr::null_mut());
                let mut bi: BITMAPINFO = std::mem::zeroed();
                bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                bi.bmiHeader.biWidth = fw as i32;
                bi.bmiHeader.biHeight = -(fh as i32);
                bi.bmiHeader.biPlanes = 1;
                bi.bmiHeader.biBitCount = 32;
                let mut bits = std::ptr::null_mut();
                let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
                if bmp.is_null() || bits.is_null() {
                    DeleteDC(dc);
                    self.dc = 0;
                    return;
                }
                SelectObject(dc, bmp);
                (self.dc, self.bmp, self.bits, self.w, self.h) = (dc as isize, bmp as isize, bits as usize, fw, fh);
            }
            let all = std::slice::from_raw_parts_mut(self.bits as *mut u32, fw * fh);
            if framed {
                all.fill(0x0100_0000);
            }
            for (y, row) in buf.chunks_exact(w).take(h).enumerate() {
                let d = &mut all[(oy + y) * fw + ox..(oy + y) * fw + ox + w];
                d.copy_from_slice(row);
            }
            let dst = &mut all[..];
            let r = if zoomed { 0 } else { (CORNER * f32::from_bits(SCALE.load(std::sync::atomic::Ordering::Relaxed))).round() as usize };
            let r = r.min(w / 2).min(h / 2);
            for cy in 0..r {
                for cx in 0..r {
                    let (dx, dy) = (r as f32 - cx as f32 - 0.5, r as f32 - cy as f32 - 0.5);
                    let cov = (r as f32 - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                    if cov >= 1.0 {
                        continue;
                    }
                    let c = (cov * 256.0) as u32;
                    for (x, y) in [(cx, cy), (w - 1 - cx, cy), (cx, h - 1 - cy), (w - 1 - cx, h - 1 - cy)] {
                        let p = &mut dst[y * w + x];
                        let ch = |sh: u32| (((*p >> sh) & 255) * c >> 8) << sh;
                        *p = (ch(24) | ch(16) | ch(8) | ch(0)).max(0x0100_0000);
                    }
                }
            }
            let screen = GetDC(std::ptr::null_mut());
            let size = SIZE { cx: fw as i32, cy: fh as i32 };
            let src = POINT { x: 0, y: 0 };
            let at = pos.map(|(x, y)| POINT { x, y });
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            UpdateLayeredWindow(
                hwnd as _,
                screen,
                at.as_ref().map_or(std::ptr::null(), |p| p as *const POINT),
                &size,
                self.dc as _,
                &src,
                0,
                &blend,
                ULW_ALPHA,
            );
            ReleaseDC(std::ptr::null_mut(), screen);
        }
    }
}

struct Tex {
    w: usize,
    h: usize,
    px: Vec<Color32>,
}

#[derive(Default)]
struct Raster {
    w: usize,
    h: usize,
    buf: Vec<u32>,
    ui: Vec<u32>,
    ui_key: u64,
    flat_key: u64,
    base_key: u64,
    tex_gen: u64,
    textures: HashMap<TextureId, Tex>,
}

fn edge(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

#[inline]
fn blend(dst: u32, s: [u32; 4]) -> u32 {
    if s[3] == 0 {
        return dst;
    }
    if s[3] == 255 {
        return 0xFF00_0000 | s[0] << 16 | s[1] << 8 | s[2];
    }
    let inv = 255 - s[3];
    let a = s[3] + ((dst >> 24) * inv + 127) / 255;
    let r = s[0] + (((dst >> 16) & 0xFF) * inv + 127) / 255;
    let g = s[1] + (((dst >> 8) & 0xFF) * inv + 127) / 255;
    let b = s[2] + ((dst & 0xFF) * inv + 127) / 255;
    a.min(255) << 24 | r.min(255) << 16 | g.min(255) << 8 | b.min(255)
}

impl Raster {
    fn apply(&mut self, id: TextureId, d: &ImageDelta) {
        self.tex_gen += 1;
        let (w, h, px): (usize, usize, Vec<Color32>) = match &d.image {
            ImageData::Color(img) => (img.size[0], img.size[1], img.pixels.clone()),
            ImageData::Font(f) => (f.size[0], f.size[1], f.srgba_pixels(None).collect()),
        };
        match d.pos {
            None => {
                self.textures.insert(id, Tex { w, h, px });
            }
            Some([x0, y0]) => {
                if let Some(t) = self.textures.get_mut(&id) {
                    for y in 0..h {
                        for x in 0..w {
                            let (tx, ty) = (x0 + x, y0 + y);
                            if tx < t.w && ty < t.h {
                                t.px[ty * t.w + tx] = px[y * w + x];
                            }
                        }
                    }
                }
            }
        }
    }

    fn render(&mut self, w: usize, h: usize, prims: &[ClippedPrimitive], ppp: f32, base: Option<&[u32]>, base_key: u64, shadow: bool) -> bool {
        if self.w != w || self.h != h {
            self.w = w;
            self.h = h;
            self.buf = vec![0; w * h];
            self.ui_key = 0;
            self.flat_key = 0;
        }
        let key = ui_key(prims, w, h, ppp, shadow, self.tex_gen);
        let Some(base) = base.filter(|b| b.len() == w * h) else {
            if key == self.flat_key {
                return false;
            }
            self.flat_key = key;
            self.base_key = 0;
            let c = crate::widgets::bg_col();
            self.buf.fill(0xFF00_0000 | (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32);
            let mut buf = std::mem::take(&mut self.buf);
            self.draw(&mut buf, w, h, prims, ppp, shadow);
            self.buf = buf;
            return true;
        };
        self.flat_key = 0;
        if key == self.ui_key && base_key == self.base_key && self.ui.len() == w * h {
            return false;
        }
        self.base_key = base_key;
        if key != self.ui_key || self.ui.len() != w * h {
            let mut ui = std::mem::take(&mut self.ui);
            ui.clear();
            ui.resize(w * h, 0);
            self.draw(&mut ui, w, h, prims, ppp, shadow);
            self.ui = ui;
            self.ui_key = key;
        }
        self.composite(base);
        true
    }

    fn refresh_base(&mut self, base: &[u32], base_key: u64) -> bool {
        if self.ui.len() != self.w * self.h || base.len() != self.ui.len() || base_key == self.base_key {
            return false;
        }
        self.base_key = base_key;
        self.composite(base);
        true
    }

    fn composite(&mut self, base: &[u32]) {
        let (w, h) = (self.w, self.h);
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get() / 2).clamp(1, 4);
        let band = (w * h).div_ceil(threads).max(1);
        std::thread::scope(|sc| {
            for ((out, ui), base) in self.buf.chunks_mut(band).zip(self.ui.chunks(band)).zip(base.chunks(band)) {
                sc.spawn(move || {
                    for ((o, &u), &b) in out.iter_mut().zip(ui).zip(base) {
                        let a = u >> 24;
                        *o = if a == 0 {
                            b
                        } else if a == 255 {
                            u
                        } else {
                            let inv = 255 - a;
                            let ch = |sh: u32| (((u >> sh) & 255) + (((b >> sh) & 255) * inv + 127) / 255).min(255);
                            ch(24) << 24 | ch(16) << 16 | ch(8) << 8 | ch(0)
                        };
                    }
                });
            }
        });
    }

    fn draw(&self, buf: &mut [u32], w: usize, h: usize, prims: &[ClippedPrimitive], ppp: f32, shadow: bool) {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get() / 2).clamp(1, 4);
        let band = h.div_ceil(threads).max(1);
        std::thread::scope(|sc| {
            for (i, chunk) in buf.chunks_mut(band * w).enumerate() {
                sc.spawn(move || self.draw_band(chunk, (i * band) as i32, w, h, prims, ppp, shadow));
            }
        });
    }

    fn draw_band(&self, buf: &mut [u32], row0: i32, w: usize, h: usize, prims: &[ClippedPrimitive], ppp: f32, shadow: bool) {
        let rows = (buf.len() / w) as i32;
        for p in prims {
            let Primitive::Mesh(mesh) = &p.primitive else {
                continue;
            };
            let Some(tex) = self.textures.get(&mesh.texture_id) else {
                continue;
            };
            let clip = (
                ((p.clip_rect.min.x * ppp).round().max(0.0) as i32),
                ((p.clip_rect.min.y * ppp).round().max(0.0) as i32).max(row0),
                ((p.clip_rect.max.x * ppp).round().min(w as f32) as i32),
                ((p.clip_rect.max.y * ppp).round().min(h as f32) as i32).min(row0 + rows),
            );
            if clip.0 >= clip.2 || clip.1 >= clip.3 {
                continue;
            }
            for t in mesh.indices.chunks_exact(3) {
                let v = [
                    &mesh.vertices[t[0] as usize],
                    &mesh.vertices[t[1] as usize],
                    &mesh.vertices[t[2] as usize],
                ];
                if shadow && mesh.texture_id == TextureId::default() && v.iter().any(|v| v.uv != egui::epaint::WHITE_UV) {
                    let dark = |v: &Vertex| Vertex {
                        pos: v.pos + egui::vec2(1.0, 1.0) / ppp,
                        uv: v.uv,
                        color: Color32::from_black_alpha((v.color.a() as f32 * 0.6) as u8),
                    };
                    let d = [dark(v[0]), dark(v[1]), dark(v[2])];
                    tri(buf, row0, w, tex, [&d[0], &d[1], &d[2]], clip, ppp);
                }
                tri(buf, row0, w, tex, v, clip, ppp);
            }
        }
    }
}

fn ui_key(prims: &[ClippedPrimitive], w: usize, h: usize, ppp: f32, shadow: bool, tex_gen: u64) -> u64 {
    let mut k: u64 = 0xcbf2_9ce4_8422_2325 ^ (w as u64) << 32 ^ h as u64 ^ (ppp.to_bits() as u64) << 16 ^ shadow as u64 ^ tex_gen << 40;
    let mut mix = |v: u64| k = (k ^ v).wrapping_mul(0x0100_0000_01b3);
    for p in prims {
        for f in [p.clip_rect.min.x, p.clip_rect.min.y, p.clip_rect.max.x, p.clip_rect.max.y] {
            mix(f.to_bits() as u64);
        }
        let Primitive::Mesh(mesh) = &p.primitive else {
            continue;
        };
        mix(match mesh.texture_id {
            TextureId::Managed(i) => i,
            TextureId::User(i) => i ^ 1 << 63,
        });
        for v in &mesh.vertices {
            mix((v.pos.x.to_bits() as u64) << 32 | v.pos.y.to_bits() as u64);
            mix((v.uv.x.to_bits() as u64) << 32 | v.uv.y.to_bits() as u64);
            mix(u32::from_le_bytes(v.color.to_array()) as u64);
        }
        mix(mesh.indices.len() as u64);
    }
    k
}

unsafe extern "system" fn on_hit(hwnd: windows_sys::Win32::Foundation::HWND, msg: u32, wp: usize, lp: isize, _: usize, _: usize) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MINMAXINFO, WM_GETMINMAXINFO, WM_NCCALCSIZE, WM_NCHITTEST};
    if !CUSTOM.load(std::sync::atomic::Ordering::Relaxed) {
        return windows_sys::Win32::UI::Shell::DefSubclassProc(hwnd, msg, wp, lp);
    }
    match msg {
        WM_NCHITTEST => hit_test(hwnd, lp),
        WM_NCCALCSIZE if wp != 0 => {
            if windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed(hwnd) != 0 {
                if let Some((_, wk)) = monitor_of(hwnd) {
                    (*(lp as *mut windows_sys::Win32::UI::WindowsAndMessaging::NCCALCSIZE_PARAMS)).rgrc[0] = wk;
                }
            }
            0
        }
        WM_GETMINMAXINFO => {
            let r = windows_sys::Win32::UI::Shell::DefSubclassProc(hwnd, msg, wp, lp);
            if let Some((m, wk)) = monitor_of(hwnd) {
                let info = &mut *(lp as *mut MINMAXINFO);
                info.ptMaxPosition.x = wk.left - m.left;
                info.ptMaxPosition.y = wk.top - m.top;
                info.ptMaxSize.x = wk.right - wk.left;
                info.ptMaxSize.y = wk.bottom - wk.top;
            }
            r
        }
        _ => windows_sys::Win32::UI::Shell::DefSubclassProc(hwnd, msg, wp, lp),
    }
}

unsafe fn monitor_of(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<(windows_sys::Win32::Foundation::RECT, windows_sys::Win32::Foundation::RECT)> {
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    (GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi) != 0).then_some((mi.rcMonitor, mi.rcWork))
}

unsafe fn hit_test(hwnd: windows_sys::Win32::Foundation::HWND, lp: isize) -> isize {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, IsZoomed};
    let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    GetClientRect(hwnd, &mut r);
    let mut o = POINT { x: 0, y: 0 };
    ClientToScreen(hwnd, &mut o);
    let r = RECT { left: o.x, top: o.y, right: o.x + r.right, bottom: o.y + r.bottom };
    let k = f32::from_bits(SCALE.load(std::sync::atomic::Ordering::Relaxed));
    let edge = (6.0 * k) as i32;
    let (lx, ly, w, h) = (x - r.left, y - r.top, r.right - r.left, r.bottom - r.top);
    if IsZoomed(hwnd) == 0 {
        let (left, right, top, bottom) = (lx < edge, lx >= w - edge, ly < edge, ly >= h - edge);
        match (left, right, top, bottom) {
            (true, _, true, _) => return 13,
            (_, true, true, _) => return 14,
            (true, _, _, true) => return 16,
            (_, true, _, true) => return 17,
            (true, ..) => return 10,
            (_, true, ..) => return 11,
            (_, _, true, _) => return 12,
            (.., true) => return 15,
            _ => {}
        }
    }
    let on_buttons = if crate::widgets::glass() { (lx as f32) < DOTS_W * k } else { (lx as f32) >= w as f32 - 3.0 * CAPTION_BUTTON_W * k };
    if (ly as f32) < TITLE_H * k && !on_buttons {
        return 2;
    }
    1
}

unsafe extern "system" fn move_tick(hwnd: windows_sys::Win32::Foundation::HWND, _: u32, _: usize, _: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GUITHREADINFO, GUI_INMOVESIZE};
    let mut info: GUITHREADINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    let moving = GetGUIThreadInfo(windows_sys::Win32::System::Threading::GetCurrentThreadId(), &mut info) != 0 && info.flags & GUI_INMOVESIZE != 0;
    if moving && crate::widgets::glass() {
        use windows_sys::Win32::Graphics::Gdi::{RedrawWindow, RDW_INTERNALPAINT, RDW_INVALIDATE};
        RedrawWindow(hwnd, std::ptr::null(), std::ptr::null_mut(), RDW_INVALIDATE | RDW_INTERNALPAINT);
    }
}

fn sample(tex: &Tex, u: f32, v: f32) -> Color32 {
    let x = ((u * tex.w as f32) as isize).clamp(0, tex.w as isize - 1) as usize;
    let y = ((v * tex.h as f32) as isize).clamp(0, tex.h as isize - 1) as usize;
    tex.px[y * tex.w + x]
}

fn tri(buf: &mut [u32], row0: i32, w: usize, tex: &Tex, v: [&Vertex; 3], clip: (i32, i32, i32, i32), ppp: f32) {
    let p = [
        (v[0].pos.x * ppp, v[0].pos.y * ppp),
        (v[1].pos.x * ppp, v[1].pos.y * ppp),
        (v[2].pos.x * ppp, v[2].pos.y * ppp),
    ];
    let area = edge(p[0], p[1], p[2]);
    if area.abs() < 1e-6 {
        return;
    }
    let owns = |a: (f32, f32), b: (f32, f32)| (b.1 < a.1 || (b.1 == a.1 && b.0 > a.0)) != (area < 0.0);
    let own = [owns(p[1], p[2]), owns(p[2], p[0]), owns(p[0], p[1])];
    let inv = 1.0 / area;
    let x0 = (p[0].0.min(p[1].0).min(p[2].0).floor() as i32).max(clip.0);
    let x1 = (p[0].0.max(p[1].0).max(p[2].0).ceil() as i32).min(clip.2);
    let y0 = (p[0].1.min(p[1].1).min(p[2].1).floor() as i32).max(clip.1);
    let y1 = (p[0].1.max(p[1].1).max(p[2].1).ceil() as i32).min(clip.3);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let flat = v[0].color == v[1].color && v[1].color == v[2].color && v[0].uv == v[1].uv && v[1].uv == v[2].uv;
    let mul = |c: Color32, t: Color32| -> [u32; 4] {
        [
            (c.r() as u32 * t.r() as u32 + 127) / 255,
            (c.g() as u32 * t.g() as u32 + 127) / 255,
            (c.b() as u32 * t.b() as u32 + 127) / 255,
            (c.a() as u32 * t.a() as u32 + 127) / 255,
        ]
    };
    let solid = flat.then(|| mul(v[0].color, sample(tex, v[0].uv.x, v[0].uv.y)));
    if let Some(s) = solid {
        if s[3] == 0 {
            return;
        }
    }
    let cf = |c: Color32| [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32];
    let c = [cf(v[0].color), cf(v[1].color), cf(v[2].color)];
    for y in y0..y1 {
        let py = y as f32 + 0.5;
        let row = (y - row0) as usize * w;
        for x in x0..x1 {
            let pt = (x as f32 + 0.5, py);
            let w0 = edge(p[1], p[2], pt) * inv;
            let w1 = edge(p[2], p[0], pt) * inv;
            let w2 = edge(p[0], p[1], pt) * inv;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 || (w0 == 0.0 && !own[0]) || (w1 == 0.0 && !own[1]) || (w2 == 0.0 && !own[2]) {
                continue;
            }
            let s = match solid {
                Some(s) => s,
                None => {
                    let u = v[0].uv.x * w0 + v[1].uv.x * w1 + v[2].uv.x * w2;
                    let vv = v[0].uv.y * w0 + v[1].uv.y * w1 + v[2].uv.y * w2;
                    let t = sample(tex, u, vv);
                    let k = |i: usize| (c[0][i] * w0 + c[1][i] * w1 + c[2][i] * w2).round().clamp(0.0, 255.0) as u8;
                    mul(Color32::from_rgba_premultiplied(k(0), k(1), k(2), k(3)), t)
                }
            };
            let i = row + x as usize;
            buf[i] = blend(buf[i], s);
        }
    }
}

fn present(hwnd: isize, buf: &[u32], w: usize, h: usize) {
    use windows_sys::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    };
    if w == 0 || h == 0 || buf.len() < w * h {
        return;
    }
    unsafe {
        let hwnd = hwnd as *mut core::ffi::c_void;
        let dc = GetDC(hwnd);
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w as i32;
        bi.bmiHeader.biHeight = -(h as i32);
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        SetDIBitsToDevice(
            dc,
            0,
            0,
            w as u32,
            h as u32,
            0,
            0,
            0,
            h as u32,
            buf.as_ptr() as *const core::ffi::c_void,
            &bi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(hwnd, dc);
    }
}

struct Runner<A: SoftApp> {
    opts: Options,
    app: A,
    ctx: egui::Context,
    window: Option<Arc<Window>>,
    hwnd: isize,
    state: Option<egui_winit::State>,
    raster: Raster,
    next_paint: Option<Instant>,
    closing: bool,
    last: Option<(Vec<ClippedPrimitive>, f32)>,
    last_paint: Instant,
    next_base: Instant,
    layer: Layer,
}

impl<A: SoftApp> Runner<A> {
    fn paint(&mut self) {
        let started = Instant::now();
        let (Some(window), Some(state)) = (self.window.clone(), self.state.as_mut()) else {
            return;
        };
        if let Some((rgba, s)) = ICON.lock().ok().and_then(|mut i| i.take()) {
            use winit::platform::windows::WindowExtWindows;
            window.set_window_icon(Icon::from_rgba(rgba.clone(), s, s).ok());
            window.set_taskbar_icon(Icon::from_rgba(rgba, s, s).ok());
        }
        let raw = state.take_egui_input(&window);
        let app = &mut self.app;
        let out = self.ctx.run(raw, |ctx| app.update(ctx));
        state.handle_platform_output(&window, out.platform_output);
        let ppp = out.pixels_per_point;
        for (id, d) in &out.textures_delta.set {
            self.raster.apply(*id, d);
        }
        let size = window.inner_size();
        if size.width > 0 && size.height > 0 {
            let prims = self.ctx.tessellate(out.shapes, ppp);
            let (w, h) = (size.width as usize, size.height as usize);
            let base = self.app.base(self.hwnd, w, h, ppp, true);
            if self.raster.render(w, h, &prims, ppp, base.map(|b| b.0), base.map_or(0, |b| b.1), crate::widgets::glass()) {
                self.show();
            }
            self.last_paint = Instant::now();
            self.last = Some((prims, ppp));
        }
        for id in &out.textures_delta.free {
            self.raster.textures.remove(id);
        }
        if let Some(vo) = out.viewport_output.get(&ViewportId::ROOT) {
            for c in &vo.commands {
                match c {
                    ViewportCommand::Close => self.closing = true,
                    ViewportCommand::Minimized(on) => window.set_minimized(*on),
                    ViewportCommand::Maximized(_) => window.set_maximized(!window.is_maximized()),
                    _ => {}
                }
            }
            if vo.repaint_delay < Duration::from_secs(3600) {
                let due = (Instant::now() + vo.repaint_delay).max(started + Duration::from_millis(16));
                self.next_paint = Some(self.next_paint.map_or(due, |n| n.min(due)));
            }
        }
    }
}

impl<A: SoftApp> Runner<A> {
    fn show(&mut self) {
        let (w, h) = (self.raster.w, self.raster.h);
        if !self.opts.custom_frame {
            present(self.hwnd, &self.raster.buf, w, h);
            return;
        }
        if let Some(window) = &self.window {
            SCALE.store((window.scale_factor() as f32).to_bits(), std::sync::atomic::Ordering::Relaxed);
        }
        self.layer.present(self.hwnd, &self.raster.buf, w, h, None);
    }

    fn repaint_moved(&mut self) {
        let (Some(window), Some((prims, ppp))) = (self.window.clone(), self.last.take()) else {
            return;
        };
        let size = window.inner_size();
        let (w, h) = (size.width as usize, size.height as usize);
        if (w, h) == (self.raster.w, self.raster.h) {
            if let Some((b, k)) = self.app.base(self.hwnd, w, h, ppp, false) {
                if self.raster.refresh_base(b, k) {
                    self.show();
                }
            }
            self.last_paint = Instant::now();
        }
        self.last = Some((prims, ppp));
    }
}

impl<A: SoftApp> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        CUSTOM.store(self.opts.custom_frame, std::sync::atomic::Ordering::Relaxed);
        let mut attrs = Window::default_attributes()
            .with_decorations(!self.opts.custom_frame)
            .with_title(self.opts.title.clone())
            .with_inner_size(LogicalSize::new(self.opts.size[0], self.opts.size[1]))
            .with_resizable(self.opts.resizable);
        if !self.opts.resizable {
            attrs = attrs.with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE);
        }
        if let Some(m) = self.opts.min_size {
            attrs = attrs.with_min_inner_size(LogicalSize::new(m[0], m[1]));
        }
        if let Some((rgba, s)) = self.opts.icon.take() {
            attrs = attrs.with_window_icon(Icon::from_rgba(rgba, s, s).ok());
        }
        let Ok(window) = el.create_window(attrs) else {
            el.exit();
            return;
        };
        let window = Arc::new(window);
        if let Ok(h) = window.window_handle() {
            if let RawWindowHandle::Win32(w) = h.as_raw() {
                self.hwnd = w.hwnd.get();
                unsafe {
                    if self.opts.custom_frame {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED};
                        let ex = GetWindowLongPtrW(self.hwnd as _, GWL_EXSTYLE);
                        SetWindowLongPtrW(self.hwnd as _, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
                    }
                    windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(self.hwnd as _, 0x6C61, 1000 / crate::widgets::GLASS_FPS, Some(move_tick));
                    windows_sys::Win32::UI::Shell::SetWindowSubclass(self.hwnd as _, Some(on_hit), 0x6C61, 0);
                }
            }
        }
        self.state = Some(egui_winit::State::new(
            self.ctx.clone(),
            ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            None,
            None,
        ));
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if matches!(event, WindowEvent::RedrawRequested) {
            self.paint();
            if self.closing {
                el.exit();
            }
            return;
        }
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let resp = state.on_window_event(&window, &event);
        if matches!(event, WindowEvent::Moved(_)) && crate::widgets::glass() && self.last_paint.elapsed() >= Duration::from_millis(15) {
            self.repaint_moved();
        }
        if matches!(event, WindowEvent::CloseRequested) {
            self.closing = true;
            window.request_redraw();
        }
        if resp.repaint {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let glass = crate::widgets::glass();
        if glass && now >= self.next_base {
            self.next_base = now + Duration::from_micros(1_000_000 / crate::widgets::GLASS_FPS as u64);
            self.repaint_moved();
        }
        if self.next_paint.is_some_and(|due| now >= due) {
            self.next_paint = None;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        let wake = if glass { Some(self.next_paint.map_or(self.next_base, |n| n.min(self.next_base))) } else { self.next_paint };
        el.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

pub fn run<A: SoftApp>(opts: Options, ctx: egui::Context, app: A) {
    let Ok(el) = EventLoop::new() else {
        return;
    };
    let mut runner = Runner {
        opts,
        app,
        ctx,
        window: None,
        hwnd: 0,
        state: None,
        raster: Raster::default(),
        next_paint: None,
        last: None,
        last_paint: Instant::now(),
        next_base: Instant::now(),
        layer: Layer::default(),
        closing: false,
    };
    let _ = el.run_app(&mut runner);
    runner.app.on_exit();
}

#[cfg(test)]
#[derive(Default)]
pub struct Offscreen(Raster);

#[cfg(test)]
impl Offscreen {
    pub fn frame(&mut self, ctx: &egui::Context, out: egui::FullOutput, w: usize, h: usize, base: Option<&[u32]>) -> &[u32] {
        for (id, d) in &out.textures_delta.set {
            self.0.apply(*id, d);
        }
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        self.0.render(w, h, &prims, out.pixels_per_point, base, self.0.base_key.wrapping_add(1), crate::widgets::glass());
        &self.0.buf
    }
}
