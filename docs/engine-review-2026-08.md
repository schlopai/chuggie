# Engine review — August 2026

A whole-stack read of chuggie-engine: the Rust crates, the tish language as it actually compiles, the
`packages/` authoring layer, and the 137 examples. Written to answer one question at the end —
**what would it take to build a real-time strategy game here** — but the first five sections stand
on their own as a current-state account.

Companions, not duplicates: [`ARCHITECTURE.md`](../ARCHITECTURE.md) is the layer map,
[`INVENTORY.md`](../INVENTORY.md) the contents plus the consistency backlog,
[`CONTRACT.md`](../CONTRACT.md) the compiler↔framework wire format, and
[`docs/perf-rules.md`](perf-rules.md) the seven measured frame costs. This review cites all four
rather than restating them, and adds what none of them cover: the language traps that live only in
commit messages and issue threads, and the RTS gap analysis.

---

## 1. The stack, as it is

```
agb 0.25                     hardware, VRAM, mixer, fixnum, input
  ▲
③ tish_runtime_gba (facade)  THE BOUNDARY (lives in the tish repo). Value/Fixed vocabulary,
  │                          gba::{init,halt,pre_commit}, asset arenas.
  ▼
④a tish-agb                  12,558 lines. Idiomatic agb wrapper: deferred draw list, handle
  │                          arenas, frame() driver, input, audio, dialogue, fade, camera.
  ▼
④b tish-gba-game-engine      7,870 lines in one lib.rs. SoA ECS + fixed per-frame pipeline +
  │                          genre modules (grid/platformer/topdown/shmup) + the isob_* board.
  ▼
④c packages/*.tish           43 top-level + clan/ (5) + the SRPG rules modules (15) = 63 tish modules.
  ▼
④d examples/*                137 directories.
```

The one architectural rule — dependencies point down and toward agb, never sideways or up — **still
holds in the code**. `tish-gba-game-engine` drives `tish-agb` and has no direct `agb` dependency;
both share the facade's `Value`/`Fixed`.

The stack has grown roughly 2× since `INVENTORY.md`'s figures were written (it says ~2,100 and
~4,100 lines for the two crates, and "52 games/demos"). Those numbers are stale, not wrong in
spirit; the shape is unchanged.

### The three dispatch tiers

Every tish→Rust call lands in one of three tiers, and the difference between them is the single
biggest performance lever in the repo:

1. **`native_*` (Rust→Rust)** — the engine calling into `tish-agb` inside a frame. No `Value`, no
   lowering. Internal to the crates; a game never sees these.
2. **`*_typed` (typed externs)** — a `declare fn` in the crate's `tish.d.tish` paired with a
   `cargo:` import lowers the tish call to a direct Rust call. No boxing, no `value_call`.
   **244 declarations exist today** (168 in the engine, 76 in tish-agb) against 197 `_typed`
   functions in the engine — the coverage is real but incomplete.
3. **boxed `fn(&[Value]) -> Value`** — dynamic dispatch through `value_call`. Correct for dialogue,
   config objects, arrays and callbacks; **wrong for anything per-frame**, at ~117 ticks a call.

`INVENTORY.md` §6 lists the hot calls still stuck in tier 3 with no typed twin. That list is still
accurate and still worth closing: `entity_tag`, `topdown_facing/moving`, `platformer_grounded/
blocked/vy`, `grid_facing/col/row`, `camera_set`, `key_released`, `sprite_set_visible/depth`. Two
orphans remain — `damage_typed` and `sprite_set_flip_typed` exist in Rust but are not declared, so
they are unreachable. That is a two-line fix that has been outstanding for a while.

## 2. The native surface

**374 `pub fn` in the engine, 344 across tish-agb** — roughly 700 entry points. Grouped:

**`tish-agb` (hardware).** Sprites (`sprite_new/set_pos/frame/sheet/flip/visible/depth/hud`),
backgrounds and tilemaps (`bg_new`, `bg_bands` for per-scanline banding, `tilemap_set/set8`,
`scene_stream`, `map_stream`, `camera_set`, `fade`), destructible terrain (`terrain_carve/disc/
mass/planet`), input, text and the UI canvas (`text_draw`, `hud_text`, `hud_bar`, `ui_begin/rect/
text/clear/reserve_tiles/release_scratch`), dialogue, audio (PCM mixer, PSG synth, chiptune, the
`deck` sequencer, adaptive ducking), particles and screen FX, save (slots + a raw SRAM window),
frame/profiling (`ticks`, `frame_stats`, `heap_free`, `log`), link cable, and the Mode 7 / kart
module.

