#!/usr/bin/env python3
"""Generate the `shmup` example's ROM art — all procedural, no external source pixels.

Everything a top-down shoot-'em-up needs, drawn with hard-edged PIL primitives (no anti-aliasing, so
each shape lands on exact palette colours a 4bpp GBA sprite/background can hold):

  ships16.png   sheet16 — 16 × 16px frames: player ship (+ 2 banks), player/enemy bullets, grunt (2),
                weaver, shooter, weapon + shield power-ups, a 4-frame explosion.
  boss32.png    sheet32 — 2 × 32px frames: the boss battlecruiser + its hit-flash.
  stars_far.png / stars_near.png   256×256 tileable parallax star layers (opaque; scroll + wrap).
  title-bg.png  240×160 nebula backdrop for the title screen (opaque).

Sprite sheets keep ≤15 non-transparent colours (one 16-colour palette bank); backgrounds are opaque
(the `background:` importer blanks any alpha). Re-run after editing; output → examples/shmup/assets.
"""
import os
import random
from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(__file__), "..", "examples", "shmup", "assets")
os.makedirs(OUT, exist_ok=True)

# ── Sprite palette (RGBA); index 0 is transparent. 14 colours + transparent = 15. ──
T = (0, 0, 0, 0)
WHITE = (240, 246, 255, 255)
CYAN = (96, 208, 236, 255)
BLUE = (56, 120, 200, 255)
NAVY = (30, 54, 110, 255)
YEL = (250, 220, 70, 255)
ORA = (245, 150, 45, 255)
RED = (226, 60, 55, 255)
GRN = (96, 206, 110, 255)
DGRN = (40, 120, 70, 255)
PUR = (172, 96, 216, 255)
DPUR = (96, 50, 140, 255)
MAG = (240, 90, 190, 255)
GREY = (150, 165, 185, 255)


def tile(size=16):
    img = Image.new("RGBA", (size, size), T)
    return img, ImageDraw.Draw(img)


def player_ship(bank=0):
    """A cyan interceptor pointing up. bank -1/0/+1 rolls the wings for a lean."""
    img, d = tile()
    # hull
    d.polygon([(8, 1), (10, 6), (10, 11), (6, 11), (6, 6)], fill=CYAN)
    d.polygon([(8, 1), (9, 6), (9, 11), (8, 11)], fill=WHITE)  # nose highlight streak
    # wings (rolled by bank)
    lw = 12 + bank      # left wingtip y
    rw = 12 - bank      # right wingtip y
    d.polygon([(6, 7), (1, lw), (3, lw), (6, 10)], fill=BLUE)
    d.polygon([(10, 7), (15, rw), (13, rw), (10, 10)], fill=BLUE)
    d.point([(1, lw), (15, rw)], fill=NAVY)   # wingtip lights
    # cockpit
    d.ellipse([7, 3, 9, 6], fill=NAVY)
    d.point([(8, 4)], fill=WHITE)
    # engine flame
    d.rectangle([7, 11, 8, 12], fill=ORA)
    d.point([(7, 13), (8, 13)], fill=YEL)
    return img


def player_bullet():
    img, d = tile()
    d.rectangle([7, 3, 8, 12], fill=YEL)
    d.rectangle([7, 3, 8, 7], fill=WHITE)
    d.point([(6, 6), (9, 6)], fill=YEL)
    return img


def enemy_bullet():
    img, d = tile()
    d.ellipse([5, 5, 10, 10], fill=MAG)
    d.ellipse([6, 6, 8, 8], fill=WHITE)
    return img


def grunt(flap=0):
    """Green fighter pointing DOWN (toward the player). flap raises/lowers the wingtips."""
    img, d = tile()
    d.polygon([(8, 14), (11, 9), (11, 4), (5, 4), (5, 9)], fill=GRN)
    d.polygon([(8, 14), (9, 9), (9, 5), (7, 5), (7, 9)], fill=DGRN)  # spine
    wt = 3 + flap
    d.polygon([(5, 7), (1, wt + 2), (3, wt + 2), (5, 9)], fill=DGRN)
    d.polygon([(11, 7), (15, wt + 2), (13, wt + 2), (11, 9)], fill=DGRN)
    d.ellipse([7, 5, 9, 8], fill=RED)   # eye/core
    d.point([(8, 6)], fill=WHITE)
    return img


def weaver():
    """Purple saucer that weaves side to side."""
    img, d = tile()
    d.ellipse([2, 6, 14, 11], fill=DPUR)
    d.ellipse([3, 6, 13, 9], fill=PUR)
    d.ellipse([6, 4, 10, 8], fill=CYAN)   # dome
    d.point([(8, 6)], fill=WHITE)
    d.point([(3, 10), (8, 11), (12, 10)], fill=MAG)   # underlights
    return img


