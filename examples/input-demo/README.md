# INPUT DEMO

> *Demonstrates advanced input handling and key state debouncing.*

<img src="preview.gif" alt="preview" width="480">

A small game written entirely in tish, exercising the current `tish-agb` surface:

- **assets** — `import { player } from 'asset:../assets/player.png'` bakes the PNG into the ROM
- **d-pad + buttons** — `input_x`/`input_y` axes, `key_pressed(3)` (START, edge-triggered)
- **sprite control** — `sprite_set_pos`, `sprite_set_flip` (faces its walking direction), `sprite_set_visible`
- **typed physics** — positions are `fixed`; `px = px + speed` compiles to native `Num<i32,8>` integer math (no FPU, no boxing)

Controls: **d-pad** moves the hero (it flips to face left/right); **START** toggles a trailing "ghost".

Build: `npm run build`. Boot `input-demo.gba` in mgba.

Notably, every capability here is a plain `tish-agb` binding (`cargo:`) or an `asset:` import — **no edits to the tish compiler** were needed to add buttons, flip, or visibility.
