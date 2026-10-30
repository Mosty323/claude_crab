use std::collections::HashMap;

const THICKNESS: f32 = 80.0;
const BEZEL: f32 = 60.0;
const IOR: f32 = 3.0;
const SPEC_OPACITY: f32 = 0.5;
const SPEC_SATURATION: f32 = 4.0;
const SPEC_ANGLE: f32 = std::f32::consts::PI / 3.0;
const TINT: f32 = 0.06;
const INNER_BLUR: f32 = 20.0;
const INNER_SPREAD: f32 = -5.0;
const OUTER_BLUR: f32 = 24.0;
const OUTER_ALPHA: f32 = 0.18;
const OUTER_DY: f32 = 4.0;

fn surface(x: f32) -> f32 {
    (1.0 - (1.0 - x).powi(4)).max(0.0).powf(0.25)
}

fn profile(thickness: f32, bezel: f32, samples: usize) -> Vec<f32> {
    let eta = 1.0 / IOR;
    (0..samples)
        .map(|i| {
            let x = i as f32 / samples as f32;
            let y = surface(x);
            let dx = if x < 1.0 { 1e-4 } else { -1e-4 };
            let deriv = (surface(x + dx) - y) / dx;
            let mag = (deriv * deriv + 1.0).sqrt();
            let (nx, ny) = (-deriv / mag, -1.0 / mag);
            let k = 1.0 - eta * eta * (1.0 - ny * ny);
            if k < 0.0 {
                return 0.0;
            }
            let sq = k.sqrt();
            let (rx, ry) = (-(eta * ny + sq) * nx, eta - (eta * ny + sq) * ny);
            rx * ((y * bezel + thickness) / ry)
        })
        .collect()
}

struct Cell {
    op: f32,
    dx: f32,
    dy: f32,
    spec: f32,
    inner: f32,
}

struct Rim {
    x: u16,
    y: u16,
    sx: u16,
    sy: u16,
    fancy: bool,
    glow: u32,
    op: f32,
    spec: f32,
    inner: f32,
}

pub struct Bg<'a> {
    pub px: &'a [u32],
    pub x0: i32,
    pub y0: i32,
    pub w: usize,
    pub h: usize,
}

impl Bg<'_> {
    #[inline]
    fn index(&self, x: i32, y: i32) -> usize {
        let gx = (x - self.x0).clamp(0, self.w as i32 - 1) as usize;
        let gy = (y - self.y0).clamp(0, self.h as i32 - 1) as usize;
        gy * self.w + gx
    }

    #[inline]
    fn at(&self, x: i32, y: i32) -> u32 {
        self.px[self.index(x, y)]
    }
}

struct Maps {
    rim: Vec<Rim>,
    spans: Vec<(u16, u16)>,
    shadow: Vec<(i16, i16, u8)>,
}

fn phi_c(t: f32) -> f32 {
    let x = t / std::f32::consts::SQRT_2;
    let s = 1.0 / (1.0 + 0.3275911 * x.abs());
    let erf = 1.0 - (((((1.061405429 * s - 1.453152027) * s) + 1.421413741) * s - 0.284496736) * s + 0.254829592) * s * (-x * x).exp();
    let erf = if x < 0.0 { -erf } else { erf };
    0.5 * (1.0 - erf)
}

