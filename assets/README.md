# Vendored assets

Art packs, fonts, and emoji used by examples and generators. Each pack has its own license — check the pack directory before reusing assets outside this repo.

| Pack | Path | License | Used by |
|------|------|---------|---------|
| **Ninja Adventure** | `ninja-adventure/` | CC0 (`LICENSE.txt`) | Top-down RPG examples, `fighter_art.py` |
| **Sunnyside** | `sunnyside/` | itch.io terms (`LICENSE.txt`) | Farming/life-sim examples |
| **Void fleet** | `void/` | See `SOURCE.md` | Space shmup examples |
| **Serenity emoji** | `emoji/serenity/` | See `LICENSE.txt` | `emoji:` import scheme |
| **Iso blocks** | `iso-blocks/` | See `License.txt` | Isometric examples |
| **Fonts** | `fonts/*.ttf` + `*.OFL.txt` | SIL OFL per font | `font:` import scheme |

## Canonical source

`ninja-adventure/` is the canonical art pack. `scripts/asset_search/` indexes it for MCP/CLI lookup.

## Adding assets

1. Place files under a named subdirectory
2. Include `LICENSE.txt` or `LICENSE.md` with terms
3. Add a pack `README.md` describing layout and attribution
4. Reference from examples via `sheet:`, `background:`, or `scene:` imports

Do not commit assets without a license file.
