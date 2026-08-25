#!/usr/bin/env bash
# sunnyside-farm verify — de-risk 4 of the sunnyside ladder (examples/sunnyside/SPEC.md).
# Drives the whole loop headlessly: till -> plant -> water -> five growth days -> harvest.
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
npm run build >/tmp/sunnyside-farm-build.log 2>&1 \
  || { echo "  FAIL build (see /tmp/sunnyside-farm-build.log)"; exit 1; }
echo "  ok   build"

log=$(mktemp)
# boot settles ~frame 406; then: hoe, ->seeds, plant, ->can, water, and one
# watering after each day tick (period 600), then ->scythe and harvest
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-farm.gba \
  /tmp/sunnyside-farm-verify.png 4200 \
  "500:a,510:,530:r,540:,560:r,570:,600:a,610:,640:l,650:,680:a,690:,1060:a,1070:,1660:a,1670:,2260:a,2270:,2860:a,2870:,3460:r,3470:,3490:r,3500:,3530:a,3540:" \
  >"$log" 2>&1 || { echo "  FAIL headless run crashed"; tail -20 "$log"; exit 1; }

if grep -Eq "$CRASH" "$log"; then
  echo "  FAIL crash lines:"; grep -E "$CRASH" "$log" | head -5; fail=1
else
  echo "  ok   no crash lines"
fi
grep -q 'FARM READY' "$log" \
  && echo "  ok   world + plot booted" \
  || { echo "  FAIL no FARM READY marker"; fail=1; }

# growth only on watered days: five 'grown=1' days then the harvest
grown_days=$(grep -c 'grown=1' "$log" || true)
if [ "$grown_days" -ge 5 ]; then
  echo "  ok   crop advanced on $grown_days watered days"
else
  echo "  FAIL crop grew on only $grown_days days (want 5)"; fail=1
fi
grep -q 'HARVEST OK total=1' "$log" \
  && echo "  ok   harvest landed" \
  || { echo "  FAIL no harvest"; grep 'FARM day' "$log" | tail -3; fail=1; }

# the negative control for growth: an unwatered day must NOT grow
grep -q 'grown=0' "$log" \
  && echo "  ok   negative control: unwatered day grew nothing" \
  || { echo "  FAIL no unwatered day observed"; fail=1; }

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/sunnyside-farm-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
if [ "${n:-0}" -ge 10 ]; then
  echo "  ok   frame paints ($n colours)"
else
  echo "  FAIL frame near-blank ($n colours)"; fail=1
fi

echo
[ "$fail" = 0 ] && echo "sunnyside-farm verify: PASS" || echo "sunnyside-farm verify: FAIL"
exit $fail
