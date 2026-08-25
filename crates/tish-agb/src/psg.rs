//! The GBA's PSG synth — two square channels, a 32-step wavetable channel and a noise channel —
//! driven straight through their hardware registers.
//!
//! **Why bother, when we already have a mixer.** A mixer voice is a *recording*: akari's two themes
//! are 2.2 MB of its 4.47 MB ROM, and playing them costs a slice of every frame forever. A PSG voice
//! is a *register write*. It costs no ROM at all, and the CPU cost is genuinely zero — the hardware
//! oscillates on its own, so nothing has to be fed. That last part is worth more than it sounds:
//! `pump_audio`'s scattered call sites in `lib.rs` exist entirely because a long synchronous build (a
//! menu layout, a dialog page, a scene stream) never returns to `frame()` and starves the software
//! mixer's ~50ms of buffer. PSG music cannot starve, so none of that applies to it.
//!
//! **agb doesn't support this, but it also isn't in the way.** agb 0.25's `sound` module is the
//! software mixer and nothing else; it never touches `NR10`–`NR44`, wave RAM, or `SOUNDCNT_L`. It
//! does own `SOUNDCNT_H` (Direct Sound) and sets the master enable in `SOUNDCNT_X`, which is why
//! [`init`] is a careful read-modify-write of the former and why it must run *after* the mixer is
//! constructed — agb's blanket write to `SOUNDCNT_H` would otherwise clobber our PSG volume bits
//! back to 25%.

/// Volatile write to an I/O register.
#[inline(always)]
fn w16(addr: usize, v: u16) {
    unsafe { (addr as *mut u16).write_volatile(v) }
}

/// Volatile read of an I/O register.
#[inline(always)]
fn r16(addr: usize) -> u16 {
    unsafe { (addr as *const u16).read_volatile() }
}

// Per-channel registers. Each GBA sound channel is a pair of 16-bit words: one packing the
// duty/length byte with the envelope byte, one packing the 11-bit frequency with the control bits.
const SOUND1CNT_L: usize = 0x0400_0060; // ch1 sweep
const SOUND1CNT_H: usize = 0x0400_0062; // ch1 duty/length + envelope
const SOUND1CNT_X: usize = 0x0400_0064; // ch1 frequency + control
const SOUND2CNT_L: usize = 0x0400_0068; // ch2 duty/length + envelope
const SOUND2CNT_H: usize = 0x0400_006C; // ch2 frequency + control
const SOUND3CNT_L: usize = 0x0400_0070; // ch3 enable / wave bank
const SOUND3CNT_H: usize = 0x0400_0072; // ch3 length + volume
const SOUND3CNT_X: usize = 0x0400_0074; // ch3 frequency + control
const SOUND4CNT_L: usize = 0x0400_0078; // ch4 length + envelope
const SOUND4CNT_H: usize = 0x0400_007C; // ch4 noise divisor + control
const WAVE_RAM: usize = 0x0400_0090; // ch3 wavetable, 16 bytes = 32 nibble samples

const SOUNDCNT_L: usize = 0x0400_0080; // PSG routing + per-side volume — entirely ours
const SOUNDCNT_H: usize = 0x0400_0082; // bits 0-1 PSG mix ratio (ours), 2+ Direct Sound (agb's)
const SOUNDCNT_X: usize = 0x0400_0084; // bit 7 master enable

/// Restart the channel (latch the envelope and begin the note).
const RESTART: u16 = 1 << 15;
/// Stop at the end of the programmed length rather than sustaining.
const USE_LENGTH: u16 = 1 << 14;

/// Wave period (the `131072/F` term) for each semitone of octave 2, highest-resolution octave the
/// square channels can reach. Every other octave is this shifted right, because the period halves
/// per octave — which keeps note lookup to a table index and a shift, with no division and no
/// floating point on a CPU that has neither.
///
/// C2 is the floor: the register holds `2048 - period`, so a period above 2047 is unrepresentable
/// and the square channels simply cannot play below ~64 Hz.
const PERIOD_C2: [u16; 12] = [
    2004, 1892, 1785, 1685, 1591, 1501, 1417, 1338, 1262, 1192, 1125, 1062,
];

/// Semitones per octave, and the octave `PERIOD_C2` is measured at.
const BASE_OCTAVE: i32 = 2;

