#!/usr/bin/env python3
"""recipe_to_tiled.py — one-time migration: bake a scene recipe (the compact
scenepack format) into an editable Tiled map (.tmj) + external tilesets (.tsj), so the scene can
be hand-edited visually in Tiled. After this, the .tmj is the source of truth and the scenepack
reads it directly (see crates/tish-gba-scenepack/src/tiled.rs).

The autotiled ground is baked to explicit tiles (Tiled has no live autotile without terrains set
up). Layers produced: Ground (TilesetFloor), Objects (props), Collision (a red marker layer —
non-empty cell = solid; hidden), and a Spawns object layer. Tilesets + map are written to
assets/ninja-adventure/tiled/ (the Tiled project root) with sensible relative image paths.

    python3 scripts/recipe_to_tiled.py examples/ninja-village/assets/village_square_scene.json village_square
"""
import json, os, sys
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ninja_autotile import Autotiler

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NA = os.path.join(ROOT, "assets/ninja-adventure")
# .tsj tilesets live in the SHARED library (referenced by every map, no PNG copies); the .tmj is
# written next to the example that uses it.
TILED_DIR = os.path.join(NA, "tiled")
TS_SRC = os.path.join(NA, "Backgrounds/Tilesets")

# Fixed tileset order + firstgid assignment (must be deterministic so GIDs are stable).
TILESETS = ["TilesetFloor", "TilesetHouse", "TilesetNature", "TilesetElement", "TilesetFloorDetail"]


def tileset_geom(name):
    w, h = Image.open(os.path.join(TS_SRC, name + ".png")).size
    return w, h, w // 16, h // 16


def write_tsj(name):
    w, h, cols, rows = tileset_geom(name)
    tsj = {
        "columns": cols, "image": f"../Backgrounds/Tilesets/{name}.png",
        "imagewidth": w, "imageheight": h, "margin": 0, "spacing": 0,
        "name": name, "tilecount": cols * rows, "tiledversion": "1.11.0",
        "tilewidth": 16, "tileheight": 16, "type": "tileset", "version": "1.10",
    }
    json.dump(tsj, open(os.path.join(TILED_DIR, name + ".tsj"), "w"), indent=1)
    return cols * rows


def write_collision_tileset():
    # a 16x16 translucent-red marker so the collision layer is visible in Tiled
    img = Image.new("RGBA", (16, 16), (220, 40, 40, 110))
    img.save(os.path.join(TILED_DIR, "Collision.png"))
    tsj = {
        "columns": 1, "image": "Collision.png", "imagewidth": 16, "imageheight": 16,
        "margin": 0, "spacing": 0, "name": "Collision", "tilecount": 1,
        "tiledversion": "1.11.0", "tilewidth": 16, "tileheight": 16,
        "type": "tileset", "version": "1.10",
    }
    json.dump(tsj, open(os.path.join(TILED_DIR, "Collision.tsj"), "w"), indent=1)


def is_transparent(atlas, col, row):
    cell = atlas.crop((col * 16, row * 16, col * 16 + 16, row * 16 + 16))
    return all(p[3] == 0 for p in cell.getdata())


