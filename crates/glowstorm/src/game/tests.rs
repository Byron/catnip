//! Game behavior tests: combat math, scaling, boss and upgrade flow.

use super::*;

#[test]
fn plasma_slot_fires_a_dense_stream_at_low_and_high_frame_rates() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let select = Input {
        pressed: vec![KeyCode::Char('4')],
        ..Input::default()
    };
    let fire = Input {
        fire: true,
        ..Input::default()
    };
    let mut game = Game::new(7);
    game.update(0.0, &select, &view);
    assert_eq!(game.difficulty, Difficulty::Medium);
    assert_eq!(game.player.weapon, Weapon::Default);
    game.state = State::Playing;
    game.player.fire_cd = 0.2;
    game.update(0.0, &select, &view);
    assert_eq!(game.player.weapon, Weapon::Plasma);
    game.update_player(0.0, &fire, &view);
    assert!(game.bullets.is_empty(), "switching preserves the cooldown");

    for rapid in [false, true] {
        for fps in [30, 60, 144] {
            let mut game = Game::new(7);
            game.player.weapon = Weapon::Plasma;
            game.player.rapid_t = if rapid { 2.0 } else { 0.0 };
            for _ in 0..fps {
                game.update_player(1.0 / fps as f32, &fire, &view);
            }
            let expected = if rapid { 80 } else { 40 };
            assert!(
                (expected..=expected + 1).contains(&game.bullets.len()),
                "{fps} FPS, rapid={rapid}: {} shots",
                game.bullets.len()
            );
            assert!(
                game.bullets
                    .iter()
                    .all(|b| b.weapon == Weapon::Plasma && b.damage == PLASMA_DAMAGE)
            );
            assert!(
                (game.player.heat - 0.68).abs() < 0.04,
                "heat: {}",
                game.player.heat
            );
            assert!(!game.player.overheated);
        }
    }
    game.player.fire_cd = 0.0;
    game.update_player(2.0, &Input::default(), &view);
    game.player.aim = 0.0;
    game.player.pos = v2(300.0, 500.0);
    game.player.vel = V2::default();
    game.update_player(1.0 / 30.0, &fire, &view);
    assert_eq!(game.bullets.len(), 2, "idle time does not bank shots");
    game.update_bullets(1.0 / 30.0);
    assert!(game.bullets[0].traveled > 50.0);
    assert!(
        game.bullets[1].traveled < 14.0,
        "later shots only fly for their part of the frame"
    );
    assert!((game.bullets[0].pos - game.bullets[1].pos).len() > 35.0);
}

#[test]
fn plasma_hits_splatter_and_push_even_heavy_ships_into_a_drift() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for boss in [false, true] {
        let mut game = Game::new(7);
        game.player.pos = v2(300.0, 500.0);
        game.player.aim = 0.0;
        game.player.weapon = Weapon::Plasma;
        let target = v2(500.0, 500.0);
        if boss {
            game.spawn_boss();
            let b = game.boss.as_mut().unwrap();
            b.entering = 0.0;
            b.pos = target;
            b.hp = 1000.0;
        } else {
            game.add_enemy(Kind::Tank, target, V2::default(), false);
            game.enemies[0].hp = 1000.0;
        }
        game.update_player(
            0.0,
            &Input {
                fire: true,
                ..Input::default()
            },
            &view,
        );
        game.particles.clear();
        game.update_bullets(0.2);
        assert!(game.bullets.is_empty());
        assert_eq!(game.particles.len(), 18);
        assert!(
            game.particles
                .iter()
                .all(|p| matches!(p.hue, Hue::Cyan | Hue::White))
        );
        assert!(
            game.particles
                .iter()
                .any(|p| p.size == 2 && p.vel.y.abs() > 100.0)
        );
        if boss {
            let b = game.boss.as_ref().unwrap();
            assert!(b.hp <= 994.0 && b.impulse.x > 90.0 && b.stun > 0.0);
            game.update_boss(0.1);
            assert!(game.boss.as_ref().unwrap().pos.x > target.x + 5.0);
        } else {
            let e = &game.enemies[0];
            assert!(e.hp <= 994.0 && e.impulse.x > 140.0 && e.stun > 0.0);
            game.update_enemies(0.1);
            assert!(game.enemies[0].pos.x > target.x + 10.0);
        }
    }
}

#[test]
fn bfg_charge_grows_and_honors_timing_power_upgrades_and_pause() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for seconds in [0.12, 0.36, 0.57, 0.6] {
        let mut game = Game::new(7);
        game.state = State::Playing;
        game.player.weapon = Weapon::Plasma;
        game.player.pos = v2(300.0, 500.0);
        game.player.aim = 0.0;
        game.player.upgrades.add(Upgrade::Range);
        game.player.upgrades.add(Upgrade::Range);
        game.update_player(
            seconds,
            &Input {
                charge: true,
                ..Input::default()
            },
            &view,
        );
        game.update_player(0.0, &Input::default(), &view);
        assert_eq!(game.bfgs.len(), 1);
        assert!(game.bullets.is_empty() && game.gatling.is_none());
        let forced = seconds == 0.6;
        let boosted = seconds == 0.57;
        let multiplier = if forced {
            0.75
        } else if boosted {
            1.1
        } else {
            1.0
        };
        let ball = &game.bfgs[0];
        let charge = seconds / 0.6;
        assert!((ball.charge - charge).abs() < 1e-5);
        assert!((ball.radius() - (24.0 + 56.0 * charge)).abs() < 1e-4);
        assert!((ball.damage - (120.0 + 360.0 * charge) * multiplier).abs() < 1e-3);
        assert_eq!(ball.range, 2000.0);
        assert!((ball.vel.len() - 560.0 * if boosted { 1.35 } else { 1.0 }).abs() < 1e-4);
        assert_eq!(game.player.overheated, forced);
        assert_eq!(game.player.charge, 0.0);
        let pos = ball.pos;
        game.pause();
        game.update(0.5, &Input::default(), &view);
        assert_eq!(game.bfgs[0].pos, pos);
        game.start();
        assert!(game.bfgs.is_empty());
        assert_eq!(game.player.weapon, Weapon::Default);
    }
}

#[test]
fn middle_click_detonates_flying_bfgs_and_requires_release_before_charging_again() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.player.weapon = Weapon::Plasma;
    game.player.pos = v2(300.0, 700.0);
    let hp = game.player.hp;
    for pos in [v2(700.0, 700.0), v2(1700.0, 700.0)] {
        game.bfgs
            .push(Bfg::new(pos, v2(560.0, 0.0), 0.5, 300.0, 1600.0));
    }
    game.add_enemy(Kind::Tank, v2(700.0, 900.0), V2::default(), false);
    game.enemies[0].hp = 1000.0;
    game.orbs.push(Orb {
        pos: v2(700.0, 900.0),
        prev: v2(700.0, 900.0),
        vel: V2::default(),
        life: 3.0,
        heavy: false,
    });
    let held = Input {
        charge: true,
        ..Input::default()
    };
    game.update(0.0, &held, &view);
    assert_eq!(game.bfgs.len(), 2, "holding after launch does not detonate");
    let positions: Vec<_> = game.bfgs.iter().map(|ball| ball.pos).collect();
    let mut click = Input {
        charge: true,
        middle_clicked: true,
        ..Input::default()
    };
    game.pause();
    game.update(0.0, &click, &view);
    assert_eq!(game.bfgs.len(), 2, "pause freezes the balls");
    game.state = State::Playing;
    game.update(0.0, &click, &view);
    assert!(game.bfgs.is_empty());
    for pos in positions {
        assert!(
            game.rings
                .iter()
                .any(|ring| ring.pos == pos && ring.hue == Hue::Green)
        );
    }
    assert!(game.enemies[0].hp < 900.0 && game.enemies[0].impulse.y > 400.0);
    assert!(game.orbs.is_empty());
    assert_eq!(game.player.hp, hp);
    assert!(game.particles.iter().any(|p| p.hue == Hue::Green));
    assert_eq!(game.player.charge, 0.0);
    assert!(game.player.charge_blocked);
    click.end_frame();
    game.update_player(0.4, &click, &view);
    assert_eq!(
        game.player.charge, 0.0,
        "the detonating press cannot charge another shot"
    );
    game.update_player(0.0, &Input::default(), &view);
    assert!(!game.player.charge_blocked);
    game.update_player(0.12, &held, &view);
    assert!(game.player.charge > 0.0, "release rearms ordinary charging");
}

