// Career mode: 100 levels in 10 chapters, coins, cars and upgrades. Also the
// sandbox (endless levels that keep getting harder, after level 100), the
// daily challenge, and quick races (any car, any track, any difficulty).
//
// The economy, roughly: a perfect first run through all 100 levels pays about
// 230,000 coins. Every car and every upgrade costs about 270,000, so maxing
// everything out means replaying levels for 3 stars and doing dailies. Each
// time you finish the career and start again ("reset"), coins pay 25% more.
// Weapons are bought too (about 50,000 for all three, fully upgraded): a
// career starts with none, and each one unlocks further into the career.
import { DEFAULT_CONFIG, PAINT_SWATCHES, RANGES, cloneConfig, type BodyShape, type CarConfig, type SpoilerStyle, type Topper, type Tuning } from './carConfig';
import type { EventKind, EventSpec } from './event';
import { RIVAL_NAMES, type RivalSpec } from './rivals';
import { seededRandom } from '../libs/random';
import { MAX_WEAPON_LEVEL, NO_WEAPONS, STOCK_WEAPONS, type Loadout } from './weapons';
import { VENUES, type MapName } from './venues';

// ---------- cars ----------

/** A car you can buy in the career. */
export interface Chassis {
  name: string;
  body: BodyShape;
  price: number;
  /** What's special about it. */
  perk: string;
  tuning: Partial<Tuning>;
  spoiler: SpoilerStyle;
  /** Grass, rain and snow slow it down less (see CarPhysics.allTerrain). */
  allTerrain?: boolean;
}

export const CHASSIS: Chassis[] = [
  {
    name: 'Hot Hatch', body: 'hatch', price: 0, perk: 'Nimble: the sharpest steering of any car.', spoiler: 'lip',
    tuning: { horsepower: 150, topSpeed: 92, grip: 0.95, brakes: 0.9, steering: 36, drift: 5, nitroPower: 0.9, nitroSeconds: 3 },
  },
  {
    name: 'Muscle Car', body: 'muscle', price: 3000, perk: 'Big torque: lots of power, and it loves to slide.', spoiler: 'none',
    tuning: { horsepower: 340, topSpeed: 110, grip: 0.92, brakes: 0.9, steering: 28, drift: 7, nitroPower: 1.1, nitroSeconds: 3.5 },
  },
  {
    name: 'Rally Truck', body: 'pickup', price: 8000, perk: 'All-terrain: grass, rain and snow barely slow it down.', spoiler: 'none', allTerrain: true,
    tuning: { horsepower: 380, topSpeed: 114, grip: 1, brakes: 1, steering: 32, drift: 6, downforce: 0.5, nitroPower: 1, nitroSeconds: 3.5 },
  },
  {
    name: 'Sports GT', body: 'sports', price: 16000, perk: 'Balanced: quick everywhere, with a little downforce.', spoiler: 'wing',
    tuning: { horsepower: 470, topSpeed: 128, grip: 1.05, brakes: 1.1, steering: 30, drift: 5, downforce: 2, nitroPower: 1.1, nitroSeconds: 4 },
  },
  {
    name: 'Supercar', body: 'wedge', price: 40000, perk: 'Aero: big downforce, so it flies through fast corners.', spoiler: 'gt',
    tuning: { horsepower: 660, topSpeed: 150, grip: 1.12, brakes: 1.25, steering: 28, drift: 4, downforce: 4.5, nitroPower: 1.2, nitroSeconds: 4 },
  },
  {
    name: 'Hypercar', body: 'wedge', price: 100000, perk: 'Hyperdrive: monster nitro, and the fastest thing on wheels.', spoiler: 'drag',
    tuning: { horsepower: 920, topSpeed: 178, grip: 1.2, brakes: 1.4, steering: 28, drift: 4, downforce: 7, nitroPower: 1.5, nitroSeconds: 5 },
  },
];

export type UpgradeId = 'engine' | 'gearbox' | 'tyres' | 'brakes' | 'nitro' | 'aero' | 'armour';

export const UPGRADES: { id: UpgradeId; name: string; about: string; icon: string }[] = [
  { id: 'engine', name: 'Engine', about: '+8% power: faster acceleration', icon: '🔧' },
  { id: 'gearbox', name: 'Gearbox', about: '+3.5% top speed', icon: '⚙️' },
  { id: 'tyres', name: 'Tyres', about: '+3% grip: faster through corners', icon: '🛞' },
  { id: 'brakes', name: 'Brakes', about: '+6% braking: brake later', icon: '🛑' },
  { id: 'nitro', name: 'Nitro', about: '+5% boost and a longer-lasting bottle', icon: '🔥' },
  { id: 'aero', name: 'Aero', about: 'More downforce: more grip at speed', icon: '🪽' },
  { id: 'armour', name: 'Armour', about: '+12% toughness in takedowns', icon: '🛡️' },
];

/** Most upgrade levels in the career; the sandbox goes up to SANDBOX_MAX_UPGRADE. */
export const MAX_UPGRADE = 10;
export const SANDBOX_MAX_UPGRADE = 20;

/** What the next level of an upgrade costs, from level `level`. */
export function upgradeCost(level: number): number {
  return Math.round((200 * 1.4 ** level) / 10) * 10;
}

/**
 * How much an upgrade at `level` adds, in "steps": one step per level up to
 * 10, then half a step per level after that (diminishing returns).
 */
function steps(level: number): number {
  return Math.min(level, 10) + Math.max(0, level - 10) * 0.5;
}

export type Upgrades = Record<UpgradeId, number>;

