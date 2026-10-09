//! Building blocks for neon sprites: masks to glowing images, rotation sets.

use super::*;

/// Normal frames plus all-white frames for the hit flash.
pub struct Flashing {
    pub normal: Vec<Frame>,
    pub flash: Vec<Frame>,
    /// The angle the frames cover; a shape with n-fold symmetry repeats after
    /// TAU / n, so it only needs frames for that slice.
    pub sweep: f32,
}

impl Flashing {
    /// The frame closest to `angle`, white when flashing.
    pub fn at(&self, angle: f32, white: bool) -> Frame {
        let frames = if white { &self.flash } else { &self.normal };
        frames[angle_frame(angle, frames.len(), self.sweep)]
    }
}

/// Builds one square neon sprite of logical `radius`, drawn by `draw` into
/// (lines, body) masks in pixel space with center `c` and pixel radius `r`.
pub(super) fn neon_sprite(
    s: f32,
    radius: f32,
    color: Color,
    draw: impl Fn(&mut Mask, &mut Mask, (f32, f32), f32),
) -> Image {
    let pad = radius * 0.6 + 6.0;
    let size = ((radius + pad) * 2.0 * s).ceil() as u32;
    let c = (size as f32 / 2.0, size as f32 / 2.0);
    let mut lines = Mask::new(size, size);
    let mut body = Mask::new(size, size);
    draw(&mut lines, &mut body, c, radius * s);
    neon(
        &lines,
        &body,
        color,
        ((radius * 0.25 + 3.0) * s) as usize,
        1.6,
    )
}

pub(super) fn star(points: usize, inner: f32) -> Vec<(f32, f32)> {
    (0..points * 2)
        .map(|i| {
            let a = i as f32 * PI / points as f32;
            let r = if i % 2 == 0 { 1.0 } else { inner };
            (a.cos() * r, a.sin() * r)
        })
        .collect()
}

pub(super) fn polygon(sides: usize, phase: f32) -> Vec<(f32, f32)> {
    (0..sides)
        .map(|i| {
            let a = phase + i as f32 * TAU / sides as f32;
            (a.cos(), a.sin())
        })
        .collect()
}

pub(super) type Shape<'a> = &'a (dyn Fn(&mut Mask, &mut Mask, (f32, f32), f32, f32) + Sync);

/// Adds `frames` rotations of a shape (normal + white flash variant).
pub(super) fn rotations(
    atlas: &mut AtlasBuilder,
    s: f32,
    radius: f32,
    color: Color,
    frames: usize,
    sweep: f32,
    shape: Shape,
) -> Flashing {
    let images = par_map(frames * 2, |i| {
        let angle = (i / 2) as f32 * sweep / frames as f32;
        let draw = |l: &mut Mask, b: &mut Mask, c: (f32, f32), r: f32| shape(l, b, c, r, angle);
        neon_sprite(s, radius, if i % 2 == 0 { color } else { WHITE }, draw)
    });
    let mut normal = Vec::new();
    let mut flash = Vec::new();
    for (i, img) in images.into_iter().enumerate() {
        let frame = atlas.add(img);
        if i % 2 == 0 {
            normal.push(frame)
        } else {
            flash.push(frame)
        }
    }
    Flashing {
        normal,
        flash,
        sweep,
    }
}
