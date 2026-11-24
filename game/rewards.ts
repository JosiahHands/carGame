// Rewards on top of the career: driver XP and levels, the prize wheel you
// spin at every new level, and trophies for milestones. All of it lives in
// the career save (see CareerSave in career.ts).
import { CHASSIS, LEVELS, UPGRADES, prestige, saveCareer, upgradeCap, type CareerSave } from './career';

/** How many gold wrenches and free-roam challenge stars there are (see freeRoam.ts). */
export const WRENCHES = 68;
export const ZONE_STARS = 78;

// ---------- stats ----------

/** Adds to a counted stat (near misses, takedowns...). */
export function bump(save: CareerSave, key: string, amount = 1): void {
  save.stats[key] = (save.stats[key] ?? 0) + amount;
}

/** Keeps the best value of a record stat (top speed, best chain...). */
export function record(save: CareerSave, key: string, value: number): void {
  if (value > (save.stats[key] ?? 0)) save.stats[key] = value;
}

// ---------- driver level ----------

/** XP needed to go from level `level` to the next: it grows as you go. */
export function xpForLevel(level: number): number {
  return 5000 + 2500 * (level - 1);
}

/** The driver level for an amount of XP, and progress through it (0..1). */
export function driverLevel(xp: number): { level: number; into: number; need: number } {
  let level = 1;
  let left = xp;
  while (left >= xpForLevel(level)) {
    left -= xpForLevel(level);
    level++;
  }
  return { level, into: left, need: xpForLevel(level) };
}

/** Adds XP. Each new level earns a prize spin. Returns how many levels were gained. */
export function addXp(save: CareerSave, xp: number): number {
  const before = driverLevel(save.xp).level;
  save.xp += Math.max(0, Math.round(xp));
  const gained = driverLevel(save.xp).level - before;
  save.spins += gained;
  return gained;
}

// ---------- the prize wheel ----------

export interface Prize {
  label: string;
  color: string;
  /** How likely, compared to the others. */
  weight: number;
  coins?: number;
  coinRush?: number;
  upgrade?: boolean;
}

export const PRIZES: Prize[] = [
  { label: '500', color: '#3a4256', weight: 22, coins: 500 },
  { label: 'Coin rush', color: '#7b2cbf', weight: 14, coinRush: 1 },
  { label: '1,500', color: '#1b98e0', weight: 18, coins: 1500 },
  { label: 'Free upgrade', color: '#3bb273', weight: 12, upgrade: true },
  { label: '3,000', color: '#f46036', weight: 12, coins: 3000 },
  { label: '2× Coin rush', color: '#c62f9e', weight: 8, coinRush: 2 },
  { label: '6,000', color: '#d7263d', weight: 8, coins: 6000 },
  { label: 'JACKPOT', color: '#d4af37', weight: 3, coins: 25000 },
];

/**
 * Spins the wheel: picks a prize by weight, gives it, and returns which one
 * (an index into PRIZES) and a line saying what was won.
 */
export function spinWheel(save: CareerSave): { index: number; text: string } {
  const total = PRIZES.reduce((a, p) => a + p.weight, 0);
  let roll = Math.random() * total;
  let index = 0;
  while (roll >= PRIZES[index].weight) roll -= PRIZES[index++].weight;
  const prize = PRIZES[index];
  save.spins = Math.max(0, save.spins - 1);
  bump(save, 'spins');
  let text = '';
  if (prize.coins) {
    const coins = Math.round(prize.coins * prestige(save));
    save.coins += coins;
    save.earned += coins;
    text = `${prize.label === 'JACKPOT' ? 'JACKPOT! ' : ''}+🪙 ${coins.toLocaleString()}`;
  } else if (prize.coinRush) {
    save.coinRush += prize.coinRush;
    text = `Coin rush ×${prize.coinRush}: your next ${prize.coinRush === 1 ? 'event pays' : `${prize.coinRush} events pay`} double`;
  } else {
    // A free level on a random upgrade that isn't maxed out, or coins if they all are.
    const open = UPGRADES.filter((u) => save.upgrades[u.id] < upgradeCap(save));
    if (open.length) {
      const u = open[Math.floor(Math.random() * open.length)];
      save.upgrades[u.id]++;
      text = `Free upgrade: ${u.icon} ${u.name} → level ${save.upgrades[u.id]}`;
    } else {
      save.coins += 5000;
      text = 'Everything is maxed: +🪙 5,000 instead';
    }
  }
  saveCareer(save);
  return { index, text };
}

// ---------- trophies ----------

export interface Trophy {
  id: string;
  name: string;
  about: string;
  icon: string;
  /** Coins for winning it. */
  reward: number;
  /** How far along it is, and what it needs. */
  value: (save: CareerSave) => number;
  need: number;
}

const stat = (key: string) => (save: CareerSave) => save.stats[key] ?? 0;
const bosses = (save: CareerSave) => save.stars.filter((s, i) => s > 0 && (i + 1) % 10 === 0).length;
const maxUpgrade = (save: CareerSave) => Math.max(...Object.values(save.upgrades));

