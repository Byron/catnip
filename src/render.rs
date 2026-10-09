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
    game::{Difficulty, Game, Kind, POPUP_TIME, PickupKind, SHOCK_RADIUS, State, V2, card_span},
};

pub const ATLAS_ID: u32 = 10;
pub const FLOOR_ID: u32 = 11;
/// 1×1 translucent black pixel, stretched behind the HUD row.
pub const SHADE_ID: u32 = 12;
/// 1×1 translucent white pixel for screen flashes.
pub const FLASH_ID: u32 = 13;

// Negative z keeps images below the text layer (HUD, banners).
const Z_FLOOR: i32 = -1_000_000;
const Z_MOTES: i32 = -950;
const Z_RINGS: i32 = -900;
const Z_PICKUPS: i32 = -800;
const Z_GHOSTS: i32 = -700;
const Z_BOSS: i32 = -650;
const Z_ENEMIES: i32 = -600;
const Z_BULLETS: i32 = -550;
const Z_PLAYER: i32 = -400;
const Z_ORBS: i32 = -350;
const Z_PARTICLES: i32 = -300;
const Z_POPUPS: i32 = -200;
const Z_FLASH: i32 = -90;
const Z_SHADE: i32 = -100;
const Z_DIM: i32 = -80;
const Z_CARD: i32 = -70;
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
    flashing: bool,
    cards_shown: bool,
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
            flashing: false,
            cards_shown: false,
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
            if e.elite {
                let aura = &sprites.aura[usize::from(e.kind == Kind::Tank)];
                let k = art::angle_frame(game.time * 1.5 + e.phase, art::AURA_FRAMES, TAU / 10.0);
                draw.sprite(aura[k], e.pos, Z_ENEMIES - 1);
            }
        }

        if let Some(b) = &game.boss {
            draw_boss(&mut draw, sprites, game, b);
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
            draw_heat_bar(&mut draw, sprites, game);
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
            draw_world_text(&mut draw, sprites, size, hue, &p.text, p.pos);
        }
        if game.flash > 0.0 {
            let _ = write!(
                draw.out,
                "\x1b[1;1H\x1b_Ga=p,i={FLASH_ID},p=1,c={},r={},z={Z_FLASH},C=1,q=2\x1b\\",
                win.cols, win.rows
            );
            self.flashing = true;
        } else if self.flashing {
            kitty::delete_placement(draw.out, FLASH_ID, 1);
            self.flashing = false;
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

        if matches!(
            game.state,
            State::Playing | State::Paused | State::Dying | State::Upgrade
        ) {
            let _ = write!(
                text.out,
                "\x1b[1;1H\x1b_Ga=p,i={SHADE_ID},p=1,c={},r=1,z={Z_SHADE},C=1,q=2\x1b\\",
                win.cols
            );
            text.hud(game, stats, self.sprites_drawn);
            if let Some(b) = &game.boss {
                text.boss_bar(b.hp / b.max_hp, b.phase(), b.entering);
            }
        } else {
            kitty::delete_placement(text.out, SHADE_ID, 1);
        }
        // Dim the arena behind menus so their text stays readable.
        let dim = matches!(game.state, State::Paused | State::GameOver | State::Upgrade);
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
        if game.state == State::Upgrade {
            text.cards(game);
            self.cards_shown = true;
        } else if self.cards_shown {
            for id in [10, 11, 12, 20] {
                kitty::delete_placement(text.out, SHADE_ID, id);
            }
            kitty::delete_placement(text.out, FLASH_ID, 2);
            self.cards_shown = false;
        }
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
                // Difficulty menu: the selected one is yellow.
                let mut r = r + 3;
                for d in Difficulty::ALL {
                    let hue = if d == game.difficulty {
                        Hue::Yellow
                    } else {
                        Hue::White
                    };
                    r += text.big(r, TextSize::Small, hue, d.name());
                }
                text.line(r + 1, "38;2;200;210;255", game.difficulty.blurb());
                if game.best > 0 {
                    text.line(
                        r + 2,
                        "38;2;140;150;190",
                        &format!("best {}", group(game.best)),
                    );
                }
                text.line(
                    r + 4,
                    "1;38;2;255;225;77",
                    "↑/↓ choose · click or Enter to start",
                );
                text.line(win.rows, "38;2;90;100;140", "Esc pause · Q quit");
            }
            State::Playing
                if game.boss_pending > 0.0
                    || game.boss.as_ref().is_some_and(|b| b.entering > 0.0) =>
            {
                if (game.time * 4.0) as i32 % 2 == 0 {
                    text.big(row(0.18), TextSize::Medium, Hue::Red, "WARNING");
                }
            }
            State::Playing if game.wave_clear > 0.0 => {
                text.big(row(0.2), TextSize::Medium, Hue::Cyan, "WAVE CLEAR");
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
                text.line(
                    r + 3,
                    "38;2;140;150;190",
                    &format!(
                        "{} · click or R to restart · Esc menu · Q quit",
                        game.difficulty.name()
                    ),
                );
            }
            _ => {}
        }
    }
}

