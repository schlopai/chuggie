#!/usr/bin/env bash
# sunnyside verify — the flagship: boots the generated island, accepts the
# goblin's quest through the dialog system, walks home, sleeps (autosave),
# restores on a second boot, then survives a four-day unattended soak.
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..
fail=0
CRASH='Bad memory|Unimplemented memory|panicked at|Illegal opcode|Jumped to invalid address|SpriteFull'

python3 "$ROOT/scripts/const_to_let.py" --check src >/dev/null 2>&1 \
  && echo "  ok   const_to_let clean" \
  || { echo "  FAIL const_to_let --check"; fail=1; }

# generator artifacts + the worldgen twin still agree (the ROM's island is
# seed 7 of the same generator sunnyside-worldgen verifies exhaustively).
# The source pack is not vendored (its license forbids it) — the re-bake only
# runs where SUNNYSIDE_SRC (or a local raw/ copy) provides it. The committed
# baked/ output is what the ROM build below actually consumes either way.
SUN_SRC="${SUNNYSIDE_SRC:-$ROOT/assets/sunnyside}"
if [ -d "$SUN_SRC/raw" ]; then
  SUNNYSIDE_SRC="$SUN_SRC" python3 "$ROOT/scripts/gen_sunnyside_pack.py" >/dev/null \
    && echo "  ok   gen_sunnyside_pack regenerates" \
    || { echo "  FAIL gen_sunnyside_pack.py"; fail=1; }
  if ! git diff --quiet -- ../../assets/sunnyside/baked src/data_world.tish src/data_anim.tish 2>/dev/null; then
    echo "  FAIL baked assets drifted from generator output"; fail=1
  else
    echo "  ok   baked assets match generator"
  fi
else
  echo "  skip re-bake (source pack not present; set SUNNYSIDE_SRC to enable)"
fi

rm -rf .tish
unset CARGO_TARGET_DIR
npm run build >/tmp/sunnyside-build.log 2>&1 \
  || { echo "  FAIL build (see /tmp/sunnyside-build.log)"; exit 1; }
echo "  ok   build"

rm -f tish-agb-sunnyside.sav
l1=$(mktemp); l2=$(mktemp); l3=$(mktemp)

# run 1 — title, follow the path east and north, round house2 to its door
# (seed-7 island), take the goblin's quest through the dialog choices, walk
# home across the farm, sleep
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside.gba /tmp/sunnyside-v1.png 2400 \
  "120:start,130:,600:right,700:,705:up,905:,910:right,1050:,1055:down,1100:,1105:left,1265:,1280:a,1290:,1430:a,1440:,1570:a,1580:,1620:right,1750:,1755:down,1905:,1910:left,2055:,2070:a,2080:" \
  >"$l1" 2>&1 || { echo "  FAIL run 1 crashed"; tail -20 "$l1"; exit 1; }
grep -Eq "$CRASH" "$l1" && { echo "  FAIL crash in run 1"; grep -E "$CRASH" "$l1" | head -3; fail=1; } \
  || echo "  ok   run 1: no crash lines"
grep -q 'SUNNYSIDE READY' "$l1" \
  && echo "  ok   world boots (procgen island, streamed)" \
  || { echo "  FAIL no READY"; fail=1; }
grep -q 'QUEST ACCEPTED' "$l1" \
  && echo "  ok   dialog choices: the goblin's quest accepted" \
  || { echo "  FAIL quest dialog"; fail=1; }
grep -q 'DAY day=2 grown=0 passout=0' "$l1" \
  && echo "  ok   voluntary sleep at the barn door -> day 2" \
  || { echo "  FAIL sleep"; grep 'DAY ' "$l1" | head -3; fail=1; }
grep -q 'SAVED day=2' "$l1" \
  && echo "  ok   autosave on sleep" \
  || { echo "  FAIL autosave"; fail=1; }

# run 2 — same .sav: the save must restore day 2 and the accepted quest
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside.gba /tmp/sunnyside-v2.png 700 \
  "120:start,130:" >"$l2" 2>&1 || { echo "  FAIL run 2 crashed"; exit 1; }
grep -q 'LOADED day=2 gold=120 quest=1' "$l2" \
  && echo "  ok   restore: day 2, quest remembered" \
  || { echo "  FAIL restore:"; grep 'LOADED' "$l2"; fail=1; }

# run G — gathering: cast at the south shore, catch on the deterministic
# bite (play-rng stream), cross the bridge, fell the tree at (29,40) with
# three axe hits, then idle to the 02:00 pass-out (which autosaves); the
# reboot must restore wood and fish
rm -f tish-agb-sunnyside.sav
lg=$(mktemp); lg2=$(mktemp)
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside.gba /tmp/sunnyside-vg.png 5600 \
  "120:start,130:,600:l,608:,615:down,655:,665:a,675:,812:a,822:,900:l,908:,915:up,959:,965:right,1115:,1120:down,1148:,1152:right,1154:,1165:a,1175:,1215:a,1225:,1265:a,1275:" \
  >"$lg" 2>&1 || { echo "  FAIL gather run crashed"; tail -10 "$lg"; exit 1; }
