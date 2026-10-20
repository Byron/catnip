//! Text and menus: HUD, banners, title, pause, game over, upgrade cards.

use super::*;

impl Renderer {
    pub(super) fn text(
        &mut self,
        out: &mut Vec<u8>,
        game: &Game,
        sprites: &Sprites,
        win: Window,
        stats: &Stats,
        scale: f32,
    ) {
        for row in self.text_rows.drain(..) {
            // Spaces, not erase-line: WezTerm paints erased cells opaque
            // over images.
            let _ = write!(out, "\x1b[{row};1H{:width$}", "", width = win.cols as usize);
        }
        let mut text = Text {
            out,
            rows: &mut self.text_rows,
            placer: &mut self.atlas,
            sprites,
            win,
            scale,
        };

        if matches!(
            game.state,
            State::Playing | State::Paused | State::Dying | State::Upgrade
        ) {
            let hud_rows = if game.wave_modifier == crate::game::WaveModifier::Calm {
                2
            } else {
                3
            };
            let _ = write!(
                text.out,
                "\x1b[1;1H\x1b_Ga=p,i={SHADE_ID},p=1,c={},r={hud_rows},z={Z_SHADE},C=1,q=2\x1b\\",
                win.cols
            );
            text.hud(game, stats, self.sprites_drawn);
            if let Some(b) = &game.boss {
                text.boss_bar(b);
            }
        } else {
            kitty::delete_placement(text.out, SHADE_ID, 1);
        }
        // Dim the arena behind menus so their text stays readable.
        let dim = matches!(game.state, State::Paused | State::GameOver | State::Upgrade);
        if dim {
            let _ = write!(
                text.out,
                "\x1b[1;1H\x1b_Ga=p,i={SHADE_ID},p=2,c={},r={},z={Z_DIM},C=1,q=2\x1b\\",
                win.cols, win.rows
            );
        } else if self.dimmed {
            kitty::delete_placement(text.out, SHADE_ID, 2);
        }
        self.dimmed = dim;
        if game.state == State::Upgrade {
            text.cards(game);
            self.cards_shown = true;
        } else if self.cards_shown {
            for id in [10, 11, 12] {
                kitty::delete_placement(text.out, SHADE_ID, id);
            }
            kitty::delete_placement(text.out, HIGHLIGHT_ID, 2);
            self.cards_shown = false;
        }
        let row = |fraction: f32| ((win.rows as f32 * fraction) as u32).max(2);
        match game.state {
            State::Title => {
                let fits = sprites.font(TextSize::Large, Hue::Cyan).width("GLOWSTORM")
                    < win.width as f32 * 0.9;
                let size = if fits {
                    TextSize::Large
                } else {
                    TextSize::Medium
                };
                let r = row(0.22) + text.big(row(0.22), size, Hue::Cyan, "GLOWSTORM") + 2;
                text.line(
                    r,
                    "38;2;200;210;255",
                    "WASD move · Shift thrust · mouse aim · left fire · wheel / 1–0 weapon",
                );
                text.line(
                    r + 1,
                    "38;2;200;210;255",
                    "Tap right / Space evade · hold for star-laser · middle / E charge weapon · release to fire",
                );
                let help_rows = if win.rows >= 36 {
                    text.line(
                        r + 2,
                        "38;2;200;210;255",
                        "1 gun · 2 sniper · 3 rocket · 4 plasma · 5 gravity",
                    );
                    text.line(
                        r + 3,
                        "38;2;200;210;255",
                        "6 ricochet · 7 lightning · 8 blade · 9 mines · 0 flame",
                    );
                    5
                } else {
                    3
                };
                if self.ghostty {
                    text.line(
                        r + help_rows - 1,
                        "38;2;255;225;77",
                        "Ghostty: use Space if right-click interferes with firing",
                    );
                }
                // Difficulty menu: the selected one is yellow.
                let mut r = r + help_rows;
                for d in Difficulty::ALL {
                    let hue = if d == game.difficulty {
                        Hue::Yellow
                    } else {
                        Hue::White
                    };
                    r += text.big(r, TextSize::Small, hue, d.name());
                }
                text.line(r + 1, "38;2;200;210;255", game.difficulty.blurb());
                if game.best > 0 {
                    text.line(
                        r + 2,
                        "38;2;140;150;190",
                        &format!("best {}", group(game.best)),
                    );
                }
                text.line(
                    r + 4,
                    "1;38;2;255;225;77",
                    "↑/↓ choose · click or Enter to start",
                );
                text.line(win.rows, "38;2;90;100;140", "Esc pause · Q quit");
            }
            State::Playing
                if game.boss_pending > 0.0
                    || game.boss.as_ref().is_some_and(|b| b.entering > 0.0) =>
            {
                if (game.time * 4.0) as i32 % 2 == 0 {
                    text.big(row(0.18), TextSize::Medium, Hue::Red, "WARNING");
                }
                let kind = game
                    .boss
                    .as_ref()
                    .map_or_else(|| BossKind::for_level(game.wave / 5), |b| b.kind);
                text.big(row(0.3), TextSize::Small, Hue::White, kind.name());
            }
            State::Playing if game.wave_clear > 0.0 => {
                text.big(row(0.2), TextSize::Medium, Hue::Cyan, "WAVE CLEAR");
            }
            State::Playing if game.banner > 0.0 => {
                let r = row(0.2)
                    + text.big(
                        row(0.2),
                        TextSize::Medium,
                        Hue::Yellow,
                        &format!("WAVE {}", game.wave),
                    );
                if game.wave_modifier != crate::game::WaveModifier::Calm {
                    text.line(r + 1, "1;38;2;255;59;78", game.wave_modifier.name());
                    for (i, line) in wrap(
                        game.wave_modifier.blurb(),
                        win.cols.saturating_sub(2) as usize,
                    )
                    .iter()
                    .enumerate()
                    {
                        text.line(r + 3 + i as u32, "38;2;200;210;255", line);
                    }
                }
            }
            State::Paused => {
                let r = row(0.3) + text.big(row(0.3), TextSize::Medium, Hue::Cyan, "PAUSED") + 1;
                text.line(r, "38;2;200;210;255", "Esc or click to resume · Q to quit");
            }
            State::GameOver => {
                let mut r =
                    row(0.22) + text.big(row(0.22), TextSize::Medium, Hue::Red, "GAME OVER") + 1;
                r += text.big(r, TextSize::Small, Hue::White, &group(game.score)) + 1;
                if game.new_best {
                    r += text.big(r, TextSize::Small, Hue::Yellow, "NEW BEST!");
                } else {
                    text.line(r, "38;2;255;225;77", &format!("best {}", group(game.best)));
                }
                text.line(
                    r + 1,
                    "38;2;200;210;255",
                    &format!("reached wave {}", game.wave),
                );
                text.line(
                    r + 3,
                    "38;2;140;150;190",
                    &format!(
                        "{} · click or R to restart · Esc menu · Q quit",
                        game.difficulty.name()
                    ),
                );
            }
            _ => {}
        }
    }
}