#[test]
fn rocket_slot_fires_slow_explosive_rounds_and_charge_scales_to_ten() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    game.update(
        0.0,
        &Input {
            pressed: vec![KeyCode::Char('3')],
            ..Input::default()
        },
        &view,
    );
    assert_eq!(game.difficulty, Difficulty::Extreme);
    assert_eq!(game.player.weapon, Weapon::Default);
    game.state = State::Playing;
    game.update(
        0.0,
        &Input {
            pressed: vec![KeyCode::Char('3')],
            ..Input::default()
        },
        &view,
    );
    assert_eq!(game.player.weapon, Weapon::Rocket);
    game.update_player(
        0.0,
        &Input {
            fire: true,
            ..Input::default()
        },
        &view,
    );
    assert_eq!(game.bullets.len(), 1);
    assert_eq!(game.bullets[0].damage, 22.0);
    assert!((game.bullets[0].vel.len() - ROCKET_SPEED).abs() < 0.01);
    assert!(!game.bullets[0].homing && !game.bullets[0].falloff);
    assert_eq!(game.player.fire_cd, 0.65);
    assert!((game.player.heat - 0.3).abs() < 1e-5);
    for (charge, count, forced) in [
        (0.01, 1, false),
        (0.4, 4, false),
        (0.95, 10, false),
        (1.0, 10, true),
    ] {
        let mut game = Game::new(7);
        game.player.weapon = Weapon::Rocket;
        game.player.charge = charge;
        game.player.heat = charge;
        game.player.upgrades.add(Upgrade::Spread);
        game.player.upgrades.add(Upgrade::Spread);
        game.player.upgrades.add(Upgrade::Range);
        game.player.upgrades.add(Upgrade::Range);
        game.update_player(0.0, &Input::default(), &view);
        assert_eq!(game.bullets.len(), count, "salvo stays capped with Spread");
        let multiplier = if forced {
            0.75
        } else if charge >= 0.9 {
            1.1
        } else {
            1.0
        };
        assert!(game.bullets.iter().all(|b| b.weapon == Weapon::Rocket
            && b.homing
            && b.range == 1750.0
            && (b.damage - (1.0 + 2.0 * charge) * 10.0 * multiplier).abs() < 1e-4));
        assert!(game.gatling.is_none());
        assert_eq!(game.player.overheated, forced);
        assert_eq!(game.player.charge_blocked, forced);
        assert_eq!(game.player.charge, 0.0);
    }
}

#[test]
fn charged_rockets_turn_gently_toward_targets_ahead_without_losing_speed() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for fps in [30, 60, 144] {
        for boss_target in [false, true] {
            let mut game = Game::new(7);
            game.player.pos = v2(300.0, 500.0);
            game.player.weapon = Weapon::Rocket;
            game.player.aim = 0.0;
            game.player.charge = 0.01;
            game.update_player(0.0, &Input::default(), &view);
            if boss_target {
                game.spawn_boss();
                let b = game.boss.as_mut().unwrap();
                b.pos = v2(1000.0, 900.0);
                b.entering = 0.0;
            } else {
                game.add_enemy(Kind::Tank, v2(1000.0, 900.0), V2::default(), true);
            }
            game.add_enemy(Kind::Tank, v2(250.0, 400.0), V2::default(), false);
            for _ in 0..fps / 10 {
                game.update_bullets(1.0 / fps as f32);
            }
            let b = &game.bullets[0];
            assert!(
                b.vel.angle() > 0.06 && b.vel.angle() <= 0.081,
                "gentle turn at {fps} fps"
            );
            assert!((b.vel.len() - ROCKET_SPEED).abs() < 0.01);
            assert_eq!(b.weapon, Weapon::Rocket);
        }
    }
}

#[test]
fn rocket_blast_damages_and_stuns_all_nearby_ships_with_radial_push() {
    let mut game = Game::new(7);
    let center = v2(700.0, 700.0);
    for offset in [v2(-50.0, 0.0), v2(0.0, 170.0), v2(233.0, 0.0)] {
        game.add_enemy(Kind::Tank, center + offset, V2::default(), true);
        game.enemies.last_mut().unwrap().hp = 100.0;
    }
    game.spawn_boss();
    let b = game.boss.as_mut().unwrap();
    b.pos = center + v2(100.0, 0.0);
    b.entering = 0.0;
    b.hp = 100.0;
    game.add_orb(center + v2(199.0, 0.0), V2::default());
    game.add_orb(center + v2(201.0, 0.0), V2::default());
    game.rocket_blast(center, 20.0);
    assert!(game.enemies[0].hp < game.enemies[1].hp && game.enemies[1].hp < 100.0);
    assert!(
        game.enemies[..2]
            .iter()
            .all(|e| e.stun > 2.0 && e.stun <= 3.0 && e.act == Act::Recover)
    );
    assert!(game.enemies[0].impulse.x < -500.0 && game.enemies[1].impulse.y > 300.0);
    assert_eq!(game.enemies[2].hp, 100.0);
    assert_eq!(game.enemies[2].stun, 0.0);
    let b = game.boss.as_ref().unwrap();
    assert!(b.hp < 100.0 && b.stun > 0.7 && b.impulse.x > 300.0);
    assert_eq!(game.orbs.len(), 1);
    assert_eq!(game.particles.len(), 144);
    game.boss.as_mut().unwrap().entering = 1.0;
    let hp = game.boss.as_ref().unwrap().hp;
    game.rocket_blast(center, 20.0);
    assert_eq!(
        game.boss.as_ref().unwrap().hp,
        hp,
        "boss entrance stays protected"
    );
    assert!(
        game.enemies[0].stun <= 3.0,
        "blast stuns refresh without stacking"
    );
}

#[test]
fn rocket_self_damage_uses_only_the_ship_center_inside_the_inner_half() {
    for distance in [0.0, 99.9, 100.0, 100.1, 199.9, 200.0, 205.0] {
        for protected in [false, true] {
            let mut game = Game::new(7);
            game.state = State::Playing;
            game.player.pos = v2(700.0 + distance, 700.0);
            game.player.invuln = if protected { 1.0 } else { 0.0 };
            let hp = game.player.hp;
            game.rocket_blast(v2(700.0, 700.0), 22.0);
            assert_eq!(
                game.player.hp,
                hp - i32::from(distance < 100.0 && !protected),
                "distance {distance}"
            );
            assert_eq!(game.player.vel.len() > 0.0, distance < 200.0);
            if distance < 200.0 {
                assert!(game.player.vel.x > 0.0);
            }
        }
    }
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.player.hp = 1;
    game.rocket_blast(game.player.pos, 22.0);
    assert_eq!(game.state, State::Dying);
    assert_eq!(game.player.hp, 0);
}

