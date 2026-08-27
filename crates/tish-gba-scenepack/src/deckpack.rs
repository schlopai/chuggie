//! Compile a `.deck` (GBA engines only) into static `DeckSong` ROM data.
//!
//! Accepts `gameBoyDmg` (LR35902 / PSG) and `gbaDirectSound` (GBA PCM). Everything else is a
//! compile error that names the track and generator — see `docs/deck.md`.

use quote::quote;
use std::collections::HashMap;
use std::path::Path;

const FPS_NUM: i64 = 597_275; // 59.7275 * 10000
const FPS_DEN: i64 = 10_000;

const VOICE_PULSE: u8 = 0;
const VOICE_WAVE: u8 = 1;
const VOICE_NOISE: u8 = 2;
const VOICE_DS_PCM: u8 = 3;
/// A real recorded instrument, as opposed to `VOICE_DS_PCM`'s generated 32-byte wavetable.
///
/// These are two different things and deliberately two different kinds. `VOICE_DS_PCM` synthesises
/// a single cycle of a saw/sine/pulse and loops it forever — a synth that happens to run through
/// the mixer. `VOICE_DS_SAMPLE` plays sampled audio with its own rate, root key, loop point and
/// key zones. Overloading kind 3 would have quietly changed what every existing `gbaDirectSound`
/// param means.
const VOICE_DS_SAMPLE: u8 = 4;

/// agb's mixer has 8 channels in total, and sound effects have to come from the same eight. Six
/// for music is the most that leaves room for a game; whether six actually *fits the frame* is a
/// separate question a music example's verify.sh soak answers, not this constant.
const MAX_PCM_VOICES: u8 = 6;

/// Must match `MAX_TRACKS` in `crates/tish-agb/src/deck_player.rs`.
///
/// ⚠️ The player does `song.tracks.iter().take(MAX_TRACKS)`, so a 9-track song does not fail — it
/// silently loses its last stem. Two imported sample-based decks were doing exactly that. Erroring here turns a
/// missing instrument nobody notices into a build error that names the track.
const MAX_TRACKS: usize = 8;

const ENV_STEP: u8 = 0;
const ENV_CONST: u8 = 1;
const ENV_ADSR: u8 = 2;

const FLAG_NOISE_NARROW: u8 = 1;
const FLAG_BITCRUSH: u8 = 2;
const FLAG_ENV_UP: u8 = 4;
const FLAG_SWEEP_DOWN: u8 = 8;
const FLAG_NOISE_SHIFT_SET: u8 = 16;

#[derive(Clone)]
struct VoiceParams {
    gen: String,
    // DMG
    type_: String,
    duty: String,
    env_mode: String,
    vol: u8,
    noise_mode: String,
    wave_shape: String,
    // PCM
    waveform: String,
    bitcrush: bool,
    // Sampled instrument: which `sampleset` program this track plays, if any.
    program: Option<i64>,
    /// Stereo position, -64..63, 0 = centre.
    pan: i8,
    // shared
    attack_s: f64,
    decay_s: f64,
    sustain: u8,
    release_s: f64,
    vib_rate: f64,
    vib_amt: f64,
    arp_rate: f64,
    arp_semis: i8,
    drop_semis: i8,
    drop_dec_s: f64,
    mix_gain: f64,
    // HW PSG surface
    len: u8,
    env_step: u8,
    env_up: bool,
    soft_sweep: i8,
    sweep_shift: u8,
    sweep_period: u8,
    sweep_down: bool,
    noise_shift: Option<u8>,
    noise_ratio: u8,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            gen: String::new(),
            type_: "pulse".into(),
            duty: "50".into(),
            env_mode: "step".into(),
            vol: 15,
            noise_mode: "long".into(),
            wave_shape: "saw".into(),
            waveform: "pulse".into(),
            bitcrush: true,
            program: None,
            pan: 0,
            attack_s: 0.0,
            decay_s: 0.0,
            sustain: 15,
            release_s: 0.0,
            vib_rate: 0.0,
            vib_amt: 0.0,
            arp_rate: 0.0,
            arp_semis: 0,
            drop_semis: 0,
            drop_dec_s: 0.05,
            mix_gain: 1.0,
            len: 0,
            env_step: 0,
            env_up: false,
            soft_sweep: 0,
            sweep_shift: 0,
            sweep_period: 0,
            sweep_down: false,
            noise_shift: None,
            noise_ratio: 0,
        }
    }
}

#[derive(Clone)]
struct Note {
    start_beat: f64,
    dur_beat: f64,
    midi: u8,
    vel: u8,
}

struct TrackIn {
    id: String,
    #[allow(dead_code)]
    name: String,
    bars: u32,
    /// Stem audible when player intensity >= this (0 = always). crossfading-stem-style layers.
    min_intensity: u8,
    params: VoiceParams,
    notes: Vec<Note>,
    steps: Option<[bool; 16]>,
    step_pitch: u8,
}

fn beat_to_frames(beat: f64, bpm: f64) -> u16 {
    // frames = beat * 60 * fps / bpm
    let frames = beat * 60.0 * (FPS_NUM as f64 / FPS_DEN as f64) / bpm;
    frames.round().clamp(0.0, 65535.0) as u16
}

fn secs_to_frames(s: f64) -> u8 {
    let f = s * (FPS_NUM as f64 / FPS_DEN as f64);
    f.round().clamp(0.0, 255.0) as u8
}

fn duty_code(d: &str) -> u8 {
    match d {
        "12_5" | "12.5" => 0,
        "25" => 1,
        "75" => 3,
        _ => 2, // 50
    }
}

fn env_code(m: &str) -> u8 {
    match m {
        "constant" | "const" => ENV_CONST,
        "adsr" => ENV_ADSR,
        _ => ENV_STEP,
    }
}

