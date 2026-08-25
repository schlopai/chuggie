#!/usr/bin/env python3
"""Art for the JRPG PARTY example — a front-view, party-vs-party battle on the Ninja Adventure pack.

Emits, into `examples/jrpg-party/assets/`:

  heroes.png   sheet32: 4 heroes x 3 frames [idle, attack, dead], cell index = hero * 3 + frame
  foes.png     sheet32: 4 monsters x 2 frames [bob0, bob1],       cell index = foe  * 2 + frame

KEY layout facts, verified against the sheets rather than assumed (the same ones
`scripts/gen_creature_rpg.py` establishes, which is why this file reuses its rules):

  * `Actor/Character/<Name>/SeparateAnim/{Idle,Attack,Dead}.png` is 4 columns of **16x16**, column =
    direction in the engine's `facing()` order (0 down, 1 up, 2 left, 3 right). A front-view battle
    stands the party on the RIGHT looking left, so every hero frame here is column 2 and the other
    three columns are never read.
    (Never slice a character's combined `SpriteSheet.png` — it is not 16px aligned and yields
    garbled, all-identical frames. `SeparateAnim/` is the only correct source.)
  * `Actor/Monster/<Name>/SpriteSheet.png` is 64x64 = 4x4 of 16x16, and — unlike the Character cast —
    its four ROWS are animation/aspect variants, NOT four directions. Every cell faces the viewer.
    That is what makes monsters usable on a battle screen with no authoring at all, and it is also
    why cols 0 and 1 of row 0 are a usable two-frame idle bob.
  * The pack is inconsistent about the monster sheet's filename: most are `SpriteSheet.png`, some are
    named after their folder. Try both rather than assume.

  * EVERY CELL IS DOUBLED TO 32x32. 16x16 art on a 240x160 battle screen reads as a bug — the same
    conclusion, and the same `BATTLE_CEL = 32`, that `gen_creature_rpg.py` reached for its creatures.
    Doubling is NEAREST, so the result is the pack's own pixels at 2x and not a resampled blur.

  * ONE `clamp_colors` PASS PER ASSEMBLED SHEET, never per frame. That is what makes each sheet share
    a single `Palette16` — one of the GBA's sixteen palette banks instead of four. Two sheets here,
    so two banks; `docs/MEMORY.md` records that 16 banks is a hard ceiling that panics inside agb on
    an innocent caller when it is passed.

The roster indices below are duplicated in `examples/jrpg-party/src/main.tish` as the `art` argument
to `partyAdd`. Keep the two in step.

Run from the repo root:  python3 scripts/gen_jrpg_party.py
"""
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
OUT = os.path.join(ROOT, "examples", "jrpg-party", "assets")
os.makedirs(OUT, exist_ok=True)

CEL = 16          # the pack's cell size
BATTLE_CEL = 32   # what it is doubled to — see the header
LEFT = 2          # the SeparateAnim column that faces left

# (folder, display name) — the party, drawn facing left on the right of the screen.
HEROES = [
    ("Knight", "ROLAND"),
    ("SorcererBlack", "MERIC"),
    ("Shaman", "AVELLA"),
    ("Hunter", "KESTREL"),
]
HERO_ANIM = ["Idle.png", "Attack.png", "Dead.png"]

# (folder, display name) — the monsters, drawn facing the viewer on the left.
FOES = [
    ("Cyclope", "CYCLOPS"),
    ("Flam", "EMBERLING"),
    ("Dragon", "WYRMLET"),
    # NOT `Skeleton`: it is an `Actor/Character/`, whose combined sheet is 64x112 and not 16px
    # aligned. The size assert below is what caught it — see the header's note on slicing.
    ("Beast", "DIREBEAST"),
]


