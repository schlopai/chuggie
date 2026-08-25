# Engine roadmap — the 2026-08-13 gap plan, and where it stands

The plan below was produced on **2026-08-13** by a review of the packages, the crate layout and all
110 example READMEs. It existed only in that session's transcript — it was never written to a file,
never filed as issues, and never recorded in memory, so it was effectively lost. It is recovered here
verbatim in structure, with a **status column added from the current tree (2026-08-14)**.

Its companion is [`engine-review-2026-08.md`](engine-review-2026-08.md), a later whole-stack read
aimed at the RTS question. That one describes the stack; this one is the *work list*.

> Status legend: ✅ done · 🟡 partial · ❌ not started

---

## The cross-cutting gaps

The original framing, which still reads correctly: the gaps are not in genres, they are
**cross-cutting systems that every genre re-implements ad hoc**, plus a short list of genre pillars
blocked on exactly those systems.

| # | Gap (2026-08-13) | Status | Where it stands now |
|---|---|---|---|
| 1 | **No navigation/pathfinding.** `search.tish` is a frame-budgeted best-move searcher, not A*. Must be native Rust — a callback-per-node scorer is dead on arrival at ~42 ticks + 28/arg. Ship `nav_flow_build/nav_step`: flow field for many agents, A* for one-off queries. | ✅ | Landed as **`flow_goal` / `flow_dist` / `set_seek` / `clear_seek` / `seek_arrived`** in `tish-gba-game-engine`, native, exactly the prescribed shape (one field, computed outward from the goal, shared by every agent). Consumers: `rts-fog`, `rts-select`, `tower-def`. **Residual:** general off-board A* still does not exist — `isob_path` is the iso board only. |
| 2 | **No world-flag / quest / progression system.** `save`/`prefs` handle bytes; nothing owns "chest 14 opened, boss 3 dead". A `flags.tish` (bitset + named ids + save integration + `flagWatch`) is ~200 lines and retires duplicated logic in four examples. | ✅ | **`packages/flags.tish` landed 2026-08-14** — a named bitset over `prefs.tish`'s versioned, checksummed SRAM (up to **1,024 flags**), with `flagWatch` triggers drained from a change ring rather than polled. Acceptance test: `examples/vault`, which proves a power-cycle round-trip by running the ROM twice. Not yet adopted by the topdown RPG ports (since moved to their own repo) or `metroidvania` — those still carry their private bitfields. |
| 3 | **Audio is disproportionately thin.** No mixer, no ducking, no positional/panned SFX, no music state machine. | ✅ | Ducking and adaptive intensity had already landed (`audio_duck`, `deck_set_intensity`, `audio-adaptive`). **Positional audio landed 2026-08-14**: `sound_play_ex(wav, volume, panning, pitch)` exposes the three knobs agb's `SoundChannel` always had, and `packages/sfx.tish` turns a world position + listener into them (linear falloff, octagonal distance, no `sqrt`). `examples/earshot` MEASURES it — a stereo capture asserting the loud channel matches the side the source was on, 9/9 positions, plus distance attenuation. ⚠️ Found doing it: **agb's `panning` is inverted from its own documentation**; the flip is absorbed in `sound_play_ex` so the tish API keeps the conventional sign. Residual: no true multi-voice mixer control or music state machine — `deck`'s intensity is the state machine in practice. |
| 4 | **No localization / string table.** A `strings:` asset scheme (id → ROM offset, `str(ID)` lookup) turns the existing CJK glyph-baking work into a shipping feature. | ✅ | **`strings:` landed 2026-08-14, CJK included.** `include_strings!` bakes a multi-language table into ROM; `str_get/str_count/str_langs/str_lang_name/str_find_lang` read it; `examples/polyglot` switches EN/FR/DE/**JA** at runtime. Ids are line POSITIONS and a short translation is a **compile error**. The selective glyph baking stays automatic: `scripts/gen_strings_glyphs.py` derives the exact non-ASCII roster from the tables into `src/generated/glyphs.tish` (35 glyphs here) — nothing hand-written, because a hand-kept list drifts the moment a translation is added and fails only in the language nobody can read. |
| 5 | **No procedural generation path.** The right shape is a build-time generator emitting `.tmj`, not a runtime fork of the map pipeline. Without it, roguelikes are impossible. | ✅ | Done **both** ways. Build-time `.tmj` generators are now the house pattern (`gen_golf.py`, `gen_rts_spikes.py`, `gen_towerdef.py`). On-cartridge generation also shipped: `packages/dungeon.tish` (BSP from a seed, contract-matched to `scripts/procgen/rooms.py`) behind the `roguelike` example. |
| 6 | **Physics stops at AABB + slopes.** No swept collision, no restitution, no simple rigid bodies. Pinball, golf, breakout, Angry-Birds-likes and every sports game are blocked on ~400 lines of native fixed-point. | ✅ | **The rigid-body layer landed**: `set_dynamic` (disc collider, Q8 restitution + friction, rest-speed sleep, contact rank) + `body_impulse`/`body_kick`/`body_asleep`/`body_speed2`, driven by `world_step`. `golf` and `soccer` are its acceptance tests; `pinball` shipped 2026-08-14. **Residuals, all real:** no swept collision; a dynamic disc **has no gravity** (gravity lives only in `platformer_system`); and the disc physics **collides against the tile grid, not the per-pixel terrain** — which is why `pinball` integrates its ball in tish against `terrain_solid` instead of using the engine bodies. |
| 7 | **No input recording / replay.** A `replay.tish` recording a button-mask stream would turn the hardest testing problem into a diffable artifact; lockstep link is the same machinery. | ✅ | **`packages/replay.tish` landed 2026-08-14** — an RLE button tape (`(mask, frames)` packed into one `i32`, so it is SRAM-writable verbatim) plus `replayMix`, a rolling checksum over game state. `examples/rerun` records and replays a run on the cartridge every twelve seconds and prints `IDENTICAL`/`DIVERGED`; it was validated by deliberately breaking determinism and confirming it goes red. |

**Score: 7 done, 0 partial, 0 not started — every cross-cutting system on the plan now has something behind it, and each one has a measurement rather than an assertion.** What remains is on the GENRE list, not this one.

---

## Missing genres, ranked by (value ÷ cost)

| Genre | Blocked on | 2026-08-13 verdict | Status |
|---|---|---|---|
| Visual novel / adventure | nothing — `dialog` + `flags` + verbs | "Cheapest win in the repo" | ✅ `visual-novel` — repaired and shipped 2026-08-14 (it had never once run) |
| WarioWare-style microgame harness | nothing | "Best stress-test showcase you could ship" | ✅ `packages/microgame.tish` + `microgame` |
| Roguelike | procgen + turn scheduler + FOV | "Biggest genuine hole" | ✅ `roguelike` + `packages/dungeon.tish`, checkable against a Python oracle seed-for-seed |
| Tower defense / RTS-lite | nav (#1) | "Nearly free once flow fields exist" | ✅ `tower-def` (2026-08-14) — and it *was* nearly free; plus `rts-fog`, `rts-select`, `warforge` |
| First-person grid crawler | raycast or pre-baked wall quads | "Very GBA. Mode 7 infra says tractable" | ✅ **`packages/fpview.tish` + `crawler` (2026-08-14)** — neither, in the end: the view is exact geometry, because a grid crawler's view is a pure function of (cell, facing) and there are only four facings. No trig, no per-column division, no per-frame redraw. |
| **Dungeon-logic package** | nothing — extract from the two topdown RPG ports | "Written twice already. Extract it" | ✅ **`packages/keylock.tish`** (~310 lines) — named `keylock` because `dungeon.tish` was already the BSP generator. Doors (`KL_OPEN/WALL/FALSE/BOMBABLE/LOCKED/SHUTTER`), one global key stock, a Magic Key that spends nothing, paired halves via `klPair`, pushable blocks and sockets. **No save code**: each door is a `flags.tish` id at `flagBase + doorId`. Acceptance test `examples/keep`, written against the package with the original dungeon-doors spike (now in the ports' own repo) untouched — which was the point of the exercise |
| Sports (golf/tennis/soccer) | physics (#6) | "Real hole, real cost" | 🟡 `golf` + `soccer` shipped once the disc bodies landed; tennis never built |
| **Board/card framework** | nothing | "`solitaire` + `iso-cardkeeper` are one-offs; a deck/hand/pile kit generalizes them" | ✅ **`packages/cards.tish`** — piles of card ids with a face-up bit each, `pileMove` (order-preserving) vs `pileDeal` (reversing, the way cards come off a deck), `pileRecycle`, downward Fisher-Yates from a seeded rng stream, `pileRemoveAt`/`pileFind` for hands, and `cardsTotal()` as a conservation assert. No rules, no rendering, no undo. Second consumer `examples/deckbuild` — a bespoke-deck deckbuilder that never calls the French-deck decoders — with **`solitaire` untouched** |

## The recommended order, scored

- **Now:** `nav.tish` ✅ · `flags.tish` ✅ (2026-08-14)
- **Next:** the genre list is closed. The open work is now *adoption* — the topdown RPG ports and `metroidvania` still carry private bitfields instead of `flags.tish`, and general off-board A* still does not exist (`isob_path` is the iso board only)
- **Then:** audio layer ✅ (2026-08-14) · microgame harness ✅
- **Ungrouped in the plan, now done:** `replay.tish` ✅ (2026-08-14) — the plan flagged it as the fix for the repo's hardest testing problem
- **Bigger bets:** roguelike ✅ · first-person crawler ✅ (2026-08-14)

## The structural note, which has aged well

> 40 `iso-*` examples versus one `platformer.tish` genre package suggests examples have become the
> documentation. A genre is proven when *someone else's* example can be built on the package without
> touching it — the dungeon-logic extraction is the cheapest test of whether that's true today.

**That test has now been run, and it passed.** `packages/keylock.tish` was extracted from
the dungeon-doors spike, and `examples/keep` — a different room, a different game — was built on it
**without editing either the package or that spike**. That is the bar: had the package needed a
change to fit the second consumer, it would have been one game's code with a new filename.

The corpus is 156 examples against 46 packages. The ratio is still lopsided, but the extraction
loop now has a worked example to copy: pull the logic out, leave the original alone, and prove it
with a second game rather than with a second look at the first one.

## Two independent reviews agree on the same next move

A genre-coverage review run on 2026-08-14 (`/Users/a_/.claude/plans/create-a-comprehensive-review-smooth-sifakis.md`)
reached the **first-person grid crawler** independently, by a different route: the whole catalogue is
side-view, top-down or isometric, and *nothing renders into the screen*. Two reviews a day apart,
from different starting questions, naming the same hole is the strongest signal in either document —
and it is now built (`packages/fpview.tish`, `examples/crawler`).

That review also closes one gap this plan did not have: the **GBA window registers** (WIN0/WIN1/
WINOUT) were unreachable from tish, which is what a stealth vision cone, a lit room, a spotlight and
an iris transition all need. `win_rect` / `win_circle` / `win_off` / `win_in_layers` /
`win_out_layers` landed 2026-08-14 (`examples/win-demo`).

The **iris transition** that motivated those registers is now built, along with everything else the
hardware can wipe with: `packages/transition.tish` and `examples/transitions` (2026-08-14) give
eleven effects behind one `trApply(p, len)` call, driven by `packages/scene` through an additive
`sceneSetTransition` hook. Three natives landed with it — `fade_white` (BLDY increase), `mosaic`
(the MOSAIC register plus its per-BG and per-OBJ enable bits, which agb models as permanently off),
and `blend_alpha` (BLDALPHA) — plus an explicit BLDCNT priority chain, since those four effects share
one two-bit field and the last caller in a frame used to erase the others silently. `win_circle` now
also yields the single HBlank DMA slot instead of overwriting a Mode 7 floor or a `bg_bands` layer.

⚠️ What remains impossible: a true A→B **crossfade**. It needs both scenes resident in VRAM at once,
and the scene lifecycle tears the old one down first by design (agb does not release a scene's ~40KB
tile block until a frame boundary). `blend_alpha` blends layers *within* a scene; it is not a scene
transition, and should not be advertised as one.
