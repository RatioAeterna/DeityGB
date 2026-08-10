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
output. Blargg ROMs commonly print ASCII status text such as `Passed` or
`Failed`. Mooneye ROMs use their documented serial/register protocol: passing
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
implemented MBC3 ROM/RAM banking subset; MBC3 RTC registers are not implemented.

## ROM-Suite Regression Tests

The heavier bundled ROM checks are available as ignored tests:

```sh
nix develop --command cargo test --test headless -- --ignored --test-threads=1
```

They are ignored by default because the current emulator may expose accuracy
gaps while CPU, timer, PPU, and APU work is still in progress.

## References

- Pan Docs: memory map, boot ROM handoff, and serial `FF01`/`FF02` behavior.
  <https://gbdev.io/pandocs/>
- Mooneye Test Suite README and pass/fail protocol.
  <https://github.com/Gekkio/mooneye-test-suite>
- Bundled Blargg README:
  `src/roms/gb-test-roms/readme.txt`
