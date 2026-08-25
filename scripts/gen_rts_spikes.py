#!/usr/bin/env python3
"""Generate the assets for the three RTS de-risk spikes.

Writes:
  examples/rts-*/assets/rts_tiles.png  — a LOCAL tileset: grass, dirt, wall, shroud, half-shroud
  examples/rts-*/assets/rts_tiles.tsj  — its Tiled sidecar
  examples/rts-*/assets/maze.tmj       — a 30x20 obstacle course referencing that tileset

Tiles are cropped from the pixel-verified Ninja Adventure catalog, but into a LOCAL tileset rather
than referencing the shared .tsj library — because of the fog.

`tilemap_new` uploads its asset's palettes to all sixteen background banks, so a shroud layer built
from its own black-and-transparent PNG repaints the entire map black (measured), and building it
before the scene instead leaves the shroud drawing in whatever colour the map keeps at that palette
index (measured: brown). Putting the two shroud cells in the MAP's tileset makes both layers share
one palette by construction, which is the only arrangement of the three that is correct. This is the
same local-tileset shape `scripts/gen_wsg.py` generates for warsong.

Walls are painted on a layer named `Solid`, NOT `Collision`: `Collision` can only force cells
*walkable* and an empty cell there ERASES whatever collision the tileset declared, so it cannot
author a wall (crates/tish-gba-scenepack/src/tiled.rs documents the pair).

    python3 scripts/gen_rts_spikes.py
"""

from __future__ import annotations

import json
import pathlib

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
LIB = "../../../assets/ninja-adventure/tiled"

W, H = 30, 20
TILE = 16
NA = ROOT / "assets/ninja-adventure"

# The local tileset, in order. Indices here ARE the gids-minus-one the map and `fog_blit` use, so
# this list is the contract between the generator, the .tmj and the ROM.
T_GRASS, T_DIRT, T_WALL, T_SHROUD, T_HALF = 0, 1, 2, 3, 4
TS_COLS = 5
GID_SHROUD = T_SHROUD + 1
GID_HALF = T_HALF + 1
SOLID_MARK = 100  # Collision.tsj firstgid: any non-zero cell on `Solid` forces solid


def crop(rel: str, col: int, row: int) -> Image.Image:
    """One 16px cell out of a catalog tileset, addressed the way catalog/tilesets.md addresses it."""
    src = Image.open(NA / "Backgrounds" / "Tilesets" / rel).convert("RGBA")
    x, y = col * TILE, row * TILE
    return src.crop((x, y, x + TILE, y + TILE))


def build_tileset(out_png: pathlib.Path, out_tsj: pathlib.Path) -> None:
    """Terrain + the two shroud cells in ONE image, so the map and the fog share one palette.

    The shroud cells are plain black: opaque for unseen, a 50% checkerboard for explored. The
    checkerboard is a dither rather than a hardware blend because the GBA has one blend register per
    layer and an RTS wants it for something else.
    """
    sheet = Image.new("RGBA", (TILE * TS_COLS, TILE), (0, 0, 0, 0))
    sheet.paste(crop("TilesetFloor.png", 0, 12), (T_GRASS * TILE, 0))  # plain grass fill
    sheet.paste(crop("TilesetFloor.png", 1, 8), (T_DIRT * TILE, 0))  # solid dirt
    sheet.paste(crop("TilesetHouse.png", 11, 9), (T_WALL * TILE, 0))  # fortress wall body
    px = sheet.load()
    for y in range(TILE):
        for x in range(TILE):
            px[T_SHROUD * TILE + x, y] = (0, 0, 0, 255)
            if (x + y) & 1:
                px[T_HALF * TILE + x, y] = (0, 0, 0, 255)
    out_png.parent.mkdir(parents=True, exist_ok=True)
    sheet.save(out_png)

    out_tsj.write_text(
        json.dumps(
            {
                "columns": TS_COLS,
                "image": out_png.name,
                "imagewidth": TILE * TS_COLS,
                "imageheight": TILE,
                "margin": 0,
                "name": "rts_tiles",
                "spacing": 0,
                "tilecount": TS_COLS,
                "tiledversion": "1.11.0",
                "tileheight": TILE,
                "tilewidth": TILE,
                "type": "tileset",
                "version": "1.10",
            },
            indent=1,
        )
    )
    print(f"{out_png.relative_to(ROOT)}  {TS_COLS} cells (grass dirt wall shroud half)")


def maze() -> tuple[list[int], list[int]]:
    """Ground gids and the Solid overlay for a course that has no straight line through it.

    Three staggered barriers with a single gap each: a unit that merely walks toward the goal gets
    stuck on the first one, so the ROM shows at a glance whether the field is being followed or the
    units are just steering at the target.
    """
    ground = [T_GRASS + 1] * (W * H)
    solid = [0] * (W * H)

    def wall(c: int, r: int) -> None:
        ground[r * W + c] = T_WALL + 1
        solid[r * W + c] = SOLID_MARK

    for c in range(W):  # border
        wall(c, 0)
        wall(c, H - 1)
    for r in range(H):
        wall(0, r)
        wall(W - 1, r)

    # Three vertical barriers, gaps at top / bottom / middle so the route has to weave.
    for col, gap in ((8, 2), (15, H - 4), (22, H // 2)):
        for r in range(1, H - 1):
            if abs(r - gap) > 1:
                wall(col, r)

    # A dirt lane through each gap, purely so the intended route reads in a screenshot.
    for col, gap in ((8, 2), (15, H - 4), (22, H // 2)):
        ground[gap * W + col] = T_DIRT + 1

    return ground, solid


def layer(name: str, data: list[int], lid: int) -> dict:
    return {
        "id": lid,
        "name": name,
        "type": "tilelayer",
        "data": data,
        "width": W,
        "height": H,
        "x": 0,
        "y": 0,
        "opacity": 1,
        "visible": name != "Solid",
    }


def write_map(out: pathlib.Path) -> None:
    ground, solid = maze()
    m = {
        "type": "map",
        "version": "1.10",
        "tiledversion": "1.11.0",
        "orientation": "orthogonal",
        "renderorder": "right-down",
        "width": W,
        "height": H,
        "tilewidth": TILE,
        "tileheight": TILE,
        "infinite": False,
        "nextlayerid": 4,
        "nextobjectid": 1,
        "tilesets": [
            {"firstgid": 1, "source": "rts_tiles.tsj"},
            {"firstgid": SOLID_MARK, "source": f"{LIB}/Collision.tsj"},
        ],
        "layers": [layer("Ground", ground, 1), layer("Solid", solid, 2)],
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(m, indent=1))
    print(f"{out.relative_to(ROOT)}  {W}x{H} three-barrier course")


def main() -> None:
    for ex in ("rts-flow", "rts-fog", "rts-select"):
        d = ROOT / "examples" / ex / "assets"
        build_tileset(d / "rts_tiles.png", d / "rts_tiles.tsj")
        write_map(d / "maze.tmj")


if __name__ == "__main__":
    main()
