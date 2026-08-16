#!/usr/bin/env bash
set -euo pipefail

cargo build --release --bin DeityGB --no-default-features
cp target/release/DeityGB target/release/DeityGB-no-audio-link
cargo build --release --bin deitygb-audio

package_dir="dist/DeityGB-linux-x86_64"
rm -rf "$package_dir"
mkdir -p "$package_dir"
install -m 755 target/release/DeityGB-no-audio-link "$package_dir/DeityGB"
install -m 755 target/release/deitygb-audio "$package_dir/deitygb-audio"
install -m 644 packaging/linux/DeityGB.desktop "$package_dir/DeityGB.desktop"
install -m 644 assets/deitygb-icon-512.png "$package_dir/deitygb.png"
install -m 644 packaging/linux/README.txt "$package_dir/README.txt"

tar -C dist -czf dist/DeityGB-linux-x86_64.tar.gz DeityGB-linux-x86_64
echo "Created dist/DeityGB-linux-x86_64.tar.gz"
