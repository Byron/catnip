//! Drawing the arena: every entity is a placement of the sprite atlas.

use super::*;
use crate::game::{CHARGE_BOOST_HEAT, SPIN_RADIUS, Weapon, ray_exit, v2};

/// Everything in the arena, back to front; the floor is placed by the caller.
pub(super) fn draw_world(draw: &mut Draw, game: &Game, sprites: &Sprites) {
    for m in &game.motes {
        draw.sprite(sprites.mote[m.hue], m.pos, Z_MOTES);
    }

    for ring in &game.rings {
        if ring.t < 0.0 {
            continue;
        }
        let frames = &sprites.ring[ring.hue.index()];
        let progress = (ring.t / ring.dur).clamp(0.0, 0.999);
        let k = (progress * frames.len() as f32) as usize;
        let k = if ring.inward { frames.len() - 1 - k } else { k };
        draw.sprite(frames[k], ring.pos, Z_RINGS);
    }

    for pickup in &game.pickups {
        // Blink during the last two seconds.
        if pickup.life < 2.0 && (pickup.life * 10.0) as i32 % 2 == 0 {
            continue;
        }
        let pulse = ((game.time * 6.0).sin() > 0.0) as usize;
        let frame = match pickup.kind {
            PickupKind::Heart => sprites.heart[pulse],
            PickupKind::Rapid => sprites.bolt[pulse],
        };
        draw.sprite(frame, pickup.pos, Z_PICKUPS);
    }

    for ghost in &game.ghosts {
        let k = art::angle_frame(ghost.angle, art::PLAYER_ANGLES, TAU);
        draw.sprite(sprites.ghost[k], ghost.pos, Z_GHOSTS);
    }

    for e in &game.enemies {
        let set = match e.kind {
            Kind::Chaser => &sprites.chaser,
            Kind::Swarm => &sprites.swarm,
            Kind::Shooter => &sprites.shooter,
            Kind::Tank => &sprites.tank,
        };
        // Wind-ups blink white and shudder: the tell before an attack.
        let blink = e.winding_up() && (game.time * 18.0) as i32 % 2 == 0;
        let white = e.flash > 0.0 || blink;
        let shudder = if e.winding_up() {
            V2 {
                x: (game.time * 90.0 + e.phase).sin() * 2.5,
                y: 0.0,
            }
        } else {
            V2::default()
        };
        let angle = if e.kind.spec().faces_player {
            e.facing
        } else {
            e.spin
        };
        draw.sprite(set.at(angle, white), e.pos + shudder, Z_ENEMIES);
        if e.stun > 0.0 {
            for i in 0..3 {
                let at = e.pos
                    + V2::from_angle(game.time * 5.0 + e.phase + i as f32 * TAU / 3.0)
                        * (e.kind.radius() + 8.0);
                draw.sprite(sprites.spark[Hue::Yellow.index()][1], at, Z_ENEMIES + 1);
            }
        }
        if e.enraged {
            // A red ring pulsing outward, three times a second.
            let frames = &sprites.ring[Hue::Red.index()];
            let k = ((game.time * 3.0 + e.phase).fract() * frames.len() as f32) as usize;
            draw.sprite(frames[k.min(frames.len() - 1)], e.pos, Z_ENEMIES - 2);
        }
        if e.elite {
            let aura = &sprites.aura[usize::from(e.kind.radius() > 25.0)];
            draw.sprite(
                aura.at(game.time * 1.5 + e.phase, false),
                e.pos,
                Z_ENEMIES - 1,
            );
        }
    }

    if let Some(b) = &game.boss {
        draw_boss(draw, sprites, game, b);
    }

    // Shockwaves: a ring of sparks, thinning as it expands.
    for w in game.shockwaves.iter().filter(|w| w.r > 0.0) {
        let count = 44;
        let size = if w.r < SHOCK_RADIUS * 0.6 { 2 } else { 1 };
        for i in 0..count {
            let a = i as f32 * TAU / count as f32 + w.r * 0.004;
            let at = w.pos
                + V2 {
                    x: a.cos() * w.r,
                    y: a.sin() * w.r,
                };
            draw.sprite(sprites.spark[Hue::Green.index()][size], at, Z_PARTICLES);
        }
    }

    for b in &game.bullets {
        let k = art::angle_frame(b.vel.angle(), art::BULLET_ANGLES, TAU);
        // Dimmer once damage has mostly fallen off.
        let frames = if b.weapon == Weapon::Rocket {
            &sprites.rocket
        } else if b.weapon == Weapon::Sniper {
            &sprites.sniper[usize::from(b.faded())]
        } else if b.faded() {
            &sprites.bullet_faded
        } else {
            &sprites.bullet
        };
        draw.sprite(frames[k], b.pos, Z_BULLETS);
        if b.weapon == Weapon::Rocket {
            for i in 1..=4 {
                let at = b.pos - b.vel.norm() * (15.0 + i as f32 * 6.0);
                draw.sprite(
                    sprites.spark[Hue::Orange.index()][usize::from(i < 3)],
                    at,
                    Z_BULLETS - 1,
                );
            }
        }
    }

    if let Some(gun) = &game.gatling {
        let frames = &sprites.charged[usize::from(gun.boosted)];
        let k = (gun.charge * gun.strength() * (frames.len() - 1) as f32).round() as usize;
        draw.sprite(
            frames[k],
            game.player.pos + V2::from_angle(game.player.aim) * 28.0,
            Z_PLAYER + 1,
        );
    }

    let p = &game.player;
    if p.thrust != 0.0 && matches!(game.state, State::Playing | State::Paused) {
        let forward = V2::from_angle(p.aim);
        let exhaust = forward * -p.thrust.signum();
        let count = if p.thrust > 0.0 { 1 } else { 2 };
        for jet in 0..count {
            let nozzle = p.pos
                + exhaust * 18.0
                + forward.perp()
                    * if count == 1 {
                        0.0
                    } else {
                        (jet as f32 * 2.0 - 1.0) * 9.0
                    };
            for i in 0..8 {
                let flicker = 0.8 + 0.2 * (game.time * 60.0 + i as f32 + jet as f32).sin();
                let length = if count == 1 { 6.0 } else { 3.5 } * p.thrust.abs();
                let at = nozzle + exhaust * ((i as f32 + 1.0) * length * flicker);
                let hue = if i < 2 {
                    Hue::White
                } else if i < 5 {
                    Hue::Yellow
                } else {
                    Hue::Orange
                };
                draw.sprite(
                    sprites.spark[hue.index()][if i < 3 { 2 } else { 1 }],
                    at,
                    Z_PLAYER - 1,
                );
            }
        }
    }
    if p.charge > 0.0 && p.charge_spin {
        for i in 0..12 {
            let at = p.pos
                + V2::from_angle(game.time * 4.0 + i as f32 * TAU / 12.0)
                    * (24.0 + p.charge * 30.0);
            draw.sprite(sprites.spark[Hue::Orange.index()][1], at, Z_PLAYER);
        }
    }
    if p.charge > 0.0 && matches!(game.state, State::Playing | State::Paused) {
        let boosted = p.heat >= CHARGE_BOOST_HEAT;
        let frames = &sprites.charged[usize::from(boosted)];
        let k = (p.charge * (frames.len() - 1) as f32).round() as usize;
        draw.sprite(
            frames[k.min(frames.len() - 1)],
            p.pos + V2::from_angle(p.aim) * 28.0,
            Z_PLAYER + 1,
        );
        // Sparks converge on the muzzle as the gun charges.
        let muzzle = p.pos + V2::from_angle(p.aim) * 28.0;
        for i in 0..12 {
            let t = (game.time * 2.0 + i as f32 / 12.0).fract();
            let at = muzzle
                + V2::from_angle(i as f32 * TAU / 12.0 + game.time)
                    * (18.0 + (1.0 - t) * (25.0 + 65.0 * p.charge));
            draw.sprite(sprites.spark[Hue::Cyan.index()][1], at, Z_PLAYER + 1);
        }
        if boosted {
            draw_world_text(
                draw,
                sprites,
                TextSize::Tiny,
                Hue::White,
                "RELEASE!",
                p.pos + V2 { x: 0.0, y: -48.0 },
            );
        }
    }
    let blink = p.invuln > 0.0 && (p.invuln * 14.0) as i32 % 2 == 1;
    if matches!(game.state, State::Playing | State::Paused | State::Upgrade) && !blink {
        let k = art::angle_frame(p.angle(), art::PLAYER_ANGLES, TAU);
        draw.sprite(sprites.player[k], p.pos, Z_PLAYER);
    }
    if let Some(spin) = &p.spin {
        let width = if spin.width() > 4.5 {
            4
        } else if spin.width() > 2.5 {
            3
        } else if spin.width() > 1.5 {
            2
        } else if spin.width() > 0.75 {
            1
        } else {
            0
        };
        for i in 0..8 {
            let angle = spin.laser_angle() + i as f32 * TAU / 8.0;
            let dir = V2::from_angle(angle);
            let frame = sprites.laser[width][art::angle_frame(angle, 128, TAU)];
            let lo = draw.cam * (1.0 / draw.scale) - p.pos;
            let hi = lo + draw.view_size;
            let margin = v2(art::WALL, art::WALL);
            let wall = ray_exit(p.pos, dir, margin, game.arena_size - margin);
            let (mut enter, mut exit) = (0.0f32, wall - 8.0);
            for (dir, lo, hi) in [(dir.x, lo.x, hi.x), (dir.y, lo.y, hi.y)] {
                if dir.abs() < 1e-6 {
                    if lo > 0.0 || hi < 0.0 {
                        exit = -1.0;
                    }
                } else {
                    let (a, b) = (lo / dir, hi / dir);
                    enter = enter.max(a.min(b));
                    exit = exit.min(a.max(b));
                }
            }
            let n = if exit >= enter {
                ((exit - enter) / 12.0).ceil() as usize + 1
            } else {
                0
            };
            for j in 0..n {
                let distance = (enter + j as f32 * 12.0).min(exit);
                draw.sprite(frame, p.pos + dir * distance, Z_PLAYER - 1);
            }
        }
        for i in 0..20 {
            let at = p.pos + V2::from_angle(spin.angle + i as f32 * TAU / 20.0) * SPIN_RADIUS;
            draw.sprite(sprites.spark[Hue::Yellow.index()][1], at, Z_PLAYER + 1);
        }
    }
    if matches!(game.state, State::Playing | State::Paused | State::Upgrade) {
        for at in p.shield_orbs(game.time) {
            draw.sprite(sprites.shield, at, Z_PLAYER);
        }
    }
    if game.state == State::Playing && (p.heat > 0.01 || p.overheated) {
        draw_heat_bar(draw, sprites, game);
    }

    for orb in &game.orbs {
        let frame = if orb.heavy {
            sprites.heavy_orb
        } else {
            sprites.orb
        };
        draw.sprite(frame, orb.pos, Z_ORBS);
    }

    for particle in &game.particles {
        // Kitty has no per-placement opacity, so particles fade by shrinking.
        let left = particle.life / particle.max;
        let size = if left > 0.6 {
            particle.size
        } else {
            particle.size.saturating_sub(1)
        };
        let size = if left < 0.25 { 0 } else { size };
        draw.sprite(
            sprites.spark[particle.hue.index()][size],
            particle.pos,
            Z_PARTICLES,
        );
    }

    for p in &game.popups {
        // Combo callouts are big and yellow; kill scores are small and white.
        let (size, hue) = if p.combo {
            (TextSize::Small, Hue::Yellow)
        } else {
            (TextSize::Tiny, Hue::White)
        };
        // Shrink-free fade: blink out over the last fifth of the life.
        if p.t > POPUP_TIME * 0.8 && (p.t * 30.0) as i32 % 2 == 0 {
            continue;
        }
        draw_world_text(draw, sprites, size, hue, &p.text, p.pos);
    }
}

