use crate::art::{Def, ItemKind, DEFS};
use egui::{self, ColorImage, TextureHandle, TextureOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const HAT_W: usize = 12;
pub const HAT_H: usize = 6;
pub const CLOTH_W: usize = 24;
pub const CLOTH_H: usize = 16;
pub const ITEM_N: usize = 8;

pub fn data_path(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn parse_hex(s: &str) -> [u8; 4] {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]
}

#[derive(Clone, PartialEq, Default)]
pub struct Anim {
    pub frames: Vec<Sprite>,
    pub fps: u8,
    pub mode: u8,
}

#[derive(Clone, PartialEq, Default)]
pub struct Sprite {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 4]>,
    pub anim: Anim,
    pub back: bool,
}

pub fn anim_index(n: usize, fps: u8, mode: u8, t: f32) -> usize {
    if n <= 1 {
        return 0;
    }
    let k = (t.max(0.0) * fps.clamp(1, 24) as f32) as usize;
    match mode {
        1 => {
            let p = 2 * n - 2;
            let i = k % p;
            if i < n { i } else { p - i }
        }
        2 => k.min(n - 1),
        _ => k % n,
    }
}

pub fn clock() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32()
}

impl Sprite {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![[0, 0, 0, 0]; w * h],
            anim: Anim::default(),
            back: false,
        }
    }

    pub fn is_clothing(&self) -> bool {
        (self.w, self.h) == (CLOTH_W, CLOTH_H)
    }

    pub fn count(&self) -> usize {
        1 + self.anim.frames.len()
    }

    pub fn frame(&self, i: usize) -> &Sprite {
        if i == 0 || self.anim.frames.is_empty() {
            return self;
        }
        &self.anim.frames[(i - 1).min(self.anim.frames.len() - 1)]
    }

    pub fn at(&self, t: f32) -> &Sprite {
        self.frame(anim_index(self.count(), self.anim.fps, self.anim.mode, t))
    }

    pub fn is_hat(&self) -> bool {
        (self.w, self.h) == (HAT_W, HAT_H) || (self.w, self.h) == (HAT_W * 2, HAT_H * 2)
    }

    pub fn is_item(&self) -> bool {
        self.w == ITEM_N && self.h == ITEM_N
    }

    pub fn is_buddy(&self) -> bool {
        !self.is_hat() && !self.is_item() && !self.is_clothing()
    }

    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        self.px[y * self.w + x]
    }

    pub fn set(&mut self, x: usize, y: usize, c: [u8; 4]) {
        self.px[y * self.w + x] = c;
    }

    pub fn flood(&mut self, x: usize, y: usize, c: [u8; 4]) {
        let target = self.get(x, y);
        if target == c {
            return;
        }
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            if self.get(cx, cy) != target {
                continue;
            }
            self.set(cx, cy, c);
            if cx > 0 {
                stack.push((cx - 1, cy));
            }
            if cx + 1 < self.w {
                stack.push((cx + 1, cy));
            }
            if cy > 0 {
                stack.push((cx, cy - 1));
            }
            if cy + 1 < self.h {
                stack.push((cx, cy + 1));
            }
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.px.iter().flat_map(|p| p.iter().copied()).collect()
    }

    pub fn color_image(&self) -> ColorImage {
        ColorImage::from_rgba_unmultiplied([self.w, self.h], &self.bytes())
    }

    pub fn save(&self, path: &Path, scale: u32) -> Result<(), String> {
        let (w, h) = (self.w as u32, self.h as u32);
        let img = image::RgbaImage::from_raw(w, h, self.bytes())
            .ok_or_else(|| "Bilddaten ungültig".to_string())?;
        let out = if scale > 1 {
            image::imageops::resize(
                &img,
                w * scale,
                h * scale,
                image::imageops::FilterType::Nearest,
            )
        } else {
            img
        };
        out.save(path).map_err(|e| e.to_string())
    }

    fn to_hex(&self) -> String {
        let mut s = String::with_capacity(self.px.len() * 8);
        for p in &self.px {
            for b in p {
                s.push_str(&format!("{b:02x}"));
            }
        }
        s
    }

    fn from_hex(w: usize, h: usize, hex: &str) -> Option<Self> {
        if !hex.is_ascii() || hex.len() != w * h * 8 {
            return None;
        }
        let mut px = Vec::with_capacity(w * h);
        for i in 0..w * h {
            let mut p = [0u8; 4];
            for (k, slot) in p.iter_mut().enumerate() {
                let at = i * 8 + k * 2;
                *slot = u8::from_str_radix(&hex[at..at + 2], 16).ok()?;
            }
            px.push(p);
        }
        Some(Self { w, h, px, anim: Anim::default(), back: false })
    }
}

