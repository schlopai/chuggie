# deckbuild — a deckbuilding combat, on `packages/cards.tish`

<img src="preview.gif" alt="preview" width="480">

Draw five, spend three energy, discard, reshuffle when the draw pile runs dry. One combat against
the Warden, played entirely from `ui_rect` and `ui_text` — no art, no sprites, no OAM. It plays
itself; press anything to take over.

```bash
npm run build && npm start
```

## Why this example exists

`packages/cards.tish` was extracted from `examples/solitaire`, and **a package extracted from one
game is not yet a package** — it is that game's code with a new filename. This is the second
consumer, and it was chosen to be as unlike patience as a card game gets:

| | solitaire | deckbuild |
|---|---|---|
| deck | French, 52 cards, rank + suit | bespoke, 12 cards, cost + effect — `cardRank`/`cardSuit`/`cardIsRed` are never called |
| cards leave | off the top of a pile | from the **middle of the hand**, by index (`pileRemoveAt`) |
| reshuffle | stock recycle, at a fixed moment | draw pile runs dry **mid-draw** (`pileDeal` returns short → `pileRecycle` → `pileShuffle`) |
| cards can leave play | no | yes — PURGE exhausts to a fourth pile |

**`examples/solitaire` was not touched.** That is the bar, the same one `examples/keep` holds
`packages/keylock.tish` to: if the original had needed an edit to fit the extraction, the extraction
had not worked.

## The function the whole example exists to exercise

```
DB reshuffle, draw=9
```

`drawCards` asks for five from a pile holding two. `pileDeal` moves what it can and **reports how
many** — so the caller tips the discard back, reshuffles, and draws the remainder, without counting
cards itself. Patience never has this case; it recycles the stock at a moment it chooses.

## Cards are conserved, and the check is proven to work

`cardsTotal()` is asserted against the deck size every turn, because a move that loses a card hides
for hours and surfaces as a deck that quietly thins over a long combat.

A 30,000-frame soak: **42 combats, 81 reshuffles, 0 leaks, 0 faults.**

An assertion that never fires proves nothing on its own, so it was given a negative control — a
deliberate `pilePop(DISC)` on turn 3:

```
DB *** CARD LEAK: 11 of 12
```

It fires when a card is genuinely lost and is silent otherwise.

## Two things worth knowing

**A pixel check compared against the wrong colour and could never fail.** Confirming a five-card
hand fits on screen meant testing points against the background — but `0x181422` reaches the screen
as `(24, 16, 33)`, because the GBA is 15-bit and drops the low three bits of every channel. Compared
against the *source* constant, every sample counted as "a card is here" and the check reported five
cards on a frame that had two. Sample the background from the frame; never hard-code it.

**The attract player never loses.** It blocks when the Warden telegraphs its big hit and wins in
about six turns, so 41 of 42 soak combats were wins and the `S_OVER === 2` branch is **not exercised
by the soak**. It is reachable by playing badly at the pad; it is not covered automatically.

## Files

| | |
|---|---|
| `packages/cards.tish` | piles, shuffles, deals and moves — no rules, no rendering, no undo |
| `src/main.tish` | the combat: card table, turn loop, paint, and the greedy attract player |
