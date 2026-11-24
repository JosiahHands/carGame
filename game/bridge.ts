// The calls the game page can make to Electron's main process, set up by
// app/electron/preload.ts. Missing when the page runs outside Electron (in a
// plain browser, for testing), so every use checks for it.

export interface CarGameBridge {
  /** Whether the game was built from a folder its packs can be removed from. */
  packsInfo(): Promise<{ removable: boolean }>;
  /** Takes a pack out of packs/installed.json, so the next update leaves it out. */
  removePack(pack: string): Promise<boolean>;
  /** Calls something a pack's main-process part offers. */
  packCall(pack: string, name: string, ...args: unknown[]): Promise<unknown>;
}

export function bridge(): CarGameBridge | null {
  return (window as unknown as { carGame?: CarGameBridge }).carGame ?? null;
}
