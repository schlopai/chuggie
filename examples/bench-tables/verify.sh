#!/usr/bin/env bash
# Verify bench-tables, and answer the one question it was built to answer:
#
#     is a warmed copy of a generated table worth building?
#
# The answer decides whether `drop-gba/packages/drop_tables.tish`'s two-slot cache gets generalised
# into this repo's table generator, or whether that whole design is dead code here.
#
# ── GATES vs VERDICTS ─────────────────────────────────────────────────────────────────────────
#
# The checks under "the cost model" are GATES: they fail the build, because each one pins a shape
# that optimisation decisions in this repo are made from, and a silent change to any of them
# invalidates advice that is written down elsewhere.
#
# The checks under "the verdict" are NOT gates. They print an answer and always exit 0. A bench
# whose job is to decide something must be free to decide either way — wiring the decision to the
# exit code would mean CI goes red the day the compiler improves, which is the opposite of useful.
# `bench-access` has the scar: its literal-vs-pushed assertion was written as a gate, the compiler
# fixed the underlying bug, and the assertion is now a false claim that still passes because it was
# never re-run against the new binary.
#
# Every number here is a RATIO of net per-op costs. Absolute ticks would break on any compiler
# change, and the shape is what matters.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }
verdict() { printf '  ->  %s\n' "$*"; }

echo "== rom =="
unset CARGO_TARGET_DIR

# The tables are generated, and a stale tables.tish would measure yesterday's data. Regenerate and
# fail if that changed anything the committed file did not already have.
python3 ../../scripts/gen_bench_tables.py >/dev/null 2>&1
check $? "regenerates src/tables.tish"
git diff --quiet -- src/tables.tish
check $? "committed src/tables.tish matches the generator"

npm run build >/tmp/bench-tables-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -20 /tmp/bench-tables-build.log; exit 1; fi

assert_agb_fork .
check $? "resolved agb to the fork"

assert_typed_scalars src
check $? "no untyped module scalars"

GBA_SHOT_LOG=1 ../../scripts/screenshot.sh bench-tables.gba /tmp/bench-tables.png 200 2>&1 \
  | grep -E 'BENCH-TABLES|'"$CRASH_RE" > /tmp/bench-tables.log
check $? "runs headless"

crash_grep /tmp/bench-tables.log
check $? "no panic, no allocation failure"

grep -q 'BENCH-TABLES DONE' /tmp/bench-tables.log
check $? "ran every measurement to completion"

# ⚠️ THE ASSERTION THAT MAKES THE REST MEAN ANYTHING.
#
# `timer_read()` is 16 bit and wraps every ~15 frames. The ROM checks, for itself: every span is
# positive, every span is inside one frame, every bench arm cost MORE than its own empty loop, and
# the literal and pushed arms hold identical data. bench-access learned why the hard way — a run
# with no `frame()` in it reported a call at 0.026 ticks and an array parameter as NEGATIVE, and the
# half of that run which happened to land in the first frame was believed for weeks.
grep -q 'BENCH-TABLES SANE 1 ' /tmp/bench-tables.log
check $? "spans positive, inside a frame, above their controls, and both arms agree"
note "$(grep -o 'SANE.*' /tmp/bench-tables.log)"

raw() { grep -o "BENCH-TABLES RAW.*" /tmp/bench-tables.log | tr ' ' '\n' | grep "^$1=" | cut -d= -f2; }
LOOPP=$(raw loopP); P1=$(raw p1); W=$(raw w)
LOOPL=$(raw loopL); L1=$(raw l1)
LOOPC=$(raw loopC); L2=$(raw l2); P2=$(raw p2)
FILL32=$(raw fill32); FILL64=$(raw fill64)
FILLCONST=$(raw fillConst); FILLPUSH=$(raw fillPush); FILLMASK=$(raw fillMask)
PN=$(raw PN); LN=$(raw LN); CN=$(raw CN); FN=$(raw FN); FN2=$(raw FN2)

if [ -z "${P1:-}" ] || [ -z "${L1:-}" ]; then
  echo "FAIL no RAW line to read — nothing below can be computed"
  echo "bench-tables: FAIL"; exit 1
fi

