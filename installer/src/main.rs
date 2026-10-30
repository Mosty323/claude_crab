#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(dead_code)]
#[path = "../../src/art.rs"]
mod art;
#[path = "../../src/shared.rs"]
mod shared;
#[path = "../../src/soft.rs"]
mod soft;
#[allow(dead_code)]
#[path = "../../src/widgets.rs"]
mod widgets;

use egui::{
    self, Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Rounding, Stroke, Vec2,
};
use shared::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use widgets::*;

const PAYLOAD: &[u8] = include_bytes!(env!("CRAB_PAYLOAD"));
const ICON: &[u8] = include_bytes!("../../assets/claude_crab.ico");
const LICENSE: &[u8] = include_bytes!("../../LICENSE");
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Default)]
struct Progress {
    step: String,
    frac: f32,
    done: bool,
    error: Option<String>,
    notes: Vec<String>,
}

#[derive(Clone)]
struct Options {
    dir: PathBuf,
    desktop: bool,
    autostart: bool,
    launch: bool,
    de: bool,
}

#[derive(PartialEq)]
enum Page {
    Welcome,
    Busy,
    Done,
    Failed,
}

struct Setup {
    de: bool,
    page: Page,
    dir: String,
    existing: Option<PathBuf>,
    desktop: bool,
    autostart: bool,
    launch: bool,
    progress: Arc<Mutex<Progress>>,
}

fn t(de: bool, a: &'static str, b: &'static str) -> &'static str {
    if de {
        a
    } else {
        b
    }
}

impl Setup {
    fn new() -> Self {
        let de = false;
        let existing = reg_get_str(UNINSTALL_KEY, "InstallLocation")
            .map(PathBuf::from)
            .filter(|p| p.join(APP_EXE).exists());
        let default_dir = existing.clone().unwrap_or_else(|| {
            local_app_data()
                .unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"))
                .join("Programs")
                .join(APP_NAME)
        });
        let autostart = if existing.is_some() {
            autostart_enabled()
        } else {
            true
        };
        Self {
            de,
            page: Page::Welcome,
            dir: default_dir.display().to_string(),
            existing,
            desktop: true,
            autostart,
            launch: true,
            progress: Arc::new(Mutex::new(Progress::default())),
        }
    }

    fn start(&mut self) {
        let opts = Options {
            dir: PathBuf::from(self.dir.trim()),
            desktop: self.desktop,
            autostart: self.autostart,
            launch: self.launch,
            de: self.de,
        };
        *self.progress.lock().unwrap() = Progress::default();
        self.page = Page::Busy;
        let progress = self.progress.clone();
        std::thread::spawn(move || {
            let result = install(&opts, &progress);
            let mut p = progress.lock().unwrap();
            match result {
                Ok(()) => {
                    p.frac = 1.0;
                    p.done = true;
                }
                Err(e) => p.error = Some(e),
            }
        });
    }
}

fn set_step(p: &Arc<Mutex<Progress>>, frac: f32, step: &str) {
    let mut g = p.lock().unwrap();
    g.frac = frac;
    g.step = step.to_string();
}