/// Convert a MIDI-style note number (60 = C4) to a square channel's 11-bit frequency register.
///
/// Notes below C2 are clamped rather than dropped: a bass line written an octave too low should
/// sound wrong-but-present, not silently vanish, because a silent channel in a four-channel
/// arrangement is very hard to notice and very easy to ship.
pub fn square_rate(note: i32) -> u16 {
    let note = note.clamp(24, 119); // C1..B8; below C2 is clamped by the shift below
    let semitone = (note % 12) as usize;
    let octave = note / 12 - 1; // MIDI note 60 = C4 => octave 4
    let shift = (octave - BASE_OCTAVE).clamp(0, 10);
    let period = (PERIOD_C2[semitone] >> shift).max(1);
    2048u16.saturating_sub(period)
}

/// Same note, on the wavetable channel. Channel 3 consumes 32 samples per cycle at
/// `2097152/(2048-n)` samples per second, so its output tone is `65536/(2048-n)` — exactly half the
/// square channels' period for the same pitch.
pub fn wave_rate(note: i32) -> u16 {
    let note = note.clamp(12, 119);
    let semitone = (note % 12) as usize;
    let octave = note / 12 - 1;
    // One shift further than the square channels for the same pitch — which also means the wave
    // channel reaches a full octave LOWER than they can, down to C1. Bass lines live there, so the
    // difference is not academic: shifting by the square amount and halving afterwards would clamp
    // C1 up to C2 and quietly transpose every bass note in the lowest octave.
    let shift = (octave - BASE_OCTAVE + 1).clamp(0, 11);
    let period = (PERIOD_C2[semitone] >> shift).max(1);
    2048u16.saturating_sub(period)
}

/// Square-channel rate from pitch in cents (`midi * 100 + cents`). Used for vibrato / soft slides
/// that need sub-semitone resolution. Interpolates between adjacent MIDI notes.
pub fn square_rate_cents(note_cents: i32) -> u16 {
    let note_cents = note_cents.clamp(24 * 100, 119 * 100);
    let note = note_cents / 100;
    let frac = (note_cents - note * 100).clamp(0, 99) as u32;
    let r0 = square_rate(note) as u32;
    let r1 = square_rate(note + 1) as u32;
    (r0 + (((r1 as i32 - r0 as i32) as u32 * frac + 50) / 100)) as u16
}

/// Wave-channel rate from pitch in cents (see [`square_rate_cents`]).
pub fn wave_rate_cents(note_cents: i32) -> u16 {
    let note_cents = note_cents.clamp(12 * 100, 119 * 100);
    let note = note_cents / 100;
    let frac = (note_cents - note * 100).clamp(0, 99) as u32;
    let r0 = wave_rate(note) as u32;
    let r1 = wave_rate(note + 1) as u32;
    (r0 + (((r1 as i32 - r0 as i32) as u32 * frac + 50) / 100)) as u16
}

/// Mid-note square pitch — frequency register **without** `RESTART` (vibrato / arp / pitch-drop).
pub fn set_square_rate(ch: u8, rate: u16) {
    let control = rate & 0x07FF;
    match ch {
        1 => w16(SOUND1CNT_X, control),
        2 => w16(SOUND2CNT_H, control),
        _ => {}
    }
}

/// Mid-note wave pitch (no restart).
pub fn set_wave_rate(rate: u16) {
    w16(SOUND3CNT_X, rate & 0x07FF);
}

/// Mid-note noise divisor (no restart). `narrow` selects the 7-bit LFSR.
pub fn set_noise_div(shift: u8, ratio: u8, narrow: bool) {
    let freq =
        ((ratio as u16) & 7) | (if narrow { 1 << 3 } else { 0 }) | (((shift as u16) & 0xF) << 4);
    w16(SOUND4CNT_H, freq);
}

/// Mid-note square volume / duty without restarting. Soft ADSR owns the gate.
pub fn set_square_vol(ch: u8, duty: u8, vol: u8) {
    let envelope = (((duty as u16) & 3) << 6) | (((vol as u16) & 0xF) << 12);
    match ch {
        1 => w16(SOUND1CNT_H, envelope),
        2 => w16(SOUND2CNT_L, envelope),
        _ => {}
    }
}

