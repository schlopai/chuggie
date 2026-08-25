#!/usr/bin/env python3
"""Generate every asset and data table for examples/kart-circuit.

Run from the repo root:  python3 scripts/gen_kart_circuit.py

Emits into examples/kart-circuit/assets/
    track.png     512x512 RGB   'affine:'   the course, drawn TOP-DOWN

and into examples/kart-circuit/src/
    track.tish    the surface mask, waypoints, checkpoints and starting grid

ONE SOURCE OF TRUTH.  The circuit is a single closed spline.  The art, the
surface mask the physics reads, the AI's waypoints and the lap checkpoints are
all derived from it here.  A track whose art and collision are authored
separately is one where you can be told you are on grass while looking at
tarmac, and nothing in the game can detect the disagreement.

⚠️ THE 256-TILE CEILING, which is why this file autotiles instead of painting.

An affine background stores each map entry as a single BYTE, so it can address
only 256 distinct tiles — and agb does not complain when you exceed that.
`set_tile_at_pos` silently substitutes tile 0 for any index over 255
(agb/src/display/tiled/affine_background.rs:328), so an over-budget track does
not fail to build and does not panic: it quietly paints holes in the course.

Painting the circuit freehand and slicing it produced 925 unique tiles, because
a smooth curve crossing an 8px grid makes a near-unique 8x8 pattern at every
step along its length.  Stripping the centre line, the kerb stripes and the
grass check only brought it to 729 — the cost is the road EDGE itself, not the
decoration.

So the art is generated the way a real Mode 7 racer's was: from a small tile
set, indexed by where each cell sits relative to the track.  Each cell is drawn
by interpolating the distance-to-centre-line across it, which keeps the band
edges smooth, and tiles are cached by a quantised signature of that distance at
the cell's four corners.  Cells far from any edge all share one signature, so
the whole infield and the whole road interior cost two tiles each.

Other constraints:
  * 64x64 tiles is the largest affine map that boots (128x128 panics inside
    agb), so the world is exactly 512x512 texture pixels, and it WRAPS.
  * The affine layer owns all 256 background palette entries and the ui_* canvas
    hardcodes bank 15, so the track stays well under 240 colours and leaves
    240-255 for a UI canvas that may want them.
  * Scanlines above the horizon sample the texture's ORIGIN TEXEL, so pixel
    (0,0) is painted the backdrop colour and becomes the sky.
"""

import math
import os

import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "examples", "kart-circuit", "assets")
EX_SRC = os.path.join(ROOT, "examples", "kart-circuit", "src")

# ── The world ────────────────────────────────────────────────────────────────
WORLD = 512            # texture pixels; 64 tiles of 8px, the largest map that boots
CELL = 8               # one surface-mask cell per tile
CELLS = WORLD // CELL   # 64

ROAD_HW = 23.0         # half-width of tarmac, world pixels
KERB_W = 6.0           # rumble strip either side

TILE_BUDGET = 256      # hardware: affine map entries are one byte

# Surface classes.  Kept in step with SURF_* in packages/kart.tish.
S_GRASS = 0
S_ROAD = 1
S_KERB = 2
S_BOOST = 3
S_FINISH = 4

# ── Palette (few colours, flat fills — this is what keeps tiles reusable) ─────
SKY = (36, 40, 64)              # must match backdrop() in main.tish
C_GRASS_A = (58, 122, 62)
C_GRASS_B = (50, 110, 55)
C_ROAD_A = (88, 88, 98)
C_ROAD_B = (80, 80, 90)
C_KERB_A = (198, 66, 58)
C_KERB_B = (232, 226, 214)
C_BOOST_A = (240, 176, 48)
C_BOOST_B = (250, 226, 120)
C_FIN_A = (238, 236, 226)
C_FIN_B = (34, 34, 40)

