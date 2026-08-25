# Backgrounds and parallax on the GBA

Everything about the four background layers: what they cost, how `frame()` hands them out, the three
kinds a game can build, and how to get more apparent depth than you have layers.

The last part — **per-scanline banding** — is the niche one. It is easy to lose because nothing in a
game's source hints that it exists, so start there if that is what you came for: [More depths than
layers](#more-depths-than-layers).

## The budget: four, and they are all spoken for

The GBA has exactly four regular background layers. agb **panics** on the fifth `show()` ("Can only
have 4 backgrounds at once"), which with `panic=abort` is a dead ROM. The layer count here is
data-driven — a Tiled map's layer count, plus any `bg_new`, plus the UI canvas — so an artist adding
a layer to a map could otherwise crash the game with no diagnostic.

So `frame()` **budgets** them instead of trusting the count (`crates/tish-agb/src/lib.rs`, the
`MAX_BG` block):

1. **The UI canvas is reserved first**, whenever `uiInit` has run. A menu you cannot see is worse
   than a missing parallax layer.
2. Then everything else fills what is left, **front priority first** — so a map that overruns the
   budget loses its backmost layer, not the one the player is looking at.

A typical game with menus therefore has **three**: the world, and two backdrops.

```
P0  UI canvas (dialog, shop)      ← reserved
P2  world
P3  backdrop
P3  backdrop
```

### Priority is not a style choice

**World sprites draw at priority 2, and an OBJ beats a BG of equal priority.** That tie is what puts
characters in front of the map. So:

- the world layer must be **P2** — at P1 it draws *over* the player, and the symptom is not "layering
  looks wrong", it is **the player being invisible while the camera follows them perfectly**;
- everything behind the world therefore shares **P3**, and that tie is broken by **creation order** —
  earlier is in front. Build backdrops near-to-far.

In a `.tmj`, set an int `priority` custom property per layer. The default is Tiled order
back-to-front (3, 2, 1, …), which is wrong for any map with backdrops, because it puts the world at
1. Two layers at the same priority are ordered by Tiled's own stacking (higher in the editor = in
front), because the baker emits layers front-first for exactly that reason.

## Three kinds of background

| | built by | wraps? | all tiles resident? | scroll |
|---|---|---|---|---|
| **full-screen image** | `bg_new(handle, priority)` | yes, every 256px | yes | `bg_scroll` / `bg_parallax` / bands |
| **tile grid** | `tilemap_new(tileset, cols, w, h, gids, priority)` | yes, every 256px | yes | same |
| **streamed map layer** | a `scene:` world layer | no | **no — a 256×256 window** | follows the camera |

The distinction that matters is the last column. A streamed layer (`InfiniteScrolledMap`) only keeps
the tiles around the camera in VRAM, which is what lets a 240×80 overworld exist at all — it is 38KB
as a `Vec<i16>`, enough to OOM EWRAM on a reload after fragmentation. The price is that **a streamed
layer cannot be scrolled far from the camera**: past ~16px of slack you are showing tiles that were
never loaded.

That single fact decides most of what follows.

### A tilemap cell is 16px — except through `tilemap_set8`

Every tilemap call takes cells, not tiles: `tilemap_new`'s `gids` and `tilemap_set(handle, tileset,
cols, col, row, gid)` both address a **16×16px cell**, laid down as the 2×2 block of hardware tiles at
`(2*col, 2*row)`, and `cols` is the tileset's width in 16px cells. That is the right unit for a world
map, and it is why `tilemap_new` also takes the tileset width — it has to find the four corners of
each block.

It is also a ceiling. The background is `Background32x32` — 32×32 hardware tiles, so **16×16 cells**,
of which about 15×10 are on screen. A board that wants more cells than that has to drop to tiles:

```
tilemap_set8(handle, tileset, col, row, tile)     // ONE 8x8 tile; `tile` 1-based, 0 blanks
```

`examples/blockfall` is why it exists: a classic falling-block well is 10 columns by 20 rows, which is
320px tall at 16px cells and does not fit the screen, let alone the map. At 8px it is 80×160 and
leaves 144px for the HUD.

⚠️ **It takes no `cols`.** `include_background_gfx!` bakes tiles row-major over the source image's own
8×8 grid, so a linear index already names a tile and a width parameter would be one that lies — which
matters more than it sounds, because tish does not check call arity: a caller passing an extra
argument in that slot would compile, and the tile index would silently be the width.

Build the map the usual way and paint it afterwards: `tilemap_new(tiles, cols, 0, 0, [], prio)` skips
the paint loop with `w = h = 0` but still uploads the palettes and sets the backdrop, which is the
part you need. ⚠️ It rewrites all 16 background palettes and re-derives the backdrop colour, so call
`backdrop()` **after** it, not before.

### Scene backdrops are wrapping backgrounds, not streamed

A `.tmj` tile layer whose Tiled parallax factor is **not 1.0** is a *backdrop*, and the scene loader
builds it as a plain hardware-wrapping background (`push_scene_backdrop`) rather than streaming it.
It is filled from the layer's **top-left 16×16 cells** — exactly 256×256px, exactly the GBA's wrap —
so those cells tile the screen forever in both axes.

Two consequences worth knowing:

- **Anything drawn beyond the first 16×16 cells of a parallax layer is not shown.** There is nowhere
  for it to go. Paint the backdrop as a tile that repeats.
- Backdrops get a `visible` flag, and hiding one really does hand its slot back
  (`sceneBgVisible(i, on)`). An interior with no sky can afford a layer the outdoors could not.

They are pooled and reused across scene loads, like stream layers, because reallocating a 2KB tile
box on every warp is what fragments EWRAM into "allocation of N bytes failed" on the next warp.

## One palette set, for all of them

`bg_new`, `tilemap_new` and `tilemap_stream` **each call `set_background_palettes`, which replaces
all 16 background palettes.** Two different `background:` images on screen at once therefore fight,
and the loser renders in the winner's colours.

So: **bake every background a scene shows at once into ONE image.** For a Tiled game that falls out
for free — one `.tmj` is one atlas is one palette set, which is the reason a sky belongs in the map
as a layer rather than as a separate `background:` import.

## Whole-layer parallax

`bg_parallax(handle, mulX, mulY)` scrolls a background at a fraction of the camera, in 1/256ths: 256
tracks the camera exactly, 128 is half speed, 0 pins it to the screen, negative drifts it the other
way. In a `.tmj` this is just Tiled's own per-layer `parallaxx` / `parallaxy` (1.0 = locked), baked
to the same scale.

It is applied natively inside `frame()`, immediately before `show`, from the camera the engine wrote
microseconds earlier in the same step — so a backdrop can never trail the world by a frame, and it
costs two multiplies instead of a boxed `value_call` per layer per frame.

`bg_scroll` takes a layer *off* automatic parallax; `bg_parallax` puts it back.

## More depths than layers

Two backdrops is a hard ceiling on separately-scrolling layers. It is not a hard ceiling on
**depths**.

A background's horizontal scroll register can be rewritten *between scanlines*, by DMA, while the
screen is drawing. So one layer can scroll at one rate across the stars, another across the
mountains and a third across the treeline:

```tish
bg_bands(bg, [0, 12, 52, 72, 104, 240])
```

A flat `[firstRow, mulX, …]`: rows 0..159 top to bottom, `mulX` in the same 1/256ths as
`bg_parallax`. Each band runs to the next one's first row; the first also covers anything above it.
`bg_bands(bg, [])` turns it off.

- `bg_bands(handle, bands)` — for a `bg_new` / `tilemap_new` background.
- `sceneBands(i, bands)` (`packages/engine`) — for a `scene:` backdrop, `i` counting the `.tmj`'s
  parallax layers in emit order (0 = frontmost). Call it **after** `loadSceneRom`; a scene load
  rebuilds the backdrops and clears their bands.

The engine expands the bands to a 160-entry table each frame and hands it to an `HBlankDma` on the
layer's scroll register. Cost: one 320-byte table per frame, no extra VRAM, no extra layer.

**Worked example: [`examples/bands-demo`](../examples/bands-demo/).** Three depths from one
background, in about forty lines of pure `cargo:tish_agb`.

### Bands as a ground plane

Bands also buy the effect people reach for affine to get. There is no affine/Mode-7 background API
in tish-agb — one existed and was removed in `7f6f969` — so a receding floor is built out of bands
instead, and the result is cheaper: no second layer, no transform per scanline, no 256-colour
source.

The trick is that a band is a **depth**, so everything that varies with depth has to vary together.
For a band at depth *d*, a perspective projection makes both the on-screen tile width and the
on-screen scroll speed proportional to 1/*d*. Draw the floor with a wider tile per band going down
the screen, give each band a proportionally faster `mulX`, and let the horizontal joints between
bands crowd together toward the horizon. Get the widths without the rates and it is a perspective
painting sliding sideways as one sheet; get the rates without the widths and it is a flat texture
shearing. Together it reads as ground going past.

One constraint that is easy to miss: **the tile widths must divide 256**, because that is the
hardware wrap period. A band drawn with 24px tiles leaves a seam that walks across the screen once
per lap. 8/16/32/64 are the usable sizes.

See **[`examples/rap-dojo`](../examples/rap-dojo/)**, whose generator draws the image and emits the
band table from the same numbers — the picture and the scroll rates are one design, and splitting
them across two hand-maintained files is how a floor ends up shearing.

### The two hard limits

**One banded layer per game.** agb's `GraphicsFrame` holds a single `next_dma` slot, and
`HBlankDma::commit` hardcodes DMA channel 0 — a second banded layer would silently replace the
first. The engine gives it to the first layer that asks and lets the rest scroll normally.

**Wrapping backgrounds only.** A `background:` image or a scene backdrop, both of which repeat every
256px in hardware. **Not the streamed world layer** — band offsets run to hundreds of pixels and it
only has a 256×256 window resident, so there is nothing there to show.

A whole-layer `bg_parallax` on the same layer does *not* conflict; the DMA overrides it per scanline.
(Tested, because it looked like it should.)

### It wants art drawn as strata

Banding gives different scroll rates to horizontal *bands of scanlines*. It cannot separate two
things that overlap vertically, and splitting one shape across a boundary shears it.

This is why `examples/oakhollow` does **not** use it despite being the game that motivated it: its
sky is a flat horizontal gradient (a horizontal scroll on it is invisible) and its treeline is a
single silhouette. Banding wants a backdrop drawn *as* stacked strata — which is what bands-demo's
art is for.

## Verifying a scroll rate

Cross-correlating two captures N frames apart and dividing by N is the right method, and it will lie
to you in at least three ways. All three produced a confident wrong answer during development:

1. **A uniform region matches at every offset.** A silhouette's flat interior gives residual 0.00 at
   whatever shift the *surrounding* content has. Correlate a structured edge.
2. **Other layers contaminate the window.** Measuring "the backdrop" in a region that also holds
   world tiles measures the world. **Isolate the layer by a colour unique to it** and correlate a
   per-column count of only those pixels.
3. **Repeating art aliases.** bands-demo's conifers repeat every 16px, so a true 56px shift reads as
   8. Give the art a period longer than the expected shift, or measure a band that does not repeat.

Two more: check the rows you are measuring actually *contain* the layer's art (a probe split at row
55 was useless because the silhouette spanned 55..87, entirely on one side of the line), and always
report the residual — a large one means the answer is noise, not a measurement.

`scripts/screenshot.sh rom.gba out.png <frames> "<key schedule>"` drives all of this headlessly; see
`tools/gba-shot.c` for the schedule syntax.

## Where the code is

| | |
|---|---|
| budget, `frame()` order, band DMA | `crates/tish-agb/src/lib.rs` (`MAX_BG`, `attach_band_dma`) |
| scene backdrops, stream layers | same file (`push_scene_backdrop`, `push_stream_layer`) |
| layer priority / parallax baking | `crates/tish-gba-scenepack/src/tiled.rs` |
| tish API | `packages/engine.tish` (`sceneBands`, `sceneBgVisible`), `packages/parallax.tish` |
| worked example | `examples/bands-demo` |