pub fn sprite_from_def(d: &Def) -> Sprite {
    let h = d.rows.len();
    let w = d.rows[0].chars().count();
    let mut s = Sprite::new(w, h);
    for (y, row) in d.rows.iter().enumerate() {
        for (x, ch) in row.chars().take(w).enumerate() {
            if let Some((_, hex)) = d.colors.iter().find(|(k, _)| *k == ch) {
                s.set(x, y, parse_hex(hex));
            }
        }
    }
    s
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub name: String,
    pub scale: f32,
    pub speed: f32,
    pub opacity: f32,
    pub idle: usize,
    pub anchor: usize,
    pub on_top: bool,
    pub follow: bool,
    pub click_through: bool,
    pub autostart: bool,
    pub hat_visible: bool,
    pub hat_nudge: f32,
    pub hat: String,
    pub buddy: String,
    pub item: String,
    pub item_visible: bool,
    pub lang: usize,
    pub hidden: Vec<String>,
    pub lasso: bool,
    pub debug: bool,
    pub glass: bool,
    pub obs: bool,
    pub accent: usize,
    pub black: bool,
    pub accent_hex: String,
    pub palette: Vec<String>,
    pub crab_body: String,
    pub crab_claw: String,
    pub crab_eye: String,
    pub crab_rainbow: bool,
    pub clothes: Vec<String>,
    pub clothes_hidden: Vec<String>,
    pub clothes_locked: Vec<String>,
    pub outfits: Vec<Outfit>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Outfit {
    pub name: String,
    pub hat: String,
    pub item: String,
    pub clothes: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "Claude".to_string(),
            scale: 1.0,
            speed: 1.0,
            opacity: 1.0,
            idle: 0,
            anchor: 0,
            on_top: true,
            follow: false,
            click_through: false,
            autostart: false,
            hat_visible: true,
            hat_nudge: 0.0,
            hat: "Cowboy-Hut".to_string(),
            buddy: String::new(),
            item: "Kaffee".to_string(),
            item_visible: true,
            lang: 1,
            hidden: Vec::new(),
            lasso: true,
            debug: false,
            glass: true,
            obs: false,
            accent: 0,
            black: false,
            accent_hex: String::new(),
            palette: Vec::new(),
            crab_body: String::new(),
            crab_claw: String::new(),
            crab_eye: String::new(),
            crab_rainbow: false,
            clothes: Vec::new(),
            clothes_hidden: Vec::new(),
            clothes_locked: Vec::new(),
            outfits: Vec::new(),
        }
    }
}

impl Settings {
    fn sanitize(&mut self) {
        self.scale = self.scale.clamp(0.5, 1.6);
        self.speed = self.speed.clamp(0.2, 3.0);
        self.opacity = self.opacity.clamp(0.2, 1.0);
        self.hat_nudge = self.hat_nudge.clamp(-3.0, 3.0);
        self.idle = self.idle.min(3);
        self.anchor = self.anchor.min(3);
        self.lang = self.lang.min(1);
        self.accent_hex = self.accent_hex.chars().take(7).collect();
        self.palette.truncate(32);
        self.clothes.truncate(16);
        self.outfits.truncate(24);
        for s in [&mut self.crab_body, &mut self.crab_claw, &mut self.crab_eye] {
            *s = s.chars().take(7).collect();
        }
    }
}

pub fn read_settings() -> Option<Settings> {
    let text = std::fs::read_to_string(data_path("crab_config.json")).ok()?;
    let mut s: Settings = serde_json::from_str(&text).ok()?;
    s.sanitize();
    if let Some(on) = crate::sys::autostart_enabled() {
        s.autostart = on;
    }
    Some(s)
}

pub struct Slot {
    pub name: String,
    pub sprite: Sprite,
    pub tex: Option<TextureHandle>,
    pub builtin: bool,
}

impl Slot {
    pub fn label(&self, en: bool) -> &str {
        if self.builtin && en {
            if let Some(d) = DEFS.iter().find(|d| d.name == self.name) {
                return d.name_en;
            }
        }
        &self.name
    }
}

#[derive(Serialize, Deserialize)]
struct SlotDisk {
    name: String,
    w: usize,
    h: usize,
    data: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    frames: Vec<String>,
    #[serde(default)]
    fps: u8,
    #[serde(default)]
    mode: u8,
    #[serde(default)]
    back: bool,
}

