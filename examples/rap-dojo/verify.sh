#!/usr/bin/env bash
# Verify rap-dojo end to end: the chart is complete, the judge scores correct input, and the song
# actually sounds.
#
# The thing that makes a rhythm game hard to test is that its whole behaviour is a number nobody can
# see. A run where input is silently ignored looks, in a screenshot, exactly like a run the player
# fluffed — same screen, same characters, same "AWFUL". So the ROM logs one RESULT line when it
# reaches the results screen, and this asserts against that.
#
# The input schedule is derived from scripts/gen_rap_dojo_music.py's own PHRASES table — the same
# table that emits the song and the chart. A test carrying its own copy of the timing would keep
# passing after the chart moved underneath it.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
root="$(cd ../.. && pwd)"

npm run build >/tmp/rap-dojo-build.log 2>&1 || {
  echo "FAIL: build"; tail -30 /tmp/rap-dojo-build.log; exit 1
}

# START is pressed at frame 60; the scene fade takes 16 frames and `enter()` lands the frame after,
# so deck frame 0 is emulator frame 77. Confirmed by sweep: -3..+3 around 77 is all COOL, +-6 is
# mixed, +-9 is GOOD, +-12 is BAD/miss.
START_FRAME=60
DECK0=77
FRAMES=3000

sched() {   # $1 = press hold in frames (0 = no input at all); $2 = frames early/late, default 0
  python3 - "$1" "$START_FRAME" "$DECK0" "${2:-0}" <<'PY'
import sys
sys.path.insert(0, "../../scripts")
import gen_rap_dojo_music as m
hold, start, deck0, off = (int(x) for x in sys.argv[1:5])
ev = [(start, "start"), (start + 6, "")]
if hold > 0:
    for f, key in m.response_cues():
        ev.append((deck0 + f + off, key))
        ev.append((deck0 + f + off + hold, ""))
ev.sort()
print(",".join(f"{f}:{k}" for f, k in ev))
PY
}

run() {     # $1 = schedule -> the RESULT line
  GBA_SHOT_LOG=1 "$root/tools/gba-shot" rap-dojo.gba /tmp/rap-dojo-verify.ppm "$FRAMES" "$1" 2>&1 \
    | grep -o "RESULT .*" | head -1
}

field() { echo "$1" | grep -o "$2=[-0-9]*" | cut -d= -f2; }

fails=0
check() {   # name, got, want
  if [ "$2" = "$3" ]; then
    echo "  ok   $1 = $2"
  else
    echo "  FAIL $1 = $2 (want $3)"
    fails=$((fails + 1))
  fi
}
# ── 0. The chart and the song are on the same grid ───────────────────────────────────────────────
# Parse the notes actually written into battle.deck and check each master's-call cue lands on the
# note the song plays there.
#
# This is the assertion the suite was missing, and its absence let a real bug ship: the chart ran at
# four times the song's tempo for a whole session because one conversion read its unit as a sixteenth
# of a BEAT and the other as a sixteenth NOTE. Every other check here passed throughout — the miss
# count was right, the scored run was right, the audio was present — because they all compare the
# chart against itself. Nothing looked at the music.
echo "chart vs song:"
python3 - <<'PY'
import re, sys
sys.path.insert(0, "../../scripts")
import gen_rap_dojo_music as m

# The Lead track's notes, straight out of the emitted file.
lead, notes = False, []
for line in open("assets/battle.deck"):
    if line.startswith("track "):
        lead = " id lead " in line
        continue
    if lead:
        n = re.match(r"\s+note\s+(\d+)\s+([\d.]+)\s+[\d.]+\s+v\s+\d+", line)
        if n:
            pitch, beat = int(n.group(1)), float(n.group(2))
            notes.append((round(beat * 60.0 * m.FPS / m.BPM), pitch))
notes.sort()

want = m.call_cues()
if notes == want:
    print(f"  ok   all {len(notes)} call cues land on the note the song plays")
