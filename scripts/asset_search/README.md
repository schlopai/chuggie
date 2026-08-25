# Asset search — text + image search over the Ninja Adventure pack

A dependency-light (Pillow + numpy only) searchable index over the vendored asset
pack and its hand-curated catalog, exposed to AI agents as MCP tools. It answers:

- **text** → "cozy wooden interior floor" → ranked catalog entries (BM25 over
  names / tags / themes / palettes / per-region descriptions).
- **image** → a tile or sprite → the catalog assets it matches, **pixel-exact
  first** (score 1.0) then nearest-neighbour.
- **tilemap** → a whole image → sliced into a grid, each cell identified against
  the tileset database → a reconstructed **gid map** you can build a scene from.

The flagship flow: *hand an AI a reference map, it slices it, matches each slice to
a tile, and emits a gid grid + which tileset(s) to build from.*

## Build the index

```bash
cd scripts
python3 -m asset_search.cli build
```

Writes to `assets/ninja-adventure/catalog/search/` (git-ignored, ~2 MB, ~1 s):
`records.jsonl` (text), `fingerprints.npz` + `fp_meta.jsonl` (image), `manifest.json`.
**Rebuild whenever `catalog/*.json` or the source PNGs change.**

## CLI

```bash
python3 -m asset_search.cli text "red wooden door" --kind tile,region --limit 5
python3 -m asset_search.cli image path/to/tile.png --kind tile --limit 5
python3 -m asset_search.cli tilemap path/to/reference.png --tile-size 16
python3 -m asset_search.cli tilemap path/to/reference.png --grid-only        # just the 2D gids
python3 -m asset_search.cli tilemap path/to/reference.png --tileset Backgrounds/Tilesets/TilesetField.png
python3 -m asset_search.cli get "tileset:Backgrounds/Tilesets/Interior/Elements.png"
```

## MCP tools (for AI agents)

Registered for Claude Code in the repo-root `.mcp.json` (`asset-search` server).
Restart Claude Code to pick it up; approve the server when prompted. Tools:

| tool | args | returns |
|------|------|---------|
| `search_text` | `query`, `kind?`, `limit?` | ranked records |
| `search_image` | `path` \| `image_base64`, `kind?`, `tileset?`, `limit?` | matches (exact first) |
| `match_tilemap` | `path` \| `image_base64`, `tile_size?`, `tileset?`, `grid_only?` | per-cell gid map + histogram + scores |
| `get_record` | `id` | full record |
| `list_tilesets` | — | tileset summaries (grid, theme, autotile) |

Register elsewhere with:

```bash
claude mcp add asset-search -- python3 /abs/path/to/scripts/asset_search/mcp_server.py
```

## How matching works

Each 16×16 tile (tilesets are sliced on their catalog grid) and each whole asset PNG
gets a fingerprint: a canonical 16×16 RGBA thumbnail (transparent pixels flattened to
0,0,0,0), a blake2b content hash for **O(1) exact hits**, a 64-bit dHash, and mean
colour. Queries fingerprint the same way; exact-hash hits win, otherwise ranking is L2
distance over the thumbnails. For a map built from these tiles, every slice is a
pixel-exact hit (`exact: true`, `score: 1.0`). `gid` is the tile's 1-based index within
its tileset (`row*cols + col + 1`), matching the catalog's `notable_tiles.gid`.

Layout: `common.py` (paths, tokenizer, fingerprints) · `build.py` (indexer) ·
`search.py` (`AssetSearch` query API) · `cli.py` · `mcp_server.py` (zero-dep stdio MCP).
