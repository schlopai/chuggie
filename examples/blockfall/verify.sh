#!/usr/bin/env bash
# Verify blockfall — a falling-block puzzle game on the guideline ruleset.
#
# WHAT THIS VERIFIER IS FOR. Almost nothing in a falling-block game fails loudly. A rotation table with
# a transposed digit, a kick row in the wrong slot, a search space that cannot reach two of the ten
# columns — none of them crash, none of them look broken in a screenshot, and every one of them just
# makes the game play a slightly different game. So the ROM asserts its own rules at boot, before a
# frame is drawn, and this script reads them back by exact value.
#
# Each one is here because it caught something or because nothing else could:
#
#   rotcells  the four rotations are GENERATED from the spawn shape, so this checks the rotation
#             FORMULA: four cells inside the box, and four turns returning to where it started.
#   reach     for every piece, rotation and column, the search must have a candidate that gets there.
#             It did not: targets were the piece's BOX left edge, so a vertical I (which occupies box
#             column 2 alone) could never be placed in columns 0 or 1, and an O could never reach
#             column 9. The symptom was a ragged, holed left edge and one line every six pieces —
#             which reads exactly like bad evaluator weights, and the weights were blamed first.
#   feats     the surface scan the AI is scored on is entirely bitwise and entirely illegible.
#   kick      the canonical SRS I-piece wall kick, by resulting column. A table shifted by one row
#             kicks the other way and still plays.
#   clear     a row clears AND the survivors above it fall. A collapse that did the first and not the
#             second passes any "did it clear" check.
#   quad      four rows at once, and the score — at level 0 the multiplier is ONE, not zero.
#   tspin     three corners plus a rotation, asserted BOTH ways: the same board reached by a slide
#             must not count, or every flat T lock in a corner scores like a spin.
#   topout    the spawn box overlapping the stack ends the game.
#
# ⚠️ The `topout` selftest ends a game, so it logs one GAMEOVER before gameplay. Any assertion about
# a real top-out has to account for it — which is why there is none: the attract player is good enough
# that it does not top out inside a verify run, and asserting that it does would be asserting that the
# AI is bad.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== static =="
# The index rule, worth 13x. `examples/bench-grid` measured `arr[(col << 5) + row]` at 20,318
# ticks/1000 against 1,494 for `|`: `+` is not a ToInt32 context in JS so the chain leaves the integer
# domain, while the shift has already cleared the low bits so `|` is exactly equivalent. This file
# indexes four strided arrays and one `+` in any of them would cost more than every other decision in
# it, silently. Comments are stripped first — the file explains the slow form at length.
python3 - <<'CHK'
import re, sys, pathlib
bad = []
for n, line in enumerate(pathlib.Path("src/main.tish").read_text().splitlines(), 1):
    code = line.split("//")[0]
    if re.search(r"\[[^\]]*<<[^\]]*\+", code):
        bad.append(f"{n}: {code.strip()}")
for b in bad:
    print("  " + b)
sys.exit(1 if bad else 0)
CHK
check $? "no index in src/main.tish packs with + instead of |"

# tish does not check call ARITY: a call with too few arguments compiles, the missing parameter
# arrives as null, and a TYPED parameter's prologue then panics at runtime the first frame that path
# runs. There is no compile error and no warning.
python3 ../../scripts/arity_check.py --self-test >/dev/null 2>&1 \
  && python3 ../../scripts/arity_check.py src >/dev/null
check $? "no call in src/ can panic on a missing typed argument"

# Auto-advance is a DEBUG affordance and must never run in front of a player: every timer here is
# gated on G.human, which latches on the first key and never clears.
python3 ../../scripts/autoadvance_check.py src/main.tish >/dev/null
check $? "no screen advances itself while a player is holding the pad"

echo "== rom =="
unset CARGO_TARGET_DIR
npm run build >/tmp/blockfall-build.log 2>&1
built=$?
check $built "builds"
if [ $built != 0 ]; then tail -25 /tmp/blockfall-build.log; exit 1; fi

# ⚠️ Logged to a FILE and grepped from there, never `echo "$x" | grep -q`: under `set -o pipefail`
# `grep -q` closes the pipe, the writer dies of SIGPIPE, and the check then fails PRECISELY BECAUSE
# the string was found.
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh blockfall.gba /tmp/blockfall.png 2400 2>&1 \
  | grep -E 'SELFTEST|BLOCKFALL |CLEAR |LEVEL |GAMEOVER|TICK |AI ticks|frame:|'"$CRASH_RE" \
  > /tmp/blockfall.log
