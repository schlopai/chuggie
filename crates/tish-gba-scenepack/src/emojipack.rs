//! SELECTIVE emoji baking. The SerenityOS emoji set is ~1,750 tiny (~10px) pixel-art PNGs, one per
//! codepoint (`U+1F600.png`). This bakes ONLY the emoji a program actually uses — the tish compiler
//! passes the non-ASCII characters it collected from string literals (the same `{charset}` the font
//! baker gets), and we keep the ones that (a) are in an emoji codepoint range and (b) have a vendored
//! PNG. Those are composited into a single 16x16-per-frame sprite strip written as a sibling build
//! artifact, then handed to agb's own `include_aseprite_inner!` — so the emoji ride the exact same
//! colour-sprite path as any other `asset:` sheet (per-frame ≤15-colour palettes, VRAM dedup), no
//! COLR/CPAL font rasterization anywhere. A codepoint→frame table lets the text renderer draw the
//! right frame inline as a fallback wherever a font has no glyph for an emoji.
use proc_macro2::TokenStream;
use quote::quote;
use std::path::Path;

/// The pixel size of one emoji cell — a native GBA sprite size (S16x16). The ~10px SerenityOS art is
/// centred in it; the text baker reserves this same advance for an emoji codepoint so inline layout
/// lines up. Kept in sync with `EMOJI_PX` in `tish-agb`'s text renderer and `fontpack`.
const CELL: u32 = 16;

/// Is this codepoint one we treat as a picture emoji (⇒ draw the colour sprite) rather than a text
/// glyph? Deliberately EXCLUDES the ASCII-adjacent symbols a normal font legitimately carries
/// (©/®/™, arrows, punctuation) so importing emoji never hijacks ordinary text; it covers the
/// Miscellaneous-Symbols/Dingbats emoji people actually use plus the full pictographic planes.
pub fn is_emoji_cp(cp: u32) -> bool {
    (0x2600..=0x27BF).contains(&cp)      // ☀ ★ ✂ ✈ ❤ ➡ … (Misc Symbols + Dingbats)
        || (0x2B00..=0x2BFF).contains(&cp) // ⬆ ⭐ ⬛ …
        || (0x1F000..=0x1FAFF).contains(&cp) // 😀 🎮 🚀 … (Emoji, Supplemental/Symbols & Pictographs)
}

/// The vendored PNG filename for a codepoint, e.g. `U+1F600` / `U+A9` (uppercase hex, no zero-pad —
/// the SerenityOS naming). Sequence files (`U+..._U+...`) are intentionally not matched: v1 renders
/// single codepoints, so a ZWJ sequence degrades to its base characters.
fn png_name(cp: u32) -> String {
    format!("U+{cp:X}.png")
}

/// Build the emoji atlas for the used `charset`, writing the sprite strip next to `atlas_out` and
/// returning the tokens: an `include_aseprite_inner!` over the strip, a codepoint→frame table, and a
/// `__emoji_register()` the `emoji:` scheme calls. With no used emoji it emits an empty registration.
pub fn build(dir: &Path, charset: &str, atlas_out: &Path) -> Result<TokenStream, String> {
    if !dir.is_dir() {
        return Err(format!("emoji: '{}' is not a directory", dir.display()));
    }

    // The used emoji: charset chars that are emoji codepoints AND have a vendored PNG, sorted &
    // de-duplicated by codepoint (the table is binary-searched at runtime).
    let mut cps: Vec<u32> = charset
        .chars()
        .map(|c| c as u32)
        .filter(|&cp| is_emoji_cp(cp) && dir.join(png_name(cp)).is_file())
        .collect();
    cps.sort_unstable();
    cps.dedup();

    if cps.is_empty() {
        return Ok(quote! {
            pub fn __emoji_register() -> i32 {
                tishlang_runtime::gba::__asset_register_emoji(&[], &[])
            }
        });
    }

    // Composite each emoji, centred, into its 16x16 cell of a horizontal strip (frame i ↔ cps[i]).
    let n = cps.len() as u32;
    let mut strip = image::RgbaImage::new(CELL * n, CELL);
    for (i, &cp) in cps.iter().enumerate() {
        let path = dir.join(png_name(cp));
        let img = image::open(&path)
            .map_err(|e| format!("emoji: reading {}: {e}", path.display()))?
            .to_rgba8();
        let (w, h) = (img.width().min(CELL), img.height().min(CELL));
        let ox = i as u32 * CELL + (CELL - w) / 2; // centre horizontally
        let oy = (CELL - h) / 2; // centre vertically; the renderer nudges to the text baseline
        for y in 0..h {
            for x in 0..w {
                strip.put_pixel(ox + x, oy + y, *img.get_pixel(x, y));
            }
        }
    }
    strip
        .save(atlas_out)
        .map_err(|e| format!("emoji: writing {}: {e}", atlas_out.display()))?;

    let atlas_str = atlas_out.to_string_lossy().to_string();
    let entries = cps.iter().enumerate().map(|(i, &cp)| {
        let frame = i as u16;
        quote!((#cp, #frame))
    });

    Ok(quote! {
        agb::include_aseprite_inner!(16x16 #atlas_str);
        static __EMOJI_TABLE: &[(u32, u16)] = &[#(#entries),*];
        pub fn __emoji_register() -> i32 {
            tishlang_runtime::gba::__asset_register_emoji(SPRITES, __EMOJI_TABLE)
        }
    })
}
