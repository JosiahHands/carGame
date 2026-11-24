// What goes on inside each cast member: what they remember, what they've
// come to believe, how they feel about everyone (you included), what they
// need right now, and how their personality is drifting.
//
// Memory works like the "generative agents" research: every experience is
// written down with how important it was, and when they need to think, the
// memories that are recent, important and about the people involved come
// back first. Now and then they reflect on what they've been through, which
// turns a pile of memories into a belief ("the driver in the red car is a
// menace"), a nudge to their personality, or a new habit.
//
// They also learn, in a simple way, which places they enjoy: each visit
// either lifts their mood or doesn't, and they come back to the ones that do.
import { CAST, TRAIT_NAMES, type CastMember, type Emotion, type PlaceKind, type TraitName, type Traits } from './cast';

export interface Memory {
  /** When, in city time (seconds the city has been running). */
  t: number;
  text: string;
  /** 1 (forgettable) to 10 (life-changing). */
  importance: number;
  /** Who it's about: cast ids, or 'driver' for you. */
  about: string[];
  kind: 'saw' | 'talk' | 'heard' | 'chat' | 'insight';
}

export interface Needs {
  /** 1 is fully satisfied, 0 is desperate. */
  social: number;
  food: number;
  rest: number;
  fun: number;
}

export interface Mind {
  id: string;
  traits: Traits;
  memories: Memory[];
  /** Beliefs they've come to, newest last. */
  insights: string[];
  /** Little habits picked up along the way. */
  habits: string[];
  /** How they feel about each person (and 'driver'): -1 can't stand them .. 1 adore them. */
  feelings: Record<string, number>;
  /** How many times they've talked to each person. */
  met: Record<string, number>;
  /** How much they've learned they enjoy each kind of place, -1..1. */
  likes: Partial<Record<PlaceKind, number>>;
  needs: Needs;
  /** Overall mood, -1..1, and the feeling on their face. */
  mood: number;
  emotion: Emotion;
  /** Importance piled up since they last reflected. */
  pending: number;
  /** The driver's name, once the driver has told them. */
  driverName?: string;
}

const MAX_MEMORIES = 80;
const MAX_INSIGHTS = 6;
const MAX_HABITS = 4;

export function newMind(member: CastMember): Mind {
  const feelings: Record<string, number> = { driver: 0 };
  for (const other of CAST) if (other.id !== member.id) feelings[other.id] = member.friends.includes(other.id) ? 0.45 : 0.05;
  if (member.bond) feelings[member.bond] = 0.9;
  for (const other of CAST) if (other.bond === member.id) feelings[other.id] = 0.9;
  const likes: Partial<Record<PlaceKind, number>> = {};
  for (const kind of member.likes) likes[kind] = 0.4;
  if (member.job) likes[member.job] = Math.max(likes[member.job] ?? 0, 0.15);
  return {
    id: member.id,
    traits: { ...member.traits },
    memories: [],
    insights: [],
    habits: [],
    feelings,
    met: {},
    likes,
    needs: { social: 0.6 + Math.random() * 0.3, food: 0.5 + Math.random() * 0.4, rest: 0.8, fun: 0.5 + Math.random() * 0.4 },
    mood: 0.3,
    emotion: 'calm',
    pending: 0,
  };
}

/** Writes down an experience. Very similar recent ones are merged, so a long chase isn't fifty memories. */
export function remember(mind: Mind, now: number, text: string, importance: number, about: string[], kind: Memory['kind']): void {
  const clean = text.trim().slice(0, 160);
  if (!clean) return;
  const last = mind.memories[mind.memories.length - 1];
  if (last && last.text === clean && now - last.t < 60) {
    last.importance = Math.min(10, last.importance + 1);
    last.t = now;
    return;
  }
  mind.memories.push({ t: now, text: clean, importance: Math.max(1, Math.min(10, Math.round(importance))), about, kind });
  mind.pending += importance;
  // Forget the least worth keeping: old and unimportant.
  if (mind.memories.length > MAX_MEMORIES) {
    let worst = 0;
    let worstScore = Infinity;
    mind.memories.forEach((m, i) => {
      const score = m.importance - (now - m.t) / 600;
      if (score < worstScore) {
        worstScore = score;
        worst = i;
      }
    });
    mind.memories.splice(worst, 1);
  }
}

/**
 * The memories that come to mind: recent ones, important ones, and ones about
 * these people or mentioning these words, best first.
 */
export function recall(mind: Mind, now: number, about: string[], words: string[] = [], count = 6): Memory[] {
  const lowered = words.map((w) => w.toLowerCase()).filter((w) => w.length > 3);
  const score = (m: Memory) => {
    const recency = Math.pow(0.995, (now - m.t) / 60); // fades over a few hours of city time
    const relevance = m.about.filter((a) => about.includes(a)).length * 0.6 + lowered.filter((w) => m.text.toLowerCase().includes(w)).length * 0.3;
    return recency + m.importance / 10 + relevance;
  };
  return [...mind.memories].sort((a, b) => score(b) - score(a)).slice(0, count);
}

