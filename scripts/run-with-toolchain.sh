#!/usr/bin/env bash
set -euo pipefail
export PATH="${HOME}/.cargo/bin:${PATH}"
if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install the pinned Rust toolchain (see rust-toolchain.toml):" >&2
  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.88.0" >&2
  echo "Then restart this terminal, or run: source \"\$HOME/.cargo/env\"" >&2
  exit 127
fi
exec "$@"
