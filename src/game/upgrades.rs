//! Upgrade cards offered after each boss.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upgrade {
    Spread,
    Pierce,
    TwinDash,
    Coolant,
    Shield,
    Heart,
    Range,
}

impl Upgrade {
    pub const ALL: [Upgrade; 7] = [
        Upgrade::Spread,
        Upgrade::Pierce,
        Upgrade::TwinDash,
        Upgrade::Coolant,
        Upgrade::Shield,
        Upgrade::Heart,
        Upgrade::Range,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Upgrade::Spread => "SPREAD SHOT",
            Upgrade::Pierce => "PIERCING",
            Upgrade::TwinDash => "TWIN DASH",
            Upgrade::Coolant => "COOLANT",
            Upgrade::Shield => "SHIELD ORB",
            Upgrade::Heart => "EXTRA HEART",
            Upgrade::Range => "LONG RANGE",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Upgrade::Spread => "two more bullets per shot, at 60% damage",
            Upgrade::Pierce => "bullets pass through one more enemy",
            Upgrade::TwinDash => "hold two dash charges",
            Upgrade::Coolant => "25% less heat per shot",
            Upgrade::Shield => "an orb circles you, eats bullets, burns enemies",
            Upgrade::Heart => "+1 max heart and a full heal",
            Upgrade::Range => "+25% range and full-damage distance",
        }
    }

    pub fn max_level(self) -> u8 {
        match self {
            Upgrade::TwinDash => 1,
            _ => 2,
        }
    }

    fn index(self) -> usize {
        Upgrade::ALL
            .iter()
            .position(|&u| u == self)
            .expect("listed in ALL")
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Upgrades {
    levels: [u8; Upgrade::ALL.len()],
}

impl Upgrades {
    pub fn level(&self, upgrade: Upgrade) -> u8 {
        self.levels[upgrade.index()]
    }

    pub fn add(&mut self, upgrade: Upgrade) {
        let level = &mut self.levels[upgrade.index()];
        *level = (*level + 1).min(upgrade.max_level());
    }

    pub fn available(&self) -> Vec<Upgrade> {
        Upgrade::ALL
            .into_iter()
            .filter(|&u| self.level(u) < u.max_level())
            .collect()
    }
}

/// Horizontal extent of card `i` of `n`, in window pixels; shared by the
/// renderer and mouse hit-testing so they always agree.
pub fn card_span(i: usize, n: usize, window_width: f32) -> (f32, f32) {
    let card = window_width * 0.24;
    let gap = window_width * 0.03;
    let total = n as f32 * card + (n.saturating_sub(1)) as f32 * gap;
    let left = (window_width - total) / 2.0 + i as f32 * (card + gap);
    (left, left + card)
}
