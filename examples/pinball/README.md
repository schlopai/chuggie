# pinball

> *A table with no tiles: per-pixel walls, a ball integrated in fixed point, and two flippers that actually swing.*

![preview](preview.gif)

Plunge, flip, and try to keep three balls off the drain. Bumpers score and kick, the funnels feed
the flippers, and a flipper caught on the rise throws the ball harder the closer to the tip it hits.
Leave it alone and it plays itself.

## Controls

- **A** — hold to charge the plunger, release to launch
- **L** or **LEFT** — left flipper · **R** or **RIGHT** — right flipper
- Touch nothing and the attract player takes over, pressing the same buttons you would.

## What it proves

**The per-pixel terrain layer is not just for destructible planets.** `terrain_*` exists for
`examples/warheads`, where it is a planet being blown apart. Here it holds a static hand-drawn
table, and that is what makes a *pinball* table possible at all: arbitrary curves, angled funnels,
and walls at any angle rather than on a grid.

**Not everything belongs on the engine's physics.** `set_dynamic` + `world_step` is this repo's disc
physics — `golf` and `soccer` are built on it — and it is the wrong tool here, which is worth
knowing before someone else reaches for it:

- **A dynamic disc has no gravity.** Gravity lives only in `platformer_system`, as a constant, for
  AABB bodies.
- **It collides against the tile grid, not the terrain.** A `.tmj` table is a 16px staircase, so
  every bounce leaves at the wrong angle — and a flipper cannot rotate, because tiles do not.

So the ball is integrated here, in Q8 fixed point, against `terrain_solid`. The whole step —
gravity, substeps, terrain probes, normals, both flippers — measures **476 ticks average, 2,106
peak** against a 4,389-tick frame.

## The five things that were wrong

Each of these looked like a physics bug and every one was geometry or tuning. They are recorded
because a per-pixel table gives you no compiler errors at all.

1. **The lane had no floor**, so its bottom *was* the drain. Three balls were lost in a row before a
   shot was ever taken.
2. **The "guide curve" sealed the lane.** It sloped the wrong way; the ball climbed to y=62, fell
   back and rested, with the game logged as in-play and nothing on the table ever touched.
3. **The launch was on the threshold.** A 98px climb needs 4.6 px/frame *before* the per-frame
   damping, so whether a ball reached the table came down to the charge roll — measured: in play for
   14% of 6,000 frames, and the first 1,200 produced none. Fixed by opening a 40px doorway rather
   than a slot, and giving the launch real margin.
4. **A 12px doorway was still a slot.** The deflector turned the ball left correctly — a traced
   `vx` of -471 — and it then hit the top of the inner wall four pixels below and came straight
   back. Zero play in 6,000 frames. A ball arrives at whatever height its own speed took it to.
5. **The ball wedged beside the right flipper**, jittering at (143,121) for 4,000 frames: the funnel
   ended 4px inboard of the pivot, leaving a notch, and the attract player *held* the flipper up,
   which was the lid on it. The funnels now overlap the pivots, and the attract player taps.

The debugging that mattered was never a screenshot — a stuck ball and a moving one look identical in
a still. Every one of these was found by logging `x, y, vx, vy` and reading the trajectory.

## The ball search

There is one, for the reason real tables have one: a ball *can* rest somewhere its designer did not
think of, and a pinball machine that has quietly stopped is indistinguishable from a crashed ROM. If
the ball is slow for two seconds it gets nudged; if that fails it is drained. It caught a genuine
pocket on top of the deflector — three nudges at (227,48) in one game — which was then designed out
by tucking the deflector up under the ceiling, leaving no room above it for a ball to sit in.

A 12,000-frame soak now runs with **zero nudges and zero stuck-drains**.

## Build

```bash
npm run build && npm start
```

```bash
python3 scripts/gen_pinball.py
```
