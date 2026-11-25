// 🧠 Living City: a whole city of people with names, jobs, homes, friends
// and minds of their own. They live through the city's days: they get up,
// grab a coffee, go to work, have lunch, meet friends in the evening and go
// home to sleep. They stop to talk when they run into each other, gossip
// about each other and about you, make plans to meet again (and keep them),
// remember what they've seen, and slowly change: forming beliefs, picking up
// habits, growing braver or warier, making friends and holding grudges.
//
// What they say and think is written by a free, open-source AI model that
// runs on this computer (see main.ts): nothing is sent anywhere. Until it's
// downloaded (or when it's busy or switched off) a simpler built-in brain
// keeps them going.
//
// Click on anyone (or pull up next to them and press K) to talk to them.
// Talk to people often and treat them well and they become your friends:
// they wave when you drive by, greet you by name and remember your talks.
// Everything they remember is saved in the game's data folder, which
// updates leave alone, so it lasts as long as the game is installed.
//
// GUY, the cheerful bank teller in the blue shirt, and his best friend
// BUDDY, a police officer who goes everywhere with him (and writes you
// tickets), are among them. L opens the city journal.
//
// They're free: each has a goal and a plan for the day, private thoughts
// (💭), and chooses for themselves what to do next, from going for lunch to
// driving across town, giving a friend a lift, getting up to mischief or
// texting you. BUDDY drives a police car (GUY rides along), and if he
// decides you've gone too far he chases you: siren, ramming, roof gun.
// Lose him to escape, or pull over and take the fine. Friends hail you for
// rides, and on Saturday nights the whole city parties in the park.
//
// The parts: cast.ts and residents.ts (who they are), mind.ts (memory,
// feelings, needs, friendship), brain.ts (what they say), city.ts (places,
// homes and walking routes), body.ts (how they look and move), labels.ts
// (name tags and speech bubbles), ui.ts (chat, journal and settings),
// drive.ts (people's cars and routes on the roads), police.ts (BUDDY's
// police car), and main.ts (the main-process side: the local AI and the
// saved city).
import * as THREE from 'three';
import type { CarConfig } from '../../game/carConfig';
import type { GameApi, Pack, PackInstance } from '../../game/packs';
import { el } from '../../libs/dom';
import { Body, type Action } from './body';
import { LocalBrain, RuleBrain, type Brain, type ConversationResult, type Decision, type Line, type MeetPlan, type Moment, type Person, type Situation } from './brain';
import { CAST, type CastMember, type Emotion, type PlaceKind } from './cast';
import { CityMap, type Place } from './city';
import { NpcCar, parkingNear, routeTo, type Obstacle } from './drive';
import { Label } from './labels';
import {
  addHabit,
  addInsight,
  feel,
  feelingWord,
  friendship,
  isFriend,
  jolt,
  learnPlace,
  logChat,
  nudge,
  remember,
  restoreMind,
  setGoal,
  setStory,
  think as addThought,
  tickNeeds,
  type Friendship,
  type Mind,
} from './mind';
import { MODELS, type AiStatus, type ModelChoice } from './protocol';
import { PoliceCar } from './police';
import { aiWords, buildChat, buildJournal, DEFAULT_OPTIONS, settingsPanel, type Message, type Options, type Talk } from './ui';

/** Two people's plan to meet: where, and when (city clock). */
interface Meetup {
  ids: [string, string];
  placeId: string;
  kind: PlaceKind;
  at: number;
}

/** What gets saved: the city's clock, every mind, plans, the latest news and talk, and your record with the police. */
interface Saved {
  version: 2 | 3;
  /** When it was saved (real time), so the newest copy wins. */
  savedAt: number;
  clock: number;
  minds: Record<string, Mind>;
  tickets: number;
  meetups: Meetup[];
  news: string[];
  talks: Talk[];
  /** Since version 3: how wanted you are (0..5), what for, your chases, and the messages you've been sent. */
  heat?: number;
  crimes?: string[];
  record?: PoliceRecord;
  messages?: Message[];
}

/** Your record with BUDDY. */
interface PoliceRecord {
  chases: number;
  escapes: number;
  busts: number;
  /** Coins paid in fines. */
  fines: number;
}

/** A car journey: the driver (and anyone riding along) walks to a car at the kerb, drives, parks and gets out. */
interface Trip {
  driver: Npc;
  riders: Npc[];
  /** The car: someone's own, or null for BUDDY's police car. */
  car: NpcCar | null;
  phase: 'walk' | 'drive';
  /** Where they get in, on the pavement by the car. */
  door: { x: number; z: number };
  /** Where each of them is going once they're out (null: BUDDY, back to GUY's side). */
  dests: Map<Npc, Place | 'home' | null>;
  /** Where the car's going. */
  to: Place | 'home';
  started: number;
}

/** BUDDY chasing you. */
interface Chase {
  started: number;
  /** Seconds you've been out of his sight (8 and you've lost him). */
  lost: number;
  /** Seconds you've been stopped beside him (3 and you're busted). */
  stopped: number;
  /** How shot up and rammed your car is (100: you can't go on). */
  damage: number;
  /** Who's in the police car. */
  crew: Npc[];
  /** People who saw it go past (each remembers it once). */
  seen: Set<Npc>;
}

/** What someone's doing: heading somewhere, spending time there, wandering, going home, asleep, or (BUDDY) sticking with GUY. */
type Plan =
  | { kind: 'go'; place: Place; spot: THREE.Vector2; meet?: Meetup }
  | { kind: 'stay'; place: Place; until: number; meet?: Meetup }
  | { kind: 'wander'; until: number }
  | { kind: 'homeward'; nap?: boolean }
  | { kind: 'sleep'; until: number }
  /** Sticking with someone: BUDDY with GUY, or anyone looking for, tagging along with, or (BUDDY) going after someone. */
  | { kind: 'follow'; who: string; until?: number; why?: 'talk' | 'tag' | 'arrest' }
  /** On a car trip (see Trip). */
  | { kind: 'trip' }
  /** Standing still a moment, making up their mind. */
  | { kind: 'pause'; until: number }
  /** Riding in your car, to a place. */
  | { kind: 'ride'; place: Place };

export interface Npc {
  member: CastMember;
  mind: Mind;
  body: Body;
  label: Label;
  x: number;
  z: number;
  heading: number;
  path: THREE.Vector2[];
  plan: Plan;
  /** Their front door (set once the city is built). */
  home: { x: number; z: number } | null;
  asleep: boolean;
  /** A reaction that overrides walking for a moment (diving, cheering, waving). */
  reaction: Action | null;
  reactionUntil: number;
  /** Which way a dive throws them. */
  diveX: number;
  diveZ: number;
  /** Something they're looking at, if anything. */
  lookX: number | null;
  lookZ: number;
  convo: Convo | null;
  emotionUntil: number;
  /** When they last called out, reacted to something scary, reflected, or greeted you. */
  lastShout: number;
  lastScare: number;
  lastReflect: number;
  lastGreet: number;
  reflecting: boolean;
  /** Their mood when they arrived where they are, to learn whether they like it. */
  visit: { kind: PlaceKind; mood: number; talked: boolean } | null;
  chatting: boolean;
  /** Where they stood with you last time it was checked (to tell you when you become friends). */
  bond: Friendship;
  /** The car trip they're on, and whether they're in the car now (the body is hidden). */
  trip: Trip | null;
  inCar: boolean;
  /** Waiting on a decision or a day plan. */
  thinking: boolean;
  planning: boolean;
  /** Mischief they're off to do when they get there. */
  mischief: string | null;
  /** How you're talking to them: beside them, by phone, or with them in your car. */
  contact: 'beside' | 'phone' | 'passenger';
}

interface Convo {
  people: Npc[];
  /** null while they're still thinking of what to say. */
  lines: Line[] | null;
  index: number;
  next: number;
  started: number;
}

/** BUDDY, the police officer, and the friend he looks after. */
const OFFICER = 'buddy';
const PARTNER = 'guy';