fn load_user_slots() -> Vec<(String, Sprite)> {
    if let Ok(text) = std::fs::read_to_string(data_path("crab_library.json")) {
        let disk: Vec<SlotDisk> = serde_json::from_str(&text).unwrap_or_default();
        return disk
            .into_iter()
            .filter_map(|d| {
                let mut s = Sprite::from_hex(d.w, d.h, &d.data)?;
                s.anim.frames = d.frames.iter().filter_map(|f| Sprite::from_hex(d.w, d.h, f)).collect();
                s.anim.fps = d.fps.clamp(1, 24);
                s.anim.mode = d.mode.min(2);
                s.back = d.back;
                Some((d.name, s))
            })
            .collect();
    }
    legacy_hats()
}

fn legacy_hats() -> Vec<(String, Sprite)> {
    let Ok(text) = std::fs::read_to_string(data_path("custom_hats.json")) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, rows) in map {
        let Some(rows) = rows.as_array() else {
            continue;
        };
        let mut s = Sprite::new(HAT_W, HAT_H);
        for (y, row) in rows.iter().take(HAT_H).enumerate() {
            let Some(cells) = row.as_array() else {
                continue;
            };
            for (x, cell) in cells.iter().take(HAT_W).enumerate() {
                if let Some(hex) = cell.as_str() {
                    s.set(x, y, parse_hex(hex));
                }
            }
        }
        out.push((name, s));
    }
    out
}

fn shift(o: Option<usize>, removed: usize) -> Option<usize> {
    match o {
        Some(b) if b == removed => None,
        Some(b) if b > removed => Some(b - 1),
        other => other,
    }
}

pub struct Core {
    pub settings: Settings,
    pub slots: Vec<Slot>,
    pub buddy: Option<usize>,
    pub hat: Option<usize>,
    pub item: Option<usize>,
    pub dirty_lib: bool,
    counter: usize,
    next_tex: usize,
    t0: f32,
}

impl Core {
    pub fn load(ctx: Option<&egui::Context>) -> Self {
        let settings = read_settings().unwrap_or_else(|| Settings {
            autostart: crate::sys::autostart_enabled().unwrap_or(false),
            ..Settings::default()
        });
        let mut core = Self {
            settings,
            slots: Vec::new(),
            buddy: None,
            hat: None,
            item: None,
            dirty_lib: false,
            counter: 0,
            next_tex: 0,
            t0: clock(),
        };
        for def in DEFS.iter() {
            if core.settings.hidden.iter().any(|h| h == def.name) {
                continue;
            }
            let mut s = sprite_from_def(def);
            s.back = def.name == "Umhang";
            core.push_slot(ctx, def.name.to_string(), s, true);
        }
        let fresh = !data_path("crab_library.json").exists();
        for (name, sprite) in load_user_slots() {
            let unique = core.unique_name(&name);
            core.push_slot(ctx, unique, sprite, false);
            core.dirty_lib = fresh;
        }
        core.hat = core
            .find_name(&core.settings.hat)
            .filter(|&i| core.slots[i].sprite.is_hat());
        core.item = core
            .find_name(&core.settings.item)
            .filter(|&i| core.slots[i].sprite.is_item());
        core.buddy = core
            .find_name(&core.settings.buddy)
            .filter(|&i| core.slots[i].sprite.is_buddy());
        core
    }

    fn push_slot(
        &mut self,
        ctx: Option<&egui::Context>,
        name: String,
        sprite: Sprite,
        builtin: bool,
    ) -> usize {
        let shown = if builtin && ItemKind::of(Some(name.as_str())) == ItemKind::Laptop {
            sprite_from_def(&crate::art::LAPTOP_FINE)
        } else {
            sprite.clone()
        };
        let tex = ctx.map(|c| self.texture(c, &shown));
        self.slots.push(Slot {
            name,
            sprite,
            tex,
            builtin,
        });
        self.slots.len() - 1
    }

    fn find_name(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        self.slots.iter().position(|s| s.name == name)
    }

    pub fn find_same(&self, sprite: &Sprite) -> Option<usize> {
        self.slots.iter().position(|s| s.sprite == *sprite)
    }

