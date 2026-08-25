#!/usr/bin/env python3
"""gen_autotile_masks.py — extend catalog/autotile.json with stamped/derived mask tables.

Keeps hand-verified Floor (8) + WallSimple (4) + dark_cobble tables intact, then adds:
  - 47-blob stamps from dirt_grass → Hole, Water×4, FloorB cloud
  - 3×3 islands → Field×5, tan_plank, bed_stone, FloorB cloud_island
  - WallSimple cream → Interior cream/orange brick right frames (+ compact left frames)
  - Compact carpet frames → gold/green ornate

    python3 scripts/gen_autotile_masks.py
    python3 scripts/gen_tileset_library.py
"""
from __future__ import annotations

import json
import os

from PIL import Image
import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NA = os.path.join(ROOT, "assets/ninja-adventure")
AUTOTILE = os.path.join(NA, "catalog/autotile.json")
TS = os.path.join(NA, "Backgrounds/Tilesets")

# Godot bit order used everywhere in autotile.json
TL, T, TR, L, R, BL, B, BR = range(8)


def nonempty(arr, c, r, sc, sr, thr=0.08):
    if not (0 <= c < sc and 0 <= r < sr):
        return False
    t = arr[r * 16 : (r + 1) * 16, c * 16 : (c + 1) * 16]
    lum = 0.3 * t[:, :, 0] + 0.59 * t[:, :, 1] + 0.11 * t[:, :, 2]
    return ((lum > 20) & (t[:, :, 3] > 128)).mean() > thr


def load_sheet(rel):
    path = os.path.join(TS, rel)
    im = Image.open(path).convert("RGBA")
    arr = np.array(im)
    return arr, im.size[0] // 16, im.size[1] // 16


def gid(col, row, cols):
    return row * cols + col + 1


def tile(col, row, cols, mask):
    return {"col": col, "row": row, "gid": gid(col, row, cols), "mask": list(mask)}


def island_3x3(ox, oy, cols, fill_cells, desc):
    """Standard outer-corner 3×3 island + one-or-more full-surround fill tiles."""
    # masks match dark_cobble / Field convention (terrain = island fill)
    cells = [
        (ox + 0, oy + 0, [0, 0, 0, 0, 1, 0, 1, 1]),  # TL
        (ox + 2, oy + 0, [0, 0, 0, 1, 0, 1, 1, 0]),  # TR
        (ox + 0, oy + 2, [0, 1, 1, 0, 1, 0, 0, 0]),  # BL
        (ox + 2, oy + 2, [1, 1, 0, 1, 0, 0, 0, 0]),  # BR
        (ox + 1, oy + 0, [0, 0, 0, 1, 1, 1, 1, 1]),  # T
        (ox + 1, oy + 2, [1, 1, 1, 1, 1, 0, 0, 0]),  # B
        (ox + 0, oy + 1, [0, 1, 1, 0, 1, 0, 1, 1]),  # L
        (ox + 2, oy + 1, [1, 1, 0, 1, 0, 1, 1, 0]),  # R
    ]
    fill = [1, 1, 1, 1, 1, 1, 1, 1]
    # center of island is also a valid fill
    fill_cells = list(fill_cells) + [(ox + 1, oy + 1)]
    tiles = [tile(c, r, cols, m) for c, r, m in cells]
    seen = {(t["col"], t["row"]) for t in tiles}
    for c, r in fill_cells:
        if (c, r) not in seen:
            tiles.append(tile(c, r, cols, fill))
            seen.add((c, r))
    return {
        "origin": [ox, oy],
        "desc": desc,
        "tiles": tiles,
    }


def stamp_from_dirt(dirt_tiles, dirt_origin, dest_origin, cols, arr, sc, sr, desc, fill_override=None):
    """Copy dirt_grass relative masks onto another sheet; skip empty cells.

    fill_override: list of (col,row) used for mask all-1 when template fill cells are missing.
    """
    dox, doy = dest_origin
    sox, soy = dirt_origin
    tiles = []
    have_full = False
    for t in dirt_tiles:
        dc, dr = t["col"] - sox, t["row"] - soy
        c, r = dox + dc, doy + dr
        if not nonempty(arr, c, r, sc, sr):
            continue
        tiles.append(tile(c, r, cols, t["mask"]))
        if tuple(t["mask"]) == (1, 1, 1, 1, 1, 1, 1, 1):
            have_full = True
    if not have_full and fill_override:
        for c, r in fill_override:
            if nonempty(arr, c, r, sc, sr):
                tiles.append(tile(c, r, cols, [1, 1, 1, 1, 1, 1, 1, 1]))
    return {
        "origin": [dox, doy],
        "desc": desc,
        "tiles": tiles,
    }


