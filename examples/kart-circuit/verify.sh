#!/usr/bin/env bash
# Verify kart-circuit end to end: a race can be completed, the opponents drive themselves, the
# surface under the kart actually matters, and a lap cannot be faked.
#
# The hard part of testing a racer is that almost everything worth asserting is a number nobody can
# see, and the obvious assertions are circular — a lap counter checked against the same code that
# increments it proves nothing. So each check below is chosen to be able to FAIL for a real reason,
# and the ROM emits two kinds of line for them to read: one `RESULT` at the end of a race, and one
# `TEL` telemetry line a second while racing.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
. ../../scripts/verify_common.sh
root="$(cd ../.. && pwd)"
unset CARGO_TARGET_DIR

npm run build >/tmp/kart-circuit-build.log 2>&1 || {
  echo "FAIL: build"; tail -30 /tmp/kart-circuit-build.log; exit 1
}

# The lights go out about 300 emulator frames in: title, START, a scene fade, then a 150-frame
# countdown. Anything that wants the player driving must start after that.
GO=320
FRAMES=4200

run() {   # $1 = schedule, $2 = frames -> the ROM's log
  GBA_SHOT_LOG=1 "$root/tools/gba-shot" kart-circuit.gba /tmp/kart-verify.ppm "${2:-$FRAMES}" "$1" 2>&1
}
field() { echo "$1" | grep -o "$2=[-0-9]*" | cut -d= -f2; }

fails=0
check() {   # name, got, want
  if [ "$2" = "$3" ]; then echo "  ok   $1 = $2"
  else echo "  FAIL $1 = $2 (want $3)"; fails=$((fails + 1)); fi
}
check_num() {   # name, got, lo, hi
  if [ -n "$2" ] && [ "$2" -ge "$3" ] && [ "$2" -le "$4" ]; then echo "  ok   $1 = $2 (in $3..$4)"
  else echo "  FAIL $1 = ${2:-nothing} (want $3..$4)"; fails=$((fails + 1)); fi
}

# ── 1. A race can actually be completed ─────────────────────────────────────────────────────────
# Attract mode (SELECT) hands the player's kart to the same driver the opponents use. It is the only
# way to exercise a WHOLE race: a fixed button schedule cannot steer a circuit it gets no feedback
# from, and a hand-tuned one would be testing the schedule rather than the game.
echo "demo race:"
demo="$(run "60:select,66:")"
r1="$(echo "$demo" | grep -o 'RESULT .*' | head -1)"
[ -n "$r1" ] || { echo "  FAIL no RESULT — the race never reached the results screen"; exit 1; }
check "laps"     "$(field "$r1" laps)" 3
check "finished" "$(field "$r1" fin)"  1
check_num "position" "$(field "$r1" pos)" 1 4
# A plausible race, not just a terminating one: three laps of a 1188px circuit at a top speed of
# 2.55px/frame cannot be quicker than about 700 frames, and a driver that is merely wandering takes
# far longer than 2500.
check_num "total frames" "$(field "$r1" time)" 700 2500
check_num "best lap"     "$(field "$r1" best)" 200 900

# ── 2. The opponents drive themselves ───────────────────────────────────────────────────────────
# Start a REAL race and never touch a button. If the AI were somehow being carried by the player's
# input — or if positions were computed from one shared number — this could not come out right.
echo "idle player:"
idle="$(run "60:start,66:")"
r2="$(echo "$idle" | grep -o 'RESULT .*' | head -1)"
[ -n "$r2" ] || { echo "  FAIL no RESULT — the race never ended, so nobody finished"; exit 1; }
check "position" "$(field "$r2" pos)"  4
check "laps"     "$(field "$r2" laps)" 0
check "finished" "$(field "$r2" fin)"  0
# …and the negative control for the drift test below: a stationary kart cannot earn a mini-turbo.
check "turbos while parked" "$(echo "$idle" | grep -c 'TURBO')" 0

# ── 3. The surface under the kart is real ───────────────────────────────────────────────────────
# Hold the accelerator with no steering: the kart runs up the start straight on tarmac and then off
# into the grass at the first bend. Its top speed must collapse. This is what catches a surface mask
# that is all-road, or one misaligned with the art it was generated from — neither of which any
# amount of lap counting would notice.
echo "off-road:"
off="$(run "60:start,66:,${GO}:0x1,1400:0x1" 1500)"
on_max="$(echo "$off"  | grep -o 'TEL .*surf=[12] ' | grep -o 'spd=[-0-9]*' | cut -d= -f2 | sort -n | tail -1)"
gr_max="$(echo "$off"  | grep -o 'TEL .*surf=0 '    | grep -o 'spd=[-0-9]*' | cut -d= -f2 | sort -n | tail -1)"
if [ -z "$on_max" ] || [ -z "$gr_max" ]; then
  echo "  FAIL the run never sampled both tarmac and grass (on=${on_max:-none} grass=${gr_max:-none})"
  fails=$((fails + 1))
elif [ "$gr_max" -lt $((on_max / 2)) ]; then
  echo "  ok   tarmac tops out at $on_max, grass at $gr_max (under half)"
else
  echo "  FAIL grass ($gr_max) is not meaningfully slower than tarmac ($on_max)"
  fails=$((fails + 1))
fi

# ── 4. A lap cannot be faked ────────────────────────────────────────────────────────────────────
# The grid sits behind the start line, so driving forward crosses it immediately. This run crosses
# it, reverses back over it, and crosses again — three times over the finish line without ever going
# round the course. A lap counter written as "crossed the line" awards laps here; ordered gates do
# not. Nothing else in this suite tests the ordering.
echo "lap cannot be faked:"
cheat="$(run "60:start,66:,${GO}:0x1,420:0x2,700:0x1,820:0x2,1100:0x1,1300:0x2")"
r3="$(echo "$cheat" | grep -o 'RESULT .*' | head -1)"
moved="$(echo "$cheat" | grep -o 'spd=[-0-9]*' | cut -d= -f2 | sort -n | tail -1)"
check "laps after crossing the line repeatedly" "$(field "$r3" laps)" 0
check_num "…and the kart really was moving" "$moved" 100 400

