// The bridge between the game page and Electron's main process. The page runs
// sandboxed with no Node access; this gives it a few safe calls, as
// window.carGame (typed in game/bridge.ts).
import { contextBridge, ipcRenderer } from 'electron';

/** Pack ids are folder names; the things they offer are names like "think" or "setKey". */
const PACK = /^[a-z0-9-]+$/;
const NAME = /^[a-zA-Z0-9-]+$/;

contextBridge.exposeInMainWorld('carGame', {
  /** Whether the game was built from a folder its packs can be removed from. */
  packsInfo: (): Promise<{ removable: boolean }> => ipcRenderer.invoke('packs:info'),
  /** Takes a pack out of packs/installed.json, so the next update leaves it out. */
  removePack: (pack: string): Promise<boolean> => ipcRenderer.invoke('packs:remove', pack),
  /** Calls something a pack's main-process part offers (see "A main-process part" in packs/README.md). */
  packCall: (pack: string, name: string, ...args: unknown[]): Promise<unknown> => {
    if (!PACK.test(pack) || !NAME.test(name)) return Promise.reject(new Error('Bad pack call'));
    return ipcRenderer.invoke(`pack:${pack}:${name}`, ...args);
  },
});
