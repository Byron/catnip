//! Game behavior tests: combat math, scaling, boss and upgrade flow.

use super::*;

#[test]
fn taps_roll_forward_and_holding_charges_a_forward_star_laser() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for keyboard in [false, true] {
        let mut game = Game::new(7);
        let start = game.player.pos;
        let press = Input {
            dash: !keyboard,
            spin: !keyboard,
            held: if keyboard {
                [KeyCode::Char(' '), KeyCode::Char('d')].into()
            } else {
                [KeyCode::Char('d')].into()
            },
            pressed: if keyboard {
                vec![KeyCode::Char(' ')]
            } else {
                vec![]
            },
            ..Input::default()
        };
        game.update_player(0.05, &press, &view);
        assert!(game.player.pos.x > start.x);
        assert!(game.player.dashing() && game.player.spin.is_none());
        game.update_player(0.05, &Input::default(), &view);
        assert!(game.player.spin.is_none());
        assert_eq!(game.player.charge, 0.0);
        assert_eq!(game.player.dash_charges, 0);

        let mut game = Game::new(7);
        game.update_player(0.01, &press, &view);
        let held = Input {
            spin: !keyboard,
            held: if keyboard {
                [KeyCode::Char(' ')].into()
            } else {
                Default::default()
            },
            ..Input::default()
        };
        for _ in 0..80 {
            game.update_player(0.01, &held, &view);
        }
        assert!(game.player.spin.is_none());
        let charge = game.player.charge;
        assert!(charge > 0.3 && charge < 0.4, "{charge}");
        game.update_player(0.01, &Input::default(), &view);
        let spin = game.player.spin.as_ref().unwrap();
        assert_eq!(spin.charge, charge);
        assert!(!spin.boosted);
        assert!((spin.duration - (0.35 + 1.25 * charge)).abs() < 1e-5);
        assert!(
            game.player.vel.x > 0.0,
            "offensive move follows evasion direction"
        );
        assert!(game.gatling.is_none());
    }
}

#[test]
fn star_charge_rewards_timing_penalizes_overheat_and_requires_release_to_rearm() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for forced in [false, true] {
        let mut game = Game::new(7);
        let held = Input {
            spin: true,
            ..Input::default()
        };
        game.update_player(
            0.01,
            &Input {
                dash: true,
                spin: true,
                ..Input::default()
            },
            &view,
        );
        for _ in 0..if forced { 210 } else { 185 } {
            game.update_player(0.01, &held, &view);
            if game.player.spin.is_some() {
                break;
            }
        }
        if !forced {
            game.update_player(0.01, &Input::default(), &view);
        }
        let spin = game.player.spin.as_ref().unwrap();
        assert_eq!(spin.boosted, !forced);
        let multiplier = if forced { 0.75 } else { 1.1 };
        assert!((spin.damage - (1.0 + 2.0 * spin.charge) * multiplier).abs() < 1e-5);
        assert_eq!(game.player.overheated, forced);
        if forced {
            for _ in 0..250 {
                game.update_player(0.01, &held, &view);
                game.update_spin(0.01);
            }
            assert!(game.player.spin.is_none());
            assert_eq!(game.player.charge, 0.0);
            assert!(!game.player.overheated);
            game.update_player(0.01, &Input::default(), &view);
            game.update_player(
                0.01,
                &Input {
                    dash: true,
                    spin: true,
                    ..Input::default()
                },
                &view,
            );
            assert!(game.player.dashing());
        }
    }
}

#[test]
fn star_charge_honors_shared_heat_coolant_and_overheat_lock() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let held = Input {
        spin: true,
        ..Input::default()
    };
    for coolant in [false, true] {
        let mut game = Game::new(7);
        game.player.charge_spin = true;
        if coolant {
            game.player.upgrades.add(Upgrade::Coolant);
        }
        for _ in 0..190 {
            game.update_player(0.01, &held, &view);
        }
        assert_eq!(game.player.spin.is_some(), !coolant);
        if coolant {
            assert!(game.player.heat < 0.8 && game.player.charge > 0.7);
        }
    }
    let mut game = Game::new(7);
    game.player.charge_spin = true;
    game.player.heat = 0.95;
    game.update_player(0.1, &held, &view);
    assert!(game.player.spin.is_some());
    assert!((game.player.spin.as_ref().unwrap().charge - 0.05).abs() < 1e-5);

    let mut game = Game::new(7);
    game.player.charge_spin = true;
    game.player.heat = 1.0;
    game.player.overheated = true;
    game.update_player(0.1, &held, &view);
    assert!(game.player.spin.is_none());
    assert_eq!(game.player.charge, 0.0);
}

#[test]
fn spinning_ship_is_immune_but_contact_damages_and_pushes_enemies_and_boss() {
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.player.vel = v2(-SPIN_SPEED, 0.0);
    game.player.spin = Some(Spin {
        life: 0.5,
        active: 0.02,
        fire_cd: 1.0,
        ..Spin::new(0.0, false, 1.0, game.player.pos, 0.0)
    });
    let at = game.player.pos + v2(20.0, 0.0);
    game.add_enemy(Kind::Tank, at, V2::default(), true);
    game.spawn_boss();
    let boss = game.boss.as_mut().unwrap();
    boss.pos = at;
    boss.entering = 0.0;
    let (hp, boss_hp, player_hp) = (game.enemies[0].hp, boss.hp, game.player.hp);
    game.update_spin(0.02);
    game.player_collisions();
    assert!(game.enemies[0].hp < hp - 3.0);
    assert!(game.enemies[0].impulse.x > 100.0);
    assert!(game.boss.as_ref().unwrap().hp < boss_hp - 3.0);
    assert!(game.boss.as_ref().unwrap().impulse.x > 100.0);
    assert_eq!(game.player.vel, v2(-SPIN_SPEED, 0.0));
    assert_eq!(game.player.hp, player_hp);
}

