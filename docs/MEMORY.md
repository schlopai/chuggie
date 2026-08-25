# Memory on this engine — what exists, what eats it, how it fails, and how to find it

Everything learned about GBA memory in this repo, in one place. Written 2026-08-12 after a session
where four separate "impossible" bugs — a boot crash, a 32-board ceiling, a battle that would not
start, and a cutscene that would not link — all turned out to be one allocator problem wearing
different hats.

**The governing fact:** a commercial GBA SRPG ships 162 maps, portrait dialogue, full menus
and a 300-mission campaign on this exact silicon. So *"it doesn't fit"* is never a conclusion. It is
a statement that we are spending something the cartridge did not.

---

## 1. The budget

| region | size | what it is |
|---|---|---|
| EWRAM | 256 KB | the heap lives here, minus our static data |
| IWRAM | 32 KB | stack + `.iwram` code; tiny, and overflowing it is fatal |
| BG VRAM | 64 KB | **1024 background tiles**, hard ceiling |
| OBJ VRAM | 32 KB | 1024 sprite tiles |
| palettes | 512 B | **16 BG banks + 16 OBJ banks**, hard ceiling |
| ROM | 32 MB | free real estate — put everything here |

Measured on the large SRPG example (since moved to the chuggie-tactics repo): static data ~16 KB, leaving **~237 KB of heap**.

**The cartridge spends ~0 bytes of heap on data.** Tiles, glyphs, maps and tables are ROM, DMA'd to
VRAM. Every kilobyte of ours is a decoded copy of something that did not need decoding.

---

## 2. Measuring — and the trap that invalidates most measurements

### ⚠️⚠️ `heap_free(n)` RESULTS ARE NOT COMPARABLE ACROSS DIFFERENT `n`

`heap_free(1024)` and `heap_free(64)` probe with different block sizes and return different numbers
for the same heap. Comparing one against the other produces confident nonsense.

This cost a wrong conclusion in the session that produced this document: 162 boards measured at
`heap_free(64)` looked like it had eaten 56 KB against a `heap_free(1024)` baseline, which was
reported as "~384 bytes per board." The truth was **256 bytes total between 96 and 162 boards** —
board registration is effectively free. Always compare like with like.

### The instruments

```tish
heap_free(1024)                    // bytes allocatable in 1 KB blocks
memSnapshot("tag")                 // packages/memdebug — heap + delta, entities, sprite pool,
                                   // UI pools, ptrs, hooks, plus every registered onMemReport
memTrace()                         // the same, automatically, at every scene/menu boundary
ui_mem_report()                    // UI internals: tiles, peak, solid, cap, cells, rows, spare,
                                   // box, tw, spr, pal N/15, palovf
```

