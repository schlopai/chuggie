# Sunnyside — a Harvest-Moon-style farming game on a generated island

The flagship example for the vendored Sunnyside World pack
(`assets/sunnyside/`): a top-down farming life-sim whose world is procedurally
generated on cartridge, written in fully-typed tish over the existing engine
natives.  This file is the contract the `sunnyside-*` de-risk ladder cites.

## The de-risk ladder

| # | example | proves |
|---|---------|--------|
| 1 | `sunnyside-sheet` | baked 64x32/32x32 character sheets, hflip-only facing, palette budget |
| 2 | `sunnyside-terrain` | lip-model runtime autotiling, one-upload discipline, engine camera |
| 3 | `sunnyside-worldgen` | the full island generator vs `scripts/procgen/sunnyside.py`, seed for seed |
| 4 | `sunnyside-farm` | till/plant/water/harvest via streamed-layer patches, crop growth |
| 5 | `sunnyside-day` | clock, sleep, day/night palette tint |
| 6 | `sunnyside-save` | farm state round-trip through prefs.tish |

Each ships its own `verify.sh`; the main game reuses the pieces.

## World generation (the draw-order contract)

`examples/sunnyside/src/worldgen.tish` and `scripts/procgen/sunnyside.py` are
twins: the ORDER AND COUNT of rng draws is a contract, spelled out in the
worldgen file header.  Phases: island border noise → river → buildings
(barn fixed; shop/house/house2 jittered in zones, 20 bounded retries each) →
L-paths between doors → farm plot (no draws) → 110 tree placement draws.
Any change touches both files in the same commit, and
`sunnyside-worldgen/verify.sh` enforces the match over a seed sweep.

World: 64x48 cells, terrain ids sea/grass/path/soil in `TERR`, solidity in a
separate `SOLID` plane (sea, trees, buildings), painted via the baked lip
tables into `UPLOAD` and streamed in one call.  All three arrays stay inside
the worldgen module (tishlang/tish#663).

## The farming loop (main example scope)

- 4 tools on L/R cycle: hoe, watering can, seeds, scythe; A uses the tool on
  the faced cell, animated with the pack's dig/water/attack/carry actions
- tilled/planted/watered cell state as parallel typed arrays over the farm
  plot; visuals via per-cell `bg_set_tile` patches (one write per player
  action; bulk repaint only at load)
- 3 crops: carrot (cheap/fast), potato (medium), pumpkin (slow/dear); growth
  ticks on sleep, watered cells only
- day clock; dusk/night tint via the hardware brightness blend (fade/BLDY —
  palette rewrites lose: entry order is nondeterministic and sprites would
  stay bright; see sunnyside-day); sleeping in the barn advances the day,
  grows crops, autosaves
- stamina drains per tool use, refilled by sleep; pass-out at 2am
- the shop buys harvest and sells seeds (packages/shop); gold
- 3 NPCs at fixed posts (two humans, one goblin), 2-3 dialog pages each
  (packages/dialog), one fetch quest on packages/flags

## Non-goals (cut to ship)

Mining, livestock, seasons, NPC schedules/pathing, friendship, weather,
tool upgrades, interiors beyond the barn one-room, marriage, festivals.
(Fishing, wood chopping and mushroom foraging shipped after the first cut —
axe/rod tools, a per-day play-rng stream separate from the worldgen
contract, and plank bridges where paths cross water.)  The pack has art for several of these — the catalog keeps them
reachable for later examples.

## Budgets

- Sprite palettes: player + 3 NPC sheets are quantized to one Palette16 each
  by the baker; UI/emotes share; hard ceiling 16 banks
- hud_text strings: clock, gold, stamina, tool name only (budget ~19)
- run() stack: scene split (title / world / interior / shop) with shell
  factories from day one — the 29KB scene-split lesson
- Heap: the three worldgen planes cost ~36KB; one fill loop per array (see
  worldgen.tish header); keep ~60KB free after boot
