#!/usr/bin/env python3
"""Tileset for examples/grid-demo: five 16x16 gem tiles plus an empty and a frame tile.

Deliberately tiny and generated rather than pulled from the catalog: this demo exists to prove
packages/grid.tish is generic, and hand-picked art would make it look like a specific game. Each
gem carries a distinct GLYPH as well as a hue, which is the readable-without-colour rule the
the retired match-3 art already follows.
"""
import struct, zlib, pathlib

CELL = 16
COLS = 8
W, H = CELL * COLS, CELL

BG = (16, 16, 24)
GEMS = [
    ((208, 56, 64), "diamond"),
    ((80, 208, 112), "square"),
    ((224, 160, 48), "circle"),
    ((74, 144, 232), "triangle"),
    ((176, 96, 216), "cross"),
]

px = [[BG for _ in range(W)] for _ in range(H)]

def put(cx, x, y, c):
    if 0 <= x < CELL and 0 <= y < CELL:
        px[y][cx * CELL + x] = c

# tile 0 stays empty (backdrop)
for i, (col, shape) in enumerate(GEMS):
    cx = i + 1
    dark = tuple(max(0, v - 70) for v in col)
    lite = tuple(min(255, v + 60) for v in col)
    for y in range(CELL):
        for x in range(CELL):
            inside = False
            dx, dy = x - 7.5, y - 7.5
            if shape == "diamond":
                inside = abs(dx) + abs(dy) <= 6.5
            elif shape == "square":
                inside = abs(dx) <= 5 and abs(dy) <= 5
            elif shape == "circle":
                inside = dx * dx + dy * dy <= 40
            elif shape == "triangle":
                inside = dy > -6 and abs(dx) <= (dy + 6) * 0.55
            else:  # cross
                inside = (abs(dx) <= 2.2 and abs(dy) <= 6.5) or (abs(dy) <= 2.2 and abs(dx) <= 6.5)
            if inside:
                # one highlight pixel band so the gem reads as solid rather than flat
                put(cx, x, y, lite if (dx + dy) < -3 else col)
    # a dark rim, so adjacent same-colour gems stay countable
    for t in range(CELL):
        put(cx, t, 0, dark)
        put(cx, t, CELL - 1, dark)
        put(cx, 0, t, dark)
        put(cx, CELL - 1, t, dark)

# tile 6: the well floor/wall
for y in range(CELL):
    for x in range(CELL):
        put(6, x, y, (56, 52, 72) if (x + y) % 4 else (74, 70, 92))

raw = b"".join(b"\x00" + b"".join(bytes(p) for p in row) for row in px)
def chunk(t, d):
    c = t + d
    return struct.pack(">I", len(d)) + c + struct.pack(">I", zlib.crc32(c))
out = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(raw))
       + chunk(b"IEND", b""))
p = pathlib.Path(__file__).resolve().parents[1] / "examples/grid-demo/assets/gems.png"
p.write_bytes(out)
print(f"wrote {p} ({W}x{H}, {COLS} tiles)")