pub(super) fn draw_boss(draw: &mut Draw, sprites: &Sprites, game: &Game, b: &crate::game::Boss) {
    let blink = b.stun <= 0.0 && b.windup > 0.0 && (game.time * 16.0) as i32 % 2 == 0;
    let white = b.flash > 0.0 || blink;
    draw.sprite(sprites.boss_shell.at(b.spin, white), b.pos, Z_BOSS);
    draw.sprite(sprites.boss_core.at(b.core_spin, white), b.pos, Z_BOSS + 1);
    // Charge tell: a blinking dotted line along the dash path.
    if b.stun <= 0.0 && b.charging() && (game.time * 12.0) as i32 % 2 == 0 {
        for i in 1..18 {
            let at = b.pos + b.charge_dir * (i as f32 * 40.0 + 40.0);
            draw.sprite(sprites.spark[Hue::Violet.index()][1], at, Z_PARTICLES);
        }
    }
}

/// Neon sprite text centered on a world position.
pub(super) fn draw_world_text(
    draw: &mut Draw,
    sprites: &Sprites,
    size: TextSize,
    hue: Hue,
    text: &str,
    at: V2,
) {
    let font = sprites.font(size, hue);
    let center = at * draw.scale - draw.cam;
    let mut x = center.x - font.width(text) / 2.0;
    let top = center.y - font.height / 2.0;
    for c in text.chars() {
        if let Some(frame) = font.glyph(c) {
            let pos = (
                (x - font.pad).round() as i32,
                (top - font.pad).round() as i32,
            );
            draw.placer.place(draw.out, frame, pos, Z_POPUPS);
        }
        x += font.advance;
    }
}

