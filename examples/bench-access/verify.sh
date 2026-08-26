#!/usr/bin/env bash
# Verify bench-access, and pin the four facts it exists to establish.
#
# These are RATIOS, not absolute ticks. An absolute bar would break on any compiler improvement,
# which is the opposite of what this bench is for — the point is the SHAPE of the cost model, and
# the shape is what optimisation decisions in this repo are made from. Each assertion below is a
# belief that was acted on before it was measured, and two of them were wrong.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR
npm run build >/tmp/bench-access-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -20 /tmp/bench-access-build.log; exit 1; fi

GBA_SHOT_LOG=1 ../../scripts/screenshot.sh bench-access.gba /tmp/bench-access.png 200 2>&1 \
  | grep -E 'BENCH-ACCESS|'"$CRASH_RE" > /tmp/bench-access.log
check $? "runs headless"

grep -qE "$CRASH_RE" /tmp/bench-access.log
check $((1 - $?)) "no panic, no allocation failure"

grep -q 'BENCH-ACCESS DONE' /tmp/bench-access.log
check $? "ran every measurement to completion"

# ⚠️ THE FIRST ASSERTION, AND THE ONE THAT MAKES THE REST MEAN ANYTHING.
#
# `timer_read()` is 16 bit. The first version of this bench took all its spans back to back from
# module scope, ~50,000 ticks with no `frame()` in it, and everything after the first wrap was
# fiction: a function call measured at 0.026 ticks and the array-parameter case came out NEGATIVE.
# The array numbers in that run were correct, because they happened to land in the first frame —
# which is exactly how a wrapped measurement gets believed. The ROM now checks every span itself.
grep -q 'BENCH-ACCESS SANE 1 ' /tmp/bench-access.log
check $? "every span is positive and inside one frame (the timer did not wrap)"
note "$(grep -o 'SANE.*' /tmp/bench-access.log)"

raw() { grep -o "BENCH-ACCESS RAW.*" /tmp/bench-access.log | tr ' ' '\n' | grep "^$1=" | cut -d= -f2; }
LOOP=$(raw loop); LOCAL=$(raw local); MODULE=$(raw module)
C0=$(raw c0); C4=$(raw c4); C4ARR=$(raw c4arr); BIG=$(raw big); C4ARR7=$(raw c4arr7)
BIGMOD=$(raw bigmod); MODWR=$(raw modwr); LITERAL=$(raw literal)
note "raw: loop=$LOOP local=$LOCAL module=$MODULE bigmod=$BIGMOD modwr=$MODWR literal=$LITERAL c0=$C0 c4=$C4 c4arr=$C4ARR big=$BIG c4arr7=$C4ARR7"

echo "== the cost model =="

# 1. A MODULE-level typed array read costs more than a LOCAL one, but only about 3x.
#
# docs/memory-perf-review-2026-07.md S-10 records that a module typed array has no by-reference
# read form, so every element access re-borrows. True — but the price is ~1.35 ticks against ~0.48,
# not the ~40 that `drop_attack.tish` was rewritten around. That 40 was DIVIDED OUT of a whole
# subsystem's measurement rather than measured, and it was wrong by a factor of thirty.
[ "$MODULE" -gt "$LOCAL" ]
check $? "a module array read costs more than a local one ($MODULE vs $LOCAL per 1000)"
[ $(( MODULE * 100 / LOCAL )) -lt 500 ]
check $? "...but only ~3x, not the ~80x a 40-tick read would imply"

# 2. A trivial typed call with scalar arguments is FREE — it lowers to a direct native fn and
#    inlines. 1, 4 and 8 arguments all measure at the empty loop. So "a tish call costs ~150 ticks"
#    is not a property of calls; see 3 and 4 for what it is a property of.
[ "$C4" -le $(( LOOP / 8 )) ]
check $? "a trivial 4-argument typed call is free (c4=$C4 against a 1000-iteration loop of $LOOP)"

