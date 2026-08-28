#!/usr/bin/env bash
# Baseline verification for an example that has no verify.sh of its own.
#
# "Every example that is not a bench or a repro must be verified." 46 of the 90 game examples had
# nothing checking them at all: they were compiled in CI and never run, so a ROM that built and then
# crashed on boot, or drew a blank screen, shipped green.
#
# This is deliberately the WEAKEST useful check, because it has to hold for every genre with no
# knowledge of any of them:
#
#   1. the ROM builds                  (the caller has already done this)
#   2. it resolved agb to our fork     (not crates.io — see check_agb_fork.sh)
#   3. it boots and runs without a crash string or an allocator halt
#   4. it actually DRAWS something     (a frame with more than a flat fill on it)
#
# An example with real assertions should have its own verify.sh; this is the floor, not a substitute.
# Where a game needs input before it draws, give it a key schedule as the second argument.
set -uo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
. "$root/scripts/verify_common.sh"

ex="${1:?usage: verify_default.sh <example-name> [frames] [key-schedule]}"
frames="${2:-300}"
keys="${3:-}"
dir="$root/examples/$ex"
[ -d "$dir" ] || { echo "FAIL: no such example: $ex"; exit 1; }

fail=0
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== $ex (baseline) =="

rom="$(find "$dir" -maxdepth 1 -name '*.gba' | head -1)"
[ -n "$rom" ]
check $? "a ROM was built"
[ -n "$rom" ] || { echo "$ex: FAIL"; exit 1; }

assert_agb_fork "$dir" >/dev/null 2>&1
check $? "resolved agb to the fork"

log="$(mktemp)"
GBA_SHOT_LOG=1 "$root/tools/gba-shot" "$rom" "$log.ppm" "$frames" "$keys" >"$log" 2>&1
check $? "runs $frames frames headless"

crash_grep "$log"
check $? "no crash, no allocation failure"

# An allocator failure HALTS without logging, so the log cannot show it — a repeating SWI 02 can.
! grep -qE 'SWI: 02' "$log"
check $? "no halt loop"

# Does it draw? A frame that is one flat colour is a ROM that booted into nothing. The bar is the
# same one scripts/best_still.py uses: more than a flat fill, not a colour count, because plenty of
# these legitimately draw a sprite on an empty backdrop.
python3 - "$log.ppm" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
counts = sorted((n for n, _ in im.getcolors(1 << 16)), reverse=True)
total = im.width * im.height
sys.exit(0 if len(counts) >= 2 and counts[0] / total < 0.999 else 1)
PY
check $? "the final frame draws something"

rm -f "$log" "$log.ppm"
[ "$fail" = 0 ] && echo "$ex: PASS" || echo "$ex: FAIL"
exit "$fail"
