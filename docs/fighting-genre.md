# The versus fighting and brawler genres

> **Canonical version:** https://chuggie.dev/docs/packages/fighter

`packages/fighter.tish` + `packages/motion.tish`, demonstrated by `examples/versus`; and its sibling
`packages/beatemup.tish`, demonstrated by `examples/beatemup`. They share the input ring and the art
pipeline (`scripts/fighter_art.py`) and nothing else — see §8 for why not.

A fighting game is two state machines, a table of numbers, and a rule about who was holding back.
Almost none of it is engine work — the GBA has no trouble drawing two sprites. What it has trouble
with is the *shape* of the code, and that is what this pair of packages is for.

---

## 1. The model

A character is **24 body poses** on one sheet and **nine moves** in a table. The poses are fixed:

| cells | pose |
|---|---|
| 0-3 | idle |
| 4-7 | walk |
| 8, 9 | jump, fall |
| 10, 11 | crouch, crouching attack |
| 12-14 / 15-17 / 18-20 | attack 1 / 2 / 3, one pose per phase |
| 21, 22, 23 | guard, take hit, KO |
| 24-33 | the attack poses' FX overlays (see §4) |
| 34 | select-screen portrait |

Because the layout is fixed, `fighter.tish` contains no character-specific code and a new fighter is
a call to `fighterDefChar` plus nine `fighterDefMove`/`fighterDefBox` pairs. `examples/versus/src/chars.tish`
defines all four of its characters through one `define()` with three tuning numbers each.

A move is:

```
fighterDefMove(chr, mv, clip, clipn, startup, active, recovery, dmg, kind)
fighterDefBox (chr, mv, hbX, hbY, hbW, hbH, hitStun, blockStun, push, hitStop)
fighterDefInput(chr, mv, motion, btn, cost, airOk)
fighterDefCancel(chr, mv, mask)
```

Split into four calls on purpose: tish does not check arity (`scripts/arity_check.py` exists because
of it), and a single eighteen-argument call is one dropped comma away from a silently zero-damage
move.

**Everything is measured from the feet.** `hbY` is height above the ground, hurtboxes hang down to
it, gravity stops at it, and sprite cells are bottom-aligned to match — so a taller character needs
no per-character draw offset. Positions are 8.8 fixed point, because a walk speed of "one and a bit
pixels a frame" is not expressible in integers and rounding it to 1 makes every character walk at
the same speed.

## 2. Blocking is decided at the moment of contact

There is no "block" state a player enters. Holding away from the opponent walks backwards; whether
that *guards* is decided inside `resolve()`, from the move's `kind` and the defender's stance:

| kind | blocked by |
|---|---|
| `KIND_MID` | standing or crouching |
| `KIND_LOW` | crouching only — the sweep |
| `KIND_OVERHEAD` | standing only — beats a turtle |
| `KIND_UNBLOCKABLE` | nothing — throws |

That is the entire guessing game of a fighting game, and it is one column of a table.

## 3. Input is a ring, not a state machine

`motion.tish` keeps 16 frames of pad history per player, packed one word per frame
(`dir | held << 4 | edge << 8`), and every recogniser is a backward scan over it. Commands are
written once, facing right; `motionRead` mirrors the recorded directions (4↔6, 1↔3, 7↔9) before
matching. A recognised motion is *consumed*, or one quarter-circle throws twelve fireballs.

⚠️ **The ring size must stay a power of two.** The index is `& RING_MASK`. Written the obvious way,
as `% RING`, it is an integer modulo — and the ARM7TDMI has no divide instruction, so every one is a
call into a software division routine. `motionRead` walks twelve entries and measured **1,400 Timer2
ticks** of a 4,389-tick frame, almost all of it division.

## 3b. Dashes and combos come out of the same two tables

A dash is a double tap on the horizontal — the recogniser looks for *press, release, press*, because
the release is the only thing separating it from holding a direction. It is reported in the same
packed word as the motion, on its own consume marker so reading one input does not eat the other.

The two dashes are deliberately asymmetric, and the asymmetry is the design: the **forward** dash is
short and may be cancelled into an attack after a few frames (that is what makes running attacks a
thing), while the **back** dash is longer and invulnerable while it leaves (that is what makes it an
escape rather than a retreat). Neither is a special case in the state machine — both are one state
with a timer and a velocity.

