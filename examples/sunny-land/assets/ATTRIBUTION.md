# Art attribution

All pixel art in this folder is derived from the **"Sunny Land"** asset pack by **Luis Zuno
(Ansimuz)**, released into the public domain (CC0). You may use, modify, and redistribute it freely;
credit is appreciated but not required. The original license is vendored alongside as
[`LICENSE-sunny-land.pdf`](LICENSE-sunny-land.pdf).

- Source: Sunny Land by Ansimuz — https://ansimuz.itch.io/sunny-land-pixel-game-art
- `scripts/gen_sunnyland.py` (repo root) copies the specific tiles/frames this game uses out of the
  pack, crops/scales them into GBA-ready sheets, and writes the level. It is the source of truth for
  everything here except this file and the license.

What each file is:

| file | from | notes |
|------|------|-------|
| `tileset.png` | environment tileset | curated 8-tile 16px strip, flattened onto an opaque sky |
| `player.png` | Foxy character | 32×32 sheet: idle ×4, run ×6, jump-up, fall, hurt ×2 |
| `opossum.png` | Opossum character | 32×32 walk cycle (patrol enemy) |
| `cherry.png` / `gem.png` | items | 16×16 spins — coin / double-jump powerup |
| `fx-poof.png` / `fx-spark.png` | FX | 32×32 enemy-death puff / item sparkle |
| `heart.png` | — | synthetic HUD heart (empty / half / full), generated in the script |
