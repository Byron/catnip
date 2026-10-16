# catnip-engine

A small Rust game engine for terminals that speak the
[kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
(kitty, Ghostty, Rustty). Imported as `catnip`.

- `kitty`: image uploads through shared memory on Unix or direct streaming on
  Windows, placements, a per-frame
  placement manager, upload verification
- `term`: terminal setup and restore, window size, terminal detection,
  kitty keyboard protocol check, and a portable `read_event` API that preserves
  kitty key releases and pixel mouse reports on Windows
- `gfx`: procedural shapes, neon glow, atlas packing (paged)
- `font`: a 5×7 pixel font for sprite text
- `synth`: sound synthesis and playback

The idea: upload sprite sheets once, then every frame only sends placement
commands, so the terminal's GPU does the compositing. See
[Glowstorm](https://github.com/Hemantabhusal/catnip) for a full game built on it.

`kitty::Transport::Direct` also works on Unix. Upload methods accept any
`io::Write`, so a buffered terminal writer can stream large images without a
second image-sized buffer. `term::read_event` must be the only event reader;
do not mix it with crossterm input calls on Windows.

MIT licensed.
