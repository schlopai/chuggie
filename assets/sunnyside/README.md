# Sunnyside World (vendored)

Pixel-art farming/life-sim pack by Daniel Diggle, vendored for the `sunnyside`
example family.

- Source: https://danieldiggle.itch.io/sunnyside (ASSET_PACK_V2.1, downloaded 2026-08-18)
- License: itch.io "V1" terms — free for commercial and non-commercial use,
  modification allowed, credit appreciated but not required, resale/NFT/AI
  training prohibited.  See LICENSE.txt.

## Layout

- `raw/` — the pack as shipped (minus GameMaker project, GIF previews and
  Aseprite sources): `Tileset/` (1024x1024 16px master sheet + 32px forest),
  `Characters/{Human,Goblin,Skeleton}` (per-action 96x64-frame strips; Human is
  layered base/hair/tools, right-facing only), `Elements/` (crops with 6 growth
  stages, animated deco strips), `UI/`.
- `gm/` — the pack's own GameMaker example-room descriptors (`Room1.yy`,
  `tileset_sunnysideworld.yy`).  Kept because the baker LEARNS from them: the
  room's `land`/`paths` layers are real artist-placed autotiling, and its
  `building`+`walls` layers are the source of the building stamps.
- `baked/` — GBA-ready sheets produced by `scripts/gen_sunnyside_pack.py`
  (committed, deterministic):
  - `char_player.png` — 64x32 frames, 10 actions (idle/walk/run/dig/water/axe/
    attack/carry/doing/hurt), base+spikeyhair+tools composited, feet on a fixed
    row; face left by hflipping the sprite.
  - `char_npc_{long,bowl,goblin}.png` — 32x32 idle+walk NPC sheets.
  - `world_tiles.png` + `.json` — a `bgtiles:` atlas: sea/grass/path fills
    with 20-entry "lip" transition tables (Sunnyside's own model: material
    cells are plain fill, the neighbouring cell carries a mostly-transparent
    fringe overlay; tables learned from the GameMaker room and composited
    opaque), soil + 11 crops x 6 stages x dry/wet farm tiles, four building
    stamps (shop/barn/house/house2) and three tree stamps.  Full cliff-wall
    coasts are out of scope — vertical island edges stay plain.
- `catalog/autotile.json` — learned mask->tileset-index tables (U=1 D=2 L=4 R=8,
  bit set = same terrain; off-map counts as same).

Frame tables and atlas GID tables are emitted as typed tish modules into
`examples/sunnyside/src/data_anim.tish` and `data_world.tish`; the other
`sunnyside-*` examples import them from there.

Regenerate everything with:

```
python3 scripts/gen_sunnyside_pack.py
```