export function noUpgrades(): Upgrades {
  return { engine: 0, gearbox: 0, tyres: 0, brakes: 0, nitro: 0, aero: 0, armour: 0 };
}

/** All upgrades at the same level (for rivals). */
function allUpgrades(level: number): Upgrades {
  const u = noUpgrades();
  for (const k of Object.keys(u) as UpgradeId[]) u[k] = level;
  return u;
}

// ---------- weapons ----------

export type WeaponId = 'gun' | 'rockets' | 'shockwave';

/** The weapons you can buy, in the order of the weapon slots (1, 2, 3). */
export const WEAPON_SHOP: { id: WeaponId; name: string; about: string; icon: string; price: number; unlock: number }[] = [
  { id: 'gun', name: 'Machine gun', about: 'Fast shots from the roof. Overheats if you hold the trigger too long.', icon: '🔫', price: 800, unlock: 6 },
  { id: 'rockets', name: 'Rocket pod', about: 'Rockets that blow up everything near where they land.', icon: '🚀', price: 3500, unlock: 11 },
  { id: 'shockwave', name: 'Shockwave', about: 'A blast all round the car, then a few seconds to recharge.', icon: '💥', price: 10000, unlock: 31 },
];

/** What each weapon upgrade does. */
export const WEAPON_UPGRADE_ABOUT = 'Each upgrade: +15% damage, and the gun runs cooler or it reloads faster.';

/** Each weapon's level: 0 not bought yet, 1 (bought) to MAX_WEAPON_LEVEL. */
export type Arsenal = Record<WeaponId, number>;

export function noWeapons(): Arsenal {
  return { gun: 0, rockets: 0, shockwave: 0 };
}

/** The weapons for the car, slot by slot (see Weapons.setLoadout). */
export function loadoutOf(arsenal: Arsenal): Loadout {
  return [arsenal.gun, arsenal.rockets, arsenal.shockwave];
}

/**
 * The weapons a car has: none when weapons are off (an event without them,
 * or the Weapons setting off in free roam); the career car carries what's
 * been bought (`arsenal`); any other car (null) has all three, stock.
 */
export function carLoadout(weaponsOn: boolean, arsenal: Arsenal | null): Loadout {
  if (!weaponsOn) return [...NO_WEAPONS];
  return arsenal ? loadoutOf(arsenal) : [...STOCK_WEAPONS];
}

/** What the next level of a weapon costs, from `level` (level 0 is buying it). */
export function weaponCost(id: WeaponId, level: number): number {
  const price = WEAPON_SHOP.find((w) => w.id === id)!.price;
  return level <= 0 ? price : Math.round((price * 0.35 * 1.4 ** (level - 1)) / 10) * 10;
}

/** Why a weapon can't be bought yet, or '' if it can: each unlocks once you reach its level. */
export function weaponLockedReason(save: CareerSave, id: WeaponId): string {
  const { unlock } = WEAPON_SHOP.find((w) => w.id === id)!;
  return save.mode === 'sandbox' || save.stars[unlock - 2] > 0 ? '' : `Unlocks at level ${unlock}`;
}

/** Buys a weapon, or its next upgrade. Returns false if it can't (locked, maxed or too few coins). */
export function buyWeapon(save: CareerSave, id: WeaponId): boolean {
  const level = save.arsenal[id];
  const cost = weaponCost(id, level);
  if (weaponLockedReason(save, id) || level >= MAX_WEAPON_LEVEL || save.coins < cost) return false;
  save.coins -= cost;
  save.arsenal[id] = level + 1;
  return true;
}

/** A car's tuning: the chassis, with the upgrades on top. */
export function careerTuning(chassis: number, up: Upgrades): Tuning {
  const c = CHASSIS[Math.max(0, Math.min(CHASSIS.length - 1, chassis))];
  const t: Tuning = { ...DEFAULT_CONFIG.tuning, downforce: 0, ...c.tuning, jump: 0, gravity: 1, bounce: 0.35, airControl: 0.15, infiniteNitro: false };
  t.horsepower *= 1 + 0.08 * steps(up.engine);
  t.topSpeed *= 1 + 0.035 * steps(up.gearbox);
  t.grip += 0.03 * steps(up.tyres);
  t.brakes *= 1 + 0.06 * steps(up.brakes);
  t.nitroPower *= 1 + 0.05 * steps(up.nitro);
  t.nitroSeconds += 0.3 * steps(up.nitro);
  t.downforce = Math.min(10, t.downforce + 0.4 * steps(up.aero));
  return t;
}

/** How much damage a car with this armour level can take, for EventSpec.armour. */
export function armourFor(level: number): number {
  return 100 * (1 + 0.12 * steps(level));
}

/**
 * The player's career car: the chassis and upgrades, wearing the paint and
 * parts from the garage. Size, stretch and silly physics from the garage are
 * left out, so everyone races on equal terms.
 */
export function careerCar(looks: CarConfig, chassis: number, up: Upgrades): CarConfig {
  const c = CHASSIS[chassis];
  return {
    ...looks,
    body: c.body,
    size: 1, length: 1, width: 1, height: 1,
    wheelSize: Math.min(1.2, looks.wheelSize), tireWidth: Math.min(1.5, looks.tireWidth), track: Math.min(1.2, looks.track), lift: Math.min(0.15, looks.lift),
    tuning: careerTuning(chassis, up),
  };
}

