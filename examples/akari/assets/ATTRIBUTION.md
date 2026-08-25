# Art attribution

All sprites and tiles in AKARI are sliced/derived from the **Ninja Adventure — Asset Pack** by
**Pixel-boy & AAA** (https://pixel-boy.itch.io/ninja-adventure-asset-pack), released under **CC0 1.0
(public domain)**. No rights reserved by the original authors; no attribution is legally required, but
it is given here with thanks.

The pack is vendored in this repo at [`assets/ninja-adventure/`](../../../assets/ninja-adventure/).
These example sheets are generated from it by [`scripts/gen_akari.py`](../../../scripts/gen_akari.py)
(sprites) and [`scripts/gen_akari_maps.py`](../../../scripts/gen_akari_maps.py) (the town + shrine
Tiled maps). Re-run those from the repo root to regenerate everything here.

| File | Source in the pack |
|---|---|
| `hero.png` | `Separate/{Idle,Walk,Attack,Push}` ⊕ `Items/Weapons/Katana/Sprite` ⊕ `Weapon/Katana` slash pieces via indexed `KATANA_ATTACK` table (`attack-index/`); Push frames become the throw pose |
| `slime.png` | `Actor/Monster/Slime` |
| `bat.png` | `Actor/Monster/YellowsBat` |
| `skeleton.png` | `Actor/Character/Skeleton` |
| `boss.png` | `Actor/Boss/DemonCyclop` (downscaled to 16×16) |
| `elder.png` | `Actor/Character/OldMan` |
| `woman.png` | `Actor/Character/Woman` |
| `merchant.png` | `Actor/Character/Noble` |
| `sensei.png` | `Actor/Character/Master` |
| `hearts.png` | `Ui/Receptacle/Heart` |
| `items.png` | `Items/Potion`, `Items/Treasure`, `Items/Resource` |
| `town.tmj` / `shrine.tmj` | `Backgrounds/Tilesets/{TilesetFloor,TilesetNature,TilesetHouse,TilesetWater,Interior/*,TilesetDungeon}` |
| `audio/*.wav` | Pack `Audio/` → GBA mixer (10512 Hz mono); see [`audio/SOURCE.md`](audio/SOURCE.md) |
| `faces32.png` | Character `Faceset.png` portraits + choice cursor + `Ui/Input/Gamepad` A/B/L/R/D-pad/Start/Select prompts + Theme Wood `button_{normal,hover,pressed}` |

The title font (Alagard) is bundled by `packages/title.tish`; see its own license note.
