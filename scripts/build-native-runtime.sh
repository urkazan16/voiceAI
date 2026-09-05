#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cmake -S "$ROOT/src-tauri/native" -B "$ROOT/src-tauri/native/build"
cmake --build "$ROOT/src-tauri/native/build"
echo "Native runtime stub built. Link a reviewed MIT subset of whisper.cpp/llama.cpp only after recording the commit SHA in docs/licensing/NATIVE.md."
