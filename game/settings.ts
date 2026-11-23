// Graphics and game settings, chosen on the Settings tab of the garage.
// Saved between sessions, like the car.

export interface Settings {
  /** How sharp the picture is: 1 draws every pixel, 0.5 draws a quarter as many. */
  resolution: number;
  /** Shadow detail. Shadows are one of the slowest things to draw. */
  shadows: 'off' | 'low' | 'high' | 'ultra';
  /** How far you can see, in metres, before everything fades into the fog. */
  viewDistance: number;
  /** How much smoke, fire and debris explosions make. */
  effects: 'low' | 'medium' | 'high';
  /** Highest frames per second to draw; 0 means no limit. */
  fpsLimit: number;
  /** Smooth jagged edges. Only changes when the game starts. */
  antialias: boolean;
  fov: number;
  showFps: boolean;
  minimap: boolean;
  weapons: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  resolution: 1,
  shadows: 'high',
  viewDistance: 430,
  effects: 'high',
  fpsLimit: 0,
  antialias: true,
  fov: 62,
  showFps: false,
  minimap: true,
  weapons: true,
};

/** The quality presets. Each one sets the four graphics settings at once. */
export const PRESETS = {
  Low: { resolution: 0.5, shadows: 'off', viewDistance: 200, effects: 'low' },
  Medium: { resolution: 0.75, shadows: 'low', viewDistance: 300, effects: 'medium' },
  High: { resolution: 1, shadows: 'high', viewDistance: 430, effects: 'high' },
  Ultra: { resolution: 1, shadows: 'ultra', viewDistance: 600, effects: 'high' },
} as const;

/** Shadow map size, and how many metres around the car get shadows. */
export const SHADOW_SIZES = {
  low: { mapSize: 1024, area: 35 },
  high: { mapSize: 2048, area: 45 },
  ultra: { mapSize: 4096, area: 70 },
};

/** How many particles and pieces of debris each effects level makes, compared to High. */
export const EFFECTS_AMOUNT = { low: 0.35, medium: 0.65, high: 1 };

const STORAGE_KEY = 'carGame.settings.v1';

export function loadSettings(): Settings {
  const settings = { ...DEFAULT_SETTINGS };
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}');
    // Only keep saved values of the right type, so a broken save can't break the game.
    for (const key of Object.keys(settings) as (keyof Settings)[]) {
      if (typeof saved[key] === typeof settings[key]) (settings as Record<string, unknown>)[key] = saved[key];
    }
  } catch {
    // Unreadable or blocked storage: use the defaults.
  }
  // Keep numbers in range and words to the ones the game knows.
  const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v || min));
  settings.resolution = clamp(settings.resolution, 0.5, 1);
  settings.viewDistance = clamp(settings.viewDistance, 150, 600);
  settings.fov = clamp(settings.fov, 50, 90);
  if (![0, 30, 60, 120].includes(settings.fpsLimit)) settings.fpsLimit = 0;
  if (!['off', 'low', 'high', 'ultra'].includes(settings.shadows)) settings.shadows = DEFAULT_SETTINGS.shadows;
  if (!['low', 'medium', 'high'].includes(settings.effects)) settings.effects = DEFAULT_SETTINGS.effects;
  return settings;
}

export function saveSettings(settings: Settings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Saving is a convenience; the game keeps working without it.
  }
}
