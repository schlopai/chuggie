#!/usr/bin/env bash
# Headless screenshot of a GBA ROM (or a .tish source, which it builds first) via libmgba —
# NO emulator window, NO macOS Screen-Recording/Accessibility permission needed. Outputs a PNG.
#
# Usage:  scripts/screenshot.sh <rom.gba | src/main.tish> [out.png] [frames] [keys]
#   frames : frames to run before capturing (default 180 ~ 3s at 59.7fps).
#   keys   : held keys ("a,start") or a frame schedule ("90:a,120:") — see tools/gba-shot.c.
#   env    : MGBA_PREFIX=... (default `brew --prefix mgba`); TISH=... (if building a .tish and
#            `tish` is not on PATH); GBA_SHOT_LOG=1 / GBA_SHOT_TRACE=1 (see tools/gba-shot.c).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"          # repo root
input="${1:?usage: screenshot.sh <rom.gba|main.tish> [out.png] [frames]}"
out="${2:-screenshot.png}"
frames="${3:-180}"
keys="${4:-}"

# locate libmgba (headers + dylib/so): explicit override, then macOS Homebrew, then system dirs
# (Linux `libmgba-dev` installs to /usr — headers /usr/include/mgba, lib in the default search path).
mgba_prefix="${MGBA_PREFIX:-}"
if [ -z "$mgba_prefix" ]; then
  mgba_prefix="$(brew --prefix mgba 2>/dev/null || true)"
fi
if [ -z "$mgba_prefix" ] || [ ! -d "$mgba_prefix/include/mgba" ]; then
  for p in /usr/local /usr /opt/homebrew; do
    [ -d "$p/include/mgba" ] && { mgba_prefix="$p"; break; }
  done
fi
if [ -z "$mgba_prefix" ] || [ ! -d "$mgba_prefix/include/mgba" ]; then
  echo "error: libmgba not found — install it (macOS: brew install mgba; Debian/Ubuntu:" >&2
  echo "       apt-get install libmgba-dev) or set MGBA_PREFIX=<prefix>." >&2
  exit 1
fi

# build the headless renderer if missing/stale. A non-standard prefix (Homebrew) needs -I/-L + rpath;
# a system prefix (/usr) is already on the compiler's default search paths.
shot="$here/tools/gba-shot"
if [ ! -x "$shot" ] || [ "$here/tools/gba-shot.c" -nt "$shot" ]; then
  echo "compiling tools/gba-shot (against libmgba at $mgba_prefix) ..." >&2
  # -lm for the audio capture's RMS; a no-op on macOS (libm is in libSystem), required on Linux.
  cflags=(); ldflags=(-lmgba -lm)
  if [ "$mgba_prefix" != "/usr" ]; then
    cflags=(-I"$mgba_prefix/include")
    ldflags=(-L"$mgba_prefix/lib" -lmgba -lm -Wl,-rpath,"$mgba_prefix/lib")
  fi
  cc "$here/tools/gba-shot.c" -o "$shot" "${cflags[@]}" "${ldflags[@]}"
fi

# build the ROM if given a .tish source
rom="$input"
if [[ "$input" == *.tish ]]; then
  rom="${input%.tish}.gba"
  tish="${TISH:-$(command -v tish || true)}"
  [ -n "$tish" ] || { echo "error: building a .tish needs 'tish' on PATH or TISH=..." >&2; exit 1; }
  echo "building $input -> $rom" >&2
  "$tish" build "$input" --target gba -o "$rom"
fi
[ -f "$rom" ] || { echo "error: ROM not found: $rom" >&2; exit 1; }

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
