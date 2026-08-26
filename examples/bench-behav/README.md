# BENCH BEHAV

> *A benchmark testing the performance of the behav subsystem.*

<img src="preview.gif" alt="preview" width="480">

`bench-room` answered "are native enemies expensive?" — they are not. Twenty-four of them on
`chase`/`hopper` hold 60fps, at roughly 115 ticks each. This bench answers the question that was
actually behind the topdown RPG port's slowdown: **what does it cost to reach a tish component at all?**

All three components below run the same body — decrement two counters that live with the entity, set
one animation frame. Only the calling convention differs, so the spread is the convention.

| convention | ticks per component | ×4 | whole frame (4389 = 60fps) |
| --- | --- | --- | --- |
| no component | ~10 | 40 | 4403 |
| `update: ({ this })`, state in `this.data` | **~1000** | 3942 | **6647** |
| `update: ({ this })`, state in cvars | ~770 | 3068 | 4404 |
| `lean: true` + `tick: (id: i32)` | **~230** | 913 | 4403 |

## What it means

**A boxed component costs about 1000 ticks per frame before its body does anything useful.** That is
23% of a 60fps frame to decrement two counters. Four of them eat 90% of the frame and push the frame
period to 6647 ticks — the game is then missing vblanks, which is what "the room slows to a halt"
looks like from the couch.

The cost is not the body, it is the path to the body. `registerBehaviour` in `packages/engine.tish`
wraps every `update` hook so that each call materialises an entity wrapper (`hookSelf`), packs it
into a `{ this, dt }` object literal, and calls through a closure. None of that survives to do any
work; it is rebuilt every frame for every component.

Splitting the difference shows where it goes. Keeping the boxed hook but moving state out of
`this.data` into engine cvars saves only ~22% (1000 → 770) — so the property lookups are the smaller
half. The remaining ~770 is the wrapper and the ctx object, and the only thing that removes it is the
lean convention, where the engine hands the body a raw `id: i32` with no wrapper, no ctx and no
closure hop.

## What to do with it

- **Anything that runs every frame should be `lean`.** Movement and animation should not be a tish
  component at all — use the native components (`chase`, `hopper`, `jumper`, `patrol`) and pay ~115
  ticks instead of ~1000.
- **`update: ({ this })` is for events, not frames.** It is a fine way to write `onCollide`,
  `onDeath`, `onInteract` and one-shot logic, where it runs occasionally and the ergonomics are worth
  it. It is the wrong tool for a hook that fires 60 times a second.
- **Budget it.** At ~1000 ticks each, four boxed per-frame components is the entire frame. If a game
  wants more than one or two, they have to be lean.

## Running it

```bash
npm run build -w bench-behav
GBA_SHOT_LOG=1 scripts/screenshot.sh examples/bench-behav/bench-behav.gba /tmp/bb.png 8000 "" \
  | rg BEHAV
```

`step_ticks(1)` is the behaviour phase in isolation, so the reading is not diluted by movement,
collision or render.
