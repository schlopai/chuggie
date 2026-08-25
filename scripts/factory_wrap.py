#!/usr/bin/env python3
"""Factory-wrap a tish module: move its module-level bindings into a factory function.

Every module-level binding is a permanent slot in the generated run() stack frame
(see the gba-run-frame notes and the large SRPG example's game.tish, now in the chuggie-tactics repo). On GBA that frame comes out of a
~29.7 KB IWRAM stack, and tish #655's stack guard trips when the resting SP sinks
into its 2 KB margin — so module growth eventually wedges the ROM at boot with an
"expected number" panic (the guard's parked RangeError eaten by a typed call site).

The wrap: imports stay; exported `let`/`const` DATA stays module-level (cross-module
writes and captures keep working); everything else moves into `function __make()`,
exported functions lose their `export` and are returned in an object; shims re-export
them. Only worth it when private bindings outnumber exports (each export costs a shim
slot + an object property) — the script refuses otherwise unless --force.

Usage: python3 scripts/factory_wrap.py <module.tish> [--force] [--dry-run]
"""
from __future__ import annotations

import re
import sys


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    force = "--force" in sys.argv
    dry = "--dry-run" in sys.argv
    path = args[0]
    src = open(path).read()
    if "__make()" in src:
        print(f"skip {path}: already wrapped")
        return 0
    lines = src.splitlines(keepends=True)

    # split off the import header (multi-line imports included)
    last_import = -1
    in_block = False
    for i, l in enumerate(lines):
        s = l.strip()
        if s.startswith("import "):
            last_import = i
            in_block = "from" not in s
        elif in_block:
            last_import = i
            if "from" in s:
                in_block = False
    head, body = lines[: last_import + 1], lines[last_import + 1 :]

    # hoist exported data bindings (stay module-level)
    hoisted, rest = [], []
    for l in body:
        if re.match(r"^export (let|const) ", l):
            hoisted.append(l)
        else:
            rest.append(l)

    # collect exported function signatures, then de-export them in the body
    sigs = []
    for l in rest:
        m = re.match(r"^export function (\w+)\(([^)]*)\)(: [\w\[\]]+)?\s*\{", l)
        if m:
            name, params, ret = m.group(1), m.group(2), m.group(3) or ""
            argnames = [p.split(":")[0].strip() for p in params.split(",") if p.strip()]
            sigs.append((name, params, ret, argnames))
    n_export = len(sigs) + len(hoisted)
    n_private = sum(1 for l in rest if re.match(r"^(function|let|const) ", l))
    if n_private < len(sigs) and not force:
        print(f"refuse {path}: private {n_private} < exported fns {len(sigs)} (see the "
              f"SRPG-example measurements — an export-heavy wrap makes the frame WORSE). --force to override.")
        return 1
    if dry:
        print(f"{path}: would wrap {n_private} private bindings, {len(sigs)} fn shims, "
              f"{len(hoisted)} data exports hoisted")
        return 0

    rest = [re.sub(r"^export function ", "function ", l) for l in rest]
    out = head + ["\n"] + hoisted
    out.append("\n// Factory-wrapped by scripts/factory_wrap.py — module-level bindings are\n")
    out.append("// permanent run()-frame slots on GBA; built in a frame that POPS instead.\n")
    out.append("// ⚠️ Do not hoist a function back to module scope — it buys back its slot.\n")
    out.append("function __make() {\n")
    out.extend(rest)
    out.append("  return { " + ", ".join(f"{n}: {n}" for n, _, _, _ in sigs) + " }\n")
    out.append("}\n")
    # Call the factory through a boxed indirection: a DIRECT call gets inlined by LLVM
    # right back into run() (measured: six small direct-call factories made the frame
    # WORSE and overflowed at init), while a value_call through an untyped array cell
    # is opaque to the inliner, so the bindings really do live in a frame that pops.
    out.append("let __mk = [__make]\n")
    out.append("const __M = __mk[0]()\n")
    for name, params, ret, argnames in sigs:
        out.append(f"export function {name}({params}){ret} {{ return __M.{name}({', '.join(argnames)}) }}\n")
    open(path, "w").write("".join(out))
    print(f"wrapped {path}: {n_private} private, {len(sigs)} shims, {len(hoisted)} data hoisted")
    return 0


if __name__ == "__main__":
    sys.exit(main())
