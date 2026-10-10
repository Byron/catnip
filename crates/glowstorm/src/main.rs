//! Glowstorm: a twin-stick shooter running inside kitty.

mod app;
mod art;
mod game;
mod render;
mod sound;

use std::time::Instant;

use catnip::{kitty, term};

use sound::Audio;

fn main() {
    if term::Terminal::detect().is_none() {
        // Started from another terminal: open ourselves in one that works.
        if std::env::var_os("GLOWSTORM_RELAUNCHED").is_none() && relaunch_in_supported_terminal() {
            println!("Glowstorm opened in a new window.");
            return;
        }
        eprintln!("{UNSUPPORTED}");
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
    // Before stderr goes to the log: a panic must restore the screen and
    // print where the user can see it.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        term::restore();
        kitty::cleanup_shm();
        default_hook(info);
    }));
    let log = std::env::temp_dir().join("glowstorm.log");
    let _ = term::redirect_stderr(&log);
    eprintln!(
        "window {}x{} px, {}x{} cells of {:?} px, scale {scale:.2}, atlas {} pages {}, floor {}x{}, built in {build_time:?}",
        win.width,
        win.height,
        win.cols,
        win.rows,
        win.cell(),
        atlas.len(),
        atlas
            .iter()
            .map(|p| format!("{}x{}", p.w, p.h))
            .collect::<Vec<_>>()
            .join("+"),
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

    let result =
        term::enter().and_then(|()| app::run(win, scale, atlas, floor, &sprites, audio.as_ref()));
    term::restore();
    kitty::cleanup_shm();
    match result {
        Ok(app::Exit::Quit) => {}
        Ok(app::Exit::NeedsKeyboardProtocol) => {
            eprintln!("{NO_KEYBOARD}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

const UNSUPPORTED: &str = "Glowstorm runs in kitty or Ghostty (it draws with the kitty graphics and
keyboard protocols). Both are free:
  kitty    https://sw.kovidgoyal.net/kitty/
  Ghostty  https://ghostty.org";

const NO_KEYBOARD: &str = "This terminal didn't turn on key-release reporting (the kitty keyboard
protocol), which Glowstorm needs for smooth movement. Run it in kitty or
Ghostty.";

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
        .arg(&exe);
    if quiet(kitty).status().is_ok_and(|s| s.success()) {
        return true;
    }
    // Ghostty has no --detach; spawning without waiting leaves it running.
    let mut ghostty = Command::new("ghostty");
    ghostty
        .args(["--title=Glowstorm", "--maximize=true", "-e"])
        .arg(&exe);
    quiet(ghostty).spawn().is_ok()
}
