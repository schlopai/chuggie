//! Compile a `.chip` song — a small readable tracker text — into static Rust data at build time.
//!
//! Nothing here runs on the GBA. The device sees an array of note bytes and a handful of instrument
//! structs; all the parsing, note-name lookup and validation happens on the host, which is the whole
//! point: a song costs the ROM its notes and nothing else.
//!
//! The format, by example:
//!
//! ```text
//! tempo 8                 # frames per row; 8 => ~7.5 rows/second
//! loop  16                # row to return to at the end (default 0)
//!
//! wave tri 0123456789ABCDEFFEDCBA9876543210
//!
//! inst lead square duty=2 vol=11 decay=0
//! inst bass wave   table=tri vol=1
//! inst drum noise  vol=10 decay=3 shift=5
//!
//! ch1 lead | C5 .  E5 .  | G5 .  E5 .  |
//! ch2 lead | C4 .  C4 .  | G3 .  G3 .  |
//! ch3 bass | C3 .  .  .  | G2 .  .  .  |
//! ch4 drum | x  .  x  .  | x  .  x  .  |
//! ```
//!
//! Row tokens are `.` (hold), `-` (note off), `x` (trigger, for the noise channel) or a note name
//! (`C4`, `C#4`, `Db5`). `|` is decoration and ignored. Repeated `chN` lines append, so a song is
//! written a bar at a time.

use quote::quote;
use std::collections::HashMap;
use std::path::Path;

const HOLD: u8 = 0;
const OFF: u8 = 1;

struct Instrument {
    kind: u8, // 0 square, 1 wave, 2 noise
    duty: u8,
    vol: u8,
    decay: u8,
    len: u8,
    wave: u8,
    shift: u8,
}

/// `C`, `C#`/`Db`, ... to a semitone offset. Both spellings are accepted because insisting on one
/// makes transcribing from anything else an exercise in mental arithmetic.
fn semitone(name: &str) -> Option<i32> {
    let bytes = name.as_bytes();
    let base = match bytes.first()?.to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    Some(match bytes.get(1) {
        Some(b'#') => base + 1,
        Some(b'b') => base - 1,
        _ => base,
    })
}

