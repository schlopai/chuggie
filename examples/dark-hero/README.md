# DARK HERO

> *A genre template or demo for a dark-themed action game.*

An **animation-state showcase** built on the platformer engine, using the CC0-ish "DARK - Hero" (Free
Version) character pack. One combined 64×64 sheet holds every clip, and the `Hero` component (see
[src/components.tish](src/components.tish)) is a mostly self-contained state machine that drives all
ten states from physics + input + tile probes — the same pattern as the sunny-land example.

![preview](preview.gif)

## The ten states
| State | Trigger |
|-------|---------|
| **Idle** | grounded, no input |
| **Run** | grounded + d-pad (hold **B** to run faster) |
| **Jump** | **A** while grounded — the *rising* part (`vy < 0`) |
| **Fall** | airborne and descending (`vy > 0`) |
| **Land** | one-shot on touchdown after being airborne |
| **Ledge Grab** → **Ledge Grab Idle** | fall into a wall's top edge (facing it) → the body freezes and hangs |
| **Ledge Climb** | **A** / **Up** while hanging → climb up onto the ledge; **Down** drops off |
| **Hit** | HP drops from a hazard hit (brief i-frame flicker) |
| **Death** | at 0 HP — the death animation plays, then you respawn |

## Controls
- **d-pad** move · **A** jump (hold = higher) · **B** run.
- Run into a wall's edge while falling to **grab** it; **Up** climbs, **Down** drops.
- **Stomp** the spike orb from above to pop it; touch its side and you take a hit.
- On boot the hero spawns in the air, so it **Falls** and **Lands** with no input — then it's yours.

## What it adds to the engine (reusable)
- `platformer_vy` — read vertical velocity, so a component can tell **Jump** (rising) from **Fall**.
- `platformer_hold` — freeze a platformer body in place (no gravity/movement); the ledge hang uses it,
  and the climb teleports + releases. Exposed as `this.vy()` / `this.hold(on)`.
- Everything else reuses the existing platformer / health / tag / animation-controller / off-screen
  culling systems. Bigger-than-hitbox art (64×64 over a 16×16 box) rides on `spriteOffset`.

## Build / run
```bash
npm run build      # build the ROM
npm start          # build + open in mGBA
npm run shot       # build + headless screenshot
```
Regenerate art + level from the pack: `python3 scripts/gen_darkhero.py` (repo root). See
[assets/ATTRIBUTION.md](assets/ATTRIBUTION.md) for the art source + license note.

## Notes
- Verified headless (mGBA): rendering, **Idle**, boot **Fall**+**Land**, **Hit**, **Death**+respawn,
  hazard patrol, and **Ledge Grab / Grab Idle** (via a positioned fall). **Run / Jump / Ledge Climb**
  are driven by held-key input, which this headless setup can't deliver — they're code-complete (Climb
  is the grab path plus a teleport); confirm them in mGBA with `npm start`.
- Streamed backgrounds page in over the first several hundred frames (a white screen that then clears),
  so a headless screenshot needs ~450+ frames.
