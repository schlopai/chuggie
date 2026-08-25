# UI system roadmap

Reference for features to add to [`ui.tish`](./ui.tish) (and closely related packages). Not a commitment to order — pick by game need.

## Already shipped

| Area | What exists |
|------|-------------|
| Layout | Flex-lite: `dir`, `gap`, `pad`, `justify`, `align`, `w`/`h` |
| Leaves | Text (`ellip`, `wrap`, in-box `align`, `shadow`), icon (sprite pool), panel `fill` / `border` |
| Theme | Token map via `uiInit({ theme })`; font tokens `font` / `titleFont` / `tinyFont` + `uiFont()` |
| Lists | `makeSelector` (+ `select()`), `makeCursor`, scroll + scrollbar |
| Grids | `makeGrid` — icon bag cells (qty/label, empty slots, focus chrome, scrolling `h`); used by `rpg-menu` |
| Buttons | `makeButton`, `makeButtonGroup`, `buttonStyle` / `buttonPaint` |
| Prompts | `makePrompt`, `makePromptBar` |
| Chrome helpers | `uiOverlay`, `uiModal`, `uiReveal` / `uiRevealWarm` |
| Focus stack | `uiPush` / `uiPop` / `uiTop` / `uiDepth` / `uiStackPaint` / `uiStackUpdate` — pause + file overwrite in [`menu.tish`](./menu.tish) |
| Dirty updates | `uiSetText`, `uiRowText` / `uiBlankText`, `uiRepaint`, stream paint |
| Profiling | `uiInit({ stats: 1 })` + `uiLayoutStats()` — per-pass ticks for one render |
| Screen paint | `uiPaint` / `uiPaintStep` — one-shot or streamed open (wraps `uiStream*`) |
| Cursors | `makePointer`, `makeArrow` |
| Shop-grade chrome | `makePanel` (+ `titleAlign`), `makeListRow`, `makeDetailPanel`, `makeStepper`, `uiKeys` |
| Options widgets | `makeToggle` / `makeToggleGroup`, `makeTabs`, `makeBar`, `uiAct`, exported `BTN_*` |
| Pause / file select | [`menu.tish`](./menu.tish) + [`save.tish`](./save.tish) — streamed open, in-place cursor |
| Options screen | `optionsMenu` / `optionsWarm` — toggle / picker / action rows, one slot per value |

Related but separate: [`dialog.tish`](./dialog.tish) (conversation boxes), [`shop.tish`](./shop.tish),
[`menu.tish`](./menu.tish) / [`save.tish`](./save.tish) (pause + file select + SRAM), engine `text_draw` / `hud_bar`.

---

## Performance contract (SRPG / action-RPG GBA parity)

The reference for **feel** is the commercial GBA SRPGs and the commercial GBA top-down
action-RPGs. Not their art — their responsiveness. Three reference-SRPG screens define what we have to match:

1. **Equip / item browse** — category tabs, item list, stat preview, detail box. Moving the list
   cursor updates highlight, stats and detail **together**; nothing "loads later".
2. **Unit menu** — fixed chrome (portrait, HP, stats, MENU list) plus a HELP box whose text follows
   the selection. A cursor move is a highlight change plus one string patch.
3. **Field dialog** — parchment box + portrait over the live map, clan-funds HUD still up. The box
   appears and disappears with **no map rebuild**.

The action-RPGs add: Start → item screen is instant, area/door transitions are short, HUD is always resident.

### Budgets

| Event | Budget | Meaning |
|-------|--------|---------|
| Area / New Game / warp to first painted map | **< 1s** wall clock | A short transition, not a white blank |
| Mode open (pause, file select, menu screen) once warm | **≤ 1 frame** | Show chrome that is already in VRAM |
| **Change selected menu item** | **< 0.2s** (~12 frames) | Cursor *and* every dependent panel (help, stats, detail, icons) |
| Dialog box show / hide | **≤ 1–2 frames** | Tavern-talk feel |

### Rules that follow from them

1. Selection change **never** calls `ui_begin` or paints the whole tree. Move the cursor, recolour the
   one or two affected rows, patch the dependent slots — all on the **same press**.
2. Don't debounce a dependent panel past the budget. A patch of an already-painted panel is cheap
   enough to do immediately; only a first, full paint may wait, and then only a couple of frames with
   a hard deadline (see `DETAIL_SETTLE` / `DETAIL_DEADLINE` in [`shop.tish`](./shop.tish)).
3. Canvas DynamicTiles are for dense mode **entry**. HUD / pause / help lines are retained sprites or
   in-place `ui_text_box`.
4. Tear VRAM down only on scene load. Dismissing a mode hides or blanks.
5. A selection press must not spike `frame_stats` render/period into the hundreds of ms. Bracket a
   suspect section with `ticks()` when it does — whole-frame maxima can't say which part was slow. For a
   whole render, `uiInit({ stats: 1 })` + `uiLayoutStats()` already brackets every pass (below).

### Where a render's time goes (`uiLayoutStats`)

`frame_stats()` says a press blew the budget; it cannot say which pass did. `uiInit({ stats: 1 })` times the
five passes of a `uiRender` and `uiLayoutStats()` returns them, e.g. `ui-demo`'s streamed tab open:

```
ui: n=32 flat=30871 meas=6168 arr=13889 wb=26324 paint=61284 tot=138536t 528ms
```

Read it as: 32 nodes; `flat` = flatten the boxed tree into the typed Vec, `meas` / `arr` = the native flex
passes, `wb` = write geometry back onto the boxed nodes, `paint` = glyphs + rects + icon sprites. Ticks are
Timer2 (1 vblank ≈ 4389, ≈262/ms). Paint accumulates across a streamed open, so `tot` is the screen's cost.

**Boxed node handling is the bill, not the flex math.** In that line `flat + wb` is 41% and `meas + arr` is
15% — the arithmetic we tuned into native `i32` is the cheapest part, while merely reading and writing object
fields costs three times more. That is the whole argument for `uiBake`: a baked screen skips flatten,
measure, arrange AND write-back, leaving only paint.

Two things to know before trusting a number. Each pass laps every 8 nodes rather than end-to-end, because
Timer2 wraps every 65536 ticks (~250ms) and a dense screen's pass runs longer than that — timed end-to-end it
would silently lose a whole wrap. And the ticks include the `audio_pump()` each pass makes every 8 nodes:
real work, but not layout. Sanity-check the scale against wall clock — that 528ms sat inside a 39-frame
(653ms) open, the rest being the per-frame vblank waits a streamed open pays by design.