else:
    print(f"  FAIL chart and song disagree ({len(notes)} notes vs {len(want)} cues)")
    for got, exp in list(zip(notes, want))[:4]:
        print(f"       song frame {got[0]} pitch {got[1]}  vs  cue frame {exp[0]} pitch {exp[1]}")
    if notes and want and notes[0][0] and want[0][0]:
        print(f"       first-cue ratio {notes[0][0] / want[0][0]:.2f} (1.00 = same grid)")
    sys.exit(1)
PY
[ $? -eq 0 ] || fails=$((fails + 1))

# ── 1. No input at all ───────────────────────────────────────────────────────────────────────────
# Every response cue must time out as a miss and the run must still reach the results screen. The
# miss count is the total number of response cues in the chart, derived from PHRASES below, so this
# pins the chart's size too: a phrase silently failing to build shows up here as the wrong number.
echo "run 1 — no input:"
r1="$(run "$(sched 0)")"
[ -n "$r1" ] || { echo "  FAIL no RESULT line — the run never reached the results screen"; exit 1; }
want_misses="$(python3 -c "
import sys; sys.path.insert(0,'../../scripts')
import gen_rap_dojo_music as m; print(len(m.response_cues()))")"
check "miss"  "$(field "$r1" miss)"  "$want_misses"
check "cool"  "$(field "$r1" cool)"  0
check "good"  "$(field "$r1" good)"  0
check "bad"   "$(field "$r1" bad)"   0
check "score" "$(field "$r1" score)" 0
check "rank"  "$(field "$r1" rank)"  0

# ── 2. The right buttons on the right frames ─────────────────────────────────────────────────────
echo "run 2 — every cue played on time:"
r2="$(run "$(sched 3)")"
[ -n "$r2" ] || { echo "  FAIL no RESULT line"; exit 1; }
check "rank (3 = U RAPPIN COOL)" "$(field "$r2" rank)" 3
# `bad` is the assertion that matters. A press that registers is judged on its distance from the
# beat, so a nonzero `bad` means the clock has drifted or the judge is matching the wrong cue.
check "bad" "$(field "$r2" bad)" 0
# Every cue, on the beat, with nothing dropped. This used to be a `>= 35` threshold excused by
# scripted presses occasionally falling inside an unsampled frame — which was a wrong diagnosis of a
# real bug: the chart ran at four times the song's tempo, so cues sat 4 frames apart and a 3-frame
# press pulse genuinely could not thread them. On the correct grid the nearest cues are ~19 frames
# apart and a clean run is exact, so the suite asserts exact.
check "cool" "$(field "$r2" cool)" "$want_misses"
check "miss" "$(field "$r2" miss)" 0
echo "  ($r2)"

# ── 3. The hit window is CENTRED on the beat ────────────────────────────────
# Press every cue a fixed amount early, then the same amount late, and the two runs must score the
# same. A window that is merely wide enough can still sit off-centre, and off-centre is what a player
# feels as "it will not accept anything": before the press-time estimate in `rhythmStep`, 3 frames
# early scored 39 COOL and 3 frames late scored 33 GOOD, because input is polled once per game loop
# while the playhead advances once per display frame — so a press detected at poll time had already
# happened, on average, half a gap ago.
#
# Section 2 cannot see this: it presses exactly on the cue, which is the one offset where a
# late-biased judge still looks perfect. Only the symmetry does.
echo "hit window centring:"
for off in -3 3; do
  r="$(run "$(sched 3 "$off")")"
  check "cool at ${off} frames" "$(field "$r" cool)" "$want_misses"
done

