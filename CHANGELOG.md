# Changelog

## [0.1.0] - 2026-10-10

The first release: Glowstorm, and the Catnip engine it runs on.

### Glowstorm
- Twin-stick neon arena shooter at 60 fps in kitty and Ghostty: WASD to move,
  mouse to aim, hold click to fire, Space or right click to dash.
- Four enemy types with telegraphed attacks: lunging chasers, flanking swarms,
  shooters that lead their shots, and tanks with shockwaves and heavy volleys
  that enrage when left for last.
- Elite enemies from wave 6, and PRISM, a three-phase boss every fifth wave.
- Seven upgrade cards to pick from after each boss.
- Medium, Hard and Extreme, with separate best scores.
- Gun heat, damage falloff, dash charges, combo multiplier with rising notes,
  score popups, slow motion and screen flashes.
- All art and sound are generated at start-up; no asset files.
- Opens itself in kitty or Ghostty when started from another terminal.

### Catnip engine (`catnip-engine`)
- kitty graphics protocol: shared-memory uploads, paged sprite atlases,
  per-frame placement manager, upload verification.
- Terminal setup and restore, terminal detection, kitty keyboard protocol check.
- Procedural shapes with neon glow, a 5×7 pixel font, sound synthesis.
