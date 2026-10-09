//! Juice: particles, rings, dash ghosts, score popups, motes, slow motion.

use super::*;

pub struct Particle {
    pub pos: V2,
    pub(super) vel: V2,
    pub life: f32,
    pub max: f32,
    pub hue: Hue,
    pub size: usize,
}

pub struct Ring {
    pub pos: V2,
    pub t: f32,
    pub dur: f32,
    pub hue: Hue,
    /// Telegraph rings shrink towards the spawn point instead of expanding.
    pub inward: bool,
}

pub struct Ghost {
    pub pos: V2,
    pub angle: f32,
    pub life: f32,
}

/// Floating "+40" / "X3" text.
pub struct Popup {
    pub pos: V2,
    pub text: String,
    pub t: f32,
    pub combo: bool,
}

/// Background specks that drift and get blown around by explosions.
pub struct Mote {
    pub pos: V2,
    pub(super) vel: V2,
    pub(super) drift: V2,
    pub hue: usize,
}

/// Particles are the first thing to give up under kitty's ~1,500-placement budget.
pub(super) const MAX_PARTICLES: usize = 600;

pub(super) const MOTES: usize = 90;

pub const POPUP_TIME: f32 = 0.8;

impl Game {
    pub(super) fn update_effects(&mut self, dt: f32) {
        self.particles.retain_mut(|p| {
            p.pos += p.vel * dt;
            p.vel = p.vel * (-3.2 * dt).exp();
            p.life -= dt;
            p.life > 0.0
        });
        self.rings.retain_mut(|r| {
            r.t += dt;
            r.t < r.dur
        });
        self.ghosts.retain_mut(|g| {
            g.life -= dt;
            g.life > 0.0
        });
        self.popups.retain_mut(|p| {
            p.t += dt;
            p.pos.y -= 60.0 * dt;
            p.t < POPUP_TIME
        });
        let settle = 1.0 - (-1.2 * dt).exp();
        for m in &mut self.motes {
            m.vel += (m.drift - m.vel) * settle;
            m.pos += m.vel * dt;
            m.pos.x = m.pos.x.rem_euclid(WORLD_W);
            m.pos.y = m.pos.y.rem_euclid(WORLD_H);
        }
    }

    /// Blows background motes away from an explosion.
    pub(super) fn push_motes(&mut self, at: V2, radius: f32, strength: f32) {
        for m in &mut self.motes {
            let d = m.pos - at;
            let len = d.len();
            if len < radius {
                m.vel += d.norm() * strength * (1.0 - len / radius);
            }
        }
    }

    pub(super) fn popup(&mut self, pos: V2, text: String, combo: bool) {
        if self.popups.len() < 40 {
            self.popups.push(Popup {
                pos,
                text,
                t: 0.0,
                combo,
            });
        }
    }

    /// Slows game time to `scale` for `seconds` of real time.
    pub(super) fn slow(&mut self, seconds: f32, scale: f32) {
        if self.slowmo <= 0.0 || scale <= self.slowmo_scale {
            self.slowmo = seconds;
            self.slowmo_scale = scale;
        }
    }

    pub(super) fn emit(&mut self, pos: V2, vel: V2, life: f32, hue: Hue, size: usize) {
        if self.particles.len() < MAX_PARTICLES {
            self.particles.push(Particle {
                pos,
                vel,
                life,
                max: life,
                hue,
                size,
            });
        }
    }
}
