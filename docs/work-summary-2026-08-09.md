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

## Game Boy Color / Pokemon Silver

### Goal and Baseline

The goal was not merely to make a CGB-capable ROM execute in DMG compatibility
mode. Pokemon Silver needed to recognize color hardware, use its native tile
attributes and palettes, and reach a visibly correct color title screen without
regressing the now-playable Pokemon Red and Kirby paths.

Before implementation, the emulator had a flat 64 KiB MMU, one 8 KiB VRAM
bank, one fixed/switchable DMG WRAM arrangement, and a PPU framebuffer packed
as four 2-bit monochrome pixels per byte. Macroquad and the headless runner each
expanded that packed buffer into the four DMG green shades. CGB registers were
mostly caught by a placeholder and ignored. `STOP` permanently stopped the CPU,
and the frontend advanced every subsystem by the same CPU cycle count.

The local Pokemon Silver cartridge was inspected before changing the core. Its
header marks it as a dual-mode CGB cartridge (`0143=80`) using MBC3 with timer,
RAM, and battery (`0147=10`), a 2 MiB / 128-bank ROM, and 32 KiB / four-bank
external RAM. Its SHA-256 is
`72b190859a59623cbef6c49d601f8de52c1d2331b4f08a8d2acc17274fc19a8c`.
It already reached intro/name screens through the DMG path, which gave us a
useful CPU/MBC baseline but proved that cartridge execution alone was not CGB
support.

The implementation was based on Pan Docs' memory map, CGB register, tile map,
tile data, OAM, palette, priority, speed-switch, and VRAM DMA documentation.
No implementation code was searched for or copied from another emulator.

### CGB Mode Selection and Boot Handoff

- `MMU::load_rom` now reads the cartridge CGB flag and records whether the
  current machine should expose CGB hardware. CGB-only (`C0`) and dual-mode
  (`80`) headers both select CGB mode; DMG cartridges retain the old path.
- DeityGB still uses its bundled 256-byte DMG boot ROM, preserving the Nintendo
  boot animation and the known working boot flow. That ROM cannot perform the
  CGB boot ROM's final register initialization.
- At the exact point where the boot ROM permanently unmaps itself through
  `FF50`, the CPU performs a one-time compatibility handoff. For a CGB
  cartridge it sets A to the documented CGB value `11`; for DMG cartridges it
  leaves the accumulator untouched. Pokemon Silver consequently detects color
  hardware and enters its CGB code path after the familiar DMG boot animation.
- The handoff is explicitly one-shot so later reads/writes to boot state cannot
  unexpectedly rewrite A during cartridge execution.

### Banked CGB Memory and Registers

- Added the second 8 KiB VRAM bank and implemented `VBK` (`FF4F`). CPU VRAM
  reads/writes use the selected bank, while the PPU can explicitly read either
  bank so tile IDs remain in bank 0 and tile attributes come from bank 1.
- Added CGB WRAM banks 1-7 and implemented `SVBK` (`FF70`), including the
  hardware rule that selecting bank 0 aliases bank 1. Bank 0 at `C000-CFFF`
  remains fixed; `D000-DFFF` and its echo follow the selected bank.
- Added 64-byte background and object palette RAM arrays. `BGPI/BGPD`
  (`FF68-FF69`) and `OBPI/OBPD` (`FF6A-FF6B`) implement six-bit indices and
  bit-7 auto-increment after data writes.
- Palette entries are decoded as little-endian RGB555. Each five-bit channel is
  expanded to eight bits for the frontend while bit 15 is ignored.
- Implemented CGB object-priority mode `OPRI` (`FF6C`). OAM-index ordering is
  used in the CGB default mode, while coordinate ordering remains available
  through the register and remains the DMG behavior.
- Implemented `KEY1` (`FF4D`) preparation/current-speed bits and `STOP`-driven
  speed switching. A prepared CGB `STOP` toggles speed and continues execution;
  an unprepared `STOP` retains the existing stopped-CPU behavior.
- Added basic documented values for the remaining exposed CGB status ports so
  CGB software no longer falls through the old noisy "unknown CGB register"
  path for implemented hardware.

### CGB VRAM DMA

- Implemented `HDMA1-HDMA5` (`FF51-FF55`) source, destination, length, status,
  cancellation, and mode selection.
- General-purpose DMA copies all requested 16-byte blocks immediately into the
  currently selected VRAM bank.
- HBlank DMA records an active transfer and copies one 16-byte block whenever
  the PPU enters HBlank on a visible scanline, updating source, destination,
  remaining-block status, and completion state after each block.
- Destination addresses are constrained to the documented `8000-9FF0` VRAM
  range and both source/destination low nibbles are masked to 16-byte alignment.

### Color PPU and Pixel Composition

- Added an RGBA framebuffer alongside the existing packed 2-bit framebuffer.
  The packed buffer remains available for old diagnostics, while RGBA is now
  authoritative for display, screenshots, and visual regression hashes.
- Background and window map tile IDs are fetched from VRAM bank 0. In CGB mode,
  the corresponding byte in bank 1 supplies palette number, VRAM tile-data
  bank, horizontal flip, vertical flip, and BG-priority attributes.
- Signed (`8800/9000`) and unsigned (`8000`) tile-data addressing both accept a
  VRAM bank. Flips are applied to the pixel position within each tile before
  reading the two bitplanes.
