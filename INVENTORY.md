# chuggie inventory — what is where, and why

> **Before optimising anything on this machine, read [docs/perf-rules.md](docs/perf-rules.md).**
> The seven things that actually cost a frame, each measured on device — starting with the one
> nobody guesses: an untyped scalar is soft-float, and `scripts/const_to_let.py` fixes it in bulk.

A complete map of the codebase: the Rust crates, the shareable tish packages, and the examples —
what each contains, why it lives in Rust vs tish, and where the architecture has drifted.

Companion to [`ARCHITECTURE.md`](ARCHITECTURE.md) (the layer map + dependency rules) and
[`CONTRACT.md`](CONTRACT.md) (the compiler↔framework wire format). This doc is the *contents* of
each layer plus a prioritized **consistency backlog** (bottom).

## The layer stack (what actually lives in each tier)

```
agb 0.25                       hardware, VRAM, mixer, fixnum (Num<i32,8>), input
  ▲
③ tish_runtime_gba (facade)    THE BOUNDARY (in the tish repo). Value/Fixed vocabulary,
  │                            gba::{init,halt,pre_commit hooks}, asset arenas
  │                            (sheet/bg/wav/font/emoji/map), get/set_prop incl. Value::Struct.
  ▼
④a tish-agb                    idiomatic agb wrapper. GbaCtx = deferred draw list + handle arenas
  │                            (sprites/bgs/stream layers/text/bars), frame() driver, input,
  │                            audio, dialogue, fade, camera. ~2100 lines, one file.
  ▼
④b tish-gba-game-engine        SoA ECS (15 components) + fixed per-frame pipeline (20 systems) +
  │                            genre modules (grid/platformer/topdown/shmup) + a separate isoboard
  │                            board subsystem. Drives tish-agb; NO direct agb dep. ~4100 lines.
  ▼
④c packages/*.tish             reusable tish sugar: engine.tish (makeEntity), shmup.tish (genre kit),
  │                            scene.tish, title.tish, cutscene.tish.
  ▼
④d examples/*                  125 games/demos.
```

**The one rule (from ARCHITECTURE.md):** dependencies only point *down and toward agb*. The engine
drives tish-agb; tish-agb wraps agb; both share the facade's `Value`/`Fixed`. Nothing reaches sideways
or up. This holds in the code.

---

## The tish↔Rust boundary — the "why", and the 3 dispatch tiers

**Guiding principle (stated throughout the engine):** the *engine owns per-frame deterministic
resolution* (movement, collision, physics, combat, animation, culling, pathfinding) in native Rust;
the *tish game owns decisions/rules* (when to fire, what to do on collide/death/interact). "Keep
movement/animation native, only decisions in tish." Density comes from native work; a screen full of
bullets/blobs is free, a screen full of AI is not.

There are **three call tiers** between tish and Rust, fastest → slowest:

1. **`native_*` (Rust→Rust)** — plain functions the engine calls into tish-agb every frame
   (`native_sprite_set_pos/frame/flip`, `native_camera_set`, `native_sprite_release/restore`). No
   `Value`, no compiler lowering. The hottest path; internal to the crates.
2. **`*_typed` (typed externs)** — a `declare fn` in a crate's `tish.d.tish` paired with a `cargo:`
   import lowers a tish call to a direct `crate::name_typed(..)` Rust call — no `Value` boxing, no
   `value_call`. Used for the hot per-frame scalar paths a *game* calls.
3. **boxed `fn(&[Value]) -> Value`** — dynamic dispatch via `value_call`. For low-cadence or
   genuinely dynamic calls (dialogue, config objects, arrays, callbacks).

Plus the perf-critical **data** patterns: typed structs (`interface` → native `TishStruct_*`, fields
are native loads via `Value::Struct`), per-entity **cvar** i32 slots (native tick state instead of a
boxed `Value` context), and **lean ticks** (`lean:true` → the tick gets only the entity id).

---

## ④a tish-agb — hardware binding (crates/tish-agb/src/lib.rs, ~2100 lines)

