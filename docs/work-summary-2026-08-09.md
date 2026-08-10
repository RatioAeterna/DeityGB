# DeityGB Work Summary - 2026-08-09

This checkpoint covers the work completed during the August 8-9 development
session. It preserves the first build in this session that was manually played
through Oak's introduction, Pallet Town, the first rival battle, Route 1,
Viridian City, wild battles, and delivery of Oak's Parcel.

## Development and Verification Setup

- Split the emulator modules into a reusable `deitygb` library while retaining
  the macroquad desktop frontend.
- Added `gb-headless`, a command-line runner for deterministic emulation without
  a window or audio device.
- Added bounded runs by cycle/second count, scripted button taps, serial output
  collection, pass/fail/timeout reporting, CPU/MMU state summaries, and 160x144
  PPM framebuffer capture.
- Added unit and integration coverage for serial transfers, framebuffer shape,
  joypad selection and interrupts, off-screen sprites, LCD disable behavior,
  MBC1 banking, MBC3 banking/RAM, and one-way boot-ROM disable behavior.
- Added opt-in local ROM regressions for Blargg CPU instructions, Mooneye DAA,
  and Pokemon Red's title-to-New Game path.
- Added development documentation and reproducible Nix commands for GUI,
  headless, visual, and ROM-suite verification.
- Updated the macOS Nix development environment and made the launch scripts run
  the release frontend with Pokemon Red from a stable repository-root path.

## Pokemon Red and Cartridge Support

- Implemented the MBC3 ROM banking and banked external-RAM subset needed by
  Pokemon Red. RTC registers and latching remain unimplemented.
- Corrected MBC1 mode-one banking behavior.
- Reworked joypad state into separate direction/button groups and added the
  falling-edge joypad interrupt behavior needed for reliable menu input.
- Fixed the startup/New Game crash caused by off-screen sprite coordinates
  underflowing in debug builds.
- Disabled the experimental APU and audio-device initialization by default.
  Audio can be explicitly enabled with `--apu`.

## PPU and Frontend Stability

- Added LCD disable/re-enable handling: reset LY, mode, accumulated PPU cycles,
  and window state when the LCD is turned off, then restart from OAM mode.
- Corrected sprite clipping, sprite row/flip calculations, palette application,
  window coordinates, and window line-counter resets.
- Changed frontend pacing to the DMG frame duration of 70,224 cycles at about
  59.7 Hz.
- Kept the macroquad event loop alive once per host frame while the emulated LCD
  is disabled, preventing transition code from starving window/input polling.
- Replaced per-frame GPU texture creation with one persistent 160x144 texture
  updated in place. Macroquad 0.4.0 otherwise leaked the old GPU texture each
  frame, eventually producing blank or striped output.
- Added opt-in `--capture-lcd` diagnostics. It logs CPU bank/PC and LCD state at
  each LCD edge and captures both emulator and actual-window images under
  `/tmp/deitygb-lcd-*`.

## Boot-ROM Transition Fix

- Removed a CPU hack that forcibly jumped to `0x0100` whenever `FF50` contained
  `1`. Pokemon writes this register during scene setup; the forced jump sent
  execution through cartridge header data and into a repeating `RST 38` loop,
  visibly producing stripes.
- Made boot-ROM disable one-way until emulator reset while retaining DeityGB's
  established post-boot sentinel value for compatibility with existing paths.
- Added a regression test proving a later zero write cannot remap the boot ROM.

## APU Work Preserved

The pre-existing uncommitted APU work is included in this checkpoint. It adds
channel frequency/length/envelope state, trigger tracking, DAC-disable handling,
DIV-APU sequencing, and sample generation changes. This work is experimental;
the frontend therefore leaves it disabled unless `--apu` is supplied.

## Save Support Status

Battery-backed save persistence was attempted, found to corrupt/inappropriately
flush Pokemon SRAM, and then removed completely. This checkpoint does not load,
poll, or write `.sav` files. Save support should be redesigned after the core is
more deterministic and added behind game-level regression coverage.

## Verification at This Checkpoint

- Default unit/integration suite: 8 passed, 3 ignored.
- Pokemon Red release title-to-New Game regression: passed.
- Manual playthrough reported working through multiple battles, Viridian City,
  and delivery of Oak's Parcel.
- `git diff --check`: clean before commit.

## Known Risks and Next Priorities

Transition failures that appear nondeterministic are likely timing-sensitive,
not random. Host input duration changes interrupt alignment and can expose one
of several known accuracy gaps at different scene boundaries.

1. Correct interrupt entry timing (currently accounted as 5 cycles rather than
   20 T-cycles) and add focused interrupt timing tests.
2. Implement HALT and the HALT bug from hardware behavior; the current scaffold
   contains a deliberately disabled branch.
3. Model OAM DMA duration and CPU/bus restrictions instead of copying all 160
   bytes instantaneously.
4. Tighten PPU mode timing, STAT edge behavior, and VRAM/OAM access restrictions
   against Mooneye tests.
5. Turn the manual Pokemon route and first battle into deterministic input/state
   regressions before making further persistence changes.
