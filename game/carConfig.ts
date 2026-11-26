// Everything the player can customize about their car, plus saving, loading,
// share codes, saved builds and the ready-made showcase builds.

export type BodyShape = 'sports' | 'muscle' | 'hatch' | 'pickup' | 'van' | 'wedge';
export type PaintFinish = 'gloss' | 'metallic' | 'matte' | 'pearl' | 'chrome' | 'carbon' | 'glow' | 'rainbow';
export type Pattern = 'none' | 'twoTone' | 'flames' | 'checker' | 'camo' | 'tiger' | 'dots';
export type StripeStyle = 'none' | 'racing' | 'side';
export type WheelStyle = 'fiveSpoke' | 'multiSpoke' | 'dish' | 'offroad';
export type SpoilerStyle = 'none' | 'lip' | 'wing' | 'gt' | 'drag';
export type PartColor = 'body' | 'accent' | 'black' | 'carbon';
export type ExhaustStyle = 'single' | 'dual' | 'quad' | 'stacks';
export type Topper = 'none' | 'crown' | 'topHat' | 'cone' | 'horns' | 'sharkFin' | 'halo' | 'duck';
export type Antenna = 'none' | 'whip' | 'flag' | 'ball';
export type Underglow = 'off' | 'on' | 'rainbow';
export type BoostTrail = 'flames' | 'rainbow' | 'sparkles' | 'smoke';
export type Horn = 'classic' | 'truck' | 'clown' | 'tune' | 'train';
export type EngineSound = 'v8' | 'v12' | 'diesel' | 'electric' | 'jet' | 'moped';

/** How the car drives. Real units where there are some. */
export interface Tuning {
  horsepower: number;
  topSpeed: number; // mph
  grip: number; // × normal
  brakes: number; // × normal
  steering: number; // degrees of steering lock
  drift: number; // 1..10, how far the back steps out with the handbrake
  downforce: number; // 0..10, extra grip at speed
  nitroPower: number; // × normal
  nitroSeconds: number; // how long a full bottle lasts
  infiniteNitro: boolean;
  jump: number; // meters (0 = no jumping)
  airControl: number; // 0..1, how much you can steer in the air
  gravity: number; // × Earth
  bounce: number; // 0..1.5, how much the car bounces off walls
}

export interface CarConfig {
  // Body and size
  body: BodyShape;
  size: number; // scale of the whole car (1 = normal, 25 = over 100 ft tall)
  length: number; // body stretch, × normal
  width: number;
  height: number;
  plate: string;
  raceNumber: number; // 0 = none
  // Paint
  paintColor: string;
  finish: PaintFinish;
  accentColor: string;
  pattern: Pattern;
  stripes: StripeStyle;
  windowTint: number; // 0..1
  // Wheels and suspension
  wheelStyle: WheelStyle;
  rimColor: string;
  tireColor: string;
  wheelSize: number; // × normal
  tireWidth: number; // × normal
  track: number; // how far out the wheels sit, × normal
  camber: number; // degrees; negative tilts the tops in
  lift: number; // meters the body is raised (negative lowers it)
  // Parts
  spoiler: SpoilerStyle;
  spoilerColor: PartColor;
  exhaust: ExhaustStyle;
  topper: Topper;
  antenna: Antenna;
  bullBar: boolean;
  lightBar: boolean;
  /**
   * Show the weapons the car has on the roof (the machine gun and rocket
   * pod). Off, they fire from just above the bonnet instead.
   */
  roofGuns: boolean;
  /**
   * Which roof weapons the car has right now: set by the game when it builds
   * the car you drive (none when weapons are off, what you've bought in the
   * career), never saved. Left out, the car has none, so parked cars and
   * traffic (which can't fire) never carry guns.
   */
  armed?: { gun: boolean; rockets: boolean };
  // Lights, sound and effects
  headlightColor: string;
  underglow: Underglow;
  underglowColor: string;
  nitroColor: string;
  boostTrail: BoostTrail;
  tireSmoke: boolean;
  tireSmokeColor: string;
  horn: Horn;
  engineSound: EngineSound;
  tuning: Tuning;
}

