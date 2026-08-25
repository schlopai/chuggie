#!/usr/bin/env bash
# Verify feel-demo — and, through it, packages/feel.tish.
#
# THE POINT OF THIS FILE. Until feel-demo existed, nothing in chuggie-engine compiled `packages/feel.tish`
# at all: its only consumers were in the sibling card-gba tree, reaching across the repo boundary.
# So this repo shipped 670 lines it could not build, let alone test, and the original showcase
# (card-gba/examples/fx-demo, now removed) had no verify.sh either. Every assertion below was
# unasserted anywhere before this.
#
# Each check is a CLAIM feel.tish's own header makes, not "the ROM booted". The two that matter
# most are the cooldown and the channel, because both were wrong when first measured:
#
#   * `feelPlay` refused an ENTIRE preset while on cooldown, so a cascade that destroyed four things
#     shook the screen once. Its table says only the DRAWING is rate-limited. Fixed in feel.tish;
#     asserted here as "four plays, one banner".
#   * the demo drained its own event queue in the same frame it filled it, which is a function call
#     with extra steps rather than a channel. Asserted here as "EMIT and DRAIN on different frames".
set -uo pipefail
cd "$(dirname "$0")"
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$1" = 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fail=1; fi; }

echo "== static =="
# EVERY EXPORT IS EXERCISED. A demo for a package is only evidence for the parts it calls, and the
# gap is invisible by eye — the first version of this ROM missed five exports (feelStop, feelPalette,
# feelBump, feelFreeze, feelChain) and looked complete. Derived from the package, not a hand list, so
# adding an export makes this fail until the demo covers it.
python3 - <<'PY'
import re, sys, pathlib
pkg = pathlib.Path("../../packages/feel.tish").read_text(encoding="utf-8")
src = pathlib.Path("src/main.tish").read_text(encoding="utf-8")
fns = set(re.findall(r"^export function (\w+)", pkg, re.M))
missing = sorted(f for f in fns if not re.search(r"\b%s\s*\(" % f, src))
print(f"  {len(fns)} exported functions, {len(fns) - len(missing)} called by the demo")
if missing:
    print("  NOT EXERCISED: " + ", ".join(missing))
sys.exit(1 if missing else 0)
PY
check $? "the demo calls every function packages/feel.tish exports"

echo "== rom =="
unset CARGO_TARGET_DIR
npm run build >/tmp/feel-demo-build.log 2>&1
built=$?
check $built "builds (this repo compiles feel.tish at all)"
# Bail on THIS check only. Guarding on the cumulative `fail` flag meant any earlier failure — the
# static export-coverage one, say — skipped every runtime assertion below it, and the run still
# printed "FAIL" at the end, so nothing looked wrong. A suite that stops at the first failure hides
# the other twenty-five; the whole point of the list is to see them all at once.
if [ $built != 0 ]; then tail -30 /tmp/feel-demo-build.log; exit 1; fi

# The capture filter must keep every line CRASH_RE can match — a filter that drops
# `Illegal opcode` lines makes the crash check below pass vacuously.
. ../../scripts/verify_common.sh
GBA_SHOT_LOG=1 ../../scripts/screenshot.sh feel-demo.gba /tmp/feel-demo.png 1400 2>&1 \
  | grep -E "FEEL DEMO|PLAY |REFUSED|SPAM|CALLOUT|CASCADE|CHAIN|EMIT|DRAIN|INTENSITY|TICK|$CRASH_RE" \
  > /tmp/feel-demo.log
check $? "runs headless"

crash_grep /tmp/feel-demo.log
check $? "no crash marker in the log (full regex, incl. Illegal opcode / invalid jump)"

grep -q 'FEEL DEMO START PRESETS 7 SLOTS 6' /tmp/feel-demo.log
check $? "boots, and the preset and slot counts are the package's own"
note "$(grep -o 'FEEL DEMO START.*' /tmp/feel-demo.log | head -1)"

echo "== the presets =="
# All seven, by name. A preset that silently stopped firing would otherwise leave every other check
# here passing.
missing=""
for p in PLACE DESTROY BUFF LANE ILLEGAL VICTORY DEFEAT; do
  grep -q "PLAY $p " /tmp/feel-demo.log || missing="$missing $p"
done
[ -z "$missing" ]
check $? "all seven presets played${missing:+ (missing:$missing)}"

# A call-out is the composed part — a delayed row that spawns a slot, rises and ramps colour. It is
# drawn by feel itself now (`feelCalloutDraw`, sprite text) rather than by the demo painting into the
# canvas, so it no longer erases a band and no longer constrains what may sit under it. VICTORY is
# the deepest preset in the table (flash + bump + freeze + sfx + callout + burst from one call), so
# it is named specifically.
grep -q 'CALLOUT VICTORY' /tmp/feel-demo.log
check $? "the deepest preset composed its call-out"

