#!/usr/bin/env python3
"""Author the battle as a Tiled .tmj (this stands in for hand-editing in Tiled). Orthogonal logical
grid with three layers: `terrain` (tile type via GID into terrain.tsj), `height` (elevation via GID
into heights.tsj; empty = height 0), and a `units` object layer (class/team per spawn). Edit the
arrays below (or the .tmj in Tiled) and re-run, then `import_battle.py` to regenerate engine data."""
import json, os

D = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # example dir
OUT = os.path.join(D, "tiled/battle.tmj")

# terrain codes → GID = code+1 (terrain.tsj firstgid 1): 0 grass, 1 water, 2 stone, 3 tall-grass
G, W, S, T = 0, 1, 2, 3
# Plateau cells stay grass (G) — elevation comes from the `height` layer + a render offset, not from
# pre-tall art (which would double up). W = water (unwalkable), S = stone path.
terrain = [
    [G, G, G, S, S, G, G, G],
    [G, G, G, S, G, G, G, G],
    [G, S, S, S, G, G, G, G],
    [G, S, W, W, G, G, G, G],
    [G, S, W, W, S, S, S, G],
    [G, G, W, W, G, G, S, G],
    [G, G, G, G, G, G, S, G],
    [G, G, G, G, G, G, G, G],
]
# elevation per cell (0..3). The T (tall) block is a raised plateau.
height = [
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 1, 1, 0],
    [0, 0, 0, 0, 1, 2, 1, 0],
    [0, 0, 0, 0, 1, 1, 1, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
]
H = len(terrain); Wd = len(terrain[0])
HEIGHT_FIRSTGID = 5  # after terrain's 4 tiles (firstgids 1..4)

# units: class 0 fighter (team 0 = player), 1 enemy. Object x/y are pixel = (col,row)*32.
units = [
    (1, 6, 0, 0),   # player fighter (class 0, team 0)
    (5, 2, 1, 1),   # enemy mage (class 1, team 1) on the plateau
]

terrain_data = [terrain[r][c] + 1 for r in range(H) for c in range(Wd)]
height_data = [(HEIGHT_FIRSTGID + height[r][c] - 1) if height[r][c] > 0 else 0
               for r in range(H) for c in range(Wd)]

tmj = {
    "type": "map", "orientation": "orthogonal", "renderorder": "right-down",
    "compressionlevel": -1, "infinite": False,
    "width": Wd, "height": H, "tilewidth": 32, "tileheight": 32,
    "nextlayerid": 4, "nextobjectid": len(units) + 1,
    "tiledversion": "1.11.0", "version": "1.10",
    "tilesets": [
        {"firstgid": 1, "source": "terrain.tsj"},
        {"firstgid": HEIGHT_FIRSTGID, "source": "heights.tsj"},
    ],
    "layers": [
        {"type": "tilelayer", "name": "terrain", "id": 1, "width": Wd, "height": H,
         "opacity": 1, "visible": True, "x": 0, "y": 0, "data": terrain_data},
        {"type": "tilelayer", "name": "height", "id": 2, "width": Wd, "height": H,
         "opacity": 0.5, "visible": True, "x": 0, "y": 0, "data": height_data},
        {"type": "objectgroup", "name": "units", "id": 3, "opacity": 1, "visible": True,
         "x": 0, "y": 0, "draworder": "topdown", "objects": [
             {"id": i + 1, "name": "unit", "type": "", "x": c * 32, "y": r * 32,
              "width": 32, "height": 32, "rotation": 0, "visible": True,
              "properties": [{"name": "cls", "type": "int", "value": cls},
                             {"name": "team", "type": "int", "value": team}]}
             for i, (c, r, cls, team) in enumerate(units)
         ]},
    ],
}
json.dump(tmj, open(OUT, "w"), indent=1)
print(f"wrote {OUT} ({Wd}x{H}, {len(units)} units)")
