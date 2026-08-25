# Assets

Both files are drawn by `scripts/gen_pinball.py` — nothing here comes from the Ninja Adventure pack.

`scripts/gen_golf_art.py` settled the rule this follows: the pack has nothing that reads as a steel
sphere at 8px (a seed or a nut at that size is a brown smudge, which is worse than a drawn circle),
and a flipper is not a thing any tile pack contains.

| file | what |
|---|---|
| `ball.png` | `sheet8:` — one 8x8 steel ball, a shaded disc with a specular highlight |
| `flipper.png` | `sheet32:` — five 32x32 frames sweeping a tapered flipper from rest to raised, pivot at the left end. The right flipper is the same sheet drawn with `sprite_set_flip`. |

The table itself is not art at all: it is per-pixel `terrain_*`, drawn at boot as runs of overlapping
discs (see `buildTable` in `src/main.tish`).

The font is `assets/fonts/tinypixel.ttf`; see its licence beside it.
