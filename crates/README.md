# Rust crates

Four ROM-target crates power Chuggie Engine. All target `thumbv4t-none-eabi` and depend on [agb](https://github.com/agbrs/agb) 0.25.

| Crate | Role |
|-------|------|
| **`tish-agb`** | Low-level agb wrapper — sprites, backgrounds, input, audio, save, UI canvas, terrain, kart physics, deck player. Typed ABI in `tish.d.tish`. Import schemes in `tish.schemes.json`. |
| **`tish-gba-game-engine`** | SoA ECS (15 components) + fixed per-frame pipeline (20 systems) + genre modules (grid, platformer, topdown, shmup). Drives `tish-agb`; no direct agb dependency. |
| **`tish-gba-scenepack`** | Compile-time asset baking — `scene:`, `deck:`, `font:`, `chip:`, `strings:`, `isoboard:`, `isobattle:` proc-macros. |
| **`tish-agb-sio`** | Link cable SIO — 4-function ABI in `tish.d.tish`, used by `packages/link.tish`. |

## Dependency direction

```
agb
  ▲
tish-agb
  ▲
tish-gba-game-engine
```

`tish-gba-scenepack` and `tish-agb-sio` are independent helpers consumed by games via `cargo:` imports.

## API documentation

- **Typed tish surface:** `crates/*/tish.d.tish` — primary reference for game developers
- **Rust internals:** module-level `//!` and `///` doc comments in `src/lib.rs` and submodules
- **User guides:** [chuggie.dev/docs](https://chuggie.dev/docs)

## Building

Crates are built as dependencies of example ROMs via `tish build --target gba`, not standalone `cargo build` from this directory (except host tooling in scenepack tests).

See `CONTRACT.md` for the compiler ↔ framework ABI.
