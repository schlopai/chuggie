# Assets

All art is from the **Ninja Adventure Asset Pack** by *Pixel-Boy* and *AAA*, released **CC0**
(public domain). It is vendored in this repository at `assets/ninja-adventure/` — see
`assets/ninja-adventure/LICENSE` and the pixel-verified index at
`assets/ninja-adventure/catalog/`.

> https://pixel-boy.itch.io/ninja-adventure-asset-pack

Nothing here is hand-placed. Both files are emitted by a script, so the pack stays the source:

| file | made by | from |
|---|---|---|
| `heroes.png` | `scripts/gen_jrpg_party.py` | `Actor/Character/{Knight,SorcererBlack,Shaman,Hunter}/SeparateAnim/{Idle,Attack,Dead}.png` |
| `foes.png` | ″ | `Actor/Monster/{Cyclope,Flam,Dragon,Beast}/` row 0, columns 0-1 |

Both sheets are cells of the pack's own 16x16 art doubled to 32x32 with NEAREST — a 16px figure on
a 240x160 battle screen reads as a bug — and each sheet is quantised to 15 colours **as an assembled
strip**, so it occupies exactly one of the GBA's sixteen palette banks.

The font is `assets/fonts/tinypixel.ttf`; see its licence beside it.
