# Audio on the GBA: samples vs. synth

The GBA makes sound two independent ways, and this repo now uses both.

**Direct Sound** — two DMA-fed FIFOs playing PCM samples, mixed in software. This is all agb 0.25
exposes (`agb::sound::mixer`); it is what `sound_play` and `music_play` use. It plays anything, and
it costs ROM proportional to the length of the audio plus a slice of every frame to mix.

**The PSG** — four hardware channels (two square waves with adjustable duty, a 32-step wavetable,
and a noise generator) that oscillate by themselves. agb has no support for these at all. They are
driven here by `crates/tish-agb/src/psg.rs`, writing `NR10`–`NR44`, wave RAM and `SOUNDCNT_L`
directly. A note costs a register write; sustaining it costs nothing.

## Why akari moved to the synth

akari's two themes were 51.7 s and 60 s of recorded music from the Ninja Adventure pack. agb bakes
WAVs into the ROM as 8-bit at the mixer rate, so they cost **1.09 MB** — the ROM went from 4.47 MB
to 3.38 MB when they were replaced. The same music as note data is about **1.5 KB**.

The saving that isn't bytes matters as much. The software mixer has to be fed: `pump_audio` is
called from the long synchronous operations in `crates/tish-agb/src/lib.rs` — text shaping, rect
fill, tile streaming, menu layout — purely because a build that doesn't return to `frame()` for
hundreds of milliseconds drains the mixer's ~50 ms of buffer and the BGM stutters audibly. That was
the acute "music breaks up while the pause menu loads" symptom. PSG music cannot underrun, so on the
synth path none of that machinery applies; a slow frame delays the next note and nothing else.

## What can and cannot be converted

**Sound effects convert well** — coins, blips, menu ticks and impacts are what these channels were
designed for, and the hardware envelope and pitch sweep mean an effect is one call with no per-frame
work. `packages/chipsfx.tish` is the library.

**deck songs** — author with **LR35902** (`gameBoyDmg`) and/or **GBA PCM**
(`gbaDirectSound`), drop the `.deck` into the game, import with `deck:`, play with `deckPlay` from
`packages/deck.tish`. Soft ADSR, vibrato, arp, and pitch-drop are stepped per frame. Full agent guide:
[`docs/deck.md`](deck.md). This is separate from `.chip` (simple tracker rows); both can coexist, but
only one PSG BGM owner at a time (`chip_play` XOR `deck_play`).

**Recorded music does not convert at all.** Going from a rendered mix back to note data is
polyphonic transcription; tools exist and will give you a melody line worth starting from, but the
result is a re-arrangement, not a conversion. akari's `.chip` themes are new compositions in the
mood of the originals. If you have the music as a tracker module or MIDI already, that is note data
and it converts directly.

**If you want sampled music but smaller**, the third option is `agb_tracker` 0.25.0, which matches
our agb version and plays XM/MOD/S3M. It still uses the mixer, so it neither sounds like the PSG nor
saves the per-frame cost, but it stores instruments plus notes rather than a recording.

**A fourth option: a deck `sampleset`** — note data plus a bank of instrument samples, which is the
same shape as a tracker module but inside the format the engine already plays. A
`gbaDirectSound` track with `gen program <n>` plays a real recording with its own root key, loop
point and key zones (`docs/deck.md` → GBA extensions). A sampled-music SRPG example is the worked
case: a full sampled instrument bank in M4A voicegroup shape.

⚠️ **This re-incurs the whole `pump_audio` tax described above**, and deliberately. Sampled BGM can
underrun; PSG BGM cannot. A game that takes this path inherits the "music breaks up while the pause
menu loads" failure mode and every `pump_audio` call site that exists to prevent it. That is the
trade: the PSG path escaped a real cost, and this walks back into it in exchange for sounding like
the source material. Pick per game, not per engine — both paths ship, and the PSG and sampled-bank music examples are the same eight songs either way.

Two sizing rules that are not obvious:

