// Packages the built game (dist/ and dist-electron/) as a real desktop app for
// this computer: "Car Game.app" on macOS, car-game.exe on Windows, car-game on Linux.
// Prints the folder the app was written to.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { packager } from '@electron/packager';
import { NATIVE_SKIP, nativeFolders } from './native.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
// The packages with native code that the installed packs load (see native.mjs):
// the only part of node_modules the app ships.
const { withMain } = JSON.parse(readFileSync(join(ROOT, 'dist', 'packs.json'), 'utf8'));
const keep = nativeFolders(ROOT, withMain);

// The game is already bundled into dist/, so ship only what runs.
const LEFT_OUT = [
  /^\/(app|game|libs|scripts|release|\.git)(\/|$)/,
  /^\/(install|update)\.sh$/,
  /^\/(tsconfig.*\.json|vite\.config\.mts|README\.md|\.gitignore)$/,
];

/** Whether a file (a path like "/node_modules/x/index.js") is left out of the app. */
const skip = (file) => {
  if (!file) return false;
  const rel = file.slice(1);
  if (rel === 'node_modules' || rel.startsWith('node_modules/')) {
    // Kept: a kept package, anything in one, or a folder on the way to one.
    return !keep.some((k) => rel === k || rel.startsWith(`${k}/`) || k.startsWith(`${rel}/`)) || NATIVE_SKIP.some((r) => r.test(rel));
  }
  return LEFT_OUT.some((r) => r.test(file));
};
const [appPath] = await packager({
  dir: '.',
  name: 'Car Game',
  // macOS shows the program's file name as the app name, so keep "Car Game"
  // there; Windows and Linux get a simple file name.
  executableName: process.platform === 'darwin' ? 'Car Game' : 'car-game',
  appBundleId: 'io.github.josiahhands.cargame',
  appCategoryType: 'public.app-category.racing-games',
  icon: 'build/icon', // packager picks icon.icns (macOS) or icon.ico (Windows)
  out: 'release',
  overwrite: true,
  // The native engines' files stay outside the archive, where the computer can load them.
  asar: keep.length ? { unpack: '{**/node_modules/@node-llama-cpp/**,**/node_modules/node-llama-cpp/bins/**}' } : true,
  quiet: true,
  ignore: skip,
});

if (process.platform === 'darwin') {
  // Packaging changes Electron's files, which breaks its signature, and Apple
  // Silicon Macs won't run an app with a broken signature. A local ("ad-hoc")
  // signature is all an app built on this computer needs.
  execFileSync('codesign', ['--force', '--deep', '--sign', '-', join(appPath, 'Car Game.app')], { stdio: 'inherit' });
}

console.log(appPath);
