#!/usr/bin/env python3
"""Local sprite sheets for examples/bench-room.

Emits `examples/bench-room/assets/{hero,slime,bat}.png` — one 32x32 cell each, from the vendored
Ninja Adventure pack.

WHY THIS EXISTS AT ALL. bench-room used to import the topdown RPG port's hero/slime/bat PNGs.
`slime.png` and `bat.png` were deleted from that example in `4dd90fc1`, bench-room was the only remaining
reference to them, and it has not built since — silently, because a benchmark nobody reruns has no
ROM to go missing. Reaching into another example's assets is what made that possible, and it is
already against the house rule (`docs/MEMORY.md`: never grab art from other examples — bake from the
catalog). `examples/bench-ai` gets this right with its own local `marker.png`.

The sheets are one frame each and that is correct: bench-room never animates them. It calls
`e.setSprite(sprite_new(sheet))` and nothing else — no `anim_play`, no directional animation — so
the art is a stand-in for an entity that exists, and what the benchmark measures (room streaming and
per-entity cost) does not depend on it.

Three separate files rather than one strip, because `enemy(e, col, row, sheet, …)` takes a SHEET
handle per kind — matching the shape the benchmark already had.

Run from the repo root:  python3 scripts/gen_bench_room.py
"""
import pathlib

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
NA = ROOT / "assets/ninja-adventure"
OUT = ROOT / "examples/bench-room/assets"
CEL = 16
OUT_CEL = 32

# (output name, source, kind) — `char` takes the left-facing idle column, `monster` takes row 0 col 0.
SHEETS = [
    ("hero.png", "Actor/Character/NinjaGreen/SeparateAnim/Idle.png", "char"),
    ("slime.png", "Slime", "monster"),
    ("bat.png", "BlueBat", "monster"),
]


def monster_sheet(folder):
    """The pack is inconsistent: most monsters are `SpriteSheet.png`, some are named after their
    folder (Slime is `Slime.png`), and Mushroom's is lowercase. Try all three rather than assume."""
    for name in (f"{folder}.png", f"{folder.lower()}.png", "SpriteSheet.png"):
        p = NA / "Actor" / "Monster" / folder / name
        if p.is_file():
            return p
    raise FileNotFoundError(f"no sheet for monster {folder}")


def quantise(img, limit=15):
    a = img.getchannel("A").point(lambda v: 255 if v > 127 else 0)
    flat = Image.new("RGB", img.size, (0, 0, 0))
    flat.paste(img.convert("RGB"), (0, 0), a)
    out = flat.quantize(colors=limit, dither=Image.NONE).convert("RGBA")
    out.putalpha(a)
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, rel, kind in SHEETS:
        path = NA / rel if kind == "char" else monster_sheet(rel)
        src = Image.open(path).convert("RGBA")
        if kind == "char":
            # column = direction (0 down, 1 up, 2 left, 3 right); row 0 is the idle frame.
            cell = src.crop((0, 0, CEL, CEL))
        else:
            # A monster sheet's four ROWS are animation variants, not directions — every cell faces
            # the viewer, so (0,0) is a usable standing frame.
            assert src.size == (4 * CEL, 4 * CEL), f"{path} is {src.size}, expected 64x64"
            cell = src.crop((0, 0, CEL, CEL))
        big = quantise(cell).resize((OUT_CEL, OUT_CEL), Image.NEAREST)
        big.save(OUT / name)
        print(f"  {name:10} {OUT_CEL}x{OUT_CEL}  from {rel}")


if __name__ == "__main__":
    print("bench-room sprites")
    main()
