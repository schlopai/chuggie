#!/usr/bin/env bash
# Headless proof of the ecosystem/platformer engine natives. Every check in the ROM prints "ok ..." or
# "FAIL ...": this script counts both, requires the full set, and fails on any FAIL — a
# check that crashed before printing counts as missing, not as passed.
set -u
cd "$(dirname "$0")"

fails=0
say() { printf '  %-4s %s\n' "$1" "$2"; }

echo "repro-eco-natives:"

if npm run build >/dev/null 2>&1; then
  say ok "builds"
else
  say FAIL "builds"
  echo "repro-eco-natives: 1 failure(s)"
  exit 1
fi

out=$(GBA_SHOT_LOG=1 ../../scripts/screenshot.sh repro-eco-natives.gba /tmp/repro-eco-natives.png 700 "" 2>&1)

want=13
# mGBA prefixes every log line with "[frame N] ", so match the marker mid-line.
oks=$(printf '%s\n' "$out" | grep -c '] ok ' || true)
bads=$(printf '%s\n' "$out" | grep -c '] FAIL ' || true)
done_line=$(printf '%s\n' "$out" | grep -c 'repro-eco-natives done' || true)

if [ "$bads" -gt 0 ]; then
  say FAIL "ROM checks ($bads FAIL lines)"
  printf '%s\n' "$out" | grep '] FAIL '
  fails=$((fails + 1))
else
  say ok "no FAIL lines"
fi
if [ "$oks" -eq "$want" ]; then
  say ok "all $want checks printed"
else
  say FAIL "expected $want ok lines, got $oks (a crashed check prints nothing)"
  printf '%s\n' "$out" | grep '] ok ' || true
  fails=$((fails + 1))
fi
if [ "$done_line" -ge 1 ]; then
  say ok "ran to completion"
else
  say FAIL "did not reach the done line"
  fails=$((fails + 1))
fi

echo "repro-eco-natives: $fails failure(s)"
[ "$fails" -eq 0 ]