/** A rival's car: a chassis with upgrades and a random paint job. */
function rivalCar(chassis: number, upgrade: number, rand: () => number, boss = false): CarConfig {
  const pick = <T>(list: readonly T[]) => list[Math.floor(rand() * list.length)];
  const c = CHASSIS[chassis];
  const config = cloneConfig(DEFAULT_CONFIG);
  Object.assign(config, {
    body: c.body,
    spoiler: c.spoiler,
    paintColor: pick(PAINT_SWATCHES),
    accentColor: pick(PAINT_SWATCHES),
    finish: pick(['gloss', 'metallic', 'pearl', 'matte'] as const),
    stripes: pick(['none', 'racing', 'side'] as const),
    pattern: rand() < 0.2 ? pick(['twoTone', 'flames', 'checker'] as const) : 'none',
    wheelStyle: pick(['fiveSpoke', 'multiSpoke', 'dish', 'offroad'] as const),
    raceNumber: 1 + Math.floor(rand() * 99),
    roofGuns: false,
    plate: 'RIVAL',
  } satisfies Partial<CarConfig>);
  if (boss) {
    // Bosses stand out: black and gold, chrome, with a crown (or horns).
    Object.assign(config, {
      paintColor: '#0b0b0b', accentColor: '#d4af37', finish: 'chrome', stripes: 'racing', rimColor: '#d4af37',
      topper: (rand() < 0.5 ? 'crown' : 'horns') as Topper, underglow: 'on', underglowColor: '#ffd23f', raceNumber: 1, plate: 'BOSS',
    } satisfies Partial<CarConfig>);
  }
  config.tuning = careerTuning(chassis, allUpgrades(upgrade));
  return config;
}

// ---------- the save ----------

export interface CareerSave {
  coins: number;
  /** Stars (0-3) for each of the 100 levels. */
  stars: number[];
  /** Best score for each level, as shown (e.g. "1st", "1:02.33"). */
  best: string[];
  /** The raw best score for each level (a place, seconds or points; 0 for none), to tell if a new result beats it. */
  bestScore: number[];
  upgrades: Upgrades;
  /** The weapons bought, and their upgrades. */
  arsenal: Arsenal;
  /** Which cars (indices into CHASSIS) are owned, and which one is in use. */
  owned: number[];
  car: number;
  /** 'sandbox' once the player chose to carry on after level 100. */
  mode: 'career' | 'sandbox';
  /** The next sandbox level (101 and up). */
  sandboxLevel: number;
  /** Best sandbox level reached. */
  sandboxBest: number;
  /** Times the career has been finished and reset: each one pays 25% more coins. */
  completions: number;
  /**
   * Has the player chosen Sandbox or Reset after beating level 100? Until
   * they do, the choice keeps being offered (see awaitingChoice).
   */
  finished: boolean;
  /** Date of the last daily challenge done (YYYY-MM-DD). */
  daily: string;
  /** Days in a row with the daily challenge done. */
  streak: number;
  /** For the stats: events entered, won, and coins earned. */
  events: number;
  wins: number;
  earned: number;
  /** Driver XP, from skill chains and events (see rewards.ts for levels). */
  xp: number;
  /** Prize spins waiting to be spun (one per driver level). */
  spins: number;
  /** Coin rush tokens from the prize wheel: each one doubles an event's coins. */
  coinRush: number;
  /** Counts and records for the trophies: near misses, best chain, top speed... */
  stats: Record<string, number>;
  /** Trophies won (ids from rewards.ts). */
  trophies: string[];
  /** Collectibles found in free roam ("map:index"). */
  found: string[];
  /** Stars won on free-roam challenges ("map:index" -> 0..3). */
  zones: Record<string, number>;
  /** Rivals' wins over the player: the one with the most is the nemesis. */
  nemesis: Record<string, number>;
}

const SAVE_KEY = 'carGame.career.v1';
export const LEVELS = 100;

export function newCareer(completions = 0): CareerSave {
  return {
    coins: 0, stars: new Array(LEVELS).fill(0), best: new Array(LEVELS).fill(''), bestScore: new Array(LEVELS).fill(0), upgrades: noUpgrades(), arsenal: noWeapons(), owned: [0], car: 0,
    mode: 'career', sandboxLevel: LEVELS + 1, sandboxBest: 0, completions, finished: false, daily: '', streak: 0, events: 0, wins: 0, earned: 0,
    xp: 0, spins: 0, coinRush: 0, stats: {}, trophies: [], found: [], zones: {}, nemesis: {},
  };
}