### agb realities to design around

- `InfiniteScrolledMap` copies 2 tile-rows per `set_scroll_pos`, so waiting on the frame loop for an
  initial fill is a multi-second blank. Burst-fill it instead (`prime_stream_layers`).
- `DynamicTile16::new` / `Object::to_vram` are real allocations. Thrashing them per cursor move blows
  the budget; retain and patch.
- The BG canvas is **not** double-buffered, so a patch costing more than one frame of CPU is *visibly*
  painting in. That makes "how many frames did the screen keep changing" a direct budget measurement —
  `GBA_SHOT_TRACE=1` reports exactly that, with no ROM instrumentation.

Forking agb is not required for any of this.

---

## Performance work (done)

### Bake text metrics
- `font:path@N` emits `FontMetrics` (per-glyph advances) beside the agb `Font`.
- `text_width` / `text_wrap_height` sum advances / greedy-wrap instead of agb `Layout` for imported fonts.

### Canvas tile lookup (the selection-budget fix)
- The UI canvas's tiles were a `BTreeMap` keyed by tile (col,row). Every in-place text patch does one
  lookup **per tile column per pixel row**, and on the GBA's uncached bus a B-tree descent cost ~4000
  cycles — a 3-line description repaint alone was ~80ms, and one shop cursor move was ~230ms.
- Now a flat 32×32 `u16` grid (`ui_cell`) indexing a dense tile list: ~30 cycles per lookup. Kept at
  `u16` deliberately — a 12KB grid of `Option<DynamicTile16>` OOMs the heap on a dense shop screen.
- `ui_text_box` writes a tile's rows in one **run** per tile instead of resolving per pixel row, skips
  the read-modify-write on full-tile masks, and fills blank nibbles branch-free.
- Net: shop cursor move ~230ms → ~85ms, inside the <0.2s selection budget.

### Shared shop chrome
- Lifted into `ui.tish`: `makePanel`, `makeListRow`, `makeDetailPanel`, `makeStepper`, `uiKeys`/`uiKeyDown`.
- `shop.tish` consumes those; buy/sell state machine stays in shop.
- **Nav patches the detail on the keypress frame** (SRPG-style) whenever the panel is already painted.
  Only the first fill after a deferred tab open waits, and `DETAIL_DEADLINE` force-flushes it so
  holding the d-pad can't leave a placeholder up.
- `keys_edge()` batches the whole pad in one ctx read; stream paint pumps audio every 2 nodes.
- **List scroll** rebuilds only the left panel (`uiRelayoutAt`) — not a full `renderTab`.
- Nothing re-renders a whole tab after the open: a flashed message and its expiry patch the footer, a
  buy/sell patches gold + footer + right panel (+ the list only when selling changed it).

### Pause + file select
- `menu.tish` is the framework pause / save-load UI (with `save.tish` for SRAM peek/write/load).
- **Pause is Sprite text** (`text_draw` + `text_visible`) — not UI-canvas `DynamicTile16`. Warm with `pauseWarm` after scene load; open/close only flips OAM visibility. Measured open: ~3 frames including emulator key latency.
- **File select is the same retained-sprite path.** Every line owns a fixed `text_draw` slot; `fileWarm`
  rasterizes them from the **title screen** (nothing is on a clock there) and leaves them hidden, so
  New Game / Continue opens the picker in ~2 frames. Lines are drawn with the theme `bg` as a shadow so
  the picker reads over title art without a canvas panel behind it.
