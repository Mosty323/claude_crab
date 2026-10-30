#![allow(dead_code)]

use std::ffi::c_void;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_WRITE, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, OpenMutexW, SetEvent, EVENT_MODIFY_STATE,
    SYNCHRONIZATION_SYNCHRONIZE,
};

pub const APP_NAME: &str = "Claude Crab";
pub const APP_EXE: &str = "claude_crab.exe";
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "ClaudeCrab";
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ClaudeCrab";
pub const INSTANCE_MUTEX: &str = r"Local\ClaudeCrab_Instance";
pub const SHOW_EVENT: &str = r"Local\ClaudeCrab_Show";
pub const QUIT_EVENT: &str = r"Local\ClaudeCrab_Quit";
pub const MANIFEST: &str = "install.json";

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn system_is_german() -> bool {
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    id & 0x3ff == 0x07
}

pub fn reg_set_str(subkey: &str, name: &str, value: &str) -> bool {
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let sk = wide(subkey);
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sk.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        ) != 0
        {
            return false;
        }
        let n = wide(name);
        let v = wide(value);
        let ok = RegSetValueExW(
            key,
            n.as_ptr(),
            0,
            REG_SZ,
            v.as_ptr() as *const u8,
            (v.len() * 2) as u32,
        ) == 0;
        RegCloseKey(key);
        ok
    }
}

pub fn reg_set_dword(subkey: &str, name: &str, value: u32) -> bool {
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let sk = wide(subkey);
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            sk.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        ) != 0
        {
            return false;
        }
        let n = wide(name);
        let ok = RegSetValueExW(
            key,
            n.as_ptr(),
            0,
            REG_DWORD,
            &value as *const u32 as *const u8,
            4,
        ) == 0;
        RegCloseKey(key);
        ok
    }
}

pub fn reg_get_str(subkey: &str, name: &str) -> Option<String> {
    unsafe {
        let sk = wide(subkey);
        let n = wide(name);
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let r = RegGetValueW(
            HKEY_CURRENT_USER,
            sk.as_ptr(),
            n.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        );
        if r != 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

pub fn reg_delete_value(subkey: &str, name: &str) {
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let sk = wide(subkey);
        if RegOpenKeyExW(HKEY_CURRENT_USER, sk.as_ptr(), 0, KEY_SET_VALUE, &mut key) == 0 {
            let n = wide(name);
            RegDeleteValueW(key, n.as_ptr());
            RegCloseKey(key);
        }
    }
}

pub fn reg_delete_tree(subkey: &str) {
    let sk = wide(subkey);
    unsafe {
        RegDeleteTreeW(HKEY_CURRENT_USER, sk.as_ptr());
    }
}

pub fn autostart_enabled() -> bool {
    reg_get_str(RUN_KEY, RUN_VALUE).is_some()
}

pub fn set_autostart(exe: &std::path::Path, on: bool) -> bool {
    if on {
        reg_set_str(RUN_KEY, RUN_VALUE, &format!("\"{}\"", exe.display()))
    } else {
        reg_delete_value(RUN_KEY, RUN_VALUE);
        true
    }
}

pub fn claim_instance() -> Option<HANDLE> {
    let name = wide(INSTANCE_MUTEX);
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if h.is_null() {
            return Some(h);
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(h);
            return None;
        }
        Some(h)
    }
}

pub fn instance_running() -> bool {
    let name = wide(INSTANCE_MUTEX);
    unsafe {
        let h = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr());
        if h.is_null() {
            false
        } else {
            CloseHandle(h);
            true
        }
    }
}

pub fn create_event(name: &str, manual_reset: bool) -> HANDLE {
    let n = wide(name);
    unsafe { CreateEventW(std::ptr::null(), manual_reset as i32, 0, n.as_ptr()) }
}

pub fn signal(name: &str) -> bool {
    let n = wide(name);
    unsafe {
        let h = OpenEventW(EVENT_MODIFY_STATE, 0, n.as_ptr());
        if h.is_null() {
            return false;
        }
        SetEvent(h);
        CloseHandle(h);
        true
    }
}

pub fn close_window(title: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
    let t = wide(title);
    unsafe {
        let h = FindWindowW(std::ptr::null(), t.as_ptr());
        if !h.is_null() {
            PostMessageW(h, WM_CLOSE, 0, 0);
        }
    }
}

pub fn stop_running_instance(timeout_ms: u64) -> bool {
    close_window("Claude Crab Studio");
    if !instance_running() {
        return true;
    }
    signal(QUIT_EVENT);
    let start = std::time::Instant::now();
    while start.elapsed().as_millis() < timeout_ms as u128 {
        if !instance_running() {
            std::thread::sleep(std::time::Duration::from_millis(300));
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    !instance_running()
}

fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    unsafe {
        let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p);
        if hr != 0 || p.is_null() {
            return None;
        }
        let mut len = 0;
        while *p.add(len) != 0 {
            len += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        CoTaskMemFree(p as *const c_void);
        Some(PathBuf::from(s))
    }
}

pub fn desktop_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Desktop)
}

pub fn programs_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Programs)
}

pub fn pictures_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Pictures)
}

pub fn local_app_data() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData)
}

pub fn write_manifest(dir: &std::path::Path, files: &[PathBuf]) -> std::io::Result<()> {
    let mut s = String::new();
    for f in files {
        s.push_str(&f.display().to_string());
        s.push('\n');
    }
    std::fs::write(dir.join(MANIFEST), s)
}

pub fn read_manifest(dir: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_to_string(dir.join(MANIFEST))
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim().is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn message_box(text: &str, yes_no: bool) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
    };
    let t = wide(text);
    let c = wide(APP_NAME);
    let style = if yes_no {
        MB_YESNO | MB_ICONQUESTION
    } else {
        MB_OK | MB_ICONINFORMATION
    };
    unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), style) == IDYES }
}

pub fn hidden_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new(program);
    c.creation_flags(0x0800_0000);
    c
}
