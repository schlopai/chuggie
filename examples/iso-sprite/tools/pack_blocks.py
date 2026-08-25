#!/usr/bin/env python3
"""Slice the chosen isometric cubes from the vendored pack atlas into this example's sprite sheet
(a horizontal strip of 16x16 frames, the order `sheet:` indexes). Re-run after changing PICKS."""
from PIL import Image
import os
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
ATLAS = os.path.join(ROOT, "assets/iso-blocks/blocks_iso_16.png")
OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets/blocks.png")
# frame order (see main.tish): 0 grass, 1 dirt, 2 stone, 3 water, 4 sand, 5 player
PICKS = [78, 73, 57, 64, 72, 80]
COLS = 16
def quantize15(cube):
    """GBA sprites are 16 colours (15 + transparent). Each cube here has dozens of shades, so
    reduce it to <=15 opaque colours, keeping its own hue. Transparent pixels are first flooded with
    the cube's most common opaque colour so they don't burn a palette slot, then masked back out."""
    alpha = cube.getchannel("A")
    opaque = [p[:3] for p in cube.getdata() if p[3] > 127]
    fill = max(set(opaque), key=opaque.count) if opaque else (0, 0, 0)
    flooded = Image.composite(cube.convert("RGB"), Image.new("RGB", cube.size, fill),
                              alpha.point(lambda a: 255 if a > 127 else 0))
    q = flooded.quantize(colors=15, method=Image.MEDIANCUT).convert("RGBA")
    q.putalpha(alpha.point(lambda a: 255 if a > 127 else 0))
    return q

atlas = Image.open(ATLAS).convert("RGBA")
sheet = Image.new("RGBA", (16 * len(PICKS), 16), (0, 0, 0, 0))
for i, idx in enumerate(PICKS):
    c, r = idx % COLS, idx // COLS
    cube = atlas.crop((c*16, r*16, c*16+16, r*16+16))
    frame = quantize15(cube)
    n = len(set(p[:3] for p in frame.getdata() if p[3] > 127))
    sheet.paste(frame, (i*16, 0))
    print(f"  frame {i} (block {idx}): {n} colours")
sheet.save(OUT)
print(f"wrote {OUT} ({sheet.size[0]}x{sheet.size[1]}, {len(PICKS)} frames)")
