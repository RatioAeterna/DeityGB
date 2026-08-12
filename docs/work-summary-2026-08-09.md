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
- Added an ignored aggregate regression for the sound ROMs passing at this
  initial checkpoint.
- Baseline before the rewrite was 1/12 DMG and 1/12 CGB. This first checkpoint
  reached 7/12 DMG and 9/12 CGB, with all former sweep timeouts eliminated.
- Passing in both modes: registers, length counters, trigger timing, sweep,
  sweep details, trigger overflow, and registers after power. CGB also passes
  wave retrigger and its dedicated wave timer/phase/access test.
- All active Rust regressions passed. The tests left for the completion pass
  covered sequencer phase across APU power, length persistence across power,
  and DMG/CGB active wave-RAM cycle windows.

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

Those wave access failures exposed the timing boundary that the later completion
pass addressed. CPU
instructions execute their memory access before DeityGB advances the APU for the
instruction's aggregate cycle count. Real DMG wave RAM is available only during
a very narrow interval around the channel's own byte fetch. Correctly deciding
whether a CPU read landed inside that interval required preserving the CPU bus
phase and projecting channel-3 state to it. The completed model is described in
`APU Timing Completion: 24/24 Blargg Sound ROMs` below.

### Reproduction Commands

Fast active tests:

```sh
nix develop --command cargo test --release --lib --tests
```

The complete 24-ROM sound regression:

```sh
nix develop --command cargo test --release --test headless \
  blargg_sound_core_roms_pass -- --ignored --exact
```

## Battery-Backed Cartridge Saves and MBC3 RTC Persistence

This work adds normal cartridge persistence for games such as Pokemon Red and
Pokemon Silver. It is deliberately not an emulator save-state feature: only the
external cartridge RAM and MBC3 RTC state that real battery-backed cartridges
retain across power cycles are written to disk.

### File Ownership and Format

Persistence is owned by `cartridge_save`, not by the CPU, PPU, frontend loop, or
headless runner. The MMU remains the owner of cartridge-visible state: external
RAM bytes, MBC bank registers, RTC registers, latched RTC values, halt/carry
bits, and dirty flags. The save layer owns host paths, startup loading, atomic
file replacement, and shutdown/debounce flushing.

Save paths are derived only from the selected ROM path. DeityGB does not scan
directories and does not discover unrelated legacy saves. A ROM
`pokemon_silver.gbc` maps to adjacent files:

- `pokemon_silver.sav`: raw cartridge SRAM only.
- `pokemon_silver.rtc`: DeityGB's versioned MBC3 RTC sidecar.

The raw `.sav` file is exactly the RAM size declared by the cartridge header
RAM-size byte. For Pokemon Silver (`0147=10`, `0149=03`) this is 32 KiB, four
8 KiB banks. Pokemon Crystal uses the same MBC3 timer/RAM/battery cartridge
type with RAM-size code `05`, the MBC30-style 64 KiB variant with eight SRAM
banks. Banks `00-07` are treated as cartridge RAM and banks `08-0C` remain the
RTC register window. Keeping RTC out of `.sav` preserves interoperability with
tools that expect the file to contain only cartridge RAM.

The `.rtc` sidecar is a small text format beginning with
`DEITYGB_MBC3_RTC_V1`. It stores the five MBC3 RTC registers, the sub-second
cycle accumulator, and the host Unix timestamp at flush time. On load, elapsed
host seconds are applied if the RTC halt bit is clear. If the halt bit is set,
the clock remains stopped. Day bit 8 and the carry bit live in RTC register
`0C` as Pan Docs specifies; overflow wraps the 9-bit day counter and sets carry
until software clears it by writing the register.

### Lifecycle

The macroquad frontend and headless runner load the ROM first, then initialize
cartridge persistence exactly once from the resulting header metadata. This is
important because battery/RAM/RTC eligibility comes from the cartridge header,
not filenames or previous files on disk.

Missing save files are normal and leave first boot pristine. No file is created
until the emulated game actually writes cartridge RAM or RTC registers, or the
running RTC advances and marks RTC state dirty. Truncated saves copy the bytes
that exist and leave the rest of external RAM zeroed. Oversized saves copy only
the declared RAM range and are rewritten to the declared size on the next dirty
flush. Malformed RTC sidecars are ignored without panicking or corrupting RAM.

Only cartridge types whose header declares a battery enable persistence. MBC3
RAM without battery, for example, still has in-memory external RAM while the
emulator runs, but the save manager stays disabled and never writes a `.sav`.

### Flush Policy

Cartridge RAM writes through `A000-BFFF` mark RAM dirty after the active MBC has
resolved enable state and bank selection. MBC3 RTC register writes mark RTC
state dirty; ordinary elapsed RTC seconds are restored from the sidecar's host
timestamp rather than forcing repeated disk writes while a game is merely
running. The frontend checks dirty state on a one-second debounce and on
shutdown. The headless runner flushes once before exit.

Flushes use a temporary file beside the target followed by `rename`, so a crash
or interruption should leave either the old complete save or the new complete
save. Ordinary frames, idle loops, and gameplay without cartridge RAM/RTC
changes do not write to disk.

