# illusions

![preview](preview.png)

Ten optical illusions on real GBA hardware. LEFT/RIGHT pick a page, A crosses over now; left alone
it cycles through all ten on its own, using a different screen transition each time.

```bash
npm run build && npm start
```

## Why a GBA is good at this

Most classic illusions are flat geometry, a repeating pattern, or a colour that changes without the
picture changing — which is to say, they are the three things this machine does in hardware and
almost nothing else. A cafe wall is a tilemap. A lilac chaser is one palette write per frame. An
afterimage is the BLDY brightness register. A barber pole is a scroll register and an occluder.

The illusions that are *hard* here are the ones a modern engine finds trivial: anything needing a
soft gradient, a rotation, or per-pixel alpha. That inversion is the interesting part of the
example, and it is why nothing here is a picture of an illusion — every page is generated.

## The pages

| # | page | what it claims | how it is drawn |
|---|---|---|---|
| 0 | cafe wall | parallel rows look wedge-shaped | tilemap; quarter-block stagger, **mid-grey** mortar |
| 1 | hermann grid | grey ghosts at crossings you are not looking at | tilemap; 24px pitch |
| 2 | ouchi | the inset floats on its own plane and slides | tilemap; two 2:1 checkerboards at 90° |
| 3 | kanizsa triangle | a bright triangle with edges, over nothing | terrain; 3 discs + carved 60° wedges |
| 4 | ebbinghaus | two **identical** discs look different sizes | terrain; 6 large vs 12 small satellites |
| 5 | muller-lyer | two **identical** shafts look different lengths | UI canvas; hairlines only |
| 6 | lilac chaser | a green disc that is not there, chasing a gap | terrain; one `terrain_palette` write per step |
| 7 | afterimage | the flag returns in its real colours | tilemap in complements + `fade_white` |
| 8 | motion aftereffect | a stopped, static screen drifts | tilemap + `bg_scroll`, clamped dead |
| 9 | barber pole | horizontal motion seen as vertical | tilemap + `bg_scroll` + a canvas aperture |

Several of the transitions between pages are illusions themselves — `TR_MOSAIC` is pixelation
recognition, `TR_CHECKER` a dissolve, `TR_BARS` an aperture effect — so the chrome is part of the
content rather than a wrapper round it.

## The three surfaces, and why each page picks one

**The pattern tilemap** (`assets/pat.png`, priority 3) — every full-screen repeating figure. A cafe
wall covers 540 cells. On the UI canvas that is 540 *allocated* tiles, because `ui_rect`'s filled
path is pixel-exact for anything ≤ 48px tall; on a tilemap it is 540 references to the eight
distinct tiles in the asset. Same picture, about 40KB of VRAM apart.

**The terrain layer** (priority 2) — every figure made of circles. `terrain_disc` / `terrain_carve`
are the only per-pixel primitives in the engine. Its second use is subtler and is why two pages
animate for free: terrain is 4bpp with fifteen material indices, so a disc drawn as material 7 can
be shown or hidden *later* by writing its palette entry. No redraw at all. That is the whole lilac
chaser.

**The UI canvas** (front) — the label, thin straight lines, and the barber pole's aperture.
Deliberately not used for anything that covers area, except through `ui_fill_cells`.

## The traps this example ran into

- **A `background:` image and a repainting UI canvas cannot coexist.** The label renders as blocks
  of the page's own tile. There is no `background:` import in this ROM for that reason, and
  `verify.sh` fails if one appears. A tilemap whose *identity* never changes — only its cell
  indices — is not the same thing and is safe.
- **`bgtiles:`, not `background:`, for the tileset.** `background:` passes agb's `deduplicate`,
  which collapses identical 8×8 tiles and shortens the table `tilemap_set8` indexes into. This
  tileset is mostly flat fills, i.e. exactly what dedup collapses, and the failure is silent.
- **A software transition owns the canvas.** `TR_CHECKER` clears the cells it painted as it opens,
  so anything a page drew there during `enter()` — which happens at the black midpoint, *before*
  the curtain lifts — is wiped. It cost an hour on the lilac chaser's fixation cross, which simply
  was not there. Pages that draw on the canvas get a hardware transition.
- **A scrolling page must paint all 32×32 cells, not the 30×20 on screen.** `tilemap_new` allocates
  a 256×256 background and the hardware wraps it; the unwritten rows are invisible until something
  scrolls, and then the pattern tears open.
- **`ticks()` wraps about every fifteen frames.** A build slower than that wraps and comes back
  looking *fast* — the barber pole's 1,024 tilemap writes report 2,267 ticks. `verify.sh` measures
  the page-to-page cadence in frames instead, which cannot wrap.
- **Fill the label band, do not clear it.** `ui_clear_rect` leaves the cells transparent, which was
  fine until two pages started scrolling stripes through the text.

## Verifying

```bash
./verify.sh
```

The screenshot checks do not ask "is something on screen" — nearly every way this example can break
still produces a perfectly reasonable-looking picture. An Ebbinghaus whose two centre discs are
*genuinely* different sizes looks exactly like a working Ebbinghaus, and that one is undetectable by
eye by construction, since the page is about the eye getting that comparison wrong. So each check
asserts the specific claim its page makes: the two Ebbinghaus centres are counted and must match
exactly, the two Muller-Lyer shafts are measured and must match, the lilac gap must *move*, and the
motion aftereffect must both run and then stop dead.

Two traps worth keeping if you extend it:

- **Sample across the axis the thing moves along.** The motion aftereffect's stripes are horizontal
  and scroll vertically, so every row is a flat colour and stays one — a row-wise comparison calls a
  scrolling page static.
- **A repeating pattern aliases.** Stripes with an 8px period scrolling 2px a frame are pixel
  identical every 4 frames, however fast they are moving. The first version compared frames 60
  apart — exactly 15 periods — and reported a perfect false negative.

## Regenerating the tileset

```bash
cd assets && python3 make_tiles.py
```

`make_tiles.py` is the source of truth for the tile indices; the `T_*` constants at the top of
`src/main.tish` mirror the order it prints.
