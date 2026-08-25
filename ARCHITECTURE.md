# chuggie-engine architecture — layers & separation of concerns

What lives where, and the dependency rules that keep the layers clean. This is the map;
`CONTRACT.md` pins the exact compiler↔framework wire format, and the tish repo's
`docs/gba-target.md` covers the compiler-side (how little GBA code lives in the language).

## The layers (bottom → top)

```
         agb 0.25 (upstream: hardware, VRAM, mixer, fixnum)
                    ▲                    ▲
                    │                    │
   ┌────────────────┴───────┐   ┌────────┴─────────────────────────────┐
   │ ③ tish_runtime_gba     │   │ ④a tish-agb  (this repo)             │
   │    THE BOUNDARY         │   │     idiomatic agb bindings           │
   │    (lives in tish repo) │◀──│     GbaCtx, handle arenas, input,    │
   │    no_std runtime glue;  │   │     sprites, bg, audio, camera,      │
   │    Value surface; Fixed; │   │     tilemaps. #[tish_export] fns.    │
   │    gba::init/halt/hooks; │   └────────┬─────────────────────────────┘
   │    asset arenas          │            │  drives (handles ARE tish-agb handles)
   └────────────────┬─────────┘   ┌────────┴─────────────────────────────┐
                    │             │ ④b tish-gba-game-engine (this repo)  │
                    └─────────────│     SoA entity store + fixed pipeline │
                    facade        │     + genre modules. Drives tish-agb; │
                    (Value, Fixed)│     NO direct agb dependency.         │
                                  └────────┬─────────────────────────────┘
                                           │  cargo: / import
                          ┌────────────────┴─────────────────────────────┐
                          │ ④c packages/*.tish  — tish-side sugar         │
                          │ ④d examples/*        — the games              │
                          └───────────────────────────────────────────────┘
```

## Who may depend on what

| Crate | May depend on | Must NOT depend on | Role |
|---|---|---|---|
| **tish_runtime_gba** (facade, in the **tish** repo) | `agb`, `tishlang_core`/`tishlang_builtins` (portable) | tish-agb, the engine | The boundary. The compiled tish program's runtime: `Value`, `Fixed`, `gba::{init,halt,hooks}`, the `asset:`/`background:`/`map:`/`wav:` arenas. Versions in lockstep with the compiler. |
| **tish-agb** | `agb`, the facade | the engine | Idiomatic agb wrapper. Handle arenas (sprites/backgrounds/channels), input, deferred draw, per-frame driver, tilemaps/streaming. The low level a game can build against *alone*. |
| **tish-gba-game-engine** | tish-agb, the facade | **`agb` directly** | The RPG-Maker-class framework: SoA store, fixed per-frame pipeline, genre modules, dialogue, camera. **Drives tish-agb** — it does not own rendering or poke hardware. |
| **packages/** (`.tish`) | any `cargo:` crate above | — | tish-source ergonomics (method-object sugar over the handle APIs). |
| **examples/** (`.tish`) | packages + `cargo:`/`asset:` | — | The games. |

**The one rule to remember:** dependencies only ever point *down and toward agb*. The
engine drives tish-agb; tish-agb wraps agb; both share the facade's `Value`/`Fixed`
vocabulary. Nothing in this repo reaches "sideways" or "up".

### Why the engine has no direct `agb` dependency

The engine is agb-*coupled by design* (hot path, fixed-point positions), but it declares
no `agb` dependency. Hardware and rendering go through **tish-agb**; the fixed-point
`Fixed` type comes from the **facade** (`tishlang_runtime_gba::Fixed`, the canonical
definition per CONTRACT §5). agb still resolves transitively — so agb's inherent `Num`
methods stay callable — but keeping the dependency *declaration* out of the engine makes
the layering explicit and prevents the engine from quietly reaching past tish-agb into raw
hardware APIs. If the engine ever needs an agb type tish-agb doesn't surface, add it to
tish-agb (or re-export it from the facade) rather than depending on `agb` in the engine.

## Where does new code go?

- **"The hardware/agb can do X, expose it to games"** → **tish-agb**. A handle-returning,
  `#[tish_export]`-marked idiomatic wrapper. No `Value` marshalling (the compiler generates
  that), no game rules.
- **"Cross-entity game-system behavior (movement, collision, camera, dialogue, a genre)"**
  → **tish-gba-game-engine**. Consumes tish-agb handles; writes intent that the pipeline
  resolves. Never touches `agb::` directly.
- **"A new bake-a-file-into-ROM import kind"** (e.g. a new sprite/tile/audio format) →
  a scheme entry in **`crates/tish-agb/tish.schemes.json`** + a facade arena/register fn.
  **Zero tish-compiler edits** — the registry is generic (see `docs/gba-in-tish-core.md`).
- **"Runtime glue the *generated code* calls"** (a new `tishlang_runtime::…` name, an
  `asset:` arena, an executor hook) → the **facade** in the tish repo, and pin it in
  `CONTRACT.md` (breaking change → update both sides in lockstep).
- **"Ergonomic tish API over the handles"** → **packages/**.

## The cross-repo seam

The facade `tish_runtime_gba` physically lives in the **tish** repo (it versions with the
compiler — the generated crate depends on it under the renamed `tishlang_runtime`), but it
is *the interface this repo builds against*. tish-agb and the engine reference it by path.
`CONTRACT.md` is the source of truth for that interface; any change to the prelude surface,
handle ABI, `gba` module, numeric model, or scheme templates is a lockstep change.

### agb version — single source of truth

Every crate in the ROM build graph must resolve to the **same** agb (a skew → two agb
crates → a type mismatch at the sprite-registration boundary). The **facade's `Cargo.toml`
is the source of truth**: the tish compiler reads the agb version from it when generating
the ROM crate (`read_facade_agb_version`), and tish-agb pins the same `0.25.0`. Cargo then
unifies them to one agb within the build. When bumping agb, change the **facade** first,
then tish-agb to match.
