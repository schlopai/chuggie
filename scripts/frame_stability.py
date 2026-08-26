#!/usr/bin/env python3
"""Report whether each example's golden reference frame sits on an animation PLATEAU.

THE BUG THIS EXISTS FOR
-----------------------
`drop-puzzle` captured its reference at frame 560. Frame 560 and 561 differ by 596 pixels — the
balloons at x80-111 go from settled to mid-burst — so 560 was the last frame before a transition.

Any change that shifted the schedule by a single frame moved the capture onto the far side of that
edge and produced an identical 596-pixel "regression", repeatedly, with nothing actually wrong.
Adding one `ui_clear_rect` to a mode start was enough. The failure is indistinguishable from a real
one, and the obvious response — bless the new frame — makes it flip back next time.

A golden frame is only meaningful if the frames around it look the same. This finds the ones that
do not.

    python3 scripts/frame_stability.py                       # every example in THIS repo
    python3 scripts/frame_stability.py examples/feel-demo    # just one
    python3 scripts/frame_stability.py --find examples/feel-demo
    python3 .../chuggie-engine/scripts/frame_stability.py ../game/*/  # a game repo's own ROM dirs

An argument is a ROM directory (or its verify.sh, or a bare example name in this repo), so a game
repo that consumes this engine can point it at its own cartridges. With no arguments it sweeps this
repo's `examples/*`.

Exits non-zero if any reference frame is within one frame of a visible transition. Slow: it runs
the emulator twice per example.
"""

import re
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SHOT = ROOT / "scripts" / "screenshot.sh"
# Follow what the golden check actually READS, then find the capture that writes it. Matching on a
# filename convention instead ("*-screen.png") silently skipped two examples — the match-3 port compares
# the long run's own frame and pocket-modes uses "-vs.png". A checker that quietly covers less than
# it claims is the thing this whole file exists to stop.
SCREEN_CHECK = re.compile(r"screen_check\.py\s+(\S+)\s+reference\.png")


def capture_for(verify: Path):
    """(rom, frame) for the golden reference in this verify.sh, or None."""
    text = verify.read_text(encoding="utf-8")
    m = SCREEN_CHECK.search(text)
    if not m:
        return None
    target = re.escape(m.group(1))
    shot = re.search(rf"screenshot\.sh\s+(\S+\.gba)\s+{target}\s+(\d+)", text)
    if not shot:
        return None
    return shot.group(1), int(shot.group(2))


def shoot(rom: Path, n: int, dest: Path):
    """One capture from a FRESH BOOT.

    The .sav must go first, every time, and forgetting it made this script lie. These ROMs write
    32 KB of SRAM during a headless run, so capturing frame N and then N+1 without clearing it means
    the second run boots from the first run's save and plays a different game. That is not a
    hypothetical: `--find` reported a four-frame plateau at 2,048 for pocket-game while the sweep
    called the same frame unstable, and the sweep was right — from equal fresh boots, 2,048 and 2,049
    differ by 190 px. Every verdict this file produced about a save-writing ROM was measured against
    a state no verify.sh reproduces.

    Fresh boots are exactly reproducible: the same frame captured twice this way is 0 px apart.
    """
    (rom.parent / f"{rom.stem}.sav").unlink(missing_ok=True)
    subprocess.run([str(SHOT), rom.name, str(dest), str(n)],
                   cwd=rom.parent, capture_output=True)


def frames(rom: Path, at: int, out_dir: Path):
    shots = []
    for n in (at, at + 1):
        dest = out_dir / f"stab_{rom.stem}_{n}.png"
        shoot(rom, n, dest)
        shots.append(dest)
    return shots


def pixel_diff(a: Path, b: Path) -> int:
    A, B = Image.open(a).convert("RGB"), Image.open(b).convert("RGB")
    pa, pb = A.load(), B.load()
    return sum(1 for x in range(A.width) for y in range(A.height) if pa[x, y] != pb[x, y])


