#!/usr/bin/env node
// Root CLI:
//   npm run itch -- publish <example> [--frames N]
//   npm run itch -- serve <example> [--port N]
'use strict';

const { spawnSync } = require('child_process');
const fs = require('fs');
const http = require('http');
const path = require('path');
const { URL } = require('url');

const root = path.resolve(__dirname, '..');
const publishSh = path.join(root, 'scripts', 'publish-itch.sh');
const examplesDir = path.join(root, 'examples');
const itchDist = path.join(root, 'dist', 'itch');

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.wasm': 'application/wasm',
  '.gba': 'application/octet-stream',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.json': 'application/json',
  '.map': 'application/json',
  '.txt': 'text/plain; charset=utf-8',
  '.md': 'text/plain; charset=utf-8',
};

function usage(code = 2) {
  console.error(`usage:
  npm run itch -- publish <example> [--frames N]
  npm run itch -- serve <example> [--port N]

  publish  Package examples/<example> for itch.io (HTML5 zip + cover art).
  serve    HTTP-serve dist/itch/<example>/html5 with COOP/COEP (mGBA WASM).

  Examples:
    npm run itch -- publish shmup
    npm run itch -- serve shmup
    npm run itch -- serve shmup --port 4173

  Optional publish: ITCH_TARGET=user/game:html5 butler-pushes the html5/ folder.`);
  process.exit(code);
}

function listExamples() {
  if (!fs.existsSync(examplesDir)) return [];
  return fs
    .readdirSync(examplesDir, { withFileTypes: true })
    .filter((d) => d.isDirectory() && fs.existsSync(path.join(examplesDir, d.name, 'package.json')))
    .map((d) => d.name)
    .sort();
}

function listPackaged() {
  if (!fs.existsSync(itchDist)) return [];
  return fs
    .readdirSync(itchDist, { withFileTypes: true })
    .filter((d) => d.isDirectory() && fs.existsSync(path.join(itchDist, d.name, 'html5', 'index.html')))
    .map((d) => d.name)
    .sort();
}

function requireExampleName(name) {
  if (!name || name.startsWith('-')) {
    console.error('error: missing example name');
    const ex = listExamples();
    if (ex.length) console.error(`known examples: ${ex.join(', ')}`);
    usage();
  }
  return name;
}

function publish(name, forward) {
  const examplePath = path.join(examplesDir, name);
  if (!fs.existsSync(path.join(examplePath, 'package.json'))) {
    console.error(`error: no example at examples/${name}`);
    const ex = listExamples();
    if (ex.length) console.error(`known examples: ${ex.join(', ')}`);
    process.exit(1);
  }
  const r = spawnSync(publishSh, [examplePath, '--name', name, ...forward], {
    stdio: 'inherit',
    cwd: root,
    env: process.env,
  });
  process.exit(r.status === null ? 1 : r.status);
}

function parsePort(forward) {
  let port = Number(process.env.PORT) || 4173;
  for (let i = 0; i < forward.length; i++) {
    if (forward[i] === '--port') {
      port = Number(forward[i + 1]);
      if (!Number.isInteger(port) || port < 1 || port > 65535) {
        console.error('error: --port needs an integer 1–65535');
        process.exit(1);
      }
      i++;
    } else {
      console.error(`error: unknown serve flag: ${forward[i]}`);
      usage();
    }
  }
  return port;
}

function serve(name, forward) {
  const html5 = path.join(itchDist, name, 'html5');
  if (!fs.existsSync(path.join(html5, 'index.html'))) {
    console.error(`error: no packaged build at dist/itch/${name}/html5`);
    console.error(`run:  npm run itch -- publish ${name}`);
    const packaged = listPackaged();
    if (packaged.length) console.error(`packaged: ${packaged.join(', ')}`);
    process.exit(1);
  }

  const port = parsePort(forward);
  const rootDir = path.resolve(html5);

  const server = http.createServer((req, res) => {
    // COOP + COEP so SharedArrayBuffer / mGBA threads work (same as itch embed option).
    res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
    res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
    res.setHeader('Cross-Origin-Resource-Policy', 'same-origin');

    if (req.method !== 'GET' && req.method !== 'HEAD') {
      res.writeHead(405);
      res.end();
      return;
    }

    let urlPath;
    try {
      urlPath = decodeURIComponent(new URL(req.url || '/', `http://127.0.0.1`).pathname);
    } catch {
      res.writeHead(400);
      res.end('bad url');
      return;
    }
    if (urlPath.endsWith('/')) urlPath += 'index.html';

    const filePath = path.resolve(rootDir, '.' + urlPath);
    if (!filePath.startsWith(rootDir + path.sep) && filePath !== rootDir) {
      res.writeHead(403);
      res.end('forbidden');
      return;
    }

    fs.stat(filePath, (err, st) => {
      if (err || !st.isFile()) {
        res.writeHead(404);
        res.end('not found');
        return;
      }
      const type = MIME[path.extname(filePath).toLowerCase()] || 'application/octet-stream';
      res.writeHead(200, {
        'Content-Type': type,
        'Content-Length': st.size,
        'Cache-Control': 'no-store',
      });
      if (req.method === 'HEAD') {
        res.end();
        return;
      }
      fs.createReadStream(filePath).pipe(res);
    });
  });

  server.listen(port, '127.0.0.1', () => {
    console.log(`itch serve: http://127.0.0.1:${port}/  (${path.relative(root, html5)})`);
    console.log('COOP/COEP enabled — open that URL to debug the mGBA WASM player.');
  });
}

const args = process.argv.slice(2);
if (args.length === 0 || args[0] === '-h' || args[0] === '--help') usage(args.length ? 0 : 2);

const cmd = args[0];
if (cmd === 'publish') {
  const name = requireExampleName(args[1]);
  publish(name, args.slice(2));
} else if (cmd === 'serve') {
  const name = requireExampleName(args[1]);
  serve(name, args.slice(2));
} else {
  console.error(`error: unknown command '${cmd}'`);
  usage();
}
