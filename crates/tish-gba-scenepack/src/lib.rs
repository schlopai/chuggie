//! `include_scene!("scene")` — packs a scene into a GBA background at compile time, from EITHER a
//! compact **recipe** (`.json`: autotiled ground + prop-stamps from the vendored catalog — see
//! `recipe.rs`/`pack.rs`) or a **Tiled map** (`.tmj`: hand-drawn tile layers + per-tile collision
//! and/or a legacy `Collision` layer + a spawns object layer, multi-tileset — see `tiled.rs`). The
//! extension picks the path. Proc-macros always run on the *host*, so this crate is a normal std
//! crate (uses `image` + `serde_json`) despite being linked into a no_std thumbv4t game — no different
//! from how agb's own `include_background_gfx!`/`include_aseprite!` already do real image decoding
//! at build time.
//!
//! Reuses agb's proven graphics macro rather than reimplementing GBA tile/palette encoding: this
//! macro only does the "which pixels go where" packing (slice extraction, autotiling, atlas
//! layout), writes the result as sibling build-artifact files next to the recipe
//! (`<recipe>.atlas.png` / `<recipe>.map.bin` — regenerated every build, not hand-edited), and
//! then emits a nested `agb::include_background_gfx!` call over the packed atlas.
mod autotile;
mod chippack;
#[cfg(test)]
mod corpus_test;
mod deckpack;
mod emojipack;
mod fontpack;
mod isobattlepack;
mod isoboard;
mod pack;
#[cfg(test)]
mod profile_test;
mod recipe;
mod stringspack;
mod tiled;

use proc_macro::TokenStream;
use quote::quote;
use std::path::PathBuf;
use syn::{parse_macro_input, LitInt, LitStr, Token};

