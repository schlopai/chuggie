#!/usr/bin/env python3
"""Assert that a deck's INTENSITY LAYERS actually engage during a captured run.

WHAT THIS IS FOR
----------------
`deck` gates tracks by layer and `deckSetIntensity(n)` enables layers <= n. `driveIntensity` in
packages/drop.tish drives that 1..3 off how deep the board has filled, so the arrangement is
supposed to thicken as the player gets into trouble.

Nothing checked that it did. A track whose top layer never comes in — because the layer number is
wrong, because a parameter silently failed to parse, because the intensity never actually reaches
3 — plays perfectly and sounds merely flat. `scripts/drop_music_check.py` confirms the tempo, which
does not change when a layer is missing.

HOW IT MEASURES
---------------
Compares a window early in the capture against one at the end, on two axes:

  * RMS — more voices, more energy. A weak signal on its own: the mixer shares headroom, so adding
    a voice raises the total much less than you would expect.
  * ZERO-CROSSING RATE — a proxy for brightness. This is the strong signal, because the layers that
    come in last are the high ones (an arpeggio in the Pocket voicing, PCM stabs in the MD3 one),
    and a high voice raises the crossing rate whether or not it raises the level.

Passing needs the brightness to rise by at least `--min-brightness` percent. It is a directional
check, not a measurement of any particular layer: it says the arrangement changed and changed
upward. Proving *which* layer arrived would need per-channel capture the emulator does not give.

SET THE BAR AT THE FAILURE MODE, NOT AT THE MEASUREMENT. Observed values vary by voicing:

    pocket-drop  (3 squares + noise)   RMS +6%   brightness +15% to +23%
    drop-solo    (MD3, wavetable+PCM)  RMS +3%   brightness  +9%

They differ because of WHAT arrives at the top: Pocket's layer 3 is a PSG arpeggio riding above the
tune, which moves the crossing rate a long way; MD3's is a PCM sawtooth stab in the middle of the
spectrum, which does not.

A ROM is deterministic, so a FIXED BINARY reproduces its figure to the percent — but the figure is
NOT stable across builds. Any change that shifts frame timing moves where in the run the intensity
transitions land, and therefore what falls inside the sampled windows. Adding one per-frame `if` to
packages/drop.tish took pocket-drop from +23% to +15%. A threshold set just under a measurement is
a test that fails on unrelated edits.

So the bar goes where the FAILURE is, not where the measurement is. A track whose top layer never
arrives shows roughly 0% or negative. Anything comfortably above zero means a voice came in. That
is all this can honestly claim, and it is the thing worth catching.

    python3 scripts/deck_intensity_check.py run.wav [--min-brightness 12] [--tail 8]
"""

import argparse
import audioop
import struct
import sys
import wave
from pathlib import Path


def mono(path: Path):
    w = wave.open(str(path))
    sw, ch, sr = w.getsampwidth(), w.getnchannels(), w.getframerate()
    data = w.readframes(w.getnframes())
    if ch == 2:
        data = audioop.tomono(data, sw, 0.5, 0.5)
    return data, sw, sr


def window(data, sw, sr, t0, t1):
    a, b = int(t0 * sr) * sw, int(t1 * sr) * sw
    seg = data[a:b]
    if len(seg) < sw * 2:
        return None
    rms = audioop.rms(seg, sw)
    s = struct.unpack(f"<{len(seg) // sw}h", seg)
    zc = sum(1 for i in range(1, len(s)) if (s[i - 1] < 0) != (s[i] < 0)) / max(1, len(s))
    return rms, zc


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("wav", type=Path)
    ap.add_argument("--min-brightness", type=float, default=12.0,
                    help="required rise in zero-crossing rate, percent (default 12)")
    ap.add_argument("--tail", type=float, default=8.0, help="seconds to sample at each end")
    args = ap.parse_args()

    data, sw, sr = mono(args.wav)
    secs = (len(data) // sw) / sr
    if secs < args.tail * 3:
        print(f"  {args.wav.name}: only {secs:.1f}s — too short to compare ends "
              f"(need {args.tail * 3:.0f}s)")
        return 1

    early = window(data, sw, sr, 2, 2 + args.tail)
    late = window(data, sw, sr, secs - args.tail - 2, secs - 2)
    if not early or not late:
        print(f"  {args.wav.name}: could not sample both ends")
        return 1

    d_rms = 100 * (late[0] - early[0]) / max(1, early[0])
    d_zc = 100 * (late[1] - early[1]) / max(1e-9, early[1])
    print(f"  {args.wav.name} over {secs:.0f}s: RMS {d_rms:+.0f}%, brightness {d_zc:+.0f}% "
          f"(want brightness >= +{args.min_brightness:.0f}%)")
    if d_zc >= args.min_brightness:
        print("  OK: the arrangement thickens as the board fills")
        return 0
    print("  FAIL: the arrangement never gained a voice — check the track `layer` numbers, that "
          "driveIntensity reaches 3, and that no `gen` parameter silently failed to parse "
          "(scripts/deck_lint.py)")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