export function loadCareer(): CareerSave {
  const save = newCareer();
  try {
    const raw = JSON.parse(localStorage.getItem(SAVE_KEY) ?? 'null') as Partial<CareerSave> | null;
    if (!raw || typeof raw !== 'object') return save;
    const num = (v: unknown, fallback: number, max = 1e12) => (typeof v === 'number' && Number.isFinite(v) ? Math.max(0, Math.min(max, Math.floor(v))) : fallback);
    save.coins = num(raw.coins, 0);
    if (Array.isArray(raw.stars)) save.stars = save.stars.map((_, i) => num(raw.stars![i], 0, 3));
    if (Array.isArray(raw.bestScore)) save.bestScore = save.bestScore.map((_, i) => (typeof raw.bestScore![i] === 'number' && Number.isFinite(raw.bestScore![i]) ? Math.max(0, raw.bestScore![i]) : 0));
    if (Array.isArray(raw.best)) save.best = save.best.map((_, i) => (typeof raw.best![i] === 'string' ? raw.best![i].slice(0, 20) : ''));
    for (const k of Object.keys(save.upgrades) as UpgradeId[]) save.upgrades[k] = num(raw.upgrades?.[k], 0, SANDBOX_MAX_UPGRADE);
    for (const k of Object.keys(save.arsenal) as WeaponId[]) save.arsenal[k] = num(raw.arsenal?.[k], 0, MAX_WEAPON_LEVEL);
    if (Array.isArray(raw.owned)) save.owned = [...new Set([0, ...raw.owned.filter((i) => typeof i === 'number' && CHASSIS[i])])];
    save.car = save.owned.includes(raw.car as number) ? (raw.car as number) : 0;
    save.mode = raw.mode === 'sandbox' ? 'sandbox' : 'career';
    // A save from before weapons were bought (1.2.0 and older) had all three
    // all along: it keeps the ones its progress has unlocked, at level 1.
    if (!('arsenal' in raw)) for (const w of WEAPON_SHOP) if (!weaponLockedReason(save, w.id)) save.arsenal[w.id] = 1;
    save.sandboxLevel = Math.max(LEVELS + 1, num(raw.sandboxLevel, LEVELS + 1));
    save.sandboxBest = num(raw.sandboxBest, 0);
    save.completions = num(raw.completions, 0, 1000);
    save.finished = raw.finished === true;
    save.daily = typeof raw.daily === 'string' ? raw.daily.slice(0, 10) : '';
    save.streak = num(raw.streak, 0);
    save.events = num(raw.events, 0);
    save.wins = num(raw.wins, 0);
    save.earned = num(raw.earned, 0);
    save.xp = num(raw.xp, 0);
    save.spins = num(raw.spins, 0, 1000);
    save.coinRush = num(raw.coinRush, 0, 1000);
    const numbers = (v: unknown, max: number) => {
      const out: Record<string, number> = {};
      if (v && typeof v === 'object') for (const [k, x] of Object.entries(v)) if (typeof x === 'number' && Number.isFinite(x)) out[k.slice(0, 40)] = Math.max(0, Math.min(max, x));
      return out;
    };
    const strings = (v: unknown) => (Array.isArray(v) ? [...new Set(v.filter((x): x is string => typeof x === 'string').map((x) => x.slice(0, 40)))] : []);
    save.stats = numbers(raw.stats, 1e12);
    save.trophies = strings(raw.trophies);
    save.found = strings(raw.found);
    save.zones = numbers(raw.zones, 3);
    save.nemesis = numbers(raw.nemesis, 1e6);
  } catch {
    // Unreadable save: start fresh.
  }
  return save;
}

export function saveCareer(save: CareerSave): void {
  try {
    localStorage.setItem(SAVE_KEY, JSON.stringify(save));
  } catch {
    // The game keeps working without saving.
  }
}

/** Level 100 is beaten but Sandbox or Reset hasn't been chosen yet. */
export function awaitingChoice(save: CareerSave): boolean {
  return save.stars[LEVELS - 1] > 0 && !save.finished;
}

export function totalStars(save: CareerSave): number {
  return save.stars.reduce((a, b) => a + b, 0);
}

/** The highest upgrade level allowed: 10, or 20 in the sandbox. */
export function upgradeCap(save: CareerSave): number {
  return save.mode === 'sandbox' ? SANDBOX_MAX_UPGRADE : MAX_UPGRADE;
}

/** Coins pay more after each reset. */
export function prestige(save: CareerSave): number {
  return 1 + 0.25 * save.completions;
}

// ---------- the levels ----------

interface Chapter {
  name: string;
  about: string;
  venues: MapName[];
  /** The kinds of the first nine levels. Level 10 is always the boss duel. */
  kinds: EventKind[];
  boss: string;
  /** What the rivals drive: a chassis for the first half and the second half of the chapter. */
  cars: [number, number];
}

export const CHAPTERS: Chapter[] = [
  {
    name: 'Rookie Cup', about: 'Your first races, on the Grand Prix circuit and the kart track.', venues: ['circuit', 'kart'],
    kinds: ['race', 'timeAttack', 'race', 'drift', 'race', 'speedTrap', 'elimination', 'race', 'race'], boss: 'Big Earl', cars: [0, 0],
  },
  {
    name: 'Street Kings', about: 'Smash up the city, then race the neon streets at night.', venues: ['neon', 'city'],
    kinds: ['destruction', 'race', 'stunt', 'race', 'takedown', 'drift', 'race', 'destruction', 'elimination'], boss: 'Vinnie Vega', cars: [0, 1],
  },
  {
    name: 'Speedway Showdown', about: 'Flat out on banked ovals. Nitro and nerve.', venues: ['oval'],
    kinds: ['race', 'speedTrap', 'race', 'elimination', 'timeAttack', 'race', 'takedown', 'race', 'elimination'], boss: 'Dale "Thunder" Rhodes', cars: [1, 1],
  },
  {
    name: 'Desert Heat', about: 'Long, fast canyon roads between red rock mesas.', venues: ['canyon'],
    kinds: ['race', 'timeAttack', 'drift', 'race', 'takedown', 'speedTrap', 'race', 'elimination', 'race'], boss: 'Sandstorm Sam', cars: [1, 2],
  },
  {
    name: 'Frozen Peaks', about: 'Snow, ice and mountain passes. Brake early.', venues: ['alpine'],
    kinds: ['race', 'drift', 'timeAttack', 'race', 'elimination', 'race', 'drift', 'takedown', 'race'], boss: 'Ice Queen Ingrid', cars: [2, 3],
  },
  {
    name: 'Night Riders', about: 'The city after dark. Walls close, rivals closer.', venues: ['neon', 'city'],
    kinds: ['race', 'takedown', 'destruction', 'race', 'drift', 'elimination', 'stunt', 'race', 'takedown'], boss: 'Neon Nyx', cars: [3, 3],
  },
  {
    name: 'Into the Woods', about: 'Rally stages in the rain. No barriers, just trees.', venues: ['forest'],
    kinds: ['race', 'timeAttack', 'race', 'elimination', 'drift', 'race', 'speedTrap', 'race', 'timeAttack'], boss: 'Grizzly Gus', cars: [3, 3],
  },
  {
    name: 'Kart Mayhem', about: 'Tight, twisty and packed with rivals.', venues: ['kart'],
    kinds: ['race', 'elimination', 'drift', 'race', 'takedown', 'timeAttack', 'race', 'elimination', 'race'], boss: 'Tiny Tina', cars: [3, 4],
  },
  {
    name: 'World Tour', about: 'Every venue, every kind of event. Prove you can do it all.', venues: ['circuit', 'oval', 'alpine', 'canyon', 'neon', 'forest', 'kart', 'city'],
    kinds: ['race', 'elimination', 'drift', 'takedown', 'race', 'timeAttack', 'speedTrap', 'race', 'destruction'], boss: 'The Baron', cars: [4, 4],
  },
  {
    name: 'Legends', about: 'The best drivers in the world, in the best cars. Then: The Phantom.', venues: ['circuit', 'canyon', 'alpine', 'neon', 'forest', 'oval'],
    kinds: ['race', 'elimination', 'takedown', 'timeAttack', 'race', 'drift', 'race', 'speedTrap', 'elimination'], boss: 'The Phantom', cars: [5, 5],
  },
];

