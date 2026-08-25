#!/usr/bin/env python3
"""Read or write a `packages/prefs.tish` save block in a .sav file, from a test.

Two things need this. A test that asserts an unlock PERSISTED has to read the block back and know
it is genuinely valid rather than four plausible bytes. And a test that wants to START from an
unlocked cartridge has to write one — hand-poking bytes works right up until the block grows a
checksum, at which point the game correctly rejects the poke and the test fails for the wrong
reason. (That is exactly what happened when drop-story moved off its ad-hoc two-byte flag.)

Layout, matching packages/prefs.tish: magic lo/hi, version, count, checksum lo/hi, then `count`
32-bit little-endian slots. The checksum is the sum of every payload byte folded to 16 bits. The
window begins at cartridge offset 2048 — below that are the engine's own fixed save records, see
crates/tish-agb/src/save_api.rs.

    scripts/prefs_io.py game.sav --magic 0x4D44 --version 1 --count 2 --read
    scripts/prefs_io.py game.sav --magic 0x4D44 --version 1 --count 2 --set 0=1 --set 1=500
    scripts/prefs_io.py game.sav --magic 0x4D44 --version 1 --count 2 --expect 0=1

`--read` prints the slots. `--expect` exits non-zero unless the block is valid AND the slot
matches. `--set` writes (creating the file if needed) and rewrites the checksum.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

RAW_BASE = 2048
SRAM_LEN = 32 * 1024
HDR = 6
SLOT_BYTES = 4


def parse_kv(s: str) -> tuple[int, int]:
    k, _, v = s.partition("=")
    return int(k), int(v, 0)


def read_block(data: bytes, magic: int, version: int, count: int) -> list[int] | None:
    """The slots, or None when the block is blank, mismatched or corrupt — the game's own rule."""
    b = data[RAW_BASE:]
    if len(b) < HDR + count * SLOT_BYTES:
        return None
    if b[0] | (b[1] << 8) != magic or b[2] != version or b[3] != count:
        return None
    want = b[4] | (b[5] << 8)
    slots, total = [], 0
    for i in range(count):
        off = HDR + i * SLOT_BYTES
        v = int.from_bytes(b[off:off + SLOT_BYTES], "little")
        slots.append(v)
        total += sum(b[off:off + SLOT_BYTES])
    if total & 0xFFFF != want:
        return None
    return slots


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("sav")
    ap.add_argument("--magic", type=lambda s: int(s, 0), required=True)
    ap.add_argument("--version", type=int, default=1)
    ap.add_argument("--count", type=int, required=True)
    ap.add_argument("--read", action="store_true")
    ap.add_argument("--set", action="append", default=[], metavar="SLOT=VALUE")
    ap.add_argument("--expect", action="append", default=[], metavar="SLOT=VALUE")
    # Bit-wise, because a flags slot gains bits over time: asserting the whole word means the test
    # breaks the day a second flag is added, for no reason and in a way that reads like a bug.
    ap.add_argument("--expect-bit", action="append", default=[], metavar="SLOT=BIT")
    args = ap.parse_args()

    p = Path(args.sav)
    # 0xFF is what an erased cartridge reads as, which is also what the game must treat as blank.
    data = bytearray(p.read_bytes()) if p.exists() else bytearray(b"\xff" * SRAM_LEN)
    if len(data) < SRAM_LEN:
        data.extend(b"\xff" * (SRAM_LEN - len(data)))

    if args.set:
        slots = read_block(bytes(data), args.magic, args.version, args.count) or [0] * args.count
        for k, v in (parse_kv(s) for s in args.set):
            slots[k] = v
        total = 0
        for i, v in enumerate(slots):
            off = RAW_BASE + HDR + i * SLOT_BYTES
            raw = v.to_bytes(SLOT_BYTES, "little", signed=False)
            data[off:off + SLOT_BYTES] = raw
            total += sum(raw)
        data[RAW_BASE + 0] = args.magic & 0xFF
        data[RAW_BASE + 1] = (args.magic >> 8) & 0xFF
        data[RAW_BASE + 2] = args.version
        data[RAW_BASE + 3] = args.count
        data[RAW_BASE + 4] = total & 0xFF
        data[RAW_BASE + 5] = (total >> 8) & 0xFF
        p.write_bytes(bytes(data))
        print(f"  wrote {p.name}: {slots}")
        return 0

    slots = read_block(bytes(data), args.magic, args.version, args.count)
    if slots is None:
        print(f"  {p.name}: no valid prefs block (blank, wrong version, or bad checksum)")
        return 1
    if args.read:
        print(f"  {p.name}: {slots}")
    ok = True
    for k, v in (parse_kv(s) for s in args.expect):
        if slots[k] != v:
            print(f"  {p.name}: slot {k} is {slots[k]}, expected {v}")
            ok = False
    for k, b in (parse_kv(s) for s in args.expect_bit):
        if not slots[k] & (1 << b):
            print(f"  {p.name}: slot {k} is {slots[k]}, bit {b} not set")
            ok = False
    if (args.expect or args.expect_bit) and ok:
        want = args.expect + [f"bit {s}" for s in args.expect_bit]
        print(f"  {p.name}: prefs block valid, {want} as expected")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
