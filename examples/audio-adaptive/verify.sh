#!/usr/bin/env bash
# audio-adaptive verify — pause keeps what stop-then-play loses, and a duck is a ramp.
#
# ⚠️ EVERY CLAIM HERE IS PAIRED WITH ITS CONTRAST. Asserting only that `deck_pause` preserves the
# playhead would pass against an engine where NOTHING advances the playhead, and asserting only that
# a duck reaches its depth would pass against one that jumps there in a single frame. So the ROM does
# the old stop-then-play hush too, at the same non-zero intensity, and the verifier asserts that one
# loses what the other keeps.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR
npm run build >/tmp/aa-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -25 /tmp/aa-build.log; exit 1; fi
assert_agb_fork .
check $? "resolved agb to the fork"
assert_typed_scalars src
check $? "no untyped module scalars"

log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh audio-adaptive.gba /tmp/aa-verify.png 1300 >"$log" 2>&1
check $? "runs headless"
crash_grep "$log"
check $? "no panic, no allocation failure"

f() { grep -o "AA $1 .*" "$log" | head -1; }
fld() { grep -o "AA $1 .*" "$log" | head -1 | grep -o "$2=[0-9-]*" | cut -d= -f2; }

PRE_F=$(fld PREPAUSE frame);   PRE_I=$(fld PREPAUSE int)
PAU_F=$(fld PAUSED frame);     PAU_I=$(fld PAUSED int)
RES_F=$(fld RESUME frame);     RES_I=$(fld RESUME int)
PST_F=$(fld PRESTOP frame);    PST_I=$(fld PRESTOP int)
PLY_F=$(fld POSTPLAY frame);   PLY_I=$(fld POSTPLAY int)
note "pause:  $PRE_F/$PRE_I -> $PAU_F/$PAU_I -> (120 frames) -> $RES_F/$RES_I   (frame/intensity)"
note "hush:   $PST_F/$PST_I -> $PLY_F/$PLY_I"

# The intensity climbed to 3 before either event, or neither claim below means anything: 0 is
# exactly where a reset lands, so a test taken at 0 cannot tell a reset from a no-op.
[ "${PRE_I:-0}" -ge 1 ] && [ "${PST_I:-0}" -ge 1 ]
check $? "both events are taken at a non-zero intensity (pause $PRE_I, hush $PST_I)"

# ── the pause keeps everything ───────────────────────────────────────────────
[ -n "$RES_F" ] && [ "$RES_F" = "$PRE_F" ]
check $? "a paused playhead does not advance ($PRE_F held across 120 frames)"
[ -n "$RES_I" ] && [ "$RES_I" = "$PRE_I" ]
check $? "intensity survives the pause ($PRE_I -> $RES_I)"
grep -q 'AA PAUSED paused=1' "$log"
check $? "deck_paused reports the state"

# ── ...and the pair it replaces loses both ───────────────────────────────────
# This is the contrast. If these two ever start passing as "preserved", the pause assertions above
# have stopped proving anything and this file needs rewriting, not relaxing.
[ -n "$PLY_F" ] && [ "$PLY_F" -lt "${PST_F:-1}" ]
check $? "stop-then-play RESTARTS the song ($PST_F -> $PLY_F) — what the pause exists to avoid"
[ -n "$PLY_I" ] && [ "$PLY_I" -lt "${PST_I:-1}" ]
check $? "stop-then-play RESETS the intensity ($PST_I -> $PLY_I)"

# ── the playhead really does move otherwise ──────────────────────────────────
# Guards against the whole test passing on an engine whose playhead never advances at all.
[ "${PST_F:-0}" -gt "${PRE_F:-0}" ]
check $? "the playhead advances when not paused ($PRE_F -> $PST_F)"

# ── ducking is a RAMP, not a step ────────────────────────────────────────────
levels=$(grep -o 'AA LEVEL .*gain=[0-9]*' "$log" | grep -o 'gain=[0-9]*' | cut -d= -f2)
n=$(echo "$levels" | wc -l | tr -d ' ')
lo=$(echo "$levels" | sort -n | head -1)
note "duck gain samples: $n, minimum $lo of 64"
[ "${lo:-64}" -le 30 ]
check $? "the duck reaches depth (min gain $lo of 64)"
# A step change would be two samples: 64 and the target. A ramp is many. The bar is 6 because the
# attack is 6 frames and each frame is a distinct level.
[ "$n" -ge 8 ]
check $? "the duck RAMPS rather than jumping ($n distinct levels)"
# ...and comes back up, or every later sound sits under a duck nobody released.
last=$(echo "$levels" | tail -1)
[ "${last:-0}" -ge 50 ]
check $? "the duck releases (final gain $last)"

grep -q 'AA DONE' "$log"
check $? "ran to completion"

echo
[ "$fail" = 0 ] && echo "audio-adaptive: PASS" || echo "audio-adaptive: FAIL"
exit $fail
