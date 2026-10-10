//! Enemy types, their attacks (always telegraphed), bullets and shockwaves.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Chaser,
    Swarm,
    Shooter,
    Tank,
}

/// Everything about an enemy type that is a number or a choice, in one place.
/// Behavior (how it moves and attacks) lives in `update_enemies`.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub hp: f32,
    pub radius: f32,
    pub score: u64,
    pub hue: Hue,
    /// Wave budget one spawn uses.
    pub cost: i32,
    /// First (effective) wave it can spawn in.
    pub first_wave: u32,
    /// How many spawn together.
    pub group: usize,
    /// Relative chance to be picked when it's allowed.
    pub weight: f32,
    pub can_be_elite: bool,
    /// Hit impulse per damage point for a bullet at base speed.
    pub knockback: f32,
    /// Spin speed (rad/s) for the spinning sprite.
    pub spin: f32,
    /// Its sprite turns to face the player instead of spinning.
    pub faces_player: bool,
    /// A big death: bigger blast, hit-stop, white flash.
    pub big_death: bool,
    /// What it breaks into when killed.
    pub splits_into: Option<(Kind, usize)>,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Chaser, Kind::Swarm, Kind::Shooter, Kind::Tank];

    pub fn spec(self) -> Spec {
        let base = Spec {
            hp: 1.0,
            radius: 16.0,
            score: 10,
            hue: Hue::Magenta,
            cost: 1,
            first_wave: 1,
            group: 1,
            weight: 1.0,
            can_be_elite: true,
            knockback: 8.0,
            spin: 0.0,
            faces_player: false,
            big_death: false,
            splits_into: None,
        };
        match self {
            Kind::Chaser => Spec {
                hp: 2.0,
                spin: 4.0,
                ..base
            },
            Kind::Swarm => Spec {
                hp: 1.0,
                radius: 10.0,
                score: 5,
                hue: Hue::Yellow,
                cost: 3,
                first_wave: 2,
                group: 5,
                // Too small to read a gold ring on.
                can_be_elite: false,
                spin: 7.0,
                ..base
            },
            Kind::Shooter => Spec {
                hp: 4.0,
                radius: 18.0,
                score: 25,
                hue: Hue::Orange,
                cost: 3,
                first_wave: 3,
                faces_player: true,
                ..base
            },
            Kind::Tank => Spec {
                hp: 16.0,
                radius: 32.0,
                score: 80,
                hue: Hue::Green,
                cost: 6,
                first_wave: 4,
                // At cost 6 tanks would otherwise eat about half of every
                // budget, and their volleys would dominate the wave.
                weight: 0.5,
                knockback: 1.5,
                spin: 0.8,
                big_death: true,
                splits_into: Some((Kind::Chaser, 3)),
                ..base
            },
        }
    }

    pub fn radius(self) -> f32 {
        self.spec().radius
    }

    pub fn hue(self) -> Hue {
        self.spec().hue
    }
}

/// What an enemy is doing. Attacks always start with a visible wind-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Move,
    Windup,
    Strike,
    Recover,
}

pub struct Enemy {
    pub kind: Kind,
    pub pos: V2,
    pub vel: V2,
    /// Bullet momentum, damped separately so attacks cannot overwrite it.
    pub(super) impulse: V2,
    pub(super) hp: f32,
    pub flash: f32,
    pub spin: f32,
    pub facing: f32,
    pub(super) cooldown: f32,
    pub phase: f32,
    pub act: Act,
    pub(super) act_t: f32,
    pub(super) aim: V2,
    /// Gold-ringed: 3× health, nastier attack, guaranteed loot.
    pub elite: bool,
    pub(super) id: u32,
    /// Tanks left alone at the end of a wave: faster, angrier.
    pub enraged: bool,
    /// The current wind-up ends in a ranged volley (tanks) rather than a slam.
    pub(super) ranged: bool,
}

impl Enemy {
    pub fn winding_up(&self) -> bool {
        self.act == Act::Windup
    }
}

/// A tank's ground-pound: an expanding ring that hurts where it passes.
pub struct Shockwave {
    pub pos: V2,
    pub r: f32,
    pub(super) hit: bool,
}

