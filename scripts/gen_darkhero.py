#!/usr/bin/env python3
"""Build the `dark-hero` example — an animation-state showcase using the CC0 "DARK - Hero" pack.

The hero's ten animation sheets (all 48x48 frames) are concatenated, in a fixed order, into ONE 64x64
sprite sheet so the engine's animation controller can `play` each state as a frame range. Each 48x48
frame is centred + bottom-aligned into a 64x64 cell (GBA sprites must be 32 or 64), preserving the
animator's per-frame positioning so motion-heavy states (ledge climb rises, death sprawls) still flow.

Outputs into examples/dark-hero/:
  assets/hero.png     sheet64 — Idle8 Run8 Jump4 Fall4 Land12 LedgeGrab4 LedgeGrabIdle14 Climb13 Hit2 Death10
  assets/tileset.png  dark stone tiles (opaque, flattened onto a dark sky)
  assets/hazard.png   sheet — a stompable spike orb (alive / popped)
  src/maps.tish       the level: ground, a pit, a grabbable stone ledge, hazard + player spawns

Frame offsets (kept in sync with src/components.tish):
  IDLE 0  RUN 8  JUMP 16  FALL 20  LAND 24  LGRAB 36  LHANG 40  CLIMB 54  HIT 67  DEATH 69   (79 total)
"""
import os
from PIL import Image, ImageDraw

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
EX = os.path.join(REPO, "examples", "dark-hero")
ASSETS = os.path.join(EX, "assets")
SRC = os.path.join(EX, "src")
DH = os.path.expanduser("~/Downloads/DARK - Hero FREE Version")

SKY = (32, 30, 48)  # dark night sky


def clamp_colors(im, maxc=15):
    """Keep an RGBA sheet within a GBA 4bpp sprite's 15-colour budget (transparency excluded)."""
    im = im.convert("RGBA")
    px = list(im.getdata())
    opaque = set((r, g, b) for (r, g, b, a) in px if a > 8)
    if len(opaque) <= maxc:
        return im
    q = im.convert("RGB").quantize(colors=maxc).convert("RGB")
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    for i, (r, g, b, a) in enumerate(px):
        if a > 8:
            x, y = i % im.width, i // im.width
            out.putpixel((x, y), q.getpixel((x, y)) + (255,))
    return out


# state -> (source file suffix, frame count), IN LAYOUT ORDER
STATES = [
    ("Idle", 8), ("Run", 8), ("Jump", 4), ("Fall", 4), ("Land", 12),
    ("Ledge Grab", 4), ("Ledge Grab Idle", 14), ("Ledge Climb", 13), ("Hit", 2), ("Death", 10),
]


def make_hero():
    cells = []
    for name, n in STATES:
        im = Image.open(os.path.join(DH, f"Platformer Hero FREE Version-{name}.png")).convert("RGBA")
        frames = [im.crop((i * 48, 0, i * 48 + 48, 48)) for i in range(n)]
        # Centre the CHARACTER horizontally in the 64 cell (so flipping is symmetric — the art isn't
        # centred in its 48 frame). Use ONE dx per state (the average content centre) to keep intra-
        # animation motion intact; bottom-align the frame (feet ~cell y63). x=32 is the cell centre.
        centres = [((b[0] + b[2]) / 2) for b in (f.getbbox() for f in frames) if b]
        dx = int(round(32 - (sum(centres) / len(centres) if centres else 24)))
        for fr in frames:
            cell = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
            cell.paste(fr, (dx, 16), fr)
            cells.append(cell)
    sheet = Image.new("RGBA", (64 * len(cells), 64), (0, 0, 0, 0))
    for i, cell in enumerate(cells):
        sheet.paste(cell, (i * 64, 0))
    sheet = clamp_colors(sheet)
    sheet.save(os.path.join(ASSETS, "hero.png"))
    print(f"hero: {len(cells)} frames, 64x64; character centred at cell x=32, feet ~y63")


# tileset GIDs
SKY_GID, GROUND, DIRT, STONE = 1, 2, 3, 4
SOLID = [GROUND, DIRT, STONE]


