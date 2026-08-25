#!/usr/bin/env python3
"""Assert physical laws about an object's trajectory through hardware OAM.

WHY THIS EXISTS. `scripts/oam_parse.py` turns emulator OAM dumps into what the GBA actually drew.
This script is the half that judges it. It deliberately asserts LAWS rather than expected values:
"never moved more than N px in a frame", "only changed direction on a 16 px boundary", "moved at
all" — because a test pinned to literal constants from a previous build is a test that has to be
rewritten every time the build churns, and one that gets "fixed" by pasting in whatever the broken
build produced. A law can only be satisfied by behaving correctly.

⚠️ NEVER ASSERT AN ABSOLUTE PALETTE BANK OR A TILE INDEX COPIED FROM A PREVIOUS BUILD. tish and agb
emit in a nondeterministic order and agb's palette-bank assignment varies build to build. Tile
indices are stable WITHIN one built ROM, so `--tile-range` is for bounding an actor to its own
strip, and `--tile-changes` asserts that animation happens at all — neither pins a literal cell
from a different build.

⚠️ A FROZEN OBJECT PASSES EVERY "NEVER MOVED TOO FAR" CHECK. `--min-travel` is not optional
decoration; without it, a completely broken actor that never moves is maximally law-abiding. The
same applies to a hidden one: `--require-visible` exists because a disabled object keeps its last
coordinates and tile, so it can look perfectly correct while drawing nothing.

Input is the JSON emitted by `oam_parse.py --track SLOT --json` (a list of per-frame records with
`frame`, `x`, `y`, `tile`, `disabled`, ...), read from a file or stdin.

Usage
  oam_parse.py <dumps...> --track 9 --json | oam_assert.py - --max-step 2 --min-travel 24
  oam_assert.py traj.json --label walker --max-step 2 --turns-on-grid 16 --min-travel 24

Every assertion prints `  ok   <what>` or `  FAIL <what>: <evidence>`; exit status is 1 if any
failed, so this gates in a verify.sh rather than merely reporting.
"""
from __future__ import annotations

import argparse
import json
import sys


def load(path: str) -> list[dict]:
    raw = sys.stdin.read() if path == "-" else open(path).read()
    data = json.loads(raw)
    if not isinstance(data, list) or not data:
        sys.exit("oam_assert: expected a non-empty JSON list from `oam_parse.py --track ... --json`")
    return data


class Checks:
    def __init__(self, label: str):
        self.label = label
        self.failed = 0
        self.vacuous_n = 0

    def ok(self, what: str) -> None:
        print(f"  ok   {self.label}: {what}")

    def fail(self, what: str, evidence: str) -> None:
        print(f"  FAIL {self.label}: {what}: {evidence}")
        self.failed += 1

    def check(self, cond: bool, what: str, evidence: str = "") -> None:
        self.ok(what) if cond else self.fail(what, evidence)

    def vacuous(self, what: str, why: str) -> None:
        """A law that held only because nothing happened. NOT a pass.

        ⚠️ This exists because of a real miss. A capture window landed entirely inside one of the
        shipping hopper's ~130-frame idle stretches, so `never moves more than 2 px in a frame`
        reported ok — while the actor's real behaviour, a 16 px jump, sat just outside the window.
        Reporting that as a green is precisely the failure mode this whole harness was built to
        end, so a motion law with no motion to judge says so out loud."""
        print(f"  vac  {self.label}: {what}  (NOT PROVEN: {why})")
        self.vacuous_n += 1


def steps(traj: list[dict]) -> list[tuple[dict, dict, float, float]]:
    """Consecutive pairs with their dx/dy."""
    out = []
    for a, b in zip(traj, traj[1:]):
        out.append((a, b, b["x"] - a["x"], b["y"] - a["y"]))
    return out


