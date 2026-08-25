#!/usr/bin/env bash
# One-time local dev setup.
#
# The examples depend on the tish CLI as a normal npm package (`@tishlang/tish`). In production that
# package is installed from npm and is fully self-contained. For LOCAL development against a sibling
# tish checkout — where the GBA target support may not be published yet — this script makes that
# checkout's in-repo npm package (`npm/tish`) usable as the `file:` dependency the examples reference:
#
#   1. builds the tish CLI (`target/release/tish`) if it isn't built,
#   2. materializes it as the npm package's platform binary (what `npm install` runs), and
#   3. self-contains the package the way `npm publish` does — the GBA build needs the tish source
#      workspace (the runtime/facade crates), which the published tarball bundles and we symlink.
#
# After this: `cd examples/<name> && npm install && npm start` (or `npm run build` / `npm run shot`).
# Idempotent — safe to re-run after pulling tish (re-materializes the binary). Leaves the tish repo's
# tracked files untouched (materialized paths go in its local .git/info/exclude).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"                    # chuggie-engine repo root
tish_repo="${TISH_REPO:-$(cd "$here/../tish/tish" 2>/dev/null && pwd || true)}"
if [ -z "$tish_repo" ] || [ ! -d "$tish_repo/crates/tish_runtime" ]; then
  echo "error: tish checkout not found." >&2
  echo "       expected a sibling at ../tish/tish, or set TISH_REPO=/path/to/tish (the dir with crates/tish_runtime)." >&2
  exit 1
fi
pkg="$tish_repo/npm/tish"

# 1) build the CLI if needed
bin="$tish_repo/target/release/tish"
if [ ! -x "$bin" ]; then
  echo "▶ building tish (release) ..."
  ( cd "$tish_repo" && cargo build --release -p tish )
fi

# 2) materialize it as the npm package's binary (postinstall copies platform/<os>-<arch> → bin/)
platform="$(node -e 'process.stdout.write(process.platform+"-"+process.arch)')"
mkdir -p "$pkg/platform/$platform"
cp "$bin" "$pkg/platform/$platform/tish"
cp "$bin" "$pkg/bin/tish"

# 3) self-contain the package (published pack copies these; locally we symlink to the real workspace)
ln -sfn ../../crates    "$pkg/crates"
ln -sfn ../../Cargo.toml "$pkg/Cargo.toml"
ln -sfn ../../justfile   "$pkg/justfile" 2>/dev/null || true

# 4) keep the tish repo clean — exclude the materialized paths locally (not a tracked .gitignore edit)
excl="$tish_repo/.git/info/exclude"
if [ -f "$excl" ] && ! grep -q "npm/tish/crates" "$excl"; then
  printf '\n# chuggie-engine local dev: @tishlang/tish package materialized for file: linking\nnpm/tish/crates\nnpm/tish/Cargo.toml\nnpm/tish/justfile\n' >> "$excl"
fi

echo "✓ local @tishlang/tish ready (from $tish_repo)"
echo "  next:  cd examples/<name> && npm install && npm start"
