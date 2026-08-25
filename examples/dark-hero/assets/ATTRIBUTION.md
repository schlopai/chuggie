# Art attribution

The hero animations are derived from the **"DARK - Hero" (Free Version)** platformer character pack
(48×48 frames). `scripts/gen_darkhero.py` (repo root) copies the frames this demo uses out of the pack
and packs them into one GBA sprite sheet; it is the source of truth for `hero.png`.

The tileset, the spike-orb hazard, and the HUD heart are generated procedurally in that same script
(not from the pack).

> The pack shipped without a bundled license file. Before redistributing, confirm the pack's terms on
> its store page and add the author's name and license here. Replace this note once verified.

Sheet layout (frame ranges kept in sync with `src/components.tish`):
`Idle 0..8 · Run 8..16 · Jump 16..20 · Fall 20..24 · Land 24..36 · Ledge Grab 36..40 ·
Ledge Grab Idle 40..54 · Ledge Climb 54..67 · Hit 67..69 · Death 69..79` (79 frames, 64×64).
