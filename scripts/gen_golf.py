#!/usr/bin/env python3
"""Generate examples/golf's nine holes as Tiled .tmj maps.

⚠️ THE COURSE IS A .tmj, NOT A TISH ARRAY. Every map in this repo is a Tiled map consumed through
the `scene:` scheme, and that is not bookkeeping: `examples/bench-boot` measured a per-tile tish
marking loop at ~0.175 frames PER TILE, which was one example's entire four-second boot. A 20x12
hole is 240 cells; done in tish that is 42 frames of black screen per hole, done through `scene:` it
is one native call.

Collision comes from a **`Solid`** layer, not a `Collision` one. `Collision` forces cells WALKABLE
and an empty cell there ERASES whatever the tileset said — the opposite of what a wall wants.
`Solid` is the force-solid counterpart and is applied last, so it wins.

The SURFACE classes (green / rough / sand / the two slopes) are a separate object layer rather than
tile properties, because the vendored tilesets carry no per-tile properties at all and adding them
would mean editing the shared pack for one example's sake.

    python3 scripts/gen_golf.py
"""

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "examples/golf/assets"

W, H = 20, 12                     # 320x192 px — wider and taller than the 240x160 screen, so the
                                  # camera has somewhere to go and a hole is not all visible at once.
TILE = 16

# Tileset: the vendored Field set. firstgid 1, 5 columns, 75 tiles.
TSJ = "../../../assets/ninja-adventure/tiled/TilesetField.tsj"
FIRSTGID = 1

# Tile ids inside TilesetField (see the .tsj's wangsets): the fully-surrounded centre tile of each
# terrain is the flat fill for that terrain.
G_GREEN = FIRSTGID + 18           # spring_grass centre
G_ROUGH = FIRSTGID + 33           # summer_grass centre
G_SAND = FIRSTGID + 48            # autumn_ground centre
G_WALL = FIRSTGID + 63            # field_snow centre, used as the out-of-bounds wall

# Surface class ids — the contract with src/main.tish's surface_def calls.
S_GREEN, S_ROUGH, S_SAND, S_SLOPE_E, S_SLOPE_S = 0, 1, 2, 3, 4

# One entry per hole: (par, tee col/row, cup col/row, [rough rects], [sand rects], [wall rects],
# [slope rects with a class]). Hand-authored rather than generated: nine holes is few enough that
# each one can be a deliberate shape, and a random course is a worse course.
HOLES = [
    # 1 — straight, one bunker. Teaches the power meter.
    dict(par=2, tee=(3, 6), cup=(16, 6), rough=[], sand=[(9, 5, 2, 2)], walls=[], slopes=[]),
    # 2 — dogleg round a wall.
    dict(par=3, tee=(3, 9), cup=(16, 3), rough=[(8, 6, 5, 3)],
         sand=[], walls=[(9, 0, 2, 6)], slopes=[]),
    # 3 — a rough corridor: the ball dies fast off the fairway.
    dict(par=3, tee=(2, 6), cup=(17, 6), rough=[(6, 0, 8, 5), (6, 8, 8, 4)],
         sand=[], walls=[], slopes=[]),
    # 4 — downhill: an east slope carries the ball, so power must come OFF.
    dict(par=2, tee=(3, 6), cup=(17, 6), rough=[], sand=[],
         walls=[], slopes=[(6, 4, 8, 5, S_SLOPE_E)]),
    # 5 — a bunker guarding the cup.
    dict(par=3, tee=(3, 3), cup=(16, 9), rough=[],
         sand=[(13, 7, 5, 4)], walls=[], slopes=[]),
    # 6 — two walls, a threaded gap.
    dict(par=3, tee=(2, 6), cup=(17, 6), rough=[],
         sand=[], walls=[(8, 0, 2, 5), (8, 7, 2, 5)], slopes=[]),
    # 7 — south slope across the approach.
    dict(par=3, tee=(3, 2), cup=(16, 4), rough=[(10, 0, 8, 12)],
         sand=[], walls=[], slopes=[(10, 0, 8, 12, S_SLOPE_S)]),
    # 8 — sand belt.
    dict(par=4, tee=(2, 6), cup=(17, 6), rough=[],
         sand=[(8, 0, 3, 12)], walls=[], slopes=[]),
    # 9 — everything: wall, rough, bunker, slope.
    dict(par=4, tee=(2, 10), cup=(17, 2), rough=[(6, 4, 6, 4)],
         sand=[(12, 1, 3, 3)], walls=[(9, 7, 2, 5)], slopes=[(6, 4, 6, 4, S_SLOPE_E)]),
]


