#!/usr/bin/env python3
"""Shared Tiled `.tmj` plumbing for the map generators.

Lifted verbatim out of `gen_akari_maps.py` when a second game needed it. It is generic map
machinery — tilesets, layers, stamping, spawn objects, and the wangset painter — with no game
content in it.

⚠️ `gen_akari_maps.py` PREDATES this module and still carries its own copy of these functions.
So there are two copies today and they CAN drift. Pointing akari's generator here (and diffing its
emitted `.tmj` byte-for-byte to prove nothing moved) is the follow-up; it was deliberately not done
in the same pass that introduced a second game. Until then, a fix to the wangid maths below has to
be made in both places.

Maps reference the SHARED `.tsj` library (`assets/ninja-adventure/tiled/*.tsj`) by relative path,
so wangsets and per-tile collision are authored once in the tileset rather than per map. Solids
come from that tile collision — see the layer contract in `crates/tish-gba-scenepack/src/tiled.rs`:
a `"Collision"` mask layer can only force cells WALKABLE (a blank cell in it ERASES a solid), and
only a `"Solid"` layer or tileset collision can add one.
"""
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ninja_autotile import Autotiler   # noqa: E402

AUTOTILE = Autotiler(os.path.join(ROOT, "assets/ninja-adventure/catalog/autotile.json"))

TSJ_DIR = "assets/ninja-adventure/tiled"
TSJ_REL = "../../../assets/ninja-adventure/tiled"   # relative to a map in examples/<name>/assets/


def tsj_config(name):
    """Read the shared .tsj's authoritative columns + tilecount, so firstgids and gid maths match
    Tiled and the importer exactly rather than relying on image-dimension guesses."""
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

    def merge_into(self, src_name, dst_name):
        """Fold one tile layer's cells into another and drop it — for layers that never overlap.

        Every rendered tile layer costs the runtime a full InfiniteScrolledMap's page bookkeeping
        (~8KB of heap, fixed, regardless of how few cells the layer holds — a downstream game's map paid
        it for a Props layer with SEVEN cells). Two same-priority layers whose painted cells never
        collide composite identically as one, and the src's transparency still reads over whatever
        sits below dst. Asserts on overlap rather than silently painting over content."""
        src = dst = None
        for i, (name, data) in enumerate(self.layers):
            if name == src_name:
                src = (i, data)
            if name == dst_name:
                dst = data
        if src is None or dst is None:
            return
        si, sdata = src
        for j, gid in enumerate(sdata):
            if gid:
                assert dst[j] == 0, f"merge_into: {src_name} overlaps {dst_name} at cell {j}"
                dst[j] = gid
        del self.layers[si]

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


def autotile_paths(m, floor, grid):
    """Autotile a 0/1 terrain grid into TilesetFloor's `dirt_grass` blob (dirt with grass-blended
    edges) and return the gid layer. TilesetFloor must be the map's FIRST tileset (firstgid 1) so
    the autotiler's gids (firstgid-1 convention) map onto the map's gids directly."""
    assert floor.firstgid == 1
    return AUTOTILE.terrain_to_gids(grid, m.w, m.h, "TilesetFloor.png", "dirt_grass",
                                    fill=1, oob_same=False)


_BLANK_CACHE = {}


def _blank_tiles(tsj):
    """Tile ids in this tileset whose art is entirely transparent. Cached per tileset image."""
    img = tsj["image"]
    if img in _BLANK_CACHE:
        return _BLANK_CACHE[img]
    from PIL import Image
    path = os.path.normpath(os.path.join(ROOT, TSJ_DIR, img))
    im = Image.open(path).convert("RGBA")
    tw, th, cols = tsj["tilewidth"], tsj["tileheight"], tsj["columns"]
    alpha = im.getchannel("A")
    out = set()
    for t in range(tsj["tilecount"]):
        x, y = (t % cols) * tw, (t // cols) * th
        if x + tw > im.width or y + th > im.height:
            out.add(t)                                        # off the end of the sheet entirely
            continue
        if alpha.crop((x, y, x + tw, y + th)).getextrema()[1] == 0:
            out.add(t)
    _BLANK_CACHE[img] = out
    return out


def paint_wangset(m, layer, ts, wangset_name, is_terrain, oob_terrain=True):
    """Paint `layer` by APPLYING a Tiled wangset straight from the shared `.tsj` — the wangset is the
    single source of autotiling truth (open the map in Tiled and the terrain brush uses the SAME rules).
    For every cell where `is_terrain`, compute its wangid the way Tiled does — the four edges from the
    orthogonal neighbours, each corner set only when both its adjacent edges are terrain too — then place
    the matching wangtile. Out-of-map counts as terrain by default (so a map-edge wall faces inward).
    Nearest-wangid fallback covers pieces a template lacks (e.g. a doorway's outer corner).

    ⚠️ BLANK VARIANTS ARE DROPPED, because a vendored wangset can name tiles that are EMPTY ART and
    two of them do. `TilesetFloor`'s `ice_blue` lists three tiles for its solid fill and two of them
    are fully transparent, so two thirds of an ice floor came out as holes: the first Icebox room
    rendered as a black-and-white checkerboard that looks exactly like a palette bug and is not one.
    `dark_cobble` has a proper 2x2 running bond and never showed it. Refusing to place transparent
    art is the general fix — nothing ever wants a blank tile from a terrain brush, so this cannot
    take away a tile a caller meant."""
    tsj = json.load(open(os.path.join(ROOT, TSJ_DIR, ts.name + ".tsj")))
    ts_cols = tsj["columns"]
    ws = next(w for w in tsj["wangsets"] if w["name"] == wangset_name)
    blank = _blank_tiles(tsj)
    lut = {}                                                  # wangid -> [tileid, ...] (variants)
    dropped = 0
    for w in ws["wangtiles"]:
        if w["tileid"] in blank:
            dropped += 1
            continue
        lut.setdefault(tuple(w["wangid"]), []).append(w["tileid"])
    if dropped:
        print(f"  wangset {ts.name}/{wangset_name}: dropped {dropped} transparent tile(s)")
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
            # ⚠️ VARIANTS CAN BE A REPEATING BLOCK, AND ITS SHAPE IS IN THE TILESET. A fill's
            # variants are often one ornament sliced into tiles — dark_cobble is a 2x2 running
            # bond, gold_ornate/green_ornate are a 1-wide, 2-TALL medallion. The old rule only
            # recognised "exactly 4 = 2x2" and scattered everything else pseudo-randomly, which
            # sprinkled the ornament's top and bottom halves at random across the diner floor.
            # The tiles' own positions in the tileset say what the unit is: when the variants
            # exactly fill a WxH rectangle there, lay them by (c mod W, r mod H) so the artwork
            # reassembles; anything that isn't a clean block really is a variant pool and keeps
            # the deterministic scatter.
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


