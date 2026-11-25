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
  /**
   * Their life so far, in their own words: rewritten now and then as they
   * reflect, so what older memories meant stays with them after the details
   * fade. Kept for the life of the game's install.
   */
  story: string;
  /** Their talks with the driver, newest last. */
  chats: ChatLine[];
  /** What they're aiming for, in their own words (they choose it, and change it when they like). */
  goal: string;
  /** Their plan for the day, and which city day it's for. */
  plan: DayStep[];
  planDay: number;
  /** Their latest private thoughts, newest last. */
  thoughts: string[];
  /**
   * Standing decisions, in their own words ("I'm done chasing the driver",
   * "I'm going to quit my job and paint"): made freely, kept until they
   * choose to change their mind, and part of every later thought.
   */
  resolutions: string[];
}

/** One step of a day plan: from this hour, be here, doing this. */
export interface DayStep {
  hour: number;
  place: PlaceKind | 'home';
  what: string;
}

/** One exchange with the driver. */
export interface ChatLine {
  t: number;
  you: string;
  them: string;
}

const MAX_MEMORIES = 200;
const MAX_CHATS = 40;
const MAX_STORY = 700;
const MAX_INSIGHTS = 6;
const MAX_HABITS = 4;
const MAX_THOUGHTS = 8;
const MAX_RESOLUTIONS = 5;

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
    story: '',
    chats: [],
    goal: '',
    plan: [],
    planDay: -1,
    thoughts: [],
    resolutions: [],
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

/** Writes down a talk with the driver. */
export function logChat(mind: Mind, now: number, you: string, them: string): void {
  mind.chats.push({ t: now, you: you.slice(0, 300), them: them.slice(0, 300) });
  if (mind.chats.length > MAX_CHATS) mind.chats.shift();
  mind.met.driver = (mind.met.driver ?? 0) + 1;
}

/** A private thought. */
export function think(mind: Mind, thought: string): void {
  const text = thought.replace(/\s+/g, ' ').trim().slice(0, 200);
  if (!text || mind.thoughts[mind.thoughts.length - 1] === text) return;
  mind.thoughts.push(text);
  if (mind.thoughts.length > MAX_THOUGHTS) mind.thoughts.shift();
}

/** A new goal, in their words ('' keeps the old one). */
export function setGoal(mind: Mind, goal: string): void {
  const text = goal.replace(/\s+/g, ' ').trim().slice(0, 140);
  if (text.length > 3) mind.goal = text;
}

/**
 * A standing decision, made for themselves. Returns whether it's new. Only
 * they can let one go: with no room left, a new one isn't taken until they
 * drop one (the AI is told so), rather than quietly forgetting the oldest.
 */
export function resolve(mind: Mind, now: number, decision: string): boolean {
  const text = decision.replace(/\s+/g, ' ').trim().slice(0, 120);
  if (text.length < 6 || mind.resolutions.some((r) => r.toLowerCase() === text.toLowerCase())) return false;
  if (mind.resolutions.length >= MAX_RESOLUTIONS) return false;
  mind.resolutions.push(text);
  remember(mind, now, `I made up my mind: ${text}`, 8, [], 'insight');
  return true;
}

/** Changes their mind about a standing decision (by its number, from 1). Returns what they let go of. */
export function release(mind: Mind, now: number, number: number): string {
  const [text] = mind.resolutions.splice(number - 1, 1);
  if (text) remember(mind, now, `I changed my mind about "${text}"`, 7, [], 'insight');
  return text ?? '';
}

/**
 * The kinds of thing a standing decision can rule out, by the words it uses.
 * The AI reads the decisions in their own words; the built-in brain (which
 * can't) uses these so it keeps them too.
 */
export const VOW_WORDS = {
  chase: /\bchas|\bpursu/i,
  ticket: /\bticket|\bfin(e|ing)\b/i,
  work: /\bwork|\bjob\b|\bshifts?\b/i,
  mischief: /mischief|prank|trouble|vandal/i,
  text: /\btext|\bmessag|\bphone/i,
  report: /\breport|snitch|call (the )?police|call buddy/i,
  drive: /\bdriv(e|es|ing)\b|\bcars?\b/i,
  ride: /\bride|\blift|police car/i,
} as const;
export type VowKind = keyof typeof VOW_WORDS;

/** Words that make a decision a "won't" rather than a "will". */
const NOT = /\b(no|never|not|won't|wont|don't|dont|stop|stopping|quit|quitting|refuse|swear off|no more|give up|giving up|avoid|without)\b|\b(done|finished|through) (with|\w+ing)\b|n't\b/i;
/** ...unless it's a "never stop" (a will after all). */
const NEVER_STOP = /\b(never|won't|wont|not|don't|dont)\s+(stop|quit|give up)\b/i;

/** Whether they have room for another standing decision. */
export function roomToResolve(mind: Mind): boolean {
  return mind.resolutions.length < MAX_RESOLUTIONS;
}

