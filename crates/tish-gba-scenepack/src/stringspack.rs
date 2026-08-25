//! Compile a `.strings` file — one readable text file, several languages — into a static table.
//!
//! Nothing here runs on the GBA. The device sees an array of `&'static str` in ROM; all the parsing
//! and, more importantly, all the CHECKING happens on the host.
//!
//! ## Why this exists
//!
//! Every string in every example is currently inline in tish source. That is fine for one language
//! and hopeless for two: a translated build means editing the game, and the same sentence written in
//! three places drifts. The engine already does selective glyph baking for CJK
//! (`font<N>:` + `docs/`), so the fonts were ready and the text was not.
//!
//! ## The format
//!
//! ```text
//! # A comment. Blank lines are ignored.
//! [en]
//! Hello, traveller.
//! The door is locked.
//! Gold: %d
//!
//! [ja]
//! こんにちは、旅人。
//! 扉には鍵がかかっている。
//! 所持金: %d
//! ```
//!
//! A `[lang]` header opens a section; every non-empty line after it is one string, and **the id is
//! the line's position in the section**. That is deliberate: ids that are positions cannot drift
//! between languages the way named keys can, and the lookup on device is an array index rather than
//! a string compare.
//!
//! ## ⚠️ THE COUNT CHECK IS THE POINT
//!
//! A translation with a missing line silently shifts every id after it, so the game shows the wrong
//! sentence — in a language the author cannot read, which is exactly when nobody notices. Every
//! section must therefore have the SAME number of strings, and a mismatch is a compile error naming
//! the language and both counts. This is the one bug this format exists to make impossible.
//!
//! An empty string is written `~` so that "this line is deliberately blank" is distinguishable from
//! a dropped line.

use quote::quote;
use std::path::Path;

#[derive(Debug)]
pub struct Strings {
    pub langs: Vec<String>,
    /// `rows[lang][id]`
    pub rows: Vec<Vec<String>>,
}

pub fn parse(text: &str) -> Result<Strings, String> {
    let mut langs: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cur: Option<usize> = None;

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(format!("line {}: empty [] language header", lineno + 1));
            }
            if langs.contains(&name) {
                return Err(format!(
                    "line {}: language [{}] appears twice",
                    lineno + 1,
                    name
                ));
            }
            langs.push(name);
            rows.push(Vec::new());
            cur = Some(rows.len() - 1);
            continue;
        }
        match cur {
            None => {
                return Err(format!(
                "line {}: a string before any [lang] header — every string belongs to a language",
                lineno + 1
            ))
            }
            Some(i) => {
                // `~` is an explicitly empty string; see the module note on dropped lines.
                rows[i].push(if line == "~" {
                    String::new()
                } else {
                    line.to_string()
                });
            }
        }
    }

    if langs.is_empty() {
        return Err("no [lang] sections — the file defines no languages".into());
    }
    let want = rows[0].len();
    if want == 0 {
        return Err(format!("[{}] has no strings", langs[0]));
    }
    for (i, r) in rows.iter().enumerate().skip(1) {
        if r.len() != want {
            return Err(format!(
                "[{}] has {} strings but [{}] has {} — every language must define the same ids, or \
                 every string after the missing one shifts and the game shows the wrong sentence",
                langs[i],
                r.len(),
                langs[0],
                want
            ));
        }
    }
    Ok(Strings { langs, rows })
}

pub fn build(path: &Path) -> Result<proc_macro2::TokenStream, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let s = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;

    let n_lang = s.langs.len();
    let n_str = s.rows[0].len();
    let lang_toks: Vec<_> = s.langs.iter().map(|l| quote! { #l }).collect();
    let row_toks: Vec<_> = s
        .rows
        .iter()
        .map(|r| {
            let items: Vec<_> = r.iter().map(|t| quote! { #t }).collect();
            quote! { &[#(#items),*] }
        })
        .collect();

    Ok(quote! {
        pub static LANGS: [&str; #n_lang] = [#(#lang_toks),*];
        pub static TABLE: [&[&str]; #n_lang] = [#(#row_toks),*];
        pub const COUNT: usize = #n_str;
        pub fn __strings_register() -> i32 {
            tish_agb::register_strings(&LANGS, &TABLE)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_languages() {
        let s = parse("[en]\nHello\nBye\n\n[ja]\nこんにちは\nさようなら\n").unwrap();
        assert_eq!(s.langs, vec!["en", "ja"]);
        assert_eq!(s.rows[0], vec!["Hello", "Bye"]);
        assert_eq!(s.rows[1][0], "こんにちは");
    }

    #[test]
    fn tilde_is_an_empty_string() {
        let s = parse("[en]\n~\nBye\n").unwrap();
        assert_eq!(s.rows[0][0], "");
    }

    #[test]
    fn a_short_translation_is_an_error() {
        // The bug the format exists to prevent: [ja] is missing a line, so every id after it would
        // shift and the game would show the wrong sentence in a language nobody reviewing can read.
        let e = parse("[en]\nHello\nBye\n[ja]\nこんにちは\n").unwrap_err();
        assert!(e.contains("must define the same ids"), "{e}");
    }

    #[test]
    fn a_string_before_any_header_is_an_error() {
        assert!(parse("Hello\n[en]\nHi\n").is_err());
    }

    #[test]
    fn a_duplicate_language_is_an_error() {
        assert!(parse("[en]\nHi\n[en]\nHo\n").is_err());
    }
}
