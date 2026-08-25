#!/usr/bin/env bash
# illusions — the checks that actually catch this example breaking.
#
# An illusion demo has an unusual failure mode: nearly every way it can break still produces a
# perfectly reasonable-looking picture. A Kanizsa wedge pointing the wrong way is three tidy
# pac-men. A cafe wall with black mortar is a neat checkerboard. An Ebbinghaus whose two centre
# discs are genuinely different sizes looks exactly like a working Ebbinghaus — that one is not
# merely undetectable by eye, it is undetectable by eye BY CONSTRUCTION, since the whole page is
# about the eye getting that comparison wrong.
#
# So the screenshot checks below do not ask "is something on screen". They assert the specific
# geometric CLAIM each page makes, by counting pixels of a known colour. That is the only kind of
# check with any power here.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
# shellcheck source=../../scripts/verify_common.sh
source $ROOT/scripts/verify_common.sh
fail=0
src=src/main.tish
rom=illusions.gba

assert_agb_fork || fail=1
assert_typed_scalars $src || fail=1

[ -f $rom ] || npm run build >/tmp/illusions-vbuild.log 2>&1 || true
[ -f $rom ] || { echo "  FAIL $rom not built:"; tail -20 /tmp/illusions-vbuild.log; exit 1; }

# ── Source-level rules ───────────────────────────────────────────────────────────────────────────
grep -q "sceneStart(" $src && grep -q "sceneStep()" $src \
  && echo "  ok   driven by packages/scene, not a hand-rolled page counter" \
  || { echo "  FAIL no longer uses the scene machine"; fail=1; }

# ⚠️ THE ROM MUST NOT GAIN A `background:` IMPORT. A changing background image and a repainting UI
# canvas cannot coexist — the label renders as blocks of the page's own tile, and it is not fixable
# from tish (see the header of src/main.tish). The tilemap is `bgtiles:` precisely so this stays
# true, and adding a plate later is the one change that would silently undo it.
if grep -q "from 'background:" $src; then
  echo "  FAIL a background: image was added — it will fight the UI canvas for the label"; fail=1
else
  echo "  ok   no background: image; the pattern layer is a bgtiles: tilemap"
fi

# ⚠️ `bgtiles:`, NOT `background:`, for the tileset. `background:` passes agb's `deduplicate`, which
# collapses identical 8x8 tiles and shortens the table `tilemap_set8`'s indices point into — and
# this tileset is mostly flat fills, i.e. exactly what dedup collapses. The failure is silent: the
# indices past the first few draw nothing at all.
grep -q "from 'bgtiles:" $src \
  && echo "  ok   the tileset is imported without dedup, so tile indices are stable" \
  || { echo "  FAIL the tileset import is not bgtiles: — indices will not survive dedup"; fail=1; }

# ⚠️ EVERY PAGE PUTS BACK WHAT IT TOOK. BLDCNT, the terrain layer, the canvas and the tilemap are
# all shared and global; a page that leaves one dirty breaks the NEXT page, which is the hardest
# version of this to read from a screenshot.
for want in "fade_white(0)" "terrain_clear()" "ui_clear_rect" "clearPage()"; do
  if sed -n '/^function pageLeave/,/^}/p' $src | grep -qF "$want"; then
    echo "  ok   pageLeave restores: $want"
  else
    echo "  FAIL pageLeave no longer calls $want — the next page inherits the mess"; fail=1
  fi
done

# ── Runtime ──────────────────────────────────────────────────────────────────────────────────────
# A full lap plus the start of a second, so anything that leaks per page change has somewhere to
# show up — and so the wrap back to page 0 is exercised rather than assumed.
soak_rom $rom 3600 || fail=1

log=$(mktemp)
GBA_SHOT_LOG=1 $ROOT/scripts/screenshot.sh $rom /dev/null 3600 >"$log" 2>&1 || true

# Every page must be reached and named. A page that panics on entry simply never prints, and the
# cycle is modulo PAGES, so nothing else would notice.
for p in "cafe wall" "hermann grid" "ouchi" "kanizsa triangle" \
         "ebbinghaus" "muller-lyer" "lilac chaser" "afterimage" \
         "motion aftereffect" "barber pole"; do
  if grep -q "ILLUSIONS page .* $p build" "$log"; then
    echo "  ok   page reached: $p"
  else
    echo "  FAIL page never reached: $p"; fail=1
  fi
done

# ⚠️ "Ran out of video RAM for tiles" IS NOT A CRASH STRING, so soak_rom's regex does not see it.
# It is the expected failure if a page ever moves from the tilemap to a per-frame ui_rect fill.
if grep -q "Ran out of video RAM" "$log"; then
  echo "  FAIL the tile allocator ran dry — a page is allocating canvas tiles per cell"; fail=1
else
  echo "  ok   the tile allocator survived a full lap"
fi

