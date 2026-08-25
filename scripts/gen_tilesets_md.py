#!/usr/bin/env python3
"""gen_tilesets_md.py — regenerate catalog/tilesets.md from catalog/tilesets.json (the single
source of truth). Every tileset section carries a coverage badge (occupied/documented cell counts
+ COMPLETE/INCOMPLETE) computed by scripts/tileset_coverage.py's own logic — run that script with
--apply first so the "coverage" field is fresh, then run this to sync the docs.

    python3 scripts/tileset_coverage.py --apply
    python3 scripts/gen_tilesets_md.py
"""
import json, os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TS_JSON = os.path.join(ROOT, "assets/ninja-adventure/catalog/tilesets.json")
MD_OUT = os.path.join(ROOT, "assets/ninja-adventure/catalog/tilesets.md")


def render(t):
    cov = t.get("coverage", {})
    badge = (f"**Coverage: {cov.get('documented','?')}/{cov.get('occupied','?')} occupied cells "
             f"documented — {'✅ COMPLETE' if cov.get('complete') else '⚠️ INCOMPLETE'}**")
    lines = [
        f"## {t['name']}  ({t['file']})  grid={tuple(t['grid'])}",
        "",
        badge,
        "",
        f"**Theme:** {t['theme']}",
        "",
        f"**Palette:** {t['palette']}",
        "",
        f"**Description:** {t['description']}",
        "",
    ]
    if t.get("autotile", {}).get("terrains"):
        lines.append("**Autotile:**")
        for terr in t["autotile"]["terrains"]:
            notes = f" — {terr['notes']}" if terr.get("notes") else ""
            lines.append(f"- [{terr.get('kind','')}] {terr.get('material','')}: {terr.get('region','')}{notes}")
        lines.append("")
    if t.get("structures"):
        lines.append("**Structures:**")
        for s in t["structures"]:
            doors = ""
            if s.get("door_tiles"):
                doors = " — doors: " + ", ".join(f"({d.get('col')},{d.get('row')})" for d in s["door_tiles"])
            notes = f" — {s['notes']}" if s.get("notes") else ""
            lines.append(f"- {s.get('name','')}: {s.get('region','')}{doors}{notes}")
        lines.append("")
    if t.get("furniture"):
        lines.append("**Furniture:**")
        for f in t["furniture"]:
            lines.append(f"- {f.get('name','')}: {f.get('region','')}")
        lines.append("")
    if t.get("regions"):
        lines.append("**Regions:**")
        for r in t["regions"]:
            tags = ",".join(r.get("tags", []))
            lines.append(f"- {r.get('area','')}: {r.get('content','')} `{tags}`")
        lines.append("")
    if t.get("notable_tiles"):
        lines.append("**Notable tiles:**")
        for n in t["notable_tiles"]:
            lines.append(f"- ({n.get('col')},{n.get('row')}) gid={n.get('gid')}: {n.get('what','')}")
        lines.append("")
    lines.append(f"**Map-building use:** {t.get('map_building_use','')}")
    lines.append("")
    return "\n".join(lines)


def main():
    ts = json.load(open(TS_JSON))
    n_complete = sum(1 for t in ts if t.get("coverage", {}).get("complete"))
    header = [
        "# Ninja Adventure — Tileset catalog",
        "",
        "Every map tileset in `Backgrounds/Tilesets/`: theme, tile grid, autotile regions, notable",
        "tiles (doors, stairs, water/cliff edges), and how to use each when building a map. Tile coords",
        "are (col,row); **gid = row*cols + col + 1**. Usable Tiled wangsets / mask tables live in",
        "[`autotile.json`](autotile.json) (Floor×8, WallSimple×4, InteriorFloor floors+walls+carpets,",
        "Hole, Water×4, FloorB, Field×5, bed stone). Modular kits (Pipes, Desert walls, Relief cliffs)",
        "are documented below but are **not** wangsets — hand-place. Regenerate masks with",
        "`scripts/gen_autotile_masks.py`, then `scripts/gen_tileset_library.py` for `tiled/*.tsj`.",
        "",
        f"**Pixel-level coverage: {n_complete}/{len(ts)} tilesets have every occupied tile cell",
        "accounted for** (verified by `scripts/tileset_coverage.py`, which cross-checks every",
        "non-transparent 16x16 cell against the region/structure/notable-tile text below — not just",
        "eyeballed). Regenerate this file after any catalog edit with `scripts/gen_tilesets_md.py`.",
        "",
    ]
    body = "\n".join(render(t) for t in ts)
    open(MD_OUT, "w").write("\n".join(header) + body)
    print(f"wrote {MD_OUT}: {len(ts)} tilesets, {n_complete} complete")


if __name__ == "__main__":
    main()
