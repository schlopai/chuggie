#!/usr/bin/env python3
"""gen_tileset_library.py — generate a Tiled `.tsj` for EVERY vendored tileset PNG, into the
shared library at assets/ninja-adventure/tiled/. Each `.tsj` references the vendored PNG under
Backgrounds/Tilesets/ (no image copies), so any Tiled map in any example can paint with any of the
pack's tilesets. Run after adding/changing tilesets. Also writes the Collision marker (legacy
overlay) and stamps per-tile collision from catalog/tile_collision.json.

    python3 scripts/gen_tileset_library.py
"""
import json, os
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NA = os.path.join(ROOT, "assets/ninja-adventure")
TS_SRC = os.path.join(NA, "Backgrounds/Tilesets")
OUT = os.path.join(NA, "tiled")
AUTOTILE_JSON = os.path.join(NA, "catalog/autotile.json")
COLLISION_JSON = os.path.join(NA, "catalog/tile_collision.json")

# autotile.json stores each blob tile's neighbour mask in Godot "match corners and sides" order:
#   [top_left, top, top_right, left, right, bottom_left, bottom, bottom_right]   (1 = same terrain)
# Tiled's wangid is 8 entries in a different order (clockwise from top, edges + corners interleaved):
#   [Top, TopRight, Right, BottomRight, Bottom, BottomLeft, Left, TopLeft]
# so painting a terrain in Tiled autotiles with the SAME verified pieces the build uses.
def _mask_to_wangid(mask):
    tl, t, tr, l, r, bl, b, br = mask
    return [t, tr, r, br, b, bl, l, tl]   # values are already 0/1 → wang colour 0 (none) or 1 (terrain)


_WANG_COLORS = ["#38d24a", "#2f8f3a", "#e0c060", "#e090b0", "#f0f0f0", "#8a5a30",
                "#7fc4e6", "#e08040", "#c04848", "#48c0c0", "#c0c048", "#9060c0"]


def wangsets_for(tileset_name):
    """Build Tiled wangsets for `<tileset_name>.png` from catalog/autotile.json — one `mixed`
    (corner+edge) wangset per material, its wangtiles converted straight from the verified masks. So the
    autotiling that used to live only in ninja_autotile.py now exists as a real TILED TERRAIN: open any
    map, pick the terrain, paint, and Tiled fills the correct corner/edge tiles."""
    try:
        cat = json.load(open(AUTOTILE_JSON))
    except FileNotFoundError:
        return []
    entry = cat.get("tilesets", {}).get(tileset_name + ".png")
    if not entry:
        return []
    sets = []
    for i, (mat, md) in enumerate(entry.get("materials", {}).items()):
        tiles = md.get("tiles") or []
        if not tiles:
            continue
        wangtiles = [{"tileid": t["gid"] - 1, "wangid": _mask_to_wangid(t["mask"])} for t in tiles]
        sets.append({
            "colors": [{"color": _WANG_COLORS[i % len(_WANG_COLORS)], "name": mat,
                        "probability": 1, "tile": -1}],
            "name": mat, "tile": -1, "type": "mixed", "wangtiles": wangtiles,
        })
    return sets


def load_collision_ids():
    """Local tile ids that should carry a full-cell Tiled collision rect."""
    try:
        cat = json.load(open(COLLISION_JSON))
    except FileNotFoundError:
        return {}
    return {k: set(v) for k, v in (cat.get("tilesets") or {}).items()}


def collision_tile_defs(solid_ids):
    """Tiled Collision Editor shape: one 16×16 rect per solid local id (boolean grid at bake time)."""
    tiles = []
    for i, tid in enumerate(sorted(solid_ids)):
        tiles.append({
            "id": tid,
            "objectgroup": {
                "draworder": "index",
                "id": i + 1,
                "name": "",
                "objects": [{
                    "height": 16, "width": 16, "x": 0, "y": 0,
                    "id": 1, "name": "", "rotation": 0, "type": "", "visible": True,
                }],
                "opacity": 1, "type": "objectgroup", "visible": True, "x": 0, "y": 0,
            },
        })
    return tiles


def write_tsj(png_rel, collision_by_name):
    """png_rel is relative to TS_SRC, e.g. 'TilesetFloor.png' or 'Interior/TilesetInteriorFloor.png'."""
    name = os.path.splitext(os.path.basename(png_rel))[0]
    w, h = Image.open(os.path.join(TS_SRC, png_rel)).size
    cols, rows = w // 16, h // 16
    tsj = {
        "columns": cols,
        # image path is relative to the .tsj (which lives in OUT = assets/ninja-adventure/tiled/)
        "image": f"../Backgrounds/Tilesets/{png_rel}",
        "imagewidth": w, "imageheight": h, "margin": 0, "spacing": 0,
        "name": name, "tilecount": cols * rows, "tiledversion": "1.11.0",
        "tilewidth": 16, "tileheight": 16, "type": "tileset", "version": "1.10",
    }
    ws = wangsets_for(name)
    if ws:
        tsj["wangsets"] = ws
    solid = collision_by_name.get(name) or set()
    if solid:
        tsj["tiles"] = collision_tile_defs(solid)
    json.dump(tsj, open(os.path.join(OUT, name + ".tsj"), "w"), indent=1)
    return name, cols, rows, len(ws), len(solid)


def write_collision():
    Image.new("RGBA", (16, 16), (220, 40, 40, 110)).save(os.path.join(OUT, "Collision.png"))
    json.dump({
        "columns": 1, "image": "Collision.png", "imagewidth": 16, "imageheight": 16,
        "margin": 0, "spacing": 0, "name": "Collision", "tilecount": 1,
        "tiledversion": "1.11.0", "tilewidth": 16, "tileheight": 16, "type": "tileset", "version": "1.10",
    }, open(os.path.join(OUT, "Collision.tsj"), "w"), indent=1)


def main():
    os.makedirs(OUT, exist_ok=True)
    collision_by_name = load_collision_ids()
    pngs = []
    for root, _, files in os.walk(TS_SRC):
        for f in sorted(files):
            if f.endswith(".png"):
                pngs.append(os.path.relpath(os.path.join(root, f), TS_SRC))
    pngs.sort()
    names = set()
    made = 0
    for p in pngs:
        name, cols, rows, nws, nsolid = write_tsj(p, collision_by_name)
        if name in names:
            print(f"  !! duplicate tileset name '{name}' — {p} collides")
        names.add(name)
        made += 1
        tags = []
        if nws:
            tags.append(f"+{nws} wangset(s)")
        if nsolid:
            tags.append(f"+{nsolid} solid")
        wtag = ("  [" + ", ".join(tags) + "]") if tags else ""
        print(f"  {name}.tsj  ({cols}x{rows} tiles)  -> {p}{wtag}")
    write_collision()
    print(f"\nwrote {made} tileset .tsj + Collision.tsj into {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
