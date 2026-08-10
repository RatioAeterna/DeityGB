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

## Later Session: Kirby Stage-Intro Fix

Kirby's Dream Land previously reached the Stage 1 / Green Greens card but
never entered gameplay. The frontend remained responsive at roughly 57 FPS,
which established that this was an emulated CPU control-flow problem rather
than a host-window hang.

### Reproduction and Diagnosis

- Reproduced the failure deterministically with `gb-headless`, a Start pulse at
  emulated second 10, APU disabled, and framebuffer captures between seconds
  13 and 40.
- Identified the wait loop in Kirby ROM bank 6:
  - `0x4399`: `CB F6` / `SET 6,(HL)`, setting bit 6 at HRAM `FF8C`.
  - `0x439B`: `HALT`, waiting for VBlank.
  - `0x439C`: `CB 76` / `BIT 6,(HL)`.
  - `0x439E`: `JR NZ`, repeating until the VBlank handler clears the bit.
- Confirmed that Kirby's VBlank handler clears the `FF8C` wait flag, but the
  stage countdown in `DE` remained fixed at `0x013F` and execution repeatedly
  appeared halted at `0x439C` or `0x439E`.
- Added headless reporting for `DE`, HALT/IME state, LYC, DIV/TIMA/TMA/TAC,
  JOYP, and the transition-related `FF8B`, `FF8C`, `FF8E`, `FF94`, and `D03B`
  values. These diagnostics made the bad interrupt return address visible.

### CPU Bugs Found

Two ordering bugs combined specifically when `HALT` was followed by a
CB-prefixed instruction:

1. `CPU::cycle` fetched and processed the CB prefix before checking whether the
   CPU was halted. Every idle HALT tick could therefore mutate PC even though a
   halted CPU must not fetch an opcode.
2. After an interrupt woke HALT, interrupt arbitration still occurred inside
   `decode_execute`, after the CB-prefix fetch had advanced PC. VBlank pushed
   `0x439D` instead of `0x439C` as its return address. `RETI` then returned into
   the `0x76` operand of `CB 76`, interpreting that operand as a standalone
   `HALT` and trapping the transition.

An intermediate hypothesis blamed the general CB instruction length. A focused
test disproved it: the decoder intentionally backs PC up after reading the
prefix, so its existing two-byte final increment was correct. That provisional
change was reverted before the final fix.

### Implementation

- HALT is now resolved before any opcode or prefix fetch. With no enabled
  pending interrupt it consumes 4 T-cycles, updates DIV/TIMA, and leaves PC
  untouched.
- Interrupt priority and entry now occur before opcode fetch for both running
  and newly awakened CPUs.
- Interrupt entry clears the selected IF bit, disables IME, pushes the exact
  current PC, jumps to the corresponding vector, and consumes the hardware's
  20 T-cycles instead of the previous incorrect 5.
- Added CPU state getters used by the headless diagnostics.

### Regression Coverage and Results

- Added a focused regression proving an idle halted CPU cannot consume a
  following `0xCB` byte or advance PC.
- Added a regression proving VBlank after `HALT` is serviced before a following
  CB-prefix fetch and pushes the correct return address.
- Added an ignored deterministic Kirby ROM smoke test. It presses Start at
  second 10 and verifies a Green Greens gameplay framebuffer at second 18 with
  FNV-1a hash `e2149d8593366ec9`.
- Default headless integration suite: 10 passed, 4 ignored.
- Kirby title-to-gameplay smoke test: passed.
- Pokemon Red title-to-New Game framebuffer regression: still passed with its
  existing hash.
- Blargg `cpu_instrs`: all 11 groups reported `ok`; overall result `Passed`.
- Blargg `interrupt_time` still displays `Failed`. Interrupt ordering and entry
  duration are now corrected, but cycle-exact timer/bus interrupt behavior is
  still incomplete and remains a separate accuracy project.

## Fierce Deity Frontend Branding

- Embedded `assets/fierce_deity.png` in the executable and display it for 1.8
  seconds before CPU/MMU construction and boot-ROM execution.
- Replaced the previously unused local Macroquad `Conf` value with the actual
  `window_conf` callback, preserving the intended 800x720 window title/size.
- Generate 16x16, 32x32, and 64x64 RGBA icon variants for Miniquad-supported
  platforms.
- Added a macOS Cocoa application-icon hook because the pinned Miniquad release
  does not implement macOS Dock icons. The Fierce Deity art now appears in the
  Dock while DeityGB runs.
- Kept all branding work isolated from emulation state and timing. The splash
  completes before the Game Boy core is constructed.
