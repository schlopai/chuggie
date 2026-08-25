"""The Sunnyside island generator, in Python, draw for draw.

⚠️ THIS FILE IS A CONTRACT.  It is the twin of
`examples/sunnyside/src/worldgen.tish` — same phases, same order and count of
rng draws (the file header there spells the contract out).  Change either
side and you MUST change the other in the same commit;
`examples/sunnyside-worldgen/verify.sh` diffs the two over a seed sweep.
"""
from .rng import Rng

W, H = 64, 48
T_SEA, T_GRASS, T_PATH, T_SOIL, T_BRIDGE = 0, 1, 2, 3, 4
TREE_ATTEMPTS = 110

# stamp footprints — must match the baked stamps' sizes
# (assets/sunnyside/baked/world_tiles.json); order: shop, house, house2, barn
SIZES = {"shop": (11, 11), "house": (7, 6), "house2": (8, 11), "barn": (7, 7)}
# solid insets (L, T, R, B) — the crop margins stay walkable, mirror of the baker
INSETS = {"shop": (1, 1, 1, 2), "house": (0, 0, 0, 0), "house2": (1, 0, 1, 1), "barn": (1, 1, 1, 1)}
FARM_W, FARM_H = 12, 8


class World:
    def __init__(self):
        self.terr = [T_SEA] * (W * H)
        self.solid = [0] * (W * H)
        self.bld = {}  # name -> (x, y, w, h)
        self.trees = 0
        self.farm = (0, 0)

    def door(self, name):
        x, y, w, h = self.bld[name]
        return x + w // 2, y + h

    def rect_free(self, x0, y0, w, h):
        if x0 < 1 or y0 < 1 or x0 + w >= W - 1 or y0 + h >= H - 1:
            return False
        for y in range(y0 - 1, y0 + h + 1):
            for x in range(x0 - 1, x0 + w + 1):
                i = y * W + x
                if self.terr[i] != T_GRASS or self.solid[i] > 0:
                    return False
        return True

    def mark_solid(self, x0, y0, w, h):
        for y in range(y0, y0 + h):
            for x in range(x0, x0 + w):
                self.solid[y * W + x] = 1

    def in_building(self, x, y):
        for (bx, by, bw, bh) in self.bld.values():
            if bx <= x < bx + bw and by <= y < by + bh:
                return True
        return False

    def path_cell(self, x, y):
        if 0 <= x < W and 0 <= y < H:
            i = y * W + x
            if self.terr[i] == T_GRASS and self.solid[i] == 0:
                self.terr[i] = T_PATH
            elif self.terr[i] == T_SEA:
                self.terr[i] = T_BRIDGE
            elif self.terr[i] == T_GRASS and self.solid[i] == 1 and not self.in_building(x, y):
                self.terr[i] = T_PATH
                self.solid[i] = 0

    def carve_path(self, rng, x0, y0, x1, y1):
        del rng  # carving draws nothing now: both elbows are cut
        for corner in (0, 1):
            self.carve_l(x0, y0, x1, y1, corner)

    def carve_l(self, x0, y0, x1, y1, corner):
        cx, cy = (x0, y1) if corner == 1 else (x1, y0)
        sx = x0
        while sx != cx:
            self.path_cell(sx, y0)
            self.path_cell(sx, y0 + 1)
            sx += 1 if sx < cx else -1
        sy = y0
        while sy != cy:
            self.path_cell(x0, sy)
            self.path_cell(x0 + 1, sy)
            sy += 1 if sy < cy else -1
        sx = cx
        while sx != x1:
            self.path_cell(sx, cy)
            self.path_cell(sx, cy + 1)
            sx += 1 if sx < x1 else -1
        sy = cy
        while sy != y1:
            self.path_cell(cx, sy)
            self.path_cell(cx + 1, sy)
            sy += 1 if sy < y1 else -1
        self.path_cell(x1, y1)


