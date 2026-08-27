# Contributing to chuggie

Thank you for contributing to Chuggie Engine. User-facing documentation lives on **[chuggie.dev](https://chuggie.dev/docs)** — update MDX there when adding packages or public APIs. This repo holds source, examples, and contributor docs.

## Dev setup

### Prerequisites

- Node.js 22+, npm
- Rust nightly with `rust-src` (see `rust-toolchain.toml`)
- `agb-gbafix` (`cargo install agb-gbafix`)
- mGBA on PATH (`brew install mgba`)
- Sibling checkouts (recommended):
  - `../tish/tish` — tish compiler
  - `../agb/agb` — agb fork (patched via `.cargo/config.toml`)

### One-time setup

```bash
npm run setup          # links local tish CLI (TISH_REPO=/path/to/tish if not sibling)
npm install            # workspace install at repo root
```

### Build and run an example

```bash
npm start -w akari                    # build + play in mGBA
scripts/rom.sh play akari             # play ROM on disk (no build)
scripts/rom.sh shot akari             # headless screenshot
npm run vscode                        # regenerate .vscode tasks after adding examples
```

See [`docs/agent-dev-loop.md`](docs/agent-dev-loop.md) for the full contributor workflow.

## Where to put documentation

| Audience | Location |
|----------|----------|
| Game developers | [chuggie.dev](https://chuggie.dev) MDX (`../chuggie.dev/content/docs/`) |
| Architecture / ABI | `ARCHITECTURE.md`, `CONTRACT.md`, `INVENTORY.md` |
| Contributor guides | `docs/` (see [`docs/README.md`](docs/README.md)) |
| API truth | `crates/*/tish.d.tish` + Rust `///` comments |

See [`DOCUMENTATION.md`](DOCUMENTATION.md) for the full policy.

## Code layout

| Path | Purpose |
|------|---------|
| `crates/` | Rust ROM crates (`tish-agb`, `tish-gba-game-engine`, `tish-gba-scenepack`, `tish-agb-sio`) |
| `packages/` | Tish authoring modules (published as `@schlopai/chuggie`) |
| `examples/` | Reference ROMs — each has its own `README.md` |
| `scripts/` | Build, verify, screenshot, itch, generators |
| `assets/` | Vendored art packs |

**Dependency rule:** dependencies only point down toward `agb`. See `ARCHITECTURE.md`.

## Adding a new package

1. Add `packages/your-module.tish` and export from `packages/package.json` if needed
2. Add a canonical example under `examples/`
3. Add MDX page on chuggie.dev under `content/docs/packages/`
4. Update `packages/README.md` module table and `INVENTORY.md`

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `TISH_REPO` | `../tish/tish` | Path to tish checkout for `npm run setup` |
| `MGBA` | `mgba-qt` / `mgba` | Emulator binary for `scripts/rom.sh` |
| `MGBA_ARGS` | _(empty)_ | Extra emulator flags (`--scale 4`) |
| `MGBA_PREFIX` | Homebrew prefix | libmgba path for headless tools |
| `ITCH_TARGET` | _(unset)_ | Butler push target (`user/game:html5`) |
| `PORT` | `4173` | `npm run itch -- serve` port |
| `TISH` | `npx tish` | Override tish CLI in screenshot scripts |
| `GBA_SHOT_*` | see `scripts/shot_common.sh` | Headless screenshot tuning |
| `GIF_SCALE`, `GIF_FROM`, `GIF_EVERY`, `GIF_MAX_FRAMES` | see `scripts/gen_previews.js` | GIF preview generation |

### Cargo features (`tish_agb`)

- `save-flash-512k` — 64 KiB flash save
- `save-flash-1m` — 128 KiB flash save

## Pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, etc.)
- Run the example's verify script if one exists (`verify.sh`)
- Keep changes focused — one feature or fix per PR when possible

## License

By contributing, you agree that your contributions are licensed under MIT OR Apache-2.0, at the recipient's option.