export interface Option<T extends string> {
  value: T;
  label: string;
}

export const BODY_OPTIONS: Option<BodyShape>[] = [
  { value: 'sports', label: 'Sports' },
  { value: 'muscle', label: 'Muscle' },
  { value: 'hatch', label: 'Hatchback' },
  { value: 'pickup', label: 'Pickup' },
  { value: 'van', label: 'Van' },
  { value: 'wedge', label: 'Supercar' },
];

export const FINISH_OPTIONS: Option<PaintFinish>[] = [
  { value: 'gloss', label: 'Gloss' },
  { value: 'metallic', label: 'Metallic' },
  { value: 'matte', label: 'Matte' },
  { value: 'pearl', label: 'Pearl' },
  { value: 'chrome', label: 'Chrome' },
  { value: 'carbon', label: 'Carbon' },
  { value: 'glow', label: 'Glow' },
  { value: 'rainbow', label: 'Rainbow' },
];

export const PATTERN_OPTIONS: Option<Pattern>[] = [
  { value: 'none', label: 'None' },
  { value: 'twoTone', label: 'Two-tone' },
  { value: 'flames', label: 'Flames' },
  { value: 'checker', label: 'Checker' },
  { value: 'camo', label: 'Camo' },
  { value: 'tiger', label: 'Tiger' },
  { value: 'dots', label: 'Dots' },
];

export const STRIPE_OPTIONS: Option<StripeStyle>[] = [
  { value: 'none', label: 'None' },
  { value: 'racing', label: 'Racing' },
  { value: 'side', label: 'Side' },
];

export const WHEEL_OPTIONS: Option<WheelStyle>[] = [
  { value: 'fiveSpoke', label: '5-spoke' },
  { value: 'multiSpoke', label: 'Mesh' },
  { value: 'dish', label: 'Dish' },
  { value: 'offroad', label: 'Off-road' },
];

export const SPOILER_OPTIONS: Option<SpoilerStyle>[] = [
  { value: 'none', label: 'None' },
  { value: 'lip', label: 'Lip' },
  { value: 'wing', label: 'Wing' },
  { value: 'gt', label: 'GT wing' },
  { value: 'drag', label: 'Dragster' },
];

export const PART_COLOR_OPTIONS: Option<PartColor>[] = [
  { value: 'body', label: 'Body' },
  { value: 'accent', label: 'Accent' },
  { value: 'black', label: 'Black' },
  { value: 'carbon', label: 'Carbon' },
];

export const EXHAUST_OPTIONS: Option<ExhaustStyle>[] = [
  { value: 'single', label: 'Single' },
  { value: 'dual', label: 'Dual' },
  { value: 'quad', label: 'Quad' },
  { value: 'stacks', label: 'Truck stacks' },
];

export const TOPPER_OPTIONS: Option<Topper>[] = [
  { value: 'none', label: 'None' },
  { value: 'crown', label: 'Crown' },
  { value: 'topHat', label: 'Top hat' },
  { value: 'cone', label: 'Traffic cone' },
  { value: 'horns', label: 'Horns' },
  { value: 'sharkFin', label: 'Shark fin' },
  { value: 'halo', label: 'Halo' },
  { value: 'duck', label: 'Rubber duck' },
];

export const ANTENNA_OPTIONS: Option<Antenna>[] = [
  { value: 'none', label: 'None' },
  { value: 'whip', label: 'Whip' },
  { value: 'flag', label: 'Flag' },
  { value: 'ball', label: 'Ball' },
];

export const UNDERGLOW_OPTIONS: Option<Underglow>[] = [
  { value: 'off', label: 'Off' },
  { value: 'on', label: 'On' },
  { value: 'rainbow', label: 'Rainbow' },
];

