#!/usr/bin/env python3
"""Build the `oakhollow` example's ROM assets + levels.

Oakhollow is a side-scrolling town on three tiers: an undertown of cellars and docks, the main
street, and a rooftop walk you reach by ladder and ledge-grab. This script turns two raw art packs
plus the vendored Ninja Adventure catalog into GBA-ready sheets, and writes the levels.

Outputs (examples/oakhollow/assets):

  tiles.png     ONE image holding the sky, the hill silhouette AND every world tile, because
                loading a tileset REPLACES all 16 background palettes — two tilesets on screen at
                once fight over them and the loser renders in the winner's colours. One image means
                one palette set. Every layer of every level is painted from this.
  tiles.tsj     the Tiled tileset over tiles.png: which tiles are solid, which are one-way
                platforms, which can be climbed. Collision belongs to the TILE, not to the map.
  town.tmj      the 120x30 town — sky, hills and world as three layers with Tiled's own parallax
                factors, plus a spawn object layer
  inn/store/forge.tmj       the three interiors, one layer each
  hero.png      `sheet64:` — COPIED from examples/dark-hero (the "DARK - Hero" pack), so the
                two examples share one player character. Not built here; see make_hero().
  npcs.png      `sheet64:` — eight townsfolk, palette-shifted from the same character
  ui32.png      `sheet32:` — dialog portraits, shop ware icons, the selection cursor

The .tmj/.tsj files are written ONCE, by this script, and are the source of truth from then on:
open them in Tiled and edit. `scene:` bakes them into ROM at build time — the atlas it packs and
the map blob it writes are build artifacts, not checked in. Re-running this script regenerates the
art; it will also overwrite the levels, so don't, once you have edited them in the editor.

Art sources:
  ~/Downloads/Adventurer-1.5, ~/Downloads/Adventurer-Hand-Combat   (rvros) — the character
  ~/Downloads/Sunny-land-files                                     (Ansimuz, CC0) — world + sky
  assets/ninja-adventure/                                          (Pixel-boy & AAA, CC0) — icons

Run: python3 scripts/gen_oakhollow.py
"""
import json
import os
import colorsys
from PIL import Image, ImageDraw

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
EX = os.path.join(REPO, "examples", "oakhollow")
ASSETS = os.path.join(EX, "assets")

DL = os.path.expanduser("~/Downloads")
ADV = os.path.join(DL, "Adventurer-1.5", "Individual Sprites")
ADV_HC = os.path.join(DL, "Adventurer-Hand-Combat", "Individual Sprites")
SL = os.path.join(DL, "Sunny-land-files", "Assets", "environment")
PROPS = os.path.join(SL, "Props")
NINJA = os.path.join(REPO, "assets", "ninja-adventure")

TILE = 16
ATLAS_COLS = 16          # 16 tiles x 16 px = a 256px-wide atlas


# ── colour helpers ───────────────────────────────────────────────────────────────────────────────
def clamp_colors(im, maxc=15):
    """Hold an RGBA sprite sheet inside a GBA 4bpp sprite's budget (15 colours + transparent).

    Two things here are load-bearing and were both learned the hard way.

    The palette is chosen from the OPAQUE PIXELS ONLY. Quantising the sheet directly hands the
    quantiser an image that is mostly transparent — and `convert("RGB")` turns transparent to
    BLACK, so on a 64x64-cell sheet nine tenths of the histogram is black, the median cut spends its
    budget splitting shades of nothing, and the character comes out in about three colours. Packing
    the opaque pixels into a strip first gives the quantiser only pixels that will actually be seen.

    And it quantises the SHEET, not each frame: one palette across the whole animation. A per-frame
    palette makes the character's skin tone shimmer as the clip plays.
    """
    im = im.convert("RGBA")
    px = list(im.getdata())
    opaque = [(r, g, b) for (r, g, b, a) in px if a > 8]
    if len(set(opaque)) <= maxc:
        return im
    strip = Image.new("RGB", (len(opaque), 1))
    strip.putdata(opaque)
    chosen = strip.quantize(colors=maxc, method=Image.MEDIANCUT, dither=Image.NONE)
    pal = chosen.getpalette()[: maxc * 3]
    palimg = Image.new("P", (1, 1))
    palimg.putpalette(pal + [0] * (768 - len(pal)))
    mapped = im.convert("RGB").quantize(palette=palimg, dither=Image.NONE).convert("RGB")
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    mpx, opx = mapped.load(), out.load()
    for i, (r, g, b, a) in enumerate(px):
        if a > 8:
            x, y = i % im.width, i // im.width
            opx[x, y] = mpx[x, y] + (255,)
    return out


def recolor(im, hue_shift, sat_mul, val_mul):
    """Shift an RGBA image around the colour wheel, preserving shading.

    This is how eight townsfolk come out of one character sheet. Rotating HUE (rather than swapping
    a fixed palette) keeps every shadow and highlight exactly where the artist put it, so a recoloured
    villager still reads as the same well-drawn sprite — just in a different coat.
    """
    im = im.convert("RGBA")
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    src, dst = im.load(), out.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = src[x, y]
            if a <= 8:
                continue
            h, s, v = colorsys.rgb_to_hsv(r / 255.0, g / 255.0, b / 255.0)
            h = (h + hue_shift) % 1.0
            s = min(1.0, s * sat_mul)
            v = min(1.0, v * val_mul)
            nr, ng, nb = colorsys.hsv_to_rgb(h, s, v)
            dst[x, y] = (int(nr * 255), int(ng * 255), int(nb * 255), 255)
    return out


def paste_wrapped(dst, src, x, y):
    """Paste with horizontal wrap, so anything crossing the right edge reappears on the left.

    A parallax layer is a 256px-wide background the GBA repeats forever; a silhouette that runs off
    the edge has to come back on the other side or the repeat shows a hard seam every screen.
    """
    dst.alpha_composite(src, (x % dst.width, y))
    if x % dst.width + src.width > dst.width:
        dst.alpha_composite(src, (x % dst.width - dst.width, y))


