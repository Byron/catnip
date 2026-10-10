# Catnip

![Glowstorm, a twin-stick shooter running inside kitty](assets/glowstorm.webp)

A small Rust game engine for the [kitty](https://sw.kovidgoyal.net/kitty/) terminal.
It also runs in [Ghostty](https://ghostty.org), which implements kitty's protocols.

Catnip doesn't draw with text characters. It uploads sprite sheets to kitty once
and then moves them around as image placements every frame, so kitty's GPU does
the compositing. A busy frame is a few kilobytes of escape codes and well under
a millisecond of CPU time.

It targets kitty's protocols on purpose, instead of the lowest common
denominator that works in every terminal.

## Glowstorm

A twin-stick arena shooter, and the first game built on Catnip.

### Install

Glowstorm needs [kitty](https://sw.kovidgoyal.net/kitty/) or
[Ghostty](https://ghostty.org) to play in. Started from any other terminal, it
opens itself in one of them.

- **Download:** grab `glowstorm-v*-x86_64-linux.tar.gz` from
  [Releases](https://github.com/Hemantabhusal/catnip/releases), unpack it and
  run `./glowstorm`.
- **Build from source** (needs [Rust](https://rustup.rs) and the ALSA headers,
  `libasound2-dev` on Debian/Ubuntu):

  ```sh
  git clone https://github.com/Hemantabhusal/catnip
  cd catnip
  cargo run --release
  ```

Sound uses ALSA (`alsa-lib` on Arch, `libasound2` on Debian/Ubuntu); without an
audio device the game runs silently.

| Control | Action |
|---|---|
| WASD | move |
| mouse | aim |
| hold left click | fire |
| Space / right click | dash |
| Esc / P | pause |
| Q | quit |

Enemies come in waves, and every attack starts with a visible wind-up:
chasers lunge, swarms flank, shooters lead their shots, and tanks slam out a
shockwave when you get close. Every fifth wave, PRISM arrives: a three-phase
boss with bullet rings, spirals, charges and summoned minions.

From wave 6, rare elite enemies show up with a gold ring: three times the
health, a nastier attack, and guaranteed loot. After each boss you pick one of
three upgrades: spread shot, piercing bullets, twin dash, coolant, a shield orb,
an extra heart, or longer range.

Pick Medium, Hard or Extreme on the title screen. Harder settings mean fewer
hearts and faster, more aggressive, more numerous enemies, and pay up to 2× the
score.
Enemies grow stronger every wave; you grow a little too, roughly 1.2× by the
time they're twice as tough.

Your gun heats up as you fire and locks for a second if it overheats, and
shots lose up to a quarter of their damage at long range, so it pays to get
close. Quick kills build a multiplier up to ×8 with a rising note for every
kill in the chain; getting hit resets it. Best scores are kept per difficulty in `~/.local/share/glowstorm/`.

## How it works

- All art is generated in code at start-up, at the window's real pixel size:
  one sprite atlas (rotations are pre-rendered, since kitty can't rotate an
  image) and one large arena floor.
- Both are sent to kitty once through shared memory. After that, a frame only
  contains placement commands.
- The camera scrolls by changing which part of the floor image is shown.
- Input uses kitty's keyboard protocol (so held keys and key release work) and
  pixel-precise mouse reporting.
- Titles are neon sprite text drawn from a built-in 5×7 pixel font, frames use
  synchronized output, and sound effects are synthesized at start-up.

The repository is a Cargo workspace with two crates:

```
crates/
├── catnip/              the engine (crate catnip-engine), no game-specific code
│   ├── src/kitty.rs     graphics protocol: uploads, placements
│   ├── src/term.rs      terminal setup, window size, input modes
│   ├── src/gfx.rs       procedural shapes, glow, atlas packing
│   ├── src/font.rs      5×7 pixel font
│   ├── src/synth.rs     sound synthesis and playback
│   └── examples/spike.rs  placement and bandwidth benchmark
└── glowstorm/           the game
    ├── src/main.rs      start-up and terminal detection
    ├── src/app.rs       frame loop and input events
    ├── src/sound.rs     the game's sound effects
    ├── src/art/         sprites, palette, fonts, arena floor
    ├── src/game/        simulation: player, enemies, waves, combat, boss, upgrades
    └── src/render/      drawing: the arena, HUD and menus
```

## Requirements

- a recent kitty, or Ghostty 1.x
- Linux (macOS is untested)
- ALSA for sound; the game runs silently without an audio device

## Resource use

Measured on a laptop (i5-13500H, Intel Iris Xe) with a maximized kitty window:

| | CPU (one core) | Memory |
|---|---|---|
| Glowstorm | about 3% | about 17 MB |
| kitty while playing | about 15% | about 120 MB |

A frame takes about 0.2 ms of CPU and 2–10 KB of escape codes. Integrated
graphics is enough. Start-up takes a second or two while the art is drawn.

## Benchmark

```sh
cargo run --release -p catnip-engine --example spike
```

Moves bouncing sprites around with an on-screen readout of fps and bytes per
frame. `]` and `[` double and halve the sprite count, `b` toggles a full-window
pixel buffer, `u` removes the 60 fps cap. On a recent laptop, kitty keeps about
1,500 moving sprites at 60 fps.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

`GLOWSTORM_WAVE=6` starts at wave 6, `GLOWSTORM_GOD=1` turns off damage, and
`GLOWSTORM_CARDS=1` opens the upgrade screen right away, and `GLOWSTORM_SCALE=0.8`
forces a render scale (0.3–2.2).
Runs with any of them set don't save a best score. Logs go to `$TMPDIR/glowstorm.log`.

## License

MIT, see [LICENSE](LICENSE).
