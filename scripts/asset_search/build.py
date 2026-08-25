"""Build the searchable index from the Ninja Adventure catalog.

Outputs (under assets/ninja-adventure/catalog/search/):
  records.jsonl      one text-searchable record per asset / region / tile / actor / item ...
  fingerprints.npz   image fingerprints: thumb (uint8), phash (uint64), avg (float32)
  fp_meta.jsonl      per-fingerprint metadata, row-aligned with the npz arrays
  manifest.json      build parameters + counts

Run:  python -m asset_search.build      (from scripts/)
      python scripts/asset_search/build.py
"""
from __future__ import annotations

import json
import os
import sys
from typing import Dict, List

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from asset_search.common import (  # noqa: E402
    CATALOG_DIR,
    FP,
    SEARCH_DIR,
    Fingerprint,
    resolve_asset,
)


def _load(name: str):
    with open(os.path.join(CATALOG_DIR, name), "r") as f:
        return json.load(f)


def _join(*parts) -> str:
    out = []
    for p in parts:
        if p is None:
            continue
        if isinstance(p, (list, tuple)):
            out.extend(str(x) for x in p if x)
        else:
            out.append(str(p))
    return " ".join(out)


# ---------------------------------------------------------------------------
# Text records
# ---------------------------------------------------------------------------
def build_records() -> List[dict]:
    recs: List[dict] = []

    # -- tilesets: whole-sheet, region, and notable-tile records ------------
    tilesets = _load("tilesets.json")
    for ts in tilesets:
        file = ts["file"]
        name = ts.get("name", file)
        cols = (ts.get("grid") or [0, 0])[0]
        region_txt = _join(*[
            _join(r.get("content"), r.get("tags"), r.get("area")) for r in ts.get("regions", [])
        ])
        notable_txt = _join(*[nt.get("what") for nt in ts.get("notable_tiles", [])])
        furniture_txt = _join(*[f.get("name") for f in ts.get("furniture", [])])
        recs.append({
            "id": f"tileset:{file}",
            "kind": "tileset",
            "file": file,
            "name": name,
            "tags": _tags_from(ts.get("theme"), ts.get("palette"), "tileset"),
            "grid": ts.get("grid"),
            "tile_size": ts.get("tile_size"),
            "autotile": bool((ts.get("autotile") or {}).get("has_autotile")) if isinstance(ts.get("autotile"), dict) else bool(ts.get("autotile")),
            "text": _join(name, name, name, ts.get("theme"), ts.get("theme"), ts.get("palette"),
                          ts.get("description"), ts.get("map_building_use"), region_txt,
                          notable_txt, furniture_txt),
        })
        for i, r in enumerate(ts.get("regions", [])):
            recs.append({
                "id": f"region:{file}#{i}",
                "kind": "region",
                "file": file,
                "name": f"{name} — {r.get('content','region')[:48]}",
                "tags": list(r.get("tags", [])),
                "area": r.get("area"),
                "text": _join(r.get("content"), r.get("tags"), r.get("tags"), name, ts.get("theme")),
            })
        for nt in ts.get("notable_tiles", []):
            col, row = nt.get("col"), nt.get("row")
            recs.append({
                "id": f"tile:{file}#c{col}r{row}",
                "kind": "tile",
                "file": file,
                "name": f"{name} tile ({col},{row})",
                "tags": _tags_from(nt.get("what")),
                "col": col, "row": row, "gid": nt.get("gid"),
                "text": _join(nt.get("what"), nt.get("what"), name, ts.get("theme")),
            })

    # -- actors --------------------------------------------------------------
    actors = _load("actors.json")
    for a in actors.get("actors", []):
        sheet = (a.get("sheet") or {}).get("file")
        recs.append({
            "id": f"actor:{a.get('name')}",
            "kind": "actor",
            "file": sheet,
            "name": a.get("name"),
            "tags": _tags_from(a.get("group"), "actor", "character"),
            "text": _join(a.get("name"), a.get("name"), a.get("group"), "actor character",
                          (a.get("sheet") or {}).get("note")),
        })

    # -- structural catalogs: items / fx / ui --------------------------------
    for cat in ("items", "fx", "ui"):
        d = _load(f"{cat}.json")
        for a in d.get("assets", []):
            recs.append({
                "id": f"{cat}:{a.get('file')}",
                "kind": cat[:-1] if cat.endswith("s") else cat,  # item / fx / ui
                "file": a.get("file"),
                "name": a.get("name"),
                "tags": _tags_from(a.get("category"), a.get("type"), cat),
                "text": _join(a.get("name"), a.get("name"), a.get("category"), a.get("type"), cat),
            })

    return recs


