#!/usr/bin/env python3
"""index_group.py — structural index of a flat-ish asset group (Items / FX / Ui) into
catalog/<group>.json + <group>.md. Records every png with its category (subfolder path),
dimensions, and a coarse type guess. Names + folders in this pack are descriptive, so the
structural index is self-documenting; combined sheets get a vision pass separately.

    python3 scripts/index_group.py Items
    python3 scripts/index_group.py FX
    python3 scripts/index_group.py Ui
"""
import os, json, sys
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def guess_type(w, h):
    if w == 16 and h == 16:
        return "icon-16"
    if w == h:
        return f"icon-{w}"
    if h == 16 and w % 16 == 0:
        return f"strip-h({w // 16})"
    if w == 16 and h % 16 == 0:
        return f"strip-v({h // 16})"
    if w % 16 == 0 and h % 16 == 0:
        return f"sheet-16({w // 16}x{h // 16})"
    return "irregular"


def main(group):
    base = os.path.join(ROOT, "assets/ninja-adventure", group)
    entries = []
    for root, _, files in os.walk(base):
        for f in sorted(files):
            if not f.lower().endswith(".png"):
                continue
            p = os.path.join(root, f)
            try:
                w, h = Image.open(p).size
            except Exception:
                w, h = -1, -1
            rel = os.path.relpath(p, os.path.join(ROOT, "assets/ninja-adventure"))
            cat = os.path.relpath(root, base)
            entries.append({
                "file": rel, "name": os.path.splitext(f)[0],
                "category": cat if cat != "." else group,
                "dims": [w, h], "type": guess_type(w, h),
            })
    entries.sort(key=lambda e: e["file"])
    json.dump({"group": group, "count": len(entries), "assets": entries},
              open(os.path.join(ROOT, f"assets/ninja-adventure/catalog/{group.lower()}.json"), "w"), indent=1)

    # markdown: grouped by category
    from collections import defaultdict, Counter
    bycat = defaultdict(list)
    for e in entries:
        bycat[e["category"]].append(e)
    lines = [f"# Ninja Adventure — {group} catalog", "",
             f"{len(entries)} assets across {len(bycat)} categories. Type key: `icon-N`=NxN sprite, "
             "`strip-h(N)`=N-frame horizontal strip, `sheet-16(CxR)`=C×R grid of 16px frames, "
             "`irregular`=inspect dims.", ""]
    types = Counter(e["type"].split("(")[0] for e in entries)
    lines.append("**Types:** " + ", ".join(f"{k}×{v}" for k, v in types.most_common()))
    lines.append("")
    for cat in sorted(bycat):
        lines.append(f"## {cat} ({len(bycat[cat])})")
        lines.append("")
        lines.append("| Name | Dims | Type |")
        lines.append("|---|---|---|")
        for e in bycat[cat]:
            lines.append(f"| {e['name']} | {e['dims'][0]}×{e['dims'][1]} | {e['type']} |")
        lines.append("")
    open(os.path.join(ROOT, f"assets/ninja-adventure/catalog/{group.lower()}.md"), "w").write("\n".join(lines))
    print(f"{group}: {len(entries)} assets, {len(bycat)} categories, types={dict(types)}")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "Items")
