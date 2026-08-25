# Void — space art for `examples/warheads`

Two Creative Commons Zero packs by **Foozle** (`foozlecc.itch.io`). CC0 waives attribution, but we
record provenance anyway so the next person can re-fetch, diff or replace them.

## What to download, and where to put it

| pack | file | size | source |
|---|---|---|---|
| Void — Fleet Pack 1 (Kla'ed) | `Foozle_2DS0012_Void_FleetPack_1.zip` | 2.9 MB | https://foozlecc.itch.io/void-fleet-pack-1 |
| Void — Environment Pack | `Foozle_2DS0015_Void_EnvironmentPack.zip` | 1.6 MB | https://foozlecc.itch.io/void-environment-pack |

itch.io's free-download flow needs a click-through ("No thanks, just take me to the downloads"), so
these are fetched by hand rather than by a script.

Unpack them so the tree looks like this — `scripts/gen_warheads.py` reads these paths:

```
assets/void/
  SOURCE.md          this file
  LICENSE.txt        the CC0 text (paste from https://creativecommons.org/publicdomain/zero/1.0/legalcode)
  fleet/             everything from FleetPack_1.zip
  environment/       everything from EnvironmentPack.zip
```

## Why this pack

- **CC0**, so nothing about redistribution or attribution is load-bearing.
- **One faction is one coherent palette.** This matters more than it sounds: every imported sheet
  claims one of the GBA's sixteen sprite palette banks and must quantise to ≤15 colours *as a whole
  sheet*, not per frame. Art drawn as a set survives that; art assembled from three sources does not.
- Its hull sizes (Scout → Frigate → Dreadnought) map onto the three ship classes without editing.

## Until it is here

`scripts/gen_warheads.py` runs either way. With `fleet/` and `environment/` present it bakes the real
art; without them it draws the same frames as flat placeholder shapes and prints a notice. So the
game is buildable and testable before the download happens, and dropping the zips in later is a
change to the generator alone — no game code moves.
