//! Glowstorm: a twin-stick shooter running inside kitty.

mod art;
mod game;
mod render;

use std::{
    collections::HashSet,
    io::{self, Write},
    time::{Duration, Instant},
};

use catnip::{audio::Audio, gfx::Image, kitty, term};
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

use game::{Game, Input, V2, View, v2};
use render::{ATLAS_ID, FLOOR_ID, Renderer, SHADE_ID, Stats};

const FRAME: Duration = Duration::from_micros(16_667);

fn main() {
    if !term::supported_terminal() {
        // Started from another terminal: open ourselves in kitty (or Ghostty).
        if std::env::var_os("GLOWSTORM_RELAUNCHED").is_none() && relaunch_in_supported_terminal() {
            println!("Glowstorm opened in a new window.");
            return;
        }
        eprintln!("Glowstorm needs kitty (https://sw.kovidgoyal.net/kitty/) or Ghostty.");
        std::process::exit(1);
    }
    let win = match term::window_size() {
        Ok(win) => win,
        Err(e) => {
            eprintln!("cannot read window size: {e}");
            std::process::exit(1);
        }
    };

    println!("building sprites…");
    let started = Instant::now();
    let scale = art::scale_for(win);
    let (atlas, sprites) = art::build_sprites(scale);
    let floor = art::build_floor(scale);
    let build_time = started.elapsed();

    let log = std::env::temp_dir().join("glowstorm.log");
    let _ = term::redirect_stderr(&log);
    eprintln!(
        "window {}x{} px, {}x{} cells of {:?} px, scale {scale:.2}, atlas {}x{}, floor {}x{}, built in {build_time:?}",
        win.width,
        win.height,
        win.cols,
        win.rows,
        win.cell(),
        atlas.w,
        atlas.h,
        floor.w,
        floor.h
    );
    let audio = Audio::open();
    eprintln!(
        "audio: {}",
        if audio.is_some() {
            "on"
        } else {
            "unavailable, running silent"
        }
    );

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::restore();
        kitty::cleanup_shm();
        default_hook(info);
    }));

    let result =
        term::enter().and_then(|()| run(win, scale, atlas, floor, &sprites, audio.as_ref()));
    term::restore();
    kitty::cleanup_shm();
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn relaunch_in_supported_terminal() -> bool {
    use std::process::{Command, Stdio};
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let quiet = |mut cmd: Command| {
        cmd.args(&args)
            .env("GLOWSTORM_RELAUNCHED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    };

    let mut kitty = Command::new("kitty");
    kitty
        .args(["--detach", "--start-as=maximized", "--title", "Glowstorm"])
        // Hide the pointer as soon as it rests, and don't bring it back on
        // movement: the game draws its own reticle.
        .args(["-o", "mouse_hide_wait=0.01 1000 100000 no"])
        .arg(&exe);
    if quiet(kitty).status().is_ok_and(|s| s.success()) {
        return true;
    }
    // Ghostty has no --detach; spawning without waiting leaves it running.
    let mut ghostty = Command::new("ghostty");
    ghostty
        .args([
            "--title=Glowstorm",
            "--maximize=true",
            "--mouse-hide-while-typing=true",
            "-e",
        ])
        .arg(&exe);
    quiet(ghostty).spawn().is_ok()
}

fn run(
    mut win: term::Window,
    scale: f32,
    atlas: Image,
    floor: Image,
    sprites: &art::Sprites,
    audio: Option<&Audio>,
) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let mut frame = Vec::with_capacity(1 << 18);
    kitty::upload(&mut frame, ATLAS_ID, atlas.w, atlas.h, &atlas.rgba)?;
    kitty::upload(&mut frame, FLOOR_ID, floor.w, floor.h, &floor.rgba)?;
    kitty::upload(&mut frame, SHADE_ID, 1, 1, &[4, 5, 12, 170])?;
    out.write_all(&frame)?;
    out.flush()?;
    let mut renderer = Renderer::new((floor.w, floor.h));
    drop((atlas, floor));

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let mut game = Game::new(seed);
    let mut input = Input::default();
    let mut stats = Stats {
        fps: 0.0,
        frame_ms: 0.0,
        kb_per_frame: 0.0,
    };
    let mut window_start = Instant::now();
    let (mut frames, mut work, mut bytes) = (0u32, Duration::ZERO, 0usize);
    let mut last = Instant::now();
    let mut deadline = Instant::now();

    loop {
        // Input until the frame deadline.
        while event::poll(deadline.saturating_duration_since(Instant::now()))? {
            match event::read()? {
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
                            let ctrl_c = code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL);
                            if code == KeyCode::Char('q') || ctrl_c {
                                return Ok(());
                            }
                            input.held.insert(code);
                            input.pressed.push(code);
                        }
                    }
                }
                Event::Mouse(mouse) => {
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
                Event::Resize(..) => win = term::window_size()?,
                _ => {}
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

        if let Some(audio) = audio {
            let mut played = HashSet::new();
            for sfx in game.sfx.drain(..) {
                if played.insert(sfx) {
                    audio.play(sfx);
                }
            }
        } else {
            game.sfx.clear();
        }

        frame.clear();
        renderer.frame(&mut frame, &game, sprites, scale, win, &stats);
        out.write_all(&frame)?;
        out.flush()?;

        frames += 1;
        work += started.elapsed();
        bytes += frame.len();
        let elapsed = window_start.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            stats = Stats {
                fps: frames as f32 / elapsed,
                frame_ms: work.as_secs_f32() * 1000.0 / frames as f32,
                kb_per_frame: bytes as f32 / frames as f32 / 1024.0,
            };
            window_start = Instant::now();
            (frames, work, bytes) = (0, Duration::ZERO, 0);
        }
    }
}
