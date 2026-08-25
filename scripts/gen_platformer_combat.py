#!/usr/bin/env python3
"""Generate placeholder art + level data for the `platformer-combat` example.

Outputs into examples/platformer-combat/:
  assets/tileset.png   6-tile 16px strip: sky, grass, dirt, brick, platform(solid), one-way ledge
  assets/hero.png      4-frame 16x16 side hero (idle / run x2 / jump)
  assets/enemy.png     2-frame 16x16 patrol blob
  assets/heart.png     3-frame 16x16 heart: empty / half / full (perHeart = 2)
  src/maps.tish        the level as stream-map data (solid + one-way gids, player + enemy spawns)
"""
import os
from PIL import Image, ImageDraw

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
EX = os.path.join(REPO, "examples", "platformer-combat")

SKY, GRASS, DIRT, BRICK, PLAT, ONEWAY = 1, 2, 3, 4, 5, 6
SOLID = [GRASS, DIRT, BRICK, PLAT]
ONEWAY_GIDS = [ONEWAY]
SKY_RGB = (122, 190, 246)


def make_tileset():
    im = Image.new("RGBA", (16 * 6, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = lambda i: i * 16
    d.rectangle([c(0), 0, c(0) + 15, 15], fill=SKY_RGB)                              # sky
    d.rectangle([c(1), 0, c(1) + 15, 15], fill=(120, 78, 44))                        # grass
    d.rectangle([c(1), 0, c(1) + 15, 4], fill=(88, 176, 80)); d.rectangle([c(1), 5, c(1) + 15, 6], fill=(64, 132, 58))
    d.rectangle([c(2), 0, c(2) + 15, 15], fill=(120, 78, 44))                        # dirt
    for (x, y) in [(3, 3), (9, 6), (6, 11), (12, 13)]: d.point((c(2) + x, y), fill=(96, 60, 32))
    d.rectangle([c(3), 0, c(3) + 15, 15], fill=(122, 122, 132))                      # brick
    for y in (0, 8): d.line([c(3), y, c(3) + 15, y], fill=(80, 80, 90))
    d.line([c(3) + 8, 0, c(3) + 8, 7], fill=(80, 80, 90)); d.line([c(3) + 4, 8, c(3) + 4, 15], fill=(80, 80, 90))
    d.rectangle([c(4), 0, c(4) + 15, 15], fill=(120, 82, 44))                        # platform (solid wood)
    d.rectangle([c(4), 0, c(4) + 15, 4], fill=(168, 120, 66)); d.rectangle([c(4), 0, c(4) + 15, 1], fill=(198, 150, 92))
    d.line([c(4), 8, c(4) + 15, 8], fill=(96, 64, 34))
    d.rectangle([c(5), 0, c(5) + 15, 15], fill=SKY_RGB)                              # one-way ledge (thin plank on sky)
    d.rectangle([c(5), 2, c(5) + 15, 6], fill=(150, 108, 60)); d.rectangle([c(5), 2, c(5) + 15, 3], fill=(190, 146, 88))
    d.line([c(5), 6, c(5) + 15, 6], fill=(110, 74, 40))
    return im


def make_hero():
    SKIN, SHIRT, PANTS, HAIR = (245, 205, 160), (60, 120, 210), (40, 44, 60), (70, 46, 30)
    sheet = Image.new("RGBA", (16 * 4, 16), (0, 0, 0, 0))

    def draw(fx, legs):
        im = Image.new("RGBA", (16, 16), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
        d.rectangle([5, 1, 11, 5], fill=HAIR); d.rectangle([6, 3, 11, 7], fill=SKIN)
        d.rectangle([11, 4, 12, 5], fill=(20, 20, 20)); d.rectangle([5, 7, 11, 12], fill=SHIRT)
        d.rectangle([11, 8, 12, 11], fill=SKIN)
        for (x0, x1, y0, y1) in legs: d.rectangle([x0, y0, x1, y1], fill=PANTS)
        sheet.paste(im, (fx * 16, 0))
    draw(0, [(6, 8, 12, 15), (9, 11, 12, 15)])   # idle
    draw(1, [(4, 6, 12, 15), (10, 12, 12, 14)])  # run a
    draw(2, [(6, 8, 12, 14), (9, 11, 12, 15)])   # run b
    draw(3, [(5, 7, 11, 14), (10, 12, 11, 14)])  # jump
    return sheet


def make_enemy():
    BODY, DARK, EYE = (196, 72, 96), (150, 44, 68), (250, 250, 250)
    sheet = Image.new("RGBA", (16 * 2, 16), (0, 0, 0, 0))
    for fx, squash in ((0, 0), (1, 1)):
        im = Image.new("RGBA", (16, 16), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
        top = 5 + squash
        d.ellipse([2, top, 13, 15], fill=BODY)
        d.ellipse([2, top + 5, 13, 15], fill=DARK)
        d.rectangle([5, top + 3, 6, top + 4], fill=EYE); d.rectangle([9, top + 3, 10, top + 4], fill=EYE)
        sheet.paste(im, (fx * 16, 0))
    return sheet


def make_heart():
    """3 frames: 0 empty, 1 half, 2 full."""
    RED, DARK, GREY = (220, 60, 70), (150, 30, 44), (70, 70, 80)
    sheet = Image.new("RGBA", (16 * 3, 16), (0, 0, 0, 0))

    def heart_mask(d, fill):
        d.ellipse([2, 3, 8, 9], fill=fill); d.ellipse([7, 3, 13, 9], fill=fill)
        d.polygon([(2, 7), (13, 7), (7, 14)], fill=fill)

    for fx in range(3):
        im = Image.new("RGBA", (16, 16), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
        heart_mask(d, GREY)                                  # empty outline/base
        if fx >= 1:                                          # half = left side red
            half = Image.new("RGBA", (16, 16), (0, 0, 0, 0)); hd = ImageDraw.Draw(half)
            heart_mask(hd, RED)
            if fx == 1:
                for y in range(16):
                    for x in range(8, 16):
                        half.putpixel((x, y), (0, 0, 0, 0))
            im.alpha_composite(half)
        sheet.paste(im, (fx * 16, 0))
    return sheet


#   '.' sky   '#' ground   '=' solid platform   '^' one-way ledge   'B' brick
#   '@' player spawn   'E' enemy spawn
# Enemies must sit above solid ground on a bounded segment (walls / pits turn them). Pits are 3
# tiles — clearable with a running jump.
LEVEL = [
    "................................................................",
    "................................................................",
    ".....................^^^^^......................................",
    "................................................^^^^^...........",
    "............===.................====............................",
    "................................................................",
    "....................^^^^^.......................................",
    "....@.....E.................E.....................E..............",
    "................................................................",
    "................................................................",
    "................................................................",
    "###############...####################...######################",
    "###############...####################...######################",
    "###############...####################...######################",
]


def parse_level(rows):
    h = len(rows)
    w = max(len(r) for r in rows)
    rows = [r.ljust(w, ".") for r in rows]
    solid_ch = set("#=B")
    gid = [[SKY] * w for _ in range(h)]
    spawn = (4, h - 4)
    ecols, erows = [], []
    for y in range(h):
        for x in range(w):
            ch = rows[y][x]
            if ch == "@":
                spawn = (x, y)
            elif ch == "E":
                ecols.append(x); erows.append(y)
            elif ch == "#":
                above = rows[y - 1][x] if y > 0 else "."
                gid[y][x] = GRASS if above not in solid_ch else DIRT
            elif ch == "=":
                gid[y][x] = PLAT
            elif ch == "^":
                gid[y][x] = ONEWAY
            elif ch == "B":
                gid[y][x] = BRICK
    flat = [gid[y][x] for y in range(h) for x in range(w)]
    return w, h, flat, spawn, ecols, erows


def emit_maps(path, w, h, data, spawn, ecols, erows):
    rowstrs = []
    for y in range(h):
        rowstrs.append("      " + ", ".join(str(data[y * w + x]) for x in range(w)) + ("," if y < h - 1 else ""))
    body = "\n".join(rowstrs)
    txt = f"""// Generated by scripts/gen_platformer_combat.py — a side-scrolling platformer level.
// GIDs: 1 sky, 2 grass, 3 dirt, 4 brick, 5 solid platform, 6 one-way ledge.
export const level = {{
  width: {w}, height: {h}, tileSize: 16, tilesetCols: 6,
  spawnCol: {spawn[0]}, spawnRow: {spawn[1]},
  solid: [{", ".join(map(str, SOLID))}],
  oneway: [{", ".join(map(str, ONEWAY_GIDS))}],
  enemyCols: [{", ".join(map(str, ecols))}],
  enemyRows: [{", ".join(map(str, erows))}],
  layers: [
    {{ priority: 2, data: [
{body}
    ] }}
  ]
}}
"""
    open(path, "w").write(txt)


if __name__ == "__main__":
    os.makedirs(os.path.join(EX, "assets"), exist_ok=True)
    os.makedirs(os.path.join(EX, "src"), exist_ok=True)
    make_tileset().save(os.path.join(EX, "assets", "tileset.png"))
    make_hero().save(os.path.join(EX, "assets", "hero.png"))
    make_enemy().save(os.path.join(EX, "assets", "enemy.png"))
    make_heart().save(os.path.join(EX, "assets", "heart.png"))
    w, h, data, spawn, ecols, erows = parse_level(LEVEL)
    emit_maps(os.path.join(EX, "src", "maps.tish"), w, h, data, spawn, ecols, erows)
    print(f"platformer-combat: {w}x{h} tiles, player {spawn}, {len(ecols)} enemies")
