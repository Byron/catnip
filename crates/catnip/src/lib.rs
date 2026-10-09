//! Catnip: a small game engine for the kitty terminal.
//!
//! - `kitty`: graphics protocol commands (uploads, placements)
//! - `term`: terminal setup, window size, input modes
//! - `gfx`: procedural sprite drawing and atlas packing
//! - `font`: a 5×7 pixel font for sprite text
//! - `synth`: sound synthesis and playback

pub mod font;
pub mod gfx;
pub mod kitty;
pub mod synth;
pub mod term;