export const TRAIL_OPTIONS: Option<BoostTrail>[] = [
  { value: 'flames', label: 'Flames' },
  { value: 'rainbow', label: 'Rainbow' },
  { value: 'sparkles', label: 'Sparkles' },
  { value: 'smoke', label: 'Smoke' },
];

export const HORN_OPTIONS: Option<Horn>[] = [
  { value: 'classic', label: 'Classic' },
  { value: 'truck', label: 'Air horn' },
  { value: 'train', label: 'Train' },
  { value: 'clown', label: 'Clown' },
  { value: 'tune', label: 'Tune' },
];

export const ENGINE_OPTIONS: Option<EngineSound>[] = [
  { value: 'v8', label: 'V8' },
  { value: 'v12', label: 'V12' },
  { value: 'diesel', label: 'Diesel' },
  { value: 'electric', label: 'Electric' },
  { value: 'jet', label: 'Jet' },
  { value: 'moped', label: 'Moped' },
];

export const PAINT_SWATCHES = [
  '#d7263d', '#f46036', '#ffd23f', '#3bb273', '#1b98e0', '#2e294e',
  '#7b2cbf', '#ff70a6', '#f2f2f2', '#9aa0a6', '#2b2d42', '#0b0b0b',
];

export const LIGHT_SWATCHES = ['#fff6e0', '#ffffff', '#cfe6ff', '#ffd27a', '#7af0ff', '#c58bff'];

export const DEFAULT_CONFIG: CarConfig = {
  body: 'sports',
  size: 1,
  length: 1,
  width: 1,
  height: 1,
  plate: 'DRIVE',
  raceNumber: 0,
  paintColor: '#d7263d',
  finish: 'metallic',
  accentColor: '#f2f2f2',
  pattern: 'none',
  stripes: 'racing',
  windowTint: 0.6,
  wheelStyle: 'fiveSpoke',
  rimColor: '#c9ccd1',
  tireColor: '#1a1a1a',
  wheelSize: 1,
  tireWidth: 1,
  track: 1,
  camber: 0,
  lift: 0,
  spoiler: 'wing',
  spoilerColor: 'carbon',
  exhaust: 'dual',
  topper: 'none',
  antenna: 'none',
  bullBar: false,
  lightBar: false,
  roofGuns: true,
  headlightColor: '#fff6e0',
  underglow: 'off',
  underglowColor: '#7af0ff',
  nitroColor: '#4fb3ff',
  boostTrail: 'flames',
  tireSmoke: true,
  tireSmokeColor: '#e8e8e8',
  horn: 'classic',
  engineSound: 'v8',
  tuning: {
    horsepower: 400,
    topSpeed: 110,
    grip: 1,
    brakes: 1,
    steering: 30,
    drift: 5,
    downforce: 0,
    nitroPower: 1,
    nitroSeconds: 3.5,
    infiniteNitro: false,
    jump: 0,
    airControl: 0.15,
    gravity: 1,
    bounce: 0.35,
  },
};

/** The allowed range of every number setting. The garage sliders use these too. */
export const RANGES: Record<string, [number, number]> = {
  size: [0.25, 25],
  length: [0.5, 3],
  width: [0.6, 2.5],
  height: [0.5, 3],
  raceNumber: [0, 99],
  windowTint: [0, 1],
  wheelSize: [0.5, 4],
  tireWidth: [0.5, 3],
  track: [0.8, 2],
  camber: [-20, 20],
  lift: [-0.08, 3],
  horsepower: [50, 10000],
  topSpeed: [20, 1000],
  grip: [0.2, 3],
  brakes: [0.3, 5],
  steering: [10, 60],
  drift: [1, 10],
  downforce: [0, 10],
  nitroPower: [0.5, 5],
  nitroSeconds: [1, 30],
  jump: [0, 30],
  airControl: [0, 1],
  gravity: [0.1, 3],
  bounce: [0, 1.5],
};

