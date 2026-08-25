# Isometric block pack (vendored)

**Big Pixel Isometric Block Pack** by **Ajay Karat / Devil's Work.shop** —
<https://devilsworkshop.itch.io/big-pixel-isometric-block-pack-free-2d-sprites>. Free for commercial
use, modify/redistribute, no permission required (credit appreciated, not required — see
[`License.txt`](License.txt)). Vendored here for the isometric examples.

Only the two **16×16 pixel** variants are vendored (GBA-native size); the pack's 50×50 / 36×50 /
1024×1024 variants are omitted. The 231 blocks of each variant are packed into a single 16-wide grid
atlas (no loose files) — index `i` is at grid cell `(i % 16, i // 16)`:

| File | Grid | Contents |
|---|---|---|
| [`blocks_iso_16.png`](blocks_iso_16.png) | 16×15 (256×240) | the **angled isometric cubes** (diamond top + two side faces) — used as depth-sorted sprites in the true-iso example |
| [`blocks_flat_16.png`](blocks_flat_16.png) | 16×15 (256×240) | the **flat top-down faces** of the same blocks — a top-down tilemap variant |

Handy indices (both atlases share the same ordering): `0` grass · `64` water/ice · `44`–`57` stone/
cobble · `72`–`78` dirt/wood · `90` sand · `80` red · `208`–`212` dark stone. Re-pack from the source
pack with the packer in the example that consumes it.
