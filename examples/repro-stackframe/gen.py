#!/usr/bin/env python3
"""Generate src/main.tish with a dialled-in number of module-level bindings and call depth.

The point is to find what makes the generated `run()`'s single stack frame grow, and how much of
the ~29 KB GBA IWRAM stack it eats — see docs/, and `framesize.py`. Full-game builds take ~5
minutes; this one builds in well under a minute, which is the whole reason it exists.

Usage: gen.py [n_bindings] [depth] [kind]
  kind: obj (module-level object literals) | scalar (i32 consts) | arr (typed arrays)
"""
import sys

n = int(sys.argv[1]) if len(sys.argv) > 1 else 200
depth = int(sys.argv[2]) if len(sys.argv) > 2 else 40
kind = sys.argv[3] if len(sys.argv) > 3 else "obj"

out = ["import { log, vblank } from 'cargo:tish_agb'", ""]

if kind == "struct":
    # A type alias whose RHS is an object shape: codegen emits a real Rust `TishStruct_K`, so
    # these bindings should NOT be boxed `Value`s at all. This is the comparison against `obj`.
    out.append("type K = { id: i32, x: i32, y: i32, on: i32 }")
    out.append("")

for i in range(n):
    if kind == "num":
        # No type annotation: the backend has a STATIC path for these (`static G_K0: SingleCore<..>`)
        # rather than a stack slot in run(). This is the comparison that matters.
        out.append(f"const K{i} = {i}")
    elif kind == "scalar":
        out.append(f"const K{i}: i32 = {i}")
    elif kind == "arr":
        out.append(f"const K{i}: i32[] = [{i}, {i + 1}, {i + 2}, {i + 3}]")
    elif kind == "struct":
        out.append(
            f"const K{i}: K = {{ id: {i}, x: {i * 2}, y: {i * 3}, on: 1 }}"
        )
    else:
        out.append(
            f'const K{i} = {{ id: {i}, name: "k{i}", x: {i * 2}, y: {i * 3}, on: 1 }}'
        )
out.append("")

# A boxed call chain `depth` deep. Each level touches a binding so nothing folds away, and the
# chain is mutually recursive-free so the backend cannot rotate it into a typed native fn.
for i in range(depth):
    tail = f"f{i + 1}(v + 1)" if i < depth - 1 else "v"
    if kind in ("scalar", "num"):
        touch = f"K{i % n}"
    elif kind == "struct":
        touch = f"K{i % n}.id"
    elif kind == "arr":
        touch = f"K{i % n}[0]"
    else:
        touch = f"K{i % n}.id"
    out.append(f"function f{i}(v) {{")
    out.append(f"  let t = {touch}")
    out.append(f"  if (v > 100000) {{ return t }}")
    out.append(f"  return {tail}")
    out.append("}")
out.append("")

out.append('log("start")')
out.append('log("chain=" + f0(0))')
out.append("let fr: i32 = 0")
out.append("while (fr < 60) { vblank() fr = fr + 1 }")
out.append('log("done")')

open("src/main.tish", "w").write("\n".join(out) + "\n")
print(f"wrote src/main.tish: {n} {kind} bindings, chain depth {depth}")
