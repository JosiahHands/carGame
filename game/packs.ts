// Game packs: optional add-ons that live in packs/<name>/ and are built into
// the game when installed (see packs/README.md). This file is the game's side:
// what a pack can see and hook into (GameApi), and the PackHost that starts,
// stops and removes packs, and keeps a broken pack from breaking the game.
import type * as THREE from 'three';
import { bridge } from './bridge';
import type { CarRig } from './carModel';
import type { CarPhysics } from './carPhysics';
import type { Effects } from './effects';
import type { Pedestrians } from './pedestrians';
import type { Settings } from './settings';
import type { Traffic } from './traffic';
import type { World } from './world';

/** What a pack is: its name and how to start it. packs/<name>/index.ts exports one as its default. */
export interface Pack {
  /** The folder name in packs/, like "graphics". */
  id: string;
  name: string;
  icon: string;
  /** One or two sentences for the Packs tab. */
  about: string;
  /** Starts the pack. Called at startup, and again when it's switched back on. */
  activate(game: GameApi): PackInstance;
}

/** A running pack. Every hook is optional. */
export interface PackInstance {
  /** A new map (or track) was just built. */
  mapLoaded?(): void;
  /** Every frame, after the game has moved everything and before drawing. `dt` is game time (0 when paused). */
  frame?(dt: number, realDt: number): void;
  /** Draws the frame instead of the game. Returns false to let the game draw it. */
  render?(): boolean;
  /** The window changed size. */
  resize?(width: number, height: number): void;
  /** Day or night changed. */
  nightChanged?(night: boolean): void;
  /** A car model was built: the player's, a rival's, or a traffic or parked car. */
  carBuilt?(rig: CarRig): void;
  /** Adds the pack's own options to its section of the Packs tab. */
  settings?(panel: PanelUI): void;
  /** Stops the pack and undoes everything it changed. */
  dispose(): void;
}

/** Things that happen in the game that packs can react to. */
export interface GameEvents {
  /** The player hit something. `impact` is the speed lost, in m/s. */
  crash: { x: number; z: number; impact: number };
  /** Something was destroyed: a building, a parked car, a lamp, a tree. */
  destroyed: { x: number; z: number; kind: string };
  /** The player landed a jump. */
  jump: { x: number; z: number; airTime: number };
  /** The player skimmed past traffic. */
  nearMiss: { x: number; z: number };
  explosion: { x: number; z: number; size: number };
  horn: { x: number; z: number };
  /** The player banked a drift. */
  drift: { x: number; z: number; points: number };
}

type Listener<K extends keyof GameEvents> = (event: GameEvents[K]) => void;

/** Controls a pack can put on its section of the Packs tab (the garage's own controls). */
export interface PanelUI {
  heading(text: string): void;
  hint(text: string): void;
  choice<T extends string>(label: string, options: { value: T; label: string }[], get: () => T, set: (v: T) => void): void;
  slider(label: string, min: number, max: number, step: number, get: () => number, set: (v: number) => void, format: (v: number) => string): void;
  /** A row with a label and any element (an input, a few buttons...). */
  row(label: string, ...content: HTMLElement[]): void;
  append(...content: HTMLElement[]): void;
  /** Draws the tab again, after a change that shows or hides other options. */
  refresh(): void;
}

/** The game, as a pack sees it. */
export interface GameApi {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene: THREE.Scene;
  readonly camera: THREE.PerspectiveCamera;
  readonly world: World;
  readonly effects: Effects;
  readonly settings: Settings;
  readonly pedestrians: Pedestrians;
  readonly traffic: Traffic;
  /** The player's car physics (position, speed, heading...). */
  readonly player: CarPhysics;
  /** The player's car model as it is now (it's rebuilt when the car changes). */
  playerCar(): CarRig;
  /** What the game is showing. */
  mode(): 'menu' | 'garage' | 'drive' | 'photo';
  /** In a race or other event (not free roam). */
  inEvent(): boolean;
  /** Paused, or in a menu: game time isn't running. */
  paused(): boolean;
  /** A short message in the middle of the screen. */
  toast(text: string, ms?: number): void;
  /** A layer over the game for the pack's own labels and panels. */
  overlay: HTMLElement;
  /** Listens for something happening in the game. Returns a function that stops listening. */
  on<K extends keyof GameEvents>(type: K, listener: Listener<K>): () => void;
  /** Runs `handler` when a key (a KeyboardEvent code, like "KeyK") is pressed while driving. Returns a function that stops it. */
  onKey(code: string, handler: () => void): () => void;
  /** Saved data for this pack, kept between sessions. */
  load<T>(key: string, fallback: T): T;
  save(key: string, value: unknown): void;
  /** Calls the pack's main-process part (packs/<name>/main.ts). Rejects outside Electron. */
  callMain(name: string, ...args: unknown[]): Promise<unknown>;
}

