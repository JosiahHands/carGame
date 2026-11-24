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
// The parts: cast.ts and residents.ts (who they are), mind.ts (memory,
// feelings, needs, friendship), brain.ts (what they say), city.ts (places,
// homes and walking routes), body.ts (how they look and move), labels.ts
// (name tags and speech bubbles), ui.ts (chat, journal and settings), and
// main.ts (the main-process side: the local AI and the saved city).
import * as THREE from 'three';
import type { CarConfig } from '../../game/carConfig';
import type { GameApi, Pack, PackInstance } from '../../game/packs';
import { el } from '../../libs/dom';
import { Body, type Action } from './body';
import { LocalBrain, RuleBrain, type Brain, type ConversationResult, type Line, type MeetPlan, type Moment, type Person } from './brain';
import { CAST, type CastMember, type Emotion, type PlaceKind } from './cast';
import { CityMap, type Place } from './city';
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
  setStory,
  tickNeeds,
  type Friendship,
  type Mind,
} from './mind';
import { MODELS, type AiStatus, type ModelChoice } from './protocol';
import { aiWords, buildChat, buildJournal, DEFAULT_OPTIONS, settingsPanel, type Options, type Talk } from './ui';

/** Two people's plan to meet: where, and when (city clock). */
interface Meetup {
  ids: [string, string];
  placeId: string;
  kind: PlaceKind;
  at: number;
}

/** What gets saved: the city's clock, every mind, plans, the latest news and talk, and your record with the police. */
interface Saved {
  version: 2;
  /** When it was saved (real time), so the newest copy wins. */
  savedAt: number;
  clock: number;
  minds: Record<string, Mind>;
  tickets: number;
  meetups: Meetup[];
  news: string[];
  talks: Talk[];
}

/** What someone's doing: heading somewhere, spending time there, wandering, going home, asleep, or (BUDDY) sticking with GUY. */
type Plan =
  | { kind: 'go'; place: Place; spot: THREE.Vector2; meet?: Meetup }
  | { kind: 'stay'; place: Place; until: number; meet?: Meetup }
  | { kind: 'wander'; until: number }
  | { kind: 'homeward' }
  | { kind: 'sleep'; until: number }
  | { kind: 'follow' };

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

const pick = <T,>(list: readonly T[]): T => list[Math.floor(Math.random() * list.length)];

