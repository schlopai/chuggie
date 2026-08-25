# warsong — Warsong Gulch battlegrounds

3v3 CTF on the top-down engine: soft targeting (R), A/B primary/secondary skills, L skill wheel,
double-tap dash, classic WoW classes × 3 specs, frame-loop bots.

## Map

The arena is a **Tiled** map: [`assets/wsg.tmj`](assets/wsg.tmj) with a **local tileset**
([`wsg_tiles.tsj`](assets/wsg_tiles.tsj) / [`wsg_tiles.png`](assets/wsg_tiles.png)) cropped from
Ninja Adventure dirt/grass/wall fills (seamless — no per-cell outlines), faction-tinted bases,
and a mud mid run. Not the soccer/golf field tileset.

Characters are 16px class sprites ([`wsg16.png`](assets/wsg16.png)): Knight, Hunter, Monk, Mage, etc.,
with Alliance/Horde color badges. Movement uses engine `set_topdown` + double-tap dash.

Regenerate:

```bash
npm run assets --workspace=warsong
```

## Controls

| Input | Action |
|-------|--------|
| D-pad | move; double-tap same dir = dash |
| R | soft-target nearest foe |
| A | primary skill |
| B | secondary skill |
| Hold L + U/D/L/R/A/B/Select | skill wheel (slots 0–5 and 7) |
| Select (no L) | clear target |
| Start | skip class select / (in match) noop |

Class select is a `packages/ui` panel listing all eight skills with icons and bindings
(`A`, `B`, and `L+` wheel keys). Match HUD uses `makeBar` for HP / MP / target meters.

## Build

```bash
npm run assets --workspace=warsong
npm run build --workspace=warsong
npm start --workspace=warsong
```
