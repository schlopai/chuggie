# repro-uibar

`uiRender` laid out every node with the **previous** render's geometry, from the second render on.

`packages/ui.tish` imported `lay_reset` but never called it. The native layout pool
(`crates/tish-agb/src/ui_layout.rs`) therefore only grew: the second render pushed its nodes after
the first screen's, while the write-back pass read slots `0..count-1`. Every box came back offset by
the first tree's node count.

Visible symptom: a `makeBar` fill (`70x4`) receiving a container's box painted a screen-sized block
of the `good` green over the game — warsong's match screen after leaving the class-select screen.

```bash
npm run build && GBA_SHOT_LOG=1 ../../scripts/screenshot.sh repro-uibar.gba out.png 200
```

Both the `makeBar` shape and the identical inline shape must report `72x6` / `70x4`:

```
bar    track w=72 h=6 -> _w=72 _h=6
bar    fill  w=70 h=4 -> _w=70 _h=4
inline track w=72 h=6 -> _w=72 _h=6
inline fill  w=70 h=4 -> _w=70 _h=4
```

Before the fix the second render reported `_w=0 _h=0` / `_w=236 _h=0`. It is not a `makeBar` bug —
the inline copy breaks the same way, and one render alone is always correct.