### Verification

The default integration suite now includes focused persistence regressions:

- Clean battery first boot does not create a save file.
- MBC3 SRAM banks round-trip through `.sav`.
- Pokemon Crystal's MBC30-style banks 4-7 round-trip before the RTC register
  window.
- Saved RAM length is exactly the cartridge header's declared size.
- Truncated and oversized `.sav` files load safely and rewrite to declared size.
- Non-battery cartridges do not persist external RAM.
- Dirty flushes are atomic and clear dirty state.
- MBC3 RTC sidecars round-trip, apply elapsed host time, and respect halt.

The design follows Pan Docs cartridge header, external RAM, and MBC3 RTC
documentation, plus the project's existing MBC tests. No implementation from
another Game Boy emulator was searched for or copied.

## Blargg CPU/Memory Timing and DMG OAM Checkpoint

This checkpoint broadens DeityGB's verification from instruction results and
audio behavior into CPU bus ordering, divider-driven events, interrupt entry,
LCD startup, and the original DMG's OAM corruption defect. It was developed
against Pan Docs and the bundled Blargg sources and ROM output. No
implementation from another emulator was consulted.

### Why Correct Instruction Totals Were Not Enough

DeityGB already returned the correct total cycle count for most instructions,
which was sufficient for `cpu_instrs` and `instr_timing`. The memory-timing ROMs
observe *when within an instruction* a read or write reaches the bus. Advancing
all timers after an instruction allowed an access on the final machine cycle to
see hardware state from the instruction's beginning.

The CPU now records how many T-cycles of the current instruction have already
advanced. `timed_read` and `timed_write` advance timers to the relevant bus
phase before touching the MMU; the epilogue advances only the remainder.
Absolute and high-memory loads use this path, as do `(HL)` operations. For
CB-prefixed `(HL)` opcodes, BIT performs its read at the tested boundary, while
rotates, shifts, RES, and SET perform the later write too. This is a generic bus
model rather than a list of test-ROM addresses.

Result: both generations of `mem_timing` pass all three groups, while
`cpu_instrs` remains 11/11 and `instr_timing` remains passing.

### Delayed TIMA Reload and Clocked Serial

TIMA overflow is now represented as a state transition. Overflow first changes
TIMA to `00`; four T-cycles later TMA is copied into TIMA and IF bit 2 is
requested. Writing TIMA during that pending interval cancels the reload. A unit
test records these boundaries so timer work cannot silently restore immediate
reload behavior.

Internal-clock serial transfers are likewise no longer instantaneous. A normal
transfer completes after 4096 T-cycles and CGB fast serial after 128. Completion
captures the outgoing byte, replaces SB with `FF`, clears SC bit 7, and requests
the serial interrupt. This preserves Blargg's reporting protocol while making
serial interrupt timing meaningful.

### Interrupt Entry and CGB Double Speed

Interrupt service remains a five-machine-cycle, 20 T-cycle operation. The PC
high and low bytes now reach the stack on their respective entry machine cycles,
with the remaining service time advanced afterward. The implementation now
models the documented sequence instead of only returning the correct total.

Blargg's `interrupt_time` also verifies CGB speed without trusting KEY1: it
counts CPU instructions while an APU length counter advances at the base
hardware rate. Its final output identifies normal speed as `00`, double speed
as `01`, measures `0D` in both modes, and reports `Passed`. This jointly checks
STOP/KEY1 switching, peripheral scaling, DIV-APU timing, and interrupt latency.

### DMG OAM Corruption as a CPU/PPU Interaction

During mode 2 on original DMG hardware, 16-bit increment/decrement and stack
bus activity can corrupt the OAM row currently scanned by the PPU. CGB hardware
does not have this defect. The PPU now publishes its mode and cycle position to
the MMU. Corruption is applied only with the LCD enabled, in mode 2, on DMG,
for a 16-bit address in `FE00- FEFF`, and after the first row.

The CPU reports IDU activity from INC/DEC BC, DE, HL, and SP; auto-changing HL
loads; and stack operations. POP combines its first OAM read with IDU activity
and places its second read one machine cycle later. PUSH models its initial IDU
event and two writes on successive machine-cycle offsets. The MMU implements
the documented write/IDU, read, and combined read-plus-IDU row formulas.

The aggregate OAM ROM has a dual-mode header, so `--dmg` explicitly selects the
hardware on which the defect exists. The override is applied before ROM loading
and does not change normal CGB selection for Pokemon Silver. With forced DMG,
all eight OAM groups pass, including the instruction-effect checksum.

### First LCD Line Timing

The OAM suite exposed a first-line LCD synchronization edge. A broad cycle
offset fixed that single observation but displaced ordinary mode-2 timing, so
it was rejected. The implemented behavior is narrow: after LCD enable on DMG,
LY advances at dot 452 of the startup line while the transition remains at dot
456. HBlank then avoids incrementing LY twice. LCD synchronization and
steady-state OAM timing therefore pass together without shifting CGB behavior.

### HALT Bug: Closed by Hardware-Visible IF Semantics

