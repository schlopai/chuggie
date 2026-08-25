#!/usr/bin/env python3
"""Bake the `spectra` example's art and levels.

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

from PIL import Image, ImageDraw

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EX = os.path.join(REPO, "examples", "spectra")
ASSETS = os.path.join(EX, "assets")

TILE = 16
ATLAS_COLS = 8

# ── The palette, as authored ────────────────────────────────────────────────────────────────────
# Twelve colours, and each one means exactly one thing. They are what the tiles are drawn in, and —
# unlike the first two versions of this example — they are never rewritten at runtime.
VOID = (20, 26, 36)          # empty air
INK = (107, 122, 143)        # neutral stone — always present, never phases
INK_DK = (58, 70, 88)
INK_LT = (159, 176, 196)
A_MAIN = (224, 96, 58)       # band A — DAWN
A_DARK = (143, 51, 32)
B_MAIN = (79, 143, 208)      # band B — DUSK
B_DARK = (38, 80, 127)
C_MAIN = (87, 178, 106)      # band C — VOID
C_DARK = (43, 107, 60)
HAZ = (240, 208, 96)         # hazards, prisms and the exit — the one warning colour
LIT = (255, 255, 255)        # highlights

PALETTE = [VOID, INK, INK_DK, INK_LT, A_MAIN, A_DARK, B_MAIN, B_DARK, C_MAIN, C_DARK, HAZ, LIT]


def blank(colour=VOID):
    return Image.new("RGBA", (TILE, TILE), colour + (255,))


# ── Tiles ───────────────────────────────────────────────────────────────────────────────────────
# Every tile is drawn from the twelve colours above and nothing else. No blending, no anti-aliasing,
# no gradients: a soft edge would introduce a thirteenth colour, and on a 4bpp background a pixel
# with alpha != 255 is a hole rather than a shade.

def tile_solid_stone():
    """Neutral stone. Always there under every lens — the floor you can trust."""
    im = blank(INK)
    d = ImageDraw.Draw(im)
    d.rectangle([0, 0, TILE - 1, 0], fill=INK_LT)
    d.rectangle([0, 0, 0, TILE - 1], fill=INK_LT)
    d.rectangle([0, TILE - 1, TILE - 1, TILE - 1], fill=INK_DK)
    d.rectangle([TILE - 1, 0, TILE - 1, TILE - 1], fill=INK_DK)
    # A little masonry so a wall of these does not read as one flat slab.
    d.rectangle([4, 5, 5, 6], fill=INK_DK)
    d.rectangle([10, 9, 11, 10], fill=INK_DK)
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
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    d.rectangle([1, 1, TILE - 2, TILE - 2], fill=main)
    d.line([(1, 1), (TILE - 2, 1)], fill=LIT)
    d.line([(1, 1), (1, TILE - 2)], fill=LIT)
    d.line([(1, TILE - 2), (TILE - 2, TILE - 2)], fill=dark)
    d.line([(TILE - 2, 1), (TILE - 2, TILE - 2)], fill=dark)
    d.polygon([(8, 4), (12, 8), (8, 12), (4, 8)], outline=dark)
    d.point((8, 7), fill=LIT)
    d.point((8, 8), fill=LIT)
    return im


def tile_phase_oneway(main, dark):
    """A band platform you land on and jump up through. Half height, so it reads as a ledge."""
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    d.rectangle([0, 2, TILE - 1, 6], fill=main)
    d.line([(0, 2), (TILE - 1, 2)], fill=LIT)
    d.line([(0, 6), (TILE - 1, 6)], fill=dark)
    for x in range(1, TILE, 4):
        d.point((x, 4), fill=dark)
    return im


def tile_spike():
    """A hazard that is always real — the baseline the band teeth vary from."""
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    for x in (0, 8):
        d.polygon([(x + 1, TILE - 1), (x + 4, 3), (x + 7, TILE - 1)], fill=HAZ)
        d.line([(x + 4, 3), (x + 1, TILE - 1)], fill=LIT)
    d.rectangle([0, TILE - 2, TILE - 1, TILE - 1], fill=INK_DK)
    return im


def tile_phase_spike(main, dark):
    """Teeth that only bite while their band is real. Same silhouette, band-coloured."""
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    for x in (0, 8):
        d.polygon([(x + 1, TILE - 1), (x + 4, 3), (x + 7, TILE - 1)], fill=main)
        d.line([(x + 4, 3), (x + 1, TILE - 1)], fill=LIT)
        d.line([(x + 4, 3), (x + 7, TILE - 1)], fill=dark)
    return im


def tile_backdrop():
    """Empty air."""
    return blank(VOID)


def tile_backdrop_star():
    """Air with a fleck in it, so a big empty room is not a flat field of nothing."""
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    d.point((5, 4), fill=INK_DK)
    d.point((12, 11), fill=INK_DK)
    return im


def tile_prism():
    """A charging crystal: stand on it to refill the lantern that WHITE drains."""
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    d.polygon([(8, 2), (13, 8), (8, 14), (3, 8)], fill=HAZ)
    d.line([(8, 2), (3, 8)], fill=LIT)
    d.line([(8, 2), (13, 8)], fill=LIT)
    d.point((8, 8), fill=LIT)
    return im


def tile_door(main, dark):
    """A chroma door: a shutter that only OPENS under its own band's lens."""
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    d.rectangle([2, 0, TILE - 3, TILE - 1], fill=main)
    d.rectangle([2, 0, 2, TILE - 1], fill=LIT)
    d.rectangle([TILE - 3, 0, TILE - 3, TILE - 1], fill=dark)
    for y in range(2, TILE, 4):
        d.line([(4, y), (TILE - 5, y)], fill=dark)
    return im


