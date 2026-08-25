#!/usr/bin/env python3
"""Art prep for the RAP DOJO example — a call-and-response rhythm game on the Ninja Adventure pack.

Run from the repo root:  python3 scripts/gen_rap_dojo.py

Emits into examples/rap-dojo/assets/:
  student.png  288x32  — 9 frames of 32x32 (`sheet32:`), the NinjaGreen pupil, one pose per button
  master.png   576x64  — 9 frames of 64x64 (`sheet64:`), the GiantBamboo teacher
  icons.png    320x16  — 20 frames of 16x16 (`sheet:`), button prompts; frame index == BUTTON CODE,
                         and code+10 is the lit variant, so drawing is `sprite_set_frame(s, btn)`
  stage.png    256x256 — the backdrop (`background:`), drawn for per-scanline banding

SHEETS ARE SINGLE-ROW STRIPS. agb also accepts a grid (akari packs 11x4 and indexes row-major), but
a strip makes "frame index == position" true by construction, and every frame index in this game is
either a button code or a small enum — worth more than the pixels a grid would save.

FRAME LAYOUT OF THE SOURCE ART — verified, not assumed (the same traps gen_akari.py documents):
  * NinjaGreen `Separate/*.png` are COLUMN = direction (down/up/left/right), ROW = frame. Transpose.
  * The combined NinjaGreen `SpriteSheet.png` is NOT 16px-aligned — never slice it.
  * Boss art is the other convention: `Actor/Boss/*/*.png` are single horizontal strips, one
    direction only. Their frame width is NOT reliably the sheet height — GiantRedSamurai's
    Idle.png is 576x48 but its stride is 96, not 48, and slicing it at 48 halves every figure.
    GiantBamboo genuinely is square (372x62 = 6 frames of 62), which is why it is the master here:
    at 62px it also fits the 64x64 hardware sprite ceiling that the samurai's 70px-wide sword
    (body plus blade) does not.
  * `Ui/Input/Gamepad/Button{Up,Down,Left,Right}.png` are 17x17, not 16 — they get cropped.

THE STAGE, AND WHY IT IS DRAWN THE WAY IT IS
Parappa's look is a flat character posed in a scene with depth. There is no affine/Mode-7 background
API in tish-agb (it existed briefly and was removed in `7f6f969`), so the depth here comes from
per-scanline horizontal scrolling — `bg_bands`, one layer, one DMA channel.

The floor is split into depth bands. For a band at depth d, a real perspective projection makes both
the on-screen tile width and the on-screen scroll speed proportional to 1/d. So each band down the
screen gets a WIDER tile and a FASTER multiplier, in step. Get one without the other and the floor
reads as either a sliding flat texture or a static perspective painting; together they read as
ground going past.

The widths are 8/16/32/64 because the hardware wraps a background every 256px and those are the
divisors of 256 — a width of, say, 24 would leave a seam that walks across the screen once per lap.
BANDS in rap-dojo's main.tish must stay in step with FLOOR_BANDS below.
"""
import math
import os

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
OUT = os.path.join(ROOT, "examples", "rap-dojo", "assets")
EX_SRC = os.path.join(ROOT, "examples", "rap-dojo", "src")
os.makedirs(OUT, exist_ok=True)
os.makedirs(EX_SRC, exist_ok=True)

CEL = 32          # student cells  (sheet32:)
MCEL = 64         # master cells   (sheet64:) — 48px boss art centred with headroom
ICEL = 16         # icon cells     (sheet:)

# Button codes, straight from tish-agb's `button_of`. The icon strip is indexed BY THESE, so a
# prompt for button b is frame b and its lit twin is frame b + 10.
BTN_A, BTN_B, BTN_L, BTN_R = 0, 1, 4, 5
BTN_UP, BTN_DOWN, BTN_LEFT, BTN_RIGHT = 6, 7, 8, 9


def load(rel):
    return Image.open(os.path.join(PACK, rel)).convert("RGBA")


def clamp_colors(img, maxc=15):
    """Quantize to <= maxc opaque colours — the GBA 4bpp sprite budget (index 0 is transparent)."""
    img = img.convert("RGBA")
    opaque = {(r, g, b) for (r, g, b, a) in img.getdata() if a > 0}
    if len(opaque) <= maxc:
        return img
    alpha = img.getchannel("A")
    rgb = img.convert("RGB").quantize(colors=maxc, method=Image.MEDIANCUT).convert("RGBA")
    rgb.putalpha(alpha)
    return rgb


