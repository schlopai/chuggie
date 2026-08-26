//! The declarative scene-recipe schema. A recipe describes a small chuggie map purely as
//! references into the vendored Ninja Adventure catalog (autotile.json + tileset PNGs) — no
//! prebuilt atlas or binary is checked in; `include_scene!` (see lib.rs) packs both at compile
//! time from this file alone.
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Recipe {
    pub width: u32,
    pub height: u32,
    /// Path to `catalog/autotile.json`, relative to this recipe file.
    pub autotile_json: String,
    /// Directory holding the source tileset PNGs (`TilesetHouse.png`, …), relative to this recipe file.
    pub tilesets_root: String,
    pub ground: GroundSpec,
    #[serde(default)]
    pub border_fence: Option<FenceSpec>,
    #[serde(default)]
    pub stamps: Vec<StampSpec>,
    pub spawns: Vec<SpawnSpec>,
}

#[derive(Deserialize)]
pub struct GroundSpec {
    pub tileset: String,
    pub material: String,
    /// (x, y, w, h) cell rect to fill with the autotiled material.
    pub fill_rect: [i32; 4],
}

#[derive(Deserialize)]
pub struct FenceSpec {
    pub tileset: String,
    /// Top-left of the small source block the post/run/rail offsets below are relative to.
    pub origin: [i32; 2],
    pub post: [i32; 2],
    pub run: [i32; 2],
    pub rail: [i32; 2],
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct StampSpec {
    pub tileset: String,
    pub origin: [i32; 2],
    pub size: [i32; 2],
    pub at: [i32; 2],
    /// (dc, dr) cell within the stamp left walkable (not solid) — a door threshold.
    #[serde(default)]
    pub door: Option<[i32; 2]>,
    /// Whether the stamp's (opaque) cells block movement. Houses/trees/props: true (default);
    /// ground decor you walk over (grass tufts, saplings): false.
    #[serde(default = "default_true")]
    pub solid: bool,
}

#[derive(Deserialize)]
pub struct SpawnSpec {
    pub col: i32,
    pub row: i32,
    pub kind: u16,
    #[serde(default)]
    pub a: i16,
    #[serde(default)]
    pub b: i16,
}
