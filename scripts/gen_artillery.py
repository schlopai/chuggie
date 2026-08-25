#!/usr/bin/env python3
"""Build examples/artillery's two sprite sheets.

CATALOG FIRST, DRAWN ONLY WHERE THE CATALOG GENUINELY HAS NOTHING — the gen_golf_art.py rule.

The vendored Ninja Adventure pack supplies the shell (EnergyBall), the blast (the Explosion sheet),
the trail spark and the aim arrow. It supplies NO planets: I searched the catalog index and the
asset-search MCP for space/planet/star/sci-fi and the only hits are a "Moon" SPELL ICON, a KeySpace
keyboard prompt, and floor sparkles tagged `star`. So the planets are drawn discs here, and that is
the whole extent of the drawing.

⚠️ EACH SHEET CLAIMS ONE OF THE GBA'S SIXTEEN SPRITE PALETTE BANKS, and must quantise to at most 15
colours AS A WHOLE SHEET rather than per frame. Two sheets here, so two banks.

    python3 scripts/gen_artillery.py
"""

import pathlib
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/artillery/assets"

# ── Frame order IS the contract with src/main.tish. ──────────────────────────────────────────────
# wh8.png, 8x8 cells:
F_SHELL, F_TRAIL, F_GHOST, F_SEG_OFF, F_SEG_ON, F_SEG_HOT, F_TURRET0, F_TURRET1 = range(8)
CELL8 = 8
N8 = 8

# planets32.png, 32x32 cells. Three classes; the radii are the contract with PL_R in main.tish.
PLANET_R = [10, 13, 15]
CELL32 = 32

# Planet palettes. Deliberately few colours: the whole 32x32 sheet shares one 15-colour bank with
# the blast frames, so three planets at four shades each already spends twelve of them.
PLANET_COLS = [
    ((92, 148, 208), (56, 104, 160), (168, 208, 240)),    # ice blue
    ((196, 132, 72), (140, 88, 44), (232, 184, 128)),     # rust
    ((128, 176, 104), (84, 124, 68), (176, 216, 152)),    # moss
]


def load(rel):
    return Image.open(NA / rel).convert("RGBA")


def cell(img, x, y, w, h):
    return img.crop((x, y, x + w, y + h))


