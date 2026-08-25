#!/usr/bin/env python3
"""Find code that can open the UI canvas twice in one frame.

    python3 scripts/check_ui_begin.py            # every package and example
    python3 scripts/check_ui_begin.py packages/shop.tish

`ui_begin()` blanks the canvas and hands back the previous render's tiles — but that VRAM is not
actually returned until the frame commits, so a second `ui_begin` in the same frame asks for a whole
extra canvas and dies inside agb's tile allocator. `tish-agb` asserts on it rather than letting the
allocator fail, because the allocator's message points at agb instead of at the call that broke the
rule:

    ui_begin called twice in one frame: the previous canvas has not released its VRAM yet.
    Call ui_clear() and return, then ui_begin() on the next frame.

⚠️ THE ASSERTION NAMES THE RULE, NOT THE CALLER, and that is what makes this expensive. It took down
every game with a shop in it — shop-demo, iso-town, oakhollow — the moment the keeper's menu was
answered, and the crash reads as a bug in the UI library. It was one line in `packages/shop.tish`:
`enterTab` called `ui_begin()` to release the greeting's tiles and then reached `uiStreamBegin`,
which opens a canvas of its own. The fix is `ui_hide()`, which releases exactly those tiles, keeps
the background, and leaves the frame's one `ui_begin` for the render that needs it.

## What counts as opening a canvas

Directly `ui_begin`, and via `packages/ui.tish`: `uiRender`, `uiPaint`, `uiPaintBox`, `uiReplay`,
`uiReplayBegin`, `uiStreamBegin`, `uiStackPaint`. A local function that reaches any of those counts
too, transitively.

## What this reports, and what it deliberately does not

Only STRAIGHT-LINE pairs: two opens in one function body with no `return` between them. That is the
shape of the real bug. Two opens in an `if`/`else` are mutually exclusive and are not reported —
flagging them buries the real finding in dozens of state dispatchers (`partyUpdate` alone dispatches
fifteen screens, every one of which opens a canvas, and it is correct).

⚠️ It is a LINE scanner, not a parser. An `if (a) { openA() } else { openB() }` written across
several lines will be reported. Read the hit before changing anything: the question is always
whether both calls can run in the SAME frame.
"""
from __future__ import annotations

import pathlib
import re
import sys

OPEN_ROOT = {
    "ui_begin", "uiRender", "uiPaint", "uiPaintBox", "uiReplay",
    "uiReplayBegin", "uiStreamBegin", "uiStackPaint",
}
FN = re.compile(r"^\s*(?:export\s+)?function\s+([A-Za-z_][\w]*)\s*\(", re.M)
ROOT = pathlib.Path(__file__).resolve().parent.parent


def sources(args: list[str]) -> list[pathlib.Path]:
    if args:
        return [pathlib.Path(a) for a in args]
    out = list((ROOT / "packages").rglob("*.tish"))
    out += list((ROOT / "examples").rglob("src/*.tish"))
    # `.tish/` is the build directory, not source.
    return [p for p in out if ".tish/" not in str(p)]


def scan(path: pathlib.Path) -> list[tuple[str, str, str, str]]:
    # Strip line comments: this file's own prose mentions `ui_begin()` a dozen times, and so does
    # every file that documents the rule. A scanner that counts comments reports its own warnings.
    text = "\n".join(re.sub(r"//.*$", "", ln) for ln in path.read_text().splitlines())
    marks = [(m.start(), m.group(1)) for m in FN.finditer(text)]
    if not marks:
        return []
    marks.append((len(text), None))
    bodies = {marks[i][1]: text[marks[i][0]:marks[i + 1][0]] for i in range(len(marks) - 1)}

    # Which local functions reach an opener, to a fixed point.
    opens: set[str] = set()
    changed = True
    while changed:
        changed = False
        for name, body in bodies.items():
            if name in opens:
                continue
            called = set(re.findall(r"\b([A-Za-z_]\w*)\s*\(", body))
            if called & (OPEN_ROOT | opens):
                opens.add(name)
                changed = True

    hits = []
    for name, body in bodies.items():
        prev = None
        for line in body.splitlines():
            stripped = line.strip()
            # A `return` means the second open is unreachable from the first — but only a
            # STATEMENT-LEADING one.
            # ⚠️ Matching `return` anywhere on the line is what made the first version of this miss
            # the very bug it was written for: `enterTab` opens a canvas, then builds a selector
            # whose cell callback is `cell: (i) => { return cellFor(i) }`, and that `return` — inside
            # a closure that does not run here at all — broke the chain before `renderTab` was
            # reached. A checker that cannot find its own motivating case is worse than none.
            if stripped.startswith("return"):
                prev = None
                continue
            # …and so does a new conditional branch. This is what makes the report usable: a state
            # dispatcher like `partyUpdate` opens a canvas in each of fifteen `else if` arms, all of
            # them correct and mutually exclusive, and counting those buries the one real hit under
            # fifty. Only calls that run UNCONDITIONALLY after a previous open are reported.
            # A conditional line's calls are arms of the SAME decision — `{ X } else { Y }` — so
            # they neither chain with each other nor with what follows. Without this the fifteen-way
            # state dispatchers dominate the report and the one real hit is invisible.
            if re.match(r"(\}\s*)?else\b|if\s*\(", stripped):
                prev = None
                continue
            for call in re.findall(r"\b([A-Za-z_]\w*)\s*\(", line):
                if call == name or (call not in OPEN_ROOT and call not in opens):
                    continue
                if prev is not None:
                    hits.append((name, prev, call, line.strip()[:72]))
                prev = call
    return hits


def main() -> int:
    total = 0
    for p in sorted(sources(sys.argv[1:])):
        for name, a, b, line in scan(p):
            try:
                rel = p.relative_to(ROOT)
            except ValueError:
                rel = p          # a path outside the repo (a scratch copy under test)
            print(f"{rel}: {name}() opens twice — {a}() then {b}()")
            print(f"    {line}")
            total += 1
    if total:
        print(f"\n{total} straight-line pair(s). Each is a question, not a verdict: can both calls "
              f"run in the SAME frame? If yes, the earlier one usually wants ui_hide().")
    else:
        print("no straight-line double opens")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
