//! Wave flow: what spawns, when, and what happens when a wave is cleared.

use super::*;

pub struct Spawn {
    pub(super) kind: Kind,
    pub(super) pos: V2,
    pub(super) t: f32,
    pub(super) elite: bool,
}

pub(super) const SPAWN_TIME: f32 = 0.9;

impl Game {
    pub(super) fn start_wave(&mut self, wave: u32) {
        self.wave = wave;
        self.spawn_cd = 0.0;
        self.enrage_announced = false;
        if wave.is_multiple_of(5) {
            // Boss wave: no regular spawns, the boss brings its own minions.
            self.budget = 0;
            self.banner = 0.0;
            self.boss_pending = 2.6;
            self.sfx.push(Sfx::Siren);
        } else {
            self.budget = ((6 + 4 * wave) as f32 * self.difficulty.tuning().4).round() as i32;
            self.banner = 2.0;
            self.sfx.push(Sfx::Wave);
        }
    }

    pub(super) fn update_waves(&mut self, dt: f32) {
        self.banner -= dt;
        if self.state != State::Playing {
            return;
        }
        if self.boss_pending > 0.0 {
            self.boss_pending -= dt;
            if self.boss_pending <= 0.0 {
                self.spawn_boss();
            }
        }
        if self.wave_clear > 0.0 {
            self.wave_clear -= dt;
            if self.wave_clear <= 0.0 {
                let offer = self.roll_offer();
                if self.wave.is_multiple_of(5) && !offer.is_empty() {
                    self.offer = offer;
                    self.offer_pick = 0;
                    self.state = State::Upgrade;
                    self.sfx.push(Sfx::LevelUp);
                } else {
                    self.start_wave(self.wave + 1);
                }
            }
        } else if self.budget > 0 {
            self.spawn_cd -= dt;
            if self.spawn_cd <= 0.0 && self.banner < 1.3 {
                self.spawn_cd = (0.95 - 0.05 * self.wave as f32).max(0.25);
                self.spawn_group();
            }
        } else if self.spawns.is_empty()
            && self.enemies.is_empty()
            && self.boss.is_none()
            && self.boss_pending <= 0.0
        {
            self.wave_clear = 1.8;
            // The wave bonus scales with difficulty but not with the combo.
            let bonus = (100.0 * self.wave as f32 * self.difficulty.tuning().6).round() as u64;
            self.score += bonus;
            self.slow(0.6, 0.35);
            self.flash = 0.08;
            self.sfx.push(Sfx::WaveClear);
            self.popup(self.player.pos + v2(0.0, -46.0), format!("+{bonus}"), true);
            self.popup(
                self.player.pos + v2(0.0, 46.0),
                "POWER UP".to_string(),
                true,
            );
        }

        let mut ready = Vec::new();
        self.spawns.retain_mut(|s| {
            s.t -= dt;
            if s.t <= 0.0 {
                ready.push((s.kind, s.pos, s.elite));
                false
            } else {
                true
            }
        });
        for (kind, pos, elite) in ready {
            self.add_enemy(kind, pos, V2::default(), elite);
            for _ in 0..6 {
                let v = self.rng.dir() * self.rng.range(80.0, 220.0);
                self.emit(pos, v, 0.35, kind.hue(), 1);
            }
        }
    }

    pub(super) fn spawn_group(&mut self) {
        let mut options = vec![Kind::Chaser];
        if self.wave >= 2 {
            options.push(Kind::Swarm);
        }
        if self.wave >= 3 {
            options.push(Kind::Shooter);
        }
        if self.wave >= 4 {
            options.push(Kind::Tank);
        }
        options.retain(|k| k.cost() <= self.budget);
        let kind = options[(self.rng.next() as usize) % options.len()];
        self.budget -= kind.cost();

        // Somewhere inside the arena, not on top of the player.
        let margin = 90.0;
        let mut pos = V2::default();
        for _ in 0..24 {
            pos = v2(
                self.rng.range(margin, WORLD_W - margin),
                self.rng.range(margin, WORLD_H - margin),
            );
            if (pos - self.player.pos).len() > 460.0 {
                break;
            }
        }
        let count = if kind == Kind::Swarm { 5 } else { 1 };
        // Elites from wave 6: rare at first, up to 15%.
        let elite_chance = if self.wave >= 6 {
            (0.06 + 0.01 * (self.wave - 6) as f32).min(0.15)
        } else {
            0.0
        };
        let elite = kind != Kind::Swarm && self.rng.chance(elite_chance);
        for i in 0..count {
            let offset = if count > 1 {
                V2::from_angle(i as f32 * TAU / count as f32) * 34.0
            } else {
                V2::default()
            };
            let at = pos + offset;
            self.spawns.push(Spawn {
                kind,
                pos: at,
                t: SPAWN_TIME,
                elite,
            });
            self.rings.push(Ring {
                pos: at,
                t: 0.0,
                dur: SPAWN_TIME,
                hue: if elite { Hue::Yellow } else { kind.hue() },
                inward: true,
            });
        }
    }
}
