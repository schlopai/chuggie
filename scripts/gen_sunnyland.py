#!/usr/bin/env python3
"""Build the `sunny-land` example's ROM assets + level from the vendored Sunny Land art.

Source art (CC0 / public domain) is Luis Zuno's "Sunny Land" pack; the raw files live outside the
repo (~/Downloads). This script copies the pixels it uses into GBA-ready sheets under
examples/sunny-land/assets and writes the level as stream-map data. Re-run after editing LEVEL.

  tileset.png   8-tile 16px strip (opaque — the `background:` importer forced-blanks on any alpha):
                sky, grass, dirt, stone, crate, grass-platform(one-way), grass-tuft, rock
  player.png    sheet32 — Foxy: idle x4, run x6, jump-up, fall, hurt x2   (14 frames)
  opossum.png   sheet32 — 6-frame walk cycle (patrol enemy)
  cherry.png    sheet16 — 5-frame spin (coin)
  gem.png       sheet16 — 5-frame spin (double-jump powerup)
  heart.png     sheet16 — 3 frames empty/half/full (HP HUD, 2 hp per heart)
  fx-poof.png   sheet32 — 6-frame enemy-death puff
  fx-spark.png  sheet32 — 4-frame item-collect sparkle
  src/maps.tish the level (tile GIDs + solid/one-way sets + object spawns)

Sprites keep their transparency (GBA maps alpha to the transparent palette index); only the
background tileset is flattened onto an opaque sky. Every sheet is kept within a GBA 4bpp sprite's
15-colour budget (see `clamp_colors`). All scaling is NEAREST so the pixel-art palette stays small.
"""
import os
from PIL import Image

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
EX = os.path.join(REPO, "examples", "sunny-land")
ASSETS = os.path.join(EX, "assets")
SRC = os.path.join(EX, "src")

# The raw Sunny Land pack (two mirrors of it in ~/Downloads); tiles come from the web-demo tileset,
# characters/items from the source pack.
DL = os.path.expanduser("~/Downloads")
TILESET_SRC = os.path.join(DL, "sunny-land/assets/environment/tileset.png")
PACK = os.path.join(DL, "Sunny-land-files/Assets")

SKY = (128, 235, 252)  # matches the pack's back.png sky (a light cyan)


# ── helpers ──────────────────────────────────────────────────────────────────
def clamp_colors(im, maxc=15):
    """Keep an RGBA sprite sheet within a GBA 4bpp sprite's colour budget (15 + transparent).
    Only touches opaque pixels; transparency is preserved."""
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


def union_bbox(frames):
    """The alpha bounding box that contains every frame's content (for consistent alignment)."""
    box = None
    for f in frames:
        b = f.getbbox()
        if b is None:
            continue
        box = b if box is None else (min(box[0], b[0]), min(box[1], b[1]),
                                     max(box[2], b[2]), max(box[3], b[3]))
    return box


def place(frame, size, dx, dy):
    """Paste `frame` into a `size`×`size` transparent cell at (dx, dy) (PIL clips overflow)."""
    cell = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    cell.paste(frame, (dx, dy), frame)
    return cell


