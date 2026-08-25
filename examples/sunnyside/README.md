# Sunnyside — a farming life-sim on a generated island

The flagship of the `sunnyside-*` family (SPEC.md here; six de-risk ROMs
prove the pieces).  A Harvest-Moon-style day on a GBA cartridge, built
entirely from the vendored Sunnyside World pack and the existing engine:

- the island — coast, river, town, forest, farm — is **procedurally
  generated on boot** (seed 7 of `src/worldgen.tish`, the generator whose
  every rng draw is mirrored by `scripts/procgen/sunnyside.py`), lip-model
  autotiled in typed tish, and streamed in one upload
- **farm**: hoe, watering can, seeds, scythe on L/R; three crops (carrot /
  potato / pumpkin) with six growth stages as tile patches; watered cells
  grow while you sleep, at each crop's own pace
- **time**: a quarter-minute-per-frame clock, dusk ramp at 17:00, hardware
  BLDY night, pass-out at 02:00; sleeping at the barn door starts the next
  day and autosaves
- **stamina** drains per tool swing and refills in bed (partially, if you
  fainted in the dirt)
- **gathering**: the axe fells trees in three hits (+2 wood, the map and
  walk grid open up), the rod fishes at any water edge (cast, wait for the
  '!', reel — the pack's three fishing animations), and mushrooms sprout on
  the grass each morning; paths lay plank bridges where they cross the river
- **town**: the store is `packages/shop` — the same greet/buy/sell/quantity
  component shop-demo and oakhollow use, with the pack's item art as list
  icons; a villager gives farming advice; the goblin pays 200G for three
  carrots (a flags-style fetch quest) on `packages/dialog`
- **save**: the whole farm (96 cells x state/crop/stage/watered), day, gold,
  quest, inventory and seeds in `packages/prefs`' checksummed SRAM block —
  restored on the next boot

Controls: D-pad walk · L/R tool (hoe/can/seeds/scythe/axe/rod) · SELECT crop · A use/talk/sleep · B reel away · START begin.

All tish is fully typed (`let x: i32`, parallel typed arrays, no boxed row
tables); the traps hit along the way are recorded where they bit — see the
worldgen header (heap fragmentation, tishlang/tish#663), sunnyside-terrain
(boxed helper calls in paint loops, the world_step/frame input double-pump)
and sunnyside-day (why night is BLDY, not palettes).

`./verify.sh` boots the island, takes the goblin's quest through the dialog
system with a key schedule, sleeps, proves the SRAM restore on a second
boot, and runs a four-day unattended soak asserting pass-out cycles and a
steady heap.
