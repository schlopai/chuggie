# BENCH TABLES

> *What a generated table costs to read, and whether caching one is worth anything.*


This ROM was built to answer one question before any code was written against it:

> The topdown RPG port ships 76 KB of `export let X: i32[] = [...]`, and its sibling port and the large SRPG example ship more.
> Should this repo generalise a warmed two-slot table cache into its table generator?

The answer is **no**, and the reason is not the one the question assumed.

## What it measures

The same 2,304 values, authored two ways — as a module-level array **literal** (which the compiler
promotes to a Rust `static`) and as an array built with **`push()`** (which stays a
`VmRef<Vec<i32>>`) — read six ways. 2,304 = 18 × 128 is
the shape of the topdown RPG port's generated dungeon-room table; this bench is about that
file.

Both arms are checksummed against each other at boot: a bench whose two arms hold different data
measures nothing, so the ROM refuses to report unless they agree.

## Results

Net of a loop-only control at the same N, in ticks per operation. One frame is 4,389.

| read | ticks |
|---|---:|
| `LIT[k & 2047]` — promoted static, masked index | **0.45** |
| `PUSH[k & 2047]` — pushed `Vec`, masked index | 1.54 |
| `WARM[k & 127]` — a warmed 128-entry pushed copy of `LIT` | 1.40 |
| `litAt(k)` — `LIT` through an accessor that reads a module scalar | **175.8** |
| `pushAt(k)` — the same accessor over `PUSH` | 177.0 |

### 1. The cache has nothing left to recover

A promoted literal now reads **3.4× faster** than the pushed copy you would cache it into. A static
is a ROM load; a `VmRef<Vec<i32>>` costs a borrow and a bounds check. `tishlang/tish#645` closed the
37× gap [`bench-access`](../bench-access/README.md) measured, and then inverted it.

### 2. The cost that replaced it is the accessor, not the array

175.8 ticks against a 0.45-tick read — and **the same number over either arm**, within 1%. So it is
not a property of how the table was written and no cache can touch it. The topdown RPG port reaches its
tables through `uwDoor`, which calls `uwIndex`, which reads the module scalars `quest` and
`curLevel`; touching module state disqualifies a function from typed lowering
(`tishlang/tish#647`), so every one of that file's nineteen accessors is a boxed `value_call`.

**Hoist the index. Do not cache the data.**

### 3. ⚠️ A live bug: the index *expression* decides a promoted array's cost

Reading the identical static costs **0.4 ticks** an element with a masked index and **25** with an
additive one. Same array, same values, same loop, same function. The generated Rust says why, and it
is not bounds-checking:

```rust
// LIT[(base + i) & 2047]
G_LIT[((base.wrapping_add(i) & 2047)).max(0) as usize]          // a direct i32 load

// LIT[base + i]
{ let _i = (base.wrapping_add(i)) as usize;
  if _i < 2304 { G_LIT[_i] as f64 } else { f64::NAN } } as i32  // out to f64 and back
```

`#645` ("a promoted array's read leaves the integer domain") landed for the shapes the compiler can
reduce to a mask. The **bounds-checked fallback** emitted for every other index is still `f64`-typed
— two soft-float conversions per element on a chip with no FPU ([perf-rules §1]).

That is most of the generated tables in this repo. `UW_DOORS[uwIndex(r) * 4 + side]` pays it;
the sibling port's `ow_portals` and `uw_doors` are the same shape. Filed upstream. Until it lands, **mask the
index** — 0.4 ticks, against 1.5 for a warmed copy and 25 for the additive form.

## Build / run

```bash
npm run verify
```

`npm run assets` regenerates `src/tables.tish`; `verify.sh` fails if the committed file has drifted
from the generator.

## Reading the numbers safely

Every span sits alone inside its own frame. `timer_read()` is 16-bit and wraps every ~15 frames, and
`bench-access` learned what that costs: a run with no `frame()` in it reported a call at 0.026 ticks
and an array parameter as *negative*, and the half that happened to land in the first frame was
believed for weeks. This ROM checks itself — every span positive, inside a frame, and above its own
control — and prints `SANE 1` only if all of that holds.

The fill is measured at **two widths** for the same reason. One width cannot separate the
per-element slope from the one-off boxed call, and the first draft of this bench reported ~28 ticks
an element for a copy whose parts cost ~2 — inventing a problem for a cache to solve.

[perf-rules §1]: ../../docs/perf-rules.md
