#!/usr/bin/env node
// Records examples/<name>/preview.gif — the committed showcase clip, the moving counterpart to the
// hand-picked preview.png. Captures from the ROM ALREADY ON DISK (it does not build), so this is
// minutes rather than the hours a full cargo rebuild of every example would take; build an example
// yourself first if its ROM is stale or missing.
//
// Each clip inherits that example's own `gif` npm script — its ROM name, frame count and key
// schedule — because those were tuned so the capture lands on something worth looking at.
//
// Captured at native 240x160 — one GBA pixel per pixel, the smallest the file can possibly be —
// and capped to PREVIEW_FRAMES. How BIG a clip appears on a page is a display decision, so it is
// made once in the markup (see PREVIEW_WIDTH in scripts/gen_examples_readme.py) rather than baked
// into every committed binary; recording at 2x doubled the repo cost for something an attribute
// does for free.
//
// Usage: node scripts/gen_previews.js [--force] [example ...]
//   --force  re-record even when preview.gif is newer than the ROM (default: skip those)
'use strict';
const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const PREVIEW_SCALE = '1';
const PREVIEW_FRAMES = '60';    // recorded frames
const PREVIEW_EVERY = '3';      // one frame in 3 => ~20fps playback, so ~3s of motion
const MIN_FROM = 60;            // never open on the boot frames — see the window comment below

const root = path.resolve(__dirname, '..');
const dir = path.join(root, 'examples');

const argv = process.argv.slice(2);
const force = argv.includes('--force');
const only = argv.filter((a) => !a.startsWith('--'));

// `gif` looks like: npm run build && ../../scripts/gif.sh <rom> screenshot.gif [frames] [keys]
const GIF_RE = /scripts\/gif\.sh\s+(\S+|"[^"]*")\s+screenshot\.gif\s*(\d+)?\s*(.*)$/;

const examples = fs
  .readdirSync(dir, { withFileTypes: true })
  .filter((e) => e.isDirectory() && fs.existsSync(path.join(dir, e.name, 'package.json')))
  .map((e) => e.name)
  .filter((n) => !only.length || only.includes(n))
  .sort();

// Stored frame count. Pillow dedupes frames identical to their predecessor, so 1 means the picture
// never changed across the whole window. Returns 2 ("keep it") if Pillow is unavailable.
function frameCount(gif) {
  const res = spawnSync('python3', ['-c',
    'import sys;from PIL import Image;print(Image.open(sys.argv[1]).n_frames)', gif],
    { encoding: 'utf8' });
  const n = Number((res.stdout || '').trim());
  return Number.isFinite(n) && n > 0 ? n : 2;
}

const done = [], skipped = [], failed = [], still = [];
for (const name of examples) {
  const exDir = path.join(dir, name);
  const pkg = JSON.parse(fs.readFileSync(path.join(exDir, 'package.json'), 'utf8'));
  const gif = pkg.scripts && pkg.scripts.gif;
  if (!gif) continue;
  const m = GIF_RE.exec(gif);
  if (!m) { failed.push(`${name}: unparseable gif script`); continue; }

  // The ROM name is normally "$npm_package_name.gba"; a couple of examples hardcode something else.
  // Expand it from pkg.name, NOT the directory name — several examples (sunnyside*, bench-entities)
  // are published as tish-agb-<dir> and build a ROM under that name.
  const romArg = m[1].replace(/"/g, '').replace('$npm_package_name', pkg.name || name);
  const rom = path.join(exDir, romArg);
  if (!fs.existsSync(rom)) { skipped.push(`${name} (no ROM — build it first)`); continue; }

  const out = path.join(exDir, 'preview.gif');
  if (!force && fs.existsSync(out) && fs.statSync(out).mtimeMs >= fs.statSync(rom).mtimeMs) {
    skipped.push(`${name} (up to date)`);
    continue;
  }

  const frames = Number(m[2] || 300);
  const keys = (m[3] || '').trim().replace(/^"|"$/g, '');
  // Record the window ENDING on the example's tuned shot frame, rather than a fixed offset from
  // boot. That frame is the moment its author picked as representative — and for the many examples
  // that idle on a title screen until a scheduled keypress lands, a boot-anchored window records
  // nothing but a still title card.
  //
  // Floored at MIN_FROM, never 0: the opening frames are boot, and an example tuned to a shot at
  // frame 120 or 180 would otherwise have its whole window start at power-on. When that floor
  // leaves too little room, run PAST the tuned frame instead of shortening the clip — a short
  // capture is what made the previous pass emit one-frame previews.
  const span = Number(PREVIEW_FRAMES) * Number(PREVIEW_EVERY);
  const from = Math.max(MIN_FROM, frames - span);
  const run = Math.max(frames, from + span);
  const res = spawnSync(path.join(root, 'scripts', 'gif.sh'), [rom, out, String(run), keys], {
    env: {
      ...process.env,
      GIF_SCALE: PREVIEW_SCALE,
      GIF_MAX_FRAMES: PREVIEW_FRAMES,
      GIF_EVERY: PREVIEW_EVERY,
      GIF_FROM: String(from),
    },
    stdio: ['ignore', 'ignore', 'pipe'],
    encoding: 'utf8',
  });
  // gif.sh exits 3 for "nothing to record" — a screen that never has a picture on it during the
  // window. That is the same outcome as a one-frame clip below, not a tooling failure.
  if (res.status === 3) { still.push(name); continue; }
  if (res.status !== 0) {
    failed.push(`${name}: ${(res.stderr || '').trim().split('\n').pop()}`);
    continue;
  }
  // A one-frame "animation" is just a worse preview.png. Examples that idle at a title screen
  // until a player presses something have nothing to record unless their shot line carries a key
  // schedule, so drop the file and name them at the end rather than committing 33 still GIFs.
  if (frameCount(out) <= 1) {
    fs.unlinkSync(out);
    still.push(name);
    continue;
  }
  const kb = Math.round(fs.statSync(out).size / 1024);
  console.log(`${name}: preview.gif (${kb}kb)`);
  done.push(name);
}

console.log(`\nrecorded ${done.length}, static ${still.length}, skipped ${skipped.length}, failed ${failed.length}`);
if (still.length) {
  console.log(`nothing moved (no preview.gif written — give the example's shot/gif line a key`
    + ` schedule if it needs input to do anything): ${still.join(', ')}`);
}
if (skipped.length) console.log(`skipped: ${skipped.join(', ')}`);
if (failed.length) { console.log(`failed:\n  ${failed.join('\n  ')}`); process.exitCode = 1; }
