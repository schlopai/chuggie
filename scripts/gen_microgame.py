#!/usr/bin/env python3
"""Build examples/microgame's two sprite sheets from the vendored Ninja Adventure pack.

⚠️ TWO SHEETS, NOT SEVEN. The GBA has sixteen sprite palette banks for the whole machine and each
imported sheet claims one, so "a sheet per microgame" is how a twenty-microgame cartridge crashes
inside agb the moment two of them are warm at once. Everything a microgame can show is one 16x16
prop on ONE sheet, plus one actor sheet — two banks, forever, however many microgames get added.

That constraint is the reason the props are chosen by ROLE rather than by game: a microgame asks for
PROP_FOOD or PROP_HAZARD, and adding a microgame adds no art.

    python3 scripts/gen_microgame.py
"""

import pathlib
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/microgame/assets"

CELL = 16

# Frame order IS the contract with src/props.tish — a prop's index is its frame. Grouped by role so
# a microgame can pick "any food" as a contiguous range without knowing which food.
PROPS = [
    # 0-5 food: the CATCH target
    "Items/Food/Meat.png", "Items/Food/Fish.png", "Items/Food/Noodle.png",
    "Items/Food/Sushi.png", "Items/Food/Honey.png", "Items/Food/Nut.png",
    # 6-9 hazards: the DODGE threat
    "Items/Projectile/Shuriken.png", "Items/Projectile/Kunai.png",
    "Items/Projectile/Bomb.png", "Items/Projectile/Caltrop.png",
    # 10-13 treasure: the GRAB target
    "Items/Treasure/GoldCoin.png", "Items/Treasure/SilverCoin.png",
    "Items/Resource/GemRed.png", "Items/Resource/GemGreen.png",
    # 14-15 chrome: life pip and timer
    "Items/Potion/Heart.png", "Items/Object/Hourglass.png",
]

# One actor, four facings x three walk frames, from the pack's standard 16x16 sheet layout.
ACTOR = "Actor/Character/NinjaBlue/SpriteSheet.png"


def cell(img: Image.Image, w: int = CELL, h: int = CELL) -> Image.Image:
    """Centre an irregular source in a CELLxCELL transparent cell. The pack's items are 13x13 to
    16x16 and a sheet's frames must be uniform, so anything else silently shears every later frame."""
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    img = img.convert("RGBA")
    if img.width > w or img.height > h:
        img.thumbnail((w, h), Image.NEAREST)
    out.paste(img, ((w - img.width) // 2, (h - img.height) // 2), img)
    return out


def quantise(img: Image.Image, limit: int = 15) -> Image.Image:
    """Squeeze the opaque pixels down to `limit` colours, keeping alpha binary.

    Quantised ACROSS THE WHOLE SHEET, not per frame: the sheet gets one 16-colour bank on the
    hardware, so quantising a frame at a time produces frames that each look right alone and clash
    the moment two are on screen. Same rule the pack's portraits follow (quantised together per
    character, because there are only sixteen banks)."""
    rgba = img.convert("RGBA")
    alpha = rgba.getchannel("A").point(lambda a: 255 if a > 127 else 0)
    flat = Image.new("RGB", rgba.size, (0, 0, 0))
    flat.paste(rgba.convert("RGB"), (0, 0), alpha)
    q = flat.quantize(colors=limit, method=Image.MEDIANCUT, dither=Image.NONE).convert("RGB")
    out = q.convert("RGBA")
    out.putalpha(alpha)
    return out


def check_palette(img: Image.Image, name: str, limit: int = 15) -> int:
    """A 4bpp GBA sheet has 15 colours plus transparent. Over that, agb quantises silently and the
    art comes back wrong rather than failing, so count here where it can be an error."""
    cols = {p for p in img.convert("RGBA").getdata() if p[3] > 0}
    if len(cols) > limit:
        raise SystemExit(
            f"{name}: {len(cols)} opaque colours, limit {limit}. "
            "Drop a prop or quantise — a 4bpp sheet cannot hold this and agb will not tell you."
        )
    return len(cols)


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    strip = Image.new("RGBA", (CELL * len(PROPS), CELL), (0, 0, 0, 0))
    for i, rel in enumerate(PROPS):
        p = NA / rel
        if not p.exists():
            raise SystemExit(f"missing catalogued asset: {p}")
        strip.paste(cell(Image.open(p)), (i * CELL, 0))
    strip = quantise(strip)
    n = check_palette(strip, "props16.png")
    strip.save(OUT / "props16.png")
    print(f"props16.png  {len(PROPS)} frames, {n} colours")

    src = Image.open(NA / ACTOR).convert("RGBA")
    # The pack's walk sheet is 4 rows (down/up/left/right) x 3 frames at 16x16. Flatten to one strip
    # so the sheet is a plain `sheet:` import with frame = dir*3 + step.
    fw = fh = CELL
    cols, rows = src.width // fw, min(4, src.height // fh)
    actor = Image.new("RGBA", (fw * rows * 3, fh), (0, 0, 0, 0))
    for r in range(rows):
        for c in range(min(3, cols)):
            actor.paste(src.crop((c * fw, r * fh, c * fw + fw, r * fh + fh)), ((r * 3 + c) * fw, 0))
    actor = quantise(actor)
    n = check_palette(actor, "hero16.png")
    actor.save(OUT / "hero16.png")
    print(f"hero16.png   {rows * 3} frames, {n} colours")


if __name__ == "__main__":
    main()
