#!/usr/bin/env bash
# cutscene-raw verify — schlopai/chuggie-engine#63 and #66.
#
# The claim: a game that CANNOT link the game-engine crate can stage a scene. Not "can show a
# dialogue box" — can move an actor on its own, pan a camera, and branch. #66 is explicit that a core
# with `cutSay` and `cutChoose` but no movement "gets a game talking heads with a working prompt,
# which is what card-gba already hand-rolled, and is not a cutscene".
#
# So the structural assertions (no crate) and the behavioural ones (things actually moved) are both
# required, and neither is sufficient. A ROM that links nothing and does nothing would pass half of
# this file.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== the crate-free contract =="
unset CARGO_TARGET_DIR

# ⚠️ THE ROOT FIX. If this import comes back, every game that cannot link the engine loses the
# sequencer again — which is the state #63 was filed about.
# ⚠️ Match an IMPORT LINE, not the bare crate name — this file's header explains at length
# why it does not import the crate, and a bare grep matches that prose and fails on correct code.
grep -qE "^import .*cargo:tish_gba_game_engine" ../../packages/cutscene-core.tish
if [ $? = 0 ]; then echo "FAIL cutscene-core imports the game-engine crate"; fail=1
else echo "ok   packages/cutscene-core.tish imports no game-engine crate"; fi

grep -qE "^import .*from './dialog'" ../../packages/cutscene-core.tish
if [ $? = 0 ]; then echo "FAIL cutscene-core imports packages/dialog (links chipsfx at boot, #64)"; fail=1
else echo "ok   packages/cutscene-core.tish imports no dialogue package (it is injected)"; fi

# ...and #66's specific ask: the MOVEMENT verbs must be in the core, not only the entity-free ones.
for v in cutWalkFrom cutFace cutPan cutSay cutChoose cutWait cutFadeOut cutFadeIn; do
  grep -q "export function $v" ../../packages/cutscene-core.tish || { echo "FAIL core is missing $v"; fail=1; }
done
echo "ok   core carries every hook-driven verb (movement, facing, camera), not just the free ones"

python3 -c "
import json,sys
d=json.load(open('package.json'))['tish']['rustDependencies']
assert 'tish_gba_game_engine' not in d, 'this example must not depend on the game engine'
" 2>&1
check $? "this ROM declares no game-engine crate dependency"

npm run build >/tmp/cr-build.log 2>&1
check $? "builds with tish_agb alone"
if [ $fail = 1 ]; then tail -20 /tmp/cr-build.log; exit 1; fi

# The strongest structural check: the crate is not in the generated ROM crate's manifest at all.
grep -q "tish_gba_game_engine" .tish/gba/cutscene-raw/Cargo.toml
if [ $? = 0 ]; then echo "FAIL the game engine reached the generated crate anyway"; fail=1
else echo "ok   the game-engine crate is absent from the generated Cargo.toml"; fi

echo "== the scene actually happens =="
log=$(mktemp)
SCHED=$(python3 -c "
parts=[]
for f in range(120, 1400, 30):
    parts.append(f'{f}:a'); parts.append(f'{f+4}:')
print(','.join(parts))")
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh cutscene-raw.gba /tmp/cr-verify.png 1500 "$SCHED" >"$log" 2>&1
check $? "runs the scene headless"
crash_grep "$log"
check $? "no panic, no allocation failure"

f() { grep -o "CUT $1.*" "$log" | head -1; }

# ⚠️ BEAT 1 — an actor moved ON ITS OWN. #66: "Nene sits down opposite you before the first match —
# you walk into a stationary sprite instead." This is the assertion that makes the difference between
# a sequencer and a prompt.
S=$(grep -o 'CUT BEAT1 start nene=[0-9-]*' "$log" | cut -d= -f2)
E=$(grep -o 'CUT BEAT1 end nene=[0-9-]*' "$log" | cut -d= -f2)
note "beat 1: nene $S -> $E"
[ -n "$S" ] && [ -n "$E" ] && [ "$S" != "$E" ]
check $? "an actor walked into frame under its own power ($S -> $E)"

# ⚠️ BEAT 2 — the camera moved, and CAME BACK. Asserting only that it moved would pass on a camera
# that drifted off and never returned, which is a bug that looks like a pan for the first second.
C0=$(grep -o 'CUT BEAT2 start cam=[0-9-]*' "$log" | cut -d= -f2)
C1=$(grep -o 'CUT BEAT2 mid cam=[0-9-]*' "$log" | cut -d= -f2)
C2=$(grep -o 'CUT BEAT2 end cam=[0-9-]*' "$log" | cut -d= -f2)
note "beat 2: camera $C0 -> $C1 -> $C2"
[ -n "$C1" ] && [ "$C1" != "$C0" ]
check $? "the camera panned off the player ($C0 -> $C1)"
[ "$C2" = "$C0" ]
check $? "...and came back ($C1 -> $C2)"

# The branch reads an index and sets a flag — `cutChoose` returning the index is what makes a
# branch read normally at the call site.
grep -q 'CUT CHOICE 0' "$log"
check $? "cutChoose returned an index"
grep -q 'CUT FLAG accepted=1' "$log"
check $? "the branch was taken and the story flag was stored"

# BEAT 4 — the player character was moved BY the scene rather than by input.
Y=$(grep -o 'CUT BEAT4 end you=[0-9]*,[0-9]*' "$log" | cut -d= -f2)
[ "$Y" = "96,60" ]
check $? "the scene moved the player itself (ended at $Y)"

grep -q 'CUT DONE' "$log"
check $? "the scene ran to the end"

lo=$(grep -o 'heap=[0-9]*' "$log" | cut -d= -f2 | sort -n | head -1)
hi=$(grep -o 'heap=[0-9]*' "$log" | cut -d= -f2 | sort -n | tail -1)
note "heap across the scene: $hi -> $lo"
[ $(( hi - lo )) -le 8192 ]
check $? "heap bounded across the scene (span $(( hi - lo )) B)"

n=$(python3 - <<'EOF'
from PIL import Image
im = Image.open('/tmp/cr-verify.png').convert('RGB')
print(len(set(im.getdata())))
EOF
) || n=0
[ "${n:-0}" -ge 5 ]
check $? "frame paints ($n colours)"

echo
[ "$fail" = 0 ] && echo "cutscene-raw: PASS" || echo "cutscene-raw: FAIL"
exit $fail
