# transitions

![preview](preview.gif)

Every screen transition the GBA can do, run through the real scene lifecycle.

`packages/transition` owns the effects; `packages/scene` drives them. Two scenes trade places once
per effect, with the effect's name in the corner. LEFT/RIGHT pick one by hand, A crosses over now;
left alone it cycles through all eleven on its own.

```bash
npm run build && npm start
```

## Why this exists and `win-demo` does not cover it

`examples/win-demo` proved the window registers work. It could not prove a *transition* works,
because it never changed scenes — it drove a phase counter over one static screen. A transition has
to survive what actually happens at its midpoint: the old scene torn down and a new one built. Every
bug found while building this one lived exactly there, and none of them would have shown up in a
demo that never swaps.

## The effects

| effect | mechanism | cost |
|---|---|---|
| `TR_FADE` | BLDY decrease — dip to black | one register |
| `TR_WHITE` | BLDY increase — blow out to white | one register |
| `TR_IRIS` | circle closing on the screen centre | one register + **the HBlank DMA** |
| `TR_IRIS_AT` | circle closing on `trFocus(x, y)` — shuts on the player | one register + **the HBlank DMA** |
| `TR_BOX` | rectangle shrinking to the focus point — the iris without the DMA | one register |
| `TR_WIPE` | hard edge sweeping right to left | one register |
| `TR_CURTAIN` | two panels closing in from the sides | two registers |
| `TR_BARS` | full-width bands closing from top and bottom | two registers |
| `TR_MOSAIC` | pixelate to 16px blocks, then a short dip to black | one register |
| `TR_RAIN` | columns of curtain falling at staggered speeds | canvas cells |
| `TR_CHECKER` | cells filling in a scrambled but fixed order | canvas cells |

Nine of the eleven are pure hardware: nothing is redrawn, the display does the work as it scans out.
The two software ones paint the UI canvas.

## Using it

```tish
import { sceneSetTransition, sceneGotoFx } from '../../packages/scene'
import { TR_IRIS, TR_BOX } from '../../packages/transition'

sceneSetTransition(TR_IRIS, 24)   // every crossing from here on
sceneGotoFx(cave, TR_BOX)         // …except this one
```

Or drive it yourself, without the scene machine — `trApply(p, len)` paints the effect at progress
`p` (0 = visible, `len` = hidden), and `trOut`/`trIn` are the blocking forms a cutscene wants.

The default is unchanged: a game that never mentions transitions still gets the 16-frame fade to
black it always had.

## The traps, all of which this example hit

**One BLDCNT for the whole screen.** `fade`, `fade_white`, `fx_flash` and `blend_alpha` share a
single two-bit effect field, and agb resets it on every call — so without arbitration the last
caller in a frame silently erased the others. The engine now resolves them in a fixed priority
(fade > white > flash > alpha) instead of letting call order decide. A hit-spark cannot interrupt a
scene change.

**One HBlank DMA slot.** `TR_IRIS` rewrites WIN0's horizontal extent per scanline, and so does a
Mode 7 floor and a `bg_bands` parallax layer. The circle used to claim the slot last and
unconditionally, dropping whatever had claimed it with no diagnostic; it now yields and logs. Over
those scenes, use `TR_BOX` — visually near-identical, no DMA.

**A slow `enter()` is felt as a transition that drags.** The first version of this demo rebuilt its
96-disc board inside `enter()`. That is ~24,000 per-pixel writes, and it took **87 frames** — so the
machine sat at full black for a second and a half every crossing and every effect looked like it
hung at its midpoint. The transition was fine. `scene.tish` swaps at full black precisely so that
`enter()` is invisible, but invisible is not free. The board is now built once at boot; scenes only
recolour.

**A software curtain must retreat, not just advance.** The first `TR_RAIN` only ever painted more
curtain, on the assumption a transition runs one way. It does not — the same effect runs backwards
to open the screen — so the curtain stayed down through the whole fade-in and the scene popped into
view at the end.

**And it must be made of shareable tiles.** The canvas shares one tile per solid colour, but only
for *fully covered* cells, and `ui_rect`'s filled path draws anything ≤48px tall as exact pixels
(right for chrome, fatal for a curtain). Both software effects now use `ui_fill_cells`, which always
takes the shared path — so a full-screen curtain costs **one tile**, not 600, and this ROM reserves
just 128. Before that it emptied the tile allocator about a second into the fall.

**`ui_clear_rect` could not clear a shared-solid cell at all.** It only blanked cells that owned a
tile, so clearing over any large fill was a silent no-op — a pre-existing gap that a curtain is
simply the first thing to *need* fixed, because a screen that never un-blacks reads as a hang.

## What is not here

A true A→B **crossfade**. Blending needs both images resident in VRAM at once, and the scene
lifecycle deliberately tears the old scene down before building the new one — agb does not return a
scene's ~40KB tile block until a frame boundary, which is why `scene.tish` holds at black for a
frame in between. `blend_alpha` exists as a native and is worth having (a ghost, a glass pane, a
dimmed panel), but it blends layers *within* one scene and is not a scene transition.

## Verify

```bash
./verify.sh
```

Checks that all eleven effects are reached, that 2400 frames soak without a crash or an alloc halt,
that the tile allocator survives both software effects, and — the check that matters — that eight
hand-picked mid-transition frames are **partially** covered. "The screen is black" is both the
correct state at the middle of every effect and the failure state of nearly every bug in one, so a
test that accepted black would have passed on all three of the bugs above.