Combos are a bitmask per move of what it may cancel into, applied only once the move has *connected*.
Damage scales down with the length of the string (`SCALE`, 1/256ths, indexed by hits already landed):
without it, one opening is the whole round.

## 4. The sprite the art does not fit in

A hand-drawn attack frame is much wider than a GBA sprite: the fighters in `examples/versus` are
~33×54 px, but a sword arc reaches ~95 px from the body's centre. Shrinking the character until the
whole frame fits gives a 28-pixel fighter.

It is not only the width: an overhead swing reaches 124 px ABOVE the feet against a 54 px body, so
the top of the sword gets cut off too.

So each attack frame is **cut in two** by `scripts/gen_versus.py`: the body cell, and ONE
neighbouring 64x64 window — the one holding the most of what did not fit. `FX_DX`/`FX_DY` record
which neighbour, in cell units, and `fighterDraw` mirrors the offset and the cell together when the
fighter faces left.

⚠️ **Adjacent windows only, never a diagonal.** A diagonal piece shares only a corner with the body
cell, so whatever it holds reads as a white slab floating beside the fighter rather than as the rest
of the sword. An edge-sharing piece always looks joined, even when the far tip of the arc is the
part that got left behind — which it often is, because only one extra cell is kept.

Two traps in that pipeline, both of which look fine in a preview:

- **Alpha must be hardened to 0 or 255.** A GBA sprite has one transparent colour, not an alpha
  channel, so the importer keeps a pixel or drops it. Antialiased text and LANCZOS resampling both
  produce feathered edges; a 1px font stroke is almost *entirely* edge, and the digits came out as
  disconnected dashes — while compositing perfectly over any preview background.
- **Quantise the assembled sheet, never per frame.** `include_aseprite_inner!` emits one `Palette16`
  per PNG, so body + FX + portrait in one file costs one of the sixteen sprite palette banks.
  Quantising twice produces two palettes.

## 5. What actually costs a frame

> The general form of this section now lives in [perf-rules.md](perf-rules.md), which covers the
> whole engine. What follows is the genre-specific version and the numbers these two games measured.


Measured on `examples/versus` with `ticks()` brackets, against a 4,389-tick 60fps frame:

| | ticks |
|---|---|
| a 1-argument call into a function that touches module state | **~117** |
| a call into a function that touches NO module state | ~1 |
| a module-array (`i32[]`) read | ~1.7 |
| one `sprite_set_frame` on a 64×64 cell | ~130 |

The split is the promotion rule, not the module boundary: a function that reads or writes a
module-level array captures it and becomes a boxed `VmRef<Value>` closure, dispatched with every
argument boxed through an `f64`. A pure leaf lowers to a real Rust fn (`family`, `mirror` and `pct`
in this example do; everything holding an array does not). Since a fighter's state IS a module
array, essentially every call on its hot path is the expensive kind, and the whole optimisation
story for this genre is **counting calls**:

- ask the pad **one** question a frame, not four — `motionFrame` returns dir, edge, both button
  buffers and the recognised motion in one packed word;
- give the AI and the HUD **packed views** (`fighterAiView`, `fighterHudView`) instead of eleven
  getters;
- read the whole pad with `keys_held()` (added for this genre) rather than eight `key_held` calls;
- and **do not ask at all** on the frames where nothing happened: `tryStart` returns on its first
  line when no button was pressed and none is buffered, which is the overwhelming majority of
  frames. Walking the nine move slots unconditionally cost ~4,500 ticks — more than a whole frame
  spent deciding not to punch.

That took the fight loop from 19,700 ticks on its worst frame to ~9,800.

## 6. ⚠️⚠️ Do not put a fighting stage on `bg_bands`

A fighting stage is horizontally stratified — sky, cliff, treeline, floor — which is exactly the
shape per-scanline band parallax handles, and `examples/versus` was built that way first.

It shredded. `bg_bands` hands the horizontal scroll register to an HBlank DMA whose table is rebuilt
from the camera inside `frame()`, and agb commits a frame **without waiting for vblank** when the
game has already overrun. On a late frame the DMA is re-armed partway down the visible screen, its
source pointer restarts at row 0, and every row below the re-arm gets the *sky* band's offset
instead of its own — the treeline jumps eight pixels sideways for the bottom half of the picture.

