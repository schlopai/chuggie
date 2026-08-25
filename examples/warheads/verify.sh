#!/usr/bin/env bash
# Verify warheads: one console, two consoles, and — the point of the link half — that the two are
# running the SAME match.
#
# ⚠️ THIS SUITE IS DELIBERATELY NOT ORIGINAL. examples/pong-link already worked out how to test a
# lockstep game on this hardware, including the traps: `grep -q` under `pipefail` failing BECAUSE it
# matched, and a differ that reports agreement between two empty logs. Its structure, its differ and
# its ready-gate are reused here almost verbatim, and this ROM prints `SYNC f=<n> …` in pong-link's
# exact format so the same comparison works without adapting it.
#
# A desynced lockstep game is the hardest kind of bug to see: each console shows a completely
# plausible artillery duel, they are simply not the same duel. No screenshot of one unit can catch it.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
. ../../scripts/verify_common.sh
root="$(cd ../.. && pwd)"
unset CARGO_TARGET_DIR

GEN=.tish/gba/warheads/src/main.rs
fails=0
ran=0
EXPECT=32
finished=0

# ⚠️ A CHECK THAT CRASHES PRINTS NEITHER ok NOR FAIL, so a suite counting only FAILs reports a clean
# run for a script that died a third of the way in. The trap catches a script that DIED; the counter
# at the bottom catches one that SURVIVED but skipped a check.
trap '[ "$finished" = 1 ] || { echo; echo "warheads: ABORTED — died before its last check"; }' EXIT

check() {   # exit status, message
  ran=$((ran + 1))
  if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fails=$((fails + 1)); fi
}

echo "== build =="
python3 ../../scripts/gen_warheads_tables.py >/dev/null 2>&1 && python3 ../../scripts/gen_warheads.py >/dev/null 2>&1
check $? "regenerates tables and sheets"

npm run build >/tmp/warheads-build.log 2>&1
check $? "builds"
if [ "$fails" != 0 ]; then tail -25 /tmp/warheads-build.log; finished=1; exit 1; fi

assert_agb_fork . ; check $? "resolved agb to the fork"
assert_typed_scalars src ; check $? "no untyped module scalars"

# ── The typed-lowering gates, checked in the COMPILED OUTPUT ────────────────────────────────────
# On this chip `dx * dx` between two i32 locals is an f64 multiply, and an array index the compiler
# cannot reduce to a mask emits an f64 bounds-check fallback whose type poisons everything it feeds.
# Source that looks integral proves nothing; the generated Rust does.
echo
echo "== integer arithmetic, in the compiled output =="
# ⚠️ SCOPED TO THIS EXAMPLE'S OWN SYMBOLS, and that scoping is the honest part. tish inlines every
# imported package into one main.rs, so the moment warheads pulled in title/feel/chipsfx the whole-
# binary count jumped from 0 to 36 — every one of them inside a package this example only consumes,
# none touching a warheads identifier. A gate that fails on someone else's code teaches nothing and
# gets muted. MINE is the frame path: the integrator, the terrain, the hulls, the tables.
MINE='PL_|SH[XYVWKP]|S_[XYVH]|TERR|GACC|SQ\[|ISQRT'
n=$(grep "as f64) \* ((" "$GEN" | grep -cE "$MINE" || true)
[ "${n:-1}" = 0 ] ; check $? "no f64 multiplies on this game's frame path ($n)"
n=$(grep "to_int_unchecked" "$GEN" | grep -cE "$MINE" || true)
[ "${n:-1}" = 0 ] ; check $? "no f64 round-trips on this game's frame path ($n)"
n=$(grep -c "G_[A-Za-z_]*\.with" "$GEN" || true)
[ "${n:-1}" = 0 ] ; check $? "no soft-float module scalars ($n)"
# ⚠️ SCOPED LIKE THE THREE ABOVE, and it had to be. This used to count the whole binary against a
# threshold of 8, which worked only while the example imported small packages. Adopting packages/ui
# for the HUD took the whole-binary count to 179 — every one of them inside a general-purpose layout
# engine this game merely consumes, none touching a warheads identifier. A gate that fails on
# someone else's code teaches nothing and gets muted or raised until it means nothing either.
# MINE is this game's own frame path: the integrator, the terrain, the hulls, the tables.
n=$(grep "f64::NAN" "$GEN" | grep -cE "$MINE" || true)
[ "${n:-99}" = 0 ] ; check $? "no array on this game's frame path falls back to the bounds-checked path ($n)"