#[test]
fn spin_reflects_fast_bullets_as_friendly_shots_and_ignores_near_misses() {
    for heavy in [false, true] {
        let mut game = Game::new(7);
        game.state = State::Playing;
        let at = game.player.pos;
        game.player.spin = Some(Spin {
            life: 0.5,
            active: 0.1,
            fire_cd: 1.0,
            ..Spin::new(0.0, false, 1.0, at, 0.0)
        });
        game.add_orb(at + v2(100.0, 0.0), v2(-2000.0, 0.0));
        game.orbs[0].heavy = heavy;
        game.add_orb(at + v2(100.0, 1000.0), v2(-2000.0, 0.0));
        game.update_enemies(0.1);
        game.update_spin(0.1);
        assert_eq!(game.orbs.len(), 1);
        assert_eq!(game.bullets.len(), 1);
        assert_eq!(game.bullets[0].vel, v2(BULLET_SPEED, 0.0));
        assert_eq!(game.bullets[0].damage, if heavy { 6.0 } else { 3.0 });
        game.add_enemy(Kind::Tank, at + v2(150.0, 0.0), V2::default(), true);
        let hp = game.enemies[0].hp;
        game.update_bullets(0.1);
        assert!(game.enemies[0].hp < hp);
        game.player_collisions();
        assert_eq!(game.player.hp, game.player.max_hp);
    }
}

#[test]
fn star_beams_destroy_crossing_bullets_at_range_but_leave_gaps_and_near_misses() {
    for (offset, crossing, hit) in [
        (v2(300.0, 0.0), true, true),
        (v2(300.0, 0.0), false, true),
        (v2(300.0, 15.1), false, false),
        (v2(250.0, 100.0), false, false),
        (v2(700.0, 0.0), false, true),
    ] {
        let mut game = Game::new(7);
        let at = game.player.pos;
        game.player.spin = Some(Spin {
            active: 0.001,
            ..Spin::new(1.0, false, 1.0, at, 0.0)
        });
        game.add_orb(at + offset, V2::default());
        if crossing {
            game.orbs[0].prev += v2(0.0, -30.0);
            game.orbs[0].pos += v2(0.0, 30.0);
        }
        game.update_spin(0.001);
        assert_eq!(game.orbs.is_empty(), hit, "{offset:?}, crossing={crossing}");
        assert!(
            game.bullets.is_empty(),
            "outer beam contacts destroy, rather than reflect"
        );
    }
}

#[test]
fn star_laser_fades_and_damage_stays_similar_at_different_frame_rates() {
    let mut totals = Vec::new();
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for fps in [30, 60, 144] {
        let mut game = Game::new(7);
        game.player.spin = Some(Spin::new(1.0, false, 1.0, game.player.pos, 0.0));
        let width = game.player.spin.as_ref().unwrap().width();
        game.add_enemy(
            Kind::Tank,
            game.player.pos + v2(250.0, 0.0),
            V2::default(),
            true,
        );
        game.enemies[0].hp = 1000.0;
        for _ in 0..fps {
            game.update_player(1.0 / fps as f32, &Input::default(), &view);
            game.update_spin(1.0 / fps as f32);
        }
        let spin = game.player.spin.as_ref().unwrap();
        assert!(spin.width() < width);
        assert!(spin.strength() < 0.4);
        assert!(game.bullets.is_empty());
        let damage = 1000.0 - game.enemies[0].hp;
        assert!(damage > 25.0, "{damage} at {fps} fps");
        totals.push(damage);
    }
    let min = totals.iter().copied().fold(f32::INFINITY, f32::min);
    let max = totals.iter().copied().fold(0.0, f32::max);
    assert!(max < min * 1.1, "frame-rate damage spread: {totals:?}");
}

#[test]
fn star_lasers_keep_full_reach_as_they_thin_and_rotate_slowly() {
    for fading in [false, true] {
        let mut game = Game::new(7);
        game.resize_arena(v2(5000.0, 3500.0));
        game.player.pos = v2(200.0, 200.0);
        let mut spin = Spin::new(1.0, false, 1.0, game.player.pos, 0.0);
        spin.active = 0.001;
        if fading {
            spin.life = 0.001;
        }
        assert!(spin.angular_speed() <= TAU * 0.5);
        assert_eq!(spin.width() < 1.0, fading);
        game.player.spin = Some(spin);
        game.add_enemy(Kind::Tank, v2(4400.0, 200.0), V2::default(), true);
        let hp = game.enemies[0].hp;
        game.add_orb(v2(4400.0, 200.0), V2::default());
        game.add_orb(v2(4400.0, 210.0), V2::default());
        game.update_spin(0.001);
        assert!(
            game.enemies[0].hp < hp,
            "distant ships take damage even at the end"
        );
        assert_eq!(
            game.orbs.len(),
            usize::from(fading),
            "fading beam still reaches, but misses the offset shot"
        );
    }
}

#[test]
fn landing_destroys_every_overlapping_ship_and_costs_exactly_one_heart() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for boss_target in [false, true] {
        let mut game = Game::new(7);
        game.state = State::Playing;
        game.player.vel = v2(-SPIN_SPEED, 0.0);
        game.player.invuln = 10.0;
        game.player.spin = Some(Spin {
            life: 0.5,
            active: 0.02,
            fire_cd: 1.0,
            ..Spin::new(0.0, false, 1.0, game.player.pos, 0.0)
        });
        let at = game.player.pos;
        for offset in [-10.0, 10.0] {
            game.add_enemy(Kind::Tank, at + v2(offset, 0.0), V2::default(), true);
        }
        if boss_target {
            game.spawn_boss();
            let boss = game.boss.as_mut().unwrap();
            boss.pos = at;
            boss.entering = 0.0;
        }
        let hp = game.player.hp;
        game.player.spin.as_mut().unwrap().life = 0.0;
        game.update_player(0.01, &Input::default(), &view);
        game.update_spin(0.01);
        game.update_bullets(0.01);
        assert!(game.enemies.is_empty());
        assert!(game.boss.is_none());
        assert!(game.player.spin.is_none());
        assert_eq!(game.player.hp, hp - 1);
        assert_eq!(game.player.vel, v2(-SPIN_SPEED * 0.35, 0.0));
        assert_eq!(game.player.pos, at);
        game.player_collisions();
        assert_eq!(game.player.hp, hp - 1);
    }
}

