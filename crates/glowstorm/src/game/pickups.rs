//! Hearts and rapid fire dropped by enemies.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PickupKind {
    Heart,
    Rapid,
}

pub struct Pickup {
    pub kind: PickupKind,
    pub pos: V2,
    pub life: f32,
}

pub(super) const RAPID_TIME: f32 = 7.0;

impl Game {
    pub(super) fn update_pickups(&mut self, dt: f32) {
        let p = self.player.pos;
        let mut collected = Vec::new();
        self.pickups.retain_mut(|k| {
            k.life -= dt;
            let d = p - k.pos;
            if d.len() < 150.0 {
                k.pos += d.norm() * 420.0 * dt;
            }
            if d.len() < 30.0 {
                collected.push((k.kind, k.pos));
                return false;
            }
            k.life > 0.0
        });
        for (kind, pos) in collected {
            match kind {
                PickupKind::Heart => self.player.hp = (self.player.hp + 1).min(self.player.max_hp),
                PickupKind::Rapid => self.player.rapid_t = RAPID_TIME,
            }
            let hue = if kind == PickupKind::Heart {
                Hue::Red
            } else {
                Hue::Yellow
            };
            self.rings.push(Ring {
                pos,
                t: 0.0,
                dur: 0.35,
                hue,
                inward: false,
            });
            self.sfx.push(Sfx::Pickup);
        }
    }
}
