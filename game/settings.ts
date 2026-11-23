// Graphics and gameplay settings, with quality presets. Saved between sessions.

export type Quality = 'low' | 'medium' | 'high' | 'ultra';
export type ShadowQuality = 'off' | 'low' | 'high' | 'ultra';
export type EffectsQuality = 'low' | 'medium' | 'high';

export interface Settings {
  /** The preset the graphics settings match, or 'custom' after changing one by hand. */
  quality: Quality | 'custom';
  /** Fraction of the screen's full resolution to draw at (0.5..1). */
  renderScale: number;
  shadows: ShadowQuality;
  /** How far you can see, in metres. */
  viewDistance: number;
  /** How many sparks, smoke puffs and pieces of debris explosions make. */
  effects: EffectsQuality;
  /** Smooth jagged edges. Takes effect the next time the game starts. */
  antialias: boolean;
  /** Frames per second cap; 0 means as fast as the screen refreshes. */
  fpsLimit: number;
  fov: number;
  showFps: boolean;
  minimap: boolean;
  weapons: boolean;
}

type GraphicsKeys = 'renderScale' | 'shadows' | 'viewDistance' | 'effects';

export const PRESETS: Record<Quality, Pick<Settings, GraphicsKeys>> = {
  low: { renderScale: 0.5, shadows: 'off', viewDistance: 200, effects: 'low' },
  medium: { renderScale: 0.75, shadows: 'low', viewDistance: 300, effects: 'medium' },
  high: { renderScale: 1, shadows: 'high', viewDistance: 430, effects: 'high' },
  ultra: { renderScale: 1, shadows: 'ultra', viewDistance: 600, effects: 'high' },
};

export const DEFAULT_SETTINGS: Settings = {
  quality: 'high',
  ...PRESETS.high,
  antialias: true,
  fpsLimit: 0,
  fov: 62,
  showFps: false,
  minimap: true,
  weapons: true,
};

/** Shadow map size and the area (metres either side of the car) it covers. */
export const SHADOW_SIZES: Record<Exclude<ShadowQuality, 'off'>, { map: number; extent: number }> = {
  low: { map: 1024, extent: 35 },
  high: { map: 2048, extent: 45 },
  ultra: { map: 4096, extent: 70 },
};

/** Multiplier for particle and debris counts. */
export const EFFECTS_SCALE: Record<EffectsQuality, number> = { low: 0.35, medium: 0.65, high: 1 };

const STORAGE_KEY = 'carGame.settings.v1';

function pickOne<T extends string>(value: unknown, options: readonly T[], fallback: T): T {
  return options.includes(value as T) ? (value as T) : fallback;
}

function num(value: unknown, min: number, max: number, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

export function sanitizeSettings(raw: unknown): Settings {
  const d = DEFAULT_SETTINGS;
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  return {
    quality: pickOne(r.quality, ['low', 'medium', 'high', 'ultra', 'custom'] as const, d.quality),
    renderScale: num(r.renderScale, 0.5, 1, d.renderScale),
    shadows: pickOne(r.shadows, ['off', 'low', 'high', 'ultra'] as const, d.shadows),
    viewDistance: num(r.viewDistance, 150, 600, d.viewDistance),
    effects: pickOne(r.effects, ['low', 'medium', 'high'] as const, d.effects),
    antialias: bool(r.antialias, d.antialias),
    fpsLimit: [0, 30, 60, 120].includes(r.fpsLimit as number) ? (r.fpsLimit as number) : d.fpsLimit,
    fov: num(r.fov, 50, 90, d.fov),
    showFps: bool(r.showFps, d.showFps),
    minimap: bool(r.minimap, d.minimap),
    weapons: bool(r.weapons, d.weapons),
  };
}

/** Applies a quality preset's graphics settings. */
export function applyPreset(settings: Settings, quality: Quality): void {
  Object.assign(settings, PRESETS[quality], { quality });
}

/** The preset the graphics settings currently match, or 'custom'. */
export function matchingPreset(settings: Settings): Quality | 'custom' {
  for (const q of Object.keys(PRESETS) as Quality[]) {
    const p = PRESETS[q];
    if ((Object.keys(p) as GraphicsKeys[]).every((k) => p[k] === settings[k])) return q;
  }
  return 'custom';
}

export function loadSettings(): Settings {
  try {
    const text = localStorage.getItem(STORAGE_KEY);
    if (text) return sanitizeSettings(JSON.parse(text));
  } catch {
    // Unreadable or blocked storage: use the defaults.
  }
  return { ...DEFAULT_SETTINGS };
}

export function saveSettings(settings: Settings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Saving is a convenience; the game keeps working without it.
  }
}
