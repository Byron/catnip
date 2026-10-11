//! The ship: movement, dash charges, gun heat and firing.

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weapon {
    #[default]
    Default,
    Sniper,
    Rocket,
    Plasma,
    Gravity,
    Ricochet,
    Lightning,
    Blade,
    Mines,
    Flame,
}

impl Weapon {
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "1 GUN",
            Self::Sniper => "2 SNIPER",
            Self::Rocket => "3 ROCKET",
            Self::Plasma => "4 PLASMA",
            Self::Gravity => "5 GRAVITY",
            Self::Ricochet => "6 RICOCHET",
            Self::Lightning => "7 LIGHTNING",
            Self::Blade => "8 BLADE",
            Self::Mines => "9 MINES",
            Self::Flame => "0 FLAME",
        }
    }

    fn special(self) -> bool {
        matches!(
            self,
            Self::Gravity
                | Self::Ricochet
                | Self::Lightning
                | Self::Blade
                | Self::Mines
                | Self::Flame
        )
    }
}

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
    pub weapon: Weapon,
    /// Signed thrust along the aim direction: rear rocket (+) or front jets (-).
    pub thrust: f32,
    /// Shared heat, 0..1. Firing, charging and thrust add heat; at 1 the gun locks.
    pub heat: f32,
    pub overheated: bool,
    /// Charge accumulated for the gatling or star laser, 0..1.
    pub charge: f32,
    /// Automatic discharge needs a release before another charge can start.
    pub(super) charge_blocked: bool,
    pub spin: Option<Spin>,
    pub(super) roll_hold: Option<f32>,
    pub charge_spin: bool,
}

pub struct Spin {
    pub life: f32,
    pub angle: f32,
    pub(super) fire_cd: f32,
    pub(super) from: V2,
    pub(super) active: f32,
    pub duration: f32,
    pub charge: f32,
    pub boosted: bool,
    pub(super) damage: f32,
}

impl Spin {
    pub(super) fn new(charge: f32, boosted: bool, multiplier: f32, pos: V2, angle: f32) -> Self {
        let duration = 0.35 + 1.25 * charge;
        Self {
            life: duration,
            duration,
            charge,
            boosted,
            damage: (1.0 + 2.0 * charge) * multiplier,
            angle,
            fire_cd: 0.0,
            from: pos,
            active: 0.0,
        }
    }

    pub fn strength(&self) -> f32 {
        (self.life / self.duration).clamp(0.0, 1.0)
    }

    pub fn width(&self) -> f32 {
        0.5 + (2.5 + 3.0 * self.charge) * self.strength()
    }

    pub fn angular_speed(&self) -> f32 {
        TAU * if self.boosted { 0.8 } else { 0.5 }
    }

    pub fn laser_angle(&self) -> f32 {
        let step = TAU / (8.0 * 16.0);
        (self.angle / step).round() * step
    }
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
            charge: 0.0,
            charge_blocked: false,
            spin: None,
            roll_hold: None,
            charge_spin: false,
            dash_charges: 1,
            upgrades: Upgrades::default(),
            weapon: Weapon::Default,
            thrust: 0.0,
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

