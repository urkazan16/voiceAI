#!/usr/bin/env bash

set -euo pipefail

die() {
  echo "harden-appimage: $*" >&2
  exit 1
}

if [[ $# -ne 1 ]]; then
  die "usage: $0 path/to/Application.AppImage"
fi

appimage="$(realpath "$1")"
[[ -f "$appimage" ]] || die "AppImage not found: $appimage"
command -v mksquashfs >/dev/null 2>&1 || die "mksquashfs is required (install squashfs-tools)"

work_dir="$(mktemp -d)"
repacked="${appimage}.repacked.$$"
cleanup() {
  rm -rf -- "$work_dir"
  rm -f -- "$repacked"
}
trap cleanup EXIT

offset="$($appimage --appimage-offset)"
[[ "$offset" =~ ^[0-9]+$ ]] || die "could not determine the type-2 runtime offset"

(
  cd "$work_dir"
  umask 022
  "$appimage" --appimage-extract >/dev/null
)

app_dir="$work_dir/squashfs-root"
[[ -d "$app_dir" ]] || die "the AppImage did not extract a squashfs-root directory"

# linuxdeploy can preserve 0770 on AppRun.wrapped. That works for the build
# owner, but fails when catalogs or end users execute the AppImage as a
# different account. Keep existing executable bits and grant world read/search
# access throughout the AppDir, then explicitly harden both launchers.
chmod -R a+rX "$app_dir"
chmod a+x "$app_dir/AppRun"
if [[ -e "$app_dir/AppRun.wrapped" ]]; then
  chmod a+x "$app_dir/AppRun.wrapped"
fi

runtime="$work_dir/runtime"
squashfs="$work_dir/payload.squashfs"

head -c "$offset" "$appimage" >"$runtime"
# Tauri's current type-2 runtime supports zlib and Zstandard, but not XZ.
# Zstandard also keeps release packaging reasonably fast while preserving the
# runtime taken from the original Tauri-generated AppImage.
mksquashfs "$app_dir" "$squashfs" -root-owned -noappend -comp zstd >/dev/null
cat "$runtime" "$squashfs" >"$repacked"
chmod a+x "$repacked"

# Verify compression, launcher modes, and .DirIcon before replacing the
# original Tauri artifact.
bash "$(dirname "$0")/validate-appimage.sh" "$repacked"

mv "$repacked" "$appimage"
echo "Hardened AppImage permissions: $appimage"
