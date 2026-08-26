# BENCH BOOT

> *A benchmark testing the performance of the boot subsystem.*

The isoboard SRPG example shows its first pixel on **frame 70** (~1.2s). `akari` takes **465** (~7.8s), the topdown RPG port
380, `sunny-land` 548. That is a black screen for the length of the GBA BIOS intro, twice over, and
it is the first thing a player experiences.

It turned out to be one bug in the tish compiler, fixed below. What it was worth (`./run.sh --games`,
first frame the picture changes):

| example | before | after | |
|---|---|---|---|
| akari | 465 (7.8s) | **39 (0.65s)** | 12x |
| the topdown RPG port | 380 (6.4s) | **59 (0.99s)** | 6.4x |
| ninja-adventure | 287 (4.8s) | **34 (0.57s)** | 8.4x |
| dark-hero | 347 (5.8s) | **94 (1.6s)** | 3.7x |
| platformer-combat | 419 (7.0s) | **162 (2.7s)** | 2.6x |
| sunny-land | 548 (9.2s) | **261 (4.4s)** | 2.1x |
| the isoboard SRPG example | 70 (1.2s) | **26 (0.44s)** | 2.7x |

With both findings in, and every ROM rebuilt, **nothing in the repo boots slower than one second**.
The slowest are `sunny-land` at 60 frames (a 102×15 streamed map) and `rpg-menu` at 53; the median
example paints around frame 30. `dark-hero` briefly looked like a third outstanding case at 94
frames, and was simply a stale ROM — it rebuilds to 35.

(`platformer-combat` and `sunny-land` kept a second or more of their own startup work on top. That got
attributed too — see **The second finding** below — and it turned out to be one shared function.)

## What "fast" actually is: the pure-agb floor

A hand-written agb ROM in Rust boots **instantly**, and until that is on the page none of the numbers
above mean anything — 30 frames is either excellent or terrible depending on what was available.
`agb-floor/` is that reference: a pure-agb ROM, no tish, doing exactly what `floor.tish` does (set
backdrop `0x101018`, then present frames). Keep the two in step or the subtraction below is void.

```bash
cd examples/bench-boot/agb-floor && unset CARGO_TARGET_DIR && cargo build --release
agb-gbafix target/thumbv4t-none-eabi/release/agb_floor -o agb-floor.gba
```

| ROM | size | first paint | |
|---|---|---|---|
| `agb-floor` — pure agb, no tish | 153K | **1** | 0.02s |
| `floor.tish` — tish runtime, no assets | 476K | **5** | 0.08s |
| the isoboard SRPG example | 1.9M | 39 | 0.65s |
| the topdown RPG port | 4.1M | 30 | 0.50s |
| `sunny-land` | 2.6M | 60 | 1.01s |

**Starting the tish runtime costs 4 frames — 67ms.** That is the whole price of the language at boot,
and it is small enough that no game's startup is explained by it. The other 25 to 55 frames are the
game's own: registering assets, loading a scene, building a world. So the question this bench exists
to answer is never "is tish slow to start" — it is *which of a game's own boot stages is expensive*,
which is what the staged markers in `main.tish` are for.

It also sets the ceiling on what any future work here can win. A game paying 30 frames has at most 26
to give back, and the last 4 are not for sale.

## The second finding: marking collision in tish

The games that stayed slow after the import fix were the ones whose map is a **tish literal** rather
than a `scene:` import. They are the four slowest examples in the repo, and their boot time is very
nearly a straight line in how many tiles the map has:

| example | tile literals | before | after |
|---|---|---|---|
| platformer-rooms | 610 | 94 | **32** |
| platformer-scroll | 647 | 98 | **32** |
| platformer-combat | 923 | 162 | **42** |
| sunny-land | 1,567 | 261 (4.37s) | **60 (1.01s)** |

≈0.175 frames per map tile, with an intercept near zero — i.e. the map *was* the boot. Splitting
`loadStreamMap` on sunny-land (102×15 = 1,530 tiles) says exactly where:

