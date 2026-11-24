import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
// A plain JavaScript helper, shared with the install scripts.
import { availablePacks, installedPacks } from './scripts/packs.mjs';

const ROOT = fileURLToPath(new URL('.', import.meta.url));
const VIRTUAL = 'virtual:installed-packs';

/**
 * The packs to build into the game: the ones in packs/installed.json, or in
 * the CAR_GAME_PACKS environment variable when it's set ("all" for every pack,
 * which is how the checks on GitHub build them all).
 */
export function packsToBuild(): string[] {
  const all: string[] = availablePacks();
  const wanted = process.env.CAR_GAME_PACKS;
  const chosen: string[] = wanted === 'all' ? all : wanted !== undefined ? wanted.split(',').filter(Boolean) : installedPacks();
  return chosen.filter((p) => all.includes(p));
}

/**
 * Makes `import packs from 'virtual:installed-packs'` a list of the installed
 * packs. Only those packs' code goes into the game; the rest is left out.
 * It also writes dist/packs.json, which tells the Electron side which packs
 * there are and where the game's folder is (to remove a pack from the game).
 */
function installedPacksPlugin(): Plugin {
  const packs = packsToBuild();
  return {
    name: 'car-game-packs',
    resolveId(id) {
      return id === VIRTUAL ? `\0${VIRTUAL}` : null;
    },
    load(id) {
      if (id !== `\0${VIRTUAL}`) return null;
      const imports = packs.map((p, i) => `import pack${i} from ${JSON.stringify(join(ROOT, 'packs', p, 'index.ts'))};`);
      const list = `export default [${packs.map((_, i) => `pack${i}`).join(', ')}];`;
      // Changes with every build, so the game can tell a rebuilt copy from the old one.
      const buildId = `export const buildId = ${JSON.stringify(String(Date.now()))};`;
      return `${imports.join('\n')}\n${list}\n${buildId}\n`;
    },
    generateBundle() {
      const withMain = packs.filter((p) => existsSync(join(ROOT, 'packs', p, 'main.ts')));
      this.emitFile({
        type: 'asset',
        fileName: 'packs.json',
        source: JSON.stringify({ packs, withMain, source: ROOT }, null, 2),
      });
    },
  };
}

// The game page lives in app/. It is built into dist/ with relative paths
// so Electron can load it straight from disk.
export default defineConfig({
  root: 'app',
  base: './',
  plugins: [installedPacksPlugin()],
  resolve: { dedupe: ['three'] },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 4000,
  },
});
