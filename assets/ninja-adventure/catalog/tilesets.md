# Ninja Adventure — Tileset catalog

Every map tileset in `Backgrounds/Tilesets/`: theme, tile grid, autotile regions, notable
tiles (doors, stairs, water/cliff edges), and how to use each when building a map. Tile coords
are (col,row); **gid = row*cols + col + 1**. Usable Tiled wangsets / mask tables live in
[`autotile.json`](autotile.json) (Floor×8, WallSimple×4, InteriorFloor floors+walls+carpets,
Hole, Water×4, FloorB, Field×5, bed stone). Modular kits (Pipes, Desert walls, Relief cliffs)
are documented below but are **not** wangsets — hand-place. Regenerate masks with
`scripts/gen_autotile_masks.py`, then `scripts/gen_tileset_library.py` for `tiled/*.tsj`.

**Pixel-level coverage: 23/23 tilesets have every occupied tile cell
accounted for** (verified by `scripts/tileset_coverage.py`, which cross-checks every
non-transparent 16x16 cell against the region/structure/notable-tile text below — not just
eyeballed). Regenerate this file after any catalog edit with `scripts/gen_tilesets_md.py`.
## Interior Elements (misc props)  (Backgrounds/Tilesets/Interior/Elements.png)  grid=(9, 3)

**Coverage: 20/20 occupied cells documented — ✅ COMPLETE**

**Theme:** small miscellaneous interior props (wall pilaster, cushions/barrels, boxes)

**Palette:** orange brick + grey/dark objects + green objects, on an olive-green backdrop

**Description:** A tiny grab-bag sheet (9x3 tiles) of loose interior props over an olive backdrop: an orange brick wall pilaster on the left, a set of grey rounded cushions/benches (actually 3 tiles wide, cols 2-4) and larger dark barrel-or-couch objects in the middle, and a green seamed box plus a green pouch/slime-like object on the right. No autotiling — each item is placed individually as decoration.

**Furniture:**
- orange brick wall pilaster / column: cols 0-1 rows 0-2
- grey cushion / bench (upper): cols 2-4 rows 0-0
- grey cushion / bench (lower): cols 2-4 rows 1-1
- dark barrels / padded couch: cols 5-6 rows 0-2
- green box / cushion: cols 7-8 rows 0-0

**Regions:**
- cols 0-1 rows 0-2: orange brick wall pilaster / column with central vertical seam (2 wide x 3 tall) `wall,pillar,brick,orange`
- cols 2-4 rows 0-1: grey rounded cushions / benches / logs (two stacked 3x1 objects, actually spanning cols 2-4, not just 3-4) `cushion,bench,grey,prop`
- cols 5-6 rows 0-2: large dark grey object(s) — pair of barrels or a padded couch/seat `barrel,couch,dark,prop`
- cols 7-8 rows 0-1: green box/cushion with horizontal seams (7,0) and green rounded pouch/slime with dark eyes (8,0) `box,green,prop`

**Notable tiles:**
- (0,0) gid=1: orange brick pilaster top-left tile
- (7,0) gid=8: green seamed box / cushion
- (8,0) gid=9: green rounded pouch / slime prop with dark eyes
- (2,0) gid=3: left edge of the grey rounded cushion/bench/log object; the object is 3 tiles wide (cols 2-4), not 2 (cols 3-4) as the tile grid first suggested

**Map-building use:** Sprinkle these single props into finished interiors: stand the orange brick pilaster against a wall as a column, place grey cushions/barrels or the couch as seating, and drop the green box/pouch as clutter or a container.

## Interior Pipes / Ornate Tubing  (Backgrounds/Tilesets/Interior/TilesetInterior.png)  grid=(16, 20)

**Coverage: 262/262 occupied cells documented — ✅ COMPLETE**

**Theme:** interior decoration — connective pipes / ornate wall tubing that link into rectangular loops

**Palette:** four palettes arranged in quadrants: cream/tan (top-left), orange (top-right), brown/dark (bottom-left), green (bottom-right)

**Description:** A decorative connective-pipe (or ornate tubing/trim) tileset presented in four color palettes filling the four quadrants of the sheet: cream, orange, dark-brown, and green. Each palette repeats the same kit of rounded corners, straight runs, T/cross junctions, and end caps that snap together into rectangular frames and branching manifolds. Two grate/vent tiles sit in the lower half. These are overlay props, not autotiling terrain, though the segments interlock like one.

**Regions:**
- cols 0-7 rows 0-9: cream/tan pipe & tube segment set: outer rounded-corner frame, straight horizontal/vertical runs, inner branching manifold, T and cross junctions, end caps `pipes,tubing,decoration,cream,connective`
- cols 8-15 rows 0-9: orange pipe & tube segment set (same layout as cream block, recolored) `pipes,tubing,decoration,orange,connective`
- cols 0-7 rows 10-19: brown/dark pipe & tube segment set (same layout, recolored); includes a grate/vent tile `pipes,tubing,decoration,dark,brown,connective`
- cols 8-15 rows 10-19: green pipe & tube segment set (same layout, recolored); includes a grate/vent tile `pipes,tubing,decoration,green,connective`

**Notable tiles:**
- (5,11) gid=182: white-striped grate / floor vent (dark-brown palette); approximate position
- (13,11) gid=190: white-striped grate / floor vent (green palette); approximate position

**Map-building use:** Use on a decoration/overlay layer above floors and walls to suggest plumbing, ducting, or ornate room trim. Pick one palette per room, then chain corner + straight + junction tiles to draw a connected loop; drop the grate tile where a vent is wanted.

## Interior Floors & Walled Rooms  (Backgrounds/Tilesets/Interior/TilesetInteriorFloor.png)  grid=(22, 17)

**Coverage: 294/294 occupied cells documented — ✅ COMPLETE**

**Theme:** indoor floors and walled-room wall sets for building interior rooms

**Palette:** cream/tan brick, orange brick, ornate gold, green ornate, plain tan floor, dark cobblestone

**Description:** The primary interior floor-and-wall sheet. The left half holds two brick walled-room wall sets (cream/tan and orange) plus a plain tan plank floor for filling rooms. The right half holds ornate gold and green medallion carpets/walled blocks over a dark cobblestone floor for cellars or grand halls. A few small sparkle/cobweb accent decals sit in the gaps between palette blocks. A handful of extra overflow tiles (col 10 and col 21) continue the brick/floor/rug/cobble blocks one column further; a tall glow/light-shaft decal sits at the far-left column between each pair of stacked wall blocks; and small carved-rune glyph tiles plus flat color-fill swatches sit just right of the small gold/green ornate motif blocks.

**Autotile:**
- [walled-room-3x3] cream/tan brick wall: cols 0-9 rows 0-4 — Verified cream_brick (large cols 4-8) + cream_brick_sm (cols 0-3) wangsets in autotile.json; masks from WallSimple cream_wall.
- [walled-room-3x3] orange brick wall: cols 0-9 rows 6-10 — Verified orange_brick + orange_brick_sm wangsets (cream + row offset 6).
- [carpet-frame] gold ornate carpet: cols 15-20 rows 0-4 — Verified gold_ornate carpet-frame wangset (cols 15-19 rows 0-3) in autotile.json.
- [walled-room-3x3] green ornate wall: cols 15-20 rows 6-11 — Verified green_ornate carpet-frame wangset (cols 15-19 rows 6-9) in autotile.json.
- [3x3] tan plank/tile floor: cols 0-9 rows 12-16 — Verified tan_plank 3x3 wangset at cols 0-2 rows 12-14; cols 3+ are fill variants.
- [3x3] dark cobblestone floor: cols 11-20 rows 12-16 — Verified dark_cobble 3x3 island at cols 11-13 rows 12-14; fill 2x2 bond (16-17,13-14). Indexed: catalog/tilemaps/TilesetInteriorFloor_dark_cobble_indexes.png.

**Furniture:**
- large gold ornate rug / medallion carpet: cols 15-20 rows 0-4
- green ornate rug / medallion carpet: cols 15-20 rows 6-11

**Regions:**
- cols 11-14 rows 0-3: small tan/gold ornate floor-or-ceiling motif tiles (4-tile medallions) `ornate,gold,motif,carpet`
- cols 15-20 rows 0-4: large gold ornate rug / walled medallion: decorative border framing a central medallion floor `rug,carpet,gold,ornate,medallion`
- cols 11-14 rows 6-9: small green ornate floor motif tiles `ornate,green,motif,carpet`
- cols 0-9 rows 0-4: cream/tan brick walled-room wall set `wall,brick,tan,walled-room`
- cols 0-9 rows 6-10: orange brick walled-room wall set `wall,brick,orange,walled-room`
- cols 0-9 rows 12-16: plain tan plank/tile floor fill `floor,tan,interior`
- cols 11-20 rows 12-16: dark cobblestone floor fill `floor,dark,cobblestone,dungeon`
- col 10 rows 2-3: extra cream/tan brick wall fill tiles, a one-column overflow of the tan brick wall block (cols 0-9 rows 0-4) spilling into col 10 `wall,brick,tan,overflow`
- col 10 rows 8-9: extra orange brick wall fill tiles, a one-column overflow of the orange brick wall block (cols 0-9 rows 6-10) spilling into col 10 `wall,brick,orange,overflow`
- col 10 rows 14-15: extra tan plank/tile floor fill tiles, a one-column overflow of the tan floor block (cols 0-9 rows 12-16) spilling into col 10 `floor,tan,overflow`
- col 21 rows 2-3: extra gold ornate motif/rug fill tiles, a one-column overflow of the large gold rug block (cols 15-20 rows 0-4) spilling into col 21 `rug,carpet,gold,ornate,overflow`
- col 21 rows 8-9: extra green ornate motif fill tiles, a one-column overflow of the green walled-room/rug block (cols 15-20 rows 6-11) spilling into col 21 `rug,carpet,green,ornate,overflow`
- col 21 rows 14-15: extra dark cobblestone floor fill tiles, a one-column overflow of the dark cobblestone floor block (cols 11-20 rows 12-16) spilling into col 21 `floor,dark,cobblestone,overflow`

**Notable tiles:**
- (12,5) gid=123: small accent decal (sparkle/cloud/cobweb) between the gold and orange blocks
- (13,5) gid=124: accent decal (sparkle/cobweb)
- (14,5) gid=125: accent decal (sparkle/star)
- (12,11) gid=255: accent decal (sparkle/cobweb) between green and dark blocks
- (13,11) gid=256: accent decal (sparkle/cobweb)
- (14,11) gid=257: accent decal (sparkle/star)
- (0,5) gid=111: tall glow/light-shaft decal (tan palette): two brick merlons at top over a bright white radial glow beneath, sitting in the seam between the tan wall block (rows 0-4) and the orange wall block (rows 6-10) at the left edge, col 0
- (0,11) gid=243: tall glow/light-shaft decal (orange palette recolor of the tile at (0,5)): brick merlons over a bright white radial glow, sitting between the orange wall block (rows 6-10) and the tan floor block (rows 12-16) at the left edge, col 0
- (11,4) gid=100: small tan carved rune/glyph decal (H-shaped symbol) on a standalone tile, just right of the gold ornate motif block (cols 11-14 rows 0-3)
- (11,5) gid=122: solid flat brown/tan color-fill swatch tile (no pattern), left of the sparkle decals at cols 12-14 row 5
- (11,10) gid=232: small green carved rune/glyph decal (H-shaped symbol), mirroring the tan glyph at (11,4), just right of the green ornate motif block (cols 11-14 rows 6-9)
- (11,11) gid=254: solid flat dark-green color-fill swatch tile (no pattern), mirroring the brown fill at (11,5), left of the sparkle decals at cols 12-14 row 11

**Map-building use:** Build the room shell from a walled-room wall set (tan or orange bricks, or the ornate gold/green borders), fill the interior with the matching floor terrain (plain tan planks or dark cobble), then drop an ornate rug/medallion carpet in the room center and scatter the small sparkle/cobweb decals for detail.

## Simple Walls  (Backgrounds/Tilesets/Interior/TilesetWallSimple.png)  grid=(10, 11)

**Coverage: 92/92 occupied cells documented — ✅ COMPLETE**

**Theme:** simple square interior wall frames with a decorative center medallion, in four palettes

**Palette:** cream/tan, orange, brown/dark, green — one palette per quadrant

