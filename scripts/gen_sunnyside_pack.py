#!/usr/bin/env python3
"""Bake the Sunnyside World pack into GBA-ready sheets and data tables.

The source pack is NOT vendored (its license forbids redistributing the source
files) — download it from https://danieldiggle.itch.io/sunnyside and point
SUNNYSIDE_SRC at a directory laid out as:
  $SUNNYSIDE_SRC/raw/            the pack as shipped (strips, tileset, elements)
  $SUNNYSIDE_SRC/gm/             the pack's GameMaker Room1 + tileset descriptors

Outputs (committed, deterministic):
  assets/sunnyside/baked/char_player.png     64x32 frames, every player action
  assets/sunnyside/baked/char_<npc>.png      32x32 frames, idle+walk per NPC
  assets/sunnyside/baked/world_tiles.png     bgtiles atlas: terrain 3x3 blocks,
                                             farm/crop tiles, building stamps
  assets/sunnyside/baked/world_tiles.json    atlas layout report (for humans)
  assets/sunnyside/catalog/autotile.json     learned mask->tile tables per material
  examples/sunnyside/src/data_anim.tish      typed frame tables (shared by sunnyside-*)
  examples/sunnyside/src/data_world.tish     typed GID tables for the atlas

Terrain transitions use Sunnyside's own model, learned from the pack's
GameMaker example room (see learn_fill_and_lips): a material cell is plain
fill, and the NEIGHBOURING cell carries a mostly-transparent "lip" overlay
(grass fringe hanging over the water, path edge spilling onto grass).  The
baker splits each terrain layer's cells by tile opacity, votes lip tiles under
the mask of which orthogonal neighbours are material cells (U=1 D=2 L=4 R=8,
diagonal-only keys 16..19), and emits 20-entry mask->gid tables of PRE-
COMPOSITED opaque tiles — so a game paints its whole map in tish with plain
table lookups and hands it over in one tilemap_stream call.

Character frames: the pack draws everything on a 96x64 canvas facing right.
Feet sit at source y=39.  We crop a fixed window per sheet size so the feet
land on the same output row in every action (left facing = hflip at runtime).
"""
import json
import os
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC = Path(os.environ.get("SUNNYSIDE_SRC", ROOT / "assets/sunnyside"))
RAW = SRC / "raw"
GM = SRC / "gm"
BAKED = ROOT / "assets/sunnyside/baked"
CATALOG = ROOT / "assets/sunnyside/catalog"
EX_SRC = ROOT / "examples/sunnyside/src"

TILE = 16
TS_COLS = 64  # spr_tileset_sunnysideworld_16px.png is a 64x64 grid of 16px tiles

# ---------------------------------------------------------------- characters

# action -> (dirname, strip frame count).  Frame counts are in the filenames;
# they are re-checked against the actual image width.
PLAYER_ACTIONS = [
    ("idle", "IDLE", 9),
    ("walk", "WALKING", 8),
    ("run", "RUN", 8),
    ("dig", "DIG", 13),
    ("water", "WATERING", 5),
    ("axe", "AXE", 10),
    ("attack", "ATTACK", 10),
    ("carry", "CARRY", 8),
    ("doing", "DOING", 8),
    ("hurt", "HURT", 8),
    # fishing: the rod line extends below the feet and clips at the frame
    # edge — it lands on the water tile the player faces, so nothing is lost
    ("cast", "CASTING", 15),
    ("reel", "REELING", 13),
    ("caught", "CAUGHT", 10),
]
NPC_ACTIONS = [("idle", "IDLE", 9), ("walk", "WALKING", 8)]

# 64x32 player window: covers every base+tools frame except 2px of the attack
# swing top and the watering-can dribble below the feet (both invisible).
P_WIN = (26, 10, 90, 42)   # left, top, right, bottom in the 96x64 canvas
# 32x32 npc window (idle/walk bodies span x43..57)
N_WIN = (33, 10, 65, 42)

HUMAN = RAW / "Characters/Human"
GOBLIN = RAW / "Characters/Goblin/PNG"


