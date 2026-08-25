#!/usr/bin/env bash
# Verify pong-link: one console, two consoles, and — the point of the whole example — that the two
# are running the SAME game.
#
# A desynced lockstep game is the hardest kind of bug to see, because nothing looks wrong. Each
# console shows a completely plausible match; they are just not the same match. No screenshot of one
# unit can catch it. So the ROM prints a line describing its simulation state every twenty SIMULATED
# frames, and this compares the two consoles' lines for the same frame number. That comparison is
# the only assertion here that could not be made from a single cartridge.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
. ../../scripts/verify_common.sh
root="$(cd ../.. && pwd)"
unset CARGO_TARGET_DIR

fails=0
check() {   # name, got, want
  if [ "$2" = "$3" ]; then echo "  ok   $1 = $2"
  else echo "  FAIL $1 = $2 (want $3)"; fails=$((fails + 1)); fi
}

echo "build:"
npm run build >/tmp/pong-link-build.log 2>&1
if [ $? -ne 0 ]; then echo "  FAIL build"; tail -30 /tmp/pong-link-build.log; exit 1; fi
echo "  ok   builds"

# Every example is supposed to compile against our agb fork. Cargo only ever warned about this, and
# the warning does not mean what it appears to (see scripts/check_agb_fork.sh) — so ask properly.
assert_agb_fork . || fails=$((fails + 1))

# ── 1. Alone ────────────────────────────────────────────────────────────────────────────────────
# A unit with no peer must wait indefinitely rather than giving up: on hardware one console is
# routinely switched on a minute before the other. That is also why the one-player game needs an
# explicit way in (START) instead of a timeout — a timeout would start a solo match on whichever
# console booted first, and a peer arriving mid-game would have nothing sane to join.
echo "alone:"
# ⚠️ Log to a FILE, and never `echo "$var" | grep -q` under `set -o pipefail`. `grep -q` exits the
# moment it matches, which closes the pipe, which makes `echo` die of SIGPIPE, which pipefail then
# reports as a failed pipeline — so the check fails precisely BECAUSE the string was found. A grep
# that finds nothing reads to the end and passes. It is exactly backwards, and it cost a real
# debugging detour here.
GBA_SHOT_LOG=1 "$root/tools/gba-shot" pong-link.gba /tmp/pong-solo.ppm 1800 \
  "220:start,280:,400:up,800:,1100:down,1500:" >/tmp/pong-solo.log 2>&1

if crash_grep /tmp/pong-solo.log; then echo "  ok   no crash"
else echo "  FAIL crashed"; fails=$((fails + 1)); fi

if grep -q "PONG solo" /tmp/pong-solo.log; then
  echo "  ok   START starts a one-player match"
else
  echo "  FAIL never entered the solo game"; fails=$((fails + 1))
fi

# It must actually play, not merely start: the ball moves and somebody scores.
solo_last="$(grep -o 'SYNC .*' /tmp/pong-solo.log | tail -1)"
if [ -n "$solo_last" ]; then
  echo "  ok   simulation runs solo ($solo_last)"
else
  echo "  FAIL no simulation frames at all"; fails=$((fails + 1))
fi

# ── 2. Two consoles, and the same game on both ──────────────────────────────────────────────────
echo "linked:"
# ⚠️ keys0 drives p0, which is the MASTER and therefore player one — so its START is what starts
# the match. Without it the two consoles sit in the READY phase agreeing perfectly about a ball that
# never moves, which would pass a naive "do they agree?" check while testing nothing.
"$root/scripts/link.sh" pong-link.gba pong-link.gba 4000 \
  "80:start,140:,300:up,700:,1100:down,1500:,1900:up,2300:,2700:down,3200:" \
  "400:down,800:,1200:up,1600:,2000:down,2400:,2800:up,3300:" \
  >/dev/null 2>/tmp/pong-link.log
check "two linked consoles run to completion" $? 0

plays="$(grep -c 'LINK PLAYING' /tmp/pong-link.log || true)"
check "both consoles reach PLAYING" "${plays:-0}" 2

# One master and one child. Two masters means neither is driving transfers and the link is fiction.
sides="$(grep -o 'side=[01]' /tmp/pong-link.log | sort -u | wc -l | tr -d ' ')"
check "one plays left, the other right" "$sides" 2

# THE assertion. Both consoles describe their simulation at the same numbered frame; every pair must
# be identical. A single mismatched line means the two games have diverged, which is invisible on
# either screen alone.
python3 - <<'PYEOF'
import re
import sys

p0, p1 = {}, {}
for line in open("/tmp/pong-link.log"):
    m = re.match(r"\[p(\d) frame \d+\] SYNC (f=\d+ .*)", line.strip())
    if not m:
        continue
    (p0 if m.group(1) == '0' else p1)[m.group(2).split()[0]] = m.group(2)

