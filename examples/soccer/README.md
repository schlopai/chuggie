# SOCCER

> *Six players and a ball — the acceptance test for disc-vs-disc contact.*

<img src="preview.gif" width="480">

[`golf`](../golf/README.md) proves one disc integrates, bounces off tiles and comes to rest. That
says nothing about what happens when discs meet **each other**, which is the half of a physics
engine that is easy to make *present* and hard to make *stable*.

## What it proves

```
ok   the ball never leaves the pitch (no body walks through a solid tile)
ok   players never sink into one another (min d2 64, touching is 100)
ok   17 goals — contact moves the ball, it does not just unstick it
ok   the ball reaches real speed (max v2 36770) — players move it, not the reverse
ok   7 own goals — body_last_hit attributes the toucher
ok   heap flat across the match (span 0 B)
ok   entity count constant (ENT 7 ) — 1 ball + 6 players, nothing spawns
```

**The rank split is the mass model.** The ball is rank 0 and the players rank 1: a *lower* rank
takes the whole of a contact correction against a higher one, so the ball is what moves when a
player runs into it, while two players meeting split it evenly. That is deliberately not a mass —
the textbook `m₂/(m₁+m₂)` split is a software division per contact per iteration on a chip with no
divide instruction. Getting the order backwards gives a ball that shoves players around and cannot
be dribbled, which is why the verifier checks the ball reaches real speed rather than trusting the
constant.

**`body_last_hit` is the difference between a goal and an own goal.** The engine records the toucher
during contact resolution; reconstructing it afterwards from positions is exactly the guess that
field exists to remove.

**Six chasers cost zero per-frame tish callbacks.** The steering is arithmetic in the frame loop —
eight-way from the sign of each axis, no `atan2`, no division, no component.

## The bug this example found

The ball drifted out of the stadium — at frame 1,536 it was at x=431 on a 352-wide pitch and still
going, a fraction of a pixel per frame, **with its velocity reading zero**.

It survived three wrong fixes, and the wrong fixes are the interesting part:

1. *"Contact shoves a sleeping body and nothing re-checks the wall"* → wake the body on a shove.
   No change.
2. *"The wake is undone by the sleep pass later in the same frame"* → move the sleep pass before
   contact. Goals doubled, ball still escaped.
3. *"A shove must never land in a solid tile"* → revert a shove that does. Still escaped.

Each was a real defect and each fix is still in the engine. None was the cause. The cause was that
**`movement_system` integrates `transform += body` for anything with a body, with no collision
check**, and a rigid disc qualified — so every disc was moved twice per frame, and half that travel
was unchecked. `dynamic_system` now owns disc integration exclusively.

The lesson worth keeping: three plausible explanations of a symptom, each confirmed by inspection,
none of them it. The question that broke it was not *"why does the sleep logic fail"* but *"what
else writes a transform"*.

## The pitch

`scripts/gen_soccer.py` emits `pitch.tmj`. The goal **mouths are gaps in the wall**, not tiles with
a property — a goal is a region the ball passes through, and tiles that stopped the ball would be a
goal you cannot score in. Walls come from a `Solid` layer (`Collision` is the opposite: it forces
cells *walkable*).

A ball wedged in a corner between two players is a legal physical outcome, so there is a **dead-ball
restart** after 600 idle frames. Without it the soak sat on a stalemate and reported "no goals",
which reads like broken physics.

## Controls

None. It plays itself, deliberately: what is under test is the physics, and a human in the middle
would make the soak's outcome depend on nobody holding the pad.

```bash
npm run verify
```