/// Ten segments under the ship; they blink red while the gun is locked.
pub(super) fn draw_heat_bar(draw: &mut Draw, sprites: &Sprites, game: &Game) {
    const SEGMENTS: usize = 10;
    const SPACING: f32 = 10.0;
    let p = &game.player;
    let lit = (p.heat * SEGMENTS as f32).ceil() as usize;
    let hot = if p.overheated {
        3
    } else {
        match p.heat {
            h if h < 0.5 => 0,
            h if h < 0.75 => 1,
            h if h < 0.9 => 2,
            _ => 3,
        }
    };
    let blink_off = p.overheated && (game.time * 8.0) as i32 % 2 == 0;
    let left = p.pos.x - SPACING * (SEGMENTS as f32 - 1.0) / 2.0;
    for i in 0..SEGMENTS {
        let frame = if i < lit && !blink_off {
            sprites.heat[hot]
        } else {
            sprites.heat_off
        };
        draw.sprite(
            frame,
            V2 {
                x: left + i as f32 * SPACING,
                y: p.pos.y + 34.0,
            },
            Z_PLAYER,
        );
    }
}

pub(super) struct Draw<'a> {
    pub(super) out: &'a mut Vec<u8>,
    pub(super) placer: &'a mut AtlasPlacer,
    pub(super) scale: f32,
    pub(super) cam: V2,
    pub(super) view_size: V2,
}

impl Draw<'_> {
    /// Places an atlas frame centered on a world position.
    pub(super) fn sprite(&mut self, frame: Frame, world: V2, z: i32) {
        let x = world.x * self.scale - self.cam.x - frame.rect.w as f32 / 2.0;
        let y = world.y * self.scale - self.cam.y - frame.rect.h as f32 / 2.0;
        self.placer
            .place(self.out, frame, (x.round() as i32, y.round() as i32), z);
    }
}
