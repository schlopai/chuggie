#!/usr/bin/env python3
"""Song + chart for the RAP DOJO example. Run from the repo root:

    python3 scripts/gen_rap_dojo_music.py

Emits TWO files from ONE table:
  examples/rap-dojo/assets/battle.deck  — the music
  examples/rap-dojo/src/chart.tish      — the cues the game judges

They are generated together on purpose. The teacher's call is a melody in the song AND a row of
button prompts in the chart, and if those two ever disagree the game is unplayable in a way that
looks like a timing bug: the player hears a phrase, copies what they heard, and is told they are
wrong. Writing the notes by hand in one file and the cues by hand in the other is exactly the setup
where that happens, so PHRASES below is the only place either is written down.

STRUCTURE
  bars 0-1    intro groove — the player hears the beat before anything is asked of them
  bars 2-17   eight call-and-response phrases: one bar the teacher raps, one bar the pupil answers
  bars 18-19  outro groove
Twenty bars at 96 BPM is about 50 seconds.

The lead voice plays ONLY during call bars. The silence under the response bar is the point: the
drums and bass keep the beat, and the hole where the melody was is what the player is filling.

⚠️ Two deckpack rules this file depends on, both silent when broken (docs/deck.md):
  * `gen` parameters are only read from INDENTED body lines. On a `track` header they parse fine
    and are discarded, so the voice quietly falls back to a default 50%-duty pulse.
  * Every track needs the SAME `* N`. Mixing `* 4` and `* 20` yields a 20-bar song in which the
    `* 4` track stops after bar 4 — no error, it just goes missing two thirds of the way through.
"""
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EX = os.path.join(ROOT, "examples", "rap-dojo")
os.makedirs(os.path.join(EX, "assets"), exist_ok=True)
os.makedirs(os.path.join(EX, "src"), exist_ok=True)

BPM = 96
BARS = 20
INTRO_BARS = 2

# Button codes from tish-agb's `button_of`.
A, B, UP, DOWN, LEFT, RIGHT = 0, 1, 6, 7, 8, 9

# Each button is a degree of C minor pentatonic, low to high, so the d-pad reads as a scale under
# the fingers: Down is the bottom of the phrase and Up is the top. Any pattern of buttons is
# therefore a playable lick, which is what lets the phrases below be written for FEEL rather than
# checked for harmony afterwards.
PITCH = {DOWN: 67, LEFT: 70, A: 72, B: 75, RIGHT: 77, UP: 82}
NAME = {A: "A", B: "B", UP: "UP", DOWN: "DOWN", LEFT: "LEFT", RIGHT: "RIGHT"}

# (offsets in 16ths within the call bar, button per offset). Difficulty climbs from plain quarters to
# six notes with eighths in them.
#
# ⚠️ THE TIGHTEST GAP IS AN EIGHTH — two 16ths — and that is a display constraint, not a taste one.
# A prompt icon is 16px wide and the lane runs at BEAT_PX pixels per beat, so a gap of one 16th puts
# two icons 11px apart at BEAT_PX 44: they overlap, and a row of half-covered arrows is unreadable in
# the second it is on screen. The first version ended on a bar of eight straight 16ths and that bar
# was a smear. Six notes a bar is also simply enough to ask someone to memorise by ear.
PHRASES = [
    ([0, 4, 8, 12],             [A, A, B, A]),
    ([0, 4, 8, 12],             [LEFT, RIGHT, LEFT, A]),
    ([0, 4, 8, 12],             [A, B, UP, A]),
    ([0, 4, 6, 8, 12],          [LEFT, LEFT, RIGHT, A, B]),
    ([0, 4, 8, 10, 12],         [UP, DOWN, LEFT, RIGHT, A]),
    ([0, 2, 4, 8, 12],          [A, A, B, LEFT, RIGHT]),
    ([0, 4, 6, 8, 12, 14],      [A, UP, B, DOWN, LEFT, RIGHT]),
    ([0, 2, 4, 6, 8, 12],       [A, B, A, B, LEFT, UP]),
]

# Phrase i's call starts here, in 16ths of a beat from the top of the song.
def phrase_start16(i):
    return (INTRO_BARS + i * 2) * 16      # 2 bars per phrase, 16 steps per bar


CALL_LEN16 = 16   # the call is one bar: 16 steps

