//! Packs a Recipe into a small combined atlas PNG + a `map:`-format binary, writing both as
//! sibling files next to the recipe (`<recipe>.atlas.png` / `<recipe>.map.bin`) — build
//! artifacts, regenerated on every compile, not meant to be hand-edited or committed.
use crate::autotile::Autotiler;
use crate::recipe::Recipe;
use image::{imageops, RgbaImage};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct Output {
    pub atlas_path: PathBuf,
    pub map_path: PathBuf,
}

/// One packed block: which row-band of the new atlas it occupies.
struct Block {
    row_offset: i32,
}

pub fn build(recipe_path: &Path) -> Result<Output, String> {
    let dir = recipe_path
        .parent()
        .ok_or_else(|| "recipe path has no parent directory".to_string())?;
    let text = std::fs::read_to_string(recipe_path)
        .map_err(|e| format!("reading recipe {}: {e}", recipe_path.display()))?;
    let recipe: Recipe = serde_json::from_str(&text).map_err(|e| format!("parsing recipe: {e}"))?;

    let tilesets_root = dir.join(&recipe.tilesets_root);
    let autotile_path = dir.join(&recipe.autotile_json);
    let at = Autotiler::load(&autotile_path)?;

    let mut source_cache: HashMap<String, RgbaImage> = HashMap::new();
    let mut load_source = |tileset: &str| -> Result<RgbaImage, String> {
        if let Some(im) = source_cache.get(tileset) {
            return Ok(im.clone());
        }
        let p = tilesets_root.join(tileset);
        let im = image::open(&p)
            .map_err(|e| format!("opening tileset {}: {e}", p.display()))?
            .to_rgba8();
        source_cache.insert(tileset.to_string(), im.clone());
        Ok(im)
    };

    // ---- 1. plan every block that needs packing (ground material, fence, each stamp) ----
    struct PlannedBlock {
        key: String, // unique name used for gid lookups later
        tileset: String,
        origin: (i32, i32),
        size: (i32, i32),
    }
    let mut planned = Vec::new();

    let (gc0, gr0, gc1, gr1) = *at
        .bounds
        .get(&(
            recipe.ground.tileset.clone(),
            recipe.ground.material.clone(),
        ))
        .ok_or_else(|| {
            format!(
                "no autotile bounds for {}/{}",
                recipe.ground.tileset, recipe.ground.material
            )
        })?;
    planned.push(PlannedBlock {
        key: "__ground".into(),
        tileset: recipe.ground.tileset.clone(),
        origin: (gc0, gr0),
        size: (gc1 - gc0 + 1, gr1 - gr0 + 1),
    });

    if let Some(f) = &recipe.border_fence {
        let max_dc = f.post[0].max(f.run[0]).max(f.rail[0]);
        let max_dr = f.post[1].max(f.run[1]).max(f.rail[1]);
        planned.push(PlannedBlock {
            key: "__fence".into(),
            tileset: f.tileset.clone(),
            origin: (f.origin[0], f.origin[1]),
            size: (max_dc + 1, max_dr + 1),
        });
    }

    for (i, s) in recipe.stamps.iter().enumerate() {
        planned.push(PlannedBlock {
            key: format!("__stamp{i}"),
            tileset: s.tileset.clone(),
            origin: (s.origin[0], s.origin[1]),
            size: (s.size[0], s.size[1]),
        });
    }

    // ---- 2. pack: stack blocks vertically, atlas width = widest block ----
    let atlas_cols = planned.iter().map(|b| b.size.0).max().unwrap_or(1).max(1);
    let atlas_rows: i32 = planned.iter().map(|b| b.size.1).sum();
    let mut atlas = RgbaImage::new((atlas_cols * 16) as u32, (atlas_rows * 16) as u32);

    let mut blocks: HashMap<String, Block> = HashMap::new();
    // per-block set of fully-transparent (dc, dr) cells — these place no gid and are never solid,
    // so a round tree's transparent canopy corners don't become invisible walls.
    let mut transparent: HashMap<String, HashSet<(i32, i32)>> = HashMap::new();
    let mut row_cursor = 0i32;
    for p in &planned {
        let src = load_source(&p.tileset)?;
        let crop = imageops::crop_imm(
            &src,
            (p.origin.0 * 16) as u32,
            (p.origin.1 * 16) as u32,
            (p.size.0 * 16) as u32,
            (p.size.1 * 16) as u32,
        )
        .to_image();
        let mut empties = HashSet::new();
        for dr in 0..p.size.1 {
            for dc in 0..p.size.0 {
                let cell = imageops::crop_imm(&crop, (dc * 16) as u32, (dr * 16) as u32, 16, 16)
                    .to_image();
                if cell.pixels().all(|px| px.0[3] == 0) {
                    empties.insert((dc, dr));
                }
            }
        }
        transparent.insert(p.key.clone(), empties);
        imageops::overlay(&mut atlas, &crop, 0, (row_cursor * 16) as i64);
        blocks.insert(
            p.key.clone(),
            Block {
                row_offset: row_cursor,
            },
        );
        row_cursor += p.size.1;
    }

    let new_gid = |key: &str, dc: i32, dr: i32| -> i32 {
        let b = &blocks[key];
        (b.row_offset + dr) * atlas_cols + dc + 1
    };

    // ---- 3. ground layer: autotile, then remap old TilesetFloor gids into the packed block ----
    let w = recipe.width as i32;
    let h = recipe.height as i32;
    let mut terrain = vec![0i32; (w * h) as usize];
    let [fx, fy, fw, fh] = recipe.ground.fill_rect;
    for r in fy..(fy + fh) {
        for c in fx..(fx + fw) {
            if c >= 0 && c < w && r >= 0 && r < h {
                terrain[(r * w + c) as usize] = 1;
            }
        }
    }
    let old_gids = at.terrain_to_gids(
        &terrain,
        w,
        h,
        &recipe.ground.tileset,
        &recipe.ground.material,
        1,
        false,
    );
    let src_cols = at.cols[&recipe.ground.tileset];
    let mut ground_layer = vec![0i32; (w * h) as usize];
    for (i, &g) in old_gids.iter().enumerate() {
        if g == 0 {
            continue;
        }
        let old_tid = g - 1;
        let (oc, orow) = (old_tid % src_cols, old_tid / src_cols);
        let (dc, dr) = (oc - gc0, orow - gr0);
        ground_layer[i] = new_gid("__ground", dc, dr);
    }

    // ---- 4. objects layer: border fence + stamps; solid grid ----
    let mut objects_layer = vec![0i32; (w * h) as usize];
    let mut solid = vec![0u8; (w * h) as usize];
    let set = |layer: &mut [i32], c: i32, r: i32, gid: i32| {
        if c >= 0 && c < w && r >= 0 && r < h {
            layer[(r * w + c) as usize] = gid;
        }
    };
    let mut mark_solid = |c: i32, r: i32| {
        if c >= 0 && c < w && r >= 0 && r < h {
            solid[(r * w + c) as usize] = 1;
        }
    };

    if let Some(f) = &recipe.border_fence {
        for c in 0..w {
            let corner = c == 0 || c == w - 1;
            let piece = if corner { f.post } else { f.run };
            set(
                &mut objects_layer,
                c,
                0,
                new_gid("__fence", piece[0], piece[1]),
            );
            set(
                &mut objects_layer,
                c,
                h - 1,
                new_gid("__fence", piece[0], piece[1]),
            );
            mark_solid(c, 0);
            mark_solid(c, h - 1);
        }
        for r in 0..h {
            let corner = r == 0 || r == h - 1;
            let piece = if corner { f.post } else { f.rail };
            set(
                &mut objects_layer,
                0,
                r,
                new_gid("__fence", piece[0], piece[1]),
            );
            set(
                &mut objects_layer,
                w - 1,
                r,
                new_gid("__fence", piece[0], piece[1]),
            );
            mark_solid(0, r);
            mark_solid(w - 1, r);
        }
    }

    for (i, s) in recipe.stamps.iter().enumerate() {
        let key = format!("__stamp{i}");
        let empties = &transparent[&key];
        for dr in 0..s.size[1] {
            for dc in 0..s.size[0] {
                if empties.contains(&(dc, dr)) {
                    continue; // transparent cell: no tile, no collision
                }
                let (ac, ar) = (s.at[0] + dc, s.at[1] + dr);
                set(&mut objects_layer, ac, ar, new_gid(&key, dc, dr));
                let is_door = s.door == Some([dc, dr]);
                if s.solid && !is_door {
                    mark_solid(ac, ar);
                }
            }
        }
    }

    // ---- 5. write sibling files ----
    let stem = recipe_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let atlas_path = dir.join(format!("{stem}.atlas.png"));
    let map_path = dir.join(format!("{stem}.map.bin"));

    atlas
        .save(&atlas_path)
        .map_err(|e| format!("writing atlas {}: {e}", atlas_path.display()))?;

    let mut buf = Vec::new();
    buf.extend_from_slice(&(recipe.width as u16).to_le_bytes());
    buf.extend_from_slice(&(recipe.height as u16).to_le_bytes());
    buf.extend_from_slice(&(atlas_cols as u16).to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // nlayers
    buf.extend_from_slice(&3u16.to_le_bytes()); // ground priority (back)
    for &g in &ground_layer {
        buf.extend_from_slice(&(g as u16).to_le_bytes());
    }
    buf.extend_from_slice(&2u16.to_le_bytes()); // objects priority (front of ground)
    for &g in &objects_layer {
        buf.extend_from_slice(&(g as u16).to_le_bytes());
    }
    buf.extend_from_slice(&solid);
    buf.extend_from_slice(&(recipe.spawns.len() as u16).to_le_bytes());
    for sp in &recipe.spawns {
        buf.extend_from_slice(&(sp.col as i16).to_le_bytes());
        buf.extend_from_slice(&(sp.row as i16).to_le_bytes());
        buf.extend_from_slice(&sp.kind.to_le_bytes());
        buf.extend_from_slice(&sp.a.to_le_bytes());
        buf.extend_from_slice(&sp.b.to_le_bytes());
    }
    std::fs::write(&map_path, &buf)
        .map_err(|e| format!("writing map {}: {e}", map_path.display()))?;

    Ok(Output {
        atlas_path,
        map_path,
    })
}
