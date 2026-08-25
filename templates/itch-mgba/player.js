// Auto-loads game.gba into vendored mGBA WASM (@thenick775/mgba-wasm).
// Requires cross-origin isolation (COOP + COEP) for SharedArrayBuffer / threads.
import mGBA from './vendor/mgba.js';

const statusEl = document.getElementById('status');
const canvas = document.getElementById('screen');

function setStatus(msg) {
  if (!statusEl) return;
  if (!msg) {
    statusEl.classList.add('hidden');
    statusEl.textContent = '';
    return;
  }
  statusEl.classList.remove('hidden');
  statusEl.textContent = msg;
}

function isolationHint() {
  if (typeof SharedArrayBuffer === 'undefined') {
    return (
      'This build needs SharedArrayBuffer (cross-origin isolation).\n' +
      'On itch.io: enable SharedArrayBuffer / COOP+COEP in Embed options.\n' +
      'Locally: serve with Cross-Origin-Opener-Policy: same-origin and ' +
      'Cross-Origin-Embedder-Policy: require-corp.'
    );
  }
  return null;
}

async function main() {
  const iso = isolationHint();
  if (iso) {
    setStatus(iso);
    return;
  }

  setStatus('Starting mGBA…');
  const Module = await mGBA({ canvas });
  await Module.FSInit();

  // Prefer a clean boot for published builds (no leftover autosave from prior visits).
  Module.setCoreSettings?.({
    autoSaveStateEnable: false,
    restoreAutoSaveStateOnLoad: false,
    rewindEnable: false,
    showFpsCounter: false,
  });

  setStatus('Loading ROM…');
  const res = await fetch('./game.gba');
  if (!res.ok) throw new Error(`Failed to fetch game.gba (${res.status})`);
  const buf = new Uint8Array(await res.arrayBuffer());
  const romPath = `${Module.filePaths().gamePath}/game.gba`;
  Module.FS.writeFile(romPath, buf);

  if (!Module.loadGame(romPath)) {
    throw new Error('mGBA failed to load game.gba');
  }

  setStatus(null);
  canvas.focus();

  // Click-to-focus (itch iframe often steals focus until interaction).
  canvas.addEventListener('click', () => canvas.focus());
  document.addEventListener('click', () => canvas.focus());
}

main().catch((err) => {
  console.error(err);
  setStatus(err?.message || String(err));
});
