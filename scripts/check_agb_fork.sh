#!/usr/bin/env bash
# Assert that a built example resolved `agb` to OUR FORK and not to crates.io.
#
#   scripts/check_agb_fork.sh <example-dir>        # e.g. examples/pong-link
#   scripts/check_agb_fork.sh --all               # every example that has been built
#
# WHY THIS EXISTS. Every example is supposed to build against the fork at ../agb, wired up by the
# `[patch.crates-io]` block in the repo-root .cargo/config.toml. Nothing enforced that. What you got
# instead was a WARNING, on every single build:
#
#   warning: patch `agb v0.25.0 (/Users/a_/Projects/agb/agb)` was not used in the crate graph
#
# which reads exactly like "your fork is being ignored" and is not that at all — `-Z build-std`
# creates a second resolve for core/alloc, that resolve genuinely has no `agb` in it, and cargo warns
# about the patch being unused THERE. The real graph does use the fork. But "a warning that always
# fires and does not mean what it says" is the worst possible guarantee: the day it starts meaning
# what it says, nobody will notice.
#
# So ask cargo directly. `cargo metadata` reports where each package was resolved from, and a package
# with `source: null` and a manifest outside the registry is a path dependency — the fork.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fork="$(cd "$here/../agb" 2>/dev/null && pwd || true)"

if [ -z "$fork" ]; then
  echo "FAIL: the agb fork is not checked out at $here/../agb"
  echo "      git clone https://github.com/spacedevin/agb $(dirname "$here")/agb"
  exit 1
fi

check_one() {   # $1 = example dir
  local ex="$1"
  local name
  name="$(basename "$ex")"
  local build="$ex/.tish/gba/$name"
  if [ ! -f "$build/Cargo.toml" ]; then
    echo "  skip $name (not built yet)"
    return 0
  fi
  local got
  got="$(cd "$build" && cargo metadata --format-version 1 --filter-platform thumbv4t-none-eabi 2>/dev/null \
    | python3 -c "
import json,sys
m = json.load(sys.stdin)
for p in m['packages']:
    if p['name'] == 'agb':
        print(p['manifest_path'])
        break
")"
  if [ -z "$got" ]; then
    echo "  FAIL $name: cargo could not resolve agb at all"
    return 1
  fi
  case "$got" in
    "$fork"/*)
      echo "  ok   $name -> $got"
      return 0
      ;;
    *)
      echo "  FAIL $name resolved agb from $got"
      echo "       expected the fork at $fork — check [patch.crates-io] in .cargo/config.toml"
      return 1
      ;;
  esac
}

rc=0
if [ "${1:-}" = "--all" ]; then
  echo "agb fork ($fork):"
  for ex in "$here"/examples/*/; do
    [ -d "$ex/.tish" ] || continue
    check_one "${ex%/}" || rc=1
  done
else
  ex="${1:?usage: check_agb_fork.sh <example-dir> | --all}"
  [ -d "$ex" ] || { echo "FAIL: no such example dir: $ex"; exit 1; }
  echo "agb fork ($fork):"
  check_one "$(cd "$ex" && pwd)" || rc=1
fi
exit $rc
