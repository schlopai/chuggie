# Sunnyside worldgen — the island generator against its Python twin

![preview](preview.gif)

Sunnyside de-risk 3, part of the `sunnyside` example family (see
`examples/sunnyside/SPEC.md`).  Generates the whole farming island on
cartridge from a seed — grass island with noisy coast, a river, four building
stamps (fixed farmhouse barn + jittered shop and two houses with bounded
retries), two-cell-wide L-paths between their doors, the farm plot, and a
forest of tree stamps — then paints it with the baked lip tables and hands it
over in one `tilemap_stream` + one `grid_from_gids(SOLID)` call.

The generator lives in `examples/sunnyside/src/worldgen.tish`, shared with
the rest of the family.  Its file header is a draw-order contract: the order
and count of `rngBelow` draws is mirrored, draw for draw, by
`scripts/procgen/sunnyside.py`, and `./verify.sh` diffs the ROM's per-seed
report (`SS GEN seed=… land=… trees=… hash=…` plus building placements)
against the twin over a 12-seed sweep — with a negative control so an empty
report cannot pass.  A 20-bit rolling hash over the terrain and solidity
planes is what makes "the same world" a checkable claim.

Learned the hard way here: pushing three 3,072-cell arrays from one loop
interleaves their doubling reallocs and fragments the GBA heap until a 64KB
grow fails at boot — one fill loop per array fixes it.

- D-pad walks the farmer (sea, trees and buildings are solid)
- A regenerates with the next seed
