//! Kart racing: physics, surfaces, laps and opponent AI, all resolved natively.
//!
//! The tish game says what the player is *asking* for — throttle, steering, drift — and reads back
//! where everyone ended up. Everything between those two is here.
//!
//! ⚠️ WHY THIS IS NATIVE AND NOT A tish PACKAGE. Every tish function call is a boxed closure costing
//! roughly 150 ticks, and a vblank is 4389. Four karts, each needing a heading rotation, a grip
//! solve, a surface lookup and a checkpoint test, is somewhere north of a hundred float operations a
//! frame — and this CPU has no FPU, so each one is a software routine. The same shape has been
//! measured twice in this repo already: projecting four billboards in tish took a frame from 4589
//! ticks to 8611, and a per-icon divide in the rhythm lane cost twelve frames a second. Doing it in
//! tish and porting later would mean writing it twice.
//!
//! Everything below is fixed point (`Num<i32, 8>`, so 256 = 1.0). No `f64` appears anywhere in the
//! per-frame path, and there is no `sqrt`: the model tracks speed ALONG the heading and slide ACROSS
//! it as two scalars, which is both cheaper and a better description of how a kart actually drifts.

use agb::fixnum::Num;
use alloc::vec::Vec;
use core::cell::RefCell;
use tishlang_runtime_gba::{SingleCore, Value};

use crate::num;

type Fx = Num<i32, 8>;

fn fx(v: f64) -> Fx {
    Fx::from_raw((v * 256.0) as i32)
}

// ── Surfaces ─────────────────────────────────────────────────────────────────
// Kept in step with SURF_* in packages/kart.tish and with gen_kart_circuit.py.
const SURF_GRASS: i32 = 0;
const SURF_ROAD: i32 = 1;
const SURF_KERB: i32 = 2;
const SURF_BOOST: i32 = 3;
const SURF_FINISH: i32 = 4;

// ── Events, reported per kart per frame (a bitmask) ──────────────────────────
pub const EV_LAP: i32 = 1;
pub const EV_BOOST_PAD: i32 = 2;
pub const EV_MINI_TURBO: i32 = 4;
pub const EV_FINISH: i32 = 8;
pub const EV_OFFROAD: i32 = 16;
pub const EV_PICKUP: i32 = 32;
pub const EV_HIT: i32 = 64;

// ── Items ────────────────────────────────────────────────────────────────────
// Kept in step with ITEM_* in packages/kart.tish and with the frame order of items.png.
pub const ITEM_NONE: i32 = 0;
pub const ITEM_BOOST: i32 = 1;
pub const ITEM_SHELL: i32 = 2;
pub const ITEM_BANANA: i32 = 3;

const BOX_RESPAWN: i32 = 300; // frames a collected box stays gone
const BOX_R2: i32 = 13 * 13; // pickup radius, squared
const HAZARD_R2: i32 = 11 * 11; // how close a shell or banana has to be to bite
const SHELL_SPEED: f64 = 4.2;
const SHELL_TTL: i32 = 240;
const BANANA_TTL: i32 = 1800;
const SPIN_FRAMES: i32 = 46;
const OWNER_GRACE: i32 = 20; // frames your own shell cannot hit you
const ITEM_BOOST_FRAMES: i32 = 80;
const MAX_HAZARDS: usize = 8;

// ── Handling. These numbers ARE the game feel; they are grouped so they can be
//    read as a set rather than hunted for individually. ─────────────────────────
const ACCEL: f64 = 0.055; // px/frame² on tarmac
const BOOST_ACCEL: f64 = 0.170;
const BRAKE: f64 = 0.090;
const DRAG: f64 = 0.020; // fraction of forward speed shed per frame
const GRASS_DRAG: f64 = 0.090; // the off-road penalty, on top of a lower top speed
const TOP_ROAD: f64 = 2.55;
const TOP_KERB: f64 = 2.20;
const TOP_GRASS: f64 = 1.05;
const TOP_BOOST: f64 = 4.00;
const REVERSE_TOP: f64 = 0.90;
const GRIP: f64 = 0.80; // lateral velocity retained per frame
const DRIFT_GRIP: f64 = 0.95; // …while drifting, so the back end steps out
const SLIDE_GAIN: f64 = 0.115; // how much of a drifting turn becomes sideways travel
                               // ⚠️ Sized to the CIRCUIT, not picked by feel. To hold a corner of radius `r` at speed `v` a kart
                               // must turn at `v/r` radians per frame; this track's tightest corner is about 60px and top speed is
                               // 2.55px/frame, which is 2.4 degrees — call it 2/256ths of a turn. The first value here was 4, i.e.
                               // twice what the tightest corner needs, and the result was a kart that left the road the instant you
                               // touched the d-pad and a drift that could never be held long enough to charge anything.
const STEER_MAX: i32 = 2; // 1/256ths of a turn per frame
const DRIFT_STEER: i32 = 3;
const SPIN_STEER: i32 = 11; // while spun out
                            // ⚠️ These are ANGLES in disguise. A drift held for N frames rotates the kart N * DRIFT_STEER/256 of
                            // a turn, so at 3/256 a 34-frame charge is 143 degrees — tighter than any corner on this circuit,
                            // which meant no driver, human or AI, could ever earn a mini-turbo while staying on the road. Read
                            // them as "the small turbo wants a 76-degree corner, the big one a 143-degree hairpin".
const MINI_MIN: i32 = 18; // frames of drift for the small turbo
const MINI_BIG: i32 = 34; // …and the big one
const BOOST_PAD_FRAMES: i32 = 55;
const DRIFT_MIN_SPEED: f64 = 1.10;
/// Frames a drift survives a momentary loss of steering or speed before it actually breaks.
///
/// ⚠️ Without this, a charge never builds. Steering wobbles — the AI corrects around its line, and a
/// player feathers the d-pad — and every wobble was ending the drift and resetting the meter. The
/// demo driver's charge peaked at FOUR frames against a threshold of eighteen, so no driver, human or
/// AI, ever earned a mini-turbo. Letting go of the drift button still ends it instantly; this only
/// forgives the wobble.
const DRIFT_GRACE: i32 = 12;

