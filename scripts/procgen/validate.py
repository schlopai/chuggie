"""Checks a generated dungeon is playable, not merely generated.

`scripts/gen_kart_circuit.py` records why a generator needs its own validator: an over-budget
tileset is SILENT — agb substitutes tile 0 — so a generator that can emit any layout must be able to
count what it emitted.
"""


def flood(grid, w, h, start):
    seen = bytearray(w * h)
    stack = [start]
    seen[start] = 1
    n = 1
    while stack:
        i = stack.pop()
        r, c = divmod(i, w)
        for dr, dc in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            rr, cc = r + dr, c + dc
            if 0 <= rr < h and 0 <= cc < w:
                j = rr * w + cc
                if grid[j] and not seen[j]:
                    seen[j] = 1
                    n += 1
                    stack.append(j)
    return n


def connected(grid, w, h):
    """Every floor cell reachable from the first one. A dungeon with an unreachable half is a
    dungeon you can be trapped in, and it looks fine in a screenshot."""
    floors = [i for i, v in enumerate(grid) if v]
    if not floors:
        return False
    return flood(grid, w, h, floors[0]) == len(floors)


def floor_count(grid):
    return sum(1 for v in grid if v)
