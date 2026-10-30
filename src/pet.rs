use crate::art::{
    self, Frame, ItemKind, COWBOY_QUOTES, COWBOY_QUOTES_EN, FETCH_QUOTES, FETCH_QUOTES_EN,
    GENERAL_QUOTES, GENERAL_QUOTES_EN, IDLE_BLINK, IDLE_LOOK_L, IDLE_LOOK_R, IDLE_STAND, LOOK_UP,
    TOSS_QUOTES, TOSS_QUOTES_EN,
};
use crate::claude::{Cc, Tracker};
use crate::physics::{Body, World};
use crate::render::{self, Brush, Fx, ItemPose};
use crate::sprite::Core;
use crate::sys;
use crate::render::Canvas;
use egui::{Pos2, Rect, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default, Clone, Copy, Debug)]
pub struct Input {
    pub clicked: bool,
    pub double_clicked: bool,
    pub secondary_clicked: bool,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub dragging: bool,
    pub any: bool,
}
use std::f32::consts::{FRAC_PI_4, PI, TAU};
use std::time::{SystemTime, UNIX_EPOCH};

const TICK: f64 = 1.0 / 30.0;
const TASKBAR: f32 = 52.0;
const WALK_SPEED: f32 = 1.5;
const WALK_FRAME_TICKS: u64 = 5;
const ANTICIPATION: u32 = 10;
const POSE_HOLD: u32 = 8;
const THINK_DOTS: [&str; 3] = [".", "..", "..."];
const TEMPO: f32 = 0.7;
const STRETCH_PHASE_SECONDS: [f32; 3] = [1.0, 2.0, 1.0];

const STRETCH_PHASE: [usize; 30] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2];

fn sprite_anchor(rows: &[&str]) -> (f32, f32, f32, f32) {
    let pad = rows.len() - 16;
    let body = |row: &str| -> Vec<usize> { row.bytes().enumerate().filter(|(_, b)| matches!(b, b'#' | b'd' | b'E')).map(|(c, _)| c).collect() };
    let (head, cols) = rows.iter().enumerate().map(|(r, row)| (r, body(row))).find(|(_, c)| c.len() >= 8).unwrap_or((pad, vec![6, 21]));
    let center = (cols[0] + cols[cols.len() - 1] + 1) as f32 / 2.0;
    let legs = pad + 12;
    let hand: Vec<(usize, usize)> = ((head + 4).min(legs)..legs).filter_map(|r| body(rows[r]).last().map(|&c| (r, c))).collect();
    let right = hand.iter().map(|h| h.1).max().unwrap_or(25);
    let at: Vec<usize> = hand.iter().filter(|h| h.1 == right).map(|h| h.0).collect();
    let mid = at.first().map_or(14.0, |&a| (a + at[at.len() - 1] + 1) as f32 / 2.0);
    let eyes = (head..(head + 6).min(rows.len())).find(|&r| rows[r].contains('E')).unwrap_or(head + 2);
    let hat_row = head.min(eyes.saturating_sub(2));
    ((center - 14.0) / 2.0, (hat_row as f32 - pad as f32) / 2.0, (right as f32 - 25.0) / 2.0, (mid - pad as f32 - 6.0) / 2.0)
}

fn seq_ticks(range: std::ops::Range<usize>) -> u32 {
    range.map(|i| stretch_ticks(STRETCH_PHASE[i])).sum()
}

fn crouch_frame(t: u32) -> usize {
    10 + ((t / stretch_ticks(1)) % 15) as usize
}

fn seq_frame(range: std::ops::Range<usize>, mut t: u32) -> Option<usize> {
    for i in range {
        let hold = stretch_ticks(STRETCH_PHASE[i]);
        if t < hold {
            return Some(i);
        }
        t -= hold;
    }
    None
}

fn pickup_ticks(d: &Dropped) -> u32 {
    PICKUP_TICKS + d.pose.dx.abs() as u32
}

fn stretch_ticks(phase: usize) -> u32 {
    let frames = STRETCH_PHASE.iter().filter(|&&p| p == phase).count() as f32;
    (STRETCH_PHASE_SECONDS[phase] / TEMPO * 30.0 / frames).round() as u32
}
const THROW_DIST: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Idle,
    LookAround,
    WalkLeft,
    WalkRight,
    Sleep,
    Happy,
    Cowboy,
    Drag,
    Fall,
    Play,
    Claude,
    Stretch,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Play {
    Toss,
    Throw,
    Drink,
    Sip,
    Sniff,
    Wave,
    Swing,
    Type,
    Rock,
}

#[derive(Clone, Debug)]
struct Dropped {
    body: Body,
    center: (f32, f32),
    pose: ItemPose,
    back: u32,
    carry: u32,
    from: (f32, f32),
    to: (f32, f32),
    desk: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Crouch {
    Off,
    Enter(u32),
    Loop(u32),
    Exit(u32),
}

const CARRY_TICKS: u32 = 24;
const CLAUDE_REST_TICKS: u32 = 60;
const PAT_TICKS: u32 = 24;
const GUITAR_TICKS: u32 = 5;
const TYPE_LOOP_TICKS: u32 = 90;

const PICKUP_TICKS: u32 = 12;
const LAPTOP_DESK: (f32, f32) = (0.5, 4.0);

fn body_pose(body: &Body, center: (f32, f32)) -> ItemPose {
    let (rx, ry) = render::rot(center, body.angle);
    ItemPose {
        dx: body.x - rx,
        dy: body.y - ry,
        angle: body.angle,
    }
}

fn land_of(core: &Core) -> f32 {
    core.item_sprite()
        .map(|s| 4.5 - render::item_bottom(s, 0.0))
        .unwrap_or(2.0)
}

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn float(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }

    fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }
}