/** The allowed choices for every text setting that isn't a color or the plate. */
const CHOICES: Record<string, Option<string>[]> = {
  body: BODY_OPTIONS,
  finish: FINISH_OPTIONS,
  pattern: PATTERN_OPTIONS,
  stripes: STRIPE_OPTIONS,
  wheelStyle: WHEEL_OPTIONS,
  spoiler: SPOILER_OPTIONS,
  spoilerColor: PART_COLOR_OPTIONS,
  exhaust: EXHAUST_OPTIONS,
  topper: TOPPER_OPTIONS,
  antenna: ANTENNA_OPTIONS,
  underglow: UNDERGLOW_OPTIONS,
  boostTrail: TRAIL_OPTIONS,
  horn: HORN_OPTIONS,
  engineSound: ENGINE_OPTIONS,
};

export function cleanPlate(text: string): string {
  return text.toUpperCase().replace(/[^A-Z0-9 -]/g, '').slice(0, 8);
}

/**
 * Copies each setting from `raw` onto a copy of `defaults` if it is valid:
 * numbers are kept in range, choices must be known ones and colors must be
 * #rrggbb. Anything else keeps the default.
 */
function cleanFields<T extends object>(raw: unknown, defaults: T): T {
  const result = { ...defaults } as Record<string, unknown>;
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  for (const [key, fallback] of Object.entries(defaults)) {
    const value = r[key];
    if (typeof fallback === 'number' && typeof value === 'number' && Number.isFinite(value)) {
      const [min, max] = RANGES[key];
      result[key] = Math.min(max, Math.max(min, value));
    } else if (typeof fallback === 'boolean' && typeof value === 'boolean') {
      result[key] = value;
    } else if (typeof fallback === 'string' && typeof value === 'string') {
      if (key === 'plate') result[key] = cleanPlate(value);
      else if (CHOICES[key]) result[key] = CHOICES[key].some((o) => o.value === value) ? value : fallback;
      else if (/^#[0-9a-f]{6}$/i.test(value)) result[key] = value.toLowerCase();
    }
  }
  return result as T;
}

/**
 * Older saves scored the tuning from 1 to 10 (5 was normal) and had a ride
 * height from -1 to 1. This turns those into today's settings, so the car drives
 * the way it did before: each score goes through the old physics formula and is
 * divided by what a score of 5 gave, then scaled to today's normal value.
 */
function upgradeOldSave(r: Record<string, unknown>): void {
  const round2 = (n: number) => Math.round(n * 100) / 100;
  if (typeof r.rideHeight === 'number' && r.lift === undefined) r.lift = r.rideHeight * 0.07;
  const old = (r.tuning && typeof r.tuning === 'object' ? r.tuning : null) as Record<string, unknown> | null;
  if (!old || !('power' in old) || 'horsepower' in old) return;
  // A missing or broken score counts as the old default of 5.
  const score = (key: string) => {
    const v = old[key];
    return typeof v === 'number' && Number.isFinite(v) ? Math.min(10, Math.max(1, v)) : 5;
  };
  const normal = DEFAULT_CONFIG.tuning;
  // Old acceleration was 5 + 0.9 × power; today's grows with horsepower^0.6.
  const accel = (5 + 0.9 * score('power')) / 9.5;
  r.tuning = {
    horsepower: Math.round(normal.horsepower * accel ** (1 / 0.6)),
    topSpeed: Math.round((normal.topSpeed * (28 + 5 * score('topSpeed'))) / 53),
    grip: round2((2.5 + 0.9 * score('grip')) / 7),
    brakes: round2((12 + 2 * score('brakes')) / 22),
    steering: Math.round(((0.38 + 0.03 * score('steering')) * 180) / Math.PI),
    drift: score('drift'),
    nitroPower: round2((0.6 + 0.12 * score('nitro')) / 1.2),
    // Stronger nitro used to empty the bottle faster.
    nitroSeconds: round2((normal.nitroSeconds * 0.305) / (0.18 + 0.025 * score('nitro'))),
  };
}

/** Turns anything (old saves, share codes, hand-edited data) into a valid config. */
export function sanitizeConfig(raw: unknown): CarConfig {
  // Work on a copy so the caller's object isn't changed.
  const r = { ...(raw && typeof raw === 'object' ? raw : {}) } as Record<string, unknown>;
  upgradeOldSave(r);
  // Older saves stored the underglow as on/off.
  if (typeof r.underglow === 'boolean') r.underglow = r.underglow ? 'on' : 'off';
  const config = cleanFields(r, DEFAULT_CONFIG);
  config.tuning = cleanFields(r.tuning, DEFAULT_CONFIG.tuning);
  return config;
}

export function cloneConfig(config: CarConfig): CarConfig {
  return { ...config, tuning: { ...config.tuning } };
}

// ---------- saving ----------

const STORAGE_KEY = 'carGame.config.v1';
const BUILDS_KEY = 'carGame.builds.v1';

export function loadConfig(): CarConfig {
  try {
    const text = localStorage.getItem(STORAGE_KEY);
    if (text) return sanitizeConfig(JSON.parse(text));
  } catch {
    // Unreadable or blocked storage: fall back to the default car.
  }
  return cloneConfig(DEFAULT_CONFIG);
}

export function saveConfig(config: CarConfig): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch {
    // Saving is a convenience; the game keeps working without it.
  }
}