def clamp_colors(img, maxc=15):
    """Quantize to <= maxc opaque colours (the GBA's 4bpp budget), preserving transparency.

    Run on the ASSEMBLED sheet, never per frame — see the header."""
    img = img.convert("RGBA")
    opaque = {(r, g, b) for (r, g, b, a) in img.getdata() if a > 0}
    if len(opaque) <= maxc:
        return img
    alpha = img.getchannel("A")
    rgb = img.convert("RGB").quantize(colors=maxc, method=Image.MEDIANCUT).convert("RGBA")
    rgb.putalpha(alpha)
    return rgb


def monster_sheet(folder):
    """40 monsters are `SpriteSheet.png`, 26 are named after their folder, Mushroom's is lowercase."""
    for name in (f"{folder}.png", f"{folder.lower()}.png", "SpriteSheet.png"):
        for group in ("Monster", "Character"):
            path = os.path.join(PACK, "Actor", group, folder, name)
            if os.path.isfile(path):
                return path
    raise FileNotFoundError(f"no sheet for {folder}")


def double(im):
    return im.resize((im.width * 2, im.height * 2), Image.NEAREST)


def make_heroes(out_name):
    n = len(HEROES) * len(HERO_ANIM)
    strip = Image.new("RGBA", (n * CEL, CEL), (0, 0, 0, 0))
    for i, (folder, name) in enumerate(HEROES):
        base = os.path.join(PACK, "Actor", "Character", folder, "SeparateAnim")
        for f, anim in enumerate(HERO_ANIM):
            src = Image.open(os.path.join(base, anim)).convert("RGBA")
            # Column = direction, row = frame — for the animations that HAVE directions. `Dead.png`
            # is a lone 16x16 cell: a corpse faces nowhere, so the pack does not draw four of them.
            # Both shapes are real, so branch on the width instead of asserting one of them.
            col = LEFT
            if src.width == CEL:
                col = 0
            else:
                assert src.width == 4 * CEL, f"{folder}/{anim} is {src.size}, expected 1 or 4 columns"
            cell = src.crop((col * CEL, 0, (col + 1) * CEL, CEL))
            strip.paste(cell, ((i * len(HERO_ANIM) + f) * CEL, 0))
        print(f"    {name:9} {folder:14} {len(HERO_ANIM)} frames")
    out = double(clamp_colors(strip, 15))
    out.save(os.path.join(OUT, out_name))
    cols = len({(r, g, b) for (r, g, b, a) in out.getdata() if a > 0})
    print(f"  {out_name:11} {len(HEROES)} heroes x{len(HERO_ANIM)}  {out.width}x{out.height}  {cols} colours")


def make_foes(out_name):
    strip = Image.new("RGBA", (len(FOES) * 2 * CEL, CEL), (0, 0, 0, 0))
    for i, (folder, name) in enumerate(FOES):
        sheet = Image.open(monster_sheet(folder)).convert("RGBA")
        assert sheet.size == (4 * CEL, 4 * CEL), f"{folder}: {sheet.size}, expected 64x64"
        for f in range(2):
            strip.paste(sheet.crop((f * CEL, 0, (f + 1) * CEL, CEL)), ((i * 2 + f) * CEL, 0))
        # A two-frame bob that is two IDENTICAL frames is a still picture with a wasted cell, and it
        # is invisible in a preview. Assert the pack actually animates this monster.
        a = sheet.crop((0, 0, CEL, CEL)).tobytes()
        b = sheet.crop((CEL, 0, 2 * CEL, CEL)).tobytes()
        assert a != b, f"{name} ({folder}): cols 0 and 1 of row 0 are identical — pick another row"
        print(f"    {name:10} {folder:10} 2-frame bob")
    out = double(clamp_colors(strip, 15))
    out.save(os.path.join(OUT, out_name))
    cols = len({(r, g, b) for (r, g, b, a) in out.getdata() if a > 0})
    print(f"  {out_name:11} {len(FOES)} foes x2      {out.width}x{out.height}  {cols} colours")


if __name__ == "__main__":
    print("jrpg-party art")
    make_heroes("heroes.png")
    make_foes("foes.png")
