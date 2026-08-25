#!/usr/bin/env bash
# Verify beatemup — the brawler.
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
#             ten arguments. A dropped comma is a harmless attack, not an error.
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

echo "beatemup:"
npm run build > /tmp/beatemup-verify-build.log 2>&1
check $? "builds"

assert_typed_scalars src ../../packages/beatemup.tish ../../packages/motion.tish
check $? "every module scalar is typed (docs/perf-rules.md §1)"

python3 ../../scripts/arity_check.py src ../../packages/beatemup.tish ../../packages/motion.tish > /tmp/beatemup-arity.log 2>&1
check $? "no call can panic on a missing typed argument"

soak_rom beatemup.gba 12000 "100:start,110:,200:a,212:,400:a,412:,600:right,900:a,912:,1100:right,1600:r,1612:,1800:l,1812:,2000:b,2012:,3000:a,3012:,5000:start,5100:a,8000:a,8012:" > /tmp/beatemup-soak.log 2>&1
check $? "9000 frames with input: no crash, no halt"

for f in 140 300 700 1200; do
  python3 ../../scripts/shot_check.py beatemup.gba "$f" "100:start,110:,200:a,212:,300:a,312:" > /dev/null 2>&1
  check $? "frame $f is a live picture"
done

echo "beatemup: $fails failure(s)"
exit $((fails > 0))