/** A car the player saved under a name, in the Builds tab. */
export interface SavedBuild {
  name: string;
  config: CarConfig;
}

export function loadBuilds(): SavedBuild[] {
  try {
    const list = JSON.parse(localStorage.getItem(BUILDS_KEY) ?? '[]');
    if (!Array.isArray(list)) return [];
    return list.map((b) => ({ name: String(b?.name ?? 'My car').slice(0, 30), config: sanitizeConfig(b?.config) }));
  } catch {
    return [];
  }
}

export function saveBuilds(builds: SavedBuild[]): void {
  try {
    localStorage.setItem(BUILDS_KEY, JSON.stringify(builds));
  } catch {
    // ignore
  }
}

// ---------- share codes ----------
// A share code is the car's settings that differ from the default car, as
// JSON, in base64, after "CAR1-". Paste one into the Builds tab to load the car.

export function shareCode(config: CarConfig): string {
  const changed: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(config)) {
    if (key !== 'tuning' && value !== (DEFAULT_CONFIG as unknown as Record<string, unknown>)[key]) changed[key] = value;
  }
  const tuning: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(config.tuning)) {
    if (value !== (DEFAULT_CONFIG.tuning as unknown as Record<string, unknown>)[key]) tuning[key] = value;
  }
  if (Object.keys(tuning).length > 0) changed.tuning = tuning;
  // Every value is plain ASCII (the plate only allows A-Z, 0-9, space and -), so btoa is safe.
  return 'CAR1-' + btoa(JSON.stringify(changed));
}

/** Reads a share code back into a car, or returns null if it isn't one. */
export function readShareCode(code: string): CarConfig | null {
  const match = /^CAR1-([A-Za-z0-9+/=]+)$/.exec(code.trim());
  if (!match) return null;
  try {
    const data = JSON.parse(atob(match[1]));
    // A car is always a JSON object ({} is the stock car). Anything else, like
    // null, a number or a list, isn't a share code.
    if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
    // Every key must be a car setting, so random text that happens to be JSON is refused.
    for (const key of Object.keys(data)) if (!(key in DEFAULT_CONFIG)) return null;
    return sanitizeConfig(data);
  } catch {
    return null;
  }
}

// ---------- showcase builds ----------

type BuildChanges = Partial<Omit<CarConfig, 'tuning'>> & { tuning?: Partial<Tuning> };

