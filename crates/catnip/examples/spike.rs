//! Benchmark: how far can kitty's GPU-composited placements go?
//!
//! One sprite atlas is uploaded once. Every frame only sends placement
//! commands ("show slice k at pixel x,y"), plus an optional full-window
//! pixel buffer through the platform's default upload transport to compare against.

use std::{
    collections::HashSet,
    io::{self, Write},
    time::{Duration, Instant},
};

use catnip::{
    kitty::{self, Rect},
    term::{self, Window},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

const SPRITE: u32 = 24;
const ATLAS_SPRITES: u32 = 4;
const PLAYER_KIND: u32 = 3;
const ATLAS_ID: u32 = 1;
const BG_IDS: [u32; 2] = [2, 3];
const PLAYER_PLACEMENT: u32 = 1;
const BALL_PLACEMENT_BASE: u32 = 2;
const MAX_BALLS: usize = 20_000;
// Negative z keeps every image under the HUD text.
const Z_BACKGROUND: i32 = -1000;
const Z_BALLS: i32 = -100;
const Z_PLAYER: i32 = -50;
const FRAME: Duration = Duration::from_micros(16_667);
const ACCEL: f32 = 4000.0;
const DRAG: f32 = 6.0;

fn main() {
    if !term::supported_terminal() {
        eprintln!("the spike runs inside kitty, Ghostty, or Rustty");
        std::process::exit(1);
    }
    let win = match term::window_size() {
        Ok(win) => win,
        Err(e) => {
            eprintln!("cannot read window size: {e}");
            std::process::exit(1);
        }
    };

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::restore();
        kitty::cleanup_shm();
        default_hook(info);
    }));

    let result = term::enter().and_then(|()| run(win));
    term::restore();
    kitty::cleanup_shm();
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Background {
    Off,
    Half,
    Full,
}

impl Background {
    fn next(self) -> Self {
        match self {
            Self::Off => Self::Half,
            Self::Half => Self::Full,
            Self::Full => Self::Off,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Half => "half-res",
            Self::Full => "full-res",
        }
    }
}

struct Body {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    kind: u32,
}

fn run(mut win: Window) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let mut frame = Vec::with_capacity(1 << 20);
    kitty::upload(
        &mut frame,
        ATLAS_ID,
        SPRITE * ATLAS_SPRITES,
        SPRITE,
        &atlas(),
    )?;
    out.write_all(&frame)?;
    out.flush()?;

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut player = Body {
        x: win.width as f32 / 2.0,
        y: win.height as f32 / 2.0,
        vx: 0.0,
        vy: 0.0,
        kind: PLAYER_KIND,
    };
    let mut balls: Vec<Body> = Vec::new();
    let mut target_balls = 64;
    let mut held: HashSet<KeyCode> = HashSet::new();
    let mut background = Background::Off;
    let mut bg_pixels = Vec::new();
    let mut bg_frame = 0usize;
    let mut capped = true;
    let mut stats = Stats::new();
    let start = Instant::now();
    let mut last = Instant::now();
    let mut deadline = Instant::now();

    loop {
        frame.clear();

        // Handle input until the frame deadline (or just drain it when uncapped).
        loop {
            let timeout = if capped {
                deadline.saturating_duration_since(Instant::now())
            } else {
                Duration::ZERO
            };
            let Some(event) = term::read_event(timeout)? else {
                break;
            };
            match event {
                Event::Key(key) => {
                    let code = normalize(key.code);
                    if key.kind == KeyEventKind::Release {
                        held.remove(&code);
                        continue;
                    }
                    held.insert(code);
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match code {
                        _ if is_quit(&key) => return Ok(()),
                        KeyCode::Char(']') => target_balls = (target_balls * 2).clamp(1, MAX_BALLS),
                        KeyCode::Char('[') => target_balls /= 2,
                        KeyCode::Char('u') => capped = !capped,
                        KeyCode::Char('b') => {
                            background = background.next();
                            if background == Background::Off {
                                BG_IDS
                                    .iter()
                                    .for_each(|&id| kitty::delete_image(&mut frame, id));
                            }
                        }
                        _ => {}
                    }
                }
                Event::Resize(..) => win = term::window_size()?,
                _ => {}
            }
        }
        let now = Instant::now();
        deadline = (deadline + FRAME).max(now);
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;
        let work_started = Instant::now();

        // Simulate.
        steer(&mut player, &held, dt);
        step(&mut player, dt, &win);
        while balls.len() < target_balls {
            balls.push(random_ball(&mut rng, &win));
        }
        for index in target_balls..balls.len() {
            kitty::delete_placement(&mut frame, ATLAS_ID, BALL_PLACEMENT_BASE + index as u32);
        }
        balls.truncate(target_balls);
        for ball in &mut balls {
            step(ball, dt, &win);
        }

        // Draw. Synchronized output makes Kitty show the whole frame at once.
        frame.extend_from_slice(b"\x1b[?2026h");
        let mut shm_bytes = 0;
        if background != Background::Off {
            let scale = if background == Background::Half { 2 } else { 1 };
            let size = (win.width / scale, win.height / scale);
            draw_background(&mut bg_pixels, size, start.elapsed().as_secs_f32());
            let id = BG_IDS[bg_frame % 2];
            kitty::show_fullscreen(
                &mut frame,
                id,
                size,
                &bg_pixels,
                (win.cols, win.rows),
                Z_BACKGROUND,
            )?;
            kitty::delete_image(&mut frame, BG_IDS[(bg_frame + 1) % 2]);
            bg_frame += 1;
            if kitty::Transport::default() == kitty::Transport::SharedMemory {
                shm_bytes = bg_pixels.len();
            }
        }
        let cell = win.cell();
        for (index, ball) in balls.iter().enumerate() {
            let placement = (ATLAS_ID, BALL_PLACEMENT_BASE + index as u32);
            kitty::place(
                &mut frame,
                placement,
                sprite(ball.kind),
                pos(ball),
                cell,
                Z_BALLS,
            );
        }
        let placement = (ATLAS_ID, PLAYER_PLACEMENT);
        kitty::place(
            &mut frame,
            placement,
            sprite(player.kind),
            pos(&player),
            cell,
            Z_PLAYER,
        );
        let pty_bytes = frame.len();
        draw_hud(&mut frame, &stats, balls.len(), background, capped);
        frame.extend_from_slice(b"\x1b[?2026l");

        // write_all blocks while Kitty is still consuming the previous frame,
        // so the measured time includes Kitty's side of the work.
        out.write_all(&frame)?;
        out.flush()?;
        stats.record(pty_bytes, shm_bytes, work_started.elapsed());
    }
}

