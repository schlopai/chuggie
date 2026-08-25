#!/usr/bin/env python3
"""Art prep for the AKARI example (a top-down action-RPG on the Ninja Adventure pack).

Hero is packed as 64x64 (`sheet64:`) with the 32px NinjaGreen body centered at (16,16) so the
katana can overhang. NPCs/monsters/items stay 32x32 (`sheet32:`). No attack FX layer.

KEY layout facts (verified against the sheets, not guessed):
  * `SeparateAnim/Walk.png` + `Idle.png` (standard chars) and NinjaGreen's `Separate/*.png` are laid
    out COLUMN = direction (down/up/left/right), ROW = frame. We transpose to ROW = direction.
  * The combined `SpriteSheet.png` is NOT cleanly 16px-aligned — do not slice it (that produced the
    garbled all-identical frames). Monster sheets (`Slime.png`, `YellowsBat/SpriteSheet.png`) ARE
    clean 64x64 with ROW = direction, COL = frame.
  * `Ui/Receptacle/Heart.png` is frame0 EMPTY .. frame4 FULL (the HUD wants frame 0 = empty).

Character sheet layout we emit: ROW = facing (0 down / 1 up / 2 left / 3 right); columns:
  hero (11): [idle, walk0..3, attack0..3, throw0..1]     monsters/NPCs (5): [idle, walk0..3]
Run from the repo root:  python3 scripts/gen_akari.py
"""
import math
import os
import json
import numpy as np
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
OUT = os.path.join(ROOT, "examples", "akari", "assets")
os.makedirs(OUT, exist_ok=True)

CEL = 32       # NPC/monster/item cells (sheet32:)
HERO_CEL = 64  # hero cells (sheet64:) — 32px body + margin for blade overhang
BODY_OFF = 16  # paste 32×32 source body at this offset inside HERO_CEL

# Blade CCW angles (SpriteInHand tip-DOWN at 0°). From hands catalog grips.
_BLADE_FOLLOW = {"DN": 0, "UP": 180, "LF": -90, "RT": 90}
_BLADE_IDLE = 180
_HANDLE = (2, 1)


def load(rel):
    return Image.open(os.path.join(PACK, rel)).convert("RGBA")


def clamp_colors(img, maxc=15):
    """Quantize to <= maxc opaque colors (the GBA 4bpp budget), preserving transparency."""
    img = img.convert("RGBA")
    opaque = {(r, g, b) for (r, g, b, a) in img.getdata() if a > 0}
    if len(opaque) <= maxc:
        return img
    alpha = img.getchannel("A")
    rgb = img.convert("RGB").quantize(colors=maxc, method=Image.MEDIANCUT).convert("RGBA")
    rgb.putalpha(alpha)
    return rgb


