# UI BUILDER VS LITERAL

> *The same screen authored two ways, measured — so the API choice is a number, not an opinion.*

Games declare UI as **object literals** today: a tree of boxed objects that `uiRender` walks, reads
field by field, flattens into the native pool, and throws away. The alternative is a **builder** —
calls that write straight into that pool, with no tree in between.

This example draws one screen (a panel of eight rows, nine nodes) both ways, proves the output is
**pixel-identical**, and prints what each costs.

![preview](preview.png)

## The numbers

| | build | draw | total | heap |
|---|---|---|---|---|
| **literal** | 830t | 4,672t | 5,502t | **2,816 B** |
| **builder** | 3,028t | 2,033t | 5,061t | **0 B** |

**The decision is not about speed.** The totals are within 8% — on this screen the drawing dominates
and the authoring style barely registers.

**It is about allocation.** The literal tree costs ~313 bytes per node, allocated only to be read
once by flatten and dropped. Nine nodes is 2,816 B; a 60-node shop tab is **~19 KB of churn per
open**, on a machine where that is the difference between the next dialog allocating and failing.
The builder allocates nothing.

**What the builder costs is ergonomics, and that is real.** A literal nests; a builder has to name
its parent. It also pays the text measurement up front rather than deferring it into flatten —
which is exactly why its `build` is 3.6× the literal's while its draw is half. A `pushText`-style
helper hides most of the verbosity, and this file shows both so the trade can be read directly.

## Running it

```
npm run build
../../tools/gba-shot ui-builder-vs-literal.gba /tmp/lit.ppm 100   # literal path
../../tools/gba-shot ui-builder-vs-literal.gba /tmp/bld.ppm 320   # builder path
cmp /tmp/lit.ppm /tmp/bld.ppm                                     # must be identical
```

`verify.sh` does the same and also asserts the screen is not blank.

## Three traps this example had to survive

Each of these made an earlier version of it report a **false** result, and each is commented at the
site in `src/main.tish`:

1. **A blank screen compares equal to a blank screen.** The first run "passed" while drawing
   nothing — `ui_reserve_tiles` was missing (the canvas had no VRAM) and the loop used `vblank()`
   instead of `frame()` (nothing was ever committed). Count distinct colours before trusting
   equality; this screen has 3.
2. **Lookalike colours are not the same colours.** The builder used a hand-picked gold and panel
   rather than the theme's, so the diff failed for a reason unrelated to authoring style. Both paths
   must use `DEFAULT_THEME`'s values.
3. **Timer2 wraps at 65536** (~250ms) and a render runs longer than that, so a raw subtraction goes
   negative. Every delta is corrected through `lap()`.

Building it also caught a real engine bug: `lay_set_paint` was reading the interned fill/border
colours *before* they were assigned, so container fills had silently stopped drawing. The three
screens checked during the paint switchover all happened not to fill a container, so the regression
compared identical and got through — see commit `d85d9755`.
