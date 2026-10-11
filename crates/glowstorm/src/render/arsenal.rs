//! Distinct silhouettes and field effects for the six extra weapon slots.

use super::*;
use crate::game::Weapon;

pub(super) fn charge_hue(weapon: Weapon) -> Hue {
    match weapon {
        Weapon::Gravity => Hue::Violet,
        Weapon::Ricochet | Weapon::Lightning => Hue::Yellow,
        Weapon::Mines | Weapon::Flame => Hue::Orange,
        _ => Hue::Cyan,
    }
}

fn circle(draw: &mut Draw, sprites: &Sprites, pos: V2, radius: f32, angle: f32, hue: Hue, z: i32) {
    let count = (radius / 8.0).ceil().clamp(10.0, 24.0) as usize;
    for i in 0..count {
        let at = pos + V2::from_angle(angle + i as f32 * TAU / count as f32) * radius;
        draw.sprite(sprites.spark[hue.index()][1], at, z);
    }
}

fn blade(draw: &mut Draw, sprites: &Sprites, pos: V2, radius: f32, angle: f32) {
    let size = (((radius - 22.0) / 5.0).round() as usize).min(sprites.blades.len() - 1);
    let frames = &sprites.blades[size];
    draw.sprite(
        frames[art::angle_frame(angle, frames.len(), TAU)],
        pos,
        Z_BULLETS,
    );
}

fn ricochet(draw: &mut Draw, sprites: &Sprites, pos: V2, radius: f32) {
    let size = (((radius - 7.0) / 5.0).round() as usize).min(sprites.ricochet.len() - 1);
    draw.sprite(sprites.ricochet[size], pos, Z_BULLETS);
}

fn fuel(draw: &mut Draw, sprites: &Sprites, pos: V2, radius: f32) {
    let size = (((radius - 10.0) / 5.0).round() as usize).min(sprites.fuel.len() - 1);
    draw.sprite(sprites.fuel[size], pos, Z_BULLETS);
}

