#!/usr/bin/env python3
"""tileset_coverage.py — audit catalog/tilesets.json against the actual pixel content of every
tileset PNG. For each tileset: compute which 16x16 cells are OCCUPIED (contain any non-transparent
pixel), parse every coordinate reference out of the catalog entry (regions/structures/furniture/
autotile terrains/notable_tiles — "cols A-B rows C-D", "cols A-B row C", single (col,row) pairs)
to get the DOCUMENTED cell set, and report any occupied cell NOT covered by a documented region.

A tileset is "complete" when every occupied cell is documented. Run after editing tilesets.json to
re-check; run with --apply to write the coverage report + completeness flags into tilesets.json.

    python3 scripts/tileset_coverage.py            # report only
    python3 scripts/tileset_coverage.py --apply     # also write coverage+complete into tilesets.json
"""
import json, os, re, sys
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NA = os.path.join(ROOT, "assets/ninja-adventure")
TS_JSON = os.path.join(NA, "catalog/tilesets.json")

RANGE_RE = re.compile(r"cols?\s+(\d+)(?:\s*-\s*(\d+))?\s+rows?\s+(\d+)(?:\s*-\s*(\d+))?", re.I)
PAIR_RE = re.compile(r"\((\d+)\s*,\s*(\d+)\)")


def occupied_cells(png_path, cols, rows):
    im = Image.open(png_path).convert("RGBA")
    occ = set()
    px = im.load()
    for r in range(rows):
        for c in range(cols):
            found = False
            for y in range(r * 16, r * 16 + 16):
                for x in range(c * 16, c * 16 + 16):
                    if px[x, y][3] != 0:
                        found = True
                        break
                if found:
                    break
            if found:
                occ.add((c, r))
    return occ


def parse_cells(text, cols, rows):
    """Extract every (col,row) cell implied by a free-text coordinate reference."""
    cells = set()
    for m in RANGE_RE.finditer(text):
        c0 = int(m.group(1)); c1 = int(m.group(2)) if m.group(2) else c0
        r0 = int(m.group(3)); r1 = int(m.group(4)) if m.group(4) else r0
        for c in range(min(c0, c1), max(c0, c1) + 1):
            for r in range(min(r0, r1), max(r0, r1) + 1):
                if 0 <= c < cols and 0 <= r < rows:
                    cells.add((c, r))
    for m in PAIR_RE.finditer(text):
        c, r = int(m.group(1)), int(m.group(2))
        if 0 <= c < cols and 0 <= r < rows:
            cells.add((c, r))
    return cells


def documented_cells(entry, cols, rows):
    blobs = []
    for r in entry.get("regions", []):
        blobs.append(r.get("area", "") + " " + r.get("content", "") + " " + r.get("notes", ""))
    for s in entry.get("structures", []):
        blobs.append(s.get("region", "") + " " + s.get("notes", ""))
        for d in s.get("door_tiles", []):
            blobs.append(f"({d.get('col')},{d.get('row')})")
        parts = s.get("parts", [])
        if isinstance(parts, list):
            blobs.extend(str(p) for p in parts)
        elif isinstance(parts, str):
            blobs.append(parts)
    for f in entry.get("furniture", []):
        blobs.append(f.get("region", "") + " " + f.get("notes", ""))
    for t in entry.get("autotile", {}).get("terrains", []):
        blobs.append(t.get("region", "") + " " + t.get("notes", ""))
    for n in entry.get("notable_tiles", []):
        blobs.append(f"({n.get('col')},{n.get('row')})")
        blobs.append(n.get("what", ""))
    cells = set()
    for b in blobs:
        cells |= parse_cells(b, cols, rows)
    return cells


def main():
    apply = "--apply" in sys.argv
    ts = json.load(open(TS_JSON))
    report = []
    for entry in ts:
        cols, rows = entry["grid"]
        png = os.path.join(NA, entry["file"])
        occ = occupied_cells(png, cols, rows)
        doc = documented_cells(entry, cols, rows)
        missing = sorted(occ - doc)
        cov = {
            "occupied": len(occ), "documented": len(occ & doc),
            "missing_count": len(missing),
            "missing_cells": [list(m) for m in missing],
            "complete": len(missing) == 0,
        }
        entry["coverage"] = cov
        report.append((entry["name"], cov))
        flag = "COMPLETE" if cov["complete"] else f"MISSING {cov['missing_count']}"
        print(f"{entry['name']:32} occupied={cov['occupied']:4} documented={cov['documented']:4}  [{flag}]")
        if missing and not apply:
            print("    gaps:", missing[:20], "..." if len(missing) > 20 else "")
    if apply:
        json.dump(ts, open(TS_JSON, "w"), indent=1)
        print("\nwrote coverage into", TS_JSON)
    n_complete = sum(1 for _, c in report if c["complete"])
    print(f"\n{n_complete}/{len(report)} tilesets complete")


if __name__ == "__main__":
    main()
