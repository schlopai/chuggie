#!/usr/bin/env python3
"""Capture the single most informative frame of a ROM as a PNG.

A shot tuned to one frame number can miss the picture entirely: the diagnostic ROMs paint their
output for a frame or two and then clear, and plenty of examples spend their tuned frame on a
transition. So this scans a whole run and keeps the frame with the MOST distinct colours — the
closest cheap proxy there is for "the frame with the most on it".

Uses tools/gba-shot's sequence mode (GBA_SHOT_SEQ), so it is one emulator boot, not one per frame.

Exits 2 when the best frame found is still a dead screen — at most MIN_COLOURS distinct colours,
the same bar scripts/shot_check.py uses to call a frame a crash page rather than a picture.

Usage: python3 scripts/best_still.py <rom.gba> <out.png> [frames] [keys]
"""
import os
import subprocess
import sys
import tempfile
from glob import glob

from PIL import Image

MIN_COLOURS = 4          # matches assert_live() in scripts/shot_check.py
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    rom, out = sys.argv[1], sys.argv[2]
    frames = sys.argv[3] if len(sys.argv) > 3 else "400"
    keys = sys.argv[4] if len(sys.argv) > 4 else ""

    with tempfile.TemporaryDirectory() as tmp:
        env = dict(
            os.environ,
            GBA_SHOT_SEQ=tmp,
            GBA_SHOT_SEQ_FROM="0",
            GBA_SHOT_SEQ_EVERY="1",
            GBA_SHOT_SEQ_MAX=frames,
            # Keep the blanks: the blank guard is for where a CLIP should start, and here a blank
            # frame simply loses the comparison on colour count anyway.
            GBA_SHOT_SEQ_BLANK="1",
        )
        r = subprocess.run([os.path.join(ROOT, "tools", "gba-shot"), rom,
                            os.path.join(tmp, "_final.ppm"), frames, keys],
                           env=env, capture_output=True, text=True)
        if r.returncode != 0:
            sys.stderr.write(r.stderr)
            return 1

        best, best_n = None, -1
        for f in sorted(glob(os.path.join(tmp, "f*.ppm"))):
            im = Image.open(f).convert("RGB")
            n = len(im.getcolors(1 << 16) or [])
            if n > best_n:
                best_n, best = n, im.copy()

        if best is None:
            print("best_still: no frames captured", file=sys.stderr)
            return 1
        if best_n <= MIN_COLOURS:
            print(f"best_still: dead screen — the best frame in {frames} has only {best_n} "
                  f"distinct colour(s); this ROM never draws anything.", file=sys.stderr)
            return 2
        best.save(out)
        print(f"best_still: {best_n} colours -> {out}", file=sys.stderr)
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