/// Mid-note wave volume divider (0 silent, 1 full, 2 half, 3 quarter) without restart.
pub fn set_wave_vol(vol: u8) {
    let cur = r16(SOUND3CNT_H);
    let len = cur & 0xFF;
    w16(SOUND3CNT_H, len | (((vol as u16) & 3) << 13));
}

/// Mid-note noise volume without restart.
pub fn set_noise_vol(vol: u8) {
    w16(SOUND4CNT_L, ((vol as u16) & 0xF) << 12);
}

/// Power on the PSG and route every channel to both speakers.
///
/// Must run *after* agb's mixer is constructed. agb writes `SOUNDCNT_H` wholesale when it enables
/// Direct Sound, which zeroes bits 0-1 and leaves the PSG mixed in at 25%; we read-modify-write them
/// back to 100% here. `SOUNDCNT_L` powers up routing nothing anywhere, so without this every note
/// plays into silence — which is a genuinely confusing failure, since the channel is running and the
/// registers read back exactly as written.
pub fn init() {
    w16(SOUNDCNT_X, r16(SOUNDCNT_X) | 0x80); // master enable (agb sets this too; harmless to repeat)
                                             // bits 0-2 right volume, 4-6 left volume, 8-11 right enables ch1-4, 12-15 left enables ch1-4.
    w16(SOUNDCNT_L, 0xFF77);
    w16(SOUNDCNT_H, (r16(SOUNDCNT_H) & !0x0003) | 0x0002); // PSG at 100%, Direct Sound bits intact
}

/// PSG master volume, 0..=7 on each side, preserving the per-channel routing enables in the high
/// byte. This is the duck's lever: it attenuates every PSG voice at once without touching a single
/// note's envelope, so a duck cannot desync from the music the way per-voice scaling can.
///
/// ⚠️ `SOUNDCNT_H` is NOT touched here. Its low two bits are the PSG/DirectSound balance and agb
/// writes that register wholesale when it builds a Mixer — a duck implemented there disappears the
/// next time one is constructed.
pub fn master(vol: u8) {
    let v = vol.min(7) as u16;
    w16(SOUNDCNT_L, 0xFF00 | (v << 4) | v);
}

/// Silence one channel (1-4) by zeroing its envelope. Zero volume is used rather than the master
/// enable bits so that stopping one voice can never disturb another.
pub fn stop(ch: u8) {
    match ch {
        1 => w16(SOUND1CNT_H, 0),
        2 => w16(SOUND2CNT_L, 0),
        3 => w16(SOUND3CNT_L, 0), // ch3 has no envelope; disable the channel outright
        4 => w16(SOUND4CNT_L, 0),
        _ => {}
    }
}

/// Silence everything. Used when music stops so a held note doesn't hang under the next scene.
pub fn stop_all() {
    for ch in 1..=4 {
        stop(ch);
    }
}

/// Play a note on square channel 1 or 2.
///
/// * `duty` 0-3 selects the pulse width (12.5% / 25% / 50% / 75%) — the timbre knob.
/// * `vol` 0-15 is the envelope's starting volume.
/// * `decay` 0-7 is the envelope step: 0 sustains forever, 1 is a fast pluck, 7 a slow fade.
/// * `len` 0-63 is the hardware length counter; 0 means "sustain until told otherwise".
/// * `env_up` — envelope direction (false = decay toward 0, true = amplify toward 15).
pub fn square(ch: u8, note: i32, duty: u8, vol: u8, decay: u8, len: u8, env_up: bool) {
    let rate = square_rate(note);
    // Low byte: bits 0-5 length, 6-7 duty. High byte: bits 0-2 envelope step, 3 direction
    // (0 = decay, 1 = amplify), 4-7 initial volume.
    let envelope = ((len as u16) & 0x3F)
        | (((duty as u16) & 3) << 6)
        | (((decay as u16) & 7) << 8)
        | (if env_up { 1 << 11 } else { 0 })
        | (((vol as u16) & 0xF) << 12);
    let control = (rate & 0x07FF) | RESTART | if len > 0 { USE_LENGTH } else { 0 };
    match ch {
        1 => {
            w16(SOUND1CNT_L, 0); // no sweep — `slide` is the channel-1-only version of this call
            w16(SOUND1CNT_H, envelope);
            w16(SOUND1CNT_X, control);
        }
        2 => {
            w16(SOUND2CNT_L, envelope);
            w16(SOUND2CNT_H, control);
        }
        _ => {}
    }
}

