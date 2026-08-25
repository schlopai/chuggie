//! SELECTIVE font baking. agb's `include_font!` rasterizes EVERY glyph in the TTF — fine for a latin
//! font (~100 glyphs), impossible for a Pan-CJK font (~24,000). This bakes only the glyphs a program
//! actually uses: printable ASCII (always) plus the exact non-ASCII characters the tish compiler
//! collected from the program's string literals (passed as the `chars` argument). So you can import a
//! full 24k-glyph CJK TTF and pay for just the dozen 你好 you display.
//!
//! Rasterization uses FreeType:
//! - **default**: `TARGET_NORMAL` (grayscale) + coverage threshold — scales cleanly for bitmap faces,
//!   CJK fallback, and soft-scaled sizes (e.g. monogram@12 footer).
//! - **Pixelify***: `TARGET_MONO` + `MONOCHROME` — that outline pixel family only snaps with TT hints.
//!
//! It emits the same `agb::display::font::Font` / `FontLetter` a normal `include_font!` would (1bpp
//! packed bitmaps, line metrics), so the runtime path is identical — only the glyph set is trimmed.
//! Kerning is dropped (pixel fonts don't use it), keeping the emitted data minimal.
//!
//! `build_pack` also emits a parallel [`tish_agb::FontMetrics`] advance table — agb keeps
//! `FontLetter::advance_width` crate-private, so menu measure cannot read advances from `Font`.
use freetype::bitmap::PixelMode;
use freetype::face::LoadFlag;
use freetype::{Face, Library};
use proc_macro2::TokenStream;
use quote::quote;
use std::path::Path;

/// Inline advance (px) baked for an emoji codepoint's blank glyph — matches the emoji sprite cell
/// (`emojipack::CELL`) so overlaid emoji and surrounding text don't collide. A touch under 16 keeps
/// emoji from feeling over-spaced next to small pixel fonts.
const EMOJI_ADVANCE: u8 = 15;

/// Grayscale coverage cutoff (0..255). Matches the old fontdue path's "firm" snap.
const GRAY_THRESH: u8 = 120;

struct Letter {
    c: char,
    width: u8,
    height: u8,
    xmin: i8,
    ymin: i8,
    advance: u8,
    data: Vec<u8>,
}

/// Pack a glyph coverage grid into 1bpp, rows padded to 8px, LSB-first — matching agb's own packer.
fn pack_1bpp(content_width: usize, height: usize, on: impl Fn(usize, usize) -> bool) -> Vec<u8> {
    let width = content_width.div_ceil(8) * 8;
    let mut out = Vec::with_capacity(height * (width / 8));
    for y in 0..height {
        for chunk in (0..width).step_by(8) {
            let mut byte = 0u8;
            for bit in 0..8 {
                let px = chunk + bit;
                if px < content_width && on(px, y) {
                    byte |= 1 << bit;
                }
            }
            out.push(byte);
        }
    }
    out
}

struct PackParts {
    font_expr: TokenStream,
    ascii_adv: Vec<u8>,
    extra_adv: Vec<TokenStream>,
    line_height: i32,
}

/// Bake a `Font` expression only (legacy `include_font_used!`).
pub fn build(font_path: &Path, size: f32, extra_chars: &str) -> Result<TokenStream, String> {
    let pack = rasterize(font_path, size, extra_chars)?;
    Ok(pack.font_expr)
}