# ── One console ─────────────────────────────────────────────────────────────────────────────────
echo
echo "== one console =="
log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh warheads.gba /tmp/warheads-verify.png 12000 >/dev/null 2>"$log"
check $? "runs 12000 frames headless"
crash_grep "$log" ; check $? "no panic, no allocation failure"

shots=$(grep -ac "WH SHOT" "$log" || true)
[ "${shots:-0}" -ge 10 ] ; check $? "shots are fired ($shots)"
digs=$(grep -ac "WH DIG" "$log" || true)
[ "${digs:-0}" -ge 10 ] ; check $? "terrain is reshaped ($digs)"

# ⚠️ NEGATIVE CONTROL for the warhead system: one crater size for every weapon would mean the
# shot/warhead composition is not actually reaching the arrival path.
radii=$(grep -ao "WH DIG .* r=[0-9]*" "$log" | grep -o "r=[0-9]*" | sort -u | wc -l | tr -d ' ')
[ "${radii:-0}" -ge 2 ] ; check $? "different warheads dig differently ($radii distinct radii)"

# ⚠️ NEGATIVE CONTROL for the shell-theorem gravity. A shell that flies into a planet whose core has
# been blown out must still be able to reach a wall and detonate. While the integrator used each
# planet's FULL mass it was instead pulled to the dead centre and orbited there until its TTL — which
# on screen is indistinguishable from a player missing, and which no other check in this file sees.
# ⚠️ IT COUNTS ONLY SHELLS THAT EXPIRED INSIDE A PLANET. Reaching the TTL out in open space is a
# legitimate orbit — four gravity wells can produce one — and failing on that would have forced the
# gate to be loosened, which would then have hidden the real defect. `in=1` is the defect: held at a
# core that is no longer there.
stuck=$(grep -a "WH STUCK" "$log" | grep -ac "in=1" || true)
orbits=$(grep -ac "WH STUCK" "$log" || true)
[ "${stuck:-1}" = 0 ] ; check $? "no shell is ever trapped inside a planet ($stuck of $orbits TTL expiries)"

# ⚠️ THE MANOEUVRE HAS TO BE EXERCISED, AND THE MATCH HAS TO SURVIVE IT. The jump was unreachable
# in an unattended soak for the whole of this example's life, and it was broken the entire time: the
# thrust phase was gated on space mode, so a landed hop entered a phase nothing could leave and the
# game froze. Twenty-eight checks were green. A phase that only a human can reach is a phase with no
# test — so the driver takes the hop every eighth turn, and this asserts turns continue afterwards.
thrusts=$(grep -ac "WH THRUST" "$log" || true)
[ "${thrusts:-0}" -ge 2 ] ; check $? "the hull manoeuvres ($thrusts jumps)"
lastthrust=$(grep -a "WH THRUST" "$log" | tail -1 | grep -o "f=[0-9]*" | cut -d= -f2)
lastturn=$(grep -a "WH TURN" "$log" | tail -1 | grep -o "f=[0-9]*" | cut -d= -f2)
[ -n "${lastthrust:-}" ] && [ -n "${lastturn:-}" ] && [ "$lastturn" -gt "$lastthrust" ]
check $? "the match continues after a jump (last jump f=${lastthrust:-?}, last turn f=${lastturn:-?})"

hits=$(grep -a "WH BOOM" "$log" | grep -avc "d0=0 d1=0" || true)
[ "${hits:-0}" -ge 2 ] ; check $? "shells land damage ($hits)"
wins=$(grep -ac "WH RESULT" "$log" || true)
[ "${wins:-0}" -ge 1 ] ; check $? "a match reaches a verdict ($wins)"

