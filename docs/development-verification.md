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

The host window is presented at least once per DMG frame even while the game
disables the LCD. This keeps macroquad input polling alive during scene and
battle transitions that do not produce an emulated VBlank.

## Headless ROM Runs

```sh
nix develop --command cargo run --bin gb-headless -- src/roms/gb-test-roms/cpu_instrs/individual/01-special.gb --seconds 30
nix develop --command cargo run --bin gb-headless -- src/roms/mts-20240926-1737-443f6e1/acceptance/instr/daa.gb --seconds 30
```

The headless runner reports `Passed`, `Failed`, or `Timeout` by watching serial
output and Blargg's documented cartridge-RAM protocol at `A000-A004`. The
runner prints the memory status and escaped diagnostic text when that protocol
is present. Mooneye ROMs use their documented serial/register protocol: passing
tests send `3, 5, 8, 13, 21, 34`; failing tests send `0x42` six times.

## Visual Snapshots

```sh
nix develop --command cargo run --bin gb-headless -- src/roms/dmg-acid2.gb --seconds 1 --dump-frame /tmp/deitygb-frame.ppm
```

The snapshot is a raw PPM image at the original DMG resolution, 160x144.

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

The bundled Blargg `cpu_instrs` ROM passes all 11 groups. The cycle-exact
`interrupt_time` ROM still reports failure and tracks remaining timer/bus
interrupt precision work.

## ROM-Suite Regression Tests

The heavier bundled ROM checks are available as ignored tests:

```sh
nix develop --command cargo test --test headless -- --ignored --test-threads=1
```

They are ignored by default because the current emulator may expose accuracy
gaps while CPU, timer, PPU, and APU work is still in progress.

Run the passing DMG/CGB APU core set directly with:

```sh
nix develop --command cargo test --release --test headless blargg_sound_core_roms_pass -- --ignored --exact
```

The current sound baseline is 7/12 DMG and 9/12 CGB. Both suites pass register
behavior, length counters, fifth-register trigger timing, sweep, sweep details,
sweep overflow, and post-power register behavior. CGB additionally passes wave
retrigger and the dedicated wave timer/phase/access test. Remaining failures
cover frame-sequencer phase across APU power, length persistence across power,
and cycle-window behavior for active wave RAM; these are narrower timing gaps,
not missing pulse, wave, or noise synthesis.

## References

- Pan Docs: memory map, boot ROM handoff, and serial `FF01`/`FF02` behavior.
  <https://gbdev.io/pandocs/>
- Pan Docs audio registers and timing details.
  <https://gbdev.io/pandocs/Audio_Registers.html>
  <https://gbdev.io/pandocs/Audio_details.html>
- Mooneye Test Suite README and pass/fail protocol.
  <https://github.com/Gekkio/mooneye-test-suite>
- Bundled Blargg README:
  `src/roms/gb-test-roms/readme.txt`