def fit(img, w, h):
    """Centre `img` in a w*h transparent cell, scaling down only if it does not fit."""
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    img = img.copy()
    if img.width > w or img.height > h:
        img.thumbnail((w, h), Image.NEAREST)
    out.paste(img, ((w - img.width) // 2, (h - img.height) // 2), img)
    return out


def disc(size, r, cols):
    """A planet: a lit disc with a terminator and a rim. Drawn, because the pack has no planets."""
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = size // 2
    base, dark, lit = cols
    d.ellipse([c - r, c - r, c + r - 1, c + r - 1], fill=base + (255,))
    # Night side: a second disc offset down-right, clipped to the planet by drawing it smaller.
    d.chord([c - r, c - r, c + r - 1, c + r - 1], 20, 200, fill=dark + (255,))
    # Highlight up-left, the light source every other sprite in the pack assumes.
    hr = max(2, r // 3)
    d.ellipse([c - r + 2, c - r + 2, c - r + 2 + hr, c - r + 2 + hr], fill=lit + (255,))
    # Rim, so a planet reads as a disc and not a blob against the black backdrop.
    d.ellipse([c - r, c - r, c + r - 1, c + r - 1], outline=lit + (255,))
    return im


def quantise(im, n=15):
    """Flatten to at most `n` opaque colours plus transparency.

    The GBA gives a 4bpp sprite sheet sixteen palette entries and entry 0 is the transparent one, so
    fifteen is the real budget — for the WHOLE sheet, which is why this runs once at the end rather
    than per frame.
    """
    alpha = im.getchannel("A").point(lambda v: 255 if v > 128 else 0)
    rgb = im.convert("RGB").quantize(colors=n, method=Image.MEDIANCUT, dither=Image.NONE)
    out = rgb.convert("RGBA")
    out.putalpha(alpha)
    return out


def build8():
    sheet = Image.new("RGBA", (CELL8 * N8, CELL8), (0, 0, 0, 0))

    # Shell: the pack's EnergyBall, 4 frames of 16x16 in a 64x16 strip. Frame 1 is the fully-formed
    # ball; frame 0 is a spawn wisp and reads as nothing at 8px.
    ball = cell(load("FX/Projectile/EnergyBall.png"), 16, 0, 16, 16)
    sheet.paste(fit(ball, CELL8, CELL8), (F_SHELL * CELL8, 0))

    # ⚠️ THE TRAIL DOT IS DRAWN, NOT TAKEN FROM THE PACK — and this is the one place the catalog
    # actively made things worse. The obvious candidate is FX/Particle/Spark.png, but a 14x8 spark
    # scaled into an 8x8 cell and quantised to a shared 15-colour bank comes out as a one-pixel
    # speck of an indeterminate colour, and a hundred of them along an arc read as screen dirt
    # rather than as a line. A trail dot is a UI primitive, not a particle: it wants to be uniform,
    # legible against black, and identical every time. Three pixels of deliberate shape beat a
    # downsampled illustration here.
    trail = Image.new("RGBA", (CELL8, CELL8), (0, 0, 0, 0))
    d = ImageDraw.Draw(trail)
    d.rectangle([3, 2, 4, 5], fill=(196, 232, 255, 255))   # a 2x4 + 4x2 plus-shape core...
    d.rectangle([2, 3, 5, 4], fill=(196, 232, 255, 255))
    d.point([(3, 3), (4, 3), (3, 4), (4, 4)], fill=(255, 255, 255, 255))  # ...with a hot centre
    sheet.paste(trail, (F_TRAIL * CELL8, 0))

    # The preview dot is the same shape one step down in size and brightness: a proposal, not a fact.
    ghost = Image.new("RGBA", (CELL8, CELL8), (0, 0, 0, 0))
    d = ImageDraw.Draw(ghost)
    d.rectangle([3, 3, 4, 4], fill=(96, 128, 160, 255))
    sheet.paste(ghost, (F_GHOST * CELL8, 0))

    # Power segments. Drawn: a meter segment is three rectangles and the pack's UI is fantasy wood,
    # which reads as a plank next to a gun.
    for idx, col in ((F_SEG_OFF, (48, 56, 72)), (F_SEG_ON, (96, 200, 120)), (F_SEG_HOT, (232, 96, 72))):
        seg = Image.new("RGBA", (CELL8, CELL8), (0, 0, 0, 0))
        ImageDraw.Draw(seg).rectangle([1, 2, 6, 5], fill=col + (255,))
        sheet.paste(seg, (idx * CELL8, 0))

    # Turrets. Drawn discs with a muzzle nub; two colours so the two sides are told apart at a
    # glance, which is the entire visual job a turret has in this spike.
    for idx, col in ((F_TURRET0, (216, 216, 232)), (F_TURRET1, (240, 176, 96))):
        t = Image.new("RGBA", (CELL8, CELL8), (0, 0, 0, 0))
        d = ImageDraw.Draw(t)
        d.ellipse([1, 1, 6, 6], fill=col + (255,), outline=(24, 24, 32, 255))
        sheet.paste(t, (idx * CELL8, 0))

    quantise(sheet).save(OUT / "wh8.png")
    return CELL8 * N8, CELL8


def build32():
    n = len(PLANET_R) + 4                       # three planets + four blast frames
    sheet = Image.new("RGBA", (CELL32 * n, CELL32), (0, 0, 0, 0))
    for i, r in enumerate(PLANET_R):
        sheet.paste(disc(CELL32, r, PLANET_COLS[i]), (i * CELL32, 0))

    # Blast: the pack's Explosion sheet is 360x40, nine 40x40 frames. Take four, evenly spaced, so
    # the animation reads as a full bloom-and-fade in the frames a turn can spare.
    boom = load("FX/Elemental/Explosion/SpriteSheet.png")
    for j, src in enumerate((1, 3, 5, 7)):
        f = cell(boom, src * 40, 0, 40, 40)
        sheet.paste(fit(f, CELL32, CELL32), ((len(PLANET_R) + j) * CELL32, 0))

    quantise(sheet).save(OUT / "planets32.png")
    return CELL32 * n, CELL32


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    w8, h8 = build8()
    w32, h32 = build32()
    print(f"wrote {(OUT / 'wh8.png').relative_to(ROOT)}        {w8}x{h8}  ({N8} frames, 8x8)")
    print(f"wrote {(OUT / 'planets32.png').relative_to(ROOT)}  {w32}x{h32}  "
          f"({len(PLANET_R)} planets + 4 blast, 32x32)")
    print(f"  planet radii {PLANET_R} — these ARE PL_R in src/main.tish")
    print("  2 sheets = 2 of the GBA's 16 sprite palette banks, each quantised to 15 colours")


if __name__ == "__main__":
    main()
