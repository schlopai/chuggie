#!/usr/bin/env bash
# Verify grid-demo — and through it packages/grid.tish, the generic cell-grid kit.
#
# WHY THE DEMO IS A FLOOR-STACKED MATCH-3 AND NOT A CEILING-FED ONE. grid.tish was extracted from
# ceiling-fed rules, so a ceiling-fed demo would pass whether the kit were generic or merely the
# old code renamed. This is the opposite board — gravity toward the floor, gems dropped in from the
# top — and `anchor: 1` is the only line that differs.
#
# The three SELFTEST assertions run at boot, before any gameplay, so a failure names the broken rule
# instead of showing up as "the demo scored less". Each caught a real bug while this was written:
#
#   inert    an authored board must NOT clear itself. `gridSet` writes no seed; only `gridPush` and
#            `gridInsertRow` do. This is the causality rule — what separates a board that happens to
#            contain a run from a run the player caused.
#   seeded   the same board plus one pushed cell completing the run MUST clear all three. Read 0 on
#            the first build: `gridClearMarks` kept the cell byte and the wild bit but wiped the
#            CACHED MATCH MASK, so every pass after the first saw classless cells.
#   cascade  clearing a run drops survivors into a new one, and `gridCollapse` re-seeds anything
#            that MOVED so the chain continues with no further input. Read 0 first time because the
#            fixture left a gap in a column, and this kit models PACKED columns.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== static =="
# The index rule, worth 13x. `examples/bench-grid` measured `arr[(c<<5)+r]` at 20,318 ticks/1000
# against 1,494 for `arr[(c<<5)|r]`: `+` is not a ToInt32 context in JS so the chain leaves the
# integer domain, while the shift has already cleared the low bits so `|` is exactly equivalent. One
# `+` slipped into an index would cost more than every other optimisation in the file, silently.
python3 - <<'CHK'
import re, sys, pathlib
# Comments are stripped FIRST: the file documents the slow form, and the first version of this
# check duly failed on its own explanation.
bad = []
for n, line in enumerate(pathlib.Path("../../packages/grid.tish").read_text().splitlines(), 1):
    code = line.split("//")[0]
    if re.search(r"\[[^\]]*<<[^\]]*\+", code):
        bad.append(f"{n}: {code.strip()}")
for b in bad:
    print("  " + b)
sys.exit(1 if bad else 0)
CHK
check $? "no index in grid.tish packs with + instead of |"

# EVERY package, not just grid.tish — this is a repo-wide lint and it lives here because this is the
# verifier for the generic kit. tish does not check call ARITY: a call with too few arguments
# compiles, the missing parameter arrives as null, and a TYPED parameter's prologue then panics at
# runtime on the frame that path first runs. There is no compile error and no warning.
#
# One shipped: `drop_paint(P2)`, one argument to a two-argument function, undetected until a VS
# match ran on the tish rules core rather than the Rust one — the Rust painter took that parameter
# as `_rv` and ignored it, so for as long as it was the only core the missing argument genuinely did
# not matter. A mechanical port that changes nothing about a call site can still change what that
# call site means.
#
# It fails only where the missing parameter is TYPED, because only that panics. A short call into a
# parameter the body guards with `present(v)` / `pick(v, d)` / a null comparison is this codebase's
# way of saying "optional" and is correct; `--strict` reports those too.
python3 ../../scripts/arity_check.py --self-test >/dev/null 2>&1 \
  && python3 ../../scripts/arity_check.py ../../packages >/dev/null
check $? "no call in packages/ can panic on a missing typed argument"

echo "== rom =="
unset CARGO_TARGET_DIR
npm run build >/tmp/grid-demo-build.log 2>&1
built=$?
check $built "builds"
# Only a BUILD failure dumps the build log. Keying this off `$fail` meant an unrelated earlier
# failure printed 25 lines of cargo warnings and buried its own message.
if [ $built != 0 ]; then tail -25 /tmp/grid-demo-build.log; exit 1; fi

