#!/usr/bin/env python3
"""Build the AKARI maps as Tiled `.tmj` files that the `scene:` importer bakes into ROM.

Two areas: `town.tmj` (Willow Vale — grass, houses, a red torii, a cave-mouth shrine entrance,
NPCs) and `shrine.tmj` (the Hollow Shrine — a four-room vertical dungeon of brick + dark walls,
enemies, a chest, and the boss). Tiles are addressed by (col,row) into the vendored Ninja Adventure
tilesets, using the pixel-verified coordinates in assets/ninja-adventure/catalog/tilesets.json.

Each map references the shared `.tsj` library (wangsets + per-tile collision). Solids come from
Tiled tile collision on those tilesets — no separate `Collision` layer. Entity spawns live in an
object layer whose `kind` int the game reads. Run from the repo root:

    python3 scripts/gen_akari_maps.py
"""
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ninja_autotile import Autotiler   # noqa: E402

OUT = os.path.join(ROOT, "examples", "akari", "assets")
os.makedirs(OUT, exist_ok=True)
AUTOTILE = Autotiler(os.path.join(ROOT, "assets/ninja-adventure/catalog/autotile.json"))

# tileset image paths relative to the .tmj (examples/akari/assets/)
TSROOT = "../../../assets/ninja-adventure/Backgrounds/Tilesets"

# ── spawn kinds (shared with the tish scene dispatcher in components.tish) ──
K_PLAYER = 0
K_ELDER, K_WOMAN, K_MERCHANT, K_SENSEI = 20, 21, 22, 23
K_SLIME, K_BAT, K_SKELETON, K_BOSS = 30, 31, 32, 40
K_CHEST, K_HEART = 12, 13


# The shared Tiled tileset library (wangsets + per-tile collision), referenced by `.tmj` source
# just like examples/ninja-*/*.tmj — the "properly configured" way, vs embedding a bare image.
TSJ_DIR = "assets/ninja-adventure/tiled"
TSJ_REL = "../../../assets/ninja-adventure/tiled"   # relative to a map in examples/<name>/assets/


def tsj_config(name):
    """Read the shared .tsj's authoritative columns + tilecount (so firstgids + gid math match Tiled
    and the importer exactly — no reliance on our own image-dimension guesses)."""
    t = json.load(open(os.path.join(ROOT, TSJ_DIR, name + ".tsj")))
    return t["columns"], t["tilecount"]


class Tileset:
    def __init__(self, name, cols, firstgid):
        self.name, self.cols, self.firstgid = name, cols, firstgid

    def gid(self, col, row):
        return self.firstgid + row * self.cols + col


