#!/usr/bin/env bash
# Assert what the PSG actually played, by capturing the emulated audio and measuring it.
#
#   ./verify.sh
#
# `tones.gba` plays one known pitch per one-second window (see src/tones.tish); this captures the
# sound with `gba-shot`'s GBA_SHOT_AUDIO and FFTs each window, then checks the dominant frequency
# against what was asked for. A note table that is a semitone out, an octave low, or silently
# clamped all sound plausible to a person and are all caught here.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
root="$(cd ../.. && pwd)"

GBA_SHOT_AUDIO=/tmp/chiptune-tones.wav "$root/tools/gba-shot" tones.gba /tmp/chiptune.ppm 400 2>&1 \
  | sed -n 's/^gba-shot: audio/captured:/p'
GBA_SHOT_AUDIO=/tmp/chiptune-song.wav "$root/tools/gba-shot" song.gba /tmp/chiptune.ppm 400 2>&1 \
  | sed -n 's/^gba-shot: audio/captured:/p'

python3 - "$@" <<'PY'
import struct, sys, wave
import numpy as np

# (window index, label, expected Hz or None for "noise: broadband", tolerance in cents)
EXPECT = [
    (0, "ch1 square A4", 440.00),
    (1, "ch1 square A5", 880.00),
    (2, "ch2 square E4", 329.63),
    (3, "ch3 wave   A4", 440.00),
    (4, "ch4 noise",     None),
    (5, "silence",       0.0),
]

def load(path):
    with wave.open(path) as w:
        rate = w.getframerate()
        data = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
    return rate, data.reshape(-1, 2).mean(axis=1)

rate, mono = load("/tmp/chiptune-tones.wav")

# Windows are counted from the first audible sample rather than from reset. Boot time is not a
# constant — it differs per ROM and moved by a factor of twelve the last time the compiler was
# fixed — so a hardcoded offset silently slides the analysis onto the neighbouring note and reports
# a scale that is uniformly a tone sharp.
FRAMES_PER_WINDOW = 60
SEC = FRAMES_PER_WINDOW / 59.7275

def onset(sig, thresh=500):
    loud = np.abs(sig) > thresh
    return (int(np.argmax(loud)) / rate) if loud.any() else 0.0

start = onset(mono)

def dominant(seg):
    seg = seg - seg.mean()
    if np.abs(seg).max() < 50:
        return 0.0, 0.0
    win = seg * np.hanning(len(seg))
    spec = np.abs(np.fft.rfft(win))
    freqs = np.fft.rfftfreq(len(seg), 1 / rate)
    band = (freqs > 40) & (freqs < 8000)
    spec, freqs = spec[band], freqs[band]
    peak = int(np.argmax(spec))
    # Spectral flatness separates a tone (one tall spike) from noise (energy everywhere).
    flat = float(np.exp(np.mean(np.log(spec + 1e-9))) / (np.mean(spec) + 1e-9))
    return float(freqs[peak]), flat

fails = 0
print()
print(f"{'WINDOW':<16} {'EXPECTED':>10} {'MEASURED':>10} {'ERROR':>9}   RESULT")
print(f"{'-'*16} {'-'*10:>10} {'-'*10:>10} {'-'*9:>9}   ------")
for idx, label, want in EXPECT:
    a = int((start + idx * SEC + 0.25) * rate)
    b = int((start + idx * SEC + 0.90) * rate)
    got, flat = dominant(mono[a:b])
    if want is None:                       # noise: assert broadband, not a pitch
        ok = flat > 0.02
        detail, err = f"{'noise':>10}", f"{'flat ' + format(flat, '.3f'):>9}"
    elif want == 0.0:                      # silence: assert nothing is playing
        ok = got == 0.0
        detail, err = f"{'silent':>10}", f"{'':>9}"
        if not ok:
            detail = f"{got:>10.1f}"
    else:
        ok = got > 0 and abs(1200 * np.log2(got / want)) < 40   # within 40 cents
        cents = 1200 * np.log2(got / want) if got > 0 else float("-inf")
        detail, err = f"{got:>10.1f}", f"{cents:>+8.0f}c"
    want_s = "noise" if want is None else ("silent" if want == 0.0 else f"{want:.1f}")
    print(f"{label:<16} {want_s:>10} {detail} {err}   {'PASS' if ok else 'FAIL'}")
    if not ok:
        fails += 1

# ── the `chip:` pipeline: does the compiled song play the notes it was written with? ──────────────
# assets/scale.chip is a C major scale at 30 frames/row. Checking every row catches the failures a
# listener never would: a note table a semitone out, a tempo that drifts, a sequencer off by a row.
SCALE = [("C4", 261.63), ("D4", 293.66), ("E4", 329.63), ("F4", 349.23),
         ("G4", 392.00), ("A4", 440.00), ("B4", 493.88), ("C5", 523.25)]
ROW_SEC = 30 / 59.7275

rate, mono = load("/tmp/chiptune-song.wav")
song_start = onset(mono)
print(f"{'SONG ROW':<16} {'EXPECTED':>10} {'MEASURED':>10} {'ERROR':>9}   RESULT")
print(f"{'-'*16} {'-'*10:>10} {'-'*10:>10} {'-'*9:>9}   ------")
for i, (name, want) in enumerate(SCALE):
    a = int((song_start + i * ROW_SEC + 0.10) * rate)
    b = int((song_start + i * ROW_SEC + ROW_SEC - 0.05) * rate)
    got, _ = dominant(mono[a:b])
    cents = 1200 * np.log2(got / want) if got > 0 else float("-inf")
    ok = got > 0 and abs(cents) < 40
    print(f"{'row ' + str(i) + ' ' + name:<16} {want:>10.1f} {got:>10.1f} {cents:>+8.0f}c   {'PASS' if ok else 'FAIL'}")
    if not ok:
        fails += 1

print()
sys.exit(1 if fails else 0)
PY
