#!/usr/bin/env python3
"""Report the stack frame a GBA ELF's functions allocate, and gate the IWRAM budget.

WHY THIS EXISTS. A GBA ROM's user stack is ~32,512 bytes of IWRAM, and tish emits every
module binding and every module's top-level statements into ONE `run()` (tishlang/tish#682).
That frame grows with program size until it eats the stack — and an overflow does NOT trap:
the deepest SP aliases into the top of the EWRAM heap through the GBA's 256 KB mirroring, so
the ROM keeps running while corrupting allocated memory. A large SRPG example once reached its
third act with zero faults while overflowing by 1,992 bytes. A green ROM is therefore not
evidence of stack safety; this measurement is.

⚠️ TWO PROLOGUE FORMS, and getting the second wrong is the failure this tool exists to catch.
A small frame is `sub sp, #imm` — the immediate is right there. A LARGE frame cannot encode
its size in the instruction, so the compiler loads it from the literal pool as a NEGATIVE
number and adds it:

    ldr r6, [pc, #123]      ; r6 = 0xffff952c  (= -27,348)
    add sp, r6              ; sp += -27,348

Read that literal as unsigned and you report a ~4 GB frame; ignore the sign and you report a
tiny one for the single biggest function in the program. Both are silently wrong in the
direction that hides the bug, so the sign handling below is deliberate and tested.

⚠️ MEASURE UNDER THE DEFAULT (FAT) LTO. Thin LTO produces materially larger frames — it costs
that example ~1,832 bytes and pushes it over budget — and it also poisons any shared cargo cache. A
number taken under a different LTO mode is not comparable to one taken here.

⚠️ SUM THE CALLEE. `run()` calling a factory means BOTH frames are live at once. Pass the
factory with --also so the reported total is what the hardware actually sees.

Usage
  scripts/framesize.py <elf> [--also SYM ...] [--budget N] [--json]
  scripts/framesize.py <elf> --all            # every function over --min-report bytes

Exit status is 1 when the measured total exceeds the budget, so this gates in CI rather than
merely reporting.
"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys

# The GBA hands the user stack ~32,512 bytes of IWRAM (0x03007F00 down to __iwram_end).
# See docs/agent-dev-loop.md and tish docs/gba-target.md for the measured breakdown.
DEFAULT_BUDGET = 32512


def _tool(*names: str) -> str | None:
    """Find a binutil, honouring the ORDER of `names` across both search locations.

    ⚠️ Each name is looked up in PATH *and* in rustup's llvm-tools before moving to the
    next name. Searching all of PATH first would find Apple's /usr/bin/objdump — which
    cannot disassemble a single symbol of a thumbv4t ELF (`unknown argument
    '--disassemble=...'`) — and never reach the llvm-objdump rustup ships out of PATH.
    """
    import glob
    import os
    for n in names:
        p = shutil.which(n)
        if p:
            return p
        if n.startswith("llvm-"):
            for c in sorted(glob.glob(os.path.expanduser(
                    f"~/.rustup/toolchains/*/lib/rustlib/*/bin/{n}")), reverse=True):
                if os.access(c, os.X_OK):
                    return c
    return None


def disassemble(elf: str, symbol: str) -> list[str]:
    """Disassemble one symbol. Prefer llvm-objdump (ships with rustup's llvm-tools)."""
    objdump = _tool("arm-none-eabi-objdump", "llvm-objdump", "objdump")
    if not objdump:
        sys.exit("framesize: need arm-none-eabi-objdump or llvm-objdump on PATH")
    if "llvm-objdump" in objdump:
        cmd = [objdump, "-d", f"--disassemble-symbols={symbol}", elf]
    else:
        cmd = [objdump, "-d", "--disassemble=" + symbol, elf]
    out = subprocess.run(cmd, capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(f"framesize: objdump failed for {symbol}:\n{out.stderr.strip()}")
    return out.stdout.splitlines()


def symbols(elf: str) -> list[tuple[int, str]]:
    """(size, name) for every function symbol, largest first."""
    nm = _tool("arm-none-eabi-nm", "llvm-nm", "nm")
    if not nm:
        sys.exit("framesize: need nm on PATH")
    out = subprocess.run([nm, "--print-size", "--size-sort", elf],
                         capture_output=True, text=True)
    found = []
    for line in out.stdout.splitlines():
        parts = line.split()
        if len(parts) == 4 and parts[2].lower() in ("t", "w"):
            found.append((int(parts[1], 16), parts[3]))
    return sorted(found, reverse=True)


def resolve_symbol(elf: str, want: str) -> str | None:
    """Exact match, else the mangled `…3run…` style suffix match."""
    names = [n for _, n in symbols(elf)]
    if want in names:
        return want
    pat = re.compile(rf"\d+{re.escape(want)}\b")
    hits = [n for n in names if pat.search(n)] or [n for n in names if want in n]
    if not hits:
        return None
    # the biggest match is the definition, not a thunk
    sizes = dict((n, s) for s, n in symbols(elf))
    return max(hits, key=lambda n: sizes.get(n, 0))


_SUB_SP = re.compile(r"\bsub\s+sp,\s*(?:sp,\s*)?#(?:0x)?([0-9a-fA-F]+)")
# `ldr r6, [pc, #0x380]   @ 0x8000a80 <...>` — the trailing comment is the literal's
# ADDRESS, not its value. The value must be read out of the ELF at that address.
_LDR_LIT = re.compile(r"\bldr\s+(r\d+),\s*\[pc[^\]]*\][^@;]*[@;]\s*(?:0x)?([0-9a-fA-F]+)")
_ADD_SP = re.compile(r"\badd\s+sp,\s*(?:sp,\s*)?(r\d+)")


def _sections(elf: str) -> list[tuple[int, int, int]]:
    """(vaddr, file_offset, size) per allocated section, for literal-pool reads."""
    import struct
    with open(elf, "rb") as fh:
        f = fh.read()
    if f[:4] != b"\x7fELF":
        sys.exit(f"framesize: {elf} is not an ELF")
    shoff = struct.unpack_from("<I", f, 0x20)[0]
    shentsize = struct.unpack_from("<H", f, 0x2E)[0]
    shnum = struct.unpack_from("<H", f, 0x30)[0]
    out = []
    for i in range(shnum):
        _n, _t, _fl, addr, off, size = struct.unpack_from("<6I", f, shoff + i * shentsize)
        if addr:
            out.append((addr, off, size))
    return out


def _word_at(elf: str, addr: int) -> int | None:
    """The 32-bit little-endian word the ROM holds at `addr` (a literal-pool entry)."""
    import struct
    with open(elf, "rb") as fh:
        f = fh.read()
    for vaddr, off, size in _sections(elf):
        if vaddr <= addr < vaddr + size:
            fo = off + (addr - vaddr)
            if fo + 4 <= len(f):
                return struct.unpack_from("<I", f, fo)[0]
    return None


def frame_of(elf: str, symbol: str, window: int = 24) -> tuple[int, str]:
    """Bytes this symbol's prologue reserves, and which form was recognised."""
    lines = disassemble(elf, symbol)
    body = [l for l in lines if ":" in l and "\t" in l]
    head = body[:window]

    total = 0
    forms: list[str] = []

    # Form 1: a direct immediate — `sub sp, #0x18`. May appear more than once.
    for l in head:
        m = _SUB_SP.search(l)
        if m:
            total += int(m.group(1), 16)
            forms.append("sub-imm")

    # Form 2: the literal-pool load. A frame too large to encode as an immediate is loaded
    # as a TWO'S-COMPLEMENT NEGATIVE and ADDED to sp: 27,348 bytes appears as 0xffff952c.
    # Read it unsigned and you report a 4 GB frame; drop the sign and you report a tiny one
    # for the single biggest function in the program. Both hide the bug this tool exists for.
    loaded: dict[str, int] = {}
    for l in head:
        m = _LDR_LIT.search(l)
        if m:
            reg, lit_addr = m.group(1), int(m.group(2), 16)
            w = _word_at(elf, lit_addr)
            if w is not None:
                loaded[reg] = w
        m2 = _ADD_SP.search(l)
        if m2:
            reg = m2.group(1)
            if reg in loaded:
                raw = loaded[reg]
                signed = raw - (1 << 32) if raw >= (1 << 31) else raw
                if signed < 0:
                    total += -signed
                    forms.append("ldr+add-sp")
    return total, "+".join(dict.fromkeys(forms)) or "none"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("elf")
    ap.add_argument("--also", action="append", default=[],
                    help="additional symbol whose frame is LIVE AT THE SAME TIME as run()'s "
                         "(a factory called from run()); summed into the total")
    ap.add_argument("--symbol", default="run", help="entry symbol (default: run)")
    ap.add_argument("--budget", type=int, default=DEFAULT_BUDGET)
    ap.add_argument("--all", action="store_true", help="report every large function")
    ap.add_argument("--min-report", type=int, default=512)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    rows: list[tuple[str, int, str]] = []

    if args.all:
        for size, name in symbols(args.elf)[:60]:
            f, form = frame_of(args.elf, name)
            if f >= args.min_report:
                rows.append((name, f, form))
        rows.sort(key=lambda r: -r[1])
    else:
        for want in [args.symbol] + args.also:
            sym = resolve_symbol(args.elf, want)
            if not sym:
                sys.exit(f"framesize: no symbol matching {want!r} in {args.elf}")
            f, form = frame_of(args.elf, sym)
            rows.append((sym, f, form))

    total = sum(f for _, f, _ in rows)
    over = total > args.budget

    if args.json:
        print(json.dumps({
            "elf": args.elf, "budget": args.budget, "total": total, "over": over,
            "frames": [{"symbol": s, "bytes": f, "form": fm} for s, f, fm in rows],
        }, indent=1))
    else:
        for s, f, fm in rows:
            short = s if len(s) <= 58 else s[:55] + "..."
            print(f"  {f:>7,} B  {short}  [{fm}]")
        if len(rows) > 1:
            print(f"  {'-' * 7}")
            print(f"  {total:>7,} B  TOTAL (simultaneously live)")
        head = round(args.budget - total)
        verdict = "OVER BUDGET" if over else "ok"
        print(f"\nframesize: {total:,} of {args.budget:,} B  "
              f"({head:+,} B headroom)  {verdict}")
    # one stable machine-readable line for A/B diffing across a compiler change
    print(f"FRAMESIZE total={total} budget={args.budget} headroom={args.budget - total} "
          f"over={int(over)} elf={args.elf}", file=sys.stderr)
    return 1 if over else 0


if __name__ == "__main__":
    sys.exit(main())