- Cursor move redraws only the four affected lines (old + new file's label and detail); slot peeks
  happen on warm/open, never on a d-pad press.
- `fileFree()` returns the picker's Sprite VRAM once a slot is taken (the game needs those tiles); a
  cancel only hides, since the player lands back on the title where a re-open should be free.
- The overwrite prompt is still a canvas box — the file lines hide while it is up, since sprites draw
  in front of the canvas.
- Entering on an A press means the press is often still held now that the open is fast: the picker
  latches A and waits for a fresh press.
- Input via **`uiKeys`** / `navBits`.

### Baked layout (`uiBake` / `uiReplay`) — solve a screen once, replay it forever
The layout of a screen is not news at runtime. Measured on a nine-node dialog box, one `uiRender` costs:

| pass | ticks | what it is |
|---|---|---|
| flatten | 7,454 | reading the boxed object tree |
| measure | 1,363 | native i32 flex math |
| arrange | 3,523 | native i32 flex math |
| geometry write-back | 6,183 | six boxed writes per node |

18.5k ticks (4.2 frames) for **nine** nodes, and 74% of it is boxed traversal, not the flex solve — about
2k ticks per node per open. A 60-node shop tab is therefore ~27 frames of layout before a glyph is drawn,
which is why tab entry had to stream. Per-open layout is not a viable way to build screens.

So a screen is solved **once** and compiled to a flat **display list**:
- `uiBake(id, root)` — run the solver one time, then emit one native op per draw, in paint order:
  `kind, x, y` plus five packed fields (rect: w/h/colour · text: font/colour/wrap/align/string slot ·
  icon: frame). Strings live in a side table. Returns 0 for a tree containing a `scroll` container, whose
  children genuinely are culled against a live offset.
- `uiReplay(id)` — `ui_begin` + walk the op array. No flatten, no measure, no arrange, no write-back, and
  no boxed reads in the paint either (`paintNode` cost ~8 property lookups per node).
  `uiReplayBegin`/`uiReplayStep(budget)` spread a dense screen's glyph blitting over frames.
- `uiBakeText(node, s)` / `uiBakeIcon(node, frame)` — the only per-open work a "dynamic" screen needs:
  swap the string or frame in its slot, at geometry that never moves. Two array writes.

Rules for authors (this is what makes a screen bakeable):
- **Build the tree once and keep the node refs you patch.** The bake writes geometry onto those node
  objects, so `uiRowText` / `uiReveal` / `uiReframeIcon` keep working on them; a rebuilt object literal
  gets nothing.
- **Give anything whose content varies a fixed `w`/`h`.** A content-sized label re-anchors when the string
  changes length, and an opaque in-place rewrite needs a box to cover.
- Quantity may vary (row counts, string lengths); geometry may not. That is the contract.
- The op pool is packed (stride 8, shared across bakes) and a baked shape **drops its tree** — containers
  and spacers are dead weight on a 256K heap once the display list exists.

### Dialog page open / choices
- **Chrome is baked per box SHAPE.** The shape is `speaker? · portrait?/size/side · pos · reserved body
  height · choice count · multi-page · font · body width` — everything else (the name, the face, the page
  text) is a string or a frame drawn into a fixed box. First box of a shape solves + bakes; **every later
  box of that shape is a replay plus a few slot writes**. Measured across the five demo styles: first open
  ~30-36k ticks (one-time), later opens ~5-8k ticks, box on screen in 1-2 frames either way.
- `dialogWarm(text, opts)` pre-solves a shape (same options as `dialogSay`) without touching the canvas,
  so even the first conversation of a scene replays. The reserved body height quantises to whole lines, so
  warming with any two-line string covers every two-line box of that shape.
- **Resident shapes are capped** — `dialogInit({ cacheShapes })`, default 3, one shape ≈ 20K (see "Memory is
  the other budget"). Body height is part of the shape, so a scene with 1-, 2- and 3-line boxes has three;
  when the cap is reached the cache flushes and the next box of an old shape re-solves. `dialogFree()`
  releases the lot for a screen that needs the heap more.
- Streamed paint (`uiStreamBegin`/`uiStreamStep`) remains as the fallback for a shape that won't bake.
- **Chrome is retained across pages.** The body box reserves the height of the box's *tallest* page, so
  the window never changes shape mid-conversation, and a page turn writes the new text opaquely into
  that reserved box (`uiRowText` with the panel fill) — no `ui_begin`, no re-layout, no re-measure.
  Measured: page turn ~39.6k ticks over 10 frames → ~7.7k ticks in one call (~1.8 frames).
- **The continue arrow has no layout row.** It is a sprite, so it parks in the bottom-right corner of the
  box's CONTENT COLUMN (`placeArrow`, computed once per shape from solved geometry) and rides the frame the
  way the reference SRPG's does. The row it used to reserve was an empty line of window height on *every* box — the shop
  greet was half the screen for one line of text and three choices. Anchored to the column, not the box, so
  a right-side portrait doesn't get the arrow on its face.
- Painting the choice slot must not touch the icon pool: `uiRelayoutRows` used to reset the pool cursor and
  hide everything past its own rows, which hid the dialog PORTRAIT the moment the choice menu appeared.
- Last page **reserves choice-slot height** while typing; `finishTyping` paints labels in place with
  `uiRelayoutRows` — an opaque per-row write, because `uiRelayoutAt`'s tile-aligned `ui_clear_rect`
  sheared the bottom pixel rows of the body paragraph directly above the menu.
- The over-map HUD stays live: dialog only touches the UI canvas and the icon pool, never `hud_text` /
  `hud_hearts`.
- A paragraph that reserves a fixed `h` now skips `text_wrap_height` in both layout paths — that call is
  a full agb shaping pass and the reserved height already wins.
- Phase 1 (continue arrow) no longer re-renders at all (slot already reserved).
- Input via **`uiKeys`** + `makeCursor.navBits` / `actBits`.
- Shop greet uses **`instantChoices: 1`** so Buy/Sell/Leave lands with the last page in one stream.
- Still open: the glyph/tile blitting itself (~8-12k ticks for a panel fill + border + speaker) is real
  work a display list can't remove. Retaining canvas tiles across boxes would need a `ui_visible` gate
  plus an epoch to invalidate when another screen paints — the canvas is shared, so a hidden-but-resident
  box would also hide co-tenant canvas text.

### Bakeable lists — windowed rows (2026-07-30)
`makeSelector` with an `h` used to build all `n` rows and mark its container `scroll`, culling the ones
outside the viewport. That put the whole stock in the tree whether or not it could be seen, and made
geometry depend on a live offset — which is why `uiBake` refused it and why lists were the last screen
still laying out per open.

Given `h`, a selector is now **windowed**: it builds exactly the rows that fit and scrolls by refilling
them in place. `select()`/`scrollChanged()` return **0** for a scroll, because there is nothing left for
the caller to re-render — the shop's `if (sel.select(i) === 1) { refreshListPanel() }` simply stops
taking the expensive branch. Proven bakeable: `uiBake` on a windowed list returns **1** where the scroll
container returned 0.

Two things the model needs, both learned the hard way:
- **A refill must repaint through `uiRelayoutRows`, not `paint`.** `paint` draws over without clearing,
  and it re-measures nothing. `uiRelayoutRows` measures and arranges the single row inside the box it
  already occupies, and deliberately leaves the icon pool alone — resetting the pool here would blank the
  sprites belonging to the rest of the screen.
- **Wipe the whole row first, with `ui_rect`.** The opaque fill only covers each string's own box and a
  row's cells are content-sized, so a short name replacing a long one leaves the tail behind — "Kunai"
  landing on "Tonic Scroll" rendered as *"Kunai Scroll"*. `ui_rect` is pixel-exact, so it takes the row
  without touching the 8x8 tiles it shares with its neighbours, which is what rules out `ui_clear_rect`.

The scrollbar moved with it: the box is marked `win` rather than `scroll`, and the selector writes the
`_content`/`_view` that arrange used to record — computed from the full item count, which the window
itself no longer knows.

Not yet done, and deliberately separate: **no screen calls `uiBake` on its list yet.** The ~74K a tab
holds is only handed back once one does, and the shop's tab is the natural first (it is the heaviest
screen and the one that was crashing). Known cosmetic issue, pre-existing but newly reachable now that
lists scroll: the bar is drawn over the right edge of the rows rather than in reserved space, so a
scrolling list clips its right-hand column by 3px.

### Memory is the other budget (2026-07-29)

A GBA has no OOM killer: the allocator returns null, `handle_alloc_error` fires, and the player sees "The
game crashed :(". So every retained-for-speed structure needs a memory answer too, and `heap_free()`
(tish-agb, alongside `ticks()`) exists to get one — it claims 1KB blocks until the allocator refuses and
gives them all back, so a flow can be bracketed the way a slow one is bracketed with `ticks`.

Measured on shop-demo (a shop cursor move crashed with a failed 20KB allocation):

| moment | free heap |
|---|---|
| shop opens | 136K |
| keeper's greet box up | 112K |
| greet closed, tab entered | 97K |
| tab node tree built | 57K |
| tab painted | 18K |
| deferred detail filled | 8K |
| cursor moved twice | **4K** |

The numbers that matter: **a tish UI node costs ~1.3K** (each object is a hash map), so a shop tab's tree is
~40K and one dialog box shape is ~20K; a full canvas paint costs another ~35K in tile bookkeeping and glyph
buffers. Three fixes came out of it, all of the same shape — *nothing keeps memory it isn't using*:

- **`ui_reserve_tiles(n)` (called by `uiInit`, `tileReserve`, default 320).** agb keeps bookkeeping per live
  `DynamicTile16` and grows it in doubling steps (the HashMap doubles its backing at 3/5 load), so a paint
  that lights up more tiles than any before it re-allocates *mid-paint*, on a heap that by then holds the
  game. None of it ever shrinks, so the only question is WHEN the peak gets paid for: at boot, or in the
  middle of a cutscene. Reserve at boot, sized to the screen's **measured peak live tiles** —
  `ui_mem_report()` reports that peak (`peak N`), and sizing matters in both directions: too little leaves
  the step in the game, too much buys an extra doubling. Akari's peak is 180 (a full-width box: border plus
  three lines of glyph cells), it reserves 224, and that alone is the difference between dying in the intro
  and running the whole playthrough.
- **The layout pools no longer pin the last screen.** `LAYOUT.raw` held the flat node list until the next
  render, and a finished `uiStreamStep` kept both `STREAM.raw` and the root that produced it — so the screen
  a player had just left was still fully resident while the next one allocated. Both are released the moment
  the paint is on the canvas; this alone gave the shop ~35K back on leaving a tab.
- **Per-call scratch is never per-call.** The typewriter's shaped-row buffer and `ui_text_box`'s compositing
  buffer are both sized by the text (hundreds of rows at 16 bytes; ~1.5K for a wide box), and both used to be
  built fresh every time — so each new dialog page re-walked the doubling ladder, 512B → 1K → 2K → 4K, asking
  for a bigger contiguous block each step on a heap that was already full. That ladder is what akari's crash
  logs actually named (`allocation of 2048/4096 bytes failed`). Both now live in `GbaCtx` and are refilled in
  place; the row buffer's capacity is parked in `ui_row_spare` when the canvas blanks, so it survives the box
  that owned it. Capacity outlives the cache, contents never do.
- **Retention is capped and revocable.** `dialogInit({ cacheShapes })` (default 3) bounds how many solved
  box shapes stay resident — a shape is per body HEIGHT, so a chatty scene generates several and uncapped
  they accumulated 20K each until a box couldn't allocate (akari died two boxes into its intro cutscene;
  it now runs on `cacheShapes: 1`). `dialogFree()` / `uiBakeFreeAll()` hand it all back for a screen that
  needs the memory more, guarded by a bake **epoch** so a node kept across a release can't write into a
  recycled op slot. `shop.tish` uses both directions: it frees the keeper's box when a tab takes over, and
  drops the tab's trees before the keeper speaks again.