**Description:** A compact simple-wall sheet: four rounded square wall frames, one per quadrant, in cream/tan, orange, dark-brown, and green. Each frame is a walled-room set (corners plus edge runs) with a decorative round medallion at its center — a target ring in the tan/orange versions and a spiral in the dark/green versions. The two lower (dark and green) blocks each add a small striped grate/vent tile. A single flat, uniform dark teal-black color-fill tile repeats across the whole of row 5 (cols 0-9), forming a plain divider/void strip between the upper (tan/orange) and lower (dark/green) wall-frame quadrants.

**Autotile:**
- [walled-room-3x3] cream/tan wall: cols 0-4 rows 0-4 — Verified cream_wall wangset in autotile.json (dual inner/outer corners).
- [walled-room-3x3] orange wall: cols 5-9 rows 0-4 — Verified orange_wall wangset (same layout as cream).
- [walled-room-3x3] brown/dark wall: cols 0-4 rows 6-10 — Verified brown_wall wangset; block also includes a grate/vent tile on the left edge.
- [walled-room-3x3] green wall: cols 5-9 rows 6-10 — Verified green_wall wangset; block also includes a grate/vent tile on the left edge.

**Furniture:**
- round floor medallion / rug (tan, target ring): cols 2-3 rows 1-2
- round floor medallion / rug (orange, target ring): cols 7-8 rows 1-2
- round floor medallion / rug (dark, spiral): cols 2-3 rows 7-8
- round floor medallion / rug (green, spiral): cols 7-8 rows 7-8

**Regions:**
- cols 0-4 rows 0-4: cream/tan square wall frame + centered ring medallion `wall,tan,walled-room`
- cols 5-9 rows 0-4: orange square wall frame + centered ring medallion `wall,orange,walled-room`
- cols 0-4 rows 6-10: brown/dark square wall frame + centered spiral medallion + grate `wall,dark,walled-room`
- cols 5-9 rows 6-10: green square wall frame + centered spiral medallion + grate `wall,green,walled-room`
- cols 0-9 row 5: solid flat near-black teal-grey color-fill tile (RGB approx 20,27,27), byte-identical across all 10 columns — a plain divider/void filler strip separating the tan/orange wall quadrants (rows 0-4) from the dark/green wall quadrants (rows 6-10) `divider,fill,shadow,void,filler`

**Notable tiles:**
- (1,7) gid=72: white-striped grate / floor vent (dark palette)
- (6,7) gid=77: white-striped grate / floor vent (green palette)
- (3,2) gid=24: tan ring-medallion center tile
- (8,2) gid=29: orange ring-medallion center tile
- (0,5) gid=51: solid flat color-fill tile (near-black teal, RGB ~20,27,27); the same flat fill repeats unchanged across cols 0-9 row 5 as a divider strip between the two wall-frame rows

**Map-building use:** Fastest way to box in a small interior room: pick a palette, lay the four corner tiles and edge runs as a rectangle, then optionally place the matching round medallion (rug/seal) in the room center and a grate tile on the floor. Good for closets, cells, and shrine rooms.

## Pipes  (Backgrounds/Tilesets/Pipes.png)  grid=(15, 3)

**Coverage: 42/42 occupied cells documented — ✅ COMPLETE**

**Theme:** Connectable segmented pipe/tube props in 3 color variants (reads as segmented pipe-worms)

**Palette:** Three variants of five columns each: orange/tan (cols 0-4), silver-gray (cols 5-9), olive-green (cols 10-14). Dark outlines with small black accents (valve/eye and bolt marks).

**Description:** A modular pipe/tube construction set in three color variants (orange, silver-gray, olive-green), each a self-contained 5-column by 3-row kit. Every kit provides a vertical straight run with a rounded end-cap/valve head, a horizontal straight run with a matching head, and two 2x2 rounded pipe rings whose corners serve as elbows. The pieces read visually as segmented pipe-worms and snap together end-to-end, but this is a hand-placed segment/junction kit, not a Godot 47-blob corner-match autotile.

**Autotile:**
- [modular segment/junction set (straight, ring-elbow, end-cap) — NOT a Godot 47-blob] pipe / segmented tube: cols 0-4 rows 0-2 (orange), cols 5-9 (gray), cols 10-14 (green) — Connectable like a wang set only in the loose sense: pieces snap end-to-end and form rings. Each color variant is one 5-col x 3-row kit: a vertical straight run in col 0 ending in a rounded end-cap/valve at the bottom (row 2), a horizontal straight run across row 0 (cols 1-4) with a rounded head/end, and two 2x2 rounded pipe rings (cols 1-2 and cols 3-4, rows 1-2) whose corner tiles act as elbows. Hand-place; there is no corner-match blob covering all 47 cases.

**Regions:**
- cols 0-4 rows 0-2: Orange/tan pipe kit: vertical run + end-cap (col 0), horizontal run with head (row 0 cols 1-4), and two 2x2 pipe rings (rows 1-2). `pipe,orange,autotile-ish,kit`
- cols 5-9 rows 0-2: Silver-gray pipe kit, same piece layout as the orange kit. `pipe,gray,kit`
- cols 10-14 rows 0-2: Olive-green pipe kit, same piece layout as the orange kit. `pipe,green,kit`

**Notable tiles:**
- (0,0) gid=1: orange vertical straight, top segment
- (0,2) gid=31: orange vertical end-cap / valve head (rounded, black accent)
- (1,0) gid=2: orange horizontal straight, left segment
- (4,0) gid=5: orange horizontal head / right end-cap
- (1,1) gid=17: orange pipe ring top-left elbow (ring A, cols 1-2 rows 1-2)
- (2,1) gid=18: orange pipe ring top-right elbow (ring A)
- (1,2) gid=32: orange pipe ring bottom-left elbow (ring A)
- (2,2) gid=33: orange pipe ring bottom-right elbow (ring A)
- (3,1) gid=19: orange pipe ring top-left elbow (ring B, cols 3-4 rows 1-2)
- (5,2) gid=36: gray vertical end-cap / valve head (gray kit start col 5)
- (10,2) gid=41: green vertical end-cap / valve head (green kit start col 10)

**Map-building use:** Decorative pipe/tube runs for dungeons, sewers, factories or as segmented-creature bodies. Chain the horizontal (row 0) and vertical (col 0) straight pieces end-to-end and cap the ends with the rounded valve/head tile; assemble the 2x2 ring blocks for junctions/loops. Pick one of the three color variants per run: orange = cols 0-4, gray = cols 5-9, green = cols 10-14. gid = row*15 + col + 1; the gray and green kits are the orange gids shifted by +5 and +10 columns.

## TilesetDesert  (Backgrounds/Tilesets/TilesetDesert.png)  grid=(20, 12)

**Coverage: 228/228 occupied cells documented — ✅ COMPLETE**

**Theme:** desert ruins / oasis (sandstone buildings, palms, statues, water)

**Palette:** warm sand and tan sandstone with white cap-rims and brick coursing; green palm/dome foliage; light-blue oasis water; accents of red/yellow/blue market cloth, bone-white statues

**Description:** A dense desert-ruins theme sheet built around a modular sandstone wall kit (faces, corners, battlements, barred windows, dark doorways) for constructing temples, keeps and courtyards. It adds large set pieces - green-domed turrets, a battlemented keep, sun-face and sphinx statues, a well - plus an oasis pool, a tiled bath, a palm-tree grove, market tents/stalls, treasure chests and scattered bones. Used chiefly on the object/building layer above plain sand ground.

**Autotile:**
- [wall-kit (edges-only)] sandstone ruin wall: cols 6-19 rows 0-5 — Modular temple/ruin wall building set rather than a flat terrain blob: brick-coursed wall faces with white cap-rim tops, straight runs, L corners (visible cols 10-13), battlements, barred windows and dark doorways. Tiles snap together to enclose rooms/courtyards. NOT a ground autotile - the sand ground itself uses the plain floor tileset.

**Regions:**
- cols 0-2 rows 0-4: small green-domed turrets (3-wide: cols 0-2) with a center dark doorway (1,2) flanked by oval windows (0,2 and 2,2); a slender free-standing stone post with a rounded white finial cap stands at col2 rows3-4, between this turret cluster and the taller tower `building,tower,dome`
- cols 3-5 rows 0-5: taller 2-wide green-domed tower with doorway; body continues rows3-4 with two oval windows (3,3 and 5,3) flanking a stepped doorway with ladder rails at (4,4); row5 begins a third, smaller green dome (cols4-5) that continues down into the market fragment area (cols2-5 rows6-11) `building,tower,dome`
- cols 0-1 rows 3-5: round stone well/fountain basin, continuing down into its stone pedestal/base courses at row5 `well,fountain,object`
- cols 6-19 rows 0-5: sandstone ruin wall kit: faces, corners, battlements, barred windows, dark doorways `wall,ruin,building,autotile`
- cols 6-9 rows 3-8: stepped wall with balustrade + blue banners + round tiled bath/pool `wall,bath,pool,banner`
- cols 10-13 rows 4-11: palm/date tree grove (canopies + trunks, several sizes) `palm,tree,foliage`
- cols 14-16 rows 4-11: large sandstone keep/tower: battlements, arrow-slit windows, bottom gate/portcullis `building,keep,tower,gate`
- cols 17-19 rows 7-10: oasis water pool (irregular light-blue blob) `water,oasis,pool`
- cols 0-1 rows 6-11: carved stone idols: sun-face statue (with water) and large sphinx/lion statue `statue,idol,object`
- cols 2-5 rows 6-11: brick building fragment + green tents/awnings + market stalls (red/yellow/blue cloth) + treasure chests `building,tent,market,stall,chest`
- cols 16-19 rows 10-11: desert debris: bones/skulls, pottery shards, treasure map `bones,debris,prop`
- cols 6-9 rows 9-11: green tent/awning canopy with support posts (cols 6-7 rows 9-10), a second treasure chest with a gold/yellow lid (7,11) pairing with the red-lidded chest at (6,11), a brown rubble/basket mound (8,9), and leafy green bushes/shrubbery (cols 8-9 rows 9-11) `tent,chest,bush,debris`
- cols 17-19 row 6: wall-to-ground foundation trim row directly beneath the sandstone wall kit and above the oasis pool: sloped/tapered wall base at (17,6), a small dark barred window set into the wall at (18,6), and a plain wall base with a minor step notch at (19,6) `wall,foundation,transition`

**Notable tiles:**
- (0,0) gid=1: small green-domed turret top
- (4,2) gid=45: green-domed tower doorway (dark)
- (0,3) gid=61: stone well/fountain basin
- (0,6) gid=121: carved sun-face idol statue
- (0,9) gid=181: large sphinx/lion statue
- (7,0) gid=8: sandstone wall top with battlement
- (7,1) gid=28: barred window in wall
- (6,2) gid=47: dark doorway in wall
- (16,3) gid=77: green curtain/door
- (8,7) gid=149: round tiled bath/pool (blue water)
- (11,4) gid=92: palm tree canopy
- (11,6) gid=132: palm tree trunk
- (15,4) gid=96: keep battlement top
- (15,9) gid=196: keep gate/portcullis (bottom)
- (18,8) gid=179: oasis water pool (center)
- (3,8) gid=164: green market tent/awning
- (3,10) gid=204: market stall with red cloth
- (4,10) gid=205: market stall with yellow cloth
- (6,11) gid=227: treasure chest
- (17,11) gid=238: scattered bones/skull debris

**Map-building use:** Mostly an object/building layer over plain sand ground (from the floor tileset). Assemble ruins and towers from the sandstone wall kit (cols 6-19 rows 0-5): straight faces between corner tiles, battlements on top, doorways/windows punched in. Drop the keep, green-domed turrets and statues as large multi-tile set pieces. Scatter palm trees, tents, market stalls, chests and bone debris for dressing. Paint the oasis pool and round bath as water features. Doorways and gates are natural warp/entrance points.

## TilesetDungeon  (Backgrounds/Tilesets/TilesetDungeon.png)  grid=(12, 4)

**Coverage: 25/25 occupied cells documented — ✅ COMPLETE**

**Theme:** Dungeon interior props & puzzle objects: crates, chests, storage shelves, orbs on pedestals, colored switches, gem/crystal blocks, cushions

