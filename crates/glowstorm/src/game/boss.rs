//! PRISM, the boss that arrives every fifth wave.

use super::*;

pub const BOSS_RADIUS: f32 = 62.0;
const ENTER_TIME: f32 = 2.4;
/// Distance the boss tries to keep from the player.
const KEEP_AWAY: f32 = 360.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Rest,
    Ring,
    Spiral,
    Fan,
    Charge,
    Summon,
}

pub struct Boss {
    pub pos: V2,
    vel: V2,
    pub(super) impulse: V2,
    pub hp: f32,
    pub max_hp: f32,
    pub flash: f32,
    pub spin: f32,
    pub core_spin: f32,
    /// Seconds left of the entrance; invulnerable meanwhile.
    pub entering: f32,
    pub pattern: Pattern,
    pattern_t: f32,
    step: u32,
    step_t: f32,
    /// Seconds left of an attack's wind-up (the boss blinks).
    pub windup: f32,
    emit_angle: f32,
    pub charge_dir: V2,
    pub level: u32,
    from: V2,
    home: V2,
    /// Difficulty speed factor for its bullets and pauses.
    pace: f32,
}

impl Boss {
    pub(super) fn resize_arena(&mut self, size: V2) {
        let r = BOSS_RADIUS + WALL;
        self.home = self.home.clamp(v2(r, r), size - v2(r, r));
        if self.entering <= 0.0 {
            self.pos = self.pos.clamp(v2(r, r), size - v2(r, r));
        }
    }

    /// 1 above 60% health, 2 above 25%, then 3.
    pub fn phase(&self) -> u32 {
        let left = self.hp / self.max_hp;
        if left > 0.6 {
            1
        } else if left > 0.25 {
            2
        } else {
            3
        }
    }

    pub fn charging(&self) -> bool {
        self.pattern == Pattern::Charge && self.step == 0
    }
}

impl Game {
    pub(super) fn spawn_boss(&mut self) {
        let level = (self.wave / 5).max(1);
        // +25% per boss level, capped at 2× like regular enemies.
        let growth = (1.0 + 0.25 * (level - 1) as f32).min(2.0);
        let max_hp = 130.0 * self.difficulty.tuning().boss_hp * growth;
        let pace = self.difficulty.tuning().enemy_speed;
        // Settle on the side of the arena away from the player.
        let center = self.arena_size * 0.5;
        let mut away = (center - self.player.pos).norm();
        if away == V2::default() {
            away = self.rng.dir();
        }
        let home = center + away * 300.0;
        let from = v2(home.x, -260.0);
        self.boss = Some(Boss {
            pos: from,
            vel: V2::default(),
            impulse: V2::default(),
            hp: max_hp,
            max_hp,
            flash: 0.0,
            spin: 0.0,
            core_spin: 0.0,
            entering: ENTER_TIME,
            pattern: Pattern::Rest,
            pattern_t: 1.0,
            step: 0,
            step_t: 0.0,
            windup: 0.0,
            emit_angle: 0.0,
            charge_dir: V2::default(),
            level,
            from,
            home,
            pace,
        });
        self.rings.push(Ring {
            pos: home,
            t: 0.0,
            dur: ENTER_TIME,
            hue: Hue::Violet,
            inward: true,
        });
    }