fn rounded_sd(px: f32, py: f32, w: f32, h: f32, r: f32) -> f32 {
    let qx = (px - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (py - h / 2.0).abs() - (h / 2.0 - r);
    let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    out + qx.max(qy).min(0.0) - r
}

fn cells(w: usize, h: usize, r: f32, k: f32, rim: f32) -> Vec<Cell> {
    let ri = r.floor();
    let bezel = (BEZEL * k).min(rim).min(ri - 1.0).min(w.min(h) as f32 / 2.0 - 1.0).max(1.0);
    let prof = profile(THICKNESS * k, bezel, 128);
    let n = prof.len();
    let rb_sq = (ri - bezel).max(0.0).powi(2);
    let (wb, hb) = (w as f32 - ri * 2.0, h as f32 - ri * 2.0);
    let sv = (SPEC_ANGLE.cos(), SPEC_ANGLE.sin());
    let (sigma, spread) = (INNER_BLUR * k / 2.0, INNER_SPREAD * k);
    let mut cells = Vec::with_capacity(w * h);
    for y1 in 0..h {
        for x1 in 0..w {
            let (fx, fy) = (x1 as f32, y1 as f32);
            let x = if fx < ri { fx - ri } else if fx >= w as f32 - ri { fx - ri - wb } else { 0.0 };
            let y = if fy < ri { fy - ri } else if fy >= h as f32 - ri { fy - ri - hb } else { 0.0 };
            let d_sq = x * x + y * y;
            let mut c = Cell { op: 0.0, dx: 0.0, dy: 0.0, spec: 0.0, inner: 0.0 };
            let e = -rounded_sd(fx + 0.5, fy + 0.5, w as f32, h as f32, r);
            c.op = (e + 0.5).clamp(0.0, 1.0);
            if c.op <= 0.0 {
                cells.push(c);
                continue;
            }
            let dist = d_sq.sqrt();
            c.inner = phi_c((e - spread) / sigma.max(0.5));
            if dist > 0.0 && d_sq >= rb_sq {
                let from_side = ri - dist;
                let (cos, sin) = (x / dist, y / dist);
                let bi = (((from_side / bezel).powi(2) * n as f32) as usize).min(n - 1);
                let disp = prof[bi];
                c.dx = -cos * disp * c.op / 2.0;
                c.dy = -sin * disp * c.op / 2.0;
            }
            if dist > 0.0 {
                let from_side = ri - dist;
                let dot = ((x / dist) * sv.0 + (-y / dist) * sv.1).abs();
                let edge = (1.0 - (1.0 - from_side / k.max(1.0)).powi(2)).max(0.0).sqrt();
                let coeff = dot * edge;
                c.spec = coeff;
            }
            cells.push(c);
        }
    }
    cells
}

fn cloud_sd(x: f32, y: f32, w: f32, h: f32) -> f32 {
    let b = (h * 0.3).clamp(4.0, 30.0);
    let i = b * 0.55;
    let circle = |cx: f32, cy: f32, r: f32| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r;
    let mut d = rounded_sd(x - i, y - i, w - 2.0 * i, h - 2.0 * i, b);
    let n = ((w - 2.0 * b) / (1.5 * b)).ceil().max(1.0) as i32;
    let step = (w - 2.0 * b) / n as f32;
    for k in 0..=n {
        d = d.min(circle(b + step * k as f32, b, b));
    }
    for k in 0..n {
        d = d.min(circle(b + step * (k as f32 + 0.5), h - b, b));
    }
    d.min(circle(b, h / 2.0, b)).min(circle(w - b, h / 2.0, b))
}

fn cells_sdf(w: usize, h: usize, k: f32, rim: f32, sd: &dyn Fn(f32, f32) -> f32) -> Vec<Cell> {
    let bezel = (BEZEL * k).min(rim).min(w.min(h) as f32 / 2.0 - 1.0).max(1.0);
    let prof = profile(THICKNESS * k, bezel, 128);
    let n = prof.len();
    let sv = (SPEC_ANGLE.cos(), SPEC_ANGLE.sin());
    let (sigma, spread) = (INNER_BLUR * k / 2.0, INNER_SPREAD * k);
    let mut cells = Vec::with_capacity(w * h);
    for y1 in 0..h {
        for x1 in 0..w {
            let (px, py) = (x1 as f32 + 0.5, y1 as f32 + 0.5);
            let e = -sd(px, py);
            let mut c = Cell { op: (e + 0.5).clamp(0.0, 1.0), dx: 0.0, dy: 0.0, spec: 0.0, inner: 0.0 };
            if c.op <= 0.0 {
                cells.push(c);
                continue;
            }
            c.inner = phi_c((e - spread) / sigma.max(0.5));
            let (gx, gy) = (sd(px + 0.5, py) - sd(px - 0.5, py), sd(px, py + 0.5) - sd(px, py - 0.5));
            let len = (gx * gx + gy * gy).sqrt();
            if len > 1e-4 && e < bezel + 2.0 {
                let (nx, ny) = (gx / len, gy / len);
                let from_side = e.max(0.0);
                if from_side < bezel {
                    let bi = (((from_side / bezel).powi(2) * n as f32) as usize).min(n - 1);
                    c.dx = -nx * prof[bi] * c.op / 2.0;
                    c.dy = -ny * prof[bi] * c.op / 2.0;
                }
                let edge = (1.0 - (1.0 - from_side / k.max(1.0)).powi(2)).max(0.0).sqrt();
                c.spec = (nx * sv.0 - ny * sv.1).abs() * edge;
            }
            cells.push(c);
        }
    }
    cells
}

fn maps(w: usize, h: usize, r: f32, k: f32, rim: f32, cloud: bool) -> Maps {
    let (fw, fh) = (w as f32, h as f32);
    let sd = |x: f32, y: f32| if cloud { cloud_sd(x, y, fw, fh) } else { rounded_sd(x, y, fw, fh, r) };
    let cells = if cloud { cells_sdf(w, h, k, rim, &sd) } else { cells(w, h, r, k, rim) };
    let plain = |c: &Cell| c.op >= 1.0 && c.dx.round() == 0.0 && c.dy.round() == 0.0 && c.spec < 1e-3 && c.inner < 0.5 / 255.0;
    let mut rim = Vec::new();
    let mut spans = Vec::with_capacity(h);
    for y in 0..h {
        let row = &cells[y * w..(y + 1) * w];
        let a = row.iter().position(|c| plain(c)).unwrap_or(w);
        let b = row.iter().rposition(|c| plain(c)).map_or(a, |b| b + 1);
        spans.push((a as u16, b as u16));
        for (x, c) in row.iter().enumerate() {
            if c.op <= 0.0 || (x >= a && x < b && plain(c)) {
                continue;
            }
            rim.push(Rim {
                x: x as u16,
                y: y as u16,
                sx: (x as f32 + c.dx).round().clamp(0.0, w as f32 - 1.0) as u16,
                sy: (y as f32 + c.dy).round().clamp(0.0, h as f32 - 1.0) as u16,
                fancy: c.op < 1.0 || c.spec >= 1e-3,
                glow: (c.inner * 256.0).round() as u32,
                op: c.op,
                spec: c.spec,
                inner: c.inner,
            });
        }
    }
    let m = (OUTER_BLUR * k) as i32 + 4;
    let sigma = OUTER_BLUR * k / 2.0;
    let mut shadow = Vec::new();
    for y in -m..h as i32 + m {
        for x in -m..w as i32 + m {
            let (lx, ly) = (x as f32 + 0.5, y as f32 + 0.5);
            if sd(lx, ly) <= 0.0 {
                continue;
            }
            let a = (OUTER_ALPHA * phi_c(sd(lx, ly - OUTER_DY * k) / sigma) * 255.0).round();
            if a >= 1.0 {
                shadow.push((x as i16, y as i16, a as u8));
            }
        }
    }
    Maps { rim, spans, shadow }
}

pub fn exclude_from_capture(hwnd: isize, on: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE};
    unsafe {
        SetWindowDisplayAffinity(hwnd as _, if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
    }
}

pub struct Shape {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    pub r: f32,
    pub rim: f32,
    pub cloud: bool,
    pub clip: (i32, i32, i32, i32),
}

#[derive(Default)]
pub struct Glass {
    maps: HashMap<(usize, usize, u32, u32, u32, bool), Maps>,
}

fn over(dst: u32, rgb: [f32; 3], a: f32) -> u32 {
    let inv = 1.0 - a;
    let ch = |sh: u32, v: f32| ((v * a + ((dst >> sh) & 255) as f32 * inv).round().clamp(0.0, 255.0)) as u32;
    ch(24, 255.0) << 24 | ch(16, rgb[0]) << 16 | ch(8, rgb[1]) << 8 | ch(0, rgb[2])
}

impl Glass {
    pub fn apply(&mut self, out: &mut [u32], ow: usize, oh: usize, k: f32, s: &Shape, bg: &Bg) {
        self.prepare(k, s);
        self.draw(out, 0, ow, oh, k, s, bg);
    }

    fn key(k: f32, s: &Shape) -> (usize, usize, u32, u32, u32, bool) {
        (s.w, s.h, s.r.min(s.w as f32 / 2.0).min(s.h as f32 / 2.0).floor() as u32, k.to_bits(), s.rim.to_bits(), s.cloud)
    }

    fn prepare(&mut self, k: f32, s: &Shape) {
        if s.w >= 4 && s.h >= 4 {
            let key = Self::key(k, s);
            if self.maps.len() > 64 && !self.maps.contains_key(&key) {
                self.maps.clear();
            }
            self.maps.entry(key).or_insert_with(|| maps(s.w, s.h, key.2 as f32, k, s.rim, s.cloud));
        }
    }

    fn samples(&self, k: f32, s: &Shape, ow: usize, oh: usize, bg: &Bg, out: &mut Vec<usize>) {
        let Some(maps) = self.maps.get(&Self::key(k, s)) else {
            return;
        };
        let (cx0, cy0) = (s.clip.0.max(0), s.clip.1.max(0));
        let (cx1, cy1) = (s.clip.2.min(ow as i32), s.clip.3.min(oh as i32));
        for c in &maps.rim {
            let (x, y) = (s.x + c.x as i32, s.y + c.y as i32);
            if x >= cx0 && x < cx1 && y >= cy0 && y < cy1 {
                out.push(bg.index(s.x + c.sx as i32, s.y + c.sy as i32));
            }
        }
    }

    fn draw(&self, out: &mut [u32], row0: i32, ow: usize, oh: usize, k: f32, s: &Shape, bg: &Bg) {
        let (x0, y0, gw, gh) = (s.x, s.y, s.w, s.h);
        let Some(maps) = self.maps.get(&Self::key(k, s)) else {
            return;
        };
        let cx0 = s.clip.0.max(0);
        let cy0 = s.clip.1.max(row0);
        let cx1 = s.clip.2.min(ow as i32);
        let cy1 = s.clip.3.min(oh as i32).min(row0 + (out.len() / ow) as i32);
        let m = (OUTER_BLUR * k) as i32 + 4;
        if x0 - m >= cx1 || y0 - m >= cy1 || x0 + gw as i32 + m <= cx0 || y0 + gh as i32 + m <= cy0 {
            return;
        }
        let inside = |x: i32, y: i32| x >= cx0 && x < cx1 && y >= cy0 && y < cy1;

        let (lo, hi) = (cy0 - y0, cy1 - y0);
        let shadow = &maps.shadow[maps.shadow.partition_point(|s| (s.1 as i32) < lo)..];
        for &(dx, dy, a) in shadow {
            if dy as i32 >= hi {
                break;
            }
            let (x, y) = (x0 + dx as i32, y0 + dy as i32);
            if inside(x, y) {
                let i = (y - row0) as usize * ow + x as usize;
                out[i] = darken(out[i], a as u32);
            }
        }

        for (yy, &(a, b)) in maps.spans.iter().enumerate() {
            let y = y0 + yy as i32;
            if y < cy0 || y >= cy1 {
                continue;
            }
            let xa = (x0 + a as i32).max(cx0);
            let xb = (x0 + b as i32).min(cx1);
            if xa >= xb {
                continue;
            }
            let row = (y - row0) as usize * ow;
            out[row + xa as usize..row + xb as usize].fill(TINT_PX);
        }

        let rgb = |p: u32| [((p >> 16) & 255) as f32, ((p >> 8) & 255) as f32, (p & 255) as f32];
        let rim = &maps.rim[maps.rim.partition_point(|c| (c.y as i32) < lo)..];
        for c in rim {
            if c.y as i32 >= hi {
                break;
            }
            let (x, y) = (x0 + c.x as i32, y0 + c.y as i32);
            if !inside(x, y) {
                continue;
            }
            let i = (y - row0) as usize * ow + x as usize;
            let src = bg.at(x0 + c.sx as i32, y0 + c.sy as i32);
            if !c.fancy {
                out[i] = glow(tint(src), c.glow);
                continue;
            }
            let disp = rgb(src);
            let lum = 0.2126 * disp[0] + 0.7152 * disp[1] + 0.0722 * disp[2];
            let a_s = c.spec * c.spec * c.op;
            let a_f = a_s * SPEC_OPACITY;
            let mut o = [0.0f32; 3];
            for i in 0..3 {
                let sat = (lum + (disp[i] - lum) * SPEC_SATURATION).clamp(0.0, 255.0);
                let with_sat = sat * a_s + disp[i] * (1.0 - a_s);
                let v = 255.0 * c.spec * a_f + with_sat * (1.0 - a_f);
                let v = v + (255.0 - v) * TINT;
                o[i] = v + (255.0 - v) * c.inner;
            }
            out[i] = over(out[i], o, c.op);
        }
    }
}

#[inline]
fn glow(p: u32, g: u32) -> u32 {
    if g == 0 {
        return p;
    }
    let ch = |sh: u32| {
        let v = (p >> sh) & 255;
        v + (((255 - v) * g) >> 8)
    };
    0xFF00_0000 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

#[inline]
fn darken(p: u32, a: u32) -> u32 {
    let inv = 256 - a;
    let ch = |sh: u32| (((p >> sh) & 255) * inv) >> 8;
    (a + ch(24)).min(255) << 24 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

const TINT_PX: u32 = {
    let a = (TINT * 255.0) as u32;
    a << 24 | a << 16 | a << 8 | a
};
const CLEAR_PX: u32 = 0x0100_0000;
pub const RIM: f32 = 6.0;

#[inline]
fn tint(p: u32) -> u32 {
    const T: u32 = (TINT * 256.0) as u32;
    let ch = |sh: u32| {
        let v = (p >> sh) & 255;
        v + (((255 - v) * T) >> 8)
    };
    0xFF00_0000 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

type Area = (i32, i32, usize, usize);
pub const FPS: u64 = crate::widgets::GLASS_FPS as u64;
const CAPTURE_MARGIN: i32 = 128;

struct Shot {
    area: Area,
    px: Vec<u32>,
}

struct Wgc {
    device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    ctx: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    pool: windows::Graphics::Capture::Direct3D11CaptureFramePool,
    session: windows::Graphics::Capture::GraphicsCaptureSession,
    staging: Option<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    staging_size: (usize, usize),
    last: Option<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    last_want: Area,
    area: Area,
}

impl Drop for Wgc {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

impl Wgc {
    fn open(cx: i32, cy: i32) -> Option<Self> {
        use windows::Graphics::Capture::GraphicsCaptureItem;
        use windows::Win32::Foundation::POINT;
        use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
        use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
        unsafe {
            let monitor = MonitorFromPoint(POINT { x: cx, y: cy }, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
                return None;
            }
            let item: GraphicsCaptureItem = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().ok()?.CreateForMonitor(monitor).ok()?;
            Self::start(item, (mi.rcMonitor.left, mi.rcMonitor.top))
        }
    }

    fn window(hwnd: isize) -> Option<Self> {
        use windows::Graphics::Capture::GraphicsCaptureItem;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
        let b = window_bounds(hwnd)?;
        let item: GraphicsCaptureItem = unsafe { windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().ok()?.CreateForWindow(HWND(hwnd as _)).ok()? };
        Self::start(item, (b.0, b.1))
    }

    fn start(item: windows::Graphics::Capture::GraphicsCaptureItem, origin: (i32, i32)) -> Option<Self> {
        use windows::core::Interface;
        use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureAccess, GraphicsCaptureAccessKind};
        use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
        use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
        use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION};
        use windows::Win32::Graphics::Dxgi::IDXGIDevice;
        use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
        unsafe {
            let size = item.Size().ok()?;
            let mut device = None;
            let mut ctx = None;
            D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, None, D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx)).ok()?;
            let device = device?;
            let d3d: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>().ok()?).ok()?.cast().ok()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size).ok()?;
            let session = pool.CreateCaptureSession(&item).ok()?;
            let _ = session.SetIsCursorCaptureEnabled(false);
            if let Ok(op) = GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless) {
                let _ = op.get();
            }
            let _ = session.SetIsBorderRequired(false);
            let _ = session.SetMinUpdateInterval(windows::Foundation::TimeSpan { Duration: 10_000_000 / FPS as i64 });
            session.StartCapture().ok()?;
            let area = (origin.0, origin.1, size.Width.max(0) as usize, size.Height.max(0) as usize);
            Some(Self { device, ctx: ctx?, pool, session, staging: None, staging_size: (0, 0), last: None, last_want: (0, 0, 0, 0), area })
        }
    }

    fn grab(&mut self, want: Area, px: &mut Vec<u32>) -> Result<Option<Area>, ()> {
        use windows::core::Interface;
        use windows::Win32::Graphics::Direct3D11::{ID3D11Texture2D, D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING};
        use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
        if let Ok(frame) = self.pool.TryGetNextFrame() {
            unsafe {
                let tex: ID3D11Texture2D = frame.Surface().map_err(|_| ())?.cast::<IDirect3DDxgiInterfaceAccess>().map_err(|_| ())?.GetInterface().map_err(|_| ())?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut desc);
                let stale = self.last.as_ref().map_or(true, |l| {
                    let mut d = D3D11_TEXTURE2D_DESC::default();
                    l.GetDesc(&mut d);
                    (d.Width, d.Height) != (desc.Width, desc.Height)
                });
                if stale {
                    desc.BindFlags = 0;
                    desc.MiscFlags = 0;
                    desc.CPUAccessFlags = 0;
                    let mut t = None;
                    self.device.CreateTexture2D(&desc, None, Some(&mut t)).map_err(|_| ())?;
                    self.last = t;
                }
                self.ctx.CopyResource(self.last.as_ref().ok_or(())?, &tex);
            }
            let _ = frame.Close();
        } else if want == self.last_want || self.last.is_none() {
            return Ok(None);
        }
        self.last_want = want;
        let region = unsafe {
            let tex = self.last.as_ref().ok_or(())?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            tex.GetDesc(&mut desc);
            let (tw, th) = (desc.Width as i32, desc.Height as i32);
            let x0 = (want.0 - CAPTURE_MARGIN - self.area.0).clamp(0, tw);
            let y0 = (want.1 - CAPTURE_MARGIN - self.area.1).clamp(0, th);
            let x1 = (want.0 + want.2 as i32 + CAPTURE_MARGIN - self.area.0).clamp(x0, tw);
            let y1 = (want.1 + want.3 as i32 + CAPTURE_MARGIN - self.area.1).clamp(y0, th);
            let (rw, rh) = ((x1 - x0) as usize, (y1 - y0) as usize);
            if rw == 0 || rh == 0 {
                return Ok(None);
            }
            if self.staging_size != (tw as usize, th as usize) {
                desc.Usage = D3D11_USAGE_STAGING;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                desc.BindFlags = 0;
                desc.MiscFlags = 0;
                desc.MipLevels = 1;
                desc.ArraySize = 1;
                let mut st = None;
                self.device.CreateTexture2D(&desc, None, Some(&mut st)).map_err(|_| ())?;
                self.staging = st;
                self.staging_size = (tw as usize, th as usize);
            }
            let st = self.staging.as_ref().ok_or(())?;
            let bx = D3D11_BOX { left: x0 as u32, top: y0 as u32, front: 0, right: x1 as u32, bottom: y1 as u32, back: 1 };
            self.ctx.CopySubresourceRegion(st, 0, 0, 0, 0, tex, 0, Some(&bx));
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut map)).map_err(|_| ())?;
            px.resize(rw * rh, 0);
            for y in 0..rh {
                let src = std::slice::from_raw_parts((map.pData as *const u8).add(y * map.RowPitch as usize) as *const u32, rw);
                px[y * rw..(y + 1) * rw].copy_from_slice(src);
            }
            self.ctx.Unmap(st, 0);
            (self.area.0 + x0, self.area.1 + y0, rw, rh)
        };
        px.iter_mut().for_each(|p| *p |= 0xFF00_0000);
        Ok(Some(region))
    }
}