/// `A4` / `C#3` / `Db5` to a MIDI note number (60 = C4).
fn parse_note(tok: &str) -> Option<u8> {
    let split = tok.find(|c: char| c.is_ascii_digit() || c == '-')?;
    let (name, octave) = tok.split_at(split);
    let semis = semitone(name)?;
    let octave: i32 = octave.parse().ok()?;
    let midi = (octave + 1) * 12 + semis;
    // 2 is the first value that isn't HOLD or OFF; 127 is MIDI's ceiling.
    if (2..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

/// Parse `key=value` pairs off an instrument line into a lookup.
fn kv(parts: &[&str]) -> HashMap<String, String> {
    parts
        .iter()
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn num<T: std::str::FromStr>(map: &HashMap<String, String>, key: &str, default: T) -> T {
    map.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Strip a trailing `#` comment — but only where `#` *starts* a token, because `G#4` is a note and
/// splitting the line on the first `#` anywhere turns every sharp in the song into a truncated line.
fn strip_comment(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            return &raw[..i];
        }
    }
    raw
}

pub fn build(path: &Path) -> Result<proc_macro2::TokenStream, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut tempo: u8 = 8;
    let mut loop_row: u16 = 0;
    let mut waves: Vec<[u8; 16]> = Vec::new();
    let mut wave_ids: HashMap<String, u8> = HashMap::new();
    let mut insts: HashMap<String, Instrument> = HashMap::new();
    // Per channel: the instrument named by its first `chN` line, and the accumulated note rows.
    let mut ch_inst: [Option<String>; 4] = [None, None, None, None];
    let mut ch_notes: [Vec<u8>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];

    for (lineno, raw) in src.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let where_ = || format!("{}:{}", path.display(), lineno + 1);
        let parts: Vec<&str> = line.split_whitespace().collect();

        match parts[0] {
            "tempo" => {
                tempo = parts
                    .get(1)
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| format!("{}: tempo needs a frame count", where_()))?;
                if tempo == 0 {
                    return Err(format!("{}: tempo 0 would never advance a row", where_()));
                }
            }
            "loop" => {
                loop_row = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "wave" => {
                let name = parts
                    .get(1)
                    .ok_or_else(|| format!("{}: wave needs a name", where_()))?;
                let digits = parts.get(2).copied().unwrap_or("");
                if digits.len() != 32 {
                    return Err(format!(
                        "{}: wave '{}' has {} steps, needs exactly 32 hex digits",
                        where_(),
                        name,
                        digits.len()
                    ));
                }
                let mut table = [0u8; 16];
                for (i, byte) in table.iter_mut().enumerate() {
                    let hi = u8::from_str_radix(&digits[i * 2..i * 2 + 1], 16)
                        .map_err(|_| format!("{}: wave '{}' is not hex", where_(), name))?;
                    let lo = u8::from_str_radix(&digits[i * 2 + 1..i * 2 + 2], 16)
                        .map_err(|_| format!("{}: wave '{}' is not hex", where_(), name))?;
                    *byte = (hi << 4) | lo;
                }
                wave_ids.insert(name.to_string(), waves.len() as u8);
                waves.push(table);
            }
            "inst" => {
                let name = parts
                    .get(1)
                    .ok_or_else(|| format!("{}: inst needs a name", where_()))?;
                let kind_s = parts.get(2).copied().unwrap_or("square");
                let kind = match kind_s {
                    "square" => 0,
                    "wave" => 1,
                    "noise" => 2,
                    other => return Err(format!("{}: unknown voice '{}'", where_(), other)),
                };
                let opts = kv(&parts[3..]);
                let wave = match opts.get("table") {
                    Some(t) => *wave_ids.get(t).ok_or_else(|| {
                        format!("{}: inst '{}' uses undefined wave '{}'", where_(), name, t)
                    })?,
                    None => 0,
                };
                insts.insert(
                    name.to_string(),
                    Instrument {
                        kind,
                        duty: num(&opts, "duty", 2u8).min(3),
                        vol: num(&opts, "vol", if kind == 1 { 1 } else { 12 }),
                        decay: num(&opts, "decay", 0u8).min(7),
                        len: num(&opts, "len", 0u8).min(63),
                        wave,
                        shift: num(&opts, "shift", 4u8).min(13),
                    },
                );
            }
            tag if tag.starts_with("ch") && tag.len() == 3 => {
                let ch = tag.as_bytes()[2] - b'1';
                if ch > 3 {
                    return Err(format!("{}: '{}' — channels are ch1..ch4", where_(), tag));
                }
                let ch = ch as usize;
                let mut rest = &parts[1..];
                // The instrument name is optional after the first line for this channel, so bars can
                // be written as bare rows once the voice is established.
                if let Some(first) = rest.first() {
                    if insts.contains_key(*first) {
                        if ch_inst[ch].is_none() {
                            ch_inst[ch] = Some(first.to_string());
                        }
                        rest = &rest[1..];
                    }
                }
                if ch_inst[ch].is_none() {
                    return Err(format!(
                        "{}: {} has no instrument — name one on its first line",
                        where_(),
                        tag
                    ));
                }
                for tok in rest {
                    match *tok {
                        "|" => {}
                        "." => ch_notes[ch].push(HOLD),
                        "-" => ch_notes[ch].push(OFF),
                        "x" => ch_notes[ch].push(60), // noise ignores pitch; any note triggers it
                        note => {
                            let n = parse_note(note).ok_or_else(|| {
                                format!("{}: '{}' is not a note (try C4, F#3, Bb5)", where_(), note)
                            })?;
                            ch_notes[ch].push(n);
                        }
                    }
                }
            }
            other => return Err(format!("{}: don't understand '{}'", where_(), other)),
        }
    }

    // Ragged channels are the bug this format exists to make impossible: one bar missing from the
    // bass drifts the whole arrangement apart, progressively, and is very hard to hear as "row 47 is
    // missing" rather than "this song is bad".
    let rows = ch_notes.iter().map(|n| n.len()).max().unwrap_or(0);
    for (i, notes) in ch_notes.iter().enumerate() {
        if !notes.is_empty() && notes.len() != rows {
            return Err(format!(
                "{}: ch{} is {} rows but the song is {} — every channel must be the same length",
                path.display(),
                i + 1,
                notes.len(),
                rows
            ));
        }
    }
    if rows == 0 {
        return Err(format!("{}: no rows — the song is empty", path.display()));
    }
    if loop_row as usize >= rows {
        return Err(format!(
            "{}: loop row {} is past the end ({} rows)",
            path.display(),
            loop_row,
            rows
        ));
    }

    // A channel with no rows still needs a track; give it silence so the runtime array stays [_; 4].
    let mut track_toks = Vec::new();
    for ch in 0..4 {
        let silent = Instrument {
            kind: if ch == 3 {
                2
            } else if ch == 2 {
                1
            } else {
                0
            },
            duty: 2,
            vol: 0,
            decay: 0,
            len: 0,
            wave: 0,
            shift: 4,
        };
        let inst = ch_inst[ch]
            .as_ref()
            .and_then(|n| insts.get(n))
            .unwrap_or(&silent);
        let (kind, duty, vol, decay, len, wave, shift) = (
            inst.kind, inst.duty, inst.vol, inst.decay, inst.len, inst.wave, inst.shift,
        );
        let notes = &ch_notes[ch];
        let notes = if notes.is_empty() {
            vec![HOLD; rows]
        } else {
            notes.clone()
        };
        track_toks.push(quote! {
            tish_agb::chiptune::Track {
                inst: tish_agb::chiptune::Instrument {
                    kind: #kind, duty: #duty, vol: #vol, decay: #decay,
                    len: #len, wave: #wave, shift: #shift,
                },
                notes: &[#(#notes),*],
            }
        });
    }

    let wave_toks = waves.iter().map(|w| {
        let bytes = w.iter().copied();
        quote! { [#(#bytes),*] }
    });
    let rows_u16 = rows as u16;

    Ok(quote! {
        pub static SONG: tish_agb::chiptune::Song = tish_agb::chiptune::Song {
            frames_per_row: #tempo,
            loop_row: #loop_row,
            rows: #rows_u16,
            tracks: [#(#track_toks),*],
            waves: &[#(#wave_toks),*],
        };
        pub fn __chip_register() -> i32 {
            tish_agb::native_song_register(&SONG)
        }
    })
}