#[derive(Clone, Copy)]
struct Kart {
    x: Fx,
    z: Fx,
    yaw: i32,  // 1/256ths of a turn, measured from +z toward +x (the renderer's convention)
    speed: Fx, // along the heading
    slide: Fx, // across it; this is what a drift looks like
    drifting: i32,
    drift_dir: i32,
    charge: i32, // frames of held drift, which become a mini-turbo
    boost: i32,  // frames of boost left
    spin: i32,   // frames of being out of control
    grace: i32,  // frames a wobbling drift is still allowed to live
    lap: i32,
    next_cp: i32,
    finished: i32,
    finish_order: i32,
    surface: i32,
    events: i32,
    ai: i32,
    skill: i32,  // 0..100: line accuracy and corner confidence
    rubber: i32, // 0..100: how hard the field is dragged back toward the player
    wp: i32,
    in_throttle: i32,
    in_steer: i32,
    in_drift: i32,
    item: i32,
    ai_use: i32, // frames until an AI fires what it is holding
}

impl Kart {
    fn new(x: i32, z: i32, yaw: i32) -> Self {
        Kart {
            x: Fx::new(x),
            z: Fx::new(z),
            yaw: yaw.rem_euclid(256),
            speed: Fx::new(0),
            slide: Fx::new(0),
            drifting: 0,
            drift_dir: 0,
            charge: 0,
            boost: 0,
            spin: 0,
            grace: 0,
            lap: 0,
            next_cp: 0,
            finished: 0,
            finish_order: 0,
            surface: SURF_ROAD,
            events: 0,
            ai: 0,
            skill: 70,
            rubber: 50,
            wp: 0,
            in_throttle: 0,
            in_steer: 0,
            in_drift: 0,
            item: ITEM_NONE,
            ai_use: 0,
        }
    }
}

/// A banana on the road or a shell in flight. One type, because the only thing that differs is
/// whether it moves — and a shell that has stopped is, for every purpose here, a banana.
#[derive(Clone, Copy)]
struct Hazard {
    kind: i32,
    x: Fx,
    z: Fx,
    vx: Fx,
    vz: Fx,
    ttl: i32,
    owner: i32,
    grace: i32,
}

struct Race {
    karts: Vec<Kart>,
    mask: Vec<i32>,
    cells: i32,
    cell_px: i32,
    wpx: Vec<i32>,
    wpz: Vec<i32>,
    cpx: Vec<i32>,
    cpz: Vec<i32>,
    cp_r2: i32,
    laps: i32,
    finished_count: i32,
    running: i32,
    boxx: Vec<i32>,
    boxz: Vec<i32>,
    box_timer: Vec<i32>,
    hazards: Vec<Hazard>,
    /// Deterministic, so a replay and a test run see the same items. `Math.random()` on this
    /// hardware is unseeded and identical every boot anyway; this is at least honest about it.
    rng: u32,
}

impl Race {
    const fn new() -> Self {
        Race {
            karts: Vec::new(),
            mask: Vec::new(),
            cells: 0,
            cell_px: 8,
            wpx: Vec::new(),
            wpz: Vec::new(),
            cpx: Vec::new(),
            cpz: Vec::new(),
            cp_r2: 26 * 26,
            laps: 3,
            finished_count: 0,
            running: 0,
            boxx: Vec::new(),
            boxz: Vec::new(),
            box_timer: Vec::new(),
            hazards: Vec::new(),
            rng: 0x2545_F491,
        }
    }

    /// What is under this point. Wraps, because the affine layer does.
    fn surface_at(&self, x: i32, z: i32) -> i32 {
        if self.cells <= 0 || self.mask.is_empty() {
            return SURF_ROAD;
        }
        let cx = x.div_euclid(self.cell_px).rem_euclid(self.cells);
        let cz = z.div_euclid(self.cell_px).rem_euclid(self.cells);
        let idx = (cz * self.cells + cx) as usize;
        match self.mask.get(idx >> 3) {
            Some(w) => (w >> ((idx & 7) * 4)) & 15,
            None => SURF_ROAD,
        }
    }

