#!/usr/bin/env python3
"""Bake the `prismfall` example's art, world and story.

SPECTRA is a puzzle-platformer whose verb is the PALETTE: the world is painted in more colours than
the screen shows at once, your lantern holds four, and what is not in your lens is not there — not
merely invisible, but not solid. So the art here is not "pixel art that happens to have colours in
it"; every tile is authored against a fixed, tiny set of colours, and each colour band gets its own
tile layer so the game can light one at a time.

What comes out (examples/spectra/assets/):
  tileset.png   background: the world atlas, 16x16 tiles, FULLY OPAQUE
  tiles.tsj     which tiles are walls
  rNN.tmj       the twelve rooms, four layers each: three bands plus the world
  hero.png      sheet32: the glass figure
  stalker.png   sheet32: the eye, drawn once per band
and (examples/spectra/src/):
  rooms.tish    the room table, the band cells, and the lens backdrops

⚠️ Rules baked into the numbers below:
  1. `background:` art must be fully opaque — `asset_bg` forced-blanks the screen otherwise. So the
     atlas has no transparent pixels and agb prepends its own transparent colour at index 0.
  2. Never name a palette entry. Which index holds which colour is NONDETERMINISTIC ACROSS BUILDS:
     agb's optimiser pushes the per-tile colour sets through a hash-ordered bin-packer, and it
     assigns per atlas, so two rooms disagree with each other as well. This cost two days; see the
     long note beside LENSES.
  3. A `.tmj` layer's tiles are REMAPPED when `scene:` packs its atlas, so the gid in this file is
     not the gid at runtime. Nothing here may depend on a gid reaching the game.
"""
import json
import os
import sys

from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EX = os.path.join(REPO, "examples", "prismfall")
ASSETS = os.path.join(EX, "assets")

TILE = 16
ATLAS_COLS = 8

# ── THE FOUR COLOURS ────────────────────────────────────────────────────────────────────────────
# HARD REQUIREMENT: at most FOUR colours on screen at any instant. Not four "inks with shading" —
# four values, counted off a screenshot by `scripts/count_screen_colours.py`, sprites included.
#
# That is a Game Boy constraint on Game Boy Advance hardware, and it forces the whole design:
#
#   VOID    the backdrop, and the ONLY one a lens changes (`backdrop()` writes palette 0 entry 0 by
#           name, so it is the one colour on this machine that can be aimed deterministically)
#   INK     every solid surface — stone, walls, floors. ONE value, no bevel, no shading.
#   BAND    whichever colour band is currently real. All three bands are drawn in this SAME index,
#           because two lit bands would otherwise be two colours.
#   ACCENT  hazards, prisms, doors, the exit — AND the HUD text. It is pure white because
#           `hud_text` draws in white and takes no colour argument, so anything the game writes on
#           screen is a fifth colour unless white is already one of the four. Making ACCENT white
#           turns that constraint into the palette's meaning: white is the game TALKING to you —
#           a thing to take, a thing that will kill you, or a word.
#
# ⚠️ THE ATLAS MAY HOLD MORE THAN FOUR; THE SCREEN MAY NOT. A hidden background layer contributes
# nothing, so bands can each have their own tiles as long as only one layer is ever visible. What
# broke the rule before was shading: three greys for stone and two tones per band put eighteen
# colours on screen at once.
VOID = (16, 24, 33)          # backdrop / empty air
INK = (107, 123, 140)        # every solid surface
BAND = (231, 99, 57)         # whichever band is live
ACCENT = (255, 255, 255)     # hazards, prisms, the exit — AND the HUD text

# Aliases kept so the tile painters below read naturally; they are all one of the four.
INK_DK = INK
INK_LT = INK
A_MAIN = BAND
A_DARK = BAND
B_MAIN = BAND
B_DARK = BAND
C_MAIN = BAND
C_DARK = BAND
HAZ = ACCENT
LIT = ACCENT

PALETTE = [VOID, INK, BAND, ACCENT]


# ⚠️ TRANSPARENT, NOT "BACKDROP-COLOURED". A tile pixel painted in a fixed dark colour is a FIFTH
# colour the moment the backdrop changes per lens — the two no longer match. Leaving it transparent
# lets palette 0 through, so every empty pixel in the world is literally the backdrop and recolours
# with the lantern. (The "background art must be fully opaque" rule applies to `background:` assets;
# a `scene:` atlas carries its own transparency, which is how band layers already work.)
# Halftone shading lives in `scripts/pixelart.py` so every generator in this repo can use it — see
# that module for why dither beats both alpha blending and a fifth palette entry on this hardware,
# and for the rule that shading must punch HOLES rather than paint a dark colour.
from pixelart import halftone, ramp   # noqa: E402

def blank(colour=None):
    if colour is None:
        return Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
    return Image.new("RGBA", (TILE, TILE), colour + (255,))


# ── Tiles ───────────────────────────────────────────────────────────────────────────────────────
# Every tile is drawn from the twelve colours above and nothing else. No blending, no anti-aliasing,
# no gradients: a soft edge would introduce a thirteenth colour, and on a 4bpp background a pixel
# with alpha != 255 is a hole rather than a shade.

def tile_solid_stone():
    """Neutral stone. Always there under every lens — the floor you can trust."""
    im = blank(INK)
    d = ImageDraw.Draw(im)
    # A lit cap, then the body shaded by halftone: 25% of the backdrop over the middle and 50% over
    # the base gives three apparent tones out of two colours.
    ramp(im, 0, 3, TILE - 1, TILE - 1, None, steps=(0, 25, 50, 75))
    d.rectangle([0, 0, TILE - 1, 0], fill=ACCENT)
    return im


def tile_stone_top():
    """Stone with a lit cap — the top row of a platform."""
    im = tile_solid_stone()
    d = ImageDraw.Draw(im)
    d.rectangle([0, 0, TILE - 1, 2], fill=INK_LT)
    d.rectangle([0, 3, TILE - 1, 3], fill=INK)
    return im


def tile_phase(main, dark):
    """A PHASE BLOCK: solid under its own lens, absent under the others.

    Drawn as a faceted crystal rather than a plain square, because the game is asking the player to
    read colour as substance — the facets make a band's blocks findable at a glance in a busy room.
    """
    im = blank()
    d = ImageDraw.Draw(im)
    d.rectangle([1, 1, TILE - 2, TILE - 2], fill=main)
    d.line([(1, 1), (TILE - 2, 1)], fill=ACCENT)
    d.line([(1, 1), (1, TILE - 2)], fill=ACCENT)
    # Halftone the lower-right so the block has a lit face and a shaded one — depth from two colours.
    halftone(im, 2, 8, TILE - 2, TILE - 2, None, 25)
    halftone(im, 9, 2, TILE - 2, TILE - 2, None, 25)
    d.point((7, 6), fill=ACCENT)
    d.point((8, 6), fill=ACCENT)
    return im