Shop-demo now cycles buy/sell/qty/tab-switch for 7,000 frames with 20K free at the quantity prompt, and akari
clears a 29,500-frame scripted playthrough (intro, then walking/talking/pausing) plus a mash-A-through-everything
stress run.

One lesson worth keeping: **a "clean" harness run proves nothing until you check it was still pressing keys** —
`gba-shot`'s schedule limit silently truncated the long walkthroughs, so runs that "survived 9,000 frames" were
idling from frame 1,200 on. The limit is now 4,096 entries.

### The entity wrapper was the budget (2026-07-29)

Akari still crashed after all of the above — on an area change, with 2K unavailable. The UI was not the problem;
it never had been the main one. A scripted town↔shrine round trip (a temporary debug entry, driven from the
harness so the flow could be measured instead of walked to) said this about loading one small town:

| step | free heap |
|---|---|
| boot, before any scene | 110K |
| map streamed (`scene_stream`) | 98K |
| collision grid built | 96K |
| **entities spawned (`populate`)** | **34K** |

Five entities cost 62K. `makeEntity`'s rich wrapper — the `this` a behaviour hook receives — is a hash map plus
one heap closure per method, and it had grown to ~55 methods covering every genre the engine supports: **~9K per
entity, whether or not the game calls any of it.** A top-down RPG was paying for `jump`, `drop`, `bounce` and
`gridStep` on every NPC. Two changes, both about not building what nobody calls:

**`entityApi({ grid, platformer, topdown, health, anim, dialogue, extra })`** — genre groups, all on by default
(an example gets the full documented API). Akari turns off `grid`, `platformer` and `extra`: 22 of the 55
closures, **~14K back** — `populate` now leaves 48K free instead of 34K, and the town plays in the tens of K
instead of 5-7K, which takes the whole class of "the next few KB failed" crashes with it. `heap_free()` plus a
scripted round trip is how to check any game: if `populate` is the step that eats the heap, this is why.

**The other ~16K is not safely reclaimable, and the trap is worth knowing.** Dropping the wrapper cache after
`loadScene` populates looks free — spawning needs the rich object, `makeEntity` is a lazy cache, so anything
that still needs one rebuilds on the next call. It measured ~16K back and it broke akari's warps, room advance
and save position, because a wrapper's `x`/`y` are **snapshots**, refreshed only when `makeEntity` finds *that
same object* in the cache. A game that stores a wrapper (`sceneReg.player`) and reads `p.x` each frame silently
freezes at spawn coordinates the moment the cache stops holding it — the player walks, the position it is
compared against does not, and no warp ever fires. Trim the wrapper by not BUILDING what nobody calls, never by
dropping live ones.

Still open: the wrapper is still ~9K for a game that needs every group. The durable fix is entity storage
that isn't a tish object per entity, and positions that are read live rather than snapshotted.

### The "warp leak" was mostly fragmentation (2026-07-29)

Repeated town↔shrine round trips in akari lose ground steadily — free heap after entering town went 13K, 9K,
8K, 4K and by the 26th warp 1K, one allocation from dead. That reads like a leak, and hunting for the owner
found nothing: `ui_mem_report()` is flat across every warp (tiles, peak, caches, sprite table — unchanged),
`clear_world` does drop behaviour data, and `bg_clear` does drop the stream layers.

It is mostly **fragmentation**, and the way to see that is to ask the allocator the same question twice:

