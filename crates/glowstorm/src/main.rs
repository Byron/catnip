//! Glowstorm: a twin-stick shooter running inside kitty.

mod app;
mod art;
mod game;
mod render;
mod sound;

use std::time::Instant;

use catnip::{kitty, term};
use clap::Parser;

use sound::Audio;

fn main() {
    let options = Options::parse();
    if term::Terminal::detect().is_none() {
        // Started from another terminal: open ourselves in one that works.
        if !options.relaunched && relaunch_in_supported_terminal() {
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
    let scale = art::scale_for(win, options.scale);
    let (scale, atlas, sprites) = match art::build_fitting_sprites(win, scale) {
        Ok(images) => images,
        Err(e) => {
            eprintln!("cannot build sprites: {e}");
            std::process::exit(1);
        }
    };
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

    let result = term::enter()
        .and_then(|()| app::run(win, scale, atlas, floor, sprites, audio.as_ref(), &options));
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

#[derive(Debug, Parser)]
#[command(version, about = "A terminal twin-stick shooter for kitty and Ghostty")]
struct Options {
    /// Skip sound synthesis, audio initialization, and playback
    #[arg(long, env = "GLOWSTORM_NO_SOUND", value_parser = clap::builder::BoolishValueParser::new())]
    no_sound: bool,
    /// Force the render scale (0.3–1.8); otherwise follow the terminal font size
    #[arg(long, env = "GLOWSTORM_SCALE", value_parser = parse_scale)]
    scale: Option<f32>,
    /// Target frames per second
    #[arg(long, env = "GLOWSTORM_FPS", default_value_t = 60, value_parser = clap::value_parser!(u16).range(1..))]
    fps: u16,
    #[command(flatten)]
    dev: game::DevFlags,
    #[arg(long, env = "GLOWSTORM_RELAUNCHED", hide = true, value_parser = clap::builder::BoolishValueParser::new())]
    relaunched: bool,
}

fn parse_scale(value: &str) -> Result<f32, String> {
    let scale = value.parse::<f32>().map_err(|e| e.to_string())?;
    if !(0.3..=art::MAX_SCALE).contains(&scale) {
        return Err(format!("scale must be between 0.3 and {}", art::MAX_SCALE));
    }
    Ok(scale)
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
        .args([
            "--detach",
            "--start-as=maximized",
            "--title",
            "Glowstorm",
            "-o",
            "clear_all_mouse_actions=yes",
        ])
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
    fn parses_defaults_flags_and_help() {
        let defaults = Options::try_parse_from(["glowstorm"]).unwrap();
        assert!(!defaults.no_sound);
        assert_eq!(defaults.fps, 60);
        assert_eq!(defaults.scale, None);
        assert_eq!(defaults.dev.first_wave, 1);
        assert!(!defaults.dev.god && !defaults.dev.cards && !defaults.dev.perfect_cooling);
        assert!(!defaults.dev.spread_shot && !defaults.dev.rapid_fire);
        for help in ["-h", "--help"] {
            assert_eq!(
                Options::try_parse_from(["glowstorm", help])
                    .unwrap_err()
                    .kind(),
                clap::error::ErrorKind::DisplayHelp
            );
        }
        let options = Options::try_parse_from([
            "glowstorm",
            "--no-sound",
            "--wave",
            "6",
            "--god",
            "--cards",
            "--perfect-cooling",
            "--spread-shot",
            "--lightning",
            "--scale",
            "0.8",
            "--fps",
            "144",
            "--relaunched",
        ])
        .unwrap();
        assert!(options.no_sound && options.relaunched);
        assert_eq!(options.fps, 144);
        assert_eq!(options.scale, Some(0.8));
        assert_eq!(options.dev.first_wave, 6);
        assert!(options.dev.god && options.dev.cards && options.dev.perfect_cooling);
        assert!(options.dev.spread_shot && options.dev.rapid_fire);
    }

    #[test]
    fn rejects_unknown_arguments_and_invalid_numbers() {
        for args in [
            vec!["--no-soud"],
            vec!["unexpected"],
            vec!["--fps", "0"],
            vec!["--fps", "-1"],
            vec!["--fps", "NaN"],
            vec!["--wave", "0"],
            vec!["--scale", "NaN"],
            vec!["--scale", "inf"],
            vec!["--scale", "0.2"],
            vec!["--scale", "1.9"],
        ] {
            assert!(Options::try_parse_from(std::iter::once("glowstorm").chain(args)).is_err());
        }
    }

    #[test]
    fn parses_environment_with_command_line_precedence() {
        if std::env::var_os("GLOWSTORM_TEST_ENV").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tests::parses_environment_with_command_line_precedence",
                ])
                .env_clear()
                .envs([
                    ("GLOWSTORM_TEST_ENV", "1"),
                    ("GLOWSTORM_NO_SOUND", "1"),
                    ("GLOWSTORM_SCALE", "0.8"),
                    ("GLOWSTORM_FPS", "30"),
                    ("GLOWSTORM_WAVE", "6"),
                    ("GLOWSTORM_GOD", "yes"),
                    ("GLOWSTORM_CARDS", "0"),
                    ("GLOWSTORM_PERFECT_COOLING", "true"),
                    ("GLOWSTORM_SPREAD_SHOT", "on"),
                    ("GLOWSTORM_RAPID_FIRE", "false"),
                    ("GLOWSTORM_RELAUNCHED", "1"),
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let options = Options::try_parse_from(["glowstorm"]).unwrap();
        assert!(options.no_sound && options.relaunched);
        assert_eq!(options.scale, Some(0.8));
        assert_eq!(options.fps, 30);
        assert_eq!(options.dev.first_wave, 6);
        assert!(options.dev.god && options.dev.perfect_cooling && options.dev.spread_shot);
        assert!(!options.dev.cards && !options.dev.rapid_fire);
        let options = Options::try_parse_from([
            "glowstorm",
            "--scale",
            "1.2",
            "--fps",
            "144",
            "--wave",
            "2",
            "--cards",
            "--rapid-fire",
        ])
        .unwrap();
        assert_eq!(options.scale, Some(1.2));
        assert_eq!(options.fps, 144);
        assert_eq!(options.dev.first_wave, 2);
        assert!(options.dev.cards && options.dev.rapid_fire);
    }
}