grep -q 'CHOP wood=2' "$lg" \
  && echo "  ok   three axe hits felled the tree (wood=2)" \
  || { echo "  FAIL chop"; fail=1; }
grep -q 'FISH CAUGHT fish=1' "$lg" \
  && echo "  ok   cast, bite, catch (fish=1)" \
  || { echo "  FAIL fishing"; grep FISH "$lg" | head -3; fail=1; }
grep -q 'MUSH spawned=5' "$lg" \
  && echo "  ok   five forage mushrooms spawned" \
  || { echo "  FAIL mushroom spawn"; fail=1; }
grep -q 'SAVED day=2 gold=120 w=2 f=1' "$lg" \
  && echo "  ok   gathered goods autosaved at pass-out" \
  || { echo "  FAIL gather save"; grep SAVED "$lg"; fail=1; }
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside.gba /tmp/sunnyside-vg2.png 700 \
  "120:start,130:" >"$lg2" 2>&1
grep -q 'LOADED day=2 gold=120 quest=0 w=2 f=1' "$lg2" \
  && echo "  ok   wood and fish restored on reboot" \
  || { echo "  FAIL gather restore"; grep LOADED "$lg2"; fail=1; }

# run S — the store: walk to the shopkeeper, through the greeting into the
# BUY tab (packages/shop), buy carrot seeds twice through the qty stepper
rm -f tish-agb-sunnyside.sav
ls=$(mktemp)
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside.gba /tmp/sunnyside-vs.png 2400 \
  "120:start,130:,600:right,700:,705:up,830:,835:right,1035:,1040:up,1080:,1120:a,1130:,1250:a,1260:,1380:a,1390:,1520:a,1530:,1660:a,1670:,1800:a,1810:,1950:a,1960:" \
  >"$ls" 2>&1 || { echo "  FAIL shop run crashed"; tail -10 "$ls"; exit 1; }
grep -Eq "$CRASH" "$ls" && { echo "  FAIL crash at the shop"; grep -E "$CRASH" "$ls" | head -3; fail=1; } \
  || echo "  ok   shop run: no crash lines"
n_buys=$(grep -c 'BOUGHT crop=0' "$ls" || true)
if [ "$n_buys" -ge 2 ]; then
  echo "  ok   packages/shop: bought seeds twice through the qty stepper"
else
  echo "  FAIL shop purchases: $n_buys"; grep 'BOUGHT' "$ls"; fail=1
fi

# run 3 — a fresh four-day unattended soak: pass-out cycles, flat heap
rm -f tish-agb-sunnyside.sav
GBA_SHOT_LOG=1 "$ROOT/scripts/screenshot.sh" tish-agb-sunnyside.gba /tmp/sunnyside-v3.png 21000 \
  "120:start,130:" >"$l3" 2>&1 || { echo "  FAIL soak crashed"; tail -20 "$l3"; exit 1; }
grep -Eq "$CRASH" "$l3" && { echo "  FAIL crash in soak"; grep -E "$CRASH" "$l3" | head -3; fail=1; } \
  || echo "  ok   soak: no crash lines"
days=$(grep -c 'passout=1' "$l3" || true)
if [ "$days" -ge 3 ]; then
  echo "  ok   soak: $days unattended pass-out day cycles"
else
  echo "  FAIL soak reached only $days day cycles"; fail=1
fi
uniq_h=$(grep -o 'HEAP [0-9]*' "$l3" | sort -u | wc -l | tr -d ' ')
if [ "$uniq_h" -le 2 ]; then
  echo "  ok   heap steady across days ($(grep -o 'HEAP [0-9]*' "$l3" | sort -u | tr '\n' ' '))"
else
  echo "  FAIL heap drift: $(grep -o 'HEAP [0-9]*' "$l3" | tr '\n' ' ')"; fail=1
fi

# night really darkens the world: frame 4400 is ~22:00 of day 1
rm -f tish-agb-sunnyside.sav
"$ROOT/scripts/screenshot.sh" tish-agb-sunnyside.gba /tmp/sunnyside-night.png 4400 \
  "120:start,130:" >/dev/null 2>&1
python3 - <<'EOF'
from PIL import Image
def mean(p):
    im = Image.open(p).convert('RGB')
    px = list(im.getdata())
    return sum(sum(c) for c in px) / len(px) / 3
day = mean('/tmp/sunnyside-v2.png')       # 06:xx, full light
night = mean('/tmp/sunnyside-night.png')  # ~22:00, BLDY level 7
assert night < day * 0.75, f"night {night:.1f} not darker than day {day:.1f}"
print(f"  ok   night brightness {night:.1f} vs day {day:.1f}")
EOF
[ $? = 0 ] || fail=1

echo
[ "$fail" = 0 ] && echo "sunnyside verify: PASS" || echo "sunnyside verify: FAIL"
exit $fail
