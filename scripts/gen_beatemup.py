#!/usr/bin/env python3
"""Build the `beatemup` example's ROM assets from the vendored CC0 art.

Same source packs and same 24-pose sheet layout as `examples/versus` — the baking lives in
scripts/fighter_art.py and is shared. What differs is everything downstream of the character:

  <char>.png   sheet64 — 35 cells, but baked SHORTER than the fighting game's (see TARGET_H)
  street.png   background — one opaque 256x256 image: sky and treeline above, a walkable road below
  shadow.png   sheet16 — 4 sizes of ground shadow, which is the only thing that makes a jump legible
  hit.png      sheet32 — impact bursts
  digits.png   sheet16 — score and health numbers, which must not be text

⚠️ WHY THE CHARACTERS ARE SHORTER HERE. A versus fighter is 54 px because two of them share the
screen. A brawler puts FOUR on it, in a road that also has to be deep enough to walk up and down,
so 46 px is what leaves room for the lanes to read as depth rather than as a single line.
"""
import os

from PIL import Image

import sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fighter_art import (REPO, ART, CELL, FEET, build_char, clamp_colors, digits_sheet,
                         fx_side_tish, harden_alpha)

EX = os.path.join(REPO, "examples", "beatemup")
ASSETS = os.path.join(EX, "assets")
SRC = os.path.join(EX, "src")

TARGET_H = 46

# The roster: the player first, then the enemy types. Same four packs as versus, cast differently —
# the samurai is the one you play, the rest are the gang.
CHARS = [("hero", "HERO"), ("hero2", "NINJA"), ("hero3", "BRUISER"), ("warrior", "BOSS")]

# ── the street ───────────────────────────────────────────────────────────────────────────────────
# Where the road is, in screen rows. The fighters' FEET move between LANE_TOP and LANE_BOTTOM; that
# band IS the depth axis, and everything about how the game reads depends on it being wide enough to
# see. 38 rows against a 46 px character is about the ratio Final Fight uses.
ROAD_TOP = 92
LANE_TOP = 104
LANE_BOTTOM = 150

MD = "mountain-dusk/Super Mountain Dusk Files/Assets/version A/Layers"
ROAD_FAR = (86, 62, 74)
ROAD_NEAR = (54, 38, 50)
KERB = (120, 92, 84)

# ── the parallax stage ───────────────────────────────────────────────────────────────────────────
# THREE layers out of ONE atlas, which is the only way the GBA will do this:
#
#   * `tilemap_new` calls `set_background_palettes`, which REPLACES all sixteen background palettes.
#     Two different `background:` imports on screen at once therefore fight and the loser renders in
#     the winner's colours — so every layer is built from GIDs into one shared tileset.
#   * A layer in front needs HOLES for the one behind to show through, and a hole is GID 0 — an
#     ABSENT tile, not a transparent one. Partially covered tiles keep their own alpha, which agb
#     maps to palette index 0 (`Colour::is_transparent` is `a != 255`), so the silhouettes stay
#     pixel-shaped rather than 16px-blocky.
#   * Four background layers exist and `ui_begin` has already taken one, so THREE is the budget.
#
# The layers are cut from the pack's own separate PNGs rather than from a flattened picture, which
# is what makes them separable at all.
TILE = 16
GRID_W = 16          # 16 x 16px tiles = 256px = exactly the GBA's background wrap
GRID_H = 10          # 160px, the visible height
ATLAS_COLS = 16

