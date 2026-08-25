# Assets

Everything in this directory is **generated** by `scripts/gen_kart_circuit.py`. Nothing is derived
from a vendored art pack, so there is no upstream licence to carry.

| File | Size | Import scheme | Notes |
|---|---|---|---|
| `track.png` | 512×512 | `affine:` | The circuit, drawn top-down. 153 unique 8×8 tiles, 11 colours. |
| `karts32.png` | 1024×32 | `sheet32:` | 4 racers × 8 headings, near tier. 13 colours. |
| `karts16.png` | 512×16 | `sheet:` | The same, far tier. |
| `items.png` | 48×16 | `sheet:` | Item box, banana, shell. 8 colours. |
| `race.deck` | — | `deck:` | 16-bar loop, four intensity stems (`scripts/gen_kart_circuit_music.py`). |

Fonts are referenced in place from `assets/fonts/` and are not copied here:
`kenney-high-square.ttf` and `tinypixel.ttf`, each with its own licence file beside it.

## Why generated rather than sourced

A Mode 7 racer needs its karts drawn **from behind, at several rotation angles**. A scan of itch.io's
free and CC0 racing assets found plenty of top-down car packs and modular road tiles, but nothing
with a vehicle at multiple headings in a behind-the-camera view — that is the one thing a billboard
racer cannot do without. (Sprite rips of commercial kart games exist on sprite sites; those are
their publisher's and are not usable here.)

The track is generated for a different reason: the **surface map the physics reads has to come from
the same source as the art**, or the two can drift apart and the game will tell you that you are on
grass while you are looking at tarmac. Both come out of one centre-line spline. See the README.

CC0 road-tile packs that *were* found, in case the track art is ever revisited and someone wants a
richer asphalt look — the surface map would still need generating alongside it:

- Racing Track Tiles by D8H — CC0, 10 modular top-down road tiles, SVG + PNG.
- Mini Pixel Pack 2 by GrafxKid — free, top-down arcade racing assets.