# THE UNIT. Everything in this file counts STEPS, and a step is a SIXTEENTH NOTE — a quarter of a
# beat, four to the beat, sixteen to a 4/4 bar.
#
# ⚠️ This one constant is why the file has a single conversion function now. It used to convert twice:
# the song divided steps by 4 to get beats, and the cue times divided by 16, on the reading that a
# "beat16" was a sixteenth of a beat. Nothing errored and nothing looked wrong in isolation — the
# chart just ran four times faster than the music, so the prompts bore no relation to what the player
# heard. `packages/rhythm.tish`'s `frameOfStep` must agree with `step_to_beat` below.
STEPS_PER_BEAT = 4

# Must match `beat_to_frames` in crates/tish-gba-scenepack/src/deckpack.rs: 59.7275Hz, not 60, and
# each position rounded on its own.
FPS = 597275 / 10000


def step_to_beat(step):
    return step / STEPS_PER_BEAT


def step_to_frame(step, bpm=BPM):
    return round(step_to_beat(step) * 60.0 * FPS / bpm)


# gba-shot's key names, for verify.sh's input schedule.
KEYNAME = {A: "a", B: "b", UP: "up", DOWN: "down", LEFT: "left", RIGHT: "right"}


def call_cues():
    """(sequencer frame, MIDI pitch) for every cue the MASTER plays.

    verify.sh cross-checks these against the notes actually written into battle.deck. That is the
    one assertion that ties the chart to the music: everything else in the suite — the miss count,
    the scored run, even the audio check — is satisfied by a chart and a song that share a table and
    disagree about what its numbers mean, which is precisely the bug that shipped once.
    """
    out = []
    for i, (offs, btns) in enumerate(PHRASES):
        s16 = phrase_start16(i)
        for off, btn in zip(offs, btns):
            out.append((step_to_frame(s16 + off), PITCH[btn]))
    return sorted(out)


def response_cues():
    """(sequencer frame, gba-shot key name) for every cue the player must hit.

    verify.sh drives the ROM from this, so the test presses buttons derived from the same table the
    song and the chart come from. A test with its own copy of the timing would keep passing after
    the chart moved underneath it.
    """
    out = []
    for i, (offs, btns) in enumerate(PHRASES):
        s16 = phrase_start16(i) + CALL_LEN16
        for off, btn in zip(offs, btns):
            out.append((step_to_frame(s16 + off), KEYNAME[btn]))
    return sorted(out)


def deck_file():
    L = []
    L.append("# RAP DOJO — the master's lesson. Call one bar, answer the next.")
    L.append("#")
    L.append("# GENERATED by scripts/gen_rap_dojo_music.py — edit PHRASES there, not this file.")
    L.append("# The Lead track and examples/rap-dojo/src/chart.tish are two views of the same table.")
    L.append("deck 1")
    L.append(f"bpm {BPM}")
    L.append("")

    # ── Lead (pulse ch1): the master's voice. Only sounds on call bars. ──
    L.append(f"track Lead id lead gen gameBoyDmg * {BARS}")
    L.append("  gen type pulse duty 50 env_mode adsr vol 13")
    L.append("  gen attack 0 decay 0.06 sustain 9 release 0.05")
    for i, (offs, btns) in enumerate(PHRASES):
        s16 = phrase_start16(i)
        L.append(f"  # phrase {i + 1}: " + " ".join(NAME[b] for b in btns))
        for off, btn in zip(offs, btns):
            beat = step_to_beat(s16 + off)
            L.append(f"  note {PITCH[btn]} {beat:g} 0.22 v 112")
    L.append("")

    # ── Harmony (pulse ch2): a two-chord bed. Also the channel chipsfx borrows for the pupil's
    #    answering blips, which is why it is the pad and not the lead. ──
    L.append(f"track Harm id harm gen gameBoyDmg * {BARS}")
    L.append("  gen type pulse duty 25 env_mode constant vol 7")
    L.append("  note 48 0 2 v 85")
    L.append("  note 51 2 2 v 85")
    L.append("")

    # ── NO WAVE CHANNEL, and it costs us the sub-bass. ──
    #
    # ⚠️ The wave voice tears the Mode 7 floor. Measured: with a `type wave` track, one stray
    # scanline turns black or red at a random height roughly every five frames during play; remove
    # the track and it is exactly zero, over and over. `deck_player.rs` already documents the shape
    # of this — it moved the wavetable load out of `step` because writing the wave bank "lands at an
    # arbitrary point in the frame" — and a per-scanline affine table is far more sensitive to that
    # than a tilemap is.
    #
    # So the bottom end goes on the harmony pulse instead, an octave down. A pulse cannot sound
    # below C2 (it silently plays C2), and these notes are C3/D#3, so they are safely in range.
    L.append("")

    # ── Drums (noise ch4): backbeat and hats. ──
    L.append(f"track Drums id drums gen gameBoyDmg * {BARS}")
    L.append("  gen type noise vol 9 noise_mode long")
    L.append("  gen attack 0 decay 0.05 sustain 0 release 0.02")
    for beat, pitch, vel in [
        (0, 38, 127), (0.5, 62, 55), (1, 62, 70), (1.5, 40, 105),
        (2, 62, 55), (2.5, 38, 120), (3, 62, 70), (3.5, 62, 60),
    ]:
        L.append(f"  note {pitch} {beat:g} 0.2 v {vel}")
    L.append("")

    # ⚠️ NO PCM TRACK, deliberately. `gbaDirectSound` starts agb's SOFTWARE mixer, which mixes
    # samples on the CPU every frame. That cost lands inside the frame, and on a Mode 7 frame going
    # over budget does not drop a frame — it arms the scanline DMA late and the whole picture rolls
    # like a mistuned CRT. The kick is a noise hit instead; the PSG costs nothing per frame because
    # the hardware sounds it on its own.
    L.append("")
    return "\n".join(L) + "\n"


