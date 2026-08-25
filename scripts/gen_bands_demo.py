#!/usr/bin/env python3
"""Build the `bands-demo` example's one background image.

The whole example is about ONE background showing three depths at once, so the art has to make the
three obvious: each band is a different SHAPE at a different SPACING, so when they scroll at
different rates you can see them slide past each other rather than having to take it on trust.

  y   0.. 51   stars      — small dots, widely spaced, on night sky
  y  52..103   mountains  — big triangles
  y 104..159   trees      — a dense row of conifers
  y 160..255   filler     — never on screen (the GBA wraps a background at 256px, and the screen is
                            160 tall), but the image has to be a full 256x256 to tile cleanly.

Everything is drawn on an OPAQUE background: this is the furthest layer, there is nothing behind it,
and a transparent pixel here shows the backdrop colour instead.

Run: python3 scripts/gen_bands_demo.py
"""
import os
from PIL import Image, ImageDraw

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
ASSETS = os.path.join(REPO, "examples", "bands-demo", "assets")

W = H = 256                      # exactly the GBA's background wrap, so the art tiles seamlessly
SKY = (24, 20, 56)
STAR = (232, 240, 255)
MTN = (72, 64, 120)
MTN_CAP = (150, 150, 190)
TREE = (28, 76, 72)
TREE_HI = (44, 112, 96)
GROUND = (18, 14, 38)


def main():
    os.makedirs(ASSETS, exist_ok=True)
    im = Image.new("RGB", (W, H), SKY)
    d = ImageDraw.Draw(im)

    # ── stars (y 0..51) ──────────────────────────────────────────────────────────────────────────
    # Positions from a fixed table, not random: the same image every run, so a screenshot diff means
    # something changed rather than that the generator rolled differently.
    for (x, y, r) in [(12, 10, 1), (48, 22, 1), (73, 8, 2), (104, 30, 1), (131, 16, 1),
                      (160, 38, 2), (188, 12, 1), (214, 27, 1), (238, 42, 1), (30, 44, 1),
                      (92, 47, 1), (170, 6, 1), (250, 18, 2)]:
        d.ellipse([x - r, y - r, x + r, y + r], fill=STAR)

    # ── mountains (y 52..103) ────────────────────────────────────────────────────────────────────
    # Peaks at a spacing that does NOT divide 256 evenly per peak, so the range reads as a range
    # rather than as one shape stamped repeatedly — but the run as a whole still wraps at 256.
    base = 104
    for (cx, half, top) in [(20, 34, 58), (78, 26, 68), (128, 40, 52), (190, 30, 64), (246, 34, 58)]:
        d.polygon([(cx - half, base), (cx, top), (cx + half, base)], fill=MTN)
        # A cap, so the peak has an edge the eye can track as it scrolls.
        cap = top + 9
        k = (cap - top) * half // (base - top)
        d.polygon([(cx - k, cap), (cx, top), (cx + k, cap)], fill=MTN_CAP)

    # ── trees (y 104..159) ───────────────────────────────────────────────────────────────────────
    d.rectangle([0, 150, W, H], fill=GROUND)
    for i in range(0, W, 16):
        x = i + 8
        d.polygon([(x - 9, 152), (x, 112), (x + 9, 152)], fill=TREE)
        d.polygon([(x - 5, 152), (x, 124), (x + 5, 152)], fill=TREE_HI)

    # ── filler (y 160..255) ──────────────────────────────────────────────────────────────────────
    d.rectangle([0, 160, W, H], fill=GROUND)

    out = os.path.join(ASSETS, "bands.png")
    im.save(out)
    colors = len(set(im.getdata()))
    print(f"bands.png: {im.size}, {colors} colours "
          f"(a 4bpp background palette holds 16, and agb quantises at bake time)")


if __name__ == "__main__":
    main()