HALT with IME clear and an already-pending enabled interrupt suppresses the next
opcode-fetch PC increment. DeityGB now carries that condition beyond HALT and
reuses the following opcode byte as the instruction's first operand. A focused
`LD A,d8` test verifies that lifecycle. An interrupt waking a genuinely halted
CPU with IME enabled is still serviced before the next opcode, preserving the
Kirby startup fix.

The remaining Blargg failure was not the opcode-reuse path itself. The IF
register at `FF0F` has five real interrupt-request bits; the upper three bits
are unused and read back as one on the bus. DeityGB had been exposing the stored
byte verbatim, so `halt_bug.gb` saw stale high-bit behavior in its printed
matrix and checksum even though CPU interrupt arbitration masked to the real
low-five bits.

The boundary is now explicit. `MMU::get_byte(0xFF0F)` returns `stored | 0xE0`,
matching what emulated software reads. CPU interrupt arbitration already masks
to the real low-five interrupt-request bits, so the raw `get_if`/`set_if`
helpers keep their existing stored-byte behavior for interrupt timing and PPU
call sites. A focused unit test covers normal bus reads, raw `get_if`, and
direct `set_if`.

With that fix, the full `halt_bug.gb` matrix passes. The visible rows now show
the expected hardware-facing IF values such as `E1` and `F1`, while the CPU
still services only enabled low-five interrupt requests. This closes the last
known bundled Blargg CPU/timing holdout.

While rechecking the neighboring suites, `interrupt_time` initially appeared to
fail only because it had been launched with `--no-apu`. Its `get_cpu_speed`
helper intentionally measures CPU speed through APU behavior rather than KEY1,
so the ROM must run with APU enabled to validate both normal and double-speed
rows. With APU enabled it reports `00 08 0D` and `01 08 0D` and passes.

### Verification Matrix

- `cpu_instrs`: 11/11 passed.
- `instr_timing`: passed.
- `mem_timing`: 3/3 passed.
- `mem_timing-2`: 3/3 passed.
- `interrupt_time`: passed in normal and CGB double speed with APU enabled.
- `oam_bug`: 8/8 passed with `--dmg --no-boot`.
- `dmg_sound`: 12/12 passed.
- `cgb_sound`: 12/12 passed.
- `halt_bug`: passed.

The headless runner now reports CGB mode, KEY1, and double-speed state, accepts
`--dmg` for hardware-specific ROMs, and accepts `--trace` for targeted CPU
diagnosis. Full traces are kept outside the repository and filtered by address
before inspection; the temporary trace used for this investigation is not part
of version control.

## DMG and CGB Acid2 PPU Completion

The Acid2 ROMs are compact visual specifications for the line renderer. They do
not require a dot-accurate FIFO: both tests make their scanline-specific register
writes during mode 2 and intentionally accept a line-based implementation. This
makes them a useful boundary test for DeityGB's current PPU architecture.

### Establishing the Baseline Correctly

The first one- and two-second DMG captures showed only the Nintendo logo because
the bundled boot animation had not completed. Capturing at eight emulated seconds
allowed the ROM to finish drawing and exposed the real result. Nearly the entire
face already matched the official image: text, hair suppression, both eyes,
sprite priority, nose flips, mouth construction, and footer were correct. Only
the right edge of the chin was absent. The newly added official CGB test ROM
produced the same isolated omission in native color.

Using the test author's reference images made verification objective:

- CGB output is identical in all 160 x 144 RGB pixels.
- DMG output is identical in all 160 x 144 two-bit shade indices after mapping
  DeityGB's four green LCD colors to the reference image's four grayscale levels.
- The green DMG palette remains a presentation choice; changing it is not needed
  for PPU correctness because BGP still selects the same four hardware shades.

### What Acid2 Exercises

The completed images jointly cover the renderer's major composition rules:

- The `HELLO WORLD!` row selects only the first ten OAM entries intersecting a
  scanline, so the eleventh white object cannot hide the background exclamation.
- DMG background disable draws BGP color 0, hiding the mohawk background tiles.
- Background and window tile-data selection switches between unsigned `8000`
  addressing and signed tile indices centered at `9000`.
- CGB tile attributes select VRAM bank, horizontal/vertical flip, palette, and
  BG-to-OBJ priority independently for every background/window tile.
- DMG object priority resolves lower X first and then lower OAM index; CGB's
  `OPRI` mode resolves the intended OAM-order priority.
- Transparent object color 0 does not replace the composed background, while
  nonzero object pixels obey object-behind-background and CGB master-priority
  rules.
- Eight-by-sixteen objects ignore tile-index bit 0 and choose the second tile
  after applying vertical flip, which builds the curved mouth correctly.
- Background map selection and signed tile addressing change again for the
  footer without disturbing the face already rendered above it.

### The Window Counter Bug

The window has an internal Y counter separate from `LY` and `WY`. It increments
only on scanlines where the window is actually drawn. Hiding the window does not
rewind it; the next visible window line resumes where the earlier region stopped.

