use crate::art;
use crate::render::{self, Brush, ItemPose};
use crate::sprite::{anim_index, Anim, Core, Settings, Sprite, CLOTH_H, CLOTH_W, HAT_H, HAT_W, ITEM_N};
use crate::sys;
use crate::widgets::*;
use std::time::Duration;
use egui::{
    self, Align2, Color32, Context, FontId, Id, Key, Pos2, Rect, RichText, Rounding, Sense, Stroke,
    TextureHandle, TextureOptions, Ui, Vec2,
};

const SIZES: [(usize, usize); 7] = [(HAT_W, HAT_H), (HAT_W * 2, HAT_H * 2), (CLOTH_W, CLOTH_H), (ITEM_N, ITEM_N), (16, 16), (32, 32), (48, 48)];
const SCALE_LABELS: [&str; 4] = ["1x", "4x", "8x", "16x"];
const SCALES: [u32; 4] = [1, 4, 8, 16];
const GRIP: (usize, usize) = (1, 5);

pub struct PetRemote {
    flags: isize,
    polled: f64,
}

impl PetRemote {
    pub fn new() -> Self {
        Self {
            flags: 0,
            polled: -1.0,
        }
    }

    fn refresh(&mut self, now: f64) {
        if now - self.polled > 0.3 {
            self.polled = now;
            self.flags = crate::petwin::query().unwrap_or(0);
        }
    }

    fn send(&self, cmd: &str) {
        if !crate::petwin::send(cmd) {
            dlog!("studio: no pet running for '{cmd}'");
        }
    }

    pub fn is_sleeping(&self) -> bool {
        self.flags & 1 != 0
    }

    pub fn is_paused(&self) -> bool {
        self.flags & 2 != 0
    }

    pub fn trigger_cowboy(&mut self, _core: &Core) {
        self.send("cowboy");
    }

    pub fn trigger_look_around(&mut self) {
        self.send("look");
    }

    pub fn trigger_play(&mut self, core: &Core) -> bool {
        let has_item = core.buddy_sprite().is_none()
            && core.settings.item_visible
            && core.item_sprite().is_some();
        if has_item {
            self.send("trick");
        }
        has_item
    }

    pub fn toggle_sleep(&mut self, _core: &Core) {
        self.send("sleep");
        self.flags ^= 1;
    }

    pub fn toggle_pause(&mut self) {
        self.send("pause");
        self.flags ^= 2;
    }

    pub fn set_speech(&mut self, _core: &Core, text: &str) {
        self.send(&format!("say:{text}"));
    }

    pub fn quit(&mut self) {
        self.send("quit");
    }

    pub fn reload(&mut self) {
        self.send("reload");
    }
}

fn config_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(crate::sprite::data_path("crab_config.json"))
        .and_then(|m| m.modified())
        .ok()
}
fn tx(en: bool, de: &'static str, e: &'static str) -> &'static str {
    if en {
        e
    } else {
        de
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Crab,
    Behavior,
    Claude,
    Pixel,
    Library,
    App,
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Pencil,
    Eraser,
    Fill,
    Picker,
    Line,
    Rect,
    Circle,
    Select,
}

type Area = (i32, i32, i32, i32);

#[derive(Clone, Copy)]
enum LibAct {
    Edit,
    Overwrite,
    Use,
    Rename,
    Export,
    Delete,
}

fn is_blank(s: &Sprite) -> bool {
    s.px.iter().all(|p| p[3] == 0)
}

pub struct Studio {
    en: bool,
    tab: Tab,
    tab_t0: f64,
    sprite: Sprite,
    undo: Vec<Sprite>,
    redo: Vec<Sprite>,
    tool: Tool,
    color: Color32,
    size_idx: usize,
    show_grid: bool,
    mirror: bool,
    mirror_v: bool,
    zoom: f32,
    anchor: Option<(i32, i32)>,
    area: Option<Area>,
    moving: Option<((i32, i32), Vec<(i32, i32, [u8; 4])>, Area)>,
    export_idx: usize,
    last: Option<(i32, i32)>,
    hover_cell: Option<(i32, i32)>,
    preview: TextureHandle,
    shown: Sprite,
    sel: Option<usize>,
    rename: String,
    save_name: String,
    speech: String,
    toast: Option<(String, f64)>,
    saved: Settings,
    saved_sel: (Option<usize>, Option<usize>, Option<usize>),
    disk_mtime: Option<std::time::SystemTime>,
    disk_check: f64,
    template: usize,
    cc_on: bool,
    frames: Vec<Sprite>,
    cur: usize,
    fps: u8,
    mode: u8,
    playing: bool,
    play_t0: f64,
    onion: bool,
    drag_frame: Option<usize>,
    pose: usize,
    back: bool,
    icon_key: u64,
    outfit_name: String,
    wear_pick: usize,
}

impl Studio {
    pub fn new(ctx: &Context, core: &Core) -> Self {
        let full = core
            .hat
            .and_then(|i| core.slots.get(i))
            .map(|s| s.sprite.clone())
            .unwrap_or_else(|| Sprite::new(HAT_W, HAT_H));
        let sprite = full.frame(0).clone();
        let preview = ctx.load_texture("preview", sprite.color_image(), TextureOptions::NEAREST);
        let mut me = Self {
            en: core.settings.lang == 1,
            tab: Tab::Crab,
            tab_t0: 0.0,
            shown: sprite.clone(),
            sprite,
            undo: Vec::new(),
            redo: Vec::new(),
            tool: Tool::Pencil,
            color: Color32::from_rgb(226, 188, 116),
            size_idx: 0,
            show_grid: true,
            mirror: false,
            mirror_v: false,
            zoom: 1.0,
            anchor: None,
            area: None,
            moving: None,
            export_idx: 1,
            last: None,
            hover_cell: None,
            preview,
            sel: None,
            rename: String::new(),
            save_name: String::new(),
            speech: String::new(),
            toast: None,
            saved: core.settings.clone(),
            saved_sel: (core.hat, core.buddy, core.item),
            disk_mtime: config_mtime(),
            disk_check: 0.0,
            template: 0,
            cc_on: crate::claude::is_connected(),
            frames: Vec::new(),
            cur: 0,
            fps: 6,
            mode: 0,
            playing: false,
            play_t0: 0.0,
            onion: true,
            drag_frame: None,
            pose: 0,
            back: false,
            icon_key: 0,
            outfit_name: String::new(),
            wear_pick: 0,
        };
        me.load_anim(full);
        me
    }

    fn load_anim(&mut self, s: Sprite) {
        self.fps = if s.anim.fps == 0 { 6 } else { s.anim.fps.clamp(1, 24) };
        self.mode = s.anim.mode.min(2);
        self.back = s.back;
        let mut first = s.clone();
        first.anim = Anim::default();
        let more = if first.is_item() { s.anim.frames.clone() } else { Vec::new() };
        self.frames = std::iter::once(first).chain(more).collect();
        self.cur = 0;
        self.sprite = self.frames[0].clone();
        self.playing = false;
        self.area = None;
        self.undo.clear();
        self.redo.clear();
        self.sync_size_idx();
    }

    fn assembled(&self) -> Sprite {
        let mut all = self.frames.clone();
        if let Some(f) = all.get_mut(self.cur) {
            *f = self.sprite.clone();
        }
        let mut first = all.remove(0);
        first.anim = if all.is_empty() || !first.is_item() { Anim::default() } else { Anim { frames: all, fps: self.fps, mode: self.mode } };
        first.back = self.back && first.is_clothing();
        first
    }

    fn goto(&mut self, i: usize) {
        if i >= self.frames.len() || i == self.cur {
            return;
        }
        self.frames[self.cur] = self.sprite.clone();
        self.cur = i;
        self.sprite = self.frames[i].clone();
        self.undo.clear();
        self.redo.clear();
        self.area = None;
    }

    fn shown_frame(&self, t: f64) -> Sprite {
        if !self.playing || self.frames.len() < 2 {
            return self.sprite.clone();
        }
        let i = anim_index(self.frames.len(), self.fps, self.mode, (t - self.play_t0) as f32);
        if i == self.cur { self.sprite.clone() } else { self.frames[i].clone() }
    }