**`tish-gba-game-engine` (simulation).** The SoA world and its fixed pipeline; **21 native AI and
movement systems** (`set_patrol`, `set_chase`, `set_shooter`, `set_charger`, `set_jumper`,
`set_hopper`, `set_orbiter`, `set_boomerang`, `set_grabber`, `set_lure`, `set_train`, `set_trap`,
`set_guard`, `set_mover`, `set_walk`, `set_blocker`, `set_part`, `set_lifetime`, `set_stun`,
`set_arena_wrap`, `set_despawn_offscreen`); physics with dynamic disc bodies; health and damage
with types, weaknesses and immunities; projectiles (`fire_bullet/angle/aimed/spread/ring`); entity
pools; three character controllers (platformer, top-down, grid); the `isob_*` iso board; and the
profiling counters.

**The design principle is stated consistently throughout and is worth repeating**: the engine owns
per-frame *deterministic resolution* natively — movement, collision, physics, combat, animation,
culling, pathfinding. The tish game owns *decisions*: when to fire, what to do on collide, on
death, on interact. Density comes from native work. A screen full of bullets is free; a screen full
of tish AI is not.

The 21 native AI systems are that principle paying off, and they are the reason the "what's
missing" list in §6 is short.

### The one real architectural divergence

`INVENTORY.md` P1 item 1 is still open and still the biggest one: **the SRPG stack is a second engine
bolted alongside the first.** `IsoBoardGrid`/`IsoBoardUnit` keep their own globals, duplicate hp/max/team/
speed that `Health`+`tag` already model, use an immediate death model where the World uses deferred
`onDeath`, have no SoA/mask/generational-ids, pack fields as `u8`/`i16` where the World is `i32`
everywhere, are not driven by `world_step`, and are entirely boxed with zero typed twins.

It works, and the isoboard examples prove it. But it should be either blessed in `ARCHITECTURE.md` as a
deliberate standalone "logical board" subsystem or folded onto the World. Today it is silent
divergence, and §6 shows the cost: an RTS wants grid pathing, and the only grid pathing we have is
inside a subsystem shaped entirely around turn-based play.

## 3. The packages layer

63 tish modules. The genre-kit extraction has gone well and is not finished:

| Genre | State |
|---|---|
| platformer | ✅ `packages/platformer.tish` — walk/run, jump-feel windows, crouch/slide, ladders, ledge grab, wall jump, NPC/sign/door components. Five older platformers still carry private copies worth migrating. |
| isometric | ✅ `packages/iso.tish` + `iso_actors.tish` — projection, depth bias, riser redraw, camera clamp. Extracted after the two verbatim copies had already drifted (`UNIT_LIFT` was 12 in one and 20 in the other). |
| top-down | ✅ `packages/topdown.tish` — two movers, facing/tile/room arithmetic that had **seven** copies in one topdown RPG port, the warp latch both games hand-rolled, `tdContextAction`, soft targeting, and the L/R chord skill wheel. Room streaming deliberately still per-game; see [`topdown-genre.md`](topdown-genre.md). |
| puzzle/grid | `packages/grid.tish` exists (90 KB, packed one-i32-per-cell board) but `packages/grid` as a *genre kit* is still on the backlog. |
| SRPG | ⚠️ The SRPG presentation package is presentation only. The isoboard SRPG example still hand-rolls ~763 lines of battle controller. |
| **RTS** | **Does not exist.** §6. |

Two modules dominate by size and both are load-bearing: `ui.tish` (159 KB — a runtime flexbox-lite
layout engine, widget factories and flash-free patch paths) and `battle.tish` (88 KB — an entire
grid-battle fight as a component).

`battle.tish`'s size is exactly why `party.tish` was written rather than reused: `battle.tish`
drags in `isob_*`, `./iso` and the SRPG rules modules and cannot be separated from a grid, so a front-view JRPG
battle with no board could not use it. That is the right call, and it is also a signal — a package
that cannot be used without its genre's *board* is a genre implementation, not a kit.

