#!/usr/bin/env python3
"""Generate the art for examples/rpg-menu — an inventory / equipment / shop screen.

EVERYTHING here is sourced from the vendored Ninja Adventure catalog
(assets/ninja-adventure/). We only COMPOSITE catalog art — pack the framed
24x24 "Skill Icon" set into a 32x32 sprite sheet, and nine-slice the wood theme
panel into a full-screen UI background. No hand-drawn art except the 1-colour
selection cursor.

Outputs (into examples/rpg-menu/assets/):
  icons32.png  — sheet32: strip, one 32x32 frame per item + a cursor frame
  ui.png       — background: a 240x160 UI with three wood-framed panels

Run:  python3 scripts/gen_rpg_menu.py
"""
import os
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
OUT = os.path.join(ROOT, "examples", "rpg-menu", "assets")
os.makedirs(OUT, exist_ok=True)

CELL = 32  # GBA sprite cell (valid size); catalog icons are 24x24, centred in it.

# The item icons, in FRAME ORDER. Each entry: (frame_index, catalog_path).
# Kept ninja-themed to match the pack. These indices MUST match items.tish.
ICON_DIR_IW = os.path.join(PACK, "Ui", "Skill Icon", "Items & Weapon")
ICON_DIR_JA = os.path.join(PACK, "Ui", "Skill Icon", "Job & Action")
ICONS = [
    os.path.join(ICON_DIR_IW, "Kunai.png"),     # 0  weapon
    os.path.join(ICON_DIR_IW, "Shuriken.png"),  # 1  weapon
    os.path.join(ICON_DIR_IW, "Hook.png"),      # 2  weapon (kusarigama)
    os.path.join(ICON_DIR_IW, "Arrow.png"),     # 3  weapon (bow)
    os.path.join(ICON_DIR_IW, "Armor.png"),     # 4  armor
    os.path.join(ICON_DIR_IW, "Helmet.png"),    # 5  helmet
    os.path.join(ICON_DIR_IW, "Boot.png"),      # 6  accessory
    os.path.join(ICON_DIR_IW, "Ring.png"),      # 7  accessory
    os.path.join(ICON_DIR_IW, "Amulet.png"),    # 8  accessory
    os.path.join(ICON_DIR_IW, "Guard.png"),     # 9  accessory (bracer)
    os.path.join(ICON_DIR_JA, "Potion.png"),    # 10 consumable
    os.path.join(ICON_DIR_IW, "Scroll.png"),    # 11 consumable
    os.path.join(ICON_DIR_IW, "Money.png"),     # 12 gold icon (HUD only)
]
CURSOR_FRAME = len(ICONS)          # 13
EMPTY_FRAME = len(ICONS) + 1       # 14 — greyed "empty slot" marker
FRAMES = len(ICONS) + 2


def quantize_cell(rgba, max_colors=15):
    """Quantize one cell's opaque pixels to <=max_colors so agb can bake a
    single 16-colour sprite palette (index 0 reserved for transparency)."""
    # Split alpha; quantize the RGB of opaque pixels only.
    alpha = rgba.split()[3]
    rgb = rgba.convert("RGB")
    q = rgb.quantize(colors=max_colors, method=Image.MEDIANCUT, dither=Image.NONE)
    out = q.convert("RGBA")
    # Restore transparency where the source alpha was low.
    out.putalpha(alpha.point(lambda a: 255 if a >= 128 else 0))
    # Force fully-transparent pixels to a canonical colour so they collapse.
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = px[x, y]
            if a == 0:
                px[x, y] = (0, 0, 0, 0)
    return out


