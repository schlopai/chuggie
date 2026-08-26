#!/usr/bin/env python3
"""Convert the Tiled battle map (tiled/battle.tmj) into a tish data module (src/battle_map.tish) the
example imports. Reads the `terrain` layer (→ render frame + walkable), the `height` layer (→
elevation), and the `units` object layer (→ spawns). This is the "Tiled map → engine data" build
step; re-run after editing battle.tmj."""
import json, os

D = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TMJ = os.path.join(D, "tiled/battle.tmj")
OUT = os.path.join(D, "src/battle_map.tish")
FLIP = 0x1FFFFFFF

d = json.load(open(TMJ))
W, H = d["width"], d["height"]
firstgid = {os.path.basename(t["source"]): t["firstgid"] for t in d["tilesets"]}
tg = firstgid["terrain.tsj"]
hg = firstgid["heights.tsj"]

layers = {L["name"]: L for L in d["layers"] if L["type"] == "tilelayer"}
terr = layers["terrain"]["data"]
hgt = layers["height"]["data"]

# terrain frame = GID - terrain_firstgid (0 grass,1 water,2 stone,3 tall); water(1) is unwalkable.
frames = [((terr[i] & FLIP) - tg) if (terr[i] & FLIP) else 0 for i in range(W * H)]
heights = [((hgt[i] & FLIP) - hg + 1) if (hgt[i] & FLIP) else 0 for i in range(W * H)]
walk = [0 if frames[i] == 1 else 1 for i in range(W * H)]  # water not walkable

units = []
for L in d["layers"]:
    if L["type"] == "objectgroup" and L["name"] == "units":
        for o in L["objects"]:
            p = {pr["name"]: pr["value"] for pr in o.get("properties", [])}
            units.append((int(o["x"] // 32), int(o["y"] // 32), p.get("cls", 0), p.get("team", 0)))

def arr(xs):
    return "[" + ", ".join(str(x) for x in xs) + "]"

lines = [
    "// GENERATED from tiled/battle.tmj by tools/import_battle.py — do not edit by hand.",
    "// The battlefield as engine data: per-cell render frame + elevation + walkability, and unit spawns.",
    f"export let MAPW = {W}",
    f"export let MAPH = {H}",
    f"export let frames = {arr(frames)}",
    f"export let heights = {arr(heights)}",
    f"export let walkable = {arr(walk)}",
    "export let units = [",
]
for (c, r, cls, team) in units:
    lines.append(f"  {{ col: {c}, row: {r}, cls: {cls}, team: {team} }},")
lines.append("]")
open(OUT, "w").write("\n".join(lines) + "\n")
print(f"wrote {OUT}: {W}x{H}, {len(units)} units")
