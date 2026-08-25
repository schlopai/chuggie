#!/usr/bin/env python3
"""Generate the sprite sheet for examples/shop-demo.

Everything is sourced from the vendored Ninja Adventure catalog (assets/ninja-adventure/):
the ware ICONS are the framed 24x24 "Skill Icon" set (centred in 32x32 cells), and the SHOPKEEPER
PORTRAIT is a character Faceset (38x38 -> 32). All are packed left-to-right into ONE `sheet32:` strip
(the UI icon pool + dialog portrait share a single sheet), quantized to <=15 colours per cell so agb
can bake a 16-colour sprite palette per frame.

Frame order (must match src/main.tish):
  0 Potion  1 Tonic(Scroll)  2 Kunai  3 Shuriken  4 Armor  5 Helmet  6 Ring  7 Amulet  8 KEEPER

Output:  examples/shop-demo/assets/shop32.png
Run:     python3 scripts/gen_shop_demo.py
"""
import os
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
OUT = os.path.join(ROOT, "examples", "shop-demo", "assets")
os.makedirs(OUT, exist_ok=True)

CELL = 32
IW = os.path.join(PACK, "Ui", "Skill Icon", "Items & Weapon")
JA = os.path.join(PACK, "Ui", "Skill Icon", "Job & Action")

# (frame, catalog icon path) — ware icons, then the keeper portrait is appended last.
ICONS = [
    os.path.join(JA, "Potion.png"),      # 0
    os.path.join(IW, "Scroll.png"),      # 1
    os.path.join(IW, "Kunai.png"),       # 2
    os.path.join(IW, "Shuriken.png"),    # 3
    os.path.join(IW, "Armor.png"),       # 4
    os.path.join(IW, "Helmet.png"),      # 5
    os.path.join(IW, "Ring.png"),        # 6
    os.path.join(IW, "Amulet.png"),      # 7
]
KEEPER = os.path.join(PACK, "Actor", "Character", "Villager6", "Faceset.png")  # frame 8
KEEPER_FRAME = len(ICONS)      # 8
CURSOR_FRAME = len(ICONS) + 1  # 9 — the ► selection cursor (drawn, not catalog)
# Extra frames for the SRPG clan Equip/Item List (appended so shop-demo 0–9 stay stable).
# Frame 11 uses DefenseUpgrade (shield art); Guard.png is crossed swords.
EXTRA = [
    os.path.join(IW, "Boot.png"),   # 10 shoes
    # Guard.png is crossed swords; DefenseUpgrade is an actual shield.
    os.path.join(PACK, "Ui", "Skill Icon", "Spell", "DefenseUpgrade.png"),  # 11 shield
]
FRAMES = len(ICONS) + 2 + len(EXTRA)


def quantize_cell(rgba, max_colors=15):
    alpha = rgba.split()[3]
    q = rgba.convert("RGB").quantize(colors=max_colors, method=Image.MEDIANCUT, dither=Image.NONE).convert("RGBA")
    q.putalpha(alpha.point(lambda a: 255 if a >= 128 else 0))
    px = q.load()
    for y in range(q.height):
        for x in range(q.width):
            if px[x, y][3] == 0:
                px[x, y] = (0, 0, 0, 0)
    return q


def build():
    sheet = Image.new("RGBA", (FRAMES * CELL, CELL), (0, 0, 0, 0))
    for i, path in enumerate(ICONS):
        im = quantize_cell(Image.open(path).convert("RGBA"), 15)
        ox = i * CELL + (CELL - im.width) // 2
        oy = (CELL - im.height) // 2
        sheet.paste(im, (ox, oy), im)
    # Keeper portrait: 38x38 faceset -> 32x32, quantized.
    face = Image.open(KEEPER).convert("RGBA").resize((CELL, CELL), Image.LANCZOS)
    face = quantize_cell(face, 15)
    sheet.paste(face, (KEEPER_FRAME * CELL, 0))
    # Cursor: a SMALL, softly-rounded yellow ► in the top-left of the 32px cell (rest transparent).
    # Optical centre ~y=6 (oy=3 + half of 7px triangle). packages/ui `pointerAtRow` centres that hot
    # spot on the text row (font7 ≈ 8px, etc.) — do not assume a fixed 12px row anymore.
    cur = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    d = ImageDraw.Draw(cur)
    yl = (0xFF, 0xE0, 0x6A, 255)
    oy = 3
    d.polygon([(1, 0 + oy), (7, 3 + oy), (1, 6 + oy)], fill=yl)   # small right triangle
    # round it: drop the three sharp corner pixels + soften the tip
    for (rx, ry) in [(1, 0 + oy), (1, 6 + oy), (7, 3 + oy)]:
        cur.putpixel((rx, ry), (0, 0, 0, 0))
    cur.putpixel((6, 3 + oy), yl)   # keep a 1px nose so it still reads as a point
    sheet.paste(cur, (CURSOR_FRAME * CELL, 0), cur)
    # Clan extras (Boot / Guard) after cursor.
    for j, path in enumerate(EXTRA):
        im = quantize_cell(Image.open(path).convert("RGBA"), 15)
        fi = CURSOR_FRAME + 1 + j
        ox = fi * CELL + (CELL - im.width) // 2
        oy = (CELL - im.height) // 2
        sheet.paste(im, (ox, oy), im)
    out = os.path.join(OUT, "shop32.png")
    sheet.save(out)
    print(f"shop32.png: {sheet.size} ({FRAMES} frames, keeper={KEEPER_FRAME}, cursor={CURSOR_FRAME}, boot/guard=10/11) -> {out}")
    # Keep iso-town in sync when regenerating.
    town = os.path.join(ROOT, "examples", "iso-town", "assets", "shop32.png")
    if os.path.isdir(os.path.dirname(town)):
        sheet.save(town)
        print(f"  also -> {town}")


if __name__ == "__main__":
    build()
