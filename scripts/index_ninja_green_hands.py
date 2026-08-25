#!/usr/bin/env python3
"""Index EVERY hand/mitt on NinjaGreen — full SpriteSheet + all Separate sheets.

Mitt RGB (239,145,79) = gloves AND shoes. 4-connected components; wide blobs split;
feet vs hands by position. Every occupied 32px cell is documented.

Outputs:
  assets/ninja-adventure/catalog/ninja_green_hands.json
  assets/ninja-adventure/catalog/ninja_green_hands.md
  examples/akari/assets/attack-index/HANDS_INDEX/*.png

Run from repo root:  python3 scripts/index_ninja_green_hands.py
"""
from __future__ import annotations

import json
import os

import numpy as np
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACK = os.path.join(ROOT, "assets", "ninja-adventure")
NG32 = os.path.join(PACK, "Actor", "CharacterAnimated", "NinjaGreen")
SEP32 = os.path.join(NG32, "Separate")
NG16 = os.path.join(PACK, "Actor", "Character", "NinjaGreen")
SEP16 = os.path.join(NG16, "SeparateAnim")
OUT_CAT = os.path.join(PACK, "catalog", "ninja_green_hands.json")
OUT_MD = os.path.join(PACK, "catalog", "ninja_green_hands.md")
OUT_VIS = os.path.join(ROOT, "examples", "akari", "assets", "attack-index", "HANDS_INDEX")

MITT = (239, 145, 79)
DIRS = ("DN", "UP", "LF", "RT")
CEL = 32  # default; 16px Character/ sheets pass cell_px explicitly

# SpriteSheet row bands (verified by matching Separate cells → sheet coords)
SPRITESHEET_LAYOUT = {
    "file": "assets/ninja-adventure/Actor/CharacterAnimated/NinjaGreen/SpriteSheet.png",
    "dims": [256, 544],
    "grid": [8, 17],
    "cell_px": 32,
    "note": "8 cols × 17 rows of 32px. Left block = locomotion/idle/swim/jump; right = attack/hit/roll/push/misc.",
    "rows": {
        "0-3": "Idle (cols 0-3 dirs DN/UP/LF/RT × frames 0-3) | Attack (cols 4-7). Attack DNf2=DNf3 and UPf2=UPf3 share cells.",
        "4-5": "Walk DN/LF/RT (col0/2/3); Walk UP sparse (col1). Hit (cols 4-7, 2 frames).",
        "6-7": "Walk continued | Roll (cols 4-7, frames 0-1).",
        "8": "Swim f0 (cols 0-3) | Roll f2 (cols 4-7).",
        "9-11": "Swim f1-f3 (cols 0-3) | Push f0-f2 (cols 4-7).",
        "12": "Jump mixed (cols 0-3) | Push f3 (cols 4-7).",
        "13-14": "Jump continued (cols 0-3) | Dead/Climb/Pickup/Item (cols 4-7).",
        "15-16": "Climb f2-f3 at (5,15)/(5,16) only.",
    },
}


def connected_components(pts):
    s = set(pts)
    seen = set()
    comps = []
    for p in pts:
        if p in seen:
            continue
        stack = [p]
        seen.add(p)
        cl = []
        while stack:
            x, y = stack.pop()
            cl.append((x, y))
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                q = (x + dx, y + dy)
                if q in s and q not in seen:
                    seen.add(q)
                    stack.append(q)
        comps.append(cl)
    return comps


def split_wide(cl, max_span=5):
    xs = sorted(set(p[0] for p in cl))
    if len(xs) < 2 or xs[-1] - xs[0] <= max_span:
        return [cl]
    gaps = [(xs[i + 1] - xs[i], i) for i in range(len(xs) - 1)]
    gap, gi = max(gaps)
    if gap < 2:
        return [cl]
    cut = xs[gi]
    left = [p for p in cl if p[0] <= cut]
    right = [p for p in cl if p[0] > cut]
    return [c for c in (left, right) if c]