/** Changes how they feel about someone. */
export function feel(mind: Mind, who: string, change: number): void {
  mind.feelings[who] = Math.max(-1, Math.min(1, (mind.feelings[who] ?? 0) + change));
}

/** Nudges a trait: personalities drift slowly with experience. */
export function nudge(mind: Mind, trait: TraitName, up: boolean, amount = 0.03): void {
  mind.traits[trait] = Math.max(0.03, Math.min(0.98, mind.traits[trait] + (up ? amount : -amount)));
}

export function addInsight(mind: Mind, now: number, insight: string): void {
  const text = insight.trim().slice(0, 140);
  if (!text || mind.insights.includes(text)) return;
  mind.insights.push(text);
  if (mind.insights.length > MAX_INSIGHTS) mind.insights.shift();
  remember(mind, now, text, 7, [], 'insight');
}

export function addHabit(mind: Mind, habit: string): void {
  const text = habit.trim().slice(0, 100);
  if (!text || mind.habits.includes(text)) return;
  mind.habits.push(text);
  if (mind.habits.length > MAX_HABITS) mind.habits.shift();
}

/** Learns from a visit: a place that lifted their mood is liked a little more next time. */
export function learnPlace(mind: Mind, kind: PlaceKind, reward: number): void {
  const now = mind.likes[kind] ?? 0;
  mind.likes[kind] = Math.max(-1, Math.min(1, now + 0.25 * (reward - now)));
}

/** Needs run down over time; mood follows how well they're doing and what's happened lately. */
export function tickNeeds(mind: Mind, dt: number, doing: { social: boolean; eating: boolean; resting: boolean; fun: boolean }): void {
  const n = mind.needs;
  const minutes = dt / 60;
  n.social = clamp01(n.social + (doing.social ? 0.25 : -0.025 * (0.5 + mind.traits.energy)) * minutes);
  n.food = clamp01(n.food + (doing.eating ? 0.35 : -0.02) * minutes);
  n.rest = clamp01(n.rest + (doing.resting ? 0.15 : -0.012 * (1.5 - mind.traits.energy)) * minutes);
  n.fun = clamp01(n.fun + (doing.fun ? 0.25 : -0.02 * (0.5 + mind.traits.curiosity)) * minutes);
  const wellbeing = (n.social + n.food + n.rest + n.fun) / 4 - 0.45;
  mind.mood += (wellbeing * 1.6 - mind.mood) * Math.min(1, minutes * 0.5);
  mind.mood = Math.max(-1, Math.min(1, mind.mood));
}

/** A shock or a treat: moves the mood straight away (it settles back over time). */
export function jolt(mind: Mind, change: number, emotion: Emotion): void {
  mind.mood = Math.max(-1, Math.min(1, mind.mood + change));
  mind.emotion = emotion;
}

function clamp01(x: number): number {
  return Math.max(0, Math.min(1, x));
}

/** A trait in words, for prompts and the journal. */
export function traitWords(traits: Traits): string {
  const words: Record<TraitName, [string, string, string]> = {
    kindness: ['prickly', 'decent', 'very kind'],
    courage: ['timid', 'steady', 'fearless'],
    curiosity: ['set in their ways', 'interested', 'endlessly curious'],
    humour: ['serious', 'good-humoured', 'a joker'],
    energy: ['sleepy', 'lively', 'bursting with energy'],
  };
  return TRAIT_NAMES.map((t) => words[t][traits[t] < 0.35 ? 0 : traits[t] < 0.7 ? 1 : 2]).join(', ');
}

/** How they feel about someone, in words. */
export function feelingWord(value: number): string {
  if (value > 0.75) return 'adores';
  if (value > 0.4) return 'likes';
  if (value > 0.12) return 'is warming to';
  if (value > -0.12) return 'is neutral about';
  if (value > -0.4) return 'is wary of';
  if (value > -0.75) return 'dislikes';
  return "can't stand";
}

export function moodWord(mind: Mind): string {
  return mind.mood > 0.5 ? 'great' : mind.mood > 0.15 ? 'good' : mind.mood > -0.15 ? 'okay' : mind.mood > -0.5 ? 'low' : 'awful';
}

/** Keeps saved minds sound when loading (old saves, or a cast that changed). */
export function restoreMind(member: CastMember, saved: Partial<Mind> | undefined): Mind {
  const fresh = newMind(member);
  if (!saved || typeof saved !== 'object') return fresh;
  return {
    ...fresh,
    traits: { ...fresh.traits, ...(saved.traits ?? {}) },
    memories: Array.isArray(saved.memories) ? saved.memories.slice(-MAX_MEMORIES) : [],
    insights: Array.isArray(saved.insights) ? saved.insights.slice(-MAX_INSIGHTS) : [],
    habits: Array.isArray(saved.habits) ? saved.habits.slice(-MAX_HABITS) : [],
    feelings: { ...fresh.feelings, ...(saved.feelings ?? {}) },
    met: { ...(saved.met ?? {}) },
    likes: { ...fresh.likes, ...(saved.likes ?? {}) },
    mood: typeof saved.mood === 'number' ? saved.mood : fresh.mood,
    driverName: typeof saved.driverName === 'string' ? saved.driverName.slice(0, 24) : undefined,
  };
}
