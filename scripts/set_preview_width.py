#!/usr/bin/env python3
"""Point every example README at its committed preview, at one shared display width.

For each examples/<name>/ this makes the README embed whichever preview the directory actually has
— preview.gif where there is motion to show, preview.png otherwise — inserting the embed if there
is none, and REMOVING it if the example has no preview at all (bench-behav and bench-boot lost
theirs to an art-licensing problem, and a README pointing at a deleted file renders as a broken
image on GitHub).

The previews are committed at native 240x160 — one GBA pixel per pixel, the smallest the files can
be. How big they APPEAR is a display decision, so it lives in the markup at one width shared with
the generated index (PREVIEW_WIDTH in scripts/gen_examples_readme.py) rather than being baked into
80-odd committed binaries. Markdown's ![alt](src) cannot carry a width, so embeds are rewritten as
<img>, keeping their alt text.

Idempotent. Run it after scripts/gen_previews.js, or after changing PREVIEW_WIDTH.

Usage: python3 scripts/set_preview_width.py [--check]
"""
import glob
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_examples_readme import PREVIEW_WIDTH

MD_EMBED = re.compile(r"!\[([^\]]*)\]\(preview\.(?:gif|png)\)")
HTML_EMBED = re.compile(r'<img\s+src="preview\.(?:gif|png)"([^>]*)>')


def alt_of(attrs):
    m = re.search(r'\salt="([^"]*)"', attrs or "")
    return m.group(1) if m else ""


def embed(src, alt):
    alt_attr = f' alt="{alt}"' if alt else ""
    return f'<img src="{src}"{alt_attr} width="{PREVIEW_WIDTH}">'


def rewrite(text, target):
    if target is None:
        # No preview on disk: drop the embed and any blank line it left behind.
        text = MD_EMBED.sub("", text)
        text = HTML_EMBED.sub("", text)
        return re.sub(r"\n{3,}", "\n\n", text)

    if MD_EMBED.search(text) or HTML_EMBED.search(text):
        text = MD_EMBED.sub(lambda m: embed(target, m.group(1)), text)
        return HTML_EMBED.sub(lambda m: embed(target, alt_of(m.group(1))), text)

    # No embed yet — lead with the preview, under the title/tagline block.
    lines = text.split("\n")
    at = 1
    for i, line in enumerate(lines[:8]):
        if line.startswith("> "):
            at = i + 1
            break
        if line.startswith("# "):
            at = i + 1
    while at < len(lines) and lines[at].strip() == "":
        at += 1
    lines[at:at] = [embed(target, "preview"), ""]
    return "\n".join(lines)


def main():
    check = "--check" in sys.argv
    stale = []
    for path in sorted(glob.glob("examples/*/README.md")):
        d = os.path.dirname(path)
        target = next((p for p in ("preview.gif", "preview.png")
                       if os.path.exists(os.path.join(d, p))), None)
        text = open(path, encoding="utf-8").read()
        new = rewrite(text, target)
        if new == text:
            continue
        stale.append(path)
        if not check:
            open(path, "w", encoding="utf-8").write(new)
    if check:
        for p in stale:
            print(f"stale preview embed: {p}")
        print(f"{len(stale)} example README(s) need updating" if stale
              else f"ok   every example README points at its preview at width={PREVIEW_WIDTH}")
        return 1 if stale else 0
    print(f"updated {len(stale)} example README(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
