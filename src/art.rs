//! Every sprite in the game, drawn procedurally at the window's pixel scale,
//! plus the arena floor. Nothing is loaded from disk.

use std::f32::consts::{PI, TAU};

use catnip::font;
use catnip::gfx::{AtlasBuilder, Color, Frame, Image, Mask, WHITE, glow_dot, hex, neon, rotate};

pub const WORLD_W: f32 = 2400.0;
pub const WORLD_H: f32 = 1500.0;
pub const WALL: f32 = 14.0;

pub const PLAYER_ANGLES: usize = 48;
pub const BULLET_ANGLES: usize = 32;
pub const SHOOTER_ANGLES: usize = 32;
pub const SPIN_FRAMES: usize = 16;
pub const RING_FRAMES: usize = 9;
pub const SPARK_SIZES: [f32; 3] = [5.0, 9.0, 15.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hue {
    Cyan,
    Magenta,
    Yellow,
    Orange,
    Green,
    White,
    Red,
}

pub const HUES: [Hue; 7] = [
    Hue::Cyan,
    Hue::Magenta,
    Hue::Yellow,
    Hue::Orange,
    Hue::Green,
    Hue::White,
    Hue::Red,
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
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Normal frames plus all-white frames for the hit flash.
pub struct Flashing {
    pub normal: Vec<Frame>,
    pub flash: Vec<Frame>,
}

pub struct Sprites {
    pub player: Vec<Frame>,
    pub ghost: Vec<Frame>,
    pub bullet: Vec<Frame>,
    pub chaser: Flashing,
    pub swarm: Flashing,
    pub shooter: Flashing,
    pub tank: Flashing,
    pub orb: Frame,
    /// [hue][size]
    pub spark: Vec<[Frame; 3]>,
    /// [hue][frame], small to large
    pub ring: Vec<Vec<Frame>>,
    pub heart: [Frame; 2],
    pub bolt: [Frame; 2],
    /// Bullets near the end of their range, dimmer to show weaker damage.
    pub bullet_faded: Vec<Frame>,
    /// HUD lives: [full, empty].
    pub hud_heart: [Frame; 2],
    pub reticle: Frame,
    /// Heat bar segments: lit cool → hot, then unlit.
    pub heat: [Frame; 4],
    pub heat_off: Frame,
    fonts: Vec<(TextSize, Hue, Font)>,
}

impl Sprites {
    pub fn font(&self, size: TextSize, hue: Hue) -> &Font {
        self.fonts
            .iter()
            .find(|(s, h, _)| *s == size && *h == hue)
            .map(|(_, _, font)| font)
            .expect("font style is built in build_sprites")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSize {
    Large,
    Medium,
    Small,
}

impl TextSize {
    /// Logical units per font pixel.
    fn pixel(self) -> f32 {
        match self {
            TextSize::Large => 11.0,
            TextSize::Medium => 7.0,
            TextSize::Small => 4.5,
        }
    }
}

/// Neon glyph sprites for one size and color.
pub struct Font {
    glyphs: Vec<Option<Frame>>,
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
const FONT_STYLES: [(TextSize, Hue); 6] = [
    (TextSize::Large, Hue::Cyan),
    (TextSize::Medium, Hue::Yellow),
    (TextSize::Medium, Hue::Cyan),
    (TextSize::Medium, Hue::Red),
    (TextSize::Small, Hue::White),
    (TextSize::Small, Hue::Yellow),
];

fn build_font(atlas: &mut AtlasBuilder, s: f32, size: TextSize, hue: Hue) -> Font {
    let px = size.pixel() * s;
    let pad = (px * 2.0).ceil();
    let w = (font::COLS as f32 * px + pad * 2.0).ceil() as u32;
    let h = (font::ROWS as f32 * px + pad * 2.0).ceil() as u32;
    let gap = px * 0.08;
    let glyphs = font::CHARSET
        .chars()
        .map(|c| {
            let mut lines = Mask::new(w, h);
            let mut lit = false;
            for (x, y) in font::pixels(c) {
                lit = true;
                let (x0, y0) = (pad + x as f32 * px + gap, pad + y as f32 * px + gap);
                let (x1, y1) = (x0 + px - gap * 2.0, y0 + px - gap * 2.0);
                lines.fill_polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], 1.0);
            }
            lit.then(|| atlas.add(neon(&lines, &lines, hue.color(), (px * 1.1) as usize, 1.3)))
        })
        .collect();
    Font {
        glyphs,
        advance: px * (font::COLS + 1) as f32,
        pad,
        height: font::ROWS as f32 * px,
    }
}

/// Pixels per logical unit, chosen so roughly 900 units fit the window height.
pub fn scale_for(window_height: u32) -> f32 {
    (window_height as f32 / 900.0).clamp(0.5, 3.5)
}

/// Builds one square neon sprite of logical `radius`, drawn by `draw` into
/// (lines, body) masks in pixel space with center `c` and pixel radius `r`.
fn neon_sprite(
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

fn star(points: usize, inner: f32) -> Vec<(f32, f32)> {
    (0..points * 2)
        .map(|i| {
            let a = i as f32 * PI / points as f32;
            let r = if i % 2 == 0 { 1.0 } else { inner };
            (a.cos() * r, a.sin() * r)
        })
        .collect()
}

fn polygon(sides: usize, phase: f32) -> Vec<(f32, f32)> {
    (0..sides)
        .map(|i| {
            let a = phase + i as f32 * TAU / sides as f32;
            (a.cos(), a.sin())
        })
        .collect()
}

type Shape<'a> = &'a dyn Fn(&mut Mask, &mut Mask, (f32, f32), f32, f32);

/// Adds `frames` rotations of a shape (normal + white flash variant).
fn rotations(
    atlas: &mut AtlasBuilder,
    s: f32,
    radius: f32,
    color: Color,
    frames: usize,
    sweep: f32,
    shape: Shape,
) -> Flashing {
    let mut normal = Vec::new();
    let mut flash = Vec::new();
    for i in 0..frames {
        let angle = i as f32 * sweep / frames as f32;
        let draw = |l: &mut Mask, b: &mut Mask, c: (f32, f32), r: f32| shape(l, b, c, r, angle);
        normal.push(atlas.add(neon_sprite(s, radius, color, draw)));
        flash.push(atlas.add(neon_sprite(s, radius, WHITE, draw)));
    }
    Flashing { normal, flash }
}

pub fn build_sprites(s: f32) -> (Image, Sprites) {
    let mut atlas = AtlasBuilder::new(4096);
    let line = |w: f32| (w * s).max(1.2);

    // Player: a chevron pointing along +x.
    let ship = [(1.0, 0.0), (-0.75, 0.7), (-0.35, 0.0), (-0.75, -0.7)];
    let mut player = Vec::new();
    let mut ghost = Vec::new();
    for i in 0..PLAYER_ANGLES {
        let angle = i as f32 * TAU / PLAYER_ANGLES as f32;
        let img = neon_sprite(s, 20.0, Hue::Cyan.color(), |l, b, c, r| {
            let pts = rotate(&ship, angle, c, r);
            b.fill_polygon(&pts, 1.0);
            l.stroke_polygon(&pts, line(2.4));
            l.fill_circle(rotate(&[(-0.2, 0.0)], angle, c, r)[0], r * 0.12, 1.0);
        });
        ghost.push(atlas.add(img.with_alpha(0.35)));
        player.push(atlas.add(img));
    }

    let mut bullet = Vec::new();
    let mut bullet_faded = Vec::new();
    for i in 0..BULLET_ANGLES {
        let angle = i as f32 * TAU / BULLET_ANGLES as f32;
        let img = neon_sprite(s, 11.0, hex(0xfff3a0), |l, _, c, r| {
            let ends = rotate(&[(-1.0, 0.0), (1.0, 0.0)], angle, c, r);
            l.line(ends[0], ends[1], line(4.0));
        });
        bullet_faded.push(atlas.add(img.with_alpha(0.4)));
        bullet.push(atlas.add(img));
    }

    let chaser_shape = |l: &mut Mask, b: &mut Mask, c: (f32, f32), r: f32, angle: f32| {
        let pts = rotate(&star(4, 0.4), angle, c, r);
        b.fill_polygon(&pts, 1.0);
        l.stroke_polygon(&pts, line(2.2));
    };
    let chaser = rotations(
        &mut atlas,
        s,
        17.0,
        Hue::Magenta.color(),
        SPIN_FRAMES,
        PI / 2.0,
        &chaser_shape,
    );
    let swarm = rotations(
        &mut atlas,
        s,
        11.0,
        Hue::Yellow.color(),
        SPIN_FRAMES,
        PI / 2.0,
        &chaser_shape,
    );

    let shooter = rotations(
        &mut atlas,
        s,
        20.0,
        Hue::Orange.color(),
        SHOOTER_ANGLES,
        TAU,
        &|l, b, c, r, angle| {
            let outer = rotate(
                &[(1.0, 0.0), (-0.65, 0.85), (-0.35, 0.0), (-0.65, -0.85)],
                angle,
                c,
                r,
            );
            b.fill_polygon(&outer, 1.0);
            l.stroke_polygon(&outer, line(2.2));
            l.stroke_circle(c, r * 0.28, line(2.0));
        },
    );

    let tank = rotations(
        &mut atlas,
        s,
        34.0,
        Hue::Green.color(),
        SPIN_FRAMES,
        PI / 3.0,
        &|l, b, c, r, angle| {
            let outer = rotate(&polygon(6, 0.0), angle, c, r);
            let inner = rotate(&polygon(6, PI / 6.0), -angle * 2.0, c, r * 0.55);
            b.fill_polygon(&outer, 1.0);
            l.stroke_polygon(&outer, line(3.0));
            l.stroke_polygon(&inner, line(2.4));
            l.fill_circle(c, r * 0.14, 1.0);
        },
    );

    let orb = atlas.add(glow_dot((22.0 * s) as u32, Hue::Red.color(), 0.45));

    let spark = HUES
        .iter()
        .map(|hue| {
            SPARK_SIZES
                .map(|size| atlas.add(glow_dot(((size * s) as u32).max(3), hue.color(), 0.5)))
        })
        .collect();

    let ring = HUES
        .iter()
        .map(|hue| {
            (0..RING_FRAMES)
                .map(|k| {
                    let t = k as f32 / (RING_FRAMES - 1) as f32;
                    let img = neon_sprite(s, 60.0, hue.color(), |l, _, c, r| {
                        l.stroke_circle(c, r * (0.2 + 0.8 * t), line(7.0 - 5.0 * t));
                    });
                    atlas.add(img.with_alpha(1.0 - 0.75 * t))
                })
                .collect()
        })
        .collect();

    let heart_shape = |scale: f32| {
        move |l: &mut Mask, b: &mut Mask, (cx, cy): (f32, f32), r: f32| {
            let r = r * scale;
            let lobe = r * 0.5;
            let left = (cx - lobe * 0.95, cy - r * 0.2);
            let right = (cx + lobe * 0.95, cy - r * 0.2);
            let tip = [
                (cx - r * 0.98, cy - r * 0.05),
                (cx + r * 0.98, cy - r * 0.05),
                (cx, cy + r * 0.95),
            ];
            for m in [&mut *b, &mut *l] {
                m.fill_circle(left, lobe, 1.0);
                m.fill_circle(right, lobe, 1.0);
                m.fill_polygon(&tip, 1.0);
            }
        }
    };
    let heart =
        [1.0, 1.15].map(|k| atlas.add(neon_sprite(s, 15.0, Hue::Red.color(), heart_shape(k))));
    let hud_heart = [
        atlas.add(neon_sprite(s, 8.0, Hue::Red.color(), heart_shape(1.0))),
        atlas.add(neon_sprite(s, 8.0, hex(0x5a2a3a), heart_shape(1.0)).with_alpha(0.6)),
    ];

    let reticle = atlas.add(neon_sprite(s, 14.0, hex(0xd8fbff), |l, _, c, r| {
        l.stroke_circle(c, r * 0.62, line(1.8));
        for k in 0..4 {
            let ends = rotate(&[(0.82, 0.0), (1.15, 0.0)], k as f32 * TAU / 4.0, c, r);
            l.line(ends[0], ends[1], line(2.0));
        }
        l.fill_circle(c, r * 0.08, 1.0);
    }));

    let segment = |color: Color| {
        let (w, h) = (8.0 * s, 5.0 * s);
        let pad = (6.0 * s).ceil();
        let (iw, ih) = ((w + pad * 2.0).ceil() as u32, (h + pad * 2.0).ceil() as u32);
        let mut m = Mask::new(iw, ih);
        m.fill_polygon(
            &[
                (pad, pad),
                (pad + w, pad),
                (pad + w, pad + h),
                (pad, pad + h),
            ],
            1.0,
        );
        neon(&m, &m, color, (3.0 * s) as usize, 1.2)
    };
    let heat =
        [hex(0x3df2ff), hex(0xffe14d), hex(0xff8a2b), hex(0xff3b4e)].map(|c| atlas.add(segment(c)));
    let heat_off = atlas.add(segment(hex(0x2a3550)).with_alpha(0.7));

    let bolt_pts = [
        (0.2, -1.0),
        (-0.55, 0.15),
        (-0.05, 0.15),
        (-0.25, 1.0),
        (0.55, -0.2),
        (0.05, -0.2),
    ];
    let bolt = [1.0, 1.15].map(|k| {
        atlas.add(neon_sprite(s, 16.0, Hue::Yellow.color(), |l, b, c, r| {
            let pts = rotate(&bolt_pts, 0.0, c, r * k);
            b.fill_polygon(&pts, 1.0);
            l.stroke_polygon(&pts, line(2.2));
        }))
    });

    let sprites = Sprites {
        player,
        ghost,
        bullet,
        chaser,
        swarm,
        shooter,
        tank,
        orb,
        spark,
        ring,
        heart,
        bolt,
        bullet_faded,
        hud_heart,
        reticle,
        heat,
        heat_off,
        fonts: FONT_STYLES
            .iter()
            .map(|&(size, hue)| (size, hue, build_font(&mut atlas, s, size, hue)))
            .collect(),
    };
    (atlas.build(), sprites)
}

/// Picks the rotation frame closest to `angle` out of `count` evenly spaced ones.
pub fn angle_frame(angle: f32, count: usize, sweep: f32) -> usize {
    let t = angle.rem_euclid(sweep) / sweep;
    ((t * count as f32).round() as usize) % count
}

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

fn floor_pixel(x: f32, y: f32, s: f32) -> [u8; 4] {
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

fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}
