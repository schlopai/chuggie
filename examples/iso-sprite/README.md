# ISO SPRITE

> *An isometric SRPG subsystem demo showcasing sprite.*

<img src="preview.png" alt="preview" width="480">

Isometric rendering the honest way on the GBA, which has **no isometric hardware mode**: every tile
is an isometric-cube **sprite** placed by an iso projection and drawn back-to-front (painter's
algorithm). A moving player token correctly slips in front of nearer tiles and behind farther ones.

## How it works

- **Projection.** The cubes' top face is a 16×8 diamond, so one grid step is `(±8, +4)` px:
  `screenX = OX + (col−row)·8`, `screenY = OY + (col+row)·4`.
- **Depth.** Each tile is tagged with `sprite_set_depth(handle, col+row)`. tish-agb draws world
  sprites ordered by depth (higher = nearer = drawn in front), so cube bodies and the player overlap
  correctly. This is the general y-sort hook — it also does top-down y-sorting (`depth = y`).
- **Player.** Tracked in quarter-tile units (integers, no floats); the d-pad glides it along the
  four diamond edges.

## Controls

D-pad moves the red player token along the isometric grid.

## Art

The vendored CC0 **Devil's Work.shop** isometric block pack (`assets/iso-blocks/`). The chosen cubes
are sliced from the pack atlas into a 16×16-frame sheet by `tools/pack_blocks.py` (each cube is
quantized to ≤15 colours to fit a GBA sprite palette). Edit `PICKS` there and re-run to change tiles.

## Build

```bash
npm run build      # build the ROM
npm start          # build + open in mGBA
```

For a fuller isometric build (an SRPG prototype with a build-time-baked iso board), see
the isoboard SRPG example in the chuggie-tactics repo.