    /// xorshift32 — a few instructions, and the same sequence every run.
    fn next_rand(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn spawn_hazard(&mut self, h: Hazard) {
        if self.hazards.len() < MAX_HAZARDS {
            self.hazards.push(h);
        }
    }

    /// How far around the course a kart is, as one comparable number.
    fn progress(&self, k: &Kart) -> i32 {
        let ncp = self.cpx.len().max(1) as i32;
        if k.finished != 0 {
            // Finishers rank by the order they crossed, ahead of anyone still driving.
            return 1_000_000 - k.finish_order;
        }
        k.lap * ncp + k.next_cp
    }
}

static RACE: SingleCore<RefCell<Race>> = SingleCore::new(RefCell::new(Race::new()));

fn with_race<T>(f: impl FnOnce(&mut Race) -> T) -> T {
    RACE.with(|r| f(&mut r.borrow_mut()))
}

fn read_i32s(v: Option<&Value>) -> Vec<i32> {
    let mut out = Vec::new();
    if let Some(Value::Array(a)) = v {
        for item in a.borrow().iter() {
            if let Value::Number(n) = item {
                out.push(*n as i32);
            }
        }
    }
    out
}

// ── Setup ────────────────────────────────────────────────────────────────────

/// `kart_track(mask, cells, cellPx)` — install the surface map.
///
/// `mask` is packed four bits per cell, eight cells per entry, row-major — exactly what
/// `scripts/gen_kart_circuit.py` emits, and the reason the table is 2 KB instead of 16.
pub fn kart_track(args: &[Value]) -> Value {
    let mask = read_i32s(args.first());
    let cells = num(args, 1) as i32;
    let cell_px = (num(args, 2) as i32).max(1);
    with_race(|r| {
        r.mask = mask;
        r.cells = cells;
        r.cell_px = cell_px;
    });
    Value::Null
}

/// `kart_waypoints(xs, zs)` — the racing line the AI follows.
pub fn kart_waypoints(args: &[Value]) -> Value {
    let xs = read_i32s(args.first());
    let zs = read_i32s(args.get(1));
    with_race(|r| {
        r.wpx = xs;
        r.wpz = zs;
    });
    Value::Null
}

/// `kart_checkpoints(xs, zs, radius, laps)` — the ordered gates that validate a lap.
///
/// A lap counts only when every gate has been passed IN SEQUENCE. Without that, a kart can turn
/// around at the line and farm laps by crossing it repeatedly, which is the first thing a tester
/// tries and the last thing a lap counter written as "crossed the line" survives.
pub fn kart_checkpoints(args: &[Value]) -> Value {
    let xs = read_i32s(args.first());
    let zs = read_i32s(args.get(1));
    let radius = (num(args, 2) as i32).max(4);
    let laps = (num(args, 3) as i32).max(1);
    with_race(|r| {
        r.cpx = xs;
        r.cpz = zs;
        r.cp_r2 = radius * radius;
        r.laps = laps;
    });
    Value::Null
}

/// `kart_item_boxes(xs, zs)` — where the item boxes sit. Collected boxes come back on their own.
pub fn kart_item_boxes(args: &[Value]) -> Value {
    let xs = read_i32s(args.first());
    let zs = read_i32s(args.get(1));
    with_race(|r| {
        r.box_timer = alloc::vec![0; xs.len()];
        r.boxx = xs;
        r.boxz = zs;
    });
    Value::Null
}

/// Fire whatever racer `i` is holding. Shared by the player's button and the opponents' timer, so
/// the two cannot end up with different rules about what an item does.
///
/// A boost is instant. A shell leaves along the kart's heading and spins the first racer it touches.
/// A banana is dropped BEHIND, which is the whole point of holding one while leading.
fn fire_item(r: &mut Race, i: usize) {
    let k = match r.karts.get(i) {
        Some(k) => *k,
        None => return,
    };
    match k.item {
        ITEM_BOOST => {
            if let Some(kk) = r.karts.get_mut(i) {
                kk.boost = kk.boost.max(ITEM_BOOST_FRAMES);
            }
        }
        ITEM_SHELL => {
            let (s, c) = sin_cos(k.yaw);
            r.spawn_hazard(Hazard {
                kind: ITEM_SHELL,
                x: k.x + s * 16,
                z: k.z + c * 16,
                vx: s * fx(SHELL_SPEED),
                vz: c * fx(SHELL_SPEED),
                ttl: SHELL_TTL,
                owner: i as i32,
                grace: OWNER_GRACE,
            });
        }
        ITEM_BANANA => {
            let (s, c) = sin_cos(k.yaw);
            r.spawn_hazard(Hazard {
                kind: ITEM_BANANA,
                x: k.x - s * 20,
                z: k.z - c * 20,
                vx: Fx::new(0),
                vz: Fx::new(0),
                ttl: BANANA_TTL,
                owner: i as i32,
                grace: OWNER_GRACE,
            });
        }
        _ => return,
    }
    if let Some(kk) = r.karts.get_mut(i) {
        kk.item = ITEM_NONE;
    }
}

/// `kart_use(i)` — fire whatever racer `i` is holding. Does nothing if empty.
pub fn kart_use(args: &[Value]) -> Value {
    let i = num(args, 0) as usize;
    with_race(|r| fire_item(r, i));
    Value::Null
}

/// `kart_reset()` — remove every kart and unfinish the race. The track stays loaded.
pub fn kart_reset(args: &[Value]) -> Value {
    let _ = args;
    with_race(|r| {
        r.karts.clear();
        r.hazards.clear();
        for t in r.box_timer.iter_mut() {
            *t = 0;
        }
        r.rng = 0x2545_F491;
        r.finished_count = 0;
        r.running = 0;
    });
    Value::Null
}

/// `kart_add(x, z, yaw) -> index` — put a kart on the grid.
pub fn kart_add(args: &[Value]) -> Value {
    let x = num(args, 0) as i32;
    let z = num(args, 1) as i32;
    let yaw = num(args, 2) as i32;
    with_race(|r| {
        r.karts.push(Kart::new(x, z, yaw));
        Value::Number((r.karts.len() - 1) as f64)
    })
}

/// `kart_set_ai(i, skill, rubber)` — make a kart drive itself.
///
/// `skill` 0..100 sets how tightly it holds the racing line and how much it trusts a corner.
/// `rubber` 0..100 is how hard the field is pulled back toward the player: at 0 the opponents drive
/// their own race and a good player laps them, at 100 they are always in the mirror.
pub fn kart_set_ai(args: &[Value]) -> Value {
    let i = num(args, 0) as usize;
    let skill = (num(args, 1) as i32).clamp(0, 100);
    let rubber = (num(args, 2) as i32).clamp(0, 100);
    with_race(|r| {
        if let Some(k) = r.karts.get_mut(i) {
            k.ai = 1;
            k.skill = skill;
            k.rubber = rubber;
        }
    });
    Value::Null
}

/// `kart_start(on)` — 1 to let the field move, 0 to hold it (the countdown).
pub fn kart_start(args: &[Value]) -> Value {
    let on = num(args, 0) as i32;
    with_race(|r| r.running = on);
    Value::Null
}

/// `kart_input(i, throttle, steer, drift)` — one kart's intent this frame.
///
/// `throttle` -1..1, `steer` -1..1, `drift` 0/1. Ignored for AI karts, which decide for themselves.
pub fn kart_input(args: &[Value]) -> Value {
    let i = num(args, 0) as usize;
    let throttle = (num(args, 1) as i32).clamp(-1, 1);
    let steer = (num(args, 2) as i32).clamp(-1, 1);
    let drift = num(args, 3) as i32;
    with_race(|r| {
        if let Some(k) = r.karts.get_mut(i) {
            k.in_throttle = throttle;
            k.in_steer = steer;
            k.in_drift = drift;
        }
    });
    Value::Null
}

/// `kart_bump(i, frames)` — spin a kart out, for an item hit or a heavy collision.
pub fn kart_bump(args: &[Value]) -> Value {
    let i = num(args, 0) as usize;
    let frames = (num(args, 1) as i32).max(1);
    with_race(|r| {
        if let Some(k) = r.karts.get_mut(i) {
            if k.spin <= 0 {
                k.spin = frames;
                k.boost = 0;
                k.charge = 0;
                k.drifting = 0;
            }
        }
    });
    Value::Null
}

// ── The frame ────────────────────────────────────────────────────────────────

fn sin_cos(yaw: i32) -> (Fx, Fx) {
    // agb's table is 256 entries over one turn, and `Num<i32, 8>` raw units ARE 1/256ths of a turn,
    // so a yaw in 1/256ths converts with `from_raw` and no arithmetic at all.
    let a: Fx = Fx::from_raw(yaw.rem_euclid(256));
    (a.sin(), a.cos())
}

/// What the AI wants to do this frame: (throttle, steer).
///
/// Deliberately not a pathfinder. It aims at a point on the racing line a little ahead, eases off
/// when the required steering is large, and drifts when it is larger still — which is what a player
/// does, so the opponents read as racers rather than as things on rails.
fn ai_decide(r: &Race, k: &Kart) -> (i32, i32, i32, i32) {
    if r.wpx.is_empty() {
        return (1, 0, 0, 0);
    }
    let n = r.wpx.len() as i32;
    let wi = (k.wp.rem_euclid(n)) as usize;
    let tx = Fx::new(r.wpx[wi]) - k.x;
    let tz = Fx::new(r.wpz[wi]) - k.z;

    // Steer toward the target without an atan2: the right-hand vector is the derivative of forward
    // with respect to yaw, so the target's component along it IS the direction to turn.
    let (s, c) = sin_cos(k.yaw);
    let along_right = tx * c - tz * s;
    let ahead = tx * s + tz * c;

    let mut steer = 0;
    // A dead zone proportional to skill: a sloppier driver corrects later and wanders more.
    let dead = Fx::new(1) + Fx::from_raw((100 - k.skill) * 12);
    if along_right > dead {
        steer = 1;
    } else if along_right < -dead {
        steer = -1;
    }
    // Something directly behind gives a near-zero lateral component; commit to a full-lock turn
    // rather than dithering.
    if ahead < Fx::new(0) && steer == 0 {
        steer = 1;
    }

    // Ease off when the corner is tight and we are quick — the confidence to keep the throttle down
    // is what `skill` buys.
    let brave = Fx::from_raw(160 + k.skill * 2);
    let throttle = if steer != 0 && k.speed > brave { 0 } else { 1 };
    // Drift only when genuinely committed, or the AI looks twitchy.
    let mut drift =
        if steer != 0 && along_right.to_raw().abs() > 700 && k.speed > fx(DRIFT_MIN_SPEED) {
            1
        } else {
            0
        };

    // ⚠️ COMMIT to a drift once it has started, and keep steering the same way.
    //
    // Re-deciding from scratch every frame is what stopped the opponents ever charging one. The AI
    // corrects around its line, so its steering passes through zero constantly, and every one of
    // those frames read as "let go of the drift": the meter reset from 4 back to 1 and never came
    // near the 18 a mini-turbo needs. Hold until the corner is actually over — either the kart has
    // overshot to the OTHER side of the line, or it is no longer quick enough to drift at all.
    let overshot = along_right.to_raw() * k.drift_dir < -400;
    if k.drifting != 0 && !overshot && k.speed > fx(DRIFT_MIN_SPEED) && k.charge < MINI_BIG + 8 {
        steer = k.drift_dir;
        drift = 1;
    }

    // ⚠️ Advance the target IN PIXELS. This test used to mix fixed-point scales and came out as
    // "within 1.4 px", which no kart ever satisfies — so every opponent orbited its first waypoint
    // for the whole race, permanently mid-corner and therefore permanently throttle-off at the
    // cornering speed cap. It looked like bad driving rather than a units bug.
    let px = tx.to_raw() >> 8;
    let pz = tz.to_raw() >> 8;
    let d2 = px * px + pz * pz;
    let reach = 26 + (100 - k.skill) / 6; // a sloppier driver accepts a looser pass
                                          // Also let go of a target that has ended up BEHIND us: on a tight corner the racing line can
                                          // carry a kart past a waypoint without ever entering its radius, and chasing it backwards is
                                          // how an opponent gets stuck facing the wrong way.
    let advance = if d2 < reach * reach || (ahead < Fx::new(0) && d2 < (reach * 3) * (reach * 3)) {
        1
    } else {
        0
    };
    (throttle, steer, drift, advance)
}

fn top_speed_for(surface: i32) -> Fx {
    match surface {
        SURF_GRASS => fx(TOP_GRASS),
        SURF_KERB => fx(TOP_KERB),
        SURF_ROAD | SURF_BOOST | SURF_FINISH => fx(TOP_ROAD),
        _ => fx(TOP_ROAD),
    }
}

/// `kart_step() -> finishedCount` — advance the whole field by one frame.
///
/// One call does every kart: input or AI, steering, grip, surface, checkpoints, laps. The tish side
/// makes exactly one crossing per frame no matter how many racers there are.
pub fn kart_step(args: &[Value]) -> Value {
    let _ = args;
    with_race(|r| {
        if r.karts.is_empty() {
            return Value::Number(0.0);
        }

        // The player is kart 0 by convention, and is what the rubber band measures against.
        let player_progress = r.progress(&r.karts[0]);

        let n = r.karts.len();
        for i in 0..n {
            let mut k = r.karts[i];
            k.events = 0;

            if k.finished != 0 || r.running == 0 {
                // Still integrate a finished kart so it coasts to a stop rather than freezing mid-air.
                k.speed = k.speed - k.speed * fx(DRAG) * 3;
                if k.speed < Fx::new(0) {
                    k.speed = Fx::new(0);
                }
                let (s, c) = sin_cos(k.yaw);
                k.x += s * k.speed;
                k.z += c * k.speed;
                r.karts[i] = k;
                continue;
            }

            let (mut throttle, mut steer, mut drift) = (k.in_throttle, k.in_steer, k.in_drift);
            if k.ai != 0 {
                let d = ai_decide(r, &k);
                throttle = d.0;
                steer = d.1;
                drift = d.2;
                if d.3 != 0 && !r.wpx.is_empty() {
                    k.wp = (k.wp + 1).rem_euclid(r.wpx.len() as i32);
                }
            }

            // A spin-out overrides everything: no throttle, no steering authority, just rotation.
            if k.spin > 0 {
                k.spin -= 1;
                k.yaw = (k.yaw + SPIN_STEER).rem_euclid(256);
                throttle = 0;
                steer = 0;
                drift = 0;
            }

            let surface = r.surface_at(k.x.to_raw() >> 8, k.z.to_raw() >> 8);
            if surface == SURF_GRASS && k.surface != SURF_GRASS {
                k.events |= EV_OFFROAD;
            }
            if surface == SURF_BOOST && k.boost < BOOST_PAD_FRAMES {
                k.boost = BOOST_PAD_FRAMES;
                k.events |= EV_BOOST_PAD;
            }
            k.surface = surface;

            // ── Item boxes ──
            // Only one item at a time, so a box is worth nothing while you are already holding
            // something — which is what stops a leader hoovering up a row of three.
            if k.item == ITEM_NONE && k.finished == 0 {
                let kx = k.x.to_raw() >> 8;
                let kz = k.z.to_raw() >> 8;
                let mut got = usize::MAX;
                for b in 0..r.boxx.len() {
                    if r.box_timer[b] > 0 {
                        continue;
                    }
                    let dx = kx - r.boxx[b];
                    let dz = kz - r.boxz[b];
                    if dx * dx + dz * dz <= BOX_R2 {
                        got = b;
                        break;
                    }
                }
                if got != usize::MAX {
                    r.box_timer[got] = BOX_RESPAWN;
                    // Weighted: a boost is the common one, a shell is the prize, a banana the
                    // consolation. Nothing here scales with position — rubber-banding lives in the
                    // top-speed nudge, and doing it twice would be invisible and unbalanceable.
                    let roll = r.next_rand() % 100;
                    k.item = if roll < 45 {
                        ITEM_BOOST
                    } else if roll < 75 {
                        ITEM_SHELL
                    } else {
                        ITEM_BANANA
                    };
                    k.events |= EV_PICKUP;
                    if k.ai != 0 {
                        // Opponents hold on for a moment rather than firing the instant they pick
                        // up, so an item reads as a decision rather than a reflex.
                        k.ai_use = 30 + (r.next_rand() % 90) as i32;
                    }
                }
            }
            if k.ai != 0 && k.item != ITEM_NONE && k.ai_use > 0 {
                k.ai_use -= 1;
            }

            // ── Drift and the mini-turbo it charges ──
            let fast_enough = k.speed > fx(DRIFT_MIN_SPEED);
            let holding = drift != 0 && steer != 0 && fast_enough && k.spin <= 0;
            if holding {
                if k.drifting == 0 {
                    k.drifting = 1;
                    k.drift_dir = steer;
                    k.charge = 0;
                }
                k.grace = DRIFT_GRACE;
                if steer == k.drift_dir {
                    k.charge += 1;
                }
            } else if k.drifting != 0 && drift != 0 && k.grace > 0 {
                // Wobble, not a release: hold the drift open and keep the charge.
                k.grace -= 1;
            } else if k.drifting != 0 {
                // Released: the longer it was held, the longer the boost.
                if k.charge >= MINI_BIG {
                    k.boost = k.boost.max(90);
                    k.events |= EV_MINI_TURBO;
                } else if k.charge >= MINI_MIN {
                    k.boost = k.boost.max(45);
                    k.events |= EV_MINI_TURBO;
                }
                k.drifting = 0;
                k.charge = 0;
                k.grace = 0;
            }

            // ── Steering ──
            // Scaled by speed, because a kart that turns as sharply at walking pace as at full
            // throttle feels like a cursor rather than a vehicle.
            if steer != 0 {
                let rate = if k.drifting != 0 {
                    DRIFT_STEER
                } else {
                    STEER_MAX
                };
                let speed_factor = {
                    let s = k.speed.to_raw();
                    // 0 at rest, full by about a third of top speed.
                    (s * 3).min(256)
                };
                let delta = (rate * speed_factor) / 256;
                k.yaw = (k.yaw + steer * delta.max(1)).rem_euclid(256);
                if k.drifting != 0 {
                    // The back steps out: some of the turn becomes lateral travel.
                    k.slide += Fx::from_raw(
                        (steer * ((fx(SLIDE_GAIN).to_raw() * k.speed.to_raw()) >> 8)) as i32,
                    );
                }
            }

            // ── Longitudinal ──
            let boosting = k.boost > 0;
            if boosting {
                k.boost -= 1;
            }
            let mut top = if boosting {
                fx(TOP_BOOST)
            } else {
                top_speed_for(surface)
            };
            let accel = if boosting { fx(BOOST_ACCEL) } else { fx(ACCEL) };

            // ── Rubber band ──
            // ⚠️ Scales the TOP SPEED; it must never be a per-frame subtraction from speed. The first
            // version took a constant off `speed` every frame, and once a leader's gap saturated, that
            // constant exceeded the acceleration — so the AI in front smoothly decelerated to a dead
            // stop in the middle of the circuit. As a ceiling it cannot do that: an opponent still
            // accelerates normally, just to a slightly different maximum.
            if k.ai != 0 && k.rubber > 0 && r.running != 0 {
                let gap = (r.progress(&k) - player_progress).clamp(-8, 8);
                // Behind the player: a little faster. Ahead: a little slower. ±15% at rubber 100.
                let permille = -gap * k.rubber * 150 / 800;
                top += Fx::from_raw(top.to_raw() * permille / 1000);
            }

            if throttle > 0 {
                k.speed += accel;
            } else if throttle < 0 {
                k.speed -= fx(BRAKE);
            }
            // Drag, plus the off-road penalty. Grass is slow twice over — a lower ceiling AND more
            // resistance — because only the second one is felt the instant you touch it.
            let drag = if surface == SURF_GRASS {
                fx(GRASS_DRAG)
            } else {
                fx(DRAG)
            };
            k.speed -= k.speed * drag;
            if k.speed > top {
                k.speed = top;
            }
            let rev = -fx(REVERSE_TOP);
            if k.speed < rev {
                k.speed = rev;
            }

            // ── Lateral: grip pulls the slide out, unless we are drifting ──
            let grip = if k.drifting != 0 {
                fx(DRIFT_GRIP)
            } else {
                fx(GRIP)
            };
            k.slide *= grip;

            // ── Integrate ──
            let (s, c) = sin_cos(k.yaw);
            k.x += s * k.speed + c * k.slide;
            k.z += c * k.speed - s * k.slide;

            // The world wraps, exactly as the affine layer does, so nobody can drive off the map.
            let world = Fx::new(r.cells * r.cell_px);
            if world > Fx::new(0) {
                while k.x < Fx::new(0) {
                    k.x += world;
                }
                while k.x >= world {
                    k.x -= world;
                }
                while k.z < Fx::new(0) {
                    k.z += world;
                }
                while k.z >= world {
                    k.z -= world;
                }
            }

            // ── Checkpoints, and therefore laps ──
            if !r.cpx.is_empty() {
                let ncp = r.cpx.len() as i32;
                let ci = k.next_cp.rem_euclid(ncp) as usize;
                let dx = (k.x.to_raw() >> 8) - r.cpx[ci];
                let dz = (k.z.to_raw() >> 8) - r.cpz[ci];
                if dx * dx + dz * dz <= r.cp_r2 {
                    k.next_cp += 1;
                    if k.next_cp >= ncp {
                        k.next_cp = 0;
                        k.lap += 1;
                        k.events |= EV_LAP;
                        if k.lap >= r.laps {
                            k.finished = 1;
                            r.finished_count += 1;
                            k.finish_order = r.finished_count;
                            k.events |= EV_FINISH;
                        }
                    }
                }
            }

            r.karts[i] = k;
        }

        // ── Opponents fire what they are holding, once their delay is up ──
        // After the movement loop, so every kart is at this frame's position when a shell leaves.
        if r.running != 0 {
            for i in 0..r.karts.len() {
                let k = r.karts[i];
                if k.ai != 0 && k.item != ITEM_NONE && k.ai_use <= 0 && k.finished == 0 {
                    fire_item(r, i);
                }
            }
        }

        // ── Boxes come back ──
        for t in r.box_timer.iter_mut() {
            if *t > 0 {
                *t -= 1;
            }
        }

        // ── Hazards: shells fly, bananas wait, both bite ──
        // Done after every kart has moved so a shell is tested against where they ended up, not
        // against a mix of this frame and last.
        let mut hi = 0;
        while hi < r.hazards.len() {
            let mut h = r.hazards[hi];
            h.ttl -= 1;
            if h.grace > 0 {
                h.grace -= 1;
            }
            h.x += h.vx;
            h.z += h.vz;
            // A shell that leaves the road loses its momentum and becomes an obstacle where it
            // stopped — cheaper than bouncing it off walls this track does not have.
            if h.kind == ITEM_SHELL
                && r.surface_at(h.x.to_raw() >> 8, h.z.to_raw() >> 8) == SURF_GRASS
            {
                h.vx = Fx::new(0);
                h.vz = Fx::new(0);
                h.ttl = h.ttl.min(90);
            }

            let hx = h.x.to_raw() >> 8;
            let hz = h.z.to_raw() >> 8;
            let mut hit = usize::MAX;
            for ki in 0..r.karts.len() {
                if h.grace > 0 && h.owner == ki as i32 {
                    continue;
                }
                let k = &r.karts[ki];
                if k.finished != 0 || k.spin > 0 {
                    continue;
                }
                let dx = (k.x.to_raw() >> 8) - hx;
                let dz = (k.z.to_raw() >> 8) - hz;
                if dx * dx + dz * dz <= HAZARD_R2 {
                    hit = ki;
                    break;
                }
            }
            if hit != usize::MAX {
                if let Some(k) = r.karts.get_mut(hit) {
                    k.spin = SPIN_FRAMES;
                    k.boost = 0;
                    k.charge = 0;
                    k.drifting = 0;
                    k.speed /= 3;
                    k.events |= EV_HIT;
                }
                r.hazards.swap_remove(hi);
                continue;
            }
            if h.ttl <= 0 {
                r.hazards.swap_remove(hi);
                continue;
            }
            r.hazards[hi] = h;
            hi += 1;
        }

        Value::Number(r.finished_count as f64)
    })
}

// ── Presentation ─────────────────────────────────────────────────────────────

/// `kart_draw(bgHandle, bbFirst, sheetNear, sheetFar, farDist, nearSize, farSize)` — present the field.
///
/// Places every kart's billboard, picks its sprite frame from its heading RELATIVE to the camera,
/// and swaps between the near and far sheets by distance. One crossing into native code for the lot.
///
/// ⚠️ Why the tier swap exists at all: the GBA cannot scale a sprite. There is no affine-object
/// wrapper in this engine, so a kart forty pixels away and one four hundred pixels away would be
/// drawn identically — which reads as the far kart being enormous. Two baked sizes is the same
/// answer the classic SNES kart racers used. The swap rebuilds the agb `Object` (new VRAM allocation), so it
/// only fires on an actual crossing, never per frame.
///
/// Billboards must have been registered with `mode7_billboard` in kart order starting at `bbFirst`.
/// Frames are laid out racer-major, eight headings each: `frame = kartIndex * 8 + heading`, heading 0
/// being "driving directly away from the camera".
pub fn kart_draw(args: &[Value]) -> Value {
    let handle = num(args, 0) as usize;
    let bb_first = num(args, 1) as usize;
    let sheet_near = num(args, 2) as i32;
    let sheet_far = num(args, 3) as i32;
    let far_dist = (num(args, 4) as i32).max(1);
    let near_size = (num(args, 5) as i32).max(1);
    let far_size = (num(args, 6) as i32).max(1);

    // The camera, straight from the layer that is about to be drawn with it — so the karts and the
    // floor can never be a frame out of step.
    let cam = crate::with_ctx(|ctx| {
        ctx.affine_bgs
            .get(handle)
            .and_then(|a| a.m7.as_ref())
            .map(|m| (m.cam_x.to_raw() >> 8, m.cam_z.to_raw() >> 8, m.yaw.to_raw()))
    });
    let (camx, camz, camyaw) = match cam {
        Some(c) => c,
        None => return Value::Null,
    };

    let far2 = far_dist * far_dist;
    let shots: Vec<(usize, i32, i32, i32, i32)> = with_race(|r| {
        r.karts
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let kx = k.x.to_raw() >> 8;
                let kz = k.z.to_raw() >> 8;
                // +16 rounds to the nearest eighth rather than truncating, which otherwise biases
                // every kart's sprite one step anticlockwise.
                let rel = (k.yaw - camyaw + 16).rem_euclid(256) / 32;
                let dx = kx - camx;
                let dz = kz - camz;
                (i, kx, kz, rel, dx * dx + dz * dz)
            })
            .collect()
    });

    // ⚠️ ONE `with_ctx` for the whole loop, and nothing inside it may call a native.
    //
    // `native_sprite_set_sheet` and `native_sprite_set_frame` each take the context borrow
    // themselves, so calling them from in here is a re-entrant `RefCell::borrow_mut` — which is not
    // a compile error, it is a panic on the frame the first kart crosses the distance threshold.
    // The sprite work is therefore inlined against the `ctx` already held.
    crate::with_ctx(|ctx| {
        for (i, kx, kz, rel, d2) in shots {
            let (sprite, want, size, frame) = match ctx.billboards.get_mut(bb_first + i) {
                Some(b) => {
                    b.x = Fx::new(kx);
                    b.z = Fx::new(kz);
                    let far = d2 > far2;
                    // The anchor size has to follow the tier. A billboard is anchored bottom-centre
                    // from its declared w/h, so leaving a 16px sprite declared as 32 lifts it half a
                    // sprite off the ground — the kart floats, and only once it is far away, which
                    // reads as a projection bug rather than a bookkeeping one.
                    let size = if far { far_size } else { near_size };
                    b.w = size;
                    b.h = size;
                    (
                        b.sprite,
                        if far { sheet_far } else { sheet_near },
                        size,
                        (i as i32) * 8 + rel,
                    )
                }
                None => continue,
            };
            let _ = size;
            let cur = ctx
                .sprites
                .get(sprite as usize)
                .map(|s| s.sheet)
                .unwrap_or(-1);
            if cur != want {
                // Re-binding allocates a fresh sprite-VRAM object, so it must only happen on an
                // actual tier crossing — never once per frame.
                if let Some(sheet) = tishlang_runtime_gba::gba::asset_sheet(want) {
                    let idx = (frame.max(0) as usize).min(sheet.len().saturating_sub(1));
                    let object = agb::display::object::Object::new(&sheet[idx]);
                    if let Some(s) = ctx.sprites.get_mut(sprite as usize) {
                        s.object = Some(object);
                        s.sheet = want;
                        s.frame = idx as i32;
                    }
                }
            } else if let Some(sheet) = tishlang_runtime_gba::gba::asset_sheet(want) {
                let idx = (frame.max(0) as usize).min(sheet.len().saturating_sub(1));
                if let Some(s) = ctx.sprites.get_mut(sprite as usize) {
                    if s.frame != idx as i32 {
                        s.object = Some(agb::display::object::Object::new(&sheet[idx]));
                        s.frame = idx as i32;
                    }
                }
            }
        }
    });
    Value::Null
}

