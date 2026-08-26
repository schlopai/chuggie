#!/usr/bin/env bash
# Alignment/collision harness for the isometric player.
#
# Motion is quarter-tile, the world is whole-tile. Everything that has gone wrong here came from the
# two disagreeing: a character drawn off the front edge of a raised block (UNIT_LIFT), a body walked
# into an NPC before the step was refused, a height that popped a beat early or late (cellOf vs
# cellNear). None of it is visible on flat empty ground, which is why it kept coming back.
#
# Produces four frames per case at a fixed camera so they can be compared against each other and
# against a previous run. LOOK AT THEM — a build proves nothing here.
#
#   ./verify-align.sh [rom.gba]
#
# What to check in shots/align-*.png:
#   rest      feet sit on the MIDDLE of the tile diamond, not its front corner
#   block     walking onto a raised tile, the character rises when it is mostly on it
#   npc       walking into an NPC stops with a tile between the sprites, not overlapping them
#   talk      STILL UNVERIFIED — but no longer for the reason this note used to give.
#
#             It claimed the NPC cells "come from the BAKED BOARD, not from source". That is
#             wrong: they are in tiled/town.tmj's `units` object layer, and
#             crates/tish-gba-scenepack/src/tacticspack.rs converts them with
#             col = round(x / tilewidth), row = round(y / tilewidth)   (tilewidth = 32)
#
#             which gives, for this map:
#               player start   (8, 13)   cls 0
#               Elder          (4, 4)    cls 1
#               Healer/clinic  (11, 10)  cls 2
#               Guard          (8, 3)    cls 3
#               Merchant       (12, 11)  cls 4
#
#             What is actually missing is a WALKABLE path. Motion is quarter-tile (Q = 4), one
#             sub-step per held frame, so a move of N cells is 4N frames; but the plaza has a pond
#             and raised props between the spawn and every NPC, and a blocked step silently stops
#             the walk short — which looks identical to a failed interaction. Three scripted
#             approaches were tried and each ended one or two cells short.
#
#             To finish it: read the walkable mask out of tiled/terrain.tsj (a terrain tile with
#             `walkable = false` is impassable), path-find a clear route from (8,13) to a cell
#             orthogonally adjacent to one NPC, and make the LAST held direction face that NPC —
#             facing is set from the input delta even when the step itself is refused. Then this
#             case becomes the regression test for "can only talk from a diagonal".
#
#             Until then, check it by hand in mGBA.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rom="${1:-$here/iso-town.gba}"
out="$here/shots"; mkdir -p "$out"
[ -f "$rom" ] || { echo "no rom at $rom — run: npm run build" >&2; exit 1; }

shot() { "$here/../../scripts/screenshot.sh" "$rom" "$out/align-$1.png" "$2" "${3:-}" >/dev/null 2>&1; }

shot rest   200
shot block  330 "200:up,320:"
shot npc    360 "200:right,350:"
shot walk   300 "200:down,290:"
# Head-on interaction: face the NPC square and press A. If the dialog only opens from a diagonal,
# something is asking a different question about "which cell am I on" than movement does.
shot talk   520 "200:right,330:,360:up,470:,500:a,512:"
echo "wrote $out/align-{rest,block,npc,walk,talk}.png"
