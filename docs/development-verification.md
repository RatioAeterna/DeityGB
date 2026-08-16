# Development Verification

This project keeps the macroquad frontend and also exposes a headless runner for
fast emulator checks.

## Quick Checks

```sh
nix develop --command cargo test
```

Run the macroquad frontend from the repository root with:

```sh
nix develop --command cargo run --release --bin DeityGB -- src/roms/pokemon_red.gb
nix develop --command cargo run --release --bin DeityGB -- src/roms/pokemon_silver.gbc
nix develop --command cargo run --release --bin DeityGB -- src/roms/links_awakening.gbc
```

Append `--apu` to enable host audio:

```sh
nix develop --command cargo run --release --bin DeityGB -- src/roms/pokemon_silver.gbc --apu
```

Controls are WASD for the D-pad, J/K for A/B, Enter for Start, and Left Shift
for Select.

Battery-backed cartridge saves are loaded once after the selected ROM has been
initialized. The save path is deterministic and adjacent to the ROM, with the
same stem and a `.sav` extension; for example `src/roms/pokemon_silver.sav`.
MBC3 cartridges with an RTC also use an adjacent `.rtc` sidecar so the raw
`.sav` remains ordinary cartridge RAM. The frontend and headless runner print
the selected paths at startup when the header declares a battery.

The `.sav` file is exactly the cartridge RAM size declared by header byte
`0149`. Missing files mean a clean first boot and are not created until the game
writes cartridge RAM or RTC state. Truncated saves are loaded into the beginning
of RAM with the rest left empty; oversized saves load only the declared RAM
range and are rewritten to the declared size after the next dirty flush.
Non-battery cartridges never write `.sav` files.

Flushes are deliberate and atomic: DeityGB writes a temporary file beside the
target and renames it into place. The macroquad frontend flushes dirty cartridge
persistence on shutdown and at a one-second debounce while playing; the
headless runner flushes once before exit. Dirty tracking lives in the MMU at
the cartridge RAM and RTC write boundary, so ordinary frames do not cause disk
writes merely because a game is running.

The host window is presented at least once per DMG frame even while the game
disables the LCD. This keeps macroquad input polling alive during scene and
battle transitions that do not produce an emulated VBlank.

## Headless ROM Runs

```sh
nix develop --command cargo run --bin gb-headless -- src/roms/gb-test-roms/cpu_instrs/individual/01-special.gb --seconds 30
nix develop --command cargo run --release --bin gb-headless -- src/roms/mts-20240926-1737-443f6e1/acceptance/instr/daa.gb --seconds 10 --no-apu
```

The headless runner reports `Passed`, `Failed`, or `Timeout` by watching serial
output and Blargg's documented cartridge-RAM protocol at `A000-A004`. The
runner prints the memory status and escaped diagnostic text when that protocol
is present. Mooneye ROMs use their documented serial/register protocol: passing
tests report `3, 5, 8, 13, 21, 34` in `BCDEHL`; failing tests report `0x42`
in all six registers. The ROMs may use their fast serial-broken path and leave
only the final byte visible in `SB`, so the headless runner recognizes the
register tuple after serial reporting starts.

## Visual Snapshots

```sh
nix develop --command cargo run --release --bin gb-headless -- src/roms/dmg-acid2.gb --seconds 8 --no-apu --dump-frame /tmp/dmg-acid2.ppm
nix develop --command cargo run --release --bin gb-headless -- src/roms/cgb-acid2.gbc --seconds 8 --no-apu --dump-frame /tmp/cgb-acid2.ppm
```

The snapshot is a raw PPM image at the original DMG resolution, 160x144.
Both Acid2 images have exact framebuffer regressions:

```sh
nix develop --command cargo test --release --test headless dmg_acid2_matches_reference_layout -- --ignored --exact
nix develop --command cargo test --release --test headless cgb_acid2_matches_reference_image -- --ignored --exact
```

The CGB output matches the official RGB reference pixel-for-pixel. The DMG
output matches every official shade index; DeityGB deliberately presents those
four indices through its green LCD palette instead of grayscale.

For game debugging where sound behavior is not under test, add `--no-apu`.
Script a Start-button pulse with `--press-start-at N`, where `N` is an
emulated second:

```sh
nix develop --command cargo run --release --bin gb-headless -- src/roms/pokemon_red.gb --seconds 42 --press start@35 --no-apu --dump-frame /tmp/pokemon.ppm
```

`--press BUTTON@SECOND` may be repeated to script longer flows. Append `/FRAMES`
for a short tap, such as `--press left@180/4`. The bundled
Pokemon Red ROM currently reaches the New Game menu and proceeds through the
intro into the player's bedroom with scripted input. Its cartridge uses the
implemented MBC3 ROM/RAM banking and RTC registers.