/// `kart_draw_items(bbFirst, size)` — place the item boxes and every live hazard.
///
/// Billboards must have been registered after the karts: the boxes first, then `kart_hazard_slots()`
/// spare slots. Frame order matches items.png — 0 box · 1 banana · 2 shell. An empty slot is HIDDEN
/// rather than parked off-screen, because a billboard outside the frustum still costs the projection
/// and an OAM entry.
#[allow(clippy::type_complexity)]
pub fn kart_draw_items(args: &[Value]) -> Value {
    let bb_first = num(args, 0) as usize;
    let size = (num(args, 1) as i32).max(1);

    let (boxes, hazards): (Vec<(i32, i32, bool)>, Vec<(i32, i32, i32)>) = with_race(|r| {
        let b = (0..r.boxx.len())
            .map(|i| (r.boxx[i], r.boxz[i], r.box_timer[i] == 0))
            .collect();
        let h = r
            .hazards
            .iter()
            .map(|h| (h.x.to_raw() >> 8, h.z.to_raw() >> 8, h.kind))
            .collect();
        (b, h)
    });

    crate::with_ctx(|ctx| {
        let mut slot = bb_first;
        for (bx, bz, alive) in boxes {
            if let Some(b) = ctx.billboards.get_mut(slot) {
                b.x = Fx::new(bx);
                b.z = Fx::new(bz);
                b.w = size;
                b.h = size;
                b.active = alive;
            }
            slot += 1;
        }
        for i in 0..MAX_HAZARDS {
            let live = hazards.get(i).copied();
            let (sprite, sheet_h) = match ctx.billboards.get_mut(slot) {
                Some(b) => {
                    b.active = live.is_some();
                    if let Some((hx, hz, _)) = live {
                        b.x = Fx::new(hx);
                        b.z = Fx::new(hz);
                        b.w = size;
                        b.h = size;
                    }
                    let sp = b.sprite;
                    (
                        sp,
                        ctx.sprites.get(sp as usize).map(|s| s.sheet).unwrap_or(-1),
                    )
                }
                None => {
                    slot += 1;
                    continue;
                }
            };
            if let Some((_, _, kind)) = live {
                let frame = if kind == ITEM_SHELL { 2usize } else { 1usize };
                if let Some(sheet) = tishlang_runtime_gba::gba::asset_sheet(sheet_h) {
                    let idx = frame.min(sheet.len().saturating_sub(1));
                    if let Some(sp) = ctx.sprites.get_mut(sprite as usize) {
                        if sp.frame != idx as i32 {
                            sp.object = Some(agb::display::object::Object::new(&sheet[idx]));
                            sp.frame = idx as i32;
                        }
                        sp.visible = true;
                    }
                }
            }
            slot += 1;
        }
    });
    Value::Null
}

