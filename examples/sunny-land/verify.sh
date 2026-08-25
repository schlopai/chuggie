#!/usr/bin/env bash
# Verify sunny-land — the platformer template.
#
# ⚠️ THIS EXISTS BECAUSE THE EXAMPLE SHIPPED BROKEN AND NOTHING NOTICED. It crashed on boot for
# weeks: `entityForget` was a back-compat no-op with an empty body and an unannotated parameter, so
# tish promoted it to a real Rust fn taking a NUMBER, and its only callers — the three spawn loops
# here — pass the wrapper OBJECT from `create()`. A stub whose whole job was to keep old games
# building panicked at frame 47 instead.
#
# It built cleanly the entire time. That is the point: `npm run build` says nothing about whether a
# ROM runs, and `shot_check.py` is the thing that refuses to report pixel numbers off a crash page.
set -u
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh

fails=0
check() { if [ "$1" -eq 0 ]; then echo "  ok   $2"; else echo "  FAIL $2"; fails=$((fails+1)); fi; }

echo "sunny-land:"
npm run build > /tmp/sunny-verify-build.log 2>&1
check $? "builds"

assert_typed_scalars src ../../packages/engine.tish
check $? "every module scalar is typed (docs/perf-rules.md §1)"

# The regression that started this: the ROM must still be alive AFTER the spawn loops have run.
# Frame 47 is where it used to die, so the early frames matter as much as the late ones.
for f in 60 120 240 700 2000; do
  python3 ../../scripts/shot_check.py sunny-land.gba "$f" "120:right,300:a,320:right" > /dev/null 2>&1
  check $? "frame $f is a live picture"
done

soak_rom sunny-land.gba 9000 "120:right,300:a,320:right,600:a,620:right,900:left,1200:a,1400:right,2000:a,2100:right,3000:a,3200:right,4000:b,4200:right,5000:a,6000:right,7000:a,8000:right" > /tmp/sunny-soak.log 2>&1
check $? "9000 frames with input: no crash, no halt"

echo "sunny-land: $fails failure(s)"
exit $((fails > 0))