def classify(centroid):
    cx, cy = centroid
    if cy >= 22 and 9 <= cx <= 17:
        return "foot"
    if cy >= 24:
        return "foot"
    return "hand"


def analyze_cell(cell_a):
    pts = [
        (int(x), int(y))
        for y in range(cell_a.shape[0])
        for x in range(cell_a.shape[1])
        if tuple(cell_a[y, x, :3]) == MITT and cell_a[y, x, 3] > 200
    ]
    raw = []
    for cl in connected_components(pts):
        for part in split_wide(cl):
            xs = [p[0] for p in part]
            ys = [p[1] for p in part]
            raw.append(
                {
                    "pixels": [[x, y] for x, y in sorted(part)],
                    "n": len(part),
                    "centroid": [int(round(sum(xs) / len(xs))), int(round(sum(ys) / len(ys)))],
                    "bbox": [min(xs), min(ys), max(xs), max(ys)],
                }
            )
    hands, feet = [], []
    for cl in raw:
        role = classify(cl["centroid"])
        entry = {**cl, "role": role}
        (hands if role == "hand" else feet).append(entry)
    hands.sort(key=lambda h: (h["centroid"][0], h["centroid"][1]))
    feet.sort(key=lambda h: h["centroid"][0])
    for i, h in enumerate(hands):
        h["id"] = f"hand{i}"
    for i, ft in enumerate(feet):
        ft["id"] = f"foot{i}"
    return hands, feet


def pick_sword_hand(dir_name, frame_idx, hands, anim):
    if not hands:
        return None
    if anim == "Attack" and frame_idx == 1:
        return None
    if anim == "Attack" and frame_idx == 0:
        return min(hands, key=lambda h: h["centroid"][1])["id"]
    if anim == "Attack" and frame_idx >= 2:
        if dir_name == "RT":
            return max(hands, key=lambda h: h["centroid"][0])["id"]
        return min(hands, key=lambda h: h["centroid"][0])["id"]
    if dir_name == "RT":
        return max(hands, key=lambda h: h["centroid"][0])["id"]
    if dir_name == "LF":
        return min(hands, key=lambda h: h["centroid"][0])["id"]
    return min(hands, key=lambda h: h["centroid"][1])["id"]


def dir_label(ncols, c):
    if ncols == 4:
        return DIRS[c]
    if ncols == 1:
        return "—"
    return f"col{c}"


