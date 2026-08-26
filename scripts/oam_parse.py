#!/usr/bin/env python3
"""Decode a GBA OAM dump into the 128 objects the hardware actually drew.

WHY THIS EXISTS. Every de-risk-spike `verify.sh` asserts on `CHK <name> ok` lines printed by a
probe harness living in that same example's own `src/`. Those suites were all green while the game
had no enemies spawning, a duplicate HUD, a yellow-blob hero, and the wrong old-man and sword
sprites — because a check that reads the tested code's own printf is not a check. This tool reads
OAM, which is the hardware's object-attribute memory: the literal list of what the GBA put on
screen. A wrong sprite is a wrong tile index here; wrong movement is a wrong X/Y trajectory here.
Game code cannot forge it, because game code is not what writes this file — the emulator is.

Produce the dumps with an mgba-based memory probe (the `gba-probe` tool, which now lives in the
writing `<prefix>-f<frame>-OAM.bin`, 1024 bytes = 128 entries x 8 bytes.

⚠️ COORDINATES ARE NOT SCREEN COORDINATES UNTIL YOU SIGN THEM. Y is 8 bits and X is 9 bits, both
stored unsigned and both wrapping. An object one pixel off the top of the screen stores Y=255, not
-1. Compare raw values and a sprite walking off the top of the screen looks like it teleported to
the bottom — which is exactly the kind of false trajectory this tool exists to prevent. `y`/`x`
below are sign-corrected; `y_raw`/`x_raw` keep the stored bits.

⚠️ A DISABLED OBJECT STILL HAS COORDINATES AND A TILE. Hidden sprites keep their last attributes,
so "the object is at the right place with the right tile" is meaningless unless you also checked it
is visible. `disabled` is decoded here; assertions must honour it.

⚠️ TILE INDICES ARE COMPARABLE, PALETTE INDICES ARE NOT. tish/agb builds are nondeterministic in
emission order and agb palette bank assignment varies per build, so never assert an absolute
`palbank`. Tile indices are stable for a given built ROM, so compare a trajectory's tiles against
ANOTHER CAPTURE of the same ROM, or assert relative facts (the cell changed / stayed within a
range), never a literal from a previous build.

Usage
  oam_parse.py <dump.bin> [...]                 decode; one table per dump
  oam_parse.py <dump.bin> [...] --json          machine-readable
  oam_parse.py <dump.bin> [...] --visible       hide disabled objects
  oam_parse.py <dump.bin> [...] --track N       follow object slot N across dumps as a trajectory
  oam_parse.py <dump.bin> [...] --near X,Y[,R]  only objects within R px (default 24) of a point
"""
from __future__ import annotations

import argparse
import json
import re
import struct
import sys

# (shape, size) -> (width, height) in pixels. GBA Programming Manual, OBJ attributes.
OBJ_DIMS = {
    (0, 0): (8, 8),   (0, 1): (16, 16), (0, 2): (32, 32), (0, 3): (64, 64),   # square
    (1, 0): (16, 8),  (1, 1): (32, 8),  (1, 2): (32, 16), (1, 3): (64, 32),   # wide
    (2, 0): (8, 16),  (2, 1): (8, 32),  (2, 2): (16, 32), (2, 3): (32, 64),   # tall
}

OBJ_MODES = {0: "normal", 1: "blend", 2: "window", 3: "invalid"}


def decode_entry(slot: int, attr0: int, attr1: int, attr2: int) -> dict:
    """One OAM entry -> a record. Pure bit decoding, no interpretation."""
    rotscale = bool(attr0 & 0x100)
    # ⚠️ Bit 9 means two different things. With rot/scale ON it doubles the drawing box; with
    # rot/scale OFF it DISABLES the object. Conflating them reports hidden sprites as visible.
    double_size = rotscale and bool(attr0 & 0x200)
    disabled = (not rotscale) and bool(attr0 & 0x200)

    shape = (attr0 >> 14) & 0x3
    size = (attr1 >> 14) & 0x3
    w, h = OBJ_DIMS.get((shape, size), (0, 0))

    y_raw = attr0 & 0xFF
    x_raw = attr1 & 0x1FF
    # Sign-correct into screen space (see the header warning).
    y = y_raw - 256 if y_raw >= 160 else y_raw
    x = x_raw - 512 if x_raw >= 256 else x_raw

    return {
        "slot": slot,
        "x": x, "y": y, "x_raw": x_raw, "y_raw": y_raw,
        "tile": attr2 & 0x3FF,
        "priority": (attr2 >> 10) & 0x3,
        "palbank": (attr2 >> 12) & 0xF,
        "w": w, "h": h, "shape": shape, "size": size,
        "hflip": bool(attr1 & 0x1000) and not rotscale,
        "vflip": bool(attr1 & 0x2000) and not rotscale,
        "rotscale": rotscale,
        "affine": ((attr1 >> 9) & 0x1F) if rotscale else None,
        "double_size": double_size,
        "disabled": disabled,
        "mode": OBJ_MODES[(attr0 >> 10) & 0x3],
        "mosaic": bool(attr0 & 0x1000),
        "bpp": 8 if (attr0 & 0x2000) else 4,
    }


