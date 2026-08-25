#!/usr/bin/env bash
# Verify versus — the 1v1 fighter.
#
# What can go wrong here is not what a screenshot shows. A ROM that builds and paints can still have
# a fighter that never becomes actionable, a hitbox that reaches through the whole stage, or an
# unannotated constant that quietly halved the frame rate. So this checks the three properties that
# nothing else does:
#
#   typed     every module-level scalar carries `: i32`. See docs/perf-rules.md §1 — an untyped one
#             is a soft-float thread-local and costs ~20% of a frame. This is the regression gate for
#             the repo-wide sweep; without it the next edit undoes the work silently.
#   arity     tish does NOT check call arity, and this game's frame-data authoring calls take up to
#             ten arguments. A dropped comma is a zero-damage move, not an error.
#   soak      the ROM survives 9,000 frames of INPUT — attract mode alone never presses a button, so
#             a soak without a schedule never executes the player's code path at all.
#   live      the picture at several points is a real frame, not agb's crash page. `shot_check.py`
#             refuses to report pixel metrics off a dead ROM, which is the trap that made a constant
#             crash-screen reading look like a stable measurement.
set -u
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh

fails=0
check() { if [ "$1" -eq 0 ]; then echo "  ok   $2"; else echo "  FAIL $2"; fails=$((fails+1)); fi; }

echo "versus:"
npm run build > /tmp/versus-verify-build.log 2>&1
check $? "builds"

assert_typed_scalars src ../../packages/fighter.tish ../../packages/motion.tish
check $? "every module scalar is typed (docs/perf-rules.md §1)"

python3 ../../scripts/arity_check.py src ../../packages/fighter.tish ../../packages/motion.tish > /tmp/versus-arity.log 2>&1
check $? "no call can panic on a missing typed argument"

soak_rom versus.gba 9000 "100:start,140:a,300:right,330:b,360:down,400:a,500:left,540:l,600:r,900:a,3000:b,6000:start,6100:a" > /tmp/versus-soak.log 2>&1
check $? "9000 frames with input: no crash, no halt"

for f in 130 260 520 2800; do
  python3 ../../scripts/shot_check.py versus.gba "$f" "100:start,105:,140:a,145:" > /dev/null 2>&1
  check $? "frame $f is a live picture"
done

echo "versus: $fails failure(s)"
exit $((fails > 0))