# A page build has to stay inside the black hold, or the crossing visibly drags.
#
# ⚠️ THIS IS MEASURED IN FRAMES, NOT IN THE `build` TICK COUNTS THE ROM PRINTS. `ticks()` wraps
# about every fifteen frames, so a build slower than that wraps and comes back looking FAST — the
# barber pole's 1,024 tilemap writes report 2,267 ticks. A threshold on that number would pass
# precisely the pages it exists to catch. The interval between consecutive page-entry log lines is
# in frames and cannot wrap: it is dwell + transition + build, so a build that blows out shows up
# here as a longer gap and nowhere else.
gaps=$(grep -o "^\[frame [0-9]*\] ILLUSIONS page" "$log" | grep -o "[0-9]*" \
       | awk 'NR>1 {print $1-p} {p=$1}')
worst=$(echo "$gaps" | sort -n | tail -1)
if [ -n "$worst" ] && [ "$worst" -lt 420 ]; then
  echo "  ok   slowest page crossing is $worst frames (dwell 300 + transition 49 + build)"
else
  echo "  FAIL a page crossing takes ${worst:-?} frames — a build is overrunning the black hold"
  fail=1
fi
rm -f "$log"

# ── The per-page geometric claims ────────────────────────────────────────────────────────────────
# Frame numbers are mid-dwell for each page, read off the cadence the log prints above
# (entries as of this build: cafe 22, hermann 367, ouchi 691, kanizsa 1044, ebbinghaus 1376,
# muller 1700, lilac 2028, afterimage 2359, aftereffect 2687, barber 3025 — re-read the log and
# re-anchor these if the cadence moves again).
shoot() {
  $ROOT/scripts/screenshot.sh $rom "$2" "$1" >/dev/null 2>&1 || true
  [ -f "$2" ] || { echo "  FAIL no screenshot at frame $1"; fail=1; return 1; }
}

dir=$(mktemp -d)
for f in 177 534 871 1227 1570 1906 2178 2218 2591 2600 2900 2903 2940 2955 3130 3164; do
  shoot $f "$dir/$f.png" || true
done

python3 - "$dir" <<'PY' || fail=1
import sys
from PIL import Image
d = sys.argv[1]
bad = 0

def px(f):
    return list(Image.open(f"{d}/{f}.png").convert('RGB').getdata())

def count(f, c):
    return px(f).count(c)

def ok(msg):   print(f"  ok   {msg}")
def no(msg):
    global bad
    bad = 1
    print(f"  FAIL {msg}")

GREY   = (132, 132, 132)   # cafe-wall mortar / lilac field, 0x808080-ish through 5-bit
WHITE  = (255, 255, 255)
BLUE   = (33, 82, 198)     # the two Ebbinghaus centres
BLANK  = (148, 148, 148)   # the lilac disc currently hidden

# ── cafe wall: the mortar must be MID GREY and must be there. In black or white it is a plain
# checkerboard and the tilt does not happen — the single most likely silent regression on the page.
n = count(177, GREY)
if 1800 < n < 2600: ok(f"cafe wall has grey mortar lines ({n} px)")
else:               no(f"cafe wall mortar is missing or the wrong colour ({n} px grey)")

# ── ebbinghaus: THE CLAIM. The two centre discs are the same size. Split the blue pixels down the
# screen's midline and require the halves to match within a pixel or two of quantisation.
im = Image.open(f"{d}/1570.png").convert('RGB')
left  = sum(1 for y in range(160) for x in range(0, 120) if im.getpixel((x, y)) == BLUE)
right = sum(1 for y in range(160) for x in range(120, 240) if im.getpixel((x, y)) == BLUE)
if left > 300 and abs(left - right) <= 4:
    ok(f"ebbinghaus centres are identical ({left} vs {right} px)")
else:
    no(f"ebbinghaus centres differ — the page asserts they do not ({left} vs {right} px)")

# ── muller-lyer: THE CLAIM. The two shafts are the same length. Measure the longest run of white
# on each shaft's row rather than trusting the source constant.
im = Image.open(f"{d}/1906.png").convert('RGB')
def runlen(y):
    best = cur = 0
    for x in range(240):
        cur = cur + 1 if im.getpixel((x, y)) == WHITE else 0
        best = max(best, cur)
    return best
# ⚠️ Measure each shaft's TOP row. A shaft is 2px tall, and on its lower row the inward-pointing
# arrowheads touch the shaft's own ends and extend the white run by a pixel — so the two rows
# disagree by one for a reason that has nothing to do with the lengths being compared.
a, b = runlen(58), runlen(112)
if a > 100 and a == b: ok(f"muller-lyer shafts are the same length ({a} px each)")
else:                  no(f"muller-lyer shafts differ: {a} vs {b} px")

