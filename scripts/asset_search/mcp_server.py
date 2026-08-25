"""Zero-dependency MCP server exposing asset search as AI tools (stdio transport).

Implements the MCP JSON-RPC handshake directly (newline-delimited JSON on
stdin/stdout) so it runs on stock Python 3.9 with no `mcp` SDK dependency.

Tools:
  search_text     keyword/BM25 search over the catalog metadata
  search_image    find the tiles/assets most similar to an image (path or base64)
  match_tilemap   slice an image into a grid and identify each tile -> a gid map
  get_record      fetch a full catalog record by id
  list_tilesets   summarise the available tilesets (grid, theme, autotile)

Register in Claude Code via .mcp.json (see scripts/asset_search/README.md), or run
directly for a smoke test:  python -m asset_search.mcp_server
"""
from __future__ import annotations

import base64
import io
import json
import os
import sys
import traceback

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

PROTOCOL_VERSION = "2024-11-05"
SERVER_INFO = {"name": "ninja-adventure-asset-search", "version": "0.1.0"}

_engine = None  # lazy AssetSearch


def log(*a):
    print("[asset-search-mcp]", *a, file=sys.stderr, flush=True)


def engine():
    global _engine
    if _engine is None:
        from asset_search import AssetSearch
        _engine = AssetSearch()
        log("index loaded:", len(_engine.records), "records,", len(_engine.fp_meta), "fingerprints")
    return _engine


def _image_arg(args):
    """Return a path str or PIL.Image from {path} or {image_base64}."""
    if args.get("image_base64"):
        from PIL import Image
        raw = base64.b64decode(args["image_base64"])
        return Image.open(io.BytesIO(raw)).convert("RGBA")
    if args.get("path"):
        return args["path"]
    raise ValueError("provide either 'path' or 'image_base64'")


# ---------------------------------------------------------------------------
# Tool registry
# ---------------------------------------------------------------------------
TOOLS = [
    {
        "name": "search_text",
        "description": "Keyword search over the Ninja Adventure asset catalog (tilesets, tiles, "
                       "regions, actors, items, fx, ui) by name/tags/theme/description. Ranked by BM25.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "free-text query, e.g. 'cozy wooden interior floor'"},
                "kind": {"type": "string", "description": "comma-separated filter: tileset,region,tile,actor,item,fx,ui"},
                "limit": {"type": "integer", "default": 10},
            },
            "required": ["query"],
        },
    },
    {
        "name": "search_image",
        "description": "Find the catalog tiles/assets most visually similar to an image. Returns pixel-"
                       "exact matches first (score 1.0, exact=true) then nearest neighbours. Pass a file "
                       "path or base64 PNG.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "path to an image file"},
                "image_base64": {"type": "string", "description": "base64-encoded image bytes (PNG)"},
                "kind": {"type": "string", "enum": ["tile", "image"], "description": "restrict to sliced tiles or whole images"},
                "tileset": {"type": "string", "description": "restrict to one tileset file"},
                "limit": {"type": "integer", "default": 10},
            },
        },
    },
    {
        "name": "match_tilemap",
        "description": "Slice an image into a tile_size grid and identify each cell against the tileset "
                       "database. Returns a reconstructed gid map (per-cell file+gid, a 2D grid, a "
                       "dominant-tileset histogram, and exact/mean match scores) suitable for building a "
                       "matching map. Fully transparent cells become empty (gid 0).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "path to the image to reconstruct"},
                "image_base64": {"type": "string", "description": "base64-encoded image bytes (PNG)"},
                "tile_size": {"type": "integer", "default": 16},
                "tileset": {"type": "string", "description": "restrict matches to a single tileset file"},
                "grid_only": {"type": "boolean", "default": False, "description": "return only the 2D gid grid"},
            },
        },
    },
    {
        "name": "get_record",
        "description": "Fetch a full catalog record by its id (as returned by search_text, e.g. "
                       "'tileset:Backgrounds/Tilesets/Interior/Elements.png').",
        "inputSchema": {
            "type": "object",
            "properties": {"id": {"type": "string"}},
            "required": ["id"],
        },
    },
    {
        "name": "list_tilesets",
        "description": "List every tileset with its grid dimensions, tile size, theme, and whether it "
                       "has autotile terrain data. Useful for picking a tileset to build a map from.",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


def call_tool(name, args):
    e = engine()
    if name == "search_text":
        kind = args.get("kind")
        kind = kind.split(",") if kind else None
        return e.search_text(args["query"], kind=kind, limit=int(args.get("limit", 10)))
    if name == "search_image":
        return e.search_image(_image_arg(args), limit=int(args.get("limit", 10)),
                              kind=args.get("kind"), tileset=args.get("tileset"))
    if name == "match_tilemap":
        res = e.match_tilemap(_image_arg(args), tile_size=int(args.get("tile_size", 16)),
                              tileset=args.get("tileset"))
        if args.get("grid_only"):
            return [[c["gid"] for c in row] for row in res["grid"]]
        return res
    if name == "get_record":
        return e.get(args["id"])
    if name == "list_tilesets":
        return [{k: r.get(k) for k in ("id", "file", "name", "grid", "tile_size", "autotile", "tags")}
                for r in e.records if r["kind"] == "tileset"]
    raise ValueError(f"unknown tool: {name}")


# ---------------------------------------------------------------------------
# JSON-RPC / MCP loop
# ---------------------------------------------------------------------------
def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def handle(req):
    method = req.get("method")
    rid = req.get("id")
    is_notification = "id" not in req

    if method == "initialize":
        client_ver = (req.get("params") or {}).get("protocolVersion") or PROTOCOL_VERSION
        return {"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": client_ver,
            "capabilities": {"tools": {}},
            "serverInfo": SERVER_INFO,
        }}
    if method in ("notifications/initialized", "initialized"):
        return None
    if method == "ping":
        return {"jsonrpc": "2.0", "id": rid, "result": {}}
    if method == "tools/list":
        return {"jsonrpc": "2.0", "id": rid, "result": {"tools": TOOLS}}
    if method == "tools/call":
        params = req.get("params") or {}
        name = params.get("name")
        args = params.get("arguments") or {}
        try:
            result = call_tool(name, args)
            text = json.dumps(result, ensure_ascii=False, indent=1)
            return {"jsonrpc": "2.0", "id": rid, "result": {
                "content": [{"type": "text", "text": text}],
                "isError": False,
            }}
        except Exception as ex:  # noqa: BLE001 - surface tool errors to the client
            log("tool error:", name, repr(ex))
            log(traceback.format_exc())
            return {"jsonrpc": "2.0", "id": rid, "result": {
                "content": [{"type": "text", "text": f"Error in {name}: {ex}"}],
                "isError": True,
            }}

    if is_notification:
        return None
    return {"jsonrpc": "2.0", "id": rid,
            "error": {"code": -32601, "message": f"Method not found: {method}"}}


def main():
    log("ready; waiting for MCP client on stdio")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as ex:
            log("bad json:", ex)
            continue
        try:
            resp = handle(req)
        except Exception as ex:  # noqa: BLE001
            log("handler error:", repr(ex))
            resp = {"jsonrpc": "2.0", "id": req.get("id"),
                    "error": {"code": -32603, "message": str(ex)}}
        if resp is not None:
            send(resp)
    log("stdin closed; exiting")


if __name__ == "__main__":
    main()
