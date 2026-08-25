# BENCH BUILD

> *A benchmark testing the performance of the build subsystem.*

Isolated example for measuring build cost. **Do not use a full game for this.**

## What it measures

| Target | Imports |
|--------|---------|
| `engine_only` | `packages/engine` (+ tiny `scene_hooks`) |
| `with_ui` | engine + full `packages/ui` |

## Profiles (`tish` → nested GBA `Cargo.toml`)

| Env | Meaning |
|-----|---------|
| _(default)_ | `opt-level=3`, **thin** LTO, 8 codegen-units, no debuginfo |
| `TISH_FAST_NATIVE_BUILD=1` | iteration: opt 1, **no** LTO, 16 CGUs, incremental |
| `TISH_GBA_FAT_LTO=1` | ship: fat LTO, 1 CGU (slow) |
| `TISH_GBA_DEBUG=1` | keep release debuginfo |

```bash
unset CARGO_TARGET_DIR
export PATH="$PWD/../../../tish/tish/target/release:$PATH"
npm run bench          # times all profiles; prints main.rs line counts
npm run build:fast     # day-to-day iteration
npm run build          # default thin LTO
npm run build:fat      # old fat-LTO ship build
```

## Measured (Apple Silicon, warm `build-std` cache)

| Target | `main.rs` lines | Profile | Wall |
|--------|----------------:|---------|-----:|
| `engine_only` | ~3 000 | FAST | ~25 s |
| `engine_only` | ~3 000 | thin LTO (default) | ~33 s |
| `engine_only` | ~3 000 | fat LTO | ~45 s |
| `with_ui` | ~14 700 | FAST | ~60 s |

Before `scene_hooks`, an engine-only ROM pulled the whole ui package (~14–17 k lines).
Large games still pay for one mega-`fn run()`; use `TISH_FAST_NATIVE_BUILD=1` while
iterating, and prefer measuring here rather than rebuilding the full game for profile A/B.
