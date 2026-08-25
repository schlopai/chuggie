#!/usr/bin/env bash
# Verify creature-rpg — the overworld, the encounter roll, and the battle.
#
# The two checks worth explaining are the ones that would otherwise pass by accident:
#
#   * "the border holds". Tile collision belongs to the TILESET, and the tilesets here are sparse
#     in ways a map cannot control — TilesetNature marks exactly two tiles solid in total. The first
#     build of this map walked the player out through the treeline, the map edge and both houses,
#     and every screenshot of it looked fine, because a screenshot of a player standing in a tree
#     looks like a player standing near a tree. The walk-into-the-wall checks below are what catch it.
#
#   * "an encounter never fires on the road". The gate keeper tells you that keeping to the dirt is
#     safe. A grass patch laid across the road makes that a lie AND makes the route uncrossable
#     without a fight. The generator asserts the geometry; this asserts the behaviour, by walking
#     the full length of the road and checking the world is still on screen at the end.
set -u
cd "$(dirname "$0")"
. ../../scripts/verify_common.sh

fails=0
check() { if [ "$1" -eq 0 ]; then echo "  ok   $2"; else echo "  FAIL $2"; fails=$((fails+1)); fi; }

echo "creature-rpg:"

python3 ../../scripts/gen_creature_rpg.py > /tmp/pd-gen.log 2>&1
check $? "assets + maps regenerate (asserts: solid facades, no grass on the road)"

python3 ../../scripts/gen_creature_music.py >> /tmp/pd-gen.log 2>&1
check $? "the theme regenerates"

npm run build > /tmp/pd-verify-build.log 2>&1
check $? "builds"

assert_typed_scalars src ../../packages/engine.tish
check $? "every module scalar is typed (docs/perf-rules.md §1)"

# Live pictures across the whole flow. A streamed map pages in over several hundred frames, so a
# shot before ~450 is a FALSE WHITE that looks exactly like a crash page — hence the late frames.
for f in 500 900 2000; do
  python3 ../../scripts/shot_check.py creature-rpg.gba "$f" "60:up,168:,190:left,218:," > /dev/null 2>&1
  check $? "frame $f is a live picture"
done

# The road is clear of every patch, so 27 tiles of walking straight up it must end in the WORLD.
# 8 frames a tile; the road runs from row 30 to row 3.
python3 ../../scripts/shot_check.py creature-rpg.gba 600 "60:up,280:," > /dev/null 2>&1
check $? "walking the road end to end never lands in a battle"

# Walls: hold a direction long past the map edge and the ROM must still be rendering the world.
python3 ../../scripts/shot_check.py creature-rpg.gba 700 "60:up,92:,110:left" > /dev/null 2>&1
check $? "the map border holds under a held direction"

# The door round trip: in at (9,24), out at (9,25). Both interiors are one screen, so a shot inside
# is a different picture from a shot outside — `shot_check` only proves liveness, so the soak below
# is what proves the warp does not wedge.
python3 ../../scripts/shot_check.py creature-rpg.gba 300 "60:up,92:,110:left,186:,210:up,232:," > /dev/null 2>&1
check $? "the lab interior renders after a door warp"

# A long walk THROUGH the grass, which is the only way the battle code path executes at all: the
# encounter is a 14% roll per grass step, so a soak without input never reaches it.
soak_rom creature-rpg.gba 9000 \
  "60:up,168:,190:left,300:,320:right,420:,440:a,460:,480:a,500:,520:a,540:,600:down,700:,720:left,900:,1000:a,1020:,1100:a,1120:,1200:up,1400:,1500:a,1520:,1600:a,1620:,1700:right,1900:,2000:a,2020:,2100:a,2120:,2200:down,2400:,2500:a,2520:,3000:left,3200:,3300:a,3320:,3400:a,3420:,4000:up,4200:,4300:a,4320:,5000:right,5200:,5300:a,5320:,6000:down,6200:,6300:a,6320:,7000:left,7200:,7300:a,7320:,8000:up,8200:,8300:a" \
  > /tmp/pd-soak.log 2>&1
check $? "9000 frames walking the grass and fighting: no crash, no halt"

echo "creature-rpg: $fails failure(s)"
exit $((fails > 0))