def cell(src, col, row, w, h):
    """One cell of a grid sheet, clamped so a layout surprise crops instead of throwing."""
    cols = max(1, src.width // w)
    rows = max(1, src.height // h)
    col = min(col, cols - 1)
    row = min(row, rows - 1)
    return src.crop((col * w, row * h, col * w + w, row * h + h))


def centred(art, size):
    c = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    c.paste(art, ((size - art.width) // 2, (size - art.height) // 2), art)
    return c


def strip(frames, size):
    """Pack frames left-to-right into a single-row strip of `size` cells."""
    out = Image.new("RGBA", (len(frames) * size, size), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        out.paste(centred(f, size), (i * size, 0))
    return out


# ── The student: NinjaGreen, one pose per button ─────────────────────────────────────────────────
# Frame order is the game's POSE enum (see rap-dojo/src/main.tish), not the button codes: the poses
# are looked up through a table there because two different buttons can share a pose.
def make_student():
    ng = "Actor/CharacterAnimated/NinjaGreen/Separate"
    idle, atk, push = load(ng + "/Idle.png"), load(ng + "/Attack.png"), load(ng + "/Push.png")
    roll, hit, item = load(ng + "/Roll.png"), load(ng + "/Hit.png"), load(ng + "/Item.png")
    # COLUMN = direction (0 down, 1 up, 2 left, 3 right), ROW = frame.
    DN, UP, LF, RT = 0, 1, 2, 3
    frames = [
        cell(idle, DN, 0, CEL, CEL),   # 0 idle
        cell(atk, DN, 2, CEL, CEL),    # 1 punch forward   (A)
        cell(push, DN, 1, CEL, CEL),   # 2 hands out       (B)
        cell(atk, UP, 2, CEL, CEL),    # 3 reach up        (Up)
        cell(roll, DN, 1, CEL, CEL),   # 4 duck            (Down)
        cell(atk, LF, 2, CEL, CEL),    # 5 swing left      (Left)
        cell(atk, RT, 2, CEL, CEL),    # 6 swing right     (Right)
        cell(hit, DN, 0, CEL, CEL),    # 7 stumble         (miss)
        cell(item, DN, 0, CEL, CEL),   # 8 celebrate
    ]
    clamp_colors(strip(frames, CEL)).save(os.path.join(OUT, "student.png"))


# ── The master: GiantBamboo, 62x62 boss art in 64px cells ────────────────────────────────────────
def make_master():
    gs = "Actor/Boss/GiantBamboo"
    idle, walk = load(gs + "/Idle.png"), load(gs + "/Walk.png")
    atk, hit = load(gs + "/Attack.png"), load(gs + "/Hit.png")
    BCEL = 62   # square, and confirmed by dividing each sheet's width by its frame count

    def f(src, i):
        return cell(src, i, 0, BCEL, BCEL)

    frames = [
        f(idle, 0), f(idle, 3),        # 0,1 bob      — the two-frame idle sway
        f(walk, 2), f(walk, 6),        # 2,3 call     — arms thrown on a syllable
        f(atk, 1), f(atk, 3),          # 4,5 accent   — these two glow at the crown; they land on
                                       #     the first syllable of a phrase so the bar has a downbeat
                                       #     you can see as well as hear
        f(walk, 10),                   # 6 pleased
        f(hit, 1),                     # 7 annoyed    — the whole stalk flushes red
        f(idle, 4),                    # 8 listening  — held while the pupil answers
    ]
    clamp_colors(strip(frames, MCEL)).save(os.path.join(OUT, "master.png"))


# ── Button prompts: frame index == button code, +10 == lit ───────────────────────────────────────
#
# `Ui/Input/Gamepad/Button{Up,Down,Left,Right}.png` are NOT arrows — each is a 17x17 picture of the
# whole d-pad with one circle tinted. That reads at a glance in a settings screen and reads as four
# smudges in a 16px icon flying past on the beat, so the direction prompts are built instead from
# `Ui/Arrow.png` (the pack's own 13x13 pointer) rotated four ways, keeping the pack's colour coding
# for each direction. A and B keep their real button art, which is already a legible letter.
#
# Every icon is flattened to two tones. That is a palette decision, not a style one: a sprite sheet
# gets ONE 16-colour bank, and the first version of this function brightened each icon to make its
# lit twin — twenty frames of subtly different colours, which blew the budget and quantised the
# whole strip to mush. The lit twin is now a white fill, so it costs one shared colour, and the
# assert at the end makes a future overrun a build failure rather than a muddy screenshot.
DIR_COLOR = {
    BTN_UP: (255, 173, 93, 255),      # the tint the pack itself uses for each d-pad direction
    BTN_DOWN: (116, 163, 52, 255),
    BTN_LEFT: (121, 184, 206, 255),
    BTN_RIGHT: (224, 57, 76, 255),
}
INK = (19, 27, 27, 255)               # the pack's near-black outline
LIT = (255, 255, 255, 255)


def two_tone(mask, fill):
    """Redraw a shape as a 1px `INK` border around a flat `fill` interior."""
    w, h = mask.size
    m = mask.load()
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    o = out.load()
    for y in range(h):
        for x in range(w):
            if not m[x, y]:
                continue
            edge = False
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                nx, ny = x + dx, y + dy
                if nx < 0 or ny < 0 or nx >= w or ny >= h or not m[nx, ny]:
                    edge = True
                    break
            o[x, y] = INK if edge else fill
    return out


def arrow_mask():
    """A chunky arrow pointing UP, filling a 16px cell.

    Drawn rather than taken from `Ui/Arrow.png`: that asset LOOKS like an arrow because of its
    internal shading — light face, dark underside — but its silhouette is a rounded blob, so
    lifting its alpha as a shape yields an octagon. Nothing else in the catalog is a plain
    directional glyph at this size (the Button{Up,…} art is a d-pad diagram, see above), and a
    rhythm bar needs the direction readable in the quarter-second it is on screen.
    """
    m = Image.new("L", (ICEL, ICEL), 0)
    d = ImageDraw.Draw(m)
    d.polygon([(8, 1), (15, 9), (11, 9), (11, 15), (5, 15), (5, 9), (1, 9)], fill=255)
    return m


def make_icons():
    gp = "Ui/Input/Gamepad"
    shape = arrow_mask()
    # PIL rotates counter-clockwise, and the source points up.
    ROT = {BTN_UP: 0, BTN_LEFT: 90, BTN_DOWN: 180, BTN_RIGHT: 270}

    def dir_icon(code, fill):
        return two_tone(shape.rotate(ROT[code], resample=Image.NEAREST), fill)

    base = {c: dir_icon(c, DIR_COLOR[c]) for c in DIR_COLOR}
    lit = {c: dir_icon(c, LIT) for c in DIR_COLOR}
    base[BTN_A] = load(gp + "/ButtonA/Idle.png")
    base[BTN_B] = load(gp + "/ButtonB/Idle.png")
    lit[BTN_A] = load(gp + "/ButtonA/Pressed.png")
    lit[BTN_B] = load(gp + "/ButtonB/Pressed.png")

    # Frame 20: the HIT MARKER — an open bracket a cue slides into, so the player can see WHERE the
    # beat is, not just what button it wants. Two tones already in the sheet, so the 15-colour budget
    # is untouched.
    marker = Image.new("RGBA", (ICEL, ICEL), (0, 0, 0, 0))
    md = ImageDraw.Draw(marker)
    for (x0, x1) in ((0, 3), (12, 15)):
        md.rectangle([x0, 0, x1, 15], outline=INK)
        md.rectangle([x0 + 1, 1, x1 - 1, 14], fill=LIT)
    blank = Image.new("RGBA", (ICEL, ICEL), (0, 0, 0, 0))
    frames = ([base.get(c, blank) for c in range(10)]
              + [lit.get(c, blank) for c in range(10)]
              + [marker])
    sheet = strip(frames, ICEL)

    opaque = {(r, g, b) for (r, g, b, a) in sheet.getdata() if a > 0}
    assert len(opaque) <= 15, (
        f"icons.png needs {len(opaque)} colours; a GBA sprite sheet gets 15 "
        f"(index 0 is transparent). Flatten a variant rather than quantising — quantising this "
        f"strip destroys the arrows."
    )
    sheet.save(os.path.join(OUT, "icons.png"))


# ── The stage ────────────────────────────────────────────────────────────────────────────────────
#
# TWO images now, and the important one is drawn TOP-DOWN.
#
# The floor used to be a picture of perspective. It is now a plan view of a dojo floor handed to a
# Mode 7 camera, which supplies the perspective itself — drawing it into the texture as well would
# apply it twice. This is the whole difference between the old stage and this one: the depth is
# computed from where the camera is, so it survives the camera moving.
#
# The wall is a 256px PANORAMA on an ordinary background, scrolled one full lap per camera
# revolution — a cylindrical backdrop. It sits at priority 3, behind the affine floor at 2, so above
# the horizon (where the floor's scanlines sample off-map and show nothing) the wall shows through,
# and below it the floor covers it. No transparency needed in either image.
FLOOR_W = FLOOR_H = 256      # texture pixels, tiled across the Mode 7 world
WALL_W, WALL_H = 256, 160

PLANK_A = (176, 132, 86)
PLANK_B = (196, 152, 100)
PLANK_C = (162, 120, 78)
MAT = (150, 62, 58)
MAT_EDGE = (198, 176, 120)
JOINT = (120, 86, 52)
SKY = (24, 18, 24)
# Must equal the `backdrop()` colour in rap-dojo's main.tish — this texel IS the sky.
SKY_TEXEL = (36, 26, 38)
WALL_HI = (86, 62, 52)
WALL_LO = (64, 46, 38)
PILLAR = (48, 34, 28)


def make_floor():
    """A plan view: planks running one way, a dojo mat in the middle, joints between boards."""
    img = Image.new("RGB", (FLOOR_W, FLOOR_H), PLANK_A)
    d = ImageDraw.Draw(img)
    # Boards, 16px wide, staggered so the short joints do not line up into a grid.
    for by in range(0, FLOOR_H, 16):
        for bx in range(0, FLOOR_W, 64):
            shade = [PLANK_A, PLANK_B, PLANK_C][((bx // 64) + (by // 16)) % 3]
            off = 32 if (by // 16) % 2 else 0
            x0 = (bx + off) % FLOOR_W
            d.rectangle([x0, by, x0 + 63, by + 15], fill=shade)
        d.line([(0, by), (FLOOR_W, by)], fill=JOINT)
    for by in range(0, FLOOR_H, 16):
        off = 32 if (by // 16) % 2 else 0
        for bx in range(0, FLOOR_W, 64):
            x0 = (bx + off) % FLOOR_W
            d.line([(x0, by), (x0, by + 15)], fill=JOINT)
    # The mat the lesson happens on — a landmark, so the camera's motion is legible. Without
    # something asymmetric the floor could be sliding rather than turning and you could not tell.
    #
    # Drawn centred on the texture's ORIGIN, in four corner pieces, so that when the texture tiles
    # across the world a whole mat lands where the tiles meet. That is what lets the stage sit at the
    # CENTRE of the Mode 7 world: with the mat at the texture's middle instead, the only mats are a
    # quarter-world from every edge, and a camera orbiting one of them spends half its revolution
    # looking off the end of the map — which renders as the floor collapsing to a thin band.
    mat = Image.new("RGB", (128, 128), MAT)
    md = ImageDraw.Draw(mat)
    md.rectangle([0, 0, 127, 127], outline=MAT_EDGE, width=3)
    md.ellipse([40, 40, 87, 87], outline=MAT_EDGE, width=3)
    for ox, oy in ((-64, -64), (FLOOR_W - 64, -64), (-64, FLOOR_H - 64), (FLOOR_W - 64, FLOOR_H - 64)):
        img.paste(mat, (ox, oy))
    # 256 colours are available (affine backgrounds are 8bpp) but few are needed; keeping the count
    # down keeps the baked tile data small.
    # Quantise first, then stamp the SKY TEXEL, so quantisation cannot merge it away.
    #
    # The floor wraps, which is what removes the dark band at the horizon — a NoWrap floor simply
    # runs out a few rows below it and shows the backdrop, on every frame. But a wrapping floor has
    # nowhere "off the map" to park the scanlines ABOVE the horizon: wherever they point, they sample
    # a real texel. So one is set to the backdrop colour and they all point at it. Those rows have
    # PA = PC = 0, meaning the whole scanline samples that single texel, so two pixels of texture buy
    # a clean sky. They land in the middle of the mat and are invisible at 1/256th of a lap.
    q = img.quantize(colors=30, method=Image.MEDIANCUT).convert("RGB")
    for yy in range(2):
        for xx in range(2):
            q.putpixel((xx, yy), SKY_TEXEL)
    q.save(os.path.join(OUT, "floor.png"))


def make_wall():
    """A 256px panorama: the far wall of the hall, scrolled one lap per camera revolution."""
    img = Image.new("RGB", (WALL_W, WALL_H), SKY)
    d = ImageDraw.Draw(img)
    horizon = 92
    for y in range(44, horizon):
        base = WALL_HI if y >= 76 else WALL_LO
        for x in range(WALL_W):
            img.putpixel((x, y), PILLAR if (x % 64) < 5 else base)
    d.rectangle([0, horizon - 3, WALL_W, horizon], fill=(122, 40, 40))
    # Below the horizon is never seen — the affine floor covers it — but it is filled rather than
    # left black so a camera dipped below the design height degrades to floor colour, not to a void.
    d.rectangle([0, horizon, WALL_W, WALL_H], fill=PLANK_A)

    house = load("Backgrounds/Tilesets/TilesetHouse.png")
    banners = house.crop((22 * 16, 20 * 16, 29 * 16, 22 * 16))
    sign = house.crop((4 * 16, 4 * 16, 6 * 16, 5 * 16))
    rgba = img.convert("RGBA")
    for x in range(0, WALL_W, banners.width):
        rgba.alpha_composite(banners, (x, 44))
    rgba.alpha_composite(sign, (WALL_W // 2 - sign.width // 2, horizon - sign.height - 1))
    rgba.convert("RGB").quantize(colors=30, method=Image.MEDIANCUT).convert("RGB").save(
        os.path.join(OUT, "wall.png")
    )


def main():
    make_student()
    make_master()
    make_icons()
    make_floor()
    make_wall()
    print("wrote student.png master.png icons.png floor.png wall.png ->", OUT)


if __name__ == "__main__":
    main()
