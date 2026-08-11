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
CGB double-speed operation.

The DMG-only OAM corruption suite must be run with `--dmg`, because its ROM
header also permits CGB hardware and CGB hardware does not exhibit the bug:

```sh
nix develop --command cargo run --release --bin gb-headless -- \
  src/roms/gb-test-roms/oam_bug/oam_bug.gb --seconds 120 --no-apu --dmg
```

All eight OAM groups pass. The standalone `halt_bug.gb` remains the only known
failing bundled Blargg ROM. Basic opcode-byte reuse after HALT is covered by a
unit test, but the complete pending-interrupt matrix remains follow-up work.

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

The current sound baseline is 12/12 DMG and 12/12 CGB. The aggregate runs every
single ROM in both bundled sound suites, including frame-sequencer power timing,
DMG/CGB length persistence, active wave-RAM arbitration, and DMG wave-retrigger
corruption. A failure prints the suite, ROM name, and Blargg memory diagnostic.

Headless diagnostics accept `--trace` to enable the CPU instruction/state trace
and print CGB mode, KEY1, and double-speed state in the final report. Use this
selectively because instruction traces become large quickly.

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