// ---------- the city's clock ----------
// A city day is 24 minutes of play: an hour is a minute. The clock keeps
// running from one session to the next.
const HOUR = 60;
const START_HOUR = 8;
const DAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
const hourOf = (t: number) => (START_HOUR + t / HOUR) % 24;
const dayOf = (t: number) => Math.floor((START_HOUR + t / HOUR) / 24);
const weekday = (t: number) => dayOf(t) % 7 < 5;
/** The clock time when it next reaches `hour` (0..24). */
const nextHour = (t: number, hour: number) => t + (((hour - hourOf(t)) % 24) + 24) % 24 * HOUR;
function timeWords(t: number): string {
  const h = hourOf(t);
  const part = h < 5 ? 'the middle of the night' : h < 8 ? 'early morning' : h < 12 ? 'morning' : h < 14 ? 'lunchtime' : h < 17 ? 'afternoon' : h < 20 ? 'evening' : 'night';
  return `${DAYS[dayOf(t) % 7]}, ${part}`;
}
function clockText(t: number): string {
  const h = hourOf(t);
  const minutes = Math.floor((h % 1) * 60);
  return `${DAYS[dayOf(t) % 7]} ${String(Math.floor(h)).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
}

/** Bodies further from the camera than this aren't drawn (they still live their lives). */
const SHOW_RANGE = 150;
/** How close (to your car) someone has to be to chat when you click on them. */
const CHAT_RANGE = 60;
/** The AI writes conversations within this distance of you first. */
const NEAR_RANGE = 220;
/** The AI decides for people within this distance of you (others use their plan and the built-in brain). */
const DECIDE_RANGE = 140;
/** At most this many people's cars on the roads (and parked from trips) at once. */
const MAX_CARS = 3;
/** Further than this (metres) and people may take a car. */
const DRIVE_FROM = 150;
/** Bits of mischief people get up to (and BUDDY fines them for). */
const MISCHIEF = [
  'drew a silly face on a shop window',
  'knocked over the bins',
  "let down a parked car's tyres",
  'climbed the park statue',
  'set off a car alarm for fun',
  'stuck posters all over a lamp post',
];

const pick = <T,>(list: readonly T[]): T => list[Math.floor(Math.random() * list.length)];

const pack: Pack = {
  id: 'npc',
  name: 'Living City',
  icon: '🧠',
  about:
    `A whole city of ${CAST.length} people with jobs, homes, friends and minds of their own, brought to life by a free, open-source AI that runs on this computer. ` +
    'They live through the days, talk to each other, make plans, remember everything (for as long as the game is installed) and change over time. ' +
    'They choose freely what to do: drive across town, give friends lifts, make mischief, text you. BUDDY drives a police car, and may chase you if you go too far. ' +
    'Click on anyone to talk to them, and make friends. L opens the city journal (city free roam).',
  activate(game: GameApi): PackInstance {
    const stored = game.load<Partial<Options>>('options', {});
    const options: Options = {
      think: stored.think === 'nearby' || stored.think === 'everywhere' ? stored.think : DEFAULT_OPTIONS.think,
      tags: typeof stored.tags === 'boolean' ? stored.tags : DEFAULT_OPTIONS.tags,
      chases: typeof stored.chases === 'boolean' ? stored.chases : DEFAULT_OPTIONS.chases,
    };
    let clock = 0;
    let tickets = 0;
    /** How wanted you are, 0..5 stars: offences add to it, and it cools off with time. */
    let heat = 0;
    const crimes: string[] = []; // what you've done lately, newest first
    const record: PoliceRecord = { chases: 0, escapes: 0, busts: 0, fines: 0 };
    const messages: Message[] = []; // texts from people, newest first
    const meetups: Meetup[] = [];
    const news: string[] = []; // what's been happening, newest first
    const talks: Talk[] = []; // conversations overheard, newest first
    /** Switched off: answers still on their way are ignored. */
    let disposed = false;

    // ---------- the local AI ----------
    let ai: AiStatus | null = null; // null: not in the desktop game
    let waitUntil = 0; // after the AI is busy or fails, wait a little before asking again
    let aiTalks = 0; // conversations the AI is writing now
    let aiReflects = 0; // and reflections
    let aiDecides = 0; // and decisions
    let aiPlans = 0; // and day plans
    let lastAiDecide = -Infinity;
    let updateSettings: ((status: AiStatus | null) => void) | null = null;
    let firstStatus = true;
    const pollStatus = () =>
      game
        .callMain('status')
        .then((s) => {
          ai = s as AiStatus;
          // The first time the pack runs, fetch the model that suits this computer.
          if (firstStatus && ai.state === 'missing' && ai.downloaded.length === 0 && !game.load('fetched', false)) {
            game.save('fetched', true);
            void game.callMain('download', ai.model);
            game.toast(`🧠 Living City is downloading its local AI (${MODELS[ai.model].label}, ${MODELS[ai.model].size} GB, once). Until then the city uses its built-in brain.`, 6000);
          }
          firstStatus = false;
        })
        .catch(() => (ai = null))
        .finally(() => updateSettings?.(ai));
    void pollStatus();
    const poll = window.setInterval(() => void pollStatus(), 1500);
    const aiReady = () => ai?.state === 'ready' && performance.now() > waitUntil;
    const rules: Brain = new RuleBrain();
    const local = new LocalBrain(
      {
        call: (request) => game.callMain('think', request),
        failed: (reason, message) => {
          if (disposed) return;
          if (reason === 'not-ready') void pollStatus();
          waitUntil = performance.now() + (reason === 'busy' ? 4000 : reason === 'error' ? 20_000 : 2000);
          if (reason === 'error') console.warn('Living City: the local AI failed:', message);
        },
      },
      rules,
    );

    // ---------- the city and its people ----------
    const city = new CityMap(game.scene);
    const layer = el('div', 'pack-layer npc-layer');
    game.overlay.append(layer);
    const npcs: Npc[] = [];
    const byId = new Map<string, Npc>();
    const lastPair = new Map<string, number>(); // when each pair last talked
    let active = false;
    let thinkTimer = 0;
    let saveTimer = 0;

    const reset = (minds: Record<string, Mind> | undefined) => {
      clearRoads();
      for (const npc of npcs) {
        npc.body.dispose();
        npc.label.dispose();
      }
      npcs.length = 0;
      byId.clear();
      for (const member of CAST) {
        const body = new Body(member.look);
        body.root.visible = false;
        body.root.userData.npc = member.id; // for clicking on them
        game.scene.add(body.root);
        const style = member.id === PARTNER ? 'hero' : member.id === OFFICER ? 'police' : member.star ? '' : 'resident';
        const mind = restoreMind(member, minds?.[member.id]);
        const npc: Npc = {
          member,
          mind,
          body,
          label: new Label(layer, member.name, style, () => openChat(npc)),
          x: 0,
          z: 0,
          heading: Math.random() * Math.PI * 2,
          path: [],
          plan: { kind: 'wander', until: 0 },
          home: null,
          asleep: false,
          reaction: null,
          reactionUntil: 0,
          diveX: 0,
          diveZ: 0,
          lookX: null,
          lookZ: 0,
          convo: null,
          emotionUntil: 0,
          lastShout: -Infinity,
          lastScare: -Infinity,
          lastReflect: clock,
          lastGreet: -Infinity,
          reflecting: false,
          visit: null,
          chatting: false,
          bond: friendship(mind),
          trip: null,
          inCar: false,
          thinking: false,
          planning: false,
          mischief: null,
          contact: 'beside',
        };
        npcs.push(npc);
        byId.set(member.id, npc);
      }
      placeEveryone();
    };

    /** Puts everyone where they'd be at this time of day: in bed, at work, or somewhere they like. */
    const placeEveryone = () => {
      if (city.places.length === 0) return;
      for (const npc of npcs) {
        npc.home = city.home(npc.member.id);
        npc.path = [];
        npc.convo = null;
        npc.visit = null;
        if (npc.member.id === OFFICER) continue;
        if (bedtime(npc) && npc.home) {
          npc.x = npc.home.x;
          npc.z = npc.home.z;
          sleep(npc);
          continue;
        }
        npc.asleep = false;
        const place = bestPlace(npc) ?? city.places[0];
        const spot = spotAt(place);
        npc.x = spot.x;
        npc.z = spot.y;
        arrive(npc, place);
      }
      const officer = byId.get(OFFICER);
      const partner = byId.get(PARTNER);
      if (officer && partner) {
        officer.x = partner.x + 1.4;
        officer.z = partner.z + 0.6;
        officer.plan = { kind: 'follow', who: PARTNER };
        officer.asleep = partner.asleep;
      }
    };

    const spotAt = (place: Place) => {
      const a = Math.random() * Math.PI * 2;
      const r = place.spread * (0.3 + Math.random() * 0.7);
      return new THREE.Vector2(place.x + Math.cos(a) * r, place.z + Math.sin(a) * r);
    };

    // ---------- words for prompts ----------

    const carWords = () => describeCar(game.playerCar().config);
    const moment = (): Moment => ({ now: clock, time: timeWords(clock), hour: hourOf(clock), car: carWords(), news });
    const placeName = (x: number, z: number) => {
      const near = city.nearestPlace(x, z);
      return near && near.distance < 45 ? near.place.label : 'the street';
    };
    const doing = (npc: Npc): string => {
      if (npc.asleep) return hourOf(clock) > 9 && hourOf(clock) < 20 ? 'resting at home' : 'asleep at home';
      if (chase?.crew.includes(npc)) return npc.member.id === OFFICER ? 'chasing the driver in the police car, siren wailing' : "riding along in BUDDY's police car, in a chase with the driver";
      if (npc.chatting) {
        if (npc.contact === 'phone') return 'on the phone to the driver';
        if (npc.contact === 'passenger') return "riding in the driver's car, chatting";
        return 'talking to the driver, who has pulled up beside them';
      }
      if (npc.convo) {
        const others = npc.convo.people.filter((p) => p !== npc).map((p) => p.member.name);
        return `chatting with ${others.join(' and ')}`;
      }
      const meeting = (meet?: Meetup) => (meet ? ` to meet ${byId.get(meet.ids.find((id) => id !== npc.member.id) ?? '')?.member.name ?? 'a friend'}` : '');
      switch (npc.plan.kind) {
        case 'go':
          return `walking to ${npc.plan.place.label}${meeting(npc.plan.meet)}`;
        case 'stay':
          if (npc.plan.meet) return `waiting at ${npc.plan.place.label}${meeting(npc.plan.meet)}`;
          return npc.plan.place.kind === npc.member.job ? `working at ${npc.plan.place.label}` : `hanging out at ${npc.plan.place.label}`;
        case 'wander':
          return 'strolling around the city';
        case 'homeward':
          return npc.plan.nap ? 'heading home for a rest' : 'heading home for the night';
        case 'sleep':
          return 'asleep at home';
        case 'follow': {
          const name = byId.get(npc.plan.who)?.member.name ?? 'a friend';
          if (npc.plan.why === 'arrest') return `going after ${name}, who has been up to mischief`;
          if (npc.plan.why === 'talk') return `looking for ${name}`;
          if (npc.plan.why === 'tag') return `tagging along with ${name}`;
          return `walking with ${name}, keeping an eye on things`;
        }
        case 'trip': {
          const trip = npc.trip;
          if (!trip || !npc.inCar) return trip?.car === null ? 'walking to the police car' : 'walking to a car to drive somewhere';
          const where = trip.to === 'home' ? 'home' : trip.to.label;
          if (trip.driver === npc) return `driving ${trip.car ? '' : 'the police car '}to ${where}`;
          return `riding in ${trip.driver.member.name}'s ${trip.car ? 'car' : 'police car'} to ${where}`;
        }
        case 'pause':
          return 'stopping to think what to do';
        case 'ride':
          return `riding in the driver's car to ${npc.plan.place.label}`;
      }
    };
    const person = (npc: Npc): Person => ({ member: npc.member, mind: npc.mind, where: placeName(npc.x, npc.z), doing: doing(npc), contact: npc.chatting ? npc.contact : undefined });
    const tellNews = (text: string) => {
      if (news[0] === text) return;
      news.unshift(text);
      if (news.length > 10) news.pop();
    };

    // ---------- their days ----------

    /** When they go to bed and get up: the lively stay up later. */
    const bedHour = (npc: Npc) => 22.5 + npc.mind.traits.energy * 2; // 22:30 .. 00:30
    const wakeHour = (npc: Npc) => 5.5 + (1 - npc.mind.traits.energy) * 2; // 05:30 .. 07:30
    /** Saturday night is the street party in the park: nobody goes to bed before half past eleven. */
    const partyTime = () => dayOf(clock) % 7 === 5 && hourOf(clock) >= 19 && hourOf(clock) < 23.5;
    const partyPlace = () => city.placesOf('park')[0] ?? null;
    const bedtime = (npc: Npc) => {
      if (partyTime()) return false;
      const h = hourOf(clock);
      const late = h < 12 ? h + 24 : h; // counts the small hours as late tonight
      return late >= bedHour(npc) && late < wakeHour(npc) + 24;
    };

    const goHome = (npc: Npc, nap = false) => {
      if (!npc.home) return;
      leaveVisit(npc);
      npc.path = city.route(npc.x, npc.z, npc.home.x, npc.home.z);
      npc.plan = { kind: 'homeward', nap };
    };

    /** To bed (or, with `hours`, a rest at home for that long). */
    const sleep = (npc: Npc, hours = 0) => {
      npc.asleep = true;
      npc.path = [];
      npc.label.hush();
      npc.plan = { kind: 'sleep', until: hours ? clock + hours * HOUR : nextHour(clock, wakeHour(npc)) };
    };

    const wake = (npc: Npc) => {
      npc.asleep = false;
      npc.mind.needs.rest = 1;
      const morning = hourOf(clock) < 10;
      if (morning) remember(npc.mind, clock, `Woke up on ${DAYS[dayOf(clock) % 7]} morning.`, 1, [], 'saw');
      decide(npc, morning ? 'just woke up for the day' : 'just got up after a rest at home');
    };

    /** How much someone wants to be at a place right now: their needs, their routine, what they've learned they like, and who's there. */
    const appeal = (npc: Npc, place: Place): number => {
      const { mind, member } = npc;
      const n = mind.needs;
      const t = mind.traits;
      const h = hourOf(clock);
      let score = (mind.likes[place.kind] ?? 0) * 0.8;
      const kind = place.kind;
      // Hungry: somewhere to eat (breakfast, lunch and dinner come round at their times).
      const eat: Partial<Record<PlaceKind, number>> = { diner: 1.6, coffee: 1.1, bakery: 1.3, market: 0.8 };
      score += (1 - n.food) * (eat[kind] ?? 0);
      if (h >= 6.5 && h < 9.5 && (kind === 'coffee' || kind === 'bakery')) score += 0.6;
      if (h >= 12 && h < 13.5 && (kind === 'diner' || kind === 'bakery' || kind === 'market')) score += 0.5;
      if (h >= 18 && h < 20.5 && kind === 'diner') score += 0.7;
      // Tired, bored, curious.
      if (kind === 'park') score += (1 - n.rest) * 1.2 + (1 - n.fun) * 0.3;
      if (kind === 'stunts') score += (1 - n.fun) * (0.4 + t.courage);
      if (kind === 'music') score += (1 - n.fun) * (0.3 + t.humour * 0.8) + (h >= 19 ? 0.4 * t.energy : 0);
      if (kind === 'gym') score += (1 - n.fun) * t.energy * 0.8 - (1 - n.rest) * 0.8;
      if (kind === 'news' || kind === 'library') score += t.curiosity * 0.4;
      if (kind === 'market') score += t.curiosity * 0.2;
      // Shops and offices close in the evening (unless you work there).
      const shop = kind !== 'park' && kind !== 'stunts' && kind !== 'music';
      if (shop && kind !== member.job && (h < 7 || h >= 21)) score -= 1.5;
      // Work: in working hours on weekdays, when they're not desperate for anything.
      if (kind === member.job) {
        const hours = weekday(clock) && h >= 9 && h < 17;
        score += hours ? 1.4 * Math.min(1, 0.4 + Math.min(n.food, n.rest)) : -0.3;
      }
      // Friends there (or on their way): the lonelier they are, the more it counts.
      for (const other of npcs) {
        if (other === npc || other.asleep) continue;
        const at = other.plan.kind === 'stay' || other.plan.kind === 'go' ? other.plan.place : null;
        if (at === place) score += Math.max(0, mind.feelings[other.member.id] ?? 0) * (0.25 + (1 - n.social) * 0.6);
      }
      // What they planned for this time of day.
      const step = planStep(mind);
      if (step && step.place === kind) score += 0.9;
      // The Saturday street party.
      if (partyTime() && place === partyPlace()) score += 3;
      // Some variety, especially for the curious.
      if (npc.plan.kind === 'stay' && npc.plan.place === place) score -= 0.4;
      score += Math.random() * (0.2 + t.curiosity * 0.4);
      return score - Math.hypot(place.x - npc.x, place.z - npc.z) * 0.0015;
    };

    /** The step of their day plan for now (the latest one that's started). */
    const planStep = (mind: Mind) => {
      const h = hourOf(clock);
      let step: Mind['plan'][number] | null = null;
      for (const p of mind.plan) if (p.hour <= h) step = p;
      return step;
    };

    const bestPlace = (npc: Npc): Place | null => {
      let best: Place | null = null;
      let bestScore = -Infinity;
      for (const place of city.places) {
        const score = appeal(npc, place);
        if (score > bestScore) {
          bestScore = score;
          best = place;
        }
      }
      return best;
    };

    /** Leaving somewhere: was it a good time? They'll remember. */
    const leaveVisit = (npc: Npc) => {
      if (!npc.visit) return;
      const reward = (npc.mind.mood - npc.visit.mood) * 2.5 + (npc.visit.talked ? 0.35 : 0) + (npc.mind.likes[npc.visit.kind] ?? 0) * 0.3;
      learnPlace(npc.mind, npc.visit.kind, Math.max(-1, Math.min(1, reward)));
      npc.visit = null;
    };

    const goTo = (npc: Npc, place: Place, meet?: Meetup) => {
      const spot = spotAt(place);
      npc.path = city.route(npc.x, npc.z, spot.x, spot.y);
      npc.plan = { kind: 'go', place, spot, meet };
    };

    const wander = (npc: Npc) => {
      const spot = city.wanderSpot(npc.x, npc.z, Math.random);
      npc.path = city.route(npc.x, npc.z, spot.x, spot.y);
      npc.plan = { kind: 'wander', until: clock + 40 };
    };

    /** Where next, the built-in way: their plan for the day, their needs, and a little whim. */
    const ruleNext = (npc: Npc) => {
      if (npc.member.id === OFFICER) {
        npc.plan = { kind: 'follow', who: PARTNER };
        return;
      }
      if (bedtime(npc) && npc.home) return travelHome(npc);
      if (Math.random() < npc.mind.traits.curiosity * 0.1) return wander(npc);
      const best = bestPlace(npc);
      if (best) travel(npc, best);
    };

    /**
     * Time to choose what to do next. People near you (when the AI has a
     * moment) think it over freely with the AI; everyone else follows their
     * plan for the day and their needs, with the built-in brain choosing
     * something unusual now and then (mischief, a message to you).
     */
    const decide = (npc: Npc, why = 'deciding what to do next') => {
      leaveVisit(npc);
      if (npc.member.id === OFFICER) return ruleNext(npc);
      if (bedtime(npc) && npc.home) return travelHome(npc);
      const close = Math.hypot(npc.x - game.player.x, npc.z - game.player.z) < DECIDE_RANGE;
      if (close && aiReady() && aiDecides === 0 && aiTalks < 2 && clock - lastAiDecide > 6) return askDecision(npc, why, local);
      if (Math.random() < 0.3) return askDecision(npc, why, rules);
      ruleNext(npc);
    };

    /** Who's around someone, by name. */
    const nearbyNames = (npc: Npc) => near(npc.x, npc.z, 25).filter((n) => n.npc !== npc && !n.npc.inCar).map((n) => n.npc.member.name);

    const askDecision = (npc: Npc, why: string, brain: Brain, situation: Situation = { why, nearby: nearbyNames(npc) }) => {
      npc.thinking = true;
      npc.path = [];
      npc.plan = { kind: 'pause', until: clock + (brain === local ? 45 : 2) };
      if (brain === local) {
        aiDecides++;
        lastAiDecide = clock;
      }
      void brain
        .decide(person(npc), moment(), situation)
        .catch(() => rules.decide(person(npc), moment(), situation))
        .then((d) => {
          if (brain === local) aiDecides--;
          if (disposed || !npc.thinking) return;
          npc.thinking = false;
          act(npc, d);
        });
    };

    /** Shows what someone decided (what they say, or the thought on their mind) and keeps it for the journal. */
    const share = (npc: Npc, d: Decision) => {
      if (d.thought) addThought(npc.mind, d.thought);
      if (d.goal) setGoal(npc.mind, d.goal);
      if (npc.convo || npc.chatting || npc.asleep) return;
      const tag = npc.inCar ? npc.trip?.driver === npc || chase?.crew[0] === npc : true;
      if (!tag) return;
      if (d.say && d.action !== 'text the driver') npc.label.say(d.say, 2.5 + d.say.split(' ').length * 0.3, clock);
      else if (d.thought && (d.ai || Math.random() < 0.3)) npc.label.say(`💭 ${d.thought}`, 4.5, clock, true);
    };

    /** Does what they decided. */
    const act = (npc: Npc, d: Decision) => {
      share(npc, d);
      if (npc.member.id === OFFICER) return policeAct(npc, d);
      const who = d.person && d.person !== 'driver' ? (byId.get(d.person) ?? null) : null;
      const eat: PlaceKind[] = ['diner', 'coffee', 'bakery', 'market'];
      switch (d.action) {
        case 'go somewhere':
          return d.place ? headTo(npc, d.place) : ruleNext(npc);
        case 'drive somewhere':
          return headTo(npc, d.place ?? planStep(npc.mind)?.place ?? bestPlace(npc)?.kind ?? 'park', true);
        case 'go home':
          return headTo(npc, 'home');
        case 'rest':
          return headTo(npc, d.place === 'home' ? 'home' : 'park');
        case 'eat':
          return headTo(npc, d.place && eat.includes(d.place as PlaceKind) ? d.place : (pick(eat) as PlaceKind));
        case 'work':
          return npc.member.job ? headTo(npc, npc.member.job) : ruleNext(npc);
        case 'wander':
          return wander(npc);
        case 'find someone':
          return who ? seek(npc, who, 'talk') : ruleNext(npc);
        case 'follow someone':
          return who ? seek(npc, who, 'tag') : ruleNext(npc);
        case 'avoid someone':
          return who ? avoid(npc, who) : ruleNext(npc);
        case 'give someone a lift':
          return who ? lift(npc, who, d.place) : ruleNext(npc);
        case 'get up to mischief':
          return mischief(npc);
        case 'text the driver':
          sendText(npc, d.say || pick(['Hey! What are you up to?', 'Come and say hi sometime!', 'Saw you drive past earlier!']));
          return ruleNext(npc);
        case 'report the driver to the police':
          report(npc);
          return ruleNext(npc);
        default:
          return ruleNext(npc);
      }
    };

    /** Off to a kind of place (the one they like best), or home. `drive` takes a car if one's free. */
    const headTo = (npc: Npc, kind: PlaceKind | 'home', drive = false) => {
      if (kind === 'home') return travelHome(npc, hourOf(clock) < 19);
      const options = city.placesOf(kind);
      if (!options.length) return ruleNext(npc);
      const place = options.reduce((a, b) => (appeal(npc, a) >= appeal(npc, b) ? a : b));
      travel(npc, place, drive);
    };

    /** Goes to a place: on foot, or (far away, or keen to) by car. */
    const travel = (npc: Npc, place: Place, drive = false) => {
      const far = Math.hypot(place.x - npc.x, place.z - npc.z) > DRIVE_FROM;
      if ((drive || (far && Math.random() < 0.5)) && startTrip(npc, place)) return;
      goTo(npc, place);
    };

    /** Goes home: to bed, or (`nap`) for a rest. */
    const travelHome = (npc: Npc, nap = false) => {
      if (!npc.home) return nap ? goTo(npc, city.placesOf('park')[0] ?? city.places[0]) : undefined;
      const far = Math.hypot(npc.home.x - npc.x, npc.home.z - npc.z) > DRIVE_FROM * 1.3;
      if (far && Math.random() < 0.5 && startTrip(npc, 'home')) return; // arriveHome() works out bed or a rest
      goHome(npc, nap);
    };

    /** Home after a trip: to bed if it's late, otherwise a rest. */
    const arriveHome = (npc: Npc) => {
      if (bedtime(npc) || hourOf(clock) >= 21) goHome(npc);
      else goHome(npc, true);
    };

    /** Looks for someone (to talk), or tags along with them, for a while. */
    const seek = (npc: Npc, who: Npc, why: 'talk' | 'tag') => {
      if (who.asleep || who.inCar || who === npc) return ruleNext(npc);
      npc.plan = { kind: 'follow', who: who.member.id, until: clock + (why === 'talk' ? 120 : 90), why };
      npc.path = [];
    };

    /** Keeps away from someone: somewhere they like, far from them. */
    const avoid = (npc: Npc, who: Npc) => {
      const away = city.places.reduce((a, b) => (appeal(npc, a) + Math.hypot(a.x - who.x, a.z - who.z) * 0.004 >= appeal(npc, b) + Math.hypot(b.x - who.x, b.z - who.z) * 0.004 ? a : b));
      feel(npc.mind, who.member.id, -0.02);
      travel(npc, away);
    };

    /** Offers someone nearby a lift, and drives them both there. */
    const lift = (npc: Npc, who: Npc, kind: PlaceKind | 'home' | null) => {
      const free = !who.asleep && !who.inCar && !who.trip && !who.convo && !who.chatting && who.member.id !== OFFICER;
      if (!free || Math.hypot(who.x - npc.x, who.z - npc.z) > 40) return seek(npc, who, 'talk');
      const theirs = who.plan.kind === 'go' || who.plan.kind === 'stay' ? who.plan.place : null;
      const place = kind && kind !== 'home' ? (city.placesOf(kind)[0] ?? theirs) : (theirs ?? bestPlace(who));
      if (!place || !startTrip(npc, place, [who])) return seek(npc, who, 'talk');
      remember(who.mind, clock, `${npc.member.name} offered me a lift to ${place.label}.`, 4, [npc.member.id], 'saw');
      feel(who.mind, npc.member.id, 0.04);
    };

    /** Off to make some mischief at a place (see doMischief). */
    const mischief = (npc: Npc) => {
      const place = bestPlace(npc);
      if (!place) return;
      npc.mischief = pick(MISCHIEF);
      travel(npc, place);
    };

    const arrive = (npc: Npc, place: Place, meet?: Meetup) => {
      const working = place.kind === npc.member.job && weekday(clock);
      const stay = meet ? 2 * HOUR : working ? 90 + Math.random() * 120 : 40 + Math.random() * 60;
      npc.plan = { kind: 'stay', place, until: clock + stay, meet };
      npc.visit = { kind: place.kind, mood: npc.mind.mood, talked: false };
      if (partyTime() && place === partyPlace()) {
        remember(npc.mind, clock, `Went to the Saturday street party at ${place.label}.`, 5, [], 'saw');
        npc.plan.until = Math.max(npc.plan.until, nextHour(clock, 23.4));
      } else if (Math.random() < 0.1) remember(npc.mind, clock, `Spent some time at ${place.label}.`, 1, [], 'saw');
      if (npc.mischief) doMischief(npc, place);
    };

    // ---------- day plans ----------

    /**
     * Each morning everyone plans their day (a goal, and a few steps by the
     * hour). The AI writes them when it has nothing else to do; otherwise
     * (or if it's getting late) the built-in brain does.
     */
    const planDay = (npc: Npc) => {
      const day = dayOf(clock);
      if (npc.mind.planDay === day || npc.asleep || npc.planning) return;
      const late = hourOf(clock) > wakeHour(npc) + 3 || hourOf(clock) < 5;
      const thinking = ai?.state === 'ready' && !late;
      if (thinking && !(aiReady() && aiPlans === 0 && aiTalks === 0 && aiDecides === 0)) return; // later, when the AI has a quiet moment
      const brain = thinking ? local : rules;
      npc.planning = true;
      if (brain === local) aiPlans++;
      void brain
        .plan(person(npc), moment())
        .catch(() => rules.plan(person(npc), moment()))
        .then((result) => {
          if (brain === local) aiPlans--;
          npc.planning = false;
          if (disposed) return;
          npc.mind.planDay = day;
          npc.mind.plan = result.plan.slice(0, 8);
          if (result.goal) setGoal(npc.mind, result.goal);
        });
    };

    // ---------- plans to meet ----------

    const placeById = (id: string, kind: PlaceKind) => city.place(id) ?? city.placesOf(kind)[0];

    const makePlan = (a: Npc, b: Npc, plan: MeetPlan) => {
      const place = city.placesOf(plan.place).sort((p, q) => Math.hypot(p.x - a.x, p.z - a.z) - Math.hypot(q.x - a.x, q.z - a.z))[0];
      if (!place) return;
      // Soon is within the hour or two, later is this evening-ish, tomorrow is tomorrow in the daytime.
      let at =
        plan.when === 'soon'
          ? clock + (1 + Math.random()) * HOUR
          : plan.when === 'later'
            ? clock + (4 + Math.random() * 3) * HOUR
            : nextHour(clock, 10 + Math.random() * 8);
      if (plan.when === 'tomorrow' && dayOf(at) === dayOf(clock)) at += 24 * HOUR;
      // Nobody meets up in the middle of the night: that becomes tomorrow morning.
      if (hourOf(at) >= 22 || hourOf(at) < 8) at = nextHour(at, 9 + Math.random() * 2);
      // One plan per pair at a time.
      const old = meetups.findIndex((m) => m.ids.includes(a.member.id) && m.ids.includes(b.member.id));
      if (old >= 0) meetups.splice(old, 1);
      const meetup: Meetup = { ids: [a.member.id, b.member.id], placeId: place.id, kind: place.kind, at };
      meetups.push(meetup);
      if (meetups.length > 40) meetups.shift();
      const when = plan.when === 'tomorrow' ? 'tomorrow' : plan.when === 'later' ? 'later today' : 'soon';
      remember(a.mind, clock, `Planned to meet ${b.member.name} at ${place.label} ${when}.`, 5, [b.member.id], 'talk');
      remember(b.mind, clock, `Planned to meet ${a.member.name} at ${place.label} ${when}.`, 5, [a.member.id], 'talk');
      tellNews(`${a.member.name} and ${b.member.name} plan to meet at ${place.label} ${when}`);
    };

    /** Keeps their plans: heads off when it's time, and notices when someone doesn't turn up. */
    const keepPlans = () => {
      for (let i = meetups.length - 1; i >= 0; i--) {
        const m = meetups[i];
        const people = m.ids.map((id) => byId.get(id));
        if (people.some((p) => !p)) {
          meetups.splice(i, 1);
          continue;
        }
        const [a, b] = people as Npc[];
        const place = placeById(m.placeId, m.kind);
        if (!place) continue;
        // Time to go (they leave a little early).
        if (clock > m.at - 0.5 * HOUR && clock < m.at + HOUR) {
          for (const p of [a, b]) {
            if (p.asleep || p.chatting || p.convo || p.inCar || p.trip || p.plan.kind === 'follow' || p.plan.kind === 'ride' || bedtime(p)) continue;
            if ((p.plan.kind === 'go' || p.plan.kind === 'stay') && p.plan.meet === m) continue;
            leaveVisit(p);
            goTo(p, place, m);
          }
          // Both there: they talk (and the plan is done).
          if (Math.hypot(a.x - b.x, a.z - b.z) < 7 && !a.convo && !b.convo && !a.chatting && !b.chatting && !a.asleep && !b.asleep) {
            meetups.splice(i, 1);
            remember(a.mind, clock, `Met up with ${b.member.name} at ${place.label}, like we planned.`, 4, [b.member.id], 'talk');
            remember(b.mind, clock, `Met up with ${a.member.name} at ${place.label}, like we planned.`, 4, [a.member.id], 'talk');
            feel(a.mind, b.member.id, 0.04);
            feel(b.mind, a.member.id, 0.04);
            // They spend a while together, then get on with their day.
            for (const p of [a, b]) {
              if (p.plan.kind === 'go' || p.plan.kind === 'stay') p.plan.meet = undefined;
              if (p.plan.kind === 'stay') p.plan.until = clock + (0.5 + Math.random()) * HOUR;
            }
            startTalk(a, b);
          }
        } else if (clock >= m.at + HOUR) {
          // Someone didn't turn up: whoever came remembers it.
          meetups.splice(i, 1);
          for (const [p, other] of [
            [a, b],
            [b, a],
          ] as const) {
            const came = Math.hypot(p.x - place.x, p.z - place.z) < 25;
            const otherCame = Math.hypot(other.x - place.x, other.z - place.z) < 25;
            if (came && !otherCame) {
              remember(p.mind, clock, `Waited for ${other.member.name} at ${place.label}, but they never showed up.`, 6, [other.member.id], 'saw');
              feel(p.mind, other.member.id, -0.08);
              jolt(p.mind, -0.08, 'sad');
            }
          }
        }
      }
    };

    // ---------- feelings on their faces ----------

    const emote = (npc: Npc, emotion: Emotion, seconds = 20) => {
      npc.mind.emotion = emotion;
      npc.emotionUntil = clock + seconds;
    };

    const SHOUTS: Partial<Record<Emotion, string[]>> = {
      scared: ['Whoa!', 'Look out!', 'Yikes!', 'Not again!'],
      annoyed: ['Hey! Watch it!', 'Seriously?', 'Slow down!', 'Some of us walk here!'],
      excited: ['Wooo!', 'Did you see that?!', 'Amazing!', 'Do it again!'],
      amused: ['Ha!', 'Nice one!', "That's one way to do it.", 'Classic!'],
      worried: ['Oh dear.', 'Is everyone okay?', 'That looked expensive.'],
      happy: ['Hey there!', 'Morning!', 'Lovely!'],
    };
    const shout = (npc: Npc, emotion: Emotion) => {
      if (npc.convo?.lines || npc.chatting || npc.asleep || clock - npc.lastShout < 6) return;
      const lines = SHOUTS[emotion];
      if (!lines) return;
      npc.lastShout = clock;
      npc.label.say(pick(lines), 2.4, clock);
    };

    /** Everyone awake within `radius` of a spot, with how far away they are. */
    const near = (x: number, z: number, radius: number) =>
      npcs.filter((npc) => !npc.asleep).map((npc) => ({ npc, d: Math.hypot(npc.x - x, npc.z - z) })).filter((n) => n.d < radius);

    /** How someone takes a wild bit of driving: the bold and fun-loving enjoy it, the timid and kind don't. */
    const thrill = (npc: Npc) => {
      const t = npc.mind.traits;
      return (t.courage + t.energy + t.humour) / 3 - 0.5 + (npc.mind.feelings.driver ?? 0) * 0.4;
    };

    // ---------- the police ----------

    let lastTicket = -Infinity;
    let lastSpeeding = -Infinity;
    /**
     * Something you did that BUDDY may care about. It makes you more wanted
     * (`severity`, in stars), and if BUDDY sees it (or someone who saw it
     * calls him) he decides what to do about it: let it go, write a ticket,
     * or come after you.
     */
    const offence = (what: string, x: number, z: number, reach = 30, severity = 1) => {
      if (!active) return;
      heat = Math.min(5, heat + severity * (chase ? 0.3 : 1));
      if (crimes[0] !== what) crimes.unshift(what);
      if (crimes.length > 6) crimes.pop();
      const officer = byId.get(OFFICER);
      if (chase || !officer || officer.asleep || officer.chatting || officer.thinking || clock - lastTicket < 40) return;
      const sees = Math.hypot(officer.x - x, officer.z - z) <= (officer.inCar ? reach * 2 : reach);
      if (sees) return policeConsider(`You just saw the driver in ${carWords()}: ${what}, near ${placeName(x, z)}.`);
      // Out of his sight: someone who saw it (and doesn't like it) may call him.
      if (heat < 2.5) return;
      for (const { npc } of near(x, z, Math.max(reach, 40))) {
        if (npc === officer || npc.inCar || npc.chatting || (npc.mind.feelings.driver ?? 0) > 0.1 || Math.random() > 0.3) continue;
        npc.label.say(pick(["I'm calling the police!", 'Right, that does it. BUDDY!', 'Somebody call BUDDY!']), 2.6, clock);
        remember(npc.mind, clock, `Called BUDDY about the driver: ${what}.`, 5, ['driver', 'buddy'], 'saw');
        return policeConsider(`${npc.member.name} called you: the driver in ${carWords()} was ${what} near ${placeName(x, z)}.`);
      }
    };

    /** BUDDY makes up his own mind about the driver: the AI if it can, the built-in rules otherwise. */
    const policeConsider = (why: string, crime = true) => {
      const officer = byId.get(OFFICER);
      if (!officer || officer.thinking || officer.asleep) return;
      if (crime) lastTicket = clock;
      const brain = aiReady() && aiDecides < 2 ? local : rules;
      officer.thinking = true;
      if (brain === local) aiDecides++;
      const situation: Situation = { why, nearby: nearbyNames(officer), crimes: crime || heat >= 0.5 ? { list: crimes.slice(0, 5), heat: Math.round(heat) } : undefined };
      void brain
        .decide(person(officer), moment(), situation)
        .catch(() => rules.decide(person(officer), moment(), situation))
        .then((d) => {
          if (brain === local) aiDecides--;
          officer.thinking = false;
          if (disposed || !active) return;
          act(officer, d);
        });
    };

    /** What BUDDY does about it. */
    const policeAct = (officer: Npc, d: Decision) => {
      switch (d.action) {
        // Too little for a chase: a ticket (or, with nothing at all against you, a drive instead).
        case 'chase the driver down':
          if (heat < 1.5) return heat >= 0.5 ? ticket(officer) : patrol(officer);
          return canChase() ? startChase() : ticket(officer);
        case 'ticket the driver':
          return heat >= 0.5 ? ticket(officer) : undefined;
        case 'let it go':
          return heat >= 0.5 ? warn(officer) : undefined;
        case 'go for a drive with GUY':
          return patrol(officer);
        case 'text the driver':
          return sendText(officer, d.say || 'Drive safe out there. I mean it.');
      }
    };

    const ticket = (officer: Npc) => {
      const what = crimes[0] ?? 'reckless driving';
      tickets++;
      heat = Math.max(0, heat - 1);
      officer.label.say(pick([`That's a ticket for ${what}!`, 'Pull over! Ticket time.', `Whoa there! ${what[0].toUpperCase() + what.slice(1)}!`, 'I saw that! Ticket!']), 3.5, clock);
      remember(officer.mind, clock, `Wrote the driver in ${carWords()} a ticket for ${what}.`, 5, ['driver'], 'saw');
      feel(officer.mind, 'driver', -0.04);
      game.toast(`🚓 BUDDY wrote you a ticket for ${what} (${tickets} so far)`, 2600);
      const partner = byId.get(PARTNER);
      if (partner && Math.hypot(partner.x - officer.x, partner.z - officer.z) < 12) {
        remember(partner.mind, clock, `Watched BUDDY give the driver a ticket for ${what}.`, 3, ['buddy', 'driver'], 'saw');
      }
      emote(officer, 'proud');
      officer.lookX = game.player.x;
      officer.lookZ = game.player.z;
      tellNews(`BUDDY ticketed the driver for ${what}`);
    };

    const warn = (officer: Npc) => {
      const what = crimes[0] ?? 'reckless driving';
      const name = officer.mind.driverName;
      heat = Math.max(0, heat - 0.5);
      officer.label.say(pick([`Let's call that a warning${name ? `, ${name}` : ''}.`, "I didn't see that. This time.", 'Easy, friend. Easy.']), 3.5, clock);
      remember(officer.mind, clock, `Let the driver off with a warning for ${what}.`, 4, ['driver'], 'saw');
      game.toast(`🚓 BUDDY let you off with a warning for ${what}`, 2600);
      tellNews(`BUDDY warned the driver for ${what}`);
    };

    /** BUDDY takes GUY for a drive in the police car. */
    const patrol = (officer: Npc) => {
      if (officer.inCar || officer.trip) return;
      const partner = byId.get(PARTNER);
      const theirs = partner && (partner.plan.kind === 'go' || partner.plan.kind === 'stay') ? partner.plan.place : null;
      const place = theirs && Math.hypot(theirs.x - officer.x, theirs.z - officer.z) > 80 ? theirs : pick(city.places);
      if (place) startTrip(officer, place);
    };

    /** Someone who's had enough of you calls BUDDY. */
    const report = (npc: Npc) => {
      if (heat < 0.5 || !crimes.length) return;
      npc.label.say(pick(["I'm reporting you!", 'BUDDY needs to hear about this.']), 2.6, clock);
      remember(npc.mind, clock, `Reported the driver to BUDDY for ${crimes[0]}.`, 5, ['driver', 'buddy'], 'saw');
      heat = Math.min(5, heat + 0.5);
      tellNews(`${npc.member.name} reported the driver to the police`);
      policeConsider(`${npc.member.name} called you to report the driver for ${crimes[0]}.`);
    };

    // ---------- mischief, and BUDDY on the case ----------

    const fines = new Map<string, string>(); // what each culprit BUDDY is after did

    const doMischief = (npc: Npc, place: Place) => {
      const what = npc.mischief ?? pick(MISCHIEF);
      npc.mischief = null;
      npc.label.say(pick(['Heh heh.', 'Nobody saw that.', 'Oops!', 'Worth it.']), 2.5, clock);
      remember(npc.mind, clock, `I ${what} at ${place.label}. Ha!`, 5, [], 'saw');
      npc.mind.needs.fun = Math.min(1, npc.mind.needs.fun + 0.3);
      emote(npc, 'amused');
      tellNews(`${npc.member.name} ${what} at ${place.label}`);
      for (const { npc: other } of near(npc.x, npc.z, 25)) {
        if (other === npc || other.inCar) continue;
        remember(other.mind, clock, `Saw ${npc.member.name} ${what} at ${place.label}.`, 4, [npc.member.id], 'saw');
        feel(other.mind, npc.member.id, -0.05);
      }
      // BUDDY hears about it and goes after them (unless he's busy).
      const officer = byId.get(OFFICER);
      if (!officer || officer.asleep || officer.inCar || officer.trip || officer.chatting || chase || npc === officer) return;
      if (officer.plan.kind !== 'follow' || officer.plan.who !== PARTNER || Math.hypot(officer.x - npc.x, officer.z - npc.z) > 400) return;
      fines.set(npc.member.id, what);
      officer.plan = { kind: 'follow', who: npc.member.id, until: clock + 150, why: 'arrest' };
      if (Math.hypot(officer.x - npc.x, officer.z - npc.z) < 60) officer.label.say(pick(['Hold it right there!', 'I saw that!', 'Hey! Stop!']), 2.5, clock);
    };

    /** BUDDY caught up with someone up to mischief. */
    const fine = (officer: Npc, culprit: Npc) => {
      const what = fines.get(culprit.member.id) ?? 'causing trouble';
      fines.delete(culprit.member.id);
      officer.plan = { kind: 'follow', who: PARTNER };
      officer.lookX = culprit.x;
      officer.lookZ = culprit.z;
      officer.label.say(`That's a fine, ${culprit.member.name}. Behave yourself.`, 3.2, clock);
      emote(officer, 'proud');
      if (!culprit.inCar && !culprit.asleep) {
        culprit.lookX = officer.x;
        culprit.lookZ = officer.z;
        emote(culprit, 'annoyed');
      }
      remember(officer.mind, clock, `Fined ${culprit.member.name}, who ${what}.`, 5, [culprit.member.id], 'saw');
      remember(culprit.mind, clock, `BUDDY caught me and gave me a fine. I ${what}.`, 6, ['buddy'], 'saw');
      feel(culprit.mind, OFFICER, -0.06);
      jolt(culprit.mind, -0.1, 'annoyed');
      tellNews(`BUDDY fined ${culprit.member.name} for mischief`);
    };

    // ---------- the chase ----------

    let chase: Chase | null = null;
    let chaseCooldown = 0; // no new chase before this (city clock)
    let police: PoliceCar | null = null;

    /** BUDDY's police car (made the first time it's needed). */
    const cruiser = (): PoliceCar => {
      if (police) return police;
      const car = new PoliceCar(game.scene, game.world, game.audio, game.world.isNight);
      car.setActive(false);
      car.onShot = (from, to, hit) => {
        const dir = to.clone().sub(from).normalize();
        game.effects.muzzleFlash(from, dir);
        game.effects.tracer(from, to);
        if (hit) game.effects.bulletHit(to, false);
        if (from.distanceTo(game.camera.position) < 70) game.audio.gunshot();
      };
      car.onHit = () => {
        if (!chase) offence('shooting at a police car', car.x, car.z, 500, 3);
      };
      police = car;
      return car;
    };

    const canChase = () => {
      const officer = byId.get(OFFICER);
      return options.chases && active && !chase && clock > chaseCooldown && !!officer && !officer.asleep && !(police && police.wrecked > 0);
    };

    /** Where to stand by a car to get in or out (on the pavement side). */
    const doorOf = (x: number, z: number, heading: number, i = 0) => ({
      x: x - Math.cos(heading) * 2.6 - Math.sin(heading) * i * 1.2,
      z: z + Math.sin(heading) * 2.6 - Math.cos(heading) * i * 1.2,
    });

    const startChase = () => {
      const officer = byId.get(OFFICER);
      if (!officer) return;
      const partner = byId.get(PARTNER);
      const car = cruiser();
      const crew = [officer];
      const trip = officer.trip;
      if (trip && trip.car === null && trip.phase === 'drive') {
        // Already out in the police car: GUY (and anyone else) stays aboard.
        crew.push(...trip.riders);
        trips.splice(trips.indexOf(trip), 1);
        for (const npc of crew) npc.trip = null;
      } else {
        if (trip) cancelTrip(trip);
        // Jumps into the police car, parked just by him (GUY too, if he's there).
        if (Math.hypot(car.x - officer.x, car.z - officer.z) > 30 || !car.rig.root.visible) {
          const spot = parkingNear(officer.x, officer.z);
          car.place(spot.x, spot.z, spot.heading);
        }
        if (partner && !partner.asleep && !partner.inCar && !partner.trip && !partner.chatting && Math.hypot(partner.x - officer.x, partner.z - officer.z) < 15) crew.push(partner);
        for (const npc of crew) {
          if (npc.convo) endTalk(npc.convo);
          npc.inCar = true;
          npc.path = [];
        }
      }
      for (const npc of crew) npc.plan = { kind: 'follow', who: npc === officer ? PARTNER : OFFICER };
      chase = { started: clock, lost: 0, stopped: 0, damage: 0, crew, seen: new Set() };
      car.mode = 'chase';
      car.firing = heat >= 4;
      car.setActive(true);
      car.setSiren(true);
      record.chases++;
      game.toast(`🚨 BUDDY is coming after you${crimes[0] ? ` for ${crimes[0]}` : ''}! Lose him, or pull over and take the fine.`, 3800);
      tellNews('BUDDY chased the driver through the city, siren wailing');
      remember(officer.mind, clock, `Went after the driver in ${carWords()} in the police car${crimes[0] ? ` for ${crimes[0]}` : ''}.`, 7, ['driver'], 'saw');
      if (crew.includes(partner!)) remember(partner!.mind, clock, 'Rode along with BUDDY in a real police chase after the driver!', 7, ['buddy', 'driver'], 'saw');
      if (rider) remember(rider.npc.mind, clock, "I was in the driver's car when BUDDY came after us!", 7, ['driver', 'buddy'], 'saw');
    };

    /** Every frame of a chase: BUDDY drives, rams and shoots; you get away, or you don't. */
    const updateChase = (dt: number) => {
      const c = chase;
      const car = police;
      if (!c || !car) return;
      const p = game.player;
      const step = car.update(dt, p, false);
      for (const npc of c.crew) {
        npc.x = car.x;
        npc.z = car.z;
      }
      car.hearing = Math.hypot(car.x - game.camera.position.x, car.z - game.camera.position.z);
      if (car.hp <= 0) return wreckPolice();
      if (step.rammed > 3) {
        c.damage += step.rammed * 1.6;
        game.effects.bulletHit(new THREE.Vector3((p.x + car.x) / 2, p.y + 0.6, (p.z + car.z) / 2), true);
      }
      c.damage += step.shot;
      // After a while (or straight away if you've been really bad) he opens fire.
      if (!car.firing && clock - c.started > 14) {
        car.firing = true;
        c.crew[0].label.say('Last warning! Pull over!', 3, clock);
      }
      const d = Math.hypot(car.x - p.x, car.z - p.z);
      // Out of BUDDY's sight (too far, or behind a building) for long enough, and you've got away.
      const seen = car.canSee(p.x, p.z, dt);
      c.lost = !seen ? c.lost + dt : Math.max(0, c.lost - dt * 0.5);
      if (c.lost > 8 || d > 320) return endChase('escaped');
      // Pulled over where he can see you (not just near him, behind a wall).
      const speed = Math.hypot(p.vx, p.vz);
      c.stopped = seen && d < 16 && speed < 2 ? c.stopped + dt : Math.max(0, c.stopped - dt);
      if (c.stopped > 3) return endChase('busted');
      if (c.damage >= 100) return endChase('busted', true);
      // The city watches it go by.
      for (const { npc } of near(p.x, p.z, 30)) {
        if (c.seen.has(npc) || npc.inCar) continue;
        c.seen.add(npc);
        const fun = thrill(npc) > 0.1;
        remember(npc.mind, clock, `Saw BUDDY chase the driver past ${placeName(npc.x, npc.z)}, siren wailing!`, 6, ['buddy', 'driver'], 'saw');
        jolt(npc.mind, fun ? 0.05 : -0.05, fun ? 'excited' : 'scared');
        emote(npc, npc.mind.emotion);
        shout(npc, npc.mind.emotion);
      }
    };

    /** Your weapons wrecked the police car. */
    const wreckPolice = () => {
      const car = police;
      if (!car || car.wrecked > 0) return;
      car.wreck();
      const at = new THREE.Vector3(car.x, car.physics.y + 1, car.z);
      game.effects.explosion(at, 1.4);
      game.audio.explosion(1.4, at.distanceTo(game.camera.position));
      const officer = byId.get(OFFICER);
      if (officer) {
        remember(officer.mind, clock, `The driver wrecked my police car! I won't forget that.`, 9, ['driver'], 'saw');
        feel(officer.mind, 'driver', -0.2);
        jolt(officer.mind, -0.2, 'annoyed');
      }
      tellNews("the driver wrecked BUDDY's police car");
      heat = 5;
      if (chase) endChase('wrecked');
      else {
        const trip = trips.find((t) => t.car === null);
        if (trip) (trip.phase === 'drive' ? bail : cancelTrip)(trip);
      }
    };

    /** The chase is over: busted (a fine), escaped, the police car wrecked, or called off (an event, the option switched off). */
    const endChase = (outcome: 'busted' | 'escaped' | 'wrecked' | 'off', wrecked = false) => {
      const c = chase;
      const car = police;
      chase = null;
      if (!c || !car) return;
      chaseCooldown = clock + (outcome === 'off' ? 30 : 120);
      if (car.mode === 'chase') car.mode = 'parked';
      car.setSiren(false);
      car.firing = false;
      const [officer, ...rest] = c.crew;
      const partner = rest.find((n) => n.member.id === PARTNER);
      const where = placeName(game.player.x, game.player.z);
      if (outcome === 'busted') {
        const fine = 60 + Math.round(heat) * 40 + (wrecked ? 50 : 0);
        const paid = -game.addCoins(-fine);
        tickets++;
        record.busts++;
        record.fines += paid;
        heat = 0;
        crimes.length = 0;
        game.toast(`🚓 Busted${wrecked ? ': your car is too shot up to go on' : ''}! BUDDY fined you ${paid} coins (${tickets} ticket${tickets === 1 ? '' : 's'}).`, 4200);
        officer.label.say(pick(['Gotcha! That will be a fine.', 'Nice try. License, please.', "End of the road, friend."]), 3.5, clock);
        remember(officer.mind, clock, `Caught the driver after a chase to ${where} and fined them ${paid} coins.`, 8, ['driver'], 'saw');
        emote(officer, 'proud', 40);
        if (partner) remember(partner.mind, clock, `BUDDY caught the driver after a chase! I was right there in the car.`, 7, ['buddy', 'driver'], 'saw');
        tellNews(`BUDDY caught the driver after a chase to ${where}`);
      } else if (outcome === 'escaped') {
        record.escapes++;
        heat = Math.min(heat, 3);
        game.toast('😎 You lost BUDDY! Lie low for a while.', 3200);
        remember(officer.mind, clock, 'The driver got away from me in a chase. Next time.', 7, ['driver'], 'saw');
        feel(officer.mind, 'driver', -0.06);
        emote(officer, 'annoyed', 40);
        if (partner) remember(partner.mind, clock, 'We chased the driver all over the city and they got away!', 6, ['buddy', 'driver'], 'saw');
        tellNews('the driver got away from BUDDY in a chase');
        for (const npc of c.seen) if (thrill(npc) > 0.1) feel(npc.mind, 'driver', 0.03);
      } else if (outcome === 'wrecked') {
        record.escapes++;
        game.toast("💥 You wrecked BUDDY's police car! He won't forget that.", 3800);
      }
      // Everyone out (the police car stays where it stopped, and gets towed once nobody's looking).
      c.crew.forEach((npc, i) => {
        const door = doorOf(car.x, car.z, car.physics.heading, i);
        npc.inCar = false;
        npc.x = door.x;
        npc.z = door.z;
        if (outcome === 'wrecked') {
          emote(npc, 'scared', 20);
          npc.label.say(pick(['My car!', 'Whoa!', 'Everyone okay?']), 2.5, clock);
        }
        if (npc === officer) npc.plan = { kind: 'follow', who: PARTNER };
        else decide(npc, 'just got out of the police car after a chase');
      });
      hud.classList.add('hidden');
    };

    // ---------- messages ----------

    let messageCard: { npc: Npc; until: number } | null = null;
    const phone = el('div', 'npc-phone hidden');
    phone.addEventListener('click', () => {
      if (messageCard) openChat(messageCard.npc, 'phone');
      phone.classList.add('hidden');
      messageCard = null;
    });
    layer.append(phone);

    /** Someone texts you (they need to have met you, to have your number). */
    const sendText = (npc: Npc, text: string) => {
      if (!(npc.mind.met.driver ?? 0) && (npc.mind.feelings.driver ?? 0) < 0.3) return;
      messages.unshift({ from: npc.member.name, text, t: clock });
      if (messages.length > 20) messages.pop();
      remember(npc.mind, clock, `Sent the driver a message: "${text}"`, 3, ['driver'], 'chat');
      if (!active || chat.open) return;
      phone.replaceChildren(el('b', '', `📱 ${npc.member.name}`), el('span', '', text), el('small', '', 'Click to reply'));
      phone.classList.remove('hidden');
      messageCard = { npc, until: clock + 10 };
      ping();
    };

    /** A phone's two-note ping, through the game's sound. */
    const ping = () => {
      const ctx = game.audio.context;
      const out = game.audio.output;
      if (!ctx || !out) return;
      [0, 0.14].forEach((delay, i) => {
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.frequency.value = i ? 1320 : 880;
        gain.gain.setValueAtTime(0.0001, ctx.currentTime + delay);
        gain.gain.exponentialRampToValueAtTime(0.08, ctx.currentTime + delay + 0.01);
        gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + delay + 0.12);
        osc.connect(gain).connect(out);
        osc.start(ctx.currentTime + delay);
        osc.stop(ctx.currentTime + delay + 0.15);
      });
    };

    // ---------- the wanted level ----------

    const hud = el('div', 'npc-wanted hidden');
    layer.append(hud);
    const updateHud = () => {
      const c = chase;
      if (!c && heat < 0.5) {
        hud.classList.add('hidden');
        return;
      }
      hud.classList.remove('hidden');
      const stars = Math.max(1, Math.min(5, Math.ceil(heat)));
      const status = !c
        ? 'Wanted: stay out of trouble and it cools off'
        : c.stopped > 0.2
          ? `Pulling over… ${Math.round((c.stopped / 3) * 100)}%`
          : c.lost > 0.2
            ? `Losing him… ${Math.round((c.lost / 8) * 100)}%`
            : 'BUDDY is after you! Lose him, or stop and take the fine';
      hud.replaceChildren(
        el('b', '', `🚨 ${'★'.repeat(stars)}${'☆'.repeat(5 - stars)}`),
        el('span', '', status),
        ...(c ? [el('small', '', `Car damage ${Math.min(100, Math.round(c.damage))}%`)] : []),
      );
    };

    // ---------- reacting to what you do ----------

    const witness = (x: number, z: number, radius: number, react: (npc: Npc, d: number) => void) => {
      if (!active) return;
      for (const { npc, d } of near(x, z, radius)) {
        react(npc, d);
        npc.lookX = x;
        npc.lookZ = z;
      }
    };

    game.on('crash', (e) => {
      if (e.impact < 6) return;
      const where = placeName(e.x, e.z);
      tellNews(`the driver crashed hard near ${where}`);
      witness(e.x, e.z, 35, (npc) => {
        const fun = thrill(npc) > 0.15;
        remember(npc.mind, clock, fun ? `Saw the driver in ${carWords()} smash into something near ${where}. Wild!` : `The driver in ${carWords()} crashed near ${where}. Scary.`, 4 + Math.min(4, e.impact / 6), ['driver'], 'saw');
        feel(npc.mind, 'driver', fun ? 0.01 : -0.05);
        jolt(npc.mind, fun ? 0.02 : -0.06, fun ? 'amused' : 'worried');
        emote(npc, npc.mind.emotion);
        shout(npc, npc.mind.emotion);
      });
      offence('reckless driving', e.x, e.z, 30, 0.7);
      // Your passenger felt that.
      if (rider && e.impact > 8) {
        remember(rider.npc.mind, clock, `The driver crashed with me in the car! ${thrill(rider.npc) > 0.2 ? 'What a ride.' : 'Never again.'}`, 6, ['driver'], 'saw');
        feel(rider.npc.mind, 'driver', thrill(rider.npc) > 0.2 ? 0 : -0.06);
        game.toast(`😱 ${rider.npc.member.name}: "${pick(['Careful!', 'Whoa! Eyes on the road!', 'My neck!'])}"`, 1800);
      }
    });

    game.on('destroyed', (e) => {
      const where = placeName(e.x, e.z);
      const thing = e.kind === 'car' ? 'a parked car' : e.kind === 'building' ? 'a building' : `a ${e.kind}`;
      tellNews(`the driver wrecked ${thing} near ${where}`);
      witness(e.x, e.z, 45, (npc) => {
        const fun = thrill(npc) > 0.25;
        remember(npc.mind, clock, `The driver wrecked ${thing} near ${where}${fun ? '. Kind of amazing.' : '. Who pays for that?'}`, e.kind === 'building' ? 7 : 5, ['driver'], 'saw');
        feel(npc.mind, 'driver', fun ? 0 : -0.07);
        jolt(npc.mind, fun ? 0 : -0.08, fun ? 'excited' : 'annoyed');
        emote(npc, npc.mind.emotion);
        shout(npc, npc.mind.emotion);
      });
      offence(`smashing ${thing}`, e.x, e.z, 40, e.kind === 'building' ? 2 : e.kind === 'car' ? 1.2 : 0.8);
    });

    game.on('jump', (e) => {
      if (e.airTime < 0.8) return;
      const where = placeName(e.x, e.z);
      tellNews(`the driver flew ${e.airTime.toFixed(1)} seconds off a jump near ${where}`);
      witness(e.x, e.z, 70, (npc) => {
        const fun = thrill(npc) > -0.1;
        remember(npc.mind, clock, fun ? `Saw the driver fly through the air for ${e.airTime.toFixed(1)} seconds near ${where}!` : `The driver went flying near ${where}. Someone will get hurt.`, 3 + Math.min(5, e.airTime * 1.5), ['driver'], 'saw');
        feel(npc.mind, 'driver', fun ? 0.04 + e.airTime * 0.01 : -0.03);
        jolt(npc.mind, fun ? 0.08 : -0.03, fun ? 'excited' : 'worried');
        emote(npc, npc.mind.emotion);
        if (fun && !npc.convo && !npc.chatting) {
          npc.reaction = 'cheer';
          npc.reactionUntil = clock + 2.5;
        }
        shout(npc, npc.mind.emotion);
        if (fun) npc.mind.needs.fun = Math.min(1, npc.mind.needs.fun + 0.1);
      });
    });

    game.on('nearMiss', (e) => {
      witness(e.x, e.z, 22, (npc) => {
        remember(npc.mind, clock, `The driver in ${carWords()} skimmed past traffic right in front of me.`, 3, ['driver'], 'saw');
        const fun = thrill(npc) > 0.2;
        jolt(npc.mind, fun ? 0.03 : -0.04, fun ? 'excited' : 'scared');
        emote(npc, npc.mind.emotion, 8);
        shout(npc, npc.mind.emotion);
      });
      offence('dangerous driving', e.x, e.z, 25, 0.5);
    });

    game.on('explosion', (e) => {
      const where = placeName(e.x, e.z);
      tellNews(`there was an explosion near ${where}`);
      witness(e.x, e.z, 55, (npc, d) => {
        remember(npc.mind, clock, `An explosion went off near ${where}! The driver was right there.`, 8, ['driver'], 'saw');
        feel(npc.mind, 'driver', -0.1);
        jolt(npc.mind, -0.15, 'scared');
        emote(npc, 'scared', 25);
        if (d < 25 && !npc.chatting) dive(npc, npc.x - e.x, npc.z - e.z);
        shout(npc, 'scared');
      });
      offence('causing an explosion', e.x, e.z, 60, 2.5);
    });

    game.on('horn', (e) => {
      witness(e.x, e.z, 20, (npc) => {
        const likes = (npc.mind.feelings.driver ?? 0) > 0.25;
        if (likes && !npc.convo && !npc.chatting) {
          npc.reaction = 'wave';
          npc.reactionUntil = clock + 2;
        }
        emote(npc, likes ? 'happy' : 'annoyed', 6);
        shout(npc, likes ? 'happy' : 'annoyed');
      });
    });

    game.on('drift', (e) => {
      if (e.points < 300) return;
      witness(e.x, e.z, 40, (npc) => {
        const fun = thrill(npc) > 0;
        if (Math.random() < 0.4) remember(npc.mind, clock, fun ? `The driver drifted round the corner by ${placeName(e.x, e.z)}. Smooth!` : 'The driver was sliding all over the road again.', 2, ['driver'], 'saw');
        feel(npc.mind, 'driver', fun ? 0.015 : -0.01);
        emote(npc, fun ? 'amused' : 'annoyed', 8);
        shout(npc, npc.mind.emotion);
      });
    });

    /** Throws themselves out of the way. */
    const dive = (npc: Npc, dx: number, dz: number) => {
      const len = Math.hypot(dx, dz) || 1;
      npc.diveX = dx / len;
      npc.diveZ = dz / len;
      npc.reaction = 'dive';
      npc.reactionUntil = clock + 0.9;
    };

    // ---------- talking to each other ----------

    const pairKey = (a: Npc, b: Npc) => [a.member.id, b.member.id].sort().join('+');

    /** Whether two people stop to talk. */
    const wantToTalk = (a: Npc, b: Npc) => {
      if (a.convo || b.convo || a.chatting || b.chatting || a.reaction || b.reaction || a.asleep || b.asleep) return false;
      if (a.inCar || b.inCar || a.trip || b.trip || a.thinking || b.thinking) return false;
      // Too late to stop and chat: they're off to bed.
      if (bedtime(a) || bedtime(b)) return false;
      // BUDDY is always with GUY, so he talks to him now and then, and listens in on GUY's conversations.
      const officerPair = a.member.id === OFFICER || b.member.id === OFFICER;
      const partnerPair = officerPair && (a.member.id === PARTNER || b.member.id === PARTNER);
      if (officerPair && !partnerPair) return false;
      const gap = partnerPair ? 240 : 150;
      if (clock - (lastPair.get(pairKey(a, b)) ?? -Infinity) < gap) return false;
      const fondness = ((a.mind.feelings[b.member.id] ?? 0) + (b.mind.feelings[a.member.id] ?? 0)) / 2;
      const lonely = 2 - a.mind.needs.social - b.mind.needs.social;
      // Strangers sometimes get talking too, more so the outgoing and curious.
      const outgoing = (a.mind.traits.energy + b.mind.traits.energy + a.mind.traits.curiosity + b.mind.traits.curiosity) * 0.1;
      return fondness > -0.35 && lonely * 0.5 + fondness + outgoing > 0.5;
    };

    /** Which brain writes a conversation: the AI while it has room (conversations near you first), or the built-in one. */
    const brainFor = (a: Npc): Brain => {
      if (!aiReady()) return rules;
      const close = Math.hypot(a.x - game.player.x, a.z - game.player.z) < NEAR_RANGE;
      if (close) return aiTalks < 3 ? local : rules;
      return options.think === 'everywhere' && aiTalks < 1 ? local : rules;
    };

    const startTalk = (a: Npc, b: Npc) => {
      const convo: Convo = { people: [a, b], lines: null, index: 0, next: 0, started: clock };
      lastPair.set(pairKey(a, b), clock);
      for (const npc of convo.people) {
        npc.convo = convo;
        npc.path = [];
        if (npc.visit) npc.visit.talked = true;
      }
      a.lookX = b.x;
      a.lookZ = b.z;
      b.lookX = a.x;
      b.lookZ = a.z;
      a.label.say('…', 60, clock);
      const brain = brainFor(a);
      if (brain === local) aiTalks++;
      const where = placeName(a.x, a.z);
      void brain
        .conversation(person(a), person(b), moment())
        .catch(() => rules.conversation(person(a), person(b), moment()))
        .then((result) => {
          if (brain === local) aiTalks--;
          if (disposed || a.convo !== convo) return; // switched off, or interrupted while thinking
          hear(a, b, result, where);
          convo.lines = result.lines.length ? result.lines : [{ speaker: a.member.id, text: 'Hi!', emotion: 'happy' }];
          convo.next = clock;
        });
    };

    /** What a conversation leaves behind: memories, changed feelings, a plan, and a line in the journal. */
    const hear = (a: Npc, b: Npc, result: ConversationResult, where: string) => {
      const pair = [a, b];
      for (const m of result.memories) {
        const who = byId.get(m.who);
        const other = pair.find((p) => p !== who);
        if (who && other) remember(who.mind, clock, m.text, m.importance, [other.member.id, ...mentions(m.text)], 'talk');
      }
      for (const f of result.feelings) {
        const who = byId.get(f.from);
        if (who && (f.about === 'driver' || byId.has(f.about))) feel(who.mind, f.about, f.change * 0.06);
      }
      for (const npc of pair) {
        const other = pair.find((p) => p !== npc)!;
        npc.mind.met[other.member.id] = (npc.mind.met[other.member.id] ?? 0) + 1;
        // Good company: friends grow closer just by spending time together.
        if ((npc.mind.feelings[other.member.id] ?? 0) > -0.2) feel(npc.mind, other.member.id, 0.015);
      }
      if (result.plan) makePlan(a, b, result.plan);
      talks.unshift({
        t: clock,
        where,
        lines: result.lines.map((l) => ({ name: byId.get(l.speaker)?.member.name ?? l.speaker, text: l.text })),
        ai: result.ai,
      });
      if (talks.length > 30) talks.pop();
    };

    /** Cast members named in a bit of text (for whom a memory is about). */
    const namePatterns = CAST.map((c) => ({ id: c.id, re: new RegExp(`\\b(${c.name}|${c.name[0]}${c.name.slice(1).toLowerCase()})\\b`) }));
    const mentions = (text: string) => {
      const ids = namePatterns.filter((p) => p.re.test(text)).map((p) => p.id);
      if (/driver/i.test(text)) ids.push('driver');
      return ids;
    };

    /** Back on their way after stopping to talk (stopping dropped the route). */
    const resume = (npc: Npc) => {
      if (npc.plan.kind === 'go') npc.path = city.route(npc.x, npc.z, npc.plan.spot.x, npc.plan.spot.y);
      else if (npc.plan.kind === 'homeward' && npc.home) npc.path = city.route(npc.x, npc.z, npc.home.x, npc.home.z);
      else if (npc.plan.kind === 'trip' && npc.trip && !npc.inCar) npc.path = city.route(npc.x, npc.z, npc.trip.door.x, npc.trip.door.z);
    };

    const endTalk = (convo: Convo) => {
      for (const npc of convo.people) {
        npc.convo = null;
        npc.lookX = null;
        resume(npc);
        if (!convo.lines) npc.label.hush(); // the "…" while they were thinking
        npc.mind.needs.social = Math.min(1, npc.mind.needs.social + 0.3);
        jolt(npc.mind, 0.04, npc.mind.emotion);
      }
    };

    const playTalk = (convo: Convo) => {
      if (!convo.lines) {
        if (clock - convo.started > 70) endTalk(convo); // gave up waiting
        return;
      }
      if (clock < convo.next) return;
      const line = convo.lines[convo.index++];
      if (!line) {
        endTalk(convo);
        return;
      }
      const speaker = byId.get(line.speaker) ?? convo.people[0];
      const seconds = 1.8 + line.text.split(' ').length * 0.28;
      speaker.label.say(line.text, seconds, clock);
      emote(speaker, line.emotion, 15);
      convo.next = clock + seconds + 0.3;
      for (const npc of convo.people) npc.reaction = null;
    };

    // ---------- reflecting ----------

    /**
     * Thinks things over when a lot has happened. The AI does it one person at
     * a time, when it isn't busy with conversations; without the AI, the
     * built-in brain does.
     */
    const maybeReflect = (npc: Npc) => {
      if (npc.reflecting || npc.mind.pending < 28 || clock - npc.lastReflect < 120) return;
      if (ai?.state === 'ready') {
        if (!aiReady() || aiReflects > 0 || aiTalks > 1) return; // later, when the AI has a moment
        reflect(npc, local);
      } else reflect(npc, rules);
    };

    const reflect = (npc: Npc, brain: Brain) => {
      npc.reflecting = true;
      npc.lastReflect = clock;
      if (brain === local) aiReflects++;
      void brain
        .reflect(person(npc), moment())
        .catch(() => rules.reflect(person(npc), moment()))
        .then((result) => {
          if (brain === local) aiReflects--;
          npc.reflecting = false;
          if (disposed) return;
          npc.mind.pending = 0;
          for (const insight of result.insights) addInsight(npc.mind, clock, insight);
          if (result.trait) nudge(npc.mind, result.trait.name, result.trait.up, 0.04);
          if (result.habit) addHabit(npc.mind, result.habit);
          if (result.story) setStory(npc.mind, result.story);
          if (result.insights[0] && !npc.convo && !npc.chatting && !npc.asleep && !npc.inCar) npc.label.say(`💭 ${result.insights[0]}`, 5, clock, true);
        });
    };

    // ---------- talking to you, and making friends ----------

    /** Tells you when someone starts to count you as a friend (or stops). */
    const checkBond = (npc: Npc) => {
      const now = friendship(npc.mind);
      if (now === npc.bond) return;
      const ladder: Friendship[] = ['enemy', 'wary', 'stranger', 'acquaintance', 'friend', 'close friend', 'best friend'];
      const up = ladder.indexOf(now) > ladder.indexOf(npc.bond);
      npc.bond = now;
      if (up && isFriend(npc.mind)) {
        game.toast(`💙 ${npc.member.name} now counts you as ${now === 'friend' ? 'a friend' : `a ${now}`}!`, 3200);
        remember(npc.mind, clock, `I think the driver${npc.mind.driverName ? `, ${npc.mind.driverName},` : ''} and I are ${now === 'friend' ? 'friends' : `${now}s`} now.`, 7, ['driver'], 'insight');
        tellNews(`${npc.member.name} and the driver are ${now === 'friend' ? 'friends' : `${now}s`} now`);
      } else if (!up && (now === 'wary' || now === 'enemy')) {
        game.toast(`💔 ${npc.member.name} has gone off you`, 2600);
      }
    };

    const chat = buildChat(game, {
      send: async (npc: Npc, said: string) => {
        const brain: Brain = ai?.state === 'ready' ? local : rules;
        const result = await brain.chat(person(npc), said, moment()).catch(() => rules.chat(person(npc), said, moment()));
        if (disposed) return result.reply;
        const mind = npc.mind;
        if (result.driverName) mind.driverName = result.driverName;
        // Every friendly chat helps a little; how it went counts for more.
        feel(mind, 'driver', result.opinion * 0.1 + 0.03);
        remember(mind, clock, result.memory || `The driver stopped to chat with me.`, result.importance, ['driver', ...mentions(result.memory)], 'chat');
        logChat(mind, clock, said, result.reply);
        mind.needs.social = Math.min(1, mind.needs.social + 0.1);
        emote(npc, result.emotion, 15);
        npc.label.say(result.reply, 3 + result.reply.split(' ').length * 0.3, clock);
        checkBond(npc);
        return result.reply;
      },
      describe: (npc: Npc) => `${npc.contact === 'phone' ? '📱 ' : npc.contact === 'passenger' ? '🚗 ' : ''}${npc.member.role} · ${friendship(npc.mind)} · ${feelingWord(npc.mind.feelings.driver ?? 0)} you`,
      close: (npc: Npc) => {
        npc.chatting = false;
        npc.contact = 'beside';
        npc.lookX = null;
        resume(npc);
      },
    });

    /** Starts a chat with someone: clicked on, the nearest when you press K, your passenger, or by phone (a message you're answering). */
    const openChat = (npc: Npc, contact: Npc['contact'] = 'beside') => {
      if (!active || game.paused()) return;
      const p = game.player;
      if (npc.asleep) return game.toast(`${npc.member.name} is asleep at home`, 1800);
      if (contact === 'beside') {
        if (npc.inCar) return game.toast(`${npc.member.name} is out driving`, 1800);
        if (Math.hypot(npc.x - p.x, npc.z - p.z) > CHAT_RANGE) return game.toast(`Drive closer to talk to ${npc.member.name}`, 1800);
        if (Math.hypot(p.vx, p.vz) > 7) return game.toast('Slow down to talk', 1500);
      }
      if (npc.convo) endTalk(npc.convo);
      npc.chatting = true;
      npc.contact = contact;
      if (!npc.inCar && !npc.trip) npc.path = [];
      npc.reaction = null;
      chat.show(npc);
    };

    const startChat = () => {
      if (!active) return;
      if (chat.open) return chat.focus();
      if (rider) return openChat(rider.npc, 'passenger');
      const p = game.player;
      let best: Npc | null = null;
      let bestD = 14;
      for (const npc of npcs) {
        const d = Math.hypot(npc.x - p.x, npc.z - p.z);
        if (!npc.asleep && !npc.inCar && d < bestD) {
          bestD = d;
          best = npc;
        }
      }
      if (!best) return game.toast('Click on someone, or pull up next to them, to talk', 1800);
      openChat(best);
    };

    // Clicking on someone starts a chat (and doesn't fire the car's weapons).
    const raycaster = new THREE.Raycaster();
    const pointer = new THREE.Vector2();
    const onPointer = (e: PointerEvent) => {
      if (!active || e.button !== 0 || e.target !== game.renderer.domElement || game.paused()) return;
      const rect = game.renderer.domElement.getBoundingClientRect();
      pointer.set(((e.clientX - rect.left) / rect.width) * 2 - 1, -((e.clientY - rect.top) / rect.height) * 2 + 1);
      raycaster.setFromCamera(pointer, game.camera);
      const shown = npcs.filter((n) => n.body.root.visible).map((n) => n.body.root);
      for (const body of shown) body.updateMatrixWorld();
      const hit = raycaster.intersectObjects(shown, true)[0];
      let object: THREE.Object3D | null = hit?.object ?? null;
      while (object && !object.userData.npc) object = object.parent;
      const npc = object ? byId.get(object.userData.npc as string) : undefined;
      if (!npc) return;
      e.stopPropagation();
      e.preventDefault();
      openChat(npc);
    };
    window.addEventListener('pointerdown', onPointer, true);

    /** Friends wave and say hello when you drive slowly past. */
    const greetFriends = () => {
      const p = game.player;
      if (Math.hypot(p.vx, p.vz) > 14) return;
      for (const npc of npcs) {
        if (npc.asleep || npc.convo || npc.chatting || npc.reaction || clock - npc.lastGreet < 180 || !isFriend(npc.mind)) continue;
        if (Math.hypot(npc.x - p.x, npc.z - p.z) > 16) continue;
        npc.lastGreet = clock;
        npc.reaction = 'wave';
        npc.reactionUntil = clock + 2.2;
        npc.lookX = p.x;
        npc.lookZ = p.z;
        const name = npc.mind.driverName;
        npc.label.say(pick(name ? [`Hey ${name}!`, `${name}! Over here!`, `Morning, ${name}!`, `Looking good, ${name}!`] : ['Hey, friend!', 'Hi there!', 'Good to see you!']), 2.6, clock);
        emote(npc, 'happy', 8);
      }
    };

    // ---------- the journal ----------

    const plansWords = (id?: string) =>
      meetups
        .filter((m) => !id || m.ids.includes(id))
        .sort((a, b) => a.at - b.at)
        .map((m) => {
          const [a, b] = m.ids.map((i) => byId.get(i)?.member.name ?? i);
          const hours = Math.max(0, Math.round((m.at - clock) / HOUR));
          return `${a} and ${b}: ${placeById(m.placeId, m.kind)?.label ?? m.kind}, ${hours < 1 ? 'any minute now' : `in ${hours} h`} (${clockText(m.at)})`;
        });

    const journal = buildJournal(game, {
      npcs: () => npcs,
      now: () => clock,
      time: () => clockText(clock),
      doing,
      placeName: (npc) => placeName(npc.x, npc.z),
      footer: () => `${clockText(clock)} · ${aiWords(ai)}`,
      news: () => news,
      talks: () => talks,
      plans: plansWords,
      record: () =>
        `🚓 ${tickets} ticket${tickets === 1 ? '' : 's'}` +
        (record.chases ? ` · ${record.chases} chase${record.chases === 1 ? '' : 's'} (${record.escapes} escaped, ${record.busts} busted, ${record.fines} coins in fines)` : '') +
        (heat >= 0.5 ? ` · wanted ${'★'.repeat(Math.ceil(heat))}` : ''),
      messages: () => messages,
      find: (npc) => {
        beam.position.set(npc.x, 0, npc.z);
        beam.visible = true;
        beamUntil = clock + 12;
        const d = Math.round(Math.hypot(npc.x - game.player.x, npc.z - game.player.z));
        game.toast(`${npc.member.name} is ${npc.asleep ? 'asleep at home' : `at ${placeName(npc.x, npc.z)}`}, ${d} m away`, 2400);
      },
    });

    // A tall beam of light over someone you're looking for.
    const beam = new THREE.Mesh(
      new THREE.CylinderGeometry(0.7, 0.7, 80, 16, 1, true).translate(0, 40, 0),
      new THREE.MeshBasicMaterial({ color: 0x7cc8ff, transparent: true, opacity: 0.35, depthWrite: false, side: THREE.DoubleSide }),
    );
    beam.visible = false;
    let beamUntil = 0;
    game.scene.add(beam);

    game.onKey('KeyK', startChat);
    game.onKey('KeyL', () => active && journal.toggle());

    // ---------- people's cars ----------

    const trips: Trip[] = [];
    const parked: NpcCar[] = []; // cars left at the kerb after a trip, taken away once nobody's looking
    const carsOut = () => trips.filter((t) => t.car).length + parked.length;
    /** Whether two people are together (BUDDY at GUY's side). */
    const together = (a: Npc, b: Npc) => !a.asleep && !b.asleep && !a.inCar && !b.inCar && !a.trip && !b.trip && Math.hypot(a.x - b.x, a.z - b.z) < 15;

    /**
     * Sets off on a car trip: walk to a car at the nearest kerb, drive there,
     * park, get out. GUY and BUDDY go in BUDDY's police car, BUDDY driving.
     * Returns false if no car can be had (too many on the road).
     */
    const startTrip = (who: Npc, dest: Place | 'home', riders: Npc[] = []): boolean => {
      if (!active || clearing || chase || who.inCar || who.trip) return false;
      const officer = byId.get(OFFICER);
      const partner = byId.get(PARTNER);
      const dests = new Map<Npc, Place | 'home' | null>([[who, dest], ...riders.map((r) => [r, dest] as [Npc, Place | 'home'])]);
      let driver = who;
      let usePolice = false;
      if (officer && partner && (who === officer || (who === partner && together(officer, partner)))) {
        // BUDDY drives (back to GUY's side when they get out); GUY rides along if he's with him.
        usePolice = true;
        driver = officer;
        dests.set(officer, null);
        if (who === partner) riders = [partner, ...riders];
        else if (together(officer, partner)) {
          riders = [partner, ...riders];
          dests.set(partner, dest);
        }
      } else if (officer && partner && riders.includes(partner) && together(officer, partner)) {
        // GUY's getting a lift: BUDDY squeezes in too.
        riders = [...riders, officer];
        dests.set(officer, null);
      }
      if (usePolice && police && police.wrecked > 0) return false;
      if (!usePolice && carsOut() >= MAX_CARS) return false;
      let car: NpcCar | null = null;
      let door: { x: number; z: number };
      if (usePolice) {
        const cop = cruiser();
        if (Math.hypot(cop.x - driver.x, cop.z - driver.z) > 40 || cop.wrecked > 0) {
          const spot = parkingNear(driver.x, driver.z);
          cop.place(spot.x, spot.z, spot.heading);
        }
        cop.mode = 'parked';
        door = doorOf(cop.x, cop.z, cop.physics.heading);
      } else {
        const spot = parkingNear(driver.x, driver.z);
        car = new NpcCar(game.world, spot.x, spot.z, spot.heading);
        door = spot.door;
      }
      const trip: Trip = { driver, riders, car, phase: 'walk', door, to: dest, dests, started: clock };
      for (const npc of [driver, ...riders]) {
        leaveVisit(npc);
        if (npc.convo) endTalk(npc.convo);
        npc.trip = trip;
        npc.thinking = false;
        npc.plan = { kind: 'trip' };
        npc.path = city.route(npc.x, npc.z, door.x, door.z);
      }
      trips.push(trip);
      return true;
    };

    /** Where a trip's car is now. */
    const carOf = (trip: Trip) => (trip.car ? { x: trip.car.x, z: trip.car.z, heading: trip.car.heading } : { x: police!.x, z: police!.z, heading: police!.physics.heading });

    /** Everyone who's near the car gets in, and off they go. */
    const board = (trip: Trip) => {
      trip.riders = trip.riders.filter((npc) => {
        const ok = !npc.asleep && !npc.chatting && Math.hypot(npc.x - trip.door.x, npc.z - trip.door.z) < 14;
        if (!ok) leaveTrip(npc, trip);
        return ok;
      });
      for (const npc of [trip.driver, ...trip.riders]) {
        if (npc.convo) endTalk(npc.convo);
        npc.inCar = true;
        npc.path = [];
        npc.label.hush();
      }
      const home = trip.riders.find((n) => trip.dests.get(n) === 'home')?.home ?? trip.driver.home;
      const to = trip.to === 'home' ? (home ?? { x: trip.driver.x, z: trip.driver.z }) : trip.to;
      if (trip.car) trip.car.driveTo(to.x, to.z);
      else {
        const cop = police!;
        cop.setRoute(routeTo(cop.x, cop.z, cop.physics.heading, to.x, to.z));
        cop.mode = 'route';
      }
      trip.phase = 'drive';
      trip.started = clock;
    };

    /** Someone who didn't make it to the car goes their own way. */
    const leaveTrip = (npc: Npc, trip: Trip) => {
      npc.trip = null;
      const dest = trip.dests.get(npc);
      if (dest === 'home') travelHome(npc, hourOf(clock) < 19);
      else if (dest) goTo(npc, dest);
      else ruleNext(npc);
    };

    /** Out of the car at the kerb, and on to where each of them is going. */
    const alight = (trip: Trip, scared = false) => {
      const at = carOf(trip);
      trips.splice(trips.indexOf(trip), 1);
      [trip.driver, ...trip.riders].forEach((npc, i) => {
        const door = doorOf(at.x, at.z, at.heading, i);
        npc.inCar = false;
        npc.trip = null;
        npc.x = door.x;
        npc.z = door.z;
        if (scared) {
          remember(npc.mind, clock, `The car I was in got wrecked near ${placeName(at.x, at.z)}! ${Math.hypot(game.player.x - at.x, game.player.z - at.z) < 40 ? 'The driver did it.' : ''}`, 8, ['driver'], 'saw');
          if (Math.hypot(game.player.x - at.x, game.player.z - at.z) < 40) feel(npc.mind, 'driver', -0.15);
          jolt(npc.mind, -0.15, 'scared');
          emote(npc, 'scared', 25);
          npc.label.say(pick(['My car!', 'Are you kidding me?!', 'Everyone out!']), 2.5, clock);
        }
        const dest = trip.dests.get(npc);
        if (dest === 'home') arriveHome(npc);
        else if (dest) goTo(npc, dest);
        else ruleNext(npc);
      });
      if (!scared) remember(trip.driver.mind, clock, `Drove ${trip.riders.length ? `${trip.riders.map((r) => r.member.name).join(' and ')} ` : ''}${trip.to === 'home' ? 'home' : `to ${trip.to.label}`}.`, 2, trip.riders.map((r) => r.member.id), 'saw');
      if (trip.car) {
        trip.car.parkedAt = clock;
        parked.push(trip.car);
      } else if (police && police.mode === 'route') police.mode = 'parked';
    };
    /** The car got wrecked on the way: everyone out, shaken. */
    const bail = (trip: Trip) => alight(trip, true);

    /** Called off before they got going (or the pack is pausing): back on foot. */
    const cancelTrip = (trip: Trip) => {
      if (trip.phase === 'drive') return alight(trip);
      trips.splice(trips.indexOf(trip), 1);
      for (const npc of [trip.driver, ...trip.riders]) leaveTrip(npc, trip);
      if (trip.car) {
        trip.car.parkedAt = clock;
        parked.push(trip.car);
      }
    };

    const updateTrips = (dt: number) => {
      const p = game.player;
      const night = game.world.isNight;
      const obstacles: (Obstacle & { trip?: Trip })[] = [{ x: p.x, z: p.z }];
      for (const t of trips) if (t.phase === 'drive') obstacles.push({ ...carOf(t), trip: t });
      for (const c of parked) obstacles.push({ x: c.x, z: c.z });
      // The city's traffic (cars the world moves about: worth 15000, see World.addTrafficCar), but not people's own cars.
      if (trips.some((t) => t.car && t.phase === 'drive')) {
        const own = new Set(trips.map((t) => t.car?.target));
        for (const t of game.world.targets) {
          if (t.kind === 'car' && t.value === 15000 && t.alive && !own.has(t)) obstacles.push({ x: t.rig.root.position.x, z: t.rig.root.position.z });
        }
      }
      if (police?.rig.root.visible && !trips.some((t) => !t.car)) obstacles.push({ x: police.x, z: police.z });
      for (const trip of [...trips]) {
        if (trip.phase === 'walk') {
          const d = trip.driver;
          if (d.path.length === 0 || Math.hypot(d.x - trip.door.x, d.z - trip.door.z) < 1.5) board(trip);
          else if (clock - trip.started > 150) cancelTrip(trip); // couldn't get there
          continue;
        }
        const at = carOf(trip);
        const unseen = Math.hypot(at.x - game.camera.position.x, at.z - game.camera.position.z) > SHOW_RANGE;
        let arrived = false;
        if (trip.car) {
          if (trip.car.wrecked) {
            bail(trip);
            continue;
          }
          arrived = trip.car.update(
            dt,
            obstacles.filter((o) => o.trip !== trip),
            night,
            unseen,
            clock,
          );
        } else {
          const cop = police!;
          if (cop.wrecked > 0) {
            bail(trip);
            continue;
          }
          arrived = cop.update(dt, p, false).arrived;
          cop.hearing = Math.hypot(cop.x - game.camera.position.x, cop.z - game.camera.position.z);
        }
        const now = carOf(trip);
        for (const npc of [trip.driver, ...trip.riders]) {
          npc.x = now.x;
          npc.z = now.z;
          npc.heading = now.heading;
        }
        // Taking far too long (stuck somewhere): they park where they are and walk the rest.
        if (arrived || clock - trip.started > (unseen ? 150 : 300)) alight(trip);
      }
      // Cars left parked go once nobody's looking.
      for (let i = parked.length - 1; i >= 0; i--) {
        const c = parked[i];
        c.update(dt, [], night, true, clock);
        const seen = Math.hypot(c.x - game.camera.position.x, c.z - game.camera.position.z) < SHOW_RANGE;
        if (!seen && (clock - c.parkedAt > 60 || parked.length > 2)) {
          c.dispose();
          parked.splice(i, 1);
        }
      }
      // The police car: parked when not in use (and towed away out of sight once wrecked).
      const cop = police;
      if (cop && !chase && !trips.some((t) => !t.car)) {
        const d = Math.hypot(cop.x - game.camera.position.x, cop.z - game.camera.position.z);
        cop.setActive(d < SHOW_RANGE + 40);
        if (cop.rig.root.visible) cop.update(dt, p, false);
        if (cop.hp <= 0) wreckPolice();
      } else if (cop) {
        cop.setActive(true);
        if (cop.hp <= 0) wreckPolice();
      }
    };

    /** Clears every car off the roads (the pack pausing for an event, or the city being reset). */
    let clearing = false; // nobody sets off in a new car meanwhile
    const clearRoads = () => {
      clearing = true;
      if (chase) endChase('off');
      for (const trip of [...trips]) cancelTrip(trip);
      for (const c of parked) c.dispose();
      parked.length = 0;
      police?.setActive(false);
      if (rider) dropOff(false);
      hail = null;
      clearing = false;
    };

    // ---------- giving people a lift ----------

    let hail: { npc: Npc; place: Place; until: number } | null = null;
    let rider: { npc: Npc; place: Place; since: number } | null = null;
    let lastHail = -Infinity;

    /**
     * People on their way somewhere far wave you down when you drive slowly
     * past. Stop beside them and they hop in; drop them off there (stop near
     * the place) for a tip and a friend.
     */
    const lifts = () => {
      const p = game.player;
      const speed = Math.hypot(p.vx, p.vz);
      if (rider) {
        const r = rider;
        if (speed < 1.5 && Math.hypot(p.x - r.place.x, p.z - r.place.z) < 35) dropOff(true);
        else if (clock - r.since > 480) {
          game.toast(`${r.npc.member.name}: "Just let me out here, I'll walk."`, 2600);
          dropOff(false);
        }
        return;
      }
      if (hail) {
        const h = hail;
        const d = Math.hypot(h.npc.x - p.x, h.npc.z - p.z);
        if (clock > h.until || d > 30 || h.npc.convo || h.npc.inCar || h.npc.asleep) hail = null;
        else if (speed < 1.5 && d < 10) pickUp(h);
        return;
      }
      if (chase || speed > 9 || clock - lastHail < 90) return;
      for (const { npc, d } of near(p.x, p.z, 12)) {
        if (d < 3 || npc.plan.kind !== 'go' || npc.convo || npc.chatting || npc.inCar || npc.trip || npc.thinking) continue;
        if (npc.member.id === OFFICER || npc.member.id === PARTNER || (npc.mind.feelings.driver ?? 0) < -0.1) continue;
        const place = npc.plan.place;
        if (Math.hypot(place.x - npc.x, place.z - npc.z) < DRIVE_FROM) continue;
        lastHail = clock;
        hail = { npc, place, until: clock + 20 };
        npc.reaction = 'wave';
        npc.reactionUntil = clock + 3;
        npc.lookX = p.x;
        npc.lookZ = p.z;
        npc.label.say(`Hey! Any chance of a lift to ${place.label}?`, 4, clock);
        game.toast(`🙋 ${npc.member.name} wants a lift to ${place.label}: stop next to them`, 3200);
        return;
      }

      function pickUp(h: { npc: Npc; place: Place }) {
        const npc = h.npc;
        hail = null;
        leaveVisit(npc);
        npc.inCar = true;
        npc.path = [];
        npc.label.hush();
        npc.plan = { kind: 'ride', place: h.place };
        rider = { npc, place: h.place, since: clock };
        beam.position.set(h.place.x, 0, h.place.z);
        beam.visible = true;
        beamUntil = Infinity;
        game.toast(`🚗 ${npc.member.name} hopped in. Drop them at ${h.place.label} (K to chat)`, 3200);
        remember(npc.mind, clock, `The driver picked me up in ${carWords()} to take me to ${h.place.label}.`, 5, ['driver'], 'saw');
      }
    };

    /** Your passenger gets out: there (a tip and a friend), or not. */
    const dropOff = (there: boolean) => {
      const r = rider;
      if (!r) return;
      rider = null;
      beam.visible = false;
      beamUntil = 0;
      const p = game.player;
      const h = p.heading;
      const npc = r.npc;
      npc.inCar = false;
      npc.x = p.x - Math.cos(h) * 2.6;
      npc.z = p.z + Math.sin(h) * 2.6;
      if (npc.chatting) chat.hide();
      if (there) {
        const tip = game.addCoins(15 + Math.round(Math.max(0, npc.mind.feelings.driver ?? 0) * 40));
        feel(npc.mind, 'driver', 0.12);
        npc.mind.met.driver = (npc.mind.met.driver ?? 0) + 1;
        remember(npc.mind, clock, `The driver gave me a lift to ${r.place.label}. What a star!`, 6, ['driver'], 'saw');
        npc.label.say(pick(['Thanks for the ride!', 'You are a lifesaver!', 'Best taxi in town!']), 3, clock);
        game.toast(`💰 ${npc.member.name} tipped you ${tip} coins`, 2600);
        tellNews(`the driver gave ${npc.member.name} a lift to ${r.place.label}`);
        checkBond(npc);
      } else {
        feel(npc.mind, 'driver', -0.03);
        remember(npc.mind, clock, `The driver was meant to take me to ${r.place.label}, but I had to get out and walk.`, 4, ['driver'], 'saw');
      }
      goTo(npc, r.place);
    };

    // ---------- the city's moments: the street party, and your new car ----------

    let partyDay = -1;
    let lookKey = '';
    let lookChanged = -Infinity;
    const lookSeen = new Set<Npc>();
    const carKey = () => {
      const c = game.playerCar().config;
      return `${c.body}|${c.paintColor}|${c.pattern}|${c.finish}|${c.size}`;
    };

    const moments = () => {
      // Saturday night: the whole city heads to the park.
      if (partyTime() && partyDay !== dayOf(clock)) {
        partyDay = dayOf(clock);
        const park = partyPlace();
        if (park) {
          tellNews(`the Saturday street party is on at ${park.label}`);
          game.toast(`🎉 The Saturday street party is on at ${park.label}! Everyone's going.`, 4000);
          for (const npc of npcs) if (npc.plan.kind === 'stay' && npc.plan.place !== park) npc.plan.until = Math.min(npc.plan.until, clock + Math.random() * 20);
        }
      }
      if (partyTime()) {
        const park = partyPlace();
        for (const npc of npcs) {
          if (npc.plan.kind !== 'stay' || npc.plan.place !== park || npc.convo || npc.reaction || Math.random() > 0.03) continue;
          npc.reaction = 'cheer';
          npc.reactionUntil = clock + 2.5;
          npc.mind.needs.fun = Math.min(1, npc.mind.needs.fun + 0.05);
          npc.mind.needs.social = Math.min(1, npc.mind.needs.social + 0.03);
        }
      }
      // A new car (or a new look): people notice.
      const key = carKey();
      if (lookKey && key !== lookKey) {
        lookChanged = clock;
        lookSeen.clear();
      }
      lookKey = key;
      if (clock - lookChanged < 150) {
        const p = game.player;
        for (const { npc } of near(p.x, p.z, 18)) {
          if (lookSeen.has(npc) || npc.inCar || npc.convo || npc.chatting) continue;
          lookSeen.add(npc);
          const fun = thrill(npc) > 0;
          const name = isFriend(npc.mind) ? npc.mind.driverName : '';
          const line = isFriend(npc.mind)
            ? `New wheels${name ? `, ${name}` : ''}? ${pick(['Love it!', 'Very you.', 'Bold choice!'])}`
            : fun
              ? pick(['Ooh, nice car!', "Now that's a car!", 'Look at that thing!'])
              : pick(['Hmm. Interesting colour.', 'Is that thing even legal?', 'Another new car. Of course.']);
          npc.label.say(line, 3, clock);
          npc.lookX = p.x;
          npc.lookZ = p.z;
          remember(npc.mind, clock, `The driver showed up in a new car: ${carWords()}.`, 3, ['driver'], 'saw');
          if (fun) feel(npc.mind, 'driver', 0.02);
        }
      }
      if (messageCard && clock > messageCard.until) {
        messageCard = null;
        phone.classList.add('hidden');
      }
    };

    // ---------- every frame ----------

    const target = new THREE.Vector2();

    const walk = (npc: Npc, dt: number): { action: Action; speed: number } => {
      if (npc.asleep || npc.inCar) return { action: 'stand', speed: 0 };
      // Diving, cheering and waving come first.
      if (npc.reaction && clock < npc.reactionUntil) {
        if (npc.reaction === 'dive') {
          const step = 5 * dt * Math.max(0, (npc.reactionUntil - clock) / 0.9);
          npc.x += npc.diveX * step;
          npc.z += npc.diveZ * step;
          npc.heading = Math.atan2(npc.diveX, npc.diveZ);
        }
        return { action: npc.reaction, speed: 0 };
      }
      npc.reaction = null;
      if (npc.chatting) {
        // Beside your car they look at you; on the phone, wherever.
        if (npc.contact === 'beside') {
          npc.lookX = game.player.x;
          npc.lookZ = game.player.z;
        }
        return { action: 'talk', speed: 0 };
      }
      if (npc.convo) {
        const speaking = npc.convo.lines && npc.label.talking && npc.convo.lines[npc.convo.index - 1]?.speaker === npc.member.id;
        return { action: speaking ? 'talk' : 'listen', speed: 0 };
      }
      // Where to next?
      let running = false;
      if (npc.plan.kind === 'follow') {
        const plan = npc.plan;
        const leader = byId.get(plan.who);
        const guarding = npc.member.id === OFFICER && plan.who === PARTNER; // BUDDY at GUY's side
        if (!leader || leader.asleep || leader.inCar || (plan.until !== undefined && clock > plan.until)) {
          if (guarding) return { action: 'stand', speed: 0 };
          fines.delete(plan.who);
          if (npc.member.id === OFFICER) npc.plan = { kind: 'follow', who: PARTNER };
          else decide(npc, plan.why === 'talk' ? `couldn't find ${leader?.member.name ?? 'them'}` : `done tagging along with ${leader?.member.name ?? 'them'}`);
          return { action: 'stand', speed: 0 };
        }
        if (guarding) {
          // At GUY's side, a step to his right.
          const side = leader.heading - Math.PI / 2;
          target.set(leader.x + Math.sin(side) * 1.5, leader.z + Math.cos(side) * 1.5);
        } else {
          // A step behind them.
          target.set(leader.x - Math.sin(leader.heading) * 1.6, leader.z - Math.cos(leader.heading) * 1.6);
        }
        const d = Math.hypot(target.x - npc.x, target.y - npc.z);
        if (plan.why === 'arrest' && Math.hypot(leader.x - npc.x, leader.z - npc.z) < 2.5) {
          fine(npc, leader);
          return { action: 'stand', speed: 0 };
        }
        if (plan.why === 'talk' && Math.hypot(leader.x - npc.x, leader.z - npc.z) < 2.8) {
          // Found them: a chat (if they're free), then on with the day.
          npc.plan = { kind: 'pause', until: clock + 4 };
          if (!leader.convo && !leader.chatting && !leader.trip && !leader.thinking) startTalk(npc, leader);
          return { action: 'stand', speed: 0 };
        }
        if (leader.convo || leader.chatting) {
          npc.lookX = leader.x;
          npc.lookZ = leader.z;
          if (d < 2.5) return { action: 'listen', speed: 0 };
        } else if (d < 0.6) {
          npc.heading = leader.heading;
          return { action: 'stand', speed: 0 };
        }
        running = d > 14 || plan.why === 'arrest';
        npc.path = [target];
      } else if (npc.path.length === 0) {
        if (npc.plan.kind === 'go') arrive(npc, npc.plan.place, npc.plan.meet);
        else if (npc.plan.kind === 'wander') decide(npc, 'finished a stroll'); // got where they were wandering to
        else if (npc.plan.kind === 'homeward') {
          if (npc.plan.nap && !bedtime(npc)) sleep(npc, 1 + Math.random());
          else sleep(npc);
          return { action: 'stand', speed: 0 };
        } else if (npc.plan.kind === 'trip' && npc.trip) {
          // Waiting by the car for the others.
          const car = carOf(npc.trip);
          npc.heading += wrap(Math.atan2(car.x - npc.x, car.z - npc.z) - npc.heading) * Math.min(1, dt * 4);
        }
        if (npc.path.length === 0) {
          // Hanging around: face the middle of the place, or whatever caught their eye.
          if (npc.plan.kind === 'stay') {
            const place = npc.plan.place;
            const want = Math.atan2(place.x - npc.x, place.z - npc.z);
            npc.heading += wrap(want - npc.heading) * Math.min(1, dt * 2);
          }
          return { action: 'stand', speed: 0 };
        }
      }
      const next = npc.path[0];
      const dx = next.x - npc.x;
      const dz = next.y - npc.z;
      const d = Math.hypot(dx, dz);
      // Hurrying to a meetup, or home late at night.
      const hurry = (npc.plan.kind === 'go' && npc.plan.meet && clock > npc.plan.meet.at) || npc.plan.kind === 'homeward';
      // Out of sight, people walk faster, so a trip across town fits in the city's short day.
      const unseen = npc.body.root.visible ? 1 : 4;
      const speed = running ? 4.2 : (1.2 + npc.mind.traits.energy * 0.6) * (hurry ? 1.5 : 1) * unseen;
      if (d < 0.35) {
        npc.path.shift();
        return { action: 'stand', speed: 0 };
      }
      const step = Math.min(d, speed * dt);
      npc.x += (dx / d) * step;
      npc.z += (dz / d) * step;
      npc.heading += wrap(Math.atan2(dx, dz) - npc.heading) * Math.min(1, dt * 8);
      return { action: running ? 'run' : 'walk', speed: speed / 1.4 };
    };

    /** Gets out of the way of the player's car: dives aside, and remembers it. */
    const dodge = (npc: Npc) => {
      const p = game.player;
      const speed = Math.hypot(p.vx, p.vz);
      if (speed < 4 || npc.reaction === 'dive' || npc.chatting || npc.asleep || npc.inCar) return;
      const reach = 1.6 * Math.max(1, game.playerCar().config.size);
      const rx = npc.x - p.x;
      const rz = npc.z - p.z;
      if (Math.abs(rx) > 40 || Math.abs(rz) > 40) return;
      const along = (rx * p.vx + rz * p.vz) / speed;
      const across = (rx * p.vz - rz * p.vx) / speed;
      if (along < -reach || along > Math.min(14, speed * 0.7) + reach || Math.abs(across) > reach + 1.2) return;
      const side = across === 0 ? (Math.random() < 0.5 ? -1 : 1) : Math.sign(across);
      dive(npc, (p.vz / speed) * side, (-p.vx / speed) * side);
      if (npc.convo) endTalk(npc.convo);
      if (clock - npc.lastScare > 15) {
        npc.lastScare = clock;
        const hit = Math.hypot(rx, rz) < reach;
        remember(npc.mind, clock, hit ? `The driver in ${carWords()} knocked me flat at ${placeName(npc.x, npc.z)}!` : `The driver in ${carWords()} nearly ran me over at ${placeName(npc.x, npc.z)}!`, hit ? 9 : 7, ['driver'], 'saw');
        feel(npc.mind, 'driver', hit ? -0.25 : -0.1);
        jolt(npc.mind, hit ? -0.3 : -0.15, 'scared');
        emote(npc, 'scared', 25);
        npc.label.say(pick(hit ? ['Ow!', 'Hey!!', 'My back!'] : ['Whoa!', 'Look out!', 'Hey!']), 2, clock);
        tellNews(`the driver nearly ran over ${npc.member.name}`);
        for (const { npc: other } of near(npc.x, npc.z, 25)) {
          if (other !== npc) remember(other.mind, clock, `The driver nearly ran over ${npc.member.name}!`, 6, ['driver', npc.member.id], 'saw');
        }
        offence(hit ? `hitting ${npc.member.name}` : 'nearly hitting someone', npc.x, npc.z, 30, hit ? 2.5 : 1.2);
        checkBond(npc);
      }
    };

    let lastBeat = 0;
    const think = (dt: number) => {
      const p = game.player;
      const partner = byId.get(PARTNER);
      for (const npc of npcs) {
        const m = npc.mind;
        const staying = npc.plan.kind === 'stay' ? npc.plan.place.kind : null;
        tickNeeds(m, dt, {
          social: !!npc.convo || npc.chatting,
          eating: staying === 'diner' || staying === 'coffee' || staying === 'bakery',
          resting: npc.asleep || staying === 'park' || (staying !== null && staying !== npc.member.job && !npc.path.length),
          fun: staying === 'stunts' || staying === 'music' || staying === 'gym' || npc.reaction === 'cheer',
        });
        if (npc.asleep) m.needs.rest = Math.min(1, m.needs.rest + dt / (5 * HOUR));
        if (clock > npc.emotionUntil && m.emotion !== 'calm') m.emotion = m.mood < -0.3 ? 'sad' : 'calm';
        if (npc.member.id === OFFICER) {
          // BUDDY sleeps when GUY does (he lives next door).
          if (partner && npc.asleep !== partner.asleep && !npc.inCar && !npc.trip && !partner.inCar) {
            npc.asleep = partner.asleep;
            npc.x = partner.x + 1.4;
            npc.z = partner.z + 0.6;
          }
          // Now and then, on the beat, he thinks about what to do with his day.
          const free = !npc.asleep && !npc.inCar && !npc.trip && !npc.thinking && !npc.chatting && !npc.convo && npc.plan.kind === 'follow' && npc.plan.who === PARTNER;
          if (free && clock - lastBeat > 240) {
            lastBeat = clock;
            const close = Math.hypot(npc.x - p.x, npc.z - p.z) < DECIDE_RANGE;
            if (close && aiReady() && aiDecides === 0) policeConsider('Out on the beat with GUY. What now?', false);
            else if (Math.random() < 0.3) patrol(npc);
          }
        } else if (npc.plan.kind === 'sleep') {
          if (clock >= npc.plan.until) wake(npc);
        } else if (!npc.convo && !npc.chatting && !npc.trip && !npc.inCar) {
          if (npc.plan.kind === 'pause') {
            if (clock > npc.plan.until) {
              // Still no answer from the AI: they make up their own mind.
              const waited = npc.thinking;
              npc.thinking = false;
              if (waited) ruleNext(npc);
              else decide(npc);
            }
          } else if (npc.plan.kind !== 'homeward' && bedtime(npc)) travelHome(npc);
          else if (npc.plan.kind === 'stay' && clock > npc.plan.until) decide(npc, `has spent a while at ${npc.plan.place.label}`);
        }
        planDay(npc);
        // A lot on their mind: time to think it over (asleep counts: they dream on it).
        maybeReflect(npc);
        // Speeding past the police.
        if (npc.member.id === OFFICER && !npc.asleep && clock - lastSpeeding > 6 && Math.hypot(p.vx, p.vz) > 32 && Math.hypot(npc.x - p.x, npc.z - p.z) < (npc.inCar ? 45 : 22)) {
          lastSpeeding = clock;
          offence('speeding', npc.x, npc.z, 45, 0.4);
        }
        npc.label.setFriend(isFriend(m));
      }
      keepPlans();
      greetFriends();
      lifts();
      moments();
      updateHud();
      // People who bump into each other may stop for a chat.
      for (let i = 0; i < npcs.length; i++) {
        const a = npcs[i];
        if (a.asleep || a.convo) continue;
        for (let j = i + 1; j < npcs.length; j++) {
          const b = npcs[j];
          if (Math.abs(a.x - b.x) < 4 && Math.abs(a.z - b.z) < 4 && Math.hypot(a.x - b.x, a.z - b.z) < 4 && wantToTalk(a, b)) {
            if (Math.random() < 0.5) startTalk(a, b);
            else startTalk(b, a);
            break;
          }
        }
      }
    };

    /** Puts someone's body where they are. */
    const pose = (npc: Npc) => {
      npc.body.root.position.set(npc.x, game.world.heightAt(npc.x, npc.z), npc.z);
      npc.body.root.rotation.y = npc.heading;
    };

    /** Only the people near the camera are drawn and animated. */
    const shown = (npc: Npc) => active && !npc.asleep && !npc.inCar && Math.hypot(npc.x - game.camera.position.x, npc.z - game.camera.position.z) < SHOW_RANGE;

    const update = (dt: number) => {
      clock += dt;
      if (!chase) heat = Math.max(0, heat - dt * 0.012); // cools off: five stars to none in about seven minutes
      thinkTimer += dt;
      if (thinkTimer > 0.25) {
        think(thinkTimer);
        thinkTimer = 0;
      }
      for (const npc of npcs) if (npc.convo && npc.convo.people[0] === npc) playTalk(npc.convo);
      updateTrips(dt);
      updateChase(dt);
      if (rider) {
        rider.npc.x = game.player.x;
        rider.npc.z = game.player.z;
      }
      for (const npc of npcs) {
        dodge(npc);
        const { action, speed } = walk(npc, dt);
        const visible = shown(npc);
        npc.body.root.visible = visible;
        if (!visible) continue;
        pose(npc);
        const look = npc.lookX === null ? null : wrap(Math.atan2(npc.lookX - npc.x, npc.lookZ - npc.z) - npc.heading);
        npc.body.animate(dt, action, speed, look);
        npc.label.setEmotion(npc.mind.emotion);
      }
      if (beam.visible && clock > beamUntil) beam.visible = false;
      if (chat.open && chat.npc?.contact === 'beside' && Math.hypot(chat.npc.x - game.player.x, chat.npc.z - game.player.z) > CHAT_RANGE + 20) chat.hide();
    };

    const placeLabels = () => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      for (const npc of npcs) {
        if (npc.inCar) {
          // The driver's name over the car.
          const driving = (npc.trip?.driver === npc && npc.trip.phase === 'drive') || chase?.crew[0] === npc;
          const seen = driving && Math.hypot(npc.x - game.camera.position.x, npc.z - game.camera.position.z) < SHOW_RANGE;
          npc.label.place(npc.x, game.world.heightAt(npc.x, npc.z) + 2.3, npc.z, game.camera, w, h, clock, seen, options.tags);
          continue;
        }
        npc.label.place(npc.x, npc.body.root.position.y + npc.body.top + 0.12, npc.z, game.camera, w, h, clock, npc.body.root.visible, options.tags);
      }
    };

    const setActive = (on: boolean) => {
      active = on;
      layer.style.display = on ? '' : 'none';
      city.setVisible(on);
      for (const npc of npcs) {
        npc.body.root.visible = shown(npc);
        if (npc.body.root.visible) pose(npc);
      }
      if (!on) {
        clearRoads();
        chat.hide();
        journal.hide();
        beam.visible = false;
      }
    };

    // ---------- saving: in the game's data folder, for as long as it's installed ----------

    const snapshot = (): Saved => {
      const minds: Record<string, Mind> = {};
      for (const npc of npcs) minds[npc.member.id] = npc.mind;
      return { version: 3, savedAt: Date.now(), clock, minds, tickets, meetups, news, talks, heat, crimes, record, messages };
    };

    /** Takes in a saved city (from the data folder, or from older versions that kept it in the browser storage). */
    const restore = (saved: Partial<Saved> | null) => {
      if (!saved || typeof saved !== 'object') return;
      clock = typeof saved.clock === 'number' && Number.isFinite(saved.clock) ? saved.clock : 0;
      tickets = typeof saved.tickets === 'number' ? saved.tickets : 0;
      // Version 3 additions (an older city starts with a clean record).
      heat = typeof saved.heat === 'number' && Number.isFinite(saved.heat) ? Math.max(0, Math.min(5, saved.heat)) : 0;
      crimes.splice(0, crimes.length, ...(Array.isArray(saved.crimes) ? saved.crimes.filter((c) => typeof c === 'string').slice(0, 6) : []));
      const r = saved.record;
      record.chases = typeof r?.chases === 'number' ? r.chases : 0;
      record.escapes = typeof r?.escapes === 'number' ? r.escapes : 0;
      record.busts = typeof r?.busts === 'number' ? r.busts : 0;
      record.fines = typeof r?.fines === 'number' ? r.fines : 0;
      messages.splice(0, messages.length, ...(Array.isArray(saved.messages) ? saved.messages.filter((m) => m && typeof m.text === 'string' && typeof m.from === 'string').slice(0, 20) : []));
      meetups.splice(0, meetups.length, ...(Array.isArray(saved.meetups) ? saved.meetups.filter((m) => m && Array.isArray(m.ids) && typeof m.at === 'number') : []));
      news.splice(0, news.length, ...(Array.isArray(saved.news) ? saved.news.filter((n) => typeof n === 'string').slice(0, 10) : []));
      talks.splice(0, talks.length, ...(Array.isArray(saved.talks) ? saved.talks.filter((t) => t && Array.isArray(t.lines)).slice(0, 30) : []));
      lastPair.clear();
      reset(saved.minds);
      setActive(active);
    };

    /** Nothing is saved until the saved city has been read, so a slow read can't be overwritten by a fresh one. */
    let loaded = false;
    const local0 = game.load<Partial<Saved> | null>('city', null);
    restore(local0);
    if (!local0) reset(undefined);
    void game
      .callMain('loadCity')
      .then((file) => {
        const saved = file as Partial<Saved> | null;
        // The newest copy wins (the data folder, normally; the browser storage only for a city from before it moved).
        if (!disposed && saved && (saved.savedAt ?? 0) >= (local0?.savedAt ?? 0)) restore(saved);
      })
      .catch(() => {})
      .finally(() => (loaded = true));

    const persist = () => {
      if (!loaded) return;
      const data = snapshot();
      void game
        .callMain('saveCity', data)
        .then((ok) => {
          // Saved to the data folder: the old copy in the browser storage isn't needed any more.
          if (ok && game.load('city', null)) game.save('city', null);
          if (!ok) game.save('city', data);
        })
        // Not in the desktop game: the browser storage will have to do.
        .catch(() => game.save('city', data));
    };

    const mapLoaded = () => {
      clearRoads();
      if (game.world.map === 'city') {
        city.build(game.world);
        placeEveryone();
      } else {
        city.clear();
      }
    };
    mapLoaded();
    setActive(false);

    return {
      mapLoaded,
      frame(dt) {
        const want = game.world.map === 'city' && !game.inEvent() && city.places.length > 0;
        if (want !== active) setActive(want);
        if (!active) return;
        if (!game.paused() && dt > 0) update(Math.min(dt, 0.1));
        else for (const npc of npcs) if (npc.body.root.visible) pose(npc);
        // A chat can't stay open over the menu or settings.
        if (game.paused() && chat.open) chat.hide();
        if (chase && !options.chases) endChase('off');
        placeLabels();
        journal.tick();
        saveTimer += dt;
        if (saveTimer > 20) {
          saveTimer = 0;
          persist();
        }
      },
      settings(panel) {
        updateSettings = settingsPanel(panel, {
          options,
          save: () => game.save('options', options),
          status: () => ai,
          configure: async (change) => {
            ai = ((await game.callMain('configure', change).catch(() => ai)) as AiStatus | null) ?? ai;
            waitUntil = 0;
          },
          download: (choice: ModelChoice) => void game.callMain('download', choice).then(() => pollStatus()),
          cancel: () => void game.callMain('cancelDownload').then(() => pollStatus()),
          remove: (choice: ModelChoice) => void game.callMain('removeModel', choice).then(() => pollStatus()),
          resetCity: () => {
            for (const convo of new Set(npcs.map((n) => n.convo).filter((c): c is Convo => !!c))) endTalk(convo);
            chat.hide();
            // The clock starts again, so everything timed by it starts again too.
            restore({ clock: 0, tickets: 0, meetups: [], news: [], talks: [], minds: {}, heat: 0, crimes: [], messages: [] });
            lastTicket = -Infinity;
            beamUntil = 0;
            beam.visible = false;
            loaded = true;
            persist();
          },
        });
      },
      dispose() {
        persist();
        disposed = true;
        window.clearInterval(poll);
        window.removeEventListener('pointerdown', onPointer, true);
        updateSettings = null;
        clearRoads();
        police?.dispose();
        chat.hide();
        chat.dispose();
        journal.dispose();
        for (const npc of npcs) {
          npc.body.dispose();
          npc.label.dispose();
        }
        city.dispose();
        beam.removeFromParent();
        beam.geometry.dispose();
        beam.material.dispose();
        layer.remove();
      },
    };
  },
};

