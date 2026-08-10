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

## APU Implementation

The old APU was a partial channel-1 prototype: it used floating-point phase for
one square wave, returned silence for channels 2-4, reloaded length on every
trigger, and did not implement sweep, envelopes, wave playback, noise, routing,
or hardware-rate sample output. The APU was rebuilt from the project's existing
register bus using Pan Docs and the bundled Blargg sound documentation/source as
the hardware oracle. No emulator implementation was searched for or copied.

### Mental Model and Ownership

The implementation treats the APU as four small digital machines sharing a
slow control clock, rather than as four mathematical oscillators. This matters:
Game Boy software can observe channel status, length expiration, sweep overflow,
and wave RAM contention even when the output volume is zero. Producing a tone
that sounds approximately right is therefore only the final stage of emulation.

The responsibilities are divided as follows:

- `MMU` owns the software-visible audio registers at `FF10-FF3F`. It applies
  read masks, rejects writes while sound is powered down, records edge-sensitive
  writes, exposes `NR52` channel status, and detects DIV-APU falling edges.
- `APU` owns hidden hardware state that software cannot read directly: frequency
  countdowns, duty positions, length counters, envelope timers, sweep shadow
  frequency, wave position/sample buffer, and the noise LFSR.
- A CPU instruction writes registers through the MMU. At the next APU step,
  write flags transfer the relevant register event into hidden channel state.
  This distinction prevents a trigger bit from behaving like persistent memory.
- Channel timers advance in the base 4.194304 MHz domain. The frame sequencer
  advances only on the selected divider falling edge, and host samples are
  emitted by a separate fractional-rate accumulator. None of those clocks is
  derived from wall-clock time.

One useful way to read the resulting signal path is:

```text
CPU register write
  -> MMU masks and records the event
  -> APU updates hidden channel state
  -> frequency timer advances waveform position
  -> envelope/length/sweep frame clocks modify that state
  -> channel emits a 4-bit digital sample
  -> DAC conversion and NR51 routing
  -> NR50 terminal volume and high-pass filter
  -> host-rate stereo sample
```

This separation is also why headless tests can validate the APU without opening
an audio device: the hardware state machines run identically and generated
samples are simply drained from an in-memory channel.

### Hardware State Machines

- Replaced floating phases with explicit pulse frequency timers and eight-step
  duty sequencers for channels 1 and 2.
- Added independent 64-tick pulse/noise length counters and the wave channel's
  256-tick length counter. Length writes reload counters without re-enabling a
  channel; triggers preserve a nonzero length and reload only a zero counter.
- Implemented the fifth-register length-enable edge behavior, including the
  extra length clock in the appropriate half of the frame-sequencer period.
- Added DAC gating for all channels. Clearing `NR12`, `NR22`, `NR30`, or `NR42`
  now disables the channel immediately, while enabling a DAC does not implicitly
  enable its channel.
- Added the 512 Hz DIV-APU frame sequencer: length clocks at 256 Hz, channel-1
  sweep at 128 Hz, and volume envelopes at 64 Hz. CGB double-speed mode watches
  internal DIV bit 13 rather than bit 12 so audio remains in the base-speed
  domain.
- Implemented channel-1 sweep shadow frequency, timer reload, add/subtract,
  trigger-time overflow check, post-update overflow check, private frequency
  copy, and the negate-to-add disable quirk.
- Implemented pulse/noise volume envelopes, including period-zero-as-eight and
  stopping at volume 0/15 without disabling the channel.
- Implemented the 32-sample wave sequencer, 16-byte wave RAM nibble selection,
  output-level shifts, 11-bit frequency timer, trigger phase/timer reset, and
  CGB active-byte wave RAM aliasing.
- Implemented the noise channel's divisor/shift timer, 15-bit LFSR, and seven-bit
  width mode.

### Register Writes, Power, and Triggering

- `NR52` bit 7 is the APU power switch; its low four bits are generated channel
  status, not ordinary writable memory. Powering down disables all channels and
  clears ordinary sound registers while preserving wave RAM. CGB and DMG length
  behavior during power transitions remains explicitly separated.
- Each `NRx4` trigger bit is write-only and event-like. The MMU remembers both
  that a write occurred and the previous length-enable value, allowing the APU
  to distinguish a trigger from an ordinary frequency-high write and to detect
  the documented disabled-to-enabled length transition.
- Triggering initializes the channel's timer/envelope/sweep machinery and may
  enable the channel only if its DAC is enabled. It does not blindly reload a
  nonzero length counter. If length is zero, trigger reloads the channel maximum
  and applies the extra length clock only in the applicable sequencer half.
- Writes to DAC-control registers are consumed before channel status is exposed.
  This makes DAC disable immediate, as observed by the Blargg tests, while DAC
  enable alone never resurrects an expired channel.
- On DMG hardware, length fields remain writable while the APU is off. Only the
  actual length bits are retained for pulse/noise registers; duty bits do not
  become writable merely because they share the same byte.

### Channel-by-Channel Behavior

- **Pulse 1:** an 11-bit frequency selects a timer period. Timer expiration
  advances one of four eight-step duty patterns. Its envelope supplies the
  current 4-bit amplitude. The sweep unit keeps a private trigger-time frequency
  copy, performs add/subtract calculations at 128 Hz, writes successful updates
  back to the frequency registers, and disables on overflow.
- **Pulse 2:** uses the same duty, frequency, length, DAC, and envelope machinery
  without sweep. Sharing a channel structure removes duplicated timing behavior
  while leaving pulse 1's sweep state independent.
