#!/usr/bin/env python3
"""Pack the Tiny Tactics character + weapon art into GBA-ready sprite sheets.

Each CHARACTER (fighter / mage / cleric) becomes one 32x32 mega-sheet holding every animation state
for both drawn facings (SE, NE — the game h-flips them for SW / NW). Frame layout, per facing block:

    walk 0..7 | attack 8..11 | charge 12 | release 13 | damage 14 | weak 15 | dead 16

SE block starts at frame 0, NE block at frame DIR_STRIDE (17). These indices are mirrored by
`FRAME_*` constants in src/anim.tish — keep the two in sync.

Each WEAPON (6 of them) becomes a 64x64 sheet of the 4-frame attack swing for both facings
(SE 0..3, NE 4..7). The source swings are 48x48 (they arc outside the 32x32 body), centred into a
64x64 cell so the game can pin the weapon's centre to the character's centre with a fixed offset.

GBA sprites are 15 colours + transparent with no partial alpha, so every sheet is alpha-binarized
and each character is quantized to a single <=15-colour palette (one palette per character sheet)."""
from PIL import Image
import os

D = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))       # example dir
SRC = "/Users/a_/Downloads/TinyTactics_BattleKitI_v1_0"               # vendored kit (SE/NE state files)
CH_DATE, WP_DATE = "20240427", "20240429"
DIR_STRIDE = 17                                                       # frames per facing block

CHARS = ["fighter", "mage", "cleric"]
WEAPONS = ["WoodenSword", "IronSword", "Hatchet", "IronAxe", "WoodenStaff", "Scepter"]

# Sheets that arrive ALREADY assembled in the layout above, from outside the kit. They still have to
# meet the hardware's terms — 15 colours and no partial alpha — so they get the same conditioning the
# kit characters get, just without the frame-assembly step. `knight` came in at 29 colours, which
# fails the build with `TooManyColoursInSprite` from deep inside the sprite importer rather than
# anywhere that mentions the file, so it is worth having a named place for this to happen.
PREPACKED = ["knight"]


def binarize(im):
    """Snap alpha to 0/255 (GBA has no partial alpha; include_aseprite drops non-opaque pixels)."""
    im = im.convert("RGBA")
    px = im.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            px[x, y] = (r, g, b, 255) if a >= 128 else (0, 0, 0, 0)
    return im


def quantize15(sheet):
    """Reduce a whole RGBA sheet to a single <=15-colour palette, preserving binary transparency.
    Transparent pixels are parked on black so they cost one shared palette slot, not many."""
    sheet = binarize(sheet)
    alpha = sheet.getchannel("A")
    rgb = sheet.convert("RGB")   # transparent pixels are already (0,0,0) from binarize
    q = rgb.quantize(colors=15, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    out = q.convert("RGBA")
    out.putalpha(alpha)
    return out


def state_frames(who, state, direction):
    """Return the list of 32x32 frames for one character state+facing, row-major."""
    p = os.path.join(SRC, f"{CH_DATE}{who}-{state}{direction}.png")
    im = Image.open(p).convert("RGBA")
    W, H = im.size
    return [im.crop((c * 32, r * 32, c * 32 + 32, r * 32 + 32))
            for r in range(H // 32) for c in range(W // 32)]


def build_char(who):
    # SE block then NE block; each block laid out walk(8) attack(4) charge release damage weak dead.
    order = [("walking", 8), ("attack", 4), ("charging", 1),
             ("release", 1), ("damage", 1), ("weak", 1), ("dead", 1)]
    cells = 4                       # sheet is 4 frames wide
    total = 2 * DIR_STRIDE          # 34 frames
    rows = (total + cells - 1) // cells
    sheet = Image.new("RGBA", (cells * 32, rows * 32), (0, 0, 0, 0))
    for d, direction in enumerate(["SE", "NE"]):
        base = d * DIR_STRIDE
        fi = base
        for state, n in order:
            frames = state_frames(who, state, direction)
            assert len(frames) >= n, f"{who} {state}{direction}: want {n}, got {len(frames)}"
            for k in range(n):
                col, row = fi % cells, fi // cells
                sheet.paste(frames[k], (col * 32, row * 32))
                fi += 1
    out = quantize15(sheet)
    out.save(os.path.join(D, "assets", f"{who}.png"))
    # per-frame safety
    mx = max(len(set(p[:3] for p in out.crop((c * 32, r * 32, c * 32 + 32, r * 32 + 32)).getdata()
                     if p[3] > 0)) for r in range(rows) for c in range(cells))
    print(f"{who}.png: {out.size} = {total} frames (SE 0..16, NE 17..33), max {mx} colours/frame")


def build_weapon(name):
    cell = 64
    sheet = Image.new("RGBA", (cell * 4, cell * 2), (0, 0, 0, 0))    # 4 wide x 2 rows (SE row, NE row)
    for d, direction in enumerate(["SE", "NE"]):
        p = os.path.join(SRC, f"{WP_DATE}weapons-{name}attack{direction}.png")
        src = binarize(Image.open(p))                                # 192x48 = 4 frames of 48x48
        for i in range(4):
            fr = src.crop((i * 48, 0, i * 48 + 48, 48))
            padded = Image.new("RGBA", (cell, cell), (0, 0, 0, 0))
            padded.paste(fr, (8, 8), fr)                             # centre 48 in 64
            sheet.paste(padded, (i * cell, d * cell), padded)
    sheet.save(os.path.join(D, "assets", "weapons", f"{name}.png"))
    print(f"weapons/{name}.png: {sheet.size} = 8 frames (SE 0..3, NE 4..7) of 64x64")


def condition_prepacked(who):
    """Bring an externally-supplied sheet up to GBA terms, in place. Idempotent: a sheet already at
    15 colours quantizes to itself."""
    p = os.path.join(D, "assets", f"{who}.png")
    src = Image.open(p).convert("RGBA")
    before = len(set(q[:3] for q in src.getdata() if q[3] > 0))
    out = quantize15(src)
    out.save(p)
    after = len(set(q[:3] for q in out.getdata() if q[3] > 0))
    print(f"{who}.png: {out.size}, {before} -> {after} colours (GBA limit 15)")


# The kit is a vendored download; without it the prepacked sheets can still be conditioned.
if os.path.isdir(SRC):
    for w in CHARS:
        build_char(w)
    for w in WEAPONS:
        build_weapon(w)
else:
    print(f"skipping kit-built sheets: {SRC} not present")
for w in PREPACKED:
    condition_prepacked(w)
print("done.")