DeityGB reset `window_line_counter` whenever LCDC disabled the window. Acid2
draws the right eye with the window for 16 lines, hides it by moving `WX` off
screen, changes the window tile map, and later re-enables it for the right chin.
Resetting on the temporary hide made the chin read map row 0 instead of row 2,
so only that side disappeared. The correction is deliberately narrow:

- A disabled or off-screen window pauses the counter.
- A visible window scanline increments it once after drawing.
- LCD disable and the start of a new frame still reset it to zero.

This is shared hardware behavior, so the same correction completes both DMG and
CGB Acid2 instead of introducing mode-specific rendering paths.

### Visual Regression Contract

Two ignored release tests now boot each local Acid2 ROM for eight emulated
seconds and compare the complete RGBA framebuffer using a 64-bit FNV-1a hash.
The expected hashes were recorded only after direct pixel comparison against the
official reference images reported zero differences. Any future change to tile
addressing, object selection, priority, window visibility, palettes, or scanline
state will therefore fail a single deterministic test rather than relying on a
manual screenshot inspection.

Run them with:

```sh
nix develop --command cargo test --release --test headless \
  dmg_acid2_matches_reference_layout -- --ignored --exact
nix develop --command cargo test --release --test headless \
  cgb_acid2_matches_reference_image -- --ignored --exact
```

The implementation was derived from Pan Docs and the Acid2 authors' test guide,
source-level behavior descriptions, and official reference images. No other
emulator implementation was used or copied.

Interactive CGB gameplay with audio:

```sh
nix develop --command cargo run --release --bin DeityGB -- \
  src/roms/pokemon_silver.gbc --apu
```

For debugging an individual case, the headless binary prints both the
outcome and Blargg's escaped memory report:

```sh
nix develop --command cargo run --release --bin gb-headless -- \
  "src/roms/gb-test-roms/dmg_sound/rom_singles/09-wave read while on.gb" \
  --seconds 30
```

## Link's Awakening DX Startup and MBC5

Link's Awakening DX initially panicked shortly after startup with the CPU at
`PC=5C1B`, while diagnostics still reported switchable ROM bank 1. The cartridge
header identified the actual architectural mismatch immediately:

- title: `ZELDA`
- CGB capability byte: `80`
- cartridge type: `1B` (MBC5 + RAM + battery)
- ROM size: 1 MiB / 64 banks
- RAM size: 32 KiB / 4 banks

Before this change, DeityGB recognized only MBC1 and MBC3. Writes to MBC5's bank
registers therefore fell through the ROM-write catch-all and were ignored. The
game jumped to an address that was valid in its requested bank, but DeityGB
continued fetching bank 1; execution eventually encountered an unsupported
zero-cycle path and the headless safety assertion exposed the bad mapping. This
was a cartridge-controller failure, not a Link's Awakening-specific CPU bug.

### MBC5 Mapping Model

- Added independent MBC5 state for RAM enable, a nine-bit ROM bank, and a
  four-bit RAM bank. MBC5 switchable bank zero is valid, unlike MBC1/MBC3's
  common zero-to-one remapping rule.
- Writes at `0000-1FFF` enable external RAM when the low nibble is `0A`.
- Writes at `2000-2FFF` replace ROM-bank bits 0-7. Writes at `3000-3FFF` replace
  bit 8 without disturbing the low byte. This supports the controller's full
  512-bank address range even though Link's Awakening itself has 64 banks.
- Writes at `4000-5FFF` select external RAM bank bits 0-3. ROM reads from
  `0000-3FFF` remain fixed at bank 0; reads from `4000-7FFF` use the complete
  selected MBC5 bank.
- Cartridge RAM detection now includes MBC5 RAM and rumble+RAM type codes
  `1A`, `1B`, `1D`, and `1E`. Link's Awakening consequently allocates the four
  8 KiB banks declared by header RAM-size code `03` instead of reporting no RAM.
- `mapped_rom_bank` now returns a 16-bit value so diagnostics and future tests
  can represent MBC5 banks 256-511 without truncation.

### Evidence and Regression

- Added a synthetic 8 MiB MBC5 cartridge test. It selects bank 257 by writing
  the low byte and ninth bit separately, verifies data from that bank, and then
  proves external RAM bank 3 is isolated from bank 0.
- The exact pre-fix Link's Awakening command previously panicked after entering
  incorrectly mapped code. With MBC5 enabled it ran 45 emulated seconds, stayed
  in a normal HALT/VBlank loop, and rendered the complete native-color title.
- A deeper 75-second scripted run pressed through the title/file flow and
  reached Marin's opening dialogue in Link's house with coherent CGB graphics.
  That path is captured as an ignored framebuffer regression.
- The existing `.sav` file is not consumed by this work. Save persistence remains
  intentionally disabled; this test exercises newly allocated in-memory MBC5
  RAM and starts from the game's normal empty-file behavior.

## APU Timing Completion: 24/24 Blargg Sound ROMs

The initial APU checkpoint deliberately stopped at 7/12 DMG and 9/12 CGB. Its
channel generators were functional, but the remaining tests observed details
at a finer boundary than an audible gameplay check can reveal. This pass closes
that gap: every bundled `dmg_sound` and `cgb_sound` single ROM now reports
`Passed`, giving a 12/12 result on each hardware model and 24/24 overall.

