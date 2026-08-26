# P0 spike findings

De-risking results from the P0 spikes, run before touching the tish compiler.
These correct assumptions in the development plan and pin choices for later phases.

## P0a — no_std codegen-shape spike: **PASS**

`examples/p0-spike` hand-writes the shape the tish `Gba` emit mode will generate
and builds it for `thumbv4t-none-eabi`, then produces a valid `.gba` ROM.

Proven to compile and link for the real GBA target:

- **Arc→Rc alias** (`pub type Arc<T> = alloc::rc::Rc<T>`) absorbs emitted
  `Arc::from(..)` / `Arc::clone(..)` / `Arc::<str>::from(..)` sites unchanged.
  Zero-churn trick #1 holds.
- **Dynamic `Value` core** — `Number(f64) | Str(Rc<str>) | Bool | Null |
  Array(Rc<RefCell<Vec>>) | Object(Rc<RefCell<ObjectMap>>) | Function(Rc<dyn Fn>)` —
  compiles no_std. `Rc<dyn Fn(&[Value]) -> Value>` (what `Value::native` lowers to)
  works.
- **libm** soft-float math (`sin`/`sqrt`/`floor`/`trunc`/`fabs`) links and runs
  through the dynamic path.
- **`run() -> Result<(), Box<dyn core::error::Error>>`** — the `?` / boxed-error
  machinery survives no_std via stable `core::error::Error`.
- **`#[agb::entry]` + `agb::println!`** entry mirrors the emitted `agb_main`.
- ELF → **valid ROM** via `agb-gbafix` (188 KB; correct ARM entry branch, Nintendo
  logo header, `"p0-spike"` title, `0x96` header byte).

### Correction to the plan: hasher is FxHasher, not foldhash

The plan assumed `foldhash` for the portable `ObjectMap`. **foldhash 0.1.5 imports
`core::sync::atomic::AtomicUsize` in its seed module and does not compile on the
no-atomics thumbv4t target.** Switched to **`rustc-hash` (`FxBuildHasher`)**, which
is atomics-free and is exactly what agb's own `agb_hashmap` uses on GBA.

➜ **Decision:** portable `tish_core` standardizes `ObjectMap` / `IndexMap` on
`rustc_hash::FxBuildHasher`. Do not use foldhash or ahash on GBA.

### Boot verification — **RUNTIME PASS** (confirmed in mGBA 0.10.5)

`cargo run` in interactive mGBA runs the ROM at 60.7 fps and logs, via
`agb::println!`, exactly the expected results — proving the dynamic core doesn't
just link but *executes correctly* on-device:

```
p0-spike: dynamic Value core is alive
Mara            # Rc<str>-keyed FxHasher object map: stored + read back
75              # 100 - floor(sin(1.0)*30) — dynamic-Value arithmetic
[9, 75, ok]     # heterogeneous array display via alloc formatting
42              # sqrt(1764) dispatched through an Rc<dyn Fn> native fn
25              # floor(sin(1.0)*30) — libm soft-float
p0-spike: run() ok   # Box<dyn core::error::Error> path, no panic
```

The screen is blank white: the spike is a pure compute/logging smoke test and
never touches the display (no `gfx.frame()`/`commit()`), which is correct —
rendering starts at P2/P3.

Boot path: `cargo run` (interactive `mgba-qt`, per `.cargo/config.toml` runner)
works on the dev machine and streams the log to the terminal — this is the
standard manual boot-verification path for every phase. Automated/headless CI
boot checks use `mgba-test-runner` (mGBA-SDL needs a display window).

## P0c — hecs no_std on thumbv4t: **FAIL → use SoA store**

`hecs = { version = "0.11", default-features = false }` does **not** compile for
`thumbv4t-none-eabi`. Two transitive blockers, both atomics on a no-atomics target:

- `spin 0.10.1` → `unresolved import crate::atomic::AtomicU8` (`once.rs`).
- `foldhash` → `unresolved import core::sync::atomic::AtomicUsize` (`seed.rs`).

`spin` has a `portable-atomic` path, but `foldhash` imports `core::sync::atomic`
directly with no feature gate, so hecs cannot be made to work without patching a
dependency. Reproduction preserved in `p0c-hecs-probe.rs.txt` /
`p0c-hecs-Cargo.toml.txt` — re-check when hecs/spin/foldhash publish
atomics-free releases.

