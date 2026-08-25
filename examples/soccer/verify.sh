#!/usr/bin/env bash
# soccer verify — the acceptance test for disc-vs-disc contact at N bodies.
#
# golf proves one disc integrates, bounces and rests. This proves what happens when discs meet each
# other, which is the half of a physics engine that is easy to make PRESENT and hard to make STABLE.
#
# The assertion that matters most is the containment one. A resolver can look completely healthy on
# screen while quietly walking a body through a wall a fraction of a pixel at a time — that is
# exactly what this example found, three times, before the cause turned out to be `movement_system`
# integrating the same body a second time with no collision check.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR

python3 ../../scripts/gen_soccer.py >/dev/null 2>&1
check $? "regenerates the pitch and the sprite sheet"

npm run build >/tmp/soccer-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -25 /tmp/soccer-build.log; exit 1; fi

assert_agb_fork .
check $? "resolved agb to the fork"
assert_typed_scalars src
check $? "no untyped module scalars"

python3 - <<'EOF'
import json
d = json.load(open('assets/pitch.tmj'))
names = [l['name'] for l in d['layers']]
assert 'Solid' in names, 'no Solid layer'
assert 'Collision' not in names, 'a Collision layer would force cells walkable'
EOF
check $? "pitch carries a Solid layer and no Collision layer"

log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh soccer.gba /tmp/soccer-verify.png 6000 >"$log" 2>&1
check $? "plays 6000 frames headless"
crash_grep "$log"
check $? "no panic, no allocation failure"

# ⚠️⚠️ CONTAINMENT. The pitch is 22x14 tiles = 352x224 px. The ball must never be outside it.
#
# This is the assertion this example exists for. A ball leaving the field is not a gameplay quirk,
# it is a body that got through a solid tile, and it happened here in three different ways before
# the real cause was found. It is checked as an ABSOLUTE BOUND rather than "looks about right",
# because the drift was a fraction of a pixel per frame and any tolerance would have hidden it.
python3 - "$log" <<'EOF'
import re, sys
bad = []
for m in re.finditer(r'bx=(-?[\d.]+) by=(-?[\d.]+)', open(sys.argv[1]).read()):
    x, y = float(m.group(1)), float(m.group(2))
    if not (-8 <= x <= 360) or not (-8 <= y <= 232):
        bad.append((x, y))
if bad:
    print(f"  ball left the pitch {len(bad)}x, e.g. {bad[:3]}")
    sys.exit(1)
EOF
check $? "the ball never leaves the pitch (no body walks through a solid tile)"

# ⚠️ NO INTERPENETRATION, in SQUARED EUCLIDEAN. Manhattan is always >= Euclidean, so a Manhattan
# separation reads healthy for two discs that are overlapping — the first version of this audit did
# exactly that. Players are diameter 10, so touching is a centre distance of 10, i.e. d2 = 100.
# The bar is 49 (7px): contact allows a little penetration before it is resolved, but half a
# diameter would mean bodies sinking into each other.
worst=$(grep -o 'SOC SEP d2min=[0-9]*' "$log" | cut -d= -f2 | sort -n | head -1)
[ -n "$worst" ] && [ "$worst" -ge 49 ]
check $? "players never sink into one another (min d2 ${worst:-?}, touching is 100)"

# Goals happen, so contact actually transfers momentum rather than merely separating bodies.
goals=$(grep -c 'SOC GOAL' "$log" || true)
[ "$goals" -ge 5 ]
check $? "$goals goals — contact moves the ball, it does not just unstick it"

# ⚠️ THE RANK SPLIT, observed rather than asserted from the source. The ball is rank 0 and the
# players rank 1, so a player meeting the ball moves the BALL. If that were inverted the ball would
# shove players around, never travel far, and essentially no goals would be scored — which the line
# above would catch — while the ball's speed stayed near zero. So: the ball must reach real speed.
fast=$(grep -o 'bv2=[0-9]*' "$log" | cut -d= -f2 | sort -n | tail -1)
[ -n "$fast" ] && [ "$fast" -ge 1000 ]
check $? "the ball reaches real speed (max v2 ${fast:-?}) — players move it, not the reverse"

# `body_last_hit` distinguishes a goal from an own goal. Own goals must occur, or the field is
# never read and the assertion above it proves nothing about attribution.
own=$(grep -c 'own=1' "$log" || true)
[ "$own" -ge 1 ]
check $? "$own own goals — body_last_hit attributes the toucher"

# A wedged ball is a legal outcome, but a game that only ever wedges is a stalemate, not a test.
dead=$(grep -c 'SOC DEADBALL' "$log" || true)
[ "$dead" -lt "$goals" ]
check $? "$dead dead balls against $goals goals — play is live, not a stalemate"

counts=$(grep -o 'ENT [0-9]*' "$log" | sort -u | tr '\n' ' ')
[ "$(grep -o 'ENT [0-9]*' "$log" | sort -u | wc -l | tr -d ' ')" = 1 ]
check $? "entity count constant ($counts) — 1 ball + 6 players, nothing spawns"

lo=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | head -1)
hi=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | tail -1)
[ -n "$lo" ] && [ $(( hi - lo )) -le 4096 ]
check $? "heap flat across the match (span $(( ${hi:-0} - ${lo:-0} )) B)"

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/soccer-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
[ "${n:-0}" -ge 5 ]
check $? "frame paints ($n colours)"

echo
[ "$fail" = 0 ] && echo "soccer: PASS" || echo "soccer: FAIL"
exit $fail
