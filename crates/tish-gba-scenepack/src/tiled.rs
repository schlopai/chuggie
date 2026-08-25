//! Read a Tiled map (`.tmj`, JSON) and pack it into the same atlas + map-binary the recipe path
//! produces — so a scene authored visually in Tiled builds into ROM at compile time exactly like a
//! recipe. Supports maps that reference several tilesets: every used tile is resolved to its source
//! tileset via `firstgid`, cropped, deduplicated, and packed into one combined atlas with the tile
//! GIDs remapped. Conventions:
//!   - Tile layers render back-to-front in Tiled order (first layer → priority 3, then 2, 1, 0).
//!     A layer can override that with an int `priority` custom property — which a side-scroller
//!     needs, because world sprites draw at P2 and the map must sit at P2 to be behind them, so a
//!     map with backdrops cannot use the default 3/2/1 ladder.
//!   - Tiled's own per-layer **parallax factors** (`parallaxx` / `parallaxy`, 1.0 = locked to the
//!     camera) are baked as 1/256ths, so a sky or a treeline is a layer of the map rather than a
//!     separate `background:` image. That matters on hardware: `set_background_palettes` replaces
//!     all 16 background palettes, so a backdrop from a second image would fight the map's own
//!     tileset for them. One .tmj is one atlas is one palette set.
//!   - Solids come from **per-tile collision** in the tileset (Tiled Collision Editor shapes, and/or
//!     a `walkable = false` custom property) — any render-layer cell that places such a tile is solid.
//!   - Two more OPTIONAL collision planes come from `oneway = true` / `ladder = true` tile
//!     properties: a platform you land on but can jump up through, and something you can climb.
//!     The three are independent — a ladder is climbable and not solid, a beam is one-way and not
//!     solid, a ladder cap is both. A top-down map sets neither and pays nothing for them.
//!   - Two optional, un-rendered mask layers refine that (case-insensitive names):
//!     **"Collision"** forces cells WALKABLE — an EMPTY cell clears whatever the tileset said, which
//!     is how bridges, cave mouths and doorways are cut out of otherwise-solid art. It cannot make
//!     anything solid, and painting it does nothing; that is deliberate.
//!     **"Solid"** is the counterpart and forces cells SOLID. Applied last, so it wins. This is the
//!     only way a map can author a wall the tileset does not already declare, which it often must:
//!     TilesetNature marks two tiles solid in total, and several tilesets mark none.
//!   - An object layer supplies spawns: each object's tile is (x/16, y/16); its `kind` int property
//!     (or name → player=0/npc=1/heart=2) is the spawn kind. Optional int props `a` / `b` are baked
//!     as i16 args (default 0) for warps, NPC ids, triggers, etc.
use crate::pack::Output;
use image::{imageops, RgbaImage};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ── .tmj / .tsj shapes (only the fields we use) ──────────────────────────────
#[derive(Deserialize)]
struct TmjMap {
    width: i32,
    height: i32,
    tilesets: Vec<TmjTilesetRef>,
    layers: Vec<TmjLayer>,
}

#[derive(Deserialize)]
struct TmjTilesetRef {
    firstgid: i32,
    // external tileset file (our converter emits these); embedded tilesets inline image/columns.
    source: Option<String>,
    image: Option<String>,
    columns: Option<i32>,
    /// Embedded tileset tile definitions (collision / properties) when no external `.tsj`.
    #[serde(default)]
    tiles: Vec<TsjTile>,
}

#[derive(Deserialize)]
struct TmjLayer {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: Vec<i64>,
    #[serde(default)]
    objects: Vec<TmjObject>,
    /// Tiled's native per-layer parallax factors. 1.0 (the default, and what Tiled omits from the
    /// file) means the layer is locked to the camera — an ordinary world layer.
    #[serde(default = "one")]
    parallaxx: f64,
    #[serde(default = "one")]
    parallaxy: f64,
    /// Custom properties. `priority` (int, 0 = front … 3 = back) overrides the default
    /// back-to-front assignment.
    #[serde(default)]
    properties: Vec<TmjProp>,
}

fn one() -> f64 {
    1.0
}

#[derive(Deserialize)]
struct TmjObject {
    #[serde(default)]
    name: String,
    x: f64,
    y: f64,
    #[serde(default)]
    properties: Vec<TmjProp>,
}

#[derive(Deserialize, Clone)]
struct TmjProp {
    name: String,
    value: serde_json::Value,
}