```
STAGE                       AT    COST
modules-done                13      13   <- building the 1,567-value literal is cheap
setupGrid                   14       1
tilemap_stream              30      16   <- rendering all 1,530 tiles, natively
collision loop             248     218   <- marking them solid, in tish
```

**Rendering every tile cost 16 frames; marking their collision cost 218** — 3.65 seconds, about
40,000 CPU cycles per cell. The loop did four property lookups, two function calls and up to two
native crossings per cell, all interpreted.

This was already known and already fixed *for ROM maps*: `grid_from_map` exists precisely because
"the tish equivalent was a w×h interpreter loop with two native calls per cell, which dominated area
load time". The literal-map path never got the same treatment, so `loadStreamMap` and `loadMap` now
call **`grid_from_gids(width, height, data, solid, oneway)`** — the same work, one crossing per array
instead of per cell.

Worth knowing if you touch it: three of the five maps have **no `oneway` field at all**, so the
native side is handed a null and has to read it as an empty list. The tish helper it replaced opened
with `if (!list) { return false }`; dropping that guard cost three examples their entire screen, and
a pending throw on a null property presents as a **white screen**, not as an error.

## Measure freshly-built ROMs

The first pass of this investigation "found" seven slow examples. Five of them were **stale binaries**
— `platformer-rooms` and `overworld-demo` were built five days earlier, before the import fix landed.
Rebuilding took `beatemup` from 273 frames to 22 and `overworld-demo` from 140 to 24 with no source
change at all. A checked-in `.gba` is a snapshot of whatever the engine looked like that day, so
rebuild before you attribute anything, or you will go hunting for a bug that was fixed a week ago.

Every intuition about why is wrong, which is what this bench exists to prove:

| guess | why it's wrong |
|---|---|
| ROM size | akari is 4.5MB and boots in 7.8s — but `shop-demo` is 2.1MB and boots in **0.8s** |
| the `packages/` UI stack | `ui-demo`, `shop-demo` and `rpg-menu` import all of it and boot under 1.2s |
| map size | akari's boot scene is 3,600 tiles; the topdown RPG port's is 38,400 and boots **faster** |
| the scene stream / collision grid | streaming that port's 38,400-tile overworld costs **10 frames** |
| asset registration | 14 sprite sheets, 2 scenes and a font together cost **0 frames** |

## The answer

**Every named `cargo:` import rebuilds the crate's entire export table, once per imported name.**

The compiler emits this, per imported symbol:

```rust
let spawn = { let _ns = crate::generated_native::cargo_native_tish_gba_game_engine_object();
              match _ns { Value::Object(ref _o) => _o.borrow().strings.get("spawn").cloned()... } };
```

`cargo_native_..._object()` builds an `ObjectMap` containing **every** function the crate exports —
68 `Arc<str>` keys and 68 native closures — then one key is read out of it and the rest is thrown
away. Import 68 names and it runs 68 times: **4,624 allocations to bind 68 functions.** The cost is
quadratic in (names you import × names the crate exports), and it is paid before your first statement.

`packages/engine.tish` imports 68 symbols from `tish_gba_game_engine` and 49 from `tish_agb`:
**7,417 wasted inserts, 3.9 seconds**, and every game built on the engine pays it to boot.

The measurement, from `./run.sh` (frames at 59.7fps, cost charged against the import-free floor):

```
VARIANT                        AT     COST   SECONDS      after the fix
floor (no imports)              4        4      0.07              4
font                            4        0      0.00              4   <- assets were always free
sheets (14)                     4        0      0.00              4
scenes (2)                      4        0      0.00              4
native (3 named imports)        4        0      0.00              4   <- 3 imports: free
named68 (68, same crate)      184      180      3.02              7   <- 68 imports: 3 seconds
ui                             21       17      0.28              6
dialog                         26       22      0.37              7
engine                        261      257      4.30             11   <- ui+dialog+117 cargo imports
```

`native` vs `named68` is the whole finding in two rows: **same crate, same everything, 3 imports
versus 68, and the only difference is 3 seconds.**

