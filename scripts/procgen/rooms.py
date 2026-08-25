"""BSP rooms and corridors — the Python half of a two-implementation generator.

⚠️ THE ORDER AND COUNT OF `rng` DRAWS IS THE CONTRACT. `packages/dungeon.tish` performs exactly the
same draws in exactly the same order, so the same seed produces the same dungeon on the cartridge
and here. Adding a draw, reordering two, or short-circuiting one on a branch that the other side
does not take will desync the two — silently, and only for some seeds, which is the worst way for
this to break.

The split is LEVEL-ORDER and iterative rather than recursive: recursion is expensive in tish and its
traversal order is one more thing the two implementations would have to agree about by accident.
"""

WALL, FLOOR = 0, 1


def bsp_regions(rng, w: int, h: int, depth: int):
    """Split the interior into 2**depth regions, one draw per split."""
    regions = [(1, 1, w - 2, h - 2)]
    for _ in range(depth):
        nxt = []
        for (x, y, rw, rh) in regions:
            # Which axis is decided by SHAPE, not by a draw — a draw here would be a coin flip the
            # tish side would have to make identically for no gain.
            if rw >= rh:
                cut = (rw // 4) + rng.below(max(1, rw // 2))
                if cut < 2:
                    cut = 2
                if cut > rw - 2:
                    cut = rw - 2
                nxt.append((x, y, cut, rh))
                nxt.append((x + cut, y, rw - cut, rh))
            else:
                cut = (rh // 4) + rng.below(max(1, rh // 2))
                if cut < 2:
                    cut = 2
                if cut > rh - 2:
                    cut = rh - 2
                nxt.append((x, y, rw, cut))
                nxt.append((x, y + cut, rw, rh - cut))
        regions = nxt
    return regions


def rooms_in(rng, regions):
    """One room per region: four draws each, always, even when the region is too small to vary."""
    out = []
    for (x, y, rw, rh) in regions:
        maxw = max(2, rw - 2)
        maxh = max(2, rh - 2)
        w = 2 + rng.below(max(1, maxw - 1))
        h = 2 + rng.below(max(1, maxh - 1))
        ox = rng.below(max(1, rw - w))
        oy = rng.below(max(1, rh - h))
        out.append((x + ox, y + oy, w, h))
    return out


def carve(grid, w, rooms):
    for (rx, ry, rw, rh) in rooms:
        for r in range(ry, ry + rh):
            for c in range(rx, rx + rw):
                grid[r * w + c] = FLOOR


def connect(grid, w, rooms):
    """L-corridors between consecutive rooms. No draws: the elbow is always horizontal-then-vertical,
    so the two implementations cannot disagree about it."""
    for i in range(len(rooms) - 1):
        ax, ay = rooms[i][0] + rooms[i][2] // 2, rooms[i][1] + rooms[i][3] // 2
        bx, by = rooms[i + 1][0] + rooms[i + 1][2] // 2, rooms[i + 1][1] + rooms[i + 1][3] // 2
        for c in range(min(ax, bx), max(ax, bx) + 1):
            grid[ay * w + c] = FLOOR
        for r in range(min(ay, by), max(ay, by) + 1):
            grid[r * w + bx] = FLOOR


def generate(rng, w: int, h: int, depth: int = 3):
    """The whole thing. Returns (grid, rooms)."""
    grid = [WALL] * (w * h)
    regions = bsp_regions(rng, w, h, depth)
    rooms = rooms_in(rng, regions)
    carve(grid, w, rooms)
    connect(grid, w, rooms)
    return grid, rooms