echo "== springs and cooldowns =="
# THE ASSERTION THIS ROM EXISTS FOR. Four plays inside ONE frame must produce FOUR plays — every
# bump lands on the spring and compounds — and exactly ONE banner, because the cooldown rate-limits
# the drawing and nothing else. Before the fix this window read one PLAY and three REFUSED.
# The window is the RUN of play lines immediately after the SPAM marker, not a fixed line count. A
# fixed 40 lines reached into the next attract cycle and counted a fifth, unrelated play.
spam_line=$(grep -n 'SPAM 4x' /tmp/feel-demo.log | head -1 | cut -d: -f1)
if [ -n "${spam_line:-}" ]; then
  burst=$(tail -n +$(( spam_line + 1 )) /tmp/feel-demo.log \
    | awk '/(PLAY|REFUSED) DESTROY/ { print; next } { exit }')
  plays=$(printf '%s\n' "$burst" | grep -c 'PLAY DESTROY' || true)
  refused=$(printf '%s\n' "$burst" | grep -c 'REFUSED DESTROY' || true)
  banners=$(tail -n +"$spam_line" /tmp/feel-demo.log | head -30 | grep -c 'CALLOUT DESTROY' || true)
  [ "$plays" = 4 ] && [ "$refused" = 0 ]
  check $? "four plays in one frame all landed on the spring (plays=$plays refused=$refused)"
  [ "$banners" = 1 ]
  check $? "and drew exactly one banner, not four (banners=$banners)"
else
  check 1 "the spam window was not reached"
fi

echo "== hit-stop =="
# The GAME counter must fall behind the demo's own frame counter, and the gap must be the frames the
# feel layer reported frozen. A freeze that did nothing would leave them equal.
# The HIGHEST tick, not the last one. Attract loops by resetting the demo's counter, so `tail -1`
# picks up a line from after the wrap where the tick has restarted at 60 and the game count has not.
last=$(grep -oE 'TICK [0-9]+ GAME [0-9]+ FROZEN [0-9]+' /tmp/feel-demo.log | sort -k2 -n | tail -1)
t=$(echo "$last" | awk '{print $2}'); g=$(echo "$last" | awk '{print $4}'); f=$(echo "$last" | awk '{print $6}')
[ "${f:-0}" -gt 0 ] && [ "${g:-0}" -lt "${t:-0}" ]
check $? "hit-stop stalled the game while the feel layer kept ticking ($last)"
# The two must agree: every frame the game skipped is a frame feel reported frozen.
[ $(( t - g )) = "$f" ]
check $? "the stall is exactly the frozen frame count ($((t - g)) skipped vs $f frozen)"

echo "== the raw primitives =="
# CASCADE is the eighth list entry and is not a preset: feelChain / feelBump / feelFreeze called
# directly, one link per frame so the rising notes are actually audible as a chain.
grep -q 'CASCADE START' /tmp/feel-demo.log
check $? "the cascade ran without going through a preset"
links=$(grep -c '^\[frame [0-9]*\] CHAIN ' /tmp/feel-demo.log || true)
[ "${links:-0}" -ge 5 ]
check $? "it stepped all five links (got ${links:-0})"
# Spread over frames, not fired in one — five notes inside a single frame are one note on a PSG
# channel, so a cascade that logged all five at the same frame number would be silent in practice.
spread=$(grep -oE '^\[frame [0-9]+\] CHAIN ' /tmp/feel-demo.log | grep -oE '[0-9]+' | sort -u | wc -l | tr -d ' ')
[ "${spread:-0}" -ge 4 ]
check $? "the links landed on different frames (${spread:-0} distinct)"

echo "== the channel =="
grep -q 'EMIT 3 PENDING 3' /tmp/feel-demo.log
check $? "three events were broadcast and queued"
drains=$(grep -c 'DRAIN KIND ' /tmp/feel-demo.log || true)
[ "${drains:-0}" = 3 ]
check $? "all three were drained (got ${drains:-0})"
# THE DEFERRAL IS THE POINT. An emit read back inside the same frame is not a channel.
ef=$(grep -oE '^\[frame [0-9]+\] EMIT ' /tmp/feel-demo.log | head -1 | grep -oE '[0-9]+')
df=$(grep -oE '^\[frame [0-9]+\] DRAIN ' /tmp/feel-demo.log | head -1 | grep -oE '[0-9]+')
[ -n "${ef:-}" ] && [ -n "${df:-}" ] && [ "$df" -gt "$ef" ]
check $? "the drain happened on a LATER frame than the emit (emit=$ef drain=$df)"

echo "== the intensity dial =="
grep -q 'INTENSITY 512' /tmp/feel-demo.log
check $? "the dial reached 2x (feelScale amplifies, it does not merely clamp)"
# The hard-off. A play attempted at 0 must be refused outright — this is the accessibility switch,
# and "it looked calmer" is not evidence.
off_line=$(grep -n 'INTENSITY 0' /tmp/feel-demo.log | head -1 | cut -d: -f1)
[ -n "${off_line:-}" ] && tail -n +"$off_line" /tmp/feel-demo.log | head -20 | grep -q 'REFUSED'
check $? "at intensity 0 a play is refused outright, not merely quieter"