def parse(path: str) -> list[dict]:
    with open(path, "rb") as fh:
        raw = fh.read()
    if len(raw) < 1024:
        sys.exit(f"oam_parse: {path} is {len(raw)} bytes; an OAM dump is 1024")
    out = []
    for slot in range(128):
        a0, a1, a2, _fill = struct.unpack_from("<4H", raw, slot * 8)
        out.append(decode_entry(slot, a0, a1, a2))
    return out


def frame_of(path: str) -> int | None:
    """gba-probe names dumps `<prefix>-f<frame>-<block>.bin`."""
    m = re.search(r"-f(\d+)-", path)
    return int(m.group(1)) if m else None


def visible(o: dict) -> bool:
    """On-screen and not hidden. The GBA screen is 240x160."""
    if o["disabled"] or o["w"] == 0:
        return False
    return (o["x"] + o["w"] > 0 and o["x"] < 240
            and o["y"] + o["h"] > 0 and o["y"] < 160)


def _fmt_row(o: dict) -> str:
    flags = "".join([
        "H" if o["hflip"] else "-", "V" if o["vflip"] else "-",
        "R" if o["rotscale"] else "-", "X" if o["disabled"] else "-",
    ])
    return (f"  {o['slot']:>3}  {o['x']:>5},{o['y']:>4}  {o['w']:>2}x{o['h']:<2} "
            f" tile {o['tile']:>4}  pal {o['palbank']:>2}  pri {o['priority']}  {flags}"
            f"  {o['mode']}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dumps", nargs="+")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--visible", action="store_true", help="only on-screen, non-disabled objects")
    ap.add_argument("--track", type=int, metavar="SLOT",
                    help="follow one object slot across the dumps as a trajectory")
    ap.add_argument("--near", metavar="X,Y[,R]",
                    help="only objects within R px (default 24) of a point")
    args = ap.parse_args()

    # dumps are given in whatever order the shell globbed them; frame order is what matters
    dumps = sorted(args.dumps, key=lambda p: (frame_of(p) is None, frame_of(p) or 0, p))

    near = None
    if args.near:
        parts = [int(v) for v in args.near.split(",")]
        near = (parts[0], parts[1], parts[2] if len(parts) > 2 else 24)

    frames = []
    for path in dumps:
        objs = parse(path)
        if args.visible:
            objs = [o for o in objs if visible(o)]
        if near:
            nx, ny, r = near
            objs = [o for o in objs
                    if abs(o["x"] + o["w"] // 2 - nx) <= r and abs(o["y"] + o["h"] // 2 - ny) <= r]
        frames.append({"file": path, "frame": frame_of(path), "objects": objs})

    if args.track is not None:
        traj = []
        for f in frames:
            o = next((o for o in parse(f["file"]) if o["slot"] == args.track), None)
            if o:
                traj.append({"frame": f["frame"], **o})
        if args.json:
            print(json.dumps(traj, indent=1))
        else:
            print(f"slot {args.track} across {len(traj)} dumps:")
            print("  frame      x,   y   tile  pal  vis   dx,  dy")
            px = py = None
            for t in traj:
                dx = "" if px is None else f"{t['x']-px:>+4}"
                dy = "" if py is None else f"{t['y']-py:>+4}"
                vis = "yes" if visible(t) else "NO "
                print(f"  {str(t['frame']):>5}  {t['x']:>5},{t['y']:>4}  {t['tile']:>4}"
                      f"  {t['palbank']:>3}  {vis}  {dx},{dy}")
                px, py = t["x"], t["y"]
        return 0

    if args.json:
        print(json.dumps(frames, indent=1))
        return 0

    for f in frames:
        label = f"frame {f['frame']}" if f["frame"] is not None else f["file"]
        print(f"── {label}  ({len(f['objects'])} objects)")
        print("  slot      x,   y   size   tile   pal  pri  flags  mode")
        for o in f["objects"]:
            print(_fmt_row(o))
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
