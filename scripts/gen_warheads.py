#!/usr/bin/env python3
"""Build examples/warheads' sprite sheets and starfield.

⚠️ THIS RUNS WITH OR WITHOUT THE ART, AND THAT IS THE POINT.

The real ships come from Foozle's CC0 Void packs, which have to be fetched by hand because itch.io's
free download needs a click-through (see assets/void/SOURCE.md). Rather than block the whole game on
that, every frame this script emits has a drawn placeholder, and the game is built and tested against
those. When `assets/void/fleet/` and `assets/void/environment/` appear, the same frames are baked
from real art instead — so dropping the zips in changes THIS FILE'S OUTPUT and nothing in the game.

⚠️ EACH SHEET CLAIMS ONE OF THE GBA'S SIXTEEN SPRITE PALETTE BANKS, and has to quantise to at most 15
colours AS A WHOLE SHEET, not per frame. That is the single hardest constraint on borrowed art, and
it is why one faction's ships are worth more than three prettier ships from three sources.

    python3 scripts/gen_warheads.py
"""

import pathlib
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
VOID = ROOT / "assets/void"
NA = ROOT / "assets/ninja-adventure"   # the blast still comes from the catalog
OUT = ROOT / "examples/warheads/assets"

# ── Frame order IS the contract with src/main.tish. ──────────────────────────────────────────────
# wh16.png, 16x16: three hulls x two teams, then the placement cursor, then a 16-way arrow.
F_HULL0 = 0          # 0..5  = class 0/1/2 for team 0, then class 0/1/2 for team 1
F_CURSOR = 6
F_ARROW0 = 7         # 7..22 = 16 headings, 0 = pointing right, rising anticlockwise on screen
N16 = 23
CELL16 = 16

# wh8.png, 8x8.
G_SHELL, G_TRAIL, G_GHOST, G_SEG_OFF, G_SEG_ON, G_SEG_HOT, G_BLIP_P0, G_BLIP_P1, \
    G_BLIP_PLANET, G_BLIP_SHELL = range(10)
# ⚠️ ONE PROJECTILE PER WARHEAD KIND, at G_SHELL0 + WK_*. A shell in flight is the only thing on
# screen during the most interesting part of a turn, and if every weapon looks identical the player
# has to read a menu to know what is coming — which is why the menu was on screen during flight at
# all. Give the shot a face and the menu can go away.
G_SHELL0 = 10          # 10..14 = BLAST, HEAVY, FRAG, DIGGER, BUILDER
N8 = 15
CELL8 = 8

# Planets. Two sheets so a small planet does not pay a 64x64 sprite's 64 tiles of VRAM for nothing:
# three small at 16 tiles each plus two large at 64 is 176 tiles, against 320 if all were 64x64.
# ⚠️ ONE SHEET, ONE CELL SIZE, ALL FIVE PLANETS.
#
# They used to be split across a 32x32 sheet and a 64x64 one to save sprite VRAM, and the game
# re-pointed a planet's sprite between them with `sprite_set_sheet`. That does not work: a sprite's
# CELL SIZE is fixed when it is created, so pointing a 64x64 sprite at a 32x32 sheet reads four cells
# as one and a planet renders as a rounded square. The split also let the drawn radius drift from the
# colliding one, which is the bug that started all this. One sheet makes the frame index the class
# index and the draw offset a constant, and agb refcounts sprite VRAM per frame in use, so only the
# sizes actually on the board are resident.
PLANET_R = [14, 17, 20, 26, 30]
CELL32, CELL64 = 32, 64

PLANET_COLS = [
    ((92, 148, 208), (52, 96, 152), (168, 208, 240)),     # ice
    ((196, 132, 72), (136, 84, 40), (232, 184, 128)),     # rust
    ((128, 176, 104), (80, 120, 64), (176, 216, 152)),    # moss
    ((176, 128, 200), (116, 80, 140), (216, 184, 232)),   # violet
    ((208, 200, 168), (144, 136, 112), (240, 236, 216)),  # bone
]

TEAM_COLS = [((120, 200, 248), (56, 120, 168)), ((248, 176, 96), (168, 108, 44))]

have_fleet = (VOID / "fleet").is_dir()
have_env = (VOID / "environment").is_dir()


def quantise(im, n=15):
    """At most `n` opaque colours plus transparency, for the WHOLE sheet."""
    alpha = im.getchannel("A").point(lambda v: 255 if v > 128 else 0)
    rgb = im.convert("RGB").quantize(colors=n, method=Image.MEDIANCUT, dither=Image.NONE)
    out = rgb.convert("RGBA")
    out.putalpha(alpha)
    return out