#[test]
fn rockets_detonate_at_first_contact_or_expiry_and_damage_each_ship_once() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for obstacle in 0..4 {
        let mut game = Game::new(7);
        game.player.pos = v2(300.0, 500.0);
        game.player.weapon = Weapon::Rocket;
        game.player.aim = 0.0;
        game.update_player(
            0.0,
            &Input {
                fire: true,
                ..Input::default()
            },
            &view,
        );
        game.bullets[0].vel = v2(ROCKET_SPEED, 0.0);
        game.add_enemy(Kind::Tank, v2(500.0, 500.0), V2::default(), true);
        game.enemies[0].hp = 100.0;
        match obstacle {
            0 => {}
            1 => game.add_orb(v2(420.0, 500.0), V2::default()),
            2 => game.arena_size.x = 460.0,
            _ => game.bullets[0].range = 100.0,
        }
        game.update_bullets(0.4);
        assert!(game.bullets.is_empty());
        assert!(game.sfx.contains(&Sfx::BigExplode));
        let blast_pos = game.particles.iter().find(|p| p.life == 0.55).unwrap().pos;
        let expected_x = match obstacle {
            0 => 463.0,
            1 => 409.0,
            2 => 460.0 - WALL,
            _ => 422.0,
        };
        assert!((blast_pos.x - expected_x).abs() < 0.01);
        let strength =
            (1.0 - ((500.0 - expected_x).abs() - 32.0).max(0.0) / BLAST_RADIUS).clamp(0.0, 1.0);
        assert!(
            (game.enemies[0].hp - (100.0 - 22.0 * (0.3 + 0.7 * strength))).abs() < 1e-4,
            "splash only, no duplicate direct damage"
        );
    }
}

#[test]
fn rocket_salvos_freeze_when_paused_and_expire_with_bounded_effects() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.player.weapon = Weapon::Rocket;
    game.player.charge = 0.95;
    game.player.heat = 0.95;
    game.update_player(0.0, &Input::default(), &view);
    let pos = game.bullets[0].pos;
    game.pause();
    game.update(0.5, &Input::default(), &view);
    assert_eq!(game.bullets.len(), 10);
    assert_eq!(game.bullets[0].pos, pos);
    for _ in 0..240 {
        game.update_bullets(1.0 / 60.0);
        game.update_effects(1.0 / 60.0);
        assert!(game.particles.len() <= effects::MAX_PARTICLES);
    }
    assert!(game.bullets.is_empty() && game.particles.is_empty());
}

#[test]
fn sniper_entry_and_exit_impacts_are_separate_and_exit_throws_more_debris() {
    for boss_target in [false, true] {
        for lethal in [false, true] {
            let mut game = Game::new(7);
            let radius = if boss_target {
                BOSS_RADIUS + 5.0
            } else {
                Kind::Tank.radius() + 5.0
            };
            if boss_target {
                game.spawn_boss();
                let b = game.boss.as_mut().unwrap();
                b.pos = v2(500.0, 500.0);
                b.entering = 0.0;
                b.hp = if lethal { 1.0 } else { 100.0 };
            } else {
                game.add_enemy(Kind::Tank, v2(500.0, 500.0), V2::default(), true);
                game.enemies[0].hp = if lethal { 1.0 } else { 100.0 };
            }
            // Offset from center so both effects must use the actual boundary intersection.
            game.bullets.push(Bullet {
                pos: v2(400.0, 515.0),
                prev: v2(400.0, 515.0),
                delay: 0.0,
                vel: v2(5000.0, 0.0),
                traveled: 0.0,
                range: 1800.0,
                damage: SNIPER_DAMAGE,
                pierce: 0,
                hit_ids: Vec::new(),
                exits: Vec::new(),
                homing: false,
                weapon: Weapon::Sniper,
                falloff: false,
            });
            game.update_bullets(0.02);
            assert_eq!(game.bullets[0].exits.len(), 1, "round is still inside");
            let entry_x = 500.0 - (radius * radius - 225.0).sqrt();
            let entry: Vec<_> = game
                .particles
                .iter()
                .filter(|p| (p.pos - v2(entry_x, 515.0)).len() < 0.01)
                .collect();
            assert_eq!(entry.len(), 25);
            assert!(
                entry
                    .iter()
                    .all(|p| p.vel.x < 10.0 && p.size == 0 && p.life == 0.25)
            );
            if lethal && boss_target {
                game.boss = None;
            } else if lethal {
                game.enemies.clear(); // Isolate the exit from hits on the tank's spawned minions.
            }
            game.particles.clear();
            game.update_bullets(0.02);
            assert!(game.bullets[0].exits.is_empty());
            let exit_x = 500.0 + (radius * radius - 225.0).sqrt();
            assert_eq!(game.particles.len(), 62);
            assert!(
                game.particles
                    .iter()
                    .all(|p| (p.pos - v2(exit_x, 515.0)).len() < 0.01
                        && p.vel.x > 100.0
                        && p.size >= 1
                        && p.life == 0.5)
            );
            game.particles.clear();
            game.update_bullets(0.02);
            assert!(game.particles.is_empty(), "exit only happens once");
        }
    }
}

#[test]
fn sniper_exit_follows_deflection_inside_a_ship_and_stops_at_range() {
    for range in [110.0, 1800.0] {
        let mut game = Game::new(7);
        game.add_enemy(Kind::Tank, v2(500.0, 500.0), V2::default(), true);
        game.enemies[0].hp = 100.0;
        game.add_orb(v2(510.0, 500.0), V2::default());
        let mut rng = Rng(game.rng.0);
        let dir = V2::from_angle(rng.range(-TAU / 12.0, TAU / 12.0));
        game.bullets.push(Bullet {
            pos: v2(400.0, 500.0),
            prev: v2(400.0, 500.0),
            delay: 0.0,
            vel: v2(5000.0, 0.0),
            traveled: 0.0,
            range,
            damage: SNIPER_DAMAGE,
            pierce: 0,
            hit_ids: Vec::new(),
            exits: Vec::new(),
            homing: false,
            weapon: Weapon::Sniper,
            falloff: false,
        });
        game.update_bullets(0.1);
        let debris: Vec<_> = game.particles.iter().filter(|p| p.life == 0.5).collect();
        if range < 200.0 {
            assert!(
                game.bullets.is_empty() && debris.is_empty(),
                "no exit beyond range"
            );
        } else {
            assert_eq!(debris.len(), 62);
            let exit = debris[0].pos;
            assert!(((exit - v2(500.0, 500.0)).len() - 37.0).abs() < 0.01);
            assert!(exit.y != 500.0 && (exit - v2(499.0, 500.0)).norm().dot(dir) > 0.9999);
            assert!(game.bullets[0].exits.is_empty());
        }
    }
}

#[test]
fn sniper_hits_push_surviving_ships_into_a_visible_drift() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for fps in [30, 60, 144] {
        for boss_target in [false, true] {
            let mut game = Game::new(7);
            game.state = State::Playing;
            game.player.pos = v2(300.0, 500.0);
            game.player.aim = 0.0;
            game.player.weapon = Weapon::Sniper;
            if boss_target {
                game.spawn_boss();
                let b = game.boss.as_mut().unwrap();
                b.pos = v2(500.0, 500.0);
                b.entering = 0.0;
            } else {
                game.add_enemy(Kind::Tank, v2(500.0, 500.0), v2(-100.0, 0.0), true);
            }
            game.update_player(
                0.0,
                &Input {
                    fire: true,
                    ..Input::default()
                },
                &view,
            );
            game.update_bullets(0.06);
            let impulse = if boss_target {
                game.boss.as_ref().unwrap().impulse
            } else {
                game.enemies[0].impulse
            };
            assert!(impulse.x > if boss_target { 200.0 } else { 400.0 });
            let frames = if boss_target { fps / 4 } else { fps / 2 };
            for _ in 0..frames {
                if boss_target {
                    game.update_boss(1.0 / fps as f32);
                } else {
                    game.update_enemies(1.0 / fps as f32);
                }
            }
            let (pos, impulse) = if boss_target {
                let b = game.boss.as_ref().unwrap();
                (b.pos, b.impulse)
            } else {
                let e = &game.enemies[0];
                (e.pos, e.impulse)
            };
            assert!(pos.x - 500.0 > if boss_target { 30.0 } else { 100.0 });
            assert!(
                impulse.x > if boss_target { 100.0 } else { 150.0 },
                "momentum persists through drift"
            );
        }
    }
}

