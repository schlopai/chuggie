#!/usr/bin/env python3
"""Screenshot a ROM and REFUSE to report pixel metrics if it is not actually running.

A GBA panic renders a crash page: a couple of flat colours and a QR code. Every pixel of it is
"not the backdrop", so any metric of the form "count pixels that should not be there" scores a
crashed ROM as maximally broken and, worse, scores it IDENTICALLY however the code changes. That
cost a whole debugging session: three conclusions in a row were drawn from a constant that was
measuring a crash screen, not a floor.

So liveness is checked first and separately, and it fails loudly.
"""
import subprocess, sys, os
from PIL import Image
import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def shot(rom, out, frame, keys=""):
    subprocess.run([os.path.join(ROOT, "scripts", "screenshot.sh"), rom, out, str(frame), keys],
                   check=True, capture_output=True)
    return np.array(Image.open(out).convert("RGB")).astype(int)


def assert_live(a, where=""):
    """A running game has variety; a crash page has almost none."""
    n = len(np.unique(a.reshape(-1, 3), axis=0))
    if n <= 4:
        raise SystemExit(
            f"DEAD ROM{where}: only {n} distinct colours — this is a crash page, not a frame.\n"
            f"Re-run with GBA_SHOT_LOG=1 and read the panic before trusting any pixel number."
        )
    return n


if __name__ == "__main__":
    rom, frame = sys.argv[1], int(sys.argv[2])
    keys = sys.argv[3] if len(sys.argv) > 3 else ""
    a = shot(rom, "/tmp/shot_check.png", frame, keys)
    print(f"live: {assert_live(a)} distinct colours")
