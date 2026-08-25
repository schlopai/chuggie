#!/usr/bin/env bash
# Run two GBA cores with their link ports wired together, and print both consoles' output.
#
# The companion to scripts/screenshot.sh: that one runs a single core, which means a link game
# always sees an empty cable and every headless test exercises only its offline path. This one uses
# mGBA's own SIO lockstep to model a cable between two cores in one process, so the transport —
# register writes, master/child split, transfer handshake — is actually executed.
#
#   scripts/link.sh <rom0> [rom1] [frames] [keys0] [keys1]
#
# rom1 defaults to rom0 (two copies of the same cartridge, which is the normal case). Keys use the
# same syntax as screenshot.sh. Output goes to stderr, each line prefixed with the console id.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

rom0="${1:?usage: link.sh <rom0> [rom1] [frames] [keys0] [keys1]}"
rom1="${2:-$rom0}"
frames="${3:-600}"
keys0="${4:-}"
keys1="${5:-}"

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
  echo "error: libmgba headers not found (set MGBA_PREFIX)" >&2
  exit 1
fi

link="$here/tools/gba-link"
if [ ! -x "$link" ] || [ "$here/tools/gba-link.c" -nt "$link" ]; then
  echo "compiling tools/gba-link (against libmgba at $mgba_prefix) ..." >&2
  cc "$here/tools/gba-link.c" -o "$link" \
    -I"$mgba_prefix/include" -L"$mgba_prefix/lib" -lmgba
fi

GBA_LINK_LOG=1 "$link" "$rom0" "$rom1" /tmp/gba-link-0.ppm /tmp/gba-link-1.ppm \
  "$frames" "$keys0" "$keys1"
