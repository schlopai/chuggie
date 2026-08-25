#!/usr/bin/env bash
# verify.sh — headless assertions for the hardware effects layer.
#
# Resolve the script's directory BEFORE re-exec and carry it in the environment: inside the snapshot
# `$0` is the mktemp path, so `cd "$(dirname "$0")"` there lands in /tmp, every run comes back empty,
# and a suite whose checks are "grep found no panic" reports a clean pass having tested nothing.
if [ -z "${VERIFY_HOME:-}" ]; then
  VERIFY_HOME="$(cd "$(dirname "$0")" && pwd)"
  export VERIFY_HOME
  snap="$(mktemp -t fx-verify.XXXXXX)" || exit 1
  cat "$0" >"$snap"
  exec bash "$snap" "$@"
fi
set -u
cd "$VERIFY_HOME"
rm -f "$0"   # the open fd keeps it readable; nothing on disk to edit or leave behind

ROM=fx-particles.gba
[ -f "$ROM" ] || npm run build >/tmp/fxp-vbuild.log 2>&1 || true
[ -f "$ROM" ] || { echo "  FAIL $ROM missing:"; tail -20 /tmp/fxp-vbuild.log; exit 1; }
SHOT=../../scripts/screenshot.sh
mkdir -p shots
PASS=0
FAIL=0
ok()  { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; }

run() { LOG=$(GBA_SHOT_NOSAVE=1 GBA_SHOT_LOG=1 "$SHOT" "$ROM" "shots/$1.png" "$2" "$3" 2>&1); }

# "No panic" is only evidence when the ROM printed something. An empty log satisfies every
# absence-check in this file and would turn the suite green having run nothing.
nopanic() {
  if [ -z "$LOG" ]; then bad "$1 (NO OUTPUT — harness or ROM did not run)"; return; fi
  if grep -qi "panic\|out of video ram\|allocation of" <<<"$LOG"; then bad "$1"; else ok "$1"; fi
}