    pub fn frame(&mut self, ctx: &Context, core: &mut Core, pet: &mut PetRemote) {
        self.en = core.settings.lang == 1;
        pet.refresh(ctx.input(|i| i.time));
        self.sync_from_disk(ctx, core);
        let st = &core.settings;
        set_theme(ctx, if st.glass { 0 } else if st.black { 2 } else { 1 });
        set_accent(ctx, parse_hex(&st.accent_hex).unwrap_or(ACCENTS[st.accent.min(ACCENTS.len() - 1)]));
        render::apply_crab_colors(st);
        render::set_outfit(core.outfit_layers());
        let look = render::crab_look_key(core);
        if look != self.icon_key {
            self.icon_key = look;
            crate::soft::set_icon(render::crab_icon(core, 64), 64);
        }
        let st = &core.settings;
        if st.crab_rainbow {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        let (cmd, shift, z, y) = ctx.input(|i| {
            (
                i.modifiers.command,
                i.modifiers.shift,
                i.key_pressed(Key::Z),
                i.key_pressed(Key::Y),
            )
        });
        if cmd && z {
            if shift {
                self.do_redo();
            } else {
                self.do_undo();
            }
        }
        if cmd && y {
            self.do_redo();
        }
        if self.tab == Tab::Pixel && !ctx.wants_keyboard_input() && !cmd {
            let keys = [
                (Key::B, Tool::Pencil),
                (Key::E, Tool::Eraser),
                (Key::G, Tool::Fill),
                (Key::I, Tool::Picker),
                (Key::L, Tool::Line),
                (Key::R, Tool::Rect),
                (Key::C, Tool::Circle),
                (Key::M, Tool::Select),
            ];
            ctx.input(|i| {
                for (k, t) in keys {
                    if i.key_pressed(k) {
                        self.tool = t;
                    }
                }
            });
            let (del, esc, zin, zout) = ctx.input(|i| (i.key_pressed(Key::Delete), i.key_pressed(Key::Escape), i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals), i.key_pressed(Key::Minus)));
            let (space, prev, next) = ctx.input(|i| (i.key_pressed(Key::Space), i.key_pressed(Key::Comma), i.key_pressed(Key::Period)));
            if space && self.frames.len() > 1 && self.sprite.is_item() {
                self.playing = !self.playing;
                self.play_t0 = ctx.input(|i| i.time);
            }
            let n = self.frames.len();
            if prev && n > 1 {
                self.goto((self.cur + n - 1) % n);
            }
            if next && n > 1 {
                self.goto((self.cur + 1) % n);
            }
            if del {
                self.clear_area();
            }
            if esc {
                self.area = None;
            }
            if zin {
                self.zoom = (self.zoom * 1.25).min(8.0);
            }
            if zout {
                self.zoom = (self.zoom / 1.25).max(0.25);
            }
        }

        let disp = self.shown_frame(ctx.input(|i| i.time));
        if self.shown != disp {
            self.preview.set(disp.color_image(), TextureOptions::NEAREST);
            self.shown = disp;
        }
        if self.playing {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        let title = egui::TopBottomPanel::top("title")
            .exact_height(crate::soft::TITLE_H)
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(if glass() { Color32::TRANSPARENT } else { panel_col() }))
            .show(ctx, |ui| title_bar(ui));
        if glass() {
            glass_rect(title.response.rect.shrink2(Vec2::new(6.0, 3.0)), 10.0, ctx.screen_rect());
        }

        let nav = egui::SidePanel::left("nav")
            .exact_width(84.0)
            .resizable(false)
            .show_separator_line(!glass())
            .frame(panel_frame(8.0, panel_col()))
            .show(ctx, |ui| self.sidebar(ui, core));
        if glass() {
            glass_rect(nav.response.rect.shrink(6.0), 16.0, ctx.screen_rect());
        }

        if self.tab == Tab::Pixel {
            let tools = egui::SidePanel::left("tools")
                .exact_width(214.0)
                .resizable(false)
                .show_separator_line(!glass())
                .frame(panel_frame(14.0, panel_col()))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.tools(ui, core));
                });
            if glass() {
                glass_rect(tools.response.rect.shrink(6.0), 16.0, ctx.screen_rect());
            }
        }

        let central = egui::CentralPanel::default()
            .frame(if glass() {
                egui::Frame::none()
                    .outer_margin(egui::Margin::same(6.0))
                    .inner_margin(egui::Margin::same(14.0))
            } else {
                egui::Frame::none().fill(bg_col()).inner_margin(egui::Margin::same(18.0))
            })
            .show(ctx, |ui| {
                let k = ((ctx.input(|i| i.time) - self.tab_t0) / 0.24).clamp(0.0, 1.0) as f32;
                if k < 1.0 {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
                let e = 1.0 - (1.0 - k).powi(3);
                ui.add_space((1.0 - e) * 18.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Crab => self.crab_view(ui, core),
                        Tab::Behavior => self.behavior_view(ui, core, pet),
                        Tab::Claude => self.claude_view(ui, core, pet),
                        Tab::Pixel => self.pixel_view(ui, core),
                        Tab::Library => self.library_view(ui, core),
                        Tab::App => self.app_view(ui, core),
                    });
            });
        if glass() {
            glass_rect(central.response.rect.shrink(6.0), 16.0, ctx.screen_rect());
        }

        self.draw_toast(ctx);
        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing {
            dlog!("studio: close requested");
        }
        self.autosave(ctx, core, pet, closing);
    }

    fn sync_from_disk(&mut self, ctx: &Context, core: &mut Core) {
        let now = ctx.input(|i| i.time);
        if now - self.disk_check < 1.0 {
            return;
        }
        self.disk_check = now;
        let m = config_mtime();
        if m != self.disk_mtime && core.settings == self.saved {
            if let Some(s) = crate::sprite::read_settings() {
                core.settings = s;
                self.saved = core.settings.clone();
            }
            self.disk_mtime = m;
        }
        ctx.request_repaint_after(Duration::from_millis(1000));
    }

    fn autosave(&mut self, ctx: &Context, core: &mut Core, pet: &mut PetRemote, force: bool) {
        let busy = ctx.input(|i| i.pointer.any_down());
        let mut wrote = false;
        if core.dirty_lib && (!busy || force) {
            match core.save_library() {
                Ok(_) => {
                    core.dirty_lib = false;
                    wrote = true;
                }
                Err(e) => self.notify(ctx, format!("{}: {e}", tx(self.en, "Speichern fehlgeschlagen", "Saving failed"))),
            }
        }
        let sel = (core.hat, core.buddy, core.item);
        if (core.settings != self.saved || sel != self.saved_sel) && (!busy || force) {
            match core.save_settings() {
                Ok(_) => {
                    self.saved = core.settings.clone();
                    self.saved_sel = sel;
                    wrote = true;
                }
                Err(e) => self.notify(ctx, format!("{}: {e}", tx(self.en, "Speichern fehlgeschlagen", "Saving failed"))),
            }
        }
        if wrote {
            self.disk_mtime = config_mtime();
            pet.reload();
        }
    }

    fn go(&mut self, ctx: &Context, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.tab_t0 = ctx.input(|i| i.time);
        }
    }

    fn notify(&mut self, ctx: &Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time)));
    }

    fn push_undo(&mut self) {
        self.undo.push(self.sprite.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn do_undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.sprite, s));
            self.sync_size_idx();
        }
    }

    fn do_redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.sprite, s));
            self.sync_size_idx();
        }
    }

    fn sync_size_idx(&mut self) {
        if let Some(i) = SIZES
            .iter()
            .position(|&(w, h)| w == self.sprite.w && h == self.sprite.h)
        {
            self.size_idx = i;
        }
    }

    fn plot(&mut self, x: i32, y: i32, c: [u8; 4]) {
        let (w, h) = (self.sprite.w as i32, self.sprite.h as i32);
        for (px, py) in mirrored(x, y, w, h, self.mirror, self.mirror_v) {
            if px >= 0 && py >= 0 && px < w && py < h {
                self.sprite.set(px as usize, py as usize, c);
            }
        }
    }

    fn export(&mut self, ctx: &Context, sprite: Sprite, name: &str) {
        let path = sys::export_dir().join(format!("{name}.png"));
        let en = self.en;
        match sprite.save(&path, SCALES[self.export_idx]) {
            Ok(_) => self.notify(ctx, format!("{}: {}", tx(en, "Gespeichert", "Saved"), path.display())),
            Err(e) => self.notify(ctx, format!("{}: {e}", tx(en, "Export fehlgeschlagen", "Export failed"))),
        }
    }

    fn export_anim(&mut self, ctx: &Context, anim: &Sprite, dir: &std::path::Path) {
        let en = self.en;
        let k = SCALES[self.export_idx];
        let imgs: Vec<image::RgbaImage> = (0..anim.count())
            .filter_map(|i| {
                let f = anim.frame(i);
                let img = image::RgbaImage::from_raw(f.w as u32, f.h as u32, f.bytes())?;
                Some(image::imageops::resize(&img, f.w as u32 * k, f.h as u32 * k, image::imageops::FilterType::Nearest))
            })
            .collect();
        let (fw, fh) = imgs[0].dimensions();
        let mut sheet = image::RgbaImage::new(fw * imgs.len() as u32, fh);
        for (i, img) in imgs.iter().enumerate() {
            image::imageops::overlay(&mut sheet, img, (i as u32 * fw) as i64, 0);
        }
        let order: Vec<usize> = match anim.anim.mode {
            1 => (0..imgs.len()).chain((1..imgs.len() - 1).rev()).collect(),
            _ => (0..imgs.len()).collect(),
        };
        let gif = || -> Result<(), String> {
            let file = std::fs::File::create(dir.join("crab_sprite.gif")).map_err(|e| e.to_string())?;
            let mut enc = image::codecs::gif::GifEncoder::new(file);
            enc.set_repeat(if anim.anim.mode == 2 { image::codecs::gif::Repeat::Finite(0) } else { image::codecs::gif::Repeat::Infinite })
                .map_err(|e| e.to_string())?;
            let delay = image::Delay::from_numer_denom_ms(1000, anim.anim.fps.clamp(1, 24) as u32);
            enc.encode_frames(order.iter().map(|&i| image::Frame::from_parts(imgs[i].clone(), 0, 0, delay)))
                .map_err(|e| e.to_string())
        };
        let result = sheet
            .save(dir.join("crab_sprite_sheet.png"))
            .map_err(|e| e.to_string())
            .and_then(|_| gif());
        match result {
            Ok(_) => self.notify(ctx, format!("{}: {}", tx(en, "GIF + Spritesheet gespeichert", "GIF + spritesheet saved"), dir.display())),
            Err(e) => self.notify(ctx, format!("{}: {e}", tx(en, "Export fehlgeschlagen", "Export failed"))),
        }
    }

    fn timeline(&mut self, ui: &mut Ui) {
        let en = self.en;
        let t = ui.input(|i| i.time);
        heading(ui, "Animation");
        let n = self.frames.len();
        let playing_at = self.playing.then(|| anim_index(n, self.fps, self.mode, (t - self.play_t0) as f32));
        let mut rects = Vec::new();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(6.0);
            for i in 0..n {
                let f = if i == self.cur { &self.sprite } else { &self.frames[i] };
                let k = (40.0 / f.w.max(f.h) as f32).floor().max(1.0);
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::click_and_drag());
                rects.push(rect);
                let p = ui.painter();
                let on = playing_at.map_or(i == self.cur, |j| j == i);
                p.rect_filled(rect, Rounding::same(5.0), row_dark_col());
                let o = rect.center() - Vec2::new(f.w as f32 * k, f.h as f32 * k) / 2.0;
                for y in 0..f.h {
                    for x in 0..f.w {
                        let c = f.get(x, y);
                        if c[3] > 0 {
                            p.rect_filled(
                                Rect::from_min_size(o + Vec2::new(x as f32 * k, y as f32 * k), Vec2::splat(k)),
                                0.0,
                                Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
                            );
                        }
                    }
                }
                p.rect_stroke(rect, Rounding::same(5.0), Stroke::new(if on { 2.0f32 } else { 1.0 }, if on { accent() } else { line_col() }));
                p.text(rect.left_top() + Vec2::new(4.0, 2.0), Align2::LEFT_TOP, format!("{}", i + 1), FontId::proportional(10.0), MUTED);
                if resp.clicked() {
                    self.playing = false;
                    self.goto(i);
                }
                if resp.drag_started() {
                    self.drag_frame = Some(i);
                }
            }
        });
        if let Some(from) = self.drag_frame {
            let pos = ui.input(|i| i.pointer.hover_pos());
            let target = pos.and_then(|p| rects.iter().position(|r| r.contains(p)));
            if let Some(to) = target.filter(|&to| to != from) {
                ui.painter().rect_stroke(rects[to].expand(2.0), Rounding::same(6.0), Stroke::new(2.0f32, accent()));
            }
            if ui.input(|i| i.pointer.any_released()) {
                self.drag_frame = None;
                if let Some(to) = target.filter(|&to| to != from) {
                    self.frames[self.cur] = self.sprite.clone();
                    let cur = self.frames[self.cur].clone();
                    let f = self.frames.remove(from);
                    self.frames.insert(to, f);
                    self.cur = self.frames.iter().position(|x| *x == cur).unwrap_or(to);
                    self.sprite = self.frames[self.cur].clone();
                }
            }
        }
        ui.horizontal(|ui| {
            if button(ui, tx(en, "+ Frame", "+ Frame"), 70.0, false).clicked() && n < 30 {
                self.frames[self.cur] = self.sprite.clone();
                self.frames.insert(self.cur + 1, Sprite::new(self.sprite.w, self.sprite.h));
                self.goto(self.cur + 1);
            }
            if button(ui, tx(en, "Duplizieren", "Duplicate"), 86.0, false).clicked() && n < 30 {
                self.frames[self.cur] = self.sprite.clone();
                self.frames.insert(self.cur + 1, self.sprite.clone());
                self.goto(self.cur + 1);
            }
            if button(ui, tx(en, "Löschen", "Delete"), 70.0, false).clicked() && n > 1 {
                self.frames.remove(self.cur);
                self.cur = self.cur.min(self.frames.len() - 1);
                self.sprite = self.frames[self.cur].clone();
                self.undo.clear();
                self.redo.clear();
            }
            let label = if self.playing { tx(en, "Pause", "Pause") } else { tx(en, "Abspielen", "Play") };
            if button(ui, label, 86.0, self.playing).clicked() && n > 1 {
                self.playing = !self.playing;
                self.play_t0 = t;
            }
        });
        let mut fps = self.fps as f32;
        row(ui, "FPS", |ui| slim(ui, &mut fps, 1.0..=24.0, 120.0, |v| format!("{v:.0}")));
        self.fps = fps.round().clamp(1.0, 24.0) as u8;
        let mut mode = self.mode as usize;
        row(ui, tx(en, "Ablauf", "Playback"), |ui| {
            combo(ui, "anim_mode", &mut mode, &[tx(en, "Schleife", "Loop"), "Ping-Pong", tx(en, "Einmal", "Once")], 110.0)
        });
        self.mode = mode as u8;
        row(ui, tx(en, "Zwiebelschicht", "Onion skin"), |ui| check(ui, &mut self.onion));
    }

    fn apply_current(&mut self, ctx: &Context, core: &mut Core) {
        let en = self.en;
        let sprite = self.assembled();
        if is_blank(&sprite) {
            if sprite.is_hat() {
                core.hat = None;
                self.notify(ctx, tx(en, "Hut abgelegt", "Hat removed"));
            } else if sprite.is_item() {
                core.item = None;
                self.notify(ctx, tx(en, "Item weggelegt", "Item put away"));
            } else {
                self.notify(ctx, tx(en, "Die Zeichnung ist leer", "The drawing is empty"));
            }
            return;
        }
        let idx = match core.find_same(&sprite) {
            Some(i) => i,
            None => {
                let name = self.save_name.trim().to_string();
                let name = if name.is_empty() { None } else { Some(name) };
                core.add_slot(ctx, name, sprite.clone())
            }
        };
        if sprite.is_clothing() {
            let name = core.slots[idx].name.clone();
            core.settings.clothes.retain(|n| *n != name);
            core.settings.clothes.push(name);
            self.notify(ctx, tx(en, "Angezogen", "Put on"));
        } else if sprite.is_hat() {
            core.hat = Some(idx);
            core.settings.hat_visible = true;
            self.notify(ctx, tx(en, "Neuer Style!", "New style!"));
        } else if sprite.is_item() {
            core.item = Some(idx);
            core.settings.item_visible = true;
            self.notify(ctx, tx(en, "Item ist in der Hand", "Item is in hand"));
        } else {
            core.buddy = Some(idx);
            self.notify(ctx, tx(en, "Sprite ist jetzt dein Buddy", "Sprite is now your buddy"));
        }
        self.sel = Some(idx);
        self.rename = core.slots[idx].name.clone();
    }

    fn save_current(&mut self, ctx: &Context, core: &mut Core) {
        let en = self.en;
        if is_blank(&self.sprite) {
            self.notify(ctx, tx(en, "Die Zeichnung ist leer", "The drawing is empty"));
            return;
        }
        let name = self.save_name.trim().to_string();
        let name = if name.is_empty() { None } else { Some(name) };
        let idx = core.add_slot(ctx, name, self.assembled());
        self.sel = Some(idx);
        self.rename = core.slots[idx].name.clone();
        self.save_name.clear();
        self.notify(ctx, tx(en, "In Bibliothek gespeichert", "Saved to library"));
    }

    fn logo(&self, ui: &mut Ui, w: f32, core: &Core) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 66.0), Sense::hover());
        let p = ui.painter_at(rect);
        let brush = Brush::painter(p.clone(), rect.min, ui.ctx().pixels_per_point());
        let blk = 3.0;
        let x = ((w - 12.0 * blk) / 2.0).floor();
        let y = 6.0 * blk + 6.0;
        render::draw_crab(&brush, x, y, blk, &art::IDLE_STAND, 0.0);
        if let Some(hat) = core.hat_sprite().filter(|_| core.settings.hat_visible) {
            render::draw_hat(&brush, hat, x, y, blk, core.hat_drop() * blk);
        }
        p.text(
            rect.center_bottom() + Vec2::new(0.0, -6.0),
            Align2::CENTER_CENTER,
            "CLAUDE CRAB",
            FontId::proportional(9.0),
            accent(),
        );
    }

    fn sidebar(&mut self, ui: &mut Ui, core: &Core) {
        let ctx = ui.ctx().clone();
        let en = self.en;
        let tabs = [
            (Tab::Crab, tx(en, "Krabbe", "Crab")),
            (Tab::Behavior, tx(en, "Verhalten", "Behavior")),
            (Tab::Claude, "Claude"),
            (Tab::Pixel, "Pixel Art"),
            (Tab::Library, "Sprites"),
            (Tab::App, "App"),
        ];
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
        let w = ui.available_width();
        self.logo(ui, w, core);
        ui.add_space(10.0);
        let item_h = 58.0;
        let origin = ui.next_widget_position();
        let idx = tabs.iter().position(|(t, _)| *t == self.tab).unwrap_or(0) as f32;
        let ind = ctx.animate_value_with_time(Id::new("tab_indicator"), idx, 0.22);
        let ir = Rect::from_min_size(
            Pos2::new(origin.x, origin.y + ind * (item_h + 6.0)),
            Vec2::new(w, item_h),
        );
        {
            let p = ui.painter();
            if glass() {
                glass_rect(ir.shrink(2.0), 12.0, ui.clip_rect());
            } else {
                p.rect_filled(ir, Rounding::same(8.0), accent().gamma_multiply(0.10));
            }
            p.rect_filled(
                Rect::from_min_size(
                    Pos2::new(ir.left(), ir.top() + 16.0),
                    Vec2::new(3.0, item_h - 32.0),
                ),
                Rounding::same(2.0),
                accent(),
            );
        }
        for (tab, name) in tabs {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, item_h), Sense::click());
            let h = ctx.animate_bool_with_time(resp.id, resp.hovered(), 0.12);
            let t = ctx.animate_bool_with_time(resp.id.with("s"), tab == self.tab, 0.2);
            let col = mix(mix(MUTED, TEXT, h), accent(), t);
            let c = rect.center() + Vec2::new(0.0, -8.0);
            let p = ui.painter();
            match tab {
                Tab::Crab => icon_crab(p, c, col),
                Tab::Behavior => icon_sliders(p, c, col),
                Tab::Claude => icon_terminal(p, c, col),
                Tab::Pixel => icon_pixels(p, c, col),
                Tab::Library => icon_library(p, c, col),
                Tab::App => icon_gear(p, c, col),
            }
            p.text(
                rect.center() + Vec2::new(0.0, 16.0),
                Align2::CENTER_CENTER,
                name,
                FontId::proportional(11.5),
                col,
            );
            if resp.clicked() {
                self.go(&ctx, tab);
            }
        }
    }

    fn size_labels(&self) -> [&'static str; 7] {
        [
            tx(self.en, "Hut 12 x 6", "Hat 12 x 6"),
            tx(self.en, "Hut fein 24 x 12", "Fine hat 24 x 12"),
            tx(self.en, "Kleidung 24 x 16", "Clothing 24 x 16"),
            tx(self.en, "Item 8 x 8", "Item 8 x 8"),
            "16 x 16",
            "32 x 32",
            "48 x 48",
        ]
    }

    fn tools(&mut self, ui: &mut Ui, core: &mut Core) {
        let ctx = ui.ctx().clone();
        let en = self.en;
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        heading(ui, tx(en, "Werkzeug", "Tool"));
        ui.horizontal(|ui| {
            for (t, name) in [
                (Tool::Pencil, tx(en, "Stift", "Pencil")),
                (Tool::Eraser, tx(en, "Radierer", "Eraser")),
            ] {
                if button(ui, name, 86.0, self.tool == t).clicked() {
                    self.tool = t;
                }
            }
        });
        for pair in [
            [(Tool::Fill, tx(en, "Füllen", "Fill")), (Tool::Picker, tx(en, "Pipette", "Picker"))],
            [(Tool::Line, tx(en, "Linie", "Line")), (Tool::Rect, tx(en, "Rechteck", "Rectangle"))],
            [(Tool::Circle, tx(en, "Kreis", "Circle")), (Tool::Select, tx(en, "Auswahl", "Select"))],
        ] {
            ui.horizontal(|ui| {
                for (t, name) in pair {
                    if button(ui, name, 86.0, self.tool == t).clicked() {
                        self.tool = t;
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            for (op, name) in [(0, tx(en, "Spiegeln", "Flip")), (1, tx(en, "Kippen", "Flip V")), (2, tx(en, "Drehen", "Rotate"))] {
                if button(ui, name, 56.0, false).clicked() {
                    self.transform(op);
                }
            }
        });
        ui.label(
            RichText::new(tx(
                en,
                "B Stift · E Radierer · G Füllen · I Pipette · L Linie · R Rechteck · C Kreis · M Auswahl · Entf löscht Auswahl · Strg+Mausrad Zoom",
                "B pencil · E eraser · G fill · I picker · L line · R rect · C circle · M select · Del clears selection · Ctrl+wheel zoom",
            ))
            .size(11.0)
            .color(MUTED),
        );

        ui.add_space(6.0);
        heading(ui, tx(en, "Farbe", "Color"));
        ui.horizontal_wrapped(|ui| {
            for (r, g, b) in PALETTE {
                let c = Color32::from_rgb(r, g, b);
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                let t = ctx.animate_bool_with_time(resp.id, self.color == c, 0.14);
                let h = ctx.animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.1);
                let p = ui.painter();
                p.rect_filled(rect, Rounding::same(4.0), c);
                if t > 0.01 || h > 0.01 {
                    p.rect_stroke(
                        rect.expand(2.0 * t),
                        Rounding::same(5.0),
                        Stroke::new(1.0 + t, mix(line_col(), accent(), t.max(h * 0.6))),
                    );
                }
                if resp.clicked() {
                    self.color = c;
                    if self.tool == Tool::Eraser {
                        self.tool = Tool::Pencil;
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            egui::color_picker::color_edit_button_srgba(
                ui,
                &mut self.color,
                egui::color_picker::Alpha::Opaque,
            );
            let [r, g, b, _] = self.color.to_srgba_unmultiplied();
            ui.label(
                RichText::new(format!("#{r:02x}{g:02x}{b:02x}"))
                    .size(12.0)
                    .color(MUTED),
            );
        });
        ui.horizontal_wrapped(|ui| {
            let mut remove = None;
            for (i, hex) in core.settings.palette.iter().enumerate() {
                let Some(c) = parse_hex(hex) else { continue };
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                let p = ui.painter();
                p.rect_filled(rect, Rounding::same(4.0), c);
                if self.color == c || resp.hovered() {
                    p.rect_stroke(rect.expand(1.0), Rounding::same(5.0), Stroke::new(1.5f32, accent()));
                }
                if resp.clicked() {
                    self.color = c;
                    if self.tool == Tool::Eraser {
                        self.tool = Tool::Pencil;
                    }
                }
                if resp.secondary_clicked() {
                    remove = Some(i);
                }
            }
            if let Some(i) = remove {
                core.settings.palette.remove(i);
            }
            if button(ui, "+", 24.0, false).clicked() && core.settings.palette.len() < 32 {
                let hex = hex_of(self.color);
                if !core.settings.palette.contains(&hex) {
                    core.settings.palette.push(hex);
                }
            }
        });
        ui.horizontal(|ui| {
            let path = sys::export_dir().join("palette.hex");
            if button(ui, tx(en, "Palette laden", "Load palette"), 86.0, false).clicked() {
                match std::fs::read_to_string(&path) {
                    Ok(text) => {
                        core.settings.palette = text.lines().filter_map(|l| parse_hex(l).map(hex_of)).take(32).collect();
                        self.notify(&ctx, format!("{}: {}", tx(en, "Geladen", "Loaded"), path.display()));
                    }
                    Err(_) => self.notify(&ctx, format!("{}: {}", tx(en, "Nicht gefunden", "Not found"), path.display())),
                }
            }
            if button(ui, tx(en, "Palette sichern", "Save palette"), 86.0, false).clicked() {
                let text: String = core.settings.palette.iter().map(|h| format!("{}\n", h.trim_start_matches('#').to_lowercase())).collect();
                match std::fs::write(&path, text) {
                    Ok(_) => self.notify(&ctx, format!("{}: {}", tx(en, "Gespeichert", "Saved"), path.display())),
                    Err(e) => self.notify(&ctx, format!("{}: {e}", tx(en, "Speichern fehlgeschlagen", "Saving failed"))),
                }
            }
        });

        ui.add_space(6.0);
        heading(ui, tx(en, "Neu", "New"));
        ui.horizontal(|ui| {
            if button(ui, tx(en, "Neues Item", "New item"), 86.0, false).clicked() {
                self.load_anim(Sprite::new(ITEM_N, ITEM_N));
            }
            if button(ui, tx(en, "Neuer Hut", "New hat"), 86.0, false).clicked() {
                self.load_anim(Sprite::new(HAT_W, HAT_H));
            }
        });
        if button(ui, tx(en, "Neue Kleidung", "New clothing"), 178.0, false).clicked() {
            self.load_anim(Sprite::new(CLOTH_W, CLOTH_H));
        }
        let bases: Vec<usize> = (0..core.slots.len())
            .filter(|&i| core.slots[i].sprite.is_item() || core.slots[i].sprite.is_hat() || core.slots[i].sprite.is_clothing())
            .collect();
        if !bases.is_empty() {
            let names: Vec<String> = bases
                .iter()
                .map(|&i| {
                    let kind = if core.slots[i].sprite.is_item() {
                        "Item"
                    } else if core.slots[i].sprite.is_clothing() {
                        tx(en, "Kleidung", "Clothing")
                    } else {
                        tx(en, "Hut", "Hat")
                    };
                    format!("{kind}: {}", core.slots[i].label(en))
                })
                .collect();
            let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
            self.template = self.template.min(bases.len() - 1);
            ui.horizontal(|ui| {
                combo(ui, "template", &mut self.template, &refs, 100.0);
                if button(ui, tx(en, "Laden", "Load"), 60.0, false).clicked() {
                    self.load_anim(core.slots[bases[self.template]].sprite.clone());
                }
            });
        }

        ui.add_space(6.0);
        heading(ui, tx(en, "Raster", "Canvas"));
        let before = self.size_idx;
        let labels = self.size_labels();
        row(ui, tx(en, "Größe", "Size"), |ui| {
            combo(ui, "grid_size", &mut self.size_idx, &labels, 100.0)
        });
        if self.size_idx != before {
            let (w, h) = SIZES[self.size_idx];
            self.load_anim(Sprite::new(w, h));
        }
        row(ui, tx(en, "Gitter", "Grid"), |ui| check(ui, &mut self.show_grid));
        row(ui, tx(en, "Symmetrie links/rechts", "Mirror left/right"), |ui| check(ui, &mut self.mirror));
        row(ui, tx(en, "Symmetrie oben/unten", "Mirror top/bottom"), |ui| check(ui, &mut self.mirror_v));
        row(ui, "Zoom", |ui| {
            if button(ui, "+", 28.0, false).clicked() {
                self.zoom = (self.zoom * 1.25).min(8.0);
            }
            ui.label(RichText::new(format!("{:.0} %", self.zoom * 100.0)).size(12.0).color(MUTED));
            if button(ui, "-", 28.0, false).clicked() {
                self.zoom = (self.zoom / 1.25).max(0.25);
            }
        });

        if self.sprite.is_hat() {
            ui.add_space(6.0);
            heading(ui, tx(en, "Hut-Höhe", "Hat height"));
            ui.horizontal(|ui| {
                if button(ui, tx(en, "Höher", "Higher"), 86.0, false).clicked() {
                    core.settings.hat_nudge = (core.settings.hat_nudge.round() - 1.0).max(-3.0);
                }
                if button(ui, tx(en, "Tiefer", "Lower"), 86.0, false).clicked() {
                    core.settings.hat_nudge = (core.settings.hat_nudge.round() + 1.0).min(3.0);
                }
            });
        }
        if self.sprite.is_clothing() {
            ui.add_space(6.0);
            heading(ui, tx(en, "Kleidung", "Clothing"));
            row(ui, tx(en, "Hinter der Krabbe", "Behind the crab"), |ui| check(ui, &mut self.back));
            ui.label(
                RichText::new(tx(
                    en,
                    "Male direkt auf die Krabbe. Vorne liegende Kleidung erscheint nur auf dem Körper (blasse Pixel daneben sieht man nicht) und passt sich jeder Pose an. Umhänge hinter der Krabbe dürfen überstehen.",
                    "Paint right onto the crab. Front clothing only shows on the body (faded pixels beside it stay hidden) and follows every pose. Capes behind the crab may stick out.",
                ))
                .size(12.0)
                .color(MUTED),
            );
        }
        if self.sprite.is_item() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(tx(
                    en,
                    "Der farbige Rahmen zeigt die Stelle, an der die Krabbe das Item greift.",
                    "The colored frame marks where the crab grips the item.",
                ))
                .size(12.0)
                .color(MUTED),
            );
        }

        ui.add_space(6.0);
        heading(ui, tx(en, "Aktionen", "Actions"));
        ui.horizontal(|ui| {
            if button(ui, tx(en, "Rückgängig", "Undo"), 86.0, false).clicked() {
                self.do_undo();
            }
            if button(ui, tx(en, "Wiederholen", "Redo"), 86.0, false).clicked() {
                self.do_redo();
            }
        });
        ui.horizontal(|ui| {
            if button(ui, tx(en, "Leeren", "Clear"), 86.0, false).clicked() {
                self.push_undo();
                self.sprite = Sprite::new(self.sprite.w, self.sprite.h);
            }
            if button(ui, tx(en, "Exportieren", "Export"), 86.0, false).clicked() {
                let s = self.assembled();
                if s.count() > 1 {
                    self.export_anim(&ctx, &s, &sys::export_dir());
                } else {
                    self.export(&ctx, s, "crab_sprite");
                }
            }
        });
        row(ui, tx(en, "Skalierung", "Scale"), |ui| {
            combo(ui, "export_scale", &mut self.export_idx, &SCALE_LABELS, 60.0)
        });

        ui.add_space(6.0);
        heading(ui, tx(en, "Verwenden", "Use"));
        row(ui, "Name", |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(90.0));
        });
        let use_label = if self.sprite.is_clothing() {
            tx(en, "Anziehen", "Put on")
        } else if self.sprite.is_hat() {
            tx(en, "Aufsetzen", "Wear")
        } else if self.sprite.is_item() {
            tx(en, "In die Hand", "Hold")
        } else {
            tx(en, "Als Buddy", "As buddy")
        };
        ui.horizontal(|ui| {
            if button(ui, tx(en, "Speichern", "Save"), 86.0, false).clicked() {
                self.save_current(&ctx, core);
            }
            if button(ui, use_label, 86.0, false).clicked() {
                self.apply_current(&ctx, core);
            }
        });
    }

    fn canvas(&mut self, ui: &mut Ui, cell: f32) {
        let view = self.shown_frame(ui.input(|i| i.time));
        let (w, h) = (view.w, view.h);
        let size = Vec2::new(cell * w as f32, cell * h as f32);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let n = self.frames.len();
        let ghosts: Vec<(&Sprite, u8)> = if self.onion && !self.playing && n > 1 {
            let mut g = vec![(&self.frames[(self.cur + n - 1) % n], 90u8)];
            if n > 2 {
                g.push((&self.frames[(self.cur + 1) % n], 45u8));
            }
            g
        } else {
            Vec::new()
        };
        let painter = ui.painter_at(rect);
        let light = Color32::from_rgb(54, 49, 45);
        let dark = Color32::from_rgb(45, 41, 38);

        for y in 0..h {
            for x in 0..w {
                let r = Rect::from_min_size(
                    rect.min + Vec2::new(x as f32 * cell, y as f32 * cell),
                    Vec2::splat(cell),
                );
                painter.rect_filled(r, 0.0, if (x + y) % 2 == 0 { light } else { dark });
                let body = view.is_clothing() && matches!(art::IDLE_STAND[y].as_bytes()[x], b'#' | b'E');
                if body {
                    let col = match art::IDLE_STAND[y].as_bytes()[x] {
                        b'E' => render::eye_color(false),
                        _ if (4..8).contains(&y) && (x < 4 || x >= 20) => render::claw_color(x),
                        _ => render::body_color(x),
                    };
                    painter.rect_filled(r, 0.0, col);
                }
                for (g, a) in &ghosts {
                    let q = g.get(x, y);
                    if q[3] > 0 {
                        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(q[0], q[1], q[2], *a));
                    }
                }
                let p = view.get(x, y);
                if p[3] > 0 {
                    let hidden = view.is_clothing() && !body && !self.back;
                    let c = Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]);
                    painter.rect_filled(r, 0.0, if hidden { c.gamma_multiply(0.35) } else { c });
                }
            }
        }

        if self.show_grid && cell >= 6.0 {
            let s = Stroke::new(1.0f32, Color32::from_white_alpha(10));
            for i in 0..=w {
                let o = i as f32 * cell;
                painter.line_segment(
                    [rect.min + Vec2::new(o, 0.0), rect.min + Vec2::new(o, size.y)],
                    s,
                );
            }
            for i in 0..=h {
                let o = i as f32 * cell;
                painter.line_segment(
                    [rect.min + Vec2::new(0.0, o), rect.min + Vec2::new(size.x, o)],
                    s,
                );
            }
        }
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0f32, line_col()));

        if self.sprite.is_item() {
            let g = Rect::from_min_size(
                rect.min + Vec2::new(GRIP.0 as f32 * cell, GRIP.1 as f32 * cell),
                Vec2::splat(cell),
            );
            painter.rect_stroke(g.shrink(1.0), Rounding::same(2.0), Stroke::new(2.0f32, accent()));
        }

        let (wi, hi) = (w as i32, h as i32);
        let to_cell = |pos: Pos2| -> (i32, i32) {
            (
                ((pos.x - rect.left()) / cell).floor() as i32,
                ((pos.y - rect.top()) / cell).floor() as i32,
            )
        };
        let inb = |x: i32, y: i32| x >= 0 && y >= 0 && x < wi && y < hi;

        self.hover_cell = resp.hover_pos().map(to_cell).filter(|&(x, y)| inb(x, y));
        if let Some((hx, hy)) = self.hover_cell {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            let cells = mirrored(hx, hy, wi, hi, self.mirror, self.mirror_v);
            for (i, (cx, cy)) in cells.into_iter().enumerate() {
                let r = Rect::from_min_size(
                    rect.min + Vec2::new(cx as f32 * cell, cy as f32 * cell),
                    Vec2::splat(cell),
                );
                let a = if i == 0 { 1.0 } else { 0.45 };
                painter.rect_stroke(r, 0.0, Stroke::new(1.5f32, TEXT.gamma_multiply(a)));
            }
        }

        if let Some((x0, y0, x1, y1)) = self.area {
            let r = Rect::from_min_max(
                rect.min + Vec2::new(x0 as f32 * cell, y0 as f32 * cell),
                rect.min + Vec2::new((x1 + 1) as f32 * cell, (y1 + 1) as f32 * cell),
            );
            painter.rect_stroke(r, 0.0, Stroke::new(2.0f32, accent()));
        }
        if resp.hovered() {
            let z = ui.input(|i| i.zoom_delta());
            if z != 1.0 {
                self.zoom = (self.zoom * z).clamp(0.25, 8.0);
            }
        }

        if self.playing {
            if resp.clicked() {
                self.playing = false;
            }
            return;
        }
        let secondary = resp.dragged_by(egui::PointerButton::Secondary)
            || resp.secondary_clicked();
        let start = resp.drag_started() || resp.clicked() || resp.secondary_clicked();
        let selecting = self.tool == Tool::Select && !secondary;
        if start && !selecting {
            self.push_undo();
            self.last = None;
            self.anchor = None;
        }
        if start || resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let cur = to_cell(pos);
                let tool = if secondary { Tool::Eraser } else { self.tool };
                match tool {
                    Tool::Pencil | Tool::Eraser => {
                        let c = if tool == Tool::Pencil {
                            self.color.to_srgba_unmultiplied()
                        } else {
                            [0, 0, 0, 0]
                        };
                        let from = self.last.unwrap_or(cur);
                        for (x, y) in line_pts(from, cur) {
                            self.plot(x, y, c);
                        }
                        self.last = Some(cur);
                    }
                    Tool::Fill if start && inb(cur.0, cur.1) => {
                        let c = self.color.to_srgba_unmultiplied();
                        self.sprite.flood(cur.0 as usize, cur.1 as usize, c);
                    }
                    Tool::Line | Tool::Rect | Tool::Circle => {
                        let a = *self.anchor.get_or_insert(cur);
                        if let Some(base) = self.undo.last() {
                            self.sprite = base.clone();
                        }
                        let c = self.color.to_srgba_unmultiplied();
                        for (x, y) in shape_pts(tool, a, cur) {
                            self.plot(x, y, c);
                        }
                    }
                    Tool::Select => self.select_drag(start, cur),
                    Tool::Picker if start && inb(cur.0, cur.1) => {
                        let p = self.sprite.get(cur.0 as usize, cur.1 as usize);
                        if p[3] > 0 {
                            self.color = Color32::from_rgb(p[0], p[1], p[2]);
                            self.tool = Tool::Pencil;
                        }
                    }
                    _ => {}
                }
            }
        }
        if resp.drag_stopped() || resp.clicked() || resp.secondary_clicked() {
            self.last = None;
            self.anchor = None;
            self.moving = None;
        }
    }

    fn select_drag(&mut self, start: bool, cur: (i32, i32)) {
        let inside = |r: Area| cur.0 >= r.0 && cur.0 <= r.2 && cur.1 >= r.1 && cur.1 <= r.3;
        if start {
            match self.area.filter(|&r| inside(r)) {
                Some(r) => {
                    self.push_undo();
                    let mut px = Vec::new();
                    for y in r.1..=r.3 {
                        for x in r.0..=r.2 {
                            px.push((x, y, self.sprite.get(x as usize, y as usize)));
                        }
                    }
                    self.moving = Some((cur, px, r));
                }
                None => {
                    self.moving = None;
                    self.anchor = Some(cur);
                    self.area = None;
                }
            }
            return;
        }
        if let Some((from, px, r)) = &self.moving {
            let (dx, dy) = (cur.0 - from.0, cur.1 - from.1);
            let Some(base) = self.undo.last() else { return };
            let mut s = base.clone();
            for &(x, y, _) in px.iter() {
                s.set(x as usize, y as usize, [0, 0, 0, 0]);
            }
            for &(x, y, c) in px.iter() {
                let (nx, ny) = (x + dx, y + dy);
                if c[3] > 0 && nx >= 0 && ny >= 0 && nx < s.w as i32 && ny < s.h as i32 {
                    s.set(nx as usize, ny as usize, c);
                }
            }
            self.area = Some((r.0 + dx, r.1 + dy, r.2 + dx, r.3 + dy));
            self.sprite = s;
        } else if let Some(a) = self.anchor {
            let (w, h) = (self.sprite.w as i32 - 1, self.sprite.h as i32 - 1);
            let c = (cur.0.clamp(0, w), cur.1.clamp(0, h));
            let a = (a.0.clamp(0, w), a.1.clamp(0, h));
            self.area = Some((a.0.min(c.0), a.1.min(c.1), a.0.max(c.0), a.1.max(c.1)));
        }
    }

    fn region(&self) -> Area {
        let (w, h) = (self.sprite.w as i32, self.sprite.h as i32);
        self.area
            .map(|r| (r.0.max(0), r.1.max(0), r.2.min(w - 1), r.3.min(h - 1)))
            .filter(|r| r.0 <= r.2 && r.1 <= r.3)
            .unwrap_or((0, 0, w - 1, h - 1))
    }

    fn clear_area(&mut self) {
        if self.area.is_none() {
            return;
        }
        self.push_undo();
        let r = self.region();
        for y in r.1..=r.3 {
            for x in r.0..=r.2 {
                self.sprite.set(x as usize, y as usize, [0, 0, 0, 0]);
            }
        }
    }

    fn transform(&mut self, op: u8) {
        self.push_undo();
        let r = self.region();
        let src = self.sprite.clone();
        let (rw, rh) = (r.2 - r.0 + 1, r.3 - r.1 + 1);
        let (ox, oy) = if op == 2 { (r.0 + (rw - rh) / 2, r.1 + (rh - rw) / 2) } else { (r.0, r.1) };
        for y in r.1..=r.3 {
            for x in r.0..=r.2 {
                self.sprite.set(x as usize, y as usize, [0, 0, 0, 0]);
            }
        }
        let (w, h) = (self.sprite.w as i32, self.sprite.h as i32);
        for y in 0..rh {
            for x in 0..rw {
                let p = src.get((r.0 + x) as usize, (r.1 + y) as usize);
                let (nx, ny) = match op {
                    0 => (rw - 1 - x, y),
                    1 => (x, rh - 1 - y),
                    _ => (rh - 1 - y, x),
                };
                let (tx, ty) = (ox + nx, oy + ny);
                if p[3] > 0 && tx >= 0 && ty >= 0 && tx < w && ty < h {
                    self.sprite.set(tx as usize, ty as usize, p);
                }
            }
        }
        if op == 2 && self.area.is_some() {
            self.area = Some((ox, oy, ox + rh - 1, oy + rw - 1));
        }
    }

    fn crab_preview(&self, ui: &mut Ui, core: &Core, size: Vec2) {
        let t = ui.input(|i| i.time);
        let cur = self.shown_frame(t);
        let frame: &crate::art::Frame = match self.pose {
            1 => art::WALK[((t * 10.0) as usize) % 8],
            2 => if ((t * 3.0) as usize) % 2 == 0 { &art::CHEER } else { &art::IDLE_STAND },
            _ => if (t * 0.9).fract() > 0.93 { &art::IDLE_BLINK } else { &art::IDLE_STAND },
        };
        let head = frame.iter().position(|r| r.contains('#')).unwrap_or(0) as f32 / 2.0;
        let grip = crate::pet::hand_row(frame, 11).unwrap_or(2.0) - 2.0;
        let pose = self.pose;
        ui.ctx().request_repaint_after(next_change(
            |t| match pose {
                1 => ((t * 10.0) as usize) % 8,
                2 => ((t * 3.0) as usize) % 2,
                _ => usize::from((t * 0.9).fract() > 0.93),
            },
            t,
        ));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, Rounding::same(6.0), row_dark_col());
        p.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0f32, line_col()));
        let item = self.sprite.is_item();
        let blk = if item { 6.0 } else { 8.0 };
        let brush = Brush::painter(p, rect.min, ui.ctx().pixels_per_point());
        let x = if item {
            14.0
        } else {
            ((size.x - 12.0 * blk) / 2.0).floor()
        };
        let y = size.y - 8.0 * blk - 16.0;
        render::draw_shadow(&brush, x + blk, y + 8.0 * blk - 4.0, 10.0 * blk, 7.0);
        if cur.is_clothing() {
            let mut layers = core.outfit_layers();
            layers.push((cur.clone(), self.back));
            render::set_outfit(layers);
        }
        render::draw_crab(&brush, x, y, blk, frame, 0.0);
        render::set_outfit(core.outfit_layers());
        let nudge = (core.settings.hat_nudge.round() + head) * blk;
        if cur.is_clothing() {
            if let Some(h) = core.hat_sprite() {
                render::draw_hat(&brush, h, x, y, blk, nudge + core.hat_drop() * blk);
            }
        } else if item {
            if let Some(h) = core.hat_sprite() {
                render::draw_hat(&brush, h, x, y, blk, nudge + core.hat_drop() * blk);
            }
            render::draw_item(&brush, &cur, x, y, blk, ItemPose { dy: grip, ..ItemPose::default() });
        } else {
            let drop = core
                .find_same(&self.sprite)
                .filter(|&i| core.slots[i].builtin)
                .map(|i| art::hat_drop(&core.slots[i].name))
                .unwrap_or(0.0);
            render::draw_hat(&brush, &cur, x, y, blk, nudge + drop * blk);
        }
    }

    fn pixel_view(&mut self, ui: &mut Ui, core: &Core) {
        let en = self.en;
        let avail = ui.available_size();
        let (w, h) = (self.sprite.w as f32, self.sprite.h as f32);
        let cw = (avail.x - 270.0).max(120.0);
        let ch = (avail.y - if self.sprite.is_item() { 300.0 } else { 40.0 }).max(96.0);
        let cell = ((cw / w).min(ch / h) * self.zoom).floor().clamp(2.0, 120.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(cw);
                egui::ScrollArea::both().id_salt("canvas_scroll").max_width(cw).max_height(ch).show(ui, |ui| self.canvas(ui, cell));
                if self.sprite.is_item() {
                    ui.add_space(8.0);
                    self.timeline(ui);
                }
                ui.add_space(6.0);
                let txt = match self.hover_cell {
                    Some((x, y)) => format!("{}, {}", x + 1, y + 1),
                    None => "-".to_string(),
                };
                ui.label(RichText::new(txt).size(12.0).color(MUTED));
                ui.label(
                    RichText::new(tx(
                        en,
                        "Linksklick: Malen, Rechtsklick: Radieren",
                        "Left click: draw, right click: erase",
                    ))
                    .size(12.0)
                    .color(MUTED),
                );
            });
            ui.add_space(14.0);
            ui.vertical(|ui| {
                heading(ui, tx(en, "Vorschau", "Preview"));
                if self.sprite.is_hat() || self.sprite.is_item() || self.sprite.is_clothing() {
                    self.crab_preview(ui, core, Vec2::new(190.0, 120.0));
                    combo(ui, "preview_pose", &mut self.pose, &[tx(en, "Stehen", "Idle"), tx(en, "Laufen", "Walk"), tx(en, "Jubeln", "Cheer")], 110.0);
                } else {
                    let n = self.sprite.w.max(self.sprite.h);
                    let px = ((128 / n).max(1) * n) as f32;
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(px + 16.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, Rounding::same(6.0), row_dark_col());
                    p.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0f32, line_col()));
                    p.image(
                        self.preview.id(),
                        rect.shrink(8.0),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("{} x {} px", self.sprite.w, self.sprite.h))
                        .size(12.0)
                        .color(MUTED),
                );
                ui.add_space(8.0);
                let (r1, _) = ui.allocate_exact_size(
                    Vec2::new(self.sprite.w as f32 + 12.0, self.sprite.h as f32 + 12.0),
                    Sense::hover(),
                );
                let p = ui.painter();
                p.rect_filled(r1, Rounding::same(4.0), row_dark_col());
                p.image(
                    self.preview.id(),
                    r1.shrink(6.0),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            });
        });
    }

    fn stage(&self, ui: &mut Ui, core: &Core) {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 160.0), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, Rounding::same(6.0), row_dark_col());
        p.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0f32, line_col()));
        let ground = rect.height() - 18.0;
        p.line_segment(
            [
                Pos2::new(rect.left() + 12.0, rect.top() + ground),
                Pos2::new(rect.right() - 12.0, rect.top() + ground),
            ],
            Stroke::new(1.0f32, line_col()),
        );
        let s = &core.settings;
        let t = ui.ctx().input(|i| i.time) as f32 * s.speed;
        let blk = (5.0 * s.scale).round().max(2.0);
        let (mx, my, sq) = render::idle_motion(s.idle, t);
        let crab_w = 12.0 * blk;
        let buddy = core.buddy_sprite().is_some();
        let body_h = if buddy { 12.0 * blk } else { 8.0 * blk };
        let right = s.anchor == 0 || s.anchor == 2;
        let bottom = s.anchor == 0 || s.anchor == 1;
        let base_x = if right {
            rect.width() - 28.0 - crab_w - 9.0 * blk
        } else {
            28.0
        };
        let base_y = if bottom {
            ground - body_h
        } else {
            14.0 + 6.0 * blk
        };
        let x = base_x + mx * blk;
        let y = base_y + my * blk;
        let brush = Brush::painter(p, rect.min, ui.ctx().pixels_per_point());
        render::draw_shadow(&brush, x + blk, y + body_h - 3.0, crab_w - 2.0 * blk, 6.0);
        match core.buddy_sprite() {
            Some(tex) => render::draw_buddy(&brush, tex, x, y, crab_w, false, sq * blk),
            None => {
                render::draw_crab(&brush, x, y, blk, &art::IDLE_STAND, sq * blk);
                if s.hat_visible {
                    if let Some(h) = core.hat_sprite() {
                        let extra = (s.hat_nudge.round() + core.hat_drop()) * blk + sq * blk;
                        render::draw_hat(&brush, h, x, y, blk, extra);
                    }
                }
                if s.item_visible {
                    if let Some(it) = core.item_sprite() {
                        render::draw_item_of(&brush, core.item_kind(), it, x, y, blk, ItemPose::default());
                    }
                }
            }
        }
        let (idle, speed) = (s.idle, s.speed);
        let wait = next_change(|t| render::idle_motion(idle, t as f32 * speed), ui.ctx().input(|i| i.time));
        ui.ctx().request_repaint_after(core.item_next_frame().map_or(wait, |d| wait.min(d)));
    }

    fn clothes_view(&mut self, ui: &mut Ui, core: &mut Core) {
        let ctx = ui.ctx().clone();
        let en = self.en;
        ui.add_space(6.0);
        heading(ui, tx(en, "Kleidung (oberste Ebene zuerst)", "Clothing (top layer first)"));
        let slots = &core.slots;
        let label = |name: &str| slots.iter().find(|x| x.name == name).map_or(name.to_string(), |x| x.label(en).to_string());
        let s = &mut core.settings;
        let n = s.clothes.len();
        let mut swap: Option<(usize, usize)> = None;
        let mut remove = None;
        for k in (0..n).rev() {
            let name = s.clothes[k].clone();
            let mut shown = !s.clothes_hidden.contains(&name);
            let mut locked = s.clothes_locked.contains(&name);
            row(ui, &label(&name), |ui| {
                if !locked && button(ui, "x", 24.0, false).clicked() {
                    remove = Some(k);
                }
                ui.label(RichText::new(tx(en, "Sperren", "Lock")).size(12.0).color(MUTED));
                check(ui, &mut locked);
                ui.label(RichText::new(tx(en, "Sichtbar", "Visible")).size(12.0).color(MUTED));
                check(ui, &mut shown);
                if !locked && k > 0 && button(ui, tx(en, "Runter", "Down"), 64.0, false).clicked() {
                    swap = Some((k, k - 1));
                }
                if !locked && k + 1 < n && button(ui, tx(en, "Hoch", "Up"), 64.0, false).clicked() {
                    swap = Some((k, k + 1));
                }
            });
            s.clothes_hidden.retain(|h| *h != name);
            if !shown {
                s.clothes_hidden.push(name.clone());
            }
            s.clothes_locked.retain(|h| *h != name);
            if locked {
                s.clothes_locked.push(name);
            }
        }
        if let Some((a, b)) = swap {
            s.clothes.swap(a, b);
        }
        if let Some(k) = remove {
            s.clothes.remove(k);
        }
        let free: Vec<String> = core
            .slots
            .iter()
            .filter(|sl| sl.sprite.is_clothing() && !core.settings.clothes.contains(&sl.name))
            .map(|sl| sl.name.clone())
            .collect();
        if !free.is_empty() {
            let shown: Vec<String> = free.iter().map(|n| core.slots.iter().find(|x| x.name == *n).map_or(n.clone(), |x| x.label(en).to_string())).collect();
            let labels: Vec<&str> = shown.iter().map(|x| x.as_str()).collect();
            self.wear_pick = self.wear_pick.min(free.len() - 1);
            row(ui, tx(en, "Anziehen", "Put on"), |ui| {
                if button(ui, tx(en, "Anziehen", "Put on"), 86.0, false).clicked() {
                    core.settings.clothes.push(free[self.wear_pick].clone());
                }
                combo(ui, "wear_pick", &mut self.wear_pick, &labels, 160.0);
            });
        } else if n == 0 {
            ui.label(RichText::new(tx(en, "Zeichne Kleidung im Pixel-Art-Tab (Größe Kleidung 24 x 16).", "Draw clothing in the Pixel Art tab (size Clothing 24 x 16).")).size(12.0).color(MUTED));
        }

        ui.add_space(6.0);
        heading(ui, "Outfits");
        let mut wear = None;
        let mut drop = None;
        for (k, o) in core.settings.outfits.iter().enumerate() {
            let parts = o.clothes.len() + usize::from(!o.hat.is_empty()) + usize::from(!o.item.is_empty());
            row(ui, &o.name, |ui| {
                if button(ui, "x", 24.0, false).clicked() {
                    drop = Some(k);
                }
                if button(ui, tx(en, "Anziehen", "Wear"), 86.0, false).clicked() {
                    wear = Some(k);
                }
                ui.label(RichText::new(format!("{parts} {}", tx(en, "Teile", "parts"))).size(12.0).color(MUTED));
            });
        }
        if let Some(k) = wear {
            let o = core.settings.outfits[k].clone();
            core.wear_outfit(&o);
            self.notify(&ctx, format!("{}: {}", tx(en, "Outfit an", "Outfit on"), o.name));
        }
        if let Some(k) = drop {
            core.settings.outfits.remove(k);
        }
        row(ui, tx(en, "Aktuelles Outfit", "Current outfit"), |ui| {
            if button(ui, tx(en, "Speichern", "Save"), 86.0, false).clicked() {
                let name = self.outfit_name.trim().to_string();
                let name = if name.is_empty() { format!("Outfit {}", core.settings.outfits.len() + 1) } else { name };
                core.settings.outfits.retain(|o| o.name != name);
                let o = core.current_outfit(name);
                core.settings.outfits.push(o);
                self.outfit_name.clear();
                self.notify(&ctx, tx(en, "Outfit gespeichert", "Outfit saved"));
            }
            ui.add(egui::TextEdit::singleline(&mut self.outfit_name).desired_width(120.0).hint_text("Name"));
        });
    }

    fn crab_view(&mut self, ui: &mut Ui, core: &mut Core) {
        let en = self.en;
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 6.0);
        heading(ui, tx(en, "Vorschau", "Preview"));
        self.stage(ui, core);
        ui.add_space(6.0);
        ui.columns(2, |cols| {
            let (left, right) = cols.split_at_mut(1);
            {
                let ui = &mut left[0];
                let s = &mut core.settings;
                heading(ui, tx(en, "Krabbenfarbe", "Crab color"));
            row(ui, tx(en, "Vorlage", "Preset"), |ui| {
                for (name, body, claw, eye, rainbow) in CRAB_PRESETS.iter().rev() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                    let p = ui.painter();
                    if *rainbow {
                        for k in 0..6 {
                            let a = k as f32 / 6.0 * std::f32::consts::TAU;
                            let c: Color32 = egui::ecolor::Hsva::new(k as f32 / 6.0, 0.62, 0.95, 1.0).into();
                            p.circle_filled(rect.center() + Vec2::angled(a) * 3.5, 3.5, c);
                        }
                    } else {
                        p.circle_filled(rect.center(), 7.0, parse_hex(body).unwrap_or(art::CRAB));
                        p.circle_filled(rect.center() + Vec2::new(4.0, 3.0), 3.0, parse_hex(claw).unwrap_or(parse_hex(body).unwrap_or(art::CRAB)));
                    }
                    let on = s.crab_rainbow == *rainbow && (*rainbow || (s.crab_body == *body && s.crab_claw == *claw));
                    if on || resp.hovered() {
                        p.circle_stroke(rect.center(), 9.5, Stroke::new(1.5f32, if on { TEXT } else { MUTED }));
                    }
                    let resp = resp.on_hover_text(*name);
                    if resp.clicked() {
                        s.crab_body = body.to_string();
                        s.crab_claw = claw.to_string();
                        s.crab_eye = eye.to_string();
                        s.crab_rainbow = *rainbow;
                    }
                }
            });
            row(ui, tx(en, "Körper", "Body"), |ui| {
                color_field(ui, &mut s.crab_body, art::CRAB);
                if !s.crab_body.is_empty() {
                    s.crab_rainbow = false;
                }
            });
            row(ui, tx(en, "Scheren", "Claws"), |ui| color_field(ui, &mut s.crab_claw, parse_hex(&s.crab_body).unwrap_or(art::CRAB)));
            row(ui, tx(en, "Augen", "Eyes"), |ui| color_field(ui, &mut s.crab_eye, art::EYE));
            }
            Self::hat_item(&mut right[0], core, en);
        });
        self.clothes_view(ui, core);
    }

    fn hat_item(ui: &mut Ui, core: &mut Core, en: bool) {
        heading(ui, tx(en, "Hut", "Hat"));
        let hat_ids: Vec<usize> = core
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.sprite.is_hat())
            .map(|(i, _)| i)
            .collect();
        let mut names: Vec<String> = vec![tx(en, "Kein Hut", "No hat").to_string()];
        names.extend(hat_ids.iter().map(|&i| core.slots[i].label(en).to_string()));
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let mut pick = core
            .hat
            .and_then(|h| hat_ids.iter().position(|&i| i == h))
            .map(|p| p + 1)
            .unwrap_or(0);
        let pick_before = pick;
        row(ui, tx(en, "Vorlage", "Preset"), |ui| combo(ui, "hat_pick", &mut pick, &refs, 160.0));
        if pick != pick_before {
            core.hat = if pick == 0 {
                None
            } else {
                core.settings.hat_visible = true;
                Some(hat_ids[pick - 1])
            };
        }
        row(ui, tx(en, "Hut tragen", "Wear hat"), |ui| {
            check(ui, &mut core.settings.hat_visible)
        });
        row(ui, tx(en, "Hut-Höhe", "Hat height"), |ui| {
            slim(ui, &mut core.settings.hat_nudge, -3.0..=3.0, 140.0, |v| {
                format!("{:+.0}", v.round())
            })
        });

        ui.add_space(6.0);
        heading(ui, tx(en, "Item in der Hand", "Held item"));
        let item_ids: Vec<usize> = core
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.sprite.is_item())
            .map(|(i, _)| i)
            .collect();
        let mut inames: Vec<String> = vec![tx(en, "Kein Item", "No item").to_string()];
        inames.extend(item_ids.iter().map(|&i| core.slots[i].label(en).to_string()));
        let irefs: Vec<&str> = inames.iter().map(|s| s.as_str()).collect();
        let mut ipick = core
            .item
            .and_then(|h| item_ids.iter().position(|&i| i == h))
            .map(|p| p + 1)
            .unwrap_or(0);
        let ipick_before = ipick;
        row(ui, "Item", |ui| combo(ui, "item_pick", &mut ipick, &irefs, 160.0));
        if ipick != ipick_before {
            core.item = if ipick == 0 {
                None
            } else {
                core.settings.item_visible = true;
                Some(item_ids[ipick - 1])
            };
        }
        row(ui, tx(en, "Item halten", "Hold item"), |ui| {
            check(ui, &mut core.settings.item_visible)
        });
    }

    fn behavior_view(&mut self, ui: &mut Ui, core: &mut Core, pet: &mut PetRemote) {
        let ctx = ui.ctx().clone();
        let en = self.en;
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 6.0);
        ui.columns(2, |cols| {
            let s = &mut core.settings;
            heading(&mut cols[0], tx(en, "Verhalten", "Behavior"));
            row(&mut cols[0], "Name", |ui| {
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(120.0));
            });
            row(&mut cols[0], tx(en, "Leerlauf", "Idle"), |ui| {
                combo(
                    ui,
                    "idle",
                    &mut s.idle,
                    &[
                        tx(en, "Atmen", "Breathe"),
                        tx(en, "Wippen", "Sway"),
                        tx(en, "Hüpfen", "Hop"),
                        tx(en, "Keine", "None"),
                    ],
                    110.0,
                )
            });
            row(&mut cols[0], tx(en, "Position", "Position"), |ui| {
                combo(
                    ui,
                    "anchor",
                    &mut s.anchor,
                    &[
                        tx(en, "Unten rechts", "Bottom right"),
                        tx(en, "Unten links", "Bottom left"),
                        tx(en, "Oben rechts", "Top right"),
                        tx(en, "Oben links", "Top left"),
                    ],
                    110.0,
                )
            });
            row(&mut cols[0], tx(en, "Größe", "Size"), |ui| {
                slim(ui, &mut s.scale, 0.5..=1.6, 140.0, |v| format!("{:.1}x", v))
            });
            row(&mut cols[0], tx(en, "Tempo", "Speed"), |ui| {
                slim(ui, &mut s.speed, 0.2..=3.0, 140.0, |v| format!("{:.1}x", v))
            });
            heading(&mut cols[1], tx(en, "Fenster & Maus", "Window & mouse"));
            row(&mut cols[1], tx(en, "Immer im Vordergrund", "Always on top"), |ui| {
                check(ui, &mut s.on_top)
            });
            row(&mut cols[1], tx(en, "Folgt dem Cursor", "Follows the cursor"), |ui| {
                check(ui, &mut s.follow)
            });
            row(&mut cols[1], tx(en, "Lasso-Rodeo", "Lasso rodeo"), |ui| {
                check(ui, &mut s.lasso)
            });
            row(&mut cols[1], tx(en, "Klicks durchlassen (Strg = anklickbar)", "Click-through (Ctrl = clickable)"), |ui| {
                check(ui, &mut s.click_through)
            });
        });
        ui.add_space(6.0);
        heading(ui, tx(en, "Aktionen", "Actions"));
        ui.horizontal_wrapped(|ui| {
            if core.settings.lasso
                && button(ui, tx(en, "Lasso-Rodeo", "Lasso rodeo"), 128.0, false).clicked()
            {
                pet.trigger_cowboy(core);
            }
            if button(ui, tx(en, "Umschauen", "Look around"), 128.0, false).clicked() {
                pet.trigger_look_around();
            }
            if button(ui, tx(en, "Item-Trick", "Item trick"), 128.0, false).clicked()
                && !pet.trigger_play(core)
            {
                self.notify(&ctx, tx(en, "Erst ein Item in die Hand geben", "Give the crab an item first"));
            }
            let sleep = if pet.is_sleeping() {
                tx(en, "Aufwachen", "Wake up")
            } else {
                tx(en, "Schlafen", "Sleep")
            };
            if button(ui, sleep, 128.0, pet.is_sleeping()).clicked() {
                pet.toggle_sleep(core);
            }
            let pause = if pet.is_paused() {
                tx(en, "Fortsetzen", "Resume")
            } else {
                tx(en, "Pausieren", "Pause")
            };
            if button(ui, pause, 128.0, pet.is_paused()).clicked() {
                pet.toggle_pause();
            }
            if button(ui, tx(en, "Beenden", "Quit"), 128.0, false).clicked() {
                pet.quit();
            }
        });

    }

    fn claude_view(&mut self, ui: &mut Ui, core: &mut Core, pet: &mut PetRemote) {
        let ctx = ui.ctx().clone();
        let en = self.en;
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 6.0);
        heading(ui, "Claude Code");
        let mut on = self.cc_on;
        row(ui, tx(en, "Mit Claude Code verbinden", "Connect to Claude Code"), |ui| check(ui, &mut on));
        if on != self.cc_on {
            match crate::claude::connect(on) {
                Ok(()) => {
                    self.cc_on = on;
                    self.notify(
                        &ctx,
                        if on {
                            tx(en, "Hooks eingetragen", "Hooks installed")
                        } else {
                            tx(en, "Hooks entfernt", "Hooks removed")
                        },
                    );
                }
                Err(e) => self.notify(&ctx, e),
            }
        }
        let path = crate::claude::settings_path().map(|p| p.display().to_string()).unwrap_or_default();
        let cmd = crate::claude::our_command().unwrap_or_default();
        ui.label(
            RichText::new(format!(
                "{} {path}\n{}: {}\n{} \"{cmd}\" (async, timeout 5 s)\n{}",
                tx(en, "Schreibt in", "Writes to"),
                tx(en, "Ereignisse", "Events"),
                crate::claude::EVENTS.join(", "),
                tx(en, "Befehl:", "Command:"),
                tx(
                    en,
                    "Bestehende Hooks bleiben erhalten, vorher wird settings.json.crab-backup angelegt. Übertragen wird nur die Art der Tätigkeit, nie Prompt- oder Dateiinhalte.",
                    "Existing hooks are kept, settings.json.crab-backup is written first. Only the kind of activity is sent, never prompt or file contents.",
                ),
            ))
            .size(11.0)
            .color(MUTED),
        );
        ui.add_space(6.0);
        heading(ui, tx(en, "Testen", "Try it"));
        ui.horizontal_wrapped(|ui| {
            for (state, de, e) in [
                ("think", "Denken", "Thinking"),
                ("edit", "Code", "Code"),
                ("ask", "Frage", "Question"),
                ("fail", "Fehler", "Error"),
                ("done", "Fertig", "Done"),
            ] {
                if button(ui, tx(en, de, e), 96.0, false).clicked() {
                    pet.send(&format!("cc:{state}:test"));
                }
            }
        });

        let mut send = false;
        row(ui, tx(en, "Sprechblase", "Speech bubble"), |ui| {
            if button(ui, tx(en, "Senden", "Send"), 70.0, false).clicked() {
                send = true;
            }
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.speech)
                    .hint_text(tx(en, "Text eingeben", "Type a message"))
                    .desired_width(190.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                send = true;
            }
        });
        if send {
            let text = self.speech.trim().to_string();
            if !text.is_empty() {
                pet.set_speech(core, &text);
            }
        }

    }

    fn app_view(&mut self, ui: &mut Ui, core: &mut Core) {
        let ctx = ui.ctx().clone();
        let en = self.en;
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 6.0);
        let autostart_before = core.settings.autostart;
        ui.columns(2, |cols| {
            let s = &mut core.settings;
            heading(&mut cols[0], tx(en, "Darstellung", "Appearance"));
            row(&mut cols[0], tx(en, "Sprache", "Language"), |ui| {
                combo(ui, "lang", &mut s.lang, &["Deutsch", "English"], 110.0)
            });
            row(&mut cols[0], tx(en, "Design", "Design"), |ui| {
                let mut d = if s.glass { 0 } else if s.black { 2 } else { 1 };
                combo(ui, "design", &mut d, &["Apple Lover", tx(en, "Klassisch", "Classic"), tx(en, "Schwarz", "Black")], 120.0);
                s.glass = d == 0;
                s.black = d == 2;
            });
            row(&mut cols[0], tx(en, "Akzentfarbe", "Accent color"), |ui| accent_picker(ui, &mut s.accent, &mut s.accent_hex));
            heading(&mut cols[1], "System");
            row(&mut cols[1], tx(en, "Mit System starten", "Start with system"), |ui| {
                check(ui, &mut s.autostart)
            });
            row(&mut cols[1], tx(en, "Für OBS sichtbar", "Visible in OBS"), |ui| check(ui, &mut s.obs));
            row(&mut cols[1], tx(en, "Debug-Konsole", "Debug console"), |ui| {
                check(ui, &mut s.debug)
            });
        });
        if core.settings.autostart != autostart_before {
            let want = core.settings.autostart;
            match sys::set_autostart(want) {
                Ok(_) => self.notify(
                    &ctx,
                    if want {
                        tx(en, "Autostart aktiviert", "Autostart enabled")
                    } else {
                        tx(en, "Autostart deaktiviert", "Autostart disabled")
                    },
                ),
                Err(e) => {
                    core.settings.autostart = autostart_before;
                    self.notify(&ctx, e);
                }
            }
        }

        ui.add_space(10.0);
        heading(ui, tx(en, "Einstellungen", "Settings"));
        ui.horizontal(|ui| {
            if button(ui, tx(en, "Einstellungen speichern", "Save settings"), 170.0, false).clicked() {
                match core.save_settings().and_then(|_| core.save_library()) {
                    Ok(_) => {
                        self.saved = core.settings.clone();
                        self.saved_sel = (core.hat, core.buddy, core.item);
                        core.dirty_lib = false;
                        self.notify(&ctx, tx(en, "Gespeichert", "Saved"));
                    }
                    Err(e) => self.notify(
                        &ctx,
                        format!("{}: {e}", tx(en, "Speichern fehlgeschlagen", "Saving failed")),
                    ),
                }
            }
            if button(ui, tx(en, "Standard", "Defaults"), 100.0, false).clicked() {
                let keep = (
                    core.settings.name.clone(),
                    core.settings.lang,
                    core.settings.hidden.clone(),
                );
                core.settings = Settings::default();
                core.settings.name = keep.0;
                core.settings.lang = keep.1;
                core.settings.hidden = keep.2;
                core.settings.autostart = autostart_before;
                self.notify(&ctx, tx(en, "Einstellungen zurückgesetzt", "Settings reset"));
            }
        });
    }

    fn library_view(&mut self, ui: &mut Ui, core: &mut Core) {
        let ctx = ui.ctx().clone();
        let en = self.en;
        heading(ui, "Sprites");
        let mut add = false;
        let mut restore = false;
        let mut act: Option<LibAct> = None;
        let mut clicked: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(10.0);
            for i in 0..core.slots.len() {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(112.0, 132.0), Sense::click());
                let selected = self.sel == Some(i);
                let t = ctx.animate_bool_with_time(resp.id, selected, 0.15);
                let h = ctx.animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.12);
                let p = ui.painter();
                p.rect_filled(rect, Rounding::same(6.0), row_dark_col());
                p.rect_stroke(
                    rect,
                    Rounding::same(6.0),
                    Stroke::new(1.0 + t, mix(line_col(), accent(), t.max(h * 0.5))),
                );
                let sp = &core.slots[i].sprite;
                let [sw, sh] = core.slots[i].tex.as_ref().map_or([sp.w, sp.h], |t| t.size());
                let k = (88.0 / sw as f32).min(88.0 / sh as f32);
                let k = if k >= 2.0 { k.floor() } else { k };
                let inner = Rect::from_center_size(
                    rect.center() + Vec2::new(0.0, -10.0),
                    Vec2::new(sw as f32 * k, sh as f32 * k),
                );
                p.image(
                    core.slots[i].tex.as_ref().map(|t| t.id()).unwrap_or_default(),
                    inner,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                if sp.count() > 1 {
                    p.text(rect.left_top() + Vec2::new(8.0, 6.0), Align2::LEFT_TOP, format!("{} Frames", sp.count()), FontId::proportional(10.5), MUTED);
                }
                if core.buddy == Some(i) || core.hat == Some(i) || core.item == Some(i) {
                    p.rect_filled(
                        Rect::from_min_size(
                            rect.right_top() + Vec2::new(-16.0, 6.0),
                            Vec2::splat(8.0),
                        ),
                        Rounding::same(2.0),
                        accent(),
                    );
                }
                p.text(
                    rect.center_bottom() + Vec2::new(0.0, -12.0),
                    Align2::CENTER_CENTER,
                    core.slots[i].label(en),
                    FontId::proportional(11.5),
                    mix(MUTED, TEXT, t.max(h)),
                );
                if resp.clicked() {
                    clicked = Some(i);
                }
            }

            let (rect, resp) = ui.allocate_exact_size(Vec2::new(112.0, 132.0), Sense::click());
            let h = ctx.animate_bool_with_time(resp.id, resp.hovered(), 0.12);
            let col = mix(MUTED, accent(), h);
            let p = ui.painter();
            p.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0f32, mix(line_col(), accent(), h)));
            let c = rect.center() + Vec2::new(0.0, -10.0);
            let st = Stroke::new(2.0f32, col);
            p.line_segment([c + Vec2::new(-12.0, 0.0), c + Vec2::new(12.0, 0.0)], st);
            p.line_segment([c + Vec2::new(0.0, -12.0), c + Vec2::new(0.0, 12.0)], st);
            p.text(
                rect.center_bottom() + Vec2::new(0.0, -12.0),
                Align2::CENTER_CENTER,
                tx(en, "Aktuelle Zeichnung", "Current drawing"),
                FontId::proportional(11.5),
                col,
            );
            if resp.clicked() {
                add = true;
            }
        });

        if let Some(i) = clicked {
            self.sel = Some(i);
            self.rename = core.slots[i].name.clone();
        }

        ui.add_space(4.0);
        ui.label(
            RichText::new(tx(
                en,
                "Der farbige Punkt markiert Hut, Item und Buddy, die gerade aktiv sind.",
                "The colored dot marks the hat, item and buddy that are currently active.",
            ))
            .size(12.0)
            .color(MUTED),
        );

        let missing = core.missing_defaults();
        if missing > 0 {
            ui.add_space(6.0);
            let label = format!(
                "{} ({missing})",
                tx(en, "Vorlagen wiederherstellen", "Restore presets")
            );
            if button(ui, &label, 230.0, false).clicked() {
                restore = true;
            }
        }

        if add {
            if is_blank(&self.sprite) {
                self.notify(&ctx, tx(en, "Die Zeichnung ist leer", "The drawing is empty"));
            } else {
                let idx = core.add_slot(&ctx, None, self.assembled());
                self.sel = Some(idx);
                self.rename = core.slots[idx].name.clone();
            }
        }

        if restore {
            let n = core.restore_defaults(&ctx);
            self.sel = None;
            self.notify(
                &ctx,
                format!("{n} {}", tx(en, "Vorlagen wiederhergestellt", "presets restored")),
            );
        }

        if let Some(i) = self.sel.filter(|&i| i < core.slots.len()) {
            ui.add_space(14.0);
            heading(ui, tx(en, "Auswahl", "Selection"));
            let kind_hat = core.slots[i].sprite.is_hat();
            let kind_item = core.slots[i].sprite.is_item();
            let kind_cloth = core.slots[i].sprite.is_clothing();
            let in_use = if kind_cloth {
                core.settings.clothes.contains(&core.slots[i].name)
            } else if kind_hat {
                core.hat == Some(i)
            } else if kind_item {
                core.item == Some(i)
            } else {
                core.buddy == Some(i)
            };
            let use_label = match (kind_hat || kind_cloth, kind_item, in_use) {
                (true, _, true) if kind_cloth => tx(en, "Ausziehen", "Take off"),
                (true, _, false) if kind_cloth => tx(en, "Anziehen", "Put on"),
                (true, _, true) => tx(en, "Ablegen", "Take off"),
                (true, _, false) => tx(en, "Aufsetzen", "Wear"),
                (_, true, true) => tx(en, "Weglegen", "Put away"),
                (_, true, false) => tx(en, "In die Hand", "Hold"),
                (_, _, true) => tx(en, "Buddy lösen", "Unset buddy"),
                (_, _, false) => tx(en, "Als Buddy", "As buddy"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                if button(ui, tx(en, "Bearbeiten", "Edit"), 108.0, false).clicked() {
                    act = Some(LibAct::Edit);
                }
                if button(ui, tx(en, "Überschreiben", "Overwrite"), 108.0, false).clicked() {
                    act = Some(LibAct::Overwrite);
                }
                if button(ui, use_label, 108.0, in_use).clicked() {
                    act = Some(LibAct::Use);
                }
                if button(ui, tx(en, "Exportieren", "Export"), 108.0, false).clicked() {
                    act = Some(LibAct::Export);
                }
                if button(ui, tx(en, "Löschen", "Delete"), 108.0, false).clicked() {
                    act = Some(LibAct::Delete);
                }
            });
            ui.add_space(4.0);
            if !core.slots[i].builtin {
                row(ui, "Name", |ui| {
                    if button(ui, tx(en, "Umbenennen", "Rename"), 96.0, false).clicked() {
                        act = Some(LibAct::Rename);
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.rename).desired_width(160.0));
                });
            }
            row(ui, tx(en, "Skalierung beim Export", "Export scale"), |ui| {
                combo(ui, "lib_scale", &mut self.export_idx, &SCALE_LABELS, 60.0)
            });
        }

        if let (Some(a), Some(i)) = (act, self.sel) {
            if i < core.slots.len() {
                self.lib_action(&ctx, core, a, i);
            }
        }
    }

    fn lib_action(&mut self, ctx: &Context, core: &mut Core, a: LibAct, i: usize) {
        let en = self.en;
        match a {
            LibAct::Edit => {
                self.load_anim(core.slots[i].sprite.clone());
                self.go(ctx, Tab::Pixel);
            }
            LibAct::Overwrite => {
                if core.slots[i].builtin {
                    self.notify(
                        ctx,
                        tx(
                            en,
                            "Vorlagen sind schreibgeschützt, speichere als neues Sprite",
                            "Presets are read-only, save as a new sprite",
                        ),
                    );
                } else {
                    core.overwrite_slot(i, self.assembled());
                    self.notify(ctx, tx(en, "Sprite überschrieben", "Sprite overwritten"));
                }
            }
            LibAct::Use => {
                if core.slots[i].sprite.is_clothing() {
                    let name = core.slots[i].name.clone();
                    if core.settings.clothes_locked.contains(&name) {
                        self.notify(ctx, tx(en, "Diese Ebene ist gesperrt", "This layer is locked"));
                    } else if core.settings.clothes.contains(&name) {
                        core.settings.clothes.retain(|n| *n != name);
                    } else {
                        core.settings.clothes.push(name);
                    }
                } else if core.slots[i].sprite.is_hat() {
                    if core.hat == Some(i) {
                        core.hat = None;
                    } else {
                        core.hat = Some(i);
                        core.settings.hat_visible = true;
                    }
                } else if core.slots[i].sprite.is_item() {
                    if core.item == Some(i) {
                        core.item = None;
                    } else {
                        core.item = Some(i);
                        core.settings.item_visible = true;
                    }
                } else {
                    core.buddy = if core.buddy == Some(i) { None } else { Some(i) };
                }
            }
            LibAct::Rename => {
                let name = self.rename.trim().to_string();
                if name.is_empty() {
                    self.notify(ctx, tx(en, "Der Name darf nicht leer sein", "The name cannot be empty"));
                } else if name != core.slots[i].name {
                    if core.slots.iter().any(|s| s.name == name) {
                        self.notify(ctx, tx(en, "Dieser Name ist schon vergeben", "This name is already taken"));
                    } else {
                        core.rename_slot(i, name);
                        self.notify(ctx, tx(en, "Umbenannt", "Renamed"));
                    }
                }
            }
            LibAct::Export => {
                let s = core.slots[i].sprite.clone();
                let name = format!("sprite_{}", i + 1);
                self.export(ctx, s, &name);
            }
            LibAct::Delete => {
                let builtin = core.slots[i].builtin;
                core.remove_slot(i);
                self.sel = None;
                if builtin {
                    self.notify(
                        ctx,
                        tx(
                            en,
                            "Vorlage entfernt, du kannst sie jederzeit wiederherstellen",
                            "Preset removed, you can restore it any time",
                        ),
                    );
                }
            }
        }
    }

    fn draw_toast(&mut self, ctx: &Context) {
        let Some((msg, t0)) = self.toast.clone() else {
            return;
        };
        let age = (ctx.input(|i| i.time) - t0) as f32;
        if age > 3.2 {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        let a = (age / 0.2).min(1.0).min(((3.2 - age) / 0.4).clamp(0.0, 1.0));
        let slide = (1.0 - a) * 10.0;
        egui::Area::new(Id::new("toast"))
            .anchor(Align2::RIGHT_BOTTOM, [-20.0, -20.0 + slide])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(panel_col().gamma_multiply(a))
                    .stroke(Stroke::new(1.0f32, accent().gamma_multiply(a)))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(14.0, 9.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).size(12.5).color(TEXT.gamma_multiply(a)));
                    });
            });
    }
}