# Item sprites, drawn as one 16px strip: 0 box · 1 banana · 2 shell.
C_BOX_A = (96, 208, 240)
C_BOX_B = (40, 128, 176)
C_BOX_C = (232, 244, 252)
C_BANANA = (244, 212, 72)
C_BANANA_D = (168, 132, 24)
C_SHELL_A = (232, 88, 72)
C_SHELL_B = (150, 44, 40)
C_SHELL_C = (240, 236, 224)

# The circuit.  A long start straight up the left, a fast right-hand sweep, a
# tight hairpin top right and an S through the middle — enough corner variety
# that drift is worth using and the AI's line is not a circle.
CONTROL = [
    (120, 404),
    (108, 250),
    (152, 128),
    (272, 92),
    (386, 126),
    (412, 224),
    (330, 282),
    (302, 352),
    (366, 424),
    (248, 452),
]

BOOST_AT = [0.17, 0.54, 0.81]     # fraction of a lap
N_WAYPOINTS = 24                  # AI steering targets
N_CHECKPOINTS = 8                 # ordered gates; lap only counts if all were hit
# Item boxes: rows of three across the road, placed by position ALONG the lap so they stay on the
# racing line if the control points move.
BOX_AT = [0.10, 0.42, 0.74]
BOX_SPREAD = 14.0                 # lateral gap between the three in a row


def catmull_rom_closed(points, per_span=48):
    """A closed Catmull-Rom spline through `points`, as a dense polyline.

    The circuit is authored as a handful of control points because that is the
    part a human tunes; everything downstream wants a dense centre-line.
    """
    n = len(points)
    out = []
    for i in range(n):
        p0, p1 = points[(i - 1) % n], points[i]
        p2, p3 = points[(i + 1) % n], points[(i + 2) % n]
        for s in range(per_span):
            t = s / per_span
            t2, t3 = t * t, t * t * t
            out.append((
                0.5 * (2 * p1[0] + (-p0[0] + p2[0]) * t
                       + (2 * p0[0] - 5 * p1[0] + 4 * p2[0] - p3[0]) * t2
                       + (-p0[0] + 3 * p1[0] - 3 * p2[0] + p3[0]) * t3),
                0.5 * (2 * p1[1] + (-p0[1] + p2[1]) * t
                       + (2 * p0[1] - 5 * p1[1] + 4 * p2[1] - p3[1]) * t2
                       + (-p0[1] + 3 * p1[1] - 3 * p2[1] + p3[1]) * t3),
            ))
    return out


def arc_lengths(line):
    """Cumulative distance along the closed polyline, and its total length."""
    pts = np.asarray(line)
    d = np.hypot(*(np.roll(pts, -1, axis=0) - pts).T)
    return np.concatenate([[0.0], np.cumsum(d)[:-1]]), float(d.sum())


def distance_field(line, xs, ys):
    """Distance from each (x, y) to the nearest point on the centre-line.

    Also returns which sample was nearest, which is what places the boost pads
    and the start line along the lap rather than in space.
    """
    pts = np.asarray(line)
    q = np.stack([xs.ravel(), ys.ravel()], axis=1)
    best = np.full(len(q), 1e9)
    idx = np.zeros(len(q), dtype=np.int32)
    # Chunked so the (points x samples) matrix stays small.
    for i in range(0, len(pts), 64):
        blk = pts[i:i + 64]
        d = np.hypot(q[:, 0, None] - blk[None, :, 0], q[:, 1, None] - blk[None, :, 1])
        m = d.argmin(axis=1)
        dm = d[np.arange(len(q)), m]
        take = dm < best
        best[take] = dm[take]
        idx[take] = (i + m)[take]
    return best.reshape(xs.shape), idx.reshape(xs.shape)


def band_of(d):
    """Which surface a distance-from-centre falls in."""
    if d <= ROAD_HW:
        return S_ROAD
    if d <= ROAD_HW + KERB_W:
        return S_KERB
    return S_GRASS


