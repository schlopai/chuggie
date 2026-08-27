# Native tools

Small C programs linked against libmgba for headless testing and link cable simulation.

## gba-shot.c

Headless mGBA screenshot capture. Used by `scripts/screenshot.sh` and `scripts/gif.sh`.

**Build:** requires libmgba (`brew install mgba` on macOS). `MGBA_PREFIX` overrides the install prefix.

```bash
scripts/screenshot.sh examples/akari/akari.gba out.png 120
```

## gba-link.c

Dual-core link cable test harness. Boots two engine instances in one process and connects their SIO ports virtually.

**Build:** `scripts/link.sh`

Used by `packages/link.tish` and `examples/link-demo` for local multiplayer debugging without two physical GBAs.

## Why C?

These tools predate the npm workflow and need direct libmgba access for frame-accurate headless capture and SIO bridging. Game ROMs themselves are built with tish + Rust.