#[test]
fn sniper_charge_fires_a_straight_stream_with_timing_upgrades_and_weapon_lock() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for fps in [30, 60, 144] {
        for (seconds, boosted, forced) in
            [(0.2, false, false), (0.55, true, false), (0.7, false, true)]
        {
            let mut game = Game::new(7);
            game.player.weapon = Weapon::Sniper;
            game.player.aim = 0.3;
            game.player.upgrades.add(Upgrade::Spread);
            game.player.upgrades.add(Upgrade::Pierce);
            game.player.upgrades.add(Upgrade::Range);
            game.player.upgrades.add(Upgrade::Range);
            let held = Input {
                charge: true,
                ..Input::default()
            };
            for _ in 0..(seconds * fps as f32).ceil() as usize {
                game.update_player(1.0 / fps as f32, &held, &view);
            }
            game.update_player(0.0, &Input::default(), &view);
            let gun = game.gatling.as_ref().unwrap();
            assert_eq!(gun.weapon, Weapon::Sniper);
            assert_eq!(gun.boosted, boosted);
            assert_eq!(game.player.overheated, forced);
            let multiplier = if forced {
                0.75
            } else if boosted {
                1.1
            } else {
                1.0
            };
            assert!((gun.damage - (6.0 + 12.0 * gun.charge) * multiplier).abs() < 1e-4);
            assert!((gun.range - (1200.0 + 600.0 * gun.charge) * 1.25).abs() < 1e-4);
            let duration = gun.duration;
            game.player.weapon = Weapon::Default;
            game.player.vel = v2(0.0, 200.0);
            while game.gatling.is_some() {
                game.update_gatling(1.0 / fps as f32);
            }
            let interval = if boosted { 0.03 } else { 0.045 } / 1.25;
            assert_eq!(game.bullets.len(), (duration / interval).ceil() as usize);
            assert!(game.bullets.iter().all(|b| b.weapon == Weapon::Sniper
                && b.pierce == 1
                && !b.falloff
                && (b.vel.angle() - 0.3).abs() < 1e-5));
            assert!(game.bullets.last().unwrap().damage < game.bullets[0].damage);
            assert!(
                (game.bullets[0].vel.len() - if boosted { 8000.0 } else { 5000.0 }).abs() < 0.01
            );
            assert!(game.player.vel.x < 0.0, "stream has recoil");
        }
    }
}

#[test]
fn shift_thrust_boosts_forward_more_than_reverse_and_heats_only_when_active() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for fps in [30, 60, 144] {
        for key in ['d', 'a', 'w'] {
            let mut normal = Game::new(7);
            let mut boosted = Game::new(7);
            normal.cam = V2::default();
            boosted.cam = V2::default();
            let mut movement = Input {
                held: [KeyCode::Char(key)].into(),
                mouse: Some(v2(1400.0, 750.0)),
                ..Input::default()
            };
            let mut thrust = Input {
                boost: true,
                held: movement.held.clone(),
                mouse: movement.mouse,
                ..Input::default()
            };
            for _ in 0..fps / 2 {
                movement.mouse = Some(normal.player.pos + v2(1000.0, 0.0));
                thrust.mouse = Some(boosted.player.pos + v2(1000.0, 0.0));
                normal.update_player(1.0 / fps as f32, &movement, &view);
                boosted.update_player(1.0 / fps as f32, &thrust, &view);
            }
            let ratio = boosted.player.vel.len() / normal.player.vel.len();
            if key == 'w' {
                assert!((ratio - 1.0).abs() < 1e-5);
                assert_eq!(boosted.player.thrust, 0.0);
                assert_eq!(boosted.player.heat, 0.0);
            } else {
                assert!((ratio - if key == 'd' { 2.2 } else { 1.5 }).abs() < 0.01);
                assert_eq!(
                    boosted.player.thrust.signum(),
                    if key == 'd' { 1.0 } else { -1.0 }
                );
                assert!(boosted.player.heat > 0.07 && boosted.player.heat < 0.22);
            }
            boosted.update_player(
                0.1,
                &Input {
                    boost: true,
                    mouse: movement.mouse,
                    ..Input::default()
                },
                &view,
            );
            assert_eq!(
                boosted.player.thrust, 0.0,
                "stationary input has no exhaust or extra heat"
            );
        }
    }
}

#[test]
fn thrust_uses_shared_heat_and_preserves_cooling_dash_and_laser_rules() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let thrust = Input {
        boost: true,
        held: [KeyCode::Char('d')].into(),
        mouse: Some(v2(1400.0, 750.0)),
        ..Input::default()
    };
    let mut game = Game::new(7);
    game.player.heat = 0.99;
    game.update_player(
        0.1,
        &Input {
            fire: true,
            ..thrust
        },
        &view,
    );
    assert!(game.player.overheated && game.bullets.is_empty());
    game.update_player(
        0.1,
        &Input {
            boost: true,
            held: [KeyCode::Char('d')].into(),
            ..Input::default()
        },
        &view,
    );
    assert_eq!(game.player.thrust, 0.0);
    assert!(
        game.player.heat < 1.0,
        "held shift does not prevent cooling"
    );
    for _ in 0..60 {
        game.update_player(1.0 / 60.0, &Input::default(), &view);
    }
    assert!(!game.player.overheated);
    game.player.spin = Some(Spin::new(1.0, false, 1.0, game.player.pos, 0.0));
    game.update_player(
        0.1,
        &Input {
            boost: true,
            held: [KeyCode::Char('d')].into(),
            ..Input::default()
        },
        &view,
    );
    assert!(game.player.thrust > 0.0 && game.player.heat > 0.0);
    game.player.spin = None;
    game.update_player(
        0.05,
        &Input {
            boost: true,
            dash: true,
            held: [KeyCode::Char('d')].into(),
            ..Input::default()
        },
        &view,
    );
    assert_eq!(game.player.thrust, 0.0, "evasion retains its normal speed");
}

#[test]
fn thrust_leaves_less_heat_for_firing_and_coolant_reduces_its_cost() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut counts = Vec::new();
    for boost in [false, true] {
        let mut game = Game::new(7);
        let input = Input {
            boost,
            fire: true,
            held: [KeyCode::Char('d')].into(),
            ..Input::default()
        };
        for _ in 0..120 {
            game.update_player(1.0 / 60.0, &input, &view);
        }
        counts.push(game.bullets.len());
    }
    assert!(
        counts[1] < counts[0],
        "thrust leaves fewer shots before heat locks the gun"
    );
    let mut heats = Vec::new();
    for coolant in [false, true] {
        let mut game = Game::new(7);
        if coolant {
            game.player.upgrades.add(Upgrade::Coolant);
        }
        for _ in 0..30 {
            game.update_player(
                1.0 / 60.0,
                &Input {
                    boost: true,
                    held: [KeyCode::Char('d')].into(),
                    ..Input::default()
                },
                &view,
            );
        }
        heats.push(game.player.heat);
    }
    assert!(heats[1] < heats[0]);
}

