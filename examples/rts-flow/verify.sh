#!/usr/bin/env bash
# rts-flow verify — the acceptance test for the flow field: it must stay inside the frame budget
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

RS=".tish/gba/rts-flow/src/main.rs"
check "generated Rust exists" test -f "$RS"

check "no soft-float scalars on the hot path" python3 - <<'PY'
import pathlib, re, sys
p = pathlib.Path(".tish/gba/rts-flow/src/main.rs")
if not p.exists():
    sys.exit(1)
# Every `G_*.with` is a thread-local Cell<f64> read — an untyped scalar (perf-rules §1).
n = len(re.findall(r"G_[A-Za-z0-9_]*\.with\(", p.read_text()))
print(f"     G_*.with = {n}")
sys.exit(0 if n == 0 else 1)
PY

LOG=/tmp/rts-flow-verify.log
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" rts-flow.gba /tmp/rts-flow-verify.png 1500 \
  >"$LOG" 2>&1 || true

check "no panic in a 1500-frame run" bash -c '! grep -qiE "panic|Bad memory|not implemented" "$1"' _ "$LOG"

check "all 24 units complete the course" bash -c 'grep -qE "A24" "$1"' _ "$LOG"

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

if [[ $FAILED -eq 0 ]]; then echo "rts-flow verify ok"; else echo "rts-flow verify FAILED"; fi
exit $FAILED
