//! Turns a Game into one kitty frame: the floor scrolls by changing its
//! source rect, every sprite is a placement of the atlas, text goes on top.
//!
//! `world` draws the arena, `ui` the HUD and menus.

use std::{f32::consts::TAU, fmt::Write as _, io::Write};

use catnip::{
    gfx::Frame,
    kitty::{self, AtlasPlacer, Placer, Rect},
    term::Window,
};

use crate::{
    art::{self, Hue, Sprites, TextSize},
    game::{Difficulty, Game, Kind, POPUP_TIME, PickupKind, SHOCK_RADIUS, State, V2, card_span},
};

mod ui;
mod world;

use world::Draw;

/// Atlas pages are uploaded as ids ATLAS_ID, ATLAS_ID + 1, …
pub const ATLAS_ID: u32 = 100;
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
    atlas: AtlasPlacer,
    floor: Placer,
    floor_size: (u32, u32),
    text_rows: Vec<u32>,
    dimmed: bool,
    flashing: bool,
    cards_shown: bool,
    pub sprites_drawn: u32,
}

impl Renderer {
    pub fn new(floor_size: (u32, u32), atlas_pages: usize) -> Self {
        Self {
            atlas: AtlasPlacer::new(ATLAS_ID, atlas_pages),
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

        world::draw_world(&mut draw, game, sprites);

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
}
