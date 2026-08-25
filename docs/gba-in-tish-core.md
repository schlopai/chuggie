# GBA support inside tish core — footprint, server-stability risk, and separation options

> **See also:** the tish repo's `docs/gba-target.md` is now the primary compiler-side
> separation reference (the four-layer model + the "no agb dependency in compiler crates"
> invariant + where new GBA code goes), and this repo's `ARCHITECTURE.md` covers the
> framework-side layers. This document remains the deep-dive on **server-stability risk**
> and the **separation options** for physically removing GBA code from the mainline binary.

**Status:** documented; two low-risk separations since performed (agb version single-sourced
from the facade; the game engine's direct `agb` dependency dropped). **Decision on the larger
options: a separate `gba` Cargo feature was considered and declined** — the existing `portable`
feature is the single embedded carve-out (on the runtime crates), and the compiler's GBA emit
code is already agb-free + `emit_mode`-gated, so a second flag is redundant. If the shared
build-surface residual ever needs isolating, add a non-GBA **CI job**, not a feature (Option 4).
**Date:** 2026-07.
**Why this doc exists:** tish is primarily used to compile **serverless functions, desktop apps, and web apps** — *not* games. GBA support was added in-tree (a locked decision in the original plan). This document records exactly what GBA-specific code now lives in the tish core repo, assesses whether it can destabilize the mainstream (serverless especially) use cases, and lays out the options for physically separating it later. It is the reference for that future decision.

---

## TL;DR

- **The mainline tish compiler does NOT depend on agb.** `tish_compile`, `tish_native`, and the `tish` CLI have zero agb / GBA-facade dependencies (verified in their `Cargo.toml`s). The only agb-dependent crate, `tish_runtime_gba`, is **`exclude`d from the workspace** (root `Cargo.toml`) and is built solely as part of a GBA target. A serverless build links no game/agb code.
- **GBA code emission is runtime-gated** behind `if emit_mode == NativeEmitMode::Gba`. A serverless/desktop compile runs in `DesktopBin` mode and never executes any GBA branch.
- **The few codegen changes that run for *all* targets are bug fixes**, not new behavior, and pass the 74-test differential gauntlet (typed == boxed == interpreter).
- **Net:** behavioral and dependency risk to server functions today is **low**. The real residual is that the GBA emit code is *compiled into* the mainline binary (a build-break / maintenance-surface concern), not that it can misbehave at runtime.

---

## Exact GBA footprint in tish core

### A. GBA-specific — only active for `--target gba`

| File | What | Notes |
|---|---|---|
| `tish_compile/src/lib.rs:45` | `NativeEmitMode::Gba` enum variant | An enum discriminant; harmless when unused. |
| `tish_compile/src/codegen.rs` | 9 `emit_mode == Gba` branches (lines ~2387, 2469, 2496, 2525, 2635, 3034, 3060, 7890, 8558) + `gba_no_std_rewrite` (line 762) | no_std header, `#[agb::entry] agb_main`, perf-pass/PropIC gating, the `std::`→`core::` post-pass, scheme-module emission. **All string emission — no agb dependency.** Unreachable unless emit_mode is Gba. |
| `tish_compile/src/types.rs:26` | `RustType::Fixed` (= agb `Num<i32,8>`) + ~7 match arms (115, 279, 285, 326, 363, 453, 478) | Emits `tishlang_runtime::Fixed`, which only resolves in a GBA build. Activates **only** on a `fixed` annotation; server code that never writes `fixed` never touches it (and couldn't compile `fixed` anyway — the facade type is absent off-GBA). |
| `tish_native/src/config.rs` | `NativeArtifact::GbaRom`, `NativeBuildConfig::gba()`, `gba_runtime_features()` | The GBA build config. |
| `tish_native/src/build.rs` | `build_gba_rom()` (line 322), the `thumbv4t-none-eabi` cargo scaffold, `agb-gbafix` invocation | Shells out to cargo/gbafix; links no agb into the compiler. |
| `tish/src/main.rs:917-937` | `--target gba` CLI handling | Selects `NativeBuildConfig::gba()`. **This is the surface the production CLI exposes** — a serverless-oriented `tish` still advertises a game-build target. |

### B. Generic / shared — runs for ALL targets (incl. serverless)

These were added during the GBA work but are **not** GBA-specific:

| Change | File | Server-stability assessment |
|---|---|---|
| **Import-scheme registry** (`schemes.rs`) | `tish_compile/src/schemes.rs` | Generic extension mechanism. `builtin()` is **empty** — zero agb knowledge. Inert unless a project declares/imports a scheme. |
| **Narrow int widths** `i8/u8/i16/u16/u32` | `types.rs`, `codegen.rs` | Activated only by those annotations. Generic and useful for desktop/server (compact structs). Additive. |
| **Struct-field-write fast path** (`try_emit_native_member_assign`, `codegen.rs:12265`) | `codegen.rs` | **Bug fix.** Previously `obj.field = v` on a native struct boxed to a throwaway `set_prop` and *silently lost the write*; now emits a native field store. Can only make correct server code *more* correct. Gauntlet-tested. |
| **`emit_f64` coercion fix** (`codegen.rs`) | `codegen.rs` | **Bug fix.** Previously returned a raw integer-scalar where an `f64` was required (`return <i32-local>` from a native-vec fn was a hard `rustc` error); now coerces. Pure fix. |
| **`fixed`-literal folding** (`fixed_literal_of`, `codegen.rs:13043`) | `codegen.rs` | Only fires when one operand is `Fixed` (a `fixed` annotation) — never for server code. |
| **Native integer arithmetic** (`codegen.rs`, the `Expr::Binary` arm) | `codegen.rs` | **Perf, semantics-preserving.** When both operands are already integer scalars, `+ - < <= > >= === !==` emit as `i32` instead of widening to `f64`. Every i32 is exact in f64, so both domains give bit-identical answers; adds use `wrapping_*`, matching JS's modulo-2³² ToInt32 and the truncating `as` store integer targets already perform. `/ % *` deliberately excluded — real division, NaN-vs-panic on a zero divisor, and the range headroom the bitwise/hash lowering relies on. `u32` excluded as the one width that does not fit. A whole-number **literal** folds into the integer side (`x > 0`, `i < 100`), which is where most of the win is; a fractional or out-of-range literal does not and stays on floats. Server-relevant only where a `: i32` annotation or the i32-loop-var lowering already applies. Full suite green including the backend-equivalence run. **Worth 44x on a fully-typed GBA entity loop and 64% off the UI layout solver's `arrange`.** |
| **`: i32` store without the box** (`codegen.rs`, `Assign` arm) | `codegen.rs` | **Perf.** `d = d + 1` on a `: i32` local used to box into `Value::Number` and run ToInt32 back out, because the store path only recognised bitwise chains. An integer RHS now stores straight into the register. |

The `portable` feature on `tish_core`/`tish_builtins` is purely additive (default `= ["std"]`); the std path is unchanged.

---

## Risk assessment for serverless / desktop / web

| Risk | Level | Basis |
|---|---|---|
| **Dependency instability** (agb pulled into the server binary) | **None** | Compiler crates don't depend on agb; `tish_runtime_gba` excluded from workspace. |
| **Runtime/codegen misbehavior of server code** | **Low** | GBA branches are `emit_mode`-gated (unreachable off-GBA). The only unconditional changes are bug fixes, gauntlet-verified. `fixed`/narrow-int are annotation-gated. |
| **Build-break surface** (a GBA-code change fails to compile → breaks *everyone's* `tish_compile` build) | **Moderate** | The GBA emit code is compiled into the mainline binary. It compiles today (string emission, no agb), but it is shared build surface. **This is the main residual risk.** |
| **Maintenance / cognitive load** | **Moderate** | 9 `emit_mode == Gba` branches interleaved in the shared codegen; a `Gba` arm to remember in `NativeEmitMode`/`RustType` matches. |

---

## Separation options (for a future decision)

Ordered by effort. Each is a *future* option — none is implemented.

1. **Feature-gate the entry points** — a `gba` Cargo feature (default OFF) on `tish_native` gating the build scaffold + `--target gba` CLI. Without it, `NativeEmitMode::Gba` is never constructed and `--target gba` errors clearly; the codegen Gba branches remain as dead-but-harmless code. *Low effort; stops the production CLI from exposing/invoking any game path. Doesn't remove GBA code from compilation.*
2. **Feature-gate + `#[cfg]`-out** — the `gba` feature *plus* `#[cfg(feature = "gba")]` on the codegen Gba branches, `gba_no_std_rewrite`, and `RustType::Fixed`, so **none** of the GBA code is compiled into the mainline binary. *Strongest "not in the serverless binary, can't break its build" guarantee. More invasive: cfg attributes through codegen + a few `RustType` match arms; requires a non-gba CI build to keep it honest.*
3. **Full extraction to a backend crate** — introduce a codegen backend seam and move the whole Gba emit mode into its own crate outside the tish mainline. *Cleanest end state; large refactor (codegen is monolithic today) with its own risk. Likely overkill given the compiler already links no agb.*
4. **Leave as-is + CI guards** — accept the current isolation; add CI that builds/tests the mainline (non-GBA) paths against a corpus of representative serverless-function programs, so any GBA change that regresses the shared path is caught. *Lowest effort; no physical separation.*

### Decision (2026-07): Option 4 only; no `gba` feature

Options 1 and 2 add a **`gba` Cargo feature** — a *second* embedded flag alongside the
existing **`portable`** feature. That was considered and **declined**: `portable` (on the
runtime crates) is already the single embedded carve-out, and the compiler's GBA emit code
is agb-free and `emit_mode`-gated, so a parallel flag is redundant surface for a maintenance-
only concern (it adds no runtime or dependency risk to serverless today). We keep **one**
embedded feature. **Option 4 (a non-GBA CI build + a server-program corpus)** is the chosen
path *if* the shared-build-surface residual ever needs guarding — it's cheap and directly
catches a GBA change that breaks the mainline, without a new flag or `#[cfg]` scatter.
Option 3 (backend-crate extraction) stays a far-future possibility only if a second embedded
target arrives.

### Triggers that would revisit this

- A GBA codegen change breaks a mainline build → add the **Option 4 CI job**.
- A second embedded target (beyond GBA) arrives and the `emit_mode` interleaving gets
  unwieldy → reconsider **Option 3** (a real backend seam), not a per-target feature flag.

---

## What is already clean

The **import-scheme layer is fully decoupled**: tish core's scheme registry (`schemes.rs::builtin()`) is empty of agb knowledge, and the `asset:` scheme is *contributed by tish-agb* (`crates/tish-agb/tish.schemes.json`, auto-discovered from a game's `rustDependencies`). New asset kinds (backgrounds, audio, custom schemes) and new runtime bindings (`cargo:`) need **zero tish-core edits**. See `docs/findings/P0-findings.md` and the project memory for details. Only the *emit-mode* + *`fixed` type* + *build-scaffold* layers remain in core.