pub(crate) fn hand_row(frame: &Frame, col: usize) -> Option<f32> {
    frame.iter().position(|r| r.as_bytes()[col * 2] == b'#').map(|r| r as f32 / 2.0)
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn snap(a: f32) -> f32 {
    (a / FRAC_PI_4).round() * FRAC_PI_4
}

fn cursor_points(ppp: f32) -> Option<Pos2> {
    sys::cursor_screen().map(|c| Pos2::new(c.x / ppp, c.y / ppp))
}

pub struct Pet {
    mode: Mode,
    rng: Rng,
    x: f32,
    y: f32,
    win_w: f32,
    win_h: f32,
    body_h: f32,
    block: i32,
    buddy: bool,
    crab_rel_x: f32,
    crab_rel_y: f32,
    work: Rect,
    direction: i32,
    state_ticks: u32,
    frame_count: u64,
    velocity_y: f32,
    frame: &'static Frame,
    blink_timer: i32,
    look_step: u8,
    look_timer: u32,
    lean: i32,
    speech: Option<String>,
    typed: usize,
    speech_ticks: u32,
    cloud: bool,
    paused: bool,
    acc: f64,
    last_time: f64,
    grab: Vec2,
    dragging: bool,
    moved: bool,
    ready: bool,
    applied_anchor: usize,
    ppp: f32,
    bubble: Option<Rect>,
    want_studio: bool,
    pub quit: bool,
    logged_mode: Mode,
    play: Play,
    kind: ItemKind,
    pose: ItemPose,
    fx: Fx,
    play_origin_x: f32,
    land: f32,
    dropped: Option<Dropped>,
    cc: Tracker,
    cc_anim: Option<(Cc, u32)>,
    cc_label: Option<&'static str>,
    cc_sign: Option<bool>,
    cc_burst: Option<u32>,
    lift: f32,
    bob: f32,
    stretch_frame: Option<usize>,
    crouch: Crouch,
    cc_rest: u32,
    pat: u32,
    hand: (i32, i32),
    guitar_frame: Option<usize>,
}

impl Pet {
    pub fn new() -> Self {
        Self {
            mode: Mode::Idle,
            rng: Rng::new(),
            x: 200.0,
            y: 200.0,
            win_w: 320.0,
            win_h: 250.0,
            body_h: 80.0,
            block: 10,
            buddy: false,
            crab_rel_x: 100.0,
            crab_rel_y: 150.0,
            work: Rect::from_min_max(Pos2::ZERO, Pos2::new(1920.0, 1080.0 - TASKBAR)),
            direction: 1,
            state_ticks: 0,
            frame_count: 0,
            velocity_y: 0.0,
            frame: &IDLE_STAND,
            blink_timer: 100,
            look_step: 0,
            look_timer: 0,
            lean: 0,
            speech: None,
            typed: 0,
            speech_ticks: 0,
            cloud: false,
            paused: false,
            acc: 0.0,
            last_time: 0.0,
            grab: Vec2::ZERO,
            dragging: false,
            moved: false,
            ready: false,
            applied_anchor: usize::MAX,
            ppp: 1.0,
            bubble: None,
            want_studio: false,
            quit: false,
            logged_mode: Mode::Idle,
            play: Play::Toss,
            kind: ItemKind::Custom,
            pose: ItemPose::default(),
            fx: Fx::None,
            play_origin_x: 0.0,
            land: 1.0,
            dropped: None,
            cc: Tracker::default(),
            cc_anim: None,
            cc_label: None,
            cc_sign: None,
            cc_burst: None,
            lift: 0.0,
            bob: 0.0,
            stretch_frame: None,
            crouch: Crouch::Off,
            cc_rest: 0,
            pat: 0,
            hand: (0, 0),
            guitar_frame: None,
        }
    }

    pub fn debug_state(&self) -> String {
        format!(
            "mode={:?} play={:?} pos=({:.0},{:.0}) win={}x{} dragging={} paused={} speech={}",
            self.mode,
            self.play,
            self.x,
            self.y,
            self.win_w,
            self.win_h,
            self.dragging,
            self.paused,
            self.speech.is_some()
        )
    }

    pub fn flags(&self) -> isize {
        (self.is_sleeping() as isize) | (self.paused as isize) << 1
    }

    pub fn is_sleeping(&self) -> bool {
        self.mode == Mode::Sleep
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn take_studio(&mut self) -> bool {
        std::mem::take(&mut self.want_studio)
    }

    fn layout(&mut self, block: i32, buddy: bool) {
        self.block = block;
        self.buddy = buddy;
        let b = block as f32;
        let crab_w = 12.0 * b;
        self.body_h = if buddy { 12.0 * b } else { 8.0 * b };
        self.win_w = (crab_w + 30.0 * b).max(320.0).floor();
        self.win_h = (self.body_h + 14.0 * b + 90.0).floor();
        self.crab_rel_x = ((self.win_w - crab_w) / 2.0).floor();
        self.crab_rel_y = self.win_h - self.body_h - 20.0;
    }

    fn floating(&self, core: &Core) -> bool {
        core.settings.anchor >= 2
    }

    fn ground(&self) -> f32 {
        self.work.max.y - self.win_h + 14.0
    }

    fn x_limits(&self) -> (f32, f32) {
        let b = self.block as f32;
        (
            self.work.min.x - (self.crab_rel_x - 2.0 * b),
            self.work.max.x - self.win_w + 3.0 * b,
        )
    }

    fn apply_anchor(&mut self, core: &Core) {
        let a = core.settings.anchor;
        let right = a == 0 || a == 2;
        let (lo, hi) = self.x_limits();
        self.x = if right { hi - 20.0 } else { lo + 20.0 };
        self.y = if a <= 1 {
            self.ground()
        } else {
            self.work.min.y
        };
        self.velocity_y = 0.0;
    }

    fn update_work_area(&mut self) {
        let ppp = self.ppp;
        if let Some((l, t, r, b)) = sys::work_area() {
            self.work = Rect::from_min_max(
                Pos2::new(l / ppp, t / ppp),
                Pos2::new(r / ppp, b / ppp),
            );
        }
    }

    pub fn window_pos(&self) -> (i32, i32) {
        (
            (self.x.round() * self.ppp).round() as i32,
            (self.y.round() * self.ppp).round() as i32,
        )
    }

    pub fn canvas_size(&self) -> (usize, usize) {
        (
            (self.win_w * self.ppp).round() as usize,
            (self.win_h * self.ppp).round() as usize,
        )
    }

    pub fn sync(&mut self, now: f64, ppp: f32, core: &Core) {
        let s = &core.settings;
        let block = ((5.0 * s.scale).round() as i32).max(2) * 2;
        let buddy = core.buddy_sprite().is_some();
        if ppp != self.ppp {
            self.ppp = ppp;
            self.update_work_area();
        }
        if !self.ready || self.frame_count % 30 == 0 {
            self.update_work_area();
        }

        if !self.ready {
            self.layout(block, buddy);
            self.apply_anchor(core);
            self.applied_anchor = s.anchor;
            self.last_time = now;
            self.ready = true;
            dlog!(
                "pet ready: work={:?} window={}x{} block={} ppp={}",
                self.work,
                self.win_w,
                self.win_h,
                self.block,
                ppp
            );
        } else if block != self.block || buddy != self.buddy {
            let old_h = self.win_h;
            let old_w = self.win_w;
            self.layout(block, buddy);
            if s.anchor <= 1 {
                self.y += old_h - self.win_h;
            }
            self.x += (old_w - self.win_w) / 2.0;
        }

        if s.anchor != self.applied_anchor {
            self.applied_anchor = s.anchor;
            self.apply_anchor(core);
        }

        let cursor = cursor_points(ppp);
        let dt = (now - self.last_time).clamp(0.0, 0.25);
        self.last_time = now;
        self.acc += dt * s.speed as f64;
        let mut steps = 0;
        while self.acc >= TICK && steps < 8 {
            self.acc -= TICK;
            steps += 1;
            self.step(core, cursor);
        }
        if self.mode != self.logged_mode {
            dlog!("pet mode {:?} -> {:?}", self.logged_mode, self.mode);
            self.logged_mode = self.mode;
        }
    }

    pub fn repaint_ms(&self, studio_open: bool, core: &Core) -> u64 {
        let typing = self
            .speech
            .as_ref()
            .map(|t| self.typed < t.chars().count())
            .unwrap_or(false);
        let item_moving = self.dropped.as_ref().map(|d| !d.body.asleep || d.back > 0).unwrap_or(false);
        if studio_open || self.dragging || self.mode == Mode::Fall {
            16
        } else if typing
            || item_moving
            || matches!(
                self.mode,
                Mode::Play | Mode::WalkLeft | Mode::WalkRight | Mode::Cowboy | Mode::Happy | Mode::Claude | Mode::Stretch
            )
        {
            33
        } else if self.mode == Mode::Sleep {
            100
        } else if matches!(core.settings.idle, 1 | 2) && self.mode == Mode::Idle {
            40
        } else {
            100
        }
    }

    fn has_item(core: &Core) -> bool {
        core.buddy_sprite().is_none() && core.settings.item_visible && core.item_sprite().is_some()
    }

    fn claude_busy(&self) -> bool {
        self.mode == Mode::Claude || self.cc.current().is_some() || self.cc_anim.is_some() || self.cc_rest > 0
    }

    pub fn trigger_cowboy(&mut self, core: &Core) {
        if self.claude_busy() {
            return;
        }
        if !core.settings.lasso {
            self.speak_random(core);
            return;
        }
        self.mode = Mode::Cowboy;
        self.state_ticks = 0;
        self.lean = 0;
        let quote = self.rng.pick(if core.settings.lang == 1 {
            COWBOY_QUOTES_EN
        } else {
            COWBOY_QUOTES
        });
        self.set_speech(core, quote);
    }

    pub fn trigger_stretch(&mut self) {
        if self.claude_busy() || matches!(self.mode, Mode::Drag | Mode::Fall) {
            return;
        }
        self.mode = Mode::Stretch;
        self.state_ticks = 0;
    }

    fn stretch_step(&mut self) {
        self.stretch_frame = seq_frame(0..30, self.state_ticks);
        if self.stretch_frame.is_some() {
            return;
        }
        self.frame = &IDLE_STAND;
        self.mode = Mode::Idle;
        self.state_ticks = 0;
    }

    pub fn trigger_look_around(&mut self) {
        if self.claude_busy() {
            return;
        }
        self.mode = Mode::LookAround;
        self.state_ticks = 0;
        self.look_step = 0;
        self.look_timer = 0;
    }

    fn trigger_happy(&mut self, _core: &Core) {
        self.mode = Mode::Happy;
        self.state_ticks = 0;
        self.lean = 0;
    }

    pub fn trigger_play(&mut self, core: &Core) -> bool {
        if self.claude_busy() || !Self::has_item(core) || matches!(self.mode, Mode::Drag | Mode::Fall) {
            return false;
        }
        self.kind = core.item_kind();
        let special = match self.kind {
            ItemKind::Bottle => Some(Play::Drink),
            ItemKind::Coffee => Some(Play::Sip),
            ItemKind::Flower => Some(Play::Sniff),
            ItemKind::Wand => Some(Play::Wave),
            ItemKind::Sword => Some(Play::Swing),
            ItemKind::Laptop => Some(Play::Type),
            ItemKind::Guitar => Some(Play::Rock),
            ItemKind::Custom => None,
        };
        let blk = self.block as f32;
        let room = self.x + (THROW_DIST + 1.0) * blk <= self.x_limits().1;
        let mut opts = vec![Play::Toss];
        if let Some(p) = special {
            opts.push(p);
            opts.push(p);
        }
        if room {
            opts.push(Play::Throw);
        }
        let pick = opts[(self.rng.next() % opts.len() as u64) as usize];
        self.start_play(core, pick);
        true
    }

    fn start_play(&mut self, core: &Core, play: Play) {
        self.play = play;
        self.mode = Mode::Play;
        self.state_ticks = 0;
        self.lean = 0;
        self.play_origin_x = self.x;
        self.land = land_of(core);
        self.dropped = None;
        dlog!("pet: play {:?} with {:?}", play, self.kind);
    }

    pub fn set_speech(&mut self, core: &Core, text: &str) {
        self.speech = Some(text.replace("{name}", &core.settings.name));
        self.cloud = false;
        self.typed = 0;
        self.speech_ticks = 0;
    }

    pub fn speak_random(&mut self, core: &Core) {
        if self.claude_busy() {
            return;
        }
        let en = core.settings.lang == 1;
        let quotes = match (self.mode == Mode::Cowboy, en) {
            (true, false) => COWBOY_QUOTES,
            (true, true) => COWBOY_QUOTES_EN,
            (false, false) => GENERAL_QUOTES,
            (false, true) => GENERAL_QUOTES_EN,
        };
        let quote = self.rng.pick(quotes);
        self.set_speech(core, quote);
        if !matches!(self.mode, Mode::Cowboy | Mode::Sleep | Mode::Play) {
            self.mode = Mode::Idle;
            self.frame = &IDLE_STAND;
            self.state_ticks = 0;
        }
    }

    pub fn toggle_sleep(&mut self, core: &Core) {
        if self.claude_busy() {
            return;
        }
        if self.mode == Mode::Sleep {
            return self.trigger_happy(core);
        }
        self.mode = Mode::Sleep;
        self.state_ticks = 0;
        self.lean = 0;
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    fn step(&mut self, core: &Core, cursor: Option<Pos2>) {
        self.frame_count += 1;
        self.state_ticks += 1;
        self.cc.expire(self.frame_count);
        self.cc_rest = self.cc_rest.saturating_sub(1);
        self.pat = self.pat.saturating_sub(1);
        if !matches!(self.mode, Mode::Idle | Mode::Sleep | Mode::Claude) && self.state_ticks > 1800 {
            self.mode = Mode::Idle;
            self.dragging = false;
            self.state_ticks = 0;
        }
        self.update_speech();
        self.drop_step(core);

        match self.mode {
            Mode::Idle => self.idle(core, cursor),
            Mode::LookAround => self.look_around(),
            Mode::WalkLeft | Mode::WalkRight => self.walk(core, cursor),
            Mode::Play => self.play_step(core),
            Mode::Claude => self.claude_step(core),
            Mode::Stretch => self.stretch_step(),
            Mode::Cowboy => {
                self.frame = &IDLE_STAND;
                self.lean = 0;
                if self.state_ticks > 140 {
                    self.mode = Mode::Idle;
                    self.state_ticks = 0;
                }
            }
            Mode::Happy => {
                let t = self.state_ticks;
                let cheer = t.saturating_sub(ANTICIPATION);
                self.frame = if t < ANTICIPATION {
                    &art::CROUCH
                } else if cheer < 4 * POSE_HOLD {
                    if (cheer / POSE_HOLD) % 2 == 0 { &art::CHEER } else { &IDLE_STAND }
                } else if cheer < 6 * POSE_HOLD {
                    &IDLE_BLINK
                } else {
                    &IDLE_STAND
                };
                self.lean = 0;
                if cheer > 7 * POSE_HOLD {
                    self.mode = Mode::Idle;
                    self.state_ticks = 0;
                }
            }
            Mode::Sleep => {
                self.frame = &art::SLEEP;
                self.lean = 0;
                if self.state_ticks > 600 && self.rng.float() < 0.005 {
                    self.trigger_happy(core);
                }
            }
            Mode::Drag => {
                self.frame = if (self.frame_count / 4) % 2 == 0 {
                    &art::DRAG_1
                } else {
                    &art::DRAG_2
                };
                self.lean = 0;
            }
            Mode::Fall => {
                self.frame = &art::FALL;
                self.velocity_y += 0.8;
                self.y += self.velocity_y;
                let ground = self.ground();
                if self.y >= ground {
                    self.y = ground;
                    self.velocity_y = 0.0;
                    self.trigger_happy(core);
                }
            }
        }
    }

    fn world(&self) -> World {
        let blk = self.block as f32;
        World {
            ground: 4.5,
            left: -self.crab_rel_x / blk - 12.5,
            right: (self.win_w - self.crab_rel_x) / blk - 12.5,
        }
    }

    fn drop_step(&mut self, core: &Core) {
        let release = self.mode == Mode::Sleep || self.claude_busy();
        if release && self.dropped.is_none() && Self::has_item(core) {
            if let Some(s) = core.item_sprite() {
                let (mut body, center) = Body::from_sprite(s, render::GRIP);
                let aside = self.mode == Mode::Claude;
                let desk = aside && core.item_kind() == ItemKind::Laptop;
                let px = (-12.0 - s.w as f32).max(self.world().left + render::GRIP.0 + 0.5);
                let to = if desk { (center.0 + LAPTOP_DESK.0, center.1 + LAPTOP_DESK.1) } else { (center.0 + px, center.1 + land_of(core) - 0.5) };
                if !aside {
                    body.vx = 0.12;
                    body.vy = -0.3;
                    body.spin = 0.16;
                }
                let carry = if aside && !desk { CARRY_TICKS } else { 0 };
                if desk {
                    (body.x, body.y) = to;
                }
                self.dropped = Some(Dropped { body, center, pose: ItemPose::default(), back: 0, carry, from: center, to, desk });
            }
        }
        let world = self.world();
        let Some(d) = self.dropped.as_mut() else {
            return;
        };
        if release {
            d.back = 0;
            if d.carry > 0 {
                d.carry -= 1;
                let k = smooth(1.0 - d.carry as f32 / CARRY_TICKS as f32);
                d.body.x = lerp(d.from.0, d.to.0, k);
                d.body.y = lerp(d.from.1, d.to.1, k) - (k * PI).sin() * 6.0;
            } else if !d.desk {
                d.body.step(&world);
            }
            d.pose = body_pose(&d.body, d.center);
        } else if self.mode == Mode::Play {
            self.dropped = None;
        } else {
            d.back += 1;
            if d.back >= pickup_ticks(d) {
                self.dropped = None;
            }
        }
    }

    fn dropped_pose(&self) -> Option<ItemPose> {
        let d = self.dropped.as_ref()?;
        let k = smooth(d.back as f32 / pickup_ticks(d) as f32);
        Some(ItemPose {
            dx: d.pose.dx * (1.0 - k),
            dy: d.pose.dy * (1.0 - k) - (k * PI).sin() * (1.0 + 0.4 * d.pose.dx.abs()),
            angle: d.pose.angle * (1.0 - k),
        })
    }

    fn walk_frame(&self) -> &'static Frame {
        art::WALK[((self.frame_count / WALK_FRAME_TICKS) % 8) as usize]
    }

    fn play_step(&mut self, core: &Core) {
        self.guitar_frame = None;
        self.stretch_frame = None;
        let t = self.state_ticks as f32;
        let blk = self.block as f32;
        let lean_px = (blk * 0.25) as i32;
        let mut pose = ItemPose::default();
        let mut fx = Fx::None;
        let mut frame: &'static Frame = &IDLE_STAND;
        let mut lean = 0;
        let end: f32;

        match self.play {
            Play::Toss => {
                end = 58.0;
                if t < 8.0 {
                    pose.dy = (t / 8.0 * PI).sin() * 0.8;
                    frame = if (2.0..7.0).contains(&t) { &art::CROUCH } else { &IDLE_STAND };
                } else if t < 44.0 {
                    let k = (t - 8.0) / 36.0;
                    pose.dy = -9.0 * 4.0 * k * (1.0 - k);
                    pose.angle = snap(k * TAU);
                    frame = if pose.dy < -3.0 { &LOOK_UP } else { &IDLE_STAND };
                } else {
                    let k = (t - 44.0) / 14.0;
                    pose.dy = (k * PI).sin() * 0.5;
                    frame = if (0.15..0.5).contains(&k) { &art::CROUCH } else { &IDLE_STAND };
                }
            }
            Play::Throw => {
                let d = THROW_DIST;
                let land = self.land;
                end = 94.0;
                if t < 10.0 {
                    let k = smooth(t / 10.0);
                    pose.angle = -0.8 * k;
                    pose.dx = -0.8 * k;
                    frame = &IDLE_LOOK_R;
                    lean = -lean_px;
                } else if t < 34.0 {
                    let k = (t - 10.0) / 24.0;
                    pose.dx = lerp(-0.8, d, k);
                    pose.dy = lerp(0.0, land, k) - 6.0 * 4.0 * k * (1.0 - k);
                    pose.angle = snap(lerp(-0.8, TAU, k));
                    frame = &IDLE_LOOK_R;
                    lean = lean_px;
                } else if t < 44.0 {
                    let k = (t - 34.0) / 10.0;
                    pose.dx = d + k;
                    pose.dy = land - (k * PI).sin() * 1.2;
                    frame = &IDLE_LOOK_R;
                } else if t < 52.0 {
                    pose.dx = d + 1.0;
                    pose.dy = land;
                    frame = &IDLE_LOOK_R;
                    if self.state_ticks == 44 && self.rng.float() < 0.6 {
                        let en = core.settings.lang == 1;
                        let q = self.rng.pick(if en { FETCH_QUOTES_EN } else { FETCH_QUOTES });
                        self.set_speech(core, q);
                    }
                } else if t < 80.0 {
                    let k = (t - 52.0) / 28.0;
                    let moved = (d + 1.0) * k;
                    self.x = self.play_origin_x + moved * blk;
                    pose.dx = d + 1.0 - moved;
                    pose.dy = land;
                    frame = self.walk_frame();
                } else {
                    let k = smooth((t - 80.0) / 10.0);
                    pose.dy = land * (1.0 - k);
                    frame = if k < 1.0 { &IDLE_BLINK } else { &IDLE_STAND };
                }
            }
            Play::Drink | Play::Sip | Play::Sniff => {
                let (dx, dy, ang, hold, effect) = match self.play {
                    Play::Drink => (-1.5, -1.5, -1.6, 48.0, Fx::Bubbles),
                    Play::Sip => (-1.5, -1.5, -1.0, 44.0, Fx::Steam),
                    _ => (-2.0, -1.5, -0.7, 40.0, Fx::Hearts),
                };
                end = 12.0 + hold + 12.0 + 6.0;
                let k = if t < 12.0 {
                    smooth(t / 12.0)
                } else if t < 12.0 + hold {
                    1.0
                } else {
                    1.0 - smooth((t - 12.0 - hold) / 12.0)
                };
                pose = ItemPose {
                    dx: dx * k,
                    dy: dy * k,
                    angle: ang * k,
                };
                if t >= 12.0 && t < 12.0 + hold {
                    frame = &IDLE_BLINK;
                    fx = effect;
                    if self.play != Play::Sniff && (self.state_ticks / 6) % 2 == 1 {
                        pose.dy += 0.25;
                    }
                }
            }
            Play::Wave => {
                end = 72.0;
                let raise = smooth(t / 8.0).min(1.0 - smooth((t - 62.0) / 10.0));
                pose.dy = -1.0 * raise;
                if t < 8.0 {
                    pose.angle = -0.2 * raise;
                } else if t < 48.0 {
                    pose.angle = -0.2 + 0.6 * ((t - 8.0) * 0.35).sin();
                    fx = Fx::Sparkles;
                    frame = &IDLE_LOOK_R;
                } else if t < 62.0 {
                    pose.angle = -0.2;
                    fx = Fx::Burst;
                    frame = &LOOK_UP;
                } else {
                    pose.angle = -0.2 * raise;
                }
            }
            Play::Type => {
                let (enter, exit) = (seq_ticks(1..10), seq_ticks(25..30));
                let st = self.state_ticks;
                end = (enter + TYPE_LOOP_TICKS + exit) as f32;
                self.stretch_frame = if st < enter {
                    seq_frame(1..10, st)
                } else if st < enter + TYPE_LOOP_TICKS {
                    fx = Fx::Code;
                    Some(crouch_frame(st - enter))
                } else {
                    seq_frame(25..30, st - enter - TYPE_LOOP_TICKS)
                };
            }
            Play::Rock => {
                let solo = GUITAR_TICKS * 30;
                end = (solo + seq_ticks(26..30)) as f32;
                if self.state_ticks < solo {
                    self.guitar_frame = Some((self.state_ticks / GUITAR_TICKS) as usize);
                } else {
                    self.stretch_frame = seq_frame(26..30, self.state_ticks - solo);
                }
            }
            Play::Swing => {
                end = 58.0;
                if t < 48.0 {
                    let c = t % 24.0;
                    if c < 10.0 {
                        pose.angle = -1.3 * smooth(c / 10.0);
                        lean = -lean_px;
                    } else if c < 15.0 {
                        pose.angle = lerp(-1.3, 1.5, (c - 10.0) / 5.0);
                        fx = Fx::Slash;
                        lean = lean_px;
                    } else {
                        pose.angle = lerp(1.5, 0.0, smooth((c - 15.0) / 9.0));
                        lean = if c < 18.0 { lean_px } else { 0 };
                    }
                    frame = &IDLE_LOOK_R;
                }
            }
        }

        self.pose = pose;
        self.fx = fx;
        self.frame = frame;
        self.lean = lean;

        if t >= end {
            self.pose = ItemPose::default();
            self.fx = Fx::None;
            self.mode = Mode::Idle;
            self.frame = &IDLE_STAND;
            self.state_ticks = 0;
            let en = core.settings.lang == 1;
            let quotes: &[&str] = match self.play {
                Play::Toss => {
                    if en {
                        TOSS_QUOTES_EN
                    } else {
                        TOSS_QUOTES
                    }
                }
                Play::Throw => &[],
                _ => self.kind.quotes(en),
            };
            if !quotes.is_empty() && self.rng.float() < 0.6 {
                let q = self.rng.pick(quotes);
                self.set_speech(core, q);
            }
        }
    }

    fn update_speech(&mut self) {
        let Some(len) = self.speech.as_ref().map(|t| t.chars().count()) else {
            return;
        };
        if self.typed < len {
            if self.frame_count % 2 == 0 {
                self.typed += 1;
            }
        } else {
            self.speech_ticks += 1;
            if self.speech_ticks > 170 {
                self.speech = None;
            }
        }
    }

    fn idle(&mut self, core: &Core, cursor: Option<Pos2>) {
        if self.cc.current().is_some() {
            return self.enter_claude();
        }
        if self.cc_rest > 0 {
            self.frame = &IDLE_STAND;
            return;
        }
        self.lean = 0;
        self.blink_timer -= 1;
        if self.blink_timer <= 0 {
            if self.blink_timer > -5 {
                self.frame = &IDLE_BLINK;
            } else {
                self.frame = &IDLE_STAND;
                self.blink_timer = self.rng.range(60, 160);
            }
        } else {
            self.frame = &IDLE_STAND;
        }

        if self.paused {
            return;
        }

        if core.settings.follow {
            if let Some(c) = cursor {
                let target = c.x - self.win_w / 2.0;
                if (target - self.x).abs() > 40.0 {
                    self.direction = if target < self.x { -1 } else { 1 };
                    self.start_walk();
                    return;
                }
            }
        }

        if self.state_ticks as i32 > self.rng.range(90, 200) {
            let dice = self.rng.float();
            if Self::has_item(core) {
                if dice < 0.30 {
                    self.direction = if self.rng.float() < 0.5 { -1 } else { 1 };
                    self.start_walk();
                } else if dice < 0.38 {
                    self.trigger_look_around();
                } else if dice < 0.44 {
                    self.trigger_stretch();
                } else if dice < 0.52 {
                    self.trigger_cowboy(core);
                } else if dice < 0.60 {
                    self.toggle_sleep(core);
                } else if dice < 0.68 {
                    self.speak_random(core);
                } else if dice < 0.95 {
                    self.trigger_play(core);
                } else {
                    self.state_ticks = 0;
                }
            } else if dice < 0.40 {
                self.direction = if self.rng.float() < 0.5 { -1 } else { 1 };
                self.start_walk();
            } else if dice < 0.55 {
                self.trigger_look_around();
            } else if dice < 0.65 {
                self.trigger_stretch();
            } else if dice < 0.80 {
                self.trigger_cowboy(core);
            } else if dice < 0.90 {
                self.toggle_sleep(core);
            } else if dice < 0.97 {
                self.speak_random(core);
            } else {
                self.state_ticks = 0;
            }
        }
    }

    pub fn claude_event(&mut self, core: &Core, ev: &str) {
        let (state, sid) = ev.split_once(':').unwrap_or((ev, "-"));
        let Some(cc) = Cc::parse(state) else {
            return;
        };
        if !self.cc.event(cc, sid, self.frame_count) {
            return;
        }
        let free = !matches!(self.mode, Mode::Drag | Mode::Fall);
        let idle = self.cc.current().is_none() && self.cc_anim.is_none();
        match cc {
            Cc::Hello if free && idle => {
                self.trigger_happy(core);
                self.set_speech(core, cc.label(core.settings.lang == 1));
                return;
            }
            Cc::Bye if free && idle => {
                self.mode = Mode::Sleep;
                self.state_ticks = 0;
                return;
            }
            Cc::Fail | Cc::Done if !self.cc.outranks(cc) => self.cc_anim = Some((cc, 0)),
            _ => {}
        }
        if free && (self.cc.current().is_some() || self.cc_anim.is_some()) {
            self.enter_claude();
        }
    }

    fn enter_claude(&mut self) {
        if self.mode != Mode::Claude {
            self.crouch = Crouch::Off;
            self.mode = Mode::Claude;
            self.state_ticks = 0;
            self.pose = ItemPose::default();
            self.lean = 0;
        }
    }

    fn claude_step(&mut self, core: &Core) {
        let en = core.settings.lang == 1;
        let f = self.frame_count;
        let cur = self.cc.current();
        self.lean = 0;
        self.lift = 0.0;
        self.bob = 0.0;
        self.fx = Fx::None;
        self.cc_label = None;
        self.cc_sign = None;
        self.cc_burst = None;
        self.hand = (0, 0);
        self.stretch_frame = None;
        let wants = self.cc_anim.is_none() && matches!(cur, Some(Cc::Work(_)));
        self.crouch = match (self.crouch, wants) {
            _ if matches!(self.cc_anim, Some((Cc::Fail, _))) => Crouch::Off,
            (Crouch::Off | Crouch::Exit(_), true) => Crouch::Enter(0),
            (Crouch::Enter(t), true) if seq_frame(1..10, t + 1).is_some() => Crouch::Enter(t + 1),
            (Crouch::Enter(_), true) => Crouch::Loop(0),
            (Crouch::Loop(t), true) => Crouch::Loop(t + 1),
            (Crouch::Enter(_) | Crouch::Loop(_), false) => Crouch::Exit(0),
            (Crouch::Exit(t), false) if seq_frame(25..30, t + 1).is_some() => Crouch::Exit(t + 1),
            (_, false) => Crouch::Off,
        };
        self.stretch_frame = match self.crouch {
            Crouch::Off => None,
            Crouch::Enter(t) => seq_frame(1..10, t),
            Crouch::Loop(t) => Some(crouch_frame(t)),
            Crouch::Exit(t) => seq_frame(25..30, t),
        };
        if matches!(self.crouch, Crouch::Exit(_)) {
            return;
        }

        if let Some((anim, k)) = self.cc_anim {
            self.cc_anim = Some((anim, k + 1));
            self.cc_label = (anim == Cc::Fail).then(|| anim.label(en));
            let end = if anim == Cc::Fail { self.fail_anim(k) } else { self.done_anim(core, k) };
            if end {
                self.cc_anim = None;
            }
            return;
        }
        match cur {
            None => {
                self.frame = &IDLE_STAND;
                self.mode = Mode::Idle;
                self.state_ticks = 0;
                self.cc_rest = CLAUDE_REST_TICKS;
            }
            Some(Cc::Think) => {
                self.frame = &LOOK_UP;
                self.hand = if (f / POSE_HOLD as u64) % 2 == 0 { (-2, -6) } else { (-1, -5) };
                self.cc_label = Some(THINK_DOTS[((f / 10) % 3) as usize]);
            }
            Some(Cc::Work(_)) => {
                self.cc_label = cur.map(|c| c.label(en));
            }
            Some(_) => {
                self.frame = &art::CHEER;
                self.bob = if (f / 15) % 2 == 1 { 1.0 } else { 0.0 };
                self.cc_sign = Some(false);
            }
        }
    }

    fn fail_anim(&mut self, k: u32) -> bool {
        if k < 2 * POSE_HOLD {
            self.frame = &art::SHOCK;
            self.lift = if (1..6).contains(&k) { -1.0 } else { 0.0 };
        } else if k < 5 * POSE_HOLD {
            self.frame = if ((k / 6) % 2) == 0 { &IDLE_LOOK_L } else { &IDLE_LOOK_R };
        } else {
            self.frame = &IDLE_STAND;
        }
        k >= 6 * POSE_HOLD
    }

    fn done_anim(&mut self, core: &Core, k: u32) -> bool {
        let jump = 16;
        let land = ANTICIPATION + jump;
        let cheer_end = land + 6 + 36;
        if k < ANTICIPATION {
            self.frame = &art::CROUCH;
        } else if k < land {
            let q = (k - ANTICIPATION) as f32 / jump as f32;
            self.lift = (-3.0 * 4.0 * q * (1.0 - q)).round();
            self.frame = &art::CHEER;
            if k >= ANTICIPATION + 4 {
                self.cc_burst = Some(k - ANTICIPATION - 4);
            }
        } else if k < land + 6 {
            self.frame = &art::CROUCH;
        } else if k < cheer_end {
            self.frame = &art::CHEER;
            self.cc_sign = Some(true);
        } else {
            self.frame = &IDLE_STAND;
            if k == cheer_end {
                self.set_speech(core, Cc::Done.label(core.settings.lang == 1));
                self.cloud = true;
            }
        }
        k >= cheer_end + POSE_HOLD
    }

    fn start_walk(&mut self) {
        self.mode = if self.direction < 0 {
            Mode::WalkLeft
        } else {
            Mode::WalkRight
        };
        self.state_ticks = 0;
    }

    fn look_around(&mut self) {
        self.look_timer += 1;
        let lean = (self.block as f32 * 0.25) as i32;
        match self.look_step {
            0 => {
                self.frame = &IDLE_LOOK_L;
                self.lean = -lean;
                if self.look_timer > 22 {
                    self.look_step = 1;
                    self.look_timer = 0;
                }
            }
            1 => {
                self.frame = &IDLE_BLINK;
                self.lean = 0;
                if self.look_timer > 6 {
                    self.look_step = 2;
                    self.look_timer = 0;
                }
            }
            2 => {
                self.frame = &IDLE_LOOK_R;
                self.lean = lean;
                if self.look_timer > 22 {
                    self.look_step = 3;
                    self.look_timer = 0;
                }
            }
            _ => {
                self.frame = &IDLE_BLINK;
                self.lean = 0;
                if self.look_timer > 6 {
                    self.mode = Mode::Idle;
                    self.state_ticks = 0;
                    self.frame = &IDLE_STAND;
                }
            }
        }
    }

    fn walk(&mut self, core: &Core, cursor: Option<Pos2>) {
        if self.paused {
            self.mode = Mode::Idle;
            return;
        }
        self.lean = 0;

        let mut following = false;
        if core.settings.follow {
            if let Some(c) = cursor {
                following = true;
                let target = c.x - self.win_w / 2.0;
                if (target - self.x).abs() <= 12.0 {
                    self.mode = Mode::Idle;
                    self.state_ticks = 0;
                    self.frame = &IDLE_STAND;
                    return;
                }
                self.direction = if target < self.x { -1 } else { 1 };
                self.mode = if self.direction < 0 {
                    Mode::WalkLeft
                } else {
                    Mode::WalkRight
                };
            }
        }

        if self.state_ticks < ANTICIPATION {
            self.frame = &art::CROUCH;
            return;
        }
        self.x += self.direction as f32 * WALK_SPEED;
        self.frame = self.walk_frame();

        let (lo, hi) = self.x_limits();
        if self.x < lo {
            self.x = lo;
            self.direction = 1;
            self.mode = Mode::WalkRight;
        } else if self.x > hi {
            self.x = hi;
            self.direction = -1;
            self.mode = Mode::WalkLeft;
        }

        if !following && self.state_ticks as i32 > self.rng.range(120, 280) {
            self.mode = Mode::Idle;
            self.state_ticks = 0;
            self.frame = &IDLE_STAND;
        }
    }

    pub fn frame(&mut self, input: &Input, canvas: &Rc<RefCell<Canvas>>, now: f64, core: &Core) {
        if !self.ready {
            return;
        }
        self.handle_input(input, core);
        self.paint(canvas, now, core);
    }

    fn handle_input(&mut self, input: &Input, core: &Core) {
        if self.dragging && !input.dragging && !input.drag_stopped {
            self.dragging = false;
            self.drop_pet(core);
        }
        if input.secondary_clicked {
            dlog!("pet: right click, studio requested");
            self.want_studio = true;
        }

        if input.drag_started {
            if let Some(c) = cursor_points(self.ppp) {
                self.grab = c - Pos2::new(self.x, self.y);
                self.dragging = true;
                dlog!("pet: drag started");
                self.moved = false;
                self.velocity_y = 0.0;
            }
        }

        if self.dragging && input.dragging {
            if let Some(c) = cursor_points(self.ppp) {
                let nx = c.x - self.grab.x;
                let ny = c.y - self.grab.y;
                if (nx - self.x).abs() > 0.5 || (ny - self.y).abs() > 0.5 {
                    self.moved = true;
                    self.mode = Mode::Drag;
                    self.x = nx;
                    self.y = ny;
                }
            }
        }

        if self.dragging && input.drag_stopped {
            self.dragging = false;
            dlog!("pet: drag stopped");
            self.drop_pet(core);
            return;
        }

        if input.double_clicked && !self.claude_busy() {
            let dice = self.rng.float();
            if Self::has_item(core) && dice < 0.45 {
                self.trigger_play(core);
            } else if dice < 0.65 {
                self.trigger_cowboy(core);
            } else {
                self.speak_random(core);
            }
        } else if input.clicked {
            let local = cursor_points(self.ppp).map(|c| c - Pos2::new(self.x, self.y).to_vec2());
            let on_bubble = match (local, self.bubble) {
                (Some(p), Some(r)) => r.contains(p),
                _ => false,
            };
            if on_bubble {
                let len = self.speech.as_ref().map(|t| t.chars().count()).unwrap_or(0);
                if self.typed < len {
                    self.typed = len;
                } else {
                    self.speech = None;
                }
            } else if self.mode == Mode::Claude {
                self.pat = PAT_TICKS;
            } else if self.mode != Mode::Play {
                self.trigger_happy(core);
            }
        }
    }

    fn drop_pet(&mut self, core: &Core) {
        self.pose = ItemPose::default();
        self.fx = Fx::None;
        if !self.moved {
            if self.mode != Mode::Play {
                self.trigger_happy(core);
            }
            return;
        }
        if self.floating(core) {
            self.trigger_happy(core);
            return;
        }
        let ground = self.ground();
        if self.y < ground - 6.0 {
            self.mode = Mode::Fall;
            self.velocity_y = 1.0;
        } else {
            self.y = ground;
            self.trigger_happy(core);
        }
    }

    fn paint(&mut self, canvas: &Rc<RefCell<Canvas>>, now: f64, core: &Core) {
        let s = &core.settings;
        let brush = Brush::canvas(canvas.clone(), self.ppp);
        let (ox, oy) = self.window_pos();
        crate::glass::set_origin(ox, oy);
        let blk = self.block as f32;
        let t = now as f32 * s.speed;
        let (mx, my, sq) = if self.mode == Mode::Idle {
            render::idle_motion(s.idle, t)
        } else if std::ptr::eq(self.frame, art::WALK[2]) || std::ptr::eq(self.frame, art::WALK[6]) {
            (0.0, 0.0, 0.5)
        } else if self.mode == Mode::Claude {
            (0.0, self.lift, self.bob)
        } else if self.mode == Mode::Stretch {
            (0.0, 0.0, self.bob)
        } else {
            (0.0, 0.0, 0.0)
        };
        let head = self.frame.iter().position(|r| r.contains('#')).unwrap_or(0) as f32 / 2.0;
        let sprite: Option<&[&str]> = match (self.mode, self.stretch_frame, self.guitar_frame) {
            (Mode::Play, _, Some(i)) => Some(&art::GUITAR_FRAMES[i][..]),
            (Mode::Stretch | Mode::Claude | Mode::Play, Some(i), _) => Some(&art::STRETCH_FRAMES[i][..]),
            _ => None,
        };
        let (head_dx, head_dy, hand_dx, hand_dy) = sprite.map(sprite_anchor).unwrap_or((0.0, 0.0, 0.0, 0.0));
        let (head, hand_base) = match sprite {
            Some(_) => (0.0, 0.0),
            None => (head, hand_row(self.frame, 11).unwrap_or(2.0) - 2.0),
        };
        let body = head + sq + head_dy;
        let grip = hand_base + sq + hand_dy;
        let grip_dx = hand_dx;
        let crab_w = 12.0 * blk;
        let x = self.crab_rel_x + self.lean as f32 + mx * blk;
        let y = self.crab_rel_y + my * blk;

        if self.mode != Mode::Fall && self.mode != Mode::Drag {
            let sh = blk.max(4.0);
            render::draw_shadow(
                &brush,
                self.crab_rel_x + blk + self.lean as f32,
                self.crab_rel_y + self.body_h - sh / 2.0,
                crab_w - 2.0 * blk,
                sh,
            );
        }

        let hat = if s.hat_visible { core.hat_sprite() } else { None };
        match core.buddy_sprite() {
            Some(sprite) => {
                render::draw_buddy(&brush, sprite, x, y, crab_w, self.direction < 0, sq * blk);
            }
            None => {
                let frame_laptop = sprite.map_or(false, |rows| rows.iter().any(|r| r.contains('g')));
                let desk = self.dropped.as_ref().map_or(false, |d| d.desk)
                    && match self.crouch {
                        Crouch::Enter(_) | Crouch::Loop(_) => true,
                        Crouch::Exit(_) => frame_laptop,
                        Crouch::Off => false,
                    };
                let own = frame_laptop && core.item_kind() == ItemKind::Laptop;
                let lying = self.dropped_pose().filter(|_| self.mode != Mode::Play && s.item_visible && !desk && !own);
                if let (Some(item), Some(pose)) = (core.item_sprite(), lying) {
                    render::draw_item_of(&brush, core.item_kind(), item, x, self.crab_rel_y, blk, pose);
                }
                match sprite {
                    Some(rows) => render::draw_sprite_frame(&brush, x, y, blk, rows),
                    None => render::draw_crab_posed(&brush, x, y, blk, self.frame, sq * blk, 0.0, [(0, 0), if self.mode == Mode::Claude { self.hand } else { (0, 0) }], 0),
                }
                if let Some(h) = hat {
                    let extra = (body + s.hat_nudge.round() + core.hat_drop()) * blk;
                    render::draw_hat(&brush, h, x + head_dx * blk, y, blk, extra);
                }
                if self.mode == Mode::Claude && self.hand != (0, 0) {
                    let px = blk / 2.0;
                    let (hx, hy) = (x + (20 + self.hand.0) as f32 * px, y + (4 + self.hand.1) as f32 * px);
                    brush.rect(hx, hy, 4.0 * px, 4.0 * px, render::claw_color(20));
                }
                if self.mode == Mode::Cowboy {
                    render::draw_lasso(&brush, x, y + grip * blk, blk, self.frame_count);
                } else if s.item_visible && !(self.mode == Mode::Play && self.guitar_frame.is_some()) {
                    if let Some(item) = core.item_sprite() {
                        let dropped = self.dropped_pose();
                        let (kind, pose, fx, tick) = if self.mode == Mode::Play {
                            (self.kind, self.pose, self.fx, self.state_ticks)
                        } else if let Some(pose) = dropped {
                            (core.item_kind(), pose, Fx::None, 0)
                        } else {
                            let kind = core.item_kind();
                            let f = self.frame_count;
                            let mut pose = ItemPose::default();
                            if kind == ItemKind::Custom && (f / 5) % 24 == 0 {
                                pose.dy = -1.0;
                            }
                            (kind, pose, Fx::Ambient, f as u32)
                        };
                        let pose = if dropped.is_some() && self.mode != Mode::Play {
                            pose
                        } else {
                            ItemPose { dx: pose.dx + grip_dx, dy: pose.dy + grip, ..pose }
                        };
                        let own_laptop = (self.mode == Mode::Play || kind == ItemKind::Laptop) && sprite.map_or(false, |rows| rows.iter().any(|r| r.contains('g')));
                        if lying.is_none() && !own_laptop && !desk {
                            render::draw_item_of(&brush, kind, item, x, y, blk, pose);
                            render::draw_item_fx(&brush, kind, fx, x, y, blk, pose, tick);
                        }
                    }
                }
                if let Some(check) = self.cc_sign {
                    let hand = hand_row(self.frame, 0).unwrap_or(2.0) + sq;
                    render::draw_sign(&brush, x, y + hand * blk, blk, check, s.glass);
                }
                if self.pat > 0 && self.mode == Mode::Claude {
                    let pose = ItemPose { dx: -7.5 + head_dx, dy: -0.5 + body, angle: 0.0 };
                    render::draw_item_fx(&brush, ItemKind::Custom, Fx::Hearts, x, y, blk, pose, PAT_TICKS - self.pat);
                }
                if let Some(b) = self.cc_burst {
                    let pose = ItemPose { dx: -10.5, dy: -2.5, angle: 0.0 };
                    render::draw_item_fx(&brush, ItemKind::Custom, Fx::Burst, x, y, blk, pose, b);
                }
            }
        }

        self.bubble = None;
        let snore = match self.mode {
            _ if self.speech.is_some() => None,
            Mode::Claude => self.cc_label,
            _ => None,
        };
        let sleep_cloud = s.glass && self.mode == Mode::Sleep && self.speech.is_none();
        if self.mode == Mode::Sleep && self.speech.is_none() && core.buddy_sprite().is_none() && !s.glass {
            let hat_top = if hat.is_some() { 4.0 * blk } else { 0.0 };
            let ax = x + crab_w / 2.0 + head_dx * blk + 3.0 * blk;
            let ay = y + body * blk - hat_top - blk;
            render::draw_zzz(&brush, ax, ay, blk, self.frame_count);
        }
        if sleep_cloud {
            let lift = if hat.is_some() && core.buddy_sprite().is_none() { 6.0 * blk } else { 16.0 };
            self.bubble = render::draw_bubble(
                &brush,
                "Zzz",
                1 + ((self.frame_count / 20) % 3) as usize,
                (x + crab_w / 2.0 + head_dx * blk) as i32,
                (y + body * blk - lift) as i32,
                self.win_w as i32,
                true,
                true,
            );
        } else if let Some(z) = snore {
            let lift = if hat.is_some() && core.buddy_sprite().is_none() {
                6.0 * blk
            } else {
                16.0
            };
            self.bubble = render::draw_bubble(
                &brush,
                z,
                z.chars().count(),
                (x + crab_w / 2.0 + head_dx * blk) as i32,
                (y + body * blk - lift) as i32,
                self.win_w as i32,
                s.glass,
                matches!(self.cc_anim, Some((Cc::Fail, _))),
            );
        } else if let Some(text) = &self.speech {
            if self.typed > 0 {
                let lift = if hat.is_some() && core.buddy_sprite().is_none() {
                    7.0 * blk
                } else {
                    16.0
                };
                self.bubble = render::draw_bubble(
                    &brush,
                    text,
                    self.typed,
                    (x + crab_w / 2.0 + head_dx * blk) as i32,
                    (y + body * blk - lift) as i32,
                    self.win_w as i32,
                    s.glass,
                    self.cloud,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn render(_ctx: &(), pet: &mut Pet, core: &Core, w: u32, h: u32) -> image::RgbaImage {
        let canvas = Rc::new(RefCell::new(Canvas::new(w as usize, h as usize)));
        pet.ppp = 1.0;
        pet.paint(&canvas, 0.0, core);
        let c = canvas.borrow();
        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([46, 52, 64, 255]));
        for (i, &p) in c.px.iter().enumerate() {
            if p >> 24 != 0 {
                let (x, y) = ((i % c.w) as u32, (i / c.w) as u32);
                img.put_pixel(x, y, image::Rgba([(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]));
            }
        }
        img
    }
    const ORIGINAL: [(&str, [&str; 8]); 5] = [
        ("IDLE_STAND", ["..########..", "..#E####E#..", "############", "############", "..########..", "..########..", "..#.#..#.#..", "..#.#..#.#.."]),
        ("IDLE_BLINK", ["..########..", "..########..", "############", "############", "..########..", "..########..", "..#.#..#.#..", "..#.#..#.#.."]),
        ("IDLE_LOOK_L", ["..########..", "..E####E##..", "############", "############", "..########..", "..########..", "..#.#..#.#..", "..#.#..#.#.."]),
        ("IDLE_LOOK_R", ["..########..", "..##E####E..", "############", "############", "..########..", "..########..", "..#.#..#.#..", "..#.#..#.#.."]),
        ("SLEEP", ["............", "..########..", "..#-####-#..", "############", "############", "..########..", "..#.#..#.#..", "..#.#..#.#.."]),
    ];

    fn doubled(rows: &[&str; 8]) -> Vec<String> {
        rows.iter()
            .flat_map(|r| {
                let w: String = r.chars().flat_map(|c| [c, c]).collect();
                [w.clone(), w]
            })
            .collect()
    }

    #[test]
    fn rest_frames_are_exact_2x_of_original_design() {
        let fine: [&Frame; 5] = [&IDLE_STAND, &IDLE_BLINK, &IDLE_LOOK_L, &IDLE_LOOK_R, &art::SLEEP];
        for ((name, coarse), f) in ORIGINAL.iter().zip(fine) {
            assert_eq!(f.iter().map(|r| r.to_string()).collect::<Vec<_>>(), doubled(coarse), "{name} differs from the original design");
        }
    }

    #[test]
    fn crouch_poses_keep_width_and_legs() {
        let width = |f: &Frame| {
            let cols: Vec<usize> = (0..24).filter(|&c| f.iter().any(|r| r.as_bytes()[c] == b'#')).collect();
            cols.last().unwrap() - cols[0] + 1
        };
        let filled = |f: &Frame| f.iter().map(|r| r.bytes().filter(|b| *b == b'#').count()).sum::<usize>();
        for f in [&art::CROUCH] {
            assert_eq!(width(f), 24);
            assert!(filled(f) <= filled(&IDLE_STAND));
            assert_eq!(f[15], IDLE_STAND[15]);
        }
        for f in art::WALK {
            assert_eq!(&f[..12], &IDLE_STAND[..12]);
        }
    }

    #[test]
    fn stretch_timing_matches_reference_ratio() {
        let total: Vec<u32> = (0..3).map(|p| stretch_ticks(p) * STRETCH_PHASE.iter().filter(|&&q| q == p).count() as u32).collect();
        assert!(total[1] > total[0] && total[1] > total[2]);
        assert!(STRETCH_PHASE.iter().all(|&p| stretch_ticks(p) >= 4));
    }

    #[test]
    fn traced_last_frame_is_idle_stand() {
        let last = &art::STRETCH_FRAMES[29];
        for (r, row) in last.iter().enumerate() {
            let inner = if (8..24).contains(&r) { IDLE_STAND[r - 8] } else { "........................" };
            assert_eq!(&row[2..26], inner, "row {r}");
            assert!(row[..2].bytes().chain(row[26..].bytes()).all(|b| b == b'.'));
        }
    }

    #[test]
    fn guitar_frames_use_palette() {
        for (i, f) in art::GUITAR_FRAMES.iter().enumerate() {
            assert!(f.iter().all(|r| r.len() == 44 && r.bytes().all(|b| b".#dEkbwyn".contains(&b))), "guitar frame {}", i + 1);
            assert!(f[27].bytes().any(|b| b == b'#' || b == b'd'), "guitar frame {} has no feet", i + 1);
        }
        let (hx, hy, _, _) = sprite_anchor(&IDLE_PADDED);
        assert_eq!((hx, hy), (0.0, 0.0));
        for (i, f) in art::GUITAR_FRAMES.iter().enumerate() {
            let (_, hy, _, _) = sprite_anchor(&f[..]);
            let eye = f.iter().position(|r| r.contains('E')).unwrap_or(99) as f32;
            assert!(hy * 2.0 + 12.0 + 2.0 <= eye, "guitar frame {}: hat would cover the eyes", i + 1);
        }
    }

    const IDLE_PADDED: [&str; 28] = [
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "............................................",
        "......################......................",
        "......################......................",
        "......##EE########EE##......................",
        "......##EE########EE##......................",
        "..########################..................",
        "..########################..................",
        "..########################..................",
        "..########################..................",
        "......################......................",
        "......################......................",
        "......################......................",
        "......################......................",
        "......##..##....##..##......................",
        "......##..##....##..##......................",
        "......##..##....##..##......................",
        "......##..##....##..##......................",
    ];

    #[test]
    fn sleep_zs_stay_in_window_and_grow() {
        for tick in 0..240u64 {
            for (x, y, _) in render::zzz_cells(tick) {
                assert!((-2..=24).contains(&x) && (-38..=1).contains(&y), "tick {tick}: cell {x},{y}");
            }
        }
        let width = |t: u64| {
            let xs: Vec<i32> = render::zzz_cells(t).iter().filter(|c| !c.2).map(|c| c.0).collect();
            xs.iter().max().unwrap() - xs.iter().min().unwrap()
        };
        assert!(width(0) > 0);
        let lone: Vec<_> = render::zzz_cells(0).into_iter().filter(|c| !c.2 && c.1 > -9).collect();
        assert!(!lone.is_empty(), "newest Z starts near the head");
    }

    #[test]
    fn traced_frames_are_clean() {
        let body = |f: &[&str; 24], r: usize, c: usize| matches!(f[r].as_bytes()[c], b'#' | b'd');
        for (i, f) in art::STRETCH_FRAMES.iter().enumerate() {
            assert!(f.iter().all(|r| r.len() == 40 && r.bytes().all(|b| b".#dEg".contains(&b))), "frame {} palette", i + 1);
            for r in 1..20 {
                if !f[r].contains('E') {
                    continue;
                }
                for c in 1..39 {
                    let open_eye = f[r].as_bytes()[c] == b'.' && body(f, r - 1, c) && body(f, r + 1, c) && (body(f, r, c - 1) || body(f, r, c + 1));
                    assert!(!open_eye, "frame {} transparent eye notch at {r},{c}", i + 1);
                }
            }
            for r in 0..24 {
                for c in 0..40 {
                    if !body(f, r, c) {
                        continue;
                    }
                    let in_square = [(0, 0), (0, 1), (1, 0), (1, 1)].iter().any(|&(dr, dc)| {
                        let (r0, c0) = (r as i32 - dr, c as i32 - dc);
                        r0 >= 0 && c0 >= 0 && r0 < 23 && c0 < 39 && (0..2).all(|a| (0..2).all(|b| {
                            let (rr, cc) = ((r0 + a) as usize, (c0 + b) as usize);
                            body(f, rr, cc) || f[rr].as_bytes()[cc] == b'E'
                        }))
                    });
                    assert!(in_square, "frame {} loose pixel at {r},{c}", i + 1);
                }
            }
        }
    }

    #[test]
    fn laptop_item_matches_traced_laptop() {
        let cells = |rows: &[&str], ch: char| -> Vec<(i32, i32)> {
            let all: Vec<(i32, i32)> = rows.iter().enumerate().flat_map(|(r, row)| row.chars().enumerate().filter(move |&(_, c)| c == ch).map(move |(c, _)| (c as i32, r as i32))).collect();
            let (x0, y0) = (all.iter().map(|p| p.0).min().unwrap_or(0), all.iter().map(|p| p.1).min().unwrap_or(0));
            all.into_iter().map(|(x, y)| (x - x0, y - y0)).collect()
        };
        let item = cells(art::LAPTOP_FINE.rows, 'G');
        for n in [8, 9, 11, 13, 14, 17, 18, 19, 20, 21, 22, 23] {
            assert_eq!(cells(&art::STRETCH_FRAMES[n - 1], 'g'), item, "frame {n}");
        }
    }

    #[test]
    fn claude_states_block_items_and_tricks() {
        let mut core = Core::load(None);
        core.settings.item_visible = true;
        core.item = core.slots.iter().position(|s| s.name == "Kaffee");
        let mut pet = Pet::new();
        pet.layout(10, false);
        pet.ready = true;
        pet.work = Rect::from_min_max(Pos2::ZERO, Pos2::new(4000.0, 1000.0));
        pet.x = 500.0;
        assert!(pet.dropped.is_none());
        pet.claude_event(&core, "think:t");
        for i in 0..600 {
            match i {
                150 => pet.claude_event(&core, "edit:t"),
                300 => pet.claude_event(&core, "ask:t"),
                450 => pet.claude_event(&core, "fail:t"),
                _ => {}
            }
            pet.step(&core, None);
            assert!(pet.mode == Mode::Claude, "left Claude mode at tick {i}: {:?}", pet.mode);
            assert!(pet.dropped.is_some(), "item in hand at tick {i}");
            assert!(!pet.trigger_play(&core));
            pet.trigger_cowboy(&core);
            pet.trigger_look_around();
            pet.trigger_stretch();
            pet.speak_random(&core);
            pet.toggle_sleep(&core);
            assert!(pet.mode == Mode::Claude && pet.speech.is_none(), "trick started at tick {i}");
        }
        pet.claude_event(&core, "done:t");
        let mut ticks = 0;
        while (pet.mode == Mode::Claude || pet.dropped.is_some()) && ticks < 1200 {
            pet.step(&core, None);
            ticks += 1;
        }
        assert!(pet.mode != Mode::Claude && pet.dropped.is_none(), "item not picked up after {ticks} ticks");
    }

    #[test]
    #[ignore]
    fn guitar_sheet() {
        let ctx = ();
        let mut core = Core::load(None);
        core.settings.item_visible = true;
        core.item = core.slots.iter().position(|s| s.name == "Gitarre");
        let (tw, th) = (420u32, 310u32);
        for (name, hat) in [("guitar_nohat.png", None), ("guitar_hat.png", core.slots.iter().position(|s| s.name == "Bauhelm"))] {
            core.hat = hat;
            core.settings.hat_visible = hat.is_some();
            let mut sheet = image::RgbaImage::from_pixel(tw * 30, th, image::Rgba([24, 24, 24, 255]));
            let mut pet = Pet::new();
            pet.layout(10, false);
            pet.ready = true;
            pet.kind = core.item_kind();
            pet.start_play(&core, Play::Rock);
            for i in 0..30u32 {
                while pet.state_ticks < i * GUITAR_TICKS + 1 {
                    pet.step(&core, None);
                }
                let tile = render(&ctx, &mut pet, &core, tw, th);
                image::imageops::overlay(&mut sheet, &tile, (i * tw) as i64, 0);
            }
            sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join(name)).unwrap();
        }
    }

    #[test]
    #[ignore]
    fn stretch_sheet() {
        let ctx = ();
        let mut core = Core::load(None);
        core.item = None;
        core.hat = None;
        let (tw, th) = (420u32, 310u32);
        let mut sheet = image::RgbaImage::from_pixel(tw * 30, th, image::Rgba([24, 24, 24, 255]));
        let mut pet = Pet::new();
        pet.layout(10, false);
        pet.ready = true;
        pet.trigger_stretch();
        for (i, &phase) in STRETCH_PHASE.iter().enumerate() {
            pet.step(&core, None);
            let tile = render(&ctx, &mut pet, &core, tw, th);
            image::imageops::overlay(&mut sheet, &tile, (i as u32 * tw) as i64, 0);
            for _ in 1..stretch_ticks(phase) {
                pet.step(&core, None);
            }
        }
        sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("stretch.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn design_compare() {
        let pairs: [(&[&str; 8], &Frame); 6] = [
            (&ORIGINAL[0].1, &IDLE_STAND),
            (&ORIGINAL[0].1, art::WALK[1]),
            (&["..########..", "..#E####E#..", "############", "############", "..########..", "..########..", ".#..#..#..#.", ".#..#..#..#."], art::WALK[2]),
            (&ORIGINAL[0].1, art::WALK[5]),
            (&["..########..", "..#E####E#..", "############", "############", "..########..", "..########..", "..#..##..#..", "..#..##..#.."], art::WALK[6]),
            (&ORIGINAL[0].1, &art::CROUCH),
        ];
        let blk = 20.0;
        let (tw, th) = (2 * 12 * 20 + 60, 8 * 20 + 40);
        let canvas = Rc::new(RefCell::new(Canvas::new(tw, th * pairs.len())));
        let brush = Brush::canvas(canvas.clone(), 1.0);
        for (i, (old, new)) in pairs.iter().enumerate() {
            let y = (i * th + 20) as f32;
            for (r, row) in old.iter().enumerate() {
                for (c, ch) in row.chars().enumerate() {
                    let col = match ch {
                        '#' => art::CRAB,
                        'E' => art::EYE,
                        _ => continue,
                    };
                    brush.rect(20.0 + c as f32 * blk, y + r as f32 * blk, blk, blk, col);
                }
            }
            render::draw_crab(&brush, 40.0 + 12.0 * blk, y, blk, new, 0.0);
        }
        let c = canvas.borrow();
        let mut img = image::RgbaImage::from_pixel(c.w as u32, c.h as u32, image::Rgba([24, 24, 24, 255]));
        for (i, &p) in c.px.iter().enumerate() {
            if p >> 24 != 0 {
                img.put_pixel((i % c.w) as u32, (i / c.w) as u32, image::Rgba([(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]));
            }
        }
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("design_compare.png")).unwrap();
    }

    fn coding_pet(core: &mut Core, item: &str) -> Pet {
        core.settings.item_visible = true;
        core.item = core.slots.iter().position(|s| s.name == item);
        let mut pet = Pet::new();
        pet.layout(10, false);
        pet.ready = true;
        pet.work = Rect::from_min_max(Pos2::ZERO, Pos2::new(4000.0, 1000.0));
        pet.x = 500.0;
        pet.kind = core.item_kind();
        pet.claude_event(core, "edit:test");
        pet
    }

    fn laptops(img: &image::RgbaImage) -> usize {
        let (w, h) = (img.width() as i32, img.height() as i32);
        let grey = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && img.get_pixel(x as u32, y as u32).0[..3] == [126, 126, 126];
        let mut seen = vec![false; (w * h) as usize];
        let mut blobs = 0;
        for y in 0..h {
            for x in 0..w {
                if !grey(x, y) || seen[(y * w + x) as usize] {
                    continue;
                }
                let (mut stack, mut n) = (vec![(x, y)], 0);
                seen[(y * w + x) as usize] = true;
                while let Some((cx, cy)) = stack.pop() {
                    n += 1;
                    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                        let (nx, ny) = (cx + dx, cy + dy);
                        if grey(nx, ny) && !seen[(ny * w + nx) as usize] {
                            seen[(ny * w + nx) as usize] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                if n >= 20 {
                    blobs += 1;
                }
            }
        }
        blobs
    }

    #[test]
    fn clothing_stays_on_body() {
        let px = |c: &Canvas, x: usize, y: usize| c.px[y * c.w + x] & 0xFFFFFF;
        let mut front = crate::sprite::Sprite::new(crate::sprite::CLOTH_W, crate::sprite::CLOTH_H);
        front.set(0, 0, [0, 0, 255, 255]);
        front.set(5, 5, [0, 0, 255, 255]);
        let mut back = crate::sprite::Sprite::new(crate::sprite::CLOTH_W, crate::sprite::CLOTH_H);
        back.set(1, 0, [0, 255, 0, 255]);
        render::set_outfit(vec![(front, false), (back, true)]);
        let canvas = Rc::new(RefCell::new(Canvas::new(48, 32)));
        render::draw_crab(&Brush::canvas(canvas.clone(), 1.0), 0.0, 0.0, 2.0, &IDLE_STAND, 0.0);
        render::set_outfit(Vec::new());
        let c = canvas.borrow();
        assert_eq!(px(&c, 5, 5), 0x0000FF);
        assert_eq!(c.px[0] >> 24, 0);
        assert_eq!(px(&c, 1, 0), 0x00FF00);
        assert_ne!(px(&c, 6, 6), 0x0000FF);

        let mut core = Core::load(None);
        let i = core.slots.iter().position(|s| s.name == "T-Shirt").unwrap();
        core.settings.clothes = vec!["T-Shirt".into()];
        core.settings.outfits = vec![core.current_outfit("o".into())];
        core.rename_slot(i, "Hemd".into());
        assert_eq!(core.settings.clothes, vec!["Hemd".to_string()]);
        assert_eq!(core.settings.outfits[0].clothes, vec!["Hemd".to_string()]);
        assert_eq!(core.outfit_layers().len(), 1);
    }

    #[test]
    fn never_two_laptops() {
        let scripts: [&[(u32, &str)]; 8] = [
            &[(0, "edit"), (200, "think"), (350, "bash"), (500, "done")],
            &[(0, "bash"), (150, "fail"), (300, "edit"), (450, "done")],
            &[(0, "think"), (120, "edit"), (300, "ask"), (450, "read"), (600, "done")],
            &[(0, "edit"), (60, "think"), (90, "edit"), (120, "think"), (150, "web"), (400, "done")],
            &[(0, "tool"), (100, "done"), (130, "edit"), (160, "done")],
            &[(0, "edit")],
            &[],
            &[(0, "bash"), (400, "done")],
        ];
        for (si, script) in scripts.iter().enumerate() {
            let mut core = Core::load(None);
            core.settings.glass = false;
            let mut pet = coding_pet(&mut core, "Laptop");
            if si == 7 {
                pet.claude_event(&core, "done:test");
                for _ in 0..400 {
                    pet.step(&core, None);
                }
                pet.mode = Mode::Sleep;
                pet.frame = &art::SLEEP;
                for _ in 0..300 {
                    pet.step(&core, None);
                }
                assert!(pet.dropped.is_some());
            }
            if script.is_empty() {
                pet.claude_event(&core, "done:test");
                for _ in 0..400 {
                    pet.step(&core, None);
                }
                pet.trigger_stretch();
            }
            for t in 0..1400u32 {
                for (at, ev) in script.iter() {
                    if *at == t && t > 0 {
                        pet.claude_event(&core, &format!("{ev}:test"));
                    }
                }
                pet.step(&core, None);
                if t % 3 == 0 {
                    let tile = render(&(), &mut pet, &core, 420, 310);
                    let n = laptops(&tile);
                    if n > 1 {
                        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("two_laptops.png");
                        tile.save(&out).unwrap();
                        panic!("script {si} tick {t}: {n} laptops, mode {:?}, crouch {:?}, stretch {:?}, dropped {:?}", pet.mode, pet.crouch, pet.stretch_frame, pet.dropped.as_ref().map(|d| d.desk));
                    }
                }
            }
        }
    }

    #[test]
    fn laptop_goes_to_desk_for_coding() {
        let mut core = Core::load(None);
        let mut pet = coding_pet(&mut core, "Laptop");
        for _ in 0..120 {
            pet.step(&core, None);
        }
        let d = pet.dropped.as_ref().expect("laptop is put down");
        assert!(d.desk);
        let pose = pet.dropped_pose().unwrap();
        assert!((pose.dx - LAPTOP_DESK.0).abs() < 0.01 && (pose.dy - LAPTOP_DESK.1).abs() < 0.01, "{pose:?}");
        let mut core = Core::load(None);
        let mut pet = coding_pet(&mut core, "Kaffee");
        for _ in 0..120 {
            pet.step(&core, None);
        }
        assert!(!pet.dropped.as_ref().unwrap().desk);
    }

    #[test]
    #[ignore]
    fn laptop_coding_sheet() {
        let mut core = Core::load(None);
        core.settings.hat_visible = false;
        let mut pet = coding_pet(&mut core, "Laptop");
        let (tw, th, n) = (420u32, 310u32, 18u32);
        let mut sheet = image::RgbaImage::from_pixel(tw * n, th, image::Rgba([46, 52, 64, 255]));
        for i in 0..n {
            match i {
                9 => pet.claude_event(&core, "think:test"),
                14 => pet.claude_event(&core, "done:test"),
                _ => {}
            }
            for _ in 0..6 {
                pet.step(&core, None);
            }
            let tile = render(&(), &mut pet, &core, tw, th);
            image::imageops::overlay(&mut sheet, &tile, (i * tw) as i64, 0);
        }
        sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("laptop_coding.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn bash_sheet() {
        let (tw, th, n) = (420u32, 310u32, 14u32);
        let mut sheet = image::RgbaImage::from_pixel(tw * n, th * 2, image::Rgba([46, 52, 64, 255]));
        for (row, body) in ["", "#000000"].iter().enumerate() {
            let mut core = Core::load(None);
            core.settings.glass = false;
            core.settings.crab_body = body.to_string();
            core.hat = core.slots.iter().position(|s| s.name == "Bauhelm");
            render::apply_crab_colors(&core.settings);
            core.settings.item_visible = true;
            core.item = core.slots.iter().position(|s| s.name == "Laptop");
            let mut pet = Pet::new();
            pet.layout(10, false);
            pet.ready = true;
            pet.work = Rect::from_min_max(Pos2::ZERO, Pos2::new(4000.0, 1000.0));
            pet.x = 500.0;
            pet.kind = core.item_kind();
            pet.mode = Mode::Sleep;
            pet.frame = &art::SLEEP;
            for _ in 0..300 {
                pet.step(&core, None);
            }
            pet.claude_event(&core, "bash:test");
            for col in 0..n {
                for _ in 0..[1, 4, 4, 4, 4, 6, 6, 8, 10, 15, 20, 30, 40, 60][col as usize] {
                    pet.step(&core, None);
                }
                let tile = render(&(), &mut pet, &core, tw, th);
                image::imageops::overlay(&mut sheet, &tile, (col * tw) as i64, (row as u32 * th) as i64);
            }
        }
        render::set_crab_colors(None, None, None, false);
        sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("bash.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn readme_crabs() {
        let core = Core::load(None);
        let find = |n: &str| core.slots.iter().find(|s| s.name == n).map(|s| s.sprite.clone());
        let looks: [(usize, &str, &str, &[&str]); 6] = [
            (0, "Cowboy-Hut", "Kaffee", &[]),
            (1, "Detektivhut", "", &["Umhang"]),
            (2, "Krone", "Schwert", &[]),
            (3, "Magierhut", "Zauberstab", &[]),
            (4, "Bauhelm", "Laptop", &["T-Shirt"]),
            (5, "Zylinder", "Blume", &["Schuhe"]),
        ];
        let (blk, cell) = (10.0f32, 190usize);
        let canvas = Rc::new(RefCell::new(Canvas::new(cell * looks.len(), 170)));
        let brush = Brush::canvas(canvas.clone(), 1.0);
        for (i, (preset, hat, item, clothes)) in looks.iter().enumerate() {
            let (_, body, claw, eye, rainbow) = crate::widgets::CRAB_PRESETS[*preset];
            let hex = crate::widgets::parse_hex;
            render::set_crab_colors(hex(body), hex(claw), hex(eye), rainbow);
            render::set_outfit(clothes.iter().filter_map(|c| find(c).map(|s| { let b = s.back; (s, b) })).collect());
            let (x, y) = ((i * cell) as f32 + 25.0, 80.0);
            render::draw_crab(&brush, x, y, blk, &IDLE_STAND, 0.0);
            if let Some(h) = find(hat) {
                render::draw_hat(&brush, &h, x, y, blk, art::hat_drop(hat) * blk);
            }
            if let Some(it) = find(item) {
                render::draw_item_of(&brush, ItemKind::of(Some(item)), &it, x, y, blk, ItemPose::default());
            }
        }
        render::set_crab_colors(None, None, None, false);
        render::set_outfit(Vec::new());
        let c = canvas.borrow();
        let mut img = image::RgbaImage::new(c.w as u32, c.h as u32);
        for (i, &p) in c.px.iter().enumerate() {
            let a = p >> 24;
            let un = |v: u32| if a == 0 { 0 } else { (v * 255 / a).min(255) as u8 };
            img.put_pixel((i % c.w) as u32, (i / c.w) as u32, image::Rgba([un((p >> 16) & 255), un((p >> 8) & 255), un(p & 255), a as u8]));
        }
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs").join("images").join("crabs.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn icon_sheet() {
        let mut core = Core::load(None);
        core.hat = core.slots.iter().position(|s| s.name == "Detektivhut");
        core.settings.clothes = vec!["T-Shirt".into()];
        core.settings.crab_body = "#000000".into();
        core.settings.crab_eye = "#FFFFFF".into();
        let a = render::crab_icon(&core, 64);
        core.settings.crab_eye.clear();
        core.hat = None;
        core.settings.clothes.clear();
        core.settings.crab_body.clear();
        let b = render::crab_icon(&core, 64);
        render::set_crab_colors(None, None, None, false);
        render::set_outfit(Vec::new());
        let mut img = image::RgbaImage::from_pixel(128 * 2 + 16, 128, image::Rgba([46, 52, 64, 255]));
        for (n, px) in [a, b].iter().enumerate() {
            let icon = image::RgbaImage::from_raw(64, 64, px.clone()).unwrap();
            let big = image::imageops::resize(&icon, 128, 128, image::imageops::FilterType::Nearest);
            image::imageops::overlay(&mut img, &big, (n * 144) as i64, 0);
        }
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("icons.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn clothing_sheet() {
        let (tw, th, n) = (420u32, 310u32, 6u32);
        let outfits: [&[&str]; 3] = [&["T-Shirt"], &["Umhang", "Schuhe"], &["Umhang", "T-Shirt", "Schuhe"]];
        let mut sheet = image::RgbaImage::from_pixel(tw * n, th * outfits.len() as u32, image::Rgba([46, 52, 64, 255]));
        for (row, clothes) in outfits.iter().enumerate() {
            let mut core = Core::load(None);
            core.settings.glass = false;
            core.settings.clothes = clothes.iter().map(|c| c.to_string()).collect();
            render::set_outfit(core.outfit_layers());
            let mut pet = coding_pet(&mut core, "Laptop");
            for col in 0..n {
                match col {
                    3 => pet.claude_event(&core, "think:test"),
                    4 => pet.claude_event(&core, "done:test"),
                    5 => {
                        for _ in 0..400 {
                            pet.step(&core, None);
                        }
                        pet.mode = Mode::WalkRight;
                    }
                    _ => {}
                }
                for _ in 0..[1, 40, 90, 20, 30, 12][col as usize] {
                    pet.step(&core, None);
                }
                let tile = render(&(), &mut pet, &core, tw, th);
                image::imageops::overlay(&mut sheet, &tile, (col * tw) as i64, (row as u32 * th) as i64);
            }
        }
        render::set_outfit(Vec::new());
        sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("clothing.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn crab_color_sheet() {
        let (tw, th, n) = (420u32, 310u32, 4u32);
        let presets = crate::widgets::CRAB_PRESETS;
        let mut sheet = image::RgbaImage::from_pixel(tw * n, th * presets.len() as u32, image::Rgba([46, 52, 64, 255]));
        for (row, (_, body, claw, eye, rainbow)) in presets.iter().enumerate() {
            let mut core = Core::load(None);
            core.settings.glass = false;
            core.settings.crab_body = body.to_string();
            core.settings.crab_claw = claw.to_string();
            core.settings.crab_eye = eye.to_string();
            core.settings.crab_rainbow = *rainbow;
            render::apply_crab_colors(&core.settings);
            let mut pet = coding_pet(&mut core, "Laptop");
            for col in 0..n {
                if col == 3 {
                    pet.claude_event(&core, "think:test");
                }
                for _ in 0..[1, 40, 90, 20][col as usize] {
                    pet.step(&core, None);
                }
                let tile = render(&(), &mut pet, &core, tw, th);
                image::imageops::overlay(&mut sheet, &tile, (col * tw) as i64, (row as u32 * th) as i64);
            }
        }
        render::set_crab_colors(None, None, None, false);
        sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("crab_colors.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn hat_poses() {
        let (tw, th, n) = (420u32, 310u32, 12u32);
        let hats = ["Detektivhut", "Bauhelm"];
        let mut sheet = image::RgbaImage::from_pixel(tw * n, th * 2 * hats.len() as u32, image::Rgba([46, 52, 64, 255]));
        for (hi, hat) in hats.iter().enumerate() {
            let mut core = Core::load(None);
            core.settings.hat_visible = true;
            let mut pet = coding_pet(&mut core, "Laptop");
            core.hat = core.slots.iter().position(|s| s.name == *hat);
            for i in 0..n {
                for _ in 0..6 {
                    pet.step(&core, None);
                }
                let tile = render(&(), &mut pet, &core, tw, th);
                image::imageops::overlay(&mut sheet, &tile, (i * tw) as i64, (hi as u32 * 2 * th) as i64);
            }
            let mut pet = Pet::new();
            pet.layout(10, false);
            pet.ready = true;
            pet.work = Rect::from_min_max(Pos2::ZERO, Pos2::new(4000.0, 1000.0));
            pet.x = 500.0;
            for i in 0..n {
                for _ in 0..9 {
                    pet.step(&core, None);
                }
                let tile = render(&(), &mut pet, &core, tw, th);
                image::imageops::overlay(&mut sheet, &tile, (i * tw) as i64, ((hi as u32 * 2 + 1) * th) as i64);
            }
        }
        sheet.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("hat_poses.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn glass_compare() {
        let (w, h) = (900usize, 260usize);
        let canvas = Rc::new(RefCell::new(Canvas::new(w, h)));
        let brush = Brush::canvas(canvas.clone(), 1.0);
        for (i, glass) in [false, true].into_iter().enumerate() {
            let x0 = (i * 450) as i32;
            render::draw_bubble(&brush, "It compiles!", 12, x0 + 160, 110, x0 + 440, glass, false);
            render::draw_sign(&brush, (x0 + 330) as f32, 120.0, 12.0, false, glass);
            render::draw_sign(&brush, (x0 + 400) as f32, 120.0, 12.0, true, glass);
            render::draw_bubble(&brush, "Zzz", 3, x0 + 160, 240, x0 + 440, glass, true);
        }
        let c = canvas.borrow();
        let bg = |x: usize, y: usize| -> [u32; 3] {
            match (x / 75 + y / 65) % 4 {
                0 => [24, 24, 28],
                1 => [230, 232, 236],
                2 => [40, 90, 170],
                _ => [200, 120, 60],
            }
        };
        let mut img = image::RgbaImage::new(w as u32, h as u32);
        for (i, &p) in c.px.iter().enumerate() {
            let (x, y) = (i % w, i / w);
            let inv = 255 - (p >> 24);
            let b = bg(x, y);
            let ch = |sh: u32, k: usize| (((p >> sh) & 255) + b[k] * inv / 255) as u8;
            img.put_pixel(x as u32, y as u32, image::Rgba([ch(16, 0), ch(8, 1), ch(0, 2), 255]));
        }
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("glass_compare.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn laptop_compare() {
        let core = Core::load(None);
        let laptop = core.slots.iter().find(|s| s.name == "Laptop").unwrap();
        let blk = 24.0;
        let canvas = Rc::new(RefCell::new(Canvas::new(3 * 22 * 24, 16 * 24)));
        let brush = Brush::canvas(canvas.clone(), 1.0);
        let y = 6.0 * blk;
        render::draw_sprite_frame(&brush, 0.0, y, blk, &art::STRETCH_FRAMES[12]);
        render::draw_crab(&brush, 22.0 * blk, y, blk, &IDLE_STAND, 0.0);
        render::draw_item_of(&brush, ItemKind::Laptop, &laptop.sprite, 22.0 * blk, y, blk, ItemPose::default());
        render::draw_item_of(&brush, ItemKind::Laptop, &laptop.sprite, 44.0 * blk, y, blk, ItemPose::default());
        let c = canvas.borrow();
        let mut img = image::RgbaImage::from_pixel(c.w as u32, c.h as u32, image::Rgba([46, 52, 64, 255]));
        for (i, &p) in c.px.iter().enumerate() {
            if p >> 24 != 0 {
                img.put_pixel((i % c.w) as u32, (i / c.w) as u32, image::Rgba([(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]));
            }
        }
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("laptop_compare.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn hat_compare() {
        let core = Core::load(None);
        let hat = core.slots.iter().find(|s| s.name == "Detektivhut").unwrap();
        let (w, h, blk) = (756usize, 754usize, 44.0);
        let (x, y) = (77.0, 287.0);
        let reference = std::env::var("CRAB_REF").ok().and_then(|p| image::open(p).ok()).map(|i| i.to_rgba8());
        let mut img = image::RgbaImage::from_pixel(w as u32 * 3, h as u32, image::Rgba([255, 255, 255, 255]));
        if let Some(r) = &reference {
            image::imageops::overlay(&mut img, r, 0, 0);
        }
        for (panel, with_hat) in [(1, true), (2, false)] {
            let canvas = Rc::new(RefCell::new(Canvas::new(w, h)));
            let brush = Brush::canvas(canvas.clone(), 1.0);
            render::draw_crab(&brush, x, y, blk, &IDLE_STAND, 0.0);
            if with_hat {
                render::draw_hat(&brush, &hat.sprite, x, y, blk, art::hat_drop(&hat.name) * blk);
            }
            let c = canvas.borrow();
            for (i, &p) in c.px.iter().enumerate() {
                if p >> 24 != 0 {
                    img.put_pixel((panel * w + i % c.w) as u32, (i / c.w) as u32, image::Rgba([(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]));
                }
            }
        }
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("hat_compare.png")).unwrap();
    }

    #[test]
    #[ignore]
    fn contact_sheet() {
        let ctx = ();
        let mut core = Core::load(None);
        core.settings.item_visible = true;
        core.settings.hat_visible = true;
        let find = |core: &Core, n: &str| core.slots.iter().position(|s| s.name == n);
        let cases: [(&str, Option<Play>, u32); 8] = [
            ("Gitarre", Some(Play::Rock), 180),
            ("Laptop", Some(Play::Type), 220),
            ("Flasche", None, 1),
            ("Kaffee", None, 1),
            ("Blume", None, 1),
            ("Zauberstab", None, 1),
            ("Schwert", None, 1),
            ("Laptop", None, 1),
        ];
        let cols = 10u32;
        let (tw, th) = (420u32, 310u32);
        let mut sheet = image::RgbaImage::from_pixel(tw * cols, th * cases.len() as u32, image::Rgba([46, 52, 64, 255]));
        for (row, (item, play, len)) in cases.iter().enumerate() {
            core.item = find(&core, item);
            core.hat = find(&core, if play.is_some() { "Cowboy-Hut" } else { "Bauhelm" });
            let mut pet = Pet::new();
            pet.layout(10, false);
            pet.ready = true;
            pet.work = Rect::from_min_max(Pos2::ZERO, Pos2::new(4000.0, 1000.0));
            pet.x = 500.0;
            pet.kind = core.item_kind();
            match play {
                Some(p) => pet.start_play(&core, *p),
                None if *len == 1 => {
                    pet.mode = Mode::Sleep;
                    pet.frame = &art::SLEEP;
                }
                None => {}
            }
            for col in 0..cols {
                if play.is_some() {
                    let target = len * col / cols + 1;
                    while pet.state_ticks < target && pet.mode == Mode::Play {
                        pet.step(&core, None);
                    }
                } else {
                    if *len == 1 {
                        for _ in 0..7 {
                            pet.step(&core, None);
                        }
                    } else {
                        pet.frame_count += 5;
                    }
                }
                let tile = render(&ctx, &mut pet, &core, tw, th);
                image::imageops::overlay(&mut sheet, &tile, (col * tw) as i64, (row as u32 * th) as i64);
            }
        }
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("contact_sheet.png");
        sheet.save(&out).unwrap();
        println!("saved {}", out.display());

        let hats: Vec<usize> = (0..core.slots.len()).filter(|&i| core.slots[i].builtin && core.slots[i].sprite.is_hat()).collect();
        let items: Vec<usize> = (0..core.slots.len()).filter(|&i| core.slots[i].builtin && core.slots[i].sprite.is_item()).collect();
        let n = hats.len().max(items.len()) as u32;
        let mut gallery = image::RgbaImage::from_pixel(tw * n, th * 2, image::Rgba([46, 52, 64, 255]));
        for (row, list) in [&hats, &items].into_iter().enumerate() {
            for (col, &i) in list.iter().enumerate() {
                let mut pet = Pet::new();
                pet.layout(10, false);
                pet.ready = true;
                if row == 0 {
                    core.hat = Some(i);
                    core.item = None;
                } else {
                    core.hat = None;
                    core.item = Some(i);
                }
                let tile = render(&ctx, &mut pet, &core, tw, th);
                image::imageops::overlay(&mut gallery, &tile, (col as u32 * tw) as i64, (row as u32 * th) as i64);
            }
        }
        let seqs: [(&[&str], u32); 8] = [
            (&["happy"], 7),
            (&["walk"], 4),
            (&["think:a"], 10),
            (&["edit:a"], 12),
            (&["ask:a"], 4),
            (&["edit:a", "fail:a"], 5),
            (&["edit:a", "done:a"], 14),
            (&["sleep"], 4),
        ];
        let mut poses = image::RgbaImage::from_pixel(tw * cols, th * seqs.len() as u32, image::Rgba([46, 52, 64, 255]));
        core.item = find(&core, "Kaffee");
        core.hat = find(&core, "Cowboy-Hut");
        for (row, (evs, every)) in seqs.iter().enumerate() {
            let mut pet = Pet::new();
            pet.layout(10, false);
            pet.ready = true;
            pet.work = Rect::from_min_max(Pos2::ZERO, Pos2::new(4000.0, 1000.0));
            pet.x = 500.0;
            for ev in evs.iter() {
                match *ev {
                    "happy" => pet.trigger_happy(&core),
                    "sleep" => {
                        pet.mode = Mode::Sleep;
                        pet.frame = &art::SLEEP;
                    }
                    "walk" => {
                        pet.direction = 1;
                        pet.start_walk();
                    }
                    e => {
                        pet.claude_event(&core, e);
                        if Some(ev) != evs.last() {
                            for _ in 0..80 {
                                pet.step(&core, None);
                            }
                        }
                    }
                }
            }
            for col in 0..cols {
                let tile = render(&ctx, &mut pet, &core, tw, th);
                image::imageops::overlay(&mut poses, &tile, (col * tw) as i64, (row as u32 * th) as i64);
                for _ in 0..*every {
                    pet.step(&core, None);
                }
            }
        }
        poses.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("poses.png")).unwrap();
        gallery.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("gallery.png")).unwrap();
    }
}
