# FX DEMO

> *Demonstrates visual effects, blending modes, and palettes.*

<img src="preview.gif" alt="preview" width="480">

One-shot attack / magic VFX from the **Ninja Adventure** pack (`sheet32:`),
paired with PSG sound effects from `packages/chipsfx` and a looping chiptune bed.

```bash
npm start
```

| Input | Action |
|-------|--------|
| **Left / Right** | Prev / next effect |
| **A** | Replay (animation + SFX) |

Effects auto-replay every ~1.5 s. Music keeps playing underneath — SFX borrow a
PSG channel briefly via `chip_borrow`, then the sequencer reclaims it.
