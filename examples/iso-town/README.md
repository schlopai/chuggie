# iso-town — isometric plaza with talk + shop

<img src="preview.gif" alt="preview" width="480">

A walkable outdoor town on the same **Tiled isometric bake** as
[`iso-tactics`](../iso-tactics): free d-pad movement, **camera follow** (akari-style), talk to NPCs
with A, and a merchant that opens a buy/sell shop (`packages/dialog` + `packages/shop`).

## Controls

| Input | Action |
|-------|--------|
| D-pad | Walk (isometric diagonals); camera follows |
| A | Talk to adjacent NPC / confirm in dialog & shop |
| B | Back out of shop menus |

## Build / run

```bash
cd examples/iso-town
unset CARGO_TARGET_DIR && npm run build   # -> iso-town.gba
npm start                                 # build + mgba
npm run shot                              # headless screenshot
```

## Layout

- [`tiled/town.tmj`](tiled/town.tmj) — 16×16 plaza (paths, pond, raised props, spawns)
- [`src/main.tish`](src/main.tish) — bake, depth draw, walk, camera, interact loop
- [`src/npcs.tish`](src/npcs.tish) — dialogue lines + merchant flag
- Art reused from iso-tactics (tiles + actors) and shop-demo (`shop32.png`)

Large boards bake to a **512×512** floor atlas and scroll via `camera_set` + `bg_scroll`; classic
≤8×8 tactics maps still use the fixed 256×256 canvas.