**Banding turns a dropped frame into a corrupt one.** It belongs on a scene with headroom
(`examples/bands-demo` has a whole frame spare). A plain background layer degrades the other way:
its scroll register is written once, so a late commit shifts the whole picture by the one pixel the
camera moved, which nobody can see.

The same reasoning applies to the screen shake. Jittering the camera by ±7 px a frame is what made
the tear obvious in the first place; `fighterDraw(ox, oy)` takes an offset so the shake moves the
*fighters*, which is where a fighting game's impact belongs anyway.

## 6b. Menus want the hardware backdrop, not a filled canvas

The character-select screen hides the stage with `bg_set_visible` and shows a flat colour through
`backdrop()`. The first version filled the UI canvas instead — `ui_rect(0, 0, 240, 160, …, 1)` — and
a rectangle of exactly that shape survived into the next fight and sat over the middle of the arena.
A full-screen canvas fill allocates ~600 UI tiles that then all have to be un-painted; the backdrop
shows wherever no layer draws, costs nothing, and leaves nothing behind.

Two more things the select screen taught, both of which look like a broken `ui_rect`:

- **`ui_clear_rect` snaps OUT to whole 8x8 tiles.** The portraits sit on a 59px pitch, so clearing
  cell n+1's cursor region also blanks the last few pixels of cell n's — and interleaving the clears
  with the draws produced a cursor box with three sides. Clear ALL of them, then draw the one.
- **Check the last cell reaches the right edge.** The final cursor box ran one pixel past 239 and
  its right border simply was not drawn, which reads as the same bug from a completely different
  cause. Lay a row out from the total width, not from a pitch that looked about right.

## 6c. Testing input headlessly

`scripts/screenshot.sh`'s key schedule counts DISPLAY frames. A fighting game runs under 60fps when
things are busy, so a two-display-frame tap can land entirely inside one game frame and never reach
the input ring. A double tap written as `255:left,257:,259:left` produced ONE frame of input and no
dash; `255:left,262:,269:left` produced it every time. Suspect the schedule before the code, and
dump the ring rather than inferring from a sprite's position — the fighter that "will not walk" is
usually a fighter in blockstun, because holding away from an attacking opponent is what blocking is.

## 7. Everything that changes every second is a digit sprite

`text_draw` caches per slot, so redrawing the same string is free — but the moment the string
*changes* it re-shapes the glyphs and allocates sprite VRAM, and that spike overruns the frame. The
round clock changes once a second and the combo counter changes mid-combo: i.e. on exactly the
busiest frames. Ten pre-baked digit cells turn both into `sprite_set_frame`.

The perf overlay in `examples/versus` (SELECT) is four digit sprites for the same reason — the first
version of it was a `text_draw` and cost more than the game did, which is a counter measuring
itself.

---

## 8. The brawler sibling

`packages/beatemup.tish` is the same idea with a third axis and more than two people, and it is
**deliberately not built on `fighter.tish`**. A versus fighter is two actors on a line; a brawler is
four on a plane. The two differences — a loop over actors, and a depth test inside every hit — are
exactly the things a game with neither should not be paying for, and merging them would have put
both into the one that ships at 60fps.

What IS shared is the part that generalises: the 24-pose sheet layout and its bake, and
`packages/motion.tish` (the ring, the buffers, and the double-tap that is a dash in one game and a
run in the other).

### The depth axis

An actor's `y` is where it stands ACROSS the road, and it decides three things that must agree:

| | |
|---|---|
| draw row | `screen = y - z` |
| draw ORDER | `sprite_set_depth(spr, y)` — nearer the camera covers further up the street |
| whether a punch connects | `\|attacker.y - victim.y\| <= DEPTH_TOL` |

The third is the one the old version of that example was missing, and without it the road is
decoration: every punch lands on whoever shares your column of the screen.

⚠️ **The shadow is load-bearing.** With a virtual Z axis, "standing far away" and "hanging in the
air" are the same sprite at the same screen row. A shadow that stays on the ground and shrinks with
altitude is the only thing that distinguishes them, and a jump is unreadable without it.

### The stage: three layers from one atlas, and agb's palette packer

