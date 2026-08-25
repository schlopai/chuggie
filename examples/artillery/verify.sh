#!/usr/bin/env bash
# artillery verify — the acceptance test for N-body gravity integrated in pure integer tish.
#
# Three claims, and each one is easy to fake, so each has a control that would catch the fake:
#
#   1. THE ARC IS BENT BY THE PLANETS. Faked by an integrator that ignores gravity — which would pass
#      the determinism check perfectly, because a straight line is extremely reproducible. Caught by
#      measuring the integrated turning of a real shot trace.
#   2. IT IS COMPUTED IN INTEGERS. Faked by source that LOOKS integral. On this chip `dx * dx`
#      between two i32 locals is an f64 multiply and a mis-sized array turns a masked index into an
#      f64 bounds-check fallback, so the only honest place to check is the GENERATED RUST.
#   3. IT IS REPRODUCIBLE. Faked by a run too short to diverge. Caught by requiring a real trace.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
. ../../scripts/verify_common.sh
unset CARGO_TARGET_DIR

GEN=.tish/gba/artillery/src/main.rs
fails=0
ran=0
EXPECT=26
finished=0

# ⚠️ A CHECK THAT CRASHES PRINTS NEITHER ok NOR FAIL, so a suite that only counts FAILs reports a
# clean run for a script that died a third of the way in. Two guards, because they catch different
# things: this trap catches a script that DIED (an unset var under -u, a syntax error, a heredoc that
# took the shell with it), and the counter below catches one that SURVIVED but skipped a check.
trap '[ "$finished" = 1 ] || { echo; echo "artillery: ABORTED — died before its last check"; }' EXIT

check() {  # message, then $? already captured by the caller
  ran=$((ran + 1))
  if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fails=$((fails + 1)); fi
}

echo "== build =="
python3 ../../scripts/gen_artillery_tables.py >/dev/null 2>&1 && python3 ../../scripts/gen_artillery.py >/dev/null 2>&1
check $? "regenerates the lookup tables and both sprite sheets"

npm run build >/tmp/art-verify-build.log 2>&1
check $? "builds"
if [ "$fails" != 0 ]; then tail -25 /tmp/art-verify-build.log; finished=1; exit 1; fi

assert_agb_fork .
check $? "resolved agb to the fork"

assert_typed_scalars src
check $? "no untyped module scalars"

# ── The typed-lowering gates. THIS IS CLAIM 2, AND IT IS CHECKED IN THE GENERATED RUST. ──────────
echo
echo "== integer arithmetic, in the compiled output =="

# `dx * dx` between two i32 LOCALS compiles to `((dx) as f64) * ((dx) as f64)` — verified in
# examples/soccer's generated Rust. Every multiply on the frame path must be Math.imul or an SQ read.
n=$(grep -c "as f64) \* ((" "$GEN" || true)
[ "${n:-1}" = 0 ]
check $? "no f64 multiplies anywhere ($n)"

# The f64 bounds-check fallback the compiler emits for any index it cannot reduce to a mask: 25 ticks
# against 0.45 for the identical array, and its f64 type is CONTAGIOUS through the surrounding
# expression. Two hits are the runtime's own `NaN` global, which is not indexing anything.
n=$(grep -c "f64::NAN" "$GEN" || true)
[ "${n:-9}" -le 2 ]
check $? "no array falls back to the f64 bounds-checked path ($n, 2 are the runtime's NaN global)"

# An integer expression that leaves the integer domain and is converted back. This is what a stray
# `+` around a shift, or one poisoned array read, actually looks like once compiled.
n=$(grep -c "to_int_unchecked" "$GEN" || true)
[ "${n:-1}" = 0 ]
check $? "no f64 round-trips on any expression ($n)"

# An untyped module scalar is a thread-local Cell<f64>, so every use is three soft-float ops.
n=$(grep -c "G_[A-Za-z_]*\.with" "$GEN" || true)
[ "${n:-1}" = 0 ]
check $? "no module scalar is soft-float ($n)"