#[test]
fn empty_landing_is_free_and_last_heart_landing_is_lethal() {
    for overlap in [false, true] {
        let mut game = Game::new(7);
        game.state = State::Playing;
        game.dev.god = false;
        game.player.hp = 1;
        game.player.spin = Some(Spin {
            life: 0.0,
            active: 0.0,
            fire_cd: 1.0,
            ..Spin::new(0.0, false, 1.0, game.player.pos, 0.0)
        });
        if overlap {
            game.add_enemy(Kind::Tank, game.player.pos, V2::default(), true);
        }
        game.update_spin(0.01);
        game.update_bullets(0.01);
        assert!(game.enemies.is_empty());
        assert_eq!(game.player.hp, if overlap { 0 } else { 1 });
        assert_eq!(
            game.state,
            if overlap {
                State::Dying
            } else {
                State::Playing
            }
        );
    }
}

#[test]
fn star_charge_cancels_on_pause_and_active_star_freezes_without_recoil() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let held = Input {
        spin: true,
        ..Input::default()
    };
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.update_player(
        0.01,
        &Input {
            dash: true,
            spin: true,
            ..Input::default()
        },
        &view,
    );
    for _ in 0..60 {
        game.update_player(0.01, &held, &view);
    }
    assert!(game.player.charge > 0.0);
    game.pause();
    assert_eq!(game.player.charge, 0.0);
    game.state = State::Playing;
    game.update_player(0.01, &held, &view);
    assert_eq!(game.player.charge, 0.0);
    assert!(game.player.spin.is_none());

    game.player.spin = Some(Spin::new(1.0, false, 1.0, game.player.pos, 0.0));
    game.player.vel = v2(SPIN_SPEED, 0.0);
    game.pause();
    let life = game.player.spin.as_ref().unwrap().life;
    let pos = game.player.pos;
    game.update(0.5, &held, &view);
    assert_eq!(game.player.spin.as_ref().unwrap().life, life);
    assert_eq!(game.player.pos, pos);
    game.state = State::Playing;
    game.update_player(
        0.02,
        &Input {
            fire: true,
            ..Input::default()
        },
        &view,
    );
    game.update_spin(0.02);
    assert_eq!(game.player.vel, v2(SPIN_SPEED, 0.0));
    assert!(game.bullets.is_empty(), "star fires beams, not pellets");
    game.state = State::Dying;
    game.update_world(0.01);
    assert!(game.player.spin.is_none());
}

#[test]
fn automatic_spin_landing_clears_overlaps_and_charges_the_heart_in_world_update() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.dev.god = false;
    game.player.hp = 2;
    game.player.spin = Some(Spin {
        life: 0.005,
        active: 0.0,
        fire_cd: 1.0,
        ..Spin::new(0.0, false, 1.0, game.player.pos, 0.0)
    });
    game.add_enemy(Kind::Tank, game.player.pos, V2::default(), true);
    game.enemies[0].act = Act::Recover;
    game.enemies[0].act_t = 1.0;
    game.update(
        0.01,
        &Input {
            spin: true,
            ..Input::default()
        },
        &view,
    );
    assert!(game.player.spin.is_none());
    assert!(game.enemies.is_empty());
    assert_eq!(game.player.hp, 1);
    assert!(
        game.pickups.is_empty(),
        "crushed ships cannot immediately refund the heart"
    );
}

#[test]
fn arena_grows_independently_and_survives_restarts() {
    let mut game = Game::new(7);
    let mut view = View {
        scale: 1.0,
        size: v2(3200.0, 900.0),
    };
    game.update(0.0, &Input::default(), &view);
    assert_eq!(game.arena_size, v2(3200.0, WORLD_H));
    assert_eq!(game.player.pos, game.arena_size * 0.5);
    game.set_difficulty(Difficulty::Extreme);
    assert_eq!(game.player.pos, game.arena_size * 0.5);

    view.size = v2(3200.0, 2000.0);
    game.update(0.0, &Input::default(), &view);
    game.start();
    assert_eq!(game.arena_size, view.size);
    assert_eq!(game.player.pos, view.size * 0.5);

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
    assert_eq!(game.arena_size, view.size);
    assert_eq!(game.player.pos, view.size * 0.5);
}

#[test]
fn shrinking_a_paused_arena_keeps_entities_reachable_and_snaps_camera() {
    let mut game = Game::new(7);
    game.resize_arena(v2(3200.0, 2000.0));
    game.start();
    game.state = State::Paused;
    let corner = v2(3100.0, 1900.0);
    game.player.pos = corner;
    game.add_enemy(Kind::Tank, corner, V2::default(), false);
    game.spawns.push(Spawn {
        kind: Kind::Chaser,
        pos: corner,
        t: 0.9,
        elite: false,
    });
    game.pickups.push(Pickup {
        kind: PickupKind::Heart,
        pos: corner,
        life: 10.0,
    });
    game.spawn_boss();
    let boss = game.boss.as_mut().unwrap();
    boss.entering = 0.0;
    boss.pos = corner;
    game.cam = corner;

    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    game.update(0.0, &Input::default(), &view);
    assert_eq!(game.state, State::Paused);
    assert_eq!(game.arena_size, v2(WORLD_W, WORLD_H));
    for (pos, radius) in [
        (game.player.pos, PLAYER_RADIUS),
        (game.enemies[0].pos, Kind::Tank.radius()),
        (game.spawns[0].pos, Kind::Chaser.radius()),
        (game.pickups[0].pos, PLAYER_RADIUS),
        (game.boss.as_ref().unwrap().pos, BOSS_RADIUS),
    ] {
        assert_eq!(pos, game.arena_size - v2(WALL + radius, WALL + radius));
    }
    assert_ne!(game.cam, corner);
    assert!(!game.snap_camera);
}

#[test]
fn fast_bullet_path_hits_enemy_between_frames() {
    // A 25-unit step jumps straight over a radius-10 enemy; the swept test must still catch it.
    assert!(segment_hit(v2(12.0, 0.0), v2(0.0, 0.0), v2(25.0, 0.0), 10.0).is_some());
    assert!(segment_hit(v2(12.0, 30.0), v2(0.0, 0.0), v2(25.0, 0.0), 10.0).is_none());
}