/** Stars needed from a chapter's first nine levels to face its boss. */
export const BOSS_STARS = 18;

/** One level, ready to play. */
export interface Level {
  number: number;
  chapter: number;
  spec: EventSpec;
  /** Coins for 3 stars (before bonuses). */
  reward: number;
  isBoss: boolean;
}

/** Kinds that happen in the city, and kinds that need a race track. */
const CITY_KINDS: EventKind[] = ['destruction', 'stunt'];

/** Base coins for a level: grows with the level. */
function baseReward(level: number): number {
  return 100 + 20 * level;
}

/**
 * Builds level `n` (1 to 100 for the career; above 100 for the sandbox). The
 * same number always gives the same event.
 */
export function makeLevel(n: number): Level {
  if (n > LEVELS) return sandboxLevel(n);
  const chapter = Math.ceil(n / 10);
  const ch = CHAPTERS[chapter - 1];
  const index = (n - 1) % 10;
  const isBoss = index === 9;
  const kind: EventKind = isBoss ? 'duel' : ch.kinds[index];
  const rand = seededRandom(n * 7919);
  const map = pickVenue(kind, ch.venues, index);
  // Mix the main tracks with generated ones: every third level is the main track.
  const layout = index % 3 === 0 || map === 'city' ? 0 : n;
  const progress = (n - 1) / (LEVELS - 1); // 0 at level 1, 1 at level 100
  const skill = 0.8 + 0.16 * progress + (isBoss ? 0.04 : 0);
  const chassis = index < 5 ? ch.cars[0] : ch.cars[1];
  const upgrade = Math.min(MAX_UPGRADE, Math.round((chapter - 1) * 1.0 + index * 0.15));
  const spec = buildSpec({
    kind, map, layout, rand, skill, level: n,
    rivals: rivalCount(kind, chapter),
    rivalCar: () => rivalCar(chassis, upgrade, rand),
    boss: isBoss ? { name: ch.boss, car: rivalCar(Math.min(5, ch.cars[1] + (chapter >= 9 ? 0 : 1)), Math.min(MAX_UPGRADE, upgrade + 1), rand, true) } : undefined,
    night: map === 'neon' || chapter === 6 || (chapter >= 3 && rand() < 0.15),
    modifiers: chapter >= 4,
  });
  spec.title = isBoss ? `Boss: ${ch.boss}` : `${ch.name} ${index + 1}`;
  return { number: n, chapter, spec, reward: baseReward(n), isBoss };
}

/** A venue for a kind of event, from the chapter's list. */
function pickVenue(kind: EventKind, venues: MapName[], index: number): MapName {
  if (CITY_KINDS.includes(kind)) return 'city';
  const tracks = venues.filter((v) => v !== 'city');
  return tracks.length ? tracks[index % tracks.length] : 'circuit';
}

function rivalCount(kind: EventKind, chapter: number): number {
  if (kind === 'duel') return 1;
  if (kind === 'takedown') return 4 + Math.floor(chapter / 4);
  if (kind === 'elimination') return 5;
  if (kind === 'race') return Math.min(7, 4 + Math.floor(chapter / 3));
  return 0;
}

interface SpecOptions {
  kind: EventKind;
  map: MapName;
  layout: number;
  rand: () => number;
  /** Rival skill, about 0.8 to 1.05. */
  skill: number;
  /** Difficulty level, for the challenge targets (1 to 100, more in the sandbox). */
  level: number;
  rivals: number;
  rivalCar: () => CarConfig;
  boss?: { name: string; car: CarConfig };
  night: boolean;
  /** Allow random twists: no nitro, or a race with weapons. */
  modifiers: boolean;
  laps?: number;
  catchUp?: number;
}