`pool.tish`, `fx.tish` and `music.tish` are constants-only name files for native subsystems. That
is a good pattern and under-used; it costs nothing and makes native APIs readable.

## 4. The tish language, as measured

[`perf-rules.md`](perf-rules.md) has the seven frame costs with their numbers and is the document
to read first. What follows is the set of **language-level traps that are not in it** and currently
live only in commit messages, issue threads and one engineer's memory. They have each cost real
time at least once.

### Typing

- **An untyped scalar is soft-float.** `const X = 5` *and* bare `let X = 5` compile to a
  thread-local `Cell<f64>` on a CPU with no FPU. Annotate `: i32`. This is rule 1 of `perf-rules.md`
  and it is repeated here because it is the one that costs the most and the one everyone forgets.
- **An untyped array is 28 bytes per element.** `let xs = []` boxes every element;
  `let xs: i32[] = []` is a `Vec<i32>` at 4. One package leaked 46 KB to this.
- **`a * b` on i32 locals is still soft-float.** Multiplication does not follow the same typed
  lowering as add/subtract — use `Math.imul`. A masked array index additionally needs a
  **power-of-two array length** *and* the mask written at the index site, not upstream.
- **A typed struct assignment copies.** `let s: Ship = SHIP` takes a snapshot; every subsequent
  write to `s` is discarded. Go through `SHIP.field`.
- **A typed fn still emits a boxed closure.** Adding types to a function changed its stack frame by
  zero bytes in one measurement — the compiler emits both a native fn *and* a `Value::native`.
  Typing helps the call *sites*, not the definition's footprint.
- **`arr.length = 0` silently no-ops on a typed array.** Use a pop loop.

### Semantics

- **No `undefined`** — a missing value is `null`.
- **No loose equality** — `someBool === 1` is `false`. Use `= null` initialisers and explicit `0`/`1`.
- **`/` is not reliably truncating.** Where a division genuinely belongs, write `((a * b) / c) | 0`.
  And remember the ARM7TDMI has no divide instruction at all: every `%` and `/` is a software call.

### Build behaviour

- **Builds are nondeterministic.** The same source emits Rust in a different *order* every build
  (a `HashMap` in codegen). Diffing two builds of unchanged source is not a signal.
- **`tish build` caches packages.** Editing `packages/*.tish` may not recompile. When a package
  change appears not to take: `rm -rf.tish`, then grep the generated Rust to confirm.
- **⚠️ `TISH_FAST_NATIVE_BUILD` hides errors.** It exits 0 on a failed GBA compile and leaves the
  *previous* `.gba` in place, so you debug a stale ROM. Do not use it while iterating.
- **A missing name builds.** tish does not check that an imported function exists; a typo compiles
  and then throws at runtime — a black screen, not a compile error and not a hang.
- Any interrupted or corrupted build: `rm -rf.tish` and rebuild. There is no cheaper recovery.

### The stack

The largest single class of "impossible" bug in this repo's history: **`run()`'s frame eats ~27 KB
of a 29 KB stack.** Module-level functions become closures that live in `run()`'s frame for the
whole program, so a game with several hundred of them boots and then faults somewhere unrelated —
and the fault *relocates* when you perturb the code, which is what makes it look impossible.

The fix that works is structural, not organisational: **wrap a module's functions in a factory** so
their closures leave `run()`'s frame. Moving code between *files* changes nothing; closure
*lifetime* is the variable. This landed for `packages/battle.tish` and for the large SRPG example's shell and took
the battle from "resets the ROM" to zero faults.

The corollary for any new large module — including the RTS work in §6 — is to adopt the factory
shape from the first commit rather than discovering the ceiling at 600 closures.

## 5. The example corpus

137 directories, and they are not all the same kind of thing. Read by category:

