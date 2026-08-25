#!/bin/bash
# prismfall — headless checks. Asserts COUNTS, never the absence of errors: a check that crashes
# prints neither ok nor FAIL. Logs are capped inline; a crashed ROM writes millions of lines.
set -u
cd "$(dirname "$0")"
ROM=prismfall.gba; SHOT=../../scripts/screenshot.sh; fails=0
check() { if [ "$2" = "$3" ]; then printf '  ok   %-40s %s\n' "$1" "$3"
  else printf '  FAIL %-40s expected %s, got %s\n' "$1" "$2" "$3"; fails=$((fails+1)); fi; }
[ -f "$ROM" ] || { echo "no $ROM — run npm run build"; exit 1; }
echo "prismfall verify"

# 1. The facility loads with the expected geometry.
n=$(GBA_SHOT_LOG=1 $SHOT $ROM /tmp/pf_v1.png 90 2>&1 | head -c 200000 | grep -ac 'facility 112x40')
check "facility loads" 1 "$n"

# 2. The gate holds: with only DAWN owned, L/R must NOT change the lens, so the lens HUD never draws.
#    (It is drawn only on the frames a switch actually happens.)
h=$(GBA_SHOT_LOG=1 $SHOT $ROM /tmp/pf_v2.png 200 "60:r,72:,100:l,112:,140:0x300,160:" 2>&1 \
    | head -c 300000 | grep -ac 'hud_text enter')
check "locked lens: no switch without a lens" 0 "$h"

# 3. The player actually moves, asserted from the SCREEN rather than from a log line. Walking right
#    for a few seconds must change what is on it — the camera follows, so a still screen means a
#    stuck player. This is the check that would have caught the truncated-speed bug, where a
#    configured 1.9 px/frame silently became 1.0 and the hero trudged.
$SHOT $ROM /tmp/pf_v_a.png 60 >/dev/null 2>&1
$SHOT $ROM /tmp/pf_v_b.png 300 "30:right,300:" >/dev/null 2>&1
moved=$(python3 -c "
from PIL import Image, ImageChops
a=Image.open('/tmp/pf_v_a.png').convert('RGB'); b=Image.open('/tmp/pf_v_b.png').convert('RGB')
d=ImageChops.difference(a,b).convert('L')
n=sum(1 for p in d.getdata() if p>16)
print(1 if n > a.width*a.height//10 else 0)
")
check "player covers ground (screen changes)" 1 "$moved"

# 4. THE FOUR-COLOUR RULE, counted off the screen in every lens state. This is the requirement the
#    whole art pipeline exists to satisfy, so it is asserted rather than trusted: sprites included,
#    boot / DAWN / DUSK / WHITE, no frame over four distinct colours.
$SHOT $ROM /tmp/pf_c1.png 60 >/dev/null 2>&1
$SHOT $ROM /tmp/pf_c2.png 180 "40:right,140:" >/dev/null 2>&1
$SHOT $ROM /tmp/pf_c3.png 210 "40:right,140:,160:r,172:" >/dev/null 2>&1
$SHOT $ROM /tmp/pf_c4.png 230 "40:right,140:,170:0x300" >/dev/null 2>&1
if python3 ../../scripts/count_screen_colours.py /tmp/pf_c1.png /tmp/pf_c2.png /tmp/pf_c3.png /tmp/pf_c4.png >/tmp/pf_cols.txt 2>&1; then
  check "four colours on screen, every lens" 1 1
else
  printf '  FAIL %-40s\n' "four colours on screen, every lens"; sed 's/^/    /' /tmp/pf_cols.txt; fails=$((fails+1))
fi

echo
if [ "$fails" -eq 0 ]; then echo "all checks passed"; else echo "$fails check(s) FAILED"; exit 1; fi
