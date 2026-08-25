#!/usr/bin/env bash
# Boot Cosmo Kitchen (Starport Market hub) and assert audible dual-engine audio.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
root="$(cd ../.. && pwd)"

npm run build

GBA_SHOT_AUDIO=/tmp/cosmo-kitchen.wav "$root/tools/gba-shot" cosmo-kitchen.gba /tmp/cosmo-kitchen.ppm 360 2>&1 \
  | sed -n 's/^gba-shot: audio/captured:/p'

python3 - <<'PY'
import wave
import numpy as np

def load(path):
    with wave.open(path) as w:
        rate = w.getframerate()
        data = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
    return rate, data.reshape(-1, 2).mean(axis=1)

rate, mono = load("/tmp/cosmo-kitchen.wav")

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
    print("FAIL: audio too quiet (expected dual-engine hub)")
    fails += 1

# Wave bass often wins a full-band FFT (~65 Hz). Require midrange energy from the
# spoon lead / PCM hats (C4+), proving dual-engine content beyond the bass bed.
a = int((start + 0.05) * rate)
b = int((start + 0.45) * rate)
seg = mono[a:b] - mono[a:b].mean()
win = seg * np.hanning(len(seg))
spec = np.abs(np.fft.rfft(win))
freqs = np.fft.rfftfreq(len(seg), 1 / rate)
mid_band = (freqs > 200) & (freqs < 4000)
mid_e = float(spec[mid_band].sum()) if mid_band.any() else 0.0
low_band = (freqs > 40) & (freqs < 120)
low_e = float(spec[low_band].sum()) if low_band.any() else 0.0
got = dominant(mono[a:b])
print(f"fft_peak={got:.1f}Hz mid_e={mid_e:.0f} low_e={low_e:.0f}")
if mid_e < 1e5:
    print("FAIL: missing midrange (expected pulse lead / PCM)")
    fails += 1
if low_e < 1e4:
    print("FAIL: missing low end (expected wave bass)")
    fails += 1

mid = int((start + 1.2) * rate)
mid_peak = float(np.abs(mono[mid:mid + rate // 4]).max()) if mid < len(mono) else 0
print(f"mid_peak={mid_peak:.0f}")
if mid_peak < 200:
    print("FAIL: song went silent too early")
    fails += 1

raise SystemExit(1 if fails else 0)
PY
