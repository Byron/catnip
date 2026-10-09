//! Procedural sprite drawing: anti-aliased shapes into coverage masks, a
//! neon-glow compositor, and a shelf packer that builds one sprite atlas.

use crate::kitty::Rect;

pub type Color = [f32; 3];

pub const WHITE: Color = [1.0, 1.0, 1.0];

pub fn hex(rgb: u32) -> Color {
    [
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
    ]
}

/// Straight-alpha RGBA image.
#[derive(Clone)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            rgba: vec![0; (w * h * 4) as usize],
        }
    }

    pub fn with_alpha(&self, factor: f32) -> Self {
        let mut out = self.clone();
        for px in out.rgba.chunks_exact_mut(4) {
            px[3] = (px[3] as f32 * factor) as u8;
        }
        out
    }
}

/// Per-pixel coverage in 0..=1.
#[derive(Clone)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub v: Vec<f32>,
}

const SUB: usize = 4;

impl Mask {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w: w as usize,
            h: h as usize,
            v: vec![0.0; (w * h) as usize],
        }
    }

    fn put(&mut self, x: usize, y: usize, coverage: f32) {
        let cell = &mut self.v[y * self.w + x];
        *cell = cell.max(coverage.clamp(0.0, 1.0));
    }

    fn bounds(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> (usize, usize, usize, usize) {
        let clamp_x = |v: f32| (v.max(0.0) as usize).min(self.w);
        let clamp_y = |v: f32| (v.max(0.0) as usize).min(self.h);
        (
            clamp_x(x0.floor()),
            clamp_y(y0.floor()),
            clamp_x(x1.ceil() + 1.0),
            clamp_y(y1.ceil() + 1.0),
        )
    }

    /// Filled polygon (even-odd), anti-aliased with 4×4 supersampling.
    /// Scanline: each sub-row finds its edge crossings once, then counts the
    /// covered sub-samples per pixel along the spans between them.
    pub fn fill_polygon(&mut self, pts: &[(f32, f32)], strength: f32) {
        let (min_x, max_x) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (min_y, max_y) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let (x0, y0, x1, y1) = self.bounds(min_x, min_y, max_x, max_y);
        let mut hits = vec![0u32; x1.saturating_sub(x0)];
        let mut crossings = Vec::with_capacity(pts.len());
        for y in y0..y1 {
            hits.fill(0);
            for sy in 0..SUB {
                let py = y as f32 + (sy as f32 + 0.5) / SUB as f32;
                crossings.clear();
                let mut j = pts.len() - 1;
                for i in 0..pts.len() {
                    let ((xi, yi), (xj, yj)) = (pts[i], pts[j]);
                    if (yi > py) != (yj > py) {
                        crossings.push(xi + (py - yi) * (xj - xi) / (yj - yi));
                    }
                    j = i;
                }
                crossings.sort_by(f32::total_cmp);
                for span in crossings.chunks_exact(2) {
                    // Sub-sample k of pixel x sits at x + (k + 0.5) / SUB.
                    let first =
                        ((span[0] * SUB as f32 - 0.5).ceil()).max((x0 * SUB) as f32) as usize;
                    let end = ((span[1] * SUB as f32 - 0.5).ceil()).min((x1 * SUB) as f32) as usize;
                    for sample in first..end.max(first) {
                        hits[sample / SUB - x0] += 1;
                    }
                }
            }
            for (i, &n) in hits.iter().enumerate() {
                if n > 0 {
                    self.put(x0 + i, y, strength * n as f32 / (SUB * SUB) as f32);
                }
            }
        }
    }

    /// Closed polygon outline of the given line width.
    pub fn stroke_polygon(&mut self, pts: &[(f32, f32)], width: f32) {
        for i in 0..pts.len() {
            self.line(pts[i], pts[(i + 1) % pts.len()], width);
        }
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32) {
        let half = width / 2.0;
        let (x0, y0, x1, y1) = self.bounds(
            a.0.min(b.0) - half - 1.0,
            a.1.min(b.1) - half - 1.0,
            a.0.max(b.0) + half + 1.0,
            a.1.max(b.1) + half + 1.0,
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let d = segment_distance((x as f32 + 0.5, y as f32 + 0.5), a, b);
                self.put(x, y, half - d + 0.5);
            }
        }
    }

    pub fn fill_circle(&mut self, (cx, cy): (f32, f32), r: f32, strength: f32) {
        let (x0, y0, x1, y1) = self.bounds(cx - r - 1.0, cy - r - 1.0, cx + r + 1.0, cy + r + 1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                self.put(x, y, (r - d + 0.5).clamp(0.0, 1.0) * strength);
            }
        }
    }

    pub fn stroke_circle(&mut self, (cx, cy): (f32, f32), r: f32, width: f32) {
        let half = width / 2.0;
        let outer = r + half + 1.0;
        let (x0, y0, x1, y1) = self.bounds(cx - outer, cy - outer, cx + outer, cy + outer);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                self.put(x, y, half - (d - r).abs() + 0.5);
            }
        }
    }

    /// Three box-blur passes: close enough to a gaussian for glow halos.
    pub fn blurred(&self, radius: usize) -> Mask {
        let mut out = self.clone();
        if radius == 0 {
            return out;
        }
        let mut tmp = vec![0.0; out.v.len()];
        for _ in 0..3 {
            box_blur(&out.v, &mut tmp, out.w, out.h, radius, true);
            box_blur(&tmp, &mut out.v, out.w, out.h, radius, false);
        }
        out
    }
}

