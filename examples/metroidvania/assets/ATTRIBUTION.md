# Art credits

Every source pack here is **CC0 1.0 Universal** (public domain dedication) by **Luis Zuno
(ansimuz)**. Credit is not required by the licence; it is recorded because the art is the reason
this example looks like anything.

The files in this folder are **not** the originals — `scripts/gen_metroidvania.py` rescales,
re-anchors, re-cuts and colour-clamps the source art into GBA sheets (15 colours per 4bpp sprite
sheet, fully opaque backgrounds, alpha hardened to 0/255). The raw packs are not vendored; they are
tens of MB of one-PNG-per-frame folders.

## Re-downloading the raw art

    mkdir -p ~/Downloads/metroidvania-art && cd ~/Downloads/metroidvania-art
    curl -O https://opengameart.org/sites/default/files/gothicvania-cemetery-files_1.zip
    curl -O "https://opengameart.org/sites/default/files/%20gothicvania%20patreon%20collection.zip"
    unzip -q gothicvania-cemetery-files_1.zip -d cemetery
    unzip -q " gothicvania patreon collection.zip" -d patreon

then `npm run assets` in this example.

| pack | licence | source | used for |
|---|---|---|---|
| GothicVania Cemetery | CC0 | [opengameart](https://opengameart.org/content/gothicvania-cemetery-pack) · [itch](https://ansimuz.itch.io/gothicvania-cemetery) | hero (19 poses), ghost, skeleton, hell-gato, death FX |
| GothicVania Patreon Collection | CC0 | [opengameart](https://opengameart.org/content/gothicvania-patreons-collection) | Old Dark Castle interior tileset + backdrop |

The heart HUD is drawn by the generator (`make_heart`) in the castle's own palette — the packs do
not ship a HUD, and taking one from another example in this repo is not allowed.
