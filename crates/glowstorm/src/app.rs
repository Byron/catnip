//! The frame loop: read input, step the game, play sounds, draw, measure.

use std::{
    collections::HashSet,
    io::{self, Write},
    time::{Duration, Instant},
};

use catnip::{gfx::Image, kitty, term};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

use crate::{
    art::Sprites,
    game::{Game, Input, V2, View, v2},
    render::{ATLAS_ID, FLASH_ID, FLOOR_ID, Renderer, SHADE_ID, Stats, VOID_ID},
    sound::Audio,
};

const FRAME: Duration = Duration::from_micros(16_667);

/// Why the frame loop ended.
pub enum Exit {
    Quit,
    /// The terminal doesn't report key releases (kitty keyboard protocol off).
    NeedsKeyboardProtocol,
}

pub fn run(
    mut win: term::Window,
    scale: f32,
    atlas: Vec<Image>,
    floor: Image,
    sprites: &Sprites,
    audio: Option<&Audio>,
) -> io::Result<Exit> {
    let mut out = io::stdout().lock();
    upload_images(&mut out, &atlas, &floor)?;
    // Fail with a clear message instead of running with invisible sprites.
    kitty::verify(&mut out, Duration::from_secs(5)).map_err(io::Error::other)?;
    // Smooth movement needs key releases.
    if !term::keyboard_protocol_active(Duration::from_millis(800)) {
        return Ok(Exit::NeedsKeyboardProtocol);
    }
    let mut renderer = Renderer::new((floor.w, floor.h), atlas.len());
    drop((atlas, floor));

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let mut game = Game::new(seed);
    let mut input = Input::default();
    let mut meter = Meter::new();
    let mut frame = Vec::with_capacity(1 << 18);
    let mut last = Instant::now();
    let mut deadline = Instant::now();

    loop {
        // Input until the frame deadline.
        while event::poll(deadline.saturating_duration_since(Instant::now()))? {
            match event::read()? {
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
        deadline = (deadline + FRAME).max(now);
        let dt = (now - last).as_secs_f32().min(1.0 / 30.0);
        last = now;
        let started = Instant::now();

        let view = View {
            scale,
            size: V2 {
                x: win.width as f32 / scale,
                y: win.height as f32 / scale,
            },
        };
        game.update(dt, &input, &view);
        input.end_frame();
        play_sounds(&mut game, audio);

        frame.clear();
        renderer.frame(&mut frame, &game, sprites, scale, win, meter.stats());
        out.write_all(&frame)?;
        out.flush()?;
        meter.record(started.elapsed(), frame.len());
    }
}

/// The atlas pages, the floor, and two 1×1 overlay pixels go to kitty once.
fn upload_images(out: &mut impl Write, atlas: &[Image], floor: &Image) -> io::Result<()> {
    let mut bytes = Vec::new();
    for (i, page) in atlas.iter().enumerate() {
        kitty::upload(&mut bytes, ATLAS_ID + i as u32, page.w, page.h, &page.rgba)?;
    }
    kitty::upload(&mut bytes, FLOOR_ID, floor.w, floor.h, &floor.rgba)?;
    kitty::upload(&mut bytes, SHADE_ID, 1, 1, &[4, 5, 12, 170])?;
    kitty::upload(&mut bytes, FLASH_ID, 1, 1, &[255, 255, 255, 60])?;
    kitty::upload(&mut bytes, VOID_ID, 1, 1, &[3, 4, 10, 255])?;
    out.write_all(&bytes)?;
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
                MouseEventKind::Down(MouseButton::Right) => input.dash = true,
                _ => {}
            }
        }
        Event::FocusLost => {
            input.held.clear();
            input.fire = false;
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
