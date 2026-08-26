# BENCH MEMORY

> *A benchmark testing the performance of the memory subsystem.*

<img src="preview.gif" alt="preview" width="480">

**Which subsystem does not give its heap back?**

Nearly every crash on this hardware is an allocation failure. The hard part is never that the heap is
small — it is 136K and always was — it is finding which of a dozen subsystems keeps a few K after it says
it is done. Chasing that inside a game means walking a player to a door for two minutes per build, with a
scene load, a dialog, a menu warm and an audio swap all landing in the same frame.

So this runs one subsystem at a time, in a loop, printing free heap after every cycle.

```
npm run build -w bench-memory
GBA_SHOT_LOG=1 scripts/screenshot.sh examples/bench-memory/bench-memory.gba /tmp/m.png 4000
```

## Reading it

A cycle that gives everything back is **flat**: the number after cycle 6 equals the number after cycle 1,
and the trial reports `leaked 0`. A slope is a leak, and the trial name says who owns it.

The first cycle of each trial is a warm-up and is not measured. First touch legitimately allocates — a
lazy cache, a font atlas, a sprite pool — and counting it as a leak would flag everything.

`heap_free(blk)` probes with a given block size. Compare `heap_free()` (1K) against `heap_free(64)` to see
fragmentation: if the 64-byte number stays healthy while the 1K number falls, the heap has the space but
not in one piece.

## What it found

- **An entity wrapper cost 10.6K, and there was one per entity.** ~62 keys at ~90 bytes each (a tish
  object's hash map bucket plus its key string) and ~57 bytes per method closure. Eleven entities and
  nothing else OOM'd the machine. Wrappers are pooled now (see `packages/engine.tish`) and a scene's
  entities cost ~520 bytes each, all of it native.
- **A trial that skipped `frame()` looked exactly like a leak in `ui_clear`.** Tiles are VRAM-backed and
  released at the commit, so rendering six panels without presenting piled up every tile, tripped agb's
  live-tile map into its 20K growth step, and then ran the GBA out of tile VRAM. The bench was measuring
  something no game does. If a trial reports a leak, check it does the thing a game would do.
- **`heap_free` used to take the game down with it.** It held its probe blocks in a `Vec` of pointers, and
  a 64-byte probe needs thousands of slots — asking a fragmented heap for that one contiguous 16K failed,
  in the exact conditions the probe existed to diagnose. It threads a free list through the blocks now and
  allocates nothing but what it counts.

## Correctness, not just size

Pooled wrappers buy their memory with a rule: a wrapper is a borrowed slot. The rule is only worth
something if the shapes a game actually writes are safe, so the same binary asserts them — spawning inside
a hook and still using `this`, keeping a list of entities to unload later, `this` and `other` being
different entities. Each was a real bug (one despawned the player and the game stopped responding without
crashing). They print `PASS`/`FAIL` before the memory trials.
