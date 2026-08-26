# UI DEMO

> *Demonstrates UI components, layouts, and rendering.*

![preview](preview.gif)

Thin showcase of **`packages/ui.tish`** — the demo only supplies data and a self-play loop; stream /
panel / list / detail-patch live in the shared library (same patterns as `shop-demo`).

What it uses:
- **`makePanel` / `makeSelector` / `makeDetailPanel`** — list + detail chrome
- **`sel.select(i)`** — shared list selection (owns `moveHi`; returns 1 if scroll needs a full paint)
- **`DET.patch`** — flash-free detail update
- **`uiPaint` / `uiPaintStep`** — streamed first open
- **`uiModal`** — toggled once per full lap of the list
- **Fonts** — alagard title, ark-pixel list, tinypixel detail

```bash
npm run build   # -> ui-demo.gba
npm run shot    # headless screenshot
npm start       # run in mGBA
```

See `examples/shop-demo` for the same helpers in a full BUY/SELL flow, and `examples/button-demo`
for **`makeButton`** / **`makePrompt`**.
