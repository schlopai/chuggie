#!/usr/bin/env bash
# Shared front-end for the headless capture scripts (screenshot.sh, gif.sh). Sourced, not run:
# it only defines functions. Everything here is about GETTING TO a runnable tools/gba-shot —
# finding libmgba, compiling the tool on demand, and turning a .tish source into a ROM.

# Locate libmgba (headers + dylib/so): explicit override, then macOS Homebrew, then system dirs
# (Linux `libmgba-dev` installs to /usr — headers /usr/include/mgba, lib in the default search path).
# Prints the prefix on stdout.
find_mgba_prefix() {
  local mgba_prefix="${MGBA_PREFIX:-}"
  if [ -z "$mgba_prefix" ]; then
    mgba_prefix="$(brew --prefix mgba 2>/dev/null || true)"
  fi
  if [ -z "$mgba_prefix" ] || [ ! -d "$mgba_prefix/include/mgba" ]; then
    local p
    for p in /usr/local /usr /opt/homebrew; do
      [ -d "$p/include/mgba" ] && { mgba_prefix="$p"; break; }
    done
  fi
  if [ -z "$mgba_prefix" ] || [ ! -d "$mgba_prefix/include/mgba" ]; then
    echo "error: libmgba not found — install it (macOS: brew install mgba; Debian/Ubuntu:" >&2
    echo "       apt-get install libmgba-dev) or set MGBA_PREFIX=<prefix>." >&2
    return 1
  fi
  echo "$mgba_prefix"
}

# Build the headless renderer if missing/stale. A non-standard prefix (Homebrew) needs -I/-L + rpath;
# a system prefix (/usr) is already on the compiler's default search paths.
# Usage: ensure_gba_shot <repo-root> <mgba-prefix>; prints the tool path on stdout.
ensure_gba_shot() {
  local here="$1" mgba_prefix="$2"
  local shot="$here/tools/gba-shot"
  if [ ! -x "$shot" ] || [ "$here/tools/gba-shot.c" -nt "$shot" ]; then
    echo "compiling tools/gba-shot (against libmgba at $mgba_prefix) ..." >&2
    # -lm for the audio capture's RMS; a no-op on macOS (libm is in libSystem), required on Linux.
    local cflags=() ldflags=(-lmgba -lm)
    if [ "$mgba_prefix" != "/usr" ]; then
      cflags=(-I"$mgba_prefix/include")
      ldflags=(-L"$mgba_prefix/lib" -lmgba -lm -Wl,-rpath,"$mgba_prefix/lib")
    fi
    cc "$here/tools/gba-shot.c" -o "$shot" "${cflags[@]}" "${ldflags[@]}"
  fi
  echo "$shot"
}

# Turn the script's input into a ROM path, building it first when given a .tish source.
# Usage: resolve_rom <input>; prints the ROM path on stdout.
resolve_rom() {
  local input="$1" rom="$1" tish
  if [[ "$input" == *.tish ]]; then
    rom="${input%.tish}.gba"
    tish="${TISH:-$(command -v tish || true)}"
    [ -n "$tish" ] || { echo "error: building a .tish needs 'tish' on PATH or TISH=..." >&2; return 1; }
    echo "building $input -> $rom" >&2
    "$tish" build "$input" --target gba -o "$rom"
  fi
  [ -f "$rom" ] || { echo "error: ROM not found: $rom" >&2; return 1; }
  echo "$rom"
}
