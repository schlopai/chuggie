//! `include_isobattle!("battle.tmj")` — bake a whole SRPG-style battlefield from a Tiled map at compile
//! time: the isometric floor image PLUS the per-cell data the grid-battle engine needs (terrain frame,
//! elevation, walkability) and the unit spawns, all registered with the engine so the game loads it
//! with one `isob_load(handle)`. No import script, no generated `.tish` data module — the Tiled files
//! ARE the source.
//!
//! **Height = stacked tile layers (the standard Tiled isometric pattern).** Instead of an abstract
//! `height` layer, elevation is built by stacking tile layers, each raised by its Tiled **vertical
//! offset** (`offsety`): a full block is 16px, a half block 8px. This composites every layer at its
//! offset — so the baked floor is pixel-identical to what Tiled shows — and a cell's elevation is
//! simply the offset of its top tile (in 8px units: full = 2, half = 1). You SEE the towers while
//! editing, and the game matches. Walkability is a `walkable = false` tile property on the terrain
//! tileset; unit spawns come from a `units` object layer (`cls`/`team` props, on the tile grid).
use image::{imageops, Rgba, RgbaImage};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

// iso projection — MUST match `packages/iso.tish`, which is the single tish-side source for all of
// this (ISO_TILE_HALF_W/H, ISO_LIFT_PER_STEP, and the origin the games read back via
// `isob_board_ox/oy`). The duplication is unavoidable: this code runs in a proc-macro at build time
// to composite the board image, so it cannot call into the tish package — but there are now exactly
// two copies rather than the previous four, and the games no longer hardcode the origin at all.
//
// Small boards (≤8×8 with classic margins) stay on a 256×256 canvas at ORIGIN (96,24).
// Larger boards expand to 512×512 with an origin that keeps every tile on-canvas.
// `packages/iso.tish` distinguishes the two by reading the origin back, so if these constants
// change, that inference changes with them.
const TILE: u32 = 32;
const HALF_W: i64 = 16;
const HALF_H: i64 = 8;
const ELEV_UNIT: i64 = 8; // one elevation "unit" = 8px (half block = 1, full block = 2)
const CLASSIC_OX: i64 = 96;
const CLASSIC_OY: i64 = 24;
const CLASSIC_CANVAS: u32 = 256;
const LARGE_CANVAS: u32 = 512;
const LARGE_OY: i64 = 48; // headroom for raised stacks above the north tip

/// Quantise so every 8x8 tile draws from one of at most 16 palette banks, keeping as much colour
/// as that allows.
///
/// Tries progressively harsher global quantisation and stops at the first level whose per-tile
/// palettes pack into 16 banks of 15 (+transparent). agb's own packer is stricter than a greedy
/// estimate, so the check here is deliberately pessimistic — first-fit, largest palette first.
fn quantize_to_banks(img: &mut RgbaImage) {
    for &n in &[240usize, 180, 120, 90, 64, 48, 32, 24, 15] {
        let mut trial = img.clone();
        crate::isoboard::quantize_in_place(&mut trial, n);
        fit_4bpp_tiles(&mut trial, 15);
        // ⚠️ 4, not 16, and the margin is measured. agb's palette optimiser is far stricter than
        // a first-fit estimate: at a level this check scored 16 banks it reported 25-30 and refused
        // to build. Targeting a quarter of the hardware limit is what actually clears its packer.
        if banks_needed(&trial) <= 4 {
            *img = trial;
            return;
        }
    }
    crate::isoboard::quantize_in_place(img, 15);
    fit_4bpp_tiles(img, 15);
}

/// How many 15-colour banks the image's per-tile palettes need, first-fit.
fn banks_needed(img: &RgbaImage) -> usize {
    use std::collections::BTreeSet;
    let (w, h) = (img.width(), img.height());
    let mut pals: Vec<BTreeSet<[u8; 4]>> = Vec::new();
    for ty in (0..h).step_by(8) {
        for tx in (0..w).step_by(8) {
            let mut set = BTreeSet::new();
            for y in ty..(ty + 8).min(h) {
                for x in tx..(tx + 8).min(w) {
                    let p = img.get_pixel(x, y).0;
                    if p[3] >= 128 {
                        set.insert(p);
                    }
                }
            }
            if !set.is_empty() && !pals.contains(&set) {
                pals.push(set);
            }
        }
    }
    pals.sort_by_key(|p| core::cmp::Reverse(p.len()));
    let mut banks: Vec<BTreeSet<[u8; 4]>> = Vec::new();
    for p in pals {
        let mut placed = false;
        for b in banks.iter_mut() {
            if b.union(&p).count() <= 15 {
                b.extend(p.iter().copied());
                placed = true;
                break;
            }
        }
        if !placed {
            banks.push(p);
        }
    }
    banks.len()
}

