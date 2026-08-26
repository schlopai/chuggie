#!/usr/bin/env bash
# Headless ANIMATED GIF of a GBA ROM (or a .tish source, which it builds first) via libmgba — the
# moving counterpart to scripts/screenshot.sh, captured the same way: NO emulator window, NO macOS
# Screen-Recording/Accessibility permission. A still cannot show movement, animation, a transition
# or a particle effect; this can, and it gets the whole clip out of a SINGLE emulator run.
#
# Usage:  scripts/gif.sh <rom.gba | src/main.tish> [out.gif] [frames] [keys]
#   frames : frames to run in total (default 300 ~ 5s at 59.7fps).
#   keys   : held keys ("a,start") or a frame schedule ("90:a,120:") — see tools/gba-shot.c.
#   env    : GIF_FROM=<n>        first frame to record (default 60 — skips the boot/logo frames)
#            GIF_EVERY=<n>       record one frame in n (default 3, i.e. ~20fps playback)
#            GIF_SCALE=<n>       integer upscale, nearest-neighbour (default 2 -> 480x320)
#            GIF_MAX_FRAMES=<n>  cap on recorded frames (default 300)
#            plus MGBA_PREFIX / TISH / GBA_SHOT_* exactly as scripts/screenshot.sh takes them.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"          # repo root
. "$here/scripts/shot_common.sh"
input="${1:?usage: gif.sh <rom.gba|main.tish> [out.gif] [frames] [keys]}"
out="${2:-screenshot.gif}"
frames="${3:-300}"
keys="${4:-}"

from="${GIF_FROM:-60}"
every="${GIF_EVERY:-3}"
scale="${GIF_SCALE:-2}"
max_frames="${GIF_MAX_FRAMES:-300}"

mgba_prefix="$(find_mgba_prefix)"
shot="$(ensure_gba_shot "$here" "$mgba_prefix")"
rom="$(resolve_rom "$input")"

# One run, every Nth frame straight to disk. (scripts/frame_stability.py re-boots the emulator per
# frame; for a 300-frame clip that would be 300 boots.) The explicit X's template is portable to
# both BSD and GNU mktemp — see the same note in screenshot.sh.
seq_dir="$(mktemp -d "${TMPDIR:-/tmp}/gbagif.XXXXXX")"
ppm="$seq_dir/_final.ppm"
trap 'rm -rf "$seq_dir"' EXIT

GBA_SHOT_SEQ="$seq_dir" GBA_SHOT_SEQ_FROM="$from" GBA_SHOT_SEQ_EVERY="$every" \
  GBA_SHOT_SEQ_MAX="$max_frames" "$shot" "$rom" "$ppm" "$frames" "$keys"

# `f*.ppm` deliberately excludes the throwaway _final.ppm above (gba-shot names frames fNNNNN). Sorted by name = playback order,
# because gba-shot numbers by frames emitted.
ppms=("$seq_dir"/f*.ppm)
# Two ways to end up with nothing: a window that starts past the end of the run, or a screen that
# is a flat colour for the whole window (gba-shot will not open a clip on a blank frame).
[ -e "${ppms[0]}" ] || { echo "error: gba-shot recorded no frames — the screen is blank for the whole window, or GIF_FROM ($from) is past the end of the $frames-frame run." >&2; exit 3; }

# The GBA runs at 59.727fps, so one recorded frame in $every lasts every/59.727 seconds. GIF stores
# delays in centiseconds, and a 0/1cs delay is the "as fast as possible" special case browsers
# reinterpret — so floor at 2.
delay_cs="$(awk -v e="$every" 'BEGIN { d = int(e * 100 / 59.727 + 0.5); if (d < 2) d = 2; print d }')"

if python3 -c "import PIL" >/dev/null 2>&1; then
  python3 "$here/scripts/ppm_to_gif.py" --out "$out" --delay-cs "$delay_cs" --scale "$scale" "${ppms[@]}"
elif command -v magick >/dev/null 2>&1 || command -v convert >/dev/null 2>&1; then
  im="$(command -v magick || command -v convert)"
  "$im" -delay "$delay_cs" -loop 0 "${ppms[@]}" -scale "$((scale * 100))%" -layers OptimizePlus "$out"
else
  echo "error: no GIF encoder — install Pillow (pip install pillow) or ImageMagick." >&2
  exit 1
fi
echo "wrote $out (${#ppms[@]} frames, ${delay_cs}cs each)"