#[test]
fn bullets_intercept_orbs_only_when_their_cores_meet_at_the_same_time() {
    // Player shot moves from (300, 300) to (425, 300) in 0.1 seconds.
    for (pos, vel, heavy, hit) in [
        (v2(360.0, 300.0), V2::default(), false, true),
        (v2(425.0, 300.0), v2(-1250.0, 0.0), false, true),
        (v2(362.5, 237.5), v2(0.0, 1250.0), false, true),
        (v2(362.5, 175.0), v2(0.0, 1250.0), false, false),
        (v2(330.0, 300.0), v2(1250.0, 0.0), false, false),
        (v2(360.0, 310.9), V2::default(), false, true),
        (v2(360.0, 311.1), V2::default(), false, false),
        (v2(360.0, 316.9), V2::default(), true, true),
        (v2(360.0, 317.1), V2::default(), true, false),
    ] {
        let mut game = Game::new(7);
        game.bullets.push(Bullet {
            pos: v2(300.0, 300.0),
            prev: v2(300.0, 300.0),
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: BULLET_RANGE,
            damage: 1.0,
            pierce: 0,
            last_hit: 0,
            falloff: true,
        });
        game.orbs.push(Orb {
            pos,
            prev: pos,
            vel,
            life: 6.0,
            heavy,
        });
        game.update_enemies(0.1);
        game.update_bullets(0.1);
        assert_eq!(game.orbs.is_empty(), hit, "orb at {pos:?}, vel {vel:?}");
        assert_eq!(game.bullets.is_empty(), hit);
        if hit {
            assert!(game.sfx.contains(&Sfx::Hit));
            assert!(!game.particles.is_empty());
        } else {
            assert_eq!(game.orbs[0].pos, pos + vel * 0.1);
        }
    }
}

#[test]
fn normal_and_gatling_shots_intercept_the_first_orb_without_upgrades() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for gatling in [false, true] {
        let mut game = Game::new(7);
        if gatling {
            game.update_player(
                0.5,
                &Input {
                    charge: true,
                    ..Input::default()
                },
                &view,
            );
            game.update_player(0.0, &Input::default(), &view);
            game.update_gatling(0.01);
        } else {
            game.update_player(
                0.01,
                &Input {
                    fire: true,
                    ..Input::default()
                },
                &view,
            );
        }
        // Isolate one pellet so the rest of the spray cannot clear the second orb.
        game.bullets.truncate(1);
        let shot = &game.bullets[0];
        let (pos, vel) = (shot.pos, shot.vel);
        // Reverse vector order: the nearer orb must win.
        game.add_orb(pos + vel * 0.12, V2::default());
        game.add_orb(pos + vel * 0.07, V2::default());
        let count = game.bullets.len();
        game.update_bullets(0.15);
        assert_eq!(game.bullets.len(), count - 1);
        assert_eq!(game.orbs.len(), 1);
        assert_eq!(game.orbs[0].pos, pos + vel * 0.12);
    }
}

#[test]
fn interceptions_respect_enemy_and_boss_order_and_piercing() {
    for (boss_target, pierce, orb_first) in [
        (false, 0, false),
        (false, 1, false),
        (false, 0, true),
        (true, 0, false),
        (true, 0, true),
    ] {
        let mut game = Game::new(7);
        game.bullets.push(Bullet {
            pos: v2(300.0, 300.0),
            prev: v2(300.0, 300.0),
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: BULLET_RANGE,
            damage: 1.0,
            pierce,
            last_hit: 0,
            falloff: true,
        });
        let (target_x, orb_x) = if orb_first {
            (480.0, 350.0)
        } else {
            (400.0, 480.0)
        };
        game.add_orb(v2(orb_x, 300.0), V2::default());
        let hp = if boss_target {
            game.spawn_boss();
            let boss = game.boss.as_mut().unwrap();
            boss.pos = v2(target_x, 300.0);
            boss.entering = 0.0;
            boss.hp
        } else {
            game.add_enemy(Kind::Tank, v2(target_x, 300.0), V2::default(), false);
            game.enemies[0].hp
        };
        game.update_bullets(0.2);
        assert!(game.bullets.is_empty());
        assert_eq!(game.orbs.is_empty(), orb_first || pierce > 0);
        let remaining = if boss_target {
            game.boss.as_ref().unwrap().hp
        } else {
            game.enemies[0].hp
        };
        assert_eq!(remaining < hp, !orb_first);
    }
}

#[test]
fn range_and_walls_end_the_shots_sweep_without_leaving_it_waiting_for_orbs() {
    for wall in [false, true] {
        let mut game = Game::new(7);
        let x = if wall { WORLD_W - WALL - 20.0 } else { 300.0 };
        game.bullets.push(Bullet {
            pos: v2(x, 300.0),
            prev: v2(x, 300.0),
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: if wall { BULLET_RANGE } else { 50.0 },
            damage: 1.0,
            pierce: 0,
            last_hit: 0,
            falloff: true,
        });
        game.add_orb(
            v2(x + if wall { 15.0 } else { 45.0 }, 380.0),
            v2(0.0, -1000.0),
        );
        game.update_enemies(0.15);
        game.update_bullets(0.15);
        assert!(game.bullets.is_empty());
        assert_eq!(game.orbs.len(), 1);
        assert!(!game.sfx.contains(&Sfx::Hit));
    }
}

#[test]
fn an_orbs_final_sweep_can_be_intercepted_before_it_leaves_or_expires() {
    for (vel, life, hit) in [
        (v2(-4000.0, 0.0), 6.0, true),
        (V2::default(), 0.06, true),
        (V2::default(), 0.01, false),
    ] {
        let mut game = Game::new(7);
        game.bullets.push(Bullet {
            pos: v2(300.0, 300.0),
            prev: v2(300.0, 300.0),
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: BULLET_RANGE,
            damage: 1.0,
            pierce: 0,
            last_hit: 0,
            falloff: true,
        });
        game.add_orb(v2(360.0, 300.0), vel);
        game.orbs[0].life = life;
        game.update_enemies(0.1);
        game.update_bullets(0.1);
        assert!(game.orbs.is_empty());
        assert_eq!(game.bullets.is_empty(), hit);
        assert_eq!(game.sfx.contains(&Sfx::Hit), hit);
    }
}

