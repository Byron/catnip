//! Every sprite in the game, drawn procedurally at the window's pixel scale,
//! plus the arena floor. Nothing is loaded from disk.

use std::f32::consts::{PI, TAU};

use catnip::font;
use catnip::gfx::{
    AtlasBuilder, Color, Frame, Image, Mask, WHITE, glow_dot, hex, neon, par_map, rotate,
};
use catnip::term::Window;

mod floor;
mod palette;
mod shapes;
mod sprites;
mod text;

pub use floor::*;
pub use palette::*;
pub use shapes::*;
pub use sprites::*;
pub use text::*;

pub const WORLD_W: f32 = 2400.0;
pub const WORLD_H: f32 = 1500.0;
pub const WALL: f32 = 14.0;

pub const PLAYER_ANGLES: usize = 48;
pub const BULLET_ANGLES: usize = 32;
pub const SHOOTER_ANGLES: usize = 32;
pub const SPIN_FRAMES: usize = 16;
pub const RING_FRAMES: usize = 9;
pub const SPARK_SIZES: [f32; 3] = [5.0, 9.0, 15.0];
pub const BOSS_SHELL_FRAMES: usize = 16;
pub const BOSS_CORE_FRAMES: usize = 12;
pub const AURA_FRAMES: usize = 8;

/// Pixels per logical unit. Follows the cell height (font size × display
/// density) so the game looks the same size in any window and terminal; the
/// window only decides how much of the arena is visible, within limits.
pub fn scale_for(win: Window) -> f32 {
    const CELL_PX_PER_UNIT: f32 = 19.1;
    let height = win.height as f32;
    let (_, cell_h) = win.cell();
    // Show between 600 and 1300 units vertically, whatever the font size.
    (cell_h as f32 / CELL_PX_PER_UNIT)
        .clamp(height / 1300.0, height / 600.0)
        .clamp(0.5, 3.5)
}

/// Picks the rotation frame closest to `angle` out of `count` evenly spaced ones.
pub fn angle_frame(angle: f32, count: usize, sweep: f32) -> usize {
    let t = angle.rem_euclid(sweep) / sweep;
    ((t * count as f32).round() as usize) % count
}
