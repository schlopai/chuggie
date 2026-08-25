#!/usr/bin/env python3
"""Assets for examples/tower-def — a fixed-track tower defence.

Writes into `examples/tower-def/assets/`:

  td_tiles.png / td_tiles.tsj   a LOCAL tileset: grass (buildable), path (dirt), rock
  track.tmj                     a 15x10 map — EXACTLY one screen, so there is no camera
  units.png                     sheet: 16x16 cells — 2 creeps, 2 towers, 1 cursor

WHY THE MAP IS 15x10. The GBA screen is 240x160 and a cell is 16px, so a 15x10 map is the whole
board with nothing off-screen. A tower defence is a game you read at a glance — the player is
choosing where to build against a route they can see all of — and it means no camera, no scrolling,
and no `fog_blit`-style wrap arithmetic. `examples/rts-fog` needs all three because its map is
30x20; this one deliberately does not.

WHY EVERYTHING OFF THE TRACK IS SOLID. Creeps walk by flow field (`flow_goal` / `set_seek`), and a
flow field routes around solid cells. If the grass were walkable the creeps would cut straight
across it and there would be no track at all. Towers are ENTITIES, not tiles, so building on a solid
cell is fine — which is exactly the classic fixed-track shape.

Walls go on a layer named `Solid`, NOT `Collision`: `Collision` can only force cells *walkable* and
an empty cell there ERASES whatever collision the tileset declared, so it cannot author a wall
(crates/tish-gba-scenepack/src/tiled.rs documents the pair).

    python3 scripts/gen_towerdef.py
"""
from __future__ import annotations

import json
import pathlib

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/tower-def/assets"
LIB = "../../../assets/ninja-adventure/tiled"

W, H = 15, 10
TILE = 16

T_GRASS, T_PATH, T_ROCK = 0, 1, 2
TS_COLS = 3
SOLID_MARK = 10001  # firstgid of the shared Collision tileset — the `Solid` layer's mark

# The track, as waypoints in CELLS. The creeps enter at the first and leave at the last; every cell
# on a segment between consecutive waypoints is path. Turns are what make towers at a corner worth
# more than towers on a straight, so there are five of them.
TRACK = [(0, 4), (3, 4), (3, 1), (7, 1), (7, 7), (10, 7), (10, 3), (14, 3)]

# Sprite cells, 16x16. The order IS the contract with src/main.tish's ART_* constants.
CREEPS = [("Slime", "SLIME"), ("BlueBat", "BAT")]
TOWERS = [("Actor/Character/Knight/SeparateAnim/Idle.png", 2),      # arrow tower — faces left
          ("Actor/Character/SorcererBlack/SeparateAnim/Idle.png", 2)]  # mage tower


def crop_tile(rel: str, col: int, row: int) -> Image.Image:
    src = Image.open(NA / "Backgrounds" / "Tilesets" / rel).convert("RGBA")
    x, y = col * TILE, row * TILE
    return src.crop((x, y, x + TILE, y + TILE))


def monster_sheet(folder: str) -> pathlib.Path:
    for name in (f"{folder}.png", f"{folder.lower()}.png", "SpriteSheet.png"):
        p = NA / "Actor" / "Monster" / folder / name
        if p.is_file():
            return p
    raise FileNotFoundError(folder)


def quantise(img: Image.Image, limit: int = 15) -> Image.Image:
    a = img.getchannel("A").point(lambda v: 255 if v > 127 else 0)
    flat = Image.new("RGB", img.size, (0, 0, 0))
    flat.paste(img.convert("RGB"), (0, 0), a)
    out = flat.quantize(colors=limit, dither=Image.NONE).convert("RGBA")
    out.putalpha(a)
    return out


def build_tileset() -> None:
    sheet = Image.new("RGBA", (TILE * TS_COLS, TILE), (0, 0, 0, 0))
    sheet.paste(crop_tile("TilesetFloor.png", 0, 12), (T_GRASS * TILE, 0))
    sheet.paste(crop_tile("TilesetFloor.png", 1, 8), (T_PATH * TILE, 0))
    sheet.paste(crop_tile("TilesetNature.png", 1, 20), (T_ROCK * TILE, 0))
    OUT.mkdir(parents=True, exist_ok=True)
    sheet.save(OUT / "td_tiles.png")
    (OUT / "td_tiles.tsj").write_text(json.dumps({
        "columns": TS_COLS, "image": "td_tiles.png",
        "imagewidth": TILE * TS_COLS, "imageheight": TILE,
        "margin": 0, "name": "td_tiles", "spacing": 0, "tilecount": TS_COLS,
        "tiledversion": "1.11.0", "tileheight": TILE, "tilewidth": TILE,
        "type": "tileset", "version": "1.10",
    }, indent=1))
    print(f"  td_tiles.png  {TS_COLS} cells (grass path rock)")