def tile_lock_field():
    """A lens-lock field: inside it the lantern will not turn. Hatched, so it reads as a no-go."""
    im = blank(VOID)
    d = ImageDraw.Draw(im)
    for i in range(-TILE, TILE, 4):
        d.line([(i, 0), (i + TILE, TILE)], fill=INK_DK)
    return im


def tile_goal():
    """The way out."""
    im = blank(VOID)
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
    ]


def emit_atlas(tiles, path):
    rows = (len(tiles) + ATLAS_COLS - 1) // ATLAS_COLS
    atlas = Image.new("RGBA", (ATLAS_COLS * TILE, rows * TILE), VOID + (255,))
    for i, (_, im) in enumerate(tiles):
        atlas.paste(im, ((i % ATLAS_COLS) * TILE, (i // ATLAS_COLS) * TILE))
    atlas = atlas.convert("RGB").convert("RGBA")   # hard-opaque; agb blanks a BG with any alpha
    atlas.save(path)
    used = {px[:3] for px in atlas.getdata()}
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
        "name": "spectra", "image": "tileset.png",
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


# ── The gauntlet ────────────────────────────────────────────────────────────────────────────────
# Rooms are ASCII because a puzzle you cannot read is a puzzle you cannot tune, and every one of
# these was moved a tile at a time. A screen is 15x10 cells at 16px, so a 15-wide room is exactly one
# screen and the camera never moves — deliberate for a puzzle: the whole problem is visible at once,
# so a solution is a plan rather than an exploration.
#
# LEGEND
#   .  air            *  air with a fleck      #  stone (always solid)     ^  spike (always lethal)
#   A B C  phase block, band A / B / C         a b c  phase ledge (land on, jump up through)
#   1 2 3  phase teeth (only bite in-band)     X Y Z  chroma door (open only in-band)
#   =  lens-lock field   o  prism (recharges the lantern)   G  the exit
#   P  player start      S  stalker            p  repaint pad
#
# THE RULE THE ROOMS ARE TUNED AGAINST: you are never FORCED to switch while standing on a block of
# the band you are switching away from. Every switch point is neutral stone, or a ledge of the band
# you are switching to. Room 2 is the single exception, and it is the room that teaches the exception.
ROOMS = [
    # 1 ── FIRST LIGHT. Walk on band A, stand on the neutral landing, switch, walk on band B.
    # The pit below is the whole stake: nothing here can hurt you except being wrong.
    ("FIRST LIGHT", 0, [
        "...............",
        "...............",
        "...............",
        "...............",
        "P..........G...",
        "##AAA##BBB#####",
        "...............",
        "...............",
        "...............",
        "...............",
    ]),
    # 2 ── THE CRUSH. The corridor is roofed with band C, so switching to VOID anywhere along it puts
    # a block inside you and the lantern refuses. The one legal cell is the notch at col 7. This is
    # the room that teaches that a refused switch is information rather than a bug.
    ("THE CRUSH", 0, [
        "...............",
        "...............",
        "###############",
        "#CCCCCC.CCCCCC#",
        "#..P........G.#",
        "###############",
        "...............",
        "...............",
        "...............",
        "...............",
    ]),
    # 3 ── THE THIRD LENS. A climb alternating all three bands, so the third arrives as a rung rather
    # than an announcement. Ledges, not blocks: a mistimed switch drops you a rung instead of killing.
    ("THE THIRD LENS", 0, [
        "..............G",
        "############.##",
        "......ccc......",
        "...............",
        "...bbb.........",
        "...............",
        "aaa............",
        "...............",
        "P..............",
        "###############",
    ]),
    # 4 ── THE LOCKED DOOR. Two doors, one decoy. The band B door is the way out; the band C door
    # opens onto the pit, and it is placed so the greedy read — "VOID, the near door" — is the one
    # that costs you.
    ("THE LOCKED DOOR", 0, [
        "...............",
        "...............",
        "#####...#######",
        "#...#...#.....#",
        "#P..Z...Y....G#",
        "#####...#######",
        ".......#.......",
        "...............",
        "...............",
        "...............",
    ]),
    # 5 ── TEETH. A floor of band A teeth you cross in any lens but DAWN. The prism above is bait:
    # reaching it means being in DAWN, and in DAWN the floor bites.
    ("TEETH", 1, [
        "...............",
        "...............",
        "......o........",
        "...............",
        "....BBBBB......",
        "...............",
        "P.11111111111.G",
        "###############",
        "...............",
        "...............",
    ]),
    # 6 ── THE NARROWS. Inside the hatching the lantern will not turn, so the lens you enter with is
    # the lens you finish with. Two corridors, each solvable in exactly one lens, and the choice is
    # made before you commit to it.
    ("THE NARROWS", 0, [
        "...............",
        "###############",
        "#=====BBBB====#",
        "#..P..........#",
        "###############",
        "#=====AAAA====#",
        "#............G#",
        "###############",
        "...............",
        "...............",
    ]),
    # 7 ── THE STALKER. It can only see you while you share its band; in any other lens you are not
    # there. The room is a straight run, which is the point — the difficulty is entirely in holding a
    # lens you would rather not be in.
    ("THE STALKER", 1, [
        "...............",
        "...............",
        "...............",
        "..*.......*....",
        "P.......S....G.",
        "###############",
        "...............",
        "...............",
        "...............",
        "...............",
    ]),
    # 8 ── CROSSFIRE. Act II all at once: a stalker on the floor, band teeth over the ledges, a door
    # at the end. There is a lens for each third of the room and no lens for two thirds at a time.
    ("CROSSFIRE", 2, [
        "...............",
        "...............",
        "....22...33....",
        "...bbb...ccc...",
        "...............",
        "P....S.......Y.",
        "#####.....#####",
        "...............",
        "...........G...",
        "###############",
    ]),
    # 9 ── WHITE. Hold L+R. The crossing needs a band A block and a band B block under the same run,
    # so no single lens makes it. WHITE drains the lantern and the prisms refill it.
    ("WHITE", 0, [
        "...............",
        "...............",
        "....o.....o....",
        "...............",
        "P.............G",
        "##AAA.BBB.AAA##",
        "...............",
        "...............",
        "...............",
        "...............",
    ]),
    # 10 ── THE PAINTERS. A repaint pad permanently moves the room's band C into band A, and it is
    # the first thing in the game you cannot undo — it is the only way to reach the exit, and also
    # the only way to lose the prism.
    ("THE PAINTERS", 2, [
        "...............",
        "...............",
        "......CCC......",
        "...............",
        "..o.........G..",
        "###.......#####",
        "...............",
        "..P..p.........",
        "###############",
        "...............",
    ]),
    # 11 ── THE LONG DARK. Everything the game has taught, in one crossing with no floor: three spans
    # of three bands, a lock field over the middle so the lens is chosen before it matters, and teeth
    # under the only span wide enough to stand on and think.
    #
    # (This slot was going to be moving band platforms. The engine has no rider-carry — `set_mover`
    # and `set_patrol` move the entity and not what stands on it — so a lift here would have slid out
    # from under the player. Recorded in the README as a real engine gap rather than faked.)
    ("THE LONG DARK", 1, [
        "...............",
        "...............",
        "....=====......",
        "...............",
        "P..aaa...ccc..G",
        "##....bbb....##",
        "...............",
        "....11111......",
        "...............",
        "...............",
    ]),
    # 12 ── THE SHAFT. Twenty rows straight up, alternating all three bands on a rhythm, with prisms
    # placed so the lantern reaches the top with nothing to spare.
    ("THE SHAFT", 0, [
        "..............G",
        "############.##",
        "...............",
        "....ccc........",
        "...............",
        "........bbb....",
        "......o........",
        "...aaa.........",
        "...............",
        ".......ccc.....",
        "...............",
        "...bbb.....aaa.",
        "......o........",
        "...............",
        "........ccc....",
        "...............",
        "...aaa.........",
        "...............",
        "P..............",
        "###############",
    ]),
]

CHARS = {
    ".": "air", "*": "air_star", "#": "stone", "^": "spike",
    "o": "prism", "=": "lock", "G": "goal",
    "A": "a_block", "a": "a_ledge", "1": "a_spike", "X": "a_door",
    "B": "b_block", "b": "b_ledge", "2": "b_spike", "Y": "b_door",
    "C": "c_block", "c": "c_ledge", "3": "c_spike", "Z": "c_door",
}
SPAWN_CHARS = {"P": "player", "S": "stalker", "p": "pad"}
CHAR_BAND = {"A": 1, "a": 1, "1": 1, "X": 1,
             "B": 2, "b": 2, "2": 2, "Y": 2,
             "C": 3, "c": 3, "3": 3, "Z": 3}
LEDGE_CHARS = set("abc")
HAZARD_CHARS = set("123")
DOOR_CHARS = set("XYZ")

GID = {}


def parse_room(rows):
    """Turn one ASCII room into (world grid, three band grids, spawns, band cells).

    EACH BAND GETS ITS OWN TILE LAYER, which is the whole implementation of "what is not in your lens
    is not there": showing a band is `stream_visible(band, on)`, one call, no gid and no palette
    entry anywhere in it. The world layer keeps plain air under every band cell, so a hidden band
    leaves the room looking like there was never anything there.

    The band cells come out HERE rather than out of the map blob, because the scene packer bakes only
    the solid / one-way / ladder planes and knows nothing about bands. Emitting them from the same
    description that emits the .tmj is what keeps the collision the runtime applies from drifting
    away from the art the map draws: there is one source, and it is the picture above.
    """
    h, w = len(rows), len(rows[0])
    for i, r in enumerate(rows):
        if len(r) != w:
            raise SystemExit(f"room row {i} is {len(r)} wide, expected {w}: {r!r}")
    g = grid(w, h, 0)
    # 0 is "no tile here", which leaves the band layer transparent so the world shows through.
    bands = [grid(w, h, 0) for _ in range(3)]
    spawns, cells = [], []
    for r, line in enumerate(rows):
        for c, ch in enumerate(line):
            if ch in SPAWN_CHARS:
                spawns.append((SPAWN_CHARS[ch], c, r, 0))
                g[r][c] = GID["air"]
                continue
            if ch == "G":
                # The exit is a tile you can see AND a trigger you can touch.
                spawns.append(("door", c, r, 0))
            if ch == "o":
                spawns.append(("prism", c, r, 0))
            name = CHARS.get(ch)
            if name is None:
                raise SystemExit(f"unknown room character {ch!r} at {c},{r}")
            if ch in CHAR_BAND:
                bands[CHAR_BAND[ch] - 1][r][c] = GID[name]
                g[r][c] = GID["air"]
            else:
                g[r][c] = GID[name]
            if ch in CHAR_BAND:
                # word = col | row<<6 | band<<12 | kind<<14
                #   kind: 0 block, 1 ledge, 2 hazard, 3 door, 4 lock field
                # Only COLLISION needs these — the art is the layer's job. Packed into one word
                # because they are a flat `i32[]`: a list of small objects would be a boxed Value
                # each, and this is the hot data on a lens switch.
                kind = (1 if ch in LEDGE_CHARS else
                        2 if ch in HAZARD_CHARS else
                        3 if ch in DOOR_CHARS else 0)
                cells.append(c | (r << 6) | (CHAR_BAND[ch] << 12) | (kind << 14))
            elif ch == "=":
                # A lock field is not a band cell, but it lives in the same list because it is the
                # same question asked of the same coordinates: what does the lantern do here?
                cells.append(c | (r << 6) | (4 << 14))
    return g, bands, spawns, cells


def emit_rooms_tish(path, rooms):
    """Write `src/rooms.tish` — the room table and the band cells.

    ⚠️ EVERY ARRAY IS `i32[]` AND EVERY SCALAR `i32`. An unannotated `let` in tish is a thread-local
    `Cell<f64>` and an unannotated array is a boxed `Value::Array` of boxed `Value::Number` — three
    soft-float operations per element read, on a chip with no FPU. These are walked on every lens
    switch; boxed, a switch would cost more than the room it is switching.
    """
    names = [r[0] for r in rooms]
    starts, cells = [0], []
    for r in rooms:
        cells.extend(r[4])
        starts.append(len(cells))
    body = [
        "// GENERATED by scripts/gen_spectra.py — do not edit; edit the ASCII rooms there.",
        "//",
        "// The band cells of every room, flattened into one array. Emitted from the same room",
        "// description that emits the .tmj files, which is what stops the collision the runtime",
        "// applies from drifting away from the art the map draws.",
        "",
        f"export let ROOMS: i32 = {len(rooms)}",
        "export let R_NAME: string[] = [" + ", ".join(json.dumps(n) for n in names) + "]",
        "// Which lens each room starts you in.",
        "export let R_LENS: i32[] = [" + ", ".join(str(r[1]) for r in rooms) + "]",
        "// Room size in tiles. The height tells the game where 'fell out of the room' is; a fixed",
        "// threshold either kills a tall room's player mid-climb or leaves a short room's player",
        "// falling for four seconds through nothing before anything notices.",
        "export let R_W: i32[] = [" + ", ".join(str(r[2]) for r in rooms) + "]",
        "export let R_H: i32[] = [" + ", ".join(str(r[3]) for r in rooms) + "]",
        "",
        "// Band cells, packed: col | row<<6 | band<<12 | kind<<14.",
        "//   band 1 = A/DAWN, 2 = B/DUSK, 3 = C/VOID; band 0 = no band (a lens-lock field)",
        "//   kind 0 = block, 1 = ledge, 2 = hazard, 3 = door, 4 = lock field",
        "// Room r owns R_CELL[R_START[r] .. R_START[r+1]).",
        "export let R_START: i32[] = [" + ", ".join(str(s) for s in starts) + "]",
        "export let R_CELL: i32[] = [",
    ]
    for i in range(0, len(cells), 16):
        body.append("  " + ", ".join(str(v) for v in cells[i:i + 16]) + ",")
    body[-1] = body[-1].rstrip(",")
    body += [
        "]",
        "",
        "// The lenses. A lens is a BACKDROP COLOUR plus which band layers are shown — see the long",
        "// note beside LENSES in scripts/gen_spectra.py for why it is not a palette write.",
        "export let LENS: i32 = " + str(len(LENSES)),
        "export let LENS_NAME: string[] = [" + ", ".join(json.dumps(n) for n, _ in LENSES) + "]",
        "export let LENS_BACKDROP: i32[] = [" + ", ".join(str(c) for _, c in LENSES) + "]",
        "",
        "// Scene layer index per band: A = 0, B = 1, C = 2; the world is 3. Set by the layer order",
        "// the generator emits — read the note beside `emit_tmj` before changing either.",
        "export let LAYER_BAND: i32 = 0",
        "export let LAYER_WORLD: i32 = 3",
        "",
    ]
    with open(path, "w") as f:
        f.write("\n".join(body) + "\n")
    return len(cells)


def main():
    global GID
    os.makedirs(ASSETS, exist_ok=True)
    os.makedirs(os.path.join(EX, "src"), exist_ok=True)
    tiles = build_tiles()
    n, rows, used = emit_atlas(tiles, os.path.join(ASSETS, "tileset.png"))
    names = [t[0] for t in tiles]
    GID = {nm: i + 1 for i, nm in enumerate(names)}
    emit_tsj(os.path.join(ASSETS, "tiles.tsj"), names, n)

    hero, nframes = build_hero()
    hero.save(os.path.join(ASSETS, "hero.png"))
    stalker, nstalk = build_stalker()
    stalker.save(os.path.join(ASSETS, "stalker.png"))

    built = []
    for i, (name, lens, art) in enumerate(ROOMS):
        g, bands, sp, cells = parse_room(art)
        rh, rw = len(art), len(art[0])
        sp = [(k, c, r, (i + 1) % len(ROOMS) if k == "door" else a) for (k, c, r, a) in sp]
        # ⚠️ LAYER ORDER IS LOAD-BEARING TWICE OVER. The packer sorts layers by (priority,
        # Reverse(tiled index)) and the runtime indexes `stream_visible` by that emitted order — so
        # listing the world FIRST here is what makes band A layer 0, B 1, C 2 and the world 3.
        # Every layer is priority 2 because world sprites draw at 2 and an object beats a background
        # of the same priority; the earlier-emitted layer wins the tie, which puts the bands in front
        # of the world's air and still behind the player.
        emit_tmj(os.path.join(ASSETS, f"r{i + 1:02d}.tmj"), rw, rh,
                 [("World", 2, g), ("BandC", 2, bands[2]),
                  ("BandB", 2, bands[1]), ("BandA", 2, bands[0])], sp)
        built.append((name, lens, rw, rh, cells))
        kinds = {}
        for k, _, _, _ in sp:
            kinds[k] = kinds.get(k, 0) + 1
        print(f"  r{i + 1:02d} {name:16s} {rw:2d}x{rh:2d}  {len(cells):3d} band cells  {kinds}")

    ncells = emit_rooms_tish(os.path.join(EX, "src", "rooms.tish"), built)
    with open(os.path.join(ASSETS, "tiles.json"), "w") as f:
        json.dump({"names": names,
                   "palette": ["#%02x%02x%02x" % c for c in PALETTE]}, f, indent=1)
        f.write("\n")
    print(f"tileset.png: {n} tiles, {ATLAS_COLS}x{rows}, {len(used)} distinct colours")
    print(f"hero.png: {nframes} frames · stalker.png: {nstalk} frames")
    print(f"rooms.tish: {len(built)} rooms, {ncells} band cells, {len(LENSES)} lenses")


if __name__ == "__main__":
    main()