check $? "runs headless"

crash_grep /tmp/blockfall.log
check $? "no panic, no allocation failure"

grep -q 'BLOCKFALL GAME 1 cols 10 rows 20' /tmp/blockfall.log
check $? "boots, and the well reports its own geometry"

echo "== the rules =="
for t in "rotcells=0" "reach=0" "feats=110101" "bag=127" "kick=6" "clear=16" "quad=800" "tspin=10" "topout=1"; do
  name=${t%%=*}
  grep -q "SELFTEST $t " /tmp/blockfall.log
  case "$name" in
    rotcells) what="four rotations per piece, closed under four turns" ;;
    reach)    what="the search space reaches every column, for every piece and rotation" ;;
    feats)    what="the bitwise surface scan agrees with an authored board" ;;
    bag)      what="a 7-bag deals each piece exactly once" ;;
    kick)     what="the SRS I-piece wall kick lands where the published table says" ;;
    clear)    what="a full row clears AND the survivors above it fall into it" ;;
    quad)     what="four rows at once score 800 at level 0" ;;
    tspin)    what="three corners plus a rotation is a T-spin; the same board by a slide is not" ;;
    topout)   what="a spawn into the stack ends the game" ;;
  esac
  check $? "$what"
done

echo "== gameplay =="
lines=$(grep -oE 'lines [0-9]+' /tmp/blockfall.log | tail -1 | grep -oE '[0-9]+')
[ "${lines:-0}" -ge 8 ]
check $? "the attract player cleared at least 8 lines in 2400 frames (got ${lines:-0})"
pieces=$(grep -oE 'pieces [0-9]+' /tmp/blockfall.log | tail -1 | grep -oE '[0-9]+')
[ "${pieces:-0}" -ge 30 ]
check $? "and locked at least 30 pieces (got ${pieces:-0})"
# Lines per piece is the one number that says the AI is PLAYING rather than merely surviving. Four
# cells is 0.4 of a row, so a player that never wastes a cell clears one line per 2.5 pieces; the
# broken search space above scored one per six, and that is the regression this catches.
python3 -c "import sys; sys.exit(0 if ${pieces:-1} and ${lines:-0}/${pieces:-1} >= 0.25 else 1)"
check $? "and cleared better than one line per four pieces (${lines:-0}/${pieces:-0})"
grep -q 'LEVEL 1' /tmp/blockfall.log
check $? "and played long enough to reach level 1"
note "$(grep -oE 'TICK .*' /tmp/blockfall.log | tail -1)"

echo "== the search =="
plans=$(grep -oE 'plans [0-9]+' /tmp/blockfall.log | tail -1 | grep -oE '[0-9]+')
[ "${plans:-0}" -ge 20 ]
check $? "the attract AI completed real searches (${plans:-0} decisions)"

# AND IT MUST FIT A FRAME. One frame is 4389 ticks. A tick budget is checked BEFORE a pair is handed
# out and cannot preempt the evaluation that follows, so the worst frame is the budget plus one whole
# evaluation — which means the PEAK is the number that matters, not the mean. grid-demo learned this
# the expensive way: its average sat at 4,463 while its peak was 5,672, and asserting only the mean
# let a search that visibly missed frames report itself as fitting.
avg=$(grep -oE 'avg [0-9]+' /tmp/blockfall.log | tail -1 | grep -oE '[0-9]+')
[ "${avg:-99999}" -lt 2200 ]
check $? "the AI's average thinking frame fits inside a frame (${avg:-?} of 4389 ticks)"
peak=$(grep -oE 'peak [0-9]+' /tmp/blockfall.log | tail -1 | grep -oE '[0-9]+')
[ "${peak:-99999}" -lt 3200 ]
check $? "and so does its WORST frame (${peak:-?} of 4389 ticks)"
note "$(grep -o 'AI ticks:.*' /tmp/blockfall.log | tail -1)"

