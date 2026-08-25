#!/usr/bin/env python3
"""Fail if any tish call passes the wrong number of arguments.

WHY THIS EXISTS
---------------
tish does not check call arity. A call with too few arguments compiles, and the missing parameter
arrives as `Value::Null`; a typed prologue then does

    match &args.get(1)... { Value::Number(n) => ..., _ => panic!("expected number") }

so the ROM builds cleanly and hard-panics the first time that line runs. There is no compile error,
no warning, and no symptom until the code path executes.

THE BUG THAT MOTIVATED IT. `packages/drop_modes.tish` called `drop_paint(P2)` — one argument to a
two-argument function — for the VS mode's second board. It was invisible for a long time for a
reason worth remembering: the ORIGINAL `drop_paint` was a Rust extern whose second parameter was
`_rv`, unused, because the native painter did its own version check. So the missing argument
genuinely did not matter. When the rules were ported to tish the painter started USING `rv` as its
skip gate, and the same untouched call site became a crash — in the one mode whose ROM had no
verifier. A mechanical port that changes nothing about a call site can still change what that call
site means.

WHAT IT CHECKS
--------------
Every `function name(params)` it can see becomes a signature; every `name(args)` that is not a
declaration, not a method call (`obj.name(...)`) and not inside a comment or string becomes a call.

A name resolves the way the compiler resolves it: the file must DECLARE it, or IMPORT it from a file
that declares it. `cargo:` / `font:` / npm specifiers resolve to nothing and are never checked. Both
halves of that rule earn their keep — `line` is a four-parameter function in `game.tish` and a
two-parameter local in `ui.tish`, and `swing` is a six-parameter Rust extern in `engine.tish` and an
unrelated zero-parameter local in `battle.tish`. Matching on the name alone called all of those bugs.

A parameter is OPTIONAL if it has a default (`= expr`) or if the body guards it — `present(v)`,
`pick(v, d)`, or a null comparison. That is how this codebase declares an optional argument; tish has
no `?` and no package uses a default. Omitting a guarded parameter is correct and is not reported.

Failures are only the ones that CRASH: a missing argument whose parameter is TYPED. Anything else —
an unguarded omission into an untyped parameter, or extra arguments (silently dropped) — is a note,
and `--strict` fails on those too.

Usage:  python3 scripts/arity_check.py [--self-test] [--strict] FILE_OR_DIR...
        python3 scripts/arity_check.py packages/ examples/foo/src/
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

KEYWORDS = {
    "if", "while", "for", "switch", "return", "function", "catch", "match",
    "typeof", "new", "await", "and", "or", "not", "in", "of", "as", "let", "const",
}

DECL = re.compile(r"(?:^|\n)\s*(?:export\s+)?function\s+(\w+)\s*\(([^)]*)\)")
CALL = re.compile(r"(?<![.\w])(\w+)\s*\(")


def strip_noise(text: str) -> str:
    """Blank out comments and string literals, preserving offsets so line numbers stay right.

    Offsets are preserved rather than the text removed, because the whole value of this check is a
    file:line a reader can open — and a comment that mentions `drop_paint(P2)` in prose is exactly
    the kind of thing that would otherwise be reported as the bug.

    ⚠️ Comments become SPACES and strings become `s`. That difference is load-bearing: blanking a
    string to spaces turns `col2("selected")` into `col2(          )`, which counts as ZERO
    arguments, and the first version of this file reported 145 findings in packages/ that were all
    that one mistake. A string is an argument; it just must not contribute commas or brackets.
    """
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i + 1] == "/":
            while i < n and text[i] != "\n":
                out[i] = " "
                i += 1
        elif c == "/" and i + 1 < n and text[i + 1] == "*":
            while i < n and not (text[i] == "*" and i + 1 < n and text[i + 1] == "/"):
                if text[i] != "\n":
                    out[i] = " "
                i += 1
            for _ in range(2):
                if i < n:
                    out[i] = " "
                    i += 1
        elif c in "\"'`":
            quote = c
            out[i] = "s"
            i += 1
            while i < n and text[i] != quote:
                if text[i] == "\\":
                    out[i] = "s"
                    i += 1
                if i < n:
                    if text[i] != "\n":
                        out[i] = "s"
                    i += 1
            if i < n:
                out[i] = "s"
                i += 1
        else:
            i += 1
    return "".join(out)


def split_top(s: str) -> int:
    """Count top-level comma-separated items in an argument or parameter list."""
    if not s.strip():
        return 0
    depth, n = 0, 1
    for ch in s:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 0:
            n += 1
    return n


def balanced_args(text: str, open_paren: int):
    """The text between `(` at open_paren and its match, or None if unbalanced."""
    depth = 0
    for j in range(open_paren, len(text)):
        if text[j] in "([{":
            depth += 1
        elif text[j] in ")]}":
            depth -= 1
            if depth == 0:
                return text[open_paren + 1:j]
    return None


# Parsed from the RAW text, before strip_noise mangles the module specifier — the specifier is the
# whole point. `engine.tish` imports `swing` from `cargo:tish_gba_game_engine` (a six-parameter Rust
# extern) while `battle.tish` declares an unrelated local `function swing()`. Matching on the NAME
# alone reported the correct call as a bug.
IMPORT = re.compile(r"import\s*\{([^}]*)\}\s*from\s*['\"]([^'\"]+)['\"]", re.S)

# How this codebase says "this argument may be absent". There is no `?` in tish and no package uses
# a default parameter, so the convention lives in the body: `present(v)` (0 when null), `pick(v, d)`
# (d when null), or a plain null comparison. A parameter guarded that way is optional BY DESIGN and
# a call that omits it is correct — reporting it teaches people to ignore this checker.
GUARD = [
    r"\bpresent\s*\(\s*{p}\s*\)",
    r"\bpick\s*\(\s*{p}\s*,",
    r"\b{p}\s*(?:===|!==|==|!=)\s*null",
    r"\bnull\s*(?:===|!==|==|!=)\s*{p}\b",
]


def body_after(clean: str, start: int) -> str:
    """The braced body of a declaration whose text begins at `start`, or '' if unbalanced."""
    open_brace = clean.find("{", start)
    if open_brace < 0:
        return ""
    depth = 0
    for j in range(open_brace, len(clean)):
        if clean[j] == "{":
            depth += 1
        elif clean[j] == "}":
            depth -= 1
            if depth == 0:
                return clean[open_brace:j]
    return clean[open_brace:]


def signatures(files):
    """Per-file signature tables, keyed by resolved path.

    Deliberately NOT one global map. `line` is declared in `game.tish` with four parameters and
    also exists as a two-parameter local in `ui.tish`; a global map reports every correct local call
    as a bug. A name resolves in a file only if that file DECLARES it or IMPORTS it FROM THAT FILE —
    the same rule the compiler uses.
    """
    per_file = {}
    for f in files:
        clean = strip_noise(f.read_text(encoding="utf-8"))
        decls = {}
        for m in DECL.finditer(clean):
            params = [p for p in m.group(2).split(",") if p.strip()]
            names = [p.split("=")[0].split(":")[0].strip() for p in params]
            body = body_after(clean, m.end())
            optional, typed = [], []
            for p, name in zip(params, names):
                guarded = any(re.search(g.format(p=re.escape(name)), body) for g in GUARD)
                optional.append("=" in p or guarded)
                # TYPED (`x: i32`) is the whole difference between a crash and a shrug.
                typed.append(":" in p.split("=")[0])
            decls[m.group(1)] = (optional, typed, f.name)
        per_file[f.resolve()] = decls
    return per_file


def import_map(raw: str, this_file: Path):
    """`{name: resolved Path | None}` for every `import { … } from '…'` in this file.

    `None` means the name comes from somewhere this checker cannot see — `cargo:`, `font:`,
    `sheet:`, an npm package — and must NOT be matched against a same-named local declaration
    elsewhere in the corpus. That was the `swing` false positive: a six-parameter Rust extern and a
    zero-parameter local helper in another package, reported as a wrong call because only the name
    was compared.
    """
    out = {}
    for m in IMPORT.finditer(raw):
        spec = m.group(2)
        target = None
        if spec.startswith("."):
            p = (this_file.parent / spec).resolve()
            if p.suffix != ".tish":
                p = p.with_suffix(".tish")
            target = p
        for part in m.group(1).split(","):
            name = part.strip().split(" as ")[-1].strip()
            if name:
                out[name] = target
    return out


def check(files, per_file):
    bad = []
    for f in files:
        raw = f.read_text(encoding="utf-8")
        clean = strip_noise(raw)
        local = per_file[f.resolve()]
        imports = import_map(raw, f)
        for m in CALL.finditer(clean):
            name = m.group(1)
            if name in KEYWORDS:
                continue
            if name in local:
                sig = local[name]
            elif name in imports:
                target = imports[name]
                # External module (cargo:/font:/npm) — nothing here can be its signature.
                if target is None or target not in per_file or name not in per_file[target]:
                    continue
                sig = per_file[target][name]
            else:
                continue
            before = clean[max(0, m.start() - 12):m.start()]
            if re.search(r"\bfunction\s+$", before):
                continue
            args = balanced_args(clean, m.end() - 1)
            if args is None:
                continue
            n = split_top(args)
            optional, typed, src = sig
            hi = len(optional)
            required = max((i + 1 for i, o in enumerate(optional) if not o), default=0)
            if n == hi or (required <= n < hi and all(optional[n:])):
                continue
            line = clean.count("\n", 0, m.start()) + 1
            want = f"{required}" if required == hi else f"{required}..{hi}"
            if n < hi:
                # A missing argument arrives as `Value::Null`. An UNTYPED parameter accepts that
                # happily and several packages omit one on purpose — but only where the body GUARDS
                # it (`present(bg)`, `pick(v, d)`, `v === null`); an unguarded omission is a real
                # question. A TYPED parameter cannot accept null at all: its prologue matches on
                # `Value::Number` and panics. Only that is a crash, and only that fails a build.
                missing = [i for i in range(n, hi) if not optional[i]]
                fatal = any(typed[i] for i in missing)
                kind = "crash" if fatal else "soft"
            else:
                # Extra arguments are dropped silently. Usually a stale call, never a panic.
                fatal, kind = False, "extra"
            bad.append((f, line, name, n, want, src, kind, fatal))
    return bad


def gather(args):
    files = []
    for a in args:
        p = Path(a)
        if p.is_dir():
            files.extend(sorted(p.rglob("*.tish")))
        elif p.is_file():
            files.append(p)
        else:
            raise SystemExit(f"arity_check: {a}: no such file or directory")
    return files


def self_test() -> int:
    import tempfile

    ok = True
    # (name, source, expected fatal count, expected total findings)
    cases = [
        ("the real bug — one argument short of a TYPED parameter",
         "export function drop_paint(slot: i32, rv: i32): i32 { return 0 }\n"
         "function go() { drop_paint(P2) }\n", 1, 1),
        ("short of an UNTYPED parameter is a null, not a crash",
         "function renderTab(defer, stream) {}\nfunction go() { renderTab() }\n", 0, 1),
        ("short of an untyped param AFTER a typed one is still soft",
         "function f(a: i32, bg) {}\nfunction go() { f(1) }\n", 0, 1),
        ("a STRING is an argument, not an empty one",
         'function col2(v) {}\nfunction go() { col2("selected") }\n', 0, 0),
        ("a comma INSIDE a string is not an argument separator",
         'function f(a) {}\nfunction go() { f("x,y,z") }\n', 0, 0),
        ("too many arguments is reported but never fatal",
         "function f(a) {}\nfunction go() { f(1, 2) }\n", 0, 1),
        ("a correct call", "function f(a, b) {}\nfunction go() { f(1, 2) }\n", 0, 0),
        ("a nested call as one argument",
         "function f(a, b) {}\nfunction g(x) { return x }\n"
         "function go() { f(1, g(2)) }\n", 0, 0),
        ("a default parameter makes the argument optional",
         "function f(a, b = 3) {}\nfunction go() { f(1) }\n", 0, 0),
        # The three ways this codebase says "may be absent". No package uses a default parameter,
        # so if these are not honoured the checker reports every deliberate short call and gets
        # ignored — which is worse than not having it.
        ("present(v) in the body makes a parameter optional",
         "function present(v) { if (v === null) { return 0 } return 1 }\n"
         "function renderTab(defer, stream) {\n"
         "  if (present(defer) === 1) {}\n  if (present(stream) === 1) {}\n}\n"
         "function go() { renderTab() }\n", 0, 0),
        ("pick(v, d) in the body makes a parameter optional",
         "function pick(v, d) { return v }\n"
         "function f(a, b) { let x = pick(b, 3) }\nfunction go() { f(1) }\n", 0, 0),
        ("a null comparison in the body makes a parameter optional",
         "function f(a, bg) { if (bg !== null) {} }\nfunction go() { f(1) }\n", 0, 0),
        ("an UNGUARDED short call is still reported",
         "function f(a, b) { return b }\nfunction go() { f(1) }\n", 0, 1),
        ("a call named in a COMMENT is not a call",
         "function f(a, b) {}\n// f(1) would be wrong\nfunction go() { f(1, 2) }\n", 0, 0),
        ("a call named in a STRING is not a call",
         'function f(a, b) {}\nfunction go() { log("f(1)"); f(1, 2) }\n', 0, 0),
        ("a method call is not this function",
         "function f(a, b) {}\nfunction go() { obj.f(1) }\n", 0, 0),
        ("an unknown name is ignored, not guessed at",
         "function go() { somethingImported(1, 2, 3) }\n", 0, 0),
    ]
    with tempfile.TemporaryDirectory() as d:
        for name, src, want_fatal, want_all in cases:
            p = Path(d) / "case.tish"
            p.write_text(src, encoding="utf-8")
            found = check([p], signatures([p]))
            n_fatal = sum(1 for r in found if r[7])
            if (n_fatal, len(found)) == (want_fatal, want_all):
                print(f"  self-test ok   — {name}")
            else:
                print(f"  self-test FAIL — {name}: expected {want_fatal} fatal of {want_all}, "
                      f"got {n_fatal} of {len(found)}")
                ok = False

    # Cross-file cases, which need more than one file and so cannot go in the table above.
    multi = [
        # The `swing` false positive: a six-parameter Rust extern in one file, an unrelated
        # zero-parameter local in another. Name-only matching called the correct call a bug.
        ("an extern from cargo: is not a same-named local in another file", {
            "battle.tish": "function swing() { return 1 }\n",
            "engine.tish": "import { swing } from 'cargo:tish_gba_game_engine'\n"
                           "function go() { swing(1, 2, 3, 4, 5, 6) }\n",
        }, 0, 0),
        # …but a real cross-file import still resolves and is still checked.
        ("a relative import DOES resolve, and is checked", {
            "core.tish": "export function paint(slot: i32, rv: i32) { return 0 }\n",
            "modes.tish": "import { paint } from './core'\nfunction go() { paint(1) }\n",
        }, 1, 1),
    ]
    for name, files, want_fatal, want_all in multi:
        with tempfile.TemporaryDirectory() as d:
            paths = []
            for fn, src in files.items():
                p = Path(d) / fn
                p.write_text(src, encoding="utf-8")
                paths.append(p)
            found = check(paths, signatures(paths))
            n_fatal = sum(1 for r in found if r[7])
            if (n_fatal, len(found)) == (want_fatal, want_all):
                print(f"  self-test ok   — {name}")
            else:
                print(f"  self-test FAIL — {name}: expected {want_fatal} fatal of {want_all}, "
                      f"got {n_fatal} of {len(found)}")
                ok = False
    return 0 if ok else 1


def main() -> int:
    argv = sys.argv[1:]
    if "--self-test" in argv:
        return self_test()
    if not argv:
        print("usage: arity_check.py [--self-test] FILE_OR_DIR...")
        return 2
    files = gather([a for a in argv if not a.startswith("-")])
    if not files:
        print("arity_check: no .tish files in", " ".join(argv))
        return 2
    strict = "--strict" in argv
    bad = check(files, signatures(files))
    fatal = [r for r in bad if r[7]]
    soft = [r for r in bad if not r[7]]

    for f, line, name, n, want, src, kind, _ in fatal:
        print(f"FAIL: {f}:{line}  {name}() called with {n} argument(s); {src} declares {want} "
              f"— the missing one is TYPED, so this panics the moment it runs")
    for f, line, name, n, want, src, kind, _ in soft:
        tag = "extra argument(s), silently dropped" if kind == "extra" \
            else "missing argument(s) arrive as null (untyped, so no panic)"
        print(f"{'FAIL' if strict else 'note'}: {f}:{line}  {name}() called with {n}; "
              f"{src} declares {want} — {tag}")

    if fatal or (strict and soft):
        n = len(fatal) + (len(soft) if strict else 0)
        print(f"\n{n} call(s) with the wrong arity. tish does not check this: a missing argument")
        print("arrives as null and panics inside the callee's typed prologue, at runtime, only on")
        print("the frame that path first runs.")
        return 1
    if soft:
        print(f"\n{len(soft)} soft mismatch(es) above — no panic, but check each is deliberate. "
              f"--strict fails on them.")
    print(f"arity_check: {len(files)} file(s), no call can panic on a missing typed argument")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
