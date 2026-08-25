#!/usr/bin/env python3
"""Tileset for examples/blockfall: twelve 8x8 tiles, derived from the vendored isometric block pack.

WHY 8x8 AND NOT 16x16. Every other tilemap example paints 16px cells, because that is what
`tilemap_set` addresses. A classic falling-block well is 10 columns by 20 rows, which is 320px tall at
16px — taller than the screen and taller than the map. `tilemap_set8` (added for this example) paints
one hardware tile, so the well is 80x160 and the HUD gets the other 144px.

WHY THE ART IS DERIVED AND NOT DRAWN. `scripts/gen_grid_demo.py` draws its gems from nothing on
purpose: that demo exists to prove `packages/grid.tish` is generic, and recognisable art would make a
genericity proof look like a specific game. blockfall has the opposite job — it is a GAME — so the
repo's asset rule applies and the art comes from the catalog:
`assets/iso-blocks/blocks_flat_16.png`, the Big Pixel Isometric Block Pack by Ajay Karat / Devil's
Work.shop (free for commercial use; see that directory's License.txt). Seven flat block faces are
box-downscaled 16 -> 8 and posterised to three shades of their own mean hue, then bevelled.

THE BEVEL IS NOT DECORATION. Two adjacent cells of the same colour have to stay countable, or a
4-wide flat row of O-pieces reads as one bar and the player cannot see where a gap is. Every block
gets a light top-left edge and a dark bottom-right one, which is the same readable-without-colour rule
the grid-demo gems follow with their glyphs.

⚠️ THE PALETTE CEILING IS SILENT. A 4bpp background tile must fit ONE 16-colour palette, and the
hardware has 16 of them. `include_background_gfx!` does not report a violation — it packs what it can
and the rest comes out as the wrong colour on device, which looks like a game bug. So this script
asserts both bounds and prints what it used, in the same spirit as `gen_kart_circuit.py` printing its
unique-tile count because an affine layer addresses tiles with a u8.

No Pillow: CI installs python3-numpy and nothing else, so the source PNG is decoded here with zlib.
"""
import pathlib
import struct
import zlib

SRC = pathlib.Path(__file__).resolve().parents[1] / "assets/iso-blocks/blocks_flat_16.png"
DST = pathlib.Path(__file__).resolve().parents[1] / "examples/blockfall/assets/blocks.png"
# The SAME art again, as a `sheet8:` sprite sheet. The settled stack is a tilemap; the falling piece
# and its ghost are sprites, because a tilemap write costs ~310 ticks on device and a piece plus ghost
# is sixteen of them per horizontal move — more than a whole frame. Sprites move by position.
DST_SPR = pathlib.Path(__file__).resolve().parents[1] / "examples/blockfall/assets/pieces.png"

CELL = 8            # the output tile: one hardware tile
SRCCELL = 16        # the pack's 16x16 variant
ATLAS_COLS = 16     # the pack is a 16-wide grid; index i is at (i % 16, i // 16)

# The well interior. Kept as ONE dark pair rather than pure black so an empty well still reads as a
# well — a black well against a black backdrop has no edges at all.
VOID = (14, 14, 22)
VOID2 = (24, 24, 36)

# The seven pieces, in the standard I O T S Z J L order, each named by its index in the pack's atlas.
# Hues chosen for maximum separation at 8x8 (see the pack's README for its own index landmarks):
# 64 is the water/ice block, 203-206 the bright warm/green family, 83 the deep blue, 127 the purple.
PIECES = [
    ("I", 64),    # cyan
    ("O", 205),   # yellow
    ("T", 127),   # purple
    ("S", 123),   # green
    ("Z", 204),   # red
    ("J", 83),    # blue
    ("L", 203),   # orange
]
WALL_BLOCK = 210      # dark stone, the NEUTRAL one of the 208-212 run — the well's side walls
GARBAGE_BLOCK = 45    # grey cobble — a topped-out row, and the AI's own mess


