//! Holds this crate to the `gba` profile of the shared conformance corpus.
//!
//! `profiles.json` in the deck repo declares which corpus cases the GBA bake may legitimately
//! reject — it supports a subset of the language, two sound chips and no session/automation/co-DJ
//! layer. That declaration was previously just a document: nothing checked that the bake actually
//! behaves the way the profile says.
//!
//! This closes that. For every case in the corpus:
//!
//!   * listed in `mustAccept` — the bake MUST succeed. If it starts rejecting one, the subset has
//!     quietly shrunk and a `.deck` file that used to work no longer does.
//!   * listed in `mayReject`  — the bake may go either way, but a rejection has to be a clean error
//!     naming the reason, never a panic and never a silently mangled song.
//!
//! The corpus travels inside the `deckfile` crate (`deckfile::corpus`), so it is the same bytes the
//! JS build and the Tish source are checked against — not a copy that can drift.
use std::io::Write;

/// Minimal field lookup for `profiles.json` — enough to read the `gba` entry without pulling serde
/// into a proc-macro's dependency tree for one test.
fn profile_case_names(profiles: &str, section: &str) -> Vec<String> {
    // Find `"gba"`, then the section within it, then collect the quoted keys/strings inside.
    let Some(gba) = profiles.find("\"gba\"") else {
        return Vec::new();
    };
    let rest = &profiles[gba..];
    let Some(start) = rest.find(&format!("\"{section}\"")) else {
        return Vec::new();
    };
    let body = &rest[start..];
    // Bounded by the matching close of the section's object/array.
    let open = body.find(['{', '[']).unwrap_or(0);
    let close_ch = if body.as_bytes().get(open) == Some(&b'{') {
        '}'
    } else {
        ']'
    };
    let end = body.find(close_ch).unwrap_or(body.len());
    let inner = &body[open..end];

    let mut out = Vec::new();
    let mut chars = inner.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '"' {
            continue;
        }
        if let Some(j) = inner[i + 1..].find('"') {
            let s = &inner[i + 1..i + 1 + j];
            // Case names look like `001-comments-and-sharps`; reasons are prose.
            if s.len() > 4 && s.as_bytes()[3] == b'-' && s[..3].chars().all(|c| c.is_ascii_digit())
            {
                out.push(s.to_string());
            }
            while let Some(&(k, _)) = chars.peek() {
                if k > i + j {
                    break;
                }
                chars.next();
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn bake(name: &str, source: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join("deckpack_profile");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.deck"));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(source.as_bytes()).unwrap();
    drop(f);
    crate::deckpack::build(&path).map(|_| ())
}

#[test]
fn gba_profile_is_honest() {
    let profiles = deckfile::corpus::profiles();
    let must_accept = profile_case_names(profiles, "mustAccept");
    let may_reject = profile_case_names(profiles, "mayReject");
    assert!(
        !must_accept.is_empty() && !may_reject.is_empty(),
        "could not read the gba profile out of profiles.json"
    );

    let mut unaccounted = Vec::new();
    let mut broke = Vec::new();

    for case in deckfile::corpus::cases() {
        let name = case.name.to_string();
        let declared_reject = may_reject.contains(&name);
        let declared_accept = must_accept.contains(&name);
        if !declared_reject && !declared_accept {
            unaccounted.push(name);
            continue;
        }

        match bake(case.name, case.source) {
            Ok(()) => {
                // Accepting something we said we might reject is fine — the profile is a ceiling,
                // not a floor. The failure that matters is the other direction.
            }
            Err(e) => {
                if declared_accept {
                    broke.push(format!(
                        "{name}: profile says mustAccept, but the bake failed: {e}"
                    ));
                } else {
                    assert!(
                        !e.is_empty(),
                        "{name}: rejected with an empty message — a rejection must say why"
                    );
                }
            }
        }
    }

    assert!(
        unaccounted.is_empty(),
        "corpus cases missing from the gba profile (add to mustAccept, or mayReject with a reason): {}",
        unaccounted.join(", ")
    );
    assert!(broke.is_empty(), "{}", broke.join("\n"));
}
