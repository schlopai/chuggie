#!/usr/bin/env python3
"""Build this example's terrain sheet from the vendored Tiny Tactics tileset, plus the cursor and
move-range highlight sprites.

`tiles.png` is the FULL tileset (16x13 grid of 32x32 iso tiles) so the Tiled map can paint any tile:
`sheet32:` indexes it row-major (frame = row*16 + col) and `terrain.tsj` mirrors it 1:1. The only
transform is alpha binarization — GBA sprites (include_aseprite) drop non-opaque pixels, so we snap
each pixel to fully opaque / fully transparent to keep every tile body solid."""
from PIL import Image, ImageDraw
import os

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
TS = os.path.join(ROOT, "assets/tiny-tactics/tinyTacticsTileset00.png")
OUT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COLS = 16  # tileset is 16 tiles wide; frame index = row*COLS + col

# Named terrain frames the sample map uses (full-tileset indices). Any other tile is paintable too.
NAMED = {"grass": (0, 0), "water": (0, 2), "stone": (2, 5), "tall": (8, 7)}

ts = Image.open(TS).convert("RGBA")
# Binarize alpha: opaque tile bodies stay, anti-aliased diamond edges snap on/off (no dropped px).
px = ts.load()
for y in range(ts.height):
    for x in range(ts.width):
        r, g, b, a = px[x, y]
        px[x, y] = (r, g, b, 255 if a >= 128 else 0)
ts.save(os.path.join(OUT, "assets/tiles.png"))
cols, rows = ts.width // 32, ts.height // 32
print(f"tiles.png: full tileset {ts.size} = {cols}x{rows} = {cols * rows} frames")
for name, (c, r) in NAMED.items():
    print(f"  {name}: ({c},{r}) -> frame {r * COLS + c}")

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

# menu pointer: a solid right-pointing triangle (▶) that sits left of the selected menu item. 16x16,
# bright yellow with a 1px dark outline so it reads on any background. The menu items are fixed text;
# only this sprite moves, so the HUD never reflows.
ptr = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
pp = ptr.load()
# A small triangle (~7px tall, ~4px wide) centred in the 16x16 cell so it matches the font glyphs
# (which are far shorter than the sprite cell) instead of dwarfing them.
for y in range(16):
    reach = 4 - abs(y - 8)          # widest at the vertical centre, tapering to a point at the right
    if reach > 0:
        for x in range(4, 4 + reach):
            pp[x, y] = (255, 224, 40, 255)
out_ptr = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
op = out_ptr.load()
for y in range(16):
    for x in range(16):
        if pp[x, y][3] == 0:        # dark outline on transparent pixels touching the triangle
            if any(0 <= x + dx < 16 and 0 <= y + dy < 16 and pp[x + dx, y + dy][3] > 0
                   for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1))):
                op[x, y] = (30, 20, 0, 255)
out_ptr.alpha_composite(ptr)
out_ptr.save(os.path.join(OUT, "assets/pointer.png"))
print("pointer.png: 16x16 menu chevron")
