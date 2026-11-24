import { app, BrowserWindow, ipcMain, Menu, session, type IpcMainInvokeEvent } from 'electron';
import { fileURLToPath } from 'node:url';
import * as fs from 'node:fs';
import * as path from 'node:path';

/** What the build wrote about the packs in this game (see vite.config.mts). */
interface PacksFile {
  packs: string[];
  /** Packs with a main-process part (packs/<name>/main.ts). */
  withMain: string[];
  /** The game's source folder, where packs/installed.json lists the installed packs. */
  source: string;
}

function readPacksFile(): PacksFile {
  try {
    return JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'dist', 'packs.json'), 'utf8'));
  } catch {
    return { packs: [], withMain: [], source: '' };
  }
}

/** The installed packs list in the game's source folder, if this copy was built from one. */
function installedList(packsFile: PacksFile): string | null {
  if (!packsFile.source) return null;
  const file = path.join(packsFile.source, 'packs', 'installed.json');
  return fs.existsSync(file) ? file : null;
}

/** The game's own page. Only it may ask the main process to do things (remove a pack, call a pack). */
const GAME_PAGE = path.join(__dirname, '..', 'dist', 'index.html');

function fromGame(event: IpcMainInvokeEvent): boolean {
  const url = event.senderFrame?.url ?? '';
  try {
    return url.startsWith('file:') && path.resolve(fileURLToPath(url)) === path.resolve(GAME_PAGE);
  } catch {
    return false;
  }
}

/** What a pack's main-process part gets (packs/<name>/main.ts exports activate(host)). */
export interface PackMainHost {
  /** Answers the game page's carGame.packCall(pack, name, ...args). */
  handle(name: string, handler: (...args: unknown[]) => unknown): void;
  /** A folder for this pack's files, inside the app's data folder. */
  dataDir: string;
}

/**
 * Sets up the packs: removing one from the Packs tab, and starting the
 * main-process part of each pack that has one. A pack that fails to start is
 * skipped, so it can't stop the game from opening.
 */
function startPacks(): void {
  const packsFile = readPacksFile();
  ipcMain.handle('packs:info', () => ({ removable: installedList(packsFile) !== null }));
  ipcMain.handle('packs:remove', (event, pack: unknown) => {
    if (!fromGame(event)) throw new Error('Not allowed');
    const file = installedList(packsFile);
    if (!file || typeof pack !== 'string') return false;
    try {
      const saved = JSON.parse(fs.readFileSync(file, 'utf8'));
      const packs = (Array.isArray(saved.packs) ? saved.packs : []).filter((p: unknown) => p !== pack);
      fs.writeFileSync(file, `${JSON.stringify({ packs }, null, 2)}\n`);
      return true;
    } catch {
      return false;
    }
  });
  for (const pack of packsFile.withMain) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const mod = require(path.join(__dirname, 'packs', `${pack}.js`)) as { activate?: (host: PackMainHost) => void };
      const dataDir = path.join(app.getPath('userData'), 'packs', pack);
      fs.mkdirSync(dataDir, { recursive: true });
      mod.activate?.({
        handle: (name, handler) =>
          ipcMain.handle(`pack:${pack}:${name}`, (event, ...args) => {
            if (!fromGame(event)) throw new Error('Not allowed');
            return handler(...args);
          }),
        dataDir,
      });
    } catch (error) {
      console.error(`The ${pack} pack failed to start:`, error);
    }
  }
}

function createWindow(): void {
  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 800,
    minHeight: 500,
    title: 'Car Game',
    backgroundColor: '#0b0f17',
    autoHideMenuBar: true,
    // Window and taskbar icon on Linux; macOS and Windows use the app's own icon.
    icon: path.join(__dirname, '..', 'build', 'icon.png'),
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      // window.carGame: the few calls the game page can make to this side.
      preload: path.join(__dirname, 'preload.js'),
    },
  });

  // The game never opens other pages or windows.
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  win.webContents.on('will-navigate', (event) => event.preventDefault());

  void win.loadFile(GAME_PAGE);
}

app.whenReady().then(() => {
  // macOS keeps its standard menu (Cmd+Q, copy/paste). Other systems get none.
  if (process.platform !== 'darwin') Menu.setApplicationMenu(null);
  startPacks();
  createWindow();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

// Write saved settings (localStorage) to disk before the app exits.
app.on('before-quit', () => session.defaultSession.flushStorageData());

app.on('window-all-closed', () => app.quit());