/** The game without the per-pack parts, which PackHost fills in for each pack. */
export type GameCore = Omit<GameApi, 'on' | 'onKey' | 'load' | 'save' | 'callMain' | 'overlay'>;

/** Each pack's switched-off and removed state, saved between sessions. */
interface PackState {
  off: string[];
  /**
   * Removed packs, and the build they were removed from. A removed pack stays
   * in the game until it's rebuilt; if a later build still has it, it was
   * installed again.
   */
  removed: Record<string, string>;
}

const STATE_KEY = 'carGame.packs.v1';
/** A pack that throws this many errors is switched off, so the game keeps running. */
const MAX_ERRORS = 5;

interface Running {
  pack: Pack;
  instance: PackInstance;
  errors: number;
  /** Event listeners and keys to remove when it stops. */
  cleanup: (() => void)[];
}

/**
 * Starts the installed packs and calls their hooks. Every call into a pack is
 * wrapped, so a pack that throws is reported, and switched off if it keeps
 * failing, instead of stopping the game.
 */
export class PackHost {
  private running = new Map<string, Running>();
  private listeners = new Map<keyof GameEvents, Set<(e: unknown) => void>>();
  private keys = new Map<string, Set<() => void>>();
  private state: PackState;
  /** Whether packs can be removed (the game was built from a folder with packs/installed.json). */
  removable = false;

  constructor(
    readonly packs: Pack[],
    /** Changes every time the game is built (see vite.config.mts). */
    private buildId: string,
    private core: GameCore,
    private overlay: HTMLElement,
  ) {
    this.state = loadState();
    void bridge()?.packsInfo().then((info) => (this.removable = info.removable)).catch(() => {});
  }

  /** The packs in the game that haven't been removed. */
  get installed(): Pack[] {
    return this.packs.filter((p) => this.state.removed[p.id] !== this.buildId);
  }

  isOn(id: string): boolean {
    return this.running.has(id);
  }

  /** Starts every installed pack that isn't switched off. */
  startAll(): void {
    for (const pack of this.installed) if (!this.state.off.includes(pack.id)) this.start(pack);
  }

  /** Switches a pack on or off (and remembers it). */
  setOn(id: string, on: boolean): void {
    const pack = this.installed.find((p) => p.id === id);
    if (!pack) return;
    this.state.off = this.state.off.filter((p) => p !== id);
    if (!on) this.state.off.push(id);
    saveState(this.state);
    if (on && !this.running.has(id)) this.start(pack);
    if (!on) this.stop(id);
  }

  /**
   * Removes a pack: stops it now, and takes it off the installed list so the
   * next update builds the game without it.
   */
  async remove(id: string): Promise<boolean> {
    const done = (await bridge()?.removePack(id).catch(() => false)) ?? false;
    if (!done) return false;
    this.stop(id);
    this.state.removed[id] = this.buildId;
    saveState(this.state);
    return true;
  }

  // ---------- calling the packs ----------

  mapLoaded(): void {
    this.each('mapLoaded', (i) => i.mapLoaded?.());
  }

  frame(dt: number, realDt: number): void {
    this.each('frame', (i) => i.frame?.(dt, realDt));
  }

  /** Lets a pack draw the frame. Returns true if one did. */
  render(): boolean {
    let drawn = false;
    this.each('render', (i) => {
      if (!drawn && i.render) drawn = i.render();
    });
    return drawn;
  }

  resize(width: number, height: number): void {
    this.each('resize', (i) => i.resize?.(width, height));
  }

  nightChanged(night: boolean): void {
    this.each('nightChanged', (i) => i.nightChanged?.(night));
  }

  carBuilt(rig: CarRig): void {
    this.each('carBuilt', (i) => i.carBuilt?.(rig));
  }

