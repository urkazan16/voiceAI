#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"
if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null; then
  echo "Install Rust 1.88+ first: https://rustup.rs"
  exit 1
fi
if ! command -v npm >/dev/null; then
  echo "Install Node.js 20+ first."
  exit 1
fi
npm install
echo "LocalFlow is ready. Run: npm run tauri dev"
echo "Hold Control+Shift+Space to record; release to process."
