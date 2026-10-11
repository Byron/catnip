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

/// Distance from `p` to the segment a→b. Fast bullets test their whole path
/// this frame, so they can't tunnel through small enemies.
pub(super) fn segment_distance(p: V2, a: V2, b: V2) -> f32 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 == 0.0 {
        0.0
    } else {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    };
    (p - (a + ab * t)).len()
}

pub struct Rng(pub(super) u64);

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
