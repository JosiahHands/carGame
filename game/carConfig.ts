// Everything the player can customize about their car, plus saving/loading it.

export type BodyShape = 'sports' | 'muscle' | 'hatch' | 'pickup';
export type PaintFinish = 'gloss' | 'metallic' | 'matte' | 'pearl' | 'chrome';
export type StripeStyle = 'none' | 'racing' | 'side';
export type WheelStyle = 'fiveSpoke' | 'multiSpoke' | 'dish' | 'offroad';
export type SpoilerStyle = 'none' | 'lip' | 'wing' | 'gt';
export type PartColor = 'body' | 'accent' | 'black' | 'carbon';
export type ExhaustStyle = 'single' | 'dual' | 'quad';

export interface Tuning {
  power: number; // 1..10 acceleration
  topSpeed: number; // 1..10
  grip: number; // 1..10 cornering grip
  brakes: number; // 1..10
  steering: number; // 1..10 steering lock
  drift: number; // 1..10 how loose the handbrake makes the rear
  nitro: number; // 1..10 nitro boost strength and bottle size
}

export interface CarConfig {
  body: BodyShape;
  paintColor: string;
  finish: PaintFinish;
  accentColor: string;
  stripes: StripeStyle;
  windowTint: number; // 0..1
  wheelStyle: WheelStyle;
  rimColor: string;
  wheelSize: number; // 0.85..1.25
  tireWidth: number; // 0.8..1.4
  rideHeight: number; // -1..1
  spoiler: SpoilerStyle;
  spoilerColor: PartColor;
  exhaust: ExhaustStyle;
  headlightColor: string;
  underglow: boolean;
  underglowColor: string;
  nitroColor: string;
  /** Show the machine gun and rocket pod on the roof. */
  roofGuns: boolean;
  plate: string;
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
];

export const FINISH_OPTIONS: Option<PaintFinish>[] = [
  { value: 'gloss', label: 'Gloss' },
  { value: 'metallic', label: 'Metallic' },
  { value: 'matte', label: 'Matte' },
  { value: 'pearl', label: 'Pearl' },
  { value: 'chrome', label: 'Chrome' },
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
];

export const PAINT_SWATCHES = [
  '#d7263d', '#f46036', '#ffd23f', '#3bb273', '#1b98e0', '#2e294e',
  '#7b2cbf', '#ff70a6', '#f2f2f2', '#9aa0a6', '#2b2d42', '#0b0b0b',
];

export const LIGHT_SWATCHES = ['#fff6e0', '#ffffff', '#cfe6ff', '#ffd27a', '#7af0ff', '#c58bff'];

export const TUNING_LABELS: Record<keyof Tuning, string> = {
  power: 'Power',
  topSpeed: 'Top speed',
  grip: 'Grip',
  brakes: 'Brakes',
  steering: 'Steering',
  drift: 'Drift',
  nitro: 'Nitro',
};

export const DEFAULT_CONFIG: CarConfig = {
  body: 'sports',
  paintColor: '#d7263d',
  finish: 'metallic',
  accentColor: '#f2f2f2',
  stripes: 'racing',
  windowTint: 0.6,
  wheelStyle: 'fiveSpoke',
  rimColor: '#c9ccd1',
  wheelSize: 1,
  tireWidth: 1,
  rideHeight: 0,
  spoiler: 'wing',
  spoilerColor: 'carbon',
  exhaust: 'dual',
  headlightColor: '#fff6e0',
  underglow: false,
  underglowColor: '#7af0ff',
  nitroColor: '#4fb3ff',
  roofGuns: true,
  plate: 'DRIVE',
  tuning: { power: 5, topSpeed: 5, grip: 5, brakes: 5, steering: 5, drift: 5, nitro: 5 },
};

const STORAGE_KEY = 'carGame.config.v1';

function oneOf<T extends string>(value: unknown, options: Option<T>[], fallback: T): T {
  return options.some((o) => o.value === value) ? (value as T) : fallback;
}

