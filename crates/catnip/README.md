# catnip-engine

A small Rust game engine for terminals that speak the
[kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
(kitty, Ghostty). Imported as `catnip`.

- `kitty`: image uploads through shared memory, placements, a per-frame
  placement manager, upload verification
- `term`: terminal setup and restore, window size, terminal detection,
  kitty keyboard protocol check
- `gfx`: procedural shapes, neon glow, atlas packing (paged)
- `font`: a 5×7 pixel font for sprite text
- `synth`: sound synthesis and playback

The idea: upload sprite sheets once, then every frame only sends placement
commands, so the terminal's GPU does the compositing. See
[Glowstorm](https://github.com/Hemantabhusal/catnip) for a full game built on it.

MIT licensed.
