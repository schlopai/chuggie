# REPRO INTRO HEAP

> *Where the large SRPG example's memory actually goes — measured in 30 seconds instead of guessed at over five-minute
> game builds.*

The large SRPG example (since moved to the chuggie-tactics repo) reaches its tutorial battle with ~21.5 KB free and dies partway through the first
turn on a 96-byte allocation. Three plausible culprits were proposed and all three were wrong. This
example reproduces just the opening's UI calls and prints the heap after each step.

## What it measures

| step | free heap | delta |
|---|---|---|
| boot | 242,368 | |
| **`ui_reserve_tiles(320)`** | 198,016 | **−44,352** |
| `ui_begin` + dialogue band | 195,712 | −2,304 |
| after line 1 | 194,816 | −896 |
| after all 14 lines | 194,816 | **0** |
| `ui_clear` → `ui_release_scratch` | 197,824 | +3,008 |
| second `ui_begin` + band | 195,712 | same as the first |
| `ui_reserve_tiles(0)` | 197,824 | **0 — nothing comes back** |

## What that settles

1. **The opening is not a leak.** Fourteen dialogue lines cost ~3 KB in total, the lines after the
   first cost *nothing* (one `ui_begin`, then the text boxes are patched), and the teardown returns
   what it took. A second canvas costs exactly what the first did, so the memory is genuinely reused.
2. **The tile reserve is the big holder: 44 KB**, taken at boot, for a canvas the battle never uses.
3. ⚠️⚠️ **It cannot be given back.** `ui_reserve_tiles(0)` frees nothing and re-reserving costs
   nothing — agb's DynamicTile map grows once and never shrinks. So "release the canvas before the
   fight" is not available: the reserve is a permanent boot-time tax, and the only lever is choosing
   a smaller number up front.

This corrects two earlier hypotheses that cost real time: that the opening leaked ~46 KB (it does
not), and that `ui_clear`/`ui_release_scratch` were failing to return dropped tiles (they return
everything they took — the tiles were never the cost).

## Running it

```
npm run build
../../tools/gba-shot repro-intro-heap.gba /tmp/x.ppm 160    # with GBA_SHOT_LOG=1
```