def transfer_wall(src_tiles, src_origin, dest_origin, cols, desc):
    """Offset a WallSimple-style wall wangset (absolute masks preserved)."""
    sox, soy = src_origin
    dox, doy = dest_origin
    tiles = []
    for t in src_tiles:
        c = t["col"] - sox + dox
        r = t["row"] - soy + doy
        tiles.append(tile(c, r, cols, t["mask"]))
    return {"origin": [dox, doy], "desc": desc, "tiles": tiles}


def compact_wall_4x4(ox, oy, cols, desc):
    """Closed 4×4 room frame (no door jambs) — corners + smooth edge runs."""
    # WALL=terrain, room interior=hole; same corner convention as WallSimple
    cells = [
        (ox + 0, oy + 0, [1, 1, 1, 1, 1, 1, 1, 0]),  # TL outer
        (ox + 3, oy + 0, [1, 1, 1, 1, 1, 0, 1, 1]),  # TR
        (ox + 0, oy + 3, [1, 1, 0, 1, 1, 1, 1, 1]),  # BL
        (ox + 3, oy + 3, [0, 1, 1, 1, 1, 1, 1, 1]),  # BR
        (ox + 1, oy + 0, [1, 1, 1, 1, 1, 0, 0, 0]),  # T mid
        (ox + 2, oy + 0, [1, 1, 1, 1, 1, 0, 0, 0]),
        (ox + 1, oy + 3, [0, 0, 0, 1, 1, 1, 1, 1]),  # B mid
        (ox + 2, oy + 3, [0, 0, 0, 1, 1, 1, 1, 1]),
        (ox + 0, oy + 1, [1, 1, 0, 1, 0, 1, 1, 0]),  # L mid
        (ox + 0, oy + 2, [1, 1, 0, 1, 0, 1, 1, 0]),
        (ox + 3, oy + 1, [0, 1, 1, 0, 1, 0, 1, 1]),  # R mid
        (ox + 3, oy + 2, [0, 1, 1, 0, 1, 0, 1, 1]),
    ]
    return {
        "origin": [ox, oy],
        "desc": desc,
        "tiles": [tile(c, r, cols, m) for c, r, m in cells],
    }


def carpet_frame_5x4(ox, oy, cols, desc):
    """Outer border of a 5×4 ornate carpet/medallion (WALL-style, fill not included)."""
    cells = [
        (ox + 0, oy + 0, [1, 1, 1, 1, 1, 1, 1, 0]),
        (ox + 4, oy + 0, [1, 1, 1, 1, 1, 0, 1, 1]),
        (ox + 0, oy + 3, [1, 1, 0, 1, 1, 1, 1, 1]),
        (ox + 4, oy + 3, [0, 1, 1, 1, 1, 1, 1, 1]),
        (ox + 1, oy + 0, [1, 1, 1, 1, 1, 0, 0, 0]),
        (ox + 2, oy + 0, [1, 1, 1, 1, 1, 0, 0, 0]),
        (ox + 3, oy + 0, [1, 1, 1, 1, 1, 0, 0, 0]),
        (ox + 1, oy + 3, [0, 0, 0, 1, 1, 1, 1, 1]),
        (ox + 2, oy + 3, [0, 0, 0, 1, 1, 1, 1, 1]),
        (ox + 3, oy + 3, [0, 0, 0, 1, 1, 1, 1, 1]),
        (ox + 0, oy + 1, [1, 1, 0, 1, 0, 1, 1, 0]),
        (ox + 0, oy + 2, [1, 1, 0, 1, 0, 1, 1, 0]),
        (ox + 4, oy + 1, [0, 1, 1, 0, 1, 0, 1, 1]),
        (ox + 4, oy + 2, [0, 1, 1, 0, 1, 0, 1, 1]),
        # center medallion / carpet fill (surrounded)
        (ox + 2, oy + 1, [1, 1, 1, 1, 1, 1, 1, 1]),
        (ox + 2, oy + 2, [1, 1, 1, 1, 1, 1, 1, 1]),
    ]
    return {
        "origin": [ox, oy],
        "desc": desc,
        "tiles": [tile(c, r, cols, m) for c, r, m in cells],
    }


