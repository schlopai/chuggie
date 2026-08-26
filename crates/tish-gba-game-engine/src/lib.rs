//! `tish_gba_game_engine` — the RPG-Maker-class game engine for tish-on-GBA.
//!
//! Shape (Unity analogy): this Rust crate is the *engine* (written once, hot path,
//! agb-coupled); a tish game is the *components + game logic* (what users write and
//! modify). It's imported like any binding — `import { … } from 'cargo:tish_gba_game_engine'`.
//!
//! This first cut is the foundation everything else hangs off: a **SoA entity
//! store** (component columns + a presence bitmask + generational ids) and a **fixed
//! per-frame pipeline** (`world_step` = movement → render → commit). Positions are
//! `fixed` (agb `Num<i32,8>`), so integration is native integer math. Sprites are
//! tish-agb handles: the engine *drives* tish-agb (it doesn't own rendering), so the
//! low level stays reachable and handles are shared.
#![no_std]
// Engine natives take their full argument lists deliberately — the tish ABI passes scalars, not
// config structs, so arity mirrors the script-side signature.
#![allow(clippy::too_many_arguments)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

// Q24.8 fixed-point (`Fixed` = agb `Num<i32,8>`) — the engine's position/velocity type,
// matching tish `fixed`. Sourced from the runtime facade (the canonical `Fixed` definition,
// CONTRACT §5) rather than depending on `agb` directly: the engine drives tish-agb + the facade
// and needs no direct agb dependency of its own.
use tishlang_runtime_gba::{get_prop, set_prop, value_call, Fixed, SingleCore, Value};

fn to_fixed(n: f64) -> Fixed {
    Fixed::from_raw((n * 256.0) as i32)
}
fn from_fixed(f: Fixed) -> f64 {
    f.to_raw() as f64 / 256.0
}

// ── Components (fixnum POD, one dense column each) ───────────────────────────
// The presence mask is a u16, so up to 16 component types.
const C_TRANSFORM: u32 = 1 << 0;
const C_BODY: u32 = 1 << 1;
const C_SPRITE: u32 = 1 << 2;
const C_COLLIDER: u32 = 1 << 3;
const C_GRIDPOS: u32 = 1 << 4;
const C_ANIM: u32 = 1 << 5;
const C_WALK: u32 = 1 << 6;
const C_PLATFORMER: u32 = 1 << 7;
const C_HEALTH: u32 = 1 << 8;
const C_PATROL: u32 = 1 << 9;
/// Auto-despawn: a time-to-live and/or "gone when it leaves the screen" flag. The lifeblood of a
/// shoot-'em-up (bullets, spent shots, explosions) and of any spawn-and-forget FX — without it a
/// static-screen game (no camera to cull against) would leak a sprite per shot until OAM overflows.
const C_LIFE: u32 = 1 << 10;
/// Hurt box: this entity deals damage on contact to entities carrying a target tag (a bullet to
/// enemies, an enemy body to the player). The `combat_system` resolves it natively — no per-bullet
/// tish callback and no `makeEntity` wrapper — so a screen full of shots stays cheap.
const C_HURT: u32 = 1 << 11;
/// Native movement pattern (straight / sideways-weave). The `mover_system` drives the entity's
/// `Body` velocity in pure Rust every frame, so a screen full of weaving enemies costs NO per-frame
/// tish `tick` — the shmup analogue of `patrol_system`. Firing/AI decisions stay in tish; only the
/// (cheap, formulaic) MOVEMENT is native.
const C_MOVER: u32 = 1 << 12;
/// Free 8-directional top-down movement with solid-tile collision (the top-down action-RPG genre).
/// Unlike `GridPos` (tile-locked, grid-RPG-style) the entity moves by pixels in any of eight
/// directions and its `Collider` box is resolved axis-by-axis against the solid grid — the
/// platformer's collision, minus gravity. `topdown_system` owns integration; drive it with
/// `topdown_move` each frame. Both the player and chase-AI enemies use it, so they slide along
/// walls instead of snapping to tiles.
const C_TOPDOWN: u32 = 1 << 13;
/// Native top-down "seek the player" AI (component `C_CHASE`). `chase_system` steers a `C_TOPDOWN`
/// entity toward the camera target each frame and picks its walk frame — ALL in Rust, so a room of
/// enemies costs ZERO per-frame tish `tick`s (the shmup lesson: keep movement/animation native, only
/// decisions in tish). This is the top-down counterpart to `patrol_system`/`mover_system`.
const C_CHASE: u32 = 1 << 14;
/// Solid entity blocker: its `Collider` stops top-down movers the way a solid tile does (NPCs,
/// closed gates, pushable crates). The box moves with the entity — no spawn-tile `setSolid` hack —
/// so a walked-away NPC leaves no invisible wall behind and blocks wherever it stands now.
const C_BLOCKER: u32 = 1 << 15;
const C_HOPPER: u32 = 1 << 16;
const C_JUMPER: u32 = 1 << 17;
/// A rank-and-file enemy that SHOOTS (`shooter_system`). The two most common enemies in a top-down
/// action game of this kind — projectile-spitting walkers and spear-throwers — are ranged, and modelling them as plain
/// hoppers makes them melee, which changes how most of the map plays. Native, so a screen full of
/// them still costs no per-frame tish call: the bullet style in force when `set_shooter` is called
/// is snapshotted into the component and restored around each shot.
const C_SHOOTER: u32 = 1 << 18;
/// An enemy that bolts along an axis once it lines up with the player (`charger_system`) — the
/// classic striking snake. It rides on top of whatever normally moves the entity, taking over only while lined up.
const C_CHARGER: u32 = 1 << 19;
/// The entity blocks damage arriving from the direction it FACES. In the genre this is one rule wearing
/// two hats: the player's shield stops arrows and fireballs they are walking into, and an armoured knight
/// stops the sword unless you get behind it. Bit 1 guards melee/contact, bit 2 guards projectiles,
/// so a shield and a suit of armour are the same component with a different mask.
const C_GUARD: u32 = 1 << 20;

/// Directional walk animation on a SHARED sprite strip (`diranim_system`).
///
/// The older directional path is `set_chase`'s, and it assumes the entity owns its sheet: it plays
/// `facing * stride` with no per-actor origin, so on a strip that holds thirty-three actors every
/// chasing enemy animated whatever sits at frame 0. It also assumes five columns per row (idle at
/// the base, four walk frames after it).
///
/// This is the layout an NES-era baked actor strip actually has: `base + facing * stride` for
/// `frames` frames, no idle column, with `base` naming where THIS actor starts.
const C_DIRANIM: u32 = 1 << 21;

/// The entity's `Collider` is a DISC of diameter `Collider.w`, centred in the (square) box.
///
/// Deliberately a TAG on the existing box rather than a new column: every AABB path in the engine —
/// `slots_overlap`, `combat_system`, `collect_collisions`, off-screen culling, `box_hits_solid` —
/// keeps working unchanged and treats the disc as its bounding box, which is correct as a
/// broadphase and perfectly adequate for a goal trigger or a pickup. Only `dynamic_system` reads
/// this bit and does the exact circle test. Radius is `w >> 1`, a shift.
const C_CIRCLE: u32 = 1 << 22;
/// A rigid body that integrates, bounces off tiles, resolves against other discs and comes to REST
/// (`dynamic_system`). Golf's ball, soccer's ball and players, a pinball, a pool table.
const C_DYNAMIC: u32 = 1 << 23;
/// On overlap with a tagged target, briefly stun it (`grabber_system`) — the classic shield-eater / ceiling-hand grab, lite.
const C_GRABBER: u32 = 1 << 24;
/// Inert until the player shares its row/col, then dash (`trap_system`) — Blade Trap-lite.
const C_TRAP: u32 = 1 << 25;
/// Keep this entity glued to a parent transform (`follow_system`): part / train segment / orbiter.
const C_FOLLOW: u32 = 1 << 26;
/// Projectile that reverses toward its owner after N frames (`boomerang_system`).
const C_BOOMERANG: u32 = 1 << 27;
/// Walk down a shared flow field toward its goal (`seek_system`) — the RTS "move here" order.
const C_SEEK: u32 = 1 << 28;
/// Attack-move: seek while nothing is in range, break to engage, resume (`soldier_system`).
const C_SOLDIER: u32 = 1 << 29;
/// Reveals fog of war around itself each frame (`fog_system`).
const C_VISION: u32 = 1 << 30;
/// DORMANT. A pooled slot that has been created but is not in play: the per-frame systems skip it
/// entirely, so a pool costs ROM and VRAM but not frame time.
///
/// This is the fix for "a pooled entity is not free" — 26 parked entities measured 2,631 ticks of a
/// 4,389-tick frame doing nothing at all, which forces a game to choose between a big enough unit
/// pool and a playable frame rate. It is NOT off-screen culling: an off-screen unit must keep
/// walking, a sleeping one has nothing to do by definition.
const C_SLEEP: u32 = 1 << 31;

/// `guard` mask bits.
pub const GUARD_MELEE: i32 = 1;
pub const GUARD_SHOT: i32 = 2;

// Weapon kinds, mirroring the NES original's per-weapon damage-type enum.
// A hurt box carries ONE of these; a victim carries a MASK of the ones that bounce off it, which is
// how the genre says "this armoured boss ignores the sword" and "only an arrow opens that boss's eye"
// without a per-monster special case in the collision code.
pub const DMG_SWORD: i32 = 1;
pub const DMG_BOOMERANG: i32 = 2;
pub const DMG_ARROW: i32 = 4;
pub const DMG_BOMB: i32 = 8;
pub const DMG_MAGIC: i32 = 0x10;
pub const DMG_FIRE: i32 = 0x20;

/// Tile size in pixels for the grid/RPG genre (grid-RPG overworld).
const TILE: i32 = 16;
/// Off-screen culling margin (px). An entity whose box is farther than this outside the camera
/// view is "inactive": its behaviour/physics/animation are skipped until it scrolls back near.
const CULL_MARGIN: i32 = 32;
/// Grid walk speed in pixels/frame (`TILE`/`GRID_SPEED` frames per tile step).
const GRID_SPEED: i32 = 2;

// Side-scrolling platformer tuning, in fixed-point raw units (pixels/frame · 256).
const P_WALK: i32 = 320; // 1.25 px/frame walk speed
const P_RUN: i32 = 576; // 2.25 px/frame run speed (hold the run button)
const P_GRAVITY: i32 = 77; // ~0.3 px/frame² downward acceleration
const P_JUMP: i32 = 1280; // 5 px/frame initial jump velocity (≈2.5-tile apex)
const P_TERMINAL: i32 = 1536; // 6 px/frame terminal fall speed
                              // "Game feel" windows, in frames.
const P_COYOTE: i32 = 6; // can still jump this many frames after walking off a ledge
const P_JUMP_BUFFER: i32 = 6; // a jump press within this window before landing still fires
const P_DROP: i32 = 8; // frames a down-drop lets the box fall through one-way platforms
                       // Vertical slack on `platformer_interact`'s probe, in px. Side-scroller NPCs stand on steps,
                       // doorsills and awnings, and their art rarely shares the player's hitbox height — without slack the
                       // talk only lands when the two boxes happen to line up, which reads as the button not working.
const PF_INTERACT_PAD: i32 = 8;
const INVULN_FRAMES: i32 = 40; // post-hit invincibility (~0.67s); sprite flickers during it

// Top-down (action-RPG) tuning, in fixed-point raw units (px/frame · 256).
const TD_WALK: i32 = 320; // 1.25 px/frame default top-down move speed
const TD_DIAG: i32 = 181; // Q8 scale (~0.707) applied per axis on a diagonal so it isn't faster
                          // The hit reaction, matching the NES original's behaviour: the shove steps 1px FOUR times per
                          // frame with a collision test on each step, so a shove is 4 px/frame; the distance decides how
                          // long it lasts. The player is shoved 0x20 = 32 px and a monster 0x40 = 64 px — so 8 frames and
                          // 16 frames respectively.
                          //
                          // This was 2.5 px/frame for 8 frames = 20 px, symmetric for both. Wrong in three ways at once: too
                          // slow, too short, and identical for the player and the thing that hit him.
const TD_KNOCK: i32 = 1024; // 4 px/frame — the original moves 1px x4 per frame
const TD_KNOCK_FRAMES: i32 = 8; // 8 frames x 4 px = the player's 0x20 shove

// Top-down snap profiles (`topdown_snap`). These are whole character-controller personalities, not
// flags that combine: each one owns how intent becomes motion, so a game picks exactly one per
// entity. 0 (the default) is free 8-direction movement — no snapping at all.
/// Tile stepping: a direction commits the entity to one full 16px cell, and it rests centred on the
/// cell it lands on. Board-game / grid-RPG feel. Input during the step is ignored until it finishes.
const TD_SNAP_TILE: u8 = 2;
#[derive(Clone, Copy, Default)]
struct Transform {
    x: Fixed,
    y: Fixed,
}
#[derive(Clone, Copy, Default)]
struct Body {
    vx: Fixed,
    vy: Fixed,
}
#[derive(Clone, Copy)]
struct SpriteRef {
    handle: i32,
    /// Draw offset (px) of the sprite's top-left from the entity's transform. Lets a sprite be
    /// larger than its collider — e.g. a 32×32 character on a 16×16 hitbox draws at (-8, -16) so it
    /// centres horizontally on the box with its feet on the box's bottom edge. Default (0, 0).
    pub ox: i32,
    pub oy: i32,
}
/// An axis-aligned box `w`×`h` with its top-left at the entity's transform (matching
/// agb's top-left sprite positioning).
#[derive(Clone, Copy, Default)]
struct Collider {
    w: Fixed,
    h: Fixed,
}
/// The shared configuration a burst of bullets is spawned with. Set once (`set_bullet_style`) before a
/// pattern call, so `fire_ring`/`fire_spread`/etc. can spawn every bullet natively without the game
/// re-resolving an options object per bullet. `size` is the square hitbox in px; `target` is the tag
/// the bullet damages; `tag` is the bullet's own kind tag; `ttl` is its lifetime in frames.
#[derive(Clone, Copy, Default)]
struct BulletStyle {
    sheet: i32,
    frame: i32,
    size: i32,
    damage: i32,
    target: i32,
    tag: i32,
    ttl: i32,
    /// Which weapon the bullets of this burst are, as a `DMG_*` bit. Sticky like every other field
    /// here: set immediately before firing, never once at boot. 0 = untyped, which is what every
    /// existing caller gets, so `set_bullet_style`'s seven arguments stay exactly as they were.
    damage_type: i32,
}
/// Grid/RPG position: which tile the entity occupies, its facing, and (while stepping)
/// the target pixel position it's sliding toward. The grid system drives `Transform`
/// from this; entities move a whole tile at a time (tile-locked), blocked by solids.
#[derive(Clone, Copy, Default)]
struct GridPos {
    col: i32,
    row: i32,
    moving: bool,
    tx: Fixed,
    ty: Fixed,
    /// Facing direction (last attempted step), for the interact probe.
    fx: i32,
    fy: i32,
}
/// Sprite-sheet animation: cycle `frames` frames, advancing every `speed` game frames.
/// A playing animation **clip**: the frame range `[from, from+len)` advanced one step
/// every `speed` frames, looping (or stopping on the last frame when `looping` is false).
/// `cur` is the offset within the range. This is the low-level primitive a tish-side
/// animation controller drives via `anim_play` (bind a state → a clip).
#[derive(Clone, Copy, Default)]
struct Anim {
    from: i32,
    len: i32,
    speed: i32,
    timer: i32,
    cur: i32,
    looping: bool,
    playing: bool,
}

/// Directional walk animation driven by `GridPos` facing + movement. Expects a
/// character sheet laid out as ROWS of `cols` frames: row 0 = facing down, row 1 = up,
/// row 2 = side (used for left as-is, and for right horizontally flipped). Within a row,
/// column 1 is the standing frame and columns 0 / 2 are the two walking steps. `speed`
/// is frames-per-step-toggle; `phase` flips between the two step columns while moving.
#[derive(Clone, Copy, Default)]
struct Walk {
    cols: i32,
    speed: i32,
    timer: i32,
    phase: bool,
}

/// The tile column/row containing a box's FAR EDGE, given that edge's coordinate.
///
/// A box spanning `[a, a+w)` ends at the pixel just before `a+w`, so the cell it reaches is the one
/// holding `a+w - 1/256`, not the one holding `a+w`. This must agree EXACTLY with how
/// `box_hits_solid` picks its last cell (`((v).to_raw() - 1) >> 8`), because one decides that a move
/// is blocked and the other decides where to put the box back.
///
/// Getting it wrong is not a rounding error, it is a teleport. The old form —
/// `(v.floor() - 1) / TILE` — differs from this only when the far edge lands in the first PIXEL of a
/// cell (say a box bottom at 320.3 against a floor at 320): `box_hits_solid` sees the box in row 20
/// and blocks it, while `floor(320.3) - 1 = 319` names row 19, and the box is clamped a whole tile
/// back the way it came. A character standing still on a floor penetrates by exactly that much on
/// its first gravity step, so it was flung a tile upward, fell, was flung again — grounded flickering
/// on and off forever, an animation that never settled, and a hero hovering a tile above the street.
fn last_cell(edge: Fixed) -> i32 {
    ((edge.to_raw() - 1) >> 8).div_euclid(TILE)
}

/// Side-scrolling platformer body. Unlike `Body`, the plain movement system does NOT touch
/// it — `platformer_system` owns integration so it can apply gravity and resolve the entity's
/// `Collider` box against the solid tile grid axis-by-axis. `grounded` is true the frame the
/// box is resting on a solid (gates jumping and drives animation). Carries the "game feel"
/// state (coyote time, jump buffer, variable-height jump, one-way drop-through).
#[derive(Clone, Copy, Default)]
struct Platformer {
    vx: Fixed,
    vy: Fixed,
    grounded: bool,
    dir: i32, // horizontal move intent this frame (-1/0/1), set by `platformer_walk`
    // Which way the entity is FACING (-1 left, +1 right). `dir` is the intent this frame and goes
    // to 0 the moment input stops, so it cannot answer "which way am I looking?" — every platformer
    // example ended up shadowing it with its own `data.face`. This keeps the last non-zero `dir`,
    // which is what animation (sprite flip) and `platformer_interact` both need. Starts facing
    // right so a freshly spawned, never-moved entity still has a defined interact direction.
    face: i32,
    run: bool,        // run speed instead of walk (set by `platformer_run`)
    coyote: i32,      // frames of coyote time left (jump shortly after leaving ground)
    jump_buffer: i32, // frames a buffered jump press stays live
    jumping: bool,    // rising from a jump — lets `platformer_jump_release` cut the height
    drop: i32,        // frames left to fall through one-way platforms (down-drop)
    blocked: bool, // pushed into a wall this frame (a wall stopped the intended move) — patrol AI
    held: bool,    // frozen in place (no gravity/movement) — e.g. hanging on a ledge grab
    // Per-entity ground speeds in Fixed raw (1/256 px/frame), or 0 to take the engine's P_WALK /
    // P_RUN. Zero-means-default rather than seeding every body with the constants, so a `Platformer`
    // still has a meaningful `Default` and the six games that never ask are bit-for-bit unaffected.
    // A hero should be able to be quicker than the shared default without retuning every other game.
    walk_raw: i32,
    run_raw: i32,
    // Per-entity jump impulse / gravity in Fixed raw, 0 = the engine's P_JUMP / P_GRAVITY — the
    // same zero-means-default contract as the speeds above. Set by `platformer_set_physics`: a
    // hero carrying something heavy jumps lower; carrying something buoyant falls slower.
    jump_raw: i32,
    grav_raw: i32,
    // One-shot persistent horizontal velocity in Fixed raw (a throw arc). While non-zero it
    // REPLACES the dir*speed walk velocity, and it clears itself on grounding or on hitting a
    // wall — a plain platformer body has no persistent vx (it is recomputed from `dir` every
    // frame), so a thrown entity needs this to fly. Set by `platformer_launch`.
    launch_raw: i32,
    // Standing on a carrier (`set_carrier`) this frame. `riding` gates `carrier` — an encoded
    // entity id, which may legitimately be 0, so the bool is the validity bit, not the id.
    riding: bool,
    carrier: i32,
}

/// Hit points with post-hit invincibility. `damage` ignores hits while `invuln > 0` and, on a
/// real hit, starts the i-frame window (the render flickers the sprite during it). At `hp <= 0`
/// the health system fires the entity's `onDeath` hook, or despawns it if none is defined.
#[derive(Clone, Copy, Default)]
struct Health {
    hp: i32,
    max: i32,
    invuln: i32,
    /// How many i-frames a hit grants (0 = none). The player wants a mercy window after being hit;
    /// a shmup enemy wants 0 so every bullet in a stream chips it. Defaults to `INVULN_FRAMES`.
    invuln_max: i32,
    dead: bool,
}

/// Native patrol AI for a platformer entity: walk in `dir`, reversing at walls and ledges. Runs
/// entirely in Rust (`patrol_system`) so a screen full of enemies costs no per-frame tish calls —
/// only their `onCollide` (which fires on contact). Needs `Platformer` (for movement + collision).
#[derive(Clone, Copy)]
struct Patrol {
    dir: i32,
    /// How the sprite should mirror to match `dir`. 0 = leave the sprite alone, 1 = hflip when
    /// walking RIGHT (art drawn facing left), 2 = hflip when walking LEFT (art drawn facing right).
    /// Without this a game has to add a per-frame tish `tick` back purely to call `setFlip`, which
    /// costs far more than the patrol it was meant to replace — see `patrol_system`.
    flip_mode: i32,
    /// Last direction the flip was issued for, so the sprite is only told when it changes.
    flipped_for: i32,
}
impl Default for Patrol {
    fn default() -> Self {
        Patrol {
            dir: -1,
            flip_mode: 0,
            flipped_for: 0,
        }
    }
}

/// Auto-despawn timer + off-screen flag (component `C_LIFE`). `ttl > 0` counts down each frame and
/// despawns the entity at 0 (spent bullets, a finished explosion). `offscreen` despawns it the moment
/// its box leaves the visible area (bullets that fly off the top; enemies that drift off the bottom).
/// The two combine — a bullet typically has both (a TTL backstop and off-screen cleanup).
#[derive(Clone, Copy, Default)]
struct Life {
    ttl: i32,
    offscreen: bool,
}

/// A rigid disc (component `C_DYNAMIC`). Golf and soccer are the same physics with different
/// tuning, which is why there is one of these and not two genre packages.
///
/// ⚠️ CONTACT RANK, NOT MASS, and that is a hardware decision. The textbook impulse split is
/// `m2/(m1+m2)` — a DIVISION per contact per iteration, and this chip has no divide instruction
/// (`docs/perf-rules.md` §3: one `% RING` on a hot path cost 1,400 of a frame's 4,389 ticks). Golf
/// and soccer need exactly two relationships: *equal* (agent vs agent, ball vs ball), where each
/// takes half the correction with a `>> 1`; and *one side wins* (a wall, a goalpost, an agent vs
/// the ball), where the lower rank takes all of it. 255 is immovable. That is zero divisions in
/// contact resolution, and it is a real limitation: there is no way to say "this crate is three
/// times heavier than that one".
#[derive(Clone, Copy, Default)]
struct Dynamic {
    /// Bounce, Q8 (256 = elastic, 0 = dead stop). Applied to the NORMAL component only.
    restitution: i32,
    /// Per-frame velocity retention, Q8. One multiply and a shift — never a divide.
    friction: i32,
    /// Speed SQUARED (raw Q8, `>> 8`) below which the body parks. Squared so the rest test needs
    /// no sqrt and no divide.
    rest_v2: i32,
    rank: u8,
    asleep: u8,
    /// Who last pushed this body. Soccer's last-toucher — own goals and assists — with no
    /// per-contact tish callback.
    last_hit: i32,
}

/// What a surface class DOES: a constant acceleration in px/frame^2 (a slope, wind, a conveyor)
/// plus a Q8 per-frame velocity retention (240 ~ green, 200 ~ rough, 150 ~ sand, 256 = ice).
///
/// This is `kart.rs`'s `top_speed_for` surface table generalised, so a slope, a sand trap and a
/// boost pad are ONE mechanism rather than three special cases.
#[derive(Clone, Copy, Default)]
struct SurfaceDef {
    ax: Fixed,
    ay: Fixed,
    friction: i32,
}

impl SurfaceDef {
    /// `Default::default()` is not callable in a const context, and the table lives in a `const`
    /// initialiser, so class 0 — "plain ground, no acceleration, use the body's own friction" —
    /// is spelled out here instead.
    const fn flat() -> Self {
        SurfaceDef {
            ax: Fixed::from_raw(0),
            ay: Fixed::from_raw(0),
            friction: 0,
        }
    }
}

/// A fixed set of entities created ONCE and re-armed forever after — the shape six examples in this
/// repo hand-rolled identically before it lived here (the topdown RPG port's cast, sub-weapons and
/// floor drops; its pool and cast spikes; `sunny-land`'s FX). A spawn costs ~1,400 ticks on the frame it
/// happens and reallocates sprite VRAM (`docs/perf-rules.md` §6); a re-arm is a few dozen stores.
///
/// ⚠️ `kind` IS the live flag. Three of the six hand-rolled versions had already collapsed the two
/// into one column because a pooled slot's payload is never meaningful while it is free — so -1 is
/// free and anything >= 0 is live and carries whatever the caller stored there.
///
/// There is deliberately no ttl column: retirement runs through `life_system`, which already owns
/// four rules a pooled projectile wants (ttl, off-screen, hurt box hits a solid tile, room cutoff).
/// A second countdown here would be a second answer to the same question.
struct Pool {
    ent: Vec<i32>,
    /// tish-agb sprite handle per slot, or -1 for an entity-only pool.
    spr: Vec<i32>,
    kind: Vec<i32>,
    /// The sprite offset, held on the POOL rather than re-applied per arm: it is a property of the
    /// sheet, not of the shot, and every hand-rolled version repeated `set_sprite_offset(e, -8, -8)`
    /// at each arm site only because `reset_entity` rebuilds `SpriteRef` with `ox`/`oy` at 0.
    ox: i32,
    oy: i32,
    live: i32,
    /// High-water live count. The pool spike's verify.sh already asserted this and computed it by
    /// scanning the whole pool on every arm.
    high: i32,
}

/// A contact hurt box (component `C_HURT`). On overlapping an entity whose `tag == target_tag` that
/// has `Health`, it deals `damage`; a bullet (`despawn_on_hit`) is then consumed, while a body-contact
/// hazard (an enemy ramming the player) stays and relies on the victim's i-frames to rate-limit. The
/// `combat_system` applies this in pure Rust every frame, so bullet-hell density costs no tish calls.
#[derive(Clone, Copy, Default)]
struct Hurt {
    damage: i32,
    target_tag: i32,
    despawn_on_hit: bool,
    /// Frames to stun the victim for on a landed hit. The classic boomerang is the reason this exists:
    /// it is not primarily a damage weapon, it is a *stopper* — it freezes what it touches for a
    /// moment and only outright kills the one-hit enemies.
    stun: i32,
    /// Which WEAPON this box is, as a `DMG_*` bit, or 0 for "untyped — lands on everything".
    /// Untyped is the default so every hurt box that predates this keeps its behaviour exactly.
    /// Paired with `World::immune` on the victim: a hit is discarded when `immune & damage_type`.
    damage_type: i32,
}

/// Native movement pattern (component `C_MOVER`). `mover_system` drives `Body` velocity each frame in
/// pure Rust: `pattern` 0 = straight (down at `base_vy`), 1 = weave (a sideways triangle wave of
/// amplitude `amp` over `period` frames while descending at `base_vy`). `t` is the internal phase
/// counter. This is what keeps a screen full of weaving enemies at zero per-frame tish cost — the
/// shmup counterpart to `patrol_system`; only firing/AI decisions stay in tish `tick`s.
#[derive(Clone, Copy, Default)]
struct Mover {
    pattern: u8,
    t: i32,
    base_vy: Fixed,
    amp: Fixed,
    period: i32,
}

/// Free top-down movement (component `C_TOPDOWN`). Each frame `topdown_move` sets the move intent
/// (`dx`/`dy` ∈ {-1,0,1}); `topdown_system` scales it by `speed` (× `TD_DIAG` per axis on a diagonal),
/// resolves the `Collider` box against solids axis-by-axis (no gravity), and clears the intent so a
/// frame with no input stops (arcade feel). `facing` persists while idle (0 down/1 up/2 left/3 right)
/// so a swing/animation knows which way the entity looks. `knock*`/`knock` carry a brief hit-shove
/// that overrides input (action-RPG hit-shove knockback), decaying to nothing over `TD_KNOCK_FRAMES`.
#[derive(Clone, Copy, Default)]
struct TopDown {
    dx: i32,
    dy: i32,
    facing: i32,
    moving: bool,
    speed: i32, // raw fixed px/frame
    kx: Fixed,
    ky: Fixed,
    knock: i32,
    snap_mode: u8,
    snap_dx: i32,
    snap_dy: i32,
    snap_target_x: Fixed,
    snap_target_y: Fixed,
}

/// Native chase AI: seek the camera target within `aggro` (manhattan px). `stride` > 0 = a directional
/// Directional walk clip on a shared strip: `base + facing * stride`, `frames` long.
#[derive(Clone, Copy, Default)]
struct DirAnim {
    base: i32,
    stride: i32,
    frames: i32,
    speed: i32,
}

/// sheet (row = facing, `stride` cols/row: idle at base, walk at base+1..4), animated at `anim_speed`;
/// `stride` == 0 = a non-directional creature (e.g. a bat) looped over frames `0..flap` (facing frozen).
#[derive(Clone, Copy, Default)]
struct Chase {
    aggro: i32,
    stride: i32,
    flap: i32,
    anim_speed: i32,
}

/// RTS "move to a destination". The destination itself lives in a shared `FlowField`, so N units
/// ordered to the same place cost ONE breadth-first search between them rather than N path queries:
/// this component only remembers which field to read and how fast to walk.
#[derive(Clone, Copy, Default)]
struct Seek {
    field: i32,
    /// Manhattan px from the field's goal at which the unit considers itself arrived and stops.
    /// Without it a crowd converging on one cell grinds against each other forever.
    arrive: i32,
    stride: i32,
    anim_speed: i32,
    done: bool,
}

/// Attack-move. `range`/`dmg`/`cooldown` are the unit's own weapon; `team` is what makes another
/// soldier an enemy — deliberately NOT the `tag`, because a game wants tags free for its own kinds
/// and an RTS re-uses one unit kind across both sides.
#[derive(Clone, Copy, Default)]
struct Soldier {
    team: i32,
    range: i32,
    dmg: i32,
    cooldown: i32,
    timer: i32,
    /// Entity id currently being engaged, or -1. Held across frames so a unit does not re-acquire
    /// (and re-scan every other unit) every single frame.
    target: i32,
    recheck: i32,
}

/// Fog-of-war emitter: reveals a disc of `radius` cells around itself every frame.
#[derive(Clone, Copy, Default)]
struct Vision {
    radius: i32,
    /// Last cell this entity was seen in, so `fog_system` can skip the whole pass on a frame where
    /// nobody crossed a cell boundary.
    last_col: i32,
    last_row: i32,
}

/// How many flow fields can exist at once. Not a hardware limit — a budget: a field is 2 bytes per
/// map cell, and an RTS wants one per STANDING ORDER, not one per unit.
///
/// Six, because a harvest run needs TWO: one to the resource and one back to the drop-off. Sharing
/// a single field between the two legs means an outbound worker re-goals the field under a
/// returning one and the whole crew follows whoever moved last — measured as an economy that
/// simply never accrued anything.
const MAX_FLOWS: usize = 6;

const FOG_UNSEEN: u8 = 0;
const FOG_EXPLORED: u8 = 1;
const FOG_VISIBLE: u8 = 2;

/// A breadth-first distance field over the collision grid, measured in steps FROM the goal. Every
/// unit ordered to the same place shares one of these; walking is then "step to whichever neighbour
/// has a smaller number", which is two array reads per axis with no call, divide or float.
struct FlowField {
    cols: i32,
    rows: i32,
    goal_col: i32,
    goal_row: i32,
    /// `u16::MAX` = unreachable. Sized to the grid on first use and reused thereafter.
    dist: Vec<u16>,
    /// BFS frontier, kept as a field so a rebuild allocates nothing.
    queue: Vec<i32>,
    ready: bool,
}

impl FlowField {
    const fn new() -> Self {
        FlowField {
            cols: 0,
            rows: 0,
            goal_col: -1,
            goal_row: -1,
            dist: Vec::new(),
            queue: Vec::new(),
            ready: false,
        }
    }
}

/// Fog of war: one byte per map cell of `FOG_*`, plus what was last painted to the shroud layer so
/// a blit only writes what changed.
struct Fog {
    cols: i32,
    rows: i32,
    state: Vec<u8>,
    /// Per BG cell (256 of them): the fog state currently painted there.
    shown: Vec<u8>,
    /// Per BG cell: which MAP cell it is currently showing, or -1. The shroud layer wraps every 16
    /// cells, so a BG cell is reused by a different map cell as the camera scrolls — without this
    /// the layer would keep stale shroud from wherever the camera used to be.
    win: Vec<i32>,
    on: bool,
}

impl Fog {
    const fn new() -> Self {
        Fog {
            cols: 0,
            rows: 0,
            state: Vec::new(),
            shown: Vec::new(),
            win: Vec::new(),
            on: false,
        }
    }
}

#[derive(Clone, Copy, Default)]
/// A continuously-walking, tile-aligned wanderer.
///
/// The behaviour `set_hopper` could not express, and the reason enemy movement was wrong. A hopper
/// idles ~30 frames, picks a RANDOM cardinal direction and lurches a whole 16 px. A wanderer walks
/// without stopping and may only change direction when it is standing exactly on a tile boundary,
/// and its turns are never random — it always turns onto the PERPENDICULAR axis, toward the target
/// (the NES original's target-player / turn-if-time / turn-on-axis walker rules).
///
/// Generic on purpose: nothing here knows about any one game. `turn_rate` and the walker's speed are the
/// caller's data.
struct Wanderer {
    /// 0..255. ⚠️ HIGHER MEANS MORE AGGRESSIVE, which reads backwards until you follow it through:
    /// the roll is `rand(256) > turn_rate -> drift`, so a big `turn_rate` makes the drift branch
    /// rare and the "line up on the target" branch common.
    turn_rate: i32,
    /// Counts down; a turn is only allowed at zero, so this is what spaces turns out in time.
    turn_timer: i32,
    /// Set when the walker has just turned to face the target — i.e. it is lined up and would like
    /// to shoot. A shooting system consumes it; the walker itself never reads it.
    want_shoot: i32,
    /// Last RAW fixed-point position (`Fixed::to_raw`), used only to notice that the walker is
    /// BLOCKED. Raw, not floored: at 0.5 px/frame the floor only changes every other frame, so a
    /// floored comparison calls a perfectly healthy walker "blocked" half the time.
    last_x: i32,
    last_y: i32,
    /// The room this walker belongs to, captured when it was configured.
    ///
    /// ⚠️ NOT the camera's room. Confining to `room_cam.cur_rx/cur_ry` looked equivalent and was
    /// not: the camera follows the player, and when the player was knocked into the next room the
    /// clamp rect jumped with it and TELEPORTED every wanderer — measured as a -38.5 px step in one
    /// frame. A monster belongs to the room it was placed in, whatever the camera is showing.
    home_rx: i32,
    home_ry: i32,
}

#[derive(Clone, Copy, Default)]
struct Hopper {
    stride: i32,
    timer: i32,
    state: i32,
    start_x: Fixed,
    start_y: Fixed,
    // The hop's direction, re-asserted into topdown intent EVERY frame of the hop:
    // topdown_system consumes dx/dy each frame, so setting them only on the hop's
    // first frame moved the enemy exactly 1px per 16px hop. Every hopper walker in
    // every game was effectively stationary (caught by a basic-foes spike).
    dir_x: i32,
    dir_y: i32,
}

#[derive(Clone, Copy, Default)]
struct Shooter {
    interval: i32,
    timer: i32,
    speed: Fixed,
    /// false → fire along the entity's facing (a spitter shoots the way it walks); true → fire at
    /// wherever the player is standing.
    aimed: bool,
    style: BulletStyle,
}

#[derive(Clone, Copy, Default)]
struct Charger {
    /// Raw fixed px/frame while charging, and the entity's normal speed to restore afterwards.
    speed: i32,
    base: i32,
    /// px of slack that still counts as "lined up" on the other axis.
    band: i32,
    active: bool,
}

/// Stun-on-overlap grabber (`C_GRABBER`). Minimal: brief stun on the tagged target, no teleport.
#[derive(Clone, Copy, Default)]
struct Grabber {
    target_tag: i32,
}

/// Blade-trap dash (`C_TRAP`). Parks at `home_*` until the player shares a row/col within `band`,
/// then bolts like a charger. Restores idle (zero intent, parked speed) when the line breaks.
#[derive(Clone, Copy, Default)]
struct Trap {
    home_x: Fixed,
    home_y: Fixed,
    speed: i32,
    base: i32,
    band: i32,
    active: bool,
}

/// Follow / link modes for `C_FOLLOW`.
const FOLLOW_PART: u8 = 0;
const FOLLOW_TRAIN: u8 = 1;
const FOLLOW_ORBIT: u8 = 2;

/// Parent-linked transform follow (`C_FOLLOW`): a part/train segment keeps a fixed offset from its
/// parent; an orbiter circles at `radius` px (angle in 1/256ths of a turn).
#[derive(Clone, Copy, Default)]
struct Follow {
    kind: u8,
    parent: i32,
    radius: i32,
    ox: Fixed,
    oy: Fixed,
    angle: i32,
}

/// Boomerang return-mover (`C_BOOMERANG`): after `timer` frames, reverse `Body` velocity toward
/// `owner`. Pair with `set_lifetime` / `set_despawn_offscreen` like any other projectile.
#[derive(Clone, Copy, Default)]
struct Boomerang {
    timer: i32,
    owner: i32,
    returning: bool,
}

// ── Second component mask (`mask2`) ──────────────────────────────────────────────────────────────
// All 32 bits of `mask` are taken (C_SLEEP is bit 31), so the NES-era enemy/boss natives live on a
// parallel `mask2` column. Every bit here defaults to 0, so entities that never call the new
// natives behave exactly as before — the guards below are strictly additive.
/// Native AI state machine (`nai` column): ambusher / drifter / flicker-caster / bouncer.
const M2_NAI: u32 = 1 << 0;
/// Invulnerable right now (a drifter in flight, a caster mid-flicker). `damage()` refuses, but the
/// entity is still tangible — its contact hurt box still lands. Readable via `entity_phased`.
const M2_PHASED: u32 = 1 << 1;
/// Fully intangible AND invisible (an ambusher underground, a caster between appearances): no damage
/// taken, no contact damage dealt, sprite hidden. Implies the `M2_PHASED` damage refusal.
const M2_HIDDEN: u32 = 1 << 2;
/// Damage to this entity is re-routed to `zx.proxy` (boss neck → head, body segment → tail).
const M2_PROXY: u32 = 1 << 3;
/// Damage only lands while `zx.gate != 0` (an eye-open window, a boss's last-hit window).
const M2_GATE: u32 = 1 << 4;
/// On death, push `zx.code` to the world's death-note queue (part-death notification to the
/// parent's tish logic — read with `death_note()`).
const M2_NOTE: u32 = 1 << 5;
/// Rideable: this entity's top edge acts as one-way moving ground for platformer bodies
/// (`set_carrier`). A rider inherits the carrier's frame-to-frame motion while standing on it.
/// The carrier may itself be a platformer body (a walking beast — integrated in pass 1 of
/// `platformer_system`) or a Body/mover float (a raft — integrated earlier, in `movement_system`).
const M2_CARRIER: u32 = 1 << 6;

/// A continuous tile-aligned wanderer walker (the NES-era rank-and-file walk pattern).
///
/// ⚠️ This lives in `mask2` because the `mask` word is FULL — bit 31 (C_SLEEP) is the last one.
const M2_WANDERER: u32 = 1 << 7;

/// Gravity for the bouncer's hop arc, Q8 px/frame² (~0.16 px/f²).
const NAI_BOUNCE_G: i32 = 40;

/// Enemy AI column (`M2_NAI`). One state machine struct for all four kinds keeps the spawn
/// and reset paths to a single extra column; each kind is O(1) per frame (a few compares + stores).
/// `kind`: 1 ambusher, 2 drifter, 3 flicker-caster, 4 bouncer. `a`/`b` are the two phase lengths
/// (hidden/surfaced, rest/fly, hidden/visible, rest/hop). `speed` is Q8 px/frame.
#[derive(Clone, Copy, Default)]
struct Nai {
    kind: u8,
    state: u8,
    timer: i32,
    a: i32,
    b: i32,
    speed: i32,
    /// Drifter spin-up/down increment per frame (Q8), computed once at configure time so the ramp
    /// costs no division on the hot path.
    step: i32,
    /// Current heading (-1/0/1 each axis).
    dx: i32,
    dy: i32,
    /// Kind-specific scratch: drifter = current ramped speed; flicker = fired-this-window flag;
    /// bouncer = the sprite's base `oy` (the hop arc writes `oy = base - z`).
    aux: i32,
    /// Bouncer hop arc, Q8 height + Q8 vertical speed.
    z: i32,
    vz: i32,
    /// Flicker-caster: the bullet style captured at configure time (same contract as `set_shooter`).
    style: BulletStyle,
}

/// Boss-glue column (`M2_PROXY` / `M2_GATE` / `M2_NOTE`): hit re-routing, vulnerability gate and
/// part-death notification. Read only when the matching mask2 bit is set.
#[derive(Clone, Copy, Default)]
struct Zx {
    proxy: i32,
    gate: i32,
    code: i32,
}

#[derive(Clone, Copy, Default)]
struct Jumper {
    timer: i32,
    state: i32, // 0 = idle, 1 = jumping
    dx: Fixed,
    dy: Fixed,
    z: Fixed,
    dz: Fixed,
}

/// A triangle wave in `[-amp, amp]` over `period` frames — a soft-float-free stand-in for `sin`, so a
/// weave costs a few integer/fixed ops (no `libm` on the FPU-less ARM7TDMI). `0` outside a valid period.
fn tri_fixed(t: i32, period: i32, amp: Fixed) -> Fixed {
    if period <= 0 {
        return Fixed::from_raw(0);
    }
    let half = (period / 2).max(1);
    let p = t.rem_euclid(period);
    // `frac` ramps 0→1→0 across the period (Q8 fixed).
    let frac_raw = if p < half {
        (p * 256) / half
    } else {
        ((period - p) * 256) / half
    };
    let frac = Fixed::from_raw(frac_raw);
    // (frac*2 - 1) ∈ [-1, 1], then scale by amp.
    (frac * Fixed::from_raw(2 * 256) - Fixed::from_raw(256)) * amp
}

/// Room-locked camera: the map is a grid of screen-sized "rooms" (`room_w`×`room_h` tiles).
/// The camera locks to the player's current room instead of smooth-following; when the player
/// steps across a room boundary the view SLIDES to the next room over `dur` frames (input locked),
/// the player sliding from one screen edge to the other. `enabled=false` = classic follow camera.
#[derive(Clone, Copy, Default)]
struct RoomCam {
    enabled: bool,
    room_w: i32,
    room_h: i32,
    cur_rx: i32,
    cur_ry: i32,
    transitioning: bool,
    timer: i32,
    dur: i32,
    from_cam: (i32, i32),
    to_cam: (i32, i32),
    from_px: (i32, i32),
    to_px: (i32, i32),
}

// ── Behaviour bridge (the "Unity component" feel) ────────────────────────────
// A tish `defineComponent(name, { start, update })` registers a `ComponentDef`
// (the callbacks, held as `Value`s). `addBehaviour(entity, name, data)` attaches a
// `BehaviourInstance` (which def + the per-instance data object). Each frame the
// pipeline invokes `update(self, entity, dt)` — the callback reads/writes its `self`
// data and drives the entity through the engine API (`set_body`, `entity_x`, …).

/// A registered component type: its name + `start`/`update` callbacks (`Value::Null`
/// when absent). Callbacks are tish functions boxed as `Value::Function`.
struct ComponentDef {
    name: String,
    start: Value,
    update: Value,
    on_collide: Value,
    on_interact: Value,
    on_death: Value,
    /// Fast per-frame hook. Unlike `update` (which builds a rich method-bearing `this` and makes an
    /// ABI round-trip per operation), `tick` is handed the entity's `data` object pre-filled with
    /// its state (x/y/grounded/blocked/…) and reads its decisions back out (move/jump/…) — so a
    /// per-frame behaviour costs ONE tish call plus plain field reads/writes, not ~8 ABI trips.
    tick: Value,
    /// A `lean: true` component's tick is called with just the entity id (a number) and NO boxed
    /// context object: no per-frame `set_prop` marshalling of x/y/vx/vy in, no `prop_num` readback
    /// out. The tish tick reads state via the typed getters (`entity_x`, `cvar`, …) and writes via
    /// the typed setters (`set_body`, `set_cvar`, …), so a fully-typed tick pays zero boxed field ops.
    lean: bool,
}

/// A behaviour attached to one entity: which def, its mutable per-instance `data`
/// (a tish object, shared by `Rc` so callback mutations persist), and whether
/// `start` has run yet.
struct BehaviourInstance {
    def: usize,
    data: Value,
    started: bool,
}

// ── The SoA world ────────────────────────────────────────────────────────────
// Parallel columns indexed by entity SLOT. `mask[s]` says which components slot `s`
// has; systems iterate slots and test the mask. Generational ids (`gen[s]`) make a
// reused slot's id distinct so a stale entity handle can't alias a new entity.
struct World {
    gen: Vec<u16>,
    alive: Vec<bool>,
    mask: Vec<u32>,
    free: Vec<u32>,
    /// Which pool owns this slot, packed `(pool << 16) | poolSlot`, or -1 for an unpooled entity.
    ///
    /// ⚠️ This is OWNERSHIP, not state. `reset_entity` must NOT clear it — a re-armed slot is still
    /// the pool's, and clearing it would strand the pool's `live` count on the next retire. `despawn`
    /// and `clear_world` must, because the slot has left the pool's hands entirely.
    pool_of: Vec<i32>,
    pools: Vec<Pool>,
    dynamic: Vec<Dynamic>,
    /// Per-tile SURFACE class, 4 bits per cell (16 classes). Parallel to `solid` and — like
    /// `oneway` — EMPTY until the first write, so a game with no surfaces pays nothing. A 64x32
    /// golf course is 1 KB; a 240x80 overworld would be 9.6 KB, which is why it is a nibble plane
    /// and not a byte one.
    surface: Vec<u8>,
    surf: [SurfaceDef; 16],
    /// Reused scratch for the disc broadphase — never allocated per frame, same reason
    /// `buf_updates`/`buf_ticks` exist.
    buf_dyn: Vec<u32>,
    transform: Vec<Transform>,
    body: Vec<Body>,
    sprite: Vec<SpriteRef>,
    collider: Vec<Collider>,
    gridpos: Vec<GridPos>,
    anim: Vec<Anim>,
    walk: Vec<Walk>,
    platformer: Vec<Platformer>,
    health: Vec<Health>,
    patrol: Vec<Patrol>,
    life: Vec<Life>,
    hurt: Vec<Hurt>,
    guard: Vec<i32>,
    /// Weapon kinds that bounce off this entity, as `DMG_*` bits. 0 = hurt by everything, which is
    /// every entity until something calls `set_immunity`.
    ///
    /// A slab of its own rather than a field on `Health`, because `set_health` rebuilds `Health`
    /// wholesale — a boss that re-arms its hit points mid-fight (multi-phase bosses do) would
    /// silently lose its immunities.
    immune: Vec<i32>,
    /// Damage-type vulnerability mask (`DMG_*` bits). 0 = hurt by everything; non-zero ALLOWS only
    /// matching weapon kinds (the complement of `immune`). Same slab rationale as `immune`.
    weak: Vec<i32>,
    diranim: Vec<DirAnim>,
    mover: Vec<Mover>,
    topdown: Vec<TopDown>,
    /// Shared destination fields (RTS move orders) and the fog-of-war plane. Both are world state
    /// rather than separate globals, because both are derived from the collision grid the World
    /// already owns — a second global would be a second thing to keep in step.
    flows: [FlowField; MAX_FLOWS],
    fog: Fog,
    /// Terrain as a streamed window (an alternative to `scene:`; see `terrain_load`).
    terr: Vec<i32>,
    terr_cols: i32,
    terr_rows: i32,
    terr_shown: Vec<i32>,
    terr_win: Vec<i32>,
    /// False until fog has been computed once, so the first frame always runs a full pass.
    fog_settled: bool,
    chase: Vec<Chase>,
    seek: Vec<Seek>,
    soldier: Vec<Soldier>,
    vision: Vec<Vision>,
    hopper: Vec<Hopper>,
    jumper: Vec<Jumper>,
    /// Per-entity game-defined kind tag (0 = untagged). Not a component — a lightweight label so a
    /// collision/interaction handler can tell *what* it hit (player vs enemy vs pickup) without
    /// threading the other entity's component data through. Set with `set_tag`, read with `entity_tag`.
    tag: Vec<i32>,
    shooter: Vec<Shooter>,
    charger: Vec<Charger>,
    grabber: Vec<Grabber>,
    trap: Vec<Trap>,
    follow: Vec<Follow>,
    boomerang: Vec<Boomerang>,
    /// Second component mask — see the `M2_*` bits. Additive: 0 means "nothing new applies".
    mask2: Vec<u32>,
    /// S1: every mask bit EVER attached this scene, so a system whose component has no users can be
    /// skipped without scanning. STICKY on purpose — cleared only by `clear_world` — because
    /// per-despawn recount bookkeeping is exactly the kind of many-site invariant that rots; a
    /// system whose last user died keeps scanning until the scene changes, which is today's cost.
    used: u32,
    used2: u32,
    nai: Vec<Nai>,
    wanderer: Vec<Wanderer>,
    zx: Vec<Zx>,
    /// Carrier support (`M2_CARRIER`): each carrier's top-left transform as of the END of last
    /// frame's `platformer_system`, so riders can inherit this frame's motion delta. Gated by the
    /// mask2 bit — a stale value in a recycled slot is unreachable.
    carr_prev: Vec<(Fixed, Fixed)>,
    /// Part-death notification FIFO (`set_death_note` / `death_note`), capped small — a boss has a
    /// handful of parts, not hundreds.
    death_notes: Vec<i32>,
    /// Boomerang catches since the game last asked (`boomerang_caught`).
    boomer_catches: i32,
    /// Frames of stun remaining. While non-zero the native AI systems (hopper / chase / jumper)
    /// hold the entity still. Written by `hurt_system` from the hurt box's own `stun`.
    stun: Vec<i32>,
    /// A decoy the native AI prefers over the player while it lasts: `(entity, radius px, frames)`.
    /// This is the classic bait item — enemies inside the radius walk to it instead of at the player.
    lure: (i32, i32, i32),
    /// Per-entity TYPED state slots — 8 raw `i32` cells a component's tish `tick` can read/write
    /// natively (via `cvar`/`set_cvar` typed externs), instead of storing counters/flags on a boxed
    /// `Value` context object and paying a hashmap `get_prop`/`set_prop` for every field access every
    /// frame. `fixed` values are stored as their raw i32 (`cvarf`/`set_cvarf` do the ×256 conversion).
    ndata: Vec<[i32; 8]>,
    /// Per-entity behaviour (most entities have none → `Option`, and `Value` isn't `Copy`).
    behaviour: Vec<Option<BehaviourInstance>>,
    /// Registered component types (the `defineComponent` registry, not per-entity).
    defs: Vec<ComponentDef>,
    /// Tile-collision grid for the RPG genre. Bit-packed (1 bit/cell) so a 240×80 overworld
    /// is ~2.4KB instead of ~19KB per plane — critical for EWRAM when warping to/from caves.
    grid_cols: i32,
    grid_rows: i32,
    solid: Vec<u8>,
    /// One-way platform bits (parallel to `solid`). Empty until first `grid_set_oneway`.
    oneway: Vec<u8>,
    /// Climbable (ladder / vine / rope) bits, parallel to `solid`. Empty until first
    /// `grid_set_ladder`, so nothing but a side-scroller with ladders pays for the plane.
    /// The physics does not read this — climbing is a state a game drives with `platformer_hold`
    /// + `set_transform`; the grid just answers "is there something to climb here?".
    ladder: Vec<u8>,
    /// Logical cell count the bitplanes cover (may exceed cols*rows after a shrink).
    grid_cells: usize,
    /// Entity the camera follows (its transform is centred on screen, clamped to the map).
    /// `None` = no camera (fixed screen; small maps).
    camera_target: Option<i32>,
    rng: u32,
    /// Room-locked camera (screen-by-screen with edge-slide transitions). Off by default;
    /// `set_room_camera` turns it on and takes over from the smooth-follow path.
    room_cam: RoomCam,
    /// Last camera top-left in world px (set by `update_camera`), used for off-screen culling.
    cam_x: i32,
    cam_y: i32,
    /// Current bullet spawn configuration for the native pattern emitters (`fire_ring` et al.).
    bullet_style: BulletStyle,
    /// Wrap-around ("toroidal") arena: the Asteroids rule, where leaving one edge re-enters the
    /// opposite one. Off by default; `set_arena_wrap` turns it on for the whole world.
    arena_wrap: bool,
    buf_updates: Vec<(Value, Value, i32)>,
    buf_ticks: Vec<TickJob>,
    buf_results: Vec<(i32, TickOut)>,
}

fn grid_bit_bytes(cells: usize) -> usize {
    cells.div_ceil(8)
}

fn grid_bit(bits: &[u8], i: usize) -> bool {
    let b = i / 8;
    if b >= bits.len() {
        return false;
    }
    (bits[b] & (1u8 << (i & 7))) != 0
}

fn grid_bit_set(bits: &mut [u8], i: usize, on: bool) {
    let b = i / 8;
    if b >= bits.len() {
        return;
    }
    if on {
        bits[b] |= 1u8 << (i & 7);
    } else {
        bits[b] &= !(1u8 << (i & 7));
    }
}

/// Pack `(slot, generation)` into the i32 entity id a tish program holds.
fn encode(slot: u32, gen: u16) -> i32 {
    ((gen as i32) << 16) | (slot as i32 & 0xFFFF)
}
/// Unpack an entity id into `(slot, generation)`.
fn decode(e: i32) -> (u32, u16) {
    ((e & 0xFFFF) as u32, ((e >> 16) & 0x7FFF) as u16)
}

/// Read a Value property as a number (0 if absent/not a number).
fn prop_num(obj: &Value, key: &str) -> f64 {
    match get_prop(obj, key) {
        Value::Number(x) => x,
        _ => 0.0,
    }
}
/// Read a Value property as a bool (a nonzero number counts as true).
fn prop_truthy(obj: &Value, key: &str) -> bool {
    match get_prop(obj, key) {
        Value::Bool(b) => b,
        Value::Number(x) => x != 0.0,
        _ => false,
    }
}

/// One entity's `tick` job: the callback + its data ctx + the state to pre-fill.
#[derive(Clone)]
struct TickJob {
    cb: Value,
    data: Value,
    entity: i32,
    x: i32,
    y: i32,
    grounded: bool,
    blocked: bool,
    /// Current free-movement velocity (px/frame), pre-filled into the ctx so a hook that ignores
    /// `vx`/`vy` keeps its heading. 0 for entities without a `Body`.
    vx: f64,
    vy: f64,
    /// Which output group this entity actually uses — so the pre-fill/read-back only touches the
    /// props that matter (a free-flying shmup enemy pays for `vx`/`vy`, not the 9 platformer props).
    platformer: bool,
    body: bool,
    /// A `lean` tick gets only the entity id (no boxed ctx); the loop skips all marshalling/readback.
    lean: bool,
}
/// A `tick`'s decisions, read back from the data ctx after the callback.
#[derive(Clone, Copy, Default)]
struct TickOut {
    move_dir: i32,
    jump: bool,
    jump_cut: bool,
    run: bool,
    drop: bool,
    flip: bool,
    bounce: i32,
    /// Free-movement velocity (px/frame) for a top-down / free-flying entity — a shmup enemy's
    /// weave/dive pattern writes these and the engine applies them to `Body`. Independent of the
    /// platformer `move`/`jump` outputs (an entity uses one movement model or the other). Pre-filled
    /// with the entity's current velocity, so a hook that leaves them alone keeps flying straight.
    vx: f64,
    vy: f64,
}

/// Move `*v` toward `target` by at most `step`, snapping on arrival. Returns whether it
/// reached the target this call.
fn approach(v: &mut Fixed, target: Fixed, step: Fixed) -> bool {
    if *v < target {
        *v += step;
        if *v >= target {
            *v = target;
            return true;
        }
        false
    } else if *v > target {
        *v -= step;
        if *v <= target {
            *v = target;
            return true;
        }
        false
    } else {
        true
    }
}

impl World {
    const fn new() -> Self {
        World {
            gen: Vec::new(),
            alive: Vec::new(),
            mask: Vec::new(),
            free: Vec::new(),
            pool_of: Vec::new(),
            pools: Vec::new(),
            dynamic: Vec::new(),
            surface: Vec::new(),
            surf: [SurfaceDef::flat(); 16],
            buf_dyn: Vec::new(),
            transform: Vec::new(),
            body: Vec::new(),
            sprite: Vec::new(),
            collider: Vec::new(),
            gridpos: Vec::new(),
            anim: Vec::new(),
            walk: Vec::new(),
            platformer: Vec::new(),
            health: Vec::new(),
            patrol: Vec::new(),
            life: Vec::new(),
            hurt: Vec::new(),
            guard: Vec::new(),
            immune: Vec::new(),
            weak: Vec::new(),
            diranim: Vec::new(),
            mover: Vec::new(),
            topdown: Vec::new(),
            flows: [
                FlowField::new(),
                FlowField::new(),
                FlowField::new(),
                FlowField::new(),
                FlowField::new(),
                FlowField::new(),
            ],
            fog: Fog::new(),
            terr: Vec::new(),
            terr_cols: 0,
            terr_rows: 0,
            terr_shown: Vec::new(),
            terr_win: Vec::new(),
            fog_settled: false,
            chase: Vec::new(),
            seek: Vec::new(),
            soldier: Vec::new(),
            vision: Vec::new(),
            hopper: Vec::new(),
            jumper: Vec::new(),
            tag: Vec::new(),
            shooter: Vec::new(),
            charger: Vec::new(),
            grabber: Vec::new(),
            trap: Vec::new(),
            follow: Vec::new(),
            boomerang: Vec::new(),
            mask2: Vec::new(),
            used: 0,
            used2: 0,
            nai: Vec::new(),
            wanderer: Vec::new(),
            zx: Vec::new(),
            carr_prev: Vec::new(),
            death_notes: Vec::new(),
            boomer_catches: 0,
            stun: Vec::new(),
            lure: (-1, 0, 0),
            ndata: Vec::new(),
            behaviour: Vec::new(),
            defs: Vec::new(),
            grid_cols: 0,
            grid_rows: 0,
            solid: Vec::new(),
            oneway: Vec::new(),
            ladder: Vec::new(),
            grid_cells: 0,
            camera_target: None,
            rng: 123456789,
            room_cam: RoomCam {
                enabled: false,
                room_w: 15,
                room_h: 10,
                cur_rx: 0,
                cur_ry: 0,
                transitioning: false,
                timer: 0,
                dur: 24,
                from_cam: (0, 0),
                to_cam: (0, 0),
                from_px: (0, 0),
                to_px: (0, 0),
            },
            cam_x: 0,
            cam_y: 0,
            bullet_style: BulletStyle {
                sheet: 0,
                frame: 0,
                size: 6,
                damage: 1,
                target: 0,
                tag: 0,
                ttl: 240,
                // 0 = untyped, which `BulletStyle::damage_type`'s own comment names as what every
                // existing caller gets.
                damage_type: 0,
            },
            arena_wrap: false,
            buf_updates: Vec::new(),
            buf_ticks: Vec::new(),
            buf_results: Vec::new(),
        }
    }

    fn spawn(&mut self) -> i32 {
        let slot = if let Some(s) = self.free.pop() {
            let s = s as usize;
            self.alive[s] = true;
            self.mask[s] = 0;
            self.behaviour[s] = None;
            self.tag[s] = 0; // a recycled slot must not inherit the previous entity's kind tag
            self.stun[s] = 0;
            // Same reason as `tag`: `immune` is not gated behind a component mask bit, so without
            // this a recycled slot would inherit whatever the last entity in it was immune to —
            // a boss's immunities turning up on a rank-and-file walker.
            self.immune[s] = 0;
            // Same reason as `immune`: weakness is not gated by a mask bit.
            self.weak[s] = 0;
            self.ndata[s] = [0; 8]; // clear typed state slots so a reused entity starts fresh
                                    // A reused slot must not carry the previous entity's sprite handle (despawn/clear_world
                                    // already free + clear it; reset defensively so a stale handle can't alias a recycled
                                    // tish-agb sprite).
            self.sprite[s].handle = -1;
            self.sprite[s].ox = 0;
            self.sprite[s].oy = 0;
            // A recycled slot belongs to nobody until a pool claims it, for the same reason `tag`
            // and `immune` are cleared here: inheriting the last occupant's owner is how a retire
            // ends up decrementing a pool that never armed this entity.
            self.pool_of[s] = -1;
            // mask2 gates every new column, but clear the columns too so a recycled slot cannot
            // leak a previous life's proxy target or death code if a bit is set later.
            self.mask2[s] = 0;
            self.nai[s] = Nai::default();
            self.wanderer[s] = Wanderer::default();
            self.zx[s] = Zx::default();
            self.carr_prev[s] = (Fixed::from_raw(0), Fixed::from_raw(0));
            s
        } else {
            let s = self.gen.len();
            self.gen.push(0);
            self.alive.push(true);
            self.mask.push(0);
            self.pool_of.push(-1);
            self.dynamic.push(Dynamic::default());
            self.transform.push(Transform::default());
            self.body.push(Body::default());
            self.sprite.push(SpriteRef {
                handle: -1,
                ox: 0,
                oy: 0,
            });
            self.collider.push(Collider::default());
            self.gridpos.push(GridPos::default());
            self.anim.push(Anim::default());
            self.walk.push(Walk::default());
            self.platformer.push(Platformer::default());
            self.health.push(Health::default());
            self.patrol.push(Patrol::default());
            self.life.push(Life::default());
            self.hurt.push(Hurt::default());
            self.guard.push(0);
            self.immune.push(0);
            self.weak.push(0);
            self.diranim.push(DirAnim::default());
            self.mover.push(Mover::default());
            self.topdown.push(TopDown::default());
            self.chase.push(Chase::default());
            self.seek.push(Seek::default());
            self.soldier.push(Soldier::default());
            self.vision.push(Vision::default());
            self.hopper.push(Hopper::default());
            self.jumper.push(Jumper::default());
            self.tag.push(0);
            self.shooter.push(Shooter::default());
            self.charger.push(Charger::default());
            self.grabber.push(Grabber::default());
            self.trap.push(Trap::default());
            self.follow.push(Follow::default());
            self.boomerang.push(Boomerang::default());
            self.mask2.push(0);
            self.nai.push(Nai::default());
            self.wanderer.push(Wanderer::default());
            self.zx.push(Zx::default());
            self.carr_prev
                .push((Fixed::from_raw(0), Fixed::from_raw(0)));
            self.stun.push(0);
            self.ndata.push([0; 8]);
            self.behaviour.push(None);
            s
        };
        encode(slot as u32, self.gen[slot])
    }

    /// Register a component type from a `defineComponent(name, config)` call, returning
    /// its def index. `config.start` / `config.update` are the tish callbacks.
    fn define_component(&mut self, name: String, config: &Value) -> usize {
        let start = get_prop(config, "start");
        let update = get_prop(config, "update");
        let on_collide = get_prop(config, "onCollide");
        let on_interact = get_prop(config, "onInteract");
        let on_death = get_prop(config, "onDeath");
        let tick = get_prop(config, "tick");
        let lean = get_prop(config, "lean").is_truthy();
        let idx = self.defs.len();
        self.defs.push(ComponentDef {
            name,
            start,
            update,
            on_collide,
            on_interact,
            on_death,
            tick,
            lean,
        });
        idx
    }

    fn def_index_by_name(&self, name: &str) -> Option<usize> {
        self.defs.iter().position(|d| d.name == name)
    }

    /// Collect the behaviour callbacks to run this frame as `(callback, self_data,
    /// entity)` tuples — under the world borrow — WITHOUT invoking them. The caller
    /// runs them after dropping the borrow, so a callback may re-enter the engine
    /// (`set_body`, …) without a `RefCell` double-borrow. A fresh behaviour's `start`
    /// is queued before its `update`.
    fn collect_behaviours(&mut self) {
        self.buf_updates.clear();
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.behaviour[s].is_none() {
                continue;
            }
            // Off-screen culling: skip the (expensive tish) behaviour callbacks for entities that
            // have scrolled out of view. The camera target is always active.
            if !self.is_active(s) {
                continue;
            }
            let (def_idx, was_started) = {
                let b = self.behaviour[s].as_ref().unwrap();
                (b.def, b.started)
            };
            // Peek what actually needs to run BEFORE cloning the data ctx — a component with only an
            // `onDeath` (e.g. a straight-flying shmup grunt) runs nothing here, and cloning its data
            // every frame for nothing is pure overhead at bullet-hell entity counts.
            let has_start = !was_started && !matches!(self.defs[def_idx].start, Value::Null);
            let has_update = !matches!(self.defs[def_idx].update, Value::Null);
            if !was_started {
                self.behaviour[s].as_mut().unwrap().started = true;
            }
            if !has_start && !has_update {
                continue;
            }
            let data = self.behaviour[s].as_ref().unwrap().data.clone();
            let entity = encode(s as u32, self.gen[s]);
            if has_start {
                self.buf_updates
                    .push((self.defs[def_idx].start.clone(), data.clone(), entity));
            }
            if has_update {
                self.buf_updates
                    .push((self.defs[def_idx].update.clone(), data, entity));
            }
        }
    }

    /// Gather the `tick` jobs for active entities (the fast per-frame hook). Called after
    /// `collect_behaviours` (so `start` has run). The caller fills each job's data ctx with state,
    /// invokes the callback, and applies the outputs — all without the world borrow held.
    fn collect_ticks(&mut self) {
        self.buf_ticks.clear();
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.behaviour[s].is_none() || !self.is_active(s) {
                continue;
            }
            let def_idx = self.behaviour[s].as_ref().unwrap().def;
            // Peek `tick` BEFORE cloning the data ctx — most entities have no tick (a straight grunt,
            // a bullet with no behaviour), so cloning their data every frame is wasted work.
            if matches!(self.defs[def_idx].tick, Value::Null) {
                continue;
            }
            let cb = self.defs[def_idx].tick.clone();
            let entity = encode(s as u32, self.gen[s]);
            // Lean tick: no ctx marshalling at all — the callback gets just the entity id and reads/
            // writes state through the typed getters/setters. Skips the data clone + x/y/vx/vy prep.
            if self.defs[def_idx].lean {
                self.buf_ticks.push(TickJob {
                    cb,
                    data: Value::Null,
                    entity,
                    x: 0,
                    y: 0,
                    grounded: false,
                    blocked: false,
                    vx: 0.0,
                    vy: 0.0,
                    platformer: false,
                    body: false,
                    lean: true,
                });
                continue;
            }
            let data = self.behaviour[s].as_ref().unwrap().data.clone();
            let has_p = self.any(s, C_PLATFORMER);
            let has_b = self.any(s, C_BODY);
            self.buf_ticks.push(TickJob {
                cb,
                data,
                entity,
                x: self.transform[s].x.floor(),
                y: self.transform[s].y.floor(),
                grounded: has_p && self.platformer[s].grounded,
                blocked: has_p && self.platformer[s].blocked,
                vx: if has_b {
                    from_fixed(self.body[s].vx)
                } else {
                    0.0
                },
                vy: if has_b {
                    from_fixed(self.body[s].vy)
                } else {
                    0.0
                },
                platformer: has_p,
                body: has_b,
                lean: false,
            });
        }
    }

    /// Apply a `tick`'s decisions to its entity's platformer (move intent, jump, run, drop, bounce)
    /// and sprite flip — all native, no ABI round-trip.
    fn apply_tick(&mut self, entity: i32, out: &TickOut) {
        let Some(s) = self.slot_of(entity) else {
            return;
        };
        if self.any(s, C_PLATFORMER) {
            let p = &mut self.platformer[s];
            p.dir = out.move_dir.signum();
            p.run = out.run;
            if out.drop {
                p.drop = P_DROP;
            }
            if out.jump {
                p.jump_buffer = P_JUMP_BUFFER;
            }
            if out.jump_cut && p.jumping && p.vy.to_raw() < 0 {
                p.vy = Fixed::from_raw(p.vy.to_raw() / 2);
                p.jumping = false;
            }
            if out.bounce > 0 {
                p.vy = Fixed::from_raw(-out.bounce.abs() * 256);
                p.grounded = false;
                p.jumping = false;
            }
        }
        // Free-movement steering (top-down / flying entities): a `tick`'s `vx`/`vy` (pre-filled with
        // the current velocity, so an untouched hook flies straight) write into `Body`, which
        // `movement_system` integrates. Platformer entities steer with `move`/`jump` instead, so this
        // is scoped to free-movers (has `Body`, not `Platformer`).
        if self.any(s, C_BODY) && !self.any(s, C_PLATFORMER) {
            self.body[s].vx = to_fixed(out.vx);
            self.body[s].vy = to_fixed(out.vy);
        }
        if self.any(s, C_SPRITE) {
            let h = self.sprite[s].handle;
            if h >= 0 {
                tish_agb::native_sprite_set_flip(h, out.flip);
            }
        }
    }

    /// The live slot for entity id `e`, or `None` if `e` is stale/dead/out-of-range.
    fn slot_of(&self, e: i32) -> Option<usize> {
        let (slot, g) = decode(e);
        let s = slot as usize;
        if s < self.alive.len() && self.alive[s] && self.gen[s] == g {
            Some(s)
        } else {
            None
        }
    }

    /// Read/write a component's typed i32 state slot (`k` clamped to 0..8). Native — the tish tick
    /// keeps its counters/flags here instead of on a boxed `Value` object.
    fn cvar(&self, e: i32, k: usize) -> i32 {
        self.slot_of(e).map(|s| self.ndata[s][k & 7]).unwrap_or(0)
    }
    fn set_cvar(&mut self, e: i32, k: usize, v: i32) {
        if let Some(s) = self.slot_of(e) {
            self.ndata[s][k & 7] = v;
        }
    }

    fn despawn(&mut self, e: i32) {
        if let Some(s) = self.slot_of(e) {
            // FREE the tish-agb sprite (releases its VRAM + recycles the arena slot), not just hide
            // it — otherwise every despawn/respawn cycle (projectiles, enemies) would leak a sprite.
            // The engine owns the reference, so drop it too so a reused entity slot can't reach it.
            if self.any(s, C_SPRITE) && self.sprite[s].handle >= 0 {
                tish_agb::native_sprite_destroy(self.sprite[s].handle);
                self.sprite[s].handle = -1;
            }
            self.alive[s] = false;
            self.mask[s] = 0;
            self.mask2[s] = 0;
            self.behaviour[s] = None;
            // Leaving a pool's hands for good. The pool's `ent[]` entry is now a stale id, which
            // `pool_arm` detects through `slot_of` and refuses rather than resetting a dead slot.
            self.pool_of[s] = -1;
            self.gen[s] = self.gen[s].wrapping_add(1);
            self.free.push(s as u32);
        }
    }

    /// Reset a LIVE entity to a blank slate without despawning it: every component bit,
    /// native-AI system, timer, cvar, tag, immunity, weakness, stun and behaviour is cleared, but
    /// the slot, its generation (the id stays valid) and its tish-agb sprite handle all
    /// survive. This is the keystone of a pooled cast: room population reconfigures a
    /// fixed set of entities instead of despawn/spawn churn — a spawn costs ~1,400 ticks
    /// on the frame it happens (docs/perf-rules.md §6) and reallocates sprite VRAM, while
    /// a reset is a few dozen stores. The kept sprite is HIDDEN (not freed) so a parked
    /// slot draws nothing; the caller re-points it with `sprite_set_sheet`/`sprite_set_frame`
    /// and shows it again when the slot is reconfigured. `C_SPRITE` stays set for a kept
    /// handle so a later `despawn`/`clear_world` still frees the VRAM.
    fn reset_entity(&mut self, e: i32) {
        if let Some(s) = self.slot_of(e) {
            let handle = self.sprite[s].handle;
            if handle >= 0 {
                tish_agb::native_sprite_set_visible(handle, false);
            }
            self.mask[s] = if handle >= 0 { C_SPRITE } else { 0 };
            self.used |= C_SPRITE;
            self.behaviour[s] = None;
            self.tag[s] = 0;
            self.stun[s] = 0;
            self.immune[s] = 0;
            self.weak[s] = 0;
            self.ndata[s] = [0; 8];
            self.transform[s] = Transform::default();
            self.body[s] = Body::default();
            self.collider[s] = Collider::default();
            self.gridpos[s] = GridPos::default();
            self.anim[s] = Anim::default();
            self.walk[s] = Walk::default();
            self.platformer[s] = Platformer::default();
            self.health[s] = Health::default();
            self.patrol[s] = Patrol::default();
            self.life[s] = Life::default();
            self.hurt[s] = Hurt::default();
            self.guard[s] = 0;
            self.diranim[s] = DirAnim::default();
            self.mover[s] = Mover::default();
            self.topdown[s] = TopDown::default();
            self.chase[s] = Chase::default();
            self.seek[s] = Seek::default();
            self.soldier[s] = Soldier::default();
            self.vision[s] = Vision::default();
            self.hopper[s] = Hopper::default();
            self.jumper[s] = Jumper::default();
            self.shooter[s] = Shooter::default();
            self.charger[s] = Charger::default();
            self.grabber[s] = Grabber::default();
            self.trap[s] = Trap::default();
            self.follow[s] = Follow::default();
            self.boomerang[s] = Boomerang::default();
            self.mask2[s] = 0;
            self.nai[s] = Nai::default();
            self.wanderer[s] = Wanderer::default();
            self.zx[s] = Zx::default();
            self.carr_prev[s] = (Fixed::from_raw(0), Fixed::from_raw(0));
            self.dynamic[s] = Dynamic::default();
            self.sprite[s] = SpriteRef {
                handle,
                ox: 0,
                oy: 0,
            };
        }
    }

    // ── Rigid discs ──────────────────────────────────────────────────────────
    // Golf, soccer, pinball, pool. See `struct Dynamic`.

    fn surface_at(&self, col: i32, row: i32) -> u8 {
        if self.surface.is_empty()
            || col < 0
            || row < 0
            || col >= self.grid_cols
            || row >= self.grid_rows
        {
            return 0;
        }
        let i = (row * self.grid_cols + col) as usize;
        let b = self.surface[i >> 1];
        if i & 1 == 0 {
            b & 0x0f
        } else {
            b >> 4
        }
    }

    fn grid_set_surface(&mut self, col: i32, row: i32, id: i32) {
        if col < 0 || row < 0 || col >= self.grid_cols || row >= self.grid_rows {
            return;
        }
        if self.surface.is_empty() {
            // Allocated on FIRST WRITE, exactly like `oneway`: a game with no surfaces pays nothing.
            self.surface = alloc::vec![0u8; self.grid_cells.div_ceil(2)];
        }
        let i = (row * self.grid_cols + col) as usize;
        let v = (id.clamp(0, 15) as u8) & 0x0f;
        let b = &mut self.surface[i >> 1];
        if i & 1 == 0 {
            *b = (*b & 0xf0) | v
        } else {
            *b = (*b & 0x0f) | (v << 4)
        }
    }

    /// How many substeps this velocity needs so a single-step clamp per axis stays EXACT.
    ///
    /// ⚠️ A COMPARE LADDER, NOT A DIVISION. The alternative is a swept `tmax/tdelta` DDA, which is
    /// a software division per axis per body per frame. Substepping instead preserves the sub-tile
    /// speed invariant that makes `last_cell` exact — the comment there records what violating it
    /// costs: "not a rounding error, it is a teleport".
    fn substeps(vx: Fixed, vy: Fixed) -> i32 {
        const D_MAX_STEP: i32 = 2048; // 8 px/frame, against TILE = 16
        let m = vx.to_raw().abs().max(vy.to_raw().abs());
        if m <= D_MAX_STEP {
            1
        } else if m <= D_MAX_STEP << 1 {
            2
        } else if m <= D_MAX_STEP << 2 {
            4
        } else {
            8
        }
    }

    fn dynamic_system(&mut self) {
        let n = self.alive.len();

        // ── Pass 1: surface acceleration + friction ──────────────────────────
        for s in 0..n {
            if !self.alive[s]
                || self.mask[s] & C_SLEEP != 0
                || !self.has(s, C_DYNAMIC | C_TRANSFORM | C_BODY)
                || self.dynamic[s].asleep != 0
            {
                continue;
            }
            let (cx, cy) = self.center_of(s);
            let id = self.surface_at(cx.to_raw() / (TILE << 8), cy.to_raw() / (TILE << 8)) as usize;
            let sd = self.surf[id];
            let f = if sd.friction != 0 {
                sd.friction
            } else {
                self.dynamic[s].friction
            };
            let mut b = self.body[s];
            b.vx += sd.ax;
            b.vy += sd.ay;
            // Q8 retention: a multiply and a shift, never a divide.
            b.vx = Fixed::from_raw((b.vx.to_raw() * f) >> 8);
            b.vy = Fixed::from_raw((b.vy.to_raw() * f) >> 8);
            self.body[s] = b;
        }

        // ── Pass 2: integrate, bouncing off solid tiles ──────────────────────
        //
        // ⚠️ `is_solid` REPORTS OUT-OF-BOUNDS AS SOLID, so a game that has set up no collision grid
        // has every cell reading solid and a ball bounces off the air on its first frame, loses its
        // energy to restitution and parks about two pixels from where it started. `life_system`
        // guards its own solid check the same way and says why: "a shmup sets up no grid (every
        // cell would read solid)". Without the grid there is nothing to bounce off, so integrate
        // plainly — the arena is whatever the game draws.
        let walled = self.grid_cols > 0;
        for s in 0..n {
            if !self.alive[s]
                || !self.has(s, C_DYNAMIC | C_TRANSFORM | C_BODY | C_COLLIDER)
                || self.dynamic[s].asleep != 0
            {
                continue;
            }
            let steps = Self::substeps(self.body[s].vx, self.body[s].vy);
            let rest = self.dynamic[s].restitution;
            for _ in 0..steps {
                let (vx, vy) = (self.body[s].vx / steps, self.body[s].vy / steps);
                let c = self.collider[s];
                // X, then Y — the same axis-by-axis clamp `platformer_system` uses, but REFLECTING
                // instead of zeroing.
                let t = self.transform[s];
                let nx = t.x + vx;
                if walled && self.box_hits_solid(nx, t.y, c.w, c.h) {
                    self.body[s].vx = Fixed::from_raw(-((self.body[s].vx.to_raw() * rest) >> 8));
                } else {
                    self.transform[s].x = nx;
                }
                let t = self.transform[s];
                let ny = t.y + vy;
                if walled && self.box_hits_solid(t.x, ny, c.w, c.h) {
                    self.body[s].vy = Fixed::from_raw(-((self.body[s].vy.to_raw() * rest) >> 8));
                } else {
                    self.transform[s].y = ny;
                }
            }
        }

        // ── Pass 3: sleep ────────────────────────────────────────────────────
        //
        // ⚠️ BEFORE CONTACT, NOT AFTER, and the ordering is the whole of a bug that took a soak to
        // see. Contact wakes a body it shoves, so that the tile-collision pass can test where it was
        // shoved TO. With the sleep test running last, that wake was undone in the same frame — the
        // body was asleep again by the time the next frame's tile check ran, so it skipped, and six
        // players herding a resting ball walked it straight through the hoarding: at frame 1,536 it
        // was at x=431 on a 352-wide pitch and still drifting, a fraction of a pixel per frame,
        // forever. Sleeping first means a body woken by contact stays awake for a frame and gets
        // its wall check.
        for s in 0..n {
            if !self.alive[s] || !self.has(s, C_DYNAMIC | C_BODY) || self.dynamic[s].asleep != 0 {
                continue;
            }
            let (vx, vy) = (self.body[s].vx.to_raw(), self.body[s].vy.to_raw());
            // Speed SQUARED, in raw Q8 shifted down — no sqrt, no divide.
            let v2 = ((vx >> 4) * (vx >> 4)) + ((vy >> 4) * (vy >> 4));
            if v2 <= self.dynamic[s].rest_v2 {
                self.body[s].vx = Fixed::from_raw(0);
                self.body[s].vy = Fixed::from_raw(0);
                self.dynamic[s].asleep = 1;
            }
        }
        // ── Pass 4: disc vs disc ─────────────────────────────────────────────
        // A SORTED SWEEP, not a spatial hash. Soccer is 12 agents plus a ball = 13 bodies = 78
        // pairs; a uniform-grid hash costs a bucket rebuild and an indirection every frame and
        // LOSES at n = 13. `collect_collisions`' own history is the precedent — its last
        // optimisation was a loop restructure, not a data structure.
        self.buf_dyn.clear();
        for s in 0..n {
            if self.alive[s] && self.has(s, C_DYNAMIC | C_CIRCLE | C_TRANSFORM | C_COLLIDER) {
                self.buf_dyn.push(s as u32);
            }
        }
        // Insertion sort by x: near-sorted frame to frame, so O(n) in practice.
        for i in 1..self.buf_dyn.len() {
            let mut j = i;
            while j > 0 {
                let (a, b) = (self.buf_dyn[j - 1] as usize, self.buf_dyn[j] as usize);
                if self.transform[a].x.to_raw() <= self.transform[b].x.to_raw() {
                    break;
                }
                self.buf_dyn.swap(j - 1, j);
                j -= 1;
            }
        }
        let m = self.buf_dyn.len();
        for i in 0..m {
            let a = self.buf_dyn[i] as usize;
            let ra = self.collider[a].w.to_raw() >> 1;
            for k in (i + 1)..m {
                let b = self.buf_dyn[k] as usize;
                let rb = self.collider[b].w.to_raw() >> 1;
                let (ca, cb) = (self.center_of(a), self.center_of(b));
                let dx = cb.0.to_raw() - ca.0.to_raw();
                // Sorted by x, so once the gap exceeds the widest possible contact nothing further
                // right can touch `a` either.
                if dx > ra + rb {
                    break;
                }
                let dy = cb.1.to_raw() - ca.1.to_raw();
                let rsum = ra + rb;
                // Integer pixel space for the test: raw Q8 squared overflows i32 fast, and the
                // radius-squared idiom is `kart.rs`'s (BOX_R2/HAZARD_R2).
                let (pdx, pdy, prs) = (dx >> 8, dy >> 8, rsum >> 8);
                if pdx * pdx + pdy * pdy > prs * prs {
                    continue;
                }
                if pdx == 0 && pdy == 0 {
                    continue;
                } // exactly concentric: no normal to push along
                self.resolve_pair(a, b, dx, dy, rsum);
            }
        }
    }

    /// One contact. ONE division here — the normalise — and it is paid per COLLIDING PAIR, never
    /// per body per frame. `Num::sqrt` is a digit-at-a-time restoring sqrt (shifts and subtracts,
    /// no division), which is why the cheap thing on this chip is the square root and the expensive
    /// one is the divide that follows it.
    fn resolve_pair(&mut self, a: usize, b: usize, dx: i32, dy: i32, rsum: i32) {
        let d2 = ((dx >> 8) * (dx >> 8) + (dy >> 8) * (dy >> 8)).max(1);
        let mut len = 1i32;
        while len * len < d2 {
            len += 1;
        } // integer sqrt, ~a dozen iterations at these radii
        let len_raw = (len << 8).max(1);
        let pen = rsum - len_raw;
        if pen <= 0 {
            return;
        }
        // Unit normal in Q8.
        let nx = (dx << 8) / len_raw;
        let ny = (dy << 8) / len_raw;

        // Rank decides who moves. EQUAL ranks split it with a shift; otherwise the lower rank takes
        // all of it. Zero divisions — see `struct Dynamic`.
        let (rka, rkb) = (self.dynamic[a].rank, self.dynamic[b].rank);
        let (sa, sb) = if rka == rkb {
            (pen >> 1, pen >> 1)
        } else if rka < rkb {
            (pen, 0)
        } else {
            (0, pen)
        };
        // ⚠️ MOVING A BODY WAKES IT, even when no impulse is exchanged.
        //
        // Pass 2 — the one that bounces a body off solid tiles — skips sleeping bodies, because a
        // parked body has nowhere to go. But a sleeping body can still be SHOVED here, and if it
        // stays asleep nothing ever tests where it was shoved TO. Six players herding a resting
        // ball walked it straight through the hoarding and out of the map: at frame 1,536 it was at
        // x=431 on a 352-wide pitch and still drifting, pushed a little further every frame by the
        // chasers that had followed it out. Waking it here puts it back under the wall check on the
        // very next frame.
        // ⚠️ A SHOVE MAY NOT PUT A BODY IN A WALL — checked HERE, not left to the integrator.
        //
        // The tile-collision pass runs before this one and skips sleeping bodies, so a resting body
        // shoved into a wall is never tested against it: six players herding a resting ball walked
        // it through the hoarding and out of the map, a fraction of a pixel per frame, forever
        // (x=431 on a 352-wide pitch and still going). Waking the body is not enough on its own —
        // the shove has already happened by the time anything else could look. Reverting a shove
        // that lands in a solid tile is local, needs no ordering to be right, and is correct for a
        // sleeping body and an awake one alike.
        if sa > 0 {
            let old = self.transform[a];
            self.transform[a].x -= Fixed::from_raw((nx * sa) >> 8);
            self.transform[a].y -= Fixed::from_raw((ny * sa) >> 8);
            let c = self.collider[a];
            if self.grid_cols > 0
                && self.box_hits_solid(self.transform[a].x, self.transform[a].y, c.w, c.h)
            {
                self.transform[a] = old;
            }
            self.dynamic[a].asleep = 0;
        }
        if sb > 0 {
            let old = self.transform[b];
            self.transform[b].x += Fixed::from_raw((nx * sb) >> 8);
            self.transform[b].y += Fixed::from_raw((ny * sb) >> 8);
            let c = self.collider[b];
            if self.grid_cols > 0
                && self.box_hits_solid(self.transform[b].x, self.transform[b].y, c.w, c.h)
            {
                self.transform[b] = old;
            }
            self.dynamic[b].asleep = 0;
        }

        // Exchange the normal component, scaled by the softer restitution of the two.
        let rest = self.dynamic[a].restitution.min(self.dynamic[b].restitution);
        let van = (self.body[a].vx.to_raw() * nx + self.body[a].vy.to_raw() * ny) >> 8;
        let vbn = (self.body[b].vx.to_raw() * nx + self.body[b].vy.to_raw() * ny) >> 8;
        let approaching = van - vbn;
        if approaching > 0 {
            let j = (approaching * (256 + rest)) >> 8;
            let (ja, jb) = if rka == rkb {
                (j >> 1, j >> 1)
            } else if rka < rkb {
                (j, 0)
            } else {
                (0, j)
            };
            if ja > 0 {
                self.body[a].vx -= Fixed::from_raw((nx * ja) >> 8);
                self.body[a].vy -= Fixed::from_raw((ny * ja) >> 8);
                self.dynamic[a].asleep = 0;
                self.dynamic[a].last_hit = encode(b as u32, self.gen[b]);
            }
            if jb > 0 {
                self.body[b].vx += Fixed::from_raw((nx * jb) >> 8);
                self.body[b].vy += Fixed::from_raw((ny * jb) >> 8);
                self.dynamic[b].asleep = 0;
                self.dynamic[b].last_hit = encode(a as u32, self.gen[a]);
            }
        }
    }

    // ── Entity pools ─────────────────────────────────────────────────────────
    // See `struct Pool`. The whole API is six calls and no policy: the pool does not decide when to
    // fire and carries no per-slot callback, because a tish closure per slot is ~151 bytes plus a
    // boxed `value_call` per retire — the cost a pool exists to remove.

    /// Create a pool of `count` entities, each with a sprite off `sheet` (or -1 for an entity-only
    /// pool). Every slot is spawned, hidden and left free. Returns the pool id, or -1 for `count<=0`.
    fn pool_new(&mut self, count: i32, sheet: i32, ox: i32, oy: i32) -> i32 {
        if count <= 0 {
            return -1;
        }
        let mut pool = Pool {
            ent: Vec::new(),
            spr: Vec::new(),
            kind: Vec::new(),
            ox,
            oy,
            live: 0,
            high: 0,
        };
        let p = self.pools.len() as i32;
        for i in 0..count {
            let e = self.spawn();
            let s = self.slot_of(e).unwrap();
            self.pool_of[s] = (p << 16) | i;
            let h = if sheet >= 0 {
                let h = tish_agb::sprite_new_typed(sheet);
                if h >= 0 {
                    tish_agb::native_sprite_set_visible(h, false);
                    self.sprite[s] = SpriteRef { handle: h, ox, oy };
                    self.mask[s] |= C_SPRITE;
                    self.used |= C_SPRITE;
                }
                h
            } else {
                -1
            };
            pool.ent.push(e);
            pool.spr.push(h);
            pool.kind.push(-1);
        }
        self.pools.push(pool);
        p
    }

    /// Arm a slot: reset it, restore the pool's sprite offset, show the sprite, store `kind`, and —
    /// when `ttl > 0` — hand retirement to `life_system`. Returns the entity id, so an arm site needs
    /// no second call before `set_transform`/`set_hurt`/…, or -1 if it could not arm.
    ///
    /// `slot >= 0` takes that exact slot and **refuses a live one**. That is a deliberate change from
    /// every hand-rolled version, which would stomp a live slot and relied on the caller checking
    /// `slotFree()` first; making refusal the engine's job deletes the guard at each site and closes
    /// the bug. A caller that means to re-arm calls `pool_retire` first.
    /// `POOL_ANY` (-1) takes the lowest free slot. `POOL_STEAL` (-2) takes the lowest free slot, or
    /// recycles the live one with the least time left.
    fn pool_arm(&mut self, p: i32, slot: i32, kind: i32, ttl: i32) -> i32 {
        let pi = match self.pools.get(p as usize) {
            Some(_) => p as usize,
            None => return -1,
        };
        let n = self.pools[pi].ent.len();
        let idx = if slot >= 0 {
            let i = slot as usize;
            if i >= n || self.pools[pi].kind[i] >= 0 {
                return -1;
            }
            i
        } else {
            match (0..n).find(|&i| self.pools[pi].kind[i] < 0) {
                Some(i) => i,
                // POOL_STEAL: no free slot, so take the live one closest to retiring. `sunny-land`'s
                // rule — an effect that steals the oldest reads better than one that does not appear.
                None if slot == -2 => {
                    let mut best = 0usize;
                    let mut best_ttl = i32::MAX;
                    for i in 0..n {
                        let t = self
                            .slot_of(self.pools[pi].ent[i])
                            .map(|s| self.life[s].ttl)
                            .unwrap_or(0);
                        if t < best_ttl {
                            best_ttl = t;
                            best = i;
                        }
                    }
                    self.pool_retire(p, best as i32);
                    best
                }
                None => return -1,
            }
        };
        let e = self.pools[pi].ent[idx];
        let s = match self.slot_of(e) {
            Some(s) => s,
            // The slot was despawned out from under the pool. Refuse rather than reset a dead entity.
            None => return -1,
        };
        self.reset_entity(e);
        let (ox, oy) = (self.pools[pi].ox, self.pools[pi].oy);
        self.sprite[s].ox = ox;
        self.sprite[s].oy = oy;
        let h = self.pools[pi].spr[idx];
        if h >= 0 {
            tish_agb::native_sprite_set_visible(h, true);
        }
        if ttl > 0 {
            self.life[s] = Life {
                ttl,
                offscreen: false,
            };
            self.mask[s] |= C_LIFE;
            self.used |= C_LIFE;
        }
        self.pools[pi].kind[idx] = kind;
        self.pools[pi].live += 1;
        if self.pools[pi].live > self.pools[pi].high {
            self.pools[pi].high = self.pools[pi].live;
        }
        e
    }

    /// Park a slot: reset the entity, hide its sprite, mark the slot free. Never despawns — the
    /// entity and its sprite VRAM outlive every arm/retire cycle, which is the point.
    fn pool_retire(&mut self, p: i32, slot: i32) {
        let pi = p as usize;
        let i = slot as usize;
        if pi >= self.pools.len() || i >= self.pools[pi].ent.len() || self.pools[pi].kind[i] < 0 {
            return;
        }
        let e = self.pools[pi].ent[i];
        self.reset_entity(e);
        if let Some(s) = self.slot_of(e) {
            self.mask[s] &= !C_LIFE;
        }
        if self.pools[pi].spr[i] >= 0 {
            tish_agb::native_sprite_set_visible(self.pools[pi].spr[i], false);
        }
        self.pools[pi].kind[i] = -1;
        self.pools[pi].live -= 1;
    }

    /// `life_system`'s hook. Takes the packed `pool_of` value so the retire path costs one unpack
    /// rather than a search.
    fn pool_retire_packed(&mut self, packed: i32) {
        self.pool_retire(packed >> 16, packed & 0xffff);
    }

    /// Retire every live slot. One call instead of the caller's ten, which matters on a room change.
    fn pool_clear(&mut self, p: i32) {
        let pi = p as usize;
        if pi >= self.pools.len() {
            return;
        }
        for i in 0..self.pools[pi].ent.len() as i32 {
            self.pool_retire(p, i);
        }
    }

    /// One slot's field: 0 kind · 1 ttl · 2 entity · 3 sprite. A selector rather than four
    /// accessors, matching `fx_set(id, field, value)` — every export is boot heap for every ROM that
    /// imports the engine, so four names for four integers is four names too many.
    fn pool_get(&self, p: i32, slot: i32, field: i32) -> i32 {
        let pi = p as usize;
        let i = slot as usize;
        if pi >= self.pools.len() || i >= self.pools[pi].ent.len() {
            return -1;
        }
        match field {
            0 => self.pools[pi].kind[i],
            1 => self
                .slot_of(self.pools[pi].ent[i])
                .map(|s| self.life[s].ttl)
                .unwrap_or(0),
            2 => self.pools[pi].ent[i],
            3 => self.pools[pi].spr[i],
            _ => -1,
        }
    }

    /// One pool's counter: 0 count · 1 live · 2 high-water.
    fn pool_stat(&self, p: i32, field: i32) -> i32 {
        let pi = p as usize;
        if pi >= self.pools.len() {
            return -1;
        }
        match field {
            0 => self.pools[pi].ent.len() as i32,
            1 => self.pools[pi].live,
            2 => self.pools[pi].high,
            _ => -1,
        }
    }

    /// Tear down the current scene: despawn every live entity and reset the grid, so a
    /// fresh scene can be built. Component definitions (from `mount`) survive; the caller
    /// resets tish-agb's sprite/background arenas (`sprite_clear` / `bg_clear`).
    fn clear_world(&mut self) {
        // S1: a fresh scene starts with no components attached anywhere.
        self.used = 0;
        self.used2 = 0;
        for s in 0..self.alive.len() {
            if self.alive[s] {
                if self.any(s, C_SPRITE) && self.sprite[s].handle >= 0 {
                    tish_agb::native_sprite_destroy(self.sprite[s].handle);
                    self.sprite[s].handle = -1;
                }
                self.alive[s] = false;
                self.mask[s] = 0;
                self.behaviour[s] = None;
                self.pool_of[s] = -1;
                self.gen[s] = self.gen[s].wrapping_add(1);
                self.free.push(s as u32);
            }
        }
        // Every pooled entity was just despawned above (which freed its sprite), so the pools now
        // describe nothing. Held pool ids become invalid; games rebuild after a scene load, which is
        // what `castBuild`/`weaponsBuild` already do at the same point in the frame.
        self.pools.clear();
        self.grid_cols = 0;
        self.grid_rows = 0;
        self.grid_cells = 0;
        // Keep bitplane capacity — grow-only across scene loads (see grid_setup).
        self.solid.clear();
        self.oneway.clear();
        self.ladder.clear();
        // ⚠️ The surface plane is scene state and MUST go with the scene. Left behind it is worse
        // than a leak: the next map inherits the last one's classes, so a fairway sits on the
        // previous hole's sand and the ball dies on grass that looks perfectly green.
        self.surface.clear();
    }

    fn has(&self, s: usize, bits: u32) -> bool {
        self.mask[s] & bits == bits
    }

    fn any(&self, s: usize, bits: u32) -> bool {
        (self.mask[s] & bits) != 0
    }

    /// Movement system (pure Rust): integrate `Body` into `Transform`.
    fn movement_system(&mut self) {
        for s in 0..self.alive.len() {
            // ⚠️ A RIGID DISC IS NOT MOVED HERE. `dynamic_system` owns its integration, because that
            // is where the tile bounce, the substepping and the sleep state live. Moved in both
            // places a disc travels at DOUBLE its velocity, and — far worse — half of that travel
            // is this unchecked `+=`, which walks it straight through a solid tile. It presented as
            // a football drifting out of the stadium a fraction of a pixel at a time with its
            // velocity reading zero, and it survived three wrong fixes aimed at the sleep logic
            // before anyone looked at what else writes a transform.
            if self.alive[s]
                && self.mask[s] & C_SLEEP == 0
                && self.has(s, C_TRANSFORM | C_BODY)
                && !self.any(s, C_DYNAMIC)
            {
                self.transform[s].x += self.body[s].vx;
                self.transform[s].y += self.body[s].vy;
            }
        }
    }

    /// Turn the screen into a torus: anything that leaves an edge re-enters the opposite one. This
    /// is the Asteroids rule, and it is a WORLD flag rather than a per-entity component because in
    /// such a game it holds for everything at once — ship, rocks, saucer and shots alike — and the
    /// shots are spawned natively by `fire_bullet`, where there is no tish-side handle to flag.
    ///
    /// A per-frame tish loop cannot do this: 20-odd rocks × (read x, read y, write transform) is
    /// several thousand ticks of call overhead, most of a 60fps frame, for arithmetic that costs
    /// nothing here.
    fn wrap_system(&mut self) {
        if !self.arena_wrap {
            return;
        }
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_TRANSFORM) {
                continue;
            }
            // The valid range for the top-left corner is [-w, 240) — i.e. an entity is teleported
            // only once it is FULLY past the edge, and lands exactly one span over, fully past the
            // opposite edge. So it slides off and back on with no gap and no visible jump, and the
            // wrap costs it not one frame of being invisible.
            let (w, h) = if self.has(s, C_COLLIDER) {
                (self.collider[s].w, self.collider[s].h)
            } else {
                (Fixed::from_raw(TILE << 8), Fixed::from_raw(TILE << 8))
            };
            let span_x = Fixed::from_raw(240 << 8) + w;
            let span_y = Fixed::from_raw(160 << 8) + h;
            let t = &mut self.transform[s];
            // `while`, not `if`: a teleport (a fresh spawn, a scene load) can leave something more
            // than one span out, and a single subtraction would strand it off-screen forever.
            while t.x >= Fixed::from_raw(240 << 8) {
                t.x -= span_x;
            }
            while t.x < -w {
                t.x += span_x;
            }
            while t.y >= Fixed::from_raw(160 << 8) {
                t.y -= span_y;
            }
            while t.y < -h {
                t.y += span_y;
            }
        }
    }

    /// Turn the wrap-around arena on or off for the whole world.
    fn set_arena_wrap(&mut self, on: bool) {
        self.arena_wrap = on;
    }

    // ── Side-scrolling platformer genre ──────────────────────────────────────
    /// Give an entity platformer physics (gravity + tile collision). It also needs a
    /// `Collider` (its hitbox) and a `Transform`; drive it with `platformer_walk`/`_jump`.
    fn set_platformer(&mut self, e: i32) {
        if let Some(s) = self.slot_of(e) {
            self.platformer[s] = Platformer {
                face: 1,
                ..Platformer::default()
            };
            self.mask[s] |= C_PLATFORMER;
            self.used |= C_PLATFORMER;
        }
    }

    /// Set the horizontal move intent this frame (dir < 0 left, 0 stop, > 0 right). Call every
    /// frame from input — 0 stops immediately (arcade feel). The system applies it at walk or run
    /// speed. No-op while a room slide has input locked.
    fn platformer_walk(&mut self, e: i32, dir: i32) {
        if self.input_locked(e) {
            return;
        }
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            let d = dir.signum();
            self.platformer[s].dir = d;
            // Only a real move updates facing — releasing the d-pad keeps you looking where you were.
            if d != 0 {
                self.platformer[s].face = d;
            }
        }
    }

    /// Set run mode for this frame (hold the run button). Off = walk speed. Call every frame.
    fn platformer_run(&mut self, e: i32, on: bool) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            self.platformer[s].run = on && !self.input_locked(e);
        }
    }

    /// Buffer a jump press (edge-trigger this from the player). The system fires it if the entity
    /// is grounded OR within its coyote-time window, so a press just before landing or just after
    /// a ledge still jumps. No-op while a room slide has input locked.
    fn platformer_jump(&mut self, e: i32) {
        if self.input_locked(e) {
            return;
        }
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            self.platformer[s].jump_buffer = P_JUMP_BUFFER;
        }
    }

    /// Release the jump button: if still rising from a jump, cut the upward velocity so a short
    /// tap makes a short hop (variable jump height). Call on the button-release edge.
    fn platformer_jump_release(&mut self, e: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            let p = &mut self.platformer[s];
            if p.jumping && p.vy.to_raw() < 0 {
                p.vy = Fixed::from_raw(p.vy.to_raw() / 2);
                p.jumping = false;
            }
        }
    }

    /// Request dropping through a one-way platform (hold Down + jump). Opens a short window in
    /// which one-way floors are ignored so the box falls through.
    fn platformer_drop(&mut self, e: i32) {
        if self.input_locked(e) {
            return;
        }
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            self.platformer[s].drop = P_DROP;
        }
    }

    /// Launch the entity upward at `vel` px/frame (stomp bounce, springs, knockback). Clears
    /// grounded so gravity takes over from the apex.
    fn platformer_bounce(&mut self, e: i32, vel: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            self.platformer[s].vy = Fixed::from_raw(-vel.abs() * 256);
            self.platformer[s].grounded = false;
            self.platformer[s].jumping = false;
        }
    }

    /// Set vertical velocity outright, in px/frame (negative rises, positive falls). `bounce` can
    /// only ever launch UPWARD (`-vel.abs()`), which is right for a stomp or a spring but cannot
    /// express the other half of the vocabulary: clamping a fall to a slow scrape down a wall,
    /// an updraft, a downward slam. This is the general form.
    fn platformer_set_vy(&mut self, e: i32, vy_raw: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            self.platformer[s].vy = Fixed::from_raw(vy_raw);
            // A jump that is being overridden is over — otherwise a later `jump_release` would try
            // to halve a velocity this call chose deliberately.
            self.platformer[s].jumping = false;
        }
    }

    /// Set this body's ground speeds, in Fixed raw. Either may be 0 to keep the engine default.
    fn platformer_set_speed(&mut self, e: i32, walk_raw: i32, run_raw: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            self.platformer[s].walk_raw = walk_raw;
            self.platformer[s].run_raw = run_raw;
        }
    }

    /// Per-entity jump impulse / gravity in Fixed raw; 0 keeps the engine default (P_JUMP /
    /// P_GRAVITY) — the same zero-means-default contract as `platformer_set_speed`, and for the
    /// same reason: held-weight physics belongs to one body, not to every game sharing the engine.
    fn platformer_set_physics(&mut self, e: i32, jump_raw: i32, grav_raw: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            self.platformer[s].jump_raw = jump_raw;
            self.platformer[s].grav_raw = grav_raw;
        }
    }

    /// Launch a platformer body on a throw arc: a persistent horizontal velocity (clears itself on
    /// landing or on hitting a wall — see `launch_raw`) plus an immediate vertical velocity.
    /// Un-holds and un-rides: a thrown body is airborne by definition.
    fn platformer_launch(&mut self, e: i32, vx_raw: i32, vy_raw: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_PLATFORMER)) {
            let p = &mut self.platformer[s];
            p.launch_raw = vx_raw;
            p.vy = Fixed::from_raw(vy_raw);
            p.held = false;
            p.riding = false;
            p.grounded = false;
        }
    }

    /// Is player input frozen because the camera target is mid room-slide?
    fn input_locked(&self, e: i32) -> bool {
        self.room_cam.enabled && self.room_cam.transitioning && self.camera_target == Some(e)
    }

    /// Does the AABB (top-left `x,y`, size `w×h`) overlap any solid tile? Uses inclusive
    /// pixel bounds so a box flush against a wall doesn't read the next tile as a hit.
    fn box_hits_solid(&self, x: Fixed, y: Fixed, w: Fixed, h: Fixed) -> bool {
        let left = x.floor();
        let top = y.floor();
        let right = ((x + w).to_raw() - 1) >> 8;
        let bottom = ((y + h).to_raw() - 1) >> 8;
        if right < left || bottom < top {
            return false;
        }
        let (c0, c1) = (left.div_euclid(TILE), right.div_euclid(TILE));
        let (r0, r1) = (top.div_euclid(TILE), bottom.div_euclid(TILE));
        let mut r = r0;
        while r <= r1 {
            let mut c = c0;
            while c <= c1 {
                if self.is_solid(c, r) {
                    return true;
                }
                c += 1;
            }
            r += 1;
        }
        false
    }

    /// Is a floor tile directly beneath the box's bottom edge — i.e., is the box resting on ground?
    /// Tests the tile ROW JUST BELOW the box, unlike `box_hits_solid` which tests the box's own
    /// cells: a box clamped exactly onto a tile top has no solid *inside* it, so without this it would
    /// micro-fall and re-land every few frames (flickering `grounded` and any grounded-driven
    /// animation). `oneway` counts one-way platforms as floor — pass false while dropping through one.
    fn on_floor(&self, x: Fixed, y: Fixed, w: Fixed, h: Fixed, oneway: bool) -> bool {
        let row = (y + h).floor().div_euclid(TILE);
        let c0 = x.floor().div_euclid(TILE);
        let c1 = (x + w).floor().saturating_sub(1).div_euclid(TILE);
        let mut c = c0;
        while c <= c1 {
            if self.is_solid(c, row) || (oneway && self.is_oneway(c, row)) {
                return true;
            }
            c += 1;
        }
        false
    }

    /// If the box's bottom crosses a one-way platform's top this move (landing on it from above),
    /// return the tile row to rest on; else `None`. One-way tiles only block downward from above.
    fn oneway_floor(
        &self,
        x: Fixed,
        prev_y: Fixed,
        new_y: Fixed,
        w: Fixed,
        h: Fixed,
    ) -> Option<i32> {
        let prev_bottom = (prev_y + h).floor();
        let new_bottom = (new_y + h).floor();
        let row = (new_bottom - 1).div_euclid(TILE);
        let top = row * TILE;
        if prev_bottom <= top && new_bottom > top {
            let (c0, c1) = (
                x.floor().div_euclid(TILE),
                (x + w).floor().saturating_sub(1).div_euclid(TILE),
            );
            let mut c = c0;
            while c <= c1 {
                if self.is_oneway(c, row) {
                    return Some(row);
                }
                c += 1;
            }
        }
        None
    }

    /// Platformer system: apply gravity, then move + resolve each platformer entity's box
    /// against the solid grid axis-by-axis (X then Y), snapping to the tile edge on a hit and
    /// setting `grounded` when it lands. Also handles game feel — coyote time, jump buffering,
    /// variable jump height, and one-way platforms. Speeds stay below `TILE`, so a single-step
    /// clamp per axis is exact. The camera target is skipped while a room slide freezes it.
    fn platformer_system(&mut self) {
        // Gather this frame's carriers once (a handful at most; fixed cap, no per-frame alloc —
        // extras past the cap simply act as plain entities this frame).
        let mut carriers = [0usize; 16];
        let mut ncar = 0usize;
        for s in 0..self.alive.len() {
            if self.alive[s]
                && self.mask2[s] & M2_CARRIER != 0
                && self.has(s, C_TRANSFORM | C_COLLIDER)
                && ncar < carriers.len()
            {
                carriers[ncar] = s;
                ncar += 1;
            }
        }
        let frozen = if self.room_cam.enabled && self.room_cam.transitioning {
            self.camera_target.and_then(|e| self.slot_of(e))
        } else {
            None
        };
        // Pass 1: carriers that are themselves platformer bodies (a walking beast) integrate FIRST,
        // so a rider always resolves against its carrier's FINAL position this frame — slot order
        // stops mattering. Body/mover carriers (a drifting raft) already moved in movement_system.
        for i in 0..ncar {
            let s = carriers[i];
            if !self.has(s, C_PLATFORMER) || Some(s) == frozen || !self.is_active(s) {
                continue;
            }
            self.platformer_integrate(s, &carriers[..ncar]);
        }
        // Pass 2: everyone else (riders included). Carriers are skipped: a platformer carrier ran
        // in pass 1, and a carrier cannot itself ride another carrier.
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_PLATFORMER | C_TRANSFORM | C_COLLIDER) {
                continue;
            }
            if self.mask2[s] & M2_CARRIER != 0 {
                continue;
            }
            if Some(s) == frozen || !self.is_active(s) {
                continue; // frozen mid room-slide, or culled off-screen (freeze until it scrolls back)
            }
            self.platformer_integrate(s, &carriers[..ncar]);
        }
        // Snapshot carrier positions: next frame's rider delta is measured from here. Done for
        // every carrier (even culled ones) so a carrier scrolling back on screen has no stale
        // multi-frame delta waiting to yank its rider.
        for &s in carriers.iter().take(ncar) {
            self.carr_prev[s] = (self.transform[s].x, self.transform[s].y);
        }
    }

    /// One platformer body's integration for this frame: gravity, jump/coyote/buffer game feel,
    /// axis-by-axis solid-grid resolution, one-way platforms, and carrier ride/landing. Extracted
    /// from `platformer_system` so pass 1 (carriers) and pass 2 (everyone else) share it exactly.
    fn platformer_integrate(&mut self, s: usize, carriers: &[usize]) {
        let term = Fixed::from_raw(P_TERMINAL);
        let zero = Fixed::from_raw(0);
        if self.platformer[s].held {
            // Held in place (e.g. hanging on a ledge grab): no gravity, no movement. The game
            // logic decides when to release (drop → fall) or climb (teleport up + release). Also
            // CLEAR any pending jump — otherwise a jump buffered just before the grab survives the
            // (skipped) physics and fires the instant a climb sets the body back down, launching it.
            let p = &mut self.platformer[s];
            p.vx = Fixed::from_raw(0);
            p.vy = Fixed::from_raw(0);
            p.grounded = false;
            p.blocked = false;
            p.jump_buffer = 0;
            p.coyote = 0;
            p.jumping = false;
            p.riding = false;
            p.launch_raw = 0;
            return;
        }
        // Rider pre-step: inherit the carrier's motion since last frame (horizontal AND vertical,
        // so a lift or a bobbing raft carries its rider) — or detach if the carrier despawned,
        // left the room, or stopped being a carrier. Generation-checked ids make the despawn case
        // a clean resolve failure, never a stale slot.
        if self.platformer[s].riding {
            let car = self.platformer[s].carrier;
            match self
                .slot_of(car)
                .filter(|&c| self.mask2[c] & M2_CARRIER != 0 && self.same_room(s, c))
            {
                Some(c) => {
                    let (px, py) = self.carr_prev[c];
                    let dx = self.transform[c].x - px;
                    let dy = self.transform[c].y - py;
                    self.transform[s].x += dx;
                    self.transform[s].y += dy;
                }
                None => self.platformer[s].riding = false,
            }
        }
        let (w, h) = (self.collider[s].w, self.collider[s].h);
        let p = self.platformer[s];
        // Zero-means-default per-entity gravity/jump (`platformer_set_physics`): held-weight
        // physics — a body carrying something heavy jumps lower, something buoyant falls slower.
        let g = Fixed::from_raw(if p.grav_raw != 0 {
            p.grav_raw
        } else {
            P_GRAVITY
        });
        let was_grounded = p.grounded;
        let mut coyote = if was_grounded {
            P_COYOTE
        } else {
            (p.coyote - 1).max(0)
        };
        let mut jump_buffer = p.jump_buffer;
        let mut jumping = p.jumping;
        let speed = if p.run {
            if p.run_raw != 0 {
                p.run_raw
            } else {
                P_RUN
            }
        } else if p.walk_raw != 0 {
            p.walk_raw
        } else {
            P_WALK
        };
        // A launch (`platformer_launch` — a thrown body's arc) replaces the dir*speed walk
        // velocity until it lands or hits a wall; a plain platformer body has no persistent vx.
        let mut launch = p.launch_raw;
        let mut vx = if launch != 0 {
            Fixed::from_raw(launch)
        } else {
            Fixed::from_raw(p.dir.signum() * speed)
        };
        let mut vy = p.vy;
        // Fire a buffered jump if grounded or still within the coyote window.
        if jump_buffer > 0 && (was_grounded || coyote > 0) {
            vy = Fixed::from_raw(-(if p.jump_raw != 0 { p.jump_raw } else { P_JUMP }));
            jumping = true;
            jump_buffer = 0;
            coyote = 0;
        }
        jump_buffer = (jump_buffer - 1).max(0);
        let drop = (p.drop - 1).max(0);
        // Gravity; once falling, the jump is over (later releases can't cut it).
        vy += g;
        if vy > term {
            vy = term;
        }
        if vy.to_raw() >= 0 {
            jumping = false;
        }
        // X axis: move, then clamp against the leading vertical edge if it hit a wall.
        let (x, y) = (self.transform[s].x, self.transform[s].y);
        let nx = x + vx;
        let rx = if vx > zero && self.box_hits_solid(nx, y, w, h) {
            let col = last_cell(nx + w);
            vx = zero;
            launch = 0;
            Fixed::from_raw(col * TILE * 256) - w
        } else if vx < zero && self.box_hits_solid(nx, y, w, h) {
            let col = nx.floor().div_euclid(TILE);
            vx = zero;
            launch = 0;
            Fixed::from_raw((col + 1) * TILE * 256)
        } else {
            nx
        };
        // Y axis: floor/ceiling against solids, one-way platforms, then carriers when falling.
        let ny = y + vy;
        let mut grounded = false;
        let mut riding = false;
        let mut carrier_id = p.carrier;
        let mut ry = if vy > zero {
            if self.box_hits_solid(rx, ny, w, h) {
                let row = last_cell(ny + h);
                vy = zero;
                grounded = true;
                Fixed::from_raw(row * TILE * 256) - h
            } else if drop == 0 {
                match self.oneway_floor(rx, y, ny, w, h) {
                    Some(row) => {
                        vy = zero;
                        grounded = true;
                        Fixed::from_raw(row * TILE * 256) - h
                    }
                    // A carrier's top edge is one-way moving ground. Down+jump (`drop`) falls
                    // through it exactly like a one-way tile. A flush rider re-lands here every
                    // frame (gravity pulls its bottom across the carrier top), which is what keeps
                    // `grounded` stable while riding — the carrier analogue of `on_floor`.
                    None => match self.carrier_land(s, carriers, rx, y, ny, w, h) {
                        Some((top, id)) => {
                            vy = zero;
                            grounded = true;
                            riding = true;
                            carrier_id = id;
                            top - h
                        }
                        None => ny,
                    },
                }
            } else {
                ny
            }
        } else if vy < zero && self.box_hits_solid(rx, ny, w, h) {
            let row = ny.floor().div_euclid(TILE);
            vy = zero;
            Fixed::from_raw((row + 1) * TILE * 256)
        } else {
            ny
        };
        // Stable ground contact: if the box came to rest exactly on a tile top (or is falling the
        // last sub-pixel onto one), `box_hits_solid` misses it, so probe the row beneath. Keeps a
        // resting entity `grounded` every frame and snaps it flush to the tile — no landing
        // jitter, no idle/jump animation flicker. Skipped while moving upward or dropping through.
        if !grounded && vy >= zero && self.on_floor(rx, ry, w, h, drop == 0) {
            grounded = true;
            vy = zero;
            let floor_row = (ry + h).floor().div_euclid(TILE);
            ry = Fixed::from_raw(floor_row * TILE * 256) - h;
        }
        if grounded {
            launch = 0; // a throw arc ends on landing (tile, one-way or carrier alike)
        }
        self.transform[s].x = rx;
        self.transform[s].y = ry;
        // "blocked" = had horizontal intent but a wall zeroed the move (patrol AI turns on this).
        let blocked = p.dir != 0 && vx.to_raw() == 0;
        let p = &mut self.platformer[s];
        p.vx = vx;
        p.vy = vy;
        p.grounded = grounded;
        p.coyote = coyote;
        p.jump_buffer = jump_buffer;
        p.jumping = jumping;
        p.drop = drop;
        p.blocked = blocked;
        p.launch_raw = launch;
        p.riding = riding;
        p.carrier = carrier_id;
    }

    /// If the box's bottom crosses a carrier's top edge this move (landing on it from above),
    /// return `(top, carrier id)` for the highest such carrier; else None. The 4px upward
    /// tolerance lets a RISING carrier catch a body it lifted into (the raft coming up under a
    /// standing rider) without being large enough to teleport a body that walked into its side.
    fn carrier_land(
        &self,
        s: usize,
        carriers: &[usize],
        x: Fixed,
        prev_y: Fixed,
        new_y: Fixed,
        w: Fixed,
        h: Fixed,
    ) -> Option<(Fixed, i32)> {
        let prev_bottom = prev_y + h;
        let new_bottom = new_y + h;
        let tol = Fixed::from_raw(4 * 256);
        let mut best: Option<(Fixed, i32)> = None;
        for &c in carriers {
            if c == s || !self.alive[c] || !self.same_room(s, c) {
                continue;
            }
            let cx = self.transform[c].x;
            let cw = self.collider[c].w;
            if x + w <= cx || x >= cx + cw {
                continue;
            }
            let top = self.transform[c].y;
            if prev_bottom <= top + tol && new_bottom > top {
                let better = match best {
                    Some((bt, _)) => top < bt,
                    None => true,
                };
                if better {
                    best = Some((top, encode(c as u32, self.gen[c])));
                }
            }
        }
        best
    }

    // ── Free top-down (action-RPG) genre ────────────────────────────────────
    /// Give an entity free 8-directional top-down movement with solid-tile collision. It also needs
    /// a `Collider` (its hitbox) and a `Transform`; drive it with `topdown_move` each frame.
    fn set_topdown(&mut self, e: i32) {
        if let Some(s) = self.slot_of(e) {
            self.topdown[s] = TopDown {
                speed: TD_WALK,
                ..TopDown::default()
            };
            self.mask[s] |= C_TOPDOWN;
            self.used |= C_TOPDOWN;
        }
    }

    /// Mark an entity's collider as a solid blocker for top-down movers (an NPC the player can't
    /// walk through). Needs `C_COLLIDER` + `C_TRANSFORM`; the box travels with the entity.
    fn set_blocker(&mut self, e: i32) {
        if let Some(s) = self
            .slot_of(e)
            .filter(|&s| self.has(s, C_TRANSFORM | C_COLLIDER))
        {
            self.mask[s] |= C_BLOCKER;
            self.used |= C_BLOCKER;
        }
    }

    /// Make this entity's top edge one-way moving ground for platformer bodies (`M2_CARRIER`):
    /// stand on a walking beast or a drifting raft, inherit its motion, jump off normally, and
    /// Down+jump drops through it like a one-way tile. Needs `C_TRANSFORM` + `C_COLLIDER`; the
    /// surface travels with the entity. Carriers are ground, not walls — one moving sideways INTO
    /// a standing body pushes nothing — and a carrier cannot itself ride another carrier.
    fn set_carrier(&mut self, e: i32) {
        if let Some(s) = self
            .slot_of(e)
            .filter(|&s| self.has(s, C_TRANSFORM | C_COLLIDER))
        {
            self.mask2[s] |= M2_CARRIER;
            self.used2 |= M2_CARRIER;
            self.carr_prev[s] = (self.transform[s].x, self.transform[s].y);
        }
    }

    /// Would the box `(x,y,w,h)` overlap any `C_BLOCKER` other than `mover`? Returns the first hit's
    /// box so the caller can snap to its edge. Room-gated: with a room camera, only blockers in the
    /// same room count (no phantom walls through a doorway).
    fn first_blocker_hit(
        &self,
        mover: usize,
        x: Fixed,
        y: Fixed,
        w: Fixed,
        h: Fixed,
    ) -> Option<(Fixed, Fixed, Fixed, Fixed)> {
        for b in 0..self.alive.len() {
            if b == mover
                || !self.alive[b]
                || !self.has(b, C_BLOCKER | C_TRANSFORM | C_COLLIDER)
                || !self.is_active(b)
            {
                continue;
            }
            if !self.same_room(mover, b) {
                continue;
            }
            let bx = self.transform[b].x;
            let by = self.transform[b].y;
            let bw = self.collider[b].w;
            let bh = self.collider[b].h;
            if x < bx + bw && bx < x + w && y < by + bh && by < y + h {
                return Some((bx, by, bw, bh));
            }
        }
        None
    }

    /// Give a top-down entity native chase-the-player AI (no per-frame tish `tick`). `stride` = the
    /// character sheet's columns per direction row (5 for idle+4walk), or 0 for a non-directional
    /// creature that just loops frames `0..flap`.
    /// Make `e` shoot every `interval` frames at `speed` px/frame, using the bullet style that is
    /// in force RIGHT NOW (so the caller sets the style once, at spawn, and never again).
    fn set_shooter(&mut self, e: i32, interval: i32, speed: Fixed, aimed: bool) {
        let style = self.bullet_style;
        if let Some(s) = self.slot_of(e) {
            // Stagger the first shot so a room of them does not volley in lockstep.
            self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let jitter = ((self.rng >> 16) as i32).rem_euclid(interval.max(1));
            self.shooter[s] = Shooter {
                interval: interval.max(1),
                timer: jitter,
                speed,
                aimed,
                style,
            };
            self.mask[s] |= C_SHOOTER;
            self.used |= C_SHOOTER;
        }
    }

    /// Native shooter AI: no per-frame tish call, and it obeys the same room cutoff and stun the
    /// other AI systems do — a stunned enemy must not keep firing.
    ///
    /// Shots are collected first and fired after the scan, because firing spawns entities and that
    /// would reallocate the very columns being walked.
    fn shooter_system(&mut self) {
        let Some(target) = self.camera_target else {
            return;
        };
        let Some(ts) = self.slot_of(target) else {
            return;
        };
        let (ptx, pty) = self.center_of(ts);
        // A fixed-size buffer rather than a Vec: this runs every frame and must not allocate.
        // Anything past the cap simply fires on its next tick.
        let mut shots: [(Fixed, Fixed, Fixed, Fixed, BulletStyle); 8] = [(
            Fixed::from_raw(0),
            Fixed::from_raw(0),
            Fixed::from_raw(0),
            Fixed::from_raw(0),
            BulletStyle::default(),
        ); 8];
        let mut n = 0usize;
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_SHOOTER | C_TRANSFORM) || !self.is_active(s) {
                continue;
            }
            if self.stun[s] > 0 || !self.same_room(s, ts) {
                continue;
            }
            self.shooter[s].timer -= 1;
            if self.shooter[s].timer > 0 {
                continue;
            }
            let sh = self.shooter[s];
            self.shooter[s].timer = sh.interval;
            if n == shots.len() {
                continue;
            }
            // From the shooter's box CENTRE, not its transform. `fire_bullet` reads the point it is
            // given as the bullet's centre, so passing the top-left put every shot half a box up and
            // left of the enemy — usually inside the neighbouring tile, where the projectile's
            // solid-tile check retired it on the frame it spawned. 20 of every 21 shots died there.
            let (cx, cy) = self.center_of(s);
            // Unit heading: at the player for an aimed shooter, along its facing otherwise (which is
            // what makes a spitter something you dodge by staying out of its lane).
            let (ux, uy) = if sh.aimed {
                let (dx, dy) = (ptx - cx, pty - cy);
                let len = (dx * dx + dy * dy).sqrt();
                if len.to_raw() == 0 {
                    (Fixed::from_raw(0), Fixed::from_raw(256))
                } else {
                    (dx / len, dy / len)
                }
            } else {
                match self.topdown[s].facing {
                    1 => (Fixed::from_raw(0), Fixed::from_raw(-256)),
                    2 => (Fixed::from_raw(-256), Fixed::from_raw(0)),
                    3 => (Fixed::from_raw(256), Fixed::from_raw(0)),
                    _ => (Fixed::from_raw(0), Fixed::from_raw(256)),
                }
            };
            // Start it clear of the shooter's own body for the same reason.
            let muzzle = Fixed::from_raw(12 * 256);
            shots[n] = (
                cx + ux * muzzle,
                cy + uy * muzzle,
                ux * sh.speed,
                uy * sh.speed,
                sh.style,
            );
            n += 1;
        }
        for &(x, y, vx, vy, style) in shots.iter().take(n) {
            let save = self.bullet_style;
            self.bullet_style = style;
            self.fire_bullet(x, y, vx, vy);
            self.bullet_style = save;
        }
    }

    /// The centre of slot `s`'s collider box in world px (its transform if it has no collider).
    fn center_of(&self, s: usize) -> (Fixed, Fixed) {
        if self.has(s, C_COLLIDER) {
            (
                self.transform[s].x + self.collider[s].w / 2,
                self.transform[s].y + self.collider[s].h / 2,
            )
        } else {
            (self.transform[s].x, self.transform[s].y)
        }
    }

    /// Give `e` a charge: while it lines up with the player on an axis (within `band` px on the
    /// other one) it bolts along that axis at `speed` px/frame instead of doing whatever it
    /// normally does.
    fn set_charger(&mut self, e: i32, speed: i32, band: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TOPDOWN)) {
            self.charger[s] = Charger {
                speed: speed.max(1) * 256,
                base: self.topdown[s].speed,
                band: band.max(1),
                active: false,
            };
            self.mask[s] |= C_CHARGER;
            self.used |= C_CHARGER;
        }
    }

    /// Runs AFTER the hopper/chase systems so a charge overrides the ordinary move intent, and
    /// hands the speed back the moment the line is broken.
    fn charger_system(&mut self) {
        let Some(target) = self.camera_target else {
            return;
        };
        let Some(ts) = self.slot_of(target) else {
            return;
        };
        let (px, py) = (self.transform[ts].x.floor(), self.transform[ts].y.floor());
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_CHARGER | C_TOPDOWN) || !self.is_active(s) {
                continue;
            }
            let c = self.charger[s];
            let lined = if self.stun[s] > 0 || !self.same_room(s, ts) {
                None
            } else {
                let dx = px - self.transform[s].x.floor();
                let dy = py - self.transform[s].y.floor();
                if dy.abs() <= c.band && dx != 0 {
                    Some((dx.signum(), 0))
                } else if dx.abs() <= c.band && dy != 0 {
                    Some((0, dy.signum()))
                } else {
                    None
                }
            };
            match lined {
                Some((mx, my)) => {
                    if !self.charger[s].active {
                        self.charger[s].active = true;
                        self.charger[s].base = self.topdown[s].speed;
                        self.topdown[s].speed = c.speed;
                    }
                    self.topdown[s].dx = mx;
                    self.topdown[s].dy = my;
                    self.topdown[s].facing = if mx < 0 {
                        2
                    } else if mx > 0 {
                        3
                    } else if my < 0 {
                        1
                    } else {
                        0
                    };
                }
                None => {
                    if self.charger[s].active {
                        self.charger[s].active = false;
                        self.topdown[s].speed = c.base;
                    }
                }
            }
        }
    }

    /// `set_weakness(e, mask)` — damage-type vulnerability mask (`DMG_*` bits). 0 = hurt by
    /// everything; non-zero ALLOWS only matching weapon kinds. Complement of `set_immunity`.
    fn set_weakness(&mut self, e: i32, mask: i32) {
        if let Some(s) = self.slot_of(e) {
            self.weak[s] = mask;
        }
    }

    /// `set_grabber(e, targetTag)` — on overlap with a tagged target, briefly stun it.
    fn set_grabber(&mut self, e: i32, target_tag: i32) {
        if let Some(s) = self.slot_of(e) {
            self.grabber[s] = Grabber { target_tag };
            self.mask[s] |= C_GRABBER;
            self.used |= C_GRABBER;
        }
    }

    /// Stun any overlapping tagged target for a short grab window.
    fn grabber_system(&mut self) {
        const GRAB_STUN: i32 = 24;
        let n = self.alive.len();
        let mut stuns: Vec<(usize, i32)> = Vec::new();
        for s in 0..n {
            if !self.alive[s]
                || !self.has(s, C_GRABBER | C_TRANSFORM | C_COLLIDER)
                || !self.is_active(s)
            {
                continue;
            }
            let want = self.grabber[s].target_tag;
            for t in 0..n {
                if t == s || !self.alive[t] || self.tag[t] != want {
                    continue;
                }
                if !self.has(t, C_TRANSFORM | C_COLLIDER) || !self.slots_overlap(s, t) {
                    continue;
                }
                if !self.same_room(s, t) {
                    continue;
                }
                stuns.push((t, GRAB_STUN));
            }
        }
        for (t, frames) in stuns {
            self.stun[t] = self.stun[t].max(frames);
        }
    }

    /// `set_trap(e)` — blade-trap: inert until the player shares a row/col, then dash.
    fn set_trap(&mut self, e: i32) {
        if let Some(s) = self
            .slot_of(e)
            .filter(|&s| self.has(s, C_TOPDOWN | C_TRANSFORM))
        {
            let base = self.topdown[s].speed;
            self.trap[s] = Trap {
                home_x: self.transform[s].x,
                home_y: self.transform[s].y,
                speed: 4 * 256,
                base,
                band: 8,
                active: false,
            };
            self.mask[s] |= C_TRAP;
            self.used |= C_TRAP;
        }
    }

    /// Charger-like dash while lined up with the player; otherwise hold still at home speed.
    fn trap_system(&mut self) {
        let Some(target) = self.camera_target else {
            return;
        };
        let Some(ts) = self.slot_of(target) else {
            return;
        };
        let (px, py) = (self.transform[ts].x.floor(), self.transform[ts].y.floor());
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_TRAP | C_TOPDOWN) || !self.is_active(s) {
                continue;
            }
            let c = self.trap[s];
            let lined = if self.stun[s] > 0 || !self.same_room(s, ts) {
                None
            } else {
                let dx = px - self.transform[s].x.floor();
                let dy = py - self.transform[s].y.floor();
                if dy.abs() <= c.band && dx != 0 {
                    Some((dx.signum(), 0))
                } else if dx.abs() <= c.band && dy != 0 {
                    Some((0, dy.signum()))
                } else {
                    None
                }
            };
            match lined {
                Some((mx, my)) => {
                    if !self.trap[s].active {
                        self.trap[s].active = true;
                        self.trap[s].base = self.topdown[s].speed;
                        self.topdown[s].speed = c.speed;
                    }
                    self.topdown[s].dx = mx;
                    self.topdown[s].dy = my;
                    self.topdown[s].facing = if mx < 0 {
                        2
                    } else if mx > 0 {
                        3
                    } else if my < 0 {
                        1
                    } else {
                        0
                    };
                }
                None => {
                    if self.trap[s].active {
                        self.trap[s].active = false;
                        self.topdown[s].speed = c.base;
                    }
                    // Drift back to the home corner when the line breaks.
                    let dx = c.home_x.floor() - self.transform[s].x.floor();
                    let dy = c.home_y.floor() - self.transform[s].y.floor();
                    if dx.abs() <= 1 && dy.abs() <= 1 {
                        self.transform[s].x = c.home_x;
                        self.transform[s].y = c.home_y;
                        self.topdown[s].dx = 0;
                        self.topdown[s].dy = 0;
                    } else if dx.abs() >= dy.abs() {
                        self.topdown[s].dx = dx.signum();
                        self.topdown[s].dy = 0;
                    } else {
                        self.topdown[s].dx = 0;
                        self.topdown[s].dy = dy.signum();
                    }
                }
            }
        }
    }

    fn set_follow(&mut self, e: i32, kind: u8, parent: i32, radius: i32) {
        let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TRANSFORM)) else {
            return;
        };
        let (ox, oy) = match self.slot_of(parent).filter(|&p| self.has(p, C_TRANSFORM)) {
            Some(p) => (
                self.transform[s].x - self.transform[p].x,
                self.transform[s].y - self.transform[p].y,
            ),
            None => (Fixed::from_raw(0), Fixed::from_raw(0)),
        };
        self.follow[s] = Follow {
            kind,
            parent,
            radius: radius.max(0),
            ox,
            oy,
            angle: 0,
        };
        self.mask[s] |= C_FOLLOW;
        self.used |= C_FOLLOW;
    }

    /// Glue `C_FOLLOW` entities to their parent each frame (offset follow / orbit).
    fn follow_system(&mut self) {
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask[s] & C_SLEEP != 0 || !self.has(s, C_FOLLOW | C_TRANSFORM)
            {
                continue;
            }
            let f = self.follow[s];
            let Some(p) = self
                .slot_of(f.parent)
                .filter(|&p| self.alive[p] && self.has(p, C_TRANSFORM))
            else {
                continue;
            };
            let (px, py) = (self.transform[p].x, self.transform[p].y);
            match f.kind {
                FOLLOW_ORBIT => {
                    let rev = Fixed::from_raw(f.angle & 255);
                    let r = Fixed::from_raw(f.radius * 256);
                    self.transform[s].x = px + rev.cos() * r;
                    self.transform[s].y = py + rev.sin() * r;
                    self.follow[s].angle = f.angle.wrapping_add(2);
                }
                _ => {
                    // part + train: simple offset follow
                    self.transform[s].x = px + f.ox;
                    self.transform[s].y = py + f.oy;
                }
            }
        }
    }

    /// `set_boomerang(e, returnFrames)` — after N frames, reverse Body velocity toward the owner
    /// (camera target at configure time). A return-mover companion to `set_mover` / `set_lifetime`.
    fn set_boomerang(&mut self, e: i32, return_frames: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_BODY)) {
            let owner = self.camera_target.unwrap_or(-1);
            self.boomerang[s] = Boomerang {
                timer: return_frames.max(1),
                owner,
                returning: false,
            };
            self.mask[s] |= C_BOOMERANG;
            self.used |= C_BOOMERANG;
        }
    }

    fn boomerang_system(&mut self) {
        // Catches collected first, despawned after the scan (despawning mid-walk would mutate the
        // very columns being iterated). A tiny fixed buffer: more than 4 live boomerangs is not a
        // thing a top-down action-RPG frame does.
        let mut caught: [i32; 4] = [-1; 4];
        let mut nc = 0usize;
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_BOOMERANG | C_BODY) || !self.is_active(s) {
                continue;
            }
            if !self.boomerang[s].returning {
                self.boomerang[s].timer -= 1;
                if self.boomerang[s].timer > 0 {
                    continue;
                }
                self.boomerang[s].returning = true;
                // Flip the outbound velocity as a first approximation; the seek below retargets.
                self.body[s].vx = -self.body[s].vx;
                self.body[s].vy = -self.body[s].vy;
            }
            let owner = self.boomerang[s].owner;
            let Some(os) = self
                .slot_of(owner)
                .filter(|&os| self.alive[os] && self.has(os, C_TRANSFORM))
            else {
                continue;
            };
            if !self.has(s, C_TRANSFORM) {
                continue;
            }
            let (ox, oy) = (self.transform[os].x, self.transform[os].y);
            let (ex, ey) = (self.transform[s].x, self.transform[s].y);
            let (dx, dy) = (ox - ex, oy - ey);
            // The catch: a returning boomerang within reach of its owner is caught — despawned,
            // and the catch reported to `boomerang_caught()`. Before this the return contract was
            // unfinished: the boomerang orbited its owner until a lifetime retired it.
            if dx.to_raw().abs() < 10 * 256 && dy.to_raw().abs() < 10 * 256 {
                if nc < caught.len() {
                    caught[nc] = encode(s as u32, self.gen[s]);
                    nc += 1;
                    self.boomer_catches += 1;
                }
                continue;
            }
            let speed = {
                let vx = self.body[s].vx.to_raw();
                let vy = self.body[s].vy.to_raw();
                // Cheap length stand-in: max-norm, good enough to keep return speed.
                vx.abs().max(vy.abs()).max(256)
            };
            let adx = dx.to_raw().abs().max(1);
            let ady = dy.to_raw().abs().max(1);
            let dom = adx.max(ady);
            self.body[s].vx = Fixed::from_raw((dx.to_raw() * speed) / dom);
            self.body[s].vy = Fixed::from_raw((dy.to_raw() * speed) / dom);
        }
        for &id in caught.iter().take(nc) {
            self.despawn(id);
        }
    }

    /// Put a decoy in the world. Enemies within `radius` px steer at it instead of the player for
    /// `frames`. Passing a dead/absent entity, or letting the timer run out, clears it.
    fn set_lure(&mut self, e: i32, radius: i32, frames: i32) {
        self.lure = (e, radius, frames);
    }

    /// The lure's position this frame, if one is live. Also ages it out — called once per frame from
    /// the AI phase, before hopper/chase read it.
    fn lure_point(&mut self) -> Option<(i32, i32)> {
        let (e, _r, f) = self.lure;
        if e < 0 || f <= 0 {
            return None;
        }
        self.lure.2 = f - 1;
        let s = self.slot_of(e)?;
        if !self.alive[s] || !self.has(s, C_TRANSFORM) {
            self.lure = (-1, 0, 0);
            return None;
        }
        Some((self.transform[s].x.floor(), self.transform[s].y.floor()))
    }

    /// Is slot `s` close enough to the lure to be distracted by it?
    fn lured_to(&self, s: usize, lp: Option<(i32, i32)>) -> Option<(i32, i32)> {
        let (lx, ly) = lp?;
        if self.lure.0 >= 0 && self.slot_of(self.lure.0) == Some(s) {
            return None; // the bait does not chase itself
        }
        let d = (lx - self.transform[s].x.floor()).abs() + (ly - self.transform[s].y.floor()).abs();
        if d <= self.lure.1 {
            Some((lx, ly))
        } else {
            None
        }
    }

    // ── Native enemy AI (`nai` column, `M2_NAI`) ─────────────────────────────
    // Four state machines from the NES-era top-down bestiary that the existing AI natives could
    // not express, all O(1) per entity per frame: the burrower (submerge cycle), the hovering
    // drifter (spin-up flight, hittable only at rest), the teleporting caster (teleport-flicker +
    // aimed cast), and the bouncer (parabolic hops).

    /// Advance the world RNG and return 16 usable bits (same LCG `set_shooter` staggers with).
    fn rnd16(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        self.rng >> 16
    }

    /// Store heading `(mx,my)` on both the nai record and the top-down mover, updating facing the
    /// same way `topdown_move` does (horizontal wins) so `set_dir_anim` composes.
    fn nai_head(&mut self, s: usize, mx: i32, my: i32) {
        self.nai[s].dx = mx;
        self.nai[s].dy = my;
        self.topdown[s].dx = mx;
        self.topdown[s].dy = my;
        if mx < 0 {
            self.topdown[s].facing = 2;
        } else if mx > 0 {
            self.topdown[s].facing = 3;
        } else if my < 0 {
            self.topdown[s].facing = 1;
        } else if my > 0 {
            self.topdown[s].facing = 0;
        }
    }

    /// Pick a random 8-direction heading.
    fn nai_pick_dir8(&mut self, s: usize) {
        const DIRS: [(i32, i32); 8] = [
            (0, 1),
            (0, -1),
            (1, 0),
            (-1, 0),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];
        let (mx, my) = DIRS[(self.rnd16() & 7) as usize];
        self.nai_head(s, mx, my);
    }

    /// Install a nai record. Every kind starts in state 0 with `timer = a` (plus a random stagger
    /// so a room of them does not cycle in lockstep — same rationale as `set_shooter`'s jitter).
    fn nai_install(&mut self, e: i32, kind: u8, a: i32, b: i32, speed: i32) -> Option<usize> {
        let s = self.slot_of(e)?;
        let a = a.max(1);
        let jitter = (self.rnd16() as i32).rem_euclid(a);
        self.nai[s] = Nai {
            kind,
            state: 0,
            timer: a - (jitter >> 1).min(a - 1),
            a,
            b: b.max(1),
            speed,
            ..Nai::default()
        };
        self.mask2[s] |= M2_NAI;
        self.used2 |= M2_NAI;
        Some(s)
    }

    /// `set_ambusher(e, hideFrames, surfaceFrames, speedQ8)` — burrower ground-emergence cycle.
    /// Hidden (`hideFrames`): intangible + invisible, drifting toward the player underground at
    /// `speedQ8` (Q8 px/frame) on the dominant axis. Surfaced (`surfaceFrames`): visible,
    /// vulnerable, stationary — compose with `set_shooter` for a surfacing shooter (the shooter
    /// fires whenever it can; while hidden the entity deals/takes nothing anyway, and its shots
    /// pause because hidden is also a fine time for the interval to elapse unanswered). Needs `C_TOPDOWN`.
    fn set_ambusher(&mut self, e: i32, hide: i32, surface: i32, speed: i32) {
        if let Some(s) = self.nai_install(e, 1, hide, surface, speed) {
            if !self.has(s, C_TOPDOWN) {
                self.mask2[s] &= !M2_NAI;
                return;
            }
            self.mask2[s] |= M2_PHASED | M2_HIDDEN;
            self.used2 |= M2_PHASED | M2_HIDDEN;
        }
    }

    /// `set_drifter(e, restFrames, flyFrames, speedQ8)` — hovering-drifter floating wander. Rest
    /// (`restFrames`): stationary and VULNERABLE — the only window the drifter can be hit. Fly
    /// (`flyFrames`): invulnerable (`M2_PHASED`, still tangible — contact still hurts), speed
    /// ramping up over the first quarter and down over the last (the spin-up/spin-down), heading
    /// re-picked every 32 frames. Needs `C_TOPDOWN`. The damage path reads the flag natively;
    /// game code can too, via `entity_phased`.
    fn set_drifter(&mut self, e: i32, rest: i32, fly: i32, speed: i32) {
        if let Some(s) = self.nai_install(e, 2, rest, fly.max(4), speed) {
            if !self.has(s, C_TOPDOWN) {
                self.mask2[s] &= !M2_NAI;
                return;
            }
            // Spin-up increment per frame, computed ONCE here (a divide costs nothing at
            // configure time; on the per-frame path it would be a software call — no divide
            // instruction on the ARM7TDMI).
            self.nai[s].step = (speed * 4 / fly.max(4)).max(1);
        }
    }

    /// `set_flicker_caster(e, hideFrames, visFrames, shotSpeed)` — teleporting caster. Hidden
    /// (`hideFrames`): gone (intangible + invisible). On appearing it teleports to a random
    /// walkable tile near the player (up to 8 tries against the solid grid; stays put if all
    /// fail), flickers in for 12 frames (blinking, still unhittable), then stands vulnerable for
    /// the rest of `visFrames`, casting ONE aimed shot at the window's midpoint using the bullet
    /// style in force at configure time (same capture contract as `set_shooter`). Needs
    /// `C_TRANSFORM`.
    fn set_flicker_caster(&mut self, e: i32, hide: i32, vis: i32, shot_speed: Fixed) {
        let style = self.bullet_style;
        if let Some(s) = self.nai_install(e, 3, hide, vis.max(16), shot_speed.to_raw()) {
            if !self.has(s, C_TRANSFORM) {
                self.mask2[s] &= !M2_NAI;
                return;
            }
            self.nai[s].style = style;
            self.mask2[s] |= M2_PHASED | M2_HIDDEN;
            self.used2 |= M2_PHASED | M2_HIDDEN;
        }
    }

    /// `set_bouncer(e, restFrames, hopFrames, speedQ8)` — parabolic bouncing movement. Rest
    /// (`restFrames`): grounded pause. Hop (`hopFrames`): moves at `speedQ8` on a heading that is
    /// 50% at-the-player (dominant axis), 50% random 8-dir, with a parabolic arc drawn by writing
    /// the sprite's `oy` (the entity OWNS its sprite offset while a bouncer — the base captured
    /// here is restored on landing). Collision stays the top-down mover's. Needs `C_TOPDOWN`.
    /// `set_ricochet(e, speedQ8)` — a diagonal that reflects off whatever stops it: tiles,
    /// blockers, the room's edge. Movement integrates through the top-down mover, so collision is
    /// what turns "this axis stopped" into the bounce — the system just watches the transform and
    /// flips the heading on the axis that failed to advance. a downstream game's NINE-BOUNCE bug ("frozen
    /// mid-leap nine thousand years ago and still going") is the reason it exists: it shipped on
    /// the shmup `set_mover`, whose Body integration ignores tiles entirely, and sailed through
    /// its room's walls off the map. Needs `C_TOPDOWN`.
    fn set_ricochet(&mut self, e: i32, speed: i32) {
        if let Some(s) = self.nai_install(e, 5, 1, 1, speed) {
            if !self.has(s, C_TOPDOWN) {
                self.mask2[s] &= !M2_NAI;
                return;
            }
            // Heading from the slot index, so a room of these fans out instead of marching in step.
            self.nai[s].dx = if s & 1 == 0 { 1 } else { -1 };
            self.nai[s].dy = if s & 2 == 0 { 1 } else { -1 };
            // Last-position scratch (a/b), primed OFF the spawn point so frame one cannot read as
            // a stall and flip the heading before the creature has moved at all.
            self.nai[s].a = self.transform[s].x.to_raw() - 1;
            self.nai[s].b = self.transform[s].y.to_raw() - 1;
        }
    }

    fn set_bouncer(&mut self, e: i32, rest: i32, hop: i32, speed: i32) {
        if let Some(s) = self.nai_install(e, 4, rest, hop.max(8), speed) {
            if !self.has(s, C_TOPDOWN) {
                self.mask2[s] &= !M2_NAI;
                return;
            }
            self.nai[s].aux = self.sprite[s].oy;
        }
    }

    /// The nai state machines, one pass per frame (pipeline phase 2, just before
    /// `topdown_system` integrates the intent they set). Stun freezes a machine in place — a
    /// stunned caster does not finish vanishing. Shots are deferred exactly like
    /// `shooter_system`'s, because firing spawns entities into the columns being walked.
    /// Continuous tile-aligned walking with perpendicular, target-seeking turns.
    ///
    /// A faithful port of the NES original's wanderer target-player routine. The shape
    /// that matters, and that no existing native expressed:
    ///
    ///   * it NEVER STOPS. Intent is re-asserted from `facing` every single frame, so the walker
    ///     is always moving. (The reference only halts for a shooting windup, which is the
    ///     shooter's business, not the walker's.)
    ///   * it may only TURN ON A TILE BOUNDARY. The original bails out unless the actor's
    ///     tile offset is 0. Positions here are world Fixed and actors spawn tile-aligned,
    ///     so testing the floored position against the 16 px grid is the same predicate.
    ///   * its turns are NOT RANDOM. Only the TIMING is. `TurnX`/`TurnY` always face the target;
    ///     `TurnIfTime` always switches to the perpendicular axis.
    ///
    /// ⚠️ `topdown_system` CONSUMES dx/dy every frame. Setting intent once makes the entity travel
    /// a single pixel and stop — the exact bug documented on `Hopper.dir_x`. Hence the
    /// unconditional re-assert at the bottom of the loop.
    fn wanderer_system(&mut self) {
        let Some(target) = self.camera_target else {
            return;
        };
        let Some(ts) = self.slot_of(target) else {
            return;
        };
        let tx = self.transform[ts].x.floor();
        let ty = self.transform[ts].y.floor();

        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask2[s] & M2_WANDERER == 0 || !self.has(s, C_TOPDOWN) {
                continue;
            }

            // ⚠️⚠️ CONFINE FIRST, BEFORE ANY CULLING GATE — ORDER IS THE WHOLE BUG.
            //
            // This was below `is_active`, and could therefore never fire. `camera_focus` adds half
            // the collider, so a walker at transform x 1792 focuses at 1798 and is not yet "at the
            // edge"; by the time its focus reaches the boundary its transform is ~1786, which is
            // already off-screen, so `is_active` culled it and `continue`d before the edge test ran.
            // The walker then sat outside the room — skipped by `same_room` too — motionless for
            // 268 frames. Confinement has to run for a walker the camera has given up on, which is
            // exactly the one that needs it.
            //
            // Clamping, not merely steering: the original stops a monster at the room margin
            // rather than trusting it to turn in time.
            let mut at_edge = false;
            if self.room_cam.enabled {
                let (rw, rh) = (self.room_cam.room_w * TILE, self.room_cam.room_h * TILE);
                let (left, top) = (self.wanderer[s].home_rx * rw, self.wanderer[s].home_ry * rh);
                let (cw, ch) = if self.has(s, C_COLLIDER) {
                    (self.collider[s].w.floor(), self.collider[s].h.floor())
                } else {
                    (0, 0)
                };
                let (right, bottom) = (left + rw - cw, top + rh - ch);
                let (x, y) = (self.transform[s].x.floor(), self.transform[s].y.floor());
                if x < left {
                    self.transform[s].x = Fixed::from_raw(left << 8);
                    at_edge = true;
                } else if x > right {
                    self.transform[s].x = Fixed::from_raw(right << 8);
                    at_edge = true;
                }
                if y < top {
                    self.transform[s].y = Fixed::from_raw(top << 8);
                    at_edge = true;
                } else if y > bottom {
                    self.transform[s].y = Fixed::from_raw(bottom << 8);
                    at_edge = true;
                }
            }

            if !self.is_active(s) {
                continue;
            }
            if self.stun[s] > 0 {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }
            if !self.same_room(s, ts) {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }
            // The turn timer ticks even while the walker cannot act (as in the original).
            if self.wanderer[s].turn_timer > 0 {
                self.wanderer[s].turn_timer -= 1;
            }
            // Knockback owns the entity while it lasts: the original returns early during a
            // shove. Steering during a shove fights the knock and cancels it.
            if self.topdown[s].knock > 0 {
                continue;
            }

            let px = self.transform[s].x.floor();
            let py = self.transform[s].y.floor();
            let aligned = (px & 0xF) == 0 && (py & 0xF) == 0;

            // ⚠️⚠️ BLOCKED IS ALSO A DECISION POINT, OR THE WALKER WEDGES FOREVER.
            //
            // Deriving "on a tile boundary" from the world position is only equivalent to the
            // reference's `tileOffset` while the walker is actually moving. Walk into a wall
            // mid-tile and the position stops changing at, say, x = 1706.5 — `aligned` is false and
            // can never become true again, so the walker never re-steers and stands pushing into
            // the wall for the rest of the run. Observed exactly that: a walker walked past its
            // target to x 1706.5 and was still facing left into a wall 400 frames later.
            //
            // The reference does not have this failure because `tileOffset` is its own counter that
            // collision resolution maintains, not a reading of the position. Treating "did not move
            // although it wanted to" as a decision point restores the intent — the walker gets to
            // pick a new direction the moment it is stuck — without inventing a second notion of
            // alignment.
            // ⚠️ COMPARE THE RAW FIXED POSITION, NOT THE FLOORED ONE. A walker at StdSpeed covers
            // 0.5 px/frame, so its FLOORED position is unchanged on every other frame — comparing
            // floors reports "blocked" half the time, which cleared the turn timer and let it
            // re-steer mid-tile. That produced 202 off-grid turns, the first only 7 frames in.
            let (rx_now, ry_now) = (self.transform[s].x.to_raw(), self.transform[s].y.to_raw());
            let blocked = rx_now == self.wanderer[s].last_x && ry_now == self.wanderer[s].last_y;
            self.wanderer[s].last_x = rx_now;
            self.wanderer[s].last_y = ry_now;

            // ⚠️⚠️ CONFINE THE WALKER TO ITS ROOM, OR IT WALKS OUT AND FREEZES FOR GOOD.
            //
            // A hopper moves in 16 px lurches and rarely escapes; a wanderer walks continuously and
            // WILL leave. The moment it does, `same_room` above is false, its intent is zeroed and
            // it is skipped forever — a live entity standing off-screen for the rest of the run.
            // Measured exactly that: a walker spawned at world x 1856 inside the room spanning
            // 1792..2047, walked left to 1706.5, and never moved again from frame 347 to 599.
            // The original confines monsters with an explicit world-margin check.
            // Being stuck IS the trigger to choose again. Without clearing the timer the walker
            // still could not act: `wanderer_turn_if_time` returns early while `turn_timer` is
            // non-zero, and that timer is re-armed with up to 255 frames on every turn — so a
            // blocked walker stood motionless for 252 frames waiting it out.
            let stuck = blocked || at_edge;
            if stuck {
                self.wanderer[s].turn_timer = 0;
            }

            if self.topdown[s].speed != 0 && (aligned || stuck) {
                let r = (self.rnd16() & 0xFF) as i32;
                if r > self.wanderer[s].turn_rate {
                    self.wanderer_turn_if_time(s, px, py, tx, ty);
                } else if (px - tx).abs() < 9 {
                    // already sharing the target's column — line up vertically
                    self.wanderer_turn_y(s, py, ty);
                } else if (py - ty).abs() < 9 {
                    self.wanderer_turn_x(s, px, tx);
                } else {
                    self.wanderer_turn_if_time(s, px, py, tx, ty);
                }
            }

            // moving = facing, as in the original.
            let (dx, dy) = match self.topdown[s].facing {
                1 => (0, -1),
                2 => (-1, 0),
                3 => (1, 0),
                _ => (0, 1),
            };
            self.topdown[s].dx = dx;
            self.topdown[s].dy = dy;
        }
    }

    /// Turn-if-time rule: drift onto the perpendicular axis, but only
    /// once the turn timer has expired.
    fn wanderer_turn_if_time(&mut self, s: usize, px: i32, py: i32, tx: i32, ty: i32) {
        self.wanderer[s].want_shoot = 0;
        if self.wanderer[s].turn_timer != 0 {
            return;
        }
        // facing 0=down 1=up are the vertical ones, so turn horizontally, and vice versa.
        if self.topdown[s].facing <= 1 {
            self.wanderer_turn_x(s, px, tx);
        } else {
            self.wanderer_turn_y(s, py, ty);
        }
    }

    /// Horizontal turn. Faces the target horizontally and re-arms the
    /// timer with a fresh random interval.
    fn wanderer_turn_x(&mut self, s: usize, px: i32, tx: i32) {
        self.topdown[s].facing = if tx < px { 2 } else { 3 };
        let t = (self.rnd16() & 0xFF) as i32;
        self.wanderer[s].turn_timer = t;
        self.wanderer[s].want_shoot = 1;
    }

    /// Vertical turn (mirror of the horizontal one).
    fn wanderer_turn_y(&mut self, s: usize, py: i32, ty: i32) {
        self.topdown[s].facing = if ty < py { 1 } else { 0 };
        let t = (self.rnd16() & 0xFF) as i32;
        self.wanderer[s].turn_timer = t;
        self.wanderer[s].want_shoot = 1;
    }

    /// Has this walker just lined up on the target? A shooting system reads and clears it.
    fn wanderer_wants_shot(&self, s: usize) -> bool {
        self.wanderer[s].want_shoot != 0
    }

    fn nai_system(&mut self) {
        let ts = self.camera_target.and_then(|t| self.slot_of(t));
        let mut shots: [(Fixed, Fixed, Fixed, Fixed, BulletStyle); 4] = [(
            Fixed::from_raw(0),
            Fixed::from_raw(0),
            Fixed::from_raw(0),
            Fixed::from_raw(0),
            BulletStyle::default(),
        ); 4];
        let mut nsh = 0usize;
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask2[s] & M2_NAI == 0 || !self.is_active(s) {
                continue;
            }
            if self.stun[s] > 0 {
                continue;
            }
            let near = ts.filter(|&t| self.same_room(s, t));
            let z = self.nai[s];
            match z.kind {
                // ── Ambusher: hidden-approach → surface → repeat ─────────────
                1 => {
                    if z.state == 0 {
                        if let Some(t) = near {
                            let dx = self.transform[t].x.floor() - self.transform[s].x.floor();
                            let dy = self.transform[t].y.floor() - self.transform[s].y.floor();
                            let (mx, my) = if dx.abs() >= dy.abs() {
                                (dx.signum(), 0)
                            } else {
                                (0, dy.signum())
                            };
                            self.topdown[s].speed = z.speed;
                            self.nai_head(s, mx, my);
                        } else {
                            self.nai_head(s, 0, 0);
                        }
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            self.nai[s].state = 1;
                            self.nai[s].timer = z.b;
                            self.mask2[s] &= !(M2_PHASED | M2_HIDDEN);
                            self.nai_head(s, 0, 0);
                        }
                    } else {
                        self.topdown[s].dx = 0;
                        self.topdown[s].dy = 0;
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            self.nai[s].state = 0;
                            self.nai[s].timer = z.a;
                            self.mask2[s] |= M2_PHASED | M2_HIDDEN;
                            self.used2 |= M2_PHASED | M2_HIDDEN;
                        }
                    }
                }
                // ── Drifter: rest (vulnerable) → spin-up flight (phased) ─────
                2 => {
                    if z.state == 0 {
                        self.topdown[s].dx = 0;
                        self.topdown[s].dy = 0;
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            self.nai[s].state = 1;
                            self.nai[s].timer = z.b;
                            self.nai[s].aux = z.step; // current ramped speed
                            self.mask2[s] |= M2_PHASED;
                            self.used2 |= M2_PHASED;
                            self.nai_pick_dir8(s);
                        }
                    } else {
                        let quarter = (z.b >> 2).max(1);
                        let elapsed = z.b - z.timer;
                        let cur = if elapsed < quarter {
                            (z.aux + z.step).min(z.speed)
                        } else if z.timer < quarter {
                            (z.aux - z.step).max(z.step)
                        } else {
                            z.speed
                        };
                        self.nai[s].aux = cur;
                        self.topdown[s].speed = cur;
                        if z.timer & 31 == 0 {
                            self.nai_pick_dir8(s);
                        } else {
                            self.topdown[s].dx = z.dx;
                            self.topdown[s].dy = z.dy;
                        }
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            self.nai[s].state = 0;
                            self.nai[s].timer = z.a;
                            self.mask2[s] &= !M2_PHASED;
                            self.nai_head(s, 0, 0);
                        }
                    }
                }
                // ── Flicker caster: vanish → teleport near player → flicker in → cast ──
                3 => {
                    if z.state == 0 {
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            if let Some(t) = near {
                                let pc = self.transform[t].x.floor() / TILE;
                                let pr = self.transform[t].y.floor() / TILE;
                                for _ in 0..8 {
                                    let r = self.rnd16();
                                    let dc = ((r & 7) as i32) - 3;
                                    let dr = (((r >> 3) & 7) as i32) - 3;
                                    if dc.abs() + dr.abs() < 2 {
                                        continue; // never right on top of the player
                                    }
                                    let (c, rw) = (pc + dc, pr + dr);
                                    if !self.is_solid(c, rw) {
                                        self.transform[s].x = Fixed::from_raw(c * TILE * 256);
                                        self.transform[s].y = Fixed::from_raw(rw * TILE * 256);
                                        break;
                                    }
                                }
                            }
                            self.nai[s].state = 1;
                            self.nai[s].timer = z.b;
                            self.nai[s].aux = 0; // not yet cast this window
                        }
                    } else {
                        let elapsed = z.b - z.timer;
                        if elapsed < 12 {
                            // Flicker-in: blinking and still unhittable.
                            self.mask2[s] |= M2_PHASED;
                            self.used2 |= M2_PHASED;
                            if elapsed & 2 == 0 {
                                self.mask2[s] |= M2_HIDDEN;
                                self.used2 |= M2_HIDDEN;
                            } else {
                                self.mask2[s] &= !M2_HIDDEN;
                            }
                        } else {
                            self.mask2[s] &= !(M2_PHASED | M2_HIDDEN);
                        }
                        if elapsed == z.b >> 1 && self.nai[s].aux == 0 && nsh < shots.len() {
                            if let Some(t) = near {
                                self.nai[s].aux = 1;
                                let (ptx, pty) = self.center_of(t);
                                let (cx, cy) = self.center_of(s);
                                let (dx, dy) = (ptx - cx, pty - cy);
                                let len = (dx * dx + dy * dy).sqrt();
                                let (ux, uy) = if len.to_raw() == 0 {
                                    (Fixed::from_raw(0), Fixed::from_raw(256))
                                } else {
                                    (dx / len, dy / len)
                                };
                                let muzzle = Fixed::from_raw(12 * 256);
                                let spd = Fixed::from_raw(z.speed);
                                shots[nsh] = (
                                    cx + ux * muzzle,
                                    cy + uy * muzzle,
                                    ux * spd,
                                    uy * spd,
                                    z.style,
                                );
                                nsh += 1;
                            }
                        }
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            self.nai[s].state = 0;
                            self.nai[s].timer = z.a;
                            self.mask2[s] |= M2_PHASED | M2_HIDDEN;
                            self.used2 |= M2_PHASED | M2_HIDDEN;
                        }
                    }
                }
                // ── Bouncer: grounded pause → parabolic hop ──────────────────
                4 => {
                    if z.state == 0 {
                        self.topdown[s].dx = 0;
                        self.topdown[s].dy = 0;
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            self.nai[s].state = 1;
                            self.nai[s].timer = z.b;
                            let r = self.rnd16();
                            if let (true, Some(t)) = (r & 1 == 0, near) {
                                let dx = self.transform[t].x.floor() - self.transform[s].x.floor();
                                let dy = self.transform[t].y.floor() - self.transform[s].y.floor();
                                let (mx, my) = if dx.abs() >= dy.abs() {
                                    (dx.signum(), 0)
                                } else {
                                    (0, dy.signum())
                                };
                                self.nai_head(s, mx, my);
                            } else {
                                self.nai_pick_dir8(s);
                            }
                            self.topdown[s].speed = z.speed;
                            self.nai[s].z = 0;
                            self.nai[s].vz = (NAI_BOUNCE_G * z.b) >> 1;
                        }
                    } else {
                        self.topdown[s].dx = z.dx;
                        self.topdown[s].dy = z.dy;
                        self.nai[s].z = (self.nai[s].z + self.nai[s].vz).max(0);
                        self.nai[s].vz -= NAI_BOUNCE_G;
                        if self.has(s, C_SPRITE) {
                            self.sprite[s].oy = z.aux - (self.nai[s].z >> 8);
                        }
                        self.nai[s].timer -= 1;
                        if self.nai[s].timer <= 0 {
                            self.nai[s].state = 0;
                            self.nai[s].timer = z.a;
                            self.nai[s].z = 0;
                            if self.has(s, C_SPRITE) {
                                self.sprite[s].oy = z.aux;
                            }
                            self.nai_head(s, 0, 0);
                        }
                    }
                }
                // ── Ricochet: flip the axis that stopped, keep going forever ─
                5 => {
                    let px = self.transform[s].x.to_raw();
                    let py = self.transform[s].y.to_raw();
                    if px == z.a && z.dx != 0 {
                        self.nai[s].dx = -z.dx;
                    }
                    if py == z.b && z.dy != 0 {
                        self.nai[s].dy = -z.dy;
                    }
                    self.nai[s].a = px;
                    self.nai[s].b = py;
                    self.topdown[s].speed = z.speed;
                    self.topdown[s].dx = self.nai[s].dx;
                    self.topdown[s].dy = self.nai[s].dy;
                }
                _ => {}
            }
            // Sprite visibility tracks the hidden bit every frame (idempotent when unchanged;
            // `health_system` runs later and agrees — it carries the same M2_HIDDEN guard).
            if self.has(s, C_SPRITE) {
                let h = self.sprite[s].handle;
                if h >= 0 {
                    tish_agb::native_sprite_set_visible(h, self.mask2[s] & M2_HIDDEN == 0);
                }
            }
        }
        for &(x, y, vx, vy, style) in shots.iter().take(nsh) {
            let save = self.bullet_style;
            self.bullet_style = style;
            self.fire_bullet(x, y, vx, vy);
            self.bullet_style = save;
        }
    }

    // ── Multi-part boss glue (`zx` column) ───────────────────────────────────
    // Minimal primitives the boss_parts.json taxonomy needs beyond set_part/set_train/set_orbiter:
    // per-part hit ROUTING (neck→head, segment→shrinking tail), a vulnerability GATE (an eye that
    // must be open, a last-hit window), part-death NOTIFICATION to the parent (an all-parts-dead
    // check), and detached-part PROMOTION (a head that flies off on part death — here: clear the
    // follow glue and give the same entity new AI).

    /// `set_hit_proxy(e, target)` — damage dealt to `e` lands on `target` instead (≤4 hops,
    /// re-pointable every frame for the shrinking-tail rule). `target < 0` clears.
    fn set_hit_proxy(&mut self, e: i32, target: i32) {
        if let Some(s) = self.slot_of(e) {
            if target < 0 {
                self.mask2[s] &= !M2_PROXY;
            } else {
                self.zx[s].proxy = target;
                self.mask2[s] |= M2_PROXY;
                self.used2 |= M2_PROXY;
            }
        }
    }

    /// `set_vuln_gate(e, open)` — while closed (`open == 0`), `damage()` refuses every hit on `e`
    /// (and every hit routed INTO `e` by a proxy). Installing the gate open is how a game arms a
    /// gated-eye boss: close it while the eye is shut, open it on the eye-open frames.
    fn set_vuln_gate(&mut self, e: i32, open: i32) {
        if let Some(s) = self.slot_of(e) {
            self.zx[s].gate = open;
            self.mask2[s] |= M2_GATE;
            self.used2 |= M2_GATE;
        }
    }

    /// `set_death_note(e, code)` — when `e` dies, push `code` (non-zero) to the death-note queue
    /// the parent's logic drains with `death_note()`.
    fn set_death_note(&mut self, e: i32, code: i32) {
        if let Some(s) = self.slot_of(e) {
            self.zx[s].code = code;
            self.mask2[s] |= M2_NOTE;
            self.used2 |= M2_NOTE;
        }
    }

    /// `set_phased(e, on)` — direct control of the invulnerable flag for state machines the game
    /// owns (a core invulnerable while its orbiting children live, a boss awaiting its trigger
    /// item). Tangibility is
    /// kept: contact damage still lands on the player.
    fn set_phased(&mut self, e: i32, on: bool) {
        if let Some(s) = self.slot_of(e) {
            if on {
                self.mask2[s] |= M2_PHASED;
                self.used2 |= M2_PHASED;
            } else {
                self.mask2[s] &= !M2_PHASED;
            }
        }
    }

    /// `detach_part(e)` — promotion: sever the `set_part`/`set_train`/`set_orbiter` glue so the
    /// entity moves on its own again (a severed flying head: detach, then give it drifter +
    /// shooter AI — same entity, same sprite, no respawn).
    fn detach_part(&mut self, e: i32) {
        if let Some(s) = self.slot_of(e) {
            self.mask[s] &= !C_FOLLOW;
        }
    }

    fn set_chase(&mut self, e: i32, aggro: i32, stride: i32, flap: i32, anim_speed: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TOPDOWN)) {
            self.chase[s] = Chase {
                aggro,
                stride,
                flap: flap.max(1),
                anim_speed: anim_speed.max(1),
            };
            self.mask[s] |= C_CHASE;
            self.used |= C_CHASE;
        }
    }

    /// Native chase system (pure Rust, zero tish callbacks): each `C_CHASE` entity steers toward the
    /// camera target when within aggro and animates from its facing — a room of enemies for free.
    fn chase_system(&mut self) {
        let Some(target) = self.camera_target else {
            return;
        };
        let Some(ts) = self.slot_of(target) else {
            return;
        };
        let ptx = self.transform[ts].x.floor();
        let pty = self.transform[ts].y.floor();
        let lp = self.lure_point();
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_CHASE | C_TOPDOWN) || !self.is_active(s) {
                continue;
            }
            if self.stun[s] > 0 {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }
            // Bait outranks the player: a distracted enemy walks to the food, which is the whole
            // point of carrying it past a hungry guard.
            let (tx, ty) = self.lured_to(s, lp).unwrap_or((ptx, pty));
            let c = self.chase[s];
            let dx = tx - self.transform[s].x.floor();
            let dy = ty - self.transform[s].y.floor();
            let (mut mx, mut my) = (0, 0);
            // Room-confined chase: with a room camera, an enemy only pursues while it shares the
            // player's current room. Otherwise it holds position — no chasing across the screen
            // boundary into the next room — and it freezes entirely during a room slide.
            let same_room = match self.camera_target.and_then(|e| self.slot_of(e)) {
                Some(ts) => self.same_room(s, ts),
                None => true,
            };
            // Chase only in the [contact, aggro] band: within ~12px the enemy is already touching the
            // player (contact damage handles it), so it STOPS instead of jittering ±1px around it —
            // that jitter flipped the facing every frame and made the sprite look like it was spinning.
            let dist = dx.abs() + dy.abs();
            if same_room && dist < c.aggro && dist > 12 {
                if dx.abs() > 6 {
                    mx = dx.signum();
                }
                if dy.abs() > 6 {
                    my = dy.signum();
                }
            }
            // Read the component test BEFORE taking the mutable borrow of `topdown`.
            let owns_frame = self.has(s, C_DIRANIM);
            let td = &mut self.topdown[s];
            td.dx = mx;
            td.dy = my;
            if c.stride > 0 {
                // directional: face the move (horizontal wins) + play the walk / idle clip
                if mx < 0 {
                    td.facing = 2;
                } else if mx > 0 {
                    td.facing = 3;
                } else if my < 0 {
                    td.facing = 1;
                } else if my > 0 {
                    td.facing = 0;
                }
                // `C_DIRANIM` owns the frame when present: it knows where this actor starts on a
                // SHARED strip, which the lines below do not. Without this guard every chasing
                // enemy on a game's 33-actor strip animated frames 0..n — i.e. only the one actor
                // whose base the strip happened to start at.
                if !owns_frame {
                    let base = td.facing * c.stride;
                    let moving = mx != 0 || my != 0;
                    let e = encode(s as u32, self.gen[s]);
                    if moving {
                        self.anim_play(e, base + 1, 4, c.anim_speed, true);
                    } else {
                        self.anim_play(e, base, 1, 1, false);
                    }
                }
            } else if !owns_frame {
                // non-directional (a bat) — just loop the flap frames
                let e = encode(s as u32, self.gen[s]);
                self.anim_play(e, 0, c.flap, c.anim_speed, true);
            }
        }
    }

    // ── RTS: flow fields, seek, attack-move, fog of war ──────────────────────
    //
    // The genre's defining problem is many units moving at once under ONE player intent, and the
    // engine had no shape for it. `isob_path` answers "the route for THIS unit, right now", which is
    // the turn-based question; twelve units ordered to the same place want the opposite — one
    // breadth-first search whose result every unit reads in O(1), rebuilt only when the destination
    // moves. Everything below exists so that a screen of units costs no tish calls at all; see
    // `docs/perf-rules.md` rule 7 for why a per-unit `tick` hook is not an option.

    /// Ensure flow field `id` exists and is sized to the current collision grid. Fields are
    /// allocated lazily and reused: the `dist`/`queue` buffers are the only allocation an RTS makes
    /// after load, and they are made once.
    fn flow_ensure(&mut self, id: usize) -> bool {
        if id >= MAX_FLOWS || self.grid_cols <= 0 || self.grid_rows <= 0 {
            return false;
        }
        let cells = (self.grid_cols * self.grid_rows) as usize;
        let f = &mut self.flows[id];
        if f.cols != self.grid_cols || f.rows != self.grid_rows {
            f.cols = self.grid_cols;
            f.rows = self.grid_rows;
            f.dist = alloc::vec![u16::MAX; cells];
            f.queue = Vec::with_capacity(cells);
            f.goal_col = -1;
            f.goal_row = -1;
            f.ready = false;
        }
        true
    }

    /// Breadth-first fill of `id` outward from (col,row) over walkable cells. Uniform cost, so a
    /// plain FIFO queue is exact — no priority queue, no sorting, no division.
    ///
    /// Re-running with the goal it already has is a no-op, which is what makes this affordable: a
    /// move order rebuilds the field once and then every unit reads it for free until the player
    /// gives another order.
    fn flow_goal(&mut self, id: usize, col: i32, row: i32) {
        if !self.flow_ensure(id) {
            return;
        }
        if self.flows[id].ready && self.flows[id].goal_col == col && self.flows[id].goal_row == row
        {
            return;
        }
        let (cols, rows) = (self.grid_cols, self.grid_rows);
        if col < 0 || row < 0 || col >= cols || row >= rows {
            return;
        }
        // Walkability is read from the World's own collision grid, so a flow field automatically
        // agrees with what actually blocks movement — there is no second map to keep in step.
        let mut walk = alloc::vec![false; (cols * rows) as usize];
        for r in 0..rows {
            for c in 0..cols {
                walk[(r * cols + c) as usize] = !self.is_solid(c, r);
            }
        }
        let f = &mut self.flows[id];
        f.goal_col = col;
        f.goal_row = row;
        f.ready = true;
        for d in f.dist.iter_mut() {
            *d = u16::MAX;
        }
        f.queue.clear();
        let start = (row * cols + col) as usize;
        f.dist[start] = 0;
        f.queue.push(start as i32);
        let mut head = 0usize;
        while head < f.queue.len() {
            let cur = f.queue[head] as usize;
            head += 1;
            let d = f.dist[cur];
            if d == u16::MAX {
                continue;
            }
            let nd = d.saturating_add(1);
            let cc = (cur as i32) % cols;
            let cr = (cur as i32) / cols;
            // Four-neighbour, written out rather than looped over an offset table: the bounds test
            // differs per direction and a table would need a multiply per step.
            if cc > 0 {
                let i = cur - 1;
                if walk[i] && f.dist[i] > nd {
                    f.dist[i] = nd;
                    f.queue.push(i as i32);
                }
            }
            if cc < cols - 1 {
                let i = cur + 1;
                if walk[i] && f.dist[i] > nd {
                    f.dist[i] = nd;
                    f.queue.push(i as i32);
                }
            }
            if cr > 0 {
                let i = cur - cols as usize;
                if walk[i] && f.dist[i] > nd {
                    f.dist[i] = nd;
                    f.queue.push(i as i32);
                }
            }
            if cr < rows - 1 {
                let i = cur + cols as usize;
                if walk[i] && f.dist[i] > nd {
                    f.dist[i] = nd;
                    f.queue.push(i as i32);
                }
            }
        }
    }

    /// Steps-to-goal at (col,row), or -1 if the cell cannot reach the goal. The value a unit reads.
    fn flow_dist(&self, id: usize, col: i32, row: i32) -> i32 {
        if id >= MAX_FLOWS {
            return -1;
        }
        let f = &self.flows[id];
        if !f.ready || col < 0 || row < 0 || col >= f.cols || row >= f.rows {
            return -1;
        }
        let d = f.dist[(row * f.cols + col) as usize];
        if d == u16::MAX {
            -1
        } else {
            d as i32
        }
    }

    fn set_seek(&mut self, e: i32, field: i32, arrive: i32, stride: i32, anim_speed: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TOPDOWN)) {
            self.seek[s] = Seek {
                field,
                arrive: arrive.max(0),
                stride,
                anim_speed: anim_speed.max(1),
                done: false,
            };
            self.mask[s] |= C_SEEK;
            self.used |= C_SEEK;
        }
    }

    fn clear_seek(&mut self, e: i32) {
        if let Some(s) = self.slot_of(e) {
            self.mask[s] &= !C_SEEK;
            self.topdown[s].dx = 0;
            self.topdown[s].dy = 0;
        }
    }

    fn seek_arrived(&self, e: i32) -> bool {
        self.slot_of(e).map(|s| self.seek[s].done).unwrap_or(true)
    }

    /// Walk every seeking entity one step down its flow field. Per unit this is two array reads per
    /// axis and no call, no divide and no float — the entire reason the field exists.
    fn seek_system(&mut self) {
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask[s] & C_SLEEP != 0 || !self.has(s, C_SEEK | C_TOPDOWN) {
                continue;
            }
            if self.stun[s] > 0 {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }
            // A soldier that has broken off to fight owns its own movement this frame — it is either
            // closing on its target or standing still to swing, and `soldier_system` (which runs
            // first) has already written that intent. Zeroing it here instead would mean a unit
            // could never walk the last few pixels to something it had already decided to attack.
            if self.has(s, C_SOLDIER) && self.soldier[s].target >= 0 {
                continue;
            }
            let sk = self.seek[s];
            let id = sk.field as usize;
            if id >= MAX_FLOWS || !self.flows[id].ready {
                continue;
            }
            // A transform is the collider's TOP-LEFT (see `box_hits_solid`), so every cell question
            // below has to be asked about the box's CENTRE. Asking it about the corner puts the unit
            // a half-box off, which reads as "it paths into the wall beside the gap".
            let (px, py) = self.seek_centre(s);
            let col = px >> 4;
            let row = py >> 4;
            let (gc, gr) = (self.flows[id].goal_col, self.flows[id].goal_row);
            // Arrival is measured in PIXELS against the goal cell's centre, not in cells: a unit
            // whose cell index matches the goal can still be 15px away, and a crowd that stops on
            // "same cell" piles into one tile and jitters.
            let d_px = (px - (gc * 16 + 8)).abs() + (py - (gr * 16 + 8)).abs();
            if d_px <= sk.arrive.max(4) {
                self.seek[s].done = true;
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                if sk.stride > 0 && !self.has(s, C_DIRANIM) {
                    let base = self.topdown[s].facing * sk.stride;
                    let e = encode(s as u32, self.gen[s]);
                    self.anim_play(e, base, 1, 1, false);
                }
                continue;
            }
            self.seek[s].done = false;
            let here = self.flow_dist(id, col, row);
            // Off the field (spawned inside a wall, or walled off from the goal): hold still rather
            // than wander. A unit that cannot reach its order is a UI problem, not a movement one.
            if here < 0 {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }
            // Inside the goal cell the field is flat — every neighbour is FARTHER, so the "step to a
            // smaller number" rule finds no move and the unit parks wherever it entered the tile,
            // up to 15px off centre and permanently short of its arrive radius. The last step of a
            // move order is therefore in PIXELS, not in cells.
            if here == 0 {
                let gx = gc * 16 + 8;
                let gy = gr * 16 + 8;
                let td = &mut self.topdown[s];
                td.dx = if (px - gx).abs() <= 2 {
                    0
                } else if px < gx {
                    1
                } else {
                    -1
                };
                td.dy = if (py - gy).abs() <= 2 {
                    0
                } else if py < gy {
                    1
                } else {
                    -1
                };
                continue;
            }
            let west = self.flow_dist(id, col - 1, row);
            let east = self.flow_dist(id, col + 1, row);
            let north = self.flow_dist(id, col, row - 1);
            let south = self.flow_dist(id, col, row + 1);
            let better = |n: i32| n >= 0 && n < here;
            let mut mx = 0;
            let mut my = 0;
            // Both axes may improve at once, which is what gives diagonal movement around corners
            // without storing a direction per cell.
            if better(west) && (!better(east) || west <= east) {
                mx = -1;
            } else if better(east) {
                mx = 1;
            }
            if better(north) && (!better(south) || north <= south) {
                my = -1;
            } else if better(south) {
                my = 1;
            }
            // Corridor centring. The field reasons in CELLS; a unit is a BOX, and a box walking
            // along row 3 with its bottom edge poking into row 4 is blocked by row 4's wall even
            // though every cell of its route is clear. It then wedges permanently at the first gap.
            //
            // So when the step is purely along one axis, drift toward the centre of the lane on the
            // other. This is free (it uses movement the unit was making anyway) and it is what stops
            // "the path is right but the unit is stuck" — the single most confusing failure a flow
            // field produces.
            if mx != 0 && my == 0 {
                let cy = row * 16 + 8;
                if (py - cy).abs() > 2 {
                    my = if py < cy { 1 } else { -1 };
                }
            } else if my != 0 && mx == 0 {
                let cx = col * 16 + 8;
                if (px - cx).abs() > 2 {
                    mx = if px < cx { 1 } else { -1 };
                }
            }
            let owns_frame = self.has(s, C_DIRANIM);
            let td = &mut self.topdown[s];
            td.dx = mx;
            td.dy = my;
            if sk.stride > 0 {
                if mx < 0 {
                    td.facing = 2;
                } else if mx > 0 {
                    td.facing = 3;
                } else if my < 0 {
                    td.facing = 1;
                } else if my > 0 {
                    td.facing = 0;
                }
                if !owns_frame {
                    let base = td.facing * sk.stride;
                    let moving = mx != 0 || my != 0;
                    let e = encode(s as u32, self.gen[s]);
                    if moving {
                        self.anim_play(e, base + 1, 4, sk.anim_speed, true);
                    } else {
                        self.anim_play(e, base, 1, 1, false);
                    }
                }
            }
        }
    }

    // ── Terrain as a streamed window ─────────────────────────────────────────
    //
    // An alternative to the `scene:` Tiled pipeline, for a game whose terrain and whose OVERLAY
    // (fog, in practice) must share one palette.
    //
    // `scene:` bakes an atlas from the tiles a map actually uses, and `tilemap_new` uploads the
    // whole tileset's palettes — and the GBA has ONE set of sixteen background palette banks, so
    // whichever ran last owns the colours on screen. Two bakers over the same PNG produce two
    // orderings, and the loser draws in the winner's colours: measured in warforge as a black map,
    // then a brown shroud, depending which way round they were built.
    //
    // Loading the map here instead means one asset, one palette, and no conflict possible. The map
    // costs 4 bytes a cell in EWRAM (a 40x26 map is ~4KB) and is written once per mission.
    /// `gids` / `solid` are the tish arrays, read STRAIGHT into the reused buffer.
    ///
    /// The obvious version — collect each into a `Vec<i32>`, then `to_vec()` into `self.terr` —
    /// allocates three 1,040-element buffers per mission load and drops them again. On a small heap
    /// that churn fragmented it enough that the first mission transition died in the allocator.
    /// `clear()` keeps the capacity, so after the first map no allocation happens at all.
    fn terrain_load(&mut self, cols: i32, rows: i32, gids: Option<&Value>, solid: Option<&Value>) {
        self.terr_cols = cols;
        self.terr_rows = rows;
        self.terr.clear();
        if let Some(Value::Array(a)) = gids {
            let b = a.borrow();
            self.terr.reserve(b.len());
            for v in b.iter() {
                self.terr.push(match v {
                    Value::Number(f) => *f as i32,
                    _ => 0,
                });
            }
        }
        self.terr_shown = alloc::vec![-1i32; 256];
        self.terr_win = alloc::vec![-1i32; 256];
        // The collision grid comes from the same arrays, so terrain and pathing cannot disagree.
        self.grid_setup(cols, rows);
        if let Some(Value::Array(a)) = solid {
            let b = a.borrow();
            for r in 0..rows {
                for c in 0..cols {
                    let i = (r * cols + c) as usize;
                    let on = match b.get(i) {
                        Some(Value::Number(f)) => *f != 0.0,
                        _ => false,
                    };
                    if on {
                        self.grid_set_solid(c, r, true);
                    }
                }
            }
        }
    }

    /// Repaint a single terrain cell — a razed building reverting to ground, a felled tree becoming
    /// a stump. The window blit picks the change up on its next pass.
    fn terrain_set(&mut self, col: i32, row: i32, gid: i32, solid: i32) {
        if col < 0 || row < 0 || col >= self.terr_cols || row >= self.terr_rows {
            return;
        }
        let i = (row * self.terr_cols + col) as usize;
        if i < self.terr.len() {
            self.terr[i] = gid;
        }
        self.grid_set_solid(col, row, solid != 0);
    }

    /// Paint the terrain window for the camera, writing only what changed — the same wrapping
    /// 16x16-cell torus `fog_blit` uses, and for the same reason: a `tilemap_new` layer is 256x256
    /// px and wraps, and that wrap is what lets one small layer cover a map of any size.
    ///
    /// The fog is painted **into this same layer**: an unseen cell writes `gid_unseen` instead of
    /// its terrain. A separate shroud layer is the obvious design and was built first — it works in
    /// isolation (examples/rts-fog) but a second `tilemap_new` layer stacked over a first one, with
    /// a UI canvas also claiming a slot, would not come to the front however its priority was set.
    /// Folding it in costs one blit instead of two and removes the ordering question entirely.
    ///
    /// The model this gives is Warcraft's own: unseen is black, explored stays visible as terrain,
    /// and what the fog actually hides is UNITS — which the game does by hiding their sprites.
    fn terrain_blit(
        &mut self,
        bg: i32,
        tileset: i32,
        ts_cols: i32,
        cam_x: i32,
        cam_y: i32,
        gid_unseen: i32,
    ) -> i32 {
        if self.terr_cols <= 0 {
            return 0;
        }
        let (cols, rows) = (self.terr_cols, self.terr_rows);
        let c0 = cam_x >> 4;
        let r0 = cam_y >> 4;
        let mut wrote = 0;
        for dr in 0..11 {
            let r = r0 + dr;
            if r < 0 || r >= rows {
                continue;
            }
            for dc in 0..16 {
                let c = c0 + dc;
                if c < 0 || c >= cols {
                    continue;
                }
                let map_i = r * cols + c;
                let mut gid = self.terr[map_i as usize];
                if gid_unseen > 0
                    && self.fog.on
                    && self
                        .fog
                        .state
                        .get(map_i as usize)
                        .copied()
                        .unwrap_or(FOG_VISIBLE)
                        == FOG_UNSEEN
                {
                    gid = gid_unseen;
                }
                let bgi = (((r & 15) * 16) + (c & 15)) as usize;
                // `terr_shown` holds the gid actually painted — which may be the shroud — so a
                // cell repaints when the FOG changes as well as when the terrain does.
                if self.terr_win[bgi] == map_i && self.terr_shown[bgi] == gid {
                    continue;
                }
                self.terr_win[bgi] = map_i;
                self.terr_shown[bgi] = gid;
                tish_agb::native_tilemap_set(bg, tileset, ts_cols, c & 15, r & 15, gid);
                wrote += 1;
            }
        }
        wrote
    }

    /// Centre of an entity's collider in whole pixels. The engine's transform is a box's top-left,
    /// which is right for collision and wrong for every "which cell am I in" question.
    fn seek_centre(&self, s: usize) -> (i32, i32) {
        let (mut hw, mut hh) = (0, 0);
        if self.has(s, C_COLLIDER) {
            hw = self.collider[s].w.floor() / 2;
            hh = self.collider[s].h.floor() / 2;
        }
        (
            self.transform[s].x.floor() + hw,
            self.transform[s].y.floor() + hh,
        )
    }

    fn set_soldier(&mut self, e: i32, team: i32, range: i32, dmg: i32, cooldown: i32) {
        if let Some(s) = self.slot_of(e) {
            self.soldier[s] = Soldier {
                team,
                range: range.max(1),
                dmg: dmg.max(0),
                cooldown: cooldown.max(1),
                timer: 0,
                target: -1,
                recheck: 0,
            };
            self.mask[s] |= C_SOLDIER;
            self.used |= C_SOLDIER;
        }
    }

    fn soldier_team(&self, e: i32) -> i32 {
        self.slot_of(e).map(|s| self.soldier[s].team).unwrap_or(-1)
    }

    fn soldier_target(&self, e: i32) -> i32 {
        self.slot_of(e)
            .map(|s| self.soldier[s].target)
            .unwrap_or(-1)
    }

    /// Attack-move: hold fire while nothing hostile is in reach, engage the nearest enemy soldier
    /// when one is, resume walking when it dies. Composed from the health machinery that already
    /// exists, so the only new work here is the acquire scan.
    ///
    /// The scan is **staggered**: a unit re-acquires every `ACQUIRE_PERIOD` frames on a phase
    /// derived from its slot, so twenty units never scan on the same frame. That turns an O(n²)
    /// every frame into O(n²/8) with no behavioural difference a player can see.
    fn soldier_system(&mut self) {
        const ACQUIRE_PERIOD: i32 = 8;
        let n = self.alive.len();
        for s in 0..n {
            if !self.alive[s] || self.mask[s] & C_SLEEP != 0 || !self.has(s, C_SOLDIER) {
                continue;
            }
            if self.soldier[s].timer > 0 {
                self.soldier[s].timer -= 1;
            }
            if self.stun[s] > 0 {
                continue;
            }
            let (sx, sy) = self.seek_centre(s);
            let team = self.soldier[s].team;
            let range = self.soldier[s].range;
            // A unit ACQUIRES far outside the distance at which it can swing, then walks the rest.
            // With one radius for both, a unit only ever fights what it physically bumps into: two
            // armies marched past each other on this exact bug (see examples/rts-select).
            let aggro = range * 4;

            // Drop a target that died, was recycled, or ran away. The give-up distance is wider than
            // the acquire one so a target hovering at the edge does not flicker on and off.
            let mut tgt = self.soldier[s].target;
            if tgt >= 0 {
                match self.slot_of(tgt) {
                    Some(ts) if self.alive[ts] && self.health_alive_slot(ts) => {
                        let (ox, oy) = self.seek_centre(ts);
                        if (ox - sx).abs() + (oy - sy).abs() > aggro + 16 {
                            tgt = -1;
                        }
                    }
                    _ => tgt = -1,
                }
            }

            self.soldier[s].recheck -= 1;
            if tgt < 0 && self.soldier[s].recheck <= 0 {
                self.soldier[s].recheck = ACQUIRE_PERIOD;
                let mut best = -1;
                let mut best_d = aggro + 1;
                for o in 0..n {
                    if o == s || !self.alive[o] || !self.has(o, C_SOLDIER) {
                        continue;
                    }
                    if self.soldier[o].team == team || !self.health_alive_slot(o) {
                        continue;
                    }
                    let (ox, oy) = self.seek_centre(o);
                    let d = (ox - sx).abs() + (oy - sy).abs();
                    if d <= best_d {
                        best_d = d;
                        best = encode(o as u32, self.gen[o]);
                    }
                }
                tgt = best;
            } else if tgt < 0 {
                // Not this unit's frame to scan — stagger by slot so the cost spreads.
                self.soldier[s].recheck = self.soldier[s].recheck.max(s as i32 & 7);
            }
            self.soldier[s].target = tgt;

            if tgt < 0 {
                continue;
            }
            let Some(ts) = self.slot_of(tgt) else {
                continue;
            };
            let (ox, oy) = self.seek_centre(ts);
            let (dx, dy) = (ox - sx, oy - sy);
            // Face the thing before doing anything to it, so both the walk and the swing read.
            if self.has(s, C_TOPDOWN) {
                self.topdown[s].facing = if dx.abs() > dy.abs() {
                    if dx < 0 {
                        2
                    } else {
                        3
                    }
                } else if dy < 0 {
                    1
                } else {
                    0
                };
            }
            if dx.abs() + dy.abs() > range {
                // Closing. Steer straight at the target rather than down the flow field: the field
                // points at the ORDER, and the fight is somewhere else. This is short-range and
                // unobstructed by construction — anything further away was never acquired.
                if self.has(s, C_TOPDOWN) {
                    let td = &mut self.topdown[s];
                    td.dx = if dx.abs() <= 2 {
                        0
                    } else if dx > 0 {
                        1
                    } else {
                        -1
                    };
                    td.dy = if dy.abs() <= 2 {
                        0
                    } else if dy > 0 {
                        1
                    } else {
                        -1
                    };
                }
                continue;
            }
            // In range: stand and swing on the cooldown.
            if self.has(s, C_TOPDOWN) {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
            }
            if self.soldier[s].timer == 0 {
                let dmg = self.soldier[s].dmg;
                self.soldier[s].timer = self.soldier[s].cooldown;
                if dmg > 0 {
                    self.damage(tgt, dmg);
                }
            }
        }
    }

    fn health_alive_slot(&self, s: usize) -> bool {
        !self.has(s, C_HEALTH) || self.health[s].hp > 0
    }

    fn set_vision(&mut self, e: i32, radius: i32) {
        if let Some(s) = self.slot_of(e) {
            self.vision[s] = Vision {
                radius: radius.max(0),
                last_col: i32::MIN,
                last_row: i32::MIN,
            };
            if radius > 0 {
                self.mask[s] |= C_VISION;
                self.used |= C_VISION;
            } else {
                self.mask[s] &= !C_VISION;
            }
        }
    }

    fn fog_init(&mut self, cols: i32, rows: i32) {
        let cells = (cols.max(0) * rows.max(0)) as usize;
        self.fog.cols = cols;
        self.fog.rows = rows;
        self.fog.state = alloc::vec![0u8; cells];
        // `shown`/`win` are indexed by the 256 cells of the wrapping shroud LAYER, not by the map.
        // They start at values nothing can match, so the first blit paints the whole window once and
        // every blit after it paints only what changed.
        self.fog.shown = alloc::vec![0xFFu8; 256];
        self.fog.win = alloc::vec![-1i32; 256];
        self.fog.on = cells > 0;
        self.fog_settled = false;
    }

    fn fog_reveal(&mut self, col: i32, row: i32, radius: i32) {
        if !self.fog.on {
            return;
        }
        let (cols, rows) = (self.fog.cols, self.fog.rows);
        let rr = radius * radius;
        for dr in -radius..=radius {
            let r = row + dr;
            if r < 0 || r >= rows {
                continue;
            }
            let base = r * cols;
            for dc in -radius..=radius {
                let c = col + dc;
                if c < 0 || c >= cols {
                    continue;
                }
                if dc * dc + dr * dr <= rr {
                    self.fog.state[(base + c) as usize] = FOG_VISIBLE;
                }
            }
        }
    }

    /// Demote last frame's visible cells to "explored", then re-stamp from every seeing entity.
    /// Explored never goes back to unseen — that is the difference between fog and a black screen.
    fn fog_system(&mut self) {
        if !self.fog.on {
            return;
        }
        // Nothing to do unless a seeing entity crossed a CELL boundary.
        //
        // The sweep below is O(map) — 1,040 cells on a 40x26 map — and it recomputes an identical
        // answer on every frame where nobody moved far enough to change what is visible. Units walk
        // at 1px/frame and cells are 16px, so a given unit changes cell about once every 16 frames;
        // this skips the great majority of frames outright.
        //
        // (The obvious alternative — tracking lit cells in a list and demoting only those — was
        // built and measured WORSE: `world_step` 2,121 -> 2,550, because pushing ~300 bounds-checked
        // indices costs more than sweeping 1,040 bytes. Skipping whole frames is the win; making
        // the sweep itself incremental is not.)
        let mut moved = false;
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask[s] & C_SLEEP != 0 || !self.has(s, C_VISION | C_TRANSFORM)
            {
                continue;
            }
            let (cx, cy) = self.seek_centre(s);
            let (col, row) = (cx >> 4, cy >> 4);
            if self.vision[s].last_col != col || self.vision[s].last_row != row {
                self.vision[s].last_col = col;
                self.vision[s].last_row = row;
                moved = true;
            }
        }
        if !moved && self.fog_settled {
            return;
        }
        self.fog_settled = true;

        // A flat sweep of the whole state array, deliberately.
        //
        // The O(map) cost here looks wrong — 1,040 cells a frame on a 40x26 map, independent of
        // what is happening — so it was rewritten to remember which cells were lit and demote only
        // those. That measured WORSE: `world_step` went 2,121 -> 2,550. A vision disc is ~300 cells
        // across four units, and pushing 300 bounds-checked i32s costs more than sweeping 1,040
        // bytes linearly. Left as a sweep; do not "optimise" it again without measuring.
        for v in self.fog.state.iter_mut() {
            if *v == FOG_VISIBLE {
                *v = FOG_EXPLORED;
            }
        }
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask[s] & C_SLEEP != 0 || !self.has(s, C_VISION | C_TRANSFORM)
            {
                continue;
            }
            let (cx, cy) = self.seek_centre(s);
            let col = cx >> 4;
            let row = cy >> 4;
            let r = self.vision[s].radius;
            self.fog_reveal(col, row, r);
        }
    }

    /// Push changed fog cells to a shroud background layer, and return how many cells were written.
    ///
    /// The shroud is a **streamed window**, not a whole-map layer, because a `tilemap_new`
    /// background is `Background32x32` — 32×32 8px tiles = 16×16 of our 16px cells — and it WRAPS.
    /// A map any bigger than 256×256px cannot be painted into one. That wrap is not the problem, it
    /// is the mechanism: map cell (c,r) is painted at BG cell (c & 15, r & 15), and since the screen
    /// is only 15×10 cells no two visible cells ever collide. Point the layer at the camera with
    /// `bg_parallax(bg, 256, 256)` and the wrap lines up on its own.
    ///
    /// `shown` therefore tracks what each of the 256 BG cells is currently displaying — the map cell
    /// AND its state packed together — so a cell is rewritten either when its fog changes or when
    /// the camera scrolls a different map cell onto it. `tileset` supplies three 16px cells:
    /// **1 = opaque shroud, 2 = dithered half-shroud**; a visible cell is written blank.
    #[allow(clippy::too_many_arguments)]
    fn fog_blit(
        &mut self,
        bg: i32,
        tileset: i32,
        ts_cols: i32,
        cam_x: i32,
        cam_y: i32,
        gid_unseen: i32,
        gid_explored: i32,
    ) -> i32 {
        if !self.fog.on {
            return 0;
        }
        if self.fog.shown.len() < 256 {
            self.fog.shown = alloc::vec![0xFFu8; 256];
            self.fog.win = alloc::vec![-1i32; 256];
        }
        let (cols, rows) = (self.fog.cols, self.fog.rows);
        let c0 = cam_x >> 4;
        let r0 = cam_y >> 4;
        let mut wrote = 0;
        // 16×11 cells covers a 240×160 screen plus the partial row/column at each edge.
        for dr in 0..11 {
            let r = r0 + dr;
            if r < 0 || r >= rows {
                continue;
            }
            for dc in 0..16 {
                let c = c0 + dc;
                if c < 0 || c >= cols {
                    continue;
                }
                let map_i = r * cols + c;
                let st = self.fog.state[map_i as usize];
                let bgi = (((r & 15) * 16) + (c & 15)) as usize;
                if self.fog.win[bgi] == map_i && self.fog.shown[bgi] == st {
                    continue;
                }
                self.fog.win[bgi] = map_i;
                self.fog.shown[bgi] = st;
                let gid = match st {
                    FOG_VISIBLE => 0,
                    FOG_EXPLORED => gid_explored,
                    _ => gid_unseen,
                };
                tish_agb::native_tilemap_set(bg, tileset, ts_cols, c & 15, r & 15, gid);
                wrote += 1;
            }
        }
        wrote
    }

    /// Rest a top-down collider on the center of the 16px tile under its center.
    /// Same formula as snap_mode 2 targets — pad = 8 - w/2 (e.g. 12px box → +2).
    fn snap_topdown_to_tile(&mut self, s: usize) {
        if !self.has(s, C_TRANSFORM | C_COLLIDER) {
            return;
        }
        let w = self.collider[s].w;
        let h = self.collider[s].h;
        let cx = self.transform[s].x.floor() + w.floor() / 2;
        let cy = self.transform[s].y.floor() + h.floor() / 2;
        let col = cx.div_euclid(TILE);
        let row = cy.div_euclid(TILE);
        self.transform[s].x = Fixed::from_raw(col * TILE * 256) + Fixed::from_raw(8 * 256) - w / 2;
        self.transform[s].y = Fixed::from_raw(row * TILE * 256) + Fixed::from_raw(8 * 256) - h / 2;
    }

    /// Native hopper system: moves 8px, snaps to grid, and pauses.
    fn hopper_system(&mut self) {
        let Some(target) = self.camera_target else {
            return;
        };
        let Some(_ts) = self.slot_of(target) else {
            return;
        };

        let lp = self.lure_point();
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_HOPPER | C_TOPDOWN) || !self.is_active(s) {
                continue;
            }
            if self.stun[s] > 0 {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }

            // Similar room confinement check to chase_system
            let same_room = match self.camera_target.and_then(|e| self.slot_of(e)) {
                Some(ts) => self.same_room(s, ts),
                None => true,
            };
            if !same_room {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }

            let h = &mut self.hopper[s];
            let px = self.transform[s].x;
            let py = self.transform[s].y;

            if h.state == 0 {
                // idle
                h.timer -= 1;
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                if h.timer <= 0 {
                    h.state = 1;
                    h.timer = 32; // max frames to complete the 16px hop
                    h.start_x = px;
                    h.start_y = py;

                    // Baited: take the hop toward the food rather than a random one. Still a hop —
                    // the movement stays the enemy's own, only the direction is chosen.
                    if let Some((lx, ly)) = self.lured_to(s, lp) {
                        let ddx = lx - px.floor();
                        let ddy = ly - py.floor();
                        if ddx.abs() > ddy.abs() {
                            self.hopper[s].dir_x = ddx.signum();
                            self.hopper[s].dir_y = 0;
                        } else {
                            self.hopper[s].dir_x = 0;
                            self.hopper[s].dir_y = ddy.signum();
                        }
                        continue;
                    }
                    self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let r = (self.rng >> 16) % 4;

                    if r == 0 {
                        self.hopper[s].dir_x = 1;
                        self.hopper[s].dir_y = 0;
                    } else if r == 1 {
                        self.hopper[s].dir_x = -1;
                        self.hopper[s].dir_y = 0;
                    } else if r == 2 {
                        self.hopper[s].dir_x = 0;
                        self.hopper[s].dir_y = 1;
                    } else {
                        self.hopper[s].dir_x = 0;
                        self.hopper[s].dir_y = -1;
                    }
                }
            } else {
                // moving
                let dist = (px - h.start_x).abs() + (py - h.start_y).abs();
                if dist >= Fixed::from_raw(16 * 256) || h.timer <= 0 {
                    h.state = 0;
                    h.timer = 30 + (px.floor() % 30).abs();
                    h.dir_x = 0;
                    h.dir_y = 0;
                    self.topdown[s].dx = 0;
                    self.topdown[s].dy = 0;
                    self.topdown[s].snap_dx = 0;
                    self.topdown[s].snap_dy = 0;
                    // Tile center when snap-locked; legacy 8px half-tile otherwise.
                    if self.topdown[s].snap_mode == TD_SNAP_TILE {
                        self.snap_topdown_to_tile(s);
                    } else {
                        let snap_x = (px.floor() + 4) & !7;
                        let snap_y = (py.floor() + 4) & !7;
                        self.transform[s].x = Fixed::from_raw(snap_x * 256);
                        self.transform[s].y = Fixed::from_raw(snap_y * 256);
                    }
                } else {
                    h.timer -= 1;
                }
            }

            // While a hop is in flight, drive the intent EVERY frame (the mover
            // consumed last frame's) — mirrors how chase_system stays in motion.
            if self.hopper[s].state == 1 {
                self.topdown[s].dx = self.hopper[s].dir_x;
                self.topdown[s].dy = self.hopper[s].dir_y;
            }
            let td = &mut self.topdown[s];
            let c_stride = self.hopper[s].stride;
            if c_stride > 0 {
                let mx = td.dx;
                let my = td.dy;
                if mx < 0 {
                    td.facing = 2;
                } else if mx > 0 {
                    td.facing = 3;
                } else if my < 0 {
                    td.facing = 1;
                } else if my > 0 {
                    td.facing = 0;
                }

                let base = td.facing * c_stride;
                let moving = mx != 0 || my != 0;
                let e = encode(s as u32, self.gen[s]);
                if moving {
                    self.anim_play(e, base + 1, 4, 8, true);
                } else {
                    self.anim_play(e, base, 1, 1, false);
                }
            } else {
                let e = encode(s as u32, self.gen[s]);
                self.anim_play(e, 0, 2, 6, true);
            }
        }
    }

    /// Native jumper system: parabolic jump over walls.
    fn jumper_system(&mut self) {
        let Some(target) = self.camera_target else {
            return;
        };
        let Some(_ts) = self.slot_of(target) else {
            return;
        };

        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_JUMPER | C_TOPDOWN) || !self.is_active(s) {
                continue;
            }
            if self.stun[s] > 0 {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }

            let same_room = match self.camera_target.and_then(|e| self.slot_of(e)) {
                Some(ts) => self.same_room(s, ts),
                None => true,
            };
            if !same_room {
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                continue;
            }

            let px = self.transform[s].x;
            let _py = self.transform[s].y;

            if self.jumper[s].state == 0 {
                // idle
                self.jumper[s].timer -= 1;
                self.topdown[s].dx = 0;
                self.topdown[s].dy = 0;
                if self.jumper[s].timer <= 0 {
                    self.jumper[s].state = 1; // jumping
                    self.jumper[s].z = Fixed::from_raw(0);
                    self.jumper[s].dz = Fixed::from_raw(400); // upward velocity

                    self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let r = (self.rng >> 16) % 8;

                    if r == 0 {
                        self.jumper[s].dx = Fixed::from_raw(2 * 256);
                        self.jumper[s].dy = Fixed::from_raw(2 * 256);
                    } else if r == 1 {
                        self.jumper[s].dx = Fixed::from_raw(-2 * 256);
                        self.jumper[s].dy = Fixed::from_raw(2 * 256);
                    } else if r == 2 {
                        self.jumper[s].dx = Fixed::from_raw(-2 * 256);
                        self.jumper[s].dy = Fixed::from_raw(-2 * 256);
                    } else if r == 3 {
                        self.jumper[s].dx = Fixed::from_raw(2 * 256);
                        self.jumper[s].dy = Fixed::from_raw(-2 * 256);
                    } else if r == 4 {
                        self.jumper[s].dx = Fixed::from_raw(2 * 256);
                        self.jumper[s].dy = Fixed::from_raw(0);
                    } else if r == 5 {
                        self.jumper[s].dx = Fixed::from_raw(-2 * 256);
                        self.jumper[s].dy = Fixed::from_raw(0);
                    } else if r == 6 {
                        self.jumper[s].dx = Fixed::from_raw(0);
                        self.jumper[s].dy = Fixed::from_raw(2 * 256);
                    } else {
                        self.jumper[s].dx = Fixed::from_raw(0);
                        self.jumper[s].dy = Fixed::from_raw(-2 * 256);
                    }
                }
            } else {
                // jumping
                // apply jump manual movement, bypassing collision
                let dx = self.jumper[s].dx;
                let dy = self.jumper[s].dy;
                self.transform[s].x += dx;
                self.transform[s].y += dy;
                let dz = self.jumper[s].dz;
                self.jumper[s].z += dz;
                self.jumper[s].dz -= Fixed::from_raw(40); // gravity

                if self.jumper[s].z <= Fixed::from_raw(0) {
                    self.jumper[s].z = Fixed::from_raw(0);
                    self.jumper[s].state = 0;
                    self.jumper[s].timer = 30 + (px.floor() % 60).abs();
                    if self.topdown[s].snap_mode == TD_SNAP_TILE {
                        self.snap_topdown_to_tile(s);
                    } else {
                        let snap_x = (self.transform[s].x.floor() + 4) & !7;
                        let snap_y = (self.transform[s].y.floor() + 4) & !7;
                        self.transform[s].x = Fixed::from_raw(snap_x * 256);
                        self.transform[s].y = Fixed::from_raw(snap_y * 256);
                    }
                }

                if self.has(s, C_SPRITE) {
                    self.sprite[s].oy = -self.jumper[s].z.floor();
                }
            }

            let e = encode(s as u32, self.gen[s]);
            if self.jumper[s].state == 1 {
                self.anim_play(e, 1, 1, 1, false);
            } else {
                self.anim_play(e, 0, 1, 1, false);
            }
        }
    }
    /// Set the move speed in px/frame (default 1.25). Keep it below one tile (16) so the single-step
    /// collision clamp stays exact.
    fn topdown_speed(&mut self, e: i32, px: f64) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TOPDOWN)) {
            self.topdown[s].speed = (px * 256.0) as i32;
        }
    }

    /// Set the move intent this frame (dx/dy ∈ {-1,0,1}); call every frame from input or AI. 0/0
    /// stops. Also updates the persistent facing (horizontal wins, matching the 4-row char sheets).
    /// No-op while a room slide has this entity's input locked.
    fn topdown_move(&mut self, e: i32, dx: i32, dy: i32) {
        if self.input_locked(e) {
            return;
        }
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TOPDOWN)) {
            let (dx, dy) = (dx.signum(), dy.signum());
            self.topdown[s].dx = dx;
            self.topdown[s].dy = dy;
            if dx < 0 {
                self.topdown[s].facing = 2;
            } else if dx > 0 {
                self.topdown[s].facing = 3;
            } else if dy < 0 {
                self.topdown[s].facing = 1;
            } else if dy > 0 {
                self.topdown[s].facing = 0;
            }
        }
    }

    /// Briefly shove the entity in (dx, dy), overriding input for `TD_KNOCK_FRAMES` frames — the
    /// classic hit-reaction. `power` is the raw fixed impulse per axis (0 = the default `TD_KNOCK`).
    fn topdown_knockback(&mut self, e: i32, dx: i32, dy: i32, power: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TOPDOWN)) {
            let p = if power > 0 { power } else { TD_KNOCK };
            self.topdown[s].kx = Fixed::from_raw(dx.signum() * p);
            self.topdown[s].ky = Fixed::from_raw(dy.signum() * p);
            self.topdown[s].knock = TD_KNOCK_FRAMES;
        }
    }

    /// Top-down system: for each `C_TOPDOWN` entity, turn its move intent (or an active knockback)
    /// into a velocity, resolve its box against the solid grid axis-by-axis (no gravity), and clear
    /// the intent. Culled/room-frozen entities are skipped, like the other movers.
    fn topdown_system(&mut self) {
        let zero = Fixed::from_raw(0);
        let frozen = if self.room_cam.enabled && self.room_cam.transitioning {
            self.camera_target.and_then(|e| self.slot_of(e))
        } else {
            None
        };
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_TOPDOWN | C_TRANSFORM | C_COLLIDER) {
                continue;
            }
            if Some(s) == frozen || !self.is_active(s) {
                continue;
            }
            let mut td = self.topdown[s];

            if td.snap_mode == TD_SNAP_TILE
                && td.snap_dx == 0
                && td.snap_dy == 0
                && (td.dx != 0 || td.dy != 0)
            {
                // Idle and got input: start a new snap (ortho only)
                if td.dx != 0 {
                    td.dy = 0;
                }
                td.snap_dx = td.dx;
                td.snap_dy = td.dy;

                let base_x =
                    self.transform[s].x - Fixed::from_raw(8 * 256) + self.collider[s].w / 2;
                let base_y =
                    self.transform[s].y - Fixed::from_raw(8 * 256) + self.collider[s].h / 2;

                let col = if td.dx > 0 {
                    base_x.floor().div_euclid(TILE) + 1
                } else if td.dx < 0 {
                    (base_x.to_raw() + TILE * 256 - 1).div_euclid(TILE * 256) - 1
                } else {
                    (base_x + Fixed::from_raw(8 * 256)).floor().div_euclid(TILE)
                };

                let row = if td.dy > 0 {
                    base_y.floor().div_euclid(TILE) + 1
                } else if td.dy < 0 {
                    (base_y.to_raw() + TILE * 256 - 1).div_euclid(TILE * 256) - 1
                } else {
                    (base_y + Fixed::from_raw(8 * 256)).floor().div_euclid(TILE)
                };

                td.snap_target_x = Fixed::from_raw(col * TILE * 256) + Fixed::from_raw(8 * 256)
                    - self.collider[s].w / 2;
                td.snap_target_y = Fixed::from_raw(row * TILE * 256) + Fixed::from_raw(8 * 256)
                    - self.collider[s].h / 2;
                self.topdown[s] = td;
            }

            let (w, h) = (self.collider[s].w, self.collider[s].h);
            let had_intent = td.dx != 0 || td.dy != 0;
            // A knockback shove overrides input while it lasts; otherwise scale the intent by speed
            // (× TD_DIAG per axis on a diagonal so a diagonal isn't faster than a cardinal move).
            let (mut vx, mut vy) = if td.knock > 0 {
                (td.kx, td.ky)
            } else {
                let sp = if td.speed == 0 { TD_WALK } else { td.speed };
                let (use_dx, use_dy) =
                    if td.snap_mode == TD_SNAP_TILE && (td.snap_dx != 0 || td.snap_dy != 0) {
                        (td.snap_dx, td.snap_dy)
                    } else {
                        (td.dx, td.dy)
                    };
                let diag = use_dx != 0 && use_dy != 0;
                let axis = |d: i32| {
                    if d == 0 {
                        zero
                    } else {
                        let raw = if diag { (sp * TD_DIAG) / 256 } else { sp };
                        Fixed::from_raw(d.signum() * raw)
                    }
                };
                (axis(use_dx), axis(use_dy))
            };
            // X axis, then Y axis: clamp to the tile edge on a wall hit (speeds stay below a tile).
            let (x, y) = (self.transform[s].x, self.transform[s].y);
            // Chase AI must stay in its current room — doorway tiles are walkable for the player,
            // so without this clamp enemies walk through into adjacent rooms while chasing.
            let confine = self.room_cam.enabled && self.has(s, C_CHASE);
            let (rmin_x, rmax_x, rmin_y, rmax_y) = if confine {
                let tw = self.room_cam.room_w * TILE;
                let th = self.room_cam.room_h * TILE;
                // Room from pre-move center so knockback / chase can't push into the next room.
                let cx = x.floor() + w.floor() / 2;
                let cy = y.floor() + h.floor() / 2;
                let room_x = cx.div_euclid(tw);
                let room_y = cy.div_euclid(th);
                (
                    Fixed::from_raw(room_x * tw * 256),
                    Fixed::from_raw((room_x + 1) * tw * 256) - w,
                    Fixed::from_raw(room_y * th * 256),
                    Fixed::from_raw((room_y + 1) * th * 256) - h,
                )
            } else {
                (zero, zero, zero, zero)
            };
            // A body that is ALREADY inside a solid must be allowed to walk out of it. Every clamp
            // below tests the destination, and from inside a wall every destination is also inside
            // one — so both axes get zeroed, every frame, forever: the entity is stuck for good and
            // nothing reports it. Skipping the clamps while embedded lets it walk free, and the
            // frame it clears the tile the normal clamps take over again. A body resting AGAINST a
            // wall is unaffected: it is adjacent, not overlapping, and `box_hits_solid` is
            // inclusive-exclusive precisely so a box clamped onto a tile edge has no solid inside it.
            let embedded = self.box_hits_solid(x, y, w, h);
            let nx = x + vx;
            let mut rx = if embedded {
                nx
            } else if vx > zero && self.box_hits_solid(nx, y, w, h) {
                let col = (((nx + w).to_raw() - 1) >> 8).div_euclid(TILE);
                vx = zero;
                Fixed::from_raw(col * TILE * 256) - w
            } else if vx < zero && self.box_hits_solid(nx, y, w, h) {
                let col = nx.floor().div_euclid(TILE);
                vx = zero;
                Fixed::from_raw((col + 1) * TILE * 256)
            } else {
                nx
            };
            // Entity blockers (NPCs, …): snap to the hit box's edge on the axis we moved, same as a
            // solid tile — so the player's collision follows the NPC, not a leftover spawn tile.
            if vx != zero {
                if let Some((bx, _, bw, _)) = self.first_blocker_hit(s, rx, y, w, h) {
                    rx = if vx > zero { bx - w } else { bx + bw };
                    vx = zero;
                }
            }
            let ny = y + vy;
            let mut ry = if embedded {
                ny
            } else if vy > zero && self.box_hits_solid(rx, ny, w, h) {
                let row = (((ny + h).to_raw() - 1) >> 8).div_euclid(TILE);
                vy = zero;
                Fixed::from_raw(row * TILE * 256) - h
            } else if vy < zero && self.box_hits_solid(rx, ny, w, h) {
                let row = ny.floor().div_euclid(TILE);
                vy = zero;
                Fixed::from_raw((row + 1) * TILE * 256)
            } else {
                ny
            };
            if vy != zero {
                if let Some((_, by, _, bh)) = self.first_blocker_hit(s, rx, ry, w, h) {
                    ry = if vy > zero { by - h } else { by + bh };
                    vy = zero;
                }
            }
            if confine {
                if rx < rmin_x {
                    rx = rmin_x;
                } else if rx > rmax_x {
                    rx = rmax_x;
                }
                if ry < rmin_y {
                    ry = rmin_y;
                } else if ry > rmax_y {
                    ry = rmax_y;
                }
            }
            if td.snap_mode == TD_SNAP_TILE && (td.snap_dx != 0 || td.snap_dy != 0) {
                let over_x = if td.snap_dx > 0 {
                    rx >= td.snap_target_x
                } else {
                    rx <= td.snap_target_x
                };
                let over_y = if td.snap_dy > 0 {
                    ry >= td.snap_target_y
                } else {
                    ry <= td.snap_target_y
                };
                let hit_wall = (td.snap_dx != 0 && vx == zero) || (td.snap_dy != 0 && vy == zero);
                if (td.snap_dx != 0 && over_x) || (td.snap_dy != 0 && over_y) || hit_wall {
                    let match_intent = if td.snap_dx != 0 {
                        td.dx == td.snap_dx
                    } else {
                        td.dy == td.snap_dy
                    };
                    if !hit_wall && match_intent {
                        // Land exactly on this cell, then aim at the next. Without the clamp,
                        // speed that doesn't divide 16 leaves a permanent sub-tile drift while
                        // holding a direction — entities never rest on the floor grid.
                        rx = td.snap_target_x;
                        ry = td.snap_target_y;
                        self.topdown[s].snap_target_x =
                            td.snap_target_x + Fixed::from_raw(td.snap_dx * TILE * 256);
                        self.topdown[s].snap_target_y =
                            td.snap_target_y + Fixed::from_raw(td.snap_dy * TILE * 256);
                    } else {
                        if hit_wall {
                            // Wall clamp can leave a sub-tile offset — rest on the cell we occupy.
                            self.transform[s].x = rx;
                            self.transform[s].y = ry;
                            self.snap_topdown_to_tile(s);
                            rx = self.transform[s].x;
                            ry = self.transform[s].y;
                        } else {
                            rx = td.snap_target_x;
                            ry = td.snap_target_y;
                        }
                        self.topdown[s].snap_dx = 0;
                        self.topdown[s].snap_dy = 0;
                    }
                }
            }

            self.transform[s].x = rx;
            self.transform[s].y = ry;

            let td = &mut self.topdown[s];
            td.moving = td.knock <= 0 && had_intent; // walk anim even when pushing into a wall
            if td.knock > 0 {
                td.knock -= 1;
                if td.knock == 0 {
                    td.snap_dx = 0;
                    td.snap_dy = 0;
                }
            }
            // Hard idle lock: nothing with snap_mode 2 may rest between tiles (script teleports,
            // knock settle, wall clamps, hopper land). Mid-step / knock transit may be between.
            let knock_left = td.knock;
            let snap_busy = td.snap_dx != 0 || td.snap_dy != 0;
            let mode = td.snap_mode;
            td.dx = 0;
            td.dy = 0;
            if mode == TD_SNAP_TILE && knock_left == 0 && !snap_busy {
                self.snap_topdown_to_tile(s);
            }
        }
    }

    /// Spawn a short-lived melee hurt box in front of `attacker` (offset by its top-down facing): a
    /// `size`×`size` box `reach` px past the attacker's edge that deals `damage` to `target_tag`
    /// entities for `ttl` frames. The victim's i-frames make one swing land once. Invisible — the
    /// attacker's own attack animation is the visual. Returns the hitbox entity id.
    fn swing(
        &mut self,
        attacker: i32,
        target_tag: i32,
        damage: i32,
        reach: i32,
        size: i32,
        ttl: i32,
    ) -> i32 {
        let Some(s) = self.slot_of(attacker) else {
            return 0;
        };
        // ⚠️ A platformer has no top-down facing, and `0` means DOWN — so before this, every melee
        // swing in a side-scroller spawned its hurt box under the attacker's feet and the sword
        // visibly missed everything in front of it. `Platformer.face` is the sticky -1/+1 heading
        // (it keeps the last non-zero direction, unlike `dir`, which is this frame's intent), which
        // is exactly what a swing wants.
        let facing = if self.has(s, C_TOPDOWN) {
            self.topdown[s].facing
        } else if self.has(s, C_PLATFORMER) {
            if self.platformer[s].face < 0 {
                2
            } else {
                3
            }
        } else {
            0
        };
        let ax = self.transform[s].x.floor();
        let ay = self.transform[s].y.floor();
        let cw = self.collider[s].w.floor();
        let ch = self.collider[s].h.floor();
        let (cx, cy) = (ax + cw / 2, ay + ch / 2);
        let (bx, by) = match facing {
            1 => (cx - size / 2, ay - reach - size), // up
            2 => (ax - reach - size, cy - size / 2), // left
            3 => (ax + cw + reach, cy - size / 2),   // right
            _ => (cx - size / 2, ay + ch + reach),   // down
        };
        let e = self.spawn();
        if let Some(ss) = self.slot_of(e) {
            self.transform[ss] = Transform {
                x: to_fixed(bx as f64),
                y: to_fixed(by as f64),
            };
            self.collider[ss] = Collider {
                w: to_fixed(size as f64),
                h: to_fixed(size as f64),
            };
            self.hurt[ss] = Hurt {
                damage,
                target_tag,
                despawn_on_hit: false,
                stun: 0,
                damage_type: 0,
            };
            self.life[ss] = Life {
                ttl: ttl.max(1),
                offscreen: false,
            };
            self.mask[ss] |= C_TRANSFORM | C_COLLIDER | C_HURT | C_LIFE;
            self.used |= C_TRANSFORM | C_COLLIDER | C_HURT | C_LIFE;
        }
        e
    }

    // ── Health / combat ──────────────────────────────────────────────────────
    /// Give an entity `max` hit points (starts full). Enables `damage`/`heal` + the health system.
    fn set_health(&mut self, e: i32, max: i32, invuln_max: i32) {
        if let Some(s) = self.slot_of(e) {
            let m = max.max(1);
            self.health[s] = Health {
                hp: m,
                max: m,
                invuln: 0,
                invuln_max: invuln_max.max(0),
                dead: false,
            };
            self.mask[s] |= C_HEALTH;
            self.used |= C_HEALTH;
        }
    }

    /// Apply `amount` damage unless mid i-frames / already dead. A real hit starts the i-frame
    /// window; reaching 0 hp flags death (the health phase fires onDeath / despawns). Returns
    /// whether the hit landed.
    fn damage(&mut self, e: i32, amount: i32) -> bool {
        // Boss glue (all gated on mask2 bits, so an entity that never used the new natives
        // takes the plain path below unchanged):
        //   - phased/hidden (drifter in flight, burrower underground): the hit is refused outright.
        //   - closed vulnerability gate (the weak-point eye shut): refused.
        //   - hit proxy (boss neck → head, segment → current tail): the damage is
        //     re-routed, following at most 4 hops so a mis-wired cycle cannot hang the frame.
        //     Each hop's own phase/gate is honoured — routing INTO a closed part still refuses.
        let mut e = e;
        for _ in 0..4 {
            let Some(s) = self.slot_of(e) else { break };
            let m2 = self.mask2[s];
            if m2 & (M2_PHASED | M2_HIDDEN) != 0 {
                return false;
            }
            if m2 & M2_GATE != 0 && self.zx[s].gate == 0 {
                return false;
            }
            if m2 & M2_PROXY != 0 {
                let p = self.zx[s].proxy;
                if p >= 0 && p != e && self.slot_of(p).is_some() {
                    e = p;
                    continue;
                }
            }
            break;
        }
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_HEALTH)) {
            if self.health[s].invuln > 0 || self.health[s].dead || amount <= 0 {
                return false;
            }
            let h = &mut self.health[s];
            h.hp -= amount;
            h.invuln = h.invuln_max;
            if h.hp <= 0 {
                h.hp = 0;
                h.dead = true;
            }
            return true;
        }
        false
    }

    /// Heal up to `max`; no effect once dead.
    fn heal(&mut self, e: i32, amount: i32) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_HEALTH)) {
            let h = &mut self.health[s];
            if !h.dead {
                h.hp = (h.hp + amount.max(0)).min(h.max);
            }
        }
    }

    // ── Auto-despawn + contact damage (the shoot-'em-up core) ────────────────
    /// Give an entity a time-to-live: it despawns `ttl` frames from now (an explosion that clears
    /// once its clip finishes, a bullet with a bounded range). Preserves any off-screen flag.
    fn set_lifetime(&mut self, e: i32, ttl: i32) {
        if let Some(s) = self.slot_of(e) {
            let offscreen = self.has(s, C_LIFE) && self.life[s].offscreen;
            self.life[s] = Life {
                ttl: ttl.max(0),
                offscreen,
            };
            self.mask[s] |= C_LIFE;
            self.used |= C_LIFE;
        }
    }

    /// Mark an entity for off-screen cleanup: it despawns the moment its box leaves the visible area
    /// (bullets that fly off-screen, enemies that exit past the player). Preserves any TTL.
    fn set_despawn_offscreen(&mut self, e: i32, on: bool) {
        if let Some(s) = self.slot_of(e) {
            let ttl = if self.has(s, C_LIFE) {
                self.life[s].ttl
            } else {
                0
            };
            self.life[s] = Life { ttl, offscreen: on };
            self.mask[s] |= C_LIFE;
            self.used |= C_LIFE;
        }
    }

    /// `set_guard(e, mask)` — block damage arriving from the direction this entity FACES.
    ///
    /// `GUARD_MELEE` (1) stops swings and body contact, `GUARD_SHOT` (2) stops projectiles; a mask
    /// of 0 removes the guard. The genre needs both halves of the same rule — the player's shield
    /// stops what they walk into, an armoured knight stops the sword until you get behind it — so this is one
    /// component rather than two.
    ///
    /// Requires `C_TOPDOWN` for the facing. Without it there is no "front" and nothing is blocked.
    /// Animate `frames` frames starting at `base + facing * stride`, following the entity's facing.
    ///
    /// Facing is whatever the movement systems last set (0 down, 1 up, 2 left, 3 right), so this
    /// composes with `set_chase`, `set_hopper`, `set_jumper` or plain top-down movement — it reads
    /// the facing rather than producing it.
    fn set_dir_anim(&mut self, e: i32, base: i32, stride: i32, frames: i32, speed: i32) {
        if let Some(s) = self.slot_of(e) {
            self.diranim[s] = DirAnim {
                base,
                stride: stride.max(0),
                frames: frames.max(1),
                speed: speed.max(1),
            };
            self.mask[s] |= C_DIRANIM;
            self.used |= C_DIRANIM;
        }
    }

    /// Re-point each directional entity's clip at its current facing, every frame.
    ///
    /// `anim_play` is idempotent for an unchanged clip, so calling it every frame costs a compare
    /// and only restarts the cycle when the facing actually changes — which is the behaviour you
    /// want anyway: turning a corner should show the new direction's first frame at once.
    fn diranim_system(&mut self) {
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask[s] & C_SLEEP != 0 || !self.has(s, C_DIRANIM | C_TOPDOWN)
            {
                continue;
            }
            let d = self.diranim[s];
            let facing = self.topdown[s].facing.clamp(0, 3);
            let e = encode(s as u32, self.gen[s]);
            self.anim_play(e, d.base + facing * d.stride, d.frames, d.speed, true);
        }
    }

    fn set_guard(&mut self, e: i32, mask: i32) {
        if let Some(s) = self.slot_of(e) {
            self.guard[s] = mask;
            if mask == 0 {
                self.mask[s] &= !C_GUARD;
            } else {
                self.mask[s] |= C_GUARD;
                self.used |= C_GUARD;
            }
        }
    }

    /// Does slot `t`'s guard stop a hit whose hurt box is centred at (hcx, hcy)?
    ///
    /// `despawn_on_hit` is what tells a projectile from a swing: in this engine a hurt box consumed
    /// by its own hit IS a bullet, and one that survives is a sword arc or a body. That is a real
    /// invariant of `set_hurt`/`swing` rather than a guess, and it keeps the guard from needing a
    /// new field on every hurt box in the game.
    ///
    /// The direction test uses the DOMINANT axis, so a hit from the side is not blocked by a front
    /// guard — walking into a spitter still hurts, and an armoured knight can still be flanked.
    fn guard_blocks(&self, t: usize, hcx: i32, hcy: i32, is_shot: bool) -> bool {
        let want = if is_shot { GUARD_SHOT } else { GUARD_MELEE };
        if self.guard[t] & want == 0 || !self.has(t, C_TOPDOWN) {
            return false;
        }
        let vcx = self.transform[t].x.floor() + self.collider[t].w.floor() / 2;
        let vcy = self.transform[t].y.floor() + self.collider[t].h.floor() / 2;
        let (dx, dy) = (hcx - vcx, hcy - vcy);
        match self.topdown[t].facing {
            1 => dy < 0 && dy.abs() >= dx.abs(),
            2 => dx < 0 && dx.abs() >= dy.abs(),
            3 => dx > 0 && dx.abs() >= dy.abs(),
            _ => dy > 0 && dy.abs() >= dx.abs(),
        }
    }

    /// Make an entity a contact hurt box (bullet / body hazard). On overlap with a `target_tag`
    /// entity that has health it deals `damage`; `despawn_on_hit` consumes a bullet on contact.
    fn set_hurt(&mut self, e: i32, damage: i32, target_tag: i32, despawn_on_hit: bool, stun: i32) {
        if let Some(s) = self.slot_of(e) {
            self.hurt[s] = Hurt {
                damage,
                target_tag,
                despawn_on_hit,
                stun,
                damage_type: 0,
            };
            self.mask[s] |= C_HURT;
            self.used |= C_HURT;
        }
    }

    // ── Native bullet emitters (the bullet-hell hot path) ────────────────────
    // A pattern like a 16-bullet ring used to cost, per bullet, a boxed tish call chain
    // (fireAngle → fireBullet → mkSprite + 6× default-pick + a dozen `set_*`) — all `Value`
    // dispatch. These build the whole pure-component bullet natively from the current
    // `bullet_style`, so a ring is one native call plus one `Value` opts-resolve, not sixteen.

    fn set_bullet_style(
        &mut self,
        sheet: i32,
        frame: i32,
        size: i32,
        damage: i32,
        target: i32,
        tag: i32,
        ttl: i32,
    ) {
        // Preserve the weapon type across a style change: `bullet_damage_type` is set beside
        // `bullet_style` by the caller, and the two orderings are equally natural to write.
        let dt = self.bullet_style.damage_type;
        self.bullet_style = BulletStyle {
            sheet,
            frame,
            size,
            damage,
            target,
            tag,
            ttl,
            damage_type: dt,
        };
    }

    /// Spawn one bullet at screen point (`cx`,`cy`) with velocity (`vx`,`vy`), configured by the
    /// current `bullet_style`. A pure-component entity: it flies (`Body`), damages `style.target`
    /// (`Hurt`), and auto-despawns on hit / off-screen / after `style.ttl` — no per-frame tish tick.
    fn fire_bullet(&mut self, cx: Fixed, cy: Fixed, vx: Fixed, vy: Fixed) -> i32 {
        let st = self.bullet_style;
        let size_f = Fixed::from_raw(st.size * 256);
        let half = size_f / 2;
        // Off-screen bullets get their VRAM released just like `attach_sprite` does; `render_system`
        // restores the sprite the moment the bullet is on screen, so a dense pattern doesn't try to
        // hold a VRAM sprite for every bullet at spawn time.
        let sp = tish_agb::sprite_new_typed(st.sheet);
        tish_agb::sprite_set_frame_typed(sp, st.frame);
        let off = (st.size - 16) / 2;
        let e = self.spawn();
        if let Some(s) = self.slot_of(e) {
            self.transform[s] = Transform {
                x: cx - half,
                y: cy - half,
            };
            self.collider[s] = Collider {
                w: size_f,
                h: size_f,
            };
            self.body[s] = Body { vx, vy };
            self.sprite[s] = SpriteRef {
                handle: sp,
                ox: off,
                oy: off,
            };
            self.tag[s] = st.tag;
            self.mask[s] |= C_TRANSFORM | C_COLLIDER | C_BODY | C_SPRITE;
            self.used |= C_TRANSFORM | C_COLLIDER | C_BODY | C_SPRITE;
        }
        self.set_hurt(e, st.damage, st.target, true, 0);
        if st.damage_type != 0 {
            if let Some(s) = self.slot_of(e) {
                self.hurt[s].damage_type = st.damage_type;
            }
        }
        self.set_lifetime(e, st.ttl);
        self.set_despawn_offscreen(e, true);
        // NB: unlike `attach_sprite` we do NOT release the sprite's VRAM here. Bullets spawn on-screen
        // and live briefly, so releasing now only to have `render_system` re-create the Object next
        // frame is pure churn; `set_despawn_offscreen` reclaims it the moment the bullet exits.
        e
    }

    /// Fire one bullet at a heading in degrees (0 = right, 90 = down, −90 = up) and scalar `speed`.
    /// agb's `Num::{cos,sin}` take revolutions, so degrees fold to `deg/360`; with the screen's
    /// y-down axis those components are already the correct velocity (no sign flips).
    fn fire_angle(&mut self, cx: Fixed, cy: Fixed, deg: Fixed, speed: Fixed) -> i32 {
        let rev = deg / Fixed::from_raw(360 * 256);
        self.fire_bullet(cx, cy, speed * rev.cos(), speed * rev.sin())
    }

    /// A full ring of `count` bullets evenly spaced around (`cx`,`cy`) at `speed` — the staple.
    fn fire_ring(&mut self, cx: Fixed, cy: Fixed, count: i32, speed: Fixed) {
        if count <= 0 {
            return;
        }
        let count_f = Fixed::from_raw(count * 256);
        for k in 0..count {
            let rev = Fixed::from_raw(k * 256) / count_f;
            self.fire_bullet(cx, cy, speed * rev.cos(), speed * rev.sin());
        }
    }

    /// A fan of `count` bullets `spread_deg` wide, centred on `center_deg`.
    fn fire_spread(
        &mut self,
        cx: Fixed,
        cy: Fixed,
        center_deg: Fixed,
        count: i32,
        spread_deg: Fixed,
        speed: Fixed,
    ) {
        if count <= 0 {
            return;
        }
        if count == 1 {
            self.fire_angle(cx, cy, center_deg, speed);
            return;
        }
        let start = center_deg - spread_deg / 2;
        let step = spread_deg / Fixed::from_raw((count - 1) * 256);
        for k in 0..count {
            let deg = start + step * Fixed::from_raw(k * 256);
            self.fire_angle(cx, cy, deg, speed);
        }
    }

    /// Fire one bullet from (`cx`,`cy`) aimed at (`tox`,`toy`) at `speed`. Normalises the direction
    /// vector directly (a `sqrt`) — no `atan2`, no angle round-trip.
    fn fire_aimed(&mut self, cx: Fixed, cy: Fixed, tox: Fixed, toy: Fixed, speed: Fixed) -> i32 {
        let dx = tox - cx;
        let dy = toy - cy;
        let len = (dx * dx + dy * dy).sqrt();
        if len.to_raw() == 0 {
            return self.fire_bullet(cx, cy, Fixed::from_raw(0), speed);
        }
        self.fire_bullet(cx, cy, speed * dx / len, speed * dy / len)
    }

    fn set_topdown_snap(&mut self, e: i32, mode: u8) {
        if let Some(s) = self.slot_of(e).filter(|&s| self.has(s, C_TOPDOWN)) {
            self.topdown[s].snap_mode = mode;
        }
    }

    /// Give an entity a native movement pattern (also ensures it has a `Body` to drive). `pattern`
    /// 0 = straight down at `base_vy`; 1 = weave (sideways triangle of amplitude `amp` over `period`
    /// frames while descending at `base_vy`).
    fn set_mover(&mut self, e: i32, pattern: u8, base_vy: Fixed, amp: Fixed, period: i32) {
        if let Some(s) = self.slot_of(e) {
            self.mover[s] = Mover {
                pattern,
                t: 0,
                base_vy,
                amp,
                period,
            };
            self.body[s] = Body {
                vx: Fixed::from_raw(0),
                vy: base_vy,
            };
            self.mask[s] |= C_MOVER | C_BODY;
            self.used |= C_MOVER | C_BODY;
        }
    }

    /// Movement-pattern system (pipeline phase 2, before `movement_system` integrates): drive each
    /// mover's `Body` velocity in pure Rust. No per-frame tish `tick` — a screen full of weaving
    /// enemies is as cheap as the native patrol AI. Off-screen movers are skipped like everything else.
    fn mover_system(&mut self) {
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_MOVER | C_BODY) || !self.is_active(s) {
                continue;
            }
            let m = self.mover[s];
            let t = m.t + 1;
            self.mover[s].t = t;
            let vx = if m.pattern == 1 {
                tri_fixed(t, m.period, m.amp)
            } else {
                Fixed::from_raw(0)
            };
            self.body[s].vx = vx;
            self.body[s].vy = m.base_vy;
        }
    }

    /// Lifetime system (pipeline phase 2): tick TTLs down and despawn anything that expired or (if
    /// flagged) has left the screen. Runs before collision so a spent bullet doesn't hit or render.
    /// Uses the same camera-relative bounds as culling, so it works with or without a scrolling camera
    /// (a fixed-screen shmup has `cam=(0,0)` ⇒ the plain 240×160 view plus the cull margin).
    fn life_system(&mut self) {
        let mut dead: Vec<i32> = Vec::new();
        let mut pooled: Vec<i32> = Vec::new();
        for s in 0..self.alive.len() {
            if !self.alive[s] || self.mask[s] & C_SLEEP != 0 || !self.has(s, C_LIFE) {
                continue;
            }
            let mut remove = false;
            if self.life[s].ttl > 0 {
                self.life[s].ttl -= 1;
                if self.life[s].ttl == 0 {
                    remove = true;
                }
            }
            if !remove && self.life[s].offscreen && self.has(s, C_TRANSFORM) && !self.on_screen(s) {
                remove = true;
            }
            // A flying hurt box (thrown star, shot) is stopped by the level: once the game has a
            // collision grid, retire it the frame its box overlaps a solid tile, so a projectile
            // can't fly through a dungeon wall into the next room. A shmup sets up no grid (every
            // cell would read solid), and a body hazard like a charging enemy has no `Body`.
            if !remove
                && self.grid_cols > 0
                && self.has(s, C_BODY | C_HURT | C_TRANSFORM | C_COLLIDER)
            {
                let (t, c) = (self.transform[s], self.collider[s]);
                if self.box_hits_solid(t.x, t.y, c.w, c.h) {
                    remove = true;
                }
            }
            // Room cutoff: doorway tiles are walkable, so the solid check above won't catch a star
            // sailing through an open door. With a room camera, retire any flying hurt box the
            // moment it leaves the player's current room — hard cutoff, no next-room hits.
            if !remove
                && self.room_cam.enabled
                && self.has(s, C_BODY | C_HURT | C_TRANSFORM)
                && !self.in_current_room(s)
            {
                remove = true;
            }
            if remove {
                // ⚠️ A POOLED ENTITY IS PARKED, NOT DESPAWNED. This one branch is why the pool needs
                // no ttl column and no system of its own: every rule above — the timer, off-screen,
                // a hurt box meeting a solid tile, the room cutoff — now retires a pool slot instead
                // of freeing its sprite VRAM and pushing the entity onto the free list, which is the
                // exact churn a pool exists to avoid.
                //
                // It also fixes something by arriving: the hand-rolled sub-weapon pools counted
                // their own ttl and never called `set_lifetime`, so `life_system` skipped them
                // entirely and their shots flew through walls into the next room.
                if self.pool_of[s] >= 0 {
                    pooled.push(self.pool_of[s]);
                } else {
                    dead.push(encode(s as u32, self.gen[s]));
                }
            }
        }
        for p in pooled {
            self.pool_retire_packed(p);
        }
        for e in dead {
            self.despawn(e);
        }
    }

    /// Combat system (pipeline phase 2): every hurt box vs every health entity carrying its target
    /// tag. On overlap: deal damage (honouring the victim's i-frames) and, for a bullet, despawn it.
    /// All native — a bullet never needs a tish callback, so a bullet-hell frame stays affordable.
    /// Death itself is flagged in `Health` and dispatched by the normal `collect_deaths` phase, so an
    /// enemy killed by a bullet still runs its tish `onDeath` (score, explosion) exactly once.
    fn combat_system(&mut self) {
        let n = self.alive.len();
        // Gather (victim, damage) hits and (bullet) despawns under an immutable read, then apply —
        // avoids aliasing `self` while iterating, and matches the deferred style of the other systems.
        let mut hits: Vec<(i32, i32, i32, i32)> = Vec::new(); // (victim id, damage, hurt cx, hurt cy)
        let mut consumed: Vec<i32> = Vec::new();
        // Health boxes (player, enemies, boss) are FEW; hurt boxes (bullets) can number in the
        // hundreds during bullet-hell. Collect the victim slots once so the inner test is
        // O(hurt × health) instead of O(hurt × N) — the whole-slab rescan was the cost that grew
        // as the level filled with bullets.
        let mut victims: Vec<usize> = Vec::new();
        for t in 0..n {
            // `is_active` matters twice here: a sleeping pooled unit still carries
            // C_HEALTH|C_TRANSFORM|C_COLLIDER, so without it a dormant slot is tested by every
            // hurt box AND can take damage while parked in the pool.
            if self.alive[t] && self.is_active(t) && self.has(t, C_HEALTH | C_TRANSFORM | C_COLLIDER)
                // Fully intangible right now (burrower underground, caster between appearances):
                // weapons pass straight through, they are not consumed. A merely PHASED entity
                // (drifter in flight) stays a victim — `damage()` refuses the hit, but a bullet
                // still pings off and is consumed.
                && self.mask2[t] & M2_HIDDEN == 0
            {
                victims.push(t);
            }
        }
        for h in 0..n {
            if !self.alive[h]
                || !self.has(h, C_HURT | C_TRANSFORM | C_COLLIDER)
                || !self.is_active(h)
            {
                continue;
            }
            // A hidden (submerged/teleported-away) entity deals no contact damage either.
            if self.mask2[h] & M2_HIDDEN != 0 {
                continue;
            }
            let hurt = self.hurt[h];
            for &t in &victims {
                if t == h {
                    continue;
                }
                if self.tag[t] != hurt.target_tag || !self.slots_overlap(h, t) {
                    continue;
                }
                // Hard room cutoff: a sword reach / shuriken / contact hurt must not land on an
                // entity whose focus sits in a different room (doorway edges + cull margin would
                // otherwise let next-room enemies take damage or deal it).
                if !self.same_room(h, t) {
                    continue;
                }
                let hcx = self.transform[h].x.floor() + self.collider[h].w.floor() / 2;
                let hcy = self.transform[h].y.floor() + self.collider[h].h.floor() / 2;
                // Blocked by a shield / armour facing the hit. The centre is already computed here
                // for the knockback shove, so the direction the blow came from costs nothing extra.
                // A blocked BULLET is still consumed — it hits the shield and stops, it does not
                // sail on through — while a blocked swing simply does nothing.
                if self.has(t, C_GUARD) && self.guard_blocks(t, hcx, hcy, hurt.despawn_on_hit) {
                    if hurt.despawn_on_hit {
                        consumed.push(encode(h as u32, self.gen[h]));
                    }
                    break;
                }
                // Immune to this WEAPON. Placed after the guard test so a directional shield still
                // wins first, and before `hits.push` so an immune target takes no damage, no
                // knockback and no stun — the original discards the whole interaction on an
                // immune hit. A bullet is still consumed: an arrow that pings off an armoured
                // knight stops, it does not sail onward.
                if hurt.damage_type != 0 && (self.immune[t] & hurt.damage_type) != 0 {
                    if hurt.despawn_on_hit {
                        consumed.push(encode(h as u32, self.gen[h]));
                    }
                    break;
                }
                // Weakness: when non-zero, ONLY matching `DMG_*` bits land (vulnerability mask —
                // the complement of `immune`). Untyped damage never matches a listed weakness.
                if self.weak[t] != 0
                    && (hurt.damage_type == 0 || (self.weak[t] & hurt.damage_type) == 0)
                {
                    if hurt.despawn_on_hit {
                        consumed.push(encode(h as u32, self.gen[h]));
                    }
                    break;
                }
                hits.push((encode(t as u32, self.gen[t]), hurt.damage, hcx, hcy));
                if hurt.stun > 0 {
                    self.stun[t] = self.stun[t].max(hurt.stun);
                }
                if hurt.despawn_on_hit {
                    consumed.push(encode(h as u32, self.gen[h]));
                }
                break; // one hurt box damages one victim per frame (non-piercing)
            }
        }
        for (victim, dmg, hcx, hcy) in hits {
            // Only a landed hit (not blocked by i-frames) shoves the victim — a top-down entity gets
            // knocked back away from the hurt box's centre (classic hit-reaction).
            if self.damage(victim, dmg) {
                if let Some(t) = self.slot_of(victim).filter(|&t| self.has(t, C_TOPDOWN)) {
                    let vcx = self.transform[t].x.floor() + self.collider[t].w.floor() / 2;
                    let vcy = self.transform[t].y.floor() + self.collider[t].h.floor() / 2;
                    self.topdown[t].kx = Fixed::from_raw((vcx - hcx).signum() * TD_KNOCK);
                    self.topdown[t].ky = Fixed::from_raw((vcy - hcy).signum() * TD_KNOCK);
                    self.topdown[t].knock = TD_KNOCK_FRAMES;
                }
            }
        }
        for bullet in consumed {
            self.despawn(bullet);
        }
    }

    /// Health system (pipeline phase 4): tick i-frames down and flicker the sprite while invincible.
    fn health_system(&mut self) {
        for s in 0..self.alive.len() {
            // Dormant first: the stun tick below sits BEFORE any component filter, so without this
            // a sleeping pool slot pays for stun bookkeeping and the i-frame sprite blink.
            if self.mask[s] & C_SLEEP != 0 {
                continue;
            }
            if self.alive[s] && self.stun[s] > 0 {
                self.stun[s] -= 1;
            }
            if !self.alive[s] || !self.has(s, C_HEALTH) {
                continue;
            }
            let inv = self.health[s].invuln;
            if inv > 0 {
                self.health[s].invuln = inv - 1;
            }
            if self.has(s, C_SPRITE) {
                let handle = self.sprite[s].handle;
                if handle >= 0 {
                    // hidden on alternating 4-frame spans while invincible, visible otherwise —
                    // and never visible while a nai state machine holds the entity M2_HIDDEN
                    // (submerged burrower, vanished caster), or the i-frame blink would undo it.
                    let vis = (inv <= 0 || (inv / 4) % 2 == 0) && self.mask2[s] & M2_HIDDEN == 0;
                    tish_agb::native_sprite_set_visible(handle, vis);
                }
            }
        }
    }

    /// Collect dead entities as `(onDeath, data, entity)` (clearing the dead flag) for the caller
    /// to dispatch after dropping the borrow. A `Null` callback means "no onDeath hook" → despawn.
    fn collect_deaths(&mut self) -> Vec<(Value, Value, i32)> {
        let mut out = Vec::new();
        for s in 0..self.alive.len() {
            if !self.alive[s]
                || self.mask[s] & C_SLEEP != 0
                || !self.has(s, C_HEALTH)
                || !self.health[s].dead
            {
                continue;
            }
            self.health[s].dead = false;
            // Part-death notification: a boss part carrying `set_death_note` reports its code to
            // the queue the parent's tish logic drains with `death_note()`. Capped so a runaway
            // cannot grow the heap; a boss has a handful of parts.
            if self.mask2[s] & M2_NOTE != 0 && self.death_notes.len() < 16 {
                self.death_notes.push(self.zx[s].code);
            }
            let entity = encode(s as u32, self.gen[s]);
            let (cb, data) = match &self.behaviour[s] {
                Some(b) => (self.defs[b.def].on_death.clone(), b.data.clone()),
                None => (Value::Null, Value::Null),
            };
            out.push((cb, data, entity));
        }
        out
    }

    // ── Native patrol AI ─────────────────────────────────────────────────────
    /// Enable native patrol on a platformer entity (walk + turn at walls/ledges), starting facing
    /// left. Runs in Rust every frame — no tish callback — so many on-screen enemies stay cheap.
    fn set_patrol(&mut self, e: i32, flip_mode: i32) {
        if let Some(s) = self.slot_of(e) {
            self.patrol[s] = Patrol {
                flip_mode,
                ..Patrol::default()
            };
            self.mask[s] |= C_PATROL;
            self.used |= C_PATROL;
        }
    }

    /// Patrol system (pipeline phase 2, before `platformer_system`): each on-screen patrol entity
    /// reverses at a wall (last frame's `blocked`) or a ledge (no ground on the tile ahead at foot
    /// level) while grounded, then feeds its direction to the platformer as this frame's move
    /// intent, and mirrors the sprite to match if the entity asked for it (`flip_mode`).
    fn patrol_system(&mut self) {
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_PATROL | C_PLATFORMER | C_TRANSFORM | C_COLLIDER) {
                continue;
            }
            if !self.is_active(s) {
                continue;
            }
            if self.stun[s] > 0 {
                // Stunned — or carried (a grab sets a long stun): AI suspended, matching every
                // other AI system's stun gate. Move intent is zeroed so the body stands still
                // instead of walking on with its last direction and no one steering.
                self.platformer[s].dir = 0;
                continue;
            }
            let dir = self.patrol[s].dir;
            // ⚠️ Only reverse while GROUNDED. An airborne probe (mid-fall, knocked back, spawned in
            // the air) sees no ground ahead at foot level, reads that as a ledge, and flips direction
            // EVERY frame — so the entity lands moving an arbitrary way. A flat-ground patroller is
            // unaffected (it is always grounded); anything that can leave the floor needed this.
            if !self.platformer[s].grounded {
                self.platformer[s].dir = dir; // keep walking the way it was already going
                continue;
            }
            let x = self.transform[s].x.floor();
            let y = self.transform[s].y.floor();
            let w = self.collider[s].w.floor();
            let h = self.collider[s].h.floor();
            let probe_x = if dir > 0 { x + w } else { x - 1 };
            let ahead_col = probe_x.div_euclid(TILE);
            let foot_row = (y + h).div_euclid(TILE);
            let ledge = !self.is_solid(ahead_col, foot_row);
            let dir = if self.platformer[s].blocked || ledge {
                -dir
            } else {
                dir
            };
            self.patrol[s].dir = dir;
            self.platformer[s].dir = dir; // move intent → platformer_system walks it that way

            // Mirror the sprite to match, only when the direction actually changed. This is here
            // rather than in the game because a tish `tick` that exists purely to call `setFlip`
            // reintroduces the whole per-enemy boxed-callback cost that `set_patrol` removes.
            let mode = self.patrol[s].flip_mode;
            if mode != 0 && self.patrol[s].flipped_for != dir && self.has(s, C_SPRITE) {
                self.patrol[s].flipped_for = dir;
                let handle = self.sprite[s].handle;
                if handle >= 0 {
                    let flip = if mode == 1 { dir > 0 } else { dir < 0 };
                    tish_agb::native_sprite_set_flip(handle, flip);
                }
            }
        }
    }

    /// Start a looping sprite-sheet animation on an entity: cycle `frames` frames,
    /// advancing one every `speed` game frames.
    fn set_anim(&mut self, e: i32, frames: i32, speed: i32) {
        // Convenience: loop the whole sheet from frame 0 (the common "spin these N frames").
        self.anim_play(e, 0, frames, speed, true);
    }

    /// Play the clip `[from, from+len)` at `speed` frames-per-step. Idempotent: calling it
    /// with the clip already playing does NOT restart it (so a controller can call it every
    /// frame while a state holds); a different `from`/`len` switches clips and shows the
    /// first frame at once.
    fn anim_play(&mut self, e: i32, from: i32, len: i32, speed: i32, looping: bool) {
        let Some(s) = self.slot_of(e) else {
            return;
        };
        let len = len.max(1);
        let same = {
            let a = &self.anim[s];
            self.has(s, C_ANIM) && a.playing && a.from == from && a.len == len
        };
        let a = &mut self.anim[s];
        a.from = from;
        a.len = len;
        a.speed = speed.max(1);
        a.looping = looping;
        a.playing = true;
        self.mask[s] |= C_ANIM;
        self.used |= C_ANIM;
        if !same {
            a.timer = 0;
            a.cur = 0;
            let handle = self.sprite[s].handle;
            if handle >= 0 {
                tish_agb::native_sprite_set_frame(handle, from);
            }
        }
    }

    fn set_walk(&mut self, e: i32, cols: i32, speed: i32) {
        if let Some(s) = self.slot_of(e) {
            self.walk[s] = Walk {
                cols: cols.max(1),
                speed: speed.max(1),
                timer: 0,
                phase: false,
            };
            self.mask[s] |= C_WALK;
            self.used |= C_WALK;
        }
    }

    /// The entity's grid facing as a direction code: 0 down, 1 up, 2 left, 3 right (a
    /// tish animation controller maps this + movement to a clip). Default down.
    /// The entity's current grid tile column/row (its logical tile, not the pixel transform).
    /// `-1` if the entity has no grid position — a sentinel no teleporter/tile check will match.
    fn grid_col(&self, e: i32) -> i32 {
        self.slot_of(e)
            .filter(|&s| self.has(s, C_GRIDPOS))
            .map(|s| self.gridpos[s].col)
            .unwrap_or(-1)
    }
    fn grid_row(&self, e: i32) -> i32 {
        self.slot_of(e)
            .filter(|&s| self.has(s, C_GRIDPOS))
            .map(|s| self.gridpos[s].row)
            .unwrap_or(-1)
    }

    fn grid_facing(&self, e: i32) -> i32 {
        if let Some(s) = self.slot_of(e) {
            if self.has(s, C_GRIDPOS) {
                let g = &self.gridpos[s];
                return if g.fy > 0 {
                    0
                } else if g.fy < 0 {
                    1
                } else if g.fx < 0 {
                    2
                } else {
                    3
                };
            }
        }
        0
    }

    /// Animation system (pure Rust): advance each playing animation's frame and push it
    /// to the entity's tish-agb sprite.
    fn anim_system(&mut self) {
        for s in 0..self.alive.len() {
            if !self.alive[s]
                || self.mask[s] & C_SLEEP != 0
                || !self.has(s, C_ANIM | C_SPRITE)
                || !self.anim[s].playing
            {
                continue;
            }
            if !self.is_active(s) {
                continue; // off-screen — no need to advance animation
            }
            let a = &mut self.anim[s];
            a.timer += 1;
            if a.timer >= a.speed {
                a.timer = 0;
                a.cur += 1;
                if a.cur >= a.len {
                    if a.looping {
                        a.cur = 0;
                    } else {
                        a.cur = a.len - 1;
                        a.playing = false;
                    }
                }
                let frame = a.from + a.cur;
                let handle = self.sprite[s].handle;
                if handle >= 0 {
                    tish_agb::native_sprite_set_frame(handle, frame);
                }
            }
        }
    }

    /// Directional walk animation: choose the sprite frame + horizontal flip from each
    /// walking entity's `GridPos` facing and movement. Standing shows the row's neutral
    /// column; moving toggles the two step columns at `speed`.
    fn walk_system(&mut self) {
        for s in 0..self.alive.len() {
            if !self.alive[s] || !self.has(s, C_WALK | C_GRIDPOS | C_SPRITE) {
                continue;
            }
            if !self.is_active(s) {
                continue;
            }
            let (fx, fy, moving) = {
                let g = &self.gridpos[s];
                (g.fx, g.fy, g.moving)
            };
            // Facing → row (down / up / side) and flip (right reuses the side row).
            let (row, flip) = if fy > 0 {
                (0, false) // down
            } else if fy < 0 {
                (1, false) // up
            } else if fx < 0 {
                (2, false) // left
            } else if fx > 0 {
                (2, true) // right
            } else {
                (0, false) // default: face down
            };
            let cols = self.walk[s].cols.max(1);
            let col = if moving {
                let w = &mut self.walk[s];
                w.timer += 1;
                if w.timer >= w.speed {
                    w.timer = 0;
                    w.phase = !w.phase;
                }
                if w.phase {
                    2
                } else {
                    0
                }
            } else {
                let w = &mut self.walk[s];
                w.timer = 0;
                w.phase = false;
                1 // standing
            };
            let frame = row * cols + col.min(cols - 1);
            let handle = self.sprite[s].handle;
            if handle >= 0 {
                tish_agb::native_sprite_set_frame(handle, frame);
                tish_agb::native_sprite_set_flip(handle, flip);
            }
        }
    }

    /// Render system (pure Rust): drive tish-agb's sprites (no boxing per sprite).
    fn render_system(&mut self) {
        // (sleeping slots are skipped below via the mask test)
        for s in 0..self.alive.len() {
            // A sleeping slot is parked off-map with a hidden sprite; there is nothing to draw and
            // nothing to compute a screen position for.
            if self.alive[s] && self.mask[s] & C_SLEEP == 0 && self.has(s, C_TRANSFORM | C_SPRITE) {
                let h = self.sprite[s].handle;
                if h < 0 {
                    continue;
                }
                // Off-screen sprites release their VRAM `Object` so a large scrolling level with many
                // entities holds a sprite allocation only for what's on screen (GBA sprite VRAM is
                // small); they rebuild when they scroll back in. `is_active` keeps the camera target
                // (and every entity in a non-scrolling game) resident, so small scenes are unchanged.
                if self.is_active(s) {
                    tish_agb::native_sprite_restore(h);
                    tish_agb::native_sprite_set_pos(
                        h,
                        self.transform[s].x.floor() + self.sprite[s].ox,
                        self.transform[s].y.floor() + self.sprite[s].oy,
                    );
                } else {
                    tish_agb::native_sprite_release(h);
                }
            }
        }
    }

    fn set_camera_target(&mut self, e: i32) {
        self.camera_target = Some(e);
        self.room_cam.enabled = false;
    }

    /// Enable the room-locked camera following `e`, with rooms `room_w`×`room_h` tiles
    /// (0 → the screen default 15×10). The starting room is taken from the entity's current tile.
    fn set_room_camera(&mut self, e: i32, room_w: i32, room_h: i32) {
        self.camera_target = Some(e);
        let rw = if room_w > 0 { room_w } else { 15 };
        let rh = if room_h > 0 { room_h } else { 10 };
        // Starting room: from the grid tile (RPG) or, for a free-moving platformer, the box
        // centre's pixel position.
        let (rx, ry) = match self.slot_of(e) {
            Some(s) if self.has(s, C_GRIDPOS) => {
                (self.gridpos[s].col / rw, self.gridpos[s].row / rh)
            }
            Some(s) if self.has(s, C_TRANSFORM) => {
                let (cx, cy) = self.camera_focus(s);
                (cx.div_euclid(rw * TILE), cy.div_euclid(rh * TILE))
            }
            _ => (0, 0),
        };
        self.room_cam.enabled = true;
        self.room_cam.room_w = rw;
        self.room_cam.room_h = rh;
        self.room_cam.cur_rx = rx;
        self.room_cam.cur_ry = ry;
        self.room_cam.transitioning = false;
    }

    /// Centre the camera on the target entity, clamped so it never scrolls past the map
    /// edge (and stays at 0 on an axis where the map is smaller than the screen). Pushes the
    /// result to tish-agb, which scrolls the streamed layers and offsets the sprites.
    fn update_camera(&mut self) {
        // Room camera: lock to the current room, or interpolate across rooms mid-slide.
        if self.room_cam.enabled {
            let rc = &self.room_cam;
            let (cx, cy) = if rc.transitioning {
                (
                    rc.from_cam.0 + (rc.to_cam.0 - rc.from_cam.0) * rc.timer / rc.dur,
                    rc.from_cam.1 + (rc.to_cam.1 - rc.from_cam.1) * rc.timer / rc.dur,
                )
            } else {
                (rc.cur_rx * rc.room_w * TILE, rc.cur_ry * rc.room_h * TILE)
            };
            self.cam_x = cx;
            self.cam_y = cy;
            tish_agb::native_camera_set(cx, cy);
            return;
        }
        let Some(e) = self.camera_target else { return };
        let Some(s) = self.slot_of(e) else { return };
        if !self.has(s, C_TRANSFORM) {
            return;
        }
        let (px, py) = (self.transform[s].x.floor(), self.transform[s].y.floor());
        let max_x = (self.grid_cols * TILE - 240).max(0);
        let max_y = (self.grid_rows * TILE - 160).max(0);
        let cx = (px + TILE / 2 - 120).clamp(0, max_x);
        let cy = (py + TILE / 2 - 80).clamp(0, max_y);
        self.cam_x = cx;
        self.cam_y = cy;
        tish_agb::native_camera_set(cx, cy);
    }

    /// Is slot `s`'s box within the camera view (plus `CULL_MARGIN`)? Entities with no transform
    /// count as on-screen (nothing to place them off it).
    fn on_screen(&self, s: usize) -> bool {
        if !self.has(s, C_TRANSFORM) {
            return true;
        }
        let x = self.transform[s].x.floor();
        let y = self.transform[s].y.floor();
        let (w, h) = if self.has(s, C_COLLIDER) {
            (
                self.collider[s].w.floor().max(1),
                self.collider[s].h.floor().max(1),
            )
        } else {
            (TILE, TILE)
        };
        let m = CULL_MARGIN;
        x + w >= self.cam_x - m
            && x <= self.cam_x + 240 + m
            && y + h >= self.cam_y - m
            && y <= self.cam_y + 160 + m
    }

    /// Should slot `s` run its game loop this frame? The camera target and, for a non-scrolling
    /// game (no camera), everything is always active; otherwise off-screen entities are culled.
    fn is_active(&self, s: usize) -> bool {
        // A dormant pool slot is inert everywhere, whatever the camera is doing.
        if self.mask[s] & C_SLEEP != 0 {
            return false;
        }
        match self.camera_target {
            // No camera target: the game drives its own camera, and everything stays active.
            //
            // This looks like a missed optimisation — an RTS with a unit pool pays full per-entity
            // cost for parked slots — and an opt-in `set_cull_offscreen` was built and measured to
            // close it. It is not a missed optimisation: an RTS army ordered across the map walks
            // off screen and MUST keep walking, and with culling on it simply stopped. The saving
            // was ~600 ticks and the cost was the game. Left here so the next person does not spend
            // the afternoon rediscovering it.
            None => true,
            Some(e) if self.slot_of(e) == Some(s) => true,
            _ => self.on_screen(s),
        }
    }

    /// The pixel point the camera aims at for slot `s`: its collider box centre, or the bare
    /// transform when it has no collider.
    fn camera_focus(&self, s: usize) -> (i32, i32) {
        let (bx, by) = if self.has(s, C_COLLIDER) {
            (
                self.collider[s].w.floor() / 2,
                self.collider[s].h.floor() / 2,
            )
        } else {
            (0, 0)
        };
        (
            self.transform[s].x.floor() + bx,
            self.transform[s].y.floor() + by,
        )
    }

    /// Room-grid coords `(rx, ry)` for slot `s` from its focus point. Doorway tiles sit on a
    /// room edge; the focus decides which room owns the entity for combat / interact gating.
    fn room_of(&self, s: usize) -> (i32, i32) {
        let (rw, rh) = (self.room_cam.room_w, self.room_cam.room_h);
        let (cx, cy) = self.camera_focus(s);
        (cx.div_euclid(rw * TILE), cy.div_euclid(rh * TILE))
    }

    /// Hard room cutoff: with a room camera, two entities only interact (hurt, collide, talk)
    /// while they share a room. Off / no room-cam → always true. Mid room-slide → nothing
    /// interacts (the world is frozen for the pan).
    fn same_room(&self, a: usize, b: usize) -> bool {
        if !self.room_cam.enabled {
            return true;
        }
        if self.room_cam.transitioning {
            return false;
        }
        self.room_of(a) == self.room_of(b)
    }

    /// Is slot `s` in the player's locked room? Flying hurt boxes (stars, shots) that leave it
    /// are retired so a projectile can't sail through an open doorway and hit the next room.
    fn in_current_room(&self, s: usize) -> bool {
        if !self.room_cam.enabled {
            return true;
        }
        if self.room_cam.transitioning {
            return false;
        }
        let (rx, ry) = self.room_of(s);
        rx == self.room_cam.cur_rx && ry == self.room_cam.cur_ry
    }

    /// Nearest OTHER entity with `tag` within `radius` px (manhattan, box centers), or -1.
    /// Deliberately NOT gated on `is_active`: simulation entities keep mattering off screen.
    /// O(n) scan — callers stagger queries (every 8–16 frames, cache the id) rather than asking
    /// for every entity every frame.
    fn nearest_tag(&self, e: i32, tag: i32, radius: i32) -> i32 {
        let Some(s) = self.slot_of(e) else {
            return -1;
        };
        let (cx, cy) = self.center_of(s);
        let mut best = -1;
        let mut best_d = i32::MAX;
        for c in 0..self.alive.len() {
            if c == s || !self.alive[c] || self.tag[c] != tag || !self.has(c, C_TRANSFORM) {
                continue;
            }
            if !self.same_room(s, c) {
                continue;
            }
            let (ox, oy) = self.center_of(c);
            let d = (ox - cx).to_raw().abs() + (oy - cy).to_raw().abs();
            if d < best_d {
                best_d = d;
                best = encode(c as u32, self.gen[c]);
            }
        }
        if best_d <= radius.saturating_mul(256) {
            best
        } else {
            -1
        }
    }

    /// Manhattan distance between two entities' box centers in whole px, or -1 if either is gone.
    fn entity_dist(&self, a: i32, b: i32) -> i32 {
        match (self.slot_of(a), self.slot_of(b)) {
            (Some(sa), Some(sb)) => {
                let (ax, ay) = self.center_of(sa);
                let (bx, by) = self.center_of(sb);
                ((ax - bx).to_raw().abs() + (ay - by).to_raw().abs()) >> 8
            }
            _ => -1,
        }
    }

    /// For a free-moving (platformer) camera target, start a room slide when its box centre
    /// crosses into a new room. Grid entities trigger from `grid_step` instead; this is the
    /// side-scrolling counterpart. Runs each frame before `room_transition_system`.
    fn room_track_free(&mut self) {
        if !self.room_cam.enabled || self.room_cam.transitioning {
            return;
        }
        let Some(e) = self.camera_target else { return };
        let Some(s) = self.slot_of(e) else { return };
        if self.has(s, C_GRIDPOS) || !self.has(s, C_TRANSFORM) {
            return;
        }
        let (rw, rh) = (self.room_cam.room_w, self.room_cam.room_h);
        let (cx, cy) = self.camera_focus(s);
        let (nrx, nry) = (cx.div_euclid(rw * TILE), cy.div_euclid(rh * TILE));
        if (nrx, nry) != (self.room_cam.cur_rx, self.room_cam.cur_ry) {
            self.begin_room_transition_free(s, nrx, nry);
        }
    }

    /// Slide the camera one room over while the player holds still: `from_px == to_px`, so
    /// `room_transition_system` pins it at its current pixel for the slide (input is locked by
    /// `input_locked`). The metroidvania-style "walk through the door, screen pans" transition.
    fn begin_room_transition_free(&mut self, s: usize, nrx: i32, nry: i32) {
        let rc = &self.room_cam;
        let (rw, rh) = (rc.room_w, rc.room_h);
        let from_cam = (rc.cur_rx * rw * TILE, rc.cur_ry * rh * TILE);
        let to_cam = (nrx * rw * TILE, nry * rh * TILE);
        let px = (self.transform[s].x.floor(), self.transform[s].y.floor());
        let dx = nrx - rc.cur_rx;
        let dy = nry - rc.cur_ry;
        let rc = &mut self.room_cam;
        rc.cur_rx = nrx;
        rc.cur_ry = nry;
        rc.transitioning = true;
        rc.timer = 0;
        rc.from_cam = from_cam;
        rc.to_cam = to_cam;
        rc.from_px = px;
        rc.to_px = (px.0 + dx * 24, px.1 + dy * 24);
    }

    // ── Grid / RPG genre ─────────────────────────────────────────────────────
    /// Is tile `(col,row)` solid? Out-of-bounds is solid (the map edge blocks).
    fn is_solid(&self, col: i32, row: i32) -> bool {
        if col < 0 || row < 0 || col >= self.grid_cols || row >= self.grid_rows {
            return true;
        }
        let i = (row * self.grid_cols + col) as usize;
        grid_bit(&self.solid, i)
    }

    /// Is tile `(col,row)` a one-way platform? Out-of-bounds is never one-way.
    fn is_oneway(&self, col: i32, row: i32) -> bool {
        if col < 0 || row < 0 || col >= self.grid_cols || row >= self.grid_rows {
            return false;
        }
        if self.oneway.is_empty() {
            return false;
        }
        let i = (row * self.grid_cols + col) as usize;
        grid_bit(&self.oneway, i)
    }

    /// Is tile `(col,row)` climbable (a ladder / vine / rope)? Out-of-bounds is never climbable.
    fn is_ladder(&self, col: i32, row: i32) -> bool {
        if col < 0 || row < 0 || col >= self.grid_cols || row >= self.grid_rows {
            return false;
        }
        if self.ladder.is_empty() {
            return false;
        }
        let i = (row * self.grid_cols + col) as usize;
        grid_bit(&self.ladder, i)
    }

    fn grid_setup(&mut self, cols: i32, rows: i32) {
        self.grid_cols = cols.max(0);
        self.grid_rows = rows.max(0);
        let n = (self.grid_cols * self.grid_rows) as usize;
        self.grid_cells = n;
        let bytes = grid_bit_bytes(n);
        // Grow-only bitplanes: never free the overworld-sized buffer when entering a small
        // cave (and never re-allocate 19KB on return — that OOMs after heap fragmentation).
        if bytes > self.solid.capacity() {
            self.solid = alloc::vec![0u8; bytes];
        } else {
            self.solid.resize(bytes, 0);
            self.solid.fill(0);
        }
        // Oneway stays empty until first grid_set_oneway (topdown games never touch it).
        if !self.oneway.is_empty() {
            if bytes > self.oneway.capacity() {
                self.oneway = alloc::vec![0u8; bytes];
            } else {
                self.oneway.resize(bytes, 0);
                self.oneway.fill(0);
            }
        }
        // Same deal for ladders — only a game that has placed one carries the plane.
        if !self.ladder.is_empty() {
            if bytes > self.ladder.capacity() {
                self.ladder = alloc::vec![0u8; bytes];
            } else {
                self.ladder.resize(bytes, 0);
                self.ladder.fill(0);
            }
        }
    }

    fn grid_set_solid(&mut self, col: i32, row: i32, solid: bool) {
        if col >= 0 && row >= 0 && col < self.grid_cols && row < self.grid_rows {
            let i = (row * self.grid_cols + col) as usize;
            grid_bit_set(&mut self.solid, i, solid);
        }
    }

    fn grid_set_oneway(&mut self, col: i32, row: i32, on: bool) {
        if col >= 0 && row >= 0 && col < self.grid_cols && row < self.grid_rows {
            let bytes = grid_bit_bytes(self.grid_cells);
            if self.oneway.len() < bytes {
                self.oneway.resize(bytes, 0);
            }
            let i = (row * self.grid_cols + col) as usize;
            grid_bit_set(&mut self.oneway, i, on);
        }
    }

    fn grid_set_ladder(&mut self, col: i32, row: i32, on: bool) {
        if col >= 0 && row >= 0 && col < self.grid_cols && row < self.grid_rows {
            let bytes = grid_bit_bytes(self.grid_cells);
            if self.ladder.len() < bytes {
                self.ladder.resize(bytes, 0);
            }
            let i = (row * self.grid_cols + col) as usize;
            grid_bit_set(&mut self.ladder, i, on);
        }
    }

    /// Place an entity on the grid at tile `(col,row)`: sets `GridPos` + `Transform`
    /// (pixel = tile·TILE) and marks it grid-controlled. Default facing = down.
    fn attach_grid(&mut self, e: i32, col: i32, row: i32) {
        if let Some(s) = self.slot_of(e) {
            let px = Fixed::from_raw(col * TILE * 256);
            let py = Fixed::from_raw(row * TILE * 256);
            self.gridpos[s] = GridPos {
                col,
                row,
                moving: false,
                tx: px,
                ty: py,
                fx: 0,
                fy: 1,
            };
            self.transform[s] = Transform { x: px, y: py };
            self.mask[s] |= C_GRIDPOS | C_TRANSFORM;
            self.used |= C_GRIDPOS | C_TRANSFORM;
        }
    }

    /// Request a tile step in direction `(dx,dy)` (4-directional; horizontal wins if
    /// both). Always faces that way; only actually steps if idle and the target tile
    /// isn't solid. The grid system then slides the transform over several frames.
    fn grid_step(&mut self, e: i32, dx: i32, dy: i32) {
        // Room camera locks player input while a screen-slide transition plays.
        if self.room_cam.enabled && self.room_cam.transitioning && self.camera_target == Some(e) {
            return;
        }
        let Some(s) = self.slot_of(e) else {
            return;
        };
        if !self.has(s, C_GRIDPOS) {
            return;
        }
        let (sdx, sdy) = if dx != 0 {
            (dx.signum(), 0)
        } else if dy != 0 {
            (0, dy.signum())
        } else {
            return;
        };
        let (col, row, moving) = {
            let g = &mut self.gridpos[s];
            g.fx = sdx;
            g.fy = sdy;
            (g.col, g.row, g.moving)
        };
        if moving {
            return;
        }
        let (tc, tr) = (col + sdx, row + sdy);
        if self.is_solid(tc, tr) || self.tile_occupied(tc, tr, s) {
            return;
        }
        // Room camera: stepping across a room boundary starts a screen-slide instead of a step —
        // the player's tile updates now (so they're logically in the new room) but the visual slide
        // of both player and camera is driven by `room_transition_system`.
        if self.room_cam.enabled && self.camera_target == Some(e) {
            let (rw, rh) = (self.room_cam.room_w, self.room_cam.room_h);
            let (old_rx, old_ry) = (col / rw, row / rh);
            let (new_rx, new_ry) = (tc / rw, tr / rh);
            if (new_rx, new_ry) != (old_rx, old_ry) {
                self.begin_room_transition(s, tc, tr, new_rx, new_ry);
                return;
            }
        }
        let g = &mut self.gridpos[s];
        g.col = tc;
        g.row = tr;
        g.tx = Fixed::from_raw(tc * TILE * 256);
        g.ty = Fixed::from_raw(tr * TILE * 256);
        g.moving = true;
    }

    /// Kick off a room-to-room screen slide: the player's tile jumps to (tc,tr) in the new room,
    /// and `room_transition_system` interpolates both the camera (one full room over) and the
    /// player's pixel position (one tile) over `dur` frames while input stays locked.
    fn begin_room_transition(&mut self, s: usize, tc: i32, tr: i32, new_rx: i32, new_ry: i32) {
        let rc = &self.room_cam;
        let (rw, rh) = (rc.room_w, rc.room_h);
        let from_cam = (rc.cur_rx * rw * TILE, rc.cur_ry * rh * TILE);
        let to_cam = (new_rx * rw * TILE, new_ry * rh * TILE);
        let from_px = (self.gridpos[s].tx.floor(), self.gridpos[s].ty.floor());
        let to_px = (tc * TILE, tr * TILE);
        // logical tile is the new room's entry tile immediately
        let g = &mut self.gridpos[s];
        g.col = tc;
        g.row = tr;
        g.tx = Fixed::from_raw(tc * TILE * 256);
        g.ty = Fixed::from_raw(tr * TILE * 256);
        g.moving = false;
        let rc = &mut self.room_cam;
        rc.cur_rx = new_rx;
        rc.cur_ry = new_ry;
        rc.transitioning = true;
        rc.timer = 0;
        rc.from_cam = from_cam;
        rc.to_cam = to_cam;
        rc.from_px = from_px;
        rc.to_px = to_px;
    }

    /// Advance an in-progress room slide one frame: lerp the player's pixel transform toward the
    /// new-room entry tile (the camera lerp is computed in `update_camera` from the same timer),
    /// and finalise on arrival. Runs before `render_system` so the sprite draws at the slid pos.
    fn room_transition_system(&mut self) {
        if !self.room_cam.enabled || !self.room_cam.transitioning {
            return;
        }
        let Some(e) = self.camera_target else { return };
        let Some(s) = self.slot_of(e) else { return };
        let rc = &mut self.room_cam;
        rc.timer += 1;
        let done = rc.timer >= rc.dur;
        let (px, py) = if done {
            rc.transitioning = false;
            rc.to_px
        } else {
            (
                rc.from_px.0 + (rc.to_px.0 - rc.from_px.0) * rc.timer / rc.dur,
                rc.from_px.1 + (rc.to_px.1 - rc.from_px.1) * rc.timer / rc.dur,
            )
        };
        self.transform[s].x = Fixed::from_raw(px * 256);
        self.transform[s].y = Fixed::from_raw(py * 256);
    }

    /// Is tile `(col,row)` claimed by a grid entity other than slot `except`? (Entities
    /// block each other — you can't step onto an NPC's tile, so you face it instead.)
    fn tile_occupied(&self, col: i32, row: i32, except: usize) -> bool {
        for o in 0..self.alive.len() {
            if o != except
                && self.alive[o]
                && self.has(o, C_GRIDPOS)
                && self.gridpos[o].col == col
                && self.gridpos[o].row == row
            {
                return true;
            }
        }
        false
    }

    /// Grid system: slide any mid-step entity's transform toward its target tile.
    fn grid_system(&mut self) {
        let speed = Fixed::from_raw(GRID_SPEED * 256);
        for s in 0..self.alive.len() {
            if !self.alive[s]
                || self.mask[s] & C_SLEEP != 0
                || !self.has(s, C_GRIDPOS)
                || !self.gridpos[s].moving
            {
                continue;
            }
            let (tx, ty) = (self.gridpos[s].tx, self.gridpos[s].ty);
            let ax = approach(&mut self.transform[s].x, tx, speed);
            let ay = approach(&mut self.transform[s].y, ty, speed);
            if ax && ay {
                self.gridpos[s].moving = false;
            }
        }
    }

    /// The entity the faced tile holds that has an `onInteract` — gathered as
    /// `(callback, target_data, target_entity, actor_entity)` for reentrancy-safe dispatch.
    fn collect_interact(&self, e: i32) -> Option<(Value, Value, i32, i32)> {
        let s = self.slot_of(e)?;
        if !self.has(s, C_GRIDPOS) {
            return None;
        }
        let g = self.gridpos[s];
        let (tc, tr) = (g.col + g.fx, g.row + g.fy);
        for o in 0..self.alive.len() {
            if o == s || !self.alive[o] || !self.has(o, C_GRIDPOS) {
                continue;
            }
            if self.gridpos[o].col == tc && self.gridpos[o].row == tr {
                if let Some(b) = &self.behaviour[o] {
                    let cb = self.defs[b.def].on_interact.clone();
                    if !matches!(cb, Value::Null) {
                        let target = encode(o as u32, self.gen[o]);
                        return Some((cb, b.data.clone(), target, e));
                    }
                }
            }
        }
        None
    }

    /// Top-down interact probe: the strip of ground `reach` px deep in front of the entity (by its
    /// top-down facing, as wide as its own box); if an entity overlapping it defines `onInteract`,
    /// return the (cb, data, target, actor) to fire. Mirrors `collect_interact` for the
    /// free-movement genre ("talk to the NPC" / "open the chest").
    ///
    /// A strip, not the single point this used to test: pressed up against a 14px NPC the point
    /// `reach` px out already sat PAST it, so the talk silently missed — and with one context button
    /// (talk-or-attack) a miss means swinging your sword at the person you meant to greet.
    fn collect_topdown_interact(&self, e: i32, reach: i32) -> Option<(Value, Value, i32, i32)> {
        let s = self.slot_of(e)?;
        if !self.has(s, C_TOPDOWN | C_TRANSFORM | C_COLLIDER) {
            return None;
        }
        let ax = self.transform[s].x.floor();
        let ay = self.transform[s].y.floor();
        let cw = self.collider[s].w.floor();
        let ch = self.collider[s].h.floor();
        let r = reach.max(1);
        let (px, py, pw, ph) = match self.topdown[s].facing {
            1 => (ax, ay - r, cw, r),
            2 => (ax - r, ay, r, ch),
            3 => (ax + cw, ay, r, ch),
            _ => (ax, ay + ch, cw, r),
        };
        for o in 0..self.alive.len() {
            if o == s
                || !self.alive[o]
                || !self.has(o, C_TRANSFORM | C_COLLIDER)
                || self.behaviour[o].is_none()
            {
                continue;
            }
            // Room cutoff: talking / opening never reaches into the next room through a doorway.
            if !self.same_room(s, o) {
                continue;
            }
            let ox = self.transform[o].x.floor();
            let oy = self.transform[o].y.floor();
            let ow = self.collider[o].w.floor();
            let oh = self.collider[o].h.floor();
            if px < ox + ow && ox < px + pw && py < oy + oh && oy < py + ph {
                let b = self.behaviour[o].as_ref().unwrap();
                let cb = self.defs[b.def].on_interact.clone();
                if !matches!(cb, Value::Null) {
                    return Some((cb, b.data.clone(), encode(o as u32, self.gen[o]), e));
                }
            }
        }
        None
    }

    /// Side-scrolling counterpart of `collect_topdown_interact`: probe the column of space `reach`
    /// px wide beside the entity, on the side it is FACING, over its own box height; if something
    /// there defines `onInteract`, return the (cb, data, target, actor) to fire.
    ///
    /// A platformer has no `TopDown.facing`, and its `Platformer.dir` is zeroed the moment the
    /// d-pad is released — you talk to an NPC while standing still, so the probe reads
    /// `Platformer.face` (the last direction actually moved) instead.
    ///
    /// The probe is deliberately TALLER than the box: `PF_INTERACT_PAD` px of slack above and below
    /// so you can talk to an NPC standing on a step, or one whose art sits on a different hitbox
    /// height, without having to line the boxes up exactly.
    fn collect_platformer_interact(&self, e: i32, reach: i32) -> Option<(Value, Value, i32, i32)> {
        let s = self.slot_of(e)?;
        if !self.has(s, C_PLATFORMER | C_TRANSFORM | C_COLLIDER) {
            return None;
        }
        let ax = self.transform[s].x.floor();
        let ay = self.transform[s].y.floor();
        let cw = self.collider[s].w.floor();
        let ch = self.collider[s].h.floor();
        let r = reach.max(1);
        // The strip covers our OWN box as well as `reach` px beyond the facing edge. Probing only
        // the space in front looks right on paper and fails in practice for the two commonest cases:
        // a doorway is a cell you STAND IN, and an NPC you have walked all the way up to overlaps
        // you rather than sitting beside you. Both would answer "there is nothing here".
        let px = if self.platformer[s].face < 0 {
            ax - r
        } else {
            ax
        };
        let pw = cw + r;
        let py = ay - PF_INTERACT_PAD;
        let ph = ch + 2 * PF_INTERACT_PAD;
        for o in 0..self.alive.len() {
            if o == s
                || !self.alive[o]
                || !self.has(o, C_TRANSFORM | C_COLLIDER)
                || self.behaviour[o].is_none()
            {
                continue;
            }
            if !self.same_room(s, o) {
                continue;
            }
            let ox = self.transform[o].x.floor();
            let oy = self.transform[o].y.floor();
            let ow = self.collider[o].w.floor();
            let oh = self.collider[o].h.floor();
            if px < ox + ow && ox < px + pw && py < oy + oh && oy < py + ph {
                let b = self.behaviour[o].as_ref().unwrap();
                let cb = self.defs[b.def].on_interact.clone();
                if !matches!(cb, Value::Null) {
                    return Some((cb, b.data.clone(), encode(o as u32, self.gen[o]), e));
                }
            }
        }
        None
    }

    /// Do entity slots `a` and `b` (both `TRANSFORM | COLLIDER`) overlap? AABB test in
    /// fixed-point, each box top-left at its transform.
    fn slots_overlap(&self, a: usize, b: usize) -> bool {
        let (ta, ca) = (self.transform[a], self.collider[a]);
        let (tb, cb) = (self.transform[b], self.collider[b]);
        ta.x < tb.x + cb.w && tb.x < ta.x + ca.w && ta.y < tb.y + cb.h && tb.y < ta.y + ca.h
    }

    /// Whether two entity ids currently overlap (for the `overlaps(a, b)` query). `false`
    /// if either lacks a collider or is stale.
    fn entities_overlap(&self, ea: i32, eb: i32) -> bool {
        match (self.slot_of(ea), self.slot_of(eb)) {
            (Some(a), Some(b))
                if self.has(a, C_TRANSFORM | C_COLLIDER)
                    && self.has(b, C_TRANSFORM | C_COLLIDER) =>
            {
                self.slots_overlap(a, b)
            }
            _ => false,
        }
    }

    /// Collision detection: find every overlapping pair among collidable entities
    /// (O(n²) — a spatial hash is the later optimization) and, for each side that has
    /// an `onCollide` behaviour, gather `(callback, me_data, self_entity, other_entity)`.
    /// Gathered under the world borrow; the caller invokes them after dropping it
    /// (reentrancy-safe, like `collect_behaviours`).
    fn collect_collisions(&self) -> Vec<(Value, Value, i32, i32)> {
        let mut out = Vec::new();
        let n = self.alive.len();
        // Only entities whose component defines `onCollide` need collision EVENTS — usually just the
        // player. Colliders, by contrast, are many (every bullet carries one for the hurt system). The
        // old all-pairs loop tested every bullet against every other bullet (O(n²)) to discover neither
        // responds — the cost that grew as the screen filled with shots. Iterate responders × colliders
        // instead (O(responders·n)); each responder still fires its own `onCollide` for every overlap,
        // and firing only the responder's side (not both) avoids the double-dispatch the pair loop
        // needed care to avoid, with identical semantics.
        for a in 0..n {
            if !self.alive[a] || !self.has(a, C_TRANSFORM | C_COLLIDER) || !self.is_active(a) {
                continue;
            }
            let responds = match &self.behaviour[a] {
                Some(b) => !matches!(self.defs[b.def].on_collide, Value::Null),
                None => false,
            };
            if !responds {
                continue;
            }
            let ea = encode(a as u32, self.gen[a]);
            for b in 0..n {
                if b == a
                    || !self.alive[b]
                    || !self.has(b, C_TRANSFORM | C_COLLIDER)
                    || !self.is_active(b)
                {
                    continue;
                }
                if !self.slots_overlap(a, b) {
                    continue;
                }
                // Room cutoff: onCollide (pickup, trigger) never fires across a room boundary.
                if !self.same_room(a, b) {
                    continue;
                }
                let eb = encode(b as u32, self.gen[b]);
                self.push_on_collide(&mut out, a, ea, eb);
            }
        }
        out
    }

    fn push_on_collide(
        &self,
        out: &mut Vec<(Value, Value, i32, i32)>,
        slot: usize,
        me_entity: i32,
        other_entity: i32,
    ) {
        if let Some(b) = &self.behaviour[slot] {
            let cb = self.defs[b.def].on_collide.clone();
            if !matches!(cb, Value::Null) {
                out.push((cb, b.data.clone(), me_entity, other_entity));
            }
        }
    }
}

static WORLD: SingleCore<RefCell<World>> = SingleCore::new(RefCell::new(World::new()));

fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> R {
    WORLD.with(|c| f(&mut c.borrow_mut()))
}

/// Read a tish array of numbers into a plain Vec. Used only at load time — passing a typed module
/// array to a native de-optimises every other read of it (see examples/probe-arrayarg), so this
/// crosses the boundary once per mission and never per frame.
#[allow(dead_code)] // load-time boundary helper; callers moved out with the extracted games
fn read_i32_arr(v: Option<&Value>) -> Vec<i32> {
    match v {
        Some(Value::Array(a)) => a
            .borrow()
            .iter()
            .map(|x| match x {
                Value::Number(f) => *f as i32,
                _ => 0,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn n(args: &[Value], i: usize) -> f64 {
    match args.get(i) {
        Some(Value::Number(x)) => *x,
        _ => 0.0,
    }
}

fn name_of(args: &[Value], i: usize) -> String {
    args.get(i)
        .map(|v| v.to_display_string())
        .unwrap_or_default()
}

// ── tish-facing API (native-module ABI) ──────────────────────────────────────

/// `spawn()` — create an entity, returning its id.
pub fn spawn(_args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.spawn()) as f64)
}

/// `despawn(entity)` — remove an entity (its id becomes stale).
pub fn despawn(args: &[Value]) -> Value {
    with_world(|w| w.despawn(n(args, 0) as i32));
    Value::Null
}

/// `reset_entity(entity)` — clear every component/system/timer/tag from a LIVE entity,
/// keeping its id and (hidden) sprite handle, so a pooled slot can be reconfigured
/// without despawn/spawn churn. See `World::reset_entity`.
pub fn reset_entity(args: &[Value]) -> Value {
    with_world(|w| w.reset_entity(n(args, 0) as i32));
    Value::Null
}

/// `set_stun(entity, frames)` — hold an entity still for `frames`. The native AI systems
/// (hopper/chase/jumper/shooter/charger) already honour the same stun the hurt system
/// writes; this is the direct handle for the boomerang and the Clock.
pub fn set_stun(args: &[Value]) -> Value {
    set_stun_typed(n(args, 0) as i32, n(args, 1) as i32);
    Value::Null
}

/// `is_stunned(entity)` — 1 while the entity's stun timer is running, else 0.
pub fn is_stunned(args: &[Value]) -> Value {
    Value::Number(is_stunned_typed(n(args, 0) as i32) as f64)
}

// ── Rigid discs ──────────────────────────────────────────────────────────────
// `dynamic_system` runs in world_step phase 2, between `topdown_system` and `wrap_system`. See
// `struct Dynamic` for why contact uses a RANK rather than a mass.

/// `set_dynamic(e, diameter, restitution, friction, restSpeed, rank)` — one call: make the collider
/// a DISC of `diameter`, attach `Dynamic`, set the contact rank. `restitution`/`friction` are Q8
/// (256 = 1.0). Below `restSpeed` px/frame the body parks and stops costing anything.
///
/// One call rather than three (`set_circle` + `set_dynamic` + `set_rank`) because every export is
/// boot heap for EVERY ROM that imports the engine, golf or not — `examples/bench-boot` measured
/// the namespace table's cost and three names for one concept is two names too many.
pub fn set_dynamic(args: &[Value]) -> Value {
    set_dynamic_typed(
        n(args, 0) as i32,
        to_fixed(n(args, 1)),
        n(args, 2) as i32,
        n(args, 3) as i32,
        to_fixed(n(args, 4)),
        n(args, 5) as i32,
    );
    Value::Null
}

/// `body_impulse(e, turn, speed)` — add `speed` px/frame along `turn`, and WAKE the body.
///
/// ⚠️ `turn` is in 1/256ths of a turn, NOT degrees. `fire_angle` divides by `360*256` on every
/// single call; `kart.rs` shows a 1/256th yaw reaching agb's sin/cos table with no arithmetic at
/// all. This is golf's aim-and-power, and it costs zero divisions.
pub fn body_impulse(args: &[Value]) -> Value {
    body_impulse_typed(n(args, 0) as i32, n(args, 1) as i32, to_fixed(n(args, 2)));
    Value::Null
}

/// `body_kick(e, fromX, fromY, speed)` — impulse directly AWAY from a point. One sqrt and one
/// division, paid per kick, never per frame.
pub fn body_kick(args: &[Value]) -> Value {
    body_kick_typed(
        n(args, 0) as i32,
        to_fixed(n(args, 1)),
        to_fixed(n(args, 2)),
        to_fixed(n(args, 3)),
    );
    Value::Null
}

/// `body_asleep(e)` — 1 once the body has come to rest.
///
/// This is golf's whole game loop. A caller diffing `entity_x` two frames apart cannot tell
/// "resting" from "moving very slowly", and guessing a threshold in tish re-implements — badly —
/// a decision the engine already made with the real velocity.
pub fn body_asleep(args: &[Value]) -> Value {
    Value::Number(body_asleep_typed(n(args, 0) as i32) as f64)
}

/// `body_speed2(e)` — speed SQUARED, raw Q8. Squared because a power meter and a verify.sh
/// assertion both work fine on it and neither needs the square root.
pub fn body_speed2(args: &[Value]) -> Value {
    Value::Number(body_speed2_typed(n(args, 0) as i32) as f64)
}

/// `body_last_hit(e)` — the entity that last pushed `e`, or 0. Soccer's last-toucher (own goals,
/// assists) with no per-contact tish callback.
pub fn body_last_hit(args: &[Value]) -> Value {
    Value::Number(body_last_hit_typed(n(args, 0) as i32) as f64)
}

/// `grid_set_surface(col, row, id)` — one tile's surface class, 0..15.
///
/// For RUNTIME edits only (a divot, a broken wall). Author a course through `grid_from_gids`:
/// `examples/bench-boot` measured a per-tile tish marking loop at ~0.175 frames PER TILE, and that
/// measurement was one example's four-second boot.
pub fn grid_set_surface(args: &[Value]) -> Value {
    grid_set_surface_typed(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32);
    Value::Null
}

/// `surface_def(id, ax, ay, friction)` — what a class DOES. `ax`/`ay` are px/frame^2 (a slope,
/// wind, a conveyor); `friction` is Q8 retention (240 ~ green, 200 ~ rough, 150 ~ sand, 256 = ice).
pub fn surface_def(args: &[Value]) -> Value {
    surface_def_typed(
        n(args, 0) as i32,
        to_fixed(n(args, 1)),
        to_fixed(n(args, 2)),
        n(args, 3) as i32,
    );
    Value::Null
}

// ── Entity pools ─────────────────────────────────────────────────────────────
// A pool is a fixed set of entities created ONCE and re-armed forever after (docs/perf-rules.md §6:
// a spawn costs ~1,400 ticks on the frame it happens). It owns no policy — no `maxLive`, no
// cooldown, no per-slot callback — because the six hand-rolled pools this replaces each enforced
// different rules, structurally, by which slot they armed. See `packages/pool.tish` for the field
// and sentinel names, which is a constants-only file with no functions in it at all.

/// `pool_new(count, sheet, ox, oy)` — create `count` entities with sprites off `sheet` (-1 for an
/// entity-only pool), hidden and free. Returns the pool id.
pub fn pool_new(args: &[Value]) -> Value {
    Value::Number(pool_new_typed(
        n(args, 0) as i32,
        n(args, 1) as i32,
        n(args, 2) as i32,
        n(args, 3) as i32,
    ) as f64)
}

/// `pool_arm(p, slot, kind, ttl)` — arm a slot and return its entity id, or -1.
/// `slot >= 0` refuses a LIVE slot; -1 takes the lowest free; -2 steals the shortest-lived.
/// `ttl > 0` hands retirement to `life_system`; `ttl = 0` means the slot lives until retired.
pub fn pool_arm(args: &[Value]) -> Value {
    Value::Number(pool_arm_typed(
        n(args, 0) as i32,
        n(args, 1) as i32,
        n(args, 2) as i32,
        n(args, 3) as i32,
    ) as f64)
}

/// `pool_retire(p, slot)` — park a slot. Never despawns.
pub fn pool_retire(args: &[Value]) -> Value {
    pool_retire_typed(n(args, 0) as i32, n(args, 1) as i32);
    Value::Null
}

/// `pool_clear(p)` — retire every live slot in one call.
pub fn pool_clear(args: &[Value]) -> Value {
    pool_clear_typed(n(args, 0) as i32);
    Value::Null
}

/// `pool_get(p, slot, field)` — 0 kind · 1 ttl · 2 entity · 3 sprite.
pub fn pool_get(args: &[Value]) -> Value {
    Value::Number(pool_get_typed(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32) as f64)
}

/// `pool_stat(p, field)` — 0 count · 1 live · 2 high-water.
pub fn pool_stat(args: &[Value]) -> Value {
    Value::Number(pool_stat_typed(n(args, 0) as i32, n(args, 1) as i32) as f64)
}

/// `clear_world()` — despawn every entity and reset the grid, for a scene transition.
/// The sugar's `loadScene` calls this, then resets tish-agb's arenas, then builds anew.
pub fn clear_world(_args: &[Value]) -> Value {
    with_world(|w| w.clear_world());
    Value::Null
}

/// `set_transform(entity, x, y)` — position (fixed-point).
pub fn set_transform(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let x = to_fixed(n(args, 1));
    let y = to_fixed(n(args, 2));
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.transform[s] = Transform { x, y };
            w.mask[s] |= C_TRANSFORM;
            w.used |= C_TRANSFORM;
        }
    });
    Value::Null
}

/// `set_body(entity, vx, vy)` — velocity (fixed-point per frame); the movement
/// system integrates it into the transform every `world_step`.
pub fn set_body(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let vx = to_fixed(n(args, 1));
    let vy = to_fixed(n(args, 2));
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.body[s] = Body { vx, vy };
            w.mask[s] |= C_BODY;
            w.used |= C_BODY;
        }
    });
    Value::Null
}

/// `set_collider(entity, w, h)` — give an entity a `w`×`h` collision box (top-left at
/// its transform). Overlapping colliders fire `onCollide` behaviour callbacks.
pub fn set_collider(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let w = to_fixed(n(args, 1));
    let h = to_fixed(n(args, 2));
    with_world(|w2| {
        if let Some(s) = w2.slot_of(e) {
            w2.collider[s] = Collider { w, h };
            w2.mask[s] |= C_COLLIDER;
            w2.used |= C_COLLIDER;
        }
    });
    Value::Null
}

/// `overlaps(a, b)` — do two entities' colliders currently overlap? For imperative
/// checks (triggers, range tests) alongside the `onCollide` event.
pub fn overlaps(args: &[Value]) -> Value {
    let a = n(args, 0) as i32;
    let b = n(args, 1) as i32;
    Value::Bool(with_world(|w| w.entities_overlap(a, b)))
}

// ── Side-scrolling platformer genre ABI ────────────────────────────────────────

/// `set_platformer(entity)` — give an entity platformer physics (gravity + solid-tile
/// collision). It also needs a `set_collider` hitbox and a transform; the solid grid comes
/// from `grid_setup`/map loading. Drive it with `platformer_walk` / `platformer_jump`.
pub fn set_platformer(args: &[Value]) -> Value {
    with_world(|w| w.set_platformer(n(args, 0) as i32));
    Value::Null
}

/// `platformer_walk(entity, dir)` — horizontal move intent this frame (dir<0 left, 0 stop,
/// >0 right). Call every frame from input.
pub fn platformer_walk(args: &[Value]) -> Value {
    with_world(|w| w.platformer_walk(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `platformer_run(entity, on)` — run speed instead of walk this frame (hold the run button).
pub fn platformer_run(args: &[Value]) -> Value {
    with_world(|w| w.platformer_run(n(args, 0) as i32, truthy(args, 1)));
    Value::Null
}

/// `platformer_jump(entity)` — buffer a jump (edge-trigger on press). Fires if grounded or within
/// coyote time, so presses just before landing / just after a ledge still jump.
pub fn platformer_jump(args: &[Value]) -> Value {
    with_world(|w| w.platformer_jump(n(args, 0) as i32));
    Value::Null
}

/// `platformer_jump_release(entity)` — cut the jump short if still rising (variable jump height).
/// Edge-trigger on release.
pub fn platformer_jump_release(args: &[Value]) -> Value {
    with_world(|w| w.platformer_jump_release(n(args, 0) as i32));
    Value::Null
}

/// `platformer_drop(entity)` — fall through a one-way platform (hold Down + jump).
pub fn platformer_drop(args: &[Value]) -> Value {
    with_world(|w| w.platformer_drop(n(args, 0) as i32));
    Value::Null
}

/// `platformer_grounded(entity)` — is the entity resting on a solid this frame? (Jump gating,
/// idle/run/fall animation.)
pub fn platformer_grounded(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Bool(with_world(|w| {
        w.slot_of(e)
            .map(|s| w.has(s, C_PLATFORMER) && w.platformer[s].grounded)
            .unwrap_or(false)
    }))
}

/// `platformer_face(entity)` — which way the entity is facing: -1 left, +1 right. This is the last
/// direction it actually MOVED, not this frame's input, so it survives the d-pad being released —
/// which is what sprite-flip and `platformer_interact` need. Every platformer example used to keep
/// its own `data.face` copy of exactly this.
pub fn platformer_face(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_PLATFORMER))
            .map(|s| w.platformer[s].face as f64)
            .unwrap_or(1.0)
    }))
}

/// `platformer_blocked(entity)` — did a wall stop the entity's horizontal move this frame? Patrol
/// AI turns around on this.
pub fn platformer_blocked(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Bool(with_world(|w| {
        w.slot_of(e)
            .map(|s| w.has(s, C_PLATFORMER) && w.platformer[s].blocked)
            .unwrap_or(false)
    }))
}

/// `platformer_bounce(entity, vel)` — launch the entity upward at `vel` px/frame (stomp, spring).
pub fn platformer_bounce(args: &[Value]) -> Value {
    with_world(|w| w.platformer_bounce(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `platformer_set_vy(entity, vy)` — set vertical velocity in px/frame (fractions allowed;
/// negative rises, positive falls). The counterpart to `platformer_vy`, and the general form of
/// `platformer_bounce`, which can only launch upward. A wall slide clamps the fall with this
/// (`platformer_set_vy(e, 0.75)`); so does an updraft, a slam, or a slow-fall item.
pub fn platformer_set_vy(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let vy_raw = (n(args, 1) * 256.0) as i32;
    with_world(|w| w.platformer_set_vy(e, vy_raw));
    Value::Null
}

/// `platformer_set_speed(entity, walk, run)` — this body's ground speeds in px/frame, overriding the
/// engine defaults (1.25 walk / 2.25 run). Pass 0 for either to keep the default.
///
/// Per ENTITY, not per game: the defaults are shared by every platformer in the repo, so tuning them
/// to make one game's hero quicker would silently retune five others. Set once from the component's
/// `start`; it is not a per-frame call. Slide inherits the run speed, since a slide IS a run burst.
pub fn platformer_set_speed(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let walk_raw = (n(args, 1) * 256.0) as i32;
    let run_raw = (n(args, 2) * 256.0) as i32;
    with_world(|w| w.platformer_set_speed(e, walk_raw, run_raw));
    Value::Null
}

/// `platformer_set_physics(entity, jumpVel, gravity)` — this body's jump impulse and gravity in
/// px/frame (0 keeps the engine default). Held-weight physics: heavy cargo lowers `jumpVel`,
/// buoyant cargo lowers `gravity`. Same zero-means-default contract as `platformer_set_speed`.
pub fn platformer_set_physics(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let jump_raw = (n(args, 1) * 256.0) as i32;
    let grav_raw = (n(args, 2) * 256.0) as i32;
    with_world(|w| w.platformer_set_physics(e, jump_raw, grav_raw));
    Value::Null
}

/// `platformer_launch(entity, vx, vy)` — throw arc, px/frame: a persistent horizontal velocity
/// (clears itself on landing or on hitting a wall) plus an immediate vertical velocity (negative
/// = up). This is how a thrown body flies: a plain platformer body has no persistent vx.
pub fn platformer_launch(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let vx_raw = (n(args, 1) * 256.0) as i32;
    let vy_raw = (n(args, 2) * 256.0) as i32;
    with_world(|w| w.platformer_launch(e, vx_raw, vy_raw));
    Value::Null
}

/// `platformer_vy(entity)` — the entity's vertical velocity in px/frame (negative = rising, positive
/// = falling). Lets a component tell a Jump (rising) from a Fall (falling) for animation.
pub fn platformer_vy(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_PLATFORMER))
            .map(|s| w.platformer[s].vy.to_raw() as f64 / 256.0)
            .unwrap_or(0.0)
    }))
}

/// `platformer_hold(entity, on)` — freeze the entity's platformer body in place (no gravity, no
/// movement) while `on`, e.g. hanging on a ledge grab. Release (`on=0`) resumes normal physics
/// (it falls). Position it and set `held` on grab; teleport + release to climb up.
pub fn platformer_hold(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let on = n(args, 1) != 0.0;
    with_world(|w| {
        if let Some(s) = w.slot_of(e).filter(|&s| w.has(s, C_PLATFORMER)) {
            w.platformer[s].held = on;
        }
    });
    Value::Null
}

/// `set_patrol(entity, flipMode?)` — native patrol AI (walk + turn at walls/ledges), all in Rust.
/// Needs a `set_platformer` body. No per-frame tish callback, so many on-screen enemies stay cheap.
/// `flipMode`: 0/omitted = don't touch the sprite, 1 = mirror when walking RIGHT (art faces left),
/// 2 = mirror when walking LEFT (art faces right).
pub fn set_patrol(args: &[Value]) -> Value {
    with_world(|w| w.set_patrol(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `tile_solid(col, row)` — is that map tile solid? (AI probing the level; out-of-bounds = solid.)
pub fn tile_solid(args: &[Value]) -> Value {
    Value::Bool(with_world(|w| {
        w.is_solid(n(args, 0) as i32, n(args, 1) as i32)
    }))
}

// ── Free top-down (action-RPG) genre ABI ───────────────────────────────────────

/// `set_topdown(entity)` — give an entity free 8-directional movement with solid-tile collision. It
/// also needs a `set_collider` hitbox + a transform; the solid grid comes from the loaded map. Drive
/// it with `topdown_move` each frame (player input or chase AI).
pub fn set_topdown(args: &[Value]) -> Value {
    with_world(|w| w.set_topdown(n(args, 0) as i32));
    Value::Null
}

/// `set_blocker(entity)` — this entity's collider blocks top-down movers (an NPC you can't walk
/// through). Needs a collider + transform; the box moves with the entity.
pub fn set_blocker(args: &[Value]) -> Value {
    with_world(|w| w.set_blocker(n(args, 0) as i32));
    Value::Null
}

/// `topdown_snap(entity, mode)` — pick the entity's character-controller profile. These are whole
/// personalities, not combinable flags:
///   0 — free 8-direction movement (the default): pixel-exact, diagonals allowed, no grid.
///   2 — tile stepping: one direction commits a full 16px cell and it rests centred on the cell it
///       lands on. Input during the step is ignored until the step finishes.
pub fn topdown_snap(args: &[Value]) -> Value {
    with_world(|w| w.set_topdown_snap(n(args, 0) as i32, n(args, 1) as u8));
    Value::Null
}

/// `set_chase(entity, aggro, stride, flap, animSpeed)` — native chase-the-player AI (no per-frame
/// tish tick). `stride` = the sheet's cols/direction-row (5 for idle+4walk), or 0 for a
/// non-directional flap loop over frames `0..flap`.
pub fn set_chase(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_chase(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            n(args, 4) as i32,
        )
    });
    Value::Null
}

// ── RTS ABI: flow fields, seek, attack-move, fog ─────────────────────────────

/// `flow_goal(field, col, row)` — (re)build shared flow field `field` (0..3) so that every cell
/// holds its step count to (col,row). Idempotent: re-issuing the goal it already has costs nothing,
/// so a game may call this every frame from its order handler without checking.
///
/// This is the RTS counterpart to `isob_path`. `isob_path` is one route for one unit; a flow field is
/// one search that any number of units read in O(1), which is the only affordable shape when twelve
/// units share a destination.
pub fn flow_goal(args: &[Value]) -> Value {
    with_world(|w| w.flow_goal(n(args, 0) as usize, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `flow_dist(field, col, row)` — steps from that cell to the field's goal, or -1 if it cannot get
/// there. Useful to grey out an unreachable order before issuing it.
pub fn flow_dist(args: &[Value]) -> Value {
    Value::Number(with_world(|w| {
        w.flow_dist(n(args, 0) as usize, n(args, 1) as i32, n(args, 2) as i32)
    }) as f64)
}

/// `flow_ready(field)` — has this field been built at least once?
pub fn flow_ready(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    Value::Bool(with_world(|w| id < MAX_FLOWS && w.flows[id].ready))
}

/// `set_seek(entity, field, arrivePx, stride, animSpeed)` — walk this entity down flow field
/// `field` until it is within `arrivePx` (manhattan) of the goal. Needs `set_topdown` for the
/// movement and tile collision. `stride` is the sheet's columns-per-facing-row (0 = the game owns
/// the frames).
///
/// The missing sibling of `set_chase` (which follows an *entity*) and `set_mover` (which follows a
/// *pattern*): this follows a *destination*, which is the only order an RTS ever gives.
pub fn set_seek(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_seek(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            n(args, 4) as i32,
        )
    });
    Value::Null
}

/// `clear_seek(entity)` — cancel the move order and stop where it stands.
pub fn clear_seek(args: &[Value]) -> Value {
    with_world(|w| w.clear_seek(n(args, 0) as i32));
    Value::Null
}

/// `seek_arrived(entity)` — true once it is inside its arrive radius (or has no order).
pub fn seek_arrived(args: &[Value]) -> Value {
    Value::Bool(with_world(|w| w.seek_arrived(n(args, 0) as i32)))
}

/// `set_soldier(entity, team, rangePx, damage, cooldown)` — attack-move. The unit walks its seek
/// order while nothing hostile is in range, stops to hit the nearest enemy soldier when one is, and
/// resumes when that target dies. `team` (not the entity tag) decides who is hostile, so a game can
/// use one unit kind on both sides and keep tags for its own purposes.
pub fn set_soldier(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_soldier(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            n(args, 4) as i32,
        )
    });
    Value::Null
}

/// `soldier_target(entity)` — the entity it is currently engaging, or -1. This is what a HUD reads
/// to draw a target reticle, and what an AI reads to decide it is already busy.
pub fn soldier_target(args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.soldier_target(n(args, 0) as i32)) as f64)
}

/// `soldier_team(entity)` — the team set by `set_soldier`, or -1.
pub fn soldier_team(args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.soldier_team(n(args, 0) as i32)) as f64)
}

/// `terrain_load(cols, rows, gids, solid)` — load a map from two flat arrays and size the collision
/// grid from the same data, so terrain and pathing cannot disagree.
///
/// Use this INSTEAD of `scene:` when the game also paints an overlay layer (fog) from the same
/// tileset: two bakers over one PNG give two palette orderings, and the GBA has one set of
/// background palettes, so one of the two layers ends up drawing in the other's colours.
pub fn terrain_load(args: &[Value]) -> Value {
    let cols = n(args, 0) as i32;
    let rows = n(args, 1) as i32;
    with_world(|w| w.terrain_load(cols, rows, args.get(2), args.get(3)));
    Value::Null
}

/// `terrain_set(col, row, gid, solid)` — repaint one cell and set its collision.
pub fn terrain_set(args: &[Value]) -> Value {
    with_world(|w| {
        w.terrain_set(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
        )
    });
    Value::Null
}

/// `terrain_blit(bg, tileset, tsCols, camX, camY, gidUnseen)` — paint the camera's terrain window,
/// writing only the cells that changed, and return how many were written.
///
/// `gidUnseen` folds the fog in: a cell the viewing team has never seen is painted with that tile
/// instead of its terrain (pass 0 to disable and draw plain terrain).
pub fn terrain_blit(args: &[Value]) -> Value {
    Value::Number(with_world(|w| {
        w.terrain_blit(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            n(args, 4) as i32,
            n(args, 5) as i32,
        )
    }) as f64)
}

/// `set_sleeping(entity, on)` — mark a pooled slot dormant (1) or in play (0).
///
/// A sleeping entity is skipped by every per-frame system, so a game can hold a large unit pool
/// without paying for it. Arm the slot (`set_sleeping(e, 0)`) to bring it back.
pub fn set_sleeping(args: &[Value]) -> Value {
    with_world(|w| {
        if let Some(s) = w.slot_of(n(args, 0) as i32) {
            if n(args, 1) != 0.0 {
                w.mask[s] |= C_SLEEP;
                w.used |= C_SLEEP;
            } else {
                w.mask[s] &= !C_SLEEP;
            }
        }
    });
    Value::Null
}

/// `fog_init(cols, rows)` — allocate the fog plane at map size. Every cell starts unseen.
pub fn fog_init(args: &[Value]) -> Value {
    with_world(|w| w.fog_init(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `set_vision(entity, radiusCells)` — this entity reveals fog around itself every frame (0 = off).
pub fn set_vision(args: &[Value]) -> Value {
    with_world(|w| w.set_vision(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `fog_reveal(col, row, radiusCells)` — reveal a disc by hand (a scripted reveal, a building's
/// static sight). Entities with `set_vision` do not need this.
pub fn fog_reveal(args: &[Value]) -> Value {
    with_world(|w| w.fog_reveal(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `fog_state(col, row)` — 0 unseen, 1 explored, 2 visible. The gate an RTS puts in front of
/// "should this enemy sprite be drawn" and "may I click this".
pub fn fog_state(args: &[Value]) -> Value {
    let (c, r) = (n(args, 0) as i32, n(args, 1) as i32);
    Value::Number(with_world(|w| {
        if !w.fog.on || c < 0 || r < 0 || c >= w.fog.cols || r >= w.fog.rows {
            FOG_UNSEEN as i32
        } else {
            w.fog.state[(r * w.fog.cols + c) as usize] as i32
        }
    }) as f64)
}

/// `fog_blit(bg, tileset, tsCols, camX, camY, gidUnseen, gidExplored)` — paint the shroud layer for
/// the camera's window, writing only the cells that changed, and return how many were written.
/// A visible cell is blanked; the other two states use the gids you pass.
///
/// Pair the layer with `bg_parallax(bg, 256, 256)` so it tracks the camera: the shroud is a 16x16
/// wrapping window, and that wrap is what lets one small layer cover a map of any size.
///
/// ⚠️ `tileset` should be **the map's own tileset**, not a separate shroud image. `tilemap_new`
/// uploads its asset's palettes to all sixteen background banks, so a shroud built from its own
/// two-colour PNG repaints the whole map in those two colours — and building it in the other order
/// leaves the shroud drawing in whatever colour the map happens to keep at that palette index
/// (measured in `examples/rts-fog`: a brown shroud). Bake the shroud cells into the map's tileset
/// and both layers share one palette by construction.
pub fn fog_blit(args: &[Value]) -> Value {
    Value::Number(with_world(|w| {
        w.fog_blit(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            n(args, 4) as i32,
            n(args, 5) as i32,
            n(args, 6) as i32,
        )
    }) as f64)
}

/// `topdown_speed(entity, px)` — set move speed in px/frame (default 1.25). Keep it under 16.
pub fn set_wanderer(args: &[Value]) -> Value {
    if let [Value::Number(e), Value::Number(turn_rate)] = args {
        let (e, tr) = (*e as i32, *turn_rate as i32);
        with_world(|w| {
            if let Some(s) = w.slot_of(e).filter(|&s| w.has(s, C_TOPDOWN)) {
                w.wanderer[s] = Wanderer {
                    turn_rate: tr.clamp(0, 255),
                    turn_timer: 0,
                    want_shoot: 0,
                    // A sentinel no world position can equal, so the FIRST frame never looks
                    // "blocked" just because there is no previous sample yet.
                    last_x: i32::MIN,
                    last_y: i32::MIN,
                    home_rx: 0,
                    home_ry: 0,
                };
                // Capture the room it was placed in. castPlace sets the transform before reaching
                // the AI switch, so the position is already correct here.
                if w.room_cam.enabled {
                    let (rw, rh) = (w.room_cam.room_w * TILE, w.room_cam.room_h * TILE);
                    let (fx, fy) = w.camera_focus(s);
                    w.wanderer[s].home_rx = fx.div_euclid(rw);
                    w.wanderer[s].home_ry = fy.div_euclid(rh);
                }
                w.mask2[s] |= M2_WANDERER;
                w.used2 |= M2_WANDERER;
            }
        });
    }
    Value::Null
}

/// Sub-pixel movement speed, in raw 8.8 fixed point (256 = 1 px/frame).
///
/// ⚠️⚠️ THIS EXISTS BECAUSE `topdown_speed` TAKES WHOLE PIXELS, AND THAT SILENTLY FLATTENED AN
/// ENTIRE BESTIARY. Its minimum non-zero speed is 1 px/frame, but most NES-era walkers are slower
/// than that: the standard speed of 0x20 over the original's four sub-steps per frame is 0.5
/// px/frame, and the armoured knight's 0x28 is 0.625. Neither is representable, so every enemy in the game was given speed
/// 1 — twice the intended pace for the common case, and identical for all 63 species. Speed is
/// stored as 8.8 internally either way; only the setter's units were lossy.
pub fn topdown_speed_raw(args: &[Value]) -> Value {
    if let [Value::Number(e), Value::Number(raw)] = args {
        let (e, raw) = (*e as i32, *raw as i32);
        with_world(|w| {
            if let Some(s) = w.slot_of(e).filter(|&s| w.has(s, C_TOPDOWN)) {
                w.topdown[s].speed = raw.max(0);
            }
        });
    }
    Value::Null
}

/// Has this wanderer just turned to face the target (i.e. is it lined up)? Drives a shot windup.
pub fn wanderer_wants_shot(args: &[Value]) -> Value {
    if let [Value::Number(e)] = args {
        let e = *e as i32;
        return with_world(|w| match w.slot_of(e) {
            Some(s) if w.mask2[s] & M2_WANDERER != 0 && w.wanderer_wants_shot(s) => {
                Value::Number(1.0)
            }
            _ => Value::Number(0.0),
        });
    }
    Value::Number(0.0)
}

pub fn set_hopper(args: &[Value]) -> Value {
    if let [Value::Number(e), Value::Number(stride)] = args {
        let e = *e as i32;
        with_world(|w| {
            if let Some(s) = w.slot_of(e) {
                w.hopper[s] = Hopper {
                    stride: *stride as i32,
                    timer: 30,
                    state: 0,
                    start_x: Fixed::from_raw(0),
                    start_y: Fixed::from_raw(0),
                    dir_x: 0,
                    dir_y: 0,
                };
                w.mask[s] |= C_HOPPER;
                w.used |= C_HOPPER;
            }
        });
    }
    Value::Null
}

pub fn set_jumper(args: &[Value]) -> Value {
    if let [Value::Number(e)] = args {
        let e = *e as i32;
        with_world(|w| {
            if let Some(s) = w.slot_of(e) {
                w.jumper[s] = Jumper::default();
                w.mask[s] |= C_JUMPER;
                w.used |= C_JUMPER;
            }
        });
    }
    Value::Null
}

pub fn topdown_speed(args: &[Value]) -> Value {
    with_world(|w| w.topdown_speed(n(args, 0) as i32, n(args, 1)));
    Value::Null
}

/// `topdown_move(entity, dx, dy)` — move intent this frame (each ∈ {-1,0,1}). Call every frame; 0/0
/// stops. Updates facing (horizontal priority).
pub fn topdown_move(args: &[Value]) -> Value {
    with_world(|w| w.topdown_move(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `topdown_facing(entity)` — 0 down / 1 up / 2 left / 3 right (persists while idle). For animation
/// and to aim a `swing`.
pub fn topdown_facing(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_TOPDOWN))
            .map(|s| w.topdown[s].facing)
            .unwrap_or(0)
    }) as f64)
}

/// `topdown_moving(entity)` — did it have move intent this frame? (walk vs idle animation.)
pub fn topdown_moving(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Bool(with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_TOPDOWN))
            .map(|s| w.topdown[s].moving)
            .unwrap_or(false)
    }))
}

/// `topdown_knockback(entity, dx, dy, power?)` — shove the entity in (dx, dy) for a few frames,
/// overriding input (a scripted push; contact hits already knock back automatically). `power` raw = 0
/// uses the default.
pub fn topdown_knockback(args: &[Value]) -> Value {
    with_world(|w| {
        w.topdown_knockback(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
        )
    });
    Value::Null
}

/// `swing(attacker, targetTag, damage, reach, size, ttl)` — spawn a short-lived melee hurt box in
/// front of a top-down `attacker` (aimed by its facing) that deals `damage` to `targetTag` entities
/// for `ttl` frames. Returns the hitbox entity id. Pair with the attacker's attack animation / a
/// slash FX for the visual.
pub fn swing(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let target = n(args, 1) as i32;
    let dmg = n(args, 2) as i32;
    let reach = n(args, 3) as i32;
    let size = n(args, 4) as i32;
    let ttl = n(args, 5) as i32;
    Value::Number(with_world(|w| w.swing(e, target, dmg, reach, size, ttl)) as f64)
}

// ── Health / combat ABI ────────────────────────────────────────────────────────

/// `set_health(entity, max, invuln?)` — give an entity `max` hit points (starts full). The optional
/// `invuln` sets how many i-frames a hit grants: omit it for the usual post-hit mercy window
/// (`INVULN_FRAMES`), or pass `0` for a shmup enemy that should take every bullet in a stream.
pub fn set_health(args: &[Value]) -> Value {
    let invuln = match args.get(2) {
        Some(Value::Number(v)) => *v as i32,
        _ => INVULN_FRAMES,
    };
    with_world(|w| w.set_health(n(args, 0) as i32, n(args, 1) as i32, invuln));
    Value::Null
}

/// `set_lifetime(entity, ttl)` — despawn the entity `ttl` frames from now (a timed bullet, an
/// explosion that clears when its clip ends). Combine with `set_despawn_offscreen` for a bullet.
pub fn set_lifetime(args: &[Value]) -> Value {
    with_world(|w| w.set_lifetime(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `set_despawn_offscreen(entity, on)` — when `on`, despawn the entity as soon as its box leaves the
/// visible area. The auto-cleanup a static-screen shooter needs (nothing culls without a camera).
/// `set_arena_wrap(on)` — when `on`, the 240×160 screen becomes a torus for EVERY entity: whatever
/// leaves one edge re-enters the opposite one, fully off-screen, so it slides across without a gap.
///
/// The Asteroids playfield. It is a world switch, not a per-entity component, because in a game that
/// wants it the rule is universal — and natively-spawned bullets have no tish handle to flag. Note
/// that nothing is ever off-screen while it is on, so `set_despawn_offscreen` stops retiring shots:
/// give them a `ttl` (`bullet_style`) instead, which is the classic behaviour anyway.
pub fn set_arena_wrap(args: &[Value]) -> Value {
    with_world(|w| w.set_arena_wrap(truthy(args, 0)));
    Value::Null
}

pub fn set_despawn_offscreen(args: &[Value]) -> Value {
    with_world(|w| w.set_despawn_offscreen(n(args, 0) as i32, truthy(args, 1)));
    Value::Null
}

/// `set_hurt(entity, damage, targetTag, despawnOnHit)` — make the entity deal `damage` on contact to
/// entities tagged `targetTag` that have health. A bullet passes `despawnOnHit = true` (consumed on
/// impact); a body hazard (an enemy the player can crash into) passes `false` and lets i-frames
/// rate-limit it. Resolved natively each frame, so a bullet needs no per-frame tish callback.
/// `set_shooter(entity, interval, speed, aimed)` — make an enemy fire every `interval` frames at
/// `speed` px/frame, with the bullet style that is in force at the time of this call. `aimed` 0 =
/// along its facing, 1 = at the player.
pub fn set_shooter(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_shooter(
            n(args, 0) as i32,
            n(args, 1) as i32,
            to_fixed(n(args, 2)),
            n(args, 3) as i32 != 0,
        )
    });
    Value::Null
}

/// `set_charger(entity, speed, band)` — bolt along an axis at `speed` while lined up with the
/// player within `band` px on the other axis.
/// `set_guard(entity, mask)` — block damage arriving from the direction the entity faces.
/// `mask` is 1 (melee/contact), 2 (projectiles) or 3 (both); 0 removes it. Needs a top-down entity,
/// because the guard is a direction. The player's shield and an armoured knight's armour are the same rule.
pub fn set_guard(args: &[Value]) -> Value {
    with_world(|w| w.set_guard(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `set_dir_anim(entity, base, stride, frames, speed)` — animate `frames` frames starting at
/// `base + facing * stride`, following the entity's facing (0 down, 1 up, 2 left, 3 right).
///
/// For a sheet SHARED by many actors, where `base` says where this one begins. `set_chase`'s own
/// directional mode assumes the entity owns frame 0 of its sheet and that rows are five wide.
pub fn set_dir_anim(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_dir_anim(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            n(args, 4) as i32,
        )
    });
    Value::Null
}

pub fn set_charger(args: &[Value]) -> Value {
    with_world(|w| w.set_charger(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `entity_alive(entity)` — 1 while the entity still exists. A game that keeps a handle to
/// something it spawned (the one boomerang that may be in the air) needs to ask rather than track:
/// the thing can also be retired by its lifetime, by the room cutoff, or by a scene load, and a
/// tracked bool goes stale on every one of those and strands the weapon forever.
pub fn entity_alive(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| match w.slot_of(e) {
        Some(s) if w.alive[s] => 1.0,
        _ => 0.0,
    }))
}

/// `entity_hp(entity) -> i32` — current hit points, or 0 if the entity has no health component.
///
/// This existed only as a method on the boxed `this` wrapper, which meant that any behaviour needing
/// to read its own health could not run as a `lean` tick — one method call forced the whole
/// component back onto the `update:` path and its ~8 ABI round trips per frame. A scalar getter is
/// all it ever needed. Pairs with `entity_hp_max` so a "am I at full health" test (a full-health
/// sword beam, a low-health warning) costs two typed calls and no boxing.
pub fn entity_hp(args: &[Value]) -> Value {
    Value::Number(entity_hp_typed(n(args, 0) as i32) as f64)
}

/// `entity_hp_max(entity) -> i32` — maximum hit points, or 0 if the entity has no health component.
pub fn entity_hp_max(args: &[Value]) -> Value {
    Value::Number(entity_hp_max_typed(n(args, 0) as i32) as f64)
}

/// `set_lure(entity, radius, frames)` — make `entity` a decoy the native AI prefers over the player
/// while it lasts. Enemies within `radius` px walk to it instead. `frames <= 0` clears it.
pub fn set_lure(args: &[Value]) -> Value {
    with_world(|w| w.set_lure(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `set_hurt(entity, damage, targetTag, despawnOnHit, stun = 0)` — see `set_hurt` on `World`.
pub fn set_hurt(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_hurt(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            truthy(args, 3),
            n(args, 4) as i32,
        )
    });
    Value::Null
}

/// `set_damage_type(entity, dmgBits)` — declare which WEAPON a hurt box is, as one `DMG_*` bit.
/// 0 (the default) is untyped and lands on everything, so nothing that predates this changes.
/// Pair with `set_immunity` on the victim.
pub fn set_damage_type(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let dt = n(args, 1) as i32;
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.hurt[s].damage_type = dt;
        }
    });
    Value::Null
}

/// See [`set_damage_type`].
pub fn set_damage_type_typed(e: i32, dt: i32) {
    set_damage_type(&[Value::Number(e as f64), Value::Number(dt as f64)]);
}

/// `set_immunity(entity, dmgBits)` — the weapon kinds that BOUNCE OFF this entity, as a mask of
/// `DMG_*` bits. 0 (the default) is hurt by everything. This is the classic per-monster
/// invincibility mask: it is what makes one boss ignore the sword and another answer only to an
/// arrow, without the collision code knowing what either monster is.
///
/// Survives `set_health`, deliberately — a boss that re-arms its hit points mid-fight must not
/// silently lose its immunities.
pub fn set_immunity(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let mask = n(args, 1) as i32;
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.immune[s] = mask;
        }
    });
    Value::Null
}

/// See [`set_immunity`].
pub fn set_immunity_typed(e: i32, mask: i32) {
    set_immunity(&[Value::Number(e as f64), Value::Number(mask as f64)]);
}

/// `set_weakness(entity, dmgBits)` — damage-type vulnerability mask (`DMG_*` bits). 0 = hurt by
/// everything; non-zero ALLOWS only matching weapon kinds. Complement of `set_immunity`: immunity
/// bounces listed types, weakness requires a listed type to land.
pub fn set_weakness(args: &[Value]) -> Value {
    with_world(|w| w.set_weakness(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// See [`set_weakness`].
pub fn set_weakness_typed(e: i32, mask: i32) {
    with_world(|w| w.set_weakness(e, mask));
}

/// `set_grabber(entity, targetTag)` — on overlap with a tagged target, briefly stun it (the
/// classic grab-on-contact enemy, lite). Needs a collider on both sides.
pub fn set_grabber(args: &[Value]) -> Value {
    with_world(|w| w.set_grabber(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// See [`set_grabber`].
pub fn set_grabber_typed(e: i32, target_tag: i32) {
    with_world(|w| w.set_grabber(e, target_tag));
}

/// `set_trap(entity)` — blade trap: inert until the player shares a row/col, then dash. Needs
/// `C_TOPDOWN`. Home position is snapshotted at configure time.
pub fn set_trap(args: &[Value]) -> Value {
    with_world(|w| w.set_trap(n(args, 0) as i32));
    Value::Null
}

/// See [`set_trap`].
pub fn set_trap_typed(e: i32) {
    with_world(|w| w.set_trap(e));
}

/// `set_carrier(entity)` — the entity's top edge becomes one-way moving ground for platformer
/// bodies: stand on a walking beast or a drifting raft, inherit its motion, jump off normally,
/// Down+jump drops through. Needs a transform + collider; the surface travels with the entity.
pub fn set_carrier(args: &[Value]) -> Value {
    with_world(|w| w.set_carrier(n(args, 0) as i32));
    Value::Null
}

/// See [`set_carrier`].
pub fn set_carrier_typed(e: i32) {
    with_world(|w| w.set_carrier(e));
}

/// `set_part(entity, parentId)` — glue this entity to `parentId` with the offset at configure time.
pub fn set_part(args: &[Value]) -> Value {
    with_world(|w| w.set_follow(n(args, 0) as i32, FOLLOW_PART, n(args, 1) as i32, 0));
    Value::Null
}

/// See [`set_part`].
pub fn set_part_typed(e: i32, parent_id: i32) {
    with_world(|w| w.set_follow(e, FOLLOW_PART, parent_id, 0));
}

/// `set_train(entity, headId)` — train-segment follow: same offset glue as `set_part`, parented to
/// the head (or previous car). Minimal stub for segmented-worm chains.
pub fn set_train(args: &[Value]) -> Value {
    with_world(|w| w.set_follow(n(args, 0) as i32, FOLLOW_TRAIN, n(args, 1) as i32, 0));
    Value::Null
}

/// See [`set_train`].
pub fn set_train_typed(e: i32, head_id: i32) {
    with_world(|w| w.set_follow(e, FOLLOW_TRAIN, head_id, 0));
}

/// `set_orbiter(entity, centerId, radius)` — circle `centerId` at `radius` px each frame.
pub fn set_orbiter(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_follow(
            n(args, 0) as i32,
            FOLLOW_ORBIT,
            n(args, 1) as i32,
            n(args, 2) as i32,
        )
    });
    Value::Null
}

/// See [`set_orbiter`].
pub fn set_orbiter_typed(e: i32, center_id: i32, radius: i32) {
    with_world(|w| w.set_follow(e, FOLLOW_ORBIT, center_id, radius));
}

/// `set_boomerang(entity, returnFrames)` — boomerang return-mover: after `returnFrames`, reverse
/// `Body` velocity toward the owner (camera target at configure time). Pair with `set_lifetime` /
/// `set_despawn_offscreen` like any other projectile. See also `set_mover`.
pub fn set_boomerang(args: &[Value]) -> Value {
    with_world(|w| w.set_boomerang(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// See [`set_boomerang`].
pub fn set_boomerang_typed(e: i32, return_frames: i32) {
    with_world(|w| w.set_boomerang(e, return_frames));
}

/// `boomerang_caught()` — how many boomerangs completed their return and were caught by their
/// owner since the last call (the counter clears on read). The catch also despawns the boomerang,
/// so `entity_alive` on it goes 0 — this is the "report catch" half of the return contract.
pub fn boomerang_caught(_args: &[Value]) -> Value {
    Value::Number(with_world(|w| {
        let c = w.boomer_catches;
        w.boomer_catches = 0;
        c
    }) as f64)
}

/// See [`boomerang_caught`].
pub fn boomerang_caught_typed() -> i32 {
    with_world(|w| {
        let c = w.boomer_catches;
        w.boomer_catches = 0;
        c
    })
}

// ── NES-era enemy AI natives ──────────────────────────────────────────────────────────────────

/// `set_ambusher(entity, hideFrames, surfaceFrames, speedQ8)` — burrower submerge-move-surface
/// cycle: hidden (intangible + invisible) drifting toward the player at `speedQ8` (1/256 px per
/// frame; 320 = 1.25 px), then surfaced (visible, vulnerable, stationary) — repeat. Needs
/// `C_TOPDOWN`. Compose with `set_shooter` for a surfacing shooter.
pub fn set_ambusher(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_ambusher(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
        )
    });
    Value::Null
}

/// See [`set_ambusher`].
pub fn set_ambusher_typed(e: i32, hide: i32, surface: i32, speed_q8: i32) {
    with_world(|w| w.set_ambusher(e, hide, surface, speed_q8));
}

/// `set_drifter(entity, restFrames, flyFrames, speedQ8)` — hovering-drifter floating wander with
/// spin-up/spin-down, INVULNERABLE while moving (only hittable at rest — the damage path enforces
/// it natively; `entity_phased` reads the same flag). Needs `C_TOPDOWN`.
pub fn set_drifter(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_drifter(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
        )
    });
    Value::Null
}

/// See [`set_drifter`].
pub fn set_drifter_typed(e: i32, rest: i32, fly: i32, speed_q8: i32) {
    with_world(|w| w.set_drifter(e, rest, fly, speed_q8));
}

/// `set_flicker_caster(entity, hideFrames, visFrames, shotSpeed)` — teleporting-caster flicker +
/// one aimed cast per appearance, with the bullet style in force at THIS call (the `set_shooter`
/// capture contract). `shotSpeed` is px/frame (fixed — NOT an integer; see the typed twin).
pub fn set_flicker_caster(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_flicker_caster(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            to_fixed(n(args, 3)),
        )
    });
    Value::Null
}

/// See [`set_flicker_caster`]. ⚠️ `shot_speed` is `Fixed`, matching the `fixed` declaration —
/// declaring it `i32` would truncate every sub-pixel speed to 0/1 (the platformer_set_speed trap).
pub fn set_flicker_caster_typed(e: i32, hide: i32, vis: i32, shot_speed: Fixed) {
    with_world(|w| w.set_flicker_caster(e, hide, vis, shot_speed));
}

/// `set_bouncer(entity, restFrames, hopFrames, speedQ8)` — parabolic bouncing hops: an
/// arc (drawn via the sprite `oy` offset), heading 50% at the player / 50% random per hop, tile
/// collision from the top-down mover. Needs `C_TOPDOWN`.
pub fn set_bouncer(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_bouncer(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
        )
    });
    Value::Null
}

/// See [`set_bouncer`].
pub fn set_bouncer_typed(e: i32, rest: i32, hop: i32, speed_q8: i32) {
    with_world(|w| w.set_bouncer(e, rest, hop, speed_q8));
}

/// `set_ricochet(entity, speedQ8)` — a diagonal that reflects off whatever stops it. See the
/// World impl for the full story; the NINE-BOUNCE fix was its first consumer.
pub fn set_ricochet(args: &[Value]) -> Value {
    with_world(|w| w.set_ricochet(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// See [`set_ricochet`].
pub fn set_ricochet_typed(e: i32, speed_q8: i32) {
    with_world(|w| w.set_ricochet(e, speed_q8));
}

/// `entity_phased(e)` — 1 while `e` is currently invulnerable/intangible under a nai state
/// machine or `set_phased` (drifter in flight, submerged burrower, vanished caster). The native
/// damage path already refuses such hits; this is for game-side damage logic (a custom sword
/// swing) to honour the same flag.
pub fn entity_phased(args: &[Value]) -> Value {
    Value::Number(entity_phased_typed(n(args, 0) as i32) as f64)
}

/// See [`entity_phased`].
pub fn entity_phased_typed(e: i32) -> i32 {
    with_world(|w| {
        w.slot_of(e)
            .map(|s| (w.mask2[s] & (M2_PHASED | M2_HIDDEN) != 0) as i32)
            .unwrap_or(0)
    })
}

/// `set_phased(e, on)` — set/clear the invulnerable flag directly (a core invulnerable while its
/// orbiting children live, a boss awaiting its trigger item). Tangible: contact damage still lands on the player.
pub fn set_phased(args: &[Value]) -> Value {
    with_world(|w| w.set_phased(n(args, 0) as i32, n(args, 1) as i32 != 0));
    Value::Null
}

/// See [`set_phased`].
pub fn set_phased_typed(e: i32, on: i32) {
    with_world(|w| w.set_phased(e, on != 0));
}

// ── Multi-part boss glue ──────────────────────────────────────────────────────────────────────

/// `set_hit_proxy(entity, targetId)` — route damage dealt to `entity` onto `targetId` instead
/// (boss neck→head; re-point per frame for the shrinking-tail rule). `targetId < 0`
/// clears. Follows at most 4 hops; each hop honours its own gate/phase.
pub fn set_hit_proxy(args: &[Value]) -> Value {
    with_world(|w| w.set_hit_proxy(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// See [`set_hit_proxy`].
pub fn set_hit_proxy_typed(e: i32, target: i32) {
    with_world(|w| w.set_hit_proxy(e, target));
}

/// `set_vuln_gate(entity, open)` — while `open == 0`, no damage lands on `entity` (an eye-open
/// gate, a boss's last-hit window). Combine with `set_weakness` for "arrow only AND eye open".
pub fn set_vuln_gate(args: &[Value]) -> Value {
    with_world(|w| w.set_vuln_gate(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// See [`set_vuln_gate`].
pub fn set_vuln_gate_typed(e: i32, open: i32) {
    with_world(|w| w.set_vuln_gate(e, open));
}

/// `set_death_note(entity, code)` — when `entity` dies, push `code` to the death-note queue.
/// The parent's logic drains it with `death_note()` — part-death notification without a per-part
/// tish callback.
pub fn set_death_note(args: &[Value]) -> Value {
    with_world(|w| w.set_death_note(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// See [`set_death_note`].
pub fn set_death_note_typed(e: i32, code: i32) {
    with_world(|w| w.set_death_note(e, code));
}

/// `death_note()` — pop the next part-death code, or 0 when the queue is empty. Use a non-zero
/// code per part.
pub fn death_note(_args: &[Value]) -> Value {
    Value::Number(death_note_typed() as f64)
}

/// See [`death_note`].
pub fn death_note_typed() -> i32 {
    with_world(|w| {
        if w.death_notes.is_empty() {
            0
        } else {
            w.death_notes.remove(0)
        }
    })
}

/// `detach_part(entity)` — sever the `set_part`/`set_train`/`set_orbiter` glue: the entity keeps
/// its sprite/health/position but moves on its own again (flying-head promotion — detach,
/// then give the same entity `set_drifter` + `set_shooter`).
pub fn detach_part(args: &[Value]) -> Value {
    with_world(|w| w.detach_part(n(args, 0) as i32));
    Value::Null
}

/// See [`detach_part`].
pub fn detach_part_typed(e: i32) {
    with_world(|w| w.detach_part(e));
}

/// See [`set_dir_anim`]. Typed twin: this is called once per enemy per room population, on a
/// shared 33-actor sprite strip, so the boxed path was pure marshalling overhead.
pub fn set_dir_anim_typed(e: i32, base: i32, stride: i32, frames: i32, speed: i32) {
    with_world(|w| w.set_dir_anim(e, base, stride, frames, speed));
}

/// `bullet_damage_type(dmgBits)` — stamp the CURRENT bullet style with a weapon kind, so the
/// bullets a `fire_*` spawns carry it. Sticky and set beside `bullet_style`, in either order.
/// Kept out of `bullet_style`'s argument list so its seven-argument signature — and every game
/// already calling it — is untouched.
pub fn bullet_damage_type(args: &[Value]) -> Value {
    let dt = n(args, 0) as i32;
    with_world(|w| w.bullet_style.damage_type = dt);
    Value::Null
}

/// See [`bullet_damage_type`].
pub fn bullet_damage_type_typed(dt: i32) {
    bullet_damage_type(&[Value::Number(dt as f64)]);
}

/// `entity_immunity(entity)` — the entity's current immunity mask, so a test can assert it without
/// having to infer it from damage that did or did not land.
pub fn entity_immunity(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| w.slot_of(e).map(|s| w.immune[s]).unwrap_or(0)) as f64)
}

/// See [`entity_immunity`].
pub fn entity_immunity_typed(e: i32) -> i32 {
    with_world(|w| w.slot_of(e).map(|s| w.immune[s]).unwrap_or(0))
}

// ── Native bullet emitters ABI ──────────────────────────────────────────────────
// `bullet_style` sets the shared config once; the `fire_*` calls then spawn whole patterns natively.
// A game resolves its options object once per burst (not per bullet) and hands the scalars here.

/// `bullet_style(sheet, frame, size, damage, target, tag, ttl)` — configure the next burst of bullets.
pub fn bullet_style(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_bullet_style(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            n(args, 4) as i32,
            n(args, 5) as i32,
            n(args, 6) as i32,
        )
    });
    Value::Null
}
pub fn bullet_style_typed(
    sheet: i32,
    frame: i32,
    size: i32,
    damage: i32,
    target: i32,
    tag: i32,
    ttl: i32,
) {
    with_world(|w| w.set_bullet_style(sheet, frame, size, damage, target, tag, ttl));
}

/// `fire_bullet(cx, cy, vx, vy)` — one bullet with the current style. Returns its entity id.
pub fn fire_bullet(args: &[Value]) -> Value {
    Value::Number(with_world(|w| {
        w.fire_bullet(
            to_fixed(n(args, 0)),
            to_fixed(n(args, 1)),
            to_fixed(n(args, 2)),
            to_fixed(n(args, 3)),
        )
    }) as f64)
}
pub fn fire_bullet_typed(cx: Fixed, cy: Fixed, vx: Fixed, vy: Fixed) -> i32 {
    with_world(|w| w.fire_bullet(cx, cy, vx, vy))
}

/// `fire_angle(cx, cy, deg, speed)` — one bullet at a heading (0 right, 90 down, −90 up).
pub fn fire_angle(args: &[Value]) -> Value {
    Value::Number(with_world(|w| {
        w.fire_angle(
            to_fixed(n(args, 0)),
            to_fixed(n(args, 1)),
            to_fixed(n(args, 2)),
            to_fixed(n(args, 3)),
        )
    }) as f64)
}
pub fn fire_angle_typed(cx: Fixed, cy: Fixed, deg: Fixed, speed: Fixed) -> i32 {
    with_world(|w| w.fire_angle(cx, cy, deg, speed))
}

/// `fire_ring(cx, cy, count, speed)` — a full ring of `count` evenly-spaced bullets.
pub fn fire_ring(args: &[Value]) -> Value {
    with_world(|w| {
        w.fire_ring(
            to_fixed(n(args, 0)),
            to_fixed(n(args, 1)),
            n(args, 2) as i32,
            to_fixed(n(args, 3)),
        )
    });
    Value::Null
}
pub fn fire_ring_typed(cx: Fixed, cy: Fixed, count: i32, speed: Fixed) {
    with_world(|w| w.fire_ring(cx, cy, count, speed));
}

/// `fire_spread(cx, cy, centerDeg, count, spreadDeg, speed)` — a fan of `count` bullets.
pub fn fire_spread(args: &[Value]) -> Value {
    with_world(|w| {
        w.fire_spread(
            to_fixed(n(args, 0)),
            to_fixed(n(args, 1)),
            to_fixed(n(args, 2)),
            n(args, 3) as i32,
            to_fixed(n(args, 4)),
            to_fixed(n(args, 5)),
        )
    });
    Value::Null
}
pub fn fire_spread_typed(
    cx: Fixed,
    cy: Fixed,
    center_deg: Fixed,
    count: i32,
    spread_deg: Fixed,
    speed: Fixed,
) {
    with_world(|w| w.fire_spread(cx, cy, center_deg, count, spread_deg, speed));
}

/// `fire_aimed(cx, cy, tox, toy, speed)` — one bullet aimed at a target point. Returns its entity id.
pub fn fire_aimed(args: &[Value]) -> Value {
    Value::Number(with_world(|w| {
        w.fire_aimed(
            to_fixed(n(args, 0)),
            to_fixed(n(args, 1)),
            to_fixed(n(args, 2)),
            to_fixed(n(args, 3)),
            to_fixed(n(args, 4)),
        )
    }) as f64)
}
pub fn fire_aimed_typed(cx: Fixed, cy: Fixed, tox: Fixed, toy: Fixed, speed: Fixed) -> i32 {
    with_world(|w| w.fire_aimed(cx, cy, tox, toy, speed))
}

/// `set_mover(entity, pattern, vy, amp, period)` — give an entity a NATIVE movement pattern driven by
/// `mover_system` in pure Rust (no per-frame tish `tick`). `pattern` 0 = straight down at `vy`;
/// 1 = weave (a sideways triangle of amplitude `amp` px over `period` frames while descending at `vy`).
/// The shmup counterpart to `set_patrol` — a screen full of weaving enemies costs zero tish calls.
/// For a boomerang-style return after N frames, see `set_boomerang` (reverses `Body` toward owner).
pub fn set_mover(args: &[Value]) -> Value {
    with_world(|w| {
        w.set_mover(
            n(args, 0) as i32,
            n(args, 1) as u8,
            to_fixed(n(args, 2)),
            to_fixed(n(args, 3)),
            n(args, 4) as i32,
        )
    });
    Value::Null
}

/// `damage(entity, amount)` — deal damage (ignored during i-frames). Returns whether it landed.
pub fn damage(args: &[Value]) -> Value {
    Value::Bool(with_world(|w| {
        w.damage(n(args, 0) as i32, n(args, 1) as i32)
    }))
}

/// `heal(entity, amount)` — restore hit points up to max.
pub fn heal(args: &[Value]) -> Value {
    with_world(|w| w.heal(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `health_hp(entity)` / `health_max(entity)` — current / maximum hit points (0 if no health).
/// `set_hp(entity, hp)` — set CURRENT hit points, leaving the maximum alone.
///
/// For restoring a carried or saved health value. `set_health` cannot do it (it resets the maximum
/// too) and `damage` must not (it fires i-frames and a death check), so without this a game that
/// wants a hero to keep his wounds across a scene load has no way to say so.
pub fn set_hp(args: &[Value]) -> Value {
    with_world(|w| {
        let e = n(args, 0) as i32;
        let hp = n(args, 1) as i32;
        if let Some(s) = w.slot_of(e).filter(|&s| w.has(s, C_HEALTH)) {
            let m = w.health[s].max;
            let v = hp.clamp(0, m);
            w.health[s].hp = v;
            w.health[s].dead = v <= 0;
        }
    });
    Value::Null
}

pub fn health_hp(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_HEALTH))
            .map(|s| w.health[s].hp)
            .unwrap_or(0)
    }) as f64)
}
pub fn health_max(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_HEALTH))
            .map(|s| w.health[s].max)
            .unwrap_or(0)
    }) as f64)
}

/// `health_alive(entity)` — is the entity present with hp > 0?
pub fn health_alive(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Bool(with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_HEALTH))
            .map(|s| w.health[s].hp > 0)
            .unwrap_or(false)
    }))
}

/// `entity_on_screen(entity)` — is the entity within the camera view (plus the cull margin)?
/// Off-screen entities have their behaviour/physics/animation/collision skipped automatically; this
/// query lets game logic react too (e.g. only spawn or activate something once it's on screen).
pub fn entity_on_screen(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Bool(with_world(|w| {
        w.slot_of(e).map(|s| w.on_screen(s)).unwrap_or(false)
    }))
}

/// `set_tag(entity, tag)` — label an entity with a game-defined integer kind (e.g. 1 = player,
/// 2 = enemy, 3 = pickup). A collision/interaction handler reads it back with `entity_tag` to tell
/// what it touched, so a pickup can grant only to the player and ignore an enemy that walks over it.
pub fn set_tag(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let t = n(args, 1) as i32;
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.tag[s] = t;
        }
    });
    Value::Null
}

/// `entity_tag(entity)` — the entity's kind tag (0 if untagged or unknown). See `set_tag`.
pub fn entity_tag(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| w.slot_of(e).map(|s| w.tag[s]).unwrap_or(0)) as f64)
}

// ── Typed per-entity component state (the fast tick path) ───────────────────────
// `cvar(e, k)` / `set_cvar(e, k, v)` give a component's `tick` 8 native i32 slots to keep its
// counters and flags in, instead of a boxed `Value` context object whose every `d.field` is a hashmap
// lookup. With typed args these lower to direct `cvar_typed`/`set_cvar_typed` calls — no boxing.

/// `cvar(entity, k)` — read typed state slot `k` (0..7). 0 for an unknown entity.
pub fn cvar(args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.cvar(n(args, 0) as i32, n(args, 1) as usize)) as f64)
}
pub fn cvar_typed(e: i32, k: i32) -> i32 {
    with_world(|w| w.cvar(e, k.max(0) as usize))
}
/// `set_cvar(entity, k, v)` — write typed state slot `k` (0..7).
pub fn set_cvar(args: &[Value]) -> Value {
    with_world(|w| w.set_cvar(n(args, 0) as i32, n(args, 1) as usize, n(args, 2) as i32));
    Value::Null
}
pub fn set_cvar_typed(e: i32, k: i32, v: i32) {
    with_world(|w| w.set_cvar(e, k.max(0) as usize, v));
}

// ── Grid / RPG genre ABI ──────────────────────────────────────────────────────

/// `grid_setup(cols, rows)` — create the tile-collision grid (all walkable).
pub fn grid_setup(args: &[Value]) -> Value {
    with_world(|w| w.grid_setup(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `grid_set_solid(col, row, solid)` — mark a tile solid (non-zero) or walkable (0).
pub fn grid_set_solid(args: &[Value]) -> Value {
    with_world(|w| w.grid_set_solid(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) != 0.0));
    Value::Null
}

/// `grid_from_map()` — size the collision grid to the loaded ROM map (`map_stream` /
/// `scene_stream`) and mark every solid tile, all in Rust. The tish equivalent was a w×h
/// interpreter loop with two native calls per cell, which dominated area load time.
pub fn grid_from_map(_args: &[Value]) -> Value {
    let grid = tish_agb::native_map_solid_grid();
    // The one-way and ladder planes are optional: a top-down map has neither, and every map baked
    // before they existed simply ends after its spawn list.
    let oneway = tish_agb::native_map_oneway_grid();
    let ladder = tish_agb::native_map_ladder_grid();
    if let Some((solid, width, height)) = grid {
        with_world(|w| {
            w.grid_setup(width, height);
            for row in 0..height {
                let base = (row * width) as usize;
                for col in 0..width {
                    let i = base + col as usize;
                    if let Some(l) = ladder {
                        if l[i] != 0 {
                            w.grid_set_ladder(col, row, true);
                        }
                    }
                    // One-way wins over solid where a map marks both, matching `grid_from_gids`:
                    // the softer rule leaves a passable platform rather than an invisible wall.
                    if oneway.map(|o| o[i] != 0).unwrap_or(false) {
                        w.grid_set_oneway(col, row, true);
                    } else if solid[i] != 0 {
                        w.grid_set_solid(col, row, true);
                    }
                }
            }
        });
    }
    Value::Null
}

/// Read a tish array of numbers into a `Vec<i32>`. Costs one `get_prop` plus one `get_index` per
/// element — which is exactly the crossing this module exists to make ONCE instead of per cell.
///
/// Anything that is not an array reads as empty, and that case is REACHED, not defensive: a map is a
/// plain tish object, so a platformer map with no one-way platforms simply has no `oneway` key and
/// passes null. The tish helper this replaced opened with `if (!list) { return false }` for exactly
/// that reason; taking `.length` off a null instead leaves a pending throw that kills the ROM at the
/// next check, which shows up as a white screen rather than as an error.
fn read_i32_array(v: &Value) -> Vec<i32> {
    match v {
        Value::Array(_) => {}
        _ => return Vec::new(),
    }
    let len = match get_prop(v, "length") {
        Value::Number(n) => n as usize,
        _ => 0,
    };
    let mut out = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        out.push(
            match tishlang_runtime_gba::get_index(v, &Value::Number(i as f64)) {
                Value::Number(n) => n as i32,
                _ => 0,
            },
        );
        i += 1;
    }
    out
}

/// `grid_from_gids(width, height, data, solid, oneway)` — size the collision grid and mark every
/// solid / one-way cell from a tish GID array, all in Rust.
///
/// This is [`grid_from_map`]'s counterpart for maps that live in the GAME rather than in ROM. A
/// `scene:`-imported map is packed at build time and `grid_from_map` reads it straight out of ROM;
/// a map written as a tish literal (`{ width, height, solid: [...], layers: [{ data: [...] }] }`)
/// has no ROM form, so `loadStreamMap` walked it in the interpreter instead — a w×h loop doing four
/// property lookups, two function calls and up to two native calls **per cell**.
///
/// That loop was the entire boot time of every game using it. Measured on `sunny-land` (102×15):
/// rendering all 1,530 tiles through `tilemap_stream` cost 16 frames, and marking their collision in
/// tish cost **218** — 3.65 seconds of black screen, ~40,000 CPU cycles per cell. Same work here is
/// one crossing per array and a native double loop.
///
/// `solid` and `oneway` are GID lists rather than a per-cell mask because that is the shape a Tiled
/// map already has, and they are scanned linearly for the same reason the tish version did: they
/// hold a handful of entries, and a set would cost more to build than the scan saves.
pub fn grid_from_gids(args: &[Value]) -> Value {
    let width = n(args, 0) as i32;
    let height = n(args, 1) as i32;
    if width <= 0 || height <= 0 {
        return Value::Null;
    }
    let data = match args.get(2) {
        Some(v) => read_i32_array(v),
        None => return Value::Null,
    };
    let solid = args.get(3).map(read_i32_array).unwrap_or_default();
    let oneway = args.get(4).map(read_i32_array).unwrap_or_default();
    let ladder = args.get(5).map(read_i32_array).unwrap_or_default();
    with_world(|w| {
        w.grid_setup(width, height);
        for row in 0..height {
            let base = (row * width) as usize;
            for col in 0..width {
                let gid = match data.get(base + col as usize) {
                    Some(g) => *g,
                    None => continue,
                };
                // Ladder is a SEPARATE plane, not an alternative to solid/one-way: a ladder tile
                // that ends in a landing is both climbable and a one-way floor, and a ladder set
                // into a wall is climbable and solid. So mark it, then fall through to the
                // solidity rules rather than `else`-ing them out.
                if ladder.contains(&gid) {
                    w.grid_set_ladder(col, row, true);
                }
                // One-way is tested first and wins, matching the tish version's if/else — a GID in
                // both lists is a map-authoring mistake, and picking the softer rule keeps it a
                // passable platform rather than an invisible wall.
                if oneway.contains(&gid) {
                    w.grid_set_oneway(col, row, true);
                } else if solid.contains(&gid) {
                    w.grid_set_solid(col, row, true);
                }
            }
        }
    });
    Value::Null
}

/// `grid_set_oneway(col, row, on)` — mark a tile a one-way platform (block a platformer box only
/// from above). Independent of `grid_set_solid`.
pub fn grid_set_oneway(args: &[Value]) -> Value {
    with_world(|w| w.grid_set_oneway(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) != 0.0));
    Value::Null
}

/// `grid_set_ladder(col, row, on)` — mark a tile climbable (ladder, vine, rope). Independent of
/// `grid_set_solid` and `grid_set_oneway`: a ladder can be set into a wall or capped by a one-way
/// landing tile, and the three planes describe different things.
pub fn grid_set_ladder(args: &[Value]) -> Value {
    with_world(|w| w.grid_set_ladder(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) != 0.0));
    Value::Null
}

/// `tile_ladder(col, row)` — is this tile climbable? The probe a climb state machine runs to decide
/// whether Up should grab a ladder, and whether it has run off the top of one. Out-of-bounds is
/// false (unlike `tile_solid`, where out-of-bounds is a wall).
pub fn tile_ladder(args: &[Value]) -> Value {
    let col = n(args, 0) as i32;
    let row = n(args, 1) as i32;
    Value::Bool(with_world(|w| w.is_ladder(col, row)))
}

/// `attach_grid(entity, col, row)` — place an entity on the grid (tile-locked
/// movement). Sets its transform to the tile's pixel position.
pub fn attach_grid(args: &[Value]) -> Value {
    with_world(|w| w.attach_grid(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `grid_step(entity, dx, dy)` — try to walk one tile (4-directional). Faces that way;
/// steps only if idle and the target tile isn't solid.
pub fn grid_step(args: &[Value]) -> Value {
    with_world(|w| w.grid_step(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `grid_moving(entity)` — is the entity mid-step? (Gate input so one press = one tile.)
pub fn grid_moving(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Bool(with_world(|w| {
        w.slot_of(e)
            .map(|s| w.has(s, C_GRIDPOS) && w.gridpos[s].moving)
            .unwrap_or(false)
    }))
}

/// `grid_interact(entity)` — probe the tile the entity faces; if an entity there defines
/// `onInteract`, fire it (the "talk to the NPC" event). Reentrancy-safe.
pub fn grid_interact(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let call = with_world(|w| w.collect_interact(e));
    if let Some((cb, data, target, actor)) = call {
        value_call(
            &cb,
            &[
                data,
                Value::Number(target as f64),
                Value::Number(actor as f64),
            ],
        );
    }
    Value::Null
}

/// `topdown_interact(entity, reach)` — the free-movement counterpart to `grid_interact`: probe a
/// point `reach` px in front of a top-down entity (by its facing) and fire the `onInteract` of an
/// entity there (talk to an NPC, read a sign, open a chest). Reentrancy-safe. Returns 1 when
/// something ahead handled it, 0 when nothing did — so one button can talk when facing an NPC and
/// swing otherwise (the classic context action).
pub fn topdown_interact(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let reach = n(args, 1) as i32;
    let call = with_world(|w| w.collect_topdown_interact(e, reach));
    if let Some((cb, data, target, actor)) = call {
        value_call(
            &cb,
            &[
                data,
                Value::Number(target as f64),
                Value::Number(actor as f64),
            ],
        );
        return Value::Number(1.0);
    }
    Value::Number(0.0)
}

/// `platformer_interact(entity, reach)` — the side-scrolling counterpart to `topdown_interact`:
/// probe `reach` px to the side the entity is facing (`platformer_face`) over its own box height
/// plus a little slack, and fire the `onInteract` of an entity there — talk to a townsperson, read
/// a signpost, open a door. Reentrancy-safe. Returns 1 when something ahead handled it, 0 otherwise,
/// so one button can talk when facing an NPC and do something else when it isn't.
pub fn platformer_interact(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let reach = n(args, 1) as i32;
    let call = with_world(|w| w.collect_platformer_interact(e, reach));
    if let Some((cb, data, target, actor)) = call {
        value_call(
            &cb,
            &[
                data,
                Value::Number(target as f64),
                Value::Number(actor as f64),
            ],
        );
        return Value::Number(1.0);
    }
    Value::Number(0.0)
}

/// `platformer_can_interact(entity, reach)` — is there something to interact with in front of this
/// entity right now? Runs the same probe as `platformer_interact` but fires nothing.
///
/// This is what a context PROMPT needs — the little "A" that appears over your head when you are
/// standing in front of a townsperson. Without it a game has to either show the prompt always (which
/// teaches the player nothing about who will talk) or discover the answer by pressing the button
/// (which is too late to be a prompt).
pub fn platformer_can_interact(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let reach = n(args, 1) as i32;
    let found = with_world(|w| w.collect_platformer_interact(e, reach).is_some());
    Value::Number(if found { 1.0 } else { 0.0 })
}

/// `set_anim(entity, frames, speed)` — loop a sprite-sheet animation (needs a `sheet:`
/// sprite): cycle `frames` frames, one every `speed` game frames.
pub fn set_anim(args: &[Value]) -> Value {
    with_world(|w| w.set_anim(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `set_walk(entity, cols, speed)` — enable directional walk animation on a grid entity.
/// Its sprite must be a `sheet:` laid out as rows of `cols` frames (down / up / side; see
/// `Walk`). `speed` is frames per step toggle. The grid facing + movement drive it.
pub fn set_walk(args: &[Value]) -> Value {
    with_world(|w| w.set_walk(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// Read argument `i` as a boolean (accepts a bool or a nonzero number).
fn truthy(args: &[Value], i: usize) -> bool {
    match args.get(i) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(x)) => *x != 0.0,
        _ => false,
    }
}

/// `anim_play(entity, from, len, speed, loop)` — the low-level clip primitive an animation
/// controller drives: play frames `[from, from+len)` at `speed` frames-per-step, looping or
/// stopping on the last frame. Idempotent (re-issuing the active clip won't restart it).
pub fn anim_play(args: &[Value]) -> Value {
    with_world(|w| {
        w.anim_play(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            truthy(args, 4),
        )
    });
    Value::Null
}

/// `grid_facing(entity)` — the entity's facing as a direction code (0 down, 1 up, 2 left,
/// 3 right), for a controller to select a directional clip.
pub fn grid_facing(args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.grid_facing(n(args, 0) as i32)) as f64)
}

/// `grid_col(entity)` / `grid_row(entity)` — the entity's current grid tile. Used to detect when a
/// player stands on a teleporter/door tile (then `attach_grid`/`onGrid` warps them to the target).
pub fn grid_col(args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.grid_col(n(args, 0) as i32)) as f64)
}
pub fn grid_row(args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.grid_row(n(args, 0) as i32)) as f64)
}

/// `entity_sprite(entity)` — the entity's tish-agb sprite handle (or -1). For manual
/// frame control (`sprite_set_frame`), e.g. directional/facing frames.
pub fn entity_sprite(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    Value::Number(with_world(|w| w.slot_of(e).map(|s| w.sprite[s].handle).unwrap_or(-1)) as f64)
}

/// `attach_sprite(entity, spriteHandle)` — bind a tish-agb sprite (from `sprite_new`)
/// to an entity; the render system keeps it at the entity's transform.
pub fn attach_sprite(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let h = n(args, 1) as i32;
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.sprite[s] = SpriteRef {
                handle: h,
                ox: 0,
                oy: 0,
            };
            w.mask[s] |= C_SPRITE;
            w.used |= C_SPRITE;
        }
    });
    // Release the sprite's VRAM immediately: `render_system` restores it the moment the entity is on
    // screen and re-releases it when it leaves. This keeps setup from holding a VRAM sprite for every
    // entity at once (a big level's off-screen enemies/pickups would otherwise exhaust sprite VRAM
    // before the first frame renders); at most a couple are resident during spawning.
    tish_agb::native_sprite_release(h);
    Value::Null
}

/// `set_sprite_offset(entity, ox, oy)` — draw the entity's sprite `ox`/`oy` px from its transform,
/// so the sprite can be bigger than its collider. A 32×32 character on a 16×16 hitbox uses (-8, -16):
/// centred over the box, feet on the box's bottom edge. Call after `attach_sprite`.
pub fn set_sprite_offset(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let ox = n(args, 1) as i32;
    let oy = n(args, 2) as i32;
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.sprite[s].ox = ox;
            w.sprite[s].oy = oy;
        }
    });
    Value::Null
}

/// `entity_x(entity)` / `entity_y(entity)` — read back a transform coordinate (for
/// game logic / debugging). `null` for a stale/unknown entity.
pub fn entity_x(args: &[Value]) -> Value {
    with_world(|w| {
        w.slot_of(n(args, 0) as i32)
            .map(|s| Value::Number(from_fixed(w.transform[s].x)))
            .unwrap_or(Value::Null)
    })
}

/// See [`entity_x`].
pub fn entity_y(args: &[Value]) -> Value {
    with_world(|w| {
        w.slot_of(n(args, 0) as i32)
            .map(|s| Value::Number(from_fixed(w.transform[s].y)))
            .unwrap_or(Value::Null)
    })
}

// ── Typed exports (the "typed externs" perf path) ─────────────────────────────
// Each mirrors a boxed `fn(&[Value]) -> Value` export above, but takes NATIVE arguments and returns a
// native value — so a typed tish call site lowers to a DIRECT `tish_gba_game_engine::name_typed(..)`
// Rust call (no `Value` boxing, no `value_call` dispatch). The compiler emits these when a game
// `declare fun`s the matching typed signature (shipped in the module's `.d.tish`); the boxed shims
// above stay for dynamic call sites. Keep the two in sync — same effect, native vs boxed marshalling.
pub fn spawn_typed() -> i32 {
    with_world(|w| w.spawn())
}
pub fn despawn_typed(e: i32) {
    with_world(|w| w.despawn(e));
}
pub fn reset_entity_typed(e: i32) {
    with_world(|w| w.reset_entity(e));
}
pub fn set_dynamic_typed(
    e: i32,
    diameter: Fixed,
    restitution: i32,
    friction: i32,
    rest_speed: Fixed,
    rank: i32,
) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.collider[s] = Collider {
                w: diameter,
                h: diameter,
            };
            let rv = rest_speed.to_raw() >> 4;
            w.dynamic[s] = Dynamic {
                restitution: restitution.clamp(0, 256),
                friction: friction.clamp(0, 256),
                rest_v2: rv * rv,
                rank: rank.clamp(0, 255) as u8,
                asleep: 0,
                last_hit: 0,
            };
            w.mask[s] |= C_DYNAMIC | C_CIRCLE | C_COLLIDER | C_BODY | C_TRANSFORM;
            w.used |= C_DYNAMIC | C_CIRCLE | C_COLLIDER | C_BODY | C_TRANSFORM;
        }
    });
}
pub fn body_impulse_typed(e: i32, turn: i32, speed: Fixed) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            // ⚠️ THIS IS WHY `turn` IS IN 1/256ths AND NOT DEGREES. agb's sin/cos table is 256
            // entries over one turn and `Fixed`'s raw units ARE 1/256ths of a turn, so the
            // conversion is `from_raw` and NO ARITHMETIC AT ALL. `fire_angle`, which takes degrees,
            // pays a software division by 360*256 on every single call.
            let a = Fixed::from_raw(turn.rem_euclid(256));
            let (c, si) = (a.cos(), a.sin());
            w.body[s].vx += Fixed::from_raw((speed.to_raw() * c.to_raw()) >> 8);
            w.body[s].vy += Fixed::from_raw((speed.to_raw() * si.to_raw()) >> 8);
            w.dynamic[s].asleep = 0;
        }
    });
}
pub fn body_kick_typed(e: i32, from_x: Fixed, from_y: Fixed, speed: Fixed) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            let (cx, cy) = w.center_of(s);
            let dx = (cx.to_raw() - from_x.to_raw()) >> 8;
            let dy = (cy.to_raw() - from_y.to_raw()) >> 8;
            let d2 = (dx * dx + dy * dy).max(1);
            let mut len = 1i32;
            while len * len < d2 {
                len += 1;
            }
            let nx = (dx << 8) / len;
            let ny = (dy << 8) / len;
            w.body[s].vx += Fixed::from_raw((speed.to_raw() * nx) >> 8);
            w.body[s].vy += Fixed::from_raw((speed.to_raw() * ny) >> 8);
            w.dynamic[s].asleep = 0;
        }
    });
}
pub fn body_asleep_typed(e: i32) -> i32 {
    with_world(|w| {
        w.slot_of(e)
            .map(|s| w.dynamic[s].asleep as i32)
            .unwrap_or(1)
    })
}
pub fn body_speed2_typed(e: i32) -> i32 {
    with_world(|w| {
        w.slot_of(e)
            .map(|s| {
                let (vx, vy) = (w.body[s].vx.to_raw(), w.body[s].vy.to_raw());
                ((vx >> 4) * (vx >> 4)) + ((vy >> 4) * (vy >> 4))
            })
            .unwrap_or(0)
    })
}
pub fn body_last_hit_typed(e: i32) -> i32 {
    with_world(|w| w.slot_of(e).map(|s| w.dynamic[s].last_hit).unwrap_or(0))
}
pub fn grid_set_surface_typed(col: i32, row: i32, id: i32) {
    with_world(|w| w.grid_set_surface(col, row, id));
}
pub fn surface_def_typed(id: i32, ax: Fixed, ay: Fixed, friction: i32) {
    with_world(|w| {
        let i = (id.clamp(0, 15)) as usize;
        w.surf[i] = SurfaceDef {
            ax,
            ay,
            friction: friction.clamp(0, 256),
        };
    });
}
pub fn pool_new_typed(count: i32, sheet: i32, ox: i32, oy: i32) -> i32 {
    with_world(|w| w.pool_new(count, sheet, ox, oy))
}
pub fn pool_arm_typed(p: i32, slot: i32, kind: i32, ttl: i32) -> i32 {
    with_world(|w| w.pool_arm(p, slot, kind, ttl))
}
pub fn pool_retire_typed(p: i32, slot: i32) {
    with_world(|w| w.pool_retire(p, slot));
}
pub fn pool_clear_typed(p: i32) {
    with_world(|w| w.pool_clear(p));
}
pub fn pool_get_typed(p: i32, slot: i32, field: i32) -> i32 {
    with_world(|w| w.pool_get(p, slot, field))
}
pub fn pool_stat_typed(p: i32, field: i32) -> i32 {
    with_world(|w| w.pool_stat(p, field))
}
pub fn set_stun_typed(e: i32, frames: i32) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.stun[s] = frames.max(0);
        }
    });
}
pub fn is_stunned_typed(e: i32) -> i32 {
    with_world(|w| w.slot_of(e).map(|s| (w.stun[s] > 0) as i32).unwrap_or(0))
}
pub fn set_transform_typed(e: i32, x: Fixed, y: Fixed) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.transform[s] = Transform { x, y };
            w.mask[s] |= C_TRANSFORM;
            w.used |= C_TRANSFORM;
        }
    });
}
pub fn set_body_typed(e: i32, vx: Fixed, vy: Fixed) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.body[s] = Body { vx, vy };
            w.mask[s] |= C_BODY;
            w.used |= C_BODY;
        }
    });
}
pub fn set_collider_typed(e: i32, cw: Fixed, ch: Fixed) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.collider[s] = Collider { w: cw, h: ch };
            w.mask[s] |= C_COLLIDER;
            w.used |= C_COLLIDER;
        }
    });
}
pub fn set_sprite_offset_typed(e: i32, ox: i32, oy: i32) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.sprite[s].ox = ox;
            w.sprite[s].oy = oy;
        }
    });
}
pub fn attach_sprite_typed(e: i32, h: i32) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.sprite[s] = SpriteRef {
                handle: h,
                ox: 0,
                oy: 0,
            };
            w.mask[s] |= C_SPRITE;
            w.used |= C_SPRITE;
        }
    });
    tish_agb::native_sprite_release(h);
}
pub fn set_tag_typed(e: i32, t: i32) {
    with_world(|w| {
        if let Some(s) = w.slot_of(e) {
            w.tag[s] = t;
        }
    });
}
pub fn set_hurt_typed(e: i32, damage: i32, target_tag: i32, despawn_on_hit: i32, stun: i32) {
    with_world(|w| w.set_hurt(e, damage, target_tag, despawn_on_hit != 0, stun));
}
pub fn set_lure_typed(e: i32, radius: i32, frames: i32) {
    with_world(|w| w.set_lure(e, radius, frames));
}
pub fn set_shooter_typed(e: i32, interval: i32, speed: Fixed, aimed: i32) {
    with_world(|w| w.set_shooter(e, interval, speed, aimed != 0));
}
pub fn set_charger_typed(e: i32, speed: i32, band: i32) {
    with_world(|w| w.set_charger(e, speed, band));
}
/// Typed sibling of `topdown_interact`. Without this, ONE `this.interactTD(reach)` call forces a
/// whole component onto the boxed `update:` path — the interact itself is cheap, the wrapper is not.
pub fn topdown_interact_typed(e: i32, reach: i32) -> i32 {
    match topdown_interact(&[Value::Number(e as f64), Value::Number(reach as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn entity_hp_typed(e: i32) -> i32 {
    with_world(|w| match w.slot_of(e) {
        Some(s) if w.has(s, C_HEALTH) => w.health[s].hp,
        _ => 0,
    })
}

pub fn entity_hp_max_typed(e: i32) -> i32 {
    with_world(|w| match w.slot_of(e) {
        Some(s) if w.has(s, C_HEALTH) => w.health[s].max,
        _ => 0,
    })
}

pub fn entity_alive_typed(e: i32) -> i32 {
    with_world(|w| match w.slot_of(e) {
        Some(s) if w.alive[s] => 1,
        _ => 0,
    })
}
pub fn set_lifetime_typed(e: i32, ttl: i32) {
    with_world(|w| w.set_lifetime(e, ttl));
}
pub fn set_despawn_offscreen_typed(e: i32, on: i32) {
    with_world(|w| w.set_despawn_offscreen(e, on != 0));
}
pub fn set_arena_wrap_typed(on: i32) {
    with_world(|w| w.set_arena_wrap(on != 0));
}
pub fn set_mover_typed(e: i32, pattern: i32, vy: Fixed, amp: Fixed, period: i32) {
    with_world(|w| w.set_mover(e, pattern as u8, vy, amp, period));
}
pub fn set_health_typed(e: i32, max: i32, invuln: i32) {
    with_world(|w| w.set_health(e, max, invuln));
}
pub fn damage_typed(e: i32, amount: i32) {
    with_world(|w| w.damage(e, amount));
}
pub fn entity_x_typed(e: i32) -> Fixed {
    with_world(|w| {
        w.slot_of(e)
            .map(|s| w.transform[s].x)
            .unwrap_or(Fixed::from_raw(0))
    })
}
pub fn entity_y_typed(e: i32) -> Fixed {
    with_world(|w| {
        w.slot_of(e)
            .map(|s| w.transform[s].y)
            .unwrap_or(Fixed::from_raw(0))
    })
}
pub fn entity_sprite_typed(e: i32) -> i32 {
    with_world(|w| w.slot_of(e).map(|s| w.sprite[s].handle).unwrap_or(-1))
}
pub fn health_hp_typed(e: i32) -> i32 {
    with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_HEALTH))
            .map(|s| w.health[s].hp)
            .unwrap_or(0)
    })
}
pub fn health_max_typed(e: i32) -> i32 {
    with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_HEALTH))
            .map(|s| w.health[s].max)
            .unwrap_or(0)
    })
}

pub fn topdown_move_typed(e: i32, dx: i32, dy: i32) {
    with_world(|w| w.topdown_move(e, dx, dy));
}

/// Typed twin of `topdown_speed` — px/frame as i32 (hot path; avoid boxed `value_call`).
pub fn topdown_speed_typed(e: i32, px: i32) {
    with_world(|w| w.topdown_speed(e, px as f64));
}

pub fn topdown_facing_typed(e: i32) -> i32 {
    with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_TOPDOWN))
            .map(|s| w.topdown[s].facing)
            .unwrap_or(0)
    })
}

pub fn anim_play_typed(e: i32, from: i32, len: i32, speed: i32, looping: bool) {
    with_world(|w| w.anim_play(e, from, len, speed, looping));
}

/// `defineComponent(name, { start(self,e,dt), update(self,e,dt) })` — register a
/// reusable component type (the Unity-component feel). The callbacks run each frame
/// for every entity the component is attached to.
pub fn define_component(args: &[Value]) -> Value {
    let name = name_of(args, 0);
    let config = args.get(1).cloned().unwrap_or(Value::Null);
    with_world(|w| w.define_component(name, &config));
    Value::Null
}

/// `addBehaviour(entity, name, data)` — attach a defined component to an entity with
/// its own mutable `data` object. `null` name/entity is a no-op.
pub fn add_behaviour(args: &[Value]) -> Value {
    let e = n(args, 0) as i32;
    let name = name_of(args, 1);
    let data = args.get(2).cloned().unwrap_or(Value::Null);
    with_world(|w| {
        if let (Some(s), Some(def)) = (w.slot_of(e), w.def_index_by_name(&name)) {
            w.behaviour[s] = Some(BehaviourInstance {
                def,
                data,
                started: false,
            });
        }
    });
    Value::Null
}

/// `set_camera_target(entity)` — make the camera follow this entity (centred on screen,
/// clamped to the map edges). Required for maps larger than the screen; small maps skip it.
pub fn set_camera_target(args: &[Value]) -> Value {
    with_world(|w| w.set_camera_target(n(args, 0) as i32));
    Value::Null
}

#[no_mangle]
pub fn camera_transitioning(_args: &[Value]) -> Value {
    Value::Bool(with_world(|w| {
        w.room_cam.enabled && w.room_cam.transitioning
    }))
}

#[no_mangle]
pub fn set_room_camera(args: &[Value]) -> Value {
    with_world(|w| w.set_room_camera(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `world_step()` — advance the world one frame. Two phases: (1) **behaviours** —
/// collect each attached component's callback under the world borrow, then invoke
/// them WITHOUT the borrow held (so a callback may re-enter the engine — `set_body`
/// etc. — safely); (2) **systems** — movement + render (pure Rust). Then commit the
/// display via tish-agb (draw, vblank, audio, input). The engine loop's one call/frame.
fn timer_now() -> u32 {
    match tish_agb::timer_read(&[]) {
        Value::Number(n) => n as u32,
        _ => 0,
    }
}

/// Per-phase Timer2 tick breakdown of the last `world_step` (~4389 ticks = one frame): index
/// 0 = total work, 1 = behaviours+ticks, 2 = native systems (movement/combat/life), 3 = collisions +
/// deaths, 4 = health/anim/render/camera. Read with `step_ticks(i)`.
static STEP_TICKS: SingleCore<RefCell<[u32; 5]>> = SingleCore::new(RefCell::new([0; 5]));

/// `step_ticks(phase?)` — Timer2 ticks the previous `world_step` spent (excluding the vblank wait).
/// `phase`: 0/omitted = total, 1 = behaviours+ticks, 2 = native systems, 3 = collisions+deaths,
/// 4 = render group. ~4389 ticks = one 59.7fps frame; over budget = slowdown.
pub fn step_ticks(args: &[Value]) -> Value {
    let i = (n(args, 0) as usize).min(4);
    Value::Number(STEP_TICKS.with(|c| c.borrow()[i]) as f64)
}

/// Worst-case ticks for each `world_step` phase since the last [`step_peak_reset`] — same indices as
/// [`step_ticks`]. Lets one screenshot capture the true per-phase spike (e.g. a boss ring frame).
static STEP_PEAK: SingleCore<RefCell<[u32; 5]>> = SingleCore::new(RefCell::new([0; 5]));

/// `[last_stamp, initialised, peak_period, last_period, ema_period]` — full-frame wall-clock tracker.
static FRAME_PERIOD: SingleCore<RefCell<[u32; 5]>> = SingleCore::new(RefCell::new([0; 5]));

/// `frame_period(mode?)` — the whole frame's Timer2 ticks (game + HUD + main loop + vblank), which
/// `step_ticks` can't see. `mode`: 0/omitted = last frame, 1 = peak since [`step_peak_reset`],
/// 2 = EMA (the stable "typical frame" — use this for sustained cost, the peak is dominated by the
/// one-time boot frame and the last frame is noisy). ~4389 = a clean 60fps frame; a sustained ~8778
/// means the game is holding 30fps.
pub fn frame_period(args: &[Value]) -> Value {
    let mode = n(args, 0) as i32;
    Value::Number(FRAME_PERIOD.with(|c| {
        let p = c.borrow();
        match mode {
            1 => p[2],
            2 => p[4],
            _ => p[3],
        }
    }) as f64)
}

/// `step_peak(phase?)` — the highest `step_ticks(phase)` seen since the last `step_peak_reset`.
pub fn step_peak(args: &[Value]) -> Value {
    let i = (n(args, 0) as usize).min(4);
    Value::Number(STEP_PEAK.with(|c| c.borrow()[i]) as f64)
}

/// `step_peak_reset()` — clear the per-phase worst-case tracker (and the frame-period peak).
pub fn step_peak_reset(_args: &[Value]) -> Value {
    STEP_PEAK.with(|c| *c.borrow_mut() = [0; 5]);
    FRAME_PERIOD.with(|c| {
        let mut p = c.borrow_mut();
        p[1] = 0; // re-arm (next frame re-stamps without counting the gap)
        p[2] = 0; // peak
        p[4] = 0; // EMA re-seeds on the next sample
    });
    Value::Null
}

/// `entity_count()` — how many entities are currently ALIVE (for leak/accumulation diagnostics: a
/// value that climbs and never settles means something isn't despawning).
pub fn entity_count(_args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.alive.iter().filter(|&&a| a).count()) as f64)
}

/// `stun_all(tag, frames)` — freeze every live entity carrying `tag` for `frames`.
///
/// Stun already existed, but only as something a hurt box did to whatever it touched. A screen-wide
/// freeze is a different shape and a common one: the classic action-RPG clock item stops every monster in the room the
/// moment it is picked up, and the same primitive covers a time-stop item, a boss's opening
/// cutscene, or holding the world still while a menu animates in.
///
/// Returns how many entities it froze, so a caller can tell "nothing was there" from "it worked".
pub fn stun_all(args: &[Value]) -> Value {
    Value::Number(stun_all_typed(n(args, 0) as i32, n(args, 1) as i32) as f64)
}

/// Typed twin of `stun_all` — see there.
pub fn stun_all_typed(tag: i32, frames: i32) -> i32 {
    with_world(|w| {
        let mut hit = 0;
        for s in 0..w.alive.len() {
            if w.alive[s] && w.tag.get(s).copied().unwrap_or(0) == tag {
                w.stun[s] = w.stun[s].max(frames);
                hit += 1;
            }
        }
        hit
    })
}

/// `entity_count_tag(tag)` — how many live entities carry `tag`. Counting a tag needed a Tish loop
/// over every slot before, which is why "is this room cleared yet?" was never actually asked: a
/// dungeon shutter has to notice the last enemy dying on the frame it happens.
/// `nearest_tag(entity, tag, radius)` — the nearest OTHER entity carrying `tag` within `radius`
/// px (manhattan, box centers), or -1. Not gated on the off-screen cull: an ecosystem's
/// creatures keep mattering off screen. O(entities) — stagger calls, don't ask every frame.
pub fn nearest_tag(args: &[Value]) -> Value {
    let r = with_world(|w| w.nearest_tag(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Number(r as f64)
}

/// See [`nearest_tag`].
pub fn nearest_tag_typed(e: i32, tag: i32, radius: i32) -> i32 {
    with_world(|w| w.nearest_tag(e, tag, radius))
}

/// `entity_dist(a, b)` — manhattan distance between two entities' box centers in whole px, or -1
/// if either id no longer resolves.
pub fn entity_dist(args: &[Value]) -> Value {
    let r = with_world(|w| w.entity_dist(n(args, 0) as i32, n(args, 1) as i32));
    Value::Number(r as f64)
}

/// See [`entity_dist`].
pub fn entity_dist_typed(a: i32, b: i32) -> i32 {
    with_world(|w| w.entity_dist(a, b))
}

pub fn entity_count_tag(args: &[Value]) -> Value {
    let want = match args.first() {
        Some(Value::Number(n)) => *n as i32,
        _ => 0,
    };
    Value::Number(with_world(|w| {
        let mut n = 0;
        for (i, &a) in w.alive.iter().enumerate() {
            if a && w.tag.get(i).copied().unwrap_or(0) == want {
                n += 1;
            }
        }
        n
    }) as f64)
}

/// `entity_slots()` — the SoA store's slot high-water mark (`alive.len()`), which only grows. The
/// per-frame system loops iterate this many slots, so a big gap between it and `entity_count` means a
/// past burst is still taxing every frame (the O(n) / O(n²) passes walk dead slots too).
pub fn entity_slots(_args: &[Value]) -> Value {
    Value::Number(with_world(|w| w.alive.len()) as f64)
}

// ── S4: which SYSTEM ate the frame ──────────────────────────────────────────
// `step_ticks` attributes the frame to a PHASE; this attributes phases 2 and 4 to a SYSTEM. Off by
// default — profiling costs two timer reads per system per frame — `sys_prof(1)` turns it on for a
// profiling run. Indices are the CALL ORDER in `world_step`; read names with `sys_name(i)` so a log
// reader never hard-codes this table.
//
// WHY THIS EXISTS: "most games lag at a certain point" and the only tool was a five-phase split.
// The lag mechanism is ~30 systems each scanning every slot per frame (~60 ticks per live entity,
// and an EMPTY reserved slot still bills ~24 across the pass) — but WHICH systems carry that cost
// in a given game was a bisect. Now it is a log line.
const SYS_N: usize = 32;
static SYS_NAMES: [&str; SYS_N] = [
    "patrol",
    "mover",
    "boomerang",
    "hopper",
    "jumper",
    "movement",
    "grid",
    "platformer",
    "chase",
    "soldier",
    "seek",
    "shooter",
    "charger",
    "trap",
    "wanderer",
    "nai",
    "topdown",
    "dynamic",
    "wrap",
    "follow",
    "diranim",
    "grabber",
    "combat",
    "life",
    "health",
    "anim",
    "walk",
    "room_free",
    "room_trans",
    "fog",
    "render",
    "camera",
];
static SYS_PROF: SingleCore<RefCell<bool>> = SingleCore::new(RefCell::new(false));
static SYS_TICKS: SingleCore<RefCell<[u32; SYS_N]>> = SingleCore::new(RefCell::new([0; SYS_N]));

/// Run one system, attributing its ticks when profiling is on. A fn POINTER, not a closure, so the
/// call is equally direct either way and the `prof` branch is the only overhead when off.
fn ts(w: &mut World, prof: bool, i: usize, need: u32, need2: u32, f: fn(&mut World)) {
    // S1: a system whose defining component has never been attached this scene has nothing to scan
    // for — measured at ~1.5 ticks per slot per system of pure mask-checking otherwise, which is
    // where "most games lag at a certain point" lived. `need == need2 == 0` means "always runs".
    if (need != 0 || need2 != 0) && (w.used & need) == 0 && (w.used2 & need2) == 0 {
        return;
    }
    if !prof {
        f(w);
        return;
    }
    let t0 = timer_now();
    f(w);
    let dt = timer_now().wrapping_sub(t0) & 0xFFFF;
    SYS_TICKS.with(|c| c.borrow_mut()[i] += dt);
}

/// `sys_prof(on)` — enable per-system tick attribution (see `sys_ticks`).
pub fn sys_prof(args: &[Value]) -> Value {
    SYS_PROF.with(|c| *c.borrow_mut() = n(args, 0) as i32 != 0);
    Value::Null
}
/// See [`sys_prof`].
pub fn sys_prof_typed(on: i32) {
    SYS_PROF.with(|c| *c.borrow_mut() = on != 0);
}

/// `sys_ticks(i)` — Timer2 ticks system `i` spent LAST frame; 0 unless `sys_prof(1)` is on.
pub fn sys_ticks(args: &[Value]) -> Value {
    let i = (n(args, 0) as usize).min(SYS_N - 1);
    Value::Number(SYS_TICKS.with(|c| c.borrow()[i]) as f64)
}
/// See [`sys_ticks`].
pub fn sys_ticks_typed(i: i32) -> i32 {
    SYS_TICKS.with(|c| c.borrow()[(i as usize).min(SYS_N - 1)]) as i32
}

/// `sys_count()` — how many systems `sys_ticks` indexes.
pub fn sys_count(_args: &[Value]) -> Value {
    Value::Number(SYS_N as f64)
}
/// See [`sys_count`].
pub fn sys_count_typed() -> i32 {
    SYS_N as i32
}

/// `sys_name(i)` — the system's name, for logs. Boxed-only on purpose: it is read a handful of
/// times per profiling run, never on a hot path.
pub fn sys_name(args: &[Value]) -> Value {
    let i = (n(args, 0) as usize).min(SYS_N - 1);
    Value::String(SYS_NAMES[i].into())
}

pub fn world_step(_args: &[Value]) -> Value {
    // Full-frame period: world_step runs once per frame and ends with the vblank wait, so the gap
    // between consecutive starts is the WHOLE frame's wall-clock (game logic + HUD + the main loop +
    // vblank). ~4389 Timer2 ticks = a 60fps frame; a sustained higher value = dropped frames. This is
    // the only figure that captures cost OUTSIDE world_step (e.g. the tish HUD draw).
    {
        let now = timer_now();
        FRAME_PERIOD.with(|c| {
            let mut p = c.borrow_mut();
            let period = now.wrapping_sub(p[0]) & 0xFFFF;
            p[0] = now;
            if p[1] != 0 {
                p[3] = period; // instantaneous last-frame period
                if period > p[2] {
                    p[2] = period; // peak (skip the very first, which has no valid previous stamp)
                }
                // EMA (÷16) — a stable "typical frame" read that a single noisy screenshot can trust.
                // Seed on the first real sample so it converges immediately rather than ramping from 0.
                p[4] = if p[4] == 0 {
                    period
                } else {
                    (p[4] as i32 + (period as i32 - p[4] as i32) / 16) as u32
                };
            }
            p[1] = 1;
        });
    }
    let work_start = timer_now();
    // Phase 1 — behaviour update (reentrant): collect, drop borrow, then call.
    let num_updates = with_world(|w| {
        w.collect_behaviours();
        w.buf_updates.len()
    });
    for i in 0..num_updates {
        let (callback, data, entity) = with_world(|w| w.buf_updates[i].clone());
        value_call(
            &callback,
            &[
                data,
                Value::Number(entity as f64),
                Value::Number(1.0),
                Value::native(set_hopper),
                Value::native(set_jumper),
            ],
        );
    }
    // Phase 1b — fast `tick` hooks: fill each entity's data ctx with state, call the hook (ONE
    // tish call, no per-op ABI trip), read decisions back, apply. Reentrancy-safe (no borrow held
    // during the callback), same as the behaviour dispatch.
    let num_ticks = with_world(|w| {
        w.collect_ticks();
        w.buf_ticks.len()
    });
    if num_ticks > 0 {
        with_world(|w| w.buf_results.clear());
        for i in 0..num_ticks {
            let job = with_world(|w| w.buf_ticks[i].clone());
            // Lean tick: call with just the entity id — no boxed ctx, no marshalling, no readback.
            if job.lean {
                value_call(&job.cb, &[Value::Number(job.entity as f64)]);
                continue;
            }
            set_prop(&job.data, "e", Value::Number(job.entity as f64));
            set_prop(&job.data, "x", Value::Number(job.x as f64));
            set_prop(&job.data, "y", Value::Number(job.y as f64));
            // Platformer entities get the 9 gravity/jump props; free-movers skip them entirely (each
            // set_prop/prop_num is a boxed hashmap op — a real cost at bullet-hell tick counts).
            if job.platformer {
                set_prop(&job.data, "grounded", Value::Bool(job.grounded));
                set_prop(&job.data, "blocked", Value::Bool(job.blocked));
                set_prop(&job.data, "move", Value::Number(0.0));
                set_prop(&job.data, "jump", Value::Bool(false));
                set_prop(&job.data, "jumpCut", Value::Bool(false));
                set_prop(&job.data, "run", Value::Bool(false));
                set_prop(&job.data, "drop", Value::Bool(false));
                set_prop(&job.data, "flip", Value::Bool(false));
                set_prop(&job.data, "bounce", Value::Number(0.0));
            }
            // Free-movement velocity: seed with the current heading so a hook that leaves vx/vy alone
            // keeps flying straight (and one that stops sets them to 0).
            if job.body {
                set_prop(&job.data, "vx", Value::Number(job.vx));
                set_prop(&job.data, "vy", Value::Number(job.vy));
            }
            // Pass the ctx by reference (the hook mutates it in place and we read outputs back after)
            // — a `job.data.clone()` here is a DEEP copy of the whole object every tick every frame,
            // which dominated the frame at any real enemy count.
            value_call(&job.cb, core::slice::from_ref(&job.data));
            let mut out = TickOut::default();
            if job.platformer {
                out.move_dir = prop_num(&job.data, "move") as i32;
                out.jump = prop_truthy(&job.data, "jump");
                out.jump_cut = prop_truthy(&job.data, "jumpCut");
                out.run = prop_truthy(&job.data, "run");
                out.drop = prop_truthy(&job.data, "drop");
                out.flip = prop_truthy(&job.data, "flip");
                out.bounce = prop_num(&job.data, "bounce") as i32;
            }
            if job.body {
                out.vx = prop_num(&job.data, "vx");
                out.vy = prop_num(&job.data, "vy");
            }
            with_world(|w| w.buf_results.push((job.entity, out)));
        }
        with_world(|w| {
            for j in 0..w.buf_results.len() {
                let (e, out) = {
                    let tuple = &w.buf_results[j];
                    (tuple.0, tuple.1)
                };
                w.apply_tick(e, &out);
            }
        });
    }
    let t_p1 = timer_now();
    // S4: read the profiling flag once and zero the attribution for this frame's system work.
    let prof = SYS_PROF.with(|c| *c.borrow());
    if prof {
        SYS_TICKS.with(|c| *c.borrow_mut() = [0; SYS_N]);
    }
    // Phase 2 — native AI + movement + grid stepping + platformer physics (pure Rust).
    with_world(|w| {
        ts(w, prof, 0, C_PATROL, 0, World::patrol_system); // sets move intent for patrol enemies before the platformer integrates it
        ts(w, prof, 1, C_MOVER, 0, World::mover_system); // native shmup movement patterns → Body velocity, before integration
                                                         // Boomerang return-mover: after N frames reverse Body velocity toward the owner. Runs after
                                                         // `mover_system` (so a weave pattern doesn't overwrite the return heading) and before
                                                         // `movement_system` integrates. Pair with `set_lifetime` / `set_despawn_offscreen`.
        ts(w, prof, 2, C_BOOMERANG, 0, World::boomerang_system);
        ts(w, prof, 4, C_JUMPER, 0, World::jumper_system);
        ts(w, prof, 5, C_BODY, 0, World::movement_system);
        ts(w, prof, 6, C_GRIDPOS, 0, World::grid_system);
        ts(
            w,
            prof,
            7,
            C_PLATFORMER,
            M2_CARRIER,
            World::platformer_system,
        );
        ts(w, prof, 8, C_CHASE, 0, World::chase_system); // native top-down enemy AI (sets intent + anim) — no per-frame tish tick
                                                         // ⚠️ HOPPER RUNS AFTER CHASE, OUT OF NUMERIC ORDER, ON PURPOSE. `chase_system`'s idle path
                                                         // writes dx=dy=0 every frame — that zero is what stops a chaser when the player leaves its
                                                         // aggro band — so any intent written before it is erased. When hopper ran at slot 3, a
                                                         // creature armed with BOTH (every pooled creature in the reporting game carried a chase) never hopped at
                                                         // all: measured frozen at its spawn for an entire OAM capture, and un-frozen the moment the
                                                         // chase was dropped. The pipeline's rule is already "a lined-up charger overrides whatever
                                                         // intent the above set" — the hop is the same kind of specialist and wins the same way.
                                                         // The index stays 3: it is this system's attribution ID, not its position.
        ts(w, prof, 3, C_HOPPER, 0, World::hopper_system);
        // RTS pair, in this order: `soldier_system` decides whether a unit is fighting, then
        // `seek_system` walks everyone who is not. Reversing them would spend a frame's movement on
        // a unit that is already in combat, which reads as enemies sliding while they swing.
        ts(w, prof, 9, C_SOLDIER, 0, World::soldier_system);
        ts(w, prof, 10, C_SEEK, 0, World::seek_system);
        ts(w, prof, 11, C_SHOOTER, 0, World::shooter_system); // ranged enemies fire on their own timer
        ts(w, prof, 12, C_CHARGER, 0, World::charger_system); // …and a lined-up charger overrides whatever intent the above set
        ts(w, prof, 13, C_TRAP, 0, World::trap_system); // blade traps: inert until lined up, then dash (after charger so both compose)
        ts(w, prof, 14, 0, M2_WANDERER, World::wanderer_system); // continuous tile-aligned walkers → intent, before integration
        ts(w, prof, 15, 0, M2_NAI, World::nai_system); // NES-era AI (ambusher/drifter/flicker-caster/bouncer) → intent, before integration
        ts(w, prof, 16, C_TOPDOWN, 0, World::topdown_system); // free 8-dir action-RPG movement + tile collision (top-down genre)
                                                              // Rigid discs (golf, soccer): surfaces, tile bounce, disc-vs-disc, sleep. The position is
                                                              // FORCED by the two comments around it — the agents must have moved already, so the ball
                                                              // reacts to where they are, and the goal trigger in `combat_system` must see the resolved
                                                              // position rather than the pre-contact one.
        ts(w, prof, 17, C_DYNAMIC, 0, World::dynamic_system);
        // Toroidal arena (Asteroids), after everything that writes a position and BEFORE combat +
        // lifetimes — so a shot that crosses an edge is re-entering rather than "off-screen", and
        // the collision below sees where things actually are.
        ts(w, prof, 18, 0, 0, World::wrap_system);
        // Parent-linked parts / train segments / orbiters: re-snap after parents have moved.
        ts(w, prof, 19, C_FOLLOW, 0, World::follow_system);
        ts(w, prof, 20, C_DIRANIM, 0, World::diranim_system); // facing -> frame, after every system that can turn an entity
                                                              // Shoot-'em-up core: resolve contact damage (bullets → enemies, hazards → player), then
                                                              // retire spent bullets / off-screen entities — before collision + death dispatch below, so a
                                                              // bullet's kill flows through the normal `onDeath` and a spent shot neither hits twice nor
                                                              // renders. Cheap no-ops for a game that uses neither component.
        ts(w, prof, 21, C_GRABBER, 0, World::grabber_system); // stun-on-overlap before combat so a grab lands this frame
        ts(w, prof, 22, C_HURT, 0, World::combat_system);
        ts(w, prof, 23, C_LIFE, 0, World::life_system);
    });
    let t_p2 = timer_now();
    // Phase 3 — collision: detect overlaps, then dispatch onCollide (reentrant).
    let collisions = with_world(|w| w.collect_collisions());
    for (callback, data, me, other) in collisions {
        value_call(
            &callback,
            &[data, Value::Number(me as f64), Value::Number(other as f64)],
        );
    }
    // Phase 3b — deaths: dispatch each dead entity's onDeath (reentrant — may respawn / load a
    // scene), or despawn it if it has no onDeath hook.
    let deaths = with_world(|w| w.collect_deaths());
    for (callback, data, entity) in deaths {
        if matches!(callback, Value::Null) {
            // ⚠️ A POOLED ENTITY'S DEATH IS THE POOL OWNER'S TO NOTICE — a no-op here, not a retire
            // and not a despawn. Despawning would free a pooled sprite the pool still holds a handle
            // to; auto-retiring would look tidier and break the caller, because `castReapOne` reads
            // the corpse's `entity_x`/`entity_y` on a LATER frame to place the drop, and a retire
            // clears the transform first. Doing nothing is exactly today's semantics, so no game
            // changes when the pool arrives.
            let pooled = with_world(|w| {
                w.slot_of(entity)
                    .map(|s| w.pool_of[s] >= 0)
                    .unwrap_or(false)
            });
            if !pooled {
                with_world(|w| w.despawn(entity));
            }
        } else {
            value_call(&callback, &[data, Value::Number(entity as f64)]);
        }
    }
    let t_p3 = timer_now();
    // Phase 4 — i-frames + animation + room-slide + render + camera (pure Rust).
    with_world(|w| {
        ts(w, prof, 24, C_HEALTH, 0, World::health_system);
        ts(w, prof, 25, C_ANIM, 0, World::anim_system);
        ts(w, prof, 26, C_WALK, 0, World::walk_system);
        ts(w, prof, 27, 0, 0, World::room_track_free); // side-scroller: start a room slide when the player crosses a boundary
        ts(w, prof, 28, 0, 0, World::room_transition_system); // slide the player mid-transition before render reads its pos
                                                              // Fog last among the simulation systems: it must see where everything ENDED UP this frame,
                                                              // or the shroud trails a moving army by one frame. The blit itself is the game's call.
        ts(w, prof, 29, 0, 0, World::fog_system);
        ts(w, prof, 30, C_SPRITE, 0, World::render_system);
        ts(w, prof, 31, 0, 0, World::update_camera);
    });
    let work_end = timer_now();
    let d = |a: u32, b: u32| b.wrapping_sub(a) & 0xFFFF;
    let now = [
        d(work_start, work_end),
        d(work_start, t_p1),
        d(t_p1, t_p2),
        d(t_p2, t_p3),
        d(t_p3, work_end),
    ];
    STEP_TICKS.with(|c| *c.borrow_mut() = now);
    // Running worst-case per phase since the last reset — a single HUD read then shows the true spike
    // (a bullet-hell ring frame) regardless of which frame the screenshot lands on.
    STEP_PEAK.with(|c| {
        let mut p = c.borrow_mut();
        for i in 0..5 {
            if now[i] > p[i] {
                p[i] = now[i];
            }
        }
    });
    tish_agb::frame(&[]);
    Value::Null
}

// ═══════════════════════════════════════════════════════════════════════════════
// Iso grid — a reusable isometric, height-mapped board with SRPG-style movement.
// Standalone global state (independent of the SoA `World`): a game renders however it likes and uses
// these for the logical grid, move-range flood fill, and pathfinding. See the SRPG plan doc in the chuggie-tactics repo.
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
struct IsoBoardCell {
    height: u8, // elevation in steps (raises the tile + whatever stands on it)
    tile: u8,   // terrain/type id (rendering + move-cost hooks)
    walkable: bool,
    occupant: i32, // unit id standing here, or -1
}

/// The reference SRPG's turn-counter costs. A unit is charged 500 simply for having had a turn, another 300 if it
/// moved and another 200 if it acted — so a unit that does everything pays the full 1000 and starts
/// its next wait from zero, while one that stands still pays 500 and is already halfway there.
///
/// That difference is the whole reason to model it: Wait stops being "the button you press when you
/// have nothing to do" and becomes a tempo play, because holding position buys the unit its next
/// turn twice as soon.
const COST_TURN: i32 = 500;
const COST_MOVE: i32 = 300;
const COST_ACTION: i32 = 200;

/// How much counter a unit may carry PAST the threshold while waiting for slower units to catch up.
/// Without a cap, a fast unit parked behind a queue of slow ones banks arbitrarily much and then
/// takes several turns in a row the moment it is called; the reference SRPG caps the overflow, which is why a unit
/// never needs more than 1500 to be guaranteed its turn.
const RESERVE_MAX: i32 = 500;

/// A unit's logical state (position/team/stats/turn-counter). The game owns the sprite; the engine
/// owns this. The reference SRPG's turn order is speed-based: `ct` accrues by `speed`, and the first to reach the
/// threshold acts.
#[derive(Clone, Copy)]
struct IsoBoardUnit {
    col: i32,
    row: i32,
    team: u8,
    speed: u16,
    mov: u8,
    jump: u8,
    hp: i16,
    max_hp: i16,
    ct: i32,
    alive: bool,
    /// A flier ignores terrain cost and height deltas — it still cannot enter a solid cell or one
    /// another unit occupies. This is a movement *type*, not a bonus: it does not extend the step
    /// budget, it changes which tiles that budget can spend itself on.
    flying: bool,
    /// Percent applied to `speed` when the turn counter ticks; 100 is normal. This is how Haste and
    /// Slow are expressed — the reference SRPG's own rule is that Haste adds double Speed per tick and Slow adds
    /// half, rounding down, which is 200 and 50 through integer division.
    ///
    /// It scales the ACCRUAL rather than the stat, so nothing that reads `speed` (the AI's threat
    /// tables, the Status screen) has to know the unit is hasted, and the effect lasts exactly as
    /// long as the game leaves the scale set.
    speed_scale: u16,
}

struct IsoBoardGrid {
    pub w: i32,
    pub h: i32,
    cells: Vec<IsoBoardCell>,
    units: Vec<IsoBoardUnit>,
    in_move: Vec<bool>,     // reachable mask from the last move_range
    parent: Vec<i32>,       // BFS parent cell index (-1) for path reconstruction
    reach: Vec<(i16, i16)>, // reachable (col,row) list, in BFS order
    start: (i32, i32),      // origin of the last move_range (for isob_path)
    path: Vec<(i16, i16)>,  // last reconstructed path (start..=target)
    /// Move points to ENTER a cell, indexed by its terrain id. The engine owns the search; the game
    /// owns what its terrain means, so this is filled from the example via `isob_set_terrain_cost`
    /// and defaults to 1 (flat cost) for every id — a game that never calls it gets the old
    /// uniform-cost behaviour exactly.
    cost: [u8; 256],
    /// Reused across calls so a move-range query allocates nothing. The AI asks for a range once per
    /// unit per turn, but this also runs inside the player's cursor loop.
    dist: Vec<i32>,
    /// Cells adjacent to a living enemy of whoever is currently being pathed. Rebuilt per query (it
    /// depends on the mover's team) into a buffer that is allocated once.
    zoc: Vec<bool>,
    /// Whether zone-of-control is in force at all. Off by default, so a game that never opts in keeps
    /// the movement rules it already had.
    zoc_on: bool,
}

impl IsoBoardGrid {
    const fn new() -> Self {
        IsoBoardGrid {
            w: 0,
            h: 0,
            cells: Vec::new(),
            units: Vec::new(),
            in_move: Vec::new(),
            parent: Vec::new(),
            reach: Vec::new(),
            start: (0, 0),
            path: Vec::new(),
            cost: [1u8; 256],
            dist: Vec::new(),
            zoc: Vec::new(),
            zoc_on: false,
        }
    }
    fn idx(&self, c: i32, r: i32) -> Option<usize> {
        if c >= 0 && r >= 0 && c < self.w && r < self.h {
            Some((r * self.w + c) as usize)
        } else {
            None
        }
    }
    fn init(&mut self, w: i32, h: i32) {
        self.w = w.max(0);
        self.h = h.max(0);
        let n = (self.w * self.h) as usize;
        self.cells =
            alloc::vec![IsoBoardCell { height: 0, tile: 0, walkable: true, occupant: -1 }; n];
        self.in_move = alloc::vec![false; n];
        self.parent = alloc::vec![-1i32; n];
        self.dist = alloc::vec![i32::MAX; n];
        self.zoc = alloc::vec![false; n];
        self.reach.clear();
        self.path.clear();
        self.units.clear();
    }

    fn add_unit(
        &mut self,
        col: i32,
        row: i32,
        team: u8,
        speed: u16,
        mov: u8,
        jump: u8,
        hp: i16,
    ) -> i32 {
        let id = self.units.len() as i32;
        self.units.push(IsoBoardUnit {
            col,
            row,
            team,
            speed,
            mov,
            jump,
            hp,
            max_hp: hp,
            ct: 0,
            alive: true,
            flying: false,
            speed_scale: 100,
        });
        if let Some(i) = self.idx(col, row) {
            self.cells[i].occupant = id;
        }
        id
    }

    /// Move a unit to (c,r): clears its old cell's occupant and claims the new one.
    fn unit_set_pos(&mut self, id: i32, c: i32, r: i32) {
        let (oc, or) = match self.units.get(id as usize) {
            Some(u) => (u.col, u.row),
            None => return,
        };
        if let Some(i) = self.idx(oc, or) {
            if self.cells[i].occupant == id {
                self.cells[i].occupant = -1;
            }
        }
        if let Some(i) = self.idx(c, r) {
            self.cells[i].occupant = id;
        }
        if let Some(u) = self.units.get_mut(id as usize) {
            u.col = c;
            u.row = r;
        }
    }

    /// What a unit actually adds to its turn counter per tick, after Haste or Slow.
    ///
    /// Floored at 1, which matters: a slow caster's Speed can be low enough that halving it rounds to
    /// zero, and a unit accruing zero never reaches the threshold — it would not be slowed, it would
    /// be removed from the game until the status wore off, and `turn_next` would spin looking for a
    /// tick count that lets it act.
    fn tick_speed(u: &IsoBoardUnit) -> i32 {
        (((u.speed as i32) * (u.speed_scale as i32)) / 100).max(1)
    }

    /// Advance the speed-based turn counters until an alive unit reaches the threshold; returns that
    /// unit's id (the highest counter breaks ties), or -1 if no unit is alive.
    fn turn_next(&mut self) -> i32 {
        const THRESH: i32 = 1000;
        let mut best_t = i32::MAX;
        for u in &self.units {
            if !u.alive {
                continue;
            }
            let sp = Self::tick_speed(u);
            let t = (THRESH - u.ct + sp - 1) / sp; // ceil ticks to reach threshold
            if t < best_t {
                best_t = t;
            }
        }
        if best_t == i32::MAX {
            return -1;
        }
        for i in 0..self.units.len() {
            if self.units[i].alive {
                let sp = Self::tick_speed(&self.units[i]);
                self.units[i].ct = (self.units[i].ct + best_t * sp).min(THRESH + RESERVE_MAX);
            }
        }
        let mut who = -1i32;
        let mut hi = i32::MIN;
        for (i, u) in self.units.iter().enumerate() {
            if u.alive && u.ct >= THRESH && u.ct > hi {
                hi = u.ct;
                who = i as i32;
            }
        }
        if who >= 0 {
            // Only the BASE cost of having had a turn comes off here. What the unit does with the
            // turn costs more, and it is not known yet — `turn_end` charges the rest.
            self.units[who as usize].ct -= COST_TURN;
        }
        who
    }

    /// Weighted flood fill: mark every cell reachable from (sc,sr) for `budget` move points, where
    /// entering a cell costs `self.cost[tile]` and each step's height delta must be ≤ `jump`. Blocked
    /// by unwalkable tiles and by other units' occupied cells (the start cell is always allowed).
    /// A `flying` mover pays 1 for every cell and ignores height entirely. Records a parent tree so
    /// `path_to` can walk back.
    ///
    /// This is Dial's algorithm rather than a queue-based Dijkstra: costs are small positive integers
    /// and the budget is a single-digit stat, so sweeping the board once per cost level is both
    /// simpler to read and allocation-free, where a priority queue would want a heap on a machine
    /// that has 256 KB of it. The board is tens of cells, so the O(budget × n × 4) is nothing.
    ///
    /// Correctness depends on every cost being ≥ 1, which `set_terrain_cost` enforces: a zero-cost
    /// tile would let a later sweep improve an already-settled cell and the parent tree would then
    /// disagree with the distances.
    ///
    /// `team` is the mover's, used only for zone of control; pass -1 to path with no ZoC at all.
    /// When ZoC is on, stepping into a tile adjacent to a living enemy ENDS the move: the tile is
    /// reachable, but nothing is reachable through it. The tile the unit starts on is exempt, or a
    /// unit that began its turn next to an enemy could never move again — the rule is meant to stop
    /// people running *past* a defender, not to freeze whoever is already in contact.
    fn move_range(&mut self, sc: i32, sr: i32, budget: i32, jump: i32, flying: bool, team: i32) {
        let n = self.cells.len();
        for i in 0..n {
            self.in_move[i] = false;
            self.parent[i] = -1;
            self.dist[i] = i32::MAX;
            self.zoc[i] = false;
        }
        if self.zoc_on && team >= 0 {
            for u in 0..self.units.len() {
                let e = self.units[u];
                if !e.alive || e.team as i32 == team {
                    continue;
                }
                const AROUND: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                for (dc, dr) in AROUND {
                    if let Some(i) = self.idx(e.col + dc, e.row + dr) {
                        self.zoc[i] = true;
                    }
                }
            }
        }
        self.reach.clear();
        self.start = (sc, sr);
        let start = match self.idx(sc, sr) {
            Some(i) => i,
            None => return,
        };
        self.dist[start] = 0;
        self.in_move[start] = true;
        self.reach.push((sc as i16, sr as i16));
        // One sweep per cost level, in increasing order. `reach` therefore comes out ordered by
        // distance, which is what the callers that read it in order expect.
        for d in 0..budget {
            for cur in 0..n {
                if self.dist[cur] != d {
                    continue;
                }
                if cur != start && self.zoc[cur] {
                    continue; // reached, but the enemy's reach ends the move here
                }
                let cc = (cur as i32) % self.w;
                let cr = (cur as i32) / self.w;
                let ch = self.cells[cur].height as i32;
                const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                for (dc, dr) in DIRS {
                    let (nc, nr) = (cc + dc, cr + dr);
                    let ni = match self.idx(nc, nr) {
                        Some(i) => i,
                        None => continue,
                    };
                    let cell = self.cells[ni];
                    if !cell.walkable || cell.occupant != -1 {
                        continue; // blocked terrain or another unit stands there
                    }
                    if !flying && (cell.height as i32 - ch).abs() > jump {
                        continue; // too steep to climb/drop
                    }
                    let step = if flying {
                        1
                    } else {
                        self.cost[cell.tile as usize] as i32
                    };
                    let nd = d + step;
                    if nd > budget || nd >= self.dist[ni] {
                        continue;
                    }
                    if self.dist[ni] == i32::MAX {
                        self.reach.push((nc as i16, nr as i16));
                    }
                    self.dist[ni] = nd;
                    self.parent[ni] = cur as i32;
                    self.in_move[ni] = true;
                }
            }
        }
    }

    /// Where a shove from (sc,sr) would put unit `id`, and how far it would fall getting there —
    /// `None` if nothing would move. The single home of the knockback rule, so that resolving a shove
    /// and predicting one cannot disagree.
    fn knock_dest(&self, id: i32, sc: i32, sr: i32) -> Option<(i32, i32, i32)> {
        let u = match self.units.get(id as usize) {
            Some(u) if u.alive => *u,
            _ => return None,
        };
        let (dc, dr) = (u.col - sc, u.row - sr);
        if dc == 0 && dr == 0 {
            return None; // no direction to push along
        }
        // Cardinal push along whichever axis the victim lies furthest down, so a diagonal blow still
        // resolves to one of the four directions the board actually has.
        let (sx, sy) = if dc.abs() >= dr.abs() {
            (if dc >= 0 { 1 } else { -1 }, 0)
        } else {
            (0, if dr >= 0 { 1 } else { -1 })
        };
        let (nc, nr) = (u.col + sx, u.row + sy);
        let ni = self.idx(nc, nr)?;
        if !self.cells[ni].walkable || self.cells[ni].occupant != -1 {
            return None;
        }
        let here = self.idx(u.col, u.row)?;
        Some((
            nc,
            nr,
            self.cells[here].height as i32 - self.cells[ni].height as i32,
        ))
    }

    /// Reconstruct the path from the last `move_range` start to (tc,tr) if it's in range.
    fn path_to(&mut self, tc: i32, tr: i32) {
        self.path.clear();
        let ti = match self.idx(tc, tr) {
            Some(i) if self.in_move[i] => i,
            _ => return,
        };
        let mut chain: Vec<(i16, i16)> = Vec::new();
        let mut cur = ti as i32;
        while cur >= 0 {
            let c = cur % self.w;
            let r = cur / self.w;
            chain.push((c as i16, r as i16));
            let p = self.parent[cur as usize];
            if cur == p {
                break;
            }
            cur = p;
        }
        chain.reverse(); // start .. target
        self.path = chain;
    }
}

static ISO_GRID: SingleCore<RefCell<IsoBoardGrid>> =
    SingleCore::new(RefCell::new(IsoBoardGrid::new()));

fn with_iso_grid<R>(f: impl FnOnce(&mut IsoBoardGrid) -> R) -> R {
    ISO_GRID.with(|c| f(&mut c.borrow_mut()))
}

// ── Baked iso boards (the `isoboard:` import scheme) ───────────────────────
/// A whole battlefield baked from a Tiled `.tmj` at build time by `include_isoboard!`: the floor
/// background handle plus per-cell terrain/elevation/walkability and the unit spawns. `isob_load`
/// builds the grid straight from it and the game reads the spawns to place units — so the map comes
/// entirely from the Tiled file, with no hand-generated `.tish` data module and no import script.
pub struct IsoBoard {
    pub bg: i32,
    pub w: i32,
    pub h: i32,
    /// Iso projection origin used when the floor atlas was baked (must match game `isoX`/`isoY`).
    pub ox: i32,
    pub oy: i32,
    /// Pixels the floor art rises per elevation unit, as BAKED.
    ///
    /// Normally 8 — Tiled's half-block convention, and what `ISO_LIFT_PER_STEP` assumes. But a board
    /// imported from external board data raises a level by whatever its source chose: the SRPG demos use **2**. A unit
    /// must be lifted by the same number the ART was lifted by, or it stands 6px per level above the
    /// ground it is on — invisible on a flat board, wrong on every terraced one.
    pub lift: i32,
    pub frames: &'static [u8],
    pub heights: &'static [u8],
    pub walk: &'static [u8],
    // Per-cell RAISED stack (the tiles ABOVE ground), in CSR form: cell `i`'s entries are
    // `stack_[elev|tile][stack_off[i] .. stack_off[i+1]]`. Each entry is one block: its TOP elevation
    // (8px units) and its OWN tile frame — so the render draws every level with its real tile, and a
    // tower can mix half/full blocks and different tiles.
    pub stack_off: &'static [u16],
    pub stack_elev: &'static [u8],
    pub stack_tile: &'static [u8],
    pub spawns: &'static [(u8, u8, u8, u8)], // (col, row, cls, team)
    /// The background layer's size in PIXELS. NOT the same as the atlas or the board's own extent:
    /// a direct-upload board is 512x256 (a 64x32 layer), a composited one is a square canvas. The
    /// camera must clamp to THIS or it scrolls past the end of the map and shows the wrap — terrain,
    /// a band of bare backdrop, then terrain again.
    pub mapw: i32,
    pub maph: i32,
    /// Right/bottom edge of the PAINTED content, in pixels. The camera clamps to this; the board's
    /// cell diamond is smaller than what actually gets drawn.
    pub cw: i32,
    pub ch: i32,
    /// One BGR555 backdrop colour per visible scanline, or empty for a flat backdrop.
    ///
    /// The GBA's backdrop is a single palette word, so a board whose real sky is a vertical ramp can
    /// only reproduce it by rewriting that word every scanline. `isob_load` hands this to
    /// `tish_agb::native_sky_set`, which runs it off HBlank DMA. Empty disarms — a board without a
    /// captured sky must not inherit the previous board's.
    pub sky: &'static [u16],
}

/// ⚠️ A FIXED TABLE OF REFERENCES, not a `Vec` of boards, and the difference is the whole reason a
/// game can carry many boards at all.
///
/// Every board is `&'static` data baked into the cartridge, so the registry never needed to own
/// one. As a `Vec<IsoBoard>` it did: registering N boards ran N pushes with log2(N) reallocs,
/// all during module init, before the program body — and the freed buffers left the heap too
/// fragmented for the one big block a GBA UI canvas wants. A 16-board game booted, a 48-board game
/// died in the allocator with plenty of total heap free.
///
/// This costs `MAX_BOARDS * 4` bytes of STATIC memory (1 KB) and allocates nothing, ever.
///
/// ⚠️ Not a `Vec` with `reserve` either — that was tried. Reserving for the worst case charges a
/// two-board game for the 162-board one and broke the ordinary case.
const MAX_BOARDS: usize = 256;
static ISO_BOARDS: SingleCore<RefCell<[Option<&'static IsoBoard>; MAX_BOARDS]>> =
    SingleCore::new(RefCell::new([None; MAX_BOARDS]));
static ISO_BOARDS_N: SingleCore<RefCell<usize>> = SingleCore::new(RefCell::new(0));

/// Register a baked iso board, returning its i32 handle. Called (as plain Rust, not a Value fn)
/// by the `isoboard:` scheme's generated registration in import order, before the program body.
/// Register a board whose `bg` handle is only known at runtime (the background registers itself
/// first). The static board carries `bg: 0`; this remembers the real handle beside the reference.
pub fn native_isoboard_register_bg(board: &'static IsoBoard, bg: i32) -> i32 {
    native_isoboard_register_bg2(board, bg, -1)
}

/// Register a board with BOTH of its backgrounds.
///
/// An SRPG battlefield is two layers: the ground, and a FOREGROUND of scenery — snow mounds, trees,
/// chimneys — that has to draw in front of the units standing behind it. `fg` is `-1` for a board
/// that has no such layer, which is every hand-authored Tiled board.
pub fn native_isoboard_register_bg2(board: &'static IsoBoard, bg: i32, fg: i32) -> i32 {
    let idx = native_isoboard_register(board);
    if idx >= 0 {
        ISO_BOARD_BG.with(|c| c.borrow_mut()[idx as usize] = bg);
        ISO_BOARD_FG.with(|c| c.borrow_mut()[idx as usize] = fg);
    }
    idx
}

static ISO_BOARD_BG: SingleCore<RefCell<[i32; MAX_BOARDS]>> =
    SingleCore::new(RefCell::new([0; MAX_BOARDS]));
static ISO_BOARD_FG: SingleCore<RefCell<[i32; MAX_BOARDS]>> =
    SingleCore::new(RefCell::new([-1; MAX_BOARDS]));

pub fn native_isoboard_register(board: &'static IsoBoard) -> i32 {
    ISO_BOARDS_N.with(|n| {
        let mut n = n.borrow_mut();
        if *n >= MAX_BOARDS {
            return -1;
        }
        let idx = *n as i32;
        ISO_BOARDS.with(|c| c.borrow_mut()[*n] = Some(board));
        *n += 1;
        idx
    })
}

/// Index of cell `(col,row)`'s CSR span in a board, plus its length. `None` if off-board.
fn board_stack_span(b: &IsoBoard, col: i32, row: i32) -> Option<(usize, usize)> {
    if col < 0 || row < 0 || col >= b.w || row >= b.h {
        return None;
    }
    let idx = (row * b.w + col) as usize;
    let start = *b.stack_off.get(idx)? as usize;
    let end = *b.stack_off.get(idx + 1)? as usize;
    Some((start, end - start))
}

/// `isob_stack_count(board, col, row)` — how many RAISED blocks are stacked on a cell (0 for flat).
pub fn isob_stack_count(args: &[Value]) -> Value {
    let (h, c, r) = (n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32);
    Value::Number(
        with_board(h, |b| {
            board_stack_span(b, c, r).map_or(0, |(_, len)| len as i32)
        })
        .unwrap_or(0) as f64,
    )
}
fn stack_field(args: &[Value], f: impl Fn(&IsoBoard, usize) -> i32) -> Value {
    let (h, c, r, i) = (
        n(args, 0) as i32,
        n(args, 1) as i32,
        n(args, 2) as i32,
        n(args, 3) as usize,
    );
    Value::Number(
        with_board(h, |b| match board_stack_span(b, c, r) {
            Some((start, len)) if i < len => f(b, start + i),
            _ => 0,
        })
        .unwrap_or(0) as f64,
    )
}
/// `isob_stack_elev(board, col, row, i)` — the i-th stacked block's TOP elevation (8px units).
pub fn isob_stack_elev(args: &[Value]) -> Value {
    stack_field(args, |b, j| b.stack_elev[j] as i32)
}
/// `isob_stack_tile(board, col, row, i)` — the i-th stacked block's own tile frame.
pub fn isob_stack_tile(args: &[Value]) -> Value {
    stack_field(args, |b, j| b.stack_tile[j] as i32)
}

fn with_board<R>(handle: i32, f: impl FnOnce(&IsoBoard) -> R) -> Option<R> {
    if handle < 0 || handle as usize >= MAX_BOARDS {
        return None;
    }
    ISO_BOARDS.with(|c| c.borrow()[handle as usize].map(f))
}

/// `isob_load(board)` — build the grid from a `isoboard:` board: size it, then fill every cell's
/// elevation / terrain id / walkability from the baked map. Replaces `isob_init` + a per-cell
/// `isob_set_cell` loop. Unit spawns are read separately (`isob_spawn_*`) so the game maps each to its
/// class stats before `isob_add_unit`.
pub fn isob_load(args: &[Value]) -> Value {
    let handle = n(args, 0) as i32;
    // The board's fields are all Copy (ints + `'static` slices), so copy them out and drop the
    // registry borrow before touching ISO_GRID — no nested `SingleCore` borrows.
    if let Some((w, h, frames, heights, walk, sky)) =
        with_board(handle, |b| (b.w, b.h, b.frames, b.heights, b.walk, b.sky))
    {
        // The board's own sky, or a disarm. Done here rather than left to the game so that loading a
        // board never leaves the PREVIOUS board's gradient running behind it.
        tish_agb::native_sky_set(sky);
        with_iso_grid(|t| {
            t.init(w, h);
            for i in 0..(w * h) as usize {
                if let Some(ci) = t.idx(i as i32 % w, i as i32 / w) {
                    t.cells[ci].height = *heights.get(i).unwrap_or(&0);
                    t.cells[ci].tile = *frames.get(i).unwrap_or(&0);
                    t.cells[ci].walkable = *walk.get(i).unwrap_or(&1) != 0;
                }
            }
        });
    }
    Value::Null
}

/// `isob_board_bg(board)` — the floor background handle to hand to `bg_new`. `-1` if unknown.
pub fn isob_board_bg(args: &[Value]) -> Value {
    // ⚠️ From the SIDE TABLE, not the board. The board is a `static` baked at compile time and the
    // background handle is only assigned at registration, so `IsoBoard::bg` is always 0.
    let h = n(args, 0) as i32;
    if h < 0 || h as usize >= MAX_BOARDS {
        return Value::Number(-1.0);
    }
    Value::Number(ISO_BOARD_BG.with(|c| c.borrow()[h as usize]) as f64)
}

/// `isob_board_mapw(board)` / `isob_board_maph(board)` — the layer size in pixels, for camera clamps.
pub fn isob_board_mapw(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.mapw).unwrap_or(512) as f64)
}

pub fn isob_board_maph(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.maph).unwrap_or(512) as f64)
}

/// `isob_board_cw(board)` / `isob_board_ch(board)` — the painted content's right/bottom edge in px.
pub fn isob_board_cw(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.cw).unwrap_or(512) as f64)
}

pub fn isob_board_ch(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.ch).unwrap_or(512) as f64)
}

/// `isob_board_fg(board)` — the FOREGROUND background handle, or `-1` if the board has none.
///
/// Hand it to `bg_new` at a priority ABOVE the unit sprites (a lower number; sprites sit at 2) so
/// that scenery in it — mounds, trees, chimneys — occludes the units standing behind it, which is
/// the whole reason such games keep it on their own layer.
pub fn isob_board_fg(args: &[Value]) -> Value {
    let h = n(args, 0) as i32;
    if h < 0 || h as usize >= MAX_BOARDS {
        return Value::Number(-1.0);
    }
    Value::Number(ISO_BOARD_FG.with(|c| c.borrow()[h as usize]) as f64)
}

/// `isob_board_ox(board)` / `isob_board_oy(board)` — iso projection origin used when the floor was baked.
pub fn isob_board_ox(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.ox).unwrap_or(96) as f64)
}
pub fn isob_board_oy(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.oy).unwrap_or(24) as f64)
}

/// `isob_board_lift(board)` — pixels the floor art rises per elevation unit, as baked. See
/// `IsoBoard::lift`; defaults to the classic 8 for boards that predate the field.
pub fn isob_board_lift(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.lift).unwrap_or(8) as f64)
}

/// `isob_w()` / `isob_h()` — the loaded grid's dimensions (valid after `isob_load` / `isob_init`).
pub fn isob_w(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.w) as f64)
}
pub fn isob_h(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.h) as f64)
}

/// `isob_spawn_count(board)` — how many unit spawns the map defines.
pub fn isob_spawn_count(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.spawns.len() as i32).unwrap_or(0) as f64)
}
fn spawn_field(args: &[Value], f: impl Fn(&(u8, u8, u8, u8)) -> i32) -> Value {
    let (h, i) = (n(args, 0) as i32, n(args, 1) as usize);
    Value::Number(with_board(h, |b| b.spawns.get(i).map(&f).unwrap_or(0)).unwrap_or(0) as f64)
}
/// `isob_spawn_col/row/cls/team(board, i)` — the i-th spawn's grid column / row / class index / team.
pub fn isob_spawn_col(args: &[Value]) -> Value {
    spawn_field(args, |s| s.0 as i32)
}
pub fn isob_spawn_row(args: &[Value]) -> Value {
    spawn_field(args, |s| s.1 as i32)
}
pub fn isob_spawn_cls(args: &[Value]) -> Value {
    spawn_field(args, |s| s.2 as i32)
}
pub fn isob_spawn_team(args: &[Value]) -> Value {
    spawn_field(args, |s| s.3 as i32)
}

/// `isob_init(w, h)` — (re)create a `w×h` iso board (all cells walkable, height 0, unoccupied).
pub fn isob_init(args: &[Value]) -> Value {
    with_iso_grid(|t| t.init(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `isob_set_cell(col, row, height, tile, walkable)` — set a cell's elevation, terrain id, and
/// whether a unit may stand on it. Occupancy is separate (`isob_set_occupant`).
pub fn isob_set_cell(args: &[Value]) -> Value {
    with_iso_grid(|t| {
        if let Some(i) = t.idx(n(args, 0) as i32, n(args, 1) as i32) {
            t.cells[i].height = n(args, 2) as u8;
            t.cells[i].tile = n(args, 3) as u8;
            t.cells[i].walkable = n(args, 4) != 0.0;
        }
    });
    Value::Null
}

/// `isob_height(col, row)` — a cell's elevation (0 off-board).
pub fn isob_height(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(0, |i| t.cells[i].height as i32)
    }) as f64)
}

/// `isob_tile(col, row)` — a cell's terrain/type id.
pub fn isob_tile(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(0, |i| t.cells[i].tile as i32)
    }) as f64)
}

/// `isob_walkable(col, row)` — 1 if a unit may stand on the cell, else 0.
pub fn isob_walkable(args: &[Value]) -> Value {
    Value::Bool(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .is_some_and(|i| t.cells[i].walkable)
    }))
}

/// `isob_set_occupant(col, row, entity)` — mark which unit stands on a cell (-1 = empty). Occupied
/// cells block other units' movement.
pub fn isob_set_occupant(args: &[Value]) -> Value {
    with_iso_grid(|t| {
        if let Some(i) = t.idx(n(args, 0) as i32, n(args, 1) as i32) {
            t.cells[i].occupant = n(args, 2) as i32;
        }
    });
    Value::Null
}

/// `isob_occupant(col, row)` — entity id standing on the cell, or -1.
pub fn isob_occupant(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(-1, |i| t.cells[i].occupant)
    }) as f64)
}

/// `isob_move_range(col, row, move, jump)` — flood-fill the tiles reachable from (col,row) for `move`
/// move points and `jump` max height delta; returns the count. Query with `isob_in_range` /
/// `isob_range_*`, and `isob_path` for a route into it.
///
/// The unit-less form takes the movement type as an optional 5th argument (`isob_move_range(c, r,
/// move, jump, flying)`), defaulting to walking when it is omitted. That is what lets a caller ask
/// the hypothetical — "where could a flier get to from here?" — without a unit to ask it about.
pub fn isob_move_range(args: &[Value]) -> Value {
    let flying = n(args, 4) != 0.0;
    // 6th arg is the mover's team for zone-of-control; absent (or -1) means path with no ZoC.
    let team = if args.len() > 5 {
        n(args, 5) as i32
    } else {
        -1
    };
    with_iso_grid(|t| {
        t.move_range(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            flying,
            team,
        )
    });
    Value::Number(with_iso_grid(|t| t.reach.len()) as f64)
}

/// `isob_move_cost(col, row)` — what the last computed move-range spent to REACH that cell, or -1 if
/// it never did.
///
/// The cell list alone can't tell you what the search decided. On an open 4-connected board almost
/// everything stays reachable whatever the rules say — terrain costs and zone of control change how
/// DEARLY, and detour into the price rather than out of the set. This is the number that moves, so
/// it's the one worth asking for, both for a UI that wants to show what a destination costs and for
/// a test that wants to prove a rule fired.
pub fn isob_move_cost(args: &[Value]) -> Value {
    let (c, r) = (n(args, 0) as i32, n(args, 1) as i32);
    Value::Number(with_iso_grid(|t| match t.idx(c, r) {
        Some(i) if t.in_move[i] => t.dist[i],
        _ => -1,
    }) as f64)
}

/// `isob_in_range(col, row)` — 1 if the cell is in the last computed move-range, else 0 (Number so
/// tish `isob_in_range(...) > 0` works).
pub fn isob_in_range(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(0, |i| t.in_move[i] as i32)
    }) as f64)
}

/// `isob_range_count()` — number of reachable cells from the last `isob_move_range`.
pub fn isob_range_count(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.reach.len()) as f64)
}
/// `isob_range_col(i)` / `isob_range_row(i)` — the i-th reachable cell.
pub fn isob_range_col(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.reach
            .get(n(args, 0) as usize)
            .map_or(-1, |&(c, _)| c as i32)
    }) as f64)
}
pub fn isob_range_row(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.reach
            .get(n(args, 0) as usize)
            .map_or(-1, |&(_, r)| r as i32)
    }) as f64)
}

/// `isob_path(col, row)` — reconstruct the route from the last move-range's origin to (col,row);
/// returns its length (0 if unreachable). Read with `isob_path_len` / `isob_path_col` / `isob_path_row`.
pub fn isob_path(args: &[Value]) -> Value {
    with_iso_grid(|t| t.path_to(n(args, 0) as i32, n(args, 1) as i32));
    Value::Number(with_iso_grid(|t| t.path.len()) as f64)
}
pub fn isob_path_len(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.path.len()) as f64)
}
pub fn isob_path_col(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.path
            .get(n(args, 0) as usize)
            .map_or(-1, |&(c, _)| c as i32)
    }) as f64)
}
pub fn isob_path_row(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.path
            .get(n(args, 0) as usize)
            .map_or(-1, |&(_, r)| r as i32)
    }) as f64)
}

// ── Units & turn order ──────────────────────────────────────────────────────────

/// `isob_add_unit(col, row, team, speed, move, jump, hp)` — register a unit on the board (claiming the
/// cell) and return its id. Ids count up from 0 in registration order.
pub fn isob_add_unit(args: &[Value]) -> Value {
    let id = with_iso_grid(|t| {
        t.add_unit(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as u8,
            n(args, 3) as u16,
            n(args, 4) as u8,
            n(args, 5) as u8,
            n(args, 6) as i16,
        )
    });
    Value::Number(id as f64)
}

/// `isob_clear_units()` — drop every unit, keeping the board.
///
/// ⚠️ Before this existed the ONLY way to empty the unit table was `isob_load`, which reloads the
/// whole board. A game that fights two battles on the SAME board therefore could not clear it:
/// `isoUseBoard` returns early when the board is already current, so `isob_load` never runs, the old
/// units stay registered, and the new battle's ids are appended after them. The symptoms are not
/// obviously a leak — the turn queue offers turns to units from a fight that ended, and the tish
/// side's own count disagrees with the engine's.
pub fn isob_clear_units(_args: &[Value]) -> Value {
    with_iso_grid(|t| {
        t.units.clear();
        for c in t.cells.iter_mut() {
            c.occupant = -1;
        }
    });
    Value::Null
}

/// `isob_unit_count()` — number of registered units.
pub fn isob_unit_count(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.units.len()) as f64)
}

fn unit_field(args: &[Value], f: impl Fn(&IsoBoardUnit) -> i32) -> Value {
    Value::Number(with_iso_grid(|t| t.units.get(n(args, 0) as usize).map_or(-1, f)) as f64)
}

/// Per-unit getters: `isob_unit_col/row/team/hp/maxhp/move/jump/speed(id)`, `isob_unit_alive(id)`.
pub fn isob_unit_col(args: &[Value]) -> Value {
    unit_field(args, |u| u.col)
}
pub fn isob_unit_row(args: &[Value]) -> Value {
    unit_field(args, |u| u.row)
}
pub fn isob_unit_team(args: &[Value]) -> Value {
    unit_field(args, |u| u.team as i32)
}
pub fn isob_unit_hp(args: &[Value]) -> Value {
    unit_field(args, |u| u.hp as i32)
}
pub fn isob_unit_maxhp(args: &[Value]) -> Value {
    unit_field(args, |u| u.max_hp as i32)
}
pub fn isob_unit_move(args: &[Value]) -> Value {
    unit_field(args, |u| u.mov as i32)
}
pub fn isob_unit_jump(args: &[Value]) -> Value {
    unit_field(args, |u| u.jump as i32)
}
pub fn isob_unit_speed(args: &[Value]) -> Value {
    unit_field(args, |u| u.speed as i32)
}
pub fn isob_unit_alive(args: &[Value]) -> Value {
    // Number (1/0), not Bool, so tish `isob_unit_alive(id) > 0` comparisons work on GBA.
    Value::Number(with_iso_grid(|t| {
        t.units
            .get(n(args, 0) as usize)
            .map_or(0, |u| u.alive as i32)
    }) as f64)
}

/// `isob_unit_move_range(id)` — flood-fill the reachable tiles for a unit using its own Move/Jump and
/// movement type (from its current cell). Then query with `isob_in_range` / `isob_range_*` / `isob_path`.
pub fn isob_unit_move_range(args: &[Value]) -> Value {
    with_iso_grid(|t| {
        if let Some(u) = t.units.get(n(args, 0) as usize).copied() {
            t.move_range(
                u.col,
                u.row,
                u.mov as i32,
                u.jump as i32,
                u.flying,
                u.team as i32,
            );
        }
    });
    Value::Number(with_iso_grid(|t| t.reach.len()) as f64)
}

/// `isob_set_terrain_cost(tile, cost)` — move points needed to ENTER a cell of terrain id `tile`.
///
/// The engine deliberately does not know that id 33 is water. It owns the search; the game owns what
/// its tiles mean, so cost is pushed in from the example's terrain table. Defaults to 1 everywhere,
/// so a game that never calls this behaves exactly as it did before costs existed.
///
/// Clamped to ≥ 1: a free tile would break the search's assumption that distance only ever increases
/// as it sweeps outward, and the cheapest way to say "this tile is free to cross" is a flier.
pub fn isob_set_terrain_cost(args: &[Value]) -> Value {
    let tile = n(args, 0) as usize;
    let cost = (n(args, 1) as i32).clamp(1, 255) as u8;
    with_iso_grid(|t| {
        if tile < t.cost.len() {
            t.cost[tile] = cost;
        }
    });
    Value::Null
}

/// `isob_knockback(id, from_col, from_row)` — shove a unit one tile directly away from (from_col,
/// from_row). Returns 1 if it moved, 0 if something stopped it.
///
/// Height is deliberately NOT a reason to refuse: being shoved off a ledge is the interesting case,
/// and a `jump` check here would quietly make knockback do nothing near exactly the terrain that
/// makes it worth having. The board edge, a solid cell and another unit all still stop it — a unit
/// backed against any of those takes the hit where it stands.
///
/// The engine moves the body and reports that it did. What a fall COSTS is the game's to decide, so
/// the caller compares the heights either side and applies its own damage; the same split as the
/// damage formula.
pub fn isob_knockback(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let (sc, sr) = (n(args, 1) as i32, n(args, 2) as i32);
    let moved = with_iso_grid(|t| match t.knock_dest(id, sc, sr) {
        Some((nc, nr, _)) => {
            t.unit_set_pos(id, nc, nr);
            true
        }
        None => false,
    });
    Value::Number(moved as i32 as f64)
}

/// `isob_knock_drop(id, from_col, from_row)` — how far the unit WOULD fall if shoved from there, or 0
/// if the shove is blocked or lands level or uphill.
///
/// This exists so an AI can price a shove before committing to one. It deliberately answers with the
/// drop rather than with the destination: the destination would make the caller re-derive the rule
/// for which direction a shove goes and what stops it, and the moment two copies of that rule exist
/// they start to disagree. Blocked and level both answer 0 because they are worth the same to anyone
/// asking — no fall, no damage.
pub fn isob_knock_drop(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let (sc, sr) = (n(args, 1) as i32, n(args, 2) as i32);
    Value::Number(
        with_iso_grid(|t| t.knock_dest(id, sc, sr).map_or(0, |(_, _, d)| d.max(0))) as f64,
    )
}

/// `isob_set_zoc(on)` — turn zone of control on or off for every subsequent move-range query.
///
/// With it on, stepping into a tile adjacent to a living enemy ends the move — the tile is reachable,
/// nothing through it is. That turns a defender from an obstacle you route around into one you have
/// to deal with, which is the entire point of a front line. Off by default so this changes nothing
/// for a game that does not ask for it.
pub fn isob_set_zoc(args: &[Value]) -> Value {
    let on = n(args, 0) != 0.0;
    with_iso_grid(|t| t.zoc_on = on);
    Value::Null
}

/// `isob_revive(id, hp)` — bring a KO'd unit back on the cell it fell on, at `hp` (clamped to its
/// maximum, and to at least 1). Returns 1 if it stood up, 0 if it was already alive or something has
/// since walked onto its cell.
///
/// The occupancy check is the whole reason this cannot live in the example: death RELEASES the cell,
/// so a body is not a wall, and by the time anyone reaches it with a Phoenix Down somebody may be
/// standing there. The engine owns that map, so it is the only place that can answer honestly —
/// and returning 0 rather than reviving onto an occupied cell means the caller can decline to spend
/// the item instead of quietly creating two units on one tile.
pub fn isob_revive(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let hp = n(args, 1) as i16;
    Value::Number(with_iso_grid(|t| {
        let (col, row, max_hp) = match t.units.get(id as usize) {
            Some(u) if !u.alive => (u.col, u.row, u.max_hp),
            _ => return 0,
        };
        match t.idx(col, row) {
            Some(i) if t.cells[i].occupant < 0 => t.cells[i].occupant = id,
            _ => return 0,
        }
        if let Some(u) = t.units.get_mut(id as usize) {
            u.alive = true;
            u.hp = hp.clamp(1, max_hp);
        }
        1
    }) as f64)
}

/// `isob_turn_end(id, moved, acted)` — charge the rest of the turn's counter cost, once the game knows
/// what the unit actually did with it. The base cost was taken when the turn was handed out.
///
/// The split exists so that forgetting to call this cannot wedge the queue: a game that never calls
/// it still has every unit paying 500 a turn and the order still advances, it just loses the tempo
/// rule. Making `turn_next` charge nothing and rely on this would mean one missed call returns the
/// same unit forever.
pub fn isob_turn_end(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    let moved = n(args, 1) != 0.0;
    let acted = n(args, 2) != 0.0;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id) {
            let mut cost = 0;
            if moved {
                cost += COST_MOVE;
            }
            if acted {
                cost += COST_ACTION;
            }
            u.ct = (u.ct - cost).max(0);
        }
    });
    Value::Null
}

/// `isob_unit_ct(id)` — the unit's current turn counter, for a UI that wants to show who is next.
pub fn isob_unit_ct(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    Value::Number(with_iso_grid(|t| t.units.get(id).map_or(0, |u| u.ct)) as f64)
}

/// `isob_unit_set_speed_scale(id, percent)` — scale how fast a unit's turn counter fills. 100 is
/// normal; the reference SRPG's Haste and Slow are 200 and 50.
///
/// The engine owns the turn queue and therefore owns this; which status sets it, for how long, and
/// what the numbers are stay the game's. It scales ACCRUAL and not the Speed stat, so the AI's threat
/// tables and the Status screen go on reading an unmodified stat block, and clearing the status is
/// just setting it back to 100 — there is no original value to remember and no way to double-apply.
///
/// Clamped to 1..=1000: a scale of 0 would stop the unit accruing at all, which is not a slow but a
/// removal, and would leave `turn_next` hunting for a tick count that never arrives.
pub fn isob_unit_set_speed_scale(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    let pct = (n(args, 1) as i32).clamp(1, 1000) as u16;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id) {
            u.speed_scale = pct;
        }
    });
    Value::Null
}

/// `isob_unit_set_flying(id, flying)` — a flier pays 1 move point per tile and ignores height deltas.
///
/// It is NOT a bigger move budget: it changes which tiles the budget can be spent on, so a flier
/// crosses a ford or a cliff that would cost a walker two points or stop it outright, and is no
/// faster than anyone else over open ground. Solid cells and occupied cells still block it.
pub fn isob_unit_set_flying(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    let f = n(args, 1) != 0.0;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id) {
            u.flying = f;
        }
    });
    Value::Null
}

/// `isob_unit_set_pos(id, col, row)` — move a unit (updates board occupancy).
pub fn isob_unit_set_pos(args: &[Value]) -> Value {
    with_iso_grid(|t| t.unit_set_pos(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `isob_damage(id, amount)` — subtract HP; a unit at ≤0 HP dies (alive=false, cell freed). Returns
/// remaining HP.
pub fn isob_damage(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let amt = n(args, 1) as i16;
    with_iso_grid(|t| {
        let (dead, c, r) = match t.units.get_mut(id as usize) {
            Some(u) if u.alive => {
                u.hp -= amt;
                if u.hp <= 0 {
                    u.hp = 0;
                    u.alive = false;
                }
                (!u.alive, u.col, u.row)
            }
            _ => (false, 0, 0),
        };
        if dead {
            if let Some(i) = t.idx(c, r) {
                if t.cells[i].occupant == id {
                    t.cells[i].occupant = -1;
                }
            }
        }
    });
    unit_field(args, |u| u.hp as i32)
}

/// `isob_heal(id, amount)` — restore HP to a living unit, capped at its `max_hp`. Returns the unit's
/// new HP (0 if the unit is dead/unknown — dead units cannot be healed here). For support classes.
pub fn isob_heal(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let amt = n(args, 1) as i16;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id as usize) {
            if u.alive {
                u.hp = (u.hp + amt).min(u.max_hp);
            }
        }
    });
    unit_field(args, |u| if u.alive { u.hp as i32 } else { 0 })
}

/// `isob_unit_set_maxhp(id, v)` — set a unit's HP ceiling, keeping current HP in range. Returns the
/// new maximum (0 for an unknown unit).
///
/// ⚠️ This exists because the SRPG growth rules (now in the chuggie-tactics repo) documented a rule they could not implement: "what
/// a level does NOT raise is HP … there is no setter for the maximum, so growing it would mean a
/// Rust change." Real SRPG levels raise HP, so the engine was quietly deciding a game-design
/// question. A missing setter is not a neutral omission — it becomes a rule.
///
/// Raising the ceiling raises current HP by the same amount rather than leaving the unit wounded: a
/// level-up mid-battle that hands you a bigger empty bar reads as a penalty. Lowering it (a curse, a
/// job change) clamps current HP down instead.
pub fn isob_unit_set_maxhp(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let v = (n(args, 1) as i16).max(1);
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id as usize) {
            let delta = v - u.max_hp;
            u.max_hp = v;
            if delta > 0 {
                u.hp += delta;
            }
            if u.hp > u.max_hp {
                u.hp = u.max_hp;
            }
        }
    });
    unit_field(args, |u| u.max_hp as i32)
}

/// `isob_turn_next()` — advance the speed-based turn queue and return the id of the unit whose turn it
/// is (or -1 if none are alive).
pub fn isob_turn_next(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.turn_next()) as f64)
}

// ── Attack targeting ────────────────────────────────────────────────────────────

impl IsoBoardGrid {
    /// A living enemy of `unit_id` standing on one of its 4 orthogonal neighbours, or -1.
    fn adjacent_enemy(&self, unit_id: i32) -> i32 {
        let u = match self.units.get(unit_id as usize) {
            Some(u) => *u,
            None => return -1,
        };
        const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for (dc, dr) in DIRS {
            if let Some(i) = self.idx(u.col + dc, u.row + dr) {
                let occ = self.cells[i].occupant;
                if occ >= 0 {
                    if let Some(other) = self.units.get(occ as usize) {
                        if other.alive && other.team != u.team {
                            return occ;
                        }
                    }
                }
            }
        }
        -1
    }
}

/// `isob_adjacent_enemy(unitId)` — a living enemy on one of the unit's 4 neighbouring tiles (in
/// range of a melee attack), or -1 if none. Used for attack targeting and AI.
pub fn isob_adjacent_enemy(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.adjacent_enemy(n(args, 0) as i32)) as f64)
}

// ── isob_* typed siblings ────────────────────────────────────────────────────────
// Thin adapters over the boxed exports above: the SAME body executes, so behaviour
// cannot drift — what a typed call removes is the caller's namespace lookup +
// value_call dispatch + per-argument Value boxing (~72 ticks a call vs ~7). The
// stack-enum Value construction in here is noise next to that. Generated shape;
// invert any adapter to a direct internal call if it ever shows in a profile.

pub fn isob_stack_count_typed(p0: i32, p1: i32, p2: i32) -> i32 {
    match isob_stack_count(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_stack_elev_typed(p0: i32, p1: i32, p2: i32, p3: i32) -> i32 {
    match isob_stack_elev(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_stack_tile_typed(p0: i32, p1: i32, p2: i32, p3: i32) -> i32 {
    match isob_stack_tile(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_load_typed(p0: i32) {
    isob_load(&[Value::Number(p0 as f64)]);
}
pub fn isob_board_bg_typed(p0: i32) -> i32 {
    match isob_board_bg(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_board_cw_typed(p0: i32) -> i32 {
    match isob_board_cw(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}
pub fn isob_board_ch_typed(p0: i32) -> i32 {
    match isob_board_ch(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}
pub fn isob_board_mapw_typed(p0: i32) -> i32 {
    match isob_board_mapw(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}
pub fn isob_board_maph_typed(p0: i32) -> i32 {
    match isob_board_maph(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}
pub fn isob_board_fg_typed(p0: i32) -> i32 {
    match isob_board_fg(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => -1,
    }
}
pub fn isob_board_ox_typed(p0: i32) -> i32 {
    match isob_board_ox(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_board_oy_typed(p0: i32) -> i32 {
    match isob_board_oy(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_board_lift_typed(p0: i32) -> i32 {
    match isob_board_lift(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_w_typed() -> i32 {
    match isob_w(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_h_typed() -> i32 {
    match isob_h(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_spawn_count_typed(p0: i32) -> i32 {
    match isob_spawn_count(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_spawn_col_typed(p0: i32, p1: i32) -> i32 {
    match isob_spawn_col(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_spawn_row_typed(p0: i32, p1: i32) -> i32 {
    match isob_spawn_row(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_spawn_cls_typed(p0: i32, p1: i32) -> i32 {
    match isob_spawn_cls(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_spawn_team_typed(p0: i32, p1: i32) -> i32 {
    match isob_spawn_team(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_init_typed(p0: i32, p1: i32) {
    isob_init(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn isob_set_cell_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32) {
    isob_set_cell(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
    ]);
}
pub fn isob_height_typed(p0: i32, p1: i32) -> i32 {
    match isob_height(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_tile_typed(p0: i32, p1: i32) -> i32 {
    match isob_tile(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_walkable_typed(p0: i32, p1: i32) -> i32 {
    match isob_walkable(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Bool(b) => b as i32,
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_set_occupant_typed(p0: i32, p1: i32, p2: i32) {
    isob_set_occupant(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}
pub fn isob_occupant_typed(p0: i32, p1: i32) -> i32 {
    match isob_occupant(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_move_range_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32) -> i32 {
    match isob_move_range(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
        Value::Number(p5 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_move_cost_typed(p0: i32, p1: i32) -> i32 {
    match isob_move_cost(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_in_range_typed(p0: i32, p1: i32) -> i32 {
    match isob_in_range(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_range_count_typed() -> i32 {
    match isob_range_count(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_range_col_typed(p0: i32) -> i32 {
    match isob_range_col(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_range_row_typed(p0: i32) -> i32 {
    match isob_range_row(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_path_typed(p0: i32, p1: i32) -> i32 {
    match isob_path(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_path_len_typed() -> i32 {
    match isob_path_len(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_path_col_typed(p0: i32) -> i32 {
    match isob_path_col(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_path_row_typed(p0: i32) -> i32 {
    match isob_path_row(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_add_unit_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32) -> i32 {
    match isob_add_unit(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
        Value::Number(p5 as f64),
        Value::Number(p6 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_clear_units_typed() {
    isob_clear_units(&[]);
}
pub fn isob_unit_count_typed() -> i32 {
    match isob_unit_count(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_col_typed(p0: i32) -> i32 {
    match isob_unit_col(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_row_typed(p0: i32) -> i32 {
    match isob_unit_row(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_team_typed(p0: i32) -> i32 {
    match isob_unit_team(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_hp_typed(p0: i32) -> i32 {
    match isob_unit_hp(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_maxhp_typed(p0: i32) -> i32 {
    match isob_unit_maxhp(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_move_typed(p0: i32) -> i32 {
    match isob_unit_move(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_jump_typed(p0: i32) -> i32 {
    match isob_unit_jump(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_speed_typed(p0: i32) -> i32 {
    match isob_unit_speed(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_alive_typed(p0: i32) -> i32 {
    match isob_unit_alive(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_move_range_typed(p0: i32) -> i32 {
    match isob_unit_move_range(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_set_terrain_cost_typed(p0: i32, p1: i32) {
    isob_set_terrain_cost(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn isob_knockback_typed(p0: i32, p1: i32, p2: i32) -> i32 {
    match isob_knockback(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_knock_drop_typed(p0: i32, p1: i32, p2: i32) -> i32 {
    match isob_knock_drop(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_set_zoc_typed(p0: i32) {
    isob_set_zoc(&[Value::Number(p0 as f64)]);
}
pub fn isob_revive_typed(p0: i32, p1: i32) -> i32 {
    match isob_revive(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_turn_end_typed(p0: i32, p1: i32, p2: i32) {
    isob_turn_end(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}
pub fn isob_unit_ct_typed(p0: i32) -> i32 {
    match isob_unit_ct(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_set_speed_scale_typed(p0: i32, p1: i32) {
    isob_unit_set_speed_scale(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn isob_unit_set_flying_typed(p0: i32, p1: i32) {
    isob_unit_set_flying(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn isob_unit_set_pos_typed(p0: i32, p1: i32, p2: i32) {
    isob_unit_set_pos(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}
pub fn isob_damage_typed(p0: i32, p1: i32) -> i32 {
    match isob_damage(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_heal_typed(p0: i32, p1: i32) -> i32 {
    match isob_heal(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_unit_set_maxhp_typed(p0: i32, p1: i32) -> i32 {
    match isob_unit_set_maxhp(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_turn_next_typed() -> i32 {
    match isob_turn_next(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn isob_adjacent_enemy_typed(p0: i32) -> i32 {
    match isob_adjacent_enemy(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

// ── typed siblings (grid/platformer genre + hot entity surface) ──────────────────
pub fn entity_count_typed() -> i32 {
    match entity_count(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn entity_count_tag_typed(p0: i32) -> i32 {
    match entity_count_tag(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn entity_tag_typed(p0: i32) -> i32 {
    match entity_tag(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn frame_period_typed(p0: i32) -> i32 {
    match frame_period(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn grid_col_typed(p0: i32) -> i32 {
    match grid_col(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn grid_facing_typed(p0: i32) -> i32 {
    match grid_facing(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn grid_from_map_typed() {
    grid_from_map(&[]);
}
pub fn grid_interact_typed(p0: i32) -> i32 {
    match grid_interact(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn grid_moving_typed(p0: i32) -> i32 {
    match grid_moving(&[Value::Number(p0 as f64)]) {
        Value::Bool(b) => b as i32,
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn grid_row_typed(p0: i32) -> i32 {
    match grid_row(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn grid_set_ladder_typed(p0: i32, p1: i32, p2: i32) {
    grid_set_ladder(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}
pub fn grid_set_oneway_typed(p0: i32, p1: i32, p2: i32) {
    grid_set_oneway(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}
pub fn grid_set_solid_typed(p0: i32, p1: i32, p2: i32) {
    grid_set_solid(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}
pub fn grid_setup_typed(p0: i32, p1: i32) {
    grid_setup(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn grid_step_typed(p0: i32, p1: i32, p2: i32) {
    grid_step(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}
pub fn heal_typed(p0: i32, p1: i32) {
    heal(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn overlaps_typed(p0: i32, p1: i32) -> i32 {
    match overlaps(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Bool(b) => b as i32,
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn platformer_blocked_typed(p0: i32) -> i32 {
    match platformer_blocked(&[Value::Number(p0 as f64)]) {
        Value::Bool(b) => b as i32,
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn platformer_bounce_typed(p0: i32, p1: i32) {
    platformer_bounce(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn platformer_can_interact_typed(p0: i32, p1: i32) -> i32 {
    match platformer_can_interact(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn platformer_drop_typed(p0: i32) {
    platformer_drop(&[Value::Number(p0 as f64)]);
}
pub fn platformer_face_typed(p0: i32) -> i32 {
    match platformer_face(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn platformer_grounded_typed(p0: i32) -> i32 {
    match platformer_grounded(&[Value::Number(p0 as f64)]) {
        Value::Bool(b) => b as i32,
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn platformer_hold_typed(p0: i32, p1: i32) {
    platformer_hold(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn platformer_interact_typed(p0: i32, p1: i32) -> i32 {
    match platformer_interact(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn platformer_jump_typed(p0: i32) {
    platformer_jump(&[Value::Number(p0 as f64)]);
}
pub fn platformer_jump_release_typed(p0: i32) {
    platformer_jump_release(&[Value::Number(p0 as f64)]);
}
pub fn platformer_run_typed(p0: i32) {
    platformer_run(&[Value::Number(p0 as f64)]);
}
/// Typed twin of [`platformer_set_speed`]. `walk`/`run` are FIXED — the boxed form multiplies the
/// incoming float by 256, and `Fixed::to_raw()` is already that same 8-bit fraction, so the two
/// paths agree. Taking i32 here (as this did) truncated every fractional speed to a whole pixel per
/// frame: `walkSpeed: 1.9` arrived as 1, and the engine's own documented defaults (1.25 / 2.25)
/// became 1 and 2. The symptom is a hero who "barely moves", and no amount of tuning the number
/// fixes it because every value between 1 and 2 is the same value.
pub fn platformer_set_speed_typed(e: i32, walk: Fixed, run: Fixed) {
    with_world(|w| w.platformer_set_speed(e, walk.to_raw(), run.to_raw()));
}
/// See [`platformer_set_physics`]. `fixed` args for the same reason as the speeds: 3.4 px/frame
/// must not truncate to 3.
pub fn platformer_set_physics_typed(e: i32, jump: Fixed, grav: Fixed) {
    with_world(|w| w.platformer_set_physics(e, jump.to_raw(), grav.to_raw()));
}
/// See [`platformer_launch`].
pub fn platformer_launch_typed(e: i32, vx: Fixed, vy: Fixed) {
    with_world(|w| w.platformer_launch(e, vx.to_raw(), vy.to_raw()));
}
/// ⚠️ `vy` IS FIXED-POINT, NOT AN INTEGER — the same trap `platformer_set_speed` was fixed for.
/// The boxed native multiplies by 256, so an `i32` fast path truncated every fractional velocity
/// on the way in: `platformer_set_vy(e, 0.75)` (the wall-slide scrape) arrived as 0 and the
/// character hung motionless on the wall instead of sliding down it.
pub fn platformer_set_vy_typed(e: i32, vy: Fixed) {
    with_world(|w| w.platformer_set_vy(e, vy.to_raw()));
}
/// ⚠️ Likewise on the way OUT: truncating to whole px reported a body rising at -0.5 px/frame as
/// 0, so `vy < 0` was false and the state machine called a jump a fall for its first frames.
pub fn platformer_vy_typed(e: i32) -> Fixed {
    with_world(|w| {
        w.slot_of(e)
            .filter(|&s| w.has(s, C_PLATFORMER))
            .map(|s| w.platformer[s].vy)
            .unwrap_or(Fixed::from_raw(0))
    })
}
pub fn platformer_walk_typed(p0: i32, p1: i32) {
    platformer_walk(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn set_chase_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32) {
    set_chase(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
    ]);
}
pub fn flow_goal_typed(p0: i32, p1: i32, p2: i32) {
    with_world(|w| w.flow_goal(p0 as usize, p1, p2));
}
pub fn flow_dist_typed(p0: i32, p1: i32, p2: i32) -> i32 {
    with_world(|w| w.flow_dist(p0 as usize, p1, p2))
}
pub fn set_seek_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32) {
    with_world(|w| w.set_seek(p0, p1, p2, p3, p4));
}
pub fn clear_seek_typed(p0: i32) {
    with_world(|w| w.clear_seek(p0));
}
pub fn seek_arrived_typed(p0: i32) -> i32 {
    with_world(|w| w.seek_arrived(p0)) as i32
}
pub fn set_soldier_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32) {
    with_world(|w| w.set_soldier(p0, p1, p2, p3, p4));
}
pub fn soldier_target_typed(p0: i32) -> i32 {
    with_world(|w| w.soldier_target(p0))
}
pub fn soldier_team_typed(p0: i32) -> i32 {
    with_world(|w| w.soldier_team(p0))
}
pub fn fog_init_typed(p0: i32, p1: i32) {
    with_world(|w| w.fog_init(p0, p1));
}
pub fn set_sleeping_typed(p0: i32, p1: i32) {
    set_sleeping(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn terrain_set_typed(p0: i32, p1: i32, p2: i32, p3: i32) {
    with_world(|w| w.terrain_set(p0, p1, p2, p3));
}
pub fn terrain_blit_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32) -> i32 {
    with_world(|w| w.terrain_blit(p0, p1, p2, p3, p4, p5))
}
pub fn set_vision_typed(p0: i32, p1: i32) {
    with_world(|w| w.set_vision(p0, p1));
}
pub fn fog_reveal_typed(p0: i32, p1: i32, p2: i32) {
    with_world(|w| w.fog_reveal(p0, p1, p2));
}
pub fn fog_state_typed(p0: i32, p1: i32) -> i32 {
    match fog_state(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn fog_blit_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32) -> i32 {
    with_world(|w| w.fog_blit(p0, p1, p2, p3, p4, p5, p6))
}
pub fn set_guard_typed(p0: i32, p1: i32) {
    set_guard(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn set_hopper_typed(p0: i32, p1: i32) {
    set_hopper(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn set_wanderer_typed(p0: i32, p1: i32) {
    set_wanderer(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn topdown_speed_raw_typed(p0: i32, p1: i32) {
    topdown_speed_raw(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}
pub fn wanderer_wants_shot_typed(p0: i32) -> i32 {
    match wanderer_wants_shot(&[Value::Number(p0 as f64)]) {
        Value::Number(n) => n as i32,
        _ => 0,
    }
}
pub fn set_jumper_typed(p0: i32) {
    set_jumper(&[Value::Number(p0 as f64)]);
}
pub fn step_ticks_typed(p0: i32) -> i32 {
    match step_ticks(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn swing_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32) -> i32 {
    match swing(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
        Value::Number(p5 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
pub fn world_step_typed() -> i32 {
    match world_step(&[]) {
        Value::Bool(b) => b as i32,
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

/// `iso_stack_count(board, col, row)` — how many RAISED blocks are stacked on a cell (0 for flat).
pub fn iso_stack_count(args: &[Value]) -> Value {
    let (h, c, r) = (n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32);
    Value::Number(
        with_board(h, |b| {
            board_stack_span(b, c, r).map_or(0, |(_, len)| len as i32)
        })
        .unwrap_or(0) as f64,
    )
}

/// `iso_stack_elev(board, col, row, i)` — the i-th stacked block's TOP elevation (8px units).
pub fn iso_stack_elev(args: &[Value]) -> Value {
    stack_field(args, |b, j| b.stack_elev[j] as i32)
}

/// `iso_stack_tile(board, col, row, i)` — the i-th stacked block's own tile frame.
pub fn iso_stack_tile(args: &[Value]) -> Value {
    stack_field(args, |b, j| b.stack_tile[j] as i32)
}

/// `iso_load(board)` — build the grid from a `tactics:` board: size it, then fill every cell's
/// elevation / terrain id / walkability from the baked map. Replaces `iso_init` + a per-cell
/// `iso_set_cell` loop. Unit spawns are read separately (`iso_spawn_*`) so the game maps each to its
/// class stats before `iso_add_unit`.
pub fn iso_load(args: &[Value]) -> Value {
    let handle = n(args, 0) as i32;
    // The board's fields are all Copy (ints + `'static` slices), so copy them out and drop the
    // registry borrow before touching ISO_GRID — no nested `SingleCore` borrows.
    if let Some((w, h, frames, heights, walk, sky)) =
        with_board(handle, |b| (b.w, b.h, b.frames, b.heights, b.walk, b.sky))
    {
        // The board's own sky, or a disarm. Done here rather than left to the game so that loading a
        // board never leaves the PREVIOUS board's gradient running behind it.
        tish_agb::native_sky_set(sky);
        with_iso_grid(|t| {
            t.init(w, h);
            for i in 0..(w * h) as usize {
                if let Some(ci) = t.idx(i as i32 % w, i as i32 / w) {
                    t.cells[ci].height = *heights.get(i).unwrap_or(&0);
                    t.cells[ci].tile = *frames.get(i).unwrap_or(&0);
                    t.cells[ci].walkable = *walk.get(i).unwrap_or(&1) != 0;
                }
            }
        });
    }
    Value::Null
}

/// `iso_board_bg(board)` — the floor background handle to hand to `bg_new`. `-1` if unknown.
pub fn iso_board_bg(args: &[Value]) -> Value {
    // ⚠️ From the SIDE TABLE, not the board. The board is a `static` baked at compile time and the
    // background handle is only assigned at registration, so `IsoBoard::bg` is always 0.
    let h = n(args, 0) as i32;
    if h < 0 || h as usize >= MAX_BOARDS {
        return Value::Number(-1.0);
    }
    Value::Number(ISO_BOARD_BG.with(|c| c.borrow()[h as usize]) as f64)
}

/// `iso_board_mapw(board)` / `iso_board_maph(board)` — the layer size in pixels, for camera clamps.
pub fn iso_board_mapw(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.mapw).unwrap_or(512) as f64)
}

pub fn iso_board_maph(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.maph).unwrap_or(512) as f64)
}

/// `iso_board_cw(board)` / `iso_board_ch(board)` — the painted content's right/bottom edge in px.
pub fn iso_board_cw(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.cw).unwrap_or(512) as f64)
}

pub fn iso_board_ch(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.ch).unwrap_or(512) as f64)
}

/// `iso_board_fg(board)` — the FOREGROUND background handle, or `-1` if the board has none.
///
/// Hand it to `bg_new` at a priority ABOVE the unit sprites (a lower number; sprites sit at 2) so
/// that scenery in it — mounds, trees, chimneys — occludes the units standing behind it, which is
/// the whole reason the foreground is a layer of its own.
pub fn iso_board_fg(args: &[Value]) -> Value {
    let h = n(args, 0) as i32;
    if h < 0 || h as usize >= MAX_BOARDS {
        return Value::Number(-1.0);
    }
    Value::Number(ISO_BOARD_FG.with(|c| c.borrow()[h as usize]) as f64)
}

/// `iso_board_ox(board)` / `iso_board_oy(board)` — iso projection origin used when the floor was baked.
pub fn iso_board_ox(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.ox).unwrap_or(96) as f64)
}

pub fn iso_board_oy(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.oy).unwrap_or(24) as f64)
}

/// `iso_board_lift(board)` — pixels the floor art rises per elevation unit, as baked. See
/// `IsoBoard::lift`; defaults to the classic 8 for boards that predate the field.
pub fn iso_board_lift(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.lift).unwrap_or(8) as f64)
}

/// `iso_w()` / `iso_h()` — the loaded grid's dimensions (valid after `iso_load` / `iso_init`).
pub fn iso_w(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.w) as f64)
}

pub fn iso_h(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.h) as f64)
}

/// `iso_spawn_count(board)` — how many unit spawns the map defines.
pub fn iso_spawn_count(args: &[Value]) -> Value {
    Value::Number(with_board(n(args, 0) as i32, |b| b.spawns.len() as i32).unwrap_or(0) as f64)
}

/// `iso_spawn_col/row/cls/team(board, i)` — the i-th spawn's grid column / row / class index / team.
pub fn iso_spawn_col(args: &[Value]) -> Value {
    spawn_field(args, |s| s.0 as i32)
}

pub fn iso_spawn_row(args: &[Value]) -> Value {
    spawn_field(args, |s| s.1 as i32)
}

pub fn iso_spawn_cls(args: &[Value]) -> Value {
    spawn_field(args, |s| s.2 as i32)
}

pub fn iso_spawn_team(args: &[Value]) -> Value {
    spawn_field(args, |s| s.3 as i32)
}

/// `iso_init(w, h)` — (re)create a `w×h` tactics board (all cells walkable, height 0, unoccupied).
pub fn iso_init(args: &[Value]) -> Value {
    with_iso_grid(|t| t.init(n(args, 0) as i32, n(args, 1) as i32));
    Value::Null
}

/// `iso_set_cell(col, row, height, tile, walkable)` — set a cell's elevation, terrain id, and
/// whether a unit may stand on it. Occupancy is separate (`iso_set_occupant`).
pub fn iso_set_cell(args: &[Value]) -> Value {
    with_iso_grid(|t| {
        if let Some(i) = t.idx(n(args, 0) as i32, n(args, 1) as i32) {
            t.cells[i].height = n(args, 2) as u8;
            t.cells[i].tile = n(args, 3) as u8;
            t.cells[i].walkable = n(args, 4) != 0.0;
        }
    });
    Value::Null
}

/// `iso_height(col, row)` — a cell's elevation (0 off-board).
pub fn iso_height(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(0, |i| t.cells[i].height as i32)
    }) as f64)
}

/// `iso_tile(col, row)` — a cell's terrain/type id.
pub fn iso_tile(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(0, |i| t.cells[i].tile as i32)
    }) as f64)
}

/// `iso_walkable(col, row)` — 1 if a unit may stand on the cell, else 0.
pub fn iso_walkable(args: &[Value]) -> Value {
    Value::Bool(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(false, |i| t.cells[i].walkable)
    }))
}

/// `iso_set_occupant(col, row, entity)` — mark which unit stands on a cell (-1 = empty). Occupied
/// cells block other units' movement.
pub fn iso_set_occupant(args: &[Value]) -> Value {
    with_iso_grid(|t| {
        if let Some(i) = t.idx(n(args, 0) as i32, n(args, 1) as i32) {
            t.cells[i].occupant = n(args, 2) as i32;
        }
    });
    Value::Null
}

/// `iso_occupant(col, row)` — entity id standing on the cell, or -1.
pub fn iso_occupant(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(-1, |i| t.cells[i].occupant)
    }) as f64)
}

/// `iso_move_range(col, row, move, jump)` — flood-fill the tiles reachable from (col,row) for `move`
/// move points and `jump` max height delta; returns the count. Query with `iso_in_range` /
/// `iso_range_*`, and `iso_path` for a route into it.
///
/// The unit-less form takes the movement type as an optional 5th argument (`iso_move_range(c, r,
/// move, jump, flying)`), defaulting to walking when it is omitted. That is what lets a caller ask
/// the hypothetical — "where could a flier get to from here?" — without a unit to ask it about.
pub fn iso_move_range(args: &[Value]) -> Value {
    let flying = n(args, 4) != 0.0;
    // 6th arg is the mover's team for zone-of-control; absent (or -1) means path with no ZoC.
    let team = if args.len() > 5 {
        n(args, 5) as i32
    } else {
        -1
    };
    with_iso_grid(|t| {
        t.move_range(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as i32,
            n(args, 3) as i32,
            flying,
            team,
        )
    });
    Value::Number(with_iso_grid(|t| t.reach.len()) as f64)
}

/// `iso_move_cost(col, row)` — what the last computed move-range spent to REACH that cell, or -1 if
/// it never did.
///
/// The cell list alone can't tell you what the search decided. On an open 4-connected board almost
/// everything stays reachable whatever the rules say — terrain costs and zone of control change how
/// DEARLY, and detour into the price rather than out of the set. This is the number that moves, so
/// it's the one worth asking for, both for a UI that wants to show what a destination costs and for
/// a test that wants to prove a rule fired.
pub fn iso_move_cost(args: &[Value]) -> Value {
    let (c, r) = (n(args, 0) as i32, n(args, 1) as i32);
    Value::Number(with_iso_grid(|t| match t.idx(c, r) {
        Some(i) if t.in_move[i] => t.dist[i],
        _ => -1,
    }) as f64)
}

/// `iso_in_range(col, row)` — 1 if the cell is in the last computed move-range, else 0 (Number so
/// tish `iso_in_range(...) > 0` works).
pub fn iso_in_range(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.idx(n(args, 0) as i32, n(args, 1) as i32)
            .map_or(0, |i| t.in_move[i] as i32)
    }) as f64)
}

/// `iso_range_count()` — number of reachable cells from the last `iso_move_range`.
pub fn iso_range_count(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.reach.len()) as f64)
}

/// `iso_range_col(i)` / `iso_range_row(i)` — the i-th reachable cell.
pub fn iso_range_col(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.reach
            .get(n(args, 0) as usize)
            .map_or(-1, |&(c, _)| c as i32)
    }) as f64)
}

pub fn iso_range_row(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.reach
            .get(n(args, 0) as usize)
            .map_or(-1, |&(_, r)| r as i32)
    }) as f64)
}

/// `iso_path(col, row)` — reconstruct the route from the last move-range's origin to (col,row);
/// returns its length (0 if unreachable). Read with `iso_path_len` / `iso_path_col` / `iso_path_row`.
pub fn iso_path(args: &[Value]) -> Value {
    with_iso_grid(|t| t.path_to(n(args, 0) as i32, n(args, 1) as i32));
    Value::Number(with_iso_grid(|t| t.path.len()) as f64)
}

pub fn iso_path_len(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.path.len()) as f64)
}

pub fn iso_path_col(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.path
            .get(n(args, 0) as usize)
            .map_or(-1, |&(c, _)| c as i32)
    }) as f64)
}

pub fn iso_path_row(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| {
        t.path
            .get(n(args, 0) as usize)
            .map_or(-1, |&(_, r)| r as i32)
    }) as f64)
}

/// `iso_add_unit(col, row, team, speed, move, jump, hp)` — register a unit on the board (claiming the
/// cell) and return its id. Ids count up from 0 in registration order.
pub fn iso_add_unit(args: &[Value]) -> Value {
    let id = with_iso_grid(|t| {
        t.add_unit(
            n(args, 0) as i32,
            n(args, 1) as i32,
            n(args, 2) as u8,
            n(args, 3) as u16,
            n(args, 4) as u8,
            n(args, 5) as u8,
            n(args, 6) as i16,
        )
    });
    Value::Number(id as f64)
}

/// `iso_clear_units()` — drop every unit, keeping the board.
///
/// ⚠️ Before this existed the ONLY way to empty the unit table was `iso_load`, which reloads the
/// whole board. A game that fights two battles on the SAME board therefore could not clear it:
/// `isoUseBoard` returns early when the board is already current, so `iso_load` never runs, the old
/// units stay registered, and the new battle's ids are appended after them. The symptoms are not
/// obviously a leak — the turn queue offers turns to units from a fight that ended, and the tish
/// side's own count disagrees with the engine's.
pub fn iso_clear_units(_args: &[Value]) -> Value {
    with_iso_grid(|t| {
        t.units.clear();
        for c in t.cells.iter_mut() {
            c.occupant = -1;
        }
    });
    Value::Null
}

/// `iso_unit_count()` — number of registered units.
pub fn iso_unit_count(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.units.len()) as f64)
}

/// Per-unit getters: `iso_unit_col/row/team/hp/maxhp/move/jump/speed(id)`, `iso_unit_alive(id)`.
pub fn iso_unit_col(args: &[Value]) -> Value {
    unit_field(args, |u| u.col)
}

pub fn iso_unit_row(args: &[Value]) -> Value {
    unit_field(args, |u| u.row)
}

pub fn iso_unit_team(args: &[Value]) -> Value {
    unit_field(args, |u| u.team as i32)
}

pub fn iso_unit_hp(args: &[Value]) -> Value {
    unit_field(args, |u| u.hp as i32)
}

pub fn iso_unit_maxhp(args: &[Value]) -> Value {
    unit_field(args, |u| u.max_hp as i32)
}

pub fn iso_unit_move(args: &[Value]) -> Value {
    unit_field(args, |u| u.mov as i32)
}

pub fn iso_unit_jump(args: &[Value]) -> Value {
    unit_field(args, |u| u.jump as i32)
}

pub fn iso_unit_speed(args: &[Value]) -> Value {
    unit_field(args, |u| u.speed as i32)
}

pub fn iso_unit_alive(args: &[Value]) -> Value {
    // Number (1/0), not Bool, so tish `iso_unit_alive(id) > 0` comparisons work on GBA.
    Value::Number(with_iso_grid(|t| {
        t.units
            .get(n(args, 0) as usize)
            .map_or(0, |u| u.alive as i32)
    }) as f64)
}

/// `iso_unit_move_range(id)` — flood-fill the reachable tiles for a unit using its own Move/Jump and
/// movement type (from its current cell). Then query with `iso_in_range` / `iso_range_*` / `iso_path`.
pub fn iso_unit_move_range(args: &[Value]) -> Value {
    with_iso_grid(|t| {
        if let Some(u) = t.units.get(n(args, 0) as usize).copied() {
            t.move_range(
                u.col,
                u.row,
                u.mov as i32,
                u.jump as i32,
                u.flying,
                u.team as i32,
            );
        }
    });
    Value::Number(with_iso_grid(|t| t.reach.len()) as f64)
}

/// `iso_set_terrain_cost(tile, cost)` — move points needed to ENTER a cell of terrain id `tile`.
///
/// The engine deliberately does not know that id 33 is water. It owns the search; the game owns what
/// its tiles mean, so cost is pushed in from the example's terrain table. Defaults to 1 everywhere,
/// so a game that never calls this behaves exactly as it did before costs existed.
///
/// Clamped to ≥ 1: a free tile would break the search's assumption that distance only ever increases
/// as it sweeps outward, and the cheapest way to say "this tile is free to cross" is a flier.
pub fn iso_set_terrain_cost(args: &[Value]) -> Value {
    let tile = n(args, 0) as usize;
    let cost = (n(args, 1) as i32).max(1).min(255) as u8;
    with_iso_grid(|t| {
        if tile < t.cost.len() {
            t.cost[tile] = cost;
        }
    });
    Value::Null
}

/// `iso_knockback(id, from_col, from_row)` — shove a unit one tile directly away from (from_col,
/// from_row). Returns 1 if it moved, 0 if something stopped it.
///
/// Height is deliberately NOT a reason to refuse: being shoved off a ledge is the interesting case,
/// and a `jump` check here would quietly make knockback do nothing near exactly the terrain that
/// makes it worth having. The board edge, a solid cell and another unit all still stop it — a unit
/// backed against any of those takes the hit where it stands.
///
/// The engine moves the body and reports that it did. What a fall COSTS is the game's to decide, so
/// the caller compares the heights either side and applies its own damage; the same split as the
/// damage formula.
pub fn iso_knockback(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let (sc, sr) = (n(args, 1) as i32, n(args, 2) as i32);
    let moved = with_iso_grid(|t| match t.knock_dest(id, sc, sr) {
        Some((nc, nr, _)) => {
            t.unit_set_pos(id, nc, nr);
            true
        }
        None => false,
    });
    Value::Number(moved as i32 as f64)
}

/// `iso_knock_drop(id, from_col, from_row)` — how far the unit WOULD fall if shoved from there, or 0
/// if the shove is blocked or lands level or uphill.
///
/// This exists so an AI can price a shove before committing to one. It deliberately answers with the
/// drop rather than with the destination: the destination would make the caller re-derive the rule
/// for which direction a shove goes and what stops it, and the moment two copies of that rule exist
/// they start to disagree. Blocked and level both answer 0 because they are worth the same to anyone
/// asking — no fall, no damage.
pub fn iso_knock_drop(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let (sc, sr) = (n(args, 1) as i32, n(args, 2) as i32);
    Value::Number(
        with_iso_grid(|t| t.knock_dest(id, sc, sr).map_or(0, |(_, _, d)| d.max(0))) as f64,
    )
}

/// `iso_set_zoc(on)` — turn zone of control on or off for every subsequent move-range query.
///
/// With it on, stepping into a tile adjacent to a living enemy ends the move — the tile is reachable,
/// nothing through it is. That turns a defender from an obstacle you route around into one you have
/// to deal with, which is the entire point of a front line. Off by default so this changes nothing
/// for a game that does not ask for it.
pub fn iso_set_zoc(args: &[Value]) -> Value {
    let on = n(args, 0) != 0.0;
    with_iso_grid(|t| t.zoc_on = on);
    Value::Null
}

/// `iso_revive(id, hp)` — bring a KO'd unit back on the cell it fell on, at `hp` (clamped to its
/// maximum, and to at least 1). Returns 1 if it stood up, 0 if it was already alive or something has
/// since walked onto its cell.
///
/// The occupancy check is the whole reason this cannot live in the example: death RELEASES the cell,
/// so a body is not a wall, and by the time anyone reaches it with a Phoenix Down somebody may be
/// standing there. The engine owns that map, so it is the only place that can answer honestly —
/// and returning 0 rather than reviving onto an occupied cell means the caller can decline to spend
/// the item instead of quietly creating two units on one tile.
pub fn iso_revive(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let hp = n(args, 1) as i16;
    Value::Number(with_iso_grid(|t| {
        let (col, row, max_hp) = match t.units.get(id as usize) {
            Some(u) if !u.alive => (u.col, u.row, u.max_hp),
            _ => return 0,
        };
        match t.idx(col, row) {
            Some(i) if t.cells[i].occupant < 0 => t.cells[i].occupant = id,
            _ => return 0,
        }
        if let Some(u) = t.units.get_mut(id as usize) {
            u.alive = true;
            u.hp = hp.clamp(1, max_hp);
        }
        1
    }) as f64)
}

/// `iso_turn_end(id, moved, acted)` — charge the rest of the turn's counter cost, once the game knows
/// what the unit actually did with it. The base cost was taken when the turn was handed out.
///
/// The split exists so that forgetting to call this cannot wedge the queue: a game that never calls
/// it still has every unit paying 500 a turn and the order still advances, it just loses the tempo
/// rule. Making `turn_next` charge nothing and rely on this would mean one missed call returns the
/// same unit forever.
pub fn iso_turn_end(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    let moved = n(args, 1) != 0.0;
    let acted = n(args, 2) != 0.0;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id) {
            let mut cost = 0;
            if moved {
                cost += COST_MOVE;
            }
            if acted {
                cost += COST_ACTION;
            }
            u.ct = (u.ct - cost).max(0);
        }
    });
    Value::Null
}

/// `iso_unit_ct(id)` — the unit's current turn counter, for a UI that wants to show who is next.
pub fn iso_unit_ct(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    Value::Number(with_iso_grid(|t| t.units.get(id).map_or(0, |u| u.ct)) as f64)
}

/// `iso_unit_set_speed_scale(id, percent)` — scale how fast a unit's turn counter fills. 100 is
/// normal; a haste/slow pair would typically be 200 and 50.
///
/// The engine owns the turn queue and therefore owns this; which status sets it, for how long, and
/// what the numbers are stay the game's. It scales ACCRUAL and not the Speed stat, so the AI's threat
/// tables and the Status screen go on reading an unmodified stat block, and clearing the status is
/// just setting it back to 100 — there is no original value to remember and no way to double-apply.
///
/// Clamped to 1..=1000: a scale of 0 would stop the unit accruing at all, which is not a slow but a
/// removal, and would leave `turn_next` hunting for a tick count that never arrives.
pub fn iso_unit_set_speed_scale(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    let pct = (n(args, 1) as i32).clamp(1, 1000) as u16;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id) {
            u.speed_scale = pct;
        }
    });
    Value::Null
}

/// `iso_unit_set_flying(id, flying)` — a flier pays 1 move point per tile and ignores height deltas.
///
/// It is NOT a bigger move budget: it changes which tiles the budget can be spent on, so a flier
/// crosses a ford or a cliff that would cost a walker two points or stop it outright, and is no
/// faster than anyone else over open ground. Solid cells and occupied cells still block it.
pub fn iso_unit_set_flying(args: &[Value]) -> Value {
    let id = n(args, 0) as usize;
    let f = n(args, 1) != 0.0;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id) {
            u.flying = f;
        }
    });
    Value::Null
}

/// `iso_unit_set_pos(id, col, row)` — move a unit (updates board occupancy).
pub fn iso_unit_set_pos(args: &[Value]) -> Value {
    with_iso_grid(|t| t.unit_set_pos(n(args, 0) as i32, n(args, 1) as i32, n(args, 2) as i32));
    Value::Null
}

/// `iso_damage(id, amount)` — subtract HP; a unit at ≤0 HP dies (alive=false, cell freed). Returns
/// remaining HP.
pub fn iso_damage(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let amt = n(args, 1) as i16;
    with_iso_grid(|t| {
        let (dead, c, r) = match t.units.get_mut(id as usize) {
            Some(u) if u.alive => {
                u.hp -= amt;
                if u.hp <= 0 {
                    u.hp = 0;
                    u.alive = false;
                }
                (!u.alive, u.col, u.row)
            }
            _ => (false, 0, 0),
        };
        if dead {
            if let Some(i) = t.idx(c, r) {
                if t.cells[i].occupant == id {
                    t.cells[i].occupant = -1;
                }
            }
        }
    });
    unit_field(args, |u| u.hp as i32)
}

/// `iso_heal(id, amount)` — restore HP to a living unit, capped at its `max_hp`. Returns the unit's
/// new HP (0 if the unit is dead/unknown — dead units cannot be healed here). For support classes.
pub fn iso_heal(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let amt = n(args, 1) as i16;
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id as usize) {
            if u.alive {
                u.hp = (u.hp + amt).min(u.max_hp);
            }
        }
    });
    unit_field(args, |u| if u.alive { u.hp as i32 } else { 0 })
}

/// `iso_unit_set_maxhp(id, v)` — set a unit's HP ceiling, keeping current HP in range. Returns the
/// new maximum (0 for an unknown unit).
///
/// ⚠️ This exists because a level-growth package documented a rule it could not implement: "what
/// a level does NOT raise is HP … there is no setter for the maximum, so growing it would mean a
/// Rust change." Levelling normally raises HP, so the engine was quietly deciding a game-design
/// question. A missing setter is not a neutral omission — it becomes a rule.
///
/// Raising the ceiling raises current HP by the same amount rather than leaving the unit wounded: a
/// level-up mid-battle that hands you a bigger empty bar reads as a penalty. Lowering it (a curse, a
/// job change) clamps current HP down instead.
pub fn iso_unit_set_maxhp(args: &[Value]) -> Value {
    let id = n(args, 0) as i32;
    let v = (n(args, 1) as i16).max(1);
    with_iso_grid(|t| {
        if let Some(u) = t.units.get_mut(id as usize) {
            let delta = v - u.max_hp;
            u.max_hp = v;
            if delta > 0 {
                u.hp += delta;
            }
            if u.hp > u.max_hp {
                u.hp = u.max_hp;
            }
        }
    });
    unit_field(args, |u| u.max_hp as i32)
}

/// `iso_turn_next()` — advance the speed-based turn queue and return the id of the unit whose turn it
/// is (or -1 if none are alive).
pub fn iso_turn_next(_args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.turn_next()) as f64)
}

/// `iso_adjacent_enemy(unitId)` — a living enemy on one of the unit's 4 neighbouring tiles (in
/// range of a melee attack), or -1 if none. Used for attack targeting and AI.
pub fn iso_adjacent_enemy(args: &[Value]) -> Value {
    Value::Number(with_iso_grid(|t| t.adjacent_enemy(n(args, 0) as i32)) as f64)
}

pub fn iso_stack_count_typed(p0: i32, p1: i32, p2: i32) -> i32 {
    match iso_stack_count(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_stack_elev_typed(p0: i32, p1: i32, p2: i32, p3: i32) -> i32 {
    match iso_stack_elev(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_stack_tile_typed(p0: i32, p1: i32, p2: i32, p3: i32) -> i32 {
    match iso_stack_tile(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_load_typed(p0: i32) {
    iso_load(&[Value::Number(p0 as f64)]);
}

pub fn iso_board_bg_typed(p0: i32) -> i32 {
    match iso_board_bg(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_board_cw_typed(p0: i32) -> i32 {
    match iso_board_cw(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}

pub fn iso_board_ch_typed(p0: i32) -> i32 {
    match iso_board_ch(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}

pub fn iso_board_mapw_typed(p0: i32) -> i32 {
    match iso_board_mapw(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}

pub fn iso_board_maph_typed(p0: i32) -> i32 {
    match iso_board_maph(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 512,
    }
}

pub fn iso_board_fg_typed(p0: i32) -> i32 {
    match iso_board_fg(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => -1,
    }
}

pub fn iso_board_ox_typed(p0: i32) -> i32 {
    match iso_board_ox(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_board_oy_typed(p0: i32) -> i32 {
    match iso_board_oy(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_board_lift_typed(p0: i32) -> i32 {
    match iso_board_lift(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_w_typed() -> i32 {
    match iso_w(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_h_typed() -> i32 {
    match iso_h(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_spawn_count_typed(p0: i32) -> i32 {
    match iso_spawn_count(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_spawn_col_typed(p0: i32, p1: i32) -> i32 {
    match iso_spawn_col(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_spawn_row_typed(p0: i32, p1: i32) -> i32 {
    match iso_spawn_row(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_spawn_cls_typed(p0: i32, p1: i32) -> i32 {
    match iso_spawn_cls(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_spawn_team_typed(p0: i32, p1: i32) -> i32 {
    match iso_spawn_team(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_init_typed(p0: i32, p1: i32) {
    iso_init(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}

pub fn iso_set_cell_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32) {
    iso_set_cell(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
    ]);
}

pub fn iso_height_typed(p0: i32, p1: i32) -> i32 {
    match iso_height(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_tile_typed(p0: i32, p1: i32) -> i32 {
    match iso_tile(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_walkable_typed(p0: i32, p1: i32) -> i32 {
    match iso_walkable(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Bool(b) => b as i32,
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_set_occupant_typed(p0: i32, p1: i32, p2: i32) {
    iso_set_occupant(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}

pub fn iso_occupant_typed(p0: i32, p1: i32) -> i32 {
    match iso_occupant(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_move_range_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32) -> i32 {
    match iso_move_range(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
        Value::Number(p5 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_move_cost_typed(p0: i32, p1: i32) -> i32 {
    match iso_move_cost(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_in_range_typed(p0: i32, p1: i32) -> i32 {
    match iso_in_range(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_range_count_typed() -> i32 {
    match iso_range_count(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_range_col_typed(p0: i32) -> i32 {
    match iso_range_col(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_range_row_typed(p0: i32) -> i32 {
    match iso_range_row(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_path_typed(p0: i32, p1: i32) -> i32 {
    match iso_path(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_path_len_typed() -> i32 {
    match iso_path_len(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_path_col_typed(p0: i32) -> i32 {
    match iso_path_col(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_path_row_typed(p0: i32) -> i32 {
    match iso_path_row(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_add_unit_typed(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32) -> i32 {
    match iso_add_unit(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
        Value::Number(p3 as f64),
        Value::Number(p4 as f64),
        Value::Number(p5 as f64),
        Value::Number(p6 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_clear_units_typed() {
    iso_clear_units(&[]);
}

pub fn iso_unit_count_typed() -> i32 {
    match iso_unit_count(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_col_typed(p0: i32) -> i32 {
    match iso_unit_col(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_row_typed(p0: i32) -> i32 {
    match iso_unit_row(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_team_typed(p0: i32) -> i32 {
    match iso_unit_team(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_hp_typed(p0: i32) -> i32 {
    match iso_unit_hp(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_maxhp_typed(p0: i32) -> i32 {
    match iso_unit_maxhp(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_move_typed(p0: i32) -> i32 {
    match iso_unit_move(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_jump_typed(p0: i32) -> i32 {
    match iso_unit_jump(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_speed_typed(p0: i32) -> i32 {
    match iso_unit_speed(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_alive_typed(p0: i32) -> i32 {
    match iso_unit_alive(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_move_range_typed(p0: i32) -> i32 {
    match iso_unit_move_range(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_set_terrain_cost_typed(p0: i32, p1: i32) {
    iso_set_terrain_cost(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}

pub fn iso_knockback_typed(p0: i32, p1: i32, p2: i32) -> i32 {
    match iso_knockback(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_knock_drop_typed(p0: i32, p1: i32, p2: i32) -> i32 {
    match iso_knock_drop(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_set_zoc_typed(p0: i32) {
    iso_set_zoc(&[Value::Number(p0 as f64)]);
}

pub fn iso_revive_typed(p0: i32, p1: i32) -> i32 {
    match iso_revive(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_turn_end_typed(p0: i32, p1: i32, p2: i32) {
    iso_turn_end(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}

pub fn iso_unit_ct_typed(p0: i32) -> i32 {
    match iso_unit_ct(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_set_speed_scale_typed(p0: i32, p1: i32) {
    iso_unit_set_speed_scale(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}

pub fn iso_unit_set_flying_typed(p0: i32, p1: i32) {
    iso_unit_set_flying(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]);
}

pub fn iso_unit_set_pos_typed(p0: i32, p1: i32, p2: i32) {
    iso_unit_set_pos(&[
        Value::Number(p0 as f64),
        Value::Number(p1 as f64),
        Value::Number(p2 as f64),
    ]);
}

pub fn iso_damage_typed(p0: i32, p1: i32) -> i32 {
    match iso_damage(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_heal_typed(p0: i32, p1: i32) -> i32 {
    match iso_heal(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_unit_set_maxhp_typed(p0: i32, p1: i32) -> i32 {
    match iso_unit_set_maxhp(&[Value::Number(p0 as f64), Value::Number(p1 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_turn_next_typed() -> i32 {
    match iso_turn_next(&[]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}

pub fn iso_adjacent_enemy_typed(p0: i32) -> i32 {
    match iso_adjacent_enemy(&[Value::Number(p0 as f64)]) {
        Value::Number(v) => v as i32,
        _ => 0,
    }
}
