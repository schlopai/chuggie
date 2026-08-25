#!/usr/bin/env bash
# ui-builder-vs-literal — the two authoring paths must draw the SAME screen, and it must not be blank.
#
# ⚠️ The blank check is not decoration. This example passed twice while drawing nothing: a uniform
# framebuffer compares equal to another uniform framebuffer. Equality alone proves nothing.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
fail=0
chk() { if [ "$2" = "$3" ]; then echo "  ok   $1 = $3"; else echo "  FAIL $1: got '$2', expected '$3'"; fail=1; fi }

ROM=ui-builder-vs-literal.gba
[ -f "$ROM" ] || "${TISH:-tish}" build src/main.tish --target gba -o "$ROM" >/dev/null 2>&1
[ -f "$ROM" ] || { echo "  FAIL $ROM missing — run npm run build"; exit 1; }

# Frame 100 is after the literal path has rendered; 320 is after the builder path has repainted.
"$ROOT/tools/gba-shot" "$ROM" /tmp/uibl-lit.ppm 100 >/dev/null 2>&1
"$ROOT/tools/gba-shot" "$ROM" /tmp/uibl-bld.ppm 320 >/dev/null 2>&1

if cmp -s /tmp/uibl-lit.ppm /tmp/uibl-bld.ppm; then
  echo "  ok   literal and builder render identically"
else
  echo "  FAIL literal and builder renders differ"; fail=1
fi

# Not blank: a real screen here has several colours and the background is well under 100%.
read -r colours share < <(python3 - <<'PY'
from collections import Counter
d = open('/tmp/uibl-lit.ppm','rb').read()[15:]
c = Counter(d[i:i+3] for i in range(0, len(d), 3))
print(len(c), round(100*c.most_common(1)[0][1]/(len(d)//3), 1))
PY
)
chk "distinct colours on screen" "$([ "$colours" -ge 3 ] && echo many || echo too-few)" "many"
if python3 -c "import sys; sys.exit(0 if $share < 99.0 else 1)"; then
  echo "  ok   screen has content (background $share% < 99%)"
else
  echo "  FAIL screen is effectively blank (background $share%)"; fail=1
fi

# The ROM prints both cost lines; a missing one means a path did not run.
LOG=$(GBA_SHOT_LOG=1 "$ROOT/tools/gba-shot" "$ROM" /tmp/uibl.ppm 900 2>&1 | grep -aE 'literal:|builder:' || true)
echo "$LOG" | grep -q 'literal:' && echo "  ok   literal path reported its cost" || { echo "  FAIL no literal: line"; fail=1; }
echo "$LOG" | grep -q 'builder:' && echo "  ok   builder path reported its cost" || { echo "  FAIL no builder: line"; fail=1; }
echo "$LOG" | sed 's/^/       /'

exit $fail
