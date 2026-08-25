//! The chiptune sequencer: plays a `chip:`-imported song on the four PSG channels.
//!
//! A song here is *note data*, not audio. `examples/akari`'s village theme is 51 seconds of music
//! that used to be a 1.06 MB WAV; as rows it is a couple of kilobytes, because a row is one byte per
//! channel and the instruments are six bytes each. That ratio — three orders of magnitude — is the
//! entire reason this module exists.
//!
//! **Cost.** [`step`] runs once per frame and does, per channel, one array index and a branch; it
//! only touches hardware on rows that actually start a note. There is no mixing, no buffer, and
//! nothing that can underrun — contrast `pump_audio`, which the software mixer needs sprinkled
//! through every long synchronous operation in `lib.rs` to stop the music stuttering while a menu
//! builds. A stalled frame here delays the next note; it cannot corrupt the sound.

use crate::psg;

/// How a channel's notes are voiced. One instrument per channel per song — the constraint that keeps
/// a row down to a single byte, and the way most Game Boy music is written anyway.
pub struct Instrument {
    /// 0 = square (channels 1-2), 1 = wavetable (channel 3), 2 = noise (channel 4).
    pub kind: u8,
    /// Pulse width for square voices, 0-3 (12.5 / 25 / 50 / 75%).
    pub duty: u8,
    /// Envelope start volume 0-15 for square/noise; the 4-step divider (1 = full) for the wave voice.
    pub vol: u8,
    /// Envelope decay 0-7; 0 sustains for the note's whole length.
    pub decay: u8,
    /// Hardware length counter, 0-63; 0 sustains until the next note or an explicit note-off.
    pub len: u8,
    /// Index into the song's wavetables, for `kind == 1`.
    pub wave: u8,
    /// Noise pitch (`shift`, 0-13) for `kind == 2`. Ignored otherwise.
    pub shift: u8,
}

/// One channel of a song: its voice, plus one note byte per row.
///
/// Row bytes are `HOLD` (let the previous note ring), `OFF` (silence), or a MIDI note number.
pub struct Track {
    pub inst: Instrument,
    pub notes: &'static [u8],
}

/// Sustain the previous note through this row.
pub const HOLD: u8 = 0;
/// Cut the channel on this row.
pub const OFF: u8 = 1;

/// A compiled song. Built by `include_chip!` at build time; never parsed on device.
pub struct Song {
    /// Frames per row — the tempo. 8 gives ~7.5 rows/second.
    pub frames_per_row: u8,
    /// Row to jump back to when the song ends, making the loop seamless.
    pub loop_row: u16,
    /// Total rows. Every track is this long; the build-time parser rejects ragged ones, because a
    /// channel one bar short drifts progressively out of time and is miserable to diagnose by ear.
    pub rows: u16,
    /// Channels 1-4, in hardware order.
    pub tracks: [Track; 4],
    /// Wavetables the wave voice can select, 32 4-bit samples each, packed two per byte.
    pub waves: &'static [[u8; 16]],
}

/// Playback position. One of these exists; songs replace each other rather than layering, the same
/// way `music_play` replaces the previous BGM.
pub struct Player {
    song: Option<&'static Song>,
    row: u16,
    /// Frames left on the current row.
    countdown: u8,
    /// Which wavetable is currently loaded, so an unchanged one isn't re-uploaded (reloading wave
    /// RAM mid-note clicks).
    loaded_wave: i16,
    /// Frames left on each channel's loan to a sound effect. While non-zero the sequencer leaves
    /// that channel alone, which is how a real Game Boy driver does it: the effect preempts one
    /// voice and the music carries on with the other three, rather than the whole track ducking.
    borrowed: [u8; 4],
}

impl Player {
    pub const fn new() -> Self {
        Player {
            song: None,
            row: 0,
            countdown: 0,
            loaded_wave: -1,
            borrowed: [0; 4],
        }
    }

    /// Start a song from the top. Silences whatever was playing so a held note from the previous
    /// area's theme doesn't ring under the new one.
    pub fn play(&mut self, song: &'static Song) {
        psg::stop_all();
        self.song = Some(song);
        self.row = 0;
        self.countdown = 0;
        self.loaded_wave = -1;
        self.borrowed = [0; 4];
    }

