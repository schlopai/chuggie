# iso-tactics — isometric tactics prototype

<img src="preview.gif" alt="preview" width="480">

An isometric tactics battle: a height-mapped board authored in **Tiled**,
units with class stats, and the active unit's **move range** flooded and highlighted. The reusable
tactics logic lives in the **engine**; this example is the game on top. See the full feature roadmap
in the tactics plan.

## Split: engine vs example

- **Engine** (`tish_gba_game_engine`, `tac_*` API) — the reusable tactics core: a height-mapped grid
  with occupancy, **move-range flood fill** (Move budget + Jump height-delta, blocked by terrain and
  units), **pathfinding**, a **unit registry**, and a **speed-based turn queue**. Any tactics game
  uses these.
- **Example** (this dir) — the game: isometric rendering (depth-sorted 32×32 sprites), the turn
  loop, cursor + highlight, and **unit classes as components** in [`components.tish`](src/components.tish)
  — each class bundles its stats + a per-turn AI `onTurn(ctx)`, the same behaviour-component pattern
  the ninja demos use (`update({ this })`), adapted to turns. A new class is just a new object.

## Maps in Tiled

`tiled/battle.tmj` is an **orthogonal** logical grid (edit it in Tiled with `terrain.tsj` /
`heights.tsj`): a `terrain` layer (tile type), a `height` layer (elevation), and a `units` object
layer (class/team per spawn). The build step converts it to engine data:

```bash
python3 tools/gen_battle_tmj.py    # author/regenerate battle.tmj (stand-in for editing in Tiled)
python3 tools/import_battle.py     # battle.tmj -> src/battle_map.tish (frames/heights/walkable/units)
```

Rendering is isometric even though the `.tmj` is orthogonal — the engine's iso projection draws the
logical grid at `(±16,+8)` per step, lifting each tile by its elevation.

## Current state — Phase 3 (player vs AI, move + attack + HUD)

- **Turns.** An engine **speed-based turn queue** (`tac_turn_next`) picks the next unit and floods
  its **move range**.
- **Team 0 = player.** Its turn **waits for input** — the loop keeps rendering but the turn never
  advances on its own. **Team 1 = AI** — its class `onTurn` advances toward the enemy, paced by a delay.
- **Action menu.** A player unit opens a **Move / Attack / Wait** menu (Left/Right + A). Move shows
  the range to pick a tile; **Attack** opens a **target picker** — Left/Right cycles the adjacent
  enemies, the unit **rotates to face** the selected one, A strikes, B cancels; Wait ends the turn.
- **Facing.** Units face one of the 4 iso directions (SE/NE sheet frames + flip) and **stay facing
  the direction they last attacked**. A **damage popup** ("−N") pops over the target on a hit.
- **Move + attack.** A unit **path-walks** to its tile, then **attacks** an adjacent enemy
  (`tac_adjacent_enemy` → `tac_damage` by the class's `power`, with a lunge). Deaths free the tile;
  wiping a team shows **Victory! / Defeat**.
- **HP HUD.** A **fill bar** (green→yellow→red) plus **"cur/max" text** shows the player unit's
  health, live — drawn with the engine `hud_text` (font sprites), which also renders the menu, target
  damage popups, and the result, each in its own text slot.

Engine `tac_*` covers: height grid + occupancy, **unit registry** (stats/HP/team), **move-range**
(Move+Jump), **pathfinding**, **speed turn queue**, **adjacent-enemy targeting**, and `tac_damage`.
Unit classes (stats + AI + `power`) are components in [`src/components.tish`](src/components.tish).

Verified via render: HUD, the AI reaching + attacking the player, and the player's HP dropping.
(Player *input* is code-parity — the libmgba screenshot tool can't inject key presses.)

## What's next
Ranged/ability targeting + an example damage formula, damage popups, a Status entry + cursor-inspect
of any unit's HP, and a win/lose flow.

## Build

```bash
tish build src/main.tish --target gba -o iso-tactics.gba
```

Art: vendored CC0 **Tiny Tactics – Battle Kit I** (`assets/tiny-tactics/`); tools slice it into the
`sheet32:` sprite sheets.