def shooter():
    """Heavier orange gunship with twin cannons and a red core."""
    img, d = tile()
    d.polygon([(3, 5), (13, 5), (15, 9), (11, 13), (5, 13), (1, 9)], fill=ORA)
    d.ellipse([6, 6, 10, 11], fill=RED)
    d.ellipse([7, 7, 9, 9], fill=YEL)
    d.rectangle([3, 12, 4, 15], fill=NAVY)   # cannons
    d.rectangle([11, 12, 12, 15], fill=NAVY)
    d.point([(4, 6), (11, 6)], fill=WHITE)
    return img


def powerup_weapon():
    """Gold orb with an up-arrow — a weapon upgrade."""
    img, d = tile()
    d.ellipse([2, 2, 13, 13], fill=ORA)
    d.ellipse([3, 3, 12, 12], fill=YEL)
    d.polygon([(8, 4), (11, 8), (9, 8), (9, 11), (6, 11), (6, 8), (4, 8)], fill=WHITE)
    return img


def powerup_shield():
    """Blue orb with a cross — a shield / bomb pickup."""
    img, d = tile()
    d.ellipse([2, 2, 13, 13], fill=BLUE)
    d.ellipse([3, 3, 12, 12], fill=CYAN)
    d.rectangle([7, 4, 8, 11], fill=WHITE)
    d.rectangle([4, 7, 11, 8], fill=WHITE)
    return img


def explosion(stage):
    img, d = tile()
    if stage == 0:
        d.ellipse([6, 6, 9, 9], fill=WHITE)
        d.ellipse([5, 5, 10, 10], outline=YEL)
    elif stage == 1:
        d.ellipse([3, 3, 12, 12], fill=ORA)
        d.ellipse([5, 5, 10, 10], fill=YEL)
        d.ellipse([7, 7, 8, 8], fill=WHITE)
    elif stage == 2:
        d.ellipse([1, 1, 14, 14], fill=RED)
        d.ellipse([4, 4, 11, 11], fill=ORA)
        d.ellipse([6, 6, 9, 9], fill=YEL)
    else:
        d.ellipse([0, 0, 15, 15], outline=RED)
        d.ellipse([3, 3, 12, 12], outline=ORA)
        for p in [(2, 8), (13, 7), (8, 1), (7, 14), (5, 4), (11, 12)]:
            d.point([p], fill=RED)
    return img


def build_ships16():
    frames = [
        player_ship(0), player_ship(-1), player_ship(1),   # 0,1,2
        player_bullet(), enemy_bullet(),                    # 3,4
        grunt(0), grunt(1),                                 # 5,6
        weaver(), shooter(),                                # 7,8
        powerup_weapon(), powerup_shield(),                 # 9,10  (kept before explosion)
        explosion(0), explosion(1), explosion(2), explosion(3),  # 11,12,13,14
        tile()[0],                                          # 15 spare (blank)
    ]
    sheet = Image.new("RGBA", (16 * len(frames), 16), T)
    for i, f in enumerate(frames):
        sheet.paste(f, (i * 16, 0))
    _assert_palette(sheet, 16, "ships16")
    sheet.save(os.path.join(OUT, "ships16.png"))
    return {
        "SHIP": 0, "SHIP_L": 1, "SHIP_R": 2, "PSHOT": 3, "ESHOT": 4,
        "GRUNT": 5, "GRUNT2": 6, "WEAVER": 7, "SHOOTER": 8,
        "PW_GUN": 9, "PW_SHIELD": 10, "BOOM": 11,
    }


# ── Boss (own 32px palette bank) ──
BS = (150, 160, 175, 255)
BSD = (95, 105, 125, 255)
BSDD = (55, 62, 80, 255)
BWHITE = (240, 246, 255, 255)
BRED = (230, 60, 55, 255)
BDRED = (140, 30, 40, 255)
BYEL = (250, 220, 70, 255)
BBLUE = (70, 150, 230, 255)
BCYAN = (120, 210, 240, 255)


