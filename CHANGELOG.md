# Changelog

## [Unreleased]

- Native Windows support in Rustty: streamed graphics uploads, VT keyboard and
  pixel mouse input, terminal size queries, and Windows score/log paths.
  `--graphics-transport direct` also enables streaming on Unix.
- Seeded ordinary waves add synchronized reinforcements, volatile wrecks, and
  sweeping crossfire, each announced before enemies begin spawning.
- Each boss offers a unique optional trophy card that adds an automatic power
  for the rest of the run.
- `--seed` / `GLOWSTORM_SEED` selects gameplay randomness and shuffles later
  enemy combinations. The default is 1, and retries reuse the selected seed.
- Title screen tells Ghostty players to dash with Space: Ghostty loses the
  left-button release when right-click is pressed during fire.

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
