#!/usr/bin/env python3
"""Generate examples/artillery's lookup tables as typed tish literals.

⚠️ THESE TABLES EXIST BECAUSE THE ARM7TDMI HAS NO FPU AND NO DIVIDE INSTRUCTION.

Every one of them replaces an arithmetic operation the chip cannot do cheaply:

  SQ     replaces `dx * dx`      — a plain `*` between two i32 LOCALS compiles to an f64 multiply.
                                   Verified in examples/soccer's generated Rust:
                                   `let mut d2: i32 = ((((dx) as f64) * ((dx) as f64)) + ...)`.
  GACC   replaces `C / (d*d*d)`  — an inverse-cube law, i.e. a divide, per planet per substep.
  ISQRT  replaces `sqrt(d2)`     — blast falloff is linear in DISTANCE, not distance squared.
  SINT   replaces `sin`/`cos`    — there is no trig on this chip reachable from tish at all.
  COST

They are GENERATED rather than built at boot for two reasons: a boot loop costs frames on a machine
where boot time is visible, and a committed table can be read, diffed and audited. The audit block
this script prints at the end is the point — the gravity constant is tuned by reading numbers here,
not by rebuilding the ROM and squinting at an arc.

    python3 scripts/gen_artillery_tables.py
"""

import math
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "examples/artillery/src/tables.tish"

# ── The gravity constant ─────────────────────────────────────────────────────────────────────────
# GACC[i] = C / d^3, and the runtime multiplies by dx to get an acceleration, so the force law is
# the honest inverse square: a = C * dx / d^3 = (C / d^2) * (dx / d).
#
# C is chosen so that a mass-256 planet at a TYPICAL engagement distance bends a shot by about its
# own speed over a flight — see the audit block. Too small and the wells are decoration; too large
# and every shot spirals and the game is unaimable.
C = 3_000_000_000

# ⚠️ THE NEAR-FIELD CLAMP IS THE SINGULARITY FIX, AND IT LIVES IN THE TABLE.
#
# 1/d^3 diverges at d=0. The alternative to clamping here is a runtime branch in the hottest loop in
# the game, executed per planet per substep, to guard a case that cannot actually be reached — the
# collision test fires at d = r + SHELL_R >= 12 before gravity is ever evaluated. Clamping the TABLE
# makes the kernel a bounded function everywhere including d=0, at a cost of exactly zero
# instructions, and means an out-of-range read can never produce an infinity.
DMIN = 6.0

# ── Index compression for GACC ───────────────────────────────────────────────────────────────────
# The runtime has d2 (squared distance) and wants 1/d^3. Indexing by d2 directly would need 84,000
# entries for a 240x160 arena. Two slopes instead:
#
#     d2 <  4096:  idx = d2 >> 2            fine   — 1024 buckets over d in [0, 64)
#     d2 >= 4096:  idx = 1024 | (d2 >> 8)   coarse — 1024 buckets over d in [64, 512)
#
# ⚠️ The runtime writes `1024 | hi`, NOT `1024 + hi`. A shift result is not i32-typed to a
# surrounding `+`, so the add would drag the whole index expression into soft float — exactly what
# `1 + (dist >> 5)` does in examples/golf's generated Rust. The OR is exact because hi < 1024.
GACC_N = 2048
FINE_SPLIT = 4096

# SQ covers |dx| up to 1023. The runtime masks with & 1023, so anything beyond would ALIAS a far
# offset onto a near one — a planet 1200px away yanking the shell, which looks like a physics bug
# and is a masking bug. The shell's escape bound (+/-480 of a 240x160 arena) keeps |dx| under 720.
SQ_N = 1024

# ISQRT covers d2 up to 2047, i.e. d up to 45. The widest blast radius in the spike is 32.
ISQRT_N = 2048


