# BENCH AI

> *A benchmark testing the performance of the ai subsystem.*

<img src="preview.png" alt="preview" width="480">

The same patrol AI over 100 entities, written five ways, timed on hardware-accurate emulation. It
answers the question every game in this repo eventually runs into: **how many things can I update per
frame in tish, and does annotating types help?**

Run it:

```bash
unset CARGO_TARGET_DIR && npm run build -w bench-ai
GBA_SHOT_LOG=1 tools/gba-shot examples/bench-ai/bench-ai.gba /tmp/ai.ppm 500 2>&1 | rg "BENCH-AI|ticks"
```

Costs are Timer2 ticks (3.815µs each). **One frame is ~4389 ticks** — the number that turns any of
these into "fits" or "does not fit".

## Results (N = 100)

| variant | ticks/pass | per entity | vs best | before native i32 |
|---|---|---|---|---|
| untyped — array of dynamic objects | 13319 | 133 | +8118% | 13370 |
| **typed struct array, `: i32` throughout, `: i32` counter** | **162** | **1.6** | best | 7153 |
| typed, `: i32` value local, plain `number` counter | 2020 | 20 | +1147% | 6141 |
| typed, value local left unannotated | 5435 | 54 | +3255% | 7554 |
| bare loop (`acc = acc + e`), no body | 1630 | 16 | — | 1638 |

The last column is the same bench before the compiler learned to keep integer pairs in integer
registers (see "The compiler ask, answered"). Untyped and the bare loop are the control — both run on
`f64` locals, so neither should have moved, and neither did.

**The winner changed, and it is not a close race.** A loop whose counter, bound, locals, fields *and
literals* are all integers compiles to ordinary ARM integer code: 162 ticks, **3.7% of a frame** for
100 entities, 44x faster than the same loop before. Leave the counter a plain `number` and the same
body costs 2020 — **12x more** — because the counter alone drags every compare and increment back onto
soft-float. The bare `f64` loop at 1630 ticks is the tax you are paying for that one unannotated local.

## What to do with this

**Use typed struct arrays for anything per-entity.** `type Enemy = { x: i32, dir: i32 }` with
`let T: Enemy[] = []` compiles to a `Vec<TishStruct_Enemy>` with direct field stores, and beats an
array of dynamic objects by **2.2x**. The untyped version pays a hashed `get_prop`/`set_prop` and a
boxed `Value` for every field touch.

**Annotate every integer in the loop, counter included.** The old advice here was the opposite, and it
is now the single most expensive mistake you can make in a hot loop. An integer pair stays in registers
only if *both* sides are integers, so one unannotated participant re-floats everything it touches:

```tish
let i: i32 = 0                    // counter
let n: i32 = T.length             // ...and its bound, or the compare re-floats
while (i < n) {
  let v: i32 = T[i].x + T[i].dir  // and the body's locals
  i = i + 1
}
```

Whole-number *literals* fold into the integer side automatically, so `x > 0` and `i < 100` need no
help. A fractional literal does not (`x < 2.5` is a real comparison and stays on floats), which is
correct and is also a reason to keep game constants whole.

> `packages/ui.tish`'s layout solver was the counter-example that made the old rule "measure the loop
> you actually have" — it wanted its counters annotated when this bench said not to. Both now agree,
> so the rule has become simply *annotate them*. Re-measured: the solver's `arrange` dropped 64%.

**Budget about 2 ticks per entity per pass** for a fully-typed loop, down from 60. That is a rounding
error against a 4389-tick frame, and per-entity work is no longer what to look at first. Moving a loop
into Rust (`crates/tish-gba-game-engine`, `world_step`) is now a *last* resort for tish that is already
fully annotated, not a first one.

**The `f64` loop overhead is the whole story.** The bare pass — one add and a counter, no body — is
1630 ticks and did not move, because its locals are untyped. That single unannotated counter costs more
than ten fully-typed passes over 100 entities *with* bodies. If a loop is slow, look at its counter
before you look at what is inside it.

