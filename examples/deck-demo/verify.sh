#!/usr/bin/env bash
# Capture deck-demo (boots into overworld) and assert audible dual-engine audio.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
root="$(cd ../.. && pwd)"

npm run build

GBA_SHOT_AUDIO=/tmp/deck-demo.wav "$root/tools/gba-shot" deck-demo.gba /tmp/deck-demo.ppm 360 2>&1 \
  | sed -n 's/^gba-shot: audio/captured:/p'

python3 - <<'PY'
import wave
import numpy as np

def load(path):
    with wave.open(path) as w:
        rate = w.getframerate()
        data = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
    return rate, data.reshape(-1, 2).mean(axis=1)

rate, mono = load("/tmp/deck-demo.wav")

def onset(sig, thresh=400):
    loud = np.abs(sig) > thresh
    return (int(np.argmax(loud)) / rate) if loud.any() else 0.0

def dominant(seg):
    seg = seg - seg.mean()
    if np.abs(seg).max() < 40:
        return 0.0
    win = seg * np.hanning(len(seg))
    spec = np.abs(np.fft.rfft(win))
    freqs = np.fft.rfftfreq(len(seg), 1 / rate)
    band = (freqs > 40) & (freqs < 8000)
    spec, freqs = spec[band], freqs[band]
    return float(freqs[int(np.argmax(spec))])

start = onset(mono)
fails = 0
peak = float(np.abs(mono).max())
print(f"onset={start:.3f}s peak={peak:.0f}")
if peak < 500:
    print("FAIL: audio too quiet (expected dual-engine overworld)")
    fails += 1

# Overworld lead opens on G4 (67) ≈ 392 Hz at bpm 108.
# Keep the window short so the wave bass (~64 Hz) doesn't steal the FFT peak.
a = int((start + 0.10) * rate)
b = int((start + 0.28) * rate)
got = dominant(mono[a:b])
want = 392.0
if got <= 0:
    print("G4: SILENCE")
    fails += 1
else:
    cents = 1200 * np.log2(got / want)
    # Vibrato + harmony can nudge the peak; still expect the lead in this early slice.
    ok = abs(cents) < 200
    print(f"G4-ish: want ~{want:.0f} got {got:.1f} err {cents:+.1f}c  {'OK' if ok else 'FAIL'}")
    if not ok:
        fails += 1

# Still sounding a second later (looping / continuing arrangement).
mid = int((start + 1.2) * rate)
mid_peak = float(np.abs(mono[mid:mid + rate // 4]).max()) if mid < len(mono) else 0
print(f"mid_peak={mid_peak:.0f}")
if mid_peak < 200:
    print("FAIL: song went silent too early")
    fails += 1

raise SystemExit(1 if fails else 0)
PY
