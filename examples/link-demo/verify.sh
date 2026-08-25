#!/usr/bin/env bash
# Verify link-demo: the link cable, both without a peer and with one.
#
# This example exists to be READ OFF A SCREEN by a person with two mGBA windows or two cartridges,
# so most of its value is not automatable. What is automatable is everything that would waste that
# person's time: that it boots, that it waits patiently with no peer instead of giving up, and that
# two of them do actually reach PLAYING and mirror each other's buttons.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR
npm run build >/tmp/link-demo-build.log 2>&1
check $? "builds"
if [ $fail = 1 ]; then tail -20 /tmp/link-demo-build.log; exit 1; fi

# ── ALONE ────────────────────────────────────────────────────────────────────
# A unit with no peer must wait indefinitely. On hardware and in two GUI windows this is the
# normal case — one console is switched on a minute before the other — and an earlier version
# latched to OFFLINE after three seconds, which meant whichever unit booted first could never
# see the second one.
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh link-demo.gba /tmp/link-demo-solo.png 900 2>&1 \
  | grep -E 'LINK|'"$CRASH_RE" > /tmp/link-demo-solo.log
check $? "runs headless with no peer"

grep -qE "$CRASH_RE" /tmp/link-demo-solo.log
check $((1 - $?)) "no panic, no allocation failure"

# State 1 is SEARCHING. It must still be searching at the end of a 900-frame run, not OFFLINE (0)
# and not LOST (3).
tail -1 /tmp/link-demo-solo.log | grep -q 'state 1'
check $? "with no peer it keeps searching rather than giving up"
note "$(tail -1 /tmp/link-demo-solo.log)"

# ── TWO CONSOLES ─────────────────────────────────────────────────────────────
echo "== link =="
../../scripts/link.sh link-demo.gba link-demo.gba 700 "200:a,400:a,401:up" "300:b" \
  >/dev/null 2>/tmp/link-demo-link.log
check $? "two linked consoles run to completion"

# State 2 is PLAYING, and both have to reach it.
plays=$(grep -c 'LINK STATE 2' /tmp/link-demo-link.log || true)
[ "${plays:-0}" -ge 2 ]
check $? "both consoles reach PLAYING (${plays:-0} of 2)"

# One master, one child. Two masters means neither is driving transfers and the link is a fiction.
grep -q 'master 1' /tmp/link-demo-link.log && grep -q 'master 0' /tmp/link-demo-link.log
check $? "exactly one console is the master"

# THE SEED MUST MATCH. A link that is up with two different seeds looks perfect on both screens
# and desyncs the instant a real game deals a board from it — this is the failure a lockstep game
# cannot detect from the inputs alone, which is why the demo prints it in the first place.
seeds=$(grep -oE 'seed [0-9]+' /tmp/link-demo-link.log | grep -v 'seed 0' | sort -u | wc -l | tr -d ' ')
[ "${seeds:-0}" = 1 ]
check $? "both agree on one seed"

# THE BUTTON MIRROR, which is the whole human-facing point: what one console sends is what the
# other reports receiving. The schedule holds UP on console 0 and B on console 1.
python3 - <<'MIRROR'
import re, sys
mine = {0: set(), 1: set()}
theirs = {0: set(), 1: set()}
for line in open('/tmp/link-demo-link.log', errors='replace'):
    m = re.search(r'\[p(\d) .*mine (\d+) theirs (\d+)', line)
    if m:
        p = int(m.group(1))
        mine[p].add(int(m.group(2)))
        theirs[p].add(int(m.group(3)))
# Whatever console 0 ever sent, console 1 must have seen, and vice versa.
missing0 = (mine[0] - {0}) - theirs[1]
missing1 = (mine[1] - {0}) - theirs[0]
if missing0 or missing1:
    print(f"  MIRROR BROKEN: p0 sent {sorted(missing0)} unseen by p1; "
          f"p1 sent {sorted(missing1)} unseen by p0")
    sys.exit(1)
if not (mine[0] - {0}) or not (mine[1] - {0}):
    print("  neither console pressed anything — the schedule did not reach the pad")
    sys.exit(1)
print(f"  p0 sent {sorted(mine[0] - {0})} and p1 saw it; p1 sent {sorted(mine[1] - {0})} and p0 saw it")
MIRROR
check $? "each console's buttons appear on the other"

# Round trip has to stay playable. Anything above a few frames is a link that is technically up
# and too slow to run a lockstep game over.
python3 - <<'TRIP'
import re, sys
trips = [int(m.group(1)) for m in
         re.finditer(r'trip (\d+)', open('/tmp/link-demo-link.log', errors='replace').read())]
trips = [t for t in trips if t >= 0]
if not trips:
    print("  no round-trip measurements")
    sys.exit(1)
worst = max(trips)
print(f"  round trip {min(trips)}-{worst} frames over {len(trips)} samples")
sys.exit(0 if worst <= 4 else 1)
TRIP
check $? "the round trip stays inside a playable window"

echo
if [ $fail = 0 ]; then echo "link-demo: PASS"; else echo "link-demo: FAIL"; fi
exit $fail