/// `kart_camera(i, backDist)` — where a chase camera behind kart `i` should stand.
///
/// Returns nothing; call `kart_cam_x/z/yaw` after it. Kept as one computation rather than three so
/// the three answers describe the same instant.
pub fn kart_camera(args: &[Value]) -> Value {
    let i = num(args, 0) as usize;
    let back = num(args, 1) as i32;
    with_race(|r| {
        if let Some(k) = r.karts.get(i) {
            let (s, c) = sin_cos(k.yaw);
            let cx = k.x - s * back;
            let cz = k.z - c * back;
            CAM.with(|v| *v.borrow_mut() = (cx.to_raw(), cz.to_raw(), k.yaw));
        }
    });
    Value::Null
}

static CAM: SingleCore<RefCell<(i32, i32, i32)>> = SingleCore::new(RefCell::new((0, 0, 0)));

pub fn kart_cam_x(_args: &[Value]) -> Value {
    Value::Number(CAM.with(|v| v.borrow().0) as f64 / 256.0)
}
pub fn kart_cam_z(_args: &[Value]) -> Value {
    Value::Number(CAM.with(|v| v.borrow().1) as f64 / 256.0)
}
pub fn kart_cam_yaw(_args: &[Value]) -> Value {
    Value::Number(CAM.with(|v| v.borrow().2) as f64)
}

