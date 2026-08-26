# MONO DEMO

> *Demonstrates monophonic audio or monochrome rendering.*

<img src="preview.png" alt="preview" width="480">

The tish component-authoring model, final form. Components are **exported object
literals**; each lifecycle hook takes a **single context object** and destructures only
what it needs; `mount()` auto-registers the exports by name. No register calls, no name
strings, no wrappers, no positional param ordering.

`src/components.tish`:
```tish
import { input_x, input_y } from 'cargo:tish_agb'

export const Player = {
  update: ({ me }) => { me.setVel(input_x() * 1.5, input_y() * 1.5) }
}
export const Coin = {
  onCollide: ({ me }) => { me.despawn() }     // could also destructure `other`
}
```

`src/main.tish`:
```tish
import { mount, create, step } from '../../../packages/engine'
import * as components from './components'

mount(components)              // registers Player + Coin by their export names

let hero = create()
hero.setSprite(sprite_new(player))
hero.behave('Player', {})
// ... coins ...
while (true) { step() }
```

Each hook is invoked with one context object: `update`/`start` get `{ me, dt }`,
`onCollide` gets `{ me, other }`. A hook destructures the subset it uses (`({ me })`,
`({ dt })`, `({ me, other })`) or takes nothing (`() => …`). `me` is the rich entity
(`me.setVel`, `me.despawn`, `me.x`, `me.data`, `me.overlaps(other)`).

Everything here — namespace import, `Object.keys` enumeration, param destructuring —
compiles through native codegen with zero tish-compiler changes. (`me`, not `self` — see
tishlang/tish#549.)
