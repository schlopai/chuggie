# CONTRACT — tish compiler ⇄ chuggie-engine framework

This file pins the interface between the two tracks so they move independently:

- **Compiler track** (in the tish repo `/Users/a_/Projects/tish/tish`): the `Gba`
  emit mode, the portable runtime facade `tishlang_runtime_gba`, typed Rust-scalar
  lowering, typed externs, the `asset:` import scheme, and async emission.
- **Framework track** (this repo): `tish-agb` bindings, `tish-gba-game-engine`, macros, the
  host asset importer, and the example games.

Any change to the items below is a breaking change and must be updated in lockstep
on both sides. Committed `.d.tish` files + a CI drift check enforce this.

Status legend: ✅ locked & proven in P0 · 🔷 locked by design · 🕓 lands in a later phase.

---

## 1. Generated-crate shape (what `--target gba` emits) 🔷

The `Gba` emit mode generates a `no_std` binary crate:

```rust
#![no_std]
#![no_main]
extern crate alloc;
use tishlang_runtime::Arc;   // = alloc::rc::Rc<T> (facade alias) — absorbs Arc::from/clone
// ...alloc imports, prelude `use tishlang_runtime::{...}`...

fn run() -> Result<(), Box<dyn core::error::Error>> { /* program body */ }   // 🕓 P5: async fn

#[agb::entry]
fn agb_main(gba: agb::Gba) -> ! {
    tishlang_runtime::gba::init(gba);   // stash peripherals for binding crates
    let _ = run();                       // 🕓 P5: tishlang_runtime::gba::block_on(run())
    tishlang_runtime::gba::halt()
}
```

- **Runtime crate name:** the Cargo dependency is `tishlang_runtime = { package =
  "tishlang_runtime_gba", path = ... }`. The rename makes every emitted absolute
  `tishlang_runtime::…` path resolve to the GBA facade with no codegen edits.
- **Panic/halt:** `gba::halt() -> !` loops on `agb::halt()`.

## 2. Prelude surface `tishlang_runtime_gba` must export 🔷

Name-for-name, everything `codegen.rs` emits in its `use tishlang_runtime::{...}`
list: `Value`, `VmRef`, `ObjectMap`, `ObjectData`, `PropMap`, `TishError`, the
math/json/uri/string/array/object/number/symbol/collection/typedarray helpers, and
`console_log` & co (→ `agb::println!`). Plus:

- `pub type Arc<T> = alloc::rc::Rc<T>;`
- `pub type Fixed = agb_fixnum::Num<i32, 8>;` (typed-numeric target type, §5)
- `pub mod gba` (§4).
- Capped-out capabilities (`fetch`/`serve`/`fs`/`http`/timers/ws/tty/pty) are
  `compile_error!` stubs so missing features read as "not available on GBA".

**Portable `Value` core (proven P0a):**
`Number(f64) | Str(Rc<str>) | Bool | Null | Array(Rc<RefCell<Vec<Value>>>) |
Object(Rc<RefCell<ObjectMap>>) | Function(Rc<dyn Fn(&[Value]) -> Value>) |
Promise(Rc<dyn TishPromise>) | Opaque(Rc<dyn TishOpaque>)`. Single-threaded `Rc`
(NOT `Arc`; `send-values` OFF). **`ObjectMap` hashes with
`rustc_hash::FxBuildHasher`** — NOT foldhash/ahash (both need atomics; see
`docs/findings/P0-findings.md`).

## 3. `cargo:` binding ABI (zero hand-written glue) 🔷

- Binding crates (`tish-agb`, `tish-gba-game-engine`) contain **no `Value` code**. Exports
  are idiomatic Rust fns marked `#[tish_export]` (a marker; expansion is the item
  itself, optionally `#[inline]`).
- The compiler's bindgen syn-scans `#[tish_export]` items and generates, into the
  ephemeral crate: (a) the boxed `Value::native` marshalling shim, (b) the
  `.d.tish` declaration, (c) a machine-readable signature sidecar.