/// Force every 8x8 tile to at most `cap` colours, so agb's 4bpp packer can give each one a palette.
///
/// Keeps the 16 most-used colours in the tile and remaps the rest to the nearest survivor. Only
/// tiles that actually exceed the limit are touched, so on real board art this rewrites a handful of
/// pixels out of a quarter-million.
fn fit_4bpp_tiles(img: &mut RgbaImage, cap: usize) {
    use std::collections::HashMap;
    let (w, h) = (img.width(), img.height());
    for ty in (0..h).step_by(8) {
        for tx in (0..w).step_by(8) {
            let mut count: HashMap<[u8; 4], u32> = HashMap::new();
            for y in ty..(ty + 8).min(h) {
                for x in tx..(tx + 8).min(w) {
                    *count.entry(img.get_pixel(x, y).0).or_insert(0) += 1;
                }
            }
            if count.len() <= cap {
                continue;
            }
            let mut ranked: Vec<([u8; 4], u32)> = count.into_iter().collect();
            ranked.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
            let keep: Vec<[u8; 4]> = ranked.iter().take(cap).map(|(c, _)| *c).collect();
            for y in ty..(ty + 8).min(h) {
                for x in tx..(tx + 8).min(w) {
                    let p = img.get_pixel(x, y).0;
                    if keep.contains(&p) {
                        continue;
                    }
                    let best = keep
                        .iter()
                        .min_by_key(|k| {
                            let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
                            d(k[0], p[0]) + d(k[1], p[1]) + d(k[2], p[2]) + d(k[3], p[3])
                        })
                        .copied()
                        .unwrap_or(p);
                    img.put_pixel(x, y, Rgba(best));
                }
            }
        }
    }
}

// ── The direct-upload path: ship the cartridge's own background, not a picture of it ────────────
//
// A `.tmj` that carries a `banks` map property does not get composited at all. Instead the
// background is the 64x64 tilemap the real game built in VRAM, dumped by
// the board-banks packer script: its tiles, its 16 palette banks, and its 4096 screen entries,
// uploaded byte for byte.
//
// **Why, in one measurement.** Compositing tile art at an imported board's 2px elevation lands blocks off the 8px
// tile grid, so nearly every composited tile straddles two source tiles that used *different*
// palette banks. Art with five real banks comes out the other side with 289-621 arbitrary per-tile
// palettes, and no packer fits those into the hardware's 16 — agb's reports 44 bins, plain first-fit
// is worse. Everything downstream then had to choose which half of the art to destroy: quantise and
// lose the colour, or go 8bpp and block-reduce until a schoolhouse becomes a staircase of repeated
// roofs. Board 6 needed 898 atlas tiles (28.1 KB) against ~25 KB of usable BG VRAM.
//
// The cartridge's own map has the same board at **399 tiles / 12.5 KB, six banks, full colour** —
// because it never left the grid. Across the eleven boards checked, the direct path is 6.4-19.0 KB
// where the composite was 7.0-28.1 KB, and not one of them needs reducing.
//
// This is also why elevation stops mattering to rendering here: the map already says where every
// tile goes, so there is no 2px offset left to misalign. The `.tmj` keeps its job as the source of
// GAMEPLAY data — heights, walkability, spawns — and stops being the source of pixels.
struct Banks {
    /// 4bpp tile graphics, 32 bytes each, index 0 blank. Compacted, and SHARED by both layers.
    tiles: Vec<u8>,
    /// 16 banks of 16 GBA BGR555 colours.
    palettes: Vec<u16>,
    /// The BASE layer's screen entries, row-major over 64x32:
    /// `tile | hflip<<10 | vflip<<11 | bank<<12`.
    entries: Vec<u16>,
    /// The FOREGROUND layer's, same shape — mounds, trees, chimneys, drawn in front of units.
    fg: Vec<u16>,
    rows: usize,
    cols: usize,
    /// Where cell (0,0) sits in this map, measured against THIS dump. See `place()` in
    /// the board-banks packer script — the composite path's `board_layout` describes a canvas the
    /// real game never used, so it cannot supply this.
    ox: i64,
    oy: i64,
    /// One BGR555 backdrop colour per visible scanline, or empty. An imported board's sky can be a vertical ramp, and
    /// the GBA backdrop is a single palette word, so reproducing it means rewriting that word every
    /// scanline — see `native_sky_set` in tish-agb.
    sky: Vec<u16>,
}