def gap_of(a: dict, b: dict) -> int:
    """Frames between two samples, at least 1.

    ⚠️ A LOSSY LOG MUST NOT BE JUDGED AS IF IT WERE COMPLETE. The GBA log path drops rows —
    measured ~17% here, and the topdown RPG port's own suite documents losing about one in three. If a row is
    missing, the surviving pair spans two frames, and a per-frame law applied to it either
    (a) fails a correct actor, because 2 frames of legal movement look like one illegal jump, or
    (b) passes a broken one, because a real 16 px lurch gets averaged across the gap into
    innocent-looking steps. Both have happened in this suite. So per-frame budgets are multiplied
    by the actual gap, and the gap comes from the emulator's frame number, never assumed to be 1.
    """
    fa, fb = a.get("frame"), b.get("frame")
    if fa is None or fb is None:
        return 1
    return max(1, int(fb) - int(fa))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("trajectory", help="JSON from `oam_parse.py --track N --json`, or - for stdin")
    ap.add_argument("--label", default="object")
    ap.add_argument("--contiguous", action="store_true",
                    help="require the dumps to be consecutive frames (a gap makes every "
                         "per-frame law meaningless, so this is checked, not assumed)")
    ap.add_argument("--require-visible", action="store_true",
                    help="the object must be on-screen and enabled in EVERY sampled frame")
    ap.add_argument("--max-step", type=int, metavar="PX",
                    help="per-frame displacement on either axis may never exceed PX")
    ap.add_argument("--min-travel", type=int, metavar="PX",
                    help="total path length over the window must be at least PX (catches frozen)")
    ap.add_argument("--max-still", type=int, metavar="FRAMES",
                    help="the object may not stay motionless for more than FRAMES consecutively")
    ap.add_argument("--turns-on-grid", type=int, metavar="G",
                    help="a change of movement direction may only happen when the moving axis "
                         "is aligned to a G px boundary")
    ap.add_argument("--grid-phase", type=int, default=0, metavar="PX",
                    help="offset of the grid in OAM space. ⚠️ REQUIRED WHENEVER THE ACTOR HAS A "
                         "SPRITE OFFSET. OAM holds SCREEN coordinates with the entity's sprite "
                         "offset already applied, but tile alignment is a property of the WORLD "
                         "position. An actor drawn at set_sprite_offset(-8,-8) sits at screen "
                         "coords congruent to 8 (mod 16) when it is perfectly tile-aligned, so "
                         "testing x %% 16 == 0 asks the wrong question and mislabels aligned turns "
                         "as off-grid (and vice versa).")
    ap.add_argument("--axis-locked", action="store_true",
                    help="never move on both axes in the same frame (a 4-direction walker)")
    ap.add_argument("--tile-range", metavar="LO,HI",
                    help="the object's tile index must stay within [LO,HI] (bound it to its own "
                         "sprite strip; do NOT paste a literal cell from another build)")
    ap.add_argument("--tile-changes", action="store_true",
                    help="the tile index must change at least once (the actor animates at all)")
    args = ap.parse_args()

    traj = load(args.trajectory)
    c = Checks(args.label)
    print(f"── {args.label}: {len(traj)} sampled frames "
          f"({traj[0].get('frame')}..{traj[-1].get('frame')})")

    if args.contiguous:
        gaps = [(a.get("frame"), b.get("frame")) for a, b in zip(traj, traj[1:])
                if a.get("frame") is not None and b.get("frame") is not None
                and b["frame"] - a["frame"] != 1]
        c.check(not gaps, "sampled frames are consecutive",
                f"{len(gaps)} gap(s), first {gaps[0] if gaps else ''}")

    if args.require_visible:
        hidden = [t.get("frame") for t in traj if t.get("disabled")]
        c.check(not hidden, "drawn in every sampled frame",
                f"disabled on {len(hidden)} frame(s), first {hidden[0] if hidden else ''}")

    st = steps(traj)
    # Total path length decides whether the motion laws can say anything at all.
    travel = sum(abs(dx) + abs(dy) for _a, _b, dx, dy in st)
    moving_steps = sum(1 for _a, _b, dx, dy in st if dx or dy)
    if travel == 0:
        why = f"the object never moved across {len(traj)} sampled frames"
    elif moving_steps < 4:
        why = f"only {moving_steps} frame(s) of motion in the window"
    else:
        why = None

    if args.max_step is not None and why:
        c.vacuous(f"never moves more than {args.max_step} px in a frame", why)
    elif args.max_step is not None:
        bad = [(a.get("frame"), b.get("frame"), dx, dy) for a, b, dx, dy in st
               if abs(dx) > args.max_step * gap_of(a, b)
               or abs(dy) > args.max_step * gap_of(a, b)]
        c.check(not bad, f"never moves more than {args.max_step} px in a frame",
                f"{len(bad)} violation(s), first frame {bad[0][1] if bad else ''} "
                f"moved ({bad[0][2]},{bad[0][3]})" if bad else "")

    if args.min_travel is not None:
        total = sum(abs(dx) + abs(dy) for _a, _b, dx, dy in st)
        c.check(total >= args.min_travel,
                f"travels at least {args.min_travel} px overall", f"only {total} px")

    if args.max_still is not None:
        run = worst = 0
        worst_at = None
        for a, b, dx, dy in st:
            if dx == 0 and dy == 0:
                # count FRAMES held still, not samples — with rows dropping, a run of 3 samples
                # can span 9 frames, and counting samples under-reports a freeze threefold.
                run += gap_of(a, b)
                if run > worst:
                    worst, worst_at = run, b.get("frame")
            else:
                run = 0
        c.check(worst <= args.max_still,
                f"never motionless for more than {args.max_still} frames",
                f"{worst} consecutive still frames ending at frame {worst_at}")

    if args.axis_locked and why:
        c.vacuous("moves on one axis at a time", why)
    elif args.axis_locked:
        both = [b.get("frame") for _a, b, dx, dy in st if dx != 0 and dy != 0]
        c.check(not both, "moves on one axis at a time",
                f"{len(both)} diagonal frame(s), first {both[0] if both else ''}")

    if args.turns_on_grid and why:
        c.vacuous(f"only changes direction on a {args.turns_on_grid} px boundary", why)
    elif args.turns_on_grid:
        g = args.turns_on_grid
        prev_dir = None
        bad = []
        for _a, b, dx, dy in st:
            if dx == 0 and dy == 0:
                continue
            d = (0 if dx == 0 else (1 if dx > 0 else -1),
                 0 if dy == 0 else (1 if dy > 0 else -1))
            if prev_dir is not None and d != prev_dir:
                # the turn is legal only where the object sits on a grid boundary
                ph = args.grid_phase
                if (b["x"] - ph) % g != 0 or (b["y"] - ph) % g != 0:
                    bad.append((b.get("frame"), b["x"], b["y"], prev_dir, d))
            prev_dir = d
        c.check(not bad, f"only changes direction on a {g} px boundary"
                + (f" (phase {args.grid_phase})" if args.grid_phase else ""),
                f"{len(bad)} off-grid turn(s), first at frame {bad[0][0]} "
                f"pos ({bad[0][1]},{bad[0][2]})" if bad else "")

    if args.tile_range:
        lo, hi = (int(v) for v in args.tile_range.split(","))
        out = [(t.get("frame"), t["tile"]) for t in traj if not (lo <= t["tile"] <= hi)]
        c.check(not out, f"tile index stays within [{lo},{hi}]",
                f"{len(out)} frame(s) outside, first {out[0] if out else ''}")

    if args.tile_changes:
        tiles = {t["tile"] for t in traj}
        c.check(len(tiles) > 1, "animates (tile index changes)",
                f"tile fixed at {tiles.pop() if tiles else '?'} for the whole window")

    print()
    tail = f", {c.vacuous_n} unproven" if c.vacuous_n else ""
    if c.failed:
        print(f"oam_assert {args.label}: FAIL ({c.failed} law(s) violated{tail})")
        return 1
    if c.vacuous_n:
        # Nothing was violated, but nothing was proven either. Green here would mean "the window
        # was useless" and read as "the actor is correct".
        print(f"oam_assert {args.label}: INCONCLUSIVE ({c.vacuous_n} law(s) had no motion to judge)")
        return 1
    print(f"oam_assert {args.label}: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
