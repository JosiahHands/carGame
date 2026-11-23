// Packages the built game (dist/ and dist-electron/) as a real desktop app for
// this computer: "Car Game.app" on macOS, car-game.exe on Windows, car-game on Linux.
// Prints the folder the app was written to.
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { packager } from '@electron/packager';

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
  asar: true,
  quiet: true,
  // The game is already bundled into dist/, so ship only what runs.
  ignore: [
    /^\/(app|game|libs|scripts|release|node_modules|\.git)(\/|$)/,
    /^\/(install|update)\.sh$/,
    /^\/(tsconfig.*\.json|vite\.config\.mts|README\.md|\.gitignore)$/,
  ],
});

if (process.platform === 'darwin') {
  // Packaging changes Electron's files, which breaks its signature, and Apple
  // Silicon Macs won't run an app with a broken signature. A local ("ad-hoc")
  // signature is all an app built on this computer needs.
  execFileSync('codesign', ['--force', '--deep', '--sign', '-', join(appPath, 'Car Game.app')], { stdio: 'inherit' });
}

console.log(appPath);
