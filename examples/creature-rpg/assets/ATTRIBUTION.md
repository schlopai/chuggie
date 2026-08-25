# Assets

All art is from the **Ninja Adventure Asset Pack** by *Pixel-Boy* and *AAA*, released **CC0**
(public domain). It is vendored in this repository at `assets/ninja-adventure/` — see
`assets/ninja-adventure/LICENSE` and the pixel-verified index at
`assets/ninja-adventure/catalog/`.

> https://pixel-boy.itch.io/ninja-adventure-asset-pack

Nothing here is hand-placed. Every file below is emitted by a script, so the pack stays the source:

| file | made by | from |
|---|---|---|
| `hero.png` | `scripts/gen_creature_rpg.py` | `Actor/Character/Boy/SeparateAnim/{Idle,Walk}.png` |
| `prof.png` | ″ | `Actor/Character/OldMan/SeparateAnim/` |
| `mom.png` | ″ | `Actor/Character/Woman/SeparateAnim/` |
| `rival.png` | ″ | `Actor/Character/Villager2/SeparateAnim/` |
| `guard.png` | ″ | `Actor/Character/Knight/SeparateAnim/` |
| `town.tmj` | ″ | `TilesetFloor` · `TilesetField` · `TilesetFloorDetail` · `TilesetNature` · `TilesetHouse` · `TilesetWater` |
| `lab.tmj` `home.tmj` | ″ | `TilesetInteriorFloor` · `TilesetWallSimple` · `tileset_bed` |
| `theme.deck` | `scripts/gen_creature_music.py` | original — an eight-bar loop for the four PSG voices |

The maps reference the shared Tiled tileset library at `assets/ninja-adventure/tiled/*.tsj`, which
carries the wangsets the generator paints with and the per-tile collision the engine reads.

Re-bake both with:

```bash
npm run assets
```