# ── 5. Drift charges a mini-turbo ───────────────────────────────────────────────────────────────
# Asserted on the demo race, because holding a drift requires staying on the road through a corner,
# which again is something a fixed schedule cannot do. Paired with the parked-kart control in check 2,
# this says turbos come from drifting rather than from merely existing.
echo "drift:"
turbos="$(echo "$demo" | grep -c 'TURBO')"
check_num "mini-turbos in a race" "$turbos" 1 40

# ── 6. Items exist, are collected, and turn into things on the course ───────────────────────────
# `picks` counts boxes the player drove through; `haz` is how many shells and bananas are live at
# that moment. Both being non-zero says the whole chain works: boxes sit where the racing line
# actually goes, a kart can collect one, and firing it puts something on the track. The parked-kart
# run is the control again — it collects nothing, because it never moves.
echo "items:"
check_num "boxes collected in a race" "$(field "$r1" picks)" 1 60
check "…and none while parked" "$(field "$r2" picks)" 0
haz_max="$(echo "$demo" | grep -o 'haz=[0-9]*' | cut -d= -f2 | sort -n | tail -1)"
check_num "shells/bananas live at once" "$haz_max" 1 8

# ── 7. The music plays, and the intensity stems actually gate ───────────────────────────────────
# One song, one playhead, four stems: the title runs at intensity 1 and the race at 2, so the race
# must be measurably louder. Asserting only "there is sound" would pass with every stem stuck on,
# which is exactly the failure that would make the final lap feel like nothing happened.
echo "music:"
GBA_SHOT_AUDIO=/tmp/kart-verify.wav "$root/tools/gba-shot" kart-circuit.gba /tmp/kart-verify.ppm \
  1500 "60:select,66:" >/dev/null 2>&1
python3 - <<'PYEOF2'
import sys, wave
import numpy as np
with wave.open("/tmp/kart-verify.wav") as w:
    rate = w.getframerate()
    d = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").reshape(-1, 2).mean(axis=1)

def rms(t0, t1):
    seg = d[int(rate * t0):int(rate * t1)].astype(float)
    return float(np.sqrt((seg ** 2).mean())) if len(seg) else 0.0

# ⚠️ The title window is SHORT and the boundary is sharp: START lands on frame 60 (1.0 s) and the
# race scene raises the intensity almost immediately, so a window that runs to 3 s is mostly race
# audio and the two readings come out nearly equal. That is what a first pass at this measured.
# RMS rather than peak, too — a stem gate changes how DENSE the arrangement is, and peak barely
# moves when one more quiet voice joins.
title, race = rms(0.15, 0.95), rms(6.0, 12.0)
ok = title > 1500 and race > title * 1.15
print(f"  {'ok  ' if ok else 'FAIL'} title stem rms {title:.0f}, race stem {race:.0f} "
      f"(want sound, and the race at least 15% denser)")
sys.exit(0 if ok else 1)
PYEOF2
[ $? -eq 0 ] || fails=$((fails + 1))

# ── 8. The floor renders cleanly ────────────────────────────────────────────────────────────────
# A Mode 7 plane driven by an HBlank DMA has a specific failure: one stray scanline, at a height that
# moves with the frame cost. It is invisible in any single screenshot, so sample many frames and look
# for a row that disagrees with BOTH neighbours while they agree with each other.
echo "floor:"
python3 - <<'PY'
import sys
sys.path.insert(0, "../../scripts")
import numpy as np
from shot_check import shot

bad, n = 0, 0
for f in range(360, 1400, 47):
    a = shot("kart-circuit.gba", "/tmp/kart-verify.png", f, "60:select,66:").astype(np.int32)
    n += 1
    reg = a[78:150]                     # floor rows, clear of the horizon haze and the HUD
    dp = np.abs(reg[1:-1] - reg[:-2]).mean(axis=(1, 2))
    dn = np.abs(reg[1:-1] - reg[2:]).mean(axis=(1, 2))
    ds = np.abs(reg[2:] - reg[:-2]).mean(axis=(1, 2))
    odd = np.where((dp > 18) & (dn > 18) & (ds < 9))[0] + 79
    if len(odd):
        bad += len(odd)
        print(f"       frame {f}: stray scanline at y={list(odd)}")
print(f"  {'ok  ' if bad == 0 else 'FAIL'} {bad} stray scanlines across {n} frames")
sys.exit(0 if bad == 0 else 1)
PY
[ $? -eq 0 ] || fails=$((fails + 1))

# ── 9. Nothing quietly died ─────────────────────────────────────────────────────────────────────
# `soak_rom` checks three things a log grep cannot: crash strings, the halt agb's allocation-failure
# handler performs WITHOUT logging, and whether the ROM was still painting frames at the end.
echo "soak:"
if soak_rom kart-circuit.gba 2400 "60:select,66:" >/tmp/kart-soak.log 2>&1; then
  echo "  ok   2400 frames, no crash and still rendering"
else
  echo "  FAIL soak"; tail -12 /tmp/kart-soak.log; fails=$((fails + 1))
fi

echo
if [ "$fails" -eq 0 ]; then echo "kart-circuit: PASS"; else echo "kart-circuit: $fails FAILED"; fi
exit $([ "$fails" -eq 0 ] && echo 0 || echo 1)
