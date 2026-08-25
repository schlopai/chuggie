#!/usr/bin/env python3
"""Generate `examples/README.md` — the index of every example — from the examples themselves.

Each example owns its own description; this only collects them, so the index cannot drift from the
thing it describes. For every `examples/<name>/` it reads:

  README.md    `# TITLE` on the first heading, the `> *tagline*` blockquote under it, and the
               FIRST image it embeds — whatever the example chose to show

Usage:
    python3 scripts/gen_examples_readme.py            # write examples/README.md
    python3 scripts/gen_examples_readme.py --check    # exit 1 if it is stale or a README is missing

`--check` is what CI runs. It fails on two things:
  * an example with no README.md — the index cannot describe what nobody documented, and an
    undocumented example is one nobody can find;
  * a stale index — someone edited an example's README without re-running this.

⚠️⚠️ `screenshot.png` IS GITIGNORED (`**/screenshot.png`) — it is what `npm run shot` writes to prove
a ROM boots and renders, and it is never committed. For a long time 42 example READMEs embedded it,
which means those images were BROKEN on GitHub: the file existed on the author's disk and nowhere
else. Those shots were good, so they were renamed to `preview.png` (not ignored, committed) and the
READMEs re-pointed at them.

So: `preview.png` — or a hand-named shot like `asteroids/arena.png` or `solitaire/table.png` — is a
showcase image and belongs in the index. `screenshot.png` is a build artifact and is only used here
if an example has literally nothing else, which should be treated as a bug in that example.

A missing image is NOT a hard failure: capturing a good one needs a built ROM, an emulator and
judgement about the moment. It is listed in the index instead, so the gap is visible and tracked
rather than silently absent.
"""
import argparse
import json
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXDIR = os.path.join(REPO, "examples")
OUT = os.path.join(EXDIR, "README.md")

# Examples whose whole purpose is to reproduce a bug or measure a number. They are real and they are
# kept, but they are not things to go and play, so they get their own section at the bottom.
DIAGNOSTIC = ("bench-", "repro-", "p0-", "probe-")


IMG_RE = re.compile(r"!\[[^\]]*\]\(([^)\s]+)\)")


def describe(path):
    """(title, tagline, image) from an example's README, falling back to the directory name."""
    name = os.path.basename(path)
    readme = os.path.join(path, "README.md")
    if not os.path.exists(readme):
        return None
    title, tagline = name, ""
    with open(readme, encoding="utf-8") as fh:
        text = fh.read()
    m = re.search(r"^#\s+(.+?)\s*$", text, re.M)
    if m:
        title = m.group(1).strip()
    # `> *A one-line description.*` — the convention every example README already follows.
    m = re.search(r"^>\s*\*(.+?)\*\s*$", text, re.M)
    if m:
        tagline = m.group(1).strip()
    else:
        m = re.search(r"^>\s*(.+?)\s*$", text, re.M)
        if m:
            tagline = m.group(1).strip()
    # ⚠️ Fall back to package.json's `description` rather than printing the title twice. Nine
    # READMEs had no `> *tagline*` blockquote at all (rap-dojo, kart-circuit, versus, solitaire …)
    # and the index listed them as "*(rap-dojo)*" — a row that says nothing. Every one of them had
    # a perfectly good one-line description sitting in its package.json. Read what exists before
    # demanding a format.
    if not tagline:
        pkg = os.path.join(path, "package.json")
        if os.path.exists(pkg):
            try:
                with open(pkg, encoding="utf-8") as fh:
                    tagline = (json.load(fh).get("description") or "").strip()
            except (ValueError, OSError):
                tagline = ""
    # The image the README embeds — its author's choice of what this example looks like.
    # ⚠️ A CHOSEN shot beats `screenshot.png` even when screenshot.png is embedded FIRST: several
    # READMEs lead with the CI validation frame and then show the good ones (`shmup` does), so
    # "first image wins" would still put a validation artifact on the index. screenshot.png is only
    # used when it is the only image there is.
    local = [c for c in IMG_RE.findall(text)
             if not c.startswith("http") and os.path.exists(os.path.join(path, c))]
    chosen = [c for c in local if os.path.basename(c) != "screenshot.png"]
    img = chosen[0] if chosen else (local[0] if local else None)
    # ⚠️ Last resort: a `preview.png` sitting on disk that the README never embeds. 17 examples were
    # in exactly that state — the image was there, committed and good, and both the example's own
    # page and this index showed nothing, because the index only reads what a README references.
    # An image the repo already has should never be invisible.
    if img is None and os.path.exists(os.path.join(path, "preview.png")):
        img = "preview.png"
    return title, tagline, img


