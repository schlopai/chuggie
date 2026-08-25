#!/usr/bin/env python3
"""Two assertions about `examples/ringside` that no other check can make.

── 1. EVERY TELL STAYS REACTABLE ─────────────────────────────────────────────────────────────────

A Punch-Out opponent is a set of readable wind-ups. The whole game is: see the tell, pick the right
defence, punish the whiff. Difficulty in this demo shortens tells and changes nothing else — which
is the right knob, and also a dangerous one, because there is a value below which a tell stops being
information and the game becomes memorisation with a random seed.

`boxing.tish` floors the scaled tell at `TELL_FLOOR`, but the floor only helps if it is itself above
human reaction time. At 60 fps, ~12 frames is 200 ms, which is a normal simple-reaction time; below
about 9 frames (150 ms) a player is guessing no matter how well they know the pattern.

Nothing else in the suite can see this. The ROM builds, paints, soaks and screenshots identically
whether the tells are 20 frames or 2. It would simply stop being a game, silently, in a way only
playing it would reveal — and a later balance tweak is exactly the kind of change nobody re-plays
three difficulties of.

── 2. THE SPRITE BUDGET FITS ─────────────────────────────────────────────────────────────────────

Sprite VRAM does not degrade, it PANICS: `SpriteFull`, thrown from inside agb, minutes into play, on
no particular frame. This example hit it for real — six 64x64 cells re-pointing on one frame held
both the old and the new tiles, because agb frees tile VRAM only at commit. The fix was structural
(halving two bands that were half empty, and staggering the idle animation), and the guard against
it coming back is a number the generator prints and this script asserts.
"""
import os
import re
import subprocess
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SRC = os.path.join(REPO, "examples", "ringside", "src")
PKG = os.path.join(REPO, "packages", "boxing.tish")

# 150 ms at 60 fps. Below this a tell is not information.
MIN_REACTABLE = 9
# Leaves ~22 KB of the 32 KB arena for agb's commit-time churn and the banner's text sprites.
MAX_VRAM = 24576


def read(p):
    with open(p) as f:
        return f.read()


def main():
    fails = 0

    pkg = read(PKG)
    diff_cut = [int(x) for x in re.search(r"let DIFF_CUT: i32\[\] = \[([^\]]+)\]", pkg).group(1).split(",")]
    rage_cut = [int(x) for x in re.search(r"let RAGE_CUT: i32\[\] = \[([^\]]+)\]", pkg).group(1).split(",")]
    floor = int(re.search(r"let TELL_FLOOR: i32 = (\d+)", pkg).group(1))

    if floor < MIN_REACTABLE:
        print("FAIL TELL_FLOOR is %d frames (%d ms); below %d a tell is not reactable"
              % (floor, floor * 1000 // 60, MIN_REACTABLE))
        fails += 1
    else:
        print("ok   TELL_FLOOR %d frames (%d ms)" % (floor, floor * 1000 // 60))

    # Every authored tell, put through the same arithmetic boxTellFrames uses.
    opp = read(os.path.join(SRC, "opponent.tish"))
    attacks = re.findall(r"boxDefAttack\((\w+),\s*(\d+),", opp)
    if not attacks:
        print("FAIL no boxDefAttack calls found in opponent.tish")
        return 1

    worst = 999
    for name, tell in attacks:
        tell = int(tell)
        for d in range(len(diff_cut)):
            for ph in range(len(rage_cut)):
                scaled = max(floor, tell - diff_cut[d] - rage_cut[ph])
                if scaled < worst:
                    worst = scaled
                if scaled < MIN_REACTABLE:
                    print("FAIL %s at difficulty %d phase %d is %d frames" % (name, d, ph, scaled))
                    fails += 1
    print("ok   %d attacks x %d difficulties x %d phases; shortest tell %d frames (%d ms)"
          % (len(attacks), len(diff_cut), len(rage_cut), worst, worst * 1000 // 60))

    # The generator is the authority on the budget: it knows the cell sizes and the sprite count.
    out = subprocess.run([sys.executable, os.path.join(REPO, "scripts", "gen_ringside.py")],
                         capture_output=True, text=True)
    m = re.search(r"VRAM_BUDGET_BYTES=(\d+)", out.stdout)
    if not m:
        print("FAIL gen_ringside.py did not report VRAM_BUDGET_BYTES")
        return 1
    vram = int(m.group(1))
    if vram > MAX_VRAM:
        print("FAIL live sprite VRAM %d B exceeds %d B; agb panics with SpriteFull, not a warning"
              % (vram, MAX_VRAM))
        fails += 1
    else:
        print("ok   live sprite VRAM %d B of 32768 (gate %d)" % (vram, MAX_VRAM))

    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
