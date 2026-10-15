//! Boss powers fire automatically alongside the selected weapon.

use super::*;

impl Game {
    pub(super) fn update_trophies(&mut self, dt: f32) {
        if self.state != State::Playing
            || self.banner > 0.0
            || self.wave_clear > 0.0
            || self.boss_pending > 0.0
            || self.boss.as_ref().is_some_and(|b| b.entering > 0.0)
        {
            return;
        }
        self.trophy_time += dt;
        for (i, trophy) in Upgrade::TROPHIES.into_iter().enumerate() {
            if self.player.upgrades.level(trophy) == 0 {
                continue;
            }
            self.trophy_cd[i] -= dt;
            if self.trophy_cd[i] > 0.0 {
                continue;
            }
            let (period, angles): (f32, Vec<f32>) = match trophy {
                Upgrade::PrismCore => (3.0, (0..8).map(|j| j as f32 * TAU / 8.0).collect()),
                Upgrade::HelixCoil => (
                    0.3,
                    vec![self.trophy_time * 3.0, -self.trophy_time * 3.0 + TAU / 2.0],
                ),
                Upgrade::LotusBloom => (
                    2.0,
                    (-2..=2)
                        .map(|j| self.player.aim + j as f32 * 0.18)
                        .collect(),
                ),
                Upgrade::EclipseCross => (
                    1.5,
                    (0..4)
                        .map(|j| self.trophy_time * 0.4 + j as f32 * TAU / 4.0)
                        .collect(),
                ),
                Upgrade::LoomVeil => {
                    let at = self.player.pos;
                    self.orbs.retain(|o| (o.pos - at).len() > 140.0);
                    self.rings.push(Ring {
                        pos: at,
                        t: 0.0,
                        dur: 0.5,
                        hue: Hue::Cyan,
                        inward: false,
                    });
                    self.trophy_cd[i] += 4.0;
                    continue;
                }
                _ => unreachable!("only trophies"),
            };
            self.trophy_cd[i] += period;
            for angle in angles {
                // Reserve placement headroom for the player's weapon and enemy bullets.
                if self.bullets.len() >= 256 {
                    break;
                }
                let dir = V2::from_angle(angle);
                let pos = self.player.pos + dir * 22.0;
                self.bullets.push(Bullet {
                    pos,
                    prev: pos,
                    delay: 0.0,
                    vel: dir * BULLET_SPEED,
                    traveled: 0.0,
                    range: BULLET_RANGE,
                    damage: 1.2,
                    pierce: 0,
                    hit_ids: Vec::new(),
                    exits: Vec::new(),
                    falloff: false,
                    weapon: Weapon::Default,
                    homing: false,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defeated_boss_offers_only_its_unowned_trophy_alongside_regular_upgrades() {
        for (i, kind) in BossKind::ALL.into_iter().enumerate() {
            let mut game = Game::new(42);
            game.state = State::Playing;
            game.start_wave((i as u32 + 1) * 5);
            game.spawn_boss();
            let boss = game.boss.take().unwrap();
            game.boss_defeated(boss);
            game.boss_pending = 0.0;
            game.update_waves(0.0);
            game.update_waves(2.0);
            assert_eq!(game.state, State::Upgrade);
            assert_eq!(game.offer.len(), 3);
            assert_eq!(game.offer[0], kind.trophy());
            assert!(
                game.offer[1..]
                    .iter()
                    .all(|u| !Upgrade::TROPHIES.contains(u))
            );
            game.take_upgrade(kind.trophy());
            assert_eq!(game.player.upgrades.level(kind.trophy()), 1);
            assert_eq!(game.trophy_offer, None);
            assert_eq!(game.state, State::Playing);
            game.trophy_offer = Some(kind);
            assert!(!game.roll_offer().contains(&kind.trophy()));
            game.start();
            assert_eq!(game.player.upgrades.level(kind.trophy()), 0);
            assert_eq!(game.trophy_offer, None);
        }
        let mut game = Game::new(42);
        assert!(
            game.roll_offer()
                .iter()
                .all(|u| !Upgrade::TROPHIES.contains(u))
        );
        for u in game.player.upgrades.available() {
            for _ in 0..u.max_level() {
                game.player.upgrades.add(u);
            }
        }
        game.trophy_offer = Some(BossKind::Prism);
        assert_eq!(game.roll_offer(), vec![Upgrade::PrismCore]);
        game.player.upgrades.add(Upgrade::PrismCore);
        assert!(game.roll_offer().is_empty());
    }

    #[test]
    fn trophy_patterns_keep_their_cadence_and_bound_projectiles() {
        for (trophy, count) in [
            (Upgrade::PrismCore, 16),
            (Upgrade::HelixCoil, 40),
            (Upgrade::LotusBloom, 15),
            (Upgrade::EclipseCross, 16),
        ] {
            for fps in [30, 60, 144] {
                let mut game = Game::new(42);
                game.state = State::Playing;
                game.player.pos = game.arena_size * 0.5;
                game.player.upgrades.add(trophy);
                for _ in 0..fps * 6 - 1 {
                    game.update_trophies(1.0 / fps as f32);
                }
                assert_eq!(game.bullets.len(), count, "{trophy:?}, {fps} FPS");
                assert!(
                    game.bullets
                        .iter()
                        .all(|b| b.vel.len() > 0.0 && !b.falloff && b.weapon == Weapon::Default)
                );
                game.state = State::Paused;
                let cooldown = game.trophy_cd;
                game.update_trophies(3.0);
                assert_eq!(game.trophy_cd, cooldown);
                assert_eq!(game.bullets.len(), count);
                game.state = State::Playing;
                for _ in 0..1000 {
                    game.update_trophies(0.1);
                }
                assert_eq!(game.bullets.len(), 256);
                game.update_bullets(10.0);
                assert!(game.bullets.is_empty());
            }
        }
    }

    #[test]
    fn loom_veil_clears_only_nearby_bullets_and_waits_between_pulses() {
        let mut game = Game::new(42);
        game.state = State::Playing;
        game.player.pos = game.arena_size * 0.5;
        game.player.upgrades.add(Upgrade::LoomVeil);
        let at = game.player.pos;
        game.add_orb(at + v2(139.0, 0.0), V2::default());
        game.add_orb(at + v2(141.0, 0.0), V2::default());
        game.update_trophies(0.0);
        assert_eq!(game.orbs.len(), 1);
        assert_eq!(game.orbs[0].pos, at + v2(141.0, 0.0));
        game.add_orb(at, V2::default());
        game.update_trophies(3.9);
        assert_eq!(game.orbs.len(), 2);
        game.update_trophies(0.11);
        assert_eq!(game.orbs.len(), 1);
        assert!(game.bullets.is_empty());
    }
}
