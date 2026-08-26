# FX PARTICLES

> *A particle system, screen flash and screen shake — owned, stepped and **budgeted** by the engine.*

<img src="preview.gif" alt="preview" width="480">

Seven demos, each fired with a single call. There is **no per-frame effect code in this example**:
nothing here steps a particle, integrates the shake spring, or counts a sprite. That is the point.

## Controls

| Input | Action |
|-------|--------|
| A | Fire the current effect (in WEATHER, cycle rain → snow → fire → fountain) |
| B | Next demo (fireworks → flash → shake → bump → confetti → weather → budget) |
| UP / DOWN | Particle count · shake power · bumps per frame · emit rate · **game sprites** |
| LEFT / RIGHT | Launch speed · shake settle time · **OAM reserve** |
| SELECT | `fx_stop` the emitter — it stops emitting, the particles already out finish |
| START | Clear every particle |

## The API

```tish
import { fx_burst, fx_flash, fx_shake, fx_bump, fx_shake_stop, fx_active, fx_clear,
         fx_spawn, fx_set, fx_stop, fx_kill, fx_budget, fx_headroom } from 'cargo:tish_agb'
import { FXP_RAIN, FXE_RATE, FXE_FRAMEN } from '../../packages/fx'   // names, no functions

// One-shots
fx_burst(sheet, frame, x, y, count, speed, gravity, life)
fx_flash(level)             // 0..16 toward white, decays one level per frame
fx_bump(ax, ay)             // impulse the shake spring, 8.8 px/frame. Bumps ADD.
fx_shake(power, frames)     // peak ~power px, settling in ~frames frames
fx_shake_stop()             // settle now, every surface square

// Emitters — sources that keep going
let rain = fx_spawn(FXP_RAIN, sheet, frame, x, y, 256)  // 256 = the preset as authored
fx_set(rain, FXE_RATE, 128)     // particles per frame in 8.8 — 128 is one every other frame
fx_set(rain, FXE_FRAMEN, 3)     // walk frames 0..3 across each particle's life
fx_stop(rain)               // stop emitting; what is out finishes (a torch thinning)
fx_kill(rain)               // stop AND drop its particles now (scene teardown)

fx_budget(-1, 16)           // -1 = size automatically. 16 = OAM kept free for the game
fx_headroom()               // particles the layer may still spawn
fx_active()                 // particles alive
fx_clear()                  // everything, emitters included; call on scene teardown
```

Presets: `BURST` `CONFETTI` `RAIN` `SNOW` `FIRE` `SMOKE` `SPARKLE` `FOUNTAIN`. Shapes: point, box,
ring, line. Per-emitter: direction and spread, speed and variance, gravity, drag, wind, life, rate,
frame range, duration, and its own particle ceiling.

## The budget is the feature

The GBA has **128 OAM entries for the whole machine** — the player, every NPC, the HUD, every
`text_draw` slot, and every particle — and nothing in the hardware arbitrates them. A 48-particle
burst is 37.5% of that. Fire one in a town with sixteen NPCs and it does not look busy; NPCs
disappear, or the burst comes out empty, depending only on which allocated first. The failure is
invisible in the effect's own demo, where nothing else is on screen.

So the engine measures instead of hoping. Every spawn asks what the **game** is currently holding and
takes only what is genuinely spare, plus a reserve for sprites that do not exist yet — the NPC who
walks on during your victory burst. An emitter that would exceed its share emits fewer particles this
frame. **There is no error to handle, because there is nothing a caller could do with one.**

The BUDGET demo shows it: a deliberately greedy emitter (4 particles a frame, its own ceiling lifted
to the whole layer) against 40 "game" sprites you add with UP. The read-out is `g<game> hr<headroom>
p<particles>`, and the invariant holds at every point —

```
particles + game sprites + reserve  ==  128
72        + 40           + 16       ==  128
```

`verify.sh` asserts both halves: the arithmetic, *and* that the game's sprites are actually on
screen. Those are different claims — the first version of this demo had perfect accounting while
every game sprite was invisible behind the UI canvas.

- **Particles are HUD sprites** — screen space, front priority, no camera offset — so a burst sits
  correctly over a result screen and over a scrolling map alike.
- **There is no per-particle alpha, and there cannot be.** `BLDCNT` holds one blend mode for the
  whole screen and the engine spends it on the scene fade, so a particle cannot fade out by going
  transparent at any price. It fades by walking its own sheet frames instead — `FXE_FRAME0`
  … `FXE_FRAMEN` across its life. Author the last frame faintest and you have a fade. This is how it
  was done on the hardware originally, and it costs nothing per frame: the engine rebuilds a
  particle's tiles only on the frames where the index actually changes.
