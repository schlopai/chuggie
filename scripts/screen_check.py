#!/usr/bin/env python3
"""Compare a captured frame against a committed reference — a visual regression check.

WHY THIS EXISTS. Every other assertion in these examples reads the log: did a chain happen, did a
stage advance, did the ROM make sound. None of them can see the screen, and three real bugs in
this genre were found only by cropping a screenshot and looking at it:

  * the board, aim line, HUD and meter all drawn TWICE per frame, the duplicate HUD at different
    coordinates, corrupting the panel;
  * glyph caps shaved off a HUD line, because `ui_clear_rect` snaps to 8px tile rows and the
    line above reached into the row below;
  * a pierrot completely hidden behind the balloons he was carrying.

All five verifiers passed through every one of them. A reference image would have caught all three
on the frame they appeared.

DETERMINISM. These ROMs are deterministic — fixed seeds, no clock, a deterministic self-play CPU —
so a capture at a given frame with a given key schedule is byte-identical run to run (verified:
three captures, one md5). That is what makes an exact-ish comparison honest rather than flaky.

The threshold is a hair above zero rather than zero so a stray pixel from a different PNG encoder
does not fail a build, while any real layout change does. It is deliberately tight: shifting one
HUD line three pixels sideways moves 0.30% of the screen, so a "sensible-looking" 0.5% tolerance
accepts exactly the class of bug this is here to catch.

    scripts/screen_check.py actual.png reference.png [--max-diff PERCENT]
    scripts/screen_check.py actual.png reference.png --update   # bless a new reference

Exit status is non-zero when the frames differ by more than the threshold, or when the reference
does not exist and --update was not passed.
"""
from __future__ import annotations

import argparse
import shutil
import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    sys.exit("screen_check: needs Pillow — pip install Pillow")

# Tight on purpose. The captures are byte-identical run to run, so the only reason to allow ANY
# difference is a stray pixel from a different PNG encoder on another machine. A real layout
# regression is far larger: nudging one HUD line three pixels sideways moved 0.30% of the screen,
# which a 0.5% threshold happily accepted.
DEFAULT_MAX_DIFF = 0.02  # percent of pixels (~8 of 38400)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("actual")
    ap.add_argument("reference")
    ap.add_argument("--max-diff", type=float, default=DEFAULT_MAX_DIFF)
    ap.add_argument("--update", action="store_true", help="overwrite the reference with the capture")
    args = ap.parse_args()

    actual, reference = Path(args.actual), Path(args.reference)
    if not actual.exists():
        print(f"  no capture at {actual}")
        return 1

    if args.update or not reference.exists():
        reference.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(actual, reference)
        verb = "updated" if args.update else "created"
        print(f"  {verb} reference {reference.name} — review it before committing")
        return 0 if args.update else 1

    a = Image.open(actual).convert("RGB")
    b = Image.open(reference).convert("RGB")
    if a.size != b.size:
        print(f"  FAIL: capture is {a.size}, reference is {b.size}")
        return 1

    pa, pb = a.load(), b.load()
    differing = 0
    first: tuple[int, int] | None = None
    for y in range(a.height):
        for x in range(a.width):
            if pa[x, y] != pb[x, y]:
                differing += 1
                if first is None:
                    first = (x, y)
    total = a.width * a.height
    pct = 100.0 * differing / total

    if pct > args.max_diff:
        print(f"  FAIL: {differing} of {total} pixels differ ({pct:.2f}% > {args.max_diff}%), "
              f"first at {first}")
        print(f"        if the change is intended: scripts/screen_check.py "
              f"{actual} {reference} --update")
        return 1
    print(f"  matches reference ({pct:.2f}% of pixels differ)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