/** Ready-made cars that show off what the garage can do. */
export const SHOWCASE_BUILDS: { name: string; about: string; changes: BuildChanges }[] = [
  {
    name: 'Kaiju',
    about: 'A 100 ft tall muscle car that flattens buildings.',
    changes: {
      body: 'muscle', size: 22, paintColor: '#2f4f2f', finish: 'matte', pattern: 'tiger', accentColor: '#0b0b0b', stripes: 'none',
      topper: 'horns', underglow: 'on', underglowColor: '#39ff88', horn: 'train', engineSound: 'diesel', exhaust: 'stacks',
      tuning: { horsepower: 3000, topSpeed: 140 },
    },
  },
  {
    name: '500 mph',
    about: 'A low, long land-speed record car with a jet engine.',
    changes: {
      body: 'wedge', length: 1.6, width: 0.9, height: 0.8, paintColor: '#f2f2f2', finish: 'gloss', pattern: 'flames', accentColor: '#f46036',
      stripes: 'none', spoiler: 'none', engineSound: 'jet', raceNumber: 1, wheelStyle: 'dish',
      tuning: { horsepower: 10000, topSpeed: 500, grip: 2, downforce: 8, steering: 18, nitroPower: 2 },
    },
  },
  {
    name: 'Monster truck',
    about: 'Giant wheels, a big lift, and bouncy landings.',
    changes: {
      body: 'pickup', wheelSize: 3.2, tireWidth: 2.2, track: 1.4, lift: 0.5, wheelStyle: 'offroad', rimColor: '#ffd23f',
      paintColor: '#1b98e0', pattern: 'flames', accentColor: '#ffd23f', stripes: 'none', spoiler: 'none', exhaust: 'stacks',
      bullBar: true, lightBar: true, horn: 'truck', tuning: { horsepower: 900, topSpeed: 90, jump: 4, bounce: 0.8, grip: 1.3 },
    },
  },
  {
    name: 'Pocket rocket',
    about: 'A toy-sized car with a rubber duck on the roof.',
    changes: {
      body: 'hatch', size: 0.3, paintColor: '#ffd23f', pattern: 'dots', accentColor: '#1b98e0', stripes: 'none', topper: 'duck',
      antenna: 'ball', horn: 'clown', engineSound: 'moped', roofGuns: false, tuning: { horsepower: 120, topSpeed: 45, grip: 1.5 },
    },
  },
  {
    name: 'Moon buggy',
    about: 'Moon gravity, huge jumps and full control in the air.',
    changes: {
      body: 'hatch', lift: 0.4, wheelSize: 1.6, tireWidth: 1.5, wheelStyle: 'offroad', paintColor: '#f2f2f2', finish: 'matte',
      stripes: 'side', accentColor: '#1b98e0', antenna: 'flag', engineSound: 'electric', spoiler: 'none',
      tuning: { gravity: 0.17, jump: 6, airControl: 0.9, bounce: 0.9 },
    },
  },
  {
    name: 'Stretch limo',
    about: 'Three times longer, with blacked-out windows.',
    changes: {
      body: 'muscle', length: 2.8, paintColor: '#0b0b0b', finish: 'gloss', stripes: 'none', windowTint: 1, rimColor: '#f2f2f2',
      wheelStyle: 'multiSpoke', spoiler: 'none', antenna: 'whip', topper: 'topHat', horn: 'tune', engineSound: 'v12', roofGuns: false,
    },
  },
  {
    name: 'Aerial ace',
    about: 'Jump, boost and fly: steer and boost freely in the air.',
    changes: {
      body: 'wedge', paintColor: '#2f6bff', finish: 'pearl', accentColor: '#ff70a6', stripes: 'racing', topper: 'halo',
      underglow: 'rainbow', boostTrail: 'rainbow', roofGuns: false,
      tuning: { jump: 3, airControl: 1, nitroPower: 2.5, nitroSeconds: 8, horsepower: 800 },
    },
  },
  {
    name: 'Low rider',
    about: 'Slammed, wide and tilted, with rainbow neon.',
    changes: {
      body: 'muscle', lift: -0.08, camber: -14, track: 1.2, wheelSize: 0.9, paintColor: '#7b2cbf', finish: 'pearl', stripes: 'none',
      underglow: 'rainbow', spoiler: 'none', exhaust: 'quad', horn: 'tune', rimColor: '#d4af37', wheelStyle: 'multiSpoke',
      tireSmokeColor: '#ff70a6',
    },
  },
  {
    name: 'Drag racer',
    about: 'A huge wing, 3,000 hp and almost no steering.',
    changes: {
      body: 'sports', spoiler: 'drag', spoilerColor: 'accent', paintColor: '#ffd23f', accentColor: '#0b0b0b', pattern: 'checker',
      stripes: 'none', raceNumber: 7, tireWidth: 1.6, engineSound: 'v12',
      tuning: { horsepower: 3000, topSpeed: 260, grip: 1.8, steering: 14, nitroPower: 3 },
    },
  },
  {
    name: 'Rainbow',
    about: 'Everything that can be a rainbow, is.',
    changes: {
      body: 'sports', finish: 'rainbow', underglow: 'rainbow', boostTrail: 'rainbow', topper: 'crown', stripes: 'none',
      tireSmokeColor: '#ff70a6', rimColor: '#f2f2f2', horn: 'clown',
    },
  },
];

