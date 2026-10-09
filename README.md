# Catnip

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

```sh
cargo run --release
```

If you start it from another terminal, it opens itself in kitty (or Ghostty).

| Control | Action |
|---|---|
| WASD | move |
| mouse | aim |
| hold left click | fire |
| Space / right click | dash |
| arrow keys | aim and fire without a mouse |
| Esc / P | pause |
| Q | quit |

Enemies come in waves, and every attack starts with a visible wind-up:
chasers lunge, swarms flank, shooters lead their shots, and tanks slam out a
shockwave when you get close. Every fifth wave, PRISM arrives: a three-phase
boss with bullet rings, spirals, charges and summoned minions.

Pick Medium, Hard or Extreme on the title screen. Harder settings mean fewer
hearts and tougher, faster, more numerous enemies, and pay up to 2× the score.
Enemies grow stronger every wave; you grow a little too, roughly 1.2× by the
time they're twice as tough.

Your gun heats up as you fire and locks for a second if it overheats, and
shots lose damage over distance, so it pays to get close. Quick kills build a
multiplier up to ×8 with a rising note for every kill in the chain; getting hit
resets it. Your best score is kept in `~/.local/share/glowstorm/best_score`.

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

| Path | What's there |
|---|---|
| `src/kitty.rs`, `src/term.rs` | graphics protocol, terminal setup |
| `src/gfx.rs`, `src/font.rs` | shape drawing, glow, atlas packing, pixel font |
| `src/audio.rs` | sound synthesis and playback |
| `src/game.rs`, `src/art.rs`, `src/render.rs` | Glowstorm |
| `src/bin/spike.rs` | placement and bandwidth benchmark |

## Requirements

- a recent kitty, or Ghostty 1.x
- Linux (macOS is untested)
- ALSA for sound; the game runs silently without an audio device

## Benchmark

```sh
cargo run --release --bin spike
```

Moves bouncing sprites around with an on-screen readout of fps and bytes per
frame. `]` and `[` double and halve the sprite count, `b` toggles a full-window
pixel buffer, `u` removes the 60 fps cap. On a recent laptop, kitty keeps about
1,500 moving sprites at 60 fps.

## Development

```sh
cargo test
cargo clippy --all-targets
```

`GLOWSTORM_WAVE=6` starts at wave 6 and `GLOWSTORM_GOD=1` turns off damage.
Runs with either set don't save a best score. Logs go to `$TMPDIR/glowstorm.log`.
