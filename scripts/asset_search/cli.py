"""Command-line interface for the asset search index. Prints JSON.

    python -m asset_search.cli build
    python -m asset_search.cli text "cozy wooden interior floor" --kind tileset --limit 5
    python -m asset_search.cli image path/to/tile.png --limit 5
    python -m asset_search.cli tilemap path/to/screenshot.png --tile-size 16
    python -m asset_search.cli get "tileset:Backgrounds/Tilesets/Interior/Elements.png"
"""
from __future__ import annotations

import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _emit(obj):
    print(json.dumps(obj, ensure_ascii=False, indent=1))


def main(argv=None):
    p = argparse.ArgumentParser(prog="asset_search", description="Search the asset catalog.")
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("build", help="(re)build the search index")

    pt = sub.add_parser("text", help="keyword/BM25 search over catalog metadata")
    pt.add_argument("query")
    pt.add_argument("--kind", default=None,
                    help="filter: tileset,region,tile,actor,item,fx,ui (comma-separated)")
    pt.add_argument("--limit", type=int, default=10)

    pi = sub.add_parser("image", help="find the assets/tiles most similar to an image")
    pi.add_argument("path")
    pi.add_argument("--kind", default=None, choices=[None, "tile", "image"])
    pi.add_argument("--tileset", default=None, help="restrict to one tileset file")
    pi.add_argument("--limit", type=int, default=10)

    pm = sub.add_parser("tilemap", help="slice an image into a grid and identify each tile")
    pm.add_argument("path")
    pm.add_argument("--tile-size", type=int, default=16)
    pm.add_argument("--tileset", default=None, help="restrict matches to one tileset file")
    pm.add_argument("--kind", default="tile", choices=["tile", "image"])
    pm.add_argument("--grid-only", action="store_true", help="emit just the 2D gid grid")

    pg = sub.add_parser("get", help="fetch a full record by id")
    pg.add_argument("id")

    args = p.parse_args(argv)

    if args.cmd == "build":
        from asset_search import build
        build.main()
        return

    from asset_search import AssetSearch
    s = AssetSearch()

    if args.cmd == "text":
        kind = args.kind.split(",") if args.kind else None
        _emit(s.search_text(args.query, kind=kind, limit=args.limit))
    elif args.cmd == "image":
        _emit(s.search_image(args.path, limit=args.limit, kind=args.kind, tileset=args.tileset))
    elif args.cmd == "tilemap":
        res = s.match_tilemap(args.path, tile_size=args.tile_size,
                              tileset=args.tileset, kind=args.kind)
        if args.grid_only:
            _emit([[c["gid"] for c in row] for row in res["grid"]])
        else:
            _emit(res)
    elif args.cmd == "get":
        _emit(s.get(args.id))


if __name__ == "__main__":
    main()