**`heap_free(blockSize?)`** now takes a block size (default 1024). The count is in whole blocks, so the
answer is "how much is usable in pieces this size". Across three town teardowns:

| probe | visit 1 | visit 2 | visit 3 |
|---|---|---|---|
| `heap_free(1024)` | 51K | 43K | 36K |
| `heap_free(256)` | 52K | 47K | 44K |
| `heap_free(64)` | 42K | 38K | 37K |

Large-block availability falls ~8K per visit while the fine-grained total falls 3.3K then 1.2K — decelerating
toward a steady state. The bytes are still there; they are in pieces too small to serve a big request. That
matches the crash this started from exactly: **"2048 bytes unavailable"** on a heap reporting far more free
than 2K. (The 64-byte probe reads ~13K low because its own bookkeeping vector is ~16K — fine for trends, not
for absolute headroom.)

So the lesson is a diagnostic one: **a leak and fragmentation produce the identical symptom and want opposite
fixes.** Comparing two granularities tells you which you have in one build. Chasing an owner when the answer
is fragmentation is wasted effort; what helps is taking the big long-lived allocation early and keeping it
(`ui_reserve_tiles`), and not freeing and re-taking a large buffer around small survivors on every scene load.

**One real leak did turn up on the way, and it is worth knowing about tish objects:** `despawn()` evicted its
cache entry with `_entCache[id] = null`, and **a null-valued key still occupies the map — ~130 bytes each**
(400 dead keys measured at 52K). Entity ids never repeat, so a spawn/despawn-heavy game accumulated one dead
key per despawn for the whole session, growing and rehashing the map. tish does support `delete obj[key]` and
it genuinely removes the entry; both `despawn()` and `entityForget()` now use it.

**Superseded — `entityForget` is now a no-op.** Everything below about per-entity wrappers, their ~110 bytes
per closure, the dead-key growth and the "never forget a reference you keep" contract was work spent making a
per-entity wrapper affordable. It is not affordable, and it does not need to exist. See the section below.

### Boot time is a compiler bug, not our code: every `cargo:` import rebuilds the crate (2026-07-29)

the isoboard SRPG example shows its first pixel on frame 70 (~1.2s); `akari` takes 465 (~7.8s), the topdown RPG port 380,
`sunny-land` 548. **`examples/bench-boot`** was built the same way `bench-memory` was — one stage per ROM,
timestamped by the emulator — and it eliminated every plausible cause before finding the real one. Not ROM
size (`shop-demo` is 2.1MB and boots in 0.8s), not the `packages/` UI stack (`ui-demo` and `rpg-menu` import
all of it and boot under 1.2s), not map size (streaming a 38,400-tile overworld costs **10 frames**),
not asset registration (14 sheets + 2 scenes + a font: **0 frames**), and not the scene/UI/dialog/mount work
a game does at startup (**7 frames, all of it**).

**It is the tish compiler.** Each named `cargo:` import compiles to "build the crate's entire export table,
read one key out of it, throw the rest away". Import 68 names from a crate that exports 68 and it runs 68
times: 4,624 `Arc<str>`+closure allocations to bind 68 functions, quadratic in (names imported × names
exported), all of it before the game's first statement. The bench isolates it to two rows — the *same crate*
with 3 named imports is free, with 68 it costs **3 seconds**.

`packages/engine.tish` imports 117 symbols across two crates, so **every game built on the engine paid ~4.3s
to boot**, which was the whole difference between the fast and slow examples. There was no fix at our end:
`import * as` is rejected for native modules, so a module cannot take the namespace once and index it.

**Fixed in the tish repo** (`tish_compile/src/codegen.rs`): `emit_native_namespace_preamble` binds each
native module's namespace to a `run()`-local once, before any import reads from one, and
`native_module_rust_init` reads the key out of that local — O(imports × exports) becomes
O(imports + exports). Compiler suite and the `tish` integration tests pass; `bench-memory`'s contracts still
pass; akari, the topdown RPG port and the SRPG example render identically. **akari boots 12x faster (465 → 39 frames, 7.8s →
0.65s)**, the topdown RPG port 6.4x, ninja-adventure 8.4x, and the whole staged boot in `bench-boot` went 305 → 45 frames.

The lesson generalizes past this bug: *the cost was in code nobody wrote*. Six examples were slow, none of
them shared a scene, a map, an asset or a line of game code — what they shared was an import list. Profiling
inside the game could never have found it, because it is all spent before the game's first statement; only a
ROM that imports nothing, compared against one that imports one thing, can see it.

### Wrappers are pooled; a scene's entities cost ~520 bytes each (2026-07-29)

Every crash in this engine has been an allocation failure, so the work went into a bench that finds the
allocation instead of a game that dies of it: **`examples/bench-memory`** exercises one subsystem at a time in a
loop and prints free heap after every cycle. Flat = clean; a slope names its owner. It reproduced an OOM in half
a second, with no game, no walking and no dialog.

It priced the entity wrapper at **10.6K each** — and the earlier ~110-bytes-per-closure figure was the smaller
half of the story:

| | cost |
|---|---|
| 62 bare closures | 3520 B (**57 B** each) |
| 10 closures in an object | 1344 B |
| 10 **numbers** in an object | 896 B (**90 B per key**) |

So a 62-key wrapper is ~5.5K of hash map bucket and key *string* plus ~3.5K of closure. Eleven entities and
nothing else exhausted a 138K heap. An 18-entity area wanted ~190K of wrappers — which is the whole reason a
game could load its world and then fail to allocate 2K for a dialog box.

tish has no `this`, so a method cannot be shared per class — but it can be shared per SLOT. `makeEntity` now
hands out one of a few reusable wrappers whose methods read `obj.id` **when they run**, so re-pointing one field
re-points the entire API. 12 entities went from 127K and a crash to 6.2K, all of it native entity storage.

The price is a contract, and the contract is the interesting part:

- A wrapper from `create()`/`entity()` is **on loan** for the expression you got it in. To keep an entity, keep
  its `id` (a number) and call `entity(id)`. Storing the object names whatever borrowed the slot next, and the
  symptom is not a crash: akari parked a shrine room through stored wrappers, despawned **its own player**, and
  simply stopped responding.
- A hook's `this` and `other` get **their own slots**, held for the whole hook, so spawning inside a hook and
  then using `this` still means `this`. With one shared rotation, `spawnFx(...)` followed by `this.despawn()`
  despawned the effect and left the cherry on the ground.
