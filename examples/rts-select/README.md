# RTS SELECT

<img src="preview.gif" alt="preview" width="480">

RTS de-risk A3: cursor selection, order issue and attack-move over a live world, with a panel that
repaints only when it changes.

`rts-flow` and `rts-fog` proved that units can *move* and that the player can *see*. This one asks
whether a player can **command** them from a d-pad, and whether units fight on their own once they
get there.

| Input | Action |
|---|---|
| D-pad | move the cursor |
| A | on your unit: select it · on open ground: order the selection there |
| L | cycle one unit → the whole army |
| B | clear the selection and halt it |

Four enemies march on your camp from the first frame on their own flow field. Nothing orders anyone
to fight — they fight because `set_soldier` says to.

## Result — PASS

```
[frame 398] P4970 E4375 K0 S6 R2
[frame 782] P4389 E4375 K4 S6 R2
[frame 2190] P4389 E4375 K4 S6 R2
```

`K` enemies killed · `S` units selected · `R` panel repaints · `E` the EMA against 4,389 ticks.

- **`K4` by frame 782** — the enemy crossed the whole three-barrier map, was acquired, closed with
  and killed, with no input beyond one `L` tap.
- **`R2` over 2,200 frames** — the panel painted at boot and once more when the selection changed.
- **EMA 4,375, peak 4,389** — ten units, two live flow fields, combat and a HUD, on budget.

**Two live flow fields is the point of fields being a small numbered set.** Field 0 is whatever the
player last ordered; field 1 is the enemy advance. Neither disturbs the other, and re-ordering one
army does not re-path the other.

Selection is a **bitmask**, not a list: "is slot k selected" is a shift and an and, and the whole
army fits in a register.

## Two bugs this spike caught

1. **`set_soldier` acquired only within its swing range.** With one radius for both acquiring and
   attacking, a unit could only ever fight what it physically bumped into — the two armies walked
   through each other and `K` stayed 0 for 2,500 frames. A soldier now acquires at **4× its range**
   and walks the rest, which is what makes this attack-*move*. `seek_system` had to stop zeroing the
   movement intent of a fighting unit for the same reason, or it could never close those last pixels.
2. **`&` binds looser than `===`.** `(m >> k) & 1 === 1` parses as `(m >> k) & (1 === 1)`, so the
   selection counter silently reported 0 while the selection itself was correct. tish inherits
   JavaScript's precedence here and there is no warning; the parentheses are load-bearing.

## The canvas discipline

`ui_release_scratch()` is called immediately after the paint. Without it the UI canvas keeps ~48KB
of scratch, which the next scene inherits as corruption — and because the symptom appears in a
*later* scene, it is close to unfindable if it is not done as a habit here.

## Build

```bash
npm run assets --workspace=rts-select
npm run build --workspace=rts-select
bash verify.sh
```
