//! Glowstorm: a twin-stick shooter running inside kitty.

mod app;
mod art;
mod game;
mod render;
mod sound;

use std::{ffi::OsString, time::Instant};

use catnip::{kitty, term};

use sound::Audio;

fn main() {
    let options = match parse_args(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(e) => {
            eprintln!("{e}\nTry 'glowstorm --help' for usage.");
            std::process::exit(2);
        }
    };
    if options.help {
        println!("{HELP}");
        return;
    }
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
    let arena = game::View::new(win, scale).arena_size();
    let floor = art::build_floor(scale, arena.x, arena.y);
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
    let audio = if options.no_sound {
        None
    } else {
        Audio::open()
    };
    eprintln!(
        "audio: {}",
        if options.no_sound {
            "disabled (--no-sound)"
        } else if audio.is_some() {
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

const HELP: &str = "Glowstorm — a terminal twin-stick shooter for kitty and Ghostty

Usage: glowstorm [OPTIONS]

Options:
  -h, --help  Show this help and exit
  --no-sound  Skip sound synthesis, audio initialization, and playback";

#[derive(Default, Debug, PartialEq, Eq)]
struct Options {
    help: bool,
    no_sound: bool,
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Options, String> {
    let mut options = Options::default();
    for arg in args {
        match arg.to_str() {
            Some("-h" | "--help") => options.help = true,
            Some("--no-sound") => options.no_sound = true,
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy())),
        }
    }
    Ok(options)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_help_and_no_sound_with_sound_enabled_by_default() {
        assert_eq!(parse_args([]).unwrap(), Options::default());
        for help in ["-h", "--help"] {
            assert_eq!(
                parse_args([help.into()]).unwrap(),
                Options {
                    help: true,
                    no_sound: false,
                }
            );
            for args in [[help, "--no-sound"], ["--no-sound", help]] {
                assert_eq!(
                    parse_args(args.map(OsString::from)).unwrap(),
                    Options {
                        help: true,
                        no_sound: true,
                    }
                );
            }
        }
        assert_eq!(
            parse_args(["--no-sound".into()]).unwrap(),
            Options {
                help: false,
                no_sound: true,
            }
        );
    }

    #[test]
    fn rejects_unknown_arguments() {
        for arg in ["--no-soud", "unexpected"] {
            assert_eq!(
                parse_args([arg.into()]).unwrap_err(),
                format!("unknown argument: {arg}")
            );
        }
    }
}