// ── Read-back ────────────────────────────────────────────────────────────────

fn get<T>(i: usize, f: impl Fn(&Kart) -> T, dflt: T) -> T {
    with_race(|r| r.karts.get(i).map(&f).unwrap_or(dflt))
}

pub fn kart_count(_args: &[Value]) -> Value {
    with_race(|r| Value::Number(r.karts.len() as f64))
}
pub fn kart_x(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.x.to_raw(), 0) as f64 / 256.0)
}
pub fn kart_z(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.z.to_raw(), 0) as f64 / 256.0)
}
pub fn kart_yaw(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.yaw, 0) as f64)
}
/// Speed in hundredths of a pixel per frame — an integer, so the HUD needs no float.
pub fn kart_speed(args: &[Value]) -> Value {
    Value::Number((get(num(args, 0) as usize, |k| k.speed.to_raw(), 0) * 100 / 256) as f64)
}
pub fn kart_lap(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.lap, 0) as f64)
}
pub fn kart_boost(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.boost, 0) as f64)
}
pub fn kart_charge(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.charge, 0) as f64)
}
pub fn kart_surface(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.surface, SURF_ROAD) as f64)
}
pub fn kart_finished(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.finished, 0) as f64)
}
pub fn kart_events(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.events, 0) as f64)
}
pub fn kart_drifting(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.drifting, 0) as f64)
}
/// What racer `i` is holding, as an ITEM_* value.
pub fn kart_item(args: &[Value]) -> Value {
    Value::Number(get(num(args, 0) as usize, |k| k.item, ITEM_NONE) as f64)
}
/// Live shells and bananas on the course — mostly so a test can tell that firing did something.
pub fn kart_hazards(_args: &[Value]) -> Value {
    with_race(|r| Value::Number(r.hazards.len() as f64))
}
/// The number of hazard billboard slots a game must register after its item boxes.
pub fn kart_hazard_slots(_args: &[Value]) -> Value {
    Value::Number(MAX_HAZARDS as f64)
}