**Palette:** warm wood browns, grey stone pedestals, vivid gem colors (blue, white, red/orange, gold), red cushion accents

**Description:** A tiny 12x4 prop sheet of dungeon-room objects rather than architecture. Row 0-1 hold containers (keyhole-locked chest, open crates, a bottle crate, a wine-rack shelf) and red cushions; rows 2-3 hold colored orbs on stone pedestals, a glowing altar, red/blue switch pedestals, and blue/white/orange/gold gem blocks, plus a stone helmet and block. There are no autotile walls, floors, or doorways here - it is purely interactive/decorative furniture keyed by the gids above.

**Structures:**
- Orb pedestals / altars: cols 2-7 rows 2-3
- Switch pedestals (red/blue buttons): cols 3-6 row 3
- Storage / containers: cols 0-5 rows 0-1
- Gem / crystal blocks: cols 0-1 rows 2-3

**Notable tiles:**
- (0,0) gid=1: locked wooden container with keyhole
- (1,0) gid=2: locked container variant
- (0,1) gid=13: round red cushion / meditation mat
- (4,1) gid=17: crate of white bottles/potions
- (5,1) gid=18: storage shelf / wine rack (dark slots)
- (2,2) gid=27: altar / pedestal with glowing white light
- (4,2) gid=29: blue orb on pedestal
- (5,2) gid=30: white orb on pedestal
- (6,2) gid=31: red orb on pedestal
- (7,2) gid=32: amber orb on pedestal
- (3,3) gid=40: RED switch pedestal
- (4,3) gid=41: BLUE switch pedestal
- (0,2) gid=25: blue gem/ice block
- (1,3) gid=38: gold gem block
- (7,3) gid=44: grey stone helmet/shell prop
- (8,3) gid=45: grey stone block/crate

**Map-building use:** Decorate dungeon rooms and puzzle chambers: place orb pedestals and red/blue switches as lever/button puzzles, gem blocks as pushable or collectible objects, crates/shelves/cushions as room filler, and the keyhole container as a locked chest. This sheet has no walls, floors or doors - pair it with a separate wall/floor tileset for room geometry.

## TilesetElement  (Backgrounds/Tilesets/TilesetElement.png)  grid=(16, 15)

**Coverage: 146/146 occupied cells documented — ✅ COMPLETE**

**Theme:** Interior furniture and mixed decorative props (indoor decoration set)

**Palette:** Warm wood browns and oranges dominate; pale beige straw, muted reds (banners/curtains), leaf greens (potted plants), teal accents, cool grays (stone/metal doors), whites, and black (piano). Transparent background.

**Description:** A dense indoor decoration set of furniture and props rendered in warm wood tones. It supplies containers (barrels, crates, chests, sacks), seating and surfaces (chairs, stools, benches, tables, dressers, bookshelves), a hearth/kitchen cluster with food, a black piano, red banners and curtains, potted plants, and 2-tall door variants (white/wood/gray). A large fringed straw/hay mat (cols 12-15 rows 4-6) doubles as a stable or barn floor patch. Despite the 'Element' name it is decorative interior furnishing, not an elemental terrain set. Also includes an undocumented jail/cage cluster (cols 1-5 row 11): a small single-cell animal cage plus a long 3-wide barred jail wall — usable for a dungeon cell, animal pen, or prison interior. Two wheeled 2x2 cargo carts (cols 0-3 rows 3-4 — one barrel-loaded, one with a covered sack) plus a wheel-less stacked-basket goods pile (cols 4-5 row 4) supply market/trade-post dressing.

**Autotile:**
- [fill texture (not a 47-blob)] straw/hay mat: cols 12-15 rows 4-6 (irregular: (12,4) is NOT part of the mat) — Large tileable straw/hay bedding mat with fringed edges. (12,5) is the top-left corner (fringe on top+left); (13,4)-(15,4) the top fringe row; (13,5)-(15,5)/(13,6)-(15,6) fill; bottom row has bottom fringe. (12,4) is a SEPARATE white glove/mitt prop, not mat. Hand-place as a rug/stable-floor patch, not a corner-match autotile.

**Regions:**
- cols 0-5 rows 0-6: Assorted small containers and props: barrels, pots, crates, chests (open/closed), sacks/bags, wood/log piles, small stools, and a few critter/statue props. `props,containers,barrels,crates,chests`
- cols 6-10 rows 0-3: Kitchen/hearth cluster: oven/furnace, cauldron pots, round bread/donut food items, round dark mirror/frame, and scattered scroll/paper props. `kitchen,oven,food,mirror`
- cols 11-14 rows 1-3: Long multi-tile furniture: horizontal benches/shelves and a legged table spanning several columns. `furniture,table,bench,multi-tile`
- cols 12-15 rows 4-6: Large straw/hay mat fill with fringed edges (stable/barn bedding, floor rug); (12,4) is a separate white glove prop. `straw,hay,floor-fill,rug`
- cols 0-5 rows 7-10: Cabinets, dressers and drawers, small wall shelves, a black stool, and green potted plants/small tree. `furniture,dresser,shelf,plants`
- cols 6-11 rows 7-8: Wooden bookshelves and dressers with colorful books and stacked items (2-tile-wide bookcases), cols 6-7 and cols 8-9; cols 10-11 rows 7-8 is a matching plain/empty bookshelf variant with no books on the shelves (same orange-top frame, bare brown shelf slats). Previously cols 10-11 were undocumented. `bookshelf,furniture,multi-tile`
- cols 6-10 rows 9-13: Red hanging banners/curtains and drapes (two 2-tile-wide drape variants at cols 7-8 and cols 9-10, row 9); white and orange window curtains below. (6,9) is a small tan storage box/parcel with a brown cross-strap band (not a curtain, but grouped in this cluster); (6,10) is a flat book/tablet prop with a blue-teal cover lying on its side. Previously (6,9), (6,10) and (10,9) were undocumented. `banner,curtain,decor,wall`
- cols 0-4 row 11: Long black piano/keyboard laid out horizontally across five tiles. `piano,furniture,multi-tile`
- cols 6-9 rows 11-13: Door variants shown as 2-tall panels: white, orange/wood, and gray/metal doors. `door,wall,multi-tile`
- col 15 rows 0-3: Sparse misc column: a small standing figure/statue, white crown/spike props, and a small colored trinket. `misc,filler`
- cols 1-5 row 11: Prison/cage furniture: (1,11) a small single-cell wooden animal cage; (2,11)-(4,11) a long 3-wide jail-bar wall segment (vertical metal bars on a wood frame); (5,11) the cage's right end-cap. Previously undocumented. `jail,cage,prison,bars,multi-tile`
- cols 0-5 rows 3-4: Wheeled cargo carts/wagons: (0,3)-(1,4) a 2x2 wooden handcart loaded with orange barrels/crates, pull-shafts on the left, spoked wheel visible at the base; (2,3)-(3,4) the same cart loaded with a covered/tarped sack (purple/white cloth bundle) instead of barrels; (4,4)-(5,4) a wheel-less stacked barrel/basket pile (goods stand without a cart). Previously missed — corrects the "no cart in the pack" gap noted in scene_recipes/village_square.json. `cart,wagon,wheeled,goods,multi-tile,basket`
- cols 6-7 row 4: Two free-standing lamp/torch props on wood posts, to the right of the barrel/basket goods pile at (4,4)-(5,4): (6,4) a tall wood post topped with a small dark round lamp head (street-lamp / hitching-post style); (7,4) a shorter forked stand holding a dark oval lamp/torch head. Previously undocumented. `lamp,torch,post,prop`
- cols 11-14 row 0: Misc single-tile props on the top row, above the long bench/table cluster (cols 11-14 rows 1-3): (11,0) a small tan/beige standing humanoid figure or doll (plainer color variant of the (15,0) statue); (12,0) a large orange rounded hood/drape shape with a dark base (cloak, poncho, or domed jar lid); (13,0) a gray zigzag hook or post-cap fixture with an orange band; (14,0) a single-tile side table topped with a red tablecloth, distinct from the long multi-tile table below. Previously undocumented. `misc,filler,prop`

**Notable tiles:**
- (15,0) gid=16: small standing figure / statue prop
- (13,4) gid=78: straw/hay mat top fringe (fill block cols 12-15 rows 4-6; (12,5) is the actual corner tile)
- (7,7) gid=120: wooden bookshelf with books (2-tile-wide bookcase, cols 7-8)
- (9,8) gid=138: bookshelf with colorful books (cols 9-10)
- (7,9) gid=152: red hanging banner/curtain segment
- (0,11) gid=177: black piano keyboard, left end (long piano cols 0-4)
- (7,11) gid=184: white door, top panel (2-tall door variant)
- (8,11) gid=185: orange/wood door, top panel
- (9,11) gid=186: gray/metal door, top panel
- (4,4) gid=69: open treasure/storage chest prop
- (1,11) gid=178: small single-cell wooden animal cage
- (2,11) gid=179: jail-bar wall segment, left (cols 2-4 = one 3-wide barred wall)
- (5,11) gid=182: jail-bar cage right end-cap
- (0,3) gid=49: wheeled cart w/ barrels, top-left (2x2 prop, cols 0-1 rows 3-4)
- (2,3) gid=51: wheeled cart w/ covered sack load, top-left (2x2 prop, cols 2-3 rows 3-4)
- (4,4) gid=69: stacked barrel/basket goods pile, no wheels (2-wide, cols 4-5 row 4)
- (6,4) gid=70: wood lamp-post prop, tall (dark round head)
- (7,4) gid=71: wood torch/lamp stand, short (dark oval head)
- (11,0) gid=12: small tan standing humanoid figure/doll prop
- (12,0) gid=13: large orange rounded hood/drape shape prop
- (13,0) gid=14: gray zigzag hook/post-cap fixture prop
- (14,0) gid=15: single-tile side table with red tablecloth
- (11,7) gid=156: plain/empty bookshelf top, no books (matches col10)
- (11,8) gid=172: plain/empty bookshelf shelves, no books (matches col10)
- (6,9) gid=137: small tan storage box/parcel with cross-strap band
- (6,10) gid=153: flat book/tablet prop, blue-teal cover
- (10,9) gid=141: red curtain drape, right block (2-wide, cols 9-10)

**Map-building use:** Furniture and clutter layer for interiors: houses, shops, inns, kitchens, libraries and stables. Place bookshelves, dressers, tables, barrels, crates and chests as collidable object tiles above a floor layer; lay the straw mat as a stable/barn floor patch; use the multi-tile piano, benches, banners and 2-tall doors as set-piece decorations. Not a terrain autotile — everything is hand-placed props. The jail-bar cluster (cols 1-5 row 11) works as a dungeon cell wall or animal pen — pair with TilesetDungeon for a full prison room. Use the wheeled carts (cols 0-3 rows 3-4) as a market stall / trade-post prop next to a house or shop — this is the pack's only cart/wagon asset.

## TilesetField  (Backgrounds/Tilesets/TilesetField.png)  grid=(5, 15)

**Coverage: 65/65 occupied cells documented — ✅ COMPLETE**

**Theme:** field/meadow ground blocks - 5 color variants, simple island autotiles

**Palette:** ['orange tilled soil', 'yellow-green grass', 'dark-green grass', 'pink/salmon ground', 'off-white snow']

**Description:** Five stacked field-ground materials (orange soil, light-green grass, dark-green grass, pink ground, snow), each a simple rounded 3x3 island with a grassy bottom fringe and two solid interior fill tiles at cols 3-4. Simpler than the 47-blob floors - meant for quickly carpeting large meadow/field regions with clean edges.

**Autotile:**
- [3x3] orange tilled soil / clay: cols 0-2 rows 0-2 — Verified orange_soil 3x3 wangset; fill tiles (3,0)(4,0).
- [3x3] light yellow-green grass (spring): cols 0-2 rows 3-5 — Verified spring_grass 3x3 wangset; fill (3,3)(4,3).
- [3x3] dark green grass (summer): cols 0-2 rows 6-8 — Verified summer_grass 3x3 wangset; fill (3,6)(4,6).
- [3x3] pink/salmon ground (autumn/cherry): cols 0-2 rows 9-11 — Verified autumn_ground 3x3 wangset; fill (3,9)(4,9).
- [3x3] off-white snow: cols 0-2 rows 12-14 — Verified field_snow 3x3 wangset; fill (3,12)(4,12).

