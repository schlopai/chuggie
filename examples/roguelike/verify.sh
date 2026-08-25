#!/usr/bin/env bash
# roguelike verify — the cartridge's generator against a Python oracle, seed for seed.
#
# ⚠️ THIS IS THE ONLY KIND OF TEST A PROCEDURAL GENERATOR CAN HAVE. Looking at the output tells you
# almost nothing: a broken dungeon and a good one are both "some rooms". And a generator bug is
# often seed-dependent, so it reproduces for one player and not the next.
#
# So `scripts/procgen/` reproduces `packages/dungeon.tish` draw for draw, and the ROM reports the
# shape of 48 dungeons at boot. This script re-derives all 48 in Python and diffs them. A single
# reordered `rngBelow` shows up here immediately and nowhere else.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR
python3 ../../scripts/gen_roguelike.py >/dev/null 2>&1
check $? "regenerates the tileset and actor sheet"
npm run build >/tmp/rl-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -25 /tmp/rl-build.log; exit 1; fi
assert_agb_fork .
check $? "resolved agb to the fork"
assert_typed_scalars src
check $? "no untyped module scalars"

log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh roguelike.gba /tmp/rl-verify.png 1400 >"$log" 2>&1
check $? "runs headless"
crash_grep "$log"
check $? "no panic, no allocation failure"
grep -q 'RL SWEEP DONE' "$log"
check $? "generated the whole seed sweep"

# ── THE ORACLE DIFF ──────────────────────────────────────────────────────────
python3 - "$log" <<'EOF'
import re, sys
sys.path.insert(0, '../../scripts')
from procgen import Rng, rooms, validate

W, H, DEPTH = 40, 26, 3
rows = re.findall(
    r'RL SWEEP seed=(\d+) rooms=(\d+) floors=(\d+) r0x=(-?\d+) r0y=(-?\d+) r0w=(\d+) r0h=(\d+)',
    open(sys.argv[1]).read())
if len(rows) < 16:
    print(f"  only {len(rows)} sweep rows — expected 16")
    sys.exit(1)

bad = []
for seed, rn, fl, x, y, w, h in rows:
    seed, rn, fl, x, y, w, h = map(int, (seed, rn, fl, x, y, w, h))
    grid, rs = rooms.generate(Rng(seed), W, H, DEPTH)
    want = (len(rs), validate.floor_count(grid), rs[0][0], rs[0][1], rs[0][2], rs[0][3])
    got = (rn, fl, x, y, w, h)
    if want != got:
        bad.append((seed, want, got))

print(f"  {len(rows)} seeds compared against the Python oracle")
if bad:
    for seed, want, got in bad[:5]:
        print(f"  seed {seed}: python {want} != rom {got}")
    sys.exit(1)
EOF
check $? "every generated dungeon matches the Python oracle exactly"

# ⚠️ NEGATIVE CONTROL. The diff above would also pass if the ROM and the oracle were both
# degenerate — e.g. if every seed produced the identical dungeon, or none produced any floor at all.
# So: the sweep must actually VARY, and every dungeon must be non-trivial.
uniq=$(grep -o 'RL SWEEP .*floors=[0-9]*' "$log" | grep -o 'floors=[0-9]*' | sort -u | wc -l | tr -d ' ')
[ "${uniq:-0}" -ge 8 ]
check $? "the seeds produce different dungeons ($uniq distinct floor counts)"
minf=$(grep -o 'RL SWEEP .*floors=[0-9]*' "$log" | grep -o 'floors=[0-9]*' | cut -d= -f2 | sort -n | head -1)
[ "${minf:-0}" -ge 80 ]
check $? "every dungeon has real floor area (smallest $minf cells)"

# ...and Python's own view of them is connected, so the generator cannot ship a level with an
# unreachable half — which looks perfectly fine in a screenshot.
python3 - <<'EOF'
import sys
sys.path.insert(0, '../../scripts')
from procgen import Rng, rooms, validate
bad = [s for s in (1000 + i * 137 for i in range(16))
       if not validate.connected(*(lambda g, r: (g, 40, 26))(*rooms.generate(Rng(s), 40, 26, 3)))]
if bad:
    print(f"  {len(bad)} seeds generate a disconnected dungeon, e.g. {bad[:3]}")
    sys.exit(1)
EOF
check $? "every dungeon in the sweep is fully connected (no unreachable rooms)"

# The played levels must agree with the swept ones — the render path must not perturb generation.
python3 - "$log" <<'EOF'
import re, sys
txt = open(sys.argv[1]).read()
sweep = dict((int(s), (int(r), int(f))) for s, r, f in
             re.findall(r'RL SWEEP seed=(\d+) rooms=(\d+) floors=(\d+)', txt))
played = re.findall(r'RL LEVEL \d+ seed=(\d+) rooms=(\d+) floors=(\d+)', txt)
for s, r, f in played:
    s, r, f = int(s), int(r), int(f)
    if s in sweep and sweep[s] != (r, f):
        print(f"  seed {s}: swept {sweep[s]} but played ({r}, {f}) — rendering perturbed generation")
        sys.exit(1)
EOF
check $? "a rendered level matches its swept twin (rendering does not perturb the RNG)"

counts=$(grep -o 'ENT [0-9]*' "$log" | sort -u | tr '\n' ' ')
[ "$(grep -o 'ENT [0-9]*' "$log" | sort -u | wc -l | tr -d ' ')" = 1 ]
check $? "entity count constant ($counts) across descents"

lo=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | head -1)
hi=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | cut -d' ' -f2 | sort -n | tail -1)
[ -n "$lo" ] && [ $(( hi - lo )) -le 4096 ]
check $? "heap bounded across descents (span $(( ${hi:-0} - ${lo:-0} )) B)"

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/rl-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
[ "${n:-0}" -ge 5 ]
check $? "frame paints ($n colours)"

echo
[ "$fail" = 0 ] && echo "roguelike: PASS" || echo "roguelike: FAIL"
exit $fail
