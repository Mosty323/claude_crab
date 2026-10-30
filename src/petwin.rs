use crate::render::Canvas;
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP,
    HDC,
};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const CLASS: &str = "ClaudeCrabPet";
pub const WM_QUERY: u32 = WM_APP + 1;

pub use crate::pet::Input;

#[derive(Default)]
struct State {
    down: Option<POINT>,
    drag: bool,
    after_double: bool,
    input: Input,
    commands: Vec<String>,
}

thread_local! {
    static ST: RefCell<State> = RefCell::new(State::default());
}

static FLAGS: AtomicIsize = AtomicIsize::new(0);

pub fn set_flags(v: isize) {
    FLAGS.store(v, Ordering::Relaxed);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    p
}

unsafe extern "system" fn wnd_proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            SetCapture(h);
            ST.with(|s| {
                let mut s = s.borrow_mut();
                s.down = Some(cursor());
                s.drag = false;
                s.input.any = true;
                if msg == WM_LBUTTONDBLCLK {
                    s.input.double_clicked = true;
                    s.after_double = true;
                }
            });
            0
        }
        WM_MOUSEMOVE => {
            ST.with(|s| {
                let mut s = s.borrow_mut();
                if let (Some(d), false) = (s.down, s.drag) {
                    let p = cursor();
                    if (p.x - d.x).abs() > 3 || (p.y - d.y).abs() > 3 {
                        s.drag = true;
                        s.input.drag_started = true;
                    }
                }
                if s.drag {
                    s.input.any = true;
                }
            });
            0
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED => {
            if msg == WM_LBUTTONUP {
                ReleaseCapture();
            }
            ST.with(|s| {
                let mut s = s.borrow_mut();
                if s.down.take().is_some() {
                    if s.drag {
                        s.input.drag_stopped = true;
                    } else if msg == WM_LBUTTONUP && !s.after_double {
                        s.input.clicked = true;
                    }
                    s.after_double = false;
                    s.input.any = true;
                }
                s.drag = false;
            });
            0
        }
        WM_RBUTTONUP => {
            ST.with(|s| {
                let mut s = s.borrow_mut();
                s.input.secondary_clicked = true;
                s.input.any = true;
            });
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_COPYDATA => {
            let cds = &*(l as *const COPYDATASTRUCT);
            if !cds.lpData.is_null() && cds.cbData < 4096 {
                let bytes = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize);
                let text = String::from_utf8_lossy(bytes).into_owned();
                ST.with(|s| {
                    let mut s = s.borrow_mut();
                    s.commands.push(text);
                    s.input.any = true;
                });
            }
            1
        }
        WM_QUERY => FLAGS.load(Ordering::Relaxed),
        WM_CLOSE => {
            ST.with(|s| s.borrow_mut().commands.push("quit".into()));
            0
        }
        _ => DefWindowProcW(h, msg, w, l),
    }
}

pub fn take_input() -> Input {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let mut i = std::mem::take(&mut s.input);
        i.dragging = s.drag;
        i
    })
}

pub fn take_commands() -> Vec<String> {
    ST.with(|s| std::mem::take(&mut s.borrow_mut().commands))
}

pub fn create() -> Option<HWND> {
    let class = wide(CLASS);
    let title = wide("Claude Crab");
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        let h = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            inst,
            std::ptr::null(),
        );
        if h.is_null() {
            return None;
        }
        Some(h)
    }
}

pub fn show(h: HWND) {
    unsafe {
        ShowWindow(h, SW_SHOWNOACTIVATE);
    }
}

pub fn set_topmost(h: HWND, on: bool) {
    unsafe {
        SetWindowPos(
            h,
            if on { HWND_TOPMOST } else { HWND_NOTOPMOST },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub struct Presenter {
    dc: HDC,
    bmp: HBITMAP,
    old: *mut core::ffi::c_void,
    bits: *mut u32,
    w: usize,
    h: usize,
    last: u64,
}

impl Presenter {
    pub fn new() -> Self {
        Self {
            dc: std::ptr::null_mut(),
            bmp: std::ptr::null_mut(),
            old: std::ptr::null_mut(),
            bits: std::ptr::null_mut(),
            w: 0,
            h: 0,
            last: 0,
        }
    }

    fn release(&mut self) {
        unsafe {
            if !self.dc.is_null() {
                SelectObject(self.dc, self.old);
                DeleteObject(self.bmp);
                DeleteDC(self.dc);
            }
        }
        self.dc = std::ptr::null_mut();
        self.bits = std::ptr::null_mut();
    }

    fn ensure(&mut self, w: usize, h: usize) -> bool {
        if !self.dc.is_null() && self.w == w && self.h == h {
            return true;
        }
        self.release();
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            ReleaseDC(std::ptr::null_mut(), screen);
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w as i32;
            bi.bmiHeader.biHeight = -(h as i32);
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if bmp.is_null() || bits.is_null() {
                DeleteDC(dc);
                return false;
            }
            self.old = SelectObject(dc, bmp);
            self.dc = dc;
            self.bmp = bmp;
            self.bits = bits as *mut u32;
            self.w = w;
            self.h = h;
            self.last = 0;
        }
        true
    }

    pub fn present(&mut self, hwnd: HWND, canvas: &Canvas, x: i32, y: i32) -> bool {
        if canvas.w == 0 || canvas.h == 0 || !self.ensure(canvas.w, canvas.h) {
            return false;
        }
        let mut sig: u64 = 0xcbf2_9ce4_8422_2325 ^ ((x as u64) << 32 | y as u32 as u64);
        for &p in &canvas.px {
            sig = (sig ^ p as u64).wrapping_mul(0x0100_0000_01b3);
        }
        if sig == self.last {
            return false;
        }
        self.last = sig;
        unsafe {
            std::ptr::copy_nonoverlapping(canvas.px.as_ptr(), self.bits, canvas.px.len());
            let screen = GetDC(std::ptr::null_mut());
            let pos = POINT { x, y };
            let size = SIZE {
                cx: canvas.w as i32,
                cy: canvas.h as i32,
            };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(hwnd, screen, &pos, &size, self.dc, &src, 0, &blend, ULW_ALPHA);
            ReleaseDC(std::ptr::null_mut(), screen);
        }
        true
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        self.release();
    }
}

pub fn find() -> Option<HWND> {
    let class = wide(CLASS);
    let h = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    (!h.is_null()).then_some(h)
}

pub fn send(cmd: &str) -> bool {
    let Some(h) = find() else {
        return false;
    };
    let cds = COPYDATASTRUCT {
        dwData: 1,
        cbData: cmd.len() as u32,
        lpData: cmd.as_ptr() as *mut core::ffi::c_void,
    };
    let mut res: usize = 0;
    unsafe {
        SendMessageTimeoutW(
            h,
            WM_COPYDATA,
            0,
            &cds as *const COPYDATASTRUCT as LPARAM,
            SMTO_ABORTIFHUNG,
            300,
            &mut res,
        ) != 0
    }
}

pub fn query() -> Option<isize> {
    let h = find()?;
    let mut res: usize = 0;
    let ok = unsafe { SendMessageTimeoutW(h, WM_QUERY, 0, 0, SMTO_ABORTIFHUNG, 100, &mut res) };
    (ok != 0).then_some(res as isize)
}
