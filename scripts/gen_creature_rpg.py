#!/usr/bin/env python3
"""Art + maps for the CREATURE RPG example (a creature-collection RPG on the Ninja Adventure pack).

Emits, into `examples/creature-rpg/`:

  assets/hero.png prof.png mom.png rival.png guard.png   sheet:  16x16 cells, 5 cols x 4 rows
  assets/town.tmj assets/lab.tmj assets/home.tmj         scene:  Tiled maps (external .tsj refs)
  src/generated/world.tish                               the tall-grass mask, derived FROM the map

KEY layout facts (verified against the sheets, not guessed):
  * `Actor/Character/<Name>/SeparateAnim/Walk.png` is 64x64 = 4 cols x 4 rows of **16x16**, laid out
    COLUMN = direction (down/up/left/right), ROW = frame. `Idle.png` is 64x16, the same 4 columns.
    We TRANSPOSE to row = facing, col = frame, because that is what `obj.play(base, len, ...)` wants
    (`base = facing * 5`).
  * (Never slice a character's combined `SpriteSheet.png` — it is not 16px aligned, and slicing it
    yields garbled, all-identical frames. `SeparateAnim/` is the only correct source.)
  * The plain `Actor/Character/` cast is drawn at 16x16, exactly the map's tile size, so these go in
    a `sheet:` (16x16 cells) — not `sheet32:`. A 32x32 cell would quadruple the VRAM per frame to
    hold the same art surrounded by transparent padding, and would need a `spriteOffset` to sit on
    its own tile. Only a hero whose weapon overhangs the cell (akari's) needs the bigger cell.

TILE facts:
  * The pack has no dedicated grid-RPG-style "tall grass" tile. A patch is TilesetField's dark-green
    fill (col 3 row 6) — visibly deeper than TilesetFloor's plain grass — with TilesetFloorDetail
    tufts scattered on top. Neither carries collision, so a patch is walkable, which is the point.
  * Tile collision is the TILESET's, and it is sparse in ways a map does not control. TilesetNature
    marks exactly two tiles solid — the trunk cell of the pink (col 1 row 20) and green (col 4 row
    20) 3x3 canopies — so a treeline stamped every 3 columns leaves two of every three cells
    WALKABLE, a border the player strolls straight through. TilesetField, TilesetFloorDetail and
    tileset_bed mark NOTHING solid. Every wall here is therefore painted on an un-rendered `Solid`
    mask layer.
    ⚠️ NOT a layer named `Collision`: that one is real, but it only ever forces cells WALKABLE, so
    painting it does nothing and leaving it blank ERASES the tileset's own collision. The first
    build of this map used it and the player walked out through the treeline, the map border and
    both houses. `Solid` is its force-solid counterpart — see crates/tish-gba-scenepack/src/tiled.rs.
  * Only TilesetHouse facades at base 0 and base 12 carry collision; bases 4, 8, 16 and 20 do not.
    A building made from one of those is a picture you walk through, which is what the lab was on
    the first pass. Both buildings here use a solid base, and `build_world` asserts it.

Run from the repo root:  python3 scripts/gen_creature_rpg.py
"""
import os
import sys

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_akari_maps import TMap, building, paint_wangset, autotile_paths   # noqa: E402

PACK = os.path.join(ROOT, "assets", "ninja-adventure")
EX = os.path.join(ROOT, "examples", "creature-rpg")
OUT = os.path.join(EX, "assets")
GEN = os.path.join(EX, "src", "generated")
os.makedirs(OUT, exist_ok=True)
os.makedirs(GEN, exist_ok=True)

CEL = 16          # both the tile size and the sprite cell size — see the header
COLS = 5          # [idle, walk0, walk1, walk2, walk3]
DIRS = 4          # rows: 0 down, 1 up, 2 left, 3 right (engine `facing()` order)

# ── spawn kinds — these integers are duplicated in src/main.tish; keep them in step ──
K_PLAYER = 0
K_DOOR = 10
K_PROF, K_MOM, K_RIVAL, K_GUARD = 20, 21, 22, 23