- **`rate` is fractional** (8.8 particles per frame), which is the reason emitters are here rather
  than in a game's own timer. Twelve drops a second is 0.2 a frame; every game would otherwise keep
  the same accumulator, and most would round it to "one a frame" and wonder why rain costs 60 sprites.
- **`speed` and `gravity` are 1/256ths of a pixel per frame.** `gravity: 0` drifts (confetti);
  `gravity: 26` arcs over and falls (fireworks). Position and velocity are 8.8 fixed point, because
  at whole-pixel velocities a burst reads as an expanding decal rather than as an explosion.
- **`fx_flash` and `fade` share one register.** BLDY cannot brighten and darken at once, so a fade
  in progress wins — otherwise a flash fired near a scene change would fight the transition and
  strobe.
- **The shake moves every layer**, not just the camera: BG scroll, the UI canvas, and HUD sprites.
  It is applied at compose time — nothing re-renders, so a shake during a busy frame is free. The
  first version offset only the camera and was therefore invisible on any screen drawn with the UI
  canvas (a result screen, a menu, this demo), which is most of the places you would want one.
  `verify.sh` now asserts it with an image diff rather than "did not crash".
- **The shake is a damped spring, and bumps SUM.** Three impacts in one frame make one bigger,
  longer shake; a countdown-driven shake cannot do that, because the third call overwrites the first
  two and loses their decay. Try the BUMP demo at 1 and then at 6 — same impulse, six times in a
  single frame. `verify.sh` measures the peak displacement off the picture and asserts it grows.
- **There is exactly one shake in the engine.** The spring and its tuning came from
  `packages/feel.tish`, which owned a second, independent one built on `ui_scroll`; `feelBump` is now
  a delegate to `fx_bump`. Two shakes writing the same scroll register with independent lifetimes is
  a bug waiting for the first scene that runs both.
- **`fx_shake`'s `power` really is pixels**, across the whole range, because it calibrates the
  impulse against the damping at call time (a 32-iteration integer sim, once per call, never per
  frame). Damping eats the peak, so the obvious fixed impulse-per-pixel constant is wrong at both
  ends: it made `fx_shake(2, 8)` move the screen **zero pixels** and `fx_shake(16, 88)` overshoot to
  27. Both arguments set the damping — a 12-pixel swing needs longer to decay than a 2-pixel one, so
  `frames` alone cannot determine it.

## Why this is in the engine

A thirty-particle burst driven from Tish is a position write per particle per frame — sixty boxed
native calls a frame on top of whatever the scene is already doing. Stepping them in Rust costs the
game **three calls total** for the whole effect, and the game never has to remember to stop.

The same reasoning already applies elsewhere in this repo: `card-gba`'s towns lost ~4 fps to
re-issuing sixteen NPC sprite positions every frame for actors that never move.

The budget is the stronger argument, though. A particle library in a *package* can only ever see its
own particles; it cannot know that the town just spawned four NPCs, so the best it could offer is a
number the game has to keep up to date by hand — which every game would get wrong in a different
place. In the engine it reads the real sprite arena every spawn. **The developer never counts
sprites, and never finds out the hard way that they should have.**

## Build / run

```bash
cd examples/fx-particles
unset CARGO_TARGET_DIR && npm run build   # -> fx-particles.gba
npm start                                 # build + mgba
npm run shot                              # headless screenshot
npm run verify                            # headless assertions
```

## Layout

- [`src/main.tish`](src/main.tish) — the whole demo; the only per-frame lines are `fx_active()` /
  `fx_headroom()` reads
- [`assets/spark.png`](assets/spark.png) — four small bright shapes on a 32×32 grid
- [`../../packages/fx.tish`](../../packages/fx.tish) — names for the presets, fields and shapes.
  **Constants only — not one function in the file**, so it carries none of the ~151 bytes-per-function
  heap cost a Tish package normally does. You never need it; `fx_spawn(2, …)` is the same call as
  `fx_spawn(FXP_RAIN, …)`. It exists so the second one can be read six months later.

Particles have to be **tiny**. The first version of this used a character sheet because it was
already loaded, and the burst came out as a shower of small men — it read as a bug rather than as an
effect. Anything with a silhouette reads as an object being thrown.
