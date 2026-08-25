#!/usr/bin/env bash
# sunnyside-worldgen verify — the island generator against its Python twin,
# seed for seed (de-risk 3 of the sunnyside ladder, examples/sunnyside/SPEC.md).
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
fail=0
CRASH='Bad memory|Unimplemented memory|panicked at|Illegal opcode|Jumped to invalid address'

python3 "$ROOT/scripts/const_to_let.py" --check src >/dev/null 2>&1 \
  && echo "  ok   const_to_let clean" \
  || { echo "  FAIL const_to_let --check"; fail=1; }

rm -rf .tish
unset CARGO_TARGET_DIR
npm run build >/tmp/sunnyside-worldgen-build.log 2>&1 \
  || { echo "  FAIL build (see /tmp/sunnyside-worldgen-build.log)"; exit 1; }
echo "  ok   build"

log=$(mktemp)
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-worldgen.gba \
  /tmp/sunnyside-worldgen-verify.png 4200 >"$log" 2>&1 \
  || { echo "  FAIL headless run crashed"; tail -20 "$log"; exit 1; }
if grep -Eq "$CRASH" "$log"; then
  echo "  FAIL crash lines:"; grep -E "$CRASH" "$log" | head -5; fail=1
else
  echo "  ok   no crash lines"
fi
grep -q 'SS SWEEP DONE' "$log" \
  && echo "  ok   sweep completed" \
  || { echo "  FAIL sweep did not finish"; fail=1; }

# Diff every generation the ROM reported against the Python twin.
rom_report=$(mktemp)
grep -oE 'SS (GEN|BLD) .*' "$log" | sed 's/[[:space:]]*$//' > "$rom_report"
seeds=$(grep -oE 'SS GEN seed=[0-9]+' "$rom_report" | grep -oE '[0-9]+' | sort -un | tr '\n' ' ')
oracle_report=$(mktemp)
(cd "$ROOT" && python3 -m scripts.procgen.sunnyside $seeds) > "$oracle_report"
# the ROM logs seed 1 twice (boot + post-sweep rebuild); compare as sets
if diff <(sort -u "$rom_report") <(sort -u "$oracle_report") >/dev/null; then
  n=$(echo "$seeds" | wc -w | tr -d ' ')
  echo "  ok   $n seeds match the Python twin exactly (land, trees, hash, placements)"
else
  echo "  FAIL ROM diverges from the twin:"
  diff <(sort -u "$rom_report") <(sort -u "$oracle_report") | head -10
  fail=1
fi

# ⚠️ NEGATIVE CONTROL — the diff above would also pass if both sides were
# empty. Assert the sweep actually produced generations.
n_gens=$(grep -c 'SS GEN' "$rom_report" || true)
if [ "$n_gens" -ge 12 ]; then
  echo "  ok   negative control: $n_gens generations in the report"
else
  echo "  FAIL only $n_gens generations reported"; fail=1
fi

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/sunnyside-worldgen-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
if [ "${n:-0}" -ge 10 ]; then
  echo "  ok   frame paints ($n colours)"
else
  echo "  FAIL frame near-blank ($n colours)"; fail=1
fi

echo
[ "$fail" = 0 ] && echo "sunnyside-worldgen verify: PASS" || echo "sunnyside-worldgen verify: FAIL"
exit $fail
