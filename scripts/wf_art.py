#!/usr/bin/env python3
"""warforge's art, drawn from scratch — an 8x8-scale medieval RTS set in the Mini Medieval idiom.

Nothing here is vendored. Every pixel is placed by this file, which is the point: the game's whole
look is a build artifact, so a palette swap or a new unit is a code change rather than an asset hunt.

## The reference, and what was taken from it

v3x3d's *Mini Medieval* (and its siblings *Bountiful Bits* / *Bit Bonanza*) established the idiom
this follows — studied, not copied, since that pack is paid:

* **Tiny silhouettes over texture.** At 8px a unit is a helmet, a body and one weapon pixel. Detail
  goes into the SHAPE; anything smaller than a pixel of intent reads as noise.
* **Flat ground, sparse marks.** Grass is one flat olive with a few tufts scattered thinly, so a
  field reads as a field and not as static. Variants exist so a large area does not visibly tile.
* **A dark navy that is not black.** Outlines, the shroud and the backdrop all share it, which is
  what makes the palette feel like one set rather than several.
* **Two-tone rounded canopies** with a light rim at the top-left and a dark rim at the bottom-right.
* **Terracotta roofs on grey stone** — the one high-chroma accent, reserved for buildings so the eye
  finds them first.

## Scale, and why it is what it is

Units are **8x8** and terrain is **16x16**.

The terrain has to be 16px because `scene:` — the Tiled pipeline that streams maps larger than the
screen — packs its atlas in 16px cells (`crates/tish-gba-scenepack/src/tiled.rs`). Rebuilding that
for 8px would mean rebuilding map streaming, and an 8px map that fit in one background would be
32x32 cells: barely larger than the 30x20-cell screen, which is not a map an RTS can be played on.

So terrain tiles are drawn at 16px with 8px-SCALE detail — tuft and speckle motifs sized as if the
grid were 8 — and buildings are 2x2 cells (32x32px), which is where the reference's houses sit.
Units are genuinely 8x8, through the `sheet8:` scheme.
"""

from __future__ import annotations

from PIL import Image

TILE = 16
CEL = 8

# ── Palette ──────────────────────────────────────────────────────────────────
# Sixteen entries and no more: one 4bpp bank per sheet is a hard GBA limit, and overflowing the
# sixteen banks panics inside agb on an innocent frame.
NIGHT = (26, 24, 46)        # outlines, shroud, backdrop — a navy, never pure black
GRASS = (106, 140, 66)
GRASS_D = (78, 107, 47)
GRASS_L = (140, 173, 84)
DIRT = (122, 75, 42)
DIRT_L = (150, 96, 58)
STONE = (138, 143, 152)
STONE_D = (90, 96, 107)
ROOF = (164, 74, 56)
ROOF_L = (196, 97, 74)
WOOD = (107, 74, 43)
WATER = (58, 143, 168)
CREAM = (232, 217, 160)
GOLD = (224, 178, 60)
BLUE = (59, 111, 212)
RED = (200, 68, 47)
SKIN = (224, 168, 122)
LEAF = (96, 138, 61)
LEAF_D = (62, 96, 40)
LEAF_L = (140, 176, 88)

CLEAR = (0, 0, 0, 0)


def rgba(c, a=255):
    return (c[0], c[1], c[2], a)


class Canvas:
    """A tiny pixel canvas. Deliberately primitive — every shape here is placed by hand, because at
    this size a drawing primitive that is off by one pixel is off by an eighth of the sprite."""

    def __init__(self, w: int, h: int):
        self.img = Image.new("RGBA", (w, h), CLEAR)
        self.px = self.img.load()
        self.w, self.h = w, h

    def p(self, x: int, y: int, c) -> None:
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[x, y] = rgba(c)

    def rect(self, x, y, w, h, c) -> None:
        for j in range(y, y + h):
            for i in range(x, x + w):
                self.p(i, j, c)

    def row(self, y, x0, x1, c) -> None:
        for x in range(x0, x1 + 1):
            self.p(x, y, c)

    def col(self, x, y0, y1, c) -> None:
        for y in range(y0, y1 + 1):
            self.p(x, y, c)

    def blit(self, other: "Canvas", x: int, y: int) -> None:
        self.img.alpha_composite(other.img, (x, y))

    def paste_into(self, dst: Image.Image, x: int, y: int) -> None:
        dst.alpha_composite(self.img, (x, y))


