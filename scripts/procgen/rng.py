"""The engine's RNG, in Python, bit-for-bit.

⚠️ THIS FILE IS A CONTRACT, NOT A CONVENIENCE. It reproduces `packages/rng.tish` exactly — the same
LCG constants, the same 16-bit reduction, the same handling of a zero seed — so that a dungeon
generated on the cartridge and a dungeon generated here from the same seed are THE SAME DUNGEON.

That is what makes a procedural level testable at all. Without it, "the generator works" can only be
checked by looking at it; with it, a verifier re-derives the expected layout in Python and diffs the
ROM's own report against it, headlessly, for a hundred seeds.

Any change here is a change to the cartridge. `packages/rng.tish` carries the same warning in the
other direction, and its constants are additionally pinned to a retired Rust core's conformance
goldens — so this is the third implementation that has to agree, not the second.
"""


class Rng:
    """One stream. `packages/rng.tish` has eight; a generator needs one and takes it explicitly."""

    def __init__(self, seed: int):
        # Matches rngSeed: negatives are folded, and zero is remapped rather than left to produce a
        # degenerate stream.
        v = abs(int(seed))
        self.s = 2463534242 if v == 0 else v

    def next(self) -> int:
        """rngNext: a Numerical Recipes LCG reduced with `>>> 0`, not `%`.

        The reduction matters for equality, not just speed: `>>> 0` is ToUint32, and on the tish side
        it was chosen over `%` because `%` on an f64 is an `fmod` call on ARM7TDMI. Both reduce mod
        2^32, so masking here is the same arithmetic."""
        self.s = (self.s * 1664525 + 1013904223) & 0xFFFFFFFF
        return self.s

    def below(self, n: int) -> int:
        """rngBelow: the HIGH sixteen bits, then a modulo.

        ⚠️ Sixteen bits, not fifteen, and the high ones, not the low. The tish side documents both
        traps: masking with 32767 silently halves the range, and low bits of an LCG are famously
        non-random. `n <= 0` returns WITHOUT drawing; `n == 1` DOES draw, because skipping the draw
        would be arithmetically correct and would desync the stream."""
        if n <= 0:
            return 0
        hi = (self.next() // 65536) & 65535
        return hi % n

    def range(self, lo: int, hi: int) -> int:
        """Inclusive-exclusive [lo, hi). One draw, so it stays in step with the tish side."""
        return lo + self.below(hi - lo)
