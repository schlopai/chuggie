# PLATFORMER COMBAT

> *Demonstrates a platformer with health, a hearts HUD and stompable patrol enemies.*

![preview](preview.gif)

A side-scrolling platformer that exercises the reusable engine systems for
platformer combat. The camera follows the player across a 65×14 streamed level.

## Controls
- **d-pad** — move (and Down to look/drop)
- **A** — jump. Hold for a higher jump, tap for a short hop (variable height). Buffered on
  press and coyote-timed, so it feels responsive. **Down + A** drops through a one-way ledge.
- **B** — run (faster than walking)

Land on an enemy to **stomp** it (you bounce); touch its side and you take a hit (with a
brief invincibility flicker). At 0 HP you respawn at the start with full health.

## What it demonstrates (all reusable engine features)
- **Platformer feel** — run/walk, gravity + AABB tile collision, coyote time, jump buffering,
  variable-height jumps, one-way platforms + drop-through.
- **Health** — HP with post-hit i-frames (sprite flicker), `onDeath` hook (here: respawn).
- **HUD** — a hearts readout (`setupHearts` / `updateHearts`) reflecting the player's HP
  (2 hp per heart → half hearts).
- **Enemies** — a `Patrol` behaviour that turns at walls (`blocked`) and ledges (`tileSolid`),
  damages the player on contact, and dies when stomped.

All of the above are engine/sugar APIs (see `packages/engine.tish`), not example-specific code:
`this.run/jump/jumpRelease/drop/moveX/onGround/blocked/bounce`, `this.setHealth/hurt/heal/hp/alive`,
`setupHearts/updateHearts`, `tileSolid`, and the `onDeath` component hook.

## Build / regenerate

```bash
npm run build      # build the ROM
npm start          # build + open in mGBA
```

Regenerate art + level: `python3 scripts/gen_platformer-combat.py` (repo root).

Note: streamed backgrounds page in over the first few hundred frames, so a headless screenshot
needs ~400+ frames to show the filled scene.
