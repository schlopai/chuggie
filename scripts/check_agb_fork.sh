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
# So ask cargo directly. `cargo metadata` reports where each package was resolved from.
#
# WHAT COUNTS AS THE FORK depends on how .cargo/config.toml patches it, and that is read from the
# config rather than assumed, so the two cannot drift:
#
#   git = "..." + rev = "..."   cargo resolves agb with source `git+<url>?rev=<rev>`
#   path = "..."                cargo resolves it with source null and a manifest under that path
#
# It used to assume the second unconditionally and demand a sibling checkout at ../agb. When the
# patch became a pinned git rev — so CI stops cloning the fork — every verifier that calls this
# failed with "the agb fork is not checked out", while the build was in fact using the fork.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config="$here/.cargo/config.toml"

patch_line="$(grep -E '^\s*agb\s*=' "$config" 2>/dev/null | head -1)"
if [ -z "$patch_line" ]; then
  echo "FAIL: no agb entry in [patch.crates-io] in $config — nothing would use the fork"
  exit 1
fi

want_rev=""; fork=""
if printf '%s' "$patch_line" | grep -q 'git\s*='; then
  want_rev="$(printf '%s' "$patch_line" | sed -n 's/.*rev *= *"\([0-9a-f]*\)".*/\1/p')"
  if [ -z "$want_rev" ]; then
    echo "FAIL: agb is patched to a git source with no rev — pin it, or the ROMs change silently"
    exit 1
  fi
else
  rel="$(printf '%s' "$patch_line" | sed -n 's/.*path *= *"\([^"]*\)".*/\1/p')"
  fork="$(cd "$here/$rel" 2>/dev/null && pwd || true)"
  if [ -z "$fork" ]; then
    echo "FAIL: agb is patched to path $rel, which does not exist relative to $here"
    exit 1
  fi
fi

check_one() {   # $1 = example dir
  local ex="$1"
  local name
  name="$(basename "$ex")"
  # tish names the generated crate after the PACKAGE, not the directory, and for 10 of the examples
  # those differ (examples/minimal -> .tish/gba/tish-agb-minimal). Looking for a directory named
  # after the example meant this check silently reported "not built yet" for them and passed.
  local build
  build="$(find "$ex/.tish/gba" -maxdepth 2 -name Cargo.toml -print -quit 2>/dev/null)"
  build="${build%/Cargo.toml}"
  if [ -z "$build" ]; then
    echo "  skip $name (not built yet)"
    return 0
  fi
  local got
  # `source` is null for a path dependency and `git+<url>?rev=<rev>` for a git one; print both so
  # either kind of patch can be checked.
  got="$(cd "$build" && cargo metadata --format-version 1 --filter-platform thumbv4t-none-eabi 2>/dev/null \
    | python3 -c "
import json,sys
m = json.load(sys.stdin)
for p in m['packages']:
    if p['name'] == 'agb':
        print(p['source'] or '')
        print(p['manifest_path'])
        break
")"
  local src manifest
  src="$(printf '%s' "$got" | sed -n '1p')"
  manifest="$(printf '%s' "$got" | sed -n '2p')"
  if [ -z "$manifest" ]; then
    echo "  FAIL $name: cargo could not resolve agb at all"
    return 1
  fi

  if [ -n "$want_rev" ]; then
    case "$src" in
      *"spacedevin/agb"*"$want_rev"*)
        echo "  ok   $name -> $want_rev"
        return 0
        ;;
      *)
        echo "  FAIL $name resolved agb from ${src:-crates.io/a path}"
        echo "       expected the fork pinned at rev $want_rev — check [patch.crates-io] in $config"
        return 1
        ;;
    esac
  fi

  case "$manifest" in
    "$fork"/*)
      echo "  ok   $name -> $manifest"
      return 0
      ;;
    *)
      echo "  FAIL $name resolved agb from $manifest"
      echo "       expected the fork at $fork — check [patch.crates-io] in $config"
      return 1
      ;;
  esac
}

rc=0
if [ "${1:-}" = "--all" ]; then
  echo "agb fork (${want_rev:-$fork}):"
  for ex in "$here"/examples/*/; do
    [ -d "$ex/.tish" ] || continue
    check_one "${ex%/}" || rc=1
  done
else
  ex="${1:?usage: check_agb_fork.sh <example-dir> | --all}"
  [ -d "$ex" ] || { echo "FAIL: no such example dir: $ex"; exit 1; }
  echo "agb fork (${want_rev:-$fork}):"
  check_one "$(cd "$ex" && pwd)" || rc=1
fi
exit $rc