| Category | Count | Purpose |
|---|---|---|
| the SRPG example family | 31 | One SRPG subsystem each, plus the full template (3,162 LOC) and the isoboard example (2,960). |
| `iso-*` | 12 | Isometric subsystem slices, same skeleton, not yet renamed to match. |
| Full games | ~20 | the two topdown RPG ports (8,304 and 6,130 LOC, since moved to their own repo), `warheads`, `versus`, `beatemup`, `kart-circuit`, `akari`, `blockfall`, `solitaire`, `creature-rpg`, … |
| Feature demos | ~40 | One API each, small and teaching. |
| de-risk spikes | 9 | Labelled spikes (A1–A5, B1–B4) retired *before* the large topdown RPG port was written. |
| `bench-*` | 8 | Measure something. |
| probes / `repro-*` | 17 | Reproduce a specific compiler or runtime bug. |

**The de-risk spike pattern is the most valuable process artifact in the repo** and is worth naming
explicitly, because it is what §6 proposes to reuse: before building a large game, write one tiny
ROM per load-bearing assumption, measure it, and only then build the game. Nine spikes cost far
less than one 6,000-line game discovering its ceiling at the end. The repo's own memory says the
same thing more bluntly — a 5-minute build is not a debugging loop.

Two active WIPs at the time of writing: `warsong` (2,648 LOC, a real-time 3v3 CTF battleground on
the top-down engine, being polished with an FX/juice layer) and `jrpg-party` (599 LOC of pure
renderer over the new `packages/party.tish`).

### Loose ends

- `examples/visual-novel` has no `package.json` — it is not a workspace and cannot build.
- `examples/p0-spike` is Rust-only (`src/main.rs`, its own `Cargo.toml`), zero tish. It is an
  engine spike sitting in the examples directory.
- `bench-room` has no built ROM; `iso-shop` has a `.sav` but no `.gba`.
- An earlier rename of the SRPG example family left **both** ROM artifacts in most of those 31 directories.
- Six examples still lack a `preview.png`, so `examples/README.md` shows them without a thumbnail.

None of these are urgent. All of them are the kind of thing that quietly makes a corpus feel
untended, and they are cheap.

---

## 6. The RTS gap analysis

Real-time strategy is the first genre here that needs **many units moving simultaneously under a
single player intent**. Every genre built so far either has few actors (fighter, JRPG, SRPG) or
many actors with *no individual intent* (shmup bullets, brawler crowds). That distinction is the
whole problem.

### What an RTS can stand on today

| Need | What exists |
|---|---|
| Grid pathing | `isob_move_range` / `isob_path` — BFS flood-fill with parent reconstruction, per-terrain move costs, zone-of-control. |
| Unit separation | Native dynamic disc bodies (`set_body`, `set_dynamic`, `body_impulse`), proven by `examples/soccer` and `examples/golf`. |
| Acquire / chase / attack | `set_chase`, `set_guard`, `set_shooter`, `set_charger`, `fire_aimed`, `damage`, `set_health`, `set_hurt` — all native, zero tish per unit. |
| Pooling | `pool_new` / `pool_arm` / `pool_retire` — the acceptance test is `examples/microgame`. |
| Cursor + camera | `camera_set`, `set_room_camera`, plus the board cursors in the isoboard SRPG example and `warheads`. |
| Hero ability wheel | `tdChordBank` / `tdChordPoll` in `packages/topdown` — hold a shoulder, edge a direction, fire a slot. Built for `warsong`, reusable unchanged. |
| Production / build menus | `packages/ui` widget factories and the `packages/shop` state machine. |
| Mission flow | `packages/scene`, `packages/dialog`, `packages/title`, `packages/save`. |
| Maps | Tiled `.tmj` through `scene:`, plus `scripts/recipe_to_tiled.py` and `scripts/ninja_autotile.py`. |
| Art | The Ninja Adventure CC0 catalog: `TilesetFloor` (eight 47-blob materials), `TilesetNature` (forests), `TilesetHouse` (fortress wall, palisade, mine entrance), `tileset_camp` (tents, pavilion), and 89 `standard-4dir` actor sheets with walk, idle and attack rows. |

That is a great deal — and it is why the missing list is only four items long.

### The four things that do not exist

**1. No flow field.** `isob_path` answers "the route for *this* unit, *right now*", which is the
turn-based question. Twelve units converging on one destination want the opposite shape: **one**
BFS whose result every unit reads in O(1), rebuilt only when the destination changes. Running a
per-unit path query per frame in tish would be the most expensive thing an RTS could possibly do
here.