fn write_retry(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..10 {
        match std::fs::write(path, data) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    Err(last.unwrap())
}

fn make_shortcut(lnk: &Path, exe: &Path, dir: &Path, ico: &Path) -> bool {
    let script = "$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:CRAB_LNK);\
                  $s.TargetPath=$env:CRAB_EXE;$s.WorkingDirectory=$env:CRAB_DIR;\
                  $s.IconLocation=$env:CRAB_ICO;$s.Description='Claude Crab';$s.Save()";
    if let Some(parent) = lnk.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    hidden_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .env("CRAB_LNK", lnk)
        .env("CRAB_EXE", exe)
        .env("CRAB_DIR", dir)
        .env("CRAB_ICO", ico)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        && lnk.exists()
}

fn install(o: &Options, p: &Arc<Mutex<Progress>>) -> Result<(), String> {
    let de = o.de;
    if !o.dir.is_absolute() {
        return Err(t(de, "Bitte einen vollständigen Ordnerpfad angeben.", "Please enter a full folder path.").into());
    }

    set_step(p, 0.05, t(de, "Laufende Krabbe wird beendet ...", "Stopping the running crab ..."));
    if !stop_running_instance(5000) {
        let _ = hidden_command("taskkill").args(["/F", "/IM", APP_EXE]).status();
        std::thread::sleep(Duration::from_millis(600));
    }

    set_step(p, 0.25, t(de, "Dateien werden kopiert ...", "Copying files ..."));
    std::fs::create_dir_all(&o.dir).map_err(|e| {
        format!("{}: {e}", t(de, "Ordner konnte nicht angelegt werden", "Could not create folder"))
    })?;
    let exe = o.dir.join(APP_EXE);
    let ico = o.dir.join("claude_crab.ico");
    write_retry(&exe, PAYLOAD).map_err(|e| {
        format!("{}: {e}", t(de, "Programm konnte nicht geschrieben werden", "Could not write the program"))
    })?;
    let _ = std::fs::write(&ico, ICON);
    let _ = std::fs::write(o.dir.join("LICENSE"), LICENSE);
    let config = o.dir.join("crab_config.json");
    if o.de && !config.exists() {
        let _ = std::fs::write(&config, "{ \"lang\": 0 }");
    }

    set_step(p, 0.55, t(de, "Verknüpfungen werden erstellt ...", "Creating shortcuts ..."));
    let mut created = Vec::new();
    let mut wanted = Vec::new();
    if let Some(dir) = programs_dir() {
        wanted.push(dir.join(format!("{APP_NAME}.lnk")));
    }
    let desktop_lnk = desktop_dir().map(|d| d.join(format!("{APP_NAME}.lnk")));
    if let Some(l) = &desktop_lnk {
        if o.desktop {
            wanted.push(l.clone());
        } else {
            let _ = std::fs::remove_file(l);
        }
    }
    for lnk in wanted {
        if make_shortcut(&lnk, &exe, &o.dir, &ico) {
            created.push(lnk);
        } else {
            p.lock().unwrap().notes.push(format!(
                "{}: {}",
                t(de, "Verknüpfung fehlt", "Shortcut missing"),
                lnk.display()
            ));
        }
    }

    set_step(p, 0.8, t(de, "Windows wird eingerichtet ...", "Registering with Windows ..."));
    let size_kb = (PAYLOAD.len() + ICON.len() + LICENSE.len()) as u32 / 1024;
    let quoted = format!("\"{}\"", exe.display());
    let ok = reg_set_str(UNINSTALL_KEY, "DisplayName", APP_NAME)
        && reg_set_str(UNINSTALL_KEY, "DisplayVersion", VERSION)
        && reg_set_str(UNINSTALL_KEY, "Publisher", "Mosty323")
        && reg_set_str(UNINSTALL_KEY, "DisplayIcon", &ico.display().to_string())
        && reg_set_str(UNINSTALL_KEY, "InstallLocation", &o.dir.display().to_string())
        && reg_set_str(UNINSTALL_KEY, "UninstallString", &format!("{quoted} --uninstall"))
        && reg_set_dword(UNINSTALL_KEY, "NoModify", 1)
        && reg_set_dword(UNINSTALL_KEY, "NoRepair", 1)
        && reg_set_dword(UNINSTALL_KEY, "EstimatedSize", size_kb);
    if !ok {
        p.lock().unwrap().notes.push(
            t(de, "Eintrag unter Apps konnte nicht angelegt werden", "Could not add the Apps entry").into(),
        );
    }
    set_autostart(&exe, o.autostart);
    let _ = write_manifest(&o.dir, &created);

    if o.launch {
        set_step(p, 0.95, t(de, "Krabbe wird gestartet ...", "Starting the crab ..."));
        let _ = std::process::Command::new(&exe).current_dir(&o.dir).spawn();
    }
    set_step(p, 1.0, t(de, "Fertig", "Done"));
    Ok(())
}

fn hex(h: &str) -> Color32 {
    let v = u32::from_str_radix(h.trim_start_matches('#'), 16).unwrap_or(0);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn runs(p: &Painter, o: Pos2, cell: f32, rows: &[&str], color: impl Fn(char) -> Option<Color32>) {
    for (r, row) in rows.iter().enumerate() {
        let chars: Vec<char> = row.chars().collect();
        let mut c = 0;
        while c < chars.len() {
            let Some(col) = color(chars[c]) else {
                c += 1;
                continue;
            };
            let start = c;
            while c < chars.len() && color(chars[c]) == Some(col) {
                c += 1;
            }
            p.rect_filled(
                Rect::from_min_size(
                    o + Vec2::new(start as f32 * cell, r as f32 * cell),
                    Vec2::new((c - start) as f32 * cell, cell + 0.5),
                ),
                0.0,
                col,
            );
        }
    }
}

fn draw_def(p: &Painter, o: Pos2, blk: f32, name: &str) {
    let Some(d) = art::DEFS.iter().find(|d| d.name == name) else {
        return;
    };
    runs(p, o, blk, d.rows, |ch| d.colors.iter().find(|(k, _)| *k == ch).map(|(_, h)| hex(h)));
}

fn draw_crab(p: &Painter, o: Pos2, blk: f32, frame: &art::Frame) {
    runs(p, o, blk / 2.0, frame, |ch| match ch {
        '#' => Some(art::CRAB),
        'E' | 'O' => Some(art::EYE),
        '-' => Some(art::CLOSED_EYE),
        _ => None,
    });
}

fn stage(ui: &mut egui::Ui, time: f64, mood: u8) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 120.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, Rounding::same(8.0), row_dark_col());
    let ground = rect.bottom() - 16.0;
    p.line_segment(
        [Pos2::new(rect.left() + 14.0, ground), Pos2::new(rect.right() - 14.0, ground)],
        Stroke::new(1.0f32, line_col()),
    );
    let blk = 5.0;
    let crab_w = 12.0 * blk;
    let span = w - 60.0 - crab_w - 8.0 * blk;
    let speed = if mood == 1 { 2.4 } else { 1.0 };
    let phase = (time * 0.12 * speed) % 2.0;
    let k = if phase < 1.0 { phase } else { 2.0 - phase } as f32;
    let k = k * k * (3.0 - 2.0 * k);
    let hop = if mood == 2 {
        ((time * 6.0).sin().abs() * 3.0) as f32 * blk
    } else {
        0.0
    };
    let x = rect.left() + 30.0 + span * k;
    let y = ground - 8.0 * blk - hop;
    let tick = (time * 30.0 * speed) as u64;
    let frame = if mood == 2 {
        if (tick / 45) % 4 == 3 {
            &art::IDLE_BLINK
        } else {
            &art::IDLE_STAND
        }
    } else {
        art::WALK[((tick / 5) % 8) as usize]
    };
    p.rect_filled(
        Rect::from_min_size(Pos2::new(x + blk, ground - 3.0), Vec2::new(crab_w - 2.0 * blk, 4.0)),
        Rounding::same(2.0),
        Color32::from_rgb(20, 18, 17),
    );
    let o = Pos2::new(x, y);
    draw_crab(&p, o, blk, frame);
    draw_def(&p, o - Vec2::new(0.0, 6.0 * blk), blk, "Cowboy-Hut");
    draw_def(&p, o + Vec2::new(11.0 * blk, -2.0 * blk), blk, "Kaffee");
    let item = o + Vec2::new(11.0 * blk, -2.0 * blk);
    let f = (time * 6.0) as i32;
    const ZIG: [i32; 4] = [0, 0, 1, 1];
    for w in 0..2 {
        for k in 0..4 {
            let q = (f - k + w * 3).rem_euclid(12);
            if q % 6 == 5 || (k == 3 && q % 2 == 1) {
                continue;
            }
            let c = 3 + w * 2 - 1 + ZIG[(q % 4) as usize];
            let r = 1 - k;
            let col = if k <= 1 { art::STEAM } else { art::STEAM_FAINT };
            p.rect_filled(
                Rect::from_min_size(item + Vec2::new(c as f32 * blk, r as f32 * blk), Vec2::splat(blk)),
                0.0,
                col,
            );
        }
    }
}

