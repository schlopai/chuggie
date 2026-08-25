#!/usr/bin/env python3
"""Fail on .deck values that the parser will silently discard.

THE BUG THIS EXISTS FOR
-----------------------
`crates/tish-gba-scenepack/src/deckpack.rs` reads scalar generator parameters like this:

    "arp_semis" | "arpSemis" => p.arp_semis = v.parse().unwrap_or(p.arp_semis),

`unwrap_or`. A value that does not parse is not an error — it leaves the default in place. Writing
`arp_semis 0,4,7` (a chord, which is what an arpeggio intuitively wants) parses as nothing and
falls back to 0, so `arp_rate` drives an arpeggio of zero semitones: silence where a musical part
was meant to be. The deck loads, the ROM builds, the track plays, and the only symptom is that it
sounds slightly thin.

Every scalar key below is one of those. This checks that what is written can actually be read.

WHAT THIS CATCHES, AND WHAT IT DOES NOT
---------------------------------------
Catches a non-numeric value on a key `deckpack.rs` parses as a number — the comma-list mistake and
its relatives (units, ranges, stray quotes).

Does NOT check musical sense, that a key exists at all, or that an enum-valued key holds a real
variant. It is a parse-compatibility lint, not a schema.

Usage:  python3 scripts/deck_lint.py [dir ...]      (default: every assets/music/* directory)
        python3 scripts/deck_lint.py --self-test
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Keys deckpack.rs parses with `.parse().unwrap_or(...)` into a number. Sourced by reading that
# file; if it grows a scalar key, add it here — the lint cannot discover them on its own.
INT_KEYS = {
    "arp_semis", "arpSemis", "arp_rate", "arpRate",
    "vib_rate", "vibRate", "vib_amt", "vibAmt",
    "vol", "env_step", "envStep", "len",
    "drop_semis", "dropSemis", "pitch_drop", "pitchDrop",
    "sweep_shift", "sweepShift", "sweep_period", "sweepPeriod",
    "noise_shift", "noiseShift", "noise_ratio", "noiseRatio",
    "layer", "bpm", "deck",
}
FLOAT_KEYS = {"drop_dec", "dropDec", "pitch_dec", "pitchDec", "a", "d", "s", "r"}

TOKEN = re.compile(r"([A-Za-z_][A-Za-z_0-9]*)\s+([^\s]+)")


def numeric(v: str, allow_float: bool) -> bool:
    try:
        float(v) if allow_float else int(v)
        return True
    except ValueError:
        return False


def scan_text(text: str, label: str):
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        code = line.split("#")[0].strip()
        if not code:
            continue
        for key, val in TOKEN.findall(code):
            if key in INT_KEYS and not numeric(val, False):
                out.append((n, key, val, "an integer"))
            elif key in FLOAT_KEYS and not numeric(val, True):
                out.append((n, key, val, "a number"))
    return out


def self_test() -> int:
    ok = True
    bad = scan_text("  gen arp_rate 24 arp_semis 0,4,7\n", "<test>")
    if any(k == "arp_semis" for _, k, _, _ in bad):
        print("  self-test ok   — caught the real bug: arp_semis 0,4,7")
    else:
        print("  self-test FAIL — did NOT catch arp_semis 0,4,7")
        ok = False
    if scan_text("  gen arp_rate 20 arp_semis 7\n", "<test>"):
        print("  self-test FAIL — false positive on a valid scalar")
        ok = False
    else:
        print("  self-test ok   — no false positive on arp_semis 7")
    if not scan_text("  gen type pulse duty 12_5 env_mode constant\n", "<test>"):
        print("  self-test ok   — enum-valued keys are left alone")
    else:
        print("  self-test FAIL — flagged a non-numeric key it does not own")
        ok = False
    return 0 if ok else 1


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    dirs = [Path(a) for a in sys.argv[1:]] or sorted(
        d for d in (ROOT / "assets" / "music").iterdir() if d.is_dir()
    )
    bad = 0
    files = 0
    for d in dirs:
        for deck in sorted(d.glob("*.deck")):
            files += 1
            for n, key, val, want in scan_text(deck.read_text(encoding="utf-8"), deck.name):
                rel = deck.relative_to(ROOT) if deck.is_absolute() else deck
                print(f"FAIL: {rel}:{n} `{key} {val}` — {key} must be {want}; "
                      f"deckpack.rs parses it with unwrap_or and will silently keep the default")
                bad += 1
    if bad:
        print(f"\n{bad} value(s) the parser would discard silently.")
        return 1
    print(f"deck_lint: {files} decks, every scalar value parses")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