Pokemon Silver now boots as a CGB cartridge and reaches its native-color title
screen. Its ignored visual regression checks both multiple RGB colors and an
exact framebuffer hash:

```sh
nix develop --command cargo test --release --test headless pokemon_silver_reaches_color_title_screen -- --ignored --exact
```

Implemented CGB hardware includes VRAM bank 1 and `VBK`, WRAM banks and `SVBK`,
RGB555 background/object palette RAM, tile and object attributes, CGB priority
rules and `OPRI`, general/HBlank VRAM DMA, `KEY1`/`STOP` double-speed switching,
and native RGBA output in both frontends. The bundled DMG boot animation is
retained; after it unmaps, dual-mode cartridges receive the documented CGB
handoff value in register A.

Kirby's Dream Land has a deterministic title-to-gameplay regression covering
its HALT/VBlank-driven Stage 1 transition:

```sh
nix develop --command cargo test --release --test headless kirby_enters_green_greens_after_stage_intro -- --ignored --exact
```

Link's Awakening DX uses MBC5 and has a deterministic regression from its CGB
title screen through file selection into Marin's opening house dialogue:

```sh
nix develop --command cargo test --release --test headless links_awakening_dx_reaches_opening_dialogue -- --ignored --exact
```

The bundled Blargg `cpu_instrs` ROM passes all 11 groups. The `instr_timing`,
both `mem_timing` generations, and cycle-exact `interrupt_time` ROMs also pass.
`interrupt_time` validates the documented interrupt entry in both normal and
CGB double-speed operation. Run `interrupt_time` with APU enabled; its bundled
CPU-speed probe observes APU timing, so `--no-apu` produces a false speed-column
failure even when interrupt latency is correct.

The DMG-only OAM corruption suite must be run with `--dmg`, because its ROM
header also permits CGB hardware and CGB hardware does not exhibit the bug:

```sh
nix develop --command cargo run --release --bin gb-headless -- \
  src/roms/gb-test-roms/oam_bug/oam_bug.gb --seconds 120 --no-apu --dmg --no-boot
```

All eight OAM groups pass. The standalone `halt_bug.gb` now passes as well. Its
final fix was the `FF0F` bus contract: CPU interrupt arbitration masks to the
five real request bits, while normal software reads force the unused upper three
bits high.

## ROM-Suite Regression Tests

The heavier bundled ROM checks are available as ignored tests:

```sh
nix develop --command cargo test --test headless -- --ignored --test-threads=1
```

They are ignored by default because the current emulator may expose accuracy
gaps while CPU, timer, PPU, and APU work is still in progress.

Mooneye acceptance coverage has two focused ignored regressions:

```sh
nix develop --command cargo test --release --test headless \
  mooneye_acceptance_known_passes_remain_green -- --ignored --exact
nix develop --command cargo test --release --test headless \
  mooneye_acceptance_suite_reports_no_timeouts -- --ignored --exact
```

The current baseline over `src/roms/mts-20240926-1737-443f6e1/acceptance` is
75 passed, 0 failed, 0 timed out with a 10-second budget and APU disabled.
Passing coverage includes DAA, IF/IE, DI/EI sequencing, HALT timing, interrupt
dispatch edge cases, JP/CALL/RET/RST/PUSH/POP timing, ADD SP/e and
LD HL/SP+e timing, DMG-family unused HWIO bus masks, model-specific
DMG0/DMG-ABC/MGB/SGB/SGB2 post-boot register, DIV, and HWIO profiles, OAM DMA
start/restart/source/read behavior, DIV/TAC falling edges, TIMA reload/write
windows, STAT IRQ blocking, CPU-visible PPU mode/access boundary timing,
SCX- and sprite-dependent mode-3 duration, LCD-enable startup, LY/coincidence
phasing, the DMG line-144 mode-2 STAT pulse, the final-VBlank mode-2 pulse,
LCD-off LYC coincidence behavior, DMG-ABC serial boot-clock alignment, and all
basic TIMA rates.

The PPU acceptance result depends on an ownership rule that should be preserved
when adding instructions. CPU instructions advance the PPU to the machine cycle
where a video-bus read or write occurs, perform that access, then advance the
remaining instruction cycles. The frontend and headless runner therefore call
`CPU::cycle_with_ppu`; they must not also advance the PPU after the instruction.
Ordinary memory retains its established timing, and active OAM DMA retains
ownership of OAM timing. Video-register, VRAM, and non-DMA OAM accesses use the
scoped timed path. Reads and writes intentionally do not share one blanket
boundary: the hardware-visible VRAM lock begins at mode-2 dot 76, OAM writes
have a dot-76 aperture, and selected `LD A,(HL)` observations sample at their
actual end-of-access phase.