# The tables must be PROMOTED (a ROM load), not a heap Vec built at boot.
grep -q "const G_GACC: \[i32" "$GEN" && grep -q "const G_SQ: \[i32" "$GEN"
check $? "the gravity and square tables are promoted to ROM constants"

# ── Boot self-test, by exact value ────────────────────────────────────────────────────────────────
echo
echo "== the physics, by value =="
log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh artillery.gba /tmp/art-verify.png 12000 >/dev/null 2>"$log"
check $? "runs 12000 frames headless"

crash_grep "$log"
check $? "no panic, no allocation failure"

# A canned 240-tick flight over a canned arena, checksummed over the whole PATH rather than the
# endpoint — an integrator that arrives in the right place by a different route has still changed.
# ⚠️ If this number moves, the physics moved. That is the point; update it deliberately, never
# reflexively, and say in the commit what changed.
grep -q "ART SELFTEST traj=-715313308 k=167 st=3 x=481 y=-33" "$log"
check $? "the canned trajectory checksums to its known value"

# ── CLAIM 1, AND ITS NEGATIVE CONTROL ────────────────────────────────────────────────────────────
# Without this, an integrator with gravity deleted passes every other check in this file.
python3 - "$log" <<'PYEOF'
import re, sys, math
from collections import defaultdict
tr = defaultdict(list)
for line in open(sys.argv[1], errors="replace"):
    m = re.search(r"ART SHELL id=q(\d+)k(\d+) x=(-?\d+) y=(-?\d+)", line)
    if m:
        tr[int(m.group(1))].append((int(m.group(3)), int(m.group(4))))
best, seq = 0.0, -1
for q, pts in tr.items():
    if len(pts) < 4:
        continue
    turn = 0.0
    for i in range(len(pts) - 2):
        a = math.atan2(pts[i + 1][1] - pts[i][1], pts[i + 1][0] - pts[i][0])
        b = math.atan2(pts[i + 2][1] - pts[i + 1][1], pts[i + 2][0] - pts[i + 1][0])
        turn += abs((b - a + math.pi) % (2 * math.pi) - math.pi)
    if math.degrees(turn) > best:
        best, seq = math.degrees(turn), q
if not tr:
    print("  (no shell traces at all)")
    sys.exit(1)
if best < 25:
    print(f"  (worst-bent shot turned only {best:.1f} deg — the wells are doing nothing)")
    sys.exit(1)
print(f"  (shot q{seq} turned {best:.0f} deg through the wells)")
PYEOF
check $? "shots CURVE — they are not reproducible straight lines"

# ── The match actually plays ─────────────────────────────────────────────────────────────────────
echo
echo "== a match plays itself =="
shots=$(grep -ac "ART SHOT" "$log" || true)
[ "${shots:-0}" -ge 12 ]
check $? "shots are fired ($shots)"

turns=$(grep -ac "ART TURN" "$log" || true)
[ "${turns:-0}" -ge 10 ]
check $? "turns alternate ($turns)"

# ⚠️ NEGATIVE CONTROL for the turn counter: a counter that incremented without swapping sides would
# still emit alternating TURN lines, and every shot would come from one player.
s0=$(grep -ac "ART SHOT .*side=0" "$log" || true)
s1=$(grep -ac "ART SHOT .*side=1" "$log" || true)
[ "${s0:-0}" -ge 4 ] && [ "${s1:-0}" -ge 4 ]
check $? "BOTH sides fire (p0 $s0, p1 $s1) — the turn actually changes hands"

hits=$(grep -a "ART BOOM" "$log" | grep -avc "d0=0 d1=0" || true)
[ "${hits:-0}" -ge 2 ]
check $? "shells land damage ($hits blasts hurt somebody)"

# ⚠️ FALLOFF IS ASSERTED AT BOOT, BY VALUE, NOT FROM GAMEPLAY.
#
# The first version of this check counted distinct damage numbers in the match log, and it FAILED
# while the falloff was perfectly correct: the attract driver hill-climbs onto one good shot and
# then fires it byte-identically for ever, so every blast it lands deals the same damage. That is
# the driver converging, not the arithmetic being flat — and a check that a correct implementation
# cannot pass is worse than no check. The ROM now runs the real falloff arithmetic over a ladder of
# distances at boot and prints it, so this asserts the curve itself.
dmgs=$(grep -ao "ART SELFTEST blast d=[0-9]* dmg=[0-9]*" "$log" | grep -o "dmg=[0-9]*" | cut -d= -f2 | sort -un | wc -l | tr -d ' ')
[ "${dmgs:-0}" -ge 6 ]
check $? "blast damage falls off with distance ($dmgs distinct values over the radius)"