/// Bake `FONT` + `METRICS` + accessors for `font:` schemes (`include_font_pack!`).
pub fn build_pack(font_path: &Path, size: f32, extra_chars: &str) -> Result<TokenStream, String> {
    let pack = rasterize(font_path, size, extra_chars)?;
    let font_expr = pack.font_expr;
    let ascii_adv = pack.ascii_adv;
    let extra_adv = pack.extra_adv;
    let line_height = pack.line_height;
    Ok(quote! {
        pub static FONT: agb::display::font::Font = #font_expr;
        pub static METRICS: tish_agb::FontMetrics = tish_agb::FontMetrics {
            ascii: &[#(#ascii_adv),*],
            letters: &[#(#extra_adv),*],
            line_height: #line_height,
        };
        #[inline]
        pub fn font() -> &'static agb::display::font::Font { &FONT }
        #[inline]
        pub fn metrics() -> &'static tish_agb::FontMetrics { &METRICS }
    })
}

fn open_face(lib: &Library, path: &Path) -> Result<Face, String> {
    lib.new_face(path, 0)
        .map_err(|e| format!("opening font {}: {e}", path.display()))
}

fn face_has_glyph(face: &Face, c: char) -> bool {
    face.get_char_index(c as usize).is_some_and(|i| i != 0)
}

fn wants_mono_hinting(font_path: &Path) -> bool {
    font_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_ascii_lowercase().contains("pixelify"))
}

/// Rasterize one codepoint. `ymin = bitmap_top - rows` matches fontdue / agb layout.
fn rasterize_char(face: &Face, c: char, px: u32, mono: bool) -> Result<Letter, String> {
    face.set_pixel_sizes(0, px)
        .map_err(|e| format!("set_pixel_sizes({px}): {e}"))?;
    let flags = if mono {
        LoadFlag::TARGET_MONO | LoadFlag::MONOCHROME | LoadFlag::RENDER
    } else {
        LoadFlag::TARGET_NORMAL | LoadFlag::RENDER
    };
    face.load_char(c as usize, flags)
        .map_err(|e| format!("load_char U+{:04X}: {e}", c as u32))?;

    let glyph = face.glyph();
    let bitmap = glyph.bitmap();
    let width = bitmap.width() as usize;
    let height = bitmap.rows() as usize;
    let pitch = bitmap.pitch();
    let buffer = bitmap.buffer();
    let xmin = glyph.bitmap_left() as i8;
    let ymin = (glyph.bitmap_top() - bitmap.rows()) as i8;
    let advance = ((glyph.advance().x + 63) >> 6) as u8;

    let mode = bitmap.pixel_mode().unwrap_or(PixelMode::Gray);
    let data = if width == 0 || height == 0 {
        vec![]
    } else {
        match mode {
            PixelMode::Mono => {
                // FreeType mono: pitch may be negative (bottom-up); MSB = leftmost pixel.
                pack_1bpp(width, height, |x, y| {
                    let row_off = y as isize * pitch as isize;
                    if row_off < 0 {
                        return false;
                    }
                    let idx = row_off as usize + (x / 8);
                    if idx >= buffer.len() {
                        return false;
                    }
                    (buffer[idx] << (x % 8)) & 0x80 != 0
                })
            }
            _ => {
                let stride = pitch.unsigned_abs() as usize;
                pack_1bpp(width, height, |x, y| {
                    let idx = y * stride + x;
                    idx < buffer.len() && buffer[idx] > GRAY_THRESH
                })
            }
        }
    };

    Ok(Letter {
        c,
        width: (width.div_ceil(8) * 8) as u8,
        height: height as u8,
        xmin,
        ymin,
        advance,
        data,
    })
}

fn rasterize(font_path: &Path, size: f32, extra_chars: &str) -> Result<PackParts, String> {
    let px = size.round().clamp(1.0, 255.0) as u32;
    let mono_primary = wants_mono_hinting(font_path);
    let lib = Library::init().map_err(|e| format!("freetype init: {e}"))?;
    let primary = open_face(&lib, font_path)?;
    primary
        .set_pixel_sizes(0, px)
        .map_err(|e| format!("set_pixel_sizes: {e}"))?;

    let metrics = primary.size_metrics().ok_or("font has no size metrics")?;
    let line_height = ((metrics.height + 32) >> 6) as i32;
    let mut ascent = ((metrics.ascender + 32) >> 6) as i32;

    // GLYPH FALLBACK. A latin font has no CJK (or other) glyphs — drawing 你好 in it would bake a tofu
    // box. If a `fallback.ttf` sits beside the primary font, any character the primary lacks is baked
    // from the fallback instead, AT THIS FONT'S SIZE — so e.g. Ark-Pixel CJK glyphs slot into a latin
    // font at matching pixel height and the layout lines up with no runtime font-mixing. Drop one
    // `fallback.ttf` (a broad-coverage font) in your fonts dir and every font there gains its glyphs.
    // Fallback always uses grayscale (never mono) so CJK scales instead of dropping out.
    let fallback_path = font_path
        .parent()
        .map(|d| d.join("fallback.ttf"))
        .filter(|p| p != font_path && p.exists());
    let fallback = fallback_path.as_ref().and_then(|p| open_face(&lib, p).ok());

    let mut chars: Vec<char> = (0x21u32..0x7F).filter_map(char::from_u32).collect();
    // SPACE (0x20) is NOT in agb's direct-index ASCII range (0x21..0x7F); it's looked up in the
    // non-ASCII `letters` array — including for the layout's space-width. Always bake it, or a font
    // whose non-ASCII set is otherwise empty panics (`letters[0]` on an empty slice) the moment any
    // text contains a space.
    chars.push(' ');
    for c in extra_chars.chars() {
        if (c as u32) >= 0x7F && !chars.contains(&c) {
            chars.push(c);
        }
    }

    let mut letters: Vec<Letter> = Vec::with_capacity(chars.len());
    for &c in &chars {
        if crate::emojipack::is_emoji_cp(c as u32) {
            letters.push(Letter {
                c,
                width: 0,
                height: 0,
                xmin: 0,
                ymin: 0,
                advance: EMOJI_ADVANCE,
                data: vec![],
            });
            continue;
        }

        let use_fallback = !face_has_glyph(&primary, c)
            && fallback.as_ref().is_some_and(|fb| face_has_glyph(fb, c));
        let (face, mono) = if use_fallback {
            (fallback.as_ref().unwrap(), false)
        } else {
            (&primary, mono_primary)
        };
        letters.push(rasterize_char(face, c, px, mono)?);
    }
    letters.sort_by_key(|l| l.c);

    let max_above = letters
        .iter()
        .map(|l| l.height as i32 + l.ymin as i32)
        .max()
        .unwrap_or(0);
    if ascent - max_above < 0 {
        ascent = max_above;
    }

    let tok = |l: &Letter| {
        let (c, w, h, xmin, ymin, adv, d) =
            (l.c, l.width, l.height, l.xmin, l.ymin, l.advance, &l.data);
        quote!(agb::display::font::FontLetter::new(#c, #w, #h, &[#(#d),*], #xmin, #ymin, #adv, &[]))
    };
    let ascii = (0x21u32..0x7F).map(|u| {
        let c = char::from_u32(u).unwrap();
        tok(letters
            .iter()
            .find(|l| l.c == c)
            .expect("ascii glyph rasterized"))
    });
    let non_ascii: Vec<&Letter> = letters
        .iter()
        .filter(|l| !(0x21..0x7F).contains(&(l.c as u32)))
        .collect();
    let non_ascii_letters = non_ascii.iter().map(|l| tok(l));

    let ascii_adv: Vec<u8> = (0x21u32..0x7F)
        .map(|u| {
            let c = char::from_u32(u).unwrap();
            letters.iter().find(|l| l.c == c).expect("ascii").advance
        })
        .collect();
    let extra_adv: Vec<TokenStream> = non_ascii
        .iter()
        .map(|l| {
            let (c, a) = (l.c, l.advance);
            quote!((#c, #a))
        })
        .collect();

    let font_expr = quote! {
        agb::display::font::Font::new(&[#(#ascii),*], &[#(#non_ascii_letters),*], #line_height, #ascent)
    };

    Ok(PackParts {
        font_expr,
        ascii_adv,
        extra_adv,
        line_height,
    })
}
