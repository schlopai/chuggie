//! deck song player — baked deck (`gameBoyDmg` + `gbaDirectSound`) on GBA audio.
//!
//! Host-side `deckpack` emits a [`DeckSong`]; this module steps it once per frame: note events,
//! soft ADSR / vibrato / arp / pitch-drop, PSG register writes, and DirectSound wavetable voices.
//! Mutually exclusive with [`crate::chiptune`] on the PSG; both share channel-borrow for chipsfx.

use crate::psg;
use agb::fixnum::Num;
use agb::sound::mixer::{ChannelId, Mixer, SoundChannel, SoundData};

pub const VOICE_PULSE: u8 = 0;
pub const VOICE_WAVE: u8 = 1;
pub const VOICE_NOISE: u8 = 2;
pub const VOICE_DS_PCM: u8 = 3;
/// A real recorded instrument: sampled audio with its own rate, root key, loop point and key
/// zones — as opposed to [`VOICE_DS_PCM`]'s generated 32-byte single-cycle wavetable.
pub const VOICE_DS_SAMPLE: u8 = 4;

/// Both DirectSound kinds go through the mixer rather than the PSG registers.
#[inline]
fn is_pcm(kind: u8) -> bool {
    kind == VOICE_DS_PCM || kind == VOICE_DS_SAMPLE
}

/// The one place the mixer rate is decided.
///
/// ⚠️ `pcm_playback_rate` divides by this. When it was a literal `10512` in one file and a
/// `Frequency::Hz10512` in another, changing the mixer rate would have silently transposed every
/// sampled instrument instead of failing to compile — the class of bug `docs/gba-audio.md` warns
/// about, where the result is a mediocre-sounding song rather than an obvious defect.
pub const MIXER_FREQUENCY: agb::sound::mixer::Frequency = agb::sound::mixer::Frequency::Hz10512;
pub const MIXER_RATE_HZ: i32 = MIXER_FREQUENCY.frequency();

/// agb has 8 mixer channels in total and sound effects come from the same eight.
pub const MAX_PCM_SLOTS: usize = 6;

pub const ENV_STEP: u8 = 0;
pub const ENV_CONST: u8 = 1;
pub const ENV_ADSR: u8 = 2;

pub const FLAG_NOISE_NARROW: u8 = 1 << 0;
pub const FLAG_BITCRUSH: u8 = 1 << 1;
pub const FLAG_ENV_UP: u8 = 1 << 2;
pub const FLAG_SWEEP_DOWN: u8 = 1 << 3;
/// When set, `noise_shift` is authored; otherwise derive from MIDI.
pub const FLAG_NOISE_SHIFT_SET: u8 = 1 << 4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeckVoice {
    pub kind: u8,
    pub hw_ch: u8,
    pub duty: u8,
    pub vol: u8,
    pub env_mode: u8,
    pub a: u8,
    pub d: u8,
    pub s: u8,
    pub r: u8,
    pub vib_rate: u8,
    pub vib_amt: u8,
    pub arp_rate: u8,
    pub arp_semis: i8,
    pub drop_semis: i8,
    pub drop_dec: u8,
    pub wave: u8,
    pub flags: u8,
    /// Hardware length counter (0 = sustain). Pulse/noise: 0–63; wave: 0–255.
    pub len: u8,
    /// Hardware envelope step 0–7 (0 = const when not soft ADSR).
    pub env_step: u8,
    /// NR10 sweep shift 0–7 (0 = off). Channel 1 only.
    pub sweep_shift: u8,
    /// NR10 sweep period 0–7 (0 = off).
    pub sweep_period: u8,
    /// Noise clock shift 0–13 when [`FLAG_NOISE_SHIFT_SET`].
    pub noise_shift: u8,
    /// Noise divisor ratio 0–7.
    pub noise_ratio: u8,
    /// Soft pitch glide (±semis) when HW sweep is not used — gen `sweep`.
    pub soft_sweep: i8,
    /// First key zone in [`DeckSong::zones`], for [`VOICE_DS_SAMPLE`].
    pub zone_lo: u8,
    /// How many zones this voice has (0 = not a sampled instrument).
    pub zone_n: u8,
    /// Stereo position, -64..63, 0 = centre. From the track's M4A `PAN`.
    pub pan: i8,
}

/// One conditioned instrument recording.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeckSample {
    pub data: SoundData,
    /// The rate these bytes were stored at, which is what `root` sounds at.
    pub rate_hz: u32,
    /// Loop point in samples, or `u32::MAX` for a one-shot.
    ///
    /// ⚠️ Not a detail. Looping a one-shot drum turns it into a buzz, and failing to loop a
    /// sustained string turns a held note into a pluck — which is most of why the old
    /// unconditional `should_loop()` made everything sound like a synth.
    pub loop_start: u32,
}

