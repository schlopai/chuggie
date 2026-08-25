#!/usr/bin/env python3
"""Generate every asset for examples/warforge — the three campaign maps, the terrain tileset and
the unit sprite sheets.

Writes into examples/warforge/assets/:
  wf_tiles.png / wf_tiles.tsj   local terrain tileset (terrain + shroud + building stamps)
  m1.tmj / m2.tmj / m3.tmj      the three mission maps
  u_<kind>.png                  one 5x4 sheet per unit kind (idle + 4 walk frames per facing)

...and examples/warforge/src/mapdata.tish, so the mission constants the game reads (start cells,
mine and forest positions, the outpost) cannot drift from the maps they describe. `gen_wsg.py`
writes `src/skill_kit.tish` the same way and for the same reason.

All art is DRAWN, not vendored: `scripts/wf_art.py` places every pixel, in the idiom of v3x3d's
Mini Medieval (studied, not copied — that pack is paid). Units are genuinely 8x8; terrain is 16x16
with 8px-scale detail, because the `scene:` streaming pipeline packs its atlas in 16px cells.

Two constraints shape the output, both learned the hard way:

* **The shroud cells live in the TERRAIN tileset.** `tilemap_new` uploads its asset's palettes to
  all sixteen background banks, so a fog layer built from its own image repaints the whole map in
  its colours. One tileset for both layers makes them share a palette by construction. See
  examples/rts-fog for the three arrangements that were measured.
* **The palette is fixed and small.** `wf_art.py` draws from one ~16-colour set, so no quantization
  pass is needed at all — sixteen palette banks is a hard GBA ceiling and overflowing it panics
  inside agb on an innocent frame. Drawing within the budget beats clamping down to it.

    python3 scripts/gen_warforge.py
"""

from __future__ import annotations

import json
import pathlib
import sys

from PIL import Image

import wf_art as A

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
TS = NA / "Backgrounds" / "Tilesets"
OUT = ROOT / "examples/warforge/assets"
SRC = ROOT / "examples/warforge/src"
LIB = "../../../assets/ninja-adventure/tiled"

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

TILE = 16
CEL = 8
TS_COLS = 8  # tileset grid width, in 16px cells

# ── Tile indices in wf_tiles.png. These ARE the gids-minus-one the maps and the ROM use, so this
# block is the contract between this script, the .tmj files and src/mapdata.tish.
T_GRASS, T_GRASS1, T_GRASS2 = 0, 1, 2
T_DIRT, T_ROAD, T_FOREST, T_STUMP, T_GOLD = 3, 4, 5, 6, 7
T_ROCK, T_SHROUD, T_HALF, T_WATER = 8, 9, 10, 11
# Buildings are 2x2 CELLS (32x32px) — where the reference's houses sit. A 3x3 building at 16px
# cells would be 48px, a fifth of the screen.
T_HALL, T_FARM, T_BARR, T_CAMP = 16, 20, 24, 28
T_KEEP, T_TOWER, T_SMITH = 32, 36, 40
T_SCAFFOLD = 44
TS_CELLS = 48

GID_SHROUD = T_SHROUD + 1
GID_HALF = T_HALF + 1
SOLID_MARK = 900  # Collision.tsj firstgid; any non-zero cell on `Solid` forces solid

# Faction colours. One pixel of these on a unit's shoulders is the whole team read at 8px.
TEAM_ME, TEAM_FOE = A.BLUE, A.RED


