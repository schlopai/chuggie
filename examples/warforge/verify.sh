#!/usr/bin/env bash
# warforge verify — the acceptance test for the RTS campaign.
#
# It drives a real mission rather than just booting the ROM: select the army, walk the cursor to the
# enemy camp, order the attack, and confirm the campaign ADVANCES. Booting is not evidence.
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

check "assets present" bash -c 'test -f assets/wf_tiles.png && test -f src/mapdata.tish &&
  test -f src/terrain.tish && test -f assets/u_footman.png'

check "unit sheets fit one palette bank each" python3 - <<'PY'
import glob, sys
from PIL import Image
# A sprite sheet gets ONE Palette16, and sixteen banks is a hard GBA ceiling that panics inside agb
# on an innocent frame. Cheaper to assert here than to discover in play.
worst = 0
bad = []
for f in sorted(glob.glob("assets/u_*.png")):
    im = Image.open(f).convert("RGBA")
    n = len({p[:3] for p in im.getdata() if p[3] > 0})
    worst = max(worst, n)
    if n > 15:
        bad.append((f, n))
print(f"     worst unit sheet uses {worst} colours of 15")
sys.exit(1 if bad else 0)
PY

check "terrain arrays match the declared map sizes" python3 - <<'PY'
import re, sys
src = open("src/terrain.tish").read()
meta = open("src/mapdata.tish").read()
ok = True
for key in ("M1", "M2", "M3"):
    w = int(re.search(key + r"_W: i32 = (\d+)", meta).group(1))
    h = int(re.search(key + r"_H: i32 = (\d+)", meta).group(1))
    for suffix in ("G", "S"):
        body = re.search(key + "_" + suffix + r": i32\[\] = \[([^\]]*)\]", src).group(1)
        n = len(body.split(","))
        if n != w * h:
            print(f"     {key}_{suffix}: {n} cells, expected {w * h}")
            ok = False
print("     terrain arrays sized to their maps")
sys.exit(0 if ok else 1)
PY

unset CARGO_TARGET_DIR
# Deliberately NOT TISH_FAST_NATIVE_BUILD: it exits 0 on a failed GBA compile and leaves the
# previous .gba, so every check below would run against a stale ROM.
check "builds" npm run build

RS=".tish/gba/warforge/src/main.rs"
check "generated Rust exists" test -f "$RS"

check "no soft-float scalars on the hot path" python3 - <<'PY'
import pathlib, re, sys
p = pathlib.Path(".tish/gba/warforge/src/main.rs")
if not p.exists():
    sys.exit(1)
# Every `G_*.with` is a thread-local Cell<f64> read — an untyped scalar (perf-rules §1).
#
# `G_P` is exempt by name, not by accident: it is `packages/prefs.tish`'s own state record, and
# prefs is touched exactly twice in this game — once at boot and once when a mission ends. Exempting
# it keeps the check meaningful for everything that IS in the loop; raising the threshold to "43 is
# fine" would not.
hits = re.findall(r"G_([A-Za-z0-9_]*)\.with\(", p.read_text())
hot = [h for h in hits if h != "P"]
print(f"     G_*.with = {len(hits)} total, {len(hot)} outside packages/prefs")
sys.exit(0 if not hot else 1)
PY

# Drive mission 1 to a win.
#
# ⚠️ Each tap needs an explicit RELEASE after it. The schedule sets a HELD mask, so two consecutive
# `right` entries are one continuous press and `keys_edge` fires exactly ONCE — 26 taps became one
# and the cursor never moved. A schedule that "does not work" is usually the schedule.
SCHED="$(python3 - <<'SCHEDPY'
# select the army -> walk the cursor onto the enemy camp -> A orders the march.
#
# The d-pad always drives the cursor and never the menu, so there is no focus step here: SELECT
# takes the army, the arrows move, A gives the order. L/R would step the command card and are not
# needed for a plain attack-move.
#
# Distances are in HALF CELLS: the cursor moves 8px a press, not a whole 16px terrain cell. It
# starts one row below the town hall at cell (4, 11); the camp is at cell (34, 15). So 30 cells east
# and 6 south = 60 and 12 presses.
#
# This schedule has been invalidated three times by interface changes — selection capturing the
# d-pad, the cursor moving to 8px, and now the d-pad becoming unconditional. Every time the symptom
# was the same: the army marches to open ground short of the camp and the mission never ends, which
# looks exactly like the game breaking. A key schedule is part of the interface it drives.
s = ["60:select", "64:"]
f = 100
for _ in range(60):
    s += ["%d:right" % f, "%d:" % (f + 3)]
    f += 7
for _ in range(12):
    s += ["%d:down" % f, "%d:" % (f + 3)]
    f += 7
s += ["%d:a" % (f + 10), "%d:" % (f + 14)]
print(",".join(s))
SCHEDPY
)"

LOG=/tmp/warforge-verify.log
rm -f warforge.sav   # a fresh campaign: the ROM otherwise resumes from SRAM
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" warforge.gba /tmp/warforge-verify.png 6000 "$SCHED" 2>&1 \
  | grep -aE "P[0-9]+ W|panic|Bad memory|not implemented" >"$LOG" || true
rm -f warforge.sav

# Filter at the source, above: a crashed ROM writes hundreds of MB of SWI traces, and a
# panic-only grep over an unfiltered log is a false pass waiting to happen.
check "no panic in a 6000-frame run" bash -c '! grep -qiE "panic|Bad memory|not implemented" "$1"' _ "$LOG"

# The campaign must ADVANCE, not merely boot. Reaching M1 means mission 1 was won and mission 2
# loaded, with the hero carried across through SRAM.
check "campaign advances past mission 1" bash -c 'grep -qE " M1 " "$1"' _ "$LOG"

# Combat resolves with nothing ordered to attack: `set_soldier` is what fights.
check "enemies die to attack-move" bash -c 'grep -qE " F0 | F1 " "$1"' _ "$LOG"

check "HUD repaints only on change" python3 - "$LOG" <<'PY'
import re, sys
h = [int(m) for m in re.findall(r"\bH(\d+)", open(sys.argv[1]).read())]
print(f"     HUD repaints = {h[-1] if h else 'none'}")
# A panel driven by a signature paints a handful of times; anything near the frame count is a HUD
# being rebuilt continuously.
sys.exit(0 if h and h[-1] <= 40 else 1)
PY

check "frame EMA stays inside the 4389-tick budget" python3 - "$LOG" <<'PY'
import re, sys
# The SETTLED EMA (the last sample). frame_period(1) is a peak-since-reset and would be pinned
# forever by one mission-load frame; frame_period(2) answers "is this slow".
ema = [int(m) for m in re.findall(r"\bE(\d+)", open(sys.argv[1]).read())]
if not ema:
    print("     no EMA samples in the log")
    sys.exit(1)
print(f"     EMA {ema[-1]} / 4389 budget  (samples {len(ema)})")
sys.exit(0 if ema[-1] <= 4389 * 102 // 100 else 1)
PY

if [[ $FAILED -eq 0 ]]; then echo "warforge verify ok"; else echo "warforge verify FAILED"; fi
exit $FAILED
