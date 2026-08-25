#!/usr/bin/env python3
"""Turn per-frame `MV` trace rows into the trajectory JSON `oam_assert.py` judges.

WHY WORLD SPACE AND NOT OAM. `scripts/oam_parse.py` reads what the hardware DREW, which is the only
way to catch a wrong sprite cell or an actor that is silently invisible. But OAM holds SCREEN
coordinates, and the camera moves — a pan makes every object on screen appear to teleport in the
same frame. Measured: a world-static Link slid from OAM x 104 to 52 while he had not moved at all.
So motion is judged here, on `entity_x`/`entity_y` rows, and drawing is judged on OAM.

The row format matches the `MOVE_TRACE` convention already used by the topdown RPG port
(in its components module): a flag the suite flips to 1 for a smoke build, printing engine state
once per frame. The ROM chooses to print; every judgement is made out here.

    MV <x> <y> <facing> <alive> <stunned>

⚠️ TRACE ROWS CAN DROP. The GBA log path is lossy under load and the topdown RPG port's own suite documents
losing roughly one row in three. Rows are therefore numbered by ARRIVAL, not assumed to be
consecutive frames, and `--require-contiguous` is available when a suite genuinely needs to know
none were lost. Averaging across a gap would silently turn one 16 px jump into two 8 px steps —
which is the exact opposite of what these laws are for.

Usage
  trace_to_traj.py <logfile> [--json] [--tag MV] [--require-contiguous]
  trace_to_traj.py run.log --json | oam_assert.py - --max-step 1 --turns-on-grid 16
"""
from __future__ import annotations

import argparse
import json
import re
import sys

# `[frame 123] MV 1856 1200 3 1 0`  — the emulator's frame prefix is optional but preferred,
# because it is the only way to know a row was dropped.
# ⚠️⚠️ COORDINATES ARE FRACTIONAL. `entity_x`/`entity_y` return the Fixed position as a float, so a
# walker moving 0.5 px/frame logs `1706.5`. An integer-only pattern here does NOT fail loudly — it
# silently skips those rows, and it skips them SELECTIVELY: the rows that survive are the ones where
# the actor happened to land on a whole pixel, i.e. exactly the tile-aligned ones. Measured: 482 rows
# emitted, 151 parsed, and the surviving sample was biased toward the alignment that
# `--turns-on-grid` is supposed to be testing. The suite reported PASS off that filtered sample.
NUM = r"-?\d+(?:\.\d+)?"
ROW = re.compile(rf"(?:\[frame\s+(\d+)\]\s*)?\b(MV)\s+({NUM})\s+({NUM})"
                 rf"(?:\s+({NUM}))?(?:\s+({NUM}))?(?:\s+({NUM}))?")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("logfile")
    ap.add_argument("--tag", default="MV", help="row tag to collect (default MV)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--require-contiguous", action="store_true",
                    help="exit non-zero if the emulator frame numbers are not consecutive")
    args = ap.parse_args()

    raw = open(args.logfile, "rb").read().decode("utf-8", "replace")
    out: list[dict] = []
    for line in raw.splitlines():
        m = ROW.search(line)
        if not m or m.group(2) != args.tag:
            continue
        frame, x, y = m.group(1), float(m.group(3)), float(m.group(4))
        rec = {
            "frame": int(frame) if frame is not None else len(out),
            "x": x, "y": y,
            # oam_assert speaks this shape; a world row has no cell of its own, and saying so with
            # a constant is honest — `--tile-changes` against it would be a lie, not a pass.
            "tile": 0,
            "disabled": False,
        }
        if m.group(5) is not None:
            rec["facing"] = int(float(m.group(5)))
        if m.group(6) is not None:
            rec["alive"] = int(float(m.group(6)))
        if m.group(7) is not None:
            rec["stunned"] = int(float(m.group(7)))
        out.append(rec)

    emitted = raw.count(f" {args.tag} ")
    if out and emitted > len(out):
        print(f"trace_to_traj: parsed {len(out)} of {emitted} `{args.tag}` rows "
              f"({emitted - len(out)} unparsed) — the pattern is dropping rows", file=sys.stderr)

    if not out:
        print(f"trace_to_traj: no `{args.tag}` rows in {args.logfile} — "
              f"was MOVE_TRACE flipped on for this build?", file=sys.stderr)
        return 1

    if args.require_contiguous:
        gaps = [(a["frame"], b["frame"]) for a, b in zip(out, out[1:])
                if b["frame"] - a["frame"] != 1]
        if gaps:
            print(f"trace_to_traj: {len(gaps)} dropped-row gap(s), first {gaps[0]}", file=sys.stderr)
            return 1

    if args.json:
        print(json.dumps(out, indent=1))
    else:
        print(f"{len(out)} rows, frames {out[0]['frame']}..{out[-1]['frame']}")
        for r in out[:5]:
            print(" ", r)
    return 0


if __name__ == "__main__":
    sys.exit(main())