# scene ids, carried by a door spawn's `a` property
S_TOWN, S_LAB, S_HOME = 0, 1, 2


# ══ art ═══════════════════════════════════════════════════════════════════════════════════════

def clamp_colors(img, maxc=15):
    """Quantize to <= maxc opaque colours (the GBA's 4bpp budget), preserving transparency.

    Run on the ASSEMBLED sheet, never per frame: one quantisation is what makes the whole sheet
    share a single `Palette16`, i.e. one of the sixteen hardware palette banks."""
    img = img.convert("RGBA")
    opaque = {(r, g, b) for (r, g, b, a) in img.getdata() if a > 0}
    if len(opaque) <= maxc:
        return img
    alpha = img.getchannel("A")
    rgb = img.convert("RGB").quantize(colors=maxc, method=Image.MEDIANCUT).convert("RGBA")
    rgb.putalpha(alpha)
    return rgb


def make_char(folder, out_name):
    """`Actor/Character/<folder>/SeparateAnim/{Idle,Walk}.png` -> a 5x4 sheet of 16x16 cells.

    Source is col = direction, row = frame; output is row = facing, col = [idle, walk0..3]."""
    base = os.path.join(PACK, "Actor", "Character", folder, "SeparateAnim")
    idle = Image.open(os.path.join(base, "Idle.png")).convert("RGBA")
    walk = Image.open(os.path.join(base, "Walk.png")).convert("RGBA")
    assert walk.size == (4 * CEL, 4 * CEL), f"{folder}/Walk.png is {walk.size}, expected 64x64"
    assert idle.size[0] == 4 * CEL, f"{folder}/Idle.png is {idle.size}, expected 4 columns"

    def cell(src, d, f):
        return src.crop((d * CEL, f * CEL, d * CEL + CEL, f * CEL + CEL))

    sheet = Image.new("RGBA", (COLS * CEL, DIRS * CEL), (0, 0, 0, 0))
    for d in range(DIRS):
        sheet.paste(cell(idle, d, 0), (0, d * CEL))
        for f in range(4):
            sheet.paste(cell(walk, d, f), ((1 + f) * CEL, d * CEL))
    sheet = clamp_colors(sheet, 15)
    sheet.save(os.path.join(OUT, out_name))
    n = len({(r, g, b) for (r, g, b, a) in sheet.getdata() if a > 0})
    print(f"  {out_name:12} {folder:10} {sheet.width}x{sheet.height}  {n} colours")


# ── the creature roster ───────────────────────────────────────────────────────────────────────
#
# ⚠️⚠️ THE PACK HAS NO BACK SPRITES FOR MONSTERS. Every row of every monster sheet is a
# face-forward or side view — the four rows are animation and aspect variants, NOT four directions,
# unlike the `Actor/Character/` cast. Dump all sixteen cells of `GoldRacoon` and every one has eyes.
#
# That cost a wrong turn worth recording: measuring how much row 0 differs from row 1 establishes
# that the frames DIFFER, which is a different claim from "one of them is a rear view" — and the
# numbers looked convincing (Slime 280 of 765) while being no evidence at all.
#
# A battle shows the player's own creature from behind, so the back is AUTHORED here, by healing the
# face out of the front. That keeps the silhouette, the palette and the shading, so the pair reads
# as one creature from two angles rather than as two creatures.
#
# The face is stated PER SPECIES as a box, because no universal rule worked: luminance thresholds
# miss mid-tone eyes, and colour-frequency detection ate the kappa's shell while leaving the bat's
# face untouched.
#
# (folder, display name, face box x0,y0,x1,y1 inclusive, in the 16x16 front cell)
ROSTER = [
    ("KappaGreen", "KAPPLING",  (4, 5, 11, 11)),      # the starter
    ("Slime",      "SLIMELET",  (3, 4,  9, 10)),
    ("Mushroom2",  "CAPSHROOM", (4, 8, 11, 14)),
    ("YellowsBat", "NIGHTWING", (4, 5, 11, 11)),
    ("GoldRacoon", "GILDPAW",   (3, 5, 12, 12)),
    ("Lizard",     "SCALETAIL", (4, 2, 11,  9)),
    ("Fish",       "FINNIKIN",  (4, 4, 11, 11)),
]
# An authored back must actually change the picture. A box that missed the face would sail through
# unnoticed and put two identical sprites on the battle screen — which is the bug this replaced.
MIN_FACE_PIXELS = 20
BATTLE_CEL = 32   # 16x16 art doubled — a 16px creature on a 240x160 battle screen reads as a bug