### Why the Last Eight Tests Were Different

The earlier passing tests mostly observed register values and channel status
over relatively long intervals. Tests 07-10 and DMG test 12 deliberately place
CPU accesses within a few master clocks of hidden APU events. They therefore
exercise three clocks at once:

- The CPU performs a memory access during one machine cycle of an instruction.
- DIV-APU advances at 512 Hz from a falling divider bit and clocks length,
  sweep, and envelope units on selected sequencer phases.
- Channel 3 fetches a four-bit sample on its own frequency timer and temporarily
  owns one wave-RAM byte while doing so.

Treating a complete CPU instruction as an indivisible event loses the ordering
between those clocks. DeityGB still executes instructions atomically, but now
records enough timing context to project a wave-bus access to the instruction's
memory phase instead of using stale state from the previous instruction.

### DIV-APU Power and Length Semantics

The frame sequencer itself resets when the APU transitions from off to on, but
a divider edge observed while sound is powered off must not remain queued and
clock a newly powered sequencer immediately. The old boolean edge flag survived
the powered-off early return, producing a phantom length/sweep clock at power-up.
The APU now consumes such edges while off without clocking channel state.

This one ordering fix resolves both suites' test 07 and test 08:

- Test 07 verifies length and sweep periods, synchronization between them, and
  the next frame time after several power transitions separated by 8192 clocks.
- On DMG, powered-off writes to the four length fields remain legal and retained,
  while the counters do not run. The observed values are `33 44 11 22`.
- On CGB, powering the APU up resets the hidden length counters. The same test
  consequently observes `40 00 40 40`, demonstrating that the model difference
  is intentional rather than a shared approximation.

### Wave RAM as an Arbitrated Bus

Wave RAM is ordinary memory only while channel 3 is inactive. During playback,
channel 3 has priority over the CPU:

- CGB CPU reads and writes are redirected to the byte selected by the current
  wave sample position, regardless of the requested address.
- DMG performs the same redirection only during the narrow fetch window. Reads
  outside it return `FF`, and writes outside it are discarded.
- The fetch-valid state is separate from the timer value. Immediately after a
  trigger, a countdown can resemble the aftermath of a fetch even though no
  sample has actually been fetched yet. Remembering this distinction fixes the
  first boundary case in DMG test 09.

The MMU now receives the wave timer, period, sample position, and whether a real
fetch has occurred. The CPU publishes the current instruction's cycle count.
For an active wave-RAM access, the MMU projects the channel state to that bus
phase and decides which byte is visible and whether DMG permits the access. This
is a reusable timing model; it does not inspect ROM identity or test addresses.

### Trigger Latency and DMG Corruption

Channel 3 does not fetch immediately on trigger. Its frequency timer includes
the six-clock startup latency observed by the sound tests, and the previously
buffered sample remains the initial output. This aligns CGB's continuously
redirected wave reads with the hardware checksum.

DMG adds a destructive retrigger quirk when a trigger coincides with a wave
fetch. If the active byte is 0-3, its value is copied into wave byte 0. If it is
in bytes 4-15, the aligned four-byte group containing it is copied into bytes
0-3. Trigger arbitration is evaluated two APU clocks later than an ordinary
wave-RAM access, so it has its own observation offset while sharing the same
bus-state projection. This reproduces the corruption tested by ROM 10 without
changing CGB behavior.

### Regression Shape and Result

The ignored aggregate regression now lists all twelve ROMs from each suite,
instead of silently omitting known failures. Each ROM boots through the bundled
DMG boot ROM, runs for up to 30 emulated seconds, and must produce Blargg's
signed cartridge-RAM `Passed` result. The final matrix is:

- DMG sound: 12 passed, 0 failed, 0 omitted.
- CGB sound: 12 passed, 0 failed, 0 omitted.
- Combined: 24 passed, 0 failed, 0 omitted.

The implementation and diagnosis use Pan Docs, hardware behavior documented by
the test suite, and the bundled test sources. No implementation from another
emulator was used or copied.

Run the complete matrix with:

```sh
nix develop --command cargo test --release --test headless \
  blargg_sound_core_roms_pass -- --ignored --exact
```

## Optional CGB Boot ROM Plumbing

DeityGB historically loaded `src/dmg_boot.bin` for every cartridge, including
CGB titles. That file is 256 bytes and is read from disk at runtime through the
compiled-in repository path; it is not embedded with `include_bytes!`. This is
fine for DMG startup, but a real CGB boot ROM is 2304 bytes and has a second
mapped window at `0200-08FF`, so simply pointing the old loader at a CGB dump
would not have been enough.

The boot path selection now happens after the cartridge has been read, so the
header byte at `0143` can decide whether the ROM is CGB-capable. DMG cartridges
continue to use `src/dmg_boot.bin`. CGB cartridges use `src/cgb_boot.bin` when a
local dump exists, otherwise they fall back to the existing DMG compatibility
path and the CPU's A=`11` CGB handoff shim.

`src/cgb_boot.bin` is intentionally not added to git. It is a local user-provided
boot ROM dump, similar in legal shape to cartridge ROMs. The emulator support is
committed, but the binary dump remains outside the repository history.