The scanline model also keeps signals separate instead of deriving every effect
from the two visible STAT mode bits. Normal LY advances at dot 444; the first two
DMG LCD-startup lines advance at dot 452; coincidence is refreshed at the dot-456
line boundary. Mode-0 STAT can raise the shared STAT line four dots before the
visible transfer-to-HBlank transition. Transient mode-2 sources exist at DMG
LY 144 and at the end of LY 153, but still pass through the shared rising-edge
detector, so another active STAT source blocks a duplicate request. Mode 3 starts
from the SCX low-bit cost and adds the first ten on-line sprite fetch stalls,
including X-position alignment and the off-right-edge cutoff. These distinctions
are covered by focused unit tests plus the two full-suite ignored guards above.

Run the passing DMG/CGB APU core set directly with:

```sh
nix develop --command cargo test --release --test headless blargg_sound_core_roms_pass -- --ignored --exact
```

The current sound baseline is 12/12 DMG and 12/12 CGB. The aggregate runs every
single ROM in both bundled sound suites, including frame-sequencer power timing,
DMG/CGB length persistence, active wave-RAM arbitration, and DMG wave-retrigger
corruption. A failure prints the suite, ROM name, and Blargg memory diagnostic.

The default library tests also include focused envelope-period coverage. Run it
alone while changing envelope behavior with:

```sh
nix develop --command cargo test --release apu::tests:: -- --nocapture
```

In particular, a programmed envelope period of zero must reload its hidden
timer as 8 without performing automatic volume changes. This is not equivalent
to an ordinary programmed period of 8. Oracle of Seasons uses noise `NR42 = 08`
to keep the DAC enabled at volume zero; the historical bug slowly raised that
channel to volume 15 and produced persistent overworld noise.

The headless runner's final report includes raw APU registers plus CGB `PCM12`
and `PCM34`. For an audible game-specific investigation, reproduce the same
ROM/save/input checkpoint before and after a change and compare those values.
The Oracle checkpoint that found this issue retained `NR41-NR44 = [00,08,07,00]`
and `NR52 = 8F`, while the fix changed channel 4's `PCM34` nibble from `F` to
`0`. That is a stronger diagnostic than comparing recordings made through two
different host audio stacks: it proves the unwanted waveform existed inside the
emulated channel before host mixing.

Headless diagnostics accept `--trace` to enable the CPU instruction/state trace
and print CGB mode, KEY1, and double-speed state in the final report. Use this
selectively because instruction traces become large quickly.

Focused cartridge persistence coverage is part of the default `cargo test`
suite. It covers clean first boot without file creation, MBC3 SRAM bank
round-trip, exact-size `.sav` behavior for normal/truncated/oversized files,
non-battery cartridges staying disk-silent, atomic dirty flush behavior, and
MBC3 RTC sidecar restoration including halted-clock handling.

## Desktop Frontend and Packaging

The GUI can be built and exercised without supplying a ROM on the command line:

```sh
nix develop --command cargo build --release --bin DeityGB
./target/release/DeityGB
```

Press Enter on the splash, select a ROM directory in the folder browser, and
verify that the in-app library finds `.gb` and `.gbc` files recursively. Check
W/S, arrow keys, and Tab for single-row navigation, then hold each control to
verify delayed repeat and accelerated scrolling; A/D and Left/Right for ten-row
navigation; J or Enter to launch; and K or Escape to return to the directory
chooser. Also launch once with a ROM path to retain the developer and automation
path:

```sh
./target/release/DeityGB path/to/game.gb
```

Verify normal-speed gameplay on both a 60 Hz display and a high-refresh display
(120/144 Hz where available). The emulated-rate overlay should remain near the
Game Boy's 59.7275 frames per second regardless of host refresh. Holding Tab
should be the only normal frontend path to approximately 2x emulation; releasing
it must immediately return music, animation, and input timing to normal speed.

For an automatable packaging smoke test that does not need a synthetic Enter
keypress, `./target/release/DeityGB --choose-rom` skips the splash and opens the
same asynchronous chooser immediately. Linux uses a built-in directory-only
browser: select `[ Use this folder ]` to scan the displayed directory, J or
Enter to open a highlighted subdirectory, and K or Backspace to move upward.
No `zenity`, `kdialog`, portal, or other desktop-dialog helper is required.

