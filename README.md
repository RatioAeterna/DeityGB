# DeityGB

DeityGB is a desktop Game Boy and Game Boy Color emulator. It includes an
in-app ROM library, cartridge save and MBC3 RTC persistence, DMG/CGB audio and
video, 2× fast-forward, and bundled boot assets.

## Playing

Launch the DeityGB application without command-line arguments. On the startup
screen, press Enter and choose a directory containing `.gb` or `.gbc` files.
The library searches that directory recursively.

- W/S, Up/Down, or Tab: move through games; hold to scroll rapidly
- A/D or Left/Right: move ten games
- J or Enter: launch the selected game
- K or Escape: choose a different folder
- W/A/S/D: Game Boy D-pad during play
- J/K: Game Boy A/B
- Enter: Start
- Left Shift: Select
- Tab: hold for 2× fast-forward during play
- H or F1: toggle the controls overlay

The GUI still accepts a ROM path as its first non-option command-line argument
for development and automation. Audio is enabled by default; `--no-apu` is
available for diagnostics, and `--choose-rom` opens the directory chooser
immediately for packaging smoke tests.

## Building

The ordinary release build is:

```sh
nix develop --command cargo build --release --bin DeityGB
```

The GUI embeds its icon and both boot assets, so the resulting executable does
not need to find files in the source checkout.

## Packaged applications

Install `cargo-bundle` once for macOS packaging:

```sh
cargo install cargo-bundle --version 0.11.0 --locked
```

That installation command expects a current stable Rust toolchain. The pinned
Nix development shell can instead use the packaged command directly:

```sh
nix develop --command nix shell nixpkgs#cargo-bundle --command cargo bundle --release --format osx
```

Then build a zipped `.app` on macOS:

```sh
./scripts/package-macos.sh
```

The script gives the local build a complete ad-hoc signature. Public downloads
that open without an unidentified-developer warning additionally require an
Apple Developer ID certificate and notarization; those credentials deliberately
do not live in this repository.

On x86-64 Linux, build a portable application directory and tarball with:

```sh
./scripts/package-linux.sh
```

Tagged releases and manually dispatched runs of
`.github/workflows/package-desktop.yml` build both artifacts automatically.
The Linux archive includes the executable, desktop launcher, icon, and install
notes; normal host graphics and audio libraries remain operating-system
dependencies.

## Verification

See [docs/development-verification.md](docs/development-verification.md) for the
ordinary suite, Mooneye 75/75 acceptance guard, Blargg CPU/APU coverage, Acid2
image checks, and deterministic game regressions.
