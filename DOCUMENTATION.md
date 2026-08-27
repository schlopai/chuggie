# Documentation policy

Where documentation lives across the Chuggie repositories and how to keep it in sync.

## Repositories

| Repo | Role | Audience |
|------|------|----------|
| **chuggie.dev** | Marketing site + MDX docs | Game developers |
| **chuggie** | Engine source, examples, crates | Contributors + game developers (via chuggie.dev) |

## Where to write what

| Content | Location |
|---------|----------|
| User guides, package docs, getting started | `chuggie.dev/content/docs/` (canonical) |
| Architecture, ABI contract, inventory | `chuggie/ARCHITECTURE.md`, `CONTRACT.md`, `INVENTORY.md` |
| Contributor workflow, agent loops, reviews | `chuggie/docs/` (internal) |
| API truth (typed surface) | `crates/*/tish.d.tish` + Rust `///` comments |
| Example-specific docs | `examples/<name>/README.md` |
| Crate overview | `crates/README.md` |
| Scripts catalog | `scripts/README.md` |

## User-facing docs on chuggie.dev

**chuggie.dev is the single source of truth** for anything a game developer needs. When migrating content from `docs/`:

1. Adapt (don't copy verbatim) to MDX format
2. Add canonical banner to the engine source file pointing to chuggie.dev
3. List the mapping in `docs/README.md`

Migrated guides:

| Engine file | chuggie.dev page |
|-------------|------------------|
| `docs/perf-rules.md` | `/docs/advanced/performance` |
| `docs/gba-backgrounds.md` | `/docs/engine/backgrounds` |
| `docs/gba-audio.md` | `/docs/engine/audio` |
| `docs/MEMORY.md` | `/docs/advanced/memory` |
| `docs/fighting-genre.md` | `/docs/packages/fighter` |
| `docs/deck.md` | `/docs/packages/deck` |
| `docs/topdown-genre.md` | `/docs/packages/topdown` (contributor notes remain in engine) |

## Adding a new package

1. Create `packages/<name>.tish` in chuggie
2. Add entry to `packages/README.md` module table
3. Update `INVENTORY.md` genre table
4. Add canonical example under `examples/`
5. Create `chuggie.dev/content/docs/packages/<name>.mdx`
6. Update `packages/manifest.json` (machine-checkable inventory)

## Package manifest

`packages/manifest.json` is the machine-checkable list of all authoring modules. Used to cross-check:

- `packages/README.md` module table
- chuggie.dev sidebar package pages
- `INVENTORY.md` counts

Regenerate or verify after adding/removing packages.

## Landing page claims

Every genre or feature named on chuggie.dev landing must have a docs page. Audit `src/LandingPage.tsx` `GENRES` array against `content/docs/packages/` after adding packages.

## Changelog

No `CHANGELOG.md` — releases use Conventional Commits + sem. GitHub releases are the changelog.

## Contributing

See `CONTRIBUTING.md` in each repo.
