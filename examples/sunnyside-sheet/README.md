# Sunnyside sheet — character animation smoke test

<img src="preview.gif" alt="preview" width="480">

Sunnyside de-risk 1, part of the `sunnyside` example family (see
`examples/sunnyside/SPEC.md`).  Proves the baked Sunnyside character sheets on
device: the pack draws every action facing right on a padded 96x64 canvas, and
`scripts/gen_sunnyside_pack.py` crops that to 64x32 player frames (10 actions,
base+hair+tools layers composited) and 32x32 NPC frames with the feet on a
fixed row — so facing left is a single `sprite_set_flip`, not a second sheet.

- left/right walks the player and sets the facing flip
- A / B cycle through the 10 player actions (idle, walk, run, dig, water, axe,
  attack, carry, doing, hurt), label on the HUD
- three NPC sheets (long hair, bowl hair, goblin) idle on the right

Run `./verify.sh` for the full check (regenerates the baked sheets and fails
on drift, builds fresh, drives a key schedule headlessly, greps for crashes).
