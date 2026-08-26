# SHOP DEMO

> *Demonstrates a shop UI and inventory management.*

<img src="preview.gif" alt="preview" width="480">

A tour of **`packages/shop.tish`** — the reusable BUY / SELL shop, built on `packages/ui.tish`
(layout + selectable list) and `packages/dialog.tish` (the shopkeeper). The shop owns the flow and
presentation; the game plugs in the data.

## Patterns this demo exercises

- **Dialog** — streamed page open, choice height reserved while typing (no second full body paint),
  `instantChoices` on the greet menu, `uiKeys` / in-place choice recolour
- **Shop tab** — streamed open + deferred detail, `makePanel` / `makeListRow` / `makeDetailPanel` /
  `makeStepper`, `moveHi` + detail settle, `uiRelayoutInner` for qty toggle, batched `uiKeys`

## Flow

A merchant greets you (portrait + typewriter dialog) and offers **Buy / Sell / Leave**. Each tab is a
scrollable list (name · price, with quantity owned on the sell side), a detail panel with the item's
icon + description, and your gold. Choosing an item opens a **quantity picker** — `L/R ±1`, `U/D ±10`,
with a running **Total** and the **gold you'll have after**. It self-plays a buy → sell → leave loop;
play it for real with the d-pad + A/B.

## Using it

```tish
import { shopInit, shopOpen, shopUpdate, shopActive } from '../../../packages/shop'

shopInit({ font: body })
shopOpen({
  title: "Item Shop",
  keeper: { name: "Merchant", portrait: 8, greet: ["Welcome!", "Take a look."] },
  mode: "greet",                    // or "buy" / "sell" to jump straight into a tab
  stock: [ { id, name, icon, price, desc } ],        // wares to buy
  bag:   () => [ { id, name, icon, price, qty, desc } ],  // inventory to sell (live; price = sell value)
  gold:  () => playerGold,
  onBuy:  (id, qty, cost) => { /* deduct + add; return 1 on success */ },
  onSell: (id, qty, gain) => { /* add gold + remove; return 1 */ }
})
while (shopActive() > 0) { shopUpdate(); frame() }
```

`shop.tish` is scriptable (`shopNav` / `shopAct` / `shopBack` / `shopQty`, plus `dialogMove` /
`dialogAdvance` for the greeting) so a cutscene — or this demo — can drive it without live input.

## Assets

Ware icons + the shopkeeper portrait are catalog art (Ninja Adventure Skill-Icons + a Villager
faceset), packed into one `sheet32:` strip by `scripts/gen_shop_demo.py`.

```bash
python3 ../../scripts/gen_shop_demo.py   # (re)build assets/shop32.png from the catalog
npm run build   # -> shop-demo.gba
npm run shot    # headless screenshot
npm start       # run in mGBA
```
