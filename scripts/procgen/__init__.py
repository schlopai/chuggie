"""Build-time procedural generation, and the oracle half of the runtime generator.

`rng.py` reproduces `packages/rng.tish` bit-for-bit and `rooms.py` mirrors
`packages/dungeon.tish` draw for draw, so a level generated on the cartridge can be re-derived here
and diffed — which is the only way a procedural level is testable at all.
"""
from .rng import Rng            # noqa: F401
from . import rooms, validate   # noqa: F401