const pack: Pack = {
  id: 'npc',
  name: 'Living City',
  icon: '🧠',
  about:
    `A whole city of ${CAST.length} people with jobs, homes, friends and minds of their own, brought to life by a free, open-source AI that runs on this computer. ` +
    'They live through the days, talk to each other, make plans, remember everything (for as long as the game is installed) and change over time. ' +
    'Click on anyone to talk to them, and make friends. L opens the city journal (city free roam).',
  activate(game: GameApi): PackInstance {
    const stored = game.load<Partial<Options>>('options', {});
    const options: Options = {
      think: stored.think === 'nearby' || stored.think === 'everywhere' ? stored.think : DEFAULT_OPTIONS.think,
      tags: typeof stored.tags === 'boolean' ? stored.tags : DEFAULT_OPTIONS.tags,
    };
    let clock = 0;
    let tickets = 0;
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
        officer.plan = { kind: 'follow' };
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
    const moment = (): Moment => ({ now: clock, time: timeWords(clock), car: carWords(), news });
    const placeName = (x: number, z: number) => {
      const near = city.nearestPlace(x, z);
      return near && near.distance < 45 ? near.place.label : 'the street';
    };
    const doing = (npc: Npc): string => {
      if (npc.asleep) return 'asleep at home';
      if (npc.chatting) return 'talking to the driver, who has pulled up beside them';
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
          return 'heading home for the night';
        case 'sleep':
          return 'asleep at home';
        case 'follow':
          return `walking with ${byId.get(PARTNER)?.member.name ?? 'a friend'}, keeping an eye on things`;
      }
    };
    const person = (npc: Npc): Person => ({ member: npc.member, mind: npc.mind, where: placeName(npc.x, npc.z), doing: doing(npc) });
    const tellNews = (text: string) => {
      if (news[0] === text) return;
      news.unshift(text);
      if (news.length > 10) news.pop();
    };

    // ---------- their days ----------

    /** When they go to bed and get up: the lively stay up later. */
    const bedHour = (npc: Npc) => 22.5 + npc.mind.traits.energy * 2; // 22:30 .. 00:30
    const wakeHour = (npc: Npc) => 5.5 + (1 - npc.mind.traits.energy) * 2; // 05:30 .. 07:30
    const bedtime = (npc: Npc) => {
      const h = hourOf(clock);
      const late = h < 12 ? h + 24 : h; // counts the small hours as late tonight
      return late >= bedHour(npc) && late < wakeHour(npc) + 24;
    };

    const goHome = (npc: Npc) => {
      if (!npc.home) return;
      leaveVisit(npc);
      npc.path = city.route(npc.x, npc.z, npc.home.x, npc.home.z);
      npc.plan = { kind: 'homeward' };
    };

    const sleep = (npc: Npc) => {
      npc.asleep = true;
      npc.path = [];
      npc.label.hush();
      npc.plan = { kind: 'sleep', until: nextHour(clock, wakeHour(npc)) };
    };

    const wake = (npc: Npc) => {
      npc.asleep = false;
      npc.mind.needs.rest = 1;
      remember(npc.mind, clock, `Woke up on ${DAYS[dayOf(clock) % 7]} morning.`, 1, [], 'saw');
      decide(npc);
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
      // Some variety, especially for the curious.
      if (npc.plan.kind === 'stay' && npc.plan.place === place) score -= 0.4;
      score += Math.random() * (0.2 + t.curiosity * 0.4);
      return score - Math.hypot(place.x - npc.x, place.z - npc.z) * 0.0015;
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

    const decide = (npc: Npc) => {
      leaveVisit(npc);
      if (bedtime(npc) && npc.home) return goHome(npc);
      if (Math.random() < npc.mind.traits.curiosity * 0.1) {
        const spot = city.wanderSpot(npc.x, npc.z, Math.random);
        npc.path = city.route(npc.x, npc.z, spot.x, spot.y);
        npc.plan = { kind: 'wander', until: clock + 40 };
        return;
      }
      const best = bestPlace(npc);
      if (best) goTo(npc, best);
    };

    const arrive = (npc: Npc, place: Place, meet?: Meetup) => {
      const working = place.kind === npc.member.job && weekday(clock);
      const stay = meet ? 2 * HOUR : working ? 90 + Math.random() * 120 : 40 + Math.random() * 60;
      npc.plan = { kind: 'stay', place, until: clock + stay, meet };
      npc.visit = { kind: place.kind, mood: npc.mind.mood, talked: false };
      if (Math.random() < 0.1) remember(npc.mind, clock, `Spent some time at ${place.label}.`, 1, [], 'saw');
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
            if (p.asleep || p.chatting || p.convo || p.plan.kind === 'follow' || bedtime(p)) continue;
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
    const offence = (what: string, x: number, z: number, reach = 30) => {
      const officer = byId.get(OFFICER);
      if (!officer || !active || officer.chatting || officer.asleep || clock - lastTicket < 40) return;
      if (Math.hypot(officer.x - x, officer.z - z) > reach) return;
      lastTicket = clock;
      const friendly = (officer.mind.feelings.driver ?? 0) > 0.55;
      const name = officer.mind.driverName;
      if (friendly && Math.random() < 0.6) {
        officer.label.say(pick([`Let's call that a warning${name ? `, ${name}` : ''}.`, "I didn't see that. This time.", 'Easy, friend. Easy.']), 3.5, clock);
        remember(officer.mind, clock, `Let the driver off with a warning for ${what}.`, 4, ['driver'], 'saw');
        game.toast(`🚓 BUDDY let you off with a warning for ${what}`, 2600);
      } else {
        tickets++;
        officer.label.say(pick([`That's a ticket for ${what}!`, 'Pull over! Ticket time.', `Whoa there! ${what[0].toUpperCase() + what.slice(1)}!`, 'I saw that! Ticket!']), 3.5, clock);
        remember(officer.mind, clock, `Wrote the driver in ${carWords()} a ticket for ${what}.`, 5, ['driver'], 'saw');
        feel(officer.mind, 'driver', -0.04);
        game.toast(`🚓 BUDDY wrote you a ticket for ${what} (${tickets} so far)`, 2600);
        const partner = byId.get(PARTNER);
        if (partner && Math.hypot(partner.x - officer.x, partner.z - officer.z) < 12) {
          remember(partner.mind, clock, `Watched BUDDY give the driver a ticket for ${what}.`, 3, ['buddy', 'driver'], 'saw');
        }
      }
      emote(officer, 'proud');
      officer.lookX = game.player.x;
      officer.lookZ = game.player.z;
      tellNews(`BUDDY ${friendly ? 'warned' : 'ticketed'} the driver for ${what}`);
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
      offence('reckless driving', e.x, e.z);
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
      offence(`smashing ${thing}`, e.x, e.z, 40);
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
      offence('dangerous driving', e.x, e.z, 25);
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
      offence('causing an explosion', e.x, e.z, 60);
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
          if (result.insights[0] && !npc.convo && !npc.chatting && !npc.asleep) npc.label.say(`💭 ${result.insights[0]}`, 5, clock, true);
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
      describe: (npc: Npc) => `${npc.member.role} · ${friendship(npc.mind)} · ${feelingWord(npc.mind.feelings.driver ?? 0)} you`,
      close: (npc: Npc) => {
        npc.chatting = false;
        npc.lookX = null;
        resume(npc);
      },
    });

    /** Starts a chat with someone (clicked on, or the nearest when you press K). */
    const openChat = (npc: Npc) => {
      if (!active || game.paused()) return;
      const p = game.player;
      if (npc.asleep) return game.toast(`${npc.member.name} is asleep at home`, 1800);
      if (Math.hypot(npc.x - p.x, npc.z - p.z) > CHAT_RANGE) return game.toast(`Drive closer to talk to ${npc.member.name}`, 1800);
      if (Math.hypot(p.vx, p.vz) > 7) return game.toast('Slow down to talk', 1500);
      if (npc.convo) endTalk(npc.convo);
      npc.chatting = true;
      npc.path = [];
      npc.reaction = null;
      chat.show(npc);
    };

    const startChat = () => {
      if (!active) return;
      if (chat.open) return chat.focus();
      const p = game.player;
      let best: Npc | null = null;
      let bestD = 14;
      for (const npc of npcs) {
        const d = Math.hypot(npc.x - p.x, npc.z - p.z);
        if (!npc.asleep && d < bestD) {
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
      tickets: () => tickets,
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

    // ---------- every frame ----------

    const target = new THREE.Vector2();

    const walk = (npc: Npc, dt: number): { action: Action; speed: number } => {
      if (npc.asleep) return { action: 'stand', speed: 0 };
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
        npc.lookX = game.player.x;
        npc.lookZ = game.player.z;
        return { action: 'talk', speed: 0 };
      }
      if (npc.convo) {
        const speaking = npc.convo.lines && npc.label.talking && npc.convo.lines[npc.convo.index - 1]?.speaker === npc.member.id;
        return { action: speaking ? 'talk' : 'listen', speed: 0 };
      }
      // Where to next?
      let running = false;
      if (npc.plan.kind === 'follow') {
        const partner = byId.get(PARTNER);
        if (!partner) return { action: 'stand', speed: 0 };
        // At GUY's side, a step to his right.
        const side = partner.heading - Math.PI / 2;
        target.set(partner.x + Math.sin(side) * 1.5, partner.z + Math.cos(side) * 1.5);
        const d = Math.hypot(target.x - npc.x, target.y - npc.z);
        if (partner.convo || partner.chatting) {
          npc.lookX = partner.x;
          npc.lookZ = partner.z;
          if (d < 2.5) return { action: 'listen', speed: 0 };
        } else if (d < 0.6) {
          npc.heading = partner.heading;
          return { action: 'stand', speed: 0 };
        }
        running = d > 14;
        npc.path = [target];
      } else if (npc.path.length === 0) {
        if (npc.plan.kind === 'go') arrive(npc, npc.plan.place, npc.plan.meet);
        else if (npc.plan.kind === 'wander') decide(npc); // got where they were wandering to
        else if (npc.plan.kind === 'homeward') {
          sleep(npc);
          return { action: 'stand', speed: 0 };
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
      if (speed < 4 || npc.reaction === 'dive' || npc.chatting || npc.asleep) return;
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
        offence(hit ? `hitting ${npc.member.name}` : 'nearly hitting someone', npc.x, npc.z);
        checkBond(npc);
      }
    };

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
          if (partner && npc.asleep !== partner.asleep) {
            npc.asleep = partner.asleep;
            npc.x = partner.x + 1.4;
            npc.z = partner.z + 0.6;
          }
        } else if (npc.plan.kind === 'sleep') {
          if (clock >= npc.plan.until) wake(npc);
        } else if (!npc.convo && !npc.chatting) {
          if (npc.plan.kind !== 'homeward' && bedtime(npc)) goHome(npc);
          else if (npc.plan.kind === 'stay' && clock > npc.plan.until) decide(npc);
        }
        // A lot on their mind: time to think it over (asleep counts: they dream on it).
        maybeReflect(npc);
        // Speeding past the police.
        if (npc.member.id === OFFICER && !npc.asleep && Math.hypot(p.vx, p.vz) > 32 && Math.hypot(npc.x - p.x, npc.z - p.z) < 22) offence('speeding', npc.x, npc.z);
        npc.label.setFriend(isFriend(m));
      }
      keepPlans();
      greetFriends();
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
    const shown = (npc: Npc) => active && !npc.asleep && Math.hypot(npc.x - game.camera.position.x, npc.z - game.camera.position.z) < SHOW_RANGE;

    const update = (dt: number) => {
      clock += dt;
      thinkTimer += dt;
      if (thinkTimer > 0.25) {
        think(thinkTimer);
        thinkTimer = 0;
      }
      for (const npc of npcs) if (npc.convo && npc.convo.people[0] === npc) playTalk(npc.convo);
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
      if (chat.open && chat.npc && Math.hypot(chat.npc.x - game.player.x, chat.npc.z - game.player.z) > CHAT_RANGE + 20) chat.hide();
    };

    const placeLabels = () => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      for (const npc of npcs) {
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
        chat.hide();
        journal.hide();
        beam.visible = false;
      }
    };

    // ---------- saving: in the game's data folder, for as long as it's installed ----------

    const snapshot = (): Saved => {
      const minds: Record<string, Mind> = {};
      for (const npc of npcs) minds[npc.member.id] = npc.mind;
      return { version: 2, savedAt: Date.now(), clock, minds, tickets, meetups, news, talks };
    };

    /** Takes in a saved city (from the data folder, or from older versions that kept it in the browser storage). */
    const restore = (saved: Partial<Saved> | null) => {
      if (!saved || typeof saved !== 'object') return;
      clock = typeof saved.clock === 'number' && Number.isFinite(saved.clock) ? saved.clock : 0;
      tickets = typeof saved.tickets === 'number' ? saved.tickets : 0;
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
            restore({ clock: 0, tickets: 0, meetups: [], news: [], talks: [], minds: {} });
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