`packages/parallax.tish`'s rules are load-bearing: ONE `background:` atlas for every layer (because
`tilemap_new` replaces all sixteen background palettes, so two imports fight), holes are GID 0, and
`ui_begin` has already taken one of the four layer slots. Alpha IS preserved — agb's
`Colour::is_transparent` is `a != 255` — so silhouettes stay pixel-shaped rather than 16px-blocky,
provided the art was alpha-hardened first.

⚠️ **agb's palette optimiser is much worse than a greedy first-fit.** `scripts/gen_beatemup.py`
implements the greedy check and reported 8 palettes for a stage that agb's
`pagination_packing::overload_and_remove` insisted needed 25 — it fails the build with
`DoesNotFitError { count: 25 }`, naming no tile and no colour. A local check is a guard, not a
substitute: keep a whole stage under ~26 colours.

⚠️ The pack's layers are bottom-aligned to one another, so a naive drop-in buries the treeline under
the road and reads as a missing layer. Name the source rows each layer wants, and write a preview
PNG so the composition is iterable without building a ROM.

### ⚠️⚠️ `sprite_set_pos` is in WORLD coordinates

The engine subtracts `camera_x` itself when it builds the draw list for a non-HUD sprite. Subtracting
it in the game's own draw code as well is invisible while the camera travels sixteen pixels — which
is exactly how far `examples/versus` moves — and hides every character off the left of the screen the
moment a scrolling game reaches its second screen. The symptom is an empty stage with a working HUD,
because HUD sprites are in screen space and keep drawing.

### ⚠️⚠️ `const` is soft-float; `let X: i32` is not

The biggest single win in either game, and it touches no logic. A tish `const` compiles to a
`static SingleCore<Cell<f64>>`, so `A[b + A_X]` becomes an i32→f64 conversion, an f64 add and an
f64→usize conversion — three soft-float operations, per array access, on a chip with no FPU. Written
`let A_X: i32 = 5` it is a `VmRef<i32>` and the same expression is `(b).wrapping_add(A_X)`.

Converting every scalar constant in `fighter.tish`, `motion.tish`, `beatemup.tish` and both games:
the brawler's tick went 7,800 → 5,700 ticks and its worst frame 12,400 → 10,000; the fighter's worst
frame went 9,860 → 8,990. The generated Rust went from ~600 soft-float constant reads to 3.

Check any ROM with:

```bash
grep -c "G_[A-Z_]*\.with" examples/<name>/.tish/gba/<name>/src/main.rs
```

Most of `packages/` has not had this treatment.

### ⚠️⚠️ Sprite VRAM panics; it does not degrade

`examples/beatemup` died after minutes of play with `panicked at sprite_allocator.rs: have space for
sprites: SpriteFull`, on no particular frame. The 32 KB sprite arena is a GC'd cache, but two things
hold memory permanently and neither is obvious:

- **`sprite_set_visible(h, 0)` does NOT free the sprite's Object.** Four hidden 64x64 attack overlays
  were still holding 8 KB. The fix is one shared overlay sprite re-pointed with `sprite_set_sheet`
  at whoever is currently swinging — at most one swing is on screen at a time anyway.
- **`text_draw` allocates sprite VRAM per letter group.** A 16px banner is several 32x32 objects
  competing with the characters. Anything that is a *word* belongs on the UI canvas (`ui_text`);
  only things that are a *number* need to be sprites, and those are 16x16 digits.

### A crowd needs an attack token

Three rules, and a brawler is unplayable without them: exactly ONE enemy may be committed to an
attack at a time (a token, taken on startup, released on end OR on being hit, expiring on a timer);
being hit CANCELS the victim's attack — the wince, without which enemies swing through their own
hitstun and trade with you while you are mid-combo; and the player gets ~28 mercy frames after a hit
so two attackers cannot loop them.

### Inline the helpers, then look at the algorithm

The brawler's tick started at ~8,450 Timer2 ticks for four actors. Almost none of that was the
physics: a tish function that touches a module array is a boxed closure **wherever it lives**, at
roughly 120 ticks a call, and the tick was making about forty of them a frame — `clampX` alone (two
comparisons) was eight. Inlining `clampX` and the pose lookup, and running the wave check only when
something actually dies, took ~1,300 ticks off. The general rule from §5 applies with more force
here, because an N-actor loop multiplies every helper call by N.
