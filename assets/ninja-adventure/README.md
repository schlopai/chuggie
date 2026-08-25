# Ninja Adventure — vendored asset library

The **full** [Ninja Adventure asset pack](https://pixel-boy.itch.io/ninja-adventure-asset-pack)
by [Pixel-boy](https://pixel-boy.itch.io/) & AAA, vendored into the repo so maps and sprites
can be built without the external download. **License: CC0** (public domain — free for any use,
including commercial; attribution appreciated). See [LICENSE.txt](LICENSE.txt).

This is the complete pack (1,916 PNGs), **not** the small subset the Godot demo bundles. It is
the canonical asset source for every chuggie-engine example.

## Layout

| Dir | PNGs | Contents |
|-----|-----:|----------|
| [`Backgrounds/`](Backgrounds) | 51 | **Tilesets** (23 — the map-building set), **Animated** (23 — water, waterfalls, flags, mills, flowers, quicksand, conveyor), **Vehicles** (5 — boat, crane, sail, fishnet) |
| [`Actor/`](Actor) | 1,285 | **Character** (903), **Monster** (132), **Boss** (116), **Animal** (116), **CharacterAnimated** (18). Each actor: `SpriteSheet.png` (walk/attack, 16×16 frames) + `Faceset.png` portrait + `SeparateAnim/` clips |
| [`Items/`](Items) | 140 | Weapons, Food, Potion, Tool, Treasure, Scroll, Projectile, Object, Resource, Action |
| [`FX/`](FX) | 75 | Smoke, Attack, Magic, Projectile, Slash, Elemental, Environment, Particle (+ animation `Preview.gif`s) |
| [`Ui/`](Ui) | 359 | Input prompts, Skill icons, Emote, Dialog, Theme, Font |

Also included: `Ui/Font/NormalFont.ttf` (the pack's pixel font) and all 124 animation-preview
gifs (`IdlePreview`, `WalkPreview`, `HitPreview`, … — animated references for the sprite sheets).

**Audio** (96 MB raw in the source pack — `Audio/{Musics, Sounds, Jingles}`) is **not**
vendored in full. A GBA-ready subset (10512 Hz mono WAVs) lives with the Akari example at
[`examples/akari/assets/audio/`](../../examples/akari/assets/audio/) — see its `SOURCE.md`.
Convert more from the pack with `ffmpeg -ac 1 -ar 10512 -c:a pcm_s16le` when needed.

### Completeness (audited against the source pack)

Every game asset is vendored: **1,911 game PNGs** (all tilesets, sprites, facesets, items, FX, UI),
the **font**, and **124 animation gifs**. Excluded on purpose: 4 marketing PNGs (MusicCover, pack
previews) + 4 marketing gifs (Example 1–4) — not game art. Full raw audio stays out of this tree;
Akari vendors a converted GBA subset (above).

## Catalog / index — read this instead of the images

[`catalog/`](catalog) holds the machine-readable index so you can pick tiles and sprites by
reading text, never by re-opening PNGs:

- [`catalog/autotile.json`](catalog/autotile.json) — per-tileset mask tables → Tiled wangsets
  (Godot "match corners and sides" / 47-blob, plus 3×3 islands and wall/carpet frames).
  Regenerate derived materials with `scripts/gen_autotile_masks.py`; push into `tiled/*.tsj` with
  `scripts/gen_tileset_library.py`. Indexed `(col,row)` grids live under `catalog/tilemaps/`.
- [`catalog/blob_template.json`](catalog/blob_template.json) — the canonical 47-blob layout
  (relative tile position ↔ mask); makes any floor-type tileset autotile-ready from just a blob origin.
- [`catalog/tilesets.json`](catalog/tilesets.json) / [`.md`](catalog/tilesets.md) — all 23 tilesets:
  theme, grid, autotile regions, notable tiles (doors, stairs, water/cliff edges), map-building use.
  **Pixel-verified 100% complete** — every one of the 3,975 non-transparent tile cells across all
  23 sheets is accounted for by a documented region/structure/notable-tile, checked by
  `scripts/tileset_coverage.py` (not eyeballed — it scans the actual PNG alpha channel cell by
  cell and cross-references the catalog text). Each tileset's markdown section carries a
  coverage badge; regenerate with `tileset_coverage.py --apply` then `gen_tilesets_md.py`.
- [`catalog/actors.json`](catalog/actors.json) / [`.md`](catalog/actors.md) — all 206 actors
  (Character/Monster/Boss/Animal): sheet dims + frame layout, faceset, SeparateAnim clips. 100% png coverage.
- `catalog/items.json`, `catalog/fx.json`, `catalog/ui.json` — added in the remaining catalog phase.
- [`ASSETS.md`](ASSETS.md) — the human-facing top-level index (assembled from the above).

**Sprite frame convention** (row = direction/action, col = frame, 16px): `64x112` standard character
= rows 0-3 walk Down/Up/Left/Right (×4 frames), row 4 idle, row 5 attack, row 6 jump; `64x64`
simple/monster = walk 4-dir only. `scripts/index_actors.py` regenerates the actor index.

### Autotiling — `scripts/ninja_autotile.py`

The converter paints a terrain-id grid into a gid tile layer using the tables above:

```bash
# self-test: render a dirt patch on grass to eyeball edges/corners
python3 scripts/ninja_autotile.py --demo TilesetFloor.png /tmp/out.png
```

```python
from ninja_autotile import Autotiler
at = Autotiler("assets/ninja-adventure/catalog/autotile.json")
gids = at.terrain_to_gids(grid, w, h, "TilesetFloor.png", "dirt_grass", fill=1)
```

`TilesetFloor` has full 47/47 mask coverage (verified). The same catalog also covers Water/Hole
(full 47), Field/bed/interior islands, WallSimple + Interior brick/carpet frames, and FloorB
(partial cloud blob — narrower sheet). See `autotile.json` → `coverage` for the full list.

## Catalog phases

1. **Maps + tilesets** — ✅ done, pixel-verified 100% complete (23/23 tilesets, 3,975/3,975 cells).
2. **Characters** — ✅ done, structural (206/206 actors, 100% png coverage).
3. **The rest** — ✅ done, structural (items/fx/ui, 574 assets).

## Regenerate

```bash
python3 scripts/index_actors.py                        # actors.json + .md
python3 scripts/index_group.py Items|FX|Ui              # <group>.json + .md
python3 scripts/tileset_coverage.py --apply             # audit + write coverage into tilesets.json
python3 scripts/gen_tilesets_md.py                      # sync tilesets.md from tilesets.json
python3 scripts/ninja_autotile.py --demo TilesetFloor.png /tmp/out.png snow   # autotile self-test
```