/** Makes bronze, silver and gold trophies for one stat. */
function tiers(id: string, name: string, about: (n: number) => string, icon: string, value: Trophy['value'], needs: number[], rewards: number[]): Trophy[] {
  const medals = ['🥉', '🥈', '🥇'];
  return needs.map((need, i) => ({
    id: `${id}${i + 1}`, name: needs.length > 1 ? `${name} ${['I', 'II', 'III'][i]}` : name, about: about(need), icon: needs.length > 1 ? `${medals[i]}${icon}` : icon, reward: rewards[i], value, need,
  }));
}

export const TROPHIES: Trophy[] = [
  ...tiers('win', 'Winner', (n) => `Win ${n} race${n === 1 ? '' : 's'}`, '🏁', (s) => s.wins, [1, 10, 50], [500, 2000, 8000]),
  ...tiers('near', 'Close shave', (n) => `${n} near misses`, '😬', stat('nearMiss'), [10, 100, 500], [500, 2500, 8000]),
  ...tiers('takedown', 'Wrecking crew', (n) => `${n} takedowns`, '💥', stat('takedown'), [10, 100, 500], [500, 2500, 8000]),
  ...tiers('overtake', 'Coming through', (n) => `${n} overtakes`, '⏩', stat('overtake'), [25, 250, 1000], [500, 2500, 8000]),
  ...tiers('start', 'Launch control', (n) => `${n} perfect starts`, '🚀', stat('perfectStart'), [5, 25, 100], [500, 2000, 6000]),
  ...tiers('driftBoost', 'Mini-turbo', (n) => `${n} drift boosts`, '🌀', stat('driftBoost'), [10, 100, 500], [500, 2000, 6000]),
  ...tiers('draft', 'Slipstreamer', (n) => `${n} seconds in a slipstream`, '💨', stat('draft'), [60, 600, 3000], [500, 2000, 6000]),
  ...tiers('air', 'Frequent flyer', (n) => `${n} seconds in the air`, '🪂', stat('air'), [30, 300, 1500], [500, 2000, 6000]),
  ...tiers('chain', 'Chain reaction', (n) => `Bank a ${n.toLocaleString()}-point skill chain`, '⛓️', stat('bestChain'), [10000, 50000, 200000], [1000, 4000, 12000]),
  ...tiers('speed', 'Speed freak', (n) => `Reach ${n} mph`, '⚡', stat('topSpeed'), [120, 200, 400], [500, 2000, 6000]),
  ...tiers('boss', 'Giant slayer', (n) => `Beat ${n} boss${n === 1 ? '' : 'es'}`, '👑', bosses, [1, 5, 10], [1000, 5000, 15000]),
  ...tiers('perfect', 'Perfectionist', (n) => `3 stars on ${n} levels`, '⭐', (s) => s.stars.filter((x) => x === 3).length, [10, 50, LEVELS], [1000, 6000, 25000]),
  ...tiers('cars', 'Collector', (n) => `Own ${n} cars`, '🚗', (s) => s.owned.length, [3, CHASSIS.length], [1000, 10000]),
  ...tiers('maxed', 'Fully loaded', () => 'Max out an upgrade', '🔧', maxUpgrade, [10], [3000]),
  ...tiers('level', 'Driver level', (n) => `Reach driver level ${n}`, '🎖️', (s) => driverLevel(s.xp).level, [5, 15, 30], [1000, 5000, 15000]),
  ...tiers('found', 'Treasure hunter', (n) => `Find ${n} gold wrenches in free roam`, '🔧', (s) => s.found.length, [10, 30, WRENCHES], [1000, 5000, 20000]),
  ...tiers('zones', 'Explorer', (n) => `${n} stars on free-roam challenges`, '🗺️', (s) => Object.values(s.zones).reduce((a, b) => a + b, 0), [10, 40, ZONE_STARS], [1000, 5000, 15000]),
  ...tiers('daily', 'Regular', (n) => `A ${n}-day daily streak`, '📅', (s) => s.streak, [3, 7, 30], [1000, 3000, 15000]),
  ...tiers('sandbox', 'No limits', (n) => `Clear sandbox level ${100 + n}`, '∞', (s) => Math.max(0, s.sandboxBest - LEVELS), [10, 50, 100], [5000, 20000, 50000]),
  ...tiers('legend', 'Legend', () => 'Finish the career', '🏆', (s) => s.stars[LEVELS - 1] > 0 ? 1 : 0, [1], [25000]),
  ...tiers('spins', 'High roller', (n) => `Spin the prize wheel ${n} times`, '🎰', stat('spins'), [10, 50], [1000, 5000]),
];

/**
 * Awards any trophies just won: adds their coins and records them. Returns
 * the new ones, for the game to announce.
 */
export function checkTrophies(save: CareerSave): Trophy[] {
  const won: Trophy[] = [];
  for (const t of TROPHIES) {
    if (save.trophies.includes(t.id) || t.value(save) < t.need) continue;
    save.trophies.push(t.id);
    save.coins += t.reward;
    save.earned += t.reward;
    won.push(t);
  }
  if (won.length) saveCareer(save);
  return won;
}
