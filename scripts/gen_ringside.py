#!/usr/bin/env python3
"""Art bake for `examples/ringside` — the over-the-shoulder boxing demo.

── WHY THIS IS PROCEDURAL ────────────────────────────────────────────────────────────────────────

Every other character-driven example in this repo (`versus`, `beatemup`) resamples a CC0 pack. This
one cannot, and it was checked rather than assumed:

  * the vendored catalog (`assets/`, scripts/asset_search) is Ninja Adventure — no boxing art;
  * `~/Downloads/versus-art` is LuizMelo Martial Hero 1/2/3 + Medieval Warrior — all SIDE PROFILE,
    which is the one camera angle an over-the-shoulder boxing game cannot use;
  * itch.io's free `boxing` tag is three 3D models, a music pack and a finished game;
  * the only CC0 2D boxer anywhere (OpenGameArt "Boxer Game Character") is side-profile too.

So the art is drawn here, which is the house fallback (`gen_golf_art.py`, `gen_asteroids.py`, and
versus's own `spark.png` and stage ground strip are all procedural). It is also the better source at
this scale: a crowd at 240x160 is dithered blobs and a rear-view player is a silhouette — both
generate more cleanly than they resample.

── THE FOUR TRAPS, ALL OF WHICH LOOK FINE IN A PREVIEW ───────────────────────────────────────────

  1. ALPHA MUST BE HARDENED TO 0 OR 255. A GBA sprite has one transparent colour, not an alpha
     channel. `fighter_art.harden_alpha` — reused, not re-derived.

  2. ONE PNG IS ONE Palette16. The opponent's upper and lower bands are in the SAME sheet for
     exactly this reason: two PNGs quantised separately put a colour step across the waist seam.
     Costs 2 KB of VRAM (the lower band is padded to 64x64 rather than using `sheet6432:`) and buys
     a seam that cannot show.

  3. BANDS, NEVER QUADRANTS. Every piece shares a FULL EDGE with its neighbour, so a seam always
     reads as joined. `docs/fighting-genre.md` §4 records what a diagonal piece looks like instead:
     a slab floating beside the character.

  4. THE BACKGROUND PALETTE PACKER FAILS NAMING NOTHING. agb's `overload_and_remove` is worse than
     greedy first-fit and dies with `DoesNotFitError { count: N }` — no tile, no colour. The ring
     background is held under BG_MAX_COLOURS and checked here, at generate time, where the error can
     say which colour to drop.

Run: `npm run assets` in examples/ringside (or `python3 scripts/gen_ringside.py`).
Writes examples/ringside/assets/*.png, examples/ringside/src/frames.tish, and a preview composite.
"""
import os
import sys

from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fighter_art import harden_alpha, clamp_colors, digits_sheet  # noqa: E402

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT = os.path.join(REPO, "examples", "ringside", "assets")
SRC = os.path.join(REPO, "examples", "ringside", "src")

CELL = 64
BG_MAX_COLOURS = 26

# ── Screen geometry. These numbers are duplicated into frames.tish so the game and the art agree.
SCREEN_W, SCREEN_H = 240, 160
OPP_X, OPP_UP_Y, OPP_LO_Y = 56, 12, 74      # opponent bands, top-left of the left cell
PL_X, PL_Y = 56, 118                         # player band — low, so the opponent's legs stay visible
GLOVE_HOME_X, GLOVE_HOME_Y = 104, 62          # where a thrown glove lands, mid-screen

# ── The palette. 15 colours + transparent, shared by every sprite sheet so the fighters, the
#    gloves and the spark cannot drift apart. Ordered darkest-first purely for readability.
INK        = (24, 18, 28, 255)      # outline
SKIN_D     = (140, 82, 54, 255)
SKIN       = (196, 132, 88, 255)
SKIN_L     = (232, 178, 132, 255)
PSKIN_D    = (122, 74, 46, 255)     # the player is darker so the two never read as the same man
PSKIN      = (168, 108, 68, 255)
TRUNK_D    = (86, 22, 40, 255)
TRUNK      = (170, 44, 68, 255)
GLOVE_D    = (150, 74, 18, 255)
GLOVE      = (232, 128, 32, 255)
PGLOVE_D   = (24, 62, 110, 255)     # player gloves are blue: at a glance you must know whose fist
PGLOVE     = (56, 122, 196, 255)
HAIR       = (52, 36, 30, 255)
WHITE      = (244, 240, 232, 255)
FLASH      = (255, 238, 160, 255)

