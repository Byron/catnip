//! Difficulty levels, scoring and best scores.

use super::*;

/// Difficulty adds pressure, not health: enemy_hp stays 1.0 on every level,
/// so harder means faster, busier and more aggressive, never spongier.
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    pub hearts: i32,
    pub enemy_hp: f32,
    pub enemy_speed: f32,
    /// Divides attack cooldowns.
    pub attack_rate: f32,
    /// Multiplies each wave's spawn budget.
    pub enemy_count: f32,
    pub boss_hp: f32,
    /// Score multiplier.
    pub score: f32,
    /// Spawns behave as if this many waves later (earlier unlocks).
    pub head_start: u32,
}

/// Picked on the title screen. Harder settings pay more points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Medium,
    Hard,
    Extreme,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Medium, Difficulty::Hard, Difficulty::Extreme];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Medium => "MEDIUM",
            Difficulty::Hard => "HARD",
            Difficulty::Extreme => "EXTREME",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Difficulty::Medium => "5 hearts · the intended experience",
            Difficulty::Hard => "4 hearts · faster, more aggressive enemies · score ×1.5",
            Difficulty::Extreme => "3 hearts · relentless swarms of attackers · score ×2",
        }
    }

    /// The numbers behind each difficulty (see the table in the rulebook).
    pub(super) fn tuning(self) -> Tuning {
        let medium = Tuning {
            hearts: 5,
            enemy_hp: 1.0,
            enemy_speed: 1.0,
            attack_rate: 1.0,
            enemy_count: 1.0,
            boss_hp: 1.0,
            score: 1.0,
            head_start: 0,
        };
        match self {
            Difficulty::Medium => medium,
            Difficulty::Hard => Tuning {
                hearts: 4,
                enemy_speed: 1.1,
                attack_rate: 1.3,
                enemy_count: 1.25,
                boss_hp: 1.1,
                score: 1.5,
                head_start: 1,
                ..medium
            },
            Difficulty::Extreme => Tuning {
                hearts: 3,
                enemy_speed: 1.2,
                attack_rate: 1.6,
                enemy_count: 1.7,
                boss_hp: 1.25,
                score: 2.0,
                head_start: 2,
                ..medium
            },
        }
    }

    pub fn hearts(self) -> i32 {
        self.tuning().hearts
    }

    pub(super) fn file_suffix(self) -> &'static str {
        match self {
            Difficulty::Medium => "",
            Difficulty::Hard => "_hard",
            Difficulty::Extreme => "_extreme",
        }
    }
}

pub(super) fn best_score_path(difficulty: Difficulty) -> Option<PathBuf> {
    #[cfg(windows)]
    let platform_data = || {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("AppData/Local"))
            })
    };
    #[cfg(unix)]
    let platform_data = || std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"));
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(platform_data)?;
    Some(base.join(format!("glowstorm/best_score{}", difficulty.file_suffix())))
}

pub(super) fn load_best(path: Option<&PathBuf>) -> u64 {
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

impl Game {
    pub(super) fn set_difficulty(&mut self, difficulty: Difficulty) {
        self.difficulty = difficulty;
        self.player = Player::new(difficulty.hearts());
        if self.dev.spread_shot {
            self.player.upgrades.add(Upgrade::Spread);
        }
        if self.dev.rapid_fire {
            self.player.rapid_t = RAPID_TIME;
        }
        self.player.pos = self.arena_size * 0.5;
        self.best_path = best_score_path(difficulty);
        self.best = load_best(self.best_path.as_ref());
    }

    pub fn multiplier(&self) -> u64 {
        (1 + self.chain / 6).min(8) as u64
    }

    /// Points after the combo multiplier and difficulty bonus.
    pub(super) fn award(&mut self, base: u64) -> u64 {
        let points = (base as f32 * self.multiplier() as f32 * self.difficulty.tuning().score)
            .round() as u64;
        self.score += points;
        points
    }
}
