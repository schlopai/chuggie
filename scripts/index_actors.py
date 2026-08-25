#!/usr/bin/env python3
"""index_actors.py — structural index of every Ninja Adventure actor (Character / Monster /
Boss / Animal / CharacterAnimated) into catalog/actors.json + actors.md.

No vision needed: the frame layout is fixed by the pack convention (established from the
SeparateAnim clip dims + a rendered Walk sheet):

  Combined SpriteSheet.png, row = direction/action, col = frame (16x16 frames):
    64x112 (4x7)  standard character : rows 0-3 walk DOWN/UP/LEFT/RIGHT (4 frames each),
                                       row 4 idle, row 5 attack, row 6 jump
    64x64  (4x4)  simple/monster     : rows 0-3 walk DOWN/UP/LEFT/RIGHT (4 frames), no actions
    Wx16   (Nx1)  strip              : N single-row frames (tiny critters / effects)
  SeparateAnim/*.png : Walk 64x64 (dir x frame); Idle/Attack/Jump 64x16 (4 frames, facing down);
                       Dead/Item/Special1/Special2 16x16 (single frame).
  Faceset.png 38x38  : dialogue portrait.

Large bosses use non-16 frames; those are recorded with raw dims and flagged.
"""
import os, json, sys
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASE = os.path.join(ROOT, "assets/ninja-adventure/Actor")
REL = "Actor"


def dims(p):
    try:
        return list(Image.open(p).size)
    except Exception:
        return None


def infer_layout(w, h):
    """Best-effort frame layout from sheet dims (16x16 frames)."""
    if (w, h) == (64, 112):
        return "standard-4dir", "rows0-3 walk D/U/L/R x4 frames; row4 idle; row5 attack; row6 jump"
    if (w, h) == (64, 64):
        return "walk-4dir", "rows0-3 walk D/U/L/R x4 frames (no action rows)"
    if h == 16 and w % 16 == 0:
        return "strip", f"{w // 16}-frame single-row strip"
    if w % 16 == 0 and h % 16 == 0:
        return "grid-16", f"{w // 16}x{h // 16} of 16px frames (non-standard; inspect)"
    return "large-or-irregular", f"{w}x{h}; frame size not 16px (large sprite / boss) — inspect"


def index_actor(group, name, folder):
    entry = {"name": name, "group": group, "dir": os.path.relpath(folder, ROOT)}
    sheet = os.path.join(folder, "SpriteSheet.png")
    # some actors name the sheet after themselves or use Sprite.png
    if not os.path.isfile(sheet):
        for alt in (name + ".png", "Sprite.png"):
            if os.path.isfile(os.path.join(folder, alt)):
                sheet = os.path.join(folder, alt)
                break
    if os.path.isfile(sheet):
        d = dims(sheet)
        layout, note = infer_layout(*d) if d else ("?", "unreadable")
        entry["sheet"] = {"file": os.path.relpath(sheet, ROOT), "dims": d, "layout": layout, "note": note}
    fs = os.path.join(folder, "Faceset.png")
    if os.path.isfile(fs):
        entry["faceset"] = {"file": os.path.relpath(fs, ROOT), "dims": dims(fs)}
    for saname in ("SeparateAnim", "Separate"):
        sa = os.path.join(folder, saname)
        if os.path.isdir(sa):
            entry["separate_anim"] = {
                os.path.splitext(f)[0]: dims(os.path.join(sa, f))
                for f in sorted(os.listdir(sa)) if f.endswith(".png")
            }
            break
    # any extra sheets (recolors / alt animations) at the actor root
    extras = [f for f in sorted(os.listdir(folder))
              if f.endswith(".png") and f not in (os.path.basename(sheet), "Faceset.png")]
    if extras:
        entry["extra_pngs"] = {f: dims(os.path.join(folder, f)) for f in extras}
    return entry


def main():
    actors = []
    for group in sorted(os.listdir(BASE)):
        gdir = os.path.join(BASE, group)
        if not os.path.isdir(gdir):
            continue
        for name in sorted(os.listdir(gdir)):
            folder = os.path.join(gdir, name)
            if not os.path.isdir(folder):
                continue
            # an actor folder has at least one png (directly or in SeparateAnim)
            has_png = any(f.endswith(".png") for f in os.listdir(folder)) or \
                os.path.isdir(os.path.join(folder, "SeparateAnim"))
            if has_png:
                actors.append(index_actor(group, name, folder))
    # loose group-level pngs (e.g. Character/Shadow.png — the shared drop shadow)
    shared = {}
    for group in sorted(os.listdir(BASE)):
        gdir = os.path.join(BASE, group)
        if not os.path.isdir(gdir):
            continue
        loose = [f for f in sorted(os.listdir(gdir))
                 if f.endswith(".png") and "Preview" not in f]
        for f in loose:
            shared[os.path.relpath(os.path.join(gdir, f), ROOT)] = dims(os.path.join(gdir, f))

    out = os.path.join(ROOT, "assets/ninja-adventure/catalog/actors.json")
    json.dump({"pattern": __doc__.split("Large bosses")[0].strip(),
               "shared": shared, "actors": actors},
              open(out, "w"), indent=1)

    # markdown summary: counts + per-group tables (name, sheet dims, layout, has faceset/anim)
    from collections import Counter
    bygroup = Counter(a["group"] for a in actors)
    lines = ["# Ninja Adventure — Actor catalog", "",
             "Structural index of every actor. Frame layout convention (row = direction/action, "
             "col = frame, 16px):", "",
             "- **64x112** standard: rows 0-3 walk D/U/L/R (×4 frames), row 4 idle, row 5 attack, row 6 jump",
             "- **64x64** simple/monster: rows 0-3 walk D/U/L/R (×4), no action rows",
             "- **SeparateAnim/**: Walk 64x64 (dir×frame); Idle/Attack/Jump 64x16 (4 frames); Dead/Item/Special 16x16",
             "- **Faceset.png** 38x38 dialogue portrait", "",
             "| Group | Count |", "|---|---:|"]
    for g, n in sorted(bygroup.items()):
        lines.append(f"| {g} | {n} |")
    lines.append(f"| **total** | **{len(actors)}** |")
    lines.append("")
    layouts = Counter(a.get("sheet", {}).get("layout", "none") for a in actors)
    lines.append("**Sheet layouts:** " + ", ".join(f"{k}×{v}" for k, v in layouts.most_common()))
    lines.append("")
    for g in sorted(bygroup):
        lines.append(f"## {g} ({bygroup[g]})")
        lines.append("")
        lines.append("| Name | Sheet | Layout | Faceset | SepAnim |")
        lines.append("|---|---|---|:-:|:-:|")
        for a in [x for x in actors if x["group"] == g]:
            sh = a.get("sheet", {})
            dim = "×".join(map(str, sh["dims"])) if sh.get("dims") else "—"
            lines.append(f"| {a['name']} | {dim} | {sh.get('layout','—')} "
                         f"| {'✓' if 'faceset' in a else '—'} | {'✓' if 'separate_anim' in a else '—'} |")
        lines.append("")
    open(os.path.join(ROOT, "assets/ninja-adventure/catalog/actors.md"), "w").write("\n".join(lines))

    print(f"indexed {len(actors)} actors:", dict(bygroup))
    print("layouts:", dict(layouts))


if __name__ == "__main__":
    main()