- Hook slots are a **stack**, one level per nesting depth, because `interactTD()` fires another entity's
  `onInteract` from inside the caller's `update`.

Those three are asserted in the bench itself (`PASS`/`FAIL`, before the memory trials), because a rule nobody
checks is a rule that gets broken in the next example.

**Every example was then read against the contract, and two had to be migrated.** Both stored wrappers exactly
where it hurts: the topdown RPG port kept its streamed room's entities as objects, so crossing a room boundary unloaded
whichever entity had since borrowed the slot, and read the player's room from a snapshot `.x`/`.y`;
`ninja-adventure` kept the player wrapper that its two NPC spawns immediately re-pointed, so the door warp moved
an NPC. The topdown RPG port had a third variant worth naming, because it is the one code review misses: it called
`spawnRoom()` — a loop of `create()` — *in the middle of* configuring the player it had just spawned, so the
player wrapper was stale by the last line of its own setup. **Any call that can spawn is a barrier; finish
configuring a wrapper before you cross one.** The other 34 examples were already safe, almost all of them
because they configure a spawn and never look at it again.

**Two lessons about measuring, both learned the hard way.** `heap_free` held its probe blocks in a `Vec` of
pointers; probing at 64 bytes needs thousands of slots, and asking a fragmented heap for that one contiguous 16K
failed — *the probe crashed the game it was called to diagnose*, and its capacity cap silently truncated the
answer at the small sizes that matter most. It threads a free list through the blocks now and allocates nothing
but what it counts. And a bench trial that skipped `frame()` looked exactly like a 22.5K leak in `ui_clear`:
tiles are released at the commit, so six panels rendered without presenting piled up, tripped agb's live-tile map
into its 20K growth step, and ran the GBA out of tile VRAM. **If a trial reports a leak, first check it does the
thing a game would do.**

### What a wrapper actually costs, and `entityForget` (2026-07-29)

`examples/sunny-land` booted to a **white screen** — not a rendering bug: a *double* panic in
`handle_alloc_error`, which is why there was no crash screen to see. The wrapper had grown from ~30 methods to
~55 while the engine gained genre features, and this level spawns 18 entities on a 157K heap:

| step | free heap | per entity |
|---|---|---|
| boot | 157K | |
| map streamed | 144K | |
| hero | 129K | 15K |
| 5 enemies | 73K | ~11K each |
| 6th cherry | **0 — dead** | ~11K each |

A microbenchmark (`heap_free()` around 20 objects built three ways) prices it exactly: **~110 bytes per method
closure**, ~360 bytes for a 4-field object, and it makes no difference whether the methods are in the literal or
assigned after it. That is the whole story of the wrapper: **cost = method count**, and the API has ~55.

Sharing one function per method instead of building one per entity is not available — tish has **no `this`
binding** (`function() { return this.id }` is `cannot find value 'this'`), so every method must close over its
own `id`. What is available is not keeping wrappers nobody wants:

**`entityForget(e?)`** hands a wrapper back (one entity, or all of them); the entity itself is untouched, and
anything with a `this`-style hook rebuilds on demand. Sunny-land forgets each enemy/coin/gem as its spawn loop
finishes with it and keeps only the hero: **a coin costs ~1K instead of ~11K, and the level plays at a steady
98K free instead of dying mid-spawn.** The contract cannot be checked, so it is the caller's: never forget a
reference you keep, or you get the frozen-`x`/`y` failure above.

Two things generalise from this:
- **A `start` hook that only configures an entity is expensive.** Cherry's `start` was one line (`animate`), but
  a hook forces the wrapper to be rebuilt on the first frame — 11 coins' worth was 70K, and it OOMed *after* the
  spawn loops had carefully released them. Configure at spawn; keep hooks for behaviour.
- **A white screen is a crash, not a blank.** `GBA_SHOT_TRACE=1` reporting `changes=1 (0 px painted)` forever
  means the ROM died before the display was ever enabled; `GBA_SHOT_LOG=1` then has the panic. Worth running
  across every example — it also cleanly separates a dead ROM from a slow one (akari's first paint is frame 462).

### Text align on UI leaves
- Canvas `ui_text` / `ui_text_span` / `ui_text_box` take optional `align`.
- Text leaves: `{ text, align: "left"|"center"|"right"|"justify", w? }` (also `start`/`end`).
- `makePanel({ titleAlign: "center" })` for panel titles.
- Centre/right need a real box width (`w` / stretch); content-sized leaves are a no-op.

### Engine hot paths (2026-07)
- `ui_text_box`: **one** agb `Layout` pass (was two); optional `bg` arg for flash-free panel patches.
- Compact `ui_rect` fills (`h≤48`): tile-row `ui_masked_write` spans (not per-pixel).
- Large fills snap **IN** (never spill into title/footer chrome).
- `ui_text_box` trusts caller `boxW` (no +8 overshoot into button borders).
- `uiRender` reuses flatten/RAW arrays across paints; `fitText` binary-searches; `optsOf` avoids `{}` allocs.
- Hard rule: **stream** Status-scale trees — never one-shot `uiPaint` of dense chrome.

---

### Where an unbaked screen's 500ms actually goes (2026-07-30)

`ui-demo` with `uiInit({ stats: 1 })` builds its 32-node screen in **~131,000 ticks (499ms)**, and every
open costs the same — there is no warm path for a screen that cannot be baked. It is *streamed* (8
nodes a frame), so it is not a 500ms freeze; it is ~30 frames of reveal against a **≤1 frame** budget.

Measured, after the fixes below:

| phase | Items | Status | what it is |
|---|---|---|---|
| flatten | 30,137 | 39,934 | reading the **boxed** node tree |
| measure | 6,207 | 6,484 | typed `LNode` — the cheapest pass, doing the most arithmetic |
| arrange | 13,987 | 14,568 | typed `LNode` |
| write-back | 25,442 | 23,781 | writing geometry onto the **boxed** nodes |
| paint | 55,150 | 64,237 | native `ui_text` / `ui_rect`, plus ~270 first-touch tile allocations |

**The shape of that table is the finding.** Measure and arrange do the actual layout maths over the
same 32 nodes for ~20K; flatten and write-back only move data in and out of boxed objects and cost
~56K. A boxed property write is ~100 ticks (≈1,700 cycles — it hashes the key), and write-back does
8 per node, which is 25,600 of its 25,442. The typed passes are ~4x cheaper per node than the boxed
ones, matching what `examples/bench-ai` measures directly.

**Two fixes landed.**

*Span fills.* `ui_rect` filled row-major, re-resolving the same tile once per pixel row and
recomputing that column's mask and colour word every row, though neither depends on the row; and
borders drew their whole perimeter with `ui_set_pixel`, a tile resolution **per pixel**. Both now go
through one `ui_fill_span` helper that walks tile-column-first and batches the rows inside a tile into
a single `ui_masked_write_rows` — the batching that function already existed to provide. A 100×12 bar
went from 156 tile resolutions to 26. Status paint **−26%**, whole screen **657ms → 568ms**; the Items
screen's panel borders **−25%**.

*The LNode pool never pooled.* It lived at `LAYOUT.ln` and was read as `let LN: LNode[] = LAYOUT.ln`.
**A typed local read out of a boxed field is a copy, not a view** — it compiles to
`.iter().map(…).collect()` — and nothing wrote the result back, so the field stayed empty forever and
every render re-pushed all 32 nodes. `LN` is now a module-level typed array. This bought no
measurable time (copying an always-empty array is free) and is recorded so nobody re-derives it as an
optimisation; it is here because the pool now works.

**≤1 frame is not reachable from here, and that is the real conclusion.** One frame is 4,389 ticks.
Deleting *every* tish phase — flatten, measure, arrange, write-back, all of it — still leaves 55K of
paint, which is 12 frames. The budget is only reachable by not rebuilding the canvas per open: retain
the tiles and replay a solved layout, which is what `uiBake`/`uiReplay` already does for the screens
it accepts, and why baked screens hit budget while these do not. **The remaining work is "bakeable
lists" below, not another pass over these phases.**

Two dead ends worth not repeating: `ui_begin`'s teardown looks like it should be expensive (it walks
2,048 cells) and is 124 ticks; and the phase labelled `flat` is genuinely flatten, not the teardown
hiding inside it — `ui_begin` runs outside the measured window.