def boss(flash=False):
    img = Image.new("RGBA", (32, 32), T)
    d = ImageDraw.Draw(img)
    body = BWHITE if flash else BS
    shade = BWHITE if flash else BSD
    dark = BWHITE if flash else BSDD
    # main hull — a wide battlecruiser pointing down
    d.polygon([(4, 2), (28, 2), (31, 12), (24, 24), (16, 30), (8, 24), (1, 12)], fill=body)
    d.polygon([(8, 4), (24, 4), (26, 12), (20, 20), (16, 24), (12, 20), (6, 12)], fill=shade)
    # armour ridges
    d.rectangle([2, 8, 30, 10], fill=dark)
    d.line([(16, 3), (16, 29)], fill=dark)
    # side gun pods
    d.ellipse([0, 10, 6, 18], fill=body)
    d.ellipse([26, 10, 32, 18], fill=body)
    d.rectangle([2, 17, 3, 22], fill=BDRED)
    d.rectangle([29, 17, 30, 22], fill=BDRED)
    # core cannon
    d.ellipse([11, 11, 21, 21], fill=BDRED)
    d.ellipse([13, 13, 19, 19], fill=BRED)
    d.ellipse([15, 15, 17, 17], fill=BYEL if not flash else BWHITE)
    # engine lights
    for x in (10, 16, 22):
        d.point([(x, 3), (x, 4)], fill=BCYAN)
    d.point([(6, 6), (26, 6)], fill=BBLUE)
    return img


def build_boss32():
    frames = [boss(False), boss(True)]
    sheet = Image.new("RGBA", (32 * len(frames), 32), T)
    for i, f in enumerate(frames):
        sheet.paste(f, (i * 32, 0))
    _assert_palette(sheet, 16, "boss32")
    sheet.save(os.path.join(OUT, "boss32.png"))


# ── Parallax starfields (opaque, 256×256, tileable — wrap seamlessly when scrolled) ──
def starfield(name, seed, count, colors, big_ratio):
    rnd = random.Random(seed)
    img = Image.new("RGB", (256, 256), (6, 8, 20))
    d = ImageDraw.Draw(img)
    for _ in range(count):
        x = rnd.randrange(256)
        y = rnd.randrange(256)
        c = rnd.choice(colors)
        if rnd.random() < big_ratio:
            d.rectangle([x, y, (x + 1) % 256 or x + 1, y + 1], fill=c)
        else:
            d.point([(x, y)], fill=c)
    img.save(os.path.join(OUT, name))


def build_stars():
    # Far layer: dim, sparse, cool — scrolls slowly.
    starfield("stars_far.png", 1, 120,
              [(60, 70, 110), (80, 90, 130), (100, 110, 150)], big_ratio=0.0)
    # Near layer: brighter, some 2px, warmer — scrolls fast.
    starfield("stars_near.png", 2, 90,
              [(210, 220, 245), (240, 246, 255), (150, 205, 235), (235, 220, 160)],
              big_ratio=0.35)


# ── Title backdrop (opaque nebula + stars + a ringed planet) ──
def build_title_bg():
    rnd = random.Random(7)
    img = Image.new("RGB", (240, 160), (10, 8, 26))
    d = ImageDraw.Draw(img)
    # banded vertical nebula (few colours to stay 4bpp-friendly)
    bands = [(18, 12, 44), (26, 14, 58), (16, 10, 40), (10, 8, 26)]
    for i, col in enumerate(bands):
        y0 = i * 40
        d.rectangle([0, y0, 240, y0 + 40], fill=col)
    # soft nebula blobs
    for cx, cy, r, col in [(60, 60, 46, (40, 22, 74)), (180, 100, 54, (30, 18, 66)),
                           (120, 30, 40, (46, 26, 82))]:
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=col)
    # stars
    for _ in range(140):
        x, y = rnd.randrange(240), rnd.randrange(160)
        c = rnd.choice([(120, 130, 170), (200, 210, 240), (245, 246, 255)])
        d.point([(x, y)], fill=c)
    # ringed planet, lower-right
    d.ellipse([182, 108, 226, 152], fill=(70, 60, 120))
    d.ellipse([182, 108, 226, 152], outline=(120, 110, 180))
    d.arc([168, 118, 240, 142], start=200, end=360, fill=(150, 140, 205))
    d.arc([168, 118, 240, 142], start=0, end=20, fill=(150, 140, 205))
    img = img.quantize(colors=15, method=Image.MEDIANCUT).convert("RGB")
    img.save(os.path.join(OUT, "title-bg.png"))


def _assert_palette(img, limit, name):
    cols = {px for px in img.getdata() if px[3] != 0}
    assert len(cols) <= limit - 1, f"{name}: {len(cols)} opaque colours (>{limit - 1})"
    print(f"  {name}: {len(cols)} opaque colours")


if __name__ == "__main__":
    print("generating shmup art →", os.path.normpath(OUT))
    ids = build_ships16()
    build_boss32()
    build_stars()
    build_title_bg()
    print("frame ids:", ids)
    print("done.")
