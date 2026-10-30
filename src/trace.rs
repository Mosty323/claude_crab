use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

macro_rules! dlog {
    ($($a:tt)*) => {
        if $crate::trace::enabled() {
            $crate::trace::line(format!($($a)*));
        }
    };
}

enum Msg {
    Line(String),
    Console(bool),
}

static START: OnceLock<Instant> = OnceLock::new();
static TX: OnceLock<SyncSender<Msg>> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(false);
static LAST_BEAT: AtomicU64 = AtomicU64::new(0);
static FRAMES: AtomicU64 = AtomicU64::new(0);
static MAX_US: AtomicU64 = AtomicU64::new(0);
static NEXT_HB: AtomicU64 = AtomicU64::new(0);

pub fn now_ms() -> u64 {
    START
        .get()
        .map(|s| s.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn line(s: String) {
    let Some(tx) = TX.get() else {
        return;
    };
    let t = std::thread::current();
    let msg = format!(
        "[{:>9.3}] [{}] {}",
        now_ms() as f64 / 1000.0,
        t.name().unwrap_or("?"),
        s
    );
    let _ = tx.try_send(Msg::Line(msg));
}

pub fn beat() {
    LAST_BEAT.store(now_ms().max(1), Ordering::Relaxed);
}

pub fn frame_end(dt: Duration, extra: impl FnOnce() -> String) {
    beat();
    if !enabled() {
        return;
    }
    FRAMES.fetch_add(1, Ordering::Relaxed);
    let us = dt.as_micros() as u64;
    MAX_US.fetch_max(us, Ordering::Relaxed);
    if us > 40_000 {
        dlog!("SLOW update: {:.1} ms", us as f64 / 1000.0);
    }
    let now = now_ms();
    if now >= NEXT_HB.load(Ordering::Relaxed) {
        NEXT_HB.store(now + 2000, Ordering::Relaxed);
        let f = FRAMES.swap(0, Ordering::Relaxed);
        let m = MAX_US.swap(0, Ordering::Relaxed);
        dlog!(
            "alive: {} frames ({:.0} fps), max update {:.1} ms | {}",
            f,
            f as f64 / 2.0,
            m as f64 / 1000.0,
            extra()
        );
    }
}

pub fn set_debug(on: bool) {
    let Some(tx) = TX.get() else {
        return;
    };
    if on {
        ENABLED.store(true, Ordering::Relaxed);
        let _ = tx.try_send(Msg::Console(true));
        dlog!("debug logging enabled");
    } else {
        dlog!("debug logging disabled");
        ENABLED.store(false, Ordering::Relaxed);
        let _ = tx.try_send(Msg::Console(false));
    }
}

pub fn init(enable: bool) {
    let _ = START.set(Instant::now());
    let (tx, rx) = sync_channel::<Msg>(8192);
    let _ = TX.set(tx);
    let _ = std::thread::Builder::new()
        .name("log-writer".into())
        .spawn(move || writer(rx));
    let _ = std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(watchdog);
    if enable {
        set_debug(true);
    }
}

fn writer(rx: Receiver<Msg>) {
    let mut file: Option<File> = None;
    let mut console: Option<File> = None;
    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Console(true) => {
                if console.is_none() {
                    console = open_console();
                }
                if file.is_none() {
                    file = File::create(crate::sprite::data_path("crab_debug.log")).ok();
                }
            }
            Msg::Console(false) => {
                console = None;
                file = None;
                close_console();
            }
            Msg::Line(l) => {
                if let Some(f) = file.as_mut() {
                    let _ = writeln!(f, "{l}");
                }
                if let Some(c) = console.as_mut() {
                    let _ = writeln!(c, "{l}");
                }
            }
        }
    }
}

fn watchdog() {
    let mut frozen_since: Option<u64> = None;
    let mut last_report = 0u64;
    loop {
        std::thread::sleep(Duration::from_millis(250));
        if !enabled() {
            frozen_since = None;
            continue;
        }
        let beat = LAST_BEAT.load(Ordering::Relaxed);
        if beat == 0 {
            continue;
        }
        let now = now_ms();
        let gap = now.saturating_sub(beat);
        if gap > 700 {
            if frozen_since.is_none() {
                frozen_since = Some(beat);
            }
            if now.saturating_sub(last_report) >= 1000 {
                last_report = now;
                dlog!("FREEZE: no frame for {} ms", gap);
            }
        } else if let Some(s) = frozen_since.take() {
            dlog!("RECOVERED: frames running again after {} ms", now.saturating_sub(s));
        }
    }
}

#[cfg(windows)]
fn open_console() -> Option<File> {
    use windows_sys::Win32::System::Console::{AllocConsole, GetConsoleWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DeleteMenu, GetSystemMenu};
    unsafe {
        AllocConsole();
        let h = GetConsoleWindow();
        if !h.is_null() {
            let m = GetSystemMenu(h, 0);
            if !m.is_null() {
                DeleteMenu(m, 0xF060, 0);
            }
        }
    }
    std::fs::OpenOptions::new().write(true).open("CONOUT$").ok()
}

#[cfg(not(windows))]
fn open_console() -> Option<File> {
    None
}

#[cfg(windows)]
fn close_console() {
    use windows_sys::Win32::System::Console::FreeConsole;
    unsafe {
        FreeConsole();
    }
}

#[cfg(not(windows))]
fn close_console() {}
