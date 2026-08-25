#!/usr/bin/env python3
"""Generate the portrait sheet for examples/dialog-demo.

Sourced entirely from the vendored Ninja Adventure catalog (assets/ninja-adventure/):
each character ships a 38x38 `Faceset.png`. GBA sprites must be a power-of-two cell
(8/16/32/64), so we downscale each face to 32x32 and quantize it to <=15 colours (one
16-colour sprite palette, index 0 = transparent), then pack them left-to-right into a
`sheet32:` strip — one 32x32 frame per portrait, frame index == position.

Outputs:  examples/dialog-demo/assets/faces32.png

Run:  python3 scripts/gen_dialog_demo.py
"""
import os
from PIL import Image, ImageDraw

CURSOR_FRAME = 4   # appended after the 4 portraits — the ► choice cursor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
OUT = os.path.join(ROOT, "examples", "dialog-demo", "assets")
os.makedirs(OUT, exist_ok=True)

CELL = 32

# Portraits in FRAME ORDER — these indices must match src/main.tish.
CHARS = [
    ("Princess", 0),          # 0  the princess
    ("OldMan3", 1),           # 1  the elder / sage
    ("KnightGold", 2),        # 2  the knight
    ("SorcererOrange", 3),    # 3  the sorcerer
]


def face(name):
    im = Image.open(os.path.join(PACK, "Actor", "Character", name, "Faceset.png")).convert("RGBA")
    # 38x38 -> 32x32. LANCZOS keeps the features; the quantize below re-hardens edges.
    im = im.resize((CELL, CELL), Image.LANCZOS)
    alpha = im.split()[3]
    q = im.convert("RGB").quantize(colors=15, method=Image.MEDIANCUT, dither=Image.NONE).convert("RGBA")
    q.putalpha(alpha.point(lambda a: 255 if a >= 128 else 0))
    px = q.load()
    for y in range(q.height):
        for x in range(q.width):
            r, g, b, a = px[x, y]
            if a == 0:
                px[x, y] = (0, 0, 0, 0)
    return q


def cursor_cell():
    """A small, softly-rounded yellow ► in the top-left of a 32px cell (matches the shop cursor).
    Optical centre ~y=6; packages/ui pointerAtRow centres it on the text row."""
    cur = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    d = ImageDraw.Draw(cur)
    yl = (0xFF, 0xE0, 0x6A, 255)
    oy = 3
    d.polygon([(1, 0 + oy), (7, 3 + oy), (1, 6 + oy)], fill=yl)
    for (rx, ry) in [(1, 0 + oy), (1, 6 + oy), (7, 3 + oy)]:
        cur.putpixel((rx, ry), (0, 0, 0, 0))
    cur.putpixel((6, 3 + oy), yl)
    return cur


def build():
    frames = len(CHARS) + 1
    sheet = Image.new("RGBA", (frames * CELL, CELL), (0, 0, 0, 0))
    for name, i in CHARS:
        sheet.paste(face(name), (i * CELL, 0))
    sheet.paste(cursor_cell(), (CURSOR_FRAME * CELL, 0), cursor_cell())
    out = os.path.join(OUT, "faces32.png")
    sheet.save(out)
    print(f"faces32.png: {sheet.size} ({len(CHARS)} portraits + cursor@{CURSOR_FRAME}) -> {out}")


if __name__ == "__main__":
    build()
