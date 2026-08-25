#!/usr/bin/env bash
# sunnyside-terrain verify — de-risk 2 of the sunnyside ladder (examples/sunnyside/SPEC.md).
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
fail=0
CRASH='Bad memory|Unimplemented memory|panicked at|Illegal opcode|Jumped to invalid address'

python3 "$ROOT/scripts/const_to_let.py" --check src >/dev/null 2>&1 \
  && echo "  ok   const_to_let clean" \
  || { echo "  FAIL const_to_let --check"; fail=1; }

# Source pack not vendored (license); re-bake only where SUNNYSIDE_SRC provides it.
SUN_SRC="${SUNNYSIDE_SRC:-$ROOT/assets/sunnyside}"
if [ -d "$SUN_SRC/raw" ]; then
SUNNYSIDE_SRC="$SUN_SRC" python3 "$ROOT/scripts/gen_sunnyside_pack.py" >/dev/null \
  && echo "  ok   gen_sunnyside_pack regenerates" \
  || { echo "  FAIL gen_sunnyside_pack.py"; fail=1; }
if ! git diff --quiet -- ../../assets/sunnyside/baked ../sunnyside/src/data_world.tish 2>/dev/null; then
  echo "  FAIL baked atlas drifted from generator output"; fail=1
else
  echo "  ok   baked atlas matches generator"
fi
else
  echo "  skip re-bake (source pack not present)"
fi

rm -rf .tish
unset CARGO_TARGET_DIR
npm run build >/tmp/sunnyside-terrain-build.log 2>&1 \
  || { echo "  FAIL build (see /tmp/sunnyside-terrain-build.log)"; exit 1; }
echo "  ok   build"

log=$(mktemp)
# boot map, walk, reseed twice (each reseed re-generates + re-uploads + re-collides)
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-terrain.gba \
  /tmp/sunnyside-terrain-verify.png 1200 "300:a,310:,700:a,710:,1000:up,1100:left" \
  >"$log" 2>&1 || { echo "  FAIL headless run crashed"; tail -20 "$log"; exit 1; }
if grep -Eq "$CRASH" "$log"; then
  echo "  FAIL crash lines:"; grep -E "$CRASH" "$log" | head -5; fail=1
else
  echo "  ok   no crash lines"
fi

# three generations logged, each with a land-cell summary
gens=$(grep -c 'TERRAIN seed=' "$log" || true)
if [ "$gens" -ge 3 ]; then
  echo "  ok   $gens generations logged"
else
  echo "  FAIL only $gens generations logged (want 3)"; fail=1
fi

# the land count is the oracle-diffable summary: rng replica must agree
expected=$(python3 - <<'EOF'
def below(state, n):
    state[0] = (state[0] * 1664525 + 1013904223) & 0xFFFFFFFF
    return ((state[0] >> 16) & 0xFFFF) % n
def land(seed):
    W, H = 48, 32
    s = [seed]
    T = [0] * (W * H)
    for y in range(3, H - 3):
        for x in range(3, W - 3):
            edge = x == 3 or x == W - 4 or y == 3 or y == H - 4
            if not edge or below(s, 100) < 55:
                T[y * W + x] = 1
    px, py = W // 2, H // 2
    for _ in range(90):
        T[py * W + px] = 0
        d = below(s, 4)
        if d == 0 and px > 5: px -= 1
        if d == 1 and px < W - 6: px += 1
        if d == 2 and py > 5: py -= 1
        if d == 3 and py < H - 6: py += 1
    ry, rx = H // 2 + 4, 4
    while rx < W - 4:
        if T[ry * W + rx] == 1: T[ry * W + rx] = 2
        if T[(ry + 1) * W + rx] == 1: T[(ry + 1) * W + rx] = 2
        wob = below(s, 3)
        if wob == 0 and ry > 6: ry -= 1
        if wob == 2 and ry < H - 7: ry += 1
        rx += 1
    return sum(1 for t in T if t > 0)
print(" ".join(f"seed={s} land={land(s)}" for s in (1, 2, 3)))
EOF
)
ok=1
for pair in $expected; do
  case "$pair" in
    seed=*) seed_part=$pair ;;
    land=*) grep -q "TERRAIN $seed_part $pair" "$log" || ok=0 ;;
  esac
done
if [ "$ok" = 1 ]; then
  echo "  ok   land counts match the rng replica ($expected)"
else
  echo "  FAIL land counts diverge from replica; got:"; grep 'TERRAIN' "$log"; fail=1
fi

# heap must be flat across reseeds (first settle allowed)
uniq_n=$(grep -o 'HEAP [0-9]*' "$log" | tail -n +2 | sort -u | wc -l | tr -d ' ')
if [ "$uniq_n" = 1 ]; then
  echo "  ok   heap flat across reseeds"
else
  echo "  FAIL heap drift: $(grep -o 'HEAP [0-9]*' "$log" | tr '\n' ' ')"; fail=1
fi

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/sunnyside-terrain-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
if [ "${n:-0}" -ge 8 ]; then
  echo "  ok   frame paints ($n colours)"
else
  echo "  FAIL frame near-blank ($n colours)"; fail=1
fi

echo
[ "$fail" = 0 ] && echo "sunnyside-terrain verify: PASS" || echo "sunnyside-terrain verify: FAIL"
exit $fail
