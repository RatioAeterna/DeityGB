# DeityGB

<p align="center">
  <img src="assets/deitygb-icon-512.png" alt="DeityGB icon" width="220">
</p>

DeityGB is a Game Boy and Game Boy Color emulator built to make the little
details feel right: the boot sequence, the color palettes, the strange timing
edges, the save clock ticking while the emulator is closed, and—most
importantly—the games themselves.

It has grown from a CPU that could just about boot Tetris into a packaged
desktop application with a ROM library, accurate DMG and CGB graphics, all four
audio channels, battery-backed saves, MBC3 real-time clocks, and an unusually
thorough hardware-test record. You can launch it, point it at a folder full of
games, and start playing. No command line required.

<p align="center">
  <img src="assets/screenshots/oracle-of-seasons-frame.png" alt="The Legend of Zelda: Oracle of Seasons running in DeityGB" width="900">
</p>
<p align="center"><em>The Legend of Zelda: Oracle of Seasons, running in native Game Boy Color mode.</em></p>

Bring your own legally obtained `.gb` and `.gbc` ROMs; games are not part of a
normal DeityGB release.

## A long road home

I started this beloved project in 2021, fresh out of my freshman-year computer
architecture class at the University of Texas. Building a Game Boy emulator
felt like the perfect way to find out whether I had really understood any of
it. Then life did what life does: the project followed me through several years
of changing priorities, long pauses, occasional bursts of progress, and all the
general chaos in between.

In 2025, DeityGB finally began to feel like a real Game Boy. The DMG boot ROM
worked. Tetris became playable. Before long, Link's Awakening and Super Mario
Land were playable too—even if they still carried a few charmingly incorrect
graphics along for the ride.

And now, at long last, 2026 is the year DeityGB is getting the finish it always
deserved: native Game Boy Color support, accurate audio, persistent saves and
clocks, desktop applications, a proper game library, full Mooneye and Blargg
coverage, and years of accumulated hardware mysteries finally tracked down one
cycle at a time.

It took the scenic route. It made it home.

### Then and now

The old screenshots are part of the fun. They are a record of the moment when
the emulator could execute a game but did not yet understand everything the
hardware was trying to tell it.

<table>
  <tr>
    <th>Pokémon Red JP — then</th>
    <th>Pokémon Red JP — now</th>
  </tr>
  <tr>
    <td><img src="assets/screenshots/history-pokemon-red-jp-before.png" alt="An early, graphically corrupted Pokémon Red Japanese title screen"></td>
    <td><img src="assets/screenshots/history-pokemon-red-jp-after.png" alt="The corrected Pokémon Red Japanese title screen"></td>
  </tr>
  <tr>
    <td align="center"><em>Executing the game was only half the battle.</em></td>
    <td align="center"><em>Tiles, sprites, and priorities where they belong.</em></td>
  </tr>
  <tr>
    <th>Link's Awakening (DMG) — 2025</th>
    <th>Link's Awakening DX — 2026</th>
  </tr>
  <tr>
    <td><img src="assets/screenshots/history-links-awakening-dmg-2025.png" alt="A 2025 Link's Awakening DMG build with heavily corrupted graphics"></td>
    <td><img src="assets/screenshots/links-awakening-dx-frame.png" alt="Link's Awakening DX rendered correctly in DeityGB"></td>
  </tr>
  <tr>
    <td align="center"><em>Playable, technically.</em></td>
    <td align="center"><em>Fixed—and in full color.</em></td>
  </tr>
  <tr>
    <th>DMG Acid2 — 2025</th>
    <th>DMG Acid2 — current</th>
  </tr>
  <tr>
    <td><img src="assets/screenshots/history-dmg-acid2-2025.png" alt="An older DMG Acid2 result"></td>
    <td><img src="assets/screenshots/dmg-acid2-current.png" alt="The current reference-correct DMG Acid2 output"></td>
  </tr>
  <tr>
    <td align="center"><em>Close enough to be encouraging; wrong enough to keep digging.</em></td>
    <td align="center"><em>Generated headlessly from the current core, with every reference shade in place.</em></td>
  </tr>
</table>

## The short version

