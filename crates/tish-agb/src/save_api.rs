//! Cartridge save bindings for tish (`save_init` / `save_has` / `save_write` / `save_read_*`).
//!
//! Fixed 3-slot layout on whichever medium `save_media` was built for — 32 KiB battery-backed SRAM
//! (the traditional cart, default) or 64/128 KiB flash (what late-era 128 KiB flash carts ship). mGBA
//! creates a `.sav` beside the ROM either way; the difference is on the bus, not in the file.
//! Deliberately **not** agb's `SaveSlotManager`: that allocator keeps heap Vecs of free sectors for
//! the whole media, and on byte-addressed SRAM the lasting heap cost tips large scene loads
//! (Akari shrine) into OOM. A packed `#[repr(C)]` blob needs no heap after init.

use tishlang_runtime_gba::Value;

use crate::{num, with_ctx};

/// Shared adventure save blob. Games pack story bits into `flags`; `score` is currency/points/etc.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SavePayload {
    pub version: u32,
    pub scene: i32,
    pub max_hp: i32,
    pub hp: i32,
    pub score: i32,
    pub flags: u32,
    pub pcol: i32,
    pub prow: i32,
    pub entry_col: i32,
    pub entry_row: i32,
}

const SAVE_VERSION: u32 = 1;
const SAVE_SLOTS: usize = 3;
/// Cart header for our layout — bump when the on-cart bytes change.
const CART_MAGIC: [u8; 16] = *b"tish-gba-sv4\0\0\0\0";
const MAGIC_LEN: usize = 16;
const SLOT_STRIDE: usize = core::mem::size_of::<SlotRecord>();

#[repr(C)]
#[derive(Clone, Copy)]
struct SlotRecord {
    occupied: u8,
    _pad: [u8; 3],
    payload: SavePayload,
    crc: u32,
}

/// Last successful `save_read` — getters return these so tish can unpack without arrays.
static mut LAST: SavePayload = SavePayload {
    version: 0,
    scene: 0,
    max_hp: 6,
    hp: 6,
    score: 0,
    flags: 0,
    pcol: 0,
    prow: 0,
    entry_col: 0,
    entry_row: 0,
};

fn last() -> &'static mut SavePayload {
    unsafe { &mut *core::ptr::addr_of_mut!(LAST) }
}

fn slot_of(args: &[Value], i: usize) -> usize {
    let s = num(args, i) as i32;
    if s < 0 {
        return 0;
    }
    let s = s as usize;
    if s >= SAVE_SLOTS {
        SAVE_SLOTS - 1
    } else {
        s
    }
}

fn slot_offset(slot: usize) -> usize {
    MAGIC_LEN + slot * SLOT_STRIDE
}

// Byte access now goes through `save_media`, which is SRAM's direct store on the default build and
// flash's erase-and-reprogram sector cache otherwise. Everything below this line is media-agnostic.
fn sram_read_bytes(offset: usize, buf: &mut [u8]) {
    crate::save_media::read(offset, buf)
}

fn sram_write_bytes(offset: usize, buf: &[u8]) {
    crate::save_media::write(offset, buf)
}