/// A key range within a sampled instrument. One sampled string section spans six recordings across the
/// keyboard; one mixer channel plays whichever zone the struck note lands in.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeckZone {
    pub lo: u8,
    pub hi: u8,
    pub sample: u8,
    /// MIDI key at which the sample plays back at its stored rate.
    pub root: u8,
    /// A pan fixed by the voicegroup, which overrides the track's. Only meaningful when
    /// [`Self::pan_set`] — a voicegroup that does not specify one leaves the track in charge.
    pub pan: i8,
    pub pan_set: bool,
    /// Play at the stored rate whatever key is struck (M4A voice type `0x08` — percussion).
    pub fixed: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeckNote {
    pub start: u16,
    pub dur: u16,
    pub midi: u8,
    pub vel: u8,
}

pub struct DeckTrack {
    pub voice: DeckVoice,
    /// Stem is audible only when player intensity >= this (0 = always). crossfading-stem-style layers.
    pub min_intensity: u8,
    pub notes: &'static [DeckNote],
}

pub struct DeckSong {
    pub loop_frame: u32,
    pub length_frames: u32,
    pub tracks: &'static [DeckTrack],
    pub waves: &'static [[u8; 16]],
    pub pcm_tables: &'static [SoundData],
    pub samples: &'static [DeckSample],
    pub zones: &'static [DeckZone],
}

impl DeckSong {
    /// The zone covering `midi`, or the nearest one. Zones are few (≤8) and ordered, so a linear
    /// scan is cheaper than anything cleverer and runs once per note-on, not per frame.
    fn zone_for(&self, voice: &DeckVoice, midi: u8) -> Option<DeckZone> {
        let lo = voice.zone_lo as usize;
        let zs = self.zones.get(lo..lo + voice.zone_n as usize)?;
        zs.iter()
            .find(|z| midi >= z.lo && midi <= z.hi)
            .or(zs.last())
            .copied()
    }
}

#[derive(Clone, Copy)]
enum AdsrPhase {
    Attack,
    Decay,
    Sustain,
    Release,
    Done,
}

#[derive(Clone, Copy)]
struct VoiceRt {
    active: bool,
    note_idx: usize,
    age: u16,
    gate: u16,
    vel: u8,
    phase: AdsrPhase,
    phase_age: u16,
    soft_vol: u8,
    pcm_id: Option<ChannelId>,
    base_cents: i32,
    /// The key this note started on, so re-pitching cannot cross a zone boundary mid-note.
    zone_key: u8,
}

impl VoiceRt {
    const fn idle() -> Self {
        VoiceRt {
            active: false,
            note_idx: 0,
            age: 0,
            gate: 0,
            vel: 0,
            phase: AdsrPhase::Done,
            phase_age: 0,
            soft_vol: 0,
            pcm_id: None,
            base_cents: 0,
            zone_key: 0,
        }
    }
}

// PSG stems and sampled stems coexist in one song, so this is 4 PSG + the PCM slots.
const MAX_TRACKS: usize = 8;

pub struct Player {
    song: Option<&'static DeckSong>,
    frame: u32,
    /// Combat/threat layer 0..=3 — stems with `min_intensity` above this stay silent.
    intensity: u8,
    voices: [VoiceRt; MAX_TRACKS],
    borrowed: [u8; 4],
    loaded_wave: i16,
    pcm_slots: [Option<ChannelId>; MAX_PCM_SLOTS],
    /// Playhead frozen. The song, the playhead, the voices and the intensity all survive — which is
    /// the whole point, and the difference between this and stop()+play().
    paused: bool,
    /// Master duck gain, 0..=64, applied on top of everything. 64 is unattenuated.
    duck: u8,
}

impl Player {
    pub const fn new() -> Self {
        Player {
            song: None,
            frame: 0,
            intensity: 0,
            paused: false,
            duck: 64,
            voices: [VoiceRt::idle(); MAX_TRACKS],
            borrowed: [0; 4],
            loaded_wave: -1,
            pcm_slots: [None; MAX_PCM_SLOTS],
        }
    }

    pub fn playing(&self) -> bool {
        self.song.is_some()
    }

    pub fn intensity(&self) -> u8 {
        self.intensity
    }

    /// The playhead, in sequencer frames since `play` (0 while stopped).
    ///
    /// This is the same clock `DeckNote::start` is keyed to, which is what makes it useful to a
    /// rhythm game: a chart that places a beat at frame N lands on the note the song plays at
    /// frame N, with no conversion and no second timebase to drift against. Counting `frame()`
    /// calls tish-side is NOT equivalent — `music_catchup` advances this playhead once per elapsed
    /// display frame, so a stalled frame moves the song without moving a tish-side counter.
    pub fn playhead(&self) -> u32 {
        self.frame
    }