# ...and by exact value at both ends, so a falloff that is merely SOME curve is not enough.
grep -q "ART SELFTEST blast d=0 dmg=39" "$log" && grep -q "ART SELFTEST blast d=24 dmg=0" "$log"
check $? "falloff is 39 at the centre and 0 at the rim"

wins=$(grep -ac "ART RESULT" "$log" || true)
[ "${wins:-0}" -ge 1 ]
check $? "a match reaches a verdict ($wins)"

# ⚠️ NEGATIVE CONTROL for the arena: a buildArena that ignored its seed would pass every determinism
# check in this file. It must VARY between matches...
arenas=$(grep -ao "ART ARENA .*" "$log" | grep -o "p=[0-9-]*" | sort -u | wc -l | tr -d ' ')
[ "${arenas:-0}" -ge 2 ]
check $? "arenas differ between matches ($arenas distinct layouts)"

# Heap flat: everything is created once at boot, and a rematch rebuilds the arena in place.
lo=$(grep -ao "HEAP [0-9]*" "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | head -1)
hi=$(grep -ao "HEAP [0-9]*" "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | tail -1)
[ -n "${lo:-}" ] && [ $(( hi - lo )) -le 8192 ]
check $? "heap bounded across matches (span $(( ${hi:-0} - ${lo:-0} )) B)"

# ── CLAIM 3: replay determinism, and the control that it measured something ──────────────────────
echo
echo "== replay =="
for n in 1 2; do
  GBA_SHOT_LOG=1 ../../scripts/screenshot.sh artillery.gba "/tmp/art-rep$n.png" 1500 >/dev/null 2>"/tmp/art-rep$n.log"
  grep -ao "ART SHELL .*" "/tmp/art-rep$n.log" > "/tmp/art-rep$n.trace"
done
lines=$(wc -l < /tmp/art-rep1.trace | tr -d ' ')
# ...and the control: two EMPTY traces are also identical.
[ "${lines:-0}" -ge 20 ]
check $? "the replay run produced a real trace ($lines samples)"

diff -q /tmp/art-rep1.trace /tmp/art-rep2.trace >/dev/null
check $? "the same ROM and inputs give a bit-identical shell trace"

# ── Frame budget ─────────────────────────────────────────────────────────────────────────────────
echo
echo "== budget =="
# The SUSTAINED cost. The peak is a separate matter and is documented rather than gated: hud_text
# reallocates sprite VRAM when its string changes, which costs ~6,000 ticks on the one frame a turn
# changes hands. See README.
avg=$(grep -ao "ART TICKS .* avg=[0-9]*" "$log" | grep -o "avg=[0-9]*" | cut -d= -f2 | sort -n | tail -1)
[ "${avg:-9999}" -lt 1200 ]
check $? "sustained frame cost ${avg:-?} of 4389 ticks"

# The number this spike exists to produce, reported as ticks x100 per substep.
per=$(grep -ao "per100=[0-9]*" "$log" | cut -d= -f2 | sort -n | tail -1)
[ "${per:-99999}" -lt 6000 ]
check $? "N-body substep costs $(( ${per:-0} / 100 )) ticks with 3 planets (incl. instrumentation)"

echo
if [ "$ran" -eq "$EXPECT" ]; then
  echo "ok   all $EXPECT checks ran"
else
  echo "FAIL only $ran of $EXPECT checks ran — one died without printing ok or FAIL"
  fails=$((fails + 1))
fi

finished=1
[ "$fails" = 0 ] && echo "artillery: PASS" || echo "artillery: $fails FAILED"
exit $([ "$fails" = 0 ] && echo 0 || echo 1)