def blank(fill):
    return [fill] * (W * H)


def paint(grid, rect, gid):
    x, y, w, h = rect
    for r in range(y, min(y + h, H)):
        for c in range(x, min(x + w, W)):
            grid[r * W + c] = gid


def border(grid, gid):
    for c in range(W):
        grid[c] = gid
        grid[(H - 1) * W + c] = gid
    for r in range(H):
        grid[r * W] = gid
        grid[r * W + W - 1] = gid


def tilelayer(name, data, lid):
    return {
        "type": "tilelayer", "name": name, "id": lid,
        "width": W, "height": H, "x": 0, "y": 0,
        "opacity": 1, "visible": True, "data": data,
    }


def build(h, idx):
    ground = blank(G_GREEN)
    for r in h["rough"]:
        paint(ground, r, G_ROUGH)
    for s in h["sand"]:
        paint(ground, s, G_SAND)
    for w in h["walls"]:
        paint(ground, w, G_WALL)
    border(ground, G_WALL)

    # The Solid layer: 0 means "not forced", so only the walls carry a gid here.
    solid = blank(0)
    for w in h["walls"]:
        paint(solid, w, G_WALL)
    border(solid, G_WALL)

    # Surfaces as objects. One rect per class per region; the ROM walks them once at hole load and
    # calls grid_set_surface, which is ~240 native calls in the worst case and happens behind a
    # fade rather than on a played frame.
    objs = []
    oid = 1

    def obj(name, x, y, kind):
        nonlocal oid
        objs.append({
            "id": oid, "name": name, "x": x * TILE, "y": y * TILE,
            "width": TILE, "height": TILE, "visible": True,
            "properties": [{"name": "kind", "type": "int", "value": kind}],
        })
        oid += 1

    # ⚠️ ONE OBJECT PER CELL, not per rect. The engine's spawn accessors expose col/row/kind and
    # NOT the object's width and height, so a rect would arrive as its top-left corner and the ROM
    # would have to re-invent the extent — which is how a bunker ends up one tile off from the sand
    # you can see. Cells are cheap here (a hole is at most a few hundred, read once behind the load)
    # and they cannot disagree with the art.
    def rect(r, kind):
        x, y, w, hh = r
        for rr in range(y, min(y + hh, H)):
            for cc in range(x, min(x + w, W)):
                obj("surface", cc, rr, kind)

    for r in h["rough"]:
        rect(r, S_ROUGH)
    for s in h["sand"]:
        rect(s, S_SAND)
    for s in h["slopes"]:
        rect((s[0], s[1], s[2], s[3]), s[4])
    obj("tee", h["tee"][0], h["tee"][1], 100)
    obj("cup", h["cup"][0], h["cup"][1], 101)
    obj("par", 0, 0, 200 + h["par"])

    return {
        "type": "map", "orientation": "orthogonal", "renderorder": "right-down",
        "infinite": False, "width": W, "height": H,
        "tilewidth": TILE, "tileheight": TILE,
        "nextlayerid": 4, "nextobjectid": oid,
        "version": "1.10", "tiledversion": "1.11.0",
        "tilesets": [{"firstgid": FIRSTGID, "source": TSJ}],
        "layers": [
            tilelayer("Ground", ground, 1),
            tilelayer("Solid", solid, 2),
            {"type": "objectgroup", "name": "spawns", "id": 3,
             "opacity": 1, "visible": True, "x": 0, "y": 0, "objects": objs},
        ],
    }


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for i, h in enumerate(HOLES):
        m = build(h, i)
        p = OUT / f"hole{i + 1}.tmj"
        p.write_text(json.dumps(m, indent=1))
        print(f"hole{i + 1}.tmj  par {h['par']}  tee {h['tee']} cup {h['cup']}")
    print(f"{len(HOLES)} holes, {W}x{H} tiles each")


if __name__ == "__main__":
    main()