# ── PNG in ──────────────────────────────────────────────────────────────────────────────────────
def read_png(path):
    """Minimal 8-bit non-interlaced RGB/RGBA reader. Returns (w, h, rows of (r,g,b,a))."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path}: not a PNG")
    pos, idat, w, h, depth, ctype = 8, b"", 0, 0, 0, 0
    while pos < len(data):
        (ln,) = struct.unpack(">I", data[pos : pos + 4])
        typ = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + ln]
        if typ == b"IHDR":
            w, h, depth, ctype, _comp, _filt, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or ctype not in (2, 6) or interlace:
                raise SystemExit(f"{path}: need 8-bit non-interlaced RGB/RGBA, got {depth=} {ctype=}")
        elif typ == b"IDAT":
            idat += body
        pos += 12 + ln
    bpp = 4 if ctype == 6 else 3
    raw = zlib.decompress(idat)
    stride = w * bpp
    out, prev = [], bytearray(stride)
    for y in range(h):
        base = y * (stride + 1)
        ft = raw[base]
        line = bytearray(raw[base + 1 : base + 1 + stride])
        # Un-filter in place. The five PNG filter types; blocks_flat_16.png uses more than one.
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if ft == 1:
                line[i] = (line[i] + a) & 255
            elif ft == 2:
                line[i] = (line[i] + b) & 255
            elif ft == 3:
                line[i] = (line[i] + ((a + b) >> 1)) & 255
            elif ft == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 255
            elif ft != 0:
                raise SystemExit(f"{path}: unknown filter {ft} on row {y}")
        row = []
        for x in range(w):
            o = x * bpp
            row.append((line[o], line[o + 1], line[o + 2], line[o + 3] if bpp == 4 else 255))
        out.append(row)
        prev = line
    return w, h, out


# ── the derivation ──────────────────────────────────────────────────────────────────────────────
def face(rows, index):
    """The pack's flat face `index`, box-downscaled 16 -> 8. Opaque pixels only."""
    ax, ay = (index % ATLAS_COLS) * SRCCELL, (index // ATLAS_COLS) * SRCCELL
    out = []
    for y in range(CELL):
        line = []
        for x in range(CELL):
            acc, n = [0, 0, 0], 0
            for dy in range(2):
                for dx in range(2):
                    r, g, b, a = rows[ay + y * 2 + dy][ax + x * 2 + dx]
                    if a > 128:
                        acc[0] += r
                        acc[1] += g
                        acc[2] += b
                        n += 1
            line.append(tuple(v // n for v in acc) if n else VOID)
        out.append(line)
    return out


def lum(c):
    return (c[0] * 299 + c[1] * 587 + c[2] * 114) // 1000


def scaled(c, f):
    return tuple(min(255, max(0, v * f // 100)) for v in c)


def brick(rows, index):
    """One block tile: three posterised shades of the face's own hue, plus a bevel.

    Posterising to the MEAN HUE rather than keeping the source pixels is what holds the palette
    ceiling: a downscaled 16x16 face carries dozens of near-identical colours, and 12 tiles of those
    would not fit 16 palettes even though each tile individually might.
    """
    src = face(rows, index)
    px = [c for row in src for c in row]
    base = tuple(sum(c[i] for c in px) // len(px) for i in range(3))
    ls = sorted(lum(c) for c in px)
    lo, hi = ls[len(ls) // 4], ls[3 * len(ls) // 4]
    shades = (scaled(base, 72), base, scaled(base, 128))
    out = [[shades[0 if lum(c) <= lo else (2 if lum(c) >= hi else 1)] for c in row] for row in src]
    # The bevel, last, so it survives the posterise. Adjacent same-colour cells must stay countable.
    edge_hi, edge_lo = scaled(base, 165), scaled(base, 45)
    for i in range(CELL):
        out[0][i] = edge_hi
        out[i][0] = edge_hi
        out[CELL - 1][i] = edge_lo
        out[i][CELL - 1] = edge_lo
    return out


def solid(fill, dot=None):
    out = [[fill for _ in range(CELL)] for _ in range(CELL)]
    if dot is not None:
        out[0][0] = dot
    return out


def outline(colour, fill):
    """The ghost piece: an unfilled cell, so it can never be mistaken for a landed block."""
    out = [[fill for _ in range(CELL)] for _ in range(CELL)]
    for i in range(CELL):
        out[0][i] = colour
        out[CELL - 1][i] = colour
        out[i][0] = colour
        out[i][CELL - 1] = colour
    return out


w, h, rows = read_png(SRC)
if (w, h) != (ATLAS_COLS * SRCCELL, 15 * SRCCELL):
    raise SystemExit(f"{SRC}: expected a {ATLAS_COLS}x15 atlas of {SRCCELL}px blocks, got {w}x{h}")

# Tile order IS the gid order (`tilemap_set8` takes a 1-based row-major index), so this list is the
# contract with src/main.tish. It names each gid; the game re-states the same numbers as T_* constants.
tiles = [("void", solid(VOID, VOID2))]                                   # gid 1
tiles += [(name, brick(rows, idx)) for name, idx in PIECES]              # gid 2..8
tiles.append(("garbage", brick(rows, GARBAGE_BLOCK)))                    # gid 9
tiles.append(("wall", brick(rows, WALL_BLOCK)))                          # gid 10
tiles.append(("ghost", outline((150, 152, 190), VOID)))                  # gid 11
tiles.append(("flash", solid((248, 248, 248), (200, 200, 216))))         # gid 12

# ── the two ceilings, asserted ──────────────────────────────────────────────────────────────────
worst = 0
for name, t in tiles:
    n = len({c for row in t for c in row})
    worst = max(worst, n)
    if n > 16:
        raise SystemExit(f"tile {name}: {n} colours — a 4bpp tile must fit ONE 16-colour palette")
banks = len({frozenset(c for row in t for c in row) for _, t in tiles})
if banks > 16:
    raise SystemExit(f"{banks} distinct palettes — the hardware has 16 background banks")

# ── PNG out ─────────────────────────────────────────────────────────────────────────────────────
W, H = CELL * len(tiles), CELL
px = [[VOID] * W for _ in range(H)]
for i, (_name, t) in enumerate(tiles):
    for y in range(CELL):
        for x in range(CELL):
            px[y][i * CELL + x] = t[y][x]

raw = b"".join(b"\x00" + b"".join(bytes(p) for p in row) for row in px)


def chunk(t, d):
    c = t + d
    return struct.pack(">I", len(d)) + c + struct.pack(">I", zlib.crc32(c))


DST.parent.mkdir(parents=True, exist_ok=True)
DST.write_bytes(
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(raw))
    + chunk(b"IEND", b"")
)
print(f"wrote {DST} ({W}x{H}, {len(tiles)} tiles of {CELL}px)")
print(f"  gids: " + " ".join(f"{i + 1}={n}" for i, (n, _) in enumerate(tiles)))
print(f"  worst tile uses {worst}/16 colours; {banks}/16 palette banks")

# ── the sprite sheet ────────────────────────────────────────────────────────────────────────────
# Frame f is piece f for f in 0..6, then the ghost. Same tiles as the tilemap so a landed piece and a
# falling one are pixel-identical — the handover from sprite to tilemap at the moment of a lock is the
# one place a difference would be glaring.
#
# RGBA with a fully transparent background, which is what `include_aseprite_inner!` reads as the
# sprite's transparency (see examples/anim-demo/assets/walker.png). ⚠️ A GBA sprite palette is 16
# colours with entry 0 transparent, and there are 16 banks for ALL sprites — eight frames of four
# colours each is well inside it, but it is the ceiling that panics inside agb on an innocent caller
# if it is ever exceeded.
spr = [t for name, t in tiles if name in {n for n, _ in PIECES}] + [dict(tiles)["ghost"]]
SW, SH = CELL * len(spr), CELL
spx = [[(0, 0, 0, 0)] * SW for _ in range(SH)]
for i, t in enumerate(spr):
    for y in range(CELL):
        for x in range(CELL):
            c = t[y][x]
            # The ghost's interior is the well colour on the tilemap; as a sprite it must be a hole,
            # or the ghost paints an opaque dark box over the stack behind it.
            spx[y][i * CELL + x] = (0, 0, 0, 0) if c == VOID else (c[0], c[1], c[2], 255)

sraw = b"".join(b"\x00" + b"".join(bytes(p) for p in row) for row in spx)
DST_SPR.write_bytes(
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", SW, SH, 8, 6, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(sraw))
    + chunk(b"IEND", b"")
)
sbanks = len({frozenset(c for row in t for c in row) for t in spr})
if sbanks > 16:
    raise SystemExit(f"{sbanks} sprite palettes — the hardware has 16 object banks")
print(f"wrote {DST_SPR} ({SW}x{SH}, {len(spr)} frames of {CELL}px)")
print(f"  frames: " + " ".join(f"{i}={n}" for i, n in enumerate([n for n, _ in PIECES] + ["ghost"])))
print(f"  {sbanks}/16 sprite palette banks")