pub struct Orb {
    pub pos: V2,
    pub(super) vel: V2,
    pub(super) life: f32,
    /// Tank volley orbs: bigger and slower than shooter bullets.
    pub heavy: bool,
}

impl Orb {
    pub(super) fn radius(&self) -> f32 {
        if self.heavy { 15.0 } else { 9.0 }
    }
}

pub(super) const ORB_SPEED: f32 = 340.0;

pub(super) const MAX_ORBS: usize = 450;
/// Tank volley orbs: slow enough to read, big enough to respect.
pub(super) const VOLLEY_SPEED: f32 = 230.0;

pub const SHOCK_RADIUS: f32 = 270.0;

pub(super) const SHOCK_SPEED: f32 = 560.0;

impl Game {
    /// Health grows 6% a wave but stops at 2×: past that, waves get harder
    /// through numbers and aggression, not bullet sponges.
    pub(super) fn enemy_hp_scale(&self) -> f32 {
        let growth = (1.0 + 0.06 * self.wave.saturating_sub(1) as f32).min(2.0);
        self.difficulty.tuning().enemy_hp * growth
    }

    pub(super) fn enemy_speed_scale(&self) -> f32 {
        self.difficulty.tuning().enemy_speed
            * (1.0 + 0.03 * self.wave.saturating_sub(1) as f32).min(1.45)
    }

    /// Multiplies attack cooldowns: below 1 means enemies attack more often.
    /// The wave ramp floors at 0.55; the difficulty factor applies after the
    /// floor so Extreme stays faster than Medium in late waves too.
    pub(super) fn enemy_cooldown_scale(&self) -> f32 {
        let ramp = (1.0 / (1.0 + 0.04 * self.wave.saturating_sub(1) as f32)).max(0.55);
        ramp / self.difficulty.tuning().attack_rate
    }

    pub(super) fn add_enemy(&mut self, kind: Kind, pos: V2, vel: V2, elite: bool) {
        let id = self.next_id;
        self.next_id += 1;
        let hp = kind.spec().hp * self.enemy_hp_scale() * if elite { 3.0 } else { 1.0 };
        let cooldown = self.rng.range(1.0, 2.0);
        let phase = self.rng.range(0.0, TAU);
        self.enemies.push(Enemy {
            kind,
            pos,
            vel,
            impulse: V2::default(),
            hp,
            flash: 0.0,
            spin: phase,
            facing: (self.player.pos - pos).angle(),
            cooldown,
            phase,
            act: Act::Move,
            act_t: 0.0,
            aim: V2::default(),
            elite,
            id,
            enraged: false,
            ranged: false,
        });
    }

