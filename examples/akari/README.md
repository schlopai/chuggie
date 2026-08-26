# AKARI

> *Puzzle game demo (Akari / Light Up).*

A **top-down action-RPG** for the Game Boy Advance, written entirely in [tish](https://github.com/tishlang/tish)
components on the chuggie engine, using the CC0 **Ninja Adventure** art pack.

> elder hands you a training blade and sends you to cleanse it.*

<img src="preview.gif" alt="preview" width="480">

It strings together every beat of a small RPG:

- **Title screen** (the reusable [`packages/title.tish`](../../packages/title.tish)) → Continue
  (when a save exists) / New Game → NES-style **file select**
  ([`packages/menu.tish`](../../packages/menu.tish) + [`packages/save.tish`](../../packages/save.tish)).
- **Pause menu** (Start) — Resume / Save / Title. Saves go to cartridge SRAM (mGBA writes
  `akari.sav` beside the ROM) via the shared 3-slot adventure blob.
- **Opening cutscene** ([`packages/cutscene.tish`](../../packages/cutscene.tish)) — the Elder crosses
  the plaza, warns of the blight, and gives you the sword.
- **Willow Vale**, a scrolling town with houses you can walk into, a red torii gate, and NPCs you can talk to.
- Each **house interior** is its own one-screen map; step onto a doorway to enter, and the south exit
  brings you back outside that door.
- **The Hollow Shrine**, a four-room dungeon (screen-by-screen *room camera*) with chase-AI enemies,
  a treasure chest, and a boss — clear it to earn a **heart container**.

## Controls
- **D-pad** — move (free 8-directional). **Double-tap** a direction to dash (a short locked-in
  burst), then **hold** to keep running at a middle speed until you release. Walk / dash / run live
  in the shared `packages/topdown` character mover.
- **A** — the context action: talk to an NPC / open a chest / advance dialogue when something is in
  front of you, and swing your sword (once you have it) when nothing is. One button, no mode.
- **B** — throw a ninja star: a spinning projectile that flies until it hits an enemy or a wall.
- **Start** — pause menu (Resume / Save / Title).
- **Audio** — village / dungeon BGM from the pack, plus slash / throw / dash / chest SFX. Dialogue
  uses `packages/dialog` (portraits + typewriter) with an RPG-style blip.
- Walk into any **house doorway** to enter that building; stand on the **south exit** inside to leave.
- Walk into the **cave north of the torii** to enter the shrine; stand on the **doorway at the bottom
  of the entrance room** to leave.

## What it shows off in the engine
This example drove three additions to `tish_gba_game_engine`, all reusable:

- **Free top-down movement** (`this.topdown(w,h)` / `this.move(dx,dy)`) — a new genre alongside grid
  and platformer: 8-directional pixel movement with axis-resolved tile collision (the platformer's
  collision, minus gravity), so the player and enemies slide along walls instead of snapping to tiles.
- **Melee combat** (`this.attack(tag, dmg, reach, size, ttl)`) — spawns a short-lived hitbox in the
  facing direction; the engine's contact-damage + i-frame + **knockback** systems do the rest.
- **Top-down interaction** (`this.interactTD(reach)`) — the free-movement counterpart to grid
  interaction: fire the `onInteract` of whatever you're facing. It reports whether anything answered,
  which is what lets one button talk *or* attack depending on what you face.
- **Thrown projectiles in a walled world** (`bullet_style` + `fire_bullet`) — the shmup bullet path
  reused for the ninja star: a flying hurt box with no per-frame callback, now retired by the
  collision grid so it can't pass through a wall.
- **Hard room cutoff** — with a room camera, combat / collide / interact only land between entities
  that share a room, and flying hurt boxes despawn the moment they leave the player's room (so a
  star through an open doorway can't snipe the next chamber).

Enemies are plain components whose `update` steers toward the player (`chasePlayer`); contact damage,
health, i-frames, and knockback are all native. Maps are Tiled `.tmj` files baked into ROM with the
`scene:` importer and streamed as the camera scrolls.

## Build / run
```bash
npm run build      # build the ROM
npm start          # build + open in mGBA
npm run shot       # build + headless screenshot
```
Regenerate the art and maps from the pack:
```bash
python3 scripts/gen_akari.py         # sprites (hero, enemies, NPCs, FX, items, hearts)
python3 scripts/gen_akari_maps.py    # town.tmj + shrine.tmj + house0..3.tmj
```
See [assets/ATTRIBUTION.md](assets/ATTRIBUTION.md) for the art source + license.

## Notes / limits
- Input-driven states (movement, attacking, talking, warping) are exercised headlessly by passing
  `tools/gba-shot` a key schedule (`"300:a,330:,360:up,375:"` — frame:keys, empty = release); the
  title screen accepts input from about frame 300. Live mGBA (`npm start`) is still the feel check.
- The boss is a 16×16 sprite like everything else: all sprites must share one import scheme (`sheet:`),
  because the tish compiler numbers asset handles per-scheme into a shared sprite arena, so mixing
  `sheet:`/`sheet64:` would swap sprites (tish #552). A larger boss awaits that fix (or a metasprite).
- A drawn sword *slash* is a planned polish pass; today the swing hitbox is invisible and feedback
  comes from the enemy's hit-flash + knockback.