#[test]
fn main_weapon_switching_keeps_cooldown_and_menu_number_keys() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    let select = Input {
        pressed: vec![KeyCode::Char('2')],
        ..Input::default()
    };
    assert_eq!(game.player.weapon, Weapon::Default);
    game.update(0.0, &select, &view);
    assert_eq!(game.difficulty, Difficulty::Hard);
    assert_eq!(game.player.weapon, Weapon::Default);
    game.state = State::Playing;
    game.update(0.0, &select, &view);
    assert_eq!(game.player.weapon, Weapon::Sniper);
    let fire = Input {
        fire: true,
        ..Input::default()
    };
    game.update_player(0.0, &fire, &view);
    assert_eq!(game.bullets.len(), 1);
    let round = &game.bullets[0];
    assert_eq!(round.weapon, Weapon::Sniper);
    assert!((round.vel.len() - 5000.0).abs() < 0.01);
    assert_eq!(round.damage, 18.0);
    assert_eq!(round.range, 1800.0);
    assert!(!round.faded());
    assert_eq!(game.player.fire_cd, 0.5);
    game.update(
        0.0,
        &Input {
            pressed: vec![KeyCode::Char('1')],
            fire: true,
            ..Input::default()
        },
        &view,
    );
    assert_eq!(game.player.weapon, Weapon::Default);
    assert_eq!(game.bullets.len(), 1, "switching does not bypass cooldown");
    game.update_player(0.25, &fire, &view);
    assert_eq!(game.bullets.len(), 1);
    game.update_player(0.25, &fire, &view);
    assert_eq!(game.bullets.len(), 2);
    assert_eq!(game.bullets[0].weapon, Weapon::Sniper);
    assert_eq!(game.bullets[1].weapon, Weapon::Default);
    game.start();
    assert_eq!(game.player.weapon, Weapon::Default);
}

#[test]
fn sniper_pierces_in_order_once_per_ship_and_loses_power() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for fps in [30, 60, 144] {
        for pierce in 0..=2 {
            let mut game = Game::new(7);
            game.player.weapon = Weapon::Sniper;
            game.player.aim = 0.0;
            game.player.upgrades.add(Upgrade::Range);
            for _ in 0..pierce {
                game.player.upgrades.add(Upgrade::Pierce);
            }
            let at = game.player.pos;
            // Reversed storage order, with overlapping circles to catch repeated hits.
            for x in [140.0, 90.0, 40.0] {
                game.add_enemy(Kind::Tank, at + v2(x, 0.0), V2::default(), true);
                game.enemies.last_mut().unwrap().hp = 100.0;
            }
            game.update_player(
                0.0,
                &Input {
                    fire: true,
                    ..Input::default()
                },
                &view,
            );
            game.bullets[0].vel = v2(5000.0, 0.0);
            for _ in 0..fps / 10 {
                game.update_bullets(1.0 / fps as f32);
            }
            let retained = 0.65 + 0.1 * pierce as f32;
            for (i, e) in game.enemies.iter().rev().enumerate() {
                let damage = 18.0 * retained.powi(i as i32);
                assert!(
                    (100.0 - e.hp - damage).abs() < 1e-4,
                    "fps {fps}, pierce {pierce}, target {i}"
                );
                let stun = (damage.max(4.0) * 0.15).min(1.2);
                assert!((e.stun - stun).abs() < 1e-4);
            }
            assert_eq!(game.bullets.len(), 1);
            assert!(game.bullets[0].faded());
            for _ in 0..fps {
                game.update_bullets(1.0 / fps as f32);
            }
            assert!(game.bullets.is_empty());
        }
    }
}

#[test]
fn stunned_enemies_drift_tumble_and_recover_without_attacking() {
    for kind in Kind::ALL {
        for fps in [30, 60, 144] {
            let mut game = Game::new(7);
            game.state = State::Playing;
            game.player.pos = v2(800.0, 500.0);
            game.add_enemy(kind, v2(500.0, 500.0), v2(200.0, 0.0), false);
            let e = &mut game.enemies[0];
            e.act = Act::Windup;
            e.act_t = 0.0;
            e.cooldown = 0.0;
            e.stagger(2.9);
            assert_eq!(e.stun, 0.0);
            assert_eq!(e.act, Act::Windup);
            assert_eq!(e.vel, v2(200.0, 0.0));
            e.stagger(6.0);
            assert_eq!(e.vel, v2(70.0, 0.0));
            let (spin, facing) = (e.spin, e.facing);
            e.impulse = v2(0.0, 80.0);
            for _ in 0..fps / 2 {
                game.update_enemies(1.0 / fps as f32);
            }
            let e = &mut game.enemies[0];
            assert_eq!(e.act, Act::Recover);
            assert!(!e.winding_up());
            assert_eq!(e.act_t, 0.2, "attack timers freeze during stun");
            assert!(e.pos.x > 520.0 && e.pos.x < 540.0 && e.pos.y > 510.0);
            assert!(e.vel.x < 50.0 && e.vel.x > 40.0);
            assert!((e.spin - spin).abs() > 2.0 && (e.facing - facing).abs() > 2.0);
            assert!(game.orbs.is_empty() && game.shockwaves.is_empty());
            let vel = e.vel;
            e.stagger(100.0);
            e.stagger(6.0);
            assert_eq!(e.stun, 1.2, "hits refresh with a duration cap");
            assert_eq!(e.vel, vel, "refreshing does not repeatedly brake the drift");
            for _ in 0..fps * 2 {
                game.update_enemies(1.0 / fps as f32);
            }
            assert_eq!(game.enemies[0].stun, 0.0);
            assert_ne!(game.enemies[0].act, Act::Recover);
        }
    }
}

#[test]
fn stun_respects_walls_and_pause() {
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.add_enemy(Kind::Shooter, v2(500.0, 500.0), V2::default(), false);
    let e = &mut game.enemies[0];
    e.stagger(6.0);
    e.pos.x = WALL + e.kind.radius();
    e.impulse = v2(-1000.0, 0.0);
    game.update_enemies(0.1);
    let e = &game.enemies[0];
    assert_eq!(e.pos.x, WALL + e.kind.radius());
    assert_eq!(e.impulse.x, 0.0);
    let (stun, pos, spin) = (e.stun, e.pos, e.spin);
    game.pause();
    game.update(
        0.5,
        &Input::default(),
        &View {
            scale: 1.0,
            size: v2(1600.0, 900.0),
        },
    );
    let e = &game.enemies[0];
    assert_eq!((e.stun, e.pos, e.spin), (stun, pos, spin));
}

#[test]
fn boss_stagger_pauses_attacks_then_recovers_and_still_allows_death() {
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.spawn_boss();
    game.update_boss(3.0);
    game.update_boss(0.1);
    let b = game.boss.as_mut().unwrap();
    b.pattern = Pattern::Fan;
    b.windup = 0.2;
    b.stagger(4.9);
    assert_eq!(b.stun, 0.0);
    b.stagger(6.0);
    assert!((b.stun - 0.3).abs() < 1e-5);
    let (pos, spin) = (b.pos, b.spin);
    game.update_boss(0.1);
    let b = game.boss.as_mut().unwrap();
    assert_eq!(b.windup, 0.2);
    assert!(b.spin < spin && b.pos != pos, "boss tumbles and drifts");
    b.stagger(100.0);
    b.stagger(6.0);
    assert_eq!(b.stun, 0.35);
    game.update_boss(0.4);
    assert!(game.orbs.is_empty());
    game.update_boss(0.3);
    assert!(!game.orbs.is_empty(), "the paused attack resumes");
    let b = game.boss.as_mut().unwrap();
    b.stagger(6.0);
    b.hp = 0.0;
    game.update_boss(0.01);
    assert!(game.boss.is_none(), "stun cannot postpone death");
}

