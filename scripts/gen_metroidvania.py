#!/usr/bin/env python3
"""Bake the `metroidvania` example's GBA art from the raw CC0 GothicVania packs.

Raw packs are NOT vendored (they are tens of MB of PNG-per-frame folders). Download them to
`~/Downloads/metroidvania-art/` and run this; see examples/metroidvania/assets/ATTRIBUTION.md for
the exact sources. Everything here is CC0 (Luis Zuno / ansimuz).

    ~/Downloads/metroidvania-art/
      cemetery/gothicvania-cemetery-files/...      # hero, ghost, skeleton, hell-gato, death fx
      patreon/ gothicvania patreon collection/...  # old dark castle interior tileset + background

What comes out (examples/metroidvania/assets/):
  hero.png      sheet32:  19 poses, one 32x32 cell each
  enemies.png   sheet32:  ghost / skeleton / hell-gato, 4 frames each
  fx.png        sheet32:  sword arc + enemy death puff
  tileset.png   background: 16x16 curated tiles, FULLY OPAQUE (asset_bg rejects alpha)
  backdrop.png  background: the parallax castle backdrop, opaque

⚠️ Three GBA rules are baked into the numbers below and are not negotiable:
  1. A 4bpp sprite sheet gets 15 colours + transparent. `clamp_colors` quantises the ASSEMBLED
     sheet, not each frame, so every frame shares one Palette16 (= one of the 16 palette banks).
  2. `background:` art must be fully opaque or asset_bg forced-blanks the screen.
  3. Anti-aliased edges must be hardened to alpha 0/255 — agb treats a != 255 as transparent, so
     a soft edge dissolves into holes. (`harden_alpha`, learned the hard way on examples/versus.)
"""
import os
import sys

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fighter_art import clamp_colors, harden_alpha, union_bbox  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EX = os.path.join(REPO, "examples", "metroidvania")
ASSETS = os.path.join(EX, "assets")
RAW = os.path.expanduser("~/Downloads/metroidvania-art")

CEM = os.path.join(RAW, "cemetery", "gothicvania-cemetery-files", "PNG")
CHURCH = os.path.join(RAW, "church", "gothicvania church files", "SPRITES")
CASTLE = os.path.join(RAW, "patreon", " gothicvania patreon collection", "Old-dark-Castle-tileset-Files", "PNG")

CELL = 32
TILE = 16

# The hero is drawn ~45px tall on a 100x59 canvas. A GBA screen is 160px tall, so a 45px hero eats a
# third of it — Samus is ~40px on a 224px SNES screen. 0.62 puts him at ~28px: the body plus a usable
# amount of sword inside one 32x32 cell, with the long attack reach going to fx.png instead.
HERO_SCALE = 0.64
ENEMY_SCALE = 0.60


def load(path):
    return Image.open(path).convert("RGBA")


def scaled(im, k):
    w, h = max(1, round(im.width * k)), max(1, round(im.height * k))
    return im.resize((w, h), Image.NEAREST)


def seq(folder, stem, n, start=1):
    """`hero-run-1.png` … `hero-run-6.png` → a list of frames."""
    out = []
    for i in range(start, start + n):
        p = os.path.join(folder, f"{stem}-{i}.png")
        if not os.path.exists(p):  # some clips are a single unnumbered file
            p = os.path.join(folder, f"{stem}.png")
        out.append(load(p))
    return out


def one(folder, stem):
    return [load(os.path.join(folder, f"{stem}.png"))]