/** The default car with a showcase build's changes on top. */
export function showcaseConfig(changes: BuildChanges): CarConfig {
  const config = cloneConfig(DEFAULT_CONFIG);
  Object.assign(config, changes, { tuning: { ...DEFAULT_CONFIG.tuning, ...changes.tuning } });
  return config;
}

// ---------- random cars ----------

function pick<T>(list: readonly T[]): T {
  return list[Math.floor(Math.random() * list.length)];
}

function randomColor(): string {
  const h = Math.random();
  const s = 0.5 + Math.random() * 0.5;
  const l = 0.25 + Math.random() * 0.45;
  // HSL -> hex
  const f = (n: number) => {
    const k = (n + h * 12) % 12;
    const a = s * Math.min(l, 1 - l);
    const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(c * 255).toString(16).padStart(2, '0');
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

/**
 * A new look for the car: body, paint, wheels and parts. Size and tuning are
 * kept, so this is also used for the normal-sized parked cars around the city.
 */
export function randomConfig(current: CarConfig): CarConfig {
  const sometimes = <T>(chance: number, value: T, otherwise: T) => (Math.random() < chance ? value : otherwise);
  return {
    ...current,
    body: pick(BODY_OPTIONS).value,
    paintColor: Math.random() < 0.5 ? pick(PAINT_SWATCHES) : randomColor(),
    // Rainbow paint only changes color while it is being driven, so parked cars don't get it.
    finish: pick(FINISH_OPTIONS.filter((o) => o.value !== 'rainbow')).value,
    accentColor: Math.random() < 0.5 ? pick(PAINT_SWATCHES) : randomColor(),
    pattern: sometimes(0.35, pick(PATTERN_OPTIONS).value, 'none'),
    stripes: pick(STRIPE_OPTIONS).value,
    windowTint: Math.round(Math.random() * 10) / 10,
    wheelStyle: pick(WHEEL_OPTIONS).value,
    rimColor: pick(['#c9ccd1', '#1a1a1a', '#d4af37', '#ffffff', randomColor()]),
    tireColor: sometimes(0.1, randomColor(), '#1a1a1a'),
    wheelSize: 0.85 + Math.round(Math.random() * 8) * 0.05,
    tireWidth: 0.8 + Math.round(Math.random() * 6) * 0.1,
    spoiler: pick(SPOILER_OPTIONS.filter((o) => o.value !== 'drag')).value,
    spoilerColor: pick(PART_COLOR_OPTIONS).value,
    exhaust: pick(EXHAUST_OPTIONS).value,
    topper: sometimes(0.15, pick(TOPPER_OPTIONS).value, 'none'),
    antenna: sometimes(0.15, pick(ANTENNA_OPTIONS).value, 'none'),
    raceNumber: sometimes(0.2, 1 + Math.floor(Math.random() * 99), 0),
    headlightColor: pick(LIGHT_SWATCHES),
    underglow: sometimes(0.4, 'on', 'off'),
    underglowColor: randomColor(),
    nitroColor: randomColor(),
    tuning: { ...current.tuning },
  };
}