def monster_sheet(folder):
    """The pack is inconsistent about this: 40 monsters are `SpriteSheet.png`, 26 are named after
    their folder, and Mushroom's is lowercase. Try all three rather than assume."""
    for name in (f"{folder}.png", f"{folder.lower()}.png", "SpriteSheet.png"):
        path = os.path.join(PACK, "Actor", "Monster", folder, name)
        if os.path.isfile(path):
            return path
    raise FileNotFoundError(f"no sheet for monster {folder}")


def make_back(front, box):
    """Author a rear view: heal the face box with the body colour around it, keep the silhouette."""
    im = front.convert("RGBA")
    w, h = im.size
    px = im.load()
    opaque = {(x, y) for y in range(h) for x in range(w) if px[x, y][3] > 0}

    def edge(x, y):
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            if (x + dx, y + dy) not in opaque:
                return True
        return False

    x0, y0, x1, y1 = box
    # Only interior pixels are healed — the outline runs along the silhouette and must survive, or
    # the creature loses its edge.
    todo = [(x, y) for (x, y) in opaque
            if x0 <= x <= x1 and y0 <= y <= y1 and not edge(x, y)]

    # ⚠️ Heal from the body OUTSIDE the box, and never from the outline colour. Filling iteratively
    # from whatever neighbour had already settled let the dark outline win the first ring and then
    # flood the whole face region black — every creature came out with a hole punched in it.
    from collections import Counter
    outline = Counter(px[x, y] for (x, y) in opaque if edge(x, y)).most_common(1)[0][0]
    sources = [(x, y) for (x, y) in opaque
               if not (x0 <= x <= x1 and y0 <= y <= y1) and px[x, y] != outline]

    out = im.copy()
    op = out.load()
    if sources:
        for (x, y) in todo:
            sx, sy = min(sources, key=lambda s: (s[0] - x) ** 2 + (s[1] - y) ** 2)
            op[x, y] = px[sx, sy]
    return out, len(todo)


def make_creatures(out_name):
    """The whole roster as ONE sheet of 32x32 cells: [front, back] per species, in ROSTER order.

    One sheet, not one per species, so all seven share a single `Palette16` — i.e. ONE of the GBA's
    sixteen palette banks instead of seven. That only works because `clamp_colors` runs on the
    assembled strip; quantising per species would produce seven palettes that happen to be stored
    together."""
    src = Image.new("RGBA", (len(ROSTER) * 2 * CEL, CEL), (0, 0, 0, 0))
    for i, (folder, name, box) in enumerate(ROSTER):
        sheet = Image.open(monster_sheet(folder)).convert("RGBA")
        assert sheet.size == (4 * CEL, 4 * CEL), f"{folder}: {sheet.size}, expected 64x64"
        front = sheet.crop((0, 0, CEL, CEL))
        back, healed = make_back(front, box)
        assert healed >= MIN_FACE_PIXELS, (
            f"{name} ({folder}): the face box healed only {healed} px — it is missing the face, and "
            f"the back would be the front again. Fix the box in ROSTER.")
        src.paste(front, ((i * 2 + 0) * CEL, 0))
        src.paste(back, ((i * 2 + 1) * CEL, 0))
        print(f"    {name:11} {folder:12} back authored, {healed:2} px of face healed")
    src = clamp_colors(src, 15)
    big = src.resize((src.width * 2, src.height * 2), Image.NEAREST)
    big.save(os.path.join(OUT, out_name))
    n = len({(r, g, b) for (r, g, b, a) in big.getdata() if a > 0})
    print(f"  {out_name:12} {len(ROSTER)} species x2 views  {big.width}x{big.height}  {n} colours")