# ── 4. The prompt lane actually scrolls ─────────────────────────────────────────────────────────
# The judge and the lane are independent: the judge reads the playhead, the lane draws from it. So
# every check above can pass while the player sees nothing move — which is exactly what happened.
# `rhythmLane`'s pixels-per-frame scale collapsed to zero, every prompt pinned itself to the hit
# marker, and the suite stayed green because nothing here looked at the picture.
#
# Asserting a RATE, not just "something changed": zero catches the collapse, and an upper bound
# catches a scale off by a factor (the unit confusion in section 0 was a 4x error, and it would show
# up here as 4x the drift).
echo "prompt lane:"
python3 - <<'PYEOF'
import sys
sys.path.insert(0, "../../scripts")
import numpy as np
from shot_check import shot

BG = np.array([36, 26, 38])
FRAMES = [420, 426, 432]

def icons(frame):
    """Left edges of the prompt icons on the master's row, right of the fixed hit marker."""
    a = shot("rap-dojo.gba", "/tmp/rap-dojo-lane.png", frame, "60:start,66:").astype(np.int32)
    band = a[0:22, 60:240]
    cols = np.where((np.abs(band - BG).sum(axis=2) > 40).any(axis=0))[0] + 60
    runs, start, prev = [], None, None
    for c in cols:
        if prev is None or c > prev + 3:
            if start is not None:
                runs.append(start)
            start = c
        prev = c
    if start is not None:
        runs.append(start)
    return runs

seen = [icons(f) for f in FRAMES]
if not any(len(r) >= 2 for r in seen):
    print(f"  FAIL no prompts drawn in the lane at frames {FRAMES} (runs {seen})")
    sys.exit(1)

# Track the rightmost icon: it is the one furthest from being judged, so it survives all three
# samples instead of scrolling off or being consumed mid-window.
xs = [max(r) for r in seen if r]
if len(xs) != len(FRAMES):
    print(f"  FAIL the lane emptied between samples (runs {seen})")
    sys.exit(1)
rate = [(b - a) / (g - f) for a, b, f, g in zip(xs, xs[1:], FRAMES, FRAMES[1:])]
if all(-3.0 <= r <= -0.5 for r in rate):
    print(f"  ok   prompts scroll left at {', '.join(f'{-r:.2f}' for r in rate)} px/frame")
else:
    print(f"  FAIL prompt drift {rate} px/frame (want -3.0..-0.5; 0 = pinned to the hit marker)")
    sys.exit(1)
PYEOF
[ $? -eq 0 ] || fails=$((fails + 1))

# ── 5. The song is actually playing ──────────────────────────────────────────────────────────────
# Pitch is not asserted: five voices sound at once here, so an FFT peak is whichever of them is
# loudest at that instant rather than a stable fact about the song. Presence and persistence are the
# claims worth making — silence would mean the deck song failed to start or the mixer stalled, and
# both are real regressions the picture cannot show.
echo "audio:"
GBA_SHOT_AUDIO=/tmp/rap-dojo.wav "$root/tools/gba-shot" rap-dojo.gba /tmp/rap-dojo-audio.ppm 1200 \
  "$START_FRAME:start,$((START_FRAME + 6)):" >/dev/null 2>&1
python3 - <<'PY'
import wave, numpy as np, sys
with wave.open("/tmp/rap-dojo.wav") as w:
    rate = w.getframerate()
    d = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").reshape(-1, 2).mean(axis=1)
# Skip the title screen, which is deliberately silent — the song starts with the lesson.
tail = d[int(rate * 2.0):]
peak = float(np.abs(tail).max()) if len(tail) else 0.0
late = d[int(rate * 12.0):int(rate * 14.0)]
late_peak = float(np.abs(late).max()) if len(late) else 0.0
ok = peak > 500 and late_peak > 200
print(f"  {'ok  ' if ok else 'FAIL'} peak={peak:.0f} (>500)  late_peak={late_peak:.0f} (>200)")
sys.exit(0 if ok else 1)
PY
[ $? -eq 0 ] || fails=$((fails + 1))

echo
if [ "$fails" -eq 0 ]; then echo "rap-dojo: PASS"; else echo "rap-dojo: $fails FAILED"; fi
exit $([ "$fails" -eq 0 ] && echo 0 || echo 1)
