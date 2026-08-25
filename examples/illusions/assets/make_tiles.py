#!/usr/bin/env python3
"""Generate `pat.png`, the 8x8 pattern tileset every procedural illusion page paints with.

WHY A TILESET AND NOT THE UI CANVAS. A cafe-wall or Hermann grid covers the whole screen, and the
canvas's filled `ui_rect` path is pixel-exact for anything <= 48px tall — which means a tile
allocated per cell, ~600 of them, ~40KB of VRAM, for a picture made of about eight distinct 8x8
squares. A tilemap REFERENCES tiles instead of allocating them, so the same screen costs the tileset
and nothing more. `tilemap_set8` addresses one 8x8 tile by a linear 1-based index, which is why this
image is laid out as a plain row-major grid and why it is imported with `bgtiles:` (no dedup) — see
the `_bgtiles_comment` in crates/tish-agb/tish.schemes.json.

Run: python3 make_tiles.py   (regenerates pat.png in place)
"""
from PIL import Image

TILE = 8
COLS = 8
ROWS = 4

# Index 0 must stay unused: agb treats palette entry 0 as transparent, and these pages are opaque.
# Fifteen usable entries is the 4bpp ceiling, so the palette is the demo's whole colour budget.
PAL = {
    0: (255, 0, 255),   # never drawn — see above
    1: (0, 0, 0),       # black
    2: (255, 255, 255), # white
    3: (128, 128, 128), # mid grey  — the cafe-wall mortar and the Hermann field
    4: (64, 64, 64),    # dark grey
    5: (192, 192, 192), # light grey
    6: (216, 0, 216),   # magenta   — lilac chaser
    7: (0, 216, 216),   # cyan
    8: (232, 216, 0),   # yellow
    9: (216, 0, 0),     # red
    10: (0, 176, 0),    # green
    11: (0, 0, 216),    # blue
    12: (232, 128, 0),  # orange
    13: (24, 24, 40),   # near-black backdrop
    14: (160, 160, 160),# the "same grey" patch — simultaneous contrast
    15: (96, 96, 96),   # grey between 4 and 3, for the Ouchi field
}


def solid(c):
    return [[c] * TILE for _ in range(TILE)]


def rows(pattern):
    """`pattern` is 8 strings of 8 chars; each char indexes COLOR_KEYS."""
    return [[KEY[ch] for ch in line] for line in pattern]


# Single-char names so the tile art below reads as a picture rather than as a table.
KEY = {'.': 1, '#': 2, 'g': 3, 'd': 4, 'l': 5, 'm': 6, 'c': 7, 'y': 8,
       'r': 9, 'G': 10, 'b': 11, 'o': 12, 'k': 13, 'p': 14, 's': 15}

TILES = []


def add(t):
    TILES.append(t)
    return len(TILES)  # 1-based index, matching tilemap_set8


# ── Row 0: flat fills. The workhorses — every page uses at least two. ────────────────────────────
T_BLACK = add(solid(1))
T_WHITE = add(solid(2))
T_GREY = add(solid(3))
T_DARK = add(solid(4))
T_LIGHT = add(solid(5))
T_PATCH = add(solid(14))
T_FIELD = add(solid(15))
T_BACK = add(solid(13))

# ── Row 1: cafe wall. The mortar is a SINGLE grey row along the top of the tile, so a row of these
# draws a continuous 1px line without a second layer — the line is what makes the illusion work, and
# a wall with black mortar is just a checkerboard.
T_MORTAR_B = add(rows(['gggggggg', '........', '........', '........',
                       '........', '........', '........', '........']))
T_MORTAR_W = add(rows(['gggggggg', '########', '########', '########',
                       '########', '########', '########', '########']))
# Hermann grid: the grey field's intersections, with the illusory-dot spot left white.
T_CROSS = add(rows(['gg####gg', 'gg####gg', '########', '########',
                    '########', '########', 'gg####gg', 'gg####gg']))

# ── Row 1 cont: stripe families. Each tiles seamlessly at 8px so a `bg_scroll` of any amount stays
# continuous — that is the whole basis of the barber pole and the waterfall page.
T_DIAG = add(rows(['....####', '...####.', '..####..', '.####...',
                   '####....', '###....#', '##....##', '#....###']))
T_HSTRIPE = add(rows(['########', '########', '########', '########',
                      '........', '........', '........', '........']))
T_VSTRIPE = add(rows(['####....'] * 8))
T_CHECK4 = add(rows(['####....', '####....', '####....', '####....',
                     '....####', '....####', '....####', '....####']))
T_OUCHI_H = add(rows(['####....', '####....', '####....', '####....',
                      '####....', '####....', '####....', '####....']))
T_OUCHI_V = add(rows(['########', '########', '########', '########',
                      '........', '........', '........', '........']))

# ── Row 2: dots and rings, for the pages the terrain layer would make lumpy at this size.
T_DOT_W = add(rows(['........', '........', '..####..', '..####..',
                    '..####..', '..####..', '........', '........']))
T_DOT_G = add(rows(['gggggggg', 'gggggggg', 'gg####gg', 'gg####gg',
                    'gg####gg', 'gg####gg', 'gggggggg', 'gggggggg']))
# The four quadrant fills a Benham-style disc and the Ouchi inset need at cell granularity.
T_TL = add(rows(['####....', '####....', '####....', '####....',
                 '........', '........', '........', '........']))
T_TR = add(rows(['....####', '....####', '....####', '....####',
                 '........', '........', '........', '........']))
T_BL = add(rows(['........', '........', '........', '........',
                 '####....', '####....', '####....', '####....']))
T_BR = add(rows(['........', '........', '........', '........',
                 '....####', '....####', '....####', '....####']))
# Afterimage plate colours — saturated complements, so the negative afterimage lands on the
# familiar one. Kept as flat tiles because the plate is drawn as blocks.
T_MAGENTA = add(solid(6))
T_CYAN = add(solid(7))
T_YELLOW = add(solid(8))

# ── Row 3: spare. Padding to a full grid keeps the row-major index arithmetic honest —
# `include_background_gfx!` bakes over the image's own 8x8 grid, so a ragged last row would shift
# nothing but is one less thing to reason about when adding a tile later.
while len(TILES) % COLS:
    add(solid(13))

assert len(TILES) <= COLS * ROWS, f"{len(TILES)} tiles will not fit {COLS}x{ROWS}"

img = Image.new('P', (COLS * TILE, ROWS * TILE), 0)
flat = []
for i in range(16):
    flat.extend(PAL[i])
img.putpalette(flat + [0] * (768 - len(flat)))
px = img.load()
for n, t in enumerate(TILES):
    ox, oy = (n % COLS) * TILE, (n // COLS) * TILE
    for y in range(TILE):
        for x in range(TILE):
            px[ox + x, oy + y] = t[y][x]
img.save('pat.png')

names = [k for k, v in sorted(globals().items()) if k.startswith('T_') and isinstance(v, int)]
print(f"pat.png: {len(TILES)} tiles, {COLS}x{ROWS} grid")
for k in sorted(names, key=lambda k: globals()[k]):
    print(f"  {globals()[k]:>3}  {k}")
