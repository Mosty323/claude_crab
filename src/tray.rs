use crate::pet::Pet;
use crate::sprite::Core;

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::art;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use tray_icon::menu::{
        CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem,
    };
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    #[derive(Clone, PartialEq)]
    struct State {
        en: bool,
        sleeping: bool,
        paused: bool,
        hat: bool,
        item: bool,
        lasso: bool,
        click: bool,
        debug: bool,
        icon: std::sync::Arc<Vec<u8>>,
    }

    pub struct Tray {
        tx: Sender<State>,
        last: Option<State>,
        look: u64,
        icon: std::sync::Arc<Vec<u8>>,
    }

    fn tray_thread(rx: Receiver<State>) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, SetTimer, TranslateMessage, MSG,
        };

        let Ok(icon) = Icon::from_rgba(art::logo_rgba(32), 32, 32) else {
            return;
        };
        let open = MenuItem::with_id("open", "Studio", true, None);
        let talk = MenuItem::with_id("talk", "...", true, None);
        let rodeo = MenuItem::with_id("rodeo", "...", true, None);
        let trick = MenuItem::with_id("trick", "...", true, None);
        let sleep = MenuItem::with_id("sleep", "...", true, None);
        let pause = MenuItem::with_id("pause", "...", true, None);
        let hat = MenuItem::with_id("hat", "...", true, None);
        let click = CheckMenuItem::with_id("click", "...", true, false, None);
        let debug = CheckMenuItem::with_id("debug", "...", true, false, None);
        let sep = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id("quit", "...", true, None);
        let menu = Menu::new();
        let items: [&dyn IsMenuItem; 11] = [
            &open, &talk, &rodeo, &trick, &sleep, &pause, &hat, &click, &debug, &sep, &quit,
        ];
        if menu.append_items(&items).is_err() {
            return;
        }
        let Ok(tray) = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("Claude Crab")
            .with_icon(icon)
            .build()
        else {
            return;
        };

        let apply = |s: &State| {
            let t = |de: &str, e: &str| if s.en { e.to_string() } else { de.to_string() };
            open.set_text(t("Studio öffnen", "Open Studio"));
            talk.set_text(t("Etwas sagen", "Say something"));
            rodeo.set_text(t("Lasso-Rodeo", "Lasso rodeo"));
            rodeo.set_enabled(s.lasso);
            trick.set_text(t("Item-Trick", "Item trick"));
            trick.set_enabled(s.item);
            sleep.set_text(if s.sleeping {
                t("Aufwachen", "Wake up")
            } else {
                t("Schlafen", "Sleep")
            });
            pause.set_text(if s.paused {
                t("Fortsetzen", "Resume")
            } else {
                t("Pausieren", "Pause")
            });
            hat.set_text(if s.hat {
                t("Hut abnehmen", "Take off hat")
            } else {
                t("Hut aufsetzen", "Put on hat")
            });
            click.set_text(t("Klicks durchlassen", "Click-through"));
            click.set_checked(s.click);
            debug.set_text(t("Debug-Konsole", "Debug console"));
            debug.set_checked(s.debug);
            quit.set_text(t("Beenden", "Quit"));
            if s.icon.len() == 64 * 64 * 4 {
                if let Ok(i) = Icon::from_rgba((*s.icon).clone(), 64, 64) {
                    let _ = tray.set_icon(Some(i));
                }
            }
        };

        unsafe {
            SetTimer(std::ptr::null_mut(), 0, 100, None);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
                while let Ok(s) = rx.try_recv() {
                    apply(&s);
                }
            }
        }
    }

    impl Tray {
        pub fn new() -> Option<Self> {
            let (tx, rx) = channel::<State>();
            std::thread::Builder::new()
                .name("tray".into())
                .spawn(move || tray_thread(rx))
                .ok()?;
            Some(Self { tx, last: None, look: 0, icon: std::sync::Arc::new(Vec::new()) })
        }

        pub fn poll(&mut self, core: &mut Core, pet: &mut Pet, open_studio: &mut bool) {
            let look = crate::render::crab_look_key(core);
            if look != self.look {
                self.look = look;
                self.icon = std::sync::Arc::new(crate::render::crab_icon(core, 64));
                crate::render::save_desktop_icon(core);
            }
            let st = State {
                en: core.settings.lang == 1,
                sleeping: pet.is_sleeping(),
                paused: pet.is_paused(),
                hat: core.settings.hat_visible,
                item: core.buddy_sprite().is_none()
                    && core.settings.item_visible
                    && core.item_sprite().is_some(),
                lasso: core.settings.lasso,
                click: core.settings.click_through,
                debug: crate::trace::enabled(),
                icon: self.icon.clone(),
            };
            if self.last.as_ref() != Some(&st) {
                self.last = Some(st.clone());
                let _ = self.tx.send(st);
            }

            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if !matches!(ev, TrayIconEvent::Move { .. }) {
                    dlog!("tray event: {:?}", ev);
                }
                match ev {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                    | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => *open_studio = true,
                    _ => {}
                }
            }

            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                dlog!("tray menu event: {:?}", ev.id);
                match ev.id.0.as_str() {
                    "open" => *open_studio = true,
                    "talk" => pet.speak_random(core),
                    "rodeo" => pet.trigger_cowboy(core),
                    "trick" => {
                        pet.trigger_play(core);
                    }
                    "sleep" => pet.toggle_sleep(core),
                    "pause" => pet.toggle_pause(),
                    "hat" => core.settings.hat_visible = !core.settings.hat_visible,
                    "click" => core.settings.click_through = !core.settings.click_through,
                    "debug" => {
                        let on = !crate::trace::enabled();
                        crate::trace::set_debug(on);
                        core.settings.debug = on;
                    }
                    "quit" => pet.quit = true,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub struct Tray;

    impl Tray {
        pub fn new() -> Option<Self> {
            None
        }

        pub fn poll(&mut self, _core: &mut Core, _pet: &mut Pet, _open_studio: &mut bool) {}
    }
}

pub use imp::Tray;
