//! The ship: movement, dash charges, gun heat and firing.

use super::*;

pub struct Player {
    pub pos: V2,
    pub vel: V2,
    pub aim: f32,
    pub hp: i32,
    pub invuln: f32,
    pub(super) fire_cd: f32,
    pub dash_cd: f32,
    pub(super) dash_t: f32,
    pub rapid_t: f32,
    pub(super) ghost_cd: f32,
    pub max_hp: i32,
    pub dash_charges: u8,
    pub upgrades: Upgrades,
    /// Weapon heat, 0..1. Firing adds heat; at 1 the gun locks until it cools.
    pub heat: f32,
    pub overheated: bool,
}

impl Player {
    pub(super) fn new(hearts: i32) -> Self {
        Self {
            pos: v2(WORLD_W / 2.0, WORLD_H / 2.0),
            vel: V2::default(),
            aim: -TAU / 4.0,
            hp: hearts,
            max_hp: hearts,
            invuln: 0.0,
            fire_cd: 0.0,
            dash_cd: 0.0,
            dash_t: 0.0,
            rapid_t: 0.0,
            ghost_cd: 0.0,
            heat: 0.0,
            overheated: false,
            dash_charges: 1,
            upgrades: Upgrades::default(),
        }
    }

    pub fn max_dash_charges(&self) -> u8 {
        1 + self.upgrades.level(Upgrade::TwinDash)
    }

    /// Positions of the orbiting shield orbs.
    pub fn shield_orbs(&self, time: f32) -> Vec<V2> {
        let n = self.upgrades.level(Upgrade::Shield);
        (0..n)
            .map(|i| {
                self.pos + V2::from_angle(time * 3.2 + i as f32 * TAU / n as f32) * SHIELD_ORBIT
            })
            .collect()
    }

    pub fn dashing(&self) -> bool {
        self.dash_t > 0.0
    }
}

pub(super) const PLAYER_RADIUS: f32 = 14.0;

pub(super) const ACCEL: f32 = 3400.0;

pub(super) const DRAG: f32 = 7.0;

pub(super) const HEAT_PER_SHOT: f32 = 0.075;

pub(super) const HEAT_COOLING: f32 = 0.42;

/// Cools from 1.0 to HEAT_RESUME in one second.
pub(super) const OVERHEAT_COOLING: f32 = HEAT_COOLING;

/// After overheating, firing resumes once heat drops to this.
pub(super) const HEAT_RESUME: f32 = 0.65;

pub(super) const DASH_SPEED: f32 = 1350.0;

pub(super) const DASH_TIME: f32 = 0.17;

pub const DASH_COOLDOWN: f32 = 1.3;

pub(super) const SHIELD_ORBIT: f32 = 50.0;

impl Game {
    /// The player's small per-wave edge: about 1.2× when enemies are 2×.
    pub fn power(&self) -> f32 {
        (1.0 + 0.02 * self.wave.saturating_sub(1) as f32).min(1.3)
    }

    /// Dash recharge: 0.03 s quicker per wave, never under 0.9 s.
    pub fn dash_cooldown(&self) -> f32 {
        (DASH_COOLDOWN - 0.03 * self.wave.saturating_sub(1) as f32).max(0.9)
    }

