#!/usr/bin/env node
// Adds (or refreshes) the `gif` script in every examples/*/package.json, right after `shot`, so
// `npm run gif -w <example>` records an animated clip the same way `npm run shot` records a still.
//
// The gif line INHERITS that example's own `shot` tuning — its ROM name, frame count and key
// schedule — because those were hand-picked so the capture lands on something worth looking at,
// and a clip wants the same run. Idempotent: re-run it after adding an example or retuning a shot.
'use strict';
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const dir = path.join(root, 'examples');
const examples = fs
  .readdirSync(dir, { withFileTypes: true })
  .filter((e) => e.isDirectory() && fs.existsSync(path.join(dir, e.name, 'package.json')))
  .map((e) => e.name)
  .sort();

// `shot` looks like: npm run build && ../../scripts/screenshot.sh <rom> screenshot.png [frames] [keys]
// Reuse everything after the script name, swapping only the output file.
const SHOT_RE = /^(npm run build && )?(\.\.\/\.\.\/scripts\/)screenshot\.sh (.*)$/;

let changed = 0;
let skipped = [];
for (const name of examples) {
  const file = path.join(dir, name, 'package.json');
  const text = fs.readFileSync(file, 'utf8');
  const pkg = JSON.parse(text);
  const shot = pkg.scripts && pkg.scripts.shot;
  if (!shot) { skipped.push(name); continue; }
  const m = SHOT_RE.exec(shot);
  if (!m) { skipped.push(name); continue; }
  const gif = `${m[1] || ''}${m[2]}gif.sh ${m[3].replace('screenshot.png', 'screenshot.gif')}`;
  if (pkg.scripts.gif === gif) continue;

  // Splice the line in as TEXT rather than re-serialising the object: a JSON.stringify round-trip
  // reflows hand-formatted blocks elsewhere in the file (several examples keep their
  // tish.rustDependencies entries on one line each) and buries this one-line change in noise.
  const lines = text.split('\n').filter((l) => !/^\s*"gif":/.test(l));
  const at = lines.findIndex((l) => /^\s*"shot":/.test(l));
  if (at < 0) { skipped.push(name); continue; }
  const indent = /^\s*/.exec(lines[at])[0];
  // `shot` is normally followed by more scripts; if it is the last one, move the comma onto it.
  const shotEndsList = !lines[at].endsWith(',');
  if (shotEndsList) lines[at] += ',';
  lines.splice(at + 1, 0, `${indent}${JSON.stringify('gif')}: ${JSON.stringify(gif)}${shotEndsList ? '' : ','}`);
  fs.writeFileSync(file, lines.join('\n'));
  changed++;
}

console.log(`gif scripts: ${changed} updated, ${examples.length - changed - skipped.length} already current`);
if (skipped.length) console.log(`no recognisable 'shot' script (skipped): ${skipped.join(', ')}`);