echo "== cost =="
# KEYFRAMES, NOT FRAMES is the rule this package is built around, so the demo has to keep it. The
# ROM logs once per 60 iterations and the emulator stamps its own frame number, so the drift between
# them IS the dropped-frame count — no profiler needed.
#
# This caught a real regression in the demo itself: repainting the `f` readout on every game frame
# cost 20% of the frame budget (TICK 1020 arriving at frame 1270). Throttled to every fourth frame
# it is ~3%. The bar is 10%, comfortably under the failure and comfortably over the noise.
boot=$(grep -oE '^\[frame [0-9]+\] FEEL DEMO START' /tmp/feel-demo.log | grep -oE '[0-9]+')
lastt=$(grep -oE '^\[frame [0-9]+\] TICK [0-9]+' /tmp/feel-demo.log | sort -t' ' -k4 -n | tail -1)
ef2=$(echo "$lastt" | grep -oE '^\[frame [0-9]+' | grep -oE '[0-9]+')
it=$(echo "$lastt" | awk '{print $4}')
drift=$(( ef2 - boot - it ))
pct=$(( drift * 100 / it ))
[ "$pct" -lt 10 ]
check $? "runs at ~60fps (${pct}% of frames dropped over ${it} iterations)"

echo "== screen =="
python3 - <<'PY'
import struct, zlib, sys
d = open('/tmp/feel-demo.png','rb').read()
pos, idat, w, h = 8, b'', 0, 0
while pos < len(d):
    ln = struct.unpack('>I', d[pos:pos+4])[0]; typ = d[pos+4:pos+8]
    if typ == b'IHDR': w, h = struct.unpack('>II', d[pos+8:pos+16])
    if typ == b'IDAT': idat += d[pos+8:pos+8+ln]
    pos += 12 + ln
raw = zlib.decompress(idat); stride = w*3+1
cols = set()
for y in range(0, h, 2):
    row = raw[y*stride+1:(y+1)*stride]
    for x in range(0, w*3-3, 3):
        cols.add(row[x:x+3])
print(f"  {len(cols)} distinct colours in {w}x{h}")
sys.exit(0 if len(cols) > 4 else 1)
PY
check $? "the ROM paints a live frame (not a blank screen)"

echo "== the call-out comes down =="
# THE FAILURE MODE SPRITE TEXT INTRODUCES. A canvas call-out could not outlive its effect, because
# erasing the band was part of drawing it. A sprite one can: `text_visible` is a sticky per-slot
# flag, so a banner interrupted by a scene or selection change simply stays on screen until someone
# takes it down. `feelStop` retires feel's slots but the text_draw slot belongs to the CALLER, which
# is why the demo pairs it with `feelCalloutHide` — and why that pairing needs an assertion rather
# than a comment. Fire DESTROY, confirm the word is up, change row mid-banner, confirm it is gone.
python3 - <<'PY'
import subprocess, sys, os
env = dict(os.environ, GBA_SHOT_NOSAVE="1")
S = "100:down,106:,140:a,146:,190:up,196:"
try:
    from PIL import Image
except ImportError:
    print("  ok   call-out teardown SKIPPED (no Pillow)"); sys.exit(0)
BG = (16, 16, 24)
def band_px(f):
    out = "/tmp/_fdhide_%d.png" % f
    subprocess.run(["../../scripts/screenshot.sh", "feel-demo.gba", out, str(f), S],
                   env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    px = Image.open(out).convert("RGB").load()
    # The band the word travels through, full width. Backdrop-only means no banner.
    return sum(1 for y in range(98, 122) for x in range(240) if px[x, y] != BG)
up   = max(band_px(f) for f in (165, 175, 188))
down = max(band_px(f) for f in (200, 210))
ok = up > 40 and down == 0
print("  %s feelCalloutHide takes the banner down on a row change (up %dpx -> after %dpx)"
      % ("ok  " if ok else "FAIL", up, down))
sys.exit(0 if ok else 1)
PY
check $? "the call-out is hidden when feelStop cancels it"

# A whole-frame reference, on an animation PLATEAU found with scripts/frame_stability.py --find.
# A frame picked at a round number lands on a transition about as often as not, and a golden frame
# one step from a change flips on any timing shift and reads as a regression.
#
# It was 1094, which had DECAYED into a one-frame island — 1093 and 1095 both differed from it — and
# the reference had been failing since packages/feel.tish changed on 2026-08-07, two days after the
# golden was taken. Nobody saw it because the build guard above bailed out of the whole runtime
# suite on the first failure. Re-found at 1098 (a plateau of 4) and regenerated, after the call-out
# moved to sprite text, which changes this frame by construction.
../../scripts/screenshot.sh feel-demo.gba /tmp/feel-demo-screen.png 1098 >/dev/null 2>&1
python3 ../../scripts/screen_check.py /tmp/feel-demo-screen.png reference.png
check $? "the screen matches its reference frame"

if [ $fail = 0 ]; then echo "feel-demo: PASS"; else echo "feel-demo: FAIL"; fi
exit $fail
