//! Turns a Game into one kitty frame: the floor scrolls by changing its
//! source rect, every sprite is a placement of the atlas, text goes on top.

use std::{f32::consts::TAU, fmt::Write as _, io::Write};

use catnip::{
    gfx::Frame,
    kitty::{self, Placer, Rect},
    term::Window,
};

use crate::{
    art::{self, Sprites},
    game::{Game, Kind, MAX_HP, PickupKind, State, V2},
};

pub const ATLAS_ID: u32 = 10;
pub const FLOOR_ID: u32 = 11;
/// 1×1 translucent black pixel, stretched behind the HUD row.
pub const SHADE_ID: u32 = 12;

// Negative z keeps images below the text layer (HUD, banners).
const Z_FLOOR: i32 = -1_000_000;
const Z_RINGS: i32 = -900;
const Z_PICKUPS: i32 = -800;
const Z_GHOSTS: i32 = -700;
const Z_ENEMIES: i32 = -600;
const Z_BULLETS: i32 = -550;
const Z_PLAYER: i32 = -400;
const Z_ORBS: i32 = -350;
const Z_PARTICLES: i32 = -300;
const Z_SHADE: i32 = -100;

pub struct Stats {
    pub fps: f32,
    pub frame_ms: f32,
    pub kb_per_frame: f32,
}

pub struct Renderer {
    atlas: Placer,
    floor: Placer,
    floor_size: (u32, u32),
    text_rows: Vec<u32>,
    pub sprites_drawn: u32,
}

impl Renderer {
    pub fn new(floor_size: (u32, u32)) -> Self {
        Self {
            atlas: Placer::new(ATLAS_ID),
            floor: Placer::new(FLOOR_ID),
            floor_size,
            text_rows: Vec::new(),
            sprites_drawn: 0,
        }
    }

    pub fn frame(
        &mut self,
        out: &mut Vec<u8>,
        game: &Game,
        sprites: &Sprites,
        scale: f32,
        win: Window,
        stats: &Stats,
    ) {
        out.extend_from_slice(b"\x1b[?2026h");
        let cell = win.cell();
        self.atlas.begin((win.width, win.height), cell);
        self.floor.begin((win.width, win.height), cell);
        let cam = (game.cam + game.shake_offset) * scale;
        let mut draw = Draw {
            out: &mut *out,
            placer: &mut self.atlas,
            scale,
            cam,
        };

        // The whole arena is one uploaded image; scrolling it is one placement.
        let (fw, fh) = self.floor_size;
        self.floor.place(
            draw.out,
            Rect {
                x: 0,
                y: 0,
                w: fw,
                h: fh,
            },
            (-cam.x.round() as i32, -cam.y.round() as i32),
            Z_FLOOR,
        );

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
            let frames = if e.flash > 0.0 {
                &set.flash
            } else {
                &set.normal
            };
            let k = match e.kind {
                Kind::Shooter => art::angle_frame(e.facing, art::SHOOTER_ANGLES, TAU),
                Kind::Tank => art::angle_frame(e.spin, art::SPIN_FRAMES, TAU / 6.0),
                _ => art::angle_frame(e.spin, art::SPIN_FRAMES, TAU / 4.0),
            };
            draw.sprite(frames[k], e.pos, Z_ENEMIES);
        }

        for b in &game.bullets {
            let k = art::angle_frame(b.vel.angle(), art::BULLET_ANGLES, TAU);
            draw.sprite(sprites.bullet[k], b.pos, Z_BULLETS);
        }

        let p = &game.player;
        let blink = p.invuln > 0.0 && (p.invuln * 14.0) as i32 % 2 == 1;
        if matches!(game.state, State::Playing | State::Paused) && !blink {
            let k = art::angle_frame(p.aim, art::PLAYER_ANGLES, TAU);
            draw.sprite(sprites.player[k], p.pos, Z_PLAYER);
        }

        for orb in &game.orbs {
            draw.sprite(sprites.orb, orb.pos, Z_ORBS);
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

        self.sprites_drawn = self.atlas.count();
        self.atlas.finish(out);
        self.floor.finish(out);
        self.text(out, game, win, stats);
        out.extend_from_slice(b"\x1b[?2026l");
    }