def tile_phase_oneway(main, dark):
    """A band platform you land on and jump up through. Half height, so it reads as a ledge."""
    im = blank()
    d = ImageDraw.Draw(im)
    d.rectangle([0, 2, TILE - 1, 6], fill=main)
    d.line([(0, 2), (TILE - 1, 2)], fill=ACCENT)
    halftone(im, 0, 5, TILE - 1, 6, None, 50)
    halftone(im, 0, 7, TILE - 1, 8, None, 75)
    return im


def tile_spike():
    """A hazard that is always real — the baseline the band teeth vary from."""
    im = blank()
    d = ImageDraw.Draw(im)
    for x in (0, 8):
        d.polygon([(x + 1, TILE - 1), (x + 4, 3), (x + 7, TILE - 1)], fill=HAZ)
        d.line([(x + 4, 3), (x + 1, TILE - 1)], fill=LIT)
    d.rectangle([0, TILE - 2, TILE - 1, TILE - 1], fill=INK_DK)
    return im


def tile_phase_spike(main, dark):
    """Teeth that only bite while their band is real. Same silhouette, band-coloured."""
    im = blank()
    d = ImageDraw.Draw(im)
    for x in (0, 8):
        d.polygon([(x + 1, TILE - 1), (x + 4, 3), (x + 7, TILE - 1)], fill=main)
        d.line([(x + 4, 3), (x + 1, TILE - 1)], fill=LIT)
        d.line([(x + 4, 3), (x + 7, TILE - 1)], fill=dark)
    return im


def tile_ghost(band):
    """Where a band's geometry sits when that band is NOT lit — a faint outline you can still read.

    ⚠️ IDENTITY BY PATTERN, NOT BY COLOUR. Every band is drawn in the same palette entry (two lit
    bands would be two colours), so a ghost cannot say WHICH band it belongs to with hue. It says it
    with texture instead: band A is sparse dots, band B a horizontal weave, band C a corner frame.
    That is a real constraint doing real work — the same way a Game Boy tells two objects apart.

    These live on the WORLD layer, so they need no extra background and no runtime toggling: the
    band's own layer simply draws the solid block on top when it is lit.
    """
    im = blank()
    d = ImageDraw.Draw(im)
    # A dithered border reads as an outline at a glance while staying obviously insubstantial — the
    # block's footprint without its body. Solid enough to plan a route through, faint enough that
    # nobody mistakes it for something they can stand on.
    d.rectangle([1, 1, TILE - 2, TILE - 2], outline=BAND)
    halftone(im, 0, 0, TILE - 1, TILE - 1, None, 50)      # knock half the outline back out
    if band == 1:
        halftone(im, 4, 4, TILE - 5, TILE - 5, BAND, 25)  # A: a sparse fill
    elif band == 2:
        for y in range(5, TILE - 4, 3):
            halftone(im, 4, y, TILE - 5, y, BAND, 50)     # B: a horizontal weave
    else:
        halftone(im, 6, 6, TILE - 7, TILE - 7, BAND, 75)  # C: a dense core
    return im


def tile_backdrop():
    """Empty air — fully transparent, though the maps leave these cells empty entirely."""
    return blank()


def tile_backdrop_star():
    """Air with a fleck in it, so a big empty room is not a flat field of nothing."""
    im = blank()
    d = ImageDraw.Draw(im)
    d.point((5, 4), fill=INK)
    d.point((12, 11), fill=INK)
    return im


def tile_prism():
    """A charging crystal: stand on it to refill the lantern that WHITE drains."""
    im = blank()
    d = ImageDraw.Draw(im)
    d.polygon([(8, 2), (13, 8), (8, 14), (3, 8)], fill=HAZ)
    d.line([(8, 2), (3, 8)], fill=LIT)
    d.line([(8, 2), (13, 8)], fill=LIT)
    d.point((8, 8), fill=LIT)
    return im


def tile_door(main, dark):
    """A chroma door: a shutter that only OPENS under its own band's lens."""
    im = blank()
    d = ImageDraw.Draw(im)
    d.rectangle([2, 0, TILE - 3, TILE - 1], fill=main)
    d.rectangle([2, 0, 2, TILE - 1], fill=LIT)
    d.rectangle([TILE - 3, 0, TILE - 3, TILE - 1], fill=dark)
    for y in range(2, TILE, 4):
        d.line([(4, y), (TILE - 5, y)], fill=dark)
    return im


def tile_lock_field():
    """A lens-lock field: inside it the lantern will not turn. Hatched, so it reads as a no-go."""
    im = blank()
    d = ImageDraw.Draw(im)
    for i in range(-TILE, TILE, 4):
        d.line([(i, 0), (i + TILE, TILE)], fill=INK_DK)
    return im


def tile_goal():
    """The way out."""
    im = blank()
    d = ImageDraw.Draw(im)
    d.rectangle([3, 1, TILE - 4, TILE - 1], fill=INK_DK)
    d.rectangle([5, 4, TILE - 6, TILE - 1], fill=HAZ)
    d.rectangle([5, 4, 5, TILE - 1], fill=LIT)
    return im


def build_tiles():
    return [
        ("air", tile_backdrop()),
        ("air_star", tile_backdrop_star()),
        ("stone", tile_solid_stone()),
        ("stone_top", tile_stone_top()),
        ("spike", tile_spike()),
        ("prism", tile_prism()),
        ("lock", tile_lock_field()),
        ("goal", tile_goal()),
        ("a_block", tile_phase(A_MAIN, A_DARK)),
        ("a_ledge", tile_phase_oneway(A_MAIN, A_DARK)),
        ("a_spike", tile_phase_spike(A_MAIN, A_DARK)),
        ("a_door", tile_door(A_MAIN, A_DARK)),
        ("b_block", tile_phase(B_MAIN, B_DARK)),
        ("b_ledge", tile_phase_oneway(B_MAIN, B_DARK)),
        ("b_spike", tile_phase_spike(B_MAIN, B_DARK)),
        ("b_door", tile_door(B_MAIN, B_DARK)),
        ("c_block", tile_phase(C_MAIN, C_DARK)),
        ("c_ledge", tile_phase_oneway(C_MAIN, C_DARK)),
        ("c_spike", tile_phase_spike(C_MAIN, C_DARK)),
        ("c_door", tile_door(C_MAIN, C_DARK)),
        ("a_ghost", tile_ghost(1)),
        ("b_ghost", tile_ghost(2)),
        ("c_ghost", tile_ghost(3)),
    ]