➜ **Decision:** `tish-gba-game-engine` uses a **custom SoA / archetype store** over the
closed component set (Transform, SpriteR, Animator, Collider, Body, GridPos,
Health, Hitbox, Projectile, Interact, CameraTarget, Persistent, Behaviour), the
fallback the plan already anticipated. For a fixed ~13-component game with a few
hundred entities this is simpler, faster, and fully controllable versus hecs's
dynamic archetype machinery.

## P1 (portable core) — tishlang_core + tishlang_builtins: **DONE**

Both crates gained a purely-additive `portable` (no_std + alloc) feature and now
compile for `thumbv4t-none-eabi`, with the default (std) build unchanged.

- **tishlang_core**: new `compat` module — `Arc→Rc` alias, `ArcStr` newtype over
  `Rc<str>`, `AHashMap`/`RandomState` = `hashbrown` + **FxHasher**, `AtomicU64` via
  `portable-atomic`, single-core `OnceLock`/`Mutex`/`RwLock`/`SingleCore` shims,
  a `FloatExt` trait (libm) so `n.abs()`/`n.floor()`/… compile no_std, and
  installable clock/RNG hooks. `thread_local!`s, `std::env`, `std::io`,
  `catch_unwind` all cfg-gated. Feature graph: `default = ["std"]` (ahash/arcstr
  optional; indexmap no_std-capable).
- **tishlang_builtins**: `#![no_std]` under portable; float files import
  `FloatExt`; `Date.now` / `Math.random` / array shuffle route through the
  tishlang_core hooks; symbol registry uses the portable locks + get/insert
  (no `Entry` hasher dependence); `unicode-normalization` set to no_std.
- **Verified**: `cargo build -p tishlang_core -p tishlang_builtins
  --no-default-features --features portable --target thumbv4t-none-eabi
  -Zbuild-std` (with `RUSTFLAGS=--cfg portable_atomic_unsafe_assume_single_core`)
  → clean; full host workspace build → green (no downstream breakage across ~30
  crates); `cargo test -p tishlang_core -p tishlang_builtins` → 85 passed / 0
  failed (std behavior preserved).

### P1 facade — `crates/tish_runtime_gba` (package `tishlang_runtime_gba`): **DONE**

The no_std crate generated GBA code links against (via the `package =` rename).
Compiles clean for thumbv4t (→ rlib). It re-exports the base codegen prelude
name-for-name — types (`Value`/`ObjectMap`/`ObjectData`/`PropMap`/`VmRef`/
`ArcStr`), `Arc` alias, `TishError`, pending-throw + recursion-guard plumbing,
JSON, and the globals/math/object/number/string/constructor/collection/
typedarray/symbol forwarders (all resolving to the now-portable
`tishlang_builtins`) — plus GBA-specific bits: `Fixed = Num<i32,8>`, `console_*`
→ `agb::println!`, the 6 hyperbolic math fns via `FloatExt`, and
`gba::{init, halt, register_pre_commit, run_pre_commit}`.

Key gotcha resolved: `agb` sets `portable_atomic_unsafe_assume_single_core`, which
forbids `portable-atomic`'s `critical-section` feature — so tishlang_core must NOT
enable it (agb supplies the single-core backend in the real build; standalone CI
passes the cfg).

**P1 is complete.**

## P2 — `tish build --target gba` emits a bootable ROM: **DONE**

`tish build hello.tish --target gba -o hello.gba` produces a valid **426 KB GBA
ROM** (verified header: ARM entry branch, Nintendo logo, "hello" title). The same
program runs identically on the host interpreter — semantics preserved, normal
tish paths unregressed (full host workspace build green).

Compiler changes (tish repo):
- `NativeEmitMode::Gba` + `NativeArtifact::GbaRom` + `NativeBuildConfig::gba()`
  (thumbv4t-none-eabi); GBA feature cap = none.
- codegen `emit_program`: Gba branch emits `#![no_std] #![no_main]` + alloc
  imports + `use tishlang_runtime::{Arc, FloatExt}`; the big prelude import is
  unchanged (facade re-exports every name); entry = `#[agb::entry] fn
  agb_main(gba) { gba::init(gba); let _ = run(); gba::halt() }`; `run()` stays
  sync (async is P5); native perf passes + `PropIC` inline cache gated off (both
  emit atomics/`thread_local!`); `cached_object_key` emits a direct `Arc::from`
  (no `OnceLock`); a `gba_no_std_rewrite` post-pass maps residual `std::` paths to
  `core::` and `ObjectMap::from(` → the facade's `object_map_from`.
