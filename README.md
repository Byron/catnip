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
- **Build from source** (needs [Rust](https://rustup.rs); on Linux, also the ALSA
  headers, `libasound2-dev` on Debian/Ubuntu):

  ```sh
  git clone https://github.com/Hemantabhusal/catnip
  cd catnip
  cargo run --release
  ```

Sound uses CoreAudio on macOS and ALSA on Linux (`alsa-lib` on Arch,
`libasound2` on Debian/Ubuntu); without an audio device the game runs silently.

| Control | Action |
|---|---|
| WASD | move |
| mouse | aim |
| hold left click | fire |
| hold middle click / E, then release | charge and release a gatling barrage |
| tap right click / Space / R | barrel roll |
| hold right click / Space / R, then release | charge a star-laser spin |
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

Hold middle mouse or E to charge the gatling gun (about 0.6 seconds from cold),
then release a rapid spread barrage. Keep aiming as it fires. Longer charges
increase pellet damage, reach, and burst duration (up to 1.6 seconds); the barrage
starts strong and weakens as it runs out. Each volley sprays six pellets;
Spread upgrades add four more per level, and
Piercing and Long Range apply too.
Release in the final 10% of the heat meter, when **RELEASE!** appears, for 10%
extra damage, faster pellets, and a faster firing rate. Holding too long forces
it to fire with 25% less damage and overheats the gun; release before charging
again. Both weapons share heat. Regular fire and charging wait until the barrage
finishes; Coolant extends the charging time.
Every bullet pushes its target along its flight direction, including bosses.
Repeated impacts build momentum, while firing gives your ship a little recoil.
Direct hits from either weapon destroy enemy bullets, including tank volleys
and boss shots, consuming your bullet too.

Right click, Space or R starts an immediate forward barrel roll, spending a dash
charge. Release within 0.18 seconds for evasion; keep holding to charge the
offensive version, then release to fire a rotating eight-spoke star laser.
Holding can charge it even while evasion is recharging.
It shares the gatling's heat rules: more charge means more damage and
duration (up to 1.6 seconds); release in the final 10% of heat for 10% extra
damage and a faster spin. Overheating forces a discharge with 25% less power
and requires release to rearm. Coolant extends charging time. The lasers extend
across the entire screen with no range limit, becoming thinner as their power
fades. They rotate at half a turn per second (0.8 with the timing bonus) and
destroy enemy bullets they touch;
the close shield reflects bullets back as friendly shots. Ship contact deals
heavy damage and pushes enemies away, with no damage or recoil to your ship.
Landing inside enemy ships destroys all overlapping ships (including the boss)
and costs one heart, even during hit grace.

Your gun heats up as you fire and locks for a second if it overheats, and
shots lose up to a quarter of their damage at long range, so it pays to get
close. Quick kills build a multiplier up to ×8 with a rising note for every
kill in the chain; getting hit resets it. Best scores are kept per difficulty in `~/.local/share/glowstorm/`.

The arena is at least 2400 × 1500 game units. Larger terminals expand its width
and height independently to fill the window, including when resized mid-run.

## How it works

- All art is generated in code at start-up, at the window's real pixel size:
  one sprite atlas (rotations are pre-rendered, since kitty can't rotate an
  image) and one large arena floor.
- Both are sent to kitty through shared memory. Frames contain placement commands;
  the floor is rebuilt and uploaded again when resizing changes the arena size.
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
- Linux or macOS
- CoreAudio on macOS, ALSA on Linux; the game runs silently without an audio device

## Known issues

- **Ghostty on Linux:** pressing right-click while holding left-click makes
  Ghostty lose the left-button release, so the gun keeps firing until your next
  left click. Dash with Space instead. kitty isn't affected.

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

`GLOWSTORM_WAVE=6` starts at wave 6, `GLOWSTORM_GOD=1` prevents death but keeps hit reactions, and
`GLOWSTORM_CARDS=1` opens the upgrade screen right away, and `GLOWSTORM_SCALE=0.8`
forces a render scale (0.3–2.2).
Runs with any of them set don't save a best score. Logs go to `$TMPDIR/glowstorm.log`.

## License

MIT, see [LICENSE](LICENSE).