fn box_blur(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| {
        if horizontal {
            line * w + i
        } else {
            i * w + line
        }
    };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = 0.0;
        for i in 0..=r.min(len - 1) {
            sum += src[at(line, i)];
        }
        for i in 0..len {
            dst[at(line, i)] = sum * norm;
            if i + r + 1 < len {
                sum += src[at(line, i + r + 1)];
            }
            if i >= r {
                sum -= src[at(line, i - r)];
            }
        }
    }
}

#[cfg(test)]
fn point_in_polygon(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let (dx, dy) = (p.0 - (a.0 + abx * t), p.1 - (a.1 + aby * t));
    (dx * dx + dy * dy).sqrt()
}

/// Composites a neon look: a soft colored halo, a translucent body and a hot,
/// near-white outline.
pub fn neon(lines: &Mask, body: &Mask, color: Color, glow_radius: usize, glow: f32) -> Image {
    let halo = lines.blurred(glow_radius);
    let hot = lerp_color(color, WHITE, 0.55);
    let mut img = Image::new(lines.w as u32, lines.h as u32);
    for (i, px) in img.rgba.chunks_exact_mut(4).enumerate() {
        // Premultiplied "over", back to front: halo, body, lines.
        let mut acc = [0.0f32; 4];
        let mut over = |c: Color, a: f32| {
            let a = a.clamp(0.0, 1.0);
            for k in 0..3 {
                acc[k] = c[k] * a + acc[k] * (1.0 - a);
            }
            acc[3] = a + acc[3] * (1.0 - a);
        };
        over(color, (halo.v[i] * glow).min(0.85));
        over(color, body.v[i] * 0.32);
        over(hot, lines.v[i]);
        if acc[3] > 0.0 {
            for k in 0..3 {
                px[k] = (acc[k] / acc[3] * 255.0).min(255.0) as u8;
            }
            px[3] = (acc[3] * 255.0) as u8;
        }
    }
    img
}

