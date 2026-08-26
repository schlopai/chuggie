#!/usr/bin/env python3
"""Set the display width of every committed preview embed in the example READMEs.

The previews are committed at native 240x160 — one GBA pixel per pixel, the smallest the files can
be. How big they APPEAR is a display decision, so it lives in the markup, at one width shared with
the generated index (PREVIEW_WIDTH in scripts/gen_examples_readme.py) rather than being baked into
57 committed binaries.

Markdown's ![alt](src) cannot carry a width, so embeds are rewritten as <img>, keeping their alt
text. Idempotent — re-run after changing PREVIEW_WIDTH.

Usage: python3 scripts/set_preview_width.py [--check]
"""
import glob
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_examples_readme import PREVIEW_WIDTH

MD_EMBED = re.compile(r"!\[([^\]]*)\]\((preview\.(?:gif|png))\)")
HTML_EMBED = re.compile(r'<img\s+src="(preview\.(?:gif|png))"([^>]*)>')


def rewrite(text):
    def from_md(m):
        alt, src = m.group(1), m.group(2)
        alt_attr = f' alt="{alt}"' if alt else ""
        return f'<img src="{src}"{alt_attr} width="{PREVIEW_WIDTH}">'

    def from_html(m):
        src, rest = m.group(1), m.group(2)
        # Keep any alt already there; replace whatever width/height was set.
        alt = re.search(r'\salt="[^"]*"', rest)
        return f'<img src="{src}"{alt.group(0) if alt else ""} width="{PREVIEW_WIDTH}">'

    return HTML_EMBED.sub(from_html, MD_EMBED.sub(from_md, text))


def main():
    check = "--check" in sys.argv
    stale = []
    for path in sorted(glob.glob("examples/*/README.md")):
        text = open(path, encoding="utf-8").read()
        new = rewrite(text)
        if new == text:
            continue
        stale.append(path)
        if not check:
            open(path, "w", encoding="utf-8").write(new)
    if check:
        for p in stale:
            print(f"stale preview width: {p}")
        print(f"ok   {len(stale)} example README(s) need updating" if stale
              else f"ok   every example README embed is at width={PREVIEW_WIDTH}")
        return 1 if stale else 0
    print(f"set width={PREVIEW_WIDTH} on {len(stale)} example README(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
