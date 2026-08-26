#!/usr/bin/env bash
# Headless screenshot of a GBA ROM (or a .tish source, which it builds first) via libmgba —
# NO emulator window, NO macOS Screen-Recording/Accessibility permission needed. Outputs a PNG.
# For an animated clip of the same run, see scripts/gif.sh.
#
# Usage:  scripts/screenshot.sh <rom.gba | src/main.tish> [out.png] [frames] [keys]
#   frames : frames to run before capturing (default 180 ~ 3s at 59.7fps).
#   keys   : held keys ("a,start") or a frame schedule ("90:a,120:") — see tools/gba-shot.c.
#   env    : MGBA_PREFIX=... (default `brew --prefix mgba`); TISH=... (if building a .tish and
#            `tish` is not on PATH); GBA_SHOT_LOG=1 / GBA_SHOT_TRACE=1 (see tools/gba-shot.c).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"          # repo root
. "$here/scripts/shot_common.sh"
input="${1:?usage: screenshot.sh <rom.gba|main.tish> [out.png] [frames]}"
out="${2:-screenshot.png}"
frames="${3:-180}"
keys="${4:-}"

mgba_prefix="$(find_mgba_prefix)"
shot="$(ensure_gba_shot "$here" "$mgba_prefix")"
rom="$(resolve_rom "$input")"

# render headless -> PPM, then convert to PNG
# `mktemp -t gbashot` is a BSD-ism: macOS treats the argument as a PREFIX, GNU treats it as a
# TEMPLATE and rejects it for having fewer than three X's ("too few X's in template"). So this
# worked locally and failed on every Linux runner. An explicit path with X's is portable to both.
ppm="$(mktemp "${TMPDIR:-/tmp}/gbashot.XXXXXX").ppm"
trap 'rm -f "$ppm"' EXIT
"$shot" "$rom" "$ppm" "$frames" "$keys"
if command -v sips >/dev/null 2>&1; then
  sips -s format png "$ppm" --out "$out" >/dev/null
elif command -v magick >/dev/null 2>&1; then
  magick "$ppm" "$out"
elif command -v convert >/dev/null 2>&1; then
  convert "$ppm" "$out"
else
  out="${out%.png}.ppm"; cp "$ppm" "$out"
  echo "note: no PNG converter (sips/ImageMagick) — left PPM." >&2
fi
echo "wrote $out"