# ⚠️ THE ARENA MUST FIT IN VIDEO RAM, and this check exists because nothing else could see it.
# Terrain is real 8x8 tiles: a planet of radius r claims ~r^2/20 of the GBA's 1024, so four large
# worlds exhaust them before the HUD canvas and starfield have had any. The solo soak was green
# throughout — the crash only ever appeared on the LINKED pair, because those two consoles dealt
# bigger planets than the single one did, and there it showed up as agb's "Ran out of video RAM for
# tiles" rather than as anything to do with the arena.
tiles=$(grep -ao "WH TILES .* est=[0-9]*" "$log" | grep -o "est=[0-9]*" | cut -d= -f2 | sort -n | tail -1)
[ -n "${tiles:-}" ] && [ "$tiles" -le 700 ]
check $? "the arena fits in video RAM (worst ~$tiles of 1024 tiles)"

# ⚠️ NEGATIVE CONTROL for the arena generator: one that ignored its seed would pass every
# determinism check in this file.
arenas=$(grep -ao "WH ARENA .*" "$log" | grep -o "p=[0-9-]*" | sort -u | wc -l | tr -d ' ')
[ "${arenas:-0}" -ge 2 ] ; check $? "arenas differ between matches ($arenas)"

# ⚠️ NEGATIVE CONTROL for the weapon panel's dirty key. Its fields used to be ORed together
# unmasked, and an unlimited rack stores its ammo as -1 — which sets every bit above its field, so
# the weapon-selection bits were already 1 and changing weapon could not change the key. The panel
# only refreshed on the next TURN, when `who` finally moved a bit the -1 had not claimed. Nothing
# else in this file could see it: the rack was right, the selection was right, only the picture was
# stale. So: the panel must repaint more often than the turn changes.
panels=$(grep -ac "WH PANEL" "$log" || true)
turns=$(grep -ac "WH TURN" "$log" || true)
[ "${panels:-0}" -gt "${turns:-0}" ] ; check $? "the weapon panel repaints on selection, not only on turn ($panels repaints, $turns turns)"

# ⚠️ TWO CHECKS, BECAUSE "span" MEASURED THE WRONG THING once the HUD became a layout tree. A tree
# is boxed objects, so a repaint allocates tens of kilobytes and frees them again — free heap dips
# to ~11 KB for a frame and returns. That is a transient, not a leak, and a span gate cannot tell
# the two apart: it fails on the dip and would have to be loosened to a number that no longer
# catches a real leak either. So: a FLOOR (does it ever come close to running out?) and a TREND
# (does it end where it started?).
lo=$(grep -ao "HEAP [0-9]*" "$log" | cut -d' ' -f2 | sort -n | head -1)
# ⚠️ NOT THE FIRST SAMPLE. The first reading is taken before the UI pools have grown, so comparing
# against it reports one-time warm-up as a 35 KB leak. The second is after the game has settled.
first=$(grep -ao "HEAP [0-9]*" "$log" | cut -d' ' -f2 | sed -n '3p')
last=$(grep -ao "HEAP [0-9]*" "$log" | cut -d' ' -f2 | tail -1)
[ -n "${lo:-}" ] && [ "$lo" -ge 8192 ]
check $? "heap floor (lowest free ${lo:-?} B)"
[ -n "${first:-}" ] && [ $(( first - last )) -le 16384 ]
check $? "heap does not leak (ended $(( ${first:-0} - ${last:-0} )) B lower than it began)"

# ⚠️ THE THRESHOLD IS MEASURED, AND IT HAS GONE UP TWICE FOR REASONS WORTH WRITING DOWN.
#
# It was 2000 while the attract driver stood on the fire button for 680 ticks a turn — most frames
# were idle, and the average described the idling rather than the game. Fixing that took it to 3350.
#
# Adopting packages/ui for the HUD took it to ~3950. That cost is real and was measured three ways,
# because a layout engine repaints the WHOLE canvas and this HUD changes twice a turn:
#   streamed at the default budget of 6 : avg 4219, worst frame 42,000
#   streamed at budget 24               : avg 3986, worst frame 34,000
#   painted in one go                   : avg 3901, worst frame 64,000
# Presentation went from ~550 ticks a frame to ~1400. Budget 24 is the shipped compromise: no
# fourteen-frame stall, no 500-tick tax on every frame.
#
# 4200 leaves the remaining headroom visible while still failing on a regression. ⚠️ mGBA's FPS
# counter cannot check any of this — it measures the emulated LCD, which reads 60 whether or not the
# game finished its frame.
avg=$(grep -ao "WH TICKS .* avg=[0-9]*" "$log" | grep -o "avg=[0-9]*" | cut -d= -f2 | sort -n | tail -1)
[ "${avg:-9999}" -lt 4200 ] ; check $? "sustained frame cost ${avg:-?} of 4389 ticks"

