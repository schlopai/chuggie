//! Bake a static ISOMETRIC board (a Tiled `.tmj` with a `terrain` tile layer + a `height` tile layer)
//! into a single background image at COMPILE TIME. Isometric terrain is drawn back-to-front with the
//! painter's algorithm; doing that with one OBJ per tile burns ~64 sprites and blows the GBA's
//! per-scanline OBJ budget. Since the board never moves it belongs on a background layer — so this
//! composes every tile (at its real elevation) into one image the game shows as a background. Raised
//! tiles that must OCCLUDE units stay as depth-sorted sprites in the game (this only handles the
//! static floor/base render). Output is one sibling `<map>.board.png` (a regenerated build artifact,
//! not hand-edited), fed to agb's `include_background_gfx!` exactly like every other baked asset.
//!
//! The projection matches the grid-battle engine's on-device iso projection so units line up with the
//! baked terrain: 32×32 tiles, a (±16,+8) step, and a 6px lift per elevation step, over a screen-space
//! origin of (96,24). Terrain that uses a different projection would pass those in later; for now they
//! are the isoboard standard.
use crate::pack::Output;
use image::{imageops, Rgba, RgbaImage};
use serde::Deserialize;
use std::path::Path;

// iso projection — MUST match the isoboard SRPG examples (now in the chuggie-tactics repo)
const TILE: u32 = 32;
const ORIGIN_X: i64 = 96;
const ORIGIN_Y: i64 = 24;
const HALF_W: i64 = 16;
const HALF_H: i64 = 8;
const LIFT: i64 = 6;
const CANVAS: u32 = 256; // a GBA regular background is 256×256
const MAX_COLOURS: usize = 15; // 4bpp: 15 + transparent, quantized to a single shared palette

#[derive(Deserialize)]
struct TmjMap {
    width: i32,
    height: i32,
    tilesets: Vec<TilesetRef>,
    layers: Vec<Layer>,
}
#[derive(Deserialize)]
struct TilesetRef {
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
}
#[derive(Deserialize)]
struct Tsj {
    image: String,
    columns: i32,
}

const FLIP_MASK: i64 = !(0xE000_0000u32 as i64);

fn layer<'a>(map: &'a TmjMap, name: &str) -> Option<&'a Layer> {
    map.layers
        .iter()
        .find(|l| l.kind == "tilelayer" && l.name.eq_ignore_ascii_case(name))
}

pub fn build(tmj_path: &Path) -> Result<Output, String> {
    let dir = tmj_path.parent().ok_or("map path has no parent")?;
    let map: TmjMap = serde_json::from_str(
        &std::fs::read_to_string(tmj_path)
            .map_err(|e| format!("reading {}: {e}", tmj_path.display()))?,
    )
    .map_err(|e| format!("parsing {}: {e}", tmj_path.display()))?;
    let (w, h) = (map.width, map.height);

    // the FIRST tileset is the terrain sheet (grass/water/etc.); load its image + column count.
    let terr = map
        .tilesets
        .iter()
        .min_by_key(|t| t.firstgid)
        .ok_or("map has no tileset")?;
    let src = terr
        .source
        .as_ref()
        .ok_or("terrain tileset must be an external .tsj")?;
    let tsj: Tsj = serde_json::from_str(
        &std::fs::read_to_string(dir.join(src)).map_err(|e| format!("reading {src}: {e}"))?,
    )
    .map_err(|e| format!("parsing {src}: {e}"))?;
    let tsheet = image::open(dir.join(src).parent().unwrap().join(&tsj.image))
        .map_err(|e| format!("opening tileset {}: {e}", tsj.image))?
        .to_rgba8();
    let tcols = tsj.columns.max(1);
    let terr_first = terr.firstgid;

    let terrain = layer(&map, "terrain").ok_or("map needs a 'terrain' tile layer")?;
    // the height layer indexes a second tileset; elevation = gid - that tileset's firstgid + 1.
    let height_first = map
        .tilesets
        .iter()
        .map(|t| t.firstgid)
        .filter(|&g| g > terr_first)
        .min();
    let heights = layer(&map, "height");
    let height_at = |i: usize| -> i64 {
        match (heights, height_first) {
            (Some(hl), Some(hf)) => {
                let g = hl.data.get(i).copied().unwrap_or(0) & FLIP_MASK;
                if g > 0 {
                    g - hf as i64 + 1
                } else {
                    0
                }
            }
            _ => 0,
        }
    };

    // ---- compose the board, back-to-front (painter's algorithm) ----
    let mut canvas = RgbaImage::from_pixel(CANVAS, CANVAS, Rgba([0, 0, 0, 255]));
    let mut cells: Vec<usize> = (0..(w * h) as usize).collect();
    cells.sort_by_key(|&i| (i as i32 % w) + (i as i32 / w));
    for i in cells {
        let (c, r) = (i as i32 % w, i as i32 / w);
        let gid = terrain.data.get(i).copied().unwrap_or(0) & FLIP_MASK;
        if gid <= 0 {
            continue;
        }
        let local = (gid - terr_first as i64) as i64;
        let (sc, sr) = ((local % tcols as i64) as u32, (local / tcols as i64) as u32);
        let tile = imageops::crop_imm(&tsheet, sc * TILE, sr * TILE, TILE, TILE).to_image();
        let x = ORIGIN_X + (c as i64 - r as i64) * HALF_W;
        let y = ORIGIN_Y + (c as i64 + r as i64) * HALF_H - height_at(i) * LIFT;
        imageops::overlay(&mut canvas, &tile, x, y); // alpha-aware: transparent tile corners kept
    }

    quantize_in_place(&mut canvas, MAX_COLOURS);

    let stem = tmj_path.file_stem().unwrap().to_string_lossy().to_string();
    let atlas_path = dir.join(format!("{stem}.board.png"));
    canvas
        .save(&atlas_path)
        .map_err(|e| format!("writing {}: {e}", atlas_path.display()))?;
    let map_path = atlas_path.clone();
    Ok(Output {
        atlas_path,
        map_path,
    })
}