    pub(super) fn update_player(&mut self, dt: f32, input: &Input, view: &View) {
        // Half of the damage edge also goes into gun cooling.
        let cooling_boost = 1.0 + (self.power() - 1.0) / 2.0;
        let dash_cooldown = self.dash_cooldown();
        let Self {
            player: p,
            sfx,
            ghosts,
            bullets,
            rng,
            cam,
            ..
        } = self;
        let axis = |neg: &[KeyCode], pos: &[KeyCode]| {
            f32::from(u8::from(input.down(pos))) - f32::from(u8::from(input.down(neg)))
        };
        let movement = v2(
            axis(&[KeyCode::Char('a')], &[KeyCode::Char('d')]),
            axis(&[KeyCode::Char('w')], &[KeyCode::Char('s')]),
        )
        .norm();
        // Arrow keys: keyboard-only twin-stick (aim + fire).
        let arrows = v2(
            axis(&[KeyCode::Left], &[KeyCode::Right]),
            axis(&[KeyCode::Up], &[KeyCode::Down]),
        );

        if arrows != V2::default() {
            p.aim = arrows.angle();
        } else if let Some(mouse) = input.mouse {
            let world = *cam + mouse * (1.0 / view.scale);
            p.aim = (world - p.pos).angle();
        } else if movement != V2::default() {
            p.aim = movement.angle();
        }

        // Charges refill one at a time.
        p.dash_cd -= dt;
        if p.dash_charges < p.max_dash_charges() && p.dash_cd <= 0.0 {
            p.dash_charges += 1;
            if p.dash_charges < p.max_dash_charges() {
                p.dash_cd = dash_cooldown;
            }
        }
        p.invuln -= dt;
        p.rapid_t -= dt;
        let wants_dash = input.dash || input.hit(&[KeyCode::Char(' ')]);
        if wants_dash && p.dash_charges > 0 {
            p.dash_charges -= 1;
            if p.dash_cd <= 0.0 {
                p.dash_cd = dash_cooldown;
            }
            let dir = if movement != V2::default() {
                movement
            } else {
                V2::from_angle(p.aim)
            };
            p.vel = dir * DASH_SPEED;
            p.dash_t = DASH_TIME;
            sfx.push(Sfx::Dash);
        }

        if p.dash_t > 0.0 {
            p.dash_t -= dt;
            p.ghost_cd -= dt;
            if p.ghost_cd <= 0.0 {
                p.ghost_cd = 0.018;
                ghosts.push(Ghost {
                    pos: p.pos,
                    angle: p.aim,
                    life: 0.22,
                });
            }
            if p.dash_t <= 0.0 {
                p.vel = p.vel * 0.35;
            }
        } else {
            p.vel += movement * ACCEL * dt;
            p.vel = p.vel * (-DRAG * dt).exp();
        }
        p.pos += p.vel * dt;
        let lo = WALL + PLAYER_RADIUS;
        if p.pos.x < lo || p.pos.x > WORLD_W - lo {
            p.pos.x = p.pos.x.clamp(lo, WORLD_W - lo);
            p.vel.x = 0.0;
        }
        if p.pos.y < lo || p.pos.y > WORLD_H - lo {
            p.pos.y = p.pos.y.clamp(lo, WORLD_H - lo);
            p.vel.y = 0.0;
        }

        p.fire_cd -= dt;
        let cooling = if p.overheated {
            OVERHEAT_COOLING
        } else {
            HEAT_COOLING * cooling_boost
        };
        p.heat = (p.heat - cooling * dt).max(0.0);
        if p.overheated && p.heat <= HEAT_RESUME {
            p.overheated = false;
        }
        let firing = input.fire || arrows != V2::default();
        if firing && p.fire_cd <= 0.0 && !p.overheated {
            let rapid = p.rapid_t > 0.0;
            let interval = if rapid { 0.055 } else { 0.11 };
            p.fire_cd = (p.fire_cd + interval).max(0.0);
            // Rapid fire shoots twice as often for the same heat per second.
            let coolant = 1.0 - 0.25 * f32::from(p.upgrades.level(Upgrade::Coolant));
            p.heat += coolant
                * if rapid {
                    HEAT_PER_SHOT / 2.0
                } else {
                    HEAT_PER_SHOT
                };
            if p.heat >= 1.0 {
                p.heat = 1.0;
                p.overheated = true;
                sfx.push(Sfx::Overheat);
            }
            let spread = if rapid { 0.09 } else { 0.035 };
            let aim = p.aim + rng.range(-spread, spread);
            let dir = V2::from_angle(aim);
            let muzzle = p.pos + dir * 22.0;
            // Long Range: level 1 removes damage falloff, level 2 adds reach.
            let long_range = p.upgrades.level(Upgrade::Range);
            let range = BULLET_RANGE * if long_range >= 2 { 1.25 } else { 1.0 };
            let falloff = long_range == 0;
            let pierce = p.upgrades.level(Upgrade::Pierce);
            // (angle offset, damage) per bullet of a shot.
            let pattern: &[(f32, f32)] = match p.upgrades.level(Upgrade::Spread) {
                0 => &[(0.0, 1.0)],
                1 => &[(0.0, 1.0), (-0.14, 0.35), (0.14, 0.35)],
                _ => &[
                    (0.0, 1.0),
                    (-0.14, 0.35),
                    (0.14, 0.35),
                    (-0.28, 0.3),
                    (0.28, 0.3),
                ],
            };
            for &(offset, damage) in pattern {
                let d = V2::from_angle(aim + offset);
                bullets.push(Bullet {
                    pos: muzzle,
                    prev: p.pos,
                    vel: d * BULLET_SPEED + p.vel * 0.25,
                    traveled: 0.0,
                    range,
                    damage,
                    pierce,
                    last_hit: 0,
                    falloff,
                });
            }
            sfx.push(Sfx::Shoot);
            let vel = p.vel;
            for _ in 0..2 {
                let v = vel + (dir * 260.0 + self.rng.dir() * 120.0);
                self.emit(muzzle, v, 0.12, Hue::White, 0);
            }
        }
    }
}
