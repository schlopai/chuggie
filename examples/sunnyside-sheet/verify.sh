#!/usr/bin/env bash
# sunnyside-sheet verify — de-risk 1 of the sunnyside ladder (examples/sunnyside/SPEC.md).
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
fail=0
CRASH='Bad memory|Unimplemented memory|panicked at|Illegal opcode|Jumped to invalid address'

python3 "$ROOT/scripts/const_to_let.py" --check src >/dev/null 2>&1 \
  && echo "  ok   const_to_let clean" \
  || { echo "  FAIL const_to_let --check"; fail=1; }

# The baked sheets this ROM proves must be reproducible from the vendored pack.
# Source pack not vendored (license); re-bake only where SUNNYSIDE_SRC provides it.
SUN_SRC="${SUNNYSIDE_SRC:-$ROOT/assets/sunnyside}"
if [ -d "$SUN_SRC/raw" ]; then
SUNNYSIDE_SRC="$SUN_SRC" python3 "$ROOT/scripts/gen_sunnyside_pack.py" >/dev/null \
  && echo "  ok   gen_sunnyside_pack regenerates" \
  || { echo "  FAIL gen_sunnyside_pack.py"; fail=1; }
if ! git diff --quiet -- ../../assets/sunnyside/baked ../sunnyside/src/data_anim.tish 2>/dev/null; then
  echo "  FAIL baked sheets drifted from generator output"; fail=1
else
  echo "  ok   baked sheets match generator"
fi
else
  echo "  skip re-bake (source pack not present)"
fi

rm -rf .tish
unset CARGO_TARGET_DIR
npm run build >/tmp/sunnyside-sheet-build.log 2>&1 \
  || { echo "  FAIL build (see /tmp/sunnyside-sheet-build.log)"; exit 1; }
echo "  ok   build"

log=$(mktemp)
# cycle two actions, then walk left (exercises frame indexing + hflip)
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-sheet.gba \
  /tmp/sunnyside-sheet-verify.png 300 "60:a,120:a,200:left,260:left" >"$log" 2>&1 \
  || { echo "  FAIL headless run crashed"; tail -20 "$log"; exit 1; }
if grep -Eq "$CRASH" "$log"; then
  echo "  FAIL crash lines:"; grep -E "$CRASH" "$log" | head -5; fail=1
else
  echo "  ok   no crash lines"
fi
grep -q 'SHEET OK actions=13' "$log" \
  && echo "  ok   boot marker (13 actions tabled)" \
  || { echo "  FAIL boot marker missing"; fail=1; }

# a frozen text menu is 2 colours; sprites on the backdrop are many
n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/sunnyside-sheet-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
if [ "${n:-0}" -ge 8 ]; then
  echo "  ok   frame paints ($n colours)"
else
  echo "  FAIL frame near-blank ($n colours)"; fail=1
fi

echo
[ "$fail" = 0 ] && echo "sunnyside-sheet verify: PASS" || echo "sunnyside-sheet verify: FAIL"
exit $fail
