//! Baked glyph advances for fast text measure (no agb `Layout`).
//!
//! agb's `Font::letter` / `advance_width` are `pub(crate)`, so the `font:` baker emits a parallel
//! [`FontMetrics`] table. Menu flex measure (`text_width` / `text_wrap_height`) sums advances and
//! does a greedy word-wrap that matches agb's left-aligned `Align` (kerning is empty in our baker).

use alloc::vec::Vec;
use core::cell::RefCell;

use tishlang_runtime_gba::SingleCore;

/// agb private-use range used for `ChangeColour` / `Tag` control chars — zero width, like `Align`.
const AGB_PRIVATE_USE_START: u32 = 0xE000;
const AGB_PRIVATE_USE_END: u32 = 0xE000 + 48;

/// Per-font advance table baked beside the agb `Font` by `include_font_pack!`.
pub struct FontMetrics {
    /// Advances for `'!'..='~'` (0x21..0x7F); index = `c as usize - 0x21`. Length 94.
    pub ascii: &'static [u8],
    /// Sorted non-ASCII letters (always includes space); binary-searched.
    pub letters: &'static [(char, u8)],
    pub line_height: i32,
}

impl FontMetrics {
    #[inline]
    pub fn advance(&self, c: char) -> i32 {
        let cp = c as u32;
        if c == '\n' || c == '\r' {
            return 0;
        }
        if (AGB_PRIVATE_USE_START..AGB_PRIVATE_USE_END).contains(&cp) {
            return 0;
        }
        if (0x21..0x7F).contains(&cp) {
            return self.ascii[c as usize - 0x21] as i32;
        }
        match self.letters.binary_search_by_key(&c, |&(ch, _)| ch) {
            Ok(i) => self.letters[i].1 as i32,
            // Missing glyph → same fallback as agb (`letters[0]`, usually space).
            Err(_) => self.letters.first().map(|&(_, a)| a as i32).unwrap_or(0),
        }
    }

    #[inline]
    pub fn space_width(&self) -> i32 {
        self.advance(' ')
    }

    /// Single-line (or max-over-lines) pixel width — sum of advances. Matches `text_width` Layout
    /// when kerning is empty.
    pub fn width(&self, text: &str) -> i32 {
        let mut line = 0i32;
        let mut max = 0i32;
        for c in text.chars() {
            if c == '\n' {
                if line > max {
                    max = line;
                }
                line = 0;
            } else {
                line += self.advance(c);
            }
        }
        if line > max {
            max = line;
        }
        max
    }

    /// Pixel height when wrapped to `maxw` (line count × `line_height`). Mirrors agb left `Align`
    /// word-wrap: break before the overflowing word, or mid-word if it is the only word on the line.
    pub fn wrap_height(&self, text: &str, maxw: i32) -> i32 {
        let lines = self.wrap_line_count(text, maxw);
        let n = if lines < 1 { 1 } else { lines };
        n * self.line_height
    }

    fn wrap_line_count(&self, text: &str, maxw: i32) -> i32 {
        if text.is_empty() {
            return 1;
        }
        // Unlimited width: only explicit newlines (and at least one line).
        if maxw <= 0 {
            let mut lines = 1i32;
            for c in text.chars() {
                if c == '\n' {
                    lines += 1;
                }
            }
            return lines;
        }

        let space_w = self.space_width();
        let mut lines = 0i32;
        let mut processed = 0usize;

        while processed < text.len() {
            // Skip leading spaces (agb Align).
            let mut start = processed;
            let mut found = false;
            for (i, c) in text[processed..].char_indices() {
                if c != ' ' {
                    start = processed + i;
                    found = true;
                    break;
                }
            }
            if !found {
                break;
            }

            let mut words_w = 0i32;
            let mut word_w = 0i32;
            let mut word_start = start;
            let mut spaces = 0i32;
            let mut broke = false;

            for (i, c) in text[start..].char_indices() {
                let idx = start + i;
                if c == '\n' {
                    processed = idx + c.len_utf8();
                    lines += 1;
                    broke = true;
                    break;
                }
                if c == ' ' {
                    spaces += 1;
                    words_w += word_w;
                    word_w = 0;
                    word_start = idx + 1;
                    continue;
                }
                if (AGB_PRIVATE_USE_START..AGB_PRIVATE_USE_END).contains(&(c as u32)) {
                    continue;
                }
                word_w += self.advance(c);

                let total = words_w + word_w + spaces * space_w;
                if total > maxw {
                    if spaces == 0 {
                        // Break mid-word (before this char), unless this is the first char of the line.
                        if idx == start {
                            processed = idx + c.len_utf8();
                        } else {
                            processed = idx;
                        }
                    } else {
                        processed = word_start;
                    }
                    // Ensure progress even on pathological single-char overflow.
                    if processed <= start {
                        let ch = text[start..].chars().next().unwrap();
                        processed = start + ch.len_utf8();
                    }
                    lines += 1;
                    broke = true;
                    break;
                }
            }
            if !broke {
                // Rest of text fits on this line.
                lines += 1;
                processed = text.len();
            }
        }

        if lines < 1 {
            1
        } else {
            lines
        }
    }
}

static FONT_METRICS: SingleCore<RefCell<Vec<Option<&'static FontMetrics>>>> =
    SingleCore::new(RefCell::new(Vec::new()));

/// Bind metrics to a font handle returned by `__asset_register_font`. Called from `font:` schemes.
pub fn register_font(handle: i32, metrics: &'static FontMetrics) -> i32 {
    if handle < 0 {
        return handle;
    }
    FONT_METRICS.with(|c| {
        let mut v = c.borrow_mut();
        let idx = handle as usize;
        if v.len() <= idx {
            v.resize(idx + 1, None);
        }
        v[idx] = Some(metrics);
    });
    handle
}

pub fn font_metrics(handle: i32) -> Option<&'static FontMetrics> {
    if handle < 0 {
        return None;
    }
    FONT_METRICS.with(|c| c.borrow().get(handle as usize).copied().flatten())
}