def make_tileset():
    im = Image.new("RGBA", (16 * 4, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = lambda i: i * 16

    def block(i, base, top=None, line=None):
        d.rectangle([c(i), 0, c(i) + 15, 15], fill=base)
        if top:
            d.rectangle([c(i), 0, c(i) + 15, 3], fill=top)
            d.rectangle([c(i), 0, c(i) + 15, 0], fill=line or top)

    block(0, SKY)                                                  # sky
    block(1, (58, 54, 74), top=(96, 92, 120), line=(140, 200, 190))  # ground: dark stone, teal-lit top
    block(2, (46, 42, 60))                                         # dirt fill
    for (x, y) in [(3, 4), (9, 7), (6, 11), (12, 13)]:
        d.point((c(2) + x, y), fill=(36, 32, 48))
    block(3, (70, 66, 90))                                         # stone block
    for y in (0, 8):
        d.line([c(3), y, c(3) + 15, y], fill=(52, 48, 68))
    d.line([c(3) + 8, 0, c(3) + 8, 7], fill=(52, 48, 68))
    d.line([c(3) + 4, 8, c(3) + 4, 15], fill=(52, 48, 68))
    # flatten onto opaque sky so the bg importer never sees alpha
    out = Image.new("RGBA", im.size, SKY + (255,))
    out.alpha_composite(im)
    out.convert("RGB").convert("RGBA").save(os.path.join(ASSETS, "tileset.png"))


def make_hazard():
    """A spike orb: frame 0 alive (spikes out), frame 1 popped (small burst). 16x16, sheet:."""
    sheet = Image.new("RGBA", (16 * 2, 16), (0, 0, 0, 0))
    # alive
    a = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(a)
    d.ellipse([4, 4, 12, 12], fill=(60, 30, 40))
    d.ellipse([5, 5, 11, 11], fill=(150, 40, 60))
    for (dx, dy) in [(8, 0), (8, 15), (0, 8), (15, 8), (2, 2), (13, 2), (2, 13), (13, 13)]:
        d.line([8, 8, dx, dy], fill=(210, 70, 90))
    d.rectangle([6, 6, 7, 7], fill=(250, 220, 230))
    sheet.paste(a, (0, 0))
    # popped
    b = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(b)
    for (x, y) in [(3, 3), (12, 4), (5, 12), (11, 11), (8, 2), (2, 8), (14, 9)]:
        d.point((x, y), fill=(210, 70, 90))
    sheet.paste(b, (16, 0))
    sheet.save(os.path.join(ASSETS, "hazard.png"))


def make_heart():
    """HUD heart, 3 frames: empty / half / full (perHeart = 2)."""
    RED, GREY = (200, 60, 80), (58, 54, 74)
    sheet = Image.new("RGBA", (16 * 3, 16), (0, 0, 0, 0))

    def heart(dd, fill):
        dd.ellipse([2, 3, 8, 9], fill=fill); dd.ellipse([7, 3, 13, 9], fill=fill)
        dd.polygon([(2, 7), (13, 7), (7, 14)], fill=fill)

    for fx in range(3):
        im = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
        heart(ImageDraw.Draw(im), GREY)
        if fx >= 1:
            half = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
            heart(ImageDraw.Draw(half), RED)
            if fx == 1:
                for y in range(16):
                    for x in range(8, 16):
                        half.putpixel((x, y), (0, 0, 0, 0))
            im.alpha_composite(half)
        sheet.paste(im, (fx * 16, 0))
    sheet.save(os.path.join(ASSETS, "heart.png"))


#   '.' sky  '#' ground  'S' stone (walls/ledges)  '@' player  'H' hazard
# The player spawns a bit in the air (→ Fall → Land on boot, no input needed). A pit gives a longer
# fall + land; the tall stone block on the right is a grabbable ledge (jump up its face → grab →
# climb). A hazard patrols the middle floor (contact hurts; stomp pops it).
LEVEL = [
    "........................................",
    "........................................",
    "....@...................................",
    "........................................",
    "........................................",
    "........................................",
    "........................................",
    "........................................",
    "........................................",
    "........H.....................SSSS......",
    "..............................SSSS......",
    "..............................SSSS......",
    "########################################",
    "########################################",
    "########################################",
]

SOLID_CH = set("#S")


def parse_level(rows):
    h = len(rows)
    w = max(len(r) for r in rows)
    rows = [r.ljust(w, ".") for r in rows]
    gid = [[SKY_GID] * w for _ in range(h)]
    spawn = (5, 4)
    hcols, hrows = [], []
    for y in range(h):
        for x in range(w):
            ch = rows[y][x]
            if ch == "@":
                spawn = (x, y)
            elif ch == "H":
                hcols.append(x); hrows.append(y)
            elif ch == "#":
                above = rows[y - 1][x] if y > 0 else "."
                gid[y][x] = GROUND if above not in SOLID_CH else DIRT
            elif ch == "S":
                above = rows[y - 1][x] if y > 0 else "."
                gid[y][x] = STONE
    flat = [gid[y][x] for y in range(h) for x in range(w)]
    return w, h, flat, spawn, hcols, hrows


def emit_maps(path, w, h, data, spawn, hcols, hrows):
    rowstrs = ["      " + ", ".join(str(data[y * w + x]) for x in range(w)) + ("," if y < h - 1 else "")
               for y in range(h)]
    j = lambda xs: ", ".join(map(str, xs))
    txt = f"""// Generated by scripts/gen_darkhero.py — a Dark Hero animation-state showcase level.
// GIDs: 1 sky, 2 ground, 3 dirt, 4 stone.
export const level = {{
  width: {w}, height: {h}, tileSize: 16, tilesetCols: 4,
  spawnCol: {spawn[0]}, spawnRow: {spawn[1]},
  solid: [{j(SOLID)}],
  hazardCols: [{j(hcols)}], hazardRows: [{j(hrows)}],
  layers: [
    {{ priority: 2, data: [
{chr(10).join(rowstrs)}
    ] }}
  ]
}}
"""
    open(path, "w").write(txt)


if __name__ == "__main__":
    os.makedirs(ASSETS, exist_ok=True)
    os.makedirs(SRC, exist_ok=True)
    make_hero()
    make_tileset()
    make_hazard()
    make_heart()
    w, h, data, spawn, hc, hr = parse_level(LEVEL)
    emit_maps(os.path.join(SRC, "maps.tish"), w, h, data, spawn, hc, hr)
    print(f"dark-hero: {w}x{h} tiles, player {spawn}, {len(hc)} hazards")
