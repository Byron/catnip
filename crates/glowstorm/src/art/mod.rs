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
/// window decides how much of the arena is visible and when it grows.
/// The image budget can lower this cap when a larger window grows the floor.
pub const MAX_SCALE: f32 = 1.8;

pub fn scale_for(win: Window, forced: Option<f32>) -> f32 {
    const CELL_PX_PER_UNIT: f32 = 19.1;
    let height = win.height as f32;
    let (_, cell_h) = win.cell();
    if let Some(forced) = forced {
        return forced.clamp(0.3, MAX_SCALE);
    }
    // Show at least 600 units vertically; larger windows reveal more arena.
    (cell_h as f32 / CELL_PX_PER_UNIT)
        .min(height / 600.0)
        .clamp(0.5, MAX_SCALE)
}

/// Keep the atlas, arena floor and three overlay pixels below Ghostty's 320 MB quota.
pub fn floor_fits((width, height): (u32, u32), atlas_bytes: usize) -> bool {
    width <= 10_000
        && height <= 10_000
        && atlas_bytes as u64 + u64::from(width) * u64::from(height) * 4 + 12 <= 320_000_000
}

/// Rebuild only when the actual packed atlas and floor exceed the image quota.
pub fn build_fitting_sprites(
    win: Window,
    mut scale: f32,
) -> std::io::Result<(f32, Vec<Image>, Sprites)> {
    loop {
        let (atlas, sprites) = build_sprites(scale);
        let arena = crate::game::View::new(win, scale).arena_size() * scale;
        let bytes = atlas.iter().map(|page| page.rgba.len()).sum();
        if floor_fits((arena.x as u32, arena.y as u32), bytes) {
            return Ok((scale, atlas, sprites));
        }
        if scale <= 0.3 {
            return Err(std::io::Error::other(
                "window exceeds the terminal image budget",
            ));
        }
        scale = (scale * 0.9).max(0.3);
    }
}

/// Picks the rotation frame closest to `angle` out of `count` evenly spaced ones.
pub fn angle_frame(angle: f32, count: usize, sweep: f32) -> usize {
    let t = angle.rem_euclid(sweep) / sweep;
    ((t * count as f32).round() as usize) % count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_floor_keeps_the_sprite_atlas_within_the_image_quota() {
        let win = Window {
            cols: 179,
            rows: 105,
            width: 2864,
            height: 5040,
        };
        let (atlas, _) = build_sprites(MAX_SCALE);
        let bytes = atlas.iter().map(|page| page.rgba.len()).sum();
        assert!(floor_fits((4320, 3840), bytes));
        assert!(!floor_fits((4320, 5040), bytes));
        drop(atlas);
        let (scale, atlas, _) = build_fitting_sprites(win, MAX_SCALE).unwrap();
        assert!(scale < MAX_SCALE);
        let arena = crate::game::View::new(win, scale).arena_size() * scale;
        assert!(floor_fits(
            (arena.x as u32, arena.y as u32),
            atlas.iter().map(|page| page.rgba.len()).sum()
        ));
        assert!(floor_fits((10_000, 1), 319_959_988));
        assert!(!floor_fits((10_000, 1), 319_959_989));
        assert!(!floor_fits((10_001, 1), 0));
    }
}