#[test]
fn damage_is_full_up_close_and_falls_off_with_distance() {
    assert_eq!(bullet_damage(0.0, BULLET_RANGE), 1.0);
    assert_eq!(bullet_damage(FULL_DAMAGE_RANGE, BULLET_RANGE), 1.0);
    let mid = bullet_damage((FULL_DAMAGE_RANGE + BULLET_RANGE) / 2.0, BULLET_RANGE);
    assert!(mid < 1.0 && mid > MIN_DAMAGE, "{mid}");
    assert!((bullet_damage(BULLET_RANGE, BULLET_RANGE) - MIN_DAMAGE).abs() < 1e-6);
}

#[test]
fn god_mode_keeps_hit_reactions_and_stops_at_one_heart() {
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.dev.god = true;
    game.player.hp = 2;
    let source = game.player.pos - v2(20.0, 0.0);
    game.add_enemy(Kind::Chaser, source, V2::default(), false);

    for _ in 0..3 {
        game.player.invuln = 0.0;
        game.player.vel = V2::default();
        game.chain = 12;
        game.particles.clear();
        game.sfx.clear();
        game.player_collisions();

        assert_eq!(game.state, State::Playing);
        assert_eq!(game.player.hp, 1);
        assert_eq!(game.player.vel, v2(700.0, 0.0));
        assert_eq!(game.player.invuln, 1.3);
        assert_eq!(game.chain, 0);
        assert_eq!(game.shake, 16.0);
        assert_eq!(game.hitstop, 0.09);
        assert_eq!(game.sfx, vec![Sfx::Hurt]);
        assert_eq!(game.particles.len(), 22);
        assert_eq!(game.enemies[0].vel, v2(-600.0, 0.0));

        game.player_collisions();
        assert_eq!(game.particles.len(), 22, "hit grace prevents another hit");
        assert_eq!(game.sfx, vec![Sfx::Hurt]);
    }
}

#[test]
fn hits_still_kill_without_god_mode() {
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.dev.god = false;
    game.player.hp = 1;
    let source = game.player.pos - v2(20.0, 0.0);
    game.add_enemy(Kind::Chaser, source, V2::default(), false);

    game.player_collisions();

    assert_eq!(game.player.hp, 0);
    assert_eq!(game.state, State::Dying);
    assert!(game.sfx.contains(&Sfx::Hurt));
    assert!(game.sfx.contains(&Sfx::BigExplode));
}

#[test]
fn holding_fire_heats_up_gradually_then_overheats() {
    let mut game = Game::new(7);
    game.start();
    game.dev.god = true;
    let input = Input {
        fire: true,
        ..Input::default()
    };
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let released = Input::default();
    let run = |game: &mut Game, input: &Input, seconds: f32| {
        for _ in 0..(seconds * 60.0) as usize {
            game.update(1.0 / 60.0, input, &view);
        }
        (game.player.heat, game.player.overheated)
    };
    let (heat, overheated) = run(&mut game, &input, 1.5);
    assert!(
        heat > 0.25 && heat < 0.7 && !overheated,
        "after 1.5 s: {heat}"
    );
    let (_, overheated) = run(&mut game, &input, 2.5);
    assert!(overheated, "should overheat within 4 s of continuous fire");
    let (heat, overheated) = run(&mut game, &released, 1.5);
    assert!(
        !overheated && heat <= HEAT_RESUME,
        "gun unlocks after cooling: {heat}"
    );
}

#[test]
fn charged_gatling_grows_and_rewards_release_before_overheating() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for (seconds, multiplier, boosted) in [(0.5, 1.0, false), (1.7, 1.1, true), (2.0, 0.75, false)]
    {
        let mut game = Game::new(7);
        let held = Input {
            charge: true,
            fire: true,
            ..Input::default()
        };
        for _ in 0..(seconds * 60.0) as usize {
            game.update_player(1.0 / 60.0, &held, &view);
        }
        if seconds < 1.8 {
            assert!(game.gatling.is_none(), "release is needed to fire");
            assert!((game.player.charge - seconds / 1.8).abs() < 1e-5);
            game.update_player(1.0 / 60.0, &Input::default(), &view);
        }
        assert!(
            game.bullets.is_empty(),
            "regular fire must wait while charging"
        );
        let gatling = game.gatling.as_ref().unwrap();
        assert!((gatling.damage - (1.0 + 2.0 * gatling.charge) * multiplier).abs() < 1e-4);
        assert_eq!(gatling.boosted, boosted);
        assert!((gatling.duration - (0.35 + 1.25 * gatling.charge)).abs() < 1e-5);
        assert!((gatling.range - (650.0 + 550.0 * gatling.charge)).abs() < 1e-4);
        assert_eq!(game.player.charge, 0.0);
        assert_eq!(game.player.overheated, seconds > 1.8);
        game.update_player(0.1, &held, &view);
        assert_eq!(game.player.charge, 0.0, "cannot charge during the barrage");
        assert!(game.bullets.is_empty());
        game.update_gatling(2.0);
        assert!(game.gatling.is_none());
        if seconds > 1.8 {
            for _ in 0..120 {
                game.update_player(1.0 / 60.0, &held, &view);
            }
            assert!(!game.player.overheated);
            assert!(
                game.gatling.is_none(),
                "holding after autofire cannot fire again"
            );
            game.update_player(1.0 / 60.0, &Input::default(), &view);
            game.update_player(1.0 / 60.0, &held, &view);
            assert!(game.player.charge > 0.0, "release rearms charging");
        }
    }
}

#[test]
fn charged_gatling_shares_heat_and_respects_coolant_and_overheat_lock() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let held = Input {
        held: [KeyCode::Char('e')].into(),
        ..Input::default()
    };
    let mut game = Game::new(7);
    game.player.heat = 0.95;
    game.update_player(0.1, &held, &view);
    assert!(game.gatling.is_some(), "a hot gun autofires sooner");
    assert!(
        (game.gatling.as_ref().unwrap().charge - 0.05).abs() < 1e-5,
        "old heat is not free charge"
    );

    let mut game = Game::new(7);
    game.player.upgrades.add(Upgrade::Coolant);
    for _ in 0..120 {
        game.update_player(1.0 / 60.0, &held, &view);
    }
    assert!(game.gatling.is_none());
    assert!((game.player.heat - 2.0 / 1.8 * 0.75).abs() < 1e-5);

    game.player.charge = 0.0;
    game.player.heat = 1.0;
    game.player.overheated = true;
    game.update_player(0.1, &held, &view);
    assert_eq!(game.player.charge, 0.0);
    assert!(game.player.heat < 1.0 && game.player.overheated);
}

