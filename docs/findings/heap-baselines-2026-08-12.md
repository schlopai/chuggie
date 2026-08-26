# Heap baselines, 2026-08-12

Taken **before** the native entity pool (plan WS2A) touched anything, so the pool's effect on each
consumer is a difference rather than a claim.

`card-gba/docs/lessons.md:1021` is the reason both numbers matter and why the total alone does not:
a heap with 52 KB free and no 40 KB run in it is a dead ROM. Where a probe at two sizes is available,
take both; `heap_free()` is the 1 KB probe and `heap_free(40960)` is the contiguous-block one.

## Measured

| example | boot | settled | notes |
|---|---:|---:|---|
| the pool spike (moved to the topdown RPG port's repo) | 196,608 | **182,272** | flat across 6 samples while firing. Pool high-water **5 of 8**. |
| the cast spike (same repo) | 205,824 | **191,488** | flat across 4 samples. |
| the topdown RPG port (same repo) | — | **126,976** | flat across 5 samples. |

Method: `npm run build` on a clean tree, then
`GBA_SHOT_LOG=1 ../../scripts/screenshot.sh <rom>.gba /tmp/x.png 900`, reading the ROM's own
`HEAP <n>` lines. The **settled** figure is the one to compare against — the engine's SoA columns are
high-water-mark, so the first rooms that fire bullets settle the heap once and it is flat after
(per the topdown RPG port's STATUS notes). Budget peak, not average.

## Not yet taken

the two large SRPG examples and the topdown RPG port. Each is a long build and none of them is touched by WS2A; take
their baselines at the top of WS1 (auto-battle) and WS2C respectively, by the same method. The large SRPG example
already ships `heapProbe: 1`, so its breakdown comes for free.

## What these are for

The pool replaces six hand-rolled slot tables with six natives and zero tish functions. The predicted
recovery is ~151 B per deleted tish function plus ~60 B per deleted module array —
≈2.6 KB in the topdown RPG port alone. A native export costs nothing until imported, so the trade should
be one-directional; if a settled number moves the wrong way, the pool is holding something it should
have released and that is a bug, not a cost.
