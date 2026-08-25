#!/usr/bin/env bash
# Soak a GBA ROM and report every way it can be dead.
#
#   scripts/gba_soak.sh <rom.gba> [frames] [key-schedule]
#
# THREE independent checks, because no one of them subsumes the others:
#   1. crash strings in the log      — a classic panic that reached the log channel
#   2. SWI 02 (halt) in the log      — an abort. agb's allocation-failure handler HALTS and never
#                                      writes a panic line, so check 1 returns zero for it.
#   3. last painted frame ~= N       — a ROM that hangs while still calling frame() passes 1 and 2.
#
# ⚠️ When check 2 trips, this prints the final frame's colour count and tells you to LOOK at the PNG:
# agb renders its panic — with the exact file:line — to the framebuffer, not to the log. A 2-colour
# final frame is the crash screen and it usually names the bug outright.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
rom="${1:?usage: gba_soak.sh <rom.gba> [frames] [schedule]}"
frames="${2:-8000}"
keys="${3:-}"
png="$(mktemp -t soak).png"
log="$(mktemp -t soak).log"

GBA_SHOT_LOG=1 "$here/screenshot.sh" "$rom" "$png" "$frames" "$keys" > "$log" 2>&1

crash=$(grep -cE "Illegal opcode|Jumped to invalid address|panicked at" "$log" || true)
halt=$(grep -c "SWI: 02" "$log" || true)
last=$(grep -oE "rendered 240x160 @ frame [0-9]+" "$log" | tail -1 | grep -oE "[0-9]+$" || echo 0)
colours=$(python3 -c "
from PIL import Image; from collections import Counter
print(len(Counter(Image.open('$png').convert('RGB').getdata())))" 2>/dev/null || echo "?")

fail=0
[ "$crash" -gt 0 ] && { echo "  FAIL crash strings in log: $crash"; fail=1; }
[ "$halt"  -gt 0 ] && { echo "  FAIL aborted: $halt halt SWIs; final frame has $colours colours"; \
                        echo "       -> LOOK AT $png — agb renders the panic file:line to the screen"; fail=1; }
[ "$last" -lt $(( frames * 9 / 10 )) ] && { echo "  FAIL last painted frame $last of $frames (hung?)"; fail=1; }
[ $fail -eq 0 ] && echo "  ok   $(basename "$rom"): $frames frames, no crash, no halt, last paint $last, $colours colours"
[ $fail -ne 0 ] && echo "       log: $log   png: $png"
exit $fail