# name, source layers (bottom to top), opaque, parallax multiplier (1/256ths), vertical offset.
#
# ⚠️ THE OFFSETS ARE THE WHOLE JOB. The pack's layers are all 192px tall and all bottom-aligned to
# each other, so dropping them in as-is buries the treeline under the road — which is exactly what
# the first attempt did, and it read as "the trees layer is missing". Each one is shifted so the
# thing you want to SEE lands in the 160 rows above the road: the moon and far peaks up top, the
# cloud bank behind the treeline, and the trees' base right on ROAD_TOP.
# name, sources (bottom to top), opaque, parallax mul (1/256ths), height, base row.
#
# ⚠️ SCALE EACH LAYER TO ITS BAND — DO NOT CROP IT. The pack's layers are 192px tall and all
# bottom-aligned to each other, and there are only ~96 rows above the road. Taking a window out of
# each one (which is what this did first) cuts every silhouette with a hard horizontal line: the
# treeline arrives with its tops sliced off and the cloud bank becomes a rectangle. Squashing the
# layer's CONTENT into the rows it is allowed instead keeps every shape whole; a backdrop that is
# 40% shorter than the artist drew is a thing nobody notices, and a sliced one is the first thing
# they do.
#
# `height` is the content's height after scaling, `base` the screen row its BOTTOM sits on. The
# bases are staggered so each layer's foot is hidden behind the one in front of it.
# `fit` is "crop" or "scale", and choosing per layer is the whole trick:
#
#   CROP where the layer is OCCLUDED. The far layer's mountains disappear behind the cloud bank and
#   the trees, so cutting its bottom off costs nothing — and cutting rather than squashing is what
#   keeps the moon round. Squash a 192px layer into 74 rows and the moon becomes an ellipse.
#
#   SCALE where the layer is VISIBLE top to bottom. The treeline shows its tips AND its base, so
#   there is nowhere to cut it; a conifer squashed to 60% is still obviously a conifer, and a
#   conifer with its top sliced off is obviously a bug.
LAYERS = [
    ("far",   ("sky", "far-clouds", "far-mountains"),  True,   24,  86,  86, "crop"),
    ("mid",   ("mountains", "near-clouds"),            False,  96,  58,  93, "crop"),
    ("near",  ("trees",),                              False,  256, 46,  99, "scale"),
]


def compose(names, opaque):
    """One layer of the ansimuz pack, tiled to 320px and scaled to the 256px wrap."""
    canvas = Image.new("RGBA", (320, 240), (0, 0, 0, 255) if opaque else (0, 0, 0, 0))
    for n in names:
        im = Image.open(os.path.join(ART, MD, n + ".png")).convert("RGBA")
        tiled = Image.new("RGBA", (320, 240), (0, 0, 0, 0))
        for x in range(0, 320, im.width):
            tiled.paste(im, (x, 0), im)
        canvas = Image.alpha_composite(canvas, tiled)
    return canvas.resize((256, 192), Image.LANCZOS)


def road_strip(width, top, bottom):
    """The road, shaded far-to-near.

    ⚠️ The texture is PERIODIC on 16px, not random. The speckle this started with was
    `(x * 7 + y * 3) % 37`, which makes every 16x16 tile unique and turned a 40-tile road into 160
    tiles of background VRAM. A pattern whose period divides the tile size dedupes to a handful.
    """
    im = Image.new("RGBA", (width, bottom - top), (0, 0, 0, 0))
    span = bottom - top
    for y in range(top, bottom):
        t = (y - top) / float(span)
        base = tuple(int(ROAD_FAR[i] + (ROAD_NEAR[i] - ROAD_FAR[i]) * t) for i in range(3))
        for x in range(width):
            c = base
            if y < top + 2:
                c = KERB
            elif ((x & 15) == 3) and ((y & 7) == 2):
                c = tuple(min(255, v + 16) for v in base)
            elif ((x & 15) == 11) and ((y & 7) == 6):
                c = tuple(max(0, v - 14) for v in base)
            im.putpixel((x, y - top), c + (255,))
    return im