fn next_change<T: PartialEq>(f: impl Fn(f64) -> T, t: f64) -> Duration {
    let now = f(t);
    let mut k = 0.005;
    while k < 0.5 {
        if f(t + k) != now {
            return Duration::from_secs_f64(k);
        }
        k += 0.005;
    }
    Duration::from_millis(500)
}

fn mirrored(x: i32, y: i32, w: i32, h: i32, mx: bool, my: bool) -> Vec<(i32, i32)> {
    let mut out = vec![(x, y)];
    if mx {
        out.push((w - 1 - x, y));
    }
    if my {
        out.extend(out.clone().into_iter().map(|(px, py)| (px, h - 1 - py)));
    }
    out.dedup();
    out
}

fn shape_pts(tool: Tool, a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    match tool {
        Tool::Rect => {
            let mut out = Vec::new();
            for x in x0..=x1 {
                out.extend([(x, y0), (x, y1)]);
            }
            for y in y0..=y1 {
                out.extend([(x0, y), (x1, y)]);
            }
            out
        }
        Tool::Circle => {
            let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
            let (rx, ry) = ((x1 - x0) as f32 / 2.0, (y1 - y0) as f32 / 2.0);
            let n = ((rx + ry) * 8.0) as i32 + 16;
            let pts: Vec<(i32, i32)> = (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    ((cx + rx * t.cos()).round() as i32, (cy + ry * t.sin()).round() as i32)
                })
                .collect();
            pts.windows(2).flat_map(|w| line_pts(w[0], w[1])).collect()
        }
        _ => line_pts(a, b),
    }
}