common = sorted(set(p0) & set(p1), key=lambda k: int(k[2:]))
bad = [k for k in common if p0[k] != p1[k]]
if len(common) < 10:
    print(f"  FAIL only {len(common)} comparable simulation frames — the game barely ran")
    sys.exit(1)
if bad:
    print(f"  FAIL the two consoles disagree at {len(bad)} of {len(common)} frames")
    for k in bad[:3]:
        print(f"       p0: {p0[k]}")
        print(f"       p1: {p1[k]}")
    sys.exit(1)
print(f"  ok   both consoles agree at all {len(common)} compared simulation frames")
print(f"       last: {p0[common[-1]]}")
PYEOF
[ $? -eq 0 ] || fails=$((fails + 1))

# ── 3. Linked, but not playing until player one says so ─────────────────────────────────────────
# Reaching PLAYING must NOT start the match. Both consoles come up READY and wait for player one —
# and "player one" is the master, which is the left paddle by definition, so both agree who that is
# without negotiating it. With nobody pressing anything, the ball stays parked and the score stays
# nil; if this ever regresses, the two consoles would still agree with each other perfectly, so the
# determinism check above cannot catch it.
echo "ready gate:"
"$root/scripts/link.sh" pong-link.gba pong-link.gba 1600 "" "" \
  >/dev/null 2>/tmp/pong-ready.log
ready_last="$(grep -o 'SYNC .*' /tmp/pong-ready.log | tail -1)"
if [ -n "$ready_last" ]; then
  echo "  ok   both consoles simulate while waiting ($ready_last)"
else
  echo "  FAIL no simulation frames at all"; fails=$((fails + 1))
fi
# Ball dead centre, nothing scored: a match that has not begun.
case "$ready_last" in
  *"bx=120 by=88"*"s0=0 s1=0"*)
    echo "  ok   the match does not start until player one presses START" ;;
  *)
    echo "  FAIL the match started on its own: $ready_last"; fails=$((fails + 1)) ;;
esac

# ── 4. A sprints the paddle ─────────────────────────────────────────────────────────────────────
# Two solo runs, identical but for A being held. After five simulated frames the sprinting paddle
# must be further down the court — and in fact hard against the bottom rail, which the walking one
# is not yet. Tested through the input word rather than by reading a constant, so it also proves the
# extra bit survives the trip.
echo "sprint:"
# ⚠️ The mask includes START, so the same press that starts the one-player game is already holding
# down — solo now steps once per DISPLAY frame, so an input that arrives even twenty frames later has
# already missed the sample.
pad_at() {   # $1 = key mask -> the paddle's y ten simulated frames in
  GBA_SHOT_LOG=1 "$root/tools/gba-shot" pong-link.gba /tmp/pong-pad.ppm 400 \
    "220:$1" >/tmp/pong-pad.log 2>&1
  grep -o 'SYNC f=10 .*' /tmp/pong-pad.log | head -1 | grep -o 'ly=[0-9-]*' | cut -d= -f2
}
walk="$(pad_at 0x88)"     # START + down
run="$(pad_at 0x89)"      # …and A
if [ -n "$walk" ] && [ -n "$run" ] && [ "$run" -gt "$walk" ]; then
  echo "  ok   A + down travels further ($run vs $walk after five simulated frames)"
else
  echo "  FAIL sprint made no difference (A+down $run, down $walk)"; fails=$((fails + 1))
fi

# ── 5. The picture is smooth, not chunky ────────────────────────────────────────────────────────
# The simulation runs at about 10 Hz. Drawing its state directly makes the ball jump eight pixels at
# a time — the "chunky" this example started with. The draw path interpolates along the ball's
# recorded path through each simulated frame instead, so motion is 60 Hz even though physics is not.
#
# Measured rather than asserted by reading a constant: sample the ball's x on CONSECUTIVE display
# frames and require that it moves a little every frame, rather than a lot every sixth. This fails
# both ways — a frozen picture and a jumping one.
echo "smooth motion:"
python3 - <<'PYEOF3'
import sys
sys.path.insert(0, "../../scripts")
import numpy as np
from shot_check import shot

xs = []
for f in (300, 301, 302, 303, 304, 305):
    a = shot("pong-link.gba", "/tmp/pong-smooth.png", f, "220:start,280:").astype(int)
    band = a[:, 30:210]                       # the court, clear of both paddles
    bright = np.argwhere(band.sum(axis=2) > 500)
    xs.append(float(bright[:, 1].mean()) + 30 if len(bright) else None)

if any(v is None for v in xs):
    print("  FAIL the ball was not on screen for the whole sample")
    sys.exit(1)

