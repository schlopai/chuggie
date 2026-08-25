#!/usr/bin/env bash
# Time GBA cargo profiles + report codegen line counts. Does NOT rebuild any full game.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$(cd ../../../tish/tish/target/release && pwd):$PATH"
unset CARGO_TARGET_DIR

echo "tish: $(command -v tish)"
echo

lines_of() {
  local f=".tish/gba/$1/src/main.rs"
  if [ -f "$f" ]; then wc -l <"$f" | tr -d ' '; else echo "?"; fi
}

profile_of() {
  local f=".tish/gba/$1/Cargo.toml"
  if [ -f "$f" ]; then
    rg -n '^(opt-level|lto|codegen-units|incremental|debug)' "$f" || true
  fi
}

# usage: run_one LABEL STEM SRC [ENV_ASSIGN ...]
run_one() {
  local label="$1" stem="$2" src="$3"
  shift 3
  rm -rf ".tish/gba/$stem"
  # tish names the build dir after the output stem
  local out="${stem}.gba"
  echo "== $label =="
  local start end
  start=$(date +%s)
  # shellcheck disable=SC2086
  env -u CARGO_TARGET_DIR "$@" tish build "$src" --target gba -o "$out" 2>&1 \
    | rg -i "Built:|error\[|error:" | head -30
  end=$(date +%s)
  # build dir uses output file stem
  echo "wall_sec $((end - start))"
  echo "main_rs_lines $(lines_of "$stem")"
  profile_of "$stem" | sed 's/^/  /'
  echo
}

chmod +x run.sh 2>/dev/null || true
npm install --silent >/dev/null 2>&1 || true

run_one "engine_only FAST" engine_only_fast src/engine_only.tish TISH_FAST_NATIVE_BUILD=1
run_one "engine_only DEFAULT (thin LTO)" engine_only src/engine_only.tish
run_one "engine_only FAT LTO" engine_only_fat src/engine_only.tish TISH_GBA_FAT_LTO=1
run_one "with_ui DEFAULT" with_ui src/with_ui.tish

echo "== summary =="
printf '%-28s %8s\n' target lines
for s in engine_only_fast engine_only engine_only_fat with_ui; do
  printf '%-28s %8s\n' "$s" "$(lines_of "$s")"
done
