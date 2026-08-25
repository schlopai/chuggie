#!/usr/bin/env bash
# sunnyside-day verify — de-risk 5 of the sunnyside ladder (examples/sunnyside/SPEC.md).
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
npm run build >/tmp/sunnyside-day-build.log 2>&1 \
  || { echo "  FAIL build (see /tmp/sunnyside-day-build.log)"; exit 1; }
echo "  ok   build"

# one long unattended run: full day, dusk ramp, night, 02:00 pass-out, wake
log=$(mktemp)
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-day.gba \
  /tmp/sunnyside-day-verify.png 6200 >"$log" 2>&1 \
  || { echo "  FAIL headless run crashed"; tail -20 "$log"; exit 1; }
if grep -Eq "$CRASH" "$log"; then
  echo "  FAIL crash lines:"; grep -E "$CRASH" "$log" | head -5; fail=1
else
  echo "  ok   no crash lines"
fi
grep -q 'CLOCK day=1 h=17 fade=2' "$log" \
  && echo "  ok   dusk ramp starts at 17:00" \
  || { echo "  FAIL no dusk ramp"; fail=1; }
grep -q 'CLOCK day=1 h=20 fade=7' "$log" \
  && echo "  ok   night level at 20:00" \
  || { echo "  FAIL no night level"; fail=1; }
grep -q 'PASSOUT day=1' "$log" && grep -q 'WAKE day=2' "$log" \
  && echo "  ok   02:00 pass-out and wake on day 2" \
  || { echo "  FAIL pass-out/wake missing"; fail=1; }

# the tint must be VISIBLE: night frame measurably darker than day frame
"$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-day.gba /tmp/sunnyside-day-noon.png 1000 >/dev/null 2>&1
"$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-day.gba /tmp/sunnyside-day-night.png 3900 >/dev/null 2>&1
python3 - <<'EOF'
from PIL import Image
def mean(p):
    im = Image.open(p).convert('RGB')
    px = list(im.getdata())
    return sum(sum(c) for c in px) / len(px) / 3
day, night = mean('/tmp/sunnyside-day-noon.png'), mean('/tmp/sunnyside-day-night.png')
print(f"  {'ok  ' if night < day * 0.75 else 'FAIL'} night brightness {night:.1f} vs day {day:.1f}")
assert night < day * 0.75
EOF
[ $? = 0 ] || fail=1

echo
[ "$fail" = 0 ] && echo "sunnyside-day verify: PASS" || echo "sunnyside-day verify: FAIL"
exit $fail
