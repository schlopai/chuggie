//! Low-level agb bindings for tish, exposed via the native-module ABI
//! (`pub fn name(args: &[Value]) -> Value`). A tish game reaches these with
//! `import { … } from 'cargo:tish_agb'`; the tish compiler generates the glue.
//!
//! P3 surface: log, frame pacing, input (d-pad), and a retained sprite arena so a
//! tish game can move a sprite. Assets are a single hardcoded sprite for now; the
//! `asset:` pipeline and richer sprite/background/audio APIs come next.
#![no_std]
// Engine natives take their full argument lists deliberately — the tish ABI passes scalars, not
// config structs, so arity mirrors the script-side signature.
#![allow(clippy::too_many_arguments)]
// Docs here are hand-wrapped prose; list continuations are wrapped at the margin, not re-indented.
#![allow(clippy::doc_lazy_continuation)]
// For `iwram_free`, which has to name `core::alloc::Allocator` to probe agb's IWRAM arena
// directly — the global `alloc` only reaches EWRAM. agb itself is nightly-only, so this costs
// nothing in reach.
#![feature(allocator_api)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use agb::display::font::{
    AlignmentKind, ChangeColour, Font, Layout, LayoutSettings, LetterGroup,
    RegularBackgroundTextRenderer, Tag,
};
use agb::display::object::{
    DynamicSprite16, GraphicsMode, Object, PaletteVramSingle, Size, Sprite, SpriteVram,
};
use agb::display::tiled::{
    AffineBackground, AffineBackgroundSize, AffineBackgroundWrapBehaviour, AffineMatrixBackground,
    AffineTransformSource, DynamicTile16, InfiniteScrolledMap, PartialUpdateStatus,
    RegularBackground, RegularBackgroundId, RegularBackgroundSize, TileEffect, TileFormat, TileSet,
    TileSetting,
};
use agb::display::utils::blit_16_colour;
// EWRAM. Every DynamicSprite16 staging buffer in this file uses it explicitly — agb's default is
// IWRAM, which is 32 KB shared with the stack; see the note in `letter_group_object`.
use agb::display::{Graphics, GraphicsFrame, Palette16, Priority, Rgb, Rgb15};
use agb::fixnum::{Num, Vector2D};
use agb::input::{Button, ButtonController};
use agb::sound::mixer::{ChannelId, Mixer, SoundChannel};
use agb::timer::{Divider, Timer};
use agb::ExternalAllocator;

mod save_api;
mod save_media;
pub use save_api::{
    save_any, save_entry_col, save_entry_row, save_erase, save_flags, save_has, save_hp, save_init,
    save_max_hp, save_media_name, save_media_sectors, save_media_size, save_pcol, save_prow,
    save_read, save_scene, save_score, save_slots, save_write, sram_commit, sram_commit_typed,
    sram_read_u8, sram_read_u8_typed, sram_size, sram_size_typed, sram_write_u8,
    sram_write_u8_typed,
};

mod kart;
pub use kart::{
    kart_add, kart_boost, kart_bump, kart_cam_x, kart_cam_yaw, kart_cam_z, kart_camera,
    kart_charge, kart_checkpoints, kart_count, kart_draw, kart_draw_items, kart_drifting,
    kart_events, kart_finished, kart_hazard_slots, kart_hazards, kart_input, kart_item,
    kart_item_boxes, kart_lap, kart_rank, kart_reset, kart_set_ai, kart_speed, kart_start,
    kart_step, kart_surface, kart_surface_at, kart_track, kart_use, kart_waypoints, kart_x,
    kart_yaw, kart_z,
};

mod font_metrics;
mod strings;
mod ui_layout;
pub use font_metrics::{register_font, FontMetrics};
pub use strings::register_strings;

mod psg;

pub mod chiptune;
pub mod deck_player;

// ── Dialogue font + palettes ─────────────────────────────────────────────────
static FONT: Font = agb::include_font!("fonts/font.ttf", 10);
/// Background palette for dialogue body text. Index 0 is transparent (the box shows
/// through); EVERY other index is white so no glyph pixel — the renderer uses one index
/// for the letter and another for its drop shadow — is left invisible against the box.
static DIALOG_PALETTE: Palette16 = {
    let mut p = [Rgb15::WHITE; 16];
    p[0] = Rgb15::BLACK; // index 0 = transparent
    Palette16::new(p)
};
/// Background palette for the speaker name — same idea, warm yellow.
static NAME_PALETTE: Palette16 = {
    let yellow = Rgb::new(248, 224, 120).to_rgb15();
    let mut p = [yellow; 16];
    p[0] = Rgb15::BLACK;
    Palette16::new(p)
};

// Dialogue text is drawn into a background layer, which references one of the 16
// background palettes per tile. The scene's own tiles use the low slots, so the body
// and name text claim the two highest slots — no collision with the game's palettes.
const DIALOG_BODY_PAL: u8 = 15;
const DIALOG_NAME_PAL: u8 = 14;

// Dialogue-box panel palette (sprite palette — the panel sits under the transparent text BG).
// 1 = dark-navy fill, 2 = light-blue top edge, 3 = interior shade.
// The two 64×32 DynamicSprite16s are allocated ONCE at first reserve (see `dialogue_reserve` /
// `ensure_dialog_panel`) while OBJ VRAM is still empty. Allocating them on first `dialogue_show`
// in a busy topdown-RPG room left the panel as vertical blue stripes — the sprite data lost to VRAM
// pressure. Keeping the VRAM alive for the whole session means later opens are just Object wraps.
static BOX_PALETTE: Palette16 = Palette16::new([
    Rgb::new(0, 0, 0).to_rgb15(),       // 0 transparent
    Rgb::new(28, 28, 68).to_rgb15(),    // 1 dark navy fill
    Rgb::new(120, 148, 220).to_rgb15(), // 2 light-blue top edge
    Rgb::new(52, 56, 108).to_rgb15(),   // 3 interior highlight (top inner rows)
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
]);

use tishlang_runtime_gba::{get_index, get_prop, value_call, SingleCore, Value};

// ── A hardcoded 16×16 sprite (red square), à la the agb template. ────────────
const PALETTE: Palette16 = Palette16::new([
    Rgb::new(0, 0, 0).to_rgb15(),     // 0: transparent
    Rgb::new(255, 80, 80).to_rgb15(), // 1: red
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
    Rgb::new(0, 0, 0).to_rgb15(),
]);
// 16×16 @ 4bpp = 128 bytes; every nibble = color index 1 (red).
const SPRITE_DATA: [u8; 128] = [0x11; 128];
const SPRITE: Sprite = unsafe { Sprite::new(&PALETTE, &SPRITE_DATA, Size::S16x16) };

// ── Retained game context (single-core static). ──────────────────────────────

/// `SpriteData::sheet` sentinel for a slot parked on `GbaCtx::sprite_free`. Distinct from -1 (the
/// built-in demo sprite) and from any real `asset:` handle (>= 0). Every `sheet >= 0` guard in this
/// file — release, restore, frame swap — therefore skips a freed slot for free.
const SHEET_FREED: i32 = -2;

struct SpriteData {
    /// `None` marks a FREED slot: dropping the agb `Object` releases its VRAM sprite allocation, and
    /// the index is parked on `GbaCtx::sprite_free` for reuse. Keeping `SpriteData` (not the whole
    /// entry) means handle = index stays stable for live sprites.
    object: Option<Object>,
    x: i32,
    y: i32,
    hflip: bool,
    visible: bool,
    /// The `asset:` sheet this sprite draws from (-1 for the built-in demo sprite),
    /// so `sprite_set_frame` can swap to another frame in the same sheet.
    sheet: i32,
    /// Current frame index within `sheet`. Tracked so a sprite whose VRAM `Object` was released
    /// while off-screen (`native_sprite_release`) can be rebuilt on the right frame when it scrolls
    /// back into view (`native_sprite_restore`).
    frame: i32,
    /// HUD sprites draw in SCREEN space (no camera offset) at the front priority — for a
    /// heart/health bar, coin counter, etc. that stays put while the map scrolls. Default false
    /// (a normal world sprite, drawn camera-relative).
    hud: bool,
    /// Explicit background-relative priority, set via `sprite_set_priority`. -1 (the default)
    /// means automatic: HUD sprites draw at the front priority (P0), world sprites at P2. A card
    /// UI sets a HUD sprite to 1 to slide it UNDER the P0 text canvas while staying over a P2/P3
    /// decorative background - text and grids composite on top of a sprite-backed card face.
    priority: i8,
    /// Painter's-algorithm depth for overlap ordering among world sprites (isometric/top-down
    /// y-sort). Higher `depth` = nearer the camera = drawn IN FRONT. Default 0 (registration order
    /// breaks ties). Set via `sprite_set_depth`. For a HUD sprite this is ignored UNLESS the sprite
    /// is a Mode 7 billboard, which is a HUD sprite by necessity (its position is screen space) but
    /// a world object by meaning — see `billboard`.
    depth: i16,
    /// Registered with `mode7_billboard`, so `mode7_billboards_draw` owns its position and depth.
    ///
    /// Billboards have to be HUD sprites, because the projection hands back screen coordinates and
    /// the tile camera must not be applied on top. But that put them in the HUD pass, which draws in
    /// REGISTRATION order — so of two karts side by side, whichever was created first covered the
    /// other regardless of which was nearer. The flag exists to pull them out of that pass and sort
    /// them, using the depth the projection already computes.
    billboard: bool,
}

/// A retained tiled background layer + its visibility.
/// A MODE 7 camera over an affine background: a ground plane in 3D, drawn by giving every scanline
/// its own affine transform.
///
/// This is the difference between a tilted picture and a floor. One affine matrix for the whole
/// screen is an ORTHOGRAPHIC tilt — parallel lines stay parallel, and it reads as a skewed sheet
/// (which is why the earlier affine support here was removed as not looking isometric). Perspective
/// comes from the scale changing with distance, and on this hardware that means rewriting BG2PA..BG2Y
/// between scanlines by DMA. agb exposes exactly that: `AffineBackgroundId::transform_dma()` is a
/// `DmaControllable<AffineMatrixBackground>` pointing at 0x0400_0020, and `HBlankDma<Item>` transfers
/// `size_of::<Item>() / 2` halfwords per line — sixteen bytes, which is the whole matrix.
///
/// The projection, for a camera at `(cam_x, cam_z)` looking along `yaw`, `height` above the plane:
/// a scanline `dy` below the horizon sees ground at depth `focal * height / dy`, so with
/// `k = height / dy` the row's transform is
///
///     PA = cos(yaw) * k          PC = -sin(yaw) * k          PB = PD = 0
///     X  = cam_x + k * (sin(yaw) * focal - cos(yaw) * 120)
///     Y  = cam_z + k * (cos(yaw) * focal + sin(yaw) * 120)
///
/// PB/PD are zero because reloading X/Y every HBlank makes the hardware's own per-line stepping
/// irrelevant — each line is positioned outright rather than accumulated.
#[derive(Clone, Copy)]
struct Mode7 {
    cam_x: Num<i32, 8>,
    cam_z: Num<i32, 8>,
    yaw: Num<i32, 8>, // turns: 1.0 is a full rotation, matching `Num::sin`/`cos`
    height: Num<i32, 8>,
    horizon: i32, // the scanline the ground converges to
    focal: Num<i32, 8>,
    /// Scanlines below the horizon to paint as sky instead of ground — distance haze.
    ///
    /// The rows just under the horizon are where a Mode 7 plane looks worst: `k` is largest there,
    /// so one screen pixel steps many texels and any high-contrast detail (a kerb, a lane line)
    /// aliases into coloured speckle. There is no mipmapping to fix it with. Retiring the worst few
    /// rows into the sky colour costs a sliver of draw distance and removes the shimmer entirely,
    /// which is what the hardware's own era did with a horizon haze.
    haze: i32,
}

/// A flat sprite standing on the Mode 7 plane at a world position.
#[derive(Clone, Copy)]
struct Billboard {
    sprite: i32,
    x: Num<i32, 8>,
    z: Num<i32, 8>,
    w: i32,
    h: i32,
    /// False hides it regardless of where it projects.
    ///
    /// Needed because the projection pass sets `visible` from the frustum test alone, so anything
    /// that wants to be invisible for a GAME reason — a collected item box, an unused hazard slot —
    /// would be turned straight back on the moment it happened to be on screen.
    active: bool,
}

/// A retained affine ("Mode 7") background: a 256-colour tilemap the hardware transforms, plus the
/// camera that drives it when it is a ground plane.
struct AffineData {
    bg: AffineBackground,
    visible: bool,
    m7: Option<Mode7>,
}

struct BgData {
    bg: RegularBackground,
    visible: bool,
    /// The `background:` asset this layer was built from, kept so `bg_use_palettes` can re-upload
    /// its palettes later. The GBA has ONE set of sixteen background palettes shared by every
    /// layer, so two backgrounds with different palettes cannot both look right at once — but they
    /// can take turns, which is what a title screen sitting over a persistent playfield needs.
    asset: i32,
    /// Parallax scroll multipliers in 1/256 of the camera, or `None` for a layer the game scrolls
    /// itself with `bg_scroll`. 256 tracks the camera exactly (as the world layer does), 96 drifts
    /// at three-eighths speed (a near hill line), 0 is pinned to the screen (a static sky).
    /// Negative values scroll the layer the other way, which is how a cloud band moves against the
    /// wind. Applied in `frame`, from the camera the engine set moments earlier in the same step.
    parallax: Option<(i32, i32)>,
    /// Per-SCANLINE horizontal parallax, as `(firstRow, mulX)` bands sorted by row — see
    /// [`bg_bands`]. `None` for a layer that scrolls as one piece. When set, this overrides the
    /// horizontal half of `parallax` (the vertical half still applies to the whole layer).
    bands: Option<alloc::vec::Vec<(u8, i16)>>,
}

/// One line of on-screen text (see `hud_text` / `text_draw`): its rendered sprite objects plus the
/// cached string, position, font handle, and style, so the layout only re-runs when one changes.
/// `visible` gates OAM submission — hide without dropping `Object`s so Sprite VRAM stays allocated
/// (pause open/close must be free after the first paint; see agb `Object` / `DynamicSprite16`).
const HUD_TEXT_SLOT_CAP: usize = 32;

fn empty_hud_slot() -> HudTextSlot {
    HudTextSlot {
        objs: Vec::new(),
        emoji_objs: Vec::new(),
        cache: alloc::string::String::new(),
        x: -1,
        y: -1,
        font: -2,
        colors: Vec::new(),
        shadow: -2,
        align: 255,
        maxw: -1,
        vgrad: false,
        visible: true,
    }
}

struct HudTextSlot {
    objs: Vec<Object>,
    /// Colour emoji sprites overlaid inline on this line — one `Object` per emoji codepoint, drawn as
    /// a fallback wherever the font has no glyph (see `build_text_objs`). Empty for emoji-free text.
    emoji_objs: Vec<Object>,
    cache: alloc::string::String,
    x: i32,
    y: i32,
    font: i32,        // imported font handle, or -1 for the built-in dialogue font
    colors: Vec<i32>, // palette indices 1.. — 0xRRGGBB each; empty → white at index 1
    shadow: i32,      // drop-shadow colour 0xRRGGBB, or -1 for none (drawn at palette index 15)
    align: u8,        // AlignmentKind as discriminant (see align_from_u8)
    maxw: i32,        // wrap / align box width; 0 = unlimited (no wrap)
    /// When true, `colors[]` are mapped top→bottom within each glyph (see `build_text_objs_vgrad`).
    /// When false, colour is solid per glyph / `text_color` switch (horizontal washes across letters).
    vgrad: bool,
    visible: bool,
}

/// One HUD progress/health bar (see `hud_bar`): a single dynamically-drawn sprite plus the cache key
/// (position, size, filled-pixel count, colours) so it re-renders only when the fill or style change —
/// a graphical, boxless alternative to a text bar that a game would otherwise rebuild every frame.
struct HudBarSlot {
    obj: Option<Object>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fill: i32, // filled width in px (the cache key that changes as the value drops)
    fg: i32,
    bg: i32,
}

/// GID source for a streamed tile layer. ROM-baked `map:`/`scene:` imports keep GIDs in
/// flash and look them up on demand — a 240×80 overworld is 38KB as `Vec<i16>`, which is
/// enough to OOM EWRAM on a cave→overworld reload after heap fragmentation. Owned Vec is
/// only for tish-built streams (`tilemap_stream` / terrain) where the data was already on
/// the heap.
enum StreamGids {
    Owned(Vec<i16>),
    /// Little-endian u16 GIDs at `bytes[off..]` — same layout as `map.bin` layer payload.
    Rom {
        bytes: &'static [u8],
        off: usize,
    },
}

impl StreamGids {
    fn gid_at(&self, idx: usize) -> i16 {
        match self {
            StreamGids::Owned(v) => v.get(idx).copied().unwrap_or(0),
            StreamGids::Rom { bytes, off } => rd_u16(bytes, off + idx * 2) as i16,
        }
    }
}

/// A streamed tile layer for maps bigger than the screen. `InfiniteScrolledMap` wraps a
/// 32x32 background window; each frame we scroll it to the camera and it streams the
/// needed tiles in via the provider. `data` is the layer's GIDs (0 = empty); the tileset
/// (`tiles`/`settings`) is a `'static` baked asset. Each 16x16 map cell is drawn as its
/// 2x2 block of GBA 8x8 tiles.
struct StreamLayer {
    map: InfiniteScrolledMap,
    data: StreamGids,
    w: i32,
    h: i32,
    cols: i32,
    /// Runtime tile overrides as a sparse `(index, gid)` list, empty until something calls
    /// `bg_set_tile`. A `scene:` map streams straight out of ROM (`StreamGids::Rom`), so a tile that
    /// changes during play — a bush burnt away, a bombed wall opened, a block pushed — has nowhere
    /// to be written and has to live beside the map instead. Sparse because a room holds a handful
    /// at most, and consulted only when non-empty so the common path costs one branch.
    patch: Vec<(u32, i16)>,
    tiles: &'static TileSet,
    settings: &'static [TileSetting],
    /// Whether this layer is drawn at all. A hidden layer is skipped in the frame loop and hands its
    /// background slot back, exactly as `scene_bg_visible` does for a parallax backdrop — streamed
    /// layers simply never had the switch, which made "a layer of the map that comes and goes" the
    /// one thing a `.tmj` could not express. `examples/spectra` puts each colour band on its own
    /// layer and lights one at a time as the player turns their lens.
    visible: bool,
    /// Scroll speed as a fraction of the camera, in 1/256ths — the same scale as [`bg_parallax`],
    /// which does this for non-streamed backgrounds. 256 is a world layer; 0 pins the layer to the
    /// screen; anything between is a backdrop. Comes from Tiled's own per-layer parallax factor via
    /// the "PX" trailer, so a sky is a layer of the map instead of a second `background:` image
    /// fighting it for the 16 shared background palettes.
    par: (i32, i32),
}

/// [`StreamLayer::par`] for a layer locked to the camera — an ordinary world layer, and what every
/// tish-built stream (`tilemap_stream`, terrain) is. Only a Tiled backdrop asks for anything else.
const PAR_WORLD: (i32, i32) = (256, 256);

/// Metadata for a ROM-baked map (from a `map:` import). GID layers stream from ROM (see
/// `StreamGids::Rom`); the solid grid and entity spawns also stay in ROM and are read on
/// demand by `map_solid_at` / `map_spawn_*`, so the tish heap never holds the map.
struct MapInfo {
    data: &'static [u8],
    width: i32,
    height: i32,
    solid_off: usize,
    spawns_off: usize,
    nspawns: i32,
    /// Offsets of the OPTIONAL one-way and ladder planes, one byte per cell, appended after the
    /// spawn list behind a magic word. `None` for a map that declares neither — which is every
    /// top-down map, so the two stay optional rather than becoming part of the header. A
    /// side-scroller needs three collision planes; a top-down map only ever wanted one.
    /// `tish-gba-scenepack` fills these from `oneway` / `ladder` tile properties in Tiled.
    oneway_off: Option<usize>,
    ladder_off: Option<usize>,
}

/// Magic words marking the optional trailers of a map blob (see [`MapInfo`]), each chosen so a blob
/// that simply ends after its spawns cannot be mistaken for one that carries them.
const MAP_PLANES_MAGIC: i32 = 0x504C; // "PL" — one-way + ladder collision planes
const MAP_PARALLAX_MAGIC: i32 = 0x5058; // "PX" — per-layer scroll multipliers

/// One hardware particle: a sprite the ENGINE owns and steps, not the game.
///
/// Fireworks from Tish would be one `sprite_set_pos` per particle per frame — thirty particles is
/// sixty boxed native calls a frame, on top of everything else the scene is doing. Stepping them
/// here costs the game nothing per frame: it spawns a burst and forgets about it.
///
/// Position and velocity are 8.8 fixed point so a particle can drift at less than a pixel a frame;
/// at whole-pixel velocities a burst looks like a starburst decal rather than an explosion.
#[derive(Clone, Copy)]
struct Particle {
    sprite: usize,
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    gravity: i32,
    /// Velocity retained per frame, in 1/256ths. 256 is frictionless; 250 is smoke slowing to a
    /// stop. One multiply and one shift per axis per frame.
    drag: i32,
    /// Constant horizontal acceleration, in 1/256ths of a pixel per frame. Rain leans; snow drifts.
    wind: i32,
    life: i32,
    /// The life it STARTED with, so `frame` can be a function of how far through it is. This is how
    /// a particle fades on hardware that will not give us per-object alpha — see `fx_spawn`.
    life0: i32,
    frame0: i32,
    framen: i32,
    sheet: i32,
    /// Emitter slot that owns this, or -1 for a one-shot with no emitter behind it. Used to keep a
    /// single emitter inside its own share of the budget and to free the slot when it dies.
    owner: i32,
}

/// A continuous (or one-shot) source of particles, stepped by the engine.
///
/// The reason this exists rather than a game calling `fx_burst` on a timer: rate is fractional. Rain
/// at twelve drops a second is 0.2 particles per frame, which a game loop cannot express without
/// keeping an accumulator, and every game would keep the same one. It also means the LIBRARY owns
/// the sprite budget — an emitter that would exceed its share simply emits fewer this frame, which
/// no caller has to notice or handle.
struct Emitter {
    /// Generation-tagged handle. A slot is reused after an emitter dies, so a stale id held by a
    /// game must not steer whatever landed in the slot next — every call checks this, not the index.
    id: i32,
    sheet: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    shape: i32,
    /// 0..255 around the circle, or -1 for omnidirectional (a burst).
    dir: i32,
    spread: i32,
    speed: i32,
    speed_var: i32,
    gravity: i32,
    drag: i32,
    wind: i32,
    life: i32,
    /// Particles per frame in 8.8 fixed point, with `acc` carrying the fraction between frames.
    rate: i32,
    acc: i32,
    /// This emitter's own ceiling. Stops one heavy effect from spending the whole effects budget
    /// and leaving nothing for the next one.
    max: i32,
    frame0: i32,
    framen: i32,
    /// Frames left to emit for. -1 runs until stopped; 0 means it has stopped emitting and is only
    /// waiting for its last particles to die.
    duration: i32,
    /// Particles currently alive from this emitter.
    live: i32,
}

pub(crate) struct GbaCtx {
    gfx: Graphics<'static>,
    input: ButtonController,
    sprites: Vec<SpriteData>,
    /// Live effect particles, stepped once per `frame()`. Empty costs nothing.
    particles: Vec<Particle>,
    /// Live emitters, stepped once per `frame()` just before the particles they feed.
    emitters: Vec<Emitter>,
    /// Handle counter for `Emitter::id`. Only ever increments, so a freed handle is never reissued.
    fx_next_id: i32,
    /// Ceiling on OAM entries the effects layer may hold at once, or -1 to size it automatically
    /// from what the game is actually using. See `fx_headroom_of`.
    fx_budget: i32,
    /// Entries always left free for the GAME, on top of whatever effects are holding. This is the
    /// number that stops a victory burst from making the player sprite disappear.
    fx_reserve: i32,
    /// Particle randomness, deliberately NOT `shake_seed`: how much rain is falling should not
    /// decide which way the screen shakes, or a replay diverges on a cosmetic difference.
    fx_seed: u32,
    /// Screen brighten (BLDY toward white), 0..16 — the counterpart to `fade`'s darken. Decays on
    /// its own so a game fires one call and never has to remember to switch it off.
    flash: u8,
    flash_decay: u8,
    /// Screen fade toward WHITE (BLDY increase), 0..16 — `fade`'s counterpart for a transition that
    /// blows out instead of dipping to black. Unlike `flash` it does NOT decay: a transition owns
    /// its ramp and a level that faded itself back in would fight the driver.
    fade_white: u8,
    /// Alpha blend weights (BLDALPHA), 0..16 each, or `None` when no alpha blend is asked for.
    ///
    /// ⚠️ BLDCNT HOLDS ONE EFFECT FOR THE WHOLE SCREEN. `fade`, `fx_flash` and this share the same
    /// two-bit field, and agb's `Blend::alpha/brighten/darken` each begin with a `reset()` — so the
    /// last caller in a frame silently erases the others. `frame()` arbitrates them in one explicit
    /// priority chain (fade > fade_white > flash > alpha) rather than letting call order decide.
    blend_alpha: Option<(u8, u8)>,
    /// MOSAIC (0x0400_004C) block size minus one, 0..15, for backgrounds and for objects.
    ///
    /// Neither agb nor this crate owns this register: agb models the per-BG enable bit (BGxCNT bit
    /// 6) and the per-OBJ bit (OAM attr0 bit 12) as hardcoded `false` with no API to set them. So
    /// both the size register and the enable bits are written by hand AFTER `frame.commit()` — that
    /// point is inside vblank (commit waits for it) and is the one place agb has finished writing
    /// and will not clobber us until the next commit, which re-pokes.
    mosaic_bg: u8,
    mosaic_obj: u8,
    /// Whether the last committed frame had a mosaic on — see the use in `frame()`.
    mosaic_live: bool,
    /// ── The hardware WINDOWS (WIN0/WIN1/WINOUT) ────────────────────────────────────────────────
    /// The GBA can restrict which layers draw inside a rectangle and, separately, outside every
    /// rectangle. That is the register behind a spotlight, a lit room in a dark level, an iris
    /// transition, and a stealth vision cone — none of which were reachable from tish before,
    /// because nothing exposed these registers at all.
    ///
    /// `win_on[i]` enables window i; `win_box[i]` is its rect; `win_in_mask[i]` and `win_out_mask`
    /// are layer masks in the same bit order agb's `Window` uses (bit 0..3 = the nth SHOWN
    /// background, bit 4 = objects, bit 5 = blending).
    win_on: [bool; 2],
    win_box: [(i32, i32, i32, i32); 2],
    win_in_mask: [u8; 2],
    win_out_mask: u8,
    /// A CIRCULAR window on WIN0, as (cx, cy, r): the left/right edges are recomputed per scanline
    /// and fed to the window's horizontal register by HBlank DMA. ⚠️ There is exactly one HBlank DMA
    /// slot per frame in this agb fork (`GraphicsFrame::add_dma` overwrites), so a circle window and
    /// a `bg_bands` banded layer cannot both run on the same frame — the same single-channel limit
    /// `scene_bands` already documents.
    win_circle: Option<(i32, i32, i32)>,
    /// Latch so the "circle window lost the HBlank DMA slot" diagnostic prints ONCE. It fires from
    /// inside `frame()`, so an unlatched warning would emit sixty lines a second and bury whatever
    /// the game was actually logging.
    warned_win_dma: bool,
    /// Screen shake, as a DAMPED SPRING in 8.8 fixed point: position and velocity per axis, plus
    /// the spring constant and damping. Offsets the camera, the UI canvas and the HUD sprites at
    /// compose time, which is a handful of register writes rather than a redraw.
    ///
    /// This is `packages/feel.tish`'s spring, moved down into the engine. It replaced a
    /// countdown-plus-random-jitter shake that lived here, and the swap went that direction for two
    /// reasons the Tish version had already found and documented: bumps SUM (a countdown is
    /// overwritten by the next call and loses its decay, which is exactly wrong for a cascade that
    /// fires three impacts in one resolution), and it settles on POSITION AND VELOCITY (an
    /// oscillating spring crosses zero every half period with its velocity at maximum, so a
    /// position-only test snaps the screen square on the first crossing and turns a shake into a
    /// single flick). What the engine adds is reach: `feel` could only move the UI canvas through
    /// `ui_scroll`, so its shake was invisible on a scrolling map and on a screen drawn in sprites.
    shake_x: i32,
    shake_vx: i32,
    shake_y: i32,
    shake_vy: i32,
    shake_k: i32,
    shake_d: i32,
    /// Set while the spring is displaced, cleared the frame it settles — so the write that lands
    /// every surface square happens exactly ONCE. A canvas left a pixel off reads as a rendering
    /// fault, and writing zero on every idle frame is two pointless register writes forever.
    shake_live: bool,
    shake_seed: u32,
    /// The UI canvas scroll the GAME asked for. The shake adds its offset on top of this rather
    /// than overwriting it, so a game that scrolls its own canvas (packages/feel does) keeps
    /// working while shaking.
    ui_scroll_x: i32,
    ui_scroll_y: i32,
    /// Freed sprite indices (from `sprite_destroy`) available for `sprite_new`/`sprite_create` to
    /// reuse — so spawning and removing sprites over a game's lifetime keeps the arena/VRAM bounded.
    sprite_free: Vec<usize>,
    bg_ids_buf: Vec<RegularBackgroundId>,
    sprite_order_buf: Vec<usize>,
    /// Tiled background layers (drawn behind sprites, in registration order).
    backgrounds: Vec<BgData>,
    affine_bgs: Vec<AffineData>,
    billboards: Vec<Billboard>,
    /// Streamed tile layers (for maps larger than the screen) + the camera they scroll to.
    /// Layers are REUSED across scene loads (the RegularBackground's 2KB tile box stays alive)
    /// — dropping and reallocating them every warp fragments EWRAM until the next alloc fails.
    stream_layers: Vec<StreamLayer>,
    /// How many of `stream_layers` are live for the current scene (rest are dormant pool slots).
    stream_active: usize,
    /// A scene's BACKDROP layers — the ones Tiled gave a parallax factor. They are plain
    /// hardware-wrapping backgrounds rather than `InfiniteScrolledMap`s, which is not an
    /// optimisation but a requirement: a streamed layer only has a 256x256 window of tiles resident,
    /// so it cannot be scrolled far from the camera, and a backdrop's whole job is to be somewhere
    /// else. Wrapping also means the 16x16 cells at the layer's top-left tile the screen forever,
    /// which is what a sky is, and it is what makes per-scanline banding safe (see `BgData::bands`).
    ///
    /// A separate pool from `backgrounds` so `bg_new` handles stay stable across scene loads, and
    /// reused across loads for the same reason `stream_layers` is — reallocating a 2KB tile box on
    /// every warp fragments EWRAM until something fails.
    scene_bgs: alloc::vec::Vec<BgData>,
    /// The background palettes most recently uploaded, kept so a game can ASK which entry holds a
    /// given colour instead of guessing. See [`bg_pal_get`].
    bg_pal: Option<&'static [agb::display::Palette16]>,
    scene_bg_active: usize,
    /// Set when stream layers were rebound to a new map; next prime forces a full tile refill.
    stream_dirty: bool,
    /// Handles of the HUD heart sprites (a reusable health readout); `hud_hearts_update` sets
    /// their frames from an hp value. Cleared with the sprite arena.
    hud_hearts: Vec<i32>,
    /// Last `(hp, perHeart)` pushed to the hearts, so `hud_hearts_update` skips rebuilding the
    /// heart sprites when the value hasn't changed (avoids per-frame `Object::new`).
    hud_hearts_last: (i32, i32),
    /// HUD text slots (`hud_text(slot, …)`): each is an independent line of front/screen-space text
    /// (e.g. a health readout in one slot, a menu in another). Cached per slot so the layout/object
    /// rebuild only runs when that slot's string or position changes.
    hud_text: Vec<HudTextSlot>,
    /// HUD bar slots (`hud_bar(slot, …)`) — graphical health/progress bars, cached per slot.
    hud_bars: Vec<HudBarSlot>,
    /// Leaked-once, cached text palettes keyed by (colours[], shadow) — `PaletteVramSingle` needs
    /// a `&'static Palette16`, and there are only a handful of distinct text styles in a game.
    text_palettes: Vec<(Vec<i32>, i32, &'static Palette16)>,
    /// Leaked-once bar palettes keyed by (fg, bg) 0xRRGGBB — index 1 = bg, 2 = fg, 3 = border.
    bar_palettes: Vec<(i32, i32, &'static Palette16)>,
    camera_x: i32,
    camera_y: i32,
    /// The currently-loaded ROM map's metadata (solid grid + spawns), if any.
    map_info: Option<MapInfo>,
    /// Software mixer — created on first `sound_play`/`music_play`/dialog WAV blip, not at boot.
    /// agb's `Mixer::new` always enables the Timer1 swap IRQ; constructing it eagerly made silent /
    /// PSG-only games (the topdown RPG port) pay that IRQ forever and trip `RefCell already borrowed` in `swap`
    /// under heavy cave/scene loads. `None` until sampled audio is used.
    mixer: Option<Mixer<'static>>,
    /// Live looping BGM channel (high priority). `music_play` stops the previous one first so
    /// area themes don't stack forever.
    music_channel: Option<ChannelId>,
    /// Optional `wav:` handle for the dialogue typewriter blip (−1 = silent). Set via
    /// `dialogue_set_blip`; played once per revealed letter group in `frame()`.
    dialog_blip: i32,
    /// Set once a game first plays a sound. Until then `frame()` skips `mixer.frame()` — the software
    /// mixer's per-frame DSP is a real fixed cost (a slice of the ~4389-tick 60fps budget) and a game
    /// with no audio (e.g. the shmup) shouldn't pay it. Once any sound plays it stays on for good.
    audio_used: bool,
    /// True while `mixer.frame()` runs. Nested pumps must no-op — agb's mixer buffer is a `RefCell`
    /// and a re-entrant `frame()` panics with `RefCell already borrowed`.
    audio_pumping: bool,
    /// Whether `psg::init` has run. The PSG needs powering on and routing exactly once, and it has to
    /// happen after agb's mixer exists (which clobbers the PSG volume bits), so it is done lazily on
    /// the first PSG call rather than at startup — or re-run after a late mixer construct. Unlike
    /// `audio_used` this does NOT enable any per-frame work: PSG voices run in hardware and cost
    /// nothing to leave playing.
    psg_ready: bool,
    /// Screen fade-to-black level, 0 (clear) .. 16 (full black), applied via the hardware brightness
    /// blend (BLDY) each frame — drives scene transitions. 0 = no blend cost. See `fade_typed`.
    fade: u8,
    /// Active dialogue. The panel is sprites (`dialog_box`) from retained VRAM
    /// (`dialog_panel_top` / `dialog_panel_fill`); the text is a dedicated tiled background
    /// (`dialog_text_bg`) whose transparent pixels let the panel show through. Panel VRAM is
    /// allocated once via `ensure_dialog_panel` and never freed — see `BOX_PALETTE`.
    /// `dialog_groups` is the CURRENT page's body split into letter groups; the frame loop
    /// reveals them one at a time (typewriter) via `dialog_revealed`. `dialog_pages` holds
    /// every page of the message (advancing past a full page loads the next), and
    /// `dialog_speaker` is re-rendered on each page.
    dialog_panel_top: Option<SpriteVram>,
    dialog_panel_fill: Option<SpriteVram>,
    dialog_box: Vec<Object>,
    dialog_text_bg: Option<RegularBackground>,
    dialog_body: Option<RegularBackgroundTextRenderer>,
    dialog_name: Option<RegularBackgroundTextRenderer>,
    dialog_groups: Vec<LetterGroup>,
    dialog_pages: Vec<String>,
    dialog_page: usize,
    dialog_speaker: String,
    /// Choice state (empty `dialog_options` ⇒ a plain message, not a choice). The options
    /// render on a line below the question with a `>` cursor at `dialog_selected`. On
    /// confirm the selection is stashed in `dialog_result` and `dialog_choice_pending` is
    /// set; `dialogue_pump` then fires `dialog_choice_cb` AFTER the context borrow is
    /// released (so the callback may safely re-enter, e.g. to open another box).
    dialog_options: Vec<String>,
    dialog_opts_r: Option<RegularBackgroundTextRenderer>,
    dialog_selected: usize,
    dialog_choice_cb: Option<Value>,
    dialog_choice_pending: bool,
    dialog_result: usize,
    dialog_revealed: usize,
    dialog_active: bool,
    dialog_timer: i32,
    /// A reusable UI TEXT CANVAS for menus: a dedicated background that `ui_text` blits glyph TILES
    /// into (no OAM, no sprite-VRAM — unlike the sprite-based `text_draw`, which caps out a text-heavy
    /// menu's sprite VRAM). Rebuilt fresh on each `ui_begin`, so the layout engine can re-lay-out on
    /// change without leaking. Shown in `frame()` at P0 (above streamed map layers — those use P3..P0
    /// by Tiled order, so a P2 canvas lost to Paths/Props and terrain punched through dialogs).
    /// `ui_palettes` caches (0xRRGGBB colour → INDEX within the one shared `UI_PAL_SLOT` palette), so a
    /// single tile can hold many colours (glyph pixels store their colour's index) — no per-tile bleed.
    ui_bg: Option<RegularBackground>,
    /// Per-pixel destructible terrain (see the `terrain_*` section). `None` until `terrain_new`.
    terrain: Option<Terrain>,
    terrain_pal: [i32; 16],
    /// One 16-colour bank per planet — see `terrain_pal_bank`.
    terrain_banks: [[i32; 16]; 16],
    /// One shared transparent tile the canvas points every old cell at on `ui_begin` (releasing the
    /// previous render's tiles) — keeps the background PERSISTENT (no screenblock churn → no flicker)
    /// while its tiles are rebuilt each render.
    ui_blank: Option<DynamicTile16>,
    ui_palettes: Vec<(i32, u8)>,
    /// Colours requested after the shared UI palette bank filled (see `ensure_ui_palette`). Non-zero
    /// means some UI is painting in the wrong colour; reported by `ui_mem_report()`.
    ui_pal_overflow: u32,
    /// The UI canvas's live dynamic tiles, in no particular order. Reached through [`GbaCtx::ui_cell`].
    ///
    /// Tiles are PERSISTENT + reused across renders (`ui_begin` blanks each tile's PIXELS, never
    /// freeing) so the ~512-tile dynamic-tile VRAM pool isn't churned — re-creating a background +
    /// tiles every render exhausts it (agb frees a background's tiles lazily). Reuse keeps the pool
    /// bounded to the busiest screen; sharing a tile across `ui_text` calls also accumulates (ORs)
    /// glyph pixels so overlapping text doesn't clip. Freed by `ui_clear`.
    ui_tiles: alloc::vec::Vec<UiTile>,
    /// Has `ui_begin` already run since the last `frame()`? See the panic in `ui_begin`.
    ui_began_this_frame: bool,
    /// Tile lookup: a DENSE 32×32 grid (the screenblock's own size) indexed by [`ui_cell_idx`], where
    /// each entry is [`UI_CELL_EMPTY`], a solid-fill marker (see [`UI_CELL_SOLID_LO`]), or
    /// `ui_tiles` index + 1.
    ///
    /// Flat, not a `BTreeMap<(i32,i32), _>` keyed by tile: every glyph row of every in-place text
    /// patch does one lookup per (tile column, pixel row), and on the GBA's uncached bus a B-tree
    /// descent cost ~4000 cycles — a single 3-line description repaint was ~80ms, which alone blew
    /// the <0.2s selection budget. An index is ~30 cycles. Indirecting through a `u16` (rather than
    /// storing tiles in the grid) keeps this at 2KB: a 12KB grid of `Option<DynamicTile16>` was
    /// enough to OOM the GBA heap on a dense shop screen.
    ui_cell: alloc::vec::Vec<u16>,
    /// Shared solid-colour DynamicTiles for opaque panel fills (`ui_rect` filled, tile-aligned) —
    /// ONE PER FILL COLOUR, indexed by palette entry. Every solid cell points at the tile for its
    /// own colour (one VRAM slot per colour, not one per cell): a full-screen dialog fill would
    /// otherwise allocate ~200 DynamicTile16s and OOM the GBA heap.
    ///
    /// Keyed by colour rather than "the last fill" because a screen paints several: a menu backdrop,
    /// then a list panel, then a detail panel. With a single shared tile the second fill DROPPED the
    /// first — freeing a VRAM slot that the first panel's cells were still pointing at, so those
    /// cells showed whatever glyph tile took the slot next (the game visible straight through a shop
    /// panel), and a later copy-on-write in them filled with the WRONG panel's colour. Held until
    /// `ui_blank_tiles` / `ui_forget_tiles`, by which point no cell references them.
    ui_solids: alloc::vec::Vec<Option<DynamicTile16>>,
    /// Cached glyph layout for the typewriter (`ui_text_span`): a per-page reveal calls `ui_text_span`
    /// every frame with the SAME text + growing `to`, but agb's `Layout` (shaping/line-breaking) is O(all
    /// glyphs) — re-running it each char was O(n²) and, on hardware wait-states, ~a frame of CPU PER char,
    /// so a line took seconds. Shape ONCE, store each glyph pixel-row's (group first-char, abs x/y, packed
    /// row), and later reveals just blit the rows in range. Rebuilt when the (text, font, maxw, x, y) key
    /// changes; cleared on `ui_begin`/`ui_clear`.
    ui_reveal: Option<RevealCache>,
    /// Compositing scratch for `ui_text_box`, kept for the life of the program. It is sized by the box
    /// (a full-width paragraph is ~30 tile columns × 8 pixel rows of `u32`, so 1-2K), and every in-place
    /// text patch needs one — a fresh Vec per call meant thousands of 1-2K allocate/free pairs during a
    /// menu paint, which is how a heap ends up unable to hand out a contiguous 4K later.
    ui_box_scratch: alloc::vec::Vec<u32>,
    /// The reveal cache's row buffer, parked here whenever the cache is invalidated (every box close
    /// blanks the canvas). A page of dialogue is several hundred rows at 16 bytes, so letting the buffer
    /// die with the cache means the NEXT box grows a new one from nothing — 512B, 1K, 2K, 4K, each step a
    /// fresh contiguous request on a heap the game has since filled. That ladder is what took akari down
    /// mid-conversation. The capacity outlives the cache; the contents never do.
    ui_row_spare: alloc::vec::Vec<(i32, i32, i32, u32)>,
    /// High-water mark of simultaneously live canvas tiles (diagnostic; see `ui_mem_report`).
    ui_peak_tiles: usize,
    /// Memoised `text_width` for the built-in FONT only ((font, text) → px). Imported `font:` fonts use
    /// baked [`FontMetrics`] advances instead (no Layout). Bounded and freed on `ui_clear`.
    tw_cache: alloc::collections::BTreeMap<(i32, alloc::string::String), i32>,
    /// Leaked `Gba` for lazy `save_init` and lazy mixer construct (graphics/timers taken at boot;
    /// `save` and `mixer` fields remain until first use).
    gba: *mut agb::Gba,
    /// True after `save_init` — fixed-layout SRAM needs no manager object (see `save_api`).
    save_ready: bool,
    /// Timer2 as a free-running µs-ish counter (Divider64 → 3.815µs/tick, wraps
    /// every ~250ms) for frame-time instrumentation.
    timer: Timer,
    /// Per-frame timing maxima (Timer2 ticks) for the audio/perf review: render (draw list build/show),
    /// commit (incl. vblank wait), mixer pump, whole-frame period. Read via `frame_stats()`, reset by it.
    dbg_render: i32,
    dbg_commit: i32,
    dbg_mix: i32,
    dbg_total: i32,
    dbg_drops: i32,
    dbg_last: i32,
    /// Max Timer2 ticks the mixer went WITHOUT a pump (any pump: frame() or pump_audio). If this exceeds
    /// the ~3-buffer slack (~13000 ticks ≈ 50ms) the BGM underran — the key metric for "does a heavy
    /// synchronous build (menu/scene) starve the audio?".
    dbg_pumpgap: i32,
    dbg_lastpump: i32,
}

/// Cached shaped-glyph rows for the typewriter — see `GbaCtx::ui_reveal`.
struct RevealCache {
    text: alloc::string::String,
    font_handle: i32,
    maxw: i32,
    align: u8,
    x: i32,
    y: i32,
    /// One entry per glyph pixel-row: (first char index of its group, absolute px x, absolute px y, packed 4bpp row).
    rows: alloc::vec::Vec<(i32, i32, i32, u32)>,
}

static CTX: SingleCore<RefCell<Option<GbaCtx>>> = SingleCore::new(RefCell::new(None));

/// Registry for `scene:` imports (see `tish_gba_scenepack::include_scene!`): a scene packs its
/// A scene's baked tileset: palettes + full-screen tile data (agb `include_background_gfx!` output).
type SceneBg = (
    &'static [Palette16],
    &'static agb::display::tile_data::TileData,
);

/// Registered scenes: each pairs its tileset (stored HERE) with its map handle (the `map:` arena).
///
/// The tileset is held in tish-agb rather than the runtime `__asset_register_bg` arena on purpose.
/// That arena backs the `background:` import scheme, whose tish handles are COMPILE-TIME per-scheme
/// indices that must equal their runtime registration order. If scenes registered their tilesets
/// there too, every `background:` handle would be offset by the number of scenes — so `bg_new(title)`
/// would draw a scene's tileset instead (the #552 cross-scheme arena collision). Keeping scene
/// tilesets out of that arena lets `background:` handles stay correct with no compiler change.
static SCENES: SingleCore<RefCell<alloc::vec::Vec<(SceneBg, i32)>>> =
    SingleCore::new(RefCell::new(alloc::vec::Vec::new()));

/// Called from `tish_gba_scenepack::include_scene!`'s generated registration code — not part of
/// the tish native-fn ABI (plain args, not `&[Value]`), since it runs from ordinary Rust before
/// `run()`. Takes the scene's baked tileset directly (kept out of the `background:` arena — see
/// `SCENES`) plus its `map:` handle. Returns the scene handle `scene_stream` expects.
pub fn native_scene_register(
    palettes: &'static [Palette16],
    tiledata: &'static agb::display::tile_data::TileData,
    map_idx: i32,
) -> i32 {
    SCENES.with(|s| {
        let mut v = s.borrow_mut();
        v.push(((palettes, tiledata), map_idx));
        (v.len() - 1) as i32
    })
}

/// Songs registered by `chip:` imports, in import order — the same arena-per-scheme arrangement as
/// `SCENES` above, and for the same reason.
static SONGS: SingleCore<RefCell<alloc::vec::Vec<&'static chiptune::Song>>> =
    SingleCore::new(RefCell::new(alloc::vec::Vec::new()));

/// The one chiptune player. Songs replace each other rather than layering, exactly like `music_play`.
static CHIP: SingleCore<RefCell<chiptune::Player>> =
    SingleCore::new(RefCell::new(chiptune::Player::new()));

/// deck songs registered by `deck:` imports.
static DECK_SONGS: SingleCore<RefCell<alloc::vec::Vec<&'static deck_player::DeckSong>>> =
    SingleCore::new(RefCell::new(alloc::vec::Vec::new()));

/// The DECK player — mutually exclusive with [`CHIP`] on the PSG.
static DECK: SingleCore<RefCell<deck_player::Player>> =
    SingleCore::new(RefCell::new(deck_player::Player::new()));

/// Called from `tish_gba_scenepack::include_chip!`'s generated registration code.
pub fn native_song_register(song: &'static chiptune::Song) -> i32 {
    SONGS.with(|s| {
        let mut v = s.borrow_mut();
        v.push(song);
        (v.len() - 1) as i32
    })
}

/// Called from `tish_gba_scenepack::include_deck!`'s generated registration code.
pub fn native_deck_song_register(song: &'static deck_player::DeckSong) -> i32 {
    DECK_SONGS.with(|s| {
        let mut v = s.borrow_mut();
        v.push(song);
        (v.len() - 1) as i32
    })
}

/// Timer2 ticks per display frame at ~59.7275 Hz (same clock `frame_stats` / `ticks` use).
const MUSIC_FRAME_TICKS: i32 = 4389;
/// Cap catch-up so a multi-second stall does not fire dozens of note-ons in one burst.
const MUSIC_CATCHUP_MAX: i32 = 8;

/// Wall-clock of the last music step (Timer2). 0 = unset.
static MUSIC_LAST: SingleCore<Cell<i32>> = SingleCore::new(Cell::new(0));

/// When non-zero, `ui_text` / `ui_text_box` / `ui_rect` skip their per-call mixer feed.
/// Use around a batch of opaque text patches (list scroll refill), then `audio_pump()` once.
static AUDIO_DEFER: SingleCore<Cell<i32>> = SingleCore::new(Cell::new(0));

#[inline]
fn ui_feed_audio(ctx: &mut GbaCtx) {
    if AUDIO_DEFER.with(|c| c.get()) != 0 {
        return;
    }
    pump_audio(ctx);
}

/// Advance chiptune or DECK sequencer one frame. Unlike `pump_audio` this is not a deadline: the PSG
/// keeps sounding on its own, so a late call delays the next note rather than corrupting the audio.
/// Duck envelope state: target depth, per-frame attack/release steps, and the hold counter.
/// [0] gain 0..=64 · [1] target · [2] attack step · [3] release step · [4] hold frames left
static DUCK: SingleCore<Cell<[i32; 5]>> = SingleCore::new(Cell::new([64, 64, 8, 4, 0]));

/// Advance the duck one frame. A multiply and a shift when it moves, one compare when it does not —
/// the divides that pick the step sizes are paid once, in `audio_duck`, not per frame.
fn step_duck(ctx: &mut GbaCtx) {
    let mut d = DUCK.with(|c| c.get());
    if d[4] > 0 {
        d[4] -= 1;
        if d[4] == 0 {
            d[1] = 64; // hold expired: release back to unattenuated
        }
    }
    if d[0] != d[1] {
        if d[0] > d[1] {
            d[0] = (d[0] - d[2]).max(d[1]);
        } else {
            d[0] = (d[0] + d[3]).min(d[1]);
        }
        DECK.with(|p| p.borrow_mut().set_duck(d[0] as u8));
        // The PSG master is 0..7 and shared with agb's routing enables, so it is written only when
        // the 0..7 value actually changes rather than every frame.
        psg::master(((d[0] * 7) >> 6) as u8);
    }
    DUCK.with(|c| c.set(d));
    let _ = ctx;
}

#[inline]
fn step_music_once(ctx: &mut GbaCtx) {
    step_duck(ctx);
    let deck_playing = DECK.with(|p| p.borrow().playing());
    if deck_playing {
        if DECK.with(|p| p.borrow().has_pcm()) {
            ensure_mixer(ctx);
            ctx.audio_used = true;
        }
        DECK.with(|p| p.borrow_mut().step(&mut ctx.mixer));
    } else {
        CHIP.with(|p| p.borrow_mut().step());
    }
}

/// Keep BGM on wall-clock time during long synchronous work (menu scroll refill, uiRender, etc.).
///
/// `frame()` used to step the sequencer exactly once per call. A scroll that spends several vblanks
/// inside `uiRelayoutRows` never returned to `frame()`, so PSG *and* PCM music stalled — PSG does not
/// need the mixer, but it still needs `step` for the next note. This catch-up advances one sequencer
/// frame per ~vblank of elapsed Timer2 time (capped), and always pumps the DirectSound mixer.
#[inline]
fn music_catchup() {
    with_ctx(|ctx| {
        let now = ctx.timer.value() as i32;
        let last = MUSIC_LAST.with(|c| c.get());
        if last == 0 {
            step_music_once(ctx);
            pump_audio(ctx);
            MUSIC_LAST.with(|c| c.set(now));
            return;
        }
        let mut gap = now - last;
        if gap < 0 {
            gap += 65536;
        }
        let mut steps = gap / MUSIC_FRAME_TICKS;
        if steps < 1 {
            pump_audio(ctx);
            return;
        }
        if steps > MUSIC_CATCHUP_MAX {
            steps = MUSIC_CATCHUP_MAX;
        }
        let mut new_last = last;
        let mut i = 0;
        while i < steps {
            step_music_once(ctx);
            pump_audio(ctx);
            new_last += MUSIC_FRAME_TICKS;
            if new_last >= 65536 {
                new_last -= 65536;
            }
            i += 1;
        }
        MUSIC_LAST.with(|c| c.set(new_last));
    });
}

/// Advance chiptune or DECK sequencer one frame (legacy single-step entry used only if needed).
#[inline]
fn step_music_frame() {
    music_catchup();
}

/// Construct the software mixer on first sampled-audio use. agb enables the Timer1 swap IRQ inside
/// `Mixer::new`; delaying that until a WAV actually plays keeps silent/PSG-only games off the
/// `RefCell already borrowed` tripwire in `swap`. If the PSG was already powered, re-init it —
/// mixer enable clobbers the PSG mix bits in `SOUNDCNT_H`.
///
/// Field-wise form so it can run inside `frame()` while `ctx.gfx` is borrowed.
#[inline]
fn ensure_mixer_fields(mixer: &mut Option<Mixer<'static>>, gba: *mut agb::Gba, psg_ready: bool) {
    if mixer.is_some() {
        return;
    }
    // SAFETY: `gba` was leaked at first `with_ctx`; MixerController is exclusive to us.
    let m = unsafe { (*gba).mixer.mixer(deck_player::MIXER_FREQUENCY) };
    *mixer = Some(m);
    if psg_ready {
        psg::init();
    }
}

#[inline]
fn ensure_mixer(ctx: &mut GbaCtx) {
    ensure_mixer_fields(&mut ctx.mixer, ctx.gba, ctx.psg_ready);
}

/// Pump the software mixer MID-OPERATION. The audio only mixes in the main loop's `frame()`, but a long
/// synchronous build — the pause-menu layout, a dialog page's text shaping, a scene's tile stream — never
/// returns to `frame()` for hundreds of ms, so the mixer's ~3-buffer (~50ms) slack drains and the BGM
/// underruns/stutters BADLY for the whole build (the acute "stutters while loading the menu" case). agb's
/// `mixer.frame()` fills one buffer and no-ops (early-returns) when all buffers are full, so calling this
/// from the hot functions that dominate long builds (text measure/shape/paint, rect fill, tile stream) is
/// ~free on a normal frame yet keeps the audio fed throughout a heavy one. Gated on `audio_used` so a
/// silent game pays nothing.
#[inline]
fn pump_audio(ctx: &mut GbaCtx) {
    if ctx.audio_used && !ctx.audio_pumping {
        if let Some(mixer) = ctx.mixer.as_mut() {
            let now = ctx.timer.value() as i32;
            let gap = {
                let d = now - ctx.dbg_lastpump;
                if d < 0 {
                    d + 65536
                } else {
                    d
                }
            };
            if ctx.dbg_lastpump != 0 && gap > ctx.dbg_pumpgap {
                ctx.dbg_pumpgap = gap;
            }
            ctx.dbg_lastpump = now;
            ctx.audio_pumping = true;
            mixer.frame();
            ctx.audio_pumping = false;
        }
    }
}

/// Run `f` against the game context, lazily claiming the `Gba` peripheral bundle
/// from the runtime facade on first use (leaked to `'static` so `Graphics` can
/// live in the static).
pub(crate) fn with_ctx<R>(f: impl FnOnce(&mut GbaCtx) -> R) -> R {
    CTX.with(|c| {
        let mut guard = c.borrow_mut();
        if guard.is_none() {
            let gba = tishlang_runtime_gba::gba::take_gba()
                .expect("tish-agb: Gba peripheral already taken / not initialized");
            let gba: &'static mut agb::Gba = Box::leak(Box::new(gba));
            let gba_ptr: *mut agb::Gba = gba;
            let gfx = gba.graphics.get();
            // Mixer stays None until first WAV play — see `ensure_mixer`. Eager construct enabled
            // the Timer1 IRQ for silent games and panicked in `sw_mixer::swap` under cave loads.
            let timers = gba.timers.timers();
            let mut timer = timers.timer2;
            timer.set_divider(Divider::Divider64);
            timer.set_enabled(true);
            *guard = Some(GbaCtx {
                gfx,
                input: ButtonController::new(),
                sprites: Vec::new(),
                particles: Vec::new(),
                emitters: Vec::new(),
                fx_next_id: 1,
                fx_budget: -1,
                fx_reserve: FX_RESERVE_DEFAULT,
                fx_seed: 0x9E37_79B9,
                flash: 0,
                flash_decay: 0,
                fade_white: 0,
                blend_alpha: None,
                mosaic_bg: 0,
                mosaic_obj: 0,
                mosaic_live: false,
                win_on: [false, false],
                win_box: [(0, 0, 0, 0); 2],
                win_in_mask: [0x1F, 0x1F],
                win_out_mask: 0x3F,
                win_circle: None,
                warned_win_dma: false,
                shake_x: 0,
                shake_vx: 0,
                shake_y: 0,
                shake_vy: 0,
                shake_k: SHAKE_K,
                shake_d: SHAKE_D,
                shake_live: false,
                shake_seed: 0x2545_F491,
                ui_scroll_x: 0,
                ui_scroll_y: 0,
                sprite_free: Vec::new(),
                bg_ids_buf: Vec::new(),
                sprite_order_buf: Vec::new(),
                backgrounds: Vec::new(),
                affine_bgs: Vec::new(),
                billboards: Vec::new(),
                stream_layers: Vec::new(),
                stream_active: 0,
                scene_bgs: alloc::vec::Vec::new(),
                bg_pal: None,
                scene_bg_active: 0,
                stream_dirty: false,
                hud_hearts: Vec::new(),
                hud_hearts_last: (-1, -1),
                hud_text: {
                    let mut v = Vec::with_capacity(HUD_TEXT_SLOT_CAP);
                    v.resize_with(HUD_TEXT_SLOT_CAP, empty_hud_slot);
                    v
                },
                hud_bars: Vec::new(),
                text_palettes: Vec::new(),
                bar_palettes: Vec::new(),
                fade: 0,
                camera_x: 0,
                camera_y: 0,
                map_info: None,
                mixer: None,
                music_channel: None,
                dialog_blip: -1,
                audio_used: false,
                audio_pumping: false,
                psg_ready: false,
                dialog_panel_top: None,
                dialog_panel_fill: None,
                dialog_box: Vec::new(),
                dialog_text_bg: None,
                dialog_body: None,
                dialog_name: None,
                dialog_groups: Vec::new(),
                dialog_pages: Vec::new(),
                dialog_page: 0,
                dialog_speaker: String::new(),
                dialog_options: Vec::new(),
                dialog_opts_r: None,
                dialog_selected: 0,
                dialog_choice_cb: None,
                dialog_choice_pending: false,
                dialog_result: 0,
                dialog_revealed: 0,
                dialog_active: false,
                dialog_timer: 0,
                ui_bg: None,
                terrain: None,
                terrain_pal: [0; 16],
                terrain_banks: [[0; 16]; 16],
                ui_blank: None,
                ui_palettes: Vec::new(),
                ui_pal_overflow: 0,
                ui_tiles: alloc::vec::Vec::new(),
                ui_began_this_frame: false,
                ui_cell: alloc::vec::Vec::new(),
                ui_solids: alloc::vec::Vec::new(),
                ui_reveal: None,
                ui_box_scratch: alloc::vec::Vec::new(),
                ui_row_spare: alloc::vec::Vec::new(),
                ui_peak_tiles: 0,
                tw_cache: alloc::collections::BTreeMap::new(),
                gba: gba_ptr,
                save_ready: false,
                timer,
                dbg_render: 0,
                dbg_commit: 0,
                dbg_mix: 0,
                dbg_total: 0,
                dbg_drops: 0,
                dbg_last: 0,
                dbg_pumpgap: 0,
                dbg_lastpump: 0,
            });
            // Chat panel VRAM while OBJ space is still empty. Deferred to first dialogue_show, a
            // busy room (a cave scene with hero + NPC + HUD + doors) left the panel as stripes.
            ensure_dialog_panel(guard.as_mut().unwrap());
        }
        f(guard.as_mut().unwrap())
    })
}

pub(crate) fn num(args: &[Value], i: usize) -> f64 {
    match args.get(i) {
        Some(Value::Number(n)) => *n,
        _ => 0.0,
    }
}

// ── Exports (native-module ABI). ─────────────────────────────────────────────

/// `log(x)` — print a value to the mGBA debug log.
pub fn log(args: &[Value]) -> Value {
    if let Some(v) = args.first() {
        agb::println!("{}", v.to_display_string());
    }
    Value::Null
}

/// `vblank()` — block until the next vertical blank (raw, no rendering).
pub fn vblank(_args: &[Value]) -> Value {
    agb::interrupt::VBlank::get().wait_for_vblank();
    Value::Null
}

/// `timer_read()` — current Timer2 value in ticks (3.815µs each). One frame is
/// ~4389 ticks; subtract two reads to time a chunk of work.
pub fn timer_read(_args: &[Value]) -> Value {
    with_ctx(|ctx| Value::Number(ctx.timer.value() as f64))
}

/// Place `data` in the sprite arena and return its handle (index): reuse a slot freed by
/// `sprite_destroy` if one is available (its old `Object`/VRAM was already dropped), else append.
/// Recycling keeps the arena and VRAM bounded when a game spawns and removes sprites over time.
fn sprite_alloc(ctx: &mut GbaCtx, data: SpriteData) -> usize {
    if let Some(h) = ctx.sprite_free.pop() {
        ctx.sprites[h] = data;
        h
    } else {
        ctx.sprites.push(data);
        ctx.sprites.len() - 1
    }
}

/// `sprite_create()` — spawn the demo sprite; returns its handle (index).
pub fn sprite_create(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        let object = Object::new(&SPRITE);
        let handle = sprite_alloc(
            ctx,
            SpriteData {
                object: Some(object),
                x: 104,
                y: 72,
                hflip: false,
                visible: true,
                sheet: -1,
                frame: 0,
                hud: false,
                priority: -1,
                depth: 0,
                billboard: false,
            },
        );
        Value::Number(handle as f64)
    })
}

/// `sprite_new(assetHandle)` / `sprite_new(assetHandle, frame)` — spawn a sprite from a registered
/// `asset:` import; returns the object handle (index). `assetHandle` is the i32 an `asset:` import
/// binds to — the sprite sheet the generated `agb_main` registered into the facade before `run()`.
///
/// The frame used to be accepted and silently dropped, so callers asking for a sword or a rupee
/// all got frame 0 and had no way to tell: the topdown RPG port's whole HUD icon row rendered as hearts because
/// that is what frame 0 of its item sheet happens to be.
pub fn sprite_new(args: &[Value]) -> Value {
    let sheet_handle = num(args, 0) as i32;
    let frame = if args.len() > 1 {
        (num(args, 1) as i32).max(0)
    } else {
        0
    };
    with_ctx(|ctx| {
        let sheet = tishlang_runtime_gba::gba::asset_sheet(sheet_handle)
            .expect("tish-agb: sprite_new called with an unknown asset handle");
        let frame = (frame as usize).min(sheet.len() - 1);
        let object = Object::new(&sheet[frame]);
        let handle = sprite_alloc(
            ctx,
            SpriteData {
                object: Some(object),
                x: 104,
                y: 72,
                hflip: false,
                visible: true,
                sheet: sheet_handle,
                frame: frame as i32,
                hud: false,
                priority: -1,
                depth: 0,
                billboard: false,
            },
        );
        Value::Number(handle as f64)
    })
}

/// Native (non-`Value`) frame swap — called by the engine's animation system so it
/// doesn't box a `Value` per animated sprite per frame. Swaps the sprite to frame
/// `frame_idx` of its `sheet:`/`asset:` sheet (clamped); no-op for the demo sprite.
pub fn native_sprite_set_frame(handle: i32, frame_idx: i32) {
    let h = handle as usize;
    with_ctx(|ctx| {
        let sheet_handle = match ctx.sprites.get(h) {
            Some(s) if s.sheet >= 0 => s.sheet,
            _ => return,
        };
        if let Some(sheet) = tishlang_runtime_gba::gba::asset_sheet(sheet_handle) {
            let idx = (frame_idx.max(0) as usize).min(sheet.len().saturating_sub(1));
            let s = match ctx.sprites.get_mut(h) {
                Some(s) => s,
                None => return,
            };
            // No-op when already on this frame. Callers routinely RE-ASSERT a sprite's frame every
            // frame (a boss that sets its base frame each tick, a player that sets its idle frame when
            // not moving). Rebuilding the `Object` unconditionally drops the old one — often the last
            // holder of that frame's `SpriteVram`, freeing it — then `Object::new` re-allocates and
            // re-DMAs the tiles: a full per-frame VRAM upload of an UNCHANGED sprite (costly for the
            // 32×32 boss). Skipping the identity set removes that churn entirely.
            if s.frame == idx as i32 && s.object.is_some() {
                return;
            }
            s.frame = idx as i32;
            // Only touch VRAM if the sprite is currently resident; a released (off-screen) sprite
            // just records the frame and rebuilds on it when restored.
            if s.object.is_some() {
                s.object = Some(Object::new(&sheet[idx]));
            }
        }
    });
}

/// Native (non-`Value`) per-cell tilemap write — the counterpart to [`tilemap_new`], which can
/// only build a map wholesale.
///
/// Writes the four 8x8 GBA tiles of one 16px cell of `handle`'s background. This exists because a
/// grid-based game (a puzzle board, an inventory, a minimap) has to change individual cells at
/// runtime, and doing it with a sprite per cell is far more expensive than it looks:
/// `native_sprite_set_frame` rebuilds the agb `Object`, which costs roughly 2200 cycles a call.
/// A 7x9 board repainted that way spends its entire frame budget on the repaint. Four tilemap
/// entries cost a handful of stores.
///
/// `gid` is 1-based into the tileset (0 leaves the cell blank, matching `tilemap_new`).
pub fn native_tilemap_set(handle: i32, tileset: i32, cols: i32, col: i32, row: i32, gid: i32) {
    if col < 0 || row < 0 {
        return;
    }
    let cols = cols.max(1);
    with_ctx(|ctx| {
        let (_, tdata) = match tishlang_runtime_gba::gba::asset_bg(tileset) {
            Some(t) => t,
            None => return,
        };
        let bg = match ctx.backgrounds.get_mut(handle as usize) {
            Some(b) => &mut b.bg,
            None => return,
        };
        let tiles = &tdata.tiles;
        let settings = tdata.tile_settings;
        let w8 = 2 * cols;
        let (px, py) = (2 * col, 2 * row);
        if gid <= 0 {
            let blank = agb::display::tiled::TileSetting::BLANK;
            bg.set_tile(Vector2D::new(px, py), tiles, blank);
            bg.set_tile(Vector2D::new(px + 1, py), tiles, blank);
            bg.set_tile(Vector2D::new(px, py + 1), tiles, blank);
            bg.set_tile(Vector2D::new(px + 1, py + 1), tiles, blank);
            return;
        }
        let t = gid - 1;
        let (tcol, trow) = (t % cols, t / cols);
        let tl = ((2 * trow) * w8 + 2 * tcol) as usize;
        let bl = ((2 * trow + 1) * w8 + 2 * tcol) as usize;
        if bl + 1 >= settings.len() {
            return;
        }
        bg.set_tile(Vector2D::new(px, py), tiles, settings[tl]);
        bg.set_tile(Vector2D::new(px + 1, py), tiles, settings[tl + 1]);
        bg.set_tile(Vector2D::new(px, py + 1), tiles, settings[bl]);
        bg.set_tile(Vector2D::new(px + 1, py + 1), tiles, settings[bl + 1]);
    })
}

/// `tilemap_set(handle, tilesetHandle, tilesetCols, col, row, gid)` — write one 16px cell of a
/// map built by `tilemap_new`. See [`native_tilemap_set`].
pub fn tilemap_set(args: &[Value]) -> Value {
    native_tilemap_set(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
        num(args, 5) as i32,
    );
    Value::Null
}

/// Typed twin of [`tilemap_set`].
pub fn tilemap_set_typed(handle: i32, tileset: i32, cols: i32, col: i32, row: i32, gid: i32) {
    native_tilemap_set(handle, tileset, cols, col, row, gid);
}

/// Native (non-`Value`) per-TILE tilemap write — the 8px twin of [`native_tilemap_set`], for a board
/// whose cell IS a hardware tile.
///
/// EVERY OTHER TILEMAP CALL IN THIS FILE WORKS IN 16px CELLS, and that is a ceiling, not a
/// convenience: `native_tilemap_set` writes the 2x2 block at `(2*col, 2*row)`, and the background
/// `tilemap_new` allocates is `Background32x32` — 32x32 hardware tiles, which is 16x16 of those
/// cells. A classic falling-block well is 10 columns by 20 rows, so at 16px it is 320px tall: it does
/// not fit the screen, and it does not fit the map either. At 8px it is 80x160 and leaves 144px for a
/// HUD. `examples/blockfall` is the caller, and there was no way to write it without this.
///
/// ⚠️ NO `cols` PARAMETER, unlike every 16px call here — and the omission is deliberate rather than an
/// oversight. `include_background_gfx!` bakes tiles row-major over the source image's own 8x8 grid, so
/// a linear 1-based index already names a tile; the 16px calls need the width only to convert a
/// (col,row) cell into the four corners of its 2x2 block. Taking a width here and ignoring it would be
/// a parameter that lies, and tish does not check call arity, so nothing would ever catch a caller
/// passing it in the wrong slot.
///
/// `tile` is 1-based row-major over the tileset's 8x8 tiles; 0 blanks the tile, matching how
/// `tilemap_new` treats gid 0.
pub fn native_tilemap_set8(handle: i32, tileset: i32, col: i32, row: i32, tile: i32) {
    if col < 0 || row < 0 {
        return;
    }
    with_ctx(|ctx| {
        let (_, tdata) = match tishlang_runtime_gba::gba::asset_bg(tileset) {
            Some(t) => t,
            None => return,
        };
        let bg = match ctx.backgrounds.get_mut(handle as usize) {
            Some(b) => &mut b.bg,
            None => return,
        };
        let tiles = &tdata.tiles;
        let settings = tdata.tile_settings;
        let pos = Vector2D::new(col, row);
        if tile <= 0 {
            bg.set_tile(pos, tiles, agb::display::tiled::TileSetting::BLANK);
            return;
        }
        // `include_background_gfx!` lays the baked tiles out row-major over the source image's own
        // 8x8 grid, so the index is the index — no 2x2 unpacking, which is the whole point.
        let t = (tile - 1) as usize;
        if t >= settings.len() {
            return;
        }
        bg.set_tile(pos, tiles, settings[t]);
    })
}

/// `tilemap_set8(handle, tilesetHandle, col, row, tile)` — write one 8x8 tile of a map built by
/// `tilemap_new`. See [`native_tilemap_set8`].
pub fn tilemap_set8(args: &[Value]) -> Value {
    native_tilemap_set8(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
    );
    Value::Null
}

/// Typed twin of [`tilemap_set8`].
pub fn tilemap_set8_typed(handle: i32, tileset: i32, col: i32, row: i32, tile: i32) {
    native_tilemap_set8(handle, tileset, col, row, tile);
}

/// Free a live sprite's VRAM `Object` while keeping its slot + state (sheet, frame, position, flip).
/// Used by the engine to release off-screen sprites' VRAM so a big level with many entities doesn't
/// hold an agb sprite allocation for every one at once. `native_sprite_restore` rebuilds it. No-op if
/// the sprite is already released, freed, or the built-in demo sprite.
pub fn native_sprite_release(handle: i32) {
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(handle as usize) {
            if s.sheet >= 0 {
                s.object = None; // dropping the Object releases its VRAM sprite allocation
            }
        }
    });
}

/// Rebuild a released sprite's VRAM `Object` from its sheet on its current frame (see
/// `native_sprite_release`). No-op if it's already resident, freed, or the built-in demo sprite.
pub fn native_sprite_restore(handle: i32) {
    let h = handle as usize;
    with_ctx(|ctx| {
        match ctx.sprites.get(h) {
            Some(s) if s.object.is_none() && s.sheet >= 0 => {}
            _ => return,
        }
        let (sheet_handle, frame) = {
            let s = &ctx.sprites[h];
            (s.sheet, s.frame)
        };
        if let Some(sheet) = tishlang_runtime_gba::gba::asset_sheet(sheet_handle) {
            let idx = (frame.max(0) as usize).min(sheet.len().saturating_sub(1));
            let object = Object::new(&sheet[idx]);
            ctx.sprites[h].object = Some(object);
        }
    });
}

/// `sprite_set_frame(handle, frameIndex)` — swap a sprite to another frame of its
/// `sheet:`/`asset:` sheet (for manual animation, e.g. directional frames).
pub fn sprite_set_frame(args: &[Value]) -> Value {
    native_sprite_set_frame(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

/// Native (non-`Value`) sheet swap — re-point a live sprite at a DIFFERENT `sheet:`/`asset:`
/// sheet, then show frame `frame_idx` of it. Lets one sprite act as a shared overlay that draws
/// whichever asset it currently needs (e.g. a weapon-swing overlay switched to the attacker's
/// equipped weapon). No-op if the handle or sheet is unknown.
pub fn native_sprite_set_sheet(handle: i32, sheet_handle: i32, frame_idx: i32) {
    let h = handle as usize;
    with_ctx(|ctx| {
        if ctx.sprites.get(h).is_none() {
            return;
        }
        if let Some(sheet) = tishlang_runtime_gba::gba::asset_sheet(sheet_handle) {
            let idx = (frame_idx.max(0) as usize).min(sheet.len().saturating_sub(1));
            let object = Object::new(&sheet[idx]);
            if let Some(s) = ctx.sprites.get_mut(h) {
                s.object = Some(object);
                s.sheet = sheet_handle;
                s.frame = idx as i32;
            }
        }
    });
}

/// `sprite_set_sheet(handle, sheetHandle, frameIndex)` — re-bind a sprite to another asset sheet
/// (subsequent `sprite_set_frame` calls index the new sheet). Used for a shared overlay sprite.
pub fn sprite_set_sheet(args: &[Value]) -> Value {
    native_sprite_set_sheet(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// Native (non-`Value`) sprite reposition — called directly by
/// `tish_gba_game_engine`'s render system so it doesn't box a `Value` per sprite
/// per frame. The `Value`-ABI [`sprite_set_pos`] wraps this.
pub fn native_sprite_set_pos(handle: i32, x: i32, y: i32) {
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(handle as usize) {
            s.x = x;
            s.y = y;
        }
    });
}

/// `sprite_set_pos(handle, x, y)` — set a sprite's pixel position.
pub fn sprite_set_pos(args: &[Value]) -> Value {
    native_sprite_set_pos(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// Resolve a sprite's explicit priority override (-1 = automatic) against the pass default.
fn explicit_priority(p: i8, auto: Priority) -> Priority {
    match p {
        0 => Priority::P0,
        1 => Priority::P1,
        2 => Priority::P2,
        3 => Priority::P3,
        _ => auto,
    }
}

/// `sprite_set_hud(handle, on)` — mark a sprite as HUD: drawn in screen space (its `sprite_set_pos`
/// is a screen pixel position, unaffected by the camera) at the front priority, for a persistent
/// overlay like a hearts/health bar. `on = 0` reverts it to a normal camera-relative world sprite.
pub fn sprite_set_hud(args: &[Value]) -> Value {
    let handle = num(args, 0) as usize;
    let on = num(args, 1) != 0.0;
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(handle) {
            s.hud = on;
        }
    });
    Value::Null
}

/// `sprite_set_priority(handle, pri)` — explicit background-relative priority for one sprite:
/// 0..3 as on hardware (0 front). Pass -1 to revert to automatic (HUD front, world P2). Priority
/// orders a sprite against BACKGROUNDS only; sprite-vs-sprite overlap stays OAM (creation) order.
/// The one thing this buys that nothing else can: a HUD sprite at priority 1 renders UNDER the
/// P0 text canvas, so canvas text/vector overlays composite on top of a sprite image.
pub fn sprite_set_priority(args: &[Value]) -> Value {
    let handle = num(args, 0) as usize;
    let pri = num(args, 1) as i32;
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(handle) {
            s.priority = if (0..=3).contains(&pri) {
                pri as i8
            } else {
                -1
            };
        }
    });
    Value::Null
}

/// `sprite_set_depth(handle, z)` — painter's-algorithm depth for world sprites (isometric/y-sort).
/// Higher `z` = nearer the camera = drawn IN FRONT of lower-`z` sprites that overlap it. For an
/// iso tile at grid `(col,row)` use `z = col + row`; for a y-sorted top-down actor use `z = y`.
pub fn sprite_set_depth(args: &[Value]) -> Value {
    let handle = num(args, 0) as usize;
    let z = num(args, 1) as i16;
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(handle) {
            s.depth = z;
        }
    });
    Value::Null
}

/// `input_x()` — d-pad horizontal: -1 (left), 0, or 1 (right).
pub fn input_x(_args: &[Value]) -> Value {
    with_ctx(|ctx| Value::Number(ctx.input.x_tri() as i32 as f64))
}

/// `input_y()` — d-pad vertical: -1 (up), 0, or 1 (down).
pub fn input_y(_args: &[Value]) -> Value {
    with_ctx(|ctx| Value::Number(ctx.input.y_tri() as i32 as f64))
}

/// Map a tish button code to an agb `Button`. Codes: 0=A, 1=B, 2=Select, 3=Start,
/// 4=L, 5=R, 6=Up, 7=Down, 8=Left, 9=Right. The d-pad is also exposed as axes via
/// `input_x`/`input_y`; the discrete codes here give edge-triggered `key_pressed`
/// for menu navigation (one step per press).
fn button_of(code: i32) -> Option<Button> {
    Some(match code {
        0 => Button::A,
        1 => Button::B,
        2 => Button::Select,
        3 => Button::Start,
        4 => Button::L,
        5 => Button::R,
        6 => Button::Up,
        7 => Button::Down,
        8 => Button::Left,
        9 => Button::Right,
        _ => return None,
    })
}

/// `key_held(code)` — 1 while the button is held down, else 0. See [`button_of`] for codes.
pub fn key_held(args: &[Value]) -> Value {
    let held = button_of(num(args, 0) as i32)
        .map(|b| with_ctx(|ctx| ctx.input.is_pressed(b)))
        .unwrap_or(false);
    Value::Number(if held { 1.0 } else { 0.0 })
}

/// `key_pressed(code)` — 1 only on the frame the button goes down (edge), else 0.
pub fn key_pressed(args: &[Value]) -> Value {
    let pressed = button_of(num(args, 0) as i32)
        .map(|b| with_ctx(|ctx| ctx.input.is_just_pressed(b)))
        .unwrap_or(false);
    Value::Number(if pressed { 1.0 } else { 0.0 })
}

/// `keys_edge()` — bitmask of buttons that went down THIS frame (same bit = code as `key_pressed`).
/// One `with_ctx` for the whole pad — menus that polled A/B/Up/Down separately paid 4–6 ctx crossings
/// per frame on top of heavy paint; batching keeps input off the critical path during shop/UI updates.
pub fn keys_edge(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        let mut m = 0i32;
        if ctx.input.is_just_pressed(Button::A) {
            m |= 1 << 0;
        }
        if ctx.input.is_just_pressed(Button::B) {
            m |= 1 << 1;
        }
        if ctx.input.is_just_pressed(Button::Select) {
            m |= 1 << 2;
        }
        if ctx.input.is_just_pressed(Button::Start) {
            m |= 1 << 3;
        }
        if ctx.input.is_just_pressed(Button::L) {
            m |= 1 << 4;
        }
        if ctx.input.is_just_pressed(Button::R) {
            m |= 1 << 5;
        }
        if ctx.input.is_just_pressed(Button::Up) {
            m |= 1 << 6;
        }
        if ctx.input.is_just_pressed(Button::Down) {
            m |= 1 << 7;
        }
        if ctx.input.is_just_pressed(Button::Left) {
            m |= 1 << 8;
        }
        if ctx.input.is_just_pressed(Button::Right) {
            m |= 1 << 9;
        }
        Value::Number(m as f64)
    })
}

/// `keys_held()` — bitmask of buttons currently DOWN (same bit = code as `keys_edge`).
///
/// The held twin of [`keys_edge`], and it exists for the same reason, only more so: a versus
/// fighting game reads the whole pad — four directions and four attack buttons — every frame, for
/// every fighter, and it does it before anything else can run. Eight `key_held` calls is eight
/// boxed-`Value` dispatches and eight `with_ctx` crossings per fighter; this is one.
pub fn keys_held(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        let mut m = 0i32;
        if ctx.input.is_pressed(Button::A) {
            m |= 1 << 0;
        }
        if ctx.input.is_pressed(Button::B) {
            m |= 1 << 1;
        }
        if ctx.input.is_pressed(Button::Select) {
            m |= 1 << 2;
        }
        if ctx.input.is_pressed(Button::Start) {
            m |= 1 << 3;
        }
        if ctx.input.is_pressed(Button::L) {
            m |= 1 << 4;
        }
        if ctx.input.is_pressed(Button::R) {
            m |= 1 << 5;
        }
        if ctx.input.is_pressed(Button::Up) {
            m |= 1 << 6;
        }
        if ctx.input.is_pressed(Button::Down) {
            m |= 1 << 7;
        }
        if ctx.input.is_pressed(Button::Left) {
            m |= 1 << 8;
        }
        if ctx.input.is_pressed(Button::Right) {
            m |= 1 << 9;
        }
        Value::Number(m as f64)
    })
}

/// `key_released(code)` — 1 only on the frame the button goes up (edge), else 0. Used for e.g.
/// variable-height jumps (cut the jump when the button is released).
pub fn key_released(args: &[Value]) -> Value {
    let released = button_of(num(args, 0) as i32)
        .map(|b| with_ctx(|ctx| ctx.input.is_just_released(b)))
        .unwrap_or(false);
    Value::Number(if released { 1.0 } else { 0.0 })
}

/// `key_live(code)` — the button's state read STRAIGHT FROM THE HARDWARE right now (1 = held, 0 = up),
/// bypassing the per-frame input snapshot that `key_held`/`key_pressed` use. Those only refresh inside
/// `frame()`, so a press that happens while the game loop is blocked in a long operation (e.g. a heavy
/// menu re-layout that spans many vblanks) is invisible to them until the next `frame()` — and a tap
/// can be missed entirely. `key_live` reads REG_KEYINPUT (0x04000130; a bit LOW = pressed) directly, so
/// the press is seen the instant the loop next checks. Pair it with your own edge latch for a reliable
/// "pause / take control" toggle. Codes are the same as [`button_of`].
pub fn key_live(args: &[Value]) -> Value {
    // tish code -> REG_KEYINPUT bit (hardware order differs from the tish code order for the d-pad/L/R).
    let bit: u16 = match num(args, 0) as i32 {
        0 => 0, // A
        1 => 1, // B
        2 => 2, // Select
        3 => 3, // Start
        9 => 4, // Right
        8 => 5, // Left
        6 => 6, // Up
        7 => 7, // Down
        5 => 8, // R
        4 => 9, // L
        _ => return Value::Number(0.0),
    };
    let v = unsafe { core::ptr::read_volatile(0x0400_0130 as *const u16) };
    let pressed = (v >> bit) & 1 == 0; // KEYINPUT: 0 = pressed
    Value::Number(if pressed { 1.0 } else { 0.0 })
}

// ── serial (SIO) multiplayer ─────────────────────────────────────────────────
//
// The GBA's link port in MULTI-PLAYER mode: up to four units on one cable, each writing one 16-bit
// word per transfer and reading all four back. It is the only way two cartridges can play each
// other, and it is a thin enough surface to expose directly rather than wrap in a protocol — the
// protocol belongs in `packages/link.tish`, where a game can read it.
//
// The registers, from GBATEK:
//
//   RCNT        0x0400_0134   bits 15-14 must be 00 to hand the port to SIO at all
//   SIOCNT      0x0400_0128   bit 13 = 1, bit 12 = 0 selects multi-player mode
//                             bit 2  SI   0 = this unit is the MASTER (it starts transfers)
//                             bit 3  SD   1 = every attached unit is ready
//                             bit 4-5     this unit's player id, 0..3
//                             bit 6       error
//                             bit 7       start / busy — the master writes 1, hardware clears it
//   SIOMLT_SEND 0x0400_012A   the word THIS unit contributes to the next transfer
//   SIOMULTI0-3 0x0400_0120   what each unit contributed to the last one; 0xFFFF = absent
//
// WHAT THIS CANNOT BE TESTED WITH. A single emulator instance has nothing on the other end of the
// cable, so every headless test in this repo sees `sio_link_ready() == 0` and takes the offline
// path. That is a real limit and not a temporary one: the register writes below are checked
// against GBATEK and compile, and nothing here has moved a byte between two consoles.

const REG_RCNT: *mut u16 = 0x0400_0134 as *mut u16;
const REG_SIOCNT: *mut u16 = 0x0400_0128 as *mut u16;
const REG_SIOMLT_SEND: *mut u16 = 0x0400_012A as *mut u16;
const REG_SIOMULTI: *const u16 = 0x0400_0120 as *const u16;

const SIO_MULTI_MODE: u16 = 1 << 13; // bit 13 set, bit 12 clear
const SIO_SI: u16 = 1 << 2;
const SIO_SD: u16 = 1 << 3;
const SIO_ERROR: u16 = 1 << 6;
const SIO_START: u16 = 1 << 7;

/// `sio_link_open(baud)` — hand the link port to multi-player mode. `baud` 0..3 selects
/// 9600 / 38400 / 57600 / 115200; 3 is what a lockstep game wants, and the cable is short enough
/// that the fastest rate is the usual choice.
///
/// Returns 1 if the port accepted multi-player mode. It does NOT mean anything is plugged in —
/// ask `sio_link_ready` for that, and expect it to be 0 for a while after a cable goes in.
pub fn sio_link_open(args: &[Value]) -> Value {
    Value::Number(sio_link_open_typed(num(args, 0) as i32) as f64)
}

pub fn sio_link_open_typed(baud: i32) -> i32 {
    let baud = (baud.clamp(0, 3)) as u16;
    unsafe {
        // RCNT first: while it still selects GPIO or JOY-bus, writes to SIOCNT's mode bits do
        // nothing and the port looks dead for reasons that are invisible from SIOCNT alone.
        core::ptr::write_volatile(REG_RCNT, 0);
        core::ptr::write_volatile(REG_SIOCNT, SIO_MULTI_MODE | baud);
        let back = core::ptr::read_volatile(REG_SIOCNT);
        if back & SIO_MULTI_MODE != 0 {
            1
        } else {
            0
        }
    }
}

/// `sio_link_id()` — this unit's player number, 0..3, or -1 while the link is not ready.
///
/// The id is assigned by position on the cable, and it is also how a game decides who is who:
/// there is no negotiation, so player 0 is simply whoever is nearest the master end.
pub fn sio_link_id(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(sio_link_id_typed() as f64)
}

pub fn sio_link_id_typed() -> i32 {
    let c = unsafe { core::ptr::read_volatile(REG_SIOCNT) };
    if c & SIO_SD == 0 {
        return -1;
    }
    ((c >> 4) & 3) as i32
}

/// `sio_link_master()` — 1 if this unit drives transfers. Only the master may set the start bit;
/// a child writes its word and waits to be asked for it.
pub fn sio_link_master(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(sio_link_master_typed() as f64)
}

pub fn sio_link_master_typed() -> i32 {
    let c = unsafe { core::ptr::read_volatile(REG_SIOCNT) };
    if c & SIO_SI == 0 {
        1
    } else {
        0
    }
}

/// `sio_link_ready()` — 1 when SD says every attached unit is present and idle.
pub fn sio_link_ready(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(sio_link_ready_typed() as f64)
}

pub fn sio_link_ready_typed() -> i32 {
    let c = unsafe { core::ptr::read_volatile(REG_SIOCNT) };
    if c & SIO_SD != 0 {
        1
    } else {
        0
    }
}

/// `sio_link_busy()` — 1 while a transfer is in flight. The start bit is set by the master and
/// cleared by hardware on both ends, so a child polls the same bit to know a word has landed.
pub fn sio_link_busy(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(sio_link_busy_typed() as f64)
}

pub fn sio_link_busy_typed() -> i32 {
    let c = unsafe { core::ptr::read_volatile(REG_SIOCNT) };
    if c & SIO_START != 0 {
        1
    } else {
        0
    }
}

/// `sio_link_error()` — 1 if the last transfer reported an error. A game should treat this the
/// same as a disconnection: the words in SIOMULTI after an error are not trustworthy.
pub fn sio_link_error(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(sio_link_error_typed() as f64)
}

pub fn sio_link_error_typed() -> i32 {
    let c = unsafe { core::ptr::read_volatile(REG_SIOCNT) };
    if c & SIO_ERROR != 0 {
        1
    } else {
        0
    }
}

/// `sio_link_send(word)` — stage this unit's 16-bit word for the next transfer, and start that
/// transfer if this unit is the master.
///
/// Returns 1 if a transfer was started, 0 if the word was only staged (a child, or a transfer
/// already in flight). Both ends must call this every frame: a child that stops staging sends
/// whatever it staged last, which reads as a player holding a button down forever.
pub fn sio_link_send(args: &[Value]) -> Value {
    Value::Number(sio_link_send_typed(num(args, 0) as i32) as f64)
}

pub fn sio_link_send_typed(word: i32) -> i32 {
    unsafe {
        core::ptr::write_volatile(REG_SIOMLT_SEND, (word as u32 & 0xFFFF) as u16);
        let c = core::ptr::read_volatile(REG_SIOCNT);
        // Only the master starts, and only when the port is idle — setting the start bit during a
        // transfer aborts it and both ends see an error.
        //
        // Deliberately NOT gated on SD. The master's ready bit does not necessarily go high until
        // a transfer has been attempted, so requiring it here is a deadlock: the master waits for
        // a readiness that only its own transfer would establish, and the link never starts. What
        // "nobody is there" actually looks like is a transfer that comes back with the peer's slot
        // reading 0xFFFF, which the caller can see.
        if c & SIO_SI == 0 && c & SIO_START == 0 {
            core::ptr::write_volatile(REG_SIOCNT, c | SIO_START);
            return 1;
        }
        0
    }
}

/// `sio_link_recv(slot)` — the word unit `slot` contributed to the last completed transfer, or
/// -1 if no unit is in that slot. Absent units read 0xFFFF, which is why -1 rather than 0xFFFF is
/// returned: 0xFFFF is also a legal payload, and a game that cannot tell them apart will read a
/// missing player as one pressing every button.
pub fn sio_link_recv(args: &[Value]) -> Value {
    Value::Number(sio_link_recv_typed(num(args, 0) as i32) as f64)
}

pub fn sio_link_recv_typed(slot: i32) -> i32 {
    if !(0..4).contains(&slot) {
        return -1;
    }
    let w = unsafe { core::ptr::read_volatile(REG_SIOMULTI.add(slot as usize)) };
    if w == 0xFFFF {
        -1
    } else {
        w as i32
    }
}

/// `sio_link_close()` — return the port to general-purpose mode, so a game leaving a link match
/// does not leave the hardware waiting on a transfer that will never come.
pub fn sio_link_close(args: &[Value]) -> Value {
    let _ = args;
    unsafe {
        core::ptr::write_volatile(REG_SIOCNT, 0);
        core::ptr::write_volatile(REG_RCNT, 0x8000);
    }
    Value::Null
}

/// `sprite_set_flip(handle, hflip)` — mirror a sprite horizontally (facing left/right).
pub fn sprite_set_flip(args: &[Value]) -> Value {
    native_sprite_set_flip(num(args, 0) as i32, num(args, 1) != 0.0);
    Value::Null
}

// ── Typed exports (the "typed externs" perf path) ─────────────────────────────
// Native-argument twins of the boxed `fn(&[Value])` exports, for direct `tish_agb::name_typed(..)`
// call sites emitted by the compiler when a game `declare fun`s the matching typed signature.
pub fn sprite_new_typed(sheet_handle: i32) -> i32 {
    with_ctx(|ctx| {
        let sheet = tishlang_runtime_gba::gba::asset_sheet(sheet_handle)
            .expect("tish-agb: sprite_new called with an unknown asset handle");
        let object = Object::new(&sheet[0]);
        sprite_alloc(
            ctx,
            SpriteData {
                object: Some(object),
                x: 104,
                y: 72,
                hflip: false,
                visible: true,
                sheet: sheet_handle,
                frame: 0,
                hud: false,
                priority: -1,
                depth: 0,
                billboard: false,
            },
        ) as i32
    })
}
/// `sprite_new(sheet)` under its numbered name.
///
/// Declaring a second arity switches codegen from `<symbol>_typed` to `<symbol>_typed_<arity>` for
/// EVERY arity of that name (see `extern_sig_for_arity`), so the 1-arg form needs this spelling too
/// or the 1-arg call sites stop linking. `sprite_new_typed` is kept above so anything already calling
/// it directly is unaffected.
pub fn sprite_new_typed_1(sheet_handle: i32) -> i32 {
    sprite_new_typed(sheet_handle)
}

/// `sprite_new(sheet, frame)` — the 2-arity form, which 44 call sites across the corpus already use.
///
/// Only the 1-arity `sprite_new_typed` existed, so every one of those calls found no matching
/// declared arity and could never lower: the whole enclosing function stayed a boxed closure. The
/// boxed `sprite_new` has always accepted the optional frame (`args.len() > 1`), so the call sites
/// were correct and it was the typed surface that was incomplete — a gap the boxed path hid.
///
/// Clamps like the boxed path does: a frame past the end of the sheet is pinned to the last cell
/// rather than panicking, and a negative one to 0.
pub fn sprite_new_typed_2(sheet_handle: i32, frame: i32) -> i32 {
    with_ctx(|ctx| {
        let sheet = tishlang_runtime_gba::gba::asset_sheet(sheet_handle)
            .expect("tish-agb: sprite_new called with an unknown asset handle");
        let frame = (frame.max(0) as usize).min(sheet.len() - 1);
        let object = Object::new(&sheet[frame]);
        sprite_alloc(
            ctx,
            SpriteData {
                object: Some(object),
                x: 104,
                y: 72,
                hflip: false,
                visible: true,
                sheet: sheet_handle,
                frame: frame as i32,
                hud: false,
                priority: -1,
                depth: 0,
                billboard: false,
            },
        ) as i32
    })
}
pub fn sprite_set_frame_typed(handle: i32, frame_idx: i32) {
    native_sprite_set_frame(handle, frame_idx);
}
pub fn sprite_set_pos_typed(handle: i32, x: i32, y: i32) {
    native_sprite_set_pos(handle, x, y);
}
pub fn sprite_set_flip_typed(handle: i32, hflip: i32) {
    native_sprite_set_flip(handle, hflip != 0);
}
pub fn input_x_typed() -> i32 {
    with_ctx(|ctx| ctx.input.x_tri() as i32)
}
pub fn input_y_typed() -> i32 {
    with_ctx(|ctx| ctx.input.y_tri() as i32)
}
pub fn key_pressed_typed(code: i32) -> i32 {
    button_of(code)
        .map(|b| with_ctx(|ctx| ctx.input.is_just_pressed(b)))
        .unwrap_or(false) as i32
}
pub fn key_held_typed(code: i32) -> i32 {
    button_of(code)
        .map(|b| with_ctx(|ctx| ctx.input.is_pressed(b)))
        .unwrap_or(false) as i32
}

/// Native (non-`Value`) horizontal flip — called by the engine's walk-animation system
/// so a side-facing sheet can serve both left (unflipped) and right (flipped).
pub fn native_sprite_set_flip(handle: i32, hflip: bool) {
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(handle as usize) {
            s.hflip = hflip;
        }
    });
}

/// Native (non-`Value`) visibility toggle — called by the engine (e.g. to hide a
/// despawned entity's sprite). The `Value`-ABI [`sprite_set_visible`] wraps this.
pub fn native_sprite_set_visible(handle: i32, visible: bool) {
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(handle as usize) {
            s.visible = visible;
        }
    });
}

/// Native (non-`Value`) sprite free — drops the sprite's agb `Object` (releasing its VRAM sprite
/// allocation) and parks the slot on the free list for reuse. The engine calls this on `despawn`, so
/// a game that spawns and removes sprites over time (projectiles, respawning enemies) keeps VRAM and
/// the arena bounded instead of leaking one sprite per spawn. No-op for an out-of-range or
/// already-freed handle (so a double free is harmless).
///
/// Recycling keys off `sheet`, NOT off whether the slot still owns an `Object`. `object == None` is
/// ambiguous: it means "freed" for a parked slot but also "VRAM released while off-screen" for a live
/// one (`native_sprite_release`, which the engine calls on every attach and on every entity that
/// culls out of view). Keying on the Object leaked the arena index of every sprite despawned while
/// off screen — which is the normal case for a room's entities when the player walks away from them.
pub fn native_sprite_destroy(handle: i32) {
    if handle < 0 {
        return;
    }
    let h = handle as usize;
    with_ctx(|ctx| {
        if let Some(s) = ctx.sprites.get_mut(h) {
            if s.sheet != SHEET_FREED {
                // Dropping the Object (if it is still resident) frees the VRAM; a released
                // off-screen sprite has already given that back and only needs its slot parked.
                s.object = None;
                s.visible = false;
                s.sheet = SHEET_FREED;
                ctx.sprite_free.push(h);
            }
        }
    });
}

/// `sprite_destroy(handle)` — free a sprite created by `sprite_new`/`sprite_create` (see
/// [`native_sprite_destroy`]). Its handle must not be used again.
pub fn sprite_destroy(args: &[Value]) -> Value {
    native_sprite_destroy(num(args, 0) as i32);
    Value::Null
}

/// `hud_hearts(sheetHandle, count, x, y, gap)` — set up a row of `count` HUD heart sprites (screen
/// space) at (x,y), `gap` px apart, from a `sheet:` whose frames run empty(0)..full. Replaces any
/// existing hearts. Pair with `hud_hearts_update(hp, perHeart)` each frame for a live health readout.
pub fn hud_hearts(args: &[Value]) -> Value {
    let sheet = num(args, 0) as i32;
    let count = (num(args, 1) as i32).max(0);
    let (x, y, gap) = (
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
    );
    with_ctx(|ctx| {
        ctx.hud_hearts_last = (-999, -999); // force the next update to draw
                                            // free any previous hearts (drop their Objects → VRAM, park the slots)
        let old = core::mem::take(&mut ctx.hud_hearts);
        for h in old {
            if let Some(s) = ctx.sprites.get_mut(h as usize) {
                if s.object.take().is_some() {
                    s.visible = false;
                    ctx.sprite_free.push(h as usize);
                }
            }
        }
        let frames = match tishlang_runtime_gba::gba::asset_sheet(sheet) {
            Some(f) => f,
            None => return Value::Null,
        };
        let mut i = 0;
        while i < count {
            let object = Object::new(&frames[0]);
            let handle = sprite_alloc(
                ctx,
                SpriteData {
                    object: Some(object),
                    x: x + i * gap,
                    y,
                    hflip: false,
                    visible: true,
                    sheet,
                    frame: 0,
                    hud: true,
                    priority: -1,
                    depth: 0,
                    billboard: false,
                },
            );
            ctx.hud_hearts.push(handle as i32);
            i += 1;
        }
        Value::Null
    })
}

/// `hud_hearts_update(hp, perHeart)` — set each heart's frame from `hp`. Each heart covers `perHeart`
/// hp across frames 0..=perHeart (0 empty .. perHeart full): perHeart=2 with a 3-frame sheet gives
/// half hearts, perHeart=1 with a 2-frame sheet gives simple full/empty hearts.
pub fn hud_hearts_update(args: &[Value]) -> Value {
    let hp = num(args, 0) as i32;
    let per = (num(args, 1) as i32).max(1);
    with_ctx(|ctx| {
        // Skip the rebuild (per-heart Object::new) when the value hasn't changed — the common case.
        if ctx.hud_hearts_last == (hp, per) {
            return;
        }
        ctx.hud_hearts_last = (hp, per);
        let n = ctx.hud_hearts.len();
        let mut i = 0;
        while i < n {
            let h = ctx.hud_hearts[i] as usize;
            let val = (hp - (i as i32) * per).clamp(0, per);
            let sheet_handle = match ctx.sprites.get(h) {
                Some(s) if s.sheet >= 0 => s.sheet,
                _ => {
                    i += 1;
                    continue;
                }
            };
            if let Some(frames) = tishlang_runtime_gba::gba::asset_sheet(sheet_handle) {
                let idx = (val.max(0) as usize).min(frames.len().saturating_sub(1));
                let object = Object::new(&frames[idx]);
                if let Some(s) = ctx.sprites.get_mut(h) {
                    s.object = Some(object);
                }
            }
            i += 1;
        }
    });
    Value::Null
}

/// 0xRRGGBB → Rgb15; a negative value falls back to `default`.
fn rgb15_of(color: i32, default: Rgb15) -> Rgb15 {
    if color < 0 {
        default
    } else {
        let v = color as u32;
        Rgb::new(
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
        )
        .to_rgb15()
    }
}

/// Palette index used for drop shadows — keeps 1..14 free for `ChangeColour` / `text_color(n)`.
const TEXT_SHADOW_PAL: u8 = 15;
/// Built-in visual for `text_tag_set(0)`: tagged letter groups are drawn 2px higher (a lift).
const TEXT_TAG_LIFT: Tag = Tag::new(0);

fn align_to_u8(a: AlignmentKind) -> u8 {
    match a {
        AlignmentKind::Left => 0,
        AlignmentKind::Right => 1,
        AlignmentKind::Centre => 2,
        AlignmentKind::Justify => 3,
        AlignmentKind::None => 4,
    }
}

fn parse_align(s: &str) -> AlignmentKind {
    match s {
        "right" | "end" => AlignmentKind::Right,
        "center" | "centre" => AlignmentKind::Centre,
        "justify" => AlignmentKind::Justify,
        "none" => AlignmentKind::None,
        // "left" | "start" | anything else
        _ => AlignmentKind::Left,
    }
}

fn align_arg(args: &[Value], idx: usize) -> AlignmentKind {
    match args.get(idx) {
        Some(v) if !matches!(v, Value::Null) => parse_align(&v.to_display_string()),
        _ => AlignmentKind::Left,
    }
}

fn layout_settings(maxw: i32, align: AlignmentKind) -> LayoutSettings {
    LayoutSettings::new()
        .with_max_line_length(maxw)
        .with_alignment(align)
}

/// A 16-colour text palette. Index 0 is unused (OBJ transparency). Indices 1.. fill from `colors`
/// (and the rest copy colours[0] so anti-aliased edges stay tinted). Drop shadow — when
/// `shadow >= 0` — lives at [`TEXT_SHADOW_PAL`] so mid-string `text_color(2..)` stays free.
fn text_palette(colors: &[i32], shadow: i32) -> Palette16 {
    let primary = colors.first().copied().unwrap_or(-1);
    let mut arr = [rgb15_of(primary, Rgb15::WHITE); 16];
    let mut i = 0;
    while i < colors.len() && i < 14 {
        arr[i + 1] = rgb15_of(colors[i], Rgb15::WHITE);
        i += 1;
    }
    if shadow >= 0 {
        arr[TEXT_SHADOW_PAL as usize] = rgb15_of(shadow, Rgb15::WHITE);
    }
    Palette16::new(arr)
}

/// A `&'static Palette16` for a (colours[], shadow) pair, leaked once and cached.
fn cached_palette(ctx: &mut GbaCtx, colors: &[i32], shadow: i32) -> &'static Palette16 {
    if let Some((_, _, p)) = ctx
        .text_palettes
        .iter()
        .find(|(c, s, _)| c.as_slice() == colors && *s == shadow)
    {
        return p;
    }
    let leaked: &'static Palette16 =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(text_palette(colors, shadow)));
    ctx.text_palettes.push((colors.to_vec(), shadow, leaked));
    leaked
}

/// The pixel size of an inline emoji sprite cell — matches `tish_gba_scenepack::emojipack::CELL` (the
/// SerenityOS art is centred in a 16x16 S16x16 sprite) and the emoji advance the font baker reserves.
const EMOJI_PX: i32 = 16;

/// Smallest object size that can hold a letter-group of `w`×`h` px. Prefer square/compact sizes —
/// always using `S32x64` doubled title-screen VRAM (the topdown RPG port's boot menu `sprite_clear`s + redraws
/// every frame) and OOMed with AllocError.
fn text_sprite_size_for(w: i32, h: i32) -> Size {
    let w = w.max(1) as usize;
    let h = h.max(1) as usize;
    if w <= 8 && h <= 8 {
        Size::S8x8
    } else if w <= 16 && h <= 8 {
        Size::S16x8
    } else if w <= 8 && h <= 16 {
        Size::S8x16
    } else if w <= 16 && h <= 16 {
        Size::S16x16
    } else if w <= 32 && h <= 16 {
        Size::S32x16
    } else if w <= 16 && h <= 32 {
        Size::S16x32
    } else if w <= 32 && h <= 32 {
        Size::S32x32
    } else if w <= 32 && h <= 64 {
        Size::S32x64
    } else if w <= 64 && h <= 32 {
        Size::S64x32
    } else {
        Size::S64x64
    }
}

/// How many letter groups a string can possibly produce.
///
/// agb's `Layout` panics when asked for a group past the last one: having consumed the final group
/// it re-slices `text[idx..]` against a slice it has already emptied and reports
/// `start byte index N is out of bounds for string of length 0` — from an agb source file, naming
/// no caller. Any string whose whole content forms a single group triggers it, which in the topdown RPG port was
/// every single-digit counter and then, once the tester's inventory was full, "255".
///
/// A letter group always holds at least one character, so taking at most this many can never drop a
/// real group and never asks for the one that panics.
fn max_groups(text: &str) -> usize {
    text.chars().count()
}

/// Rasterise one letter group into an `Object`, clamping out-of-range pixels instead of panicking
/// (`y too big for sprite size` from agb's ObjectTextRenderer::show).
///
/// The palette index is clamped for the same reason the coordinates are. A `DynamicSprite16` holds
/// 4bpp pixels, so `set_pixel` asserts `paletted_pixel < 16`, and the index here is whatever agb's
/// `Layout` carries out of the text — which follows `ChangeColour` markers embedded in the string.
/// Any text that ends up holding a byte in that control range (a stray high byte in ROM-derived
/// text, a mis-decoded glyph) therefore takes down the whole ROM from inside a routine call, with a
/// panic naming an agb file and nothing about which string did it. A colour that falls out of range
/// should cost one wrong-coloured letter, not the game.
fn letter_group_object(
    group: &LetterGroup,
    pal: &PaletteVramSingle,
    offset: Vector2D<i32>,
) -> Object {
    let b = group.bounds();
    let size = text_sprite_size_for(b.x, b.y);
    let (spr_w, spr_h) = size.to_width_height();
    // ⚠️ `new_in(.., ExternalAllocator)`, NOT `new`. `DynamicSprite16::new` stages the pixels in
    // **IWRAM** (agb's `InternalAllocator` is `__IWRAM_ALLOC`) — 32 KB shared with the stack — and
    // this buffer is copied straight into sprite VRAM by `to_vram` and dropped. Staging it there
    // bought nothing and cost the game: walking into an overworld cave died inside
    // `BlockAllocatorInner::alloc` on the FIRST `hudText` after the load, with the allocator
    // walking a free list from address 0 and the writes marching 0x4, 0x8, 0xC… up from null.
    //
    // The reason it took so long to find is worth keeping: `heap_free()` measures the EWRAM heap
    // and reported a healthy 83 KB at the moment of death, which reads as "not memory" and sent
    // four separate investigations after VRAM, palette banks and use-after-free. Two arenas, one
    // probe. `iwram_free()` now exists so the next reader can see both.
    let mut sprite = DynamicSprite16::new_in(size, ExternalAllocator);
    for (pixel, palette_index) in group.pixels() {
        if pixel.x < 0 || pixel.y < 0 {
            continue;
        }
        let px = pixel.x as usize;
        let py = pixel.y as usize;
        if px >= spr_w || py >= spr_h {
            continue;
        }
        sprite.set_pixel(px, py, palette_index.min(15));
    }
    let mut object = Object::new(sprite.to_vram(pal.clone()));
    object.set_pos(offset + group.position());
    object
}

/// Lay `text` out in a font (handle, or -1 for the built-in dialogue font) + a `'static` palette into
/// positioned sprite objects. Per-group sprite size is chosen from that group's bounds (not a single
/// square size for the whole font) so tall glyphs don't panic and the title screen doesn't pay
/// `S32x64` for every "A".
///
/// Emoji are split out of the string and overlaid as colour sprites. Non-emoji runs keep agb's normal
/// letter grouping — do NOT force 1-char groups for the whole line (that blows sprite VRAM on any
/// text-heavy screen that mixes latin + one emoji). Alignment / wrap apply on emoji-free text; with
/// emoji we fall back to left-aligned runs (emoji break mid-string layout).
fn build_text_objs(
    font_handle: i32,
    pal: &'static Palette16,
    x: i32,
    y: i32,
    text: &str,
    shadow: bool,
    align: AlignmentKind,
    maxw: i32,
) -> (Vec<Object>, Vec<Object>) {
    // Nothing to lay out, and agb's grouper does not treat that as a no-op: handed an empty
    // string it can walk its own index past the end and panic with `start byte index N is out of
    // bounds for string of length 0`, from an agb source file with nothing naming the caller.
    // Blanking a line with "" is the ordinary way to clear one, so this is a hot path.
    if !layoutable(text) {
        return (Vec::new(), Vec::new());
    }
    let font: &'static Font = if font_handle < 0 {
        &FONT
    } else {
        match tishlang_runtime_gba::gba::asset_font(font_handle) {
            Some(f) => f,
            None => return (Vec::new(), Vec::new()),
        }
    };
    // Layout group width: keep packs modest so most groups fit in S16x16 / S32x32.
    let mut group_w = match font.line_height() {
        h if h <= 14 => 16,
        h if h <= 30 => 32,
        _ => 64,
    };
    // A single glyph wider than the group cap can never be laid out — agb skips it ("too wide")
    // and the line renders with a hole. Fallback CJK glyphs in a small latin font are wider than
    // that font's 16px cap, so widen to the next sprite tier when the text actually needs it;
    // pure-latin strings keep the modest default. (+1 accounts for the drop-shadow column.)
    if let Some(m) = font_metrics::font_metrics(font_handle) {
        let widest = text
            .chars()
            // Emoji render as their own sprites, never through Layout — don't let their baked
            // placeholder advance widen the letter groups.
            .filter(|&c| tishlang_runtime_gba::gba::emoji_sprite(c as u32).is_none())
            .map(|c| m.advance(c) + shadow as i32)
            .max()
            .unwrap_or(0);
        while group_w < widest && group_w < 64 {
            group_w *= 2;
        }
    }
    let pal_vram = PaletteVramSingle::new(pal);
    let has_emoji = text
        .chars()
        .any(|c| tishlang_runtime_gba::gba::emoji_sprite(c as u32).is_some());
    // Default to the screen width whenever the caller omitted maxw — for EVERY alignment, not just
    // the ones that need a box to align inside.
    //
    // A max line length of 0 is not "do not wrap": agb takes it literally and wraps at zero pixels,
    // so every glyph starts a new line and the group positions run off the bottom. That surfaces as
    // `index out of bounds: the len is 7 but the index is 259` inside agb's own font code — a glyph
    // asked for row 259 — and, on other strings, as the grouper walking past the end of its slice.
    // `hud_text` passes maxw 0 and left alignment, so every HUD line in the game was laid out this
    // way; which strings actually detonated depended on their length, which is why it looked like a
    // different bug each time the inventory changed.
    let mut line_w = maxw;
    if line_w <= 0 {
        line_w = 240;
    }
    let mut settings = LayoutSettings::new()
        .with_max_line_length(line_w)
        .with_max_group_width(group_w)
        .with_alignment(if has_emoji {
            AlignmentKind::Left
        } else {
            align
        });
    if shadow {
        settings = settings.with_drop_shadow(TEXT_SHADOW_PAL);
    }
    let emoji_dy = (font.line_height() - EMOJI_PX) / 2;
    let mut objs = Vec::new();
    let mut emoji = Vec::new();

    if !has_emoji {
        let base = Vector2D::new(x, y);
        // agb emits no letter group for a lone character, so a one-character string would draw
        // nothing at all. Give the grouper a second character to terminate on — a copy of the same
        // one, capped at one character per group — and keep only the first group. A trailing SPACE
        // also works and then panics inside agb's grouper (see `shape_text`); a real character does
        // not, and dropping the extra group leaves exactly the one glyph the caller asked for.
        if text.chars().nth(1).is_none() {
            let mut doubled = alloc::string::String::from(text);
            doubled.push_str(text);
            let single = settings.with_max_chars_per_group(1);
            if let Some(g) = Layout::new(doubled.as_str(), font, &single).next() {
                let bump = if g.has_tag(TEXT_TAG_LIFT) { 2 } else { 0 };
                objs.push(letter_group_object(
                    &g,
                    &pal_vram,
                    base + Vector2D::new(0, bump),
                ));
            }
            return (objs, emoji);
        }
        for g in Layout::new(text, font, &settings).take(max_groups(text)) {
            // Tag 0 (`text_tag_set(0)`) lifts the group 2px — a visible built-in so tags are demoable.
            let bump = if g.has_tag(TEXT_TAG_LIFT) { 2 } else { 0 };
            objs.push(letter_group_object(
                &g,
                &pal_vram,
                base + Vector2D::new(0, bump),
            ));
        }
        return (objs, emoji);
    }

    // Emoji path: left-aligned runs with a cursor (alignment/wrap across emoji is not supported).
    let mut cursor_x: i32 = 0;
    let mut run = alloc::string::String::new();
    let flush_run =
        |run: &mut alloc::string::String, cursor_x: &mut i32, objs: &mut Vec<Object>| {
            if run.is_empty() {
                return;
            }
            let base = Vector2D::new(x + *cursor_x, y);
            let mut run_w = 0;
            for g in Layout::new(run.as_str(), font, &settings).take(max_groups(run.as_str())) {
                let right = g.position().x + g.bounds().x;
                if right > run_w {
                    run_w = right;
                }
                let bump = if g.has_tag(TEXT_TAG_LIFT) { 2 } else { 0 };
                objs.push(letter_group_object(
                    &g,
                    &pal_vram,
                    base + Vector2D::new(0, bump),
                ));
            }
            *cursor_x += run_w;
            run.clear();
        };
    for c in text.chars() {
        let cp = c as u32;
        if let Some(sprite) = tishlang_runtime_gba::gba::emoji_sprite(cp) {
            flush_run(&mut run, &mut cursor_x, &mut objs);
            let mut obj = Object::new(sprite);
            obj.set_pos(Vector2D::new(x + cursor_x, y + emoji_dy));
            emoji.push(obj);
            cursor_x += EMOJI_PX;
        } else {
            run.push(c);
        }
    }
    flush_run(&mut run, &mut cursor_x, &mut objs);
    (objs, emoji)
}

/// Vertical gradient text: keep the 1bpp glyph mask from the font, but assign palette index from
/// the pixel's Y within the letter group — `colors[0]` at the top through `colors[n-1]` at the
/// bottom. Drop-shadow pixels stay on [`TEXT_SHADOW_PAL`]. This is the axis `text_color` cannot
/// express (that only switches a solid index mid-string, i.e. horizontal washes across letters).
fn build_text_objs_vgrad(
    font_handle: i32,
    pal: &'static Palette16,
    x: i32,
    y: i32,
    text: &str,
    shadow: bool,
    n_colors: usize,
    align: AlignmentKind,
    maxw: i32,
) -> (Vec<Object>, Vec<Object>) {
    // Nothing to lay out, and agb's grouper does not treat that as a no-op: handed an empty
    // string it can walk its own index past the end and panic with `start byte index N is out of
    // bounds for string of length 0`, from an agb source file with nothing naming the caller.
    // Blanking a line with "" is the ordinary way to clear one, so this is a hot path.
    if !layoutable(text) {
        return (Vec::new(), Vec::new());
    }
    let font: &'static Font = if font_handle < 0 {
        &FONT
    } else {
        match tishlang_runtime_gba::gba::asset_font(font_handle) {
            Some(f) => f,
            None => return (Vec::new(), Vec::new()),
        }
    };
    let group_w = match font.line_height() {
        h if h <= 14 => 16,
        h if h <= 30 => 32,
        _ => 64,
    };
    let pal_vram = PaletteVramSingle::new(pal);
    let n = (n_colors.clamp(1, 14)) as i32;
    let mut line_w = maxw;
    if line_w <= 0 && !matches!(align, AlignmentKind::Left | AlignmentKind::None) {
        line_w = 240;
    }
    let mut settings = LayoutSettings::new()
        .with_max_line_length(line_w)
        .with_max_group_width(group_w)
        .with_alignment(align);
    if shadow {
        settings = settings.with_drop_shadow(TEXT_SHADOW_PAL);
    }
    let base = Vector2D::new(x, y);
    let mut objs = Vec::new();
    for g in Layout::new(text, font, &settings).take(max_groups(text)) {
        let bump = if g.has_tag(TEXT_TAG_LIFT) { 2 } else { 0 };
        let gb = g.bounds();
        let h = gb.y.max(1);
        let size = text_sprite_size_for(gb.x, gb.y);
        let (spr_w, spr_h) = size.to_width_height();
        let mut sprite = DynamicSprite16::new_in(size, ExternalAllocator);
        for (pixel, palette_index) in g.pixels() {
            if pixel.x < 0 || pixel.y < 0 {
                continue;
            }
            let px = pixel.x as usize;
            let py = pixel.y as usize;
            if px >= spr_w || py >= spr_h {
                continue;
            }
            let idx = if palette_index == TEXT_SHADOW_PAL {
                TEXT_SHADOW_PAL
            } else if n <= 1 || h <= 1 {
                1u8
            } else {
                let gy = pixel.y.clamp(0, h - 1);
                (1 + gy * (n - 1) / (h - 1)) as u8
            };
            // Same 4bpp ceiling as the plain rasteriser: a gradient built from more than 15 colours
            // would index past the palette and assert inside agb.
            sprite.set_pixel(px, py, idx.min(15));
        }
        let mut object = Object::new(sprite.to_vram(pal_vram.clone()));
        object.set_pos(base + g.position() + Vector2D::new(0, bump));
        objs.push(object);
    }
    (objs, Vec::new())
}

/// Shared slot writer: (re)render slot `slot` only when its text/pos/font/style change.
fn set_text_slot(
    font_handle: i32,
    slot: usize,
    x: i32,
    y: i32,
    colors: Vec<i32>,
    shadow: i32,
    align: AlignmentKind,
    maxw: i32,
    vgrad: bool,
    text: &str,
) {
    let align_u = align_to_u8(align);
    with_ctx(|ctx| {
        while ctx.hud_text.len() <= slot {
            ctx.hud_text.push(HudTextSlot {
                objs: Vec::new(),
                emoji_objs: Vec::new(),
                cache: alloc::string::String::new(),
                x: -1,
                y: -1,
                font: -2,
                colors: Vec::new(),
                shadow: -2,
                align: 255,
                maxw: -1,
                vgrad: false,
                visible: true,
            });
        }
        let s = &ctx.hud_text[slot];
        if s.cache == text
            && s.x == x
            && s.y == y
            && s.font == font_handle
            && s.colors == colors
            && s.shadow == shadow
            && s.align == align_u
            && s.maxw == maxw
            && s.vgrad == vgrad
        {
            // Same glyphs already in Sprite VRAM — just ensure the line is shown.
            if !s.visible {
                ctx.hud_text[slot].visible = true;
            }
            return;
        }
        // Empty text has no glyphs, so nothing below has anything to do — and agb's `Layout` does
        // not agree. Run it on "" and it can panic from inside its own grouper with
        // `start byte index N is out of bounds for string of length 0`, naming an agb source file
        // and no caller. Clearing a HUD line with `hudText(slot, x, y, "")` is the ordinary way to
        // blank one, so this is a hot path, not a corner.
        if text.is_empty() {
            let s = &mut ctx.hud_text[slot];
            s.objs.clear();
            s.emoji_objs.clear();
            s.cache.clear();
            s.visible = false;
            s.x = x;
            s.y = y;
            s.font = font_handle;
            s.colors = colors;
            s.shadow = shadow;
            s.align = align_u;
            s.maxw = maxw;
            s.vgrad = vgrad;
            return;
        }
        // ⚠️ DROP THE OLD GLYPH OBJECTS BEFORE BUILDING THE NEW ONES. Sprite VRAM is a 32KB bank
        // that PANICS when an allocation cannot fit, and building first meant every text CHANGE
        // held the line's old and new glyph sprites simultaneously while fragmenting the bank a
        // little more each time. The HUD's palate/grains counters change on every landed hit, so
        // on builds whose sheets packed VRAM tight the game died mid-combat with
        // "memory allocation of N bytes failed" — deterministically per binary, "randomly" across
        // builds. Both happen between frames, so nothing visible flickers.
        {
            let s = &mut ctx.hud_text[slot];
            s.objs.clear();
            s.emoji_objs.clear();
        }
        let pal = cached_palette(ctx, &colors, shadow);
        let (objs, emoji_objs) = if vgrad {
            build_text_objs_vgrad(
                font_handle,
                pal,
                x,
                y,
                text,
                shadow >= 0,
                colors.len(),
                align,
                maxw,
            )
        } else {
            build_text_objs(font_handle, pal, x, y, text, shadow >= 0, align, maxw)
        };
        let s = &mut ctx.hud_text[slot];
        s.cache = text.into();
        s.x = x;
        s.y = y;
        s.font = font_handle;
        s.colors = colors;
        s.shadow = shadow;
        s.align = align_u;
        s.maxw = maxw;
        s.vgrad = vgrad;
        s.objs = objs;
        s.emoji_objs = emoji_objs;
        // Empty string drops Objects (caller clearing a slot). Non-empty draws are visible.
        s.visible = !text.is_empty();
    });
}

/// A `&'static Palette16` for a bar's (fg, bg) pair — index 1 = bg, 2 = fg, 3 = border (bg darkened).
/// Leaked once and cached (a game uses only a couple of bar styles); index 0 stays transparent.
fn cached_bar_palette(ctx: &mut GbaCtx, fg: i32, bg: i32) -> &'static Palette16 {
    if let Some((_, _, p)) = ctx
        .bar_palettes
        .iter()
        .find(|(f, b, _)| *f == fg && *b == bg)
    {
        return p;
    }
    let border = (((bg >> 17) & 0x7f) << 16) | (((bg >> 9) & 0x7f) << 8) | ((bg >> 1) & 0x7f); // bg/2
    let mut arr = [Rgb15::BLACK; 16];
    arr[1] = rgb15_of(bg, Rgb15::BLACK);
    arr[2] = rgb15_of(fg, Rgb15::WHITE);
    arr[3] = rgb15_of(border, Rgb15::BLACK);
    let leaked: &'static Palette16 =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(Palette16::new(arr)));
    ctx.bar_palettes.push((fg, bg, leaked));
    leaked
}

/// `hud_bar(slot, x, y, w, h, frac, fgColor, bgColor)` — a graphical HUD bar (health/progress) at
/// screen pixel `(x, y)`, `w`×`h` px, filled `frac` (0..1) in `fgColor` over `bgColor` with a 1px
/// darkened border. A boxless, better-looking replacement for a text bar: one dynamically-drawn sprite,
/// cached per slot so it only re-renders when the filled width or colours change (a value that ticks
/// down a few times a second costs nothing on the frames between). `w` ≤ 64, `h` ≤ 16 (one OBJ).
pub fn hud_bar(args: &[Value]) -> Value {
    let slot = (num(args, 0) as i32).max(0) as usize;
    let x = num(args, 1) as i32;
    let y = num(args, 2) as i32;
    let w_raw = num(args, 3) as i32;
    // w <= 0 clears the bar (e.g. hide the boss bar once it dies) — drops the Object, freeing its VRAM.
    if w_raw <= 0 {
        with_ctx(|ctx| {
            if let Some(s) = ctx.hud_bars.get_mut(slot) {
                s.obj = None;
                s.fill = -1;
                s.w = -1;
            }
        });
        return Value::Null;
    }
    let w = w_raw.clamp(2, 64);
    let h = (num(args, 4) as i32).clamp(2, 16);
    let frac = num(args, 5).clamp(0.0, 1.0);
    let fg = num(args, 6) as i32;
    let bg = num(args, 7) as i32;
    let fill = ((w as f64 * frac) + 0.5) as i32;
    let fill = fill.clamp(0, w);
    with_ctx(|ctx| {
        while ctx.hud_bars.len() <= slot {
            ctx.hud_bars.push(HudBarSlot {
                obj: None,
                x: -1,
                y: -1,
                w: -1,
                h: -1,
                fill: -1,
                fg: 0,
                bg: 0,
            });
        }
        {
            let s = &ctx.hud_bars[slot];
            if s.x == x
                && s.y == y
                && s.w == w
                && s.h == h
                && s.fill == fill
                && s.fg == fg
                && s.bg == bg
            {
                return; // unchanged — keep the cached sprite
            }
        }
        let pal = cached_bar_palette(ctx, fg, bg);
        // Draw into a 64x32 sprite (the widest short OBJ size) with only the top-left w×h used; a 1px
        // border, fg for the filled span, bg for the rest, and 0 = transparent everywhere else.
        // Palette indices: 1 = bg, 2 = fg, 3 = border.
        let mut spr = DynamicSprite16::new_in(Size::S64x32, ExternalAllocator);
        spr.clear(0);
        for py in 0..h {
            for px in 0..w {
                let idx = if px == 0 || px == w - 1 || py == 0 || py == h - 1 {
                    3u8
                } else if px < fill {
                    2u8
                } else {
                    1u8
                };
                spr.set_pixel(px as usize, py as usize, idx);
            }
        }
        let vram = spr.to_vram(PaletteVramSingle::new(pal));
        let s = &mut ctx.hud_bars[slot];
        s.obj = Some(Object::new(vram));
        s.x = x;
        s.y = y;
        s.w = w;
        s.h = h;
        s.fill = fill;
        s.fg = fg;
        s.bg = bg;
    });
    Value::Null
}

/// `hud_text(slot, x, y, text)` — draw `text` (stringified) at screen pixel `(x, y)` as HUD sprites:
/// front (priority 0) and camera-independent, using the built-in font in white. `slot` is an
/// independent line (0, 1, 2, …) so a game can show, say, a health readout and a menu at once; call
/// with `""` to clear a slot. Cached per slot — rebuilds only when the slot's string/position change.
pub fn hud_text(args: &[Value]) -> Value {
    let slot = (num(args, 0) as i32).max(0) as usize;
    let x = num(args, 1) as i32;
    let y = num(args, 2) as i32;
    let text = shape_text(
        args.get(3)
            .map(|v| v.to_display_string())
            .unwrap_or_default(),
    );
    set_text_slot(
        -1,
        slot,
        x,
        y,
        alloc::vec![-1],
        -1,
        AlignmentKind::Left,
        0,
        false,
        &text,
    );
    Value::Null
}

/// `hud_text_shadow(slot, x, y, text, [shadow])` — HUD text with a drop shadow. `shadow` is
/// `0xRRGGBB`, default black.
///
/// ⚠️ This exists because plain `hud_text` is unreadable over artwork, and that is not a cosmetic
/// complaint: an SRPG battle prompt (`"< Attack >  A: choose"` — the whole action menu) is drawn
/// every frame at the bottom of the screen, and over a sunlit board it vanished so completely that
/// the menu was reported as MISSING and nearly rebuilt from scratch. White glyphs on bright art have
/// no contrast at 7px. One dark outline pixel fixes it, and the renderer already supported the
/// shadow — `hud_text` was just passing -1.
pub fn hud_text_shadow(args: &[Value]) -> Value {
    let slot = (num(args, 0) as i32).max(0) as usize;
    let x = num(args, 1) as i32;
    let y = num(args, 2) as i32;
    let text = shape_text(
        args.get(3)
            .map(|v| v.to_display_string())
            .unwrap_or_default(),
    );
    let shadow = if args.len() > 4 {
        num(args, 4) as i32
    } else {
        0x000000
    };
    set_text_slot(
        -1,
        slot,
        x,
        y,
        alloc::vec![-1],
        shadow,
        AlignmentKind::Left,
        0,
        false,
        &text,
    );
    Value::Null
}

/// `text_color(paletteIndex)` — insert mid-string colour switch (agb `ChangeColour`). Index `1` is the
/// first entry of `colors` / `color` on `text_draw`; `2` is the second, etc. Returns a 1-char string
/// to concatenate into the text.
///
///   text_draw(font, 0, x, y, "Hi " + text_color(2) + "pink", { colors: [0xEEE, 0xFF5C8A] })
pub fn text_color(args: &[Value]) -> Value {
    let idx = (num(args, 0) as u32).min(15);
    let mut s = String::new();
    s.push(ChangeColour::new(idx).to_char());
    Value::string(s)
}

/// `text_tag_set(tag)` / `text_tag_unset(tag)` — agb `Tag` markers (0..15) for custom effects.
/// Tag `0` is built-in: letter groups between set/unset are drawn 2px higher (`lift`). Other tags are
/// available for game logic via the layout (no other built-in visuals yet).
pub fn text_tag_set(args: &[Value]) -> Value {
    let tag = (num(args, 0) as u32).min(15);
    let mut s = String::new();
    s.push(Tag::new(tag).set());
    Value::string(s)
}

pub fn text_tag_unset(args: &[Value]) -> Value {
    let tag = (num(args, 0) as u32).min(15);
    let mut s = String::new();
    s.push(Tag::new(tag).unset());
    Value::string(s)
}

fn colors_from_opts(opts: &Value, fallback_color: i32) -> Vec<i32> {
    let mut colors = Vec::new();
    if let Value::Array(arr) = get_prop(opts, "colors") {
        for v in arr.borrow().iter() {
            if let Value::Number(n) = v {
                colors.push(*n as i32);
            }
        }
    }
    if colors.is_empty() {
        let c = match get_prop(opts, "color") {
            Value::Number(n) => n as i32,
            _ => fallback_color,
        };
        colors.push(c);
    }
    colors
}

/// `text_draw(fontHandle, slot, x, y, text, color?, shadow?)` — sprite text in an imported font.
/// Optional 6th arg may be a number (legacy colour) **or** an opts object:
///
///   { color?, colors?: number[], shadow?, align?: "left"|"right"|"center"|"justify", maxw?,
///     vgrad? }
///
/// `colors` fills palette indices 1.. for `text_color(n)`. `align` + `maxw` use agb layout alignment
/// and word wrap. `vgrad: 1` maps `colors[]` top→bottom *inside* each glyph (vertical wash); omit it
/// (default) for solid-per-glyph colour / horizontal washes via `text_color`. `""` clears the slot
/// (drops Objects / frees Sprite VRAM).
///
/// Cached per slot: identical text+style is a no-op (already in VRAM). Use [`text_visible`] to hide
/// without freeing — required for instant pause/menu toggles.
pub fn text_draw(args: &[Value]) -> Value {
    let font_handle = num(args, 0) as i32;
    let slot = (num(args, 1) as i32).max(0) as usize;
    let x = num(args, 2) as i32;
    let y = num(args, 3) as i32;
    let text = args
        .get(4)
        .map(|v| v.to_display_string())
        .unwrap_or_default();
    let mut colors = alloc::vec![-1];
    let mut shadow = -1i32;
    let mut align = AlignmentKind::Left;
    let mut maxw = 0i32;
    let mut vgrad = false;
    match args.get(5) {
        Some(Value::Object(_)) | Some(Value::Struct(_)) => {
            let opts = args.get(5).unwrap();
            colors = colors_from_opts(opts, -1);
            if let Value::Number(n) = get_prop(opts, "shadow") {
                shadow = n as i32;
            }
            let align_v = get_prop(opts, "align");
            if !matches!(align_v, Value::Null) {
                align = parse_align(&align_v.to_display_string());
            }
            if let Value::Number(n) = get_prop(opts, "maxw") {
                maxw = n as i32;
            }
            // Truthy number → vertical gradient. Bools stringify; accept number only for simplicity.
            if let Value::Number(n) = get_prop(opts, "vgrad") {
                vgrad = n != 0.0;
            }
        }
        Some(Value::Number(n)) => {
            colors = alloc::vec![*n as i32];
            if args.len() > 6 {
                shadow = num(args, 6) as i32;
            }
        }
        _ => {}
    }
    set_text_slot(
        font_handle,
        slot,
        x,
        y,
        colors,
        shadow,
        align,
        maxw,
        vgrad,
        &text,
    );
    Value::Null
}

/// `text_visible(slot, on)` — show/hide a `text_draw` / `hud_text` slot **without** rebuilding or
/// freeing its Sprite VRAM `Object`s. agb only submits an `Object` to OAM when you call `show` on the
/// frame; skipping that leaves the DynamicSprite tiles allocated. Use this for pause/menu dismiss so
/// the next open is a boolean flip, not another `DynamicSprite16::to_vram` pass.
pub fn text_visible(args: &[Value]) -> Value {
    let slot = (num(args, 0) as i32).max(0) as usize;
    let on = args.len() < 2 || num(args, 1) != 0.0;
    with_ctx(|ctx| {
        if let Some(s) = ctx.hud_text.get_mut(slot) {
            s.visible = on && !s.cache.is_empty();
        }
    });
    Value::Null
}

// ── UI text canvas ────────────────────────────────────────────────────────
// A background-tile text layer for MENUS. `text_draw`/`hud_text` render each glyph group as a 32×32
// SPRITE (16 tiles of sprite VRAM), which a text-heavy menu exhausts. These draw glyphs into a
// dedicated background instead — no OAM, no sprite-VRAM pressure — so a full screen of menu text is
// cheap. A reusable UI layout engine (packages/ui.tish) composes these + measured widths.

/// The single background-palette slot every UI-canvas tile uses. Rather than one palette-bank per
/// colour (which forces a tile to a single colour — the classic bleed when two colours meet in one
/// 8×8 tile), ALL ui text/rects share this one 16-colour palette and each distinct colour is a
/// distinct INDEX (1..15) within it. A tile then holds several colours at once — glyph pixels carry
/// their colour's index — so a yellow name and white body sitting in the same tile never bleed.
const UI_PAL_SLOT: u8 = 15;
/// Cap on the `text_width` memo (see `GbaCtx::tw_cache`); cleared wholesale when exceeded so dynamic text
/// (changing numbers) can't grow it without bound. Comfortably fits a screen's worth of distinct labels.
const TW_CACHE_MAX: usize = 96;

/// Rebuild + upload the shared UI palette from `ctx.ui_palettes` (colour → index). Index 0 stays
/// transparent. Cheap (one palette write); called when a new colour is added and once per `ui_begin`
/// (so it survives a `bg_new`, which rewrites every background palette).
fn upload_ui_palette(ctx: &mut GbaCtx) {
    let mut arr = [Rgb15::BLACK; 16]; // index 0 = transparent
    for (c, i) in ctx.ui_palettes.iter() {
        let v = *c as u32;
        arr[*i as usize] = Rgb::new(
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
        )
        .to_rgb15();
    }
    ctx.gfx
        .set_background_palette(UI_PAL_SLOT, &Palette16::new(arr));
}

/// The colour INDEX (1..15) for `color` within the shared UI palette, allocating + uploading on first
/// use and caching by colour. Overflow past 15 distinct colours (unrealistic — a UI uses a handful)
/// reuses the last index rather than churning banks.
fn ensure_ui_palette(ctx: &mut GbaCtx, color: i32) -> u8 {
    if let Some((_, idx)) = ctx.ui_palettes.iter().find(|(c, _)| *c == color) {
        return *idx;
    }
    let next = ctx.ui_palettes.len() + 1;
    if next > 15 {
        // Bank full. Every further colour paints in whatever was registered 15th, permanently and
        // silently — so count the overflows and surface them in `ui_mem_report()`, where a theme
        // that overruns the bank is visible in the harness instead of just looking miscoloured.
        ctx.ui_pal_overflow = ctx.ui_pal_overflow.saturating_add(1);
        return ctx.ui_palettes.last().map(|(_, i)| *i).unwrap_or(1);
    }
    let idx = next as u8;
    ctx.ui_palettes.push((color, idx));
    upload_ui_palette(ctx);
    idx
}

/// Fill the BLANK nibbles of a 4bpp row `val` (within `mask`) with palette index `bg`, so an opaque
/// text box paints its panel fill and its glyphs in the same write. Branch-free: the old version
/// looped over all 8 nibbles per row, and an in-place 3-line description patch runs that loop for
/// every pixel row of every tile column it covers (~2300 iterations for one repaint).
#[inline]
fn fill_blank_nibbles(val: u32, bg: u8, mask: u32) -> u32 {
    if bg == 0 {
        return val;
    }
    // bit 0 of each lit nibble (same set-detection as `colorize_row`), widened back to 0xF.
    let hi = val & 0x8888_8888;
    let lo = val & 0x7777_7777;
    let set = (hi | ((lo + 0x7777_7777) & 0x8888_8888)) >> 3;
    let blank = !(set * 0xF) & mask;
    val | ((bg as u32) * 0x1111_1111) & blank
}

/// Spread colour index `idx` into every SET nibble of a 4bpp mask row `px` (a font glyph row, where a
/// non-zero nibble = a lit pixel). Uses the same set-nibble detection as `blit_16_colour`, so it works
/// whatever non-zero value the font uses. Result: a row whose lit pixels hold `idx`, blank pixels 0.
#[inline]
fn colorize_row(px: u32, idx: u8) -> u32 {
    let hi = px & 0x8888_8888;
    let lo = px & 0x7777_7777;
    let set = (hi | ((lo + 0x7777_7777) & 0x8888_8888)) >> 3; // bit 0 of each lit nibble
    set * (idx as u32) // idx (≤15) fits one nibble ⇒ no carry between nibbles
}

/// Tile columns/rows in the UI canvas grid — the screenblock is `Background32x32`, and
/// `set_tile_dynamic16` itself wraps coordinates into it, so this IS the addressable space.
const UI_GRID: i32 = 32;
const UI_GRID_N: usize = (UI_GRID * UI_GRID) as usize;

/// A live UI canvas tile plus the `ui_cell` slot pointing at it (so a `swap_remove` can repair the
/// grid entry of whichever tile moved).
struct UiTile {
    tile: DynamicTile16,
    cell: u16,
}

/// `ui_cell` entry for a cell with nothing drawn in it (shows the backdrop).
const UI_CELL_EMPTY: u16 = 0;
/// Distinct fill colours a screen can hold shared solid tiles for — the UI palette's size, so every
/// colour `ui_rect` can be asked for has its own.
const UI_SOLID_N: usize = 16;
/// `ui_cell` entries from here up mean "shares the solid fill tile for palette entry
/// `slot - UI_CELL_SOLID_LO`" (see `ui_cow_solid_cell`). `ui_tiles` is capped at the visible cell
/// count, so a real tile index + 1 can never reach this band.
const UI_CELL_SOLID_LO: u16 = u16::MAX - (UI_SOLID_N as u16) + 1;

/// The fill colour a solid cell shares, or `None` if the cell isn't a solid fill.
#[inline]
fn ui_cell_solid_pal(slot: u16) -> Option<u8> {
    if slot >= UI_CELL_SOLID_LO {
        Some((slot - UI_CELL_SOLID_LO) as u8)
    } else {
        None
    }
}

/// The shared solid tile for `pal`, creating it if this is the screen's first fill in that colour.
fn ui_ensure_solid(ctx: &mut GbaCtx, pal: u8) {
    if ctx.ui_solids.len() != UI_SOLID_N {
        ctx.ui_solids.clear();
        ctx.ui_solids.resize_with(UI_SOLID_N, || None);
    }
    let i = (pal as usize) % UI_SOLID_N;
    if ctx.ui_solids[i].is_none() {
        ctx.ui_solids[i] = Some(DynamicTile16::new().fill_with(pal));
    }
}

/// Make sure the canvas's shared transparent tile exists. It is the same one `ui_begin` points
/// released cells at; a cleared shared-solid cell needs somewhere to point too, and the screenblock
/// entry has to reference SOME tile. Palette index 0 is the transparent one (the UI palette
/// allocator hands out from 1 upward and can never return 0), so a tile filled with 0 shows the
/// backdrop — which is exactly what "cleared" means here.
fn ui_ensure_blank(ctx: &mut GbaCtx) {
    if ctx.ui_blank.is_none() {
        ctx.ui_blank = Some(DynamicTile16::new().fill_with(0));
    }
}

/// Hand back every shared solid tile. Only safe once no `ui_cell` still points into the band.
fn ui_drop_solids(ctx: &mut GbaCtx) {
    ctx.ui_solids.clear();
}

/// Index of tile `(tx, ty)` in `ui_cell`. Wraps like the screenblock does (a stray out-of-screen
/// coordinate paints on the far side rather than panicking — the pre-existing behaviour of
/// `set_tile_dynamic16`). `rem_euclid` on a power of two compiles to a mask.
#[inline]
fn ui_cell_idx(tx: i32, ty: i32) -> usize {
    (ty.rem_euclid(UI_GRID) * UI_GRID + tx.rem_euclid(UI_GRID)) as usize
}

/// Size the lookup grid on first use (a game that never draws UI pays no EWRAM for it).
fn ui_grid_ready(ctx: &mut GbaCtx) {
    if ctx.ui_cell.len() != UI_GRID_N {
        ctx.ui_cell.clear();
        ctx.ui_cell.resize(UI_GRID_N, UI_CELL_EMPTY);
    }
}

/// The tile drawn in cell `idx`, if any (a solid-fill or empty cell has none of its own).
#[inline]
fn ui_tile_at(ctx: &mut GbaCtx, idx: usize) -> Option<&mut DynamicTile16> {
    let slot = *ctx.ui_cell.get(idx)?;
    if slot == UI_CELL_EMPTY || slot >= UI_CELL_SOLID_LO {
        return None;
    }
    ctx.ui_tiles
        .get_mut((slot - 1) as usize)
        .map(|t| &mut t.tile)
}

/// Record `tile` as cell `idx`'s own tile. `ui_tiles` never exceeds `UI_GRID_N` entries, so the
/// index + 1 can't collide with the solid-fill band.
fn ui_put_tile(ctx: &mut GbaCtx, idx: usize, tile: DynamicTile16) {
    ctx.ui_tiles.push(UiTile {
        tile,
        cell: idx as u16,
    });
    ctx.ui_cell[idx] = ctx.ui_tiles.len() as u16;
    if ctx.ui_tiles.len() > ctx.ui_peak_tiles {
        ctx.ui_peak_tiles = ctx.ui_tiles.len();
    }
}

/// Release cell `idx`'s own tile (freeing its VRAM slot) and mark the cell empty. The caller must
/// re-point the screenblock cell itself.
fn ui_drop_tile(ctx: &mut GbaCtx, idx: usize) {
    let slot = match ctx.ui_cell.get_mut(idx) {
        Some(s) => core::mem::replace(s, UI_CELL_EMPTY),
        None => return,
    };
    if slot == UI_CELL_EMPTY || slot >= UI_CELL_SOLID_LO {
        return;
    }
    let i = (slot - 1) as usize;
    if i >= ctx.ui_tiles.len() {
        return;
    }
    ctx.ui_tiles.swap_remove(i);
    if let Some(moved) = ctx.ui_tiles.get(i) {
        let cell = moved.cell as usize;
        ctx.ui_cell[cell] = (i + 1) as u16;
    }
}

/// Drop every tile WITHOUT touching the background (used when the screenblock is going away anyway).
/// Also releases the lookup grid; `ui_grid_ready` rebuilds it on the next `ui_begin`.
fn ui_forget_tiles(ctx: &mut GbaCtx) {
    ctx.ui_tiles.clear();
    ctx.ui_cell.clear();
    ui_drop_solids(ctx);
    ui_recycle_reveal(ctx);
}

/// Invalidate the reveal cache but KEEP its row buffer for the next shape (see `ui_row_spare`).
fn ui_recycle_reveal(ctx: &mut GbaCtx) {
    if let Some(c) = ctx.ui_reveal.take() {
        let mut rows = c.rows;
        rows.clear();
        if rows.capacity() > ctx.ui_row_spare.capacity() {
            ctx.ui_row_spare = rows;
        }
    }
}

/// Point every live UI cell at the shared blank tile and drop the tiles. Keeps `ui_bg` / `ui_blank`
/// allocated — recreating `RegularBackground` on every dialog/pause open is a multi-frame hitch.
/// Caller must ensure bg/blank exist (see `ui_begin`).
fn ui_blank_tiles(ctx: &mut GbaCtx) {
    let mut clr_n = 0u32;
    for i in 0..ctx.ui_cell.len() {
        if ctx.ui_cell[i] == UI_CELL_EMPTY {
            continue;
        }
        ctx.ui_cell[i] = UI_CELL_EMPTY;
        let tx = (i as i32) % UI_GRID;
        let ty = (i as i32) / UI_GRID;
        if let (Some(bg), Some(blank)) = (ctx.ui_bg.as_mut(), ctx.ui_blank.as_ref()) {
            bg.set_tile_dynamic16(Vector2D::new(tx, ty), blank, TileEffect::default());
        }
        clr_n += 1;
        if clr_n.is_multiple_of(8) {
            pump_audio(ctx);
        }
    }
    // Dropped only after no cell references them, so the VRAM slots are free to re-use.
    ctx.ui_tiles.clear();
    ui_drop_solids(ctx);
    ui_recycle_reveal(ctx);
}

/// How many tiles `ui_reserve_tiles` holds at once by default. agb tracks live `DynamicTile16`s in a
/// HashMap that doubles its backing store when the entry count passes 3/5 of it, so holding more than
/// 307 at once forces the 512→1024-node step (a 20KB allocation) and, since the map never shrinks,
/// nothing on a 240x160 canvas (30x20 = 600 cells, all a menu screen can ever light up) can force the
/// next one.
const UI_TILE_RESERVE: usize = 320;

/// Entries `ui_tiles` is reserved to, once, at boot. The grid ADDRESSES 32x32 cells but the screen only
/// shows 30x20 = 600, and a cell you cannot see never gets a tile — so 640 is the real ceiling with slack,
/// and reserving it means the table can never reallocate while a game is running. Reserving the full 1024
/// instead costs 4.6KB more for cells that are off-screen by construction, which on a 256K heap is worth
/// more than the slack.
const UI_TILE_TABLE_CAP: usize = 640;

/// `ui_reserve_tiles([n])` — pay for agb's dynamic-tile bookkeeping NOW, at boot, while the heap is
/// empty and unfragmented.
///
/// A full menu screen is hundreds of unique tiles (a shop tab is ~500 cells of glyphs over filled
/// panels), which lands right on the boundary where agb re-allocates its live-tile map. Reached mid-game
/// that 20KB request finds a heap full of map/sprite/script data and fails, so an ordinary in-place text
/// patch — moving the shop cursor one row — took the whole game down. The tiles created here are dropped
/// immediately: only the map growth they provoke is permanent, which is the point.
///
/// ⚠️ **The dropped tiles do not give their VRAM back until a frame commits.** Call `frame()` after
/// this and before anything uploads a large background, or the warm set is still resident and the
/// two compete: at a 460-tile reserve that is ~15KB of background VRAM held past the end of this
/// call. A large SRPG example hit it uploading a battle floor in `isoInit` and it surfaced as
/// `tile_allocator.rs:47 Ran out of video RAM for tiles` inside `bg_new` — a panic that names the
/// background being created and says nothing about the reserve that filled VRAM. It looks exactly
/// like "the board is too big", and shrinking the board is the wrong fix: the board here was 20.5KB
/// against a 64KB budget, the same size as one that had been booting fine for weeks.
/// `ui_tiles_used()` — how many canvas cells currently own a tile. THE number `tileReserve` must
/// cover: the canvas RETAINS whatever it ever grows to (`ui_clear` recovers almost nothing), so a
/// screen's true cell count is a budget line, not a curiosity — and it was guessed wrong twice
/// (320 held ~24KB and starved the tasting; 224 under-covered the pause screen and it OOM'd on a
/// fragmented dungeon heap). Measure, then reserve.
pub fn ui_tiles_used(_args: &[Value]) -> Value {
    with_ctx(|ctx| Value::Number(ctx.ui_tiles.len() as f64))
}
/// See [`ui_tiles_used`].
pub fn ui_tiles_used_typed() -> i32 {
    with_ctx(|ctx| ctx.ui_tiles.len() as i32)
}

/// `bg_tile_map_reserve(n)` — pre-size agb's shared live-tile map ONCE, at boot, on the empty
/// heap. OPT-IN for games whose heaviest scene plus a full-screen UI repaint holds several hundred
/// live tiles: without it, the map's 512 -> 1024 node rehash is a 20,480-byte contiguous ask
/// issued mid-game on a fragmented heap — a downstream game's pause-screen OOM. An earlier version
/// pre-sized unconditionally inside agb and taxed every ROM in the repo 20KB; this native is the
/// same fix billed only to the game that needs it. Sizing rules live on
/// `VRamManager::reserve_tile_capacity`; 600 is the measured number for a 308..614 live-tile peak.
pub fn bg_tile_map_reserve(args: &[Value]) -> Value {
    let n = (num(args, 0) as i32).max(0) as usize;
    agb::display::tiled::VRAM_MANAGER.reserve_tile_capacity(n);
    Value::Null
}

/// See [`bg_tile_map_reserve`].
pub fn bg_tile_map_reserve_typed(n: i32) {
    agb::display::tiled::VRAM_MANAGER.reserve_tile_capacity(n.max(0) as usize);
}

pub fn ui_reserve_tiles(args: &[Value]) -> Value {
    let n = if args.is_empty() {
        UI_TILE_RESERVE
    } else {
        (num(args, 0) as i32).max(0) as usize
    };
    with_ctx(|ctx| {
        // ⚠️ THE GRID IS PREPARED WHETHER OR NOT ANY TILES ARE WARMED. This used to sit behind an
        // `if n == 0 { return }` early-out, so a game passing `tileReserve: 0` never got its canvas
        // grid and halted before the first frame. agb does NOT pre-size its live-tile map — a
        // non-zero `n` is still what forces the HashMap doubling on a clean heap.
        ui_grid_ready(ctx);
        if n == 0 {
            return;
        }
        // Our OWN table is reserved to the bound it can never exceed (one entry per canvas cell), not to
        // `n`. These are two different costs and only one of them is optional: `n` buys agb's HashMap
        // step, while this buys the Vec that records which cell owns which tile.
        //
        // Reserving it to `n` left the table growing later, and its doubling is the SAME failure the warm
        // loop exists to prevent, just smaller and later: a shop tab lights ~282 cells, the quantity
        // prompt over it crosses 320, and the 320→640 step asks for 7,680 contiguous bytes at the exact
        // moment the heap is most fragmented. It failed, and the shop took the game down. Paid in full
        // here — at boot, on an empty heap — it cannot happen at all.
        // ORDER MATTERS, and it is the difference between one 40KB allocation live at a time and two.
        //
        // Both of these are ~40KB on a full-size canvas: the table is UI_TILE_TABLE_CAP * 64 bytes,
        // and the warm buffer is `n` DynamicTile16 handles at 64 bytes each. Reserving the table
        // FIRST and then building the warm buffer means both are held simultaneously — ~80KB of
        // contiguous demand at boot, which a card game carrying a few hundred cards and a campaign
        // cannot meet. It failed as "memory allocation of 40960 bytes failed" during boot, and the
        // only apparent fix was to warm fewer tiles, which just moves the same failure into the first
        // heavy screen.
        //
        // Warm first, drop it, then reserve: the peak is one block, not two, and the growth agb needs
        // is provoked either way.
        let mut warm: alloc::vec::Vec<DynamicTile16> = alloc::vec::Vec::with_capacity(n);
        for _ in 0..n {
            warm.push(DynamicTile16::new());
        }
        drop(warm);
        // ⚠️⚠️ RESERVED TO WHAT THE GAME ASKED FOR, not to the screen's worst case.
        //
        // This was `reserve(UI_TILE_TABLE_CAP)` — 640 entries, 40,960 bytes — for every game,
        // whatever `tileReserve` it passed. On a large SRPG example that is a THIRD of the whole heap,
        // claimed at boot and held for the life of the ROM, through every battle, for menus that
        // are not on screen: measured 95,232 bytes free before `uiInit` and 48,128 after.
        //
        // A game that asks to warm `n` tiles is telling us how big its canvas gets; reserving more
        // than that is charging it for a screen it never draws. `n` is still an upper bound the
        // table cannot exceed in practice (`ui_tiles` is capped at the visible cell count), and the
        // whole point of the warm loop above — provoking agb's growth once, on an empty heap — is
        // unchanged. A game that wants the old behaviour passes `tileReserve: 640`.
        // ⚠️ SIZED BY WHAT THE GAME ASKED FOR. `UI_TILE_TABLE_CAP` (640 entries, 7,680 bytes) is the
        // worst case for a FULL-SCREEN canvas; a game passing `tileReserve` is telling us how big
        // its canvas actually gets, and charging it for a screen it never draws is a third of a
        // battle's budget on a large SRPG example. `ui_tiles` is capped at the visible cell count
        // regardless, so this cannot under-reserve into a failure — it can only grow if the game
        // exceeds its own stated bound.
        ctx.ui_tiles.reserve(n.clamp(1, UI_TILE_TABLE_CAP));
    });
    Value::Null
}

/// `ui_begin()` — start a FRESH UI text canvas, discarding everything previously drawn with `ui_text`
/// (and freeing its tiles). Call once at the top of a menu re-layout, issue `ui_text` calls, then
/// present with `frame()`. Only re-layout on change (menus are cold) — the canvas persists across
/// frames until the next `ui_begin`.
pub fn ui_begin(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        // ONE ui_begin per frame. The canvas this call throws away does not return its tiles to VRAM
        // until the frame boundary, so a second one in the same frame asks for a whole extra canvas
        // and dies inside agb's tile allocator with "Ran out of video RAM for tiles" — a message that
        // points at agb rather than at the call that broke the rule. That misdirection has cost real
        // debugging time, so fail here instead, where the rule lives.
        //
        // The fix at the call site is always the same shape: on the frame that decides to rebuild,
        // call `ui_clear()` and return; do the `ui_begin` on the next frame.
        assert!(
            !ctx.ui_began_this_frame,
            "ui_begin called twice in one frame: the previous canvas has not released its VRAM yet. \
             Call ui_clear() and return, then ui_begin() on the next frame."
        );
        ctx.ui_began_this_frame = true;
        // Keep ONE persistent background (recreating it every render churns the screenblock → the wide
        // title flickers / drops a chunk mid-update). Instead, point every previously-used cell at the
        // shared blank tile (releasing that render's dynamic tiles) then drop them, and rebuild the
        // tiles fresh below. (Reusing a tile's pixels via data_mut rendered blank on agb 0.25, so we
        // still recreate the TILES each render — just not the background.)
        if ctx.ui_bg.is_none() {
            ctx.ui_bg = Some(RegularBackground::new(
                Priority::P0,
                RegularBackgroundSize::Background32x32,
                TileFormat::FourBpp,
            ));
            ctx.ui_blank = Some(DynamicTile16::new().fill_with(0));
        }
        ui_grid_ready(ctx);
        // Releasing the PREVIOUS render's tiles (hundreds on a re-layout of a full menu) is a long,
        // tish-agb-silent loop → pump the mixer through it so the BGM doesn't underrun.
        ui_blank_tiles(ctx);
        // Re-assert the shared UI palette (a scene's `bg_new` rewrites every background palette, so a
        // menu drawn after one would otherwise render with a stale/empty slot). Cheap: one write.
        if !ctx.ui_palettes.is_empty() {
            upload_ui_palette(ctx);
        }
    });
    Value::Null
}

/// `ui_hide()` — blank the UI canvas but KEEP the `RegularBackground` allocated. Use when dismissing
/// a pause/dialog during gameplay so the next open skips a full screenblock recreate. Prefer
/// `ui_clear` on scene load (frees the screenblock for map VRAM).
pub fn ui_hide(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        if ctx.ui_bg.is_some() {
            if ctx.ui_blank.is_none() {
                ctx.ui_blank = Some(DynamicTile16::new().fill_with(0));
            }
            ui_blank_tiles(ctx);
        } else {
            ui_forget_tiles(ctx);
        }
        ctx.tw_cache.clear();
        // Hand the palette bank back too. Both branches above have just dropped every live tile, so
        // nothing references a palette index any more — the same argument `ui_clear` makes.
        //
        // Without this the 15 slots are a SESSION budget for any game that never loads a scene (the
        // only other place that clears them is `ui_clear`, which `loadScene` calls). A game whose
        // menus open over a persistent world — a shop, then a party menu, then a shop again — walks
        // the bank up until every later colour silently paints as the 15th one.
        ctx.ui_palettes.clear();
        ctx.ui_pal_overflow = 0;
    });
    Value::Null
}

/// `ui_clear()` — tear the UI canvas down entirely (leaving the UI for gameplay / scene load),
/// freeing its screenblock so map layers can claim the VRAM.
pub fn ui_clear(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        ui_forget_tiles(ctx);
        ctx.tw_cache.clear();
        ctx.ui_blank = None;
        ctx.ui_bg = None;
        // The canvas is gone, so no live tile references a palette index any more: hand the bank
        // back. Without this the 15 slots are a SESSION budget — a game with several differently
        // themed screens exhausts it and every later colour silently paints as the 15th one.
        ctx.ui_palettes.clear();
        ctx.ui_pal_overflow = 0;
        // Dropping the canvas only QUEUES its tiles. Without gc here they stay in VRAM until the
        // next commit, and a caller that ui_clear()s then bg_new()s a board in the same breath
        // peaks at both — the title canvas plus the floor — and the floor comes up empty or the
        // allocator jumps.
        agb::display::tiled::VRAM_MANAGER.gc();
    });
    Value::Null
}

/// Blit one packed 8-pixel font row (`px`, 4bpp) at absolute pixel `(ax, ay)` into the shared UI tile
/// canvas, spilling into the right-neighbour tile when the row straddles a tile boundary. New tiles are
/// created + linked into the background once (`set_tile_dynamic16` — the tile is VRAM-backed, so later
/// blits into the SAME tile show live and ACCUMULATE, which is what stops overlapping text clipping).
fn ui_blit_row(ctx: &mut GbaCtx, ax: i32, ay: i32, px: u32, idx: u8) {
    let colored = colorize_row(px, idx); // lit pixels now hold the colour index
    let tx = ax.div_euclid(8);
    let ty = ay.div_euclid(8);
    let xit = (ax.rem_euclid(8) * 4) as u32; // bit offset within the 32-bit tile row (whole nibbles)
    let yi = ay.rem_euclid(8) as usize;
    ui_blit_cell(ctx, tx, ty, yi, colored << xit);
    if xit > 0 {
        ui_blit_cell(ctx, tx + 1, ty, yi, colored >> (32 - xit));
    }
}
/// If `(tx,ty)` is still a shared solid-fill cell, copy-on-write it into a unique tile pre-filled
/// with the solid colour so subsequent glyph/border blits don't mutate every panel cell at once.
fn ui_cow_solid_cell(ctx: &mut GbaCtx, idx: usize, tx: i32, ty: i32) {
    // The colour comes from the CELL, not from whatever `ui_rect` filled last: a cell belonging to
    // an earlier panel must copy-on-write in that panel's colour, not the newest one's.
    let pal = match ctx.ui_cell.get(idx).copied().and_then(ui_cell_solid_pal) {
        Some(p) => p,
        None => return,
    };
    ctx.ui_cell[idx] = UI_CELL_EMPTY;
    let tile = DynamicTile16::new().fill_with(pal);
    if let Some(bg) = ctx.ui_bg.as_mut() {
        bg.set_tile_dynamic16(
            Vector2D::new(tx, ty),
            &tile,
            TileEffect::default().palette(UI_PAL_SLOT),
        );
    }
    ui_put_tile(ctx, idx, tile);
}

/// Composite one 4bpp row `val` (nibbles already carry colour indices) into the tile at `(tx,ty)`.
/// `blit_16_colour` replaces per-nibble wherever `val` is non-zero, so different-coloured glyphs in
/// the same tile coexist — every tile shares `UI_PAL_SLOT`, and the index in each pixel picks the
/// colour. Non-overlapping pixels accumulate; a same-pixel overwrite takes the later colour.
fn ui_blit_cell(ctx: &mut GbaCtx, tx: i32, ty: i32, yi: usize, val: u32) {
    if val == 0 {
        return; // nothing to composite for this cell
    }
    let idx = ui_cell_idx(tx, ty);
    if idx >= ctx.ui_cell.len() {
        return;
    }
    ui_cow_solid_cell(ctx, idx, tx, ty);
    if let Some(tile) = ui_tile_at(ctx, idx) {
        blit_16_colour(&mut tile.data_mut()[yi..yi + 1], &[val]);
        return;
    }
    let mut tile = DynamicTile16::new().fill_with(0);
    blit_16_colour(&mut tile.data_mut()[yi..yi + 1], &[val]);
    if let Some(bg) = ctx.ui_bg.as_mut() {
        bg.set_tile_dynamic16(
            Vector2D::new(tx, ty),
            &tile,
            TileEffect::default().palette(UI_PAL_SLOT),
        );
    }
    ui_put_tile(ctx, idx, tile);
}

/// REPLACE (not accumulate) the `mask` nibbles of tile `(tx,ty)` row `yi` with `val`'s masked nibbles:
/// `d = (d & !mask) | (val & mask)`. Unlike `ui_blit_cell` (which ORs and can't clear pixels), this
/// clears the masked pixels the new content leaves blank — the basis for flicker-free in-place text
/// updates (`ui_text_box`): one write takes a row straight from old→new with no black intermediate, and
/// the mask is a horizontal pixel span so it never touches a neighbouring line's rows in a shared tile.
#[allow(dead_code)] // single-row primitive kept beside ui_masked_write_rows for one-off writes
fn ui_masked_write(ctx: &mut GbaCtx, tx: i32, ty: i32, yi: usize, val: u32, mask: u32) {
    ui_masked_write_rows(ctx, tx, ty, yi, &[val], mask);
}

/// Fill an exact-pixel rectangle in one solid colour, walking it TILE-COLUMN first.
///
/// The obvious loop is row-major — for each pixel row, for each tile column, write the span — and it
/// is what this did. It resolves the same tile once per pixel row (a 12px-tall bar re-finds each of
/// its tiles twelve times) and recomputes that column's mask and colour word every row, though
/// neither depends on the row at all. Both are hoisted here: the mask is computed once per column,
/// and the rows inside a tile go out as ONE `ui_masked_write_rows` run, which is exactly the batching
/// that function was written for. A 100×12 bar drops from 156 tile resolutions to 26.
///
/// Used for borders too, as four 1px spans. Drawing a perimeter with `ui_set_pixel` costs a tile
/// resolution PER PIXEL, and a panel border is the most common thing on a menu screen.
fn ui_fill_span(ctx: &mut GbaCtx, x: i32, y: i32, w: i32, h: i32, pal: u8) {
    if w <= 0 || h <= 0 {
        return;
    }
    let tx0 = x.div_euclid(8);
    let tx1 = (x + w - 1).div_euclid(8);
    let mut tx = tx0;
    while tx <= tx1 {
        let tile_left = tx * 8;
        let l = (x.max(tile_left) - tile_left).clamp(0, 8);
        let r = ((x + w).min(tile_left + 8) - tile_left).clamp(0, 8);
        if r > l {
            let full: u32 = if r >= 8 {
                0xFFFF_FFFF
            } else {
                (1u32 << (r as u32 * 4)) - 1
            };
            let low: u32 = if l <= 0 {
                0
            } else {
                (1u32 << (l as u32 * 4)) - 1
            };
            let mask = full & !low;
            // Solid palette index in every nibble under the mask.
            let mut val = 0u32;
            let mut n = 0u32;
            while n < 8 {
                let nib = 0xFu32 << (n * 4);
                if (mask & nib) != 0 {
                    val |= (pal as u32) << (n * 4);
                }
                n += 1;
            }
            let vals = [val; 8];
            let mut row = 0i32;
            while row < h {
                let ay = y + row;
                let ty = ay.div_euclid(8);
                let yi = ay.rem_euclid(8) as usize;
                // How much of this tile's 8 rows this rect still wants.
                let run = core::cmp::min(8 - yi as i32, h - row) as usize;
                ui_masked_write_rows(ctx, tx, ty, yi, &vals[..run], mask);
                row += run as i32;
            }
        }
        tx += 1;
    }
}

/// `ui_masked_write` for a RUN of consecutive rows in the same tile, resolving the tile once. A text
/// patch writes every pixel row of every tile column it covers; per-row resolution meant up to 8× the
/// grid lookups and, for a new tile, 8 separate allocate-and-link paths.
fn ui_masked_write_rows(ctx: &mut GbaCtx, tx: i32, ty: i32, yi: usize, vals: &[u32], mask: u32) {
    if mask == 0 || vals.is_empty() {
        return;
    }
    let idx = ui_cell_idx(tx, ty);
    if idx >= ctx.ui_cell.len() {
        return;
    }
    ui_cow_solid_cell(ctx, idx, tx, ty);
    if let Some(tile) = ui_tile_at(ctx, idx) {
        let d = tile.data_mut();
        // A whole-tile mask needs no read-modify-write (the common interior column).
        let full = mask == u32::MAX;
        for (k, val) in vals.iter().enumerate() {
            let r = yi + k;
            if r >= d.len() {
                break;
            }
            d[r] = if full {
                *val
            } else {
                (d[r] & !mask) | (val & mask)
            };
        }
        return;
    }
    if vals.iter().all(|v| v & mask == 0) {
        return; // nothing to draw and no tile to clear → don't allocate an empty tile
    }
    let mut tile = DynamicTile16::new().fill_with(0);
    {
        let d = tile.data_mut();
        for (k, val) in vals.iter().enumerate() {
            let r = yi + k;
            if r >= d.len() {
                break;
            }
            d[r] = val & mask;
        }
    }
    if let Some(bg) = ctx.ui_bg.as_mut() {
        bg.set_tile_dynamic16(
            Vector2D::new(tx, ty),
            &tile,
            TileEffect::default().palette(UI_PAL_SLOT),
        );
    }
    ui_put_tile(ctx, idx, tile);
}

/// agb's `Layout` never emits a letter group for a lone character, so a single-character string
/// shapes to nothing and draws NOTHING at all. That is invisible until a UI drops its labels: a card
/// game showing a bare power of "7" or a lane total of "0" renders blank, while "07" or "P:7" is fine.
///
/// Padding with a trailing space gives the grouper a terminator. A space contributes no pixels, so
/// this cannot change what a caller sees for any string that already worked.
/// Identity. It used to pad a one-character string with a trailing space, because agb's `Layout`
/// emits no letter group for a lone character and such a string drew NOTHING — a lane total of "0"
/// or a bare "7" rendering blank.
///
/// The pad was worse than the bug it fixed. Trailing whitespace leaves agb's grouper with its index
/// on the end of the string; it then re-slices `text[idx..]` and panics with `start byte index N is
/// out of bounds for string of length 0`, from inside agb with nothing naming the caller. Two spaces
/// did not help — the grouper takes the whole trailing run. The topdown RPG port hit it on the first frame of any
/// scene whose rupee, key or bomb counter was a single digit, which is every new game.
///
/// The lone-character case is handled in `build_text_objs` instead, where it can be done without
/// whitespace: lay out the character twice with one character per group and keep the first group.
/// Identity. The lone-character case it used to patch up is handled where the text is laid out:
/// `build_text_objs` for sprite text, and `shape_text_lone` + `lone_settings` for the canvas paths.
/// Returning the doubled string from here instead drew every single-digit counter TWICE, because
/// the sprite path then saw a two-character string and took its ordinary route.
fn shape_text(text: alloc::string::String) -> alloc::string::String {
    text
}

/// agb's `Layout` emits no letter group for a lone character, so a one-character string draws
/// NOTHING — a rupee counter of "0" or a menu value of "-" renders blank.
///
/// The fix is to give the grouper a second character to terminate on and then draw only the first
/// group. It must NOT be whitespace: a trailing space is absorbed into the group and leaves the
/// grouper's index on the end of the string, and re-slicing there panics inside agb with
/// `start byte index N is out of bounds for string of length 0` — from an agb source file, with
/// nothing naming the caller. A trailing newline does the same. The topdown RPG port hit it on the first frame of
/// any scene with a single-digit counter, and again from the debug menu's one-character values.
///
/// Returns the text to lay out and whether only the first group should be drawn.
/// One character per group while drawing a padded lone character, so the duplicate that terminates
/// the group cannot end up inside it — otherwise the first (and only) group drawn would be both
/// copies.
fn lone_settings(settings: &LayoutSettings, only_first: bool) -> LayoutSettings {
    if only_first {
        settings.clone().with_max_chars_per_group(1)
    } else {
        settings.clone()
    }
}

fn shape_text_lone(text: alloc::string::String) -> (alloc::string::String, bool) {
    if text.chars().nth(1).is_none() && !text.is_empty() {
        let mut t = text.clone();
        t.push_str(&text);
        return (t, true);
    }
    (text, false)
}

/// Is there anything for agb's `Layout` to lay out?
///
/// Empty text has no glyphs, and agb's grouper does not treat that as a no-op: it can walk its own
/// index past the end and panic with `start byte index N is out of bounds for string of length 0`,
/// from inside an agb source file with nothing naming the caller. Blanking a line with `""` is the
/// ordinary way to clear one here, so every entry point that can be handed a caller's string checks
/// this first.
#[inline]
fn layoutable(text: &str) -> bool {
    !text.is_empty()
}

/// `ui_text(fontHandle, x, y, text, color?, maxw?, align?, shadow?, sdx?, sdy?)` — draw `text` at pixel
/// `(x, y)` onto the UI text canvas, in an imported font (`font:`; -1 = built-in) and `color`
/// (0xRRGGBB, default white).
/// `maxw` is the WRAP width in px (text longer than it flows onto more lines below `y`); omitted/≤0 =
/// 512 (effectively no wrap). Optional `align` (`"left"`/`"center"`/`"right"`/`"justify"`, also
/// `"start"`/`"end"`) uses agb line alignment within `maxw` — pass the leaf box width for centre/right.
/// Background tiles, so no OAM / sprite VRAM, and overlapping calls accumulate rather than clip.
/// Requires a prior `ui_begin()`. (Pair with `text_wrap_height` to size a wrapping paragraph's box.)
///
/// `shadow` (0xRRGGBB, omitted/negative = none) draws the same text once more UNDERNEATH, offset by
/// `sdx`/`sdy` (default 1, 1) — what makes canvas text legible over a busy map or over title art with no
/// panel behind it, matching what sprite `text_draw` already offers. It costs a second shaping + blit
/// pass, so it belongs on static text (a title, a label painted once per screen), not on anything
/// redrawn per frame. Two full passes rather than interleaved per glyph: a same-pixel write takes the
/// LATER colour, so a tightly-kerned neighbour's shadow would otherwise eat the previous glyph's body.
pub fn ui_text(args: &[Value]) -> Value {
    let font_handle = num(args, 0) as i32;
    let x = num(args, 1) as i32;
    let y = num(args, 2) as i32;
    let (text, only_first) = shape_text_lone(
        args.get(3)
            .map(|v| v.to_display_string())
            .unwrap_or_default(),
    );
    let color = if args.len() > 4 {
        num(args, 4) as i32
    } else {
        0xFFFFFF
    };
    let maxw = if args.len() > 5 && num(args, 5) as i32 > 0 {
        num(args, 5) as i32
    } else {
        512
    };
    let align = align_arg(args, 6);
    let shadow = if args.len() > 7 {
        num(args, 7) as i32
    } else {
        -1
    };
    let sdx = if args.len() > 8 {
        num(args, 8) as i32
    } else {
        1
    };
    let sdy = if args.len() > 9 {
        num(args, 9) as i32
    } else {
        1
    };
    with_ctx(|ctx| {
        ui_feed_audio(ctx); // skipped while `audio_defer(1)` (batched list scroll refill)
        if ctx.ui_bg.is_none() {
            return;
        }
        let font: &'static Font = if font_handle < 0 {
            &FONT
        } else {
            match tishlang_runtime_gba::gba::asset_font(font_handle) {
                Some(f) => f,
                None => return,
            }
        };
        let settings = layout_settings(maxw, align);
        if shadow >= 0 && (sdx != 0 || sdy != 0) {
            let spal = ensure_ui_palette(ctx, shadow);
            for g in Layout::new(&text, font, &lone_settings(&settings, only_first))
                .take(if only_first { 1 } else { max_groups(&text) })
            {
                let gp = g.position();
                for (px_start, px) in g.pixels_packed() {
                    let ax = px_start.x + gp.x + x + sdx;
                    let ay = px_start.y + gp.y + y + sdy;
                    ui_blit_row(ctx, ax, ay, px, spal);
                }
            }
        }
        let pal = ensure_ui_palette(ctx, color);
        for g in Layout::new(&text, font, &lone_settings(&settings, only_first))
            .take(if only_first { 1 } else { max_groups(&text) })
        {
            let gp = g.position();
            for (px_start, px) in g.pixels_packed() {
                let ax = px_start.x + gp.x + x;
                let ay = px_start.y + gp.y + y;
                ui_blit_row(ctx, ax, ay, px, pal);
            }
        }
    });
    Value::Null
}

/// `ui_text_span(fontHandle, x, y, text, color, maxw, from, to, align?)` — like `ui_text`, but only blits the
/// groups whose FIRST character index is in `[from, to)`. The typewriter (`uiReveal`) calls this every
/// frame with the SAME text + a growing `to`; agb's `Layout` (shaping/line-breaking) is O(all glyphs), so
/// re-running it each frame was O(n²) and ~a frame of CPU PER character on hardware. Instead we SHAPE ONCE
/// into `ctx.ui_reveal` (cached by text/font/geometry/align) and each call just blits the cached rows in range —
/// the actual O(delta)-ish behaviour the old comment claimed. Cache clears on `ui_begin`/`ui_clear`.
pub fn ui_text_span(args: &[Value]) -> Value {
    let font_handle = num(args, 0) as i32;
    let x = num(args, 1) as i32;
    let y = num(args, 2) as i32;
    let (text, only_first) = shape_text_lone(
        args.get(3)
            .map(|v| v.to_display_string())
            .unwrap_or_default(),
    );
    let color = num(args, 4) as i32;
    let maxw = if num(args, 5) as i32 > 0 {
        num(args, 5) as i32
    } else {
        512
    };
    let from = num(args, 6) as i32;
    let to = num(args, 7) as i32;
    let align = align_arg(args, 8);
    let align_u = align_to_u8(align);
    with_ctx(|ctx| {
        ui_feed_audio(ctx); // skipped while `audio_defer(1)` (batched list scroll refill)
        if ctx.ui_bg.is_none() {
            return;
        }
        let pal = ensure_ui_palette(ctx, color);
        // (Re)shape only when the text or its geometry changed — otherwise reuse the cached glyph rows.
        let stale = match ctx.ui_reveal.as_ref() {
            Some(c) => {
                c.text != text
                    || c.font_handle != font_handle
                    || c.maxw != maxw
                    || c.align != align_u
                    || c.x != x
                    || c.y != y
            }
            None => true,
        };
        if stale {
            let font: &'static Font = if font_handle < 0 {
                &FONT
            } else {
                match tishlang_runtime_gba::gba::asset_font(font_handle) {
                    Some(f) => f,
                    None => return,
                }
            };
            // Refill the previous shape's row buffer (or the one parked at the last canvas blank) instead
            // of building a new one. A page of text is several hundred glyph rows at 16 bytes each, so a
            // fresh Vec per reshape walks the doubling ladder again — 512B, 1K, 2K, 4K — and those are
            // precisely the requests that failed mid-conversation once the game had filled the heap.
            let mut rows: alloc::vec::Vec<(i32, i32, i32, u32)> = match ctx.ui_reveal.take() {
                Some(c) => {
                    let mut r = c.rows;
                    r.clear();
                    r
                }
                None => core::mem::take(&mut ctx.ui_row_spare),
            };
            let mut cum: i32 = 0; // character index of the current group's first char
            let settings = layout_settings(maxw, align);
            for g in Layout::new(&text, font, &lone_settings(&settings, only_first))
                .take(if only_first { 1 } else { max_groups(&text) })
            {
                let len = g.text().chars().count() as i32;
                let gp = g.position();
                for (px_start, px) in g.pixels_packed() {
                    rows.push((cum, px_start.x + gp.x + x, px_start.y + gp.y + y, px));
                }
                cum += len;
            }
            ctx.ui_reveal = Some(RevealCache {
                text,
                font_handle,
                maxw,
                align: align_u,
                x,
                y,
                rows,
            });
        }
        // Blit the cached rows in [from, to). Rows are ordered by group first-char, so binary-search to
        // the slice instead of scanning all of them — that keeps a per-char reveal O(delta), not O(n).
        // Take the cache out so the loop doesn't borrow ctx.ui_reveal while ui_blit_row borrows ctx
        // (disjoint fields, but the borrow checker can't see that here).
        let cache = ctx.ui_reveal.take();
        if let Some(c) = cache.as_ref() {
            let start = c.rows.partition_point(|(gc, _, _, _)| *gc < from);
            let mut i = start;
            while i < c.rows.len() {
                let (gc, ax, ay, px) = c.rows[i];
                if gc >= to {
                    break;
                }
                ui_blit_row(ctx, ax, ay, px, pal);
                i += 1;
            }
        }
        ctx.ui_reveal = cache;
    });
    Value::Null
}

/// `ui_text_box(fontHandle, x, y, text, color, boxW)` — draw a SINGLE line of `text` at `(x, y)` with
/// OPAQUE / REPLACE semantics over the horizontal footprint `[x, x+boxW)` (one font line tall): every
/// pixel in that footprint is set to the new glyph (or cleared where the glyph is blank) in ONE masked
/// write per tile-row. Unlike `ui_text` (which ORs onto whatever is there — so re-drawing a shorter or
/// different value leaves ghost pixels) followed by a tile-aligned `ui_clear_rect` (which flashes black
/// AND wipes a neighbour line's pixels in a shared 8px tile), this updates a changing value IN PLACE:
/// - no ghosting: blank glyph pixels within the footprint clear the old value's pixels,
/// - no flicker: each row goes old→new in a single write, never through a cleared/black state,
/// - no chop: the footprint is a pixel-precise horizontal span at rows `[y, y+lineHeight)` only, so a
///   neighbour line sharing a boundary tile is untouched.
/// Pass `boxW` ≥ the widest value the field ever shows (e.g. the previous width) so a shrinking number
/// fully clears its old tail; the box is auto-grown to fit the current text if `boxW` is too small.
/// OPTIONAL `wrapW` (arg 6, >0) wraps the text at that width into MULTIPLE lines, and `boxH` (arg 7, >0)
/// is the opaque footprint HEIGHT — so a shrinking paragraph clears the lines it no longer fills (pass the
/// reserved box height). With `wrapW` the footprint width IS `wrapW`. Optional `align` (arg 8) matches
/// `ui_text` (`"left"`/`"center"`/`"right"`/…). Optional `bg` (arg 9, 0xRRGGBB): blank footprint pixels
/// write this colour instead of transparent — flash-free in-place updates on a filled panel (no
/// ui_rect wipe, no backdrop punch-holes). Requires a prior `ui_begin()`.
pub fn ui_text_box(args: &[Value]) -> Value {
    let font_handle = num(args, 0) as i32;
    let x = num(args, 1) as i32;
    let y = num(args, 2) as i32;
    // shape_text_lone, NOT shape_text — the same lone-character patch `ui_text` and `ui_text_span`
    // already carry. agb's `Layout` emits no letter group for a one-character string, so without this
    // `ui_text_box` painted its opaque footprint and NO GLYPH: every single-digit field drew a
    // correctly-sized blank rectangle. That is silent and it looks like a layout bug rather than a
    // missing glyph, which is why it survived three fix attempts in card-gba (deck tab numbers, a
    // card's power) before a pixel count showed the box present and the ink absent. Measured: "1"
    // through this call rendered 0 ink pixels in every combination of align/wrap/boxH/bg/font, while
    // "Cactuar" through the identical call rendered 24.
    let (text, only_first) = shape_text_lone(shape_text(
        args.get(3)
            .map(|v| v.to_display_string())
            .unwrap_or_default(),
    ));
    let color = if args.len() > 4 {
        num(args, 4) as i32
    } else {
        0xFFFFFF
    };
    let box_w_in = if args.len() > 5 {
        num(args, 5) as i32
    } else {
        0
    };
    let wrap_w = if args.len() > 6 {
        num(args, 6) as i32
    } else {
        0
    }; // >0 = wrap into multiple lines
    let box_h_in = if args.len() > 7 {
        num(args, 7) as i32
    } else {
        0
    }; // >0 = opaque footprint height
    let align = align_arg(args, 8);
    // Optional panel/button fill behind blank glyphs (0 / omitted = transparent clear, old behaviour).
    let bg_rgb = if args.len() > 9 {
        let v = &args[9];
        if matches!(v, Value::Null) {
            0
        } else {
            num(args, 9) as i32
        }
    } else {
        0
    };
    with_ctx(|ctx| {
        ui_feed_audio(ctx); // skipped while `audio_defer(1)` (batched list scroll refill)
        if ctx.ui_bg.is_none() {
            return;
        }
        let pal = ensure_ui_palette(ctx, color);
        let bg_pal = if bg_rgb != 0 {
            ensure_ui_palette(ctx, bg_rgb)
        } else {
            0
        };
        let font: &'static Font = if font_handle < 0 {
            &FONT
        } else {
            match tishlang_runtime_gba::gba::asset_font(font_handle) {
                Some(f) => f,
                None => return,
            }
        };
        let line_h = font.line_height();
        if line_h <= 0 {
            return;
        }
        // Wrap width, or (for centre/right single-line) the opaque box so agb can align within it.
        let max_line = if wrap_w > 0 {
            wrap_w
        } else if !matches!(align, AlignmentKind::Left | AlignmentKind::None) && box_w_in > 0 {
            box_w_in
        } else {
            4096
        };
        let settings = lone_settings(&layout_settings(max_line, align), only_first);
        // Single Layout pass: measure extents while compositing glyphs into the scratch buffer.
        // (Previously ran Layout twice — once for size, once to blit — doubling shaping cost on every
        // uiRowText / DET.patch / stepper.set.)
        let mut new_w: i32 = 0;
        let mut new_h: i32 = 0;
        // Upper bound for scratch before we know box_w: use wrap/box input or a generous line.
        let scratch_w = if wrap_w > 0 {
            wrap_w
        } else if box_w_in > 0 {
            box_w_in
        } else {
            240
        };
        let scratch_h = if box_h_in > 0 {
            box_h_in
        } else if wrap_w > 0 {
            // Multi-line: allow enough rows for a dense paragraph; trim after measure.
            line_h * 8
        } else {
            line_h
        };
        let tx0_est = x.div_euclid(8);
        let tx1_est = (x + scratch_w - 1).div_euclid(8);
        let ncols_est = (tx1_est - tx0_est + 1).max(1) as usize;
        let hh_est = scratch_h.max(line_h) as usize;
        // Taken out of the context so the blit loop below can borrow `ctx` for the tile writes; put back
        // at the end of the call, capacity intact (see `ui_box_scratch`).
        let mut buf = core::mem::take(&mut ctx.ui_box_scratch);
        let need = ncols_est * hh_est;
        if buf.len() < need {
            buf.resize(need, 0);
        }
        for slot in buf[..need].iter_mut() {
            *slot = 0;
        }
        for g in
            Layout::new(&text, font, &settings).take(if only_first { 1 } else { max_groups(&text) })
        {
            let gp = g.position();
            // A lone character was laid out DOUBLED (that is what gives the grouper something to
            // terminate on), so agb centred/right-aligned two glyphs and we draw one. Re-align on the
            // real glyph's own width, or a centred digit sits half an advance to the left of centre —
            // which on a 17px deck tab is the difference between centred and visibly off.
            let mut gx = gp.x;
            if only_first && box_w_in > 0 {
                let gw = g.bounds().x;
                if matches!(align, AlignmentKind::Right) {
                    gx = box_w_in - gw;
                } else if matches!(align, AlignmentKind::Centre) {
                    gx = (box_w_in - gw) / 2;
                }
                if gx < 0 {
                    gx = 0;
                }
            }
            for (px_start, px) in g.pixels_packed() {
                let ex = px_start.x + gx + 8;
                if ex > new_w {
                    new_w = ex;
                }
                let ey = px_start.y + gp.y + line_h;
                if ey > new_h {
                    new_h = ey;
                }
                let ax = px_start.x + gx + x;
                let ay_off = px_start.y + gp.y;
                if ay_off < 0 || ay_off >= scratch_h {
                    continue;
                }
                let colored = colorize_row(px, pal);
                let tx = ax.div_euclid(8);
                let xit = (ax.rem_euclid(8) * 4) as u32;
                let col = tx - tx0_est;
                if col >= 0 && (col as usize) < ncols_est {
                    let yi = ay_off as usize;
                    if yi < hh_est {
                        buf[(col as usize) * hh_est + yi] |= colored << xit;
                    }
                }
                if xit > 0 {
                    let col2 = col + 1;
                    if col2 >= 0 && (col2 as usize) < ncols_est {
                        let yi = ay_off as usize;
                        if yi < hh_est {
                            buf[(col2 as usize) * hh_est + yi] |= colored >> (32 - xit);
                        }
                    }
                }
            }
        }
        // When the caller passes box_w_in, THAT is the opaque footprint — do not grow it from
        // scratch extents. Packed-row measure uses `+ 8` per chunk (upper bound), which overshoots
        // the real glyph width and used to expand a right-aligned ON/OFF box into the button's
        // 1px border (pad is only ~3px), erasing the right edge on every toggle update.
        let box_w = if wrap_w > 0 {
            wrap_w
        } else if box_w_in > 0 {
            box_w_in
        } else if new_w > 0 {
            new_w
        } else {
            0
        };
        if box_w <= 0 {
            ctx.ui_box_scratch = buf;
            return;
        }
        // SINGLE-LINE (wrap_w == 0) is ALWAYS exactly line_h tall — never taller. `new_h` is only a
        // meaningful multi-line height when wrapping; for one line it over-measures (a single line's
        // pixels can extend past line_h in the raw metric), and an over-tall footprint clears the rows
        // BELOW the value (the next field / the gap) → the "flashing black" on a qty change. Only the
        // wrapped path (or an explicit box_h_in) may exceed one line.
        let box_h = if box_h_in > 0 {
            box_h_in
        } else if wrap_w > 0 && new_h > line_h {
            new_h
        } else {
            line_h
        };
        let tx0 = x.div_euclid(8);
        let tx1 = (x + box_w - 1).div_euclid(8);
        let ncols = (tx1 - tx0 + 1) as usize;
        let _hh = box_h as usize;
        // Write the scratch to VRAM, one masked replace per (tile-column, row).
        // buf was sized to scratch_w/tx0_est — columns match when box_w ≤ scratch_w (always for
        // wrap/box_w_in paths; content-sized grows box_w to new_w which was measured into scratch).
        let mut col = 0usize;
        while col < ncols {
            let tx = tx0 + col as i32;
            let tile_left = tx * 8;
            let l = (x.max(tile_left) - tile_left).clamp(0, 8);
            let r = ((x + box_w).min(tile_left + 8) - tile_left).clamp(0, 8);
            if r > l {
                let full: u32 = if r >= 8 {
                    0xFFFF_FFFF
                } else {
                    (1u32 << (r as u32 * 4)) - 1
                };
                let low: u32 = if l <= 0 {
                    0
                } else {
                    (1u32 << (l as u32 * 4)) - 1
                };
                let mask = full & !low;
                // Map this write column to the scratch column (same tx0 when scratch_w covered box).
                let src_col = (tx - tx0_est) as usize;
                // Walk the box in TILE-ROW runs (≤8 pixel rows) so each tile is resolved once, not
                // once per pixel row — the whole reason an in-place patch fits the selection budget.
                let mut row = 0i32;
                while row < box_h {
                    let ay = y + row;
                    let ty = ay.div_euclid(8);
                    let yi = ay.rem_euclid(8) as usize;
                    let run = (8 - yi as i32).min(box_h - row) as usize;
                    let mut vals = [0u32; 8];
                    for (k, slot) in vals.iter_mut().enumerate().take(run) {
                        let r = row as usize + k;
                        let mut val = 0u32;
                        if src_col < ncols_est && r < hh_est {
                            val = buf[src_col * hh_est + r];
                        }
                        *slot = fill_blank_nibbles(val, bg_pal, mask);
                    }
                    ui_masked_write_rows(ctx, tx, ty, yi, &vals[..run], mask);
                    row += run as i32;
                }
            }
            col += 1;
        }
        ctx.ui_box_scratch = buf;
    });
    Value::Null
}

/// `text_wrap_height(fontHandle, text, maxw) -> i32` — the pixel HEIGHT `text` occupies when wrapped to
/// `maxw` px wide (number of lines × the font's line height). Lets the layout engine size a wrapping
/// paragraph. Measures only (no draw, no VRAM). Uses baked [`FontMetrics`] advances when available
/// (same wrap rules as agb left-align); falls back to `Layout` for the built-in font (handle -1).
pub fn text_wrap_height(args: &[Value]) -> Value {
    let font_handle = num(args, 0) as i32;
    let text = args
        .get(1)
        .map(|v| v.to_display_string())
        .unwrap_or_default();
    let maxw = if num(args, 2) as i32 > 0 {
        num(args, 2) as i32
    } else {
        512
    };
    if let Some(m) = font_metrics::font_metrics(font_handle) {
        return Value::Number(m.wrap_height(&text, maxw) as f64);
    }
    let font: &'static Font = if font_handle < 0 {
        &FONT
    } else {
        match tishlang_runtime_gba::gba::asset_font(font_handle) {
            Some(f) => f,
            None => return Value::Number(12.0),
        }
    };
    let mut lines = 0;
    for g in Layout::new(
        &text,
        font,
        &LayoutSettings::new().with_max_line_length(maxw),
    ) {
        let l = g.line() + 1;
        if l > lines {
            lines = l;
        }
    }
    if lines < 1 {
        lines = 1;
    }
    Value::Number((lines * font.line_height()) as f64)
}

/// Set a single UI-canvas pixel to colour index `idx` at absolute `(ax, ay)`.
fn ui_set_pixel(ctx: &mut GbaCtx, ax: i32, ay: i32, idx: u8) {
    let tx = ax.div_euclid(8);
    let ty = ay.div_euclid(8);
    let nib = (ax.rem_euclid(8) * 4) as u32;
    let yi = ay.rem_euclid(8) as usize;
    ui_blit_cell(ctx, tx, ty, yi, (idx as u32) << nib);
}

/// `ui_rect(x, y, w, h, color, filled?)` — draw a rectangle on the UI canvas: a 1px OUTLINE (borders
/// for panels), or a solid block when `filled` is non-zero (panel backgrounds, scrollbar thumbs).
///
/// Large fills snap IN to 8×8 tiles and share ONE DynamicTile (avoids OOM on full-width dialogs,
/// and never spill into title/footer chrome). Small fills (`h≤48`) are pixel-precise so compact
/// chrome (menu buttons / HP bars) doesn't bleed past its border. Requires a prior `ui_begin()`.
/// Keep dialog content off border tiles with `pad` so single-palette tiles don't force border + text
/// to the same colour.
pub fn ui_rect(args: &[Value]) -> Value {
    let x = num(args, 0) as i32;
    let y = num(args, 1) as i32;
    let w = num(args, 2) as i32;
    let h = num(args, 3) as i32;
    let color = num(args, 4) as i32;
    let filled = args.len() > 5 && num(args, 5) != 0.0;
    ui_rect_inner(x, y, w, h, color, filled);
    Value::Null
}

/// The SIX-ARGUMENT form, reached directly from a `declare fn` in tish.d.tish rather than through the
/// `&[Value]` slice — see the note beside that declaration.
///
/// Every extern argument is boxed into a `Value::Number` at the call site, and ui_rect is the most
/// called native in a card-gba frame: paintCell alone boxes 147 numbers, cardBody 36, cardRim 29,
/// drawGrid5 24, across 254 six-arg call sites. Typed lowering matches on exact arity, so the 5-arg
/// outline form inside packages/ui.tish is untouched and keeps the boxed path.
/// #615: with BOTH arities declared, the compiler dispatches on exact arity to an arity-suffixed
/// symbol, so each form gets its own entry point. A name declaring a single arity still links as
/// plain `<name>_typed` — the suffix appears only where overloads exist.
pub fn ui_rect_typed_6(x: i32, y: i32, w: i32, h: i32, color: i32, filled: i32) {
    ui_rect_inner(x, y, w, h, color, filled != 0);
}

/// The same six-argument form under the name the compiler in this tree actually emits.
///
/// Typed lowering names a declared extern `<name>_typed`, with no arity suffix — so the single
/// `declare fn ui_rect(x, y, w, h, color, filled)` in `tish.d.tish` resolves here, and
/// `ui_rect_typed_6` above is left for whatever still calls it by that name. Without this, every
/// site that draws a box fails to link: 24 E0425s in the topdown RPG port alone, and `ui_rect` has 55 six-arg
/// call sites across packages/ui, shop, menu, feel and drop.
///
/// This is only unambiguous because ONE arity is declared. Declaring the five-argument form as well
/// lowers both to this same name and the mismatch comes straight back — that is the state this
/// replaced. Restoring the 5-arg declaration needs the compiler to emit the suffixed symbols first.
pub fn ui_rect_typed(x: i32, y: i32, w: i32, h: i32, color: i32, filled: i32) {
    ui_rect_inner(x, y, w, h, color, filled != 0);
}

/// The FIVE-argument outline form used throughout packages/ui.tish. It matches the boxed path's
/// behaviour for a missing 6th argument exactly: `args.len() > 5 && ..` is false there, so the
/// outline (unfilled) rect is what a 5-arg call has always drawn.
pub fn ui_rect_typed_5(x: i32, y: i32, w: i32, h: i32, color: i32) {
    ui_rect_inner(x, y, w, h, color, false);
}

fn ui_rect_inner(x: i32, y: i32, w: i32, h: i32, color: i32, filled: bool) {
    if w <= 0 || h <= 0 {
        return;
    }
    with_ctx(|ctx| {
        ui_feed_audio(ctx); // skipped while `audio_defer(1)` (batched list scroll refill)
        if ctx.ui_bg.is_none() {
            return;
        }
        let pal = ensure_ui_palette(ctx, color);
        if filled {
            // Compact chrome (buttons/chips/bars): exact pixels. Gate on HEIGHT only — a wide short
            // row (toggle at w:180, HP track at w:140) used to take the tile-snap path, spill into
            // neighbour tiles, and corrupt title/tabs when a later body clear touched those cells.
            if h <= 48 {
                // Wide short fills (toggle plates, HP tracks): whole tile-row spans, not per-pixel.
                ui_fill_span(ctx, x, y, w, h, pal);
                return;
            }
            // Snap IN to tile boundaries (never spill into title/footer/neighbours). A per-cell
            // allocate for a 240×N dialog OOMs the GBA heap — shared solid tile stays. If the inset
            // is empty (panel shorter than one tile), fall back to exact pixels.
            let x0 = ((x + 7).div_euclid(8)) * 8;
            let y0 = ((y + 7).div_euclid(8)) * 8;
            let x1 = (x + w).div_euclid(8) * 8;
            let y1 = (y + h).div_euclid(8) * 8;
            if x1 <= x0 || y1 <= y0 {
                let mut j = 0;
                while j < h {
                    let mut i = 0;
                    while i < w {
                        ui_set_pixel(ctx, x + i, y + j, pal);
                        i += 1;
                    }
                    j += 1;
                }
                return;
            }
            ui_ensure_solid(ctx, pal);
            let si = (pal as usize) % UI_SOLID_N;
            let mark = UI_CELL_SOLID_LO + (si as u16);
            let tx0 = x0 / 8;
            let ty0 = y0 / 8;
            let tx1 = x1 / 8 - 1;
            let ty1 = y1 / 8 - 1;
            let mut ty = ty0;
            while ty <= ty1 {
                let mut tx = tx0;
                while tx <= tx1 {
                    let idx = ui_cell_idx(tx, ty);
                    ui_drop_tile(ctx, idx);
                    if let (Some(bg), Some(solid)) =
                        (ctx.ui_bg.as_mut(), ctx.ui_solids[si].as_ref())
                    {
                        bg.set_tile_dynamic16(
                            Vector2D::new(tx, ty),
                            solid,
                            TileEffect::default().palette(UI_PAL_SLOT),
                        );
                    }
                    if let Some(slot) = ctx.ui_cell.get_mut(idx) {
                        *slot = mark;
                    }
                    tx += 1;
                }
                ty += 1;
            }
            return;
        }
        // Border: four 1px spans. Per-pixel would cost a tile resolution per pixel, and a panel
        // border is the single most common thing on a menu screen.
        ui_fill_span(ctx, x, y, w, 1, pal);
        if h > 1 {
            ui_fill_span(ctx, x, y + h - 1, w, 1, pal);
        }
        ui_fill_span(ctx, x, y, 1, h, pal);
        if w > 1 {
            ui_fill_span(ctx, x + w - 1, y, 1, h, pal);
        }
    });
}

/// `ui_clear_rect(x, y, w, h)` — blank every UI-canvas TILE overlapping the region (revert it to the
/// backdrop), erasing text behind a modal/overlay. Whole 8×8 tiles are cleared, so leave a tile of
/// margin around content you want to keep. Requires a prior `ui_begin()`.
pub fn ui_clear_rect(args: &[Value]) -> Value {
    let x = num(args, 0) as i32;
    let y = num(args, 1) as i32;
    let w = num(args, 2) as i32;
    let h = num(args, 3) as i32;
    if w <= 0 || h <= 0 {
        return Value::Null;
    }
    with_ctx(|ctx| {
        ui_feed_audio(ctx); // skipped while `audio_defer(1)` (batched list scroll refill)
        if ctx.ui_bg.is_none() {
            return;
        }
        let tx0 = x.div_euclid(8);
        let ty0 = y.div_euclid(8);
        let tx1 = (x + w - 1).div_euclid(8);
        let ty1 = (y + h - 1).div_euclid(8);
        let mut ty = ty0;
        while ty <= ty1 {
            let mut tx = tx0;
            while tx <= tx1 {
                // Blank any existing tile's pixels (→ backdrop); positions with no tile are already
                // backdrop. Reuse — never allocate here (that would churn the tile pool).
                let idx = ui_cell_idx(tx, ty);
                if let Some(tile) = ui_tile_at(ctx, idx) {
                    let d = tile.data_mut();
                    let mut k = 0;
                    while k < d.len() {
                        d[k] = 0;
                        k += 1;
                    }
                } else if ui_cell_solid_pal(ctx.ui_cell.get(idx).copied().unwrap_or(UI_CELL_EMPTY))
                    .is_some()
                {
                    // ⚠️ A SHARED-SOLID CELL HAS NO TILE OF ITS OWN, so the branch above skipped it
                    // and the clear silently did nothing. That is not a curtain-only problem: it is
                    // every large `ui_rect` fill, which takes the same shared-tile path — clearing
                    // over one has always been a no-op. It surfaced here because a transition
                    // curtain is the first thing that MUST come back off, and a screen that never
                    // un-blacks reads exactly like a hang.
                    //
                    // Point the cell at a shared transparent tile and mark it empty. The blank is
                    // shared like the solids are, so this allocates one tile for the whole screen
                    // and none per clear.
                    ui_ensure_blank(ctx);
                    if let (Some(bg), Some(blank)) = (ctx.ui_bg.as_mut(), ctx.ui_blank.as_ref()) {
                        bg.set_tile_dynamic16(
                            Vector2D::new(tx, ty),
                            blank,
                            TileEffect::default().palette(UI_PAL_SLOT),
                        );
                    }
                    if let Some(slot) = ctx.ui_cell.get_mut(idx) {
                        *slot = UI_CELL_EMPTY;
                    }
                }
                tx += 1;
            }
            ty += 1;
        }
    });
    Value::Null
}

/// `text_width(fontHandle, text) -> i32` — the pixel width of `text` in a font, measured WITHOUT
/// drawing. Powers ellipsis + flex sizing in the layout engine. Prefer baked [`FontMetrics`]
/// advance sums (O(chars), no agb `Layout`); fall back to Layout + memo for the built-in font.
pub fn text_width(args: &[Value]) -> Value {
    let font_handle = num(args, 0) as i32;
    let text = args
        .get(1)
        .map(|v| v.to_display_string())
        .unwrap_or_default();
    if let Some(m) = font_metrics::font_metrics(font_handle) {
        return Value::Number(m.width(&text) as f64);
    }
    let font: &'static Font = if font_handle < 0 {
        &FONT
    } else {
        match tishlang_runtime_gba::gba::asset_font(font_handle) {
            Some(f) => f,
            None => return Value::Number(0.0),
        }
    };
    // Memoised Layout path for the built-in FONT (no parallel metrics table).
    let w = with_ctx(|ctx| {
        pump_audio(ctx);
        if let Some(cached) = ctx.tw_cache.get(&(font_handle, text.clone())) {
            return *cached;
        }
        let mut w = 0;
        for g in Layout::new(&text, font, &LayoutSettings::new().with_max_line_length(0)) {
            let right = g.position().x + g.bounds().x;
            if right > w {
                w = right;
            }
        }
        if ctx.tw_cache.len() >= TW_CACHE_MAX {
            ctx.tw_cache.clear();
        }
        ctx.tw_cache.insert((font_handle, text.clone()), w);
        w
    });
    Value::Number(w as f64)
}

/// `text_height(fontHandle) -> i32` — the line height (px) of a font, so the layout engine can size a
/// row to whatever font/size it uses (fonts are baked per pixel-size via `font:path@N`).
pub fn text_height(args: &[Value]) -> Value {
    let font_handle = num(args, 0) as i32;
    if let Some(m) = font_metrics::font_metrics(font_handle) {
        return Value::Number(m.line_height as f64);
    }
    let font: &'static Font = if font_handle < 0 {
        &FONT
    } else {
        match tishlang_runtime_gba::gba::asset_font(font_handle) {
            Some(f) => f,
            None => return Value::Number(12.0),
        }
    };
    Value::Number(font.line_height() as f64)
}

/// `backdrop(color)` — set the screen's backdrop (the colour shown where no background or sprite
/// covers), as 0xRRGGBB. Without this the backdrop is undefined (often white). Persists once set.
/// Disarm the per-scanline backdrop ramp. A hidden board must not keep painting its sky.
pub fn sky_clear(_args: &[Value]) -> Value {
    native_sky_set(&[]);
    Value::Null
}

pub fn backdrop(args: &[Value]) -> Value {
    let v = num(args, 0) as i32 as u32;
    let c = Rgb::new(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
    )
    .to_rgb15();
    with_ctx(|ctx| ctx.gfx.set_background_palette_colour(0, 0, c));
    Value::Null
}

/// `sprite_set_visible(handle, visible)` — show (non-zero) or hide (0) a sprite.
pub fn sprite_set_visible(args: &[Value]) -> Value {
    native_sprite_set_visible(num(args, 0) as i32, num(args, 1) != 0.0);
    Value::Null
}

/// `bg_new(assetHandle)` — create a full-screen tiled background from a registered
/// `background:` import; returns its handle. Sets the background palettes, builds a
/// `RegularBackground`, and fills it with the asset's tiles. Drawn behind sprites.
/// Set the GBA backdrop — BG palette 0, entry 0, shown wherever no tile covers the pixel.
///
/// `include_background_gfx!` leaves agb's transparent SENTINEL in that slot: bright magenta,
/// `Colour::from_rgb(255, 0, 255, 0)` in agb-image-converter. That is a marker, not a colour anyone
/// wants on screen, so it becomes black and off-map voids read as void.
///
/// ⚠️ ONLY the sentinel. A background that carries the hardware's own palette means entry 0
/// literally — SRPG battle boards put the SKY there — and blacking it out replaces the sky with a
/// void. That one is nasty to spot: the board itself is pixel-perfect, so the screen looks like a
/// correct map on a broken background rather than like a palette bug.
fn set_backdrop(ctx: &mut GbaCtx, palettes: &'static [agb::display::Palette16]) {
    // Remember what was just uploaded, so `bg_pal_get` can answer "which index holds this colour?".
    // Every upload path funnels through here, so this is the one place that sees them all.
    ctx.bg_pal = Some(palettes);
    const AGB_TRANSPARENT: Rgb15 = Rgb15(0x7C1F); // 5-bit (31, 0, 31)
    if palettes.first().map(|p| p.colour(0)) == Some(AGB_TRANSPARENT) {
        ctx.gfx.set_background_palette_colour(0, 0, Rgb15::BLACK);
    }
}

pub fn bg_new(args: &[Value]) -> Value {
    let asset = num(args, 0) as i32;
    // Optional priority 0..3 (DEFAULT 3 = furthest back when the arg is absent, so every existing
    // single-arg `bg_new(sheet)` caller keeps drawing behind sprites). Lets a game stack layers for
    // parallax — a far starfield at 3, a nearer one at 2 — with the nearer drawn IN FRONT; a bg pixel
    // of palette index 0 is transparent on hardware, so the front layer shows the far one through it.
    let pri = match if args.len() > 1 {
        num(args, 1) as i32
    } else {
        3
    } {
        0 => Priority::P0,
        1 => Priority::P1,
        2 => Priority::P2,
        _ => Priority::P3,
    };
    let (palettes, data) = tishlang_runtime_gba::gba::asset_bg(asset)
        .expect("tish-agb: bg_new called with an unknown background handle");
    make_bg(palettes, data, pri, asset)
}

// ── the FOREGROUND registry ─────────────────────────────────────────────────────────────────────
//
// A second table, here rather than in the shared runtime, and the reason is arithmetic: an SRPG
// battlefield is TWO backgrounds, so a game carrying all 162 boards needs 324 registrations against
// the runtime's `MAX_BGS = 256`. Past the cap `__asset_register_bg` hands back -1 and `bg_new`
// panics — loudly, but only once you reach the 129th board.
//
// Splitting the foregrounds into their own 256-entry table doubles the ceiling without touching the
// shared runtime, and it is the honest shape anyway: these are a distinct kind of asset that only
// `isoboard:` boards produce.
const MAX_FG: usize = 256;
type BgAsset = (
    &'static [agb::display::Palette16],
    &'static agb::display::tile_data::TileData,
);
static FG_ASSETS: SingleCore<RefCell<[Option<BgAsset>; MAX_FG]>> =
    SingleCore::new(RefCell::new([None; MAX_FG]));
static FG_N: SingleCore<RefCell<usize>> = SingleCore::new(RefCell::new(0));

/// Register a board's foreground layer, returning its handle (or -1 when the table is full).
pub fn native_fg_register(bg: BgAsset) -> i32 {
    FG_N.with(|n| {
        let mut n = n.borrow_mut();
        if *n >= MAX_FG {
            return -1;
        }
        let idx = *n as i32;
        FG_ASSETS.with(|c| c.borrow_mut()[*n] = Some(bg));
        *n += 1;
        idx
    })
}

/// `bg_new_fg(handle, priority)` — create a background from the FOREGROUND registry.
///
/// Same as `bg_new` but reading the other table. Priority is required rather than defaulted: a
/// foreground exists to draw in front of the units, which only happens below the sprite priority.
pub fn bg_new_fg(args: &[Value]) -> Value {
    let asset = num(args, 0) as i32;
    let pri = match num(args, 1) as i32 {
        0 => Priority::P0,
        1 => Priority::P1,
        2 => Priority::P2,
        _ => Priority::P3,
    };
    let Some((palettes, data)) = (if asset < 0 || asset as usize >= MAX_FG {
        None
    } else {
        FG_ASSETS.with(|c| c.borrow()[asset as usize])
    }) else {
        return Value::Number(-1.0);
    };
    make_bg(palettes, data, pri, -1)
}

/// Build a `RegularBackground` from palettes + tile data and park it in the context.
fn make_bg(
    palettes: &'static [agb::display::Palette16],
    data: &'static agb::display::tile_data::TileData,
    pri: Priority,
    asset: i32,
) -> Value {
    with_ctx(|ctx| {
        ctx.gfx.set_background_palettes(palettes);
        set_backdrop(ctx, palettes);
        // Follow the baked data's colour depth: 4bpp for ordinary backgrounds, 8bpp (256-colour) for a
        // rich board like the iso battle floor (`isoboard:` bakes it at 256 so its tileset isn't crushed
        // into one 16-colour palette).
        // Boards larger than 256×256 (e.g. a scrolling iso town) need a 64×64 map; smaller stay 32×32.
        let tw = data.width.max(1);
        let th = data.height.max(1);
        // ⚠️ Size each axis SEPARATELY. Rounding a 64x32 source up to Background64x64 costs two
        // extra screenblocks (4 KB of VRAM) to hold a vertical repeat the hardware would give for
        // free by wrapping — and an SRPG board is two 64x32 layers, so the waste doubles.
        let bg_size = match (tw > 32, th > 32) {
            (true, true) => RegularBackgroundSize::Background64x64,
            (true, false) => RegularBackgroundSize::Background64x32,
            (false, true) => RegularBackgroundSize::Background32x64,
            (false, false) => RegularBackgroundSize::Background32x32,
        };
        let map_w = if tw > 32 { 64i32 } else { 32i32 };
        let map_h = if th > 32 { 64i32 } else { 32i32 };
        let mut bg = RegularBackground::new(pri, bg_size, data.tiles.format());
        // Fill the ENTIRE map, not just the visible screen. agb's `fill_with` only populates the
        // top-left 30x20 tile window (a static full-screen background) — the remaining border stays
        // transparent and shows the black backdrop. That's invisible for a fixed screen but a
        // *scrolling* background wraps that unfilled border into view as a hard seam. Tile the source
        // across the whole map (wrapping by its tile dimensions) so it scrolls seamlessly at any
        // offset; a matching source maps 1:1, a smaller one repeats. One-time cost at load; deduped
        // tiles just bump a VRAM refcount.
        for y in 0..map_h {
            for x in 0..map_w {
                let tx = (x as usize) % tw;
                let ty = (y as usize) % th;
                bg.set_tile(
                    Vector2D::new(x, y),
                    &data.tiles,
                    data.tile_settings[ty * tw + tx],
                );
            }
        }
        let handle = ctx.backgrounds.len();
        ctx.backgrounds.push(BgData {
            bg,
            visible: true,
            asset,
            parallax: None,
            bands: None,
        });
        Value::Number(handle as f64)
    })
}

// ── The scanline table, and why it is armed from an interrupt ────────────────────────────────────
//
// The table lives in a static and DMA0 is pointed at it from a VBlank handler, rather than going
// through agb's `HBlankDma`. That is not an optimisation, it is a correctness fix.
//
// `GraphicsFrame::commit` runs: wait for vblank, commit objects, commit backgrounds, blend, windows,
// and THEN arm the DMA. So with `HBlankDma` the question "is the floor correct this frame" is really
// "did everything else finish before the display started drawing" — and the answer is sometimes yes
// and sometimes no. A frame that runs long arms the table late, the transforms start applying
// part-way down the screen, and the picture rolls like a mistuned CRT. It is not a budget you can
// stay inside either: a modest amount of music and HUD was already enough to break it, and the
// software mixer alone took it from occasional to constant.
//
// Armed from the VBlank interrupt, the table is in place before any of the frame's work happens, so
// lateness stops being representable. agb never touches DMA0 as long as nothing calls
// `frame.add_dma` — its commit only disturbs the DMA when it holds one — so the two do not fight.
//
// HBlank DMA does not fire during VBlank, so the first transfer of a frame lands in the HBlank after
// scanline 0 and applies to scanline 1. Row 0 is therefore never delivered by DMA; the layer's own
// transform covers it, which is why `set_transform(rows[0])` still matters.
const M7_LINES: usize = 160;

const M7_ZERO: AffineMatrixBackground = AffineMatrixBackground {
    a: Num::from_raw(0),
    b: Num::from_raw(0),
    c: Num::from_raw(0),
    d: Num::from_raw(0),
    x: Num::from_raw(0),
    y: Num::from_raw(0),
};

// ⚠️ Raw pointers only, never two references at once.
//
// The first attempt at this held the table in an `UnsafeCell` and, in one frame, took `&mut` to fill
// it and `&` to hand to the DMA. That is an aliasing violation, and the compiler is entitled to
// assume it cannot happen: the fill was elided and the floor stopped rendering entirely. Nothing
// warned. Writing through `addr_of_mut!` and only ever materialising an address keeps it honest.
// 228 entries for 160 scanlines, and the extra 68 are not slack — they are the fix for a one-line
// flicker on scanline 0.
//
// An HBlank DMA with `repeat` fires every HBlank forever, including the ~68 during vblank, and the
// source keeps advancing. Once past the visible rows it reads whatever follows in memory and writes
// it into BG2PA..BG2Y — AFTER agb's commit has set the transform for the coming frame. So if the
// re-arm interrupt is even slightly late, scanline 0 draws with garbage. Padding the table to a full
// display period with copies of the sky row makes the runaway harmless: it can only ever latch sky.
const M7_TOTAL: usize = 228;
static mut M7_ROWS: [AffineMatrixBackground; M7_TOTAL] = [M7_ZERO; M7_TOTAL];

struct M7Flag(Cell<bool>);
unsafe impl Sync for M7Flag {}
// Whether there is a table worth arming. `Cell`, not an atomic: thumbv4t has no atomics, and this
// is a single core with one writer.
static M7_ARMED: M7Flag = M7Flag(Cell::new(false));
static M7_IRQ_READY: M7Flag = M7Flag(Cell::new(false));

const DMA0_SAD: *mut u32 = 0x0400_00B0 as *mut u32;
const DMA0_DAD: *mut u32 = 0x0400_00B4 as *mut u32;
const DMA0_CNT: *mut u32 = 0x0400_00B8 as *mut u32;
const BG2_TRANSFORM: u32 = 0x0400_0020;

// enable | HBlank timing | 32-bit units | repeat | destination increment-and-reload.
// The reload is what makes it write the same sixteen bytes every scanline instead of walking away
// down the register file.
// enable | HBlank timing | repeat | destination increment-and-reload, in HALFWORDS (bit 26
// clear) — the same shape agb uses, eight of them making up the sixteen-byte matrix.
const DMA0_MODE7: u32 = (1 << 31) | (2 << 28) | (1 << 25) | (3 << 21);

/// Register the VBlank handler that arms DMA0, once.
/// Arm on a VCOUNT match at the LAST scanline, not on VBlank.
///
/// HBlank DMA fires during vblank as well as during the visible frame, so every HBlank between
/// arming and scanline 0 eats one row of the table. Arm at the start of vblank and 67 rows are gone
/// before the screen starts, which draws the floor shifted a third of the way up. That is also what
/// agb's own timing does in miniature: it arms partway through `commit()`, so the number of vblank
/// HBlanks left over — and therefore the size of the shift — depends on how long the frame's work
/// took. That is the flickering band, and it is why moving the arm to vblank START made it worse
/// and constant rather than better.
///
/// Arming on the last scanline leaves no vblank HBlanks at all, so row 1 lands on scanline 1
/// whatever the game is doing.
const VCOUNT_ARM: u16 = 227; // the last scanline; its own HBlank delivers row 0 to line 0
const REG_DISPSTAT: *mut u16 = 0x0400_0004 as *mut u16;
const REG_VCOUNT: *const u16 = 0x0400_0006 as *const u16;

fn m7_install_irq() {
    if M7_IRQ_READY.0.get() {
        return;
    }
    M7_IRQ_READY.0.set(true);
    // Point the VCOUNT comparator at the last scanline and enable its interrupt (DISPSTAT bit 5,
    // trigger value in the high byte).
    unsafe {
        let cur = REG_DISPSTAT.read_volatile();
        REG_DISPSTAT.write_volatile((cur & 0x00FF) | (1 << 5) | (VCOUNT_ARM << 8));
    }
    // SAFETY: the handler only writes DMA0's registers and reads a table this module owns. It
    // allocates nothing and takes no locks, which is what an interrupt handler has to promise.
    let h = unsafe {
        agb::interrupt::add_interrupt_handler(agb::interrupt::Interrupt::VCounter, |_| m7_arm_dma())
    };
    // ⚠️ The DMA is deliberately NOT stopped at vblank.
    //
    // agb writes the affine layer's OWN transform inside `bg_frame.commit()`, and there is no way to
    // stop it from outside the crate. Whatever that value is, it lands wherever the beam happens to
    // be — and with the channel switched off through vblank, nothing corrected it, so it survived
    // onto the screen as a full-width wrong line at a random height. (Setting the layer transform to
    // a sky row made those lines holes; setting it to a floor row made them red-and-tan. Changing
    // the value changed the artefact, which is how it was identified.)
    //
    // Left running, the very next HBlank overwrites agb's value with the row that line is supposed
    // to have, so the write is corrected before it can be seen. That is what the table's 228 entries
    // are for: 160 visible rows plus a vblank tail, so the channel has valid data to deliver for the
    // whole display period and the source lands back on row 0 exactly as VCOUNT reaches 227.
    // Deliberately leaked: the guard deregisters the handler when dropped, and this one has to live
    // as long as the game does. There is exactly one, installed once.
    core::mem::forget(h);
}

// ── The sky gradient ─────────────────────────────────────────────────────────────────────────────
//
// The GBA backdrop — BG palette entry 0, shown wherever no layer covers a pixel — is ONE colour for
// the whole screen. An imported battle sky is not: it runs a smooth vertical ramp, 5-bit `(1,9,31)` at
// the top to `(12,30,31)` at the bottom, 33 distinct shades. The game gets there by rewriting that
// single palette word every scanline from a 160-entry table (found at EWRAM `0x02008390`, and it
// agrees with the rendered screen on all 80 scanlines where sky is visible).
//
// So this is the same trick as the Mode 7 matrix, one halfword wide: HBlank DMA, source walking a
// table, destination FIXED on the backdrop word.
//
// ⚠️ Shares DMA0 with Mode 7 for the reasons the block above gives — DMA1/2 belong to agb's direct
// sound and DMA3 to its memory copies, so DMA0 is the only channel free to hold an HBlank table.
const BG_PALETTE_0: u32 = 0x0500_0000;
const SKY_LINES: usize = 160;

// enable | HBlank timing | 16-bit units | repeat | destination FIXED.
//
// ⚠️ Destination FIXED (2 << 21), not increment-and-reload as Mode 7 uses. There is one word to
// write, not a sixteen-byte matrix; letting the destination walk would march the gradient across
// BG0's scroll registers instead. The SOURCE increments, which is what steps the table.
const DMA0_SKY: u32 = (1 << 31) | (2 << 28) | (1 << 25) | (2 << 21);

static mut SKY_ROWS: [u16; SKY_LINES] = [0; SKY_LINES];
static SKY_ARMED: M7Flag = M7Flag(Cell::new(false));

/// Install a per-scanline backdrop ramp, or clear it when `table` is empty.
///
/// `table` is one BGR555 colour per visible scanline. Anything shorter is padded by repeating its
/// last entry, so a 2-entry ramp is a legal (if coarse) sky.
pub fn native_sky_set(table: &'static [u16]) {
    if table.is_empty() {
        SKY_ARMED.0.set(false);
        return;
    }
    unsafe {
        let dst = core::ptr::addr_of_mut!(SKY_ROWS) as *mut u16;
        let mut i = 0usize;
        while i < SKY_LINES {
            let v = table[if i < table.len() { i } else { table.len() - 1 }];
            dst.add(i).write_volatile(v);
            i += 1;
        }
    }
    SKY_ARMED.0.set(true);
    m7_install_irq();
}

/// Point DMA0 at the sky table. Returns whether it took the channel.
fn sky_arm_dma() -> bool {
    if !SKY_ARMED.0.get() {
        return false;
    }
    unsafe {
        // Line 0 by hand, then source from row 1 — a transfer in line N's HBlank lands on line N+1,
        // so sourcing from row 0 would give every line its predecessor's colour and leave line 0
        // showing whatever was latched last frame. Same correction the Mode 7 path documents.
        let rows = core::ptr::addr_of!(SKY_ROWS) as *const u16;
        (BG_PALETTE_0 as *mut u16).write_volatile(rows.read_volatile());
        DMA0_CNT.write_volatile(0);
        DMA0_SAD.write_volatile(rows.add(1) as u32);
        DMA0_DAD.write_volatile(BG_PALETTE_0);
        DMA0_CNT.write_volatile(DMA0_SKY | 1); // one halfword per HBlank
    }
    true
}

fn m7_arm_dma() {
    // ⚠️ Only act at the end of vblank, where this handler is SUPPOSED to run.
    //
    // The body deposits the sky matrix into BG2PA..BG2Y (see below). That is exactly right one line
    // before the screen starts, and catastrophic anywhere else: run it mid-frame and the scanline the
    // beam is on renders as sky — a full-width backdrop-coloured line across the floor, at a random
    // height. That is the stray line, and the giveaway was that the floor WRAPS, so there is no
    // off-map left to blame: a whole line of backdrop can only come from sampling the sky texel, and
    // only the deposit writes that outside the table.
    //
    // Checking VCOUNT costs one register read and makes a mistimed or spurious call a no-op instead
    // of a visible defect.
    let vc = unsafe { REG_VCOUNT.read_volatile() } & 0x00FF;
    if vc < VCOUNT_ARM {
        return;
    }
    if !M7_ARMED.0.get() {
        // DMA0 is free, so the sky gradient may have it. Mode 7 wins when both are on: they are the
        // same channel, and a game cannot want a perspective floor and a gradient backdrop at once.
        if sky_arm_dma() {
            return;
        }
        unsafe { DMA0_CNT.write_volatile(0) };
        return;
    }
    // Armed during scanline 227, so that line's own HBlank delivers row 0 — which is the sky row —
    // to scanline 0, and line 0's HBlank delivers row 1 to line 1. The indices line up with no
    // fudge, which is the point of arming here rather than at the start of vblank where 68 further
    // HBlanks would each eat a row before the screen even started.
    // Source is row 1, not row 0. The transfer in line N's HBlank applies to line N+1, so sourcing
    // from row 0 hands every line its PREDECESSOR's matrix. That is invisible across the floor,
    // where neighbouring rows barely differ — and glaring at the sky/floor boundary, where it shows
    // as one extra sky-coloured line pinned under the horizon on every frame. Line 0 is covered
    // separately by the deposit below.
    let src =
        unsafe { (core::ptr::addr_of!(M7_ROWS) as *const AffineMatrixBackground).add(1) } as u32;
    // Write row 0 into the registers by hand before enabling the channel.
    //
    // Scanline 0 is the one line the table cannot reach: the DMA's first transfer of a frame lands
    // in an HBlank, and every HBlank is AFTER the line it follows. So line 0 draws with whatever was
    // latched — and empirically that was a floor matrix left over from somewhere, painting a stripe
    // of the dojo mat across the top of the screen on most frames. Rows 0..horizon are all sky, so
    // depositing row 0 here makes line 0 agree with the table by construction instead of by luck.
    unsafe {
        let row0 = core::ptr::addr_of!(M7_ROWS) as *const u16;
        let dst = BG2_TRANSFORM as *mut u16;
        let mut i = 0usize;
        while i < 8 {
            dst.add(i).write_volatile(row0.add(i).read_volatile());
            i += 1;
        }
    }
    unsafe {
        DMA0_CNT.write_volatile(0);
        DMA0_SAD.write_volatile(src);
        DMA0_DAD.write_volatile(BG2_TRANSFORM);
        DMA0_CNT.write_volatile(DMA0_MODE7 | 8); // 8 halfwords = the 16-byte matrix
    }
}

/// The depth column: `k = height / dy` for every scanline, and the camera it was built for.
///
/// ⚠️ This is the whole reason the row build stopped being the most expensive thing in the frame.
/// `k` depends on the camera's HEIGHT and HORIZON and nothing else — not on yaw, not on where the
/// camera stands. A camera that orbits, or walks, or turns does not change one entry. But it was
/// being recomputed every frame, and each entry is a fixed-point division: ARM7TDMI has no divide
/// instruction, so all 160 were software routines, ~40 cycles apiece, 60 times a second, to arrive
/// at the same 160 numbers.
///
/// Cached, a moving camera costs four multiplies per scanline and no divisions at all. The cache is
/// keyed on the two inputs that matter, so a game that DOES change height (mode7-demo's A/B) simply
/// rebuilds the column on the frames it changes and keeps it on the frames it doesn't.
static mut M7_DEPTH: [Num<i32, 8>; M7_LINES] = [Num::from_raw(0); M7_LINES];
struct M7Key(Cell<i32>);
unsafe impl Sync for M7Key {}
static M7_DEPTH_KEY: M7Key = M7Key(Cell::new(i32::MIN));

/// Rebuild the depth column if this camera's height or horizon differs from the one it holds.
fn m7_depth_column(m: &Mode7) {
    // Both inputs in one key. `height` is 8.8 and realistically under 512, `horizon` a scanline.
    let key = (m.height.to_raw() << 9) ^ m.horizon;
    if M7_DEPTH_KEY.0.get() == key {
        return;
    }
    M7_DEPTH_KEY.0.set(key);
    // The clamp is the horizon's whole story. `k = height / dy` diverges as dy -> 1, so the first
    // row under the horizon is at infinite distance: PA grows past what an 8.8 fixed point holds,
    // and the row samples texels hundreds apart, which resolves to noise rather than ground. 48 left
    // one visibly wrong line pinned to the horizon on every frame; 12 caps the view at a distance
    // the texture still reads as a surface.
    let kmax: Num<i32, 8> = Num::new(12);
    let dst = core::ptr::addr_of_mut!(M7_DEPTH) as *mut Num<i32, 8>;
    let mut sy = 0usize;
    while sy < M7_LINES {
        let dy = sy as i32 - m.horizon;
        // Above the horizon there is no ground; zero is the sky row's `k`, and it also makes the
        // `pa`/`pc` multiplies below produce the flat all-zero matrix the sky wants.
        let k = if dy > 0 {
            let v = m.height / dy;
            if v > kmax {
                kmax
            } else {
                v
            }
        } else {
            Num::from_raw(0)
        };
        unsafe { dst.add(sy).write(k) };
        sy += 1;
    }
}

/// Build one frame's worth of per-scanline transforms straight into [`M7_ROWS`]. See [`Mode7`].
///
/// ⚠️ ARM code in IWRAM, not the default THUMB in ROM. This is the one function in the renderer that
/// runs 160 times a frame, and cartridge ROM is a 16-bit bus behind wait states — the same loop is
/// several times slower fetched from there. `.text_iwram` is copied to 0x0300_0000 at startup by
/// agb's boot, where fetches are one cycle on a 32-bit bus, and a32 gets the barrel-shifter forms
/// the fixed-point multiplies want.
#[link_section = ".text_iwram"]
#[instruction_set(arm::a32)]
fn mode7_rows(m: &Mode7) {
    m7_depth_column(m);
    let sn = m.yaw.sin();
    let cs = m.yaw.cos();
    let half_w: Num<i32, 8> = Num::new(120);
    // Loop-invariant, and it was inside the loop: four fixed-point multiplies per scanline that only
    // ever needed doing once. On this CPU a Num<i32,8> multiply is a 64-bit intermediate, so 640 of
    // them is not a rounding error — the row build was the single biggest item in the frame.
    let kx = sn * m.focal - cs * half_w;
    let ky = cs * m.focal + sn * half_w;
    // The sky: every scanline above the horizon samples the texture's ORIGIN TEXEL.
    //
    // The layer wraps, so there is no off-map to hide in — but these rows have PA = PC = 0, so the
    // whole scanline reads one texel. Point them at (0, 0) and paint that texel the backdrop colour
    // (scripts/gen_rap_dojo.py does) and the sky is that colour, exactly, with no edge anywhere on
    // the ground.
    // ⚠️ Written straight into the published table — there is no intermediate buffer.
    //
    // This used to build a `[AffineMatrixBackground; 160]` on the stack and `copy_nonoverlapping` it
    // into `M7_ROWS`: 2.5KB initialised to sky, 2.5KB overwritten with real rows, then 2.5KB read
    // and 2.5KB written again, every frame, to publish numbers that were already correct where they
    // sat. Rows above the horizon come out all-zero from the same arithmetic as the rest (their `k`
    // is zero), so there is nothing the pre-fill was buying either.
    //
    // Safe against the DMA because the only caller runs inside `commit()`, during vblank, when the
    // HBlank transfers are not running.
    let src = core::ptr::addr_of!(M7_DEPTH) as *const Num<i32, 8>;
    let dst = core::ptr::addr_of_mut!(M7_ROWS) as *mut AffineMatrixBackground;
    // The sky, above the horizon, is its own run — and it must be the ALL-ZERO matrix, not merely
    // the same arithmetic with `k = 0`. With PA = PC = 0 a scanline reads a single texel, and the
    // one it reads is (X, Y); running the ground formula with `k = 0` leaves (cam_x, cam_z), so the
    // sky would sample a different, MOVING floor texel as the camera walked. Pointing it at the
    // origin texel, painted the backdrop colour by the asset, is what makes the sky a flat colour
    // with no edge anywhere on the ground.
    let mut sy = 0usize;
    let sky_end = (m.horizon + 1 + m.haze).clamp(0, M7_LINES as i32) as usize;
    while sy < sky_end {
        unsafe { dst.add(sy).write(M7_ZERO) };
        sy += 1;
    }
    while sy < M7_LINES {
        let k = unsafe { src.add(sy).read() };
        let pa = cs * k;
        let pc = -(sn * k);
        let row = AffineMatrixBackground {
            a: Num::from_raw(pa.to_raw() as i16),
            b: Num::new(0),
            c: Num::from_raw(pc.to_raw() as i16),
            d: Num::new(0),
            x: m.cam_x + k * kx,
            y: m.cam_z + k * ky,
        };
        unsafe { dst.add(sy).write(row) };
        sy += 1;
    }
}

/// `affine_bg_new(assetHandle, wTiles, hTiles)` — create an affine background from a 256-colour
/// `affine:` import and return its handle.
///
/// The source image is tiled (wrapping) across a `wTiles`×`hTiles` map. Give it a camera with
/// `mode7_camera` to make it a ground plane. Affine tiles must be 256-colour, which the `affine:`
/// scheme bakes; at most two affine backgrounds can exist at once (the hardware has BG2 and BG3).
pub fn affine_bg_new(args: &[Value]) -> Value {
    let asset = num(args, 0) as i32;
    let wt = (num(args, 1) as i32).max(1);
    let ht = (num(args, 2) as i32).max(1);
    with_ctx(|ctx| {
        let (palettes, data) = tishlang_runtime_gba::gba::asset_bg(asset)
            .expect("tish-agb: affine_bg_new called with an unknown affine handle");
        ctx.gfx.set_background_palettes(palettes);
        let need = wt.max(ht);
        let size = if need <= 16 {
            AffineBackgroundSize::Background16x16
        } else if need <= 32 {
            AffineBackgroundSize::Background32x32
        } else if need <= 64 {
            AffineBackgroundSize::Background64x64
        } else {
            AffineBackgroundSize::Background128x128
        };
        // WRAP, so the ground has no edge.
        //
        // NoWrap looks tidier — out-of-bounds shows the backdrop, so the sky is free. But the rows
        // just under the horizon see furthest, so they are the first to run off the map, and that
        // renders as a dark band pinned to the horizon on EVERY frame. Enlarging the world does not
        // fix it either: the next size up is 128x128 tiles, whose screenblock does not fit in VRAM
        // and panics on boot. Wrapping removes the edge entirely; the sky is then handled by
        // `mode7_rows` pointing those scanlines at a texel painted the backdrop colour.
        let mut bg = AffineBackground::new(Priority::P2, size, AffineBackgroundWrapBehaviour::Wrap);
        // This layer's six affine registers belong to the HBlank DMA and to `m7_arm_dma`, and to
        // nothing else — see the note where the DMA is armed.
        bg.set_transform_source(AffineTransformSource::External);
        let (sw, sh) = (data.width as i32, data.height as i32);
        let mut y = 0;
        while y < ht {
            let mut x = 0;
            while x < wt {
                let sx = (x % sw) as usize;
                let sy = (y % sh) as usize;
                let ts = data.tile_settings[sy * sw as usize + sx];
                bg.set_tile((x, y), &data.tiles, ts.tile_id());
                x += 1;
            }
            y += 1;
        }
        let handle = ctx.affine_bgs.len();
        ctx.affine_bgs.push(AffineData {
            bg,
            visible: true,
            m7: None,
        });
        Value::Number(handle as f64)
    })
}

/// `mode7_camera(handle, camX, camZ, yaw256, height, horizon, focal)` — point a 3D camera at an
/// affine background, turning it into a ground plane.
///
/// `camX`/`camZ` are the camera's position on the plane in texture pixels, `yaw256` its heading in
/// 1/256 of a turn (64 = 90°), `height` how far it floats above the ground, `horizon` the scanline
/// the ground recedes to (above it the backdrop shows), and `focal` the focal length in pixels —
/// smaller is a wider, more vertiginous lens. Call it every frame; the transforms are rebuilt then.
pub fn mode7_camera(args: &[Value]) -> Value {
    let handle = num(args, 0) as usize;
    let m = Mode7 {
        cam_x: Num::from_raw((num(args, 1) * 256.0) as i32),
        cam_z: Num::from_raw((num(args, 2) * 256.0) as i32),
        yaw: Num::from_raw(num(args, 3) as i32),
        height: Num::from_raw((num(args, 4) * 256.0) as i32),
        horizon: num(args, 5) as i32,
        focal: Num::from_raw((num(args, 6).max(1.0) * 256.0) as i32),
        haze: (num(args, 7) as i32).max(0),
    };
    m7_install_irq();
    with_ctx(|ctx| {
        if let Some(a) = ctx.affine_bgs.get_mut(handle) {
            a.m7 = Some(m);
        }
    });
    Value::Null
}

/// `mode7_billboard(sprite, w, h) -> index` — register a sprite as a billboard of a `w`x`h` cell.
///
/// Registered once, positioned with `mode7_billboard_at`, drawn by `mode7_billboards_draw`.
pub fn mode7_billboard(args: &[Value]) -> Value {
    let b = Billboard {
        sprite: num(args, 0) as i32,
        x: Num::new(0),
        z: Num::new(0),
        w: num(args, 1) as i32,
        h: num(args, 2) as i32,
        active: true,
    };
    with_ctx(|ctx| {
        // Flagged so the frame loop can pull it out of the registration-ordered HUD pass and sort it
        // by the depth the projection computes. Without this a far kart draws over a near one.
        if let Some(sp) = ctx.sprites.get_mut(b.sprite as usize) {
            sp.billboard = true;
        }
        ctx.billboards.push(b);
        Value::Number((ctx.billboards.len() - 1) as f64)
    })
}

/// `mode7_reset()` — forget every affine layer and every billboard.
///
/// `bg_clear` deliberately does not touch these: it predates them. But a game that leaves a race and
/// comes back registers its billboards again, and the arrays only ever grew — so the second race
/// projected two karts onto every sprite and the third projected three. Call this when tearing a
/// Mode 7 scene down, in the same breath as `bg_clear`.
pub fn mode7_reset(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        for b in ctx.billboards.iter() {
            if let Some(sp) = ctx.sprites.get_mut(b.sprite as usize) {
                sp.billboard = false;
                sp.visible = false;
            }
        }
        ctx.billboards.clear();
        ctx.affine_bgs.clear();
        M7_ARMED.0.set(false);
        Value::Null
    })
}

/// `mode7_billboard_at(index, worldX, worldZ)` — move one. Call only when it actually moves.
pub fn mode7_billboard_at(args: &[Value]) -> Value {
    let i = num(args, 0) as usize;
    let (x, z) = (num(args, 1), num(args, 2));
    with_ctx(|ctx| {
        if let Some(b) = ctx.billboards.get_mut(i) {
            b.x = Num::from_raw((x * 256.0) as i32);
            b.z = Num::from_raw((z * 256.0) as i32);
        }
    });
    Value::Null
}

/// `mode7_billboards_draw(handle)` — project and place every billboard. ONE call per frame.
///
/// ⚠️ The projection lives here, in Rust, because the GBA has no FPU. Doing it tish-side meant about
/// ten f64 operations per billboard — and on this CPU every one is a software routine, division
/// worst of all. Four billboards took the frame from 4589 ticks (a locked 60fps) to 8611 with 64 of
/// every 128 frames dropped, i.e. half speed, which is exactly what it looks like: the camera slows
/// down whenever anything is on screen. Native, the same arithmetic is a handful of instructions and
/// the whole cast costs one boxed call.
pub fn mode7_billboards_draw(args: &[Value]) -> Value {
    let handle = num(args, 0) as usize;
    with_ctx(|ctx| {
        let m = match ctx.affine_bgs.get(handle).and_then(|a| a.m7) {
            Some(m) => m,
            None => return,
        };
        // ⚠️ FIXED POINT, not f64. This loop is per-billboard per-frame and ARM7TDMI has no FPU, so
        // every `f64` here was a software routine. With four karts that was invisible; a kart racer
        // with item boxes and hazards on the course pushed it past thirty billboards and the game
        // dropped to about a fifth of full speed. Everything below is 8.8 raw integers with one i64
        // multiply-divide per billboard, and the `focal * height` term is hoisted because it does
        // not depend on which billboard is being projected.
        let sf = m.yaw.sin().to_raw() as i64;
        let cf = m.yaw.cos().to_raw() as i64;
        let camx = m.cam_x.to_raw() as i64;
        let camz = m.cam_z.to_raw() as i64;
        let focal = m.focal.to_raw() as i64;
        let height = m.height.to_raw() as i64;
        let focal_height = focal * height;
        for i in 0..ctx.billboards.len() {
            let b = ctx.billboards[i];
            let idx = b.sprite as usize;
            let dx = b.x.to_raw() as i64 - camx;
            let dz = b.z.to_raw() as i64 - camz;
            // 8.8 * 8.8 -> 16.16, brought back to 8.8.
            let depth = (dx * sf + dz * cf) >> 8;
            let mut show = false;
            let (mut px, mut py) = (0i32, 0i32);
            if depth > 256 {
                let lateral = (dx * cf - dz * sf) >> 8;
                let den = depth << 8;
                px = (120 + ((lateral * focal) / den) as i32) - b.w / 2;
                py = m.horizon + (focal_height / den) as i32;
                show = px > -b.w && px < 240 && py >= 0 && py - b.h < 160;
            }
            if let Some(sp) = ctx.sprites.get_mut(idx) {
                sp.visible = show && b.active;
                if show {
                    sp.x = px;
                    sp.y = py - b.h;
                    sp.depth = py as i16;
                }
            }
        }
    });
    Value::Null
}

/// `mode7_visible(handle, on)` — show or hide an affine layer without destroying it.
pub fn mode7_visible(args: &[Value]) -> Value {
    let handle = num(args, 0) as usize;
    let on = num(args, 1) != 0.0;
    with_ctx(|ctx| {
        if let Some(a) = ctx.affine_bgs.get_mut(handle) {
            a.visible = on;
        }
    });
    Value::Null
}

/// Project a ground point into screen space for the camera on affine layer `handle`.
///
/// Returns `-1` when the point is behind the camera or level with the horizon. This is what puts a
/// BILLBOARD in the scene: a flat sprite drawn at `mode7_screen_x/y` and sized by `mode7_scale`
/// stands on the plane and moves with it, which is exactly how the characters in this genre work.
fn mode7_point(ctx: &GbaCtx, handle: usize, wx: f64, wz: f64) -> Option<(f64, f64, f64)> {
    let a = ctx.affine_bgs.get(handle)?;
    let m = a.m7.as_ref()?;
    // agb's fixed-point sin/cos are LUT-based and take turns, so no libm and no transcendentals —
    // which matters, because this crate is no_std and f64 trig would not link.
    let s = m.yaw.sin().to_raw() as f64 / 256.0;
    let c = m.yaw.cos().to_raw() as f64 / 256.0;
    let dx = wx - m.cam_x.to_raw() as f64 / 256.0;
    let dz = wz - m.cam_z.to_raw() as f64 / 256.0;
    // Rotate the world offset into camera space: `depth` is along the view direction.
    let depth = dx * s + dz * c;
    let lateral = dx * c - dz * s;
    if depth <= 1.0 {
        return None;
    }
    let focal = m.focal.to_raw() as f64 / 256.0;
    let height = m.height.to_raw() as f64 / 256.0;
    let sx = 120.0 + lateral * focal / depth;
    let sy = m.horizon as f64 + focal * height / depth;
    Some((sx, sy, focal / depth))
}

/// `mode7_screen_x(handle, worldX, worldZ)` — screen column of a ground point, or -1 if off camera.
pub fn mode7_screen_x(args: &[Value]) -> Value {
    let (h, x, z) = (num(args, 0) as usize, num(args, 1), num(args, 2));
    with_ctx(|ctx| match mode7_point(ctx, h, x, z) {
        Some((sx, _, _)) => Value::Number(sx),
        None => Value::Number(-1.0),
    })
}

/// `mode7_screen_y(handle, worldX, worldZ)` — screen row where a ground point meets the floor.
pub fn mode7_screen_y(args: &[Value]) -> Value {
    let (h, x, z) = (num(args, 0) as usize, num(args, 1), num(args, 2));
    with_ctx(|ctx| match mode7_point(ctx, h, x, z) {
        Some((_, sy, _)) => Value::Number(sy),
        None => Value::Number(-1.0),
    })
}

/// `mode7_scale(handle, worldX, worldZ)` — how big a thing standing there should be drawn, as a
/// multiple of its texture size (1.0 = actual size). Feed it to an affine sprite, or quantise it to
/// pick between baked sizes.
pub fn mode7_scale(args: &[Value]) -> Value {
    let (h, x, z) = (num(args, 0) as usize, num(args, 1), num(args, 2));
    with_ctx(|ctx| match mode7_point(ctx, h, x, z) {
        Some((_, _, sc)) => Value::Number(sc),
        None => Value::Number(-1.0),
    })
}

/// `bg_count()` — how many backgrounds are currently live.
///
/// An instrument, not a feature. "Ran out of video RAM for tiles" names the allocator, never the
/// caller, and the obvious question — is something creating a background per switch instead of
/// replacing one — is otherwise unanswerable from inside the game. Print this in a HUD across a
/// board switch: flat means the swap is clean, climbing means the old one was never released.
pub fn bg_count(_args: &[Value]) -> Value {
    Value::Number(with_ctx(|ctx| ctx.backgrounds.len()) as f64)
}

// ════════════════════════════════════════════════════════════════════════════════════════════════
// PER-PIXEL DESTRUCTIBLE TERRAIN
//
// An artillery-game ground layer: a solid mass you carve craters out of one PIXEL at a time.
//
// ── Why this exists at all ───────────────────────────────────────────────────────────────────────
// Nothing already here can do it, and all four near-misses are worth writing down so nobody
// re-derives them:
//   * there is no bitmap mode (3/4/5) anywhere in this crate;
//   * a streamed layer (`tilemap_stream`) has 16x16 cells — a crater is a city block;
//   * `tilemap_set8` is 8x8 but its background is `Background32x32`, so 256x256 total;
//   * the UI canvas draws pixel-exact spans, but `ensure_ui_palette` allocates from index 1 and can
//     never return 0, so it can FILL a pixel and never CLEAR one — and carving is clearing.
//
// ── How it works ─────────────────────────────────────────────────────────────────────────────────
// One `RegularBackground` of `DynamicTile16`s, allocated SPARSELY: a tile exists only where terrain
// does, so a mostly-empty arena costs a few hundred tiles rather than one per cell. Occupancy lives
// beside it as a bitmap (one bit per world pixel), which is what collision reads — an O(1) test that
// never touches VRAM. Filling and carving walk the shape ROW BY ROW and write spans, because a
// 64px-radius disc is ~13,000 pixels and per-pixel dispatch from tish would be hopeless.
//
// Terrain owns background palette bank 14, the same way the UI canvas owns 15, so it coexists with
// an ordinary `background:` layer instead of fighting it for all sixteen.
// ════════════════════════════════════════════════════════════════════════════════════════════════

/// Background palette bank the terrain's dynamic tiles render through. 15 is the UI canvas's.
const TERRAIN_PAL_SLOT: u8 = 14;

struct Terrain {
    bg: RegularBackground,
    /// One bit per world pixel, row-major, 32 pixels to a word. 512x384 is 24 KB — the whole reason
    /// collision is a bit test rather than a VRAM read.
    solid: alloc::vec::Vec<u32>,
    /// One slot per 8x8 cell. `None` means nothing is drawn there and no VRAM is held.
    tiles: alloc::vec::Vec<Option<DynamicTile16>>,
    /// Palette index the terrain in each cell is drawn with, so a carved cell can be repainted
    /// without asking the caller what colour it used to be.
    mat: alloc::vec::Vec<u8>,
    /// ⚠️ THE PALETTE BANK PER 8x8 CELL, AND IT IS WHY PLANETS CAN LOOK LIKE PLANETS. A 4bpp GBA
    /// background stores a palette bank in each screenblock entry, so tiles on ONE layer may use
    /// DIFFERENT sixteen-colour banks. Sharing a single bank across the whole board meant four
    /// planets splitting fifteen colours — about four each, which is exactly why every world came
    /// out as flat slabs of one hue. A bank per planet gives each of them fifteen: three materials
    /// of five tones, the range the reference art works in.
    ///
    /// Safe because planets never share a tile: placement keeps them at least 56 px apart.
    bank: alloc::vec::Vec<u8>,
    w: i32,
    h: i32,
    tw: i32,
    th: i32,
    /// Cells whose pixels changed since the last present, so `show` re-points only those.
    dirty: alloc::vec::Vec<u16>,
    /// ⚠️ THE PLANET SURFACE, BAKED ONCE PER WORLD RATHER THAN HASHED PER PIXEL. Two octaves of
    /// value noise is eight hashes a pixel, and a radius-64 planet is 13,000 pixels — 104,000
    /// hashes, which measured 59,780 ticks in a single frame. The same two octaves baked into a
    /// 64x64 tile is 32,000 hashes ONCE, after which a pixel costs four array reads and three
    /// lerps. It wraps, so the sphere can sample outside the tile at the limb without a seam.
    noise: alloc::vec::Vec<u8>,
    noise_seed: u32,
    /// ⚠️ sqrt(t) for t in 0..=4096, at quarter resolution. The sphere's z is a square root PER
    /// PIXEL, and a 12-iteration integer isqrt there is what actually made a band of a large world
    /// cost ~58,000 ticks — not the span calls, and not the noise. One kilobyte of table replaces
    /// it with an array read. The result is 0..64, shifted back to Q12 at the use site, which
    /// quantises z to 1.5% steps: invisible once the shading is posterised into five tones.
    sqrt_lut: alloc::vec::Vec<u8>,
    /// One shared fully-transparent tile that every empty cell points at. A screenblock entry has to
    /// name SOMETHING, and `set_tile` wants a tileset this layer does not have — the UI canvas
    /// solves it the same way with `ui_blank`.
    blank: DynamicTile16,
}

impl Terrain {
    #[inline]
    fn bit(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return false;
        }
        let i = (y * self.w + x) as usize;
        (self.solid[i >> 5] >> (i & 31)) & 1 != 0
    }

    /// Mark a cell as needing its screenblock entry re-pointed. Deduplicated by a linear scan, which
    /// is fine because a single carve touches a few dozen cells at most.
    fn touch(&mut self, cell: usize) {
        let c = cell as u16;
        if !self.dirty.contains(&c) {
            self.dirty.push(c);
        }
    }

    /// Ensure cell `ci` owns a tile, creating a transparent one on first use.
    fn ensure_tile(&mut self, ci: usize) {
        if self.tiles[ci].is_none() {
            self.tiles[ci] = Some(DynamicTile16::new().fill_with(0));
        }
    }

    /// Write one horizontal run of pixels at row `y`, from `x0` to `x1` inclusive.
    ///
    /// `pal` of 0 CLEARS — which is the whole point, and the thing the UI canvas cannot express.
    fn span(&mut self, y: i32, x0: i32, x1: i32, pal: u8) {
        if y < 0 || y >= self.h {
            return;
        }
        let x0 = x0.max(0);
        let x1 = x1.min(self.w - 1);
        if x1 < x0 {
            return;
        }
        let ty = y >> 3;
        let mut x = x0;
        while x <= x1 {
            let tx = x >> 3;
            let ci = (ty * self.tw + tx) as usize;
            // How far this run goes before it leaves the current tile.
            let tile_end = ((tx + 1) << 3) - 1;
            let end = tile_end.min(x1);
            if pal != 0 {
                self.ensure_tile(ci);
                self.mat[ci] = pal;
            }
            let mut changed = false;
            if let Some(t) = self.tiles[ci].as_mut() {
                let py = (y & 7) as usize;
                let mut px = x;
                while px <= end {
                    t.set_pixel((px & 7) as usize, py, pal);
                    px += 1;
                }
                changed = true;
            }
            // Occupancy is authoritative and is written whether or not a tile exists.
            //
            // ⚠️ A RUN IS CONTIGUOUS, SO IT IS A MASKED WORD WRITE, NOT A BIT AT A TIME. Setting
            // each bit on its own turns one OR into eight read-modify-writes with a shift and an
            // index calculation apiece, and a disc is nothing but runs — the whole point of walking
            // spans instead of pixels. On a chip without a barrel-shifter to spare this was most of
            // the cost of carving a crater.
            let mut i = (y * self.w + x) as usize;
            let iend = (y * self.w + end) as usize;
            while i <= iend {
                let word = i >> 5;
                let lo = i & 31;
                let bits = core::cmp::min(32 - lo, iend - i + 1);
                let mask: u32 = if bits == 32 {
                    u32::MAX
                } else {
                    ((1u32 << bits) - 1) << lo
                };
                if pal != 0 {
                    self.solid[word] |= mask;
                } else {
                    self.solid[word] &= !mask;
                }
                i += bits;
            }
            if changed {
                self.touch(ci);
            }
            x = end + 1;
        }
    }

    /// Fill (`pal != 0`) or carve (`pal == 0`) a disc, row by row.
    ///
    /// The half-width per row comes from an integer square root rather than a table: a disc is drawn
    /// a few times a match, not a few times a frame, and a table sized for every radius the game
    /// might want costs more than the arithmetic saves.
    ///
    /// ⚠️ THE HALF-WIDTH IS CARRIED BETWEEN ROWS, NOT RE-DERIVED FOR EACH ONE. Restarting the search
    /// at zero on every row makes this O(r^2) in multiplies for what is an O(r) shape: a radius-34
    /// crater ran about 2,400 multiply-and-compare steps purely to find its own edges, and that was
    /// the single most expensive frame in `examples/warheads` — 20,107 ticks, nearly five frames'
    /// budget, on every explosion. `hw` is monotonic in `dy` (it grows to the equator and shrinks
    /// away from it), so letting it walk from where the previous row left it costs 2r steps for the
    /// whole disc instead of r per row.
    fn disc(&mut self, cx: i32, cy: i32, r: i32, pal: u8) {
        if r <= 0 {
            return;
        }
        let rr = r * r;
        let mut hw = 0i32;
        let mut dy = -r;
        while dy <= r {
            let rem = rr - dy * dy;
            if rem >= 0 {
                while (hw + 1) * (hw + 1) <= rem {
                    hw += 1;
                }
                while hw > 0 && hw * hw > rem {
                    hw -= 1;
                }
                self.span(cy + dy, cx - hw, cx + hw, pal);
            }
            dy += 1;
        }
    }

    /// Stamp one horizontal BAND of a shaded, textured planet.
    ///
    /// This is a port of the technique in Deep-Fold's Pixel Planet Generator (MIT) into integer
    /// arithmetic: project the disc onto a sphere, sample fractal value noise in that projected
    /// space, posterise the result into a handful of flat tones, and cut it with a hard terminator.
    /// The posterising is what gives the style its look — planets made of flat regions of colour
    /// rather than gradients — and it is also exactly what a 16-colour background palette can hold.
    ///
    /// ⚠️ IT IS GENERATED, NOT SAMPLED FROM A SPRITE, AND THAT IS A REQUIREMENT RATHER THAN A
    /// PREFERENCE. This terrain is destructible to the pixel, so every pixel a crater exposes needs
    /// a colour; and the arena picks a radius per planet per match, which a fixed 48x48 or 96x96
    /// planet sprite cannot serve without scaling artefacts at both ends.
    ///
    /// ⚠️ ONE BAND PER CALL. A radius-70 planet is ~15,000 pixels and each one costs a square root
    /// and two octaves of noise; doing a whole planet in one frame is several frames' budget. The
    /// caller steps `band` from 0 to `nbands - 1` on successive SIMULATION ticks, which is the same
    /// pacing the rest of arena generation already uses and is therefore already desync-safe.
    ///
    /// ⚠️ FOUR INDICES, PACKED FOUR BITS EACH, AND THEY ARE MATERIALS RATHER THAN A RAMP. Giving
    /// each planet three consecutive slots of one hue meant a world was one colour with shading on
    /// it — and that is not what a planet looks like. Real ones are several materials at once: sea
    /// and land and cloud, or rock and dust and lava. So the palette is now ONE shared twelve-colour
    /// set spanning every material the game uses, and a class is a choice of four entries from it:
    /// `c0` lit ground, `c1` low ground, `c2` shadow, `c3` a feature appearing only where the
    /// surface noise peaks. Four planets no longer cost four private ramps, so each can be several
    /// colours instead of each being one.
    /// Bake the 64x64 wrapping surface tile for `seed`, if it is not already the current one.
    fn build_noise(&mut self, seed: u32) {
        if self.noise_seed == seed && self.noise.len() == 64 * 64 {
            return;
        }
        self.noise.clear();
        self.noise.resize(64 * 64, 0);
        for y in 0..64i32 {
            for x in 0..64i32 {
                // Octave coordinates are MASKED, not clamped, so the tile is seamless in both axes
                // — the sphere reads past its own edge near the limb, and a hard edge there would
                // draw a visible meridian down the side of every planet.
                let lo = vnoise_wrap(x << 6, y << 6, 8, seed);
                let hi = vnoise_wrap(x << 6, y << 6, 16, seed ^ 0x9e37_79b9);
                self.noise[(y * 64 + x) as usize] = ((lo * 2 + hi) / 3).clamp(0, 255) as u8;
            }
        }
        self.noise_seed = seed;
    }

    /// Sample the baked tile at Q12 sphere coordinates, bilinear, wrapping.
    fn noise_at(&self, u: i32, v: i32) -> i32 {
        // About twenty-four cells across a world: coastlines rather than continent-sized smears.
        let tu = (u * 3) >> 2;
        let tv = (v * 3) >> 2;
        let x0 = (tu >> 8) & 63;
        let y0 = (tv >> 8) & 63;
        let x1 = (x0 + 1) & 63;
        let y1 = (y0 + 1) & 63;
        let fx = tu & 255;
        let fy = tv & 255;
        let a = self.noise[(y0 * 64 + x0) as usize] as i32;
        let b = self.noise[(y0 * 64 + x1) as usize] as i32;
        let c = self.noise[(y1 * 64 + x0) as usize] as i32;
        let d = self.noise[(y1 * 64 + x1) as usize] as i32;
        let ab = a + (((b - a) * fx) >> 8);
        let cd = c + (((d - c) * fx) >> 8);
        ab + (((cd - ab) * fy) >> 8)
    }

    #[allow(clippy::too_many_arguments)]
    fn planet(&mut self, cx: i32, cy: i32, r: i32, pal4: i32, seed: u32, band: i32, nbands: i32) {
        if r <= 0 || nbands <= 0 {
            return;
        }
        self.build_noise(seed);
        // `pal4` now carries only the bank; the palette itself is a fixed layout of three materials
        // of five tones, so the shader indexes it arithmetically rather than being handed colours.
        let bank = (pal4 & 15) as u8;
        let rr = r * r;
        // One reciprocal for the whole band: a per-pixel divide to normalise the radius would be a
        // software routine on this chip, called fifteen thousand times.
        let inv_r = (1i32 << 16) / r;
        // Rotation and feature offsets, so two planets of the same class are not the same planet.
        let rot = (seed & 0xfff) as i32;
        let voff = ((seed >> 12) & 0xfff) as i32;
        let y0 = -r + (2 * r + 1) * band / nbands;
        let y1 = -r + (2 * r + 1) * (band + 1) / nbands;
        let mut dy = y0;
        while dy < y1 {
            let rem = rr - dy * dy;
            if rem < 0 {
                dy += 1;
                continue;
            }
            let hw = isqrt32(rem as u32) as i32;
            let ny = (dy * inv_r) >> 4; // Q12, -4096..4096
                                        // ⚠️ RUNS, NOT PIXELS. `span` bounds-checks the row, locates the cell, creates the tile
                                        // if it is missing and writes the occupancy word — per call. Calling it once per pixel
                                        // made a band of a large world cost ~50,000 ticks. Posterising into a handful of flat
                                        // tones is exactly what produces long runs of one colour, so batching them is nearly
                                        // free to write and turns most of those calls into none.
            let mut run_pal: u8 = 0;
            let mut run_x0: i32 = 0;
            let mut run_x1: i32 = 0;
            let mut have_run = false;
            let mut dx = -hw;
            while dx <= hw {
                let nx = (dx * inv_r) >> 4;
                let d2 = ((nx * nx) >> 12) + ((ny * ny) >> 12); // Q12
                if d2 > 4096 {
                    if have_run {
                        self.span(cy + dy, run_x0, run_x1, run_pal);
                        have_run = false;
                    }
                    dx += 1;
                    continue;
                }
                // z = sqrt(1 - d2) on the unit sphere, Q12, straight out of the table.
                let zq = (self.sqrt_lut[((4096 - d2) >> 2) as usize] as i32) << 6;
                // Spherify: push samples outward as they approach the limb, so the texture
                // compresses at the edges the way a projected sphere does. A multiply and a shift —
                // the true stereographic form needs a divide per pixel and does not look better at
                // sixteen colours.
                let k = 4096 - zq;
                let u = nx + ((nx * k) >> 13) + rot;
                let v = ny + ((ny * k) >> 13) + voff;
                let n = self.noise_at(u, v);
                // Lambert against a fixed light up and to the left, matching where the pack art
                // puts it. Q12.
                let lam = ((-nx) * 2260 + (-ny) * 2260 + zq * 2580) >> 12;
                // ⚠️ MATERIAL AND SHADE ARE SEPARATE AXES, and the palette is laid out so the two
                // compose arithmetically: index = 1 + material * 5 + shade. Three materials of five
                // tones is what the reference art uses, and it is what a whole bank buys.
                //
                // The noise picks WHICH material — sea, land, cloud — with hard edges, because hard
                // edges are what read as coastlines.
                let mat: i32 = if n > 198 {
                    2
                } else if n > 128 {
                    1
                } else {
                    0
                };
                // Light picks the tone WITHIN that material, and this one is dithered. Five tones
                // across a sphere is four visible bands otherwise — the flat slabs the whole
                // rework exists to remove. An ordered 4x4 threshold turns the boundary between two
                // tones into a gradient the eye reads as smooth, which is exactly the trick the
                // reference planets use to look shaded on a sixteen-colour palette.
                let lv = (lam.clamp(0, 4095) * 5) >> 4; // 0..1279, i.e. five tones of 256
                let mut sh = lv >> 8;
                let frac = lv & 255;
                if frac > BAYER4[(((dy & 3) << 2) | (dx & 3)) as usize] as i32 {
                    sh += 1;
                }
                sh = sh.clamp(0, 4);
                let mut pal = (1 + mat * 5 + sh) as u8;
                // A dark rim all the way round reads as curvature and separates the planet from the
                // starfield — the reference art does the same with a one-pixel outline. It is the
                // darkest tone of the first material, so it belongs to the planet's own palette
                // rather than costing a shared slot.
                if d2 > 3900 {
                    pal = 1;
                }
                self.bank[(((cy + dy) >> 3) * self.tw + ((cx + dx) >> 3)) as usize] = bank;
                let wx = cx + dx;
                if have_run && pal == run_pal {
                    run_x1 = wx;
                } else {
                    if have_run {
                        self.span(cy + dy, run_x0, run_x1, run_pal);
                    }
                    run_pal = pal;
                    run_x0 = wx;
                    run_x1 = wx;
                    have_run = true;
                }
                dx += 1;
            }
            if have_run {
                self.span(cy + dy, run_x0, run_x1, run_pal);
            }
            dy += 1;
        }
    }
}

/// A 4x4 ordered dither matrix, scaled to 0..255. Turns five tones into a readable gradient.
const BAYER4: [u8; 16] = [
    8, 136, 40, 168, 200, 72, 232, 104, 56, 184, 24, 152, 248, 120, 216, 88,
];

/// Integer square root. No FPU, and `f32::sqrt` would be a software call per pixel.
fn isqrt32(mut n: u32) -> u32 {
    let mut res = 0u32;
    let mut bit = 1u32 << 30;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if n >= res + bit {
            n -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    res
}

/// A hash, not a random number generator: the same cell must give the same value every time the
/// planet is drawn, on both consoles, for ever. lowbias32's constants.
fn hash2(x: i32, y: i32, seed: u32) -> i32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1) ^ seed;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x2974_5aad);
    h ^= h >> 16;
    (h & 0xff) as i32
}

/// Value noise over a `period`-cell grid that wraps inside the 64x64 tile. `x`/`y` are Q6 tile
/// coordinates. Used only while baking, never per pixel.
fn vnoise_wrap(x: i32, y: i32, period: i32, seed: u32) -> i32 {
    let s = (x * period) >> 6;
    let t = (y * period) >> 6;
    let xi = s >> 6;
    let yi = t >> 6;
    let xf = (s & 63) << 2;
    let yf = (t & 63) << 2;
    let m = period - 1;
    let a = hash2(xi & m, yi & m, seed);
    let b = hash2((xi + 1) & m, yi & m, seed);
    let c = hash2(xi & m, (yi + 1) & m, seed);
    let d = hash2((xi + 1) & m, (yi + 1) & m, seed);
    // smoothstep(t) = t*t*(3-2t) in Q8 — a straight lerp leaves grid creases that survive being
    // posterised into three tones and read as a lattice.
    let sx = (((xf * xf) >> 8) * (768 - 2 * xf)) >> 9;
    let sy = (((yf * yf) >> 8) * (768 - 2 * yf)) >> 9;
    let ab = a + (((b - a) * sx) >> 8);
    let cd = c + (((d - c) * sx) >> 8);
    ab + (((cd - ab) * sy) >> 8)
}

/// `terrain_new(w, h)` — create the terrain layer, `w` x `h` WORLD PIXELS. Replaces any existing one.
pub fn terrain_new(args: &[Value]) -> Value {
    let w = (num(args, 0) as i32).max(8);
    let h = (num(args, 1) as i32).max(8);
    terrain_new_typed(w, h);
    Value::Null
}

/// See [`terrain_new`].
pub fn terrain_new_typed(w: i32, h: i32) {
    let tw = (w + 7) >> 3;
    let th = (h + 7) >> 3;
    // Size each axis separately, exactly as the scene loader does: rounding a 64x32 map up to
    // 64x64 spends two extra screenblocks (4 KB) on a vertical repeat the hardware wraps for free.
    let size = match (tw > 32, th > 32) {
        (true, true) => RegularBackgroundSize::Background64x64,
        (true, false) => RegularBackgroundSize::Background64x32,
        (false, true) => RegularBackgroundSize::Background32x64,
        (false, false) => RegularBackgroundSize::Background32x32,
    };
    with_ctx(|ctx| {
        let cells = (tw * th) as usize;
        let mut tiles = alloc::vec::Vec::new();
        tiles.resize_with(cells, || None);
        ctx.terrain = Some(Terrain {
            bg: RegularBackground::new(Priority::P2, size, TileFormat::FourBpp),
            solid: alloc::vec![0u32; ((w * h) as usize).div_ceil(32)],
            tiles,
            mat: alloc::vec![0u8; cells],
            bank: alloc::vec![TERRAIN_PAL_SLOT; cells],
            w,
            h,
            tw,
            th,
            dirty: alloc::vec::Vec::new(),
            noise: alloc::vec::Vec::new(),
            noise_seed: 0,
            sqrt_lut: {
                let mut v = alloc::vec::Vec::with_capacity(1025);
                for i in 0..1025u32 {
                    v.push(isqrt32(i * 4) as u8);
                }
                v
            },
            blank: DynamicTile16::new().fill_with(0),
        });
    });
}

/// `terrain_palette(index, 0xRRGGBB)` — set one colour of the terrain's own palette bank.
/// Index 0 is transparent and cannot be set; 1..15 are materials.
pub fn terrain_palette(args: &[Value]) -> Value {
    terrain_palette_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

/// See [`terrain_palette`].
pub fn terrain_palette_typed(index: i32, color: i32) {
    if !(1..16).contains(&index) {
        return;
    }
    with_ctx(|ctx| {
        let mut arr = [Rgb15::BLACK; 16];
        for (i, slot) in ctx.terrain_pal.iter().enumerate() {
            let v = *slot as u32;
            arr[i] = Rgb::new(
                ((v >> 16) & 0xff) as u8,
                ((v >> 8) & 0xff) as u8,
                (v & 0xff) as u8,
            )
            .to_rgb15();
        }
        ctx.terrain_pal[index as usize] = color;
        let v = color as u32;
        arr[index as usize] = Rgb::new(
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
        )
        .to_rgb15();
        ctx.gfx
            .set_background_palette(TERRAIN_PAL_SLOT, &Palette16::new(arr));
    });
}

/// `terrain_pal_bank(bank, index, 0xRRGGBB)` — set one colour of one of the terrain's palette banks.
///
/// A 4bpp background picks its bank per TILE, so the terrain layer can hold several sixteen-colour
/// palettes at once. This is what lets each planet be drawn with its own fifteen tones instead of
/// every planet sharing one bank between them.
pub fn terrain_pal_bank(args: &[Value]) -> Value {
    terrain_pal_bank_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// See [`terrain_pal_bank`].
pub fn terrain_pal_bank_typed(bank: i32, index: i32, color: i32) {
    if !(0..16).contains(&bank) || !(1..16).contains(&index) {
        return;
    }
    with_ctx(|ctx| {
        ctx.terrain_banks[bank as usize][index as usize] = color;
        let mut arr = [Rgb15::BLACK; 16];
        for (i, slot) in ctx.terrain_banks[bank as usize].iter().enumerate() {
            let v = *slot as u32;
            arr[i] = Rgb::new(
                ((v >> 16) & 0xff) as u8,
                ((v >> 8) & 0xff) as u8,
                (v & 0xff) as u8,
            )
            .to_rgb15();
        }
        ctx.gfx
            .set_background_palette(bank as u8, &Palette16::new(arr));
    });
}

/// `terrain_disc(cx, cy, r, mat)` — add a solid disc of palette index `mat` (1..15).
pub fn terrain_disc(args: &[Value]) -> Value {
    terrain_disc_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
    );
    Value::Null
}

/// See [`terrain_disc`].
pub fn terrain_disc_typed(cx: i32, cy: i32, r: i32, mat: i32) {
    let pal = mat.clamp(1, 15) as u8;
    with_ctx(|ctx| {
        if let Some(t) = ctx.terrain.as_mut() {
            t.disc(cx, cy, r, pal);
        }
    });
}

/// `terrain_carve(cx, cy, r)` — remove a disc. This is the one thing nothing else in the crate can do.
pub fn terrain_carve(args: &[Value]) -> Value {
    terrain_carve_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// See [`terrain_carve`].
pub fn terrain_carve_typed(cx: i32, cy: i32, r: i32) {
    with_ctx(|ctx| {
        if let Some(t) = ctx.terrain.as_mut() {
            t.disc(cx, cy, r, 0);
        }
    });
}

/// `terrain_planet(cx, cy, r, base, seed, band, nbands)` — stamp one band of a generated planet.
///
/// See [`Terrain::planet`]. Call it once per band on successive simulation ticks; the pixels are
/// identical whichever way the caller spreads them, because the surface comes from a hash of the
/// coordinates and the seed rather than from any running state.
pub fn terrain_planet(args: &[Value]) -> Value {
    terrain_planet_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
        num(args, 5) as i32,
        num(args, 6) as i32,
    );
    Value::Null
}

/// See [`terrain_planet`].
#[allow(clippy::too_many_arguments)]
pub fn terrain_planet_typed(
    cx: i32,
    cy: i32,
    r: i32,
    pal4: i32,
    seed: i32,
    band: i32,
    nbands: i32,
) {
    with_ctx(|ctx| {
        if let Some(t) = ctx.terrain.as_mut() {
            t.planet(cx, cy, r, pal4, seed as u32, band, nbands);
        }
    });
}

/// `terrain_solid(x, y)` — 1 when that world PIXEL is solid. The collision query, and a bit test.
pub fn terrain_solid(args: &[Value]) -> Value {
    Value::Number(terrain_solid_typed(num(args, 0) as i32, num(args, 1) as i32) as f64)
}

/// See [`terrain_solid`].
pub fn terrain_solid_typed(x: i32, y: i32) -> i32 {
    with_ctx(|ctx| match ctx.terrain.as_ref() {
        Some(t) => t.bit(x, y) as i32,
        None => 0,
    })
}

/// `terrain_mass(cx, cy, r)` — how much solid terrain lies within `r` of a point.
///
/// This is what lets a game keep gravity honest as its worlds are eaten. A planet modelled as a
/// point mass at its centre keeps pulling from that point after the centre has been blown away, so
/// anything that falls in gets held at a core that is no longer there. The physical answer is the
/// shell theorem: what pulls you at distance d is the mass ENCLOSED within d, and for a hollowed-out
/// world that is nearly nothing — so you drift rather than stick.
///
/// ⚠️ Sampled every 4 px on both axes and scaled back up. What the caller wants is the RATIO of
/// enclosed mass to original, to decide how hard a half-eaten world still pulls — and that is a
/// few-percent question, not an exact one. At 2 px this was a six-frame hitch on every explosion;
/// at 4 px it is under two, and a 64 px disc still contributes ~200 samples per shell.
pub fn terrain_mass(args: &[Value]) -> Value {
    Value::Number(terrain_mass_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    ) as f64)
}

/// See [`terrain_mass`].
pub fn terrain_mass_typed(cx: i32, cy: i32, r: i32) -> i32 {
    if r <= 0 {
        return 0;
    }
    with_ctx(|ctx| {
        let Some(t) = ctx.terrain.as_ref() else {
            return 0;
        };
        let r2 = r * r;
        let mut n = 0i32;
        let mut dy = -r;
        while dy <= r {
            let rem = r2 - dy * dy;
            if rem >= 0 {
                let mut hw = 0i32;
                while (hw + 1) * (hw + 1) <= rem {
                    hw += 1;
                }
                let y = cy + dy;
                let mut x = cx - hw;
                while x <= cx + hw {
                    if t.bit(x, y) {
                        n += 1;
                    }
                    x += 4;
                }
            }
            dy += 4;
        }
        n * 16
    })
}

/// `terrain_clear()` — empty every pixel and release every tile's VRAM, keeping the layer.
pub fn terrain_clear(_args: &[Value]) -> Value {
    terrain_clear_typed();
    Value::Null
}

/// See [`terrain_clear`].
pub fn terrain_clear_typed() {
    with_ctx(|ctx| {
        if let Some(t) = ctx.terrain.as_mut() {
            for w in t.solid.iter_mut() {
                *w = 0;
            }
            for m in t.mat.iter_mut() {
                *m = 0;
            }
            let (tw, th) = (t.tw, t.th);
            for i in 0..t.tiles.len() {
                if t.tiles[i].is_some() {
                    t.tiles[i] = None;
                    let (tx, ty) = ((i as i32) % tw, (i as i32) / tw);
                    let _ = th;
                    t.bg.set_tile_dynamic16(
                        Vector2D::new(tx, ty),
                        &t.blank,
                        TileEffect::default().palette(TERRAIN_PAL_SLOT),
                    );
                }
            }
            t.dirty.clear();
        }
    });
}

/// Re-point the screenblock entries of every cell whose pixels changed, then show the layer.
/// Called from the compose pass; a frame with no carving does nothing but `show`.
/// ⚠️ Takes the terrain field, NOT the whole context: `frame` already holds a mutable borrow of
/// `ctx.gfx`, so a second `&mut GbaCtx` is E0499. Borrowing the one field keeps them disjoint, which
/// is how the ordinary background loop in the same pass gets away with it.
fn terrain_present(
    terrain: &mut Option<Terrain>,
    cam: Vector2D<i32>,
    frame: &mut GraphicsFrame<'_>,
) -> Option<RegularBackgroundId> {
    let t = terrain.as_mut()?;
    while let Some(cell) = t.dirty.pop() {
        let ci = cell as usize;
        let (tx, ty) = ((ci as i32) % t.tw, (ci as i32) / t.tw);
        let bank = t.bank[ci];
        match t.tiles[ci].as_ref() {
            Some(tile) => {
                t.bg.set_tile_dynamic16(
                    Vector2D::new(tx, ty),
                    tile,
                    TileEffect::default().palette(bank),
                );
            }
            None => {
                t.bg.set_tile_dynamic16(
                    Vector2D::new(tx, ty),
                    &t.blank,
                    TileEffect::default().palette(bank),
                );
            }
        }
    }
    t.bg.set_scroll_pos(cam);
    Some(t.bg.show(frame))
}

/// `bg_set_tile(layer, col, row, gid)` — change one map cell of a STREAMED layer at runtime.
///
/// `gid` is a Tiled global tile id, 1-based, exactly as the map data stores it; 0 blanks the cell.
/// `layer` is the index the scene's layers were pushed in (0 = the first, usually the ground).
///
/// This exists because a `scene:` map streams straight out of ROM and cannot be written to, so the
/// game had no way to change a tile at all: burning a bush, bombing a wall or pushing a block only
/// ever cleared the COLLISION (`grid_set_solid`) and left the art untouched — a burnt bush that
/// still looks like a bush and is silently walkable. The override is kept beside the layer and
/// consulted by `provide_tile`, so it survives scrolling; it dies with the layer on a scene load,
/// which is the right lifetime because a game re-applies its own persistent state on room entry.
///
/// Marks the stream dirty so the next frame refills the visible window. That is a whole-viewport
/// refill, which is why this is for one-off events and not something to call every frame.
pub fn bg_set_tile(args: &[Value]) -> Value {
    let layer = num(args, 0) as usize;
    let col = num(args, 1) as i32;
    let row = num(args, 2) as i32;
    let gid = num(args, 3) as i32;
    with_ctx(|ctx| {
        let Some(l) = ctx.stream_layers.get_mut(layer) else {
            return Value::Null;
        };
        if col < 0 || col >= l.w || row < 0 || row >= l.h {
            return Value::Null;
        }
        let idx = (row * l.w + col) as u32;
        let g = gid as i16;
        // ⚠️ AN OWNED LAYER HAS SOMEWHERE TO WRITE, SO WRITE THERE.
        //
        // The sparse `patch` list exists because a `scene:` map's GIDs live in ROM (see the doc
        // comment above): there is nowhere to put a changed tile, so it has to sit beside the map.
        // A layer built by `tilemap_stream` is `StreamGids::Owned` — a `Vec<i16>` in RAM — and for
        // those the patch is pure overhead in BOTH directions: `find` is a linear scan per write,
        // and `provide_tile` scans it again for every tile the InfiniteScrolledMap pages in.
        //
        // That is fine for what it was built for ("a room holds a handful at most") and quadratic
        // for anything that reshapes terrain. examples/warheads carves craters out of destructible
        // planets — hundreds to thousands of tiles a match — and with the patch path every streamed
        // tile would then scan a list that long, for ever, while the camera chases a shell.
        // Writing Owned GIDs in place makes the write O(1) and leaves `patch` empty, so the read
        // side keeps its one `is_empty` fast path. ROM-backed maps are untouched.
        match &mut l.data {
            StreamGids::Owned(v) => {
                if let Some(slot) = v.get_mut(idx as usize) {
                    *slot = g;
                }
            }
            StreamGids::Rom { .. } => match l.patch.iter_mut().find(|(i, _)| *i == idx) {
                Some(slot) => slot.1 = g,
                None => l.patch.push((idx, g)),
            },
        }
        ctx.stream_dirty = true;
        Value::Null
    })
}

/// See [`bg_set_tile`].
pub fn bg_set_tile_typed(layer: i32, col: i32, row: i32, gid: i32) {
    bg_set_tile(&[
        Value::Number(layer as f64),
        Value::Number(col as f64),
        Value::Number(row as f64),
        Value::Number(gid as f64),
    ]);
}

/// `bg_clear()` — drop every background layer (frees their tiles). Used on scene
/// transitions before building the next scene's background.
///
/// Stream layers keep their RegularBackground tile boxes in a pool (`stream_active = 0`)
/// instead of Drop — reallocating those 2KB boxes every warp is what fragments EWRAM
/// into "allocation of 5120 bytes failed" on the next cave/overworld enter.
pub fn bg_clear(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        ctx.backgrounds.clear();
        // ⚠️ Reclaim the tiles NOW, not at the next commit.
        //
        // Dropping a background only QUEUES its tiles for release; agb frees them in `gc()`, which
        // the frame commit runs. A caller that clears and immediately rebuilds — which is what
        // switching board costs — never reaches a commit in between, so VRAM peaks at BOTH boards
        // even though only one is ever on screen. Two 554-tile boards peak at 1106 against the
        // ~1022 the tile pools hold, and it dies as "Ran out of video RAM for tiles" on a board
        // that loads perfectly well on its own. One call here makes the switch cost the larger of
        // the two rather than their sum.
        agb::display::tiled::VRAM_MANAGER.gc();
        ctx.stream_active = 0;
        // Scene backdrops are pooled like stream layers and for the same reason — dropping their
        // tile boxes every warp is what fragments EWRAM. Deactivate, don't free.
        ctx.scene_bg_active = 0;
        ctx.stream_dirty = true;
        // Drop the stream pool. Replacing each InfiniteScrolledMap with a fresh
        // RegularBackground::new (the old `clear_tiles` path) allocates a second tile box
        // while the first is still live. That is a VRAM/heap peak, not a clear.
        ctx.stream_layers.clear();
        agb::display::tiled::VRAM_MANAGER.gc();
        ctx.camera_x = 0;
        ctx.camera_y = 0;
        ctx.map_info = None;
    });
    Value::Null
}

/// Read a tish array (boxed or numeric-packed) into a `Vec<i32>` via the facade's
/// index/length accessors, so callers don't care about the backing representation.
fn read_i32_array(v: &Value) -> Vec<i32> {
    let len = match get_prop(v, "length") {
        Value::Number(n) => n as usize,
        _ => 0,
    };
    let mut out = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        out.push(match get_index(v, &Value::Number(i as f64)) {
            Value::Number(n) => n as i32,
            _ => 0,
        });
        i += 1;
    }
    out
}

/// `tilemap_new(tilesetHandle, tilesetCols, width, height, data, priority)` — build a
/// background from a Tiled-style tile layer. `data` is a row-major array of GIDs (0 = empty;
/// N → tileset tile N-1). The tileset (baked via `background:`) is a grid `tilesetCols` 16x16
/// tiles wide; each map tile is laid down as its 2x2 block of GBA 8x8 tiles. `priority` picks
/// the layer depth (0=front .. 3=back; ground = 3, decor above it = 2). Returns a bg handle.
pub fn tilemap_new(args: &[Value]) -> Value {
    let tileset = num(args, 0) as i32;
    let cols = (num(args, 1) as i32).max(1);
    let w = num(args, 2) as i32;
    let h = num(args, 3) as i32;
    let data = match args.get(4) {
        Some(v) => read_i32_array(v),
        None => Vec::new(),
    };
    let priority = match num(args, 5) as i32 {
        0 => Priority::P0,
        1 => Priority::P1,
        2 => Priority::P2,
        _ => Priority::P3,
    };
    with_ctx(|ctx| {
        let (palettes, tdata) = match tishlang_runtime_gba::gba::asset_bg(tileset) {
            Some(t) => t,
            None => return Value::Null,
        };
        ctx.gfx.set_background_palettes(palettes);
        set_backdrop(ctx, palettes);
        let tiles = &tdata.tiles;
        let settings = tdata.tile_settings;
        let w8 = 2 * cols; // tileset width in GBA 8x8 tiles
        let mut bg = RegularBackground::new(
            priority,
            RegularBackgroundSize::Background32x32,
            TileFormat::FourBpp,
        );
        let mut r = 0;
        while r < h {
            let mut c = 0;
            while c < w {
                let gid = data.get((r * w + c) as usize).copied().unwrap_or(0);
                if gid > 0 {
                    let t = gid - 1; // tileset tile index (row-major)
                    let (tcol, trow) = (t % cols, t / cols);
                    let tl = ((2 * trow) * w8 + 2 * tcol) as usize;
                    let bl = ((2 * trow + 1) * w8 + 2 * tcol) as usize;
                    if bl + 1 < settings.len() {
                        let (px, py) = (2 * c, 2 * r);
                        bg.set_tile(Vector2D::new(px, py), tiles, settings[tl]);
                        bg.set_tile(Vector2D::new(px + 1, py), tiles, settings[tl + 1]);
                        bg.set_tile(Vector2D::new(px, py + 1), tiles, settings[bl]);
                        bg.set_tile(Vector2D::new(px + 1, py + 1), tiles, settings[bl + 1]);
                    }
                }
                c += 1;
            }
            r += 1;
        }
        let handle = ctx.backgrounds.len();
        ctx.backgrounds.push(BgData {
            bg,
            visible: true,
            asset: tileset,
            parallax: None,
            bands: None,
        });
        Value::Number(handle as f64)
    })
}

/// Place tileset tile `t` (row-major index) as its 2x2 block of 8x8 tiles at map cell
/// `(c, r)`. `w8` is the tileset width in 8x8 tiles (= 2 * cols).
fn place_map_tile(
    bg: &mut RegularBackground,
    t: i32,
    cols: i32,
    w8: i32,
    c: i32,
    r: i32,
    tiles: &TileSet,
    settings: &[TileSetting],
) {
    let (tcol, trow) = (t % cols, t / cols);
    let tl = ((2 * trow) * w8 + 2 * tcol) as usize;
    let bl = ((2 * trow + 1) * w8 + 2 * tcol) as usize;
    if bl + 1 < settings.len() {
        let (px, py) = (2 * c, 2 * r);
        bg.set_tile(Vector2D::new(px, py), tiles, settings[tl]);
        bg.set_tile(Vector2D::new(px + 1, py), tiles, settings[tl + 1]);
        bg.set_tile(Vector2D::new(px, py + 1), tiles, settings[bl]);
        bg.set_tile(Vector2D::new(px + 1, py + 1), tiles, settings[bl + 1]);
    }
}

/// The 3x3-"blob" autotiled GID for terrain `tid` at cell (c,r), using that terrain's
/// `block` (top-left tile of its 3x3 set). The corner/edge/centre piece is chosen from the
/// four orthogonal neighbours; off-map counts as "same" so the map edge isn't bordered.
/// Shared by the fixed (`tilemap_terrain`) and streamed (`tilemap_stream_terrain`) paths.
fn autotile_gid(
    ground: &[i32],
    w: i32,
    h: i32,
    cols: i32,
    block: i32,
    tid: i32,
    c: i32,
    r: i32,
) -> i32 {
    let same = |cc: i32, rr: i32| -> bool {
        if cc < 0 || cc >= w || rr < 0 || rr >= h {
            return true;
        }
        ground.get((rr * w + cc) as usize).copied().unwrap_or(0) == tid
    };
    let bx = if !same(c - 1, r) {
        0
    } else if !same(c + 1, r) {
        2
    } else {
        1
    };
    let by = if !same(c, r - 1) {
        0
    } else if !same(c, r + 1) {
        2
    } else {
        1
    };
    block + by * cols + bx + 1
}

/// Build a scene BACKDROP: a hardware-wrapping background filled from the layer's top-left 16x16
/// cells. That is exactly 256x256 px, which is the size the GBA wraps a regular background at, so
/// those cells tile the screen forever in both axes — a sky painted once in Tiled repeats itself
/// with no seam and no streaming. Anything the artist drew beyond the first 16x16 of a parallax
/// layer is NOT shown, because there is nowhere for it to go.
///
/// Reuses a pooled slot when one is free (see `GbaCtx::scene_bgs`).
#[allow(clippy::too_many_arguments)]
fn push_scene_backdrop(
    ctx: &mut GbaCtx,
    data: &'static [u8],
    off: usize,
    width: i32,
    cols: i32,
    tiles: &'static TileSet,
    settings: &'static [TileSetting],
    priority: Priority,
    par: (i32, i32),
) {
    let mut bg = RegularBackground::new(
        priority,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    let w8 = 2 * cols; // tileset width in GBA 8x8 tiles
    for r in 0..16 {
        for c in 0..16 {
            // Each 16px map cell is a 2x2 block of the GBA's 8x8 tiles.
            let gid = rd_u16(data, off + ((r * width + c) as usize) * 2);
            if gid <= 0 {
                continue;
            }
            let t = gid - 1;
            let (tcol, trow) = (t % cols, t / cols);
            let tl = ((2 * trow) * w8 + 2 * tcol) as usize;
            let bl = ((2 * trow + 1) * w8 + 2 * tcol) as usize;
            if bl + 1 >= settings.len() {
                continue;
            }
            let (px, py) = (2 * c, 2 * r);
            bg.set_tile(Vector2D::new(px, py), tiles, settings[tl]);
            bg.set_tile(Vector2D::new(px + 1, py), tiles, settings[tl + 1]);
            bg.set_tile(Vector2D::new(px, py + 1), tiles, settings[bl]);
            bg.set_tile(Vector2D::new(px + 1, py + 1), tiles, settings[bl + 1]);
        }
    }
    let i = ctx.scene_bg_active;
    ctx.scene_bg_active = i + 1;
    // Bands are NOT cleared on reuse — a game sets them per scene after the load, and a scene that
    // sets none should not inherit the last one's. Clear here; `scene_bands` runs after this.
    if i < ctx.scene_bgs.len() {
        ctx.scene_bgs[i] = BgData {
            bg,
            visible: true,
            asset: -1,
            parallax: Some(par),
            bands: None,
        };
    } else {
        ctx.scene_bgs.push(BgData {
            bg,
            visible: true,
            asset: -1,
            parallax: Some(par),
            bands: None,
        });
    }
}

/// Wrap a GID layer in an `InfiniteScrolledMap` and register it as a streamed layer.
/// Reuses a pooled slot when available so scene warps don't allocate a fresh 2KB tile box.
fn push_stream_layer(
    ctx: &mut GbaCtx,
    data: StreamGids,
    w: i32,
    h: i32,
    cols: i32,
    tiles: &'static TileSet,
    settings: &'static [TileSetting],
    priority: Priority,
    par: (i32, i32),
) {
    let i = ctx.stream_active;
    ctx.stream_dirty = true;
    if i < ctx.stream_layers.len() {
        let layer = &mut ctx.stream_layers[i];
        layer.data = data;
        // Pool reuse must reset the overrides too — see `bg_clear`. The fresh-layer branch below
        // claims "scene loads rebuild the layer list", and that is exactly what this branch does NOT
        // do: it hands back a dormant layer with its previous scene's state still on it.
        layer.patch.clear();
        layer.w = w;
        layer.h = h;
        layer.cols = cols;
        layer.tiles = tiles;
        layer.settings = settings;
        layer.par = par;
        // A pooled slot can be carrying the last scene's hidden state; a fresh scene's layers are
        // all visible until the game says otherwise.
        layer.visible = true;
        layer.map.set_priority(priority);
        ctx.stream_active = i + 1;
        return;
    }
    let bg = RegularBackground::new(
        priority,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    ctx.stream_layers.push(StreamLayer {
        map: InfiniteScrolledMap::new(bg),
        data,
        w,
        h,
        cols,
        // A fresh layer carries no overrides. Scene loads rebuild the layer list, so a burnt bush
        // does not follow the player into the next map — the game re-applies what it remembers.
        patch: Vec::new(),
        tiles,
        settings,
        visible: true,
        par,
    });
    ctx.stream_active = ctx.stream_layers.len();
}

/// `tilemap_terrain(tilesetHandle, tilesetCols, width, height, ground, blocks, baseId)` —
/// render an AUTOTILED terrain map. `ground` is a terrain id per cell; `blocks[id-1]` is the
/// top-left tile of that terrain's 3x3 autotile block in the tileset. The `baseId` terrain
/// fills a solid back layer (priority 3); every other terrain is autotiled as a transparent
/// overlay (priority 2) whose corner/edge/centre piece is chosen from its orthogonal
/// neighbours (off-map counts as "same", so the map edge isn't bordered). All per-cell work
/// happens here in Rust — nothing builds arrays on the tish side.
pub fn tilemap_terrain(args: &[Value]) -> Value {
    let tileset = num(args, 0) as i32;
    let cols = (num(args, 1) as i32).max(1);
    let w = num(args, 2) as i32;
    let h = num(args, 3) as i32;
    let ground = match args.get(4) {
        Some(v) => read_i32_array(v),
        None => Vec::new(),
    };
    let blocks = match args.get(5) {
        Some(v) => read_i32_array(v),
        None => Vec::new(),
    };
    let base_id = num(args, 6) as i32;
    with_ctx(|ctx| {
        let (palettes, tdata) = match tishlang_runtime_gba::gba::asset_bg(tileset) {
            Some(t) => t,
            None => return Value::Null,
        };
        ctx.gfx.set_background_palettes(palettes);
        set_backdrop(ctx, palettes);
        let tiles = &tdata.tiles;
        let settings = tdata.tile_settings;
        let w8 = 2 * cols;
        // Base layer: fill every cell with the base terrain's centre tile.
        let base_block = blocks
            .get((base_id - 1).max(0) as usize)
            .copied()
            .unwrap_or(0);
        let base_center = base_block + cols + 1;
        let mut base_bg = RegularBackground::new(
            Priority::P3,
            RegularBackgroundSize::Background32x32,
            TileFormat::FourBpp,
        );
        let mut r = 0;
        while r < h {
            let mut c = 0;
            while c < w {
                place_map_tile(&mut base_bg, base_center, cols, w8, c, r, tiles, settings);
                c += 1;
            }
            r += 1;
        }
        ctx.backgrounds.push(BgData {
            bg: base_bg,
            visible: true,
            asset: -1,
            parallax: None,
            bands: None,
        });
        // Overlay each non-base terrain, autotiled (same algorithm as the streamed path).
        let mut tid = 1;
        while tid <= blocks.len() as i32 {
            if tid != base_id {
                let block = blocks[(tid - 1) as usize];
                let mut ov = RegularBackground::new(
                    Priority::P2,
                    RegularBackgroundSize::Background32x32,
                    TileFormat::FourBpp,
                );
                let mut r = 0;
                while r < h {
                    let mut c = 0;
                    while c < w {
                        if ground.get((r * w + c) as usize).copied().unwrap_or(0) == tid {
                            let gid = autotile_gid(&ground, w, h, cols, block, tid, c, r);
                            place_map_tile(&mut ov, gid - 1, cols, w8, c, r, tiles, settings);
                        }
                        c += 1;
                    }
                    r += 1;
                }
                ctx.backgrounds.push(BgData {
                    bg: ov,
                    visible: true,
                    asset: -1,
                    parallax: None,
                    bands: None,
                });
            }
            tid += 1;
        }
        Value::Null
    })
}

/// Look up the streamed-layer tile at 8x8-tile position `pos`: find the 16x16 map cell it
/// falls in, that cell's GID, and the specific 8x8 subtile of that tile's 2x2 block. Off-map
/// or empty cells return the transparent tile.
fn provide_tile(
    pos: Vector2D<i32>,
    data: &StreamGids,
    patch: &[(u32, i16)],
    w: i32,
    h: i32,
    cols: i32,
    tiles: &'static TileSet,
    settings: &'static [TileSetting],
) -> (&'static TileSet, TileSetting) {
    let (mx, my) = (pos.x.div_euclid(2), pos.y.div_euclid(2));
    if mx < 0 || mx >= w || my < 0 || my >= h {
        return (tiles, TileSetting::BLANK);
    }
    let idx = (my * w + mx) as usize;
    // A runtime override wins over the baked map. Empty on every layer that has never been written
    // to, which is all of them in most games, so this is one `is_empty` on the hot path.
    let gid = match patch.iter().find(|(i, _)| *i as usize == idx) {
        Some((_, g)) => *g as i32,
        None => data.gid_at(idx) as i32,
    };
    if gid <= 0 {
        return (tiles, TileSetting::BLANK);
    }
    let t = gid - 1;
    let (tcol, trow) = (t % cols, t / cols);
    let w8 = 2 * cols;
    let (sx, sy) = (pos.x.rem_euclid(2), pos.y.rem_euclid(2));
    let sub = ((2 * trow + sy) * w8 + (2 * tcol + sx)) as usize;
    (
        tiles,
        settings.get(sub).copied().unwrap_or(TileSetting::BLANK),
    )
}

/// Scroll every streamed layer to the camera and KEEP CALLING until agb reports `Done`.
///
/// `InfiniteScrolledMap::set_scroll_pos` only copies 2 of the screen's 21 tile rows per call,
/// and its initial fill RESTARTS from row 0 whenever the camera crosses an 8px boundary
/// mid-fill. One call per frame therefore never converges while the camera is moving (a
/// following camera in a cutscene held the map blank for seconds). Bursting the calls with a
/// single fixed camera position fills the viewport within one frame instead.
fn prime_stream_layers(ctx: &mut GbaCtx) {
    let (cam_x, cam_y) = (ctx.camera_x, ctx.camera_y);
    let n = ctx.stream_active;
    let dirty = ctx.stream_dirty;
    ctx.stream_dirty = false;
    for i in 0..n {
        // A backdrop layer scrolls at its own fraction of the camera (see `StreamLayer::par`), so
        // it is streamed against ITS position, not the world's. Derived here rather than cached
        // because this runs after the engine wrote the camera in the same step — a backdrop can
        // never trail the world by a frame.
        // A hidden layer is not streamed. Filling it costs the same as filling a visible one —
        // up to 24 `set_scroll_pos` bursts over the whole viewport — and it is pure waste for a map
        // that carries more layers than it lights at once: `examples/spectra` has four (three colour
        // bands plus the world) and shows one or two.
        //
        // ⚠️ A layer made visible again is re-primed by `stream_visible`, which sets `stream_dirty`
        // — without that it would show whatever was last streamed into it, which after any scrolling
        // is the wrong part of the map.
        if !ctx.stream_layers[i].visible {
            continue;
        }
        let (mx, my) = ctx.stream_layers[i].par;
        let cam = Vector2D::new(cam_x * mx / 256, cam_y * my / 256);
        if dirty {
            // Jump ≥ one screen away so InfiniteScrolledMap resets current_pos to None
            // and does a full initial fill for the newly bound map data.
            let far = Vector2D::new(cam.x + 256, cam.y + 256);
            {
                let layer = &mut ctx.stream_layers[i];
                let data = &layer.data;
                let patch = &layer.patch;
                let (w, h, cols) = (layer.w, layer.h, layer.cols);
                let tiles = layer.tiles;
                let settings = layer.settings;
                let _ = layer.map.set_scroll_pos(far, |pos| {
                    provide_tile(pos, data, patch, w, h, cols, tiles, settings)
                });
            }
        }
        // 11 calls cover the 21-row viewport; cap well above that so a change in agb's row
        // budget degrades to a partial fill instead of hanging the frame.
        for _ in 0..24 {
            let status = {
                let layer = &mut ctx.stream_layers[i];
                let data = &layer.data;
                let patch = &layer.patch;
                let (w, h, cols) = (layer.w, layer.h, layer.cols);
                let tiles = layer.tiles;
                let settings = layer.settings;
                layer.map.set_scroll_pos(cam, |pos| {
                    provide_tile(pos, data, patch, w, h, cols, tiles, settings)
                })
            };
            // Long bursts would otherwise starve the mixer and crackle the music.
            pump_audio(ctx);
            if status == PartialUpdateStatus::Done {
                break;
            }
        }
    }
}

/// `tilemap_stream(tilesetHandle, tilesetCols, width, height, data, priority)` — like
/// `tilemap_new` but for maps bigger than the screen: registers a STREAMED layer that the
/// frame loop scrolls to the camera, paging tiles in as it moves (via `InfiniteScrolledMap`).
/// GIDs are kept as i16 to keep large maps affordable. Returns a stream-layer index.
pub fn tilemap_stream(args: &[Value]) -> Value {
    let tileset = num(args, 0) as i32;
    let cols = (num(args, 1) as i32).max(1);
    let w = num(args, 2) as i32;
    let h = num(args, 3) as i32;
    let data: Vec<i16> = match args.get(4) {
        Some(v) => read_i32_array(v).into_iter().map(|x| x as i16).collect(),
        None => Vec::new(),
    };
    let priority = match num(args, 5) as i32 {
        0 => Priority::P0,
        1 => Priority::P1,
        2 => Priority::P2,
        _ => Priority::P3,
    };
    with_ctx(|ctx| {
        let (palettes, tdata) = match tishlang_runtime_gba::gba::asset_bg(tileset) {
            Some(t) => t,
            None => return Value::Null,
        };
        ctx.gfx.set_background_palettes(palettes);
        set_backdrop(ctx, palettes);
        let handle = ctx.stream_layers.len();
        push_stream_layer(
            ctx,
            StreamGids::Owned(data),
            w,
            h,
            cols,
            &tdata.tiles,
            tdata.tile_settings,
            priority,
            PAR_WORLD,
        );
        Value::Number(handle as f64)
    })
}

/// `tilemap_stream_terrain(tilesetHandle, tilesetCols, width, height, ground, blocks, baseId)`
/// — the streamed counterpart of `tilemap_terrain`: autotiles a TERRAIN map into GID layers
/// (base fill P3 + one transparent overlay P2 per non-base terrain) IN RUST, then registers
/// them as streamed layers. Same `autotile_gid` algorithm as the fixed path — no offline/tish
/// tile-picking.
pub fn tilemap_stream_terrain(args: &[Value]) -> Value {
    let tileset = num(args, 0) as i32;
    let cols = (num(args, 1) as i32).max(1);
    let w = num(args, 2) as i32;
    let h = num(args, 3) as i32;
    let ground = match args.get(4) {
        Some(v) => read_i32_array(v),
        None => Vec::new(),
    };
    let blocks = match args.get(5) {
        Some(v) => read_i32_array(v),
        None => Vec::new(),
    };
    let base_id = num(args, 6) as i32;
    with_ctx(|ctx| {
        let (palettes, tdata) = match tishlang_runtime_gba::gba::asset_bg(tileset) {
            Some(t) => t,
            None => return Value::Null,
        };
        ctx.gfx.set_background_palettes(palettes);
        set_backdrop(ctx, palettes);
        let (tiles, settings) = (&tdata.tiles, tdata.tile_settings);
        let n = (w * h).max(0) as usize;
        // Base layer: every cell is the base terrain's centre tile.
        let base_block = blocks
            .get((base_id - 1).max(0) as usize)
            .copied()
            .unwrap_or(0);
        let base_center_gid = (base_block + cols + 1 + 1) as i16;
        push_stream_layer(
            ctx,
            StreamGids::Owned(alloc::vec![base_center_gid; n]),
            w,
            h,
            cols,
            tiles,
            settings,
            Priority::P3,
            PAR_WORLD,
        );
        // Overlay each non-base terrain, autotiled into a GID layer.
        let mut tid = 1;
        while tid <= blocks.len() as i32 {
            if tid != base_id {
                let block = blocks[(tid - 1) as usize];
                let mut data = alloc::vec![0i16; n];
                let mut r = 0;
                while r < h {
                    let mut c = 0;
                    while c < w {
                        let idx = (r * w + c) as usize;
                        if ground.get(idx).copied().unwrap_or(0) == tid {
                            data[idx] = autotile_gid(&ground, w, h, cols, block, tid, c, r) as i16;
                        }
                        c += 1;
                    }
                    r += 1;
                }
                push_stream_layer(
                    ctx,
                    StreamGids::Owned(data),
                    w,
                    h,
                    cols,
                    tiles,
                    settings,
                    Priority::P2,
                    PAR_WORLD,
                );
            }
            tid += 1;
        }
        Value::Null
    })
}

/// Read a little-endian u16 from a byte slice at offset `o`.
fn rd_u16(data: &[u8], o: usize) -> i32 {
    match (data.get(o), data.get(o + 1)) {
        (Some(a), Some(b)) => u16::from_le_bytes([*a, *b]) as i32,
        _ => 0,
    }
}

/// Reinterpret a [`rd_u16`] result as SIGNED — for blob fields that can go negative, like a
/// parallax multiplier for a layer that drifts against the camera.
fn sign16(v: i32) -> i32 {
    v as u16 as i16 as i32
}

/// `map_stream(mapHandle, tilesetHandle)` — render a ROM-baked map (`map:` import) as
/// streamed layers, and stash its solid grid + spawns for `map_solid_at` / `map_spawn_*`.
/// The map data lives in ROM; GID layers are referenced in place (no EWRAM copy). Binary
/// layout (LE): u16 width, height, tilesetCols, nlayers; then per layer u16 priority
/// + width*height u16 gids; then width*height u8 solid; then u16 nspawns + per spawn
/// (i16 col, i16 row, u16 kind, i16 a, i16 b).
pub fn map_stream(args: &[Value]) -> Value {
    let map_handle = num(args, 0) as i32;
    let tileset = num(args, 1) as i32;
    do_map_stream(map_handle, tileset)
}

/// `scene_stream(sceneHandle)` — the `scene:`-import counterpart of `map_stream`: a scene
/// packs its own atlas + map at compile time (see `tish_gba_scenepack::include_scene!`) and
/// registers both halves as one handle via `native_scene_register`; this looks that handle back
/// up into its (map, tileset) pair and streams it exactly like `map_stream` would.
pub fn scene_stream(args: &[Value]) -> Value {
    let handle = num(args, 0) as i32;
    let pair = SCENES.with(|s| s.borrow().get(handle as usize).copied());
    match pair {
        Some(((palettes, tdata), map_idx)) => do_map_stream_resolved(map_idx, palettes, tdata),
        None => Value::Null,
    }
}

/// Stream a `map:` import with its tileset from the `background:` arena (`asset_bg`). Scenes go
/// through [`do_map_stream_resolved`] directly with their own stored tileset (see `SCENES`).
fn do_map_stream(map_handle: i32, tileset: i32) -> Value {
    match tishlang_runtime_gba::gba::asset_bg(tileset) {
        Some((palettes, tdata)) => do_map_stream_resolved(map_handle, palettes, tdata),
        None => Value::Null,
    }
}

fn do_map_stream_resolved(
    map_handle: i32,
    palettes: &'static [Palette16],
    tdata: &'static agb::display::tile_data::TileData,
) -> Value {
    let data = match tishlang_runtime_gba::gba::asset_map(map_handle) {
        Some(d) => d,
        None => return Value::Null,
    };
    let width = rd_u16(data, 0);
    let height = rd_u16(data, 2);
    let cols = rd_u16(data, 4).max(1);
    let nlayers = rd_u16(data, 6);
    let cells = (width * height).max(0) as usize;

    // The fixed part of the blob is all computable from the header, so walk to the trailers FIRST:
    // a layer's parallax multiplier lives back there and is needed when the layer is pushed.
    let layer_stride = 2 + cells * 2;
    let solid_off = 8 + (nlayers.max(0) as usize) * layer_stride;
    let nspawns = rd_u16(data, solid_off + cells);
    let spawns_off = solid_off + cells + 2;

    // Optional trailers, each behind a magic word and each self-sizing, walked until one isn't
    // recognised — so a blob that simply ends after its spawns reads as "has none of these" rather
    // than as garbage, and a new trailer can be added later without invalidating existing ROMs.
    // Written by `tish_gba_scenepack::tiled`; keep the magic words and sizes in step with it.
    let mut oneway_off = None;
    let mut ladder_off = None;
    let mut parallax_off = None;
    let mut t = spawns_off + (nspawns.max(0) as usize) * SPAWN_STRIDE;
    loop {
        let body = t + 2;
        match rd_u16(data, t) {
            MAP_PLANES_MAGIC if data.len() >= body + 2 * cells => {
                oneway_off = Some(body);
                ladder_off = Some(body + cells);
                t = body + 2 * cells;
            }
            MAP_PARALLAX_MAGIC if data.len() >= body + 4 * nlayers.max(0) as usize => {
                parallax_off = Some(body);
                t = body + 4 * nlayers.max(0) as usize;
            }
            _ => break,
        }
    }
    // A layer with no parallax entry tracks the camera exactly, which is what a world layer does.
    let layer_par = |i: i32| -> (i32, i32) {
        match parallax_off {
            Some(o) => (
                sign16(rd_u16(data, o + i as usize * 4)),
                sign16(rd_u16(data, o + i as usize * 4 + 2)),
            ),
            None => (256, 256),
        }
    };

    with_ctx(|ctx| {
        ctx.gfx.set_background_palettes(palettes);
        set_backdrop(ctx, palettes);
        let (tiles, settings) = (&tdata.tiles, tdata.tile_settings);
        let mut off = 8;
        for i in 0..nlayers {
            let priority = match rd_u16(data, off) {
                0 => Priority::P0,
                1 => Priority::P1,
                2 => Priority::P2,
                _ => Priority::P3,
            };
            off += 2;
            let par = layer_par(i);
            if par != PAR_WORLD {
                // A layer Tiled gave a parallax factor is a BACKDROP, and a backdrop is built as a
                // wrapping background, not streamed — see `GbaCtx::scene_bgs` for why it has to be.
                push_scene_backdrop(ctx, data, off, width, cols, tiles, settings, priority, par);
            } else {
                // Point at ROM — do not copy width×height i16s into EWRAM (a large overworld = 38KB).
                push_stream_layer(
                    ctx,
                    StreamGids::Rom { bytes: data, off },
                    width,
                    height,
                    cols,
                    tiles,
                    settings,
                    priority,
                    par,
                );
            }
            off += cells * 2;
            pump_audio(ctx); // a scene load streams several tile layers with no return to frame() — feed the BGM
        }
        ctx.map_info = Some(MapInfo {
            data,
            width,
            height,
            solid_off,
            spawns_off,
            nspawns,
            oneway_off,
            ladder_off,
        });
        Value::Null
    })
}

fn map_field<R>(f: impl FnOnce(&MapInfo) -> R, default: R) -> R {
    with_ctx(|ctx| match ctx.map_info.as_ref() {
        Some(m) => f(m),
        None => default,
    })
}

/// `map_width()` / `map_height()` — dimensions of the loaded ROM map, in tiles.
pub fn map_width(_args: &[Value]) -> Value {
    Value::Number(map_field(|m| m.width, 0) as f64)
}
pub fn map_height(_args: &[Value]) -> Value {
    Value::Number(map_field(|m| m.height, 0) as f64)
}

/// `map_solid_at(col, row)` — 1 if the loaded map marks that cell solid, else 0.
pub fn map_solid_at(args: &[Value]) -> Value {
    let col = num(args, 0) as i32;
    let row = num(args, 1) as i32;
    let s = map_field(
        |m| {
            if col >= 0 && col < m.width && row >= 0 && row < m.height {
                let idx = m.solid_off + (row * m.width + col) as usize;
                m.data.get(idx).copied().unwrap_or(0) as i32
            } else {
                0
            }
        },
        0,
    );
    Value::Number(s as f64)
}

/// The loaded ROM map's collision bytes (one per cell, row-major) with its dimensions, so the
/// engine can build its collision grid in Rust. Reading it cell-by-cell from tish via
/// `map_solid_at` costs a w×h interpreter loop — seconds on a town-sized map.
pub fn native_map_solid_grid() -> Option<(&'static [u8], i32, i32)> {
    map_field(
        |m| {
            let cells = (m.width * m.height).max(0) as usize;
            m.data
                .get(m.solid_off..m.solid_off + cells)
                .map(|s| (s, m.width, m.height))
        },
        None,
    )
}

/// The loaded ROM map's optional ONE-WAY plane, one byte per cell, or `None` when the blob has no
/// plane trailer. Same shape and the same reason as [`native_map_solid_grid`] — a side-scroller's
/// collision has three independent planes, and reading them cell-by-cell from tish would be a w×h
/// interpreter loop on every area load.
pub fn native_map_oneway_grid() -> Option<&'static [u8]> {
    map_field(
        |m| {
            let cells = (m.width * m.height).max(0) as usize;
            m.oneway_off.and_then(|o| m.data.get(o..o + cells))
        },
        None,
    )
}

/// The loaded ROM map's optional LADDER plane. See [`native_map_oneway_grid`].
pub fn native_map_ladder_grid() -> Option<&'static [u8]> {
    map_field(
        |m| {
            let cells = (m.width * m.height).max(0) as usize;
            m.ladder_off.and_then(|o| m.data.get(o..o + cells))
        },
        None,
    )
}

/// `map_spawn_count()` and `map_spawn_col/row/kind/a/b(i)` — the map's entity spawn list.
/// Each spawn is 10 bytes: i16 col, i16 row, u16 kind, i16 a, i16 b.
pub fn map_spawn_count(_args: &[Value]) -> Value {
    Value::Number(map_field(|m| m.nspawns, 0) as f64)
}
const SPAWN_STRIDE: usize = 10;
fn spawn_field(i: i32, byte: usize) -> i32 {
    map_field(
        |m| {
            if i >= 0 && i < m.nspawns {
                let o = m.spawns_off + (i as usize) * SPAWN_STRIDE + byte;
                let raw = rd_u16(m.data, o);
                // col/row/a/b are i16; kind (byte 4) is u16
                if byte != 4 && raw >= 0x8000 {
                    raw - 0x10000
                } else {
                    raw
                }
            } else {
                0
            }
        },
        0,
    )
}
/// `map_spawn_next_in(from, c0, r0, c1, r1)` — the index of the first spawn at or after `from`
/// whose tile lies inside the inclusive rect, or -1 when there are none left.
///
/// This exists because filtering the spawn list from the game side costs a boxed call PER SPAWN
/// just to reject it. Measured on the topdown RPG port's overworld: 437 spawns × 2 reads to find the ~3 that are
/// on the current screen was 0.22s, once per screen change. The scan itself is trivial; it is the
/// crossing that is expensive, so the crossing is what this removes — the caller pays one call per
/// spawn it actually wanted plus one to learn there are no more.
pub fn map_spawn_next_in(args: &[Value]) -> Value {
    let from = num(args, 0) as i32;
    let (c0, r0) = (num(args, 1) as i32, num(args, 2) as i32);
    let (c1, r1) = (num(args, 3) as i32, num(args, 4) as i32);
    Value::Number(map_field(
        |m| {
            let mut i = from.max(0);
            while i < m.nspawns {
                let o = m.spawns_off + (i as usize) * SPAWN_STRIDE;
                let col = sign16(rd_u16(m.data, o));
                let row = sign16(rd_u16(m.data, o + 2));
                if col >= c0 && col <= c1 && row >= r0 && row <= r1 {
                    return i;
                }
                i += 1;
            }
            -1
        },
        -1,
    ) as f64)
}
pub fn map_spawn_col(args: &[Value]) -> Value {
    Value::Number(spawn_field(num(args, 0) as i32, 0) as f64)
}
pub fn map_spawn_row(args: &[Value]) -> Value {
    Value::Number(spawn_field(num(args, 0) as i32, 2) as f64)
}
pub fn map_spawn_kind(args: &[Value]) -> Value {
    Value::Number(spawn_field(num(args, 0) as i32, 4) as f64)
}
pub fn map_spawn_a(args: &[Value]) -> Value {
    Value::Number(spawn_field(num(args, 0) as i32, 6) as f64)
}
pub fn map_spawn_b(args: &[Value]) -> Value {
    Value::Number(spawn_field(num(args, 0) as i32, 8) as f64)
}

/// Native camera set (called by the engine each frame). The camera is the top-left pixel of
/// the view into the world: streamed layers scroll to it and game sprites are drawn relative
/// to it. UI (the dialogue box) is unaffected — it stays in screen space.
pub fn native_camera_set(x: i32, y: i32) {
    with_ctx(|ctx| {
        ctx.camera_x = x;
        ctx.camera_y = y;
    });
}

/// `camera_set(x, y)` — set the camera's top-left world pixel (Value ABI).
pub fn camera_set(args: &[Value]) -> Value {
    native_camera_set(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

/// `camera_x()` / `camera_y()` — the camera's top-left world pixel. There was a setter but no
/// getter, so a game whose camera is driven by the ENGINE (`entity.follow()`) had no way to ask
/// where the view is — which is exactly what anything drawn relative to the world but outside the
/// entity system needs (a parallax layer scrolled by hand, a minimap, an off-screen marker).
pub fn camera_x(_args: &[Value]) -> Value {
    Value::Number(with_ctx(|ctx| ctx.camera_x) as f64)
}
/// See [`camera_x`].
pub fn camera_y(_args: &[Value]) -> Value {
    Value::Number(with_ctx(|ctx| ctx.camera_y) as f64)
}

/// `sprite_clear()` — drop every sprite (frees their VRAM). Used on scene transitions
/// before spawning the next scene's sprites; the engine's `clear_world` despawns the
/// entities that referenced them in the same step. UI icon pools / choice cursors must be
/// re-created afterwards via `uiResetPool` / `dialogReset` (see `packages/engine` `loadScene`).
pub fn sprite_clear(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        ctx.sprites.clear();
        ctx.sprite_free.clear();
        ctx.hud_hearts.clear();
        ctx.hud_hearts_last = (-1, -1);
        // HUD text Objects hold their own VRAM; leaving them alive across a scene_stream
        // was a use-after-free (illegal opcode ~1/4s after hub→dungeon/cave warps).
        // Reset the full cache key too — a stale font/colors match after objs were dropped
        // could skip a rebuild and then show() freed VRAM.
        for slot in ctx.hud_text.iter_mut() {
            slot.objs.clear();
            slot.emoji_objs.clear();
            slot.cache.clear();
            slot.visible = false;
            slot.x = -1;
            slot.y = -1;
            slot.font = -2;
            slot.colors.clear();
            slot.shadow = -2;
            slot.align = 255;
            slot.maxw = -1;
            slot.vgrad = false;
        }
    });
    Value::Null
}

/// `ui_scroll(x, y)` — offset the whole UI canvas, in pixels. This is SCREEN SHAKE.
///
/// It costs two hardware register writes (BGxHOFS / BGxVOFS) and redraws nothing: the display
/// controller fetches tiles at the offset for you. That is why shake is cheap on this machine even
/// when a full repaint is not — the pixels never move in memory, only the window onto them.
///
/// Per-frame is the correct usage here, unlike almost everything else in this file. A shake that
/// updates on keyframes reads as a stutter; the two writes are free.
pub fn ui_scroll(args: &[Value]) -> Value {
    ui_scroll_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

/// Typed-extern form of [`ui_scroll`] — no boxing, for the per-frame shake path.
pub fn ui_scroll_typed(x: i32, y: i32) {
    with_ctx(|ctx| {
        ctx.ui_scroll_x = x;
        ctx.ui_scroll_y = y;
        if let Some(bg) = ctx.ui_bg.as_mut() {
            bg.set_scroll_pos(Vector2D::new(x, y));
        }
    });
}

/// `bg_scroll(handle, x, y)` — set a background's scroll offset in pixels.
pub fn bg_scroll(args: &[Value]) -> Value {
    bg_scroll_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// Typed-extern form of [`bg_scroll`] — called directly (no boxing) from a game that scrolls a
/// background every frame (a starfield, a parallax layer). This is the hot per-frame path.
pub fn bg_scroll_typed(handle: i32, x: i32, y: i32) {
    with_ctx(|ctx| {
        if let Some(b) = ctx.backgrounds.get_mut(handle as usize) {
            // Scrolling a layer by hand takes it off automatic parallax, so the two can't fight
            // over the same scroll register — last caller wins, and it's whichever the game used.
            b.parallax = None;
            b.bg.set_scroll_pos(Vector2D::new(x, y));
        }
    });
}

/// `bg_parallax(handle, mulX, mulY)` — scroll this background automatically at a FRACTION of the
/// camera, in 1/256ths: 256 tracks the camera exactly (what the world layer does), 128 is half
/// speed, 0 pins the layer to the screen, and a negative value drifts it the other way. To go back
/// to driving a layer by hand, call `bg_scroll` — it clears the parallax binding.
///
/// This exists natively rather than as a per-frame `bg_scroll` from tish for two reasons. It is
/// exact: `frame` reads the camera the engine's `update_camera` wrote a few microseconds earlier in
/// the SAME `world_step`, so a layer can never lag the world by a frame. And it is free: a
/// three-layer sky would otherwise be three boxed `value_call`s across the cargo boundary every
/// frame, forever, for arithmetic that is two multiplies.
///
/// Note the GBA has only four background layers and `frame` budgets them (the UI canvas is reserved
/// first) — a parallax stack spends from the same four as the map's tile layers.
pub fn bg_parallax(args: &[Value]) -> Value {
    bg_parallax_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// Typed-extern form of [`bg_parallax`].
pub fn bg_parallax_typed(handle: i32, mul_x: i32, mul_y: i32) {
    with_ctx(|ctx| {
        if let Some(b) = ctx.backgrounds.get_mut(handle as usize) {
            b.parallax = Some((mul_x, mul_y));
        }
    });
}

/// `scene_bands(index, bands)` — give one of the current scene's backdrop layers a DIFFERENT
/// horizontal parallax speed per band of scanlines, so a single background shows several depths.
///
/// Full write-up, including how to measure one of these without fooling yourself:
/// `docs/gba-backgrounds.md`. Worked example: `examples/bands-demo`.
///
/// `index` counts the scene's parallax layers in the order the .tmj emits them (0 = frontmost).
/// `bands` is a flat `[firstRow, mulX, firstRow, mulX, …]`, rows 0..159 top to bottom, `mulX` in the
/// same 1/256ths as `bg_parallax`. Each band runs to the next one's first row; the first band also
/// covers anything above it. `scene_bands(i, [])` puts the layer back to scrolling as one piece.
///
/// This is how you get more depth than you have layers. The GBA has four backgrounds and a game with
/// menus has already spent one on the UI canvas and one on the world, so two is the ceiling for
/// separate parallax layers — but a sky that scrolls at 1/16 above the horizon and at 1/4 below it is
/// two depths on ONE layer, and costs a 320-byte table per frame instead of a background.
///
/// ⚠️ ONLY ONE layer can be banded. agb's `GraphicsFrame` holds a single DMA slot and its HBlank
/// transfer hardcodes DMA channel 0, so a second banded layer would silently replace the first. The
/// first banded layer in scene order wins; the rest scroll normally.
///
/// ⚠️ Bands work because a backdrop WRAPS in hardware every 256px. They cannot be applied to the
/// world layer, which is streamed and only has a 256x256 window of tiles resident.
pub fn scene_bands(args: &[Value]) -> Value {
    let idx = num(args, 0) as i32;
    let bands = parse_bands(args, 1);
    with_ctx(|ctx| {
        if let Some(b) = ctx.scene_bgs.get_mut(idx.max(0) as usize) {
            b.bands = bands;
        }
    });
    Value::Null
}

/// `bg_bands(handle, bands)` — the same per-scanline parallax as [`scene_bands`], for a background
/// the game built itself with `tilemap_new` / `bg_new` rather than one that came out of a `scene:`
/// map. Same `[firstRow, mulX, …]` shape, same one-layer-only limit (they share the DMA channel).
pub fn bg_bands(args: &[Value]) -> Value {
    let handle = num(args, 0) as i32;
    let bands = parse_bands(args, 1);
    with_ctx(|ctx| {
        if let Some(b) = ctx.backgrounds.get_mut(handle.max(0) as usize) {
            b.bands = bands;
        }
    });
    Value::Null
}

/// Read a flat `[firstRow, mulX, …]` tish array into sorted `(row, mul)` bands. Fewer than two
/// numbers means "no bands" — the way a game turns banding back off.
fn parse_bands(args: &[Value], at: usize) -> Option<alloc::vec::Vec<(u8, i16)>> {
    let flat = match args.get(at) {
        Some(v) => read_i32_array(v),
        None => Vec::new(),
    };
    if flat.len() < 2 {
        return None;
    }
    let mut v: alloc::vec::Vec<(u8, i16)> = alloc::vec::Vec::new();
    let mut i = 0;
    while i + 1 < flat.len() {
        v.push((flat[i].clamp(0, 159) as u8, flat[i + 1] as i16));
        i += 2;
    }
    v.sort_by_key(|e| e.0);
    Some(v)
}

/// Expand `bands` to one horizontal scroll value per scanline at camera x `cam_x`, and hand the
/// table to the frame as an HBlank DMA on `id`'s scroll register. See [`scene_bands`].
fn attach_band_dma(
    frame: &mut agb::display::GraphicsFrame,
    id: agb::display::tiled::RegularBackgroundId,
    bands: &[(u8, i16)],
    cam_x: i32,
) {
    let mut rows = [0u16; 160];
    let mut bi = 0usize;
    for (y, slot) in rows.iter_mut().enumerate() {
        while bi + 1 < bands.len() && (bands[bi + 1].0 as usize) <= y {
            bi += 1;
        }
        *slot = (cam_x * bands[bi].1 as i32 / 256) as u16;
    }
    agb::dma::HBlankDma::new(id.x_scroll_dma(), &rows).show(frame);
}

/// `scene_bg_visible(index, visible)` — show/hide one of the scene's backdrop layers. Hiding really
/// does hand its slot back (the budget in `frame` counts only visible layers), which is how an
/// interior with no sky can afford a layer the outdoors could not.
pub fn scene_bg_visible(args: &[Value]) -> Value {
    let idx = num(args, 0) as i32;
    let on = num(args, 1) != 0.0;
    with_ctx(|ctx| {
        if let Some(b) = ctx.scene_bgs.get_mut(idx.max(0) as usize) {
            b.visible = on;
        }
    });
    Value::Null
}

/// `stream_visible(layer, visible)` — show or hide one of the scene's STREAMED map layers, indexed
/// the same way `bg_set_tile` indexes them (0 = frontmost, in the order the .tmj emits).
///
/// The counterpart to [`scene_bg_visible`], which only reaches a scene's wrapping parallax
/// backdrops. A hidden layer is skipped in the frame loop and hands its background slot back, so a
/// map may carry more layers than the four the hardware can show as long as they are not all lit
/// together — which is what lets `examples/spectra` give each of its three colour bands a layer.
pub fn stream_visible(args: &[Value]) -> Value {
    stream_visible_typed(num(args, 0) as i32, if num(args, 1) != 0.0 { 1 } else { 0 });
    Value::Null
}

/// Typed-extern form of [`stream_visible`].
pub fn stream_visible_typed(layer: i32, visible: i32) {
    with_ctx(|ctx| {
        let mut woke = false;
        if let Some(l) = ctx.stream_layers.get_mut(layer.max(0) as usize) {
            woke = visible != 0 && !l.visible;
            l.visible = visible != 0;
        }
        // A hidden layer was not being streamed (see `prime_stream_layers`), so it holds whatever
        // was last filled into it — which after any scrolling is the wrong part of the map. Marking
        // the set dirty forces the full re-fill on the next frame.
        if woke {
            ctx.stream_dirty = true;
        }
    });
}

/// `obj_pal_get(bank, index)` / `obj_pal_set(bank, index, 0xRRGGBB)` — read and write SPRITE palette
/// entries, straight into OBJ palette RAM at 0x05000200.
///
/// ⚠️ THIS IS THE ONLY WAY TO MAKE A SPRITE FOLLOW A PALETTE SWAP. agb exposes background palette
/// setters and nothing equivalent for objects — a sprite's palette is uploaded by the sprite loader
/// and then fixed. So a game that repaints its background palette leaves its characters behind, in
/// colours that no longer belong to the scene.
///
/// On a Game Boy the four shades are shared by everything on screen, sprites included; this is what
/// lets that hold here. `examples/prismfall` uses it to recolour the player with the lens instead of
/// reducing them to a one-colour silhouette.
///
/// Bank is the 16-colour block the sprite's palette landed in, index 1..15 (0 is transparent). Use
/// `obj_pal_get` to find which entry holds a known authored colour rather than assuming an index —
/// the assignment is as unpredictable as the background side's.
pub fn obj_pal_get(args: &[Value]) -> Value {
    Value::Number(obj_pal_get_typed(num(args, 0) as i32, num(args, 1) as i32) as f64)
}

/// See [`obj_pal_get`].
pub fn obj_pal_get_typed(bank: i32, index: i32) -> i32 {
    if !(0..16).contains(&bank) || !(0..16).contains(&index) {
        return -1;
    }
    let addr = (0x0500_0200 + (bank as usize * 16 + index as usize) * 2) as *const u16;
    let v = unsafe { addr.read_volatile() };
    let r = ((v & 31) as i32) << 3;
    let g = (((v >> 5) & 31) as i32) << 3;
    let b = (((v >> 10) & 31) as i32) << 3;
    (r << 16) | (g << 8) | b
}

/// See [`obj_pal_get`].
pub fn obj_pal_set(args: &[Value]) -> Value {
    obj_pal_set_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// See [`obj_pal_get`].
pub fn obj_pal_set_typed(bank: i32, index: i32, colour: i32) {
    if !(0..16).contains(&bank) || !(1..16).contains(&index) {
        return;
    }
    let r = ((colour >> 19) & 31) as u16;
    let g = ((colour >> 11) & 31) as u16;
    let b = ((colour >> 3) & 31) as u16;
    let v = r | (g << 5) | (b << 10);
    let addr = (0x0500_0200 + (bank as usize * 16 + index as usize) * 2) as *mut u16;
    unsafe { addr.write_volatile(v) };
}

/// `bg_pal_get(bank, index)` — read back a background palette entry as `0xRRGGBB`, or -1 if unknown.
///
/// ⚠️ THIS EXISTS TO MAKE PALETTE SWAPPING POSSIBLE AT ALL. Which index holds which of an atlas's
/// colours is decided by agb's optimiser and is **nondeterministic across builds** — same source,
/// two clean builds, a colour moves from entry 5 to entry 9. So a game cannot hardcode "entry 7 is
/// the stone", and measuring it once and baking a table is invalidated by the next build.
///
/// Reading it back at runtime settles it: a game looks up the entries holding its authored colours
/// during boot, then rewrites exactly those entries whenever it wants a different palette. That is
/// what lets `examples/prismfall` swap ALL FOUR of its colours when the player turns the lantern,
/// rather than only the backdrop.
///
/// The value is expanded from the hardware's 15-bit colour, so it is the authored colour rounded to
/// 5 bits per channel — compare with the same rounding applied.
pub fn bg_pal_get(args: &[Value]) -> Value {
    Value::Number(bg_pal_get_typed(num(args, 0) as i32, num(args, 1) as i32) as f64)
}

/// Typed-extern form of [`bg_pal_get`].
pub fn bg_pal_get_typed(bank: i32, index: i32) -> i32 {
    with_ctx(|ctx| {
        let Some(pals) = ctx.bg_pal else { return -1 };
        let Some(p) = pals.get(bank.max(0) as usize) else {
            return -1;
        };
        if !(0..16).contains(&index) {
            return -1;
        }
        let v = p.colour(index as usize).0;
        let r = ((v & 31) as i32) << 3;
        let g = (((v >> 5) & 31) as i32) << 3;
        let b = (((v >> 10) & 31) as i32) << 3;
        (r << 16) | (g << 8) | b
    })
}

/// `bg_set_visible(handle, visible)` — show (non-zero) or hide (0) a background.
pub fn bg_set_visible(args: &[Value]) -> Value {
    bg_set_visible_typed(num(args, 0) as i32, if num(args, 1) != 0.0 { 1 } else { 0 });
    Value::Null
}

/// Typed-extern form of [`bg_set_visible`] — `visible` non-zero shows the layer, zero hides it.
pub fn bg_set_visible_typed(handle: i32, visible: i32) {
    with_ctx(|ctx| {
        if let Some(b) = ctx.backgrounds.get_mut(handle as usize) {
            b.visible = visible != 0;
        }
    });
}

/// `bg_use_palettes(handle)` — re-upload this background's own palettes to the shared BG palette
/// bank, making it the layer that looks correct.
///
/// The GBA has a single sixteen-entry-by-sixteen background palette bank, and `bg_new` uploads the
/// palettes of whichever asset was created last. That is fine while a scene owns one background,
/// and wrong the moment a game keeps two around and shows them at different times: the first one
/// silently renders in the second one's colours.
///
/// The alternative is `bg_clear` plus a rebuild per switch, which throws away the tile allocation
/// and is exactly the churn that fragments EWRAM into "allocation of N bytes failed" a few
/// transitions later. This costs one palette DMA and keeps both layers alive.
///
///     bg_set_visible(board, 0)
///     bg_set_visible(title, 1)
///     bg_use_palettes(title)      // now the title's colours are the ones on screen
pub fn bg_use_palettes(args: &[Value]) -> Value {
    bg_use_palettes_typed(num(args, 0) as i32);
    Value::Null
}

/// Typed-extern form of [`bg_use_palettes`].
pub fn bg_use_palettes_typed(handle: i32) {
    with_ctx(|ctx| {
        let Some(b) = ctx.backgrounds.get(handle as usize) else {
            return;
        };
        let asset = b.asset;
        // Scene/autotile layers come from a blob rather than one `background:` asset and record
        // -1: there is no single palette set that belongs to them.
        if asset < 0 {
            return;
        }
        let Some((palettes, _)) = tishlang_runtime_gba::gba::asset_bg(asset) else {
            return;
        };
        ctx.gfx.set_background_palettes(palettes);
        set_backdrop(ctx, palettes);
    });
}

/// `fade(level)` — screen fade toward black via the GBA hardware brightness blend (BLDY), where
/// `level` is 0 (fully visible) .. 16 (fully black). Applied to every background, sprite and the
/// backdrop each frame, so it's the whole-screen dim a scene transition drives (ramp 0→16 to fade out,
/// 16→0 to fade in). Costs nothing at level 0 (the blend is skipped). Values clamp to 0..16.
pub fn fade(args: &[Value]) -> Value {
    fade_typed(num(args, 0) as i32);
    Value::Null
}

/// Typed-extern form of [`fade`] — the hot path a transition ramps every frame.
pub fn fade_typed(level: i32) {
    with_ctx(|ctx| ctx.fade = level.clamp(0, 16) as u8);
}

/// `fade_white(level)` — the same whole-screen ramp as [`fade`], toward WHITE instead of black
/// (BLDY increase), `level` 0 (visible) .. 16 (fully white).
///
/// `fx_flash` also brightens, but it DECAYS on its own — it is a hit-spark, fired once and
/// forgotten. A transition owns its ramp frame by frame, so it needs a level that stays put.
///
/// ⚠️ One BLDY register: `fade` wins over this, and this wins over `fx_flash`. Ramping both a fade
/// and a white fade at once shows only the fade.
pub fn fade_white(args: &[Value]) -> Value {
    fade_white_typed(num(args, 0) as i32);
    Value::Null
}

/// See [`fade_white`].
pub fn fade_white_typed(level: i32) {
    with_ctx(|ctx| ctx.fade_white = level.clamp(0, 16) as u8);
}

/// `blend_alpha(top, bottom)` — alpha blending (BLDALPHA), each weight 0..16 (16 = full).
///
/// The top layer here is every sprite whose graphics mode is AlphaBlending, the bottom is every
/// shown background plus the backdrop — so this is the register behind a ghost, a glass pane or a
/// dimmed HUD panel. Pass a negative weight to switch it off.
///
/// ⚠️ NOT a scene crossfade. Blending needs BOTH images resident at once, and the scene lifecycle
/// deliberately tears the old scene down before building the new one (agb does not return a scene's
/// tile block until a frame boundary). This blends layers WITHIN one scene.
///
/// ⚠️ Lowest priority on BLDCNT: a live `fade`, `fade_white` or `fx_flash` all suppress it.
pub fn blend_alpha(args: &[Value]) -> Value {
    blend_alpha_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

/// See [`blend_alpha`].
pub fn blend_alpha_typed(top: i32, bottom: i32) {
    with_ctx(|ctx| {
        ctx.blend_alpha = if top < 0 || bottom < 0 {
            None
        } else {
            Some((top.clamp(0, 16) as u8, bottom.clamp(0, 16) as u8))
        };
    });
}

/// `ui_fill_cells(x, y, w, h, color)` — fill whole CANVAS CELLS with a solid colour, always through
/// the shared-solid-tile path.
///
/// This exists because `ui_rect`'s filled path picks its strategy by SIZE: anything 48 pixels tall
/// or less is drawn as exact pixels (right for a button, a chip, an HP track — chrome that must land
/// on the pixel), and only taller fills snap to cells and share one tile. A transition curtain is
/// the other shape entirely: it is opaque, it is enormous, it does not care about a pixel, and it is
/// repainted every frame — so drawn as chrome it allocates a tile per cell per frame and empties the
/// tile allocator mid-transition. `packages/transition`'s rain and checker both hit exactly that.
///
/// So: same shared tile as a big `ui_rect`, no size gate. The rect is snapped OUT to whole cells
/// (a curtain must not leave a bright seam), which is the one behavioural difference from `ui_rect`
/// and the reason this is a separate call rather than a flag.
pub fn ui_fill_cells(args: &[Value]) -> Value {
    ui_fill_cells_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
    );
    Value::Null
}

/// See [`ui_fill_cells`].
pub fn ui_fill_cells_typed(x: i32, y: i32, w: i32, h: i32, color: i32) {
    if w <= 0 || h <= 0 {
        return;
    }
    with_ctx(|ctx| {
        ui_grid_ready(ctx);
        if ctx.ui_bg.is_none() {
            return;
        }
        let pal = ensure_ui_palette(ctx, color);
        ui_ensure_solid(ctx, pal);
        let si = (pal as usize) % UI_SOLID_N;
        let mark = UI_CELL_SOLID_LO + (si as u16);
        // Snap OUT, not in: a curtain that stops short of a cell boundary shows a lit seam.
        let tx0 = x.div_euclid(8);
        let ty0 = y.div_euclid(8);
        let tx1 = (x + w - 1).div_euclid(8);
        let ty1 = (y + h - 1).div_euclid(8);
        let mut ty = ty0;
        while ty <= ty1 {
            let mut tx = tx0;
            while tx <= tx1 {
                let idx = ui_cell_idx(tx, ty);
                // Already this exact solid? Then there is nothing to do — which is what makes a
                // full repaint every frame affordable.
                if ctx.ui_cell.get(idx).copied() != Some(mark) {
                    ui_drop_tile(ctx, idx);
                    if let (Some(bg), Some(solid)) =
                        (ctx.ui_bg.as_mut(), ctx.ui_solids[si].as_ref())
                    {
                        bg.set_tile_dynamic16(
                            Vector2D::new(tx, ty),
                            solid,
                            TileEffect::default().palette(UI_PAL_SLOT),
                        );
                    }
                    if let Some(slot) = ctx.ui_cell.get_mut(idx) {
                        *slot = mark;
                    }
                }
                tx += 1;
            }
            ty += 1;
        }
    });
}

/// `mosaic(bg, obj)` — pixelate backgrounds by `bg` and sprites by `obj`, each 0 (off) .. 15
/// (16-pixel blocks). The register behind a pixelate-dissolve transition and a "materialising"
/// sprite.
///
/// Costs nothing but the register: the hardware does the blocking as it scans out, so a mosaic
/// dissolve is free where a software one would redraw the screen every frame.
///
/// ⚠️ Unlike every other display native this one is written by hand rather than through agb, which
/// models the MOSAIC size register and both enable bits (BGxCNT bit 6, OAM attr0 bit 12) as
/// permanently off with no API. See `apply_mosaic`.
pub fn mosaic(args: &[Value]) -> Value {
    mosaic_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

/// See [`mosaic`].
pub fn mosaic_typed(bg: i32, obj: i32) {
    with_ctx(|ctx| {
        ctx.mosaic_bg = bg.clamp(0, 15) as u8;
        ctx.mosaic_obj = obj.clamp(0, 15) as u8;
    });
}

/// Write MOSAIC and its enable bits, called from `frame()` immediately after `frame.commit()`.
///
/// That timing is the whole trick. agb rewrites BG0CNT..BG3CNT and all of OAM inside `commit()`,
/// clearing the mosaic enable bits every time, and `commit()` ends having waited for vblank — so
/// the moment it returns we are inside vblank, agb is done writing, and nothing else will touch
/// these registers until the next commit re-pokes them. Writing OAM here is also the only safe
/// window: outside vblank it tears.
///
/// Enable bits are only ever SET when the corresponding size is non-zero and, just as importantly,
/// CLEARED when it is zero — a dissolve that ends must actually end.
fn apply_mosaic(bg: u8, obj: u8) {
    const REG_MOSAIC: *mut u16 = 0x0400_004C as *mut u16;
    const REG_BG_CNT: *mut u16 = 0x0400_0008 as *mut u16;
    const OAM: *mut u16 = 0x0700_0000 as *mut u16;
    let size = ((bg as u16) & 0xF)
        | (((bg as u16) & 0xF) << 4)
        | (((obj as u16) & 0xF) << 8)
        | (((obj as u16) & 0xF) << 12);
    unsafe {
        core::ptr::write_volatile(REG_MOSAIC, size);
        for i in 0..4 {
            let reg = REG_BG_CNT.add(i);
            let cur = core::ptr::read_volatile(reg);
            let next = if bg != 0 {
                cur | (1 << 6)
            } else {
                cur & !(1 << 6)
            };
            if next != cur {
                core::ptr::write_volatile(reg, next);
            }
        }
        // attr0 of each of the 128 OAM entries; entries are 8 bytes, so stride 4 u16s.
        for i in 0..128 {
            let reg = OAM.add(i * 4);
            let cur = core::ptr::read_volatile(reg);
            let next = if obj != 0 {
                cur | (1 << 12)
            } else {
                cur & !(1 << 12)
            };
            if next != cur {
                core::ptr::write_volatile(reg, next);
            }
        }
    }
}

// ── Hardware windows ─────────────────────────────────────────────────────────────────────────────
//
// The GBA's window unit answers one question per pixel: which layers am I allowed to draw here?
// Two rectangles (WIN0, WIN1) say what happens INSIDE them, and WINOUT says what happens everywhere
// else. That single mechanism is a spotlight, a lit room, an iris wipe, and a stealth vision cone.
//
// Nothing in this crate exposed it before, which is why `docs/` lists "no window registers" as one
// of the four capability gaps in the engine. `fade` and `fx_flash` (the BLDY blend) are whole-screen
// only; there was no way to darken a level and cut a hole in the darkness.
//
// THE MASK IS BY DRAW ORDER, NOT BY HANDLE. Bit 0..3 select the nth background that is actually
// SHOWN this frame, which is the order `ctx.bg_ids_buf` is built in. Handles cannot be used: a
// background's slot depends on what else is visible (see the budget in `frame()`), so a mask keyed
// to a handle would silently point at a different layer the moment a dialog opened.
//
//   bit 0..3  the nth shown background      bit 4  objects (sprites)      bit 5  blending

/// `win_rect(id, x, y, w, h)` — enable window `id` (0 or 1) over that screen rectangle.
pub fn win_rect(args: &[Value]) -> Value {
    win_rect_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
    );
    Value::Null
}

/// See [`win_rect`].
pub fn win_rect_typed(id: i32, x: i32, y: i32, w: i32, h: i32) {
    if !(0..2).contains(&id) {
        return;
    }
    with_ctx(|ctx| {
        ctx.win_on[id as usize] = true;
        ctx.win_box[id as usize] = (x, y, w.max(0), h.max(0));
        if id == 0 {
            ctx.win_circle = None;
        }
    });
}

/// `win_circle(cx, cy, r)` — WIN0 as a CIRCLE rather than a rectangle: a spotlight, or an iris.
///
/// The hardware window is rectangular, so the circle is made by rewriting WIN0's horizontal extent
/// on every scanline through an HBlank DMA — the left and right edges of the circle at that row.
/// Set `r` to 0 to close the iris completely; the effect a game usually wants is to ramp it.
///
/// ⚠️ ONE HBLANK DMA PER FRAME. `bg_bands` (banded parallax) uses the same single slot, so a circle
/// window and a banded layer cannot coexist on one frame.
pub fn win_circle(args: &[Value]) -> Value {
    win_circle_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

/// See [`win_circle`].
pub fn win_circle_typed(cx: i32, cy: i32, r: i32) {
    with_ctx(|ctx| {
        ctx.win_on[0] = true;
        ctx.win_circle = Some((cx, cy, r.max(0)));
    });
}

/// `win_off(id)` — disable window `id`. With both off and `win_out_layers` never called, the screen
/// is back to normal: everything draws everywhere.
pub fn win_off(args: &[Value]) -> Value {
    win_off_typed(num(args, 0) as i32);
    Value::Null
}

/// See [`win_off`].
pub fn win_off_typed(id: i32) {
    if !(0..2).contains(&id) {
        return;
    }
    with_ctx(|ctx| {
        ctx.win_on[id as usize] = false;
        if id == 0 {
            ctx.win_circle = None;
        }
    });
}

/// `win_in_layers(id, mask)` — which layers draw INSIDE window `id`. Defaults to everything.
pub fn win_in_layers(args: &[Value]) -> Value {
    win_in_layers_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

/// See [`win_in_layers`].
pub fn win_in_layers_typed(id: i32, mask: i32) {
    if !(0..2).contains(&id) {
        return;
    }
    with_ctx(|ctx| ctx.win_in_mask[id as usize] = (mask & 0x3F) as u8);
}

/// `win_out_layers(mask)` — which layers draw OUTSIDE every window. This is the darkness: pass 0
/// and everything not inside a window disappears to the backdrop colour.
pub fn win_out_layers(args: &[Value]) -> Value {
    win_out_layers_typed(num(args, 0) as i32);
    Value::Null
}

/// See [`win_out_layers`].
pub fn win_out_layers_typed(mask: i32) {
    with_ctx(|ctx| ctx.win_out_mask = (mask & 0x3F) as u8);
}

/// Integer square root, for the circle window's per-scanline half-width. No float, no division.
fn isqrt_u32(n: u32) -> u32 {
    let mut rem = n;
    let mut root = 0u32;
    let mut bit = 1u32 << 30;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        let t = root + bit;
        if rem >= t {
            rem -= t;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

/// `MovableWindow` and `Window` share the same `enable_*` surface but not a trait, so the layer
/// unpack is written once per type rather than made generic over two inherent impls.
fn apply_movable(
    w: &mut agb::display::MovableWindow,
    mask: u8,
    ids: &[agb::display::tiled::RegularBackgroundId],
) {
    for (i, id) in ids.iter().enumerate() {
        if i < 4 && (mask >> i) & 1 != 0 {
            w.enable_background(*id);
        }
    }
    if mask & 0x10 != 0 {
        w.enable_objects();
    }
    if mask & 0x20 != 0 {
        w.enable_blending();
    }
}

// ── Localised strings ────────────────────────────────────────────────────────────────────────────
//
// A `strings:` import bakes a multi-language table into ROM and hands back a handle. Ids are
// POSITIONS in the file, identical across languages by construction (the macro refuses to compile a
// file whose translations disagree on how many strings there are), so switching language cannot
// shift the text under a running game.
//
// These stay BOXED on purpose. A typed extern returns an i32, and these return a string; more to the
// point, string lookup is menu-and-dialogue work that happens when a screen changes, not per frame.
// The engine already has `text_draw`/`ui_text` for the drawing.

/// `str_get(handle, lang, id)` — one string. Out of range yields `""`.
pub fn str_get(args: &[Value]) -> Value {
    Value::string(strings::strings_get(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    ))
}

/// `str_count(handle)` — how many strings each language defines.
pub fn str_count(args: &[Value]) -> Value {
    Value::Number(strings::strings_count(num(args, 0) as i32) as f64)
}

/// `str_langs(handle)` — how many languages the table carries.
pub fn str_langs(args: &[Value]) -> Value {
    Value::Number(strings::strings_langs(num(args, 0) as i32) as f64)
}

/// `str_lang_name(handle, lang)` — `"en"`, `"ja"`, … or `""`.
pub fn str_lang_name(args: &[Value]) -> Value {
    Value::string(strings::strings_lang_name(
        num(args, 0) as i32,
        num(args, 1) as i32,
    ))
}

/// `str_find_lang(handle, name)` — the index of a language by name, or -1, so a game can restore a
/// saved preference without hard-coding the order its `.strings` file happens to use.
pub fn str_find_lang(args: &[Value]) -> Value {
    let name = args
        .get(1)
        .map(|v| v.to_display_string())
        .unwrap_or_default();
    Value::Number(strings::strings_find_lang(num(args, 0) as i32, &name) as f64)
}

/// `fx_flash(level)` — screen flash toward WHITE via the hardware brightness blend (BLDY), 0..16.
///
/// The counterpart to [`fade`], which darkens. Set it and forget it: the level decays by one each
/// frame, so a win can fire `fx_flash(12)` once and the flash falls off on its own. Costs nothing
/// at 0, exactly like the fade.
pub fn fx_flash(args: &[Value]) -> Value {
    fx_flash_typed(num(args, 0) as i32);
    Value::Null
}

pub fn fx_flash_typed(level: i32) {
    with_ctx(|ctx| {
        ctx.flash = level.clamp(0, 16) as u8;
        ctx.flash_decay = 1;
    });
}

/// Shake spring constant and default damping, in the `>> 8` scheme of
/// `v += (-K*x - D*v) >> 8; x += v`. K sets frequency, D damping. These are `packages/feel.tish`'s
/// original tuning unchanged — a period of roughly eight frames settling inside thirty, which reads
/// as an impact — and they are the values three shipped games were already tuned against.
const SHAKE_K: i32 = 128;
const SHAKE_D: i32 = 90;

/// `fx_bump(ax, ay)` — push the screen-shake spring, in 8.8 pixels of velocity.
///
/// The primitive the other two shake calls are built on, and the one to reach for when impacts can
/// land together: bumps ADD. Three bumps in one frame sum into one bigger, longer shake rather than
/// the third silently cancelling the decay of the first two.
///
/// This is the same spring `packages/feel.tish` shipped — `feelBump` now delegates straight here —
/// with `SHAKE_K`/`SHAKE_D` at feel's original tuning, so a game that moved over sees the motion it
/// already had, now on the camera and HUD sprites as well as the canvas.
pub fn fx_bump(args: &[Value]) -> Value {
    fx_bump_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

pub fn fx_bump_typed(ax: i32, ay: i32) {
    with_ctx(|ctx| {
        // Clamped per call, not in total, because summing is the point. The cap is per-impulse
        // insurance against a caller passing a scaled magnitude it never bounded.
        ctx.shake_vx = (ctx.shake_vx + ax.clamp(-4096, 4096)).clamp(-16384, 16384);
        ctx.shake_vy = (ctx.shake_vy + ay.clamp(-4096, 4096)).clamp(-16384, 16384);
    });
}

/// `ln(2 * power) * 256`, indexed by `power` in 0..=16. A 17-entry table instead of a logarithm:
/// this is `no_std` on an ARM7 with no FPU, and `power` is clamped into exactly this range anyway.
const SHAKE_LN2P: [i32; 17] = [
    0, 177, 355, 459, 532, 589, 636, 676, 710, 740, 767, 791, 814, 834, 853, 871, 887,
];

/// Impulse used to calibrate the spring. Any value works — the system is linear, so one run at a
/// reference impulse gives the peak-per-unit-impulse for a given damping.
const SHAKE_CAL_REF: i32 = 8192;

/// Peak |displacement| of the shake spring for `SHAKE_CAL_REF`, in 8.8 pixels, at damping `d`.
///
/// This exists because DAMPING EATS THE PEAK, and by a lot — enough that a fixed impulse-per-pixel
/// constant is wrong at both ends of the range. The first version of `fx_shake` used one, and it
/// made `fx_shake(2, 8)` move the screen by ZERO pixels while `fx_shake(16, 88)` overshot to 27.
/// A silent no-op is the exact failure this whole effect layer has been bitten by before.
///
/// Runs at most 32 iterations, and only on a call to `fx_shake` — never per frame. The peak always
/// lands within the first few frames (a quarter period), so the loop is over long before that.
fn shake_unit_peak(d: i32) -> i32 {
    let (mut x, mut v, mut peak) = (0i32, SHAKE_CAL_REF, 0i32);
    for _ in 0..32 {
        let a = ((-(SHAKE_K * x)) - d * v) >> 8;
        v += a;
        x += v;
        if x.abs() > peak {
            peak = x.abs();
        }
        if (x >> 8) == 0 && v.abs() < 256 {
            break;
        }
    }
    peak.max(1)
}

/// `fx_shake(power, frames)` — a shake that peaks at about `power` pixels and is still again in
/// about `frames` frames.
///
/// The convenience form: a game that wants "hit it this hard, be still again by then" should not
/// have to think in spring impulses. Both arguments are converted onto the one spring `fx_bump`
/// drives, so there is a single integrator in the ROM no matter which call site fires.
///
/// Both arguments together set the DAMPING — a 12-pixel swing needs longer to decay than a 2-pixel
/// one, so `frames` alone cannot determine it. The envelope goes like `e^(-D*t/512)`, and falling
/// from `power` pixels to under half a pixel in `frames` frames wants `D = 512*ln(2*power)/frames`.
/// The impulse is then calibrated against that damping so `power` really means pixels.
///
/// The last caller wins the settle time, which is deliberate and is the reason `fx_bump` exists for
/// anything that fires repeatedly. `power` is an impulse and therefore ADDS, like a bump.
pub fn fx_shake(args: &[Value]) -> Value {
    fx_shake_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

pub fn fx_shake_typed(power: i32, frames: i32) {
    let power = power.clamp(0, 16);
    let frames = frames.clamp(1, 600);
    // Zero power is nothing, not a stop — `fx_shake_stop` is the stop, and silently cancelling a
    // running shake from a call that reads as "shake by nothing" would be a trap.
    if power == 0 {
        return;
    }
    with_ctx(|ctx| {
        // Clamped well under the critical value (2*sqrt(K*256) = 362 at K=128) so it always
        // oscillates: an overdamped spring does not shake, it lurches once and creeps back.
        ctx.shake_d = (2 * SHAKE_LN2P[power as usize] / frames).clamp(24, 220);
        // Scale the reference impulse so the peak comes out at `power` pixels. The +128 is half a
        // pixel: round to the requested peak rather than truncating to one below it, which is the
        // difference between `fx_shake(1, 6)` being a one-pixel tick and being invisible.
        let imp = (SHAKE_CAL_REF * ((power << 8) + 128)) / shake_unit_peak(ctx.shake_d);
        // Randomise the sign so a single call reads as a shake and not as a diagonal lurch always
        // in the same direction. The Y impulse is halved: a screen that moves more horizontally
        // than vertically reads as an impact rather than as a scroll glitch.
        ctx.shake_seed = ctx
            .shake_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let r = (ctx.shake_seed >> 16) as i32;
        let sx = if r & 1 == 0 { imp } else { -imp };
        let sy = if r & 2 == 0 { imp / 2 } else { -imp / 2 };
        ctx.shake_vx = (ctx.shake_vx + sx).clamp(-16384, 16384);
        ctx.shake_vy = (ctx.shake_vy + sy).clamp(-16384, 16384);
    });
}

/// `fx_shake_stop()` — settle the shake NOW and put every surface back square.
///
/// Scene teardown wants this and nothing else: `fx_clear` also kills particles and the flash, which
/// is wrong for a caller that only owns the shake. A scene changed mid-shake would otherwise inherit
/// a displaced canvas and camera.
pub fn fx_shake_stop(args: &[Value]) -> Value {
    let _ = args;
    fx_shake_stop_typed();
    Value::Null
}

pub fn fx_shake_stop_typed() {
    with_ctx(|ctx| {
        ctx.shake_x = 0;
        ctx.shake_vx = 0;
        ctx.shake_y = 0;
        ctx.shake_vy = 0;
        // Back to the default damping. `fx_shake` sets it from its arguments and the last caller
        // wins, which is fine WITHIN a scene but should not leak across one — otherwise a result
        // screen's `fx_shake(4, 18)` silently retunes every `feelBump` in the next scene, whose
        // presets were tuned against SHAKE_D. Teardown is the natural place to put it back.
        ctx.shake_d = SHAKE_D;
        // `shake_live` is deliberately LEFT SET: the next `frame()` sees a settled spring, takes the
        // landing branch, and writes the surfaces square exactly once. Clearing it here would skip
        // that write and leave the canvas on its last jittered offset forever.
    });
}

// ── The effects budget ─────────────────────────────────────────────────────────────────────────
//
// THIS IS THE PART A GAME SHOULD NOT HAVE TO WRITE, and the reason the emitters below live in the
// engine rather than in a package.
//
// The GBA has 128 OAM entries for the WHOLE machine: the player, every NPC, the HUD, every
// `text_draw` slot, and every particle. Nothing arbitrates them. A 48-particle burst is 37.5% of the
// entire budget, so a victory effect fired in a town with sixteen NPCs does not "look busy" — it
// makes NPCs vanish, or comes out empty, depending only on which allocated first. That failure is
// invisible in the effect's own demo, where nothing else is on screen.
//
// So the library measures instead of hoping. Every spawn asks `fx_headroom_of`, which reads what the
// GAME is currently holding and hands back only what is genuinely spare. A game may set a tighter
// ceiling with `fx_budget`, but it never has to: the default adapts frame by frame.
//
// The reserve is the second half. Headroom computed purely from what is live would let effects take
// every free slot, and then the next NPC to walk on screen gets nothing. `fx_reserve` keeps entries
// back for sprites that do not exist yet, which is the case no amount of caller discipline covers.

/// OAM entries on the machine. Hard hardware limit, not a tunable.
const FX_OAM_LIMIT: i32 = 128;
/// Entries held back for the game by default. Sixteen is a town's NPC turnover plus a HUD line —
/// enough that walking into a crowd during a victory burst does not drop anybody.
const FX_RESERVE_DEFAULT: i32 = 16;

/// Emitter shapes: where in space a particle is born.
const FXS_POINT: i32 = 0;
const FXS_BOX: i32 = 1;
const FXS_RING: i32 = 2;
const FXS_LINE: i32 = 3;

/// Preset rows, stride 14:
///   [shape, w, h, dir, spread, speed, speed_var, gravity, drag, wind, life, rate8, count, duration]
///
/// `dir` is -1 for omnidirectional or 0..255 around the circle (64 = straight down, 192 = straight
/// up, matching `sin_cos_256`). `rate8` is particles per frame in 8.8. `duration` of 0 makes it a
/// one-shot that emits `count` immediately and retires; -1 runs until stopped.
///
/// Presets deliberately say NOTHING about sprite frames. Which frame of a sheet is "a spark" and
/// whether the sheet is an animation or four unrelated shapes is knowledge only the game has, and a
/// preset that guessed would animate through a spritesheet of small men. Frames come from the
/// `fx_spawn` argument, and animation is opted into with `FXE_FRAMEN`.
const FX_PRESET_W: usize = 14;
#[rustfmt::skip]
const FX_PRESETS: [i32; FX_PRESET_W * 8] = [
    // shape      w    h  dir  spread  speed  svar  grav  drag  wind  life  rate8  count  dur
    FXS_POINT,    0,   0,  -1,    128,   640,  384,   26,  256,    0,   46,     0,    16,   0, // FXP_BURST
    FXS_POINT,    0,   0,  -1,    128,   260,  160,    0,  256,    0,   90,     0,    18,   0, // FXP_CONFETTI
    FXS_LINE,   240,   0,  64,      4,   700,  180,   14,  256,  -18,   58,   192,     0,  -1, // FXP_RAIN
    FXS_LINE,   240,   0,  64,     20,   120,   90,    2,  254,   10,  150,    96,     0,  -1, // FXP_SNOW
    FXS_BOX,     10,   4, 192,     26,   180,  140,  -10,  250,    0,   26,   200,     0,  -1, // FXP_FIRE
    FXS_BOX,     12,   4, 192,     30,    90,   60,   -3,  248,    6,   80,    56,     0,  -1, // FXP_SMOKE
    FXS_BOX,     48,  32,  -1,    128,    20,   20,    0,  256,    0,   40,    64,     0,  -1, // FXP_SPARKLE
    FXS_POINT,    0,   0, 192,     22,   560,  200,   30,  256,    0,   64,   160,     0,  -1, // FXP_FOUNTAIN
];

/// A small xorshift-ish LCG for particle spawning. Separate from the shake's, on purpose.
fn fx_rand(ctx: &mut GbaCtx) -> i32 {
    ctx.fx_seed = ctx
        .fx_seed
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223);
    (ctx.fx_seed >> 16) as i32 & 0x7FFF
}

/// How many more particles the effects layer may hold right now.
///
/// Measured, not assumed: `sprites.len() - sprite_free.len()` is what is really resident, and
/// subtracting the particles leaves what the GAME is holding. That is the number the ceiling has to
/// be computed against, because it is the one that changes when the player walks into a crowd.
fn fx_headroom_of(ctx: &GbaCtx) -> i32 {
    let live = ctx.sprites.len() as i32 - ctx.sprite_free.len() as i32;
    let gameplay = (live - ctx.particles.len() as i32).max(0);
    let avail = (FX_OAM_LIMIT - gameplay - ctx.fx_reserve).max(0);
    let cap = if ctx.fx_budget >= 0 {
        ctx.fx_budget.min(avail)
    } else {
        avail
    };
    (cap - ctx.particles.len() as i32).max(0)
}

/// `fx_headroom()` — particles the effects layer may still spawn, given what the game is using.
///
/// Exposed because a game that wants to scale an effect to the room it has should be able to ask,
/// rather than reverse-engineer the policy. Nothing is required to call it.
pub fn fx_headroom(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(fx_headroom_typed() as f64)
}

pub fn fx_headroom_typed() -> i32 {
    with_ctx(|ctx| fx_headroom_of(ctx))
}

/// `fx_budget(max, reserve)` — cap the effects layer, or `-1` to size it automatically.
///
/// `reserve` is entries kept free for sprites the game has not spawned yet; `-1` keeps the default.
/// A game with a fixed cast and no streaming can lower it; a busy scene should raise it rather than
/// discover the ceiling as NPCs failing to appear.
pub fn fx_budget(args: &[Value]) -> Value {
    fx_budget_typed(num(args, 0) as i32, num(args, 1) as i32);
    Value::Null
}

pub fn fx_budget_typed(max: i32, reserve: i32) {
    with_ctx(|ctx| {
        ctx.fx_budget = if max < 0 { -1 } else { max.min(FX_OAM_LIMIT) };
        if reserve >= 0 {
            ctx.fx_reserve = reserve.min(FX_OAM_LIMIT);
        }
    });
}

/// Retire the particle with the least life left and hand back its index, for a one-shot that must be
/// seen. A bang is worth more than the tail of something already dying; a CONTINUOUS emitter never
/// takes this path, because stealing every frame would make two effects flicker against each other
/// forever instead of one of them simply being thinner.
fn fx_recycle_oldest(ctx: &mut GbaCtx) -> bool {
    let mut victim = usize::MAX;
    let mut least = i32::MAX;
    for (i, p) in ctx.particles.iter().enumerate() {
        if p.life < least {
            least = p.life;
            victim = i;
        }
    }
    if victim == usize::MAX {
        return false;
    }
    let (sprite, owner) = (ctx.particles[victim].sprite, ctx.particles[victim].owner);
    free_sprite_slot(ctx, sprite);
    ctx.particles.swap_remove(victim);
    if owner >= 0 {
        if let Some(e) = ctx.emitters.get_mut(owner as usize) {
            e.live -= 1;
        }
    }
    true
}

/// The one place a particle is created. Every path goes through here so the budget is enforced once
/// rather than at each call site — which is the difference between a policy and a convention.
#[allow(clippy::too_many_arguments)]
fn fx_push(
    ctx: &mut GbaCtx,
    owner: i32,
    sheet: i32,
    frame0: i32,
    framen: i32,
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    gravity: i32,
    drag: i32,
    wind: i32,
    life: i32,
    steal: bool,
) -> bool {
    if fx_headroom_of(ctx) <= 0 && !(steal && fx_recycle_oldest(ctx)) {
        return false;
    }
    let idx = match alloc_sprite_slot(ctx, sheet, frame0) {
        Some(i) => i,
        None => return false,
    };
    if let Some(sd) = ctx.sprites.get_mut(idx) {
        sd.x = x >> 8;
        sd.y = y >> 8;
    }
    let life = life.max(1);
    ctx.particles.push(Particle {
        sprite: idx,
        x,
        y,
        vx,
        vy,
        gravity,
        drag,
        wind,
        life,
        life0: life,
        frame0,
        framen,
        sheet,
        owner,
    });
    if owner >= 0 {
        if let Some(e) = ctx.emitters.get_mut(owner as usize) {
            e.live += 1;
        }
    }
    true
}

/// Turn an emitter's shape and aim into one particle's starting position and velocity.
///
/// `steal` is for ONE-SHOTS only: a bang the player asked for may take a slot from something already
/// dying, a continuous source may not (see `fx_recycle_oldest`).
fn fx_emit_one(ctx: &mut GbaCtx, slot: usize, steal: bool) -> bool {
    let e = match ctx.emitters.get(slot) {
        Some(e) => e,
        None => return false,
    };
    let (ex, ey, w, h, shape) = (e.x, e.y, e.w, e.h, e.shape);
    let (dir, spread, speed, svar) = (e.dir, e.spread, e.speed, e.speed_var);
    let (gravity, drag, wind, life) = (e.gravity, e.drag, e.wind, e.life);
    let (sheet, frame0, framen) = (e.sheet, e.frame0, e.framen);

    let r1 = fx_rand(ctx);
    let r2 = fx_rand(ctx);
    let (px, py) = match shape {
        FXS_BOX => (
            ex + (r1 % (w.max(1))) - w / 2,
            ey + (r2 % (h.max(1))) - h / 2,
        ),
        FXS_LINE => (ex + (r1 % (w.max(1))) - w / 2, ey),
        FXS_RING => {
            let a = r1 & 255;
            let (sx, sy) = sin_cos_256(a);
            (ex + ((sx * w) >> 8), ey + ((sy * w) >> 8))
        }
        _ => (ex, ey),
    };
    // Omnidirectional means the spread IS the circle; otherwise aim and scatter around it.
    let ang = if dir < 0 {
        fx_rand(ctx) & 255
    } else {
        (dir + (fx_rand(ctx) % (spread * 2 + 1)) - spread) & 255
    };
    let spd = speed - svar / 2 + (fx_rand(ctx) % (svar.max(1)));
    let (sx, sy) = sin_cos_256(ang);
    fx_push(
        ctx,
        slot as i32,
        sheet,
        frame0,
        framen,
        px << 8,
        py << 8,
        (sx * spd) >> 8,
        (sy * spd) >> 8,
        gravity,
        drag,
        wind,
        life,
        steal,
    )
}

/// `fx_spawn(preset, sheet, frame, x, y, scale)` — start an effect. Returns a handle, or 0.
///
/// `scale` is a 1/256ths multiplier on size, speed and one-shot count — 256 is the preset as
/// authored, 512 is twice as big. A one-shot preset (BURST, CONFETTI) fires and retires immediately;
/// a continuous one runs until `fx_stop`.
///
/// Everything a preset does not know — which frame of the sheet, how long, how fast — is a
/// `fx_set` away, and the preset is only the starting point.
pub fn fx_spawn(args: &[Value]) -> Value {
    Value::Number(fx_spawn_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
        num(args, 5) as i32,
    ) as f64)
}

pub fn fx_spawn_typed(preset: i32, sheet: i32, frame: i32, x: i32, y: i32, scale: i32) -> i32 {
    let p = (preset.max(0) as usize).min(7) * FX_PRESET_W;
    let s = if scale <= 0 { 256 } else { scale };
    let row = &FX_PRESETS[p..p + FX_PRESET_W];
    let (count, duration) = (row[12] * s / 256, row[13]);
    with_ctx(|ctx| {
        let id = ctx.fx_next_id;
        ctx.fx_next_id += 1;
        let e = Emitter {
            id,
            sheet,
            x,
            y,
            w: row[1] * s / 256,
            h: row[2] * s / 256,
            shape: row[0],
            dir: row[3],
            spread: row[4],
            speed: row[5] * s / 256,
            speed_var: row[6] * s / 256,
            gravity: row[7],
            drag: row[8],
            wind: row[9],
            life: row[10],
            rate: row[11],
            acc: 0,
            // A single emitter may not take more than half the layer. Two heavy effects at once is
            // the case this covers: without it the first one to run owns everything and the second
            // is simply absent, which reads as a bug rather than as a busy screen.
            max: (FX_OAM_LIMIT / 2).max(1),
            frame0: frame,
            framen: frame,
            duration,
            live: 0,
        };
        let slot = ctx.emitters.len();
        ctx.emitters.push(e);
        // A one-shot emits NOW, so a burst is on screen the frame it was asked for rather than the
        // frame after. It then sits at duration 0 until its particles die, which is what keeps
        // `live` accurate for the budget.
        if duration == 0 {
            for _ in 0..count.max(0) {
                // Its own ceiling still applies — that is the emitter's share and it is not
                // negotiable. Past that, `steal` lets the bang take a slot from something already
                // dying rather than come out empty; when even that fails, stop.
                if ctx.emitters[slot].live >= ctx.emitters[slot].max {
                    break;
                }
                if !fx_emit_one(ctx, slot, true) {
                    break;
                }
            }
        }
        id
    })
}

/// Emitter fields for `fx_set`. One selector rather than seventeen setters — the same trade this
/// codebase makes everywhere a struct crosses a call boundary.
const FXE_X: i32 = 0;
const FXE_Y: i32 = 1;
const FXE_W: i32 = 2;
const FXE_H: i32 = 3;
const FXE_SHAPE: i32 = 4;
const FXE_DIR: i32 = 5;
const FXE_SPREAD: i32 = 6;
const FXE_SPEED: i32 = 7;
const FXE_SPEEDVAR: i32 = 8;
const FXE_GRAVITY: i32 = 9;
const FXE_DRAG: i32 = 10;
const FXE_WIND: i32 = 11;
const FXE_LIFE: i32 = 12;
const FXE_RATE: i32 = 13;
const FXE_MAX: i32 = 14;
const FXE_FRAME0: i32 = 15;
const FXE_FRAMEN: i32 = 16;
const FXE_DURATION: i32 = 17;

fn fx_slot_of(ctx: &GbaCtx, id: i32) -> Option<usize> {
    ctx.emitters.iter().position(|e| e.id == id)
}

/// `fx_set(id, field, value)` — retune a live emitter. Moving one is `FXE_X`/`FXE_Y`, which is how a
/// torch follows a walking NPC without respawning the flame.
pub fn fx_set(args: &[Value]) -> Value {
    fx_set_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
    );
    Value::Null
}

pub fn fx_set_typed(id: i32, field: i32, v: i32) {
    with_ctx(|ctx| {
        let slot = match fx_slot_of(ctx, id) {
            Some(s) => s,
            None => return,
        };
        let e = &mut ctx.emitters[slot];
        match field {
            FXE_X => e.x = v,
            FXE_Y => e.y = v,
            FXE_W => e.w = v,
            FXE_H => e.h = v,
            FXE_SHAPE => e.shape = v.clamp(0, 3),
            FXE_DIR => e.dir = v,
            FXE_SPREAD => e.spread = v.clamp(0, 128),
            FXE_SPEED => e.speed = v,
            FXE_SPEEDVAR => e.speed_var = v.max(1),
            FXE_GRAVITY => e.gravity = v,
            // 256 is frictionless. Above it a particle accelerates forever, which is never what a
            // caller means and is a hang shaped like an effect.
            FXE_DRAG => e.drag = v.clamp(0, 256),
            FXE_WIND => e.wind = v,
            FXE_LIFE => e.life = v.max(1),
            FXE_RATE => e.rate = v.max(0),
            FXE_MAX => e.max = v.clamp(1, FX_OAM_LIMIT),
            FXE_FRAME0 => e.frame0 = v.max(0),
            FXE_FRAMEN => e.framen = v.max(0),
            FXE_DURATION => e.duration = v,
            _ => {}
        }
    });
}

/// `fx_stop(id)` — stop emitting; particles already out live their lives. What a torch wants when
/// its room is left, so the flame thins out instead of blinking off.
pub fn fx_stop(args: &[Value]) -> Value {
    fx_stop_typed(num(args, 0) as i32);
    Value::Null
}

pub fn fx_stop_typed(id: i32) {
    with_ctx(|ctx| {
        if let Some(s) = fx_slot_of(ctx, id) {
            ctx.emitters[s].duration = 0;
        }
    });
}

/// `fx_kill(id)` — stop emitting AND remove this emitter's particles now. Scene teardown.
pub fn fx_kill(args: &[Value]) -> Value {
    fx_kill_typed(num(args, 0) as i32);
    Value::Null
}

pub fn fx_kill_typed(id: i32) {
    with_ctx(|ctx| {
        let slot = match fx_slot_of(ctx, id) {
            Some(s) => s as i32,
            None => return,
        };
        let mut i = 0usize;
        while i < ctx.particles.len() {
            if ctx.particles[i].owner == slot {
                let sp = ctx.particles[i].sprite;
                free_sprite_slot(ctx, sp);
                ctx.particles.swap_remove(i);
            } else {
                i += 1;
            }
        }
        ctx.emitters[slot as usize].live = 0;
        ctx.emitters[slot as usize].duration = 0;
    });
}

/// `fx_emitters()` — how many emitters are alive. For a scene wanting to assert it cleaned up.
pub fn fx_emitters(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(fx_emitters_typed() as f64)
}

pub fn fx_emitters_typed() -> i32 {
    with_ctx(|ctx| ctx.emitters.len() as i32)
}

/// `fx_burst(sheet, frame, x, y, count, speed, gravity, life)` — a particle burst at a screen point.
///
/// The engine owns and steps every particle, so the game spawns a burst and never touches it again.
/// Done from Tish this would be a `sprite_set_pos` per particle per frame; thirty particles is sixty
/// boxed calls a frame on top of whatever the scene is already doing.
///
/// `speed` is the initial radial velocity in 1/256ths of a pixel per frame, `gravity` the downward
/// acceleration in the same units (0 for a starburst that drifts, ~24 for fireworks that fall), and
/// `life` the frames each particle survives. Particles draw as HUD sprites: screen space, front
/// priority, unaffected by the camera — which is what you want over a result screen.
pub fn fx_burst(args: &[Value]) -> Value {
    fx_burst_typed(
        num(args, 0) as i32,
        num(args, 1) as i32,
        num(args, 2) as i32,
        num(args, 3) as i32,
        num(args, 4) as i32,
        num(args, 5) as i32,
        num(args, 6) as i32,
        num(args, 7) as i32,
    );
    Value::Null
}

pub fn fx_burst_typed(
    sheet: i32,
    frame: i32,
    x: i32,
    y: i32,
    count: i32,
    speed: i32,
    gravity: i32,
    life: i32,
) {
    with_ctx(|ctx| {
        let n = count.clamp(0, 48);
        for i in 0..n {
            // Spread the burst evenly and give each particle a different speed, so it reads as an
            // explosion rather than as a ring. A pure circle is the giveaway that a burst is
            // procedural; the eye reads even spacing as a wheel.
            let r = fx_rand(ctx);
            let ang = (i * 256) / n.max(1) + (r & 31);
            // Speed varies from 0.4x to 1.4x, not 0.75x to 1.25x. The narrow range put every
            // particle at nearly the same radius on every frame, so the burst read as an expanding
            // RING — the one shape that says "generated". A wide spread is what makes it a cloud.
            let spd = (speed * 2 / 5) + ((r >> 5) % speed.max(1));
            let (sx, sy) = sin_cos_256(ang);
            // Through the same budgeted path as everything else. This used to allocate directly and
            // `break` when the arena said no, which meant it could take the last free OAM entries
            // out from under the game — the bug the budget exists to close. `steal` is true because
            // a one-shot bang is worth more than the tail of something already dying.
            if !fx_push(
                ctx,
                -1,
                sheet,
                frame,
                frame,
                x << 8,
                y << 8,
                (sx * spd) >> 8,
                (sy * spd) >> 8,
                gravity,
                256,
                0,
                life,
                true,
            ) {
                break;
            }
        }
    });
}

/// `fx_active()` — how many particles are still alive. Lets a scene wait for a burst to finish.
pub fn fx_active(args: &[Value]) -> Value {
    let _ = args;
    Value::Number(fx_active_typed() as f64)
}

pub fn fx_active_typed() -> i32 {
    with_ctx(|ctx| ctx.particles.len() as i32)
}

/// `fx_clear()` — kill every live effect now: particles, flash and shake. Call on a scene teardown;
/// particles hold sprite slots, and a shake left running displaces the next scene's first frames.
pub fn fx_clear(args: &[Value]) -> Value {
    let _ = args;
    fx_clear_typed();
    Value::Null
}

pub fn fx_clear_typed() {
    with_ctx(|ctx| {
        while let Some(p) = ctx.particles.pop() {
            free_sprite_slot(ctx, p.sprite);
        }
        // Emitters go too, or a scene teardown leaves the rain running into the next room and the
        // handles the old scene held now steer nothing. Every id issued so far is stale after this,
        // which is exactly what a teardown should mean.
        ctx.emitters.clear();
        ctx.flash = 0;
    });
    fx_shake_stop_typed();
}

/// Take a sprite slot for a particle, on the requested sheet and frame. Returns `None` when the
/// sheet handle is unknown or the arena is exhausted — a burst that cannot fully spawn simply comes
/// out smaller, which is far better than a panic during a victory screen.
fn alloc_sprite_slot(ctx: &mut GbaCtx, sheet_handle: i32, frame_idx: i32) -> Option<usize> {
    let sheet = tishlang_runtime_gba::gba::asset_sheet(sheet_handle)?;
    let f = (frame_idx.max(0) as usize).min(sheet.len().saturating_sub(1));
    let object = Object::new(&sheet[f]);
    Some(sprite_alloc(
        ctx,
        SpriteData {
            object: Some(object),
            x: 0,
            y: 0,
            hflip: false,
            visible: true,
            sheet: sheet_handle,
            frame: f as i32,
            hud: true,
            priority: -1,
            depth: 0,
            billboard: false,
        },
    ))
}

/// Give a particle's slot back. Dropping the `Object` releases its VRAM allocation; the index goes
/// on the free list so the next burst reuses it rather than growing the arena.
fn free_sprite_slot(ctx: &mut GbaCtx, idx: usize) {
    if let Some(s) = ctx.sprites.get_mut(idx) {
        s.visible = false;
        s.object = None;
    }
    ctx.sprite_free.push(idx);
}

/// A 256-step sine/cosine in 8.8 fixed point, by quarter-wave table. No float, no `libm`, and small
/// enough that the table costs less than the code that would approximate it.
fn sin_cos_256(a: i32) -> (i32, i32) {
    const Q: [i32; 65] = [
        0, 6, 13, 19, 25, 31, 38, 44, 50, 56, 62, 68, 74, 80, 86, 92, 98, 104, 109, 115, 121, 126,
        132, 137, 142, 147, 152, 157, 162, 167, 172, 177, 181, 185, 190, 194, 198, 202, 206, 209,
        213, 216, 220, 223, 226, 229, 231, 234, 237, 239, 241, 243, 245, 247, 248, 250, 251, 252,
        253, 254, 255, 255, 256, 256, 256,
    ];
    let a = a.rem_euclid(256);
    let sin = match a / 64 {
        0 => Q[(a % 64) as usize],
        1 => Q[(64 - a % 64) as usize],
        2 => -Q[(a % 64) as usize],
        _ => -Q[(64 - a % 64) as usize],
    };
    let c = (a + 64) % 256;
    let cos = match c / 64 {
        0 => Q[(c % 64) as usize],
        1 => Q[(64 - c % 64) as usize],
        2 => -Q[(c % 64) as usize],
        _ => -Q[(64 - c % 64) as usize],
    };
    (cos, sin)
}

/// `sound_play(wavHandle)` — play a one-shot sound effect from a registered `wav:` import.
pub fn sound_play(args: &[Value]) -> Value {
    let h = num(args, 0) as i32;
    with_ctx(|ctx| {
        if let Some(data) = tishlang_runtime_gba::gba::asset_wav(h) {
            ensure_mixer(ctx);
            ctx.audio_used = true;
            let channel = SoundChannel::new(data);
            let _ = ctx.mixer.as_mut().unwrap().play_sound(channel);
        }
    });
    Value::Null
}

/// `sound_play_ex(wavHandle, volume, panning, pitch)` — a one-shot with the three knobs agb's
/// `SoundChannel` has always had and nothing here exposed: **volume**, **panning** and **playback
/// speed**, all Q8 (256 = 1.0).
///
/// * `volume`  0..=256 (and beyond, at your own risk of clipping)
/// * `panning` -256 hard left, 0 centre, +256 hard right
/// * `pitch`   256 = normal, 512 = an octave up, 128 = an octave down
///
/// This is what positional audio is built from: `packages/sfx.tish` turns a world position and the
/// camera into these three numbers. Before it, every effect in every game played dead centre at full
/// volume, which is why "the audio does not react to the game" was the most audible thing missing.
///
/// ⚠️ PITCH IS CLAMPED. agb's mixer panics on a playback speed its Q8 resampler cannot represent
/// (`docs/MEMORY.md`, "agb mixer panics at one playback speed" — raw 383 with real samples), so this
/// keeps the value inside a range that is known not to reach it. A silent clamp beats a crash on a
/// sound effect.
pub fn sound_play_ex(args: &[Value]) -> Value {
    let h = num(args, 0) as i32;
    let vol = (num(args, 1) as i32).clamp(0, 1024);
    let pan = (num(args, 2) as i32).clamp(-256, 256);
    let pitch = (num(args, 3) as i32).clamp(64, 380);
    with_ctx(|ctx| {
        if let Some(data) = tishlang_runtime_gba::gba::asset_wav(h) {
            ensure_mixer(ctx);
            ctx.audio_used = true;
            let mut channel = SoundChannel::new(data);
            channel.volume(Num::<i16, 8>::from_raw(vol as i16));
            // ⚠️ NEGATED, AND MEASURED. agb documents `panning` as -1 = fully left, +1 = fully
            // right; on this fork it behaves the other way round. A stereo capture of
            // `examples/earshot` with the source hard RIGHT (pan +153) put the energy in the LEFT
            // channel — L rms 2379 against R 397 — and `tools/gba-shot` interleaves channel 0 into
            // the even slots, so the capture is not the thing that is backwards.
            //
            // The tish-facing API keeps the conventional sign (-256 left, +256 right) because that
            // is what every caller will assume; the flip is absorbed here, at the one boundary that
            // knows about it, rather than left for each game to rediscover.
            channel.panning(Num::<i16, 8>::from_raw(-pan as i16));
            channel.playback(Num::<u32, 8>::from_raw(pitch as u32));
            let _ = ctx.mixer.as_mut().unwrap().play_sound(channel);
        }
    });
    Value::Null
}

/// `music_play(wavHandle)` — play a looping, high-priority track (background music).
/// Stops any previous BGM so area themes replace each other instead of stacking.
pub fn music_play(args: &[Value]) -> Value {
    let h = num(args, 0) as i32;
    with_ctx(|ctx| {
        if let Some(id) = ctx.music_channel.take() {
            if let Some(mixer) = ctx.mixer.as_mut() {
                if let Some(ch) = mixer.channel(&id) {
                    ch.stop();
                }
            }
        }
        if let Some(data) = tishlang_runtime_gba::gba::asset_wav(h) {
            ensure_mixer(ctx);
            ctx.audio_used = true;
            let mut channel = SoundChannel::new_high_priority(data);
            channel.should_loop();
            ctx.music_channel = ctx.mixer.as_mut().unwrap().play_sound(channel);
        }
    });
    Value::Null
}

// ── PSG synth ────────────────────────────────────────────────────────────────
// The GBA's four hardware sound channels, exposed as-is. A note here costs no ROM (it is a register
// write, not a recording) and no CPU (the hardware oscillates by itself), which is the whole reason
// these exist alongside the sampled `sound_play`/`music_play`. See `psg.rs`.
//
// Notes are MIDI numbers: 60 = C4, 69 = A4 (440 Hz), +12 per octave. The square channels cannot
// physically play below C2 (~64 Hz).

/// Power on and route the PSG, once, before the first note. Cheap enough to call unconditionally.
fn psg_ready(ctx: &mut GbaCtx) {
    if !ctx.psg_ready {
        psg::init();
        ctx.psg_ready = true;
    }
}

/// `psg_square(ch, note, duty, vol, decay, len [, env_up])` — a pulse-wave note on channel 1 or 2.
/// `duty` 0-3 (12.5/25/50/75%), `vol` 0-15, `decay` 0-7 (0 = sustain), `len` 0-63 (0 = sustain).
/// Optional `env_up` (default 0): envelope amplifies toward 15 instead of decaying.
pub fn psg_square(args: &[Value]) -> Value {
    with_ctx(|ctx| {
        psg_ready(ctx);
        psg::square(
            num(args, 0) as u8,
            num(args, 1) as i32,
            num(args, 2) as u8,
            num(args, 3) as u8,
            num(args, 4) as u8,
            num(args, 5) as u8,
            num(args, 6) != 0.0,
        );
    });
    Value::Null
}

/// `psg_slide(note, duty, vol, decay, len, shift, period, down [, env_up])` — channel 1's hardware
/// pitch sweep: the coin/jump/laser blip. The sweep is free; the hardware walks the pitch itself.
pub fn psg_slide(args: &[Value]) -> Value {
    with_ctx(|ctx| {
        psg_ready(ctx);
        psg::slide(
            num(args, 0) as i32,
            num(args, 1) as u8,
            num(args, 2) as u8,
            num(args, 3) as u8,
            num(args, 4) as u8,
            num(args, 5) as u8,
            num(args, 6) as u8,
            num(args, 7) != 0.0,
            num(args, 8) != 0.0,
        );
    });
    Value::Null
}

/// `psg_wave(note, vol, len)` — the wavetable channel. `vol` is its 4-step divider, NOT an
/// envelope: 0 = silent, 1 = full, 2 = half, 3 = quarter.
pub fn psg_wave(args: &[Value]) -> Value {
    with_ctx(|ctx| {
        psg_ready(ctx);
        psg::wave(num(args, 0) as i32, num(args, 1) as u8, num(args, 2) as u8);
    });
    Value::Null
}

/// `psg_wave_table(samples)` — load channel 3's 32-step waveform from an array of 32 values 0-15.
/// Anything shorter is padded with silence; anything longer is truncated.
pub fn psg_wave_table(args: &[Value]) -> Value {
    let mut packed = [0u8; 16];
    if let Some(Value::Array(a)) = args.first() {
        let a = a.borrow();
        let nibble = |i: usize| -> u8 {
            match a.get(i) {
                Some(Value::Number(n)) => (*n as i32).clamp(0, 15) as u8,
                _ => 0,
            }
        };
        for (i, byte) in packed.iter_mut().enumerate() {
            *byte = (nibble(i * 2) << 4) | nibble(i * 2 + 1);
        }
    }
    with_ctx(|ctx| {
        psg_ready(ctx);
        psg::wave_table(&packed);
    });
    Value::Null
}

/// `psg_noise(vol, decay, len, shift, ratio, narrow [, env_up])` — the noise channel: percussion
/// and impacts. `shift` 0-13 pitches it (low = bright hiss, high = deep rumble); `narrow` makes it
/// metallic. Optional `env_up` amplifies the envelope instead of decaying.
pub fn psg_noise(args: &[Value]) -> Value {
    with_ctx(|ctx| {
        psg_ready(ctx);
        psg::noise(
            num(args, 0) as u8,
            num(args, 1) as u8,
            num(args, 2) as u8,
            num(args, 3) as u8,
            num(args, 4) as u8,
            num(args, 5) != 0.0,
            num(args, 6) != 0.0,
        );
    });
    Value::Null
}

/// `chip_play(songHandle)` — start a `chip:`-imported song on the PSG channels, replacing whatever
/// was playing. This is the synth counterpart of `music_play`: no ROM beyond the notes, no mixer
/// channel consumed, and nothing to feed per frame. Stops any `deck_play` song (PSG ownership).
pub fn chip_play(args: &[Value]) -> Value {
    let handle = num(args, 0) as i32;
    let song = SONGS.with(|s| s.borrow().get(handle as usize).copied());
    if let Some(song) = song {
        with_ctx(|ctx| {
            psg_ready(ctx);
            DECK.with(|p| p.borrow_mut().stop(&mut ctx.mixer));
        });
        CHIP.with(|p| p.borrow_mut().play(song));
    }
    Value::Null
}

/// `chip_stop()` — stop the song and silence every channel.
pub fn chip_stop(_args: &[Value]) -> Value {
    CHIP.with(|p| p.borrow_mut().stop());
    Value::Null
}

/// `chip_playing()` — 1 while a song is running, else 0.
pub fn chip_playing(_args: &[Value]) -> Value {
    Value::Number(if CHIP.with(|p| p.borrow().playing()) {
        1.0
    } else {
        0.0
    })
}

/// `chip_note(ch)` — the MIDI note channel `ch` is sounding, or 0 for silence.
///
/// `chiptune::Player::channel_note` has always computed this (it walks HOLD rows back to the note
/// actually ringing); it simply had no extern. examples/chiptune imports chip_note/chip_row/chip_rows
/// and could therefore never build -- the generated crate referenced three functions that do not
/// exist, E0425, at every commit in this repo's history.
pub fn chip_note(args: &[Value]) -> Value {
    let ch = num(args, 0) as usize;
    Value::Number(CHIP.with(|p| p.borrow().channel_note(ch)) as f64)
}
pub fn chip_note_typed(ch: i32) -> i32 {
    if ch < 0 {
        return 0;
    }
    CHIP.with(|p| p.borrow().channel_note(ch as usize))
}

/// `chip_row()` — the sequencer's current row, for a playhead.
pub fn chip_row(_args: &[Value]) -> Value {
    Value::Number(CHIP.with(|p| p.borrow().row()) as f64)
}
pub fn chip_row_typed() -> i32 {
    CHIP.with(|p| p.borrow().row())
}

/// `chip_rows()` — how many rows the loaded song has, so a playhead can be drawn to scale.
pub fn chip_rows(_args: &[Value]) -> Value {
    Value::Number(CHIP.with(|p| p.borrow().rows()) as f64)
}
pub fn chip_rows_typed() -> i32 {
    CHIP.with(|p| p.borrow().rows())
}

/// `chip_borrow(ch, frames)` — lend a music channel to a sound effect for `frames`. Works for both
/// chip and DECK BGM so chipsfx keeps working during either.
pub fn chip_borrow(args: &[Value]) -> Value {
    let ch = num(args, 0) as u8;
    let frames = num(args, 1) as u8;
    if DECK.with(|p| p.borrow().playing()) {
        DECK.with(|p| p.borrow_mut().borrow_channel(ch, frames));
    } else {
        CHIP.with(|p| p.borrow_mut().borrow_channel(ch, frames));
    }
    Value::Null
}

/// `deck_play(songHandle)` — start a `deck:`-imported song (LR35902 + GBA PCM). Stops chip BGM.
pub fn deck_play(args: &[Value]) -> Value {
    let handle = num(args, 0) as i32;
    let song = DECK_SONGS.with(|s| s.borrow().get(handle as usize).copied());
    if let Some(song) = song {
        CHIP.with(|p| p.borrow_mut().stop());
        with_ctx(|ctx| {
            psg_ready(ctx);
            if !song.pcm_tables.is_empty() {
                ensure_mixer(ctx);
                ctx.audio_used = true;
            }
            DECK.with(|p| p.borrow_mut().play(song, &mut ctx.mixer));
        });
    }
    Value::Null
}

/// `deck_stop()` — stop the deck song and silence PSG (+ PCM voices it owned).
pub fn deck_stop(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        DECK.with(|p| p.borrow_mut().stop(&mut ctx.mixer));
    });
    Value::Null
}

/// `deck_playing()` — 1 while a deck song is running, else 0.
pub fn deck_playing(_args: &[Value]) -> Value {
    Value::Number(if DECK.with(|p| p.borrow().playing()) {
        1.0
    } else {
        0.0
    })
}

/// `deck_frame() -> i32` — the deck playhead in sequencer frames, or -1 when no song is playing.
///
/// Added for rhythm games, where "what time is it in the song" has to be the SAME number the
/// sequencer is using or the chart slowly slides off the music. A tish-side frame counter is not
/// that number: `music_catchup` advances the sequencer once per elapsed display frame, so any
/// frame the game misses moves the song forward and leaves the counter behind. Reading the
/// playhead instead makes drift structurally impossible — there is only one clock.
///
/// Wraps to `loop_frame` when the song loops, so a chart keyed to it loops with the music.
pub fn deck_frame(_args: &[Value]) -> Value {
    DECK.with(|p| {
        let p = p.borrow();
        Value::Number(if p.playing() {
            p.playhead() as f64
        } else {
            -1.0
        })
    })
}

/// `deck_set_intensity(level)` — crossfading-stem-style stem gate (0..=3). Playhead stays synced;
/// stems with `min_intensity` above `level` mute (active notes cut). No-op if nothing playing
/// except it still stores the level for the next song.
/// `deck_pause(on)` — freeze or resume the playhead, keeping the song, position, voices and
/// intensity. This is the call that replaces `deck_stop()` + `deck_play()` around a UI canvas
/// build: that pair loses the playhead AND resets the intensity to 0, so a game that hushed its
/// music for a menu came back at the wrong point of the song and the wrong threat level.
pub fn deck_pause(args: &[Value]) -> Value {
    let on = num(args, 0) != 0.0;
    with_ctx(|_ctx| {
        DECK.with(|p| p.borrow_mut().set_paused(on));
    });
    Value::Null
}

/// `deck_paused()` — 1 while the playhead is frozen.
pub fn deck_paused(_args: &[Value]) -> Value {
    Value::Number(DECK.with(|p| p.borrow().paused()) as i32 as f64)
}

/// `audio_duck(depth, attack, hold, release)` — pull the music down under a line of dialogue or a
/// stinger, and let it back up. `depth` is 0..=64 of attenuation (40 is roughly 60% down);
/// `attack`/`release` are frames to reach and leave it; `hold` is frames at depth before releasing,
/// or 0 to hold until the next call.
///
/// The step sizes are divided ONCE here rather than per frame — the whole reason the envelope is
/// expressed this way on a chip with no divide instruction.
pub fn audio_duck(args: &[Value]) -> Value {
    let depth = (num(args, 0) as i32).clamp(0, 64);
    let attack = (num(args, 1) as i32).max(1);
    let hold = (num(args, 2) as i32).max(0);
    let release = (num(args, 3) as i32).max(1);
    let mut d = DUCK.with(|c| c.get());
    d[1] = 64 - depth;
    d[2] = ((64 - d[1]) / attack).max(1);
    d[3] = ((64 - d[1]) / release).max(1);
    d[4] = hold;
    DUCK.with(|c| c.set(d));
    Value::Null
}

/// `audio_duck_level()` — the current gain, 0..=64. For a verifier, and for a game that wants to
/// know whether the music is already down before ducking it again.
pub fn audio_duck_level(_args: &[Value]) -> Value {
    Value::Number(DUCK.with(|c| c.get())[0] as f64)
}

// Typed siblings, so a `declare fn` in the crate's .d.tish lowers these to a direct Rust call.
pub fn deck_pause_typed(on: i32) {
    with_ctx(|_ctx| {
        DECK.with(|p| p.borrow_mut().set_paused(on != 0));
    });
}
pub fn deck_paused_typed() -> i32 {
    DECK.with(|p| p.borrow().paused()) as i32
}
/// Typed twin of [`sound_play_ex`] — a `declare fn` in `tish.d.tish` REQUIRES one, or every ROM that
/// calls it fails to link (`cannot find function sound_play_ex_typed`). Same body, no boxing.
pub fn sound_play_ex_typed(wav: i32, volume: i32, panning: i32, pitch: i32) {
    sound_play_ex(&[
        Value::Number(wav as f64),
        Value::Number(volume as f64),
        Value::Number(panning as f64),
        Value::Number(pitch as f64),
    ]);
}

pub fn audio_duck_typed(depth: i32, attack: i32, hold: i32, release: i32) {
    audio_duck(&[
        Value::Number(depth as f64),
        Value::Number(attack as f64),
        Value::Number(hold as f64),
        Value::Number(release as f64),
    ]);
}
pub fn audio_duck_level_typed() -> i32 {
    DUCK.with(|c| c.get())[0]
}

pub fn deck_set_intensity(args: &[Value]) -> Value {
    let level = (num(args, 0) as i32).clamp(0, 3) as u8;
    with_ctx(|ctx| {
        DECK.with(|p| p.borrow_mut().set_intensity(level, &mut ctx.mixer));
    });
    Value::Number(level as f64)
}

/// `deck_intensity()` — current intensifier level 0..=3.
pub fn deck_intensity(_args: &[Value]) -> Value {
    Value::Number(DECK.with(|p| p.borrow().intensity()) as f64)
}

/// `psg_stop(ch)` — silence one channel (1-4), or all four when `ch` is 0.
pub fn psg_stop(args: &[Value]) -> Value {
    let ch = num(args, 0) as u8;
    if ch == 0 {
        psg::stop_all();
    } else {
        psg::stop(ch);
    }
    Value::Null
}

/// `dialogue_set_blip(wavHandle)` — RPG-style typewriter chirp. Pass a `wav:` handle; each
/// newly revealed letter group in an open dialogue box plays it. Pass −1 to disable.
pub fn dialogue_set_blip(args: &[Value]) -> Value {
    let h = num(args, 0) as i32;
    with_ctx(|ctx| {
        ctx.dialog_blip = h;
    });
    Value::Null
}

/// Tile the dialogue-box panel across the bottom (4 cols × 2 rows) and install the text
/// palettes. Shared by `dialogue_show` and `dialogue_ask`.
/// Paint one 64×32 panel tile the same way HUD bars do — every pixel via `set_pixel`. Relying on
/// `clear()` alone has been fine in empty scenes, but under VRAM pressure a sparse write left the
/// characteristic "blue speck every 8px" stripe. Filling explicitly matches the path that works.
fn paint_dialog_sprite(edge: bool) -> DynamicSprite16<ExternalAllocator> {
    let mut spr = DynamicSprite16::new_in(Size::S64x32, ExternalAllocator);
    for py in 0..32usize {
        for px in 0..64usize {
            let idx = if edge && py < 2 {
                2u8
            } else if edge && py < 4 {
                3u8
            } else {
                1u8
            };
            spr.set_pixel(px, py, idx);
        }
    }
    spr
}

/// Allocate the two panel SpriteVrams once and keep them for the session. Safe to call early
/// (boot) or lazily on first open — but early is what stops busy rooms from striping.
fn ensure_dialog_panel(ctx: &mut GbaCtx) {
    if ctx.dialog_panel_top.is_some() {
        return;
    }
    let pal = PaletteVramSingle::new(&BOX_PALETTE);
    let top = paint_dialog_sprite(true).to_vram(pal.clone());
    let fill = paint_dialog_sprite(false).to_vram(pal);
    ctx.dialog_panel_top = Some(top);
    ctx.dialog_panel_fill = Some(fill);
}

fn open_dialogue_box(ctx: &mut GbaCtx) {
    ctx.dialog_box.clear();
    ensure_dialog_panel(ctx);
    let top_v = ctx.dialog_panel_top.as_ref().unwrap().clone();
    let fill_v = ctx.dialog_panel_fill.as_ref().unwrap().clone();
    // 4 cols × 2 rows of 64×32 covering the bottom of the screen.
    for ry in 0..2 {
        for cx in 0..4 {
            let v = if ry == 0 {
                top_v.clone()
            } else {
                fill_v.clone()
            };
            let mut obj = Object::new(v);
            obj.set_pos(Vector2D::new(cx * 64, 104 + ry * 28));
            obj.set_priority(Priority::P1); // under the P0 text background, over the map + sprites
            ctx.dialog_box.push(obj);
        }
    }
    // Text palettes live in reserved high slots (persist in PALRAM — enough for every page).
    ctx.gfx
        .set_background_palette(DIALOG_BODY_PAL, &DIALOG_PALETTE);
    ctx.gfx
        .set_background_palette(DIALOG_NAME_PAL, &NAME_PALETTE);
}

/// (Re)build the text background for the CURRENT page: a fresh transparent background with
/// the speaker name (yellow), this page's body text (white, typewriter-revealed), and — in
/// choice mode — the options on a line below with a `>` cursor at the selection.
fn build_text_page(ctx: &mut GbaCtx) {
    let mut bg = RegularBackground::new(
        Priority::P0,
        RegularBackgroundSize::Background32x32,
        TileFormat::FourBpp,
    );
    // Speaker name, drawn in full at the top of the box.
    let mut name_r = RegularBackgroundTextRenderer::new((10, 106), DIALOG_NAME_PAL);
    if !ctx.dialog_speaker.is_empty() {
        for group in Layout::new(
            &ctx.dialog_speaker,
            &FONT,
            &LayoutSettings::new().with_max_line_length(216),
        )
        .take(max_groups(&ctx.dialog_speaker))
        {
            name_r.show(&mut bg, &group);
        }
    }
    // Body: collect the current page's letter groups; the frame loop reveals them one at a
    // time by drawing the next group into the background (typewriter).
    let body_r = RegularBackgroundTextRenderer::new((10, 124), DIALOG_BODY_PAL);
    let page = ctx
        .dialog_pages
        .get(ctx.dialog_page)
        .map(|s| s.as_str())
        .unwrap_or("");
    let groups: Vec<LetterGroup> = Layout::new(
        page,
        &FONT,
        &LayoutSettings::new().with_max_line_length(216),
    )
    .take(max_groups(page))
    .collect();
    // Options (choice mode): a single line like "> Yes   No" with the cursor on the
    // selection, drawn immediately (not typewritten) below the question.
    let opts_r = if ctx.dialog_options.is_empty() {
        None
    } else {
        let mut s = String::new();
        for (i, opt) in ctx.dialog_options.iter().enumerate() {
            if i > 0 {
                s.push_str("   ");
            }
            s.push_str(if i == ctx.dialog_selected { "> " } else { "  " });
            s.push_str(opt);
        }
        let mut r = RegularBackgroundTextRenderer::new((10, 140), DIALOG_BODY_PAL);
        for group in Layout::new(&s, &FONT, &LayoutSettings::new().with_max_line_length(216))
            .take(max_groups(&s))
        {
            r.show(&mut bg, &group);
        }
        Some(r)
    };
    ctx.dialog_text_bg = Some(bg);
    ctx.dialog_name = Some(name_r);
    ctx.dialog_body = Some(body_r);
    ctx.dialog_opts_r = opts_r;
    ctx.dialog_groups = groups;
    ctx.dialog_revealed = 0;
    ctx.dialog_timer = 0;
}

/// Draw every remaining body group at once (finish the typewriter for this page).
fn reveal_all(ctx: &mut GbaCtx) {
    while ctx.dialog_revealed < ctx.dialog_groups.len() {
        let idx = ctx.dialog_revealed;
        if let (Some(bg), Some(r)) = (ctx.dialog_text_bg.as_mut(), ctx.dialog_body.as_mut()) {
            r.show(bg, &ctx.dialog_groups[idx]);
        }
        ctx.dialog_revealed = idx + 1;
    }
}

/// Close the box and free the text tiles. Choice bookkeeping (`dialog_choice_cb` /
/// `dialog_choice_pending` / `dialog_result`) is intentionally left intact for the pump.
fn close_dialogue(ctx: &mut GbaCtx) {
    ctx.dialog_active = false;
    ctx.dialog_text_bg = None;
    ctx.dialog_body = None;
    ctx.dialog_name = None;
    ctx.dialog_opts_r = None;
    ctx.dialog_groups.clear();
    ctx.dialog_pages.clear();
    ctx.dialog_page = 0;
    ctx.dialog_speaker.clear();
    ctx.dialog_options.clear();
    ctx.dialog_selected = 0;
    ctx.dialog_box.clear();
    // Panel SpriteVram stays in dialog_panel_top/fill — next open just wraps new Objects.
}

/// Read the first argument as dialogue pages: an array of strings ⇒ one page each; any
/// other value ⇒ a single page.
fn pages_arg(args: &[Value]) -> Vec<String> {
    match args.first() {
        Some(Value::Array(arr)) => arr.borrow().iter().map(|p| p.to_display_string()).collect(),
        Some(v) => alloc::vec![v.to_display_string()],
        None => alloc::vec![String::new()],
    }
}

/// Read argument `i` as a speaker name: missing or null ⇒ no name.
fn speaker_arg(args: &[Value], i: usize) -> String {
    match args.get(i) {
        None | Some(Value::Null) => String::new(),
        Some(v) => v.to_display_string(),
    }
}

/// `dialogue_reserve()` — allocate the chat-panel sprite VRAM now. Called automatically on
/// engine init; exposed so a game that somehow skipped init-time graphics can still warm the
/// panel before a crowded scene. Idempotent.
pub fn dialogue_reserve(_args: &[Value]) -> Value {
    with_ctx(ensure_dialog_panel);
    Value::Null
}

/// `dialogue_show(text, speaker)` — open a plain message box: a dark panel across the
/// bottom, the (optional) `speaker` name in yellow, and `text` revealed typewriter-style.
/// `text` may be a single string (one page) or an array of strings (a multi-page message
/// the player advances through with the action button).
pub fn dialogue_show(args: &[Value]) -> Value {
    let pages = pages_arg(args);
    let speaker = speaker_arg(args, 1);
    with_ctx(|ctx| {
        open_dialogue_box(ctx);
        ctx.dialog_options.clear();
        ctx.dialog_choice_cb = None;
        ctx.dialog_choice_pending = false;
        ctx.dialog_speaker = speaker;
        ctx.dialog_pages = pages;
        ctx.dialog_page = 0;
        build_text_page(ctx);
        ctx.dialog_active = true;
    });
    Value::Null
}

/// `dialogue_ask(text, speaker, options, callback)` — open a choice box: the `text`
/// question, then `options` (an array of strings) on a line below with a `>` cursor. The
/// player moves the cursor with `dialogue_move` and confirms with `dialogue_advance`;
/// `dialogue_pump` then calls `callback(index)` with the chosen option's index.
pub fn dialogue_ask(args: &[Value]) -> Value {
    let text = match args.first() {
        Some(v) => v.to_display_string(),
        None => String::new(),
    };
    let speaker = speaker_arg(args, 1);
    let options: Vec<String> = match args.get(2) {
        Some(Value::Array(arr)) => arr.borrow().iter().map(|p| p.to_display_string()).collect(),
        _ => Vec::new(),
    };
    let cb = args.get(3).cloned();
    with_ctx(|ctx| {
        open_dialogue_box(ctx);
        ctx.dialog_speaker = speaker;
        ctx.dialog_pages = alloc::vec![text];
        ctx.dialog_page = 0;
        ctx.dialog_options = options;
        ctx.dialog_selected = 0;
        ctx.dialog_choice_cb = cb;
        ctx.dialog_choice_pending = false;
        build_text_page(ctx);
        ctx.dialog_active = true;
    });
    Value::Null
}

/// `dialogue_active()` — is a dialogue box currently showing?
pub fn dialogue_active(_args: &[Value]) -> Value {
    Value::Bool(with_ctx(|ctx| ctx.dialog_active))
}

/// `dialogue_is_choice()` — is the current box a choice (has selectable options)?
pub fn dialogue_is_choice(_args: &[Value]) -> Value {
    Value::Bool(with_ctx(|ctx| {
        ctx.dialog_active && !ctx.dialog_options.is_empty()
    }))
}

/// `dialogue_move(delta)` — move the choice cursor by `delta` (wrapping); a no-op when the
/// box is not a choice. Keeps the question fully shown while navigating.
pub fn dialogue_move(args: &[Value]) -> Value {
    let delta = num(args, 0) as i32;
    with_ctx(|ctx| {
        let n = ctx.dialog_options.len() as i32;
        if n == 0 {
            return;
        }
        let cur = ctx.dialog_selected as i32;
        let next = (((cur + delta) % n) + n) % n;
        if next as usize != ctx.dialog_selected {
            ctx.dialog_selected = next as usize;
            build_text_page(ctx);
            reveal_all(ctx); // don't re-typewriter the question when just moving the cursor
        }
    });
    Value::Null
}

/// `dialogue_advance()` — the action button: if the page is still typing, reveal it all;
/// else if it is a choice, confirm the selection and close; else if more pages remain,
/// load the next; else close.
pub fn dialogue_advance(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        if ctx.dialog_revealed < ctx.dialog_groups.len() {
            reveal_all(ctx);
        } else if !ctx.dialog_options.is_empty() {
            // Confirm the choice: stash the result; the pump fires the callback next.
            ctx.dialog_result = ctx.dialog_selected;
            ctx.dialog_choice_pending = true;
            close_dialogue(ctx);
        } else if ctx.dialog_page + 1 < ctx.dialog_pages.len() {
            ctx.dialog_page += 1;
            build_text_page(ctx);
        } else {
            close_dialogue(ctx);
        }
    });
    Value::Null
}

/// `dialogue_pump()` — after a choice is confirmed, invoke its callback with the chosen
/// index. Must run OUTSIDE any context borrow: it takes the callback out from under the
/// borrow, releases it, THEN calls, so the callback may re-enter (e.g. open a follow-up).
pub fn dialogue_pump(_args: &[Value]) -> Value {
    let pending = with_ctx(|ctx| {
        if ctx.dialog_choice_pending {
            ctx.dialog_choice_pending = false;
            ctx.dialog_choice_cb
                .take()
                .map(|cb| (cb, ctx.dialog_result))
        } else {
            None
        }
    });
    if let Some((cb, idx)) = pending {
        value_call(&cb, &[Value::Number(idx as f64)]);
    }
    Value::Null
}

/// `frame()` — the heartbeat: draw visible backgrounds, then visible sprites, then the
/// dialogue box (if active), commit (waits vblank), pump the audio mixer, refresh input.
/// Rebuilds the draw list each frame (agb 0.25's frame-scoped model).
pub fn frame(_args: &[Value]) -> Value {
    // Before the frame's work, not after: a row that starts a note should start it at the top of the
    // frame rather than after a scene stream has already eaten the budget.
    step_music_frame();
    with_ctx(|ctx| {
        let __t0 = ctx.timer.value() as i32;
        // Page streamed map tiles BEFORE taking the graphics frame: the burst needs the whole
        // ctx (audio pump) and `gfx.frame()` borrows it.
        ctx.ui_began_this_frame = false;
        prime_stream_layers(ctx);
        let __t1;
        let __t2;
        {
            // Scene-transition fade: read once (frame borrows ctx.gfx; keep this off ctx). Collect the
            // shown backgrounds' ids so the brightness blend can target them, and put every sprite into
            // AlphaBlending graphics mode so it darkens too — otherwise a fade-to-black would leave
            // sprites bright over a black backdrop and the "black" moment wouldn't be black.
            // ── Effects, stepped by the ENGINE ────────────────────────────────────────
            // Particles move, age and die here, so a game spawns a burst and never touches it
            // again. Whole-pixel positions are derived from 8.8 fixed point at draw time.
            {
                // Emitters first, so a particle born this frame is drawn this frame rather than
                // sitting invisible for one. Each is capped twice: by its own `max` and by the
                // layer's headroom, and it simply emits fewer when either is reached — there is no
                // error path for a caller to handle, because there is nothing a caller could do.
                let mut ei = 0usize;
                while ei < ctx.emitters.len() {
                    let n = {
                        let e = &mut ctx.emitters[ei];
                        if e.duration == 0 {
                            0
                        } else {
                            if e.duration > 0 {
                                e.duration -= 1;
                            }
                            e.acc += e.rate;
                            let n = e.acc >> 8;
                            e.acc &= 255;
                            n
                        }
                    };
                    for _ in 0..n {
                        let room = {
                            let e = &ctx.emitters[ei];
                            e.live < e.max
                        };
                        if !room || fx_headroom_of(ctx) <= 0 {
                            break;
                        }
                        fx_emit_one(ctx, ei, false);
                    }
                    ei += 1;
                }
                // Retire emitters that have stopped and outlived their last particle. Slots are
                // swap_removed, so any particle owned by the MOVED slot is repointed — an owner
                // index left dangling would credit its death to whatever landed there next and
                // corrupt the budget accounting slowly enough to look like a leak.
                let mut ri = 0usize;
                while ri < ctx.emitters.len() {
                    if ctx.emitters[ri].duration == 0 && ctx.emitters[ri].live <= 0 {
                        let last = ctx.emitters.len() - 1;
                        ctx.emitters.swap_remove(ri);
                        if last != ri {
                            for p in ctx.particles.iter_mut() {
                                if p.owner == last as i32 {
                                    p.owner = ri as i32;
                                }
                            }
                        }
                    } else {
                        ri += 1;
                    }
                }
                let mut i = 0usize;
                while i < ctx.particles.len() {
                    let (dead, sprite, owner, px, py, want, sheet, anim) = {
                        let p = &mut ctx.particles[i];
                        p.vy += p.gravity;
                        p.vx += p.wind;
                        // Drag is a retention fraction, so 256 is frictionless and costs one branch
                        // rather than two multiplies on the common case.
                        if p.drag != 256 {
                            p.vx = (p.vx * p.drag) >> 8;
                            p.vy = (p.vy * p.drag) >> 8;
                        }
                        p.x += p.vx;
                        p.y += p.vy;
                        p.life -= 1;
                        // FRAME OVER LIFE — the only fade this machine offers. There is one BLDCNT
                        // mode and the engine already spends it on the scene fade, so per-object
                        // alpha is not available at any price; a sprite that gets smaller, dimmer or
                        // sparser across its own frames is how a particle dies here instead.
                        let anim = p.framen > p.frame0;
                        let want = if anim {
                            let span = p.framen - p.frame0 + 1;
                            let gone = p.life0 - p.life;
                            (p.frame0 + (gone * span) / p.life0.max(1)).min(p.framen)
                        } else {
                            p.frame0
                        };
                        (
                            p.life <= 0,
                            p.sprite,
                            p.owner,
                            p.x >> 8,
                            p.y >> 8,
                            want,
                            p.sheet,
                            anim,
                        )
                    };
                    if dead {
                        free_sprite_slot(ctx, sprite);
                        ctx.particles.swap_remove(i);
                        if owner >= 0 {
                            if let Some(e) = ctx.emitters.get_mut(owner as usize) {
                                e.live -= 1;
                            }
                        }
                    } else {
                        if let Some(sd) = ctx.sprites.get_mut(sprite) {
                            sd.x = px;
                            sd.y = py;
                        }
                        // Only when the particle actually animates, and only on a CHANGE: rebuilding
                        // the Object re-uploads the frame's tiles, so an unguarded assignment would
                        // be a VRAM DMA per particle per frame.
                        if anim {
                            let differs = ctx
                                .sprites
                                .get(sprite)
                                .map(|s| s.frame != want)
                                .unwrap_or(false);
                            if differs {
                                if let Some(sh) = tishlang_runtime_gba::gba::asset_sheet(sheet) {
                                    let idx =
                                        (want.max(0) as usize).min(sh.len().saturating_sub(1));
                                    if let Some(sd) = ctx.sprites.get_mut(sprite) {
                                        sd.frame = idx as i32;
                                        sd.object = Some(Object::new(&sh[idx]));
                                    }
                                }
                            }
                        }
                        i += 1;
                    }
                }
            }
            // Flash decays on its own so one call is the whole effect.
            if ctx.flash_decay != 0 && ctx.flash > 0 {
                ctx.flash -= 1;
            }
            // Shake: ONE damped spring, stepped here and applied to every surface at compose time.
            // No divides, no redraw — the offsets become register writes further down. See the
            // `shake_*` fields for why this is a spring and not a countdown.
            //
            //   v += (-K*x - D*v) >> 8 ;  x += v          (8.8 fixed point)
            //
            let mut shake_x = 0i32;
            let mut shake_y = 0i32;
            // True on the single frame the spring settles: the frame that must write every surface
            // back to square. Distinct from `shake_live`, which is true for the whole shake.
            let mut shake_landed = false;
            // Position as well as velocity, so the guard stays correct for anything that ever
            // displaces the spring without giving it a push. Four compares on an idle frame.
            if ctx.shake_live
                || ctx.shake_vx != 0
                || ctx.shake_vy != 0
                || ctx.shake_x != 0
                || ctx.shake_y != 0
            {
                let k = ctx.shake_k;
                let d = ctx.shake_d;
                let ax = ((-(k * ctx.shake_x)) - d * ctx.shake_vx) >> 8;
                ctx.shake_vx += ax;
                ctx.shake_x += ctx.shake_vx;
                let ay = ((-(k * ctx.shake_y)) - d * ctx.shake_vy) >> 8;
                ctx.shake_vy += ay;
                ctx.shake_y += ctx.shake_vy;
                shake_x = ctx.shake_x >> 8;
                shake_y = ctx.shake_y >> 8;
                // Settle on POSITION AND VELOCITY. Position alone is wrong: an oscillating spring
                // passes through zero every half period with its velocity at maximum, so a
                // position-only test would snap the screen square on the first zero-crossing and
                // turn a shake into a single flick.
                if shake_x == 0
                    && shake_y == 0
                    && ctx.shake_vx.abs() < 256
                    && ctx.shake_vy.abs() < 256
                {
                    ctx.shake_x = 0;
                    ctx.shake_vx = 0;
                    ctx.shake_y = 0;
                    ctx.shake_vy = 0;
                    shake_landed = ctx.shake_live;
                    ctx.shake_live = false;
                } else {
                    ctx.shake_live = true;
                }
            }
            let flash_level = ctx.flash;
            let fade_level = ctx.fade;
            let white_level = ctx.fade_white;
            let mosaic_bg = ctx.mosaic_bg;
            let mosaic_obj = ctx.mosaic_obj;
            // Whether a mosaic was live LAST frame, so the frame that turns it off still runs the
            // poke and actually clears the enable bits. Without this a dissolve would pixelate in
            // and then never un-pixelate.
            let mosaic_was_live = ctx.mosaic_live;
            ctx.mosaic_live = mosaic_bg != 0 || mosaic_obj != 0;
            let fading = fade_level > 0;
            // BLDCNT is ONE effect field for the whole screen and agb's `Blend` resets it on every
            // `alpha`/`brighten`/`darken`, so the last caller would silently erase the others.
            // Resolve the winner once, here, instead of letting call order decide:
            //   fade > fade_white > fx_flash > blend_alpha
            // A transition outranks everything (a scene change must not be interruptible by a
            // hit-spark), and alpha — the only per-layer effect — yields to every whole-screen ramp.
            let alpha_weights = if fade_level > 0 || white_level > 0 || flash_level > 0 {
                None
            } else {
                ctx.blend_alpha
            };
            // Sprites only participate in a blend when their graphics mode says so, and that is true
            // of the brighten and alpha paths just as much as the darken one — a fade-to-white that
            // left the sprites Normal would blow out the scene and leave the cast sitting on top of
            // it at full brightness.
            let obj_mode =
                if fading || white_level > 0 || flash_level > 0 || alpha_weights.is_some() {
                    GraphicsMode::AlphaBlending
                } else {
                    GraphicsMode::Normal
                };
            ctx.bg_ids_buf.clear();
            let mut frame = ctx.gfx.frame();
            // The GBA has exactly 4 regular background layers and agb PANICS on the 5th `show()`
            // ("Can only have 4 backgrounds at once") — an abort, since we build with panic=abort.
            // The count here is DATA-driven (a Tiled map's layer count, plus any bg_new, plus the UI
            // canvas), so an artist adding a fourth tile layer to a map that also opens a dialog
            // would have crashed the game at runtime with no diagnostic. Budget the slots instead:
            // the UI canvas is reserved first (a menu you cannot see is worse than a missing parallax
            // layer), then map/stream layers fill what is left, front priority first.
            const MAX_BG: usize = 4;
            let ui_slots = usize::from(ctx.ui_bg.is_some());
            let map_budget = MAX_BG - ui_slots;
            let mut shown = 0usize;
            let mut dma_taken = false;

            // Mode 7 ground planes first: they sit behind everything and, when one is active, it
            // claims the single HBlank DMA slot. A game cannot have both a 3D floor and per-scanline
            // band parallax — agb's `GraphicsFrame` holds one DMA and its transfer hardcodes channel
            // 0 — and of the two, the floor is the one that stops being a floor without it.
            let mut m7_live = false;
            for a in ctx.affine_bgs.iter_mut() {
                if !a.visible || shown >= map_budget {
                    continue;
                }
                let has_m7 = a.m7.is_some();
                if let Some(m) = a.m7.as_ref() {
                    mode7_rows(m);
                }
                if has_m7 {
                    // ⚠️ Scanline 0 is NOT covered by the DMA. `HBlankDma` sources from `values[1..]`,
                    // so its first transfer lands in the HBlank after line 0 and applies to line 1.
                    // Line 0 is covered instead by `m7_arm_dma`, which deposits row 0 into the
                    // registers directly at VCOUNT 227 — during vblank, where nothing is drawing.
                    //
                    // Nothing else may touch BG2PA..BG2Y. `AffineTransformSource::External` is what
                    // says so: agb otherwise writes the layer transform AND the scroll from inside
                    // `commit()`, and on a frame heavy enough to overrun vblank that write lands on
                    // a visible scanline and repaints it with the whole-background matrix. That is
                    // the stray horizontal line, and it moves up and down the screen with the frame
                    // cost, which is what made it look random rather than like a budget symptom.
                    m7_live = true;
                }
                let id = a.bg.show(&mut frame);
                shown += 1;
                let _ = id;
                if has_m7 {
                    // The vblank tail is sky, so an overrunning DMA can only ever latch sky — and
                    // sky is the all-zero matrix the table is already initialised to, and nothing
                    // ever writes those 68 entries. It used to be re-stamped every frame from
                    // `rows[0]`, which is 1KB of stores to keep constants constant.
                    dma_taken = true;
                }
            }
            M7_ARMED.0.set(m7_live);
            let (pcx, pcy) = (ctx.camera_x + shake_x, ctx.camera_y + shake_y);
            for b in ctx.backgrounds.iter_mut() {
                // Parallax: re-derive this layer's scroll from the camera the engine wrote earlier
                // in this same step (see `bg_parallax`). Done here, immediately before `show`, so
                // the sky can never trail the world by a frame.
                if let Some((mx, my)) = b.parallax {
                    b.bg.set_scroll_pos(Vector2D::new(pcx * mx / 256, pcy * my / 256));
                }
                if b.visible && shown < map_budget {
                    let id = b.bg.show(&mut frame);
                    ctx.bg_ids_buf.push(id);
                    shown += 1;
                    if !dma_taken {
                        if let Some(bands) = b.bands.as_ref() {
                            if !bands.is_empty() {
                                attach_band_dma(&mut frame, id, bands, pcx);
                                dma_taken = true;
                            }
                        }
                    }
                }
            }
            // Scene backdrops (the .tmj layers Tiled gave a parallax factor). Shown before the
            // streamed world so that two backdrops sharing a priority break their tie by .tmj order
            // — agb hands out background numbers in `show` order, and the lower number draws in
            // front. One of them may carry per-scanline bands; only one can, because agb's frame
            // holds a single DMA slot and the HBlank transfer is hardcoded to DMA channel 0.
            for i in 0..ctx.scene_bg_active {
                if shown >= map_budget {
                    break;
                }
                let (mx, my) = ctx.scene_bgs[i].parallax.unwrap_or(PAR_WORLD);
                if !ctx.scene_bgs[i].visible {
                    continue;
                }
                // Banded layers still take a whole-layer scroll: the DMA drives BGxHOFS per
                // scanline, but BGxVOFS is written once and applies to the layer.
                ctx.scene_bgs[i]
                    .bg
                    .set_scroll_pos(Vector2D::new(pcx * mx / 256, pcy * my / 256));
                let id = ctx.scene_bgs[i].bg.show(&mut frame);
                ctx.bg_ids_buf.push(id);
                shown += 1;
                if dma_taken {
                    continue;
                }
                let Some(bands) = ctx.scene_bgs[i].bands.as_ref() else {
                    continue;
                };
                if bands.is_empty() {
                    continue;
                }
                attach_band_dma(&mut frame, id, bands, pcx);
                dma_taken = true;
            }
            // Streamed layers were already scrolled + filled by prime_stream_layers above.
            // show_if_done keeps a half-filled layer off screen (black backdrop) rather than
            // flashing garbage tiles.
            for layer in ctx.stream_layers.iter().take(ctx.stream_active) {
                if shown >= map_budget {
                    break;
                }
                // A hidden layer costs no background slot, so a scene can carry more layers than the
                // four the hardware shows as long as they are not all lit at once.
                if !layer.visible {
                    continue;
                }
                if let Some(id) = layer.map.show_if_done(&mut frame) {
                    ctx.bg_ids_buf.push(id);
                    shown += 1;
                }
            }
            // UI text canvas (menus/dialog): P0 so it sits above every streamed map layer (Ground P3,
            // Paths P2, Props P1, …). HUD/portrait sprites are also P0 and draw on top of this BG.
            // The shake moves the UI CANVAS too, and this is the whole bug it was shipped with:
            // `fx_shake` originally offset only the camera, which moves backgrounds and world
            // sprites. A screen drawn entirely on the UI canvas with HUD sprites over it — a result
            // screen, a menu, the fx demo itself — has no camera-relative pixels at all, so the
            // shake was invisible everywhere it was actually wanted. Added to the game's own
            // scroll rather than replacing it, so a game that scrolls its canvas still can.
            //
            // Only while shaking, plus the one landing frame. Writing `ui_scroll_x` here every idle
            // frame would be two pointless register writes forever, and would silently overwrite a
            // game that set the canvas scroll through some other path.
            if ctx.shake_live || shake_landed {
                let (ux, uy) = (ctx.ui_scroll_x + shake_x, ctx.ui_scroll_y + shake_y);
                if let Some(bg) = ctx.ui_bg.as_mut() {
                    bg.set_scroll_pos(Vector2D::new(ux, uy));
                }
            }
            if let Some(id) = terrain_present(&mut ctx.terrain, Vector2D::new(pcx, pcy), &mut frame)
            {
                ctx.bg_ids_buf.push(id);
            }
            if let Some(bg) = ctx.ui_bg.as_ref() {
                ctx.bg_ids_buf.push(bg.show(&mut frame));
            }
            // Game sprites, drawn relative to the camera (UI/dialogue stays screen-space).
            let (cx, cy) = (ctx.camera_x + shake_x, ctx.camera_y + shake_y);
            // OAM ORDER, NOT PRIORITY, decides sprite-vs-sprite overlap: agb assigns OAM slots in
            // show() order and an EARLIER slot draws in front. Sprite priority only orders sprites
            // against BACKGROUNDS. So the order here is front-to-back: TEXT, then HUD sprites, then
            // world sprites.
            //
            // Text used to be shown LAST, under a comment claiming it was "front, over everything" —
            // the exact opposite, which is why a judgment call-out over a character was hidden by it.
            for slot in ctx.hud_text.iter_mut() {
                if !slot.visible {
                    continue;
                }
                for obj in slot.objs.iter_mut() {
                    obj.set_priority(Priority::P0);
                    obj.set_graphics_mode(obj_mode);
                    obj.show(&mut frame);
                }
                // Inline colour emoji, over the text glyphs (they carry their own palette).
                for obj in slot.emoji_objs.iter_mut() {
                    obj.set_priority(Priority::P0);
                    obj.set_graphics_mode(obj_mode);
                    obj.show(&mut frame);
                }
            }
            for s in ctx.sprites.iter_mut() {
                if s.visible && s.hud && !s.billboard {
                    if let Some(obj) = s.object.as_mut() {
                        // Shake applies here too: HUD sprites are screen-space, so without this a
                        // shaking canvas slides out from under particles nailed to the screen.
                        obj.set_pos(Vector2D::new(s.x + shake_x, s.y + shake_y));
                        obj.set_priority(explicit_priority(s.priority, Priority::P0));
                        obj.set_hflip(s.hflip);
                        obj.set_graphics_mode(obj_mode);
                        obj.show(&mut frame);
                    }
                }
            }
            // Mode 7 billboards: behind the HUD proper, and among themselves ordered by DISTANCE.
            //
            // They are HUD sprites because the projection returns screen coordinates, but they are
            // world objects — two karts side by side must be covered by whichever is nearer, not by
            // whichever was created first. `mode7_billboards_draw` already computes each one's screen
            // depth; this is the pass that finally uses it. Nearest first, because an earlier OAM slot
            // draws in front.
            ctx.sprite_order_buf.clear();
            for (i, s) in ctx.sprites.iter().enumerate() {
                if s.visible && s.hud && s.billboard && s.object.is_some() {
                    ctx.sprite_order_buf.push(i);
                }
            }
            ctx.sprite_order_buf
                .sort_by(|&a, &b| ctx.sprites[b].depth.cmp(&ctx.sprites[a].depth));
            for idx in 0..ctx.sprite_order_buf.len() {
                let i = ctx.sprite_order_buf[idx];
                let s = &mut ctx.sprites[i];
                if let Some(obj) = s.object.as_mut() {
                    obj.set_pos(Vector2D::new(s.x + shake_x, s.y + shake_y));
                    obj.set_priority(explicit_priority(s.priority, Priority::P0));
                    obj.set_hflip(s.hflip);
                    obj.set_graphics_mode(obj_mode);
                    obj.show(&mut frame);
                }
            }
            // World sprites at priority 2, in DEPTH order (higher depth shown first → wins the overlap
            // via the earlier OAM slot). Flat top-down scenes leave depth=0 ⇒ registration order.
            ctx.sprite_order_buf.clear();
            for (i, s) in ctx.sprites.iter().enumerate() {
                if s.visible && !s.hud && s.object.is_some() {
                    ctx.sprite_order_buf.push(i);
                }
            }
            ctx.sprite_order_buf
                .sort_by(|&a, &b| ctx.sprites[b].depth.cmp(&ctx.sprites[a].depth));
            for idx in 0..ctx.sprite_order_buf.len() {
                let i = ctx.sprite_order_buf[idx];
                let s = &mut ctx.sprites[i];
                if let Some(obj) = s.object.as_mut() {
                    obj.set_pos(Vector2D::new(s.x - cx, s.y - cy));
                    obj.set_priority(explicit_priority(s.priority, Priority::P2));
                    obj.set_hflip(s.hflip);
                    obj.set_graphics_mode(obj_mode);
                    obj.show(&mut frame);
                }
            }
            // Dialogue: advance the typewriter (draw the next body group into the text
            // background), then draw the panel and the text background over the scene.
            if ctx.dialog_active {
                ctx.dialog_timer += 1;
                let idx = ctx.dialog_revealed;
                if ctx.dialog_timer >= 2 && idx < ctx.dialog_groups.len() {
                    ctx.dialog_timer = 0;
                    if let (Some(bg), Some(r)) =
                        (ctx.dialog_text_bg.as_mut(), ctx.dialog_body.as_mut())
                    {
                        r.show(bg, &ctx.dialog_groups[idx]);
                    }
                    ctx.dialog_revealed = idx + 1;
                    // Classic RPG text chirp — one short blip per revealed letter group.
                    if ctx.dialog_blip >= 0 {
                        if let Some(data) = tishlang_runtime_gba::gba::asset_wav(ctx.dialog_blip) {
                            ensure_mixer_fields(&mut ctx.mixer, ctx.gba, ctx.psg_ready);
                            ctx.audio_used = true;
                            let channel = SoundChannel::new(data);
                            let _ = ctx.mixer.as_mut().unwrap().play_sound(channel);
                        }
                    }
                }
                for (i, obj) in ctx.dialog_box.iter_mut().enumerate() {
                    let (cx, ry) = ((i % 4) as i32, (i / 4) as i32);
                    obj.set_pos(Vector2D::new(cx * 64, 104 + ry * 28));
                    obj.set_priority(Priority::P1);
                    obj.set_graphics_mode(obj_mode);
                    obj.show(&mut frame);
                }
                if let Some(bg) = ctx.dialog_text_bg.as_ref() {
                    bg.show(&mut frame);
                }
            }
            // HUD bars (health/progress) — front, screen space, drawn under the HUD text.
            for bar in ctx.hud_bars.iter_mut() {
                if let Some(obj) = bar.obj.as_mut() {
                    obj.set_pos(Vector2D::new(bar.x, bar.y));
                    obj.set_priority(Priority::P0);
                    obj.set_graphics_mode(obj_mode);
                    obj.show(&mut frame);
                }
            }
            // Scene-transition fade: darken every shown background, the backdrop, and (via their
            // AlphaBlending mode, set above) every sprite toward black by `fade_level`/16. Skipped
            // entirely at level 0 so a non-transitioning game pays nothing.
            if fading {
                let blend = frame.blend();
                let mut effect = blend.darken(Num::from_raw(fade_level));
                effect.enable_backdrop();
                effect.enable_object();
                for id in ctx.bg_ids_buf.iter() {
                    effect.enable_background(*id);
                }
            } else if white_level > 0 || flash_level > 0 {
                // The white counterpart, on the same hardware register. Mutually exclusive with the
                // fade because BLDY is one register: a scene cannot darken and brighten at once, and
                // a transition must win — otherwise a flash fired near a scene change would fight
                // the fade and the screen would strobe.
                //
                // Between the two whites, the transition's `fade_white` outranks `fx_flash` for the
                // same reason: a hit-spark must not interrupt a scene change.
                let level = if white_level > 0 {
                    white_level
                } else {
                    flash_level
                };
                let blend = frame.blend();
                let mut effect = blend.brighten(Num::from_raw(level));
                effect.enable_backdrop();
                effect.enable_object();
                for id in ctx.bg_ids_buf.iter() {
                    effect.enable_background(*id);
                }
            } else if let Some((top, bot)) = alpha_weights {
                // Last on BLDCNT, and only when no whole-screen ramp is live. `Num<u8, 4>` is the
                // 0..16 raw the hardware takes, the same scale as fade/flash.
                let blend = frame.blend();
                // ⚠️ Alpha is TWO-SIDED, so unlike the fade effects every enable takes a layer:
                // the top is the sprites (which is why `obj_mode` must be AlphaBlending for them),
                // the bottom is the backgrounds and the backdrop they blend against.
                use agb::display::Layer;
                let mut effect = blend.alpha(Num::from_raw(top), Num::from_raw(bot));
                effect.enable_object(Layer::Top);
                effect.enable_backdrop(Layer::Bottom);
                for id in ctx.bg_ids_buf.iter() {
                    effect.enable_background(Layer::Bottom, *id);
                }
            }
            // ── Hardware windows ────────────────────────────────────────────────────────────
            // Applied here, after every layer has been shown, because a window mask selects layers
            // BY DRAW ORDER — `ctx.bg_ids_buf` is only complete at this point.
            // ⚠️ GUARDED ON A WINDOW BEING ON, and nothing else. The GBA's window unit is switched
            // on by the DISPCNT bits for WIN0/WIN1/OBJWIN, so WINOUT means nothing on its own — and
            // an earlier guard that also fired on `win_out_mask != 0x3F` sent EVERY ROM in the repo
            // through this block (the mask defaulted to 0), which was harmless only by luck.
            if ctx.win_on[0] || ctx.win_on[1] {
                let shown_bgs = ctx.bg_ids_buf.len();
                // agb's Window takes one `enable_*` call per layer rather than a mask, so unpack.
                let apply =
                    |w: &mut agb::display::Window,
                     mask: u8,
                     ids: &[agb::display::tiled::RegularBackgroundId]| {
                        for (i, id) in ids.iter().enumerate() {
                            if i < 4 && (mask >> i) & 1 != 0 {
                                w.enable_background(*id);
                            }
                        }
                        if mask & 0x10 != 0 {
                            w.enable_objects();
                        }
                        if mask & 0x20 != 0 {
                            w.enable_blending();
                        }
                    };
                let bg_ids: alloc::vec::Vec<_> = ctx.bg_ids_buf.to_vec();
                let circle = ctx.win_circle;
                let win_on = ctx.win_on;
                let win_box = ctx.win_box;
                let win_in_mask = ctx.win_in_mask;
                let win_out_mask = ctx.win_out_mask;
                let _ = shown_bgs;
                {
                    let windows = frame.windows();
                    apply(windows.win_out(), win_out_mask, &bg_ids);
                    if win_on[0] {
                        let w0 = windows.win_in(agb::display::WinIn::Win0);
                        match circle {
                            // A circle sets the window's VERTICAL extent to the rows the circle
                            // covers and leaves the horizontal extent to the DMA below, which
                            // rewrites it per scanline. Rows outside the circle are excluded by the
                            // vertical extent, so the DMA never has to blank them.
                            Some((cx, cy, r)) => {
                                let top = (cy - r).clamp(0, 160);
                                let bot = (cy + r).clamp(0, 160);
                                w0.set_pos(agb::fixnum::rect(
                                    agb::fixnum::vec2(0, top),
                                    agb::fixnum::vec2(240, (bot - top).max(0)),
                                ));
                                let _ = cx;
                            }
                            None => {
                                let (x, y, w, h) = win_box[0];
                                w0.set_pos(agb::fixnum::rect(
                                    agb::fixnum::vec2(x, y),
                                    agb::fixnum::vec2(w, h),
                                ));
                            }
                        }
                        apply_movable(w0, win_in_mask[0], &bg_ids);
                    }
                    if win_on[1] {
                        let (x, y, w, h) = win_box[1];
                        let w1 = windows.win_in(agb::display::WinIn::Win1);
                        w1.set_pos(agb::fixnum::rect(
                            agb::fixnum::vec2(x, y),
                            agb::fixnum::vec2(w, h),
                        ));
                        apply_movable(w1, win_in_mask[1], &bg_ids);
                    }
                }
                // ⚠️ The circle claims the SINGLE HBlank DMA slot, exactly like a mode 7 floor or a
                // banded layer — and until this check existed it claimed it LAST and unconditionally,
                // so an iris over a banded parallax scene overwrote the bands with no diagnostic at
                // all. Losing an iris mid-transition is the more visible failure of the two, so the
                // circle yields to whoever claimed first and says so once.
                if circle.is_some() && dma_taken {
                    if !ctx.warned_win_dma {
                        ctx.warned_win_dma = true;
                        agb::println!(
                            "tish-agb: win_circle dropped — the HBlank DMA slot is already taken by a mode 7 floor or bg_bands. Use win_rect for the transition, or drop the bands for its duration."
                        );
                    }
                } else if let Some((cx, cy, r)) = circle {
                    dma_taken = true;
                    let _ = dma_taken; // last claimant today; kept so the next one inherits the guard
                                       // One (left, right) pair per scanline. `HBlankDma` sources from `values[1..]`,
                                       // so entry 0 is never used — the same off-by-one the mode 7 floor documents.
                    let mut edges = [agb::fixnum::vec2(0u8, 0u8); 160];
                    for (y, slot) in edges.iter_mut().enumerate() {
                        let dy = y as i32 - cy;
                        let d2 = r * r - dy * dy;
                        if d2 <= 0 {
                            // Outside the circle's rows: collapse to a zero-width span.
                            *slot = agb::fixnum::vec2(0u8, 0u8);
                        } else {
                            let half = isqrt_u32(d2 as u32) as i32;
                            let l = (cx - half).clamp(0, 240) as u8;
                            let rr = (cx + half).clamp(0, 240) as u8;
                            // ⚠️ (RIGHT, LEFT), NOT (LEFT, RIGHT). The register is
                            // `(left << 8) | right`, and a `Vector2D<u8>` lays x down in the LOW
                            // byte — so x must carry the RIGHT edge. Written the intuitive way
                            // round, every span comes out with X1 > X2, which the hardware treats
                            // as the WRAP-AROUND window (x >= X1 or x < X2) — precisely the
                            // complement. The demo drew a perfect circle of darkness with the
                            // checkerboard outside it, which looks like an inverted mask and is
                            // really two swapped bytes.
                            *slot = agb::fixnum::vec2(rr, l);
                        }
                    }
                    let w0 = frame.windows().win_in(agb::display::WinIn::Win0);
                    let dma = w0.horizontal_pos_dma();
                    agb::dma::HBlankDma::new(dma, &edges).show(&mut frame);
                }
            }
            __t1 = ctx.timer.value() as i32; // after building/showing the draw list, before the mixer
                                             // Pump the software mixer HERE — at the end of the frame, JUST BEFORE the vblank wait — as the
                                             // agb docs require ("do the per-frame work towards the end of the frame, just before waiting for
                                             // vblank"). agb's mixer is triple-buffered and swaps buffers on a hardware TIMER interrupt; the
                                             // buffer must be FILLED before that swap consumes it. tish-agb previously called `mixer.frame()`
                                             // AFTER `commit()` (i.e. after the vblank wait), so a buffer could be swapped-in before it was
                                             // filled → the DMA replayed stale audio → periodic underrun/stutter even at a locked 60fps.
                                             // Once per frame matches the one-swap-per-frame timer rate; the triple buffer covers a late frame.
                                             // Skipped until a game first plays a sound (a silent game keeps this whole slice of the budget).
                                             // Inlined (not `pump_audio`) because `frame` holds a borrow of ctx.gfx here; these are all
                                             // DISJOINT fields (ctx.mixer/timer/dbg_*), which the borrow checker accepts, whereas a
                                             // whole-`&mut ctx` helper call would conflict (E0499).
            if ctx.audio_used && !ctx.audio_pumping {
                if let Some(mixer) = ctx.mixer.as_mut() {
                    let now = ctx.timer.value() as i32;
                    let d = now - ctx.dbg_lastpump;
                    let gap = if d < 0 { d + 65536 } else { d };
                    if ctx.dbg_lastpump != 0 && gap > ctx.dbg_pumpgap {
                        ctx.dbg_pumpgap = gap;
                    }
                    ctx.dbg_lastpump = now;
                    ctx.audio_pumping = true;
                    mixer.frame();
                    ctx.audio_pumping = false;
                }
            }
            __t2 = ctx.timer.value() as i32; // mixer pump cost = __t2 - __t1
            frame.commit();
            // MOSAIC and its enable bits, by hand, in the one window where that is safe — see
            // `apply_mosaic`. Skipped entirely when nothing has ever asked for a mosaic, so a game
            // that does not use it pays nothing (the register is already zero at reset).
            if mosaic_bg != 0 || mosaic_obj != 0 || mosaic_was_live {
                apply_mosaic(mosaic_bg, mosaic_obj);
            }
            // Remember where that landed, so next frame's layer transform can be the row this line
            // actually wants. 160..227 is vblank, where the write is harmless and no guess is needed.
            {
                let vc = (unsafe { REG_VCOUNT.read_volatile() } & 0x00FF) as usize;
                if vc < M7_LINES {}
            }
        }
        let __t3 = ctx.timer.value() as i32; // after commit (vblank wait); commit(+wait) = __t3 - __t2
        ctx.input.update();
        // Record per-section maxima (Timer2 ticks; 1 vblank ≈ 4389). dbg_render = draw-list build/show,
        // dbg_commit = the commit()/vblank wait, dbg_mix = the mixer pump, dbg_total = whole-loop period
        // (t0→t0), dbg_drops = periods over ~1.5 vblanks (a dropped frame → the BGM underruns/stutters).
        let wrap = |d: i32| -> i32 {
            if d < 0 {
                d + 65536
            } else {
                d
            }
        };
        let render = wrap(__t1 - __t0);
        let mix = wrap(__t2 - __t1);
        let commit = wrap(__t3 - __t2);
        if render > ctx.dbg_render {
            ctx.dbg_render = render;
        }
        if commit > ctx.dbg_commit {
            ctx.dbg_commit = commit;
        }
        if mix > ctx.dbg_mix {
            ctx.dbg_mix = mix;
        }
        if ctx.dbg_last != 0 {
            let period = wrap(__t0 - ctx.dbg_last);
            if period > ctx.dbg_total {
                ctx.dbg_total = period;
            }
            if period > 6500 {
                ctx.dbg_drops += 1;
            }
        }
        ctx.dbg_last = __t0;
    });
    Value::Null
}

/// `audio_pump()` — keep BGM alive during long tish-side work (uiRender, list scroll refill, …).
///
/// Advances the chiptune/DECK sequencer for each elapsed display frame since the last pump (so PSG
/// note timing does not stall when `frame()` is not reached), and fills DirectSound mixer buffers.
/// Safe to call often: when less than one frame of wall time has passed it only attempts a mixer
/// fill, which early-returns when buffers are already full.
pub fn audio_pump(_args: &[Value]) -> Value {
    music_catchup();
    Value::Null
}

/// `audio_reserve()` — construct the software mixer NOW, at boot, while the heap still holds its
/// large contiguous runs. The lazy construct (`ensure_mixer`, first sampled-audio use) postpones
/// agb's mixer buffer allocation to the first town's `music_play` — by which point module
/// registration has fragmented the arena, and on a tight layout the allocation panics inside
/// `sw_mixer` at town entry. Same idea as `ui_reserve_tiles`: pay the big allocation at a moment
/// when the memory is provably there. Marks audio as in use so the swap IRQ is fed from the first
/// frame; a silent or PSG-only game simply does not call this and keeps the lazy path.
pub fn audio_reserve(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        ensure_mixer(ctx);
        ctx.audio_used = true;
    });
    Value::Null
}

/// `audio_defer(on)` — when `on != 0`, UI natives (`ui_text_box`, `ui_text`, `ui_rect`, …) skip their
/// per-call mixer feed so a batch of opaque patches is not dominated by music catch-up. Call
/// `audio_pump()` after clearing defer (`audio_defer(0)`).
pub fn audio_defer(args: &[Value]) -> Value {
    let on = if args.is_empty() {
        1
    } else {
        num(args, 0) as i32
    };
    AUDIO_DEFER.with(|c| c.set(if on != 0 { 1 } else { 0 }));
    Value::Null
}

/// `ticks() -> i32` — the free-running Timer2 value (same clock `frame_stats` reports; 1 vblank ≈ 4389,
/// wraps at 65536). `frame_stats` only gives whole-frame maxima, which cannot say WHICH part of a
/// keypress blew the budget; bracket a suspect section with two `ticks()` reads and log the difference.
pub fn ticks(_args: &[Value]) -> Value {
    with_ctx(|ctx| Value::Number(ctx.timer.value() as f64))
}

/// `heap_free(blockSize?) -> i32` — bytes still claimable from the EWRAM heap, measured by claiming
/// `blockSize`-byte blocks (default 1024) until the allocator says no and then handing them all back.
///
/// A GBA has no OOM killer and no error to catch: the allocator returns null, `handle_alloc_error` fires
/// and the game is over. The only defence is knowing the headroom BEFORE shipping a screen, so bracket a
/// suspect flow with this the way you'd bracket a slow one with `ticks`.
///
/// `blockSize` is the diagnostic edge: the count is in whole blocks, so the answer is "how much is usable
/// in pieces this size", not "how many bytes are free". Comparing two sizes SEPARATES A LEAK FROM
/// FRAGMENTATION — the failure they produce is identical (the next allocation dies) but the fix is not:
///
/// - both numbers fall together → memory is genuinely still held; find the owner.
/// - `heap_free(64)` holds steady while `heap_free(1024)` falls → the bytes are free but chopped into
///   pieces too small to serve a big request. Chasing an owner is wasted effort; what helps is allocating
///   the big, long-lived thing earlier, or reusing one buffer instead of freeing and re-taking it.
///
/// A GBA game hits the second case easily: a scene load takes a few large blocks (a tilemap, a map's
/// tiles) around many small ones that outlive them.
///
/// The probe itself allocates nothing besides the blocks it counts (it threads a free list through them), so
/// it is safe to leave in a tight spot — but it does briefly claim the entire heap, so don't call it from a
/// place where another allocation could interleave. `blk` must be at least 16 bytes.
pub fn heap_free(args: &[Value]) -> Value {
    let blk = match args.first() {
        Some(Value::Number(n)) if *n >= 16.0 => (*n as usize) & !3,
        _ => 1024,
    };
    let layout = match core::alloc::Layout::from_size_align(blk, 4) {
        Ok(l) => l,
        Err(_) => return Value::Number(0.0),
    };
    // The blocks ARE the bookkeeping: each one stores the previous block's address in its first word, so the
    // probe allocates nothing but what it is measuring. A Vec of pointers here was self-defeating — probing
    // with a 64-byte block needs thousands of slots, and `Vec::with_capacity` asking for that one contiguous
    // 16K is exactly the kind of allocation a fragmented heap cannot serve. It killed the game it was called
    // to diagnose, and its capacity cap silently truncated the answer at the small sizes that matter most.
    let mut head: *mut u8 = core::ptr::null_mut();
    let mut n: usize = 0;
    loop {
        let p = unsafe { alloc::alloc::alloc(layout) };
        if p.is_null() {
            break;
        }
        unsafe { core::ptr::write_unaligned(p as *mut *mut u8, head) };
        head = p;
        n += 1;
    }
    while !head.is_null() {
        let next = unsafe { core::ptr::read_unaligned(head as *mut *mut u8) };
        unsafe { alloc::alloc::dealloc(head, layout) };
        head = next;
    }
    Value::Number((n * blk) as f64)
}

/// `iwram_free(blockSize?) -> i32` — the same probe as [`heap_free`], but for **IWRAM**.
///
/// ⚠️ READ THIS BEFORE CONCLUDING "NOT MEMORY" FROM `heap_free`. A GBA program has TWO heaps, and
/// `heap_free` only sees one of them:
///
/// - **EWRAM** (`ExternalAllocator`, the global `alloc`): 256 KB, where nearly everything lives.
/// - **IWRAM** (`InternalAllocator`): 32 KB, **shared with the stack**, and the DEFAULT for several
///   agb constructors — `DynamicSprite16::new`, `DynamicSprite256::new` and the mixer's buffer all
///   land here unless you pass an allocator explicitly.
///
/// That asymmetry cost this repo a full day. Walking into an overworld cave crashed inside
/// `BlockAllocatorInner::alloc` on the first `hudText` after the scene load, and `heap_free`
/// reported 83 KB free one line earlier — so memory was ruled out and the hunt went to sprite VRAM,
/// palette banks and use-after-free in turn. The exhausted arena was IWRAM, which nothing could
/// measure. Bracket a suspect flow with BOTH probes, always.
///
/// ⚠️ FAR more invasive than `heap_free`. It claims the WHOLE arena for the duration, and unlike
/// EWRAM that is immediately fatal: IWRAM is allocated from during ordinary play (agb's mixer, the
/// dynamic-sprite staging buffers), so anything that runs while the probe holds it — including an
/// interrupt — gets null and writes through it. Measured: calling this from `hudRefresh` killed the
/// ROM on the overworld's own load, three frames after the call. Use it at a QUIET point (a boot
/// step, a paused frame), read the number, and take the call back out. It is a bisecting tool, not
/// an instrument to leave in.
pub fn iwram_free(args: &[Value]) -> Value {
    let blk = match args.first() {
        Some(Value::Number(n)) if *n >= 16.0 => (*n as usize) & !3,
        _ => 256,
    };
    let layout = match core::alloc::Layout::from_size_align(blk, 4) {
        Ok(l) => l,
        Err(_) => return Value::Number(0.0),
    };
    // Same self-threading free list as `heap_free`, for the same reason: the probe must not need a
    // Vec, because a Vec big enough to index a 32 KB arena in 64-byte pieces is itself an
    // allocation this arena cannot serve.
    // CAPPED, unlike `heap_free`. Draining IWRAM outright is fatal within a few frames (see the
    // warning above), so this stops at `CAP` blocks and the answer saturates: a result equal to
    // `CAP * blk` means "at least that much", not "exactly that much". Enough to answer the only
    // question that matters — is this arena nearly empty? — without being the thing that kills the
    // frame it is measuring.
    const CAP: usize = 48;
    let mut head: *mut u8 = core::ptr::null_mut();
    let mut n: usize = 0;
    while n < CAP {
        let p = match core::alloc::Allocator::allocate(&agb::InternalAllocator, layout) {
            Ok(p) => p.as_ptr() as *mut u8,
            Err(_) => break,
        };
        unsafe { core::ptr::write_unaligned(p as *mut *mut u8, head) };
        head = p;
        n += 1;
    }
    while !head.is_null() {
        let next = unsafe { core::ptr::read_unaligned(head as *mut *mut u8) };
        unsafe {
            core::alloc::Allocator::deallocate(
                &agb::InternalAllocator,
                core::ptr::NonNull::new_unchecked(head),
                layout,
            )
        };
        head = next;
    }
    Value::Number((n * blk) as f64)
}

/// `ui_mem_report() -> string` — where the UI subsystem's heap went, in bytes of CAPACITY (not live use),
/// so a shrinking `heap_free` can be attributed instead of guessed at. Reports the tile table, the cell
/// grid, the typewriter's row buffer and its parked spare, the box compositing scratch, the text-width
/// memo and the sprite table. Diagnostic only — call it around a flow that loses memory.
/// `ui_release_scratch()` — hand back the UI's reusable scratch buffers: the typewriter's reveal-row
/// cache, its spare row buffer, and the text-box compositing buffer.
///
/// These are kept between screens deliberately — re-allocating them for every dialog box is the
/// common case, and that churn costs more than the bytes. A full-screen menu opening over a running
/// game is the opposite trade: a shop tab is ~60 KB and wants every byte, while these hold ~5 KB of
/// the conversation that just ended, which nothing on the incoming screen will read. They grow back
/// on the next box that needs them. `packages/shop` calls this next to `dialogFree()`.
/// Hand back the UI canvas's HEAP, not just its contents.
///
/// ⚠️ `ui_clear` + `ui_release_scratch` recover almost nothing, and the reason is `Vec::clear`:
/// it sets the length to zero and KEEPS the capacity. `ui_tiles` and the 32x32 `ui_cell` grid stay
/// allocated at whatever the busiest screen needed, so a game that shows a text-heavy opening and
/// then wants to start a battle is holding that peak forever. Measured on a large SRPG example: the
/// opening took the heap from 32,512 B down to 14,144, and clear+release handed back 2.9 KB of it.
/// The battle then died on an allocation with 15 KB free.
///
/// Replacing the containers (rather than clearing them) drops the buffers. Everything here is
/// rebuilt on the next `ui_begin`, so this is safe to call between screens -- but NOT mid-canvas.
pub fn ui_free(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        ui_forget_tiles(ctx);
        // The point of this function: drop CAPACITY, which `clear()` deliberately keeps.
        ctx.ui_tiles = alloc::vec::Vec::new();
        ctx.ui_cell = alloc::vec::Vec::new();
        ctx.ui_row_spare = alloc::vec::Vec::new();
        ctx.ui_box_scratch = alloc::vec::Vec::new();
        ctx.tw_cache = alloc::collections::BTreeMap::new();
        ctx.ui_reveal = None;
        ctx.ui_blank = None;
        ctx.ui_bg = None;
        ctx.ui_palettes = alloc::vec::Vec::new();
        ctx.ui_pal_overflow = 0;
        // Dropped tiles only leave VRAM on a commit; gc here so the next screen does not peak at
        // both canvases (same reason ui_clear does it).
        agb::display::tiled::VRAM_MANAGER.gc();
    });
    Value::Null
}

pub fn ui_free_typed() {
    ui_free(&[]);
}

pub fn ui_release_scratch(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        ctx.ui_reveal = None;
        ctx.ui_row_spare = alloc::vec::Vec::new();
        ctx.ui_box_scratch = alloc::vec::Vec::new();
    });
    Value::Null
}

pub fn ui_mem_report(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        let s = alloc::format!(
            "tiles {} peak {} solid {} cap {} cells {} rows {}/{} spare {} box {} tw {} spr {} pal {}/15 palovf {}",
            ctx.ui_tiles.len(),
            ctx.ui_peak_tiles,
            ctx.ui_cell
                .iter()
                .filter(|c| ui_cell_solid_pal(**c).is_some())
                .count(),
            ctx.ui_tiles.capacity() * core::mem::size_of::<UiTile>(),
            ctx.ui_cell.capacity() * 2,
            ctx.ui_reveal.as_ref().map(|c| c.rows.len()).unwrap_or(0),
            ctx.ui_reveal.as_ref().map(|c| c.rows.capacity() * 16).unwrap_or(0),
            ctx.ui_row_spare.capacity() * 16,
            ctx.ui_box_scratch.capacity() * 4,
            ctx.tw_cache.len(),
            ctx.sprites.len(),
            ctx.ui_palettes.len(),
            ctx.ui_pal_overflow,
        );
        Value::string(&s)
    })
}

/// `frame_stats()` — "rNNN cNNN mNNN pNNN dN": max render / commit(+vblank) / mix / period ticks and drop
/// count since the last read (1 vblank ≈ 4389 ticks). Reading RESETS the maxima so each window is fresh.
pub fn frame_stats(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        let s = alloc::format!(
            "r{} c{} m{} p{} d{} g{}",
            ctx.dbg_render,
            ctx.dbg_commit,
            ctx.dbg_mix,
            ctx.dbg_total,
            ctx.dbg_drops,
            ctx.dbg_pumpgap,
        );
        ctx.dbg_render = 0;
        ctx.dbg_commit = 0;
        ctx.dbg_mix = 0;
        ctx.dbg_total = 0;
        ctx.dbg_drops = 0;
        ctx.dbg_pumpgap = 0;
        Value::string(&s)
    })
}

// ── typed siblings (input/camera/HUD-canvas scalar surface) ─────────────────────
// Thin adapters over the boxed exports: same body executes, so behaviour cannot
// drift — the win is the caller-side namespace lookup + value_call dispatch +
// per-arg Value boxing (~72 vs ~7 ticks). See the isob_* block in
// tish-gba-game-engine for the design note; invert to direct internal calls
// per-fn if one ever shows in a profile.
pub fn audio_pump_typed() {
    audio_pump(&[]);
}
pub fn audio_reserve_typed() {
    audio_reserve(&[]);
}
pub fn camera_set_typed(p0: i32, p1: i32) {
    camera_set(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn camera_x_typed() -> i32 {
    match camera_x(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn camera_y_typed() -> i32 {
    match camera_y(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn deck_frame_typed() -> i32 {
    match deck_frame(&[]) {
        Value::Number(v) => v as i32,
        _ => -1,
    }
}
pub fn frame_typed() {
    frame(&[]);
}
pub fn key_live_typed(p0: i32) -> i32 {
    match key_live(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn key_released_typed(p0: i32) -> i32 {
    match key_released(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn keys_edge_typed() -> i32 {
    match keys_edge(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn keys_held_typed() -> i32 {
    match keys_held(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn map_solid_at_typed(p0: i32, p1: i32) -> i32 {
    match map_solid_at(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn sprite_set_depth_typed(p0: i32, p1: i32) {
    sprite_set_depth(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn sprite_set_hud_typed(p0: i32, p1: i32) {
    sprite_set_hud(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn sprite_set_priority_typed(p0: i32, p1: i32) {
    sprite_set_priority(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn sprite_set_sheet_typed(p0: i32, p1: i32, p2: i32) {
    native_sprite_set_sheet(p0, p1, p2);
}
pub fn sprite_set_visible_typed(p0: i32, p1: i32) {
    sprite_set_visible(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn text_height_typed(p0: i32) -> i32 {
    match text_height(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn ticks_typed() -> i32 {
    match ticks(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn timer_read_typed() -> i32 {
    match timer_read(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn ui_begin_typed() {
    ui_begin(&[]);
}
pub fn ui_clear_rect_typed(p0: i32, p1: i32, p2: i32, p3: i32) {
    ui_clear_rect(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
    ]);
}

// ── Native UI layout (packages/ui.tish) ──────────────────────────────────────────────────────────
// The flex solver, moved off the boxed path. See `ui_layout.rs` for why. The pool is a single-core
// static like every other piece of engine state here; a screen re-lays out with no allocation.
static UI_LAYOUT: SingleCore<RefCell<ui_layout::Pool>> =
    SingleCore::new(RefCell::new(ui_layout::Pool::new()));

fn lay_with<R>(f: impl FnOnce(&mut ui_layout::Pool) -> R) -> R {
    UI_LAYOUT.with(|c| f(&mut c.borrow_mut()))
}

fn argi(args: &[Value], i: usize) -> i32 {
    match args.get(i) {
        Some(Value::Number(n)) => *n as i32,
        _ => 0,
    }
}

/// Drop the live node count, keeping the pooled slots.
pub fn lay_reset(_args: &[Value]) -> Value {
    lay_with(|p| p.reset());
    Value::Null
}

/// Append a node; returns its index. Field order matches `interface LNode`.
pub fn lay_push(args: &[Value]) -> Value {
    let idx = lay_with(|p| {
        p.push(
            argi(args, 0),
            argi(args, 1),
            argi(args, 2),
            argi(args, 3),
            argi(args, 4),
            argi(args, 5),
            argi(args, 6),
            argi(args, 7),
            argi(args, 8),
            argi(args, 9),
            argi(args, 10),
            argi(args, 11),
        )
    });
    Value::Number(idx as f64)
}

/// A leaf's measured size (text/icon extents are the font's business, so tish supplies them).
pub fn lay_set_measured(args: &[Value]) -> Value {
    lay_with(|p| p.set_measured(argi(args, 0), argi(args, 1), argi(args, 2)));
    Value::Null
}

/// Run measure + arrange into the given root box.
pub fn lay_solve(args: &[Value]) -> Value {
    lay_with(|p| p.solve(argi(args, 0), argi(args, 1), argi(args, 2), argi(args, 3)));
    Value::Null
}

fn lay_field(args: &[Value], f: impl Fn(&ui_layout::Node) -> i32) -> Value {
    let i = argi(args, 0) as usize;
    Value::Number(lay_with(|p| p.nodes.get(i).map(&f).unwrap_or(0)) as f64)
}

pub fn lay_x(args: &[Value]) -> Value {
    lay_field(args, |n| n.x)
}
pub fn lay_y(args: &[Value]) -> Value {
    lay_field(args, |n| n.y)
}
pub fn lay_w(args: &[Value]) -> Value {
    lay_field(args, |n| n.cw)
}
pub fn lay_h(args: &[Value]) -> Value {
    lay_field(args, |n| n.ch)
}
pub fn lay_hide(args: &[Value]) -> Value {
    lay_field(args, |n| n.hide)
}
pub fn lay_mw(args: &[Value]) -> Value {
    lay_field(args, |n| n.mw)
}
pub fn lay_mh(args: &[Value]) -> Value {
    lay_field(args, |n| n.mh)
}
pub fn lay_content(args: &[Value]) -> Value {
    lay_field(args, |n| n.content)
}
pub fn lay_view(args: &[Value]) -> Value {
    lay_field(args, |n| n.view)
}
pub fn lay_count(_args: &[Value]) -> Value {
    Value::Number(lay_with(|p| p.count) as f64)
}

// Typed entries — the direct-call path the `declare fn` signatures bind to (no Value boxing).
pub fn lay_reset_typed() {
    lay_with(|p| p.reset());
}
#[allow(clippy::too_many_arguments)]
pub fn lay_push_typed(
    kind: i32,
    dir: i32,
    gap: i32,
    pad: i32,
    fw: i32,
    fh: i32,
    grow: i32,
    am: i32,
    jm: i32,
    scroll: i32,
    sy: i32,
    parent: i32,
) -> i32 {
    lay_with(|p| {
        p.push(
            kind, dir, gap, pad, fw, fh, grow, am, jm, scroll, sy, parent,
        )
    })
}
pub fn lay_set_measured_typed(i: i32, mw: i32, mh: i32) {
    lay_with(|p| p.set_measured(i, mw, mh));
}
pub fn lay_solve_typed(x: i32, y: i32, w: i32, h: i32) {
    lay_with(|p| p.solve(x, y, w, h));
}
fn lay_field_typed(i: i32, f: impl Fn(&ui_layout::Node) -> i32) -> i32 {
    lay_with(|p| p.nodes.get(i as usize).map(&f).unwrap_or(0))
}
pub fn lay_x_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.x)
}
pub fn lay_y_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.y)
}
pub fn lay_w_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.cw)
}
pub fn lay_h_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.ch)
}
pub fn lay_hide_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.hide)
}
pub fn lay_mw_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.mw)
}
pub fn lay_mh_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.mh)
}
pub fn lay_content_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.content)
}
pub fn lay_view_typed(i: i32) -> i32 {
    lay_field_typed(i, |n| n.view)
}
pub fn lay_count_typed() -> i32 {
    lay_with(|p| p.count) as i32
}

/// Paint every laid-out node in one crossing.
///
/// ⚠️ THIS IS THE POINT OF THE PAINT PORT. The boxed pass cost several `Value` property reads per
/// node PER FRAME — text, font, colour, align, geometry — and a screen is redrawn constantly. Here
/// the node state was resolved once at flatten (`lay_set_paint` / `lay_set_text`), so a frame is
/// ONE call and the loop never touches tish.
///
/// Custom-paint leaves (`paint_kind == 3`) are skipped: the game draws those imperatively and the
/// engine only owes them their box. The caller paints them itself after this returns.
pub fn ui_paint_all(_args: &[Value]) -> Value {
    ui_paint_all_typed();
    Value::Null
}

/// Paint forward from `start` until a node the native pass cannot draw, returning ITS index (or the
/// node count when the screen is finished).
///
/// ⚠️⚠️ ORDER IS LOAD-BEARING: a container draws its border before its children, in one forward
/// pass. So this cannot paint "everything it can" and leave the rest to a second sweep — that would
/// lift icons and custom leaves above borders painted later. It stops, the caller draws that one
/// node the boxed way, and calls again from the next index. A screen of text and containers costs
/// ONE crossing; four icons cost five.
pub fn ui_paint_from_typed(start: i32) -> i32 {
    let count = lay_with(|p| p.count) as i32;
    let mut i = start.max(0);
    while i < count {
        let (n, text) = lay_with(|p| {
            let n = p.nodes[i as usize];
            let t = if n.paint_kind == 1 {
                p.text.get(i as usize).cloned().unwrap_or_default()
            } else {
                alloc::string::String::new()
            };
            (n, t)
        });
        // 2 = icon (sprite pool), 3 = custom paint callback, 4 = text needing reveal/ellipsis —
        // all of which live on the tish side.
        if n.paint_kind >= 2 {
            return i;
        }
        if n.hide == 0 {
            paint_one(&n, &text);
        }
        i += 1;
    }
    count
}

pub fn ui_paint_from(args: &[Value]) -> Value {
    Value::Number(ui_paint_from_typed(argi(args, 0)) as f64)
}

fn paint_one(n: &ui_layout::Node, text: &str) {
    if n.sel != 0 {
        ui_rect(&[
            Value::Number((n.x - 1) as f64),
            Value::Number((n.y - 1) as f64),
            Value::Number((n.cw + 2) as f64),
            Value::Number((n.ch + 2) as f64),
            Value::Number(n.col as f64),
        ]);
    }
    match n.paint_kind {
        1 => {
            let maxw = if n.use_w > 0 { n.cw } else { 512 };
            let align = match n.align {
                1 => "center",
                2 => "right",
                _ => "left",
            };
            let mut args = alloc::vec![
                Value::Number(n.font as f64),
                Value::Number(n.x as f64),
                Value::Number(n.y as f64),
                Value::String(text.into()),
                Value::Number(n.col as f64),
                Value::Number(maxw as f64),
                Value::String(align.into()),
            ];
            if n.shadowc >= 0 {
                args.push(Value::Number(n.shadowc as f64));
                args.push(Value::Number(n.shadow_off as f64));
                args.push(Value::Number(n.shadow_off as f64));
            }
            ui_text(&args);
        }
        0 => {
            if n.fillc >= 0 {
                ui_rect(&[
                    Value::Number(n.x as f64),
                    Value::Number(n.y as f64),
                    Value::Number(n.cw as f64),
                    Value::Number(n.ch as f64),
                    Value::Number(n.fillc as f64),
                    Value::Number(1.0),
                ]);
            }
            if n.borderc >= 0 {
                ui_rect(&[
                    Value::Number(n.x as f64),
                    Value::Number(n.y as f64),
                    Value::Number(n.cw as f64),
                    Value::Number(n.ch as f64),
                    Value::Number(n.borderc as f64),
                ]);
            }
        }
        _ => {}
    }
}

pub fn ui_paint_all_typed() {
    // Snapshot what the loop needs so the pool borrow is not held across the drawing calls (which
    // re-enter the context and, for text, the font cache).
    let jobs: alloc::vec::Vec<(ui_layout::Node, alloc::string::String)> = lay_with(|p| {
        (0..p.count)
            .map(|i| {
                let n = p.nodes[i];
                let t = if n.paint_kind == 1 {
                    p.text.get(i).cloned().unwrap_or_default()
                } else {
                    alloc::string::String::new()
                };
                (n, t)
            })
            .collect()
    });
    for (n, text) in jobs {
        if n.hide > 0 {
            continue;
        }
        if n.sel != 0 {
            ui_rect(&[
                Value::Number((n.x - 1) as f64),
                Value::Number((n.y - 1) as f64),
                Value::Number((n.cw + 2) as f64),
                Value::Number((n.ch + 2) as f64),
                Value::Number(n.col as f64),
            ]);
        }
        match n.paint_kind {
            1 => {
                let maxw = if n.use_w > 0 { n.cw } else { 512 };
                let align = match n.align {
                    1 => "center",
                    2 => "right",
                    _ => "left",
                };
                let mut args = alloc::vec![
                    Value::Number(n.font as f64),
                    Value::Number(n.x as f64),
                    Value::Number(n.y as f64),
                    Value::String(text.as_str().into()),
                    Value::Number(n.col as f64),
                    Value::Number(maxw as f64),
                    Value::String(align.into()),
                ];
                if n.shadowc >= 0 {
                    args.push(Value::Number(n.shadowc as f64));
                    args.push(Value::Number(n.shadow_off as f64));
                    args.push(Value::Number(n.shadow_off as f64));
                }
                ui_text(&args);
            }
            0 => {
                if n.fillc >= 0 {
                    ui_rect(&[
                        Value::Number(n.x as f64),
                        Value::Number(n.y as f64),
                        Value::Number(n.cw as f64),
                        Value::Number(n.ch as f64),
                        Value::Number(n.fillc as f64),
                        Value::Number(1.0),
                    ]);
                }
                if n.borderc >= 0 {
                    ui_rect(&[
                        Value::Number(n.x as f64),
                        Value::Number(n.y as f64),
                        Value::Number(n.cw as f64),
                        Value::Number(n.ch as f64),
                        Value::Number(n.borderc as f64),
                    ]);
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn lay_set_paint_typed(
    i: i32,
    paint_kind: i32,
    col: i32,
    shadowc: i32,
    fillc: i32,
    borderc: i32,
    font: i32,
    align: i32,
    use_w: i32,
    shadow_off: i32,
    sel: i32,
) {
    lay_with(|p| {
        p.set_paint(
            i, paint_kind, col, shadowc, fillc, borderc, font, align, use_w, shadow_off, sel,
        )
    });
}

pub fn lay_set_paint(args: &[Value]) -> Value {
    lay_set_paint_typed(
        argi(args, 0),
        argi(args, 1),
        argi(args, 2),
        argi(args, 3),
        argi(args, 4),
        argi(args, 5),
        argi(args, 6),
        argi(args, 7),
        argi(args, 8),
        argi(args, 9),
        argi(args, 10),
    );
    Value::Null
}

pub fn lay_set_text(args: &[Value]) -> Value {
    let i = argi(args, 0);
    let s = args
        .get(1)
        .map(|v| v.to_display_string())
        .unwrap_or_default();
    lay_with(|p| p.set_text(i, &s));
    Value::Null
}
