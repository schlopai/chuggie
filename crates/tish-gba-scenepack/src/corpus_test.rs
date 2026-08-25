//! Bakes every `.deck` file in the repo and pins the result.
//!
//! `include_deck!` turns a song into `static SONG: DeckSong { … }` — frames, hardware channels,
//! wavetables, PCM tables. That output is the actual contract with the ROM, so this walks the whole
//! shipped corpus and asserts each file still bakes, and prints a digest of the emitted tokens so
//! two revisions can be compared byte-for-byte.
//!
//! `DECK_CORPUS_DIGEST=1 cargo test -p tish_gba_scenepack -- --nocapture corpus` prints
//! `<file> <digest>` per song; diffing that between revisions is what proves a parser change did not
//! move a single note.
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // crates/tish-gba-scenepack -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn find_decks(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if name == "target" || name == "node_modules" || name == ".git" {
                continue;
            }
            find_decks(&p, out);
        } else if name.ends_with(".deck") {
            out.push(p);
        }
    }
}

/// FNV-1a over the emitted token text — enough to detect any change, no dependency needed.
fn digest(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[test]
fn corpus_bakes() {
    let root = repo_root();
    let mut decks = Vec::new();
    find_decks(&root, &mut decks);
    decks.sort();
    // The floor guards against the walk silently finding nothing (wrong root, over-eager skip
    // list). It was 60 before several game lineages were extracted to their own repos.
    assert!(
        decks.len() >= 50,
        "expected the shipped corpus, found {} files",
        decks.len()
    );

    let print = std::env::var("DECK_CORPUS_DIGEST").is_ok();
    let mut failures = Vec::new();
    for path in &decks {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        match crate::deckpack::build(path) {
            Ok(ts) => {
                if print {
                    println!("{rel} {:016x}", digest(&ts.to_string()));
                }
            }
            Err(e) => {
                if print {
                    println!("{rel} ERROR {e}");
                }
                failures.push(format!("{rel}: {e}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} songs failed to bake:\n{}",
        failures.len(),
        decks.len(),
        failures.join("\n")
    );
}

/// Behaviour this crate has always had, kept deliberately when the parser moved upstream: `voice` is
/// dropped at bake (no hardware meaning), `fx` is a hard error (it would silently change the sound).
#[test]
fn voice_is_dropped_and_fx_errors() {
    let dir = std::env::temp_dir().join("deckpack_behaviour");
    std::fs::create_dir_all(&dir).unwrap();

    let voice = dir.join("voice.deck");
    std::fs::write(
        &voice,
        "deck 1\nbpm 120\ntrack Lead id lead gen gameBoyDmg\n  voice octave -1 arp up\n  note 60 0 1 v 100\n",
    )
    .unwrap();
    assert!(
        crate::deckpack::build(&voice).is_ok(),
        "`voice` must be dropped, not rejected"
    );

    let fx = dir.join("fx.deck");
    std::fs::write(
        &fx,
        "deck 1\nbpm 120\ntrack Lead id lead gen gameBoyDmg\n  fx cutoff 1200\n  note 60 0 1 v 100\n",
    )
    .unwrap();
    let err =
        crate::deckpack::build(&fx).expect_err("`fx` has no GBA meaning and must be rejected");
    assert!(err.contains("fx"), "error should name the feature: {err}");
}