**Regions:**
- cols 0-4 rows 0-2: orange soil island (3x3) + 2 plain fill tiles `field,soil,orange,ground`
- cols 0-4 rows 3-5: light-green grass island + fill `field,grass,spring,ground`
- cols 0-4 rows 6-8: dark-green grass island + fill `field,grass,summer,ground`
- cols 0-4 rows 9-11: pink ground island + fill `field,pink,autumn,ground`
- cols 0-4 rows 12-14: snow/white island + fill `field,snow,winter,ground`

**Notable tiles:**
- (3,0) gid=4: solid orange-soil fill tile
- (3,3) gid=19: solid light-green grass fill tile
- (3,6) gid=34: solid dark-green grass fill tile
- (3,9) gid=49: solid pink-ground fill tile
- (3,12) gid=64: solid snow fill tile
- (1,2) gid=12: orange-soil bottom fringe/skirt edge (transition down)

**Map-building use:** Large flat GROUND fills. Each 3-tile block is a rounded island (top+side edges, grassy bottom fringe) plus 2 clean interior fill tiles - use the fill tiles to carpet big field/meadow areas and the island edges to round off patch borders. Five color variants let you swap seasons/biomes without changing layout.

## TilesetFloor  (Backgrounds/Tilesets/TilesetFloor.png)  grid=(22, 26)

**Coverage: 458/458 occupied cells documented — ✅ COMPLETE**

**Theme:** multi-biome ground terrain autotiles (8 materials, all 47-blob)

**Palette:** ['orange-tan sand', 'salmon/pink sand', 'green grass', 'brown dirt', 'off-white snow', 'dark mud/soil', 'ice-blue', 'orange clay']

**Description:** The main floor/terrain sheet: eight complete 47-blob autotile materials laid out in a left column (tan sand, dirt/grass, snow, ice-blue) and a right column (pink sand, dirt/grass variant, dark mud, orange clay). Each blob is ~11 tiles wide x 5-6 tall and includes edge, outer/inner corner and fully-surrounded cases plus a few decor tiles (twigs, pebbles, flowers). Use it for every biome's base ground; the dirt block is the documented reference in autotile.json.

**Autotile:**
- [47-blob] tan/sand (light-cream fill over orange-tan base): cols 0-10 rows 0-6 — blob body rows 0-4; row 4-5 has decor (twig, pebble, sand speckle); center-surrounded tile shows a small floor-panel motif
- [47-blob] pink/salmon sand (light-pink fill over pink base): cols 11-21 rows 0-6 — exact palette-swap of the tan block, same decor tiles at cols 11-12 rows 4-5
- [47-blob] dirt-over-grass (brown dirt fill over green grass): cols 0-10 rows 7-12 — CANONICAL reference block; full mask->tile table in catalog/autotile.json. Solid dirt (1,8) gid178, plain grass (0,12) gid265, pebble decor (1,11) gid244
- [47-blob] dirt-over-grass variant (darker dirt, richer/thicker green grass border): cols 11-21 rows 7-13 — second dirt/grass blob; deeper green edges than the col 0-10 block, use for a distinct grass biome; blob has a narrow one-column protrusion into col 21 at rows 9-10 (edge fill tiles, same material)
- [47-blob] snow/off-white floor (white fill, pale-gray edge): cols 0-10 rows 14-20 — decor rows 18-20: branch, pebble, and green grass-on-snow tufts (0,20)(2,20); blob has a narrow one-column protrusion into col 10 at rows 16-17 (edge/corner fill tiles, same material)
- [47-blob] dark mud/soil (dark-brown fill over tan base): cols 11-21 rows 14-20 — decor at (11,18) branch, (13,18) pebble; blob has a narrow one-column protrusion into col 21 at rows 16-17 (edge fill tiles, same material)
- [47-blob] ice-blue floor (light-blue fill over white base, white flower/foam specks): cols 0-10 rows 21-25 — reads as frozen/ice or a blue flower field; blob has a narrow one-column protrusion into col 10 at rows 23-24 (edge fill tiles, same material)
- [47-blob] orange clay floor (orange fill over white base, white flower specks): cols 11-21 rows 21-25 — reads as autumn/clay or an orange flower field; mirrors the ice-blue block

**Regions:**
- cols 0-10 rows 0-6: tan/sand 47-blob + sand decor row `autotile,sand,desert,ground`
- cols 11-21 rows 0-6: pink sand 47-blob + decor `autotile,sand,pink,ground`
- cols 0-10 rows 7-12: dirt-over-grass 47-blob (canonical, see autotile.json) `autotile,dirt,grass,path,ground`
- cols 11-21 rows 7-13: dirt-over-grass 47-blob variant (deeper green) (protrudes one column to col 21 at rows 9-10) `autotile,dirt,grass,ground`
- cols 0-9 rows 12-13: plain grass + orange-flower & grass-tuft decor tiles `grass,flowers,decor`
- cols 0-10 rows 14-20: snow/white 47-blob + snow decor (protrudes one column to col 10 at rows 16-17) `autotile,snow,ground`
- cols 11-21 rows 14-20: dark mud/soil 47-blob + decor (protrudes one column to col 21 at rows 16-17) `autotile,mud,soil,ground`
- cols 0-10 rows 21-25: ice-blue 47-blob (flower/foam specks) (protrudes one column to col 10 at rows 23-24) `autotile,ice,blue,ground`
- cols 11-21 rows 21-25: orange clay 47-blob (flower specks) `autotile,orange,clay,ground`

**Notable tiles:**
- (1,8) gid=178: solid dirt fill tile (fully surrounded) - use to fill dirt interiors
- (0,12) gid=265: plain grass fill tile (isolated) - default grass ground
- (1,11) gid=244: dirt with pebble decor
- (0,4) gid=89: sand decor: fallen twig/branch
- (1,4) gid=90: sand decor: pale pebble/egg
- (1,13) gid=288: grass with orange flower cluster
- (13,18) gid=410: dark-mud with pebble decor
- (0,0) gid=1: isolated tan-sand patch (single-tile island)
- (11,0) gid=243: isolated pink-sand patch (single-tile island)

**Map-building use:** Primary GROUND layer sheet. Pick one of the 8 materials and paint it as a Tiled terrain (Godot match-corners 47-blob); use the canonical dirt-over-grass block (cols 0-10 rows 7-12) with catalog/autotile.json for the reference mask table, and mirror that mask->tile mapping onto the other 7 blocks which share the identical internal layout.

## TilesetFloorB  (Backgrounds/Tilesets/TilesetFloorB.png)  grid=(11, 7)

**Coverage: 48/48 occupied cells documented — ✅ COMPLETE**

**Theme:** pale cloud / fluffy soft-white floor autotile

**Palette:** ['off-white/lavender fill', 'light-blue scalloped edge']

**Description:** A small single-material autotile of a pale lavender-white surface with a light-blue scalloped (cloud-like) border. Provides a full match-corners-and-sides 47-blob plus a compact rounded island and a one-tile-wide strip for thin shapes. Best read as clouds/soft platforms; contains one hatched placeholder tile at (0,6).

**Autotile:**
- [47-blob] pale cloud / soft-white floor (lavender-white fill, light-blue bumpy cloud edge): cols 4-10 rows 0-5 — Verified cloud wangset in autotile.json (stamped from dirt_grass at origin 4,0). Also cloud_island for cols 0-2.

**Regions:**
- cols 0-2 rows 0-3: 3-wide rounded cloud island (simple blob: rows 0-2 body, row 3 bottom strip) `autotile,cloud,island`
- col 3 rows 0-3: 1-wide vertical cloud strip (top/mid/bottom) + tiny diamond dot at (3,3) `autotile,narrow,strip`
- cols 4-10 rows 0-5: full 47-blob with inner corners and corner-join cases `autotile,47-blob,cloud`
- cols 0-0 rows 5-6: Blue diagonal-hatch placeholder/collision marker tile. Correction: the actual hatch tile sits at (0,5), not (0,6) as originally logged — (0,6) is empty/transparent (no tile drawn there). `placeholder,meta`

**Notable tiles:**
- (0,0) gid=1: cloud island top-left corner
- (3,3) gid=37: single small cloud dot (smallest island tile)
- (0,6) gid=67: diagonal-hatch placeholder / not-a-tile marker (do not place) (note: verified pixel content is actually at (0,5); (0,6) itself is transparent/empty)
- (0,5) gid=56: diagonal-hatch placeholder / not-a-tile marker (do not place) — corrected location, see (0,6) note

**Map-building use:** Secondary GROUND or OVERHEAD layer for cloud platforms, fog patches, or a fluffy pale floor. Paint as a 47-blob terrain using the cols 4-10 block; the cols 0-3 island/strip tiles are handy for isolated single-width cloud bridges. Ignore the hatch tile at (0,6).

## TilesetFloorDetail  (Backgrounds/Tilesets/TilesetFloorDetail.png)  grid=(16, 5)

**Coverage: 33/33 occupied cells documented — ✅ COMPLETE**

**Theme:** single-tile ground scatter decorations (rocks, plants, props)

**Palette:** ['orange/brown props', 'green foliage', 'snow-frosted foliage', 'blue berries']

**Description:** A compact decoration sheet of 16x16 scatter objects on transparent background. Row 0 is small props (rocks, twigs, roots, pumpkin, gourd, skull, bone, acorn); row 1 has blue berries; rows 2-3 are matching green and snow-frosted plant clusters (grass, fern, mushroom, clover, bush). Purely decorative overlay content, no autotiling.

**Regions:**
- cols 0-15 row 0: misc scatter props: starburst/splat, orange gravel, sparkle, seeds, stones, twigs, forked branches, roots, tan blocks, blue ripple, pumpkin, gourd/carrot, skull, bone, acorn `decor,props,rocks,objects`
- cols 0-0 row 1: blue berries / blueberry pebble scatter `decor,berries`
- cols 0-7 row 2: green plants: grass tuft, leaves, short grass, fern sprout, red mushrooms, clover, bush `decor,plants,green,grass`
- cols 0-7 row 3: snow-frosted copies of the row-2 plants (winter variants) `decor,plants,snow,frosted`

**Notable tiles:**
- (5,0) gid=6: curved fallen twig/branch
- (11,0) gid=12: orange pumpkin/pot prop
- (12,0) gid=13: orange gourd/carrot prop
- (13,0) gid=14: white skull prop
- (14,0) gid=15: white bone prop
- (15,0) gid=16: brown acorn/nut prop
- (5,2) gid=38: red-and-white mushroom cluster
- (6,2) gid=39: clover / small plant
- (0,2) gid=33: green grass tuft (scatter over ground)
- (0,3) gid=49: snow-frosted grass tuft

**Map-building use:** DETAIL/overlay layer only. Sprinkle single 16x16 props on top of a painted ground layer to break up flat terrain - stones, twigs, mushrooms, flowers, pumpkins, plus snow-frosted plant variants for winter maps. None of these tile/connect; place them individually.

## TilesetHole  (Backgrounds/Tilesets/TilesetHole.png)  grid=(11, 5)

**Coverage: 47/47 occupied cells documented — ✅ COMPLETE**

**Theme:** pits / holes / chasms (impassable void)

**Palette:** near-black dark-teal void interior; rust/brown rim edges; grey rubble pebbles along the lip

**Description:** A compact single-material 47-blob autotile for pits and holes. A closed rectangle block supplies all straight rims and outer corners around a dark void, while additional columns give the inner-corner and diagonal-corner cases needed to carve holes of any shape. Rust-brown lips and grey rubble sell the drop; the near-black interior is the impassable fall zone.

**Autotile:**
- [47-blob] pit / hole: cols 0-10 rows 0-4 — Verified hole wangset — full 47-blob. Indexed: catalog/tilemaps/TilesetHole_indexes.png.

**Regions:**
- cols 0-3 rows 0-3: closed rectangular pit (all four rims + corners + void fill) `pit,void,edges,corners`
- cols 4-8 rows 0-4: pit inner-corner / concave-rim cases `pit,inner-corner`
- cols 9-10 rows 0-3: pit diagonal double-corner cases `pit,diagonal-corner`

**Notable tiles:**
- (0,0) gid=1: pit top-left rim corner
- (1,0) gid=2: pit top rim (straight edge)
- (1,1) gid=13: pit void interior (dark fill)
- (1,3) gid=35: pit bottom rim with rubble
- (6,2) gid=29: pit inner-corner rim peak