/// `kart_rank(i)` — 1 for the leader. Ties break on distance to the next gate, so two karts on the
/// same lap and the same checkpoint still get a stable order.
pub fn kart_rank(args: &[Value]) -> Value {
    let i = num(args, 0) as usize;
    with_race(|r| {
        let me = match r.karts.get(i) {
            Some(k) => *k,
            None => return Value::Number(0.0),
        };
        let my_p = r.progress(&me);
        let my_d = gate_dist2(r, &me);
        let mut ahead = 0;
        for (j, k) in r.karts.iter().enumerate() {
            if j == i {
                continue;
            }
            let p = r.progress(k);
            if p > my_p || (p == my_p && gate_dist2(r, k) < my_d) {
                ahead += 1;
            }
        }
        Value::Number((ahead + 1) as f64)
    })
}

fn gate_dist2(r: &Race, k: &Kart) -> i32 {
    if r.cpx.is_empty() {
        return 0;
    }
    let ci = k.next_cp.rem_euclid(r.cpx.len() as i32) as usize;
    let dx = (k.x.to_raw() >> 8) - r.cpx[ci];
    let dz = (k.z.to_raw() >> 8) - r.cpz[ci];
    dx * dx + dz * dz
}

/// `kart_surface_at(x, z)` — what a point of the world is made of. For placing items and props.
pub fn kart_surface_at(args: &[Value]) -> Value {
    let x = num(args, 0) as i32;
    let z = num(args, 1) as i32;
    with_race(|r| Value::Number(r.surface_at(x, z) as f64))
}