def build_icons():
    sheet = Image.new("RGBA", (FRAMES * CELL, CELL), (0, 0, 0, 0))
    for i, path in enumerate(ICONS):
        im = Image.open(path).convert("RGBA")
        im = quantize_cell(im, 15)
        ox = i * CELL + (CELL - im.width) // 2
        oy = (CELL - im.height) // 2
        sheet.paste(im, (ox, oy), im)
    # Cursor: a bright yellow hollow selection box (2px border) framing a cell.
    cur = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    d = ImageDraw.Draw(cur)
    y1, y2 = (0xFF, 0xE0, 0x6A, 255), (0xFF, 0xF4, 0xC0, 255)
    for t in range(2):
        d.rectangle([t, t, CELL - 1 - t, CELL - 1 - t], outline=y1)
    # brighter corner ticks
    for (cx, cy) in [(0, 0), (CELL - 3, 0), (0, CELL - 3), (CELL - 3, CELL - 3)]:
        d.rectangle([cx, cy, cx + 2, cy + 2], fill=y2)
    sheet.paste(cur, (CURSOR_FRAME * CELL, 0), cur)
    # Empty-slot marker: a faint dotted inner square.
    emp = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    de = ImageDraw.Draw(emp)
    de.rectangle([9, 9, CELL - 10, CELL - 10], outline=(0x6A, 0x6A, 0x80, 200))
    sheet.paste(emp, (EMPTY_FRAME * CELL, 0), emp)
    out = os.path.join(OUT, "icons32.png")
    sheet.save(out)
    print(f"icons32.png: {sheet.size} ({FRAMES} frames, cursor={CURSOR_FRAME}, empty={EMPTY_FRAME}) -> {out}")


def nine_slice(src, w, h, corner=6):
    """Stretch a 16x16 nine-path source to (w,h)."""
    s = src.width
    inner = s - 2 * corner
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))

    def region(l, t, r, b):
        return src.crop((l, t, r, b))

    cl = corner
    # corners
    out.paste(region(0, 0, cl, cl), (0, 0))
    out.paste(region(s - cl, 0, s, cl), (w - cl, 0))
    out.paste(region(0, s - cl, cl, s), (0, h - cl))
    out.paste(region(s - cl, s - cl, s, s), (w - cl, h - cl))
    # edges (stretched)
    top = region(cl, 0, s - cl, cl).resize((w - 2 * cl, cl), Image.NEAREST)
    bot = region(cl, s - cl, s - cl, s).resize((w - 2 * cl, cl), Image.NEAREST)
    lft = region(0, cl, cl, s - cl).resize((cl, h - 2 * cl), Image.NEAREST)
    rgt = region(s - cl, cl, s, s - cl).resize((cl, h - 2 * cl), Image.NEAREST)
    out.paste(top, (cl, 0)); out.paste(bot, (cl, h - cl))
    out.paste(lft, (0, cl)); out.paste(rgt, (w - cl, cl))
    # centre (stretched)
    ctr = region(cl, cl, s - cl, s - cl).resize((w - 2 * cl, h - 2 * cl), Image.NEAREST)
    out.paste(ctr, (cl, cl))
    return out


def panel(src, w, h, corner=6, interior=(0x22, 0x1B, 0x2E)):
    """A nine-sliced wood frame with a DARK interior so content reads clearly
    (classic RPG window: ornate border, dark fill)."""
    p = nine_slice(src, w, h, corner)
    d = ImageDraw.Draw(p)
    inset = corner - 2
    d.rectangle([inset, inset, w - 1 - inset, h - 1 - inset], fill=interior + (255,))
    # a subtle inner highlight line just inside the frame
    d.rectangle([inset, inset, w - 1 - inset, h - 1 - inset], outline=(0x4A, 0x3A, 0x2E, 255))
    return p


def build_ui():
    panel_src = Image.open(os.path.join(PACK, "Ui", "Theme", "Theme Wood", "nine_path_panel.png")).convert("RGBA")
    W, H = 240, 160
    bg = Image.new("RGBA", (W, H), (0x0E, 0x0C, 0x16, 255))
    # Three framed panels: left (equipment / shop list), right (bag / preview),
    # bottom (stats / description). Content is drawn dynamically on top.
    bg.alpha_composite(panel(panel_src, 94, 104), (4, 16))
    bg.alpha_composite(panel(panel_src, 134, 104), (102, 16))
    bg.alpha_composite(panel(panel_src, 232, 32), (4, 124))
    out = os.path.join(OUT, "ui.png")
    bg.convert("RGB").save(out)
    print(f"ui.png: {bg.size} -> {out}")


if __name__ == "__main__":
    build_icons()
    build_ui()
    print("done. Frame map: 0 Kunai 1 Shuriken 2 Hook 3 Arrow 4 Armor 5 Helmet "
          "6 Boot 7 Ring 8 Amulet 9 Guard 10 Potion 11 Scroll 12 Money "
          f"{CURSOR_FRAME} Cursor {EMPTY_FRAME} Empty")
