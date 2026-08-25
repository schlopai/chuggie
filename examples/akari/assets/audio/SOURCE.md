# Akari audio

Akari's audio is **synthesised**, not sampled. The two themes are `chip:` songs played on the GBA's
four PSG hardware channels (`town.chip`, `dungeon.chip`); the sound effects are hardware voices from
`packages/chipsfx.tish`. There are no audio assets in this directory beyond the two song texts.

## Why

The music used to be `town.wav` (51.7 s) and `dungeon.wav` (60 s), downmixed to mono 10512 Hz from
the **Ninja Adventure — Asset Pack** (Pixel-boy & AAA, CC0), plus seven sampled effects. agb bakes
WAVs into the ROM as 8-bit at the mixer rate, so those recordings cost **1.09 MB of ROM** — akari
was 4.47 MB and is now 3.38 MB. The same music as note data is about **1.5 KB**.

The saving that isn't measured in bytes: the software mixer has to be fed. `pump_audio` is sprinkled
through the long synchronous operations in `crates/tish-agb/src/lib.rs` — menu layout, dialogue
shaping, scene streaming — purely so the BGM doesn't underrun while one of them runs. The PSG
oscillates in hardware, so akari's music no longer depends on any of that, and a slow frame can
delay the next note but cannot make the audio stutter.

## What was replaced

| Was | Now |
|-----|-----|
| `town.wav` ← `Musics/4 - Village.ogg` | `town.chip` — 16 bars, C major |
| `dungeon.wav` ← `Musics/21 - Dungeon.ogg` | `dungeon.chip` — 16 bars, A minor |
| `slash.wav` ← `Sounds/Whoosh & Slash/Slash.wav` | `chipSlash()` — noise burst |
| `throw.wav` ← `Sounds/Whoosh & Slash/Launch.wav` | `chipThrow()` — descending sweep |
| `whoosh.wav` ← `Sounds/Whoosh & Slash/Whoosh.wav` | `chipWhoosh()` — low noise |
| `coin.wav` ← `Sounds/Bonus/Coin.wav` | `chipCoin()` — rising sweep |
| `text_blip.wav` ← `Sounds/Menu/Move1.wav` | `chipBlip()` (via `dialogInit({ blipChip: 1 })`) |
| `hit.wav`, `accept.wav` (unused) | `chipHit()`, `chipAccept()` |

The themes are new compositions in the mood of the originals, not transcriptions: the pack's music
is recorded audio, and there is no way to recover note data from a rendered mix that is worth
listening to. The originals remain CC0 if anyone wants to compare.

## Editing the songs

The format is documented in `crates/tish-gba-scenepack/src/chippack.rs` and is meant to be edited by
hand — `tempo`, `wave`, `inst` and one `chN` line per bar. It is checked at build time: every
channel must be the same number of rows, notes must be real notes, and a wavetable must be exactly
32 steps. `examples/chiptune` plays both themes and `examples/chiptune/verify.sh` asserts the
pitches that actually come out of the emulator.