fn wave_nibbles(shape: &str) -> [u8; 16] {
    let mut nibs = [0u8; 32];
    for (i, nib) in nibs.iter_mut().enumerate() {
        let phase = i as f64 / 32.0;
        let v = match shape {
            "square" => {
                if phase < 0.5 {
                    15
                } else {
                    0
                }
            }
            "sine" => {
                let s = (phase * core::f64::consts::PI * 2.0).sin();
                ((s * 7.5) + 7.5).round().clamp(0.0, 15.0) as u8
            }
            _ => {
                // saw
                (phase * 15.0).round().clamp(0.0, 15.0) as u8
            }
        };
        *nib = v;
    }
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[i] = (nibs[i * 2] << 4) | nibs[i * 2 + 1];
    }
    out
}

fn pcm_table(waveform: &str, duty: &str, bitcrush: bool) -> Vec<u8> {
    let mut samples = Vec::with_capacity(32);
    let thresh = match duty {
        "12_5" | "12.5" => 0.125,
        "25" => 0.25,
        "75" => 0.75,
        _ => 0.5,
    };
    for i in 0..32 {
        let phase = i as f64 / 32.0;
        let mut v = match waveform {
            "sawtooth" | "saw" => phase * 2.0 - 1.0,
            "triangle" => {
                if phase < 0.5 {
                    phase * 4.0 - 1.0
                } else {
                    3.0 - phase * 4.0
                }
            }
            "sine" => (phase * core::f64::consts::PI * 2.0).sin(),
            _ => {
                // pulse
                if phase < thresh {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        if bitcrush {
            v = (v * 127.0).round() / 127.0;
        }
        // ⚠️ SIGNED 8-bit. agb's mixer loads samples with `ldrsb` (`agb/src/sound/mixer/mixer.s`),
        // so a sample is two's-complement -128..127, not offset-binary 0..255.
        //
        // This used to emit `v * 127 + 128`. Read back as signed, every value above +0 wrapped to
        // negative: a sine came out with a 231-step discontinuity at each zero crossing (on a
        // 255-wide scale) and 297% THD against 53.5% for the correct encoding. That is audible as
        // a buzz layered under every synthesised `gbaDirectSound` voice — the "where is this
        // static coming from" bug, and it was never the samples, it was the wavetable.
        let s = (v * 127.0).round().clamp(-128.0, 127.0) as i8;
        samples.push(s as u8);
    }
    // pad to 4-byte alignment with silence (0 signed, NOT 128 — see above)
    while samples.len() % 4 != 0 {
        samples.push(0);
    }
    samples
}

/// `0..=3`, or a build error naming the range.
fn clamp_layer(v: i64, file: &std::path::Display) -> Result<u8, String> {
    if !(0..=3).contains(&v) {
        return Err(format!("{file}: layer/intensity {v} out of range (0..3)"));
    }
    Ok(v as u8)
}

/// Feed a parsed `gen` line into `VoiceParams`.
///
/// The shared parser hands back typed key/value pairs with keys already camelCased, and
/// `parse_gen_kvs` below already accepts both spellings — so this flattens back to string pairs and
/// reuses it verbatim rather than restating thirty field mappings (and their truthiness quirks,
/// like `bitcrush 16bit` meaning false) in a second place.
fn apply_gen_params(params: &deckfile::Params, out: &mut VoiceParams) {
    let mut owned: Vec<(String, String)> = Vec::new();
    for (k, v) in &params.numbers {
        // Integral values must stringify without a `.0`, or `duty 50` becomes `"50.0"` and misses
        // every arm of the match below.
        let s = if v.fract() == 0.0 && v.abs() < 1e15 {
            format!("{}", *v as i64)
        } else {
            format!("{v}")
        };
        owned.push((k.clone(), s));
    }
    for (k, v) in &params.strings {
        owned.push((k.clone(), v.clone()));
    }
    let flat: Vec<&str> = owned
        .iter()
        .flat_map(|(k, v)| [k.as_str(), v.as_str()])
        .collect();
    parse_gen_kvs(&flat, out);
}

// ── `sampleset`: real recorded instruments ─────────────────────────────────────────────────────

/// One key zone of a sampled instrument: which sample to play, and over which keys.
struct Zone {
    lo: u8,
    hi: u8,
    sample: String,
    root: u8,
    /// M4A voice type `0x08`: play at the sample's own rate whatever key is struck. Percussion is
    /// voiced this way, and ignoring the flag pitch-shifts a drum kit into nonsense.
    fixed: bool,
    /// A FIXED pan from the voicegroup, overriding the track's. `None` = use the track's.
    pan: Option<i8>,
    a: u8,
    d: u8,
    s: u8,
    r: u8,
}

struct Program {
    kind: String,
    zones: Vec<Zone>,
}

/// A conditioned sample on disk: raw signed 8-bit PCM plus the sidecar that says how to play it.
struct SampleFile {
    /// Absolute path, so the emitted `include_bytes!` does not depend on where rustc is invoked.
    path: std::path::PathBuf,
    len: usize,
    rate: u32,
    /// `u32::MAX` for a one-shot. The distinction matters: looping a one-shot drum makes it a
    /// buzz, and *not* looping a sustained string makes it a pluck.
    loop_start: u32,
}

fn json_u(v: &serde_json::Value, k: &str, default: u64) -> u64 {
    v.get(k).and_then(|x| x.as_u64()).unwrap_or(default)
}

/// Load `vgNNN.json` + the `s<hash>.bin`/`s<hash>.json` pairs it references.
fn load_sampleset(
    dir: &Path,
    path: &Path,
    samples: &mut Vec<(String, SampleFile)>,
) -> Result<HashMap<i64, Program>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "{}: {e} — sample banks are not committed; build one with \
             a voicegroup-extract tool writing M4A-shaped JSON next to the deck",
            path.display()
        )
    })?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut out = HashMap::new();
    let progs = doc
        .get("programs")
        .and_then(|p| p.as_array())
        .ok_or_else(|| format!("{}: no `programs` array", path.display()))?;

    for p in progs {
        let num = p.get("program").and_then(|x| x.as_i64()).unwrap_or(-1);
        let kind = p
            .get("kind")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_string();
        let mut zones = Vec::new();
        for z in p
            .get("zones")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            let name = match z.get("sample").and_then(|x| x.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if !samples.iter().any(|(n, _)| *n == name) {
                let bin = dir.join(format!("{name}.bin"));
                let meta_path = dir.join(format!("{name}.json"));
                let bytes = std::fs::read(&bin).map_err(|e| format!("{}: {e}", bin.display()))?;
                let meta: serde_json::Value = serde_json::from_str(
                    &std::fs::read_to_string(&meta_path)
                        .map_err(|e| format!("{}: {e}", meta_path.display()))?,
                )
                .map_err(|e| format!("{}: {e}", meta_path.display()))?;
                let looped = meta
                    .get("looped")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false);
                let loop_start = if looped {
                    json_u(&meta, "loop_start", 0) as u32
                } else {
                    u32::MAX
                };
                if looped && loop_start as usize >= bytes.len() {
                    return Err(format!(
                        "{}: loop_start {loop_start} is past the {} byte sample",
                        meta_path.display(),
                        bytes.len()
                    ));
                }
                let rate = meta
                    .get("rate")
                    .and_then(|x| x.as_f64())
                    .unwrap_or(10512.0)
                    .round()
                    .clamp(1.0, 1_000_000.0) as u32;
                samples.push((
                    name.clone(),
                    SampleFile {
                        path: std::fs::canonicalize(&bin).unwrap_or(bin),
                        len: bytes.len(),
                        rate,
                        loop_start,
                    },
                ));
            }
            let adsr = z.get("adsr").cloned().unwrap_or(serde_json::Value::Null);
            zones.push(Zone {
                lo: json_u(z, "lo", 0).min(127) as u8,
                hi: json_u(z, "hi", 127).min(127) as u8,
                sample: name,
                root: json_u(z, "root", 60).min(127) as u8,
                fixed: z.get("fixed").and_then(|x| x.as_bool()).unwrap_or(false),
                pan: z
                    .get("pan")
                    .and_then(|x| x.as_i64())
                    .map(|v| v.clamp(-64, 63) as i8),
                a: json_u(&adsr, "a", 0).min(255) as u8,
                d: json_u(&adsr, "d", 0).min(255) as u8,
                s: json_u(&adsr, "s", 15).min(15) as u8,
                r: json_u(&adsr, "r", 0).min(255) as u8,
            });
        }
        out.insert(num, Program { kind, zones });
    }
    Ok(out)
}

