//! Save-media backends: **battery-backed SRAM** (the traditional cart) and **FLASH 64K/128K** (what
//! late-era 128 KiB flash carts ship).
//!
//! `save_api` used to poke `0x0E000000` directly and assume 32 KiB of byte-addressable SRAM. That is
//! correct for one of the three media a GBA cart can carry and silently wrong for the other two:
//! flash ignores a plain store until the chip has been put in program mode, so a game built for a
//! flash cart appeared to save, read back its own RAM-shadowed bytes during the same session, and
//! lost everything on power-off. This module is the seam that makes the media a build-time choice.
//!
//! # Choosing the media
//!
//! **The choice is a Cargo feature, not a runtime call, and it has to be.** An emulator or flash
//! cart decides what hardware to present by scanning the ROM image for a marker string
//! (`SRAM_Vnnn`, `FLASH512_Vnnn`, `FLASH1M_Vnnn`). Exactly one may be present — emit two and the
//! detector picks whichever it happens to match first, which is how a game ends up talking the flash
//! protocol to an emulated SRAM chip. So the marker, and therefore the backend, is fixed when the
//! ROM is built:
//!
//! ```json
//! "tish_agb": { "path": "../../crates/tish-agb", "features": ["save-flash-1m"] }
//! ```
//!
//! in an example's `package.json`. **SRAM is the absence of a feature, not a feature of its own**,
//! and that is load-bearing rather than stylistic: Cargo features are additive and unified across
//! the whole dependency graph, so a `save-sram` feature would be switched back on by
//! `tish-gba-game-engine`'s own plain dependency on `tish_agb` no matter what the example asked for.
//! The first cut had exactly that bug — `"default-features": false` in the example was silently
//! overridden and the build died on duplicate `MEDIA_LEN` definitions. Opt-in flash, default SRAM,
//! nothing to unify.
//!
//! # Why not agb's own save stack
//!
//! `agb::save::SaveManager` has the media drivers and would have been the obvious answer, but its
//! only public surface is `SaveSlotManager`, which is serde-backed and keeps heap allocations for
//! the life of the program. `save_api`'s header records why that was rejected once already: the
//! lasting heap cost tips large scene loads (the Akari shrine) into OOM. agb's low-level `SaveData`
//! accessor is `pub(crate)`, so it cannot be borrowed either. The marker statics below are the same
//! twelve bytes agb emits.
//!
//! # The write model
//!
//! SRAM takes byte stores directly. Flash cannot: a bit can only be programmed from 1 to 0, so
//! changing a byte means erasing its whole 4 KiB sector first. This module therefore keeps **one
//! sector in a write-back buffer** — `write()` faults the sector in and dirties the buffer,
//! `commit()` erases and reprograms it. That is exactly the flush point `sram_commit()` was already
//! documented as reserving, so no caller changes.
//!
//! ⚠️ **`commit()` is not optional on flash.** Uncommitted writes live only in the buffer. On SRAM it
//! remains the no-op it always was, which is precisely why a game that only ever runs on SRAM can
//! forget the call and still appear correct.

#![allow(dead_code)]

// ── media selection ──────────────────────────────────────────────────────────────────────────────
// Cargo features are additive, so a mutually exclusive set has to be policed by hand. Failing at
// compile time is the whole point: silently picking one of two requested media would put the wrong
// marker in the ROM, and the symptom (saves that vanish on power-off) shows up on hardware long
// after the build.
#[cfg(all(feature = "save-flash-512k", feature = "save-flash-1m"))]
compile_error!(
    "tish-agb: pick ONE save medium — save-flash-512k and save-flash-1m are exclusive. \
     SRAM is the default and needs no feature."
);

/// Cart bus base. Both SRAM and flash live here; only the protocol differs.
const CART_BASE: usize = 0x0E00_0000;

#[cfg(not(any(feature = "save-flash-512k", feature = "save-flash-1m")))]
pub const MEDIA_LEN: usize = 32 * 1024;
#[cfg(feature = "save-flash-512k")]
pub const MEDIA_LEN: usize = 64 * 1024;
#[cfg(feature = "save-flash-1m")]
pub const MEDIA_LEN: usize = 128 * 1024;

/// Reported to tish so a game can size its own blob, and so a test can prove which backend is live.
#[cfg(not(any(feature = "save-flash-512k", feature = "save-flash-1m")))]
pub const MEDIA_NAME: &str = "sram32k";
#[cfg(feature = "save-flash-512k")]
pub const MEDIA_NAME: &str = "flash64k";
#[cfg(feature = "save-flash-1m")]
pub const MEDIA_NAME: &str = "flash128k";

