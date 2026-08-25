#!/usr/bin/env bash
# bench-boot runner — build the two ROMs, then print WHERE the pre-first-frame time goes.
#
#   ./run.sh              # stage attribution
#   ./run.sh --games      # ...and every shipped example's first-paint frame, for context
#
# The unit is EMULATED FRAMES (59.7/s). mGBA runs a fixed slice of CPU per `runFrame` regardless of
# whether the ROM ever calls `frame()`, so a boot that spans 465 of them really did consume 465
# frames' worth of CPU — which is why this works at all, and why the numbers translate directly to
# what a player waits through on hardware.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
root="$(cd ../.. && pwd)"
shot="$root/scripts/screenshot.sh"

VARIANTS="v_font v_sheets v_scenes v_native v_named68 v_ui v_dialog v_engine v_packages"

need_build=0
for f in bench-boot floor $VARIANTS; do
  [ -f "$f.gba" ] || need_build=1
done
for s in src/*.tish; do
  [ "$s" -nt bench-boot.gba ] && need_build=1
done
if [ "$need_build" = 1 ]; then
  echo "building ..." >&2
  npm run --silent build
fi

# Emit "<frame> <stage>" for each BB marker in a ROM's log.
markers() {
  GBA_SHOT_LOG=1 "$shot" "$1" /tmp/bench-boot.png "${2:-1200}" 2>&1 \
    | sed -n 's/^\[frame \([0-9]*\)\] BB \(.*\)$/\1 \2/p'
}
# The frame a ROM reaches its first marker — i.e. the whole cost of module init.
first_marker() { markers "$1" 900 | awk 'NR==1{print $1; exit}'; }

floor_frames="$(first_marker floor.gba)"
: "${floor_frames:=0}"

# The frame a ROM's picture first changes. Needed for agb-floor, which is pure agb and so has no tish
# `log` to emit a BB marker — and used for floor.gba too, because comparing a marker against a paint
# would put two different events in one column.
first_paint() {
  GBA_SHOT_TRACE=1 "$shot" "$1" /tmp/bench-boot.png 900 2>&1 \
    | sed -n 's/^\[frame \([0-9]*\)\] screen .* (\([0-9]*\) px.*/\1 \2/p' \
    | awk '$2>2000{print $1; exit}'
}

# What the LANGUAGE costs, as opposed to what the games do with it. Both ROMs set the same backdrop
# and present a frame; agb-floor just does it without tish. Build it with `npm run agb-floor`.
if [ -f agb-floor/agb-floor.gba ]; then
  echo
  echo "== the floor under the floor: what starting tish costs =="
  agb_paint="$(first_paint agb-floor/agb-floor.gba)"; : "${agb_paint:=0}"
  tish_paint="$(first_paint floor.gba)"; : "${tish_paint:=0}"
  printf '%-24s %8s %9s\n' ROM PAINT SECONDS
  printf '%-24s %8d %9.2f\n' "agb-floor (pure agb)" "$agb_paint" "$(echo "$agb_paint/59.7" | bc -l)"
  printf '%-24s %8d %9.2f\n' "floor.tish (runtime)" "$tish_paint" "$(echo "$tish_paint/59.7" | bc -l)"
  printf '%-24s %8d %9.2f\n' "  = tish startup" "$((tish_paint - agb_paint))" \
    "$(echo "($tish_paint - $agb_paint)/59.7" | bc -l)"
fi

echo
echo "== module init: which IMPORTS cost what =="
echo "   (each ROM is the floor plus one group of imports and nothing else;"
echo "    COST is charged against the floor, so it is the imports' own price)"
echo
printf '%-24s %8s %8s %9s\n' VARIANT AT COST SECONDS
printf '%-24s %8s %8s %9s\n' ------------------------ -------- -------- ---------
printf '%-24s %8d %8d %9.2f\n' "floor (no imports)" "$floor_frames" "$floor_frames" "$(echo "$floor_frames/59.7" | bc -l)"
for v in $VARIANTS; do
  [ -f "$v.gba" ] || continue
  at="$(first_marker "$v.gba")"; : "${at:=0}"
  printf '%-24s %8d %8d %9.2f\n' "${v#v_}" "$at" "$((at - floor_frames))" "$(echo "($at - $floor_frames)/59.7" | bc -l)"
done

echo
echo "== the staged boot: what a full game pays, in order =="
echo
printf '%-18s %8s %8s %9s\n' STAGE AT COST SECONDS
printf '%-18s %8s %8s %9s\n' ------------------ -------- -------- ---------
markers bench-boot.gba | awk -v floor="$floor_frames" '
  NR==1 { prev = floor }                      # stage 1 is module init: charge it from the floor
  { d = $1 - prev; printf "%-18s %8d %8d %9.2f\n", $2, $1, d, d/59.7; prev = $1 }
'

if [ "${1:-}" = "--games" ]; then
  echo
  printf '%-20s %10s %9s   %s\n' EXAMPLE FIRST-PAINT SECONDS ROM
  printf '%-20s %10s %9s   %s\n' -------------------- ---------- --------- ---
  for rom in "$root"/examples/*/*.gba; do
    name="$(basename "$(dirname "$rom")")"
    [ "$(basename "$rom" .gba)" = "$name" ] || continue      # skip scratch/variant ROMs
    f="$(GBA_SHOT_TRACE=1 "$root/tools/gba-shot" "$rom" /tmp/bb.ppm 900 2>&1 \
        | sed -n 's/^\[frame \([0-9]*\)\] screen .* (\([0-9]*\) px.*/\1 \2/p' \
        | awk '$2>2000{print $1; exit}')"
    sz="$(ls -l "$rom" | awk '{printf "%.2fMB", $5/1048576}')"
    printf '%-20s %10s %9.2f   %s\n' "$name" "${f:-none}" "$(echo "${f:-0}/59.7" | bc -l)" "$sz"
  done | sort -k2 -n
fi