`GbaCtx` is a retained **deferred draw list** over agb 0.25's frame-scoped model: every mutator writes
into retained arenas, and `frame()` re-emits `show()` calls each vblank. `frame()` order: backgrounds →
scene backdrops → stream layers → HUD sprites → world sprites (depth-sorted) → dialogue → HUD bars →
HUD text/emoji → fade blend → commit → mixer (if `audio_used`) → input.update. (OAM *emit order*, not
priority, decides sprite-vs-sprite overlap — hence HUD-first + depth sort.)

The four background layers are a **budgeted** resource, not a free-for-all: the UI canvas is reserved
first and the rest fill front-priority-first, so an artist adding a map layer degrades the backdrop
instead of aborting the ROM on agb's 5th-`show()` panic. Per-scanline parallax (`bg_bands` /
`sceneBands`) buys extra apparent depth without spending a layer. All of it — the priority rule that
decides whether the player is visible, the one-palette-set constraint, why scene backdrops are
wrapping backgrounds rather than streamed, and how to measure a scroll rate without fooling yourself
— is in **[`docs/gba-backgrounds.md`](docs/gba-backgrounds.md)**.

| Subsystem | Key ABI (T=typed, B=boxed, N=native) | Notes / why |
|---|---|---|
| **Sprites** | `sprite_new`T, `sprite_set_frame`T, `sprite_set_pos`T; `sprite_set_flip/visible/sheet/hud/depth/destroy/clear`B; `native_sprite_*`N (engine hot path + off-screen VRAM release/restore) | arena+free-list recycle; frame identity-skip avoids re-DMA; off-screen VRAM culling |
| **Backgrounds** | `bg_scroll`T, `bg_set_visible`T; `bg_new/bg_clear/backdrop`B; `tilemap_*`B (array-driven, legacy) | `bg_new` fills the **whole 32×32 map** so scrolling has no seam |
| **Tile streaming** | `map_stream/scene_stream/map_solid_at/map_spawn_*`B | map stays in ROM (`include_bytes!`), read on demand — never on the tish heap |
| **Text + fonts** | `hud_text/text_draw`B | per-slot change-cache (re-layout only on change); inline emoji overlay |
| **HUD bar** | `hud_bar`B, `hud_hearts`B | `DynamicSprite16`, cached per slot |
| **Audio** | `sound_play/music_play`B | mixer @ 10512Hz mono; `audio_used` gate skips DSP for silent games |
| **Input** | `input_x/y`T, `key_pressed`T, `key_held`T; `key_released`B | |
| **Dialogue** | `dialogue_show/ask/active/advance/move/pump`B | typewriter box on a dedicated bg; re-entrant choice callback |
| **Fade** | `fade`T | BLDY brightness blend; skipped at level 0 |
| **Camera/misc** | `camera_set`B / `native_camera_set`N; `frame/vblank/log/timer_read`B | |

Schemes contributed by `tish-agb/tish.schemes.json`: `asset:` `sheet:` `sheet32:` `sheet64:`
`background:` `wav:` `map:` (facade arenas), and `scene:` `isoboard:` `isoboard:` `font:` (`path@N`,
default `@16`) `emoji:` (via the `tish_gba_scenepack` proc-macro crate).

---

## ④b tish-gba-game-engine — the ECS (crates/tish-gba-game-engine/src/lib.rs, ~4100 lines)

**15 components** (u16 mask, 15 flags used, 1 free), each a dense SoA column in `World`. Entity ids are
`(gen<<16)|slot`, generation-validated; `spawn`/`despawn` recycle slots via a free list and reset every
column; `despawn` frees the sprite VRAM and bumps generation. Per-entity extras that are *not*
components: `tag` (kind label) and `ndata` (8× i32 **cvar** slots for native tick state).

| Component | Purpose | Component | Purpose |
|---|---|---|---|
| Transform | Fixed position | Health | hp/max/i-frames/dead |
| Body | Fixed velocity | Patrol | walk-direction AI |
| SpriteRef | tish-agb sprite handle + offset | Life | TTL + off-screen cull |
| Collider | AABB | Hurt | contact hurt-box |
| GridPos | tile + slide + facing | Mover | native movement pattern |
| Anim | clip playback | TopDown | 8-dir intent + facing + knockback |
| Walk | directional-sheet frames | Chase | seek-player AI config |
| Platformer | gravity/jump/coyote/buffer state | | |

