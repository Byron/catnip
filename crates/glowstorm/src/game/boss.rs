//! Five boss encounters, rotating every fifth wave.

use super::*;

pub const BOSS_RADIUS: f32 = 62.0;
const ENTER_TIME: f32 = 2.4;
/// Distance the boss tries to keep from the player.
const KEEP_AWAY: f32 = 360.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossKind {
    Prism,
    Helix,
    Lotus,
    Loom,
    Eclipse,
}

impl BossKind {
    pub const ALL: [Self; 5] = [
        Self::Prism,
        Self::Helix,
        Self::Lotus,
        Self::Loom,
        Self::Eclipse,
    ];

    pub fn for_level(level: u32) -> Self {
        Self::ALL[level.saturating_sub(1) as usize % Self::ALL.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Prism => "PRISM",
            Self::Helix => "HELIX",
            Self::Lotus => "LOTUS",
            Self::Loom => "LOOM",
            Self::Eclipse => "ECLIPSE",
        }
    }

    pub fn hue(self) -> Hue {
        match self {
            Self::Prism => Hue::Violet,
            Self::Helix => Hue::Cyan,
            Self::Lotus => Hue::Magenta,
            Self::Loom => Hue::Orange,
            Self::Eclipse => Hue::Yellow,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Rest,
    Ring,
    Spiral,
    Fan,
    Charge,
    Summon,
    Helix,
    Petals,
    Curtain,
    Crossfire,
}

pub struct Boss {
    pub kind: BossKind,
    pub pos: V2,
    vel: V2,
    pub(super) impulse: V2,
    pub hp: f32,
    pub max_hp: f32,
    pub flash: f32,
    pub stun: f32,
    pub spin: f32,
    pub core_spin: f32,
    /// Seconds left of the entrance; invulnerable meanwhile.
    pub entering: f32,
    pub pattern: Pattern,
    last_pattern: Pattern,
    attack_index: usize,
    pattern_t: f32,
    step: u32,
    step_t: f32,
    /// Seconds left of an attack's wind-up (the boss blinks).
    pub windup: f32,
    /// Initial emitter direction; centers the curtain's first gap.
    pub emit_angle: f32,
    pub charge_dir: V2,
    pub level: u32,
    from: V2,
    home: V2,
    /// Difficulty speed factor for its bullets and pauses.
    pace: f32,
}

impl Boss {
    pub(super) fn stagger(&mut self, damage: f32) {
        if self.entering > 0.0 || damage < 5.0 {
            return;
        }
        if self.stun <= 0.0 {
            self.vel = self.vel * 0.5;
        }
        self.stun = self.stun.max((damage * 0.05).min(0.35));
    }

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
        let kind = BossKind::for_level(level);
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
        let r = BOSS_RADIUS + WALL;
        let home = (center + away * 300.0).clamp(v2(r, r), self.arena_size - v2(r, r));
        let from = v2(home.x, -260.0);
        self.boss = Some(Boss {
            kind,
            pos: from,
            vel: V2::default(),
            impulse: V2::default(),
            hp: max_hp,
            max_hp,
            flash: 0.0,
            stun: 0.0,
            spin: 0.0,
            core_spin: 0.0,
            entering: ENTER_TIME,
            pattern: Pattern::Rest,
            last_pattern: Pattern::Rest,
            attack_index: 0,
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
            hue: kind.hue(),
            inward: true,
        });
    }

    pub(super) fn update_boss(&mut self, dt: f32) {
        let Some(mut b) = self.boss.take() else {
            return;
        };
        let phase = b.phase();
        b.flash -= dt;
        let stunned = b.stun > 0.0;
        let attack_dt = if stunned {
            0.0
        } else {
            (dt - b.windup).max(0.0)
        };
        if !stunned {
            b.windup = (b.windup - dt).max(0.0);
        }
        b.spin += dt
            * if stunned {
                -3.0
            } else {
                0.5 + 0.35 * phase as f32
            };
        b.core_spin -= dt * (1.4 + 0.9 * phase as f32);

        if b.entering > 0.0 {
            b.entering -= dt;
            let t = 1.0 - (b.entering / ENTER_TIME).max(0.0);
            let ease = 1.0 - (1.0 - t).powi(3);
            b.pos = b.from + (b.home - b.from) * ease;
            if b.entering <= 0.0 {
                self.boss_landed(b.pos, b.kind.hue());
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
        if stunned {
            b.vel = b.vel * (-0.8 * dt).exp();
            b.pos += b.vel * dt;
        } else if b.pattern == Pattern::Charge && b.step == 1 {
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
        b.impulse = b.impulse * (-(if stunned { 2.5 } else { 4.0 }) * dt).exp();
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

        b.pattern_t -= attack_dt;
        b.step_t -= attack_dt;
        match b.pattern {
            _ if stunned || (b.windup > 0.0 && b.pattern != Pattern::Charge) => {}
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
                        self.boss_orb(&b, d, 215.0);
                    }
                    self.sfx.push(Sfx::BossBlast);
                    self.shake = (self.shake + 4.0).min(18.0);
                    self.rings.push(Ring {
                        pos: b.pos,
                        t: 0.0,
                        dur: 0.35,
                        hue: b.kind.hue(),
                        inward: false,
                    });
                    b.step += 1;
                    b.step_t = (b.step_t + 0.3).max(0.0);
                    // Later phases follow up with an offset second ring.
                    if b.step >= phase.min(2) {
                        b.emit_angle += 0.4;
                        b.rest();
                    }
                }
            }
            Pattern::Spiral => {
                if b.step_t <= 0.0 {
                    b.step_t = (b.step_t + if phase >= 3 { 0.065 } else { 0.09 } / b.pace).max(0.0);
                    for arm in 0..phase {
                        let a = b.emit_angle + arm as f32 * TAU / phase as f32;
                        let d = V2::from_angle(a);
                        self.boss_orb(&b, d, 205.0);
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
                        self.boss_orb(&b, d, 330.0);
                    }
                    self.sfx.push(Sfx::EnemyShoot);
                    b.step += 1;
                    b.step_t = (b.step_t + 0.32).max(0.0);
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
            Pattern::Helix => {
                if b.step_t <= 0.0 {
                    let arms = 1 + phase;
                    for turn in [-1.0, 1.0] {
                        for arm in 0..arms {
                            let angle = b.emit_angle
                                + turn * (0.13 + b.step as f32 * 0.19)
                                + arm as f32 * TAU / arms as f32;
                            self.boss_orb(&b, V2::from_angle(angle), 180.0 + turn * 15.0);
                        }
                    }
                    b.step += 1;
                    b.step_t = (b.step_t + (0.155 - 0.01 * phase as f32) / b.pace).max(0.0);
                    if b.step >= 18 + 4 * phase {
                        b.rest();
                    }
                }
            }
            Pattern::Petals => {
                if b.step_t <= 0.0 {
                    let petals = 4 + phase;
                    for petal in 0..petals {
                        let center = b.emit_angle
                            + petal as f32 * TAU / petals as f32
                            + b.step as f32 * 0.08;
                        for offset in -2_i32..=2 {
                            let angle = center + offset as f32 * 0.065;
                            let speed = 160.0 + (2 - offset.abs()) as f32 * 24.0;
                            self.boss_orb(&b, V2::from_angle(angle), speed);
                        }
                    }
                    self.sfx.push(Sfx::BossBlast);
                    b.step += 1;
                    b.step_t = (b.step_t + 0.75 / b.pace).max(0.0);
                    if b.step >= 2 + phase {
                        b.rest();
                    }
                }
            }
            Pattern::Curtain => {
                if b.step_t <= 0.0 {
                    let count = 25 + 4 * phase;
                    let sweep = (b.step as f32 * 0.4).sin() * 0.2;
                    for i in 0..count {
                        let offset = -1.25 + 2.5 * i as f32 / (count - 1) as f32;
                        // The lane stays wide enough for the ship even near the boss.
                        if offset.abs() < 0.23 {
                            continue;
                        }
                        let dir = V2::from_angle(b.emit_angle + sweep + offset);
                        self.boss_orb(&b, dir, 185.0);
                    }
                    self.sfx.push(Sfx::EnemyShoot);
                    b.step += 1;
                    b.step_t = (b.step_t + 0.34 / b.pace).max(0.0);
                    if b.step >= 8 + phase {
                        b.rest();
                    }
                }
            }
            Pattern::Crossfire => {
                if b.step_t <= 0.0 {
                    for arm in 0..4 {
                        let angle = b.emit_angle + b.step as f32 * 0.1 + arm as f32 * TAU / 4.0;
                        let dir = V2::from_angle(angle);
                        self.boss_orb(&b, dir, 185.0);
                        if phase >= 2 {
                            self.boss_orb(&b, dir, 245.0);
                        }
                    }
                    if b.step % 6 == 5 {
                        let aim = (self.player.pos - b.pos).angle();
                        let half = if phase >= 3 { 1 } else { 0 };
                        for offset in -half..=half {
                            self.boss_orb(&b, V2::from_angle(aim + offset as f32 * 0.15), 280.0);
                        }
                        self.sfx.push(Sfx::EnemyShoot);
                    }
                    b.step += 1;
                    b.step_t = (b.step_t + 0.14 / b.pace).max(0.0);
                    if b.step >= 18 + 4 * phase {
                        b.rest();
                    }
                }
            }
        }

        b.stun = (b.stun - dt).max(0.0);
        if b.hp <= 0.0 {
            self.boss_defeated(b);
        } else {
            self.boss = Some(b);
        }
    }

    fn next_pattern(&mut self, b: &mut Boss) {
        let phase = b.phase();
        let sequence = match b.kind {
            BossKind::Prism => None,
            BossKind::Helix => Some([Pattern::Helix, Pattern::Fan, Pattern::Helix, Pattern::Ring]),
            BossKind::Lotus => Some([
                Pattern::Petals,
                Pattern::Ring,
                Pattern::Petals,
                Pattern::Fan,
            ]),
            BossKind::Loom => Some([
                Pattern::Curtain,
                Pattern::Fan,
                Pattern::Curtain,
                Pattern::Ring,
            ]),
            BossKind::Eclipse => Some([
                Pattern::Crossfire,
                Pattern::Ring,
                Pattern::Crossfire,
                Pattern::Fan,
            ]),
        };
        let pick = if let Some(sequence) = sequence {
            let pick = sequence[b.attack_index % sequence.len()];
            b.attack_index = b.attack_index.wrapping_add(1);
            b.emit_angle = (self.player.pos - b.pos).angle();
            pick
        } else {
            let mut options = vec![Pattern::Ring, Pattern::Spiral, Pattern::Fan];
            if phase >= 2 {
                options.push(Pattern::Charge);
                if self.enemies.len() <= 8 {
                    options.push(Pattern::Summon);
                }
            }
            options.retain(|p| *p != b.last_pattern);
            options[(self.rng.next() as usize) % options.len()]
        };
        b.pattern = pick;
        b.step = 0;
        b.step_t = 0.0;
        b.pattern_t = 2.2 + 0.4 * phase as f32;
        b.charge_dir = (self.player.pos - b.pos).norm();
        b.windup = match pick {
            Pattern::Ring | Pattern::Summon => 0.55,
            Pattern::Spiral => 0.6,
            Pattern::Fan => 0.45,
            Pattern::Charge => 0.85,
            Pattern::Helix | Pattern::Crossfire => 0.7,
            Pattern::Petals => 0.75,
            Pattern::Curtain => 0.8,
            Pattern::Rest => unreachable!(),
        };
        self.rings.push(Ring {
            pos: b.pos,
            t: 0.0,
            dur: b.windup,
            hue: b.kind.hue(),
            inward: true,
        });
    }

    fn boss_orb(&mut self, b: &Boss, dir: V2, speed: f32) {
        let pos = b.pos + dir * BOSS_RADIUS;
        let vel = dir * speed * b.pace;
        // Give a nearby ship time to react instead of materializing a hit on it.
        if (pos - self.player.pos).len() > PLAYER_RADIUS + 9.0 + vel.len() * 0.25 {
            self.add_orb(pos, vel);
        }
    }

    fn boss_landed(&mut self, at: V2, hue: Hue) {
        self.shake = 18.0;
        self.flash = 0.12;
        self.sfx.push(Sfx::BigExplode);
        self.push_motes(at, 600.0, 500.0);
        for (t, hue) in [(0.0, hue), (-0.15, Hue::White)] {
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
            self.emit(at, v, life, hue, 2);
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

        for (i, hue) in [b.kind.hue(), Hue::White, Hue::Magenta, b.kind.hue()]
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
                _ => b.kind.hue(),
            };
            let life = self.rng.range(0.6, 1.6);
            self.emit(at, v, life, hue, 2);
        }
        // Its bullets burst into sparks and its minions go down with it.
        let orbs: Vec<V2> = self.orbs.drain(..).map(|o| o.pos).collect();
        self.shockwaves.clear();
        for pos in orbs {
            let v = self.rng.dir() * 120.0;
            self.emit(pos, v, 0.4, Hue::Red, 0);
        }
        let minions = std::mem::take(&mut self.enemies);
        for e in minions {
            self.kill(e, false);
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
        self.last_pattern = self.pattern;
        self.pattern = Pattern::Rest;
        self.pattern_t = ((1.5 - 0.25 * phase as f32) / self.pace).max(0.65);
        self.windup = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encounter(kind: BossKind, phase: u32) -> Game {
        let mut game = Game::new(7);
        game.wave = (kind as u32 + 1) * 5;
        game.state = State::Playing;
        game.spawn_boss();
        let b = game.boss.as_mut().unwrap();
        b.entering = 0.0;
        b.pos = game.arena_size * 0.5;
        b.hp = b.max_hp * [1.0, 0.5, 0.2][phase as usize - 1];
        b.pattern_t = 0.0;
        game.player.pos = b.pos + v2(900.0, 0.0);
        game.update_boss(0.0);
        game
    }

    #[test]
    fn fifth_waves_cycle_every_boss_and_health_growth_stops() {
        let mut game = Game::new(7);
        for level in 1..=15 {
            game.wave = level * 5;
            game.spawn_boss();
            let b = game.boss.as_ref().unwrap();
            assert_eq!(b.kind, BossKind::ALL[(level as usize - 1) % 5]);
            assert_eq!(b.level, level);
            assert!(b.max_hp > 0.0 && b.max_hp <= 260.0);
            assert_eq!(game.rings.last().unwrap().hue, b.kind.hue());
        }
    }

    #[test]
    fn attacks_wait_for_windup_and_freeze_during_stun_and_pause() {
        for kind in BossKind::ALL {
            let mut game = encounter(kind, 3);
            let windup = game.boss.as_ref().unwrap().windup;
            assert!(windup >= 0.45, "{kind:?}");
            game.update_boss(windup * 0.5);
            assert!(game.orbs.is_empty() && game.spawns.is_empty());
            let b = game.boss.as_mut().unwrap();
            b.stun = 0.3;
            let timers = (b.windup, b.step_t, b.pattern_t);
            game.update_boss(0.2);
            let b = game.boss.as_ref().unwrap();
            assert_eq!((b.windup, b.step_t, b.pattern_t), timers);
            assert!(game.orbs.is_empty() && game.spawns.is_empty());
            game.state = State::Paused;
            let position = b.pos;
            game.update(
                0.1,
                &Input::default(),
                &View {
                    scale: 1.0,
                    size: v2(1600.0, 900.0),
                },
            );
            let b = game.boss.as_mut().unwrap();
            assert_eq!(b.pos, position);
            assert_eq!((b.windup, b.step_t, b.pattern_t), timers);
            b.stun = 0.0;
            game.update_boss(windup);
            assert!(
                !game.orbs.is_empty() || !game.spawns.is_empty(),
                "{kind:?} resumes"
            );
        }
    }

    #[test]
    fn every_attack_has_a_telegraph_and_prism_remembers_its_last_attack() {
        for kind in BossKind::ALL {
            let mut game = encounter(kind, 3);
            let mut b = game.boss.take().unwrap();
            for _ in 0..64 {
                let previous = b.pattern;
                b.rest();
                assert!(b.pattern_t >= 0.65);
                game.next_pattern(&mut b);
                assert_ne!(b.pattern, previous, "{kind:?} repeats {previous:?}");
                assert!(b.windup >= 0.45, "{kind:?} {:?}", b.pattern);
                assert_eq!(game.rings.last().unwrap().dur, b.windup);
            }
        }
    }

    #[test]
    fn new_patterns_grow_in_density_and_keep_their_cadence_across_frame_rates() {
        for kind in BossKind::ALL.into_iter().skip(1) {
            let mut previous_count = 0;
            for phase in 1..=3 {
                let mut reference: Option<(usize, f32)> = None;
                for fps in [30, 60, 144] {
                    let mut game = encounter(kind, phase);
                    let mut count = 0;
                    let mut elapsed = 0.0;
                    for _ in 0..fps * 10 {
                        let dt = 1.0 / fps as f32;
                        game.update_boss(dt);
                        elapsed += dt;
                        count += game.orbs.len();
                        assert!(game.orbs.iter().all(|o| {
                            o.pos.x.is_finite()
                                && o.pos.y.is_finite()
                                && o.vel.x.is_finite()
                                && o.vel.y.is_finite()
                                && o.life > 0.0
                                && o.life <= 6.0
                        }));
                        game.orbs.clear();
                        let b = game.boss.as_ref().unwrap();
                        let r = BOSS_RADIUS + WALL;
                        assert!(b.pos.x >= r && b.pos.x <= game.arena_size.x - r);
                        assert!(b.pos.y >= r && b.pos.y <= game.arena_size.y - r);
                        if b.pattern == Pattern::Rest {
                            assert!(b.pattern_t >= 0.65);
                            break;
                        }
                    }
                    assert_eq!(game.boss.as_ref().unwrap().pattern, Pattern::Rest);
                    assert!(
                        (60..MAX_ORBS).contains(&count),
                        "{kind:?} phase {phase}: {count}"
                    );
                    if let Some((expected, duration)) = reference {
                        assert_eq!(count, expected, "{kind:?} phase {phase} at {fps} FPS");
                        assert!(
                            (elapsed - duration).abs() < 0.07,
                            "{kind:?} cadence at {fps} FPS"
                        );
                    } else {
                        assert!(count > previous_count, "{kind:?} gets denser each phase");
                        previous_count = count;
                        reference = Some((count, elapsed));
                    }
                }
            }
        }
    }

    #[test]
    fn flower_and_curtain_waves_leave_ship_sized_escape_lanes() {
        for phase in 1..=3 {
            for kind in [BossKind::Lotus, BossKind::Loom] {
                let mut game = encounter(kind, phase);
                let b = game.boss.as_mut().unwrap();
                b.windup = 0.0;
                let origin = b.pos;
                let gap = if kind == BossKind::Lotus {
                    TAU / (4 + phase) as f32 / 2.0
                } else {
                    0.0
                };
                let lane = V2::from_angle(b.emit_angle + gap);
                game.update_boss(0.0);
                assert!(game.orbs.len() >= 20);
                for distance in [150.0, KEEP_AWAY, 600.0] {
                    let escape = origin + lane * distance;
                    assert!(
                        game.orbs.iter().all(|orb| {
                            segment_hit(
                                escape,
                                orb.pos,
                                orb.pos + orb.vel * 6.0,
                                PLAYER_RADIUS + orb.radius(),
                            )
                            .is_none()
                        }),
                        "{kind:?} phase {phase}: blocked lane at {distance}"
                    );
                }
            }
        }
    }

    #[test]
    fn boss_shots_cannot_materialize_on_the_player_and_respect_the_orb_cap() {
        for pattern in [
            Pattern::Ring,
            Pattern::Spiral,
            Pattern::Fan,
            Pattern::Helix,
            Pattern::Petals,
            Pattern::Curtain,
            Pattern::Crossfire,
        ] {
            let mut game = encounter(BossKind::Prism, 3);
            let b = game.boss.as_mut().unwrap();
            b.pattern = pattern;
            b.windup = 0.0;
            game.player.pos = b.pos + v2(BOSS_RADIUS, 0.0);
            game.update_boss(0.0);
            assert!(
                game.orbs.iter().all(|orb| {
                    (orb.pos - game.player.pos).len()
                        > PLAYER_RADIUS + orb.radius() + orb.vel.len() * 0.25
                }),
                "{pattern:?}"
            );
            while game.orbs.len() < MAX_ORBS - 1 {
                game.add_orb(v2(100.0, 100.0), v2(1.0, 0.0));
            }
            game.player.pos = v2(2000.0, 1000.0);
            game.update_boss(0.5);
            assert!(game.orbs.len() <= MAX_ORBS, "{pattern:?}");
        }
    }

    #[test]
    fn every_boss_has_a_safe_entrance_and_clears_projectiles_and_splitting_minions_on_death() {
        let mut game = Game::new(7);
        for level in 1..=5 {
            game.wave = level * 5;
            game.spawn_boss();
            game.update_boss(ENTER_TIME + 0.1);
            assert!(game.orbs.is_empty() && game.spawns.is_empty());
            let at = game.boss.as_ref().unwrap().pos;
            game.add_enemy(Kind::Tank, at + v2(200.0, 0.0), V2::default(), false);
            game.add_enemy(Kind::Splitter, at - v2(200.0, 0.0), V2::default(), false);
            game.add_orb(at, v2(100.0, 0.0));
            game.shockwaves.push(Shockwave {
                pos: game.player.pos - v2(100.0, 0.0),
                r: 100.0,
                hit: false,
            });
            let player_hp = game.player.hp;
            game.boss.as_mut().unwrap().hp = 0.0;
            game.update_boss(0.0);
            assert!(game.boss.is_none());
            assert!(game.enemies.is_empty() && game.spawns.is_empty() && game.orbs.is_empty());
            assert!(game.shockwaves.is_empty());
            game.player.invuln = 0.0;
            game.player_collisions();
            assert_eq!(game.player.hp, player_hp, "the arena is safe after victory");
        }
    }
}