    pub fn unique_name(&self, base: &str) -> String {
        if !self.slots.iter().any(|s| s.name == base) {
            return base.to_string();
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base} {n}");
            if !self.slots.iter().any(|s| s.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    pub fn add_slot(&mut self, ctx: &egui::Context, name: Option<String>, sprite: Sprite) -> usize {
        self.counter += 1;
        let base = name.unwrap_or_else(|| format!("Sprite {}", self.counter));
        let unique = self.unique_name(&base);
        self.dirty_lib = true;
        self.push_slot(Some(ctx), unique, sprite, false)
    }

    fn texture(&mut self, ctx: &egui::Context, sprite: &Sprite) -> TextureHandle {
        self.next_tex += 1;
        ctx.load_texture(
            format!("slot{}", self.next_tex),
            sprite.color_image(),
            TextureOptions::NEAREST,
        )
    }

    pub fn overwrite_slot(&mut self, i: usize, sprite: Sprite) {
        if i >= self.slots.len() || self.slots[i].builtin {
            return;
        }
        if let Some(t) = self.slots[i].tex.as_mut() {
            t.set(sprite.color_image(), TextureOptions::NEAREST);
        }
        self.slots[i].sprite = sprite;
        self.fix_roles(i);
        self.dirty_lib = true;
    }

    fn fix_roles(&mut self, i: usize) {
        let sp = &self.slots[i].sprite;
        if !sp.is_hat() && self.hat == Some(i) {
            self.hat = None;
        }
        if !sp.is_item() && self.item == Some(i) {
            self.item = None;
        }
        if !sp.is_buddy() && self.buddy == Some(i) {
            self.buddy = None;
        }
    }

    pub fn remove_slot(&mut self, i: usize) {
        if i >= self.slots.len() {
            return;
        }
        if self.slots[i].builtin {
            let name = self.slots[i].name.clone();
            if !self.settings.hidden.contains(&name) {
                self.settings.hidden.push(name);
            }
        } else {
            self.dirty_lib = true;
        }
        self.slots.remove(i);
        self.buddy = shift(self.buddy, i);
        self.hat = shift(self.hat, i);
        self.item = shift(self.item, i);
    }

    pub fn missing_defaults(&self) -> usize {
        DEFS.iter()
            .filter(|d| !self.slots.iter().any(|s| s.builtin && s.name == d.name))
            .count()
    }

    pub fn restore_defaults(&mut self, ctx: &egui::Context) -> usize {
        let mut restored = 0;
        for (di, def) in DEFS.iter().enumerate() {
            if self.slots.iter().any(|s| s.builtin && s.name == def.name) {
                continue;
            }
            let pos = self
                .slots
                .iter()
                .take_while(|s| s.builtin)
                .filter(|s| {
                    DEFS.iter()
                        .position(|d| d.name == s.name)
                        .map(|p| p < di)
                        .unwrap_or(false)
                })
                .count();
            let sprite = sprite_from_def(def);
            let tex = Some(self.texture(ctx, &sprite));
            self.slots.insert(
                pos,
                Slot {
                    name: def.name.to_string(),
                    sprite,
                    tex,
                    builtin: true,
                },
            );
            let bump = |o: Option<usize>| o.map(|b| if b >= pos { b + 1 } else { b });
            self.buddy = bump(self.buddy);
            self.hat = bump(self.hat);
            self.item = bump(self.item);
            restored += 1;
        }
        self.settings.hidden.clear();
        if self.hat.is_none() {
            self.hat = self.find_name(&self.settings.hat);
        }
        if self.item.is_none() {
            self.item = self.find_name(&self.settings.item);
        }
        restored
    }

    pub fn hat_sprite(&self) -> Option<&Sprite> {
        self.hat.and_then(|i| self.slots.get(i)).map(|s| &s.sprite)
    }

    pub fn item_sprite(&self) -> Option<&Sprite> {
        self.item.and_then(|i| self.slots.get(i)).map(|s| s.sprite.at(clock() - self.t0))
    }

    pub fn item_next_frame(&self) -> Option<std::time::Duration> {
        let s = &self.item.and_then(|i| self.slots.get(i))?.sprite;
        if s.count() < 2 || !self.settings.item_visible {
            return None;
        }
        let fps = s.anim.fps.clamp(1, 24) as f32;
        let e = (clock() - self.t0) * fps;
        Some(std::time::Duration::from_secs_f32(((e.floor() + 1.0 - e) / fps).max(0.005)))
    }

    pub fn hat_drop(&self) -> f32 {
        self.hat
            .and_then(|i| self.slots.get(i))
            .filter(|s| s.builtin)
            .map(|s| crate::art::hat_drop(&s.name))
            .unwrap_or(0.0)
    }

    pub fn item_kind(&self) -> ItemKind {
        ItemKind::of(
            self.item
                .and_then(|i| self.slots.get(i))
                .filter(|s| s.builtin)
                .map(|s| s.name.as_str()),
        )
    }

    pub fn outfit_layers(&self) -> Vec<(Sprite, bool)> {
        self.settings
            .clothes
            .iter()
            .filter(|n| !self.settings.clothes_hidden.contains(n))
            .filter_map(|n| self.slots.iter().find(|s| &s.name == n && s.sprite.is_clothing()))
            .map(|s| (s.sprite.clone(), s.sprite.back))
            .collect()
    }

    pub fn rename_slot(&mut self, i: usize, name: String) {
        let old = std::mem::replace(&mut self.slots[i].name, name.clone());
        let fix = |v: &mut Vec<String>| v.iter_mut().filter(|n| **n == old).for_each(|n| *n = name.clone());
        fix(&mut self.settings.clothes);
        fix(&mut self.settings.clothes_hidden);
        fix(&mut self.settings.clothes_locked);
        for o in &mut self.settings.outfits {
            fix(&mut o.clothes);
            for n in [&mut o.hat, &mut o.item] {
                if *n == old {
                    *n = name.clone();
                }
            }
        }
        self.dirty_lib = true;
    }

    pub fn wear_outfit(&mut self, o: &Outfit) {
        let find = |n: &str| self.slots.iter().position(|s| !n.is_empty() && s.name == n);
        self.hat = find(&o.hat).filter(|&i| self.slots[i].sprite.is_hat());
        self.item = find(&o.item).filter(|&i| self.slots[i].sprite.is_item());
        self.settings.clothes = o.clothes.iter().filter(|n| find(n).is_some()).cloned().collect();
        if self.hat.is_some() {
            self.settings.hat_visible = true;
        }
        if self.item.is_some() {
            self.settings.item_visible = true;
        }
    }

    pub fn current_outfit(&self, name: String) -> Outfit {
        let name_of = |i: Option<usize>| i.and_then(|i| self.slots.get(i)).map(|s| s.name.clone()).unwrap_or_default();
        Outfit { name, hat: name_of(self.hat), item: name_of(self.item), clothes: self.settings.clothes.clone() }
    }

    pub fn buddy_sprite(&self) -> Option<&Sprite> {
        self.buddy.and_then(|i| self.slots.get(i)).map(|s| &s.sprite)
    }

    pub fn save_settings(&mut self) -> Result<(), String> {
        self.settings.hat = self
            .hat
            .and_then(|i| self.slots.get(i))
            .map(|s| s.name.clone())
            .unwrap_or_default();
        self.settings.item = self
            .item
            .and_then(|i| self.slots.get(i))
            .map(|s| s.name.clone())
            .unwrap_or_default();
        self.settings.buddy = self
            .buddy
            .and_then(|i| self.slots.get(i))
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let text = serde_json::to_string_pretty(&self.settings).map_err(|e| e.to_string())?;
        std::fs::write(data_path("crab_config.json"), text).map_err(|e| e.to_string())
    }

    pub fn save_library(&self) -> Result<(), String> {
        let list: Vec<SlotDisk> = self
            .slots
            .iter()
            .filter(|s| !s.builtin)
            .map(|s| SlotDisk {
                name: s.name.clone(),
                w: s.sprite.w,
                h: s.sprite.h,
                data: s.sprite.to_hex(),
                frames: s.sprite.anim.frames.iter().map(|f| f.to_hex()).collect(),
                fps: s.sprite.anim.fps,
                mode: s.sprite.anim.mode,
                back: s.sprite.back,
            })
            .collect();
        let text = serde_json::to_string(&list).map_err(|e| e.to_string())?;
        std::fs::write(data_path("crab_library.json"), text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_keeps_frames() {
        let old = r#"[{"name":"a","w":1,"h":1,"data":"ff000080"}]"#;
        let d: Vec<SlotDisk> = serde_json::from_str(old).unwrap();
        assert!(d[0].frames.is_empty());
        let mut s = Sprite::new(2, 1);
        s.set(0, 0, [1, 2, 3, 255]);
        let mut f = Sprite::new(2, 1);
        f.set(1, 0, [4, 5, 6, 255]);
        s.anim = Anim { frames: vec![f.clone()], fps: 8, mode: 2 };
        let disk = SlotDisk { name: "x".into(), w: 2, h: 1, data: s.to_hex(), frames: vec![f.to_hex()], fps: 8, mode: 2, back: false };
        let text = serde_json::to_string(&disk).unwrap();
        let back: SlotDisk = serde_json::from_str(&text).unwrap();
        let mut r = Sprite::from_hex(back.w, back.h, &back.data).unwrap();
        r.anim.frames = back.frames.iter().filter_map(|x| Sprite::from_hex(2, 1, x)).collect();
        r.anim.fps = back.fps;
        r.anim.mode = back.mode;
        assert!(r == s);
        assert_eq!(r.at(0.0).get(0, 0)[3], 255);
        assert_eq!(r.at(0.2).get(1, 0)[3], 255);
    }
}