fn parse_gen_kvs(parts: &[&str], p: &mut VoiceParams) {
    let mut i = 0;
    while i + 1 < parts.len() {
        let k = parts[i];
        let v = parts[i + 1];
        match k {
            "type" => p.type_ = v.to_string(),
            "duty" => p.duty = v.to_string(),
            "env_mode" | "envMode" => p.env_mode = v.to_string(),
            "vol" => p.vol = v.parse().unwrap_or(p.vol),
            "noise_mode" | "noiseMode" => p.noise_mode = v.to_string(),
            "wave_shape" | "waveShape" => p.wave_shape = v.to_string(),
            "waveform" => p.waveform = v.to_string(),
            "program" => p.program = v.parse().ok(),
            "pan" => p.pan = v.parse().unwrap_or(p.pan),
            "bitcrush" => {
                p.bitcrush = v != "false" && v != "0" && v != "16bit";
            }
            "attack" => p.attack_s = v.parse().unwrap_or(p.attack_s),
            "decay" => p.decay_s = v.parse().unwrap_or(p.decay_s),
            "sustain" => p.sustain = v.parse().unwrap_or(p.sustain),
            "release" => p.release_s = v.parse().unwrap_or(p.release_s),
            "vib_rate" | "vibRate" => p.vib_rate = v.parse().unwrap_or(p.vib_rate),
            "vib_amt" | "vibAmt" => p.vib_amt = v.parse().unwrap_or(p.vib_amt),
            "arp_rate" | "arpRate" => p.arp_rate = v.parse().unwrap_or(p.arp_rate),
            "arp_semis" | "arpSemis" => p.arp_semis = v.parse().unwrap_or(p.arp_semis),
            "pitch_drop" | "pitchDrop" => p.drop_semis = v.parse().unwrap_or(p.drop_semis),
            "pitch_dec" | "pitchDec" => p.drop_dec_s = v.parse().unwrap_or(p.drop_dec_s),
            "len" | "length" => p.len = v.parse().unwrap_or(p.len),
            "env_step" | "envStep" => p.env_step = v.parse().unwrap_or(p.env_step),
            "env_up" | "envUp" => {
                p.env_up = v == "1" || v == "true" || v == "up" || v == "amplify";
            }
            "sweep" => p.soft_sweep = v.parse().unwrap_or(p.soft_sweep),
            "sweep_shift" | "sweepShift" => {
                p.sweep_shift = v.parse().unwrap_or(p.sweep_shift);
            }
            "sweep_period" | "sweepPeriod" => {
                p.sweep_period = v.parse().unwrap_or(p.sweep_period);
            }
            "sweep_down" | "sweepDown" => {
                p.sweep_down = v == "1" || v == "true" || v == "down";
            }
            "noise_shift" | "noiseShift" => {
                p.noise_shift = v.parse().ok();
            }
            "noise_ratio" | "noiseRatio" => {
                p.noise_ratio = v.parse().unwrap_or(p.noise_ratio);
            }
            _ => {}
        }
        i += 2;
    }
}

