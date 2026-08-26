# CHIPTUNE

> *Demonstrates audio playback for chiptunes/music tracks.*

<img src="preview.gif" alt="preview" width="480">

The GBA has two independent ways to make sound. agb only exposes one of them: the **software mixer**,
which plays recorded samples through the Direct Sound FIFOs. This example uses the other one — the
**PSG**, four hardware channels (two square waves, a 32-step wavetable, and noise) that oscillate on
their own and are driven entirely by register writes.

The difference is the difference between storing a recording and storing sheet music.

| | akari, sampled | akari, synthesised |
|---|---|---|
| ROM | 4.47 MB | 3.38 MB |
| Two themes | 1.09 MB of audio | ~1.5 KB of note data |
| Per-frame CPU | software mixing, every frame | none — the hardware oscillates |
| Can it stutter? | yes, if not fed | no |

That last row is worth as much as the megabyte. `pump_audio` calls are sprinkled through the long
synchronous operations in `crates/tish-agb/src/lib.rs` — menu layout, dialogue shaping, scene
streaming — purely to stop the mixer underrunning while one of them runs. Nothing on the PSG path
needs that: a slow frame delays the next note, it cannot corrupt the sound.

## What's here

```
npm run build     # chiptune.gba (the track browser), tones.gba, song.gba
npm start         # play it
npm run verify    # assert what actually came out of the emulator
```

The four rows under the track list are the **PSG voices, live** — `SQ1`/`SQ2` square, `WAV`
wavetable, `NSE` noise — each showing the note it is sounding and a bar that scales with pitch. You
can watch the melody move between channels while the song plays. That needed four new calls in
`tish-agb` (`chip_note`, `chip_row`, `chip_rows`, `chip_borrowed`): the sequencer always knew which
note each channel was on, it simply had no way to say so, which meant a game could play a song and
have no way to react to it. `chip_note` follows `HOLD` rows back to the note actually ringing, so it
reports what you *hear* rather than what is written on the current row.

**UP/DOWN** pick a track · **A** play · **B** stop · **START** toggles attract mode (it walks the
list on its own, so the ROM demos itself unattended). `>` is the cursor, `*` is what is sounding —
they differ, because you can browse while a track keeps playing.

The three tracks and their real `.chip` sizes are on screen next to the comparison, because
"6.7 KB instead of 1.09 MB" is the entire argument and a number nobody sees persuades nobody. It
used to play two songs over a flat colour with the case made only here in the README, where a ROM
cannot make it.

- **`src/main.tish`** plays akari's two themes off the same `.chip` sources the game uses.
- **`src/tones.tish`** is the driver's calibration ROM: known pitches, one per channel.
- **`src/song.tish`** is the `chip:` pipeline's calibration ROM: a C major scale.

## Verifying audio without listening to it

`tools/gba-shot` grew a `GBA_SHOT_AUDIO=out.wav` mode that captures the emulated sound, so audio is
testable the same way the framebuffer is. `verify.sh` uses it to FFT every window and check the
pitch that actually came out:

```
WINDOW             EXPECTED   MEASURED     ERROR   RESULT
ch1 square A4         440.0      440.0       -0c   PASS
ch1 square A5         880.0      880.0       -0c   PASS
ch2 square E4         329.6      330.8       +6c   PASS
ch3 wave   A4         440.0      440.0       -0c   PASS
ch4 noise             noise      noise flat 0.865   PASS
silence              silent     silent             PASS
```

This is not ceremony. Both real bugs found while building the driver were inaudible as bugs:

- The wavetable channel played a **different bank than the one being written**. On the GBA, bit 6 of
  `SOUND3CNT_L` picks the bank that *plays*, and the CPU sees the other one. Getting it backwards
  produces sound, and the registers read back exactly as written — the channel was simply playing
  uninitialised memory. It measured 2341 Hz where 440 was asked for.
- The song format's comment stripper ate the `#` in `G#4`, silently turning a sharp into a truncated
  line. That one surfaced as a build error only because the parser validates note names.

A note table a semitone out, an octave clamp on the bass, or a sequencer a row behind all sound
perfectly plausible. None of them survive a measurement.

## The song format

`.chip` files are a small tracker text, compiled to static ROM data at build time by
`crates/tish-gba-scenepack/src/chippack.rs`. Nothing is parsed on device.

```
tempo 9                                   # frames per row
loop 0                                    # row to return to

wave tri 0123456789ABCDEFFEDCBA9876543210 # 32 steps, for the wavetable channel

inst lead square duty=2 vol=11 decay=0
inst bass wave   table=tri vol=1

ch1 lead | E4 .  G4 .  C5 .  G4 .  |      # one bar per line
ch2 harm | C4 .  E4 .  G4 .  E4 .  |
ch3 bass | C2 .  .  .  G2 .  .  .  |
ch4 drum | x  .  .  .  x  .  .  .  |
```

`.` holds the previous note, `-` cuts it, `x` triggers the noise channel, `|` is decoration. One
instrument per channel — the constraint that keeps a row to a single byte, and how most Game Boy
music is written anyway.

The build rejects ragged channels. A bar missing from the bass drifts the arrangement apart
progressively, and by ear that reads as "this song is bad" rather than "row 47 is missing".

## Limits worth knowing

- **Four voices, and one instrument each.** No per-note instrument changes, so a kick and a hi-hat
  can't share the noise channel.
- **The squares can't play below C2** (~64 Hz) — the frequency register can't represent it. The
  wavetable channel reaches an octave lower, which is why bass lines go there.
- **Sound effects preempt a channel.** `chipsfx.tish` borrows channel 2 for tonal effects and
  channel 4 for percussive ones, for a fixed number of frames, and the music reclaims them
  automatically. Channel 1 keeps the melody, so effects never punch a hole in the tune.
- **You cannot convert a recording to this.** The `.chip` files are compositions in the mood of the
  originals. Turning a rendered mix back into notes is transcription, and for anything polyphonic it
  produces a different piece.