# ── Two consoles ────────────────────────────────────────────────────────────────────────────────
echo
echo "== two consoles =="
# ⚠️ A linked simulated frame lands about every SIX display frames, so a press has to be held across
# at least twelve of them to be sampled at all. Every window below is 26 frames wide.
KEYS="$(python3 - <<'PYEOF'
p, f = [], 150
for _ in range(16):
    p += [f"{f}:a", f"{f+26}:"]
    f += 110
print(",".join(p))
PYEOF
)"
"$root/scripts/link.sh" warheads.gba warheads.gba 5000 "$KEYS" "$KEYS" >/dev/null 2>/tmp/warheads-link.log
check $? "two linked consoles run to completion"

plays=$(grep -ac "LINK PLAYING" /tmp/warheads-link.log || true)
[ "${plays:-0}" = 2 ] ; check $? "both consoles reach PLAYING ($plays)"
sides=$(grep -ao "side=[01]" /tmp/warheads-link.log | sort -u | wc -l | tr -d ' ')
[ "${sides:-0}" = 2 ] ; check $? "one plays P1, the other P2"

# THE assertion, and pong-link's differ unchanged: both consoles describe their simulation at the
# same numbered tick, and every pair must be identical.
python3 - <<'PYEOF'
import re
import sys

p0, p1 = {}, {}
for line in open("/tmp/warheads-link.log", errors="replace"):
    m = re.match(r"\[p(\d) frame \d+\] WH SYNC (f=\d+ .*)", line.strip())
    if not m:
        continue
    (p0 if m.group(1) == '0' else p1)[m.group(2).split()[0]] = m.group(2)

common = sorted(set(p0) & set(p1), key=lambda k: int(k[2:]))
bad = [k for k in common if p0[k] != p1[k]]
# ⚠️ Without this, two consoles that printed NOTHING would "agree at all 0 frames".
if len(common) < 10:
    print(f"  (only {len(common)} comparable simulation ticks — the game barely ran)")
    sys.exit(1)
if bad:
    print(f"  (the two consoles disagree at {len(bad)} of {len(common)} ticks)")
    for k in bad[:3]:
        print(f"       p0: {p0[k][:120]}")
        print(f"       p1: {p1[k][:120]}")
    sys.exit(1)
print(f"  (agreed at all {len(common)} compared ticks)")
PYEOF
check $? "both consoles run the SAME match"

crash_grep /tmp/warheads-link.log ; check $? "no crash on either console"

# ── The ready gate ──────────────────────────────────────────────────────────────────────────────
# ⚠️ If the match started on its own the two consoles would still AGREE with each other perfectly,
# so the determinism check above cannot catch this one. pong-link learned that the hard way.
echo
echo "== ready gate =="
"$root/scripts/link.sh" warheads.gba warheads.gba 1600 "" "" >/dev/null 2>/tmp/warheads-ready.log
grep -ao "WH SYNC .*" /tmp/warheads-ready.log | tail -1 > /tmp/warheads-ready.last
[ -s /tmp/warheads-ready.last ] ; check $? "both consoles simulate while waiting"
shots=$(grep -ac "WH SHOT" /tmp/warheads-ready.log || true)
[ "${shots:-0}" = 0 ] ; check $? "nothing is fired before a player acts ($shots)"

# ── A CPU never runs in a linked match ──────────────────────────────────────────────────────────
# The search terminates on a wall clock, and two consoles spend different ticks on identical
# instructions — so a linked CPU would stop at different candidates and play two different matches.
grep -q "WH CPU linked=1" /tmp/warheads-link.log
check $((1 - $?)) "no CPU is ever enabled in a linked match"

echo
if [ "$ran" -eq "$EXPECT" ]; then
  echo "ok   all $EXPECT checks ran"
else
  echo "FAIL only $ran of $EXPECT checks ran — one died without printing ok or FAIL"
  fails=$((fails + 1))
fi
finished=1
[ "$fails" = 0 ] && echo "warheads: PASS" || echo "warheads: $fails FAILED"
exit $([ "$fails" = 0 ] && echo 0 || echo 1)