The MMU boot ROM buffer now covers `0000-00FF` and the CGB-only `0200-08FF`
window while `FF50` is still zero. A write of any non-zero value to `FF50`
permanently unmaps both windows until reset, preserving the existing
`boot_rom_cannot_be_remapped_without_reset` behavior. `load_boot_rom` now fills
the boot buffer without taking ownership of cartridge `rom_data`, which keeps
boot storage and cartridge storage separate.

The macroquad frontend auto-selects the boot ROM after loading the cartridge
header. The headless runner does the same unless `--boot` supplies an explicit
path or `--no-boot` disables boot ROM loading for tests. Coverage includes
`cgb_boot_rom_maps_extended_boot_window`, which verifies that CGB boot bytes are
visible at `0000`, `0200`, and `08FF` before `FF50`, and that cartridge ROM
bytes are visible again after unmapping.

## Macroquad Frame Pacing Cleanup

The frontend was manually sleeping for roughly one Game Boy frame and then
calling `next_frame().await`. Macroquad already yields to the windowing backend
and presentation pacing there, so the extra `std::thread::sleep` could push
each frame a little late. On a 60 Hz host display that showed up as an observed
56-57 FPS in the overlay and, because APU samples are produced as emulated
cycles advance, slightly under-produced audio that sounded slow.

The manual host sleep has been removed. Emulation still advances according to
Game Boy CPU/peripheral cycles and still presents only when a VBlank frame is
ready or the fallback `CYCLES_PER_FRAME` budget has elapsed. The host-facing
pace is now owned by macroquad's frame boundary instead of a second coarse
sleep layered inside the render path. This is intentionally conservative: it
does not change PPU timing, APU timing, cartridge persistence, or the headless
runner.

The remaining timing follow-up, if frontend audio/video drift persists on some
machines, is a proper wall-clock cycle accumulator with audio-buffer feedback.
That would be a larger synchronization change. This checkpoint fixes the
obvious double-throttle first so Pokemon Crystal/Silver should run much closer
to the host's 60 Hz presentation rate.

## Mooneye Acceptance Baseline

Mooneye uses a compact pass/fail protocol rather than Blargg's text protocol.
A passing test places `03 05 08 0D 15 22` in `BCDEHL`; a failing test places
`42` in all six registers. The ROMs then report through serial. DeityGB's
current serial behavior causes many Mooneye ROMs to take their fast
serial-broken path, so the observable serial log may contain only the final
byte written to `SB` rather than all six bytes. The headless runner now treats
the register tuple as authoritative once serial reporting has started.

The command-line diagnostics now include `BC` alongside `DE` and `HL`, making
Mooneye reports readable without enabling a full CPU trace. The ignored DAA
smoke test has been replaced by two acceptance regressions: one explicit list
of currently passing ROMs, and one full-tree classifier that asserts the suite
does not produce ambiguous timeouts.

Current release baseline for
`src/roms/mts-20240926-1737-443f6e1/acceptance`, using a 10-second emulated
budget and APU disabled:

- Passed: 53.
- Failed: 22.
- Timeout: 0.

Passing families now include `instr/daa`, `if_ie_registers`, DI/EI sequencing,
all bundled HALT acceptance ROMs, `intr_timing`, `reti_intr_timing`, the
`interrupts/ie_push` interrupt-dispatch edge case, JP/CALL/RET/RST/PUSH/POP
timing, `add_sp_e_timing`, `ld_hl_sp_e_timing`, DMG-family unused HWIO bus
masks, all current OAM DMA acceptance coverage, DIV/TAC falling-edge timer
behavior, TIMA reload/write windows, and the basic `tim00`, `tim01`, `tim10`,
and `tim11` timer rates.

The unused-HWIO pass comes from modeling normal DMG-family bus reads for
otherwise-unused register bits and addresses. `SC` now reports bits 1-6 high,
`TAC` reports bits 3-7 high, `STAT` reports bit 7 high, the usual unused APU
register gaps still read as `FF`, and the DMG-family `$FF4C-$FF7F` range reads
as unmapped even though DeityGB internally keeps a boot-disable sentinel for
`FF50`.

The OAM DMA implementation is now timed instead of immediate. A write to `DMA`
creates the hardware-visible startup delay, blocks CPU OAM reads and writes
while transfer is active, copies one byte per machine cycle, preserves the old
transfer during restart delay, and handles source pages through the same memory
ownership boundaries as ordinary reads. The CPU fetch path also exposes the
initial OAM opcode grace needed by Mooneye's DMA-start probes.

Instruction timing fixes made operand and stack accesses happen at the cycles
the acceptance ROMs observe rather than as lumped instruction effects.
JP/CALL conditional forms fetch their 16-bit target before deciding whether the
branch is taken, CALL and RST push high then low return bytes at their observed
cycles, and RET/POP read low then high with the taken conditional RET offset.

Timer work taught DIV and TAC writes to detect selected-bit falling edges, and
taught TIMA/TMA writes about the reload-cycle windows. That is why the
DIV-trigger and reload-write Mooneye timer cases are now part of the protected
pass set.