# Net of the loop-only control at the SAME N, in hundredths of a tick per operation, so a sub-tick
# difference is still visible. Net, because at LN=40 the loop itself is a real fraction of the span.
NP1=$(( (P1 - LOOPP) * 100 / PN ))
NW=$((  (W  - LOOPP) * 100 / PN ))
NL1=$(( (L1 - LOOPL) * 100 / LN ))
NL2=$(( (L2 - LOOPC) * 100 / CN ))
NP2=$(( (P2 - LOOPC) * 100 / CN ))
# Two widths, so the per-element SLOPE and the one-off boxed-call INTERCEPT come apart. Scaling a
# single fill span straight to 128 counts that call four times and reports an element as costing
# ~28 ticks when the parts it is made of cost ~2 — which is how a bench invents a problem.
FSLOPE=$(( (FILL64 - FILL32) * 100 / (FN2 - FN) ))    # ticks*100 per element
FCALL=$(( FILL32 - (FSLOPE * FN / 100) ))              # what is left is the call itself
FILL128=$(( FCALL + FSLOPE * 128 / 100 ))

# The decomposition, all at FN2 and all through the same one boxed call, so they subtract cleanly.
NFC=$(( (FILLCONST - FCALL) * 100 / FN2 ))   # the WRITE alone
NFP=$(( (FILLPUSH  - FCALL) * 100 / FN2 ))   # write + pushed read
NFM=$(( (FILLMASK  - FCALL) * 100 / FN2 ))   # write + literal read, MASKED index
NFA=$(( (FILL64    - FCALL) * 100 / FN2 ))   # write + literal read, ADDITIVE index

note "raw:  loopP=$LOOPP p1=$P1 w=$W | loopL=$LOOPL l1=$L1 | loopC=$LOOPC l2=$L2 p2=$P2"
note "raw:  fill32=$FILL32 fill64=$FILL64 const=$FILLCONST push=$FILLPUSH mask=$FILLMASK"
note "net (ticks*100/op): PUSH[i]=$NP1  LIT[i]=$NL1  WARM[i]=$NW  litAt()=$NL2  pushAt()=$NP2"
note "fill (ticks*100/elem): write=$NFC  +PUSH[]=$NFP  +LIT[mask]=$NFM  +LIT[add]=$NFA  (call ~$FCALL)"
note "one 128-element fill: ~$FILL128 ticks (a frame is 4389)"

echo "== the cost model =="

# 1. ⚠️⚠️ THE REGRESSION GATE. A promoted literal must read at about what a pushed array reads.
#
# It did not always. bench-access measured 63.2 ticks against 1.68 — a 37x gap, because a promoted
# static's index path built a `Value::Number(i as f64)`, matched it back to a usize, read as f64 and
# converted to i32, per element. tishlang/tish#645 took that to ~0.65 by keeping the read in the
# integer domain, and this gate is what stops it coming back. If this fails, every generated table
# in the topdown RPG ports and the large SRPG example just got 37x more expensive to read and nothing else in this
# file matters.
[ $(( NL1 * 100 / NP1 )) -lt 300 ]
check $? "a literal array reads within 3x of a pushed one (LIT/PUSH = $(( NL1 * 100 / NP1 ))%)"

# 2. The cost that REPLACED it: the accessor, not the array.
#
# The topdown RPG port's world module reaches its tables through `uwDoor`, which calls `uwIndex`, which
# reads the module scalars `quest` and `curLevel`. Touching module state disqualifies a function from
# typed lowering (tishlang/tish#647), so the call is a boxed `value_call` — and that call now costs
# far more than the array read it wraps. This is where the remaining time in a room change is.
[ $(( NL2 * 100 / NL1 )) -gt 500 ]
check $? "a state-touching accessor costs >5x the read inside it (litAt/LIT = $(( NL2 * 100 / NL1 ))%)"

