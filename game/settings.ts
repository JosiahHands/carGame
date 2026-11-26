// Graphics and game settings, chosen on the Settings tab of the garage (and
// the map, on the Maps tab). Saved between sessions, like the car.
import { MAP_NAMES, type MapName } from './venues';

export interface Settings {
  /** How sharp the picture is: 1 draws every pixel, 0.5 draws a quarter as many. */
  resolution: number;
  /** Shadow detail. Shadows are one of the slowest things to draw. */
  shadows: 'off' | 'low' | 'high' | 'ultra';
  /** How far you can see, in meters, before everything fades into the fog. */
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
  /** Speed and size units shown in the game. */
  units: 'mph' | 'kmh';
  /** Which map to drive on. */
  map: MapName;
  /** Automatic changes gear by itself; manual is X / Z (or the D-pad). */
  transmission: 'auto' | 'manual';
  /** The game's own synth music. */
  music: boolean;
  /** A voice that calls out the countdown, overtakes and results. */
  announcer: boolean;
  /** Streaks at the edge of the screen at high speed. */
  speedLines: boolean;
  /** How many cars drive around the city in free roam. */
  traffic: 'off' | 'light' | 'normal' | 'busy';
  /** People walking on the city's sidewalks in free roam. */
  pedestrians: boolean;
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
  units: 'mph',
  map: 'city',
  transmission: 'auto',
  music: true,
  announcer: true,
  speedLines: true,
  traffic: 'normal',
  pedestrians: true,
};

/** Traffic cars for each Traffic setting. */
export const TRAFFIC_COUNTS = { off: 0, light: 16, normal: 32, busy: 56 };

/** The quality presets. Each one sets the four graphics settings at once. */
export const PRESETS = {
  Low: { resolution: 0.5, shadows: 'off', viewDistance: 200, effects: 'low' },
  Medium: { resolution: 0.75, shadows: 'low', viewDistance: 300, effects: 'medium' },
  High: { resolution: 1, shadows: 'high', viewDistance: 430, effects: 'high' },
  Ultra: { resolution: 1, shadows: 'ultra', viewDistance: 600, effects: 'high' },
} as const;

/** Shadow map size, and how many meters around the car get shadows. */
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
  if (!['mph', 'kmh'].includes(settings.units)) settings.units = DEFAULT_SETTINGS.units;
  if (!MAP_NAMES.includes(settings.map)) settings.map = DEFAULT_SETTINGS.map;
  if (!['auto', 'manual'].includes(settings.transmission)) settings.transmission = DEFAULT_SETTINGS.transmission;
  if (!(settings.traffic in TRAFFIC_COUNTS)) settings.traffic = DEFAULT_SETTINGS.traffic;
  return settings;
}

export function saveSettings(settings: Settings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Saving is a convenience; the game keeps working without it.
  }
}
