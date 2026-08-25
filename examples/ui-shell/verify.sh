#!/usr/bin/env bash
# Verify ui-shell: does packages/ui.tish's dirty repainting actually hold up?
#
# The claim under test is `makeButtonGroup(...).nav()`'s — that moving a cursor repaints only the
# two buttons whose selection changed, and never re-lays-out. `packages/drop_shell.tish` does the
# same job with hand-rolled row invalidation, and got there by shipping the bug first: a cursor
# move set the same repaint flag a screen change does, so every press cleared 240x160 and
# re-shaped every string, and the menu visibly blinked out for a frame.
#
# So this measures rather than assumes, on both axes — the selection lands where it should, and a
# press costs no layout.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR
npm run build >/tmp/ui-shell-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -20 /tmp/ui-shell-build.log; exit 1; fi

# Four presses down, then SELECT to force a full re-render for the contrast, then one more.
SCHED=$(python3 - <<'PY'
f, out = 90, []
def press(k):
    global f
    out.append(f"{f}:{k}"); f += 10
    out.append(f"{f}:");    f += 10
for _ in range(4): press("down")
press("select")
press("down")
print(",".join(out))
PY
)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh ui-shell.gba /tmp/ui-shell.png 400 "$SCHED" 2>&1 \
  | grep -E 'UI SHELL|FULL RENDER|NAV|PICKED|'"$CRASH_RE" > /tmp/ui-shell.log
check $? "runs headless"

grep -qE "$CRASH_RE" /tmp/ui-shell.log
check $((1 - $?)) "no panic, no allocation failure"

grep -q 'FULL RENDER 1' /tmp/ui-shell.log
check $? "the screen lays out and paints"
note "$(grep -m1 'FULL RENDER 1' /tmp/ui-shell.log | sed 's/.*ui: //')"

# FUNCTIONALITY. Four presses must walk 0 -> 1 -> 2 -> 3 -> 4, in order and without skipping.
python3 - <<'NAVCHK'
import re, sys
idx = [int(m.group(1)) for m in
       re.finditer(r'NAV \d+ index (\d+)', open('/tmp/ui-shell.log').read())]
if idx[:4] != [1, 2, 3, 4]:
    print(f"  selection walked {idx[:4]}, expected [1, 2, 3, 4]")
    sys.exit(1)
print(f"  selection walked {idx}")
NAVCHK
check $? "the cursor lands on each entry in turn"

# PERFORMANCE, and the whole point. A nav that re-laid-out would report FRESH numbers from
# `uiLayoutStats`; one that only repainted two buttons leaves the previous pass's numbers standing.
# So: every NAV line's stats must be IDENTICAL to the FULL RENDER before it.
python3 - <<'COSTCHK'
import re, sys
lines = open('/tmp/ui-shell.log').read().splitlines()
last_full, checked = None, 0
for ln in lines:
    m = re.search(r'(FULL RENDER \d+|NAV \d+ index \d+) (ui: .*)$', ln)
    if not m:
        continue
    if m.group(1).startswith('FULL'):
        last_full = m.group(2)
        continue
    if last_full is None:
        print("  a NAV happened before any full render")
        sys.exit(1)
    if m.group(2) != last_full:
        print(f"  A CURSOR MOVE RE-LAID-OUT:\n    render {last_full}\n    nav    {m.group(2)}")
        sys.exit(1)
    checked += 1
if checked < 4:
    print(f"  only {checked} navs to check")
    sys.exit(1)
print(f"  {checked} cursor moves, none of them triggered a layout pass")
COSTCHK
check $? "moving the cursor costs no re-layout"

# And the cost that is being avoided, stated out loud — this is why the blink happened.
python3 - <<'FULLCOST'
import re, sys
m = re.search(r'FULL RENDER 1 ui: .*tot=(\d+)t (\d+)ms', open('/tmp/ui-shell.log').read())
if not m:
    sys.exit(1)
ticks, ms = int(m.group(1)), int(m.group(2))
frames = ticks / 4389.0     # ticks in one 60Hz frame
print(f"  a full render is {ticks}t = {ms}ms = {frames:.1f} frames of work")
sys.exit(0 if frames > 1 else 1)
FULLCOST
check $? "a full render really does overrun a frame (which is what the blink was)"

# A live frame, not a blank one — the visual half of the same claim.
python3 - <<'PY'
import struct, zlib, sys
d = open('/tmp/ui-shell.png','rb').read()
pos, idat, w, h = 8, b'', 0, 0
while pos < len(d):
    ln = struct.unpack('>I', d[pos:pos+4])[0]; typ = d[pos+4:pos+8]
    if typ == b'IHDR': w, h = struct.unpack('>II', d[pos+8:pos+16])
    if typ == b'IDAT': idat += d[pos+8:pos+8+ln]
    pos += 12 + ln
raw = zlib.decompress(idat)
stride = w*3+1
cols = set()
for y in range(0, h, 4):
    row = raw[y*stride+1:(y+1)*stride]
    for x in range(0, w, 4):
        cols.add(row[x*3:x*3+3])
print(f"  {len(cols)} distinct colours in {w}x{h}")
sys.exit(0 if len(cols) >= 3 else 1)
PY
check $? "the menu is on screen after the last cursor move"

echo
if [ $fail = 0 ]; then echo "ui-shell: PASS"; else echo "ui-shell: FAIL"; fi
exit $fail
