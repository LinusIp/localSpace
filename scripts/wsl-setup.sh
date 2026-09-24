#!/usr/bin/env bash
# The one-time setup of WSL for scripts/check.sh (docs/BUILD.md, "Checking
# before a push"): what CI's runner has, on Ubuntu 24.04 in WSL. Run once, by
# the machine's owner, inside Ubuntu: it asks for their password for the
# system packages. What it fetches, all from the publishers themselves:
#   - Ubuntu's packages for building the workspace and the Tauri shell,
#     about 500 MB (apt);
#   - Google Chrome, which the browser walks drive, about 115 MB
#     (dl.google.com);
#   - Rust through rustup with rustfmt, clippy and the wasm32-wasip2 target,
#     about 350 MB (sh.rustup.rs, static.rust-lang.org);
#   - Node.js 24.19.0, the version on the Windows side, about 30 MB
#     (nodejs.org), checked against the SHA-256 nodejs.org publishes.
#
#   bash scripts/wsl-setup.sh
set -euo pipefail

sudo apt-get update
sudo apt-get install -y build-essential pkg-config curl git xz-utils procps \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev

if ! command -v google-chrome > /dev/null 2>&1; then
  deb="$(mktemp -d)/google-chrome-stable_current_amd64.deb"
  curl -fsSL -o "$deb" https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb
  sudo apt-get install -y "$deb"
fi

if ! command -v rustup > /dev/null 2>&1 && [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --profile minimal --default-toolchain stable -c rustfmt -c clippy -t wasm32-wasip2
fi
# shellcheck source=/dev/null
. "$HOME/.cargo/env"
rustup target add wasm32-wasip2

node_version=24.19.0
node_dir="node-v$node_version-linux-x64"
if [ ! -x "$HOME/.local/$node_dir/bin/node" ]; then
  work="$(mktemp -d)"
  curl -fsSL -o "$work/$node_dir.tar.xz" "https://nodejs.org/dist/v$node_version/$node_dir.tar.xz"
  curl -fsSL -o "$work/SHASUMS256.txt" "https://nodejs.org/dist/v$node_version/SHASUMS256.txt"
  (cd "$work" && grep " $node_dir.tar.xz\$" SHASUMS256.txt | sha256sum -c -)
  mkdir -p "$HOME/.local/bin"
  tar -xJf "$work/$node_dir.tar.xz" -C "$HOME/.local"
  for tool in node npm npx; do ln -sf "$HOME/.local/$node_dir/bin/$tool" "$HOME/.local/bin/$tool"; done
fi
export PATH="$HOME/.local/bin:$PATH"

echo "ready for scripts/check.sh: $(rustc --version), node $(node --version), $(google-chrome --version)"
