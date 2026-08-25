#!/usr/bin/env python3
"""Bake UI assets for examples/button-demo.

- prompts.png — gamepad icons as a sheet: (16×16) strip (native art size; no 32-cell pad)
- buttons.png — Theme Wood nine-sliced to 64×32 idle/hover/pressed/disabled

    python3 scripts/gen_button_demo.py
"""
import os
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
OUT = os.path.join(ROOT, "examples", "button-demo", "assets")
os.makedirs(OUT, exist_ok=True)

# GBA object size 64×32 (no 64×16). Art is a ~64×18 plate centered in the cell.
CEL_W, CEL_H = 64, 32
ART_W, ART_H = 64, 18
PROMPT_CEL = 16

# Order matches examples/button-demo/src/frames.tish (0-based).
PROMPT_SOURCES = [
    "Ui/Input/Gamepad/ButtonA/Idle.png",
    "Ui/Input/Gamepad/ButtonB/Idle.png",
    "Ui/Input/Gamepad/ButtonLB.png",
    "Ui/Input/Gamepad/ButtonRB.png",
    "Ui/Input/Gamepad/DPadUp.png",
    "Ui/Input/Gamepad/DPadDown.png",
    "Ui/Input/Gamepad/DPadLeft.png",
    "Ui/Input/Gamepad/DPadRight.png",
    "Ui/Input/Gamepad/DPad.png",
    "Ui/Input/Gamepad/Start.png",
    "Ui/Input/Gamepad/Select.png",
]


def nine_slice(src, w, h, corner=5):
    s = src.width
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    cl = corner

    def region(l, t, r, b):
        return src.crop((l, t, r, b))

    out.paste(region(0, 0, cl, cl), (0, 0))
    out.paste(region(s - cl, 0, s, cl), (w - cl, 0))
    out.paste(region(0, s - cl, cl, s), (0, h - cl))
    out.paste(region(s - cl, s - cl, s, s), (w - cl, h - cl))
    top = region(cl, 0, s - cl, cl).resize((w - 2 * cl, cl), Image.NEAREST)
    bot = region(cl, s - cl, s - cl, s).resize((w - 2 * cl, cl), Image.NEAREST)
    lft = region(0, cl, cl, s - cl).resize((cl, h - 2 * cl), Image.NEAREST)
    rgt = region(s - cl, cl, s, s - cl).resize((cl, h - 2 * cl), Image.NEAREST)
    out.paste(top, (cl, 0))
    out.paste(bot, (cl, h - cl))
    out.paste(lft, (0, cl))
    out.paste(rgt, (w - cl, cl))
    ctr = region(cl, cl, s - cl, s - cl).resize((w - 2 * cl, h - 2 * cl), Image.NEAREST)
    out.paste(ctr, (cl, cl))
    return out


def clamp_colors(img, maxc=15):
    img = img.convert("RGBA")
    opaque = {(r, g, b) for (r, g, b, a) in img.getdata() if a > 0}
    if len(opaque) <= maxc:
        return img
    alpha = img.getchannel("A")
    q = img.convert("RGB").quantize(colors=maxc, method=Image.MEDIANCUT, dither=Image.NONE)
    out = q.convert("RGBA")
    out.putalpha(alpha.point(lambda a: 255 if a >= 128 else 0))
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = px[x, y]
            if a == 0:
                px[x, y] = (0, 0, 0, 0)
    return out


def cell(art):
    c = Image.new("RGBA", (CEL_W, CEL_H), (0, 0, 0, 0))
    c.paste(art, (0, (CEL_H - art.height) // 2), art)
    return c


def darken(im, num=3, den=4):
    out = im.copy()
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = px[x, y]
            if a > 0:
                px[x, y] = (r * num // den, g * num // den, b * num // den, a)
    return out


def fit16(art):
    """Pack source art into a 16×16 cell (scale down only if wider/taller)."""
    a = art.convert("RGBA")
    if a.width > PROMPT_CEL or a.height > PROMPT_CEL:
        scale = min(PROMPT_CEL / a.width, PROMPT_CEL / a.height)
        a = a.resize(
            (max(1, int(a.width * scale)), max(1, int(a.height * scale))),
            Image.NEAREST,
        )
    c = Image.new("RGBA", (PROMPT_CEL, PROMPT_CEL), (0, 0, 0, 0))
    c.paste(a, ((PROMPT_CEL - a.width) // 2, (PROMPT_CEL - a.height) // 2), a)
    return c


def make_prompts():
    n = len(PROMPT_SOURCES)
    strip = Image.new("RGBA", (n * PROMPT_CEL, PROMPT_CEL), (0, 0, 0, 0))
    for i, rel in enumerate(PROMPT_SOURCES):
        src = Image.open(os.path.join(PACK, rel)).convert("RGBA")
        cell16 = fit16(src)
        strip.paste(cell16, (i * PROMPT_CEL, 0), cell16)
    out = clamp_colors(strip)
    path = os.path.join(OUT, "prompts.png")
    out.save(path)
    print(f"prompts.png: {out.size} ({n}×{PROMPT_CEL}x{PROMPT_CEL}) -> {path}")


def make_buttons():
    wood = Image.open(
        os.path.join(PACK, "Ui/Theme/Theme Wood/nine_path_panel.png")
    ).convert("RGBA")
    wood_dis = Image.open(
        os.path.join(PACK, "Ui/Theme/Theme Wood/nine_path_panel_disabled.png")
    ).convert("RGBA")

    idle = nine_slice(wood, ART_W, ART_H)
    hover = nine_slice(wood, ART_W, ART_H)
    d = ImageDraw.Draw(hover)
    # Gold focus ring — selected / hovered affordance (ToffeeCraft-like highlight).
    d.rectangle([3, 2, ART_W - 4, ART_H - 3], outline=(0xFF, 0xE0, 0x6A, 255))
    pressed = darken(nine_slice(wood, ART_W, ART_H), 5, 8)
    disabled = nine_slice(wood_dis, ART_W, ART_H)

    frames = [idle, hover, pressed, disabled]
    strip = Image.new("RGBA", (CEL_W * len(frames), CEL_H), (0, 0, 0, 0))
    for i, art in enumerate(frames):
        strip.paste(cell(art), (i * CEL_W, 0), cell(art))
    out = clamp_colors(strip)
    path = os.path.join(OUT, "buttons.png")
    out.save(path)
    print(f"buttons.png: {out.size} ({len(frames)}×{CEL_W}x{CEL_H}) -> {path}")
    print("  0 idle  1 hover  2 pressed  3 disabled")


def main():
    make_prompts()
    make_buttons()


if __name__ == "__main__":
    main()