- Game Boy (DMG) and Game Boy Color (CGB) emulation
- **75/75 Mooneye acceptance tests passing**
- **Every bundled Blargg test passing**, including CPU, timing, interrupts,
  OAM behavior, the HALT bug, and both DMG and CGB sound suites
- Reference-exact DMG Acid2 shade layout and pixel-exact CGB Acid2 color
- Full four-channel audio: two pulse channels, programmable wave, and noise
- ROM-only, MBC1, MBC3, and MBC5 cartridges
- Battery-backed `.sav` files and persistent MBC3 real-time clocks
- CGB palettes, attributes, banked VRAM/WRAM, VRAM DMA, and double speed
- A friendly in-app ROM library with recursive folder scanning and fast held-key
  navigation
- Normal-speed hardware-clock pacing and hold-to-use 2× fast-forward
- Packaged macOS and Linux desktop applications with the DeityGB icon
- A headless runner for deterministic testing, screenshots, traces, and ROM
  diagnostics

## Games we actually play on it

Passing a test ROM is wonderful, but the point is still to wander around
Koholint, hear the Pokémon Center theme, and get flattened by the first Waddle
Dee in Green Greens. DeityGB has been played and exercised with:

<table>
  <tr>
    <td><img src="assets/screenshots/pokemon-silver-party.png" alt="Pokémon Silver party screen in DeityGB"></td>
    <td><img src="assets/screenshots/pokemon-red-dmg.png" alt="Pokémon Red running with DeityGB's DMG palette"></td>
    <td><img src="assets/screenshots/kirbys-dream-land-frame.png" alt="Kirby's Dream Land running in DeityGB"></td>
  </tr>
  <tr>
    <td align="center"><em>Pokémon Silver</em></td>
    <td align="center"><em>Pokémon Red</em></td>
    <td align="center"><em>Kirby's Dream Land</em></td>
  </tr>
  <tr>
    <td><img src="assets/screenshots/links-awakening-dx-frame.png" alt="Link's Awakening DX in DeityGB"></td>
    <td><img src="assets/screenshots/dragon-warrior-iii-gameplay.png" alt="Dragon Warrior III in DeityGB"></td>
    <td><img src="assets/screenshots/super-mario-bros-deluxe-frame.png" alt="Super Mario Bros. Deluxe in DeityGB"></td>
  </tr>
  <tr>
    <td align="center"><em>Link's Awakening DX</em></td>
    <td align="center"><em>Dragon Warrior III</em></td>
    <td align="center"><em>Super Mario Bros. Deluxe</em></td>
  </tr>
  <tr>
    <td><img src="assets/screenshots/tetris-dx-gameplay.png" alt="Tetris DX gameplay in DeityGB"></td>
    <td><img src="assets/screenshots/tennis-gameplay.png" alt="The original Game Boy Tennis running in DeityGB"></td>
    <td><img src="assets/screenshots/links-awakening-dx-title.png" alt="Colorful Link's Awakening DX artwork in DeityGB"></td>
  </tr>
  <tr>
    <td align="center"><em>Tetris DX</em></td>
    <td align="center"><em>Tennis</em></td>
    <td align="center"><em>Link's Awakening DX</em></td>
  </tr>
  <tr>
    <td><img src="assets/screenshots/links-awakening-dx-shop-gameplay.png" alt="Link's Awakening DX shop scene in DeityGB"></td>
    <td><img src="assets/screenshots/pokemon-crystal-battle-gameplay.png" alt="Pokémon Crystal battle in DeityGB"></td>
    <td><img src="assets/screenshots/pokemon-crystal-overworld-gameplay.png" alt="Pokémon Crystal overworld in DeityGB"></td>
  </tr>
  <tr>
    <td align="center"><em>Link's Awakening DX</em></td>
    <td align="center"><em>Pokémon Crystal</em></td>
    <td align="center"><em>Pokémon Crystal</em></td>
  </tr>
</table>

### Pokémon

- **Pokémon Red** — monochrome play, menus, SRAM, and MBC3 banking
- **Pokémon Silver** — native Game Boy Color graphics, saves, and real-time clock
- **Pokémon Crystal** — native color, expanded MBC3 cartridge RAM, saves, and RTC