- The window renderer now follows CGB enable semantics and consumes the same
  complete attribute set as the background. Window pixels replace both the
  visible color and per-pixel priority metadata used by later OBJ composition.
- Each rendered BG/window pixel records its raw two-bit color index and tile
  priority in scanline-sized metadata buffers. Raw color is essential because
  CGB sprite priority depends on whether the BG color number is zero, not on
  the final RGB value.
- OAM parsing now records the CGB tile-data bank and three-bit OBJ palette.
  Sprite tile fetches support both banks, 8x8/8x16 selection, both flips,
  clipping, and the existing ten-sprites-per-scanline selection limit.
- CGB object composition implements LCDC bit 0 as the BG/window master-priority
  control, BG color-zero transparency to OBJ, BG tile priority, OBJ's
  behind-BG bit, and CGB OAM ordering. Transparent OBJ color zero never writes.
- The shared compositor also fixed two latent DMG sprite errors: transparency
  is now decided from raw OBJ color zero rather than the palette-mapped shade,
  and the OBJ behind-BG flag now checks the raw BG color. This deliberately
  changed Kirby's framebuffer while producing a visibly coherent Green Greens
  scene, so its regression baseline was updated rather than preserving the bug.

### Native RGBA Frontends and Timing

- Macroquad now copies the PPU RGBA buffer directly into its texture at VBlank.
  The old per-frame loop that unpacked 2-bit pixels and hard-coded four green
  shades was removed.
- The headless runner returns the exact same RGBA buffer, so automated captures
  and the interactive frontend exercise one rendering path.
- DMG rendering still maps through `BGP/OBP0/OBP1` to the exact previous green
  values. Pokemon Red therefore retained its existing framebuffer hash despite
  the frontend conversion moving into the PPU.
- In double speed, CPU instructions and CPU timers continue using their full
  cycle counts. PPU, APU, frontend frame accounting, and emulated RTC receive
  half that count, keeping them at the base 4.194304 MHz domain documented for
  CGB double-speed operation.

### Pokemon Silver's MBC3 RTC

- Pokemon Silver's cartridge type includes an MBC3 real-time clock, so leaving
  RTC banks `08-0C` as `FF` would have produced broken clock behavior after the
  colorful startup succeeded.
- Added live seconds, minutes, hours, low day, and high day/control registers.
  The day counter is nine bits and implements the halt and overflow/carry bits.
- RTC registers can be selected through the existing MBC3 RAM-bank register and
  read/written through `A000-BFFF` while cartridge RAM/RTC access is enabled.
- Implemented the documented `0 -> 1` latch sequence in `6000-7FFF`. Reads use
  a stable latched snapshot until the next latch while the live clock continues
  advancing.
- Time advances deterministically from emulated base-speed cycles rather than
  host wall-clock time. This keeps headless runs and visual regressions
  reproducible and naturally respects RTC halt.
- This work intentionally did not restore save-file persistence. Cartridge RAM
  and RTC state remain in memory only, separating CGB correctness from the
  earlier persistence regression.

### Regression Coverage and Evidence

- Added focused tests for CGB VRAM bank isolation and `VBK` selection.
- Added tests for WRAM bank isolation, `SVBK`, and bank-zero aliasing behavior.
- Added palette-port tests covering auto-increment and RGB555 decoding.
- Added a general VRAM DMA test proving a 16-byte source block reaches the
  selected VRAM bank and completion reports `FF` through `HDMA5`.
- Added a CPU test proving the one-time `A=11` boot handoff and a prepared
  `STOP` transition into double speed without stopping execution.
- Added a synthetic PPU test proving a bank-1 tile attribute selects CGB palette
  1 and renders an exact red RGBA pixel from RGB555 data.
- Added MBC3 RTC coverage for one-second advancement, stable latching, halt,
  and relatching the live value.
- Added an ignored Pokemon Silver title-screen regression. After 38 emulated
  seconds it verifies multiple distinct RGB colors and FNV-1a framebuffer hash
  `726a43cc196d20cf`.
- Captured and visually inspected Pokemon Silver startup frames. The native
  title screen shows the blue background, gold/red Pokemon logo, silver
  subtitle, dark creature silhouette, multicolor ground, and white copyright
  text instead of the four DMG greens.
- Final active release suite: 16 passed, 5 ignored.
- Pokemon Red retained exact hash `d70ba3bc7247de85`.
- Kirby reached Green Greens and passed its corrected compositor hash
  `d5a4c17fb316c4e3`.
- The bundled Blargg `cpu_instrs` aggregate test passed all instruction groups.
- `git diff --check` reported no whitespace errors, and all release targets
  compiled as part of the test run.

### Remaining Accuracy Boundaries

- This uses the bundled 256-byte DMG boot ROM plus a documented register
  handoff, not a dumped CGB boot ROM and its complete power-on register state.
- VRAM DMA currently transfers correct data but does not stall the CPU for the
  exact documented duration. OAM DMA remains the project's existing immediate
  transfer model.
- Palette access blocking during mode 3, infrared behavior, and CGB-specific
  audio differences are not cycle-accurate. The APU remains a separate project.
- RTC state and cartridge RAM are not yet persisted to disk.
