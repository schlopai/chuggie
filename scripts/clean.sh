#!/usr/bin/env bash
# Reclaim every regenerable byte: per-example build trees, cargo targets, and node_modules.
#
#   npm run clean            # delete
#   npm run clean -- -n      # show what would go, and how big, without deleting
#
# ⚠️ AFTER A FULL CLEAN, RUN `npm run setup` BEFORE BUILDING ANYTHING. The examples depend on
# `@tishlang/tish` as a `file:` link into the sibling tish checkout, and that link lives in
# node_modules. Without it `tish build` is either missing or resolves to a STALE globally-installed
# binary — which does not fail loudly. It compiles, and then a handful of generated-Rust errors show
# up in code you did not touch. That cost most of a session once; the cause was never in the source.
#
# ⚠️ The crates under `crates/` are EXCLUDED from the root cargo workspace on purpose (see the note
# in Cargo.toml — proc-macro crates build for the host, the rest only for thumbv4t as path
# dependencies of a generated ROM crate). A root `cargo clean` therefore misses them entirely, and
# they are the second-biggest thing here. Each one is cleaned in its own directory.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

dry=0
case "${1:-}" in
  -n|--dry-run) dry=1 ;;
  "") ;;
  *) echo "usage: npm run clean [-- -n]" >&2; exit 2 ;;
esac

# du on a missing path is an error, and this script runs under `set -e`.
size() { [ -e "$1" ] && du -sh "$1" 2>/dev/null | cut -f1 || echo "-"; }

builds=()
for d in examples/*/.tish; do [ -d "$d" ] && builds+=("$d"); done
# `-prune` matters: without it find walks INTO a node_modules it just matched and reports the nested
# ones too, so the list is full of paths that vanish when their parent goes.
mods=()
while IFS= read -r d; do mods+=("$d"); done < <(find . -name node_modules -type d -maxdepth 3 -prune)
# ⚠️ `${a[@]+"${a[@]}"}`, not `"${a[@]}"`. macOS ships bash 3.2, where expanding an EMPTY array
# under `set -u` is an unbound-variable error — so a second `npm run clean` on an already-clean tree
# would abort instead of doing nothing.
targets=(${builds[@]+"${builds[@]}"} ${mods[@]+"${mods[@]}"})

if [ "$dry" = 1 ]; then
  echo "would delete:"
  # An `if`, not `[ .. ] && printf` — a false test makes that list return non-zero, and one `set -e`
  # rule change away from aborting the dry run on an already-clean tree.
  if [ ${#builds[@]} -gt 0 ]; then
    printf '  %6s  %d example build trees (examples/*/.tish)\n' \
      "$(du -shc "${builds[@]}" 2>/dev/null | tail -1 | cut -f1)" "${#builds[@]}"
  fi
  for d in ${mods[@]+"${mods[@]}"}; do printf '  %6s  %s\n' "$(size "$d")" "$d"; done
  echo "would run cargo clean in:"
  echo "      -  . (workspace)"
  for c in crates/*/Cargo.toml; do [ -f "$c" ] || continue
    printf '  %6s  %s\n' "$(size "$(dirname "$c")/target")" "$(dirname "$c")"
  done
  exit 0
fi

before=$(df -k . | awk 'NR==2 {print $4}')

for d in ${targets[@]+"${targets[@]}"}; do
  echo "rm  $d"
  rm -rf "$d"
done

echo "cargo clean  ."
cargo clean --quiet 2>/dev/null || true
for c in crates/*/Cargo.toml; do
  [ -f "$c" ] || continue
  d="$(dirname "$c")"
  echo "cargo clean  $d"
  (cd "$d" && cargo clean --quiet 2>/dev/null) || true
done

after=$(df -k . | awk 'NR==2 {print $4}')
echo
echo "reclaimed $(( (after - before) / 1024 )) MB"
echo "⚠️  run 'npm run setup' before your next build — node_modules held the tish link"
