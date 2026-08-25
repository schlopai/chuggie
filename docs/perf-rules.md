# The seven things that cost a GBA frame in tish

A 60fps frame is **4,389 Timer2 ticks**. Everything below was measured on device in this repo, and
each one was found the same way: bracket a suspect section with two `ticks()` reads, `log()` the
difference, and read `.tish/gba/<name>/src/main.rs` when the number does not make sense.

They are in order of what they actually cost, which is not the order anyone guesses.

---

## 1. ⚠️⚠️ An untyped scalar is SOFT-FLOAT — annotate it `i32`

A `const`, or a `let` with no type, compiles to a thread-local `Cell<f64>`:

```rust
// const A_X = 5   /  let A_X = 5
static G_A_X: SingleCore<Cell<f64>> = SingleCore::new(Cell::new(5_f64));
// so `A[b + A_X]` becomes:
let __bi = (((b) as f64) + G_A_X.with(|c| c.get())) as usize;
```

That is an i32→f64 conversion, an f64 add and an f64→usize conversion — three soft-float operations
**per array access**, on an ARM7TDMI, which has no FPU. Written `let A_X: i32 = 5` it is a
`VmRef<i32>` and the same expression is `(b).wrapping_add(A_X)`.

The same holds for arrays: `const XS = [1, 2, 3]` — and even `const XS: i32[] = [...]` — is a boxed
`Value::Array` of boxed `Value::Number(f64)`. `let XS: i32[] = [...]` is a `Vec<i32>`.

Measured: converting `packages/beatemup.tish` and its game took the tick from **7,800 → 5,700** and
the worst frame from **12,400 → 10,000**. `examples/versus` went 9,860 → 8,990. No logic changed.

**Do this:**

```bash
python3 scripts/const_to_let.py --check packages examples   # report
python3 scripts/const_to_let.py packages examples           # rewrite
```

**Check any ROM:** every hit here is a soft-float read on some path.

```bash
grep -c "G_[A-Za-z_]*\.with" examples/<name>/.tish/gba/<name>/src/main.rs
```

## 2. Calls, not arithmetic

A one-argument call into a function that touches module state is **~117 ticks**, whether or not it
crosses a module boundary — it is a boxed `Value` dispatch, and every argument goes through an f64.
A function that touches **no** module state is promoted to a real Rust fn and costs ~1.

A module-array read is ~1.7 ticks. So the data plane is nearly free and the call plane is everything:
300 array reads cost less than three calls. Do not restructure state to reduce indexing.

- **Inline small helpers.** `clampX` — two comparisons — was eight boxed calls a frame.
- **Pack the arguments.** One packed word beats four getters: see `motionFrame`, `fighterAiView`,
  `fighterHudView`.
- **Do not ask at all.** `tryStart` returns on its first line when no button is pressed; walking the
  move table unconditionally cost ~4,500 ticks a frame to decide not to punch.

Full detail: `docs/fighting-genre.md` §5.

## 3. The ARM7TDMI has no divide instruction

Every `%` and `/` is a call into a software division routine. `motionRead` indexed a ring with
`% RING` and cost **1,400 ticks**; a power-of-two size and `& RING_MASK` took it to ~500.

- Ring buffers and strides get a power-of-two size and a mask.
- Divide by a constant power of two with `>>`.
- `if ((frame % 60) === 0)` is a division sixty times a second for something that happens once —
  count a counter down instead.
- tish's `/` is not reliably truncating either (see `packages/rng.tish`), so where a division really
  belongs, write `((a * b) / c) | 0`.

## 4. Sprite VRAM panics; it does not degrade

32 KB, and running out is `panicked at sprite_allocator.rs: have space for sprites: SpriteFull` from
inside agb, on an innocent frame, minutes into play. Two non-obvious holders:

- **`sprite_set_visible(h, 0)` does NOT free a sprite's Object.** Four hidden 64x64 overlays still
  held 8 KB. Share one sprite and re-point it with `sprite_set_sheet`.
- **`text_draw` allocates sprite VRAM per letter group.** A 16px banner is several 32x32 objects.
  Anything that is a **word** belongs on the UI canvas (`ui_text`); only things that are a **number**
  should be sprites, and those are 16x16 digits.

## 5. Anything that changes every second is a digit sprite

`text_draw` is free while a string is unchanged and expensive the moment it changes — it re-shapes
the glyphs and allocates. Clocks, scores and combo counters change on exactly the busiest frames.
Ten pre-baked digit cells make each of them one `sprite_set_frame`.

The corollary bites hardest on instrumentation: a perf overlay built out of `text_draw` costs more
than the game it is measuring, and `frame_stats()` returns a **string**. Use `ticks()` and `log()`.

## 6. ⚠️⚠️ A transient effect is a POOL, never a spawned entity