def find_plateau(rom: Path, start: int, span: int = 24) -> "tuple[int, int] | None":
    """Capture a window and return (frame, plateau_width) for the widest run of identical frames.

    Prefers the widest plateau, then the one nearest `start`, so a reference does not wander far
    from the moment it was chosen to show.
    """
    tmp = Path("/tmp")
    shots = []
    for n in range(start, start + span):
        dest = tmp / f"plat_{rom.stem}_{n}.png"
        shoot(rom, n, dest)
        shots.append((n, dest) if dest.exists() else (n, None))
    runs, cur = [], []
    for i, (n, path) in enumerate(shots):
        if path is None:
            continue
        if cur and pixel_diff(cur[-1][1], path) == 0:
            cur.append((n, path))
        else:
            if len(cur) >= 2:
                runs.append(cur)
            cur = [(n, path)]
    if len(cur) >= 2:
        runs.append(cur)
    if not runs:
        return None
    # Return the FIRST frame of the run, not its midpoint. The sweep checks N against N+1, and on a
    # two-frame plateau the midpoint IS the last frame — so `--find` would hand back a frame the
    # sweep then called unstable. The first frame always has its successor inside the same run.
    best = max(runs, key=lambda r: (len(r), -abs(r[0][0] - start)))
    return best[0][0], len(best)


def rom_dirs(args) -> "list[Path]":
    """The ROM directories to look at.

    An argument may be a ROM directory, a verify.sh, or (for this repo's own examples) a bare
    example name. Given nothing, it sweeps this repo's `examples/*` — which is right here and wrong
    anywhere else, so a caller in another repo passes its directories explicitly.
    """
    if not args:
        return sorted(p.parent for p in ROOT.glob("examples/*/verify.sh"))
    out = []
    for a in args:
        p = Path(a)
        if p.is_file() and p.name == "verify.sh":
            out.append(p.parent)
        elif p.is_dir():
            out.append(p)
        elif (ROOT / "examples" / a).is_dir():
            out.append(ROOT / "examples" / a)
        else:
            raise SystemExit(f"frame_stability: {a}: not a ROM directory or a verify.sh")
    return out


def main() -> int:
    argv = sys.argv[1:]
    if argv and argv[0] == "--find":
        rest = argv[1:]
        if not rest:
            raise SystemExit("usage: frame_stability.py --find <rom-dir>")
        d = rom_dirs(rest)[0]
        name = d.name
        cap = capture_for(d / "verify.sh")
        if not cap:
            print(f"{name}: no golden reference capture found in verify.sh")
            return 1
        rom = d / cap[0]
        got = find_plateau(rom, cap[1])
        if not got:
            print(f"{name}: no plateau within the search window — widen it")
            return 1
        print(f"{name}: use frame {got[0]} (plateau of {got[1]} frames)")
        return 0

    tmp = Path("/tmp")
    bad = 0
    checked = 0
    for d in rom_dirs(argv):
        verify = d / "verify.sh"
        name = d.name
        if not verify.is_file():
            continue
        got = capture_for(verify)
        if not got:
            continue
        rom = verify.parent / got[0]
        at = got[1]
        if not rom.exists():
            print(f"  {name:14s} skipped — {rom.name} not built")
            continue
        checked += 1
        a, b = frames(rom, at, tmp)
        if not (a.exists() and b.exists()):
            print(f"  {name:14s} skipped — capture failed")
            continue
        d = pixel_diff(a, b)
        if d == 0:
            print(f"  {name:14s} frame {at}: stable")
        else:
            print(f"  {name:14s} frame {at}: UNSTABLE — {d} px change at frame {at + 1}")
            bad += 1
    if bad:
        print(f"\n{bad} of {checked} reference frames sit on a transition. Find a plateau: capture")
        print("N and N+1 and require them identical, then move the verify.sh capture there.")
        return 1
    print(f"\nframe_stability: {checked} reference frames, all on a plateau")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
