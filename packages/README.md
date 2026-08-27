# chuggie

Game engine packages for writing Game Boy Advance games in [tish][tish], on [agb][agb].

```bash
npm install @schlopai/chuggie
```

```tish
import { mount, spawn, set_transform } from '@schlopai/chuggie'
import { sceneGoto, sceneRegister } from '@schlopai/chuggie/scene'
import { menuOpen } from '@schlopai/chuggie/menu'
```

Every module is tish source, compiled into your ROM by `tish build --target gba`. Nothing here is
pre-compiled — a GBA build has no dynamic linking, so the authoring layer ships as source and the
compiler inlines what you actually import.

User-facing documentation: **[chuggie.dev/docs](https://chuggie.dev/docs)**

## What's here

| Area | Modules |
|------|---------|
| Core | `engine` · `scene` · `scene_hooks` · `prefs` · `save` · `pool` · `rng` · `buildid` · `memdebug` |
| Genres | `platformer` · `topdown` · `shmup` · `beatemup` · `iso` · `iso_actors` · `isodemo` · `fighter` · `boxing` · `kart` · `rts` · `rhythm` · `dungeon` · `fpview` · `microgame` |
| Presentation | `ui` · `menu` · `dialog` · `cutscene` · `cutscene-core` · `title` · `parallax` · `shop` · `feel` · `fx` · `transition` · `grid` |
| Audio | `chipsfx` · `deck` · `music` · `sfx` |
| Game data | `flags` · `keylock` · `replay` · `party` · `cards` · `search` |
| Multiplayer | `link` |
| World / color | `chroma` · `chroma-world` · `mode7` · `motion` |

`import { … } from '@schlopai/chuggie'` is the engine entry (`engine.tish`); everything else is a subpath (`@schlopai/chuggie/shmup`, etc.).

## The Rust half

These modules call into native crates over tish's `cargo:` import form — `tish_agb` (agb bindings),
`tish_gba_game_engine` (the SoA entity store and frame pipeline),
`tish_gba_scenepack` (the compile-time scene/`.deck` baker), and `tish_agb_sio` (link cable). Declare
them in your game's `package.json`:

```json
{
  "tish": {
    "rustDependencies": {
      "tish_agb": { "version": "0.1" },
      "tish_gba_game_engine": { "version": "0.1" }
    }
  }
}
```

## Requirements

`@tishlang/tish` >= 3.2.2, a nightly Rust toolchain with `rust-src` (the GBA target is built with
`build-std`), and `agb-gbafix` to turn the ELF into a `.gba`. `scripts/dev-setup.sh` in the repo
installs all three.

## License

See the repository. Don't send patches here — the source of truth is
[schlopai/chuggie](https://github.com/schlopai/chuggie).

[tish]: https://github.com/tishlang/tish
[agb]: https://github.com/agbrs/agb
[deck]: https://www.npmjs.com/package/@spacedevin/deck
