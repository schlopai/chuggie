#!/usr/bin/env node
// Makes sure every example has a committed preview: examples/<name>/preview.gif where there is
// motion to show, and examples/<name>/preview.png where there is not. Captures from the ROM ALREADY
// ON DISK (it does not build), so this is minutes rather than the hours a full cargo rebuild of
// every example would take; build an example yourself first if its ROM is stale or missing.
//
// Each capture inherits that example's own `gif`/`shot` npm script — its ROM name, frame count and
// key schedule — because those were tuned so the capture lands on something worth looking at. The
// diagnostic examples (repro-*, bench-*) have no such script; they get DEFAULT_FRAMES.
//
// A still is the fallback, not a consolation prize: a readout that prints a number, or a demo that
// idles until a player presses something, has nothing to animate, and a one-frame "animation" is
// strictly worse than a PNG. Those examples were the 27 blank rows on the index.
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
const DEFAULT_FRAMES = 240;     // for examples with no shot/gif script of their own to inherit
const AUTOPLAY_FRAMES = 720;    // long enough to get through a title/intro and then play a while

// Examples whose art cannot be published. bench-behav and bench-boot were built against ripped
// Zelda sprites/tilesets, so a committed preview of either is redistribution of Nintendo's art —
// they get no image until their assets are replaced (or the examples go). Recording is skipped
// rather than the file being deleted afterwards, so a re-run cannot quietly put it back.
const NO_PREVIEW = new Set(['bench-behav', 'bench-boot']);

// Per-example capture-window overrides, for the few where the generic window produces something
// actively bad rather than merely dull.
//
// sunnyside-day's clock runs ~96s per in-game day, so its light should change slowly. It came out
// strobing — luminance sweeping 124 -> 8 -> 153 twice in three seconds — because AUTOPLAY was
// mashing START into it and skipping the clock forward. It needs no input at all: it animates on
// its own. `drive: false` opts an example out of autoplay, which is the fix whenever driving it
// does something worse than leaving it alone.
const WINDOW = {
  'sunnyside-day': { drive: false, every: 8 },
};

const root = path.resolve(__dirname, '..');
const dir = path.join(root, 'examples');

const argv = process.argv.slice(2);
const force = argv.includes('--force');
const only = argv.filter((a) => !a.startsWith('--'));

// `gif`/`shot` look like: npm run build && ../../scripts/<gif|screenshot>.sh <rom> out [frames] [keys]
const CAP_RE = /scripts\/(?:gif|screenshot)\.sh\s+(\S+|"[^"]*")\s+screenshot\.(?:gif|png)\s*(\d+)?\s*(.*)$/;

