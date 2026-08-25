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

# ⚠️ WATCHDOG. A lockstep pair can deadlock: one console panics mid-transfer and the other waits
# on SIO forever while its sibling's frame counter runs away — gba-link's frame cap never trips
# because the stalled console never reaches it. Measured once at 650,000 frames on a 5,000-frame
# run before anyone looked. LINK_TIMEOUT seconds (default 1800) is well past any honest run
# (a loaded machine measured ~15 min for 2,500 heavy frames).
GBA_LINK_LOG=1 "$link" "$rom0" "$rom1" /tmp/gba-link-0.ppm /tmp/gba-link-1.ppm \
  "$frames" "$keys0" "$keys1" &
link_pid=$!
( sleep "${LINK_TIMEOUT:-1800}"; kill "$link_pid" 2>/dev/null ) &
watchdog=$!
# ⚠️ Every step here runs under `set -e`, and each can legitimately "fail" (wait on a killed
# process, kill on an already-finished watchdog) — none of that is the script's exit status.
rc=0; wait "$link_pid" || rc=$?
kill "$watchdog" 2>/dev/null || true
wait "$watchdog" 2>/dev/null || true
if [ "$rc" -ge 128 ]; then
  echo "gba-link: killed by the ${LINK_TIMEOUT:-1800}s watchdog — the pair deadlocked" >&2
fi
exit "$rc"
