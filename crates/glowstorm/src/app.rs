//! The frame loop: read input, step the game, play sounds, draw, measure.

use std::{
    collections::HashSet,
    io::{self, Write},
    time::{Duration, Instant},
};

use catnip::{gfx::Image, kitty, term};
use crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, ModifierKeyCode, MouseButton, MouseEventKind,
};

use crate::{
    art::{self, Sprites},
    game::{Game, Input, View, v2},
    render::{ATLAS_ID, FLASH_ID, FLOOR_ID, Renderer, SHADE_ID, Stats, VOID_ID},
    sound::Audio,
};

/// Why the frame loop ended.
pub enum Exit {
    Quit,
    /// The terminal doesn't report key releases (kitty keyboard protocol off).
    NeedsKeyboardProtocol,
}

pub fn run(
    mut win: term::Window,
    mut scale: f32,
    atlas: Vec<Image>,
    floor: Image,
    mut sprites: Sprites,
    audio: Option<&Audio>,
    options: &crate::Options,
) -> io::Result<Exit> {
    let frame_time = Duration::from_secs_f64(1.0 / f64::from(options.fps));
    let mut out = io::BufWriter::with_capacity(64 * 1024, io::stdout().lock());
    let transport = options.graphics_transport;
    upload_images(&mut out, &atlas, &floor, transport)?;
    // Fail with a clear message instead of running with invisible sprites.
    kitty::verify(&mut out, Duration::from_secs(5)).map_err(io::Error::other)?;
    // Smooth movement needs key releases.
    if !term::keyboard_protocol_active(Duration::from_millis(800)) {
        return Ok(Exit::NeedsKeyboardProtocol);
    }
    let mut atlas_bytes = atlas.iter().map(|page| page.rgba.len()).sum();
    let mut atlas_pages = atlas.len();
    let mut renderer = Renderer::new((floor.w, floor.h), atlas_pages);
    drop((atlas, floor));

    eprintln!("seed: {}", options.seed);
    let mut game = Game::new(options.seed);
    game.dev = options.dev;
    let mut input = Input::default();
    let mut meter = Meter::new();
    let mut frame = Vec::with_capacity(1 << 18);
    let mut last = Instant::now();
    let mut deadline = Instant::now();

    loop {
        // Input until the frame deadline.
        while let Some(event) =
            term::read_event(deadline.saturating_duration_since(Instant::now()))?
        {
            match event {
                // Keep the old size if the query fails mid-resize.
                Event::Resize(..) => win = term::window_size().unwrap_or(win),
                event => {
                    if handle(event, &mut input, &mut game) == Flow::Quit {
                        return Ok(Exit::Quit);
                    }
                }
            }
        }
        let now = Instant::now();
        deadline = (deadline + frame_time).max(now);
        let dt = (now - last)
            .as_secs_f32()
            .min(frame_time.as_secs_f32().max(1.0 / 30.0));
        last = now;
        let started = Instant::now();

        let mut view = View::new(win, scale);
        let size = view.arena_size() * scale;
        if !art::floor_fits((size.x as u32, size.y as u32), atlas_bytes) {
            let (new_scale, atlas, new_sprites) = art::build_fitting_sprites(win, scale)?;
            let mut bytes = Vec::new();
            kitty::delete_image(&mut bytes, FLOOR_ID);
            for i in 0..atlas_pages {
                kitty::delete_image(&mut bytes, ATLAS_ID + i as u32);
            }
            out.write_all(&bytes)?;
            for (i, page) in atlas.iter().enumerate() {
                transport.upload(&mut out, ATLAS_ID + i as u32, page.w, page.h, &page.rgba)?;
            }
            kitty::verify(&mut out, Duration::from_secs(5)).map_err(io::Error::other)?;
            atlas_bytes = atlas.iter().map(|page| page.rgba.len()).sum();
            atlas_pages = atlas.len();
            renderer.replace_atlas(atlas_pages);
            renderer.floor_size = (0, 0);
            scale = new_scale;
            sprites = new_sprites;
            view = View::new(win, scale);
        }
        game.update(dt, &input, &view);
        let size = game.arena_size * scale;
        if renderer.floor_size != (size.x as u32, size.y as u32) {
            let floor = art::build_floor(scale, game.arena_size.x, game.arena_size.y);
            transport.upload(&mut out, FLOOR_ID, floor.w, floor.h, &floor.rgba)?;
            kitty::verify(&mut out, Duration::from_secs(5)).map_err(io::Error::other)?;
            renderer.floor_size = (floor.w, floor.h);
        }
        input.end_frame();
        play_sounds(&mut game, audio);

        frame.clear();
        renderer.frame(&mut frame, &game, &sprites, scale, win, meter.stats());
        out.write_all(&frame)?;
        out.flush()?;
        meter.record(started.elapsed(), frame.len());
    }
}

