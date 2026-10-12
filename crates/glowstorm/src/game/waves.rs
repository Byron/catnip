//! Wave flow: what spawns, when, and what happens when a wave is cleared.

use super::*;

pub struct Spawn {
    pub(super) kind: Kind,
    pub(super) pos: V2,
    pub(super) t: f32,
    pub(super) elite: bool,
}

pub(super) const SPAWN_TIME: f32 = 0.9;

// Each new type gets a focused introduction; death-spawned kinds count in its roster.
const INTRO_ROSTERS: [&[Kind]; 9] = [
    &[Kind::Chaser],
    &[Kind::Swarm, Kind::Chaser],
    &[Kind::Shooter, Kind::Swarm],
    &[Kind::Tank, Kind::Chaser, Kind::Shooter],
    &[Kind::Skirmisher, Kind::Swarm],
    &[Kind::Sniper, Kind::Chaser],
    &[Kind::Spinner, Kind::Shooter],
    &[Kind::Bomber, Kind::Swarm],
    &[Kind::Splitter, Kind::Swarm, Kind::Skirmisher],
];

const LATE_ROSTERS: [&[Kind]; 6] = [
    &[Kind::Sentinel, Kind::Sniper],
    &[Kind::Bomber, Kind::Spinner, Kind::Skirmisher],
    &[Kind::Splitter, Kind::Swarm, Kind::Shooter],
    &[Kind::Tank, Kind::Chaser, Kind::Sniper],
    &[Kind::Sentinel, Kind::Skirmisher, Kind::Swarm],
    &[Kind::Chaser, Kind::Shooter, Kind::Spinner],
];

impl Game {
    pub(super) fn start_wave(&mut self, wave: u32) {
        self.wave = wave;
        self.spawn_cd = 0.0;
        self.wave_spawn_index = 0;
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

    pub(super) fn wave_roster(&self) -> &'static [Kind] {
        if self.wave.is_multiple_of(5) {
            return &[];
        }
        let wave = self.effective_wave();
        let final_unlock = Kind::Sentinel.spec().first_wave;
        if wave < final_unlock {
            return INTRO_ROSTERS
                .into_iter()
                .rev()
                .find(|roster| roster.iter().all(|kind| kind.spec().first_wave <= wave))
                .unwrap_or(&[]);
        }
        // Boss waves do not consume a theme, even when difficulty advances the unlocks.
        let first = final_unlock
            .saturating_sub(self.difficulty.tuning().head_start)
            .max(1);
        let before = (first - 1) - (first - 1) / 5;
        let index = self.wave - self.wave / 5 - before - 1;
        LATE_ROSTERS[index as usize % LATE_ROSTERS.len()]
    }

    pub(super) fn spawn_group(&mut self) {
        let roster = self.wave_roster();
        let Some((offset, kind)) = (0..roster.len()).find_map(|offset| {
            let kind = roster[(self.wave_spawn_index + offset) % roster.len()];
            (kind.spec().cost <= self.budget).then_some((offset, kind))
        }) else {
            // An unspendable remainder ends the wave without adding an unrelated filler.
            self.budget = 0;
            return;
        };
        self.wave_spawn_index = (self.wave_spawn_index + offset + 1) % roster.len();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rosters_include_death_spawns_without_exceeding_three_types() {
        for roster in INTRO_ROSTERS.into_iter().chain(LATE_ROSTERS) {
            assert!(!roster.is_empty() && roster.len() <= 3);
            for kind in roster {
                if let Some((child, _)) = kind.spec().splits_into {
                    assert!(roster.contains(&child), "{kind:?} introduces {child:?}");
                }
            }
        }
    }

    #[test]
    fn late_themes_rotate_through_ordinary_waves_on_every_difficulty() {
        let mut game = Game::new(7);
        for difficulty in Difficulty::ALL {
            game.difficulty = difficulty;
            let mut index = 0;
            for wave in 1..=30 {
                game.start_wave(wave);
                if wave.is_multiple_of(5) {
                    assert!(game.wave_roster().is_empty());
                    assert_eq!(game.budget, 0);
                    assert!(game.boss_pending > 0.0);
                } else if game.effective_wave() >= Kind::Sentinel.spec().first_wave {
                    assert_eq!(game.wave_roster(), LATE_ROSTERS[index % LATE_ROSTERS.len()]);
                    index += 1;
                }
            }
            assert!(index >= LATE_ROSTERS.len() * 2);
        }
    }

    #[test]
    fn spawn_groups_cycle_through_the_roster_and_reset_with_the_wave() {
        let mut game = Game::new(7);
        for _ in 0..2 {
            game.start_wave(14);
            let roster = game.wave_roster();
            for index in 0..roster.len() * 2 {
                let before = game.spawns.len();
                game.spawn_group();
                let kind = roster[index % roster.len()];
                let group = &game.spawns[before..];
                assert_eq!(group.len(), kind.spec().group);
                assert!(group.iter().all(|spawn| spawn.kind == kind));
            }
            game.spawn_group();
        }
    }

    #[test]
    fn leftover_budget_only_buys_affordable_roster_members_or_ends_spawning() {
        let mut game = Game::new(7);
        game.state = State::Playing;
        game.start_wave(13);
        assert_eq!(game.wave_roster(), LATE_ROSTERS[1]);
        game.budget = Kind::Skirmisher.spec().cost;
        game.spawn_group();
        assert_eq!(game.budget, 0);
        assert_eq!(game.spawns.len(), 1);
        assert_eq!(game.spawns[0].kind, Kind::Skirmisher);

        game.spawns.clear();
        game.start_wave(12);
        game.banner = 0.0;
        game.budget = 1;
        game.update_waves(0.0);
        assert_eq!(game.budget, 0);
        assert!(game.spawns.is_empty() && game.enemies.is_empty());
        game.update_waves(0.0);
        assert!(
            game.wave_clear > 0.0,
            "unspendable leftovers cannot stall the wave"
        );
    }
}
