# chuggie-engine

Write Game Boy Advance games in **[tish](https://tishlang.com/)** (a TS/JS-like
language that transpiles to Rust), running on **[agb](https://github.com/agbrs/agb)
0.25**. Two layers, both usable:

- **`tish-agb`** — low-level bindings (sprites, backgrounds, input, audio, save,
  timers, rng, log). Enough to build a whole game by itself.
- **`tish-gba-game-engine`** — an optional RPG-Maker-class framework on top: entities +
  components with Unity-like `start`/`update` behaviours, a fixed per-frame
  pipeline, dialogue, scenes, and pluggable genre modules (grid RPG, platformer,
  isoboard battles, versus fighting). Framework games can always drop down to `tish-agb` / raw agb.

Layer map and dependency rules are in [`ARCHITECTURE.md`](./ARCHITECTURE.md); the
plan and phasing live in the approved strategy doc; the cross-track interface is
pinned in [`CONTRACT.md`](./CONTRACT.md); spike results in
[`docs/findings/P0-findings.md`](./docs/findings/P0-findings.md).

**Screenshots (headless):** `scripts/screenshot.sh <rom.gba | src/main.tish> [out.png] [frames]`
renders a ROM in libmgba with no window (no display / screen-recording permission), so it works
locally and in CI — see [`.github/workflows/screenshot.yml`](./.github/workflows/screenshot.yml).
`scripts/gif.sh` takes the same arguments and records the run as a looping animated GIF instead,
for everything a still cannot show — movement, animation, transitions, particles.

**Working on an example:** the whole build → drive-headlessly → self-play → strip-probes loop, with
the traps that cost the most time, is written up in
[`docs/agent-dev-loop.md`](./docs/agent-dev-loop.md).

**Backgrounds:** there are only four layers and they are all spoken for — the budget, the priority
rule that decides whether the player is visible at all, the one-palette-set constraint, and how to
get more apparent depth than you have layers (per-scanline banding) are in
[`docs/gba-backgrounds.md`](./docs/gba-backgrounds.md).

**Performance:** [docs/perf-rules.md](docs/perf-rules.md) — the seven things that cost a GBA
frame in tish, measured. `scripts/const_to_let.py` applies the biggest one in bulk.

## Status

Bootstrapping (**P0**). Done: toolchain validated, workspace scaffold, and the two
de-risking spikes —
- **P0a ✅** a hand-written no_std mock of generated code builds for
  `thumbv4t-none-eabi` and produces a valid `.gba` ROM (`examples/p0-spike`).
- **P0c ✅ (negative)** hecs can't compile on GBA (atomics) → engine uses a custom
  SoA store.

Two key plan corrections came out of P0: the portable value map hashes with
**FxHasher**, not foldhash (foldhash needs atomics), and **hecs is out**.

## Prerequisites

- **Node.js 22+** and **npm** — games are driven through npm scripts (they use the `tish` CLI, which
  ships as the [`@tishlang/tish`](https://www.npmjs.com/package/@tishlang/tish) npm package).
- Rust **nightly** with `rust-src` (pinned via `rust-toolchain.toml`) + `agb-gbafix`
  (`cargo install agb-gbafix`) — the tish GBA build shells out to cargo under the hood.
- `mgba-qt` / `mgba` on `PATH` to play ROMs (`brew install mgba`).

## Quick start — build & run a game

Every example is a normal npm project that depends on the `tish` CLI. Pick one and:

```bash
npm install                    # once, at the repo root — links the tish CLI for all examples
npm start -w fonts-demo        # build the ROM and open it in mGBA
```

or from inside an example:

```bash
cd examples/fonts-demo
npm run build                  # → fonts-demo.gba
npm start                      # build + open in mGBA
npm run shot                   # build + headless screenshot.png (no window; for CI / quick checks)
npm run gif                    # build + headless screenshot.gif — an animated clip of the same run
npm run clean                  # remove build artifacts
```

That's it — no long `tish build … --target gba -o …` incantations, no separate emulator command.

### From the editor (VS Code / Cursor)

Every example has a **Build** and a **Play** action. Play runs the ROM already on disk and never builds,
so it opens instantly — build only when you changed something.

- **Run and Debug** sidebar: pick `▶ Play <example>` once; the green play button / <kbd>F5</kbd> replays it.
- **Run Task** (<kbd>⇧⌘P</kbd>): `Play: <example>` / `Build: <example>` for any example, plus
  `Play: current example` / `Build: current example`, which use whichever example the open file belongs to
  (editing `packages/*.tish`, they fall back to the last example you ran). `Build: current example` is the
  default build task, so <kbd>⇧⌘B</kbd> builds what you are looking at.

The same thing from a terminal, and what the actions actually call:

```bash
scripts/rom.sh play akari      # run examples/akari's ROM — no build
scripts/rom.sh build akari     # build it
npm run vscode                 # regenerate .vscode/*.json after adding an example
```

Play always starts mGBA **windowed** — mGBA otherwise remembers fullscreen and re-enters it for every
ROM, and macOS puts a fullscreen window on its own Space, which hides your editor. Window size is
whatever you last dragged it to; `MGBA_ARGS="--scale 4"` (or any other emulator flags) and
`MGBA=/path/to/emulator` override the rest.

> **Local dev against a sibling `tish` checkout.** GBA support may not be in the published
> `@tishlang/tish` yet, so the examples reference the tish CLI **locally** (a `file:` dependency on
> `../tish/tish/npm/tish`). Run **`npm run setup`** once (it builds that checkout's `tish` and makes it
> installable); then `npm install` uses your local build. A real game just depends on the published
> `@tishlang/tish` and skips this step. Point `npm run setup` elsewhere with `TISH_REPO=/path/to/tish`.

## The p0-spike (hand-written Rust ROM)

`examples/p0-spike` is the P0a spike — hand-written no_std Rust (what tish codegen emits), built with
cargo rather than tish. It has npm scripts too (`npm run build -w p0-spike`), or directly:

```bash
cd examples/p0-spike
cargo build --release
agb-gbafix ../../target/thumbv4t-none-eabi/release/p0-spike -o p0-spike.gba
cargo run --release   # boot interactively (streams agb::println! to the terminal)
```

## Publish to itch.io

Package any example as an HTML5 build (ROM + mGBA WASM player) plus cover art
from a headless screenshot:

```bash
npm run itch -- publish shmup
# → dist/itch/shmup/shmup-html5.zip
#    dist/itch/shmup/cover.png        (630×500, upload as Cover Image)
#    dist/itch/shmup/embed-bg.png     (480×320, Click-to-play background)
```

Then on the itch game edit page: Kind = **HTML**, upload the zip, set embed size
**480×320**, enable **SharedArrayBuffer** / cross-origin isolation, upload
`cover.png`, and set the Click-to-play background to `embed-bg.png`.

Optional butler push (directory, not the zip):

```bash
ITCH_TARGET=you/your-game:html5 npm run itch -- publish shmup
```

Local preview (COOP/COEP headers for mGBA WASM threads):

```bash
npm run itch -- serve shmup          # http://127.0.0.1:4173/
npm run itch -- serve shmup --port 8080
```

See also [`templates/itch-mgba/README.md`](./templates/itch-mgba/README.md).

## Layout

```
.cargo/config.toml     # thumbv4t target, build-std, gba.ld rustflags, mgba runner
rust-toolchain.toml    # nightly + rust-src
Cargo.toml             # workspace + GBA-tuned profiles (opt-level 3, fat LTO)
CONTRACT.md            # compiler ⇄ framework interface (breaking changes tracked here)
docs/findings/         # P0 spike results + preserved probes
examples/p0-spike/     # P0a: hand-written "what codegen emits", builds to a ROM
templates/itch-mgba/   # mGBA WASM HTML shell for itch HTML5 packages
```

`crates/` (tish-agb, tish-agb-macros, tish-gba-game-engine, host asset importer) land from
P1 onward. See `CONTRACT.md` "Workspace host/target split" for why host tooling
isn't a plain workspace member.
