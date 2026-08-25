#!/usr/bin/env bash
# Verify ringside — the over-the-shoulder boxing bout.
#
# What can go wrong here is mostly not what a screenshot shows. A ROM that builds and paints can
# still have an opponent whose tells are too short to react to, a divide that quietly costs a tenth
# of the frame, or a sprite budget that panics three minutes in. So this checks the properties that
# nothing else does:
#
#   typed    every module scalar carries `: i32`. docs/perf-rules.md §1 — an untyped one is a
#            soft-float thread-local and costs ~20% of a frame. This is the regression gate; without
#            it the next edit silently undoes the work.
#   arity    tish does NOT check call arity, and this game's authoring calls take up to six
#            arguments. A dropped comma in boxDefAttackHit is a zero-damage haymaker, not an error.
#   divide   the ARM7TDMI has no divide instruction. Every state machine here is a countdown, so a
#            `%` or `/` that creeps onto the hot path costs ~100 ticks a frame and nothing will
#            attribute it correctly. `motion.tish` once lost 1,400 of 4,389 ticks to a single `%`.
#   vram     sprite VRAM PANICS rather than degrading — `SpriteFull`, from inside agb, minutes into
#            play, on no particular frame. This game already hit it once: six 64x64 cells re-pointing
#            on the same frame transiently held old AND new tiles. A build-time budget assertion is
#            the only cheap guard that exists.
#   tells    every attack must stay REACTABLE at every difficulty. An unreadable tell is not
#            difficulty, it is a broken game, and it is the one balance number a later tweak could
#            destroy the game with while every other check stays green.
#   soak     9,000 frames of INPUT — attract mode never presses a button, so a soak without a
#            schedule never executes the player's code path at all.
#   live     the picture at several points is a real frame, not agb's crash page.
set -u
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh

fails=0
check() { if [ "$1" -eq 0 ]; then echo "  ok   $2"; else echo "  FAIL $2"; fails=$((fails+1)); fi; }

echo "ringside:"

python3 ../../scripts/gen_ringside.py > /tmp/ringside-assets.log 2>&1
check $? "art generates (and the ring bg stays under the BG palette ceiling)"

npm run build > /tmp/ringside-build.log 2>&1
check $? "builds"

assert_typed_scalars src ../../packages/boxing.tish
check $? "every module scalar is typed (docs/perf-rules.md §1)"

python3 ../../scripts/arity_check.py src ../../packages/boxing.tish > /tmp/ringside-arity.log 2>&1
check $? "no call can panic on a missing typed argument"

# `/` and `%` outside comments and outside string literals. The two divisions this game is allowed
# are the bar reciprocals in hud.tish, and they run ONCE, at init.
if grep -nE '^[^/]*[^/*= ][[:space:]]*[%/][[:space:]]*[^/*]' ../../packages/boxing.tish \
     | grep -vE '^\s*[0-9]+:\s*//' > /tmp/ringside-div.log 2>&1; then
  check 1 "no divide or modulo in packages/boxing.tish (see /tmp/ringside-div.log)"
else
  check 0 "no divide or modulo in packages/boxing.tish"
fi

python3 ../../scripts/ringside_check.py > /tmp/ringside-tells.log 2>&1
check $? "every tell stays reactable at every difficulty, and the sprite budget fits"

soak_rom ringside.gba 9000 "100:start,260:a,300:l,340:b,380:r,420:down,470:a,520:up,560:a,700:l,760:a,900:r,960:b,1200:a,1600:start,2400:a,3000:l,3600:a,5000:b,6500:a,8000:start" > /tmp/ringside-soak.log 2>&1
check $? "9000 frames with input: no crash, no halt"

for f in 60 240 700 2200; do
  python3 ../../scripts/shot_check.py ringside.gba "$f" "40:start,45:,300:a,305:" > /dev/null 2>&1
  check $? "frame $f is a live picture"
done

echo "ringside: $fails failure(s)"
exit $((fails > 0))
