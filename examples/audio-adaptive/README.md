# AUDIO ADAPTIVE

> *Adaptive music made testable: what a pause keeps, and what the hush it replaces throws away.*

<img src="preview.png" width="240">

## The two things games kept getting wrong

### 1. Hushing the music is `deck_pause(1)`, not stop-then-play

The discipline this replaces is real and was written for a real crash: bracket anything that
rebuilds the UI canvas with `deck_stop()` / `deck_play()`, because a wavetable write landing at an
arbitrary point inside a frame froze a game on a cave entrance and cost eleven wrong theories to
diagnose. Two different games wrote that hush by hand and disagreed about it.

But the pair throws away two things nobody meant to: the **playhead**, so the music restarts at the
top of the song, and the **intensity**, which `play()` resets to 0 — so a game that hushed for a
menu mid-boss-fight came back playing the calm arrangement.

This ROM does both, back to back, at the same non-zero intensity:

```
pause:  237/3 -> 237/3 -> (120 frames) -> 237/3   (playhead/intensity)
hush:   477/3 -> 0/0
```

`deck_pause` freezes the playhead and keeps the song, the voices and the intensity. Unpausing
re-arms the wavetable, which is the part that actually mattered.

### 2. Ducking is a master attenuation, not per-voice scaling

`audio_duck` moves the PSG master volume, so every voice comes down together. Scaled per voice, a
note that started before the duck stays loud until it ends and the music ducks a beat late and
unevenly. The attack/release step sizes are divided **once** at call time, never per frame — this
chip has no divide instruction.

```
ok   the duck reaches depth (min gain 12 of 64)
ok   the duck RAMPS rather than jumping (81 distinct levels)
ok   the duck releases (final gain 64)
```

⚠️ A voice in step-envelope mode has its level owned by the *hardware* envelope, so it takes the
duck on its next attack rather than mid-note — roughly nine frames at 152 BPM sixteenths. That is
what the hardware permits, not a workaround, and it is why a stem you want to fade smoothly should
be authored ADSR or PCM.

## Every claim is paired with its contrast

Asserting only that `deck_pause` preserves the playhead would pass against an engine where *nothing*
advances the playhead. Asserting only that a duck reaches depth would pass against one that jumps
there in a single frame. So the verifier also asserts that stop-then-play **loses** what the pause
keeps, that the playhead **does** advance when not paused, and that the duck produces many distinct
levels rather than two.

If the two "stop-then-play loses it" assertions ever start passing as *preserved*, the pause
assertions have stopped proving anything and this file needs rewriting, not relaxing.

## Music

`kart-circuit`'s `race.deck`, imported rather than regenerated — it already carries all four
intensity stems (bass 0, lead 1, kick+hats 2, stabs 3), and a second copy would only let them drift.

```bash
npm run verify
```
