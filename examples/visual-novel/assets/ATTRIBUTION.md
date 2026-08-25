# Assets

Portraits are from the **Ninja Adventure Asset Pack** by *Pixel-Boy* and *AAA*, released **CC0**
(public domain), vendored at `assets/ninja-adventure/`.

> https://pixel-boy.itch.io/ninja-adventure-asset-pack

| file | made by | from |
|---|---|---|
| `faces32.png` | `scripts/gen_visualnovel.py` | `Actor/Character/{NinjaBlue,NinjaRed,Cavegirl}/Faceset.png`, each 38x38 centre-cropped to 32x32 |

One sheet, not three — three imported sheets would claim three of the GBA's sixteen sprite palette
banks to show three faces.

There is no room image. The location is drawn with `ui_rect` on the text canvas; see the note beside
`paintRoom` in `src/main.tish` for why.

The font is `assets/fonts/tinypixel.ttf`; see its licence beside it.