# ── the atlas ────────────────────────────────────────────────────────────────────────────────────
class Atlas:
    """Collects 16x16 tiles and hands back GIDs, deduplicating identical ones.

    Dedup matters twice over: it shrinks the baked image, and it means a town that uses the same
    plaster wall on four buildings pays for it once in background VRAM.
    """

    def __init__(self):
        self.tiles = []
        self.index = {}

    def add(self, im):
        im = im.convert("RGBA")
        if im.size != (TILE, TILE):
            cell = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
            cell.alpha_composite(im, (0, 0))
            im = cell
        key = im.tobytes()
        # A tile with nothing in it is GID 0 — "no tile here" — which is what lets the sky show
        # through the world layer. It must never be allocated a real slot.
        if not any(p[3] > 8 for p in im.getdata()):
            return 0
        if key in self.index:
            return self.index[key]
        self.tiles.append(im)
        gid = len(self.tiles)
        self.index[key] = gid
        return gid

    def add_grid(self, im, opaque_fill=None):
        """Cut an image into 16px cells and return a 2D GID grid (rows of columns)."""
        if opaque_fill is not None:
            bg = Image.new("RGBA", im.size, opaque_fill + (255,))
            bg.alpha_composite(im.convert("RGBA"))
            im = bg
        im = im.convert("RGBA")
        rows = []
        for r in range(im.height // TILE):
            row = []
            for c in range(im.width // TILE):
                row.append(self.add(im.crop((c * TILE, r * TILE, c * TILE + TILE, r * TILE + TILE))))
            rows.append(row)
        return rows

    def save(self, path):
        n = len(self.tiles)
        rows = (n + ATLAS_COLS - 1) // ATLAS_COLS
        sheet = Image.new("RGBA", (ATLAS_COLS * TILE, rows * TILE), (0, 0, 0, 0))
        for i, t in enumerate(self.tiles):
            sheet.paste(t, ((i % ATLAS_COLS) * TILE, (i // ATLAS_COLS) * TILE))
        sheet.save(path)
        uniq8 = set()
        for by in range(0, sheet.height, 8):
            for bx in range(0, sheet.width, 8):
                uniq8.add(sheet.crop((bx, by, bx + 8, by + 8)).tobytes())
        colors = set(p[:3] for p in sheet.getdata() if p[3] > 8)
        print(f"tiles.png: {sheet.size}  {n} tiles  {len(uniq8)} unique 8x8  {len(colors)} colours")
        return n


# ── sky + hills (the two parallax layers) ────────────────────────────────────────────────────────
# Both are 16x16-tile grids: `tilemap_new` builds a 32x32 background of 8px tiles, which is 256x256
# px, which is also exactly the size the GBA wraps a background at. So a 16x16 grid of 16px tiles
# fills it edge to edge and repeats seamlessly forever in both axes.
PX_GRID = 16


# The three SKY shades in the pack's back.png. All three are keyed out of the cloud crops; what
# survives is cloud. Missing the third one (140,247,255) is not subtle — it is a mid-blue that fills
# the gaps between wisps, so a crop keeps a pale rectangle around every cloud it contains.
SKY_KEY = {(71, 243, 255), (136, 252, 227), (140, 247, 255)}
SKY_TOP = (71, 243, 255)
SKY_LOW = (186, 244, 255)


def key_out(im, colors):
    """Punch the listed colours out to transparent.

    The pack drew its clouds ON a solid sky rather than on alpha, so cropping a cloud gives you a
    rectangle of sky with a cloud in it. Pasted over a gradient, that rectangle is plainly visible.
    Keying the sky colour out leaves the cloud alone.
    """
    im = im.convert("RGBA")
    p = im.load()
    for y in range(im.height):
        for x in range(im.width):
            if p[x, y][:3] in colors:
                p[x, y] = (0, 0, 0, 0)
    return im


def make_sky(atlas):
    """The far layer: a banded vertical gradient, and nothing else.

    The clouds live on the TREELINE layer instead (see `make_hills`), and the reason is background
    VRAM. Unlike a streamed map — which only holds the tiles currently near the camera — a parallax
    layer sets every one of its tiles at load and keeps them resident for as long as the scene does.
    The pack's sky is 384x116 of continuously varying cloud, nearly 400 distinct 8x8 tiles, and even
    the trimmed version cost 160: a permanent 5 KB spent on the thing furthest from the player, which
    is 5 KB the shop's full-screen list then could not have. This gradient is sixteen 8px bands, and
    a band of one colour deduplicates to a single tile.
    """
    canvas = Image.new("RGBA", (256, 256), SKY_LOW + (255,))
    for i, y in enumerate(range(0, 112, 8)):
        t = i / 14.0
        c = tuple(int(SKY_TOP[k] + (SKY_LOW[k] - SKY_TOP[k]) * t) for k in range(3))
        canvas.paste(Image.new("RGBA", (256, 8), c + (255,)), (0, y))
    return atlas.add_grid(canvas), SKY_TOP


def cloud_band(src):
    """A seamless 128px band of cumulus, cut from the pack's sky and keyed onto alpha.

    The bank is continuous in the source — there is no column of clear sky to cut on — so any crop
    of it runs off both edges and pastes as a rectangle with hard sides. Mirroring fixes that: a
    piece beside its own reflection joins seamlessly, and the 128px unit repeats with the far join
    mirrored too. It is also nearly free, because the background bake deduplicates flipped tiles.
    """
    piece = key_out(src.crop((176, 56, 304, 120)).resize((64, 32), Image.NEAREST), SKY_KEY)
    band = Image.new("RGBA", (128, 32), (0, 0, 0, 0))
    band.alpha_composite(piece, (0, 0))
    band.alpha_composite(piece.transpose(Image.FLIP_LEFT_RIGHT), (64, 0))
    return band


def make_hills(atlas):
    """The near layer: the pack's foliage ridge, flattened to one colour and wrapped across 256px.

    Alpha is deliberate — everything the ridge does not cover has to show the sky layer behind it.
    agb maps a fully transparent background pixel to palette index 0, which the hardware draws as
    see-through, so an alpha pixel and a GID 0 cell end up doing the same job.

    Flattening to a single colour is not just a style choice: the pack's version is shaded, and every
    leaf edge in it is a distinct 8x8 tile. As a silhouette the interior collapses to one tile and
    only the ridge line costs anything — 85 tiles instead of 272, for something that is meant to read
    as a dark shape on the horizon anyway.

    The ridge sits in the LOWER half of the layer with empty space above it. That space is the
    headroom the vertical drift needs: the camera travels 320px down the town and this layer follows
    at 3/16 of that, so the art never wraps back into view from the top.
    """
    RIDGE = (58, 92, 102, 255)
    src = Image.open(os.path.join(SL, "Background", "middle.png")).convert("RGBA")
    hill = src.crop(src.getbbox())
    # Only the RIDGE LINE is used. The pack's silhouette is nearly 300px tall — pasted at horizon
    # height it runs off the bottom of a 256px layer and WRAPS back over the sky, which is what put
    # a band of forest above the clouds the first time this was drawn.
    crest = hill.crop((0, 0, hill.width, 84))
    flat = Image.new("RGBA", crest.size, (0, 0, 0, 0))
    fp, hp = flat.load(), crest.load()
    for y in range(crest.height):
        for x in range(crest.width):
            if hp[x, y][3] > 8:
                fp[x, y] = RIDGE
    canvas = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
    # The clouds ride on THIS layer, above the ridge — see `make_sky` for why they are not on the sky
    # itself. They drift at the treeline's speed rather than their own, which for a bank sitting on
    # the horizon is where they belong anyway.
    paste_wrapped(canvas, cloud_band(src), 0, 56)
    paste_wrapped(canvas, cloud_band(src), 128, 56)
    # Two copies of the ridge, the second smaller and lower: the same hills, seen further away.
    for (x, sc, y) in [(-20, 1.0, 88), (152, 0.7, 104)]:
        h2 = flat.resize((max(1, int(flat.width * sc)), max(1, int(flat.height * sc))), Image.NEAREST)
        paste_wrapped(canvas, h2, x, y)
    # Fill each column from its TOPMOST ridge pixel to the bottom of the layer, so the crest line is
    # the silhouette's only edge. Filling from the BOTTOM pixel instead leaves the gaps between the
    # crop's own leaves see-through, and at rooftop height those gaps are what the player is looking
    # at — a treeline full of sky-coloured holes. Closing them is also cheaper: a solid interior is
    # one deduplicated tile however wide the forest is.
    cp = canvas.load()
    for x in range(256):
        top = -1
        for y in range(256):
            if cp[x, y][3] > 8:
                top = y
                break
        if top >= 0:
            for y in range(top, 256):
                cp[x, y] = RIDGE
    # And a solid base UNDER the whole ridge, high enough that no column of sky survives beneath it.
    # A pure silhouette leaves the gaps between the two copies open all the way down, and from the
    # rooftops — where the town's own ground is far below the screen — those read as pale wedges
    # falling off the bottom edge, as if the forest were floating in pieces. 128 sits just below the
    # lower copy's crest, so the ridge LINE is all you see of the shape and everything under it is
    # one deduplicated tile.
    for x in range(256):
        for y in range(128, 256):
            cp[x, y] = RIDGE
    return atlas.add_grid(canvas)


# ── world tiles ──────────────────────────────────────────────────────────────────────────────────
def tile_from(sheet, col, row):
    return sheet.crop((col * TILE, row * TILE, col * TILE + TILE, row * TILE + TILE))


def make_world_tiles(atlas, sky_rgb):
    """Curate the world tileset out of the Sunny Land tileset and props.

    Ground tiles are flattened onto the sky colour so they are fully opaque; anything decorative
    keeps its alpha and floats over whatever is behind it. Returns a name -> GID map.
    """
    ts = Image.open(os.path.join(SL, "tileset.png")).convert("RGBA")

    def solid_tile(col, row):
        """An opaque terrain tile: composite over the sky so no map cell is see-through."""
        cell = Image.new("RGBA", (TILE, TILE), sky_rgb + (255,))
        cell.alpha_composite(tile_from(ts, col, row))
        return atlas.add(cell)

    def clear_tile(col, row):
        return atlas.add(tile_from(ts, col, row))

    def prop(name, x, y):
        im = Image.open(os.path.join(PROPS, name)).convert("RGBA")
        return atlas.add(im.crop((x, y, x + TILE, y + TILE)))

    g = {}
    # ── the families ─────────────────────────────────────────────────────────────────────────────
    # This tileset is not a bag of interchangeable blocks, and treating it as one is exactly what
    # made the first version look wrong. It ships PIECE SETS, and a piece only reads correctly in the
    # position it was drawn for:
    #
    #   masonry   a 3x3 NINE-SLICE at (15,7)..(19,11) — corner, edge and interior pieces. Using its
    #             top-left corner as a general-purpose block, which is what the first pass did, gives
    #             you a wall built entirely out of corners.
    #   platforms THREE-piece runs with distinct left and right ends: wooden beams at (9..11, 1) and
    #             a grass-topped ledge at (15..19, 14). A run stamped from one middle piece has no
    #             ends, which is why every walkway read as a floating bar.
    #   ground    grass caps over dirt fill, with several variants of each so a long street does not
    #             look printed.
    #
    # `resolve_gids` picks the right piece per cell from its neighbours; everything here just names
    # them.
    g["grass"] = solid_tile(1, 1)
    g["grass2"] = solid_tile(3, 1)
    g["grass3"] = solid_tile(5, 1)
    g["dirt"] = solid_tile(7, 1)
    g["dirt2"] = solid_tile(1, 3)
    g["dirt3"] = solid_tile(5, 3)
    g["dirt4"] = solid_tile(1, 5)
    g["dirt5"] = solid_tile(5, 5)
    # Masonry nine-slice: [row][col] = top/middle/bottom x left/middle/right.
    g["stnTL"] = solid_tile(15, 7)
    g["stnT"] = solid_tile(17, 7)
    g["stnTR"] = solid_tile(19, 7)
    g["stnL"] = solid_tile(15, 9)
    g["stnM"] = solid_tile(17, 9)
    g["stnR"] = solid_tile(19, 9)
    g["stnBL"] = solid_tile(15, 11)
    g["stnB"] = solid_tile(17, 11)
    g["stnBR"] = solid_tile(19, 11)
    # Undertown backdrop: dark, opaque, and NOT solid — the wall BEHIND the cellars, which you walk
    # in front of. It has to be opaque or the sky shows through the cellars, which is exactly what
    # happened the first time: (16,16) in this tileset is an empty cell, so every other backdrop tile
    # came out as GID 0 and the undertown rendered as diagonal stripes of daylight.
    g["cellar"] = solid_tile(17, 16)
    g["cellar2"] = solid_tile(19, 16)
    # The apothecary shelving, for the walls of the forge and the storerooms.
    g["shelfA"] = solid_tile(14, 16)
    g["shelfB"] = solid_tile(15, 16)
    g["shelfC"] = solid_tile(14, 17)
    g["shelfD"] = solid_tile(15, 17)
    # Decor, all alpha, all walk-through.
    g["tuft"] = clear_tile(1, 7)
    g["reeds"] = clear_tile(3, 7)
    g["rock"] = clear_tile(5, 7)
    g["pebbles"] = clear_tile(7, 7)
    g["bush"] = clear_tile(9, 7)
    g["bush2"] = clear_tile(11, 7)
    g["vine"] = clear_tile(7, 13)
    g["vine2"] = clear_tile(7, 14)
    g["barrel"] = clear_tile(10, 10)
    g["logend"] = clear_tile(11, 10)
    g["lantern"] = clear_tile(17, 20)
    # One-way walkways, both THREE-piece runs. Beams for the built structures, grass-topped ledges for
    # the natural ones; `resolve_gids` picks the end pieces from the run's own extent.
    g["beamL"] = clear_tile(9, 1)
    g["beamM"] = clear_tile(10, 1)
    g["beamR"] = clear_tile(11, 1)
    # INDOOR copies of everything with transparent edges, composited onto the cellar wall.
    #
    # Outdoors that transparency shows the parallax sky; a room has no layer behind it, so it shows
    # the black backdrop — every barrel wearing two black bars, and the loft a hole in the wall. The
    # obvious fix is to give the room a flat backdrop LAYER, and that works, but a `RegularBackground`
    # is a 32x32 screenblock plus its bookkeeping on a heap the shop then needs every byte of. Six
    # pre-composited tiles cost nothing at runtime and look identical.
    def indoor(src):
        cell = tile_from(ts, 17, 16).copy()
        cell.alpha_composite(src)
        return atlas.add(cell)

    for suffix, src_col in (("L", 9), ("M", 10), ("R", 11)):
        g["ibeam" + suffix] = indoor(tile_from(ts, src_col, 1))
    g["ibarrel"] = indoor(tile_from(ts, 10, 10))
    g["icrate"] = indoor(Image.open(os.path.join(PROPS, "crate.png")).convert("RGBA"))
    g["ilantern"] = indoor(tile_from(ts, 17, 20))
    g["ledgeL"] = solid_tile(15, 14)
    g["ledgeM"] = solid_tile(17, 14)
    g["ledgeR"] = solid_tile(19, 14)
    g["post"] = clear_tile(10, 10)       # a turned wooden column, for holding a walkway up
    # The tileset HAS a ladder — (7,10) — which the first pass missed and replaced with a strip
    # cropped out of the tree-house prop.
    rungs = tile_from(ts, 7, 10)
    g["ladder"] = atlas.add(rungs)
    # The tile that CAPS a ladder, and the only reason climbing works at either end.
    #
    # A climb stops when the feet leave the last climbable tile — so the player is released standing
    # one row ABOVE the ladder's top, with nothing under them, and falls straight back down. The cap
    # is a beam laid across the top rungs that is climbable AND a one-way floor, so the release lands
    # on it. The same tile is what makes a floor hatch work from the other side: you can walk over the
    # shaft, and Down grabs the ladder through it.
    cap = rungs.copy()
    cap.alpha_composite(tile_from(ts, 10, 1).crop((0, 0, TILE, 7)), (0, 0))
    g["ladderTop"] = atlas.add(cap)
    g["sign"] = prop("sign.png", 0, 2)
    g["crate"] = prop("crate.png", 0, 0)
    g["door"] = atlas.add(Image.open(os.path.join(PROPS, "door.png")).convert("RGBA")
                          .crop((3, 1, 19, 17)))
    # Water for the docks: a surface row and a fill row, both opaque so the sky can't leak through.
    water = Image.new("RGBA", (TILE, TILE), (48, 132, 190, 255))
    surf = water.copy()
    d = ImageDraw.Draw(surf)
    d.rectangle([0, 0, TILE - 1, 2], fill=(120, 208, 235, 255))
    d.rectangle([0, 3, TILE - 1, 4], fill=(78, 168, 214, 255))
    g["water"] = atlas.add(surf)
    g["waterfill"] = atlas.add(water)
    return g


def make_buildings(atlas):
    """Slice each house prop into a block of 16px tiles: name -> 2D GID grid.

    Buildings live in the TILEMAP, not in sprites. A 128x96 straw roof would be twenty-four 32x32
    sprites, and the GBA can hold 128 sprites in total — the map is where scenery belongs, and it
    costs nothing per frame.
    """
    out = {}
    for key, name in [("house", "house.png"), ("wood", "wooden-house.png"),
                      ("straw", "straw-house.png"), ("plant", "plant-house.png")]:
        im = Image.open(os.path.join(PROPS, name)).convert("RGBA")
        w = (im.width + TILE - 1) // TILE * TILE
        h = (im.height + TILE - 1) // TILE * TILE
        # Bottom-align: a building's footprint has to sit exactly on the street, and the padding
        # belongs above the roof where nothing will notice it.
        cell = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        cell.alpha_composite(im, ((w - im.width) // 2, h - im.height))
        out[key] = atlas.add_grid(cell)
    return out


# ── the character ────────────────────────────────────────────────────────────────────────────────
# The Adventurer's frames are 50x37 and must land in a 64x64 sprite cell (the GBA only has 8/16/32/64).
# Every frame is centred on the CHARACTER, not on the source frame: the artist drew a run cycle that
# drifts across its own frame, and if you keep that drift the sprite lurches sideways when it flips.
CELL64 = 64

# (clip name, source dir, file prefix, frame count) in sheet order. The offsets this produces are
# printed at the end and must match the CLIPS table in the example's components.tish.
HERO_CLIPS = [
    ("idle", ADV, "adventurer-idle", 4),
    ("walk", ADV_HC, "adventurer-walk", 6),
    ("run", ADV, "adventurer-run", 6),
    ("jump", ADV, "adventurer-jump", 4),
    ("fall", ADV, "adventurer-fall", 2),
    ("land", ADV, "adventurer-stand", 3),
    ("crouch", ADV, "adventurer-crouch", 4),
    ("crouchWalk", ADV_HC, "adventurer-crouch-walk", 6),
    ("slide", ADV, "adventurer-slide", 2),
    ("climb", ADV, "adventurer-ladder-climb", 4),
    ("hang", ADV, "adventurer-crnr-grb", 4),
    ("ledge", ADV, "adventurer-crnr-clmb", 5),
    ("wallslide", ADV, "adventurer-wall-slide", 2),
    ("hurt", ADV, "adventurer-hurt", 3),
    ("dead", ADV, "adventurer-die", 7),
]


def load_clip(d, prefix, count):
    return [Image.open(os.path.join(d, f"{prefix}-{i:02d}.png")).convert("RGBA") for i in range(count)]


def pack_frames(frames):
    """Place 50x37 frames into 64x64 cells: horizontally centred on the character, feet on the floor.

    Centring is per CLIP, not per frame, so motion the animator drew INTO a clip (the lean of a run,
    the sprawl of a death) survives, while the clip as a whole sits square in the cell. Feet go on
    the cell's bottom edge so one `spriteOffset` lines every clip up with the hitbox.
    """
    box = None
    for f in frames:
        b = f.getbbox()
        if b is None:
            continue
        box = b if box is None else (min(box[0], b[0]), min(box[1], b[1]),
                                     max(box[2], b[2]), max(box[3], b[3]))
    dx = int(round(CELL64 / 2 - (box[0] + box[2]) / 2))
    dy = CELL64 - box[3]
    out = []
    for f in frames:
        cell = Image.new("RGBA", (CELL64, CELL64), (0, 0, 0, 0))
        cell.alpha_composite(f, (dx, dy))
        out.append(cell)
    return out


def build_hero_frames():
    """Every hero frame in sheet order, plus the clip offset table."""
    frames, table, at = [], [], 0
    for name, d, prefix, count in HERO_CLIPS:
        packed = pack_frames(load_clip(d, prefix, count))
        table.append((name, at, len(packed)))
        frames.extend(packed)
        at += len(packed)
    return frames, table


def sheet_of(frames):
    sheet = Image.new("RGBA", (CELL64 * len(frames), CELL64), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        sheet.paste(f, (i * CELL64, 0))
    return sheet


def make_hero(frames):
    """The PLAYER's sheet, which this script no longer builds — it copies dark-hero's.

    Oakhollow's hero used to be the rvros Adventurer, packed here out of `HERO_CLIPS`. It was swapped
    for the "DARK - Hero" pack that examples/dark-hero already bakes, so the two examples share one
    character and one sheet. The Adventurer path below is NOT dead: `build_hero_frames` still runs,
    because the townsfolk and the dialog portraits are recoloured Adventurer sprites and the ware
    icons are cropped from these frames. Only the player changed.

    ⚠️ The frame layout of the two sheets is completely different (79 frames in ten states, versus 62
    in fifteen), so `CLIPS` in src/components.tish is written against dark-hero's offsets and the
    table this function used to print no longer describes the player.
    """
    src = os.path.join(REPO, "examples", "dark-hero", "assets", "hero.png")
    if not os.path.exists(src):
        raise SystemExit(f"hero sheet missing: {src}\nrun scripts/gen_darkhero.py first")
    Image.open(src).save(os.path.join(ASSETS, "hero.png"))
    return clamp_colors(sheet_of(frames), 15)   # still returned: make_ui crops ware icons from it


# Eight townsfolk, as (name, hue shift, saturation, value). The elder is desaturated and pale; the
# kid is small (scaled in `make_npcs`); the rest are just differently dressed.
NPCS = [
    ("elder", 0.55, 0.25, 1.25),
    ("merchant", 0.92, 1.25, 1.05),
    ("smith", 0.02, 1.30, 0.72),
    ("innkeep", 0.30, 0.95, 1.10),
    ("guard", 0.60, 1.10, 0.92),
    ("kid", 0.14, 1.30, 1.20),
    ("fisher", 0.48, 0.85, 1.00),
    ("baker", 0.10, 0.70, 1.30),
]
NPC_IDLE = 4       # idle frames per townsperson
KID_INDEX = 5


def make_npcs():
    """One 64x64 sheet: four idle frames per townsperson, then a walk cycle for the one who moves.

    Frame layout (matches src/town.tish): npc i idle = i*4 .. i*4+3; the kid's walk follows at 32.
    Only the kid gets a walk cycle — the others stand at their stalls and doorways, and an idle
    breath is enough to keep them alive.
    """
    idle = load_clip(ADV, "adventurer-idle", NPC_IDLE)
    walk = load_clip(ADV_HC, "adventurer-walk", 6)
    frames = []
    for i, (name, hue, sat, val) in enumerate(NPCS):
        src = [recolor(f, hue, sat, val) for f in idle]
        if i == KID_INDEX:
            src = [shrink_child(f) for f in src]
        frames.extend(pack_frames(src))
    kid = NPCS[KID_INDEX]
    kidwalk = [shrink_child(recolor(f, kid[1], kid[2], kid[3])) for f in walk]
    frames.extend(pack_frames(kidwalk))
    sheet = clamp_colors(sheet_of(frames), 15)
    sheet.save(os.path.join(ASSETS, "npcs.png"))
    return len(frames)


def shrink_child(im):
    """Three-quarter scale, re-seated on the original baseline — the town's one child.

    Scaling around the frame's centre would leave them hovering; scaling and then dropping them back
    onto the same feet line means they stand on the same street as everyone else.
    """
    b = im.getbbox()
    if b is None:
        return im
    small = im.resize((int(im.width * 0.75), int(im.height * 0.75)), Image.NEAREST)
    out = Image.new("RGBA", im.size, (0, 0, 0, 0))
    out.alpha_composite(small, ((im.width - small.width) // 2, b[3] - small.height))
    return out


# ── UI sheet: dialog portraits + shop icons + cursor ─────────────────────────────────────────────
CELL32 = 32
# Shop stock, in frame order. The general store sells the first six, the smith the last six — see
# `STOCK` in src/town.tish, which indexes these by position.
WARES = [
    "Ui/Skill Icon/Job & Action/Potion.png",      # 0  Tonic
    "Ui/Skill Icon/Items & Weapon/Scroll.png",    # 1  Map of the Vale
    "Ui/Skill Icon/Job & Action/Dish.png",        # 2  Hot Meal
    "Ui/Skill Icon/Items & Weapon/Boot.png",      # 3  Travelling Boots
    "Ui/Skill Icon/Items & Weapon/Ring.png",      # 4  Copper Ring
    "Ui/Skill Icon/Items & Weapon/Amulet.png",    # 5  Ward Charm
    "Ui/Skill Icon/Items & Weapon/Kunai.png",     # 6  Knife
    "Ui/Skill Icon/Items & Weapon/Shuriken.png",  # 7  Throwing Star
    "Ui/Skill Icon/Items & Weapon/Armor.png",     # 8  Scale Mail
    "Ui/Skill Icon/Items & Weapon/Helmet.png",    # 9  Iron Helm
    "Ui/Skill Icon/Items & Weapon/Guard.png",     # 10 Guard's Blades
    "Ui/Skill Icon/Items & Weapon/Arrow.png",     # 11 Bundle of Arrows
]


def quantize_cell(rgba, max_colors=15):
    alpha = rgba.split()[3]
    q = rgba.convert("RGB").quantize(colors=max_colors, method=Image.MEDIANCUT,
                                     dither=Image.NONE).convert("RGBA")
    q.putalpha(alpha.point(lambda a: 255 if a >= 128 else 0))
    px = q.load()
    for y in range(q.height):
        for x in range(q.width):
            if px[x, y][3] == 0:
                px[x, y] = (0, 0, 0, 0)
    return q


def make_ui(hero_frames):
    """Portraits 0-7, ware icons 8.., cursor last — one `sheet32:` for the whole UI layer.

    packages/ui draws dialog portraits and shop icons from a single pooled sheet, so they have to
    share one import. The portraits are head crops of the SAME recoloured sprites that walk around
    town, which is the whole reason for generating the townsfolk this way: the face in the dialog box
    is provably the person you are standing in front of.
    """
    idle0 = load_clip(ADV, "adventurer-idle", 1)[0]
    icons = []
    for (name, hue, sat, val) in NPCS:
        head = recolor(idle0, hue, sat, val)
        b = head.getbbox()
        # The Adventurer's head is the top ~40% of the drawn body; crop square around it and blow it
        # up to fill the 32px cell so a portrait reads as a face and not as a distant figure.
        hw = int((b[2] - b[0]) * 1.5)
        cx = (b[0] + b[2]) // 2
        crop = head.crop((cx - hw // 2, b[1] - 2, cx + hw // 2, b[1] - 2 + hw))
        icons.append(quantize_cell(crop.resize((CELL32, CELL32), Image.NEAREST), 15))
    ware_start = len(icons)
    missing = []
    for rel in WARES:
        p = os.path.join(NINJA, rel)
        if not os.path.exists(p):
            missing.append(rel)
            icons.append(Image.new("RGBA", (CELL32, CELL32), (0, 0, 0, 0)))
            continue
        im = Image.open(p).convert("RGBA")
        cell = Image.new("RGBA", (CELL32, CELL32), (0, 0, 0, 0))
        cell.alpha_composite(im, ((CELL32 - im.width) // 2, (CELL32 - im.height) // 2))
        icons.append(quantize_cell(cell, 15))
    if missing:
        print("  WARNING: catalog icons not found: " + ", ".join(missing))
    cursor_frame = len(icons)
    icons.append(make_cursor())

    sheet = Image.new("RGBA", (CELL32 * len(icons), CELL32), (0, 0, 0, 0))
    for i, im in enumerate(icons):
        sheet.paste(im, (i * CELL32, 0))
    sheet.save(os.path.join(ASSETS, "ui32.png"))
    return ware_start, cursor_frame


def make_cursor():
    """The selection arrow the shop and the dialog choice menu point with.

    Pixel fonts have no right-pointing triangle glyph, so the cursor has to be a sprite. Its optical
    centre sits at y=6 in the cell, which is what `packages/ui` `pointerAtRow` lines up with a text row.
    """
    cur = Image.new("RGBA", (CELL32, CELL32), (0, 0, 0, 0))
    d = ImageDraw.Draw(cur)
    yl = (0xFF, 0xE0, 0x6A, 255)
    d.polygon([(1, 3), (7, 6), (1, 9)], fill=yl)
    for (rx, ry) in [(1, 3), (1, 9), (7, 6)]:
        cur.putpixel((rx, ry), (0, 0, 0, 0))
    cur.putpixel((6, 6), yl)
    return cur




# ── the levels ───────────────────────────────────────────────────────────────────────────────────
# Oakhollow is one wide streamed map on three tiers, plus a small inn interior reached through a
# door. The town is built with named helpers rather than typed out as 30 rows of 120 characters:
# at this size an ASCII block stops being readable and starts being a place for typos to hide, and
# a helper call says what a feature IS ("a ladder from the street down to the cellars") instead of
# leaving you to count columns.
#
# Tier layout, in rows:
#     0.. 3   sky headroom
#     4..12   the rooftop walk — plank catwalks strung between the roofs
#    13..19   the buildings themselves (pure scenery: you walk in FRONT of them)
#       20    the street surface
#    21..22   its foundations
#    23..27   the undertown: cellars, the forge, the dock
#    28..29   the undertown floor
TOWN_W, TOWN_H = 120, 30
STREET = 20        # row of the street surface
ROOF = 12          # row the plank catwalks run along
UNDER = 28         # row of the undertown floor


class Grid:
    """A character grid with helpers that name what they build."""

    def __init__(self, w, h):
        self.w, self.h = w, h
        self.g = [["." for _ in range(w)] for _ in range(h)]

    def put(self, c, r, ch):
        if 0 <= c < self.w and 0 <= r < self.h:
            self.g[r][c] = ch

    def get(self, c, r):
        if 0 <= c < self.w and 0 <= r < self.h:
            return self.g[r][c]
        return "."

    def hline(self, c0, c1, r, ch):
        for c in range(c0, c1 + 1):
            self.put(c, r, ch)

    def rect(self, c0, r0, c1, r1, ch):
        for r in range(r0, r1 + 1):
            self.hline(c0, c1, r, ch)

    def vline(self, c, r0, r1, ch):
        for r in range(r0, r1 + 1):
            self.put(c, r, ch)

    def ground(self, c0, c1, top, bottom, ch="#"):
        """A block of solid terrain. `to_gids` caps it with grass wherever the sky is directly above
        and fills the rest with dirt, so this only has to say WHERE the ground is."""
        self.rect(c0, top, c1, bottom, ch)

    def catwalk(self, c0, c1, r, ch="-"):
        """A one-way walkway: step up onto it from below, drop through it with Down + A. Its end
        pieces come from the run's extent, so this only has to say where the run is."""
        self.hline(c0, c1, r, ch)

    def ladder(self, c, r0, r1):
        """A ladder from row `r0` (its top) down to `r1`, capped with the climbable one-way tile.

        The cap is not decoration. A climb ends when the feet leave the last climbable tile, which
        puts the player one row above the ladder with nothing beneath them — the cap is the floor
        they land on, at both ends of the trip.
        """
        self.put(c, r0, "T")
        for r in range(r0 + 1, r1 + 1):
            self.put(c, r, "L")

    def stamp(self, c, r, block):
        """Write a small block of characters with its top-left at (c, r) — furniture, signage."""
        for dr, line in enumerate(block):
            for dc, ch in enumerate(line):
                if ch != " ":
                    self.put(c + dc, r + dr, ch)

    def scatter(self, c0, c1, r, chars, step):
        """Sprinkle decor along a row — every `step`th column, cycling through `chars`."""
        k = 0
        for c in range(c0, c1 + 1, step):
            self.put(c, r, chars[k % len(chars)])
            k += 1


def build_town():
    """The town itself. Returns (grid, building placements, entity placements, spawn)."""
    g = Grid(TOWN_W, TOWN_H)

    # ── the street ────────────────────────────────────────────────────────────────────────────────
    # One continuous surface with two hatches cut through it, which is how you reach the undertown.
    # Every tier has to stay walkable end to end: a town you cannot cross is a town nobody explores.
    g.ground(0, TOWN_W - 1, STREET, 22)
    shafts = [40, 86]
    for c in shafts:
        g.vline(c, STREET, 22, ".")

    # ── the undertown ─────────────────────────────────────────────────────────────────────────────
    # A dark brick backdrop behind it, so the cellars read as being INSIDE something rather than
    # as a hole with the sky at the far end. The backdrop is decor: you walk through it.
    for r in range(23, UNDER):
        for c in range(0, TOWN_W):
            g.put(c, r, "c" if ((c + r) % 5) else "C")
    g.ground(0, TOWN_W - 1, UNDER, TOWN_H - 1, "=")
    # Ladders down the hatches, from the street surface to the last row above the cellar floor.
    for c in shafts:
        g.ladder(c, STREET, UNDER - 1)
    # The forge end (west) is walled in masonry; the dock end (east) runs out into water. The
    # waterline is the SAME row as the cellar floor, so the shore is a step across, not a step up.
    g.rect(0, 23, 3, UNDER - 1, "=")
    g.hline(98, TOWN_W - 1, UNDER, "w")
    g.rect(98, UNDER + 1, TOWN_W - 1, TOWN_H - 1, "W")
    for c in (8, 16):
        g.stamp(c, UNDER - 2, ["12", "34"])                   # shelving behind the forge
    g.scatter(20, 34, UNDER - 1, ["a", "x", "e"], 5)
    g.scatter(60, 80, UNDER - 1, ["x", "a"], 7)
    for c in range(20, 92, 14):
        g.put(c, 24, "n")                                     # lanterns along the cellar ceiling

    # ── the buildings ─────────────────────────────────────────────────────────────────────────────
    # Eight of them, in four styles, close enough together that the street reads as a street rather
    # than as four houses in a field — and close enough that their ROOFS are the upper route. They
    # are pure scenery (see `stamp_buildings`); what you actually stand on is placed below.
    # Bottoms at row 19, so each sits square on the street without covering it.
    buildings = [
        ("house", 18, 19),    # the inn
        ("wood", 30, 19),
        ("straw", 44, 19),
        ("plant", 58, 19),
        ("straw", 66, 19),    # the general store
        ("house", 78, 19),
        ("wood", 90, 19),     # the guard house
        ("plant", 102, 19),
    ]

    # ── the rooftop walk ──────────────────────────────────────────────────────────────────────────
    # One continuous plank walk laid along the ridges and bridging the gaps between them, rather than
    # hanging in open sky — all one material, so it reads as something the town built. Two deliberate
    # breaks: 52-57 is a running jump, and the belfry columns are the ledge grab.
    #
    # Gaps are sized against the engine's own numbers rather than by eye: at run speed (2.25
    # px/frame) a jump (5 px/frame against 0.3 gravity) hangs about 33 frames and clears roughly 4.5
    # tiles, apex about 2.6 tiles.
    #
    # Nothing up here reaches down to head height — the street below stays walkable end to end.
    g.catwalk(18, 51, ROOF)
    g.catwalk(58, 67, ROOF)
    g.catwalk(72, 114, ROOF)
    # Posts holding the walk up, dropping from it into the roof below — one per building, and ONLY
    # over a building, because a post standing on nothing is worse than no post at all. Decor, not
    # collision: you walk over them.
    #
    # The masonry nine-slice is deliberately not used here. Its interior piece is a dark void — the
    # set is drawn for walls AROUND a space, which is what the interiors and the undertown use it for
    # — so a two-wide stack of it reads as a hollow frame rather than a chimney. The tileset's turned
    # wooden post is the piece that was drawn for this job.
    for (name, bc, _br) in buildings:
        g.put(bc + 3, ROOF + 1, "|")

    # The belfry — the ledge-grab target, and its height is tuned, not chosen. Standing on the walk
    # puts the box top at row ROOF-1; a jump lifts it about 2.6 tiles, to ROOF-3.6. A top edge at
    # ROOF-3 therefore passes THROUGH the grab window on the way up and again on the way down, but
    # sits just too high to land on. Put it a row higher and it is unreachable; a row lower and you
    # simply stand on it. `grabLedge` fires when a solid tile is at the box's TOP row with nothing
    # solid above, which is true for only those few frames.
    #
    # Four wide, so the nine-slice resolves to a real structure — two columns of edge and two of
    # interior — and reads as an open stone bell tower. It stands ON the general store (cols 66-73)
    # and is sunk into its roof, because a tower floating between two buildings is the same mistake
    # as a post standing on nothing.
    g.rect(68, ROOF - 3, 71, ROOF + 4, "=")

    # A tall blank wall at the west end with a walkway to leap from — somewhere to learn the wall
    # slide and the wall jump where falling costs nothing.
    g.rect(4, 6, 5, STREET - 1, "=")
    g.catwalk(7, 14, 8)
    g.put(7, 9, "|")                                          # a post under its near end
    g.put(14, 9, "|")

    # Ladders up from the street, each capped at the row of the walkway it serves.
    g.ladder(28, ROOF, STREET - 1)
    g.ladder(112, ROOF, STREET - 1)
    g.ladder(16, 8, STREET - 1)
    g.catwalk(110, 114, ROOF)                                 # a landing at the top of the east ladder

    # ── decoration on the street ──────────────────────────────────────────────────────────────────
    g.scatter(6, 118, STREET - 1, ["t", "r", ",", "*", "o", "%"], 7)
    for c in [26, 54, 86, 98]:
        g.put(c, STREET - 1, "a")
    for c in [39, 64, 76]:
        g.put(c, STREET - 1, "x")
    # Vines hanging off the undersides of the bridges.
    for c in [27, 41, 86, 99]:
        g.put(c, ROOF + 1, "v")
        g.put(c, ROOF + 2, "V")

    # ── who lives here ────────────────────────────────────────────────────────────────────────────
    # (kind, col, row) — rows are the tile the entity STANDS on, i.e. its feet are at row*16.
    #
    # The signs get their post painted into the MAP at the same cell as the entity: townsfolk carry
    # their own sprite, but a signpost that is only a collider is a conversation with thin air.
    for c in [8, 38, 44]:
        g.put(c, STREET - 1, "s")
    spawn = (10, STREET - 1)
    # Three doors, and every shopkeeper is behind one. Nobody who runs a shop stands in the street
    # (and an NPC standing ON a doorway wins the interact probe, so the door would stop working).
    # `a` is the door's destination room — see DOOR_* in src/town.tish.
    entities = [
        ("elder", 13, STREET - 1),
        ("baker", 50, STREET - 1),
        ("kid", 57, STREET - 1),
        ("guard", 94, STREET - 1),
        ("fisher", 92, UNDER - 1),
        ("sign", 8, STREET - 1),
        ("sign", 38, STREET - 1),
        ("sign", 44, STREET - 1),
        ("door", 20, STREET - 1, 0),          # the inn
        ("door", 69, STREET - 1, 1),          # the general store, in the thatched house
        ("door", 12, UNDER - 1, 2),           # the forge, down in the undertown
    ]
    return g, buildings, entities, spawn


ROOM_W, ROOM_H = 20, 11


def build_room(who, dressing):
    """One interior: a lit stone room, one screen and a bit, with its keeper and a door back out.

    Interiors are not decoration here, they are where the SHOPS live, and that is a memory decision
    as much as a design one. A shop tab lights ~500 UI tiles and wants them all at once; asking for
    that while the 120x30 town is streamed in and thirteen entities are live puts the request against
    a heap that a few thousand frames of scrolling has already carved up, and it fails. Stepping
    inside unloads the town first. It is also just how a town works.

    The Sunny Land pack has no interior art — it is an outdoor platformer — so the rooms are built
    from masonry and the apothecary shelving that were already in the atlas, lit by the undertown's
    lanterns. Eight tiles, no new art.
    """
    g = Grid(ROOM_W, ROOM_H)
    for r in range(0, ROOM_H):
        for c in range(0, ROOM_W):
            g.put(c, r, "c" if ((c + r) % 5) else "C")
    # The shell is TWO tiles thick, and that is a requirement of the nine-slice rather than a choice.
    # A one-thick wall has no masonry on either side of it, so "which of my neighbours are wall?"
    # cannot tell the room's inside from its outside, and both walls come out with the same edge
    # piece — one of them facing the wrong way. Two thick gives the resolver an outer column and an
    # inner column, and it draws the wall's edge correctly on the side you can see.
    g.rect(0, 9, ROOM_W - 1, ROOM_H - 1, "=")      # floor
    g.rect(0, 0, ROOM_W - 1, 1, "=")               # ceiling
    g.rect(0, 0, 1, 10, "=")                       # west wall
    g.rect(ROOM_W - 2, 0, ROOM_W - 1, 10, "=")     # east wall
    # Lanterns hang BELOW the ceiling, not in it — a decor tile placed inside the shell punches a
    # hole through the masonry and the nine-slice dutifully draws edges around it.
    g.put(4, 2, "N")
    g.put(10, 2, "N")
    g.put(15, 2, "N")
    for block in dressing:
        g.stamp(block[0], block[1], block[2])
    entities = [
        (who, 12, 8),
        ("door", 2, 8),
    ]
    # You arrive well clear of the door you came in by. Spawning next to it means the first A press
    # — the one you make reflexively, looking for the keeper — puts you straight back outside.
    return g, [], entities, (6, 8)


# What is stacked against each room's back wall: shelving, barrels and crates, placed as small
# character blocks (see `Grid.stamp`).
ROOM_DRESSING = {
    # The inn: a bar of shelving, casks in front of it, and a loft ladder to the rooms upstairs.
    "innkeep": [(9, 6, ["12", "34"]), (13, 6, ["12", "34"]), (6, 8, ["A"]),
                (16, 7, ["X", "A"]), (17, 8, ["X"]), (13, 5, ["+++++"]), (12, 5, ["T", "L", "L", "L"])],
    # The general store: goods stacked floor to ceiling.
    "merchant": [(13, 6, ["12", "34"]), (16, 6, ["12", "34"]), (6, 8, ["X"]),
                 (7, 8, ["A"]), (17, 8, ["X"]), (2, 7, ["A", "X"])],
    # The forge: shelving for the finished work, a barrel of quench water, stock by the door.
    "smith": [(13, 6, ["12", "34"]), (16, 7, ["12", "34"]), (6, 8, ["A"]),
              (7, 8, ["X"]), (15, 8, ["A"]), (2, 8, ["X"])],
}


# Single-tile characters. The four PIECE-SET characters — '#' earth, '=' masonry, '-' beam,
# '~' grass ledge — are not here: `to_gids` picks their tile from the cell's neighbours. A character
# with no entry at all is empty space (GID 0), which is what lets the parallax layers show through.
CHAR_TILE = {
    "c": "cellar", "C": "cellar2",
    "1": "shelfA", "2": "shelfB", "3": "shelfC", "4": "shelfD",
    "L": "ladder", "T": "ladderTop", "|": "post",
    "A": "ibarrel", "X": "icrate", "N": "ilantern",
    "w": "water", "W": "waterfill",
    "t": "tuft", "r": "reeds", "o": "rock", ",": "pebbles", "*": "bush", "%": "bush2",
    "v": "vine", "V": "vine2", "a": "barrel", "x": "crate", "e": "logend", "n": "lantern",
    "s": "sign", "d": "door",
}
# What each character DOES, by TILE NAME rather than by character, because a piece set has one
# behaviour across all nine of its pieces. The three planes are independent: a ladder is climbable
# and not solid, a beam is one-way and not solid, and a ladder cap is both climbable and one-way.
SOLID_NAMES = (["grass", "grass2", "grass3", "dirt", "dirt2", "dirt3", "dirt4", "dirt5"] +
               ["stnTL", "stnT", "stnTR", "stnL", "stnM", "stnR", "stnBL", "stnB", "stnBR"] +
               ["water", "waterfill"])
ONEWAY_NAMES = ["beamL", "beamM", "beamR", "ledgeL", "ledgeM", "ledgeR",
                "ibeamL", "ibeamM", "ibeamR", "ladderTop"]
LADDER_NAMES = ["ladder", "ladderTop"]


def stamp_buildings(grid, gids, placements, buildings):
    """Paint building tiles into empty cells only.

    Buildings are SCENERY. The street's collision is defined by the character grid, so a building is
    only allowed to fill cells that grid left empty — otherwise a roof overhanging the pavement
    would quietly delete the pavement's solidity and drop the player through the world.
    """
    for name, col, bottom in placements:
        block = buildings[name]
        rows = len(block)
        for r, line in enumerate(block):
            for c, gid in enumerate(line):
                if gid == 0:
                    continue
                tc, tr = col + c, bottom - (rows - 1) + r
                if 0 <= tc < grid.w and 0 <= tr < grid.h and grid.get(tc, tr) == ".":
                    gids[tr][tc] = gid


def to_gids(grid, tiles):
    """Turn the character grid into GIDs, choosing each piece from its NEIGHBOURS.

    This is the difference between a tileset and a texture. Three of the families are piece sets, not
    interchangeable blocks, and each cell's neighbours decide which piece belongs there:

      '='  masonry — a 3x3 nine-slice. Whether a cell is a corner, an edge or interior depends on
           which of its four neighbours are also masonry.
      '-'  wooden beam and '~' grass ledge — three-piece runs. A cell is the left end, the right end
           or middle depending on whether the run continues either side.
      '#'  earth — a grass cap wherever there is open air directly above, dirt everywhere below.

    Everything else is a single tile and maps straight through. Variant tiles (three grass caps, five
    dirt fills) are chosen by position so a long street doesn't look printed; the choice is a hash of
    the coordinates rather than a random number, so the same map always generates the same picture.
    """
    GRASS = ["grass", "grass2", "grass3"]
    DIRT = ["dirt", "dirt2", "dirt3", "dirt4", "dirt5"]
    STONE = [["stnTL", "stnT", "stnTR"], ["stnL", "stnM", "stnR"], ["stnBL", "stnB", "stnBR"]]

    def same(c, r, ch):
        return grid.get(c, r) == ch

    out = []
    for r in range(grid.h):
        row = []
        for c in range(grid.w):
            ch = grid.get(c, r)
            name = None
            if ch == "#":
                # A cap only where the sky is directly above; otherwise fill.
                name = GRASS[(c * 7 + r * 3) % len(GRASS)] if not same(c, r - 1, "#") \
                    else DIRT[(c * 5 + r * 11) % len(DIRT)]
            elif ch == "=":
                ci = 0 if not same(c - 1, r, "=") else (2 if not same(c + 1, r, "=") else 1)
                ri = 0 if not same(c, r - 1, "=") else (2 if not same(c, r + 1, "=") else 1)
                name = STONE[ri][ci]
            elif ch == "-" or ch == "~" or ch == "+":
                fam = {"-": "beam", "~": "ledge", "+": "ibeam"}[ch]
                # A ladder cap counts as part of a beam run, so a walkway reads as continuous where a
                # ladder comes up through it.
                left = same(c - 1, r, ch) or (ch in "-+" and grid.get(c - 1, r) == "T")
                right = same(c + 1, r, ch) or (ch in "-+" and grid.get(c + 1, r) == "T")
                name = fam + ("M" if (left and right) else ("R" if left else ("L" if right else "M")))
            else:
                name = CHAR_TILE.get(ch, "")
            row.append(tiles.get(name, 0))
        out.append(row)
    return out


def gid_sets(tiles):
    solid = sorted({tiles[n] for n in SOLID_NAMES if n in tiles})
    oneway = sorted({tiles[n] for n in ONEWAY_NAMES if n in tiles})
    ladder = sorted({tiles[n] for n in LADDER_NAMES if n in tiles})
    # A GID can be shared by two names after dedup (two tiles that happen to be the same image).
    # A tile that is BOTH solid and one-way would be read as one-way and become passable, so drop
    # any such overlap from the softer list.
    oneway = [g for g in oneway if g not in solid]
    return solid, oneway, ladder


# The two backdrop speeds, as Tiled parallax factors (1.0 = locked to the camera). The sky barely
# moves — a whole town's width shifts it twelve pixels — and never moves vertically at all; the
# treeline runs at three eighths across and a fifth down, which is what makes the roofs feel high
# and the cellars feel deep.
SKY_PX = (16 / 256, 0.0)
HILL_PX = (96 / 256, 48 / 256)


def emit_tsj(path, image_name, tiles, ntiles):
    """Write the Tiled tileset over the world atlas: the image, and what each tile DOES.

    Collision lives HERE rather than in the maps because the behaviour belongs to the tile, not to
    the place it was painted — a beam is one-way wherever it lands, and a map that had to re-declare
    that per cell would drift out of step with the art the first time someone moved a walkway.

    Three independent properties, which is what a side-scroller needs and a top-down map does not:
    `walkable = false` is a wall, `oneway = true` is a platform you land on but jump up through, and
    `ladder = true` is climbable. A ladder cap is one-way AND climbable; a beam is one-way and not
    solid. `tish-gba-scenepack` reads all three and bakes them into the map blob's collision planes.
    """
    solid_gids, oneway_gids, ladder_gids = gid_sets(tiles)
    flags = {}
    for gid in solid_gids:
        flags.setdefault(gid, {})["walkable"] = False
    for gid in oneway_gids:
        flags.setdefault(gid, {})["oneway"] = True
    for gid in ladder_gids:
        flags.setdefault(gid, {})["ladder"] = True
    rows = (ntiles + ATLAS_COLS - 1) // ATLAS_COLS
    doc = {
        "type": "tileset",
        "version": "1.10",
        "tiledversion": "1.10.2",
        "name": "oakhollow",
        "image": image_name,
        "imagewidth": ATLAS_COLS * TILE,
        "imageheight": rows * TILE,
        "tilewidth": TILE,
        "tileheight": TILE,
        "columns": ATLAS_COLS,
        "tilecount": rows * ATLAS_COLS,
        "margin": 0,
        "spacing": 0,
        # Atlas GIDs are 1-based (0 means "no tile here"); a Tiled tile id is 0-based. With the map's
        # firstgid at 1 the two cancel out, so a GID in the atlas is the same number in the .tmj.
        "tiles": [
            {
                "id": gid - 1,
                "properties": [{"name": k, "type": "bool", "value": v}
                               for k, v in sorted(flags[gid].items())],
            }
            for gid in sorted(flags)
        ],
    }
    with open(path, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")


def emit_tmj(path, w, h, tileset_name, layers, entities, kinds):
    """Write the level as a Tiled map — the file a person edits, and what `scene:` bakes into ROM.

    `layers` is a list of (name, priority, parallax_x, parallax_y, gid_grid), back to front, which is
    the order Tiled itself stacks them in.

    PRIORITY IS EXPLICIT ON EVERY LAYER and it is not decoration. World sprites draw at priority 2,
    and on this hardware an object beats a background of the SAME priority — so the world layer at 2
    sits behind the characters, and the same layer at 1 draws over them. (The symptom is not subtle
    and not obviously a layering problem: the player simply is not on screen, while the camera
    follows them perfectly.) That leaves priority 3 for everything behind the world, so both
    backdrops share it and the tie is broken by Tiled's own stacking order — the higher layer in the
    editor draws in front.

    PARALLAX comes from Tiled's per-layer factors, where 1.0 is locked to the camera. That is why
    the sky is a layer of this map instead of a separate image: a second tileset on screen would
    replace all 16 background palettes and repaint the town in the sky's colours.
    """
    out, lid = [], 0
    for (name, priority, px, py, gids) in layers:
        lid += 1
        out.append({
            "type": "tilelayer", "name": name, "id": lid,
            "x": 0, "y": 0, "width": w, "height": h,
            "opacity": 1, "visible": True,
            "parallaxx": px, "parallaxy": py,
            "properties": [{"name": "priority", "type": "int", "value": priority}],
            "data": [gids[r][c] for r in range(h) for c in range(w)],
        })
    lid += 1
    out.append({
        "type": "objectgroup", "name": "spawns", "id": lid,
        "x": 0, "y": 0, "opacity": 1, "visible": True, "draworder": "topdown",
        "objects": [
            {
                "id": i + 1, "name": e[0],
                "x": e[1] * TILE, "y": e[2] * TILE,
                "width": TILE, "height": TILE, "rotation": 0, "visible": True,
                "properties": [
                    {"name": "kind", "type": "int", "value": kinds.index(e[0])},
                    # `a` is the one spare number a spawn carries; on a door it names the room.
                    {"name": "a", "type": "int", "value": e[3] if len(e) > 3 else 0},
                ],
            }
            for i, e in enumerate(entities)
        ],
    })
    doc = {
        "type": "map", "version": "1.10", "tiledversion": "1.10.2",
        "orientation": "orthogonal", "renderorder": "right-down", "infinite": False,
        "width": w, "height": h, "tilewidth": TILE, "tileheight": TILE,
        "nextlayerid": lid + 1, "nextobjectid": len(entities) + 1,
        "tilesets": [{"firstgid": 1, "source": tileset_name}],
        "layers": out,
    }
    with open(path, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")


def tiled_grid(small, w, h):
    """Repeat a 16x16 backdrop grid across a full-size map layer.

    The backdrops used to be exactly 16x16 tiles because that is 256x256 px, which is the size the
    GBA wraps a background at — so one grid tiled itself forever in hardware. A map layer has no
    hardware wrap behind it, it has whatever the file says, so the repeat is baked in here. It costs
    ROM (about 7 KB a layer) and nothing at all in RAM, and the payoff is that the sky is something
    you can see and paint in the editor rather than a number in a generated table.
    """
    n = len(small)
    return [[small[r % n][c % len(small[r % n])] for c in range(w)] for r in range(h)]


# Entity kinds, in the order src/town.tish switches on them. `player` is last and is where the hero
# arrives — the level file says where you start, rather than a table beside it.
KINDS = ["elder", "merchant", "smith", "innkeep", "guard", "kid", "fisher", "baker", "sign", "door",
         "player"]


def main():
    os.makedirs(ASSETS, exist_ok=True)

    atlas = Atlas()
    sky, sky_rgb = make_sky(atlas)
    hills = make_hills(atlas)
    tiles = make_world_tiles(atlas, sky_rgb)
    buildings = make_buildings(atlas)
    ntiles = atlas.save(os.path.join(ASSETS, "tiles.png"))
    emit_tsj(os.path.join(ASSETS, "tiles.tsj"), "tiles.png", tiles, ntiles)

    hero_frames, clip_table = build_hero_frames()
    make_hero(hero_frames)
    nframes = make_npcs()
    ware_start, cursor_frame = make_ui(hero_frames)

    town, town_buildings, town_entities, town_spawn = build_town()
    town_gids = to_gids(town, tiles)
    stamp_buildings(town, town_gids, town_buildings, buildings)
    # Back to front, which is the order Tiled stacks layers in: sky furthest away, then the
    # treeline, then the town you walk on.
    emit_tmj(
        os.path.join(ASSETS, "town.tmj"), TOWN_W, TOWN_H, "tiles.tsj",
        [
            ("Sky", 3, SKY_PX[0], SKY_PX[1], tiled_grid(sky, TOWN_W, TOWN_H)),
            ("Hills", 3, HILL_PX[0], HILL_PX[1], tiled_grid(hills, TOWN_W, TOWN_H)),
            ("World", 2, 1.0, 1.0, town_gids),
        ],
        town_entities + [("player", town_spawn[0], town_spawn[1])], KINDS)

    # The three interiors, in the order src/town.tish's DOOR_* constants name them. They build as
    # LITTLE as possible — no sky, no treeline, not even a backdrop layer. That is the whole reason
    # the shops are indoors: a shop tab wants every byte of the heap the town was holding, and it
    # fails on a 192-byte allocation if it doesn't get it. The decor that would have needed a layer
    # behind it is pre-composited onto the cellar wall in the atlas instead (see `indoor`).
    rooms = []
    for (name, who) in [("inn", "innkeep"), ("store", "merchant"), ("forge", "smith")]:
        g, _, ents, spawn = build_room(who, ROOM_DRESSING[who])
        emit_tmj(os.path.join(ASSETS, name + ".tmj"), ROOM_W, ROOM_H, "tiles.tsj",
                 [("World", 2, 1.0, 1.0, to_gids(g, tiles))],
                 ents + [("player", spawn[0], spawn[1])], KINDS)
        rooms.append((name, spawn))

    print("hero.png: copied from examples/dark-hero (79 frames, ten states)")
    print(f"  (Adventurer frames still built for the townsfolk + ware icons: {len(hero_frames)})")
    print(f"npcs.png: {nframes} frames ({len(NPCS)} townsfolk x {NPC_IDLE} idle + kid walk at {len(NPCS) * NPC_IDLE})")
    print(f"ui32.png: portraits 0..{len(NPCS) - 1}, wares {ware_start}..{ware_start + len(WARES) - 1}, cursor {cursor_frame}")
    print(f"town.tmj: {TOWN_W}x{TOWN_H}, 3 layers, {len(town_entities) + 1} spawns")
    for (n, s) in rooms:
        print(f"{n + '.tmj:':10s} {ROOM_W}x{ROOM_H}, 1 layer, spawn {s}")
    solid, oneway, ladder = gid_sets(tiles)
    print(f"tiles.tsj: solid {solid}\n           oneway {oneway}\n           ladder {ladder}")


if __name__ == "__main__":
    main()