**Map-building use:** Collision/hazard layer. Paint a hole into a solid floor and let the autotiler wrap the rust rim around the opening (straight edges, outer corners at the ends, inner corners where it wraps). The dark interior tiles are impassable/fall tiles; use them for chasms, trap pits or gaps the player must go around or jump.

## TilesetHouse  (Backgrounds/Tilesets/TilesetHouse.png)  grid=(33, 23)

**Coverage: 648/648 occupied cells documented — ✅ COMPLETE**

**Theme:** Overworld buildings grab-bag: village houses, dojo, cabins, ovens, igloos, torii/gates, statues, fences & walls, plus interior furniture props

**Palette:** warm oranges & reds (roofs), tan/beige plaster walls, brown timber, grey stone, white ice (igloos), teal accents

**Description:** A large 33x23 mixed 'buildings + props' atlas. The top strip (rows 0-4) holds nine complete facade houses (orange, red-tile, slate, dojo-beige, iso-orange, timber) each with one ground-floor door, plus a tavern counter and two round ovens. Lower rows add torii/stone/wood gateway arches, a cave, thatch hut, three igloos, two big A-frame longhouse cabins, a giant tree-stump tower, a raised granary storehouse, a second tall timber warehouse, a round dome silo cluster, tan & grey idol statue galleries (8 columns wide, doubled in a weathered grey palette), modular fortress walls / palisade / picket / log fences, and two long market-shelf strips of barrels, baskets, jars, sacks and tables. Doors are the dark rectangular tiles at wall level; all are flagged above for warps.

**Autotile:**
- [modular-wall] tan fortress wall: cols 9-15 rows 8-9 — crenellated stone rampart: row 8 = battlement tops (with gaps), row 9 = wall body. Edge/corner pieces, not a 47-blob.
- [modular-fence] wooden palisade fence: cols 9-15 rows 4-7 — pointed-top log palisade wall segments + gap/gate pieces (rows 4-5); rows 6-7 are the lower cross-rail/gate transition pieces (short posts, T-junctions, small hinge/latch fragments) that lead down into the fortress wall at rows 8-9
- [modular-fence] teal picket fence: cols 9-13 rows 10-12 — dark blue-grey picket fence forming an enclosure; center arch GATE at (11,12) gid 408
- [modular-fence] light log fence: cols 9-13 rows 15-21 — horizontal-plank ranch fence rows 16-18 (row 15 is a lead-in/gap tile row above it); grey-stained variant at cols 9-13 rows 19-21

**Structures:**
- House A (orange gable roof): cols 0-3 rows 0-2 — doors: (1,2)
- House B (beige big roof / 'dojo' house): cols 4-7 rows 0-2 — doors: (5,2)
- House C (orange roof, dup of A): cols 8-11 rows 0-2 — doors: (9,2)
- House D (red tile roof): cols 12-15 rows 0-2 — doors: (13,2)
- House E (teal/slate roof): cols 16-18 rows 0-2 — doors: (17,2)
- Tavern / bar counter (green awning): cols 19-22 rows 0-2 — doors: (21,2)
- Round bread oven / kiln (large): cols 23-25 rows 0-3 — doors: (24,2)
- House H (orange, iso perspective): cols 26-28 rows 0-2 — doors: (27,2)
- House I (brown timber storehouse): cols 29-32 rows 0-3 — doors: (30,3)
- Torii gate (red): cols 0-2 rows 5-7
- Cave / mine entrance: cols 0-2 rows 8-9 — doors: (1,9)
- Thatch dome hut: cols 3-5 rows 8-9 — doors: (4,9)
- Igloo A: cols 0-2 rows 11-13 — doors: (1,13)
- Igloo B: cols 3-5 rows 11-13 — doors: (4,13)
- Igloo C (ice-brick): cols 6-8 rows 11-13 — doors: (7,13)
- Longhouse / A-frame cabin 1 (orange roof): cols 25-28 rows 7-13 — doors: (26,13)
- A-frame cabin 2 (brown/dark): cols 25-28 rows 14-18 — doors: (26,18)
- Small shed / house corner: cols 19-21 rows 18-22 — doors: (20,21)
- Round oven / kiln (small, right): cols 29-31 rows 4-7 — doors: (30,6)
- Wooden arch / gate (brown): cols 22-24 rows 6-8
- Stone gateway arch (grey): cols 29-31 rows 20-22
- Cellar double-door (dark arch): cols 0-2 row 4
- Fortress gate tower (corner post): cols 8-9 rows 3-9
- Raised storehouse (granary on stilts): cols 16-18 rows 3-9
- Timber storehouse B (tall warehouse): cols 22-24 rows 3-19 — doors: (22,15)
- Round dome silo cluster: cols 25-28 rows 3-6

**Regions:**
- cols 0-8 rows 15-22: Idol/statue display row, tan variant rows 15-18 with a weathered grey/mossy duplicate set directly below at rows 19-22 (same statues, cooler grey palette). Column by column: col0 stone pedestal pillar; col1 big round bear/golem-face idol (documented centerpiece, offering bowl arms); col2 a second matching big round-face idol plus a woven crate/basket prop at its foot (row18/22); col3-4 a two-tile-wide seated idol clutching a blue gem (rows15-16, documented at (3,16)) with a separate wide-mouthed frog/toad statue directly beneath it (rows17-18); col5 a round-face idol in an offering pose (rows15-16) over a hooded ninja/mask statue clutching a coiled rope or shell (rows17-18); col6 another big round-face idol (rows15-16) over a standing full-body ninja figure in dark garb (rows17-18); col7 a raccoon-dog/tanuki statue (rows15-16) over a plain wooden pedestal/barrel base (rows17-18); col8 a potted coral/bonsai ornament (rows15-16) over a snow-capped urn/barrel (rows17-18). Grey rows19-22 repeat this exact column layout in weathered stone colors. `statue,idol,decor,landmark`
- cols 14-21 rows 10-20: Market/storage shelving strip beneath the fences and fortress wall. Cols14-15: wooden racks/shelves holding cups and bowls, kegs/barrels, a cut log stump, a coiled rope, cloth sacks and potted herbs. Cols16-18 rows10-13 (below the raised storehouse, see structures): rows10-11 stacked round wicker/barrel lids, row12 open/empty barrels, row13 barrels full of produce - fish, cabbage, apples; rows14-17 a long wooden table/bench on legs; rows18-20 diagonal wood-plank flooring/wall boards. Cols19-21 rows14-17 (the gap between the giant tree-stump tower above and the small shed below): crates of fruit/vegetables, a woven basket, a strapped travel chest, ceramic jars, and a long bench with folded cloth on top. `furniture,market,storage,food,props`
- cols 29-32 rows 8-19: Right-side shrine/market clutter shelf, mirroring the cols14-21 strip. Rows8-9: a food bowl, a dark empty cauldron, a mossy round boulder, a chest with a small gourd on the lid. Rows10-12: a large tan round boulder/idol face with an open mouth (2 tiles wide, cols29-30) plus small side dishes and a dark jar. Rows13-14: a weathered mossy/green version of the same boulder-face idol (cols29-30) beside a tall pale urn with a dark mouth (cols31-32). Rows15-19: a blue-grey rounded statue, an orange flame/leaf ornament, a bench with folded cloth, small lantern-post and hut-roof shaped ornaments, a coiled rope, a bulging sack, and horizontal/vertical wood-plank bench and flooring pieces closing out the strip. `statue,market,storage,props,shrine`

**Notable tiles:**
- (1,2) gid=68: DOOR - House A
- (5,2) gid=72: DOOR - House B
- (9,2) gid=76: DOOR - House C
- (13,2) gid=80: DOOR - House D
- (17,2) gid=84: DOOR - House E
- (21,2) gid=88: DOOR/opening - tavern
- (24,2) gid=91: OVEN mouth (large kiln)
- (27,2) gid=94: DOOR - House H
- (30,3) gid=130: DOOR - House I
- (1,9) gid=299: DOOR - cave/mine mouth
- (4,9) gid=302: DOOR - thatch hut
- (1,13) gid=431: DOOR - igloo A
- (4,13) gid=434: DOOR - igloo B
- (7,13) gid=437: DOOR - igloo C
- (26,13) gid=456: DOOR - longhouse cabin 1
- (26,18) gid=621: DOOR - cabin 2
- (20,21) gid=714: DOOR - small shed
- (30,6) gid=229: OVEN mouth (small kiln)
- (11,12) gid=408: GATE - teal fence archway
- (0,3) gid=100: wooden shutter/window prop
- (1,3) gid=101: barred (jail) window
- (2,3) gid=102: closed orange plank door prop
- (4,4) gid=137: 'DOJO' red sign (spans cols 4-5, right half at (5,4))
- (6,4) gid=139: katana on weapon rack (spans cols 6-7, right half at (7,4))
- (19,9) gid=317: giant tree-stump / log tower, cols 19-21 rows 3-13 (tree-ring top rows3-4, trunk rows5-8, narrower upper trunk rows9-13 with a hanging heart-jar (21,12) and mask-jar (21,13))
- (1,16) gid=530: tan stone idol/statue (cols 0-1 rows 15-18); grey variant rows 19-22
- (3,16) gid=532: tan idol statue with blue gem
- (24,17) gid=586: wooden ladder (cols 24 rows 16-18)
- (22,20) gid=683: colored wall banners/tapestries row (cols 22-28 rows 20-21): red/green/brown/purple/yellow crests
- (14,17) gid=576: chicken
- (14,18) gid=609: firewood bundle
- (0,10) gid=331: small wooden 3-legged stool/bench prop
- (0,14) gid=463: loose prop shelf: white round pearl/snowball
- (1,14) gid=464: loose prop shelf: grey crescent/moon-stone
- (2,14) gid=465: loose prop shelf: dark red domed keg/treasure chest
- (3,3) gid=103: small ornate tan-framed window shutter prop
- (4,3) gid=104: wooden bench/stool prop
- (5,3) gid=105: small plus/cross emblem tile (latch or grave marker)
- (3,4) gid=136: red shrine offertory box (twin lidded boxes)
- (6,3) gid=106: red round oni-face shield/emblem
- (7,3) gid=107: brown chest/cabinet prop
- (25,19) gid=653: small wooden chest prop (foot of A-frame cabin 2)
- (26,19) gid=654: small wooden chest prop (foot of A-frame cabin 2)

**Map-building use:** The primary building library for towns: drop House A-I along streets, use the torii/arches and fences to enclose yards, and place igloos/caves/ovens/statues as biome landmarks. The raised storehouse, timber storehouse B, and round dome silo cluster (cols 16-28) round out a market/granary district, backed by two long market-shelf strips (cols 14-21 and cols 29-32) full of barrels, baskets and jars for dressing shop interiors. Every listed door_tile is a warp candidate into an interior map.

## TilesetLogic  (Backgrounds/Tilesets/TilesetLogic.png)  grid=(8, 10)

**Coverage: 80/80 occupied cells documented — ✅ COMPLETE**

**Theme:** Puzzle/logic markers and keyed props, color-coded across 8 columns

**Palette:** 8-color coded set, one color per column: col0 blue, col1 green, col2 pale yellow, col3 orange, col4 red, col5 purple/mauve, col6 dark gray, col7 white. Black outlines on a transparent background.

**Description:** A color-coded puzzle/logic marker set arranged as 8 color columns by 10 symbol rows. Each color (blue, green, yellow, orange, red, purple, gray, white) provides a matching solid block, key, oval lock/keyhole, and treasure chest (rows 0-3), plus '?', 'X', and the letters A-D (rows 4-9). It is meant for building keyed-door puzzles, colored switch/plate logic, and labeling map positions, so tiles are placed and wired up in gameplay code rather than forming terrain.