class TileSet:
    """8x8 tiles, cached by signature so the same situation costs one tile.

    The signature is the quantised distance-to-centre-line at the cell's four
    corners plus whatever overlay the cell carries.  Two cells deep in the
    infield have identical signatures and therefore share a tile; only cells
    near a band edge are distinct, and there the quantisation bounds how many
    variants a curve can produce.
    """

    # ⚠️ These two numbers ARE the tile budget, measured by sweeping them:
    #     quant 3 / window 20 -> 906 tiles     quant 4 / window 14 -> 383
    #     quant 3 / window 10 -> 379           quant 4 / window 10 -> 202
    # Coarsening past this starts to facet the road edge visibly; refining past
    # it silently overruns the 256-entry map and agb paints tile 0 into the
    # course.  Re-run the generator and read the printed count after any change
    # that adds detail — it is the only warning you get.
    def __init__(self, quant=4.0, window=10.0):
        self.quant = quant
        self.window = window
        self.tiles = {}
        self.order = []

    def sig(self, corners, overlay, parity):
        q = []
        for d in corners:
            # Only the neighbourhood of the two band edges needs resolving;
            # everything further away is "far inside" or "far outside".
            t = max(-self.window, min(self.window, d - ROAD_HW))
            q.append(int(round(t / self.quant)))
        # Parity only belongs in the signature when the cell actually shows a
        # parity-coloured band. Without this, every road-interior cell exists
        # twice for a checker nobody can see there.
        bands = [band_of(d) for d in corners]
        shows_parity = S_GRASS in bands or S_KERB in bands
        return (tuple(q), overlay, parity if shows_parity else 0)

    def get(self, corners, overlay, parity):
        s = self.sig(corners, overlay, parity)
        t = self.tiles.get(s)
        if t is None:
            t = self._render(corners, overlay, parity)
            self.tiles[s] = t
            self.order.append(s)
        return t

    def _render(self, corners, overlay, parity):
        d00, d10, d01, d11 = corners
        px = np.zeros((CELL, CELL, 3), dtype=np.uint8)
        for y in range(CELL):
            fy = (y + 0.5) / CELL
            for x in range(CELL):
                fx = (x + 0.5) / CELL
                # Bilinear across the cell keeps the band edge smooth even
                # though the tile itself is one of a small set.
                d = ((d00 * (1 - fx) + d10 * fx) * (1 - fy)
                     + (d01 * (1 - fx) + d11 * fx) * fy)
                b = band_of(d)
                if b == S_ROAD and overlay == S_BOOST:
                    c = C_BOOST_A if ((x + y) // 3) & 1 else C_BOOST_B
                elif b == S_ROAD and overlay == S_FINISH:
                    c = C_FIN_A if ((x // 4) + (y // 4)) & 1 else C_FIN_B
                elif b == S_ROAD:
                    c = C_ROAD_A if ((x + y) >> 2) & 1 else C_ROAD_B
                elif b == S_KERB:
                    c = C_KERB_A if parity else C_KERB_B
                else:
                    c = C_GRASS_A if parity else C_GRASS_B
                px[y, x] = c
        return px


def build():
    line = catmull_rom_closed(CONTROL)
    cum, total = arc_lengths(line)

    # Distance at every cell CORNER (for the art) and CENTRE (for the mask).
    cx = np.arange(CELLS + 1) * CELL
    gx, gy = np.meshgrid(cx.astype(float), cx.astype(float), indexing="xy")
    dcorner, _ = distance_field(line, gx, gy)

    mx = (np.arange(CELLS) * CELL + CELL / 2).astype(float)
    mgx, mgy = np.meshgrid(mx, mx, indexing="xy")
    dcentre, icentre = distance_field(line, mgx, mgy)

    # Overlays are placed by position ALONG the lap, so they stay on the racing
    # line if the control points move.
    boost_s = [f * total for f in BOOST_AT]
    overlay = np.zeros((CELLS, CELLS), dtype=np.int32)
    for cy in range(CELLS):
        for cxi in range(CELLS):
            if dcentre[cy, cxi] > ROAD_HW:
                continue
            s = cum[icentre[cy, cxi]]
            if s < 14.0 or s > total - 14.0:
                overlay[cy, cxi] = S_FINISH
            else:
                for bs in boost_s:
                    if abs(s - bs) < 12.0:
                        overlay[cy, cxi] = S_BOOST
                        break

    ts = TileSet()
    img = np.zeros((WORLD, WORLD, 3), dtype=np.uint8)
    surf = []
    for cy in range(CELLS):
        for cxi in range(CELLS):
            corners = (dcorner[cy, cxi], dcorner[cy, cxi + 1],
                       dcorner[cy + 1, cxi], dcorner[cy + 1, cxi + 1])
            # ⚠️ 16px blocks (two cells), not 8px. An 8px check is exactly the tile pitch, so in the
            # far field — where one screen pixel covers several texels — it beats against the
            # sampling grid and turns to speckle. Halving its spatial frequency calms that
            # considerably and costs nothing: it is still the same two tiles, just in 2x2 groups.
            parity = ((cxi >> 1) + (cy >> 1)) & 1
            ov = int(overlay[cy, cxi])
            img[cy * CELL:(cy + 1) * CELL, cxi * CELL:(cxi + 1) * CELL] = ts.get(corners, ov, parity)
            b = band_of(dcentre[cy, cxi])
            surf.append(ov if (ov and b == S_ROAD) else b)

    return line, cum, total, img, surf, ts


# ── Karts ────────────────────────────────────────────────────────────────────
# The GBA cannot rotate or scale a sprite (no affine-object wrapper exists), so
# a kart seen from a moving camera needs a BAKED frame per relative heading, and
# a second, smaller sheet for when it is far away.  Eight headings is what Super
# The classic SNES kart racers used this and it is enough: the eye reads the in-between angles from the
# motion of the floor.
#
# Frame index is the heading RELATIVE to the camera, in eighths of a turn:
#   0 = driving away from you (you see its back)      4 = coming at you
#   2 = crossing left to right                        6 = crossing right to left
# A racer's frames are contiguous, so frame = racer * 8 + heading.
KART_HUES = [
    ((228, 72, 64), (150, 40, 36)),      # 0 red     — the player
    ((72, 140, 232), (40, 82, 150)),     # 1 blue
    ((96, 200, 96), (52, 128, 58)),      # 2 green
    ((236, 196, 64), (156, 122, 32)),    # 3 yellow
]
C_TYRE = (30, 28, 34)
C_TYRE_HI = (72, 70, 80)
C_SKIN = (232, 186, 148)
C_VISOR = (60, 84, 132)
C_TRIM = (240, 240, 236)

KART_PITCH = 0.52      # how far the camera looks down: 1.0 = plan view, 0 = side on
KART_ZS = 0.86         # vertical scale applied to height, same projection


def _proj(lx, ly, lz, yaw_turns, cx, cy, s):
    """Oblique projection of a kart-local point at a given heading.

    The kart is modelled top-down and then squashed vertically, which is what a
    camera a little above the ground does to a flat thing on it.  Height lifts
    the point up the screen.  Cheap, and consistent across all eight frames,
    which matters more than accuracy — an inconsistent set reads as wobble.
    """
    a = yaw_turns * 2.0 * math.pi
    ca, sa = math.cos(a), math.sin(a)
    rx = lx * ca - ly * sa
    ry = lx * sa + ly * ca
    return (cx + rx * s, cy - ry * KART_PITCH * s - lz * KART_ZS * s)


def draw_kart(size, heading, hue, dark):
    """One kart frame, `size` px square, at `heading` eighths of a turn."""
    from PIL import ImageDraw
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    s = size / 32.0
    cx, cy = size / 2.0, size * 0.62
    yaw = heading / 8.0

    def blob(lx, ly, lz, rx, ry, fill):
        px, py = _proj(lx, ly, lz, yaw, cx, cy, s)
        d.ellipse([px - rx * s, py - ry * s, px + rx * s, py + ry * s], fill=fill)

    def quad(pts, lz, fill):
        d.polygon([_proj(px, py, lz, yaw, cx, cy, s) for px, py in pts], fill=fill)

    # Painter's order: whichever wheels are further away first.  Sorting by the
    # projected depth means one code path covers every heading.
    wheels = [(-7, -9), (7, -9), (-7, 9), (7, 9)]
    def depth(w):
        a = yaw * 2 * math.pi
        return w[0] * math.sin(a) + w[1] * math.cos(a)
    for wx, wy in sorted(wheels, key=depth):
        blob(wx, wy, 2.6, 3.4, 3.0, C_TYRE)
        blob(wx, wy, 3.6, 2.0, 1.5, C_TYRE_HI)

    # Chassis, then a lighter deck on top so the body has a readable top face.
    quad([(-6, -11), (6, -11), (6, 11), (-6, 11)], 3.0, dark)
    quad([(-5, -9), (5, -9), (5, 9), (-5, 9)], 5.2, hue)
    quad([(-4, -9), (4, -9), (4, -4), (-4, -4)], 5.4, C_TRIM)

    # Driver: shoulders, helmet, visor facing forward.
    blob(0, 1, 7.0, 4.6, 4.0, dark)
    blob(0, 0, 11.0, 4.0, 3.8, hue)
    blob(0, -2.5, 10.4, 2.6, 2.2, C_VISOR if heading in (3, 4, 5) else C_SKIN)
    return im


def draw_items():
    """One 16px strip: 0 item box · 1 banana · 2 shell.

    Small, high-contrast and readable at billboard scale — at forty pixels away
    on a moving floor, silhouette is the only thing that survives.
    """
    from PIL import ImageDraw
    im = Image.new("RGBA", (16 * 3, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)

    # Box: a floating cube with a bright question mark face.
    d.polygon([(3, 5), (8, 2), (13, 5), (8, 8)], fill=C_BOX_A)
    d.polygon([(3, 5), (8, 8), (8, 14), (3, 11)], fill=C_BOX_B)
    d.polygon([(13, 5), (8, 8), (8, 14), (13, 11)], fill=C_BOX_A)
    d.rectangle([6, 8, 6, 9], fill=C_BOX_C)
    d.rectangle([7, 7, 8, 8], fill=C_BOX_C)
    d.rectangle([7, 10, 7, 11], fill=C_BOX_C)

    # Banana: a crescent.
    d.ellipse([17 + 1, 4, 17 + 12, 14], fill=C_BANANA)
    d.ellipse([17 + 4, 2, 17 + 15, 12], fill=(0, 0, 0, 0))
    d.ellipse([17 + 2, 6, 17 + 9, 13], fill=C_BANANA_D)
    d.ellipse([17 + 4, 5, 17 + 12, 12], fill=(0, 0, 0, 0))

    # Shell: a domed shell with a pale rim.
    d.ellipse([33 + 2, 4, 33 + 13, 13], fill=C_SHELL_A)
    d.ellipse([33 + 4, 6, 33 + 11, 11], fill=C_SHELL_B)
    d.rectangle([33 + 2, 11, 33 + 13, 13], fill=C_SHELL_C)
    return im


def make_kart_sheets():
    """Both size tiers, sharing one palette so they can share a sprite bank."""
    out = {}
    for name, size in (("karts32.png", 32), ("karts16.png", 16)):
        sheet = Image.new("RGBA", (size * 8 * len(KART_HUES), size), (0, 0, 0, 0))
        for r, (hue, dark) in enumerate(KART_HUES):
            for h in range(8):
                sheet.paste(draw_kart(size, h, hue, dark), ((r * 8 + h) * size, 0))
        sheet.save(os.path.join(OUT, name))
        cols = {c for _, c in (sheet.getcolors(maxcolors=1 << 16) or []) if c[3] > 0}
        out[name] = (sheet.size, len(cols))
    items = draw_items()
    items.save(os.path.join(OUT, "items.png"))
    cols = {c for _, c in (items.getcolors(maxcolors=1 << 16) or []) if c[3] > 0}
    out["items.png"] = (items.size, len(cols))
    return out


def unique_tiles(arr):
    seen = set()
    for ty in range(arr.shape[0] // 8):
        for tx in range(arr.shape[1] // 8):
            seen.add(arr[ty * 8:ty * 8 + 8, tx * 8:tx * 8 + 8].tobytes())
    return len(seen)


def sample_at(line, cum, total, s):
    """The point on the centre-line `s` world-pixels around the lap."""
    s = s % total
    i = int(np.searchsorted(cum, s)) % len(line)
    return line[i][0], line[i][1], i


def yaw_at(line, i):
    """Heading of travel at sample `i`, in 1/256ths of a turn.

    ⚠️ The renderer's convention, not maths': `m7Move` steps by
    `x += sin(yaw)*fwd, z += cos(yaw)*fwd`, so yaw is measured from +z toward
    +x — hence atan2(dx, dz) and not the usual atan2(dz, dx).
    """
    j = (i + 6) % len(line)
    dx = line[j][0] - line[i][0]
    dz = line[j][1] - line[i][1]
    return int(round(math.atan2(dx, dz) / (2 * math.pi) * 256)) % 256


def pack_surface(surf):
    """4 bits per cell, 8 cells per i32 — 4096 cells in 512 words, 2 KB.

    One i32 per cell would be 16 KB of heap for a table that is read once per
    kart per frame, and heap is the scarce thing here.  Four bits is enough for
    the five surface classes with room to spare, and because no class reaches 8
    the top nibble never sets bit 31, so every word stays a positive i32 — which
    tish needs, since it has no unsigned type.
    """
    words = []
    for w in range(0, len(surf), 8):
        v = 0
        for k in range(8):
            v |= (surf[w + k] & 15) << (k * 4)
        words.append(v)
    assert all(0 <= v <= 0x7FFFFFFF for v in words), "a surface word went negative"
    return words


def tish_array(name, values, per_line=16):
    body = []
    for i in range(0, len(values), per_line):
        body.append("  " + ", ".join(str(v) for v in values[i:i + per_line]) + ",")
    text = "\n".join(body).rstrip(",")
    return f"export let {name}: i32[] = [\n{text}\n]\n"


def emit_track_tish(line, cum, total, surf):
    """Everything the game needs to know about the course, from the same spline."""
    waypoints = [sample_at(line, cum, total, total * k / N_WAYPOINTS) for k in range(N_WAYPOINTS)]
    checks = [sample_at(line, cum, total, total * k / N_CHECKPOINTS) for k in range(N_CHECKPOINTS)]

    # The starting grid: two by two, behind the line, staggered like a real one.
    sx, sz, si = sample_at(line, cum, total, 0.0)
    syaw = yaw_at(line, si)
    fwd = (math.sin(syaw / 256 * 2 * math.pi), math.cos(syaw / 256 * 2 * math.pi))
    side = (fwd[1], -fwd[0])
    grid = []
    for slot in range(4):
        back = 20.0 + (slot // 2) * 22.0
        lat = -11.0 if (slot % 2) == 0 else 11.0
        grid.append((round(sx - fwd[0] * back + side[0] * lat),
                     round(sz - fwd[1] * back + side[1] * lat)))

    boxes = []
    for frac in BOX_AT:
        bx, bz, bi = sample_at(line, cum, total, total * frac)
        byaw = yaw_at(line, bi)
        side = (math.cos(byaw / 256 * 2 * math.pi), -math.sin(byaw / 256 * 2 * math.pi))
        for lane in (-1, 0, 1):
            boxes.append((round(bx + side[0] * BOX_SPREAD * lane),
                          round(bz + side[1] * BOX_SPREAD * lane)))

    parts = [
        "// GENERATED by scripts/gen_kart_circuit.py — do not edit.\n",
        "//\n",
        "// Every table here comes from the SAME centre-line spline that drew track.png, so the art,\n",
        "// the surface the physics reads, the line the AI follows and the gates that validate a lap\n",
        "// cannot disagree with each other. Re-run the generator after changing CONTROL.\n\n",
        f"export const LAP_LEN: i32 = {int(total)}      // world pixels, one lap of the centre-line\n",
        f"export const CELLS: i32 = {CELLS}            // surface cells per axis\n",
        f"export const CELL_PX: i32 = {CELL}           // world pixels per surface cell\n",
        f"export const START_X: i32 = {int(sx)}\n",
        f"export const START_Z: i32 = {int(sz)}\n",
        f"export const START_YAW: i32 = {syaw}        // 1/256ths of a turn, +z toward +x\n\n",
        "// The surface under any point: 4 bits per cell, 8 cells per word. Class values are the\n",
        "// SURF_* constants in packages/kart.tish.\n",
        tish_array("SURF", pack_surface(surf), per_line=8),
        "\n// The racing line, for AI steering.\n",
        tish_array("WPX", [int(p[0]) for p in waypoints]),
        tish_array("WPZ", [int(p[1]) for p in waypoints]),
        "\n// Ordered gates. A lap counts only when every one has been passed in sequence, which is\n"
        "// what stops a kart reversing back over the finish line to farm laps.\n",
        tish_array("CPX", [int(p[0]) for p in checks]),
        tish_array("CPZ", [int(p[1]) for p in checks]),
        "\n// Starting grid, two by two behind the line.\n",
        tish_array("GRIDX", [g[0] for g in grid]),
        tish_array("GRIDZ", [g[1] for g in grid]),
        "\n// Item boxes, in rows of three across the road.\n",
        tish_array("BOXX", [b[0] for b in boxes]),
        tish_array("BOXZ", [b[1] for b in boxes]),
    ]
    with open(os.path.join(EX_SRC, "track.tish"), "w") as f:
        f.write("".join(parts))
    return total, len(waypoints), len(checks), syaw, len(boxes)


def main():
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(EX_SRC, exist_ok=True)

    line, cum, total, img, surf, ts = build()

    # The sky.  Above-horizon scanlines have PA = PC = 0, so the whole scanline
    # reads texel (0,0); painting a 2x2 block there makes the sky exactly the
    # backdrop colour with no edge anywhere on the ground.
    img[0:2, 0:2] = SKY

    im = Image.fromarray(img, "RGB")
    im.save(os.path.join(OUT, "track.png"))

    n_tiles = unique_tiles(img)
    n_colours = len(im.getcolors(maxcolors=100000) or [])
    hist = {c: surf.count(c) for c in sorted(set(surf))}
    print(f"track.png  {WORLD}x{WORLD}  lap {total:.0f} world px")
    print(f"  unique 8x8 tiles {n_tiles}  (hard ceiling {TILE_BUDGET}) "
          f"{'OK' if n_tiles <= TILE_BUDGET else 'OVER — agb will silently paint tile 0'}")
    print(f"  colours {n_colours} (ceiling 240) {'OK' if n_colours <= 240 else 'OVER'}")
    print(f"  surface cells by class {hist}")

    sheets = make_kart_sheets()
    for name, (dims, ncol) in sheets.items():
        ok = "OK" if ncol <= 15 else "OVER — a 4bpp sprite bank holds 15 + transparent"
        print(f"{name:12s} {dims[0]}x{dims[1]}  {ncol} colours  {ok}")

    lap, nwp, ncp, syaw, nbox = emit_track_tish(line, cum, total, surf)
    print(f"track.tish  lap {lap:.0f}px  {nwp} waypoints  {ncp} checkpoints  {nbox} item boxes  "
          f"start yaw {syaw}/256")


if __name__ == "__main__":
    main()