# 3. A ZERO-argument call is free too — and it did not used to be.
#
# ⚠️ THIS ASSERTION USED TO SAY THE OPPOSITE, and it was right when it was written: a niladic call
# measured ~55 ticks against ~0 for a 4-argument one, because no-params was a hole in typed
# lowering. Every `rCols()`, `rPocket()` and `atkLines()` in these packages paid it on every call.
# tishlang/tish#647 closed that hole (553 -> 26 ticks) and the old assertion became a false claim
# that still passed, because nothing re-ran it against the new compiler for two days.
#
# So it is inverted rather than deleted. A deleted assertion protects nothing, and the interesting
# direction now is the regression: if a zero-argument call ever stops lowering again, this fires.
[ "$C0" -le $(( C4 * 3 + LOOP / 8 )) ]
check $? "a ZERO-argument call lowers natively, like a 4-argument one (c0=$C0 vs c4=$C4) — #647"

# 4. Passing an `i32[]` across a module boundary is the most expensive thing here: ~222 ticks, some
#    170x a scalar call. Batching work through an array parameter to avoid N scalar calls is
#    therefore a PESSIMISATION unless N is large — which is the opposite of the intuition, and the
#    reason an attack-table rewrite recovered only a third of what it looked like it should.
[ "$C4ARR" -gt $(( C4 * 20 )) ]
check $? "an i32[] parameter costs far more than the scalar call it rides on ($C4ARR vs $C4)"

# 5. A REALISTIC accessor — a branch on a module flag plus a computed index into a large module
#    array, i.e. exactly `rColourPattern` — costs ~193 ticks and does NOT inline. This is the
#    number the engine's hot loops actually pay, and the one that reconciles the model with the
#    field: 2,340 ticks measured to build one attack line out of 14 such calls.
[ "$BIG" -gt $(( C4 * 20 )) ]
check $? "a realistic accessor does NOT inline away like the trivial ones (big=$BIG vs c4=$C4)"

# 6. An array parameter is priced PER WRITE, not just per call — ~178 ticks to pass it and ~54 for
#    every element written through it, against ~1.35 for the same write to a module array in the
#    callee's own file. Forty times. This is what makes "batch it through an out-parameter" the
#    wrong instinct on this target: the batching helper should own its buffer and expose a reader,
#    not accept one. That atkBuild was 1,469 ticks per call for seven such writes.
#
#    c4arr7 runs at WN=5 and c4arr at CN=10, so compare PER OP: (c4arr7/5) > (c4arr/10).
[ $(( C4ARR7 / 5 )) -gt $(( C4ARR / 10 )) ]
check $? "an array parameter costs per WRITE, not per call ($(( C4ARR7 / 5 )) for 7 writes vs $(( C4ARR / 10 )) for 1)"

echo "== two things that are NOT the problem =="
# Negative results, asserted so nobody spends an afternoon rediscovering them. Both were live
# hypotheses for why that atkBuild inner loop costs ~20x what the read number predicts.

# A. Array SIZE does not affect access cost. 2,048 entries reads identically to 64 — the two
#    measurements are the same number, not merely close.
[ "$BIGMOD" -lt $(( MODULE * 110 / 100 )) ]
check $? "a 2048-entry module array reads the same as a 64-entry one ($BIGMOD vs $MODULE) — size is not it"

# B. A module-array WRITE is not meaningfully worse than a read: ~1.66 ticks against ~1.35. Writes
#    through an ARRAY PARAMETER are the expensive ones (see 6 above); writes to a module array in
#    the writer's own file are ordinary.
[ "$MODWR" -lt $(( MODULE * 2 )) ]
check $? "a module array write costs about what a read does ($MODWR vs $MODULE) — writes are not it"