**Regions:**
- cols 0-7 row 0: Solid color blocks (one flat color per column) — colored floor/marker/team tiles. `color-block,marker,floor`
- cols 0-7 row 1: Keys, one per color — matched to same-color locks/chests. `key,puzzle,item`
- cols 0-7 row 2: Oval lock / keyhole plates (rounded lock face with rectangular hole), one per color. `lock,keyhole,puzzle,gate`
- cols 0-7 row 3: Treasure chests, one per color (lid + lock + slats). `chest,reward,puzzle`
- cols 0-7 row 4: Question-mark '?' markers, one per color — hint/mystery labels. `marker,hint,symbol`
- cols 0-7 row 5: 'X' markers, one per color — target/forbidden labels. `marker,target,symbol`
- cols 0-7 row 6: Letter 'A' markers, one per color. `marker,label,letter`
- cols 0-7 row 7: Letter 'B' markers, one per color. `marker,label,letter`
- cols 0-7 row 8: Letter 'C' markers, one per color. `marker,label,letter`
- cols 0-7 row 9: Letter 'D' markers, one per color. `marker,label,letter`

**Notable tiles:**
- (0,0) gid=1: blue solid color block (color-coded floor/marker)
- (4,0) gid=5: red solid color block
- (0,1) gid=9: blue key (opens blue lock/chest)
- (4,1) gid=13: red key
- (0,2) gid=17: blue oval lock / keyhole plate (locked gate marker)
- (4,2) gid=21: red oval lock / keyhole plate
- (0,3) gid=25: blue treasure chest
- (4,3) gid=29: red treasure chest
- (0,4) gid=33: blue '?' hint marker
- (0,5) gid=41: blue 'X' marker
- (0,6) gid=49: blue 'A' label marker
- (0,7) gid=57: blue 'B' label marker
- (0,8) gid=65: blue 'C' label marker
- (0,9) gid=73: blue 'D' label marker

**Map-building use:** Puzzle-design toolkit rather than terrain. Use color-coded key + lock + chest triples (same column color) to gate rooms and reward exploration; drop '?' / 'X' / 'A'-'D' symbol tiles and solid color blocks as switch/pressure-plate stand-ins, sequence steps, floor buttons, or design annotations that pair to trigger logic in code. gid = row*8 + col + 1; column index selects the color, row selects the symbol type.

## TilesetNature  (Backgrounds/Tilesets/TilesetNature.png)  grid=(24, 21)

**Coverage: 383/383 occupied cells documented — ✅ COMPLETE**

**Theme:** nature props: trees, rock/boulder formations, bushes, flowers, crystals, mushrooms, stumps, pond

**Palette:** ['green foliage', 'brown/tan bark & rock', 'gray stone', 'pink cherry blossom', 'white snow', 'orange autumn', 'blue water/crystal']

**Description:** The big nature-props sheet. The left/upper area holds tree canopies and trunks in green, bare-brown, snow-white, pink-cherry and orange-autumn variants (both 2x2 and full 3x3 forms at the bottom). The right half is dominated by large brown-then-gray boulder/rock formations meant to be placed as whole objects. Scattered between are bushes, flowers, mushrooms, colored crystals, tree stumps, a small pond and a standing wooden post - all decorative props, not autotiles.

**Autotile:**
- [edges-only] boulder/rock clusters: cols 12-23 rows 5-18 — NOT a paint-terrain: brown (upper) and gray (lower) boulder piles are pre-composed multi-tile props with baked edges/shading; place as whole objects, not autotiled

**Regions:**
- cols 0-11 rows 0-7: tree canopies + trunks: green leafy, bare brown/dead, and white snow-laden trees (2x2/2x3 each) `trees,overhead,canopy`
- cols 12-23 rows 0-4: more tree tops: white snow, pink cherry-blossom, and bright green trees `trees,overhead,cherry,snow`
- cols 0-5 rows 8-9: tree stumps, standing dead trunk/log, saplings `stump,trunk,prop`
- cols 6-11 rows 8-11: small bushes, shrubs and grass tufts `bush,shrub,detail`
- cols 0-2 rows 10-11: flowers: sunflower, clover, orange berries/blooms `flowers,detail`
- cols 0-2 rows 12-13: small pond/water + white rock/snowball `water,pond,rock`
- cols 3-11 rows 12-13: loose rocks, stones and pebble clusters `rocks,stones,detail`
- cols 0-3 rows 14-17: colored crystal/gem formations (orange, blue, purple, green) `crystal,gem,prop`
- cols 4-11 rows 14-17: mushroom clusters and berry bushes (brown/orange/pink) `mushroom,berry,detail`
- cols 12-14 rows 8-18: vertical wooden post / standing dead trunk (totem-like) `post,trunk,prop`
- cols 15-23 rows 5-13: large brown boulder / rock formations (multi-tile) `boulder,rock,collision,prop`
- cols 15-23 rows 14-19: large gray boulder / rock formations (multi-tile) + scattered rubble `boulder,rock,gray,collision`
- cols 0-11 rows 18-20: four full tree canopies: pink cherry, green, white snow, orange autumn (3x3 each) `trees,overhead,canopy,seasonal`
- cols 3-5 rows 10-11: Bush/flower transition cluster bridging the flower tiles (cols 0-2 rows 10-11) and the bush/shrub cluster (cols 6-11 rows 8-11): row 10 has three rounded yellow-green shrub/bush tiles (same style as the bushes at cols 6-11); row 11 has (3,11) an orange-red rose/poppy bloom and (4,11)-(5,11) a pair of green fern / wheat-blade plants with vertical veined leaves. Previously undocumented. `bush,shrub,flowers,fern,detail`

**Notable tiles:**
- (0,0) gid=1: green leafy tree canopy (top-left corner tile)
- (4,0) gid=5: bare/dead brown tree canopy
- (8,0) gid=9: white snow-laden tree top
- (14,0) gid=15: pink cherry-blossom tree top
- (0,8) gid=193: tree stump top (cut log)
- (0,11) gid=265: sunflower
- (1,11) gid=266: clover / shamrock
- (0,12) gid=289: small pond / water pool
- (2,14) gid=339: blue crystal formation
- (5,14) gid=342: mushroom cluster
- (17,10) gid=258: large brown boulder (center of formation)
- (16,14) gid=353: large gray boulder (center of formation)
- (13,17) gid=422: standing wooden post / dead trunk base
- (0,18) gid=433: full pink cherry tree canopy (top-left of 3x3)
- (3,18) gid=436: full green tree canopy (top-left of 3x3)
- (6,18) gid=439: full white snow tree canopy (top-left of 3x3)
- (9,18) gid=442: full orange autumn tree canopy (top-left of 3x3)
- (4,10) gid=220: rounded yellow-green bush/shrub
- (3,11) gid=244: orange-red rose/poppy bloom
- (5,11) gid=246: green fern/wheat-blade plant

**Map-building use:** OVERHEAD/object layer (and a bit of detail). Place tree canopies and boulders as whole multi-tile props above the player for depth; trunks/stumps and boulders double as collision. Use flowers, mushrooms, crystals, bushes and the pond as accent detail on top of the ground layer. This sheet does NOT autotile - stamp objects as composed groups.

## TilesetRelief  (Backgrounds/Tilesets/TilesetRelief.png)  grid=(20, 12)

**Coverage: 98/98 occupied cells documented — ✅ COMPLETE**

**Theme:** cliffs / elevation / plateaus (two-tier terrain walls)

**Palette:** green (grassy highland cliff) upper set and tan/sand (canyon/desert cliff) lower set; pale walkable plateau tops; olive-green vertical wall striations; light-blue waterfall accent

**Description:** A cliff/elevation tileset providing two full plateau-and-wall sets, one grassy-green highland and one tan desert/canyon. Each set has a pale walkable plateau top, left/right side walls, front-facing cliff faces with corners, plus inner-corner and 1-wide connector pieces. The green set includes a three-tile waterfall and the tan set a smooth ramp/slope, used to link the upper and lower terrain tiers.

**Autotile:**
- [cliff-wall-set (edges-only)] grass/highland cliff: cols 0-11 rows 0-4 — Directional plateau+wall set, NOT a flat 47-blob. cols 0-3 rows 0-2 = a small plateau (pale walkable top, left/right side walls, front cliff face with grassy foot). cols 4-6 & 8-11 = wide front-facing cliff walls (green vertical striations) with corners. col 7 rows 0-3 = a light-blue WATERFALL running down the face into a base pool. rows 3-4 hold inner-corner notches, peninsulas and 1-wide connector columns.
- [cliff-wall-set (edges-only)] sand/canyon cliff: cols 0-11 rows 5-9 — Identical plateau+wall arrangement in tan/orange for deserts and canyons. col 7 rows 5-7 is a smooth light-tan RAMP/slope band (walkable access down the cliff) in place of the green set's waterfall. rows 8-9 = inner-corner and connector pieces.

**Regions:**
- cols 0-3 rows 0-2: green plateau block: pale walkable top + side walls + front cliff face `cliff,plateau,top,walkable`
- cols 4-11 rows 0-2: green front-facing cliff walls (straight + corner variants) `cliff,wall,green`
- col 7 rows 0-3: green-cliff waterfall (top / mid / base pool) `waterfall,water,accent`
- cols 0-6 rows 3-4: green inner-corner notches, peninsulas and 1-wide connector columns `cliff,inner-corner,connector`
- cols 0-3 rows 5-7: tan/sand plateau block: walkable top + side walls + front face `cliff,plateau,desert,walkable`
- cols 4-11 rows 5-7: tan front-facing cliff walls (straight + corner variants) `cliff,wall,tan`
- col 7 rows 5-7: tan-cliff ramp/slope (walkable access band) `ramp,slope,access`
- cols 0-6 rows 8-9: tan inner-corner and connector pieces `cliff,inner-corner,connector`

**Notable tiles:**
- (0,0) gid=1: green plateau top-left corner (grass rim)
- (1,1) gid=22: green plateau flat walkable top surface
- (5,1) gid=27: green cliff front wall (face)
- (5,2) gid=46: green cliff wall base/foot
- (7,0) gid=8: green waterfall top
- (7,3) gid=68: green waterfall base pool
- (4,3) gid=65: green cliff inner-corner notch
- (0,5) gid=101: tan plateau top-left corner
- (1,6) gid=122: tan plateau flat walkable top surface
- (5,6) gid=126: tan cliff front wall (face)
- (7,5) gid=108: tan cliff ramp/slope (access band)

**Map-building use:** Elevation/collision layer. Build two-tier maps: paint the pale plateau tops as the raised walkable floor, then wrap the edges with front-facing cliff-wall tiles (corners at the ends, straight faces between) so the drop reads correctly. Place waterfalls (green col 7) or ramps (tan col 7) where the player transitions or where water spills. Cliff faces are non-walkable collision; ramps are the passable link between tiers.

## TilesetReliefDetail  (Backgrounds/Tilesets/TilesetReliefDetail.png)  grid=(12, 12)

**Coverage: 51/51 occupied cells documented — ✅ COMPLETE**

**Theme:** cliff detail / decoration overlays (caves, ladders, boulders, props)

**Palette:** green mossy rock and tan/desert rock variants; orange wooden ladders/planks; white snow-capped rock; dark cave voids; grey pebbles

**Description:** A companion decoration sheet for TilesetRelief holding cliff dressing rather than terrain. It provides two large 3x3 cave entrances (green mossy and tan desert), climbable grey and orange ladders, snow-capped and desert boulders, carved skull motifs, rope/plank ledges, bushes and pebbles. Everything is placed on an overlay layer to add depth and points of interest to otherwise plain cliff walls.

**Regions:**
- col 0 rows 0-5: scattered small details: pebbles, green bushes/moss, tiny orange rocks `decoration,pebble,bush,scatter`
- cols 1-2 rows 0-2: green-cliff detail: cave alcove/hole, carved skull motif, orange rope/plank ledge, cobblestone face `cave,skull,ledge,green`
- cols 4-5 rows 0-2: snow-capped grey boulders, snow steps/ledges, snow chunks `snow,boulder,rock`
- col 3 rows 0-5: vertical ladders: grey/wood (rows 0-2) and orange (rows 3-5) `ladder,climb`
- cols 1-2 rows 3-5: tan/desert-cliff detail: cave alcove, skull motif, rope ledge, cobblestone face `cave,skull,ledge,tan`
- cols 4-5 rows 3-5: tan/desert boulders and small rock clusters `boulder,rock,desert`
- col 0 row 6: small wooden crate/step `wood,prop`
- cols 0-2 rows 7-9: large GREEN mossy-rock cave entrance (dark mouth + wooden sign/ledge) `cave,entrance,green,object`
- cols 3-5 rows 7-9: large TAN/desert-rock cave entrance (dark mouth + wooden sign/ledge) `cave,entrance,tan,object`

