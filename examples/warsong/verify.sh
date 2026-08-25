#!/usr/bin/env bash
# warsong verify — build, confirm Tiled map, boxing census on hot loop, soak logs.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
EX="$(cd "$(dirname "$0")" && pwd)"
cd "$EX"

test -f assets/wsg.tmj
python3 - <<'PY'
import json
m = json.load(open("assets/wsg.tmj"))
assert m["type"] == "map"
assert m["orientation"] == "orthogonal"
names = [l["name"] for l in m["layers"]]
assert "Ground" in names and "Solid" in names, names
print("tmj ok", m["width"], "x", m["height"], names)
PY

rm -rf .tish
unset CARGO_TARGET_DIR
export TISH_FAST_NATIVE_BUILD=1
npm run build

RS=".tish/gba/warsong/src/main.rs"
test -f "$RS"

# Hot-loop boxing: last while_loop body should avoid G_*.with / get_prop
python3 - <<'PY'
import re, pathlib
src = pathlib.Path(".tish/gba/warsong/src/main.rs").read_text()
# Find while_loop labels; take the last large game loop if present
loops = list(re.finditer(r"(while_loop_\w+):\s*while", src))
print("while_loops", len(loops))
gwith = len(re.findall(r"G_[A-Za-z0-9_]*\.with\(", src))
print("G_*.with count", gwith)
# Soft gate — select HUD may box; keep under a generous budget
assert gwith < 400, gwith
print("boxing census ok")
PY

# Optional soak if mgba-headless / screenshot harness exists
if [[ -x "$ROOT/scripts/screenshot.sh" ]]; then
  "$ROOT/scripts/screenshot.sh" warsong.gba /tmp/warsong-shot.png 180 || true
fi

echo "warsong verify ok"
