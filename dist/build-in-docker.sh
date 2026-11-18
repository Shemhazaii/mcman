#!/usr/bin/env bash
set -euo pipefail
cd /app

echo "== [1/5] system deps =="
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  mingw-w64 binutils-mingw-w64 g++-mingw-w64-x86-64 \
  build-essential pkg-config curl wget file patch \
  libwebkit2gtk-4.1-dev libssl-dev libxdo-dev librsvg2-dev \
  libayatana-appindicator3-dev libpulse-dev \
  libglib2.0-dev libgtk-3-dev

echo "== [2/5] rust targets =="
rustup target add x86_64-pc-windows-gnu

echo "== [3/5] windows exe build =="
cargo build --release --target x86_64-pc-windows-gnu
ls -la target/x86_64-pc-windows-gnu/release/mcman.exe

echo "== [4/5] install tauri-cli =="
cargo install tauri-cli --locked

echo "== [5/5] appimage bundle =="
cargo tauri build --bundles appimage
find target/release/bundle -maxdepth 2 -type f -name '*.AppImage' -exec ls -la {} \;

echo "== chmod outputs to host user =="
chown -R 1000:1000 /app/target 2>/dev/null || true
echo "BUILD_ALL_DONE"
