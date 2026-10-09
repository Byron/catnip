//! Game behavior tests: combat math, scaling, boss and upgrade flow.

use super::*;

#[test]
fn fast_bullet_path_hits_enemy_between_frames() {
    // A 25-unit step jumps straight over a radius-10 enemy; the swept test must still catch it.
    assert!(segment_distance(v2(12.0, 0.0), v2(0.0, 0.0), v2(25.0, 0.0)) < 10.0);
    assert!(segment_distance(v2(12.0, 30.0), v2(0.0, 0.0), v2(25.0, 0.0)) > 10.0);
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
    let (_, overheated) = run(&mut game, &input, 1.5);
    assert!(overheated, "should overheat within 3 s of continuous fire");
    let (heat, overheated) = run(&mut game, &released, 1.5);
    assert!(
        !overheated && heat <= HEAT_RESUME,
        "gun unlocks after cooling: {heat}"
    );
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
    game.dev.god = true;
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