# The demo logs `FX alive <n>` once a second, which is the whole assertion surface: it reports what
# the ENGINE thinks it is stepping, not what this script hopes it spawned.
alive_max() { sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LOG" | sort -n | tail -1; }

echo "verify fx-particles: $ROM"

run 00-boot 90 ""
nopanic "boots"
[ "$(alive_max)" = "0" ] && ok "no particles before firing" || bad "no particles before firing"

# A burst spawns and then RETIRES itself. Both halves matter: particles hold sprite slots, so an
# effect that never dies is a leak that presents as sprites vanishing elsewhere much later.
run 01-burst 240 "120:a,132:"
nopanic "no panic on a burst"
N=$(alive_max)
[ "${N:-0}" -ge 20 ] && ok "burst spawned particles (peak $N)" || bad "burst spawned particles (peak ${N:-0})"
LAST=$(sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
[ "${LAST:-1}" = "0" ] && ok "particles retire themselves (ended $LAST)" || bad "particles retire themselves (ended ${LAST:-?})"

# fx_clear must free everything immediately — this is what a scene teardown relies on.
run 02-clear 240 "120:a,132:,150:start,162:"
LAST=$(sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
[ "${LAST:-1}" = "0" ] && ok "fx_clear frees every particle" || bad "fx_clear frees every particle (left ${LAST:-?})"

# Flash and shake touch registers rather than the particle list; they must not crash or leak.
run 03-flash 200 "60:b,72:,120:a,132:"
nopanic "no panic on flash"
run 04-shake 200 "60:b,72:,90:b,102:,140:a,152:"
nopanic "no panic on shake"

# SHAKE MOVES PIXELS. This is asserted with an image diff rather than "did not crash", because the
# first version of fx_shake did not crash and did nothing: it offset only the CAMERA, so a screen
# drawn on the UI canvas with HUD sprites over it — a result screen, a menu, this demo — had no
# camera-relative pixels to move. It has to shift the whole frame and then land square again.
run 06-shake-moves 210 "60:b,72:,90:b,102:,160:a,172:"
python3 - "$VERIFY_HOME" <<'PYEOF'
import sys, subprocess, os
home = sys.argv[1]
shot = os.path.join(home, "../../scripts/screenshot.sh")
rom  = os.path.join(home, "fx-particles.gba")
sch  = "60:b,72:,90:b,102:,160:a,172:"
env  = dict(os.environ, GBA_SHOT_NOSAVE="1")
def grab(frame, out):
    subprocess.run([shot, rom, out, str(frame), sch], cwd=home, env=env,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    from PIL import Image, ImageChops
except ImportError:
    print("  ok   shake pixel-diff SKIPPED (no Pillow)"); sys.exit(0)
grab(158, "/tmp/_fxs_before.png"); grab(166, "/tmp/_fxs_during.png"); grab(205, "/tmp/_fxs_after.png")
b = Image.open("/tmp/_fxs_before.png").convert("RGB")
d = Image.open("/tmp/_fxs_during.png").convert("RGB")
a = Image.open("/tmp/_fxs_after.png").convert("RGB")
moved  = ImageChops.difference(b, d).getbbox() is not None
landed = ImageChops.difference(b, a).getbbox() is None
print(("  ok   " if moved else "  FAIL ") + "shake moves the whole frame")
print(("  ok   " if landed else "  FAIL ") + "shake lands square when it ends")
sys.exit(0 if (moved and landed) else 1)
PYEOF
if [ $? -eq 0 ]; then PASS=$((PASS+2)); else FAIL=$((FAIL+1)); fi

# BUMPS SUM. The property the spring exists for, and the one a "did not crash" suite cannot see:
# six impulses in ONE frame must displace the screen further than one impulse does. A countdown
# shake fails this — the sixth call overwrites the fifth and the amplitude comes out identical.
run 07-bump 200 "60:b,72:,90:b,102:,120:b,132:"   # park in BUMP mode so a failure here is not a mode-select bug
nopanic "no panic on a bump cascade"
python3 - "$VERIFY_HOME" <<'PYEOF'
import sys, subprocess, os
home = sys.argv[1]
shot = os.path.join(home, "../../scripts/screenshot.sh")
rom  = os.path.join(home, "fx-particles.gba")
env  = dict(os.environ, GBA_SHOT_NOSAVE="1")
try:
    from PIL import Image
except ImportError:
    print("  ok   bump summing SKIPPED (no Pillow)"); sys.exit(0)

# The demo's 12px title bar is a flat colour, and at x=228 the whole column is exactly two colours —
# bar and body, no glyphs — so the bar's position in that column IS the canvas's vertical scroll,
# read straight off the picture.
#
# It takes BOTH the run start and the run height, because the two scroll directions do not look
# alike. Scrolled DOWN, the bar slides down the column intact and the run start is the offset.
# Scrolled UP, the rows below y=160 are canvas the demo never painted, so nothing wraps into view
# and the bar is simply CLIPPED at the top — the run start stays 0 and only the height falls. A
# first pass measured the run start alone, saw an unbroken string of non-negative readings, and
# missed the entire first (negative) half-swing of every shake.
#
# The pass before that thresholded on brightness down the MIDDLE column, picked up the hint text at
# y=137, and reported a 140px "amplitude" on a 160px screen — while passing. Both peaks are printed
# below for exactly that reason: an assertion whose number cannot be sanity-checked by eye is an
# assertion that can stay green while measuring the wrong thing.
H, COL, BAR, BAR_H = 160, 228, (33, 41, 66), 12

def scroll_at(path):
    px = Image.open(path).convert("RGB").load()
    rows = [y for y in range(H) if px[COL, y] == BAR]
    if not rows or len(rows) == H:
        return None
    if len(rows) < BAR_H:
        return -(BAR_H - len(rows))          # clipped at the top: scrolled up by the missing rows
    for y in rows:
        if y - 1 not in rows:
            return y
    return 0

# B thrice = mode 3 (BUMP, x3 by default), then DOWN twice for x1 or UP thrice for x6, then A.
def sched(tune, n):
    s, t = "60:b,72:,90:b,102:,120:b,132:", 150
    for _ in range(n):
        s += ",%d:%s,%d:" % (t, tune, t + 6)
        t += 12
    return s + ",200:a,206:"

def grab(s, f, tag):
    out = "/tmp/_fxb_%s_%d.png" % (tag, f)
    subprocess.run([shot, rom, out, str(f), s], cwd=home, env=env,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return scroll_at(out)

def amplitude(tune, n, tag):
    s = sched(tune, n)
    base = grab(s, 190, tag + "base")          # before the press: the resting scroll
    if base is None:
        return -1
    # The press at 200 reaches the spring around 202 and the first peak is ~2 frames after that, so
    # start before the press and sample EVERY frame: at x1 the whole excursion is one pixel, and at
    # x6 the true peak is visible for a single frame.
    peak = 0
    for f in range(198, 218):
        v = grab(s, f, tag)
        if v is not None:
            peak = max(peak, abs(v - base))
    return peak

one = amplitude("down", 2, "x1")
six = amplitude("up", 3, "x6")
# Bigger, and by more than a rounding pixel — six impulses should be several times one, not one
# pixel more. The upper bound catches a runaway spring, which would also "grow".
ok = one >= 0 and six >= one + 3 and six < 40
print("  %s bumps SUM (x1 peak %dpx, x6 peak %dpx)" % ("ok  " if ok else "FAIL", one, six))
sys.exit(0 if ok else 1)
PYEOF
if [ $? -eq 0 ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi

# ── Emitters ────────────────────────────────────────────────────────────────────────────────────
# An emitter is the thing fx_burst cannot be: a source that keeps going, at a FRACTIONAL rate. These
# read the demo's own log line, which reports what the ENGINE is holding rather than what the demo
# asked for — the distinction the whole budget rests on.
B5="60:b,72:,90:b,102:,120:b,132:,150:b,162:,180:b,192:"          # -> WEATHER (rain)
run 09-weather 420 "$B5"
nopanic "no panic with a continuous emitter running"
EM=$(sed -n 's/.*emit \([0-9]*\).*/\1/p' <<<"$LOG" | sort -n | tail -1)
[ "${EM:-0}" = "1" ] && ok "the emitter is live and reported (emit=$EM)" \
                     || bad "the emitter is live and reported (emit=${EM:-?})"
# Steady state is rate x life, not "as many as it can": 0.5/frame over 58 frames is ~29. A number
# near the budget instead would mean the rate accumulator is not actually limiting anything.
PK=$(sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LOG" | sort -n | tail -1)
[ "${PK:-0}" -ge 20 ] && [ "${PK:-99}" -le 60 ] \
  && ok "rain settles at rate x life, not at the ceiling (peak $PK)" \
  || bad "rain settles at rate x life (peak ${PK:-?}, wanted 20..60)"

# SELECT is fx_stop, not fx_kill: emission ends, the drops already falling live out their lives, and
# the emitter retires itself once the last one dies. A count that fell to zero the same frame would
# mean fx_stop was really a kill.
run 10-stop 700 "$B5,300:select,312:"
LASTP=$(sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
LASTE=$(sed -n 's/.*emit \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
[ "${LASTP:-1}" = "0" ] && [ "${LASTE:-1}" = "0" ] \
  && ok "fx_stop lets particles finish, then the emitter retires itself" \
  || bad "fx_stop drains cleanly (left ${LASTP:-?} particles, ${LASTE:-?} emitters)"

# ── THE BUDGET ──────────────────────────────────────────────────────────────────────────────────
# The reason this layer is in the engine at all. The GBA has 128 OAM entries for the WHOLE machine
# and nothing arbitrates them, so an effect that allocates greedily does not look busy — it makes
# the player and the NPCs disappear. The BUDGET demo runs a deliberately greedy emitter (4/frame,
# own ceiling lifted to 128) and adds 40 "game" sprites on top of it.
#
# The policy is exactly: particles + game sprites + reserve == 128.
B6="60:b,72:,90:b,102:,120:b,132:,150:b,162:,180:b,192:,210:b,222:"
HOGS="$B6"; t=280
for i in $(seq 1 10); do HOGS="$HOGS,$t:up,$((t+6)):"; t=$((t+14)); done
run 11-budget 620 "$HOGS"
nopanic "no panic with a saturated emitter plus game sprites"
LINE=$(grep 'FX alive' <<<"$LOG" | tail -1)
P=$(sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LINE")
G=$(sed -n 's/.*game \([0-9]*\).*/\1/p' <<<"$LINE")
[ "${G:-0}" = "40" ] && ok "the game took its 40 sprites (game=$G)" \
                     || bad "the game took its 40 sprites (game=${G:-?})"
# 128 - 40 game - 16 reserve = 72. Allow a couple either way: particles die and respawn between the
# log line and the frame, so the instantaneous count sits just under the ceiling as often as on it.
TOT=$(( ${P:-0} + ${G:-0} ))
[ "$TOT" -le 112 ] && [ "$TOT" -ge 104 ] \
  && ok "effects yielded OAM to the game (particles $P + game $G = $TOT, reserve keeps it <=112)" \
  || bad "effects yielded OAM to the game (particles ${P:-?} + game ${G:-?} = $TOT, wanted 104..112)"

# AND THE GAME'S SPRITES ACTUALLY RENDER. The count above can be perfect while every game sprite is
# invisible — which is exactly what happened here first: `sprite_new` makes a WORLD sprite at
# priority P2 and this demo paints an opaque UI canvas at P0 over it, so 40 sprites were allocated,
# counted, and behind the backdrop. Budget arithmetic is not evidence that anything is on screen.
python3 - "$VERIFY_HOME" <<'PYEOF'
import sys, subprocess, os
home = sys.argv[1]
shot = os.path.join(home, "../../scripts/screenshot.sh")
rom  = os.path.join(home, "fx-particles.gba")
env  = dict(os.environ, GBA_SHOT_NOSAVE="1")
try:
    from PIL import Image
except ImportError:
    print("  ok   game sprites visible SKIPPED (no Pillow)"); sys.exit(0)
s = "60:b,72:,90:b,102:,120:b,132:,150:b,162:,180:b,192:,210:b,222:"
t = 280
for _ in range(10):
    s += ",%d:up,%d:" % (t, t + 6)
    t += 14
# START is fx_clear: every particle goes, the demo's own sprites stay. What is left on screen is
# the game's 40 and nothing else, so the pixels are unambiguous.
s += ",600:start,612:"
out = "/tmp/_fxhogs.png"
subprocess.run([shot, rom, out, "660", s], cwd=home, env=env,
               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
px = Image.open(out).convert("RGB").load()
BG = (16, 16, 24)
# The two rows of game sprites live between the title bar and mid-screen.
lit = sum(1 for y in range(16, 70) for x in range(240) if px[x, y] != BG)
ok = lit >= 200
print("  %s the game's sprites are on screen after fx_clear (%d lit px, 40 sprites)"
      % ("ok  " if ok else "FAIL", lit))
sys.exit(0 if ok else 1)
PYEOF
if [ $? -eq 0 ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi

# THE RESERVE, on its own. Raising it must thin the effect WITHOUT a single game sprite existing —
# the slots are being held for sprites that have not been created yet, which is the case no amount
# of caller discipline covers and the reason the knob is not just "max particles". RIGHT is +8 each.
python3 - "$VERIFY_HOME" <<'PYEOF'
import sys, subprocess, os, re
home = sys.argv[1]
shot = os.path.join(home, "../../scripts/screenshot.sh")
rom  = os.path.join(home, "fx-particles.gba")
def alive(nright):
    s, t = "60:b,72:,90:b,102:,120:b,132:,150:b,162:,180:b,192:,210:b,222:", 260
    for _ in range(nright):
        s += ",%d:right,%d:" % (t, t + 6)
        t += 14
    env = dict(os.environ, GBA_SHOT_NOSAVE="1", GBA_SHOT_LOG="1")
    r = subprocess.run([shot, rom, "/tmp/_fxres.png", "620", s], cwd=home, env=env,
                       capture_output=True, text=True)
    hits = re.findall(r"FX alive (\d+)", r.stdout + r.stderr)
    return int(hits[-1]) if hits else -1
lo = alive(0)   # reserve 16
hi = alive(6)   # reserve 64
ok = lo > 0 and hi > 0 and lo - hi >= 30
print("  %s the reserve holds OAM back for sprites that do not exist yet "
      "(reserve 16 -> %d particles, reserve 64 -> %d)"
      % ("ok  " if ok else "FAIL", lo, hi))
sys.exit(0 if ok else 1)
PYEOF
if [ $? -eq 0 ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi

# fx_clear is a full teardown: emitters go with the particles, or the rain follows the player into
# the next scene and the handle the old scene held now steers whatever landed in that slot.
E=$(sed -n 's/.*emit \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
run 12-teardown 700 "$B5,300:start,312:"
LASTE=$(sed -n 's/.*emit \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
LASTP=$(sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
[ "${LASTE:-1}" = "0" ] && [ "${LASTP:-1}" = "0" ] \
  && ok "fx_clear takes the emitters with the particles" \
  || bad "fx_clear takes the emitters (left ${LASTE:-?} emitters, ${LASTP:-?} particles)"

# `power` MEANS PIXELS, at both ends of the range. fx_shake converts its two arguments onto the
# spring, and the first version of that conversion used a fixed impulse-per-pixel constant — which
# ignores that damping eats the peak. A small, quick shake came out as literally ZERO displacement:
# an effect that runs, decays, costs nothing and moves nothing, which is the same silent no-op this
# demo's whole verify.sh exists because of. The low end is the assertion that matters.
python3 - "$VERIFY_HOME" <<'PYEOF'
import sys, subprocess, os
home = sys.argv[1]
shot = os.path.join(home, "../../scripts/screenshot.sh")
rom  = os.path.join(home, "fx-particles.gba")
env  = dict(os.environ, GBA_SHOT_NOSAVE="1")
try:
    from PIL import Image
except ImportError:
    print("  ok   fx_shake amplitude SKIPPED (no Pillow)"); sys.exit(0)
H, COL, BAR, BAR_H = 160, 228, (33, 41, 66), 12

def scroll_at(path):
    px = Image.open(path).convert("RGB").load()
    rows = [y for y in range(H) if px[COL, y] == BAR]
    if not rows or len(rows) == H:
        return None
    if len(rows) < BAR_H:
        return -(BAR_H - len(rows))
    for y in rows:
        if y - 1 not in rows:
            return y
    return 0

# B twice = mode 2 (SHAKE, power 6 / settle 24 by default); UP/DOWN step the power by 2.
def sched(tune, n):
    s, t = "60:b,72:,90:b,102:", 130
    for _ in range(n):
        s += ",%d:%s,%d:" % (t, tune, t + 6)
        t += 12
    return s + ",240:a,246:"

def peak(tune, n, tag):
    s = sched(tune, n)
    def grab(f):
        out = "/tmp/_fxp_%s_%d.png" % (tag, f)
        subprocess.run([shot, rom, out, str(f), s], cwd=home, env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return scroll_at(out)
    base = grab(230)
    if base is None:
        return -1
    return max(abs((grab(f) or base) - base) for f in range(238, 262))

# Vertical only — the title bar spans the full width, so a horizontal shift is invisible on it, and
# fx_shake pushes Y at half the X impulse. Expect roughly power/2 pixels here.
lo = peak("down", 2, "p2")    # power 6 -> 2
hi = peak("up",   5, "p16")   # power 6 -> 16
ok = lo >= 1 and hi >= lo + 3 and hi < 30
print("  %s fx_shake power means pixels (p2 -> %dpx, p16 -> %dpx; p2 must not be 0)"
      % ("ok  " if ok else "FAIL", lo, hi))
sys.exit(0 if ok else 1)
PYEOF
if [ $? -eq 0 ]; then PASS=$((PASS+1)); else FAIL=$((FAIL+1)); fi

# fx_shake_stop must put the screen back square WITHOUT killing live particles — the distinction
# between it and fx_clear, which is the reason both exist.
run 08-stop 240 "60:a,72:,100:b,112:"
nopanic "no panic on shake_stop mid-burst"

# Repeated bursts must reuse sprite slots rather than growing the arena — 12 bursts back to back.
SOAK="60:a,72:"
t=100
for i in $(seq 1 12); do SOAK="$SOAK,$t:a,$((t+12)):"; t=$((t+60)); done
run 05-soak $((t+200)) "$SOAK"
nopanic "no panic across 13 bursts"
LAST=$(sed -n 's/.*FX alive \([0-9]*\).*/\1/p' <<<"$LOG" | tail -1)
[ "${LAST:-1}" = "0" ] && ok "arena settles back to empty after 13 bursts" || bad "arena settles (left ${LAST:-?})"

echo
echo "verify fx-particles: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
