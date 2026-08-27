# deck on GBA — agent music guide

> **Canonical version:** https://chuggie.dev/docs/packages/deck

Author music as **deck** (`.deck`), import it with `deck:`, play it with `deckPlay`. The GBA never
parses deck text — the host bakes a compact `DeckSong` at build time.

## Quickstart

```text
deck 1
bpm 120

track Lead id lead gen gameBoyDmg
  gen type pulse duty 50 vol 12
  note 60 0 0.5 v 100
  note 64 0.5 0.5 v 90
  note 67 1 1 v 100

track Bass id bass gen gameBoyDmg
  gen type wave wave_shape saw vol 15
  note 36 0 2 v 110
```

```tish
import { theme } from 'deck:../assets/theme.deck'
import { deckPlay, deckStop } from '../../../packages/deck'
deckPlay(theme)
```

## Hard limits

| Engine (UI) | deck `gen` | Cap |
|---------------------|-----------|-----|
| LR35902 | `gameBoyDmg` pulse | 2 (HW ch1–2) |
| LR35902 | `gameBoyDmg` wave | 1 (ch3) |
| LR35902 | `gameBoyDmg` noise | 1 (ch4) |
| GBA PCM | `gbaDirectSound` | 6 concurrent |

Overflow → **compile error** naming the track. No runtime voice stealing.

⚠️ 6 is the *music* budget, not the hardware's. agb's mixer has 8 channels in total and sound
effects come out of the same eight; whether 6 fits the frame is a question for the soak, not for
this table. GBA hardware itself has 2 DirectSound FIFOs — everything above that is software
mixing, paid for in CPU.

Square channels cannot go below **C2**; put bass on **wave**.

`chip_play` and `deck_play` are **mutually exclusive** on the PSG (each stops the other). `chipsfx`
still works via `chip_borrow`.

## Role recipes

- **Lead** — `gameBoyDmg` `type pulse` duty 50
- **Bass** — `gameBoyDmg` `type wave` `wave_shape saw` (or sine)
- **Hats / short PCM** — `gbaDirectSound` `waveform triangle` short `adsr` decay
- **Kick** — `gbaDirectSound` with `pitch_drop -12` + short decay, or `gameBoyDmg` noise

## GBA-supported grammar

```text
deck 1
bpm <40..300>
wave <name> <32 hex nibbles>          # optional named PSG wavetable
track <Name> id <id> gen gameBoyDmg|gbaDirectSound [* <bars>] [layer <0..3>]
  gen <key> <val> …
  layer <0..3>                        # alias: intensity / min_intensity
  adsr a <s> d <s> s <0..15|0..1> r <s>
  mix gain <0..1>                     # scales velocity only
  note <midi> <startBeat> <durBeats> v <vel>
  steps <16 x/. tokens>               # if no notes
  step_pitch <midi>
```

Beats are quarter notes. `* N` repeats a 1-bar pattern across N bars when notes fit in one bar.

## Intensifier stems (crossfading-stem-style)

All stems share one playhead. Mark a track with `layer N` (0–3). At runtime:

```tish
import { deckPlay, deckSetIntensity } from '../../../packages/deck'
deckPlay(siege)
deckSetIntensity(2)   // bed + lead + drums; peak stabs still muted
```

| Level | Typical stems |
|-------|----------------|
| 0 | bass + pad (always-on bed) |
| 1 | + lead / arp heat |
| 2 | + hats + kick |
| 3 | + peak stabs / sirens |

`deckSetIntensity` keeps time locked — raising intensity lets the next note-ons of higher stems through; lowering cuts active voices on stems that drop offline. `deckPlay` resets intensity to 0.

Stay inside the channel caps (2 pulse / 1 wave / 1 noise / 2 PCM) — layers are mute gates, not extra hardware voices. Recipe: `examples/deck-demo/assets/siege.deck`, `chase.deck`. Demo: select an **INT** track, L/R changes intensity.

### `gameBoyDmg` params

`type` pulse|wave|noise · `duty` 12_5|25|50|75 · `env_mode` step|constant|adsr · `vol` 0–15 ·
`noise_mode` long|short · `wave_shape` saw|square|sine · `attack`/`decay`/`sustain`/`release` ·
`vib_rate`/`vib_amt` · `arp_rate`/`arp_semis` · `pitch_drop`/`pitch_dec`

**Hardware surface (LR35902 registers):**

| Key | Range | Effect |
|-----|-------|--------|
| `len` | 0–63 (wave 0–255) | Length counter; 0 = sustain |
| `env_step` | 0–7 | Envelope period (step mode); 0 = auto from `vol` |
| `env_up` | true\|false | Envelope amplifies toward 15 instead of decaying |
| `sweep` | −7…7 | Soft semis; on pulse **ch1** auto-maps to NR10 when HW unset |
| `sweep_shift` | 0–7 | NR10 step size (ch1 only; 0 = off) |
| `sweep_period` | 0–7 | NR10 time between steps (0 = off) |
| `sweep_down` | true\|false | NR10 direction (also implied by negative `sweep`) |
| `noise_shift` | 0–13 | Noise clock shift (else from MIDI) |
| `noise_ratio` | 0–7 | Noise fine divisor |