fn window_bounds(hwnd: isize) -> Option<Area> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let ok = unsafe { DwmGetWindowAttribute(hwnd as _, DWMWA_EXTENDED_FRAME_BOUNDS as u32, &mut r as *mut RECT as _, std::mem::size_of::<RECT>() as u32) } == 0;
    (ok && r.right > r.left && r.bottom > r.top).then(|| (r.left, r.top, (r.right - r.left) as usize, (r.bottom - r.top) as usize))
}

fn overlaps(a: Area, b: Area) -> bool {
    a.0 < b.0 + b.2 as i32 && b.0 < a.0 + a.2 as i32 && a.1 < b.1 + b.3 as i32 && b.1 < a.1 + a.3 as i32
}

fn covers(a: Area, b: Area) -> bool {
    a.0 <= b.0 && a.1 <= b.1 && a.0 + a.2 as i32 >= b.0 + b.2 as i32 && a.1 + a.3 as i32 >= b.1 + b.3 as i32
}

fn windows_behind(own: isize, want: Area) -> Vec<(isize, Area)> {
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, IsIconic, IsWindowVisible, GW_HWNDNEXT};
    let mut out = Vec::new();
    let mut h = unsafe { GetWindow(own as _, GW_HWNDNEXT) };
    while !h.is_null() && out.len() < 6 {
        let hw = h as isize;
        let mut cloaked = 0u32;
        let visible = unsafe {
            IsWindowVisible(h) != 0 && IsIconic(h) == 0 && (DwmGetWindowAttribute(h, DWMWA_CLOAKED as u32, &mut cloaked as *mut u32 as _, 4) != 0 || cloaked == 0)
        };
        if visible {
            if let Some(b) = window_bounds(hw).filter(|b| overlaps(*b, want)) {
                out.push((hw, b));
                if covers(b, want) {
                    break;
                }
            }
        }
        h = unsafe { GetWindow(h, GW_HWNDNEXT) };
    }
    out
}