def center32(art):
    """Center a small (<=32) frame inside a 32x32 cell."""
    c = Image.new("RGBA", (CEL, CEL), (0, 0, 0, 0))
    c.paste(art, ((CEL - art.width) // 2, (CEL - art.height) // 2), art)
    return c


def center_in_hero(art32):
    """Center a 32×32 (or smaller) frame inside a 64×64 hero cell."""
    c = Image.new("RGBA", (HERO_CEL, HERO_CEL), (0, 0, 0, 0))
    c.paste(art32, (BODY_OFF, BODY_OFF), art32)
    return c


def load_katana_attack():
    """Blade seating from ninja_green_hands.json Attack grips. No FX."""
    path = os.path.join(PACK, "catalog", "ninja_green_hands.json")
    if not os.path.isfile(path):
        raise FileNotFoundError(f"Missing {path} — run: python3 scripts/index_ninja_green_hands.py")
    with open(path) as f:
        hands = json.load(f)
    atk = hands["sheets"]["Attack"]["frames"]
    table = {}
    for d in ("DN", "UP", "LF", "RT"):
        frames = []
        for f in range(4):
            fr = atk[d][f]
            grip = fr.get("sword_grip")
            hand = fr.get("sword_hand")
            if f == 0 and grip is not None:
                frames.append({"blade": {"grip": tuple(grip), "angle": _BLADE_IDLE, "hand": hand}})
            elif grip is not None and f >= 2:
                frames.append({"blade": {"grip": tuple(grip), "angle": _BLADE_FOLLOW[d], "hand": hand}})
            else:
                frames.append({"blade": None})
        table[d] = frames
    return table


def make_ninja_hero(out_name, katana_attack):
    """NinjaGreen hero → 11-col × 4-row sheet of 64×64
    [idle, walk×4, attack×4, throw×2].

    Attack: 32px body at (16,16) → blade → mitt. No FX layer.
    Throw: Push sheet (arms forward) — two frames for a wrist-flick on B.
    """
    ng = "Actor/CharacterAnimated/NinjaGreen/Separate"
    walk, idle, atk = load(ng + "/Walk.png"), load(ng + "/Idle.png"), load(ng + "/Attack.png")
    push = load(ng + "/Push.png")
    dirs = ("DN", "UP", "LF", "RT")
    mitt_rgb = tuple(json.load(open(os.path.join(PACK, "catalog", "ninja_green_hands.json")))["mitt_rgb"])
    blade = load("Items/Weapons/Katana/SpriteInHand.png")

    def src_cell(src, d, f):
        return src.crop((d * CEL, f * CEL, d * CEL + CEL, f * CEL + CEL))

    def compose_attack(body32, grip, angle):
        pad, cx = 64, 32
        canvas = Image.new("RGBA", (pad, pad), (0, 0, 0, 0))
        canvas.alpha_composite(blade, (cx - _HANDLE[0], cx - _HANDLE[1]))
        rot = canvas.rotate(angle, resample=Image.NEAREST, center=(cx, cx))
        out = body32.copy()
        out.alpha_composite(rot, (grip[0] - cx, grip[1] - cx))
        ba = np.array(body32)
        oa = np.array(out)
        gx, gy = grip
        for y in range(max(0, gy - 4), min(CEL, gy + 5)):
            for x in range(max(0, gx - 4), min(CEL, gx + 5)):
                r, g, b, a = ba[y, x]
                if a < 200 or (r, g, b) != mitt_rgb:
                    continue
                if y >= 22 and 10 <= x <= 16:
                    continue
                oa[y, x] = ba[y, x]
        return Image.fromarray(oa)

    out = Image.new("RGBA", (11 * HERO_CEL, 4 * HERO_CEL), (0, 0, 0, 0))
    for d in range(4):
        out.paste(center_in_hero(src_cell(idle, d, 0)), (0, d * HERO_CEL))
        for f in range(4):
            out.paste(center_in_hero(src_cell(walk, d, f)), ((1 + f) * HERO_CEL, d * HERO_CEL))
            base32 = src_cell(atk, d, f)
            spec = katana_attack[dirs[d]][f]
            if spec["blade"]:
                base32 = compose_attack(base32, spec["blade"]["grip"], spec["blade"]["angle"])
            out.paste(center_in_hero(base32), ((5 + f) * HERO_CEL, d * HERO_CEL))
        # Throw: Push frames 0–1 (arms out — distinct from the katana swing).
        for f in range(2):
            out.paste(center_in_hero(src_cell(push, d, f)), ((9 + f) * HERO_CEL, d * HERO_CEL))
    clamp_colors(out).save(os.path.join(OUT, out_name))


def make_char(char, out_name):
    """A standard character via its SeparateAnim (Walk 64x64 + Idle 64x16, 16px, COL=dir ROW=frame)
    → a 5-col × 4-row sheet of 32x32 cells: [idle, walk0..3] per direction (16px art centered)."""
    sep = f"Actor/Character/{char}/SeparateAnim"
    walk, idle = load(sep + "/Walk.png"), load(sep + "/Idle.png")

    def wcell(d, f):
        return walk.crop((d * 16, f * 16, d * 16 + 16, f * 16 + 16))

    def icell(d):
        return idle.crop((d * 16, 0, d * 16 + 16, 16))

    out = Image.new("RGBA", (5 * CEL, 4 * CEL), (0, 0, 0, 0))
    for d in range(4):
        out.paste(center32(icell(d)), (0, d * CEL))
        for f in range(4):
            out.paste(center32(wcell(d, f)), ((1 + f) * CEL, d * CEL))
    clamp_colors(out).save(os.path.join(OUT, out_name))


def make_monster(sheet_rel, out_name):
    """A monster's clean 64x64 sheet (ROW=dir, COL=frame, 16px) → 5-col × 4-row 32x32 sheet
    [idle=frame0, walk0..3] (16px art centered)."""
    src = load(sheet_rel)

    def cell(d, f):
        return src.crop((f * 16, d * 16, f * 16 + 16, d * 16 + 16))

    out = Image.new("RGBA", (5 * CEL, 4 * CEL), (0, 0, 0, 0))
    for d in range(4):
        out.paste(center32(cell(d, 0)), (0, d * CEL))
        for f in range(4):
            out.paste(center32(cell(d, f)), ((1 + f) * CEL, d * CEL))
    clamp_colors(out).save(os.path.join(OUT, out_name))


def make_bat(out_name):
    """YellowsBat's 64x64 sheet packs TWO creatures: the orange bat in COLS 0-1, a red flame-thing in
    COLS 2-3 (rows are facings). The old code grabbed cols 0-3, so the "flap" cycled bat,bat,flame,flame
    — that morphing looked like spinning. A bat has no meaningful facing here, so take the two bat frames
    (row 0, cols 0-1) as a pure 2-frame wing-flap loop, 16px centered."""
    src = load("Actor/Monster/YellowsBat/SpriteSheet.png")
    out = Image.new("RGBA", (2 * CEL, CEL), (0, 0, 0, 0))
    for f in range(2):
        cell = center32(src.crop((f * 16, 0, f * 16 + 16, 16)))
        out.paste(cell, (f * CEL, 0), cell)
    clamp_colors(out).save(os.path.join(OUT, out_name))


def make_boss(out_name):
    """DemonCyclop Walk.png (300x50 = 6 frames 50x50) → a 6-frame single-row 32x32 sheet
    (single facing; scaled to ~28px, bottom-aligned)."""
    src = load("Actor/Boss/DemonCyclop/Walk.png")
    fw = src.width // 6
    out = Image.new("RGBA", (6 * CEL, CEL), (0, 0, 0, 0))
    for i in range(6):
        f = src.crop((i * fw, 0, (i + 1) * fw, src.height)).resize((30, 30), Image.LANCZOS)
        out.paste(f, (i * CEL + 1, CEL - 30), f)
    clamp_colors(out).save(os.path.join(OUT, out_name))


def make_hearts(out_name):
    """HUD hearts: 3 frames [empty, half, full] in 32x32 cells with the 16px art at the TOP-LEFT,
    so the HUD (sprites spaced by `gap`) reads left-to-right. Ui/Receptacle/Heart.png is frame0
    EMPTY .. frame4 FULL — the HUD convention wants frame 0 = empty, so keep that order."""
    src = load("Ui/Receptacle/Heart.png")   # 80x16

    def grab(i):
        return src.crop((i * 16, 0, i * 16 + 16, 16))

    frames = [grab(0), grab(2), grab(4)]     # empty, half, full
    out = Image.new("RGBA", (3 * CEL, CEL), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        out.paste(f, (i * CEL, 0), f)        # art at top-left of each 32 cell
    out.save(os.path.join(OUT, out_name))


def pad16(img):
    cell = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    cell.paste(img, ((16 - img.width) // 2, (16 - img.height) // 2), img)
    return cell


def make_items(out_name):
    """Combined pickups, 32x32 cells (16px art centered): 0 heart 1 key 2 chest-closed 3 chest-open
    4 gem 5 heart-container."""
    heart = pad16(load("Items/Potion/Heart.png"))
    key = pad16(load("Items/Treasure/GoldKey.png"))
    chest = load("Items/Treasure/LittleTreasureChest.png")
    cw = chest.width // 2
    chest_closed = pad16(chest.crop((0, 0, cw, chest.height)))
    chest_open = pad16(chest.crop((cw, 0, 2 * cw, chest.height)))
    gem = pad16(load("Items/Resource/GemGreen.png"))
    container = pad16(load("Ui/Receptacle/Heart.png").crop((64, 0, 80, 16)))  # full heart
    frames = [heart, key, chest_closed, chest_open, gem, container]
    out = Image.new("RGBA", (len(frames) * CEL, CEL), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        out.paste(center32(f), (i * CEL, 0), center32(f))
    clamp_colors(out).save(os.path.join(OUT, out_name))


def make_shuriken(out_name):
    """The thrown ninja star (secondary weapon): the pack's own 2-frame spin, kept at 16x16 cells
    (`sheet:`) because the engine's bullet emitter draws a projectile as a 16px sprite.
    `FX/Projectile/Shuriken.png` is 32x16 = frame0 star upright, frame1 star turned 45deg."""
    src = load("FX/Projectile/Shuriken.png")
    assert src.size == (32, 16), f"expected a 2-frame 16px strip, got {src.size}"
    clamp_colors(src).save(os.path.join(OUT, out_name))


def make_title_bg(out_name):
    """A 240x160 title backdrop: a dusk sky over a hill with a black torii silhouette. Fully OPAQUE
    and <=15 colours (a `background:` import forced-blanks with transparency / too many colours)."""
    W, H = 240, 160
    img = Image.new("RGB", (W, H))
    px = img.load()
    top, bot, horizon = (26, 20, 46), (196, 104, 110), 104
    for y in range(H):
        c = tuple(int(top[i] + (bot[i] - top[i]) * (y / horizon)) for i in range(3)) if y < horizon else (36, 60, 44)
        for x in range(W):
            px[x, y] = c
    d = ImageDraw.Draw(img)
    for (sx, sy) in [(30, 18), (60, 30), (200, 22), (170, 40), (95, 15), (215, 55), (48, 48)]:
        d.point((sx, sy), fill=(240, 240, 220))
    tor, cx = (18, 12, 20), 120
    d.rectangle([cx - 40, 40, cx - 32, horizon], fill=tor)
    d.rectangle([cx + 32, 40, cx + 40, horizon], fill=tor)
    d.rectangle([cx - 54, 30, cx + 54, 40], fill=tor)
    d.polygon([(cx - 60, 30), (cx + 60, 30), (cx + 54, 24), (cx - 54, 24)], fill=tor)
    d.rectangle([cx - 44, 52, cx + 44, 58], fill=tor)
    img.convert("RGB").quantize(colors=15, method=Image.MEDIANCUT).convert("RGB").save(os.path.join(OUT, out_name))


# Portrait + UI prompt/button frames for packages/dialog + ui (sheet32:).
# Indices must match examples/akari/src/faces.tish.
FACE_CHARS = [
    ("OldMan", 0),      # Elder
    ("Woman", 1),
    ("Noble", 2),       # Merchant
    ("Master", 3),      # Sensei
    ("NinjaGreen", 4),  # Akari / narrator
]
FACE_CURSOR = 5
# Pocket-GUI-style controller prompts (Gamepad Idle / D-pad / shoulders).
PROMPT_A, PROMPT_B = 6, 7
PROMPT_L, PROMPT_R = 8, 9
PROMPT_UP, PROMPT_DOWN, PROMPT_LEFT, PROMPT_RIGHT = 10, 11, 12, 13
PROMPT_DPAD, PROMPT_START, PROMPT_SELECT = 14, 15, 16
# Theme Wood button chrome (ToffeeCraft-style panel affordances as icons).
BTN_NORMAL, BTN_HOVER, BTN_PRESSED = 17, 18, 19

# (frame_index, pack-relative path) — ~16px art centered in 32×32.
PROMPT_SOURCES = [
    (PROMPT_A, "Ui/Input/Gamepad/ButtonA/Idle.png"),
    (PROMPT_B, "Ui/Input/Gamepad/ButtonB/Idle.png"),
    (PROMPT_L, "Ui/Input/Gamepad/ButtonLB.png"),
    (PROMPT_R, "Ui/Input/Gamepad/ButtonRB.png"),
    (PROMPT_UP, "Ui/Input/Gamepad/DPadUp.png"),
    (PROMPT_DOWN, "Ui/Input/Gamepad/DPadDown.png"),
    (PROMPT_LEFT, "Ui/Input/Gamepad/DPadLeft.png"),
    (PROMPT_RIGHT, "Ui/Input/Gamepad/DPadRight.png"),
    (PROMPT_DPAD, "Ui/Input/Gamepad/DPad.png"),
    (PROMPT_START, "Ui/Input/Gamepad/Start.png"),
    (PROMPT_SELECT, "Ui/Input/Gamepad/Select.png"),
]
BTN_SOURCES = [
    (BTN_NORMAL, "Ui/Theme/Theme Wood/button_normal.png"),
    (BTN_HOVER, "Ui/Theme/Theme Wood/button_hover.png"),
    (BTN_PRESSED, "Ui/Theme/Theme Wood/button_pressed.png"),
]


def make_faces(out_name):
    """Facesets + ► cursor + gamepad prompts + Theme Wood buttons → sheet32 strip."""
    def face(name):
        im = load(f"Actor/Character/{name}/Faceset.png").resize((CEL, CEL), Image.LANCZOS)
        alpha = im.split()[3]
        q = im.convert("RGB").quantize(colors=15, method=Image.MEDIANCUT, dither=Image.NONE).convert("RGBA")
        q.putalpha(alpha.point(lambda a: 255 if a >= 128 else 0))
        px = q.load()
        for y in range(q.height):
            for x in range(q.width):
                r, g, b, a = px[x, y]
                if a == 0:
                    px[x, y] = (0, 0, 0, 0)
        return q

    cur = Image.new("RGBA", (CEL, CEL), (0, 0, 0, 0))
    d = ImageDraw.Draw(cur)
    yl = (0xFF, 0xE0, 0x6A, 255)
    oy = 3
    d.polygon([(1, 0 + oy), (7, 3 + oy), (1, 6 + oy)], fill=yl)
    for (rx, ry) in [(1, 0 + oy), (1, 6 + oy), (7, 3 + oy)]:
        cur.putpixel((rx, ry), (0, 0, 0, 0))
    cur.putpixel((6, 3 + oy), yl)

    n = BTN_PRESSED + 1
    out = Image.new("RGBA", (n * CEL, CEL), (0, 0, 0, 0))
    for name, i in FACE_CHARS:
        out.paste(face(name), (i * CEL, 0))
    out.paste(cur, (FACE_CURSOR * CEL, 0), cur)
    for i, rel in PROMPT_SOURCES:
        cell = center32(load(rel))
        out.paste(cell, (i * CEL, 0), cell)
    for i, rel in BTN_SOURCES:
        # 16×8 wood buttons → 2× nearest so they read at sheet32 icon size.
        src = load(rel)
        big = src.resize((src.width * 2, src.height * 2), Image.NEAREST)
        cell = center32(big)
        out.paste(cell, (i * CEL, 0), cell)
    clamp_colors(out).save(os.path.join(OUT, out_name))


NPCS = {"elder.png": "OldMan", "woman.png": "Woman", "merchant.png": "Noble", "sensei.png": "Master"}
MONSTERS = {"slime.png": "Actor/Monster/Slime/Slime.png", "bat.png": "Actor/Monster/YellowsBat/SpriteSheet.png",
            "skeleton.png": None}  # skeleton handled below (standard char)


def make_size_debug_sheets(hero_path="hero.png"):
    """Idle + attack-f1 per facing at 16 / 32 / 64 for Sprite Size Debug menu."""
    hero = Image.open(os.path.join(OUT, hero_path)).convert("RGBA")
    cell = HERO_CEL

    def hero_cell(d, col):
        return hero.crop((col * cell, d * cell, col * cell + cell, d * cell + cell))

    cells64 = [hero_cell(d, 0) for d in range(4)] + [hero_cell(d, 6) for d in range(4)]

    def pack(size, name):
        out = Image.new("RGBA", (8 * size, size), (0, 0, 0, 0))
        for i, c in enumerate(cells64):
            scaled = c.resize((size, size), Image.NEAREST)
            out.paste(scaled, (i * size, 0), scaled)
        clamp_colors(out).save(os.path.join(OUT, name))
        print(f"  debug {name} ({size}x{size} x8 idle+atk1)")

    pack(16, "debug-hero16.png")
    pack(32, "debug-hero32.png")
    pack(64, "debug-hero64.png")


def main():
    katana = load_katana_attack()
    print("  katana grips from catalog/ninja_green_hands.json:")
    for d, frames in katana.items():
        roles = [fr["blade"]["grip"] if fr["blade"] else "—" for fr in frames]
        print(f"    {d}: {roles}")
    make_ninja_hero("hero.png", katana)
    print(f"  hero  hero.png (NinjaGreen 64x64 sheet64, body+blade, no FX)")
    make_size_debug_sheets("hero.png")
    for out, char in NPCS.items():
        make_char(char, out); print(f"  npc   {out:12} <- {char}")
    make_char("Skeleton", "skeleton.png"); print("  enemy skeleton.png <- Skeleton")
    make_monster("Actor/Monster/Slime/Slime.png", "slime.png"); print("  enemy slime.png")
    make_bat("bat.png"); print("  enemy bat.png (4-frame flap)")
    make_boss("boss.png"); print("  boss  boss.png")
    make_hearts("hearts.png"); print("  ui    hearts.png (empty->full)")
    make_items("items.png"); print("  items items.png")
    make_shuriken("shuriken.png"); print("  wpn   shuriken.png (2-frame spin, 16px cells)")
    make_title_bg("title-bg.png"); print("  bg    title-bg.png")
    make_faces("faces32.png"); print("  ui    faces32.png (portraits + cursor + prompts + buttons)")
    print(f"AKARI art (hero sheet64, rest sheet32) -> {OUT}")


if __name__ == "__main__":
    main()
