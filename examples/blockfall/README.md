# blockfall

> *A falling-block puzzle game on the guideline ruleset: SRS kicks, 7-bag, hold, ghost, T-spins, and a budgeted search that plays it. The example tilemap_set8 was added for.*

<img src="preview.gif" alt="preview" width="480">

A falling-block puzzle game on the modern guideline ruleset — SRS rotation with wall kicks, a 7-bag,
hold, ghost, lock delay, T-spins, combos and a level curve — with a budgeted search that plays it by
itself until somebody presses a button.

**Controls** — Left/Right move · Down soft drop · Up hard drop · A rotate CW · B rotate CCW ·
L or R hold · Start pause, and Start again to retry after a top-out.

```bash
npm run build
npm run start
npm run verify
```

## What this demonstrates that grid-demo does not

`packages/grid.tish` is the repo's generic cell-grid kit and the obvious thing to build this on. It
is the wrong thing, and the reason is worth stating plainly because it is the whole difference
between the two genres: **that kit models packed columns with no gaps.** `gridDepth` is a count,
`gridCollapse` closes every hole, and grid-demo's own cascade selftest read 0 until its fixture
stopped leaving a gap in a column. A falling-block game is the opposite — an overhang, and the hole
trapped under it, is the substance of the game. It is what you are punished for and it is what the AI
is scored on. So the board here is its own pair of arrays, and these two examples share a genre and
not a core.

What is shared is `packages/search.tish`, which turns out to be genuinely generic: grid-demo drives
it over (piece, column) pairs for a match-3 dropper and this drives it over (rotation, column) pairs
for a tetromino, with no change to the kit.

## The board is two representations

| | what | who reads it |
|---|---|---|
| `MASK` | one 10-bit occupancy word per row, 24 rows | every rule, and every AI candidate |
| `CELLS` | one colour gid per cell | the painter, and nothing else |

The AI is why. A one-ply search evaluates around thirty placements per piece and has to copy the
board for each; at one word per cell that is 384 writes a candidate and no tick budget makes it fit.
At one word per **row** it is 24 — and the entire surface scan becomes bitwise. Aggregate height,
holes and the column tops come out of a single pass over the row words, using a popcount table:
`POP[seen]` is how many columns have started, and `POP[seen & ~row]` is how many have started but are
empty here, which is the definition of a hole.

## Two engine additions, and why each was unavoidable

**`tilemap_set8`.** Every tilemap call in `crates/tish-agb` addresses a 16×16px cell — `tilemap_set`
writes the 2×2 block of hardware tiles at `(2*col, 2*row)`. A classic well is 10 columns by 20 rows,
which at 16px is 320 pixels tall: taller than the screen, and taller than the map, whose background is
32×32 hardware tiles and therefore 16×16 of those cells. At 8px the well is 80×160 and the HUD gets
the other 144. The new call paints one hardware tile and takes no `cols` argument, because
`include_background_gfx!` bakes tiles row-major and a linear index already names one.

**`sheet8:`.** The 16×16 sprite scheme with an 8×8 block in the corner would have worked, at four
times the sprite VRAM per frame. See below for why sprites were needed at all.

## What actually cost the frames

This game's budget is not its rules and it is not its AI. It is **tile writes: about 310 ticks each**,
measured on device — a boxed native call with five arguments, plus agb's own per-tile work. Three
separate rounds of reasoning about the search were all wrong, and `frame_stats`' drop counter is what
settled it each time.

| what | cost | fix |
|---|---|---|
| the falling piece + ghost as tiles | 16 writes = **4,952 ticks per horizontal move** — over a whole frame, before the rules or the AI ran | 8 sprites, moved by position: ~780 ticks and no erase pass |
| the whole preview queue on one frame | 4 boxes × 12 tiles = **14,500 ticks**, once per piece | one preview slot per frame, round-robin |
| the HUD behind one composite key | 5 × `ui_text` = **3,680 ticks**, and the score changes every row of a soft drop | per-field gates: a score change is one `ui_text` |
| the clear's collapse over all 24 rows | 480 boxed array operations | the band that can actually shift |

The AI, the thing suspected first and instrumented first, peaks at 2,581 of a 4,389-tick frame.

