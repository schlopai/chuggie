#!/usr/bin/env python3
"""Assemble a sequence of PPM frames (from `tools/gba-shot` with GBA_SHOT_SEQ) into a looping GIF.

Kept separate from scripts/gif.sh so it can be run and tested on its own, like the other
scripts/*_check.py helpers. Pillow only — no ImageMagick, no ffmpeg.
"""
import argparse
import math
import sys
from collections import Counter

from PIL import Image

# Pixel budget for the throwaway image the shared palette is median-cut from. Big enough that a
# colour's share of it still reflects its share of the clip; small enough to stay instant.
PALETTE_SAMPLE_PIXELS = 1 << 20


def build_palette(frames):
    """One 256-colour palette for the whole clip, weighted by how much screen each colour holds.

    Quantising each frame on its own instead gives every frame a different palette, and then GIF's
    frame-to-frame delta compression is comparing palette INDEXES that no longer mean the same
    colour — the clip comes out as garbage. Sampling only some frames is no good either: a fade or
    a flash lives on frames the sample skips, and those frames then snap to whatever unrelated
    shade is nearest. So the histogram covers EVERY frame.
    """
    hist = Counter()
    for im in frames:
        # getcolors(None) would cap out; a GBA screen is 15-bit so the true ceiling is 32768.
        for count, colour in im.getcolors(maxcolors=1 << 16) or []:
            hist[colour] += count

    total = sum(hist.values())
    scale = min(1.0, PALETTE_SAMPLE_PIXELS / total)
    pixels = []
    for colour, count in hist.items():
        pixels.extend([colour] * max(1, int(count * scale)))   # every colour gets at least one vote

    side = math.ceil(math.sqrt(len(pixels)))
    sample = Image.new("RGB", (side, side), pixels[0])
    sample.putdata(pixels)
    return sample.quantize(colors=256, method=Image.MEDIANCUT)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("frames", nargs="+", help="PPM frames, in playback order")
    ap.add_argument("--out", required=True, help="output .gif")
    ap.add_argument("--delay-cs", type=int, default=5,
                    help="per-frame delay in centiseconds (default 5)")
    ap.add_argument("--scale", type=int, default=2,
                    help="integer upscale factor, nearest-neighbour (default 2)")
    args = ap.parse_args()

    frames = [Image.open(p).convert("RGB") for p in args.frames]
    if not frames:
        print("ppm_to_gif: no frames", file=sys.stderr)
        return 1

    palette = build_palette(frames)
    imgs = []
    for im in frames:
        p = im.quantize(palette=palette, dither=Image.Dither.NONE)
        if args.scale > 1:
            # Upscale AFTER quantising, in palette space: NEAREST on indexes is exact (no new
            # colours to approximate) and there are 1/scale² as many pixels to convert.
            # NEAREST always — pixel art must not be smoothed, and an integer factor keeps every
            # source pixel a crisp square block.
            p = p.resize((p.width * args.scale, p.height * args.scale), Image.NEAREST)
        imgs.append(p)

    imgs[0].save(
        args.out,
        save_all=True,
        append_images=imgs[1:],
        # Pillow takes milliseconds; GIF stores centiseconds, so feed it a multiple of 10.
        duration=max(2, args.delay_cs) * 10,
        loop=0,
        optimize=True,
        # 1 = leave the frame in place. `optimize` writes each frame as only the rectangle that
        # CHANGED, so the previous frame has to stay underneath — disposing to background instead
        # erases the picture and leaves the delta floating on an empty screen.
        disposal=1,
    )
    print(f"ppm_to_gif: {len(imgs)} frames -> {args.out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
