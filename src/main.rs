#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Claude Crab is a Windows app");

#[macro_use]
mod trace;
mod art;
mod claude;
mod glass;
mod pet;
mod physics;
mod petwin;
mod render;
mod shared;
mod soft;
mod sprite;
mod studio;
mod sys;
mod tray;
mod widgets;

use pet::Pet;
use render::Canvas;
use sprite::Core;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use studio::{PetRemote, Studio};
use tray::Tray;

pub const STUDIO_TITLE: &str = "Claude Crab Studio";

static SHOW_REQ: AtomicBool = AtomicBool::new(false);
static QUIT_REQ: AtomicBool = AtomicBool::new(false);

fn ipc_thread() {
    use windows_sys::Win32::System::Threading::{WaitForMultipleObjects, INFINITE};
    let show = shared::create_event(shared::SHOW_EVENT, false);
    let quit = shared::create_event(shared::QUIT_EVENT, true);
    if show.is_null() || quit.is_null() {
        return;
    }
    let handles = [show, quit];
    loop {
        let r = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
        match r {
            0 => SHOW_REQ.store(true, Ordering::Relaxed),
            1 => {
                QUIT_REQ.store(true, Ordering::Relaxed);
                return;
            }
            _ => return,
        }
    }
}

fn uninstall() {
    use shared::*;
    use std::os::windows::process::CommandExt;
    let de = std::fs::read_to_string(sprite::data_path("crab_config.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("lang").and_then(|l| l.as_u64()))
        == Some(0);
    let quiet = std::env::args().any(|a| a == "--silent");
    let t = |a: &'static str, b: &'static str| if de { a } else { b };
    if !quiet
        && !message_box(
            t(
                "Claude Crab wirklich deinstallieren?\nDeine Einstellungen und Sprites werden dabei gelöscht.",
                "Really uninstall Claude Crab?\nYour settings and sprites will be deleted.",
            ),
            true,
        )
    {
        return;
    }
    stop_running_instance(5000);
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent().map(|d| d.to_path_buf()) else {
        return;
    };
    reg_delete_value(RUN_KEY, RUN_VALUE);
    reg_delete_tree(UNINSTALL_KEY);
    for f in read_manifest(&dir) {
        if f.extension().map(|e| e.eq_ignore_ascii_case("lnk")).unwrap_or(false) {
            let _ = std::fs::remove_file(f);
        }
    }
    for name in [
        "claude_crab.ico",
        "LICENSE",
        "crab_config.json",
        "crab_library.json",
        "crab_error.log",
        "crab_debug.log",
        MANIFEST,
    ] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let _ = hidden_command("cmd")
        .current_dir(std::env::temp_dir())
        .raw_arg(format!(
            "/C ping -n 3 127.0.0.1 >nul & del /f /q \"{}\" & rmdir \"{}\"",
            exe.display(),
            dir.display()
        ))
        .spawn();
    if !quiet {
        message_box(t("Claude Crab wurde entfernt.", "Claude Crab has been removed."), false);
    }
}

struct StudioApp {
    core: Core,
    studio: Studio,
    pet: PetRemote,
    backdrop: glass::Backdrop,
    hidden: Option<bool>,
    rects: Vec<(egui::Rect, f32, egui::Rect)>,
}

impl soft::SoftApp for StudioApp {
    fn update(&mut self, ctx: &egui::Context) {
        self.studio.frame(ctx, &mut self.core, &mut self.pet);
    }

    fn on_exit(&mut self) {
        let _ = self.core.save_settings();
        let _ = self.core.save_library();
        self.pet.reload();
    }

    fn base(&mut self, hwnd: isize, w: usize, h: usize, ppp: f32, fresh: bool) -> Option<(&[u32], u64)> {
        if fresh {
            self.rects = widgets::take_glass_rects();
        }
        let glass = widgets::glass();
        let behind = self.core.settings.obs;
        let hide = glass && !behind;
        if self.hidden != Some(hide) {
            glass::exclude_from_capture(hwnd, hide);
            self.hidden = Some(hide);
        }
        if !glass {
            self.backdrop = glass::Backdrop::default();
            return None;
        }
        self.backdrop.frame(hwnd, w, h, ppp, behind, &self.rects);
        Some((self.backdrop.out(), self.backdrop.key()))
    }
}

fn run_studio() {
    if sys::bring_to_front(STUDIO_TITLE) {
        return;
    }
    let ctx = egui::Context::default();
    widgets::apply_style(&ctx);
    let core = Core::load(Some(&ctx));
    let studio = Studio::new(&ctx, &core);
    soft::run(
        soft::Options {
            title: STUDIO_TITLE.to_string(),
            size: [1000.0, 680.0],
            min_size: Some([880.0, 620.0]),
            resizable: true,
            icon: Some((art::logo_rgba(64), 64)),
            custom_frame: true,
        },
        ctx,
        StudioApp {
            core,
            studio,
            pet: PetRemote::new(),
            backdrop: glass::Backdrop::default(),
            hidden: None,
            rects: Vec::new(),
        },
    );
}
fn open_studio(last_spawn: &mut Option<Instant>) {
    if sys::bring_to_front(STUDIO_TITLE) {
        dlog!("studio already open, brought to front");
        return;
    }
    if last_spawn.map(|t| t.elapsed() < Duration::from_secs(3)).unwrap_or(false) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let ok = std::process::Command::new(exe).arg("--studio").spawn().is_ok();
        dlog!("studio started: {}", ok);
        *last_spawn = Some(Instant::now());
    }
}

fn run_pet() {
    use windows_sys::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, TranslateMessage, MSG,
        PM_REMOVE, QS_ALLINPUT, WM_QUIT,
    };
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    sys::disable_throttling();
    let Some(hwnd) = petwin::create() else {
        shared::message_box("Claude Crab: could not create the window.", false);
        return;
    };
    sys::set_hwnd(hwnd as isize);
    glass::pet_window(hwnd as isize);

    let mut core = Core::load(None);
    if core.settings.debug && !trace::enabled() {
        trace::set_debug(true);
    }
    let mut pet = Pet::new();
    let mut tray = Tray::new();
    let _ = std::thread::Builder::new().name("ipc".into()).spawn(ipc_thread);
    dlog!("pet started ({} sprites)", core.slots.len());

    let canvas = Rc::new(RefCell::new(Canvas::new(1, 1)));
    let mut presenter = petwin::Presenter::new();
    let start = Instant::now();
    let mut saved = core.settings.clone();
    let mut applied_top: Option<bool> = None;
    let mut shown = false;
    let mut last_spawn: Option<Instant> = None;
    let mut last_frame = 0.0f64;
    let mut next_trim = 5.0f64;
    let mut next_top = 0.0f64;

    'run: loop {
        let due = last_frame + pet.repaint_ms(false, &core) as f64 / 1000.0;
        let wait = ((due - start.elapsed().as_secs_f64()) * 1000.0).clamp(0.0, 1000.0) as u32;
        unsafe {
            MsgWaitForMultipleObjects(0, std::ptr::null(), 0, wait, QS_ALLINPUT);
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    break 'run;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let started = Instant::now();
        trace::beat();

        let input = petwin::take_input();
        for cmd in petwin::take_commands() {
            dlog!("command: {cmd}");
            match cmd.as_str() {
                "cowboy" => pet.trigger_cowboy(&core),
                "look" => pet.trigger_look_around(),
                "stretch" => pet.trigger_stretch(),
                "trick" => {
                    pet.trigger_play(&core);
                }
                "sleep" => pet.toggle_sleep(&core),
                "pause" => pet.toggle_pause(),
                "quit" => pet.quit = true,
                "reload" => {
                    core = Core::load(None);
                    saved = core.settings.clone();
                    if core.settings.debug != trace::enabled() {
                        trace::set_debug(core.settings.debug);
                    }
                }
                c => {
                    if let Some(text) = c.strip_prefix("say:") {
                        pet.set_speech(&core, text);
                    } else if let Some(ev) = c.strip_prefix("cc:") {
                        pet.claude_event(&core, ev);
                    }
                }
            }
        }

        let mut want_studio = false;
        if let Some(t) = tray.as_mut() {
            t.poll(&mut core, &mut pet, &mut want_studio);
        }
        if pet.take_studio() || SHOW_REQ.swap(false, Ordering::Relaxed) || want_studio {
            open_studio(&mut last_spawn);
        }
        if QUIT_REQ.load(Ordering::Relaxed) {
            pet.quit = true;
        }
        if pet.quit {
            break;
        }

        let now = start.elapsed().as_secs_f64();
        if now < due && !input.any {
            continue;
        }
        last_frame = now;

        pet.sync(now, sys::dpi_scale(), &core);
        sys::set_click_through(core.settings.click_through, sys::ctrl_down());
        glass::set_behind(core.settings.obs);
        render::apply_crab_colors(&core.settings);
        render::set_outfit(core.outfit_layers());
        if applied_top != Some(core.settings.on_top) || (core.settings.on_top && now > next_top) {
            petwin::set_topmost(hwnd, core.settings.on_top);
            applied_top = Some(core.settings.on_top);
            next_top = now + 5.0;
        }
        {
            let (w, h) = pet.canvas_size();
            let mut c = canvas.borrow_mut();
            c.resize(w, h);
            c.clear();
        }
        pet.frame(&input, &canvas, now, &core);
        glass::pet_frame_end();
        let (x, y) = pet.window_pos();
        presenter.present(hwnd, &canvas.borrow(), x, y);
        if !shown {
            petwin::show(hwnd);
            shown = true;
        }
        petwin::set_flags(pet.flags());

        if core.settings != saved {
            let _ = core.save_settings();
            saved = core.settings.clone();
        }
        if now > next_trim {
            sys::trim_memory();
            next_trim = now + 60.0;
        }

        let pet_state = if trace::enabled() {
            pet.debug_state()
        } else {
            String::new()
        };
        trace::frame_end(started.elapsed(), || {
            format!("{} ex_style={:#x}", pet_state, sys::ex_style())
        });
    }

    let _ = core.save_settings();
    shared::close_window(STUDIO_TITLE);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--uninstall") {
        uninstall();
        return;
    }
    if has("--hook") {
        claude::run_hook();
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--event") {
        if let Some(state) = args.get(i + 1) {
            petwin::send(&format!("cc:{state}:test"));
        }
        return;
    }
    if has("--studio") {
        trace::init(false);
        run_studio();
        return;
    }
    let Some(_instance) = shared::claim_instance() else {
        shared::signal(shared::SHOW_EVENT);
        return;
    };
    trace::init(has("--debug"));
    dlog!("starting, debug mode on");
    std::panic::set_hook(Box::new(|info| {
        trace::line(format!("PANIC: {info}"));
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(sprite::data_path("crab_error.log"))
        {
            let _ = writeln!(f, "[{secs}] {info}");
        }
    }));
    run_pet();
}