/// Register the two pieces of vocabulary the GBA bake adds to the core language.
///
/// These used to be a fork: `deckpack` had its own parser, so `wave` and `layer` simply existed in
/// its grammar and nowhere else. Registering them instead means the GBA dialect is a documented
/// *extension* of one shared language rather than a second one that drifts — everything else here
/// parses byte-identically to what the Deckard host sees.
///
/// Idempotent: registration overwrites, and the registry is per-thread, so calling this on every
/// macro expansion is correct and cheap.
fn register_gba_dialect() {
    use deckfile::Value;

    // `wave` used to be registered here. It is core grammar now — the language resolves both the hex
    // and `harmonics` spellings onto `program.waves` — so registering it would be ignored.

    // `sampleset <path>` — a voicegroup of real recorded instruments (zones, rates, loop points
    // and envelopes), as written by a voicegroup-extract tool (M4A-shaped JSON). The path is resolved
    // relative to the `.deck` file.
    deckfile::registerTopLevelStatement(
        Value::String("sampleset".into()),
        Value::native(|args: &[Value]| args.get(1).cloned().unwrap_or(Value::Null)),
    );

    // `layer|intensity|min_intensity <0..3>` — crossfading-stem-style stem gating.
    deckfile::registerBodyLineDialect(
        Value::Array(deckfile::runtime::VmRef::new(vec![
            Value::String("layer".into()),
            Value::String("intensity".into()),
            Value::String("min_intensity".into()),
        ])),
        Value::native(|args: &[Value]| {
            let toks = args.get(1).cloned().unwrap_or(Value::Null);
            Value::object_from_pairs([
                ("kind".into(), Value::String("layer".into())),
                ("level".into(), tok(&toks, 1)),
            ])
        }),
    );
}

/// `toks[i]` as a `Value`, or null.
fn tok(toks: &deckfile::Value, i: usize) -> deckfile::Value {
    match toks {
        deckfile::Value::Array(a) => a.borrow().get(i).cloned().unwrap_or(deckfile::Value::Null),
        _ => deckfile::Value::Null,
    }
}

fn tok_str(toks: &deckfile::Value, i: usize) -> Option<String> {
    match tok(toks, i) {
        deckfile::Value::String(s) => Some(s.to_string()),
        _ => None,
    }
}

/// A field off the raw AST — for the parts of the language the GBA subset only needs to *reject*,
/// which the typed facade does not model.
fn raw_field(program: &deckfile::DeckProgram, key: &str) -> deckfile::Value {
    deckfile::runtime::get_index(&program.raw, &deckfile::Value::String(key.into()))
}

fn raw_len(program: &deckfile::DeckProgram, key: &str) -> usize {
    match raw_field(program, key) {
        deckfile::Value::Array(a) => a.borrow().len(),
        deckfile::Value::Object(o) => o.borrow().strings.len(),
        _ => 0,
    }
}