echo "== the one that mattered, and no longer does =="
# ⚠️⚠️ THIS SECTION USED TO ASSERT THAT AN ARRAY LITERAL READS ~37x SLOWER THAN THE SAME DATA BUILT
# WITH push(). It did: 63.2 ticks against 1.68, for identical loops over identical values at
# identical indices, because a promoted static's index path built a `Value::Number(i as f64)`,
# matched it back to a usize, read the array as f64 and converted to i32, per element.
#
# That finding is why a warmed two-slot table cache is worth considering, and it
# was the reason to expect one here too. tishlang/tish#645 fixed it. A promoted literal now reads
# ~0.67 ticks against a pushed array's ~1.69 — the gap did not close, it INVERTED, because a static
# is a ROM load while a `VmRef<Vec<i32>>` costs a borrow and a bounds check.
#
# The old assertion was written as a gate specifically so it would start failing on the day the
# compiler fixed this. It did. Inverted rather than deleted, for the same reason as 3 above.
#
# ⚠️ The story does not end here, and the rest of it is NOT in this file: the fix reaches the index
# shapes the compiler can reduce to a mask, and the bounds-checked fallback it emits for a COMPUTED
# index is still f64-typed and still costs ~25 ticks an element. `examples/bench-tables` isolates
# that, at the 2,304-entry scale the generated tables in the topdown RPG ports actually use.
[ $(( LITERAL * 1000 / 40 )) -lt $(( MODULE * 100 / 1000 * 15 )) ]
check $? "a promoted literal reads no slower than a pushed array ($(( LITERAL * 100 / 40 )) vs $(( MODULE * 100 / 1000 )) ticks*100) — #645"

echo "== the mechanism behind all of it =="
# ⚠️⚠️ TYPED LOWERING IS ALL-OR-NOTHING, AND ALMOST NOTHING QUALIFIES.
#
# A tish function is lowered to a direct Rust `fn name_native(a: i32, ...) -> i32` — a real call,
# inlinable, ~0 ticks — ONLY if it touches no `VmRef` at all: no module array, no module scalar, not
# even a LOCAL array built with push(). Everything else becomes a `Value::native` closure invoked
# through `value_call` with every argument boxed as `Value::Number(x as f64)` and unboxed again in
# the callee. That is the ~190 ticks `bigRead` costs and `call4` does not.
#
# This is what the numbers above are all made of, and it is checkable directly in the generated
# Rust rather than inferred from a stopwatch. In that whole ROM, ONE function out of hundreds
# got a typed twin: `cpuMaxCandidates(cols: i32): i32 { return cols + 2 * (cols - 1) }`. Every rules
# function touches state, so every rules call is boxed.
#
# It also compounds with the literal finding above, in opposite directions — there is no fast shape:
#
#   data as a LITERAL   the function CAN be typed-lowered, but every read of it is boxed (63 ticks)
#   data PUSHED         reads are fast (1.35), but the function can NEVER be typed-lowered, so every
#                       call to it boxes and unboxes all of its arguments
#
# Filed upstream. Asserted here against the GENERATED RUST, so it is a fact about the compiler and
# not a timing that could drift.
GEN=.tish/gba/bench-access/src/main.rs
if [ -f "$GEN" ]; then
  [ "$(grep -c '^fn call4_native' $GEN)" = 1 ]
  check $? "a PURE typed function gets a direct native twin (call4_native)"
  [ "$(grep -c '^fn bigRead_native' $GEN)" = 0 ]
  check $? "a function touching a module array does NOT (bigRead has no twin)"
  [ "$(grep -c '^fn benchLiteral_native' $GEN)" = 1 ]
  check $? "...but reading a LITERAL is fine, because it promotes to a static (benchLiteral does)"
  [ "$(grep -c '^fn benchModule_native' $GEN)" = 0 ]
  check $? "...and the same loop over a PUSHED array does not (benchModule has none)"
else
  note "generated Rust not present — skipped the codegen assertions"
fi

if [ $fail = 0 ]; then echo "bench-access: PASS"; else echo "bench-access: FAIL"; fi
exit $fail