pub(super) struct Text<'a> {
    pub(super) out: &'a mut Vec<u8>,
    pub(super) rows: &'a mut Vec<u32>,
    pub(super) placer: &'a mut AtlasPlacer,
    pub(super) sprites: &'a Sprites,
    pub(super) win: Window,
    pub(super) scale: f32,
}

impl Text<'_> {
    pub(super) fn centered_col(&self, width: u32) -> u32 {
        self.win.cols.saturating_sub(width) / 2 + 1
    }

    pub(super) fn line(&mut self, row: u32, sgr: &str, s: &str) {
        let col = self.centered_col(s.chars().count() as u32);
        let _ = write!(self.out, "\x1b[{row};{col}H\x1b[{sgr}m{s}\x1b[0m");
        self.rows.push(row);
    }

    /// Plain text with its first character at (row, col).
    pub(super) fn at(&mut self, row: u32, col: u32, sgr: &str, s: &str) {
        let _ = write!(self.out, "\x1b[{row};{col}H\x1b[{sgr}m{s}\x1b[0m");
        self.rows.push(row);
    }

    /// Neon sprite text centered on the window, top edge at `row`.
    /// Returns how many terminal rows it covers.
    pub(super) fn big(&mut self, row: u32, size: TextSize, hue: Hue, s: &str) -> u32 {
        let center = self.win.width as f32 / 2.0;
        self.big_at(row, center, size, hue, s)
    }

    /// Like `big`, centered on window pixel column `center`.
    pub(super) fn big_at(
        &mut self,
        row: u32,
        center: f32,
        size: TextSize,
        hue: Hue,
        s: &str,
    ) -> u32 {
        let font = self.sprites.font(size, hue);
        let cell_h = self.win.cell().1.max(1);
        let top = (row.saturating_sub(1) * cell_h) as f32;
        let mut x = center - font.width(s) / 2.0;
        for c in s.chars() {
            if let Some(frame) = font.glyph(c) {
                let at = (
                    (x - font.pad).round() as i32,
                    (top - font.pad).round() as i32,
                );
                self.placer.place(self.out, frame, at, Z_TEXT);
            }
            x += font.advance;
        }
        (font.height / cell_h as f32).ceil() as u32
    }

    /// The upgrade screen: three cards, the highlighted one lit up.
    pub(super) fn cards(&mut self, game: &Game) {
        const CARD_ROWS: u32 = 11;
        let (cell_w, _) = self.win.cell();
        let width = self.win.width as f32;
        let top = ((self.win.rows as f32 * 0.18) as u32).max(3);
        let r = top + self.big(top, TextSize::Medium, Hue::Yellow, "CHOOSE AN UPGRADE") + 2;
        let n = game.offer.len();
        for (i, &upgrade) in game.offer.iter().enumerate() {
            let (left, right) = card_span(i, n, width);
            let col = (left / cell_w as f32) as u32 + 1;
            let cols = ((right - left) / cell_w as f32) as u32;
            let picked = i == game.offer_pick;
            let _ = write!(
                self.out,
                "\x1b[{r};{col}H\x1b_Ga=p,i={SHADE_ID},p={},c={cols},r={CARD_ROWS},z={Z_CARD},C=1,q=2\x1b\\",
                10 + i
            );
            if picked {
                let _ = write!(
                    self.out,
                    "\x1b[{r};{col}H\x1b_Ga=p,i={HIGHLIGHT_ID},p=2,c={cols},r={CARD_ROWS},z={},C=1,q=2\x1b\\",
                    Z_CARD + 1
                );
            }
            let center = (left + right) / 2.0;
            let hue = if picked { Hue::Yellow } else { Hue::White };
            // Long names drop to the tiny font on narrow windows.
            let size = if self
                .sprites
                .font(TextSize::Small, hue)
                .width(upgrade.name())
                < (right - left) * 0.9
            {
                TextSize::Small
            } else {
                TextSize::Tiny
            };
            let key = format!("{}", i + 1);
            self.at(r + 1, col + cols / 2, "38;2;140;150;190", &key);
            let name_rows = self.big_at(r + 3, center, size, hue, upgrade.name());
            let level = game.player.upgrades.level(upgrade);
            let status = if crate::game::Upgrade::TROPHIES.contains(&upgrade) {
                "BOSS TROPHY".to_string()
            } else if level == 0 {
                "NEW".to_string()
            } else {
                format!("LEVEL {} → {}", level, level + 1)
            };
            let status_col = col + cols.saturating_sub(status.chars().count() as u32) / 2;
            self.at(r + 4 + name_rows, status_col, "38;2;93;255;106", &status);
            for (k, line) in wrap(upgrade.blurb(), cols.saturating_sub(4) as usize)
                .iter()
                .enumerate()
            {
                let c = col + cols.saturating_sub(line.chars().count() as u32) / 2;
                let sgr = if picked {
                    "1;38;2;255;255;255"
                } else {
                    "38;2;200;210;255"
                };
                self.at(r + 6 + name_rows + k as u32, c, sgr, line);
            }
        }
        self.line(
            r + CARD_ROWS + 1,
            "1;38;2;255;225;77",
            "1 / 2 / 3, ← →, or click a card",
        );
    }

    /// Boss name and a segmented health bar under the HUD row.
    pub(super) fn boss_bar(&mut self, boss: &crate::game::Boss) {
        const SEGMENTS: usize = 48;
        let hue = if boss.kind == BossKind::Prism {
            Hue::Violet
        } else {
            Hue::White
        };
        let r = self.big(3, TextSize::Small, hue, boss.kind.name());
        let cell_h = self.win.cell().1 as f32;
        let y = (3 + r) as f32 * cell_h - cell_h * 0.3;
        let seg = self.sprites.boss_seg[boss.kind as usize];
        let spacing = 10.0 * self.scale;
        let left = self.win.width as f32 / 2.0 - spacing * (SEGMENTS as f32 - 1.0) / 2.0;
        // The bar fills up during the entrance.
        let shown = if boss.entering > 0.0 {
            1.0 - boss.entering / 2.4
        } else {
            boss.hp / boss.max_hp
        };
        let lit = (shown * SEGMENTS as f32).ceil() as usize;
        for i in 0..SEGMENTS {
            let frame = if i >= lit {
                self.sprites.heat_off
            } else if boss.phase() >= 3 {
                self.sprites.heat[3]
            } else {
                seg
            };
            let x = left + i as f32 * spacing - frame.rect.w as f32 / 2.0;
            let at = (
                x.round() as i32,
                (y - frame.rect.h as f32 / 2.0).round() as i32,
            );
            self.placer.place(self.out, frame, at, Z_TEXT);
        }
    }

    pub(super) fn hud(&mut self, game: &Game, stats: &Stats, sprites: u32) {
        let p = &game.player;
        // Lives are sprites: heart glyphs render differently (or in emoji
        // colors) depending on the terminal's fonts.
        let (cell_w, cell_h) = self.win.cell();
        let spacing = 20.0 * self.scale;
        let center_y = cell_h as f32 / 2.0;
        let mut x = cell_w as f32 + spacing / 2.0;
        for i in 0..p.max_hp {
            let frame = self.sprites.hud_heart[usize::from(i >= p.hp)];
            let at = (
                (x - frame.rect.w as f32 / 2.0).round() as i32,
                (center_y - frame.rect.h as f32 / 2.0).round() as i32,
            );
            self.placer.place(self.out, frame, at, Z_TEXT);
            x += spacing;
        }
        let text_col = (x / cell_w as f32).ceil() as u32 + 1;
        let mut left = String::new();
        let _ = write!(left, "\x1b[1;{text_col}H");
        let _ = write!(left, "\x1b[1;97mSCORE {}", group(game.score));
        if game.multiplier() > 1 {
            let _ = write!(left, "  \x1b[38;2;255;61;240m×{}", game.multiplier());
        }
        let _ = write!(left, "  \x1b[38;2;61;242;255mWAVE {}", game.wave);
        if game.power() > 1.0 {
            let _ = write!(
                left,
                "  \x1b[38;2;93;255;106mPOWER +{:.0}%",
                (game.power() - 1.0) * 100.0
            );
        }
        if game.difficulty != Difficulty::Medium {
            let _ = write!(left, "  \x1b[38;2;255;59;78m{}", game.difficulty.name());
        }
        if p.rapid_t > 0.0 {
            let _ = write!(left, "  \x1b[38;2;255;225;77mRAPID");
            if !game.dev.rapid_fire {
                let _ = write!(left, " {:.1}s", p.rapid_t);
            }
        }
        let charges = if p.max_dash_charges() > 1 {
            format!(" x{}", p.dash_charges)
        } else {
            String::new()
        };
        if p.spin.is_some() {
            let _ = write!(left, "  \x1b[1;38;2;61;242;255mSPIN\x1b[0m");
        } else if p.dash_charges > 0 {
            let _ = write!(left, "  \x1b[1;38;2;61;242;255mDASH{charges}\x1b[0m");
        } else {
            let _ = write!(left, "  \x1b[38;2;60;80;105mDASH {:.1}s\x1b[0m", p.dash_cd);
        }
        self.out.extend_from_slice(left.as_bytes());

        let right = format!(
            "{:.0} fps · {} sprites · {:.1} ms · {:.0} KB/frame",
            stats.fps, sprites, stats.frame_ms, stats.kb_per_frame
        );
        let col = self.win.cols.saturating_sub(right.chars().count() as u32);
        let _ = write!(self.out, "\x1b[1;{col}H\x1b[38;2;90;100;140m{right}\x1b[0m");
        self.rows.push(1);

        // A dedicated row keeps the selection clear even when HUD stats crowd row one.
        let _ = write!(
            self.out,
            "\x1b[2;1H{:width$}",
            "",
            width = self.win.cols as usize
        );
        let available = self.win.cols.saturating_sub(2) as usize;
        let mut selected = format!("WEAPON [{}]", p.weapon.name());
        if selected.len() > available {
            selected = format!("[{}]", p.weapon.name());
        }
        selected.truncate(available);
        self.at(2, 2, "1;38;2;255;225;77", &selected);
        let hint = "wheel / 1-0 to switch";
        let hint_col = selected.len() as u32 + 4;
        if hint_col + hint.len() as u32 <= self.win.cols {
            self.at(2, hint_col, "38;2;140;150;190", hint);
        }
        if game.wave_modifier != crate::game::WaveModifier::Calm {
            let label = if game.wave_modifier == crate::game::WaveModifier::Crossfire
                && game.banner <= 0.0
                && game.wave_clear <= 0.0
                && game.modifier_time % 4.0 < 1.0
            {
                if game.modifier_time % 8.0 < 4.0 {
                    "CROSSFIRE: TOP INCOMING"
                } else {
                    "CROSSFIRE: RIGHT INCOMING"
                }
            } else {
                game.wave_modifier.name()
            };
            self.line(3, "1;38;2;255;59;78", label);
        }
    }
}

