//! The neon palette shared by sprites, particles and text.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hue {
    Cyan,
    Magenta,
    Yellow,
    Orange,
    Green,
    White,
    Red,
    Violet,
}

pub const HUES: [Hue; 8] = [
    Hue::Cyan,
    Hue::Magenta,
    Hue::Yellow,
    Hue::Orange,
    Hue::Green,
    Hue::White,
    Hue::Red,
    Hue::Violet,
];

impl Hue {
    pub fn color(self) -> Color {
        match self {
            Hue::Cyan => hex(0x3df2ff),
            Hue::Magenta => hex(0xff3df0),
            Hue::Yellow => hex(0xffe14d),
            Hue::Orange => hex(0xff8a2b),
            Hue::Green => hex(0x5dff6a),
            Hue::White => WHITE,
            Hue::Red => hex(0xff3b4e),
            Hue::Violet => hex(0xb44dff),
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}