# ── Terrain (16px, drawn at 8px detail scale) ────────────────────────────────
def t_grass(variant: int = 0) -> Canvas:
    """Flat olive with a handful of tufts. Three variants so a field does not visibly repeat."""
    c = Canvas(TILE, TILE)
    c.rect(0, 0, TILE, TILE, GRASS)
    tufts = [
        [(3, 4), (10, 9), (6, 12)],
        [(12, 3), (2, 10), (8, 6)],
        [(5, 2), (13, 11), (9, 14)],
    ][variant % 3]
    for (x, y) in tufts:
        # A tuft is three pixels: a light "v" over one dark pixel. Any bigger and the field reads
        # as noise rather than as ground.
        c.p(x, y, GRASS_L)
        c.p(x - 1, y + 1, GRASS_L)
        c.p(x + 1, y + 1, GRASS_L)
        c.p(x, y + 1, GRASS_D)
    return c


def t_dirt() -> Canvas:
    c = Canvas(TILE, TILE)
    c.rect(0, 0, TILE, TILE, DIRT)
    for (x, y) in [(2, 3), (9, 5), (5, 9), (12, 12), (7, 14), (14, 7)]:
        c.p(x, y, DIRT_L)
    for (x, y) in [(4, 6), (11, 10), (2, 13)]:
        c.p(x, y, (100, 60, 34))
    return c


def t_road() -> Canvas:
    """The walked lane: dirt with wheel-rut speckle and a hint of the grass it cuts through."""
    c = t_dirt()
    for x in range(0, TILE, 3):
        c.p(x, 1, DIRT_L)
        c.p(x + 1, TILE - 2, DIRT_L)
    return c


def t_forest() -> Canvas:
    """A canopy that tiles into a mass: light rim top-left, dark rim bottom-right, trunk below.

    ⚠️ The canopy greens are deliberately far from the ground green. A first pass used a leaf colour
    a few values off `GRASS` and the whole treeline disappeared into the field — at this size a tree
    is distinguished by VALUE, not by hue.
    """
    c = t_grass(0)
    canopy = (52, 92, 40)
    rim_l = (96, 148, 66)
    rim_d = (32, 60, 28)
    c.rect(1, 1, 14, 11, canopy)
    c.row(0, 3, 12, canopy)
    c.p(0, 2, canopy)
    c.p(15, 2, canopy)
    # Light along the top-left, dark along the bottom-right: one light source for the whole set.
    c.row(0, 4, 10, rim_l)
    c.row(1, 2, 6, rim_l)
    c.col(1, 2, 5, rim_l)
    c.row(11, 3, 13, rim_d)
    c.col(14, 4, 11, rim_d)
    for (x, y) in [(4, 3), (7, 2), (3, 6)]:
        c.p(x, y, rim_l)
    for (x, y) in [(11, 8), (12, 6), (9, 10)]:
        c.p(x, y, rim_d)
    c.rect(7, 12, 2, 3, WOOD)
    c.p(7, 12, (74, 50, 30))
    return c


def t_stump() -> Canvas:
    """What lumber leaves behind — the tile a harvested forest cell becomes."""
    c = t_grass(1)
    c.rect(5, 6, 6, 5, WOOD)
    c.rect(6, 7, 4, 3, (140, 100, 60))
    c.p(7, 8, WOOD)
    c.row(11, 5, 10, (74, 50, 30))
    return c


