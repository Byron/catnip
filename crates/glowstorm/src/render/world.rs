//! Drawing the arena: every entity is a placement of the sprite atlas.

use super::*;

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
        let frames = if e.flash > 0.0 || blink {
            &set.flash
        } else {
            &set.normal
        };
        let shudder = if e.winding_up() {
            V2 {
                x: (game.time * 90.0 + e.phase).sin() * 2.5,
                y: 0.0,
            }
        } else {
            V2::default()
        };
        let k = match e.kind {
            Kind::Shooter => art::angle_frame(e.facing, art::SHOOTER_ANGLES, TAU),
            Kind::Tank => art::angle_frame(e.spin, art::SPIN_FRAMES, TAU / 6.0),
            _ => art::angle_frame(e.spin, art::SPIN_FRAMES, TAU / 4.0),
        };
        draw.sprite(frames[k], e.pos + shudder, Z_ENEMIES);
        if e.enraged {
            // A red ring pulsing outward, three times a second.
            let frames = &sprites.ring[Hue::Red.index()];
            let k = ((game.time * 3.0 + e.phase).fract() * frames.len() as f32) as usize;
            draw.sprite(frames[k.min(frames.len() - 1)], e.pos, Z_ENEMIES - 2);
        }
        if e.elite {
            let aura = &sprites.aura[usize::from(e.kind == Kind::Tank)];
            let k = art::angle_frame(game.time * 1.5 + e.phase, art::AURA_FRAMES, TAU / 10.0);
            draw.sprite(aura[k], e.pos, Z_ENEMIES - 1);
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
        let frames = if b.traveled > b.range * 0.55 {
            &sprites.bullet_faded
        } else {
            &sprites.bullet
        };
        draw.sprite(frames[k], b.pos, Z_BULLETS);
    }

    let p = &game.player;
    let blink = p.invuln > 0.0 && (p.invuln * 14.0) as i32 % 2 == 1;
    if matches!(game.state, State::Playing | State::Paused | State::Upgrade) && !blink {
        let k = art::angle_frame(p.aim, art::PLAYER_ANGLES, TAU);
        draw.sprite(sprites.player[k], p.pos, Z_PLAYER);
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
    let blink = b.windup > 0.0 && (game.time * 16.0) as i32 % 2 == 0;
    let white = b.flash > 0.0 || blink;
    let (shell, core) = if white {
        (&sprites.boss_shell.flash, &sprites.boss_core.flash)
    } else {
        (&sprites.boss_shell.normal, &sprites.boss_core.normal)
    };
    let k = art::angle_frame(b.spin, art::BOSS_SHELL_FRAMES, TAU / 8.0);
    draw.sprite(shell[k], b.pos, Z_BOSS);
    let k = art::angle_frame(b.core_spin, art::BOSS_CORE_FRAMES, TAU / 3.0);
    draw.sprite(core[k], b.pos, Z_BOSS + 1);
    // Charge tell: a blinking dotted line along the dash path.
    if b.charging() && (game.time * 12.0) as i32 % 2 == 0 {
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
