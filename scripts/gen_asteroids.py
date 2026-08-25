#!/usr/bin/env python3
"""Generate the `asteroids` example's ROM art — all procedural, no external source pixels.

The look is "low-poly space sim": every shape is a real polygon, rotated in FLOAT coordinates and
then rasterised with hard edges (no anti-aliasing, so each frame lands on exact palette colours a
4bpp GBA sprite can hold). Rocks are flat-shaded facet by facet against a FIXED light direction —
so a tumbling rock relights as it turns, the way a faceted solid does, instead of spinning a painted
texture. That single detail is most of what sells the 3D-ish read on a 32px sprite.

  space16.png   sheet16 — 16 ship headings, the same 16 with the thrust plume lit, shots, a 4-frame
                debris burst, a blinking saucer, and the medium + small rocks (8 rotations each).
  rocks32.png   sheet32 — the large rocks: two silhouettes × 8 rotations.
  starfield.png 256×256 tileable deep-space backdrop (opaque; wraps in both axes).

Rotation frames are CONTIGUOUS per shape, which is what lets a rock tumble on the engine's native
`anim_play` loop and cost the game no per-frame work at all.

Sprite sheets keep ≤15 non-transparent colours (one 16-colour palette bank); the backdrop is opaque.
Re-run after editing; output → examples/asteroids/assets.
"""
import math
import os
import random

from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(__file__), "..", "examples", "asteroids", "assets")
os.makedirs(OUT, exist_ok=True)

T = (0, 0, 0, 0)

# ── Sheet16 palette: 14 opaque colours + transparent. ──
WHITE = (238, 246, 255, 255)
CYAN = (128, 224, 244, 255)
BLUE = (58, 134, 208, 255)
NAVY = (30, 58, 112, 255)
YEL = (252, 226, 104, 255)
ORA = (246, 152, 52, 255)
RED = (222, 74, 60, 255)
GRN = (112, 216, 142, 255)
TEAL = (44, 146, 148, 255)
# Rock ramp, brightest → darkest. Four tones is what makes a facet read as a facet.
R0 = (196, 206, 224, 255)
R1 = (150, 160, 182, 255)
R2 = (104, 114, 138, 255)
R3 = (64, 72, 96, 255)
R4 = (38, 44, 62, 255)
ROCK_RAMP = (R0, R1, R2, R3)

# ── Sheet32 palette: the same grey ramp plus a warm ramp, so the two large silhouettes
#    are told apart by COLOUR as well as by outline at a glance. ──
B0 = (206, 176, 138, 255)
B1 = (166, 134, 100, 255)
B2 = (118, 92, 68, 255)
B3 = (74, 56, 44, 255)
B4 = (44, 32, 26, 255)
BROWN_RAMP = (B0, B1, B2, B3)

# Light direction (screen space, y down): from the upper left. Fixed in the WORLD, which is why a
# rotating rock is shaded from its rotated geometry rather than from a pre-painted highlight.
LIGHT = (-0.55, -0.84)


def cell(size):
    img = Image.new("RGBA", (size, size), T)
    return img, ImageDraw.Draw(img)


def rot(pts, turns):
    """Rotate local-space points by `turns` (1.0 = full circle), clockwise on screen."""
    a = turns * 2.0 * math.pi
    ca, sa = math.cos(a), math.sin(a)
    return [(x * ca - y * sa, x * sa + y * ca) for (x, y) in pts]


def at(pts, cx, cy):
    return [(cx + x, cy + y) for (x, y) in pts]


# ─────────────────────────────────────────────────────────────────────────────
# The ship — 16 headings, drawn as a rotated polygon rather than 16 hand-placed sprites
# ─────────────────────────────────────────────────────────────────────────────

# Local space: nose toward -y (up) at heading 0, matching the game's heading 0 = up. The classic
# notched dart — wingtips behind, tail indented — so the thing reads as a SHIP even at 13px.
SHIP_NOSE = (0.0, -6.9)
SHIP_R = (4.6, 5.9)
SHIP_TR = (1.7, 3.5)
SHIP_TL = (-1.7, 3.5)
SHIP_L = (-4.6, 5.9)
FLAME_A = [(-2.3, 4.2), (2.3, 4.2), (0.0, 10.4)]
FLAME_B = [(-1.2, 4.4), (1.2, 4.4), (0.0, 7.9)]