    pub(super) fn update_boss(&mut self, dt: f32) {
        let Some(mut b) = self.boss.take() else {
            return;
        };
        let phase = b.phase();
        b.flash -= dt;
        b.windup = (b.windup - dt).max(0.0);
        b.spin += dt * (0.5 + 0.35 * phase as f32);
        b.core_spin -= dt * (1.4 + 0.9 * phase as f32);

        if b.entering > 0.0 {
            b.entering -= dt;
            let t = 1.0 - (b.entering / ENTER_TIME).max(0.0);
            let ease = 1.0 - (1.0 - t).powi(3);
            b.pos = b.from + (b.home - b.from) * ease;
            if b.entering <= 0.0 {
                self.boss_landed(b.pos);
            }
            self.boss = Some(b);
            return;
        }

        let to_player = self.player.pos - b.pos;
        let dist = to_player.len();
        // On top of the player there is no direction; pick any so it backs off.
        let dir = if dist > 1.0 {
            to_player.norm()
        } else {
            v2(1.0, 0.0)
        };

        // Movement: hover at range and circle the player, or dash when charging.
        if b.pattern == Pattern::Charge && b.step == 1 {
            b.pos += b.vel * dt;
        } else {
            let radial = ((dist - KEEP_AWAY) / KEEP_AWAY).clamp(-1.0, 1.0) * 170.0;
            let orbit = dir.perp() * (60.0 + 35.0 * phase as f32);
            let mut desired = dir * radial + orbit;
            if b.windup > 0.0 {
                desired = desired * 0.15;
            }
            b.vel += (desired - b.vel) * (1.0 - (-2.0 * dt).exp());
            b.pos += b.vel * dt;
        }
        b.pos += b.impulse * dt;
        b.impulse = b.impulse * (-4.0 * dt).exp();
        let r = BOSS_RADIUS + WALL;
        if b.pos.x < r || b.pos.x > self.arena_size.x - r {
            b.pos.x = b.pos.x.clamp(r, self.arena_size.x - r);
            b.vel.x = -b.vel.x;
            b.impulse.x = 0.0;
        }
        if b.pos.y < r || b.pos.y > self.arena_size.y - r {
            b.pos.y = b.pos.y.clamp(r, self.arena_size.y - r);
            b.vel.y = -b.vel.y;
            b.impulse.y = 0.0;
        }

        b.pattern_t -= dt;
        b.step_t -= dt;
        match b.pattern {
            Pattern::Rest => {
                if b.pattern_t <= 0.0 {
                    self.next_pattern(&mut b);
                }
            }
            Pattern::Ring => {
                if b.windup <= 0.0 && b.step_t <= 0.0 {
                    let count = 16 + 6 * phase as usize;
                    let offset = b.emit_angle + b.step as f32 * TAU / count as f32 / 2.0;
                    for i in 0..count {
                        let a = offset + i as f32 * TAU / count as f32;
                        let d = V2::from_angle(a);
                        self.add_orb(b.pos + d * BOSS_RADIUS, d * 215.0 * b.pace);
                    }
                    self.sfx.push(Sfx::BossBlast);
                    self.shake = (self.shake + 4.0).min(18.0);
                    self.rings.push(Ring {
                        pos: b.pos,
                        t: 0.0,
                        dur: 0.35,
                        hue: Hue::Violet,
                        inward: false,
                    });
                    b.step += 1;
                    b.step_t = 0.3;
                    // Later phases follow up with an offset second ring.
                    if b.step >= phase.min(2) {
                        b.emit_angle += 0.4;
                        b.rest();
                    }
                }
            }
            Pattern::Spiral => {
                if b.step_t <= 0.0 {
                    b.step_t = if phase >= 3 { 0.065 } else { 0.09 } / b.pace;
                    for arm in 0..phase {
                        let a = b.emit_angle + arm as f32 * TAU / phase as f32;
                        let d = V2::from_angle(a);
                        self.add_orb(b.pos + d * BOSS_RADIUS * 0.8, d * 205.0 * b.pace);
                    }
                    b.emit_angle += 0.33;
                }
                if b.pattern_t <= 0.0 {
                    b.rest();
                }
            }
            Pattern::Fan => {
                if b.windup <= 0.0 && b.step_t <= 0.0 {
                    let lead = (dist / 380.0).min(0.8) * 0.7;
                    let aim = (self.player.pos + self.player.vel * lead - b.pos).norm();
                    let n = 3 + 2 * phase as i32;
                    for i in 0..n {
                        let a = aim.angle() + (i - n / 2) as f32 * 0.16;
                        let d = V2::from_angle(a);
                        self.add_orb(b.pos + d * BOSS_RADIUS, d * 330.0 * b.pace);
                    }
                    self.sfx.push(Sfx::EnemyShoot);
                    b.step += 1;
                    b.step_t = 0.32;
                    if b.step >= 3 {
                        b.rest();
                    }
                }
            }
            Pattern::Charge => match b.step {
                0 => {
                    // Track the player during the wind-up, lock just before release.
                    if b.windup > 0.15 {
                        b.charge_dir = dir;
                    }
                    if b.windup <= 0.0 {
                        b.step = 1;
                        b.step_t = 0.6;
                        b.vel = b.charge_dir * 1150.0;
                        self.sfx.push(Sfx::Lunge);
                    }
                }
                _ => {
                    if b.step_t <= 0.0 {
                        self.shockwaves.push(Shockwave {
                            pos: b.pos,
                            r: 0.0,
                            hit: false,
                        });
                        self.sfx.push(Sfx::Shock);
                        self.shake = (self.shake + 8.0).min(18.0);
                        self.push_motes(b.pos, 360.0, 300.0);
                        b.vel = b.vel * 0.1;
                        b.rest();
                    }
                }
            },
            Pattern::Summon => {
                let kind = if phase >= 3 {
                    Kind::Swarm
                } else {
                    Kind::Chaser
                };
                let count = if kind == Kind::Swarm { 5 } else { 3 };
                for i in 0..count {
                    let at = b.pos + V2::from_angle(i as f32 * TAU / count as f32) * 110.0;
                    self.spawns.push(Spawn {
                        kind,
                        pos: at,
                        t: SPAWN_TIME,
                        elite: false,
                    });
                    self.rings.push(Ring {
                        pos: at,
                        t: 0.0,
                        dur: SPAWN_TIME,
                        hue: kind.hue(),
                        inward: true,
                    });
                }
                b.rest();
            }
        }

        if b.hp <= 0.0 {
            self.boss_defeated(b);
        } else {
            self.boss = Some(b);
        }
    }