**2. No seek/arrive movement component.** `set_chase` follows an *entity*. `set_mover` follows a
*pattern*. Nothing follows a *destination* — which is the only order an RTS ever gives.

**3. No fog of war.** No visibility grid, no shroud layer, no reveal stamp. A per-cell circle stamp
written in tish is both divide-heavy (rule 3) and per-cell, and it runs on the frames where units
are moving, which are the busy ones.

**4. No attack-move.** The composite order — walk toward a point, break to engage anything that
comes into range, resume when it dies — has no native form. Composing it from `set_chase` plus a
tish supervisor puts a boxed decision on every unit every frame, which is rule 7 exactly: the game
would be fine on an empty map and halve its frame rate the moment two groups met.

### What that implies

All four are **native systems**, not tish helpers. That is not a preference; rule 7 of
`perf-rules.md` says it directly and gives the receipts — *"the fix is a native system, not a
faster hook"*. An RTS built with per-unit tish ticks would be slow in precisely the moments it is
interesting, and no amount of tightening the hook recovers it.

They are also, usefully, **not RTS-specific**. A flow field is the right primitive for tower
defense, for any pursuit AI, and for crowd movement of any kind. `set_seek` is the missing sibling
of two components that already exist. Fog of war is a visibility grid plus a dirty-cell blit. Only
attack-move is genuinely genre-shaped, and even that is composition over machinery already present.

### Two design constraints that follow from the hardware

Worth stating before anyone designs the game, because they determine its shape:

- **The unit cap is ~20–25 live entities at 60fps** (~60 ticks each of a 4,389-tick frame). This is
  measured, not estimated. A Warcraft-3-sized army — a food cap around 12 per side — is therefore
  not a compromise; it is the shape the hardware wants, and it happens to be the shape of the genre
  people remember most fondly.
- **Buildings must not be entities.** A building never moves, never animates and needs no body.
  Making one an entity spends a live-entity slot on scenery. A building is a stamped tilemap rect,
  a solid footprint, and a row in a flat `i32[]` table.

### What was then built, and what it measured

All four landed, plus a `packages/rts.tish` genre kit, three de-risk spikes and
[`examples/warforge`](../examples/warforge/README.md) — a three-mission campaign that runs at
**4,375–4,377 EMA against the 4,389-tick budget** on every mission.

The predictions above held: the unit cap is the shape of the genre, buildings-as-tiles works, and
the native systems cost the game nothing per unit per frame. What the spikes found that this section
did *not* predict is the more useful half:

- **A flow field's last step has to be in pixels.** Inside the goal cell the field is flat, so
  "step to a smaller number" finds no move and every unit parks up to 15px short, forever.
- **A transform is a collider's top-left, not its centre.** Ask "which cell am I in" about the
  corner and a unit walks into the wall *beside* a gap. Corridor centring is the other half of it.
- **Physical unit separation does not fit.** `set_blocker` on every unit measured 6,626 ticks (51%
  over) *and* wedged units in corridors — 1 of 24 arrived. Staggered arrive radii ship instead.
- **Off-screen culling is wrong for this genre.** An opt-in `set_cull_offscreen` was built, measured
  (~600 ticks) and removed: an army ordered across the map walks off screen and must keep walking.
  The `is_active` comment now records it so nobody rebuilds it.
- **A periodic spike is invisible to an average.** warforge sat at 4,982 EMA while *seven frames in
  eight* were comfortably inside budget — one roster scan every eighth frame cost ~4,900 ticks. It
  is exactly the failure `frame_period(2)` exists to expose and that "it feels fine" cannot.
- **A pooled entity is not free.** `world_step` walks every slot of every system: 26 parked entities
  measured 2,631 ticks doing nothing. Pool sizes are a frame-budget decision.
- **Dropping a roster is not despawning it.** Clearing the arrays between missions left the previous
  mission's entities in the world — mission 2 ran at 6,632 reached through mission 1 and 4,377
  booted into directly. A leak that only appears on the *second* scene.

The recurring shape: every one of these is a **cost the code does not look like it has**, and every
one was found by a small ROM that measured one thing. That is the argument for the de-risk spike
habit, made again.
