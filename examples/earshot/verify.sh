#!/usr/bin/env bash
# earshot verify — the panning is MEASURED, not listened to.
#
# Audio is the one subsystem a screenshot cannot check, and "it sounded fine" is not a test. This
# captures a stereo WAV and asserts, per blip, that the loud channel is the side the source was on.
set -euo pipefail
cd "$(dirname "$0")"
fail=0
CRASH='Bad memory|Unimplemented memory|panicked at|Illegal opcode|Jumped to invalid address'

npm run build >/tmp/earshot-build.log 2>&1 || { echo "  FAIL build"; exit 1; }
echo "  ok   build"

log=$(GBA_SHOT_LOG=1 GBA_SHOT_AUDIO=/tmp/earshot.wav ../../scripts/screenshot.sh earshot.gba /tmp/earshot.png 500 2>&1)
# ⚠️ BASH STRING MATCH, NOT `grep -q`. Under `set -o pipefail`, `echo "$x" | grep -q PATTERN`
# reports FAILURE when the pattern MATCHES: grep -q exits the moment it finds one, `echo` is killed
# by SIGPIPE, and pipefail propagates that. This check read "FAIL no blips" while the log plainly
# contained eleven of them.
if [[ "$log" =~ $CRASH ]]; then echo "  FAIL crashed"; fail=1; else echo "  ok   no crash"; fi
if [[ "$log" == *"EARSHOT blip"* ]]; then echo "  ok   the ROM placed sounds"; else echo "  FAIL no blips"; fail=1; fi

python3 - <<'PY' || fail=1
import wave, struct, math, sys
w = wave.open('/tmp/earshot.wav'); n = w.getnframes(); rate = w.getframerate()
s = struct.unpack('<%dh' % (n*2), w.readframes(n)); L = s[0::2]; R = s[1::2]
peak = max(abs(x) for x in s)
if peak < 1000:
    print(f"  FAIL capture is silent (peak {peak}) - the mixer never ran"); sys.exit(1)
print(f"  ok   capture has audio (peak {peak})")

W = int(0.05*rate); peaks = []
for k in range(len(L)//W):
    a, b = L[k*W:(k+1)*W], R[k*W:(k+1)*W]
    lr = int(math.sqrt(sum(x*x for x in a)/len(a))); rr = int(math.sqrt(sum(x*x for x in b)/len(b)))
    if max(lr, rr) > 800: peaks.append((k*0.05, lr, rr))
rows = []; last = -9
for t, l, r in peaks:
    if t - last > 0.3: rows.append([t, l, r])
    elif max(l, r) > max(rows[-1][1], rows[-1][2]): rows[-1] = [t, l, r]
    last = t
pans = [153, 140, 107, 57, 0, -59, -107, -140, -153]
if len(rows) < len(pans):
    print(f"  FAIL only {len(rows)} blips in the capture, wanted {len(pans)}"); sys.exit(1)
bad = 0
for i, p in enumerate(pans):
    _, l, r = rows[i]
    exp = "right" if p > 40 else ("left" if p < -40 else "centre")
    got = "right" if r > l*1.2 else ("left" if l > r*1.2 else "centre")
    if exp != got:
        print(f"  FAIL blip {i} pan={p}: expected {exp}, measured {got} (L {l} R {r})"); bad += 1
if bad: sys.exit(1)
print(f"  ok   all {len(pans)} blips panned to the correct side")
loud = max(max(l, r) for _, l, r in rows)
quiet = min(max(l, r) for _, l, r in rows)
if loud < quiet*1.3:
    print(f"  FAIL distance does not attenuate ({loud} vs {quiet})"); sys.exit(1)
print(f"  ok   distance attenuates ({loud} near vs {quiet} far)")
PY

if [ "$fail" -eq 0 ]; then echo; echo "earshot: PASS"; else echo; echo "earshot: FAIL"; exit 1; fi
