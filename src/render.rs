//! Turns a Game into one kitty frame: the floor scrolls by changing its
//! source rect, every sprite is a placement of the atlas, text goes on top.

use std::{f32::consts::TAU, fmt::Write as _, io::Write};

use catnip::{
    gfx::Frame,
    kitty::{self, Placer, Rect},
    term::Window,
};

use crate::{
    art::{self, Hue, Sprites, TextSize},
    game::{BULLET_RANGE, Game, Kind, MAX_HP, PickupKind, State, V2},
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
const Z_DIM: i32 = -80;
const Z_RETICLE: i32 = -40;
const Z_TEXT: i32 = -50;

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
    dimmed: bool,
    pub sprites_drawn: u32,
}

impl Renderer {
    pub fn new(floor_size: (u32, u32)) -> Self {
        Self {
            atlas: Placer::new(ATLAS_ID),
            floor: Placer::new(FLOOR_ID),
            floor_size,
            text_rows: Vec::new(),
            dimmed: false,
            sprites_drawn: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        out: &mut Vec<u8>,
        game: &Game,
        sprites: &Sprites,
        scale: f32,
        win: Window,
        stats: &Stats,
        mouse: Option<V2>,
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
            // Dimmer once damage has mostly fallen off.
            let frames = if b.traveled > BULLET_RANGE * 0.55 {
                &sprites.bullet_faded
            } else {
                &sprites.bullet
            };
            draw.sprite(frames[k], b.pos, Z_BULLETS);
        }

        let p = &game.player;
        let blink = p.invuln > 0.0 && (p.invuln * 14.0) as i32 % 2 == 1;
        if matches!(game.state, State::Playing | State::Paused) && !blink {
            let k = art::angle_frame(p.aim, art::PLAYER_ANGLES, TAU);
            draw.sprite(sprites.player[k], p.pos, Z_PLAYER);
        }
        if game.state == State::Playing && (p.heat > 0.01 || p.overheated) {
            draw_heat_bar(&mut draw, sprites, game);
        }
        if let (State::Playing, Some(m)) = (game.state, mouse) {
            let r = sprites.reticle.rect;
            let at = (
                (m.x - r.w as f32 / 2.0).round() as i32,
                (m.y - r.h as f32 / 2.0).round() as i32,
            );
            draw.placer.place(draw.out, r, at, Z_RETICLE);
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

        self.text(out, game, sprites, win, stats, scale);
        self.sprites_drawn = self.atlas.count();
        self.atlas.finish(out);
        self.floor.finish(out);
        out.extend_from_slice(b"\x1b[?2026l");
    }

    fn text(
        &mut self,
        out: &mut Vec<u8>,
        game: &Game,
        sprites: &Sprites,
        win: Window,
        stats: &Stats,
        scale: f32,
    ) {
        for row in self.text_rows.drain(..) {
            let _ = write!(out, "\x1b[{row};1H\x1b[2K");
        }
        let mut text = Text {
            out,
            rows: &mut self.text_rows,
            placer: &mut self.atlas,
            sprites,
            win,
            scale,
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
        // Dim the arena behind menus so their text stays readable.
        let dim = matches!(game.state, State::Paused | State::GameOver);
        if dim {
            let _ = write!(
                text.out,
                "\x1b[1;1H\x1b_Ga=p,i={SHADE_ID},p=2,c={},r={},z={Z_DIM},C=1,q=2\x1b\\",
                win.cols, win.rows
            );
        } else if self.dimmed {
            kitty::delete_placement(text.out, SHADE_ID, 2);
        }
        self.dimmed = dim;
        let row = |fraction: f32| ((win.rows as f32 * fraction) as u32).max(2);
        match game.state {
            State::Title => {
                let fits = sprites.font(TextSize::Large, Hue::Cyan).width("GLOWSTORM")
                    < win.width as f32 * 0.9;
                let size = if fits {
                    TextSize::Large
                } else {
                    TextSize::Medium
                };
                let r = row(0.22) + text.big(row(0.22), size, Hue::Cyan, "GLOWSTORM") + 2;
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
                    TextSize::Medium,
                    Hue::Yellow,
                    &format!("WAVE {}", game.wave),
                );
            }
            State::Paused => {
                let r = row(0.3) + text.big(row(0.3), TextSize::Medium, Hue::Cyan, "PAUSED") + 1;
                text.line(r, "38;2;200;210;255", "Esc or click to resume · Q to quit");
            }
            State::GameOver => {
                let mut r =
                    row(0.22) + text.big(row(0.22), TextSize::Medium, Hue::Red, "GAME OVER") + 1;
                r += text.big(r, TextSize::Small, Hue::White, &group(game.score)) + 1;
                if game.new_best {
                    r += text.big(r, TextSize::Small, Hue::Yellow, "NEW BEST!");
                } else {
                    text.line(r, "38;2;255;225;77", &format!("best {}", group(game.best)));
                }
                text.line(
                    r + 1,
                    "38;2;200;210;255",
                    &format!("reached wave {}", game.wave),
                );
                text.line(r + 3, "38;2;140;150;190", "click or R to restart · Q quit");
            }
            _ => {}
        }
    }
}

/// Ten segments under the ship; they blink red while the gun is locked.
fn draw_heat_bar(draw: &mut Draw, sprites: &Sprites, game: &Game) {
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
    placer: &'a mut Placer,
    sprites: &'a Sprites,
    win: Window,
    scale: f32,
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

    /// Neon sprite text centered on the window, top edge at `row`.
    /// Returns how many terminal rows it covers.
    fn big(&mut self, row: u32, size: TextSize, hue: Hue, s: &str) -> u32 {
        let font = self.sprites.font(size, hue);
        let cell_h = self.win.cell().1.max(1);
        let top = (row.saturating_sub(1) * cell_h) as f32;
        let mut x = (self.win.width as f32 - font.width(s)) / 2.0;
        for c in s.chars() {
            if let Some(frame) = font.glyph(c) {
                let at = (
                    (x - font.pad).round() as i32,
                    (top - font.pad).round() as i32,
                );
                self.placer.place(self.out, frame.rect, at, Z_TEXT);
            }
            x += font.advance;
        }
        (font.height / cell_h as f32).ceil() as u32
    }

    fn hud(&mut self, game: &Game, stats: &Stats, sprites: u32) {
        let p = &game.player;
        // Lives are sprites: heart glyphs render differently (or in emoji
        // colors) depending on the terminal's fonts.
        let (cell_w, cell_h) = self.win.cell();
        let spacing = 20.0 * self.scale;
        let center_y = cell_h as f32 / 2.0;
        let mut x = cell_w as f32 + spacing / 2.0;
        for i in 0..MAX_HP {
            let frame = self.sprites.hud_heart[usize::from(i >= p.hp)];
            let at = (
                (x - frame.rect.w as f32 / 2.0).round() as i32,
                (center_y - frame.rect.h as f32 / 2.0).round() as i32,
            );
            self.placer.place(self.out, frame.rect, at, Z_TEXT);
            x += spacing;
        }
        let text_col = (x / cell_w as f32).ceil() as u32 + 1;
        let mut left = String::new();
        let _ = write!(left, "\x1b[1;{text_col}H");
        let _ = write!(left, "\x1b[1;97mSCORE {}", group(game.score));
        if game.multiplier() > 1 {
            let _ = write!(left, "  \x1b[38;2;255;61;240m×{}", game.multiplier());
        }
        let _ = write!(left, "  \x1b[38;2;61;242;255mWAVE {}", game.wave);
        if p.rapid_t > 0.0 {
            let _ = write!(left, "  \x1b[38;2;255;225;77mRAPID {:.1}s", p.rapid_t);
        }
        if p.dash_cd <= 0.0 {
            let _ = write!(left, "  \x1b[1;38;2;61;242;255mDASH\x1b[0m");
        } else {
            let _ = write!(left, "  \x1b[38;2;60;80;105mDASH {:.1}s\x1b[0m", p.dash_cd);
        }
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
