#!/usr/bin/env python3
"""Art for examples/visual-novel: three portraits on one sheet, and two backdrops.

Portraits come from the vendored pack's `Faceset.png` files (38x38, one per actor). They are
cropped to 32x32 and packed into ONE sheet — three imported sheets would claim three of the GBA's
sixteen sprite palette banks to show three faces.

    python3 scripts/gen_visualnovel.py
"""
import pathlib
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/visual-novel/assets"
CELL = 32

# frame = speaker id. The order IS the contract with src/main.tish's SPK_* constants.
FACES = [
    "Actor/Character/NinjaBlue/Faceset.png",
    "Actor/Character/NinjaRed/Faceset.png",
    "Actor/Character/Cavegirl/Faceset.png",
]

# ⚠️ ONE ROOM, AND THAT IS A HARDWARE FINDING, NOT A LACK OF AMBITION.
#
# A visual novel obviously wants to change location, and on this engine it cannot — not while a text
# canvas is on screen. Every arrangement was tried and every one ends with the dialogue box drawing
# its words as orange blocks of the room's own floor tile:
#
#   * two `background:` imports + `bg_clear`/rebuild — `bg_clear` takes the UI canvas with it, and
#     the rebuilt canvas loses its palette entries to the room;
#   * both rooms alive, toggling `bg_set_visible` — fixes the palette only if you also drop
#     `bg_use_palettes`, and then the glyph TILES collide with the second room's instead;
#   * ONE 512x256 background holding both rooms side by side, scrolled 256px to change scene — a
#     wide map makes tiles resident as it scrolls, and the newly resident room tiles land on top of
#     the canvas's glyphs;
#   * a smaller `ui_reserve_tiles` — not a reserve-size problem; the canvas peaks at 93 tiles of
#     2,880 and reports itself perfectly healthy while the screen shows no text at all.
#
# The canvas is fine in every one of those. What is not fine is changing a background's tile
# residency underneath it. So: one static backdrop, never swapped and never scrolled.
ROOMS = [
    ("Backgrounds/Tilesets/TilesetFloor.png", 2, 1),
]


def quantise(img, limit=15):
    a = img.getchannel("A").point(lambda v: 255 if v > 127 else 0)
    flat = Image.new("RGB", img.size, (0, 0, 0))
    flat.paste(img.convert("RGB"), (0, 0), a)
    out = flat.quantize(colors=limit, dither=Image.NONE).convert("RGBA")
    out.putalpha(a)
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    strip = Image.new("RGBA", (CELL * len(FACES), CELL), (0, 0, 0, 0))
    for i, rel in enumerate(FACES):
        src = Image.open(NA / rel).convert("RGBA")
        # 38x38 -> centre 32x32, so the crop keeps the face rather than the frame border.
        o = (src.width - CELL) // 2
        strip.paste(src.crop((o, o, o + CELL, o + CELL)), (i * CELL, 0))
    strip = quantise(strip)
    strip.save(OUT / "faces32.png")
    print(f"faces32.png  {len(FACES)} portraits, {len({p for p in strip.getdata() if p[3] > 0})} colours")

    rel, c, r = ROOMS[0]
    src = Image.open(NA / rel).convert("RGBA")
    tile = src.crop((c * 16, r * 16, c * 16 + 16, r * 16 + 16))
    bg = Image.new("RGBA", (256, 256), (0, 0, 0, 255))
    for rr in range(16):
        for cc in range(16):
            bg.paste(tile, (cc * 16, rr * 16))
    bg = quantise(bg)
    bg.save(OUT / "room.png")
    print(f"room.png     {bg.width}x{bg.height}, {len(set(bg.getdata()))} colours")


if __name__ == "__main__":
    main()
