#!/usr/bin/env bash
# microgame verify — the acceptance test for the engine's entity pool.
#
# The claim under test is not "the games are fun". It is:
#
#   a microgame cartridge throws its whole cast away every four seconds, hundreds of times a
#   session, and costs NOTHING to do it — no spawn, no despawn, no sprite allocation, no heap drift.
#
# That is the workload the pool exists for, and the one that catches the failure mode a pool
# prevents: EWRAM fragmented into an allocation failure a few hundred transitions in, which does not
# reproduce in a short run and does not look like the code that caused it.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR

python3 ../../scripts/gen_microgame.py >/dev/null 2>&1
check $? "regenerates both sprite sheets from the vendored pack"

npm run build >/tmp/microgame-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -25 /tmp/microgame-build.log; exit 1; fi

assert_agb_fork .
check $? "resolved agb to the fork"

assert_typed_scalars src
check $? "no untyped module scalars"

# ── The pool contract, statically ────────────────────────────────────────────
# Nothing may spawn or allocate a sprite outside the boot block. This is the invariant the whole
# design rests on, and it is cheap to state and cheap to break.
if awk '/^function (armProp|beginRound|catchSetup|dodgeSetup|grabSetup|mashSetup|catchTick|dodgeTick|grabTick|mashTick|paintProps)/,/^}/' src/main.tish \
   | grep -q 'spawn(\|sprite_new('; then
  echo "FAIL spawn/sprite_new on a per-round path"; fail=1
else
  echo "ok   no spawn/sprite_new outside boot (pool contract)"
fi
# The harness must not grow a callback registry — that is the cost model this design exists to
# avoid (~151 B of heap per registered game, ~1,000 ticks per frame to dispatch it).
if grep -qE 'define_component|add_behaviour' src/main.tish; then
  echo "FAIL a component/behaviour callback appeared — the dispatch is meant to be a plain branch"; fail=1
else
  echo "ok   dispatch is a direct branch, not a boxed callback"
fi

# ── Headless soak ────────────────────────────────────────────────────────────
# Long enough to reach GAME OVER and the restart: a run is ~336 frames at full length and three
# lives take a dozen rounds to spend. A short soak would pass while three of the harness's five
# states were dead code.
log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh microgame.gba /tmp/microgame-verify.png 6000 >"$log" 2>&1
check $? "runs 6000 frames headless"

crash_grep "$log"
check $? "no panic, no allocation failure"

# ⚠️ THE ONE THAT MATTERS. Entity count is 1 player + 8 props and never moves. A spawn-per-prop
# design shows up here immediately, and nowhere else until it OOMs.
counts=$(grep -o 'ENT [0-9]*' "$log" | sort -u | tr '\n' ' ')
nc=$(grep -o 'ENT [0-9]*' "$log" | sort -u | wc -l | tr -d ' ')
[ "$nc" = 1 ]
check $? "entity count constant ($counts) across every round — nothing spawns per round"

# The pool never hands out more than it holds, and the high-water mark says whether it is sized
# right: a pool whose high-water equals its size is one microgame away from silently arming nothing.
hi=$(grep -o 'high=[0-9]*' "$log" | cut -d= -f2 | sort -n | tail -1)
cap=$(grep -o 'props=[0-9]*' "$log" | head -1 | cut -d= -f2)
[ -n "$hi" ] && [ "$hi" -le "${cap:-8}" ]
check $? "pool high-water ${hi:-?} <= size ${cap:-8}"
[ "${hi:-0}" -lt "${cap:-8}" ]
check $? "...with headroom (${hi:-?} of ${cap:-8}) — a full pool arms nothing and says nothing"

# Heap must not DRIFT. It oscillates by one probe block as HUD text sprites come and go, which is
# bounded and fine; a staircase is not. Compare first and last steady-state samples rather than
# demanding a single value, because demanding one would fail on the legitimate oscillation.
first=$(grep -o 'HEAP [0-9]*' "$log" | sed -n '2p' | cut -d' ' -f2)
last=$(grep -o 'HEAP [0-9]*' "$log" | tail -1 | cut -d' ' -f2)
lo=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | head -1)
hiH=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | tail -1)
span=$(( hiH - lo ))
[ -n "$first" ] && [ "$last" -ge $(( first - 2048 )) ] && [ "$span" -le 4096 ]
check $? "heap bounded across the soak (span ${span:-?} B, $first -> $last)"

# ── The games actually run, and can be lost ──────────────────────────────────
# Every microgame must come up at least once. A game that is registered but never selected is the
# same bug class as a branch that is dead because of a name collision.
missing=""
for g in 0 1 2 3; do
  c=$(grep -c "MG RESULT game=$g " "$log" || true)
  [ "$c" -ge 1 ] || missing="$missing $g"
done
[ -z "$missing" ]
check $? "every microgame ran at least once (missing:${missing:- none})"

# ⚠️ NEGATIVE CONTROL. Wins alone prove nothing — an attract driver that cannot lose would produce
# a clean log while the lose path, the lives counter and GAME OVER were all dead. The driver throws
# every third round on purpose so this can be asserted.
wins=$(grep -c 'outcome=1' "$log" || true)
loss=$(grep -c 'outcome=2' "$log" || true)
[ "$wins" -ge 1 ] && [ "$loss" -ge 1 ]
check $? "both outcomes occur ($wins won, $loss lost) — the lose path is live"

# Spending the last life must reach GAME OVER, and the restart must run — that is the teardown path
# this example exists to hammer.
grep -q 'MG OVER' "$log"
check $? "reached GAME OVER (lives actually spend)"
grep -q 'MG RESTART' "$log"
check $? "restarted after game over (the teardown path runs)"

rounds=$(grep -c 'MG RESULT' "$log" || true)
note "rounds played: $rounds"

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/microgame-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
[ "${n:-0}" -ge 5 ]
check $? "frame paints ($n colours)"

echo
[ "$fail" = 0 ] && echo "microgame: PASS" || echo "microgame: FAIL"
exit $fail