class TMap:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.tilesets = []
        self.next_gid = 1
        self.layers = []            # (name, data[])
        self.spawns = []            # (col, row, kind)

    def tileset(self, name):
        cols, tilecount = tsj_config(name)
        ts = Tileset(name, cols, firstgid=self.next_gid)
        self.next_gid += tilecount
        self.tilesets.append(ts)
        return ts

    def layer(self, name):
        data = [0] * (self.w * self.h)
        self.layers.append((name, data))
        return data

    def put(self, layer, col, row, gid):
        if 0 <= col < self.w and 0 <= row < self.h:
            layer[row * self.w + col] = gid

    def fill(self, layer, x, y, w, h, gid):
        for r in range(y, y + h):
            for c in range(x, x + w):
                self.put(layer, c, r, gid)

    def stamp(self, layer, x, y, ts, sc, sr, w, h, door=None, door_layer=None):
        """Place a w×h block from the tileset at (x,y). Collision comes from the tileset's per-tile
        shapes (not a map layer). When `door_layer` is set, the door cell is drawn there instead
        (so a doorway can sit on Ground behind the player sprite)."""
        for dr in range(h):
            for dc in range(w):
                gid = ts.gid(sc + dc, sr + dr)
                if door == (dc, dr) and door_layer is not None:
                    self.put(door_layer, x + dc, y + dr, gid)
                    self.put(layer, x + dc, y + dr, 0)       # no overlay tile on top of the door
                else:
                    self.put(layer, x + dc, y + dr, gid)

    def spawn(self, col, row, kind, a=0, b=0):
        """A spawn object at (col, row) carrying `kind`, plus the importer's two optional int args
        `a`/`b` (baked as i16, read back with `mapSpawnA`/`mapSpawnB`) — a door's destination scene,
        an NPC id, a trigger payload. They are omitted from the .tmj when zero, so a map that uses
        neither is byte-identical to what this emitted before they existed."""
        self.spawns.append((col, row, kind, a, b))

    def to_json(self):
        layers = []
        lid = 1
        for name, data in self.layers:
            layers.append({"type": "tilelayer", "name": name, "id": lid, "width": self.w,
                           "height": self.h, "x": 0, "y": 0, "opacity": 1, "visible": True, "data": data})
            lid += 1
        objects = []
        for i, (c, r, k, a, b) in enumerate(self.spawns):
            props = [{"name": "kind", "type": "int", "value": k}]
            if a:
                props.append({"name": "a", "type": "int", "value": a})
            if b:
                props.append({"name": "b", "type": "int", "value": b})
            objects.append({"id": i + 1, "name": "", "x": c * 16, "y": r * 16, "width": 16, "height": 16,
                            "visible": True, "properties": props})
        layers.append({"type": "objectgroup", "name": "spawns", "id": lid, "opacity": 1,
                       "visible": True, "x": 0, "y": 0, "objects": objects})
        # External .tsj references (the shared library — wangsets + tile collision).
        tilesets = [{"firstgid": t.firstgid, "source": f"{TSJ_REL}/{t.name}.tsj"} for t in self.tilesets]
        return {"type": "map", "orientation": "orthogonal", "renderorder": "right-down",
                "infinite": False, "width": self.w, "height": self.h, "tilewidth": 16, "tileheight": 16,
                "nextlayerid": lid + 1, "nextobjectid": len(objects) + 1, "version": "1.10",
                "tiledversion": "1.11.0", "tilesets": tilesets, "layers": layers}

    def save(self, path):
        with open(path, "w") as f:
            json.dump(self.to_json(), f)
        print(f"  {os.path.basename(path):12} {self.w}x{self.h}  {len(self.spawns)} spawns")


def building(m, props, ground, paths, house, x, y, w, base=0, door=1):
    """Stamp a w-wide × 3-tall house from TilesetHouse's composable facade pieces: a gable roof
    (corner + repeatable fill + corner over 2 rows) on a wall row (left window, door, window fill,
    right window). `base` selects the palette (0 = orange House A, 12 = red House D).

    The door cell is left walkable AND drawn on Ground (not Props): Ground shares the sprite's
    back priority, so the player standing in the doorway is not covered by the door tile. Paths
    on that cell are cleared so a dirt overlay can't re-cover them. Roof + walls stay on Props —
    those tiles carry collision in the tileset; the door tile does not."""
    def T(sc, sr):
        return house.gid(base + sc, sr)
    for ry in (0, 1):                                   # roof rows
        m.put(props, x, y + ry, T(0, ry))
        for cx in range(1, w - 1):
            m.put(props, x + cx, y + ry, T(1, ry))
        m.put(props, x + w - 1, y + ry, T(3, ry))
    wy = y + 2                                          # wall row
    m.put(props, x, wy, T(0, 2))
    for cx in range(1, w - 1):
        if cx == door:
            m.put(ground, x + cx, wy, T(1, 2))          # door — walkable, behind the player
            m.put(paths, x + cx, wy, 0)                 # no path tile over the doorway
            m.put(props, x + cx, wy, 0)
        else:
            m.put(props, x + cx, wy, T(2, 2))
    m.put(props, x + w - 1, wy, T(3, 2))
    return (x + door, wy)                              # door tile, for warps


def autotile_paths(m, floor, grid):
    """Autotile a 0/1 terrain grid into TilesetFloor's `dirt_grass` blob (dirt with grass-blended
    edges) and return the gid layer. TilesetFloor must be the map's FIRST tileset (firstgid 1) so
    the autotiler's gids (firstgid-1 convention) match the map's gids directly."""
    assert floor.firstgid == 1
    return AUTOTILE.terrain_to_gids(grid, m.w, m.h, "TilesetFloor.png", "dirt_grass", fill=1, oob_same=False)