/// Flash erases a whole sector at a time. SRAM has no sectors; 0 says so.
#[cfg(not(any(feature = "save-flash-512k", feature = "save-flash-1m")))]
pub const SECTOR_LEN: usize = 0;
#[cfg(any(feature = "save-flash-512k", feature = "save-flash-1m"))]
pub const SECTOR_LEN: usize = 4096;

// ── the ROM markers ──────────────────────────────────────────────────────────────────────────────
// A detector scans the ROM image for these. `black_box` is what keeps the linker from dropping a
// static nothing reads — the string exists to be FOUND, not to be used.
#[repr(align(4))]
struct Marker<T>(T);

#[cfg(not(any(feature = "save-flash-512k", feature = "save-flash-1m")))]
static MARKER: Marker<[u8; 12]> = Marker(*b"SRAM_Vnnn\0\0\0");
#[cfg(feature = "save-flash-512k")]
static MARKER: Marker<[u8; 16]> = Marker(*b"FLASH512_Vnnn\0\0\0");
#[cfg(feature = "save-flash-1m")]
static MARKER: Marker<[u8; 16]> = Marker(*b"FLASH1M_Vnnn\0\0\0\0");

/// Emit the media marker and prepare the bus. Call before the first access.
pub fn init() {
    core::hint::black_box(&MARKER);
    set_cart_waitstate();
    #[cfg(any(feature = "save-flash-512k", feature = "save-flash-1m"))]
    flash::reset_bank();
}

/// WAITCNT bits 0..1 are the cart-RAM waitstate. Flash command sequences are timing-sensitive and
/// the reset default (4 cycles) is marginal on real hardware, so widen to 8 — but OR it in rather
/// than storing a whole word, because the other fields are agb's ROM waitstates and prefetch and
/// clobbering those slows every cartridge read in the game.
fn set_cart_waitstate() {
    const WAITCNT: *mut u16 = 0x0400_0204 as *mut u16;
    unsafe {
        let cur = core::ptr::read_volatile(WAITCNT);
        core::ptr::write_volatile(WAITCNT, (cur & !0x0003) | 0x0003);
    }
}

// ── SRAM ─────────────────────────────────────────────────────────────────────────────────────────
#[cfg(not(any(feature = "save-flash-512k", feature = "save-flash-1m")))]
mod backend {
    use super::CART_BASE;

    /// An 8-bit bus: only byte-wide volatile access is defined. A `u16`/`u32` access returns the
    /// low byte replicated, which reads as plausible data and is the classic way to corrupt a save.
    pub fn read(offset: usize, buf: &mut [u8]) {
        let src = (CART_BASE + offset) as *const u8;
        for (i, b) in buf.iter_mut().enumerate() {
            *b = unsafe { core::ptr::read_volatile(src.add(i)) };
        }
    }

    pub fn write(offset: usize, buf: &[u8]) {
        let dst = (CART_BASE + offset) as *mut u8;
        for (i, b) in buf.iter().enumerate() {
            unsafe { core::ptr::write_volatile(dst.add(i), *b) };
        }
    }

    /// SRAM stores land immediately. The call still exists so a game written against SRAM has the
    /// flush call site already present when it is rebuilt for a flash cart.
    pub fn commit() -> bool {
        true
    }
}

// ── FLASH 64K / 128K ─────────────────────────────────────────────────────────────────────────────
#[cfg(any(feature = "save-flash-512k", feature = "save-flash-1m"))]
mod flash {
    use super::{CART_BASE, MEDIA_LEN, SECTOR_LEN};

    const BANK_LEN: usize = 64 * 1024;

    fn put(off: usize, v: u8) {
        unsafe { core::ptr::write_volatile((CART_BASE + off) as *mut u8, v) }
    }
    fn get(off: usize) -> u8 {
        unsafe { core::ptr::read_volatile((CART_BASE + off) as *const u8) }
    }

    /// The unlock prelude every flash command needs: 0xAA to 0x5555, 0x55 to 0x2AAA, then the
    /// opcode. Anything that skips it is ignored by the chip — which is exactly what a plain store
    /// to flash is, and why a flash cart looked like it was saving and was not.
    fn cmd(op: u8) {
        put(0x5555, 0xAA);
        put(0x2AAA, 0x55);
        put(0x5555, op);
    }

    /// Wait for the chip to finish by polling the target byte until it reads its final value. The
    /// bound is a real timeout, not a formality: a dead or absent chip never converges, and a bare
    /// `while` here hangs the ROM with no output at all.
    fn poll(off: usize, want: u8) -> bool {
        for _ in 0..0x0020_0000u32 {
            if get(off) == want {
                return true;
            }
        }
        false
    }