fn draw_boss(draw: &mut Draw, sprites: &Sprites, game: &Game, b: &crate::game::Boss) {
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
fn draw_world_text(
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
            draw.placer.place(draw.out, frame.rect, pos, Z_POPUPS);
        }
        x += font.advance;
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

    /// Plain text with its first character at (row, col).
    fn at(&mut self, row: u32, col: u32, sgr: &str, s: &str) {
        let _ = write!(self.out, "\x1b[{row};{col}H\x1b[{sgr}m{s}\x1b[0m");
        self.rows.push(row);
    }

    /// Neon sprite text centered on the window, top edge at `row`.
    /// Returns how many terminal rows it covers.
    fn big(&mut self, row: u32, size: TextSize, hue: Hue, s: &str) -> u32 {
        let center = self.win.width as f32 / 2.0;
        self.big_at(row, center, size, hue, s)
    }

    /// Like `big`, centered on window pixel column `center`.
    fn big_at(&mut self, row: u32, center: f32, size: TextSize, hue: Hue, s: &str) -> u32 {
        let font = self.sprites.font(size, hue);
        let cell_h = self.win.cell().1.max(1);
        let top = (row.saturating_sub(1) * cell_h) as f32;
        let mut x = center - font.width(s) / 2.0;
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

    /// The upgrade screen: three cards, the highlighted one lit up.
    fn cards(&mut self, game: &Game) {
        const CARD_ROWS: u32 = 11;
        let (cell_w, _) = self.win.cell();
        let width = self.win.width as f32;
        let top = ((self.win.rows as f32 * 0.18) as u32).max(3);
        let r = top + self.big(top, TextSize::Medium, Hue::Yellow, "CHOOSE AN UPGRADE") + 2;
        let n = game.offer.len();
        for (i, &upgrade) in game.offer.iter().enumerate() {
            let (left, right) = card_span(i, n, width);
            let col = (left / cell_w as f32) as u32 + 1;
            let cols = ((right - left) / cell_w as f32) as u32;
            let picked = i == game.offer_pick;
            let _ = write!(
                self.out,
                "\x1b[{r};{col}H\x1b_Ga=p,i={SHADE_ID},p={},c={cols},r={CARD_ROWS},z={Z_CARD},C=1,q=2\x1b\\",
                10 + i
            );
            if picked {
                let _ = write!(
                    self.out,
                    "\x1b[{r};{col}H\x1b_Ga=p,i={FLASH_ID},p=2,c={cols},r={CARD_ROWS},z={},C=1,q=2\x1b\\",
                    Z_CARD + 1
                );
            }
            let center = (left + right) / 2.0;
            let hue = if picked { Hue::Yellow } else { Hue::White };
            // Long names drop to the tiny font on narrow windows.
            let size = if self
                .sprites
                .font(TextSize::Small, hue)
                .width(upgrade.name())
                < (right - left) * 0.9
            {
                TextSize::Small
            } else {
                TextSize::Tiny
            };
            let key = format!("{}", i + 1);
            self.at(r + 1, col + cols / 2, "38;2;140;150;190", &key);
            let name_rows = self.big_at(r + 3, center, size, hue, upgrade.name());
            let level = game.player.upgrades.level(upgrade);
            let status = if level == 0 {
                "NEW".to_string()
            } else {
                format!("LEVEL {} → {}", level, level + 1)
            };
            let status_col = col + cols.saturating_sub(status.chars().count() as u32) / 2;
            self.at(r + 4 + name_rows, status_col, "38;2;93;255;106", &status);
            for (k, line) in wrap(upgrade.blurb(), cols.saturating_sub(4) as usize)
                .iter()
                .enumerate()
            {
                let c = col + cols.saturating_sub(line.chars().count() as u32) / 2;
                let sgr = if picked {
                    "1;38;2;255;255;255"
                } else {
                    "38;2;200;210;255"
                };
                self.at(r + 6 + name_rows + k as u32, c, sgr, line);
            }
        }
        self.line(
            r + CARD_ROWS + 1,
            "1;38;2;255;225;77",
            "1 / 2 / 3, ← →, or click a card",
        );
    }

    /// Boss name and a segmented health bar under the HUD row.
    fn boss_bar(&mut self, health: f32, phase: u32, entering: f32) {
        const SEGMENTS: usize = 48;
        let r = self.big(3, TextSize::Small, Hue::Violet, "PRISM");
        let cell_h = self.win.cell().1 as f32;
        let y = (3 + r) as f32 * cell_h - cell_h * 0.3;
        let seg = self.sprites.boss_seg.rect;
        let spacing = 10.0 * self.scale;
        let left = self.win.width as f32 / 2.0 - spacing * (SEGMENTS as f32 - 1.0) / 2.0;
        // The bar fills up during the entrance.
        let shown = if entering > 0.0 {
            1.0 - entering / 2.4
        } else {
            health
        };
        let lit = (shown * SEGMENTS as f32).ceil() as usize;
        for i in 0..SEGMENTS {
            let frame = if i >= lit {
                self.sprites.heat_off.rect
            } else if phase >= 3 {
                self.sprites.heat[3].rect
            } else {
                seg
            };
            let x = left + i as f32 * spacing - frame.w as f32 / 2.0;
            let at = (x.round() as i32, (y - frame.h as f32 / 2.0).round() as i32);
            self.placer.place(self.out, frame, at, Z_TEXT);
        }
    }

    fn hud(&mut self, game: &Game, stats: &Stats, sprites: u32) {
        let p = &game.player;
        // Lives are sprites: heart glyphs render differently (or in emoji
        // colors) depending on the terminal's fonts.
        let (cell_w, cell_h) = self.win.cell();
        let spacing = 20.0 * self.scale;
        let center_y = cell_h as f32 / 2.0;
        let mut x = cell_w as f32 + spacing / 2.0;
        for i in 0..p.max_hp {
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
        if game.power() > 1.0 {
            let _ = write!(
                left,
                "  \x1b[38;2;93;255;106mPOWER +{:.0}%",
                (game.power() - 1.0) * 100.0
            );
        }
        if game.difficulty != Difficulty::Medium {
            let _ = write!(left, "  \x1b[38;2;255;59;78m{}", game.difficulty.name());
        }
        if p.rapid_t > 0.0 {
            let _ = write!(left, "  \x1b[38;2;255;225;77mRAPID {:.1}s", p.rapid_t);
        }
        let charges = if p.max_dash_charges() > 1 {
            format!(" x{}", p.dash_charges)
        } else {
            String::new()
        };
        if p.dash_charges > 0 {
            let _ = write!(left, "  \x1b[1;38;2;61;242;255mDASH{charges}\x1b[0m");
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

/// Greedy word wrap to `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split(' ') {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width.max(8) => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
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