/// Greedy word wrap to `width` characters.
pub(super) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split(' ') {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width.max(8) => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// 12345 → "12,345"
pub(super) fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Weapon;

    #[test]
    fn group_inserts_thousands_separators() {
        assert_eq!(super::group(0), "0");
        assert_eq!(super::group(999), "999");
        assert_eq!(super::group(12345), "12,345");
        assert_eq!(super::group(1234567), "1,234,567");
    }

    #[test]
    fn modifier_warning_wraps_and_clears_after_introduction() {
        let (atlas, sprites) = art::build_sprites(0.3);
        let stats = Stats {
            fps: 60.0,
            frame_ms: 1.0,
            kb_per_frame: 1.0,
        };
        for cols in [30, 120] {
            let win = Window {
                cols,
                rows: 40,
                width: cols * 8,
                height: 640,
            };
            let mut renderer = Renderer::new((100, 100), atlas.len());
            let mut game = Game::new(42);
            game.state = State::Playing;
            game.wave = 6;
            for modifier in [
                crate::game::WaveModifier::Reinforcements,
                crate::game::WaveModifier::Volatile,
                crate::game::WaveModifier::Crossfire,
            ] {
                game.wave_modifier = modifier;
                game.banner = 2.0;
                let mut out = Vec::new();
                renderer.frame(&mut out, &game, &sprites, 0.3, win, &stats);
                let text = std::str::from_utf8(&out).unwrap();
                assert!(text.contains(modifier.name()));
                for line in wrap(modifier.blurb(), cols.saturating_sub(2) as usize) {
                    assert!(line.len() < cols as usize);
                    assert!(text.contains(&line));
                }
                game.banner = 0.0;
                out.clear();
                renderer.frame(&mut out, &game, &sprites, 0.3, win, &stats);
                let text = std::str::from_utf8(&out).unwrap();
                assert!(text.contains(modifier.name()), "modifier stays in the HUD");
                assert!(!text.contains("Watch the spawn rings."));
                assert!(text.contains(&format!("r=3,z={Z_SHADE}")));
                if modifier == crate::game::WaveModifier::Crossfire {
                    assert!(text.contains("CROSSFIRE: TOP INCOMING"));
                    game.modifier_time = 4.5;
                    out.clear();
                    renderer.frame(&mut out, &game, &sprites, 0.3, win, &stats);
                    assert!(
                        std::str::from_utf8(&out)
                            .unwrap()
                            .contains("CROSSFIRE: RIGHT INCOMING")
                    );
                }
            }
        }
    }

    #[test]
    fn weapon_row_stays_visible_and_clears_when_switching_or_leaving_play() {
        let (atlas, sprites) = art::build_sprites(0.3);
        let stats = Stats {
            fps: 144.0,
            frame_ms: 6.9,
            kb_per_frame: 999.0,
        };
        let hint = "wheel / 1-0 to switch";
        for (cols, long, short) in [
            (120, "WEAPON [7 LIGHTNING]", "WEAPON [1 GUN]"),
            (30, "WEAPON [7 LIGHTNING]", "WEAPON [1 GUN]"),
            (16, "[7 LIGHTNING]", "WEAPON [1 GUN]"),
            (8, "[7 LIG", "[1 GUN"),
        ] {
            let win = Window {
                cols,
                rows: 40,
                width: cols * 8,
                height: 640,
            };
            let blank_row = format!("\x1b[2;1H{}", " ".repeat(cols as usize));
            let selected_at = "\x1b[2;2H\x1b[1;38;2;255;225;77m";
            let mut renderer = Renderer::new((100, 100), atlas.len());
            let mut game = Game::new(42);
            game.state = State::Playing;
            game.score = u64::MAX;
            game.wave = u32::MAX;
            game.player.rapid_t = 10.0;
            let mut out = Vec::new();
            for (weapon, expected) in [(Weapon::Lightning, long), (Weapon::Default, short)] {
                game.player.weapon = weapon;
                out.clear();
                renderer.frame(&mut out, &game, &sprites, 0.3, win, &stats);
                let frame = std::str::from_utf8(&out).unwrap();
                let selected = frame
                    .split_once(selected_at)
                    .expect("selection has its own row and highlight")
                    .1
                    .split_once("\x1b[0m")
                    .unwrap()
                    .0;
                assert_eq!(selected, expected, "{cols} columns");
                assert!(selected.len() + 1 < cols as usize, "selection wraps");
                assert_eq!(frame.contains(hint), cols == 120, "{cols} columns");
                assert!(frame.contains(&format!("{blank_row}{selected_at}")));
                assert!(frame.rfind(&blank_row).unwrap() > frame.find("KB/frame").unwrap());
                assert!(frame.contains(&format!("p=1,c={cols},r=2,z={Z_SHADE}")));
            }
            game.state = State::Title;
            out.clear();
            renderer.frame(&mut out, &game, &sprites, 0.3, win, &stats);
            let frame = std::str::from_utf8(&out).unwrap();
            assert!(
                frame.contains(&blank_row),
                "old selection survives the menu"
            );
            assert!(!frame.contains(selected_at));
        }
    }
}
