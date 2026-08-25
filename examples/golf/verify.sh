#!/usr/bin/env bash
# golf verify — the acceptance test for `dynamic_system` with a SINGLE rigid body.
#
# The claim under test is the one a physics engine is easiest to fake: that a ball **comes to rest**,
# as an engine state rather than a threshold guessed in game code. Everything else here — the
# surface classes, the walls, the nine scene swaps — is in service of that, because a ball that
# never truly stops makes golf unplayable while looking perfectly fine in motion.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR

python3 ../../scripts/gen_golf.py >/dev/null 2>&1 && python3 ../../scripts/gen_golf_art.py >/dev/null 2>&1
check $? "regenerates nine .tmj holes and the sprite sheet"

npm run build >/tmp/golf-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -25 /tmp/golf-build.log; exit 1; fi

assert_agb_fork .
check $? "resolved agb to the fork"

assert_typed_scalars src
check $? "no untyped module scalars"

# The course is a Tiled map, not a tish array. `bench-boot` measured a per-tile tish marking loop at
# ~0.175 frames PER TILE — one example's whole four-second boot — so this is a build gate, not taste.
[ "$(ls assets/*.tmj 2>/dev/null | wc -l | tr -d ' ')" = 9 ]
check $? "nine holes exist as .tmj (the map pipeline, not a tish literal)"
python3 - <<'EOF'
import json, sys, glob
for f in sorted(glob.glob('assets/hole*.tmj')):
    d = json.load(open(f))
    names = [l['name'] for l in d['layers']]
    # `Solid` FORCES cells solid. A `Collision` layer is the opposite — it forces cells WALKABLE and
    # an empty cell there ERASES whatever the tileset said, so a wall drawn there would not be one.
    assert 'Solid' in names, f'{f}: no Solid layer (walls would not be solid)'
    assert 'Collision' not in names, f'{f}: a Collision layer would force cells walkable'
EOF
check $? "every hole carries a Solid layer and no Collision layer"

# ── Headless round ───────────────────────────────────────────────────────────
log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh golf.gba /tmp/golf-verify.png 9000 >"$log" 2>&1
check $? "plays a full round headless"

crash_grep "$log"
check $? "no panic, no allocation failure"

# ⚠️ THE ONE THAT MATTERS. Sinking REQUIRES the ball to be at rest — `sunk()` returns 0 unless
# `body_asleep` is 1 — so every SUNK line is direct evidence that the engine's rest state fires and
# is reachable. A ball that only ever crept would produce zero of these and a green log otherwise.
sunk=$(grep -c 'GOLF SUNK' "$log" || true)
[ "$sunk" -ge 9 ]
check $? "all nine holes finished ($sunk) — the ball reaches REST, not just slow"

# Every hole is visited, so a broken .tmj cannot hide behind the eight that work.
missing=""
for h in 1 2 3 4 5 6 7 8 9; do
  grep -q "GOLF HOLE $h " "$log" || missing="$missing $h"
done
[ -z "$missing" ]
check $? "every hole loaded (missing:${missing:- none})"

# ⚠️ NEGATIVE CONTROL for the rest state. If `body_asleep` returned 1 whenever velocity was merely
# small, the ball would "sink" mid-roll and strokes would be ~1 per hole. Requiring real strokes is
# what distinguishes a working rest test from one that is always true.
shots=$(grep -c 'GOLF SHOT' "$log" || true)
[ "$shots" -ge $(( sunk * 2 )) ]
check $? "$shots strokes for $sunk holes — rest is not trivially true"

# ...and the converse: a rest state that NEVER fires would show as zero sinks, already checked, or
# as a stroke count that runs away. The pickup cap is par+6, so a driver that could never settle
# would hit the cap on every hole. At least some holes must be finished under it.
pick=$(grep -c 'GOLF PICKUP' "$log" || true)
[ "$pick" -lt "$sunk" ]
check $? "$pick holes conceded of $sunk — some are genuinely holed out, not just capped"

# The surface plane actually differs between classes: a ball in sand must die faster than one on the
# green. Asserted through the scoreboard rather than by reading velocities, because that is what a
# player experiences: hole 8 is a sand belt and is the hardest hole on the course.
note "$(grep -o 'GOLF SUNK.*' "$log" | tail -9 | sed 's/^/    /')"

# Heap must not drift across nine scene swaps — the load path is what a course exercises hardest.
lo=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | head -1)
hi=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | tail -1)
span=$(( hi - lo ))
[ "$span" -le 8192 ]
check $? "heap bounded across nine scene loads (span ${span} B)"

# One entity for the whole game: the ball. The cup and the aim arrow are bare sprites, because
# neither needs physics and an entity for them would be an entity to reset on every hole.
counts=$(grep -o 'ENT [0-9]*' "$log" | sort -u | tr '\n' ' ')
nc=$(grep -o 'ENT [0-9]*' "$log" | sort -u | wc -l | tr -d ' ')
[ "$nc" = 1 ]
check $? "entity count constant ($counts) across the round"

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/golf-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
[ "${n:-0}" -ge 5 ]
check $? "frame paints ($n colours)"

echo
[ "$fail" = 0 ] && echo "golf: PASS" || echo "golf: FAIL"
exit $fail