    pub fn angle(&self) -> f32 {
        self.spin.as_ref().map_or(self.aim, |spin| spin.angle)
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

/// A cold gun takes 0.6 seconds to force a charged shot out.
pub(super) const CHARGE_HEAT: f32 = 1.0 / 0.6;

pub const CHARGE_BOOST_HEAT: f32 = 0.9;

pub(super) const DASH_SPEED: f32 = 1350.0;

pub(super) const DASH_TIME: f32 = 0.17;
pub(super) const ROLL_TAP_TIME: f32 = 0.18;
pub const SPIN_RADIUS: f32 = 48.0;

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
        let perfect_cooling = self.dev.perfect_cooling;
        if perfect_cooling {
            self.player.heat = 0.0;
            self.player.overheated = false;
        }
        let power = self.power();
        let mut flashes = Vec::new();
        let mut special_shots = Vec::new();
        // Half of the damage edge also goes into gun cooling.
        let cooling_boost = 1.0 + (self.power() - 1.0) / 2.0;
        let dash_cooldown = self.dash_cooldown();
        let size = self.arena_size;
        let Self {
            player: p,
            sfx,
            ghosts,
            bullets,
            bfgs,
            gatling,
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
        if let Some(mouse) = input.mouse {
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
        let wants_dash = input.dash || input.hit(&[KeyCode::Char(' '), KeyCode::Char('r')]);
        let roll_held = input.spin || input.down(&[KeyCode::Char(' '), KeyCode::Char('r')]);
        // Offensive charging does not spend a dash charge or wait for evasion cooldown.
        if wants_dash
            && roll_held
            && p.spin.is_none()
            && !p.charge_spin
            && p.charge == 0.0
            && !p.charge_blocked
            && gatling.is_none()
        {
            p.roll_hold = Some(0.0);
        }
        if wants_dash && p.dash_charges > 0 && !p.dashing() && p.spin.is_none() && !p.charge_spin {
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
        if let Some(held) = &mut p.roll_hold {
            if roll_held {
                *held += dt;
                if *held > ROLL_TAP_TIME {
                    p.charge_spin = true;
                }
            } else {
                p.roll_hold = None;
            }
        }

        if let Some(spin) = &mut p.spin {
            spin.from = p.pos;
            spin.active = dt.min(spin.life);
            spin.life -= spin.active;
            spin.angle += spin.active * spin.angular_speed();
            p.ghost_cd -= spin.active;
            if p.ghost_cd <= 0.0 && spin.active > 0.0 {
                p.ghost_cd = 0.018;
                ghosts.push(Ghost {
                    pos: p.pos,
                    angle: spin.angle,
                    life: 0.22,
                });
            }
        }
        let forward = V2::from_angle(p.aim);
        let axial = movement.dot(forward);
        p.thrust = if input.boost && !p.dashing() && !p.overheated && axial.abs() > 0.1 {
            axial
        } else {
            0.0
        };
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
            p.vel += forward * ACCEL * p.thrust * if p.thrust > 0.0 { 1.2 } else { 0.5 } * dt;
            p.vel = p.vel * (-DRAG * dt).exp();
        }
        p.pos += p.vel * dt;
        let lo = WALL + PLAYER_RADIUS;
        if p.pos.x < lo || p.pos.x > size.x - lo {
            p.pos.x = p.pos.x.clamp(lo, size.x - lo);
            p.vel.x = 0.0;
        }
        if p.pos.y < lo || p.pos.y > size.y - lo {
            p.pos.y = p.pos.y.clamp(lo, size.y - lo);
            p.vel.y = 0.0;
        }

        // Idle time never banks a burst of shots for the next press.
        p.fire_cd = p.fire_cd.max(0.0) - dt;
        let cooling = if p.overheated {
            OVERHEAT_COOLING
        } else {
            HEAT_COOLING * cooling_boost
        };
        let charging = if p.charge_spin {
            roll_held
        } else {
            input.charge || input.down(&[KeyCode::Char('e')])
        };
        if !charging && !roll_held {
            p.charge_blocked = false;
        }
        // Release uses the heat shown on the last frame, before passive cooling.
        let release = p.charge > 0.0 && !charging;
        if p.charge_spin && !charging && p.charge == 0.0 {
            p.charge_spin = false;
        }
        if p.spin.is_some()
            || (!release
                && (!charging
                    || p.overheated
                    || p.charge_blocked
                    || gatling.is_some()
                    || (p.charge == 0.0 && p.fire_cd > 0.0)))
        {
            p.heat = (p.heat - cooling * dt).max(0.0);
        }
        if p.overheated && p.heat <= HEAT_RESUME {
            p.overheated = false;
        }
        let coolant = 1.0 - 0.25 * f32::from(p.upgrades.level(Upgrade::Coolant));
        let thrust_heat = p.thrust.abs() * if p.thrust > 0.0 { 0.8 } else { 0.6 };
        if !perfect_cooling {
            p.heat = (p.heat + thrust_heat * coolant * dt).min(1.0);
        }
        if p.heat >= 1.0 && p.charge == 0.0 && !p.overheated {
            p.overheated = true;
            sfx.push(Sfx::Overheat);
        }
        if p.spin.is_some() {
            return;
        }
        if charging
            && gatling.is_none()
            && !p.overheated
            && !p.charge_blocked
            && (p.charge > 0.0 || p.fire_cd <= 0.0)
        {
            let heat = (CHARGE_HEAT * coolant / cooling_boost * dt).min(1.0 - p.heat);
            p.charge = (p.charge + heat).min(1.0);
            if !perfect_cooling {
                p.heat = (p.heat + heat).min(1.0);
            }
        }
        let forced = p.charge > 0.0 && p.heat >= 1.0;
        if release || forced {
            let boosted = !forced && p.heat >= CHARGE_BOOST_HEAT;
            let damage = (1.0 + 2.0 * p.charge)
                * if forced {
                    0.75
                } else if boosted {
                    1.1
                } else {
                    1.0
                };
            let duration = 0.35 + 1.25 * p.charge;
            if p.charge_spin {
                let multiplier = damage / (1.0 + 2.0 * p.charge);
                p.spin = Some(Spin::new(p.charge, boosted, multiplier, p.pos, p.aim));
                p.dash_t = 0.0;
                p.roll_hold = None;
                p.charge_spin = false;
                sfx.push(Sfx::Dash);
            } else if p.weapon.special() {
                special_shots.push((
                    p.weapon,
                    Some(p.charge),
                    damage / (1.0 + 2.0 * p.charge) * power,
                    boosted,
                ));
            } else if p.weapon == Weapon::Plasma {
                let dir = V2::from_angle(p.aim);
                let multiplier = damage / (1.0 + 2.0 * p.charge);
                bfgs.push(Bfg::new(
                    p.pos + dir * 26.0,
                    dir * 560.0 * if boosted { 1.35 } else { 1.0 },
                    p.charge,
                    (120.0 + 360.0 * p.charge) * multiplier * power,
                    1600.0
                        * if p.upgrades.level(Upgrade::Range) >= 2 {
                            1.25
                        } else {
                            1.0
                        },
                ));
                p.vel += -dir * (40.0 + 60.0 * p.charge);
                sfx.push(Sfx::Bfg);
            } else if p.weapon == Weapon::Rocket {
                let count = (p.charge * 10.0).ceil().clamp(1.0, 10.0) as usize;
                for i in 0..count {
                    let angle = p.aim + (i as f32 - (count - 1) as f32 * 0.5) * 0.09;
                    let dir = V2::from_angle(angle);
                    bullets.push(Bullet {
                        pos: p.pos + dir * 25.0,
                        prev: p.pos,
                        delay: 0.0,
                        vel: dir * ROCKET_SPEED * if boosted { 1.35 } else { 1.0 },
                        traveled: 0.0,
                        range: ROCKET_RANGE
                            * if p.upgrades.level(Upgrade::Range) >= 2 {
                                1.25
                            } else {
                                1.0
                            },
                        damage: damage * 10.0,
                        pierce: 0,
                        hit_ids: Vec::new(),
                        exits: Vec::new(),
                        homing: true,
                        falloff: false,
                        weapon: Weapon::Rocket,
                    });
                    p.vel += -dir * 12.0;
                }
                sfx.push(Sfx::BossBlast);
            } else {
                *gatling = Some(Gatling {
                    weapon: p.weapon,
                    charge: p.charge,
                    boosted,
                    damage: damage
                        * if p.weapon == Weapon::Sniper {
                            SNIPER_DAMAGE / 3.0
                        } else {
                            1.0
                        },
                    life: duration,
                    duration,
                    fire_cd: 0.0,
                    range: (if p.weapon == Weapon::Sniper {
                        1200.0 + 600.0 * p.charge
                    } else {
                        650.0 + 550.0 * p.charge
                    }) * if p.upgrades.level(Upgrade::Range) >= 2 {
                        1.25
                    } else {
                        1.0
                    },
                });
            }
            p.charge = 0.0;
            p.fire_cd = 0.25;
            if forced {
                p.overheated = true;
                p.charge_blocked = true;
                sfx.push(Sfx::Overheat);
            }
        }
        let firing = input.fire;
        while firing
            && gatling.is_none()
            && !charging
            && !release
            && p.fire_cd <= 0.0
            && !p.overheated
        {
            let rapid = p.rapid_t > 0.0;
            let sniper = p.weapon == Weapon::Sniper;
            let rocket = p.weapon == Weapon::Rocket;
            let plasma = p.weapon == Weapon::Plasma;
            let delay = if plasma {
                (dt + p.fire_cd).max(0.0)
            } else {
                0.0
            };
            let (interval, heat) = match p.weapon {
                Weapon::Default => (0.11, HEAT_PER_SHOT),
                Weapon::Sniper => (0.5, HEAT_PER_SHOT * 3.0),
                Weapon::Rocket => (0.65, HEAT_PER_SHOT * 4.0),
                Weapon::Plasma => (0.025, HEAT_PER_SHOT * 0.36),
                Weapon::Gravity => (0.3, 0.09),
                Weapon::Ricochet => (0.19, 0.085),
                Weapon::Lightning => (0.085, 0.04),
                Weapon::Blade => (0.5, 0.18),
                Weapon::Mines => (0.32, 0.15),
                Weapon::Flame => (0.05, 0.025),
            };
            let interval = interval * if rapid { 0.5 } else { 1.0 };
            p.fire_cd += interval;
            // Rapid fire shoots twice as often for the same heat per second.
            let coolant = 1.0 - 0.25 * f32::from(p.upgrades.level(Upgrade::Coolant));
            p.heat +=
                if perfect_cooling { 0.0 } else { coolant } * heat * if rapid { 0.5 } else { 1.0 };
            if p.heat >= 1.0 {
                p.heat = 1.0;
                p.overheated = true;
                sfx.push(Sfx::Overheat);
            }
            if p.weapon.special() {
                special_shots.push((p.weapon, None, power, false));
                continue;
            }
            let spread = if sniper {
                0.004
            } else if rapid {
                0.09
            } else {
                0.035
            };
            let aim = p.aim + rng.range(-spread, spread);
            let dir = V2::from_angle(aim);
            let muzzle = p.pos + dir * 22.0;
            // Long Range: level 1 removes damage falloff, level 2 adds reach.
            let long_range = p.upgrades.level(Upgrade::Range);
            let range = if plasma {
                1200.0
            } else if rocket {
                ROCKET_RANGE
            } else if sniper {
                1800.0
            } else {
                BULLET_RANGE
            } * if long_range >= 2 { 1.25 } else { 1.0 };
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
                let recoil = damage
                    * if plasma {
                        0.75
                    } else if rocket {
                        8.0
                    } else if sniper {
                        6.0
                    } else {
                        1.0
                    };
                let damage = damage
                    * if plasma {
                        PLASMA_DAMAGE
                    } else if rocket {
                        22.0
                    } else if sniper {
                        SNIPER_DAMAGE
                    } else {
                        1.0
                    };
                bullets.push(Bullet {
                    pos: muzzle,
                    prev: p.pos,
                    delay,
                    vel: d * if plasma {
                        1650.0
                    } else if rocket {
                        ROCKET_SPEED
                    } else {
                        BULLET_SPEED * if sniper { 4.0 } else { 1.0 }
                    } + p.vel * 0.25,
                    traveled: 0.0,
                    range,
                    damage,
                    pierce,
                    hit_ids: Vec::new(),
                    exits: Vec::new(),
                    homing: false,
                    falloff: falloff && !rocket,
                    weapon: p.weapon,
                });
                p.vel += -d * 9.0 * recoil;
            }
            sfx.push(if plasma {
                Sfx::Plasma
            } else if rocket {
                Sfx::BossBlast
            } else if sniper {
                Sfx::Sniper
            } else {
                Sfx::Shoot
            });
            flashes.push((muzzle, dir, p.vel));
            if !plasma {
                p.fire_cd = p.fire_cd.max(0.0);
                break;
            }
        }
        for (weapon, charge, multiplier, boosted) in special_shots {
            match weapon {
                Weapon::Gravity => self.fire_gravity(charge, multiplier, boosted),
                Weapon::Ricochet => self.fire_ricochet(charge, multiplier, boosted),
                Weapon::Lightning => self.fire_lightning(charge, multiplier, boosted),
                Weapon::Blade => self.fire_blade(charge, multiplier, boosted),
                Weapon::Mines => self.fire_mines(charge, multiplier, boosted),
                Weapon::Flame => self.fire_flame(charge, multiplier, boosted),
                _ => unreachable!(),
            }
        }
        for (muzzle, dir, vel) in flashes {
            for _ in 0..2 {
                let v = vel + (dir * 260.0 + self.rng.dir() * 120.0);
                self.emit(muzzle, v, 0.12, Hue::White, 0);
            }
        }
    }
}
