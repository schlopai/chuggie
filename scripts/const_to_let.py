#!/usr/bin/env python3
"""Give tish's module-level scalars an `i32` type, which is worth ~20-25% of a frame.

    python3 scripts/const_to_let.py --check packages examples      # report, change nothing
    python3 scripts/const_to_let.py packages/foo.tish              # rewrite in place

── Why ──────────────────────────────────────────────────────────────────────────────────────────
A tish `const` is not a compile-time literal. It compiles to a thread-local `Cell<f64>`:

    // const A_X = 5
    static G_A_X: SingleCore<Cell<f64>> = SingleCore::new(Cell::new(5_f64));
    // and `A[b + A_X]` becomes
    let __bi = (((b) as f64) + G_A_X.with(|c| c.get())) as usize;

That is an i32->f64 conversion, an f64 add and an f64->usize conversion — three SOFT-FLOAT
operations per array access, on an ARM7TDMI, which has no FPU. Written `let A_X: i32 = 5` it is a
`VmRef<i32>` and the same expression is `(b).wrapping_add(A_X)`: native integers.

The same is true of arrays. `const XS = [1, 2, 3]` — and even `const XS: i32[] = [1, 2, 3]` — is a
boxed `Value::Array` of boxed `Value::Number(f64)`; `let XS: i32[] = [1, 2, 3]` is a `Vec<i32>`.

Measured on device: converting one package plus its game took the tick from 7,800 Timer2 ticks to
5,700 and the worst frame from 12,400 to 10,000, against a 4,389-tick 60fps budget. No logic
changed. It is the cheapest performance work available in this codebase.

── What this tool will and will not touch ───────────────────────────────────────────────────────
Converted:
  * `const N = <integer expression>`                -> `let N: i32 = ...`
  * `const N = [<all integer literals>]`            -> `let N: i32[] = [...]`
  * `const N: i32[] = [...]`                        -> `let N: i32[] = [...]`
  * `const A = 1, B = 2`                            -> one `let` per line
  * `let N = <integer literal>` (UNTYPED)           -> `let N: i32 = ...`
  * `export const` / `export let` keep their `export`.

⚠️ It is the TYPE ANNOTATION that matters, not the keyword. An untyped `let N = 6` is exactly as
soft-float as a `const` — `packages/shmup.tish` had 44 of them left after the first pass, all
`let POWERUP_SHIELD = 6` and friends.

Left alone, deliberately:
  * strings, floats, object literals, mixed/other-typed arrays, and anything whose value is a CALL
    (`const bg = bg_new(...)`) — a handle is not a constant and the annotation would be a lie.
  * indented (function-local) declarations, which are not module state.

⚠️ Verify by BUILDING. The conversion is mechanical but tish will accept a wrong type annotation
and fail later, and a `const` you convert becomes writable — nothing in tree relies on the
immutability, but a future reader might.
"""
import os
import re
import sys

DECL = re.compile(r'^(export )?const ([A-Za-z_][A-Za-z0-9_]*)(\s*:\s*[A-Za-z0-9_\[\]]+)?\s*=\s*(.+?)(\s*//.*)?$')
INT_EXPR = re.compile(r'^[0-9A-Za-z_ ()+\-*/<>|&^]+$')
FLOAT = re.compile(r'\d+\.\d+')
CALL = re.compile(r'\w\s*\(')
INT_ARRAY = re.compile(r'^\[\s*(?:0\s*-\s*)?\d+(?:\s*,\s*(?:0\s*-\s*)?\d+)*\s*,?\s*\]$')
UNTYPED_LET = re.compile(r'^(export )?let ([A-Za-z_][A-Za-z0-9_]*)\s*=\s*((?:0\s*-\s*)?\d+|0[xX][0-9a-fA-F]+)\s*(//.*)?$')


def convert_line(line, float_assigned=frozenset()):
    """Return the rewritten line, or None to leave it alone."""
    # An untyped module-level `let` holding an integer is the same soft-float trap as a `const`.
    m = UNTYPED_LET.match(line.rstrip("\n"))
    if m and m.group(2) not in float_assigned:
        tail = "  " + m.group(4) if m.group(4) else ""
        return "%slet %s: i32 = %s%s\n" % (m.group(1) or "", m.group(2), m.group(3), tail)

    m = DECL.match(line.rstrip("\n"))
    if not m:
        return None
    exp, name, ann, rhs, comment = m.group(1) or "", m.group(2), m.group(3), m.group(4).strip(), m.group(5) or ""

    # `const A = 1, B = 2` — split, so each gets its own annotation.
    if ann is None and "," in rhs and not CALL.search(rhs) and not rhs.startswith("["):
        parts = [p.strip() for p in rhs.split(",")]
        out = []
        first = re.match(r'^(\d+|0\s*-\s*\d+)$', parts[0])
        if not first:
            return None
        out.append("%slet %s: i32 = %s" % (exp, name, parts[0]))
        for p in parts[1:]:
            mm = re.match(r'^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)$', p)
            if not mm or not re.match(r'^(\d+|0\s*-\s*\d+)$', mm.group(2).strip()):
                return None
            out.append("%slet %s: i32 = %s" % (exp, mm.group(1), mm.group(2).strip()))
        out[-1] += comment
        return "\n".join(out) + "\n"

    if '"' in rhs or "'" in rhs:
        return None
    if INT_ARRAY.match(rhs):
        return "%slet %s: i32[] = %s%s\n" % (exp, name, rhs, comment)
    if rhs.startswith("[") or rhs.startswith("{"):
        return None
    if FLOAT.search(rhs) or CALL.search(rhs):
        return None
    if not INT_EXPR.match(rhs):
        return None
    if ann is not None and ann.strip() not in (": i32",):
        return None
    return "%slet %s: i32 = %s%s\n" % (exp, name, rhs, comment)


# ⚠️ Never annotate a name the file later assigns a FLOAT to — `: i32` would silently truncate it.
FLOAT_ASSIGN = re.compile(r'^\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?:=|\+=|-=|\*=)\s*[^=].*?\d+\.\d+')


def sweep(path, check):
    src = open(path).read()
    floats = set(m.group(1) for m in FLOAT_ASSIGN.finditer(src))
    out, n = [], 0
    for line in src.splitlines(keepends=True):
        new = convert_line(line, floats)
        if new is None:
            out.append(line)
        else:
            out.append(new)
            n += 1
    if n and not check:
        open(path, "w").write("".join(out))
    return n


def main(argv):
    check = "--check" in argv
    roots = [a for a in argv if not a.startswith("--")] or ["packages", "examples"]
    files = []
    for r in roots:
        if os.path.isfile(r):
            files.append(r)
            continue
        for dirpath, dirnames, names in os.walk(r):
            dirnames[:] = [d for d in dirnames if d not in (".tish", "node_modules", "_vendor")]
            files += [os.path.join(dirpath, f) for f in names if f.endswith(".tish")]
    total = 0
    for f in sorted(files):
        n = sweep(f, check)
        if n:
            total += n
            print("%5d  %s" % (n, f))
    print("%s %d declaration(s) in %d file(s)" %
          ("would convert" if check else "converted", total, len(files)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
