# itch.io mGBA WASM player

Static HTML shell used by `scripts/publish-itch.sh` to package a `.gba` ROM for
itch.io HTML5 uploads.

## Contents

| File | Role |
|------|------|
| `index.html` | Entry point (required at zip root by itch) |
| `player.js` | Loads `game.gba` into mGBA and starts emulation |
| `vendor/mgba.js` + `vendor/mgba.wasm` | [@thenick775/mgba-wasm](https://www.npmjs.com/package/@thenick775/mgba-wasm) 2.4.1 |

The publish script copies this tree and drops the ROM in as `game.gba`.

## License

`vendor/` is mGBA compiled to WebAssembly, **MPL-2.0**
(© mGBA contributors; wasm packaging by Nicholas VanCise). See
[thenick775/mgba](https://github.com/thenick775/mgba/tree/feature/wasm).

## SharedArrayBuffer (required)

This core uses threads. The host must send:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

On itch.io, enable **SharedArrayBuffer** / cross-origin isolation in the game’s
Embed options after uploading.

## Local preview

Do not open `index.html` via `file://`. From the repo root, after packaging:

```bash
npm run itch -- publish shmup
npm run itch -- serve shmup          # http://127.0.0.1:4173/  (COOP/COEP)
npm run itch -- serve shmup --port 8080
```

## Controls (mGBA defaults)

- **A** X · **B** Z · **L** A · **R** S · **Start** Enter · **Select** Backspace · D-pad arrows