### Zelda

- **The Legend of Zelda: Link's Awakening**
- **The Legend of Zelda: Link's Awakening DX** — including the CGB palettes and
  MBC5 cartridge path
- **The Legend of Zelda: Oracle of Seasons** — including an APU investigation
  that fixed an incorrect persistent noise channel in the overworld

### Everything else

- **Dragon Warrior III**
- **Wario Land: Super Mario Land 3**
- **Super Mario Land**
- **Super Mario Bros. Deluxe**
- **Kirby's Dream Land** on original monochrome hardware
- **Donkey Kong Land**
- **Tetris**
- **Tetris DX**
- **Tennis**
- **Yoshi**

Several of these go beyond a casual boot check. The automated regression suite
drives Pokémon Red to its New Game menu, enters Green Greens in Kirby's Dream
Land, checks Pokémon Silver's native-color title screen, and takes Link's
Awakening DX through file selection into Marin's opening dialogue. Their final
framebuffers are hashed so an innocent-looking CPU or graphics change cannot
silently break a known-good game path.

This is a compatibility list, not a claim that every one of these games has
been exhaustively automated from the title screen through the final credits.

## What is emulated

### CPU, timing, and interrupts

DeityGB implements the LR35902 instruction set, CB-prefixed operations,
interrupt dispatch, HALT and the HALT bug, timers, joypad interrupts, serial
transfers, OAM DMA, and the timing boundaries software can observe while the
PPU is drawing. CGB `KEY1`/`STOP` double-speed switching is supported without
making the screen, audio, or real-time clock run twice as fast.

### Graphics

The PPU renders the 160×144 Game Boy display with backgrounds, windows,
sprites, scrolling, DMG palettes, sprite transparency and priority, and LCD/STAT
interrupt behavior.

In Game Boy Color mode it adds RGB555 background and object palettes, tile and
sprite attributes, two VRAM banks, banked WRAM, CGB priority rules, general VRAM
DMA, HBlank DMA, and native-color output. Both DMG and CGB Acid2 are checked
against their reference images; CGB is pixel-for-pixel RGB exact, while DMG
matches every shade index and displays it through DeityGB's green LCD palette.

### Audio

All four Game Boy sound channels are implemented:

- Channel 1 pulse with frequency sweep
- Channel 2 pulse
- Channel 3 programmable wave
- Channel 4 noise

That includes length counters, envelopes, sweep overflow, DAC behavior, the
frame sequencer, stereo routing, wave RAM access rules, DMG wave-retrigger
corruption, and CGB PCM state. The complete bundled Blargg sound baseline passes
**12/12 DMG and 12/12 CGB** tests.

### Cartridges and saves

DeityGB supports ROM-only cartridges plus MBC1, MBC3, and MBC5 banking, including
large ROM and RAM configurations used by the games above. Battery-backed RAM is
saved beside the ROM as a normal `.sav` file. Writes are debounced and atomic,
so playing does not hammer the disk and an interrupted write does not replace a
good save with half a file.

MBC3 clocks are persisted separately in a small `.rtc` sidecar. The clock keeps
up with real elapsed time while DeityGB is closed, respects the cartridge's halt
bit, and leaves the `.sav` file as ordinary cartridge RAM that other tools can
understand.

## Accuracy and test status

The current green baseline is:

| Suite | Result | What it exercises |
| --- | ---: | --- |
| Mooneye acceptance | **75/75** | CPU timing, interrupts, timers, DMA, serial, PPU modes, STAT/LY timing, LCD startup, and model-specific startup state |
| Blargg CPU instructions | **11/11 groups** | The instruction set and flags |
| Blargg sound | **24/24** | 12 DMG and 12 CGB APU tests |
| Blargg timing and hardware suites | **All bundled tests pass** | Instruction/memory timing, interrupts, OAM corruption, and HALT behavior |
| DMG Acid2 | **Exact shade layout** | Background, window, and sprite composition |
| CGB Acid2 | **Pixel-exact RGB** | CGB palettes, attributes, priority, and sprite behavior |