const BANK_HDR: usize = 16; // "FBN2" + u16 tile_count + i16 ox + i16 oy + u16 sky + u16 rows + u16 cols
const BANK_PAL: usize = 16 * 16 * 2;

impl Banks {
    /// Parse a `.banks` blob written by the board-banks packer script's `pack`.
    fn parse(raw: &[u8]) -> Result<Banks, String> {
        // ⚠️ `FBN2`, and a stale `FBNK` must be rejected rather than adapted. The old format's 4096
        // entries were ONE 64x64 map with the foreground folded into its bottom half — accepting it
        // would build a board that renders convincingly and is missing every mound, tree and chimney.
        if raw.len() < BANK_HDR + BANK_PAL || &raw[..4] != b"FBN2" {
            return Err(
                "not a .banks blob — expected FBN2 (re-run the board-banks packer script; \
                        the one-layer FBNK format is gone)"
                    .into(),
            );
        }
        let u16s = |off: usize, n: usize| -> Vec<u16> {
            (0..n)
                .map(|i| u16::from_le_bytes([raw[off + i * 2], raw[off + i * 2 + 1]]))
                .collect()
        };
        let hw = |at: usize| u16::from_le_bytes([raw[at], raw[at + 1]]) as usize;
        let (count, sky_n, rows, cols) = (hw(4), hw(10), hw(12), hw(14));
        let cells = rows * cols;
        let tiles_at = BANK_HDR + BANK_PAL + 2 * cells * 2;
        let want = tiles_at + count * 32 + sky_n * 2;
        if rows == 0 || cols == 0 || raw.len() != want {
            return Err(format!(
                "header says {cols}x{rows} x2 layers + {count} tiles + {sky_n} sky lines \
                 ({want} bytes) but the blob has {}",
                raw.len()
            ));
        }
        Ok(Banks {
            palettes: u16s(BANK_HDR, 16 * 16),
            entries: u16s(BANK_HDR + BANK_PAL, cells),
            fg: u16s(BANK_HDR + BANK_PAL + cells * 2, cells),
            tiles: raw[tiles_at..tiles_at + count * 32].to_vec(),
            ox: i16::from_le_bytes([raw[6], raw[7]]) as i64,
            oy: i16::from_le_bytes([raw[8], raw[9]]) as i64,
            sky: u16s(tiles_at + count * 32, sky_n),
            rows,
            cols,
        })
    }
}

