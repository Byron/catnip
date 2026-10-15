//! Wave flow: what spawns, when, and what happens when a wave is cleared.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveModifier {
    Calm,
    Reinforcements,
    Volatile,
    Crossfire,
}

impl WaveModifier {
    pub(super) const ALL: [Self; 4] = [
        Self::Calm,
        Self::Reinforcements,
        Self::Volatile,
        Self::Crossfire,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Calm => "",
            Self::Reinforcements => "REINFORCEMENTS",
            Self::Volatile => "VOLATILE WRECKS",
            Self::Crossfire => "CROSSFIRE",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Self::Calm => "",
            Self::Reinforcements => "Three groups arrive together. Watch the spawn rings.",
            Self::Volatile => "Wrecks explode after a warning. Dash through the shockwave.",
            Self::Crossfire => "Edge volleys alternate sides. Follow the gap or shoot through.",
        }
    }
}

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

pub(super) const LATE_ROSTERS: [&[Kind]; 6] = [
    &[Kind::Sentinel, Kind::Sniper],
    &[Kind::Bomber, Kind::Spinner, Kind::Skirmisher],
    &[Kind::Splitter, Kind::Swarm, Kind::Shooter],
    &[Kind::Tank, Kind::Chaser, Kind::Sniper],
    &[Kind::Sentinel, Kind::Skirmisher, Kind::Swarm],
    &[Kind::Chaser, Kind::Shooter, Kind::Spinner],
];