def ship(heading, steps, thrust):
    """One ship frame at heading index `heading` of `steps`. `thrust` lights the engine plume.

    The hull is faceted like the rocks, but it also carries a bright CYAN rim. Without the rim the
    shadow facet is close enough to empty space that half the ship disappears at some headings, and
    the silhouette appears to change shape as it turns — the rim is what keeps all 16 frames reading
    as one object. It doubles as the vector-Asteroids outline, which is the look we're after anyway.
    """
    img, d = cell(16)
    turns = heading / steps
    cx = cy = 7.5

    if thrust:
        # Drawn first so the hull covers where the plume meets the tail.
        d.polygon(at(rot(FLAME_A, turns), cx, cy), fill=ORA)
        d.polygon(at(rot(FLAME_B, turns), cx, cy), fill=YEL)

    hull = rot([SHIP_NOSE, SHIP_R, SHIP_TR, SHIP_TL, SHIP_L], turns)
    nose, r, tr, tl, lf = hull
    # Two facets off the centre line. They rotate WITH the hull — unlike a rock, a ship is lit by
    # its own canopy and engine glow, so its shading is body-fixed, not world-fixed.
    d.polygon(at([nose, tl, lf], cx, cy), fill=BLUE)
    d.polygon(at([nose, r, tr], cx, cy), fill=NAVY)
    d.polygon(at(hull, cx, cy), outline=CYAN)
    (canopy,) = rot([(0.0, -2.4)], turns)
    d.point([(cx + canopy[0], cy + canopy[1])], fill=WHITE)
    (tip,) = rot([(0.0, -6.4)], turns)
    d.point([(cx + tip[0], cy + tip[1])], fill=WHITE)
    return img


# ─────────────────────────────────────────────────────────────────────────────
# Rocks — a jittered polygon, flat-shaded facet by facet under a fixed light
# ─────────────────────────────────────────────────────────────────────────────


def rock_shape(seed, verts, radius, jitter):
    """A closed irregular polygon in local space, plus a couple of crater positions."""
    rnd = random.Random(seed)
    pts = []
    for i in range(verts):
        a = 2.0 * math.pi * i / verts
        rr = radius * (1.0 - jitter + 2.0 * jitter * rnd.random())
        pts.append((math.cos(a) * rr, math.sin(a) * rr))
    craters = []
    for _ in range(2):
        a = rnd.random() * 2.0 * math.pi
        rr = radius * (0.15 + 0.35 * rnd.random())
        craters.append((math.cos(a) * rr, math.sin(a) * rr, max(1.0, radius * 0.16)))
    return pts, craters


