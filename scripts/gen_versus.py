#!/usr/bin/env python3
"""Build the `versus` example's ROM assets from the vendored CC0 fighting-game art.

Source art (all CC0) lives OUTSIDE the repo, in ~/Downloads/versus-art — see
examples/versus/assets/ATTRIBUTION.md for what to download and from where.

  <char>.png   sheet64 — 35 cells: 24 body poses, 10 attack FX overlays, 1 select portrait
  stage.png    background — ONE opaque 256x256 image; the depth comes from bg_parallax, not layers
  spark.png    sheet32 — procedural hit sparks / block sparks / landing dust (shared, 1 palette)
  digits.png   sheet16 — the round clock and the combo counter, which must not be text

The character baking itself lives in scripts/fighter_art.py, shared with examples/beatemup: both
games use the same packs and the same fixed 24-pose layout, and the traps in that pipeline are ones
that look fine in every preview and only fail on hardware.
"""
import os

from PIL import Image

import sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fighter_art import (REPO, ART, CELL, FEET, FX_FOR, PORTRAIT, POSES,
                         build_char, clamp_colors, digits_sheet, fx_side_tish, harden_alpha)

EX = os.path.join(REPO, "examples", "versus")
ASSETS = os.path.join(EX, "assets")
SRC = os.path.join(EX, "src")

TARGET_H = 54      # idle head-to-toe, in pixels, after scaling. Two of these + HUD fit 160 rows.
FLOOR_Y = 132      # where the stage's ground line lands, for the example's own use

CHARS = [("hero", "HERO"), ("hero2", "SHINOBI"), ("hero3", "RONIN"), ("warrior", "WARRIOR")]


# ── stage ────────────────────────────────────────────────────────────────────────────────────────
# ONE opaque 256x256 image. Depth comes from bg_bands (per-scanline DMA), not from extra layers —
# see examples/bands-demo. A fighting stage is horizontally stratified, which is exactly the shape
# banding handles, and it sidesteps the `background:` importer's no-alpha rule entirely.
MD = "mountain-dusk/Super Mountain Dusk Files/Assets/version A/Layers"
GROUND_TOP = FLOOR_Y - 4       # where the dirt starts inside the 256x256 image
DIRT = (58, 40, 52)
DIRT2 = (74, 52, 62)
DIRT3 = (44, 30, 42)


def build_stage():
    canvas = Image.new("RGBA", (320, 240), (0, 0, 0, 255))
    for n in ("sky", "far-clouds", "far-mountains", "mountains", "near-clouds", "trees"):
        im = Image.open(os.path.join(ART, MD, n + ".png")).convert("RGBA")
        tiled = Image.new("RGBA", (320, 240), (0, 0, 0, 0))
        for x in range(0, 320, im.width):
            tiled.paste(im, (x, 0), im)
        canvas = Image.alpha_composite(canvas, tiled)

    # 320x240 -> 256 wide keeps the horizontal seam (every source layer loops at its own width from
    # x=0, so a uniform resize keeps them all seamless) and 192 tall crops to the 160 we show.
    art = canvas.convert("RGB").resize((256, 192), Image.LANCZOS)
    stage = Image.new("RGB", (256, 256), DIRT)
    stage.paste(art.crop((0, 0, 256, GROUND_TOP)), (0, 0))

    # A flat arena floor the fighters can stand on — the pack's treeline runs all the way down.
    for y in range(GROUND_TOP, 256):
        for x in range(256):
            if y < GROUND_TOP + 2:
                c = DIRT2
            elif (x + y * 3) % 23 == 0:
                c = DIRT3
            elif (x * 5 + y) % 31 == 0:
                c = DIRT2
            else:
                c = DIRT
            stage.putpixel((x, y), c)

    q = stage.quantize(colors=15, method=Image.MEDIANCUT, dither=Image.NONE).convert("RGB")
    q.save(os.path.join(ASSETS, "stage.png"))
    print(f"  stage    256x256  ground row {GROUND_TOP}  15 colours")


# ── digits ───────────────────────────────────────────────────────────────────────────────────────
# The round clock and the combo counter change while the fight is running, and re-laying-out a
# `text_draw` string costs enough to blow the frame — which on this stage is not a dropped frame but
# a visibly shredded backdrop, because the band DMA re-arms mid-scan (see packages/fighter.tish).
# Ten pre-rendered cells turn "the clock ticked" into two `sprite_set_frame` calls.



