//! The arena floor: one big image the camera scrolls across.

use super::*;

/// The arena floor: dark grid, faint noise, and a glowing wall around the edge.
pub fn build_floor(s: f32) -> Image {
    let w = (WORLD_W * s) as u32;
    let h = (WORLD_H * s) as u32;
    let mut img = Image::new(w, h);
    // 10+ megapixels: split rows across cores so start-up stays snappy.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows_per_chunk = (h as usize).div_ceil(threads);
    std::thread::scope(|scope| {
        for (chunk_index, chunk) in img
            .rgba
            .chunks_mut(rows_per_chunk * w as usize * 4)
            .enumerate()
        {
            scope.spawn(move || {
                let first_row = chunk_index * rows_per_chunk;
                for (i, px) in chunk.chunks_exact_mut(4).enumerate() {
                    let x = (i % w as usize) as f32;
                    let y = (first_row + i / w as usize) as f32;
                    px.copy_from_slice(&floor_pixel(x, y, s));
                }
            });
        }
    });
    img
}

pub(super) fn floor_pixel(x: f32, y: f32, s: f32) -> [u8; 4] {
    let wall = hex(0x9d4dff);
    let (u, v) = (x / s, y / s);

    // Base: deep navy, a little lighter towards the middle.
    let dx = u / WORLD_W - 0.5;
    let dy = v / WORLD_H - 0.5;
    let center = (1.0 - (dx * dx + dy * dy).sqrt() * 1.6).max(0.0);
    let mut c = [6.0 + 8.0 * center, 8.0 + 9.0 * center, 18.0 + 22.0 * center];

    // Grid lines, measured in pixels so they stay crisp at any scale.
    let grid = |spacing: f32, width: f32| {
        let gx = (u.rem_euclid(spacing)).min(spacing - u.rem_euclid(spacing)) * s;
        let gy = (v.rem_euclid(spacing)).min(spacing - v.rem_euclid(spacing)) * s;
        (width - gx.min(gy) + 0.5).clamp(0.0, 1.0)
    };
    let minor = grid(60.0, 0.6 * s.max(1.0));
    let major = grid(300.0, 1.2 * s.max(1.0));
    let major_glow = {
        let gx = u.rem_euclid(300.0).min(300.0 - u.rem_euclid(300.0));
        let gy = v.rem_euclid(300.0).min(300.0 - v.rem_euclid(300.0));
        (1.0 - gx.min(gy) / 10.0).max(0.0).powi(2)
    };
    let add = |c: &mut [f32; 3], col: [f32; 3], a: f32| {
        for k in 0..3 {
            c[k] += col[k] * a;
        }
    };
    add(&mut c, [16.0, 22.0, 46.0], minor);
    add(&mut c, [34.0, 50.0, 110.0], major);
    add(&mut c, [8.0, 12.0, 30.0], major_glow);

    // Static noise so the floor doesn't look like flat vector art.
    let n = hash(x as u32, y as u32);
    add(&mut c, [1.0, 1.0, 1.0], (n - 0.5) * 5.0);

    // Arena wall: a bright line with a glow falling off inwards.
    let edge = u.min(v).min(WORLD_W - u).min(WORLD_H - v);
    let wall_line = (1.0 - ((edge - WALL * 0.5).abs() / 3.0)).clamp(0.0, 1.0);
    let wall_glow = (1.0 - edge / 70.0).max(0.0).powi(2);
    add(
        &mut c,
        [wall[0] * 255.0, wall[1] * 255.0, wall[2] * 255.0],
        wall_glow * 0.35,
    );
    add(&mut c, [230.0, 200.0, 255.0], wall_line);

    [
        c[0].min(255.0) as u8,
        c[1].min(255.0) as u8,
        c[2].min(255.0) as u8,
        255,
    ]
}

pub(super) fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}
