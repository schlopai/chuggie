//! Rust port of scripts/ninja_autotile.py's Autotiler — Godot "match corners and sides" (47-blob)
//! mask lookup, faithful to the Python reference (same bit order, same corner-reduction rule).
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct AutotileFile {
    tilesets: HashMap<String, TilesetTable>,
}

#[derive(Deserialize)]
struct TilesetTable {
    cols: i32,
    materials: HashMap<String, MaterialTable>,
}

#[derive(Deserialize)]
struct MaterialTable {
    tiles: Vec<TileEntry>,
}

#[derive(Deserialize)]
struct TileEntry {
    col: i32,
    row: i32,
    gid: i32,
    mask: [u8; 8],
}

// bit order: [top_left, top, top_right, left, right, bottom_left, bottom, bottom_right]
const TL: usize = 0;
const T: usize = 1;
const TR: usize = 2;
const L: usize = 3;
const R: usize = 4;
const BL: usize = 5;
const B: usize = 6;
const BR: usize = 7;

pub struct Autotiler {
    /// (tileset, material) -> mask -> gid list (variants)
    lut: HashMap<(String, String), HashMap<[u8; 8], Vec<i32>>>,
    pub cols: HashMap<String, i32>,
    /// (tileset, material) -> bounding box (min_col, min_row, max_col, max_row) across its tiles
    pub bounds: HashMap<(String, String), (i32, i32, i32, i32)>,
}

impl Autotiler {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("reading autotile.json at {}: {e}", path.display()))?;
        let data: AutotileFile =
            serde_json::from_str(&text).map_err(|e| format!("parsing autotile.json: {e}"))?;
        let mut lut = HashMap::new();
        let mut cols = HashMap::new();
        let mut bounds = HashMap::new();
        for (tileset, table) in data.tilesets {
            cols.insert(tileset.clone(), table.cols);
            for (mat, mtable) in table.materials {
                let mut m: HashMap<[u8; 8], Vec<i32>> = HashMap::new();
                let (mut c0, mut r0, mut c1, mut r1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
                for t in &mtable.tiles {
                    m.entry(t.mask).or_default().push(t.gid);
                    c0 = c0.min(t.col);
                    r0 = r0.min(t.row);
                    c1 = c1.max(t.col);
                    r1 = r1.max(t.row);
                }
                let key = (tileset.clone(), mat.clone());
                lut.insert(key.clone(), m);
                bounds.insert(key, (c0, r0, c1, r1));
            }
        }
        Ok(Self { lut, cols, bounds })
    }

    fn mask(grid: &[i32], w: i32, h: i32, c: i32, r: i32, fill: i32, oob_same: bool) -> [u8; 8] {
        let same = |cc: i32, rr: i32| -> bool {
            if cc < 0 || cc >= w || rr < 0 || rr >= h {
                return oob_same;
            }
            grid[(rr * w + cc) as usize] == fill
        };
        let top = same(c, r - 1);
        let bot = same(c, r + 1);
        let left = same(c - 1, r);
        let right = same(c + 1, r);
        let mut m = [0u8; 8];
        m[T] = top as u8;
        m[B] = bot as u8;
        m[L] = left as u8;
        m[R] = right as u8;
        m[TL] = (same(c - 1, r - 1) && top && left) as u8;
        m[TR] = (same(c + 1, r - 1) && top && right) as u8;
        m[BL] = (same(c - 1, r + 1) && bot && left) as u8;
        m[BR] = (same(c + 1, r + 1) && bot && right) as u8;
        m
    }

    /// Paint every cell of `grid` equal to `fill` with `material`'s autotile gid from `tileset`.
    /// Returns row-major gids (0 = untouched).
    #[allow(clippy::too_many_arguments)]
    pub fn terrain_to_gids(
        &self,
        grid: &[i32],
        w: i32,
        h: i32,
        tileset: &str,
        material: &str,
        fill: i32,
        oob_same: bool,
    ) -> Vec<i32> {
        let key = (tileset.to_string(), material.to_string());
        let lut = self
            .lut
            .get(&key)
            .expect("unknown tileset/material in autotile.json");
        let keys: Vec<[u8; 8]> = lut.keys().copied().collect();
        let mut out = vec![0i32; (w * h) as usize];
        for r in 0..h {
            for c in 0..w {
                let i = (r * w + c) as usize;
                if grid[i] != fill {
                    continue;
                }
                let m = Self::mask(grid, w, h, c, r, fill, oob_same);
                let variants = lut.get(&m).or_else(|| {
                    keys.iter()
                        .min_by_key(|k| k.iter().zip(m.iter()).filter(|(a, b)| a != b).count())
                        .and_then(|best| lut.get(best))
                });
                if let Some(v) = variants {
                    // deterministic variant pick, matching the Python reference's spatial hash
                    let idx = ((c * 7 + r * 13) as usize) % v.len();
                    out[i] = v[idx];
                }
            }
        }
        out
    }
}
