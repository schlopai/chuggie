# NinjaGreen hand / mitt index

**Source of truth:** [`ninja_green_hands.json`](ninja_green_hands.json)
**Regenerate:** `python3 scripts/index_ninja_green_hands.py`
**Visuals:** `examples/akari/assets/attack-index/HANDS_INDEX/`

Mitt RGB `(239, 145, 79)` = gloves **and** shoes. **Every occupied cell** on both
NinjaGreen variants is indexed (hand/foot clusters, centroids, pixels, `sword_grip`).

## Coverage

- **CharacterAnimated 32px:** 244 occupied (SpriteSheet 122/136 + 12 Separate sheets)
- **Character 16px:** 60 occupied (9 sheets)
- **Grand total:** 304 occupied cells

### CharacterAnimated Separate (32px)

| Sheet | Cells | Occupied |
|-------|------:|---------:|
| SpriteSheet 8×17 | 136 | 122 |
| Separate/Attack | 16 | 16 |
| Separate/Climb | 4 | 4 |
| Separate/Dead | 2 | 2 |
| Separate/Hit | 8 | 8 |
| Separate/Idle | 16 | 16 |
| Separate/Item | 2 | 2 |
| Separate/Jump | 12 | 12 |
| Separate/Pickup | 2 | 2 |
| Separate/Push | 16 | 16 |
| Separate/Roll | 12 | 12 |
| Separate/Swim | 16 | 16 |
| Separate/Walk | 16 | 16 |

### Character SeparateAnim (16px)

| Sheet | Cells | Occupied |
|-------|------:|---------:|
| Attack | 4 | 4 |
| Dead | 1 | 1 |
| Idle | 4 | 4 |
| Item | 1 | 1 |
| Jump | 4 | 4 |
| Special1 | 1 | 1 |
| Special2 | 1 | 1 |
| SpriteSheet | 28 | 28 |
| Walk | 16 | 16 |

## SpriteSheet 32px layout

- **rows 0-3:** Idle (cols 0-3 dirs DN/UP/LF/RT × frames 0-3) | Attack (cols 4-7). Attack DNf2=DNf3 and UPf2=UPf3 share cells.
- **rows 4-5:** Walk DN/LF/RT (col0/2/3); Walk UP sparse (col1). Hit (cols 4-7, 2 frames).
- **rows 6-7:** Walk continued | Roll (cols 4-7, frames 0-1).
- **rows 8:** Swim f0 (cols 0-3) | Roll f2 (cols 4-7).
- **rows 9-11:** Swim f1-f3 (cols 0-3) | Push f0-f2 (cols 4-7).
- **rows 12:** Jump mixed (cols 0-3) | Push f3 (cols 4-7).
- **rows 13-14:** Jump continued (cols 0-3) | Dead/Climb/Pickup/Item (cols 4-7).
- **rows 15-16:** Climb f2-f3 at (5,15)/(5,16) only.

## Attack `sword_grip` (32px Separate/Attack — use for weapons)

| Dir | f0 | f1 | f2 | f3 |
|-----|----|----|----|----|
| DN | `(10,10)` | — | `(12,19)` | `(12,19)` |
| UP | `(24,10)` | — | `(8,22)` | `(8,22)` |
| LF | `(18,20)` | — | `(8,20)` | `(8,20)` |
| RT | `(12,20)` | — | `(24,20)` | `(24,20)` |

## All Separate Attack hands (every cluster)

- **DNf0** sword_grip=[10, 10] hand=hand0 — hands: [hand0@(10,10), hand1@(12,15), hand2@(16,15), hand3@(19,15)] feet: []
- **DNf1** sword_grip=None hand=None — hands: [hand0@(12,20), hand1@(16,20), hand2@(19,20)] feet: [foot0@(13,24)]
- **DNf2** sword_grip=[12, 19] hand=hand0 — hands: [hand0@(12,19), hand1@(16,19), hand2@(19,19)] feet: [foot0@(13,23)]
- **DNf3** sword_grip=[12, 19] hand=hand0 — hands: [hand0@(12,19), hand1@(16,19), hand2@(19,19)] feet: [foot0@(13,23)]
- **UPf0** sword_grip=[24, 10] hand=hand0 — hands: [hand0@(24,10)] feet: []
- **UPf1** sword_grip=None hand=None — hands: [hand0@(8,22)] feet: []
- **UPf2** sword_grip=[8, 22] hand=hand0 — hands: [hand0@(8,22)] feet: []
- **UPf3** sword_grip=[8, 22] hand=hand0 — hands: [hand0@(8,22)] feet: []
- **LFf0** sword_grip=[18, 20] hand=hand0 — hands: [hand0@(18,20)] feet: []
- **LFf1** sword_grip=None hand=None — hands: [hand0@(20,20)] feet: []
- **LFf2** sword_grip=[8, 20] hand=hand0 — hands: [hand0@(8,20), hand1@(18,20)] feet: []
- **LFf3** sword_grip=[8, 20] hand=hand0 — hands: [hand0@(8,20), hand1@(18,20)] feet: []
- **RTf0** sword_grip=[12, 20] hand=hand0 — hands: [hand0@(12,20)] feet: []
- **RTf1** sword_grip=None hand=None — hands: [hand0@(12,20)] feet: []
- **RTf2** sword_grip=[24, 20] hand=hand1 — hands: [hand0@(12,20), hand1@(24,20)] feet: []
- **RTf3** sword_grip=[24, 20] hand=hand1 — hands: [hand0@(12,20), hand1@(24,20)] feet: []

Weapon seating in `scripts/gen_akari.py` MUST read Attack `sword_grip` from this catalog.