# ── lilac chaser: exactly one disc blanked, and the gap MOVES. A chaser stuck on one disc is a
# still picture that passes every static check.
g1 = count(2178, BLANK)
g2 = count(2218, BLANK)
if g1 > 200 and g2 > 200: ok(f"lilac chaser blanks a disc ({g1} px)")
else:                     no(f"lilac chaser has no blanked disc ({g1}, {g2} px)")
im1 = Image.open(f"{d}/2178.png").convert('RGB')
im2 = Image.open(f"{d}/2218.png").convert('RGB')
def centroid(im):
    pts = [(x, y) for y in range(160) for x in range(240) if im.getpixel((x, y)) == BLANK]
    return (sum(p[0] for p in pts) / len(pts), sum(p[1] for p in pts) / len(pts)) if pts else None
c1, c2 = centroid(im1), centroid(im2)
if c1 and c2 and (abs(c1[0] - c2[0]) > 8 or abs(c1[1] - c2[1]) > 8):
    ok("lilac chaser's gap moves round the ring")
else:
    no(f"lilac chaser's gap is stuck at {c1}")

# ── kanizsa: three separate black blobs, not one and not four. A wedge carved on the wrong ray
# still gives three blobs, so this is a floor rather than a proof — the direction is asserted in
# the source comment and checked by eye.
im = Image.open(f"{d}/1227.png").convert('RGB')
dark = {(x, y) for y in range(20, 160) for x in range(240) if sum(im.getpixel((x, y))) < 120}
seen, blobs = set(), 0
for p in dark:
    if p in seen: continue
    blobs += 1
    stack = [p]
    while stack:
        q = stack.pop()
        if q in seen or q not in dark: continue
        seen.add(q)
        stack += [(q[0]+1, q[1]), (q[0]-1, q[1]), (q[0], q[1]+1), (q[0], q[1]-1)]
if blobs == 3: ok("kanizsa shows three separate inducers")
else:          no(f"kanizsa shows {blobs} dark regions, expected 3")

# ── afterimage: the screen must actually reach full white, or there is nothing to stare away from.
w = count(2600, WHITE)
if w > 36000: ok(f"afterimage flashes to full white ({w} px)")
else:         no(f"afterimage never whited out ({w} px white)")

# ── motion aftereffect: THE CLAIM is a scroll that RUNS and then STOPS DEAD. Both halves matter and
# neither is visible in a single frame — a page that never scrolled and a page that never stopped
# both look like stripes. Compare three frames: two while it should be running (must differ) and
# two after the clamp (must be identical, pixel for pixel).
# ⚠️ A COLUMN, NOT A ROW. The stripes are HORIZONTAL and scroll VERTICALLY, so every row on this
# page is a single flat colour and stays one as it moves — sampling a row compares white to white
# and calls a scrolling page static. Sample down a column instead.
def colsig(f, x=10):
    im = Image.open(f"{d}/{f}.png").convert('RGB')
    return tuple(im.getpixel((x, y)) for y in range(20, 160))

def full(f):
    return tuple(px(f))

# ⚠️ AND THE TWO FRAMES ARE 3 APART, NOT 60. A repeating pattern ALIASES: the stripes have an 8px
# period and scroll 2px a frame, so any two frames 4n apart are pixel-identical however fast it is
# moving. 60 apart is 120px, exactly 15 periods — a perfect false negative.
if colsig(2900) != colsig(2903):
    ok("motion aftereffect is scrolling during its run")
else:
    no("motion aftereffect never moves — the stripes are static throughout")
# Both frames are after the clamp at t=240 (page 8 enters ~2687, clamp ~2927) and before the
# page's own transition starts (~2987) — the dwell counter runs from the last page CHANGE, not
# from enter(), so the window is narrower than dwell+enter suggests.
if full(2940) == full(2955):
    ok("motion aftereffect stops dead after the run (frames 2940 and 2955 identical)")
else:
    no("motion aftereffect is still moving after the clamp — there is no aftereffect without a hard stop")

# ── barber pole: THE CLAIM is stripes moving behind a slot that does NOT move. If the mask scrolled
# with them the whole illusion is gone, and a screenshot of either frame alone looks correct.
im1 = Image.open(f"{d}/3130.png").convert('RGB')
im2 = Image.open(f"{d}/3164.png").convert('RGB')
BACKDROP = (24, 24, 41)
def slot_edges(im):
    row = [im.getpixel((x, 100)) == BACKDROP for x in range(240)]
    return (row.index(False) if False in row else -1,
            len(row) - 1 - row[::-1].index(False) if False in row else -1)
e1, e2 = slot_edges(im1), slot_edges(im2)
if e1 == e2 and e1[0] > 0:
    ok(f"barber pole's aperture is fixed at columns {e1[0]}..{e1[1]}")
else:
    no(f"barber pole's aperture moved: {e1} then {e2}")
if [im1.getpixel((x, 100)) for x in range(e1[0], e1[1])] != \
   [im2.getpixel((x, 100)) for x in range(e1[0], e1[1])]:
    ok("barber pole's stripes move inside the aperture")
else:
    no("barber pole's stripes are static inside the aperture")

sys.exit(bad)
PY
rm -rf "$dir"

exit $fail
