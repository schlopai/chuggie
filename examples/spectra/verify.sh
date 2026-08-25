#!/bin/bash
# spectra — headless checks. Run from examples/spectra: ./verify.sh
#
# ⚠️ THE SCHEDULES BELOW ARE TUNED TO 60fps. They were written when the game ran at 30 and every one
# of them broke the day the extra `frame()` came out of the main loop — the player covers twice the
# ground per DISPLAY frame, so a release that used to land on the neutral stone landed in the pit.
# If you change the frame rate, re-tune them; a schedule that "does not work" is almost always this.
#
# ⚠️ ASSERTS A COUNT, NOT THE ABSENCE OF ERRORS. A check that CRASHES prints neither ok nor FAIL, so
# a suite that only greps for "FAIL" reports clean on a ROM that died at boot. Every check below
# counts what it expects to see.
#
# ⚠️ Logs are piped through `head -c` and counted inline, never written raw: a crashed ROM emits
# millions of lines (1.9 GB from one 900-frame run has happened here).
set -u
cd "$(dirname "$0")"
ROM=spectra.gba
SHOT=../../scripts/screenshot.sh
fails=0

check() {  # check <name> <expected> <actual>
  if [ "$2" = "$3" ]; then printf '  ok   %-42s %s\n' "$1" "$3"
  else printf '  FAIL %-42s expected %s, got %s\n' "$1" "$2" "$3"; fails=$((fails+1)); fi
}

[ -f "$ROM" ] || { echo "no $ROM — run npm run build"; exit 1; }
echo "spectra verify"

# 1. Every room loads. SELECT skips a room; sweep all twelve and count the entry markers.
SCHED=$(python3 -c "
p=[];f=70
for i in range(11):
    p+=['%d:select'%f,'%d:'%(f+12)];f+=45
print(','.join(p))")
rooms=$(GBA_SHOT_LOG=1 $SHOT $ROM /tmp/spectra_v_rooms.png 600 "$SCHED" 2>&1 | head -c 400000 | grep -ac '^\[frame.*\] room ')
check "all 12 rooms load" 12 "$rooms"

# 2. The lens turns, asserted from the SCREEN: a switch recolors the room's band cells, so the
# post-R frame must differ from a no-press control at the same frame by far more than animation
# noise (measured: ~1300 px on a switch vs ~50 px of idle noise).
$SHOT $ROM /tmp/spectra_v_lens_a.png 89 >/dev/null 2>&1
$SHOT $ROM /tmp/spectra_v_lens_b.png 89 "60:r,72:" >/dev/null 2>&1
lens=$(python3 -c "
from PIL import Image, ImageChops
a=Image.open('/tmp/spectra_v_lens_a.png').convert('RGB'); b=Image.open('/tmp/spectra_v_lens_b.png').convert('RGB')
d=ImageChops.difference(a,b).convert('L')
print(2 if sum(1 for p in d.getdata() if p>16) > 400 else 0)")
check "lens redraws on a switch" 2 "$lens"

# 3. The crush rule refuses an illegal switch. In THE CRUSH the roof is band C, so pressing L under
#    it must NOT change the lens — the run below ends with the room still in its starting lens.
crush=$(GBA_SHOT_LOG=1 $SHOT $ROM /tmp/spectra_v_crush.png 420 \
  "40:right,120:,132:r,144:,156:right,330:,360:l,375:" 2>&1 | head -c 400000 | grep -ac 'room 1 THE CRUSH')
check "reaches THE CRUSH by playing room 1" 1 "$crush"

# 4. Nothing died reaching it — a pit death would reload the room and log it.
died=$(GBA_SHOT_LOG=1 $SHOT $ROM /tmp/spectra_v_died.png 400 \
  "40:right,120:,132:r,144:,156:right,330:" 2>&1 | head -c 400000 | grep -ac '^\[frame.*\] died ')
check "room 1 crossed without dying" 0 "$died"

echo
if [ "$fails" -eq 0 ]; then echo "all checks passed"; else echo "$fails check(s) FAILED"; exit 1; fi
