#!/usr/bin/env python3
"""Art for the PINBALL example.

Emits, into `examples/pinball/assets/`:

  ball.png     sheet8:  1 frame  — the steel ball
  flipper.png  sheet32: 5 frames — a flipper swept from rest to raised, pivot at the LEFT end

WHY THIS IS DRAWN AND NOT TAKEN FROM THE PACK. `scripts/gen_golf_art.py` settled the rule already,
for a golf ball: the Ninja Adventure pack has nothing that reads as a steel sphere at 8px, and a
seed or a nut at that size is a brown smudge — worse than three drawn circles. The same holds for a
flipper, which is not a thing any tile pack contains. Everything else in this example (the table
itself) is per-pixel terrain drawn at runtime, so these two files are the whole art budget.

FRAME ORDER IS THE CONTRACT with `src/main.tish`: flipper frame 0 is fully DOWN (rest) and frame 4
is fully UP, and the tish side indexes them from the same 0..FLIP_FRAMES-1 sweep it uses for the
collision segment's angle. The right flipper is the same sheet drawn with `sprite_set_flip`, which
is why the pivot sits at the left end of the cell rather than in the middle.

Run from the repo root:  python3 scripts/gen_pinball.py
"""
import math
import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "examples/pinball/assets"

BALL_CEL = 8
FLIP_CEL = 32
FLIP_FRAMES = 5

# The flipper's shape, in cell pixels. The pivot is the point the tish side rotates about, and it is
# duplicated there as FLIP_PIVOT_X/Y — keep the two in step.
PIVOT = (5, 16)
LENGTH = 21
W_BASE = 7          # width at the pivot
W_TIP = 3           # width at the tip — a real flipper tapers, and the taper is what makes the
                    # sprite read as a flipper rather than as a stick

# Rest is pointing down-and-out, raised is up-and-out. The sweep between them is what the ball
# gets hit by.
ANGLE_REST = 30.0
ANGLE_UP = -32.0

STEEL_HI = (232, 236, 244, 255)
STEEL_MID = (150, 158, 176, 255)
STEEL_LO = (72, 78, 96, 255)
FLIP_BODY = (232, 96, 72, 255)
FLIP_EDGE = (120, 36, 32, 255)
FLIP_HI = (255, 176, 150, 255)


def make_ball():
    im = Image.new("RGBA", (BALL_CEL, BALL_CEL), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.ellipse([0, 0, BALL_CEL - 1, BALL_CEL - 1], fill=STEEL_MID, outline=STEEL_LO)
    # A specular high on the upper left is the whole reason this reads as a sphere and not a disc.
    d.point((2, 2), fill=STEEL_HI)
    d.point((3, 2), fill=STEEL_HI)
    d.point((2, 3), fill=STEEL_HI)
    return im


def make_flipper_frame(t):
    """t in 0..1, 0 = rest (down), 1 = raised."""
    im = Image.new("RGBA", (FLIP_CEL, FLIP_CEL), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    ang = math.radians(ANGLE_REST + (ANGLE_UP - ANGLE_REST) * t)
    px, py = PIVOT
    tx = px + LENGTH * math.cos(ang)
    ty = py + LENGTH * math.sin(ang)
    # Drawn as a run of shrinking circles along the pivot->tip line: that gives the taper AND the
    # round cap at both ends for free, and it is exactly the shape the tish side collides against
    # (a capsule: distance from the ball's centre to the segment).
    steps = LENGTH * 2
    for i in range(steps + 1):
        f = i / steps
        cx = px + (tx - px) * f
        cy = py + (ty - py) * f
        r = (W_BASE + (W_TIP - W_BASE) * f) / 2.0
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=FLIP_BODY, outline=FLIP_EDGE)
    # A highlight down the upper edge, offset one pixel against the normal.
    nx, ny = -math.sin(ang), math.cos(ang)
    for i in range(steps + 1):
        f = i / steps
        cx = px + (tx - px) * f - nx * 1.5
        cy = py + (ty - py) * f - ny * 1.5
        if 0 <= cx < FLIP_CEL and 0 <= cy < FLIP_CEL:
            d.point((cx, cy), fill=FLIP_HI)
    return im


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    make_ball().save(OUT / "ball.png")
    print(f"  ball.png     1 frame  {BALL_CEL}x{BALL_CEL}")

    strip = Image.new("RGBA", (FLIP_CEL * FLIP_FRAMES, FLIP_CEL), (0, 0, 0, 0))
    for i in range(FLIP_FRAMES):
        strip.paste(make_flipper_frame(i / (FLIP_FRAMES - 1)), (i * FLIP_CEL, 0))
    strip.save(OUT / "flipper.png")
    n = len({(r, g, b) for (r, g, b, a) in strip.getdata() if a > 0})
    print(f"  flipper.png  {FLIP_FRAMES} frames {strip.width}x{strip.height}  {n} colours")


if __name__ == "__main__":
    print("pinball art")
    main()