Spawning an entity is the single most expensive thing a callback can do: `behave` builds the entity's
rich method wrapper (~30 closures) and `sprite_new` allocates sprite VRAM, and both land on one frame.
Effects are spawned by *good outcomes* — a pickup, a stomp, a kill — so the game drops frames at
exactly the moments the player is enjoying it, and it reads as "it gets slow when there are enemies
about". `examples/sunny-land`, spawning one FX every 30 frames (EMA, 4,389 ticks = one 60fps frame):

| | EMA | |
|---|---|---|
| nothing spawning | 4,371 | inside budget |
| **FX as an entity** | **5,750** | **31% over — a sustained ~45fps** |
| FX from a pool | 4,378 | indistinguishable from idle |

Pre-create the sprites once, hold them with `sprite_set_visible(h, 0)`, and drive ttl/frame/countdown
from flat `i32[]` arrays — see the FX block at the top of `examples/sunny-land/src/components.tish`.
Pooling makes an effect *free*; it does not merely make it cheaper.

The same applies to bullets, pickups dropped on death, and damage numbers. Entities are for things
that live as long as the scene.

## 7. ⚠️⚠️ A `tick` hook is NOT "plain field access" — and it is charged per ON-SCREEN entity

The fast `tick: (s) => …` hook is cheaper than `update`, and it is nowhere near free. `s` is a boxed
object. Every field touch is a clone plus a **string-keyed** lookup, and the arithmetic around it is
boxed f64. `examples/sunny-land`'s six-line opossum patrol generated, per enemy, per frame:

```rust
get_prop(&(s).clone(), "dir")                       // ×5 for dir, x, x, y, blocked
ops::add(&Value, &Value) → to_int32_value(…)        // every + and -
value_call(&_callee, &[Value::Number(…), …])        // the tileSolid() call
                                                    // + three boxed writes back
```

**And `world_step` skips off-screen entities** (`is_active`), so this is billed only for the entities
currently *visible*. A game that is fine in an empty corridor and drops below half speed with two
enemies on screen is describing this exactly — it is not combat, not effects, not entity count.

**Before optimising a per-entity hook, read what it compiled to:** `.tish/gba/<name>/src/main.rs`.
The cost is written out in plain Rust; guessing at it (or trying to reproduce a frame rate) wastes
far more time than reading it.

**This comment propagated.** The same false claim ("one tish call + plain field access, no per-op ABI
trip") sat above the same hand-rolled patrol in `examples/sunny-land`, `examples/platformer-combat` and
`examples/dark-hero` — platformer-combat's even said, two lines down, that `entity.patrol()` was the
zero-tish version. All three are now `e.patrol(…)`. If you find that comment anywhere else, it is
wrong; check the generated Rust.

**The fix is a native system, not a faster hook.** The engine already has `set_patrol` (walk + turn at
walls/ledges), `set_shooter`, `set_charger`, `set_stun`, `set_lure`, `set_mover` — same behaviour in
Rust at zero tish cost. If one is *almost* right, extend it rather than adding a hook back: sunny-land
only hand-rolled its patrol because `set_patrol` did not mirror the sprite, so `set_patrol` gained a
`flipMode` and the hook went away entirely. A `tick` that exists to call one setter costs more than
the system it is decorating. (`patrol_system` also gained the grounded gate dark-hero's hook had:
an airborne entity's ledge probe sees no ground ahead and flips it every frame.)

When a hook genuinely cannot become a native system, the fallback is `define_component`'s
`lean: true` with the tick registered directly — the engine calls it without the wrapper dispatch.
`packages/shmup.tish` does this for its shooter/boss/bouncer AIs.

---

## How to measure

```tish
let now: i32 = ticks()
let per: i32 = (now - stamp) & 0xFFFF
stamp = now
if (per > peak) { peak = per }
if ((frameNo & 63) === 0) { log("P" + peak) peak = 0 }
```

then `GBA_SHOT_LOG=1 scripts/screenshot.sh game.gba /tmp/x.png 1500 "<schedule>"`.

⚠️⚠️ **Only ever compare two builds that log the SAME thing.** The overlay is not free and it is not
neutral: on sunny-land, changing nothing but the fields in the `log()` concat moved the measured
`step_ticks(1)` from 1,466 to 2,441 — a 66% swing, entirely instrumentation, in a phase the logging
does not even run in (the string allocations perturb the heap the callbacks then run against). Two
numbers taken from differently-instrumented builds are not a before and an after. Stage A, measure,
stage B, measure, with the log line byte-identical in both.

⚠️ `frame_period(1)` is the **peak** since `step_peak_reset()`, not the sustained cost — one boot
frame or one logging frame pins it forever. `frame_period(2)` (the EMA) is the number that answers
"is this game slow"; `step_ticks(i)` is the last frame, so it is only meaningful sampled repeatedly.

⚠️ The key schedule counts **display** frames. A game that overruns runs its own loop once per two
display frames, so a two-frame tap can land inside a single game frame and never happen. Space
schedule entries ~6 frames apart, and when an input test fails, suspect the schedule first.

## Worked examples

`examples/versus` and `examples/beatemup` were built against these rules and their READMEs give the
numbers; `docs/fighting-genre.md` is the long form.
