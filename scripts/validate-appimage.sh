#!/usr/bin/env bash

set -euo pipefail

die() {
  echo "validate-appimage: $*" >&2
  exit 1
}

if [[ $# -ne 1 ]]; then
  die "usage: $0 path/to/Application.AppImage"
fi

appimage="$(realpath "$1")"
[[ -f "$appimage" ]] || die "AppImage not found: $appimage"
command -v unsquashfs >/dev/null 2>&1 || die "unsquashfs is required (install squashfs-tools)"

offset="$($appimage --appimage-offset)"
[[ "$offset" =~ ^[0-9]+$ ]] || die "could not determine the type-2 runtime offset"

work_dir="$(mktemp -d)"
cleanup() {
  rm -rf -- "$work_dir"
}
trap cleanup EXIT

# Inspect the modes stored in SquashFS directly. AppImage's extraction command
# deliberately creates a private 0700 tree, so stat/find on extracted files do
# not report the permissions that users see when the AppImage is mounted.
listing="$work_dir/squashfs-listing.txt"
unsquashfs -lln -o "$offset" "$appimage" >"$listing"

bad_directories="$(
  awk '$1 ~ /^d/ && (substr($1, 8, 1) != "r" || substr($1, 10, 1) != "x")' "$listing"
)"
if [[ -n "$bad_directories" ]]; then
  echo "AppImage contains a directory that is not world-readable/searchable:" >&2
  echo "$bad_directories" >&2
  exit 1
fi

bad_files="$(awk '$1 ~ /^-/ && substr($1, 8, 1) != "r"' "$listing")"
if [[ -n "$bad_files" ]]; then
  echo "AppImage contains a file that is not world-readable:" >&2
  echo "$bad_files" >&2
  exit 1
fi

launcher_mode() {
  local launcher="$1"
  awk -v path="squashfs-root/$launcher" '$NF == path { print $1; exit }' "$listing"
}

app_run_mode="$(launcher_mode AppRun)"
[[ -n "$app_run_mode" ]] || die "AppRun is missing from the SquashFS payload"
[[ "${app_run_mode:9:1}" == "x" ]] || die "AppRun is not executable by other users ($app_run_mode)"

wrapped_mode="$(launcher_mode AppRun.wrapped)"
if [[ -n "$wrapped_mode" && "${wrapped_mode:9:1}" != "x" ]]; then
  die "AppRun.wrapped is not executable by other users ($wrapped_mode)"
fi

# Extraction is still useful for validating the relative .DirIcon symlink; do
# not use the extracted tree to validate modes.
extract_dir="$work_dir/extract"
mkdir "$extract_dir"
(
  cd "$extract_dir"
  "$appimage" --appimage-extract >/dev/null
)
[[ -L "$extract_dir/squashfs-root/.DirIcon" ]] || die ".DirIcon is missing or is not a symlink"
[[ -e "$extract_dir/squashfs-root/.DirIcon" ]] || die ".DirIcon is a broken symlink"
diricon_target="$(readlink "$extract_dir/squashfs-root/.DirIcon")"
[[ "$diricon_target" != /* ]] || die ".DirIcon must be relative, got: $diricon_target"

require_payload_file() {
  local name="$1"
  local found
  found="$(find "$extract_dir/squashfs-root" -type f -name "$name" -print -quit)"
  [[ -n "$found" ]] || die "required packaged resource is missing: $name"
}

require_payload_file NOTICE
require_payload_file libopus.txt
require_payload_file symphonia-mpl-2.0.txt
require_payload_file symphonia-source.json
require_payload_file symphonia-adapter-apache-2.0.txt

echo "Validated AppImage catalog requirements: $appimage"
