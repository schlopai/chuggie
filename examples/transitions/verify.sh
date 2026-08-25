#!/usr/bin/env bash
# transitions — the checks that actually catch this example breaking.
#
# A transition demo is unusually easy to verify wrongly, because "the screen is black" is both the
# correct state at the middle of every effect and the failure state of nearly every bug in one. So
# the screenshot checks below are all PARTIAL-COVERAGE checks: a frame is only a pass if the screen
# is neither fully hidden nor fully visible. A test that accepted black would have passed on the
# stuck-curtain bug, the 87-frame scene-build stall and the exhausted tile allocator alike.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
# shellcheck source=../../scripts/verify_common.sh
source $ROOT/scripts/verify_common.sh
fail=0
src=src/main.tish
rom=transitions.gba

assert_agb_fork || fail=1
assert_typed_scalars $src || fail=1

[ -f $rom ] || npm run build >/tmp/transitions-vbuild.log 2>&1 || true
[ -f $rom ] || { echo "  FAIL $rom not built:"; tail -20 /tmp/transitions-vbuild.log; exit 1; }

# ── Source-level rules ───────────────────────────────────────────────────────────────────────────
grep -q "sceneStart(" $src && grep -q "sceneStep()" $src \
  && echo "  ok   driven by packages/scene, not a hand-rolled ramp" \
  || { echo "  FAIL no longer uses the scene machine"; fail=1; }
grep -q "sceneSetTransition(" $src \
  && echo "  ok   exercises the transition hook rather than the default fade" \
  || { echo "  FAIL never selects an effect"; fail=1; }
# ⚠️ sprite_new inside enter() leaks the 128-object budget on every visit — the lifecycle rule
# iso-transitions documents, restated here because this ROM crosses far more often than that one.
if grep -A 4 "enter: () =>" $src | grep -q "sprite_new"; then
  echo "  FAIL a scene allocates sprites in enter()"; fail=1
else
  echo "  ok   no sprite allocation inside a scene's enter()"
fi
# ⚠️ Rebuilding the board in enter() cost 87 frames and made every effect look like it hung at its
# midpoint. The board is built once at boot; scenes only recolour.
if grep -A 4 "enter: () =>" $src | grep -q "terrain_disc"; then
  echo "  FAIL a scene rebuilds the board in enter() — the black hold will stretch to a second"; fail=1
else
  echo "  ok   enter() recolours rather than rebuilding the board"
fi

# ── Runtime ──────────────────────────────────────────────────────────────────────────────────────
# One full pass over all eleven effects plus a second lap, so anything that leaks per crossing
# (canvas tiles, OBJ entries, windows left on) has somewhere to show up.
soak_rom $rom 2400 || fail=1

# Every effect must be reached and named. A silently skipped effect is the failure this catches:
# the cycle is modulo TR_COUNT, so an effect that panics on entry simply never prints.
log=$(mktemp)
GBA_SHOT_LOG=1 $ROOT/scripts/screenshot.sh $rom /dev/null 2400 >"$log" 2>&1 || true
for fx in fade white iris "iris at" box wipe curtain bars mosaic rain checker; do
  if grep -q "TRANSITIONS effect .* $fx" "$log"; then
    echo "  ok   effect reached: $fx"
  else
    echo "  FAIL effect never reached: $fx"; fail=1
  fi
done
# ⚠️ A check that CRASHES prints neither ok nor FAIL, so the grep for a panic is explicit rather
# than left to soak_rom's regex alone — "Ran out of video RAM for tiles" is the one this example
# actually hit, twice, and it is not a crash string.
if grep -q "Ran out of video RAM" "$log"; then
  echo "  FAIL the tile allocator ran dry — a software curtain is allocating per-cell tiles"; fail=1
else
  echo "  ok   the tile allocator survived both software effects"
fi
# ── Mid-transition frames ────────────────────────────────────────────────────────────────────────
# PARTIAL coverage, checked by PNG size: a fully-hidden frame compresses to well under 1.5KB and a
# fully-visible board to about 5.4KB, so a real half-closed wipe lands between the two. Crude, and
# exactly strong enough — it is the one property every effect shares and no failure mode fakes.
shot_partial() {
  local frame=$1 name=$2 png sz
  png=$(mktemp -u).png
  $ROOT/scripts/screenshot.sh $rom "$png" "$frame" >/dev/null 2>&1 || true
  if [ ! -f "$png" ]; then echo "  FAIL $name: no screenshot at frame $frame"; fail=1; return; fi
  sz=$(stat -f%z "$png" 2>/dev/null || stat -c%s "$png")
  rm -f "$png"
  # ⚠️ The floor is 1200, not 1500. A nearly-closed box wipe compresses to about 1.4KB — genuinely
  # partial, and it fails a tighter bound. Fully hidden is under 1000 on every effect here, so 1200
  # separates the two without calling a real frame a failure.
  if [ "$sz" -gt 1200 ] && [ "$sz" -lt 5200 ]; then
    echo "  ok   $name is partially covered at frame $frame ($sz b)"
  else
    echo "  FAIL $name at frame $frame is fully hidden or fully visible ($sz b)"; fail=1
  fi
}
# Anchors are DERIVED from each effect's logged entry frame plus a mid-close offset, because the
# absolute cadence shifts between builds (a fresh ROM enters its cycle a few frames off a stale
# one, and a hardcoded frame lands fully-hidden instead of mid-close).
fx_start() { grep "TRANSITIONS effect .* $1" "$log" | head -1 | grep -oE "frame [0-9]+" | grep -oE "[0-9]+"; }
shot_partial "$(( $(fx_start iris)    + 62 ))" "iris"
shot_partial "$(( $(fx_start box)     + 74 ))" "box"
shot_partial "$(( $(fx_start wipe)    + 74 ))" "wipe"
shot_partial "$(( $(fx_start curtain) + 74 ))" "curtain"
shot_partial "$(( $(fx_start bars)    + 74 ))" "bars"
shot_partial "$(( $(fx_start mosaic)  + 80 ))" "mosaic"
shot_partial "$(( $(fx_start rain)    + 78 ))" "rain"
shot_partial "$(( $(fx_start checker) + 76 ))" "checker"
rm -f "$log"

# ⚠️ The curtain must come OFF. This is the bug that shipped twice: the software effects only ever
# painted more curtain, so the screen stayed black through the fade-in and through the dwell after
# it. Frame 1090 sits in the dwell AFTER the rain crossing — it must show a full board.
png=$(mktemp -u).png
$ROOT/scripts/screenshot.sh $rom "$png" 1090 >/dev/null 2>&1 || true
sz=$(stat -f%z "$png" 2>/dev/null || stat -c%s "$png" 2>/dev/null || echo 0)
rm -f "$png"
if [ "$sz" -gt 5200 ]; then
  echo "  ok   the software curtain lifts after its transition ($sz b)"
else
  echo "  FAIL the curtain is still up after the rain transition ($sz b)"; fail=1
fi

exit $fail