    pub(super) fn update_enemies(&mut self, dt: f32) {
        let target = self.player.pos;
        let target_vel = self.player.vel;
        let chasing = self.state == State::Playing;
        let speedup = self.enemy_speed_scale();
        let cooldown = self.enemy_cooldown_scale();
        // Shooter fans phase in: 30% of volleys at wave 6, +15% per wave.
        let fan_chance = if self.wave >= 6 {
            0.3 + 0.15 * (self.wave - 6) as f32
        } else {
            0.0
        };
        // Tanks shouldn't be the safe thing to leave for last: once nothing
        // else is left in the wave, they enrage.
        let only_tanks = !self.enemies.is_empty()
            && self.enemies.iter().all(|e| e.kind == Kind::Tank)
            && self.budget <= 0
            && self.spawns.is_empty()
            && self.boss.is_none();
        let mut shots = Vec::new();
        let mut shocks = Vec::new();
        let mut volleys = Vec::new();
        let mut lunged = false;
        for e in &mut self.enemies {
            e.enraged = only_tanks;
            let to_player = target - e.pos;
            let dist = to_player.len();
            let dir = to_player.norm();
            e.flash -= dt;
            e.act_t -= dt;
            e.cooldown -= dt;
            let mut desired = V2::default();
            let mut steer = 3.0;
            match (e.kind, e.act) {
                (Kind::Chaser, Act::Move) => {
                    desired = dir * 230.0 * speedup;
                    if chasing && dist < 300.0 && e.cooldown <= 0.0 {
                        e.act = Act::Windup;
                        e.act_t = if e.elite { 0.32 } else { 0.45 };
                    }
                }
                (Kind::Chaser, Act::Windup) => {
                    // Brake and lock on; the lunge direction is fixed at release.
                    steer = 10.0;
                    e.aim = dir;
                    if e.act_t <= 0.0 {
                        e.act = Act::Strike;
                        e.act_t = if e.elite { 0.38 } else { 0.3 };
                        e.vel = e.aim * if e.elite { 1250.0 } else { 980.0 };
                        lunged = true;
                    }
                }
                (Kind::Chaser, Act::Strike) => {
                    steer = 0.0;
                    if e.act_t <= 0.0 {
                        e.act = Act::Recover;
                        e.act_t = 0.45;
                        e.cooldown = (1.9 + (e.phase * 3.0).fract()) * cooldown;
                    }
                }
                (_, Act::Recover) => {
                    steer = 4.0;
                    if e.act_t <= 0.0 {
                        e.act = Act::Move;
                    }
                }
                (Kind::Swarm, _) => {
                    // Flank: come in at an angle from either side, converge up close.
                    let side = if e.phase > TAU / 2.0 { 1.0 } else { -1.0 };
                    // Capped so swarms can't orbit a circling player forever.
                    let spread = 0.6 * ((dist - 120.0) / 450.0).clamp(0.0, 1.0);
                    let heading = V2::from_angle(dir.angle() + side * spread);
                    let wobble = dir.perp() * ((self.time * 7.0 + e.phase).sin() * 90.0);
                    desired = heading * 330.0 * speedup + wobble;
                    steer = 4.0;
                }
                (Kind::Shooter, Act::Move) => {
                    let radial = if dist > 460.0 {
                        1.0
                    } else if dist < 200.0 {
                        -1.7
                    } else if dist < 320.0 {
                        -1.0
                    } else {
                        0.0
                    };
                    let strafe = if e.phase > TAU / 2.0 { 1.0 } else { -1.0 };
                    desired = dir * radial * 210.0 + dir.perp() * strafe * 120.0;
                    steer = 2.5;
                    if chasing && e.cooldown <= 0.0 && dist < 950.0 {
                        e.act = Act::Windup;
                        e.act_t = 0.4;
                    }
                }
                (Kind::Shooter, Act::Windup) => {
                    steer = 6.0;
                    if e.act_t <= 0.0 {
                        // Lead the target: aim where the player is heading.
                        let lead = (dist / ORB_SPEED).min(0.9) * 0.8;
                        let aim = (target + target_vel * lead - e.pos).norm();
                        let spread: &[f32] = if e.elite {
                            &[-0.44, -0.22, 0.0, 0.22, 0.44]
                        } else if self.rng.chance(fan_chance) {
                            &[-0.22, 0.0, 0.22]
                        } else {
                            &[0.0]
                        };
                        for a in spread {
                            shots.push((e.pos + aim * 22.0, V2::from_angle(aim.angle() + a)));
                        }
                        e.act = Act::Move;
                        e.cooldown = (1.5 + (e.phase * 7.0).fract() * 0.9) * cooldown;
                    }
                }
                (Kind::Tank, Act::Move) => {
                    let pace = if e.enraged { 200.0 } else { 85.0 };
                    desired = dir * pace * speedup;
                    steer = if e.enraged { 3.0 } else { 1.5 };
                    // Slam up close, lob a volley from afar.
                    if chasing && e.cooldown <= 0.0 && dist < 1000.0 {
                        e.ranged = dist >= 240.0;
                        e.act = Act::Windup;
                        e.act_t = if e.ranged { 0.7 } else { 0.6 };
                    }
                }
                (Kind::Tank, Act::Windup) => {
                    steer = 6.0;
                    if e.act_t <= 0.0 {
                        let rage = if e.enraged { 0.5 } else { 1.0 };
                        e.act = Act::Recover;
                        if e.ranged {
                            let lead = (dist / VOLLEY_SPEED).min(1.2) * 0.6;
                            let aim = (target + target_vel * lead - e.pos).norm();
                            let n: i32 = if e.elite { 7 } else { 5 };
                            for i in 0..n {
                                let d = V2::from_angle(aim.angle() + (i - n / 2) as f32 * 0.17);
                                volleys.push((e.pos + d * 36.0, d * VOLLEY_SPEED));
                            }
                            e.act_t = 0.6;
                            e.cooldown = 4.2 * cooldown * rage;
                        } else {
                            shocks.push((e.pos, e.elite));
                            e.act_t = 0.5;
                            e.cooldown = 3.2 * cooldown * rage;
                        }
                    }
                }
                _ => {}
            }
            let desired = if chasing { desired } else { V2::default() };
            if steer > 0.0 {
                e.vel += (desired - e.vel) * (1.0 - (-steer * dt).exp());
            }
            e.pos += (e.vel + e.impulse) * dt;
            e.impulse = e.impulse * (-4.0 * dt).exp();
            let r = e.kind.radius() + WALL;
            if e.pos.x < r || e.pos.x > WORLD_W - r {
                e.pos.x = e.pos.x.clamp(r, WORLD_W - r);
                e.vel.x = -e.vel.x;
                e.impulse.x = 0.0;
            }
            if e.pos.y < r || e.pos.y > WORLD_H - r {
                e.pos.y = e.pos.y.clamp(r, WORLD_H - r);
                e.vel.y = -e.vel.y;
                e.impulse.y = 0.0;
            }
            let spin = e.kind.spec().spin;
            // Winding up spins faster, part of the tell.
            e.spin += dt * spin * if e.act == Act::Windup { 3.5 } else { 1.0 };
            let turn = (dir.angle() - e.facing + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0;
            e.facing += turn * (1.0 - (-6.0 * dt).exp());
        }

        // Push overlapping enemies apart so packs spread into readable shapes.
        let n = self.enemies.len();
        for i in 0..n {
            for j in i + 1..n {
                let d = self.enemies[j].pos - self.enemies[i].pos;
                let min = self.enemies[i].kind.radius() + self.enemies[j].kind.radius();
                let len = d.len();
                if len < min && len > 1e-3 {
                    let push = d * ((min - len) / len * 0.5);
                    self.enemies[i].pos = self.enemies[i].pos - push;
                    self.enemies[j].pos += push;
                }
            }
        }

        if lunged {
            self.sfx.push(Sfx::Lunge);
        }
        if !shots.is_empty() {
            self.sfx.push(Sfx::EnemyShoot);
        }
        for (pos, dir) in shots {
            self.add_orb(pos, dir * ORB_SPEED);
        }
        if !volleys.is_empty() {
            self.sfx.push(Sfx::BossBlast);
        }
        for (pos, vel) in volleys {
            if self.orbs.len() < MAX_ORBS {
                self.orbs.push(Orb {
                    pos,
                    vel,
                    life: 7.0,
                    heavy: true,
                });
            }
        }
        if only_tanks && !self.enrage_announced {
            self.enrage_announced = true;
            self.sfx.push(Sfx::Shock);
            let tanks: Vec<V2> = self.enemies.iter().map(|e| e.pos).collect();
            for pos in tanks {
                self.popup(pos + v2(0.0, -50.0), "ENRAGED".to_string(), true);
            }
        }
        for (pos, elite) in shocks {
            self.shockwaves.push(Shockwave {
                pos,
                r: 0.0,
                hit: false,
            });
            // Elite tanks follow up with a second ring a beat later.
            if elite {
                self.shockwaves.push(Shockwave {
                    pos,
                    r: -170.0,
                    hit: false,
                });
            }
            self.shake = (self.shake + 7.0).min(18.0);
            self.sfx.push(Sfx::Shock);
            self.push_motes(pos, 320.0, 260.0);
        }
        let inside =
            |p: V2| p.x > WALL && p.y > WALL && p.x < WORLD_W - WALL && p.y < WORLD_H - WALL;
        self.orbs.retain_mut(|o| {
            o.pos += o.vel * dt;
            o.life -= dt;
            o.life > 0.0 && inside(o.pos)
        });
    }

    pub(super) fn add_orb(&mut self, pos: V2, vel: V2) {
        if self.orbs.len() < MAX_ORBS {
            self.orbs.push(Orb {
                pos,
                vel,
                life: 6.0,
                heavy: false,
            });
        }
    }
}
