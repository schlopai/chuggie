# SerenityOS emoji (vendored)

The [SerenityOS](https://github.com/SerenityOS/serenity) emoji set — ~1,750 tiny (~10px) pixel-art
PNGs, one per emoji, named by codepoint. Vendored here so the GBA toolchain can bake the emoji a game
uses directly from pixels, with **no font/COLR rasterization** in the pipeline.

- **License:** BSD-2-Clause — see [`LICENSE.txt`](LICENSE.txt). © 2019–2026 the SerenityOS developers.
- **Naming:** `U+<HEX>.png` — uppercase hex, no zero-padding (`U+1F600.png`, `U+A9.png`). Multi-codepoint
  sequences (ZWJ / flags / keycaps) use `_`-joined parts (`U+1F468_U+200D_U+1F4BB.png`).
- **Format:** each PNG is native ~10px pixel-art with a handful of colours (≤15) and an alpha channel —
  i.e. already essentially a GBA colour sprite.

## Why PNGs, not a `.ttf`

SerenityOS ships these as a colour (COLR/CPAL) font, which fontdue / swash / skrifa all struggle to
rasterize correctly — and the point size we want is exactly the size the source pixel-art already is.
The PNGs are the higher-fidelity, lower-friction source for a build-time asset baker: the filename is
the codepoint (trivial selective baking), the art needs no resampling at ~10–16px, and the colours fit
a GBA sprite palette as-is.

## Using them

Don't reference PNGs directly — `import { emoji } from 'emoji:../…/assets/emoji/serenity'` in a tish
game. The `emoji:` scheme (`tish-agb`) bakes **only the emoji your strings use** into one sprite strip
via `tish_gba_scenepack::include_emoji_used!` and registers it as a global fallback for every font. See
[`examples/fonts-demo`](../../../examples/fonts-demo). Currently single-codepoint emoji render; ZWJ
sequences degrade to their base characters.
