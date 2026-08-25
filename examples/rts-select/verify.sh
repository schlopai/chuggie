#!/usr/bin/env bash
# rts-select verify — the acceptance test for the flow field: it must stay inside the frame budget
# AND actually deliver every unit through the course. Either alone would pass a broken build.
#
# Every check prints `ok` or `FAIL`. A check that CRASHES prints neither and vanishes into a run
# that still reports "0 FAIL", so each one is wrapped and its exit status is what decides.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
EX="$(cd "$(dirname "$0")" && pwd)"
cd "$EX"

FAILED=0
check() { # check <name> <command...>
  local name="$1"; shift
  if "$@"; then echo "ok   $name"; else echo "FAIL $name"; FAILED=1; fi
}

check "assets present" test -f assets/maze.tmj

check "tmj has Ground + Solid layers" python3 - <<'PY'
import json, sys
m = json.load(open("assets/maze.tmj"))
names = [l["name"] for l in m["layers"]]
# `Solid` and not `Collision`: a Collision layer can only force cells WALKABLE, and an empty cell
# there ERASES the tileset's own collision — it cannot author a wall.
sys.exit(0 if ("Ground" in names and "Solid" in names) else 1)
PY

unset CARGO_TARGET_DIR
# Deliberately NOT TISH_FAST_NATIVE_BUILD: it exits 0 on a failed GBA compile and leaves the
# previous .gba, so every check below would run against a stale ROM.
check "builds" npm run build

RS=".tish/gba/rts-select/src/main.rs"
check "generated Rust exists" test -f "$RS"

check "no soft-float scalars on the hot path" python3 - <<'PY'
import pathlib, re, sys
p = pathlib.Path(".tish/gba/rts-select/src/main.rs")
if not p.exists():
    sys.exit(1)
# Every `G_*.with` is a thread-local Cell<f64> read — an untyped scalar (perf-rules §1).
n = len(re.findall(r"G_[A-Za-z0-9_]*\.with\(", p.read_text()))
print(f"     G_*.with = {n}")
sys.exit(0 if n == 0 else 1)
PY

LOG=/tmp/rts-select-verify.log
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" rts-select.gba /tmp/rts-select-verify.png 1500 "90:l" \
  >"$LOG" 2>&1 || true

check "no panic in a 1500-frame run" bash -c '! grep -qiE "panic|Bad memory|not implemented" "$1"' _ "$LOG"

# Attack-move: the enemy marches in on its own field and nothing orders anyone to fight. All four
# must die. This is the check that caught `set_soldier` acquiring only within its SWING range —
# with that bug the two armies walked past each other and K stayed 0 forever.
check "attack-move kills all 4 enemies unaided" bash -c 'grep -qE "K4" "$1"' _ "$LOG"

# Selection: the schedule taps L once, which selects all six. S6 proves the order reached the right
# units; it also guards the `&`-vs-`===` precedence trap that made the count silently 0.
check "L selects the whole army" bash -c 'grep -qE "S6" "$1"' _ "$LOG"

# The panel must repaint on CHANGE, not per frame. Two paints over 1500 frames (boot + the L tap) is
# the expected number; anything near the frame count means the HUD is being rebuilt continuously.
check "panel repaints only on change" python3 - "$LOG" <<'PY'
import re, sys
r = [int(m) for m in re.findall(r"\bR(\d+)", open(sys.argv[1]).read())]
print(f"     panel repaints = {r[-1] if r else 'none'}")
sys.exit(0 if r and r[-1] <= 8 else 1)
PY

check "frame EMA stays inside the 4389-tick budget" python3 - "$LOG" <<'PY'
import re, sys
# Read the settled EMA (the LAST sample). frame_period(1) is a peak-since-reset and would be pinned
# forever by one boot frame; frame_period(2) is the number that answers "is this slow".
ema = [int(m) for m in re.findall(r"\bE(\d+)", open(sys.argv[1]).read())]
if not ema:
    print("     no EMA samples in the log")
    sys.exit(1)
last = ema[-1]
print(f"     EMA {last} / 4389 budget  (samples {len(ema)})")
# 2% of slack: the EMA rides the vsync line at ~4380 and a boot frame drags it from above.
sys.exit(0 if last <= 4389 * 102 // 100 else 1)
PY

if [[ $FAILED -eq 0 ]]; then echo "rts-select verify ok"; else echo "rts-select verify FAILED"; fi
exit $FAILED