    pub fn stop(&mut self) {
        self.song = None;
        psg::stop_all();
    }

    pub fn playing(&self) -> bool {
        self.song.is_some()
    }

    /// The note a channel is sounding right now, as a MIDI number — or 0 for silence.
    ///
    /// The sequencer already knows this; it simply never said so, which meant a game could play a
    /// song and have no way to react to it, and a demo could not show what the format was doing.
    /// `HOLD` rows are walked backwards to the note actually ringing, because "sustain the previous
    /// note" is invisible to anything reading a single row.
    pub fn channel_note(&self, ch: usize) -> i32 {
        let Some(song) = self.song else { return 0 };
        if ch >= 4 {
            return 0;
        }
        let notes = song.tracks[ch].notes;
        let mut r = self.row as usize;
        if r >= notes.len() {
            return 0;
        }
        loop {
            match notes[r] {
                OFF => return 0,
                HOLD => {
                    if r == 0 {
                        return 0;
                    }
                    r -= 1;
                }
                n => return n as i32,
            }
        }
    }

    /// Which row the sequencer is on, and how long the song is — a playhead, for anything that
    /// wants to sync to the music (a rhythm judge, a progress bar, a visualiser).
    pub fn row(&self) -> i32 {
        self.row as i32
    }

    pub fn rows(&self) -> i32 {
        match self.song {
            Some(s) => s.rows as i32,
            None => 0,
        }
    }

    /// Is this channel currently lent to a sound effect? While it is, the music is not driving it.
    pub fn channel_borrowed(&self, ch: usize) -> bool {
        ch < 4 && self.borrowed[ch] > 0
    }

    /// Lend a channel to a sound effect for `frames`, after which the music reclaims it
    /// automatically. Without the loan the next row simply retriggers the channel and cuts the
    /// effect off part-way — at a typical tempo that is within a tenth of a second, so nearly every
    /// effect would be clipped.
    ///
    /// The loan expiring is what returns the channel, so nothing in game code has to remember to.
    pub fn borrow_channel(&mut self, ch: u8, frames: u8) {
        if (1..=4).contains(&ch) {
            self.borrowed[(ch - 1) as usize] = frames.max(1);
        }
    }

    /// Advance one frame. Cheap enough to call unconditionally: with no song it is a single branch.
    pub fn step(&mut self) {
        // Loans tick down every frame, not every row, so an effect's length is in real time and does
        // not stretch with the tempo.
        for left in self.borrowed.iter_mut() {
            *left = left.saturating_sub(1);
        }
        let Some(song) = self.song else { return };
        if self.countdown > 0 {
            self.countdown -= 1;
            return;
        }
        self.countdown = song.frames_per_row.saturating_sub(1);

        for (i, track) in song.tracks.iter().enumerate() {
            let ch = (i + 1) as u8;
            if self.borrowed[i] > 0 {
                continue;
            }
            let Some(&note) = track.notes.get(self.row as usize) else {
                continue;
            };
            match note {
                HOLD => {}
                OFF => psg::stop(ch),
                n => self.trigger(song, track, ch, n),
            }
        }

        self.row += 1;
        if self.row >= song.rows {
            self.row = song.loop_row.min(song.rows.saturating_sub(1));
        }
    }

    fn trigger(&mut self, song: &'static Song, track: &Track, ch: u8, note: u8) {
        let inst = &track.inst;
        match inst.kind {
            1 => {
                // Upload the wavetable only when it changes — see `loaded_wave`.
                if self.loaded_wave != inst.wave as i16 {
                    if let Some(table) = song.waves.get(inst.wave as usize) {
                        psg::wave_table(table);
                        self.loaded_wave = inst.wave as i16;
                    }
                }
                psg::wave(note as i32, inst.vol, inst.len);
            }
            2 => psg::noise(inst.vol, inst.decay, inst.len, inst.shift, 0, false, false),
            _ => psg::square(
                ch,
                note as i32,
                inst.duty,
                inst.vol,
                inst.decay,
                inst.len,
                false,
            ),
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}