#[derive(Deserialize)]
struct Tsj {
    image: String,
    columns: i32,
    #[serde(default)]
    tiles: Vec<TsjTile>,
}

#[derive(Deserialize, Default, Clone)]
struct TsjTile {
    id: i32,
    #[serde(default)]
    properties: Vec<TmjProp>,
    /// Tiled Collision Editor shapes live here (`objects` non-empty ⇒ solid for our boolean grid).
    #[serde(default)]
    objectgroup: Option<TsjObjectGroup>,
}

#[derive(Deserialize, Default, Clone)]
struct TsjObjectGroup {
    #[serde(default)]
    objects: Vec<serde_json::Value>,
}

// resolved tileset: source image + its column count (to turn a local id into a pixel rect)
struct ResolvedTileset {
    firstgid: i32,
    image: RgbaImage,
    columns: i32,
    /// Local tile ids that block movement (collision shapes and/or `walkable = false`).
    solid_ids: HashSet<i32>,
    /// Local tile ids that force movement (e.g. bridge over water, `walkable = true`).
    walkable_ids: HashSet<i32>,
    /// Local tile ids you land on from above but pass through from below (`oneway = true`).
    oneway_ids: HashSet<i32>,
    /// Local tile ids you can climb (`ladder = true`).
    ladder_ids: HashSet<i32>,
}

const FLIP_MASK: i64 = !(0xE000_0000u32 as i64); // strip Tiled's H/V/diagonal flip flags

/// Marks the optional one-way + ladder plane trailer. Must match `MAP_PLANES_MAGIC` in tish-agb.
const MAP_PLANES_MAGIC: u16 = 0x504C; // "PL"
/// Marks the optional per-layer parallax trailer. Must match `MAP_PARALLAX_MAGIC` in tish-agb.
const MAP_PARALLAX_MAGIC: u16 = 0x5058; // "PX"

/// A Tiled parallax factor (1.0 = locked to the camera) in the 1/256ths the engine scrolls by.
fn par_256(f: f64) -> i16 {
    (f * 256.0).round().clamp(i16::MIN as f64, i16::MAX as f64) as i16
}

/// An int custom property, if the map sets it.
fn prop_int(props: &[TmjProp], name: &str) -> Option<i32> {
    props
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .and_then(|p| p.value.as_i64())
        .map(|v| v as i32)
}

fn tile_is_solid(tile: &TsjTile) -> bool {
    if tile
        .objectgroup
        .as_ref()
        .map(|g| !g.objects.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    tile.properties
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case("walkable") && p.value.as_bool() == Some(false))
}

fn solid_ids_from_tiles(tiles: &[TsjTile]) -> HashSet<i32> {
    tiles
        .iter()
        .filter(|t| tile_is_solid(t))
        .map(|t| t.id)
        .collect()
}

/// Local ids of every tile carrying `name = true` — how `walkable`, `oneway` and `ladder` are all
/// spelled in the Tiled tileset editor.
fn flag_ids(tiles: &[TsjTile], name: &str) -> HashSet<i32> {
    tiles
        .iter()
        .filter(|t| {
            t.properties
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(name) && p.value.as_bool() == Some(true))
        })
        .map(|t| t.id)
        .collect()
}

fn load_tileset(dir: &Path, tref: &TmjTilesetRef) -> Result<ResolvedTileset, String> {
    let (image_rel, columns, tiles) = if let Some(src) = &tref.source {
        if !src.ends_with(".tsj") && !src.ends_with(".json") {
            return Err(format!(
                "tileset '{src}': only external .tsj (Tiled JSON) tilesets are supported — in Tiled, \
                 File ▸ Export/Save the tileset As .tsj (or embed it in the map)"
            ));
        }
        let tsj: Tsj = serde_json::from_str(
            &std::fs::read_to_string(dir.join(src)).map_err(|e| format!("reading {src}: {e}"))?,
        )
        .map_err(|e| format!("parsing {src}: {e}"))?;
        // image path is relative to the .tsj's own directory
        let tsj_dir = dir.join(src).parent().unwrap().to_path_buf();
        (tsj_dir.join(&tsj.image), tsj.columns, tsj.tiles)
    } else {
        let img = tref
            .image
            .as_ref()
            .ok_or("embedded tileset without an image")?;
        (
            dir.join(img),
            tref.columns.ok_or("embedded tileset without columns")?,
            // an embedded tileset's tile defs live in the map; an external .tsj owns its own
            tref.tiles.clone(),
        )
    };
    let image = image::open(&image_rel)
        .map_err(|e| format!("opening tileset image {}: {e}", image_rel.display()))?
        .to_rgba8();
    Ok(ResolvedTileset {
        firstgid: tref.firstgid,
        image,
        columns,
        solid_ids: solid_ids_from_tiles(&tiles),
        walkable_ids: flag_ids(&tiles, "walkable"),
        oneway_ids: flag_ids(&tiles, "oneway"),
        ladder_ids: flag_ids(&tiles, "ladder"),
    })
}