Two facts that were checked so the diagnosis can't be dodged:
- The native crate itself is not slow to start (`v_native`, 3 imports, is free).
- `packages/engine.tish` has **no top-level statements at all** — markers placed at four points
  through its body all landed on the same frame. The time is spent binding its imports, before its
  first line runs.

There was **no workaround at the game's end**: `import * as E from 'cargo:...'` is rejected by the
compiler ("Namespace import (* as E) not supported for native module"), so a module cannot take the
namespace once and index it.

## The fix (tish repo, `tish_compile/src/codegen.rs`)

`emit_native_namespace_preamble` binds each native module's namespace object **once**, to a
`run()`-local, before any import reads from one; `native_module_rust_init` then reads the key out of
that local instead of constructing a fresh namespace per import. O(imports × exports) becomes
O(imports + exports). The generated preamble is two lines:

```rust
#[allow(unused_variables)] let __tish_native_ns_0 = crate::generated_native::cargo_native_tish_agb_object();
#[allow(unused_variables)] let __tish_native_ns_1 = crate::generated_native::cargo_native_tish_gba_game_engine_object();
...
let spawn = { match &__tish_native_ns_1 { Value::Object(ref _o) => _o.borrow().strings.get("spawn").cloned()... } };
```

Locals are numbered rather than named after the spec because `cargo:a-b` and `cargo:a_b` sanitize to
the same identifier, and binding one module's namespace under another's name would be a silent bug;
they are emitted in sorted order so the generated Rust is byte-identical across builds.

The engine bench ROM went from **127 namespace constructions to 2**, and every game built on
`packages/engine` gets ~4.2 seconds of its boot back.

## What everything else costs (for the record)

Everything a game does at startup was always nearly free — which is why the import binding was the
whole story. The staged run of `main.tish`, after the fix (the `before` column is the same work with
the quadratic still in place, to show that only the first row ever moved):

```
STAGE                     AT     COST   SECONDS      before
modules                   12        8      0.13         266   <- the fix
uiInit                    12        0      0.00           1
tileReserve224            14        2      0.03           1   <- akari's deliberate ~35K: cheap
dialogInit                14        0      0.00           1
mount (12 behaviours)     17        3      0.05           3
scene-small (3.6K tiles)  18        1      0.02           1
scene-big (38.4K tiles)   27        9      0.15          10
spawn24                   34        7      0.12           7
frame1                    37        3      0.05           3
```

A whole staged boot — two scenes streamed, 24 entities spawned, UI and dialog initialized — now costs
**45 frames (0.75s)**, down from 305.

## How it works

The unit is **emulated frames**. mGBA runs a fixed slice of CPU per `runFrame` whether or not the ROM
calls `frame()`, so a boot that spans 465 of them really did consume 465 frames of CPU, and the
numbers translate directly to what a player waits through on hardware. `tish-agb`'s `ticks()` is no
use here: it is Timer2, which wraps every ~250ms and cannot represent a 7-second boot.

Each ROM logs `BB <name>` markers; `tools/gba-shot` prefixes the emulated frame (`GBA_SHOT_LOG=1`),
so a stage's cost is the gap between consecutive markers.

- **`src/floor.tish`** — imports nothing. Every other number is charged against this one. It exists
  because module init is the one stage a game *cannot* measure from inside itself: asset registration
  and import binding run before `main.tish`'s first statement, so a `log()` at the top of a game
  reports the total and attributes nothing.
- **`src/v_*.tish`** — the floor plus exactly one group of imports, to isolate that group's price.
- **`src/main.tish`** — the full staged boot, in the order a real game pays it, using the slow games'
  own assets (akari's sheets and scenes, the topdown RPG port's overworld). A bench that profiles assets nobody
  ships measures a game that doesn't exist.

```bash
./run.sh              # the two tables above
./run.sh --games      # ...and every shipped example's first-paint frame, for context
```

`--games` needs no instrumentation at all — `GBA_SHOT_TRACE=1` reports the frame the picture changed,
which is the honest measure of "when did the player see something".