impl Game {
    pub(super) fn start_wave(&mut self, wave: u32) {
        if self.wave_modifier == WaveModifier::Volatile {
            self.shockwaves.clear();
        }
        self.wave = wave;
        self.modifier_time = 0.0;
        self.wave_modifier = if wave <= 5 || wave.is_multiple_of(5) {
            WaveModifier::Calm
        } else {
            let index = wave - wave / 5 - 5;
            self.modifiers[index as usize % self.modifiers.len()]
        };
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
        if self.state != State::Playing {
            return;
        }
        self.banner -= dt;
        if self.boss_pending > 0.0 {
            self.boss_pending -= dt;
            if self.boss_pending <= 0.0 {
                self.spawn_boss();
            }
        }
        if self.wave_clear > 0.0 {
            self.wave_clear -= dt;
            if self.wave_clear <= 0.0 {
                let offer = if self.wave.is_multiple_of(5) {
                    self.roll_offer()
                } else {
                    Vec::new()
                };
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
            let introduced = if self.wave_modifier == WaveModifier::Calm {
                self.banner < 1.3
            } else {
                self.banner <= 0.0
            };
            if self.spawn_cd <= 0.0 && introduced {
                let cadence = (0.95 - 0.05 * self.wave as f32).max(0.25);
                let count = if self.wave_modifier == WaveModifier::Reinforcements {
                    3
                } else {
                    1
                };
                self.spawn_cd = cadence * count as f32;
                for _ in 0..count {
                    if self.budget > 0 {
                        self.spawn_group();
                    }
                }
            }
        } else if self.spawns.is_empty()
            && self.enemies.is_empty()
            && self.boss.is_none()
            && self.boss_pending <= 0.0
            && (self.wave_modifier != WaveModifier::Volatile || self.shockwaves.is_empty())
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

    pub(super) fn update_modifier(&mut self, dt: f32) {
        if self.state != State::Playing
            || self.banner > 0.0
            || self.wave_clear > 0.0
            || self.wave_modifier != WaveModifier::Crossfire
            || (self.budget <= 0 && self.spawns.is_empty() && self.enemies.is_empty())
        {
            return;
        }
        let before = self.modifier_time;
        self.modifier_time += dt;
        let cycle = (self.modifier_time / 4.0) as u32;
        if before < cycle as f32 * 4.0 + 1.0 && self.modifier_time >= cycle as f32 * 4.0 + 1.0 {
            let horizontal = cycle.is_multiple_of(2);
            let vel = if horizontal {
                v2(0.0, VOLLEY_SPEED)
            } else {
                v2(-VOLLEY_SPEED, 0.0)
            };
            for pos in self.crossfire_sources() {
                self.add_orb(pos, vel);
            }
            self.sfx.push(Sfx::EnemyShoot);
        }
    }

    pub fn crossfire_sources(&self) -> Vec<V2> {
        let cycle = (self.modifier_time / 4.0) as u32;
        let horizontal = cycle.is_multiple_of(2);
        let length = if horizontal {
            self.arena_size.x
        } else {
            self.arena_size.y
        };
        let gap = 3 + (cycle as usize * 3) % 10;
        (0usize..16)
            .filter(|&i| i.abs_diff(gap) > 1)
            .map(|i| {
                let at = WALL + 30.0 + (length - 2.0 * (WALL + 30.0)) * (i as f32 + 0.5) / 16.0;
                if horizontal {
                    v2(at, WALL + 30.0)
                } else {
                    v2(self.arena_size.x - WALL - 30.0, at)
                }
            })
            .collect()
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
        self.late_rosters[index as usize % self.late_rosters.len()]
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
    fn modifiers_are_seeded_repeatable_and_skip_opening_and_boss_waves() {
        assert_ne!(Game::new(1).modifiers, Game::new(2).modifiers);
        for seed in [1, 2, 42, u64::MAX] {
            let mut game = Game::new(seed);
            let mut other = Game::new(seed);
            for _ in 0..100 {
                other.rng.next();
            }
            for difficulty in Difficulty::ALL {
                game.difficulty = difficulty;
                other.difficulty = difficulty;
                let mut schedule = Vec::new();
                for wave in 1..=20 {
                    game.start_wave(wave);
                    other.start_wave(wave);
                    assert_eq!(game.wave_modifier, other.wave_modifier);
                    if wave <= 5 || wave.is_multiple_of(5) {
                        assert_eq!(game.wave_modifier, WaveModifier::Calm);
                    } else {
                        schedule.push(game.wave_modifier);
                    }
                }
                for cycle in schedule.as_chunks::<4>().0 {
                    for modifier in WaveModifier::ALL {
                        assert!(cycle.contains(&modifier));
                    }
                }
            }
            game.start();
            assert_eq!(game.modifiers, other.modifiers);
        }
    }

    #[test]
    fn reinforcements_wait_for_warning_then_spawn_three_budgeted_groups() {
        let mut game = Game::new(42);
        game.state = State::Playing;
        game.start_wave(12);
        game.wave_modifier = WaveModifier::Reinforcements;
        let budget = game.budget;
        game.update_waves(0.8);
        assert!(game.spawns.is_empty() && game.enemies.is_empty());
        assert_eq!(game.budget, budget);
        game.update_waves(1.3);
        let roster = game.wave_roster();
        let expected_cost: i32 = (0..3).map(|i| roster[i % roster.len()].spec().cost).sum();
        let expected_count: usize = (0..3).map(|i| roster[i % roster.len()].spec().group).sum();
        assert_eq!(game.budget, budget - expected_cost);
        assert_eq!(game.enemies.len() + game.spawns.len(), expected_count);
        assert!(game.enemies.iter().all(|e| roster.contains(&e.kind)));
        let remaining = game.budget;
        game.update_waves(0.01);
        assert_eq!(game.budget, remaining);
    }

    #[test]
    fn volatile_wrecks_warn_hurt_once_and_cannot_stall_the_wave() {
        let mut game = Game::new(42);
        game.state = State::Playing;
        game.start_wave(12);
        game.wave_modifier = WaveModifier::Volatile;
        game.banner = 0.0;
        game.budget = 0;
        let at = game.arena_size * 0.5;
        game.player.pos = at + v2(50.0, 0.0);
        game.add_enemy(Kind::Chaser, at, V2::default(), false);
        let enemy = game.enemies.pop().unwrap();
        game.kill(enemy, true);
        assert_eq!(game.shockwaves.len(), 1);
        assert!(game.shockwaves[0].r < 0.0);
        let hp = game.player.hp;
        game.player_collisions();
        assert_eq!(game.player.hp, hp);
        game.update_waves(0.0);
        assert_eq!(game.wave_clear, 0.0);
        game.shockwaves[0].r = 50.0;
        game.player.dash_t = 0.1;
        game.player_collisions();
        assert_eq!(game.player.hp, hp);
        assert!(!game.shockwaves[0].hit);
        game.player.dash_t = 0.0;
        game.player_collisions();
        assert_eq!(game.player.hp, hp - 1);
        game.player.invuln = 0.0;
        game.player_collisions();
        assert_eq!(game.player.hp, hp - 1);
        for _ in 0..120 {
            game.update_world(1.0 / 60.0);
        }
        assert!(game.shockwaves.is_empty());
        assert!(game.wave_clear > 0.0 || game.wave > 12);
        game.start_wave(13);
        game.wave_modifier = WaveModifier::Volatile;
        for _ in 0..20 {
            game.add_enemy(Kind::Chaser, at, V2::default(), false);
            let enemy = game.enemies.pop().unwrap();
            game.kill(enemy, true);
        }
        assert_eq!(game.shockwaves.len(), 8);
        game.start_wave(15);
        assert!(game.shockwaves.is_empty());
        assert_eq!(game.wave_modifier, WaveModifier::Calm);
        game.add_enemy(Kind::Chaser, at, V2::default(), false);
        let enemy = game.enemies.pop().unwrap();
        game.kill(enemy, false);
        assert!(game.shockwaves.is_empty());
    }

    #[test]
    fn crossfire_warns_alternates_sides_and_respects_pause_and_bullet_limits() {
        for fps in [30, 60, 144] {
            let mut game = Game::new(42);
            game.state = State::Playing;
            game.start_wave(12);
            game.wave_modifier = WaveModifier::Crossfire;
            game.update_modifier(2.0);
            assert_eq!(game.modifier_time, 0.0);
            game.banner = 0.0;
            let sources = game.crossfire_sources();
            assert_eq!(sources.len(), 13);
            assert!(sources.iter().all(|p| p.y == WALL + 30.0));
            for _ in 0..fps / 2 {
                game.update_modifier(1.0 / fps as f32);
            }
            assert!(game.orbs.is_empty());
            game.state = State::Paused;
            let time = game.modifier_time;
            game.update_modifier(5.0);
            assert_eq!(game.modifier_time, time);
            game.state = State::Playing;
            for _ in 0..fps * 5 {
                game.update_modifier(1.0 / fps as f32);
            }
            assert_eq!(game.orbs.len(), 26, "{fps} FPS");
            assert!(
                game.orbs[..13]
                    .iter()
                    .all(|o| o.vel.y > 0.0 && o.vel.x == 0.0)
            );
            assert!(
                game.orbs[13..]
                    .iter()
                    .all(|o| o.vel.x < 0.0 && o.vel.y == 0.0)
            );
            game.orbs.clear();
            for _ in 0..MAX_ORBS - 1 {
                game.add_orb(v2(100.0, 100.0), V2::default());
            }
            game.modifier_time = 0.9;
            game.update_modifier(0.2);
            assert_eq!(game.orbs.len(), MAX_ORBS);
            game.wave_clear = 1.0;
            let time = game.modifier_time;
            game.update_modifier(2.0);
            assert_eq!(game.modifier_time, time);
        }
    }

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
    fn seed_shuffles_all_late_themes_reproducibly() {
        assert_ne!(Game::new(1).late_rosters, Game::new(2).late_rosters);
        assert_ne!(Game::new(2).rng.0, Game::new(3).rng.0);
        for seed in [1, 2, 3, 7, u64::MAX] {
            let mut game = Game::new(seed);
            let mut other = Game::new(seed);
            for roster in LATE_ROSTERS {
                assert_eq!(
                    game.late_rosters.iter().filter(|r| **r == roster).count(),
                    1
                );
            }
            // Cosmetic and gameplay draws cannot change the selected wave order.
            for _ in 0..100 {
                other.rng.next();
            }
            for difficulty in Difficulty::ALL {
                game.difficulty = difficulty;
                other.difficulty = difficulty;
                for wave in 1..=30 {
                    game.start_wave(wave);
                    other.start_wave(wave);
                    assert_eq!(game.wave_roster(), other.wave_roster());
                    assert!(
                        game.wave_roster()
                            .iter()
                            .all(|kind| kind.spec().first_wave <= game.effective_wave())
                    );
                }
            }
        }
    }

    #[test]
    fn title_activity_and_retries_reuse_the_seed_and_spawn_sequence() {
        let view = View {
            scale: 1.0,
            size: v2(1600.0, 900.0),
        };
        let mut game = Game::new(42);
        let snapshot = |game: &Game| {
            game.spawns
                .iter()
                .map(|s| (s.kind, s.pos, s.elite))
                .collect::<Vec<_>>()
        };
        game.dev.first_wave = 12;
        game.update(1.0, &Input::default(), &view);
        game.start();
        let roster = game.wave_roster();
        game.spawn_group();
        let spawns = snapshot(&game);
        for _ in 0..2 {
            game.update(0.1, &Input::default(), &view);
            game.state = State::GameOver;
            game.update(
                0.0,
                &Input {
                    pressed: vec![KeyCode::Esc],
                    ..Input::default()
                },
                &view,
            );
            assert_eq!(game.state, State::Title);
            assert_eq!(game.seed, 42);
            game.update(1.0, &Input::default(), &view);
            game.start();
            assert_eq!(game.wave_roster(), roster);
            game.spawn_group();
            assert_eq!(snapshot(&game), spawns);
            game.start();
            game.spawn_group();
            assert_eq!(snapshot(&game), spawns);
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
                    assert_eq!(
                        game.wave_roster(),
                        game.late_rosters[index % LATE_ROSTERS.len()]
                    );
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
        game.late_rosters = LATE_ROSTERS;
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
