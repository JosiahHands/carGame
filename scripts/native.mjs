// Packages that a pack needs but that can't be bundled into one file: they
// load native code (and files of their own) when they run. The build leaves
// them out of the bundle, and packaging copies them into the app as they are,
// with everything they depend on.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';

/** For each pack, the packages its main-process part loads at run time. */
export const NATIVE_PACKAGES = {
  // The local AI engine (llama.cpp) for the Living City.
  npc: ['node-llama-cpp'],
};

/** Every such package, for the build to leave out of the bundle. */
export const ALL_NATIVE = [...new Set(Object.values(NATIVE_PACKAGES).flat())];

/**
 * Parts of them the app doesn't need: the source code for building llama.cpp
 * yourself (the app uses the ready-built engine), and the very large CUDA
 * builds for NVIDIA cards (those computers use the Vulkan build instead).
 */
export const NATIVE_SKIP = [
  /^node_modules\/node-llama-cpp\/llama\/(gitRelease\.bundle|llama\.cpp)(\/|$)/,
  /^node_modules\/@node-llama-cpp\/[^/]*cuda[^/]*(\/|$)/,
];

/**
 * The folders (like "node_modules/node-llama-cpp") the installed packs'
 * native packages need, found the way Node finds them: the package's own
 * node_modules first, then each folder above it.
 */
export function nativeFolders(root, packs) {
  const found = new Set();
  const visit = (name, from) => {
    for (let dir = from; ; dir = dirname(dir)) {
      const folder = join(dir, 'node_modules', name);
      if (existsSync(join(folder, 'package.json'))) {
        const rel = relative(root, folder).split('\\').join('/');
        if (found.has(rel)) return;
        found.add(rel);
        const pkg = JSON.parse(readFileSync(join(folder, 'package.json'), 'utf8'));
        // Optional dependencies are the ready-built engines: only the ones npm installed for this computer are here.
        for (const dep of Object.keys({ ...pkg.dependencies, ...pkg.optionalDependencies })) visit(dep, folder);
        return;
      }
      if (dir === root || dirname(dir) === dir) return; // not installed (an optional one for another computer)
    }
  };
  for (const pack of packs) for (const name of NATIVE_PACKAGES[pack] ?? []) visit(name, root);
  return [...found].filter((rel) => !NATIVE_SKIP.some((skip) => skip.test(rel)));
}
