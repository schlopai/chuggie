#![no_std]

use core::ptr::{read_volatile, write_volatile};
use tishlang_runtime_gba::Value;

const SIOCNT: *mut u16 = 0x0400_0128 as *mut u16;
const SIODATA8: *mut u16 = 0x0400_012A as *mut u16;

pub fn sio_init_typed() {
    unsafe {
        // Normal 8-bit mode, internal clock
        write_volatile(SIOCNT, 0x3000);
    }
}

pub fn sio_send_typed(byte: i32) {
    unsafe {
        // (Removed busy loop on bit 7 to prevent emulator freezes)
        write_volatile(SIODATA8, (byte & 0xFF) as u16);
        // Start transfer and set bit 6 (outbound flag for mgba-bridge)
        let cnt = read_volatile(SIOCNT);
        write_volatile(SIOCNT, cnt | 0x0080 | 0x0040);
    }
}

pub fn sio_recv_typed() -> i32 {
    unsafe {
        let b = (read_volatile(SIODATA8) & 0xFF) as i32;
        let cnt = read_volatile(SIOCNT);
        // Clear bit 5
        write_volatile(SIOCNT, cnt & !0x0020);
        b
    }
}

pub fn sio_available_typed() -> i32 {
    unsafe {
        if (read_volatile(SIOCNT) & 0x0020) != 0 {
            1
        } else {
            0
        }
    }
}

// Untyped Tish ABI wrappers (required by Tish generated_native.rs)
pub fn sio_init(_args: &[Value]) -> Value {
    sio_init_typed();
    Value::Null
}

pub fn sio_send(args: &[Value]) -> Value {
    if let Value::Number(n) = args[0] {
        sio_send_typed(n as i32);
    }
    Value::Null
}

pub fn sio_recv(_args: &[Value]) -> Value {
    Value::Number(sio_recv_typed() as f64)
}

pub fn sio_available(_args: &[Value]) -> Value {
    Value::Number(sio_available_typed() as f64)
}
