#!/usr/bin/env python3
"""Emit a key schedule that plays a Magical Drop board, for `scripts/screenshot.sh`.

WHY THIS EXISTS. Every example ships in attract mode: the native CPU plays until somebody touches
the pad. That makes the demo deterministic, and it also means the code a *player* runs — the pad
read, the hold-to-repeat, the A/B mapping — is never executed unless a test presses buttons. A
schedule is the only way to press them headlessly.

WHAT IT PLAYS. Sweep across all seven columns pressing A at each, then press B. Grabs are
colour-locked, so A on a mismatched column is a no-op and the sweep collects exactly the columns
whose bottom balloon matches the first one taken. Throwing that stack into one column drops three
or more of a colour together whenever the sweep found three, which is a guaranteed clear — no
knowledge of the board required. Repeating the sweep makes it near-certain.

FORMAT. `frame:keys` entries, comma separated, keys held from that frame until the next entry.
Every press needs an explicit release: "300:right,320:right" presses right ONCE, because the key
was already down at 320.

Do not build these in a shell. In zsh `$f:up` is a parameter modifier (`:u` = uppercase) and
silently produces garbage like `492p` instead of `492:up`.

Usage:  scripts/input_schedule.py [start_frame] [sweeps]
"""
from __future__ import annotations

import sys

STEP = 8       # frames between inputs; comfortably longer than the pierrot's 4-frame walk
SWEEP_COLS = 7


def build(start: int = 240, sweeps: int = 6) -> str:
    entries: list[str] = []
    f = start

    def press(key: str) -> None:
        nonlocal f
        entries.append(f"{f}:{key}")
        f += STEP
        entries.append(f"{f}:")          # explicit release — the schedule holds until told otherwise
        f += STEP

    for _ in range(sweeps):
        # Walk the field pressing A at every column. Colour-locking does the filtering for us.
        for col in range(SWEEP_COLS):
            press("a")
            if col + 1 < SWEEP_COLS:
                press("right")
        press("b")                        # dump whatever was collected
        # Walk back to the left edge for the next sweep.
        for _ in range(SWEEP_COLS - 1):
            press("left")
    return ",".join(entries)


if __name__ == "__main__":
    start = int(sys.argv[1]) if len(sys.argv) > 1 else 240
    sweeps = int(sys.argv[2]) if len(sys.argv) > 2 else 6
    print(build(start, sweeps))
