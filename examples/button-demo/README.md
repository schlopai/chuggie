# BUTTON DEMO

> *Demonstrates reading hardware input from the GBA buttons.*

![preview](preview.png)

Showcase for **`makeButton`**, **`makeButtonGroup`**, and **`makePrompt`** in [`packages/ui.tish`](../../packages/ui.tish).

```tish
let menu = makeButtonGroup([
  { label: "Continue", icon: PROMPT_A },
  { label: "Quit", icon: PROMPT_B },
], { size: 32 })

uiRender({ children: [menu.view(), …] })
menu.nav()                 // Up/Down, flash-free
if (menu.act() === 1) { … } // A
uiSetText(msg, "Saved.")   // in-place status line
```

`makeButtonGroup` owns cursor + `buttonStyle` / `buttonPaint`. Games only describe items and call `nav` / `act`.

Controls: **Up/Down** select · **A** activate · **B** clear · **Start** prompt gallery.

```bash
npm install && npm start
```
