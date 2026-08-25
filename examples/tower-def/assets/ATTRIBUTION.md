# Assets

All art is from the **Ninja Adventure Asset Pack** by *Pixel-Boy* and *AAA*, released **CC0**
(public domain), vendored at `assets/ninja-adventure/`.

> https://pixel-boy.itch.io/ninja-adventure-asset-pack

Everything is emitted by `scripts/gen_towerdef.py`, so the pack stays the source:

| file | from |
|---|---|
| `td_tiles.png` / `.tsj` | `TilesetFloor` (grass, path) and `TilesetNature` (rock) — a LOCAL tileset, not the shared library |
| `track.tmj` | generated from the `TRACK` waypoint list; walls on a `Solid` layer |
| `units.png` | `Actor/Monster/{Slime,BlueBat}` row 0, and `Actor/Character/{Knight,SorcererBlack}/SeparateAnim/Idle.png` column 2 |

The build cursor in `units.png` is drawn, not cropped — nothing in the pack is a selection bracket.

One sheet for creeps, towers and cursor together: one sheet is one of the GBA's sixteen sprite
palette banks.

The font is `assets/fonts/tinypixel.ttf`; see its licence beside it.
