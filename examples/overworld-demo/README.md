# OVERWORLD DEMO

> *A grid-walking top-down overworld: tile-locked movement, door warps between scenes, and NPCs you face and talk to.*

<img src="preview.png" alt="preview" width="480">

A grid-walking overworld written in tish: tile-by-tile grid walking, wall/rock
collision, and an NPC you face and talk to. The genre lives in the engine (a `GridPos`
component + grid-stepping system + tile-collision grid + interact probe); the game is
components + a map, in tish.

`src/components.tish`:
```tish
export const Player = {
  update: ({ this }) => {
    if (!this.gridMoving()) {              // one press = one tile
      this.gridStep(input_x(), input_y())  // 4-dir step, blocked by solids/entities
      if (key_pressed(0) > 0) { this.interact() }   // A → talk to faced tile
    }
  }
}
export const Npc = {
  onInteract: ({ this, other }) => { log("NPC: Hello, traveler!") }
}
```

`src/main.tish` builds a 15×10 tile room: `setupGrid`, wall the border + a couple
`setSolid` obstacles, place the hero and NPC with `onGrid(col, row)`, then `step()`.

Notes:
- Grid entities slide smoothly one tile per step (fixed-point), block on solid tiles
  AND on each other's tiles (so you face the NPC instead of walking through it).
- Facing is set by the last attempted step; `interact()` probes the faced tile and
  fires that entity's `onInteract`.
- Callbacks use the natural `this` (works as an identifier on the Rust backend; unlike
  `self`, see tishlang/tish#549).

Build: `npm run build`. Boot in mgba; walk with the d-pad,
press A next to the NPC to see the greeting in the mGBA log.