  settings(id: string, panel: PanelUI): void {
    const run = this.running.get(id);
    if (run?.instance.settings) this.guard(run, 'settings', () => run.instance.settings!(panel));
  }

  emit<K extends keyof GameEvents>(type: K, event: GameEvents[K]): void {
    for (const listener of this.listeners.get(type) ?? []) {
      try {
        listener(event);
      } catch (error) {
        console.error(`A pack failed on the ${type} event:`, error);
      }
    }
  }

  /** A key was pressed while driving. Returns true if a pack used it. */
  key(code: string): boolean {
    const handlers = this.keys.get(code);
    if (!handlers || handlers.size === 0) return false;
    for (const handler of handlers) {
      try {
        handler();
      } catch (error) {
        console.error(`A pack failed on the ${code} key:`, error);
      }
    }
    return true;
  }

  // ---------- starting and stopping ----------

  private start(pack: Pack): void {
    const cleanup: (() => void)[] = [];
    const layer = document.createElement('div');
    layer.className = `pack-layer pack-${pack.id}`;
    this.overlay.append(layer);
    cleanup.push(() => layer.remove());
    const storageKey = (key: string) => `carGame.pack.${pack.id}.${key}`;
    const game: GameApi = {
      ...this.core,
      overlay: layer,
      on: (type, listener) => {
        const set = this.listeners.get(type) ?? new Set();
        this.listeners.set(type, set);
        set.add(listener as (e: unknown) => void);
        const off = () => set.delete(listener as (e: unknown) => void);
        cleanup.push(off);
        return off;
      },
      onKey: (code, handler) => {
        const set = this.keys.get(code) ?? new Set();
        this.keys.set(code, set);
        set.add(handler);
        const off = () => set.delete(handler);
        cleanup.push(off);
        return off;
      },
      load: <T,>(key: string, fallback: T): T => {
        try {
          const saved = localStorage.getItem(storageKey(key));
          return saved === null ? fallback : (JSON.parse(saved) as T);
        } catch {
          return fallback;
        }
      },
      save: (key, value) => {
        try {
          localStorage.setItem(storageKey(key), JSON.stringify(value));
        } catch {
          // Saving is a convenience; the pack keeps working without it.
        }
      },
      callMain: (name, ...args) => bridge()?.packCall(pack.id, name, ...args) ?? Promise.reject(new Error('Only in the desktop app')),
    };
    try {
      const instance = pack.activate(game);
      this.running.set(pack.id, { pack, instance, errors: 0, cleanup });
    } catch (error) {
      console.error(`The ${pack.name} pack failed to start:`, error);
      for (const undo of cleanup) undo();
      this.core.toast(`${pack.icon} ${pack.name} failed to start`, 3000);
    }
  }

  private stop(id: string): void {
    const run = this.running.get(id);
    if (!run) return;
    this.running.delete(id);
    try {
      run.instance.dispose();
    } catch (error) {
      console.error(`The ${run.pack.name} pack failed to stop cleanly:`, error);
    }
    for (const undo of run.cleanup) undo();
  }

  private each(hook: string, call: (instance: PackInstance) => void): void {
    for (const run of [...this.running.values()]) this.guard(run, hook, () => call(run.instance));
  }

  private guard(run: Running, hook: string, call: () => void): void {
    try {
      call();
    } catch (error) {
      run.errors++;
      console.error(`The ${run.pack.name} pack failed in ${hook}:`, error);
      if (run.errors >= MAX_ERRORS) {
        this.stop(run.pack.id);
        this.core.toast(`${run.pack.icon} ${run.pack.name} kept failing, so it was switched off`, 3500);
      }
    }
  }
}

function loadState(): PackState {
  try {
    const saved = JSON.parse(localStorage.getItem(STATE_KEY) ?? '{}');
    const off = Array.isArray(saved.off) ? saved.off.filter((x: unknown): x is string => typeof x === 'string') : [];
    const removed = saved.removed && typeof saved.removed === 'object' && !Array.isArray(saved.removed) ? saved.removed : {};
    return { off, removed };
  } catch {
    return { off: [], removed: {} };
  }
}

function saveState(state: PackState): void {
  try {
    localStorage.setItem(STATE_KEY, JSON.stringify(state));
  } catch {
    // Saving is a convenience.
  }
}
