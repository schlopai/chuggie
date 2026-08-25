#!/usr/bin/env python3
"""Generate the sprites for examples/pong-link.

Run from the repo root:  python3 scripts/gen_pong_link.py

Emits examples/pong-link/assets/pong.png — a 16px `sheet:` strip:

    frame 0   ball            a 6x6 block, centred in its cell
    frame 1   paddle segment  a 6x16 bar; a paddle is two of them stacked

ONE sheet and one palette for everything that moves, which keeps this to a
single sprite bank and makes the whole game five objects: two paddles of two
segments each, and a ball.
"""

import os

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "examples", "pong-link", "assets")

C_BALL = (244, 246, 252)
C_BALL_DIM = (176, 182, 208)
C_PADDLE = (232, 236, 248)
C_PADDLE_DIM = (150, 158, 190)


def main():
    os.makedirs(OUT, exist_ok=True)
    im = Image.new("RGBA", (32, 16), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)

    # Ball: a block with one darker edge, so spin direction is readable at speed.
    d.rectangle([5, 5, 10, 10], fill=C_BALL)
    d.rectangle([5, 9, 10, 10], fill=C_BALL_DIM)

    # Paddle segment: full height of the cell so two stack seamlessly.
    d.rectangle([16 + 5, 0, 16 + 10, 15], fill=C_PADDLE)
    d.rectangle([16 + 9, 0, 16 + 10, 15], fill=C_PADDLE_DIM)

    im.save(os.path.join(OUT, "pong.png"))
    cols = {c for _, c in (im.getcolors(maxcolors=1 << 16) or []) if c[3] > 0}
    print(f"pong.png  {im.size[0]}x{im.size[1]}  2 frames  {len(cols)} colours")


if __name__ == "__main__":
    main()