fn line_pts(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x0, mut y0) = a;
    let (x1, y1) = b;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        out.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
    out
}

fn traffic_lights(ui: &mut Ui, full: Rect) {
    const COLORS: [Color32; 3] = [Color32::from_rgb(255, 95, 87), Color32::from_rgb(254, 188, 46), Color32::from_rgb(40, 200, 64)];
    let zone = Rect::from_min_size(full.min, Vec2::new(crate::soft::DOTS_W, full.height()));
    let near = ui.rect_contains_pointer(zone);
    let ink = Stroke::new(1.3f32, Color32::from_black_alpha(150));
    for (i, color) in COLORS.into_iter().enumerate() {
        let c = Pos2::new(full.left() + 20.0 + i as f32 * 20.0, full.center().y);
        let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(16.0)), Id::new(("dot", i)), Sense::click());
        let p = ui.painter();
        let ring = (0..32).map(|k| c + Vec2::angled(k as f32 * std::f32::consts::TAU / 32.0) * 6.0).collect();
        p.add(egui::Shape::convex_polygon(ring, color, Stroke::NONE));
        if near {
            match i {
                0 => {
                    p.line_segment([c + Vec2::new(-2.5, -2.5), c + Vec2::new(2.5, 2.5)], ink);
                    p.line_segment([c + Vec2::new(-2.5, 2.5), c + Vec2::new(2.5, -2.5)], ink);
                }
                1 => {
                    p.line_segment([c + Vec2::new(-3.0, 0.0), c + Vec2::new(3.0, 0.0)], ink);
                }
                _ => {
                    p.line_segment([c + Vec2::new(-3.0, 0.0), c + Vec2::new(3.0, 0.0)], ink);
                    p.line_segment([c + Vec2::new(0.0, -3.0), c + Vec2::new(0.0, 3.0)], ink);
                }
            }
        }
        if resp.clicked() {
            ui.ctx().send_viewport_cmd(match i {
                0 => egui::ViewportCommand::Close,
                1 => egui::ViewportCommand::Minimized(true),
                _ => egui::ViewportCommand::Maximized(true),
            });
        }
    }
}

