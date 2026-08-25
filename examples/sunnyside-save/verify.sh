#!/usr/bin/env bash
# sunnyside-save verify — de-risk 6 of the sunnyside ladder (examples/sunnyside/SPEC.md).
# Three boots on one .sav: seed, restore+advance, restore the advanced day.
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
npm run build >/tmp/sunnyside-save-build.log 2>&1 \
  || { echo "  FAIL build (see /tmp/sunnyside-save-build.log)"; exit 1; }
echo "  ok   build"

rm -f tish-agb-sunnyside-save.sav
l1=$(mktemp); l2=$(mktemp); l3=$(mktemp)
for run in 1 2 3; do
  eval "log=\$l$run"
  GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside-save.gba \
    /tmp/sunnyside-save-verify.png 120 >"$log" 2>&1 \
    || { echo "  FAIL boot $run crashed"; tail -10 "$log"; exit 1; }
  if grep -Eq "$CRASH" "$log"; then
    echo "  FAIL crash on boot $run"; grep -E "$CRASH" "$log" | head -3; fail=1
  fi
done
echo "  ok   three boots, no crashes"

grep -q 'PASS packroundtrip' "$l1" \
  && echo "  ok   in-RAM pack/unpack lossless" \
  || { echo "  FAIL pack roundtrip"; fail=1; }
seeded_hash=$(grep -o 'SAVE SEEDED hash=[0-9]*' "$l1" | grep -o '[0-9]*$')
[ -n "${seeded_hash:-}" ] \
  && echo "  ok   boot 1 seeded a fresh cartridge (hash=$seeded_hash)" \
  || { echo "  FAIL boot 1 did not seed"; fail=1; }
grep -q "SAVE RESTORED hash=$seeded_hash day=1 gold=120 seed=7" "$l2" \
  && echo "  ok   boot 2 restored the identical farm" \
  || { echo "  FAIL boot 2 restore:"; grep 'SAVE' "$l2"; fail=1; }
grep -q "SAVE RESTORED hash=$seeded_hash day=2" "$l3" \
  && echo "  ok   boot 3 saw the advanced day (repeated writes hold)" \
  || { echo "  FAIL boot 3 restore:"; grep 'SAVE' "$l3"; fail=1; }
grep -q 'DONE fails 0' "$l3" \
  && echo "  ok   DONE fails 0" \
  || { echo "  FAIL in-ROM asserts"; fail=1; }

echo
[ "$fail" = 0 ] && echo "sunnyside-save verify: PASS" || echo "sunnyside-save verify: FAIL"
exit $fail