def collect():
    rows, missing_readme = [], []
    for name in sorted(os.listdir(EXDIR)):
        path = os.path.join(EXDIR, name)
        if not os.path.isdir(path):
            continue
        d = describe(path)
        if d is None:
            missing_readme.append(name)
            continue
        title, tagline, img = d
        rows.append((name, title, tagline, img))
    return rows, missing_readme


def table(rows):
    out = ["| | example | what it is |", "|---|---|---|"]
    for name, title, tagline, shot in rows:
        img = f'<img src="{name}/{shot}" width="140">' if shot else "—"
        desc = tagline or f"*({title})*"
        out.append(f"| {img} | **[{title}]({name}/README.md)**<br>`{name}` | {desc} |")
    return "\n".join(out)


def render(rows):
    games = [r for r in rows if not r[0].startswith(DIAGNOSTIC)]
    diags = [r for r in rows if r[0].startswith(DIAGNOSTIC)]
    noshot = [r[0] for r in games if not r[3]]
    body = [
        "<!-- GENERATED BY scripts/gen_examples_readme.py — DO NOT EDIT.",
        "     Edit the individual example's README.md instead, then re-run the script. -->",
        "",
        "# Examples",
        "",
        f"{len(rows)} example ROMs. Each directory is a self-contained npm workspace: "
        "`npm run build` makes the `.gba`, `npm start` opens it in mGBA, `npm run shot` takes a "
        "headless screenshot.",
        "",
        "Every description below is taken from that example's own README, so this page cannot drift "
        "away from what it lists.",
        "",
        "## Games and demos",
        "",
        table(games),
        "",
    ]
    if noshot:
        body += [
            "<details><summary>"
            f"{len(noshot)} of these show no image in their README yet</summary>",
            "",
            "The thumbnail above is whatever image an example's own README embeds. Capturing a good "
            "one needs a built ROM, an emulator and a judgement call about the moment, so it is done "
            "by hand (`npm run shot` gets you a frame; choosing a *good* frame is the work). "
            "`asteroids/arena.png` and `solitaire/table.png` are the pattern to follow — note that "
            "`screenshot.png` is a CI validation artifact, not a chosen shot. Still missing one:",
            "",
            "".join(f"`{n}` · " for n in noshot).rstrip(" ·"),
            "",
            "</details>",
            "",
        ]
    body += [
        "## Benchmarks, probes and regression repros",
        "",
        "Not things to play — these measure something or reproduce a specific bug.",
        "",
        table(diags),
        "",
    ]
    return "\n".join(body)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="verify instead of writing (CI)")
    args = ap.parse_args()

    rows, missing = collect()
    text = render(rows)

    if args.check:
        problems = []
        if missing:
            problems.append(
                "examples with no README.md: " + ", ".join(missing)
                + "\n  every example needs one — the index is generated from them"
            )
        current = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else None
        if current != text:
            problems.append(
                "examples/README.md is out of date\n  run: python3 scripts/gen_examples_readme.py"
            )
        if problems:
            for p in problems:
                print("FAIL " + p, file=sys.stderr)
            return 1
        print(f"ok   examples/README.md is current ({len(rows)} examples)")
        return 0

    if missing:
        print("WARNING: no README.md in: " + ", ".join(missing), file=sys.stderr)
    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(text)
    shots = sum(1 for r in rows if r[3])
    print(f"examples/README.md: {len(rows)} examples, {shots} with an image")
    return 0


if __name__ == "__main__":
    sys.exit(main())