def emit_atlas(tiles, path):
    rows = (len(tiles) + ATLAS_COLS - 1) // ATLAS_COLS
    atlas = Image.new("RGBA", (ATLAS_COLS * TILE, rows * TILE), (0, 0, 0, 0))
    for i, (_, im) in enumerate(tiles):
        atlas.paste(im, ((i % ATLAS_COLS) * TILE, (i // ATLAS_COLS) * TILE))
    # alpha hardened to 0/255 — anything between is a hole on hardware, not a shade
    px = atlas.load()
    for y in range(atlas.height):
        for x in range(atlas.width):
            r, g, b, a = px[x, y]
            px[x, y] = (r, g, b, 255) if a >= 128 else (0, 0, 0, 0)
    atlas.save(path)
    used = {px[:3] for px in atlas.getdata() if px[3] > 0}
    extra = used - set(PALETTE)
    if extra:
        raise SystemExit(f"atlas has colours outside the palette: {sorted(extra)}")
    return len(tiles), rows, sorted(used)


# ── The hero ────────────────────────────────────────────────────────────────────────────────────
# A glass figure carrying the lantern, in SPRITE colours — the one thing on screen the lens does not
# touch, and the reason the player never disappears into the room.
GLASS = (214, 232, 245)
GLASS_DK = (120, 150, 178)
EDGE = (26, 30, 44)
LAMP = (255, 214, 110)
LAMP_HOT = (255, 255, 232)

CELL = 32
FEET = 27          # y of the ground line inside the cell
CX = 16


def hero_frame(legs, arm, bob, squash=0, dead=False):
    """One pose. `legs` is (front_dx, back_dx), `arm` the lantern-hand offset, `bob` a body lift."""
    im = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    y = FEET - bob
    if dead:
        d.rectangle([CX - 8, y - 4, CX + 7, y - 1], fill=GLASS_DK, outline=EDGE)
        d.rectangle([CX - 9, y - 3, CX - 7, y - 1], fill=EDGE)
        return im
    hh = 10 - squash
    for dx in legs:
        d.rectangle([CX - 2 + dx, y - 5, CX + dx, y - 1], fill=GLASS_DK)
        d.rectangle([CX - 2 + dx, y - 1, CX + dx, y - 1], fill=EDGE)
    d.rectangle([CX - 4, y - 5 - hh, CX + 3, y - 5], fill=GLASS, outline=EDGE)
    d.rectangle([CX - 3, y - 4 - hh, CX - 2, y - 7], fill=LAMP_HOT)
    hy = y - 6 - hh
    d.rectangle([CX - 3, hy - 6, CX + 2, hy], fill=GLASS, outline=EDGE)
    d.rectangle([CX - 1, hy - 4, CX, hy - 3], fill=EDGE)
    ax, ay = CX + 5, y - 8 - hh // 2 + arm
    d.line([(CX + 3, y - 8 - hh // 2), (ax, ay)], fill=GLASS_DK)
    d.rectangle([ax - 2, ay, ax + 1, ay + 4], fill=LAMP, outline=EDGE)
    d.point((ax - 1, ay + 2), fill=LAMP_HOT)
    d.point((ax, ay + 2), fill=LAMP_HOT)
    return im


def build_hero():
    """The sheet, in the order `CLIPS` in components.tish names them."""
    frames = []
    for b in (0, 1, 1, 0):                                   # 0-3 idle breathe
        frames.append(hero_frame((-3, 2), 0, b))
    for legs, arm, bob in (((-5, 3), 1, 1), ((-3, 3), 0, 2), ((0, 1), -1, 1),
                           ((3, -5), 1, 1), ((3, -3), 0, 2), ((1, 0), -1, 1)):   # 4-9 run
        frames.append(hero_frame(legs, arm, bob))
    frames.append(hero_frame((-2, 2), -2, 3))                # 10 jump
    frames.append(hero_frame((-4, 3), 2, 1))                 # 11 fall
    frames.append(hero_frame((-4, 4), 1, 0, squash=3))       # 12 land
    frames.append(hero_frame((-3, 3), 0, 0, squash=1))       # 13 land recover
    # 14-17 shift: the body refracts through the new colour. Played over whatever the state machine
    # is doing, because turning the lantern is a thing the CHARACTER does.
    for k in range(4):
        f = hero_frame((-3, 2), 0, 1)
        d = ImageDraw.Draw(f)
        w = 2 + k * 3
        d.rectangle([CX - w, FEET - 22, CX + w - 1, FEET - 1], outline=LAMP_HOT)
        frames.append(f)
    frames.append(hero_frame((-4, 4), 3, 0))                 # 18 hurt
    for _ in range(3):                                       # 19-21 dead
        frames.append(hero_frame((0, 0), 0, 0, dead=True))
    sheet = Image.new("RGBA", (CELL, CELL * len(frames)), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        sheet.paste(f, (0, i * CELL))
    return sheet, len(frames)


def build_stalker():
    """A drifting eye, drawn once per band.

    ⚠️ THE BAND COLOURS ARE BAKED INTO THE SHEET. This is a SPRITE, and sprites have their own
    palette banks — nothing the background side does reaches them. So a stalker changing band is a
    `sprite_set_frame`, not a recolour: three sets of four frames, and the game picks the set.
    """
    frames = []
    for main, dark in ((A_MAIN, A_DARK), (B_MAIN, B_DARK), (C_MAIN, C_DARK)):
        for k in range(4):
            im = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
            d = ImageDraw.Draw(im)
            cy = 16 + (0, 1, 0, -1)[k]
            d.ellipse([CX - 7, cy - 6, CX + 6, cy + 5], fill=dark, outline=EDGE)
            d.ellipse([CX - 4, cy - 4, CX + 3, cy + 2], fill=main)
            px = CX - 2 + (0, 1, 2, 1)[k]
            d.rectangle([px, cy - 2, px + 1, cy], fill=EDGE)
            d.point((CX - 9, cy + 3 + k % 2), fill=main)
            d.point((CX + 9, cy - 3 - k % 2), fill=main)
            frames.append(im)
    sheet = Image.new("RGBA", (CELL, CELL * len(frames)), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        sheet.paste(f, (0, i * CELL))
    return sheet, len(frames)


# ── The lenses ──────────────────────────────────────────────────────────────────────────────────
# FOUR COLOURS AT A TIME, and this is where that claim is kept. At any moment the screen carries the
# backdrop, the neutral stone, exactly one band's colour, and the warning gold. The other two bands
# are not dimmed or greyed — their tiles are not drawn at all.
#
# ⚠️ A LENS IS NOT A PALETTE WRITE, and the first two attempts at this example assumed it was.
#
# Attempt 1, the obvious GBA trick: paint the absent bands the backdrop colour and change the whole
# level for twelve palette writes. It WORKS — `terrain_pal_bank` really does rewrite a `scene:` map's
# live bank — and it is unusable, because WHICH INDEX HOLDS WHICH COLOUR IS NONDETERMINISTIC ACROSS
# BUILDS. Same source, two clean builds, the backdrop moved from entry 5 to entry 9; agb's optimiser
# runs the per-tile colour sets through a hash-ordered bin-packer. It also assigns per ATLAS, and
# `scene:` packs one atlas per map, so rooms disagree with each other too. Measuring the order and
# baking a table does not rescue it: the next build invalidates the table.
#
# Attempt 2, rewrite the tiles: `bg_set_tile` does work on a ROM-backed `scene:` layer (via a patch
# list), but the packer REMAPS gids when it builds the atlas, so the number in this file is not the
# number at runtime — and it costs ~310 ticks a cell besides.
#
# What works is neither. EACH BAND IS ITS OWN TILE LAYER, and a lens is `stream_visible(band, on)`:
# one call, against a layer the hardware was already compositing, with no gid and no palette entry
# anywhere in it. The GBA's four background layers are exactly three bands plus the world.
#
# So the only thing a lens changes about the palette is `backdrop()`, which writes palette 0 entry 0
# explicitly and is the one colour call on this machine that means what it says.
LENSES = [
    ("DAWN", 0x201018),      # band A — warm, low sun
    ("DUSK", 0x101426),      # band B — cold violet
    ("VOID", 0x0a1410),      # band C — deep green, almost lightless
    ("WHITE", 0x282830),     # all three at once; the room goes pale and everything is real
]

# ── Tiled files ─────────────────────────────────────────────────────────────────────────────────
# Collision for the NEUTRAL world lives in the tileset: a wall is a wall wherever it is painted.
#
# ⚠️ THE BAND TILES ARE DELIBERATELY NOT DECLARED SOLID HERE. Their solidity is owned by the runtime,
# which sets and clears it as the lens turns. Baking them solid would make them permanent walls that
# the art merely hides, which is the exact bug this whole design exists to avoid.
SOLID_NAMES = ["stone", "stone_top"]


def emit_tsj(path, names, ntiles):
    gid = {n: i + 1 for i, n in enumerate(names)}
    props = {gid[n]: {"walkable": ("bool", False)} for n in SOLID_NAMES}
    rows = (ntiles + ATLAS_COLS - 1) // ATLAS_COLS
    doc = {
        "type": "tileset", "version": "1.10", "tiledversion": "1.10.2",
        "name": "prismfall", "image": "tileset.png",
        "imagewidth": ATLAS_COLS * TILE, "imageheight": rows * TILE,
        "tilewidth": TILE, "tileheight": TILE,
        "columns": ATLAS_COLS, "tilecount": rows * ATLAS_COLS,
        "margin": 0, "spacing": 0,
        "tiles": [
            {"id": g - 1,
             "properties": [{"name": k, "type": t, "value": v}
                            for k, (t, v) in sorted(props[g].items())]}
            for g in sorted(props)
        ],
    }
    with open(path, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")


KINDS = ["player", "prism", "stalker", "pad", "door"]


def emit_tmj(path, w, h, layers, spawns):
    """`layers` is [(name, priority, gid_grid)] in Tiled order; `spawns` is [(kind, col, row, a)]."""
    out, lid = [], 0
    for (name, priority, gids) in layers:
        lid += 1
        out.append({
            "type": "tilelayer", "name": name, "id": lid,
            "x": 0, "y": 0, "width": w, "height": h,
            "opacity": 1, "visible": True,
            "properties": [{"name": "priority", "type": "int", "value": priority}],
            "data": [gids[r][c] for r in range(h) for c in range(w)],
        })
    lid += 1
    out.append({
        "type": "objectgroup", "name": "spawns", "id": lid,
        "x": 0, "y": 0, "opacity": 1, "visible": True, "draworder": "topdown",
        "objects": [
            {"id": i + 1, "name": s[0], "x": s[1] * TILE, "y": s[2] * TILE,
             "width": TILE, "height": TILE, "rotation": 0, "visible": True,
             "properties": [{"name": "kind", "type": "int", "value": KINDS.index(s[0])},
                            {"name": "a", "type": "int", "value": s[3] if len(s) > 3 else 0}]}
            for i, s in enumerate(spawns)
        ],
    })
    doc = {
        "type": "map", "version": "1.10", "tiledversion": "1.10.2",
        "orientation": "orthogonal", "renderorder": "right-down", "infinite": False,
        "width": w, "height": h, "tilewidth": TILE, "tileheight": TILE,
        "nextlayerid": lid + 1, "nextobjectid": len(spawns) + 1,
        "tilesets": [{"firstgid": 1, "source": "tiles.tsj"}],
        "layers": out,
    }
    with open(path, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")


def grid(w, h, fill):
    return [[fill for _ in range(w)] for _ in range(h)]


# ── THE FACILITY ────────────────────────────────────────────────────────────────────────────────
# ONE connected map, not a gauntlet of rooms. 96x64 tiles = 1536x1024 px, which is 6.4 screens wide
# and 6.4 tall, and the camera scrolls freely through it.
#
# It is drawn as ASCII for the same reason spectra's rooms were: a world you cannot read is a world
# you cannot tune, and the gating here is a claim about REACHABILITY that has to be checked by eye
# and then by machine (see `check_reachable`).
#
# LEGEND — world
#   .  air          *  fleck        #  stone        ^  spike (always lethal)
#   A B C  phase block, band A/B/C      a b c  phase ledge
#   1 2 3  phase teeth                  X Y Z  chroma door
#   =  lens-lock field
# LEGEND — things
#   P  player start        D  the DUSK lens      V  the VOID lens      W  the WHITE prism
#   o  lumen prism         L  a data-log         S  save/recharge      G  the way out
#
# ⚠️ THE GATING RULE: you start owning only DAWN, so every `B`/`b`/`Y` is a wall until the DUSK lens
# is found, and every `C`/`c`/`Z` until VOID. That is the entire ability-gate system — there is no
# door type that checks a flag. A gate re-reads as an opportunity the moment the lens is picked up,
# which is the whole feeling the genre runs on.
# The facility is BUILT, not hand-drawn, and that is a correction rather than a preference. Drawn by
# hand, the gates leaked: `check_reachable` proved you could walk to the VOID lens, the WHITE prism
# and the exit with only the starting lens, because a barrier drawn as a few band blocks in a wide
# open room is scenery, not a wall. Composing it guarantees the one property the genre depends on —
# each zone is sealed from the next by a barrier of exactly one band, floor to ceiling.
#
# Four zones, left to right, each sealed by the lens you have not got yet:
#
#   ZONE 1  start, the first logs, and the DUSK lens        (reachable with DAWN alone)
#   ══ band B wall ══  needs DUSK
#   ZONE 2  the blue halls, and the VOID lens
#   ══ band C wall ══  needs VOID
#   ZONE 3  the green deep, and the WHITE prism
#   ══ the SPAN  ══    a chasm bridged by A, B and C blocks at once — only WHITE crosses it
#   ZONE 4  the way out
W_COLS_N = 112
W_ROWS_N = 40
ZONE_W = 22          # interior width of a zone
WALL_W = 2           # thickness of a band barrier


def blank_world():
    g = [["." for _ in range(W_COLS_N)] for _ in range(W_ROWS_N)]
    for c in range(W_COLS_N):
        g[0][c] = "#"
        g[W_ROWS_N - 1][c] = "#"
    for r in range(W_ROWS_N):
        g[r][0] = "#"
        g[r][W_COLS_N - 1] = "#"
    return g


def hline(g, r, c0, c1, ch="#"):
    for c in range(max(1, c0), min(W_COLS_N - 1, c1 + 1)):
        g[r][c] = ch


def vline(g, c, r0, r1, ch="#"):
    for r in range(max(1, r0), min(W_ROWS_N - 1, r1 + 1)):
        g[r][c] = ch


def barrier(g, c0, ch):
    """A full-height wall of one band. THIS is the gate — nothing else in the map is."""
    for c in range(c0, c0 + WALL_W):
        vline(g, c, 1, W_ROWS_N - 2, ch)


# Platforms rise every RISE rows, alternating left and right with an overlap in the middle, so every
# shelf is reachable from the one below at a jump the controller can actually make.
#
# ⚠️ SPACING IS 3, NOT 4, AND THAT IS THE DIFFERENCE BETWEEN A LEVEL AND A SEALED BOX. The
# reachability model allows a 4-tile rise, so shelves 4 apart are reachable only at the exact limit
# with perfect overlap — and the first draft, built on 4, checked out as UNREACHABLE in two zones.
# Three leaves the player somewhere to be wrong.
RISE = 3
FLOOR_R = 34


def zone_stack(g, c0, width, mats, tiers):
    """A climbable staircase inside one zone. Returns the (col,row) of each shelf's standing spot.

    `mats` is the list of materials the shelves cycle through, and it is why every screen has colour
    on it. The first version built each zone out of ONE material — zone 1 entirely grey stone — so
    turning the lantern in the opening area changed the HUD and nothing else, and the whole mechanic
    read as broken. A zone has to show at least two bands for the lens to be visibly doing something.
    """
    hline(g, FLOOR_R, c0, c0 + width, "#")
    spots = [(c0 + 2, FLOOR_R - 1)]
    for i in range(tiers):
        r = FLOOR_R - RISE * (i + 1)
        left = i % 2 == 0
        a0, a1 = (0, width // 2 + 3) if left else (width // 2 - 3, width)
        hline(g, r, c0 + a0, c0 + a1, mats[i % len(mats)])
        spots.append((c0 + (a1 - 2 if left else a0 + 2), r - 1))
    return spots


def band_field(g, c0, width, bands, seed):
    """Fill a zone's empty air with band structure, so turning the lantern REBUILDS the view.

    ⚠️ WITHOUT THIS THE MECHANIC IS INVISIBLE. Measured before it existed: a lens switch changed
    **0.3% of the screen** — the zones were stone shelves with the odd band ledge, so the signature
    verb of the game did almost nothing you could see. Spectra's rooms are dense with band blocks and
    read instantly; a big exploration map has to work harder for the same effect, not less.

    The blocks are placed in the MIDDLE of the gap between shelves, never against a walking surface,
    so they decorate and obstruct without sealing a route. `check_reachable` is what proves that.
    """
    n = 0
    for i in range(1, 12):
        r = FLOOR_R - RISE * i + 1          # mid-gap: one row below a shelf, one above the next
        if r < 2:
            break
        for k in range(width):
            c = c0 + k
            if (c * 7 + r * 5 + seed) % 3 != 0:
                continue
            if g[r][c] != ".":
                continue
            # never directly above a surface the player walks on
            if r + 1 < W_ROWS_N and g[r + 1][c] != ".":
                continue
            g[r][c] = bands[(c + r) % len(bands)]
            n += 1
    return n


def build_world():
    g = blank_world()
    z = [1 + i * (ZONE_W + WALL_W) for i in range(4)]

    # Each zone's shelves cycle through stone plus the bands the player owns by the time they arrive,
    # so the lantern visibly changes the world on every screen — including the first one.
    s1 = zone_stack(g, z[0], ZONE_W, ["#", "a", "#", "b"], 9)
    s2 = zone_stack(g, z[1], ZONE_W, ["#", "a", "#", "b"], 9)
    s3 = zone_stack(g, z[2], ZONE_W, ["#", "b", "#", "c"], 9)

    # Band structure in the open air of every zone, so a lens switch visibly rebuilds the view
    # rather than toggling a ledge or two. Zone 1 shows the two lenses you own there, and so on.
    band_field(g, z[0], ZONE_W, ["A", "B"], 0)
    band_field(g, z[1], ZONE_W, ["A", "B"], 2)
    band_field(g, z[2], ZONE_W, ["B", "C"], 4)

    # The gates. DOORS, not blocks — a block is solid only while its band is lit, so a wall of them
    # is passable to someone who has NOT got the lens, which is exactly backwards for a gate.
    barrier(g, z[0] + ZONE_W, "Y")          # zone 1 -> 2: a DUSK door
    barrier(g, z[1] + ZONE_W, "Z")          # zone 2 -> 3: a VOID door

    # ZONE 3 -> 4 IS "THE SPAN", the only gate WHITE opens.
    #
    # WHITE cannot be a single tile: the engine ORs the lit bands, so a cell is solid if ANY lit band
    # has a block there and no tile can mean "solid only when all three are lit". The gate is a
    # SEQUENCE instead — a walkway running AAABBBCCC, so one lens gives three steps and then a
    # six-column hole, wider than the five-tile jump. Light all three and it is continuous.
    #
    # ⚠️ THE RUN LENGTH IS LOAD-BEARING AND WAS WRONG TWICE. ABCABC leaves same-band steps three
    # apart — jumpable. AABBCC leaves them five apart — still jumpable, and the checker caught it.
    # Three-long runs put the gap at six, which is the first value the jump cannot cross.
    span_c = z[2] + ZONE_W
    SPAN_W = 18
    for c in range(span_c, span_c + SPAN_W):
        vline(g, c, 1, 30, "#")
        vline(g, c, 31, 35, ".")
        vline(g, c, 36, W_ROWS_N - 2, "#")
        g[35][c] = "^"                      # the floor of the chasm bites, so it is not a route
    for i in range(SPAN_W):
        g[33][span_c + i] = "ABC"[(i // 3) % 3]

    z4 = span_c + SPAN_W
    s4 = zone_stack(g, z4, W_COLS_N - 2 - z4, ["#", "c", "#", "a"], 8)
    band_field(g, z4, W_COLS_N - 2 - z4, ["C", "A"], 1)

    def put(ch, spot):
        c, r = spot
        g[r][c] = ch

    # Everything sits ON a shelf the staircase already proved reachable, rather than at a hand-picked
    # coordinate that the next tweak to the geometry quietly strands.
    put("P", s1[0]); put("S", (s1[0][0] + 4, s1[0][1]))
    put("L", s1[2]); put("L", s1[5]); put("o", s1[4])   # log ids are assigned in map order below
    # ⚠️ THE SECOND LENS IS THE FIRST THING YOU FIND, and that is a deliberate correction. It used to
    # sit at the top of zone 1, which meant several minutes of play before L/R did ANYTHING — the
    # game's entire signature mechanic absent from the opening. Worse, the opening area was built
    # only from stone and band A (the lens you start with), so even once you had DUSK there was
    # nothing on that screen for it to change. A game about turning the lantern has to hand you two
    # lenses and something two-coloured to look at within the first few seconds.
    put("D", (s1[0][0] + 8, s1[0][1]))       # the DUSK lens, eight tiles along the START FLOOR

    put("L", s2[1]); put("S", s2[4]); put("o", s2[6])
    put("V", s2[9])                          # the VOID lens, top of zone 2

    put("L", s3[2]); put("o", s3[5]); put("S", s3[7])
    put("W", s3[9])                          # the WHITE prism, top of zone 3

    put("L", s4[1]); put("L", s4[5])
    put("G", s4[8])                          # the way out
    return ["".join(r) for r in g]


WORLD = build_world()

CHARS_W = {
    ".": "air", "*": "air_star", "#": "stone", "^": "spike", "=": "lock",
    "A": "a_block", "a": "a_ledge", "1": "a_spike", "X": "a_door",
    "B": "b_block", "b": "b_ledge", "2": "b_spike", "Y": "b_door",
    "C": "c_block", "c": "c_ledge", "3": "c_spike", "Z": "c_door",
}
# Things become SPAWNS; the cell under them is plain air.
THINGS = {"P": "player", "D": "lens_dusk", "V": "lens_void", "W": "lens_white",
          "o": "prism", "L": "log", "S": "save", "G": "goal"}
KINDS = ["player", "lens_dusk", "lens_void", "lens_white", "prism", "log", "save", "goal"]

CHAR_BAND = {"A": 1, "a": 1, "1": 1, "X": 1,
             "B": 2, "b": 2, "2": 2, "Y": 2,
             "C": 3, "c": 3, "3": 3, "Z": 3}
LEDGE = set("abc")
TEETH = set("123")
DOORS = set("XYZ")

CHUNK = 16          # must match packages/chroma.tish


def parse_world():
    """World grid + three band grids + spawns + band cells SORTED BY CHUNK.

    The sort is what lets the runtime slice straight to a chunk's cells: `chromaChunk` takes a
    [start,end) range rather than searching the list, so bringing one chunk up to date after a lens
    switch costs only that chunk's cells.
    """
    h, w = len(WORLD), len(WORLD[0])
    for i, r in enumerate(WORLD):
        if len(r) != w:
            raise SystemExit(f"world row {i} is {len(r)} wide, expected {w}")
    if w > 128 or h > 128:
        raise SystemExit("the packed cell word holds 7 bits per axis; keep the map within 128x128")
    g = grid(w, h, 0)
    bands = [grid(w, h, 0) for _ in range(3)]
    spawns, cells = [], []
    for r, line in enumerate(WORLD):
        for c, ch in enumerate(line):
            if ch in THINGS:
                spawns.append((THINGS[ch], c, r, 0))
                g[r][c] = 0          # empty, so the backdrop shows through — see below
                continue
            name = CHARS_W.get(ch)
            if name is None:
                raise SystemExit(f"unknown world character {ch!r} at {c},{r}")
            if ch in CHAR_BAND:
                bands[CHAR_BAND[ch] - 1][r][c] = GID[name]
                g[r][c] = GID["air"]
                kind = (1 if ch in LEDGE else 2 if ch in TEETH else 3 if ch in DOORS else 0)
                cells.append((c, r, CHAR_BAND[ch], kind))
            else:
                g[r][c] = GID[name]
    chunk_cols = (w + CHUNK - 1) // CHUNK
    cells.sort(key=lambda t: ((t[1] // CHUNK) * chunk_cols + (t[0] // CHUNK), t[1], t[0]))
    packed = [c | (r << 7) | (b << 14) | (k << 16) for (c, r, b, k) in cells]
    chunk_of = [(r // CHUNK) * chunk_cols + (c // CHUNK) for (c, r, _, _) in cells]
    return g, bands, spawns, packed, chunk_of, chunk_cols, w, h


# ── The story ───────────────────────────────────────────────────────────────────────────────────
# Told entirely through logs you find. No talking heads, no cutscene that stops the game: each one is
# a short entry from the last crew of the facility, and each one also tells you where to go — which
# is the only way a wordless map teaches direction without a quest marker.
LOGS = [
    ("OBSERVATORY", [
        "We built the lantern to see",
        "the whole spectrum at once.",
        "It worked. That was the error.",
    ]),
    ("MAINTENANCE", [
        "Three bands, three lenses.",
        "Keep only one lit and the",
        "structure holds. Keep them",
        "all lit and it does not.",
    ]),
    ("DR. VEY", [
        "The DUSK lens is below the",
        "east shaft. I left it where",
        "the blue steps end.",
    ]),
    ("DR. VEY", [
        "Whatever you do, do not",
        "open WHITE inside the ring.",
        "Everything becomes real",
        "in there. Everything.",
    ]),
    ("LAST ENTRY", [
        "If you are reading this, you",
        "found a lens I could not.",
        "Take it up. Turn it off",
        "behind you. Go home.",
    ]),
    ("UNSIGNED", [
        "It is not dark in the void",
        "band. We are simply not",
        "in it.",
    ]),
]


# ── Reachability ────────────────────────────────────────────────────────────────────────────────
def check_reachable(spawns, chunk_cols):
    """Prove the gating actually works: with only DAWN you must reach the DUSK lens and no further.

    ⚠️ THIS IS THE CHECK THAT MATTERS AND IT IS THE ONE NOBODY WRITES. A metroidvania map's whole claim is
    "you cannot get there yet, and later you can", and both halves fail silently: a gate placed one
    tile too low is a sequence break nothing reports, and a gate with no route behind it is a
    softlock the player finds an hour in. Eyeballing ASCII does not catch either.
    
    The model is deliberately generous — a flood fill that walks and falls and climbs anything up to
    a 4-tile jump — so it OVER-estimates what the player can do. An over-estimate is the safe
    direction: if this says the VOID lens is unreachable with only DAWN, it truly is.
    """
    h, w = len(WORLD), len(WORLD[0])
    # A spike is not a corridor. Without this the flood fill happily walks the floor of a lethal
    # chasm and reports a route no player could take.
    solid_ch = set("#^")
    band_ch = {1: set("AaX1"), 2: set("BbY2"), 3: set("CcZ3")}

    door_ch = {1: "X", 2: "Y", 3: "Z"}
    block_ch = {1: "Aa1", 2: "Bb2", 3: "Cc3"}

    def solid(c, r, lit):
        """Stops movement AND holds the player up."""
        if not (0 <= c < w and 0 <= r < h):
            return True
        ch = WORLD[r][c]
        if ch == "#" or ch == "^":
            return True
        for band, chars in block_ch.items():
            if ch in chars and band in lit:
                return True            # a phase block is matter only while its band is lit
        for band, dch in door_ch.items():
            if ch == dch and band not in lit:
                return True            # a chroma door is shut in every lens but its own
        return False

    def lethal(c, r):
        return 0 <= c < w and 0 <= r < h and WORLD[r][c] == "^"

    def standable(c, r, lit):
        """Somewhere the player can be, alive: open, not on spikes, with solid ground beneath.

        ⚠️ SOLID AND LETHAL ARE SEPARATE AND CONFLATING THEM IS SUBTLE POISON. Treating "above a
        spike" as simply blocked made the spike floor act as GROUND one row up — so the player could
        stand inside a band block that was not lit, walk the whole WHITE span in any lens, and the
        checker reported a leak it could not explain. Spikes stop you; they do not hold you up.
        """
        if solid(c, r, lit) or lethal(c, r + 1):
            return False
        return solid(c, r + 1, lit)

    def blocked(c, r, lit):
        """Cannot be entered at all — for jump arcs, which pass through open air."""
        return solid(c, r, lit) or lethal(c, r)

    def configs(owned):
        """Every lit-set the player can select: each owned band, plus all three under WHITE."""
        out = [frozenset({b}) for b in sorted(owned) if b in (1, 2, 3)]
        if 4 in owned:
            out.append(frozenset({1, 2, 3}))
        return out

    JUMP_UP, JUMP_OUT = 4, 5      # generous: the real controller clears less

    def fall_to(c, r, lit):
        """Drop straight down from (c,r) to the first standable cell, or None.

        ⚠️ THE LANDING IS RE-CHECKED. Stopping as soon as the cell below is solid is not enough:
        the cell below a lethal spike floor IS solid, so an unchecked landing puts the player
        standing on spikes — and that let the fill walk the whole length of the WHITE span in any
        lens, which is the last leak the checker reported and the hardest to see.
        """
        if blocked(c, r, lit):
            return None
        while r + 1 < h and not solid(c, r + 1, lit):
            r += 1
        return (c, r) if standable(c, r, lit) else None

    def reach(owned):
        """BFS over (standing position, lit-config), WITH GRAVITY.

        ⚠️ THE FIRST VERSION OF THIS HAD NO GRAVITY and let the player drift sideways through open
        air, which made the whole interior one connected blob and reported leaks that were not real
        — and, worse, would have hidden the ones that were. A reachability check that is too
        generous is not conservative, it is useless: it cannot tell a wall from a room.

        Here the player only moves along ground, or through a jump arc bounded by JUMP_UP/JUMP_OUT
        and then a fall. Still an over-estimate of the real controller, which is the safe direction.
        """
        sc, sr = next((c, r) for (k, c, r, _) in spawns if k == "player")
        cfgs = configs(owned)
        seen, stack = set(), []
        for cfg in cfgs:
            st = fall_to(sc, sr, cfg)
            if st:
                stack.append((st[0], st[1], cfg))
        while stack:
            c, r, lit = stack.pop()
            if (c, r, lit) in seen:
                continue
            seen.add((c, r, lit))
            # turn the lantern where you stand, if the new configuration does not enclose you
            for other in cfgs:
                if other is not lit and not blocked(c, r, other):
                    landed = fall_to(c, r, other)
                    if landed:
                        stack.append((landed[0], landed[1], other))
            # walk one step, then settle
            for dc in (-1, 1):
                if not blocked(c + dc, r, lit):
                    landed = fall_to(c + dc, r, lit)
                    if landed:
                        stack.append((landed[0], landed[1], lit))
            # Jump: rise up to JUMP_UP, travel out up to JUMP_OUT, then fall.
            #
            # ⚠️ THE HORIZONTAL PATH IS CHECKED, NOT JUST THE LANDING. Without this the model
            # teleports through anything narrower than JUMP_OUT — it cheerfully jumped straight
            # through the two-tile-thick DUSK door and reported the entire facility reachable from
            # the first screen. A wall thinner than the jump is not a wall to a checker that only
            # looks at where you land.
            for up in range(1, JUMP_UP + 1):
                if blocked(c, r - up, lit):
                    break
                for step in (1, -1):
                    for dist in range(1, JUMP_OUT + 1):
                        tc = c + step * dist
                        if blocked(tc, r - up, lit):
                            break          # the arc is interrupted; nothing further out is reachable
                        landed = fall_to(tc, r - up, lit)
                        if landed:
                            stack.append((landed[0], landed[1], lit))
        return {(c, r) for (c, r, _) in seen}

    where = {k: (c, r) for (k, c, r, _) in spawns}
    out, fails = [], 0
    stages = [("DAWN",       {1},          "lens_dusk",  ["lens_void", "lens_white", "goal"]),
              ("+DUSK",      {1, 2},       "lens_void",  ["lens_white", "goal"]),
              ("+VOID",      {1, 2, 3},    "lens_white", ["goal"]),
              ("+WHITE",     {1, 2, 3, 4}, "goal",       [])]
    for name, owned, must, must_not in stages:
        seen = reach(owned)
        def touched(pt):
            c, r = pt
            return any((c + dc, r + dr) in seen
                       for dc in (-1, 0, 1) for dr in (-1, 0, 1, 2))
        if must in where:
            ok = touched(where[must])
            out.append(f"    {name:<10} reaches {must:<11} {'ok' if ok else 'UNREACHABLE'}")
            fails += 0 if ok else 1
        for mn in must_not:
            if mn in where and touched(where[mn]):
                out.append(f"    {name:<10} can ALREADY reach {mn} — the gate leaks")
                fails += 1
    return out, fails


def emit_story_tish(path, logs):
    body = [
        "// GENERATED by scripts/gen_prismfall.py — the facility's logs.",
        "//",
        "// The whole story, told by the place rather than at the player. Each entry also points",
        "// somewhere, because a map with no quest marker teaches direction only through its text.",
        "",
        f"export let LOGS: i32 = {len(logs)}",
        "export let LOG_TITLE: string[] = [" + ", ".join(json.dumps(t) for t, _ in logs) + "]",
        "// Lines are flat with an index, because a tish array-of-arrays is a boxed Value per row.",
        "export let LOG_START: i32[] = [",
    ]
    starts, lines = [0], []
    for _, ls in logs:
        lines.extend(ls)
        starts.append(len(lines))
    body.append("  " + ", ".join(str(s) for s in starts))
    body += ["]", "export let LOG_LINE: string[] = ["]
    for ln in lines:
        body.append("  " + json.dumps(ln) + ",")
    body[-1] = body[-1].rstrip(",")
    body += ["]", ""]
    open(path, "w").write("\n".join(body) + "\n")
    return len(lines)


def emit_pickups(body, spawns):
    """Every touchable thing as a packed word, NOT as an entity.

    ⚠️ A STATIC PICKUP SHOULD NOT BE AN ENTITY. Fifteen collider entities cost ~100 ticks each per
    frame — about 1,500 of a 4,389-tick budget — and took this game from 60fps to 40, for objects
    that never move and do nothing until touched. Comparing the player's tile against a flat table
    of fifteen words is ~30 ticks. Entities are for things that live and act; a door handle is a
    coordinate.

    Packed: col | row<<7 | kind<<14 | arg<<17.
    """
    kinds = {"lens_dusk": 1, "lens_void": 2, "lens_white": 3,
             "prism": 4, "log": 5, "save": 6, "goal": 7}
    out = []
    for (k, c, r, a) in spawns:
        if k in kinds:
            out.append(c | (r << 7) | (kinds[k] << 14) | (a << 17))
    body += ["",
             "// Touchable things, packed: col | row<<7 | kind<<14 | arg<<17.",
             "//   kind 1 DUSK lens, 2 VOID lens, 3 WHITE prism, 4 lumen prism, 5 log, 6 save, 7 exit",
             "// NOT entities — see the note in the generator for why that matters at 60fps.",
             f"export let W_PICKUPS: i32 = {len(out)}",
             "export let W_PICKUP: i32[] = [" + ", ".join(str(v) for v in out) + "]"]
    return body


def emit_world_tish(path, packed, chunk_of, chunk_cols, w, h, nchunks, spawns):
    body = [
        "// GENERATED by scripts/gen_prismfall.py — do not edit; edit the ASCII world there.",
        "//",
        "// The facility's band cells, SORTED BY CHUNK. The runtime slices straight to a chunk's",
        "// cells rather than searching, which is what lets a lens switch cost one chunk of collision",
        "// instead of the whole map — see the CHUNKS note in packages/chroma.tish.",
        "",
        f"export let W_COLS: i32 = {w}",
        f"export let W_ROWS: i32 = {h}",
        f"export let W_CHUNK_COLS: i32 = {chunk_cols}",
        f"export let W_CHUNKS: i32 = {nchunks}",
        "",
        "// Packed: col | row<<7 | band<<14 | kind<<16.",
        "export let W_CELL: i32[] = [",
    ]
    for i in range(0, len(packed), 12):
        body.append("  " + ", ".join(str(v) for v in packed[i:i + 12]) + ",")
    if body[-1].endswith(","):
        body[-1] = body[-1].rstrip(",")
    body += ["]", "",
             "// Which chunk each cell belongs to, same order — the game replays this to tell chroma",
             "// where each chunk's slice ends.",
             "export let W_CELL_CHUNK: i32[] = ["]
    for i in range(0, len(chunk_of), 20):
        body.append("  " + ", ".join(str(v) for v in chunk_of[i:i + 20]) + ",")
    if body[-1].endswith(","):
        body[-1] = body[-1].rstrip(",")
    body += ["]"]
    body = emit_pickups(body, spawns)
    body += [""]
    open(path, "w").write("\n".join(body) + "\n")


def main():
    global GID
    os.makedirs(ASSETS, exist_ok=True)
    os.makedirs(os.path.join(EX, "src"), exist_ok=True)
    tiles = build_tiles()
    n, rows, used = emit_atlas(tiles, os.path.join(ASSETS, "tileset.png"))
    names = [t[0] for t in tiles]
    GID = {nm: i + 1 for i, nm in enumerate(names)}
    emit_tsj(os.path.join(ASSETS, "tiles.tsj"), names, n)

    # The hero is Luis Zuno's CC0 GothicVania sheet (the same one examples/metroidvania uses),
    # copied in as `assets/hero_sheet.png`. It replaced AI-generated art that looked good at 1024px
    # and baked to mush at 32x32 — the size GBA sprites are DRAWN at, not downscaled to.
    hero_path = os.path.join(ASSETS, "hero_sheet.png")
    if not os.path.exists(hero_path):
        raise SystemExit(f"missing {hero_path} — bake the hero sheet first (see README)")
    nframes = Image.open(hero_path).width // 32

    g, bands, spawns, packed, chunk_of, chunk_cols, w, h = parse_world()
    nchunks = chunk_cols * ((h + CHUNK - 1) // CHUNK)
    # Layer order is load-bearing: the packer sorts by (priority, Reverse(index)) and the runtime
    # indexes `stream_visible` by the emitted order, so World FIRST here makes band A layer 0.
    li = 0
    for i, (k, c, r, a) in enumerate(spawns):
        if k == "log":
            spawns[i] = (k, c, r, li % len(LOGS))
            li += 1
    emit_tmj(os.path.join(ASSETS, "facility.tmj"), w, h,
             [("World", 2, g), ("BandC", 2, bands[2]),
              ("BandB", 2, bands[1]), ("BandA", 2, bands[0])], spawns)
    emit_world_tish(os.path.join(EX, "src", "world.tish"),
                    packed, chunk_of, chunk_cols, w, h, nchunks, spawns)
    nlines = emit_story_tish(os.path.join(EX, "src", "story.tish"), LOGS)

    kinds = {}
    for k, _, _, _ in spawns:
        kinds[k] = kinds.get(k, 0) + 1
    print(f"facility.tmj: {w}x{h} tiles, {len(packed)} band cells in {nchunks} chunks")
    print(f"  spawns: {kinds}")
    print(f"tileset.png: {n} tiles, {len(used)} colours · hero_sheet.png: {nframes} frames")
    print(f"story.tish: {len(LOGS)} logs, {nlines} lines")
    print("reachability:")
    lines, fails = check_reachable(spawns, chunk_cols)
    for l in lines:
        print(l)
    if fails:
        raise SystemExit(f"  {fails} gating problem(s) — fix the world before shipping it")


if __name__ == "__main__":
    main()
