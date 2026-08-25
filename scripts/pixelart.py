#!/usr/bin/env python3
"""Halftone shading for GBA tile art — depth on a tiny palette, paid for at build time.

Import from any example's generator:

    from pixelart import halftone, ramp, DENSITIES

## Why

**Four colours is not four tones.** A checkerboard of two colours reads, at 16px on a lit screen, as
a value between them — so a 4-colour palette carries roughly seven apparent shades. That is where
depth comes from when you cannot afford another palette entry, and it is the whole reason
`examples/prismfall` can hold a hard four-colour limit and still have shaded platforms.

This is the CGA / 1-bit technique, and on this hardware it is usually the RIGHT one:

- **Alpha blending cannot do it.** `blend_alpha`'s top layer is sprites in AlphaBlending mode, so it
  shades sprites against backgrounds — it cannot shade one background against another.
- **Runtime per-pixel shading exists but is terrain-only.** `terrain_planet` (see
  `examples/warheads`) hashes coordinates to band a generated sphere on the per-pixel terrain layer.
  That is the tool for destructible, generated surfaces; this is the tool for TILES.
- Dither costs **nothing at runtime**. It is pixels in the atlas.

## ⚠️ Punch holes; do not paint a dark colour

Dither against the BACKDROP by cutting holes (`colour=None`), not by painting a dark value:

- painting one **spends a palette entry**, and
- it **freezes**: a game that swaps its palette rewrites the entries it knows about, and a colour
  baked into the art stays put while everything around it changes. In prismfall that rendered as
  pure black over every shaded surface.

A hole shows palette 0 — which IS the backdrop — so the shading recolours with the palette for free.
"""
from PIL import Image

# Ordered dither, as a fraction of pixels taking the second colour. Keyed by percentage so calling
# code reads as a value ("shade this 25%") rather than as a matrix.
DENSITIES = {
    12: lambda x, y: (x % 4 == 0) and (y % 4 == 0),
    25: lambda x, y: (x + y * 2) % 4 == 0,
    50: lambda x, y: (x + y) % 2 == 0,
    75: lambda x, y: (x + y * 2) % 4 != 0,
    88: lambda x, y: not ((x % 4 == 0) and (y % 4 == 0)),
}


def halftone(im, x0, y0, x1, y1, colour, density):
    """Dither a rect of `im` at `density`%. `colour=None` punches holes — usually what you want."""
    if density not in DENSITIES:
        raise ValueError(f"density {density} not in {sorted(DENSITIES)}")
    test = DENSITIES[density]
    px = im.load()
    w, h = im.size
    fill = (0, 0, 0, 0) if colour is None else tuple(colour) + (255,)
    for y in range(max(0, y0), min(h, y1 + 1)):
        for x in range(max(0, x0), min(w, x1 + 1)):
            if test(x, y):
                px[x, y] = fill


def ramp(im, x0, y0, x1, y1, colour, steps=(0, 25, 50, 75)):
    """A gradient down a rect: successive horizontal bands at increasing dither density.

    Four densities over a 16px tile is what makes a plain slab read as a lit surface falling into
    shadow — the single highest-value thing you can do to flat tile art on a small palette.
    """
    n = len(steps)
    span = max(1, (y1 - y0 + 1) // n)
    for i, d in enumerate(steps):
        yy0 = y0 + i * span
        yy1 = y1 if i == n - 1 else yy0 + span - 1
        if d:
            halftone(im, x0, yy0, x1, yy1, colour, d)


def check_colours(im, allowed, label="image"):
    """Fail loudly if art strays outside its palette. Cheap insurance in a generator."""
    used = {p[:3] for p in im.convert("RGBA").getdata() if p[3]}
    extra = used - {tuple(c) for c in allowed}
    if extra:
        raise SystemExit(f"{label} uses colours outside the palette: {sorted(extra)}")
    return used