    fn text(&mut self, out: &mut Vec<u8>, game: &Game, win: Window, stats: &Stats) {
        for row in self.text_rows.drain(..) {
            let _ = write!(out, "\x1b[{row};1H\x1b[2K");
        }
        let mut text = Text {
            out,
            rows: &mut self.text_rows,
            win,
        };

        if matches!(game.state, State::Playing | State::Paused | State::Dying) {
            let _ = write!(
                text.out,
                "\x1b[1;1H\x1b_Ga=p,i={SHADE_ID},p=1,c={},r=1,z={Z_SHADE},C=1,q=2\x1b\\",
                win.cols
            );
            text.hud(game, stats, self.sprites_drawn);
        } else {
            kitty::delete_placement(text.out, SHADE_ID, 1);
        }
        let row = |fraction: f32| ((win.rows as f32 * fraction) as u32).max(2);
        match game.state {
            State::Title => {
                let scale = (win.cols / 12).clamp(1, 5);
                text.big(row(0.22), scale, "1;38;2;61;242;255", "GLOWSTORM");
                let r = row(0.22) + scale + 2;
                text.line(
                    r,
                    "38;2;200;210;255",
                    "WASD move · mouse aim · hold click fire · space / right-click dash",
                );
                text.line(
                    r + 1,
                    "38;2;140;150;190",
                    "no mouse? arrow keys aim and fire",
                );
                text.line(r + 3, "1;38;2;255;225;77", "click or press Enter to start");
                if game.best > 0 {
                    text.line(
                        r + 5,
                        "38;2;140;150;190",
                        &format!("best {}", group(game.best)),
                    );
                }
                text.line(win.rows, "38;2;90;100;140", "Esc pause · Q quit");
            }
            State::Playing if game.banner > 0.0 => {
                text.big(
                    row(0.2),
                    3,
                    "1;38;2;255;225;77",
                    &format!("WAVE {}", game.wave),
                );
            }
            State::Paused => {
                text.big(row(0.3), 4, "1;38;2;61;242;255", "PAUSED");
                text.line(
                    row(0.3) + 5,
                    "38;2;200;210;255",
                    "Esc or click to resume · Q to quit",
                );
            }
            State::GameOver => {
                let scale = (win.cols / 14).clamp(1, 4);
                text.big(row(0.22), scale, "1;38;2;255;59;78", "GAME OVER");
                let r = row(0.22) + scale + 1;
                text.big(r, 2, "1;38;2;255;255;255", &group(game.score));
                let best = if game.new_best {
                    "NEW BEST!".to_string()
                } else {
                    format!("best {}", group(game.best))
                };
                text.line(r + 3, "1;38;2;255;225;77", &best);
                text.line(
                    r + 4,
                    "38;2;200;210;255",
                    &format!("reached wave {}", game.wave),
                );
                text.line(r + 6, "38;2;140;150;190", "click or R to restart · Q quit");
            }
            _ => {}
        }
    }
}

struct Draw<'a> {
    out: &'a mut Vec<u8>,
    placer: &'a mut Placer,
    scale: f32,
    cam: V2,
}

impl Draw<'_> {
    /// Places an atlas frame centered on a world position.
    fn sprite(&mut self, frame: Frame, world: V2, z: i32) {
        let x = world.x * self.scale - self.cam.x - frame.rect.w as f32 / 2.0;
        let y = world.y * self.scale - self.cam.y - frame.rect.h as f32 / 2.0;
        self.placer.place(
            self.out,
            frame.rect,
            (x.round() as i32, y.round() as i32),
            z,
        );
    }
}

struct Text<'a> {
    out: &'a mut Vec<u8>,
    rows: &'a mut Vec<u32>,
    win: Window,
}

impl Text<'_> {
    fn centered_col(&self, width: u32) -> u32 {
        self.win.cols.saturating_sub(width) / 2 + 1
    }

    fn line(&mut self, row: u32, sgr: &str, s: &str) {
        let col = self.centered_col(s.chars().count() as u32);
        let _ = write!(self.out, "\x1b[{row};{col}H\x1b[{sgr}m{s}\x1b[0m");
        self.rows.push(row);
    }

    fn big(&mut self, row: u32, scale: u32, sgr: &str, s: &str) {
        let col = self.centered_col(s.chars().count() as u32 * scale);
        kitty::big_text(self.out, (row, col), scale, sgr, s);
        self.rows.extend(row..row + scale);
    }

    fn hud(&mut self, game: &Game, stats: &Stats, sprites: u32) {
        let p = &game.player;
        let mut left = String::new();
        let _ = write!(left, "\x1b[1;2H\x1b[1;38;2;255;59;78m");
        for i in 0..MAX_HP {
            left.push_str(if i < p.hp {
                "♥ "
            } else {
                "\x1b[38;2;70;40;55m♥ "
            });
        }
        let _ = write!(left, "\x1b[0m  \x1b[1;97mSCORE {}", group(game.score));
        if game.multiplier() > 1 {
            let _ = write!(left, "  \x1b[38;2;255;61;240m×{}", game.multiplier());
        }
        let _ = write!(left, "  \x1b[38;2;61;242;255mWAVE {}", game.wave);
        if p.rapid_t > 0.0 {
            let _ = write!(left, "  \x1b[38;2;255;225;77m⚡ {:.1}s", p.rapid_t);
        }
        let dash = if p.dash_cd <= 0.0 {
            "1;38;2;61;242;255"
        } else {
            "38;2;50;70;90"
        };
        let _ = write!(left, "  \x1b[{dash}mDASH\x1b[0m");
        self.out.extend_from_slice(left.as_bytes());

        let right = format!(
            "{:.0} fps · {} sprites · {:.1} ms · {:.0} KB/frame",
            stats.fps, sprites, stats.frame_ms, stats.kb_per_frame
        );
        let col = self.win.cols.saturating_sub(right.chars().count() as u32);
        let _ = write!(self.out, "\x1b[1;{col}H\x1b[38;2;90;100;140m{right}\x1b[0m");
        self.rows.push(1);
    }
}

/// 12345 → "12,345"
fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn group_inserts_thousands_separators() {
        assert_eq!(super::group(0), "0");
        assert_eq!(super::group(999), "999");
        assert_eq!(super::group(12345), "12,345");
        assert_eq!(super::group(1234567), "1,234,567");
    }
}