/** Turns the choices for an event into an EventSpec, with sensible laps, times and targets. */
function buildSpec(o: SpecOptions): EventSpec {
  const v = VENUES.find((x) => x.value === o.map)!;
  const length = o.map === 'city' ? 0 : (v.generate?.length ?? 2000);
  const lapsFor = (k: number) => o.laps ?? Math.max(1, Math.round(k / Math.max(900, length)));
  const names = shuffled(RIVAL_NAMES, o.rand);
  const rivals: RivalSpec[] = [];
  if (o.boss) rivals.push({ name: o.boss.name, config: o.boss.car, skill: Math.min(1.08, o.skill) });
  for (let i = 0; i < o.rivals - (o.boss ? 1 : 0); i++) {
    // A spread of skills, from a bit below the level's to a bit above.
    rivals.push({ name: names[i], config: o.rivalCar(), skill: o.skill * (0.94 + 0.08 * (i / Math.max(1, o.rivals - 1))) });
  }
  // A random roll for the twists: under 0.12 is no nitro, over 0.8 a combat
  // race. Without twists (quick races) it's 0.5, in the middle: neither.
  const twist = o.modifiers ? o.rand() : 0.5;
  const spec: EventSpec = {
    kind: o.kind, title: '', map: o.map, layout: o.layout, laps: 1, timeLimit: 0, rivals,
    targets: [3, 2, 1], night: o.night, weapons: false, noNitro: twist < 0.12, catchUp: o.catchUp ?? 0.6,
    armour: 100, rivalArmour: 150 + 2 * o.level, rivalDamage: 1.2 + o.level * 0.03, boss: o.boss?.name,
  };
  const grow = 1 + o.level * 0.006; // challenge targets get a bit harder every level
  switch (o.kind) {
    case 'race':
      spec.laps = lapsFor(6000);
      spec.weapons = twist > 0.8; // a combat race
      break;
    case 'duel':
      spec.laps = lapsFor(6000);
      spec.targets = [1, 1, 1];
      break;
    case 'elimination':
      spec.laps = 99;
      spec.timeLimit = Math.round(Math.max(18, Math.min(32, length / 75)));
      break;
    case 'timeAttack':
      spec.laps = lapsFor(4000);
      spec.pace = { config: o.rivalCar(), skill: Math.min(1, o.skill + 0.02) };
      break;
    case 'speedTrap':
      spec.laps = o.laps ?? 2;
      spec.pace = { config: o.rivalCar(), skill: Math.min(1, o.skill + 0.02) };
      break;
    case 'takedown':
      spec.weapons = true;
      spec.timeLimit = 120;
      spec.catchUp = 0;
      spec.rivalArmour = 200 + 4 * o.level; // a rocket or two each, more later on
      spec.targets = [4, 8, 13].map((t) => Math.round(t * Math.min(1.5, grow))) as [number, number, number];
      break;
    case 'drift':
      spec.timeLimit = 90;
      spec.targets = [2000, 4000, 6500].map((t) => Math.round((t * grow) / 100) * 100) as [number, number, number];
      break;
    case 'destruction':
      spec.weapons = true;
      spec.timeLimit = 75;
      spec.targets = [300000, 700000, 1200000].map((t) => Math.round((t * grow) / 10000) * 10000) as [number, number, number];
      break;
    case 'stunt':
      spec.timeLimit = 90;
      spec.targets = [6, 12, 20].map((t) => Math.round(t * Math.min(1.5, grow))) as [number, number, number];
      break;
  }
  return spec;
}

