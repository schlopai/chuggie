#!/usr/bin/env python3
"""Assert a captured GBA run actually made sound.

The Magical Drop examples synthesise every effect on the PSG channels — no ROM, no mixer voice,
no per-frame work. That is cheap, and it is also easy to break silently: a wrong channel borrow,
a zero-length envelope or a missing `chip_borrow` just goes quiet, and nothing else in the test
suite notices. A screenshot certainly does not.

This is deliberately a PRESENCE check, not a pitch one. Asserting that a chain's rising run hits
specific frequencies would mean correlating audio windows against log timestamps, which is a lot
of machinery to protect a tune nobody has specified. Asserting that the ROM is not mute costs
nothing and catches the failure that actually happens.

Usage:  scripts/audio_check.py capture.wav
Exit status is non-zero if the capture is silent or nearly so.
"""
from __future__ import annotations

import struct
import sys
import wave
from pathlib import Path

# A PSG square at a sensible volume peaks in the thousands; anything under this is noise floor.
MIN_PEAK = 2000
# Effects are short and sparse, so most frames are legitimately quiet. Requiring a tenth of them
# to be audible distinguishes "playing sounds" from "one click at boot".
MIN_AUDIBLE_PERCENT = 10


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    path = Path(argv[1])
    if not path.exists():
        print(f"  no capture at {path}", file=sys.stderr)
        return 1

    with wave.open(str(path), "rb") as w:
        channels = w.getnchannels()
        rate = w.getframerate()
        raw = w.readframes(w.getnframes())

    samples = struct.unpack("<%dh" % (len(raw) // 2), raw)[::channels]
    if not samples:
        print("  capture is empty")
        return 1

    peak = max(abs(v) for v in samples)
    window = max(1, rate // 60)
    spans = range(0, max(1, len(samples) - window), window)
    audible = sum(1 for i in spans if max(abs(v) for v in samples[i : i + window]) > peak * 0.08)
    total = max(1, len(list(spans)))
    percent = 100 * audible // total

    print(f"  peak {peak}, audible in {percent}% of frames")
    if peak < MIN_PEAK:
        print(f"  FAIL: peak {peak} is below {MIN_PEAK} — the ROM is effectively silent")
        return 1
    if percent < MIN_AUDIBLE_PERCENT:
        print(f"  FAIL: only {percent}% of frames carry sound — expected at least {MIN_AUDIBLE_PERCENT}%")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