def fit(img, w, h):
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    img = img.copy()
    if img.width > w or img.height > h:
        img.thumbnail((w, h), Image.NEAREST)
    out.paste(img, ((w - img.width) // 2, (h - img.height) // 2), img)
    return out


# The three Kla'ed hulls, smallest to largest, chosen BY NAME so the classes map onto real ships:
# a Scout for the interceptor, a Frigate for the skirmisher, a Dreadnought for the siege hull.
# ⚠️ CHOSEN BY NATIVE SIZE AS WELL AS BY ROLE. The pack's Dreadnought is 100x72 px of panelling and
# turret detail — at the ~20 px a ship may occupy next to these planets it resamples to mush, and no
# filter fixes a 5x reduction. Scout / Frigate / Bomber stand 22, 39 and 30 px, so ONE gentle scale
# factor puts all three in range with their relative sizes intact. Order is the class order in
# ships.tish: 0 LANCE (fast), 1 TORTOISE (heavy), 2 HORNET (cluster).
VOID_HULLS = ["Scout", "Frigate", "Bomber"]


# How many animation frames each hull cell carries: 0 is the ship at rest, 1..3 its engine burning.
HULL_FRAMES = 4


def find_engine(idx, n):
    """`n` engine-flame frames for hull `idx`, in the hull's own orientation, or None.

    The pack ships the flame as its OWN layer on transparency — no hull in it — which is exactly
    what compositing wants. Frames are taken evenly across the strip because it is a loop rather
    than a ramp: there is no "more thrust" frame to build up to.
    """
    if not have_fleet:
        return None
    want = VOID_HULLS[idx].lower()
    for cand in sorted((VOID / "fleet").rglob("*Engine.png")):
        if "_previews" in str(cand).lower():
            continue
        if want in cand.name.lower():
            im = Image.open(cand).convert("RGBA")
            cells = im.width // im.height
            step = max(1, cells // n)
            return [
                im.crop((min(i * step, cells - 1) * im.height, 0,
                         min(i * step, cells - 1) * im.height + im.height, im.height))
                   .rotate(-90, expand=True)
                for i in range(n)
            ]
    return None


def find_hull_raw(idx):
    """`find_hull` without the crop — the pack's full frame, so an overlay still lines up.

    ⚠️ THE ENGINE LAYER IS REGISTERED TO THE UNCROPPED FRAME. Cropping the hull to its alpha bounds
    before compositing the flame throws away exactly the offset that made the two line up, and the
    exhaust ends up somewhere in the middle of the ship.
    """
    if not have_fleet:
        return None
    want = VOID_HULLS[idx].lower()
    for cand in sorted((VOID / "fleet").rglob("*Base.png")):
        if "_previews" in str(cand).lower():
            continue
        if want in cand.name.lower():
            return Image.open(cand).convert("RGBA").rotate(-90, expand=True)
    return None


def find_hull(idx):
    """The idx'th Kla'ed hull from the Void fleet pack, nose rotated to face RIGHT, or None.

    ⚠️ TWO THINGS THE PACK DOES NOT DO FOR YOU.
    Its ships face UP — it is drawn for a vertical shmup — and this game aims in 1/256ths with 0
    pointing right, so every hull is rotated a quarter turn or the whole fleet flies sideways.
    And the pack ships a `_Previews` tree containing same-named copies, so a glob has to exclude it
    or half the picks are contact sheets. Selecting by NAME rather than by file size also means the
    class-to-hull mapping is stated here rather than being an accident of compression.
    """
    if not have_fleet:
        return None
    want = VOID_HULLS[idx].lower()
    for cand in sorted((VOID / "fleet").rglob("*Base.png")):
        if "_previews" in str(cand).lower():
            continue
        if want in cand.name.lower():
            im = Image.open(cand).convert("RGBA").rotate(-90, expand=True)
            # ⚠️ Crop to the ship, not to the frame. The pack pads every hull into a square cell big
            # enough for its largest animation, so a Scout occupies about a third of its 64x64 sheet
            # — scaled straight into a 32px cell it lands as a dozen mushy pixels while the
            # Dreadnought fills its own. Cropping to the alpha bounding box first makes every class
            # fill the cell, which is also what makes them read as different SIZES rather than as
            # the same ship at three distances.
            box = im.getbbox()
            return im.crop(box) if box else im
    return None


def tint(img, cols):
    """Recolour a greyscale-ish hull toward a team colour, preserving its shading."""
    lit, dark = cols
    out = Image.new("RGBA", img.size, (0, 0, 0, 0))
    px, ox = img.load(), out.load()
    for y in range(img.height):
        for x in range(img.width):
            r, g, b, a = px[x, y]
            if a < 128:
                continue
            v = (r * 2 + g * 3 + b) // 6
            t = v / 255.0
            ox[x, y] = (int(dark[0] + (lit[0] - dark[0]) * t),
                        int(dark[1] + (lit[1] - dark[1]) * t),
                        int(dark[2] + (lit[2] - dark[2]) * t), 255)
    return out


def hull_shape(cls, cols, size):
    """Draw hull class `cls` at `size` px, nose to the right.

    ⚠️ SHAPE FIRST, THEN SHADE. The earlier version stacked two or three solid rectangles and the
    result read as an orange brick — "blocky" is not really about the pixel grid, it is about a
    silhouette made of axis-aligned slabs with one flat colour inside. What reads as a ship at this
    size is a TAPER (the body narrows toward the nose), a swept wing that is not a rectangle, a
    lighter top and darker underside so the hull has a round, and two or three bright pixels of
    cockpit and engine to say which way is forward.

    Everything is authored on a 32-unit grid and scaled, so the same code draws the in-game hull and
    the twice-as-large one on the select screen.
    """
    lit, dark = cols
    mid = tuple((lit[i] * 2 + dark[i]) // 3 for i in range(3))
    shade = tuple(dark[i] * 2 // 3 for i in range(3))
    hot = (255, 196, 96)
    glass = (168, 240, 255)
    k = size / 32.0
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)

    def poly(pts, fill):
        d.polygon([(x * k, y * k) for (x, y) in pts], fill=fill + (255,))

    def px(x, y, w, h, fill):
        d.rectangle([x * k, y * k, (x + w) * k - 1, (y + h) * k - 1], fill=fill + (255,))

    if cls == 0:
        # LANCE — an interceptor: long taper, hard-swept wings, one engine.
        poly([(4, 13), (14, 6), (18, 12), (10, 15)], shade)          # upper wing, swept back
        poly([(4, 19), (14, 26), (18, 20), (10, 17)], dark)          # lower wing
        poly([(29, 16), (20, 13), (8, 13), (6, 16)], lit)            # body, top half
        poly([(29, 16), (20, 18), (8, 18), (6, 16)], mid)            # body, underside
        px(11, 14, 4, 2, glass)                                      # canopy
        px(3, 14, 3, 4, hot)                                         # exhaust
        px(1, 15, 2, 2, (255, 240, 200))
    elif cls == 1:
        # TORTOISE — a siege hull: deep body, armoured prow, twin nacelles.
        poly([(28, 16), (21, 8), (9, 8), (7, 16)], lit)              # upper hull
        poly([(28, 16), (21, 24), (9, 24), (7, 16)], mid)            # lower hull
        poly([(28, 16), (23, 12), (23, 20)], shade)                  # prow armour
        px(9, 4, 10, 4, dark)                                        # top nacelle
        px(9, 24, 10, 4, dark)                                       # bottom nacelle
        px(19, 12, 4, 2, glass)
        px(5, 4, 4, 4, hot)
        px(5, 24, 4, 4, hot)
        px(10, 13, 8, 6, shade)                                      # hull plating
    else:
        # HORNET — a skirmisher: small core, outrigger pods on struts.
        poly([(27, 16), (18, 12), (10, 13), (9, 16)], lit)
        poly([(27, 16), (18, 20), (10, 19), (9, 16)], mid)
        px(13, 6, 8, 4, dark)                                        # upper pod
        px(13, 22, 8, 4, dark)                                       # lower pod
        px(16, 10, 2, 3, shade)                                      # struts
        px(16, 19, 2, 3, shade)
        px(18, 15, 4, 2, glass)
        px(10, 6, 3, 4, hot)
        px(10, 22, 3, 4, hot)
        px(6, 15, 3, 2, hot)
    return im


def placeholder_hull(cls, cols):
    return hull_shape(cls, cols, CELL32)


def arrow(step):
    """A 16-way pointer. Drawn regardless of the pack: this is UI, and it has to be unambiguous."""
    import math
    im = Image.new("RGBA", (CELL16, CELL16), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    a = 2 * math.pi * step / 16.0
    cx = cy = 7.5
    tipx, tipy = cx + 6.5 * math.cos(a), cy + 6.5 * math.sin(a)
    lx, ly = cx + 3.0 * math.cos(a + 2.4), cy + 3.0 * math.sin(a + 2.4)
    rx, ry = cx + 3.0 * math.cos(a - 2.4), cy + 3.0 * math.sin(a - 2.4)
    d.polygon([(tipx, tipy), (lx, ly), (cx, cy), (rx, ry)], fill=(240, 240, 255, 255))
    return im


def disc(size, r, cols):
    """A planet: a lit disc with a day/night terminator and a limb highlight.

    ⚠️ NO HIGHLIGHT BLOB. The first version put a filled circle of the light colour in the upper
    left as a specular highlight, which at r >= 20 is an 8px disc sitting inside a 26px disc — it
    does not read as a highlight, it reads as a moon stuck to the planet. Shading a sphere at this
    size wants a terminator and a lit limb, both of which follow the silhouette, and nothing that
    has an outline of its own.
    """
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = size // 2
    base, dark, lit = cols
    box = [c - r, c - r, c + r - 1, c + r - 1]
    d.ellipse(box, fill=base + (255,))
    # Night side: a chord, so the terminator is a curve across the disc rather than a straight edge.
    d.chord(box, 25, 205, fill=dark + (255,))
    # Lit limb along the day side only — an arc, which cannot become a shape in its own right.
    d.arc([c - r + 1, c - r + 1, c + r - 2, c + r - 2], 195, 350, fill=lit + (255,))
    d.arc(box, 190, 355, fill=lit + (255,))
    return im


# ── How big a hull is DRAWN ─────────────────────────────────────────────────────────────────────
# ⚠️ NOT "as big as the cell". Every hull used to be scaled to fill its 32x32 cell, which made a
# Scout and a Dreadnought the same size on screen, made every ship twice as large as it should be
# next to a planet, and — because the pack's Dreadnought is 72x100 — resampled a hundred rows of
# detailed art down to thirty-two with NEAREST, which is what actually made them look like blocks.
#
# ONE factor, applied to all three, so the classes keep the size relationship the artist drew: a
# Scout really is two thirds of a Frigate. 0.68 lands them at 15 / 27 / 20 px, which is about half
# what they were and reads correctly against planets of radius 40-70.
HULL_SCALE = 0.68


def downscale_rgba(src, w, h):
    """LANCZOS downscale of a sprite with a hard alpha edge, without the dark fringe.

    ⚠️ RESIZING RGBA DIRECTLY PUTS A BLACK HALO ROUND EVERY SHIP. Outside the hull the pack stores
    (0,0,0,0) — transparent BLACK — and a resampling filter mixes those zeros into the colour of
    every edge pixel, so the outline comes back darkened and the alpha comes back fractional. The
    GBA has no per-pixel alpha, so the importer then thresholds that soft edge into a ragged one.
    Two fixes, both needed: bleed the hull's own colours outward before resampling so the filter has
    real colour to average, and resample the MASK separately and threshold it back to hard.
    """
    rgb = Image.new("RGB", src.size, (0, 0, 0))
    rgb.paste(src.convert("RGB"), (0, 0), src)
    a = src.getchannel("A").point(lambda v: 255 if v >= 128 else 0)
    for _ in range(3):   # three passes carry edge colour far enough out for the filter kernel
        px, ap = rgb.load(), a.load()
        grown, na = rgb.copy(), a.copy()
        gp, np_ = grown.load(), na.load()
        for y in range(src.height):
            for x in range(src.width):
                if ap[x, y]:
                    continue
                acc, n = [0, 0, 0], 0
                for dy in (-1, 0, 1):
                    for dx in (-1, 0, 1):
                        sx, sy = x + dx, y + dy
                        if 0 <= sx < src.width and 0 <= sy < src.height and ap[sx, sy]:
                            c = px[sx, sy]
                            acc[0] += c[0]; acc[1] += c[1]; acc[2] += c[2]; n += 1
                if n:
                    gp[x, y] = (acc[0] // n, acc[1] // n, acc[2] // n)
                    np_[x, y] = 255
        rgb, a = grown, na
    mask = src.getchannel("A").resize((w, h), Image.LANCZOS).point(lambda v: 255 if v >= 110 else 0)
    out = rgb.resize((w, h), Image.LANCZOS).convert("RGBA")
    out.putalpha(mask)
    return out


def hull_cell(src, cls, size, hue=0):
    """One hull, downscaled to its class size and centred in a `size` cell, colours intact.

    ⚠️ IT KEEPS THE PACK'S OWN PALETTE. The previous version ran every hull through `tint()`, which
    flattens an image onto a two-colour ramp — fine for the procedural placeholder it was written
    for, and total destruction for real art: the Kla'ed ships carry their form in hull plating,
    canopy glass and engine glow, and a luminance ramp throws all three away. The second team is a
    HUE ROTATION instead, which preserves every shading step and still reads as another livery.
    """
    if src is None:
        return hull_shape(cls, TEAM_COLS[0 if hue == 0 else 1], size)
    w = max(1, round(src.width * HULL_SCALE))
    h = max(1, round(src.height * HULL_SCALE))
    im = downscale_rgba(src, w, h)
    if hue:
        im = hue_rotate(im, hue)
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(im, ((size - w) // 2, (size - h) // 2), im)
    return out


def hue_rotate(img, deg):
    """Rotate hue, keeping saturation and value — so the shading survives."""
    import colorsys
    out = Image.new("RGBA", img.size, (0, 0, 0, 0))
    px, ox = img.load(), out.load()
    for y in range(img.height):
        for x in range(img.width):
            r, g, b, a = px[x, y]
            if a < 96:
                continue
            hh, ll, ss = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
            nr, ng, nb = colorsys.hls_to_rgb((hh + deg / 360.0) % 1.0, ll, ss)
            ox[x, y] = (int(nr * 255), int(ng * 255), int(nb * 255), 255)
    return out


def hull_frames(idx, size, hue=0):
    """`HULL_FRAMES` cells for one class: at rest, then three phases of engine burn.

    ⚠️ ONE CROP BOX FOR ALL FOUR FRAMES, and it is the union of the hull with every flame. Cropping
    each frame to its own alpha bounds re-centres the ship whenever the flame changes length, so the
    hull jitters back and forth by a pixel or two the whole time the engine is lit — the classic
    tell of a sprite sheet cropped frame by frame instead of as a set.
    """
    src = find_hull_raw(idx)
    if src is None:
        return [hull_shape(idx, TEAM_COLS[0 if hue == 0 else 1], size)] * HULL_FRAMES
    flames = find_engine(idx, HULL_FRAMES - 1) or []
    layers = [src]
    for fl in flames:
        merged = src.copy()
        merged.alpha_composite(fl.resize(src.size)) if fl.size != src.size else merged.alpha_composite(fl)
        layers.append(merged)
    box = None
    for l in layers:
        b = l.getbbox()
        if b is None:
            continue
        box = b if box is None else (min(box[0], b[0]), min(box[1], b[1]),
                                     max(box[2], b[2]), max(box[3], b[3]))
    if box is None:
        box = (0, 0, src.width, src.height)
    out = []
    for l in layers:
        out.append(hull_cell(l.crop(box), idx, size, hue))
    while len(out) < HULL_FRAMES:
        out.append(out[-1])
    return out[:HULL_FRAMES]


def build_hulls32():
    """Four frames x three hulls per team, ONE SHEET EACH.

    ⚠️ TWO SHEETS, NOT ONE, AND THAT IS THE FIDELITY DECISION. A sheet is one GBA palette bank, so
    six cells on one sheet share fifteen colours between two liveries — about seven each, which is
    why the ships looked posterised. Split per team, each livery gets the full fifteen. The cost is
    one extra bank out of sixteen, and this example uses six.
    """
    for team, hue in ((0, 0), (1, 150)):
        sheet = Image.new("RGBA", (CELL32 * 3 * HULL_FRAMES, CELL32), (0, 0, 0, 0))
        for cls in range(3):
            for fr, cell in enumerate(hull_frames(cls, CELL32, hue)):
                sheet.paste(cell, (((cls << 2) | fr) * CELL32, 0))
        quantise(sheet).save(OUT / ("hulls%s32.png" % "AB"[team]))


def build16():
    sheet = Image.new("RGBA", (CELL16 * N16, CELL16), (0, 0, 0, 0))

    cur = Image.new("RGBA", (CELL16, CELL16), (0, 0, 0, 0))
    d = ImageDraw.Draw(cur)
    for (x0, y0, x1, y1) in ((0, 0, 4, 0), (0, 0, 0, 4), (11, 0, 15, 0), (15, 0, 15, 4),
                             (0, 11, 0, 15), (0, 15, 4, 15), (15, 11, 15, 15), (11, 15, 15, 15)):
        d.line([x0, y0, x1, y1], fill=(248, 248, 200, 255))
    sheet.paste(cur, (F_CURSOR * CELL16, 0))

    for i in range(16):
        sheet.paste(arrow(i), ((F_ARROW0 + i) * CELL16, 0))
    quantise(sheet).save(OUT / "wh16.png")
    return CELL16 * N16


def build8():
    sheet = Image.new("RGBA", (CELL8 * N8, CELL8), (0, 0, 0, 0))
    d = ImageDraw.Draw(sheet)

    # Shell.
    d.ellipse([G_SHELL * CELL8 + 2, 2, G_SHELL * CELL8 + 5, 5], fill=(255, 232, 160, 255))
    d.point([(G_SHELL * CELL8 + 3, 3), (G_SHELL * CELL8 + 4, 3)], fill=(255, 255, 255, 255))

    # ⚠️ Trail and preview dots are DRAWN, and the artillery spike is why. The obvious candidate is
    # the pack's Spark, but a particle scaled into an 8x8 cell and quantised into a shared 15-colour
    # bank becomes a one-pixel speck of an indeterminate colour, and a line of them reads as screen
    # dirt. A trail dot is a UI primitive: uniform, legible on black, identical every time.
    b = G_TRAIL * CELL8
    d.rectangle([b + 3, 2, b + 4, 5], fill=(196, 232, 255, 255))
    d.rectangle([b + 2, 3, b + 5, 4], fill=(196, 232, 255, 255))
    d.rectangle([b + 3, 3, b + 4, 4], fill=(255, 255, 255, 255))
    b = G_GHOST * CELL8
    d.rectangle([b + 3, 3, b + 4, 4], fill=(96, 128, 160, 255))

    for idx, col in ((G_SEG_OFF, (48, 56, 72)), (G_SEG_ON, (96, 200, 120)), (G_SEG_HOT, (232, 96, 72))):
        d.rectangle([idx * CELL8 + 1, 2, idx * CELL8 + 6, 5], fill=col + (255,))

    for idx, col in ((G_BLIP_P0, (120, 200, 248)), (G_BLIP_P1, (248, 176, 96)),
                     (G_BLIP_PLANET, (96, 96, 112)), (G_BLIP_SHELL, (255, 255, 255))):
        d.rectangle([idx * CELL8 + 3, 3, idx * CELL8 + 4, 4], fill=col + (255,))
    # ── One projectile per warhead kind ─────────────────────────────────────────────────────────
    # Shape AND colour differ, because at 8x8 on a starfield colour alone is not enough: a player
    # tracking a shell against a moon needs the silhouette to read too.
    b = (G_SHELL0 + 0) * CELL8            # BLAST — a plain round shot
    d.ellipse([b + 2, 2, b + 5, 5], fill=(255, 232, 160, 255))
    d.point([(b + 3, 3)], fill=(255, 255, 255, 255))

    b = (G_SHELL0 + 1) * CELL8            # HEAVY — bigger, hotter, with a dark core
    d.ellipse([b + 1, 1, b + 6, 6], fill=(255, 140, 64, 255))
    d.ellipse([b + 2, 2, b + 5, 5], fill=(255, 216, 120, 255))
    d.point([(b + 3, 3), (b + 4, 4)], fill=(255, 255, 255, 255))

    b = (G_SHELL0 + 2) * CELL8            # FRAG — a loose cluster, deliberately not a disc
    for (px, py) in ((2, 2), (5, 3), (3, 5), (4, 1), (1, 4)):
        d.point([(b + px, py)], fill=(232, 240, 255, 255))
    d.point([(b + 3, 3), (b + 4, 4)], fill=(160, 200, 255, 255))

    b = (G_SHELL0 + 3) * CELL8            # DIGGER — a drill, pointing along its flight
    d.polygon([(b + 1, 2), (b + 1, 5), (b + 6, 4), (b + 6, 3)], fill=(120, 232, 224, 255))
    d.rectangle([b + 1, 3, b + 3, 4], fill=(216, 255, 252, 255))

    b = (G_SHELL0 + 4) * CELL8            # BUILDER — a brick, because it puts ground back
    d.rectangle([b + 2, 2, b + 5, 5], fill=(140, 224, 120, 255))
    d.rectangle([b + 3, 3, b + 4, 4], fill=(232, 255, 216, 255))

    quantise(sheet).save(OUT / "wh8.png")
    return CELL8 * N8


# ── Terrain tileset ──────────────────────────────────────────────────────────────────────────────
# ⚠️ MARCHING SQUARES, BECAUSE THE RENDER GRID IS COARSER THAN THE DESTRUCTION GRID.
#
# A streamed layer's cell is 16x16 (`tilemap_stream` draws each map cell as a 2x2 block of hardware
# 8x8 tiles), but craters want to be finer than that — a 16px bite out of a 32px planet removes a
# third of it. So the GAME keeps an 8px occupancy grid and the RENDERER picks, for each 16px cell,
# the tile matching which of its four 8px quadrants are still solid. Sixteen patterns; pattern 0 is
# fully empty and never drawn, because GID 0 already means "blank" to the streamer.
#
#   bit 0 = top-left quadrant solid   bit 1 = top-right
#   bit 2 = bottom-left               bit 3 = bottom-right
#
# GID = material * 15 + pattern, for pattern 1..15. Three materials so planets keep their colours.
TERRAIN_MATS = [
    ((150, 146, 160), (96, 92, 108), (198, 196, 208)),    # grey rock
    ((172, 120, 78), (114, 76, 48), (214, 168, 122)),     # rust
    ((110, 150, 106), (70, 100, 68), (156, 196, 150)),    # moss
]
TERRAIN_PATTERNS = 15

# ⚠️ THE STARS LIVE IN THE TERRAIN TILESET, because the GBA has ONE background palette shared by
# every layer. A separate `background:` starfield plus a streamed terrain layer is two images each
# wanting all sixteen banks: `tilemap_stream` calls `set_background_palettes` and `set_backdrop`, so
# whichever loaded last wins and the other turns into a flat wash of its neighbour's colour 0. Baking
# space into the same tileset makes it one image, one palette, one layer — and the sky then scrolls
# 1:1 with the world, which for a battlefield this size is honest rather than a loss.
STAR_TILES = 4


def build_terrain():
    """49 cells laid out 15 across: 3 materials x 15 occupancy patterns, then 4 star variants."""
    rows = len(TERRAIN_MATS) + 1
    sheet = Image.new("RGB", (16 * TERRAIN_PATTERNS, 16 * rows), (0, 0, 0))
    for mi, (base, dark, lit) in enumerate(TERRAIN_MATS):
        for pat in range(1, 16):
            cell = Image.new("RGB", (16, 16), (0, 0, 0))
            d = ImageDraw.Draw(cell)
            for q in range(4):
                if not (pat >> q) & 1:
                    continue
                qx, qy = (q & 1) * 8, (q >> 1) * 8
                d.rectangle([qx, qy, qx + 7, qy + 7], fill=base)
                # A lit top edge and a dark bottom edge only where the quadrant above/below is
                # genuinely missing.
                #
                # ⚠️ A QUADRANT AT THE TILE'S OWN EDGE IS ASSUMED SOLID, NOT EMPTY. A tile cannot see
                # its neighbours, so treating "outside this tile" as empty put a lit line on the top
                # of every cell and a dark line on the bottom of every cell — and a planet came out
                # as horizontal stripes, banded once per 16px row, right through its middle. Assuming
                # solid means a fully-filled tile is flat and only interior edges are drawn; the
                # seams that are genuinely at a cell boundary go unshaded, which is invisible next to
                # banding the whole silhouette.
                above = (pat >> (q - 2)) & 1 if q >= 2 else 1
                below = (pat >> (q + 2)) & 1 if q < 2 else 1
                if not above:
                    d.rectangle([qx, qy, qx + 7, qy], fill=lit)
                if not below:
                    d.rectangle([qx, qy + 7, qx + 7, qy + 7], fill=dark)
                # Deterministic speckle, so filled ground has grain without any directional edge.
                for sx, sy in ((2, 3), (5, 6)):
                    if (pat * 7 + q * 13 + sx) % 5 == 0:
                        d.point([(qx + sx, qy + sy)], fill=dark)
            sheet.paste(cell, (16 * (pat - 1), 16 * mi))
    # Star variants. Deterministic from a hash so the committed PNG never churns, and a lattice is
    # impossible because each cell is drawn independently rather than from a walked sequence.
    def mix(v):
        v = ((v ^ (v >> 16)) * 0x7FEB352D) & 0xFFFFFFFF
        v = ((v ^ (v >> 15)) * 0x846CA68B) & 0xFFFFFFFF
        return (v ^ (v >> 16)) & 0xFFFFFFFF

    for k in range(STAR_TILES):
        cell = Image.new("RGB", (16, 16), (6, 6, 16))
        d = ImageDraw.Draw(cell)
        for j in range(k):
            h = mix(k * 97 + j * 31 + 5)
            v = 70 + (h & 63) + (110 if (h >> 11) % 5 == 0 else 0)
            d.point([((h >> 3) & 15, (h >> 19) & 15)], fill=(v, v, min(255, v + 24)))
        sheet.paste(cell, (16 * k, 16 * len(TERRAIN_MATS)))

    sheet.quantize(colors=15, method=Image.MEDIANCUT, dither=Image.NONE) \
         .convert("RGB").save(OUT / "terrain.png")


def build_boom():
    """The blast, from the pack's Explosion sheet — nine 40x40 frames, of which four read as a full
    bloom-and-fade in the frames a turn can spare.

    ⚠️ This sheet exists because warheads' first version had none: `boomSpr` was created against
    `planets32` and set to frames 0-2, which are PLANETS. Every explosion drew a planet, and at
    32px on a black field that reads as a rendering glitch rather than as a wrong sprite.
    """
    sheet = Image.new("RGBA", (CELL32 * 4, CELL32), (0, 0, 0, 0))
    src = NA / "FX/Elemental/Explosion/SpriteSheet.png"
    if src.is_file():
        boom = Image.open(src).convert("RGBA")
        for j, k in enumerate((1, 3, 5, 7)):
            sheet.paste(fit(boom.crop((k * 40, 0, k * 40 + 40, 40)), CELL32, CELL32), (j * CELL32, 0))
    else:
        for j, (r, col) in enumerate(((6, (255, 240, 200)), (11, (255, 200, 96)),
                                      (15, (232, 120, 48)), (15, (128, 56, 32)))):
            d = ImageDraw.Draw(sheet)
            c = j * CELL32 + 16
            d.ellipse([c - r, 16 - r, c + r, 16 + r], fill=col + (255,))
    quantise(sheet).save(OUT / "boom32.png")


def build_menu():
    """The three hulls at 32px for the select screen, in player-one colours."""
    sheet = Image.new("RGBA", (CELL32 * 3, CELL32), (0, 0, 0, 0))
    for cls in range(3):
        # Same renderer as the arena sheet: the hull on the selection screen has to be the hull
        # that flies, at the same scale, or the menu is advertising a different ship.
        sheet.paste(hull_cell(find_hull(cls), cls, CELL32, 0), (cls * CELL32, 0))
    quantise(sheet).save(OUT / "menu32.png")


def build_planets():
    sheet = Image.new("RGBA", (CELL64 * len(PLANET_R), CELL64), (0, 0, 0, 0))
    for i, r in enumerate(PLANET_R):
        sheet.paste(disc(CELL64, r, PLANET_COLS[i]), (i * CELL64, 0))
    quantise(sheet).save(OUT / "planets64.png")


def build_stars():
    """A 256x256 tiling starfield.

    ⚠️ It MUST tile: it is a background layer scrolled by bg_parallax, and agb wraps it. A seam in a
    star field is a vertical line of nothing marching across the sky, which reads as a rendering bug.
    Generated from a fixed LCG rather than `random` so the file is byte-stable across runs and does
    not show up as a spurious diff.
    """
    # ⚠️ THE SKY IS ALWAYS GENERATED, EVEN WITH THE PACK PRESENT — and this is a hardware limit, not
    # a preference. The Void pack's starry background is a richly detailed image whose 8x8 tiles
    # barely repeat, so `include_background_gfx` cannot dedupe it and it wants most of the GBA's
    # 64 KB of BG VRAM. Per-pixel terrain is ALSO paid for out of that budget, in dynamic tiles, and
    # the two together panic inside agb's tile allocator (vram_manager.rs) a few seconds in.
    #
    # A generated field is mostly black, so it dedupes to a handful of tiles and costs almost
    # nothing. The pack earns its place on the SHIPS, where the detail is actually looked at.
    src = None
    if False and have_env:
        cands = sorted((VOID / "environment").rglob("*.png"), key=lambda p: -p.stat().st_size)
        for c in cands:
            im = Image.open(c).convert("RGBA")
            if im.width >= 240 and im.height >= 160:
                src = im.resize((256, 256), Image.LANCZOS)
                break
    if src is None:
        src = Image.new("RGBA", (256, 256), (4, 4, 12, 255))
        d = ImageDraw.Draw(src)

        # ⚠️ CONSECUTIVE LCG DRAWS ARE NOT INDEPENDENT, and using them as an (x, y) pair puts every
        # star on a lattice. The first version of this did exactly that and the "random" sky came out
        # as a set of evenly-spaced diagonal lines — Marsaglia's theorem, visible at a glance on a
        # 240x160 screen. A proper integer hash (lowbias32) per coordinate decorrelates them, and is
        # still fully deterministic so the committed PNG does not churn between runs.
        def mix(v):
            v = ((v ^ (v >> 16)) * 0x7FEB352D) & 0xFFFFFFFF
            v = ((v ^ (v >> 15)) * 0x846CA68B) & 0xFFFFFFFF
            return (v ^ (v >> 16)) & 0xFFFFFFFF

        for i in range(260):
            x = mix(i * 3 + 1) & 255
            y = mix(i * 3 + 2) & 255
            h = mix(i * 3 + 3)
            # A few bright stars among many faint ones reads as depth; a uniform spread reads as noise.
            v = 40 + (h & 63) + (96 if (h >> 12) % 7 == 0 else 0)
            d.point([(x, y)], fill=(v, v, min(255, v + 24), 255))
    # ⚠️ A background is opaque and every BG layer on this machine SHARES ONE PALETTE, so a starfield
    # spending colours freely is spending them out of every other background's budget too. Fifteen is
    # already generous for what is mostly black.
    src.convert("RGB") \
       .quantize(colors=15, method=Image.MEDIANCUT, dither=Image.NONE) \
       .convert("RGB").save(OUT / "stars.png")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    build16()
    build8()
    build_hulls32()
    build_menu()
    build_planets()
    build_boom()
    build_terrain()
    build_stars()
    where = []
    where.append("Void fleet pack" if have_fleet else "PLACEHOLDER hulls (drawn)")
    where.append("generated (pack sky does not fit BG VRAM beside per-pixel terrain)")
    print(f"wrote examples/warheads/assets/  ships: {where[0]}   sky: {where[1]}")
    print(f"  wh16.png {N16} frames · wh8.png {N8} frames · planets64 {len(PLANET_R)} · stars 256x256")
    print("  4 sprite palette banks + 1 background palette; every sheet quantised to 15 colours")
    if not (have_fleet and have_env):
        print("\n  ⚠️ Real art not found. Drop the two CC0 zips per assets/void/SOURCE.md and re-run;")
        print("     nothing in the game changes, only this script's output.")


if __name__ == "__main__":
    main()