def palettes_needed(q, maxc=15, maxp=16):
    """How many GBA background palettes this image needs, by the same greedy packing agb uses.

    ⚠️ "15 colours per 8x8 tile" is necessary but NOT sufficient. `include_background_gfx!` also has
    to fit every tile's palette into SIXTEEN palettes of sixteen, and it panics with
    `Failed to optimised palettes: DoesNotFitError { count: 25 }` when it cannot — a message that
    names no tile and no colour. Checking it here turns a proc-macro panic into a number.
    """
    sets = []
    for ty in range(0, q.height, 8):
        for tx in range(0, q.width, 8):
            cell = q.crop((tx, ty, tx + 8, ty + 8))
            c = frozenset((r, g, b) for r, g, b, a in cell.getdata() if a > 8)
            if c and len(c) <= maxc:
                sets.append(c)
            elif len(c) > maxc:
                return maxp + 99, len(c)
    pals = []
    for c in sorted(sets, key=len, reverse=True):
        best, bestgain = None, None
        for i, p in enumerate(pals):
            u = p | c
            if len(u) <= maxc:
                gain = len(u) - len(p)
                if bestgain is None or gain < bestgain:
                    best, bestgain = i, gain
        if best is None:
            pals.append(set(c))
        else:
            pals[best] |= c
    worst = max((len(c) for c in sets), default=0)
    return len(pals), worst


# ⚠️ 26 IS AN EMPIRICAL CEILING, NOT A CALCULATED ONE. `palettes_needed` below implements a greedy
# first-fit packing and says this stage needs 8 palettes at 32 colours — agb's own
# `pagination_packing::overload_and_remove` needs 25 and refuses to build. A local check is a useful
# guard but it is NOT a substitute for the real packer, so the budget list simply starts low enough
# to be safe. If a future stage fails to build with `DoesNotFitError`, drop the first entry.
def fit_palette(im, budgets=(26, 22, 18, 15)):
    """Quantise to the most colours agb will actually accept for a background."""
    for n in budgets:
        q = clamp_colors(im, n)
        pals, worst = palettes_needed(q)
        if pals <= 16:
            print("  stage    palette %d colours -> %d GBA palettes (worst tile %d of 15)"
                  % (n, pals, worst))
            return q
    return clamp_colors(im, 15)