- **Wave:** a 32-position sequencer consumes the high and low nibbles of 16-byte
  wave RAM. `NR32` selects mute, full, half, or quarter level by shifting the
  current 4-bit sample. The length maximum is 256 rather than 64. CGB active
  accesses alias the currently fetched wave byte, which is why reads from
  different wave addresses can return the same value while playback is active.
- **Noise:** timer expiration shifts a 15-bit LFSR whose XOR feedback creates
  pseudo-random output. Width mode additionally copies feedback into bit 6,
  producing the shorter repeating pattern used by many percussion effects.
  The resulting low bit gates the envelope's 4-bit amplitude.

The frame sequencer is intentionally orthogonal to waveform timers. Pulse duty,
wave position, and noise LFSR may advance thousands of times between envelope
changes; length, sweep, and envelope clocks occur only at 256, 128, and 64 Hz.

### Mixing and Frontend Output

- Each channel now produces its documented four-bit digital output. `NR51`
  routes channels to left/right, `NR50` applies terminal volume, and DAC output
  is centered before separate stereo terminal samples pass through a DC-blocking
  high-pass stage. Disabled channels contribute silence rather than DAC code 0.
- CGB `PCM12` and `PCM34` expose the live pre-mixer digital channel values.
- Sample timing now uses a fractional CPU-clock accumulator rather than a fixed
  91-cycle approximation, and macroquad passes the actual host output sample
  rate into the APU.
- The cpal callback now consumes one stereo emulator sample per host audio frame
  and maps it to the device's left/right channels. Previously it consumed a new
  mono sample for every interleaved speaker slot, halving playback speed on
  stereo devices.
- Host audio remains opt-in through `--apu`; headless emulation continues to run
  and drain the APU without needing an audio device.

The mixer does not average all four channels before routing. Each enabled
channel first becomes an analog-like DAC value, then `NR51` independently sends
it to the left terminal, right terminal, both, or neither. `NR50` scales the two
terminal sums separately. A disabled channel contributes true silence; this is
different from an enabled channel whose current digital sample is zero, because
DAC code zero is still an electrical level. The high-pass state removes that DC
component over time and is reset when the APU powers down.

The host sample accumulator adds `t_cycles * host_sample_rate` and emits while
the result exceeds 4,194,304. This preserves fractional timing at common rates
such as 44.1 and 48 kHz without periodically drifting or hard-coding a rounded
cycles-per-sample value. The cpal callback consumes one `(left, right)` pair per
host frame; mono devices receive the average and devices with extra channels
receive the stereo pair plus an averaged fill for remaining channels.

### Verification Harness and Results

- Added Blargg's cartridge-RAM result protocol to the headless runner: signature
  `DE B0 61` at `A001-A003`, status at `A000`, and zero-terminated diagnostics at
  `A004`. Detection waits for nonempty text to avoid falsely accepting the brief
  zero-status interval while a ROM initializes its report buffer.
- Added command-line reporting of Blargg memory status and diagnostics, a fast
  protocol decoding test, and a fast trigger/DAC channel-status test.
- Added an ignored aggregate regression for all currently passing sound ROMs.
- Baseline before the rewrite was 1/12 DMG and 1/12 CGB. The current result is
  7/12 DMG and 9/12 CGB, with all former sweep timeouts eliminated.
- Passing in both modes: registers, length counters, trigger timing, sweep,
  sweep details, trigger overflow, and registers after power. CGB also passes
  wave retrigger and its dedicated wave timer/phase/access test.
- All active Rust regressions pass. Remaining Blargg failures are sequencer phase
  across APU power, length persistence across APU power, and DMG/CGB active wave
  RAM cycle windows. Those require finer memory-access timing than the current
  instruction-batched APU boundary exposes and are documented rather than hidden.

The improvement was measured incrementally rather than inferred from audible
output. The corrected initial baseline exposed a false-positive hazard in the
Blargg memory protocol: the suite writes its signature before changing status
from zero to `80`. Accepting zero immediately therefore reported a blank test as
passed. Requiring nonempty diagnostic text removed that race and produced the
real 1/12 plus 1/12 baseline.

The first state-machine pass immediately converted sweep tests from timeouts to
specific assertions and made overflow-on-trigger pass. Subsequent fixes were
then driven by exact diagnostics: triggers had been incorrectly reloading every
length counter; extra clocks depended on the current sequencer half; changing
sweep from subtract to add needed the hidden `negate_used` state; and powered-off
DMG writes needed field-level masking. This progression is preserved in the
final architecture rather than special-casing individual test ROMs.

The remaining wave access failures illustrate the current timing boundary. CPU
instructions execute their memory access before DeityGB advances the APU for the
instruction's aggregate cycle count. Real DMG wave RAM is available only during
a very narrow interval around the channel's own byte fetch. Correctly deciding
whether a CPU read landed inside that interval ultimately requires interleaving
bus accesses and APU timer advancement more finely than one post-instruction
call. The implementation keeps the broad hardware behavior and documents that
architectural limitation instead of adding ROM-specific timing guesses.

### Reproduction Commands

Fast active tests:

```sh
nix develop --command cargo test --release --lib --tests
```

The 16-ROM passing sound checkpoint:

```sh
nix develop --command cargo test --release --test headless \
  blargg_sound_core_roms_pass -- --ignored --exact
```

Interactive CGB gameplay with audio:

```sh
nix develop --command cargo run --release --bin DeityGB -- \
  src/roms/pokemon_silver.gbc --apu
```

For debugging individual remaining cases, the headless binary prints both the
outcome and Blargg's escaped memory report:

```sh
nix develop --command cargo run --release --bin gb-headless -- \
  "src/roms/gb-test-roms/dmg_sound/rom_singles/09-wave read while on.gb" \
  --seconds 30
```
