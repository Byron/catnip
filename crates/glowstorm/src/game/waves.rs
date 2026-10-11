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
            self.budget = ((6 + 4 * self.effective_wave()) as f32
                * self.difficulty.tuning().enemy_count)
                .round() as i32;
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
                    self.state_t = 0.0;
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
            let bonus = (100.0 * self.wave as f32 * self.difficulty.tuning().score).round() as u64;
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

    /// Hard and Extreme start where Medium would be a wave or two in, so
    /// their opening waves aren't a quiet warm-up.
    pub(super) fn effective_wave(&self) -> u32 {
        self.wave + self.difficulty.tuning().head_start
    }

    pub(super) fn spawn_group(&mut self) {
        let wave = self.effective_wave();
        let options: Vec<Kind> = Kind::ALL
            .into_iter()
            .filter(|k| k.spec().first_wave <= wave && k.spec().cost <= self.budget)
            .collect();
        let weight = |k: &Kind| k.spec().weight;
        let total: f32 = options.iter().map(weight).sum();
        let mut roll = self.rng.unit() * total;
        let mut kind = options[0];
        for k in &options {
            kind = *k;
            roll -= weight(k);
            if roll < 0.0 {
                break;
            }
        }
        let spec = kind.spec();
        self.budget -= spec.cost;

        // Somewhere inside the arena, not on top of the player.
        let margin = 90.0;
        let mut pos = V2::default();
        for _ in 0..24 {
            pos = v2(
                self.rng.range(margin, self.arena_size.x - margin),
                self.rng.range(margin, self.arena_size.y - margin),
            );
            if (pos - self.player.pos).len() > 460.0 {
                break;
            }
        }
        let count = spec.group;
        // Elites from wave 6: rare at first, up to 15%.
        let elite_chance = if self.wave >= 6 {
            (0.06 + 0.01 * (self.wave - 6) as f32).min(0.15)
        } else {
            0.0
        };
        let elite = spec.can_be_elite && self.rng.chance(elite_chance);
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