# ── shared FX: hit sparks, block sparks, landing dust ────────────────────────────────────────────
SPARK = [(255, 244, 214), (255, 196, 92), (255, 128, 64)]
BLOCK = [(220, 244, 255), (140, 200, 255), (80, 140, 220)]
DUST = [(200, 190, 180), (150, 140, 132), (110, 100, 96)]


def ring(dr, cols, r0, r1, spokes, seed):
    """A radial burst in a 32x32 cell — the shape a hit reads as at GBA scale."""
    for i in range(spokes):
        a = (i * 2861 + seed * 97) % 360
        import math
        ca, sa = math.cos(math.radians(a)), math.sin(math.radians(a))
        for t in range(r0, r1):
            x, y = 16 + int(ca * t), 16 + int(sa * t)
            if 0 <= x < 32 and 0 <= y < 32:
                c = cols[min(len(cols) - 1, (t - r0) * len(cols) // max(1, r1 - r0))]
                dr.putpixel((x, y), c + (255,))
                if t < r1 - 2:
                    for ox, oy in ((1, 0), (0, 1)):
                        if 0 <= x + ox < 32 and 0 <= y + oy < 32:
                            dr.putpixel((x + ox, y + oy), c + (255,))


def build_spark():
    cells = []
    for i, cols in ((0, SPARK), (1, BLOCK), (2, DUST)):
        for step in range(4):
            c = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
            ring(c, cols, 2 + step * 3, 6 + step * 4, 8 if i < 2 else 6, i * 7 + step)
            cells.append(c)
    strip = Image.new("RGBA", (32 * len(cells), 32), (0, 0, 0, 0))
    for i, c in enumerate(cells):
        strip.paste(c, (i * 32, 0))
    clamp_colors(strip, 15).save(os.path.join(ASSETS, "spark.png"))
    print(f"  spark    {len(cells)} cells (hit x4, block x4, dust x4)")


# ── generated tish ───────────────────────────────────────────────────────────────────────────────
def write_frames(fx):
    lines = [
        "// GENERATED by scripts/gen_versus.py — do not edit.",
        "//",
        "// Every character sheet has the SAME 35-cell layout, which is what lets one frame-data",
        "// table in packages/fighter.tish drive all four. Cells 0..23 are body poses, 24..33 are",
        "// the attack FX overlays for body poses 11..20, cell 34 is the select-screen portrait.",
        "",
        "// Body poses.",
        "export const F_IDLE = 0",
        "export const F_IDLE_LEN = 4",
        "export const F_WALK = 4",
        "export const F_WALK_LEN = 4",
        "export const F_JUMP = 8",
        "export const F_FALL = 9",
        "export const F_CROUCH = 10",
        "export const F_CR_ATK = 11",
        "export const F_ATK1 = 12",
        "export const F_ATK2 = 15",
        "export const F_ATK3 = 18",
        "export const F_ATK_LEN = 3",
        "export const F_BLOCK = 21",
        "export const F_HIT = 22",
        "export const F_KO = 23",
        "export const F_FX = 24",
        "export const F_FX_FIRST = 11",
        "export const F_PORTRAIT = 34",
        "",
        "// Which neighbouring 64x64 window each attack pose's overlay was cut from, in CELL units,",
        "// indexed [char * 10 + (pose - 11)]: dx +1 is in front, -1 behind, dy -1 is above. (0, 0)",
        "// means the pose needs no overlay at all. Authored facing right and stored unmirrored — the",
        "// draw code negates dx and flips the cell together when the fighter faces left.",
    ]
    dxl, dyl = fx_side_tish(fx, [n for n, _ in CHARS])
    lines.append(dxl)
    lines.append(dyl)
    lines.append("")
    lines.append("// Character labels, in sheet order.")
    for name, label in CHARS:
        lines.append('export const NAME_%s = "%s"' % (name.upper(), label))
    lines.append("")
    lines.append(f"// The stage's ground line, in screen rows (scripts/gen_versus.py paints it).")
    lines.append(f"export const FLOOR_Y = {FLOOR_Y}")
    lines.append(f"export const CELL = {CELL}")
    lines.append(f"export const FEET = {FEET}")
    with open(os.path.join(SRC, "frames.tish"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("  frames.tish  written")


def main():
    os.makedirs(ASSETS, exist_ok=True)
    os.makedirs(SRC, exist_ok=True)
    print("versus assets:")
    fx = {}
    for name, _ in CHARS:
        fx[name] = build_char(name, TARGET_H, os.path.join(ASSETS, name + ".png"))
    build_stage()
    build_spark()
    digits_sheet(os.path.join(ASSETS, "digits.png"))
    print("  digits   10 cells 16x16")
    write_frames(fx)


if __name__ == "__main__":
    main()