# ══ maps ══════════════════════════════════════════════════════════════════════════════════════
#
# The world is ONE 40x36 map: Route 1 across the north, the town across the south, joined by a gap
# in a treeline. A creature-RPG overworld is continuous — splitting it in two would put a load screen in
# the middle of a walk, and the whole feel of stepping off the path into the grass depends on the
# town still being visible behind you.

W, H = 40, 36
GATE_ROW = 19        # the treeline that divides route from town
GATE_C0, GATE_C1 = 18, 20   # the walkable gap through it
STREET_ROW = 26      # the town's main street, 3 rows tall

# The two buildings, as (x, y, width, facade base column, door column within the facade).
# `base` picks one of TilesetHouse's five 4-column facades. Two constraints, both learned the hard
# way: base 4 is the ROOFLESS variant (a blank beige wall, which read as a crate rather than a
# building), and only bases 0 and 12 are marked solid in the tileset — a lab built from the blue
# awninged shopfront at base 16 looked far better and you could walk straight through it.
SOLID_FACADES = (0, 12)     # the only TilesetHouse bases whose .tsj marks the facade solid
LAB = (6, 22, 7, 12, 3)     # red pagoda
HOME = (25, 22, 5, 0, 2)    # orange roof
# A facade's door sits on its wall row, two below the roof. Known up front so the street can be
# carved to meet it — a door opening onto blank lawn is the thing that makes a town look unplanned.
LAB_DOOR = (LAB[0] + LAB[4], LAB[1] + 2)
HOME_DOOR = (HOME[0] + HOME[4], HOME[1] + 2)

# Tall-grass patches, as (col, row, w, h). Collected here rather than inline because they are
# emitted TWICE — once as tiles, once as the tish mask the encounter roll reads.
#
# ⚠️ None of them may touch the road (cols 18-20). The gate keeper says "keep to the dirt and
# nothing will bother you", and a patch laid across the road makes that a lie — the player would
# be dropped into an encounter with no way to have avoided it. One of these used to start at col 16
# and did exactly that. `build_world` asserts the road stays clear.
GRASS_PATCHES = [
    (5, 5, 10, 5),
    (24, 6, 11, 4),
    (6, 12, 9, 4),
    (25, 12, 10, 5),
    (13, 15, 5, 3),
]


def in_patches(c, r):
    for (x, y, w, h) in GRASS_PATCHES:
        if x <= c < x + w and y <= r < y + h:
            return True
    return False


def tree(m, layer, nature, x, y, pink=False):
    """One 3x3 full canopy from TilesetNature (pink cherry at col 0, green at col 3, both row 18)."""
    m.stamp(layer, x, y, nature, 0 if pink else 3, 18, 3, 3)


