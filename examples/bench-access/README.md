# BENCH ACCESS

> *What a data access and a function call actually cost on GBA.*

Every optimisation in this repo has come from removing calls or removing array accesses — and until
this ROM, the numbers behind those decisions were **divided out of a larger measurement** rather than
measured on their own. `packages/grid.tish` justified `gridSimMove` with "~150 ticks per user tish
function call"; `drop_attack.tish` was rewritten around "~40 ticks per module-level typed-array
read". Neither was ever checked directly. This checks them, one operation at a time.

It measures, on device, with Timer2 (4,389 ticks = one 60fps frame):

- a module-level typed-array read vs a local one
- call overhead, and how it scales **per argument**
- passing an array across a module boundary

## Build / run
```bash
npm run build && npm start
```

Read the results with `GBA_SHOT_LOG=1 ../../scripts/screenshot.sh bench-access.gba /tmp/x.png 900`.
The numbers it produces are the ones quoted in [docs/perf-rules.md](../../docs/perf-rules.md) §2.

## Two findings here are now historical

Both were true, both were asserted as gates so they would fail the day the compiler fixed them, and
both duly failed. They are **inverted** in `verify.sh` rather than deleted, because the interesting
direction is now the regression.

| was | now |
|---|---|
| a **zero-argument** call cost ~55 ticks against ~0 for a 4-argument one — no-params was a hole in typed lowering | free, like any other. `tishlang/tish#647` |
| an array **literal** read ~37x slower than the same data built with `push()` (63.2 ticks vs 1.68) — a promoted static's index path was boxed | ~0.67 vs ~1.69. The gap did not close, it **inverted**: a static is a ROM load, a `VmRef<Vec<i32>>` costs a borrow and a bounds check. `tishlang/tish#645` |

⚠️ The literal story does not end there, and the rest of it is not in this ROM. #645's fix reaches
the index shapes the compiler can reduce to a mask; the bounds-checked fallback emitted for a
**computed** index still reads through `f64` and still costs ~25 ticks an element.
[`bench-tables`](../bench-tables/README.md) isolates that, at the scale the generated tables in
the topdown RPG ports actually use.
