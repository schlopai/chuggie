#!/usr/bin/env python3
"""Build examples/roguelike's two-tile tileset and its actor sheet from the vendored pack.

TWO TILES is the whole tileset: a floor and a wall. A generated dungeon has no authored decoration
to place, so a larger set would be tiles the generator could never choose. Both are CROPPED from the
vendored art rather than drawn — the tile vocabulary stays authored even though the arrangement is
not (see the header of packages/dungeon.tish).

    python3 scripts/gen_roguelike.py
"""

import pathlib
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/roguelike/assets"
T = 16

# (tileset, column, row) — the gid a cell gets is its index in this list, so the order IS the
# contract with src/main.tish's GID_FLOOR / GID_WALL.
TILES = [
    ("Backgrounds/Tilesets/TilesetFloor.png", 2, 1),   # 0 floor — the patterned centre of
                                                       #   the tan autotile block
    ("Backgrounds/Tilesets/TilesetDungeon.png", 5, 1),         # 1 wall — the brick block
]

ACTOR = "Actor/Character/NinjaDark/SpriteSheet.png"


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    strip = Image.new("RGBA", (T * len(TILES), T), (0, 0, 0, 255))
    for i, (rel, c, r) in enumerate(TILES):
        src = Image.open(NA / rel).convert("RGBA")
        strip.paste(src.crop((c * T, r * T, c * T + T, r * T + T)), (i * T, 0))

    # A background tileset is 4bpp like everything else: 15 colours plus the backdrop. Quantised
    # across BOTH tiles together, because they share one palette bank on the hardware.
    cols = {p for p in strip.convert("RGBA").getdata()}
    if len(cols) > 15:
        strip = strip.convert("RGB").quantize(colors=15, dither=Image.NONE).convert("RGBA")
        cols = {p for p in strip.getdata()}
    strip.save(OUT / "tiles16.png")
    print(f"tiles16.png  {len(TILES)} tiles, {len(cols)} colours")

    src = Image.open(NA / ACTOR).convert("RGBA")
    hero = Image.new("RGBA", (T * 4, T), (0, 0, 0, 0))
    for r in range(min(4, src.height // T)):
        hero.paste(src.crop((0, r * T, T, r * T + T)), (r * T, 0))
    a = hero.getchannel("A").point(lambda v: 255 if v > 127 else 0)
    flat = Image.new("RGB", hero.size, (0, 0, 0))
    flat.paste(hero.convert("RGB"), (0, 0), a)
    hero = flat.quantize(colors=15, dither=Image.NONE).convert("RGBA")
    hero.putalpha(a)
    hero.save(OUT / "hero16.png")
    print(f"hero16.png   4 frames, {len({p for p in hero.getdata() if p[3] > 0})} colours")


if __name__ == "__main__":
    main()