- tish_native `build_gba_rom`: scaffolds an agb project (nightly + build-std +
  thumbv4t + gba.ld, `tishlang_runtime = { package = "tishlang_runtime_gba" }`,
  persistent `.tish/gba/<name>` build dir, mgba runner), runs its own `cargo
  build` (clean env so `.cargo/config` rustflags apply — `run_cargo_build` would
  shadow them via RUSTFLAGS), then `agb-gbafix` → `.gba`.
- CLI: `tish build --target gba -o game.gba`.

Facade completions P2 surfaced (generated code needs more than the prelude list):
`ops` module (Value arithmetic/compare), native-ABI `json_parse`/`json_stringify`
wrappers, `object_map_from`, `pub use FloatExt`, and the member-access functions
`get_prop`/`get_index`/`delete_property`/`set_prop`/`set_index`. Method dispatch
(`arr.map(f)` etc.) is emitted as direct builtin calls — those `array_*`/`string_*`
forwarders get added to the facade as later examples exercise them.

Boot: `cargo run --release` in the generated build dir launches mgba-qt (the p0
spike already proved the runtime facade boots and logs correctly).

## P3 (in progress) — `cargo:tish_agb` binding works on GBA: **foundation DONE**

A tish game that does `import { log, vblank } from 'cargo:tish_agb'` and calls
those agb-backed Rust functions builds to a valid **428 KB ROM**
(`examples/minimal`). This proves the whole binding path on GBA: tish import →
compiler-generated Value glue (`generated_native.rs`) → `tish_agb::fn(&[Value])`
→ agb hardware. The binding was the key de-risk for the entire framework.

- `crates/tish-agb` (package `tish_agb`, no_std, deps `tishlang_runtime_gba` +
  agb): exposes `log(&[Value])` (→ `agb::println!`) and `vblank(&[Value])` (→
  `VBlank::wait_for_vblank`), the native-module ABI the tish compiler auto-wraps.
- Compiler/scaffold fixes P3 surfaced (all in `build_gba_rom`):
  1. `cargo:` path deps arrive via `extra_dependencies_toml` (`tish.rustDependencies`) —
     thread it into the generated `[dependencies]`, not just `native_modules`.
  2. Generated crate needs an empty `[workspace]` so a parent Cargo workspace
     (the chuggie repo, when the build dir lives under it) doesn't absorb it.
  3. **cargo joins `rustflags` arrays across nested `.cargo/config.toml` files** —
     a build dir inside a project that already has `-Tgba.ld` in its config got it
     twice ("region 'ewram' already defined"). Fixed by passing the GBA rustflags
     via the `RUSTFLAGS` env (which *replaces*, not joins), keeping only
     build-std/target/runner in the generated config.
  4. The `cargo:` wrapper (`generate_native_wrapper_rs`) emits a std header;
     `build_gba_rom` maps it to the no_std facade equivalents.

### P3 — moving sprite from the d-pad, in tish: **DONE**

`examples/dpad-sprite` — a tish game that spawns a sprite and moves it with the
d-pad — builds to a valid **444 KB ROM**. This is the full vertical slice: tish
game logic → agb graphics + input → GBA hardware.

- Facade `gba::init(gba)` now **stashes** the `agb::Gba` peripheral bundle;
  `gba::take_gba()` hands it off (once) to the binding crate. `SingleCore`
  re-exported from the facade for binding-crate statics.