pub fn build(tmj_path: &Path) -> Result<Output, String> {
    let dir = tmj_path.parent().ok_or("map path has no parent")?;
    let map: TmjMap = serde_json::from_str(
        &std::fs::read_to_string(tmj_path)
            .map_err(|e| format!("reading {}: {e}", tmj_path.display()))?,
    )
    .map_err(|e| format!("parsing {}: {e}", tmj_path.display()))?;
    let (w, h) = (map.width, map.height);

    let mut tilesets: Vec<ResolvedTileset> = map
        .tilesets
        .iter()
        .map(|t| load_tileset(dir, t))
        .collect::<Result<_, _>>()?;
    tilesets.sort_by_key(|t| -t.firstgid); // highest firstgid first for resolution

    let resolve = |gid: i64| -> Option<(usize, i32)> {
        let raw = (gid & FLIP_MASK) as i32;
        if raw <= 0 {
            return None;
        }
        tilesets
            .iter()
            .position(|t| raw >= t.firstgid)
            .map(|i| (i, raw - tilesets[i].firstgid))
    };

    // classify layers: rendered tile layers (in Tiled order), the two collision-mask layers, object layers
    let mut render_layers: Vec<&TmjLayer> = Vec::new();
    let mut collision: Option<&TmjLayer> = None;
    let mut force_solid: Option<&TmjLayer> = None;
    let mut object_layers: Vec<&TmjLayer> = Vec::new();
    for l in &map.layers {
        match l.kind.as_str() {
            "tilelayer" if l.name.eq_ignore_ascii_case("collision") => collision = Some(l),
            "tilelayer" if l.name.eq_ignore_ascii_case("solid") => force_solid = Some(l),
            "tilelayer" => render_layers.push(l),
            "objectgroup" => object_layers.push(l),
            _ => {}
        }
    }

    // ---- collect every used gid across rendered layers, pack unique tiles into one atlas ----
    let mut slot_of: HashMap<i64, i32> = HashMap::new(); // masked gid -> packed slot
    let mut unique: Vec<i64> = Vec::new();
    for l in &render_layers {
        for &g in &l.data {
            let masked = g & FLIP_MASK;
            if masked > 0 && !slot_of.contains_key(&masked) {
                slot_of.insert(masked, unique.len() as i32);
                unique.push(masked);
            }
        }
    }
    let n = unique.len().max(1);
    let atlas_cols = (n as f64).sqrt().ceil() as i32;
    let atlas_cols = atlas_cols.clamp(1, 32);
    let atlas_rows = ((n as i32) + atlas_cols - 1) / atlas_cols;
    let mut atlas = RgbaImage::new((atlas_cols * 16) as u32, (atlas_rows * 16) as u32);
    for (slot, &masked) in unique.iter().enumerate() {
        if let Some((ti, local)) = resolve(masked) {
            let t = &tilesets[ti];
            let (sc, sr) = (local % t.columns, local / t.columns);
            let cell =
                imageops::crop_imm(&t.image, (sc * 16) as u32, (sr * 16) as u32, 16, 16).to_image();
            let (dc, dr) = (slot as i32 % atlas_cols, slot as i32 / atlas_cols);
            imageops::overlay(&mut atlas, &cell, (dc * 16) as i64, (dr * 16) as i64);
        }
    }
    // packed gid for a source gid = slot + 1 (0 = empty)
    let packed = |g: i64| -> i32 {
        let masked = g & FLIP_MASK;
        if masked <= 0 {
            0
        } else {
            slot_of.get(&masked).map(|s| s + 1).unwrap_or(0)
        }
    };

    // ---- solid grid: per-tile collision on any render layer ----
    let mut solid = vec![0u8; (w * h) as usize];
    for l in &render_layers {
        for (i, &g) in l.data.iter().enumerate() {
            if i >= solid.len() {
                break;
            }
            if let Some((ti, local)) = resolve(g) {
                if tilesets[ti].solid_ids.contains(&local) {
                    solid[i] = 1;
                } else if tilesets[ti].walkable_ids.contains(&local) {
                    solid[i] = 0;
                }
            }
        }
    }
    // Collision overlay: only FORCE WALKABLE (0). Do not force-solid from the converter —
    // that over-blocked the overworld vs tileset collision and broke bridges / enemy hops.
    // Zeros cover PASSABLE tiles, bridge locals, and cave-mouth clears.
    if let Some(cl) = collision {
        for (i, &v) in cl.data.iter().enumerate() {
            if i < solid.len() && v == 0 {
                solid[i] = 0;
            }
        }
    }
    // "Solid" overlay: the FORCE-SOLID counterpart, applied last so it wins over both of the above.
    //
    // A map cannot otherwise author a wall. Per-tile collision is the tileset's, and the tilesets
    // are sparse in ways a map has no say over: TilesetNature marks exactly TWO tiles solid — the
    // trunk cell of the pink and green canopies — so a treeline stamped every three columns leaves
    // two of every three cells walkable, and TilesetField/FloorDetail/tileset_bed mark none at all,
    // so a patch of scenery or a bookcase is scenery you walk through. "Collision" is no help: it
    // only ever forces WALKABLE (see above), deliberately, because forcing solid from it once
    // over-blocked the overworld and broke bridges and enemy hops. That is why this is a SECOND,
    // separately-named layer instead of a change to that one — every existing map keeps its exact
    // collision, and a map that wants a wall paints `Solid`.
    if let Some(sl) = force_solid {
        for (i, &v) in sl.data.iter().enumerate() {
            if i < solid.len() && v != 0 {
                solid[i] = 1;
            }
        }
    }

    // ---- optional side-scroller planes: one-way platforms and ladders ----
    // Set-only, never cleared: a beam is still a beam whatever is painted under it. A map whose
    // tileset declares neither property leaves both planes empty and pays nothing for them below.
    let mut oneway = vec![0u8; (w * h) as usize];
    let mut ladder = vec![0u8; (w * h) as usize];
    for l in &render_layers {
        for (i, &g) in l.data.iter().enumerate() {
            if i >= oneway.len() {
                break;
            }
            if let Some((ti, local)) = resolve(g) {
                if tilesets[ti].oneway_ids.contains(&local) {
                    oneway[i] = 1;
                }
                if tilesets[ti].ladder_ids.contains(&local) {
                    ladder[i] = 1;
                }
            }
        }
    }
    let has_planes = oneway.iter().chain(ladder.iter()).any(|&v| v != 0);

    // ---- the order the runtime must CREATE layers in, which is not Tiled's ----
    // Priority is the layer's `priority` property, else Tiled order back-to-front. Layers are then
    // emitted FRONT FIRST, and within one priority in reverse Tiled order, because the runtime
    // creates layers in blob order and an earlier-created background wins a priority tie. So Tiled's
    // own stacking order stays authoritative for two backdrops sharing P3, and when a map overruns
    // the GBA's four-background budget the layer that gets dropped is the backmost, not the closest.
    //
    // NOTE this is deliberately computed AFTER the atlas is packed — packing walks the layers too,
    // and reordering it would reshuffle every existing map's tile slots for no reason.
    let mut emit: Vec<(usize, i32, (i16, i16))> = render_layers
        .iter()
        .enumerate()
        .map(|(idx, l)| {
            let priority = prop_int(&l.properties, "priority")
                .unwrap_or((3 - idx as i32).max(0))
                .clamp(0, 3);
            (idx, priority, (par_256(l.parallaxx), par_256(l.parallaxy)))
        })
        .collect();
    emit.sort_by_key(|&(idx, priority, _)| (priority, std::cmp::Reverse(idx)));

    // ---- spawns from object layers: (col, row, kind, a, b) ----
    let prop_i16 = |props: &[TmjProp], name: &str| -> i16 {
        props
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .and_then(|p| p.value.as_i64())
            .map(|v| v as i16)
            .unwrap_or(0)
    };
    let mut spawns: Vec<(i16, i16, u16, i16, i16)> = Vec::new();
    for ol in &object_layers {
        for o in &ol.objects {
            let col = (o.x / 16.0).floor() as i16;
            let row = (o.y / 16.0).floor() as i16;
            let kind = o
                .properties
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case("kind"))
                .and_then(|p| p.value.as_i64())
                .map(|k| k as u16)
                .unwrap_or_else(|| match o.name.to_ascii_lowercase().as_str() {
                    "player" => 0,
                    "heart" | "pickup" => 2,
                    _ => 1,
                });
            let a = prop_i16(&o.properties, "a");
            let b = prop_i16(&o.properties, "b");
            spawns.push((col, row, kind, a, b));
        }
    }

    // ---- write sibling files (same shape as the recipe path) ----
    let stem = tmj_path.file_stem().unwrap().to_string_lossy().to_string();
    let atlas_path = dir.join(format!("{stem}.atlas.png"));
    let map_path = dir.join(format!("{stem}.map.bin"));
    atlas
        .save(&atlas_path)
        .map_err(|e| format!("writing atlas: {e}"))?;

    let mut buf = Vec::new();
    buf.extend_from_slice(&(w as u16).to_le_bytes());
    buf.extend_from_slice(&(h as u16).to_le_bytes());
    buf.extend_from_slice(&(atlas_cols as u16).to_le_bytes());
    // ---- fold compatible layers at BAKE time ----
    // The .tmj keeps its semantic layers (Ground / Walls / Props) so a human can open and edit it
    // in Tiled — that file is the source of truth, and collapsing it was tried and rejected. The
    // RUNTIME, though, pays a full InfiniteScrolledMap's page bookkeeping (~8KB of heap, fixed)
    // for every blob layer it streams, however few cells the layer holds — a downstream game's map paid
    // one for a Props layer with seven tiles, on a heap running ~6KB from the ceiling. So the
    // fold happens here, invisibly: consecutive emitted layers with the same priority and
    // parallax whose painted cells never overlap composite identically as one, and are written
    // as one. Overlap keeps them separate (transparency stacking is real there).
    let mut folded: Vec<(i32, (i16, i16), Vec<u16>)> = Vec::new();
    for &(idx, priority, par) in &emit {
        let cells: Vec<u16> = (0..(w * h) as usize)
            .map(|i| packed(render_layers[idx].data.get(i).copied().unwrap_or(0)) as u16)
            .collect();
        if let Some(last) = folded.last_mut() {
            if last.0 == priority
                && last.1 == par
                && last.2.iter().zip(&cells).all(|(&a, &b)| a == 0 || b == 0)
            {
                for (dst, &src) in last.2.iter_mut().zip(&cells) {
                    if src != 0 {
                        *dst = src;
                    }
                }
                continue;
            }
        }
        folded.push((priority, par, cells));
    }
    if folded.len() < emit.len() {
        println!(
            "  folded {} tile layer(s) into {} at bake (same priority, no overlap)",
            emit.len(),
            folded.len()
        );
    }
    buf.extend_from_slice(&(folded.len() as u16).to_le_bytes());
    for (priority, _, cells) in &folded {
        buf.extend_from_slice(&(*priority as u16).to_le_bytes());
        for &cell in cells {
            buf.extend_from_slice(&cell.to_le_bytes());
        }
    }
    buf.extend_from_slice(&solid);
    buf.extend_from_slice(&(spawns.len() as u16).to_le_bytes());
    for (c, r, k, a, b) in &spawns {
        buf.extend_from_slice(&c.to_le_bytes());
        buf.extend_from_slice(&r.to_le_bytes());
        buf.extend_from_slice(&k.to_le_bytes());
        buf.extend_from_slice(&a.to_le_bytes());
        buf.extend_from_slice(&b.to_le_bytes());
    }
    // Optional trailers, each behind a magic word, and each written only when the map actually uses
    // it — so a map that wants none of this ends after its spawns exactly as it always has, and an
    // older blob stays readable. Keep in sync with the trailer walk in tish-agb's
    // `do_map_stream_resolved`.
    if has_planes {
        buf.extend_from_slice(&MAP_PLANES_MAGIC.to_le_bytes());
        buf.extend_from_slice(&oneway);
        buf.extend_from_slice(&ladder);
    }
    // The parallax trailer is per WRITTEN layer, so it walks `folded`, not `emit` — a folded
    // group shares one parallax by construction (equal parallax is a fold precondition).
    if folded
        .iter()
        .any(|&(_, (mx, my), _)| mx != 256 || my != 256)
    {
        buf.extend_from_slice(&MAP_PARALLAX_MAGIC.to_le_bytes());
        for &(_, (mx, my), _) in &folded {
            buf.extend_from_slice(&mx.to_le_bytes());
            buf.extend_from_slice(&my.to_le_bytes());
        }
    }
    std::fs::write(&map_path, &buf).map_err(|e| format!("writing map: {e}"))?;
    Ok(Output {
        atlas_path,
        map_path,
    })
}