fn title_bar(ui: &mut Ui) {
    let full = ui.max_rect();
    if glass() {
        traffic_lights(ui, full);
        ui.painter().text(full.left_center() + Vec2::new(crate::soft::DOTS_W, 0.0), Align2::LEFT_CENTER, crate::STUDIO_TITLE, FontId::proportional(12.0), MUTED);
        return;
    }
    ui.painter().text(full.left_center() + Vec2::new(14.0, 0.0), Align2::LEFT_CENTER, crate::STUDIO_TITLE, FontId::proportional(12.0), MUTED);
    let w = crate::soft::CAPTION_BUTTON_W;
    for (i, kind) in [0u8, 1, 2].into_iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(full.right() - w * (3 - i) as f32, full.top()), Vec2::new(w, full.height()));
        let resp = ui.interact(rect, Id::new(("caption", kind)), Sense::click());
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, Rounding::ZERO, if kind == 2 { Color32::from_rgb(196, 43, 28) } else { Color32::from_white_alpha(20) });
        }
        let c = rect.center();
        let ink = Stroke::new(1.0f32, TEXT);
        match kind {
            0 => {
                p.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], ink);
            }
            1 => {
                p.rect_stroke(Rect::from_center_size(c, Vec2::splat(10.0)), Rounding::same(1.0), ink);
            }
            _ => {
                p.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], ink);
                p.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], ink);
            }
        }
        if resp.clicked() {
            ui.ctx().send_viewport_cmd(match kind {
                0 => egui::ViewportCommand::Minimized(true),
                1 => egui::ViewportCommand::Maximized(true),
                _ => egui::ViewportCommand::Close,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_export() {
        let ctx = Context::default();
        let core = Core::load(None);
        let mut st = Studio::new(&ctx, &core);
        let mut a = Sprite::new(3, 2);
        a.set(0, 0, [255, 0, 0, 255]);
        let mut b = Sprite::new(3, 2);
        b.set(2, 1, [0, 255, 0, 255]);
        a.anim = Anim { frames: vec![b.clone(), b], fps: 10, mode: 1 };
        let dir = std::env::temp_dir().join("crab_gif_test");
        std::fs::create_dir_all(&dir).unwrap();
        st.export_anim(&ctx, &a, &dir);
        let file = std::io::BufReader::new(std::fs::File::open(dir.join("crab_sprite.gif")).unwrap());
        let frames = image::AnimationDecoder::into_frames(image::codecs::gif::GifDecoder::new(file).unwrap()).collect_frames().unwrap();
        assert_eq!(frames.len(), 4);
        let sheet = image::open(dir.join("crab_sprite_sheet.png")).unwrap();
        assert_eq!(sheet.width(), 3 * 3 * SCALES[st.export_idx]);
    }

    #[test]
    fn animation_frames() {
        let idx = |n, fps, mode, ts: &[f32]| ts.iter().map(|&t| anim_index(n, fps, mode, t)).collect::<Vec<_>>();
        assert_eq!(idx(4, 2, 0, &[0.0, 0.5, 1.0, 1.5, 2.0]), vec![0, 1, 2, 3, 0]);
        assert_eq!(idx(3, 1, 1, &[0.0, 1.0, 2.0, 3.0, 4.0]), vec![0, 1, 2, 1, 0]);
        assert_eq!(idx(3, 1, 2, &[0.0, 1.0, 2.0, 9.0]), vec![0, 1, 2, 2]);
        assert_eq!(idx(1, 24, 0, &[5.0]), vec![0]);

        let ctx = Context::default();
        let core = Core::load(None);
        let mut st = Studio::new(&ctx, &core);
        let mut hat = Sprite::new(HAT_W, HAT_H);
        hat.anim.frames = vec![Sprite::new(HAT_W, HAT_H)];
        st.load_anim(hat);
        assert_eq!(st.frames.len(), 1);
        st.load_anim(Sprite::new(ITEM_N, ITEM_N));
        st.sprite.set(0, 0, [1, 2, 3, 255]);
        st.frames.insert(1, Sprite::new(ITEM_N, ITEM_N));
        st.goto(1);
        st.sprite.set(3, 3, [9, 9, 9, 255]);
        st.fps = 12;
        st.mode = 1;
        let a = st.assembled();
        assert_eq!(a.count(), 2);
        assert_eq!(a.get(0, 0)[3], 255);
        assert_eq!(a.frame(1).get(3, 3)[3], 255);
        assert_eq!(a.frame(1).get(0, 0)[3], 0);
        assert_eq!((a.anim.fps, a.anim.mode), (12, 1));
        st.load_anim(a.clone());
        assert_eq!((st.frames.len(), st.cur, st.fps, st.mode), (2, 0, 12, 1));
        assert!(st.assembled() == a);
    }

    #[test]
    fn editor_tools() {
        let mut rect = shape_pts(Tool::Rect, (0, 0), (2, 2));
        rect.sort();
        rect.dedup();
        assert_eq!(rect.len(), 8);
        let circle = shape_pts(Tool::Circle, (0, 0), (4, 4));
        for p in [(2, 0), (0, 2), (4, 2), (2, 4)] {
            assert!(circle.contains(&p), "{p:?}");
        }
        assert!(!circle.contains(&(2, 2)));
        let mut m = mirrored(1, 1, 4, 4, true, true);
        m.sort();
        assert_eq!(m, vec![(1, 1), (1, 2), (2, 1), (2, 2)]);

        let ctx = Context::default();
        let core = Core::load(None);
        let mut st = Studio::new(&ctx, &core);
        st.sprite = Sprite::new(3, 3);
        st.sprite.set(0, 0, [255, 0, 0, 255]);
        st.transform(2);
        assert_eq!(st.sprite.get(2, 0)[3], 255);
        assert_eq!(st.sprite.get(0, 0)[3], 0);
        st.transform(0);
        assert_eq!(st.sprite.get(0, 0)[3], 255);
        st.transform(1);
        assert_eq!(st.sprite.get(0, 2)[3], 255);
        st.do_undo();
        assert_eq!(st.sprite.get(0, 0)[3], 255);

        st.area = Some((0, 0, 0, 0));
        st.select_drag(true, (0, 0));
        st.select_drag(false, (1, 1));
        assert_eq!(st.sprite.get(1, 1)[3], 255);
        assert_eq!(st.sprite.get(0, 0)[3], 0);
        assert_eq!(st.area, Some((1, 1, 1, 1)));
    }

    #[test]
    #[ignore]
    fn studio_frame_cost() {
        for (w, h) in [(1000usize, 680usize), (1920, 1040)] {
            let bg: Vec<u32> = (0..w * h).map(|i| 0xFF00_0000 | (i as u32 * 2654435761) >> 8).collect();
            let ctx = Context::default();
            apply_style(&ctx);
            let mut core = Core::load(Some(&ctx));
            core.settings.glass = true;
            let mut studio = Studio::new(&ctx, &core);
            let mut pet = PetRemote::new();
            let mut off = crate::soft::Offscreen::default();
            let mut glass = crate::glass::Glass::default();
            let mut overlay = Vec::new();
            let (mut t_ui, mut t_glass, mut t_raster) = (0.0, 0.0, 0.0);
            let n = 30;
            for f in 0..n + 5 {
                let raw = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32))),
                    time: Some(f as f64 * 0.016),
                    ..Default::default()
                };
                let t0 = std::time::Instant::now();
                let out = ctx.run(raw, |ctx| studio.frame(ctx, &mut core, &mut pet));
                let rects = take_glass_rects();
                let t1 = std::time::Instant::now();
                crate::glass::paint(&mut glass, &mut overlay, w, h, 1.0, &crate::glass::Bg { px: &bg, x0: f, y0: 0, w, h }, &rects);
                let t2 = std::time::Instant::now();
                off.frame(&ctx, out, w, h, Some(&overlay));
                let t3 = std::time::Instant::now();
                if f >= 5 {
                    t_ui += (t1 - t0).as_secs_f64();
                    t_glass += (t2 - t1).as_secs_f64();
                    t_raster += (t3 - t2).as_secs_f64();
                }
            }
            let ms = |t: f64| t * 1000.0 / n as f64;
            println!("{w}x{h}: ui {:.1} ms, glass {:.1} ms, raster {:.1} ms", ms(t_ui), ms(t_glass), ms(t_raster));
        }
    }

    fn shot(theme: usize, tab: Tab, w: usize, h: usize, bg: &[u32], prep: impl Fn(&mut Studio, &mut Core)) -> image::RgbaImage {
        let ctx = Context::default();
        apply_style(&ctx);
        let mut core = Core::load(Some(&ctx));
        core.settings.glass = theme == 0;
        core.settings.black = theme == 2;
        let mut studio = Studio::new(&ctx, &core);
        studio.tab = tab;
        prep(&mut studio, &mut core);
        let mut pet = PetRemote::new();
        let mut off = crate::soft::Offscreen::default();
        let mut glass = crate::glass::Glass::default();
        let mut overlay = Vec::new();
        let mut img = Vec::new();
        for f in 0..6 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32))),
                time: Some(f as f64),
                ..Default::default()
            };
            let out = ctx.run(raw, |ctx| studio.frame(ctx, &mut core, &mut pet));
            let rects = take_glass_rects();
            let base = (theme == 0).then(|| {
                crate::glass::paint(&mut glass, &mut overlay, w, h, 1.0, &crate::glass::Bg { px: bg, x0: 0, y0: 0, w, h }, &rects);
                &overlay[..]
            });
            img = off.frame(&ctx, out, w, h, base).to_vec();
        }
        let mut png = image::RgbaImage::new(w as u32, h as u32);
        for (i, (&o, &b)) in img.iter().zip(bg).enumerate() {
            let a = o >> 24;
            let ch = |sh: u32| (((o >> sh) & 255) + ((b >> sh) & 255) * (255 - a) / 255).min(255) as u8;
            png.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba([ch(16), ch(8), ch(0), 255]));
        }
        png
    }

    fn worn(studio: &mut Studio, core: &mut Core, tab: Tab, theme: usize) {
        core.settings.clothes = vec!["Umhang".into(), "T-Shirt".into()];
        core.settings.clothes_locked = vec!["Umhang".into()];
        core.settings.outfits = vec![core.current_outfit("Superheld".into())];
        if tab == Tab::Pixel && theme == 1 {
            studio.load_anim(core.slots.iter().find(|s| s.name == "T-Shirt").unwrap().sprite.clone());
        } else if tab == Tab::Pixel {
            let mut cup = core.slots.iter().find(|s| s.name == "Kaffee").unwrap().sprite.clone();
            let mut up = Sprite::new(cup.w, cup.h);
            for y in 1..cup.h {
                for x in 0..cup.w {
                    up.set(x, y - 1, cup.get(x, y));
                }
            }
            cup.anim = Anim { frames: vec![up, cup.clone()], fps: 6, mode: 1 };
            studio.load_anim(cup);
            studio.goto(1);
        }
    }

    #[test]
    #[ignore]
    fn studio_sheet() {
        let (w, h) = (1000usize, 1500usize);
        let bg: Vec<u32> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let text = (60..360).contains(&y) && (300..900).contains(&x) && y % 24 < 12 && (x % 7 < 2 || y % 24 == 0 || y % 24 == 11);
                if text { 0xFFF0F0F0 } else { 0xFF00_0000 | (40 + (x * 120 / w) as u32) << 16 | (30 + (y * 60 / h) as u32) << 8 | (90 + (y * 100 / h) as u32) }
            })
            .collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("studio_sheet");
        std::fs::create_dir_all(&dir).unwrap();
        for (theme, name) in [(0, "apple"), (1, "classic"), (2, "black")] {
            for (tab, tname) in [(Tab::Crab, "crab"), (Tab::Behavior, "behavior"), (Tab::Claude, "claude"), (Tab::Pixel, "pixel"), (Tab::Library, "sprites"), (Tab::App, "app")] {
                shot(theme, tab, w, h, &bg, |st, core| worn(st, core, tab, theme)).save(dir.join(format!("{name}_{tname}.png"))).unwrap();
            }
        }
    }

    #[test]
    #[ignore]
    fn readme_images() {
        let (w, h) = (1000usize, 680usize);
        let bg: Vec<u32> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                let blob = |cx: f32, cy: f32, r: f32| (1.0 - (((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / r)).max(0.0);
                let (a, b) = (blob(0.2, 0.25, 0.6), blob(0.85, 0.8, 0.7));
                let c = |base: f32, ka: f32, kb: f32| ((base + ka * a + kb * b).clamp(0.0, 255.0)) as u32;
                0xFF00_0000 | c(40.0 + 60.0 * x, 120.0, 40.0) << 16 | c(30.0, 40.0, 70.0) << 8 | c(90.0 + 60.0 * y, 60.0, 120.0)
            })
            .collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs").join("images");
        std::fs::create_dir_all(&dir).unwrap();
        for (theme, tab, name) in [
            (0, Tab::Crab, "studio-liquid-glass.png"),
            (1, Tab::Crab, "studio-classic.png"),
            (2, Tab::Pixel, "pixel-editor.png"),
            (1, Tab::Pixel, "clothing-editor.png"),
            (0, Tab::Library, "sprite-library.png"),
        ] {
            let en = |st: &mut Studio, core: &mut Core| {
                core.settings.lang = 1;
                st.en = true;
                worn(st, core, tab, theme);
            };
            shot(theme, tab, w, h, &bg, en).save(dir.join(name)).unwrap();
        }
    }

}