Example laser blip on the lead (first pulse → HW ch1):

```text
  gen type pulse duty 50 env_mode step vol 12 sweep -5
```

Or explicit NR10:

```text
  gen type pulse duty 50 vol 12 sweep_shift 5 sweep_period 3 sweep_down true
```

### `gbaDirectSound` params

`waveform` pulse|sawtooth|triangle|sine · `duty` · `bitcrush` true|false · `vol` · ADSR · vib/arp/drop

These synthesise a 32-byte single-cycle wavetable and loop it — a synth that happens to run
through the mixer. To play a **recorded instrument** instead, give the track a `program` from a
`sampleset` (see GBA extensions); the two are different voice kinds and do not share params.

## Not supported on GBA

Two different behaviours, previously lumped together as "ignored":

**Hard error at bake** — these change how a song sounds, so silently dropping them would ship the
wrong music:

`swing` · `scale` · `auto` · `fx` · `clip` · `session_scenes` / `session_slot` · `master_mix` ·
`actor_mix` · `remove_track` · `macro` · `transpose` · `gen_block` · `@…` directives ·
`steps euclid` · any `gen` other than the two above · `deck` lane routing

**Silently dropped** — no hardware meaning, and harmless to leave in a file shared with a host:

`loops` (host playback cap) · `voice` (octave / arp / chord / strum) ·
`step_vel` / `step_prob` / `step_ratchet` / `step_nudge` (no per-step lock lanes)

## Full host grammar

`.deck` is one language with one parser. This bake uses
[`deckfile`](https://crates.io/crates/deckfile), generated from the same source as the
[`@spacedevin/deck`](https://www.npmjs.com/package/@spacedevin/deck) package the Deckard host uses —
so a file means the same thing here as it does there. This crate only adds the LOWERING to LR35902 /
GBA PCM.

Canonical grammar: [`@spacedevin/deck/grammar`](https://github.com/spacedevin/deck/blob/main/docs/DECK_GRAMMAR.md).
Only the subset above bakes for GBA; that subset is recorded as the `gba` profile in the shared
[conformance corpus](https://github.com/spacedevin/deck/tree/main/conformance), which is what keeps
this doc and the bake honest about each other.

### GBA extensions

Two additions, registered as extensions of the shared grammar rather than a fork of it:

| Extension | Form |
|-----------|------|
| Named PSG wavetable | `wave <name> <32 hex nibbles>` (top level) |
| Intensifier stem | `layer` / `intensity` / `min_intensity` `<0..3>` (track header or body) |
| Sampled instrument bank | `sampleset <path-to-vgNNN.json>` (top level) |
| Sampled instrument voice | `gen program <n>` on a `gbaDirectSound` track |

**Sampled instruments.** `sampleset` names a bank of real recordings — key zones, per-sample rate,
root key, loop point and envelope. A `gbaDirectSound` track then selects one with `gen program
<n>`, and one mixer channel covers the whole instrument however many key zones it has:

```
sampleset ../../data/music/vg039.json

track Lead id lead gen gbaDirectSound
  gen program 6 vol 12
  note 60 0 2 v 100
```

The path is resolved relative to the `.deck`. Everything about how the instrument sounds — tuning,
looping, envelope — lives in the bank, so the deck stays a score rather than a second copy of the
instrument definition. Pitch comes from each zone's root key, so **a note's absolute pitch is the
bank's business, not MIDI's**: an instrument may declare a tuning that puts middle C two octaves
up, and honouring that is the point. A sampled-music example and a voicegroup-extract
script exist as worked cases for building a bank in M4A voicegroup shape.

## Anti-patterns

- Third pulse track — use wave/noise/PCM instead
- `gen fm` / `kick_edm` / `patch` — bake will error
- Bass on pulse below C2 — use wave
- Expecting `chip_play` and `deck_play` to layer on PSG — they replace each other
- Calling `psg_*` to “compose” — write a `.deck`

## Diagnosing hitching (deck-demo)

`examples/deck-demo` tags tracks by engine:

| Tag | Engines | Notes |
|-----|---------|-------|
| `INT` | layered dual | L/R = `deckSetIntensity` 0..3 |
| `PSG` | `gameBoyDmg` only | Sequencer still needs `audio_pump` catch-up during long UI |
| `PCM` | `gbaDirectSound` only | Mixer + sequencer |
| `Sampled` | `gbaDirectSound` + `gen program` | Mixer + sequencer + ROM sample reads |
| `DUAL` | both | Both halves (incl. long `caravan.deck`) |

Long UI work (list scroll, `uiRender`) must call `audio_pump()` — it now advances the deck/chiptune
sequencer on wall-clock time **and** fills DirectSound buffers. Windowed list scroll patches row text
in place (no per-row measure) when cell shapes match.

## Verify

`examples/deck-demo/verify.sh` captures emulated audio (`GBA_SHOT_AUDIO`) and FFT-checks pitches,
same approach as `examples/chiptune/verify.sh`. Boots the dual-engine overworld.
