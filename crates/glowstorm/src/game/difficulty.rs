//! Difficulty levels, scoring and best scores.

use super::*;

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

    /// (hearts, enemy hp, enemy speed, attack rate, enemies per wave, boss hp, score)
    pub(super) fn tuning(self) -> (i32, f32, f32, f32, f32, f32, f32) {
        match self {
            Difficulty::Medium => (5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0),
            Difficulty::Hard => (4, 1.15, 1.1, 1.3, 1.25, 1.3, 1.5),
            Difficulty::Extreme => (3, 1.3, 1.2, 1.6, 1.7, 1.7, 2.0),
        }
    }

    pub fn hearts(self) -> i32 {
        self.tuning().0
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
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
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
        self.best_path = best_score_path(difficulty);
        self.best = load_best(self.best_path.as_ref());
    }

    pub fn multiplier(&self) -> u64 {
        (1 + self.chain / 6).min(8) as u64
    }

    /// Points after the combo multiplier and difficulty bonus.
    pub(super) fn award(&mut self, base: u64) -> u64 {
        let points =
            (base as f32 * self.multiplier() as f32 * self.difficulty.tuning().6).round() as u64;
        self.score += points;
        points
    }
}