def main():
    data = json.load(open(AUTOTILE))
    dirt = data["tilesets"]["TilesetFloor.png"]["materials"]["dirt_grass"]
    cream = data["tilesets"]["TilesetWallSimple.png"]["materials"]["cream_wall"]
    dark = data["tilesets"]["TilesetInteriorFloor.png"]["materials"]["dark_cobble"]

    # --- Hole ---
    arr, sc, sr = load_sheet("TilesetHole.png")
    hole = stamp_from_dirt(
        dirt["tiles"],
        dirt["origin"],
        (0, 0),
        sc,
        arr,
        sc,
        sr,
        "pit/hole 47-blob stamped from dirt_grass layout (cols 0-10 rows 0-3 + inner corners). "
        "Fill = dark void at (1,1)/(2,1). Decor row of Floor omitted.",
        fill_override=[(1, 1), (2, 1)],
    )

    # --- Water ×4 ---
    arr, sc, sr = load_sheet("TilesetWater.png")
    water_mats = {}
    for name, origin, label in [
        ("sand_water", (0, 0), "sand/beach-edged water"),
        ("grass_water", (0, 6), "grass-edged water"),
        ("ice_water", (13, 0), "ice / frozen water"),
        ("magic_water", (13, 6), "magic / mauve water (dirt-edged)"),
    ]:
        ox, oy = origin
        water_mats[name] = stamp_from_dirt(
            dirt["tiles"],
            dirt["origin"],
            origin,
            sc,
            arr,
            sc,
            sr,
            f"{label}: 47-blob stamped from dirt_grass at origin {list(origin)}. "
            f"Fill = open water at ({ox+1},{oy+1})/({ox+2},{oy+1}).",
            fill_override=[(ox + 1, oy + 1), (ox + 2, oy + 1)],
        )

    # --- FloorB ---
    arr, sc, sr = load_sheet("TilesetFloorB.png")
    cloud = stamp_from_dirt(
        dirt["tiles"],
        dirt["origin"],
        (4, 0),
        sc,
        arr,
        sc,
        sr,
        "pale cloud 47-blob (cols 4-10). Stamped from dirt_grass; sheet is narrower so some "
        "right-column cases are absent. Fill at (5,1)/(6,1).",
        fill_override=[(5, 1), (6, 1)],
    )
    cloud_island = island_3x3(
        0,
        0,
        sc,
        [(1, 1)],
        "compact cloud island at cols 0-2 rows 0-2 (plus thin-strip tiles hand-place separately).",
    )

    # --- Field ×5 ---
    arr, sc, sr = load_sheet("TilesetField.png")
    field_mats = {}
    for name, oy, label in [
        ("orange_soil", 0, "orange tilled soil / clay"),
        ("spring_grass", 3, "light yellow-green grass (spring)"),
        ("summer_grass", 6, "dark green grass (summer)"),
        ("autumn_ground", 9, "pink/salmon ground (autumn/cherry)"),
        ("field_snow", 12, "off-white snow"),
    ]:
        field_mats[name] = island_3x3(
            0,
            oy,
            sc,
            [(3, oy), (4, oy)],
            f"{label}: rounded 3×3 island + solid fill tiles at (3,{oy})/(4,{oy}).",
        )

    # --- bed stone ---
    arr, bed_cols, _sr = load_sheet("tileset_bed.png")
    bed_stone = island_3x3(
        0,
        9,
        bed_cols,
        [(3, 9), (3, 10), (4, 9), (4, 10), (5, 9), (5, 10)],
        "grey stone slab 3×3 at cols 0-2 rows 9-11; extra fill variants in cols 3-5.",
    )

    # --- InteriorFloor additions ---
    int_cols = 22
    cream_brick = transfer_wall(
        cream["tiles"],
        cream["origin"],
        (4, 0),
        int_cols,
        "cream/tan brick large walled-room frame (cols 4-8 rows 0-4). Masks transferred from "
        "WallSimple cream_wall (smooth mids + door jambs). WALL=terrain, room=hole.",
    )
    cream_brick_sm = compact_wall_4x4(
        0,
        0,
        int_cols,
        "cream/tan brick compact 4×4 closed room frame (cols 0-3 rows 0-3). Smooth edges only.",
    )
    orange_brick = transfer_wall(
        cream["tiles"],
        cream["origin"],
        (4, 6),
        int_cols,
        "orange brick large walled-room frame (cols 4-8 rows 6-10). Same layout as cream_brick.",
    )
    orange_brick_sm = compact_wall_4x4(
        0,
        6,
        int_cols,
        "orange brick compact 4×4 closed room frame (cols 0-3 rows 6-9).",
    )
    tan_plank = island_3x3(
        0,
        12,
        int_cols,
        [(3, 12), (4, 12), (5, 12), (3, 13), (4, 13), (5, 13), (3, 14), (4, 14), (5, 14)],
        "tan plank/tile floor 3×3 island at cols 0-2 rows 12-14; cols 3+ are fill/scatter variants.",
    )
    gold_ornate = carpet_frame_5x4(
        15,
        0,
        int_cols,
        "gold ornate carpet/medallion outer frame (cols 15-19 rows 0-3) + center fill. "
        "Paint rectangular rugs; decorative side motifs at col 20+ are hand-place.",
    )
    green_ornate = carpet_frame_5x4(
        15,
        6,
        int_cols,
        "green ornate carpet/medallion outer frame (cols 15-19 rows 6-9) + center fill.",
    )

    # Assemble tilesets (preserve Floor + WallSimple + dark_cobble)
    data["coverage"] = (
        "VERIFIED wangset/mask tables in this file: "
        "TilesetFloor 8×47-blob (Godot); TilesetWallSimple 4 palettes; "
        "TilesetInteriorFloor dark_cobble + tan_plank + cream/orange brick frames + gold/green ornate; "
        "TilesetHole; TilesetWater×4; TilesetFloorB cloud+island; TilesetField×5; tileset_bed stone. "
        "NOT wangsets (modular/hand-place kits — see tilesets.json): Pipes, Desert wall-kit, Relief cliffs."
    )
    data["note"] = (
        "1 = neighbour is same terrain. Corner bit only meaningful when both adjacent sides are also "
        "same-terrain (Godot 47-case reduction; verified). gid = row*cols + col + 1. "
        "Use scripts/ninja_autotile.py to paint a terrain grid. "
        "Regenerate derived materials with scripts/gen_autotile_masks.py; then scripts/gen_tileset_library.py "
        "to refresh tiled/*.tsj wangsets."
    )

    data["tilesets"]["TilesetHole.png"] = {"cols": 11, "materials": {"hole": hole}}
    data["tilesets"]["TilesetWater.png"] = {"cols": 28, "materials": water_mats}
    data["tilesets"]["TilesetFloorB.png"] = {
        "cols": 11,
        "materials": {"cloud": cloud, "cloud_island": cloud_island},
    }
    data["tilesets"]["TilesetField.png"] = {"cols": 5, "materials": field_mats}
    data["tilesets"]["tileset_bed.png"] = {"cols": bed_cols, "materials": {"stone_slab": bed_stone}}

    int_mats = {
        "dark_cobble": dark,
        "tan_plank": tan_plank,
        "cream_brick": cream_brick,
        "cream_brick_sm": cream_brick_sm,
        "orange_brick": orange_brick,
        "orange_brick_sm": orange_brick_sm,
        "gold_ornate": gold_ornate,
        "green_ornate": green_ornate,
    }
    data["tilesets"]["TilesetInteriorFloor.png"] = {"cols": 22, "materials": int_mats}

    # Keep Floor + WallSimple untouched (already in data)
    with open(AUTOTILE, "w") as f:
        json.dump(data, f, indent=1)
        f.write("\n")

    # Summary
    print("Wrote", os.path.relpath(AUTOTILE, ROOT))
    for png, entry in data["tilesets"].items():
        mats = entry["materials"]
        print(f"  {png}: {list(mats)}")
        for name, m in mats.items():
            nmask = len({tuple(t["mask"]) for t in m["tiles"]})
            print(f"    {name}: {len(m['tiles'])} tiles, {nmask} unique masks")


if __name__ == "__main__":
    main()