## The compiler ask, answered

This section used to ask for native `i32` arithmetic, on the grounds that every operation went through
`f64` on a CPU with no FPU. It landed, upstream in `tish_compile`:

When **both** operands of a binary op are already integers, `+`, `-` and the relational operators are
emitted as `i32` instead of widening to `f64` and calling soft-float. It is not an approximation —
every i32 is exact in f64, so both domains give bit-identical answers. The operators where they
genuinely disagree deliberately stay on floats: `/` (JS division is real division — `5 / 2` is 2.5),
`%` (integer `%` panics on a zero divisor where JS yields NaN) and `*` (a product can leave i32's range
while staying exact in f64, and the bitwise/hash lowering relies on that headroom). `u32` is excluded
as the one width that does not fit in an `i32`.

A whole-number **literal** counts as an integer for this, which turned out to matter more than the rest
of it put together — `x > 0`, `i < 100` and `0 - dir` are most of what a loop actually does, and each
one was pulling its integer operand back onto floats for a single comparison. Folding those in is what
took this bench from 3032 to 162.

The second half was a plain waste: `d = d + 1` on a `: i32` local boxed the result into a
`Value::Number` and ran ToInt32 back out of it, because the store path only recognised bitwise chains.
An integer RHS now stores straight into the register.

The compiler is upstream (`@tishlang/tish`), not in this repo; `docs/gba-in-tish-core.md` tracks
changes to it.

## Two ways this bench lied before it told the truth

Both are preserved in the source, because each nearly published a wrong number.

**A stale verdict.** This bench originally concluded that typed tish was *slightly slower* and could
not be made faster without a compiler fix. That was true of a compiler that mis-compiled typed writes
into a boxed throwaway — `set_prop(&object_from_pairs([...]), "x", …)` — which was slower **and
silently discarded the store**. The fix landed (`try_emit_native_member_assign`); the conclusion sat
here unre-run, and was quotable, for as long as nobody checked. It now reads 2.2x the other way.

Because that bug made stores *vanish*, and a loop whose writes are discarded is both faster and
wrong, a ratio proves nothing by itself. Every pass ends by checksumming its array, and all five must
agree (`10047`) or the comparison is between different amounts of work.

**A benchmark the optimiser deleted.** The bare pass first had an *empty* body. An empty loop computes
nothing, so LLVM removed it, and the pass duly reported 30 ticks — the cost of the two `timer_read`
calls around a loop that no longer existed. That very nearly became a published finding that loop
overhead was free, which would have pointed all of the above at the wrong culprit. It now accumulates
into a variable that gets logged. **A benchmark's result has to be observable, or the optimiser is
entitled to skip the work you are trying to time.**

## Where this bench used to disagree with the real code: the UI layout solver

`packages/ui.tish` annotates its layout-solver locals `: i32`, counters included, which this bench used
to price at +16%. Applying the finding there and de-annotating the counters made it **worse** —
measured on `ui-demo` with `uiInit({ stats: 1 })`:

| pass | counters `: i32` (shipped) | counters plain |
|---|---|---|
| measure | 6162t | 9661t (+57%) |
| arrange | 13884t | 17425t (+26%) |

For a while that stood as a warning that a microbenchmark can be convincing and wrong about code it
does not resemble. **The disagreement is now resolved, and the solver was right:** once integer pairs
stopped being widened, annotating the counter became the winning move in this bench too, by 12x. The
same solver re-measures at **measure 4119t, arrange 5038t** — a 34% and 64% cut for no source change.

Worth knowing for anyone looking for the next win: that screen still costs **435ms** to build 32 nodes
(from 499ms). The money left is in `paint` (49%), `flat` (26%) and the geometry write-back (17%) — all
native blitting or **boxed** property access, none of which an integer fast path can touch.