def bake_actor(clips, scale, cell=CELL, foot_pad=0, anchor_clips=1):
    """Scale, then crop every frame to one `cell`x`cell` box anchored bottom-centre.

    ⚠️ ONE anchor for the whole actor, never per frame — per-frame centring makes a sprite bob and
    slide when it changes clip, because each frame gets centred on its own ink.

    ⚠️⚠️ But the anchor comes from the FIRST `anchor_clips` clips (the neutral standing pose), NOT
    from every frame. Anchoring on the union of everything let the attack frames vote: this hero's
    sword reaches a long way forward, which dragged the union's centre sideways and pushed the BODY
    off-centre in every other frame — so he stood and ran leaning out of his own cell, and read as
    lying down rather than running.
    """
    frames = [scaled(f, scale) for clip in clips for f in clip]
    anchor_frames = [scaled(f, scale) for clip in clips[:anchor_clips] for f in clip]
    bb = union_bbox(anchor_frames)
    if bb is None:
        raise SystemExit("bake_actor: every frame was empty")
    x0, y0, x1, y1 = bb
    ax = (x0 + x1) // 2          # anchor: horizontal centre of the union
    ay = y1 + foot_pad           # and its bottom — the ground line
    out = []
    for f in frames:
        cellim = Image.new("RGBA", (cell, cell), (0, 0, 0, 0))
        cellim.alpha_composite(f, dest=(0, 0), source=(0, 0))
        # paste f so that (ax, ay) lands at (cell/2, cell)
        c = Image.new("RGBA", (cell, cell), (0, 0, 0, 0))
        c.alpha_composite(f, dest=(cell // 2 - ax, cell - ay))
        out.append(c)
    return out


def strip(frames, cell=CELL):
    out = Image.new("RGBA", (cell * len(frames), cell), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        out.alpha_composite(f, dest=(i * cell, 0))
    return out


def nseq(folder, stem, n, start=1):
    """`idle1.png` … `idle4.png` — the church pack numbers without a separator."""
    return [load(os.path.join(folder, f"{stem}{i}.png")) for i in range(start, start + n)]


def make_hero():
    """The hero is the CHURCH pack's monk, not the cemetery pack's swordsman.

    ⚠️ The swordsman looked better in the sheet and read as LYING DOWN in the game. His run is a deep
    forward lunge with a long sword held out level, so at 32x32 on a 160px-tall screen the silhouette
    is a horizontal bar — the pose that carries a 100px illustration is the pose that destroys a GBA
    sprite. The monk is 23x44 of compact upright body with no weapon to smear the outline, and his
    red-and-tan palette separates from the dark green castle instead of sinking into it.

    Pick sprites by SILHOUETTE AT FINAL SIZE, not by how good the source art is.
    """
    h = os.path.join(CHURCH, "player", "sprites")
    clips = [
        nseq(h, "idle", 4),                 # 0-3   idle   (anchor clip — see bake_actor)
        nseq(h, "walk", 6),                 # 4-9   walk / run
        nseq(h, "jump", 1),                 # 10    rising
        nseq(h, "fall", 1),                 # 11    falling
        nseq(h, "crouch", 1),               # 12    wall slide (reuses the crouch lean)
        nseq(h, "crouch", 1),               # 13    crouch / slide
        nseq(h, "punch", 3, start=2),       # 14-16 attack
        nseq(h, "hurt", 1),                 # 17    hurt
        nseq(h, "hurt", 2)[1:],             # 18    dead
    ]
    frames = bake_actor(clips, HERO_SCALE)
    sheet = clamp_colors(harden_alpha(strip(frames)), 15)
    sheet.save(os.path.join(ASSETS, "hero.png"))
    return len(frames)


def make_enemies():
    s = os.path.join(CEM, "Sprites")
    banks = [
        bake_actor([seq(os.path.join(s, "ghost"), "ghost", 4)], ENEMY_SCALE),
        bake_actor([seq(os.path.join(s, "skeleton-clothed"), "skeleton-clothed", 4)], ENEMY_SCALE),
        bake_actor([seq(os.path.join(s, "hell-gato"), "hell-gato", 4)], ENEMY_SCALE),
    ]
    frames = [f for b in banks for f in b]
    sheet = clamp_colors(harden_alpha(strip(frames)), 15)
    sheet.save(os.path.join(ASSETS, "enemies.png"))
    return len(frames)


def make_fx():
    s = os.path.join(CEM, "Sprites", "enemy-death")
    frames = bake_actor([seq(s, "enemy-death", 5)], ENEMY_SCALE)
    sheet = clamp_colors(harden_alpha(strip(frames)), 15)
    sheet.save(os.path.join(ASSETS, "fx.png"))
    return len(frames)


# Curated 16x16 tiles, by (col, row) in the castle interior sheet. GID 1 is the synthetic void so a
# map's empty space is a real opaque tile rather than a hole.
#   1 void   2 floor top (gold edge)   3 solid fill   4 back wall   5 ledge (one-way)   6 brick
TILE_PICKS = [(26, 10), (26, 11), (17, 12), (18, 10), (20, 12)]
VOID = (16, 14, 26)


def make_tileset():
    src = load(os.path.join(CASTLE, "old-dark-castle-interior-tileset.png"))
    cells = [Image.new("RGBA", (TILE, TILE), VOID + (255,))]
    for (c, r) in TILE_PICKS:
        cells.append(src.crop((c * TILE, r * TILE, c * TILE + TILE, r * TILE + TILE)))
    cols = len(cells)
    out = Image.new("RGBA", (TILE * cols, TILE), VOID + (255,))
    for i, cell in enumerate(cells):
        # composite onto the void colour: `background:` art must be FULLY opaque.
        base = Image.new("RGBA", (TILE, TILE), VOID + (255,))
        base.alpha_composite(cell)
        out.alpha_composite(base, dest=(i * TILE, 0))
    out = clamp_colors(out.convert("RGBA"), 15).convert("RGB")
    out.save(os.path.join(ASSETS, "tileset.png"))
    return cols


def make_heart():
    """A 16x16 heart in three states. ⚠️ ORDER MATTERS AND IT IS NOT THE OBVIOUS ONE: `hud_hearts`
    indexes `0 = empty .. perHeart = full`, so with `perHeart: 2` the sheet is empty, half, full —
    three frames, emptiest first. Baking full-first (and only two frames) is why the HUD drew three
    dark blobs that never changed."""
    full, half, empty = (214, 58, 84), (150, 40, 60), (52, 40, 62)
    shape = [
        "..XX...XX..",
        ".XXXXXXXXX.",
        "XXXXXXXXXXX",
        "XXXXXXXXXXX",
        ".XXXXXXXXX.",
        "..XXXXXXX..",
        "...XXXXX...",
        "....XXX....",
        ".....X.....",
    ]
    out = Image.new("RGBA", (16 * 3, 16), (0, 0, 0, 0))
    for i in range(3):
        for y, row in enumerate(shape):
            for x, ch in enumerate(row):
                if ch != "X":
                    continue
                if i == 0:
                    col = empty                      # frame 0: empty
                elif i == 1:
                    col = full if x < 5 else empty   # frame 1: left half filled
                else:
                    col = full                       # frame 2: full
                out.putpixel((i * 16 + x + 2, y + 3), col + (255,))
        # a dark outline row so an empty heart still reads against the castle
        for y, row in enumerate(shape):
            for x, ch in enumerate(row):
                if ch == "X" and out.getpixel((i * 16 + x + 2, y + 3))[3] == 0:
                    out.putpixel((i * 16 + x + 2, y + 3), half + (255,))
    out.save(os.path.join(ASSETS, "heart.png"))


def make_backdrop():
    src = Image.open(os.path.join(CASTLE, "old-dark-castle-interior-background.png")).convert("RGB")
    # One screen's worth, scaled to 240x160 and colour-clamped; opaque by construction.
    out = src.resize((240, 160), Image.NEAREST)
    out = clamp_colors(out.convert("RGBA"), 15).convert("RGB")
    out.save(os.path.join(ASSETS, "backdrop.png"))


def main():
    if not os.path.isdir(RAW):
        raise SystemExit(f"raw art not found at {RAW} — see assets/ATTRIBUTION.md for the packs")
    os.makedirs(ASSETS, exist_ok=True)
    nh = make_hero()
    ne = make_enemies()
    nf = make_fx()
    nt = make_tileset()
    make_backdrop()
    make_heart()
    make_level()
    print(f"metroidvania: hero {nh} frames · enemies {ne} · fx {nf} · tileset {nt} tiles · backdrop 240x160")




# ── level ────────────────────────────────────────────────────────────────────
# '.' void   '#' ground (grass-gold top row, solid fill below)   'B' brick   '^' one-way ledge
# 'w' back wall (decor, walkable)   '@' spawn   'g' ghost  's' skeleton  'c' hell-gato
# 'D' double-jump pickup   'W' wall-jump pickup   'S' slide pickup   'X' boss   'H' save/heal point
#
# The shape is the point, not the size. Reading left to right:
#   - the entry hall (spawn) is walled off from the high east ledge, so the FIRST thing you find is
#     a wall you cannot pass — a metroidvania has to show you the lock before it gives you the key;
#   - the shaft at col 20 is two facing walls with no floor holds: wall-jump only;
#   - the crawl at row 17 cols 30-36 is a one-tile gap: slide only;
#   - the gold ledge at cols 40-46 is above a double-jump-height gap;
#   - and the drop back at col 47 lands you in the entry hall, which is the backtrack shortcut.
# The hall is BUILT, not drawn. The previous version was hand-counted ASCII, and every feature was a
# column or two off from where the comment said it was — which is exactly what "the levels are
# nonsense" looks like from inside the game. Naming the coordinates means the walls, the pickups and
# the gaps cannot drift apart, and the asserts below fail the bake rather than shipping a broken room.
W, H = 40, 14
FLOOR = 11          # the floor's top row; 12 is fill, 13 is bedrock
CEIL = 0

SHAFT_L, SHAFT_R = 26, 30      # facing walls of the wall-jump shaft (interior 27..29)
LEDGE_L, LEDGE_R = 19, 22      # the double-jump ledge
LEDGE_ROW = 6
BLOCK_L, BLOCK_R = 13, 15      # the low block the double jump pickup sits on
BLOCK_ROW = 9
CRAWL_L, CRAWL_R = 33, 37      # one-tile-high crawl at floor level -> slide only

SPAWN = (3, FLOOR - 1)
PICK_DOUBLE = (14, BLOCK_ROW - 1)
PICK_WALL = (20, LEDGE_ROW - 1)
PICK_SLIDE = (28, 2)
ENEMIES = [(6, FLOOR - 1, 1), (24, FLOOR - 1, 2)]     # skeleton, hell-gato


def build_level():
    """An ENCLOSED hall, read left to right: ceiling, floor, walls at both ends, and every gate
    sitting where you can see it before you can pass it."""
    g = [["." for _ in range(W)] for _ in range(H)]
    for x in range(W):
        g[CEIL][x] = "#"
        for y in range(FLOOR, H):
            g[y][x] = "#"
    for y in range(H):
        g[y][0] = "#"
        g[y][W - 1] = "#"

    # ⚠️ No pit in the entry hall. It "taught the jump", which you can already do, and all it
    # actually did was drop you in a hole on the way to everything else. A demo room should not have
    # a failure state before it has a feature.
    for x in range(BLOCK_L, BLOCK_R + 1):                  # step up to the double jump
        g[BLOCK_ROW][x] = "#"
    for x in range(LEDGE_L, LEDGE_R + 1):                  # only a double jump reaches this
        g[LEDGE_ROW][x] = "#"

    # The wall-jump shaft: two facing walls with no holds between them, entered on foot at floor
    # level and KEEPING ITS FLOOR. ⚠️ A bottomless shaft you can fall into before you have the wall
    # jump is a soft-lock, not a gate — examples/sunny-land shipped exactly that bug for months. You
    # can always walk back out of this one; what you cannot do, until the pickup, is go UP it.
    for y in range(1, FLOOR - 1):
        g[y][SHAFT_L] = "#"
        g[y][SHAFT_R] = "#"

    for x in range(CRAWL_L, CRAWL_R + 1):                  # slide through this
        g[FLOOR - 2][x] = "#"

    for (x, y) in [SPAWN]:
        g[y][x] = "@"
    for (x, y), ch in [(PICK_DOUBLE, "D"), (PICK_WALL, "W"), (PICK_SLIDE, "S")]:
        g[y][x] = ch
    for (x, y, k) in ENEMIES:
        g[y][x] = "gsc"[k]

    rows = ["".join(r) for r in g]
    # The room must actually be a room, and the gates must actually be gaps.
    assert all(len(r) == W for r in rows), "ragged level"
    assert rows[CEIL] == "#" * W, "no ceiling"
    assert rows[FLOOR][SHAFT_L + 1] == "#", "shaft is bottomless - that is a soft-lock, not a gate"
    assert rows[FLOOR - 1][SHAFT_L] == ".", "shaft cannot be walked into"
    assert rows[LEDGE_ROW][SHAFT_L] == "#" and rows[LEDGE_ROW][SHAFT_R] == "#", "shaft walls missing"
    assert rows[FLOOR - 1][CRAWL_L] == "." and rows[FLOOR - 2][CRAWL_L] == "#", "crawl is not one tile"
    assert rows[LEDGE_ROW][LEDGE_L] == "#", "double-jump ledge missing"
    return rows


LEVEL = build_level()

CH_GID = {".": 1, "#": 3, "B": 6, "^": 5, "w": 4}
SOLID = [2, 3, 6]
ONEWAY = [5]


def parse_level(rows):
    h, w = len(rows), max(len(r) for r in rows)
    rows = [r.ljust(w, ".") for r in rows]
    gid = [[1] * w for _ in range(h)]
    spawn = (6, 8)
    enemies, pickups, boss, save = [], [], None, None
    for y in range(h):
        for x in range(w):
            ch = rows[y][x]
            if ch == "@":
                spawn = (x, y)
            elif ch in "gsc":
                enemies.append((x, y, "gsc".index(ch)))
            elif ch in "DWS":
                pickups.append((x, y, "DWS".index(ch)))
            elif ch == "X":
                boss = (x, y)
            elif ch == "H":
                save = (x, y)
            elif ch in CH_GID:
                gid[y][x] = CH_GID[ch]
    # a '#' with void directly above becomes the gold-topped surface tile
    for y in range(h):
        for x in range(w):
            if gid[y][x] == 3 and (y == 0 or gid[y - 1][x] in (1, 5)):
                gid[y][x] = 2
    flat = [gid[y][x] for y in range(h) for x in range(w)]
    return w, h, flat, spawn, enemies, pickups, boss, save


def emit_maps(path, w, h, data, spawn, enemies, pickups):
    def cols(xs, i):
        return ", ".join(str(p[i]) for p in xs)
    rowstrs = []
    for y in range(h):
        rowstrs.append("      " + ", ".join(str(data[y * w + x]) for x in range(w)) + ("," if y < h - 1 else ""))
    body = "\n".join(rowstrs)
    txt = f"""// Generated by scripts/gen_metroidvania.py — the castle. Edit LEVEL there, not here.
// GIDs: 1 void, 2 floor top, 3 solid fill, 4 back wall (decor), 5 one-way ledge, 6 brick.
export const level = {{
  width: {w}, height: {h}, tileSize: 16, tilesetCols: 6,
  spawnCol: {spawn[0]}, spawnRow: {spawn[1]},
  solid: [{", ".join(map(str, SOLID))}],
  oneway: [{", ".join(map(str, ONEWAY))}],
  enemyCols: [{cols(enemies, 0)}], enemyRows: [{cols(enemies, 1)}], enemyKinds: [{cols(enemies, 2)}],
  pickupCols: [{cols(pickups, 0)}], pickupRows: [{cols(pickups, 1)}], pickupKinds: [{cols(pickups, 2)}],
  layers: [
    {{ priority: 2, data: [
{body}
    ] }}
  ]
}}
"""
    open(path, "w").write(txt)


def make_level():
    w, h, data, spawn, enemies, pickups, boss, save = parse_level(LEVEL)
    src = os.path.join(EX, "src")
    os.makedirs(src, exist_ok=True)
    emit_maps(os.path.join(src, "maps.tish"), w, h, data, spawn, enemies, pickups)
    print(f"metroidvania: level {w}x{h} · spawn {spawn} · {len(enemies)} enemies · {len(pickups)} pickups")

if __name__ == "__main__":
    main()