def sheet_from(frames, size, ground=True):
    """Compose equal-size frames into a horizontal sheet, aligning every frame the SAME way (from the
    union bbox) so the character doesn't jitter: horizontally centred, and — for `ground` — feet on
    the cell's bottom edge (so a (-size/2, -size) sprite offset lands it on a bottom-anchored hitbox)."""
    box = union_bbox(frames)
    cx = (box[0] + box[2]) / 2
    dx = int(round(size / 2 - cx))
    dy = (size - box[3]) if ground else int(round(size / 2 - (box[1] + box[3]) / 2))
    sheet = Image.new("RGBA", (size * len(frames), size), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        sheet.paste(place(f, size, dx, dy), (i * size, 0))
    return clamp_colors(sheet)


def scale_nearest(im, w, h):
    return im.resize((w, h), Image.NEAREST)


def slice_strip(path, fw, fh, n):
    """Cut a horizontal spritesheet into `n` frames of `fw`×`fh`."""
    im = Image.open(path).convert("RGBA")
    return [im.crop((i * fw, 0, i * fw + fw, fh)) for i in range(n)]


# ── tileset ──────────────────────────────────────────────────────────────────
# GID = strip index + 1. `tilesetCols` = number of tiles (one row).
SKY_GID, GRASS, DIRT, STONE, CRATE, GPLAT, TUFT, ROCK = range(1, 9)
SOLID = [GRASS, DIRT, STONE, CRATE]
ONEWAY = [GPLAT]


def make_tileset():
    ts = Image.open(TILESET_SRC).convert("RGBA")

    def tile(c, r):
        return ts.crop((c * 16, r * 16, c * 16 + 16, r * 16 + 16))

    sky = Image.new("RGBA", (16, 16), SKY + (255,))
    # source (col, row) in the Sunny Land tileset for each curated GID (sky is synthetic).
    picks = [None, tile(1, 1), tile(1, 3), tile(7, 1), tile(1, 20),
             tile(17, 14), tile(1, 7), tile(5, 7)]
    strip = Image.new("RGBA", (16 * 8, 16), (0, 0, 0, 0))
    for i in range(8):
        cell = sky.copy()
        if picks[i] is not None:
            cell.alpha_composite(picks[i])          # flatten onto sky → fully opaque
        strip.paste(cell.convert("RGB").convert("RGBA"), (i * 16, 0))
    strip.save(os.path.join(ASSETS, "tileset.png"))


# ── characters / items ───────────────────────────────────────────────────────
def make_player():
    fx = os.path.join(PACK, "Characters/Foxy/Sprites")

    def frames(sub, name, idxs):
        # each source frame is 33×32; drop the left column (fox faces right, tail on the left) → 32×32
        return [Image.open(os.path.join(fx, sub, f"{name}-{i}.png")).convert("RGBA").crop((1, 0, 33, 32))
                for i in idxs]

    order = (frames("idle", "player-idle", [1, 2, 3, 4]) +
             frames("run", "player-run", [1, 2, 3, 4, 5, 6]) +
             frames("jump", "player-jump", [1, 2]) +
             frames("hurt", "player-hurt", [1, 2]))
    sheet_from(order, 32).save(os.path.join(ASSETS, "player.png"))


def make_opossum():
    frames = slice_strip(os.path.join(PACK, "Characters/Opossum/spritesheet.png"), 36, 28, 6)
    frames = [scale_nearest(f, 32, 25) for f in frames]   # 36×28 → fit 32 wide, keep aspect
    sheet_from(frames, 32).save(os.path.join(ASSETS, "opossum.png"))


def make_cherry():
    frames = slice_strip(os.path.join(PACK, "Misc/Sunnyland items/Spritesheets/cherry.png"), 21, 21, 5)
    frames = [scale_nearest(f, 16, 16) for f in frames]
    sheet_from(frames, 16, ground=False).save(os.path.join(ASSETS, "cherry.png"))


def make_gem():
    frames = slice_strip(os.path.join(PACK, "Misc/Sunnyland items/Spritesheets/gem.png"), 15, 13, 5)
    frames = [scale_nearest(f, 16, 14) for f in frames]
    sheet_from(frames, 16, ground=False).save(os.path.join(ASSETS, "gem.png"))


def make_fx():
    poof = slice_strip(os.path.join(PACK, "Misc/Sunnyland FX/Spritesheets/enemy-deadth.png"), 40, 41, 6)
    poof = [scale_nearest(f, 32, 32) for f in poof]
    sheet_from(poof, 32, ground=False).save(os.path.join(ASSETS, "fx-poof.png"))
    spark = slice_strip(os.path.join(PACK, "Misc/Sunnyland FX/Spritesheets/item-feedback.png"), 40, 32, 4)
    spark = [scale_nearest(f, 32, 32) for f in spark]
    sheet_from(spark, 32, ground=False).save(os.path.join(ASSETS, "fx-spark.png"))


def make_heart():
    """Synthetic HUD heart: 3 frames — empty / half / full (perHeart = 2)."""
    from PIL import ImageDraw
    RED, GREY = (222, 64, 74), (66, 70, 92)
    sheet = Image.new("RGBA", (16 * 3, 16), (0, 0, 0, 0))

    def heart(d, fill):
        d.ellipse([2, 3, 8, 9], fill=fill)
        d.ellipse([7, 3, 13, 9], fill=fill)
        d.polygon([(2, 7), (13, 7), (7, 14)], fill=fill)

    for fx in range(3):
        im = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
        d = ImageDraw.Draw(im)
        heart(d, GREY)
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


# ── level ────────────────────────────────────────────────────────────────────
#   '.' sky        '#' ground (grass on top, dirt below)   'S' stone block (solid)
#   'X' crate      '^' one-way grass platform               't' grass tuft   'r' rock (decor)
#   '@' player     'E' opossum enemy   'o' cherry (coin)    'g' gem (double-jump powerup)
# The FIRST chasm has a pillar in the middle, so the opening gap is two 1-tile hops a WALKING player
# clears. The later 3-tile chasms still want a run (hold B) — the level teaches the run before it
# demands it. Falling in is survivable either way: see PIT_Y in components.tish.
# Kept modest on entity count: the tish entity wrapper (~30 method closures) is heavy on the small
# GBA heap, so a scene tops out around ~25 live wrapped entities. Player + 5 enemies + 12 coins +
# 1 gem = 19, with headroom for a couple of transient FX. (Coins/FX free their wrapper on despawn.)
LEVEL = [
    "....................................................................................................",
    "....................................................................................................",
    "..........................o.o.o.....................................................................",
    ".......................^^^^^................................o........................................",
    "...........................................................g........................o.o.............",
    ".............o.o...................o.o.o..................^^^^^....................^^^^^..............",
    "........^^^^^..........SSS.............................................SSSS..........................",
    "..................................E.................................................................",
    "....................................................................................................",
    "...........................t.........r..............t..........r.........t..........................",
    "....@...t.....E......t.........E...........t....E........t...........r....t........E....t............",
    "###############.#.####################...##############...##############...#####################...##",
    "###############.#.####################...##############...##############...#####################...##",
    "###############.#.####################...##############...##############...#####################...##",
    "###############.#.####################...##############...##############...#####################...##",
]

SOLID_CH = set("#SX")


def parse_level(rows):
    h = len(rows)
    w = max(len(r) for r in rows)
    rows = [r.ljust(w, ".") for r in rows]
    gid = [[SKY_GID] * w for _ in range(h)]
    spawn = (4, h - 5)
    enemies, cherries, gems, tufts, rocks = [], [], [], [], []
    for y in range(h):
        for x in range(w):
            ch = rows[y][x]
            if ch == "@":
                spawn = (x, y)
            elif ch == "E":
                enemies.append((x, y))
            elif ch == "o":
                cherries.append((x, y))
            elif ch == "g":
                gems.append((x, y))
            elif ch == "t":
                gid[y][x] = TUFT
            elif ch == "r":
                gid[y][x] = ROCK
            elif ch == "#":
                above = rows[y - 1][x] if y > 0 else "."
                gid[y][x] = GRASS if above not in SOLID_CH else DIRT
            elif ch == "S":
                gid[y][x] = STONE
            elif ch == "X":
                gid[y][x] = CRATE
            elif ch == "^":
                gid[y][x] = GPLAT
    flat = [gid[y][x] for y in range(h) for x in range(w)]
    return w, h, flat, spawn, enemies, cherries, gems


def cols_rows(pairs):
    return [p[0] for p in pairs], [p[1] for p in pairs]


def emit_maps(path, w, h, data, spawn, enemies, cherries, gems):
    ec, er = cols_rows(enemies)
    cc, cr = cols_rows(cherries)
    gc, gr = cols_rows(gems)
    rowstrs = []
    for y in range(h):
        line = "      " + ", ".join(str(data[y * w + x]) for x in range(w))
        rowstrs.append(line + ("," if y < h - 1 else ""))
    body = "\n".join(rowstrs)
    j = lambda xs: ", ".join(map(str, xs))
    txt = f"""// Generated by scripts/gen_sunnyland.py — a classic side-scrolling Sunny Land level.
// GIDs: 1 sky, 2 grass, 3 dirt, 4 stone, 5 crate, 6 one-way grass platform, 7 grass tuft, 8 rock.
export const level = {{
  width: {w}, height: {h}, tileSize: 16, tilesetCols: 8,
  spawnCol: {spawn[0]}, spawnRow: {spawn[1]},
  solid: [{j(SOLID)}],
  oneway: [{j(ONEWAY)}],
  enemyCols: [{j(ec)}], enemyRows: [{j(er)}],
  cherryCols: [{j(cc)}], cherryRows: [{j(cr)}],
  gemCols: [{j(gc)}], gemRows: [{j(gr)}],
  layers: [
    {{ priority: 2, data: [
{body}
    ] }}
  ]
}}
"""
    open(path, "w").write(txt)


if __name__ == "__main__":
    os.makedirs(ASSETS, exist_ok=True)
    os.makedirs(SRC, exist_ok=True)
    make_tileset()
    make_player()
    make_opossum()
    make_cherry()
    make_gem()
    make_fx()
    make_heart()
    w, h, data, spawn, enemies, cherries, gems = parse_level(LEVEL)
    emit_maps(os.path.join(SRC, "maps.tish"), w, h, data, spawn, enemies, cherries, gems)
    print(f"sunny-land: {w}x{h} tiles · player {spawn} · "
          f"{len(enemies)} enemies · {len(cherries)} cherries · {len(gems)} gems")