/// Emit the `board_mod`-shaped statics for a directly-uploaded cartridge background.
///
/// Shaped to match what `include_background_gfx!` produces — `PALETTES` and a `TileData` named `bg`
/// — so `__isobattle_register` and everything downstream of it is identical on both paths.
fn direct_bg(b: &Banks) -> Result<proc_macro2::TokenStream, String> {
    use quote::quote;
    let cells = b.rows * b.cols;
    if b.entries.len() != cells || b.fg.len() != cells {
        return Err(format!("banks: expected {cells} entries per layer"));
    }
    if b.palettes.len() != 16 * 16 {
        return Err("banks: expected 16 palettes of 16 colours".into());
    }
    if !b.tiles.len().is_multiple_of(32) {
        return Err(format!(
            "banks: tile blob is {} bytes, not a multiple of 32",
            b.tiles.len()
        ));
    }
    // ⚠️ Advisory, not fatal. ~25 KB is what a board can have once the HUD and the screenblocks are
    // paid for, but the real ceiling depends on the game, so this reports rather than refuses.
    let kb = b.tiles.len() as f64 / 1024.0;
    if b.tiles.len() > 25_600 {
        eprintln!("tish_gba_scenepack: warning: board tiles are {kb:.1} KB — may not fit BG VRAM");
    }

    // Byte strings, not arrays of integer literals: two 2048-entry maps and ~14 KB of tiles as
    // tokens is several hundred thousand tokens a board, and a game carries 162 of them.
    let tiles_lit = proc_macro2::Literal::byte_string(&b.tiles);
    let pal_lit = b.palettes.chunks(16).map(|p| {
        let c = p.iter().map(|&v| quote!(agb::display::Rgb15(#v)));
        quote!(agb::display::Palette16::new([#(#c),*]))
    });
    let bytes = |ents: &[u16]| {
        let mut v = Vec::with_capacity(ents.len() * 2);
        for e in ents {
            v.extend_from_slice(&e.to_le_bytes());
        }
        proc_macro2::Literal::byte_string(&v)
    };
    let base_lit = bytes(&b.entries);
    let fore_lit = bytes(&b.fg);
    let (rows, cols) = (b.rows, b.cols);

    // Both layers share ONE tile set and ONE set of palette banks — that is how the cartridge holds
    // them, and it is why the foreground costs only its own screen entries (4 KB) rather than a
    // second copy of the art.
    Ok(quote! {
        mod board_mod {
            use agb::display::tiled::{TileEffect, TileFormat, TileSet, TileSetting};

            static TILES: &[u8] = agb::align_bytes!(u32, #tiles_lit);
            const BASE: &[u8] = #base_lit;
            const FORE: &[u8] = #fore_lit;
            const CELLS: usize = #rows * #cols;

            /// Unpack raw screen entries into `TileSetting`s at compile time.
            ///
            /// A `const fn` loop rather than thousands of emitted constructor calls — same output, a
            /// fraction of the tokens, and the parse cost is what makes 162 boards in one ROM
            /// affordable.
            const fn settings(src: &[u8]) -> [TileSetting; CELLS] {
                let mut out = [TileSetting::BLANK; CELLS];
                let mut i = 0;
                while i < CELLS {
                    let v = src[i * 2] as u16 | ((src[i * 2 + 1] as u16) << 8);
                    out[i] = TileSetting::new(
                        v & 0x3FF,
                        TileEffect::new(
                            (v >> 10) & 1 == 1,
                            (v >> 11) & 1 == 1,
                            ((v >> 12) & 0xF) as u8,
                        ),
                    );
                    i += 1;
                }
                out
            }
            static BASE_SET: [TileSetting; CELLS] = settings(BASE);
            static FORE_SET: [TileSetting; CELLS] = settings(FORE);

            pub static PALETTES: &[agb::display::Palette16] = &[#(#pal_lit),*];
            // SAFETY: `align_bytes!(u32, ..)` gives the 4-byte alignment `TileSet::new` requires, and
            // the length is checked to be a multiple of the 4bpp tile size before this is emitted.
            pub static bg: agb::display::tile_data::TileData = agb::display::tile_data::TileData::new(
                unsafe { TileSet::new(TILES, TileFormat::FourBpp) },
                &BASE_SET,
                #cols,
                #rows,
            );
            /// The scenery layer. Same tiles, same palettes, its own map.
            pub static fg: agb::display::tile_data::TileData = agb::display::tile_data::TileData::new(
                unsafe { TileSet::new(TILES, TileFormat::FourBpp) },
                &FORE_SET,
                #cols,
                #rows,
            );
        }
    })
}

/// Pick canvas size + origin so every cell of a `w×h` iso board lands on the image.
fn board_layout(w: i32, h: i32) -> (u32, i64, i64) {
    let left = CLASSIC_OX - (h as i64 - 1) * HALF_W;
    let right = CLASSIC_OX + (w as i64 - 1) * HALF_W + TILE as i64;
    let top = CLASSIC_OY - 48; // ~3 full blocks of raise
    let bottom = CLASSIC_OY + (w as i64 + h as i64 - 2) * HALF_H + TILE as i64;
    if left >= 0 && right <= CLASSIC_CANVAS as i64 && top >= 0 && bottom <= CLASSIC_CANVAS as i64 {
        return (CLASSIC_CANVAS, CLASSIC_OX, CLASSIC_OY);
    }
    let ox = (h as i64 - 1) * HALF_W;
    (LARGE_CANVAS, ox, LARGE_OY)
}

#[derive(Deserialize)]
struct Tmj {
    width: i32,
    height: i32,
    #[serde(default)]
    tilewidth: i32,
    tilesets: Vec<TsRef>,
    layers: Vec<Layer>,
    /// Map-level custom properties. `banks` selects the direct-upload path (see `direct_bg`);
    /// `vram_ox`/`vram_oy` are where cell (0,0) sits in the cartridge's own 64x64 map.
    #[serde(default)]
    properties: Vec<Prop>,
}
#[derive(Deserialize)]
struct TsRef {
    firstgid: i32,
    source: Option<String>,
}
#[derive(Deserialize)]
struct Layer {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: Vec<i64>,
    #[serde(default)]
    offsety: f64,
    #[serde(default)]
    objects: Vec<Obj>,
    #[serde(default)]
    properties: Vec<Prop>,
}

impl Layer {
    /// This level's gameplay elevation.
    ///
    /// Normally derived from the layer's vertical offset — a full block raises art 16px and counts
    /// as 2, a half block 8px and counts as 1. But the two are not always the same number: art
    /// imported from external board data raises a level by whatever that data chose, which need not be
    /// 8px-per-unit. A layer may then declare `elev` explicitly, and the offset stays purely
    /// visual — composite where the art belongs, walk and jump on the declared height.
    fn elevation(&self) -> i64 {
        match prop(&self.properties, "elev").and_then(|v| v.as_i64()) {
            Some(e) => e,
            None => (-self.offsety.round() as i64) / ELEV_UNIT,
        }
    }
}
#[derive(Deserialize)]
struct Obj {
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    properties: Vec<Prop>,
}
#[derive(Deserialize)]
struct Prop {
    name: String,
    value: serde_json::Value,
}
#[derive(Deserialize)]
struct TsjImg {
    image: String,
    columns: i32,
}
#[derive(Deserialize)]
struct TsjProps {
    #[serde(default)]
    tiles: Vec<TsjTile>,
}
#[derive(Deserialize)]
struct TsjTile {
    id: i32,
    #[serde(default)]
    properties: Vec<Prop>,
}

const FLIP: i64 = !(0xE000_0000u32 as i64); // strip Tiled's flip/rotation bits

fn prop<'a>(props: &'a [Prop], name: &str) -> Option<&'a serde_json::Value> {
    props.iter().find(|p| p.name == name).map(|p| &p.value)
}

pub fn build(tmj_path: &Path) -> Result<proc_macro2::TokenStream, String> {
    use quote::quote;
    let dir = tmj_path.parent().ok_or("map path has no parent")?;
    let map: Tmj = serde_json::from_str(
        &std::fs::read_to_string(tmj_path)
            .map_err(|e| format!("reading {}: {e}", tmj_path.display()))?,
    )
    .map_err(|e| format!("parsing {}: {e}", tmj_path.display()))?;
    let (w, h) = (map.width, map.height);
    let n = (w * h) as usize;
    let tw = if map.tilewidth > 0 { map.tilewidth } else { 32 };

    // terrain tileset: the lowest-firstgid one; load its image + columns.
    let terr = map
        .tilesets
        .iter()
        .min_by_key(|t| t.firstgid)
        .ok_or("map has no tileset")?;
    let terr_first = terr.firstgid as i64;
    let src = terr
        .source
        .as_ref()
        .ok_or("terrain tileset must be an external .tsj")?;
    let tsj: TsjImg = serde_json::from_str(
        &std::fs::read_to_string(dir.join(src)).map_err(|e| format!("reading {src}: {e}"))?,
    )
    .map_err(|e| format!("parsing {src}: {e}"))?;
    let tsheet = image::open(dir.join(src).parent().unwrap().join(&tsj.image))
        .map_err(|e| format!("opening tileset {}: {e}", tsj.image))?
        .to_rgba8();
    let tcols = tsj.columns.max(1) as i64;

    // walkability: terrain tiles marked `walkable = false`.
    let mut unwalkable: HashSet<i64> = HashSet::new();
    if let Ok(props) = serde_json::from_str::<TsjProps>(
        &std::fs::read_to_string(dir.join(src)).unwrap_or_default(),
    ) {
        for t in &props.tiles {
            if prop(&t.properties, "walkable").and_then(|v| v.as_bool()) == Some(false) {
                unwalkable.insert(t.id as i64);
            }
        }
    }

    // Every tile layer is a stack level; its `offsety` (px, negative = raised) is its elevation —
    // unless it declares an `elev` property, or is named `over*`.
    //
    // An `over*` layer is ART ONLY: the part of a tall object (a roof, an upper storey) that lives
    // above the diamond of the cell it stands on and so cannot fit in that cell's own block. It is
    // composited, and nothing else — it contributes no elevation, no terrain frame, no walkability
    // and no occluder. Without that exclusion a building's ridgeline wins the per-cell "top tile"
    // vote and becomes the surface units walk on.
    let tile_layers: Vec<&Layer> = map
        .layers
        .iter()
        .filter(|l| l.kind == "tilelayer")
        .collect();

    // ---- compose the board: every (cell, layer) tile drawn at its offset, painter's order ----
    // Sort by (col+row) back-to-front, then by offset so lower stack tiles draw before higher ones.
    let mut items: Vec<(i32, i32, i64, i64)> = Vec::new(); // (col, row, offsety, local_frame)
    for l in &tile_layers {
        let off = l.offsety.round() as i64;
        for i in 0..n {
            let g = l.data.get(i).copied().unwrap_or(0) & FLIP;
            if g > 0 {
                items.push((i as i32 % w, i as i32 / w, off, g - terr_first));
            }
        }
    }
    items.sort_by(|a, b| (a.0 + a.1).cmp(&(b.0 + b.1)).then(b.2.cmp(&a.2)));

    // The px the ART rises per elevation unit, taken from a layer that declares both. Normally 8,
    // but an imported board uses whatever its source chose (the SRPG demos: 2). The engine lifts
    // UNITS by this same number — otherwise a unit stands `8 - lift` px per level above its own
    // ground, which a flat board cannot show and a terraced one shows everywhere.
    let lift = tile_layers
        .iter()
        .filter(|l| !l.name.starts_with("over"))
        .find_map(|l| {
            let e = l.elevation();
            let off = -l.offsety.round() as i64;
            if e > 0 && off > 0 {
                Some((off / e) as i32)
            } else {
                None
            }
        })
        .unwrap_or(ELEV_UNIT as i32);

    // ---- the background: the cartridge's own map when the .tmj offers one, else a composite ----
    //
    // Two paths, and which one runs is decided entirely by whether the map declares a `banks`
    // property. Hand-authored Tiled boards (the isoboard SRPG examples and friends) draw their art on the 8px grid
    // and composite perfectly, so they keep the original path unchanged. Imported boards
    // carry `banks` and take the direct one — see the block comment above `Banks` for the measurement
    // that makes the difference, and why compositing cannot be fixed in place.
    let mut sky: Vec<u16> = Vec::new();
    let (mapw, maph): (i32, i32);
    // Right/bottom edge of the PAINTED content, which is what the camera must be able to reach.
    let (cw, ch): (i32, i32);
    // A directly-uploaded board has a second, FOREGROUND background; a composited one does not.
    // Both arms produce the same `__isobattle_register` shape, so nothing downstream branches.
    let mut reg_fg = quote! { tish_gba_game_engine::native_isoboard_register_bg(&BOARD, bg) };
    let (bg_tokens, origin_x, origin_y) = match prop(&map.properties, "banks")
        .and_then(|v| v.as_str())
    {
        Some(rel) => {
            let bpath = dir.join(rel);
            let b = Banks::parse(
                &std::fs::read(&bpath).map_err(|e| format!("reading {}: {e}", bpath.display()))?,
            )
            .map_err(|e| format!("{}: {e}", bpath.display()))?;
            // ⚠️ The origin comes from the BLOB, not from `board_layout` and not from the `.tmj`.
            // The cartridge's camera put the board wherever it liked inside its 64x64 map, and only
            // the dump that produced these entries knows where cell (0,0) landed. Any other origin
            // draws the right art at the wrong offset, which reads as a scrambled board rather than
            // as an offset one.
            let (ox, oy) = (b.ox, b.oy);
            sky = b.sky.clone();
            mapw = (b.cols * 8) as i32;
            maph = (b.rows * 8) as i32;
            // ⚠️ Measure it; do not derive it from the cell diamond. Block faces, overhangs and
            // decorations all paint PAST the last cell — computing `ox + (w-1)*16 + 32` left the
            // camera short of the real edge on 106 of 162 boards, which crops the board rather than
            // looking like a bad limit.
            let (mut mx, mut my) = (0usize, 0usize);
            for (i, e) in b.entries.iter().chain(b.fg.iter()).enumerate() {
                if e & 0x3FF != 0 {
                    let cell = i % (b.rows * b.cols);
                    mx = mx.max(cell % b.cols);
                    my = my.max(cell / b.cols);
                }
            }
            cw = ((mx + 1) * 8) as i32;
            ch = ((my + 1) * 8) as i32;
            // ⚠️ `tish_agb::native_fg_register`, NOT `__asset_register_bg`. The shared runtime's
            // table holds 256 backgrounds; two layers per board means 162 boards want 324, and past
            // the cap registration returns -1 and `bg_new` panics on the 129th board. The foreground
            // table is separate and doubles the ceiling.
            reg_fg = quote! {
                let fg = tish_agb::native_fg_register((board_mod::PALETTES, &board_mod::fg));
                tish_gba_game_engine::native_isoboard_register_bg2(&BOARD, bg, fg)
            };
            (direct_bg(&b)?, ox, oy)
        }
        None => {
            let (canvas_sz, origin_x, origin_y) = board_layout(w, h);
            let mut canvas = RgbaImage::from_pixel(canvas_sz, canvas_sz, Rgba([0, 0, 0, 255]));
            for (c, r, off, local) in &items {
                let (sc, sr) = ((local % tcols) as u32, (local / tcols) as u32);
                let tile = imageops::crop_imm(&tsheet, sc * TILE, sr * TILE, TILE, TILE).to_image();
                let x = origin_x + (*c as i64 - *r as i64) * HALF_W;
                let y = origin_y + (*c as i64 + *r as i64) * HALF_H + off;
                imageops::overlay(&mut canvas, &tile, x, y);
            }
            // 4bpp with a per-tile palette, not 8bpp: imported GBA board art is 4bpp with a different
            // 16-colour bank per tile, so 8bpp paid double for colours the art never had — and at 64
            // bytes a tile the boards did not fit VRAM, which forced a block reduction that turned a
            // schoolhouse into a staircase of repeated roofs.
            //
            // The quantise is the cost of compositing: it keeps full geometry and gives up colour.
            // Boards that can take the direct path above pay neither.
            quantize_to_banks(&mut canvas);
            mapw = canvas_sz as i32;
            maph = canvas_sz as i32;
            cw = canvas_sz as i32;
            ch = canvas_sz as i32;
            let stem = tmj_path.file_stem().unwrap().to_string_lossy().to_string();
            let atlas_path = dir.join(format!("{stem}.board.png"));
            canvas
                .save(&atlas_path)
                .map_err(|e| format!("writing {}: {e}", atlas_path.display()))?;
            let atlas = atlas_path.to_string_lossy().to_string();
            (
                quote! { agb::include_background_gfx!(mod board_mod, bg => 16 deduplicate #atlas); },
                origin_x,
                origin_y,
            )
        }
    };

    // ---- per-cell data: the TOP tile (highest = most-negative offset) gives the frame, elevation
    // (−offset in 8px units), and walkability. ----
    let mut frames = vec![0u8; n];
    let mut heights = vec![0u8; n];
    let mut walk = vec![1u8; n];
    for i in 0..n {
        let mut top_off = 1; // positive sentinel = nothing yet (offsets are ≤ 0)
        let mut top_local = 0i64;
        let mut top_elev = 0i64;
        for l in &tile_layers {
            if l.name.starts_with("over") {
                continue; // art-only overhang (see the `over*` note above tile_layers)
            }
            let g = l.data.get(i).copied().unwrap_or(0) & FLIP;
            let off = l.offsety.round() as i64;
            if g > 0 && off < top_off {
                top_off = off;
                top_local = g - terr_first;
                top_elev = l.elevation();
            }
        }
        if top_off <= 0 {
            frames[i] = top_local.clamp(0, 255) as u8;
            heights[i] = top_elev.clamp(0, 255) as u8;
            if unwalkable.contains(&top_local) {
                walk[i] = 0;
            }
        }
        // ⚠️ A cell carrying an OVERHANG is not standable. `over*` layers hold the part of a tall
        // object that does not fit its own cell — a roof, an upper storey — so a cell that has one
        // has a building on it, not a floor.
        //
        // This is not cosmetic. The floor is baked into ONE background image and units are sprites,
        // and a sprite always draws above the background: a unit placed on a building cell is
        // painted over its roof, and the move-range diamond drawn at that cell's base hangs in the
        // air against the wall. Both read as an elevation bug and neither is one — the unit is
        // exactly where the grid says, on a tile that should never have accepted it.
        if map.layers.iter().any(|l| {
            l.kind == "tilelayer"
                && l.name.starts_with("over")
                && (l.data.get(i).copied().unwrap_or(0) & FLIP) > 0
        }) {
            walk[i] = 0;
        }
    }

    // Per-cell RAISED stack (every tile in an offset<0 layer), CSR-packed: each entry is that block's
    // TOP elevation (8px units) and its OWN tile frame. The render draws one occluder sprite per entry,
    // so a tower shows each level's real tile (mixed half/full blocks and mixed tiles), not just the top.
    let mut stack_off: Vec<u16> = Vec::with_capacity(n + 1);
    let mut stack_elev: Vec<u8> = Vec::new();
    let mut stack_tile: Vec<u8> = Vec::new();
    stack_off.push(0);
    for i in 0..n {
        let mut entries: Vec<(u8, u8)> = Vec::new();
        for l in &tile_layers {
            if l.name.starts_with("over") {
                continue;
            }
            let off = l.offsety.round() as i64;
            if off >= 0 {
                continue; // ground / non-raised → already in the baked background
            }
            let g = l.data.get(i).copied().unwrap_or(0) & FLIP;
            if g > 0 {
                let elev = l.elevation().clamp(0, 255) as u8;
                let tile = (g - terr_first).clamp(0, 255) as u8;
                entries.push((elev, tile));
            }
        }
        entries.sort_by_key(|&(e, _)| e); // bottom → top
        for (e, t) in entries {
            stack_elev.push(e);
            stack_tile.push(t);
        }
        stack_off.push(stack_elev.len() as u16);
    }

    // unit spawns from the `units` object layer (objects on the tile grid; `cls`/`team` props).
    let mut spawns: Vec<(u8, u8, u8, u8)> = Vec::new();
    if let Some(ul) = map
        .layers
        .iter()
        .find(|l| l.kind == "objectgroup" && l.name.eq_ignore_ascii_case("units"))
    {
        for o in &ul.objects {
            let col = (o.x / tw as f64).round().clamp(0.0, 255.0) as u8;
            let row = (o.y / tw as f64).round().clamp(0.0, 255.0) as u8;
            let cls = prop(&o.properties, "cls")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .clamp(0, 255) as u8;
            let team = prop(&o.properties, "team")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .clamp(0, 255) as u8;
            spawns.push((col, row, cls, team));
        }
    }

    let frames_lit = frames.iter().map(|&b| quote!(#b));
    let heights_lit = heights.iter().map(|&b| quote!(#b));
    let walk_lit = walk.iter().map(|&b| quote!(#b));
    let stack_off_lit = stack_off.iter().map(|&x| quote!(#x));
    let stack_elev_lit = stack_elev.iter().map(|&x| quote!(#x));
    let stack_tile_lit = stack_tile.iter().map(|&x| quote!(#x));
    let spawn_lit = spawns
        .iter()
        .map(|&(c, r, cl, t)| quote!((#c, #r, #cl, #t)));
    let sky_lit = sky.iter().map(|&v| quote!(#v));
    let ox = origin_x as i32;
    let oy = origin_y as i32;
    Ok(quote! {
        // Either an `include_background_gfx!` of the composited atlas, or the cartridge's own
        // tiles/palettes/screen-entries. Both define a `board_mod` with `PALETTES` and `bg`, so
        // everything below this line is the same on both paths.
        #bg_tokens
        static FRAMES: &[u8] = &[#(#frames_lit),*];
        static HEIGHTS: &[u8] = &[#(#heights_lit),*];
        static WALK: &[u8] = &[#(#walk_lit),*];
        static STACK_OFF: &[u16] = &[#(#stack_off_lit),*];
        static STACK_ELEV: &[u8] = &[#(#stack_elev_lit),*];
        static STACK_TILE: &[u8] = &[#(#stack_tile_lit),*];
        static SPAWNS: &[(u8, u8, u8, u8)] = &[#(#spawn_lit),*];
        /// One backdrop colour per scanline; empty on boards with no captured sky.
        static SKY: &[u16] = &[#(#sky_lit),*];
        // ⚠️ The board is a `static`, and the registry stores a REFERENCE to it. It used to be
        // built on the stack and pushed into a `Vec`, which meant N boards cost N heap pushes
        // during module init — the fragmentation that stopped a game carrying more than a handful.
        // Everything in it is already `&'static` or a plain integer, so a `static` costs nothing.
        static BOARD: tish_gba_game_engine::IsoBoard = tish_gba_game_engine::IsoBoard {
            bg: 0, w: #w, h: #h, ox: #ox, oy: #oy, lift: #lift,
            mapw: #mapw, maph: #maph, cw: #cw, ch: #ch,
            frames: FRAMES, heights: HEIGHTS, walk: WALK,
            stack_off: STACK_OFF, stack_elev: STACK_ELEV, stack_tile: STACK_TILE, spawns: SPAWNS,
            sky: SKY,
        };

        pub fn __isobattle_register() -> i32 {
            let bg = tishlang_runtime::gba::__asset_register_bg((board_mod::PALETTES, &board_mod::bg));
            #reg_fg
        }
    })
}