⚠️ **Fields in a `ui_text` HUD must be spaced on tile rows.** `ui_clear_rect` blanks every whole 8×8
tile it touches, so fields 9 pixels apart share one and repainting the score erases the top of the
line count.

## The bug that looked like bad weights

The search enumerates (rotation, column) pairs, and a column target is a `px` — the left edge of the
piece's 4×4 rotation **box**, not of the piece. A vertical I occupies box column 2 alone, so putting
it in board column 0 needs `px = -2`; an O occupies box columns 0 and 1, so reaching column 9 needs
`px = 8`. The first version enumerated 0..9 directly, and the AI simply had no candidate that reached
either end of the well.

It did not look like a missing move. It looked like a weak evaluator: a ragged, holed left edge and
one line cleared every five or six pieces, which is exactly what bad weights look like — and the
weights were the first thing suspected. Fixing only the left end left the O bug, which survived the
same way.

Nothing in `packages/search.tish` can catch this, by design: the search only ever knows how *many*
candidates there are, so what they mean is entirely the game's business. `SELFTEST reach` is the
assertion that was missing — for every piece, every rotation and every column, some target must
produce a legal placement covering it — and it is now the second thing the ROM checks at boot.

## The two tiers

`searchStep` hands out one candidate at a time and stops when the frame's tick budget is spent;
anything that clears a line is shortlisted for a deeper look, drained one entry per call so two
expensive evaluations can never land in the same frame.

The cheap tier scores the resulting surface with the well-known four-feature weights (aggregate
height, lines, holes, bumpiness). The deep tier asks a genuine second-ply question — *can the next
piece clear a line on the board this leaves?* — as an early-exit probe over one rotation.

⚠️ **It is bounded on purpose, and the budget is why.** `searchStep` checks the budget *before*
handing out a pair and cannot preempt the evaluation that follows, so the worst frame is always the
budget plus one whole evaluation. A full two-ply sweep would run ~26 inner evaluations inside a single
un-preemptable call — several frames in one call, which no budget can absorb. Measured: no probe at
all peaked at 2,027 ticks; a four-rotation probe peaked at 4,019 against a 4,389-tick frame, which is
370 ticks of headroom for the entire rest of the game.

The bonus is flat and non-negative for a subtler reason: `search.tish` banks both tiers into the same
best-score register with a strictly-greater comparison, so a deep score on a different scale from the
cheap ones would make shortlisting *change* the ranking rather than refine it.

## Boot selftests

Nine of them, logged before a frame is drawn, because nothing here fails loudly — a rotation table
with a transposed digit, a kick row in the wrong slot, a search space missing two columns: none crash,
none look wrong in a screenshot, and every one just plays a slightly different game.

`rotcells` · `reach` · `feats` · `bag` · `kick` · `clear` · `quad` · `tspin` · `topout` —
see `verify.sh`, which asserts each by exact value and explains what it caught.

The rotations themselves are **generated** from the spawn shape by one formula rather than written
out as 112 numbers, so `rotcells` checks the formula: four cells inside the box, and four turns
returning to where it started.

## Art

From the catalog, not drawn and not borrowed from another example:
`assets/iso-blocks/blocks_flat_16.png`, the Big Pixel Isometric Block Pack by Ajay Karat /
Devil's Work.shop (free for commercial use). `scripts/gen_blockfall.py` box-downscales seven flat
block faces 16→8px, posterises each to three shades of its own mean hue, and bevels it — the bevel is
not decoration, it is what keeps two adjacent cells of the same colour countable, which is the same
readable-without-colour rule grid-demo's gems follow with their glyphs. It bakes the tilemap and the
sprite sheet from the same tiles, so the handover at the instant of a lock is invisible.

⚠️ It asserts the palette ceilings and prints what it used. A 4bpp tile must fit one 16-colour
palette, and there are 16 banks; `include_background_gfx!` does not report a violation, it packs what
it can and the rest comes out the wrong colour on device, which looks like a game bug.

## Attract mode

Touch nothing and it plays itself; touch anything and it never plays itself again. That is the rule
the rest of this repo holds to, and it is what lets a headless run cover a whole game with no key
schedule. `verify.sh` still drives a keyed run, because the attract player never presses anything and
so covers none of rotate, kick, hold, soft drop, hard drop or pause — and it asserts that the AI takes
no further turn for the rest of that run, which is the autoadvance rule as a runtime check.