/// The atlas pages, floor and overlay pixels are uploaded once. Direct uploads
/// stream to the writer without keeping all the base64 data in memory at once.
fn upload_images(
    out: &mut impl Write,
    atlas: &[Image],
    floor: &Image,
    transport: kitty::Transport,
) -> io::Result<()> {
    for (i, page) in atlas.iter().enumerate() {
        transport.upload(out, ATLAS_ID + i as u32, page.w, page.h, &page.rgba)?;
    }
    transport.upload(out, FLOOR_ID, floor.w, floor.h, &floor.rgba)?;
    transport.upload(out, SHADE_ID, 1, 1, &[4, 5, 12, 170])?;
    transport.upload(out, FLASH_ID, 1, 1, &[255, 255, 255, 60])?;
    transport.upload(out, VOID_ID, 1, 1, &[3, 4, 10, 255])?;
    out.flush()
}

#[derive(PartialEq, Eq)]
enum Flow {
    Continue,
    Quit,
}

fn handle(event: Event, input: &mut Input, game: &mut Game) -> Flow {
    match event {
        Event::Key(key) => {
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                other => other,
            };
            match key.kind {
                KeyEventKind::Release => {
                    input.held.remove(&code);
                }
                KeyEventKind::Repeat => {
                    input.held.insert(code);
                }
                KeyEventKind::Press => {
                    let ctrl_c =
                        code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
                    if ctrl_c || (code == KeyCode::Char('q') && game.quit_allowed()) {
                        return Flow::Quit;
                    }
                    if code == KeyCode::Char('q') {
                        game.pause();
                        return Flow::Continue;
                    }
                    input.held.insert(code);
                    input.pressed.push(code);
                }
            }
            let shift_keys = [
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                KeyCode::Modifier(ModifierKeyCode::RightShift),
            ];
            input.boost = shift_keys.iter().any(|code| input.held.contains(code))
                || (!shift_keys.contains(&code) && key.modifiers.contains(KeyModifiers::SHIFT));
        }
        Event::Mouse(mouse) => {
            if matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) {
                input.mouse_moved = true;
            }
            // Pixel mode (1016): crossterm subtracts 1 from what it thinks
            // are 1-based cells; undo that to get kitty's pixel position.
            input.mouse = Some(v2(
                f32::from(mouse.column.wrapping_add(1)),
                f32::from(mouse.row.wrapping_add(1)),
            ));
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    input.fire = true;
                    input.clicked = true;
                }
                MouseEventKind::Up(MouseButton::Left) => input.fire = false,
                MouseEventKind::Down(MouseButton::Middle) => {
                    input.middle_clicked |= !input.charge;
                    input.charge = true;
                }
                MouseEventKind::Up(MouseButton::Middle) => input.charge = false,
                MouseEventKind::Down(MouseButton::Right) => {
                    input.spin = true;
                    input.dash = true;
                }
                MouseEventKind::Up(MouseButton::Right) => input.spin = false,
                MouseEventKind::ScrollDown => {
                    input.weapon_scroll = input.weapon_scroll.saturating_add(1);
                }
                MouseEventKind::ScrollUp => {
                    input.weapon_scroll = input.weapon_scroll.saturating_sub(1);
                }
                _ => {}
            }
        }
        Event::FocusLost => {
            input.held.clear();
            input.fire = false;
            input.charge = false;
            input.middle_clicked = false;
            input.spin = false;
            input.boost = false;
            input.weapon_scroll = 0;
            game.on_focus_lost();
        }
        _ => {}
    }
    Flow::Continue
}

/// Plays this frame's sounds, each at most once.
fn play_sounds(game: &mut Game, audio: Option<&Audio>) {
    let Some(audio) = audio else {
        game.sfx.clear();
        return;
    };
    let mut played = HashSet::new();
    for sfx in game.sfx.drain(..) {
        if played.insert(sfx) {
            audio.play(sfx);
        }
    }
}

