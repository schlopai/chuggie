# Memory & performance review — chuggie + the GBA tish compiler

**Date:** 2026-07-30 · **Scope:** `/Users/a_/Projects/chuggie` (runtime crates, `packages/*.tish`, examples) and
`/Users/a_/Projects/tish/tish` (portable/no_std core, GBA facade, boxing boundary, GBA codegen regions).
**Method:** 12 parallel static review dimensions with adversarial cross-verification, plus headless
libmgba soaks of 13 shipped ROMs. ~31k lines of in-repo review surface.

The single most important result is not a code reading: **three shipped ROMs died of heap exhaustion
under ordinary play, and one died of a compiler-emitted `panic!`.** All four are fixed — the decisive
one being a hidden-class registry the GBA build maintained but never read, worth **+24.6 KB of heap at
boot** ([S-13](#s-13)). Everything else in this report is context for why the heap was that tight.

---

## 1. Executive summary

### Crashes reproduced on current committed source

| ID | Severity | What | Evidence |
|----|----------|------|----------|
| [D-1](#d-1) | **P0** | the topdown RPG port (since moved to its own repo) OOMs after ~1–2 overworld screen transitions (≈64 frames of walking) — **FIXED**, see [S-13](#s-13) | fresh build, 3/4 directions, deterministic |
| [D-2](#d-2) | **P0** | `shop-demo` OOMs when the shop is opened/closed repeatedly (7,680-byte alloc fails) — **FIXED** | idle survives 900 frames; mashing dies at 261 |
| [D-3](#d-3) | **P0** | the shmup flagship panics in generated code — `panic!("expected boolean")` — **FIXED** | `packages/shmup.tish` passes `1` to a `bool` param |
| [D-4](#d-4) | **P0** | `bench-memory` — the repo's own leak gate — OOMs on its final trial — **FIXED** | every per-cycle trial reports `leaked 0` |

### Top static findings (verified by an independent second agent)

| ID | Severity | Finding | Verdict |
|----|----------|---------|---------|
| [S-1](#s-1) | **P0** | `native_sprite_destroy` orphans the arena slot for any *off-screen* sprite — never recycled | confirmed (re-verified by hand) |
| [S-2](#s-2) | **P1** | `frame()` shows backgrounds + stream layers + `ui_bg` with no cap → agb `panic!` on the 5th | confirmed |
| [S-3](#s-3) | **P1** | `uiRender`'s layout-only path never releases `LAYOUT.raw` → ~14–40K pinned past teardown | confirmed |
| [S-4](#s-4) | **P1** | World SoA columns are high-water-mark only; ~375 B × peak entities retained for the session | confirmed |
| [S-5](#s-5) | **P1** | the port's cave greeting writes HUD text slots nothing ever clears (OBJ VRAM held to power-off) | confirmed |
| [S-6](#s-6) | **P1** | `ui_palettes` caps at 15 and never resets — silently miscolours every later UI element | confirmed |
| [S-7](#s-7) | **P1** | Behaviour hook trampoline allocates a fresh `{this, dt}` object per hook per entity per frame | confirmed |
| [S-8](#s-8) | **P1** | `demoted_numeric_locals` is keyed by **bare name across the whole program** — one boxed `i` boxes every `i` | confirmed |
| [S-9](#s-9) | **P1** | No native call ABI on GBA: every user tish fn call is boxed (`value_call`) | confirmed |
| [S-10](#s-10) | **P1** | `LNode[]` typed-array read `.collect()`s a fresh 22KB Vec per `uiRender` | confirmed |
| [S-11](#s-11) | **P1** | PropIC inline caches compiled out on GBA — every `obj.field` is an uncached key scan | plausible |
| [S-12](#s-12) | **P1** | Entity-slot high-water mark taxes ~20 SoA passes/frame forever after one burst | confirmed |
| [S-13](#s-13) | **P1** | Hidden-class shape registry maintained on GBA but never read there — uncapped, no eviction | confirmed — **fixing this resolved D-1** |

88 findings survived verification in total. P2 and note-level items are tabulated in §6.

### The one-sentence diagnosis

The heap is not leaking *per frame* — the bench harness proves the cycles are flat. It is being
**consumed once, permanently, by first-touch retention** (dialog ~26K, sprite-text ~16K, UI layout
~14–40K, SoA columns, HUD slots, leaked palettes), until a game like the topdown RPG port has ~22K left at its first
room spawn and a **1,280-byte contiguous request fails**. The failure mode is fragmentation-at-the-margin,
not a runaway allocation.

---

## 2. Dynamic evidence

All runs are headless libmgba (`tools/gba-shot`) against ROMs built from committed source.

### <a id="d-1"></a>D-1 · P0 · the topdown RPG port OOMs after 1–2 screen transitions

```
== fresh-up   (30:a,45:,120:up)    → panic frame 185 · "memory allocation of 16 bytes failed"
== fresh-left (30:a,45:,120:left)  → panic frame 208 · "memory allocation of 16 bytes failed"
== dir-right  (30:a,45:,120:right) → panic frame 244 · "memory allocation of 64 bytes failed"
== dir-down   (30:a,45:,120:down)  → survives 600 frames  (blocked by terrain: no screen transition)
== nowalk     (30:a,45:)           → survives 900 frames
```

Rebuilt from source (`npm run build -w <the port>`, 7m06s) — the crash is live, not a stale artifact.
Walking is the trigger and the budget is ~63–65 frames of movement regardless of when it starts
(start at frame 120 → die at 185; start at 200 → die at 263). **Down survives because the start
screen blocks southward movement — no screen transition happens.** Every direction that crosses a
screen boundary dies within one or two transitions.

A temporary probe (since reverted) measured the margin at the first overworld room spawn:

```
[frame 117] PROBE spawnOW enter free=21952      (heap_free(64) — 64-byte blocks)
[frame 131] Error: memory allocation of 1280 bytes failed
```

**21,952 bytes free, and a 1,280-byte request fails.** That is the fragmentation signature the
`bench-memory` README describes. Note also that adding two `log()` probe strings was itself enough to
push the *first* spawn over the edge (uninstrumented, that spawn succeeds and the game dies at the
second transition) — the game ships with essentially zero allocation headroom.

### <a id="d-2"></a>D-2 · P0 · shop-demo leaks per open/close

```
== shop-idle (60:a,80:)                                    → survives 900 frames
== shop-mash (60:a,80:,120:a,140:,180:b,200:,240:a,...)    → panic frame 261
                                       "memory allocation of 7680 bytes failed"
```

Idle is clean; **more opens die sooner** (frame 321 on a slow schedule, 261 when mashed). This is the
dynamic confirmation of the per-open factory churn found statically ([S-3](#s-3), and the shop's
`makeSelector`/`makePointer` path).

### <a id="d-3"></a>D-3 · P0 · the shmup flagship (since moved to its own repo) panics in compiler-emitted code

```
[frame 353] Error: panicked at src/main.rs:1366:169
```

The generated line is an unconditional panic:

```rust
tish_gba_game_engine::anim_play_typed(…, match &Value::Number(1_f64) {
    Value::Bool(b) => *b, _ => panic!("expected boolean") });
```

Root cause — the ABI declares a `bool`, the caller passes an integer:

- `crates/tish-gba-game-engine/tish.d.tish:34` — `declare fn anim_play(e, from, len, speed, looping: bool): void`
- `packages/shmup.tish:303` — `anim_play(e, frame, alen, pick(o.animSpeed, 12), 1)`
- `packages/shmup.tish:259` — `anim_play(e, cfg.boomF, cfg.boomLen, cfg.boomSpeed, 0)`

`packages/cutscene.tish:34-35` passes proper `true`/`false`, so shmup is the outlier. **Any game using
`packages/shmup` crashes the first time an explosion or multi-frame animation plays.**

There are two defects here. The tish one is a one-word fix. The compiler one is worse: passing an
`i32` literal to a `bool` extern parameter **compiles cleanly and emits a guaranteed runtime panic**
rather than a type error. On a platform with `panic = abort` that turns a static type mismatch into a
field crash.

### <a id="d-4"></a>D-4 · P0 · bench-memory itself OOMs

The repo's own leak gate runs 11 trials, all reporting `leaked 0` per cycle, then dies:

```
[frame 21]  bench-memory: heap at start = 124928 (1K blocks), 125312 (64B blocks)
[frame 44]  == contract: 3/3 checks passed
...
[frame 263] == ui-render+clear   leaked 0 over 6 cycles   (free 72704)
[frame 333] == ui-render+hide    leaked 0 over 6 cycles   (free 70656)
[frame 409] == ui-bake           leaked 0 over 6 cycles   (free 69632)
[frame 510] == dialog            leaked 0 over 6 cycles   (free 43008)   ← −26.6K first touch
[frame 523] == sprite-text       leaked 0 over 6 cycles   (free 26624)   ← −16.4K first touch
[frame 534] [failed]
[frame 536] Double panic: memory allocation of 96 bytes failed
```

The last trial (`component` — a `makeSelector` menu built and dropped per open) never completes. This
is the clearest statement of the whole problem: **the cycles are flat, the baseline is not.** Free
heap ratchets 124.9K → 26.6K purely through first-touch retention, and then a 96-byte allocation fails.

### Clean under the same treatment

`akari`, `sunny-land`, `dark-hero`, `dialog-demo`, `platformer-combat`, `overworld-demo`, `rpg-menu` all
survive 1,200 frames of scripted input. The isoboard SRPG example survives **22,000 frames** across two runs
(10,000 idle + 12,000 with driven combat) with no panic and no visual corruption.

---

## 3. P0 findings (static)

### <a id="s-1"></a>S-1 · P0 · `native_sprite_destroy` orphans off-screen sprite slots

**Location:** `crates/tish-agb/src/lib.rs:1020` · engine call sites `crates/tish-gba-game-engine/src/lib.rs:2358, 3753, 3843, 878`

`native_sprite_destroy` only recycles the arena index inside `if s.object.take().is_some()`. But the
engine deliberately puts sprites into the `object == None` state:

- `attach_sprite`/`attach_sprite_typed` call `native_sprite_release(h)` the instant a sprite attaches,
- `render_system` re-releases any entity that is not `is_active` (i.e. off-screen).

`native_sprite_release` sets `s.object = None` (VRAM freed, slot still live). So when `despawn()` calls
`native_sprite_destroy` for an entity that is **currently off-screen or never was on screen**,
`take()` returns `None`, the index is never pushed to `ctx.sprite_free`, and the slot is dead weight
forever. `sprite_alloc` then appends a fresh slot for the replacement.

*I verified this one by hand independently of the finder agent* — the `release` → `object = None` →
`destroy` → `take().is_some() == false` path is exactly as described.

**Cost:** ~36–40 B leaked per off-screen despawn, unbounded within a scene. The CPU cost bites first:
`frame()` scans the whole sprite arena twice per frame, so 600 orphans ≈ 1,200 extra iterations/frame
≈ 12k cycles ≈ 4% of budget, growing linearly. Reachable today: akari's `parkRoomLive()`
(`examples/akari/src/main.tish:181-191`) despawns every non-player entity of the room the player just
left — by construction those are off-screen.

**Fix:** recycle the slot whenever the handle names a live entry, not only when it still owns an
Object; or record `released: true` on `SpriteData` so `destroy` can distinguish released-but-live from
already-freed.

---

## 4. P1 findings

### <a id="s-2"></a>S-2 · `frame()` can exceed agb's 4-background limit → abort

**Location:** `crates/tish-agb/src/lib.rs:4035-4052`

`frame()` shows every visible `ctx.backgrounds` entry, then `stream_active` stream layers, then
`ui_bg` — with no count check. agb panics on the 5th (`Can only have 4 backgrounds at once`,
`agb-0.25.0/src/display/tiled.rs:349`), which under `panic = abort` is unrecoverable. The layer count
is data-driven: the scenepack writes `render_layers.len()` unclamped (`tish-gba-scenepack/src/tiled.rs:317`).
`ninja-adventure/assets/village.tmj` already has 4 render layers and survives only because it never
opens a UI canvas. Since `ui_hide` deliberately keeps `ui_bg` alive, opening a dialog in a 3-layer
dungeon puts you at exactly 4 — one artist-added Tiled layer away from an abort.

**Fix:** cap the shown-layer count in `frame()`, and clamp/emit a build-time error in the scenepack so
the failure surfaces at bake time.

### <a id="s-3"></a>S-3 · `uiRender` layout-only path never releases `LAYOUT.raw`

**Location:** `packages/ui.tish:956-962` (early return) vs the release at `:977`

The immediate-paint path clears `RAW.length = 0` with a comment explaining exactly why ("a shop tab is
~40K of node objects… the tab a player had just left made the keeper's dialog fail to allocate"). The
layout-only path (`STREAM.lo === 1`, taken by **every** `uiBake` and every `uiStreamBegin`) copies into
`STREAM.raw` and returns **before** that release. `LAYOUT.raw` keeps a second reference to every boxed
node until the next full `uiRender` — and `dialogFree`/`uiBakeFreeAll`/`releaseTab`/`loadScene` never
touch it. In akari-style games that drive all boxes through the replay path, no full `uiRender` ever
runs again during gameplay.

**Cost:** ~14–20K held indefinitely; transiently up to ~40K after a streamed shop tab — landing
precisely in the documented crash window. **This is the most likely direct cause of [D-2](#d-2).**

**Fix:** add `RAW.length = 0` in the `STREAM.lo` branch after copying into `STREAM.raw` (a separate
array, so the aliasing concern in the neighbouring comment does not apply).

### <a id="s-4"></a>S-4 · World SoA columns are high-water-mark only

**Location:** `crates/tish-gba-game-engine/src/lib.rs:654-679` (growth), `:892-912` (`clear_world` never truncates)

`spawn()` pushes onto 23 parallel column Vecs when the free list is empty; `despawn`/`clear_world` only
flip `alive[]` and push to `free`. Column length is therefore the **session-wide peak concurrent
entity count**, retained across every later scene including menus and title screens.

**Cost:** ~375 B × peak entities (a 120-slot bullet-hell burst ≈ 44K of 136K), plus Vec-doubling
fragmentation during growth.

**Fix:** truncate in `clear_world` (where every slot is dead by construction) — but preserve `gen`
entries or fold a global epoch into `encode()`, or a stale tish-held id could alias a recycled slot.

### <a id="s-12"></a>S-12 · Entity-slot high-water mark taxes every frame

**Location:** `crates/tish-gba-game-engine/src/lib.rs:4024` (`world_step`)

Same root as S-4, different cost axis: ~20 systems iterate `0..self.alive.len()` every frame. The
per-slot filter is ~15–20 cycles on EWRAM, so ~300–400 cycles per *dead* slot per frame. At an 80-slot
mark that is ~24–32k cycles/frame — **8–11% of the frame budget spent skipping dead entities**, forever
after one dense room.

**Fix:** track a `live_max` watermark and iterate `0..live_max`, or maintain a dense `Vec<u32>` of live
slots.

### <a id="s-5"></a>S-5 · the port's cave greeting pins HUD text slots for the session

**Location:** the port's `src/main.tish:377` (`greetCave`), `:565-566` (L9 refusal)

Commit `46c3395` replaced `entity(pid).say(...)` with `hudText(20+i, ...)`. Each `hud_text` slot builds
one agb `Object` per 32px glyph group backed by `DynamicSprite16` in OBJ VRAM — and the type's own doc
comment states Objects are kept so sprite VRAM stays allocated. `loadScene` calls
`clear_world`/`sprite_clear`/`ui_clear`/`bg_clear`, **none of which touch `ctx.hud_text`**. The only
`""` clears are in `title_boot.tish` and `attract.tish` — and `attract.tish` is imported by nothing.

**Cost:** "IT'S DANGEROUS TO GO ALONE! / TAKE THIS." ≈ 7 glyph groups ≈ 3.5K of the 32K OBJ VRAM (~11%)
plus ~7 OAM entries, held from the first cave to power-off. Also a visible bug — the cave line stays
painted over the overworld and every dungeon.

**Fix:** clear slots 20–23 on scene exit, or add `hudTextClearRange(lo, hi)` and call it from
`loadScene`. Do **not** add a blanket `hud_text` wipe to `sprite_clear` without auditing games that
intentionally keep a slot across a load.

### <a id="s-6"></a>S-6 · `ui_palettes` caps at 15, never resets, fails silently

**Location:** `crates/tish-agb/src/lib.rs:1696`

`ensure_ui_palette` allocates 1..15 in the shared bank; at exhaustion it returns the **last registered
index** instead of failing or evicting. `ui_palettes` is only ever pushed to — `ui_begin`, `ui_hide`
and `ui_clear` do not reset it.

**Cost:** no memory cost (~120 B). The failure is user-visible, permanent and silent: from the 16th
distinct colour on, every new-coloured UI element paints in the wrong colour for the rest of the
session, with no diagnostic.

**Fix:** reset in `ui_clear`; make the overflow branch loud and surface a counter in `ui_mem_report()`.

### <a id="s-7"></a>S-7 · Hook trampoline allocates a ctx object per hook per entity per frame

**Location:** `packages/engine.tish:339-348` · root cause `codegen.rs:3602-3607`

`comp.update({ this: s, dt: dt })` builds an object literal per invocation. On GBA `cached_object_key`
deliberately skips `OnceLock` interning and emits bare `Arc::from("this")` / `Arc::from("dt")`, so
every key is a fresh `Rc<str>`; `object_from_pairs` then allocates the `Rc<RefCell<ObjectData>>`
(~180–220 B). One of the port's AI rooms with 4–6 hooks does 12–18 transient allocs (~1 KB traffic) per frame —
~60 KB/s cycled through a 136K heap. No net growth; the cost is allocator cycles and fragmentation
pressure. The `tick:` fast path avoids this, but the port's shipped AI is on `update`.

**Fix:** pool the ctx objects like the entity wrappers (one `{this, dt}` and one `{this, other}` per
hook-nesting depth, rebound via `set_prop`). Compiler side: implement no_std key interning.

### <a id="s-8"></a>S-8 · `demoted_numeric_locals` is keyed by bare name program-wide

**Location:** `codegen.rs:11993` (collector), `:5414` (consumer)

`collect_demoted_numeric_locals` builds a flat `HashMap<String, RustType>` keyed by **bare name over
the entire bundled program**, then demotes a name to `Value` if *any* reassignment of that name
*anywhere* has a non-native RHS. `VarDecl` checks the same bare name — so one boxed `i` in one module
boxes **every** local called `i` in every function, and the fixpoint cascades.

**Cost:** ~40–60 cycles per boxed use vs ~2–4 native. The engine authors already measured this
(`packages/ui.tish:801-802`): de-annotating just two counters in `uiRender` was worth measuring and
commenting on.

**Fix:** key by `(enclosing function, name)` — `collect_reassignments_stmts` already walks statements,
so the owning `FunDecl` is available.

### <a id="s-9"></a>S-9 · No native call ABI for user functions on GBA

**Location:** `codegen.rs:3057-3062` (the `emit_mode != Gba` gate), `:23756-23794` (M5 emission)

On GBA the only native call targets are typed externs and `Math.*`. Every call to a user tish function
falls through to `value_call`: callee cloned twice, every argument boxed, callee re-unboxes, result
boxed and re-unboxed. The M5 pass that would emit direct calls is disabled for GBA — and the gate's own
comment marks it "re-audit for no_std later", i.e. unfinished work, not a design decision.

**Cost:** ~120–200 cycles of pure call overhead per user-fn call plus per-argument boxing.

**Fix:** split the gate — only the `thread_local!`-using passes need the no_std rewrite;
`emit_native_fns` itself does not.

### <a id="s-10"></a>S-10 · Typed-array read materialises a fresh Vec every `uiRender`

**Location:** `tish_compile/src/types.rs:514-520`; reached from `packages/ui.tish:667, 686`

`RustType::Vec::from_value_expr` has no by-reference form — reading a boxed array into a typed local
always `.collect()`s a fresh owned `Vec`, cloning every element. `uiRender` binds
`let LN: LNode[] = LAYOUT.ln` at entry, so each render starts from an **empty** Vec and grows it by
doubling, then drops it at exit. The module-level pool `LAYOUT.ln` is never written back — so the pool
is permanently empty and the "pool" does nothing.

**Cost:** for the engine's own documented n=142 nodes: 7 heap allocations, a **22,528-byte peak
contiguous block**, ~44.7 KB cumulative requested and ~22 KB memcpy'd, per render, on a 136K heap where
a 20K growth step is already a documented failure point.

**Fix:** give `RustType::Vec` a borrowing read form for the `let LOCAL: T[] = <boxed module field>`
shape, or have codegen reuse a persistent buffer for a typed-array local that is only mutated in place.

### <a id="s-11"></a>S-11 · PropIC inline caches compiled out on GBA

**Location:** `codegen.rs:8902-8911`

`PropIC` is built from `std::sync::atomic`, unavailable on thumbv4t, so GBA emits plain `get_prop`:
key compare, RefCell borrow, PropMap lookup (FxHash + IndexMap probe, or a linear `Arc<str>` scan),
then a Value clone. ~100–150 cycles per boxed property read; at a conservative 100 reads/frame that is
~4–5% of budget, far higher on UI relayout. Compounded by `ArcStr` having **no interning**, so key
comparison is a full string compare.

**Fix:** the blocker is the atomics, not the algorithm — a `Cell<u64>`-based PropIC behind
`#[cfg(feature = "portable")]` is sound on a single-core target.

---

## 5. Notable correctness / hygiene items

| Finding | Location | Note |
|---|---|---|
| `defs` registry appends unconditionally, `clear_world` never clears it; first-match lookup | engine `lib.rs:686-708, 892` | a game that re-`mount()`s per scene grows it forever; `shmup.tish` guards with `REG.done`, `engine.tish mount()` does not |
| `sprite_clear` invalidates every tish-held handle with no generation counter | tish-agb `lib.rs:3413` | `packages/shop`'s `PTR` is created once and never reset — latent sprite-state corruption in the first warping game that embeds the shop |
| `bg_clear` reuses background handles with no generation counter | tish-agb `lib.rs:2720` | stale handle silently aliases a new layer |
| OAM overflow silently drops the tail past 128 objects | tish-agb `frame()` | no budget check, no diagnostic |
| Text/bar palette caches `Box::leak` per distinct style, uncapped | tish-agb `lib.rs:1183, 1367` | bounded by style diversity, not time — but programmatic colour variation leaks forever |
| `tw_cache` evicts by wholesale clear at 96 entries; every lookup clones the String key | tish-agb `lib.rs:2606` | clear-thrash with dynamic labels (score counters) |
| `IsoBoardGrid::init` discards and reallocates all five per-cell buffers per board load | engine `lib.rs:4218-4231` | works against the fragmentation discipline everywhere else |
| `run_pre_commit`/`register_pre_commit` are dead code | `tish_runtime_gba/src/gba.rs:50-55` | the per-frame `Vec<fn()>` clone is real but never reached |
| `fn last()` hands out an unbounded-lifetime `&'static mut` to a `static mut` | tish-agb | UB-adjacent; no current aliasing repro |
| `sio_send` indexes `args[0]` unguarded | `crates/tish-agb-sio` | abort on a 0-arg call; crate is not wired into any example |
| `verify.sh`'s dungeon7 test asserts only "did not panic" | the port's `verify.sh` | **and it greps only for panic strings — a ROM that dies into `Jumped to invalid address` passes.** This is why D-1 went unnoticed |
| the SRPG example's docs still describe the removed height mechanic | `formula.tish` header, `components.tish` AI weights | stale after the height-removal commit |
| Generated build dirs fork per `-o` output stem | tish build driver | `examples/*/.tish` measured in the tens of GB |

**Refuted / excluded:** findings that reduced to the known-deliberate policies (never-drop stream
layers and backgrounds, pooled entity wrappers, top-level function Rc cycles, `#556`/`#558` residuals,
facade-vs-JS semantics) were dropped during verification. One finding — a per-8-frame `heap_free`
probe "baked into" the port's build — was **my own temporary instrumentation** being read by an agent
mid-review; it has been reverted and is not a real finding.

---

## 6. Long tail

37 P2 and 36 note-level findings survived verification. Recurring themes, each with concrete sites:

- **Transient per-frame Vecs** in `combat_system`, `life_system`, `collect_deaths`, `collect_collisions`
  where the same file already uses a reusable `buf_*` pattern.
- **String churn on hot paths**: `to_display_string()` allocates a `String` + a cycle-guard `Vec` per
  call; `ui_text_span` re-copies the whole page string every frame during a typewriter reveal purely to
  compare it against a cache key; `hud_text` allocates the string copy and colours Vec *before* the
  cache comparison; `text_width`'s memo clones its String key even on a hit; `shapeKey` builds its key
  from 19 nested string adds.
- **`typeof x === 'literal'` guards** on per-frame paths — 2 uninterned `Rc<str>` + a Value clone per
  evaluation (`engine.tish:111-580`, `shmup.tish:94-102`). `ui.tish:176` already stripped this and
  documents why; the fix was never propagated.
- **Full-slot scans per entity**: `topdown_system` runs two blocker scans per moving entity per frame;
  `collect_collisions` re-evaluates its ~45-cycle `is_active` predicate per pair.
- **Object/closure allocation per action**: per-shot opts literals, per-open `makeCursor`/`makeSelector`
  closure sets, `Value::from_struct_ref` allocating a fresh `Rc<RefCell<T>>` *and cloning the struct* on
  portable, read-only closure captures each getting their own `VmRef` cell.
- **Fixed↔Value crossings route through f64** on an FPU-less ARM7TDMI.

---

## 7. Clean areas

Reviewed and judged sound:

- **Sprite VRAM recycling** (the previously fixed free list) works — the defect is only the orphaned
  *arena slot* (S-1), not VRAM.
- **`room_transition_system`** is allocation-free.
- **`push_stream_layer`** correctly reuses dormant slots rather than reallocating, as designed.
- **`save_api.rs`**'s packed SRAM blob — the rejection of agb's `SaveSlotManager` (heap Vecs of free
  sectors) is well-reasoned and holds up.
- **Entity wrapper pooling** — the discipline holds; wrappers are bounded and the contract assertions
  in `bench-memory` pass 3/3.
- **`map_stream`** keeps map data in ROM with no EWRAM copy.
- **the isoboard SRPG example** — 22,000 frames of driven combat, zero leak signal.
- The **`unsafe impl Sync`** shims are justified for the single-core cooperative model as far as current
  interrupt usage goes (audio DMA does not touch interpreter state).

---

## 8. Reproducing the crashes

```bash
# Filtered soak (drops mGBA's DMA/SWI spam and collapses the post-crash opcode storm —
# an unfiltered 8000-frame crashed run wrote a 20 GB log)
GBA_SHOT_LOG=1 GBA_SHOT_TRACE=1 tools/gba-shot <rom.gba> /tmp/out.ppm <frames> "<key schedule>" 2>&1 \
  | grep -vE 'Starting DMA|SWI:' | awk '/Illegal opcode|Jumped to invalid/ {if(!d){print "FIRST-DEAD: "$0;d=1};next} {print}'
```

| Case | Command |
|---|---|
| the port's OOM | `… <the port>.gba out.ppm 600 "30:a,45:,120:up"` → panic frame 185 |
| shop-demo OOM | `… shop-demo.gba out.ppm 900 "60:a,80:,120:a,140:,180:b,200:,240:a,260:,300:b,320:,360:a,380:"` → panic frame 261 |
| the shmup flagship panic | `… <the shmup flagship>.gba out.ppm 1200 "60:a,80:,140:a,160:,220:start,240:"` → panic frame 353 |
| bench-memory | `npm run build -w bench-memory && … bench-memory.gba out.ppm 6000` → `[failed]` + OOM at the `component` trial |

---

## 9. Fixes applied in this pass

Applied and compiling (`cargo check --release` clean on the modified crates):

| Finding | Change |
|---|---|
| [D-3](#d-3) | `packages/shmup.tish:259,303` — pass `false`/`true` to `anim_play`'s `looping: bool` instead of `0`/`1`. A scan of every `bool`-typed extern parameter against every call site in `packages/` and `examples/` confirms these were the only two mismatches. |
| [S-1](#s-1) | `crates/tish-agb/src/lib.rs` — `native_sprite_destroy` now recycles on a new `SHEET_FREED` (-2) sentinel instead of on `object.take().is_some()`, so a sprite despawned while off-screen returns its arena slot. Existing `sheet >= 0` guards skip freed slots for free; double-free stays harmless. |
| [S-3](#s-3) | `packages/ui.tish` — the `STREAM.lo` (layout-only) early return now does `RAW.length = 0` after copying into `STREAM.raw`, matching the immediate-paint path. |
| [S-6](#s-6) | `crates/tish-agb/src/lib.rs` — `ui_clear` resets `ui_palettes` (no live tile references an index once the canvas is torn down), and overflow past 15 now increments `ui_pal_overflow`, reported by `ui_mem_report()` as `pal N/15 palovf M`. |
| [S-2](#s-2) | `crates/tish-agb/src/lib.rs` — `frame()` budgets the 4 hardware background slots (UI canvas reserved first, then map/stream layers) instead of calling `show()` a 5th time and letting agb abort. |

### <a id="s-13"></a>S-13 · P1 → the fix that resolved D-1 · the shape registry is write-only on GBA

**Location:** `/Users/a_/Projects/tish/tish/crates/tish_core/src/shape.rs`, driven from
`tish_core/src/value.rs:842, 851` (`PropMap::insert`)

`PropMap::insert` calls `shape::transition` for every new key, with no cfg gate, so it runs on GBA.
`transition` takes a global lock, hashes the key, and on a miss **permanently** pushes a `ShapeNode`
and inserts an `Arc::clone(key)` into the parent's edge map. `shape.rs` contains no remove, clear,
shrink or eviction of any kind.

The registry exists only to feed inline caches — and the only readers of `.shape()` in the entire
tree are `tish_runtime/src/lib.rs:1331` and `tish_vm/src/vm.rs:3894,3937,3954`. **Neither is linked
into a GBA ROM** (the GBA facade's `get_prop` goes straight to `PropMap`). So on GBA it was pure
cost: a lock plus a hash on every object construction, and permanent heap for every distinct key path.

**Fix applied:** under `feature = "portable"`, `transition` returns `DICT_SHAPE` — the sentinel the
file already defines for "opted out of shape tracking, never matches an inline cache" — and the
`Registry`/`ShapeNode`/`registry()` items are `cfg`'d out entirely. Any future reader degrades to the
slow path rather than trusting a slot index that was never recorded. The host (non-portable) build is
untouched and still compiles.

### Fix verification (rebuilt ROMs, same soaks as the original repros)

| Case | Before | After |
|---|---|---|
| **the port, walk up** | OOM at frame 185 | **900 frames clean** |
| **the port, walk left / right** | OOM at 208 / 244 | **900 frames clean each** |
| **the port, 6,000-frame wander (~64 direction changes, many screen transitions)** | died in ~1 s of walking | **clean, 2,302 painted frames**, screen renders correctly |
| the shmup flagship | panic at frame 353 | **1,500 frames clean**, 1,259 painted |
| shop-demo, original repro schedule | OOM at frame 261 | **900 frames clean** |
| shop-demo, 3,000-frame open/close stress (~70 cycles) | OOM at frame 319 even after the `LAYOUT.raw` fix | **clean** with the shape fix as well |
| **bench-memory** (the repo's own gate) | `[failed]` + OOM on the `component` trial | **all 12 trials `leaked 0`, contract 3/3, run completes** |
| bench-memory boot heap | 124,928 B (1K blocks) | **149,504 B — +24.6 KB recovered** |
| akari, 2,000-frame scripted play | clean | clean (no regression) |
| host `cargo test -p tishlang_core` | 22 passed | **22 passed** (the change is `cfg`-gated to portable) |

The `+24.6 KB` line is the headline number: it is ~18% of the 136 KB heap, recovered at boot, and it
lands squarely in the 25–40 KB the static review estimated for the shape registry. It is also what
turned three separate OOMs into passes — the port, shop-demo and the bench harness were all failing for
want of headroom rather than from a runaway per-frame leak, exactly as the diagnosis in §1 predicted.

**D-1 is resolved by S-13.** The tish-agb fixes alone did not move it (the port still died at 185 with
those applied) — the heap simply had no headroom until the shape registry stopped consuming it, which
is consistent with the ~22K-free / 1,280-byte-failure measurement.

A visual check of the post-fix wander screenshot incidentally **confirms [S-5](#s-5)**: leftover HUD
text from the title screen is still painted over the overworld map, exactly as that finding predicts.

**shop-demo still leaks per open.** The `LAYOUT.raw` pin was real and its release bought roughly one
extra open/close cycle, but it is not the dominant retainer on that path. The remaining suspects are
the ones the review flagged in the same area and that this pass did not touch: `makeSelector` building
its ~22 closures per tab entry, `makePointer` calling `sprite_new` with no matching destroy, and
`dialog.tish`'s `CHROME` cache. `packages/shop.tish` is being edited concurrently, so it was left
alone rather than patched underneath in-flight work.

**Not applied — [S-5](#s-5) (the port's cave-greeting HUD slots).** The fix is a `clearGreetSlots()` helper
(blank slots 20–23) called at the top of `enterScene`. It is left out because the port's example tree is
being edited concurrently; applying it now would collide with in-flight work.

Deliberately not attempted here — each needs a design decision rather than a patch: S-4/S-12 (SoA
truncation needs a generation/epoch scheme or a stale tish-held id could alias a recycled slot), S-7
(ctx-object pooling), S-8/S-9/S-10/S-11 (compiler changes in `/Users/a_/Projects/tish/tish`).

---

## 10. A note on the verification gate

 The port's `verify.sh` greps only for `Double panic|memory allocation|panicked at`.
A ROM that dies without printing those (jumping to an invalid address instead) passes the gate while
being completely dead. Any future gate should also fail on `Illegal opcode` / `Jumped to invalid address`
and assert that the screen is still painting at the end of the run.