“Full Mooneye” here means the complete 75-ROM acceptance tree in the pinned
Mooneye snapshot used by the project. “Full Blargg” means every Blargg ROM
bundled and run by DeityGB: `cpu_instrs`, `instr_timing`, both `mem_timing`
generations, `interrupt_time`, `halt_bug`, all eight OAM groups, and the DMG/CGB
sound suites.

The timing work is deliberately tested at the hardware boundary rather than
patched per game. That includes obscure-but-real behavior such as timer falling
edges, the shared STAT interrupt line, LCD-enable startup, LY 153, sprite fetch
stalls, SCX-dependent transfer length, OAM access windows, and the exact machine
cycle on which a CPU read sees a new PPU mode.

For the commands, individual cases, and pedagogical notes behind these claims,
see [the development verification guide](docs/development-verification.md).

## Playing

Open the DeityGB application and press **Enter** on the splash screen. Choose a
folder containing `.gb` or `.gbc` files and DeityGB will find them recursively.

<p align="center">
  <img src="assets/screenshots/rom-library.png" alt="DeityGB's built-in ROM library" width="760">
</p>
<p align="center"><em>Pick a folder, browse with the same controls you use to play, and get straight to the good part.</em></p>

### ROM library

| Control | Action |
| --- | --- |
| `W` / `S`, Up / Down, or `Tab` | Move through games; hold to scroll quickly |
| `A` / `D` or Left / Right | Jump ten games |
| `J` or Enter | Launch the selected game |
| `K` or Escape | Choose a different folder |

### In game

| Control | Game Boy button |
| --- | --- |
| `W` `A` `S` `D` | D-pad |
| `J` / `K` | A / B |
| Enter | Start |
| Left Shift | Select |
| Hold `Tab` | 2× fast-forward |
| `H` or `F1` | Toggle the controls overlay |

The emulator paces itself from the Game Boy's hardware clock rather than the
host monitor's refresh rate, so a 120 Hz MacBook display does not accidentally
turn every game into a permanent speedrun.

## Desktop builds

DeityGB is packaged as a normal `.app` on macOS and as an application directory
with desktop launcher and icon on x86-64 Linux. The executable embeds its boot
assets and instruction data, so it does not need the source checkout sitting
beside it.

macOS development builds are ad-hoc signed. A build shared outside the machine
that created it may require **System Settings → Privacy & Security → Open
Anyway** the first time it is launched. Removing that warning for public
downloads requires Apple Developer ID signing and notarization.

## Building from source

The reproducible development environment uses Nix:

```sh
nix develop --command cargo build --release --bin DeityGB
```

The resulting development executable is `target/release/DeityGB`.

For a zipped macOS application, install `cargo-bundle` with a current stable
Rust toolchain and run:

```sh
./scripts/package-macos.sh
```

On x86-64 Linux, build the portable application directory and archive with:

```sh
./scripts/package-linux.sh
```

Version tags and manually dispatched runs of
`.github/workflows/package-desktop.yml` build both desktop artifacts.

## Running tests

The ordinary release suite is:

```sh
nix develop --command cargo test --release
```

The heavier ROM regressions are intentionally ignored by the ordinary suite and
can be run explicitly. For example:

```sh
nix develop --command cargo test --release --test headless \
  mooneye_acceptance_known_passes_remain_green -- --ignored --exact

nix develop --command cargo test --release --test headless \
  blargg_sound_core_roms_pass -- --ignored --exact
```

The headless runner can also execute a ROM for a fixed emulated duration, script
button presses, capture the original 160×144 framebuffer, trace instructions,
and report Mooneye or Blargg results without opening a window.

## Scope

DeityGB aims to be a delightful, accurate emulator for the games people
actually want to play. It does not currently emulate a link cable, and cartridge
controllers outside ROM-only, MBC1, MBC3, and MBC5 are not a compatibility
promise. There will always be another wonderfully strange hardware edge case;
when one matters, the preferred fix is to understand the hardware, model it in
the shared core, and add a regression—not slip in a game-specific hack.

That philosophy is how DeityGB reached full Mooneye and Blargg coverage, and it
is how it intends to stay fun to work on.