    fn next_pattern(&mut self, b: &mut Boss) {
        let phase = b.phase();
        let mut options = vec![Pattern::Ring, Pattern::Spiral, Pattern::Fan];
        if phase >= 2 {
            options.extend([Pattern::Charge, Pattern::Summon]);
        }
        // Don't summon into a crowd, and never repeat the last attack.
        if self.enemies.len() > 8 {
            options.retain(|p| *p != Pattern::Summon);
        }
        let last = b.pattern;
        options.retain(|p| *p != last);
        let pick = options[(self.rng.next() as usize) % options.len()];
        b.pattern = pick;
        b.step = 0;
        b.step_t = 0.0;
        match pick {
            Pattern::Ring => b.windup = 0.55,
            Pattern::Spiral => b.pattern_t = 2.2 + 0.4 * phase as f32,
            Pattern::Fan => b.windup = 0.35,
            Pattern::Charge => b.windup = 0.85,
            Pattern::Summon | Pattern::Rest => {}
        }
    }

    fn boss_landed(&mut self, at: V2) {
        self.shake = 18.0;
        self.flash = 0.12;
        self.sfx.push(Sfx::BigExplode);
        self.push_motes(at, 600.0, 500.0);
        for (t, hue) in [(0.0, Hue::Violet), (-0.15, Hue::White)] {
            self.rings.push(Ring {
                pos: at,
                t,
                dur: 0.7,
                hue,
                inward: false,
            });
        }
        for _ in 0..60 {
            let v = self.rng.dir() * self.rng.range(150.0, 700.0);
            let life = self.rng.range(0.4, 1.0);
            self.emit(at, v, life, Hue::Violet, 2);
        }
    }

    pub(super) fn boss_defeated(&mut self, b: Boss) {
        let at = b.pos;
        self.slow(1.6, 0.25);
        self.flash = 0.3;
        self.shake = 18.0;
        self.hitstop = 0.1;
        self.sfx.push(Sfx::BigExplode);
        self.sfx.push(Sfx::Victory);
        let points = self.award(1500 * u64::from(b.level));
        self.popup(at + v2(0.0, -BOSS_RADIUS), format!("+{points}"), true);
        self.push_motes(at, 900.0, 700.0);

        for (i, hue) in [Hue::Violet, Hue::White, Hue::Magenta, Hue::Violet]
            .into_iter()
            .enumerate()
        {
            self.rings.push(Ring {
                pos: at,
                t: -0.18 * i as f32,
                dur: 0.9,
                hue,
                inward: false,
            });
        }
        for _ in 0..160 {
            let v = self.rng.dir() * self.rng.range(100.0, 1000.0);
            let hue = match self.rng.next() % 3 {
                0 => Hue::White,
                1 => Hue::Magenta,
                _ => Hue::Violet,
            };
            let life = self.rng.range(0.6, 1.6);
            self.emit(at, v, life, hue, 2);
        }
        // Its bullets burst into sparks and its minions go down with it.
        let orbs: Vec<V2> = self.orbs.drain(..).map(|o| o.pos).collect();
        for pos in orbs {
            let v = self.rng.dir() * 120.0;
            self.emit(pos, v, 0.4, Hue::Red, 0);
        }
        let minions = std::mem::take(&mut self.enemies);
        for e in minions {
            self.kill(e, true);
        }
        self.spawns.clear();

        for (offset, kind) in [
            (v2(0.0, -50.0), PickupKind::Heart),
            (v2(0.0, 50.0), PickupKind::Rapid),
        ] {
            self.pickups.push(Pickup {
                kind,
                pos: at + offset,
                life: 14.0,
            });
        }
    }
}

impl Boss {
    /// Ends the current attack; the pause shrinks as the boss gets angrier.
    fn rest(&mut self) {
        let phase = self.phase();
        self.pattern = Pattern::Rest;
        self.pattern_t = (1.3 - 0.3 * phase as f32).max(0.5) / self.pace;
        self.windup = 0.0;
    }
}