def build_tileset() -> None:
    rows = (TS_CELLS + TS_COLS - 1) // TS_COLS
    sheet = Image.new("RGBA", (TILE * TS_COLS, TILE * rows), (0, 0, 0, 0))

    def put(idx: int, img: Image.Image) -> None:
        sheet.alpha_composite(img, ((idx % TS_COLS) * TILE, (idx // TS_COLS) * TILE))

    put(T_GRASS, A.t_grass(0).img)
    put(T_GRASS1, A.t_grass(1).img)
    put(T_GRASS2, A.t_grass(2).img)
    put(T_DIRT, A.t_dirt().img)
    put(T_ROAD, A.t_road().img)
    put(T_FOREST, A.t_forest().img)
    put(T_STUMP, A.t_stump().img)
    put(T_GOLD, A.t_gold().img)
    put(T_ROCK, A.t_rock().img)
    put(T_SHROUD, A.t_shroud().img)
    put(T_HALF, A.t_half().img)
    put(T_WATER, A.t_water().img)
    put(T_SCAFFOLD, A.t_scaffold().img)

    # Buildings are stamped as 2x2 blocks of 16px cells, row-major — the order the game blits them.
    for base, img in ((T_HALL, A.b_hall(TEAM_ME)), (T_FARM, A.b_farm()),
                      (T_BARR, A.b_barracks(TEAM_ME)), (T_CAMP, A.b_camp(TEAM_FOE)),
                      (T_KEEP, A.b_keep(TEAM_ME)), (T_TOWER, A.b_tower(TEAM_ME)),
                      (T_SMITH, A.b_smith(TEAM_ME))):
        for r in range(2):
            for c in range(2):
                put(base + r * 2 + c,
                    img.crop((c * TILE, r * TILE, (c + 1) * TILE, (r + 1) * TILE)))

    OUT.mkdir(parents=True, exist_ok=True)
    sheet.save(OUT / "wf_tiles.png")
    (OUT / "wf_tiles.tsj").write_text(json.dumps({
        "columns": TS_COLS,
        "image": "wf_tiles.png",
        "imagewidth": sheet.width,
        "imageheight": sheet.height,
        "margin": 0,
        "name": "wf_tiles",
        "spacing": 0,
        "tilecount": TS_COLS * rows,
        "tiledversion": "1.11.0",
        "tileheight": TILE,
        "tilewidth": TILE,
        "type": "tileset",
        "version": "1.10",
    }, indent=1))
    n = len(set(sheet.convert("RGB").getdata()))
    print(f"wf_tiles.png  {sheet.width}x{sheet.height} ({TS_CELLS} cells, {n} colours)")


# ── Unit art ─────────────────────────────────────────────────────────────────
# One 8x8 sheet per kind: 5 columns (idle + 4 walk) x 4 rows (facing down/up/left/right), which is
# the `base = facing * stride` contract `set_seek` expects with stride 5.
#
# Separate sheets rather than one strip, because a pooled unit re-points its sprite at the kind's
# sheet when it is armed (`sprite_set_sheet`) — that is how one pool holds mixed kinds without
# holding every kind's VRAM at once.
UNITS = [
    ("peasant", TEAM_ME), ("footman", TEAM_ME), ("archer", TEAM_ME), ("hero", TEAM_ME),
    ("grunt", TEAM_FOE), ("raider", TEAM_FOE), ("chief", TEAM_FOE),
]


def build_units() -> None:
    for name, team in UNITS:
        A.build_unit_sheet(name, team).save(OUT / f"u_{name}.png")
    # 16px, and NOT a unit sheet: the cursor is the one thing on screen that has to read as "you are
    # commanding an army". Built from a hero sprite at first, the game read as "walk a guy around".
    A.cursor_sheet().save(OUT / "cursor.png")
    A.bar_sheet().save(OUT / "bars.png")
    A.icon_sheet().save(OUT / "icons.png")
    print(f"u_*.png  {len(UNITS)} kinds, 5x4 of {CEL}x{CEL} (stride 5) + cursor.png")


# ── Maps ─────────────────────────────────────────────────────────────────────
# Each mission is (w, h) plus a painter. Sizes stay modest: a flow field is 2 bytes a cell and the
# camera only ever shows 15x10 of them.
class Mission:
    def __init__(self, key: str, w: int, h: int) -> None:
        self.key = key
        self.w, self.h = w, h
        # Three grass variants scattered by a cheap hash, so a big field does not visibly tile.
        self.ground = [(T_GRASS + ((c * 7 + r * 13) % 3)) + 1
                       for r in range(h) for c in range(w)]
        self.solid = [0] * (w * h)
        self.meta: dict[str, tuple[int, int]] = {}

    def put(self, c: int, r: int, t: int, solid: bool = False) -> None:
        if 0 <= c < self.w and 0 <= r < self.h:
            self.ground[r * self.w + c] = t + 1
            self.solid[r * self.w + c] = SOLID_MARK if solid else 0

    def rect(self, c0: int, r0: int, w: int, h: int, t: int, solid: bool = False) -> None:
        for r in range(r0, r0 + h):
            for c in range(c0, c0 + w):
                self.put(c, r, t, solid)

    def clear_ground(self, c: int, r: int) -> int:
        """The grass variant that belongs at (c,r) — used when a razed building reverts to field."""
        return (T_GRASS + ((c * 7 + r * 13) % 3)) + 1

    def border(self) -> None:
        for c in range(self.w):
            self.put(c, 0, T_ROCK, True)
            self.put(c, self.h - 1, T_ROCK, True)
        for r in range(self.h):
            self.put(0, r, T_ROCK, True)
            self.put(self.w - 1, r, T_ROCK, True)

    def road(self, c0: int, r0: int, c1: int, r1: int) -> None:
        """An L-shaped road. Cosmetic, but it tells the player where the map wants them to walk."""
        for c in range(min(c0, c1), max(c0, c1) + 1):
            if self.ground[r0 * self.w + c] <= T_GRASS2 + 1:
                self.put(c, r0, T_ROAD)
        for r in range(min(r0, r1), max(r0, r1) + 1):
            if self.ground[r * self.w + c1] <= T_GRASS2 + 1:
                self.put(c1, r, T_ROAD)

    def treeline(self, c0: int, r0: int, w: int, h: int) -> None:
        self.rect(c0, r0, w, h, T_FOREST, True)


def mission1() -> Mission:
    """The Long March — no economy, no building. A corridor with two woods to funnel the fight."""
    m = Mission("m1", 34, 22)
    m.border()
    m.treeline(6, 1, 3, 8)
    m.treeline(6, 13, 3, 8)
    m.treeline(17, 1, 3, 6)
    m.treeline(17, 15, 3, 6)
    m.rect(24, 8, 3, 3, T_ROCK, True)
    m.road(4, 11, 30, 11)
    m.meta["start"] = (3, 11)
    m.meta["foe"] = (29, 11)
    m.meta["outpost"] = (30, 11)
    return m


def mission2() -> Mission:
    """Foothold — the full loop: harvest, build, train, and break the enemy camp."""
    m = Mission("m2", 40, 26)
    m.border()
    m.treeline(7, 3, 4, 5)       # the player's lumber
    m.treeline(30, 18, 4, 5)     # the enemy's
    m.treeline(18, 1, 2, 7)
    m.treeline(18, 18, 2, 7)
    m.rect(19, 10, 3, 5, T_ROCK, True)
    m.rect(5, 16, 2, 2, T_GOLD, True)   # gold mine, player side
    m.rect(33, 6, 2, 2, T_GOLD, True)   # gold mine, enemy side
    m.road(6, 12, 34, 12)
    m.meta["start"] = (4, 10)
    m.meta["foe"] = (34, 15)
    m.meta["mine"] = (5, 16)
    m.meta["wood"] = (7, 3)
    return m


def mission3() -> Mission:
    """The Chieftain — a defensible bowl with one approach, then a boss at the far end."""
    m = Mission("m3", 38, 24)
    m.border()
    m.rect(12, 1, 3, 8, T_ROCK, True)
    m.rect(12, 14, 3, 9, T_ROCK, True)   # a single gap at rows 9-13: the choke
    m.treeline(4, 3, 3, 4)
    m.rect(3, 17, 2, 2, T_GOLD, True)
    m.treeline(28, 2, 4, 4)
    m.road(6, 11, 32, 11)
    m.meta["start"] = (5, 11)
    m.meta["foe"] = (32, 11)
    m.meta["mine"] = (3, 17)
    m.meta["wood"] = (4, 3)
    m.meta["choke"] = (13, 11)
    return m


def layer(name: str, data: list[int], lid: int, w: int, h: int) -> dict:
    return {
        "id": lid, "name": name, "type": "tilelayer", "data": data,
        "width": w, "height": h, "x": 0, "y": 0, "opacity": 1,
        "visible": name != "Solid",
    }


def write_map_arrays(missions: list[Mission], out: pathlib.Path) -> None:
    """Emit each map as two flat tish arrays: gids and solid flags.

    warforge does NOT use `scene:`. The Tiled pipeline bakes an atlas from the tiles a map uses,
    while the shroud layer needs the whole tileset — two bakers over one PNG, two palette orderings,
    and the GBA has one set of background palettes, so one layer always draws in the other's colours
    (measured: a black map one way round, a brown shroud the other). Streaming the terrain from
    arrays through the same `background:` asset the shroud uses makes the conflict impossible.

    ~4 bytes a cell: the largest map here is 40x26, so about 4KB of typed `i32[]`.
    """
    lines = [
        "// GENERATED BY scripts/gen_warforge.py — DO NOT EDIT.",
        "//",
        "// Terrain for each mission: `_G` is the tile gid per cell, `_S` is 1 where the cell blocks",
        "// movement. Both are row-major, `w * h` long, and typed — an untyped array in tish is 28",
        "// bytes an element instead of 4.",
        "",
    ]
    for m in missions:
        k = m.key.upper()
        lines.append(f"export let {k}_G: i32[] = [" + ",".join(str(g) for g in m.ground) + "]")
        lines.append(f"export let {k}_S: i32[] = [" + ",".join("1" if v else "0" for v in m.solid) + "]")
        lines.append("")
    out.write_text("\n".join(lines))
    total = sum(len(m.ground) for m in missions)
    print(f"src/terrain.tish  {total} cells across {len(missions)} maps")


def write_map(m: Mission) -> None:
    doc = {
        "type": "map", "version": "1.10", "tiledversion": "1.11.0",
        "orientation": "orthogonal", "renderorder": "right-down",
        "width": m.w, "height": m.h, "tilewidth": TILE, "tileheight": TILE,
        "infinite": False, "nextlayerid": 4, "nextobjectid": 1,
        "tilesets": [
            {"firstgid": 1, "source": "wf_tiles.tsj"},
            {"firstgid": SOLID_MARK, "source": f"{LIB}/Collision.tsj"},
        ],
        # `Solid`, never `Collision`: a Collision layer can only force cells WALKABLE, and an empty
        # cell there ERASES the tileset's own collision, so it cannot author a wall.
        "layers": [
            layer("Ground", m.ground, 1, m.w, m.h),
            layer("Solid", m.solid, 2, m.w, m.h),
        ],
    }
    (OUT / f"{m.key}.tmj").write_text(json.dumps(doc, indent=1))
    print(f"{m.key}.tmj  {m.w}x{m.h}  {sorted(m.meta)}")


def write_mapdata(missions: list[Mission]) -> None:
    """Emit the mission constants as tish, so the maps and the game cannot drift apart."""
    lines = [
        "// GENERATED BY scripts/gen_warforge.py — DO NOT EDIT.",
        "//",
        "// Mission geometry, emitted alongside the .tmj files that contain it. Hand-copying these",
        "// numbers is how a map and the game that reads it drift; regenerate instead.",
        "//",
        "// Every value is `i32`: an untyped scalar in tish is a soft-float `Cell<f64>` on a chip",
        "// with no FPU, and these are read on the frame a mission loads.",
        "",
    ]
    for m in missions:
        k = m.key.upper()
        lines.append(f"export let {k}_W: i32 = {m.w}")
        lines.append(f"export let {k}_H: i32 = {m.h}")
        for name, (c, r) in sorted(m.meta.items()):
            lines.append(f"export let {k}_{name.upper()}_C: i32 = {c}")
            lines.append(f"export let {k}_{name.upper()}_R: i32 = {r}")
        lines.append("")
    lines += [
        "// Tileset cell indices, mirrored from the generator. `_GID` values are these plus one,",
        "// because Tiled gids are 1-based and 0 means empty.",
        f"export let TS_COLS: i32 = {TS_COLS}",
        f"export let GID_SHROUD: i32 = {GID_SHROUD}",
        f"export let GID_HALF: i32 = {GID_HALF}",
        f"export let GID_GRASS: i32 = {T_GRASS + 1}",
        f"export let GID_ROAD: i32 = {T_ROAD + 1}",
        f"export let GID_STUMP: i32 = {T_STUMP + 1}",
        f"export let GID_FOREST: i32 = {T_FOREST + 1}",
        f"export let GID_HALL: i32 = {T_HALL + 1}",
        f"export let GID_FARM: i32 = {T_FARM + 1}",
        f"export let GID_BARR: i32 = {T_BARR + 1}",
        f"export let GID_CAMP: i32 = {T_CAMP + 1}",
        f"export let GID_KEEP: i32 = {T_KEEP + 1}",
        f"export let GID_TOWER: i32 = {T_TOWER + 1}",
        f"export let GID_SMITH: i32 = {T_SMITH + 1}",
        f"export let GID_SCAFFOLD: i32 = {T_SCAFFOLD + 1}",
        "",
    ]
    SRC.mkdir(parents=True, exist_ok=True)
    (SRC / "mapdata.tish").write_text("\n".join(lines))
    print("src/mapdata.tish  mission constants + tileset gids")


def main() -> None:
    build_tileset()
    build_units()
    # ORDER MATTERS: the base-and-economy map is mission ONE.
    #
    # Shipping the no-economy tutorial first meant the game opened on a hero and three soldiers with
    # nothing to build and nothing to harvest — it read as "walk a character around", which is the
    # exact opposite of the first impression an RTS needs. The march is still a good mission; it is
    # just not the one that introduces the genre.
    foothold, march, chieftain = mission2(), mission1(), mission3()
    foothold.key, march.key, chieftain.key = "m1", "m2", "m3"
    missions = [foothold, march, chieftain]
    write_map_arrays(missions, SRC / "terrain.tish")
    write_mapdata(missions)


if __name__ == "__main__":
    main()