export default pack;

/** An angle brought into -π..π. */
function wrap(a: number): number {
  return Math.atan2(Math.sin(a), Math.cos(a));
}

/** The player's car in a few words: "a red sports car". */
export function describeCar(config: CarConfig): string {
  const kind: Record<CarConfig['body'], string> = {
    sports: 'sports car',
    muscle: 'muscle car',
    hatch: 'hatchback',
    pickup: 'pickup truck',
    van: 'van',
    wedge: 'wedge-shaped supercar',
  };
  const colour = colourName(config.paintColor);
  const giant = config.size > 2.5 ? 'giant ' : '';
  const words = `${giant}${colour} ${kind[config.body] ?? 'car'}`;
  return `${/^[aeiou]/.test(words) ? 'an' : 'a'} ${words}`;
}

/** A colour in plain words. */
function colourName(hex: string): string {
  const c = new THREE.Color(hex);
  const hsl = { h: 0, s: 0, l: 0 };
  c.getHSL(hsl);
  if (hsl.l > 0.85) return 'white';
  if (hsl.l < 0.12) return 'black';
  if (hsl.s < 0.15) return hsl.l > 0.55 ? 'silver' : 'grey';
  const h = hsl.h * 360;
  const names: [number, string][] = [
    [15, 'red'],
    [40, 'orange'],
    [68, 'yellow'],
    [160, 'green'],
    [195, 'teal'],
    [255, 'blue'],
    [290, 'purple'],
    [340, 'pink'],
    [360, 'red'],
  ];
  return names.find(([limit]) => h <= limit)?.[1] ?? 'red';
}