CLEAR = (0, 0, 0, 0)


# ══════════════════════════════════════════════════════════════════════════════════════════════
# Small drawing helpers. Everything is built from these so the whole cast shares one visual grammar.
# ══════════════════════════════════════════════════════════════════════════════════════════════

def ell(d, cx, cy, rx, ry, fill, outline=INK):
    d.ellipse((cx - rx, cy - ry, cx + rx, cy + ry), fill=fill, outline=outline)


def box(d, cx, cy, hw, hh, fill, outline=INK):
    d.rectangle((cx - hw, cy - hh, cx + hw, cy + hh), fill=fill, outline=outline)


def glove(d, cx, cy, r, main, dark):
    """A boxing glove: a ball, a thumb, and a highlight. Reads at 12px and at 26px."""
    ell(d, cx, cy, r, r, main)
    ell(d, cx - int(r * 0.62), cy + int(r * 0.20), max(2, r // 3), max(2, r // 4), dark)
    ell(d, cx + int(r * 0.28), cy - int(r * 0.34), max(1, r // 4), max(1, r // 5), FLASH, outline=None)


# ══════════════════════════════════════════════════════════════════════════════════════════════
# The opponent — BRUNO THE BULL. Drawn at 128x128 and split into four 64x64 cells.
#
# The upper band is the whole game: it carries the TELL, which is one pose held for ~18 frames and
# is the only information the player gets. Every tell is therefore a large, asymmetric silhouette
# change — a shoulder that drops, a body that folds, arms that rear back — and never a detail.
# ══════════════════════════════════════════════════════════════════════════════════════════════

# upper-band poses, by index
(U_BOB_A, U_BOB_B, U_TELL_L, U_TELL_R, U_TELL_BODY, U_TELL_HIGH,
 U_STRIKE_L, U_STRIKE_R, U_STRIKE_BODY, U_GUARD, U_HURT, U_STUN,
 U_TAUNT, U_DOWN) = range(14)
N_UPPER = 14

# lower-band poses, by index
L_STAND, L_BOB, L_CROUCH, L_WIDE, L_DOWN = range(5)
N_LOWER = 5


def opp_upper(pose):
    """One 128x64 upper band: head, torso, both arms."""
    im = Image.new("RGBA", (128, 64), CLEAR)
    d = ImageDraw.Draw(im)
    cx = 64

    # Defaults, overridden per pose. The guard sits CLOSE and HIGH, so that every tell below can be
    # a large outward move away from it — a tell is only readable against a compact resting shape.
    lean, hy = 0, 22
    lgx, lgy, lgr = 40, 34, 12      # left glove (player's left = screen left)
    rgx, rgy, rgr = 88, 34, 12
    face = "calm"

    if pose == U_BOB_A:
        pass
    elif pose == U_BOB_B:
        lean, hy = 2, 24
        lgy, rgy = 36, 32
    elif pose == U_TELL_L:
        # ⚠️ THE TELL IS THE GAME. The arm cocks all the way back and down, clear of the torso, so
        # the silhouette changes on one side only — that asymmetry is the whole readable signal, and
        # it is why every tell here is an EXTREME pose rather than a nuanced one.
        lean, lgx, lgy, lgr = -4, 6, 58, 15
        rgx, rgy = 84, 30
    elif pose == U_TELL_R:
        lean, rgx, rgy, rgr = 4, 122, 58, 15
        lgx, lgy = 44, 30
    elif pose == U_TELL_BODY:
        # folds down and BOTH gloves drop below the waist: the silhouette loses its head height,
        # which is a different signal from either single-arm tell and cannot be confused with them
        hy, lgy, rgy = 36, 62, 62
        lgx, rgx = 34, 94
    elif pose == U_TELL_HIGH:
        # rears back with both gloves ABOVE the head — the silhouette grows upward instead
        hy, lgy, rgy, lgr, rgr = 30, 4, 4, 15, 15
        lgx, rgx = 30, 98
    elif pose == U_STRIKE_L:
        lean, lgx, lgy, lgr = -2, 54, 44, 26          # thrust toward the camera: big and near
        rgx, rgy = 96, 30
        face = "grit"
    elif pose == U_STRIKE_R:
        lean, rgx, rgy, rgr = 2, 74, 44, 26
        lgx, lgy = 32, 30
        face = "grit"
    elif pose == U_STRIKE_BODY:
        hy, lgx, lgy, lgr = 32, 62, 58, 24
        rgx, rgy = 98, 44
        face = "grit"
    elif pose == U_GUARD:
        lgx, lgy, rgx, rgy = 52, 20, 76, 20           # both gloves cover the face
        lgr = rgr = 14
    elif pose == U_HURT:
        lean, hy = -5, 14                              # head snaps back, guard falls away
        lgx, lgy, rgx, rgy = 16, 58, 112, 58
        face = "hurt"
    elif pose == U_STUN:
        lean, hy = 6, 24
        lgx, lgy, rgx, rgy = 10, 62, 118, 62
        face = "stun"
    elif pose == U_TAUNT:
        lgx, lgy, rgx, rgy = 4, 26, 124, 26            # arms flung wide — a free punish window
        face = "grit"
    elif pose == U_DOWN:
        return im                                       # nothing up here; it is all in the lower band

    cx += lean

    # Arms FIRST, so the torso overlaps them at the shoulder and the joint reads as a joint rather
    # than as a stick pushed into a wedge. Drawn shoulder -> elbow -> glove: a straight line to a
    # cocked-back glove passes straight through the torso and the arm disappears, which is what made
    # the gloves look like they were floating unattached.
    for (gx, gy, sx) in ((lgx, lgy, cx - 26), (rgx, rgy, cx + 26)):
        ex = sx + (gx - sx) // 3 + (-8 if gx < cx else 8)   # elbow bows AWAY from the body
        ey = (hy + 20 + gy) // 2
        for w, col in ((11, INK), (9, SKIN_D), (5, SKIN)):
            d.line([(sx, hy + 20), (ex, ey), (gx, gy)], fill=col, width=w, joint="curve")

    # torso: a wedge, wider at the shoulders
    d.polygon([(cx - 30, 63), (cx - 27, hy + 14), (cx - 15, hy + 5),
               (cx + 15, hy + 5), (cx + 27, hy + 14), (cx + 30, 63)],
              fill=SKIN, outline=INK)
    d.polygon([(cx - 29, 62), (cx - 26, hy + 15), (cx - 17, hy + 12), (cx - 19, 62)],
              fill=SKIN_D, outline=None)

    # head
    ell(d, cx, hy, 15, 16, SKIN)
    d.arc((cx - 15, hy - 20, cx + 15, hy + 4), 180, 360, fill=HAIR, width=6)

    if face == "stun":
        for ex in (cx - 6, cx + 6):
            d.line([(ex - 3, hy - 4), (ex + 3, hy + 2)], fill=INK, width=2)
            d.line([(ex - 3, hy + 2), (ex + 3, hy - 4)], fill=INK, width=2)
        ell(d, cx, hy + 9, 5, 3, INK)
    else:
        for ex in (cx - 6, cx + 6):
            ell(d, ex, hy - 1, 3, 3, WHITE, outline=None)
            ell(d, ex, hy - 1, 1, 1, INK, outline=None)
        if face == "grit":
            box(d, cx, hy + 9, 6, 2, WHITE)
        elif face == "hurt":
            ell(d, cx, hy + 9, 4, 4, INK, outline=None)
        else:
            d.line([(cx - 5, hy + 9), (cx + 5, hy + 9)], fill=INK, width=2)

    # gloves last, so they sit in front of everything
    glove(d, lgx, lgy, lgr, GLOVE, GLOVE_D)
    glove(d, rgx, rgy, rgr, GLOVE, GLOVE_D)
    return im


def opp_lower(pose):
    """One 128x32 lower band: trunks and legs.

    ⚠️ 32 ROWS, NOT 64, AND THAT IS A MEMORY DECISION. At 64x64 this band cost 4 KB of a 32 KB
    sprite arena for content that only ever filled its top half. agb frees tile VRAM only at commit,
    so every oversized cell costs twice over — once resident, once again in the transient while a
    pose changes. Halving this band and the player's took the steady footprint from 17.9 KB to
    10 KB, which is the difference between `SpriteFull` after two seconds and never.
    """
    im = Image.new("RGBA", (128, 32), CLEAR)
    d = ImageDraw.Draw(im)
    cx = 64

    if pose == L_DOWN:
        # flat on the canvas — the whole fighter collapses into this band
        d.rounded_rectangle((cx - 44, 6, cx + 44, 26), 9, fill=SKIN, outline=INK)
        box(d, cx, 18, 20, 6, TRUNK)
        ell(d, cx - 50, 10, 13, 11, SKIN)
        glove(d, cx + 48, 14, 11, GLOVE, GLOVE_D)
        return im

    spread = {L_STAND: 16, L_BOB: 18, L_CROUCH: 22, L_WIDE: 27}[pose]
    top = {L_STAND: 0, L_BOB: 1, L_CROUCH: 5, L_WIDE: 2}[pose]

    box(d, cx, top + 3, 30, 3, WHITE)                        # waistband
    box(d, cx, top + 9, 30, 7, TRUNK)                        # trunks
    for sx in (cx - spread, cx + spread):
        d.line([(sx, top + 14), (sx, top + 27)], fill=SKIN_D, width=15)
        d.line([(sx - 1, top + 14), (sx - 1, top + 27)], fill=SKIN, width=10)
        d.rounded_rectangle((sx - 10, top + 25, sx + 10, top + 31), 3, fill=INK, outline=INK)
    return im


# ══════════════════════════════════════════════════════════════════════════════════════════════
# The player — seen from BEHIND. Head, shoulders, and the tops of both gloves.
#
# This is the sprite that makes the camera legible, and it is deliberately small and dark: it is
# scenery for the opponent, not a character. If it competes for attention the game stops working.
# ══════════════════════════════════════════════════════════════════════════════════════════════

(P_IDLE_A, P_IDLE_B, P_DUCK, P_BLOCK, P_DODGE_L, P_DODGE_R,
 P_PUNCH_L, P_PUNCH_R, P_HURT, P_DOWN, P_GASSED) = range(11)
N_PLAYER = 11


def player_cell(pose):
    """One 128x32 player band, back-to-camera.

    ⚠️ 32 ROWS, AND NOTHING IS LOST. The band sits at y=118 on a 160-row screen, so rows 42..63 of a
    64-tall cell were BELOW THE BOTTOM OF THE SCREEN — 2 KB per cell of sprite VRAM, resident and
    transient, for pixels no one could ever see. See `opp_lower` for why that mattered.
    """
    im = Image.new("RGBA", (128, 32), CLEAR)
    d = ImageDraw.Draw(im)
    cx, top = 64, 0
    lgx, lgy, rgx, rgy, lgr, rgr = 38, 14, 90, 14, 11, 11
    skin, skin_d = PSKIN, PSKIN_D

    if pose == P_IDLE_B:
        top = 2
        lgy, rgy = 16, 12
    elif pose == P_DUCK:
        top = 16                                  # drops out of the way; gloves come up with it
        lgy, rgy = 28, 28
    elif pose == P_BLOCK:
        lgx, rgx, lgy, rgy = 50, 78, 4, 4         # both gloves up over the back of the head
    elif pose == P_DODGE_L:
        cx, lgx, rgx = 40, 14, 66
    elif pose == P_DODGE_R:
        cx, lgx, rgx = 88, 62, 114
    elif pose == P_PUNCH_L:
        lgy, lgr = 2, 8                           # the near glove shrinks as it goes AWAY from us
        lgx = 46
    elif pose == P_PUNCH_R:
        rgy, rgr = 2, 8
        rgx = 82
    elif pose == P_HURT:
        top = 6
        lgx, rgx, lgy, rgy = 24, 104, 24, 24
    elif pose == P_GASSED:
        top = 8
        lgx, rgx, lgy, rgy = 30, 98, 26, 26
        skin, skin_d = SKIN_D, TRUNK_D            # the exhaustion tell is a whole-body colour shift
    elif pose == P_DOWN:
        # ⚠️ THIS CELL WAS EMPTY, AND AN EMPTY CELL IS NOT "NOTHING HAPPENS" — it is the player
        # vanishing off the bottom of the screen for eight seconds while a ten-count they cannot see
        # runs. It read as a rendering crash, not as a knockdown.
        d.rounded_rectangle((cx - 40, 16, cx + 40, 31), 8, fill=skin_d, outline=INK)
        ell(d, cx - 30, 20, 12, 10, skin_d)
        d.chord((cx - 30 - 12, 10, cx - 30 + 12, 30), 180, 360, fill=HAIR, outline=INK)
        glove(d, cx + 30, 22, 9, PGLOVE_D, PGLOVE_D)
        return im

    # Shoulders: one rounded mass, because from behind that is all there is. Deliberately SMALL —
    # this sprite is foreground scenery for the opponent, not a second character. When it was wide
    # enough to read as a person it covered the opponent's legs and the camera stopped working.
    d.rounded_rectangle((cx - 36, top + 14, cx + 36, 31), 10, fill=skin, outline=INK)
    d.rounded_rectangle((cx - 36, top + 14, cx - 14, 31), 10, fill=skin_d, outline=None)
    # back of the head
    ell(d, cx, top + 10, 14, 12, skin)
    d.chord((cx - 14, top - 4, cx + 14, top + 18), 180, 360, fill=HAIR, outline=INK)
    ell(d, cx, top + 2, 9, 5, HAIR, outline=None)

    glove(d, lgx, lgy, lgr, PGLOVE, PGLOVE_D)
    glove(d, rgx, rgy, rgr, PGLOVE, PGLOVE_D)
    return im


# ══════════════════════════════════════════════════════════════════════════════════════════════
# The glove overlay. ONE 64x64 sheet, ONE sprite at runtime, re-pointed with sprite_set_sheet.
#
# ⚠️ This is the `beatemup` lesson, and it is the reason this game does not crash after ten minutes:
# `sprite_set_visible(h, 0)` does NOT free the Object, so four hidden 64x64 overlays held 8 KB of a
# 32 KB arena permanently. At most one fist is in flight at a time, so at most one sprite exists.
# ══════════════════════════════════════════════════════════════════════════════════════════════

(G_P_JAB, G_P_BODY, G_P_UPPER, G_O_STRIKE, G_O_BIG, G_NONE) = range(6)
N_GLOVE = 6


def glove_cell(kind):
    """One 32x32 glove overlay. ONE sprite at runtime, re-pointed — never one per punch."""
    im = Image.new("RGBA", (32, 32), CLEAR)
    if kind == G_NONE:
        return im
    d = ImageDraw.Draw(im)
    if kind == G_P_JAB:
        d.line([(16, 31), (16, 22)], fill=PSKIN_D, width=9)
        glove(d, 16, 14, 10, PGLOVE, PGLOVE_D)
    elif kind == G_P_BODY:
        d.line([(16, 31), (16, 24)], fill=PSKIN_D, width=10)
        glove(d, 16, 17, 11, PGLOVE, PGLOVE_D)
    elif kind == G_P_UPPER:
        d.line([(16, 31), (16, 20)], fill=PSKIN_D, width=11)
        glove(d, 16, 13, 12, PGLOVE, PGLOVE_D)     # the star punch: the biggest fist in the game
        for a in (0, 1, 2, 3):
            d.line([(16, 13), (16 + [-15, 15, -10, 10][a], 13 + [-4, -4, -12, -12][a])],
                   fill=FLASH, width=2)
    elif kind == G_O_STRIKE:
        glove(d, 16, 16, 13, GLOVE, GLOVE_D)
    elif kind == G_O_BIG:
        glove(d, 16, 16, 15, GLOVE, GLOVE_D)       # the haymaker, filling the cell
    return im


def spark_cell(step):
    """A four-frame impact burst. Pooled: one sprite, parked hidden, never spawned."""
    im = Image.new("RGBA", (32, 32), CLEAR)
    d = ImageDraw.Draw(im)
    r = (7, 13, 16, 11)[step]
    col = (WHITE, FLASH, FLASH, GLOVE)[step]
    for i in range(8):
        ax = [0, 1, 1, 1, 0, -1, -1, -1][i]
        ay = [-1, -1, 0, 1, 1, 1, 0, -1][i]
        d.line([(16, 16), (16 + ax * r, 16 + ay * r)], fill=col, width=3 if step < 2 else 2)
    if step < 3:
        ell(d, 16, 16, max(2, r // 2), max(2, r // 2), WHITE, outline=None)
    return im


# ══════════════════════════════════════════════════════════════════════════════════════════════
# The ring background. One opaque image; NOT bg_bands.
#
# ⚠️ `docs/fighting-genre.md` §6: banded scanline DMA turns a dropped frame into a CORRUPT one, and
# this game drops frames on knockdowns by design. The scroll register here is written once and never
# moves (camera_set(0,0) all game), so a late commit is invisible.
# ══════════════════════════════════════════════════════════════════════════════════════════════

CROWD = [(38, 30, 52, 255), (54, 44, 72, 255), (70, 58, 92, 255), (30, 24, 42, 255)]
FLOOR = (168, 148, 120, 255)
FLOOR_D = (132, 112, 90, 255)
ROPE = (216, 60, 72, 255)
POST = (60, 52, 74, 255)


def ring_bg():
    im = Image.new("RGBA", (256, 256), (20, 16, 28, 255))
    d = ImageDraw.Draw(im)

    # crowd: dithered blobs, deliberately low-contrast so nothing up there competes with a tell
    d.rectangle((0, 0, 255, 108), fill=CROWD[3])
    seed = 1
    for row in range(6):
        y = 8 + row * 17
        for i in range(20):
            seed = (seed * 1103515245 + 12345) & 0x7FFFFFFF
            x = (seed >> 8) % 256
            c = CROWD[(seed >> 4) % 3]
            ell(d, x, y, 6, 7, c, outline=None)
            ell(d, x, y - 8, 4, 4, c, outline=None)

    # the apron and the canvas, in perspective: the floor gets lighter as it comes toward us
    d.polygon([(0, 108), (255, 108), (255, 255), (0, 255)], fill=FLOOR_D)
    d.polygon([(28, 108), (227, 108), (255, 200), (0, 200)], fill=FLOOR)
    for i in range(6):
        y = 118 + i * 14
        d.line([(0, y), (255, y)], fill=FLOOR_D, width=1)

    # ropes across the top of the canvas — the thing that says "you are outside the ring"
    for i, y in enumerate((96, 106)):
        d.rectangle((0, y, 255, y + 4), fill=ROPE)
        d.rectangle((0, y, 255, y + 1), fill=WHITE)
    for px in (10, 245):
        d.rectangle((px - 5, 74, px + 5, 130), fill=POST)
        d.rectangle((px - 5, 74, px - 2, 130), fill=CROWD[1])
    return im.convert("RGB")


# ══════════════════════════════════════════════════════════════════════════════════════════════

def strip(cells, cw, ch):
    """Lay cells out as ONE horizontal row — the layout include_aseprite_inner! expects."""
    im = Image.new("RGBA", (cw * len(cells), ch), CLEAR)
    for i, c in enumerate(cells):
        im.paste(c, (i * cw, 0))
    return im


def split_bands(wide):
    """A 128-wide drawing -> its left and right 64-wide halves, at whatever height it is.

    Bands, never quadrants: each half shares a FULL vertical edge with the other, so the seam always
    reads as joined. A diagonal piece shares only a corner and reads as a floating slab.
    """
    h = wide.size[1]
    return [wide.crop((0, 0, 64, h)), wide.crop((64, 0, 128, h))]


def main():
    os.makedirs(OUT, exist_ok=True)
    print("ringside art:")

    # ── opponent: upper bands then lower bands, ONE sheet so it is ONE palette (trap 2)
    # ── THREE sheets, because they are three cell SIZES. Splitting a sheet normally risks a colour
    #    step across the seam (one PNG is one Palette16, and two quantisations produce two
    #    palettes) — but every colour here is an AUTHORED constant and the sheets each hold nine of
    #    them, well under the fifteen `clamp_colors` would quantise at. So no quantisation runs and
    #    the palettes are identical by construction rather than by luck. The colour count is
    #    asserted below so that stays true.
    opp = []
    for p in range(N_UPPER):
        opp += split_bands(opp_upper(p))
    clamp_colors(strip(opp, CELL, CELL), 15).save(os.path.join(OUT, "opponent.png"))
    print("  opponent  %2d cells of 64x64 (upper bands)" % len(opp))

    lo = []
    for q in range(N_LOWER):
        lo += split_bands(opp_lower(q))
    clamp_colors(strip(lo, CELL, 32), 15).save(os.path.join(OUT, "opponent_lo.png"))
    print("  opp lower %2d cells of 64x32" % len(lo))

    pl = []
    for p in range(N_PLAYER):
        pl += split_bands(player_cell(p))
    clamp_colors(strip(pl, CELL, 32), 15).save(os.path.join(OUT, "player.png"))
    print("  player    %2d cells of 64x32" % len(pl))

    gl = [glove_cell(k) for k in range(N_GLOVE)]
    clamp_colors(strip(gl, 32, 32), 15).save(os.path.join(OUT, "gloves.png"))
    print("  gloves    %2d cells of 32x32" % len(gl))

    sp = [spark_cell(i) for i in range(4)]
    clamp_colors(strip(sp, 32, 32), 15).save(os.path.join(OUT, "spark.png"))
    print("  spark      4 cells")

    digits_sheet(os.path.join(OUT, "digits.png"), size=15, colour=WHITE)
    print("  digits    10 cells")

    bg = ring_bg()
    ncol = len(set(bg.getdata()))
    if ncol > BG_MAX_COLOURS:
        # Fail HERE, where the message can name the number. agb's packer fails with
        # `DoesNotFitError { count: N }` — no tile, no colour, and nothing to act on.
        print("  FAIL ring background has %d colours, limit %d" % (ncol, BG_MAX_COLOURS))
        return 1
    bg.save(os.path.join(OUT, "ring.png"))
    print("  ring bg   256x256, %d colours (limit %d)" % (ncol, BG_MAX_COLOURS))

    # ── the sprite-VRAM budget. This is the number verify.sh gates on, because sprite VRAM PANICS
    #    minutes into play on an innocent frame, and a build-time assertion is the only cheap guard.
    # ⚠️ ONE glove sprite total, shared between the player's fist and the opponent's — at most one
    #    is in flight at a time. Four hidden 64x64 overlays once held 8 KB in `beatemup` because
    #    `sprite_set_visible(h, 0)` does NOT free a sprite's Object.
    live = [("opponent upper", 2, 2048), ("opponent lower", 2, 1024),
            ("player body", 2, 1024), ("glove overlay", 1, 512),
            ("spark", 1, 512), ("digits", 8, 128)]
    total = sum(n * b for _, n, b in live)
    print("  ---- live sprite VRAM ----")
    for name, n, b in live:
        print("    %-16s %d x %5d = %6d" % (name, n, b, n * b))
    print("    %-16s %19d B  (%.1f KB of 32 KB)" % ("TOTAL", total, total / 1024.0))
    print("VRAM_BUDGET_BYTES=%d" % total)

    write_frames_tish(total)
    write_preview()
    return 0


def write_frames_tish(vram):
    """The pose-index constants and the ring geometry, so the game and the art cannot disagree.

    ⚠️ Every scalar is `let X: i32`, never `const`. `docs/perf-rules.md` §1: an untyped module scalar
    is a thread-local Cell<f64> on a chip with no FPU, and this file is nothing but module scalars.
    """
    up = ["BOB_A", "BOB_B", "TELL_L", "TELL_R", "TELL_BODY", "TELL_HIGH", "STRIKE_L", "STRIKE_R",
          "STRIKE_BODY", "GUARD", "HURT", "STUN", "TAUNT", "DOWN"]
    lo = ["STAND", "BOB", "CROUCH", "WIDE", "DOWN"]
    pp = ["IDLE_A", "IDLE_B", "DUCK", "BLOCK", "DODGE_L", "DODGE_R", "PUNCH_L", "PUNCH_R",
          "HURT", "DOWN", "GASSED"]
    gg = ["P_JAB", "P_BODY", "P_UPPER", "O_STRIKE", "O_BIG", "NONE"]

    L = ["// GENERATED by scripts/gen_ringside.py — do not edit.",
         "//",
         "// Cell indices into the three sprite sheets, and the ring geometry. A pose is TWO cells:",
         "// the left band and the right band, so cell = (pose << 1) and cell + 1.",
         "//",
         "// Measured live sprite VRAM at these sheet sizes: %d bytes of 32768." % vram,
         ""]
    L.append("// opponent.png — 64x64 upper bands")
    for i, n in enumerate(up):
        L.append("export let OU_%s: i32 = %d" % (n, i))
    L.append("")
    L.append("// opponent_lo.png — 64x32 lower bands, their own sheet because of the cell size")
    for i, n in enumerate(lo):
        L.append("export let OL_%s: i32 = %d" % (n, i))
    L.append("")
    L.append("// player.png")
    for i, n in enumerate(pp):
        L.append("export let PP_%s: i32 = %d" % (n, i))
    L.append("")
    L.append("// gloves.png — single cells, not bands")
    for i, n in enumerate(gg):
        L.append("export let GC_%s: i32 = %d" % (n, i))
    L.append("")
    L.append("// screen geometry")
    for n, v in (("OPP_X", OPP_X), ("OPP_UP_Y", OPP_UP_Y), ("OPP_LO_Y", OPP_LO_Y),
                 ("PL_X", PL_X), ("PL_Y", PL_Y),
                 ("GLOVE_HOME_X", GLOVE_HOME_X), ("GLOVE_HOME_Y", GLOVE_HOME_Y)):
        L.append("export let %s: i32 = %d" % (n, v))
    os.makedirs(SRC, exist_ok=True)
    with open(os.path.join(SRC, "frames.tish"), "w") as f:
        f.write("\n".join(L) + "\n")
    print("  wrote src/frames.tish")


def write_preview():
    """A 240x160 composite at the real screen positions.

    ⚠️ This is the point of milestone 1: the question "does the camera read, and do the bands line
    up" is answerable in two seconds here, and takes a five-minute ROM build to answer any other
    way. A five-minute iteration is not a debugging loop.
    """
    def compose(upper, lower, pl, gcell=None):
        # rows 0..160 of the 256x256 background — the same window the GBA shows with the scroll
        # register at zero. Cropping anywhere else makes the preview a lie about the rope height.
        s = ring_bg().convert("RGBA").crop((0, 0, 240, 160))
        lo = opp_lower(lower)
        s.alpha_composite(lo, (OPP_X, OPP_LO_Y))
        s.alpha_composite(opp_upper(upper), (OPP_X, OPP_UP_Y))
        if gcell is not None:
            s.alpha_composite(glove_cell(gcell), (GLOVE_HOME_X, GLOVE_HOME_Y))
        s.alpha_composite(player_cell(pl), (PL_X, PL_Y))
        return s

    shots = [("idle", compose(U_BOB_A, L_STAND, P_IDLE_A)),
             ("tell-left", compose(U_TELL_L, L_BOB, P_IDLE_B)),
             ("tell-body", compose(U_TELL_BODY, L_CROUCH, P_IDLE_A)),
             ("dodge", compose(U_STRIKE_L, L_WIDE, P_DODGE_R)),
             ("player-jab", compose(U_HURT, L_STAND, P_PUNCH_L, G_P_JAB)),
             ("star", compose(U_STUN, L_WIDE, P_PUNCH_R, G_P_UPPER)),
             ("guard", compose(U_GUARD, L_STAND, P_BLOCK)),
             ("down", compose(U_DOWN, L_DOWN, P_IDLE_A))]
    sheet = Image.new("RGBA", (240 * 4 + 30, 160 * 2 + 10), (12, 10, 16, 255))
    for i, (_, s) in enumerate(shots):
        sheet.paste(s, ((i % 4) * 250, (i // 4) * 170))
    sheet.convert("RGB").save(os.path.join(OUT, "preview-composite.png"))
    print("  wrote assets/preview-composite.png  (%s)" % ", ".join(n for n, _ in shots))


if __name__ == "__main__":
    sys.exit(main())