**`world_step` pipeline (one call/frame, 20 systems, Timer2-profiled in 5 phases):**
1. **Behaviours + ticks** (dispatches tish) — `collect_behaviours`→`update`; `collect_ticks`→ lean tick (id only) or non-lean tick (prefill ctx / read back intent).
2. **Native systems** (pure Rust, O(n)) — patrol → mover → movement (integrate) → grid → platformer → chase AI → topdown → combat (hurt×health) → life (TTL/offscreen cull).
3. **Collisions + deaths** (dispatches tish) — `onCollide` (responders×n) → `onDeath`.
4. **Render group** (pure Rust) — health/i-frames → anim → walk → room-transition → render (position + VRAM cull) → camera → `tish_agb::frame()`.

**ABI:** entity lifecycle, component setters, ticks/behaviours (`define_component`/`add_behaviour`,
boxed), combat, movers, native bullet emitters (`fire_bullet/angle/ring/spread/aimed`), getters,
profiler (`step_ticks/frame_period/step_peak`), and the genre helpers below. `tish.d.tish` declares 26
typed externs; the perf-critical setters/getters/emitters are typed, the config/genre-query calls boxed.

---

## Per-genre map (the whole engine, every genre)

| Genre | Native engine support | Reusable tish package? | Reference example(s) |
|---|---|---|---|
| **Shmup** | `Life`/`Mover`/`Hurt` + `combat_system`/`life_system`/`mover_system` + native emitters `fire_*` + `BulletStyle` + `set_arena_wrap` (toroidal screen, `wrap_system`) | ✅ **packages/shmup.tish** (the model kit: typed structs, cvar, lean ticks, native emitters) | `shmup` (reference), `asteroids` (rotate-and-thrust, wrap-around arena); the flagship shmup campaign moved to its own repo |
| **Platformer** | `Platformer`/`Patrol` + `platformer_system` (gravity, AABB, one-way + **ladder** planes, coyote/jump-buffer/variable-jump, sticky `face`) + `patrol_system` + `platformer_interact` + room camera | ✅ **packages/platformer.tish** — init/drive/animate + ladders, ledge grab & pull-up, wall slide & wall jump, crouch/slide, and data-driven `PfNpc`/`PfSign`/`PfDoor` | `oakhollow` (flagship: town, dialog, shops, interiors, parallax); sunny-land, dark-hero, platformer-combat, platformer-scroll/-rooms (all pre-package, still hand-rolled) |
| **Top-down (action-RPG)** | `TopDown` + `topdown_system` (8-dir + collision + knockback) + `Chase` native seek AI + `swing` melee + grid interact. `topdown_snap` profiles: **0** free 8-way · **2** tile stepping (a direction commits a full 16px cell) | ❌ **none** (uses engine.tish `makeEntity` methods) | `akari` (flagship: title+cutscene+scenes+combat); the topdown RPG port (own grid-step controller; moved to its own repo); ninja-village, ninja-adventure |
| **Grid/RPG (tile-locked)** | `GridPos` + `grid_system` (tile-locked 4-dir + occupancy) + `Walk`/`walk_system` + `grid_interact` | ❌ **none** (engine.tish `loadMap` inline arrays) | `overworld-demo` (sole) |
| **Puzzle (cell grid)** | `tilemap_set` (16px cells) and `tilemap_set8` (one 8x8 tile — added for `blockfall`, because a 10x20 well does not exist at 16px), plus `sheet8:` 8px sprites for a piece that moves every frame | ✅ **packages/grid.tish** — the packed cell word, runs, flood fill, gravity collapse, the causality plane and the tile painter, for anything whose board is columns of PACKED cells | `grid-demo` (floor-stacked match-3 on the kit, with a rising floor); `blockfall` (falling-block, guideline ruleset — deliberately **not** on the kit: packed columns cannot model an overhang, and the hole under one is the whole game) |
| **Isoboard (SRPG)** | ⚠️ a **separate** board subsystem (`IsoBoardGrid`/`IsoBoardUnit`/`IsoBoard`): BFS move-range, pathfinding, speed-CT turn order, unit HP — *not* the SoA World, *not* in `world_step`, all boxed `isob_*` | ⚠️ **partial** — `packages/iso` (projection/depth/risers/camera); the phase machine, AI and turn flow live in the SRPG examples **by choice** | a downstream project repo |
| **Isometric render** | `sprite_set_depth` + depth-sort in `frame()` | ✅ **packages/iso.tish** (+ `iso_actors.tish` for the 32px sheet frame contract) | `iso-sprite` (tech demo, pre-package); SRPG examples now in a downstream project |
| **Rhythm (call-and-response)** | `deck_frame()` — the deck sequencer's playhead, exposed so a chart and its song share ONE clock. Added for this genre: the sequencer advances per **elapsed display frame** (`music_catchup`), so a tish-side frame counter falls behind the music on every frame the game misses, and the chart slides off a frame at a time | ✅ **packages/rhythm.tish** — beat clock, hit windows, misses, stray-press-vs-freestyle, combo/score/meter, and the scrolling prompt lane | `rap-dojo` (sole; Parappa-style, fake 3D from `bg_bands`) |
| **Kart racing (Mode 7)** | `kart_*` in `crates/tish-agb/src/kart.rs` — the whole simulation behind ONE `kart_step()` per frame: fixed-point handling (speed along the heading + slide across it, no `sqrt`), per-surface top speed and drag, drift with a mini-turbo charge, boost, ordered-gate lap validation, standings, and waypoint-following opponents with a rubber band that scales TOP SPEED (as a per-frame subtraction it could exceed acceleration and stop a leading AI dead). Plus `kart_draw()`: billboard placement, the heading frame, and the near/far sheet swap the GBA needs because it cannot scale a sprite | ✅ **packages/kart.tish** — the genre kit over those natives; nothing per-kart on the frame path | `kart-circuit` (sole; drift+boost, off-road, 3 rubber-banded AI, attract-mode demo) |
| **Card / cold screen** | none — and none needed. Cards are `ui_rect` + `ui_text` on the UI text canvas: no OAM, no sprite VRAM, no palette banks, no art files. The constraints that matter are the canvas ones: ONE 15-entry UI palette, `ui_begin` once per frame, and `ui_clear_rect` snapping OUT to whole 8x8 tiles | ❌ **none** (deliberate — nothing here generalises past "a pile of cards"; the reusable half already exists as card-gba's `card-ui.tish`) | `solitaire` (Klondike: deal, run moves, undo, auto-finish, attract player). ⚠️ a whole-table repaint is ~20 FRAMES — gate per PILE. ⚠️ NEVER clear-then-draw: the beam crosses the cleared region before the redraw lands, which the player sees as FLICKER on every selection; overdraw opaquely and erase only the tail a shrunken pile leaves. ⚠️ a screenshot taken during a multi-frame repaint shows a HALF-DRAWN canvas, which looks exactly like a draw call that failed |
| **Versus fighting** | none — and the point of the example is WHY none. The fight loop is pure tish over flat `i32[]` state with no entity system in it at all, because the cost that matters is not arithmetic but CALLS: a 1-arg call into anything touching module state measured ~117 Timer2 ticks against a 4,389-tick frame (boxed `Value` dispatch, module boundary or not), a call into a function touching NO module state ~1 (promoted to a real Rust fn), and a module-array read ~1.7. Added for this genre: `keys_held()` in `crates/tish-agb` — the held twin of `keys_edge`, because reading four directions and four attack buttons per fighter was eight boxed dispatches a frame | ✅ **packages/fighter.tish** (frame data: startup/active/recovery, hitbox vs stance-dependent hurtbox, blockstun vs hitstun, pushback, hit-stop, cancels, round state) + **packages/motion.tish** (input history ring, QCF/QCB/DP/HCF recognition mirrored by facing, the 2- and 4-frame button buffers) | `versus` (sole; 4 fighters, best of three, CPU opponent that plays through the same input ring). ⚠️ the ring size must stay a POWER OF TWO — `% RING` is a software divide on the ARM7TDMI and cost 1,400 ticks a frame. ⚠️⚠️ do NOT give a stage `bg_bands` unless the scene has frame headroom: banding turns a DROPPED frame into a CORRUPT one, because the HBlank DMA re-arms mid-scanout and every row below the re-arm gets band 0's offset |
| **Beat-em-up (brawler)** | none — same argument as the fighting row (plus ⚠️⚠️ the repo-wide one it uncovered: a tish `const` is a thread-local `Cell<f64>`, so every use is SOFT-FLOAT; `let X: i32 =` is native integer and was worth 20-27% of a frame in both these games — most of packages/ has not had it), plus one addition it needs that a versus game does not: `sprite_set_depth`, so N actors on a road can be painter-sorted by where they stand rather than by registration order | ✅ **packages/beatemup.tish** — N actors on a DEPTH axis (y = position across the road, which decides draw order, draw row AND whether a punch connects), frame data, a chaining light attack, knockdowns with a landing arc, wake-up invulnerability, a health-costing panic move, and per-actor AI that converges on the player's LANE before its column. Shares `packages/motion.tish` and `scripts/fighter_art.py` with versus | `beatemup` (sole; three wave-locked arenas, four actors on screen). ⚠️⚠️ `sprite_set_pos` takes WORLD coordinates for a non-HUD sprite — the engine subtracts the camera itself. Subtracting it again is invisible in a game whose camera travels 16px and hides every character off-screen in one that scrolls. ⚠️ four actors is a VRAM ceiling: 3 sprites each at 64x64 is 16 KB of 32 KB. ⚠️ peak ~12,300 ticks with four on screen — inline the module-array helpers (`clampX` was 8 boxed calls a frame) before blaming the algorithm |

Positive cross-genre reuse: `swing` (top-down melee) builds a pure shmup-combat entity
(`Transform|Collider|Hurt|Life`); the room camera serves both grid and platformer.

---

## ④c packages/*.tish — the shareable tish layer

| Package | Lines | Layer | What it is | Style |
|---|---|---|---|---|
| **engine.tish** | 425 | wraps engine + tish-agb | `makeEntity`/`mount`/`create` component-authoring layer + map loaders (`loadMap`/`loadSceneRom`/`loadStreamMap`) + hearts/dialogue helpers | `makeEntity` builds ~30 method-closures per entity per frame (documented cost, mitigated by a per-id cache); exposes a lean `tick` escape hatch |
| **shmup.tish** | 694 | raw engine ABI | shoot-'em-up genre kit: spawns, 7 enemy AIs, native emitters, boss, director | **the reference for the perf architecture** — typed structs, cvar slots, lean ticks, once-per-burst boxing |
| **scene.tish** | 140 | `transition.tish` | genre-agnostic scene-transition state machine, now with a pluggable effect (`sceneSetTransition` / `sceneGotoFx`) rather than a hardcoded fade | typed `SceneReg`; one documented boxing point. Default is still the 16-frame fade, so existing callers are unchanged |
| **transition.tish** | 377 | tish-agb blend/window/mosaic/canvas | 11 screen transitions: fade, white, iris, iris-at, box, wipe, curtain, bars, mosaic, rain, checker. `trApply(p, len)` is direction-free, so one effect serves both closing and opening | 9 of 11 are pure hardware (zero tiles, zero CPU). ⚠️ iris spends the single HBlank DMA slot; the two software effects paint the canvas via `ui_fill_cells` |
| **rhythm.tish** | 396 | tish-agb `deck_frame` + sprite setters | call-and-response rhythm kit: chart building, beat clock, judging, combo/score/"U Rappin'" meter, scrolling prompt lane | time is the **deck playhead**, never a frame counter — drift is unrepresentable rather than fixed. Owns the cue table (a typed `i32[]` PARAM arrives boxed and cannot be stored in a typed field), which is also why the lane is drawn here: it is the one thing that must walk every visible cue every frame, and accessors would cost a boxed call per cue per field |
| **feel.tish** | 671 | tish-agb `ui_rect`/`ui_scroll` + PSG | GAME FEEL: a feedback player. One `feelPlay(preset, x, y, magnitude)` fires a whole composed effect — hit-stop, a spring bump on the scroll registers, a burst ring, a rising call-out, a PSG note — from a row range in a table. Structured after More Mountains' FEEL: uniform per-feedback envelope (delay, duration, chance, cooldown, intensity), an emit/drain channel so an engine can broadcast without learning pixel geometry, and SPRINGS rather than tweens so impulses sum instead of overwriting | the split that makes it work on this machine is cost class, not subsystem: anything that draws pixels obeys KEYFRAMES (feelTick returns a bitmask of slots to repaint, zero on most frames); anything that moves without drawing — the scroll registers — is spring-driven per frame, because on GBA a screen shake is two register writes, not a redraw. ⚠️ **`examples/feel-demo` is the only thing in this repo that compiles it** — until that ROM existed the file's only consumers were in the sibling card-gba tree (`qb-view`, `qb-engine`, and an `fx-demo` since removed in favour of feel-demo), so chuggie shipped 670 lines it could not build. Standing it up caught `feelPlay` refusing an entire preset while on cooldown when its table says only the DRAWING is rate-limited — a cascade destroying four things shook once. feel-demo's verify.sh derives the export list FROM the package and fails if the demo does not call every one |
| **prefs.tish** | 224 | tish-agb SRAM | versioned, checksummed key/value save — genre-agnostic | the companion to save.tish's adventure blob, for games whose save is a handful of loose numbers |
| **link.tish** | 245 | tish-agb `sio_link_*` | two GBAs on a cable, in lockstep: seed handshake then one word per frame | works only because the rules core is deterministic; `tools/gba-link` tests it with two cores in one process |
| **title.tish** | 167 | tish-agb + bundled font | RPG title screen (bg + font + cursor menu) | older/boxed style (untyped config, `pick()` defaults, closure-captured state) — predates the typed conventions |
| **cutscene.tish** | 291 | wraps engine/tish-agb + dialogue | blocking `cut*` calls: wait, say, **choose** (returns the index), walk, face, **fade in/out**, **camera pan**, **named story flags** | imperative, but no longer *forced* to be — `examples/probe-arrayret` shows array returns work on device, retiring the untested #553 claim. Three hooks (`cutSetAnim`/`cutSetMover`/`cutSetStep`) let raw-sprite games use it; it previously required engine entities, which silently excluded both isometric examples |

Two authoring paths coexist: **engine.tish `makeEntity`** (16 examples — the mainstream for
tilemap/world games) and **raw-engine-ABI genre kits** like shmup.tish (newer, perf-first, one genre).
Neither supersedes the other; they target different game shapes.

---

## ④d examples/* — 99 dirs

Grouped by genre / layer (see the examples pass for the full per-example table):
- **shmup**: shmup (reference), asteroids (rotate-and-thrust arena) — on packages/shmup; the flagship campaign moved to its own repo.
- **platformer**: sunny-land (reference), platformer-combat, dark-hero, platformer-scroll, platformer-rooms — on engine.tish + per-example Player controllers.
- **top-down**: akari (flagship), ninja-village, ninja-adventure — engine.tish `loadScene`.
- **grid**: overworld-demo — engine.tish `loadMap`.
- **puzzle**: grid-demo (floor-stacked match-3 on packages/grid.tish, rising garbage floor, `gridFeed`/`gridAnyOver`), blockfall (falling-block on the guideline ruleset — SRS kicks, 7-bag, hold, ghost, T-spins, and a budgeted search that plays it). ⚠️ blockfall does NOT use grid.tish: that kit models packed columns and a falling-block game is made of the holes. ⚠️ its frame budget is TILE WRITES (~310 ticks each), not rules or AI — the falling piece is sprites for that reason.
- **rhythm**: rap-dojo (sole) — on packages/rhythm; the song, the chart and the input schedule its
  verifier drives are all generated from one table, and the fake-3D floor is `bg_bands` (each depth
  band's tile width and scroll rate both scale as 1/depth).
- **card / cold screen**: solitaire (Klondike, sole) — no package and no art; every card is `ui_rect` + `ui_text`.
  ⚠️ the damage gate is PER PILE, not per table: a whole-table repaint is ~20 frames (a third of a second of lag on a
  cursor press), a two-pile repaint is 2. ⚠️ clear-then-draw FLICKERS — cards are opaque, so overdraw and erase only the tail.
  ⚠️ `ui_text` is priced per CALL, not per glyph — rank and suit are ONE string from a 52-entry label table.
  ⚠️ an attract/AI move generator that only moves a WHOLE face-up run can never split a King out of one, and the obvious
  cycle guard ("the move must expose a face-down card") silently re-imposes exactly that, because face-up cards are a
  contiguous run. Ration non-productive moves instead, and assert the move HAPPENS — not just that the rule allows it.
- **kart racing**: kart-circuit (sole) — on packages/kart + packages/mode7; the track art, the
  surface map the physics reads, the AI's racing line and the lap gates all come out of ONE spline in
  `scripts/gen_kart_circuit.py`. ⚠️ an affine map addresses tiles with a **u8**, so 256 unique tiles
  is a hard, SILENT ceiling — a freehand-painted course came to 925 and agb paints tile 0 over the
  excess without a word; the generator autotiles to 153 and prints the count.
- **link cable**: link-demo (transport diagnostic — states, seed, button mirror, round trip);
  pong-link (a real lockstep GAME over it, and the example that found three desync bugs the mirror
  could not: a handshake word leaking into gameplay, pairing the peer's answer with the wrong local
  frame, and a symmetric round scheme that drifts by one and deadlocks). ⚠️ a lockstep desync is
  INVISIBLE on one console — both screens look like a normal game — so the ROM logs its simulation
  state and verify compares the two consoles' lines for the same frame.
- **narrative**: iso-cutscene (fade / camera pan / branching choice / story flags on the iso board;
  the case that forced `packages/cutscene`'s entity coupling open), dialog-demo.
- **probes**: probe-arrayret — settles whether a tish fn can return an array on device (it can).
- **SRPG family** — the isoboard SRPG game and its ~44 subsystem examples (town, battle, UI,
  progression, persistence, audio, campaign integration) were extracted to the sibling
  downstream project, which builds against this engine's crates (`isob_*` natives,
  `packages/iso`, the `isoboard:` scheme). See that repo's INVENTORY.md for the family map and its
  verify.sh discipline.
- **tech/demo**: iso-sprite, engine-demo, collect-demo, mono-demo, anim-demo, bg-demo, input-demo, asset-sprite, dpad-sprite, minimal, title-demo, fonts-demo, bench-ai, bench-entities, repro-structwrite, p0-spike.

`package.json` shape is uniform (build/start/shot/clean, `@tishlang/tish` file link, `tish_agb`
rustDep; `tish_gba_game_engine` + `tish_gba_scenepack` added exactly where used).

---

## Consistency backlog (prioritized)

Ranked by architectural impact. These are the drifts from "one consistent architecture."

### P1 — architectural
1. **The isoboard subsystem is a second engine bolted alongside the first.** `IsoBoardGrid`/`IsoBoardUnit`/`IsoBoard` use
   their own globals, duplicate hp/max/team/speed (parallel to `Health`+`tag`), have an immediate
   death model (vs the World's deferred `onDeath`), use no SoA/mask/generational-ids/cvar, are not
   driven by `world_step`, pack fields as u8/i16 (World is i32-everywhere), and are all boxed with zero
   typed twins. **Decide:** either (a) formally bless it as a standalone "logical board" subsystem and
   document that in ARCHITECTURE.md, or (b) fold units onto the SoA World. Either way it should be a
   deliberate, documented choice — today it's silent divergence.
2. **Genre packages: platformer DONE, isometric DONE, top-down partial, grid and isoboard-battle outstanding.**
   ✅ `packages/iso.tish` now owns the isometric projection, depth biases, raised-block redraw and
   camera clamp, and `packages/iso_actors.tish` the 32px sheet frame contract. Both were verbatim
   copies in the SRPG examples that had **already drifted**: `UNIT_LIFT` was 12 in one and
   20 in the other (20 is correct — the art is byte-identical and its feet sit 8px below where 12
   puts them), and one demo hardcoded the bake origin, pinning it to boards of about 8×8. The
   remaining isoboard work is the battle controller itself.
   ✅ `packages/platformer.tish` now exists (`oakhollow` is built on it) and covers walk/run, the
   jump-feel windows, crouch/slide, ladders, ledge grab and pull-up, and wall slide/jump, plus
   data-driven NPC/sign/door components. The five older platformers still carry their own copies and
   are worth migrating.
   ✅ **Top-down is extracted.** `packages/topdown.tish` grew from one stick machine into the genre
   kit: a second mover (`topdownLaneDrive`, which the topdown RPG port's hero controller used to own
   alone — that file is 35 lines now and says only how big the hero's box is and how fast he walks),
   the facing→offset and pixel→tile→room arithmetic that had **seven** copies in that port plus
   duplicates in akari, the warp latch both games hand-rolled, and `tdContextAction` — whose return value the port
   was discarding, so pressing A beside an NPC talked to them *and* swung a sword through them on the
   same frame. Room streaming is deliberately still per-game; see
   [`docs/topdown-genre.md`](docs/topdown-genre.md) for what is owned, what is not, and why.
   ⚠️ the SRPG integration example still hand-rolls 763 lines. Still to
   extract: **`packages/grid`** and a battle-controller package. `packages/drop` now covers
   the puzzle genre, and `tilemap_set` (added for it) is the per-cell background write any future
   `packages/grid` will want — a sprite per cell is not viable, see the measurements in
   `crates/tish-agb` `native_tilemap_set`.
3. **Two authoring layers, no stated guidance.** `engine.tish` (makeEntity) vs raw-ABI genre kits.
   Document the decision matrix (makeEntity for world/RPG games; raw-ABI genre kit for arcade/
   high-density) so new games pick deliberately.
4. **Three map-loaders in one layer** — `loadStreamMap` (platformer), `loadScene(Rom)` (top-down),
   `loadMap` inline arrays (grid). Unify onto the ROM-baked `scene:`/`map:` path or document when each
   applies; there's also a grid gap (no grid example uses the streaming/`scene:` path).

### P2 — ABI hygiene (cheap, high consistency value)
5. **`#[tish_export]` contract-vs-reality gap.** CONTRACT §3 specifies binding crates carry no `Value`
   code and mark exports with `#[tish_export]` for the compiler to bindgen; **reality**: both crates
   hand-write ~40 boxed fns + hand-commit `tish.d.tish` (zero `#[tish_export]` in the tree). Either
   implement the macro-driven path or update CONTRACT.md to describe the hand-written ABI as the
   intended design.
6. **Typed-extern coverage gaps.** `damage_typed` and `sprite_set_flip_typed` exist in Rust but are
   **not declared** in `tish.d.tish` (unreachable orphans — add the two lines). Hot per-frame calls
   still boxed with no typed twin: getters `grid_facing/col/row`, `topdown_facing/moving`,
   `platformer_grounded/blocked/vy`, `entity_tag`; and `camera_set`, `key_released`,
   `sprite_set_visible/depth`. Add typed twins for the ones read/written every frame.
7. **Scheme-registration asymmetry.** Asset schemes split across two conventions — facade
   `__asset_register_*` (asset/sheet/bg/wav/map) vs scenepack `{mod}::__*_register` (scene/isoboard/
   font/emoji). Sheet sizes are still duplicated (`sheet`/`sheet32`/`sheet64`/…) because
   those specs can't carry a size parameter; fonts use `font:path@N` instead.

### P3 — cleanup
8. **Dead/legacy code:** the hardcoded red demo sprite (`sprite_create` + `SPRITE*`) threads a
   `sheet==-1` special-case through several fns; the array-driven `tilemap_new/terrain/stream*` paths
   are superseded by the ROM-baked `map_stream`/`scene_stream` (keeps the map off-heap).
9. **Stale comments / minor drift:** `cvarf`/`set_cvarf` are referenced in comments but don't exist;
   knockback logic is duplicated (inlined in `combat_system` vs `topdown_knockback`); `Mover.pattern`
   is a `u8` with only 2 values while every other small enum is i32.
10. **No enforcement of mutually-exclusive movement models** (Body vs Platformer vs TopDown vs GridPos)
    — convention only; a debug assert would catch double-attach.
11. **Examples polish:** ninja-adventure/ninja-village (and mono/anim-demo) lack READMEs; three demos
    carry a stray `tish-agb-` npm-name prefix + `version` field others don't; no dedicated **audio**
    example; `packages/scene` and `packages/cutscene` are each used by only 1 game.
12. **Naming:** ABI is snake_case but config/output keys + comments are camelCase (`onCollide`,
    `jumpCut`, `defineComponent`) — a tish sugar layer bridges them; keep the bridge consistent.