#[test]
fn sniper_pierces_a_boss_in_order_before_deflecting_off_enemy_bullets() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    game.player.weapon = Weapon::Sniper;
    game.player.aim = 0.0;
    game.player.upgrades.add(Upgrade::Range);
    let at = game.player.pos;
    game.spawn_boss();
    let boss = game.boss.as_mut().unwrap();
    boss.pos = at + v2(180.0, 0.0);
    boss.entering = 0.0;
    boss.hp = 100.0;
    for x in [600.0, 65.0] {
        game.add_enemy(Kind::Tank, at + v2(x, 0.0), V2::default(), true);
        game.enemies.last_mut().unwrap().hp = 100.0;
    }
    game.add_orb(at + v2(260.0, 0.0), V2::default());
    game.update_player(
        0.0,
        &Input {
            fire: true,
            ..Input::default()
        },
        &view,
    );
    game.bullets[0].vel = v2(5000.0, 0.0);
    game.update_bullets(0.1);
    assert!((game.enemies[1].hp - 82.0).abs() < 1e-4);
    assert!((game.boss.as_ref().unwrap().hp - 88.3).abs() < 1e-4);
    assert_eq!(game.enemies[0].hp, 100.0);
    assert!(game.orbs.is_empty());
    assert_eq!(game.bullets.len(), 1);
    assert!(game.bullets[0].vel.angle().abs() <= TAU / 12.0);
    assert!((game.bullets[0].damage - 18.0 * 0.65 * 0.65).abs() < 1e-5);
}

#[test]
fn sniper_clears_enemy_bullets_and_continues_without_losing_speed_or_power() {
    for heavy in [false, true] {
        let mut game = Game::new(7);
        game.bullets.push(Bullet {
            pos: v2(300.0, 300.0),
            prev: v2(300.0, 300.0),
            delay: 0.0,
            vel: v2(5000.0, 0.0),
            traveled: 0.0,
            range: 1800.0,
            damage: 6.0,
            pierce: 0,
            hit_ids: Vec::new(),
            exits: Vec::new(),
            homing: false,
            weapon: Weapon::Sniper,
            falloff: false,
        });
        game.add_orb(v2(400.0, 300.0), V2::default());
        game.orbs[0].heavy = heavy;
        game.update_bullets(0.1);
        assert!(game.orbs.is_empty());
        assert_eq!(game.bullets.len(), 1);
        let b = &game.bullets[0];
        assert!(b.vel.angle().abs() > 0.001 && b.vel.angle().abs() <= TAU / 12.0);
        assert!((b.vel.len() - 5000.0).abs() < 0.001);
        assert!((b.traveled - 500.0).abs() < 0.001);
        assert!(
            b.pos.x > 700.0,
            "flight continues for the rest of the frame"
        );
        assert_eq!(b.damage, 6.0);
        assert!(b.hit_ids.is_empty());
        assert!(game.sfx.contains(&Sfx::Hit) && !game.particles.is_empty());
    }
}

#[test]
fn sniper_resweeps_multiple_deflections_and_only_hits_ships_on_its_new_path() {
    let mut game = Game::new(7);
    for _ in 0..2 {
        game.add_enemy(Kind::Tank, v2(700.0, 300.0), V2::default(), true);
        game.enemies.last_mut().unwrap().hp = 100.0;
    }
    let mut rng = Rng(game.rng.0);
    let angle1 = rng.range(-TAU / 12.0, TAU / 12.0);
    let angle2 = angle1 + rng.range(-TAU / 12.0, TAU / 12.0);
    let dir1 = V2::from_angle(angle1);
    let dir2 = V2::from_angle(angle2);
    let contact1 = v2(389.0, 300.0);
    let contact2 = contact1 + dir1 * 89.0;
    game.enemies[1].pos = contact2 + dir2 * 120.0;
    game.bullets.push(Bullet {
        pos: v2(300.0, 300.0),
        prev: v2(300.0, 300.0),
        delay: 0.0,
        vel: v2(5000.0, 0.0),
        traveled: 0.0,
        range: 1800.0,
        damage: 6.0,
        pierce: 0,
        hit_ids: Vec::new(),
        exits: Vec::new(),
        homing: false,
        weapon: Weapon::Sniper,
        falloff: false,
    });
    // Reverse storage order so the first interception must be chosen by time.
    game.add_orb(contact1 + dir1 * 100.0, V2::default());
    game.add_orb(v2(400.0, 300.0), V2::default());
    game.update_bullets(0.1);
    assert!(game.orbs.is_empty());
    let b = &game.bullets[0];
    assert!((b.vel.angle() - angle2).abs() < 1e-5);
    assert!((b.pos - (contact2 + dir2 * 322.0)).len() < 0.01);
    assert!((b.traveled - 500.0).abs() < 0.001);
    assert_eq!(game.enemies[0].hp, 100.0, "old flight path is discarded");
    assert_eq!(
        game.enemies[1].hp, 94.0,
        "deflected flight still hits ships"
    );
    assert_eq!(b.hit_ids, vec![game.enemies[1].id]);
}

#[test]
fn sniper_deflected_sweep_tracks_moving_orbs_and_still_expires() {
    for fps in [30, 60, 144] {
        for wall in [false, true] {
            for offset in [0.0, 150.0] {
                let mut game = Game::new(7);
                if wall {
                    game.arena_size.x = 650.0;
                }
                let mut rng = Rng(game.rng.0);
                let dir = V2::from_angle(rng.range(-TAU / 12.0, TAU / 12.0));
                let contact = v2(389.0, 300.0);
                let cross_time = (89.0 + 100.0) / 5000.0;
                game.bullets.push(Bullet {
                    pos: v2(300.0, 300.0),
                    prev: v2(300.0, 300.0),
                    delay: 0.0,
                    vel: v2(5000.0, 0.0),
                    traveled: 0.0,
                    range: if wall { 1800.0 } else { 400.0 },
                    damage: 6.0,
                    pierce: 0,
                    hit_ids: Vec::new(),
                    exits: Vec::new(),
                    homing: false,
                    weapon: Weapon::Sniper,
                    falloff: false,
                });
                game.add_orb(v2(400.0, 300.0), V2::default());
                game.add_orb(
                    contact + dir * 100.0 - v2(0.0, 1000.0 * cross_time + offset),
                    v2(0.0, 1000.0),
                );
                for _ in 0..fps / 10 + 1 {
                    game.update_enemies(1.0 / fps as f32);
                    game.update_bullets(1.0 / fps as f32);
                }
                assert!(
                    game.bullets.is_empty(),
                    "range or wall still expires at {fps} fps, wall {wall}"
                );
                assert_eq!(
                    game.orbs.len(),
                    usize::from(offset > 0.0),
                    "intercept uses matching times at {fps} fps"
                );
            }
        }
    }
}

