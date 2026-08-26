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
#   3. links the tish source workspace into it, because a GBA build needs the runtime/facade crates.
#
# After this: `cd examples/<name> && npm install && npm start` (or `npm run build` / `npm run shot`).
# Idempotent — safe to re-run after pulling tish (re-materializes the binary).
#
# ⚠️ THIS IS A LOCAL CONVENIENCE, NOT HOW THE PACKAGE IS BUILT. An earlier version of this script
# also appended those symlinks to the tish repo's .git/info/exclude — a local, per-clone file that
# no .gitignore and no review shows. The paths were therefore untracked AND invisible, so a tarball
# packed in CI shipped none of them while a tarball packed on a machine that had run this script
# shipped all 33 crates. Nobody could see the difference. tish's own prepack now copies the payload
# from its repo root, so nothing here is load-bearing for a release; this script no longer writes to
# another repository's git configuration.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"                    # chuggie repo root
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

# 3) link the tish source workspace in, so a local GBA build can path-depend on the runtime crates.
#    No `2>/dev/null || true` here: if a link cannot be made, the next `npm run build` fails with a
#    confusing cargo error instead, so say it now.
for link in crates Cargo.toml justfile; do
  if [ ! -e "$tish_repo/$link" ]; then
    echo "error: $tish_repo/$link does not exist — is TISH_REPO pointing at a tish checkout?" >&2
    exit 1
  fi
  ln -sfn "../../$link" "$pkg/$link"
done

# These links are untracked in the tish repo. Say so, rather than hiding them in that repo's
# .git/info/exclude the way this script used to — `git status` showing them is the point.
cat <<NOTE
note: linked crates/, Cargo.toml and justfile into $pkg.
      They are untracked there, so \`git status\` in the tish repo will list them. That is
      deliberate: they are local-dev scaffolding, and hiding them is what let a broken npm
      payload ship unnoticed. Remove them with:
        rm -f $pkg/crates $pkg/Cargo.toml $pkg/justfile
NOTE

echo "✓ local @tishlang/tish ready (from $tish_repo)"
echo "  next:  cd examples/<name> && npm install && npm start"
