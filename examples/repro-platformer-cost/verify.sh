#!/usr/bin/env bash
# Perf tripwire for packages/platformer.tish. The 2026-08 de-boxing pass took driveRaw from
# 1,534 to ~1,295 ticks/frame and animate from 735 to ~400 (fast path); these ceilings hold the
# line — a change that regresses past them costs a platformer game its 60fps. Ceilings carry
# ~15% headroom over the measured means so emulator jitter doesn't flake the check.
set -u
cd "$(dirname "$0")"

fails=0
say() { printf '  %-4s %s\n' "$1" "$2"; }

echo "repro-platformer-cost:"

if npm run build >/dev/null 2>&1; then
  say ok "builds"
else
  say FAIL "builds"; echo "repro-platformer-cost: 1 failure(s)"; exit 1
fi

out=$(GBA_SHOT_LOG=1 ../../scripts/screenshot.sh repro-platformer-cost.gba /tmp/rpc.png 390 "" 2>&1)

get() { printf '%s\n' "$out" | grep "cost $1" | sed 's/.*mean=//' | tr -d '\r'; }

check_under() { # name, value, ceiling
  if [ -n "$2" ] && [ "$2" -le "$3" ]; then
    say ok "$1 = $2 (ceiling $3)"
  else
    say FAIL "$1 = ${2:-missing} (ceiling $3)"
    fails=$((fails + 1))
  fi
}

# ⭐ the number that matters: what a GAME's update pays each frame (platformerDrive).
# Ceilings sit ~10% over the measured means (emulator jitter). The 2026-08-20 pass took the
# game path 1,534 -> 904 by READING THE GENERATED RUST instead of guessing: `this_.id` is
# itself a string-keyed get_prop, and the compiled function did FIFTY-TWO of them.
check_under "drive idle (the game path)" "$(get 'E drive idle')" 1000
check_under "driveRaw idle"    "$(get 'A driveRaw idle')"    1000
check_under "driveRaw walking" "$(get 'B driveRaw walking')" 1000
check_under "animate"          "$(get 'C animate')"          450
check_under "wrapper call"     "$(get 'D one wrapper call')" 150

printf '%s\n' "$out" | grep -q "repro-platformer-cost done"
if [ $? -eq 0 ]; then say ok "ran to completion"; else say FAIL "did not complete"; fails=$((fails+1)); fi

echo "repro-platformer-cost: $fails failure(s)"
[ "$fails" -eq 0 ]
