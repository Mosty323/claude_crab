use crate::sprite::Sprite;

pub const GRAVITY: f32 = 0.07;
const SUBSTEPS: u32 = 8;
const RESTITUTION: f32 = 0.35;
const BOUNCE_MIN: f32 = 0.05;
const FRICTION: f32 = 0.6;
const DRAG: f32 = 0.998;
const SPIN_DRAG: f32 = 0.99;
const SLEEP_TICKS: u32 = 10;

#[derive(Clone, Copy, Debug)]
pub struct World {
    pub ground: f32,
    pub left: f32,
    pub right: f32,
}

#[derive(Clone, Debug)]
pub struct Body {
    hull: Vec<(f32, f32)>,
    inv_mass: f32,
    inv_inertia: f32,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub angle: f32,
    pub spin: f32,
    pub asleep: bool,
    still: u32,
}

fn cross(o: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn hull(mut pts: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let mut lower: Vec<(f32, f32)> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<(f32, f32)> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

impl Body {
    pub fn from_sprite(s: &Sprite, grip: (f32, f32)) -> (Self, (f32, f32)) {
        let mut px = Vec::new();
        for r in 0..s.h {
            for c in 0..s.w {
                if s.get(c, r)[3] != 0 {
                    px.push((c as f32 - grip.0, r as f32 - grip.1));
                }
            }
        }
        if px.is_empty() {
            px.push((-0.5, -0.5));
        }
        let n = px.len() as f32;
        let cx = px.iter().map(|p| p.0 + 0.5).sum::<f32>() / n;
        let cy = px.iter().map(|p| p.1 + 0.5).sum::<f32>() / n;
        let inertia: f32 = px
            .iter()
            .map(|p| (p.0 + 0.5 - cx).powi(2) + (p.1 + 0.5 - cy).powi(2) + 1.0 / 6.0)
            .sum();
        let corners = px
            .iter()
            .flat_map(|&(x, y)| [(x, y), (x + 1.0, y), (x, y + 1.0), (x + 1.0, y + 1.0)])
            .map(|(x, y)| (x - cx, y - cy))
            .collect();
        let body = Self {
            hull: hull(corners),
            inv_mass: 1.0 / n,
            inv_inertia: 1.0 / inertia,
            x: cx,
            y: cy,
            vx: 0.0,
            vy: 0.0,
            angle: 0.0,
            spin: 0.0,
            asleep: false,
            still: 0,
        };
        (body, (cx, cy))
    }

    #[cfg(test)]
    pub fn energy(&self, w: &World) -> f32 {
        let m = 1.0 / self.inv_mass;
        let i = 1.0 / self.inv_inertia;
        0.5 * m * (self.vx * self.vx + self.vy * self.vy) + 0.5 * i * self.spin * self.spin
            + m * GRAVITY * (w.ground - self.y)
    }

    pub fn step(&mut self, w: &World) {
        if self.asleep {
            return;
        }
        let h = 1.0 / SUBSTEPS as f32;
        let mut touching = false;
        for _ in 0..SUBSTEPS {
            self.vy += GRAVITY * h;
            self.x += self.vx * h;
            self.y += self.vy * h;
            self.angle += self.spin * h;
            touching |= self.collide(w.ground, (0.0, -1.0));
            touching |= self.collide(w.right, (-1.0, 0.0));
            touching |= self.collide(-w.left, (1.0, 0.0));
        }
        self.vx *= DRAG;
        self.vy *= DRAG;
        self.spin *= SPIN_DRAG;
        let slow = self.vx.abs() < 0.01 && self.vy.abs() < 0.01 && self.spin.abs() < 0.003;
        self.still = if touching && slow { self.still + 1 } else { 0 };
        if self.still >= SLEEP_TICKS {
            self.asleep = true;
            self.vx = 0.0;
            self.vy = 0.0;
            self.spin = 0.0;
        }
    }

    fn collide(&mut self, plane: f32, n: (f32, f32)) -> bool {
        let (s, c) = self.angle.sin_cos();
        let mut deepest = 0.0f32;
        let mut hit = false;
        for i in 0..self.hull.len() {
            let p = self.hull[i];
            let r = (p.0 * c - p.1 * s, p.0 * s + p.1 * c);
            let depth = -((self.x + r.0) * n.0 + (self.y + r.1) * n.1) - plane;
            if depth <= 0.0 {
                continue;
            }
            hit = true;
            deepest = deepest.max(depth);
            let v = (self.vx - self.spin * r.1, self.vy + self.spin * r.0);
            let vn = v.0 * n.0 + v.1 * n.1;
            if vn >= 0.0 {
                continue;
            }
            let rn = r.0 * n.1 - r.1 * n.0;
            let e = if vn < -BOUNCE_MIN { RESTITUTION } else { 0.0 };
            let j = -(1.0 + e) * vn / (self.inv_mass + self.inv_inertia * rn * rn);
            self.vx += j * n.0 * self.inv_mass;
            self.vy += j * n.1 * self.inv_mass;
            self.spin += self.inv_inertia * rn * j;

            let t = (-n.1, n.0);
            let v = (self.vx - self.spin * r.1, self.vy + self.spin * r.0);
            let vt = v.0 * t.0 + v.1 * t.1;
            let rt = r.0 * t.1 - r.1 * t.0;
            let jt = (-vt / (self.inv_mass + self.inv_inertia * rt * rt)).clamp(-FRICTION * j, FRICTION * j);
            self.vx += jt * t.0 * self.inv_mass;
            self.vy += jt * t.1 * self.inv_mass;
            self.spin += self.inv_inertia * rt * jt;
        }
        if hit {
            self.x += n.0 * deepest;
            self.y += n.1 * deepest;
        }
        hit
    }

    #[cfg(test)]
    pub fn lowest(&self) -> f32 {
        let (s, c) = self.angle.sin_cos();
        self.hull
            .iter()
            .map(|p| self.y + p.0 * s + p.1 * c)
            .fold(f32::MIN, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(rows: &[&str]) -> Sprite {
        let mut s = Sprite::new(rows[0].len(), rows.len());
        for (r, row) in rows.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                if ch == '#' {
                    s.set(c, r, [255, 255, 255, 255]);
                }
            }
        }
        s
    }

    const WORLD: World = World { ground: 4.5, left: -20.0, right: 14.0 };

    fn sword() -> Body {
        let s = sprite(&["......##", ".....##.", "....##..", ".#.##...", "..#.....", ".#.#....", "#.......", "........"]);
        let (mut b, _) = Body::from_sprite(&s, (1.5, 5.5));
        b.vx = 0.12;
        b.vy = -0.3;
        b.spin = 0.16;
        b
    }

    fn run(b: &mut Body, ticks: u32) -> Vec<(f32, f32, f32)> {
        (0..ticks)
            .map(|_| {
                b.step(&WORLD);
                (b.x, b.y, b.angle)
            })
            .collect()
    }

    #[test]
    fn comes_to_rest_and_sleeps() {
        let mut b = sword();
        run(&mut b, 600);
        assert!(b.asleep);
        assert!((b.lowest() - WORLD.ground).abs() < 0.05);
    }

    #[test]
    fn no_tunneling_at_high_speed() {
        let mut b = sword();
        b.vy = 6.0;
        b.vx = 5.0;
        for _ in 0..400 {
            b.step(&WORLD);
            assert!(b.lowest() < WORLD.ground + 0.6, "fell through: {}", b.lowest());
            assert!(b.x > WORLD.left - 1.0 && b.x < WORLD.right + 1.0);
        }
    }

    #[test]
    fn deterministic() {
        let (mut a, mut b) = (sword(), sword());
        assert_eq!(run(&mut a, 300), run(&mut b, 300));
    }

    #[test]
    fn energy_never_grows() {
        let mut b = sword();
        let mut last = b.energy(&WORLD);
        for _ in 0..600 {
            b.step(&WORLD);
            let e = b.energy(&WORLD);
            assert!(e <= last + 0.02, "energy rose from {last} to {e}");
            last = e;
        }
    }

    #[test]
    fn flat_box_settles_flat() {
        let s = sprite(&["########", "########"]);
        let (mut b, _) = Body::from_sprite(&s, (0.0, 0.0));
        b.angle = 0.6;
        run(&mut b, 600);
        let a = b.angle.rem_euclid(std::f32::consts::PI);
        assert!(a < 0.05 || a > std::f32::consts::PI - 0.05, "angle {a}");
    }
}
