//! What the player pressed this frame, and the visible part of the world.

use std::collections::HashSet;

use super::*;

#[derive(Default)]
pub struct Input {
    pub held: HashSet<KeyCode>,
    pub pressed: Vec<KeyCode>,
    /// Mouse position in window pixels, once the mouse has moved.
    pub mouse: Option<V2>,
    pub fire: bool,
    pub charge: bool,
    pub middle_clicked: bool,
    pub clicked: bool,
    pub dash: bool,
    pub spin: bool,
    pub boost: bool,
    /// Wheel steps this frame: down selects the next weapon, up the previous.
    pub weapon_scroll: i32,
    /// The mouse moved this frame (hover only follows real movement).
    pub mouse_moved: bool,
}

impl Input {
    pub(super) fn down(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.held.contains(k))
    }

    pub(super) fn hit(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.pressed.contains(k))
    }

    pub fn end_frame(&mut self) {
        self.pressed.clear();
        self.clicked = false;
        self.middle_clicked = false;
        self.dash = false;
        self.mouse_moved = false;
        self.weapon_scroll = 0;
    }
}

pub struct View {
    pub scale: f32,
    pub size: V2,
}

impl View {
    pub fn new(win: catnip::term::Window, scale: f32) -> Self {
        Self {
            scale,
            size: v2(win.width as f32 / scale, win.height as f32 / scale),
        }
    }

    pub fn arena_size(&self) -> V2 {
        v2(self.size.x.max(WORLD_W), self.size.y.max(WORLD_H))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: View = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };

    #[test]
    fn wheel_cycles_the_full_weapon_rack_and_number_keys_take_precedence() {
        let mut game = Game::new(7);
        game.state = State::Playing;
        for (index, weapon) in Weapon::ALL.into_iter().enumerate() {
            for (steps, expected) in [
                (1, (index + 1) % 10),
                (-1, (index + 9) % 10),
                (23, (index + 3) % 10),
                (-23, (index + 7) % 10),
            ] {
                game.player.weapon = weapon;
                game.update(
                    0.0,
                    &Input {
                        weapon_scroll: steps,
                        ..Input::default()
                    },
                    &VIEW,
                );
                assert_eq!(game.player.weapon, Weapon::ALL[expected]);
            }
        }
        for (key, weapon) in ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0']
            .into_iter()
            .zip(Weapon::ALL)
        {
            game.update(
                0.0,
                &Input {
                    pressed: vec![KeyCode::Char(key)],
                    weapon_scroll: -3,
                    ..Input::default()
                },
                &VIEW,
            );
            assert_eq!(game.player.weapon, weapon);
        }
        game.player.weapon = Weapon::Default;
        game.update(
            0.0,
            &Input {
                weapon_scroll: i32::MAX,
                ..Input::default()
            },
            &VIEW,
        );
        assert_eq!(game.player.weapon, Weapon::Blade);
    }

    #[test]
    fn switching_cancels_weapon_charge_but_preserves_heat_cooldown_and_spin_charge() {
        for select in [
            Input {
                weapon_scroll: 1,
                ..Input::default()
            },
            Input {
                pressed: vec![KeyCode::Char('2')],
                ..Input::default()
            },
        ] {
            let mut game = Game::new(7);
            game.state = State::Playing;
            game.player.heat = 0.5;
            game.player.fire_cd = 0.4;
            game.player.charge = 0.3;
            let mut input = Input {
                charge: true,
                fire: true,
                ..select
            };
            game.update(0.0, &input, &VIEW);
            assert_eq!(game.player.weapon, Weapon::Sniper);
            assert_eq!(
                (game.player.charge, game.player.heat, game.player.fire_cd),
                (0.0, 0.5, 0.4)
            );
            assert!(game.player.charge_blocked);
            assert!(game.gatling.is_none() && game.bullets.is_empty());
            input.end_frame();
            input.charge = false;
            game.update(0.0, &input, &VIEW);
            assert!(!game.player.charge_blocked && game.bullets.is_empty());
            game.update_player(0.4, &input, &VIEW);
            assert_eq!(
                game.bullets.len(),
                1,
                "held fire resumes when the shared cooldown expires"
            );
            assert_eq!(game.bullets[0].weapon, Weapon::Sniper);
        }

        let mut game = Game::new(7);
        game.state = State::Playing;
        game.player.charge_spin = true;
        game.player.charge = 0.3;
        game.update(
            0.0,
            &Input {
                weapon_scroll: 1,
                spin: true,
                ..Input::default()
            },
            &VIEW,
        );
        assert!(game.player.charge_spin && !game.player.charge_blocked);
        assert_eq!(
            game.player.charge, 0.3,
            "star-spin charging is independent of the weapon"
        );
        game.player.charge_spin = false;
        game.player.charge = 0.0;
        game.player.roll_hold = Some(0.1);
        game.update(
            0.0,
            &Input {
                weapon_scroll: 1,
                spin: true,
                ..Input::default()
            },
            &VIEW,
        );
        assert_eq!(game.player.roll_hold, Some(0.1));
    }

    #[test]
    fn menus_discard_wheel_input_before_play_resumes() {
        for state in [
            State::Title,
            State::Paused,
            State::Dying,
            State::GameOver,
            State::Upgrade,
        ] {
            let mut game = Game::new(7);
            game.state = state;
            let mut input = Input {
                weapon_scroll: 3,
                ..Input::default()
            };
            game.update(0.0, &input, &VIEW);
            assert_eq!(game.player.weapon, Weapon::Default, "{state:?}");
            input.end_frame();
            game.state = State::Playing;
            game.update(0.0, &input, &VIEW);
            assert_eq!(
                game.player.weapon,
                Weapon::Default,
                "no queued scroll from {state:?}"
            );
        }
    }
}