    /// 128K flash is two 64K banks behind one window; 64K flash has a single bank and no switch.
    static mut BANK: u8 = 0xFF;

    pub fn reset_bank() {
        unsafe { BANK = 0xFF };
        select_bank(0);
    }

    fn select_bank(bank: u8) {
        if MEDIA_LEN <= BANK_LEN {
            return;
        }
        // Skip the command when the bank is already live: a switch costs a full unlock sequence, and
        // a byte-at-a-time writer would otherwise pay it on every single byte.
        if unsafe { BANK } == bank {
            return;
        }
        cmd(0xB0);
        put(0x0000, bank);
        unsafe { BANK = bank };
    }

    fn split(offset: usize) -> (u8, usize) {
        ((offset / BANK_LEN) as u8, offset % BANK_LEN)
    }

    pub fn read_raw(offset: usize, buf: &mut [u8]) {
        let mut done = 0;
        while done < buf.len() {
            let (bank, base) = split(offset + done);
            select_bank(bank);
            let n = core::cmp::min(buf.len() - done, BANK_LEN - base);
            for i in 0..n {
                buf[done + i] = get(base + i);
            }
            done += n;
        }
    }

    fn erase_sector(offset: usize) -> bool {
        let (bank, base) = split(offset);
        select_bank(bank);
        cmd(0x80);
        put(0x5555, 0xAA);
        put(0x2AAA, 0x55);
        put(base, 0x30);
        // Erase leaves every bit set.
        poll(base, 0xFF)
    }

    fn program(offset: usize, v: u8) -> bool {
        let (bank, base) = split(offset);
        select_bank(bank);
        cmd(0xA0);
        put(base, v);
        poll(base, v)
    }

    /// One sector, held in EWRAM. Flash cannot rewrite a byte in place, so a byte-level API has to
    /// buffer: fault the sector in, edit the copy, and erase-and-reprogram the whole thing on
    /// commit. One sector rather than a whole-media shadow keeps this to 4 KiB — a 128 KiB shadow
    /// would be half the EWRAM budget of the entire game.
    static mut SECTOR: [u8; SECTOR_LEN] = [0xFF; SECTOR_LEN];
    static mut CACHED: usize = usize::MAX;
    static mut DIRTY: bool = false;

    fn sector_of(offset: usize) -> usize {
        offset / SECTOR_LEN
    }

    fn fault_in(sector: usize) -> bool {
        unsafe {
            if CACHED == sector {
                return true;
            }
            if DIRTY && !flush() {
                return false;
            }
            let buf = &mut *core::ptr::addr_of_mut!(SECTOR);
            read_raw(sector * SECTOR_LEN, buf);
            CACHED = sector;
            DIRTY = false;
        }
        true
    }

    fn flush() -> bool {
        unsafe {
            if !DIRTY || CACHED == usize::MAX {
                DIRTY = false;
                return true;
            }
            let base = CACHED * SECTOR_LEN;
            if !erase_sector(base) {
                return false;
            }
            let buf = &*core::ptr::addr_of!(SECTOR);
            for (i, &b) in buf.iter().enumerate() {
                // Erase already left 0xFF everywhere, so only the bytes that differ need a program
                // pulse. On a mostly-empty save sector that is the difference between 4096 slow
                // writes and a few dozen.
                if b != 0xFF && !program(base + i, b) {
                    return false;
                }
            }
            DIRTY = false;
        }
        true
    }

    pub fn read(offset: usize, buf: &mut [u8]) {
        // Read THROUGH the cache: a byte written this session but not yet committed must read back
        // as written, or a game that saves and immediately re-reads sees the old contents.
        unsafe {
            let sector = sector_of(offset);
            if CACHED == sector && offset % SECTOR_LEN + buf.len() <= SECTOR_LEN {
                let start = offset % SECTOR_LEN;
                let src = &*core::ptr::addr_of!(SECTOR);
                buf.copy_from_slice(&src[start..start + buf.len()]);
                return;
            }
        }
        read_raw(offset, buf);
    }

    pub fn write(offset: usize, buf: &[u8]) {
        for (i, &b) in buf.iter().enumerate() {
            let off = offset + i;
            if !fault_in(sector_of(off)) {
                return;
            }
            unsafe {
                let dst = &mut *core::ptr::addr_of_mut!(SECTOR);
                let idx = off % SECTOR_LEN;
                if dst[idx] != b {
                    dst[idx] = b;
                    DIRTY = true;
                }
            }
        }
    }

    pub fn commit() -> bool {
        flush()
    }
}

#[cfg(any(feature = "save-flash-512k", feature = "save-flash-1m"))]
mod backend {
    pub use super::flash::{commit, read, write};
}

pub use backend::{commit, read, write};
