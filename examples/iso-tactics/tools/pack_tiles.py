#!/usr/bin/env python3
"""Slice chosen 32x32 iso tiles from the vendored Tiny Tactics tileset into this example's terrain
sheet (a strip of 32x32 frames, the order `sheet32:` indexes), and draw a cursor sprite."""
from PIL import Image, ImageDraw
import os
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
TS = os.path.join(ROOT, "assets/tiny-tactics/tinyTacticsTileset00.png")
OUT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COLS = 16
# frame order (see main.tish): 0 grass, 1 water, 2 stone-path, 3 tall-grass (elevation)
PICKS = [(0, 0), (0, 2), (2, 5), (8, 7)]
ts = Image.open(TS).convert("RGBA")
sheet = Image.new("RGBA", (32 * len(PICKS), 32), (0, 0, 0, 0))
for i, (c, r) in enumerate(PICKS):
    sheet.paste(ts.crop((c*32, r*32, c*32+32, r*32+32)), (i*32, 0))
sheet.save(os.path.join(OUT, "assets/tiles.png"))
print(f"tiles.png: {len(PICKS)} frames, {sheet.size}")

# cursor: a bright diamond outline matching the 32x16 tile top (points: top,right,bottom,left)
cur = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
d = ImageDraw.Draw(cur)
pts = [(16, 0), (31, 8), (16, 16), (0, 8)]
d.line(pts + [pts[0]], fill=(255, 240, 60, 255), width=2)
cur.save(os.path.join(OUT, "assets/cursor.png"))
print("cursor.png: 32x32 diamond outline")

# move-range highlight: a dithered cyan diamond filling the 32x16 top face. FULLY OPAQUE cyan —
# GBA sprites have no partial alpha, and include_aseprite drops non-opaque pixels, so translucency
# comes from the checkerboard dither (~50% coverage), not the alpha value.
hl = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
hp = hl.load()
for y in range(17):
    w = int((y if y <= 8 else 16 - y) / 8 * 16)  # diamond half-width 0..16..0
    for x in range(16 - w, 16 + w):
        if (x + y) % 2 == 0:
            hp[x, y] = (90, 210, 255, 255)
hl.save(os.path.join(OUT, "assets/highlight.png"))
print("highlight.png: dithered cyan diamond (move-range)")
