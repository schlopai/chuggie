#!/usr/bin/env python3
"""Art for examples/cutscene-raw: two actors on one sheet, and a room backdrop.

ONE actor sheet, two characters in it — two imported sheets would claim two of the GBA's sixteen
sprite palette banks for a scene with two people in it.

    python3 scripts/gen_cutscene_raw.py
"""
import pathlib
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/cutscene-raw/assets"
T = 16

# frame = actor * 4 + facing, facing 0 down / 1 up / 2 left / 3 right — the layout cutSetAnim below
# maps onto. Both actors come from the pack's standard 16x16 walk sheets.
ACTORS = ["Actor/Character/NinjaBlue/SpriteSheet.png", "Actor/Character/NinjaRed/SpriteSheet.png"]


def quantise(img, limit=15):
    a = img.getchannel("A").point(lambda v: 255 if v > 127 else 0)
    flat = Image.new("RGB", img.size, (0, 0, 0))
    flat.paste(img.convert("RGB"), (0, 0), a)
    out = flat.quantize(colors=limit, dither=Image.NONE).convert("RGBA")
    out.putalpha(a)
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    strip = Image.new("RGBA", (T * 8, T), (0, 0, 0, 0))
    for ai, rel in enumerate(ACTORS):
        src = Image.open(NA / rel).convert("RGBA")
        for face in range(4):
            strip.paste(src.crop((0, face * T, T, face * T + T)), ((ai * 4 + face) * T, 0))
    strip = quantise(strip)
    strip.save(OUT / "actors16.png")
    n = len({p for p in strip.getdata() if p[3] > 0})
    print(f"actors16.png  8 frames ({len(ACTORS)} actors x 4 facings), {n} colours")

    # A 256x256 room: floor with a wall band, wide enough that a camera pan has somewhere to go.
    floor = Image.open(NA / "Backgrounds/Tilesets/TilesetFloor.png").convert("RGBA")
    tile = floor.crop((2 * T, 1 * T, 3 * T, 2 * T))
    wall = Image.open(NA / "Backgrounds/Tilesets/TilesetDungeon.png").convert("RGBA").crop(
        (5 * T, 1 * T, 6 * T, 2 * T))
    room = Image.new("RGBA", (256, 256), (0, 0, 0, 255))
    for r in range(16):
        for c in range(16):
            room.paste(wall if r < 2 else tile, (c * T, r * T))
    room = quantise(room)
    room.save(OUT / "room.png")
    print(f"room.png      256x256, {len(set(room.getdata()))} colours")


if __name__ == "__main__":
    main()