DI disables IME immediately and cancels pending EI, while EI still takes effect
only after the following instruction. Interrupt service now models the observed
`ie_push` behavior: the high PC byte is pushed first, IE/IF are sampled again,
and if that write removed the last pending interrupt the CPU vectors to
`0000` without clearing IF or pushing the low byte.

The remaining failures are real emulator accuracy gaps, not harness ambiguity.
They cluster around boot-revision register/DIV/HWIO expectations, PPU
STAT/LY/LCD-enable timing, and serial boot clock alignment. The boot-revision
group is partly a harness/model-selection issue: a single fixed startup profile
cannot truthfully satisfy every DMG0, DMG-ABC, MGB, SGB, and SGB2 expectation at
the same time. The PPU group is the next large accuracy frontier after the
CPU/timer/DMA/HWIO acceptance gains.

### Mooneye 66/75 Follow-Up

The headless runner now has an explicit `--model` post-boot path for
DMG0, DMG-ABC, MGB, SGB, and SGB2 acceptance ROMs. These profiles bypass the
bundled DMG boot ROM and install the hardware-family CPU registers plus the
documented post-boot IO state that Mooneye's model-specific ROMs assert. This
is deliberately scoped to the headless harness: the frontend still uses the
normal boot-ROM flow unless the user chooses otherwise, and the profiles do not
try to masquerade as cartridge save state or global emulator defaults.

That model split moved the acceptance baseline from 53/75 to 63/75 by making
the boot register, boot DIV, SGB boot DIV2, and DMG-family boot HWIO cases run
under the hardware family they were written to observe. The first additional
PPU/MMU timing pass raised the protected baseline to 64/75 with
`ppu/stat_irq_blocking.gb`: STAT interrupts are no longer requested directly on
every enabled mode transition. Instead, the PPU keeps the hardware-style ORed
STAT IRQ line and requests IF bit 1 only on a low-to-high transition. CPU writes
to `STAT` and `LYC` also preserve PPU-owned low bits, update coincidence state,
and request an interrupt only when a newly enabled source is already active.

The protected baseline is now 66/75. One new pass is `boot_hwio-dmg0.gb`.
DMG0's skipped-boot profile now seeds the internal PPU phase as late VBlank
(`LY=145`, mode 1, dot 100) while preserving the visible post-boot IO register
profile. That lets the test ROM's hardware-IO scan naturally wrap into the
expected `STAT`/`LY` values without running a copyrighted DMG0 boot ROM. This
seed is deliberately DMG0-only; the DMG-ABC/MGB/SGB/SGB2 boot HWIO profiles keep
the previous cold-PPU behavior because that already matches their acceptance
ROMs.

The second new pass is `serial/boot_sclk_align-dmgABCmgb.gb`. Internal-clock
serial transfers now complete one CPU tick after the scheduled final falling
edge. That keeps the transfer byte visible for the DMG-ABC boot-clock alignment
test's sampling point and updates the focused serial unit test to assert the
new boundary.

### Mooneye 67/75 STAT/LYC Follow-Up

The protected baseline is now 67/75. The new pass is
`ppu/stat_lyc_onoff.gb`, which probes a very narrow LCD-off corner rather than
ordinary rendering. While the LCD controller is disabled, DeityGB now preserves
the existing STAT coincidence bit and does not recompute it on `LYC` writes.
That models the test's assumption that the LY=LYC comparison clock is stopped
while LCD is off. When LCD is enabled again, the PPU restarts from mode 0 for
the immediate observable STAT read, recomputes coincidence once, and then lets
the normal STAT IRQ-line edge detector decide whether IF bit 1 should rise.

The ownership boundary matters here: the MMU still accepts writes to `LYC`, but
it only updates PPU-owned coincidence/interrupt state while LCD is on. The PPU
keeps responsibility for LCD enable/disable transitions, STAT mode bits, and
the latched STAT IRQ line. That split avoids a stale-LYC shortcut in the MMU
while keeping the visible LCD-off behavior stable for ROMs that read STAT
immediately around the enable edge.

The important lesson from this pass is that STAT has both stored bits and
live PPU-derived bits. It is tempting to treat a write to `LYC` as an ordinary
register write followed by an immediate `LY == LYC` refresh, but Mooneye's ROM
intentionally disables LCD, writes values that would otherwise toggle
coincidence, and then checks that the old coincidence state survived until the
LCD logic was restarted. DeityGB now lets the CPU-visible `LYC` byte change
while holding the LCD-off comparison output steady. That is why the fix lives in
both places: `MMU::set_byte` handles the CPU write without pretending to run the
stopped comparator, and `PPU::cycle` performs the restart-time comparison when
LCD transitions back on.

The STAT IRQ line is similarly level-like internally but edge-triggered at the
interrupt flag. The emulator already had an ORed STAT IRQ-line model from the
66/75 work; this change avoids clearing that internal latch as a side effect of
LCD disable, because the test cares about whether a later LCD enable produces a
fresh low-to-high transition. In plain terms: changing visible STAT mode bits is
not the same thing as pulsing IF. The PPU now updates the visible mode/coincidence
state first, then asks the edge detector whether the combined STAT source line
actually rose.

