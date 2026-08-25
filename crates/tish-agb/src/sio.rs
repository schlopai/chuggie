use core::ptr::{read_volatile, write_volatile};

const SIOCNT: *mut u16 = 0x0400_0128 as *mut u16;
const SIODATA8: *mut u16 = 0x0400_012A as *mut u16;

pub fn init() {
    unsafe {
        // Normal 8-bit mode, internal clock
        write_volatile(SIOCNT, 0x3000);
    }
}

pub fn send(byte: i32) {
    unsafe {
        // Wait until not busy (bit 7)
        while (read_volatile(SIOCNT) & 0x0080) != 0 {}
        write_volatile(SIODATA8, (byte & 0xFF) as u16);
        // Start transfer
        let cnt = read_volatile(SIOCNT);
        write_volatile(SIOCNT, cnt | 0x0080);
    }
}

pub fn recv() -> i32 {
    unsafe {
        let b = read_volatile(SIODATA8) & 0xFF;
        let cnt = read_volatile(SIOCNT);
        // Clear bit 5
        write_volatile(SIOCNT, cnt & !0x0020);
        b as i32
    }
}

pub fn available() -> i32 {
    unsafe {
        if (read_volatile(SIOCNT) & 0x0020) != 0 {
            1
        } else {
            0
        }
    }
}