**Update (2026-07-30): the layout maths got 56% cheaper for free.** The compiler now keeps integer
pairs in integer registers instead of widening to `f64` and calling soft-float (see
`examples/bench-ai/README.md`). Nothing in `ui.tish` changed; the same Items screen re-measures:

| phase | before | after | |
|---|---|---|---|
| flatten | 30,137 | 29,588 | boxed — unchanged, as expected |
| measure | 6,207 | 4,119 | −34% |
| arrange | 13,987 | 5,038 | **−64%** |
| write-back | 25,442 | 19,582 | −23% |
| paint | 55,150 | 55,823 | native — unchanged, as expected |
| **total** | **~131,000 (499ms)** | **114,150 (435ms)** | −13% |

It confirms the attribution above rather than overturning it: the two typed passes more than halved,
and the screen still costs 435ms, because 78% of it was never arithmetic. Flatten and write-back are
boxed property access and paint is native blitting. **The conclusion is unchanged — the remaining money
is in not rebuilding the screen at all.**

The one thing to carry forward into any new `ui.tish` code: an integer pair stays in registers only if
*both* sides are integers, so a single unannotated local re-floats everything it touches. The solver's
existing `: i32` discipline is now worth far more than it was when it was written.

### A screen's node tree is a memory budget, and it was crashing the shop (2026-07-30)

Opening the quantity prompt over a shop tab killed the ROM: `memory allocation of 7680 bytes failed`.
Two separate faults, found by logging `heap_free` and `ui_mem_report` through the flow.

**The trigger** was `ui_tiles`, our own table of which canvas cell owns which dynamic tile. `uiInit`
reserves 320 entries, which was chosen to buy agb's HashMap step and quietly also became the table's own
capacity. A shop tab lights ~282 cells and the prompt over it crosses 320, so the table doubled — asking
for 7,680 contiguous bytes at the single most fragmented moment in the program. `ui_reserve_tiles` now
reserves the table to 640 (30x20 visible cells plus slack) independently of the warm count: it is a
number the table cannot exceed, so the reallocation cannot happen at all. Paying the same bytes at boot
on an empty heap is free; paying them at peak is a crash.

**What it exposed** is the real budget. Free heap, in 8K blocks, across one tab open:

| point | free | cost |
|---|---|---|
| tab entered, selector built | 90,112 | |
| `sel.view()` built its 11 rows | 65,536 | 24.5K |
| stream finished painting | 24,576 | **41K** |
| detail panel | 16,384 | 8K |
| quantity panel tree | 8,192 | 8K |

`ui_mem_report` accounts for ~8.7K of that, so **the other ~74K is boxed node objects** — the tree, plus
the six geometry fields the write-back adds to every node in it. One tab is a quarter of the heap, and
the crash was not really the doubling; it was that a screen this size leaves nothing behind it.

This is the same conclusion the timing reached, arrived at from memory: a display list is ~32 bytes per
op where a node is ~600, and `uiBake` **drops the tree**. Baking the tab is worth ~30K as well as the
layout time, which makes "bakeable lists" the fix for both.

---

## Should add

### 1. Form controls (remaining)

| Helper | Status |
|--------|--------|
| `makeStepper` / qty control | **done** |
| `makeToggle` / `makeToggleGroup` | **done** |
| `makeTabs` | **done** |
| `makeSlider` | Volume (defer until needed) |

---

### 2. Text policy leftovers

**Why:** Games still drop to `text_draw` for shadow / theme fonts.

- Theme defaults: `font` / `titleFont` / `tinyFont` via `uiInit` + `uiFont()` — **done**
- Shadow on canvas text — **done**: `{ text, shadow, shadowOff? }` on a text leaf, matching sprite
  `text_draw`. `shadow` is a theme token or `0xRRGGBB`; `shadowOff` is the down-right offset (default 1).

**Keep:** canvas for dense menus; sprites for HUD overlays. In-box `align` is done.

**Shadow is two passes, not one.** `ui_text` shapes and blits the whole string in the shadow colour at the
offset, then again at the real position — not shadow-then-body per glyph. A same-pixel canvas write takes
the LATER colour, so per-glyph interleaving lets a tightly-kerned neighbour's shadow eat the previous
glyph's body. The cost is a second shaping + blit, which is why it belongs on text painted once per screen
(a title, a name banner over map art) and not on anything redrawn per frame. `uiRowText`'s patch path does
NOT carry the shadow, so a shadowed label that later gets patched loses it — don't shadow a row that changes.