def scan_grid(path, rel_file, layout_note, anim_name=None):
    """Scan any WxH image that tiles into 32px cells. Returns sheet dict + flat cell list."""
    im = Image.open(path).convert("RGBA")
    a = np.array(im)
    w, h = im.size
    assert w % CEL == 0 and h % CEL == 0, (path, w, h)
    ncols, nrows = w // CEL, h // CEL
    anim = anim_name or os.path.basename(path).replace(".png", "")
    frames_by_dir = {}
    cells = []
    empty = 0
    for r in range(nrows):
        for c in range(ncols):
            cell_a = a[r * CEL : (r + 1) * CEL, c * CEL : (c + 1) * CEL]
            opaque = int((cell_a[:, :, 3] > 200).sum())
            if opaque < 5:
                empty += 1
                cells.append(
                    {
                        "col": c,
                        "row": r,
                        "empty": True,
                        "dir": dir_label(ncols, c) if anim != "SpriteSheet" else None,
                        "frame": r if anim != "SpriteSheet" else None,
                    }
                )
                continue
            hands, feet = analyze_cell(cell_a)
            dlab = dir_label(ncols, c)
            # SpriteSheet: row is not "frame" in the Separate sense
            if anim == "SpriteSheet":
                sid = None
                grip = None
                # Prefer outermost / uppermost hand as default grip for any cell
                if hands:
                    if c in (3, 7):  # often RT columns in sheet bands
                        sid = max(hands, key=lambda h: h["centroid"][0])["id"]
                    else:
                        sid = min(hands, key=lambda h: (h["centroid"][1], h["centroid"][0]))["id"]
                    grip = next(h["centroid"] for h in hands if h["id"] == sid)
                rec = {
                    "col": c,
                    "row": r,
                    "empty": False,
                    "opaque": opaque,
                    "hands": hands,
                    "feet": feet,
                    "sword_hand": sid,
                    "sword_grip": grip,
                }
            else:
                sid = pick_sword_hand(dlab, r, hands, anim)
                grip = next((h["centroid"] for h in hands if h["id"] == sid), None) if sid else None
                rec = {
                    "col": c,
                    "row": r,
                    "dir": dlab,
                    "frame": r,
                    "empty": False,
                    "opaque": opaque,
                    "hands": hands,
                    "feet": feet,
                    "sword_hand": sid,
                    "sword_grip": grip,
                }
                frames_by_dir.setdefault(dlab, []).append(
                    {
                        "frame": r,
                        "hands": hands,
                        "feet": feet,
                        "sword_hand": sid,
                        "sword_grip": grip,
                    }
                )
            cells.append(rec)

    sheet = {
        "file": rel_file,
        "dims": [w, h],
        "layout": layout_note,
        "cell_px": CEL,
        "n_cols": ncols,
        "n_rows": nrows,
        "n_cells": ncols * nrows,
        "n_occupied": ncols * nrows - empty,
        "n_empty": empty,
        "cells": cells,
    }
    if frames_by_dir:
        sheet["frames"] = frames_by_dir
        sheet["n_frames"] = nrows
        sheet["n_dirs"] = ncols
    return sheet


def build_spritesheet_crossref(sep_sheets, ss_sheet):
    """Attach Separate anim labels onto SpriteSheet cells via pixel identity."""
    ss_path = os.path.join(ROOT, ss_sheet["file"])
    ss = np.array(Image.open(ss_path).convert("RGBA"))
    idx = {}
    for r in range(ss_sheet["n_rows"]):
        for c in range(ss_sheet["n_cols"]):
            cell = ss[r * CEL : (r + 1) * CEL, c * CEL : (c + 1) * CEL]
            if (cell[:, :, 3] > 200).sum() < 5:
                continue
            idx.setdefault(cell.tobytes(), []).append([c, r])

    for anim, sheet in sep_sheets.items():
        path = os.path.join(ROOT, sheet["file"])
        im = np.array(Image.open(path).convert("RGBA"))
        for cell in sheet["cells"]:
            if cell.get("empty"):
                continue
            c, r = cell["col"], cell["row"]
            blob = im[r * CEL : (r + 1) * CEL, c * CEL : (c + 1) * CEL].tobytes()
            hits = idx.get(blob, [])
            cell["spritesheet"] = hits[0] if hits else None
            if len(hits) > 1:
                cell["spritesheet_aliases"] = hits[1:]

    # Reverse: label each SS cell
    for cell in ss_sheet["cells"]:
        cell["labels"] = []
    # rebuild lookup by col,row
    by_cr = {(c["col"], c["row"]): c for c in ss_sheet["cells"]}
    for anim, sheet in sep_sheets.items():
        for cell in sheet["cells"]:
            if cell.get("empty") or not cell.get("spritesheet"):
                continue
            sc, sr = cell["spritesheet"]
            label = f"{anim}:{cell.get('dir', '?')}f{cell.get('frame', '?')}"
            by_cr[(sc, sr)]["labels"].append(label)


