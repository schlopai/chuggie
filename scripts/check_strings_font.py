#!/usr/bin/env python3
"""Does this font actually contain every character this string table uses?

    python3 scripts/check_strings_font.py examples/polyglot/assets/ui.strings assets/fonts/ark-pixel-cjk.ttf

## Why this is a separate check

A missing glyph and an uncollected glyph look IDENTICAL on a GBA screen, and they have opposite
fixes:

  * **a blank gap** — the character was never baked, because `font:` collects its glyph set from the
    program's string LITERALS and a `.strings` table is data. Fix: regenerate the roster with
    `scripts/gen_strings_glyphs.py`.
  * **a tofu box** — the character was baked and the FONT does not have it. No amount of charset
    work will help; change the font or change the word.

This script answers the second question directly, before anyone spends an afternoon on the first.
It found that every vendored `ark-pixel` face lacks 旅 and 薬 — which is why `examples/polyglot`
writes those two words in kana, as retro Japanese games did for the same reason.
"""
import sys, pathlib
from fontTools.ttLib import TTFont
strings, font = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
t = TTFont(str(font)); cm=set()
for tb in t['cmap'].tables: cm |= set(tb.cmap)
missing = {}
for line in strings.read_text(encoding='utf-8').splitlines():
    l=line.strip()
    if not l or l.startswith('#') or (l.startswith('[') and l.endswith(']')): continue
    for ch in l:
        if ord(ch) >= 0x7F and ord(ch) not in cm: missing.setdefault(ch, 0); missing[ch]+=1
print(f"{font.name}: {'MISSING ' + ''.join(sorted(missing)) if missing else 'covers every character'}")