# 3. ...and it costs the SAME NUMBER OF TICKS over either arm. This is the assertion that says where
#    to spend the effort: if the accessor's cost were a property of the literal, warming the table
#    would remove it. It is not, so warming cannot. Hoist the index out of the accessor instead.
#
#    ⚠️ Compared as absolute per-op costs, NOT as a ratio against each arm's own read. The first
#    draft did the latter and failed on correct data: litAt/LIT and pushAt/PUSH are wildly different
#    multiples precisely BECAUSE the two reads differ, while the accessors themselves land within a
#    percent of each other. A ratio of ratios cannot express "the same absolute cost".
D=$(( NL2 - NP2 ))
[ ${D#-} -lt $(( NP2 / 10 )) ]
check $? "the accessor costs the same over either arm ($NL2 vs $NP2, ${D} apart) — it is not the array"

echo "== the index-expression bug, FIXED upstream =="
# ⚠️ THIS SECTION USED TO GATE A LIVE BUG: reading the same promoted literal cost ~0.4 ticks an
# element with a MASKED index and ~25 with an ADDITIVE one, because the bounds-checked fallback read
# through f64 — two soft-float conversions per element on a chip with no FPU. Filed as
# tishlang/tish#658 and fixed in 56b3b9b32 (plus a8f5a637a, which made an out-of-range read answer
# null like every other backend rather than NaN).
#
# The assertion was written as a ratio SO THAT IT WOULD START FAILING when the compiler was fixed,
# and it did. Retired rather than retuned, which is what its own comment asked for. The measurement
# is kept because it is the evidence the bug is gone:
#
#     +LIT[mask] 217   +LIT[add] 225   (ticks*100 per element)
#
# — the two index shapes now cost the same, where they differed by 13x.
[ $(( NFA * 100 / NFM )) -lt 200 ]
check $? "an additive index costs about what a masked one does ($NFA vs $NFM) — tish#658"

echo "== the verdict (not a gate — see the header) =="

# 4. ⚠️ THE QUESTION. Is a warmed pushed copy meaningfully faster to READ than the literal it was
#    copied out of? If it is not, `drop_tables.tish`'s two-slot cache has nothing to recover here and
#    should not be generalised into scripts/gen_tables.py at all.
#
#    R is the literal's cost as a percentage of the warmed copy's. R > 200 means reading the literal
#    costs at least twice what reading the cache costs, i.e. the cache earns its keep.
R=$(( NL1 * 100 / NW ))
if [ "$R" -gt 200 ]; then
  verdict "WARMING IS WORTH BUILDING: the literal costs ${R}% of a warmed copy."
  verdict "   -> build the generator's --warm arm; see the plan's WS2B."
else
  verdict "WARMING IS NOT WORTH BUILDING: the literal costs ${R}% of a warmed copy — reading the"
  verdict "   promoted static DIRECTLY is $(( NW * 100 / NL1 ))% the cost of reading the cache built from it."
  verdict "   tishlang/tish#645 closed the gap this cache existed to close, and then some: a"
  verdict "   promoted static is a ROM load, while a pushed Vec costs a borrow and a bounds check."
  verdict "   -> do NOT generalise drop_tables.tish's cache into scripts/gen_tables.py."
  verdict ""
  verdict "   ⚠️ AND NOTE WHAT THIS DOES NOT SAY. Both arms above are indexed with a MASK. An"
  verdict "   ADDITIVELY-indexed literal costs $NFA against a pushed array's $NFP, so for a table read"
  verdict "   as UW_DOORS[uwIndex(r) * 4 + side] a warmed copy WOULD win. It is still the wrong fix:"
  verdict "   masking the index costs $NFM, which beats the cache, and it is a one-line change"
  verdict "   against a codegen feature. Fix the index; do not cache around it."
fi

# 5. What the cache would have COST, either way — kept because it is the number behind the rule
#    "warm off the hot frame", and a rule with a measurement attached survives a refactor.
#    drop-gba filled its cache lazily once: the fill landed on the frame that needed the data and
#    took the peak from 6,614 to 12,877 ticks, worse than having no cache at all.
if [ "$FILL128" -gt 4389 ]; then
  verdict "one 128-element fill is ~$FILL128 ticks — MORE than a frame. A lazy fill is a dropped frame."
else
  verdict "one 128-element fill is ~$FILL128 ticks, inside a 4389-tick frame."
fi

echo
if [ $fail = 0 ]; then echo "bench-tables: PASS"; else echo "bench-tables: FAIL"; fi
exit $fail