#[test]
fn pausing_cancels_charge_without_firing_on_resume() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let held = Input {
        charge: true,
        ..Input::default()
    };
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.update_player(0.5, &held, &view);
    let heat = game.player.heat;
    game.on_focus_lost();
    assert_eq!(game.state, State::Paused);
    assert_eq!(game.player.charge, 0.0);
    assert_eq!(game.player.heat, heat);
    game.state = State::Playing;
    game.update_player(1.0 / 60.0, &held, &view);
    assert_eq!(game.player.charge, 0.0, "release first after resuming");
    game.update_player(1.0 / 60.0, &Input::default(), &view);
    assert!(game.gatling.is_none());
    game.update_player(1.0 / 60.0, &held, &view);
    assert!(game.player.charge > 0.0);
}

#[test]
fn gatling_fires_spread_volleys_follows_aim_and_fades() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(13);
    game.player.pos = v2(300.0, 500.0);
    game.player.aim = 0.0;
    game.player.charge = 0.9;
    game.player.heat = 0.95;
    game.update_player(1.0 / 60.0, &Input::default(), &view);
    game.update_gatling(0.01);
    assert_eq!(
        game.bullets.len(),
        3,
        "each volley is a three-pellet spread"
    );
    assert!(game.bullets[0].vel.y < 0.0 && game.bullets[2].vel.y > 0.0);
    let first = game.bullets[1].damage;
    let origin = game.bullets[1].pos;
    assert!(game.player.vel.x < 0.0, "barrage kicks the ship backward");
    game.player.pos += v2(0.0, 100.0);
    game.player.aim = TAU / 4.0;
    game.update_gatling(0.025);
    assert_eq!(game.bullets.len(), 6);
    assert!(game.bullets[4].vel.y > 0.0 && game.bullets[4].pos.y > origin.y);
    game.update_gatling(0.5);
    assert!(
        game.bullets.last().unwrap().damage < first,
        "later volleys soften"
    );
    game.update_gatling(2.0);
    assert!(game.gatling.is_none());
    let count = game.bullets.len();
    game.update_gatling(0.5);
    assert_eq!(game.bullets.len(), count, "no shots after the burst ends");
}

#[test]
fn gatling_barrage_count_and_power_are_independent_of_frame_rate() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut counts = Vec::new();
    let mut totals = Vec::new();
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 144.0, 2.0] {
        let mut game = Game::new(17);
        game.player.charge = 0.9;
        game.player.heat = 0.95;
        game.update_player(1.0 / 60.0, &Input::default(), &view);
        while game.gatling.is_some() {
            game.update_gatling(dt);
        }
        counts.push(game.bullets.len());
        totals.push(game.bullets.iter().map(|b| b.damage).sum::<f32>());
    }
    assert!(counts.iter().all(|&n| n == counts[0] && n > 150));
    assert!(totals.iter().all(|n| (n - totals[0]).abs() < 0.01));
}

#[test]
fn gatling_uses_spread_pierce_and_range_upgrades() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(19);
    game.player.upgrades.add(Upgrade::Spread);
    game.player.upgrades.add(Upgrade::Pierce);
    game.player.upgrades.add(Upgrade::Range);
    game.player.upgrades.add(Upgrade::Range);
    game.player.charge = 0.5;
    game.update_player(1.0 / 60.0, &Input::default(), &view);
    game.update_gatling(0.01);
    assert_eq!(game.bullets.len(), 5);
    assert!(
        game.bullets
            .iter()
            .all(|b| b.pierce == 1 && !b.falloff && b.range > 1000.0)
    );
    assert!(game.bullets[0].damage < game.bullets[1].damage);
}

#[test]
fn normal_shots_transfer_forward_momentum_to_enemies_and_bosses() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for boss_target in [false, true] {
        let mut game = Game::new(21);
        game.player.pos = v2(300.0, 500.0);
        game.player.aim = 0.0;
        if boss_target {
            game.wave = 5;
            game.spawn_boss();
            let boss = game.boss.as_mut().unwrap();
            boss.pos = v2(450.0, 500.0);
            boss.entering = 0.0;
        } else {
            game.add_enemy(Kind::Tank, v2(450.0, 500.0), V2::default(), false);
        }
        game.update_player(
            1.0 / 60.0,
            &Input {
                fire: true,
                ..Input::default()
            },
            &view,
        );
        assert!(game.player.vel.x < 0.0, "normal shots have recoil too");
        game.update_bullets(0.15);
        assert!(game.bullets.is_empty(), "impact consumes the shot");
        let impulse = if boss_target {
            game.boss.as_ref().unwrap().impulse
        } else {
            game.enemies[0].impulse
        };
        assert!(
            impulse.x > 0.4 && impulse.x < 2.0 && impulse.y.abs() < impulse.x * 0.05,
            "momentum follows the bullet: {impulse:?}"
        );
    }
}

#[test]
fn gatling_barrage_gently_pushes_a_moving_boss() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let setup = || {
        let mut game = Game::new(23);
        game.state = State::Playing;
        game.wave = 5;
        game.spawn_boss();
        let boss = game.boss.as_mut().unwrap();
        boss.pos = v2(1000.0, 750.0);
        boss.entering = 0.0;
        boss.hp = 1000.0;
        game.player.pos = v2(700.0, 750.0);
        game
    };
    let mut game = setup();
    let mut idle = setup();
    game.player.charge = 0.95;
    game.player.heat = 0.95;
    game.update_player(1.0 / 60.0, &Input::default(), &view);
    for _ in 0..90 {
        game.player.aim = (game.boss.as_ref().unwrap().pos - game.player.pos).angle();
        game.update_gatling(1.0 / 60.0);
        game.update_boss(1.0 / 60.0);
        game.update_bullets(1.0 / 60.0);
        idle.update_boss(1.0 / 60.0);
    }
    let boss = game.boss.as_ref().unwrap();
    assert!(
        boss.hp < 900.0,
        "the barrage lands many pellets: {} at {:?}",
        boss.hp,
        boss.pos
    );
    let displacement = boss.pos.x - idle.boss.as_ref().unwrap().pos.x;
    assert!(
        displacement > 10.0 && displacement < 120.0,
        "the barrage adds a modest shove beyond normal movement: {displacement}"
    );
    assert!(
        boss.impulse.x > 0.0 && boss.impulse.x < 100.0,
        "hits build gentle momentum"
    );
}