# 4000 frames, not 900, because the GAME now has an ending: the rising floor eventually beats the
# attract player and the run restarts, and neither the loss nor the restart exists inside 900.
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh grid-demo.gba /tmp/grid-demo.png 4000 2>&1 \
  | grep -E 'SELFTEST|GRID DEMO|DEAL|CLEAR |CHAIN|TICK|AI ticks|RUN |FEED |LEVEL |GAMEOVER|'"$CRASH_RE" > /tmp/grid-demo.log
check $? "runs headless"

grep -qE "$CRASH_RE" /tmp/grid-demo.log
check $((1 - $?)) "no panic, no allocation failure"

grep -q 'GRID DEMO START cols=8 rows=9' /tmp/grid-demo.log
check $? "boots, and the board reports its own geometry"

echo "== the causality rule =="
grep -q 'SELFTEST inert=0 ' /tmp/grid-demo.log
check $? "an AUTHORED board does not clear itself (gridSet seeds nothing)"
grep -q 'SELFTEST seeded=3 ' /tmp/grid-demo.log
check $? "the same board clears when a PUSH completes the run (gridPush seeds)"

echo "== cascades =="
# Both links, in order. A kit that cleared the first run and stopped would pass a "did it clear"
# check and still have no chains — the re-seed inside gridCollapse is the whole mechanism.
grep -q 'SELFTEST cascade link1=5 link2=3' /tmp/grid-demo.log
check $? "clearing a run drops survivors into a new one, re-seeded, with no further input"
note "$(grep -o 'SELFTEST cascade.*' /tmp/grid-demo.log | head -1)"

echo "== gameplay =="
clears=$(grep -c '^\[frame [0-9]*\] CLEAR ' /tmp/grid-demo.log || true)
[ "${clears:-0}" -ge 5 ]
check $? "cleared at least 5 times (got ${clears:-0})"
note "$(grep -oE 'CLEAR [0-9]+ CHAIN [0-9]+ KINDS [0-9]+' /tmp/grid-demo.log | head -3 | tr '\n' ' ')"

# KINDS is the union of match classes in the clear, read BEFORE the collapse removes them. Zero
# means gridScanMarked ran too late — which it did, in the first version.
grep -qE 'KINDS [1-9]' /tmp/grid-demo.log
check $? "a clear reports which classes it contained"

# The attract player must actually play. Aiming at the shallowest column keeps the board tidy and
# almost never forms a vertical three: that version ran 900 frames and cleared nothing.
# ⚠️ THE MAXIMUM, NOT THE LAST. Now that a run can END and the attract loop starts a fresh one, the
# final TICK line is whatever the NEW run has managed so far — this read 30 on a pass whose best run
# scored 2,450, and failed an assertion about how well the AI plays by measuring a game that had been
# running for four seconds.
score=$(grep -oE 'score [0-9]+' /tmp/grid-demo.log | grep -oE '[0-9]+' | sort -n | tail -1)
[ "${score:-0}" -ge 100 ]
check $? "the attract demo builds runs rather than spreading (score ${score:-0})"

echo "== the game layer =="
# The three things that turned this from a fixture for the kit into a game, each a `grid.tish` call
# no example was exercising.

# ⚠️ `gridAnyOver`, NOT `gridAnyFull`. The kit draws the distinction explicitly and it is a whole row
# of play: a column packed to exactly ROWS is full but still playable, and a loss condition of
# `gridAnyFull` ends the game one row early. Asserted BOTH ways in one value — exactly-full must read
# 0, one past must read 1 — because a test that only checks the second passes either way.
grep -q 'SELFTEST over=1 ' /tmp/grid-demo.log
check $? "a column filled to the last row is playable; one pushed PAST it is a top-out"

# A fed row arrives at the ANCHOR, which on this floor-stacked board is the floor — so garbage pushes
# the stack toward the ceiling rather than landing on top of it. `gridPush` would do the opposite, and
# that exact mix-up inverted the descent in the original port.
grep -q 'FEED 1 level' /tmp/grid-demo.log
check $? "the rising floor feeds garbage rows from the anchor end"
feeds=$(grep -c '^\[frame [0-9]*\] FEED ' /tmp/grid-demo.log || true)
[ "${feeds:-0}" -ge 3 ]
check $? "and keeps feeding as the level rises (${feeds:-0} rows)"