#[proc_macro]
pub fn include_scene(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let scene_path = PathBuf::from(lit.value());

    // `.tmj` (a Tiled JSON map) uses the Tiled importer; anything else is a compact recipe.
    let built = if scene_path.extension().and_then(|e| e.to_str()) == Some("tmj") {
        tiled::build(&scene_path)
    } else {
        pack::build(&scene_path)
    };
    match built {
        Ok(out) => {
            let atlas_path = out.atlas_path.to_string_lossy().to_string();
            let map_path = out.map_path.to_string_lossy().to_string();
            quote! {
                agb::include_background_gfx!(mod bg_mod, bg => deduplicate #atlas_path);
                static MAP_DATA: &[u8] = include_bytes!(#map_path);
                pub fn __scene_register() -> i32 {
                    // The scene's tileset is handed straight to tish-agb (NOT `__asset_register_bg`):
                    // that arena is reserved for `background:` imports, whose handles are compile-time
                    // indices that must match registration order (see tish-agb `SCENES`, #552).
                    let map_idx = tishlang_runtime::gba::__asset_register_map(MAP_DATA);
                    tish_agb::native_scene_register(bg_mod::PALETTES, &bg_mod::bg, map_idx)
                }
            }
            .into()
        }
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_font_used!("font.ttf", 16, "你好…")` — bake a `Font` with ONLY the printable ASCII plus
/// the given (used) non-ASCII characters, instead of every glyph in the TTF (see `fontpack.rs`). The
/// `font<N>:` scheme fills the third argument with the characters the tish compiler collected from the
/// program's string literals, so importing a full CJK font costs only the glyphs you actually show.
struct FontArgs {
    path: LitStr,
    size: LitInt,
    chars: LitStr,
}
impl syn::parse::Parse for FontArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let size: LitInt = input.parse()?;
        input.parse::<Token![,]>()?;
        let chars: LitStr = input.parse()?;
        Ok(FontArgs { path, size, chars })
    }
}

#[proc_macro]
pub fn include_font_used(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as FontArgs);
    let size: f32 = match args.size.base10_parse::<u32>() {
        Ok(n) => n as f32,
        Err(e) => return e.to_compile_error().into(),
    };
    match fontpack::build(&PathBuf::from(args.path.value()), size, &args.chars.value()) {
        Ok(ts) => ts.into(),
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_font_pack!("font.ttf", 16, "你好…")` — like [`include_font_used`], but expands to
/// `FONT` + parallel `METRICS` (`tish_agb::FontMetrics`) + `font()` / `metrics()` accessors so
/// menu measure can sum baked advances without agb `Layout`.
#[proc_macro]
pub fn include_font_pack(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as FontArgs);
    let size: f32 = match args.size.base10_parse::<u32>() {
        Ok(n) => n as f32,
        Err(e) => return e.to_compile_error().into(),
    };
    match fontpack::build_pack(&PathBuf::from(args.path.value()), size, &args.chars.value()) {
        Ok(ts) => ts.into(),
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_emoji_used!("assets/emoji/serenity", "😀❤…")` — bake ONLY the emoji this program uses into
/// one colour sprite strip (see `emojipack.rs`). The `emoji:` scheme fills the second argument with the
/// characters the tish compiler collected, so the ~1,750-emoji vendored set costs only the handful you
/// actually type. The strip is written into the generated crate (`CARGO_MANIFEST_DIR`) as a build
/// artifact; a codepoint→frame table lets the text renderer draw an emoji wherever a font lacks a glyph.
struct EmojiArgs {
    dir: LitStr,
    chars: LitStr,
}
impl syn::parse::Parse for EmojiArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let dir: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let chars: LitStr = input.parse()?;
        Ok(EmojiArgs { dir, chars })
    }
}

#[proc_macro]
pub fn include_emoji_used(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as EmojiArgs);
    // Write the atlas into the crate being compiled (unique per game build), not the shared vendored
    // dir — so different games baking different emoji sets never clobber each other's strip.
    // The stem must be a valid Rust identifier: agb's `include_aseprite_inner!` derives a default tag
    // constant from the filename (no aseprite tags in a PNG), so a leading dot / dashes would panic.
    let out_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let atlas_out = PathBuf::from(out_dir).join("tish_emoji_atlas.png");
    match emojipack::build(
        &PathBuf::from(args.dir.value()),
        &args.chars.value(),
        &atlas_out,
    ) {
        Ok(ts) => ts.into(),
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_isobattle!("battle.tmj")` — bake a whole SRPG-style battlefield (isoboard floor + per-cell
/// terrain/elevation/walkability + unit spawns) from a Tiled map at compile time (see `isobattlepack.rs`),
/// registering it with the engine so the game loads it with one `isob_load(handle)`. The `isobattle:`
/// import scheme calls the emitted `__isobattle_register()` and hands back the `i32` board handle.
#[proc_macro]
pub fn include_isobattle(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    match isobattlepack::build(&PathBuf::from(lit.value())) {
        Ok(ts) => ts.into(),
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_chip!("town.chip")` — compile a chiptune song's note data into static ROM data (see
/// `chippack.rs`). Emits a `__chip_register()` that the `chip:` import scheme calls to hand back an
/// `i32` handle for `chip_play`. The song is *notes*, so a minute of music costs a couple of
/// kilobytes instead of the megabyte the same minute costs as a WAV.
#[proc_macro]
pub fn include_chip(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    match chippack::build(&PathBuf::from(lit.value())) {
        Ok(ts) => ts.into(),
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_strings!("ui.strings")` — bake a multi-language string table into ROM. See
/// `stringspack.rs`; the format is `[lang]` sections whose line POSITION is the string id, and the
/// macro refuses to compile a file whose translations disagree on how many strings exist.
#[proc_macro]
pub fn include_strings(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    match stringspack::build(&PathBuf::from(lit.value())) {
        Ok(ts) => ts.into(),
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_deck!("theme.deck")` — bake a deck song (`gameBoyDmg` / `gbaDirectSound`) into a
/// `DeckSong` for `deck_play`. See `deckpack.rs` and `docs/deck.md`.
#[proc_macro]
pub fn include_deck(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    match deckpack::build(&PathBuf::from(lit.value())) {
        Ok(ts) => ts.into(),
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}

/// `include_isoboard!("battle.tmj")` — bake a static ISOMETRIC board (a `terrain` + `height` tile
/// layer) into ONE background at compile time, so the floor costs zero OBJs on device (see
/// `isoboard.rs`). Emits a `__isoboard_register()` the `isoboard:` import scheme calls to register the
/// background and hand back an `i32` handle for `bg_new`.
#[proc_macro]
pub fn include_isoboard(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let map_path = PathBuf::from(lit.value());
    match isoboard::build(&map_path) {
        Ok(out) => {
            let atlas_path = out.atlas_path.to_string_lossy().to_string();
            quote! {
                agb::include_background_gfx!(mod board_mod, bg => deduplicate #atlas_path);
                pub fn __isoboard_register() -> i32 {
                    tishlang_runtime::gba::__asset_register_bg((board_mod::PALETTES, &board_mod::bg))
                }
            }
            .into()
        }
        Err(e) => {
            let msg = format!("tish_gba_scenepack: {e}");
            quote! { compile_error!(#msg); }.into()
        }
    }
}
