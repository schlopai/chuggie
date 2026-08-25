# Ninja Adventure — asset index

The complete [Ninja Adventure pack](https://pixel-boy.itch.io/ninja-adventure-asset-pack) (CC0,
Pixel-boy & AAA), vendored and cataloged so you can find tiles and sprites for building maps and
games by reading text, never by re-opening PNGs. **1,916 PNGs**, fully indexed.

> New here? [`README.md`](README.md) covers the vendored layout + license. This file is the index.

## Catalogs

| Catalog | Count | Depth | What it gives you |
|---|---:|---|---|
| [`tilesets`](catalog/tilesets.json) · [md](catalog/tilesets.md) | 23 | **vision (deep), pixel-verified 100%** | per-tileset theme, tile grid, autotile regions, doors/stairs/water-cliff edges, and how to use each for a map. Every one of 3,975 occupied tile cells across all 23 sheets is accounted for — checked cell-by-cell, not eyeballed (`scripts/tileset_coverage.py`) |
| [`backgrounds_extra`](catalog/backgrounds_extra.json) · [md](catalog/backgrounds_extra.md) | 28 | structural | animated tiles (water ripples, waterfalls, flags, mills, quicksand, conveyor) + vehicles (boat, crane, sail, fishnet), with frame counts |
| [`actors`](catalog/actors.json) · [md](catalog/actors.md) | 206 | structural | every Character/Monster/Boss/Animal: sheet dims + frame layout, faceset, SeparateAnim clips |
| [`items`](catalog/items.json) · [md](catalog/items.md) | 140 | structural | weapons (by type), food, potions, tools, treasure, projectiles, scrolls |
| [`fx`](catalog/fx.json) · [md](catalog/fx.md) | 75 | structural | attack slashes, elemental spells, magic auras, projectiles, smoke |
| [`ui`](catalog/ui.json) · [md](catalog/ui.md) | 359 | structural | dialog frames, emotes, font, input prompts, skill icons, themes |
| [`index.json`](catalog/index.json) | 803 | — | **flat searchable manifest** of every asset (`grep` it: file, group, name, dims, type) |

## Authoring maps

Two ways to build a map from these assets, both packed into ROM at compile time by
[`tish-gba-scenepack`](../../crates/tish-gba-scenepack)'s `scene:` import:

- **Tiled** (visual, hand-drawn) — draw a self-contained `.tmj` in the example's `assets/`,
  referencing the shared tileset library in [`tiled/`](tiled/README.md) and baking collision into a
  `Collision` layer; see `examples/ninja-village/assets/village_square.tmj` and
  `examples/ninja-adventure/assets/village.tmj`.
- **Recipe** (compact, autotiled ground + prop-stamps) — a small JSON baked into a `.tmj` with
  `scripts/recipe_to_tiled.py`; see the format there.

## Building a map — autotiling

Terrains use Godot "match corners and sides" (**47-blob**). Verified, ready-to-use materials live
in [`catalog/autotile.json`](catalog/autotile.json); paint a terrain grid with the converter:

```python
from ninja_autotile import Autotiler                       # scripts/ninja_autotile.py
at = Autotiler("assets/ninja-adventure/catalog/autotile.json")
gids = at.terrain_to_gids(grid, w, h, "TilesetFloor.png", "snow")   # 47/47 masks, verified
```

**TilesetFloor** ships 8 ground materials — `dirt_grass`, `dirt_grass_dark`, `tan_sand`,
`pink_sand`, `snow`, `dark_mud`, `ice_blue`, `orange_clay` — each full 47/47. Other verified
wangsets in the same file: WallSimple×4, InteriorFloor (floors/walls/carpets), Hole, Water×4,
FloorB cloud(+island), Field×5, bed `stone_slab`. Modular kits (Pipes, Desert walls, Relief cliffs)
are **not** wangsets — hand-place; see `autotile.json` → `coverage` and `tilesets.json` notes.

Regenerate derived masks with `scripts/gen_autotile_masks.py`, then refresh Tiled terrains via
`scripts/gen_tileset_library.py` (`tiled/*.tsj` wangsets).

Doors/warps: `tilesets.json` flags **58 door tiles** across House, VillageAbandoned, and Camp
(each with its gid) — use these to place building entrances.

## Sprite frame convention

Row = direction/action, column = frame, 16px cells:

- **64×112** standard character — rows 0-3 walk **Down/Up/Left/Right** (×4 frames), row 4 idle, row 5 attack, row 6 jump
- **64×64** simple/monster — walk 4-direction only
- **SeparateAnim/** — `Walk` 64×64 (dir×frame); `Idle`/`Attack`/`Jump` 64×16 (4 frames); `Dead`/`Item`/`Special` 16×16
- **Faceset.png** 38×38 — dialogue portrait

Bosses (Dragon, Giant*, Squid, Tengu…) are multi-part / per-animation sheets — see their
`extra_pngs` in `actors.json`.

## Regenerate

```bash
python3 scripts/index_actors.py                 # actors.json + .md
python3 scripts/index_group.py Items|FX|Ui       # <group>.json + .md
python3 scripts/tileset_coverage.py --apply      # pixel-audit tilesets.json, write coverage
python3 scripts/gen_tilesets_md.py               # sync tilesets.md (with coverage badges) from tilesets.json
python3 scripts/ninja_autotile.py --demo TilesetFloor.png /tmp/out.png snow   # autotile self-test
```
