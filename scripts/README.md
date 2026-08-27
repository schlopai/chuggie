# Scripts

Operational scripts for building, verifying, packaging, and generating content. Run from the repo root unless noted.

## Daily workflow

| Script | Purpose |
|--------|---------|
| `dev-setup.sh` | One-time local tish CLI setup (`npm run setup`) |
| `rom.sh` | Build/play/screenshot/gif an example (`scripts/rom.sh play akari`) |
| `screenshot.sh` | Headless still capture via libmgba |
| `gif.sh` | Headless animated GIF capture |
| `itch.js` / `publish-itch.sh` | Package ROM for itch.io HTML5 (`npm run itch -- publish shmup`) |
| `gen-vscode.js` | Regenerate `.vscode/tasks.json` (`npm run vscode`) |
| `clean.sh` | Remove build artifacts (`npm run clean`) |

## Verification

| Script | Purpose |
|--------|---------|
| `verify_common.sh` | Shared helpers for example `verify.sh` scripts |
| `link.sh` | Build native `tools/gba-link` for link cable testing |
| `check_agb_fork.sh` | Sanity-check agb fork patch |
| `gba_soak.sh` | Long-running soak tests |
| `deck_lint.py` | Lint `.deck` music files |
| `arity_check.py` | Check tish function arity in packages |

Per-example `verify.sh` and `*_check.py` scripts live alongside examples or in this directory.

## Content generators (`gen_*.py`)

Build-time generators that emit `.tmj`, art, or tish data for specific examples. Each is tied to one example or asset pack — not general-purpose tools. Examples:

| Script | Feeds |
|--------|-------|
| `gen_examples_readme.py` | `examples/README.md` gallery index |
| `gen_golf.py` | `examples/golf` course layout |
| `gen_rts_spikes.py` | RTS example maps |
| `gen_towerdef.py` | `examples/tower-def` |
| `gen_sunnyside_pack.py` | Sunnyside asset pack |
| `gen_oakhollow.py` | `examples/oakhollow` |
| `fighter_art.py` | Fighter sprite sheet layout |
| `procgen/rooms.py` | Dungeon BSP oracle for `packages/dungeon.tish` |

## Asset search

`asset_search/` — MCP/CLI for searching the ninja-adventure catalog. See `asset_search/README.md`.

## Environment variables

See [`CONTRIBUTING.md`](../CONTRIBUTING.md#environment-variables) for `MGBA`, `MGBA_PREFIX`, `TISH_REPO`, `ITCH_TARGET`, `GBA_SHOT_*`, and GIF variables.