fn progress_bar(ui: &mut egui::Ui, frac: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 10.0), egui::Sense::hover());
    let shown = ui.ctx().animate_value_with_time(egui::Id::new("bar"), frac, 0.35);
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(5.0), row_dark_col());
    let mut fill = rect;
    fill.set_width((w * shown).max(10.0));
    p.rect_filled(fill, Rounding::same(5.0), GOLD);
}

impl soft::SoftApp for Setup {
    fn update(&mut self, ctx: &egui::Context) {
        let de = self.de;
        let time = ctx.input(|i| i.time);
        ctx.request_repaint_after(Duration::from_millis(33));

        if self.page == Page::Busy {
            let p = self.progress.lock().unwrap();
            if p.done {
                self.page = Page::Done;
            } else if p.error.is_some() {
                self.page = Page::Failed;
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_col()).inner_margin(egui::Margin::same(20.0)))
            .show(ctx, |ui| {
                let mood = match self.page {
                    Page::Busy => 1,
                    Page::Done => 2,
                    _ => 0,
                };
                stage(ui, time, mood);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(APP_NAME).size(22.0).color(GOLD).strong());
                    ui.label(RichText::new(format!("v{VERSION}")).size(13.0).color(MUTED));
                    if self.page == Page::Welcome {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if button(ui, "Deutsch", 80.0, self.de).clicked() {
                                self.de = true;
                            }
                            if button(ui, "English", 80.0, !self.de).clicked() {
                                self.de = false;
                            }
                        });
                    }
                });
                ui.add_space(6.0);

                match self.page {
                    Page::Welcome => {
                        let sub = if self.existing.is_some() {
                            t(de, "Eine ältere Version ist installiert und wird aktualisiert. Deine Einstellungen und Sprites bleiben erhalten.",
                                "An older version is installed and will be updated. Your settings and sprites are kept.")
                        } else {
                            t(de, "Die Pixel-Krabbe für deinen Desktop. Keine Adminrechte nötig.",
                                "The pixel crab for your desktop. No admin rights needed.")
                        };
                        ui.label(RichText::new(sub).size(13.0).color(TEXT));
                        ui.add_space(10.0);
                        row(ui, t(de, "Ordner", "Folder"), |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.dir).desired_width(300.0));
                        });
                        row(ui, t(de, "Verknüpfung auf dem Desktop", "Desktop shortcut"), |ui| {
                            check(ui, &mut self.desktop)
                        });
                        row(ui, t(de, "Mit Windows starten", "Start with Windows"), |ui| {
                            check(ui, &mut self.autostart)
                        });
                        row(ui, t(de, "Danach direkt starten", "Launch when done"), |ui| {
                            check(ui, &mut self.launch)
                        });
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let label = if self.existing.is_some() {
                                t(de, "Aktualisieren", "Update")
                            } else {
                                t(de, "Installieren", "Install")
                            };
                            if button(ui, label, 150.0, true).clicked() {
                                self.start();
                            }
                            if button(ui, t(de, "Abbrechen", "Cancel"), 110.0, false).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            if let Some(dir) = &self.existing {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if button(ui, t(de, "Deinstallieren", "Uninstall"), 120.0, false).clicked() {
                                        let _ = std::process::Command::new(dir.join(APP_EXE))
                                            .arg("--uninstall")
                                            .spawn();
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                });
                            }
                        });
                    }
                    Page::Busy => {
                        let (step, frac) = {
                            let p = self.progress.lock().unwrap();
                            (p.step.clone(), p.frac)
                        };
                        ui.add_space(8.0);
                        ui.label(RichText::new(step).size(13.0).color(TEXT));
                        ui.add_space(8.0);
                        progress_bar(ui, frac);
                    }
                    Page::Done => {
                        let notes = self.progress.lock().unwrap().notes.clone();
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(t(
                                de,
                                "Fertig! Die Krabbe wohnt jetzt auf deinem Desktop. Rechtsklick auf sie öffnet das Studio, das Symbol unten rechts in der Taskleiste das Menü.",
                                "Done! The crab now lives on your desktop. Right-click it to open the studio, the tray icon opens the menu.",
                            ))
                            .size(13.0)
                            .color(TEXT),
                        );
                        for n in notes {
                            ui.label(RichText::new(n).size(12.0).color(MUTED));
                        }
                        ui.add_space(10.0);
                        progress_bar(ui, 1.0);
                        ui.add_space(12.0);
                        if button(ui, t(de, "Schließen", "Close"), 150.0, true).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    Page::Failed => {
                        let err = self.progress.lock().unwrap().error.clone().unwrap_or_default();
                        ui.label(
                            RichText::new(t(de, "Das hat nicht geklappt:", "That did not work:"))
                                .size(13.0)
                                .color(TEXT),
                        );
                        ui.label(RichText::new(err).size(12.5).color(Color32::from_rgb(230, 120, 100)));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if button(ui, t(de, "Nochmal versuchen", "Try again"), 160.0, true).clicked() {
                                self.page = Page::Welcome;
                            }
                            if button(ui, t(de, "Schließen", "Close"), 110.0, false).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    }
                }

                let p = ui.painter();
                p.text(
                    ui.max_rect().right_bottom(),
                    Align2::RIGHT_BOTTOM,
                    t(de, "Deinstallieren: Einstellungen > Apps", "Uninstall: Settings > Apps"),
                    FontId::proportional(11.0),
                    MUTED,
                );
            });
    }
}

fn silent(args: &[String]) -> i32 {
    let s = Setup::new();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let dir = args
        .iter()
        .find_map(|a| a.strip_prefix("--dir="))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&s.dir));
    let opts = Options {
        dir,
        desktop: !flag("--no-desktop"),
        autostart: !flag("--no-autostart"),
        launch: !flag("--no-launch"),
        de: s.de,
    };
    let progress = Arc::new(Mutex::new(Progress::default()));
    match install(&opts, &progress) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--silent") {
        std::process::exit(silent(&args));
    }
    let ctx = egui::Context::default();
    apply_style(&ctx);
    soft::run(
        soft::Options {
            title: format!("{APP_NAME} Setup"),
            size: [560.0, 470.0],
            min_size: None,
            resizable: false,
            icon: Some((art::logo_rgba(64), 64)),
            custom_frame: false,
        },
        ctx,
        Setup::new(),
    );
}