/// Channel 1 only: a square note whose pitch sweeps in hardware. This is the classic arcade
/// coin/jump/laser, and the reason it is worth a separate entry point is that the sweep costs
/// nothing — the hardware walks the frequency itself, so a rising blip is exactly as cheap as a
/// flat one.
///
/// * `shift` 1-7 is the size of each pitch step (larger = wider sweep); 0 disables the sweep.
/// * `period` 1-7 is the time between steps; 0 also disables.
/// * `down` sweeps the pitch downward instead of up.
/// * `env_up` — envelope direction (see [`square`]).
pub fn slide(
    note: i32,
    duty: u8,
    vol: u8,
    decay: u8,
    len: u8,
    shift: u8,
    period: u8,
    down: bool,
    env_up: bool,
) {
    let rate = square_rate(note);
    let sweep =
        ((shift as u16) & 7) | (if down { 1 << 3 } else { 0 }) | (((period as u16) & 7) << 4);
    let envelope = ((len as u16) & 0x3F)
        | (((duty as u16) & 3) << 6)
        | (((decay as u16) & 7) << 8)
        | (if env_up { 1 << 11 } else { 0 })
        | (((vol as u16) & 0xF) << 12);
    w16(SOUND1CNT_L, sweep);
    w16(SOUND1CNT_H, envelope);
    w16(
        SOUND1CNT_X,
        (rate & 0x07FF) | RESTART | if len > 0 { USE_LENGTH } else { 0 },
    );
}

/// Load channel 3's wavetable: 32 4-bit samples packed into 16 bytes, high nibble first.
///
/// Bit 6 of `SOUND3CNT_L` selects the bank being **played**, and the CPU sees the *other* one at
/// `WAVE_RAM`. So filling bank 0 means selecting bank 1 here, and [`wave`] then selects bank 0 to
/// play it back. Getting this backwards does not fail loudly — the channel runs, the registers read
/// back exactly as written, and it plays whatever happened to be in the bank you didn't write.
pub fn wave_table(samples: &[u8; 16]) {
    w16(SOUND3CNT_L, 1 << 6); // channel off, play-bank 1 => CPU writes bank 0
    for (i, chunk) in samples.as_chunks::<2>().0.iter().enumerate() {
        w16(
            WAVE_RAM + i * 2,
            (chunk[0] as u16) | ((chunk[1] as u16) << 8),
        );
    }
}

/// Play a note on the wavetable channel.
///
/// `vol` here is not a 0-15 envelope — channel 3 has no envelope, only a 4-step volume divider:
/// 0 = silent, 1 = full, 2 = half, 3 = quarter. It is passed through as-is rather than remapped,
/// because pretending it is a smooth control makes instrument definitions lie about what they sound
/// like.
pub fn wave(note: i32, vol: u8, len: u8) {
    w16(SOUND3CNT_L, 0x0080); // channel on, bank 0, 32-sample mode
    w16(
        SOUND3CNT_H,
        ((len as u16) & 0xFF) | (((vol as u16) & 3) << 13),
    );
    let rate = wave_rate(note);
    w16(
        SOUND3CNT_X,
        (rate & 0x07FF) | RESTART | if len > 0 { USE_LENGTH } else { 0 },
    );
}

/// Play the noise channel — percussion, explosions, and every "hit" a game ever needs.
///
/// * `shift` 0-13 sets the pitch (lower = brighter/hissier, higher = deeper rumble).
/// * `ratio` 0-7 is a fine divisor under the shift.
/// * `narrow` switches the LFSR to 7-bit, which turns hiss into a metallic buzz.
/// * `env_up` — envelope direction (see [`square`]).
pub fn noise(vol: u8, decay: u8, len: u8, shift: u8, ratio: u8, narrow: bool, env_up: bool) {
    let envelope = ((len as u16) & 0x3F)
        | (((decay as u16) & 7) << 8)
        | (if env_up { 1 << 11 } else { 0 })
        | (((vol as u16) & 0xF) << 12);
    let freq =
        ((ratio as u16) & 7) | (if narrow { 1 << 3 } else { 0 }) | (((shift as u16) & 0xF) << 4);
    w16(SOUND4CNT_L, envelope);
    w16(
        SOUND4CNT_H,
        freq | RESTART | if len > 0 { USE_LENGTH } else { 0 },
    );
}
