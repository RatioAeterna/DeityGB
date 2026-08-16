#!/usr/bin/env bash
set -euo pipefail

if ! cargo bundle --version >/dev/null 2>&1; then
  echo "cargo-bundle is required: cargo install cargo-bundle --version 0.11.0 --locked" >&2
  exit 1
fi

cargo bundle --release --format osx
codesign --force --deep --sign - target/release/bundle/osx/DeityGB.app
mkdir -p dist
ditto -c -k --sequesterRsrc --keepParent \
  target/release/bundle/osx/DeityGB.app \
  dist/DeityGB-macOS.zip

echo "Created dist/DeityGB-macOS.zip"
