//! Sprite fonts: neon glyphs drawn from the engine's 5×7 pixel font.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSize {
    Large,
    Medium,
    Small,
    Tiny,
}

impl TextSize {
    /// Logical units per font pixel.
    pub(super) fn pixel(self) -> f32 {
        match self {
            TextSize::Large => 11.0,
            TextSize::Medium => 7.0,
            TextSize::Small => 4.5,
            TextSize::Tiny => 2.6,
        }
    }
}

/// Neon glyph sprites for one size and color.
pub struct Font {
    pub(super) glyphs: Vec<Option<Frame>>,
    /// Window pixels from one glyph to the next.
    pub advance: f32,
    /// Glow padding around each glyph, in window pixels.
    pub pad: f32,
    /// Glyph height without padding, in window pixels.
    pub height: f32,
}

impl Font {
    pub fn glyph(&self, c: char) -> Option<Frame> {
        let index = font::CHARSET.find(c.to_ascii_uppercase())?;
        self.glyphs[index]
    }

    pub fn width(&self, text: &str) -> f32 {
        let n = text.chars().count() as f32;
        (n * self.advance - self.advance / 6.0).max(0.0)
    }
}

/// The text styles the game uses; only these get rendered.
pub(super) const FONT_STYLES: [(TextSize, Hue); 9] = [
    (TextSize::Large, Hue::Cyan),
    (TextSize::Medium, Hue::Yellow),
    (TextSize::Medium, Hue::Cyan),
    (TextSize::Medium, Hue::Red),
    (TextSize::Small, Hue::White),
    (TextSize::Small, Hue::Yellow),
    (TextSize::Small, Hue::Violet),
    (TextSize::Tiny, Hue::White),
    (TextSize::Tiny, Hue::Yellow),
];

pub(super) fn build_font(atlas: &mut AtlasBuilder, s: f32, size: TextSize, hue: Hue) -> Font {
    let px = size.pixel() * s;
    let pad = (px * 2.0).ceil();
    let w = (font::COLS as f32 * px + pad * 2.0).ceil() as u32;
    let h = (font::ROWS as f32 * px + pad * 2.0).ceil() as u32;
    let gap = px * 0.08;
    let chars: Vec<char> = font::CHARSET.chars().collect();
    let images = par_map(chars.len(), |i| {
        let mut lines = Mask::new(w, h);
        let mut lit = false;
        for (x, y) in font::pixels(chars[i]) {
            lit = true;
            let (x0, y0) = (pad + x as f32 * px + gap, pad + y as f32 * px + gap);
            let (x1, y1) = (x0 + px - gap * 2.0, y0 + px - gap * 2.0);
            lines.fill_polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], 1.0);
        }
        lit.then(|| neon(&lines, &lines, hue.color(), (px * 1.1) as usize, 1.3))
    });
    let glyphs = images
        .into_iter()
        .map(|img| img.map(|img| atlas.add(img)))
        .collect();
    Font {
        glyphs,
        advance: px * (font::COLS + 1) as f32,
        pad,
        height: font::ROWS as f32 * px,
    }
}
