#!/usr/bin/env bash
# probe-arrayarg — passing a typed module array to a native de-optimises EVERY read of it.
#
# Two arrays in one module, declared identically, filled identically, read by identical loops. The
# only difference in the whole program is that one of them is handed to a native function once.
#
# ⚠️ THE FIRST VERSION OF THIS PROBE GOT A FALSE NEGATIVE. It called `grid_from_gids(32, 32, B, A)`
# — which passes BOTH arrays — saw that A was boxed too, and concluded that passing to a native was
# not the trigger. Four other hypotheses were then chased for nothing. Check the arguments before
# believing a negative result.
set -uo pipefail
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== rom =="
unset CARGO_TARGET_DIR
rm -rf .tish   # tish build caches packages; without this the codegen assertions read a stale answer
npm run build >/tmp/pa-build.log 2>&1
check $? "builds from a clean .tish"
if [ $fail = 1 ]; then tail -20 /tmp/pa-build.log; exit 1; fi

G=.tish/gba/probe-arrayarg/src/main.rs

echo "== the codegen fact =="
# Asserted against the GENERATED RUST, so this is a statement about the compiler and not a timing
# that could drift with a toolchain bump.
awk '/let sumA = \{/,/^    \};/' "$G" | grep -q 'A.borrow()'
check $? "A — never passed to a native — reads through the typed Vec path"
awk '/let sumB = \{/,/^    \};/' "$G" | grep -q 'get_index(&tishlang_runtime::vm_read(&B)'
check $? "B — passed to a native ONCE — reads through the boxed Value path"
# ...and the boxing is not merely present on B, it is ABSENT on A. Both halves matter: a compiler
# that boxed everything would satisfy the second assertion alone.
awk '/let sumA = \{/,/^    \};/' "$G" | grep -q 'get_index' && { echo "FAIL A is boxed too"; fail=1; } || echo "ok   A is NOT boxed (the difference is real, not universal)"

echo "== the cost =="
log=$(mktemp)
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh probe-arrayarg.gba /tmp/pa.png 60 >"$log" 2>&1
check $? "runs headless"
crash_grep "$log"
check $? "no panic, no allocation failure"

raw() { grep -o "ARRAYARG.*" "$log" | head -1 | grep -o "$1=[0-9-]*" | cut -d= -f2; }
BARE=$(raw bare); NAT=$(raw nativeArg); MASK=$(raw masked); HERE=$(raw here)
note "1024 reads: A=$BARE  B(passed to native)=$NAT  A[i&1023]=$MASK  A from main.tish=$HERE"

[ -n "$BARE" ] && [ "$BARE" -gt 0 ] && [ "$NAT" -gt 0 ]
check $? "both spans are positive (the 16-bit timer did not wrap)"

[ $(( NAT * 100 / BARE )) -gt 250 ]
check $? "the passed array costs >2.5x per read ($(( NAT * 100 / BARE ))% of the untouched one)"

# Two controls, so the difference cannot be blamed on the things it is easy to blame it on.
D=$(( MASK - BARE )); [ ${D#-} -lt $(( BARE / 4 )) ]
check $? "a MASKED index is not the difference (A[i]=$BARE vs A[i&1023]=$MASK)"
D=$(( HERE - BARE )); [ ${D#-} -lt $(( BARE / 4 )) ]
check $? "the reader's MODULE is not the difference (in-module=$BARE vs entry-module=$HERE)"

echo
[ "$fail" = 0 ] && echo "probe-arrayarg: PASS" || echo "probe-arrayarg: FAIL"
exit $fail