def _tags_from(*vals) -> List[str]:
    tags: List[str] = []
    for v in vals:
        if not v:
            continue
        if isinstance(v, (list, tuple)):
            tags.extend(str(x) for x in v if x)
        else:
            # split descriptive phrases into a few keyword tags
            for w in str(v).replace("/", " ").replace(",", " ").split():
                w = w.strip("()—-").lower()
                if len(w) >= 3:
                    tags.append(w)
    # dedupe, preserve order
    seen, out = set(), []
    for t in tags:
        if t not in seen:
            seen.add(t)
            out.append(t)
    return out[:24]


# ---------------------------------------------------------------------------
# Image fingerprints
# ---------------------------------------------------------------------------
def build_fingerprints():
    thumbs: List[np.ndarray] = []
    phashes: List[int] = []
    avgs: List[np.ndarray] = []
    meta: List[dict] = []

    tileset_files = set()

    # -- tiles: slice every tileset into its grid --------------------------
    tilesets = _load("tilesets.json")
    for ts in tilesets:
        file = ts["file"]
        tileset_files.add(file)
        path = resolve_asset(file)
        if not path:
            print(f"  ! tileset image not found: {file}", file=sys.stderr)
            continue
        ts_size = int(ts.get("tile_size") or 16)
        with Image.open(path) as im:
            im = im.convert("RGBA")
            W, H = im.size
            grid = ts.get("grid") or [W // ts_size, H // ts_size]
            cols, rows = int(grid[0]), int(grid[1])
            for trow in range(rows):
                for tcol in range(cols):
                    box = (tcol * ts_size, trow * ts_size, (tcol + 1) * ts_size, (trow + 1) * ts_size)
                    if box[2] > W or box[3] > H:
                        continue
                    fp = Fingerprint.from_image(im.crop(box))
                    thumbs.append(fp.thumb)
                    phashes.append(int(fp.phash))
                    avgs.append(fp.avg)
                    meta.append({
                        "kind": "tile",
                        "file": file,
                        "name": ts.get("name"),
                        "col": tcol, "row": trow,
                        "gid": trow * cols + tcol + 1,
                        "tile_size": ts_size,
                        "exact": fp.exact,
                        "opaque": float((fp.thumb[:, :, 3] > 0).mean()),
                    })

    # -- whole-image fingerprints for every non-tileset asset --------------
    index = _load("index.json")
    for a in index.get("assets", []):
        file = a.get("file")
        if not file or file in tileset_files or a.get("group") == "tileset":
            continue
        path = resolve_asset(file)
        if not path:
            continue
        try:
            fp = Fingerprint.from_path(path)
        except Exception as e:  # noqa: BLE001 - skip unreadable assets, keep building
            print(f"  ! skip {file}: {e}", file=sys.stderr)
            continue
        thumbs.append(fp.thumb)
        phashes.append(int(fp.phash))
        avgs.append(fp.avg)
        meta.append({
            "kind": "image",
            "file": file,
            "name": a.get("name"),
            "group": a.get("group"),
            "exact": fp.exact,
            "opaque": float((fp.thumb[:, :, 3] > 0).mean()),
        })

    thumb_arr = np.stack(thumbs).astype(np.uint8) if thumbs else np.zeros((0, FP, FP, 4), np.uint8)
    phash_arr = np.array(phashes, dtype=np.uint64)
    avg_arr = np.stack(avgs).astype(np.float32) if avgs else np.zeros((0, 4), np.float32)
    return thumb_arr, phash_arr, avg_arr, meta


def main():
    os.makedirs(SEARCH_DIR, exist_ok=True)
    print("Building text records ...")
    recs = build_records()
    with open(os.path.join(SEARCH_DIR, "records.jsonl"), "w") as f:
        for r in recs:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    kinds: Dict[str, int] = {}
    for r in recs:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    print(f"  {len(recs)} records: {kinds}")

    print("Building image fingerprints ...")
    thumb, phash, avg, meta = build_fingerprints()
    np.savez_compressed(os.path.join(SEARCH_DIR, "fingerprints.npz"),
                        thumb=thumb, phash=phash, avg=avg)
    with open(os.path.join(SEARCH_DIR, "fp_meta.jsonl"), "w") as f:
        for m in meta:
            f.write(json.dumps(m, ensure_ascii=False) + "\n")
    fp_kinds: Dict[str, int] = {}
    for m in meta:
        fp_kinds[m["kind"]] = fp_kinds.get(m["kind"], 0) + 1
    print(f"  {len(meta)} fingerprints: {fp_kinds}")

    manifest = {
        "fp_size": FP,
        "records": len(recs),
        "record_kinds": kinds,
        "fingerprints": len(meta),
        "fingerprint_kinds": fp_kinds,
    }
    with open(os.path.join(SEARCH_DIR, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    print(f"Done -> {SEARCH_DIR}")


if __name__ == "__main__":
    main()
