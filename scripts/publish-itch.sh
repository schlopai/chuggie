#!/usr/bin/env bash
# Bundle a GBA ROM + mGBA WASM HTML player into an itch.io HTML5 package.
#
# Usage:
#   scripts/publish-itch.sh examples/shmup
#   scripts/publish-itch.sh path/to/game.gba --name shmup [--frames N]
#
# Env:
#   ITCH_TARGET=user/game:html5   if set, also `butler push` the html5/ directory
#   TISH=...                      tish CLI when building from source (see screenshot.sh)
#
# Outputs under dist/itch/<name>/:
#   html5/                  playable tree (index.html at root)
#   <name>-html5.zip        for manual itch upload
#   screenshot.png          raw 240×160 capture
#   cover.png               630×500 letterboxed (itch cover)
#   embed-bg.png            480×320 (click-to-play frame background)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
template="$here/templates/itch-mgba"
frames=180
name=""
input=""

usage() {
  cat >&2 <<'EOF'
usage: scripts/publish-itch.sh <examples/NAME | rom.gba> [--name NAME] [--frames N]

  Package a GBA ROM with the mGBA WASM HTML player for itch.io.
  Prefer:  npm run itch -- publish <example>
EOF
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    --name)   name="${2:?}"; shift 2 ;;
    --frames) frames="${2:?}"; shift 2 ;;
    -h|--help) usage ;;
    -*)
      echo "error: unknown flag: $1" >&2
      usage
      ;;
    *)
      if [ -n "$input" ]; then
        echo "error: unexpected argument: $1" >&2
        usage
      fi
      input="$1"
      shift
      ;;
  esac
done

[ -n "$input" ] || usage
[ -d "$template" ] || { echo "error: missing template: $template" >&2; exit 1; }
[ -f "$template/vendor/mgba.js" ] && [ -f "$template/vendor/mgba.wasm" ] || {
  echo "error: vendored mGBA WASM missing under templates/itch-mgba/vendor/" >&2
  exit 1
}

# Resolve example dir or .gba → ROM path + name
rom=""
example_dir=""

if [ -d "$input" ]; then
  example_dir="$(cd "$input" && pwd)"
elif [ -d "$here/$input" ]; then
  example_dir="$(cd "$here/$input" && pwd)"
elif [ -d "$here/examples/$input" ]; then
  example_dir="$(cd "$here/examples/$input" && pwd)"
elif [ -f "$input" ] && [[ "$input" == *.gba ]]; then
  rom="$(cd "$(dirname "$input")" && pwd)/$(basename "$input")"
elif [ -f "$here/$input" ] && [[ "$input" == *.gba ]]; then
  rom="$(cd "$(dirname "$here/$input")" && pwd)/$(basename "$input")"
else
  echo "error: not an example directory or .gba file: $input" >&2
  exit 1
fi

if [ -n "$example_dir" ]; then
  [ -f "$example_dir/package.json" ] || {
    echo "error: no package.json in $example_dir" >&2
    exit 1
  }
  pkg_name="$(node -e "console.log(require(process.argv[1]).name)" "$example_dir/package.json")"
  if [ -z "$name" ]; then
    name="$pkg_name"
  fi
  echo "building $name in $example_dir …" >&2
  (cd "$example_dir" && npm run build)
  # Prefer CLI/dir name, then package.json name (they can differ, e.g. minimal).
  if [ -f "$example_dir/${name}.gba" ]; then
    rom="$example_dir/${name}.gba"
  elif [ -f "$example_dir/${pkg_name}.gba" ]; then
    rom="$example_dir/${pkg_name}.gba"
  else
    echo "error: ROM not found after build (tried ${name}.gba and ${pkg_name}.gba)" >&2
    exit 1
  fi
elif [ -z "$name" ]; then
  name="$(basename "$rom" .gba)"
fi

[ -f "$rom" ] || { echo "error: ROM not found: $rom" >&2; exit 1; }
[ -n "$name" ] || { echo "error: could not determine package name" >&2; exit 1; }

out="$here/dist/itch/$name"
html5="$out/html5"
rm -rf "$out"
mkdir -p "$html5"

echo "screenshot ($frames frames) …" >&2
"$here/scripts/screenshot.sh" "$rom" "$out/screenshot.png" "$frames"

# --- cover + embed-bg from screenshot (nearest-neighbor / pixelated) ---
make_media() {
  local shot="$out/screenshot.png"
  local cover="$out/cover.png"
  local embed="$out/embed-bg.png"

  if command -v magick >/dev/null 2>&1; then
    magick "$shot" -filter point -resize 200% "$embed"
    magick "$shot" -filter point -resize 200% -background black -gravity center -extent 630x500 "$cover"
  elif command -v convert >/dev/null 2>&1; then
    convert "$shot" -filter point -resize 200% "$embed"
    convert "$shot" -filter point -resize 200% -background black -gravity center -extent 630x500 "$cover"
  elif command -v sips >/dev/null 2>&1; then
    # 2× GBA → 480×320 embed; letterbox into 630×500 cover
    sips -z 320 480 "$shot" --out "$embed" >/dev/null
    local tmp
    tmp="$(mktemp -t itchcover).png"
    sips -z 320 480 "$shot" --out "$tmp" >/dev/null
    sips -p 500 630 --padColor 000000 "$tmp" --out "$cover" >/dev/null
    rm -f "$tmp"
  else
    echo "error: need sips or ImageMagick to build cover/embed images" >&2
    exit 1
  fi
  echo "wrote $embed" >&2
  echo "wrote $cover" >&2
}

make_media

echo "assembling html5 package …" >&2
cp "$template/index.html" "$template/player.js" "$html5/"
cp -R "$template/vendor" "$html5/vendor"
# Keep player self-contained; omit template README from the upload zip.
cp "$rom" "$html5/game.gba"

zip_path="$out/${name}-html5.zip"
rm -f "$zip_path"
(
  cd "$html5"
  zip -rq "$zip_path" . -x '*.DS_Store'
)
echo "wrote $zip_path" >&2
echo "wrote $html5/" >&2

if [ -n "${ITCH_TARGET:-}" ]; then
  if ! command -v butler >/dev/null 2>&1; then
    echo "error: ITCH_TARGET is set but butler is not on PATH" >&2
    exit 1
  fi
  echo "butler push → $ITCH_TARGET …" >&2
  butler push "$html5" "$ITCH_TARGET"
fi

echo "ok: dist/itch/$name/ (zip + cover.png + embed-bg.png)"
