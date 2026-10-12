//! The sprite sheet: every frame the game draws, packed into one atlas.

use super::*;

pub(super) const AURA_DASHES: usize = 10;

pub struct Sprites {
    pub player: Vec<Frame>,
    pub ghost: Vec<Frame>,
    pub bullet: Vec<Frame>,
    /// Elongated sniper rounds: fresh and weakened after piercing.
    pub sniper: [Vec<Frame>; 2],
    pub rocket: Vec<Frame>,
    pub plasma: [Frame; 2],
    /// Green plasma spheres, from a fresh charge to full BFG power.
    pub bfg: Vec<Frame>,
    pub ricochet: Vec<Frame>,
    pub blades: Vec<Vec<Frame>>,
    pub mine: Frame,
    pub fuel: Vec<Frame>,
    /// Charging muzzle: normal and timing bonus.
    pub charged: [Vec<Frame>; 2],
    pub laser: [Vec<Frame>; 5],
    pub chaser: Flashing,
    pub swarm: Flashing,
    pub shooter: Flashing,
    pub tank: Flashing,
    pub skirmisher: Flashing,
    pub enemy_sniper: Flashing,
    pub spinner: Flashing,
    pub bomber: Flashing,
    pub splitter: Flashing,
    pub sentinel: Flashing,
    pub orb: Frame,
    pub heavy_orb: Frame,
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
    /// Heat bar segments: lit cool → hot, then unlit.
    pub heat: [Frame; 4],
    pub heat_off: Frame,
    pub boss_shell: Flashing,
    /// Helix, Lotus, Loom and Eclipse; the core and emitters supply their motion.
    pub boss_crests: [Flashing; 4],
    pub boss_core: Flashing,
    /// Boss health bar segment.
    pub boss_seg: [Frame; 5],
    /// Dim drifting background specks: [cyan, violet].
    pub mote: [Frame; 2],
    /// Rotating gold dashed rings marking elites: [small, big].
    pub aura: [Flashing; 2],
    pub shield: Frame,
    pub(super) fonts: Vec<(TextSize, Hue, Font)>,
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

/// Builds every sprite; returns the atlas pages and where each frame lives.
pub fn build_sprites(s: f32) -> (Vec<Image>, Sprites) {
    let mut atlas = AtlasBuilder::new();
    let line = |w: f32| (w * s).max(1.2);

    // Player: a chevron pointing along +x.
    let ship = [(1.0, 0.0), (-0.75, 0.7), (-0.35, 0.0), (-0.75, -0.7)];
    let mut player = Vec::new();
    let mut ghost = Vec::new();
    let ships = par_map(PLAYER_ANGLES, |i| {
        let angle = i as f32 * TAU / PLAYER_ANGLES as f32;
        neon_sprite(s, 20.0, Hue::Cyan.color(), |l, b, c, r| {
            let pts = rotate(&ship, angle, c, r);
            b.fill_polygon(&pts, 1.0);
            l.stroke_polygon(&pts, line(2.4));
            l.fill_circle(rotate(&[(-0.2, 0.0)], angle, c, r)[0], r * 0.12, 1.0);
        })
    });
    for img in ships {
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
    let sniper = [1.0, 0.45].map(|alpha| {
        (0..BULLET_ANGLES)
            .map(|i| {
                let angle = i as f32 * TAU / BULLET_ANGLES as f32;
                atlas.add(
                    neon_sprite(s, 30.0, Hue::Cyan.color(), |l, _, c, r| {
                        let ends = rotate(&[(-1.0, 0.0), (1.0, 0.0)], angle, c, r);
                        l.line(ends[0], ends[1], line(3.0));
                    })
                    .with_alpha(alpha),
                )
            })
            .collect()
    });

    let rocket = (0..BULLET_ANGLES)
        .map(|i| {
            let angle = i as f32 * TAU / BULLET_ANGLES as f32;
            atlas.add(neon_sprite(s, 17.0, Hue::Orange.color(), |l, b, c, r| {
                let body = rotate(
                    &[
                        (-0.55, -0.22),
                        (0.35, -0.22),
                        (0.95, 0.0),
                        (0.35, 0.22),
                        (-0.55, 0.22),
                    ],
                    angle,
                    c,
                    r,
                );
                b.fill_polygon(&body, 1.0);
                l.stroke_polygon(&body, line(2.2));
                let flame = rotate(&[(-0.5, 0.0), (-1.0, 0.0)], angle, c, r);
                l.line(flame[0], flame[1], line(4.0));
            }))
        })
        .collect();

    let plasma = [1.0, 0.55].map(|alpha| {
        atlas.add(
            neon_sprite(s, 9.0, hex(0x449dff), |l, b, c, r| {
                b.fill_circle(c, r, 1.0);
                l.stroke_circle(c, r, line(2.2));
                l.fill_circle(c, r * 0.55, 1.0);
            })
            .with_alpha(alpha),
        )
    });
    let bfg = (0..13)
        .map(|i| {
            atlas.add(neon_sprite(
                s,
                24.0 + 56.0 * i as f32 / 12.0,
                Hue::Green.color(),
                |l, b, c, r| {
                    b.fill_circle(c, r, 1.0);
                    l.stroke_circle(c, r, line(3.0));
                    l.fill_circle(c, r * 0.45, 1.0);
                    let corona = rotate(&star(7, 0.65), 0.0, c, r * 0.88);
                    l.stroke_polygon(&corona, line(2.0));
                },
            ))
        })
        .collect();

    let ricochet = (0..6)
        .map(|i| {
            atlas.add(neon_sprite(
                s,
                7.0 + 5.0 * i as f32,
                Hue::Yellow.color(),
                |l, b, c, r| {
                    let pts = rotate(&star(4, 0.55), 0.0, c, r);
                    b.fill_polygon(&pts, 0.8);
                    l.stroke_polygon(&pts, line(2.0));
                    l.fill_circle(c, r * 0.25, 1.0);
                },
            ))
        })
        .collect();
    let blades = (0..9)
        .map(|size| {
            (0..16)
                .map(|i| {
                    atlas.add(neon_sprite(
                        s,
                        22.0 + size as f32 * 5.0,
                        Hue::Cyan.color(),
                        |l, b, c, r| {
                            let teeth = rotate(&star(10, 0.62), i as f32 * TAU / 16.0, c, r);
                            b.fill_polygon(&teeth, 0.45);
                            l.stroke_polygon(&teeth, line(2.5));
                            l.stroke_circle(c, r * 0.4, line(2.0));
                        },
                    ))
                })
                .collect()
        })
        .collect();
    let mine = atlas.add(neon_sprite(s, 7.0, Hue::Orange.color(), |l, b, c, r| {
        let pts = rotate(&star(6, 0.7), 0.0, c, r);
        b.fill_polygon(&pts, 0.8);
        l.stroke_polygon(&pts, line(2.0));
        l.fill_circle(c, r * 0.35, 1.0);
    }));
    let fuel = (0..8)
        .map(|i| {
            atlas.add(neon_sprite(
                s,
                10.0 + i as f32 * 5.0,
                Hue::Orange.color(),
                |l, b, c, r| {
                    b.fill_circle(c, r, 0.8);
                    let pts = rotate(&star(5, 0.65), 0.0, c, r);
                    l.stroke_polygon(&pts, line(2.5));
                    l.fill_circle(c, r * 0.45, 1.0);
                },
            ))
        })
        .collect();

    let charged = [(Hue::Cyan, 1.0), (Hue::White, 1.0)].map(|(hue, alpha)| {
        (0..9)
            .map(|i| {
                atlas.add(
                    neon_sprite(s, 8.0 + 3.0 * i as f32, hue.color(), |l, b, c, r| {
                        b.fill_circle(c, r, 0.5);
                        l.stroke_circle(c, r, line(2.5));
                        l.fill_circle(c, r * 0.35, 1.0);
                    })
                    .with_alpha(alpha),
                )
            })
            .collect()
    });

    let laser = [0.5, 1.0, 2.0, 3.0, 6.0].map(|width| {
        (0..128)
            .map(|i| {
                let angle = i as f32 * TAU / 128.0;
                let dir = (angle.cos(), angle.sin());
                let w = ((dir.0.abs() * 16.0 + 18.0) * s).ceil() as u32;
                let h = ((dir.1.abs() * 16.0 + 18.0) * s).ceil() as u32;
                let mut lines = Mask::new(w, h);
                let mut core = Mask::new(w, h);
                let body = Mask::new(w, h);
                let center = (w as f32 * 0.5, h as f32 * 0.5);
                let a = (center.0 - dir.0 * 8.0 * s, center.1 - dir.1 * 8.0 * s);
                let b = (center.0 + dir.0 * 8.0 * s, center.1 + dir.1 * 8.0 * s);
                lines.line(a, b, width * 2.0 * s);
                core.line(a, b, (width * 0.65 * s).max(0.6));
                let mut img = neon(&lines, &body, Hue::Orange.color(), (4.0 * s) as usize, 2.0);
                for (px, coverage) in img.rgba.as_chunks_mut::<4>().0.iter_mut().zip(&core.v) {
                    for (channel, hot) in px[..3].iter_mut().zip([255.0, 250.0, 230.0]) {
                        *channel = (*channel as f32 * (1.0 - coverage) + hot * coverage) as u8;
                    }
                }
                atlas.add(img)
            })
            .collect()
    });

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

    let skirmisher = rotations(
        &mut atlas,
        s,
        18.0,
        Hue::Cyan.color(),
        16,
        TAU,
        &|l, b, c, r, angle| {
            let pts = rotate(
                &[(1.0, 0.0), (-0.85, 0.8), (-0.5, 0.0), (-0.85, -0.8)],
                angle,
                c,
                r,
            );
            b.fill_polygon(&pts, 0.7);
            l.stroke_polygon(&pts, line(2.2));
            let wings = rotate(&[(-0.2, -0.95), (0.5, 0.0), (-0.2, 0.95)], angle, c, r);
            l.line(wings[0], wings[1], line(2.0));
            l.line(wings[1], wings[2], line(2.0));
        },
    );
    let enemy_sniper = rotations(
        &mut atlas,
        s,
        20.0,
        Hue::Red.color(),
        16,
        TAU,
        &|l, b, c, r, angle| {
            let hull = rotate(
                &[(0.3, 0.0), (-0.65, 0.7), (-0.9, 0.0), (-0.65, -0.7)],
                angle,
                c,
                r,
            );
            b.fill_polygon(&hull, 0.8);
            l.stroke_polygon(&hull, line(2.2));
            for y in [-0.2, 0.2] {
                let barrel = rotate(&[(-0.25, y), (1.0, y)], angle, c, r);
                l.line(barrel[0], barrel[1], line(2.2));
            }
        },
    );
    let spinner = rotations(
        &mut atlas,
        s,
        23.0,
        Hue::Violet.color(),
        8,
        TAU / 3.0,
        &|l, b, c, r, angle| {
            l.stroke_circle(c, r * 0.28, line(2.0));
            for arm in 0..3 {
                let blade = rotate(
                    &[(0.15, -0.12), (1.0, -0.35), (0.65, 0.4), (0.4, 0.0)],
                    angle + arm as f32 * TAU / 3.0,
                    c,
                    r,
                );
                b.fill_polygon(&blade, 0.8);
                l.stroke_polygon(&blade, line(2.2));
            }
        },
    );
    let bomber = rotations(
        &mut atlas,
        s,
        28.0,
        Hue::Orange.color(),
        8,
        TAU / 4.0,
        &|l, b, c, r, angle| {
            let hull = rotate(&polygon(4, PI / 4.0), angle, c, r * 0.78);
            b.fill_polygon(&hull, 0.8);
            l.stroke_polygon(&hull, line(2.8));
            for at in rotate(&polygon(4, 0.0), angle, c, r * 0.8) {
                l.stroke_circle(at, r * 0.18, line(2.2));
            }
            l.stroke_circle(c, r * 0.25, line(2.2));
        },
    );
    let splitter = rotations(
        &mut atlas,
        s,
        24.0,
        Hue::Green.color(),
        16,
        TAU,
        &|l, b, c, r, angle| {
            for side in [-1.0, 1.0] {
                let half = rotate(
                    &[
                        (0.8, side * 0.12),
                        (-0.35, side * 0.95),
                        (-0.9, side * 0.12),
                    ],
                    angle,
                    c,
                    r,
                );
                b.fill_polygon(&half, 0.8);
                l.stroke_polygon(&half, line(2.4));
            }
        },
    );
    let sentinel = rotations(
        &mut atlas,
        s,
        27.0,
        Hue::Yellow.color(),
        8,
        TAU / 4.0,
        &|l, b, c, r, angle| {
            let hull = rotate(&polygon(4, 0.0), angle, c, r);
            b.fill_polygon(&hull, 0.7);
            l.stroke_polygon(&hull, line(3.0));
            let shield = rotate(&polygon(4, 0.0), angle, c, r * 0.65);
            l.stroke_polygon(&shield, line(2.2));
            l.fill_circle(c, r * 0.15, 1.0);
        },
    );

    let orb = atlas.add(glow_dot((22.0 * s) as u32, Hue::Red.color(), 0.45));
    let heavy_orb = atlas.add(glow_dot((38.0 * s) as u32, hex(0xff6a2b), 0.4));

    let spark = HUES
        .iter()
        .map(|hue| {
            SPARK_SIZES
                .map(|size| atlas.add(glow_dot(((size * s) as u32).max(3), hue.color(), 0.5)))
        })
        .collect();

    let ring_images = par_map(HUES.len() * RING_FRAMES, |i| {
        let (hue, k) = (HUES[i / RING_FRAMES], i % RING_FRAMES);
        let t = k as f32 / (RING_FRAMES - 1) as f32;
        neon_sprite(s, 60.0, hue.color(), |l, _, c, r| {
            l.stroke_circle(c, r * (0.2 + 0.8 * t), line(7.0 - 5.0 * t));
        })
        .with_alpha(1.0 - 0.75 * t)
    });
    let mut ring_images = ring_images.into_iter();
    let ring = HUES
        .iter()
        .map(|_| {
            (0..RING_FRAMES)
                .map(|_| atlas.add(ring_images.next().unwrap()))
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
    let boss_seg = [
        Hue::Violet,
        Hue::Cyan,
        Hue::Magenta,
        Hue::Orange,
        Hue::Yellow,
    ]
    .map(|hue| atlas.add(segment(hue.color())));

    // Boss: an eight-point shell around a counter-rotating triangle core.
    let boss_shell = rotations(
        &mut atlas,
        s,
        66.0,
        Hue::Violet.color(),
        BOSS_SHELL_FRAMES,
        TAU / 8.0,
        &|l, b, c, r, angle| {
            let outer = rotate(&star(8, 0.74), angle, c, r);
            let ring = rotate(&polygon(8, PI / 8.0), angle, c, r * 0.56);
            b.fill_polygon(&outer, 0.8);
            l.stroke_polygon(&outer, line(3.2));
            l.stroke_polygon(&ring, line(2.2));
        },
    );
    // One shell frame per new boss keeps the atlas within terminal image quotas.
    let boss_crests = [Hue::Cyan, Hue::Magenta, Hue::Orange, Hue::Yellow].map(|hue| {
        rotations(
            &mut atlas,
            s,
            66.0,
            hue.color(),
            1,
            TAU,
            &|l, b, c, r, _| match hue {
                Hue::Cyan => {
                    for side in [-1.0, 1.0] {
                        let wing = rotate(
                            &[
                                (0.0, side * 0.2),
                                (0.65, side * 0.35),
                                (0.95, side * 0.75),
                                (0.15, side * 0.95),
                                (-0.8, side * 0.6),
                                (-0.6, side * 0.2),
                            ],
                            0.0,
                            c,
                            r,
                        );
                        b.fill_polygon(&wing, 0.7);
                        l.stroke_polygon(&wing, line(3.0));
                    }
                }
                Hue::Magenta => {
                    for petal in 0..6 {
                        let pts = rotate(
                            &[(0.25, 0.0), (0.65, -0.3), (1.0, 0.0), (0.65, 0.3)],
                            petal as f32 * TAU / 6.0,
                            c,
                            r,
                        );
                        b.fill_polygon(&pts, 0.75);
                        l.stroke_polygon(&pts, line(2.8));
                    }
                }
                Hue::Orange => {
                    for arm in 0..4 {
                        let rail = rotate(
                            &[(0.25, -0.23), (0.95, -0.23), (0.95, 0.23), (0.25, 0.23)],
                            arm as f32 * TAU / 4.0,
                            c,
                            r,
                        );
                        b.fill_polygon(&rail, 0.8);
                        l.stroke_polygon(&rail, line(3.0));
                    }
                    l.stroke_circle(c, r * 0.62, line(2.2));
                }
                _ => {
                    l.stroke_circle(c, r * 0.88, line(4.0));
                    l.stroke_circle(c, r * 0.68, line(2.2));
                    for ray in 0..12 {
                        let ends = rotate(&[(0.8, 0.0), (1.0, 0.0)], ray as f32 * TAU / 12.0, c, r);
                        l.line(ends[0], ends[1], line(3.2));
                    }
                }
            },
        )
    });
    let boss_core = rotations(
        &mut atlas,
        s,
        26.0,
        Hue::Magenta.color(),
        BOSS_CORE_FRAMES,
        TAU / 3.0,
        &|l, b, c, r, angle| {
            let tri = rotate(&polygon(3, 0.0), angle, c, r);
            b.fill_polygon(&tri, 1.0);
            l.stroke_polygon(&tri, line(2.6));
            l.fill_circle(c, r * 0.22, 1.0);
        },
    );

    // Elite aura: a dashed gold ring that turns slowly.
    let aura = [30.0, 46.0].map(|radius| {
        rotations(
            &mut atlas,
            s,
            radius,
            hex(0xffc94d),
            AURA_FRAMES,
            TAU / AURA_DASHES as f32,
            &|l, _, c, r, angle| {
                for k in 0..AURA_DASHES {
                    let a0 = angle + k as f32 * TAU / AURA_DASHES as f32;
                    let ends = rotate(&[(1.0, 0.0), ((0.35f32).cos(), (0.35f32).sin())], a0, c, r);
                    l.line(ends[0], ends[1], line(2.4));
                }
            },
        )
    });
    let shield = atlas.add(neon_sprite(s, 9.0, Hue::Cyan.color(), |l, b, c, r| {
        b.fill_circle(c, r, 1.0);
        l.stroke_circle(c, r * 0.85, line(2.2));
        l.fill_circle(c, r * 0.35, 1.0);
    }));

    let mote = [Hue::Cyan, Hue::Violet]
        .map(|h| atlas.add(glow_dot((7.0 * s) as u32, h.color(), 0.2).with_alpha(0.45)));

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
        sniper,
        rocket,
        plasma,
        bfg,
        ricochet,
        blades,
        mine,
        fuel,
        charged,
        laser,
        chaser,
        swarm,
        shooter,
        tank,
        skirmisher,
        enemy_sniper,
        spinner,
        bomber,
        splitter,
        sentinel,
        orb,
        heavy_orb,
        spark,
        ring,
        heart,
        bolt,
        bullet_faded,
        hud_heart,
        heat,
        heat_off,
        boss_shell,
        boss_crests,
        boss_core,
        boss_seg,
        mote,
        aura,
        shield,
        fonts: FONT_STYLES
            .iter()
            .map(|&(size, hue)| (size, hue, build_font(&mut atlas, s, size, hue)))
            .collect(),
    };
    (atlas.build(), sprites)
}
