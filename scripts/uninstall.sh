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

UNAME="$(uname -s)"
AGENT=""
DESKTOP=""
if [[ -n "${LOCALFLOW_DATA_DIR:-}" ]]; then
  ROOT="$LOCALFLOW_DATA_DIR"
elif [[ "$UNAME" == "Darwin" ]]; then
  ROOT="${HOME}/Library/Application Support/LocalFlow"
  AGENT="${HOME}/Library/LaunchAgents/app.localflow.desktop.plist"
elif [[ "$UNAME" == "Linux" ]]; then
  ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/LocalFlow"
  DESKTOP="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/app.localflow.desktop"
else
  echo "On Windows run: powershell -File scripts/uninstall.ps1" >&2
  exit 1
fi

echo "Uninstalling LocalFlow data in $ROOT"
if [[ -n "$AGENT" && -f "$AGENT" ]]; then
  launchctl unload -w "$AGENT" 2>/dev/null || true
  rm -f "$AGENT"
  echo "removed autostart $AGENT"
fi
if [[ -n "$DESKTOP" && -f "$DESKTOP" ]]; then
  rm -f "$DESKTOP"
  echo "removed autostart $DESKTOP"
fi
REMOVED=()
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
