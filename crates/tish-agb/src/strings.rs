//! The `strings:` asset arena — a multi-language string table, in ROM.
//!
//! Everything here is `&'static str` pointing straight at data the `include_strings!` macro baked
//! into the cartridge (see `tish-gba-scenepack/src/stringspack.rs`). Nothing is copied, nothing is
//! allocated, and a lookup is two array indexes — which is the point of ids being POSITIONS rather
//! than names: a named key would mean a string compare per lookup, on a chip where that is the
//! expensive kind of work.
//!
//! The host-side macro guarantees every language defines the same number of strings, so `id` is
//! valid in every language or in none, and switching language cannot shift the text under a game.

use alloc::vec::Vec;
use core::cell::RefCell;
use tishlang_runtime_gba::SingleCore;

pub struct StringTable {
    pub langs: &'static [&'static str],
    pub rows: &'static [&'static [&'static str]],
}

static TABLES: SingleCore<RefCell<Vec<StringTable>>> = SingleCore::new(RefCell::new(Vec::new()));

/// Register a baked table and hand back its handle. Called from the `strings:` import scheme.
pub fn register_strings(
    langs: &'static [&'static str],
    rows: &'static [&'static [&'static str]],
) -> i32 {
    TABLES.with(|c| {
        let mut v = c.borrow_mut();
        v.push(StringTable { langs, rows });
        (v.len() - 1) as i32
    })
}

fn with_table<R>(handle: i32, f: impl FnOnce(&StringTable) -> R) -> Option<R> {
    if handle < 0 {
        return None;
    }
    TABLES.with(|c| c.borrow().get(handle as usize).map(f))
}

/// How many strings each language defines.
pub fn strings_count(handle: i32) -> i32 {
    with_table(handle, |t| t.rows.first().map_or(0, |r| r.len() as i32)).unwrap_or(0)
}

/// How many languages the table carries.
pub fn strings_langs(handle: i32) -> i32 {
    with_table(handle, |t| t.langs.len() as i32).unwrap_or(0)
}

/// The name of language `lang` (`"en"`, `"ja"`), or `""`.
pub fn strings_lang_name(handle: i32, lang: i32) -> &'static str {
    with_table(handle, |t| {
        if lang < 0 {
            return "";
        }
        t.langs.get(lang as usize).copied().unwrap_or("")
    })
    .unwrap_or("")
}

/// One string. Out of range is `""` rather than a panic: a missing line should show as a gap in the
/// UI, not take the cartridge down — and the macro already made a *shifted* id impossible, which is
/// the failure that actually matters.
pub fn strings_get(handle: i32, lang: i32, id: i32) -> &'static str {
    with_table(handle, |t| {
        if lang < 0 || id < 0 {
            return "";
        }
        match t.rows.get(lang as usize) {
            Some(row) => row.get(id as usize).copied().unwrap_or(""),
            None => "",
        }
    })
    .unwrap_or("")
}

/// The index of a language by name, or -1. For a game that wants to pick a language from a saved
/// preference without hard-coding the order its `.strings` file happens to use.
pub fn strings_find_lang(handle: i32, name: &str) -> i32 {
    with_table(handle, |t| {
        for (i, l) in t.langs.iter().enumerate() {
            if *l == name {
                return i as i32;
            }
        }
        -1
    })
    .unwrap_or(-1)
}