def paint_wangset(m, layer, ts, wangset_name, is_terrain, oob_terrain=True):
    """Paint `layer` by APPLYING a Tiled wangset straight from the shared `.tsj` — the wangset is the
    single source of autotiling truth (open the map in Tiled and the terrain brush uses the SAME rules).
    For every cell where `is_terrain`, compute its wangid the way Tiled does — the four edges from the
    orthogonal neighbours, each corner set only when both its adjacent edges are terrain too — then place
    the matching wangtile. Out-of-map counts as terrain by default (so a map-edge wall faces inward).
    Nearest-wangid fallback covers pieces a template lacks (e.g. a doorway's outer corner)."""
    tsj = json.load(open(os.path.join(ROOT, TSJ_DIR, ts.name + ".tsj")))
    ts_cols = tsj["columns"]
    ws = next(w for w in tsj["wangsets"] if w["name"] == wangset_name)
    lut = {}                                                  # wangid -> [tileid, ...] (variants)
    for w in ws["wangtiles"]:
        lut.setdefault(tuple(w["wangid"]), []).append(w["tileid"])
    keys = list(lut.keys())

    def terr(c, r):
        if c < 0 or r < 0 or c >= m.w or r >= m.h:
            return oob_terrain
        return bool(is_terrain(c, r))

    for r in range(m.h):
        for c in range(m.w):
            if not is_terrain(c, r):
                continue
            t, rt, b, l = terr(c, r - 1), terr(c + 1, r), terr(c, r + 1), terr(c - 1, r)

            def corner(a, bb, diag):
                return 1 if (a and bb and diag) else 0
            wid = (int(t), corner(t, rt, terr(c + 1, r - 1)), int(rt), corner(b, rt, terr(c + 1, r + 1)),
                   int(b), corner(b, l, terr(c - 1, r + 1)), int(l), corner(t, l, terr(c - 1, r - 1)))
            variants = lut.get(wid)
            if variants is None:                              # template gap → closest-matching piece
                variants = lut[min(keys, key=lambda k: sum(a != bb for a, bb in zip(k, wid)))]
            # Variants that form a clean WxH block in the tileset are one ornament sliced into
            # tiles — lay them by (c mod W, r mod H) so the artwork reassembles. (The old rule only
            # recognised 2x2; gold_ornate's 1x2 medallion came out shuffled. Same fix as
            # scripts/tiled_map.py — keep the two painters in step.)
            tid = None
            if len(variants) > 1:
                pos = [(v % ts_cols, v // ts_cols) for v in variants]
                cs = sorted({pc for pc, _ in pos})
                rs = sorted({pr for _, pr in pos})
                bw, bh = len(cs), len(rs)
                if bw * bh == len(variants) and cs == list(range(cs[0], cs[0] + bw)) \
                        and rs == list(range(rs[0], rs[0] + bh)):
                    grid = {(pc - cs[0], pr - rs[0]): v for v, (pc, pr) in zip(variants, pos)}
                    tid = grid.get((c % bw, r % bh))
            if tid is None:
                tid = variants[(c * 7 + r * 13) % len(variants)]
            m.put(layer, c, r, ts.firstgid + tid)


def build_town():
    m = TMap(40, 30)
    floor = m.tileset("TilesetFloor")   # FIRST → firstgid 1 (autotiler gids map 1:1)
    nature = m.tileset("TilesetNature")
    house = m.tileset("TilesetHouse")
    water = m.tileset("TilesetWater")

    GRASS = floor.gid(0, 12)

    ground = m.layer("Ground")
    paths = m.layer("Paths")
    props = m.layer("Props")

    m.fill(ground, 0, 0, m.w, m.h, GRASS)

    # Autotiled dirt paths: a plaza cross + a road north to the shrine gate (grass-blended edges).
    pg = [0] * (m.w * m.h)

    def carve(x, y, w, h):
        for r in range(y, y + h):
            for c in range(x, x + w):
                if 0 <= c < m.w and 0 <= r < m.h:
                    pg[r * m.w + c] = 1
    carve(4, 14, 32, 3)      # east-west street
    carve(17, 4, 3, 22)      # north-south road to the torii + cave
    carve(22, 20, 8, 4)      # a little plaza by the south houses
    for i, g in enumerate(autotile_paths(m, floor, pg)):
        if g > 0:
            paths[i] = g

    # Pond in the north-east — grass-edged water wangset (TilesetWater.tsj → grass_water).
    # Water tiles carry collision in the tileset.
    pond = {(c, r) for r in range(7, 10) for c in range(31, 35)}
    paint_wangset(m, ground, water, "grass_water", lambda c, r: (c, r) in pond, oob_terrain=False)

    # Houses around the plaza — composable facades, 5-6 tiles wide. Door cell is walkable and drawn
    # on Ground (behind the player); each door warps into that house's interior map.
    doors = []
    doors.append(building(m, props, ground, paths, house, 4, 18, 6, base=0, door=2))    # house0 orange
    doors.append(building(m, props, ground, paths, house, 29, 17, 6, base=12, door=3))  # house1 red
    doors.append(building(m, props, ground, paths, house, 9, 23, 5, base=0, door=2))    # house2 orange
    doors.append(building(m, props, ground, paths, house, 28, 24, 5, base=12, door=2))  # house3 red
    for i, (dc, dr) in enumerate(doors):
        print(f"    house{i} door @ ({dc},{dr})")

    # Trees first (trunk tiles are solid in the tileset), then the shrine approach so the cave
    # mouth isn't buried under canopy. Torii tiles are walkable; cave walls are solid house tiles.
    def tree(x, y, pink=False):
        m.stamp(props, x, y, nature, 0 if pink else 3, 18, 3, 3)
    for (x, y, p) in [(3, 8, True), (13, 8, False), (34, 21, False), (14, 24, True),
                      (35, 12, False), (2, 12, False), (35, 3, True), (2, 3, False)]:
        tree(x, y, p)
    for c in range(0, m.w, 3):
        tree(c, 0, (c // 3) % 2 == 0)
        tree(c, 27, (c // 3) % 2 == 1)

    m.stamp(props, 17, 5, house, 0, 5, 3, 3)              # torii — decorative, passable
    m.stamp(props, 17, 2, house, 0, 8, 3, 2, door=(1, 1), door_layer=ground)

    m.spawn(19, 16, K_PLAYER)
    m.spawn(19, 12, K_ELDER)
    m.spawn(9, 18, K_WOMAN)
    m.spawn(29, 20, K_MERCHANT)
    m.spawn(25, 26, K_SENSEI)
    m.save(os.path.join(OUT, "town.tmj"))


def build_shrine():
    m = TMap(15, 40)  # 1 room wide × 4 rooms tall (room camera 15×10)
    interior = m.tileset("TilesetInteriorFloor")
    wall_ts = m.tileset("TilesetWallSimple")
    dungeon = m.tileset("TilesetDungeon")

    ground = m.layer("Ground")
    walls_l = m.layer("Walls")     # walls on their own layer, autotiled from the Tiled wangset
    props = m.layer("Props")

    # (floor painted after the walls are known, below, so its shadow edges land against the walls)

    # Each 10-tall camera room is a FRAMED rectangle: its top+bottom wall rows and the two outer side
    # columns. Adjacent rooms' bottom+top wall rows meet as a natural 2-thick divider (each row faces
    # its own room, so the `cream_wall` frame tiles fit). Doorways punch BOTH divider rows.
    ROOM_H = 10
    wall_rows = set()
    for slot in range(m.h // ROOM_H):
        wall_rows.add(slot * ROOM_H)              # room top
        wall_rows.add(slot * ROOM_H + ROOM_H - 1)  # room bottom
    walls = set()
    for r in range(m.h):
        for c in range(m.w):
            if r in wall_rows or c == 0 or c == m.w - 1:
                walls.add((c, r))
    for slot in range(m.h // ROOM_H - 1):          # doorway through each 2-thick divider
        bot, top = slot * ROOM_H + ROOM_H - 1, (slot + 1) * ROOM_H
        for c in (6, 7, 8):
            walls.discard((c, bot)); walls.discard((c, top))
    # Floor: dark-cobble WANGSET on the NON-wall cells only, so its shadowed edge/corner tiles land
    # against the walls (a wall cell = void to the floor blob). Interior scatters fill variants.
    paint_wangset(m, ground, interior, "dark_cobble", lambda c, r: (c, r) not in walls, oob_terrain=False)
    # Walls on top: the shared Tiled wangset (TilesetWallSimple.tsj → cream_wall). Wall tiles are solid.
    paint_wangset(m, walls_l, wall_ts, "cream_wall", lambda c, r: (c, r) in walls, oob_terrain=True)

    # A little dressing: an altar flanked by orbs in the boss room; pillars as cover mid-dungeon.
    m.stamp(props, 6, 2, dungeon, 2, 2, 1, 1)     # altar behind the boss
    m.put(props, 4, 2, dungeon.gid(4, 2))          # blue orb pedestal (solid tile)
    m.put(props, 10, 2, dungeon.gid(4, 2))
    for (c, r) in [(3, 15), (11, 15), (3, 25), (11, 25)]:  # stone pillars = cover in combat rooms
        m.put(props, c, r, dungeon.gid(1, 2))

    # Spawns — bottom room (37) is the entrance; the boss waits at the top (row 4).
    m.spawn(7, 37, K_PLAYER)
    m.spawn(4, 34, K_HEART)                        # a heal near the entrance
    m.spawn(6, 33, K_SLIME)
    m.spawn(9, 32, K_SLIME)
    # room B (rows 20-29)
    m.spawn(5, 26, K_SLIME)
    m.spawn(9, 24, K_BAT)
    m.spawn(11, 22, K_CHEST)
    # room C (rows 10-19)
    m.spawn(6, 16, K_SKELETON)
    m.spawn(9, 14, K_SKELETON)
    # room D (rows 0-9) — boss
    m.spawn(7, 5, K_BOSS)
    m.save(os.path.join(OUT, "shrine.tmj"))


def build_house(name, wall_wang="cream_wall", floor_wang="tan_plank"):
    """One-screen (15×10) house interior: framed walls, plank/brick floor, a south doorway that
    warps back to town. The exit door tiles sit on Ground (behind the player), matching the town
    doorways."""
    m = TMap(15, 10)
    floor = m.tileset("TilesetInteriorFloor")
    wall_ts = m.tileset("TilesetWallSimple")

    ground = m.layer("Ground")
    walls_l = m.layer("Walls")

    walls = set()
    for r in range(m.h):
        for c in range(m.w):
            if r == 0 or r == m.h - 1 or c == 0 or c == m.w - 1:
                walls.add((c, r))
    # South doorway — three walkable cells in the bottom wall (matches shrine doorways).
    for c in (6, 7, 8):
        walls.discard((c, m.h - 1))

    paint_wangset(m, ground, floor, floor_wang, lambda c, r: (c, r) not in walls, oob_terrain=False)
    paint_wangset(m, walls_l, wall_ts, wall_wang, lambda c, r: (c, r) in walls, oob_terrain=True)

    m.spawn(7, 7, K_PLAYER)                            # just inside, facing the exit
    m.save(os.path.join(OUT, f"{name}.tmj"))


def main():
    build_town()
    build_shrine()
    # Four interiors — one per town house (palette pairs so they feel distinct).
    build_house("house0", wall_wang="cream_wall", floor_wang="tan_plank")
    build_house("house1", wall_wang="orange_wall", floor_wang="orange_brick")
    build_house("house2", wall_wang="brown_wall", floor_wang="cream_brick")
    build_house("house3", wall_wang="green_wall", floor_wang="gold_ornate")
    print(f"AKARI maps -> {OUT}")


if __name__ == "__main__":
    main()
