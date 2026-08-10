#!/run/current-system/sw/bin/bash

cd "$(dirname "$0")/.." || exit 1
nix develop --command cargo run --release --bin DeityGB -- src/roms/pokemon_red.gb
