//! Upgrade cards offered after each boss.

use super::*;

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

impl Game {
    pub(super) fn roll_offer(&mut self) -> Vec<Upgrade> {
        let mut pool = self.player.upgrades.available();
        let mut offer = Vec::new();
        while offer.len() < 3 && !pool.is_empty() {
            let i = (self.rng.next() as usize) % pool.len();
            offer.push(pool.swap_remove(i));
        }
        offer
    }

    pub(super) fn update_offer(&mut self, input: &Input, view: &View) {
        // The boss usually dies mid-fire or mid-dash; don't let that same
        // click or Space pick a card blind.
        const GRACE: f32 = 0.6;
        let n = self.offer.len();
        let width = view.size.x * view.scale;
        let hovered = input.mouse.and_then(|m| {
            (0..n).find(|&i| {
                let (left, right) = card_span(i, n, width);
                m.x >= left && m.x < right
            })
        });
        if input.mouse_moved
            && let Some(i) = hovered
        {
            self.offer_pick = i;
        }
        if input.hit(&[KeyCode::Left, KeyCode::Char('a')]) {
            self.offer_pick = (self.offer_pick + n - 1) % n;
        }
        if input.hit(&[KeyCode::Right, KeyCode::Char('d')]) {
            self.offer_pick = (self.offer_pick + 1) % n;
        }
        let mut take = input.hit(&[KeyCode::Enter, KeyCode::Char(' ')]);
        for (i, key) in ['1', '2', '3'].into_iter().enumerate().take(n) {
            if input.hit(&[KeyCode::Char(key)]) {
                self.offer_pick = i;
                take = true;
            }
        }
        if input.clicked && hovered.is_some() {
            take = true;
        }
        if take && self.state_t >= GRACE {
            let upgrade = self.offer[self.offer_pick];
            self.take_upgrade(upgrade);
        }
    }

    pub(super) fn take_upgrade(&mut self, upgrade: Upgrade) {
        let p = &mut self.player;
        p.upgrades.add(upgrade);
        match upgrade {
            Upgrade::Heart => {
                p.max_hp += 1;
                p.hp = p.max_hp;
            }
            Upgrade::TwinDash => p.dash_charges = p.max_dash_charges(),
            _ => {}
        }
        let at = p.pos;
        self.offer.clear();
        self.flash = 0.1;
        self.sfx.push(Sfx::Victory);
        self.rings.push(Ring {
            pos: at,
            t: 0.0,
            dur: 0.6,
            hue: Hue::Yellow,
            inward: false,
        });
        self.popup(at + v2(0.0, -50.0), upgrade.name().to_string(), true);
        self.state = State::Playing;
        self.start_wave(self.wave + 1);
    }
}