def build_stage():
    """Bake the three layers into one atlas of unique 16x16 tiles plus one GID grid each."""
    imgs = {}
    for name, srcs, opaque, _, height, base, fit in LAYERS:
        layer = Image.new("RGBA", (256, GRID_H * TILE), (0, 0, 0, 0))
        src = compose(srcs, opaque)
        box = src.getbbox()
        content = src.crop((0, box[1], 256, box[3]))
        if fit == "scale":
            content = content.resize((256, height), Image.LANCZOS)
        else:
            content = content.crop((0, 0, 256, min(height, content.height)))
        layer.paste(content, (0, base - content.height))
        if opaque:
            # The backmost layer must have no holes at all or the GBA backdrop shows through, and
            # the fill has to be the SKY colour (the layer's own top row), not its average.
            flat = Image.new("RGBA", (256, GRID_H * TILE), src.getpixel((0, 0)))
            flat.alpha_composite(layer)
            layer = flat
        if name == "near":
            # The road belongs to the nearest layer: it must travel with the camera exactly, or the
            # ground slides under the characters' feet.
            layer.paste(road_strip(256, ROAD_TOP, GRID_H * TILE), (0, ROAD_TOP))
        imgs[name] = harden_alpha(layer)

    # One shared 15-colour palette for all three, quantised together — see the note above on
    # set_background_palettes.
    merged = Image.new("RGBA", (256, GRID_H * TILE * len(LAYERS)), (0, 0, 0, 0))
    for i, (name, _, _, _, _, _, _) in enumerate(LAYERS):
        merged.paste(imgs[name], (0, i * GRID_H * TILE))
    # ⚠️ The budget is 15 colours PER 8x8 TILE, not per image: `include_background_gfx!` emits
    # several palettes and assigns one to each tile, and it PANICS ("Can have at most 16 colours in
    # a single palette") if any tile needs more. Quantising the whole stage to 15 to be safe
    # flattened the sky, the clouds and the trees into one mush; quantising to 45 blew a tile.
    # So: take the highest budget whose worst tile still fits, and say which one that was.
    merged = fit_palette(merged)
    for i, (name, _, _, _, _, _, _) in enumerate(LAYERS):
        imgs[name] = merged.crop((0, i * GRID_H * TILE, 256, (i + 1) * GRID_H * TILE))

    tiles = []
    keys = {}
    grids = {}
    for name, _, _, _, _, _, _ in LAYERS:
        g = []
        for r in range(GRID_H):
            for c in range(GRID_W):
                cell = imgs[name].crop((c * TILE, r * TILE, (c + 1) * TILE, (r + 1) * TILE))
                if cell.getbbox() is None:
                    g.append(0)          # GID 0 is an ABSENT tile — the hole the layer behind shows through
                    continue
                k = cell.tobytes()
                if k not in keys:
                    keys[k] = len(tiles) + 1
                    tiles.append(cell)
                g.append(keys[k])
        grids[name] = g

    rows = (len(tiles) + ATLAS_COLS - 1) // ATLAS_COLS
    atlas = Image.new("RGBA", (ATLAS_COLS * TILE, max(1, rows) * TILE), (0, 0, 0, 0))
    for i, t in enumerate(tiles):
        atlas.paste(t, ((i % ATLAS_COLS) * TILE, (i // ATLAS_COLS) * TILE))
    atlas.save(os.path.join(ASSETS, "stage.png"))
    # Composited FAR to NEAR — the opposite of the creation order the game uses, where the first
    # background built is the one drawn in front.
    preview = Image.new("RGBA", (256, GRID_H * TILE), (0, 0, 0, 255))
    for name, _, _, _, _, _, _ in LAYERS:
        preview.alpha_composite(imgs[name])
    preview.convert("RGB").save(os.path.join(ASSETS, "stage-preview.png"))
    print("  stage    atlas %dx%d, %d unique tiles, 3 layers"
          % (atlas.width, atlas.height, len(tiles)))
    return grids


# ── shadow ───────────────────────────────────────────────────────────────────────────────────────
# ⚠️ THE SHADOW IS NOT DECORATION. With a virtual Z axis there is nothing on screen to say whether a
# sprite is standing far away or hanging in the air above a near lane — they are the same pixels at
# the same height. The shadow stays on the ground and shrinks with altitude, and it is the only
# reason a jump reads at all. Four sizes: grounded down to high in the air.
def build_shadow():
    cells = []
    for i, (rw, rh, a) in enumerate(((11, 4, 150), (9, 3, 120), (7, 3, 95), (5, 2, 70))):
        c = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
        for y in range(16):
            for x in range(16):
                dx, dy = (x - 7.5) / rw, (y - 11.5) / rh
                if dx * dx + dy * dy <= 1.0:
                    c.putpixel((x, y), (16, 10, 20, a))
        cells.append(c)
    strip = Image.new("RGBA", (16 * len(cells), 16), (0, 0, 0, 0))
    for i, c in enumerate(cells):
        strip.paste(c, (i * 16, 0))
    clamp_colors(strip, 15).save(os.path.join(ASSETS, "shadow.png"))
    print("  shadow   4 cells (grounded .. airborne)")


# ── impact ───────────────────────────────────────────────────────────────────────────────────────
HIT = [(255, 246, 220), (255, 198, 96), (240, 120, 60)]


def build_hit():
    import math
    cells = []
    for step in range(4):
        c = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
        r0, r1 = 2 + step * 3, 7 + step * 4
        for i in range(9):
            a = (i * 2861 + step * 71) % 360
            ca, sa = math.cos(math.radians(a)), math.sin(math.radians(a))
            for t in range(r0, r1):
                x, y = 16 + int(ca * t), 16 + int(sa * t)
                if 0 <= x < 32 and 0 <= y < 32:
                    col = HIT[min(2, (t - r0) * 3 // max(1, r1 - r0))]
                    c.putpixel((x, y), col + (255,))
                    if t < r1 - 2 and x + 1 < 32:
                        c.putpixel((x + 1, y), col + (255,))
        cells.append(c)
    strip = Image.new("RGBA", (32 * len(cells), 32), (0, 0, 0, 0))
    for i, c in enumerate(cells):
        strip.paste(c, (i * 32, 0))
    clamp_colors(strip, 15).save(os.path.join(ASSETS, "hit.png"))
    print("  hit      4 cells")


# ── generated tish ───────────────────────────────────────────────────────────────────────────────
def write_frames(fx, grids):
    dxl, dyl = fx_side_tish(fx, [n for n, _ in CHARS])
    lines = [
        "// GENERATED by scripts/gen_beatemup.py — do not edit.",
        "//",
        "// Every character sheet has the same 35-cell layout as examples/versus (they share",
        "// scripts/fighter_art.py): cells 0..23 body poses, 24..33 attack FX overlays for poses",
        "// 11..20, cell 34 a portrait.",
        "",
        "export const F_IDLE = 0",
        "export const F_IDLE_LEN = 4",
        "export const F_WALK = 4",
        "export const F_WALK_LEN = 4",
        "export const F_JUMP = 8",
        "export const F_FALL = 9",
        "export const F_CROUCH = 10",
        "export const F_LOW_ATK = 11",
        "export const F_ATK1 = 12",
        "export const F_ATK2 = 15",
        "export const F_ATK3 = 18",
        "export const F_GUARD = 21",
        "export const F_HIT = 22",
        "export const F_KO = 23",
        "export const F_FX = 24",
        "export const F_FX_FIRST = 11",
        "export const F_PORTRAIT = 34",
        "",
        "// Which neighbouring 64x64 window each attack pose's overlay was cut from, in CELL units,",
        "// indexed [char * 10 + (pose - 11)]. Authored facing right and stored unmirrored.",
        dxl,
        dyl,
        "",
        "// The roster, in sheet order: index 0 is the player.",
    ]
    for i, (name, label) in enumerate(CHARS):
        lines.append('export const NAME_%s = "%s"' % (name.upper(), label))
    lines += [
        "",
        "// The street's geometry, in screen rows. LANE_TOP..LANE_BOTTOM is the depth axis: a",
        "// character's Y is where its FEET are on the road, and that is also its draw order.",
        "export const ROAD_TOP = %d" % ROAD_TOP,
        "export const LANE_TOP = %d" % LANE_TOP,
        "export const LANE_BOTTOM = %d" % LANE_BOTTOM,
        "",
        "// The parallax stage: one shared atlas, three GID grids, and how fast each drifts. See",
        "// packages/parallax.tish — layers are created NEAR first, because two backdrops sharing a",
        "// priority break the tie by creation order and the first one drawn wins.",
        "export const ATLAS_COLS = %d" % ATLAS_COLS,
        "export const GRID_W = %d" % GRID_W,
        "export const GRID_H = %d" % GRID_H,
    ] + [
        "export const MUL_%s = %d" % (n.upper(), mul) for n, _, _, mul, _, _, _ in LAYERS
    ] + [
        "export const LAYER_%s: i32[] = [%s]" % (n.upper(), ", ".join(str(g) for g in grids[n]))
        for n, _, _, _, _, _, _ in LAYERS
    ] + [
        "export const CELL = %d" % CELL,
        "export const FEET = %d" % FEET,
    ]
    with open(os.path.join(SRC, "frames.tish"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("  frames.tish  written")


def main():
    os.makedirs(ASSETS, exist_ok=True)
    os.makedirs(SRC, exist_ok=True)
    print("beatemup assets:")
    fx = {}
    for name, _ in CHARS:
        fx[name] = build_char(name, TARGET_H, os.path.join(ASSETS, name + ".png"))
    grids = build_stage()
    build_shadow()
    build_hit()
    digits_sheet(os.path.join(ASSETS, "digits.png"))
    print("  digits   10 cells 16x16")
    write_frames(fx, grids)


if __name__ == "__main__":
    main()
