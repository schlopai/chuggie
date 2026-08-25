# Shared Tiled tileset library

This folder is the **shared tileset library** for authoring GBA maps in
[Tiled](https://www.mapeditor.org/). It holds only `.tsj` tilesets — **no maps**. Each tileset
references a vendored PNG under `../Backgrounds/Tilesets/` (no image copies), so any map in any
example can paint with them without duplicating art.

**Maps live with the example that uses them**, e.g.:
- `examples/ninja-village/assets/village_square.tmj` → references the `.tsj` here
- `examples/ninja-adventure/assets/{village,interior}.tmj` → reference the `.tsj` here, with
  collision baked into a `Collision` layer inside each map (no per-map `.tsj`, no image copies)

## Files here

- **A `.tsj` for every one of the 23 vendored tilesets** (Floor, House, Nature, Water, Desert,
  Dungeon, Relief, Towers, VillageAbandoned, the Interior set, Pipes, camp, bed, …) — each with
  `image = ../Backgrounds/Tilesets/…`, ready to paint with in any map.
- `Collision.tsj` / `Collision.png` — optional legacy marker for a `Collision` overlay layer.
  Prefer per-tile collision on the art tilesets (from `catalog/tile_collision.json`).

Regenerate the whole library (after adding/changing tilesets) with:

```bash
python3 scripts/gen_tileset_library.py
```

## How a Tiled map builds into a ROM

A tish game imports a `.tmj` via the `scene:` scheme (contributed by tish-agb):

```tish
import { village } from 'scene:../tiled/village_square.tmj'
loadSceneRom(village)
```

At `tish build`, `tish-gba-scenepack`'s `include_scene!` runs the **Tiled importer**
([`crates/tish-gba-scenepack/src/tiled.rs`](../../../crates/tish-gba-scenepack/src/tiled.rs)): it
resolves every tile to its source tileset via `firstgid`, packs the used tiles into one atlas (GIDs
remapped + agb-deduplicated), and emits the ROM tile data. Sibling `*.atlas.png` / `*.map.bin` are
regenerated each build (gitignored).

## Map conventions the importer expects

| Layer | Meaning |
|---|---|
| **Tile layers** (any names) | rendered back-to-front in Tiled order (first = behind, priority 3 → 0). A cell is **solid** if any placed tile has Tiled Collision Editor shapes and/or `walkable = false`. |
| **`Collision`** tile layer (optional) | legacy overlay — not rendered; any non-empty cell = solid. Prefer per-tile collision on the `.tsj` (see `catalog/tile_collision.json`). |
| **object layer** | spawns: object tile is `(x/16, y/16)`; its `kind` int property (or name `player`/`npc`/`heart`) sets the spawn kind. |

Only external `.tsj` / embedded tilesets are read — if Tiled saved a `.tsx`, re-save it As `.tsj`.

Per-tile collision is stamped into the shared `.tsj` files by `gen_tileset_library.py` from
[`catalog/tile_collision.json`](../catalog/tile_collision.json) (full-cell 16×16 rects, visible in
Tiled's tile Collision Editor).
## Generating a map from a recipe

The compact scene-recipe format (autotiled ground + prop-stamps, tiny file) still works. Bake one
into an editable Tiled map — `.tsj` land here, the `.tmj` next to the example:

```bash
python3 scripts/recipe_to_tiled.py <recipe.json> <out_name> <example_dir/tiled>
```

Use recipes for quick/procedural scenes, Tiled for hand-drawn detail — both feed the same
compile-time packer.
