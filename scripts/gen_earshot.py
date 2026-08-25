#!/usr/bin/env python3
"""The one sound `examples/earshot` needs: a short mono blip.

Generated rather than taken from another example, for the same reason `scripts/gen_golf_art.py`
draws a golf ball — the pack has no sound effects, and copying `bg-demo`'s is the cross-example
dependency that left `bench-room` and `repro-hub-cave-heap` broken for two weeks.

Format follows the two `.wav` assets already in the repo, which is what agb's `include_wav!` is
happy with here: **mono, 16-bit, 10512 Hz**. (The mixer's samples are signed — `docs/MEMORY.md`
records a deck bug where offset-binary PCM came out 42 dB down — and 16-bit PCM in a WAV already is.)

A decaying square-ish tone: loud, brief, and broadband enough that a stereo capture can be measured
per channel without any pitch analysis.

    python3 scripts/gen_earshot.py
"""
import math
import pathlib
import struct
import wave

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "examples/earshot/assets"
RATE = 10512
MS = 140
FREQ = 440.0


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    n = int(RATE * MS / 1000)
    frames = bytearray()
    for i in range(n):
        t = i / RATE
        env = (1.0 - i / n) ** 2          # quick decay, so repeats do not smear together
        s = 1.0 if math.sin(2 * math.pi * FREQ * t) >= 0 else -1.0
        # A touch of the octave keeps it from sounding like a test tone.
        s = 0.75 * s + 0.25 * math.sin(2 * math.pi * FREQ * 2 * t)
        v = int(max(-1.0, min(1.0, s * env)) * 30000)
        frames += struct.pack("<h", v)
    with wave.open(str(OUT / "blip.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(bytes(frames))
    print(f"  blip.wav     mono 16-bit {RATE} Hz, {n} frames ({MS} ms)")


if __name__ == "__main__":
    print("earshot audio")
    main()