#[test]
fn bullet_impulse_survives_enemy_attacks_and_damps_at_walls() {
    let mut game = Game::new(25);
    game.state = State::Playing;
    game.player.pos = v2(600.0, 600.0);
    game.add_enemy(Kind::Chaser, v2(500.0, 500.0), V2::default(), false);
    let e = &mut game.enemies[0];
    e.act = Act::Windup;
    e.act_t = 0.0;
    e.impulse = v2(-100.0, 0.0);
    game.update_enemies(1.0 / 60.0);
    assert_eq!(game.enemies[0].act, Act::Strike);
    assert!(
        game.enemies[0].impulse.x < -90.0,
        "the lunge cannot overwrite hit momentum"
    );
    let e = &mut game.enemies[0];
    e.pos = v2(WALL + e.kind.radius(), 500.0);
    e.impulse = v2(-1000.0, 0.0);
    game.update_enemies(1.0 / 60.0);
    assert_eq!(game.enemies[0].impulse.x, 0.0, "a wall absorbs the impulse");
}

#[test]
fn boss_entrance_blocks_damage_and_bullet_impulse() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(27);
    game.wave = 5;
    game.spawn_boss();
    game.boss.as_mut().unwrap().pos = v2(450.0, 500.0);
    let hp = game.boss.as_ref().unwrap().hp;
    game.player.pos = v2(300.0, 500.0);
    game.player.aim = 0.0;
    game.update_player(
        1.0 / 60.0,
        &Input {
            fire: true,
            ..Input::default()
        },
        &view,
    );
    game.update_bullets(0.15);
    assert_eq!(game.boss.as_ref().unwrap().hp, hp);
    assert_eq!(game.boss.as_ref().unwrap().impulse, V2::default());
}

#[test]
fn gatling_freezes_while_paused_and_ends_on_death() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(25);
    game.state = State::Playing;
    game.player.charge = 0.5;
    game.update_player(1.0 / 60.0, &Input::default(), &view);
    let life = game.gatling.as_ref().unwrap().life;
    game.pause();
    game.update(0.5, &Input::default(), &view);
    assert_eq!(game.gatling.as_ref().unwrap().life, life);
    game.state = State::Playing;
    game.update(0.1, &Input::default(), &view);
    assert!(game.gatling.as_ref().unwrap().life < life);
    game.state = State::Dying;
    game.update(0.1, &Input::default(), &view);
    assert!(game.gatling.is_none(), "death stops the barrage");
}

#[test]
fn enemies_double_while_the_player_grows_a_fifth_by_wave_eleven() {
    let mut game = Game::new(9);
    game.wave = 11;
    assert!(
        (game.power() - 1.2).abs() < 1e-5,
        "player power {}",
        game.power()
    );
    assert!((game.enemy_hp_scale() - 1.6).abs() < 1e-5);
    game.wave = 40;
    assert!(
        (game.enemy_hp_scale() - 2.0).abs() < 1e-5,
        "health growth caps at 2×"
    );
    game.wave = 11;
    assert!(
        game.enemy_cooldown_scale() < 0.75,
        "enemies attack more often"
    );
    game.wave = 60;
    assert!(
        game.power() <= 1.3 && game.enemy_cooldown_scale() >= 0.45,
        "both are capped"
    );
}

#[test]
fn difficulty_sets_hearts_and_keeps_separate_best_scores() {
    let mut game = Game::new(9);
    game.set_difficulty(Difficulty::Extreme);
    assert_eq!(game.player.max_hp, 3);
    let paths: Vec<_> = Difficulty::ALL
        .iter()
        .map(|&d| best_score_path(d))
        .collect();
    assert!(paths[0] != paths[1] && paths[1] != paths[2]);
}

#[test]
fn multiplier_grows_with_chain_and_caps() {
    let mut game = Game::new(1);
    assert_eq!(game.multiplier(), 1);
    game.chain = 12;
    assert_eq!(game.multiplier(), 3);
    game.chain = 500;
    assert_eq!(game.multiplier(), 8);
}

#[test]
fn boss_lands_away_from_a_centered_player_and_keeps_distance() {
    let mut game = Game::new(3);
    game.start();
    game.dev.god = true;
    game.start_wave(5);
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let idle = Input::default();
    for _ in 0..(60 * 9) {
        game.update(1.0 / 60.0, &idle, &view);
    }
    let boss = game.boss.as_ref().expect("boss spawned on wave 5");
    let dist = (boss.pos - game.player.pos).len();
    assert!(dist > 200.0, "boss sits {dist} units from the player");
}

#[test]
fn after_the_boss_you_pick_an_upgrade_then_the_next_wave_starts() {
    let mut game = Game::new(11);
    game.start();
    game.dev.god = true;
    game.start_wave(5);
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let idle = Input::default();
    let run = |game: &mut Game, input: &Input, seconds: f32| {
        for _ in 0..(seconds * 60.0) as usize {
            game.update(1.0 / 60.0, input, &view);
        }
    };
    run(&mut game, &idle, 6.0);
    game.boss.as_mut().expect("boss is up").hp = 0.0;
    run(&mut game, &idle, 5.0);
    assert_eq!(game.state, State::Upgrade);
    assert_eq!(game.offer.len(), 3);
    let first = game.offer[0];
    let pick = Input {
        pressed: vec![KeyCode::Char('1')],
        ..Input::default()
    };
    run(&mut game, &pick, 1.0 / 60.0);
    assert_eq!(game.state, State::Playing);
    assert_eq!(game.wave, 6);
    assert_eq!(game.player.upgrades.level(first), 1);
}