echo "== frames actually delivered =="
# THE GROUND TRUTH, not the arithmetic. `frame_stats`'s dN is the hardware's own count of dropped
# frames per window, and it is the check that found what three rounds of reasoning did not: painting,
# not thinking. A tilemap write costs ~310 ticks, so the falling piece plus its ghost was sixteen
# writes and 4,952 ticks per horizontal move — over a whole frame, before the rules or the AI ran.
# Moving them to sprites took a 2,400-frame attract run from ~25 dropped frames per 300 to ~13, and
# they are now all on line clears, which already pause for the flash.
drops=$(grep -oE 'frame: .* d[0-9]+' /tmp/blockfall.log | grep -oE 'd[0-9]+$' | tr -d d | sort -n | tail -1)
[ "${drops:-999}" -le 25 ]
check $? "the attract run drops at most 25 frames per 300 (worst window ${drops:-?})"
note "$(grep -o 'frame: .*' /tmp/blockfall.log | tail -1)"

echo "== the player's game =="
# THE ATTRACT RUN NEVER PRESSES ANYTHING, so none of the above covers rotate, kick, hold, soft drop,
# hard drop or pause — the entire input half of the game. This drives them.
#
# ⚠️ Every press needs an explicit release: keys are held from their entry until the next one, and
# re-listing an already-held key is a no-op rather than a second press.
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh blockfall.gba /tmp/blockfall-play.png 1500 \
  "150:a,155:,170:left,185:,200:right,215:,230:b,235:,250:l,255:,300:up,305:,400:up,405:,500:up,505:,600:down,700:,800:start,810:,860:start,870:,900:up,905:,1000:up,1005:,1100:up,1105:,1200:up,1205:,1300:up,1305:" 2>&1 \
  | grep -E 'PLAYER INPUT|TICK |frame:|'"$CRASH_RE" > /tmp/blockfall-play.log
check $? "runs headless with a key schedule"

crash_grep /tmp/blockfall-play.log
check $? "no panic on the player's path"

grep -q 'PLAYER INPUT' /tmp/blockfall-play.log
check $? "the first key press latches the game out of attract mode"

# plans MUST NOT GROW across the run. This is the autoadvance rule as a runtime assertion: once a
# player has touched the pad, the AI never moves another piece, no matter how long they think.
# (The counter is lifetime plans, and the attract AI may legitimately plan its first piece before
# the frame-150 latch — so the gate is "no increase after the first sample", not "zero".)
firstplans=$(grep -oE 'plans [0-9]+' /tmp/blockfall-play.log | head -1 | grep -oE '[0-9]+')
lastplans=$(grep -oE 'plans [0-9]+' /tmp/blockfall-play.log | tail -1 | grep -oE '[0-9]+')
[ -n "${firstplans:-}" ] && [ "${lastplans:-1}" = "${firstplans:-0}" ]
check $? "and the attract player never takes another turn (plans ${firstplans:-?} -> ${lastplans:-?})"

# Hard drops must actually land pieces — a run where input did nothing would also report plans 0.
pplayed=$(grep -oE 'pieces [0-9]+' /tmp/blockfall-play.log | tail -1 | grep -oE '[0-9]+')
[ "${pplayed:-0}" -ge 6 ]
check $? "the schedule's hard drops locked pieces (${pplayed:-0})"

# ⚠️ SKIPPING THE FIRST WINDOW IS NOT CHERRY-PICKING, it is the only honest comparison: the schedule's
# first key lands at frame 150, so window one is half an attract-mode window with the search running
# in it, plus the boot selftests. It reported 29 drops against the player-path windows' 4-5, and the
# first version of this check duly failed on a number that was measuring attract mode.
pdrops=$(grep -oE 'frame: .* d[0-9]+' /tmp/blockfall-play.log | tail -n +2 | grep -oE 'd[0-9]+$' | tr -d d | sort -n | tail -1)
[ "${pdrops:-999}" -le 12 ]
check $? "and the player's game holds 60fps far better than attract (worst window ${pdrops:-?} of 300)"

echo "== soak =="
# agb's allocation-failure handler halts WITHOUT logging, so no grep can see it; soak_rom checks the
# crash strings, `SWI: 02` halts and frame progress together.
soak_rom blockfall.gba 3600 >/tmp/blockfall-soak.log 2>&1
check $? "survives a 3600-frame soak"

echo "== screen =="
python3 - <<'SHOT'
import struct, zlib, sys
d = open('/tmp/blockfall.png', 'rb').read()
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
# Seven piece hues at three shades each, two bevel edges apiece, the wall, the well and HUD text: a
# live board is far past 12.
sys.exit(0 if len(cols) > 12 else 1)
SHOT
check $? "the ROM paints a live board (not a blank screen)"

assert_agb_fork
check $? "built against the local agb fork"

if [ $fail = 0 ]; then echo "blockfall: PASS"; else echo "blockfall: FAIL"; fi
exit $fail
