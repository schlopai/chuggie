#!/usr/bin/env python3
"""Build examples/golf's one sprite sheet from the vendored Ninja Adventure pack.

ONE sheet, three frames: ball, cup, aim arrow. Each imported sheet claims one of the GBA's sixteen
sprite palette banks, and golf needs exactly one.

    python3 scripts/gen_golf_art.py
"""

import pathlib
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/golf/assets"
CELL = 8

# Frame order IS the contract with src/main.tish.
F_BALL, F_CUP, F_ARROW = 0, 1, 2


def fit(img, w=CELL, h=CELL):
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    img = img.convert("RGBA")
    img.thumbnail((w, h), Image.NEAREST)
    out.paste(img, ((w - img.width) // 2, (h - img.height) // 2), img)
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    strip = Image.new("RGBA", (CELL * 3, CELL), (0, 0, 0, 0))

    # The ball: a plain white disc. The pack has no golf ball and nothing in it reads as one at 8px
    # — a seed or a nut at this size is a brown smudge, which is worse than three drawn circles.
    ball = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    d = ImageDraw.Draw(ball)
    d.ellipse([0, 0, CELL - 1, CELL - 1], fill=(248, 248, 248, 255), outline=(160, 160, 168, 255))
    strip.paste(ball, (F_BALL * CELL, 0))

    # The cup: a dark hole with a rim, same reasoning.
    cup = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    d = ImageDraw.Draw(cup)
    d.ellipse([0, 1, CELL - 1, CELL - 2], fill=(24, 24, 32, 255), outline=(90, 70, 40, 255))
    strip.paste(cup, (F_CUP * CELL, 0))

    # The arrow IS catalogued, so it comes from the pack rather than being drawn.
    strip.paste(fit(Image.open(NA / "Ui/Arrow.png")), (F_ARROW * CELL, 0))

    cols = {p for p in strip.getdata() if p[3] > 0}
    if len(cols) > 15:
        # Quantise across the WHOLE sheet — it gets one 16-colour bank on the hardware, so per-frame
        # quantisation gives frames that clash the moment two are on screen.
        alpha = strip.getchannel("A").point(lambda a: 255 if a > 127 else 0)
        flat = Image.new("RGB", strip.size, (0, 0, 0))
        flat.paste(strip.convert("RGB"), (0, 0), alpha)
        strip = flat.quantize(colors=15, dither=Image.NONE).convert("RGBA")
        strip.putalpha(alpha)
        cols = {p for p in strip.getdata() if p[3] > 0}

    strip.save(OUT / "golf8.png")
    print(f"golf8.png  3 frames, {len(cols)} colours")


if __name__ == "__main__":
    main()