def d_of(i):
    """The distance a GACC index stands for — the inverse of the runtime's compression."""
    if i < GACC_N // 2:
        d2 = i * 4 + 2                       # midpoint of the fine bucket
    else:
        d2 = (i - GACC_N // 2) * 256 + 128   # midpoint of the coarse bucket
    return math.sqrt(max(d2, 1.0))


def rows(name, vals, per):
    """Emit `export let NAME: i32[] = [...]`, wrapped.

    ⚠️ `export let X: i32[]` — never `const`, and never unannotated. A `const`, or a `let` with no
    type, is a boxed Value::Array of boxed f64 at 28 bytes an element; the annotated `let` is
    promoted to a Rust `const [i32; N]`, which is a 0.45-tick ROM load (examples/bench-tables §3).
    """
    out = [f"export let {name}: i32[] = ["]
    for i in range(0, len(vals), per):
        out.append("  " + ", ".join(str(v) for v in vals[i:i + per]) + ",")
    out[-1] = out[-1].rstrip(",")
    out.append("]")
    return "\n".join(out)


def main():
    gacc = [int(round(C / max(d_of(i), DMIN) ** 3)) for i in range(GACC_N)]
    sq = [i * i for i in range(SQ_N)]
    isqrt = [int(round(math.sqrt(i))) for i in range(ISQRT_N)]
    sint = [int(round(256 * math.sin(2 * math.pi * a / 256))) for a in range(256)]
    cost = [int(round(256 * math.cos(2 * math.pi * a / 256))) for a in range(256)]

    # ── Overflow audit. The runtime computes, per planet per substep:
    #        imul(imul(dx, k) >> 10, mass) >> 8
    #    |dx| <= d always (since d2 = dx^2 + dy^2), so imul(dx, k) <= C/d^2, maximised at the clamp.
    peak_dxk = max(int(d_of(i) * gacc[i]) for i in range(GACC_N))
    peak_mass = 1024                                  # Q8, i.e. 4.0 — the heaviest planet allowed
    peak_second = (peak_dxk >> 10) * peak_mass
    assert peak_dxk < 2**31, f"imul(dx,k) overflows i32: {peak_dxk}"
    assert peak_second < 2**31, f"imul(.,mass) overflows i32: {peak_second}"

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(f"""// GENERATED by scripts/gen_artillery_tables.py — do not edit.
//
// Five lookup tables that stand in for arithmetic the ARM7TDMI cannot do cheaply. See the generator
// for why each exists; the short version is that this chip has no FPU and no divide instruction, so
// a multiply between two i32 locals is a soft-float call and `C / d^3` is a software division per
// planet per substep.
//
// ⚠️ EVERY READ OF THESE MUST MASK ITS INDEX — `SQ[adx & 1023]`, never `SQ[adx]`. The identical
// promoted array costs 0.45 ticks through a masked index and 25 through any other form, because the
// compiler emits an f64 bounds-check fallback for the shapes it cannot reduce to a mask
// (examples/bench-tables/README.md §3). That is a 55x difference on the hottest read in the game.

// d^2 for |d| < {SQ_N}. Replaces `dx * dx`, which is an f64 multiply even between two i32 locals.
{rows('SQ', sq, 16)}

// C / d^3 with C = {C:,}, clamped at d >= {DMIN:g}. Index is the two-slope compression of d^2
// described in the generator: `d2 < {FINE_SPLIT}` -> `d2 >> 2`, else `{GACC_N // 2} | (d2 >> 8)`.
{rows('GACC', gacc, 8)}

// round(sqrt(i)) for i < {ISQRT_N}. Blast falloff is linear in distance, so it needs a real root.
{rows('ISQRT', isqrt, 24)}

// sin and cos in Q8 (256 = 1.0) over 256 angle units per turn. A turn is 256 units and not 360
// degrees so that an angle indexes these with a bare mask and never a division.
{rows('SINT', sint, 16)}

{rows('COST', cost, 16)}
""")

    # ── The audit block. Tuning happens by reading THIS, not by rebuilding the ROM. ──
    def accel_px(d, mass_q8):
        """The runtime's own arithmetic, in python, reported in px/frame^2."""
        i = (int(d * d) >> 2) if d * d < FINE_SPLIT else (GACC_N // 2 | (int(d * d) >> 8))
        k = gacc[min(i, GACC_N - 1)]
        return ((((int(d) * k) >> 10) * mass_q8) >> 8) / 65536.0

    print(f"wrote {OUT.relative_to(ROOT)}")
    print(f"  ROM: {(GACC_N + SQ_N + ISQRT_N + 512) * 4 / 1024:.1f} KB of i32 tables")
    print(f"  overflow headroom: imul(dx,k) peaks at {peak_dxk:,} ({2**31 / peak_dxk:.1f}x)")
    print(f"                     imul(.,mass) peaks at {peak_second:,} ({2**31 / peak_second:.1f}x)")
    print("\n  planet class      surface a     a @60px    escape v    a @120px")
    print("  " + "-" * 62)
    for name, r, mass in (("SMALL  r10 m192", 10, 192),
                          ("MEDIUM r16 m320", 16, 320),
                          ("LARGE  r24 m512", 24, 512)):
        surf = accel_px(r + 2, mass)
        a60 = accel_px(60, mass)
        a120 = accel_px(120, mass)
        # Escape speed from the surface, in px/frame.
        #
        # The runtime's radial acceleration is `((d*k) >> 10) * mass) >> 8` in Q16, and with
        # k = C/d^3 that is C*mass / (d^2 * 2^10 * 2^8 * 2^16) px/frame^2 — an inverse square whose
        # gravitational parameter is therefore GM = C*mass / 2^34. Escape speed is the usual
        # sqrt(2*GM/r). This is the number that says whether a planet can CAPTURE a shot: fire
        # slower than this near the surface and the shell spirals in instead of passing by.
        gm = C * mass / float(1 << 34)
        esc = math.sqrt(2 * gm / (r + 2))
        print(f"  {name}  {surf:8.3f}    {a60:8.4f}    {esc:7.2f}     {a120:8.5f}")
    print("\n  Read the `a @60px` column: a shot crossing the arena at 3 px/frame spends roughly")
    print("  80 frames within 60px of a planet, so an accel of ~0.01 px/frame^2 there bends it by")
    print("  ~0.8 px/frame — about a quarter of its speed. That is a visible curve, not a spiral.")


if __name__ == "__main__":
    main()
