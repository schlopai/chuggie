#!/usr/bin/env python3
"""Count DISTINCT COLOURS in a screenshot — the literal test of the four-colour rule.

    python3 scripts/count_screen_colours.py shot.png [--max 4] [--list]

⚠️ THIS COUNTS WHAT THE SCREEN SHOWS, NOT WHAT THE ATLAS HOLDS, and that distinction is the whole
design. A tileset may carry a dozen colours as long as no single frame displays more than four —
which is exactly what "only 4 colours on screen at a time" means, and is achievable on this hardware
because a hidden background layer contributes nothing.

Sprites are included: the player is on the screen, so the player counts.
"""
import sys
from collections import Counter
from PIL import Image


def count(path, ignore_below=0):
    im = Image.open(path).convert("RGB")
    c = Counter(im.getdata())
    # Stray colours under a pixel threshold are emulator/scaling artefacts, not palette entries.
    return Counter({k: v for k, v in c.items() if v > ignore_below})


def main():
    args = [a for a in sys.argv[1:]]
    mx = 4
    show = "--list" in args
    if "--max" in args:
        mx = int(args[args.index("--max") + 1])
    paths = [a for a in args if not a.startswith("--") and not a.isdigit()]
    bad = 0
    for p in paths:
        c = count(p, ignore_below=8)
        n = len(c)
        flag = "ok  " if n <= mx else "OVER"
        if n > mx:
            bad += 1
        print(f"  {flag} {p}: {n} colours (max {mx})")
        if show or n > mx:
            for col, cnt in c.most_common():
                print(f"        #{col[0]:02x}{col[1]:02x}{col[2]:02x}  {cnt} px")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