function shuffled<T>(list: readonly T[], rand: () => number): T[] {
  const copy = [...list];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

// ---------- the sandbox: endless levels after 100 ----------

const ALL_KINDS: EventKind[] = ['race', 'race', 'race', 'elimination', 'elimination', 'timeAttack', 'takedown', 'drift', 'speedTrap', 'destruction', 'stunt'];
const SANDBOX_BOSSES = ['The Phantom', 'Nova', 'Ghost Rider', 'Mach Morgan', 'Velvet Viper', 'Captain Clutch', 'Zero', 'The Duke'];

/**
 * Sandbox level n (101 and up): a random event that gets harder with every
 * level. The rivals get more skilful (up to well past the career's best),
 * their cars get more upgrades (past the career's limit of 10), and the
 * challenge targets keep rising. Every tenth level is a boss duel.
 */
function sandboxLevel(n: number): Level {
  const rand = seededRandom(n * 104729 + 17);
  const past = n - LEVELS; // levels into the sandbox
  const isBoss = past % 10 === 0;
  const kind: EventKind = isBoss ? 'duel' : ALL_KINDS[Math.floor(rand() * ALL_KINDS.length)];
  const tracks = VENUES.map((v) => v.value).filter((m) => m !== 'city');
  const map = CITY_KINDS.includes(kind) ? 'city' : tracks[Math.floor(rand() * tracks.length)];
  const skill = Math.min(1.1, 0.96 + past * 0.003);
  const upgrade = Math.min(SANDBOX_MAX_UPGRADE, 10 + Math.floor(past / 4));
  const spec = buildSpec({
    kind, map, layout: map === 'city' ? 0 : n, rand, skill, level: n,
    rivals: rivalCount(kind, 10) + (['race', 'elimination', 'takedown'].includes(kind) ? Math.min(2, Math.floor(past / 25)) : 0),
    rivalCar: () => rivalCar(5, upgrade, rand),
    boss: isBoss ? { name: SANDBOX_BOSSES[(past / 10) % SANDBOX_BOSSES.length], car: rivalCar(5, Math.min(SANDBOX_MAX_UPGRADE, upgrade + 2), rand, true) } : undefined,
    night: map === 'neon' || rand() < 0.25,
    modifiers: true,
    catchUp: 0.4,
  });
  spec.title = isBoss ? `Sandbox boss: ${spec.boss}` : `Sandbox ${n}`;
  return { number: n, chapter: 11, spec, reward: baseReward(n), isBoss };
}

// ---------- unlocking and rewards ----------

/** Stars from a chapter's first nine levels (chapter 1 is levels 1-9). */
export function chapterStars(save: CareerSave, chapter: number): number {
  let total = 0;
  for (let i = 0; i < 9; i++) total += save.stars[(chapter - 1) * 10 + i];
  return total;
}

/** Why a level is locked, or '' if it can be played. */
export function lockedReason(save: CareerSave, n: number): string {
  if (n === 1) return '';
  if (save.stars[n - 2] === 0) return `Finish level ${n - 1} first`;
  if (n % 10 === 0) {
    const chapter = n / 10;
    const have = chapterStars(save, chapter);
    if (have < BOSS_STARS) return `Needs ${BOSS_STARS} ★ from this chapter (you have ${have})`;
  }
  return '';
}

/** The next level to play in the career: the first one without stars. */
export function nextLevel(save: CareerSave): number {
  const i = save.stars.findIndex((s) => s === 0);
  return i < 0 ? LEVELS : i + 1;
}

export interface Payout {
  coins: number;
  /** Lines explaining where the coins came from. */
  lines: string[];
  newStars: number;
  firstClear: boolean;
}

/**
 * Pays out for a finished level and records the stars. Coins: the level's
 * reward scaled by stars (and something even for no stars, so a hard level
 * still pays a little), plus bonuses the first time and for new stars.
 */
/**
 * The result: `stars`, the raw `score` and how it's shown, the finishing place
 * (0 for challenges), and whether a lower score is better (places and times).
 */
export interface LevelResult {
  stars: number;
  score: number;
  scoreText: string;
  position: number;
  lowerIsBetter: boolean;
}

export function payLevel(save: CareerSave, level: Level, result: LevelResult): Payout {
  const { stars, score, scoreText, position } = result;
  const n = level.number;
  const base = level.reward;
  const lines: string[] = [];
  let coins = Math.round(base * [0.15, 0.6, 0.8, 1][stars]);
  lines.push(stars > 0 ? `${'★'.repeat(stars)} finish: ${coins}` : `Entry: ${coins}`);
  let newStars = 0;
  let firstClear = false;
  if (n <= LEVELS) {
    const had = save.stars[n - 1];
    if (stars > had) {
      newStars = stars - had;
      const bonus = Math.round(base * 0.2 * newStars);
      coins += bonus;
      lines.push(`New stars: +${bonus}`);
      if (had === 0) {
        firstClear = true;
        const clear = Math.round(base * 0.5);
        coins += clear;
        lines.push(`First clear: +${clear}`);
      }
      save.stars[n - 1] = stars;
      save.best[n - 1] = scoreText;
      save.bestScore[n - 1] = score;
    } else if (stars === had && stars > 0) {
      // Same stars: still a new best if the score beats the old one.
      const old = save.bestScore[n - 1];
      const better = !save.best[n - 1] || old <= 0 || (result.lowerIsBetter ? score < old : score > old);
      if (better) {
        save.best[n - 1] = scoreText;
        save.bestScore[n - 1] = score;
      }
    }
  } else if (stars > 0 && n === save.sandboxLevel) {
    firstClear = true;
    const clear = Math.round(base * 0.5);
    coins += clear;
    lines.push(`Level cleared: +${clear}`);
    save.sandboxLevel++;
    save.sandboxBest = Math.max(save.sandboxBest, n);
  }
  if (save.completions > 0) {
    const extra = Math.round(coins * (prestige(save) - 1));
    coins += extra;
    lines.push(`Prestige ×${prestige(save).toFixed(2)}: +${extra}`);
  }
  coins = useCoinRush(save, coins, lines);
  save.coins += coins;
  save.earned += coins;
  save.events++;
  if (position === 1) save.wins++;
  saveCareer(save);
  return { coins, lines, newStars, firstClear };
}

/** Doubles an event's coins if there's a coin rush token from the prize wheel (and uses it). */
function useCoinRush(save: CareerSave, coins: number, lines: string[]): number {
  if (save.coinRush <= 0 || coins <= 0) return coins;
  save.coinRush--;
  lines.push(`🎰 Coin rush ×2: +${coins}`);
  return coins * 2;
}

/**
 * Starts the career again from level 1. `completed` counts it as a finished
 * career, which makes coins pay 25% more from then on.
 */
export function resetCareer(save: CareerSave, completed: boolean): CareerSave {
  const fresh = newCareer(save.completions + (completed ? 1 : 0));
  fresh.events = save.events;
  fresh.wins = save.wins;
  fresh.earned = save.earned;
  fresh.sandboxBest = save.sandboxBest;
  // The driver keeps their level, trophies and what they found in free roam.
  Object.assign(fresh, { xp: save.xp, spins: save.spins, stats: save.stats, trophies: save.trophies, found: save.found, zones: save.zones, nemesis: save.nemesis });
  saveCareer(fresh);
  return fresh;
}

// ---------- the daily challenge ----------

/** Today's date as YYYY-MM-DD, in local time. */
export function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/**
 * Today's daily challenge: the same for everyone on the same day, made from
 * the date. It's pitched at the player's progress and pays double.
 */
export function dailyLevel(save: CareerSave): Level {
  const date = today();
  const seed = Number(date.replace(/-/g, ''));
  const rand = seededRandom(seed);
  const reached = save.mode === 'sandbox' ? save.sandboxLevel : nextLevel(save);
  const kinds: EventKind[] = ['race', 'elimination', 'timeAttack', 'takedown', 'drift', 'speedTrap', 'destruction', 'stunt'];
  const kind = kinds[Math.floor(rand() * kinds.length)];
  const tracks = VENUES.map((v) => v.value).filter((m) => m !== 'city');
  const map = CITY_KINDS.includes(kind) ? 'city' : tracks[Math.floor(rand() * tracks.length)];
  const chapter = Math.min(10, Math.ceil(reached / 10));
  const ch = CHAPTERS[chapter - 1];
  const progress = Math.min(1, (reached - 1) / (LEVELS - 1));
  const spec = buildSpec({
    kind, map, layout: map === 'city' ? 0 : 500 + (seed % 997), rand, skill: 0.8 + 0.16 * progress, level: reached,
    rivals: rivalCount(kind, chapter),
    rivalCar: () => rivalCar(ch.cars[1], Math.min(MAX_UPGRADE, chapter), rand),
    night: map === 'neon' || rand() < 0.3,
    modifiers: true,
  });
  spec.title = `Daily challenge · ${date}`;
  return { number: 0, chapter, spec, reward: baseReward(reached) * 2, isBoss: false };
}

/** Pays for the daily challenge (once a day; replays pay the entry money). */
export function payDaily(save: CareerSave, level: Level, stars: number): Payout {
  const first = save.daily !== today();
  const lines: string[] = [];
  let coins = Math.round(level.reward * (first ? [0.1, 0.6, 0.8, 1][stars] : 0.1));
  lines.push(first ? `Daily ${'★'.repeat(stars) || 'entry'}: ${coins}` : `Already done today: ${coins}`);
  if (first && stars > 0) {
    // A streak of days in a row pays more, up to double after a week.
    const yesterday = new Date(Date.now() - 86400000);
    const y = `${yesterday.getFullYear()}-${String(yesterday.getMonth() + 1).padStart(2, '0')}-${String(yesterday.getDate()).padStart(2, '0')}`;
    save.streak = save.daily === y ? save.streak + 1 : 1;
    save.daily = today();
    const bonus = Math.round(coins * Math.min(1, (save.streak - 1) / 7));
    if (bonus > 0) {
      coins += bonus;
      lines.push(`${save.streak}-day streak: +${bonus}`);
    }
  }
  coins = useCoinRush(save, coins, lines);
  save.coins += coins;
  save.earned += coins;
  save.events++;
  saveCareer(save);
  return { coins, lines, newStars: 0, firstClear: first && stars > 0 };
}

// ---------- quick race ----------

export const DIFFICULTIES = [
  { label: 'Easy', skill: 0.8, power: 0.9, catchUp: 1 },
  { label: 'Medium', skill: 0.88, power: 0.97, catchUp: 0.7 },
  { label: 'Hard', skill: 0.95, power: 1, catchUp: 0.4 },
  { label: 'Pro', skill: 1.0, power: 1.04, catchUp: 0.2 },
  { label: 'Insane', skill: 1.05, power: 1.1, catchUp: 0 },
];

export interface QuickRaceOptions {
  kind: EventKind;
  map: MapName;
  /** 0 for the venue's main track, or a number for a generated one. */
  layout: number;
  laps: number;
  rivals: number;
  difficulty: number;
  /** The player's car: the rivals get the same performance, scaled by the difficulty. */
  car: CarConfig;
  /** The rivals' top speed in mph, or 0 to match the player's car (scaled by the difficulty). */
  rivalTopSpeed: number;
  night: boolean;
  weapons: boolean;
}

/** The rivals' top speeds a quick race offers, in mph (0 matches the player's car). */
export const RIVAL_TOP_SPEEDS = [0, 60, 90, 120, 150, 200, 300, 500, 1000];

/**
 * A quick race: everything chosen by the player. The rivals drive cars like
 * the player's, at the top speed the player picked: the player's own (scaled
 * by the difficulty) unless another one is chosen.
 */
export function quickRaceSpec(o: QuickRaceOptions): EventSpec {
  const d = DIFFICULTIES[o.difficulty];
  const rand = seededRandom(Date.now() % 1e9);
  const mine = o.car.tuning.topSpeed;
  const top = o.rivalTopSpeed > 0 ? o.rivalTopSpeed : mine * d.power;
  // Horsepower follows the top speed, so a rival given a faster car than
  // yours also pulls hard enough to reach it, and a slower one doesn't
  // out-accelerate you.
  const rivalTuning: Tuning = {
    ...o.car.tuning,
    horsepower: o.rivalTopSpeed > 0
      ? Math.min(RANGES.horsepower[1], Math.max(RANGES.horsepower[0], (o.car.tuning.horsepower * top) / mine))
      : o.car.tuning.horsepower * d.power,
    topSpeed: top,
    jump: 0, gravity: 1, infiniteNitro: false,
  };
  const looks = () => {
    const c = rivalCar(0, 0, rand);
    c.body = (['sports', 'muscle', 'hatch', 'pickup', 'wedge'] as const)[Math.floor(rand() * 5)];
    c.spoiler = (['none', 'lip', 'wing', 'gt'] as const)[Math.floor(rand() * 4)];
    c.tuning = { ...rivalTuning };
    return c;
  };
  const rivals = o.kind === 'duel' ? 1 : ['race', 'elimination', 'takedown'].includes(o.kind) ? o.rivals : 0;
  const level = Math.round(10 + o.difficulty * 25);
  const spec = buildSpec({
    kind: o.kind, map: o.map, layout: o.layout, rand, skill: d.skill, level, rivals, rivalCar: looks,
    boss: o.kind === 'duel' ? { name: 'The Champion', car: Object.assign(rivalCar(0, 0, rand, true), { body: 'wedge', tuning: { ...rivalTuning } }) } : undefined,
    night: o.night, modifiers: false, laps: o.laps, catchUp: d.catchUp,
  });
  if (o.weapons) spec.weapons = true;
  if (o.kind === 'elimination') spec.laps = 99;
  spec.title = `Quick ${o.kind === 'timeAttack' ? 'time attack' : o.kind === 'speedTrap' ? 'speed trap' : o.kind}`;
  return spec;
}