While the chooser is open, confirm that the process remains responsive and its
memory use stays bounded. On Linux, enter several directories and confirm that
each directory-only listing appears without file-manager thumbnail delays.
Select a large directory and confirm that a
scanning message continues to render. Discovery must skip symbolic links,
especially links that point to an ancestor directory; the unit test constructs
that cycle on Unix systems so a regression cannot recurse indefinitely.
On macOS, also inspect `~/Library/Logs/DiagnosticReports` after accepting and
cancelling the chooser: neither path may create a `DeityGB-*.ips` report. The
chooser intentionally runs in the system `osascript` helper rather than placing
an rfd AppKit modal panel inside miniquad's event loop.

On Linux, repeat the same responsiveness check and verify that the waiting
message continues to be presented for as long as the chooser is open. The
frontend must poll a bounded channel and call `next_frame()`; it must never
await a portal future directly on the render coroutine. Test accept, cancel,
and repeated reopen with the available desktop helper. If no helper is
installed, DeityGB should print a diagnostic and return to the splash.

After launching a ROM, judge pacing with gameplay and audio rather than the FPS
label alone. With LCD enabled, only a real PPU VBlank may cause a host
presentation; the 70,224-cycle fallback is reserved for LCD-off responsiveness.
The focused `enabled_lcd_does_not_present_a_fallback_frame_before_vblank` test
guards against counting two host presentations for one hardware frame.

On Linux, test once with a working default audio device and once with no usable
default ALSA device. The first path should negotiate either floating-point or
integer samples and play normally. The second must print an `audio:` diagnostic
and continue emulation silently rather than aborting when the ROM is launched.

ROM discovery and selection wrapping have ordinary Rust unit tests. The
packaged GUI embeds the DMG and CGB boot assets with `include_bytes!`, so verify
both a DMG and a CGB cartridge from outside the repository. Cartridge `.sav`
and RTC sidecar locations still derive from the selected ROM path; packaging
must not redirect them into the read-only application bundle.

Create a macOS artifact on a Mac with:

```sh
./scripts/package-macos.sh
codesign --verify --deep --strict --verbose=2 target/release/bundle/osx/DeityGB.app
open target/release/bundle/osx/DeityGB.app
```

The script uses `cargo-bundle`, applies an ad-hoc signature, and creates
`dist/DeityGB-macOS.zip`. Ad-hoc signing proves bundle integrity on the machine
that built it; it is not Apple Developer ID signing or notarization. A public
release should inject those credentials in release infrastructure and then
validate the downloaded artifact on a clean Mac.

Create the x86-64 Linux artifact on Linux with:

```sh
./scripts/package-linux.sh
tar -tzf dist/DeityGB-linux-x86_64.tar.gz
```

Test that archive on a clean supported distribution, including the desktop
launcher, icon, native directory helper, audio, DMG/CGB startup, and a real
save/reload. Also smoke-test a direct ROM argument; this isolates the emulator
and graphics loop from chooser integration. The GitHub Actions workflow builds
macOS and Ubuntu artifacts on version tags and by manual dispatch. A local
macOS build cannot substitute for executing the Linux artifact on Linux.

For the Linux icon, verify all three surfaces rather than merely checking that
the PNG exists in the archive:

- launch `DeityGB` and confirm its running window/task-switcher icon uses the
  DeityGB artwork;
- inspect the window's X11 properties and confirm `_NET_WM_ICON` is populated
  and `WM_CLASS` contains `DeityGB`;
- install/open the included desktop entry and confirm the application-menu icon
  and running window are grouped together rather than appearing as two apps.

Upstream miniquad 0.4.6 does not apply `Conf::icon` on Linux. DeityGB's vendored
patch must set `_NET_WM_ICON` and `WM_CLASS` between X11 window creation and
mapping. Use `xprop` immediately after the first frame appears; the properties
must already be present, not appear later. Direct launch must create only one
process and must not write desktop metadata into the user's XDG directories or
invoke `gio`/`gtk-launch`.

## References

- Pan Docs: memory map, boot ROM handoff, and serial `FF01`/`FF02` behavior.
  <https://gbdev.io/pandocs/>
- Pan Docs cartridge header and MBC3 RTC details.
  <https://gbdev.io/pandocs/The_Cartridge_Header.html>
  <https://gbdev.io/pandocs/MBC3.html>
- Pan Docs audio registers and timing details.
  <https://gbdev.io/pandocs/Audio_Registers.html>
  <https://gbdev.io/pandocs/Audio_details.html>
- Mooneye Test Suite README and pass/fail protocol.
  <https://github.com/Gekkio/mooneye-test-suite>
- Bundled Blargg README:
  `src/roms/gb-test-roms/readme.txt`