/// Median-cut the image's opaque colours down to `n`, remapping every pixel to the nearest chosen
/// colour, so agb's 4bpp `include_background_gfx!` sees a single <=16-colour palette (no per-8×8-tile
/// palette overflow at tile seams). Fully-transparent pixels are left untouched.
pub(crate) fn quantize_in_place(img: &mut RgbaImage, n: usize) {
    let opaque: Vec<[u8; 3]> = img
        .pixels()
        .filter(|p| p[3] >= 128)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    if opaque.is_empty() {
        return;
    }
    let mut buckets: Vec<Vec<[u8; 3]>> = vec![opaque];
    while buckets.len() < n {
        // split the bucket with the largest single-channel spread
        let (bi, ch) = match buckets
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| {
                let spread = |c: usize| {
                    let (mn, mx) = b
                        .iter()
                        .fold((255u8, 0u8), |(mn, mx), p| (mn.min(p[c]), mx.max(p[c])));
                    mx - mn
                };
                let ch = (0..3).max_by_key(|&c| spread(c)).unwrap();
                (i, ch, spread(ch))
            })
            .max_by_key(|&(_, _, s)| s)
        {
            Some((i, ch, _)) => (i, ch),
            None => break,
        };
        let mut b = buckets.swap_remove(bi);
        b.sort_by_key(|p| p[ch]);
        let mid = b.len() / 2;
        let hi = b.split_off(mid);
        buckets.push(b);
        buckets.push(hi);
    }
    // palette = each bucket's mean colour
    let palette: Vec<[u8; 3]> = buckets
        .iter()
        .map(|b| {
            let (mut r, mut g, mut bl) = (0u32, 0u32, 0u32);
            for p in b {
                r += p[0] as u32;
                g += p[1] as u32;
                bl += p[2] as u32;
            }
            let k = b.len().max(1) as u32;
            [(r / k) as u8, (g / k) as u8, (bl / k) as u8]
        })
        .collect();
    let nearest = |p: [u8; 3]| -> [u8; 3] {
        *palette
            .iter()
            .min_by_key(|q| {
                let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
                d(p[0], q[0]) + d(p[1], q[1]) + d(p[2], q[2])
            })
            .unwrap()
    };
    for px in img.pixels_mut() {
        if px[3] >= 128 {
            let c = nearest([px[0], px[1], px[2]]);
            *px = Rgba([c[0], c[1], c[2], 255]);
        }
    }
}