def generate(seed: int) -> World:
    rng = Rng(seed)
    w = World()

    # 1. island border noise, row-major
    for y in range(3, H - 3):
        for x in range(3, W - 3):
            edge = x == 3 or x == W - 4 or y == 3 or y == H - 4
            if edge:
                if rng.below(100) < 55:
                    w.terr[y * W + x] = T_GRASS
            else:
                w.terr[y * W + x] = T_GRASS

    # 1b. one CA smoothing pass over the border band (no rng draws) — the
    # exact mirror of worldgen.tish
    verdict = [0] * (W * H)
    for y in range(2, H - 2):
        for x in range(2, W - 2):
            if x <= 5 or x >= W - 6 or y <= 5 or y >= H - 6:
                i = y * W + x
                nb = sum(1 for d in (-W - 1, -W, -W + 1, -1, 1, W - 1, W, W + 1)
                         if w.terr[i + d] > 0)
                if nb >= 5:
                    verdict[i] = 1
                elif nb <= 3:
                    verdict[i] = 2
    for y in range(2, H - 2):
        for x in range(2, W - 2):
            if x <= 5 or x >= W - 6 or y <= 5 or y >= H - 6:
                i = y * W + x
                if verdict[i] == 1:
                    w.terr[i] = T_GRASS
                elif verdict[i] == 2:
                    w.terr[i] = T_SEA

    # 2. the river
    rx = 12 + rng.below(W - 24)
    for ry in range(3, H - 3):
        w.terr[ry * W + rx] = T_SEA
        w.terr[ry * W + rx + 1] = T_SEA
        d = rng.below(4)
        if d == 0 and rx > 8:
            rx -= 1
        if d == 1 and rx < W - 10:
            rx += 1

    # 3. buildings: barn fixed, then shop/house/house2 jitter with 20 retries
    bw, bh = SIZES["barn"]
    w.bld["barn"] = (6, H - 18, bw, bh)
    il, it_, ir, ib = INSETS["barn"]
    w.mark_solid(6 + il, H - 18 + it_, bw - il - ir, bh - it_ - ib)
    zones = {"shop": (26, 6, 26, 10), "house": (44, 14, 18, 12), "house2": (5, 8, 18, 12)}
    for name in ("shop", "house", "house2"):
        zw_, zh_ = SIZES[name]
        zx, zy, zw, zh = zones[name]
        placed = False
        for _ in range(20):
            if placed:
                break
            px = zx + rng.below(zw - zw_)
            py = zy + rng.below(zh - zh_)
            if w.rect_free(px, py, zw_, zh_):
                w.bld[name] = (px, py, zw_, zh_)
                il, it_, ir, ib = INSETS[name]
                w.mark_solid(px + il, py + it_, zw_ - il - ir, zh_ - it_ - ib)
                placed = True
        if not placed:
            w.bld[name] = (zx, zy, zw_, zh_)
            il, it_, ir, ib = INSETS[name]
            w.mark_solid(zx + il, zy + it_, zw_ - il - ir, zh_ - it_ - ib)

    # 4. paths
    w.carve_path(rng, *w.door("barn"), *w.door("shop"))
    w.carve_path(rng, *w.door("shop"), *w.door("house"))
    w.carve_path(rng, *w.door("house2"), *w.door("shop"))

    # 5. the farm plot (no draws)
    bx, by, bw, bh = w.bld["barn"]
    fx, fy = bx + bw + 2, by + 2
    w.farm = (fx, fy)
    for y in range(fy, fy + FARM_H):
        for x in range(fx, fx + FARM_W):
            i = y * W + x
            if w.terr[i] == T_GRASS and w.solid[i] == 0:
                w.terr[i] = T_SOIL

    # 6. trees: exactly TREE_ATTEMPTS coordinate draws
    for _ in range(TREE_ATTEMPTS):
        tx = rng.below(W)
        ty = rng.below(H)
        if w.rect_free(tx, ty, 2, 3):
            w.mark_solid(tx, ty, 2, 3)
            w.trees += 1

    # sea is solid
    for i in range(W * H):
        if w.terr[i] == T_SEA:
            w.solid[i] = 1
    return w


def land(w: World) -> int:
    return sum(1 for t in w.terr if t > 0)


def world_hash(w: World) -> int:
    h = 7
    for i in range(W * H):
        h = ((h << 5) - h + w.terr[i] * 2 + w.solid[i]) & 1048575
    return h


def report(seed: int) -> list[str]:
    """The exact lines the ROM logs for one generation."""
    w = generate(seed)
    lines = [
        f"SS GEN seed={seed} land={land(w)} trees={w.trees} hash={world_hash(w)}",
        "SS BLD shop={},{} house={},{} house2={},{} farm={},{}".format(
            w.bld["shop"][0], w.bld["shop"][1],
            w.bld["house"][0], w.bld["house"][1],
            w.bld["house2"][0], w.bld["house2"][1],
            w.farm[0], w.farm[1]),
    ]
    return lines


if __name__ == "__main__":
    import sys
    for s in [int(a) for a in sys.argv[1:]] or [1]:
        for line in report(s):
            print(line)
