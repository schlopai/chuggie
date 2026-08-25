# Sunnyside save — the farm state through the cartridge

Sunnyside de-risk 6, part of the `sunnyside` example family (see
`examples/sunnyside/SPEC.md`).  The whole mutable farm — 96 plot cells of
state/stage/watered plus day, gold, harvest count and the world seed — packed
into `packages/prefs.tish`'s versioned, checksummed SRAM block at 6 bits per
cell, 5 cells per 32-bit slot (24 slots total of the 32 available).

Two claims, two tests:

- **pack/unpack is lossless** — asserted in RAM on every boot against a
  canonical farm that exercises every state/stage/wet combination
  (`PASS packroundtrip`)
- **the cartridge round-trips it** — a three-boot protocol on one `.sav`:
  boot 1 finds fresh SRAM, seeds and saves; boot 2 must restore the identical
  plot hash and scalars, then advances the day and saves again; boot 3 must
  see the advanced day.  `./verify.sh` runs all three and diffs the hashes.
