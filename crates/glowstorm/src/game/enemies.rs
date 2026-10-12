//! Enemy types, their attacks (always telegraphed), bullets and shockwaves.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Chaser,
    Swarm,
    Shooter,
    Tank,
    Skirmisher,
    Sniper,
    Spinner,
    Bomber,
    Splitter,
    Sentinel,
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
    #[cfg(test)]
    pub const ALL: [Kind; 10] = [
        Kind::Chaser,
        Kind::Swarm,
        Kind::Shooter,
        Kind::Tank,
        Kind::Skirmisher,
        Kind::Sniper,
        Kind::Spinner,
        Kind::Bomber,
        Kind::Splitter,
        Kind::Sentinel,
    ];

    pub fn spec(self) -> Spec {
        let base = Spec {
            hp: 1.0,
            radius: 16.0,
            score: 10,
            hue: Hue::Magenta,
            cost: 1,
            first_wave: 1,
            group: 1,
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
                knockback: 1.5,
                spin: 0.8,
                big_death: true,
                splits_into: Some((Kind::Chaser, 3)),
                ..base
            },
            Kind::Skirmisher => Spec {
                hp: 5.0,
                radius: 18.0,
                score: 35,
                hue: Hue::Cyan,
                cost: 4,
                first_wave: 6,
                faces_player: true,
                ..base
            },
            Kind::Sniper => Spec {
                hp: 6.0,
                radius: 20.0,
                score: 45,
                hue: Hue::Red,
                cost: 5,
                first_wave: 7,
                faces_player: true,
                ..base
            },
            Kind::Spinner => Spec {
                hp: 9.0,
                radius: 23.0,
                score: 55,
                hue: Hue::Violet,
                cost: 5,
                first_wave: 8,
                spin: 1.4,
                ..base
            },
            Kind::Bomber => Spec {
                hp: 14.0,
                radius: 28.0,
                score: 75,
                hue: Hue::Orange,
                cost: 7,
                first_wave: 9,
                knockback: 2.0,
                spin: 0.5,
                big_death: true,
                ..base
            },
            Kind::Splitter => Spec {
                hp: 8.0,
                radius: 24.0,
                score: 50,
                hue: Hue::Green,
                cost: 5,
                first_wave: 11,
                faces_player: true,
                splits_into: Some((Kind::Swarm, 4)),
                ..base
            },
            Kind::Sentinel => Spec {
                hp: 12.0,
                radius: 27.0,
                score: 85,
                hue: Hue::Yellow,
                cost: 7,
                first_wave: 12,
                knockback: 3.0,
                spin: 0.35,
                big_death: true,
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
    /// Seconds left drifting without steering or attacking after a heavy hit.
    pub stun: f32,
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
    /// Tank volley instead of slam; sentinel fan instead of cross.
    pub(super) ranged: bool,
}

impl Enemy {
    pub fn winding_up(&self) -> bool {
        self.act == Act::Windup
    }

    pub(super) fn stagger(&mut self, damage: f32) {
        if damage < 3.0 {
            return;
        }
        if self.stun <= 0.0 {
            self.vel = self.vel * 0.35;
        }
        self.stun = self.stun.max((damage * 0.15).min(1.2));
        // Cancel the attack so recovery always starts with a fresh telegraph.
        self.act = Act::Recover;
        self.act_t = 0.2;
        self.cooldown = self.cooldown.max(0.4);
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
    pub(super) prev: V2,
    pub(super) vel: V2,
    pub(super) life: f32,
    /// Heavy volleys and area denial: bigger and slower than shooter bullets.
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
            stun: 0.0,
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
        let size = self.arena_size;
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
            let stunned = e.stun > 0.0;
            if !stunned {
                e.act_t -= dt;
                e.cooldown -= dt;
            }
            let mut desired = V2::default();
            let mut steer = 3.0;
            match (e.kind, e.act) {
                _ if stunned => steer = 0.8,
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
                            shots.push((
                                e.pos + aim * 22.0,
                                V2::from_angle(aim.angle() + a) * ORB_SPEED,
                            ));
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
                (Kind::Skirmisher, Act::Move) => {
                    let side = if e.phase > TAU / 2.0 { 1.0 } else { -1.0 };
                    let radial = ((dist - 340.0) / 180.0).clamp(-1.0, 1.0);
                    desired = (dir * radial * 180.0 + dir.perp() * side * 240.0) * speedup;
                    steer = 4.0;
                    if chasing && e.cooldown <= 0.0 && dist < 850.0 {
                        e.act = Act::Windup;
                        e.act_t = 0.5;
                    }
                }
                (Kind::Skirmisher, Act::Windup) => {
                    // Keep strafing through the tell so its fan sweeps across the arena.
                    desired =
                        dir.perp() * if e.phase > TAU / 2.0 { 160.0 } else { -160.0 } * speedup;
                    if e.act_t <= 0.0 {
                        let n = if e.elite { 5 } else { 3 };
                        for i in 0..n {
                            let d = V2::from_angle(dir.angle() + (i - n / 2) as f32 * 0.2);
                            shots.push((e.pos + d * 24.0, d * 315.0));
                        }
                        e.act = Act::Move;
                        e.cooldown = 1.7 * cooldown;
                    }
                }
                (Kind::Sniper, Act::Move) => {
                    desired = dir * ((dist - 650.0) / 160.0).clamp(-1.0, 1.0) * 170.0 * speedup;
                    if chasing && e.cooldown <= 0.0 && dist < 1400.0 {
                        e.aim = dir;
                        e.facing = dir.angle();
                        e.act = Act::Windup;
                        e.act_t = 1.15;
                        // A stationary muzzle makes the locked sightline honest.
                        e.vel = V2::default();
                        desired = V2::default();
                    }
                }
                (Kind::Sniper, Act::Windup) => {
                    steer = 10.0;
                    if e.act_t <= 0.0 {
                        let n = if e.elite { 3 } else { 1 };
                        for i in 0..n {
                            let d = V2::from_angle(e.aim.angle() + (i - n / 2) as f32 * 0.065);
                            shots.push((e.pos + d * 28.0, d * 720.0));
                        }
                        e.act = Act::Recover;
                        e.act_t = 0.6;
                        e.cooldown = 3.0 * cooldown;
                    }
                }
                (Kind::Spinner, Act::Move) => {
                    desired = dir * 70.0 * speedup + dir.perp() * 45.0;
                    if chasing && e.cooldown <= 0.0 && dist < 1100.0 {
                        e.act = Act::Windup;
                        e.act_t = 0.8;
                    }
                }
                (Kind::Spinner, Act::Windup) => {
                    steer = 6.0;
                    if e.act_t <= 0.0 {
                        let n = if e.elite { 12 } else { 8 };
                        for i in 0..n {
                            let d = V2::from_angle(e.spin + i as f32 * TAU / n as f32);
                            shots.push((e.pos + d * 30.0, d * 285.0));
                        }
                        e.act = Act::Recover;
                        e.act_t = 0.35;
                        e.cooldown = 2.4 * cooldown;
                    }
                }
                (Kind::Bomber, Act::Move) => {
                    desired = dir * ((dist - 280.0) / 180.0).clamp(-0.5, 1.0) * 95.0 * speedup;
                    steer = 1.4;
                    if chasing && e.cooldown <= 0.0 && dist < 950.0 {
                        e.act = Act::Windup;
                        e.act_t = 1.0;
                    }
                }
                (Kind::Bomber, Act::Windup) => {
                    steer = 6.0;
                    if e.act_t <= 0.0 {
                        let n = if e.elite { 12 } else { 8 };
                        for i in 0..n {
                            let d = V2::from_angle(e.spin + i as f32 * TAU / n as f32);
                            // Alternating speeds leave a staggered field with open lanes.
                            let speed = if i % 2 == 0 { 100.0 } else { 145.0 };
                            volleys.push((e.pos + d * 38.0, d * speed));
                        }
                        e.act = Act::Recover;
                        e.act_t = 0.8;
                        e.cooldown = 4.8 * cooldown;
                    }
                }
                (Kind::Splitter, Act::Move) => {
                    desired = (dir * 280.0
                        + dir.perp() * (self.time * 5.0 + e.phase).sin() * 130.0)
                        * speedup;
                    steer = 4.0;
                    if chasing && e.cooldown <= 0.0 && dist < 440.0 {
                        e.aim = dir;
                        e.act = Act::Windup;
                        e.act_t = 0.55;
                    }
                }
                (Kind::Splitter, Act::Windup) => {
                    steer = 10.0;
                    if e.act_t <= 0.0 {
                        e.act = Act::Strike;
                        e.act_t = 0.65;
                        e.vel = e.aim * if e.elite { 880.0 } else { 720.0 };
                        steer = 0.0;
                        lunged = true;
                    }
                }
                (Kind::Splitter, Act::Strike) => {
                    steer = 0.0;
                    if e.act_t <= 0.0 {
                        e.act = Act::Recover;
                        e.act_t = 0.3;
                        e.cooldown = 2.3 * cooldown;
                    }
                }
                (Kind::Sentinel, Act::Move) => {
                    desired = dir * ((dist - 420.0) / 180.0).clamp(-1.0, 1.0) * 95.0 * speedup;
                    steer = 2.0;
                    if chasing && e.cooldown <= 0.0 && dist < 1100.0 {
                        e.aim = dir;
                        e.act = Act::Windup;
                        e.act_t = 0.85;
                    }
                }
                (Kind::Sentinel, Act::Windup) => {
                    steer = 6.0;
                    if e.act_t <= 0.0 {
                        if e.ranged {
                            let n = if e.elite { 9 } else { 7 };
                            for i in 0..n {
                                let d = V2::from_angle(e.aim.angle() + (i - n / 2) as f32 * 0.16);
                                shots.push((e.pos + d * 34.0, d * 305.0));
                            }
                        } else {
                            for i in 0..4 {
                                let d = V2::from_angle(e.spin + i as f32 * TAU / 4.0);
                                for speed in if e.elite {
                                    &[180.0, 250.0, 320.0][..]
                                } else {
                                    &[210.0, 290.0][..]
                                } {
                                    shots.push((e.pos + d * 34.0, d * *speed));
                                }
                            }
                        }
                        e.ranged = !e.ranged;
                        e.act = Act::Recover;
                        e.act_t = 0.65;
                        e.cooldown = 3.2 * cooldown;
                    }
                }
                _ => {}
            }
            let desired = if chasing { desired } else { V2::default() };
            if steer > 0.0 {
                e.vel += (desired - e.vel) * (1.0 - (-steer * dt).exp());
            }
            e.pos += (e.vel + e.impulse) * dt;
            e.impulse = e.impulse * (-(if stunned { 1.8 } else { 4.0 }) * dt).exp();
            let r = e.kind.radius() + WALL;
            if e.pos.x < r || e.pos.x > size.x - r {
                e.pos.x = e.pos.x.clamp(r, size.x - r);
                e.vel.x = -e.vel.x;
                e.impulse.x = 0.0;
            }
            if e.pos.y < r || e.pos.y > size.y - r {
                e.pos.y = e.pos.y.clamp(r, size.y - r);
                e.vel.y = -e.vel.y;
                e.impulse.y = 0.0;
            }
            if stunned {
                let tumble = (5.0 + e.phase) * if e.phase > TAU / 2.0 { -1.0 } else { 1.0 };
                e.spin += dt * tumble;
                e.facing += dt * tumble;
            } else {
                let spin = e.kind.spec().spin;
                // Winding up spins faster, part of the tell.
                e.spin += dt * spin * if e.act == Act::Windup { 3.5 } else { 1.0 };
                let facing = if e.kind == Kind::Sniper && e.winding_up() {
                    e.aim.angle()
                } else {
                    dir.angle()
                };
                let turn = (facing - e.facing + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0;
                e.facing += turn * (1.0 - (-6.0 * dt).exp());
            }
            e.stun = (e.stun - dt).max(0.0);
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
        for (pos, vel) in shots {
            self.add_orb(pos, vel);
        }
        if !volleys.is_empty() {
            self.sfx.push(Sfx::BossBlast);
        }
        for (pos, vel) in volleys {
            if self.orbs.len() < MAX_ORBS {
                self.orbs.push(Orb {
                    pos,
                    prev: pos,
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
        for o in &mut self.orbs {
            o.prev = o.pos;
            o.pos += o.vel * dt;
            o.life -= dt;
        }
    }

    pub(super) fn add_orb(&mut self, pos: V2, vel: V2) {
        if self.orbs.len() < MAX_ORBS {
            self.orbs.push(Orb {
                pos,
                prev: pos,
                vel,
                life: 6.0,
                heavy: false,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encounter(kind: Kind, elite: bool) -> Game {
        let mut game = Game::new(7);
        game.state = State::Playing;
        game.player.pos = v2(900.0, 500.0);
        game.add_enemy(kind, v2(600.0, 500.0), V2::default(), elite);
        game.enemies[0].cooldown = 0.0;
        game
    }

    fn volley(game: &mut Game, dt: f32) {
        game.enemies[0].act = Act::Move;
        game.enemies[0].cooldown = 0.0;
        game.update_enemies(0.0);
        assert!(game.enemies[0].winding_up());
        assert!(game.orbs.is_empty(), "every volley starts with a tell");
        for _ in 0..200 {
            game.update_enemies(dt);
            if !game.enemies[0].winding_up() {
                return;
            }
        }
        panic!("windup did not finish");
    }

    #[test]
    fn ranged_enemies_telegraph_before_firing_at_all_frame_rates() {
        for (kind, normal, elite_count) in [
            (Kind::Skirmisher, 3, 5),
            (Kind::Sniper, 1, 3),
            (Kind::Spinner, 8, 12),
            (Kind::Bomber, 8, 12),
            (Kind::Sentinel, 8, 12),
        ] {
            for elite in [false, true] {
                for fps in [30, 60, 144] {
                    let mut game = encounter(kind, elite);
                    volley(&mut game, 1.0 / fps as f32);
                    assert_eq!(
                        game.orbs.len(),
                        if elite { elite_count } else { normal },
                        "{kind:?}, elite {elite}, {fps} fps"
                    );
                    assert!(game.orbs.iter().all(|orb| {
                        orb.life > 0.0
                            && orb.life <= 7.0
                            && orb.vel.len() >= 99.0
                            && orb.vel.len() <= 721.0
                    }));
                    assert!(game.enemies[0].cooldown > 0.0);
                }
            }
        }
    }

    #[test]
    fn sniper_gives_a_full_second_to_leave_its_locked_sightline() {
        let mut game = encounter(Kind::Sniper, false);
        game.update_enemies(0.0);
        let locked = game.enemies[0].facing;
        game.player.pos = v2(900.0, 800.0);
        game.update_enemies(1.0);
        assert!(game.orbs.is_empty());
        assert_eq!(game.enemies[0].facing, locked);
        game.update_enemies(0.16);
        assert_eq!(game.orbs.len(), 1);
        assert!((game.orbs[0].vel - v2(720.0, 0.0)).len() < 1e-3);
    }

    #[test]
    fn spinner_and_bomber_rings_leave_even_lanes_with_distinct_speeds() {
        for kind in [Kind::Spinner, Kind::Bomber] {
            let mut game = encounter(kind, false);
            volley(&mut game, 1.0 / 60.0);
            let directions: Vec<V2> = game.orbs.iter().map(|orb| orb.vel.norm()).collect();
            for i in 0..directions.len() {
                let a = directions[i].angle();
                let b = directions[(i + 1) % directions.len()].angle();
                let gap = (b - a).rem_euclid(TAU);
                assert!((gap - TAU / 8.0).abs() < 1e-4);
            }
            if kind == Kind::Bomber {
                assert!(game.orbs.iter().all(|orb| orb.heavy));
                let slow = game.orbs.iter().filter(|orb| orb.vel.len() < 120.0).count();
                assert_eq!(
                    slow, 4,
                    "stagger the field instead of forming one solid wall"
                );
                assert!(game.orbs.iter().all(|orb| orb.vel.len() < 150.0));
            } else {
                assert!(
                    game.orbs
                        .iter()
                        .all(|orb| !orb.heavy && orb.vel.len() > 280.0)
                );
            }
        }
    }

    #[test]
    fn sentinel_alternates_crosses_and_aimed_fans() {
        let mut game = encounter(Kind::Sentinel, false);
        volley(&mut game, 1.0 / 60.0);
        assert_eq!(game.orbs.len(), 8);
        for pair in game.orbs.as_chunks::<2>().0 {
            assert!((pair[0].vel.norm() - pair[1].vel.norm()).len() < 1e-4);
            assert!(pair[1].vel.len() > pair[0].vel.len() + 70.0);
        }
        game.orbs.clear();
        volley(&mut game, 1.0 / 60.0);
        assert_eq!(game.orbs.len(), 7);
        let target = (game.player.pos - game.enemies[0].pos).norm();
        assert!((game.orbs[3].vel.norm() - target).len() < 0.01);
        game.orbs.clear();
        volley(&mut game, 1.0 / 60.0);
        assert_eq!(game.orbs.len(), 8);
    }

    #[test]
    fn skirmishers_orbit_while_splitters_commit_to_a_long_rush() {
        let mut skirmisher = encounter(Kind::Skirmisher, false);
        skirmisher.enemies[0].cooldown = 10.0;
        skirmisher.update_enemies(0.2);
        let vel = skirmisher.enemies[0].vel;
        assert!(vel.y.abs() > vel.x.abs() * 2.0);

        let mut splitter = encounter(Kind::Splitter, false);
        splitter.update_enemies(0.0);
        splitter.update_enemies(0.4);
        assert!(splitter.enemies[0].winding_up());
        splitter.update_enemies(0.16);
        assert_eq!(splitter.enemies[0].act, Act::Strike);
        assert!(splitter.enemies[0].vel.len() > 700.0);
        splitter.update_enemies(0.4);
        assert_eq!(splitter.enemies[0].act, Act::Strike);
        splitter.update_enemies(0.3);
        assert_eq!(splitter.enemies[0].act, Act::Recover);
    }

    #[test]
    fn splitter_children_do_not_split_again_or_bypass_split_suppression() {
        for split in [false, true] {
            let mut game = encounter(Kind::Splitter, true);
            let splitter = game.enemies.pop().unwrap();
            game.kill(splitter, split);
            assert_eq!(game.enemies.len(), if split { 4 } else { 0 });
            assert!(
                game.enemies
                    .iter()
                    .all(|e| e.kind == Kind::Swarm && !e.elite)
            );
            let children = std::mem::take(&mut game.enemies);
            for child in children {
                game.kill(child, true);
            }
            assert!(game.enemies.is_empty());
        }
    }

    #[test]
    fn new_volleys_share_the_projectile_cap() {
        for kind in [
            Kind::Skirmisher,
            Kind::Sniper,
            Kind::Spinner,
            Kind::Bomber,
            Kind::Sentinel,
        ] {
            let mut game = encounter(kind, true);
            for _ in 0..MAX_ORBS - 1 {
                game.add_orb(v2(100.0, 100.0), V2::default());
            }
            game.enemies[0].act = Act::Windup;
            game.enemies[0].aim = v2(1.0, 0.0);
            game.enemies[0].act_t = 0.0;
            game.update_enemies(0.0);
            assert_eq!(game.orbs.len(), MAX_ORBS, "{kind:?}");
        }
    }

    #[test]
    fn wave_rosters_preserve_unlocks_and_eventually_spawn_every_kind() {
        let mut game = Game::new(29);
        game.state = State::Playing;
        for difficulty in Difficulty::ALL {
            game.difficulty = difficulty;
            let mut seen = Vec::new();
            for wave in 1..=24 {
                game.start_wave(wave);
                game.spawns.clear();
                let roster = game.wave_roster();
                while game.budget > 0 {
                    game.spawn_group();
                }
                for spawn in &game.spawns {
                    assert!(roster.contains(&spawn.kind));
                    assert!(spawn.kind.spec().first_wave <= game.effective_wave());
                    if !seen.contains(&spawn.kind) {
                        seen.push(spawn.kind);
                    }
                }
                for kind in roster {
                    let count = game
                        .spawns
                        .iter()
                        .filter(|spawn| spawn.kind == *kind)
                        .count();
                    assert!(
                        count >= kind.spec().group * 2,
                        "{difficulty:?}, wave {wave}: {kind:?} must shape the encounter"
                    );
                }
            }
            assert!(Kind::ALL.into_iter().all(|kind| seen.contains(&kind)));
        }
    }
}
