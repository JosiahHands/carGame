import { defineConfig } from 'vite';

// The game page lives in app/. It is built into dist/ with relative paths
// so Electron can load it straight from disk.
export default defineConfig({
  root: 'app',
  base: './',
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 2000,
  },
});
