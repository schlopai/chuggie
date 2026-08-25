# tish GBA issue triage — 2026-08-18

Against `tishlang/tish` **main = `d0bae3e58`** (3.7.1 + the verified branch; the thirteen post-3.7.1
commits reverted in `4890764a6`).

88 issues open; **18 are GBA**. The single most important fact for anyone reading this:

> **Reverting main re-opened the fixes for nine GBA issues.** The thirteen reverted commits claimed
> #581, #653, #654, #655, #658, #659, #663, #665, #669, #672, #675. Every one of those issues was
> ALREADY open on the tracker — merging the PRs never closed them — and main no longer carries the
> code. They are not regressions in behaviour terms (3.7.1 never had the fixes), but anyone assuming
> "merged = fixed" will be wrong.

## A. Fix existed on main, reverted — decide per issue

| # | Title | Status now | Note |
|---|---|---|---|
| 581 | Default ROM Cargo profile uses fat LTO + 1 CGU | **unfixed on main** | PR re-proposed: `fix/581-gba-no-fat-lto` |
| 655 | Stack-overflow guard can never fire on GBA | **unfixed, CONFIRMED REAL** | PR re-proposed: `fix/655-gba-stack-guard-main`. See evidence below |
| 653 | Same-named constants from two modules collide | **NOT REPRODUCIBLE** | `examples/repro-validate-653` passes on main: `WALK=7`, `check(7)=seven`, `modeName(1)=walking`. Candidate to close |
| 654 | Immutable module constants captured as VmRef cells | unfixed | Six repros exist (`repro-654-*`); not re-run in this pass |
| 658 | Bounds-checked path still reads through f64 | unfixed | perf, not correctness |
| 659 | `--target js` duplicate top-level alias | unfixed | not GBA |
| 663 | Typed module array to a native de-optimises every read (3.8x) | unfixed | perf |
| 665 | Module variable passed to an assigning fn panics | unfixed | **correctness** — worth prioritising |
| 669 | Two generated-Rust compile failures on host backend | unfixed | not GBA |
| 672 | Array forwarded to a `cargo:` native is still boxed | unfixed | perf; `readonly` design |
| 675 | `readonly` unusable on array natives | unfixed | follow-up to 672 |

### #655 is confirmed real, with fresh evidence

The guard "compiled in but could never fire": `stack_low()` derives its floor from
`stacker::remaining_stack()`, which reports no bounds under `no_std`, so the floor falls back to 1 and
no SP can be below it.

This session paid for that directly. The large SRPG example overflowed its stack by 1,456 bytes and produced
**a jump through a wild pointer** — `pc=0x54, sp=0x8`, 18 million bad-memory accesses, no message, no
boot marker, nothing on screen. It took days to identify as a stack overflow. A working guard would
have named it on the first run. `examples/repro-validate-655` completes 2,000-deep recursion without
the guard firing, which is the same fact from the other side.

**Recommendation: merge #655 first of the two.** It is the difference between a diagnosable failure
and a silent one.

## B. Addressed by the work now on main

| # | Title | Status |
|---|---|---|
| 647 | Typed lowering is all-or-nothing: touching ANY module state forces a boxed value_call — "1 of hundreds of functions qualified in a real ROM" | **substantially improved** |

Measured on the large SRPG example: **35 native fns emitted**, `Value::` sites **12,953 → 9,446 (−27%)**,
run() frame **27,140 → 23,412 B**. The four changes that did it are all on main now:

- mixed-struct lowering — a struct param qualifies on ONE numeric field, not all of them (`05c7b6062`)
- i32 externs admitted to the native-safety proof, so calling into a `cargo:` crate no longer
  disqualifies a function (`26484f68d`)
- `: i32` locals and extern-call initialisers seeded as numeric — an annotation added for speed was
  silently disqualifying its own function (`f92c251b9`)
- integer→f64 coercion at returns and in three assignment paths (`26484f68d`, `1f45507d2`)

⚠️ NOT closed: the specific complaint "touching module state" still holds. A fn reading a mutable
module-scope binding is still boxed — that needs struct globals in a `Cell` static, which was
deliberately reverted as belonging in chuggie-engine rather than core. #631 is the same root cause.

## C. Open, unaddressed, no fix has ever existed

| # | Title | Kind |
|---|---|---|
| 631 | A fn reading a mutable module-scope array can never be promoted | perf — same root as 647 |
| 621 | tish→tish direct native calling convention (72 ticks vs ~7) — umbrella | perf |
| 620 | PropIC compiled out on GBA — needs a Cell-based variant | perf |
| 619 | ROM size +39% over a Rust-core twin — needs a census | size |
| 612 | `typeof` on a typed array cannot answer 'object' — bricked a ROM | **correctness** |
| 603 | GBA numerics target-gated, no host parity story | test infra |
| 602 | Per-frame engine→tish callback dispatch boxes its args | perf |
| 668 | Array aliasing lost on host backend (#597 escape analysis GBA-gated) | host |

## What I would do next, in order

1. **Merge #655.** Silent stack overflows cost this project days. Nothing else on this list has that
   ratio of effort to pain avoided.
2. **Re-open or re-propose #665** (module variable passed to an assigning fn panics). It is the only
   other correctness item among the reverted set, and correctness beats the perf items.
3. **Close #653** if `repro-validate-653` is accepted as evidence — it passes on main.
4. **Update #647** with the measured numbers above rather than closing it; the headline complaint
   ("1 of hundreds") is no longer true, but its module-state half is untouched.
5. **#612** is the remaining correctness item with a ROM-bricking report and no fix anywhere.

## Two findings worth folding into the tracker

**A `declare fn` with a trailing `//` comment is silently skipped.** The name then never resolves and
the ROM jumps through null at boot with no diagnostic. Cost a debugging cycle here; guarded now by
the repro-tables example (since moved to the chuggie-tactics repo), which calls every accessor. Same family as #655 — a failure with no
message. Not filed.

**A failed allocation during module init silently halts the ROM.** No panic, no fault, nothing
printed; execution simply stops mid-statement. Reproduced in `examples/repro-shell-reserve`: a growing
typed array crossing 32,768 elements asks for 256 KB in one allocation, fails with 87 KB free, and the
ROM stops. "White screen, no markers, no faults" should mean "an allocation failed" — and nothing in
the toolchain says so. Not filed.