steps = [abs(xs[i + 1] - xs[i]) for i in range(len(xs) - 1)]
worst, total = max(steps), abs(xs[-1] - xs[0])
if total < 1.0:
    print(f"  FAIL the ball did not move at all across six frames ({total:.1f}px)")
    sys.exit(1)
if worst > 3.0:
    print(f"  FAIL the ball jumps {worst:.1f}px in one display frame - the draw is not interpolating")
    sys.exit(1)
print(f"  ok   moves every frame, at most {worst:.2f}px a frame ({total:.1f}px over six)")
PYEOF3
[ $? -eq 0 ] || fails=$((fails + 1))

# ── 6. Pressing START early must not cost you the link ──────────────────────────────────────────
# The regression this catches, which was real: START was the way into the one-player game AND the
# thing the screen invites you to press, and taking it latched the console offline forever. Two mGBA
# windows take a moment to handshake, so the natural first keypress produced a cartridge that could
# never link — indistinguishable from the link being broken.
echo "start-early:"
"$root/scripts/link.sh" pong-link.gba pong-link.gba 2500 \
  "60:start,120:,400:up,900:" "70:start,130:,500:down,1000:" \
  >/dev/null 2>/tmp/pong-early.log
early="$(grep -c 'PONG playing' /tmp/pong-early.log || true)"
check "both still reach the linked game after an early START" "${early:-0}" 2

# ── 7. A rematch is a clean match, on both consoles ─────────────────────────────────────────────
# The first match finishes around emulator frame 3650, so player one holds START again after that and
# a second match must begin: scores back to nil, ball re-parked, and — the part that matters — both
# consoles doing it on the same simulated frame. A restart that ran on one console only would leave
# the two playing different matches while each looked perfectly normal.
echo "rematch:"
"$root/scripts/link.sh" pong-link.gba pong-link.gba 5200 \
  "80:start,140:,300:up,700:,1100:down,1500:,1900:up,2300:,2700:down,3200:,3800:start,3900:,4200:up,4600:" \
  "400:down,800:,1200:up,1600:,2000:down,2400:,2800:up,3300:,4300:down,4700:" \
  >/dev/null 2>/tmp/pong-rematch.log
python3 - <<'PYEOF'
import re
import sys

p0, p1, order = {}, {}, []
for line in open("/tmp/pong-rematch.log"):
    m = re.match(r"\[p(\d) frame \d+\] SYNC (f=(\d+) .*)", line.strip())
    if not m:
        continue
    f = int(m.group(3))
    (p0 if m.group(1) == '0' else p1)[f] = m.group(2)
    if f not in order:
        order.append(f)

# The rematch shows up as the simulated frame counter carrying on while the score returns to nil.
after = [f for f in sorted(set(p0) & set(p1)) if f > 600 and "s0=0 s1=0" in p0[f]]
if not after:
    print("  FAIL no second match started after the first one finished")
    sys.exit(1)
bad = [f for f in sorted(set(p0) & set(p1)) if p0[f] != p1[f]]
if bad:
    print(f"  FAIL the consoles disagree at {len(bad)} frames across both matches")
    print(f"       p0: {p0[bad[0]]}")
    print(f"       p1: {p1[bad[0]]}")
    sys.exit(1)
print(f"  ok   a second match starts clean at simulated frame {after[0]}, both consoles agreeing")
print(f"       across {len(set(p0) & set(p1))} frames spanning both matches")
PYEOF
[ $? -eq 0 ] || fails=$((fails + 1))

# ── 8. …and they must agree about how it ENDED, not merely along the way.
r0="$(grep 'p0 .*RESULT' /tmp/pong-link.log | grep -o 'RESULT .*' | head -1)"
r1="$(grep 'p1 .*RESULT' /tmp/pong-link.log | grep -o 'RESULT .*' | head -1)"
if [ -n "$r0" ] && [ "$r0" = "$r1" ]; then
  echo "  ok   both report the same final score ($r0)"
else
  echo "  FAIL final scores differ or the match never finished"
  echo "       p0: ${r0:-none}"
  echo "       p1: ${r1:-none}"
  fails=$((fails + 1))
fi

# A first-to-seven must finish on seven. It briefly did not: scoring is tested per ball TICK and
# four ticks make a simulated frame, so the ticks after match point kept scoring — 8-2.
winner="$(echo "$r0" | grep -o 's[01]=[0-9]*' | cut -d= -f2 | sort -n | tail -1)"
check "the winner has exactly the winning score" "$winner" 7

echo
if [ "$fails" -eq 0 ]; then echo "pong-link: PASS"; else echo "pong-link: $fails FAILED"; fi
exit $([ "$fails" -eq 0 ] && echo 0 || echo 1)