* **A sample's storage rate should be set by the lowest key in its zone**, not by the recording.
  agb's mixer is point-sampling assembly with no interpolation, so anything stored faster than the
  lowest key needs is decimated on the way out — ROM spent on samples nobody hears, plus aliasing.
  Doing this halved one shared sample bank (438KB → 228KB) *and* improved it.
* **Bake only the programs a song actually plays**, and for key-split instruments only the zones
  its notes actually strike. The source bank's raw sample region is ~1.9MB; eight songs conditioned this way
  share a 228KB bank.

## ⚠️ Where "static" actually comes from

Five real sources, all found by measurement and none visible in a build, a note count, or a
peak/rms check. Full write-up: the sampled-music example's README.

1. **Sample signedness.** agb's mixer loads with `ldrsb` — samples are two's-complement −128..127,
   never offset-binary 0..255. `deckpack` emitted `v * 127 + 128`, which made a synthesised 50%
   pulse **±1 of 127** (42dB down, effectively silent) and a sine **226% THD**. Pinned now by
   `pcm_table_is_signed_8bit` in the deckpack tests.
2. **Note-off with no release ramp.** Stopping a sustained sample mid-waveform is a hard step to
   zero — a click on *every* note, heard as crackle under the music rather than as clicks.
3. **Resampling with no anti-alias filter.** Linear interpolation at 3.2x decimation passes a 20kHz
   tone at −5.7dB straight into the audible band. Use a windowed sinc with its cutoff scaled by the
   decimation factor. ⚠️ A box average is *not* the fix: it trades alias noise for passband droop
   and measures worse than doing nothing.
4. **Loop-seam discontinuity** — compare `|pcm[-1] - pcm[loop_start]|` to the mean step inside the
   loop; 2–3x is normal, an order of magnitude is a periodic click.
5. **DC offset** — a sample with an offset clicks when it starts and stops.

⚠️ Two measurement traps. Comparing a resampler to an FFT-resampled "ideal" is dominated by edge
ringing and phase shift — test aliasing directly by feeding a tone above the new Nyquist and
asserting it vanishes. And a "step > N× the median step" click metric is scale-relative: it reports
*more* clicks when the mix simply gets quieter.

## Hardware notes worth not rediscovering

- **agb owns `SOUNDCNT_H`.** Its blanket write when Direct Sound is enabled zeroes bits 0-1, leaving
  the PSG mixed in at 25%. `psg::init` read-modify-writes them back and must therefore run *after*
  the mixer is constructed. It never touches `NR10`–`NR44`, wave RAM or `SOUNDCNT_L`, so the two
  paths otherwise coexist with no fork.
- **`SOUNDCNT_L` powers up routing nothing to either speaker.** Without setting it, every note plays
  into silence while the channel runs and the registers read back correctly.
- **Wave RAM has two banks and you write the one that isn't playing.** Bit 6 of `SOUND3CNT_L` selects
  the bank being *played*; the CPU sees the other at `0x4000090`. Getting this backwards plays
  uninitialised memory, which sounds like a tone at the wrong pitch, not like a bug.
- **The square channels cannot go below C2** (~64 Hz) — the 11-bit frequency register can't represent
  it. The wavetable channel's period is half the squares' for the same pitch, so it reaches C1, which
  is where bass lines belong.

## Verifying audio without listening

`tools/gba-shot` takes `GBA_SHOT_AUDIO=out.wav` and captures the emulated sound (mGBA's two blip
buffers, drained per frame) to a 16-bit stereo WAV, plus a `peak`/`rms` line that makes "is anything
playing at all" a one-line check. `examples/chiptune/verify.sh` FFTs each window and asserts the
pitch.

Both bugs found while writing the driver were inaudible as bugs — the wave bank error produced a
confident 2341 Hz where 440 was asked for, and a note table that is a semitone out or a bass line
clamped an octave up sounds like a mediocre song rather than a defect. Measure it.
