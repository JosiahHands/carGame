// Skill chains: stylish driving scores points. Drifts, near misses,
// slipstreaming, jumps, overtakes, takedowns and boosts all add to a chain,
// and the more different skills in it, the bigger its multiplier (up to ×10).
// The chain is banked a few seconds after the last skill. A hard crash before
// then loses it all: that's the risk.
//
// Banked points become XP for the driver level (see career.ts) and refill
// some nitro.

export type SkillKind =
  | 'drift' | 'nearMiss' | 'draft' | 'air' | 'overtake' | 'takedown' | 'perfectStart'
  | 'driftBoost' | 'smash' | 'speed' | 'revenge' | 'collect';

export const SKILLS: Record<SkillKind, { label: string; color: string }> = {
  drift: { label: 'Drift', color: '#ffd23f' },
  nearMiss: { label: 'Near miss', color: '#ff70a6' },
  draft: { label: 'Slipstream', color: '#7af0ff' },
  air: { label: 'Air', color: '#9b8cff' },
  overtake: { label: 'Overtake', color: '#3bb273' },
  takedown: { label: 'Takedown', color: '#ff5a36' },
  perfectStart: { label: 'Perfect start', color: '#ffffff' },
  driftBoost: { label: 'Drift boost', color: '#4fb3ff' },
  smash: { label: 'Smash', color: '#ff9f40' },
  speed: { label: 'Speed demon', color: '#ff3b3b' },
  revenge: { label: 'Revenge', color: '#d4af37' },
  collect: { label: 'Collectible', color: '#d4af37' },
};

/** Seconds after the last skill before the chain is banked. */
export const CHAIN_TIME = 4;
const MAX_MULTIPLIER = 10;

export class SkillChain {
  /** Points in the chain so far, before the multiplier. */
  points = 0;
  /** How many skills, and which kinds, are in the chain. */
  count = 0;
  private kinds = new Set<SkillKind>();
  /** Seconds left before the chain is banked. */
  timer = 0;
  /** The latest skills, newest first, for the HUD. */
  readonly feed: { kind: SkillKind; points: number; label: string }[] = [];
  /** Called with the points when a chain is banked, and with the points lost when one is broken. */
  onBank: (points: number, count: number) => void = () => {};
  onBreak: (points: number) => void = () => {};

  /**
   * ×1 for one kind of skill, +1 for each other kind in the chain, and +0.5
   * for every 5 skills, up to ×10. Mixing it up pays.
   */
  get multiplier(): number {
    if (this.count === 0) return 1;
    return Math.min(MAX_MULTIPLIER, this.kinds.size + Math.floor(this.count / 5) * 0.5);
  }

  /** What the chain is worth now. */
  get total(): number {
    return Math.round(this.points * this.multiplier);
  }

  add(kind: SkillKind, points: number, label = SKILLS[kind].label): void {
    points = Math.round(points);
    if (points <= 0) return;
    this.points += points;
    this.count++;
    this.kinds.add(kind);
    this.timer = CHAIN_TIME;
    // A run of the same skill (slipstream ticks) shows as one growing line.
    const top = this.feed[0];
    if (top && top.kind === kind && top.label === label) top.points += points;
    else this.feed.unshift({ kind, points, label });
    this.feed.length = Math.min(this.feed.length, 5);
  }

  update(dt: number): void {
    if (this.count === 0) return;
    this.timer -= dt;
    if (this.timer <= 0) this.bank();
  }

  /** Banks the chain now (time ran out, or the event ended). */
  bank(): void {
    if (this.count === 0) return;
    const total = this.total;
    const count = this.count;
    this.clear();
    this.onBank(total, count);
  }

  /** A crash: the chain is lost. */
  break(): void {
    if (this.count === 0) return;
    const lost = this.total;
    this.clear();
    this.onBreak(lost);
  }

  clear(): void {
    this.points = 0;
    this.count = 0;
    this.kinds.clear();
    this.timer = 0;
    this.feed.length = 0;
  }
}
