# Top-down genre kit — contributor notes

> **User documentation:** [chuggie.dev — Top-Down Package](https://chuggie.dev/docs/packages/topdown)

`packages/topdown.tish` is the shared top-down movement, facing, warp, and interaction layer used by grid RPGs, action-adventure ports, and room-streaming games.

## What the package owns

- Two-mover top-down locomotion with facing and tile/room arithmetic
- Warp latch and room-transition hooks
- `tdContextAction` soft targeting and L/R chord skill wheel
- Movement profiles via `setTopdown(profile)`

## What stays per-game

- **Room streaming** — each game owns how maps load/unload at boundaries
- Tilemap layout and collision beyond the shared grid helpers
- Quest/progression state — use `packages/flags.tish` instead of ad-hoc bitfields

## Canonical examples

- `examples/overworld-demo` — basic top-down movement
- `examples/ninja-village` — NPC interaction
- See `INVENTORY.md` genre table for the full consumer list

## Why this doc exists

`INVENTORY.md` and `docs/engine-review-2026-08.md` reference this file for contributor context on what the topdown kit owns vs. what each game must implement. End-user API docs live on chuggie.dev.
