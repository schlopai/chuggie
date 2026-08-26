#!/usr/bin/env python3
"""Give a log-only example a screen, so it can have a preview on the examples index.

The repro-* / bench-* / probe-* ROMs measure something and print numbers to the debug log. They
never draw, so they render a blank white frame forever and had no image on the index — no capture
setting can photograph a screen that was never painted.

This adds the smallest thing that makes them visible: a backdrop and two lines of HUD text naming
the ROM and its state. It is drawn BEFORE the measurement (several run for over a thousand frames,
and a readout that only appears at the end leaves the ROM blank for its whole useful life) and
updated after, so nothing new runs inside the loop under test and what each repro reproduces is
unchanged.

REFUSES a ROM that imports timer_read. "Nothing new runs inside the loop under test" is true, but
the readout still runs a backdrop, two hud_text calls and a frame() immediately BEFORE it, and a
bench that reads the hardware timer measures that: bench-tables came back with fill64=-64532 and a
one-frame fill of ~-194664 ticks — a wrapped counter, not a measurement — and its SANE gate failed.
A timing ROM is worth less with a preview than it is correct, so those keep their blank screen.

Idempotent. Usage: python3 scripts/add_screen_readout.py <example> [example ...]
"""
import json
import os
import re
import subprocess
import sys

MARK = "// --- screen readout (scripts/add_screen_readout.py) ---"
AGB_IMPORT = re.compile(r"import\s*\{([^}]*)\}\s*from\s*'cargo:tish_agb'", re.S)


def after_imports(src):
    """Index just past the last complete import statement — NOT just past the last `import` line.

    These files have multi-line imports; splitting on the import KEYWORD dropped the readout into
    the middle of one and broke four builds.
    """
    end = 0
    for m in re.finditer(r"^import\b.*?from\s*'[^']*'\s*$", src, re.M | re.S):
        end = m.end()
    return src.index("\n", end) + 1 if end else 0


def patch(name):
    path = f"examples/{name}/src/main.tish"
    if not os.path.exists(path):
        return "no src/main.tish (not a tish example)"
    src = open(path, encoding="utf-8").read()
    if MARK in src:
        return "already"
    if re.search(r"\btimer_read\b", src):
        return "reads the hardware timer — a readout would land inside its measurement"

    m = AGB_IMPORT.search(src)
    if not m:
        return "no cargo:tish_agb import"
    names = [n.strip() for n in m.group(1).split(",") if n.strip()]
    for need in ("backdrop", "hud_text", "frame"):
        if need not in names:
            names.append(need)
    src = f"{src[:m.start()]}import {{ {', '.join(names)} }} from 'cargo:tish_agb'{src[m.end():]}"

    if "font:" not in src:
        i = src.index("\n", src.index("from 'cargo:tish_agb'")) + 1
        src = src[:i] + "import { body } from 'font:../../../assets/fonts/tinypixel.ttf@7'\n" + src[i:]

    i = after_imports(src)
    src = src[:i] + (
        f"\n{MARK}\n"
        "// Drawn up front so the ROM is never a blank screen; the measurement below is untouched.\n"
        "backdrop(0x101018)\n"
        f'hud_text(0, 8, 8, "{name}")\n'
        'hud_text(1, 8, 24, "running - numbers go to the log")\n'
        "frame()\n"
    ) + src[i:]

    # A bare `while (true) {}` never yields, so nothing after it runs and no frame reaches the LCD.
    src = re.sub(r"while\s*\(\s*true\s*\)\s*\{\s*\}", "while (true) { frame() }", src)

    tail = '\nhud_text(1, 8, 24, "done - numbers are in the log")\nwhile (true) { frame() }\n'
    spin = re.search(r"\nwhile\s*\(\s*true\s*\)\s*\{\s*(?:frame|vblank)\(\)\s*\}\s*$", src)
    src = (src[:spin.start()] if spin else src.rstrip()) + tail
    open(path, "w", encoding="utf-8").write(src)

    # hud_text's font comes through the scenepack crate.
    pkg_path = f"examples/{name}/package.json"
    pkg = json.load(open(pkg_path))
    deps = pkg.setdefault("tish", {}).setdefault("rustDependencies", {})
    if "tish_gba_scenepack" not in deps:
        deps["tish_gba_scenepack"] = {"path": "../../crates/tish-gba-scenepack"}
        json.dump(pkg, open(pkg_path, "w"), indent=2)
        open(pkg_path, "a").write("\n")
    return "patched"


def main():
    ok, skipped, failed = [], [], []
    for name in sys.argv[1:]:
        r = patch(name)
        if r not in ("patched", "already"):
            skipped.append(f"{name}: {r}")
            continue
        b = subprocess.run(["npm", "run", "build"], cwd=f"examples/{name}",
                           capture_output=True, text=True)
        if b.returncode == 0:
            ok.append(name)
        else:
            err = [l for l in (b.stdout + b.stderr).splitlines() if "rror" in l]
            failed.append(f"{name}: {err[0][:100] if err else 'build failed'}")
    print(f"built {len(ok)}")
    for group, label in ((skipped, "skipped"), (failed, "FAILED")):
        if group:
            print(f"{label}:", *group, sep="\n  ")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