def load_strip(path: Path, frames: int) -> list[Image.Image]:
    im = Image.open(path).convert("RGBA")
    fw = im.width // frames
    assert fw * frames == im.width, f"{path}: width {im.width} not /{frames}"
    return [im.crop((i * fw, 0, (i + 1) * fw, im.height)) for i in range(frames)]


def human_strip(action_dir: str, layer: str, frames: int):
    d = HUMAN / action_dir
    hits = sorted(d.glob(f"{layer}_*strip*.png"))
    if not hits:
        return None
    return load_strip(hits[0], frames)


def compose_human(action_dir: str, frames: int, hair: str, tools: bool) -> list[Image.Image]:
    layers = ["base"] + ([hair] if hair else []) + (["tools"] if tools else [])
    out = None
    for layer in layers:
        strip = human_strip(action_dir, layer, frames)
        if strip is None:
            continue
        if out is None:
            out = [f.copy() for f in strip]
        else:
            out = [Image.alpha_composite(a, b) for a, b in zip(out, strip)]
    assert out, f"no layers for {action_dir}"
    return out


def bake_char_sheet(name: str, actions, window, cell, hair: str, tools: bool,
                    source: str, anim_rows: dict):
    """Pack action frames sequentially into a grid-of-`cell` PNG."""
    cw, ch = cell
    frames_out: list[Image.Image] = []
    table = []
    for key, action_dir, n in actions:
        if source == "human":
            frames = compose_human(action_dir, n, hair, tools)
        else:  # goblin: single flat layer, lowercase names, 'walk' not 'walking'
            stem = {"IDLE": "idle", "WALKING": "walk"}.get(action_dir, action_dir.lower())
            hits = sorted(GOBLIN.glob(f"spr_{stem}_strip*.png"))
            assert hits, f"goblin strip missing for {action_dir}"
            # goblin strip names overstate the count; frames are 96px wide
            im = Image.open(hits[0])
            n = im.width // 96
            frames = load_strip(hits[0], n)
        start = len(frames_out)
        for f in frames:
            src = f.crop(window)
            assert src.size == (cw, ch), (name, key, src.size)
            frames_out.append(src)
        table.append((key, start, n))
    cols = 8
    rows = (len(frames_out) + cols - 1) // cols
    sheet = Image.new("RGBA", (cols * cw, rows * ch))
    for i, f in enumerate(frames_out):
        sheet.paste(f, ((i % cols) * cw, (i // cols) * ch))
    sheet = quantize15(sheet)
    quantize_report(name, sheet)
    out = BAKED / f"char_{name}.png"
    sheet.save(out)
    anim_rows[name] = table
    print(f"  {out.relative_to(ROOT)}: {len(frames_out)} frames of {cw}x{ch}")


def quantize15(sheet: Image.Image) -> Image.Image:
    return quantize_n(sheet, 15)


def quantize_n(sheet: Image.Image, n: int) -> Image.Image:
    """Reduce to <=n opaque colours + binary transparency."""
    alpha = sheet.getchannel("A").point(lambda a: 255 if a >= 128 else 0)
    rgb = sheet.convert("RGB")
    q = rgb.quantize(colors=n, method=Image.MEDIANCUT, dither=Image.Dither.NONE)
    out = q.convert("RGBA")
    out.putalpha(alpha)
    # force fully-transparent pixels to a single RGBA so the packer dedups them
    px = out.load()
    for y in range(out.height):
        for x in range(out.width):
            if px[x, y][3] == 0:
                px[x, y] = (0, 0, 0, 0)
    return out


def quantize_report(name: str, sheet: Image.Image):
    colors = {p for p in sheet.getdata() if p[3] > 0}
    if len(colors) > 15:
        print(f"  !! {name}: {len(colors)} opaque colours (>15, agb will quantize)")


# ---------------------------------------------------------------- GM decode

def load_yy(path: Path):
    txt = path.read_text()
    txt = re.sub(r",(\s*[}\]])", r"\1", txt)
    return json.loads(txt)


EMPTY = -2147483648
IDX_MASK = 0x3FFFF


def decode_layer(layer) -> list[int]:
    """GM TileCompressedData -> flat list of raw tile words (EMPTY for blank)."""
    t = layer["tiles"]
    if "TileSerialiseData" in t:
        return list(t["TileSerialiseData"])
    data = t["TileCompressedData"]
    out: list[int] = []
    i = 0
    while i < len(data):
        n = data[i]
        i += 1
        if n < 0:  # run of -n copies of the next word
            out.extend([data[i]] * (-n))
            i += 1
        else:  # n literal words
            out.extend(data[i:i + n])
            i += n
    return out


def word_tile(word: int) -> int:
    """Tile index within the GM tileset, or -1 for blank.

    Index 0 is GameMaker's empty tile, not art — treating it as real is how a
    'most common tile' vote once elected a blank as the path fill."""
    if word == EMPTY or word < 0:
        return -1
    idx = word & IDX_MASK
    return idx if idx > 0 else -1


# ------------------------------------------------------- autotile learning

def ortho_mask(grid, w, h, x, y, member) -> int:
    m = 0
    for bit, (dx, dy) in ((1, (0, -1)), (2, (0, 1)), (4, (-1, 0)), (8, (1, 0))):
        nx, ny = x + dx, y + dy
        if 0 <= nx < w and 0 <= ny < h:
            if member(grid[ny * w + nx]):
                m |= bit
        else:
            m |= bit  # off-map counts as same (engine convention)
    return m


def learn_fill_and_lips(layer_grid, w, h, ts):
    """Sunnyside's real transition model, recovered from Room1.

    A terrain layer holds OPAQUE fill tiles on its own cells and mostly-
    transparent 'lip' overlay tiles on the NEIGHBOURING cells (grass fringe
    hanging over the water).  So: split the layer's cells by tile opacity,
    take the dominant opaque tile as the fill, and for every lip cell vote
    its tile under the mask of which orthogonal neighbours are material
    cells (U=1 D=2 L=4 R=8).  Diagonal-only touches get four extra keys
    (16=TL 17=TR 18=BL 19=BR).  Returns (fill_idx, {mask: tile_idx}).
    """
    opaque_cache: dict[int, bool] = {}

    def is_opaque(t):
        if t not in opaque_cache:
            n = sum(1 for p in ts_tile(ts, t).getdata() if p[3] > 200)
            opaque_cache[t] = n >= 190
        return opaque_cache[t]

    solid = [t >= 0 and is_opaque(t) for t in layer_grid]
    fills = Counter(t for i, t in enumerate(layer_grid) if t >= 0 and solid[i])
    fill = fills.most_common(1)[0][0]
    votes: dict[int, Counter] = defaultdict(Counter)
    for y in range(h):
        for x in range(w):
            i = y * w + x
            t = layer_grid[i]
            if t < 0 or solid[i]:
                continue

            def at(dx, dy):
                nx, ny = x + dx, y + dy
                return 0 <= nx < w and 0 <= ny < h and solid[ny * w + nx]

            m = at(0, -1) * 1 + at(0, 1) * 2 + at(-1, 0) * 4 + at(1, 0) * 8
            if m == 0:
                for key, (dx, dy) in ((16, (-1, -1)), (17, (1, -1)),
                                      (18, (-1, 1)), (19, (1, 1))):
                    if at(dx, dy):
                        m = key
                        break
            if m > 0:
                votes[m][t] += 1
    # A hue guard does NOT work here: the true coast lips are pure sand and
    # waterline with no fill colour at all, while stray green deco matches
    # perfectly.  Vote count is the honest signal — Room1 places real lips by
    # the dozen, birds and sparkles a couple of times.  Rare masks (enclosed
    # one-cell water) simply fall back to the plain base tile.
    out: dict[int, int] = {}
    for m, c in votes.items():
        t, n = c.most_common(1)[0]
        if n >= (3 if m >= 16 else 4):
            out[m] = t
    return fill, out


def learn_material(layer_grid, w, h, ts=None) -> dict[int, int]:
    """mask -> most common tile index used with that orthogonal mask.

    A GM layer is not a single material — Room1's `paths` layer also carries
    planks, bridges and stepping stones.  So the winner per mask must share
    palette with the material's interior: we take the dominant colours of the
    mask-15 winner and reject candidates that use none of them.
    """
    votes: dict[int, Counter] = defaultdict(Counter)
    member = lambda t: t >= 0
    for y in range(h):
        for x in range(w):
            t = layer_grid[y * w + x]
            if t < 0:
                continue
            votes[ortho_mask(layer_grid, w, h, x, y, member)][t] += 1
    out: dict[int, int] = {}
    ref_colors: set = set()
    if ts is not None and 15 in votes:
        fill = votes[15].most_common(1)[0][0]
        px = [p for p in ts_tile(ts, fill).getdata() if p[3] > 0]
        ref_colors = {c[:3] for c, _ in Counter(px).most_common(6)}
    for m, c in votes.items():
        for tile, _ in c.most_common():
            if ts is None or not ref_colors:
                out[m] = tile
                break
            px = [p[:3] for p in ts_tile(ts, tile).getdata() if p[3] > 0]
            hit = sum(1 for p in px if p in ref_colors)
            if px and hit >= len(px) * 0.15:
                out[m] = tile
                break
    return out


# ---------------------------------------------------------------- atlas

# ortho mask for each cell of the engine's 3x3 block, row-major
# (U=1 D=2 L=4 R=8; bit set = neighbour is same material)
BLOCK_CELL_MASK = [10, 14, 6, 11, 15, 7, 9, 13, 5]

CROPS = ["beetroot", "cabbage", "carrot", "cauliflower", "kale", "parsnip",
         "potato", "pumpkin", "radish", "sunflower", "wheat"]


class Atlas:
    """A growing 16-column grid of 16px tiles; gid = index + 1."""

    def __init__(self, cols=16):
        self.cols = cols
        self.tiles: list[Image.Image] = []
        blank = Image.new("RGBA", (TILE, TILE))
        self.add(blank)  # gid 1 = transparent

    def add(self, tile: Image.Image) -> int:
        assert tile.size == (TILE, TILE)
        self.tiles.append(tile)
        return len(self.tiles)  # gid

    def image(self) -> Image.Image:
        rows = (len(self.tiles) + self.cols - 1) // self.cols
        im = Image.new("RGBA", (self.cols * TILE, rows * TILE))
        for i, t in enumerate(self.tiles):
            im.paste(t, ((i % self.cols) * TILE, (i // self.cols) * TILE))
        return im


def ts_tile(ts: Image.Image, idx: int) -> Image.Image:
    x, y = (idx % TS_COLS) * TILE, (idx // TS_COLS) * TILE
    return ts.crop((x, y, x + TILE, y + TILE))


def over(base: Image.Image, top: Image.Image) -> Image.Image:
    return Image.alpha_composite(base.copy(), top)


def paste_bottom(tile: Image.Image, sprite: Image.Image, lift: int = 1) -> Image.Image:
    out = tile.copy()
    x = (TILE - sprite.width) // 2
    y = TILE - sprite.height - lift
    layer = Image.new("RGBA", (TILE, TILE))
    layer.paste(sprite, (x, max(0, y)))
    return Image.alpha_composite(out, layer)


def components(grid, w, h):
    """4-connected components of non-blank cells -> list of (cells, bbox)."""
    seen = [False] * (w * h)
    comps = []
    for i in range(w * h):
        if grid[i] < 0 or seen[i]:
            continue
        stack, cells = [i], []
        seen[i] = True
        while stack:
            j = stack.pop()
            cells.append(j)
            x, y = j % w, j // w
            for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
                if 0 <= nx < w and 0 <= ny < h:
                    k = ny * w + nx
                    if grid[k] >= 0 and not seen[k]:
                        seen[k] = True
                        stack.append(k)
        xs = [c % w for c in cells]
        ys = [c // w for c in cells]
        comps.append((cells, (min(xs), min(ys), max(xs), max(ys))))
    return comps


def emit_tish(path: Path, header: str, tables: dict):
    """Write typed parallel-array tish data tables."""
    lines = [f"// GENERATED by scripts/gen_sunnyside_pack.py — do not edit", f"// {header}"]
    for name, val in tables.items():
        if isinstance(val, int):
            lines.append(f"export let {name}: i32 = {val}")
        elif val and isinstance(val[0], str):
            body = ", ".join(f"'{v}'" for v in val)
            lines.append(f"export const {name}: string[] = [{body}]")
        else:
            body = ", ".join(str(v) for v in val)
            lines.append(f"export const {name}: i32[] = [{body}]")
    path.write_text("\n".join(lines) + "\n")
    print(f"  -> {path.relative_to(ROOT)}")


def main():
    BAKED.mkdir(parents=True, exist_ok=True)
    CATALOG.mkdir(parents=True, exist_ok=True)
    EX_SRC.mkdir(parents=True, exist_ok=True)

    print("characters:")
    anim: dict = {}
    bake_char_sheet("player", PLAYER_ACTIONS, P_WIN, (64, 32),
                    hair="spikeyhair", tools=True, source="human", anim_rows=anim)
    bake_char_sheet("npc_long", NPC_ACTIONS, N_WIN, (32, 32),
                    hair="longhair", tools=False, source="human", anim_rows=anim)
    bake_char_sheet("npc_bowl", NPC_ACTIONS, N_WIN, (32, 32),
                    hair="bowlhair", tools=False, source="human", anim_rows=anim)
    bake_char_sheet("npc_goblin", NPC_ACTIONS, N_WIN, (32, 32),
                    hair="", tools=False, source="goblin", anim_rows=anim)

    # shop icon strip (sheet: 16x16): seeds, the three crops, wood, fish,
    # mushroom, and a pointer cursor for the list UI
    icon_srcs = ["Elements/Crops/seeds_generic.png", "Elements/Crops/carrot_05.png",
                 "Elements/Crops/potato_05.png", "Elements/Crops/pumpkin_05.png",
                 "Elements/Crops/wood.png", "Elements/Crops/fish.png"]
    icons = []
    for f in icon_srcs:
        icons.append(Image.open(RAW / f).convert("RGBA"))
    icons.append(Image.open(RAW / "Elements/Plants/spr_deco_mushroom_red_01_strip4.png")
                 .convert("RGBA").crop((0, 0, 16, 16)))
    cur = Image.open(RAW / "UI/cursor_01.png").convert("RGBA")
    cur = cur.resize((max(1, cur.width * 16 // cur.height), 16), Image.NEAREST)
    icons.append(cur)
    strip = Image.new("RGBA", (16 * len(icons), 16))
    for i, ic in enumerate(icons):
        strip.paste(ic, (i * 16 + (16 - ic.width) // 2, (16 - ic.height) // 2))
    strip = quantize15(strip)
    strip.save(BAKED / "icons16.png")
    print(f"  assets/sunnyside/baked/icons16.png: {len(icons)} icons")

    room = load_yy(GM / "Room1.yy")
    layers = {l["name"]: l for l in room["layers"] if l.get("resourceType") == "GMRTileLayer"}
    w = layers["land"]["tiles"]["SerialiseWidth"]
    h = layers["land"]["tiles"]["SerialiseHeight"]
    grids = {n: [word_tile(wd) for wd in decode_layer(layers[n])] for n in
             ("sea", "land", "paths", "building", "walls", "decoration_01",
              "decoration_02", "decoration_03", "shadows")}

    print("autotile (learned from Room1):")
    autotile = {}
    ts_learn = Image.open(RAW / "Tileset/spr_tileset_sunnysideworld_16px.png").convert("RGBA")
    for material, layer_name in [("grass_over_sea", "land"), ("path_over_grass", "paths")]:
        # the colour guard is for MIXED layers (paths also carries planks and
        # stepping stones); the land layer is homogeneous and the guard only
        # starves its coastline masks (edge tiles are half sea by design)
        table = learn_material(grids[layer_name], w, h,
                               ts_learn if material == "path_over_grass" else None)
        autotile[material] = {str(m): t for m, t in sorted(table.items())}
        print(f"  {material}: {len(table)} masks learned")
    (CATALOG / "autotile.json").write_text(json.dumps(
        {"tileset": "raw/Tileset/spr_tileset_sunnysideworld_16px.png",
         "columns": TS_COLS, "mask_bits": "U=1 D=2 L=4 R=8 (set=same)",
         "materials": autotile}, indent=1))
    print(f"  -> {CATALOG.relative_to(ROOT)}/autotile.json")

    ts = Image.open(RAW / "Tileset/spr_tileset_sunnysideworld_16px.png").convert("RGBA")

    # ---- assemble the world atlas -------------------------------------
    atlas = Atlas()
    sea_fill_idx = Counter(t for t in grids["sea"] if t >= 0).most_common(1)[0][0]
    sea_tile = ts_tile(ts, sea_fill_idx)

    grass_fill_idx, grass_lips = learn_fill_and_lips(grids["land"], w, h, ts)
    path_fill_idx, path_lips = learn_fill_and_lips(grids["paths"], w, h, ts)
    print(f"  lip tables: grass {len(grass_lips)} keys, path {len(path_lips)} keys")

    gid_sea = atlas.add(sea_tile)
    gid_grass = atlas.add(over(sea_tile, ts_tile(ts, grass_fill_idx)))
    grass_opaque = atlas.tiles[gid_grass - 1]
    gid_path = atlas.add(over(grass_opaque, ts_tile(ts, path_fill_idx)))

    # 20-entry lip tables: index = ortho mask of material neighbours (U=1 D=2
    # L=4 R=8), plus diagonal-only keys 16..19 (TL,TR,BL,BR).  Entry 0 = the
    # plain base.  Composited opaque so the map stays one layer.
    def lip_table(lips: dict, base: Image.Image, base_gid: int) -> list[int]:
        out = []
        for m in range(20):
            if m in lips:
                out.append(atlas.add(over(base, ts_tile(ts, lips[m]))))
            else:
                out.append(base_gid)
        return out

    grass_masks = lip_table(grass_lips, sea_tile, gid_sea)      # painted on SEA cells
    path_masks = lip_table(path_lips, grass_opaque, gid_grass)  # painted on GRASS cells

    # ---- the beach ring -----------------------------------------------
    # The sheet carries a cell-aligned rounded beach blob (sand core with the
    # blue waterline INSIDE the tile) around tile (6,30).  Border land cells
    # paint one of these, indexed by which orthogonal neighbours are water
    # (U=1 D=2 L=4 R=8) — this is what gives every coast, vertical edges
    # included, a real shoreline.  Hand-pinned: the blob is eight tiles and a
    # fill, and a classifier has nothing to learn from a single instance.
    BEACH = {
        0: (6, 30),               # no water: plain sand
        1: (6, 28), 2: (6, 31), 4: (5, 30), 8: (7, 30),      # one side
        5: (5, 29), 9: (7, 29), 6: (5, 31), 10: (7, 31),     # two adjacent
        3: (6, 28), 12: (5, 30),                             # opposite pairs
        7: (5, 29), 11: (7, 29), 13: (5, 31), 14: (7, 31),   # peninsulas
        15: (6, 30),              # islet
    }
    beach_masks = []
    for m in range(16):
        c, r = BEACH[m]
        beach_masks.append(atlas.add(over(sea_tile, ts_tile(ts, r * 64 + c))))

    # ---- farm + crop tiles --------------------------------------------
    crops_dir = RAW / "Elements/Crops"

    def soil(name: str) -> Image.Image:
        return paste_bottom(grass_opaque, Image.open(crops_dir / name).convert("RGBA"), lift=1)

    # bridges: the plank deck tile, laid where a path crosses water
    gid_bridge = atlas.add(over(sea_tile, ts_tile(ts, 10 * 64 + 5)))

    # forage: red mushroom (frame 0 of the deco strip) on plain grass
    mush = Image.open(RAW / "Elements/Plants/spr_deco_mushroom_red_01_strip4.png")        .convert("RGBA").crop((0, 0, 16, 16))
    gid_mushroom = atlas.add(over(grass_opaque, mush))

    gid_soil = atlas.add(soil("soil_00.png"))       # untilled mound
    gid_tilled = atlas.add(soil("soil_01.png"))     # tilled, dry
    gid_planted = atlas.add(soil("soil_03.png"))    # tilled, seeded hole
    gid_watered = atlas.add(soil("soil_04.png"))    # tilled, watered
    tilled_dry = atlas.tiles[gid_tilled - 1]
    tilled_wet = atlas.tiles[gid_watered - 1]

    crop_gids: list[int] = []  # [crop*12 + stage*2 + wet] -> gid
    for crop in CROPS:
        for stage in range(6):
            spr = Image.open(crops_dir / f"{crop}_{stage:02d}.png").convert("RGBA")
            for base in (tilled_dry, tilled_wet):
                crop_gids.append(atlas.add(paste_bottom(base, spr, lift=2)))

    # ---- building stamps: FIXED rects, all layers z-merged -------------
    # Stamps are whole buildings copied verbatim from hand-picked Room1
    # rects — not connected components.  Components looked assembled from
    # spare parts: half a building lives in the decoration layers, and a
    # component from building+walls alone ships with its lower floor
    # missing.  Per cell we take the TOPMOST non-blank tile in the room's
    # own z-order; blank cells stay 0 (keep the terrain underneath).
    # ---- building stamps: cropped WHOLE from the rendered example scene ---
    # Third design, and the one that finally holds: no tile indices at all.
    # The pack ships the artist's own 2x render of a finished map; a stamp
    # is a tile-aligned pixel crop of one complete building, sliced into
    # CONTIGUOUS atlas slots (painted as gid = BASE + row*w + col — arrays
    # through tish function params scrambled a town once).  Baked-in
    # critters are erased with small cell copies from clean/mirrored spots.
    scene = Image.open(RAW / "Sunnyside_World_ExampleScene.png")
    scene = scene.resize((scene.width // 2, scene.height // 2), Image.NEAREST).convert("RGBA")
    # name: (x0, y0, w_tiles, h_tiles, tile_patches, px_patches)
    # tile_patches: (dst_col, dst_row, w, h, src_col, src_row) cell copies;
    # px_patches: (dst_x, dst_y, w, h, src_x, src_y) pixel copies — both
    # erase critters the artist parked on the buildings (goblins, a duck).
    SCENE_STAMPS = {
        "shop":   (1002, 630, 11, 11,
                   [(4, 2, 2, 3, 7, 2), (0, 9, 3, 2, 3, 9)],
                   [(92, 62, 16, 24, 76, 62)]),
        "house":  (832, 518, 7, 6, [(6, 3, 1, 3, 0, 3)], []),
        "barn":   (1294, 666, 7, 7, [(0, 3, 1, 3, 5, 0), (0, 6, 1, 1, 5, 0)], []),
        "house2": (352, 678, 8, 11,
                   [(5, 8, 2, 3, 1, 8), (0, 7, 1, 2, 0, 5), (6, 7, 1, 1, 1, 7),
                    (7, 6, 1, 2, 7, 4), (0, 8, 1, 1, 0, 6), (7, 8, 1, 1, 7, 4)], []),
    }
    # The crops carry grass margins and porch rows so the buildings look
    # planted, but those cells must stay WALKABLE — a full-rect solid mark
    # put an invisible wall around every building.  Insets: (L, T, R, B)
    # cells of the rect that are ground, not building.
    STAMP_SOLID_INSET = {
        "shop": (1, 1, 1, 2), "house": (0, 0, 0, 0),
        "barn": (1, 1, 1, 1), "house2": (1, 0, 1, 1),
    }
    print("stamps (scene crops, contiguous atlas slots):")
    stamps = {}
    stamp_meta = {}
    for name, (x0, y0, sw, sh, patches, px_patches) in SCENE_STAMPS.items():
        crop = scene.crop((x0, y0, x0 + sw * TILE, y0 + sh * TILE)).copy()
        for (dc, dr, pw, ph, sc, sr) in patches:
            patch = crop.crop((sc * TILE, sr * TILE, (sc + pw) * TILE, (sr + ph) * TILE))
            crop.paste(patch, (dc * TILE, dr * TILE))
        for (dx, dy, pw, ph, sx, sy) in px_patches:
            patch = crop.crop((sx, sy, sx + pw, sy + ph))
            crop.paste(patch, (dx, dy))
        base = None
        for yy in range(sh):
            for xx in range(sw):
                gid = atlas.add(crop.crop((xx * TILE, yy * TILE,
                                           (xx + 1) * TILE, (yy + 1) * TILE)))
                if base is None:
                    base = gid
        stamps[name] = (sw, sh, base)
        stamp_meta[name] = {"w": sw, "h": sh, "scene_at": (x0, y0), "base_gid": base}
        print(f"  {name}: {sw}x{sh} base gid {base} at scene ({x0},{y0})")

    # ---- tree stamps: contiguous slots too -----------------------------
    deco = [-1] * (w * h)
    for n in ("decoration_01", "decoration_03"):
        for i, t in enumerate(grids[n]):
            if t >= 0:
                deco[i] = t
    tree_comps = [c for c in components(deco, w, h)
                  if 2 <= (c[1][2] - c[1][0] + 1) <= 3 and 2 <= (c[1][3] - c[1][1] + 1) <= 3
                  and len(c[0]) >= 4]
    tree_pick = Counter()
    for cells, (x0, y0, x1, y1) in tree_comps:
        sig = tuple(sorted((deco[c], c % w - x0, c // w - y0) for c in cells))
        tree_pick[sig] += 1
    trees = []
    for sig, n in tree_pick.most_common(2):
        sw = max(dx for _, dx, _ in sig) + 1
        sh = max(dy for _, _, dy in sig) + 1
        cellmap = {}
        for t, dx, dy in sig:
            cellmap[(dx, dy)] = t
        base = None
        for dy in range(sh):
            for dx in range(sw):
                tile = grass_opaque.copy()
                if (dx, dy) in cellmap:
                    tile = over(tile, ts_tile(ts, cellmap[(dx, dy)]))
                gid = atlas.add(tile)
                if base is None:
                    base = gid
        trees.append((sw, sh, base))
        print(f"  tree{len(trees)-1}: {sw}x{sh} base gid {base} (seen {n}x in room)")

    img = quantize_n(atlas.image(), 20)
    img.save(BAKED / "world_tiles.png")
    print(f"  atlas: {len(atlas.tiles)} tiles -> {(BAKED / 'world_tiles.png').relative_to(ROOT)}"
          f" ({img.width}x{img.height})")
    (BAKED / "world_tiles.json").write_text(json.dumps(
        {"cols": atlas.cols, "tiles": len(atlas.tiles), "stamps": stamp_meta}, indent=1))

    # ---- emit typed tish data -----------------------------------------
    anim_tables = {}
    for who, table in anim.items():
        anim_tables[f"ANIM_{who.upper()}_NAME"] = [k for k, _, _ in table]
        anim_tables[f"ANIM_{who.upper()}_START"] = [s for _, s, _ in table]
        anim_tables[f"ANIM_{who.upper()}_LEN"] = [n for _, _, n in table]
    emit_tish(EX_SRC / "data_anim.tish", "character sheet frame tables", anim_tables)

    world_tables = {
        "ATLAS_COLS": atlas.cols,
        "GID_SEA": gid_sea, "GID_GRASS": gid_grass, "GID_PATH": gid_path,
        "GID_MUSHROOM": gid_mushroom,
        "GID_BRIDGE": gid_bridge,
        "GID_SOIL": gid_soil, "GID_TILLED": gid_tilled,
        "GID_PLANTED": gid_planted, "GID_WATERED": gid_watered,
        "GRASS_MASK_GID": grass_masks,
        "PATH_MASK_GID": path_masks,
        "BEACH_MASK_GID": beach_masks,
        "CROP_TILE_GID": crop_gids,
        "CROP_NAME": CROPS,
    }
    for i, (sw, sh, base) in enumerate(trees):
        world_tables[f"TREE{i}_W"] = sw
        world_tables[f"TREE{i}_H"] = sh
        world_tables[f"TREE{i}_BASE"] = base
    for name, (sw, sh, base) in stamps.items():
        up = name.upper()
        world_tables[f"{up}_W"] = sw
        world_tables[f"{up}_H"] = sh
        world_tables[f"{up}_BASE"] = base
        il, it, ir, ib = STAMP_SOLID_INSET[name]
        world_tables[f"{up}_INS_L"] = il
        world_tables[f"{up}_INS_T"] = it
        world_tables[f"{up}_INS_R"] = ir
        world_tables[f"{up}_INS_B"] = ib
    emit_tish(EX_SRC / "data_world.tish", "world atlas gid tables", world_tables)


if __name__ == "__main__":
    main()
