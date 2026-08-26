# RPG MENU

> *Demonstrates nested UI menus for RPGs.*

<img src="preview.gif" alt="preview" width="480">

The RPG **"gear up" loop** — inventory, equipment, and a shop — and the first real consumer of the
`packages/ui.tish` layout engine. Each screen is a declarative flex **tree**; the engine lays it out
and renders (text to the background canvas, icons through its sprite pool). Selection is a cursor
sprite placed from the selected cell's computed geometry.

Runs an **attract-mode self-play** by default (equip gear, drink a potion, visit the shop, buy and
equip new gear — watch `ATK/DEF/SPD/HP` update live). Set `SELF_PLAY = 0` in `src/main.tish` to drive
it by hand:

- **D-pad** move the cursor · **A** use/equip (or buy in the shop) · **B** sell (or back)
- **Select** toggle Bag ↔ Gear focus · **Start** toggle Inventory ↔ Shop

## Modules (the reusable split)
- `src/items.tish` — the item **database** (content): stats, slot, price, icon, description.
- `src/rpg.tish` — the **model**: bag, equipment slots, derived stats, gold, `activate`/`unequip`/`buy`/`sell`. Game-agnostic, no rendering.
- `src/menu.tish` — local cursor helper (shop list); bag/gear use `makeGrid` from `packages/ui.tish`.
- `src/main.tish` — the **screens**: builds the UI trees from model state, handles input + self-play. Menus are cold, so it only re-lays-out on change (a "dirty" flag), not every frame.

Art (`scripts/gen_rpg_menu.py`) is composited entirely from the vendored Ninja Adventure catalog:
the Skill-Icon equipment set (24×24, colour-coded purple=weapon / orange=armor / blue=accessory)
packed into a 32×32 sheet.

```bash
python3 ../../scripts/gen_rpg_menu.py   # (re)generate art from the catalog
npm run build && npm run shot
```