fn payload_crc(p: &SavePayload) -> u32 {
    let bytes = unsafe {
        core::slice::from_raw_parts(
            (p as *const SavePayload) as *const u8,
            core::mem::size_of::<SavePayload>(),
        )
    };
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn read_magic(out: &mut [u8; 16]) {
    sram_read_bytes(0, out);
}

fn write_magic() {
    sram_write_bytes(0, &CART_MAGIC);
}

fn read_slot_record(slot: usize) -> SlotRecord {
    let mut raw = [0u8; SLOT_STRIDE];
    sram_read_bytes(slot_offset(slot), &mut raw);
    unsafe { core::ptr::read(raw.as_ptr() as *const SlotRecord) }
}

fn write_slot_record(slot: usize, rec: &SlotRecord) {
    let bytes = unsafe {
        core::slice::from_raw_parts((rec as *const SlotRecord) as *const u8, SLOT_STRIDE)
    };
    sram_write_bytes(slot_offset(slot), bytes);
}

fn slot_valid(rec: &SlotRecord) -> bool {
    if rec.occupied != 1 {
        return false;
    }
    if rec.payload.version != SAVE_VERSION {
        return false;
    }
    rec.crc == payload_crc(&rec.payload)
}

fn ensure_formatted() {
    let mut magic = [0u8; 16];
    read_magic(&mut magic);
    if magic == CART_MAGIC {
        return;
    }
    write_magic();
    let empty = SlotRecord {
        occupied: 0,
        _pad: [0; 3],
        payload: SavePayload {
            version: 0,
            scene: 0,
            max_hp: 6,
            hp: 6,
            score: 0,
            flags: 0,
            pcol: 0,
            prow: 0,
            entry_col: 0,
            entry_row: 0,
        },
        crc: 0,
    };
    for s in 0..SAVE_SLOTS {
        write_slot_record(s, &empty);
    }
}

/// `save_init()` — emit the SRAM ROM marker. Call once at boot. Format is lazy on first write.
pub fn save_init(_args: &[Value]) -> Value {
    crate::save_media::init();
    with_ctx(|ctx| {
        ctx.save_ready = true;
        Value::Number(1.0)
    })
}

pub fn save_slots(_args: &[Value]) -> Value {
    Value::Number(SAVE_SLOTS as f64)
}

pub fn save_has(args: &[Value]) -> Value {
    let slot = if args.is_empty() { 0 } else { slot_of(args, 0) };
    with_ctx(|ctx| {
        if !ctx.save_ready {
            return Value::Number(0.0);
        }
        let ok = slot_valid(&read_slot_record(slot));
        Value::Number(if ok { 1.0 } else { 0.0 })
    })
}

pub fn save_any(_args: &[Value]) -> Value {
    with_ctx(|ctx| {
        if !ctx.save_ready {
            return Value::Number(0.0);
        }
        let ok = (0..SAVE_SLOTS).any(|s| slot_valid(&read_slot_record(s)));
        Value::Number(if ok { 1.0 } else { 0.0 })
    })
}

pub fn save_write(args: &[Value]) -> Value {
    let slot = slot_of(args, 0);
    let payload = SavePayload {
        version: SAVE_VERSION,
        scene: num(args, 1) as i32,
        max_hp: num(args, 2) as i32,
        hp: num(args, 3) as i32,
        score: num(args, 4) as i32,
        flags: num(args, 5) as u32,
        pcol: num(args, 6) as i32,
        prow: num(args, 7) as i32,
        entry_col: num(args, 8) as i32,
        entry_row: num(args, 9) as i32,
    };
    with_ctx(|ctx| {
        if !ctx.save_ready {
            return Value::Number(0.0);
        }
        ensure_formatted();
        let rec = SlotRecord {
            occupied: 1,
            _pad: [0; 3],
            crc: payload_crc(&payload),
            payload,
        };
        write_slot_record(slot, &rec);
        *last() = payload;
        Value::Number(1.0)
    })
}

pub fn save_read(args: &[Value]) -> Value {
    let slot = if args.is_empty() { 0 } else { slot_of(args, 0) };
    with_ctx(|ctx| {
        if !ctx.save_ready {
            return Value::Number(0.0);
        }
        let rec = read_slot_record(slot);
        if !slot_valid(&rec) {
            return Value::Number(0.0);
        }
        *last() = rec.payload;
        Value::Number(1.0)
    })
}

pub fn save_erase(args: &[Value]) -> Value {
    let slot = slot_of(args, 0);
    with_ctx(|ctx| {
        if !ctx.save_ready {
            return Value::Number(0.0);
        }
        let empty = SlotRecord {
            occupied: 0,
            _pad: [0; 3],
            payload: SavePayload {
                version: 0,
                scene: 0,
                max_hp: 6,
                hp: 6,
                score: 0,
                flags: 0,
                pcol: 0,
                prow: 0,
                entry_col: 0,
                entry_row: 0,
            },
            crc: 0,
        };
        write_slot_record(slot, &empty);
        Value::Number(1.0)
    })
}

pub fn save_scene(_args: &[Value]) -> Value {
    Value::Number(last().scene as f64)
}
pub fn save_max_hp(_args: &[Value]) -> Value {
    Value::Number(last().max_hp as f64)
}
pub fn save_hp(_args: &[Value]) -> Value {
    Value::Number(last().hp as f64)
}
pub fn save_score(_args: &[Value]) -> Value {
    Value::Number(last().score as f64)
}
pub fn save_flags(_args: &[Value]) -> Value {
    Value::Number(last().flags as f64)
}
pub fn save_pcol(_args: &[Value]) -> Value {
    Value::Number(last().pcol as f64)
}
pub fn save_prow(_args: &[Value]) -> Value {
    Value::Number(last().prow as f64)
}
pub fn save_entry_col(_args: &[Value]) -> Value {
    Value::Number(last().entry_col as f64)
}
pub fn save_entry_row(_args: &[Value]) -> Value {
    Value::Number(last().entry_row as f64)
}

// ── Raw SRAM window ────────────────────────────────────────────────────────
// The fixed SlotRecord API above is a 9-field adventure blob. It is the wrong shape for anything that
// wants to store its own layout — a card game's collection and decks are hundreds of small numbers,
// not nine named ones — and there was no other path from Tish to cartridge SRAM at all.
//
// This exposes a raw byte window that starts ABOVE the fixed records, so the two cannot collide no
// matter what a game writes. Offsets handed to Tish are 0-based within the window.
//
// GBA SRAM is an 8-bit bus: only byte-wide volatile access is defined, which is why this is a
// byte API and not a word one.
const RAW_BASE: usize = 2048;
// Sized from the SELECTED medium, so a flash build actually gets its extra 32/96 KiB rather than
// being clamped to SRAM's 32K. `sram_size()` reports this, which is the observable difference a
// demo can assert on.
const RAW_LEN: usize = crate::save_media::MEDIA_LEN - RAW_BASE;

// The fixed records must fit below the raw window. If SlotRecord ever grows past this, the two
// regions would silently overlap and each would corrupt the other.
const _: () = assert!(MAGIC_LEN + SAVE_SLOTS * SLOT_STRIDE <= RAW_BASE);

fn raw_ok(off: i32) -> bool {
    off >= 0 && (off as usize) < RAW_LEN
}

/// `sram_read_u8(off)` — one byte from the raw window, or -1 if `off` is out of range.
///
/// Returns -1 rather than 0 on a bad offset so a caller can tell "byte is zero" from "you asked for
/// something that does not exist"; a save reader that cannot make that distinction silently treats a
/// bounds bug as valid zeroed data.
pub fn sram_read_u8(args: &[Value]) -> Value {
    let off = num(args, 0) as i32;
    if !raw_ok(off) {
        return Value::Number(-1.0);
    }
    let mut b = [0u8; 1];
    sram_read_bytes(RAW_BASE + off as usize, &mut b);
    Value::Number(b[0] as f64)
}

/// `sram_write_u8(off, v)` — one byte into the raw window. Returns 1 on success, 0 if out of range.
///
/// SRAM writes are byte-at-a-time and slow, and flash carts need a sector erase before a rewrite:
/// never call this per frame. Write on a scene transition, gated on a dirty flag.
pub fn sram_write_u8(args: &[Value]) -> Value {
    let off = num(args, 0) as i32;
    if !raw_ok(off) {
        return Value::Number(0.0);
    }
    let v = (num(args, 1) as i32) & 0xFF;
    sram_write_bytes(RAW_BASE + off as usize, &[v as u8]);
    Value::Number(1.0)
}

/// `sram_commit()` — flush point. Returns 1 on success, 0 if the medium reported a failure.
///
/// ⚠️ **On flash this is mandatory and on SRAM it is free**, which is the trap: a game developed
/// against the default SRAM build works whether or not it calls this, and then loses every save the
/// day it is rebuilt for a flash cart. Uncommitted flash writes live only in the sector buffer.
/// Call it once after a save, never per byte — a commit erases and reprograms 4 KiB.
pub fn sram_commit(_args: &[Value]) -> Value {
    let ok = crate::save_media::commit();
    with_ctx(|ctx| {
        if !ctx.save_ready {
            write_magic();
            ctx.save_ready = true;
        }
    });
    Value::Number(if ok { 1.0 } else { 0.0 })
}

/// `save_media_size()` — bytes of save medium the ROM was built for (32768 / 65536 / 131072).
pub fn save_media_size(_args: &[Value]) -> Value {
    Value::Number(crate::save_media::MEDIA_LEN as f64)
}

/// `save_media_name()` — "sram32k" / "flash64k" / "flash128k". A game can show it, and a test can
/// prove which backend a ROM actually carries rather than which one its build file asked for.
pub fn save_media_name(_args: &[Value]) -> Value {
    Value::string(crate::save_media::MEDIA_NAME)
}

/// `save_media_sectors()` — flash erase-sector size in bytes, or 0 on SRAM (no sectors).
pub fn save_media_sectors(_args: &[Value]) -> Value {
    Value::Number(crate::save_media::SECTOR_LEN as f64)
}

/// `sram_size()` — bytes available in the raw window.
pub fn sram_size(_args: &[Value]) -> Value {
    Value::Number(RAW_LEN as f64)
}

// Typed lowering companions for the `declare fn` entries in tish.d.tish. A typed extern call skips
// the boxed Value path entirely — which matters here because a save record is read a byte at a time,
// so a 400-byte record is 400 calls.
pub fn sram_read_u8_typed(off: i32) -> i32 {
    if !raw_ok(off) {
        return -1;
    }
    let mut b = [0u8; 1];
    sram_read_bytes(RAW_BASE + off as usize, &mut b);
    b[0] as i32
}

pub fn sram_write_u8_typed(off: i32, v: i32) -> i32 {
    if !raw_ok(off) {
        return 0;
    }
    sram_write_bytes(RAW_BASE + off as usize, &[(v & 0xFF) as u8]);
    1
}

pub fn sram_commit_typed() -> i32 {
    let ok = crate::save_media::commit();
    with_ctx(|ctx| {
        if !ctx.save_ready {
            write_magic();
            ctx.save_ready = true;
        }
    });
    if ok {
        1
    } else {
        0
    }
}

pub fn sram_size_typed() -> i32 {
    RAW_LEN as i32
}