- `tish-agb` gained a retained `GbaCtx` (leaks the `Gba` to `'static` so
  `Graphics<'static>` lives in a single-core static) + a sprite arena, and
  exposes `sprite_create` / `sprite_set_pos` / `input_x` / `input_y` / `frame`.
  `frame()` rebuilds the draw list each frame (agb 0.25's frame-scoped model),
  commits (vblank), and refreshes input. One hardcoded 16×16 sprite for now.
- The tish game:
  ```tish
  import { sprite_create, sprite_set_pos, input_x, input_y, frame } from 'cargo:tish_agb'
  let s = sprite_create(); let x = 104; let y = 72
  while (true) {
    x = x + input_x() * 2; y = y + input_y() * 2
    sprite_set_pos(s, x, y); frame()
  }
  ```

Boot: `cd examples/dpad-sprite/.tish/gba/dpad-sprite && cargo run --release`.

### P4 (in progress) — typed Rust-scalar lowering: **`i32`/`f64` slice DONE**

Typed tish annotations naming concrete Rust scalars now lower to native Rust
instead of boxed `Value`. `from_annotation` (types.rs) maps `f64`→`F64` and
`i32`→`I32` (the existing integer-register `RustType`; its value is the JS ToInt32
view = an annotated `i32`). A typed loop:
```tish
fn addup(n: i32): i32 { let sum: i32 = 0; let i: i32 = 0
  while (i < n) { sum = sum + i; i = i + 1 } return sum }
```
generates `let mut sum: i32 = …; let mut i: i32 = …; while ((i as f64) < …)` — a
**native i32 loop**, not `Value` arithmetic — and builds to a GBA ROM. Verified:
74 compiler tests pass (no regression — the change is all-targets), and
`addup(10)` returns the correct `45` on host.

### P4 progress — `fixed` type + struct findings

**`fixed` lands as a native type** (partial): `RustType::Fixed` (= `tishlang_runtime::Fixed`
= agb `Num<i32,8>`), `from_annotation` maps `fixed`, and all four core conversion
methods handle it (to_rust_type_str / default_value / from_value_expr /
to_value_expr, with lossless Q24.8↔f64 boundary). `let px: fixed = 100.0` →
`let mut px: tishlang_runtime::Fixed`, builds to a ROM, correct result (106). Only
2 exhaustiveness sites needed arms; 74 compiler tests still pass.

**Native `fixed` arithmetic DONE**: `let px: fixed; px = px + vx` now lowers to
`px = (px + vx);` — native agb `Num` math, no boxing, no f64 round-trip, no FPU.
Two small edits did it: `result_type_of_binop` (types.rs) gained a `Fixed⊕Fixed`
case (Add/Sub/Mul/Div → Fixed; comparisons → Bool — all backed by `Num`'s operator
impls), and the native-assign fast-path (codegen.rs ~3069) added `Fixed` to its
`matches!(F64|Bool|String)`. `fixed` is now a real perf feature (fast integer
positions/velocities), at parity with `f64`'s native path. 74 tests still pass.
(`%`/`**`/bitwise on fixed, and `fixed`-mixed-with-literal, still box — refinements.)

**Struct/interface findings** (empirical): both `type X = {…}` and `interface X {…}`
already lower to native Rust structs (`TishStruct_X { pub x: f64, pub hp: i32 }`);
`Entity[]` lowers to `Vec<TishStruct_Entity>` with native construction + native
field *reads*. Fixed a no_std bug: struct→Value boundary leaked `::std::sync::Arc`
(added to `gba_no_std_rewrite`) — structs build on GBA now.

**Struct-field-write bug FIXED.** `es[i].x = …` (and `player.x = …`) previously fell
back to a boxed `set_prop` on a throwaway copy — the write was silently lost (the
interpreter was correct; codegen wasn't). New `try_emit_native_member_assign`
(codegen.rs) emits a **direct native field store** for both a local `Named` struct
(`player.x = …`, incl. RefCell-wrapped via a temp to dodge borrow conflicts) and a
`Vec<Named>` element (`entities[i].x = …`), wired into `emit_expr_discard` (statement
position) and the `MemberAssign` arm of `emit_expr` (expression position). Two small
helpers back it: `emit_index_usize` (loop-counter subst / f64·i32 cast / Value
unbox) and `emit_coerced_native` (f64↔i32 direct cast, else one Value round-trip).
Compound member-assign (`es[i].x += v`) desugars to `MemberAssign` in the parser, so
it's covered for free. Verified: `es[j].x = ((es[j]).x + (es[j]).vx)` emits fully
native on desktop **and** in a GBA ROM for `i32`/`f64`/`fixed` fields (428 KB ROM,
`fixed` fields = `Num<i32,8>` integer math, zero `set_prop`). **AoS entity mutation —
the Unity-component feel — now compiles native; SoA is no longer forced.**

**`fixed ⊕ numeric-literal` FIXED.** `px + 1.5`, `hp * 2`, `y < 100.0` previously boxed
(the literal is `f64`-typed → `Fixed⊕f64` fell through to `ops::*`). A fold step in the
typed `Binary` emitter now rewrites a `Fixed ⊕ <number-literal>` into `Fixed ⊕ Fixed` by
folding the literal to a compile-time `Fixed::from_raw((n·256) as i32)` const (helper
`fixed_literal_of`, truncating to match the dynamic boundary bit-for-bit), so the native
`Num<i32,8>` op fires. Verified on a GBA ROM: `vx = (vx + Fixed::from_raw(384i32))` (1.5),
`px < Fixed::from_raw(51200i32)` (200.0) — zero `ops::add`. Idiomatic physics
(`pos += vel`, `vy += gravity`, bounds checks) is now fast integer math. Only *literals*
fold; a runtime `f64 ⊕ fixed` (genuinely lossy) still boxes on the safe dynamic path.

**`fixed`-return hard-error FIXED.** A `fixed`-returning fn with local `Vec` ops was
classified as a native-vec fn (`fn f_nv() -> f64`) while its body yields
`Num<i32,8>` — a hard `rustc` type error (`VecRetKind` has no `Fixed`). Guard added
in **both** passes of `detect_native_vec_fns` (initial + forwarding fixpoint):
`if ann_is_simple(return_type, "fixed") continue`, so such fns stay on the correct
(boxed) dynamic path — matching how simple `fixed`-returning fns already behaved.
`f64`/`i32` return eligibility is byte-identical (guard only trips on `fixed`).

**Narrow integer widths FIXED.** `i8`/`u8`/`i16`/`u16`/`u32` are now first-class *storage*
types — their point is compact struct fields (a `u8` HP + two `i16` coords is 5 bytes in
scarce EWRAM, not 24). New `RustType::{I8,U8,I16,U16,U32}` + `is_integer_scalar()` /
`is_narrow_int()` helpers; `from_annotation` maps the names and all four conversion methods
handle them. The arithmetic model mirrors `i32`: a narrow read *promotes* to `f64` (JS
Number semantics), and a store *truncate-casts* back to the width. Wired through struct-field
writes (`emit_coerced_native`), local plain- and compound-assign fast paths
(`hp -= 3` → `hp = (((hp) as f64) - 3.0) as u8`, no box), and typed var-decl inits. Verified
on a 430 KB GBA ROM (`pub hp: u8, pub x: i16`; `ms[j].hp = (((ms[j].hp) as f64) - 10.0) as u8`)
and by interpreter↔native equality (276, 125). Narrow stores *saturate* (`f64 as u8`),
diverging from JS wraparound — the documented tradeoff of opting into a fixed width.

**`emit_f64` coercion bug FIXED (pre-existing).** `emit_f64`'s fall-through returned an
integer-scalar operand *raw*, so `return <i32 local>;` from a native-vec `-> f64` fn was a
hard `rustc` type error (E0308) — latent before, surfaced by the first narrow-int test that
returns a typed integer. It now widens integer scalars (`(x) as f64`) and `Fixed`
(`raw/256.0`), which also unblocked plain `i32`-returning functions with local `Vec` ops.

Remaining P4: native `Fixed` return (`VecRetKind::Fixed` + fixed struct-literal init);
runtime `f64 ⊕ fixed` (boxes today — wants a lift or a checker warning); throw-guard
elision; the GBA-mode unannotated-`number` default policy (integral→i32, fractional→fixed).
Regression status: 74 compiler + 63 core + 22 builtins tests pass; `bench-entities` ROM
(440 KB) rebuilds clean.

### Architecture spike result (tish vs Rust for the framework)

`examples/bench-entities` (100 entities, movement + O(n²) AABB collision, timed via
Timer2) confirmed by code inspection that **typed tish generates native
Rust-equivalent code** (`Vec<f64>`, native f64 math, no boxing in the hot loop) —
so for typed code, tish ≈ Rust in performance. Two fixable "tish taxes" surfaced:
per-statement `has_pending_throw()` guards after native array reads (elidable in
Gba mode), and grow-checks on `push`-built arrays. Conclusion: **framework + game
logic in tish** (adoption win, ~no perf cost for typed code); keep the type-heavy,
mutation-hot store in **Rust** for now; migrate down as `fixed` + struct-write
lowering land. Boot `bench.gba` (`cargo run --release` in its build dir) for the
`ticks_per_update` number (budget ≈ 4389 ticks/frame).

Next beyond P4: `asset:` pipeline for real sprites/backgrounds/audio; the
`tish-gba-game-engine` framework (components, scenes, genre modules); async frame API (P5).
Facade `array_*`/`string_*` method forwarders get added as examples use them.

## Environment (validated)

- nightly + `rust-src` present; stock agb 0.25 template builds for thumbv4t.
- `agb-gbafix` installed (`~/.cargo/bin`); `mgba-qt` + `mgba` present.
- Toolchain baseline copied into the workspace (`rust-toolchain.toml`,
  `.cargo/config.toml`, profiles) matches the agb template.