**Notable tiles:**
- (1,0) gid=2: green cliff cave alcove/hole
- (2,0) gid=3: carved skull motif (green cliff)
- (3,0) gid=4: grey/wood ladder top
- (4,0) gid=5: snow-capped boulder
- (5,0) gid=6: snow steps/ledge
- (0,1) gid=13: green bush/moss clump
- (3,3) gid=40: orange ladder segment
- (1,3) gid=38: tan cliff cave alcove/hole
- (4,4) gid=53: tan/desert boulder
- (0,7) gid=85: large green cave entrance (top-left of 3x3)
- (3,7) gid=88: large tan cave entrance (top-left of 3x3)

**Map-building use:** Overlay/object layer above the TilesetRelief cliffs. Drop cave entrances against a cliff face to make a walkable dark doorway (pair with a warp/trigger). Lay ladders vertically over a cliff wall as a climb route. Scatter boulders, bushes, snow rocks and skull/rope ledge tiles to break up bare cliff faces. Purely decorative/prop tiles; not a terrain autotile.

## TilesetTowers  (Backgrounds/Tilesets/TilesetTowers.png)  grid=(24, 6)

**Coverage: 144/144 occupied cells documented — ✅ COMPLETE**

**Theme:** Natural rock spires / stone 'tower' formations and crystal outcrops - decorative landmark objects, NOT enterable buildings

**Palette:** sandstone orange & tan, weathered grey stone, moss-green overgrowth, red coral & green/gold crystal accents

**Description:** A 24x6 sheet of natural stone-spire 'towers' rather than architecture. It is organized as 2x2 units: two full rows of six craggy spire variants (orange sandstone then weathered grey), some with see-through arch openings, followed by full spires on rounded bases. The right half holds totem/face rock pillars, leafy saplings, red coral bushes and green/gold crystal outcrops. Use it purely as decorative landmark scenery - there are no enterable doors, only the arch gaps and one small window-slot tower flagged above.

**Structures:**
- Rock spire / arch tower - orange (sandstone) family: cols 0-11 rows 0-1
- Rock spire / arch tower - grey stone family: cols 0-11 rows 2-3
- Rock spire on mound (full pieces): cols 0-11 rows 4-5
- Totem / face rock pillars: cols 12-23 rows 0-1
- Saplings & coral clusters: cols 12-23 rows 2-3
- Small towers & crystal outcrops: cols 12-23 rows 4-5

**Notable tiles:**
- (0,1) gid=25: arch/window opening in orange rock spire (see-through)
- (1,1) gid=26: arch opening right half
- (0,3) gid=73: arch opening in grey stone spire
- (14,5) gid=135: small cream tower with two dark window slots (only door-like feature in sheet)
- (18,5) gid=139: green/gold crystal cluster (color variants at cols 18-23)
- (18,2) gid=91: red coral / flame bush
- (12,0) gid=13: totem/face rock pillar (tan); grey/mossy variants across the row

**Map-building use:** Scatter as terrain landmarks - rock pillars/arches to break up canyons, ruins fields or coastlines; crystal clusters and coral for magical/cave dressing; totem rocks as waypoint markers. These are obstacles/scenery, not warp buildings; the only opening tiles are the spire arches and the (14,5) window tower, none of which are functional doors.

## TilesetVillageAbandoned  (Backgrounds/Tilesets/TilesetVillageAbandoned.png)  grid=(20, 12)

**Coverage: 234/234 occupied cells documented — ✅ COMPLETE**

**Theme:** Abandoned/overgrown village: mossy ruined houses, big weathered timber buildings, graves, dead stumps, heavy foliage. (Tileset used by the demo village map.)

**Palette:** moss green over sun-bleached tan stone, weathered brown timber, faded orange accents, black cavity doorways

**Description:** A compact 20x12 abandoned-village set. It contains two small ruins (stone and orange) with dark doorways, a multi-storey wooden ruin with two stacked doorways, and two large moss-covered A-frame timber houses (one with a clear ground door, one a buried facade). Supporting pieces are a cross grave marker, a stilt hut, a wooden watch-platform with ladder, dead/hollow stumps, mossy pillars, a book-filled shelf, and lots of overgrowth foliage. Doorways are the solid-black tiles at wall level, all flagged for warps. The left side (cols 0-5, rows 3-11) is a dense natural cluster of mossy rock/bench props, stone pillars, boulders, broken stumps, leaf bushes, two large tri-lobed trees and two round trees (each with loose root/branch debris beneath), and the graveyard fence (cols 6-10 rows 0-2) has three wooden cross markers. A decorative moss/vine trunk column fills the gap between the two big timber houses at col 11.

**Autotile:**
- [decorative-clusters] moss/foliage overgrowth: scattered cols 6-10 rows 3-11 — green bush/shrub clusters used to bury ruins; hand-placed blobs, not a 47-tile autotile

**Structures:**
- Ruined stone house (mossy): cols 0-3 rows 0-2 — doors: (1,2)
- Ruined orange house: cols 4-5 rows 0-2 — doors: (5,2)
- Graveyard marker: cols 6-10 rows 0-2
- Ruined 2-storey wooden house: cols 11-13 rows 0-5 — doors: (12,2), (12,5)
- Wooden watch-platform / bridge: cols 14-16 rows 0-4
- Raised stilt hut: cols 17-19 rows 0-2
- Big timber house (A-frame, mossy roof): cols 12-15 rows 6-10 — doors: (13,10), (14,10)
- Big timber house (overgrown ruin, right): cols 16-19 rows 3-11

**Regions:**
- cols 0-1 rows 3-5: Large moss-covered rock formation with a weathered wood plank bench/seat embedded in the stone: rounded lichen-capped boulder top (row 3) over a flat tan bench slab (row 4) on a mossy rocky base/legs (row 5). ``
- cols 2 rows 3-5: Tall mossy stone pillar/column, full height (cap row 3 through base row 5); cap tile also flagged as notable (2,3) gid 63. ``
- cols 3 rows 3-5: Medium free-standing mossy boulder/rock, rounded lichen-covered stone, stacked rows 3-5. ``
- cols 4 rows 3-4: Moss-covered broken stump with rotten orange wood exposed through cracks in the moss cap. ``
- cols 4-5 row 5: Small round leafy bush clusters (single-tile shrub variants), one per column. ``
- cols 0-3 rows 6-8: Large tri-lobed tree canopy (wide bushy top split into 3 rounded lobes spanning cols 0-3) with an orange-brown root/trunk base peeking out at the bottom center (row 8). ``
- cols 0-3 rows 9-11: Second large tri-lobed tree canopy, identical silhouette to the cols 0-3 rows 6-8 tree, but with a yellow-olive root/trunk base color variant (row 11). ``
- cols 4-5 rows 6-7: Round single-lobe tree canopy (bushy circular top) cols 4-5 rows 6-7, orange roots peeking out at its base. ``
- cols 4-5 row 8: Loose orange root/branch debris chunks, unattached decorative rubble sitting below the cols 4-5 rows 6-7 tree canopy. ``
- cols 4-5 rows 9-10: Second round single-lobe tree canopy, same silhouette as cols 4-5 rows 6-7 but with yellow-olive roots at its base. ``
- cols 4-5 row 11: Loose yellow-olive root/branch debris chunks, unattached rubble below the cols 4-5 rows 9-10 tree canopy. ``
- cols 11 rows 6-11: Vertical decorative moss/vine-covered trunk column filling the gap between the two big timber houses: mossy tree-trunk strip rows 6-8, gap row 9, small mossy boulder cluster row 10, round orange coiled-rope/shield decorative tile row 11. ``
- cols 14-15 row 5: Gap row between the wooden watch-platform ladder and the big A-frame house roof: mossy roof overhang tile (14,5) and a final descending ladder rung continuing the (15,2)-(15,4) ladder down to (15,5). ``
- cols 17-19 rows 3-4: Upper mossy overgrown roof/foliage of the big timber house ruin (tan wood peeking through moss), directly above its cols 16-19 rows 5-11 body and below the raised stilt hut at cols 17-19 rows 0-2. ``

**Notable tiles:**
- (1,2) gid=42: DOOR - ruined stone house
- (5,2) gid=46: DOOR - ruined orange house
- (12,2) gid=53: DOOR - wooden ruin (upper)
- (12,5) gid=113: DOOR - wooden ruin (ground)
- (13,10) gid=214: DOOR - big timber house
- (9,0) gid=10: grave CROSS marker
- (12,1) gid=33: barred window
- (5,0) gid=6: dead/hollow orange tree stump (also hollow stump with hole spanning cols 5 rows 3-4, gid 86)
- (2,3) gid=63: tall mossy stone pillar/column
- (18,11) gid=239: bookshelf with colored books (cols 18-19 row 11)
- (12,11) gid=233: small critters row (frog/salamander/slime) cols 12-15 row 11 - decorative

**Map-building use:** Build a deserted/haunted village: the ruined stone & orange houses and the two big overgrown timber houses are the main structures; scatter the cross grave, dead stumps, mossy pillars and bush clusters to overgrow the streets. This is the sheet the demo village map draws from, so keep its gids stable.

## TilesetWater  (Backgrounds/Tilesets/TilesetWater.png)  grid=(28, 17)

**Coverage: 258/258 occupied cells documented — ✅ COMPLETE**

**Theme:** water / ponds / rivers / docks (multi-biome water edges)

**Palette:** cyan water body with white foam rims; edge terrains in sand-orange, grass-green, dirt-brown; ice variant in pale white/light-blue with diagonal sparkle; magic variant in mauve/purple; wood dock in warm orange

**Description:** A multi-biome water tileset: four complete 47-blob water autotiles (sand-edged, grass-edged, ice, and magic/mauve) plus open deep-water surface tiles with lily pads and fish. It also carries a wooden dock/bridge plank field and decorative props (boat, bucket, rocks, koi). Each water material tiles seamlessly against its own edge terrain, so ponds, rivers and coastlines are built by painting the body and autotiling the rim.

**Autotile:**
- [47-blob] sand/beach-edged water: cols 0-9 rows 0-4 — Verified sand_water wangset (origin 0,0), 47/47 masks.
- [47-blob] grass-edged water: cols 0-10 rows 6-10 — Verified grass_water wangset (origin 0,6), 47/47 masks.
- [47-blob] ice / frozen water: cols 13-21 rows 0-4 — Verified ice_water wangset (origin 13,0), 47/47 masks.
- [47-blob] magic / mauve water (dirt-edged): cols 13-21 rows 6-11 — Verified magic_water wangset (origin 13,6), 47/47 masks.

