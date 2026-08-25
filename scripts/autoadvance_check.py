#!/usr/bin/env python3
"""Fail if any screen can advance itself without the player, outside attract mode.

THE RULE
--------
**Auto-advance is a DEBUG affordance. It must never run in front of a player.**

Every timer in this front end exists for one reason: a screen that waits for a human is a screen no
headless run can get past, and a mode behind one is a mode nothing verifies. That is a good reason,
and it does not survive contact with an actual player — a title that walks into the mode menu, a
mode select that picks for you, a pre-match exchange that reads its own dialogue, a result screen
that clears itself while you are still looking at your score.

So every one of them is gated: the timer fires ONLY while nobody has touched the pad.

    nobody has touched it  -> the whole game demos itself, title through dialogue into a match,
                              which every no-key verifier depends on
    anyone has touched it  -> nothing advances without them, ever

`dropAttract()` (0 once a human is playing) is the gate everywhere except `drop_shell.tish`, which
has the same latch locally as `SH.human`.

WHY THIS SCRIPT EXISTS
----------------------
This was reported four separate times and fixed four separate times, each as its own bug — the
Story stage reset, the mode starts, the shell's menus, the dialogue — because each fix was found by
grepping and each grep missed the next one. The last grep hid two remaining sites in the same file
it had just been used to fix. Hand-searching does not converge. This does.

WHAT IT CATCHES
---------------
A timer comparison (`X.t > 180`, `TK.hold < TALK_HOLD`, ...) on a line that decides whether to
advance, where the same condition is not gated on `dropAttract()` or `SH.human`.

An advance that is genuinely unconditional and correct — a fixed animation step, a cooldown that
does not skip player input — can say so on the line with `autoadvance-ok:` and a reason.

The gate names (`dropAttract()`, `SH.human`, `D.human`) are the ones the drop front end happens to
use; any project's latch can be added to `GATED`. Nothing else here is genre-specific — it is a lint
for "this screen has a timer and no proof a human isn't watching it".

Usage:  python3 scripts/autoadvance_check.py [--self-test] FILE...
        python3 scripts/autoadvance_check.py packages/drop*.tish
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# A timer being compared against a bound: `ST.t > 180`, `TK.hold < TALK_HOLD`, `PZ.wait > 90`.
TIMER = re.compile(
    r"\b[A-Z]{1,3}\.(t|hold|wait|timer|delay|idle)\b\s*(<|<=|>|>=)\s*"
    r"([0-9]+|[A-Z_]{3,}|[A-Z]{1,3}\.[a-z]+)"
)
# The gates that make a timer attract-only.
GATED = ("dropAttract()", "SH.human", "D.human", "G.human")
OPT_OUT = "autoadvance-ok:"


def scan(path: Path):
    out = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        code = line.split("//")[0]
        if OPT_OUT in line:
            continue
        m = TIMER.search(code)
        if not m:
            continue
        if any(g in code for g in GATED):
            continue
        out.append((n, f"`{m.group(0).strip()}` advances without checking attract mode", line.strip()))
    return out


def self_test() -> int:
    ok = True
    tmp = ROOT / "packages" / ".autoadvance_selftest.tish"

    def run(text):
        tmp.write_text(text, encoding="utf-8")
        try:
            return scan(tmp)
        finally:
            tmp.unlink()

    # Each of the four real bugs, in the form it actually shipped.
    cases = [
        ("the Story scene timer", "  if ((keys_edge() & (1 << BTN_START)) !== 0 || ST.t > 180) {"),
        ("the dialogue hold", "  if (pressed === 0 && TK.hold < TALK_HOLD) { return TALK_RUNNING }"),
        ("the shell menu timer", "  if ((k & (1 << BTN_START)) === 0 && SH.t <= SH.hold) { return SHELL_BROWSING }"),
        ("the puzzle wait", "  if (PZ.wait > 90 || (keys_edge() & (1 << BTN_START)) !== 0) {"),
    ]
    for name, snippet in cases:
        if run(snippet + "\n"):
            print(f"  self-test ok   — caught {name}")
        else:
            print(f"  self-test FAIL — did NOT catch {name}: {snippet.strip()}")
            ok = False

    # The gated forms must pass.
    for name, snippet in [
        ("dropAttract-gated", "  if ((keys_edge() & 8) !== 0 || (dropAttract() === 1 && ST.t > 180)) {"),
        ("SH.human-gated", "  if (SH.human === 1 || SH.t <= SH.hold) { return SHELL_BROWSING }"),
    ]:
        if run(snippet + "\n"):
            print(f"  self-test FAIL — false positive on the {name} form")
            ok = False
        else:
            print(f"  self-test ok   — no false positive on the {name} form")
    return 0 if ok else 1


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    # The files to scan are ARGUMENTS. They used to be a hardcoded list of five drop packages, with
    # a `if not path.exists(): continue` that made a missing file a silent pass — so the day those
    # packages moved to another repo this would have gone on printing a clean bill of health for
    # nothing at all. A checker that cannot find its subject must say so.
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    if not args:
        print("usage: autoadvance_check.py [--self-test] FILE...")
        print("       e.g. autoadvance_check.py packages/drop*.tish")
        return 2
    paths = [Path(a) for a in args]
    missing = [p for p in paths if not p.is_file()]
    if missing:
        for p in missing:
            print(f"FAIL: {p}: no such file")
        return 2

    bad = 0
    for path in paths:
        rel = path.as_posix()
        for n, why, text in scan(path):
            print(f"FAIL: {rel}:{n} {why}")
            print(f"      {text}")
            bad += 1
    if bad:
        print(f"\n{bad} screen(s) can advance without the player. Auto-advance is for HEADLESS RUNS")
        print("only — gate it on dropAttract() (or SH.human in the shell), or, if a timer really is")
        print(f'unconditional, say why on the line and mark it "{OPT_OUT}".')
        return 1
    print(f"autoadvance_check: {len(paths)} file(s), no screen advances without the player")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