def write_visual_sheet(name, sheet, scale=3):
    os.makedirs(OUT_VIS, exist_ok=True)
    src = Image.open(os.path.join(ROOT, sheet["file"])).convert("RGBA")
    ncols, nrows = sheet["n_cols"], sheet["n_rows"]
    cell = sheet.get("cell_px", CEL)
    sc = scale
    canvas = Image.new(
        "RGBA",
        (ncols * (cell * sc + 4) + 24, nrows * (cell * sc + 18) + 36),
        (10, 12, 16, 255),
    )
    dr = ImageDraw.Draw(canvas)
    dr.text(
        (4, 2),
        f"{name} — magenta=hand cyan=foot yellow=sword_grip | {sheet['n_occupied']}/{sheet['n_cells']} cells @ {cell}px",
        fill=(255, 220, 100),
    )
    by_cr = {(c["col"], c["row"]): c for c in sheet["cells"]}
    for r in range(nrows):
        for c in range(ncols):
            cell_img = src.crop((c * cell, r * cell, c * cell + cell, r * cell + cell))
            px = 12 + c * (cell * sc + 4)
            py = 22 + r * (cell * sc + 18)
            bg = Image.new("RGBA", (cell * sc, cell * sc), (18, 20, 26, 255))
            bg.paste(
                cell_img.resize((cell * sc, cell * sc), Image.NEAREST),
                (0, 0),
                cell_img.resize((cell * sc, cell * sc), Image.NEAREST),
            )
            canvas.paste(bg, (px, py))
            rec = by_cr[(c, r)]
            if rec.get("empty"):
                continue
            for h in rec.get("hands", []):
                for x, y in h["pixels"]:
                    dr.rectangle(
                        [px + x * sc, py + y * sc, px + x * sc + sc - 1, py + y * sc + sc - 1],
                        outline=(255, 0, 255),
                    )
            for ft in rec.get("feet", []):
                for x, y in ft["pixels"]:
                    dr.rectangle(
                        [px + x * sc, py + y * sc, px + x * sc + sc - 1, py + y * sc + sc - 1],
                        outline=(0, 220, 255),
                    )
            if rec.get("sword_grip"):
                gx, gy = rec["sword_grip"]
                dr.line([px + gx * sc - 5, py + gy * sc, px + gx * sc + 5, py + gy * sc], fill=(255, 255, 0), width=2)
                dr.line([px + gx * sc, py + gy * sc - 5, px + gx * sc, py + gy * sc + 5], fill=(255, 255, 0), width=2)
            tag = rec.get("sword_grip")
            labs = rec.get("labels") or []
            label = f"{c},{r}"
            if tag:
                label += f" g={tag}"
            elif labs:
                label += " " + labs[0][:16]
            dr.text((px, py + cell * sc + 1), label, fill=(200, 200, 120))
    out = os.path.join(OUT_VIS, f"{name}_hands.png")
    canvas.save(out)
    print(f"  visual {out}")