fn normalize(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
        other => other,
    }
}

fn is_quit(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn steer(player: &mut Body, held: &HashSet<KeyCode>, dt: f32) {
    let pressed = |keys: [KeyCode; 2]| keys.iter().any(|k| held.contains(k));
    let axis = |neg: bool, pos: bool| f32::from(u8::from(pos)) - f32::from(u8::from(neg));
    let ax = axis(
        pressed([KeyCode::Left, KeyCode::Char('a')]),
        pressed([KeyCode::Right, KeyCode::Char('d')]),
    );
    let ay = axis(
        pressed([KeyCode::Up, KeyCode::Char('w')]),
        pressed([KeyCode::Down, KeyCode::Char('s')]),
    );
    let damping = (-DRAG * dt).exp();
    player.vx = (player.vx + ax * ACCEL * dt) * damping;
    player.vy = (player.vy + ay * ACCEL * dt) * damping;
}

/// Moves a body and bounces it off the window edges.
fn step(body: &mut Body, dt: f32, win: &Window) {
    let max_x = win.width.saturating_sub(SPRITE) as f32;
    let max_y = win.height.saturating_sub(SPRITE) as f32;
    body.x += body.vx * dt;
    body.y += body.vy * dt;
    if body.x < 0.0 || body.x > max_x {
        body.x = body.x.clamp(0.0, max_x);
        body.vx = -body.vx;
    }
    if body.y < 0.0 || body.y > max_y {
        body.y = body.y.clamp(0.0, max_y);
        body.vy = -body.vy;
    }
}

fn random_ball(rng: &mut Rng, win: &Window) -> Body {
    let angle = rng.unit() * std::f32::consts::TAU;
    let speed = 120.0 + rng.unit() * 360.0;
    Body {
        x: rng.unit() * win.width.saturating_sub(SPRITE) as f32,
        y: rng.unit() * win.height.saturating_sub(SPRITE) as f32,
        vx: angle.cos() * speed,
        vy: angle.sin() * speed,
        kind: (rng.next() % u64::from(PLAYER_KIND)) as u32,
    }
}

fn pos(body: &Body) -> (u32, u32) {
    (body.x as u32, body.y as u32)
}

fn sprite(kind: u32) -> Rect {
    Rect {
        x: kind * SPRITE,
        y: 0,
        w: SPRITE,
        h: SPRITE,
    }
}

/// Four shaded discs side by side; the last one is the player.
fn atlas() -> Vec<u8> {
    const COLORS: [[f32; 3]; ATLAS_SPRITES as usize] = [
        [255.0, 107.0, 107.0],
        [78.0, 205.0, 196.0],
        [255.0, 209.0, 102.0],
        [245.0, 245.0, 255.0],
    ];
    let width = SPRITE * ATLAS_SPRITES;
    let mut rgba = vec![0u8; (width * SPRITE * 4) as usize];
    let radius = SPRITE as f32 / 2.0 - 1.0;
    for (kind, color) in COLORS.iter().enumerate() {
        for y in 0..SPRITE {
            for x in 0..SPRITE {
                let dx = x as f32 + 0.5 - SPRITE as f32 / 2.0;
                let dy = y as f32 + 0.5 - SPRITE as f32 / 2.0;
                let alpha = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                // Lit from the top-left.
                let shade = 1.0 - 0.3 * ((dx + dy) / (2.0 * radius)).clamp(-1.0, 1.0);
                let i = ((y * width + kind as u32 * SPRITE + x) * 4) as usize;
                for c in 0..3 {
                    rgba[i + c] = (color[c] * shade).min(255.0) as u8;
                }
                rgba[i + 3] = (alpha * 255.0) as u8;
            }
        }
    }
    rgba
}

/// A cheap moving pattern; the point is to push a full buffer every frame.
fn draw_background(rgba: &mut Vec<u8>, (w, h): (u32, u32), t: f32) {
    rgba.resize((w * h * 4) as usize, 0);
    let shift = (t * 90.0) as u32;
    for (i, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = i as u32 % w;
        let y = i as u32 / w;
        let v = ((x + shift) ^ (y + shift / 2)) as u8;
        px.copy_from_slice(&[v / 5, v / 4, v / 3 + 30, 255]);
    }
}

fn draw_hud(
    out: &mut Vec<u8>,
    stats: &Stats,
    sprites: usize,
    background: Background,
    capped: bool,
) {
    let _ = write!(
        out,
        "\x1b[1;1H\x1b[0;1;97m {:>5.1} fps │ {} sprites │ pty {:.1} KB/frame │ shm {:.1} MB/s │ frame {:.2} ms │ bg {} │ {}\x1b[0m\x1b[K\
         \x1b[2;1H\x1b[0;37m hold arrows/WASD: move │ [ ]: sprites ÷2 ×2 │ b: background │ u: uncap │ q: quit\x1b[0m\x1b[K",
        stats.fps,
        sprites + 1,
        stats.pty_kb_per_frame,
        stats.shm_mb_per_sec,
        stats.frame_ms,
        background.label(),
        if capped { "60 cap" } else { "uncapped" },
    );
}

/// Rolling numbers for the HUD, refreshed twice a second.
struct Stats {
    window_start: Instant,
    frames: u32,
    pty_bytes: usize,
    shm_bytes: usize,
    work: Duration,
    fps: f32,
    pty_kb_per_frame: f32,
    shm_mb_per_sec: f32,
    frame_ms: f32,
}

impl Stats {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            frames: 0,
            pty_bytes: 0,
            shm_bytes: 0,
            work: Duration::ZERO,
            fps: 0.0,
            pty_kb_per_frame: 0.0,
            shm_mb_per_sec: 0.0,
            frame_ms: 0.0,
        }
    }

    fn record(&mut self, pty_bytes: usize, shm_bytes: usize, work: Duration) {
        self.frames += 1;
        self.pty_bytes += pty_bytes;
        self.shm_bytes += shm_bytes;
        self.work += work;
        let elapsed = self.window_start.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            let frames = self.frames as f32;
            self.fps = frames / elapsed;
            self.pty_kb_per_frame = self.pty_bytes as f32 / frames / 1024.0;
            self.shm_mb_per_sec = self.shm_bytes as f32 / elapsed / 1_048_576.0;
            self.frame_ms = self.work.as_secs_f32() * 1000.0 / frames;
            *self = Self {
                window_start: Instant::now(),
                frames: 0,
                pty_bytes: 0,
                shm_bytes: 0,
                work: Duration::ZERO,
                ..*self
            };
        }
    }
}

/// xorshift64: plenty for scattering test sprites, no dependency needed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}