fn wallpaper(area: Area) -> Vec<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETDESKWALLPAPER};
    let mut buf = [0u16; 520];
    let ok = unsafe { SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, buf.as_mut_ptr() as _, 0) } != 0;
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    let img = (ok && len > 0).then(|| image::open(String::from_utf16_lossy(&buf[..len])).ok().map(|i| i.to_rgb8())).flatten();
    let (w, h) = (area.2.max(1), area.3.max(1));
    let Some(img) = img else {
        return vec![0xFF20_2024; w * h];
    };
    let (sw, sh) = (img.width() as f32, img.height() as f32);
    let s = (w as f32 / sw).max(h as f32 / sh);
    let (ox, oy) = ((sw * s - w as f32) / 2.0, (sh * s - h as f32) / 2.0);
    (0..w * h)
        .map(|i| {
            let sx = ((((i % w) as f32 + ox) / s) as u32).min(img.width() - 1);
            let sy = ((((i / w) as f32 + oy) / s) as u32).min(img.height() - 1);
            let p = img.get_pixel(sx, sy).0;
            0xFF00_0000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32
        })
        .collect()
}

fn monitor_area(cx: i32, cy: i32) -> Area {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    unsafe {
        let m = MonitorFromPoint(POINT { x: cx, y: cy }, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(m, &mut mi);
        let r = mi.rcMonitor;
        (r.left, r.top, (r.right - r.left).max(1) as usize, (r.bottom - r.top).max(1) as usize)
    }
}

fn blit(dst: &mut [u32], da: Area, src: &[u32], sa: Area) {
    let x0 = da.0.max(sa.0);
    let y0 = da.1.max(sa.1);
    let x1 = (da.0 + da.2 as i32).min(sa.0 + sa.2 as i32);
    let y1 = (da.1 + da.3 as i32).min(sa.1 + sa.3 as i32);
    if x0 >= x1 || y0 >= y1 || src.len() < sa.2 * sa.3 {
        return;
    }
    let n = (x1 - x0) as usize;
    for y in y0..y1 {
        let d = (y - da.1) as usize * da.2 + (x0 - da.0) as usize;
        let s = (y - sa.1) as usize * sa.2 + (x0 - sa.0) as usize;
        dst[d..d + n].copy_from_slice(&src[s..s + n]);
    }
}

struct Behind {
    wgc: Option<Wgc>,
    px: Vec<u32>,
    area: Area,
}

#[derive(Default)]
struct Composer {
    windows: Vec<(isize, Behind)>,
    wall: Option<(Area, Vec<u32>)>,
}

impl Composer {
    fn frame(&mut self, own: isize, want: Area, out: &mut Vec<u32>) -> Option<Area> {
        let m = CAPTURE_MARGIN;
        let area = (want.0 - m, want.1 - m, want.2 + 2 * m as usize, want.3 + 2 * m as usize);
        let list = windows_behind(own, area);
        self.windows.retain(|(h, _)| list.iter().any(|(l, _)| l == h));
        let mut changed = false;
        for (h, bounds) in &list {
            if !self.windows.iter().any(|(w, _)| w == h) {
                self.windows.push((*h, Behind { wgc: Wgc::window(*h), px: Vec::new(), area: *bounds }));
                changed = true;
            }
            let b = &mut self.windows.iter_mut().find(|(w, _)| w == h).unwrap().1;
            if let Some(wgc) = b.wgc.as_mut() {
                wgc.area = *bounds;
                match wgc.grab(want, &mut b.px) {
                    Ok(Some(a)) => {
                        b.area = a;
                        changed = true;
                    }
                    Ok(None) => {}
                    Err(()) => b.wgc = None,
                }
            }
        }
        if !changed && out.len() == area.2 * area.3 {
            return None;
        }
        let mon = monitor_area(want.0 + want.2 as i32 / 2, want.1 + want.3 as i32 / 2);
        if self.wall.as_ref().map_or(true, |(a, _)| *a != mon) {
            self.wall = Some((mon, wallpaper(mon)));
        }
        out.clear();
        out.resize(area.2 * area.3, 0xFF20_2024);
        if let Some((wa, wp)) = &self.wall {
            blit(out, area, wp, *wa);
        }
        for (h, _) in list.iter().rev() {
            if let Some((_, b)) = self.windows.iter().find(|(w, _)| w == h) {
                blit(out, area, &b.px, b.area);
            }
        }
        Some(area)
    }
}

#[derive(Default)]
struct Shooter {
    want: std::sync::Mutex<Area>,
    got: std::sync::Mutex<Option<Shot>>,
    spare: std::sync::Mutex<Option<Vec<u32>>>,
    stop: std::sync::atomic::AtomicBool,
    own: std::sync::atomic::AtomicIsize,
    windows: std::sync::atomic::AtomicBool,
    touched: std::sync::atomic::AtomicU64,
}

fn shot_sig(px: &[u32], area: Area, want: Area) -> u64 {
    let m = 24;
    let x0 = (want.0 - m - area.0).clamp(0, area.2 as i32) as usize;
    let x1 = (want.0 + want.2 as i32 + m - area.0).clamp(0, area.2 as i32) as usize;
    let y0 = (want.1 - m - area.1).clamp(0, area.3 as i32) as usize;
    let y1 = (want.1 + want.3 as i32 + m - area.1).clamp(0, area.3 as i32) as usize;
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for v in [area.0 as u64, area.1 as u64, area.2 as u64, area.3 as u64] {
        h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
    }
    for y in y0..y1 {
        for &v in &px[y * area.2 + x0..y * area.2 + x1] {
            h = (h ^ v as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Shooter {
    fn start() -> std::sync::Arc<Self> {
        let me = std::sync::Arc::new(Self::default());
        let worker = me.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
            }
            let mut dup: Option<Wgc> = None;
            let mut composer: Option<Composer> = None;
            let mut px = Vec::new();
            let relaxed = std::sync::atomic::Ordering::Relaxed;
            let mut next = std::time::Instant::now();
            let mut last_sig = 0u64;
            while !worker.stop.load(relaxed) {
                let (x, y, w, h) = *worker.want.lock().unwrap();
                if now_ms().saturating_sub(worker.touched.load(relaxed)) > 300 {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    continue;
                }
                let wait = next.saturating_duration_since(std::time::Instant::now());
                if !wait.is_zero() {
                    std::thread::sleep(wait);
                }
                if w == 0 || h == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    continue;
                }
                if worker.windows.load(relaxed) {
                    dup = None;
                    let c = composer.get_or_insert_with(Composer::default);
                    if let Some(area) = c.frame(worker.own.load(relaxed), (x, y, w, h), &mut px) {
                        let spare = worker.spare.lock().unwrap().take().unwrap_or_default();
                        let full = std::mem::replace(&mut px, spare);
                        if let Some(old) = worker.got.lock().unwrap().replace(Shot { area, px: full }) {
                            *worker.spare.lock().unwrap() = Some(old.px);
                        }
                        px.clear();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1000 / FPS));
                    continue;
                }
                composer = None;
                let (cx, cy) = (x + w as i32 / 2, y + h as i32 / 2);
                let inside = |a: Area| cx >= a.0 && cy >= a.1 && cx < a.0 + a.2 as i32 && cy < a.1 + a.3 as i32;
                if !dup.as_ref().is_some_and(|d| inside(d.area)) {
                    drop(dup.take());
                    dup = Wgc::open(cx, cy);
                    px.clear();
                    if dup.is_none() {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }
                }
                let d = dup.as_mut().unwrap();
                match d.grab((x, y, w, h), &mut px) {
                    Ok(Some(area)) => {
                        next = std::time::Instant::now() + std::time::Duration::from_micros(1_000_000 / FPS);
                        let sig = shot_sig(&px, area, (x, y, w, h));
                        if sig == last_sig {
                            continue;
                        }
                        last_sig = sig;
                        let spare = worker.spare.lock().unwrap().take().unwrap_or_default();
                        let full = std::mem::replace(&mut px, spare);
                        if let Some(old) = worker.got.lock().unwrap().replace(Shot { area, px: full }) {
                            *worker.spare.lock().unwrap() = Some(old.px);
                        }
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(4)),
                    Err(()) => {
                        dup = None;
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        });
        me
    }
}

#[derive(Default)]
pub struct Backdrop {
    glass: Glass,
    shooter: Option<std::sync::Arc<Shooter>>,
    shot: Option<Shot>,
    gen: u64,
    out: Vec<u32>,
    prev: Vec<u32>,
    out_gen: u64,
    layout: u64,
    painted_gen: u64,
    samples: Vec<usize>,
    sampled: Vec<u32>,
    sampled_area: Area,
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        if let Some(s) = &self.shooter {
            s.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl Backdrop {
    pub fn key(&self) -> u64 {
        self.out_gen
    }

    pub fn out(&self) -> &[u32] {
        &self.out
    }

    pub fn frame(&mut self, hwnd: isize, w: usize, h: usize, ppp: f32, behind: bool, rects: &[(egui::Rect, f32, egui::Rect)]) -> &[u32] {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        let mut p = POINT { x: 0, y: 0 };
        unsafe { ClientToScreen(hwnd as _, &mut p) };
        let shooter = self.shooter.get_or_insert_with(Shooter::start);
        shooter.own.store(hwnd, std::sync::atomic::Ordering::Relaxed);
        shooter.touched.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
        shooter.windows.store(behind, std::sync::atomic::Ordering::Relaxed);
        *shooter.want.lock().unwrap() = (p.x, p.y, w, h);
        if let Some(shot) = shooter.got.lock().unwrap().take() {
            self.gen += 1;
            if let Some(old) = self.shot.replace(shot) {
                *shooter.spare.lock().unwrap() = Some(old.px);
            }
        }
        let shot = self.shot.get_or_insert_with(|| Shot { area: (p.x, p.y, 1, 1), px: vec![0xFF20_2024] });
        let (sx0, sy0, sw, sh) = shot.area;
        let mut layout: u64 = 0xcbf2_9ce4_8422_2325 ^ (w as u64) << 20 ^ h as u64 ^ (ppp.to_bits() as u64) << 40 ^ (p.x as u64) << 8 ^ (p.y as u64) << 32;
        for (r, rad, clip) in rects {
            for v in [r.min.x, r.min.y, r.max.x, r.max.y, *rad, clip.min.x, clip.min.y, clip.max.x, clip.max.y] {
                layout = (layout ^ v.to_bits() as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
        if layout == self.layout && self.out.len() == w * h {
            let same = self.gen == self.painted_gen
                || (shot.area == self.sampled_area && self.samples.iter().zip(&self.sampled).all(|(&i, &v)| shot.px.get(i) == Some(&v)));
            if same {
                self.painted_gen = self.gen;
                return &self.out;
            }
        }
        let relayout = layout != self.layout || shot.area != self.sampled_area;
        self.layout = layout;
        self.painted_gen = self.gen;
        let sample = Bg { px: &shot.px, x0: sx0 - p.x, y0: sy0 - p.y, w: sw, h: sh };
        let shapes = paint(&mut self.glass, &mut self.out, w, h, ppp, &sample, rects);
        if relayout {
            self.samples.clear();
            for (s, fx) in &shapes {
                self.glass.samples(*fx, s, w, h, &sample, &mut self.samples);
            }
            self.sampled_area = shot.area;
        }
        self.sampled.clear();
        self.sampled.extend(self.samples.iter().map(|&i| shot.px[i]));
        if self.out != self.prev {
            self.out_gen += 1;
            self.prev.clone_from(&self.out);
        }
        &self.out
    }
}

pub(crate) fn paint(glass: &mut Glass, out: &mut Vec<u32>, w: usize, h: usize, ppp: f32, sample: &Bg, rects: &[(egui::Rect, f32, egui::Rect)]) -> Vec<(Shape, f32)> {
    out.clear();
    out.resize(w * h, CLEAR_PX);
    let mut order: Vec<&(egui::Rect, f32, egui::Rect)> = rects.iter().collect();
    order.sort_by(|a, b| b.0.area().total_cmp(&a.0.area()));
    let k = ppp;
    let shapes: Vec<(Shape, f32)> = order
        .into_iter()
        .map(|(rect, rad, clip)| {
            let (x, y) = ((rect.min.x * k).round() as i32, (rect.min.y * k).round() as i32);
            let shape = Shape {
                x,
                y,
                w: ((rect.max.x * k).round() as i32 - x).max(0) as usize,
                h: ((rect.max.y * k).round() as i32 - y).max(0) as usize,
                r: rad * k,
                rim: RIM * k,
                cloud: false,
                clip: ((clip.min.x * k).floor() as i32, (clip.min.y * k).floor() as i32, (clip.max.x * k).ceil() as i32, (clip.max.y * k).ceil() as i32),
            };
            (shape, k * 0.15)
        })
        .collect();
    for (shape, fx) in &shapes {
        glass.prepare(*fx, shape);
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get() / 2).clamp(1, 4);
    let band = h.div_ceil(threads).max(1);
    let glass = &*glass;
    std::thread::scope(|sc| {
        for (i, chunk) in out.chunks_mut(band * w).enumerate() {
            let shapes = &shapes;
            sc.spawn(move || {
                for (shape, fx) in shapes {
                    glass.draw(chunk, (i * band) as i32, w, h, *fx, shape, sample);
                }
            });
        }
    });
    shapes
}

#[derive(Default)]
struct PetGlass {
    glass: Glass,
    shooter: Option<std::sync::Arc<Shooter>>,
    shot: Option<Shot>,
    origin: (i32, i32),
    hwnd: isize,
    used: bool,
    hidden: bool,
    behind: bool,
    idle: u32,
}

thread_local! {
    static PET: std::cell::RefCell<PetGlass> = std::cell::RefCell::new(PetGlass::default());
}

pub fn set_origin(x: i32, y: i32) {
    PET.with(|p| p.borrow_mut().origin = (x, y));
}

pub fn pet_window(hwnd: isize) {
    PET.with(|p| p.borrow_mut().hwnd = hwnd);
}

pub fn set_behind(on: bool) {
    PET.with(|p| p.borrow_mut().behind = on);
}

pub fn pet_frame_end() {
    PET.with(|p| {
        let p = &mut *p.borrow_mut();
        if p.hidden && !p.used && p.hwnd != 0 {
            exclude_from_capture(p.hwnd, false);
            p.hidden = false;
        }
        p.idle = if p.used { 0 } else { p.idle + 1 };
        if p.idle > 180 {
            if let Some(s) = p.shooter.take() {
                s.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        p.used = false;
    });
}

pub fn pet_panel(canvas: &mut crate::render::Canvas, x: i32, y: i32, w: usize, h: usize, r: f32, k: f32, rim: f32, cloud: bool) {
    PET.with(|p| {
        let p = &mut *p.borrow_mut();
        p.used = true;
        let (ox, oy) = p.origin;
        if p.hwnd != 0 && p.hidden == p.behind {
            exclude_from_capture(p.hwnd, !p.behind);
            p.hidden = !p.behind;
        }
        let shooter = p.shooter.get_or_insert_with(Shooter::start);
        shooter.own.store(p.hwnd, std::sync::atomic::Ordering::Relaxed);
        shooter.touched.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
        shooter.windows.store(p.behind, std::sync::atomic::Ordering::Relaxed);
        *shooter.want.lock().unwrap() = (ox + x, oy + y, w, h);
        if let Some(shot) = shooter.got.lock().unwrap().take() {
            if let Some(old) = p.shot.replace(shot) {
                *shooter.spare.lock().unwrap() = Some(old.px);
            }
        }
        let shot = p.shot.get_or_insert_with(|| Shot { area: (ox + x, oy + y, 1, 1), px: vec![0xFF20_2024] });
        let (sx0, sy0, sw, sh) = shot.area;
        let bg = Bg { px: &shot.px, x0: sx0 - ox, y0: sy0 - oy, w: sw, h: sh };
        let shape = Shape { x, y, w, h, r, rim, cloud, clip: (0, 0, canvas.w as i32, canvas.h as i32) };
        let (cw, ch) = (canvas.w, canvas.h);
        p.glass.apply(&mut canvas.px, cw, ch, k, &shape, &bg);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_only_reads_sampled_pixels() {
        let (w, h) = (300usize, 200usize);
        let mut bg: Vec<u32> = (0..w * h).map(|i| 0xFF00_0000 | (i as u32).wrapping_mul(2654435761) >> 8).collect();
        let rects = [
            (egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(200.0, 60.0)), 12.0, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0))),
            (egui::Rect::from_min_size(egui::pos2(40.0, 100.0), egui::vec2(220.0, 70.0)), 16.0, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0))),
        ];
        let mut g = Glass::default();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        let shapes = paint(&mut g, &mut a, w, h, 1.0, &Bg { px: &bg, x0: 0, y0: 0, w, h }, &rects);
        let mut samples = Vec::new();
        for (s, fx) in &shapes {
            g.samples(*fx, s, w, h, &Bg { px: &bg, x0: 0, y0: 0, w, h }, &mut samples);
        }
        assert!(!samples.is_empty());
        let used: std::collections::HashSet<usize> = samples.iter().copied().collect();
        for (i, p) in bg.iter_mut().enumerate() {
            if !used.contains(&i) {
                *p ^= 0x00FF_FFFF;
            }
        }
        paint(&mut g, &mut b, w, h, 1.0, &Bg { px: &bg, x0: 0, y0: 0, w, h }, &rects);
        assert!(a == b);
        let i = samples[samples.len() / 2];
        bg[i] ^= 0x0080_8080;
        paint(&mut g, &mut b, w, h, 1.0, &Bg { px: &bg, x0: 0, y0: 0, w, h }, &rects);
        assert!(a != b);
    }

    #[test]
    #[ignore]
    fn rim_sheet() {
        let (w, h) = (420usize, 140usize);
        let bg: Vec<u32> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let c = if (x / 14 + y / 14) % 2 == 0 { 0xFFE0E0E0 } else { 0xFF303848 };
                if (20..24).contains(&(y % 40)) { 0xFFD04030 } else { c }
            })
            .collect();
        let mut g = Glass::default();
        let shape = Shape { x: 40, y: 40, w: 340, h: 45, r: 10.0, rim: RIM, cloud: false, clip: (0, 0, w as i32, h as i32) };
        let src = Bg { px: &bg, x0: 0, y0: 0, w, h };
        let mut img = image::RgbaImage::new(w as u32 * 3, h as u32 * 3);
        let mut out = vec![CLEAR_PX; w * h];
        g.apply(&mut out, w, h, 0.15, &shape, &src);
        for (i, (&o, &b)) in out.iter().zip(&bg).enumerate() {
            let a = o >> 24;
            let ch = |sh: u32| (((o >> sh) & 255) + ((b >> sh) & 255) * (255 - a) / 255).min(255) as u8;
            let px = image::Rgba([ch(16), ch(8), ch(0), 255]);
            let (x, y) = ((i % w) as u32 * 3, (i / w) as u32 * 3);
            for dy in 0..3 {
                for dx in 0..3 {
                    img.put_pixel(x + dx, y + dy, px);
                }
            }
        }
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("rim_sheet.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn bench_scroll_frame() {
        let (w, h, k) = (1250usize, 875usize, 1.25f32);
        let bg: Vec<u32> = (0..w * h).map(|i| 0xFF00_0000 | (i as u32 * 2654435761) >> 8).collect();
        let mut out = bg.clone();
        let mut g = Glass::default();
        let shapes: Vec<Shape> = (0..24)
            .map(|i| Shape { x: 140, y: 40 + i * 52, w: 1080, h: 45, r: 12.5, rim: RIM, cloud: false, clip: (130, 30, 1240, 870) })
            .chain([Shape { x: 8, y: 8, w: 105, h: 860, r: 20.0, rim: RIM, cloud: false, clip: (0, 0, w as i32, h as i32) }])
            .collect();
        let run = |g: &mut Glass, out: &mut Vec<u32>| {
            out.copy_from_slice(&bg);
            for s in &shapes {
                g.prepare(k * 0.15, s);
            }
            let sample = Bg { px: &bg, x0: 0, y0: 0, w, h };
            let band = h.div_ceil(8);
            let g = &*g;
            std::thread::scope(|sc| {
                for (i, chunk) in out.chunks_mut(band * w).enumerate() {
                    let (shapes, sample) = (&shapes, &sample);
                    sc.spawn(move || {
                        for s in shapes {
                            g.draw(chunk, (i * band) as i32, w, h, k * 0.15, s, sample);
                        }
                    });
                }
            });
        };
        run(&mut g, &mut out);
        let t = std::time::Instant::now();
        for _ in 0..20 {
            run(&mut g, &mut out);
        }
        println!("glass frame: {:.2} ms", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
    }


    #[test]
    fn refraction_bends_only_the_rim() {
        let m = cells(200, 120, 40.0, 1.0, f32::MAX);
        let at = |x: usize, y: usize| &m[y * 200 + x];
        assert_eq!((at(100, 60).dx, at(100, 60).dy), (0.0, 0.0));
        assert!(at(1, 60).dx > 1.0, "left rim pulls from inside");
        assert!(at(100, 1).dy > 1.0, "top rim pulls from inside");
        assert_eq!(at(0, 0).op, 0.0, "corner outside the rounded shape");
    }
}
