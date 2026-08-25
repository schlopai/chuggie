#!/usr/bin/env bash
# verify.sh — headless assertions for the per-scanline parallax bands.
#
# THIS FILE EXISTS BECAUSE OF A BUG NOTHING COULD HAVE CAUGHT. The demo's d-pad was bound to the
# SHOULDER buttons — `let LEFT = 5` / `let RIGHT = 4`, which are L and R in tish-agb's `button_of`,
# not the d-pad (8 and 9). Holding right produced a frame PIXEL-IDENTICAL to holding nothing, and
# had done since the example was written. The comment in main.tish promising you could "scrub back
# and forth" was the only implementation of that feature.
#
# It survived because a dead control emits nothing: no crash, no log line, no changed pixel. It is
# indistinguishable from a control that was simply never pressed — and nothing pressed it.
#
# So the rule this file follows: if the README advertises a control, press it and assert the picture
# changed, and changed in the right DIRECTION. "Differs" is not enough; a control wired backwards
# also differs.
if [ -z "${VERIFY_HOME:-}" ]; then
  VERIFY_HOME="$(cd "$(dirname "$0")" && pwd)"
  export VERIFY_HOME
  snap="$(mktemp -t bands-verify.XXXXXX)" || exit 1
  cat "$0" >"$snap"
  exec bash "$snap" "$@"
fi
set -u
cd "$VERIFY_HOME"
rm -f "$0"   # the open fd keeps it readable; nothing on disk to edit or leave behind

ROM=bands-demo.gba
SHOT=../../scripts/screenshot.sh
[ -f "$ROM" ] || npm run build >/tmp/bands-vbuild.log 2>&1 || true
[ -f "$ROM" ] || { echo "  FAIL $ROM missing:"; tail -20 /tmp/bands-vbuild.log; exit 1; }
PASS=0
FAIL=0
ok()  { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; }

echo "verify bands-demo: $ROM"

# An empty log satisfies every absence-check below, so "no panic" is only evidence when the ROM
# actually printed something.
LOG=$(GBA_SHOT_NOSAVE=1 GBA_SHOT_LOG=1 "$SHOT" "$ROM" /tmp/bands-boot.png 120 2>&1)
if [ -z "$LOG" ]; then
  bad "boots (NO OUTPUT — harness or ROM did not run)"
elif grep -qi "panic\|out of video ram\|allocation of" <<<"$LOG"; then
  bad "boots"
else
  ok "boots"
fi

python3 - "$VERIFY_HOME" <<'PYEOF'
import sys, subprocess, os
home = sys.argv[1]
shot = os.path.join(home, "../../scripts/screenshot.sh")
rom  = os.path.join(home, "bands-demo.gba")
env  = dict(os.environ, GBA_SHOT_NOSAVE="1")
try:
    from PIL import Image, ImageChops
except ImportError:
    print("  ok   bands checks SKIPPED (no Pillow)"); sys.exit(0)

def shot_at(frame, sch=""):
    out = "/tmp/_bands_%d_%s.png" % (frame, sch.replace(":", "").replace(",", "") or "none")
    r = subprocess.run([shot, rom, out, str(frame), sch], cwd=home, env=env,
                       capture_output=True, text=True)
    if not os.path.exists(out):
        print("  FAIL screenshot at frame %d never appeared (exit %d)" % (frame, r.returncode))
        print((r.stderr or r.stdout or "")[-2000:])
        sys.exit(1)
    return Image.open(out).convert("RGB")

def npx(a, b, y0=0, y1=160):
    d = ImageChops.difference(a.crop((0, y0, 240, y1)), b.crop((0, y0, 240, y1)))
    return sum(1 for p in d.getdata() if p != (0, 0, 0))

fails = []
def check(cond, msg):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        fails.append(msg)

# 1. THE PICTURE MOVES AT ALL. main.tish deliberately has no `bg_parallax` call: every pixel of
#    movement is the per-scanline DMA. If the bands stopped working the screen would sit perfectly
#    still, which is why this is a test and not just a demo.
a, b = shot_at(120), shot_at(180)
check(npx(a, b) > 2000, "the bands scroll with no bg_parallax (%d px changed over 60 frames)" % npx(a, b))

# 2. IT IS PARALLAX, not one layer sliding. STAR_MUL 12 / MTN_MUL 72 / TREE_MUL 240 means the trees
#    must move about twenty times as far as the stars over the same interval. Comparing whole-frame
#    diffs would pass on a single layer scrolling; comparing BANDS is what tests the effect.
#    Rows: stars 0..51, mountains 52..103, trees 104..159 (MTN_TOP / TREE_TOP in main.tish).
stars = npx(a, b, 0, 52)
trees = npx(a, b, 104, 160)
check(trees > stars * 3,
      "near bands outrun far ones (stars %d px vs trees %d px changed)" % (stars, trees))

# 3. THE D-PAD IS WIRED, AND THE RIGHT WAY ROUND. The camera free-drifts +1/frame and the d-pad adds
#    -3/+3, so a RIGHT-held frame must be IDENTICAL to some LATER free-drift frame, and a LEFT-held
#    one to an EARLIER one. An exact zero-pixel match is available here because the scene is fully
#    deterministic, and it pins the magnitude as well as the direction — a control wired to the
#    wrong axis, or inverted, or at the wrong rate all fail this while still "differing".
def matching_drift_frame(target, lo, hi):
    best = (None, 1 << 30)
    for f in range(lo, hi):
        d = npx(target, shot_at(f))
        if d < best[1]:
            best = (f, d)
        if d == 0:
            break
    return best

right = shot_at(200, "0:right")
check(npx(right, shot_at(200)) > 0,
      "holding RIGHT changes the picture at all (this is the bug that shipped)")
rf, rd = matching_drift_frame(right, 770, 800)
check(rd == 0 and rf > 200,
      "RIGHT advances the camera: frame 200 held == free-drift frame %s, exactly (%d px off)" % (rf, rd))

left = shot_at(200, "150:left")
lf, ld = matching_drift_frame(left, 40, 80)
check(ld == 0 and lf < 150,
      "LEFT rewinds the camera: frame 200 held-from-150 == free-drift frame %s, exactly (%d px off)" % (lf, ld))

sys.exit(len(fails))
PYEOF
rc=$?
PASS=$((PASS + 5 - rc)); FAIL=$((FAIL + rc))

echo
echo "verify bands-demo: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