def draw_rock(size, pts, craters, turns, ramp, dark):
    """Rasterise one rotation of a rock: base silhouette, then one flat-shaded facet per edge."""
    img, d = cell(size)
    cx = cy = (size - 1) / 2.0
    p = rot(pts, turns)
    d.polygon(at(p, cx, cy), fill=ramp[2])
    n = len(p)
    for i in range(n):
        a, b = p[i], p[(i + 1) % n]
        # The facet's outward normal, approximated by its midpoint direction from the centre. Taken
        # AFTER rotation, so the lit side stays put on screen while the rock turns under it.
        mx, my = (a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5
        m = math.hypot(mx, my) or 1.0
        lit = (mx / m) * LIGHT[0] + (my / m) * LIGHT[1]
        # Four buckets across the terminator — the quantisation IS the low-poly look.
        idx = 0 if lit > 0.62 else 1 if lit > 0.12 else 2 if lit > -0.45 else 3
        d.polygon(at([(0.0, 0.0), a, b], cx, cy), fill=ramp[idx])
    # Craters ride the surface, so their positions rotate with it (unlike the lighting above).
    for (lx, ly), (_, _, r) in zip(rot([(c[0], c[1]) for c in craters], turns), craters):
        d.ellipse([cx + lx - r, cy + ly - r, cx + lx + r, cy + ly + r], fill=dark)
    return img


def rock_frames(size, seed, verts, radius, jitter, steps, ramp, dark):
    pts, craters = rock_shape(seed, verts, radius, jitter)
    return [draw_rock(size, pts, craters, i / steps, ramp, dark) for i in range(steps)]


# ─────────────────────────────────────────────────────────────────────────────
# Shots, debris burst, saucer
# ─────────────────────────────────────────────────────────────────────────────


def player_shot():
    img, d = cell(16)
    d.rectangle([7, 6, 8, 9], fill=CYAN)
    d.rectangle([6, 7, 9, 8], fill=CYAN)
    d.rectangle([7, 7, 8, 8], fill=WHITE)
    return img


def saucer_shot():
    img, d = cell(16)
    d.rectangle([6, 6, 9, 9], fill=RED)
    d.rectangle([7, 7, 8, 8], fill=YEL)
    return img


def burst(step):
    """Asteroids blows apart into fragments, not a fireball: a ring of debris flung outward.

    Each fragment is a short RADIAL streak rather than a dot, which at 16px is the difference
    between "an explosion" and "some dust" — a single pixel at this scale is nearly invisible
    against a starfield that is itself made of single pixels.
    """
    img, d = cell(16)
    rnd = random.Random(11)
    cx = cy = 7.5
    r = 2.0 + step * 3.1
    tone = [WHITE, YEL, ORA, R2][step]
    hot = [WHITE, WHITE, YEL, R1][step]
    for i in range(12):
        a = 2.0 * math.pi * i / 12 + rnd.random() * 0.25
        rr = r * (0.72 + 0.42 * rnd.random())
        ca, sa = math.cos(a), math.sin(a)
        inner = max(0.0, rr - 1.6 - step * 0.4)
        d.line([cx + ca * inner, cy + sa * inner, cx + ca * rr, cy + sa * rr], fill=tone)
        d.point([(cx + ca * rr, cy + sa * rr)], fill=hot)
    if step == 0:
        d.ellipse([5, 5, 10, 10], fill=WHITE)
    elif step == 1:
        d.ellipse([6, 6, 9, 9], fill=YEL)
    return img


def saucer(lights):
    """The classic UFO: two stacked trapezoids. `lights` blinks the underside lamps."""
    img, d = cell(16)
    d.polygon([(1, 9), (15, 9), (11, 12), (5, 12)], fill=TEAL)
    d.polygon([(1, 9), (15, 9), (11, 7), (5, 7)], fill=GRN)
    d.polygon([(6, 7), (10, 7), (9, 4), (7, 4)], fill=CYAN)   # dome
    d.point([(8, 5)], fill=WHITE)
    d.point([(3, 10), (8, 11), (12, 10)], fill=YEL if lights else NAVY)
    return img


# ─────────────────────────────────────────────────────────────────────────────
# Sheet assembly
# ─────────────────────────────────────────────────────────────────────────────

HEADINGS = 16     # ship rotation frames — also the game's heading resolution (22.5° per step)
ROCK_SPINS = 8    # rotation frames per rock silhouette


def build_space16():
    frames = []
    frames += [ship(i, HEADINGS, False) for i in range(HEADINGS)]   # 0..15
    frames += [ship(i, HEADINGS, True) for i in range(HEADINGS)]    # 16..31
    frames.append(player_shot())                                    # 32
    frames.append(saucer_shot())                                    # 33
    frames += [burst(i) for i in range(4)]                          # 34..37
    frames += [saucer(False), saucer(True)]                         # 38,39
    frames += rock_frames(16, 31, 9, 6.6, 0.24, ROCK_SPINS, ROCK_RAMP, R4)   # 40..47 medium
    frames += rock_frames(16, 57, 8, 3.9, 0.26, ROCK_SPINS, ROCK_RAMP, R4)   # 48..55 small
    sheet = Image.new("RGBA", (16 * len(frames), 16), T)
    for i, f in enumerate(frames):
        sheet.paste(f, (i * 16, 0))
    _assert_palette(sheet, 16, "space16")
    sheet.save(os.path.join(OUT, "space16.png"))
    return {
        "SHIP": 0, "SHIP_THRUST": 16, "PSHOT": 32, "ESHOT": 33,
        "BOOM": 34, "SAUCER": 38, "ROCK_MED": 40, "ROCK_SMALL": 48,
    }


def build_rocks32():
    frames = []
    frames += rock_frames(32, 13, 11, 14.2, 0.22, ROCK_SPINS, ROCK_RAMP, R4)     # 0..7
    frames += rock_frames(32, 29, 10, 13.6, 0.30, ROCK_SPINS, BROWN_RAMP, B4)    # 8..15
    sheet = Image.new("RGBA", (32 * len(frames), 32), T)
    for i, f in enumerate(frames):
        sheet.paste(f, (i * 32, 0))
    _assert_palette(sheet, 16, "rocks32")
    sheet.save(os.path.join(OUT, "rocks32.png"))
    return {"ROCK_A": 0, "ROCK_B": 8}


# ─────────────────────────────────────────────────────────────────────────────
# Backdrop — a 256×256 that tiles in both axes (the GBA's hardware wrap)
# ─────────────────────────────────────────────────────────────────────────────


def _wrapped(draw_fn):
    """Run a draw op nine times, offset by ±256, so anything crossing an edge tiles seamlessly."""
    for ox in (-256, 0, 256):
        for oy in (-256, 0, 256):
            draw_fn(ox, oy)


# Bayer 4×4 — the ordered-dither threshold matrix.
BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]

