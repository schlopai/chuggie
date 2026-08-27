# Contributor documentation index

User-facing guides live on **[chuggie.dev/docs](https://chuggie.dev/docs)**. This directory holds contributor-only and historical docs.

## Start here

| Doc | Purpose |
|-----|---------|
| [`../README.md`](../README.md) | Quick start, build/play, itch publish |
| [`../ARCHITECTURE.md`](../ARCHITECTURE.md) | Layer map and dependency rules |
| [`../CONTRACT.md`](../CONTRACT.md) | Compiler ↔ framework ABI |
| [`../INVENTORY.md`](../INVENTORY.md) | Full codebase inventory |
| [`../DOCUMENTATION.md`](../DOCUMENTATION.md) | Where docs live and how to update them |
| [`agent-dev-loop.md`](agent-dev-loop.md) | Build → verify → self-play workflow |

## Migrated to chuggie.dev (canonical versions on site)

These engine files remain for contributor context. **Edit the chuggie.dev MDX version for user-facing changes.**

| Engine file | User doc |
|-------------|----------|
| `perf-rules.md` | [chuggie.dev/docs/advanced/performance](https://chuggie.dev/docs/advanced/performance) |
| `gba-backgrounds.md` | [chuggie.dev/docs/engine/backgrounds](https://chuggie.dev/docs/engine/backgrounds) |
| `gba-audio.md` | [chuggie.dev/docs/engine/audio](https://chuggie.dev/docs/engine/audio) |
| `MEMORY.md` | [chuggie.dev/docs/advanced/memory](https://chuggie.dev/docs/advanced/memory) |
| `fighting-genre.md` | [chuggie.dev/docs/packages/fighter](https://chuggie.dev/docs/packages/fighter) |
| `deck.md` | [chuggie.dev/docs/packages/deck](https://chuggie.dev/docs/packages/deck) |
| `topdown-genre.md` | [chuggie.dev/docs/packages/topdown](https://chuggie.dev/docs/packages/topdown) |

## Internal / historical (contributor-only)

| Doc | Purpose |
|-----|---------|
| `engine-roadmap-status.md` | 2026-08 gap plan with status |
| `engine-review-2026-08.md` | Whole-stack review |
| `review-2026-08-14.md` | Dated review notes |
| `memory-perf-review-2026-07.md` | Memory/perf investigation |
| `tish-gba-issue-triage.md` | Issue triage notes |
| `gba-in-tish-core.md` | GBA footprint in tish compiler |
| `findings/` | P0 spike results and preserved probes |

## Reading order for new contributors

1. `../README.md` — get an example running
2. `../ARCHITECTURE.md` — understand the layers
3. `agent-dev-loop.md` — day-to-day workflow
4. `perf-rules.md` — before optimizing anything
5. `../INVENTORY.md` — find what you need