function color(value: unknown, fallback: string): string {
  return typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value) ? value.toLowerCase() : fallback;
}

function num(value: unknown, min: number, max: number, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
}

export function cleanPlate(text: string): string {
  return text.toUpperCase().replace(/[^A-Z0-9 -]/g, '').slice(0, 8);
}

/** Turns anything (old saves, hand-edited data) into a valid config. */
export function sanitizeConfig(raw: unknown): CarConfig {
  const d = DEFAULT_CONFIG;
  const r = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const t = (r.tuning && typeof r.tuning === 'object' ? r.tuning : {}) as Record<string, unknown>;
  const tuning = {} as Tuning;
  for (const key of Object.keys(d.tuning) as (keyof Tuning)[]) {
    tuning[key] = Math.round(num(t[key], 1, 10, d.tuning[key]));
  }
  return {
    body: oneOf(r.body, BODY_OPTIONS, d.body),
    paintColor: color(r.paintColor, d.paintColor),
    finish: oneOf(r.finish, FINISH_OPTIONS, d.finish),
    accentColor: color(r.accentColor, d.accentColor),
    stripes: oneOf(r.stripes, STRIPE_OPTIONS, d.stripes),
    windowTint: num(r.windowTint, 0, 1, d.windowTint),
    wheelStyle: oneOf(r.wheelStyle, WHEEL_OPTIONS, d.wheelStyle),
    rimColor: color(r.rimColor, d.rimColor),
    wheelSize: num(r.wheelSize, 0.85, 1.25, d.wheelSize),
    tireWidth: num(r.tireWidth, 0.8, 1.4, d.tireWidth),
    rideHeight: num(r.rideHeight, -1, 1, d.rideHeight),
    spoiler: oneOf(r.spoiler, SPOILER_OPTIONS, d.spoiler),
    spoilerColor: oneOf(r.spoilerColor, PART_COLOR_OPTIONS, d.spoilerColor),
    exhaust: oneOf(r.exhaust, EXHAUST_OPTIONS, d.exhaust),
    headlightColor: color(r.headlightColor, d.headlightColor),
    underglow: typeof r.underglow === 'boolean' ? r.underglow : d.underglow,
    underglowColor: color(r.underglowColor, d.underglowColor),
    nitroColor: color(r.nitroColor, d.nitroColor),
    roofGuns: typeof r.roofGuns === 'boolean' ? r.roofGuns : d.roofGuns,
    plate: typeof r.plate === 'string' ? cleanPlate(r.plate) : d.plate,
    tuning,
  };
}

export function cloneConfig(config: CarConfig): CarConfig {
  return { ...config, tuning: { ...config.tuning } };
}

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

export function randomConfig(current: CarConfig): CarConfig {
  return {
    ...current,
    body: pick(BODY_OPTIONS).value,
    paintColor: Math.random() < 0.5 ? pick(PAINT_SWATCHES) : randomColor(),
    finish: pick(FINISH_OPTIONS).value,
    accentColor: Math.random() < 0.5 ? pick(PAINT_SWATCHES) : randomColor(),
    stripes: pick(STRIPE_OPTIONS).value,
    windowTint: Math.round(Math.random() * 10) / 10,
    wheelStyle: pick(WHEEL_OPTIONS).value,
    rimColor: pick(['#c9ccd1', '#1a1a1a', '#d4af37', '#ffffff', randomColor()]),
    wheelSize: 0.85 + Math.round(Math.random() * 8) * 0.05,
    tireWidth: 0.8 + Math.round(Math.random() * 6) * 0.1,
    rideHeight: Math.round((Math.random() * 2 - 1) * 10) / 10,
    spoiler: pick(SPOILER_OPTIONS).value,
    spoilerColor: pick(PART_COLOR_OPTIONS).value,
    exhaust: pick(EXHAUST_OPTIONS).value,
    headlightColor: pick(LIGHT_SWATCHES),
    underglow: Math.random() < 0.4,
    underglowColor: randomColor(),
    nitroColor: randomColor(),
    tuning: { ...current.tuning },
  };
}
