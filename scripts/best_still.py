#!/usr/bin/env python3
"""Capture the single most informative frame of a ROM as a PNG.

A shot tuned to one frame number can miss the picture entirely: the diagnostic ROMs paint their
output for a frame or two and then clear, and plenty of examples spend their tuned frame on a
transition. So this scans a whole run and keeps the frame with the MOST distinct colours — the
closest cheap proxy there is for "the frame with the most on it".

Uses tools/gba-shot's sequence mode (GBA_SHOT_SEQ), so it is one emulator boot, not one per frame.

Exits 2 when the best frame found has no picture on it at all — a flat screen, or a bare
two-colour fill.

Usage: python3 scripts/best_still.py <rom.gba> <out.png> [frames] [keys]
"""
import os
import subprocess
import sys
import tempfile
from glob import glob

from PIL import Image

# What counts as "no picture on it". Deliberately NOT a colour count: white text on a solid backdrop
# is exactly two colours and is a perfectly good preview (repro-654-agg-push draws its name and
# result that way), while bench-access's best frame is ALSO two colours and is nothing — a
# half-white/half-black wipe. The difference is not how many colours but how they are distributed:
# a drawing puts a small amount of ink on a background; a blank or a wipe is a big flat fill.
FLAT_SHARE = 0.999      # one colour covering this much of the screen is a blank
INK_MAX_SHARE = 0.20    # in a two-colour frame, a "minority" bigger than this is a fill, not ink

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

        def is_drawing(im):
            """Something was drawn here, as opposed to the screen being blank or filled."""
            counts = sorted((n for n, _ in im.getcolors(1 << 16)), reverse=True)
            total = im.width * im.height
            if len(counts) < 2 or counts[0] / total >= FLAT_SHARE:
                return False                      # one flat colour: a blank
            if len(counts) == 2 and counts[1] / total > INK_MAX_SHARE:
                return False                      # a big two-colour fill: a wipe, not a picture
            return True

        # Test every frame as it is scanned, rather than picking the most colourful frame and
        # testing that one at the end. Those are different answers when frames tie on colour count:
        # repro-654-agg-push's wipe and its final readout are both two colours, and taking the first
        # threw away the readout. Ties now go to the LATER frame — the settled screen.
        best, best_n = None, -1
        for f in sorted(glob(os.path.join(tmp, "f*.ppm"))):
            im = Image.open(f).convert("RGB")
            if not is_drawing(im):
                continue
            n = len(im.getcolors(1 << 16) or [])
            if n >= best_n:
                best_n, best = n, im.copy()

        if best is None:
            print(f"best_still: no picture — nothing in {frames} frames is more than a flat "
                  f"screen or a two-colour fill; this ROM never draws anything.", file=sys.stderr)
            return 2
        best.save(out)
        print(f"best_still: {best_n} colours -> {out}", file=sys.stderr)
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
