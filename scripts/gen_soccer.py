#!/usr/bin/env python3
"""Generate examples/soccer's pitch (.tmj) and its one sprite sheet.

The pitch is a Tiled map for the same reason every map in this repo is: `bench-boot` measured a
per-tile tish marking loop at ~0.175 frames PER TILE. Walls come from a **`Solid`** layer —
`Collision` is the opposite, it forces cells WALKABLE and an empty cell there erases whatever the
tileset said.

The goal MOUTHS are gaps in the wall, not tiles with a property: a goal is a region the ball can
pass through, and the ROM tests the ball's position against it. Tiles that stop the ball would be a
goal you cannot score in.

    python3 scripts/gen_soccer.py
"""

import json
import pathlib
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/soccer/assets"

W, H = 22, 14
TILE = 16
TSJ = "../../../assets/ninja-adventure/tiled/TilesetField.tsj"
FIRSTGID = 1

G_PITCH = FIRSTGID + 18    # spring_grass centre
G_WALL = FIRSTGID + 63     # field_snow centre — the hoarding
G_GOAL = FIRSTGID + 33     # summer_grass centre — the goal mouth floor, so it reads as a target

# The goal mouth spans these rows on each side wall. Four rows of a fourteen-row pitch: wide enough
# to score through, narrow enough that a stray ball usually rebounds.
GOAL_R0, GOAL_R1 = 5, 8


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    ground = [G_PITCH] * (W * H)
    solid = [0] * (W * H)

    for c in range(W):
        for r in (0, H - 1):
            ground[r * W + c] = G_WALL
            solid[r * W + c] = G_WALL
    for r in range(H):
        for c in (0, W - 1):
            if GOAL_R0 <= r <= GOAL_R1:
                # The mouth: painted, but NOT solid. This is the gap the ball goes through.
                ground[r * W + c] = G_GOAL
            else:
                ground[r * W + c] = G_WALL
                solid[r * W + c] = G_WALL

    def layer(name, data, lid):
        return {"type": "tilelayer", "name": name, "id": lid, "width": W, "height": H,
                "x": 0, "y": 0, "opacity": 1, "visible": True, "data": data}

    m = {
        "type": "map", "orientation": "orthogonal", "renderorder": "right-down",
        "infinite": False, "width": W, "height": H, "tilewidth": TILE, "tileheight": TILE,
        "nextlayerid": 3, "nextobjectid": 1, "version": "1.10", "tiledversion": "1.11.0",
        "tilesets": [{"firstgid": FIRSTGID, "source": TSJ}],
        "layers": [layer("Ground", ground, 1), layer("Solid", solid, 2)],
    }
    (OUT / "pitch.tmj").write_text(json.dumps(m, indent=1))
    print(f"pitch.tmj  {W}x{H}, goal rows {GOAL_R0}-{GOAL_R1}")

    # One sheet: ball, home shirt, away shirt. Drawn rather than taken from the pack — nothing in it
    # reads as a football or a kit at 8px, and three discs are honest about that.
    CELL = 8
    strip = Image.new("RGBA", (CELL * 3, CELL), (0, 0, 0, 0))
    for i, (fill, edge) in enumerate([
        ((248, 248, 248, 255), (40, 40, 48, 255)),     # 0 ball
        ((72, 128, 240, 255), (24, 40, 96, 255)),      # 1 home
        ((240, 96, 72, 255), (96, 24, 24, 255)),       # 2 away
    ]):
        cell = Image.new("RGBA", (CELL, CELL), (0, 0, 0, 0))
        d = ImageDraw.Draw(cell)
        d.ellipse([0, 0, CELL - 1, CELL - 1], fill=fill, outline=edge)
        strip.paste(cell, (i * CELL, 0))

    cols = {p for p in strip.getdata() if p[3] > 0}
    assert len(cols) <= 15, f"{len(cols)} colours, a 4bpp sheet holds 15"
    strip.save(OUT / "soccer8.png")
    print(f"soccer8.png  3 frames, {len(cols)} colours")


if __name__ == "__main__":
    main()
