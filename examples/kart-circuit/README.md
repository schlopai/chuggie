# kart-circuit

> *A Mode 7 kart racer on a real Mode 7 ground plane: drift, boost pads, off-road penalties and three rubber-banded AI opponents around a generated circuit.*

![preview](preview.gif)

A Mode 7 kart racer on a real Mode 7 ground plane. Title → three laps against three AI
opponents → results, with drift-charged mini-turbos, boost pads, an off-road penalty and rubber-band
opponents.

**Controls** — A accelerate · B brake/reverse · Left/Right steer · R hold to drift · L use item ·
START to race · SELECT to watch a demo race.

```bash
npm run assets    # regenerate the track, karts and music (only after editing a generator)
npm run build
npm run start     # mGBA
npm run verify
```

## What is genuinely 3D

The track is a **plane being projected**, not a drawing of perspective. `packages/mode7` gives every
scanline its own affine transform, so ground scale changes with distance — which is what perspective
*is* — and the camera is somewhere, pointing somewhere, rather than being a fixed viewpoint the art
was drawn for.

The karts are **billboards**: flat sprites placed by projecting their ground coordinates. This
hardware cannot rotate or scale a sprite, so a kart's heading is one of eight baked frames and its
distance picks between two baked sizes. That is exactly what the classic SNES kart racers did, for exactly the
same reason.

## One source of truth for the course

`scripts/gen_kart_circuit.py` emits **both** `assets/track.png` and `src/track.tish` from a single
centre-line spline: the art, the surface map the physics reads, the racing line the AI follows, the
ordered gates that validate a lap, and the starting grid. None of them is authored separately, so
none of them can disagree — you cannot be told you are on grass while looking at tarmac.

## Items

Rows of three boxes sit on the racing line. Collecting one gives a **boost**, a **shell** that flies
along your heading and spins the first racer it touches, or a **banana** dropped behind you — which
is what makes holding one while leading worth doing. Opponents pick up and fire on the same code path
as the player, so the two cannot end up with different rules about what an item does.

## Music

`assets/race.deck` is a sixteen-bar loop written as **intensity stems** over one playhead: bass
always, lead from the title, drums during the race, stabs on the final lap. `deckSetIntensity` is a
mute gate on stems that are already running rather than a restart, so the arrangement lifts into the
last lap without the music skipping. It is played once at boot; every scene change is an intensity
move.

## Four things worth knowing before you change it

**The track may use at most 256 unique tiles, and going over is silent.** An affine background stores
each map entry as one byte; agb substitutes tile 0 for anything above 255 without a panic or a
warning, so an over-budget track quietly gets holes punched in it. Painting this circuit freehand
came to 925 tiles — a smooth curve crossing an 8px grid makes a near-unique pattern at every step
along its length, and stripping every bit of decoration only reached 729. The generator therefore
autotiles: it caches an 8×8 tile per quantised signature of the distance-to-centre-line at a cell's
four corners, which brings it to 153. **The generator prints the count. Read it after any change that
adds detail — it is the only warning you get.**

**The far field aliases, and two cheap things fix most of it.** One screen pixel near the horizon
covers several texels and there is no mipmapping, so high-contrast detail turns to coloured speckle.
`m7Haze(6)` hands the worst six scanlines back to the sky, and the generator's grass check is 16px
rather than 8 so it does not beat against the sampling grid. Together those took the measured shimmer
from 9.7 to 7.3 at no cost in draw distance or tiles.

**Billboard projection is fixed point, and that matters more than it sounds.** It used to be `f64`,
which was invisible at four karts. Adding item boxes and hazards pushed the course past thirty
billboards and the game dropped to about a fifth of full speed — no FPU, so every one of those was a
software routine. In 8.8 integers with a single multiply-divide each, thirty-one billboards now cost
less than four used to.

**The camera height and horizon must not change per frame.** The renderer caches its per-scanline
depth column keyed on exactly those two values, so a camera that bobs with speed rebuilds 160
fixed-point divisions every frame it moves. A speed-sensitive camera is the one effect this renderer
genuinely cannot afford.

## Where the frame goes

`kartStep()` and `kartPresent()` are one native call each, and between them they do every racer:
physics, surfaces, checkpoints, laps, opponent AI, billboard placement, sprite frames and the camera.

That is deliberate. Four karts' worth of physics written in tish would be several hundred software
float operations a frame on a CPU with no FPU — the same shape that took a four-billboard frame in
this repo from 4589 ticks to 8611, and that cost the rhythm game twelve frames a second in one divide
per prompt icon. The reusable half lives in `packages/kart.tish`; the arithmetic lives in
`crates/tish-agb/src/kart.rs`.

## Attract mode is also the test harness

SELECT runs a demo race with the player's kart handed to the same driver the opponents use. It is a
real feature, and it is the only way `verify.sh` can exercise a *complete* race: a fixed button
schedule cannot steer a circuit it gets no feedback from, and a hand-tuned one would be testing the
schedule rather than the game.

## What verify.sh actually asserts

Each check is picked to be able to fail for a real reason — a lap counter checked against the code
that increments it proves nothing.

| Check | What it would catch |
|---|---|
| Demo race finishes 3 laps in a plausible time | the race loop, gates, finish detection, results |
| Idle player comes 4th with 0 laps while the AI finish | opponents that were secretly driven by player input, or one shared progress number |
| Grass tops out under half of tarmac | a surface mask that is all-road, or misaligned with the art it came from |
| Crossing the line three times without a lap of the course scores 0 laps | a lap counter written as "crossed the line" rather than ordered gates |
| Mini-turbos fire in a race but never for a parked kart | the whole drift → charge → boost chain |
| No stray scanlines across 23 sampled frames | the HBlank DMA racing agb's own register writes |
| Boxes are collected in a race but never by a parked kart, and hazards go live | items sit on the racing line, can be picked up, and firing one puts something on the track |
| The race audio is denser than the title audio | the intensity stems actually gate — "there is sound" passes with every stem stuck on |
| `soak_rom` | crashes, and the allocation-failure halt that logs nothing |