def build_world():
    m = TMap(W, H)
    floor = m.tileset("TilesetFloor")        # FIRST -> firstgid 1, which autotile_paths asserts
    field = m.tileset("TilesetField")
    detail = m.tileset("TilesetFloorDetail")
    nature = m.tileset("TilesetNature")
    house = m.tileset("TilesetHouse")
    water = m.tileset("TilesetWater")
    collide = m.tileset("Collision")

    GRASS = floor.gid(0, 12)                 # plain grass fill
    TALL = field.gid(3, 6)                   # dark-green field fill = the tall grass carpet
    TUFT = [detail.gid(0, 2), detail.gid(2, 2), detail.gid(3, 2)]   # grass / short grass / fern
    BLOCK = collide.gid(0, 0)

    ground = m.layer("Ground")
    paths = m.layer("Paths")
    props = m.layer("Props")
    blocked = m.layer("Solid")               # not rendered; any painted cell = a wall

    m.fill(ground, 0, 0, W, H, GRASS)

    # ── dirt paths, autotiled with grass-blended edges ────────────────────────────────────────
    pg = [0] * (W * H)

    def carve(x, y, w, h):
        for r in range(y, y + h):
            for c in range(x, x + w):
                if 0 <= c < W and 0 <= r < H:
                    pg[r * W + c] = 1

    # ⚠️ ONE carve for the whole road, not one per side of the treeline: carving 3..GATE_ROW-1 and
    # GATE_ROW..N left row 18 — the gate cell itself — as bare grass, a gap in the road exactly
    # where the player walks through it.
    carve(18, 3, 3, STREET_ROW)                 # the road, from the north woods down into town
    carve(6, STREET_ROW, 28, 3)                 # the town's main street
    carve(18, STREET_ROW + 3, 3, 4)             # a stub south off the street
    for (dc, dr) in (LAB_DOOR, HOME_DOOR):      # each doorstep, down to the street
        carve(dc, dr + 1, 1, STREET_ROW - dr)
    for i, g in enumerate(autotile_paths(m, floor, pg)):
        if g > 0:
            paths[i] = g

    # ── tall grass ────────────────────────────────────────────────────────────────────────────
    # Carpet + scattered tufts. The carpet is walkable (TilesetField carries no collision), which is
    # the whole point: you choose to step in.
    for r in range(H):
        for c in range(GATE_C0, GATE_C1 + 1):
            assert not in_patches(c, r), f"grass at ({c},{r}) is on the road — see GRASS_PATCHES"
    for (x, y, w, h) in GRASS_PATCHES:
        for r in range(y, y + h):
            for c in range(x, x + w):
                m.put(ground, c, r, TALL)
                m.put(paths, c, r, 0)                       # a patch never sits under dirt
                if (c * 5 + r * 3) % 4 != 0:
                    m.put(props, c, r, TUFT[(c + r) % len(TUFT)])

    # ── the treeline borders ──────────────────────────────────────────────────────────────────
    # Trees are stamped for looks and the Collision layer is painted for truth — see the header for
    # why the tileset's own collision cannot carry a border.
    def wall(x, y, w, h):
        for r in range(y, y + h):
            for c in range(x, x + w):
                m.put(blocked, c, r, BLOCK)

    for c in range(0, W, 3):
        tree(m, props, nature, c, 0, (c // 3) % 2 == 0)      # north edge of the route
        tree(m, props, nature, c, H - 3, (c // 3) % 2 == 1)  # south edge of town
    for r in range(3, H - 3, 3):
        tree(m, props, nature, 0, r, (r // 3) % 2 == 0)      # west edge
        tree(m, props, nature, W - 3, r, (r // 3) % 2 == 1)  # east edge
    wall(0, 0, W, 3)
    wall(0, H - 3, W, 3)
    wall(0, 0, 3, H)
    wall(W - 3, 0, 3, H)

    # The dividing treeline, with one gap you walk through.
    for c in range(3, W - 3, 3):
        tree(m, props, nature, c, GATE_ROW - 1, (c // 3) % 2 == 0)
    wall(3, GATE_ROW - 1, W - 6, 3)
    for c in range(GATE_C0, GATE_C1 + 1):                    # punch the gap back open
        for r in range(GATE_ROW - 1, GATE_ROW + 2):
            m.put(blocked, c, r, 0)
            m.put(props, c, r, 0)

    # ── the pond, in the town's south-east corner ─────────────────────────────────────────────
    # Kept clear of the street: on the first pass it started on row 29 and the main road ran
    # straight into the water.
    pond = {(c, r) for r in range(30, 33) for c in range(29, 34)}
    paint_wangset(m, ground, water, "grass_water", lambda c, r: (c, r) in pond, oob_terrain=False)
    for (c, r) in pond:
        m.put(paths, c, r, 0)
        m.put(blocked, c, r, BLOCK)      # water tiles do carry collision, but be explicit

    # ── the town's two buildings ──────────────────────────────────────────────────────────────
    # `building` returns the door tile: walkable, drawn on Ground so the player passes BEHIND it.
    for (x, y, w, base, door) in (LAB, HOME):
        assert base in SOLID_FACADES, f"facade base {base} is not solid in TilesetHouse.tsj"
        got = building(m, props, ground, paths, house, x, y, w, base=base, door=door)
        assert got == (x + door, y + 2), f"door moved to {got}; the street was carved to {x+door},{y+2}"
    print(f"    lab door @ {LAB_DOOR}   home door @ {HOME_DOOR}")
    for (dc, dr) in (LAB_DOOR, HOME_DOOR):
        m.put(blocked, dc, dr, 0)        # a doorway is the one cell of a facade you may enter

    # Loose props, so the town is not an empty lawn: flowers along the verges, bushes in the
    # corners, a cut-log stump by the street.
    for (c, r) in [(5, 31), (13, 30), (23, 31), (35, 25), (4, 21), (16, 33), (31, 25), (8, 33)]:
        m.put(props, c, r, detail.gid(6, 2))                      # clover
    for (c, r) in [(6, 30), (22, 33), (34, 27), (4, 25), (24, 21)]:
        m.put(props, c, r, nature.gid(4, 10))                     # rounded bush
        m.put(blocked, c, r, BLOCK)
    m.put(props, 22, STREET_ROW - 1, nature.gid(0, 8))            # cut-log stump
    m.put(blocked, 22, STREET_ROW - 1, BLOCK)

    # ── spawns ────────────────────────────────────────────────────────────────────────────────
    m.spawn(19, STREET_ROW + 4, K_PLAYER)        # on the stub south of the street, facing the town
    m.spawn(LAB_DOOR[0], LAB_DOOR[1], K_DOOR, a=S_LAB)
    m.spawn(HOME_DOOR[0], HOME_DOOR[1], K_DOOR, a=S_HOME)
    m.spawn(15, STREET_ROW + 1, K_RIVAL)         # loitering on the main street
    m.spawn(21, 21, K_GUARD)                     # just inside the gate, warning you about the grass
    m.save(os.path.join(OUT, "town.tmj"))
    return m


# Furniture blocks in tileset_bed.png, as (srcCol, srcRow, width, height).
SHELF = (7, 6, 2, 3)      # wooden bookshelf / crate stack
RACK = (9, 6, 3, 3)       # wide slatted rack
CABINET = (12, 6, 2, 3)   # narrow drawer cabinet
BED = (0, 0, 2, 3)        # tan single bed


def build_interior(name, w, h, spawns, floor_wang, wall_wang, door_col, furniture=()):
    """A one-screen house interior: framed walls, a plank/brick floor, and a doorway in the south
    wall that warps back to town. Same shape as akari's `build_house`, plus furniture and the door
    spawn.

    Furniture comes from `tileset_bed.png`, which carries NO per-tile collision, so every piece is
    also painted onto the Collision layer — otherwise you walk through the professor's bookcase."""
    m = TMap(w, h)
    floor = m.tileset("TilesetInteriorFloor")
    wall_ts = m.tileset("TilesetWallSimple")
    bed_ts = m.tileset("tileset_bed")
    collide = m.tileset("Collision")

    ground = m.layer("Ground")
    walls_l = m.layer("Walls")
    props = m.layer("Props")
    blocked = m.layer("Solid")

    walls = {(c, r) for r in range(h) for c in range(w)
             if r == 0 or r == h - 1 or c == 0 or c == w - 1}
    walls.discard((door_col, h - 1))             # the way out

    paint_wangset(m, ground, floor, floor_wang, lambda c, r: (c, r) not in walls, oob_terrain=False)
    paint_wangset(m, walls_l, wall_ts, wall_wang, lambda c, r: (c, r) in walls, oob_terrain=True)

    for (x, y, piece) in furniture:
        sc, sr, pw, ph = piece
        m.stamp(props, x, y, bed_ts, sc, sr, pw, ph)
        for r in range(y, y + ph):
            for c in range(x, x + pw):
                assert (c, r) not in walls, f"{name}: furniture at ({c},{r}) is inside a wall"
                m.put(blocked, c, r, collide.gid(0, 0))

    for (c, r, k, a) in spawns:
        assert blocked[r * w + c] == 0, f"{name}: spawn kind {k} is inside furniture at ({c},{r})"
        m.spawn(c, r, k, a=a)
    m.save(os.path.join(OUT, f"{name}.tmj"))


# ══ the tall-grass mask, derived from the map ═════════════════════════════════════════════════

def emit_world_tish():
    """Write `src/generated/world.tish` — one flat [row, c0, c1] run per grass row.

    The game needs to answer "am I standing in tall grass?" once per completed step, and the engine
    exposes no way to read a tile's GID back out of a streamed map. Deriving the answer HERE keeps
    the .tmj the single source of truth: change a patch and the table follows, because both come
    from GRASS_PATCHES on the same run."""
    runs = []
    for r in range(H):
        c = 0
        while c < W:
            if in_patches(c, r):
                c0 = c
                while c < W and in_patches(c, r):
                    c += 1
                runs.append((r, c0, c - 1))
            else:
                c += 1
    flat = [v for run in runs for v in run]
    cells = sum(c1 - c0 + 1 for (_, c0, c1) in runs)
    body = f'''// GENERATED by scripts/gen_creature_rpg.py — do not edit.
//
// The tall-grass mask for town.tmj, as {len(runs)} flat [row, firstCol, lastCol] runs covering
// {cells} tiles. The map is the source: both this table and the tiles come from GRASS_PATCHES.
//
// This is a table and not a tile read because the engine gives no way to read a GID back out of a
// streamed map — and it wants to be cheap, because `tallGrass` is called on every completed step.

export let GRASS_RUNS: i32[] = [{", ".join(str(v) for v in flat)}]

// 1 if (col, row) is tall grass, else 0. Linear over {len(runs)} runs, once per step — not per frame.
export function tallGrass(col: i32, row: i32): i32 {{
  let i: i32 = 0
  while (i < GRASS_RUNS.length) {{
    if (GRASS_RUNS[i] === row && col >= GRASS_RUNS[i + 1] && col <= GRASS_RUNS[i + 2]) {{ return 1 }}
    i = i + 3
  }}
  return 0
}}
'''
    with open(os.path.join(GEN, "world.tish"), "w") as f:
        f.write(body)
    print(f"  world.tish   {len(runs)} grass runs, {cells} tiles")


def main():
    print("CREATURE RPG — characters")
    for folder, out in [("Boy", "hero.png"), ("OldMan", "prof.png"), ("Woman", "mom.png"),
                        ("Villager2", "rival.png"), ("Knight", "guard.png")]:
        make_char(folder, out)
    make_creatures("creatures.png")

    print("CREATURE RPG — maps")
    build_world()
    build_interior("lab", 15, 10,
                   [(7, 7, K_PLAYER, 0), (7, 4, K_PROF, 0), (7, 9, K_DOOR, S_TOWN)],
                   floor_wang="tan_plank", wall_wang="cream_wall", door_col=7,
                   furniture=[(2, 1, SHELF), (4, 1, SHELF), (10, 1, RACK),
                              (1, 5, CABINET), (12, 5, CABINET)])
    build_interior("home", 13, 9,
                   [(6, 6, K_PLAYER, 0), (4, 3, K_MOM, 0), (6, 8, K_DOOR, S_TOWN)],
                   floor_wang="orange_brick", wall_wang="orange_wall", door_col=6,
                   furniture=[(9, 1, BED), (1, 1, SHELF), (7, 5, CABINET)])

    print("CREATURE RPG — generated tish")
    emit_world_tish()
    print(f"-> {OUT}")


if __name__ == "__main__":
    main()