def chart_file():
    L = []
    L.append("// chart.tish — the cues the judge scores, GENERATED by scripts/gen_rap_dojo_music.py.")
    L.append("//")
    L.append("// Do not edit: this and the Lead track of assets/battle.deck are emitted from one table")
    L.append("// (PHRASES in that script) so the melody the master raps and the buttons the pupil must")
    L.append("// press cannot drift apart. Change the phrases there and re-run it.")
    L.append("import { chartPhrase } from '../../../packages/rhythm'")
    L.append("")
    L.append(f"export const CHART_BPM = {BPM}")
    L.append(f"export const CHART_PHRASES = {len(PHRASES)}")
    L.append("")
    L.append("// Button codes, spelled out so the patterns below read as choreography.")
    L.append("const A = 0")
    L.append("const B = 1")
    L.append("const UP = 6")
    L.append("const DOWN = 7")
    L.append("const LEFT = 8")
    L.append("const RIGHT = 9")
    L.append("")
    L.append("// ⚠️ Typed `i32[]`. An untyped tish array costs 28 bytes an element against 4 — small")
    L.append("// here, but these are the shape every chart in this genre has, and the habit is the point.")
    for i, (offs, btns) in enumerate(PHRASES):
        names = ", ".join(NAME[b] for b in btns)
        L.append(f"let OFF{i}: i32[] = [{', '.join(str(o) for o in offs)}]")
        L.append(f"let BTN{i}: i32[] = [{', '.join(NAME[b] for b in btns)}]   // {names}")
    L.append("")
    L.append("/// Append every phrase of the lesson to the judge's chart. Called once, at load.")
    L.append("export function buildChart() {")
    for i in range(len(PHRASES)):
        s16 = phrase_start16(i)
        L.append(f"  chartPhrase(CHART_BPM, {s16}, {CALL_LEN16}, OFF{i}, BTN{i})")
    L.append("}")
    return "\n".join(L) + "\n"


def main():
    deck_path = os.path.join(EX, "assets", "battle.deck")
    chart_path = os.path.join(EX, "src", "chart.tish")
    with open(deck_path, "w") as f:
        f.write(deck_file())
    with open(chart_path, "w") as f:
        f.write(chart_file())
    last = phrase_start16(len(PHRASES) - 1) + 2 * 16
    print(f"wrote {deck_path}")
    print(f"wrote {chart_path}")
    print(f"{len(PHRASES)} phrases, chart ends at beat {step_to_beat(last):g} of {BARS * 4} — "
          f"{'ok' if step_to_beat(last) <= BARS * 4 else 'OVERRUNS THE SONG'}")


if __name__ == "__main__":
    main()
