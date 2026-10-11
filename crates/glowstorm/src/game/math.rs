//! Vectors, distances and the game's random numbers.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

impl V2 {
    pub fn clamp(self, min: V2, max: V2) -> V2 {
        v2(self.x.clamp(min.x, max.x), self.y.clamp(min.y, max.y))
    }

    pub fn len(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn norm(self) -> V2 {
        let len = self.len();
        if len > 1e-6 {
            self * (1.0 / len)
        } else {
            V2::default()
        }
    }

    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }

    pub fn from_angle(angle: f32) -> V2 {
        v2(angle.cos(), angle.sin())
    }

    pub fn perp(self) -> V2 {
        v2(-self.y, self.x)
    }

    pub fn dot(self, o: V2) -> f32 {
        self.x * o.x + self.y * o.y
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        *self = *self + o;
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, k: f32) -> V2 {
        v2(self.x * k, self.y * k)
    }
}

impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        v2(-self.x, -self.y)
    }
}

/// Distance along a ray from inside a rectangle to its first boundary.
pub fn ray_exit(pos: V2, dir: V2, lo: V2, hi: V2) -> f32 {
    let mut distance = f32::INFINITY;
    for (pos, dir, lo, hi) in [(pos.x, dir.x, lo.x, hi.x), (pos.y, dir.y, lo.y, hi.y)] {
        if dir.abs() > 1e-6 {
            let edge = if dir > 0.0 { hi } else { lo };
            distance = distance.min(((edge - pos) / dir).max(0.0));
        }
    }
    distance
}

/// First contact with a circle along a→b, as a fraction of the frame.
pub(super) fn segment_hit(p: V2, a: V2, b: V2, radius: f32) -> Option<f32> {
    let offset = a - p;
    let c = offset.dot(offset) - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let delta = b - a;
    let len2 = delta.dot(delta);
    let approach = offset.dot(delta);
    if len2 == 0.0 || approach >= 0.0 {
        return None;
    }
    let discriminant = approach * approach - len2 * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = c / (-approach + discriminant.sqrt());
    (t <= 1.0).then_some(t)
}

pub struct Rng(pub(super) u64);

/// Distance between two segments, including crossings and stationary endpoints.
pub(super) fn segments_distance(a: V2, b: V2, c: V2, d: V2) -> f32 {
    let cross = |u: V2, v: V2| u.x * v.y - u.y * v.x;
    let denominator = cross(b - a, d - c);
    if denominator != 0.0 {
        let t = cross(c - a, d - c) / denominator;
        let u = cross(c - a, b - a) / denominator;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return 0.0;
        }
    }
    let distance = |p: V2, a: V2, b: V2| {
        let delta = b - a;
        let len2 = delta.dot(delta);
        let t = if len2 > 0.0 {
            ((p - a).dot(delta) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (p - a - delta * t).len()
    };
    distance(a, c, d)
        .min(distance(b, c, d))
        .min(distance(c, a, b))
        .min(distance(d, a, b))
}

impl Rng {
    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    pub(super) fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    pub(super) fn dir(&mut self) -> V2 {
        V2::from_angle(self.range(0.0, TAU))
    }
}