⚠️ **`memSnapshot` / `memTrace` used to be unavailable to most ROMs** — `packages/memdebug`
imported `entity_count` from `cargo:tish_gba_game_engine` at module top, so no `tish_agb`-only ROM
could link it, which was 104 of the 122 examples: exactly the ROMs on the tightest budgets.
`01039065` freed it (along with `topdown` and `dungeon`); the entity count is now injected via
`memSetEntityCount(fn)` and reads 0 when unset. [#68](https://github.com/schlopai/chuggie-engine/issues/68)
is still OPEN despite the work looking done — the commit referenced it rather than closing it, and
nobody has verified that a `tish_agb`-only ROM actually links `memdebug` end to end.

⚠️ `heapProbe: 1` in an example's content **briefly claims the whole heap to measure it** — leave it
off in anything shipped.

⚠️ The debug `log()` perturbs what it measures (see `perf-ab-needs-identical-logging`): the same code
with a different string concat swung a frame time by 66%. Keep logging identical across an A/B.

### The ELF, for static vs heap

Static data does not show up in `heap_free`. Read section headers directly — `.bss`, `.ewram` and
`.iwram` with addresses in `0x0200_0000` / `0x0300_0000` are RAM; everything at `0x0800_0000` is ROM
and free. On the large SRPG example: `.bss` ~13 KB, `.ewram` ~3 KB, `.iwram` ~3 KB, `.text`+`.rodata` ~10 MB
in ROM.

---

## 3. Where it actually goes — measured, not guessed

The large SRPG example, 162 boards, direct art, dialogue linked:

| stage | cost | note |
|---|---|---|
| module init (assets, boards, ROM crate) | large | assets are ROM-baked; see below |
| `uiInit` **before** the agb fix | **45,056** | agb's tile map doubling |
| `uiInit` **after** the agb fix | **11,840** | and `tileReserve: 0` removes most of the rest |
| `dialogInit` | **1,344** | with `cacheShapes: 0` |
| fonts (`@7` and `@20`) | **0** | glyphs are baked into ROM at build time |
| 162 boards vs 96 boards | **256** | registration is essentially free |
| `gameInit` | ~25 KB | battle init ~5, worldInit ~10, rebuildOffers ~8, board ~3 |

### Things that turned out to cost nothing

Blamed and exonerated by measurement, so do not re-suspect them:

- **`packages/dialog` + `packages/cutscene`** — ~1 KB together. They were left unimported for years
  on the belief that they cost ~38 KB.
- **Fonts** — a 20px face and a 7px face give byte-identical heap. Glyphs are ROM.
- **Board registration** — `ISO_BOARDS`, `ASSET_BGS` and `FG_ASSETS` are all fixed-size static
  tables (256 entries each) that allocate nothing. Someone already did that work; a `Vec` there used
  to fragment the heap at module init and a 48-board game died with plenty free.

---

## 4. How an allocation failure presents — none of them say "out of memory"

This is the single most expensive thing about GBA memory work: **the failure never names the cause.**

| symptom | what it actually is |
|---|---|
| `Bad memory Store16/Load32: 0x...` at a wild address | a failed alloc returned null and was written through |
| `Jumped to invalid address` / `Illegal opcode` in the millions | a garbage function pointer — same cause |
| PC = `0xE12FFF1E` | that word is ARM `bx lr`; the CPU is executing *data*, i.e. a corrupt pointer |
| a screen of coloured stripes / white noise | **agb's crash screen**, not a rendering bug — decode with `agb-debug` |
| `panicked at alloc.rs:574` | `handle_alloc_error` — the honest one, and the rarest |
| a silent hang with no output | an allocation that never returns, or a probe that never prints |
| `Ran out of video RAM for tiles` inside `bg_new` | tile VRAM, blamed on whichever background was innocent enough to be next |
| `assertion failed: paletted_pixel < 16` inside agb | palette bank overflow, raised on an unrelated caller |
| `capacity overflow` in `set_text_slot` | a corrupt `&str`, usually from a lowered string-returning fn |

### ⚠️⚠️ A FAULT THAT RELOCATES WHEN YOU PERTURB IT IS A LAYOUT PROBLEM

If the crash moves — frame 73, then 80, then earlier still — as you rearrange unrelated statements,
**you are moving a symptom, not approaching a cause.** That is the moment to change the design, not
to try the next arrangement. Roughly a dozen five-minute builds were spent rearranging statements
inside one function before this was taken seriously.

A fault that stays put under perturbation is a logic error. A fault that wanders is memory.

---

## 5. The patterns that cause it

### 5.1 Unbounded growth for a bounded resource ← the big one

`agb`'s `VRamManagerInner::tile_set_to_vram` was built with `HashMap::new()` and left to double.
Background VRAM holds **exactly 1024 tiles**, so that map can never need more — but its final
doubling asked for **one ~40 KB contiguous block** at whatever moment the game added its 1024th
tile, which is reliably the moment the heap is most fragmented.

Every one of these was that bug: `uiInit` hanging with 95 KB free; menus that would not open;
`main.tish`'s rule that the UI canvas must be claimed *before* the boards register; per-game
`tileReserve` warm loops that existed only to provoke the growth early; and `tileReserve: 200`
breaking `uiInit` outright because it was too small to force the step.

**Fix (applied):** reserve to the hardware ceiling once, at `init()`, on a pristine heap.

```rust
// VRamManagerInner::init()
self.tile_set_to_vram = HashMap::with_capacity(1024);
self.reference_counts.reserve_exact(1024);
```

`uiInit`: 45,056 → 11,840 bytes, engine-wide, for every game. `agb_hashmap` has no `reserve`, and
`new()` is `const`, so it must be done in `init()`.

**Generalise it:** any structure tracking a hardware-bounded resource should be sized to that bound
once, up front. Sprites, palettes, tiles, screen entries — all have fixed ceilings.

### 5.2 Contiguity, not total free

"90 KB free" and "cannot allocate 40 KB" are consistent statements. The GBA allocator hands back
contiguous blocks and there is no MMU to paper over fragmentation. **A big allocation must happen
early, on a clean heap, or be avoided entirely.**

Symptoms of a contiguity problem rather than a volume problem:
- it fails with lots of total free
- it succeeds when moved earlier in module init
- it depends on the *order* of unrelated initialisation

### 5.3 Claimed at boot, held for the ROM's life

The pattern to hunt. Examples found here:

- the UI canvas and its tile map, held through every battle, for menus not on screen — and
  `packages/battle` makes **zero** `ui_*` calls
- `ui_clear()` keeps ~48 KB of scratch; `ui_release_scratch()` exists to hand it back
- board tiles: an outgoing board's tiles are live while the incoming board uploads, so a switch
  peaks at both (`bg_clear()` does gc them first — that part is correct)

**Residency, not reservation** is the design: a screen acquires what it draws with and releases it.

### 5.4 Per-site allocation of shared things

`tishlang/tish#595`: GBA object-literal keys were allocated **per site**, so a data table paid for
every key name once per row — 1,890 sites for 474 distinct names on a real ROM. Interning them was
worth **+79% EWRAM headroom**. Now landed (`__tish_key` + a program-wide intern table).

### 5.5 Boxed values where typed ones would do

- an untyped tish array is `Vec<Value>` at **~28 bytes/element**; `: i32[]` is `Vec<i32>` at 4.
  46 KB on one package.
- a tish `const`/untyped `let` scalar is a `Cell<f64>` — soft-float on a chip with no FPU, and every
  read is a call. `let X: i32 = 5` is a native integer. Worth 20–27% of a frame.
  ⚠️ `scripts/const_to_let.py --check` is *expected* at 41 in `packages/game.tish` — converting
  those broke the campaign (issue #65). Do not "clean it up".
- boxed rows: 115 job records as tish objects did not fit; the same data as `static` Rust arrays in
  ROM costs **zero EWRAM**. That is what the SRPG data crate is (13 tables, 5,564 rows, 0 bytes).

### 5.5b Every function is a boxed value, called or not ← the import tax

5.5 covers boxed *data*. Functions are boxed too, and it is the reason importing anything is
expensive.

**Measured** on `card-gba/queensblood`'s generated Rust:

```
plain Rust fns (^fn .._native)  :   7
Value::native heap closures     : 611
```

Seven get static dispatch; 611 are materialised as heap `Value` closures at module init. The
compiler is being correct — tish functions are first-class and the codebase relies on it
(`matchBind({ onPlace: place })`, arrays of scene closures) — so it cannot prove a binding is never
read. But it means **an import materialises every function in the module, whether or not you call
it**, at roughly 151 bytes each.

Measured import costs from `card-gba`:

| import | cost |
|---|---|
| `packages/ui` for **one** reachable call (`uiInit`) | dropping it recovered **+23,552 bytes** |
| `packages/chipsfx` for 8 of its 13 sounds | **10,496 bytes**, and took `heap_free(40960)` from one full block to **zero** |
| `packages/engine` | 10.6 KB |
| `packages/shop` | ~38 KB before it runs (#64) |

**Corollary that catches people:** the cost is ROM-specific. Gains at this scale are dominated by
*where the arena fragments*, not by byte count, so the same import can be free in one ROM and fatal
in another. Measure in the target, and measure the change in isolation — the `ui` drop was +23,552
of a +24,512 total, which is how we know the 18 dead functions removed alongside it were the
rounding error rather than the win.

Filed as [#67](https://github.com/schlopai/chuggie-engine/issues/67): a function that is only ever
*called* can be emitted as a static `fn` and stripped by the linker. Seven already are. That single
change would retire most of this section.

**What the tax does to content code.** Downstream authors work around it and the workaround looks
like bad style. `card-gba` writes one function per *conversation slot* with a hundred branches
inside, rather than one function per character, because 120 characters × 4 states would be 480
functions ≈ **72 KB for text nobody has read**:

```tish
export function vTaunt(npc: i32) {
  if (npc === N_NENE)   { return "Sit. Three lanes, fifteen cards..." }
  if (npc === N_VIRGIL) { return "It was RARE. It was rare and it was mine." }
  return archTaunt(vArch(npc))
}
```

Same reason accessors get merged into one field-selector: **373 bytes recovered per function
removed**, measured. If #67 lands, both patterns should be revisited rather than copied forward.

### 5.6 Lowered functions that scribble

A tish fn with a fully scalar signature may be lowered to a free Rust fn that cannot see module
scope. It compiles and reads garbage, and corrupts things unrelated to itself. See
`tish-typed-lowering-rules`. Symptom: "corruption with no plausible owner."

### 5.7 Two module-level shapes that do not compile

Both cost `card-gba` real time and neither error names the cause.

- **A module-level mutable scalar** lowers to a boxed `VmRef` cell, and a function assigning a plain
  `i32` into one is an **E0308** at the Rust stage. Use a one-element array or an `interface`
  instance: `let CSHADOW: i32[] = [-1]`, never `let CSHADOW: i32 = -1`.
- **Binding a string out of an array** — `let name: string = NAMES[i]` — lowers to a bare
  `Vec<String>` index, which is a move out of the Vec (**E0507**). Index straight into the call
  instead: `ui_text(font, x, y, NAMES[i], colour)`.

Also worth knowing because it presents as a compiler bug: **a stale `<game>/.tish/` tree emits
contradictory types for generated `G_*_SV` globals** (`expected VmRef<Vec<i32>>, found Vec<i32>`) on
source that is perfectly valid. `rm -rf <game>/.tish` and rebuild. Three trees held 45 GB.

---

## 6. Hard ceilings worth memorising

| ceiling | value | how it fails |
|---|---|---|
| BG tiles | 1024 | `Ran out of video RAM for tiles` in an innocent `bg_new` |
| BG palette banks | 16 | build-time `DoesNotFitError { count: N }`, or silent wrong colours |
| OBJ palette banks | 16 | panics *inside* agb on an unrelated caller |
| affine BG tiles | 256 | **silent** — agb paints tile 0 past it |
| `tileReserve` | cliff at 512 | 20 KB → 40 KB; not a dial |
| registries (`MAX_BOARDS`, `MAX_BGS`, `MAX_FG`) | 256 each | returns −1, then a −1 handle is used as real |
| entities at 60fps | ~20–25 | ~60 ticks each; collision is flat |

⚠️ Two layers per board (floor + foreground occluders) means 162 boards want **324** background
registrations against `MAX_BGS = 256`. That is why foregrounds have their own `FG_ASSETS` table.

---

## 7. Things that are already right — don't "fix" them

- **The registries are static.** `ISO_BOARDS`, `ASSET_BGS`, `FG_ASSETS` are fixed arrays that
  allocate nothing. They were `Vec`s once; the pushes fragmented module init and a 48-board game
  died in the allocator.
- **`bg_clear()` gcs the outgoing board's layers** before the incoming one is built.
- **Tables live in ROM** via `cargo:` crates. Rows are free; **accessors are not** — 156 exports
  once cost ~14 KB. Take every row, only the columns actually called.
- **`BOARDS` is an array literal, not a push list.** The literal lowers 37× better, and converting
  it breaks the direct board path outright.
- **Fonts are baked at build time.** Do not add a runtime rasteriser.

---

## 8. Ideas worth doing

1. **Residency for the UI canvas.** It is ~12 KB and untouched during battles. A screen that makes
   no `ui_*` call should not hold it. Needs the engine to know who reads what —
   ⚠️ `ui_release_scratch()` before a battle **is not safe today**: it broke the command menu,
   because the runtime's *text* path shares `ui_box_scratch`. "This module doesn't call that API" is
   not "this module doesn't depend on that memory."
2. **Apply §5.1 everywhere.** Audit every `Vec::new()` / `HashMap::new()` in the runtime that tracks
   a bounded resource and give it the bound up front.
3. **Board tile residency.** Free the outgoing board's tiles before the incoming upload rather than
   peaking at both.
4. **Make exhaustion say so.** Every arena should fail with its own name and capacity, not by
   returning a null/−1 that detonates three frames later somewhere else.
5. **A `memSnapshot` in `verify.sh`.** A budget assertion at a known point turns a slow leak into a
   failing check instead of a crash six features later.
6. **Static arenas for the big blocks.** Anything that wants ≥16 KB contiguous should own a `static`
   buffer, where fragmentation cannot reach it.

---

## 9. Rules of thumb

- **"It doesn't fit" is never the answer.** The cartridge did it on the same chip.
- **Measure before attributing.** Four attributions in one session were wrong until measured —
  dialog at 38 KB (really 1), the tile table at 11.5 KB (really 2), the title font (really 0),
  and boards at 384 B each (really ~0).
- **Compare like with like.** Same probe, same granularity, same logging.
- **A wandering fault is layout; a fixed fault is logic.**
- **Big allocations go early, or not at all.**
- **Bounded resource ⇒ bounded structure, sized once.**
- **Check the tree moved before blaming your own code** — this checkout is shared, and the engine
  changed four times during the session that produced this file.
- **`rm -rf .tish` after any interrupted build.** Two concurrent builds in one example directory
  produce a missing `.rcgu.o`, a linker error and a 0-byte ROM.

---

## Related notes

`gba-wild-jumps-are-null-allocs`, `gba-ui-canvas-leak`, `gba-ui-reserve-holds-vram`,
`gba-sprite-palette-ceiling`, `gba-affine-256-tile-ceiling`, `tish-gba-runtime-arenas`,
`tish-never-untyped-arrays`, `tish-const-is-soft-float`, `tish-typed-lowering-rules`,
`agb-tile-vram-freed-only-at-commit`, `tish-gba-menu-memory-model`, the board-ceiling-two-walls note,
`docs/perf-rules.md`, `docs/memory-perf-review-2026-07.md`.