# Levels are counted in CLEARED CELLS, so a chain advances faster than the same cells cleared singly.
lvl=$(grep -oE 'LEVEL [0-9]+' /tmp/grid-demo.log | grep -oE '[0-9]+' | sort -n | tail -1)
[ "${lvl:-0}" -ge 5 ]
check $? "the level rises with the cells cleared (reached ${lvl:-0})"

# ⚠️ THE GAME MUST BE WINNABLE BY THE FLOOR. At a 120-frame feed interval the attract player survived
# 9,000 frames without ever topping out: the difficulty curve flattened out below the rate a competent
# player clears at, which is a rising floor that never rises. Nothing looked wrong — the demo just ran
# for ever, which is exactly what it did before there was a game layer at all.
grep -q 'GAMEOVER score' /tmp/grid-demo.log
check $? "the rising floor eventually beats the attract player"
note "$(grep -o 'GAMEOVER.*' /tmp/grid-demo.log | head -1)"

# And the run restarts, which is the branch a headless pass would otherwise never reach.
grep -q 'RUN 2 begins' /tmp/grid-demo.log
check $? "and the attract loop starts a fresh run afterwards"

echo "== the search =="
# The AI must actually be a search, not a heuristic — `plans` counts completed decisions.
plans=$(grep -oE 'plans [0-9]+' /tmp/grid-demo.log | tail -1 | grep -oE '[0-9]+')
[ "${plans:-0}" -ge 10 ]
check $? "the attract AI completed real searches (${plans:-0} decisions)"

# AND IT MUST FIT A FRAME. One frame is 4389 ticks. A tick budget cannot preempt a single
# evaluation, so the budget only bounds where an evaluation STARTS — which means the per-candidate
# cost is what actually has to fit. The first working version averaged 6,502 ticks a frame and ran
# the demo at 40fps; scoring one colour instead of five, restricting the run scan to dirty columns,
# and dropping a redundant plane clear took it to ~4,500, and a compiler fix for a bitwise array
# index took it to ~2,000.
#
# THE PEAK IS ASSERTED TOO, and it is the check that would have caught what the average did not. The
# average sat at 4,463 — under 4,389 only if you round the wrong way — while the peak was 5,672, over
# a frame, from a stated budget of 700. Asserting only the mean let a search that visibly missed
# frames report itself as fitting. Both are bounded now: 2,027 and 2,835.
avg=$(grep -oE 'avg [0-9]+' /tmp/grid-demo.log | tail -1 | grep -oE '[0-9]+')
[ "${avg:-99999}" -lt 2600 ]
check $? "the AI's average frame fits inside a frame (${avg:-?} of 4389 ticks)"
peak=$(grep -oE 'peak [0-9]+' /tmp/grid-demo.log | tail -1 | grep -oE '[0-9]+')
[ "${peak:-99999}" -lt 4389 ]
check $? "and so does its WORST frame (${peak:-?} of 4389 ticks)"
note "$(grep -o 'AI ticks:.*' /tmp/grid-demo.log | tail -1)"

echo "== screen =="
python3 - <<'SHOT'
import struct, zlib, sys
d = open('/tmp/grid-demo.png','rb').read()
pos, idat, w, h = 8, b'', 0, 0
while pos < len(d):
    ln = struct.unpack('>I', d[pos:pos+4])[0]; typ = d[pos+4:pos+8]
    if typ == b'IHDR': w, h = struct.unpack('>II', d[pos+8:pos+16])
    if typ == b'IDAT': idat += d[pos+8:pos+8+ln]
    pos += 12 + ln
raw = zlib.decompress(idat); stride = w*3+1
cols = set()
for y in range(0, h, 2):
    row = raw[y*stride+1:(y+1)*stride]
    for x in range(0, w*3-3, 3):
        cols.add(row[x:x+3])
print(f"  {len(cols)} distinct colours in {w}x{h}")
# Five gem hues plus rim shades, a backdrop and HUD text: a live board is well past 8.
sys.exit(0 if len(cols) > 8 else 1)
SHOT
check $? "the ROM paints a live board (not a blank screen)"

if [ $fail = 0 ]; then echo "grid-demo: PASS"; else echo "grid-demo: FAIL"; fi
exit $fail