/** "...unless they're speeding": when that's happening, the decision doesn't hold. */
const UNLESS = /\b(unless|except|apart from|but not if|but not when)\b/i;
/** "...if GUY's in the car": the decision only holds when that's happening. */
const ONLY_IF = /\b(if|when|whenever|while|as long as|so long as)\b/i;
/** "even if" changes nothing: the decision always holds. */
const EVEN_IF = /\beven (if|when|though)\b.*$/i;
/** Words too common to tell what a condition is about. */
const COMMON = new Set(['the', 'and', 'you', 'are', 'not', 'but', 'for', 'she', 'her', 'his', 'him', 'was', 'who', 'its', 'any', 'all', 'get', 'got', 'one', 'out', 'way', 'they', 'them', 'their', 'theyre', 'driver', 'drivers', 'again', 'really', 'very', 'that', 'this', 'with', 'what', 'there', 'would', 'could', 'should', 'about', 'being', 'going', 'does', 'doing', 'anyone', 'someone', 'something', 'everyone', 'actually', 'start', 'starts', 'keep', 'keeps']);

/**
 * Roughly whether a condition is happening: whether its words (the first five
 * letters of each word of three or more, names like GUY included) turn up in
 * what's going on.
 */
function happening(condition: string, context: string): boolean {
  const text = context.toLowerCase();
  const words = condition.toLowerCase().replace(/[^a-z\s]/g, '').match(/[a-z]{3,}/g) ?? [];
  return words.filter((w) => !COMMON.has(w)).some((w) => text.includes(w.slice(0, 5)));
}

/**
 * Whether one of their standing decisions rules this kind of thing out right
 * now. `context` is what's going on (why they're deciding, what the driver
 * has done, who's around): a decision with a condition ("I won't chase the
 * driver unless they're speeding") holds or not depending on it. Only the
 * part before the condition says what's ruled out.
 */
export function forbids(mind: Mind, kind: VowKind, context = ''): boolean {
  return mind.resolutions.some((decision) => {
    const r = decision.replace(EVEN_IF, '');
    const unless = UNLESS.exec(r);
    const onlyIf = unless ? null : ONLY_IF.exec(r);
    const cut = unless ?? onlyIf;
    const rule = cut ? r.slice(0, cut.index) : r;
    if (!VOW_WORDS[kind].test(rule) || !NOT.test(rule) || NEVER_STOP.test(rule)) return false;
    if (unless) return !happening(r.slice(unless.index + unless[0].length), context);
    if (onlyIf) return happening(r.slice(onlyIf.index + onlyIf[0].length), context);
    return true;
  });
}

/** Their life story, rewritten after reflecting. */
export function setStory(mind: Mind, story: string): void {
  const text = story.replace(/\s+/g, ' ').trim().slice(0, MAX_STORY);
  if (text.length > 20) mind.story = text;
}

/** Where you stand with them, from strangers to best friends. */
export type Friendship = 'enemy' | 'wary' | 'stranger' | 'acquaintance' | 'friend' | 'close friend' | 'best friend';

export function friendship(mind: Mind): Friendship {
  const like = mind.feelings.driver ?? 0;
  const talks = mind.met.driver ?? 0;
  if (like < -0.5) return 'enemy';
  if (like < -0.15) return 'wary';
  if (talks === 0) return 'stranger';
  if (like < 0.3 || talks < 3) return 'acquaintance';
  if (like < 0.55 || talks < 6) return 'friend';
  if (like < 0.8 || talks < 12) return 'close friend';
  return 'best friend';
}

/** Friends and better. */
export function isFriend(mind: Mind): boolean {
  return ['friend', 'close friend', 'best friend'].includes(friendship(mind));
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
    story: typeof saved.story === 'string' ? saved.story.slice(0, MAX_STORY) : '',
    chats: Array.isArray(saved.chats) ? saved.chats.filter((c) => c && typeof c.you === 'string' && typeof c.them === 'string').slice(-MAX_CHATS) : [],
    needs: { ...fresh.needs, ...(saved.needs ?? {}) },
    goal: typeof saved.goal === 'string' ? saved.goal.slice(0, 140) : '',
    plan: Array.isArray(saved.plan) ? saved.plan.filter((p) => p && typeof p.hour === 'number' && typeof p.place === 'string').slice(0, 8) : [],
    planDay: typeof saved.planDay === 'number' ? saved.planDay : -1,
    thoughts: Array.isArray(saved.thoughts) ? saved.thoughts.filter((t) => typeof t === 'string').slice(-MAX_THOUGHTS) : [],
    // Since 1.2.1 (older cities start without any).
    resolutions: Array.isArray(saved.resolutions) ? saved.resolutions.filter((r) => typeof r === 'string').map((r) => r.slice(0, 120)).slice(-MAX_RESOLUTIONS) : [],
  };
}
