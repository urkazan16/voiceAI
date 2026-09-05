#!/usr/bin/env bash
set -euo pipefail
KEEP=0
if [[ "${1:-}" == "--keep-history" ]]; then
  KEEP=1
elif [[ -t 0 ]]; then
  read -r -p "Keep dictation history? [y/N] " ans
  if [[ "${ans}" =~ ^[Yy] ]]; then
    KEEP=1
  fi
fi
ROOT="${LOCALFLOW_DATA_DIR:-$HOME/Library/Application Support/LocalFlow}"
echo "Uninstalling LocalFlow data in $ROOT"
REMOVED=()
skip_rm() { echo "skip $1"; }
rm_path() {
  local p="$1"
  if [[ -e "$p" ]]; then
    rm -rf "$p"
    REMOVED+=("$p")
    echo "removed $p"
  fi
}
rm_path "$ROOT/audio"
rm_path "$ROOT/models"
rm_path "$ROOT/logs"
rm_path "$ROOT/config"
if [[ "$KEEP" -eq 0 ]]; then
  rm_path "$ROOT/database"
  rmdir "$ROOT" 2>/dev/null || true
else
  echo "kept $ROOT/database"
fi
echo "Removed components:"
printf '  %s\n' "${REMOVED[@]:-(none)}"