def t_gold() -> Canvas:
    """The mine mouth: a stone arch cut into rock with ore glinting in the dark."""
    c = Canvas(TILE, TILE)
    c.rect(0, 0, TILE, TILE, STONE_D)
    for (x, y) in [(2, 2), (12, 4), (4, 12), (13, 13)]:
        c.p(x, y, STONE)
    c.rect(4, 5, 8, 10, NIGHT)
    c.row(4, 5, 10, STONE)
    c.p(4, 5, STONE)
    c.p(11, 5, STONE)
    for (x, y) in [(6, 9), (9, 11), (7, 12)]:
        c.p(x, y, GOLD)
    c.p(6, 10, (255, 220, 120))
    return c


def t_rock() -> Canvas:
    """Impassable ridge."""
    c = Canvas(TILE, TILE)
    c.rect(0, 0, TILE, TILE, STONE_D)
    c.row(0, 0, TILE - 1, STONE)
    c.col(0, 0, TILE - 1, STONE)
    c.row(TILE - 1, 0, TILE - 1, (66, 70, 80))
    c.col(TILE - 1, 0, TILE - 1, (66, 70, 80))
    for (x, y) in [(4, 4), (10, 6), (6, 11), (12, 12)]:
        c.p(x, y, STONE)
    return c


def t_shroud() -> Canvas:
    """Unseen ground.

    ⚠️ NOT a single flat colour. A tile that is 100% one colour gets collapsed by the background
    baker onto palette index 0 — which is TRANSPARENT for a background — so the shroud drew nothing
    at all while every other part of the fog worked perfectly. Painting the same cell with a
    visible tile (rock) proved the logic was fine and the tile was not. Two near-identical navies
    keep it opaque, and at this contrast the texture is invisible in play.
    """
    c = Canvas(TILE, TILE)
    c.rect(0, 0, TILE, TILE, NIGHT)
    faint = (34, 32, 56)
    for y in range(0, TILE, 4):
        for x in range(0, TILE, 4):
            c.p(x + ((y // 4) & 1) * 2, y, faint)
    return c


def t_half() -> Canvas:
    """Explored-but-not-visible: a navy checkerboard, exactly the reference's own fog treatment."""
    c = Canvas(TILE, TILE)
    for y in range(TILE):
        for x in range(TILE):
            if (x + y) & 1:
                c.p(x, y, NIGHT)
    return c


def t_water() -> Canvas:
    c = Canvas(TILE, TILE)
    c.rect(0, 0, TILE, TILE, WATER)
    for (x, y) in [(3, 4), (10, 9), (6, 12)]:
        c.row(y, x, x + 2, (110, 190, 205))
    return c


# ── Buildings (2x2 cells = 32x32) ────────────────────────────────────────────
def b_hall(team) -> Image.Image:
    """Town hall: grey stone body, terracotta gable, a dark door and a banner in the team colour."""
    c = Canvas(TILE * 2, TILE * 2)
    c.rect(4, 14, 24, 16, STONE_D)
    c.rect(5, 15, 22, 14, STONE)
    for y in range(17, 29, 3):
        for x in range(6, 27, 4):
            c.p(x, y, STONE_D)
    # Roof: a gable that narrows toward the ridge, lighter along the top edge.
    for i in range(8):
        # i counts DOWN the roof, so the span grows as it goes: ridge at the top, eaves at the wall.
        c.row(6 + i, 12 - i, 19 + i, ROOF)
    c.row(6, 12, 19, ROOF_L)
    c.row(7, 11, 20, ROOF_L)
    c.rect(13, 21, 6, 9, NIGHT)
    c.rect(14, 22, 4, 7, WOOD)
    c.col(16, 22, 28, (74, 50, 30))
    # Team banner — the only place a faction colour appears on a building.
    c.rect(7, 18, 2, 6, team)
    c.rect(23, 18, 2, 6, team)
    return c.img


def b_barracks(team) -> Image.Image:
    """Barracks: longer, lower, timber-framed, with a weapon rack by the door."""
    c = Canvas(TILE * 2, TILE * 2)
    c.rect(2, 16, 28, 14, WOOD)
    c.rect(3, 17, 26, 12, (140, 100, 60))
    for x in range(5, 28, 5):
        c.col(x, 17, 28, WOOD)
    for i in range(7):
        c.row(9 + i, 12 - i, 19 + i, ROOF)
    c.row(9, 12, 19, ROOF_L)
    c.row(10, 11, 20, ROOF_L)
    c.rect(13, 21, 6, 9, NIGHT)
    c.rect(14, 22, 4, 7, (60, 40, 24))
    c.col(24, 19, 27, STONE)
    c.p(24, 18, CREAM)
    c.col(26, 19, 27, STONE)
    c.p(26, 18, CREAM)
    c.rect(4, 19, 2, 5, team)
    return c.img


def b_farm() -> Image.Image:
    """Farm: dense rows of crop on light tilled soil, with a store hut in the corner.

    Three passes read as furniture — a cupboard, a bookshelf, a shelf again — and the culprit was
    always the same: a brown rectangle crossed by straight dark HORIZONTAL BARS is a shelf, and no
    arrangement of crops on top of it says otherwise. The bars are gone. Soil is light and warm,
    the plants are chunky 2x2 clumps that cover most of the plot, and soil only shows BETWEEN them
    — which is what a field actually looks like from above.
    """
    c = Canvas(TILE * 2, TILE * 2)
    soil = (170, 124, 78)
    soil_d = (142, 100, 60)
    crop = (108, 156, 66)
    crop_l = (156, 196, 92)
    c.rect(2, 2, 28, 28, soil)
    for (x, y) in [(5, 6), (11, 9), (20, 5), (7, 20), (16, 24), (25, 18), (3, 13)]:
        c.p(x, y, soil_d)
    # Crop clumps: 2x2 with a light crown, staggered so the rows do not line up into a grid.
    for row, y in enumerate(range(4, 27, 6)):
        for k in range(4):
            x = 4 + k * 6 + (3 if row & 1 else 0)
            if x > 25:
                continue
            c.rect(x, y + 1, 2, 2, crop)
            c.p(x, y, crop_l)
            c.p(x + 1, y, crop_l)
    c.rect(21, 20, 9, 10, WOOD)
    c.rect(22, 23, 7, 6, (152, 114, 70))
    for i in range(4):
        c.row(20 + i, 25 - i, 25 + i, ROOF)
    c.rect(24, 25, 3, 5, NIGHT)
    return c.img


def b_camp(team) -> Image.Image:
    """The enemy war camp: a hide tent with a banner pole and a fire pit."""
    c = Canvas(TILE * 2, TILE * 2)
    for i in range(12):
        c.row(30 - i, 6 + i, 25 - i, (150, 132, 96))
    c.row(30, 5, 26, (110, 96, 70))
    for i in range(12):
        c.p(6 + i, 30 - i, (110, 96, 70))
    c.rect(13, 24, 6, 7, NIGHT)
    c.col(16, 4, 18, WOOD)
    c.rect(17, 4, 6, 5, team)
    c.p(10, 30, (240, 160, 60))
    c.p(11, 29, GOLD)
    c.p(12, 30, (240, 160, 60))
    c.p(21, 30, (240, 160, 60))
    return c.img


# ── Units (8x8, 4 facings x 5 frames) ────────────────────────────────────────
# Layout is `base = facing * 5`, idle at `base` and the walk clip at `base+1 .. base+4` — the
# contract `set_seek` and `set_chase` expect (docs: "stride = cols per direction-row, 5 for
# idle+4walk"). Facing order is the engine's: 0 down, 1 up, 2 left, 3 right.
#
# At 8x8 a figure is: 2px of head, 3px of body, 2px of legs, and ONE pixel of whatever makes this
# unit different from the last one. That last pixel is the whole design.
def unit_cell(facing: int, step: int, team, kit, tool) -> Canvas:
    """One 8x8 frame.

    The silhouette is the whole design, and it is three widths stacked:

        rows 1-2   head, TWO px wide      — narrower than the body
        rows 3-5   torso, FOUR px wide    — the shoulders are what make it a person
        rows 6-7   two legs, ONE px each  — with a gap between them

    A first pass drew the head as wide as the body and hid the legs under the torso; every unit came
    out an identical 4x6 rectangle and no amount of colour rescued it. Narrow head + gapped legs is
    what separates a figure from a block at this size.
    """
    c = Canvas(CEL, CEL)
    body = kit["body"]
    head = kit.get("head", SKIN)
    trim = kit.get("trim", body)

    # Legs. Frame 0 is idle (both planted); 1-4 are the walk, one leg forward at a time.
    swing = [0, 1, 0, -1][(step - 1) % 4] if step else 0
    lx, rx = 3, 4
    c.p(lx, 6, kit["boot"])
    c.p(rx, 6, kit["boot"])
    c.p(lx, 7, kit["boot"] if swing >= 0 else NIGHT)
    c.p(rx, 7, kit["boot"] if swing <= 0 else NIGHT)

    # Torso, with the team colour on the shoulders — one pixel of faction reads at 8px.
    c.rect(2, 3, 4, 3, body)
    c.p(2, 3, team)
    c.p(5, 3, team)
    c.p(2, 5, trim)
    c.p(5, 5, trim)

    # Head: two pixels wide, so the shoulders show either side of it.
    c.rect(3, 1, 2, 2, head)
    if kit.get("helm"):
        c.rect(3, 1, 2, 1, kit["helm"])
        # A helm reads as a helm because of the brow line, not the dome.
        c.p(2, 2, kit["helm"])
        c.p(5, 2, kit["helm"])
    if facing == 0:
        c.p(3, 2, NIGHT)
        c.p(4, 2, NIGHT)
    elif facing == 1:
        c.rect(3, 1, 2, 2, kit.get("hair", kit.get("helm", head)))
    elif facing == 2:
        c.p(3, 2, NIGHT)
    else:
        c.p(4, 2, NIGHT)

    if tool:
        tool(c, facing, step)
    return c


def tool_sword(c: Canvas, facing: int, step: int):
    # Cream blade, not steel: a grey sword against grey armour is invisible, and at 8px a weapon
    # that does not read is a wasted pixel.
    x = 6 if facing != 2 else 1
    c.col(x, 2, 4, CREAM)
    c.p(x, 5, NIGHT)


def tool_bow(c: Canvas, facing: int, step: int):
    x = 6 if facing != 2 else 1
    c.col(x, 2, 5, WOOD)
    c.p(x, 3, CREAM)  # the drawn string


def tool_hammer(c: Canvas, facing: int, step: int):
    x = 6 if facing != 2 else 1
    c.col(x, 4, 5, WOOD)
    c.p(x, 3, CREAM)


def tool_axe(c: Canvas, facing: int, step: int):
    x = 6 if facing != 2 else 1
    c.col(x, 3, 5, WOOD)
    c.p(x, 2, CREAM)
    c.p(x, 1, CREAM)


def tool_banner(c: Canvas, facing: int, step: int):
    # The hero's banner is TALLER than the figure — rank is silhouette, not just colour.
    x = 6 if facing != 2 else 1
    c.col(x, 0, 5, WOOD)
    c.p(x, 0, CREAM)
    c.p(x, 1, RED)


KITS = {
    # A peasant is the only unit with no helm — bare head, brown tunic, a hammer.
    "peasant": (dict(body=(150, 116, 74), boot=WOOD, hair=(90, 62, 38), trim=(120, 92, 58)),
                tool_hammer),
    "footman": (dict(body=STONE, boot=STONE_D, helm=STONE, trim=STONE_D), tool_sword),
    "archer": (dict(body=(76, 122, 66), boot=WOOD, hair=(70, 48, 30), trim=(56, 94, 50)),
               tool_bow),
    # The hero wears gold and carries the banner: at 8px, rank is colour and a taller silhouette.
    "hero": (dict(body=GOLD, boot=WOOD, helm=(240, 208, 110), trim=(180, 130, 40)), tool_banner),
    "grunt": (dict(body=(112, 138, 76), boot=(70, 54, 36), head=(126, 156, 84),
                   hair=(88, 110, 60), trim=(84, 106, 56)), tool_axe),
    "raider": (dict(body=(86, 86, 96), boot=(56, 56, 64), head=CREAM, hair=(70, 70, 80),
                    trim=(60, 60, 70)), tool_sword),
    # The chieftain: dark iron, red trim, a big axe. The one enemy the player must find.
    "chief": (dict(body=(74, 62, 70), boot=(46, 40, 46), helm=(120, 50, 44), trim=RED), tool_axe),
}


def build_unit_sheet(kind: str, team) -> Image.Image:
    kit, tool = KITS[kind]
    img = Image.new("RGBA", (CEL * 5, CEL * 4), CLEAR)
    for facing in range(4):
        for f in range(5):
            cell = unit_cell(facing, f, team, kit, tool)
            cell.paste_into(img, f * CEL, facing * CEL)
    return img


# ── Cursor ───────────────────────────────────────────────────────────────────
def cursor_sheet() -> Image.Image:
    """The command cursor: four corner brackets around a 16px cell, in two sizes for a slow pulse.

    ⚠️ It must NOT look like a unit. The first version reused a hero sprite as the cursor, and the
    game read as "walk one character around" instead of "command an army" — which is the single
    most important pixel decision in the example, arrived at by accident and corrected on sight.
    """
    img = Image.new("RGBA", (TILE * 2, TILE), CLEAR)
    for f, inset in enumerate((0, 1)):
        c = Canvas(TILE, TILE)
        a, b = inset, TILE - 1 - inset
        arm = 5 - inset
        for i in range(arm):
            for (x, y) in ((a + i, a), (a, a + i), (b - i, a), (b, a + i),
                           (a + i, b), (a, b - i), (b - i, b), (b, b - i)):
                c.p(x, y, CREAM)
        # A dark pixel inside each corner keeps the bracket readable over pale ground.
        for (x, y) in ((a + 1, a + 1), (b - 1, a + 1), (a + 1, b - 1), (b - 1, b - 1)):
            c.p(x, y, NIGHT)
        c.paste_into(img, f * TILE, 0)
    return img


# ── More buildings ───────────────────────────────────────────────────────────
def b_keep(team) -> Image.Image:
    """The upgraded hall: taller stone, crenellations, two banners. It must read as *the same
    building, promoted* — same footprint and palette, more mass and a battlement instead of a gable."""
    c = Canvas(TILE * 2, TILE * 2)
    c.rect(3, 10, 26, 20, STONE_D)
    c.rect(4, 11, 24, 18, STONE)
    for y in range(14, 29, 4):
        for x in range(6, 27, 5):
            c.p(x, y, STONE_D)
    # Battlement: alternating merlons along the top, which is what says "keep" rather than "house".
    for x in range(3, 29, 4):
        c.rect(x, 6, 3, 5, STONE)
        c.rect(x, 6, 3, 1, CREAM)
        c.rect(x + 3, 8, 1, 3, STONE_D)
    c.rect(13, 20, 6, 10, NIGHT)
    c.rect(14, 21, 4, 8, WOOD)
    c.rect(5, 13, 2, 6, team)
    c.rect(25, 13, 2, 6, team)
    return c.img


def b_tower(team) -> Image.Image:
    """Guard tower: a narrow stone shaft with a crenellated head and an arrow slit."""
    c = Canvas(TILE * 2, TILE * 2)
    c.rect(9, 12, 14, 19, STONE_D)
    c.rect(10, 13, 12, 17, STONE)
    for x in range(8, 25, 4):
        c.rect(x, 7, 3, 6, STONE)
        c.rect(x, 7, 3, 1, CREAM)
    c.rect(8, 11, 16, 2, STONE_D)
    c.rect(14, 16, 4, 7, NIGHT)   # arrow slit
    c.rect(15, 17, 2, 4, (60, 56, 70))
    c.rect(15, 25, 3, 6, WOOD)     # door
    c.rect(11, 14, 2, 3, team)
    return c.img


def b_smith(team) -> Image.Image:
    """Blacksmith: timber shed, stone chimney, and a lit forge — the glow is the whole read."""
    c = Canvas(TILE * 2, TILE * 2)
    c.rect(2, 14, 28, 16, WOOD)
    c.rect(3, 15, 26, 14, (140, 100, 60))
    for x in range(5, 29, 6):
        c.col(x, 15, 28, WOOD)
    for i in range(7):
        c.row(7 + i, 12 - i, 19 + i, ROOF)
    c.row(7, 12, 19, ROOF_L)
    c.rect(21, 4, 5, 12, STONE_D)   # chimney
    c.rect(22, 5, 3, 10, STONE)
    c.p(23, 3, (90, 90, 100))
    c.p(23, 2, (120, 120, 130))
    c.rect(7, 20, 8, 10, NIGHT)     # forge mouth
    c.rect(8, 22, 6, 7, (200, 90, 40))
    c.rect(9, 24, 4, 4, (250, 180, 70))
    c.rect(10, 25, 2, 2, CREAM)
    c.rect(19, 22, 6, 7, (60, 40, 24))  # anvil bay
    c.rect(20, 24, 4, 2, STONE)
    c.rect(3, 16, 2, 5, team)
    return c.img


def t_scaffold() -> Canvas:
    """A construction site: bare soil, a timber frame, and a plank leaning on it.

    One tile serves every building kind. Warcraft draws a kind-specific scaffold; at 16px on a 2x2
    footprint that distinction is invisible, and a single tile keeps the tileset (and the palette)
    from growing for nothing.
    """
    c = Canvas(TILE, TILE)
    c.rect(0, 0, TILE, TILE, (150, 108, 66))
    for (x, y) in [(3, 4), (10, 6), (6, 11), (12, 12)]:
        c.p(x, y, (126, 88, 54))
    # Timber frame: two uprights and a cross-brace, in the same wood the buildings use.
    c.col(3, 2, 13, WOOD)
    c.col(12, 2, 13, WOOD)
    c.row(3, 3, 12, WOOD)
    c.row(9, 3, 12, (140, 100, 60))
    for i in range(8):
        c.p(4 + i, 11 - i, (140, 100, 60))
    c.p(3, 2, CREAM)
    c.p(12, 2, CREAM)
    return c


def bar_sheet() -> Image.Image:
    """Overhead HP bars: nine 8x8 frames, fill 0..8, coloured by how hurt the thing is.

    Baked frames, not a drawn rectangle. A bar rendered per unit per frame is a pixel loop and a
    VRAM write; a baked frame is one `sprite_set_frame`, and only when the step actually changes.
    Green/yellow/red is read at a glance without reading a number, which is the whole job at 8px.
    """
    img = Image.new("RGBA", (CEL * 9, CEL), CLEAR)
    for f in range(9):
        c = Canvas(CEL, CEL)
        # Track: a dark bed so a partly-empty bar still reads as a bar and not as a stray pixel.
        c.row(1, 0, 7, NIGHT)
        c.row(2, 0, 7, NIGHT)
        c.row(3, 0, 7, NIGHT)
        if f > 0:
            col = (86, 176, 72)
            if f <= 4:
                col = (226, 184, 62)
            if f <= 2:
                col = (206, 66, 52)
            for x in range(f):
                c.p(x, 2, col)
                c.p(x, 1, (min(col[0] + 40, 255), min(col[1] + 40, 255), min(col[2] + 40, 255)))
        c.paste_into(img, f * CEL, 0)
    return img


def icon_sheet() -> Image.Image:
    """16x16 command icons, one frame per command, in `cmd.tish`'s command order.

    At 16px an icon is a silhouette plus one accent colour — the same rule the units follow. Each
    sits on a dark plate so it reads on any panel background without needing its own border.
    """
    names = ["peasant", "footman", "archer", "farm", "barracks", "tower", "smith",
             "keep", "weapon", "armor", "mine", "chop", "stop", "attack"]
    img = Image.new("RGBA", (TILE * len(names), TILE), CLEAR)
    for i, nm in enumerate(names):
        c = Canvas(TILE, TILE)
        c.rect(0, 0, TILE, TILE, (34, 38, 58))
        c.row(0, 0, 15, (58, 64, 88))
        c.col(0, 0, 15, (58, 64, 88))
        if nm in ("peasant", "footman", "archer"):
            body = {"peasant": (150, 116, 74), "footman": STONE, "archer": (76, 122, 66)}[nm]
            c.rect(6, 3, 4, 3, SKIN if nm != "footman" else STONE)
            c.rect(5, 6, 6, 5, body)
            c.rect(6, 11, 2, 3, WOOD)
            c.rect(8, 11, 2, 3, WOOD)
            if nm == "footman":
                c.col(12, 3, 9, CREAM)
            if nm == "archer":
                c.col(12, 3, 10, WOOD)
                c.col(11, 4, 9, CREAM)
            if nm == "peasant":
                c.col(12, 5, 9, WOOD)
                c.rect(11, 3, 3, 2, STONE)
        elif nm in ("farm", "barracks", "tower", "keep", "smith"):
            # Buildings: a roof shape over a body, matching each building's own silhouette.
            if nm == "farm":
                c.rect(3, 6, 10, 7, (170, 124, 78))
                for y in (7, 10):
                    for x in range(4, 13, 3):
                        c.p(x, y, (108, 156, 66))
            elif nm == "tower":
                c.rect(6, 5, 4, 9, STONE)
                for x in range(5, 12, 2):
                    c.p(x, 3, STONE)
                    c.p(x, 4, STONE)
                c.rect(7, 8, 2, 3, NIGHT)
            elif nm == "keep":
                c.rect(3, 6, 10, 8, STONE)
                for x in range(3, 13, 3):
                    c.rect(x, 3, 2, 3, STONE)
                c.rect(7, 10, 3, 4, NIGHT)
            elif nm == "smith":
                c.rect(3, 7, 10, 7, WOOD)
                for i2 in range(4):
                    c.row(4 + i2, 7 - i2, 8 + i2, ROOF)
                c.rect(5, 9, 4, 4, (240, 160, 60))
                c.rect(10, 4, 3, 5, STONE)
            else:  # barracks
                c.rect(3, 7, 10, 7, WOOD)
                for i2 in range(4):
                    c.row(4 + i2, 7 - i2, 8 + i2, ROOF)
                c.rect(7, 10, 3, 4, NIGHT)
        elif nm == "weapon":
            c.col(8, 2, 10, CREAM)
            c.p(7, 3, CREAM)
            c.p(9, 3, CREAM)
            c.rect(6, 10, 5, 2, WOOD)
            c.rect(7, 12, 3, 2, STONE)
        elif nm == "armor":
            c.rect(5, 3, 6, 6, STONE)
            c.rect(4, 4, 8, 5, STONE)
            for i2 in range(5):
                c.row(9 + i2, 5 + i2, 10 - i2, STONE)
            c.p(7, 5, CREAM)
            c.p(8, 5, CREAM)
        elif nm == "mine":
            c.rect(3, 9, 10, 5, STONE_D)
            c.rect(5, 5, 6, 5, NIGHT)
            c.row(5, 5, 10, STONE)
            c.p(6, 7, GOLD)
            c.p(9, 8, GOLD)
            c.p(7, 8, (255, 220, 120))
        elif nm == "chop":
            c.col(9, 3, 12, WOOD)
            c.rect(5, 2, 5, 4, STONE)
            c.p(4, 3, CREAM)
            c.p(4, 4, CREAM)
        elif nm == "stop":
            c.rect(4, 4, 8, 8, (206, 66, 52))
            c.rect(5, 5, 6, 6, (240, 110, 90))
            c.rect(6, 7, 4, 2, NIGHT)
        elif nm == "attack":
            # Crossed blades: the one icon that must read instantly.
            for i2 in range(9):
                c.p(3 + i2, 3 + i2, CREAM)
                c.p(12 - i2, 3 + i2, CREAM)
            c.p(3, 12, WOOD)
            c.p(12, 12, WOOD)
        c.paste_into(img, i * TILE, 0)
    return img