This does not solve the remaining LCD-enable timing ROMs. Those tests observe
more precise dot-level relationships between LY, mode bits, access windows, and
interrupt sampling. This change is deliberately narrower: it fixes one documented
LCD-off `LYC` behavior without installing a broad speculative PPU scheduler that
could move already-passing tests. The full-tree classifier and known-pass guard
were both run after the change so the new pass is protected and the previous 66
passes stayed green.

### Mooneye 70/75 CPU-Visible PPU Boundary Follow-Up

The protected baseline is now 70/75. Three more PPU timing ROMs pass:
`ppu/intr_2_mode0_timing.gb`, `ppu/intr_2_mode3_timing.gb`, and
`ppu/intr_2_oam_ok_timing.gb`. These tests all start from a mode 2 STAT
interrupt and then ask what the CPU can observe through STAT reads or OAM reads
near the mode 2->3 and mode 3->0 boundaries.

The useful distinction is internal PPU phase versus CPU-visible bus behavior.
DeityGB still keeps the internal mode transitions at the old coarse points so
STAT interrupt timing such as `ppu/intr_2_0_timing.gb` remains stable. The MMU
now has a small `observable_ppu_mode` helper for CPU reads and writes that uses
the tracked PPU phase counters to expose the access-window boundary one machine
cycle earlier: mode 2 reads as mode 3 at dot 76, and mode 3 reads as mode 0 at
dot 248. OAM and VRAM access checks use that same observable mode, so the CPU can
see OAM become readable at the point Mooneye expects without moving the internal
STAT mode-0 interrupt edge.

This split is deliberately one-way. PPU-owned interrupt logic still uses the raw
STAT register bits, not the CPU-observable mode override, otherwise enabling the
mode-0 STAT source near the early observable HBlank boundary would request IF too
early and regress `ppu/intr_2_0_timing.gb`. LCD-off handling also forces the
observable mode to 0 so the earlier `ppu/stat_lyc_onoff.gb` fix keeps its
hardware-style LCD-disabled STAT reads. In other words: the CPU bus can expose an
early access boundary, but the PPU's own STAT IRQ source remains tied to the
internal phase transition until DeityGB grows a finer event scheduler.

The remaining 5 failures are:

- `ppu/hblank_ly_scx_timing-GS.gb`
- `ppu/intr_2_mode0_timing_sprites.gb`
- `ppu/lcdon_timing-GS.gb`
- `ppu/lcdon_write_timing-GS.gb`
- `ppu/vblank_stat_intr-GS.gb`

The failed PPU ROMs are not generic rendering problems; they probe exact
scanline-cycle ownership of LY, STAT mode bits, LYC coincidence, LCD enable
startup, OAM/VRAM access windows, and interrupt sampling. The next meaningful
step toward 75/75 is a cycle-stepped PPU/CPU boundary that can expose those
state changes at the same sub-instruction points the CPU observes. A naive
per-cycle PPU loop was tried and rejected because it slowed the suite
substantially without increasing the pass count; the next version needs to
advance only to scheduled PPU edges and timed memory accesses.

## Frontend TAB Fast-Forward

The macroquad frontend now treats a held TAB key as a deliberate user
fast-forward control. This is not the Game Boy Color hardware double-speed mode
driven by `KEY1` and `STOP`; it is a host-side playback affordance. The emulator
still advances the CPU, PPU, APU, timers, RTC, DMA, cartridge state, and save
debounce through the same cycle paths as normal play. The frontend simply runs
two emulated frames before yielding to macroquad's next presented host frame.

The first attempt only skipped every other presentation. That made the visual
path advance faster in the Codex worktree build, but it was easy to miss because
the original checkout's release binary had not been rebuilt and the overlay was
still counting presented frames rather than emulated frames. The final version
keeps a separate emulated-frame counter, presents every second emulated frame
while TAB is held, and shows `FAST 2x` in the overlay so the active path is
visible while testing.

Audio needed a separate fix. The APU correctly produced samples for every
emulated cycle, so TAB fast-forward created roughly twice as many samples per
wall-clock second. The old frontend audio path used an unbounded `mpsc` queue,
which meant the host audio callback kept playing old queued samples at normal
device speed. The user-visible result was gameplay moving at 2x while audio
lagged farther and farther behind.

The live frontend now uses a small bounded `sync_channel` and constructs the APU
with a bounded audio sink. That sink uses `try_send`, so samples that cannot be
accepted immediately are dropped instead of becoming stale buffered audio.
Headless tests and existing APU callers keep the unbounded sender constructor,
preserving their previous behavior. This policy favors synchronization during
fast-forward over audio fidelity: TAB playback may sound choppier or skip
samples, but it should stay temporally aligned with the accelerated game rather
than trailing seconds behind.

The release binary was rebuilt in the normal checkout with:

```sh
nix develop --command cargo build --release
```

Focused regression coverage was run with:

```sh
nix develop --command cargo test --release tab_fast_forward_presents_every_second_emulated_frame
```