def write_markdown(catalog):
    t = catalog["totals"]
    lines = [
        "# NinjaGreen hand / mitt index",
        "",
        "**Source of truth:** [`ninja_green_hands.json`](ninja_green_hands.json)",
        "**Regenerate:** `python3 scripts/index_ninja_green_hands.py`",
        "**Visuals:** `examples/akari/assets/attack-index/HANDS_INDEX/`",
        "",
        "Mitt RGB `(239, 145, 79)` = gloves **and** shoes. **Every occupied cell** on both",
        "NinjaGreen variants is indexed (hand/foot clusters, centroids, pixels, `sword_grip`).",
        "",
        "## Coverage",
        "",
        f"- **CharacterAnimated 32px:** {t['character_animated_32px']['occupied_cells']} occupied "
        f"(SpriteSheet {t['character_animated_32px']['spritesheet_occupied']}/"
        f"{t['character_animated_32px']['spritesheet_cells']} + "
        f"{t['character_animated_32px']['separate_sheets']} Separate sheets)",
        f"- **Character 16px:** {t['character_16px']['occupied_cells']} occupied "
        f"({t['character_16px']['sheets']} sheets)",
        f"- **Grand total:** {t['occupied_cells']} occupied cells",
        "",
        "### CharacterAnimated Separate (32px)",
        "",
        "| Sheet | Cells | Occupied |",
        "|-------|------:|---------:|",
    ]
    ss = catalog["spritesheet"]
    lines.append(f"| SpriteSheet 8×17 | {ss['n_cells']} | {ss['n_occupied']} |")
    for name, sheet in sorted(catalog["separate"].items()):
        lines.append(f"| Separate/{name} | {sheet['n_cells']} | {sheet['n_occupied']} |")
    lines += [
        "",
        "### Character SeparateAnim (16px)",
        "",
        "| Sheet | Cells | Occupied |",
        "|-------|------:|---------:|",
    ]
    for name, sheet in sorted(catalog["character_16px"]["sheets"].items()):
        lines.append(f"| {name} | {sheet['n_cells']} | {sheet['n_occupied']} |")
    lines += [
        "",
        "## SpriteSheet 32px layout",
        "",
    ]
    for band, desc in SPRITESHEET_LAYOUT["rows"].items():
        lines.append(f"- **rows {band}:** {desc}")
    lines += [
        "",
        "## Attack `sword_grip` (32px Separate/Attack — use for weapons)",
        "",
        "| Dir | f0 | f1 | f2 | f3 |",
        "|-----|----|----|----|----|",
    ]
    atk = catalog["separate"]["Attack"]["frames"]
    for d in DIRS:
        grips = []
        for f in range(4):
            g = atk[d][f]["sword_grip"]
            grips.append("—" if g is None else f"`({g[0]},{g[1]})`")
        lines.append(f"| {d} | " + " | ".join(grips) + " |")
    lines += [
        "",
        "## All Separate Attack hands (every cluster)",
        "",
    ]
    for d in DIRS:
        for fr in atk[d]:
            hands = ", ".join(f"{h['id']}@({h['centroid'][0]},{h['centroid'][1]})" for h in fr["hands"])
            feet = ", ".join(f"{ft['id']}@({ft['centroid'][0]},{ft['centroid'][1]})" for ft in fr["feet"])
            lines.append(
                f"- **{d}f{fr['frame']}** sword_grip={fr['sword_grip']} hand={fr['sword_hand']} — hands: [{hands}] feet: [{feet}]"
            )
    lines += [
        "",
        "Weapon seating in `scripts/gen_akari.py` MUST read Attack `sword_grip` from this catalog.",
        "",
    ]
    with open(OUT_MD, "w") as f:
        f.write("\n".join(lines) + "\n")
    print("wrote", OUT_MD)