#[test]
fn sniper_uses_upgrades_and_waits_for_shared_weapon_heat_and_barrages() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    game.player.weapon = Weapon::Sniper;
    game.player.upgrades.add(Upgrade::Spread);
    game.player.upgrades.add(Upgrade::Range);
    game.player.upgrades.add(Upgrade::Range);
    game.player.upgrades.add(Upgrade::Coolant);
    game.player.rapid_t = 1.0;
    let fire = Input {
        fire: true,
        ..Input::default()
    };
    game.update_player(0.0, &fire, &view);
    assert_eq!(game.bullets.len(), 3);
    assert!(
        game.bullets
            .iter()
            .all(|b| b.weapon == Weapon::Sniper && !b.falloff && b.range == 2250.0)
    );
    assert_eq!(game.player.fire_cd, 0.25);
    assert!((game.player.heat - 0.075 * 0.75 * 1.5).abs() < 1e-5);
    game.player.fire_cd = 0.0;
    game.player.heat = 1.0;
    game.player.overheated = true;
    game.update_player(0.01, &fire, &view);
    assert_eq!(game.bullets.len(), 3);
    game.player.overheated = false;
    game.player.heat = 0.0;
    game.update_player(
        0.1,
        &Input {
            charge: true,
            fire: true,
            ..Input::default()
        },
        &view,
    );
    assert_eq!(game.bullets.len(), 3);
    game.update_player(0.0, &fire, &view);
    assert!(game.gatling.is_some());
    game.player.fire_cd = 0.0;
    game.update_player(0.01, &fire, &view);
    assert_eq!(game.bullets.len(), 3);
}

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
        for _ in 0..40 {
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
fn holding_after_evasion_charges_even_during_dash_cooldown() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for key in [None, Some(KeyCode::Char(' ')), Some(KeyCode::Char('r'))] {
        for wait in [0.01, 0.25] {
            let mut game = Game::new(7);
            let press = Input {
                dash: key.is_none(),
                spin: key.is_none(),
                held: key.into_iter().collect(),
                pressed: key.into_iter().collect(),
                ..Input::default()
            };
            let held = Input {
                spin: key.is_none(),
                held: key.into_iter().collect(),
                ..Input::default()
            };
            game.update_player(0.01, &press, &view);
            game.update_player(wait, &Input::default(), &view);
            assert_eq!(game.player.dash_charges, 0);
            game.update_player(0.01, &press, &view);
            for _ in 0..40 {
                game.update_player(0.01, &held, &view);
            }
            assert!(game.player.dash_cd > 0.0);
            assert_eq!(game.player.dash_charges, 0);
            assert!(game.player.charge_spin && game.player.charge > 0.0);
            game.update_player(0.01, &Input::default(), &view);
            assert!(game.player.spin.is_some());
            assert!(game.gatling.is_none());
        }
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
        for _ in 0..if forced { 85 } else { 73 } {
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
        for _ in 0..63 {
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
fn spinning_ship_takes_contact_damage_without_interrupting_its_attack() {
    let mut game = Game::new(7);
    game.state = State::Playing;
    game.player.vel = v2(-300.0, 0.0);
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
    assert_eq!(game.player.vel, v2(-700.0, 0.0));
    assert_eq!(game.player.hp, player_hp - 1);
    assert_eq!(game.player.spin.as_ref().unwrap().life, 0.5);
    game.player_collisions();
    assert_eq!(
        game.player.hp,
        player_hp - 1,
        "normal hit grace still applies"
    );
    let hp = game.enemies[0].hp;
    game.update_spin(0.02);
    assert!(game.enemies[0].hp < hp);
    assert!(game.player.spin.is_some());
}

#[test]
fn laser_spin_uses_normal_movement_from_discharge_through_expiry() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for boosted in [false, true] {
        let mut normal = Game::new(7);
        let mut spinning = Game::new(7);
        normal.player.vel = v2(120.0, -40.0);
        spinning.player.vel = normal.player.vel;
        spinning.player.charge = 0.5;
        spinning.player.heat = if boosted { 0.95 } else { 0.5 };
        spinning.player.charge_spin = true;
        for keys in ["w", "a", "s", "d", "", ""] {
            let input = Input {
                held: keys.chars().map(KeyCode::Char).collect(),
                mouse: Some(normal.player.pos + v2(100.0, 100.0)),
                ..Input::default()
            };
            normal.update_player(0.2, &input, &view);
            spinning.update_player(0.2, &input, &view);
            spinning.update_spin(0.2);
            assert_eq!(spinning.player.vel, normal.player.vel);
            assert_eq!(spinning.player.pos, normal.player.pos);
            assert_eq!(spinning.player.aim, normal.player.aim);
        }
        assert!(spinning.player.spin.is_none());
    }
}

#[test]
fn expired_laser_only_interacts_along_the_active_part_of_movement() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    let mut game = Game::new(7);
    game.player.vel = v2(2000.0, 0.0);
    game.player.spin = Some(Spin {
        life: 0.001,
        ..Spin::new(1.0, false, 1.0, game.player.pos, 0.0)
    });
    game.add_orb(game.player.pos + v2(100.0, 25.0), V2::default());
    game.update_player(0.1, &Input::default(), &view);
    game.update_spin(0.1);
    assert!(game.player.spin.is_none());
    assert_eq!(game.orbs.len(), 1, "reflection ends when the laser expires");
}

#[test]
fn laser_spin_can_take_bullet_boss_and_shockwave_hits() {
    for hazard in 0..3 {
        let mut game = Game::new(7);
        game.state = State::Playing;
        game.player.spin = Some(Spin::new(1.0, false, 1.0, game.player.pos, 0.0));
        let at = game.player.pos + v2(20.0, 0.0);
        match hazard {
            0 => game.orbs.push(Orb {
                pos: at,
                prev: at,
                vel: V2::default(),
                life: 1.0,
                heavy: false,
            }),
            1 => {
                game.spawn_boss();
                let boss = game.boss.as_mut().unwrap();
                boss.pos = at;
                boss.entering = 0.0;
            }
            _ => game.shockwaves.push(Shockwave {
                pos: at,
                r: 20.0,
                hit: false,
            }),
        }
        let hp = game.player.hp;
        game.player_collisions();
        assert_eq!(game.player.hp, hp - 1);
        assert!(game.player.spin.is_some());
    }
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
fn laser_rays_stop_at_the_first_wall_in_any_arena() {
    for size in [v2(WORLD_W, WORLD_H), v2(5000.0, 2800.0)] {
        let lo = v2(WALL, WALL);
        let hi = size - lo;
        for pos in [size * 0.5, lo + v2(14.0, 14.0), hi - v2(14.0, 14.0)] {
            for i in 0..128 {
                let dir = V2::from_angle(i as f32 * TAU / 128.0);
                let end = pos + dir * ray_exit(pos, dir, lo, hi);
                assert!(end.x >= lo.x - 0.01 && end.x <= hi.x + 0.01);
                assert!(end.y >= lo.y - 0.01 && end.y <= hi.y + 0.01);
                let wall_distance = (end.x - lo.x)
                    .min(hi.x - end.x)
                    .min(end.y - lo.y)
                    .min(hi.y - end.y);
                assert!(wall_distance.abs() < 0.01);
            }
        }
    }
}

#[test]
fn lasers_hit_targets_before_walls_and_leave_targets_beyond_them() {
    for fading in [false, true] {
        let mut game = Game::new(7);
        let at = game.player.pos;
        let mut spin = Spin::new(1.0, false, 1.0, at, 0.0);
        spin.active = 0.001;
        if fading {
            spin.life = 0.001;
        }
        game.player.spin = Some(spin);
        for x in [WORLD_W - WALL - 10.0, WORLD_W + 60.0] {
            game.add_orb(v2(x, at.y), V2::default());
            game.add_enemy(Kind::Tank, v2(x, at.y), V2::default(), true);
        }
        let hp = game.enemies[1].hp;
        game.update_spin(0.001);
        assert_eq!(game.orbs.len(), 1);
        assert!(game.orbs[0].pos.x > WORLD_W);
        assert!(game.enemies[0].hp < hp);
        assert_eq!(game.enemies[1].hp, hp);
    }
}

#[test]
fn laser_wall_sparks_and_heat_wisps_fade_and_obey_the_particle_cap() {
    let mut game = Game::new(7);
    game.player.spin = Some(Spin::new(1.0, false, 1.0, game.player.pos, 0.0));
    game.player.spin.as_mut().unwrap().active = 0.01;
    game.update_spin(0.01);
    let wall_glows: Vec<_> = game.particles.iter().filter(|p| p.size == 2).collect();
    assert_eq!(wall_glows.len(), 8);
    for p in wall_glows {
        let edge = p
            .pos
            .x
            .min(p.pos.y)
            .min(WORLD_W - p.pos.x)
            .min(WORLD_H - p.pos.y);
        assert!((edge - WALL).abs() < 0.01);
        assert_eq!(p.hue, Hue::Orange);
    }
    assert!(
        game.particles
            .iter()
            .any(|p| p.size == 0 && p.hue == Hue::Orange)
    );
    game.player.spin = None;
    game.update_effects(0.2);
    assert_eq!(game.particles.len(), 8, "only wall afterglow remains");
    game.update_effects(0.3);
    assert!(game.particles.is_empty());
    game.player.spin = Some(Spin::new(1.0, false, 1.0, game.player.pos, 0.0));
    game.player.spin.as_mut().unwrap().active = 0.1;
    for _ in 0..10 {
        game.update_spin(0.1);
    }
    assert_eq!(game.particles.len(), MAX_PARTICLES);
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
        game.player.vel = v2(-300.0, 0.0);
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
        let velocity = v2(-300.0 * (-DRAG * 0.01).exp(), 0.0);
        assert_eq!(game.player.vel, velocity);
        assert_eq!(game.player.pos, at + velocity * 0.01);
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
    game.player.vel = v2(300.0, 0.0);
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
    assert_eq!(game.player.vel, v2(300.0 * (-DRAG * 0.02).exp(), 0.0));
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
            delay: 0.0,
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: BULLET_RANGE,
            damage: 1.0,
            pierce: 0,
            hit_ids: Vec::new(),
            exits: Vec::new(),
            homing: false,
            weapon: Weapon::Default,
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
            delay: 0.0,
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: BULLET_RANGE,
            damage: 1.0,
            pierce,
            hit_ids: Vec::new(),
            exits: Vec::new(),
            homing: false,
            weapon: Weapon::Default,
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
fn gatling_pellets_all_expire_after_their_barrage() {
    let view = View {
        scale: 1.0,
        size: v2(1600.0, 900.0),
    };
    for fps in [30, 60, 144] {
        for aim in [0.0, 0.3, 1.0, 2.0, 3.0] {
            for boosted in [false, true] {
                let mut game = Game::new(7);
                game.player.aim = aim;
                game.player.charge = 0.95;
                game.player.heat = if boosted { 0.95 } else { 0.5 };
                game.player.upgrades.add(Upgrade::Spread);
                game.player.upgrades.add(Upgrade::Spread);
                game.update_player(0.0, &Input::default(), &view);
                for _ in 0..fps * 4 {
                    game.update_gatling(1.0 / fps as f32);
                    game.update_bullets(1.0 / fps as f32);
                }
                assert!(game.gatling.is_none());
                assert!(
                    game.bullets.is_empty(),
                    "{} pellets stuck at {fps} fps, aim {aim}, boosted {boosted}: {:?}",
                    game.bullets.len(),
                    game.bullets
                        .first()
                        .map(|b| (b.pos, b.traveled, b.range, b.vel))
                );
            }
        }
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
            delay: 0.0,
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: if wall { BULLET_RANGE } else { 50.0 },
            damage: 1.0,
            pierce: 0,
            hit_ids: Vec::new(),
            exits: Vec::new(),
            homing: false,
            weapon: Weapon::Default,
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
            delay: 0.0,
            vel: v2(BULLET_SPEED, 0.0),
            traveled: 0.0,
            range: BULLET_RANGE,
            damage: 1.0,
            pierce: 0,
            hit_ids: Vec::new(),
            exits: Vec::new(),
            homing: false,
            weapon: Weapon::Default,
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
    for (seconds, multiplier, boosted) in [(0.2, 1.0, false), (0.55, 1.1, true), (0.7, 0.75, false)]
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
        if seconds < 0.6 {
            assert!(game.gatling.is_none(), "release is needed to fire");
            assert!((game.player.charge - seconds / 0.6).abs() < 1e-5);
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
        assert_eq!(game.player.overheated, seconds > 0.6);
        game.update_player(0.1, &held, &view);
        assert_eq!(game.player.charge, 0.0, "cannot charge during the barrage");
        assert!(game.bullets.is_empty());
        game.update_gatling(2.0);
        assert!(game.gatling.is_none());
        if seconds > 0.6 {
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
    for _ in 0..40 {
        game.update_player(1.0 / 60.0, &held, &view);
    }
    assert!(game.gatling.is_none());
    assert!((game.player.heat - (2.0 / 3.0) / 0.6 * 0.75).abs() < 1e-5);

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
    assert_eq!(game.bullets.len(), 6, "each volley is a six-pellet spray");
    assert!(game.bullets[0].vel.y < 0.0 && game.bullets[5].vel.y > 0.0);
    assert_ne!(
        game.bullets[0].vel, game.bullets[1].vel,
        "paired pellets fan apart"
    );
    let first = game.bullets[2].damage;
    let origin = game.bullets[2].pos;
    assert!(game.player.vel.x < 0.0, "barrage kicks the ship backward");
    game.player.pos += v2(0.0, 100.0);
    game.player.aim = TAU / 4.0;
    game.update_gatling(0.025);
    assert_eq!(game.bullets.len(), 12);
    assert!(game.bullets[8].vel.y > 0.0 && game.bullets[8].pos.y > origin.y);
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
    assert!(counts.iter().all(|&n| n == counts[0] && n > 300));
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
    assert_eq!(game.bullets.len(), 10);
    assert!(
        game.bullets
            .iter()
            .all(|b| b.pierce == 1 && !b.falloff && b.range > 1000.0)
    );
    assert!(game.bullets[0].damage < game.bullets[2].damage);
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
        assert_eq!(
            if boss_target {
                game.boss.as_ref().unwrap().stun
            } else {
                game.enemies[0].stun
            },
            0.0
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
    game.player.weapon = Weapon::Sniper;
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
    assert_eq!(game.boss.as_ref().unwrap().stun, 0.0);
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
        delay: 0.0,
        vel: v2(BULLET_SPEED, 0.0),
        traveled: 0.0,
        range: BULLET_RANGE,
        damage: 1.0,
        pierce: 1,
        hit_ids: Vec::new(),
        exits: Vec::new(),
        homing: false,
        weapon: Weapon::Default,
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
    for thrust in [1.0, -1.0] {
        game.player.thrust = thrust;
        draw(&game);
    }
    game.player.thrust = 0.0;
    game.player.weapon = Weapon::Plasma;
    game.update_player(
        0.0,
        &Input {
            fire: true,
            ..Input::default()
        },
        &view,
    );
    draw(&game);
    game.bullets.last_mut().unwrap().traveled = 1000.0;
    draw(&game);
    for charge in [0.1, 0.5, 0.95] {
        game.player.charge = charge;
        game.player.heat = charge;
        draw(&game);
        game.update_player(0.0, &Input::default(), &view);
        game.update_bfgs(0.1);
        draw(&game);
    }
    game.update_bfgs(10.0);
    draw(&game);
    game.bullets.clear();
    game.player.weapon = Weapon::Rocket;
    game.player.charge = 0.95;
    game.player.heat = 0.95;
    game.update_player(0.0, &Input::default(), &view);
    draw(&game);
    game.rocket_blast(game.player.pos + v2(300.0, 0.0), 22.0);
    draw(&game);
    game.bullets.clear();
    game.player.weapon = Weapon::Default;
    game.player.heat = 0.0;
    for e in &mut game.enemies {
        e.stagger(6.0);
    }
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
    game.player.weapon = Weapon::Sniper;
    game.player.charge = 0.7;
    game.update_player(0.0, &Input::default(), &view);
    game.update_gatling(0.1);
    draw(&game);
    game.update_gatling(2.0);
    game.player.fire_cd = 0.0;
    game.player.heat = 0.0;
    game.player.overheated = false;
    game.update_player(
        0.0,
        &Input {
            fire: true,
            ..Input::default()
        },
        &view,
    );
    draw(&game);
    game.bullets.last_mut().unwrap().hit_ids.push(999);
    draw(&game);
    game.player.weapon = Weapon::Default;
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
