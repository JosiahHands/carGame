// Keeps the list of installed game packs: packs/installed.json in the game's
// folder (not in Git, so every install has its own). The build reads it and
// puts just those packs into the game, so updating the game updates them too.
//
//   node scripts/packs.mjs add graphics     install a pack (on the next build)
//   node scripts/packs.mjs add all          install every pack
//   node scripts/packs.mjs remove graphics  remove a pack (on the next build)
//   node scripts/packs.mjs list             the installed packs, one per line
//   node scripts/packs.mjs check            drop packs that no longer exist
//
// Node, rather than sed or jq, so it works the same in Git Bash on Windows.
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const PACKS = join(ROOT, 'packs');
const LIST = join(PACKS, 'installed.json');

/** The packs in the repo: every folder in packs/ with an index.ts. */
export function availablePacks() {
  return readdirSync(PACKS, { withFileTypes: true })
    .filter((d) => d.isDirectory() && existsSync(join(PACKS, d.name, 'index.ts')))
    .map((d) => d.name)
    .sort();
}

/** The installed packs, as written in packs/installed.json (an empty list if there's no file). */
export function installedPacks() {
  try {
    const saved = JSON.parse(readFileSync(LIST, 'utf8'));
    return Array.isArray(saved.packs) ? saved.packs.filter((p) => typeof p === 'string') : [];
  } catch {
    return [];
  }
}

function save(packs) {
  writeFileSync(LIST, `${JSON.stringify({ packs: [...new Set(packs)].sort() }, null, 2)}\n`);
}

function fail(message) {
  console.error(`Error: ${message}`);
  process.exit(1);
}

// Run as a command (the build imports the two functions above instead).
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [command, name = ''] = process.argv.slice(2);
  const available = availablePacks();
  const installed = installedPacks();
  const known = () => {
    if (!/^[a-z0-9-]+$/.test(name) || !available.includes(name)) {
      fail(`there is no pack called "${name}". The packs are: ${available.join(', ')}`);
    }
  };
  switch (command) {
    case 'add':
      if (name === 'all') {
        save([...installed, ...available]);
        console.log(`==> Added every pack: ${available.join(', ')}`);
        break;
      }
      known();
      save([...installed, name]);
      console.log(`==> Added the ${name} pack`);
      break;
    case 'remove':
      known();
      save(installed.filter((p) => p !== name));
      console.log(`==> Removed the ${name} pack`);
      break;
    case 'list':
      for (const p of installed) console.log(p);
      break;
    case 'check': {
      // A pack that was renamed or taken out of the game: forget it, so the build doesn't fail.
      const gone = installed.filter((p) => !available.includes(p));
      if (gone.length > 0) {
        console.log(`==> These packs are no longer in the game, so they were removed: ${gone.join(', ')}`);
        save(installed.filter((p) => available.includes(p)));
      }
      break;
    }
    default:
      fail('use: node scripts/packs.mjs add|remove|list|check [pack]');
  }
}