/// Reject everything outside the GBA subset, as data rather than as control flow woven through a
/// parser.
///
/// Two things this buys over the old inline checks. It runs on the parsed AST, so an indented
/// `transpose` is a track-body row and is reported as one — the old head-match ran before the indent
/// test and rejected it as a top-level statement. And the subset is now a list you can read against
/// `conformance/profiles.json`, which is what stops it drifting from what the docs claim.
fn check_gba_subset(
    program: &deckfile::DeckProgram,
    file: &std::path::Display,
) -> Result<(), String> {
    let unsupported = |what: &str| -> String {
        format!("{file}: unsupported deck feature `{what}` on GBA — only gameBoyDmg/gbaDirectSound songs bake (see docs/deck.md)")
    };

    if program.swing.is_some() {
        return Err(unsupported("swing"));
    }
    if program.scale_root.is_some() {
        return Err(unsupported("scale"));
    }
    if !program.clips.is_empty() {
        return Err(unsupported("clip"));
    }
    if !program.directives.is_empty() {
        return Err(unsupported("@ directives"));
    }
    for (key, what) in [
        ("autos", "auto"),
        ("actorMixRows", "actor_mix"),
        ("sessionSlots", "session_slot"),
        ("removeTrackIds", "remove_track"),
        ("macros", "macro"),
    ] {
        if raw_len(program, key) > 0 {
            return Err(unsupported(what));
        }
    }
    if !matches!(raw_field(program, "masterMixTokens"), deckfile::Value::Null) {
        return Err(unsupported("master_mix"));
    }
    if !matches!(
        raw_field(program, "sessionSceneCount"),
        deckfile::Value::Null
    ) {
        return Err(unsupported("session_scenes"));
    }

    for tr in &program.tracks {
        if !tr.gen_blocks.is_empty() {
            return Err(unsupported("gen_block"));
        }
        for row in &tr.body {
            match row {
                deckfile::BodyRow::Fx { .. } => return Err(unsupported("fx")),
                deckfile::BodyRow::DeckRoute { .. } => return Err(unsupported("deck routing")),
                deckfile::BodyRow::Transpose { .. } => return Err(unsupported("transpose")),
                deckfile::BodyRow::Steps { euclid: true, .. } => {
                    return Err(format!(
                        "{file}: `steps euclid` is not supported on the GBA bake — expand it to note lines"
                    ))
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub fn build(path: &Path) -> Result<proc_macro2::TokenStream, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file = path.display();

    register_gba_dialect();
    let program = deckfile::parse(&src);

    // The parser is error-tolerant by design (a streaming host must be able to parse a partial
    // program); a build-time bake is not, so surface the first error rather than baking a song that
    // silently lost a line.
    if let Some(e) = program.errors.first() {
        return Err(format!("{file}:{}: {}", e.line, e.msg));
    }
    for tr in &program.tracks {
        if let Some(e) = tr.body_errors.first() {
            return Err(format!("{file}:{}: {}", e.line, e.msg));
        }
    }
    check_gba_subset(&program, &file)?;

    let bpm = program.bpm.unwrap_or(120.0);
    if !(40.0..=300.0).contains(&bpm) {
        return Err(format!("{file}: bpm {bpm} out of range 40..300"));
    }

    // `wave <name> <32 hex digits>` or `wave <name> harmonics <a1> …`. Both are core grammar now, and
    // the parser resolves either spelling to the same 32 levels — including the additive sum, which
    // has to happen here at bake time regardless: the device is no_std with no FPU, so there is no
    // sin() on the other side. All that is left is packing two levels per byte for wave RAM.
    let mut named_waves: HashMap<String, [u8; 16]> = HashMap::new();
    for w in &program.waves {
        if w.levels.len() != 32 {
            return Err(format!(
                "{file}: wave `{}` needs 32 levels, got {}",
                w.name,
                w.levels.len()
            ));
        }
        let mut packed = [0u8; 16];
        for i in 0..16 {
            packed[i] = ((w.levels[i * 2] as u8) << 4) | (w.levels[i * 2 + 1] as u8);
        }
        named_waves.insert(w.name.clone(), packed);
    }

    // `sampleset <path>` — the real instruments, if this song plays any.
    let deck_dir = path.parent().unwrap_or(Path::new("."));
    let mut sample_files: Vec<(String, SampleFile)> = Vec::new();
    let mut programs: HashMap<i64, Program> = HashMap::new();
    for entry in program.host_statement("sampleset") {
        let rel = tok_str(&entry, 1)
            .ok_or_else(|| format!("{file}: sampleset needs a path to a voicegroup json"))?;
        let p = deck_dir.join(&rel);
        // The extractor writes paths relative to the repo root, which is also how the decks read;
        // fall back to that so a deck is not tied to where it happens to sit.
        let p = if p.exists() {
            p
        } else {
            Path::new(&rel).to_path_buf()
        };
        programs.extend(load_sampleset(
            p.parent().unwrap_or(Path::new(".")),
            &p,
            &mut sample_files,
        )?);
    }

    let mut tracks: Vec<TrackIn> = Vec::new();
    for t in &program.tracks {
        if t.id.is_empty() {
            return Err(format!("{file}: track needs `id <id>`"));
        }
        if t.generator_id != "gameBoyDmg" && t.generator_id != "gbaDirectSound" {
            return Err(format!(
                "{file}: track `{}` gen `{}` is not supported on GBA — use gameBoyDmg (LR35902) or gbaDirectSound (GBA PCM)",
                t.id, t.generator_id
            ));
        }

        let mut params = VoiceParams {
            gen: t.generator_id.clone(),
            ..Default::default()
        };
        // `* inf` (no fixed length) has no meaning for a baked, finite song — treat it as one bar.
        let bars = t.loop_bars.unwrap_or(1).max(1) as u32;
        let mut min_intensity = 0u8;
        let mut notes: Vec<Note> = Vec::new();
        let mut steps: Option<[bool; 16]> = None;
        let mut step_pitch = 36u8;

        // A `layer` on the track header arrives as a header param, not a body row.
        if let Some(v) = t.gen_params.int("layer").or_else(|| {
            t.gen_params
                .int("intensity")
                .or_else(|| t.gen_params.int("minIntensity"))
        }) {
            min_intensity = clamp_layer(v, &file)?;
        }

        for row in &t.body {
            match row {
                deckfile::BodyRow::Gen { params: p } => apply_gen_params(p, &mut params),
                deckfile::BodyRow::Adsr { a, d, s, r } => {
                    if let Some(a) = a {
                        params.attack_s = *a;
                        params.env_mode = "adsr".into();
                    }
                    if let Some(d) = d {
                        params.decay_s = *d;
                    }
                    if let Some(s) = s {
                        // `s` is 0..1 or 0..15 — both spellings are in the wild.
                        params.sustain = if *s <= 1.0 {
                            (*s * 15.0).round() as u8
                        } else {
                            s.round().clamp(0.0, 15.0) as u8
                        };
                    }
                    if let Some(r) = r {
                        params.release_s = *r;
                    }
                }
                deckfile::BodyRow::Mix { gain, .. } => {
                    if let Some(g) = gain {
                        params.mix_gain = *g;
                    }
                }
                deckfile::BodyRow::Note {
                    midi,
                    start_beat,
                    dur_beats,
                    vel,
                    ..
                } => notes.push(Note {
                    start_beat: *start_beat,
                    dur_beat: *dur_beats,
                    midi: *midi as u8,
                    vel: vel.unwrap_or(100) as u8,
                }),
                deckfile::BodyRow::Steps { on, .. } => {
                    if on.len() != 16 {
                        return Err(format!(
                            "{file}: track `{}` steps needs 16 x/. tokens, got {}",
                            t.id,
                            on.len()
                        ));
                    }
                    let mut grid = [false; 16];
                    grid.copy_from_slice(&on[..16]);
                    steps = Some(grid);
                }
                deckfile::BodyRow::StepPitch { midi, .. } => step_pitch = *midi as u8,
                // Registered by register_gba_dialect().
                deckfile::BodyRow::Host { kind, value } if kind == "layer" => {
                    let level = deckfile::runtime::get_index(
                        value,
                        &deckfile::Value::String("level".into()),
                    );
                    let level = match level {
                        deckfile::Value::Number(n) => n as i64,
                        _ => return Err(format!("{file}: layer/intensity needs 0..3")),
                    };
                    min_intensity = clamp_layer(level, &file)?;
                }
                // Dropped at bake, matching what this crate has always done: the sequencer has no
                // per-step lock lanes, `loops` is a host playback cap, and `voice` (octave / arp /
                // chord / strum) has no hardware meaning here.
                deckfile::BodyRow::StepLane { .. }
                | deckfile::BodyRow::Loops { .. }
                | deckfile::BodyRow::Voice { .. } => {}
                deckfile::BodyRow::Unknown { head, .. } => {
                    return Err(format!(
                        "{file}: unsupported track body `{head}` on GBA (see docs/deck.md)"
                    ))
                }
                other => {
                    return Err(format!(
                        "{file}: track `{}` has a body line unsupported on GBA: {other:?}",
                        t.id
                    ))
                }
            }
        }

        tracks.push(TrackIn {
            id: t.id.clone(),
            name: if t.name.is_empty() {
                t.id.clone()
            } else {
                t.name.clone()
            },
            bars,
            min_intensity,
            params,
            notes,
            steps,
            step_pitch,
        });
    }

    if tracks.is_empty() {
        return Err(format!("{file}: no tracks"));
    }

    // Expand steps → notes if no notes
    for tr in &mut tracks {
        if tr.notes.is_empty() {
            if let Some(steps) = tr.steps {
                for (i, on) in steps.iter().enumerate() {
                    if *on {
                        tr.notes.push(Note {
                            start_beat: i as f64 * 0.25,
                            dur_beat: 0.25,
                            midi: tr.step_pitch,
                            vel: 100,
                        });
                    }
                }
            }
        }
        // Repeat pattern for * N bars if notes only cover bar 0
        if tr.bars > 1 && !tr.notes.is_empty() {
            let base: Vec<Note> = tr.notes.clone();
            let max_beat = base
                .iter()
                .map(|n| n.start_beat + n.dur_beat)
                .fold(0.0, f64::max);
            if max_beat <= 4.0 + 1e-6 {
                for b in 1..tr.bars {
                    for n in &base {
                        tr.notes.push(Note {
                            start_beat: n.start_beat + (b as f64) * 4.0,
                            dur_beat: n.dur_beat,
                            midi: n.midi,
                            vel: n.vel,
                        });
                    }
                }
            }
        }
        tr.notes
            .sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());
    }

    if tracks.len() > MAX_TRACKS {
        return Err(format!(
            "{file}: {} tracks — the player holds {MAX_TRACKS} and would silently drop the rest \
             (last is `{}`)",
            tracks.len(),
            tracks.last().map(|t| t.id.as_str()).unwrap_or("?")
        ));
    }

    // Channel allocation
    let mut pulse_n = 0u8;
    let mut wave_n = 0u8;
    let mut noise_n = 0u8;
    let mut pcm_n = 0u8;
    let mut alloc: Vec<(u8, u8, u8, u8)> = Vec::new(); // kind, hw_ch, wave_idx, flags
    let mut waves: Vec<[u8; 16]> = Vec::new();
    let mut pcm_tables: Vec<Vec<u8>> = Vec::new();
    let mut zones: Vec<(&Zone, u8)> = Vec::new();
    // One entry per track, in step with `alloc` — pushed unconditionally at the top of the loop so
    // a branch that does not use zones cannot silently shift every later track's zone window.
    let mut zone_span: Vec<(u8, u8)> = Vec::new();

    for tr in &tracks {
        let p = &tr.params;
        zone_span.push((0, 0));
        if p.gen == "gameBoyDmg" {
            match p.type_.as_str() {
                "wave" => {
                    if wave_n >= 1 {
                        return Err(format!(
                            "{file}: track `{}` is a 2nd wave voice — LR35902 has only 1 wave channel (docs/deck.md)",
                            tr.id
                        ));
                    }
                    let table = named_waves
                        .get(&p.wave_shape)
                        .copied()
                        .unwrap_or_else(|| wave_nibbles(&p.wave_shape));
                    let wi = waves.len() as u8;
                    waves.push(table);
                    alloc.push((VOICE_WAVE, 3, wi, 0));
                    wave_n += 1;
                }
                "noise" => {
                    if noise_n >= 1 {
                        return Err(format!(
                            "{file}: track `{}` is a 2nd noise voice — LR35902 has only 1 noise channel (docs/deck.md)",
                            tr.id
                        ));
                    }
                    let flags = if p.noise_mode == "short" {
                        FLAG_NOISE_NARROW
                    } else {
                        0
                    };
                    alloc.push((VOICE_NOISE, 4, 0, flags));
                    noise_n += 1;
                }
                _ => {
                    // pulse
                    if pulse_n >= 2 {
                        return Err(format!(
                            "{file}: track `{}` is a 3rd pulse voice — LR35902 has only 2 square channels (docs/deck.md)",
                            tr.id
                        ));
                    }
                    let ch = if pulse_n == 0 { 1 } else { 2 };
                    alloc.push((VOICE_PULSE, ch, 0, 0));
                    pulse_n += 1;
                }
            }
        } else if let Some(prog) = p.program {
            // gbaDirectSound playing a real instrument out of the `sampleset`.
            if pcm_n >= MAX_PCM_VOICES {
                return Err(format!(
                    "{file}: track `{}` is PCM voice {} — the mixer has {MAX_PCM_VOICES} \
                     (agb has 8 channels total and sound effects need some; see docs/deck.md)",
                    tr.id,
                    pcm_n + 1
                ));
            }
            let def = programs.get(&prog).ok_or_else(|| {
                format!(
                    "{file}: track `{}` plays program {prog}, which is not in the sampleset \
                     (add a `sampleset <path>` line, or re-extract with that song)",
                    tr.id
                )
            })?;
            if def.zones.is_empty() {
                return Err(format!(
                    "{file}: track `{}` plays program {prog}, which is `{}` — it has no samples \
                     to play. A PSG program belongs on a gameBoyDmg track.",
                    tr.id, def.kind
                ));
            }
            let lo = zones.len() as u8;
            for z in &def.zones {
                let si = sample_files
                    .iter()
                    .position(|(n, _)| *n == z.sample)
                    .expect("sample was loaded with the sampleset") as u8;
                zones.push((z, si));
            }
            *zone_span
                .last_mut()
                .expect("pushed at the top of this iteration") = (lo, (zones.len() as u8) - lo);
            alloc.push((VOICE_DS_SAMPLE, pcm_n, lo, 0));
            pcm_n += 1;
        } else {
            // gbaDirectSound with a generated wavetable — a synth, not a recording.
            if pcm_n >= MAX_PCM_VOICES {
                return Err(format!(
                    "{file}: track `{}` is PCM voice {} — the mixer has {MAX_PCM_VOICES} (docs/deck.md)",
                    tr.id,
                    pcm_n + 1
                ));
            }
            let table = pcm_table(&p.waveform, &p.duty, p.bitcrush);
            let wi = pcm_tables.len() as u8;
            pcm_tables.push(table);
            let flags = if p.bitcrush { FLAG_BITCRUSH } else { 0 };
            alloc.push((VOICE_DS_PCM, pcm_n, wi, flags));
            pcm_n += 1;
        }
    }

    // Song length
    let mut length_frames: u32 = 1;
    for tr in &tracks {
        for n in &tr.notes {
            let end = beat_to_frames(n.start_beat + n.dur_beat, bpm) as u32;
            length_frames = length_frames.max(end.max(1));
        }
        let bar_end = beat_to_frames(tr.bars as f64 * 4.0, bpm) as u32;
        length_frames = length_frames.max(bar_end.max(1));
    }

    // Emit tracks
    let mut track_toks = Vec::new();
    for (i, tr) in tracks.iter().enumerate() {
        let (kind, hw_ch, wave_idx, mut flags) = alloc[i];
        let p = &tr.params;
        let (zone_lo, zone_n) = zone_span[i];
        let pan = p.pan.clamp(-64, 63);
        let duty = duty_code(&p.duty);
        let vol = ((p.vol as f64) * p.mix_gain).round().clamp(0.0, 15.0) as u8;
        let env_mode = if kind == VOICE_DS_PCM || kind == VOICE_DS_SAMPLE {
            ENV_ADSR
        } else {
            env_code(&p.env_mode)
        };
        // A sampled instrument's envelope comes from its voicegroup, not from the deck: the
        // deck says which instrument to play, and the instrument knows its own attack and release.
        // Zone 0's is used for the whole voice — M4A's zones share an envelope in practice.
        let first_zone = (zone_n > 0).then(|| zones[zone_lo as usize].0);
        let (a, d, s, r) = match first_zone {
            Some(z) => (z.a, z.d, z.s, z.r),
            None => (
                secs_to_frames(p.attack_s),
                secs_to_frames(p.decay_s),
                p.sustain.min(15),
                secs_to_frames(p.release_s),
            ),
        };
        let vib_rate = (p.vib_rate * 4.0).round().clamp(0.0, 255.0) as u8;
        let vib_amt = (p.vib_amt / 2.0).round().clamp(0.0, 255.0) as u8;
        let arp_rate = p.arp_rate.round().clamp(0.0, 255.0) as u8;
        let arp_semis = p.arp_semis;
        let drop_semis = p.drop_semis;
        let drop_dec = secs_to_frames(p.drop_dec_s);
        let len = p.len;
        let env_step = p.env_step.min(7);
        let soft_sweep = p.soft_sweep;
        let sweep_shift = p.sweep_shift.min(7);
        let sweep_period = p.sweep_period.min(7);
        if p.env_up {
            flags |= FLAG_ENV_UP;
        }
        if p.sweep_down || soft_sweep < 0 {
            flags |= FLAG_SWEEP_DOWN;
        }
        let (noise_shift, noise_ratio) = if let Some(ns) = p.noise_shift {
            flags |= FLAG_NOISE_SHIFT_SET;
            (ns.min(13), p.noise_ratio.min(7))
        } else {
            (0u8, p.noise_ratio.min(7))
        };

        let note_toks: Vec<_> = tr
            .notes
            .iter()
            .map(|n| {
                let start = beat_to_frames(n.start_beat, bpm);
                let dur = beat_to_frames(n.dur_beat, bpm).max(1);
                let midi = n.midi;
                let vel = n.vel;
                quote! {
                    tish_agb::deck_player::DeckNote {
                        start: #start,
                        dur: #dur,
                        midi: #midi,
                        vel: #vel,
                    }
                }
            })
            .collect();

        let min_intensity = tr.min_intensity;
        track_toks.push(quote! {
            tish_agb::deck_player::DeckTrack {
                voice: tish_agb::deck_player::DeckVoice {
                    kind: #kind,
                    hw_ch: #hw_ch,
                    duty: #duty,
                    vol: #vol,
                    env_mode: #env_mode,
                    a: #a,
                    d: #d,
                    s: #s,
                    r: #r,
                    vib_rate: #vib_rate,
                    vib_amt: #vib_amt,
                    arp_rate: #arp_rate,
                    arp_semis: #arp_semis,
                    drop_semis: #drop_semis,
                    drop_dec: #drop_dec,
                    wave: #wave_idx,
                    flags: #flags,
                    len: #len,
                    env_step: #env_step,
                    sweep_shift: #sweep_shift,
                    sweep_period: #sweep_period,
                    noise_shift: #noise_shift,
                    noise_ratio: #noise_ratio,
                    soft_sweep: #soft_sweep,
                    zone_lo: #zone_lo,
                    zone_n: #zone_n,
                    pan: #pan,
                },
                min_intensity: #min_intensity,
                notes: &[#(#note_toks),*],
            }
        });
    }

    let wave_toks = waves.iter().map(|w| {
        let b = w.iter().copied();
        quote! { [#(#b),*] }
    });

    // PCM tables as aligned statics + SoundData::new
    let mut pcm_static_toks = Vec::new();
    let mut pcm_data_toks = Vec::new();
    for (i, table) in pcm_tables.iter().enumerate() {
        let ty = syn::Ident::new(&format!("PcmAlign{i}"), proc_macro2::Span::call_site());
        let st = syn::Ident::new(&format!("PCM_TABLE_{i}"), proc_macro2::Span::call_site());
        let bytes = table.iter().copied();
        let len = table.len();
        pcm_static_toks.push(quote! {
            #[repr(C, align(4))]
            struct #ty([u8; #len]);
            static #st: #ty = #ty([#(#bytes),*]);
        });
        pcm_data_toks.push(quote! {
            unsafe { agb::sound::mixer::SoundData::new(&#st.0) }
        });
    }

    // Sampled instruments. Unlike the 32-byte generated tables above, these are tens of kilobytes
    // each, so they are `include_bytes!`d rather than streamed through `quote!` as byte literals —
    // a 30KB sample is 30,000 tokens the proc-macro would otherwise have to build and parse.
    let mut sample_static_toks = Vec::new();
    let mut sample_toks = Vec::new();
    for (i, (_name, sf)) in sample_files.iter().enumerate() {
        let ty = syn::Ident::new(&format!("SampleAlign{i}"), proc_macro2::Span::call_site());
        let st = syn::Ident::new(&format!("SAMPLE_{i}"), proc_macro2::Span::call_site());
        let abs = sf.path.to_string_lossy().to_string();
        let len = sf.len;
        let rate = sf.rate;
        let loop_start = sf.loop_start;
        sample_static_toks.push(quote! {
            #[repr(C, align(4))]
            struct #ty([u8; #len]);
            static #st: #ty = #ty(*include_bytes!(#abs));
        });
        sample_toks.push(quote! {
            tish_agb::deck_player::DeckSample {
                data: unsafe { agb::sound::mixer::SoundData::new(&#st.0) },
                rate_hz: #rate,
                loop_start: #loop_start,
            }
        });
    }

    let zone_toks = zones.iter().map(|(z, si)| {
        let (lo, hi, root, fixed) = (z.lo, z.hi, z.root, z.fixed);
        let pan = z.pan.unwrap_or(0);
        let pan_set = z.pan.is_some();
        quote! {
            tish_agb::deck_player::DeckZone {
                lo: #lo,
                hi: #hi,
                sample: #si,
                root: #root,
                pan: #pan,
                pan_set: #pan_set,
                fixed: #fixed,
            }
        }
    });

    let loop_frame: u32 = 0;
    let length_frames = length_frames;

    Ok(quote! {
        #(#pcm_static_toks)*
        #(#sample_static_toks)*
        pub static SONG: tish_agb::deck_player::DeckSong = tish_agb::deck_player::DeckSong {
            loop_frame: #loop_frame,
            length_frames: #length_frames,
            tracks: &[#(#track_toks),*],
            waves: &[#(#wave_toks),*],
            pcm_tables: &[#(#pcm_data_toks),*],
            samples: &[#(#sample_toks),*],
            zones: &[#(#zone_toks),*],
        };
        pub fn __deck_register() -> i32 {
            tish_agb::native_deck_song_register(&SONG)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// agb's mixer loads samples with `ldrsb` — they are SIGNED. Emitting offset-binary 0..255
    /// instead collapsed a 50% pulse to ±1 (42dB down, effectively silent) and turned a sine into
    /// 226% THD buzz. Both bugs are invisible in a build and in a note-count check; they are only
    /// audible. This pins the encoding so they cannot come back.
    #[test]
    fn pcm_table_is_signed_8bit() {
        for waveform in ["pulse", "sine", "triangle", "sawtooth"] {
            let t = pcm_table(waveform, "50", false);
            let signed: Vec<i8> = t.iter().map(|&b| b as i8).collect();
            let peak = signed.iter().map(|&v| (v as i32).abs()).max().unwrap();
            assert!(
                peak > 100,
                "{waveform}: peak {peak} of 127 — reading the table as signed leaves it near \
                 silent, which means it was written as offset-binary"
            );
            // A full-scale waveform must use both polarities; offset-binary lands entirely in one.
            assert!(
                signed.iter().any(|&v| v > 64) && signed.iter().any(|&v| v < -64),
                "{waveform}: does not span both polarities when read as signed"
            );
        }
    }

    /// A 50% pulse is exactly two levels, and they must be the rails. Under the old encoding this
    /// was ±1.
    #[test]
    fn pulse_table_reaches_the_rails() {
        let t = pcm_table("pulse", "50", false);
        let signed: Vec<i8> = t.iter().map(|&b| b as i8).collect();
        assert!(
            signed.contains(&127),
            "pulse high is not full scale: {signed:?}"
        );
        assert!(
            signed.iter().any(|&v| v <= -127),
            "pulse low is not full scale: {signed:?}"
        );
    }
/// The whole point of adopting `wave` into the language: the two spellings are one sound. A
    /// `harmonics` line and the hex literal it resolves to must pack to byte-identical wave RAM, or
    /// a song would sound different depending only on how its table was written.
    ///
    /// This is the hardware end of that guarantee — the 16 bytes checked here are copied verbatim
    /// into WAVE_RAM by `psg::wave_table`.
    #[test]
    fn wave_harmonics_and_hex_pack_identically() {
        let prog = deckfile::facade::parse(
            "deck 1\nwave lit 8beffecbbbbaa9888776554444310014\nwave gen harmonics 1 0.5 0.33 0.2\n",
        );
        assert!(prog.errors.is_empty(), "{:?}", prog.errors);

        let pack = |name: &str| -> [u8; 16] {
            let w = prog.waves.iter().find(|w| w.name == name).expect("wave present");
            assert_eq!(w.levels.len(), 32);
            let mut out = [0u8; 16];
            for i in 0..16 {
                out[i] = ((w.levels[i * 2] as u8) << 4) | (w.levels[i * 2 + 1] as u8);
            }
            out
        };

        let lit = pack("lit");
        assert_eq!(pack("gen"), lit, "harmonics must bake to the same wave RAM as its hex literal");
        // Every nibble is a 4-bit level, so no byte can carry a value the hardware cannot hold.
        for b in lit {
            assert!(b >> 4 <= 15 && (b & 0x0f) <= 15);
        }
    }
}