def main(recipe_path, out_name, out_dir):
    os.makedirs(TILED_DIR, exist_ok=True)
    os.makedirs(out_dir, exist_ok=True)
    recipe = json.load(open(recipe_path))
    W, H = recipe["width"], recipe["height"]

    # firstgids
    firstgid = {}
    g = 1
    for name in TILESETS:
        firstgid[name] = g
        g += write_tsj(name)
    firstgid["Collision"] = g
    write_collision_tileset()

    cols_of = {n: tileset_geom(n)[2] for n in TILESETS}

    def norm(name):
        return name[:-4] if name.endswith(".png") else name

    def gid(tileset, tile_id):
        return firstgid[norm(tileset)] + tile_id

    ground = [0] * (W * H)
    objects = [0] * (W * H)
    collision = [0] * (W * H)
    atlases = {n: Image.open(os.path.join(TS_SRC, n + ".png")).convert("RGBA") for n in TILESETS}

    # ---- ground: autotile then map into TilesetFloor GIDs ----
    at = Autotiler(os.path.join(NA, "catalog/autotile.json"))
    gs = recipe["ground"]
    fx, fy, fw, fh = gs["fill_rect"]
    terrain = [0] * (W * H)
    for r in range(fy, fy + fh):
        for c in range(fx, fx + fw):
            if 0 <= c < W and 0 <= r < H:
                terrain[r * W + c] = 1
    fcols = at.cols(gs["tileset"])
    for i, cg in enumerate(at.terrain_to_gids(terrain, W, H, gs["tileset"], gs["material"], fill=1, oob_same=False)):
        if cg:
            ground[i] = gid("TilesetFloor", cg - 1)  # cg is tile_id+1

    def place_object(tileset, tile_id, c, r, solid):
        if 0 <= c < W and 0 <= r < H:
            objects[r * W + c] = gid(tileset, tile_id)
            if solid:
                collision[r * W + c] = gid("Collision", 0)

    # ---- border fence ----
    if "border_fence" in recipe:
        f = recipe["border_fence"]
        ts = norm(f["tileset"]); tcols = cols_of[ts]
        oc, orow = f["origin"]
        def fence_tid(piece):
            return (orow + piece[1]) * tcols + (oc + piece[0])
        for c in range(W):
            piece = f["post"] if c in (0, W - 1) else f["run"]
            place_object(ts, fence_tid(piece), c, 0, True)
            place_object(ts, fence_tid(piece), c, H - 1, True)
        for r in range(H):
            piece = f["post"] if r in (0, H - 1) else f["rail"]
            place_object(ts, fence_tid(piece), 0, r, True)
            place_object(ts, fence_tid(piece), W - 1, r, True)

    # ---- stamps ----
    for s in recipe.get("stamps", []):
        ts = norm(s["tileset"]); tcols = cols_of[ts]
        oc, orow = s["origin"]; sw, sh = s["size"]; ac, ar = s["at"]
        solid = s.get("solid", True)
        door = tuple(s["door"]) if "door" in s else None
        for dr in range(sh):
            for dc in range(sw):
                if is_transparent(atlases[ts], oc + dc, orow + dr):
                    continue
                tid = (orow + dr) * tcols + (oc + dc)
                cell_solid = solid and (dc, dr) != door
                place_object(ts, tid, ac + dc, ar + dr, cell_solid)

    # ---- spawns (object layer) — name encodes kind ----
    KIND_NAME = {0: "player", 1: "npc", 2: "heart"}
    objects_group = []
    for i, sp in enumerate(recipe.get("spawns", [])):
        objects_group.append({
            "id": i + 1, "name": KIND_NAME.get(sp["kind"], f"kind{sp['kind']}"),
            "type": "spawn", "x": sp["col"] * 16, "y": sp["row"] * 16,
            "width": 16, "height": 16, "visible": True, "rotation": 0, "point": False,
            "properties": [{"name": "kind", "type": "int", "value": sp["kind"]}],
        })

    def tilelayer(name, data, lid, visible=True):
        return {"id": lid, "name": name, "type": "tilelayer", "data": data,
                "width": W, "height": H, "x": 0, "y": 0, "opacity": 1, "visible": visible}

    # the .tmj references the SHARED .tsj library by a relative path (no per-example copies)
    def tsj_ref(fn):
        return os.path.relpath(os.path.join(TILED_DIR, fn), out_dir)
    tilesets_ref = [{"firstgid": firstgid[n], "source": tsj_ref(f"{n}.tsj")} for n in TILESETS]
    tilesets_ref.append({"firstgid": firstgid["Collision"], "source": tsj_ref("Collision.tsj")})

    tmj = {
        "type": "map", "version": "1.10", "tiledversion": "1.11.0",
        "orientation": "orthogonal", "renderorder": "right-down",
        "width": W, "height": H, "tilewidth": 16, "tileheight": 16,
        "infinite": False, "nextlayerid": 6, "nextobjectid": len(objects_group) + 1,
        "tilesets": tilesets_ref,
        "layers": [
            tilelayer("Ground", ground, 1),
            tilelayer("Objects", objects, 2),
            tilelayer("Collision", collision, 3, visible=False),
            {"id": 4, "name": "Spawns", "type": "objectgroup", "draworder": "topdown",
             "objects": objects_group, "opacity": 1, "visible": True, "x": 0, "y": 0},
        ],
    }
    out = os.path.join(out_dir, out_name + ".tmj")
    json.dump(tmj, open(out, "w"), indent=1)
    print(f"wrote {out} (.tsj library in {os.path.relpath(TILED_DIR, ROOT)})")
    print(f"  tilesets: {', '.join(f'{n}(gid {firstgid[n]})' for n in TILESETS)}, Collision(gid {firstgid['Collision']})")
    print(f"  layers: Ground, Objects, Collision(hidden), Spawns({len(objects_group)})")


if __name__ == "__main__":
    # usage: recipe_to_tiled.py <recipe.json> <out_name> <out_dir>
    recipe = sys.argv[1] if len(sys.argv) > 1 else "examples/ninja-village/tiled/village_square.recipe.json"
    name = sys.argv[2] if len(sys.argv) > 2 else "village_square"
    out_dir = sys.argv[3] if len(sys.argv) > 3 else "examples/ninja-village/tiled"
    main(
        os.path.join(ROOT, recipe) if not os.path.isabs(recipe) else recipe,
        name,
        os.path.join(ROOT, out_dir) if not os.path.isabs(out_dir) else out_dir,
    )