// The ROM an example builds. Normally "<pkg.name>.gba" from its own capture script; failing that,
// the single .gba sitting in the directory.
function findRom(exDir, pkg, m) {
  if (m) {
    const named = path.join(exDir, m[1].replace(/"/g, '').replace('$npm_package_name', pkg.name));
    if (fs.existsSync(named)) return named;
  }
  const byName = path.join(exDir, `${pkg.name}.gba`);
  if (fs.existsSync(byName)) return byName;
  const roms = fs.readdirSync(exDir).filter((f) => f.endsWith('.gba'));
  return roms.length === 1 ? path.join(exDir, roms[0]) : null;
}

const examples = fs
  .readdirSync(dir, { withFileTypes: true })
  .filter((e) => e.isDirectory() && fs.existsSync(path.join(dir, e.name, 'package.json')))
  .map((e) => e.name)
  .filter((n) => !only.length || only.includes(n))
  .sort();

// A generic "someone is holding the pad" schedule, for examples whose own capture line sends no
// input. Most examples open on a title screen and sit there forever — a shot tuned to frame 180 is
// a picture of a menu, and a CLIP of a menu is 3 seconds of nothing. So: mash start/A to get
// through the title and any intro dialogue, then walk the four directions with periodic A presses.
//
// It is deliberately dumb. It cannot know an example's controls, and it will wander into menus in
// some of them — but a wrong-looking frame of the actual game beats a correct-looking frame of the
// title card, and any example that deserves better can say so in its own shot/gif line, which
// always wins over this.
function autoplay(until) {
  const s = [];
  let f = 60;
  while (f < 360) {                                  // through the title, then any intro dialogue
    s.push(`${f}:start`, `${f + 8}:`); f += 24;
    s.push(`${f}:a`, `${f + 8}:`); f += 24;
  }
  const dirs = ['right', 'down', 'left', 'up'];
  for (let i = 0; f < until; i++) {                  // then walk, interacting as we go
    s.push(`${f}:${dirs[i % 4]}`, `${f + 28}:`); f += 36;
    s.push(`${f}:a`, `${f + 6}:`); f += 14;
  }
  return s.join(',');
}

// Stored frame count. Pillow dedupes frames identical to their predecessor, so 1 means the picture
// never changed across the whole window. Returns 2 ("keep it") if Pillow is unavailable.
function frameCount(gif) {
  const res = spawnSync('python3', ['-c',
    'import sys;from PIL import Image;print(Image.open(sys.argv[1]).n_frames)', gif],
    { encoding: 'utf8' });
  const n = Number((res.stdout || '').trim());
  return Number.isFinite(n) && n > 0 ? n : 2;
}

// Last resort for an example whose tuned frame is blank: the FIRST frame that had a picture on it.
// The diagnostic ROMs print a result and then clear to a solid colour — bench-access paints its
// table on frame 18 and is a flat block by frame 19 — so a shot tuned to frame 180 catches the
// aftermath, not the output. gba-shot's sequence mode already refuses to open on a blank frame, so
// asking it for exactly one frame from frame 0 hands back the first painted one.
function firstPaintedStill(rom, frames, keys, pngOut) {
  const tmp = `${pngOut}.first.gif`;
  const res = spawnSync(path.join(root, 'scripts', 'gif.sh'), [rom, tmp, String(frames), keys], {
    env: { ...process.env, GIF_SCALE: '1', GIF_FROM: '0', GIF_EVERY: '1', GIF_MAX_FRAMES: '1' },
    stdio: ['ignore', 'ignore', 'pipe'],
    encoding: 'utf8',
  });
  if (res.status !== 0) { if (fs.existsSync(tmp)) fs.unlinkSync(tmp); return false; }
  const conv = spawnSync('python3', ['-c',
    'import sys;from PIL import Image;Image.open(sys.argv[1]).convert("RGB").save(sys.argv[2])',
    tmp, pngOut], { encoding: 'utf8' });
  fs.unlinkSync(tmp);
  return conv.status === 0 && fs.existsSync(pngOut);
}

// Mirrors gba-shot's blank test: is the picture a single flat colour? A blank square is not a
// preview, and committing one is worse than the empty index cell it replaces.
function isFlat(png) {
  const res = spawnSync('python3', ['-c',
    'import sys;from PIL import Image;i=Image.open(sys.argv[1]).convert("RGB");'
    + 'c=i.getcolors(70000) or [];'
    + 'print(1 if not c or max(n for n,_ in c) >= 0.999*i.width*i.height else 0)', png],
    { encoding: 'utf8' });
  return (res.stdout || '').trim() === '1';
}

const done = [], skipped = [], failed = [], stills = [], nothing = [], unpublishable = [], kept = [];
for (const name of examples) {
  const exDir = path.join(dir, name);
  const pkg = JSON.parse(fs.readFileSync(path.join(exDir, 'package.json'), 'utf8'));
  const script = (pkg.scripts && (pkg.scripts.gif || pkg.scripts.shot)) || '';
  const m = CAP_RE.exec(script);

  if (NO_PREVIEW.has(name)) { unpublishable.push(name); continue; }

  const rom = findRom(exDir, pkg, m);
  if (!rom) { skipped.push(`${name} (no ROM — build it first)`); continue; }

  const gifOut = path.join(exDir, 'preview.gif');
  const pngOut = path.join(exDir, 'preview.png');
  const existing = fs.existsSync(gifOut) ? gifOut : (fs.existsSync(pngOut) ? pngOut : null);
  if (!force && existing && fs.statSync(existing).mtimeMs >= fs.statSync(rom).mtimeMs) {
    skipped.push(`${name} (up to date)`);
    continue;
  }

  const over = WINDOW[name] || {};
  const tunedFrames = Number((m && m[2]) || DEFAULT_FRAMES);
  const tunedKeys = ((m && m[3]) || '').trim().replace(/^"|"$/g, '');
  let frames = tunedFrames;
  let keys = tunedKeys;
  // An authored schedule always wins — it knows the example's controls and this does not.
  const driving = !keys && over.drive !== false;
  if (driving) {
    frames = Math.max(frames, AUTOPLAY_FRAMES);
    keys = autoplay(frames);
  }
  // Record the window ENDING on the example's tuned shot frame, rather than a fixed offset from
  // boot. That frame is the moment its author picked as representative — and for the many examples
  // that idle on a title screen until a scheduled keypress lands, a boot-anchored window records
  // nothing but a still title card.
  //
  // Floored at MIN_FROM, never 0: the opening frames are boot, and an example tuned to a shot at
  // frame 120 or 180 would otherwise have its whole window start at power-on. When that floor
  // leaves too little room, run PAST the tuned frame instead of shortening the clip — a short
  // capture is what made an earlier pass emit one-frame previews.
  const every = String(over.every || PREVIEW_EVERY);
  if (over.frames) frames = over.frames;
  const span = Number(PREVIEW_FRAMES) * Number(every);
  const from = over.from !== undefined ? over.from : Math.max(MIN_FROM, frames - span);
  const run = Math.max(frames, from + span);

  // Capture to a scratch path and promote only on success. Several of these previews are original
  // hand-committed art; writing (or deleting) in place means a run that finds nothing to record
  // destroys the picture that was already there. A --force run did exactly that to seven of them.
  // Keep the real extension: both gif.sh and Pillow pick their format from it.
  const gifTmp = path.join(exDir, '.preview.new.gif');
  const pngTmp = path.join(exDir, '.preview.new.png');
  const res = spawnSync(path.join(root, 'scripts', 'gif.sh'), [rom, gifTmp, String(run), keys], {
    env: {
      ...process.env,
      GIF_SCALE: PREVIEW_SCALE,
      GIF_MAX_FRAMES: PREVIEW_FRAMES,
      GIF_EVERY: every,
      GIF_FROM: String(from),
    },
    stdio: ['ignore', 'ignore', 'pipe'],
    encoding: 'utf8',
  });

  // gif.sh exits 3 for "nothing to record" — a screen that is blank for the whole window. Anything
  // else non-zero is a real tooling failure.
  const blank = res.status === 3;
  if (res.status !== 0 && !blank) {
    if (fs.existsSync(gifTmp)) fs.unlinkSync(gifTmp);
    failed.push(`${name}: ${(res.stderr || '').trim().split('\n').pop()}`);
    continue;
  }

  // A one-frame "animation" is just a worse PNG, so fall back to the still — captured at the
  // example's OWN tuned frame with its OWN keys, not the autoplay ones. Autoplay exists to find
  // motion; where there is none, mashing start into an example can only leave it somewhere its
  // author did not choose (it blanked `minimal` outright).
  if (blank || frameCount(gifTmp) <= 1) {
    if (fs.existsSync(gifTmp)) fs.unlinkSync(gifTmp);

    // A still that is already there was chosen by a person. Leave it — this pass exists to fill
    // gaps, not to replace curated art with a machine's guess at the best frame.
    if (fs.existsSync(pngOut)) { kept.push(name); continue; }
    // best_still.py scans the run and keeps the frame with the most distinct colours, rather than
    // trusting one tuned frame number — several examples spend theirs mid-transition, and the
    // diagnostics paint their output for a frame or two and then clear. It exits 2 when even the
    // best frame is a dead screen.
    const shot = spawnSync('python3',
      [path.join(root, 'scripts', 'best_still.py'), rom, pngTmp, String(tunedFrames), tunedKeys],
      { stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' });
    if (shot.status !== 0) {
      if (fs.existsSync(pngTmp)) fs.unlinkSync(pngTmp);
      if (shot.status === 2) { nothing.push(name); continue; }
      failed.push(`${name}: still capture: ${(shot.stderr || '').trim().split('\n').pop()}`);
      continue;
    }
    fs.renameSync(pngTmp, pngOut);
    console.log(`${name}: preview.png (${Math.round(fs.statSync(pngOut).size / 1024)}kb, still)`);
    stills.push(name);
    continue;
  }

  fs.renameSync(gifTmp, gifOut);
  const kb = Math.round(fs.statSync(gifOut).size / 1024);
  console.log(`${name}: preview.gif (${kb}kb)`);
  done.push(name);
}

console.log(`\nclips ${done.length}, stills ${stills.length}, kept ${kept.length},`
  + ` blank ${nothing.length}, skipped ${skipped.length}, failed ${failed.length}`);
if (kept.length) {
  console.log(`no motion, kept the existing hand-picked still: ${kept.join(', ')}`);
}
if (stills.length) {
  console.log(`no motion, wrote a still instead: ${stills.join(', ')}`);
}
if (unpublishable.length) {
  console.log(`no preview by policy (unlicensed art — replace the assets or drop the example):`
    + ` ${unpublishable.join(', ')}`);
}
if (nothing.length) {
  console.log(`NO PREVIEW — the screen is a flat colour even as a still: ${nothing.join(', ')}`);
}
if (skipped.length) console.log(`skipped: ${skipped.join(', ')}`);
if (failed.length) { console.log(`failed:\n  ${failed.join('\n  ')}`); process.exitCode = 1; }
