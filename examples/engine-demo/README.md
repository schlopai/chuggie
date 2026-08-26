# ENGINE DEMO

> *A comprehensive showcase of multiple engine features working together.*

<img src="preview.gif" alt="preview" width="480">

The **Behaviour bridge** — a game component written in *tish*, ticked by the *Rust*
engine. This is the Unity split made concrete: the engine (Rust) owns the store,
pipeline, movement, and rendering; the game (tish) is components + data.

```tish
define_component('Bouncer', {
  update: (me, e, dt) => {          // me = this instance's data, e = entity
    if (entity_x(e) < 8.0)   { me.vx = 0.7 }
    if (entity_x(e) > 216.0) { me.vx = -0.7 }
    ...
    set_body(e, me.vx, me.vy)       // drive the entity; the engine applies it
  }
})

let e = spawn()
attach_sprite(e, sprite_new(player))
add_behaviour(e, 'Bouncer', { vx: 0.7, vy: 0.6 })

while (true) { world_step() }       // engine ticks Bouncer → moves → renders
```

Each `world_step()`, the engine runs two phases: **behaviours** (invoke every
attached component's tish `update` callback — reentrancy-safe: callbacks may call
back into the engine) then **systems** (movement + render), then commits the frame.
The `Bouncer` sprite bounces around the screen — logic defined entirely in tish.

(Note: the instance param is `me`, not `self` — `self` is a reserved Rust keyword
that tish can't emit as an identifier.)

Engine: `crates/tish-gba-game-engine`. Build: `npm run build`.