    /// Set intensifier level (clamped 0..=3). Playhead stays put; stems fade in/out on the next notes.
    pub fn set_intensity(&mut self, level: u8, mixer: &mut Option<Mixer<'static>>) {
        let level = level.min(3);
        if level == self.intensity {
            return;
        }
        let Some(song) = self.song else {
            self.intensity = level;
            return;
        };
        let old = self.intensity;
        self.intensity = level;
        let track_count = song.tracks.len().min(MAX_TRACKS);
        for ti in 0..track_count {
            let min_i = song.tracks[ti].min_intensity;
            let was_on = old >= min_i;
            let now_on = level >= min_i;
            if was_on && !now_on && self.voices[ti].active {
                self.end_voice(ti, song.tracks[ti].voice, mixer, true);
            }
        }
    }

    /// Freeze or resume the playhead. Silences the PSG on the way in; re-arms the wavetable on the
    /// way out, for the same reason `play` loads it eagerly — a wavetable write that lands at an
    /// arbitrary point in a frame is what froze a game on a cave entrance.
    pub fn set_paused(&mut self, on: bool) {
        if on == self.paused {
            return;
        }
        self.paused = on;
        if on {
            psg::stop_all();
        } else if let Some(song) = self.song {
            for tr in song.tracks.iter().take(MAX_TRACKS) {
                if tr.voice.kind == VOICE_WAVE {
                    if let Some(table) = song.waves.get(tr.voice.wave as usize) {
                        psg::wave_table(table);
                        self.loaded_wave = tr.voice.wave as i16;
                    }
                    break;
                }
            }
        }
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Master duck, 0..=64. Attenuates the music so a line of dialogue or a stinger can sit on top
    /// of it, without touching the song or the mixer's other channels.
    pub fn set_duck(&mut self, gain: u8) {
        self.duck = gain.min(64);
    }

    pub fn duck(&self) -> u8 {
        self.duck
    }

    pub fn has_pcm(&self) -> bool {
        self.song
            .map(|s| !s.pcm_tables.is_empty() || !s.samples.is_empty())
            .unwrap_or(false)
    }

    pub fn borrow_channel(&mut self, ch: u8, frames: u8) {
        if (1..=4).contains(&ch) {
            self.borrowed[(ch - 1) as usize] = frames.max(1);
        }
    }

    pub fn stop(&mut self, mixer: &mut Option<Mixer<'static>>) {
        self.stop_pcm(mixer);
        psg::stop_all();
        self.song = None;
        self.frame = 0;
        self.voices = [VoiceRt::idle(); MAX_TRACKS];
        self.borrowed = [0; 4];
        self.loaded_wave = -1;
    }

    pub fn play(&mut self, song: &'static DeckSong, mixer: &mut Option<Mixer<'static>>) {
        self.stop(mixer);
        self.song = Some(song);
        self.frame = 0;
        self.intensity = 0;
        self.paused = false;
        // Load the wavetable HERE, not lazily on the first wave note in `step`.
        //
        // `wave_table` switches the play-bank and writes 16 bytes into WAVE_RAM. Done from inside
        // `step` it lands at an arbitrary point in the frame — including partway through a UI canvas
        // build — and that combination corrupts memory: the topdown RPG port froze on entering a cave with
        // `Jumped to invalid address`, and the crash disappeared entirely when the songs were
        // regenerated with no wave channel at all. Every other candidate (the song, the scene, the
        // ordering of `deckPlay`, the heap, the mixer, sound effects, note density) was ruled out by
        // build. Doing it once at play time keeps the write next to the channel setup where it
        // belongs and costs nothing — a song has one wavetable.
        for tr in song.tracks.iter().take(MAX_TRACKS) {
            if tr.voice.kind == VOICE_WAVE {
                if let Some(table) = song.waves.get(tr.voice.wave as usize) {
                    psg::wave_table(table);
                    self.loaded_wave = tr.voice.wave as i16;
                }
                break;
            }
        }
        for (i, _) in song.tracks.iter().enumerate().take(MAX_TRACKS) {
            self.voices[i].note_idx = 0;
        }
    }

    fn stop_pcm(&mut self, mixer: &mut Option<Mixer<'static>>) {
        if let Some(m) = mixer.as_mut() {
            for slot in self.pcm_slots.iter_mut() {
                if let Some(id) = slot.take() {
                    if let Some(ch) = m.channel(&id) {
                        ch.stop();
                    }
                }
            }
        }
        for v in self.voices.iter_mut() {
            v.pcm_id = None;
        }
    }

    pub fn step(&mut self, mixer: &mut Option<Mixer<'static>>) {
        for left in self.borrowed.iter_mut() {
            *left = left.saturating_sub(1);
        }
        // ⚠️ PAUSED IS NOT STOPPED. Everything below is skipped, but `song`, `frame`, `voices` and
        // `intensity` are untouched, so unpausing resumes mid-bar rather than from the top.
        //
        // This exists because the discipline it replaces was `deck_stop()` + `deck_play()` around
        // anything that rebuilds the UI canvas — which loses the playhead AND resets the intensity
        // to 0, so a game hushing its music for a menu came back at the wrong point of the song and
        // the wrong threat level. Two ports wrote that hush by hand and disagreed about it.
        if self.paused {
            return;
        }
        let Some(song) = self.song else { return };

        if self.frame >= song.length_frames {
            if song.loop_frame == u32::MAX {
                self.stop(mixer);
                return;
            }
            self.frame = song.loop_frame.min(song.length_frames.saturating_sub(1));
            for i in 0..song.tracks.len().min(MAX_TRACKS) {
                let track = &song.tracks[i];
                let mut idx = 0;
                while idx < track.notes.len() && (track.notes[idx].start as u32) < self.frame {
                    idx += 1;
                }
                self.voices[i].note_idx = idx;
                if self.voices[i].active {
                    self.end_voice(i, track.voice, mixer, false);
                }
            }
        }

        let frame = self.frame;
        let track_count = song.tracks.len().min(MAX_TRACKS);
        for ti in 0..track_count {
            loop {
                let note = {
                    let v = &self.voices[ti];
                    let notes = song.tracks[ti].notes;
                    if v.note_idx >= notes.len() {
                        break;
                    }
                    let n = notes[v.note_idx];
                    if (n.start as u32) > frame {
                        break;
                    }
                    n
                };
                if (note.start as u32) == frame {
                    self.note_on(ti, &song.tracks[ti], song, note, mixer);
                }
                self.voices[ti].note_idx += 1;
            }
            if self.voices[ti].active {
                self.modulate(ti, song.tracks[ti].voice, song, mixer);
            }
        }
        self.frame = self.frame.saturating_add(1);
    }

    fn end_voice(
        &mut self,
        ti: usize,
        voice: DeckVoice,
        mixer: &mut Option<Mixer<'static>>,
        soft_release: bool,
    ) {
        // ⚠️ Sampled voices take the release ramp too. They used to be excluded, so a note-off
        // called `ch.stop()` on a sustained instrument mid-waveform — a hard step from full
        // amplitude to zero, i.e. a click on EVERY note. Hundreds of notes a minute of that reads
        // as crackle sitting under the music rather than as a series of discrete clicks, which is
        // why it is easy to blame on the samples. The release machinery already existed and
        // `modulate` already drives the mixer channel's volume; only this test excluded it.
        //
        // A generated wavetable (`VOICE_DS_PCM`) clicks too, just more quietly, and gains the same
        // fix by sharing the path.
        if soft_release && voice.env_mode == ENV_ADSR && voice.r > 0 {
            self.voices[ti].phase = AdsrPhase::Release;
            self.voices[ti].phase_age = 0;
            return;
        }
        let pcm_id = self.voices[ti].pcm_id.take();
        self.voices[ti].active = false;
        self.voices[ti].phase = AdsrPhase::Done;
        if is_pcm(voice.kind) {
            if let (Some(id), Some(m)) = (pcm_id, mixer.as_mut()) {
                if let Some(ch) = m.channel(&id) {
                    ch.stop();
                }
            }
            let slot = (voice.hw_ch as usize).min(MAX_PCM_SLOTS - 1);
            self.pcm_slots[slot] = None;
        } else if self
            .borrowed
            .get(voice.hw_ch.wrapping_sub(1) as usize)
            .copied()
            .unwrap_or(0)
            == 0
        {
            psg::stop(voice.hw_ch);
        }
    }

    fn note_on(
        &mut self,
        ti: usize,
        track: &DeckTrack,
        song: &DeckSong,
        note: DeckNote,
        mixer: &mut Option<Mixer<'static>>,
    ) {
        if self.intensity < track.min_intensity {
            return;
        }
        let voice = track.voice;
        if !is_pcm(voice.kind) {
            let ch = voice.hw_ch;
            if (1..=4).contains(&ch) && self.borrowed[(ch - 1) as usize] > 0 {
                return;
            }
        }

        if self.voices[ti].active {
            self.end_voice(ti, voice, mixer, false);
        }

        let vel_scale = (note.vel as u16).max(1);
        // The duck is a Q6 multiply and a shift — never a divide (docs/perf-rules.md §3). It rides
        // on the existing /127, which is the one division already here and is per NOTE, not per
        // frame.
        let base_vol =
            ((((voice.vol as u16 * vel_scale) / 127) * self.duck as u16) >> 6).min(15) as u8;
        let soft = voice.env_mode == ENV_ADSR || is_pcm(voice.kind);
        {
            let rt = &mut self.voices[ti];
            rt.active = true;
            rt.age = 0;
            rt.zone_key = note.midi;
            rt.gate = note.dur.max(1);
            rt.vel = note.vel;
            rt.base_cents = note.midi as i32 * 100;
            rt.soft_vol = if soft && voice.a > 0 { 0 } else { base_vol };
            rt.phase_age = 0;
            rt.pcm_id = None;
            rt.phase = if soft {
                if voice.a > 0 {
                    AdsrPhase::Attack
                } else if voice.d > 0 {
                    AdsrPhase::Decay
                } else {
                    AdsrPhase::Sustain
                }
            } else {
                AdsrPhase::Sustain
            };
        }

        match voice.kind {
            VOICE_PULSE => {
                let env_up = voice.flags & FLAG_ENV_UP != 0;
                let decay = if soft {
                    0
                } else if voice.env_mode == ENV_STEP {
                    if voice.env_step > 0 {
                        voice.env_step.min(7)
                    } else {
                        (voice.vol / 2).clamp(1, 7)
                    }
                } else {
                    voice.env_step.min(7) // const: 0 unless authored
                };
                let len = voice.len.min(63);
                // HW sweep (NR10) is ch1-only. Soft gen `sweep` auto-maps when shift unset.
                let mut sw_shift = voice.sweep_shift.min(7);
                let mut sw_period = voice.sweep_period.min(7);
                let mut sw_down = voice.flags & FLAG_SWEEP_DOWN != 0;
                if sw_shift == 0 && sw_period == 0 && voice.soft_sweep != 0 && voice.hw_ch == 1 {
                    let s = voice.soft_sweep.unsigned_abs().clamp(1, 7);
                    sw_shift = s;
                    sw_period = 3;
                    sw_down = voice.soft_sweep < 0;
                }
                if voice.hw_ch == 1 && sw_shift > 0 && sw_period > 0 {
                    psg::slide(
                        note.midi as i32,
                        voice.duty,
                        base_vol.max(1),
                        decay,
                        len,
                        sw_shift,
                        sw_period,
                        sw_down,
                        env_up,
                    );
                } else {
                    psg::square(
                        voice.hw_ch,
                        note.midi as i32,
                        voice.duty,
                        base_vol.max(1),
                        decay,
                        len,
                        env_up,
                    );
                }
            }
            VOICE_WAVE => {
                if self.loaded_wave != voice.wave as i16 {
                    if let Some(table) = song.waves.get(voice.wave as usize) {
                        psg::wave_table(table);
                        self.loaded_wave = voice.wave as i16;
                    }
                }
                let wvol = match base_vol {
                    0 => 0,
                    1..=5 => 3,
                    6..=10 => 2,
                    _ => 1,
                };
                psg::wave(note.midi as i32, wvol, voice.len);
            }
            VOICE_NOISE => {
                let narrow = voice.flags & FLAG_NOISE_NARROW != 0;
                let env_up = voice.flags & FLAG_ENV_UP != 0;
                let decay = if soft {
                    0
                } else if voice.env_mode == ENV_STEP {
                    if voice.env_step > 0 {
                        voice.env_step.min(7)
                    } else {
                        (voice.vol / 2).clamp(1, 7)
                    }
                } else {
                    voice.env_step.min(7)
                };
                let shift = if voice.flags & FLAG_NOISE_SHIFT_SET != 0 {
                    voice.noise_shift.min(13)
                } else {
                    (note.midi / 8).min(13)
                };
                psg::noise(
                    base_vol,
                    decay,
                    voice.len.min(63),
                    shift,
                    voice.noise_ratio.min(7),
                    narrow,
                    env_up,
                );
            }
            VOICE_DS_PCM => {
                let Some(data) = song.pcm_tables.get(voice.wave as usize).copied() else {
                    self.voices[ti].active = false;
                    return;
                };
                let slot = (voice.hw_ch as usize).min(MAX_PCM_SLOTS - 1);
                if let Some(m) = mixer.as_mut() {
                    if let Some(old) = self.pcm_slots[slot].take() {
                        if let Some(ch) = m.channel(&old) {
                            ch.stop();
                        }
                    }
                    let mut channel = SoundChannel::new_high_priority(data);
                    channel.should_loop();
                    channel.playback(pcm_playback_rate(note.midi as i32, 32));
                    channel.volume(pcm_volume(base_vol));
                    let id = m.play_sound(channel);
                    self.pcm_slots[slot] = id;
                    self.voices[ti].pcm_id = id;
                }
            }
            VOICE_DS_SAMPLE => {
                let Some(zone) = song.zone_for(&voice, note.midi) else {
                    self.voices[ti].active = false;
                    return;
                };
                let Some(smp) = song.samples.get(zone.sample as usize).copied() else {
                    self.voices[ti].active = false;
                    return;
                };
                let slot = (voice.hw_ch as usize).min(MAX_PCM_SLOTS - 1);
                if let Some(m) = mixer.as_mut() {
                    if let Some(old) = self.pcm_slots[slot].take() {
                        if let Some(ch) = m.channel(&old) {
                            ch.stop();
                        }
                    }
                    // ⚠️ Low priority, deliberately. `Mixer::play_sound` *panics* when every
                    // high-priority channel is taken, so music that claimed high priority would
                    // turn a busy bar plus a couple of sound effects into a crash. Music that
                    // drops a voice under load is a glitch; music that panics is a bug report.
                    let mut channel = SoundChannel::new(smp.data);
                    if smp.loop_start != u32::MAX {
                        channel.should_loop().restart_point(smp.loop_start);
                    }
                    channel.playback(sample_playback_rate(&smp, &zone, note.midi as i32));
                    channel.volume(pcm_volume(base_vol));
                    channel.panning(sample_panning(if zone.pan_set {
                        zone.pan
                    } else {
                        voice.pan
                    }));
                    let id = m.play_sound(channel);
                    self.pcm_slots[slot] = id;
                    self.voices[ti].pcm_id = id;
                }
            }
            _ => {
                self.voices[ti].active = false;
            }
        }
    }

    fn modulate(
        &mut self,
        ti: usize,
        voice: DeckVoice,
        song: &DeckSong,
        mixer: &mut Option<Mixer<'static>>,
    ) {
        if !is_pcm(voice.kind) {
            let ch = voice.hw_ch;
            if (1..=4).contains(&ch) && self.borrowed[(ch - 1) as usize] > 0 {
                return;
            }
        }

        let mut do_stop = false;
        let soft_vol;
        let cents;
        let base_cents;
        let pcm_id;

        {
            let rt = &mut self.voices[ti];
            if !rt.active {
                return;
            }
            rt.age = rt.age.saturating_add(1);

            if rt.age >= rt.gate && !matches!(rt.phase, AdsrPhase::Release | AdsrPhase::Done) {
                if (voice.env_mode == ENV_ADSR || is_pcm(voice.kind)) && voice.r > 0 {
                    rt.phase = AdsrPhase::Release;
                    rt.phase_age = 0;
                } else {
                    do_stop = true;
                }
            }

            if do_stop {
                soft_vol = 0;
                cents = 0;
                base_cents = 0;
                pcm_id = rt.pcm_id;
            } else {
                let soft = voice.env_mode == ENV_ADSR || is_pcm(voice.kind);
                if soft {
                    let peak = ((voice.vol as u16 * rt.vel as u16) / 127).min(15) as u8;
                    // ⚠️ Sustain is a FRACTION of this note's peak, not an absolute level.
                    //
                    // It used to be `voice.s` directly, which silently discarded both the track
                    // volume and the note velocity the instant a note left its attack: with the
                    // common `s: 15`, every note — however quiet it was struck — sustained at
                    // full scale. Decay then interpolated from `peak` to an absolute `sustain`,
                    // so a quiet note ramped *up*. It made per-track mix automation a no-op,
                    // which is what a whole song's balance is built out of.
                    let sustain = ((peak as u16 * voice.s.min(15) as u16) / 15) as u8;
                    rt.phase_age = rt.phase_age.saturating_add(1);
                    match rt.phase {
                        AdsrPhase::Attack => {
                            if voice.a == 0 {
                                rt.soft_vol = peak;
                                rt.phase = AdsrPhase::Decay;
                                rt.phase_age = 0;
                            } else {
                                let t = rt.phase_age.min(voice.a as u16);
                                rt.soft_vol = ((peak as u16 * t) / voice.a as u16) as u8;
                                if rt.phase_age >= voice.a as u16 {
                                    rt.phase = AdsrPhase::Decay;
                                    rt.phase_age = 0;
                                }
                            }
                        }
                        AdsrPhase::Decay => {
                            if voice.d == 0 {
                                rt.soft_vol = sustain;
                                rt.phase = AdsrPhase::Sustain;
                            } else {
                                let t = rt.phase_age.min(voice.d as u16);
                                let from = peak as i16;
                                let to = sustain as i16;
                                rt.soft_vol = (from + ((to - from) * t as i16) / voice.d as i16)
                                    .clamp(0, 15)
                                    as u8;
                                if rt.phase_age >= voice.d as u16 {
                                    rt.phase = AdsrPhase::Sustain;
                                }
                            }
                        }
                        AdsrPhase::Sustain => rt.soft_vol = sustain,
                        AdsrPhase::Release => {
                            if voice.r == 0 || rt.phase_age >= voice.r as u16 {
                                do_stop = true;
                            } else {
                                let from = sustain as i16;
                                rt.soft_vol = (from - (from * rt.phase_age as i16) / voice.r as i16)
                                    .clamp(0, 15)
                                    as u8;
                            }
                        }
                        AdsrPhase::Done => {}
                    }
                }
                // ⚠️ Ducked HERE rather than at the envelope, so the envelope's own shape is
                // untouched: a duck changes how loud the note is, not how it decays. A voice in
                // ENV_STEP or ENV_CONST mode has its level owned by the hardware envelope and only
                // takes the duck on its next attack — that is what the hardware permits, not a
                // workaround, and it is why a stem you want to FADE should be authored ADSR or PCM.
                soft_vol = ((rt.soft_vol as u16 * self.duck as u16) >> 6) as u8;
                base_cents = rt.base_cents;
                cents = apply_pitch_mods(voice, rt.age, rt.base_cents);
                pcm_id = rt.pcm_id;
            }
        }

        if do_stop {
            self.end_voice(ti, voice, mixer, false);
            return;
        }

        match voice.kind {
            VOICE_PULSE => {
                if voice.env_mode == ENV_ADSR {
                    psg::set_square_vol(voice.hw_ch, voice.duty, soft_vol);
                }
                // Don't fight NR10: hardware walks frequency when sweep is armed on ch1.
                let hw_sweep = voice.hw_ch == 1
                    && ((voice.sweep_shift > 0 && voice.sweep_period > 0) || voice.soft_sweep != 0);
                if !hw_sweep {
                    psg::set_square_rate(voice.hw_ch, psg::square_rate_cents(cents));
                }
            }
            VOICE_WAVE => {
                if voice.env_mode == ENV_ADSR {
                    let wvol = match soft_vol {
                        0 => 0,
                        1..=5 => 3,
                        6..=10 => 2,
                        _ => 1,
                    };
                    psg::set_wave_vol(wvol);
                }
                psg::set_wave_rate(psg::wave_rate_cents(cents));
            }
            VOICE_NOISE => {
                if voice.env_mode == ENV_ADSR {
                    psg::set_noise_vol(soft_vol);
                }
                // Map pitch mods onto the noise divisor tree (shift + ratio).
                let narrow = voice.flags & FLAG_NOISE_NARROW != 0;
                let delta_semis = (cents - base_cents) / 100;
                let shift = if voice.flags & FLAG_NOISE_SHIFT_SET != 0 {
                    (voice.noise_shift as i32 + delta_semis).clamp(0, 13) as u8
                } else {
                    ((cents / 100) / 8).clamp(0, 13) as u8
                };
                let ratio = voice.noise_ratio.min(7);
                psg::set_noise_div(shift, ratio, narrow);
            }
            VOICE_DS_PCM => {
                if let (Some(id), Some(m)) = (pcm_id, mixer.as_mut()) {
                    if let Some(ch) = m.channel(&id) {
                        ch.volume(pcm_volume(soft_vol));
                        ch.playback(pcm_playback_rate(cents / 100, 32));
                    }
                }
            }
            VOICE_DS_SAMPLE => {
                if let (Some(id), Some(m)) = (pcm_id, mixer.as_mut()) {
                    if let Some(ch) = m.channel(&id) {
                        ch.volume(pcm_volume(soft_vol));
                        // Re-pitch off the *same* zone the note started in. Picking the zone from
                        // the modulated pitch would let a deep vibrato cross a zone boundary and
                        // swap the recording mid-note.
                        let midi = (cents / 100).clamp(0, 127);
                        if let Some(zone) = song.zone_for(&voice, self.voices[ti].zone_key) {
                            if let Some(smp) = song.samples.get(zone.sample as usize) {
                                ch.playback(sample_playback_rate(smp, &zone, midi));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

fn apply_pitch_mods(voice: DeckVoice, age: u16, mut cents: i32) -> i32 {
    if voice.drop_semis != 0 && voice.drop_dec > 0 {
        let dec = voice.drop_dec as i32;
        let age = age as i32;
        if age < dec {
            cents += (voice.drop_semis as i32) * 100 * (dec - age) / dec;
        }
    } else if voice.arp_rate > 0 && voice.arp_semis != 0 {
        let step_frames = (60u16 / voice.arp_rate as u16).max(1);
        if ((age / step_frames) & 1) == 1 {
            cents += voice.arp_semis as i32 * 100;
        }
    }
    // Soft gen `sweep` when HW NR10 is not driving (ch2, or no auto-map).
    // Glide from base toward base+soft_sweep over ~drop_dec frames (fallback 8).
    let hw_sweep = voice.hw_ch == 1
        && ((voice.sweep_shift > 0 && voice.sweep_period > 0) || voice.soft_sweep != 0);
    if !hw_sweep && voice.soft_sweep != 0 {
        let dec = if voice.drop_dec > 0 {
            voice.drop_dec as i32
        } else {
            8
        };
        let age = age as i32;
        if age < dec {
            cents += (voice.soft_sweep as i32) * 100 * age / dec;
        } else {
            cents += voice.soft_sweep as i32 * 100;
        }
    }
    if voice.vib_rate > 0 && voice.vib_amt > 0 && voice.kind != VOICE_NOISE {
        let phase = (age as i32 * voice.vib_rate as i32) / 4;
        let tri = ((phase % 120) - 60).abs();
        cents += (tri - 30) * (voice.vib_amt as i32 * 2) / 30;
    }
    cents
}

fn pcm_volume(vol_0_15: u8) -> Num<i16, 8> {
    Num::from_raw(((vol_0_15 as i32) * 256 / 15).clamp(0, 256) as i16)
}

fn pcm_playback_rate(midi: i32, table_len: i32) -> Num<u32, 8> {
    let f_milli = midi_freq_milli(midi);
    let num = f_milli as i64 * table_len as i64 * 256;
    let den = MIXER_RATE_HZ as i64 * 1000;
    Num::from_raw((num / den).clamp(1, 31 * 256) as u32)
}

/// How fast to walk a recorded sample so the struck key sounds at the right pitch.
///
/// Two ratios multiplied: the sample's stored rate against the mixer's (a recording made at
/// 22050Hz has to be read at 2.1 samples per mixer step to sound unchanged), and the struck key
/// against the zone's root key.
///
/// A `fixed` zone skips the second ratio entirely — that is what M4A voice type `0x08` means, and
/// it is why a sampled drum kit stays drums instead of becoming a chromatic tom.
fn sample_playback_rate(smp: &DeckSample, zone: &DeckZone, midi: i32) -> Num<u32, 8> {
    let mut num = smp.rate_hz as i64 * 256;
    let mut den = MIXER_RATE_HZ as i64;
    if !zone.fixed {
        num *= midi_freq_milli(midi) as i64;
        den *= midi_freq_milli(zone.root as i32) as i64;
    }
    // agb clamps playback speed well below this; the floor of 1 keeps a note audible rather than
    // silently stopping the channel.
    let raw = (num / den.max(1)).clamp(1, 64 * 256) as u32;
    Num::from_raw(avoid_agb_temp_storage_panic(raw))
}

/// ⚠️ Work around an off-by-one in agb 0.25's mixer: one playback speed *panics*.
///
/// `sw_mixer.rs` escapes to the read-from-ROM path only when `playback_speed >= 1.5`, but the
/// temp-storage copy it otherwise takes asserts `(total_to_play / 2 + 1) * 2 <= temp_storage`,
/// which stops holding just below that — `total_to_play` is `floor(speed * 176) + 1` against a
/// 265-entry buffer, so speeds in `[1.4943, 1.5)` overrun the assert instead of taking the escape.
///
/// In Q8 that band contains exactly one representable value, 383, so nudging it to 384 costs 4.5
/// cents on one quantised speed and nothing else. Generated 32-byte wavetables never hit this
/// (they are shorter than the buffer, so they take a different branch); real instrument samples
/// sweep a continuum of speeds and land on it within seconds.
///
/// Remove once the fork's mixer makes the two bounds agree.
#[inline]
fn avoid_agb_temp_storage_panic(raw: u32) -> u32 {
    if raw == 383 {
        384
    } else {
        raw
    }
}

/// Stereo position, -64..63 with **0 = centre**, to agb's -1.0..1.0.
///
/// ⚠️ This used to take the raw M4A byte and subtract 64, treating an unspecified pan of `0` as
/// hard left. Every voicegroup in the source bank stores `0` there, so the entire soundtrack collapsed into one
/// channel — measured 21dB off-centre against a cartridge that is dead centre. A voicegroup pan is
/// only meaningful when its `0x80` bit is set; the conversion now happens once, in the extractor,
/// and what arrives here is already signed and already centred.
fn sample_panning(pan: i8) -> Num<i16, 8> {
    Num::from_raw(((pan as i32) * 256 / 64).clamp(-256, 256) as i16)
}

fn midi_freq_milli(midi: i32) -> i32 {
    const A4: i32 = 440_000;
    const RATIO: [i32; 12] = [
        1000, 1059, 1122, 1189, 1260, 1335, 1414, 1498, 1587, 1682, 1782, 1888,
    ];
    let semis = midi - 69;
    let oct = semis.div_euclid(12);
    let st = semis.rem_euclid(12) as usize;
    let mut f = A4 as i64 * RATIO[st] as i64 / 1000;
    if oct >= 0 {
        f <<= oct.min(4);
    } else {
        f >>= (-oct).min(4);
    }
    f.clamp(20_000, 8_000_000) as i32
}