def main():
    os.makedirs(OUT_VIS, exist_ok=True)

    # --- Full SpriteSheet (32px CharacterAnimated) ---
    ss_path = os.path.join(NG32, "SpriteSheet.png")
    ss_sheet = scan_grid(
        ss_path,
        "assets/ninja-adventure/Actor/CharacterAnimated/NinjaGreen/SpriteSheet.png",
        "8 cols × 17 rows × 32px — see spritesheet_layout",
        anim_name="SpriteSheet",
    )
    ss_sheet["spritesheet_layout"] = SPRITESHEET_LAYOUT
    print(f"SpriteSheet32: {ss_sheet['n_occupied']}/{ss_sheet['n_cells']} occupied")

    # --- Every Separate sheet (32px) ---
    separate = {}
    for fn in sorted(os.listdir(SEP32)):
        if not fn.endswith(".png"):
            continue
        path = os.path.join(SEP32, fn)
        im = Image.open(path)
        if im.size[0] % CEL or im.size[1] % CEL:
            print("skip non-32 multiple", fn, im.size)
            continue
        key = fn.replace(".png", "")
        ncols = im.size[0] // CEL
        layout = f"{ncols} cols × {im.size[1]//CEL} rows × 32px; COL=dir when 4-col else see Jump/Climb/etc"
        separate[key] = scan_grid(
            path,
            f"assets/ninja-adventure/Actor/CharacterAnimated/NinjaGreen/Separate/{fn}",
            layout,
            anim_name=key,
        )
        print(f"Separate32/{key}: {separate[key]['n_occupied']}/{separate[key]['n_cells']}")

    build_spritesheet_crossref(separate, ss_sheet)

    occupied32 = ss_sheet["n_occupied"] + sum(s["n_occupied"] for s in separate.values())

    # --- Character/NinjaGreen 16px (standard character sheet + SeparateAnim) ---
    # Reuse analyze with scaled foot thresholds via temporary CEL monkeypatch avoided —
    # 16px cells: foot if y >= 11 near center.
    def analyze_cell_16(cell_a):
        pts = [
            (int(x), int(y))
            for y in range(cell_a.shape[0])
            for x in range(cell_a.shape[1])
            if tuple(cell_a[y, x, :3]) == MITT and cell_a[y, x, 3] > 200
        ]
        raw = []
        for cl in connected_components(pts):
            for part in split_wide(cl, max_span=3):
                xs = [p[0] for p in part]
                ys = [p[1] for p in part]
                raw.append(
                    {
                        "pixels": [[x, y] for x, y in sorted(part)],
                        "n": len(part),
                        "centroid": [int(round(sum(xs) / len(xs))), int(round(sum(ys) / len(ys)))],
                        "bbox": [min(xs), min(ys), max(xs), max(ys)],
                    }
                )
        hands, feet = [], []
        for cl in raw:
            cx, cy = cl["centroid"]
            role = "foot" if (cy >= 11 and 4 <= cx <= 11) or cy >= 13 else "hand"
            entry = {**cl, "role": role}
            (hands if role == "hand" else feet).append(entry)
        hands.sort(key=lambda h: (h["centroid"][0], h["centroid"][1]))
        feet.sort(key=lambda h: h["centroid"][0])
        for i, h in enumerate(hands):
            h["id"] = f"hand{i}"
        for i, ft in enumerate(feet):
            ft["id"] = f"foot{i}"
        return hands, feet

    def scan_grid_16(path, rel_file, layout_note, anim_name):
        im = Image.open(path).convert("RGBA")
        a = np.array(im)
        w, h = im.size
        cell = 16
        assert w % cell == 0 and h % cell == 0, (path, w, h)
        ncols, nrows = w // cell, h // cell
        frames_by_dir = {}
        cells = []
        empty = 0
        for r in range(nrows):
            for c in range(ncols):
                cell_a = a[r * cell : (r + 1) * cell, c * cell : (c + 1) * cell]
                opaque = int((cell_a[:, :, 3] > 200).sum())
                if opaque < 3:
                    empty += 1
                    cells.append({"col": c, "row": r, "empty": True})
                    continue
                hands, feet = analyze_cell_16(cell_a)
                dlab = dir_label(ncols, c)
                sid = pick_sword_hand(dlab, r, hands, anim_name)
                grip = next((h["centroid"] for h in hands if h["id"] == sid), None) if sid else None
                rec = {
                    "col": c,
                    "row": r,
                    "dir": dlab,
                    "frame": r,
                    "empty": False,
                    "opaque": opaque,
                    "hands": hands,
                    "feet": feet,
                    "sword_hand": sid,
                    "sword_grip": grip,
                }
                cells.append(rec)
                if ncols in (1, 4) or anim_name != "SpriteSheet16":
                    frames_by_dir.setdefault(dlab, []).append(
                        {
                            "frame": r,
                            "hands": hands,
                            "feet": feet,
                            "sword_hand": sid,
                            "sword_grip": grip,
                        }
                    )
        sheet = {
            "file": rel_file,
            "dims": [w, h],
            "layout": layout_note,
            "cell_px": 16,
            "n_cols": ncols,
            "n_rows": nrows,
            "n_cells": ncols * nrows,
            "n_occupied": ncols * nrows - empty,
            "n_empty": empty,
            "cells": cells,
        }
        if frames_by_dir:
            sheet["frames"] = frames_by_dir
            sheet["n_frames"] = nrows
            sheet["n_dirs"] = ncols
        return sheet

    char16 = {}
    ss16_path = os.path.join(NG16, "SpriteSheet.png")
    char16["SpriteSheet"] = scan_grid_16(
        ss16_path,
        "assets/ninja-adventure/Actor/Character/NinjaGreen/SpriteSheet.png",
        "4 cols × 7 rows × 16px (standard character: walk/idle/attack/jump rows)",
        "SpriteSheet16",
    )
    print(f"SpriteSheet16: {char16['SpriteSheet']['n_occupied']}/{char16['SpriteSheet']['n_cells']}")
    for fn in sorted(os.listdir(SEP16)):
        if not fn.endswith(".png"):
            continue
        path = os.path.join(SEP16, fn)
        im = Image.open(path)
        if im.size[0] % 16 or im.size[1] % 16:
            continue
        key = fn.replace(".png", "")
        char16[key] = scan_grid_16(
            path,
            f"assets/ninja-adventure/Actor/Character/NinjaGreen/SeparateAnim/{fn}",
            f"{im.size[0]//16} cols × {im.size[1]//16} rows × 16px",
            key,
        )
        print(f"Separate16/{key}: {char16[key]['n_occupied']}/{char16[key]['n_cells']}")

    occupied16 = sum(s["n_occupied"] for s in char16.values())

    catalog = {
        "character": "NinjaGreen",
        "mitt_rgb": list(MITT),
        "note": (
            "COMPLETE per-cell hand/mitt index for BOTH NinjaGreen variants: "
            "CharacterAnimated (32px SpriteSheet 8×17 + all Separate/) AND "
            "Character (16px SpriteSheet 4×7 + all SeparateAnim/). "
            "Color (239,145,79) = gloves AND shoes. "
            "Regenerate: python3 scripts/index_ninja_green_hands.py"
        ),
        "totals": {
            "character_animated_32px": {
                "spritesheet_cells": ss_sheet["n_cells"],
                "spritesheet_occupied": ss_sheet["n_occupied"],
                "separate_sheets": len(separate),
                "separate_occupied": sum(s["n_occupied"] for s in separate.values()),
                "occupied_cells": occupied32,
            },
            "character_16px": {
                "sheets": len(char16),
                "occupied_cells": occupied16,
            },
            "occupied_cells": occupied32 + occupied16,
        },
        "character_animated_32px": {
            "group": "CharacterAnimated",
            "spritesheet": ss_sheet,
            "separate": separate,
        },
        "character_16px": {
            "group": "Character",
            "sheets": char16,
        },
        # Back-compat for gen_akari load_katana_attack()
        "sheets": {
            k: {"frames": v["frames"], "file": v["file"], "n_frames": v.get("n_frames", v["n_rows"])}
            for k, v in separate.items()
            if "frames" in v
        },
        "spritesheet": ss_sheet,
        "separate": separate,
    }

    with open(OUT_CAT, "w") as f:
        json.dump(catalog, f, indent=2)
    print("wrote", OUT_CAT, "bytes", os.path.getsize(OUT_CAT))
    write_markdown(catalog)

    write_visual_sheet("SpriteSheet", ss_sheet, scale=2)
    for name, sheet in separate.items():
        write_visual_sheet(name, sheet, scale=4 if sheet["n_cols"] <= 4 else 3)
    write_visual_sheet("Character16_SpriteSheet", char16["SpriteSheet"], scale=4)
    for name, sheet in char16.items():
        if name == "SpriteSheet":
            continue
        write_visual_sheet(f"Character16_{name}", sheet, scale=6)

    print("\n=== Attack sword_grip ===")
    for d in DIRS:
        for fr in separate["Attack"]["frames"][d]:
            print(f"  {d}f{fr['frame']}: grip={fr['sword_grip']} hands={[h['centroid'] for h in fr['hands']]}")
    print(f"\nTOTAL occupied cells: {occupied32 + occupied16} (32px={occupied32}, 16px={occupied16})")


if __name__ == "__main__":
    main()