def track_cells() -> set[tuple[int, int]]:
    cells: set[tuple[int, int]] = set()
    for (c0, r0), (c1, r1) in zip(TRACK, TRACK[1:]):
        assert c0 == c1 or r0 == r1, "track segments must be axis-aligned"
        if c0 == c1:
            for r in range(min(r0, r1), max(r0, r1) + 1):
                cells.add((c0, r))
        else:
            for c in range(min(c0, c1), max(c0, c1) + 1):
                cells.add((c, r0))
    return cells


def build_map() -> None:
    path = track_cells()
    ground, solid = [], []
    for r in range(H):
        for c in range(W):
            on = (c, r) in path
            ground.append((T_PATH if on else T_GRASS) + 1)
            # Every non-path cell is solid, so the flow field has exactly one route — see the header.
            solid.append(0 if on else SOLID_MARK)

    def layer(name, data, lid):
        return {"id": lid, "name": name, "type": "tilelayer", "data": data,
                "width": W, "height": H, "x": 0, "y": 0, "opacity": 1,
                "visible": name != "Solid"}

    (OUT / "track.tmj").write_text(json.dumps({
        "type": "map", "version": "1.10", "tiledversion": "1.11.0",
        "orientation": "orthogonal", "renderorder": "right-down",
        "width": W, "height": H, "tilewidth": TILE, "tileheight": TILE,
        "infinite": False, "nextlayerid": 3, "nextobjectid": 1,
        "tilesets": [
            {"firstgid": 1, "source": "td_tiles.tsj"},
            {"firstgid": SOLID_MARK, "source": f"{LIB}/Collision.tsj"},
        ],
        "layers": [layer("Ground", ground, 1), layer("Solid", solid, 2)],
    }, indent=1))
    print(f"  track.tmj     {W}x{H}, {len(path)} path cells, {len(TRACK) - 2} turns")


def build_units() -> None:
    """One sheet: creeps, towers, and the build cursor. One sheet is one palette bank."""
    cells: list[Image.Image] = []
    for folder, name in CREEPS:
        sheet = Image.open(monster_sheet(folder)).convert("RGBA")
        assert sheet.size == (4 * TILE, 4 * TILE), f"{folder}: {sheet.size}"
        cells.append(sheet.crop((0, 0, TILE, TILE)))
    for rel, col in TOWERS:
        src = Image.open(NA / rel).convert("RGBA")
        assert src.width == 4 * TILE, f"{rel} is {src.size}"
        cells.append(src.crop((col * TILE, 0, (col + 1) * TILE, TILE)))
    # The build cursor: a hollow bracket, drawn rather than cropped — nothing in the pack is one.
    cur = Image.new("RGBA", (TILE, TILE), (0, 0, 0, 0))
    px = cur.load()
    for i in range(5):
        for x, y in ((i, 0), (0, i), (TILE - 1 - i, 0), (TILE - 1, i),
                     (i, TILE - 1), (0, TILE - 1 - i),
                     (TILE - 1 - i, TILE - 1), (TILE - 1, TILE - 1 - i)):
            px[x, y] = (255, 232, 120, 255)
    cells.append(cur)

    strip = Image.new("RGBA", (TILE * len(cells), TILE), (0, 0, 0, 0))
    for i, c in enumerate(cells):
        strip.paste(c, (i * TILE, 0))
    strip = quantise(strip)
    strip.save(OUT / "units.png")
    n = len({(r, g, b) for (r, g, b, a) in strip.getdata() if a > 0})
    print(f"  units.png     {len(cells)} cells (2 creeps, 2 towers, cursor), {n} colours")


if __name__ == "__main__":
    print("tower-def assets")
    build_tileset()
    build_map()
    build_units()
