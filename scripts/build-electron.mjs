// Builds the Electron side of the game into dist-electron/: the main process
// (main.js), the bridge between it and the game page (preload.js), and the
// main-process part of any installed pack that has one (packs/<name>.js,
// from packs/<name>/main.ts). Each file is bundled with what it uses, because
// the packaged app doesn't ship node_modules, apart from the few packages that
// load native code (see native.mjs), which packaging copies in as they are.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'vite';
import { ALL_NATIVE } from './native.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

// The packs the game page was just built with (vite build writes this list).
const { withMain } = JSON.parse(readFileSync(join(ROOT, 'dist', 'packs.json'), 'utf8'));

const input = {
  main: join(ROOT, 'app/electron/main.ts'),
  preload: join(ROOT, 'app/electron/preload.ts'),
};
for (const pack of withMain) {
  const file = join(ROOT, 'packs', pack, 'main.ts');
  if (existsSync(file)) input[`packs/${pack}`] = file;
}

await build({
  configFile: false,
  root: ROOT,
  logLevel: 'warn',
  build: {
    ssr: true, // for Node (Electron's main process), not a web page
    outDir: 'dist-electron',
    emptyOutDir: true,
    target: 'node22',
    minify: false,
    rollupOptions: {
      input,
      external: ['electron', ...ALL_NATIVE],
      output: { format: 'cjs', entryFileNames: '[name].js', chunkFileNames: 'chunks/[name]-[hash].js' },
    },
  },
  // Bundle every package in (only Electron, Node's own modules and the native packages stay outside).
  ssr: { noExternal: true, external: ALL_NATIVE, target: 'node' },
});