**Regions:**
- cols 0-9 rows 0-4: sand/beach-edged water autotile blob `water,autotile,beach,sand`
- col 11 rows 0-4: open deep-water surface strip (sparkle at row1, lily pad row3, fish ripple row4) `water,deep,lilypad,decoration`
- col 12 row 0: loose rock/boulder sitting in water `rock,object`
- cols 13-22 rows 0-4: ice/frozen water autotile blob + vertical ice-fall strip (col22) `water,ice,autotile,waterfall`
- cols 23-27 rows 0-1: decorative props: rock (23,0), koi fish (24,0), wooden raft/boat (25-27,0), bucket (25,1) `object,boat,fish,prop`
- cols 0-10 rows 6-10: grass-edged water autotile blob `water,autotile,grass,pond`
- cols 13-21 rows 6-11: magic/mauve dirt-edged water autotile blob `water,autotile,magic,purple`
- cols 0-10 rows 12-16: wooden dock/bridge plank field (horizontal boards); dock props (barrel/jug/crate) at row 16 cols 0-3 `wood,dock,bridge,walkable`
- col 0 row 5: isolated solid sand-colored ground-fill swatch (flat, unpatterned), sitting alone directly below the sand-edged water blob; matches the blob's sand rim color, likely a flood-fill/reference tile `sand,swatch,fill`
- col 0 row 11: isolated solid grass-green ground-fill swatch, sitting alone directly below the grass-edged water blob; matches the blob's grass rim color, likely a flood-fill/reference tile `grass,swatch,fill`
- col 10 rows 2-3: extra sand-edged water blob corner/peninsula variant tiles spilling one column past the main cols 0-9 block (same brown-rimmed circular pattern as the blob) `water,autotile,sand`
- col 13 row 5: isolated solid white/pale-ice ground-fill swatch, sitting alone directly below the ice/frozen water blob; matches the blob's pale rim color, likely a flood-fill/reference tile `ice,swatch,fill`
- cols 22-23 rows 6-9: extra magic/mauve water blob corner variant tiles spilling past the main col21 edge: col22 continues the lattice pattern rows 6-9 (analogous to the ice blob's col22 waterfall-strip column), while col23 rows 8-9 add a ring-corner tile plus a solid mauve center-fill swatch `water,autotile,magic`
- col 23 rows 2-3: extra ice/frozen water blob corner variant tiles spilling one column past the main cols 13-22 block (same pale foam-ring pattern as the blob) `water,autotile,ice`
- col 24 rows 2-4: a second open deep-water surface strip mirroring col 11 (diagonal sparkle at row2, lily pad at row3, fish ripple at row4), positioned between the ice blob and the raft/prop cluster `water,deep,lilypad,decoration`

**Notable tiles:**
- (0,0) gid=1: sand-water isolated rounded pond corner (top-left)
- (7,1) gid=36: sand-water deep/dotted fill (fully-surrounded center)
- (11,0) gid=12: open deep-water surface tile
- (11,3) gid=96: lily pad on open water
- (11,4) gid=125: fish ripple on open water
- (12,0) gid=13: loose rock/boulder in water
- (17,1) gid=46: ice-water blob center fill
- (22,1) gid=51: vertical ice-fall / waterfall strip
- (24,0) gid=25: koi fish decoration
- (25,0) gid=26: wooden raft/boat (spans cols 25-27)
- (25,1) gid=54: wooden bucket prop
- (7,7) gid=204: grass-water blob center fill
- (18,7) gid=215: magic/mauve water blob center fill
- (2,13) gid=367: wooden dock/bridge plank (interior board)
- (1,16) gid=450: dock prop (barrel/jug/crate cluster)

**Map-building use:** Ground/water layer. Paint the water body then let the terrain autotiler pick edge/corner tiles from the matching material blob (sand, grass, ice, or magic). Use the open-water strip (col 11) tiles as scattered surface variety plus lily pads/fish. Lay the wooden dock planks (rows 12-16) on an object/overlay layer for walkable bridges and piers across the water. Boats, buckets and rocks are single-tile object-layer props.

## Beds & Bedroom Furniture  (Backgrounds/Tilesets/tileset_bed.png)  grid=(14, 12)

**Coverage: 126/126 occupied cells documented — ✅ COMPLETE**

**Theme:** bedroom furniture — beds in several colors, mattresses, wooden storage & railings, stone floor

**Palette:** wood frames with tan / green / red / blue blankets, white gold-quilted mattress, grey stone

**Description:** A bedroom furniture sheet. Rows 0-5 hold rows of beds — tan and green across the top band, red and blue across the middle band — each a 2x3 wood-framed object with a pillow, blanket, and footboard. Rows 6-8 add bare white gold-quilted mattresses on the left and wooden storage (crate/chest, drawer cabinet) plus fence/railing pieces on the right. The bottom-left block (cols 0-6 rows 9-11) is a grey stone slab floor that autotiles.

**Autotile:**
- [3x3] grey stone slab floor: cols 0-6 rows 9-11 — Verified stone_slab 3x3 wangset at cols 0-2 rows 9-11; fill variants cols 3-5.

**Furniture:**
- tan bed (single, wood frame + white pillow): cols 0-1 rows 0-2
- tan beds (pair / double): cols 2-5 rows 0-2
- green bed (single): cols 7-8 rows 0-2
- green bed (single): cols 10-11 rows 0-2
- green bed (narrow / against wall): cols 12-12 rows 0-1
- red bed (single): cols 0-1 rows 3-5
- red beds (pair / double): cols 2-5 rows 3-5
- blue bed (single): cols 7-8 rows 3-5
- blue bed (single): cols 10-11 rows 3-5
- blue bed (narrow / against wall): cols 12-12 rows 3-5
- white gold-quilted mattress (single, no frame): cols 0-1 rows 6-8
- white gold-quilted mattress (large): cols 2-5 rows 6-8
- wooden crate / chest (vertical planks): cols 7-8 rows 6-8
- wooden drawer cabinet / shelf: cols 10-11 rows 6-8
- wooden fence / railing / gate: cols 12-13 rows 6-8

**Regions:**
- cols 0-13 rows 0-2: top band of beds: tan blankets on left, green blankets on right `beds,furniture`
- cols 0-13 rows 3-5: middle band of beds: red blankets on left, blue blankets on right `beds,furniture`
- cols 0-6 rows 6-8: white gold-quilted bare mattresses (single and large) `mattress,quilt,furniture`
- cols 7-13 rows 6-8: wooden storage & railings: crate/chest, drawer cabinet, fence/gate rails `wood,shelf,fence,furniture`
- cols 0-6 rows 9-11: grey stone slab floor (autotiles) `floor,stone,autotile`

**Notable tiles:**
- (0,0) gid=1: tan bed headboard + pillow (top-left of first bed)
- (0,3) gid=43: red bed headboard + pillow
- (7,0) gid=106: green bed headboard + pillow
- (7,3) gid=148: blue bed headboard + pillow
- (0,9) gid=267: grey stone slab floor corner (autotile start)

**Map-building use:** Furnish bedrooms and inns: each bed is a 2-wide x 3-tall object (headboard/pillow row, blanket row, footboard row) — pick a blanket color per NPC. Line up beds in a row for a dormitory, add wooden crates/drawer cabinets against walls and wooden railings to partition space, and lay the grey stone floor beneath as an autotiling surface.

## tileset_camp  (Backgrounds/Tilesets/tileset_camp.png)  grid=(23, 9)

**Coverage: 193/193 occupied cells documented — ✅ COMPLETE**

**Theme:** Campsite / war-camp: canvas tents, campfires & stone fire rings, logs, barrels, crates, cooking gear, hay, rope, training dummy

**Palette:** tan canvas, warm brown timber & logs, orange firelight, grey stone rings, straw yellow, red flame

**Description:** A 23x9 camp set. The left/top holds three identical 3x3 peaked canvas tents, each with a dark flap entrance, plus a large 9x9 pavilion/command tent whose walkable interior opens at a bottom-center arch (18,8). The rest is camp dressing: campfires with cooking spits, grey stone fire rings, log bundles and stumps, barrels, crates and storage chests, folded canvas/tarps, cooking pots and pans, hay bundles, coiled rope, benches and a training-dummy post. All four tent entrances are flagged above for warps. Cols 0-3 rows 1-8 are a dense lumber/woodpile corner (standing posts, cut logs, stumps, crates, hay); cols 4-9 rows 3-8 mix food supplies, cooking gear, a signpost, the training dummy, a torn tarp scrap and a storage cluster of barrels/sacks; cols 10-13 rows 7-8 add a coiled rope, a flask, a candle and dirt clumps beside the pavilion, with loose kindling debris at cols 13 rows 0-2.

**Autotile:**
- [modular-cloth] canvas / cloth panels: cols 0-3 rows 3-4 — folded tent-canvas & tarp pieces used to extend tent walls; hand-tiled, not a 47-blob

**Structures:**
- Small tent 1 (peaked canvas): cols 4-6 rows 0-2 — doors: (5,2)
- Small tent 2: cols 7-9 rows 0-2 — doors: (8,2)
- Small tent 3: cols 10-12 rows 0-2 — doors: (11,2)
- Large pavilion / command tent: cols 14-22 rows 0-8 — doors: (18,8)
- Campfire with spit: cols 8-9 rows 3-4
- Stone fire ring / pit: cols 10-13 rows 3-6

**Regions:**
- cols 0-3 rows 1-2: Lumber/log pile variants below the row-0 log-bundle: standing wood post (0,1)-(0,2); fallen cut log with a round tree-ring end cap lying horizontal (1,2)-(2,2); tall 2-tile stacked log bundle variant (3,1)-(3,2). ``
- cols 0 rows 5-6: Weathered wooden fence post / support beam, upright, 2 tiles tall. ``
- cols 1-2 rows 5-6: Large cut tree-stump slice (top-down view, concentric rings), drawn as one big 2x2 graphic across cols 1-2 rows 5-6; top-left cell is also flagged as notable tile (1,5) gid 116. ``
- cols 3 rows 5-6: Two small tree-stump / cut-log cross-section icons stacked in col 3 (variant caps, smaller than the big stump at cols 1-2). ``
- cols 3 row 7: Short bench/plank end-piece fragment, a separate shorter companion to the long bench at cols 0-2 row 7. ``
- cols 3 row 8: Small isolated straw/hay tuft, a separate smaller companion to the hay bundle at cols 0-2 row 8. ``
- cols 4 rows 3-4: Camp food supplies: round bread loaf (4,3) and a cured meat/ham chunk (4,4). ``
- cols 4 row 5: Tall narrow wooden crate/post variant with vertical slats, standing alone above the big storage crate. ``
- cols 4-5 rows 6-8: Tall wooden storage crate/basket, slatted lid at row 6 down through the crate body to its base at row 8; same crate as notable tile (4,7) gid 162 (cols 4-5). ``
- cols 5 row 5: Wooden mallet/stamp tool, short handle with a block head. ``
- cols 6 rows 5-6: Signpost/pole with a grey tool (axe or hook) hanging from it; post continues down to row 6. Same post as notable tile (6,5) gid 167. ``
- cols 7 rows 5-6: Training dummy/scarecrow on a wooden post; post base continues down to row 6. Same dummy as notable tile (7,5) gid 168. ``
- cols 8-9 rows 5-6: Large torn canvas/tarp scrap, tan cloth with pale worn/torn patches, drawn as one 2x2 piece across cols 8-9 rows 5-6. ``
- cols 6-9 rows 7-8: Camp storage cluster: wooden barrel/cask viewed from above (6,7)-(6,8); bound wood plank/crate lid (7,7); angled cut log chunk (7,8); round stump-slice/wooden shield (8,7); small dirt/mud clump (8,8); short wooden keg (9,7); tan grain/flour sack (9,8). ``
- cols 10-11 rows 7-8: Coiled rope/lasso loop (same rope as notable tile (10,7) gid 240) extending right into col 11, plus a second tan coiled-rope ring at (11,8); faint rope-tip overflow at (10,8). ``
- cols 12 row 7: Grey glass flask/potion bottle. ``
- cols 12 row 8: Small dirt/mud clump. ``
- cols 13 rows 0-2: Broken wood plank/kindling debris fragments (13,0)-(13,1) and a woven wicker basket-weave texture fragment (13,2), scattered beside the pavilion's left outer wall. ``
- cols 13 row 7: Candle standing in a small metal holder/lantern base. ``
- cols 13 row 8: Small dirt/mud clump variant, beside the pavilion's left outer wall. ``

**Notable tiles:**
- (5,2) gid=52: TENT ENTRANCE - tent 1
- (8,2) gid=55: TENT ENTRANCE - tent 2
- (11,2) gid=58: TENT ENTRANCE - tent 3
- (18,8) gid=203: TENT ENTRANCE - large pavilion (bottom center)
- (8,3) gid=78: campfire flames (with (9,3) gid 79)
- (10,3) gid=80: stone fire ring / pit
- (0,0) gid=1: stacked log bundle (lumber) - variants cols 0-3 row 0
- (1,5) gid=116: cut tree stump (top view, rings)
- (4,7) gid=162: wooden barrels & storage crates (cols 4-5 rows 7-8)
- (7,5) gid=168: training dummy / scarecrow on post
- (6,5) gid=167: signpost / pole with hook
- (10,7) gid=240: coiled rope / lasso
- (0,7) gid=162: long wooden bench/plank (cols 0-2 row 7)
- (0,8) gid=185: straw/hay bundles (cols 0-2 row 8)
- (6,3) gid=76: green cauldron/pot & pans (cooking gear cols 5-7 rows 3-4)

**Map-building use:** Assemble a military or hunter camp: place the three small tents as sleeping quarters (each flap is a warp/interaction tile) and the large pavilion as the command tent (enter at bottom-center (18,8)); ring the clearing with campfires/stone fire pits, barrels, crates, log piles, hay, and a training dummy. Tent entrances are the flagged dark tiles.
