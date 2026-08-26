# BG DEMO

> *Demonstrates background layers, tilemaps, and scrolling.*

<img src="preview.gif" alt="preview" width="480">

A world screen in tish: a tiled **background**, a **sprite** walking over it (facing
its direction), and a **sound effect** on the A button. Exercises three asset scheme
types, all contributed by tish-agb (no tish-compiler edits):

- `background:../assets/grass.png` — a full-screen (≥240×160) image baked into the ROM as tiles (agb `include_background_gfx`), shown via `bg_new`
- `asset:../assets/player.png` — a sprite (`sprite_new`, `sprite_set_flip`)
- `wav:../assets/blip.wav` — a sound (`sound_play`)

Controls: **d-pad** walks the hero (it flips to face left/right); **A** plays the blip.

Build: `npm run build`; boot in mgba.