#[test]
fn piercing_bullet_hits_two_enemies_in_a_line() {
    let mut game = Game::new(13);
    game.start();
    game.enemies.clear();
    for x in [400.0, 470.0] {
        game.add_enemy(Kind::Tank, v2(x, 300.0), V2::default(), false);
    }
    let full = game.enemies[0].hp;
    game.bullets.push(Bullet {
        pos: v2(300.0, 300.0),
        prev: v2(300.0, 300.0),
        vel: v2(BULLET_SPEED, 0.0),
        traveled: 0.0,
        range: BULLET_RANGE,
        damage: 1.0,
        pierce: 1,
        last_hit: 0,
        falloff: true,
    });
    for _ in 0..12 {
        game.update_bullets(1.0 / 60.0);
    }
    assert!(
        game.enemies.iter().all(|e| e.hp < full),
        "both tanks took a hit"
    );
    assert!(
        game.bullets.is_empty(),
        "bullet stops after its last pierce"
    );
}

#[test]
fn beating_the_boss_pays_out_and_moves_to_the_next_wave() {
    let mut game = Game::new(5);
    game.start();
    game.dev.god = true;
    game.start_wave(5);
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let idle = Input::default();
    let run = |game: &mut Game, seconds: f32| {
        for _ in 0..(seconds * 60.0) as usize {
            game.update(1.0 / 60.0, &idle, &view);
        }
    };
    run(&mut game, 6.0);
    let score = game.score;
    game.boss.as_mut().expect("boss is up").hp = 0.0;
    run(&mut game, 0.5);
    assert!(game.boss.is_none(), "boss should be gone");
    assert!(game.score >= score + 1500, "boss bounty paid");
    assert!(game.pickups.len() >= 2, "boss drops a heart and rapid fire");
    run(&mut game, 6.0);
    assert_eq!(game.state, State::Upgrade, "upgrade cards after the boss");
}

#[test]
fn tanks_lob_volleys_from_afar_and_enrage_when_left_alone() {
    let mut game = Game::new(17);
    game.start();
    game.player.invuln = 10.0;
    game.budget = 0;
    game.banner = 0.0;
    game.enemies.clear();
    game.spawns.clear();
    let far = game.player.pos + v2(600.0, 0.0);
    game.add_enemy(Kind::Tank, far, V2::default(), false);
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for _ in 0..(60 * 6) {
        game.update(1.0 / 60.0, &Input::default(), &view);
    }
    assert!(game.enemies[0].enraged, "a lone tank enrages");
    assert!(
        game.orbs.iter().any(|o| o.heavy),
        "and lobs heavy volleys at range"
    );
}

#[test]
fn firing_or_dashing_into_the_card_screen_does_not_pick_a_card() {
    let mut game = Game::new(19);
    game.start();
    game.offer = game.roll_offer();
    game.state = State::Upgrade;
    game.state_t = 0.0;
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mash = Input {
        clicked: true,
        fire: true,
        pressed: vec![KeyCode::Char(' ')],
        mouse: Some(v2(800.0, 5.0)),
        mouse_moved: true,
        ..Input::default()
    };
    for _ in 0..20 {
        game.update(1.0 / 60.0, &mash, &view);
    }
    assert_eq!(game.state, State::Upgrade, "a card was taken blind");
    for _ in 0..30 {
        game.update(1.0 / 60.0, &Input::default(), &view);
    }
    game.update(1.0 / 60.0, &mash, &view);
    assert_eq!(
        game.state,
        State::Playing,
        "a deliberate pick after the grace works"
    );
}

#[test]
fn every_state_enemy_and_boss_phase_renders_without_panicking() {
    use crate::{
        art,
        render::{Renderer, Stats},
    };
    let (atlas, sprites) = art::build_sprites(0.6);
    let win = catnip::term::Window {
        cols: 120,
        rows: 40,
        width: 1200,
        height: 800,
    };
    let mut renderer = Renderer::new((100, 100), atlas.len());
    let stats = Stats {
        fps: 60.0,
        frame_ms: 0.2,
        kb_per_frame: 4.0,
    };
    let view = View {
        scale: 0.6,
        size: v2(2000.0, 1333.0),
    };

    let mut game = Game::new(23);
    game.start();
    game.dev.god = true;
    for (i, kind) in Kind::ALL.into_iter().enumerate() {
        let at = game.player.pos + v2(80.0 * i as f32 - 120.0, 90.0);
        game.add_enemy(kind, at, V2::default(), true);
        game.add_enemy(kind, at + v2(0.0, 60.0), V2::default(), false);
    }
    game.enemies[0].enraged = true;
    game.start_wave(5);
    let mut out = Vec::new();
    let mut draw = |game: &Game| {
        out.clear();
        renderer.frame(&mut out, game, &sprites, 0.6, win, &stats);
        assert!(!out.is_empty());
    };
    // Title, play, boss entrance and fight, pause, cards, death, game over.
    draw(&game);
    game.player.spin = Some(Spin::new(1.0, true, 1.1, game.player.pos, 0.0));
    game.update_player(
        0.05,
        &Input {
            spin: true,
            ..Input::default()
        },
        &view,
    );
    game.update_spin(0.05);
    draw(&game);
    game.state = State::Paused;
    draw(&game);
    game.state = State::Playing;
    game.player.spin = None;
    draw(&game);
    for charge in [0.1, 0.5, 0.95] {
        game.player.charge = charge;
        game.player.heat = charge;
        draw(&game);
        game.update_player(1.0 / 60.0, &Input::default(), &view);
        draw(&game);
        game.update_gatling(0.3);
        draw(&game);
        if let Some(gatling) = &mut game.gatling {
            gatling.life = gatling.duration * 0.2;
        }
        draw(&game);
        game.update_gatling(2.0);
    }
    for _ in 0..(60 * 8) {
        game.update(
            1.0 / 60.0,
            &Input {
                fire: true,
                ..Input::default()
            },
            &view,
        );
        draw(&game);
    }
    if let Some(b) = game.boss.as_mut() {
        b.hp = b.max_hp * 0.2;
    }
    for _ in 0..120 {
        game.update(1.0 / 60.0, &Input::default(), &view);
        draw(&game);
    }
    for state in [
        State::Paused,
        State::Title,
        State::Upgrade,
        State::Dying,
        State::GameOver,
    ] {
        game.offer = game.roll_offer();
        game.state = state;
        draw(&game);
    }
}
