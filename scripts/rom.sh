#!/usr/bin/env bash
# Build, play, screenshot or record one example's ROM. Backs the editor actions in .vscode/ (regenerate those
# with `npm run vscode` after adding an example) and is fine to call by hand.
#
# Usage:  scripts/rom.sh play|build|shot|gif|rom [example]
#   play   : run the ROM that is ON DISK in mGBA. Does NOT build — so it starts instantly, and says
#            what to run if the ROM is missing or older than its sources.
#   build  : `npm run build` in the example.
#   shot   : headless screenshot -> examples/<name>/screenshot.png (see scripts/screenshot.sh).
#   gif    : headless animated clip -> examples/<name>/screenshot.gif (see scripts/gif.sh).
#   rom    : print the ROM path and exit (for scripting).
#
#   [example] may be a name (akari), a directory (examples/akari) or ANY path inside one
#   (examples/akari/src/main.tish) so an editor can pass the active file. Omitted, or a path outside
#   examples/ (editing packages/ui.tish), falls back to the last example this script resolved.
#
#   env: MGBA=<binary> to pick an emulator explicitly; MGBA_ARGS="--scale 4 -b bios.bin" for extra
#        emulator flags. Play always forces a windowed emulator (see below).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"          # repo root
action="${1:?usage: rom.sh play|build|shot|gif|rom [example]}"
arg="${2:-}"
memo="$here/target/.last-example"

# ── resolve the example ───────────────────────────────────────────────────────────────────────────
# Accept anything that names an example: strip a leading repo path, then keep the component after
# "examples/" (or the whole thing, if it is already a bare name).
name="${arg#"$here"/}"
name="${name#./}"
case "$name" in
  examples/*) name="${name#examples/}"; name="${name%%/*}" ;;
  */*)        name="" ;;                                    # a path, but not inside examples/
esac
if [ -z "$name" ] || [ ! -d "$here/examples/$name" ]; then
  remembered="$([ -f "$memo" ] && cat "$memo" || true)"
  if [ -n "$remembered" ] && [ -d "$here/examples/$remembered" ]; then
    [ -n "$name" ] && echo "note: '$name' is not an example — using last one ($remembered)." >&2
    name="$remembered"
  else
    echo "error: no example given and none remembered. Pass a name, e.g. rom.sh $action akari." >&2
    echo "       Available: $(cd "$here/examples" && ls -d */ | tr -d / | tr '\n' ' ')" >&2
    exit 1
  fi
fi
dir="$here/examples/$name"
mkdir -p "$here/target" && printf '%s' "$name" > "$memo"

# The build writes "$npm_package_name.gba", which is the package name — NOT always the directory name
# (examples/minimal builds tish-agb-minimal.gba).
pkg="$(node -p "require('$dir/package.json').name" 2>/dev/null || echo "$name")"
rom="$dir/$pkg.gba"

# tish looks for the built ELF under the example's own .tish/gba/<pkg>/target, so an inherited
# CARGO_TARGET_DIR (a global one in a shell profile, a sandbox, CI) sends cargo's output elsewhere and
# every build dies with "GBA ELF not found".
unset CARGO_TARGET_DIR

case "$action" in
  rom)
    echo "$rom"
    ;;

  build)
    echo "building $name ..." >&2
    cd "$dir" && exec npm run build
    ;;

  shot)
    cd "$dir" && exec npm run shot
    ;;

  gif)
    cd "$dir" && exec npm run gif
    ;;

  play)
    if [ ! -f "$rom" ]; then
      echo "error: no ROM at $rom — run the 'Build: $name' action first (or: npm run build -w $pkg)." >&2
      exit 1
    fi
    # Playing deliberately never builds, so say it when the ROM predates the sources rather than
    # letting a stale ROM look like a change that did not take.
    if [ -n "$(find "$dir/src" "$here/packages" -newer "$rom" -name '*.tish' -print -quit 2>/dev/null)" ]; then
      echo "note: $pkg.gba is older than some .tish sources — playing it anyway (build to refresh)." >&2
    fi
    emu="${MGBA:-}"
    if [ -z "$emu" ]; then
      for c in mgba-qt mgba; do
        command -v "$c" >/dev/null 2>&1 && { emu="$c"; break; }
      done
    fi
    if [ -z "$emu" ]; then
      if [ -x /Applications/mGBA.app/Contents/MacOS/mGBA ]; then
        emu=/Applications/mGBA.app/Contents/MacOS/mGBA
      else
        echo "error: no mGBA found — install it (macOS: brew install mgba) or set MGBA=<binary>." >&2
        exit 1
      fi
    fi
    echo "playing $pkg.gba in $emu" >&2
    # mGBA remembers fullscreen in its own config and re-enters it for every ROM once you have toggled
    # it — and macOS gives a fullscreen window its own Space, which hides your editor for what is
    # supposed to be a few seconds of play-testing. Override it per launch so the saved value can't come
    # back. Window size stays whatever you last dragged it to; MGBA_ARGS is there for the rest.
    if [ -n "${MGBA_ARGS:-}" ]; then
      # word-splitting MGBA_ARGS is the point — it is a list of emulator flags.
      exec "$emu" -C fullscreen=0 $MGBA_ARGS "$rom"
    fi
    exec "$emu" -C fullscreen=0 "$rom"
    ;;

  *)
    echo "error: unknown action '$action' (want play|build|shot|rom)." >&2
    exit 1
    ;;
esac