---

### 3. Progress / meters

| Helper | Status |
|--------|--------|
| `makeBar` (canvas flex leaf) | **done** — inset delta `set` / `setFrac` |
| Sprite `hud_bar` | still for over-map HUD |

---

### 4. Grid inventory

| Helper | Status |
|--------|--------|
| `makeGrid({ cols, cellW, cellH, … })` | **done** — icon + qty/label, empty slots, `selected()` focus |
| Auto-scroll viewport | **done** — pass `h` (viewport px); rows scroll whole-row to keep the selection in view |

Target pattern: classic action-RPG/JRPG bag screens — `examples/rpg-menu` bag + gear use `makeGrid`.

**Scrolling a grid** reuses `makeSelector`'s model: whole-row steps so `scrollY` stays a multiple of the row
pitch (the cull keeps only fully-visible rows, so a half-row would just vanish), and rows pinned to `rowH`
so the layout pitch *is* the scroll pitch. Only a scrolling grid pins its rows — pinning a plain grid would
clip cells its callers already lay out against. Unlike the selector there is no in-place `moveHi` to guard:
every grid move re-renders, so `view()` re-solves the offset.

The payoff is not just reach, it is **sprites**. Culled rows never paint, so a 12-slot bag in a 2-row window
spends 8 pool sprites, not 12 — and OAM, not the heap, is what caps a bag screen. `rpg-menu`'s bag was
`slots: 8` purely because a 3rd row did not fit on screen, which meant a 9th distinct item was unreachable
(`BAG` holds any number of stacks). It is now 12 slots in a 66px window.

**The scrollbar only drew from the recursive painter.** `paint` drew it; `paintNode` — the flat painter that
`uiRender` / the streamed open actually use — did not, and arrange never recorded `_content`/`_view` in the
flat path, so in practice a scrolling list had no affordance at all. Both painters now call a shared
`paintScrollbar`, and the flat arrange records content vs viewport onto the RAW node for scroll containers
only (rather than two more `i32`s on every LNode: a screen has ~200 nodes and at most a couple of scroll
containers). The bar draws 3px INSIDE the container's right edge, so a grid whose cells reach that edge
wants a slightly wider `w`.

---

### 5. Screen / focus stack

| Helper | Status |
|--------|--------|
| `uiPush` / `uiPop` / `uiTop` / `uiStackPaint` | **done** |
| `menu.tish` pause + file-select overwrite confirm | **done** (confirm is a pushed screen; B pops) |
| Options sub-screen between pause → confirm | **done** — `optionsMenu` / `optionsWarm`; akari's pause opens it |

`optionsMenu({ rows })` is the screen a pause "Options" row opens, on the same retained sprite-text model as
pause (instant after the first open). A row is a **toggle** (`get`/`set` over 0|1), a **picker**
(`choices` + `get`/`set`), or an **action** (`onSelect` alone, e.g. "Back"). Each row owns TWO text slots —
label and value — so flipping a setting repaints one slot and leaves the label alone, and `< >` brackets
appear only on the selected row as the affordance for Left/Right. `set` fires on every step, so the change
lands while the player is looking at it (akari mutes the BGM as you flip it) and there is no apply step.
Default slot base 64 clears pause (24) and file select; pass `slot` if a game parks text there.

**It calls `frame()` at the TOP of its loop, unlike `pauseMenu`.** Button edges (`is_just_pressed`) live on
the input state until the next `frame()` refreshes it, so a menu that reads keys in the same frame its caller
did sees the caller's press again — the A that chose "Options" would immediately toggle row 0. `pauseMenu`
solves the same problem for Start with its `startHeld` latch; letting a frame elapse first covers every
button at once.

---

### 6. Patch / dirty-update API docs

**Why:** Flash and full `ui_begin` are the main perf footguns; helpers exist but are easy to misuse.

**Rule of thumb**
- Structure change (new children, scroll window, tab body) → `uiPaint` / `uiRender` (prefer **stream** for dense trees).
- Label / selection / meter value → patch in place (`uiRowText`, `uiSetText`, `buttonPaint`, `makeBar.set`, `DET.patch`).
- Never one-shot a Status-scale tree; never hold two tab graphs without `clear()`.

Helpers: `uiRowText(node, text, color, bg?)` (pass panel/button `bg` on filled chrome), `uiSetText`, `buttonPaint`, `makeSelector.moveHi` / `select`, `makeBar.set`, `makeToggle.set` / `toggle`, `uiRelayoutAt` for one subtree.

---

### 7. Input binding helpers

| Helper | Status |
|--------|--------|
| `uiAct({ confirm: BTN_A, … }, bits?)` | **done** — returns action name or `""` |
| Exported `BTN_*` codes | **done** |

---

## Explicitly defer

| Idea | Why not now |
|------|-------------|
| Full CSS grid / rich layout | Flex-lite is enough for GBA menus |
| Animation / tween library | Per-widget motion (`makeArrow`) is enough |
| HTML-like rich text in UI | Tags on `text_draw` + simple colour spans cover most cases |
| Replace UI blit with stock BG text renderer | Would lose multi-colour tiles / typewriter control |
| More font schemes for their own sake | Bake-size via `font:path@N` is enough |
| Full nine-slice panel atlas | Rect fill+border is enough until a game needs framed art |
| `makeSlider` | No game needs continuous volume UI yet |

---

## Suggested next sprint

1. Canvas text **shadow** — **done** (`shadow` / `shadowOff` on a text leaf; see "Text policy leftovers").
   An outline (shadow on all four sides) is still open: it would be 5 passes, so only if a game needs it.
2. `makeGrid` viewport scroll for bags larger than the panel — **done** (`h`; see "Grid inventory")
3. Phase timers — **done**: `uiInit({ stats: 1 })` + `uiLayoutStats()` (see "Where a render's time goes")
4. Pause → options sub-screen — **done**: `optionsMenu` (see "Screen / focus stack")

---

## Notes

- Prefer helpers that own selection + flash-free paint (same model as `makeButtonGroup`).
- Prefer theme tokens over raw colours in new widgets.
- New features should land with a small demo page (extend `ui-demo` / `button-demo` / `shop-demo`) rather than docs-only APIs.
