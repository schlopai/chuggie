# Art credits

Every source pack here is **CC0 1.0 Universal** (public domain dedication). Credit is not required
by the licence; it is recorded because the art is the reason this example looks like anything.

The files in this folder are not the originals — `scripts/gen_versus.py` rescales, re-anchors,
re-cuts and colour-clamps the source art into GBA sprite sheets. The raw packs are not vendored;
download them to `~/Downloads/versus-art/` and re-run `npm run assets` to rebuild.

## Fighters — LuizMelo

| sheet | pack | source |
|---|---|---|
| `hero.png` | Martial Hero | https://luizmelo.itch.io/martial-hero |
| `hero2.png` | Martial Hero 2 | https://luizmelo.itch.io/martial-hero-2 |
| `hero3.png` | Martial Hero 3 | https://luizmelo.itch.io/martial-hero-3 |
| `warrior.png` | Medieval Warrior Pack | https://luizmelo.itch.io/medieval-warrior-pack |

Four packs by one artist, which is the only reason a single 24-pose frame table can describe all
four characters: they share an animation vocabulary (idle / run / jump / fall / attack ×3 / take
hit / death) and a drawing style. They do **not** share a scale — idle heights are 52, 56, 41 and
81 px — so the generator resamples each to a common 54 px.

## Stage — ansimuz

| file | pack | source |
|---|---|---|
| `stage.png` | Mountain Dusk Parallax Background (version A layers) | https://ansimuz.itch.io/mountain-dusk-parallax-background |

Its six layers are flattened into one opaque 256×256 image; the depth on screen comes from
`bg_bands` (per-scanline DMA), not from separate background layers. See `src/stage.tish`.

## Generated here

`spark.png` (hit / block / dust bursts) is drawn procedurally by `scripts/gen_versus.py`, and the
arena floor strip at the bottom of `stage.png` likewise — the Mountain Dusk pack is a platformer
backdrop and has no flat ground for fighters to stand on.
