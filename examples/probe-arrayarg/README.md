# PROBE ARRAYARG

> *Does passing a typed module array to a native de-optimise every OTHER read of it? Yes — 3.8x.*


A compiler probe, in the style of [`bench-tables`](../bench-tables/README.md). Two arrays in one
module, declared identically, filled identically, read by identical loops. The only difference in
the whole program is that one of them is handed to a native function **once**.

```
ok   A — never passed to a native — reads through the typed Vec path
ok   B — passed to a native ONCE — reads through the boxed Value path
ok   A is NOT boxed (the difference is real, not universal)
ok   the passed array costs >2.5x per read (379% of the untouched one)
ok   a MASKED index is not the difference (A[i]=1584 vs A[i&1023]=1680)
ok   the reader's MODULE is not the difference (in-module=1584 vs entry-module=1632)
```

1,024 reads: **1,584 ticks** for the untouched array, **6,005** for the one passed to a native.
1.55 vs 5.87 ticks per read. Filed as
[tishlang/tish#663](https://github.com/tishlang/tish/issues/663).

## Why a probe rather than a fix

`packages/dungeon.tish` was 10x slower than it should have been and **four** explanations were
tested and disproven against the generated Rust: that `export` de-typed the array; that filling it
inside a function did; that a module-scalar loop bound did; that it was universal to packages.

⚠️ **The first version of this probe produced a false negative.** It called
`grid_from_gids(32, 32, B, A)` — which passes *both* arrays — observed that A was boxed too, and
concluded that passing to a native was not the trigger. That wrong answer sent the search off for
four more rounds. Check the arguments before believing a negative result.

The two controls exist for the same reason: the difference is *not* the index shape and *not* which
module the reader lives in, and both are asserted so nobody re-litigates them from the source.

## The workaround

Keep two arrays: a private one for the hot loops, and a copy used only for the handoff.
`packages/dungeon.tish` does this — the copy is O(n) once per level at ~1.35 ticks an element,
against ~4.3 extra ticks on every read for the life of the ROM.

```bash
npm run verify
```

`verify.sh` asserts the codegen fact against the generated Rust as well as the timing, so it is a
statement about the compiler and not a number that drifts with a toolchain bump. It also
`rm -rf .tish` first — `tish build` caches packages, and reading a stale `main.rs` cost two rounds
of this investigation.