- **Classifiable param/return types** at the ABI boundary:
  `i8 u8 i16 u16 i32 u32 bool`, `Fixed`, `&str`/`String`, `()`, `Option<T>` of
  those, and `Vector2D<Fixed>` (destructured to a `(x: Fixed, y: Fixed)` scalar
  pair). Anything else falls back to the explicit `fn(args: &[Value]) -> Value`
  convention, which crates MAY still hand-write where dynamic data is the point
  (e.g. `spawn(componentConfigs)`, `defineComponent(def)`).
- `#[tish_export(init)]` marks the `fn init(gba: &mut GbaShared)`-style entry the
  generated `agb_main` invokes for an imported module before `run()`.

## 4. `tishlang_runtime_gba::gba` module 🔷 / 🕓

- `init(gba: agb::Gba)` — stash peripherals (single-core, `critical-section`
  `RefCell`s). ✅ shape proven.
- `halt() -> !`. ✅
- Peripheral accessors for binding crates (graphics/input/mixer/timers/save/rng).
- **Hooks:** `hooks::register_pre_commit(fn)` — `tish-agb`'s frame driver and
  `tish-gba-game-engine`'s pipeline install here; run once per frame before `commit()`.
- **Executor** 🕓 P5: `block_on(fut)`, tish-level `spawn`/`cancel`, and
  `WakeCondition { Ready, NextFrame, AtFrame(u32), Buttons(u16), ChannelDone,
  Flag(FlagId) }`. All wakes at frame boundaries in spawn order ⇒ deterministic.

## 5. Numeric model 🔷

- Annotations may name concrete Rust scalars: `i8 u8 i16 u16 i32 u32 bool` and
  `fixed` (≙ `Fixed` = `agb_fixnum::Num<i32,8>`). Explicit `: number` = `f64`
  (soft-float; exact JS semantics).
- Unannotated `number` in Gba mode: integral-inferred → `i32`, fractional → `fixed`.
- `Value::Number` stays `f64` on the dynamic path (lossless `fixed`↔`f64` boxing;
  JSON/formatting unchanged).

## 6. Async 🕓 P5

Phase 1 (P2–P4): synchronous frame loop; `await` in Gba mode is a **compile error**
(not a silent sync fallback). Phase 2 (P5): tish `async fn` → Rust `async fn`,
`await` → `.await`, `Value::Promise(Rc<dyn TishPromise>)` bridges a
`Pin<Box<dyn Future<Output=Value>>>`. Framework awaitables: `video.frame()`,
`time.waitFrames(n)`, `input.buttonPress(mask)`, engine `tween`/`dialogue`/`walkTo`.

## 7. `asset:` imports 🕓 P3

`import { player } from "asset:gfx/sprites.aseprite"` — compile-time file check;
Gba codegen emits `include_aseprite!` / `include_background_gfx!` / `include_wav!`
into the generated crate and registers each imported name as an **`i32` handle**
with the binding crate's asset table before `run()`. Aseprite tags are animation
handles. The build step copies the project `assets/` tree into the build dir
(mtime-diffed) so proc-macro paths resolve.

## 8. ECS decision (P0c) ✅

`tish-gba-game-engine` uses a **custom SoA/archetype store**, not hecs (hecs's `spin` +
`foldhash` need atomics unavailable on thumbv4t). Component set is closed (§ engine).

---

## Workspace host/target split

`.cargo/config.toml` sets `thumbv4t-none-eabi` as the default target, so **plain
workspace members build for GBA**. Host-only tooling must NOT be a plain member:

- `tish-agb-macros` / `tish-gba-game-engine-macros` are `proc-macro` crates → cargo builds
  them for the host automatically. ✅ safe as members.
- The host LDtk importer (`tish-gba-assets`) is consumed **only** as a
  `[build-dependencies]` entry of example games → cargo builds it for the host.
  It must live outside the default GBA members (its own `tools/` workspace, or
  `[patch]`/path build-dep) so `cargo build` never tries to cross-compile it.

Until those crates exist, the workspace member list stays minimal.