/// Soft round glow, brightest in the middle: sparks, orbs, muzzle flashes.
pub fn glow_dot(size: u32, color: Color, core: f32) -> Image {
    let mut img = Image::new(size, size);
    let c = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d =
                (((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt() / c).min(1.0);
            let a = (1.0 - d).powf(1.6);
            let col = lerp_color(color, WHITE, ((core - d) / core).clamp(0.0, 1.0));
            let i = ((y * size + x) * 4) as usize;
            let [r, g, b] = col.map(|c| (c * 255.0) as u8);
            img.rgba[i..i + 4].copy_from_slice(&[r, g, b, (a * 255.0) as u8]);
        }
    }
    img
}

pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub fn rotate(
    points: &[(f32, f32)],
    angle: f32,
    (cx, cy): (f32, f32),
    scale: f32,
) -> Vec<(f32, f32)> {
    let (sin, cos) = angle.sin_cos();
    points
        .iter()
        .map(|&(x, y)| {
            (
                cx + (x * cos - y * sin) * scale,
                cy + (x * sin + y * cos) * scale,
            )
        })
        .collect()
}

/// Runs `f(0..n)` across all cores and returns the results in order.
pub fn par_map<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |t| t.get());
    let chunk = n.div_ceil(threads).max(1);
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n)
            .step_by(chunk)
            .map(|start| {
                scope.spawn(move || (start..(start + chunk).min(n)).map(f).collect::<Vec<_>>())
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("sprite worker panicked"))
            .collect()
    })
}

/// Shelf packer: sprites go left to right in rows; one big image at the end.
pub struct AtlasBuilder {
    width: u32,
    x: u32,
    y: u32,
    row_h: u32,
    pieces: Vec<(Rect, Image)>,
}

/// A sprite frame inside the atlas. Sprites are drawn centered on their position.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub rect: Rect,
}

impl AtlasBuilder {
    pub fn new(width: u32) -> Self {
        Self {
            width,
            x: 0,
            y: 0,
            row_h: 0,
            pieces: Vec::new(),
        }
    }

    pub fn add(&mut self, img: Image) -> Frame {
        if self.x + img.w > self.width {
            self.x = 0;
            self.y += self.row_h + 1;
            self.row_h = 0;
        }
        let rect = Rect {
            x: self.x,
            y: self.y,
            w: img.w,
            h: img.h,
        };
        self.x += img.w + 1;
        self.row_h = self.row_h.max(img.h);
        self.pieces.push((rect, img));
        Frame { rect }
    }

    pub fn build(self) -> Image {
        let height = self.y + self.row_h;
        let mut atlas = Image::new(self.width, height.max(1));
        for (rect, img) in &self.pieces {
            for row in 0..img.h {
                let src = (row * img.w * 4) as usize;
                let dst = (((rect.y + row) * self.width + rect.x) * 4) as usize;
                atlas.rgba[dst..dst + (img.w * 4) as usize]
                    .copy_from_slice(&img.rgba[src..src + (img.w * 4) as usize]);
            }
        }
        atlas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanline_fill_matches_per_sample_point_test() {
        // A concave star, off-grid, partly outside the mask.
        let star: Vec<(f32, f32)> = (0..10)
            .map(|i| {
                let a = i as f32 * std::f32::consts::PI / 5.0 + 0.3;
                let r = if i % 2 == 0 { 21.3 } else { 8.7 };
                (19.4 + a.cos() * r, 17.9 + a.sin() * r)
            })
            .collect();
        let mut fast = Mask::new(36, 36);
        fast.fill_polygon(&star, 1.0);
        for y in 0..36 {
            for x in 0..36 {
                let mut inside = 0;
                for sy in 0..SUB {
                    for sx in 0..SUB {
                        let px = x as f32 + (sx as f32 + 0.5) / SUB as f32;
                        let py = y as f32 + (sy as f32 + 0.5) / SUB as f32;
                        inside += u32::from(point_in_polygon(px, py, &star));
                    }
                }
                let expected = inside as f32 / (SUB * SUB) as f32;
                assert_eq!(fast.v[y * 36 + x], expected, "pixel {x},{y}");
            }
        }
    }
}