pub(super) fn draw_arsenal(draw: &mut Draw, game: &Game, sprites: &Sprites) {
    for field in &game.gravity_fields {
        circle(
            draw,
            sprites,
            field.pos,
            field.radius,
            game.time,
            Hue::Violet,
            Z_RINGS,
        );
        for i in 0..12 {
            let t = (game.time * 2.0 + i as f32 / 12.0).fract();
            let radius = field.radius * if field.gathering { 1.0 - t } else { t };
            let at = field.pos + V2::from_angle(i as f32 * TAU / 12.0 + game.time * 3.0) * radius;
            draw.sprite(sprites.spark[Hue::Violet.index()][2], at, Z_BULLETS);
        }
        for i in 0..field.captured.min(12) {
            let at = field.pos + V2::from_angle(game.time * 5.0 + i as f32 * 2.4) * 18.0;
            draw.sprite(sprites.spark[Hue::White.index()][1], at, Z_BULLETS + 1);
        }
    }
    // Bound placements even when several lightning chains overlap.
    for arc in game.lightning_arcs.iter().rev().take(18) {
        let delta = arc.to - arc.from;
        let normal = delta.norm().perp();
        let steps = ((delta.len() / 9.0).ceil() as usize).clamp(2, 32);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let jitter =
                (i as f32 * 4.7 + game.time * 70.0).sin() * 7.0 * (t * std::f32::consts::PI).sin();
            let at = arc.from + delta * t + normal * jitter;
            let hue = if i % 3 == 0 { Hue::White } else { Hue::Yellow };
            draw.sprite(
                sprites.spark[hue.index()][usize::from(arc.life > 0.06)
                    + usize::from(arc.life > 0.06 && arc.power > 1.5)],
                at,
                Z_PARTICLES,
            );
        }
    }
    if let Some(storm) = &game.storm {
        circle(
            draw,
            sprites,
            game.player.pos,
            (35.0 + storm.charge * 20.0)
                * (0.5 + 0.5 * (storm.life / storm.duration).clamp(0.0, 1.0)),
            game.time * 8.0,
            Hue::Yellow,
            Z_PLAYER + 1,
        );
    }
    for round in &game.ricochets {
        ricochet(draw, sprites, round.pos, round.radius);
        if round.charged {
            draw.sprite(
                sprites.spark[Hue::White.index()][2],
                round.pos,
                Z_BULLETS + 1,
            );
        }
        let dir = round.vel.norm();
        for i in 1..=3 {
            draw.sprite(
                sprites.spark[Hue::Yellow.index()][0],
                round.pos - dir * (round.radius + i as f32 * 9.0),
                Z_BULLETS - 1,
            );
        }
    }
    for round in &game.blades {
        blade(draw, sprites, round.pos, round.radius, round.angle);
        if round.returning || round.charged {
            draw.sprite(
                sprites.spark[Hue::White.index()][2],
                round.pos,
                Z_BULLETS + 1,
            );
        }
    }
    for mine in &game.mines {
        draw.sprite(sprites.mine, mine.pos, Z_BULLETS);
        if mine.fuse.is_some() && (game.time * 18.0).sin() > 0.0 {
            draw.sprite(sprites.spark[Hue::Red.index()][2], mine.pos, Z_BULLETS + 1);
        }
    }
    for glob in &game.fuel_globs {
        fuel(draw, sprites, glob.pos, glob.radius);
        for i in 1..=4 {
            draw.sprite(
                sprites.spark[Hue::Orange.index()][1],
                glob.pos - glob.vel.norm() * (glob.radius + i as f32 * 8.0),
                Z_BULLETS - 1,
            );
        }
    }
    for patch in &game.fire_patches {
        let strength = (patch.life / patch.duration).clamp(0.0, 1.0);
        for i in 0..24 {
            let angle = i as f32 * 2.4 + game.time;
            let radius = patch.radius * ((i as f32 + 0.5) / 24.0).sqrt();
            let at = patch.pos + V2::from_angle(angle) * radius;
            let hue = if i % 4 == 0 { Hue::Yellow } else { Hue::Orange };
            draw.sprite(
                sprites.spark[hue.index()][if strength > 0.3 { 2 } else { 1 }],
                at,
                Z_RINGS,
            );
        }
    }
}

pub(super) fn draw_charge(draw: &mut Draw, game: &Game, sprites: &Sprites, muzzle: V2) -> bool {
    let p = &game.player;
    if p.charge_spin {
        return false;
    }
    match p.weapon {
        Weapon::Gravity => {
            circle(
                draw,
                sprites,
                muzzle,
                12.0 + 30.0 * p.charge,
                game.time * 5.0,
                Hue::Violet,
                Z_PLAYER + 1,
            );
            draw.sprite(sprites.spark[Hue::White.index()][2], muzzle, Z_PLAYER + 1);
        }
        Weapon::Ricochet => ricochet(draw, sprites, muzzle, 12.0 + 20.0 * p.charge),
        Weapon::Lightning => circle(
            draw,
            sprites,
            muzzle,
            10.0 + 25.0 * p.charge,
            game.time * 12.0,
            Hue::Yellow,
            Z_PLAYER + 1,
        ),
        Weapon::Blade => blade(
            draw,
            sprites,
            muzzle,
            30.0 + 30.0 * p.charge,
            game.time * 15.0,
        ),
        Weapon::Mines => {
            for i in 0..3 {
                draw.sprite(
                    sprites.mine,
                    muzzle
                        + V2::from_angle(game.time * 3.0 + i as f32 * TAU / 3.0)
                            * (10.0 + 15.0 * p.charge),
                    Z_PLAYER + 1,
                );
            }
        }
        Weapon::Flame => fuel(draw, sprites, muzzle, 15.0 + 25.0 * p.charge),
        _ => return false,
    }
    true
}
