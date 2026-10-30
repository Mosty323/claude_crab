use egui::Pos2;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, SetProcessInformation, SetProcessWorkingSetSize,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetCursorPos, GetWindowLongPtrW, IsIconic, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_RESTORE, SW_SHOW, WS_EX_TRANSPARENT,
};

static HWND_CACHE: AtomicIsize = AtomicIsize::new(0);

fn hwnd() -> Option<*mut c_void> {
    let h = HWND_CACHE.load(Ordering::Relaxed);
    (h != 0).then_some(h as *mut c_void)
}

pub fn set_hwnd(v: isize) {
    HWND_CACHE.store(v, Ordering::Relaxed);
}

pub fn ex_style() -> usize {
    match hwnd() {
        Some(h) => unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) as usize },
        None => 0,
    }
}

pub fn disable_throttling() {
    #[repr(C)]
    struct PowerThrottling {
        version: u32,
        control_mask: u32,
        state_mask: u32,
    }
    let state = PowerThrottling {
        version: 1,
        control_mask: 1 | 4,
        state_mask: 0,
    };
    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            4,
            &state as *const PowerThrottling as *const c_void,
            std::mem::size_of::<PowerThrottling>() as u32,
        );
    }
}

pub fn trim_memory() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

pub fn cursor_screen() -> Option<Pos2> {
    let mut p = POINT { x: 0, y: 0 };
    let ok = unsafe { GetCursorPos(&mut p) };
    (ok != 0).then(|| Pos2::new(p.x as f32, p.y as f32))
}

pub fn ctrl_down() -> bool {
    unsafe { GetAsyncKeyState(VK_CONTROL as i32) < 0 }
}

static CLICK_THROUGH: AtomicU8 = AtomicU8::new(0);

pub fn set_click_through(enabled: bool, ctrl: bool) {
    let want = (enabled && !ctrl) as u8;
    let Some(h) = hwnd() else {
        return;
    };
    if CLICK_THROUGH.swap(want, Ordering::Relaxed) == want {
        return;
    }
    unsafe {
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let n = if want == 1 {
            ex | WS_EX_TRANSPARENT as isize
        } else {
            ex & !(WS_EX_TRANSPARENT as isize)
        };
        SetWindowLongPtrW(h, GWL_EXSTYLE, n);
    }
    dlog!("click-through {}", want == 1);
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if crate::shared::set_autostart(&exe, on) {
        Ok(())
    } else {
        Err("Registry".to_string())
    }
}

pub fn autostart_enabled() -> Option<bool> {
    Some(crate::shared::autostart_enabled())
}

pub fn export_dir() -> std::path::PathBuf {
    if let Some(p) = crate::shared::pictures_dir() {
        let dir = p.join("Claude Crab");
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    crate::sprite::data_path("")
}

pub fn work_area() -> Option<(f32, f32, f32, f32)> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    let h = hwnd()?;
    unsafe {
        let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(m, &mut mi) == 0 {
            return None;
        }
        let r = mi.rcWork;
        Some((r.left as f32, r.top as f32, r.right as f32, r.bottom as f32))
    }
}

pub fn dpi_scale() -> f32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    match hwnd() {
        Some(h) => {
            let dpi = unsafe { GetDpiForWindow(h) };
            if dpi == 0 {
                1.0
            } else {
                dpi as f32 / 96.0
            }
        }
        None => 1.0,
    }
}

pub fn bring_to_front(title: &str) -> bool {
    let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = FindWindowW(std::ptr::null(), t.as_ptr());
        if h.is_null() {
            return false;
        }
        ShowWindow(h, if IsIconic(h) != 0 { SW_RESTORE } else { SW_SHOW });
        SetForegroundWindow(h);
    }
    true
}