# Nebula tones, thinnest → densest. Only three, and all close to the base colour: a backdrop that
# competes with the sprites in front of it is a worse backdrop, however pretty it is alone.
NEB = [(11, 13, 31), (16, 19, 42), (23, 27, 56)]


def _nebula(img):
    """Dither a smooth density field into three flat tones, in place.

    Flat SHAPES cannot make gas: whatever the outline (one big ellipse, or forty small ones), the
    eye locks onto the clean edge and reads planets or cartoon clouds. A dither has no edge at all —
    it trades the palette we don't have for spatial noise, which is exactly how pixel art has always
    faked a gradient. The field is evaluated with toroidal distance so the result still tiles.
    """
    px = img.load()
    rnd = random.Random(77)
    # Gaussian sources: (x, y, sigma, weight). Two loose drifts plus smaller knots on top.
    src = [(64, 80, 62, 1.0), (196, 180, 70, 1.0), (140, 40, 34, 0.5), (30, 200, 40, 0.6)]
    src += [(rnd.randrange(256), rnd.randrange(256), rnd.uniform(16, 30), rnd.uniform(0.2, 0.45))
            for _ in range(10)]
    for y in range(256):
        row = BAYER[y & 3]
        for x in range(256):
            f = 0.0
            for sx, sy, sig, wgt in src:
                dx = abs(x - sx)
                dx = min(dx, 256 - dx)          # toroidal, so the backdrop still wraps seamlessly
                dy = abs(y - sy)
                dy = min(dy, 256 - dy)
                f += wgt * math.exp(-(dx * dx + dy * dy) / (2.0 * sig * sig))
            # + the dither offset, then quantise. The offset is what breaks the contour lines that
            # a plain threshold would leave — those bands are the thing that looks cheap.
            v = f * 2.4 + (row[x & 3] / 16.0 - 0.5) * 0.62
            if v > 1.15:
                px[x, y] = NEB[2]
            elif v > 0.66:
                px[x, y] = NEB[1]
            elif v > 0.30:
                px[x, y] = NEB[0]


def build_starfield():
    rnd = random.Random(5)
    img = Image.new("RGB", (256, 256), (7, 9, 22))
    d = ImageDraw.Draw(img)

    _nebula(img)

    # A distant planet with a terminator — the one element that says "somewhere", not just "space".
    px, py, pr = 196, 96, 30
    _wrapped(lambda ox, oy: d.ellipse([px + ox - pr, py + oy - pr, px + ox + pr, py + oy + pr],
                                      fill=(44, 52, 88)))
    _wrapped(lambda ox, oy: d.chord([px + ox - pr, py + oy - pr, px + ox + pr, py + oy + pr],
                                    start=150, end=330, fill=(30, 36, 64)))
    _wrapped(lambda ox, oy: d.ellipse([px + ox - pr + 8, py + oy - pr + 6,
                                       px + ox - pr + 22, py + oy - pr + 16], fill=(62, 72, 112)))

    # Stars last, so they sit on top of the nebula and the planet's limb.
    for _ in range(320):
        x, y = rnd.randrange(256), rnd.randrange(256)
        c = rnd.choice([(58, 66, 100), (58, 66, 100), (92, 102, 140),
                        (150, 162, 200), (214, 222, 246), (240, 244, 255)])
        d.point([(x, y)], fill=c)
    for _ in range(12):   # a handful of brighter 2px stars
        x, y = rnd.randrange(254), rnd.randrange(254)
        d.rectangle([x, y, x + 1, y + 1], fill=(236, 242, 255))

    img = img.quantize(colors=15, method=Image.MEDIANCUT).convert("RGB")
    img.save(os.path.join(OUT, "starfield.png"))


def _assert_palette(img, limit, name):
    cols = {px for px in img.getdata() if px[3] != 0}
    assert len(cols) <= limit - 1, f"{name}: {len(cols)} opaque colours (>{limit - 1})"
    print(f"  {name}: {len(cols)} opaque colours, {img.width // img.height} frames")


if __name__ == "__main__":
    print("generating asteroids art →", os.path.normpath(OUT))
    print(" ", build_space16())
    print(" ", build_rocks32())
    build_starfield()
    print("done.")