/// Frame timing for the HUD readout, averaged over half-second windows.
struct Meter {
    window_start: Instant,
    frames: u32,
    work: Duration,
    bytes: usize,
    stats: Stats,
}

impl Meter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            frames: 0,
            work: Duration::ZERO,
            bytes: 0,
            stats: Stats {
                fps: 0.0,
                frame_ms: 0.0,
                kb_per_frame: 0.0,
            },
        }
    }

    fn stats(&self) -> &Stats {
        &self.stats
    }

    fn record(&mut self, work: Duration, bytes: usize) {
        self.frames += 1;
        self.work += work;
        self.bytes += bytes;
        let elapsed = self.window_start.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            let frames = self.frames as f32;
            self.stats = Stats {
                fps: frames / elapsed,
                frame_ms: self.work.as_secs_f32() * 1000.0 / frames,
                kb_per_frame: self.bytes as f32 / frames / 1024.0,
            };
            self.window_start = Instant::now();
            (self.frames, self.work, self.bytes) = (0, Duration::ZERO, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, MouseEvent};

    #[test]
    fn shift_movement_allows_turning_and_firing_in_every_direction() {
        let view = View {
            scale: 1.0,
            size: v2(1600.0, 900.0),
        };
        for keys in ["w", "a", "s", "d", "wa", "wd", "sa", "sd"] {
            let mut game = Game::new(7);
            game.state = crate::game::State::Playing;
            let mut input = Input::default();
            for key in keys.chars() {
                handle(
                    Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::SHIFT)),
                    &mut input,
                    &mut game,
                );
            }
            for (column, row, angle) in [(1400, 750, 0.0), (1200, 950, std::f32::consts::FRAC_PI_2)]
            {
                game.player.pos = v2(1200.0, 750.0);
                game.cam = v2(0.0, 0.0);
                let mouse = |kind| {
                    Event::Mouse(MouseEvent {
                        kind,
                        column: column - 1,
                        row: row - 1,
                        modifiers: KeyModifiers::SHIFT,
                    })
                };
                handle(mouse(MouseEventKind::Moved), &mut input, &mut game);
                handle(
                    mouse(MouseEventKind::Down(MouseButton::Left)),
                    &mut input,
                    &mut game,
                );
                let shots = game.bullets.len();
                game.update(0.11, &input, &view);
                assert!(input.boost && input.fire);
                assert!((game.player.aim - angle).abs() < 1e-5);
                assert_eq!(game.bullets.len(), shots + 1);
                assert!(
                    game.bullets
                        .last()
                        .unwrap()
                        .vel
                        .norm()
                        .dot(crate::game::V2::from_angle(angle))
                        > 0.99
                );
                assert!(game.player.vel.len() > 0.0);
                handle(
                    mouse(MouseEventKind::Up(MouseButton::Left)),
                    &mut input,
                    &mut game,
                );
                assert!(!input.fire && input.boost);
            }
        }
    }

    #[test]
    fn shift_thrust_tracks_both_keys_modifiers_release_and_focus_loss() {
        let mut game = Game::new(7);
        let mut input = Input::default();
        let key =
            |code, kind, modifiers| Event::Key(KeyEvent::new_with_kind(code, modifiers, kind));
        for side in [ModifierKeyCode::LeftShift, ModifierKeyCode::RightShift] {
            handle(
                key(
                    KeyCode::Modifier(side),
                    KeyEventKind::Press,
                    KeyModifiers::SHIFT,
                ),
                &mut input,
                &mut game,
            );
            input.end_frame();
            assert!(input.boost);
        }
        handle(
            key(KeyCode::Char('W'), KeyEventKind::Press, KeyModifiers::SHIFT),
            &mut input,
            &mut game,
        );
        assert!(input.held.contains(&KeyCode::Char('w')));
        handle(
            key(
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                KeyEventKind::Release,
                KeyModifiers::SHIFT,
            ),
            &mut input,
            &mut game,
        );
        assert!(input.boost, "right shift is still held");
        handle(
            key(
                KeyCode::Modifier(ModifierKeyCode::RightShift),
                KeyEventKind::Release,
                KeyModifiers::NONE,
            ),
            &mut input,
            &mut game,
        );
        assert!(!input.boost);
        assert!(
            input.held.contains(&KeyCode::Char('w')),
            "release shift while moving"
        );
        handle(
            key(
                KeyCode::Char('W'),
                KeyEventKind::Repeat,
                KeyModifiers::SHIFT,
            ),
            &mut input,
            &mut game,
        );
        assert!(input.boost, "also accept modifier flags");
        handle(Event::FocusLost, &mut input, &mut game);
        assert!(!input.boost && input.held.is_empty());
    }

    #[test]
    fn right_mouse_stays_held_until_release_or_focus_loss() {
        let mut game = Game::new(7);
        let mut input = Input::default();
        let mouse = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 100,
                row: 100,
                modifiers: KeyModifiers::NONE,
            })
        };
        handle(
            mouse(MouseEventKind::Down(MouseButton::Right)),
            &mut input,
            &mut game,
        );
        input.end_frame();
        assert!(input.spin);
        assert!(!input.dash && !input.fire && !input.clicked);
        handle(
            mouse(MouseEventKind::Up(MouseButton::Right)),
            &mut input,
            &mut game,
        );
        assert!(!input.spin);
        handle(
            mouse(MouseEventKind::Down(MouseButton::Right)),
            &mut input,
            &mut game,
        );
        handle(Event::FocusLost, &mut input, &mut game);
        assert!(!input.spin);
    }

    #[test]
    fn charge_tracks_middle_mouse_and_e_and_clears_on_focus_loss() {
        let mut game = Game::new(7);
        let mut input = Input::default();
        let mouse = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 100,
                row: 100,
                modifiers: KeyModifiers::NONE,
            })
        };
        handle(
            mouse(MouseEventKind::Down(MouseButton::Middle)),
            &mut input,
            &mut game,
        );
        assert!(input.middle_clicked);
        input.end_frame();
        assert!(input.charge, "middle mouse stays held between frames");
        assert!(!input.middle_clicked);
        handle(
            mouse(MouseEventKind::Down(MouseButton::Middle)),
            &mut input,
            &mut game,
        );
        assert!(
            !input.middle_clicked,
            "a repeated down event is not a fresh click"
        );
        assert!(!input.clicked && !input.fire && !input.dash);
        handle(
            mouse(MouseEventKind::Up(MouseButton::Middle)),
            &mut input,
            &mut game,
        );
        assert!(!input.charge);
        handle(
            Event::Key(KeyEvent::new(KeyCode::Char('E'), KeyModifiers::NONE)),
            &mut input,
            &mut game,
        );
        assert!(input.held.contains(&KeyCode::Char('e')));
        handle(
            Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('e'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
            &mut input,
            &mut game,
        );
        assert!(!input.held.contains(&KeyCode::Char('e')));
        handle(
            mouse(MouseEventKind::Down(MouseButton::Middle)),
            &mut input,
            &mut game,
        );
        assert!(input.middle_clicked);
        handle(
            Event::Key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)),
            &mut input,
            &mut game,
        );
        handle(Event::FocusLost, &mut input, &mut game);
        assert!(!input.charge && !input.middle_clicked && input.held.is_empty());
    }

    #[test]
    fn wheel_steps_accumulate_without_releasing_held_controls_and_clear_each_frame() {
        let mut game = Game::new(7);
        let mut input = Input {
            held: [KeyCode::Char('w')].into_iter().collect(),
            fire: true,
            charge: true,
            spin: true,
            boost: true,
            ..Input::default()
        };
        let mouse = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 1199,
                row: 749,
                modifiers: KeyModifiers::SHIFT,
            })
        };
        for kind in [
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollRight,
        ] {
            handle(mouse(kind), &mut input, &mut game);
        }
        assert_eq!(input.weapon_scroll, 2);
        assert_eq!(input.mouse, Some(v2(1200.0, 750.0)));
        assert!(input.held.contains(&KeyCode::Char('w')));
        assert!(input.fire && input.charge && input.spin && input.boost);
        assert!(!input.clicked && !input.middle_clicked && !input.dash && !input.mouse_moved);
        input.end_frame();
        assert_eq!(input.weapon_scroll, 0);
        assert!(input.fire && input.charge && input.spin && input.boost);
        handle(mouse(MouseEventKind::ScrollUp), &mut input, &mut game);
        handle(Event::FocusLost, &mut input, &mut game);
        assert_eq!(input.weapon_scroll, 0);
    }
}
