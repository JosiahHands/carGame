// 🧠 Living City: twelve people with names, jobs, friends and minds of their
// own live in the city. They go about their day (coffee, work, lunch, the
// park), meet up and talk, gossip about each other and about you, remember
// what they've seen, and slowly change: forming beliefs, picking up habits,
// growing braver or warier, warming to you or holding a grudge.
//
// With an Anthropic API key (Packs tab), Claude writes what they say and
// think, from who they are now and what they remember. Without one, a
// simpler built-in brain keeps them going.
//
// GUY, the cheerful bank teller in the blue shirt, and his best friend
// BUDDY, a police officer who goes everywhere with him (and writes you
// tickets), are among them. Press K by someone to talk to them, and L for
// the city journal.
//
// The parts: cast.ts (who they are), mind.ts (memory, feelings, needs),
// brain.ts (what they say), city.ts (places and walking routes), body.ts
// (how they look and move), labels.ts (name tags and speech bubbles), and
// main.ts (the main-process side that talks to Claude).
import * as THREE from 'three';
import type { CarConfig } from '../../game/carConfig';
import type { GameApi, Pack, PackInstance, PanelUI } from '../../game/packs';
import { button, el } from '../../libs/dom';
import { Body, type Action } from './body';
import { AiBrain, RuleBrain, type Brain, type Line, type Moment, type Person } from './brain';
import { CAST, castMember, TRAIT_NAMES, type CastMember, type Emotion, type PlaceKind } from './cast';
import { CityMap, type Place } from './city';
import { Label } from './labels';
import {
  addHabit,
  addInsight,
  feel,
  feelingWord,
  jolt,
  learnPlace,
  moodWord,
  nudge,
  remember,
  restoreMind,
  tickNeeds,
  traitWords,
  type Memory,
  type Mind,
} from './mind';
import { MODELS, type ModelChoice } from './protocol';

interface Options {
  model: ModelChoice;
  /** How often they use the AI: never, for what's near you, or as much as possible. */
  think: 'off' | 'normal' | 'lots';
  tags: boolean;
}

const DEFAULTS: Options = { model: 'opus', think: 'normal', tags: true };

/** What gets saved: the city's clock, every mind, and your record with the police. (The AI spend is saved on its own: see addSpend.) */
interface Saved {
  clock: number;
  minds: Record<string, Mind>;
  tickets: number;
}

/** What someone's doing: heading somewhere, spending time there, wandering, or (BUDDY) sticking with GUY. */
type Plan =
  | { kind: 'go'; place: Place; spot: THREE.Vector2 }
  | { kind: 'stay'; place: Place; until: number }
  | { kind: 'wander'; until: number }
  | { kind: 'follow' };

interface Npc {
  member: CastMember;
  mind: Mind;
  body: Body;
  label: Label;
  x: number;
  z: number;
  heading: number;
  path: THREE.Vector2[];
  plan: Plan;
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
  /** When they last called out, reacted to something scary, or reflected. */
  lastShout: number;
  lastScare: number;
  lastReflect: number;
  reflecting: boolean;
  /** Their mood when they arrived where they are, to learn whether they like it. */
  visit: { kind: PlaceKind; mood: number; talked: boolean } | null;
  chatting: boolean;
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

const pick = <T,>(list: T[]): T => list[Math.floor(Math.random() * list.length)];

const pack: Pack = {
  id: 'npc',
  name: 'Living City',
  icon: '🧠',
  about:
    'Twelve people with names, jobs and friends live in the city. They go about their day, meet up and chat, remember what you do, and change over time: ' +
    'forming opinions, picking up habits and warming to you or holding a grudge. Add an Anthropic API key and Claude writes their conversations. ' +
    'Press K next to someone to talk to them and L for the city journal (city free roam).',
  activate(game: GameApi): PackInstance {
    const options: Options = { ...DEFAULTS, ...game.load<Partial<Options>>('options', {}) };
    if (!Object.hasOwn(MODELS, options.model)) options.model = DEFAULTS.model;
    const saved = game.load<Partial<Saved>>('city', {});
    let clock = typeof saved.clock === 'number' ? saved.clock : 0;
    let tickets = typeof saved.tickets === 'number' ? saved.tickets : 0;
    let spentNow = 0;
    /** Switched off: answers still on their way are ignored, apart from what they cost. */
    let disposed = false;
    /**
     * The AI spend is saved the moment an answer arrives. An answer can arrive
     * after the pack is switched off (or off and on again), so the total is
     * read and written each time rather than kept here.
     */
    const totalSpent = () => {
      const total = game.load<unknown>('spend', 0);
      return typeof total === 'number' && Number.isFinite(total) ? total : 0;
    };
    const addSpend = (dollars: number) => {
      if (!Number.isFinite(dollars) || dollars <= 0) return;
      game.save('spend', totalSpent() + dollars);
      if (!disposed) spentNow += dollars;
    };

    // ---------- the AI ----------
    let key: 'saved' | 'environment' | 'none' | 'checking' = 'checking';
    let secure = false;
    let waitUntil = 0; // after the AI is busy or fails, wait a little before asking again
    let lastAiTalk = -Infinity;
    let lastProblem = '';
    const checkKey = () =>
      game
        .callMain('status')
        .then((s) => {
          const status = s as { key: 'saved' | 'environment' | 'none'; secure: boolean };
          key = status.key;
          secure = status.secure;
        })
        .catch(() => (key = 'none')); // not running in the desktop app
    void checkKey();
    const rules: Brain = new RuleBrain();
    const ai = new AiBrain(
      {
        get model() {
          return options.model;
        },
        call: (request) => game.callMain('think', request),
        spent: addSpend,
        failed: (reason, message) => {
          if (disposed) return;
          lastProblem = message;
          if (reason === 'no-key') {
            key = 'none';
            game.toast('Living City: the API key was not accepted. Check it in the Packs tab.', 4000);
          } else if (reason !== 'refused') {
            waitUntil = performance.now() + (reason === 'busy' ? 20_000 : 45_000);
            if (reason === 'error') console.warn('Living City: the AI request failed:', message);
          }
        },
      },
      rules,
    );
    const aiReady = () => (key === 'saved' || key === 'environment') && options.think !== 'off' && performance.now() > waitUntil;

    // ---------- the city and its people ----------
    const city = new CityMap(game.scene);
    const layer = el('div', 'pack-layer npc-layer');
    game.overlay.append(layer);
    const npcs: Npc[] = [];
    const byId = new Map<string, Npc>();
    const lastPair = new Map<string, number>(); // when each pair last talked
    const news: string[] = []; // what's been happening, newest first
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
        game.scene.add(body.root);
        const style = member.id === PARTNER ? 'hero' : member.id === OFFICER ? 'police' : '';
        const npc: Npc = {
          member,
          mind: restoreMind(member, minds?.[member.id]),
          body,
          label: new Label(layer, member.name, style),
          x: 0,
          z: 0,
          heading: Math.random() * Math.PI * 2,
          path: [],
          plan: { kind: 'wander', until: 0 },
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
          reflecting: false,
          visit: null,
          chatting: false,
        };
        npcs.push(npc);
        byId.set(member.id, npc);
      }
      placeEveryone();
    };

    /** Puts everyone somewhere sensible: at work, or somewhere they like. */
    const placeEveryone = () => {
      if (city.places.length === 0) return;
      for (const npc of npcs) {
        if (npc.member.id === OFFICER) continue;
        const kinds = [npc.member.job, ...npc.member.likes].filter((k): k is PlaceKind => !!k);
        const place = city.placesOf(kinds[0] ?? 'park')[0] ?? city.places[0];
        const spot = spotAt(place);
        npc.x = spot.x;
        npc.z = spot.y;
        npc.path = [];
        npc.plan = { kind: 'stay', place, until: clock + 20 + Math.random() * 40 };
        npc.visit = { kind: place.kind, mood: npc.mind.mood, talked: false };
      }
      const officer = byId.get(OFFICER);
      const partner = byId.get(PARTNER);
      if (officer && partner) {
        officer.x = partner.x + 1.4;
        officer.z = partner.z + 0.6;
        officer.plan = { kind: 'follow' };
      }
    };

    const spotAt = (place: Place) => {
      const a = Math.random() * Math.PI * 2;
      const r = place.spread * (0.3 + Math.random() * 0.7);
      return new THREE.Vector2(place.x + Math.cos(a) * r, place.z + Math.sin(a) * r);
    };

    reset(saved.minds);

    // ---------- words for prompts ----------

    const carWords = () => describeCar(game.playerCar().config);
    const moment = (): Moment => ({ now: clock, night: game.world.isNight, car: carWords(), news });
    const placeName = (x: number, z: number) => {
      const near = city.nearestPlace(x, z);
      return near && near.distance < 45 ? near.place.label : 'the street';
    };
    const doing = (npc: Npc): string => {
      if (npc.chatting) return 'talking to the driver, who has pulled up beside them';
      if (npc.convo) {
        const others = npc.convo.people.filter((p) => p !== npc).map((p) => p.member.name);
        return `chatting with ${others.join(' and ')}`;
      }
      switch (npc.plan.kind) {
        case 'go':
          return `walking to ${npc.plan.place.label}`;
        case 'stay':
          return npc.plan.place.kind === npc.member.job ? `working at ${npc.plan.place.label}` : `hanging out at ${npc.plan.place.label}`;
        case 'wander':
          return 'strolling around the city';
        case 'follow':
          return `walking with ${byId.get(PARTNER)?.member.name ?? 'a friend'}, keeping an eye on things`;
      }
    };
    const person = (npc: Npc): Person => ({ member: npc.member, mind: npc.mind, where: placeName(npc.x, npc.z), doing: doing(npc) });
    const tellNews = (text: string) => {
      if (news[0] === text) return;
      news.unshift(text);
      if (news.length > 8) news.pop();
    };

    // ---------- deciding where to go ----------

    /** How much someone wants to be at a place right now. */
    const appeal = (npc: Npc, place: Place): number => {
      const { mind, member } = npc;
      const n = mind.needs;
      const t = mind.traits;
      let score = (mind.likes[place.kind] ?? 0) * 0.8;
      if (place.kind === 'diner' || place.kind === 'coffee') score += (1 - n.food) * (place.kind === 'diner' ? 1.6 : 1.1);
      if (place.kind === 'park') score += (1 - n.rest) * 1.2 + (1 - n.fun) * 0.3;
      if (place.kind === 'stunts') score += (1 - n.fun) * (0.4 + t.courage);
      if (place.kind === 'music') score += (1 - n.fun) * (0.3 + t.humour * 0.8);
      if (place.kind === 'gym') score += (1 - n.fun) * t.energy * 0.8 + (1 - n.rest) * -0.8;
      if (place.kind === 'news') score += t.curiosity * 0.4;
      // Work, when they're not desperate for anything.
      if (place.kind === member.job) score += 0.9 * Math.min(n.food, n.rest, n.fun) + 0.2;
      // Friends there (or on their way): the lonelier they are, the more it counts.
      for (const other of npcs) {
        if (other === npc) continue;
        const at = other.plan.kind === 'stay' || other.plan.kind === 'go' ? other.plan.place : null;
        if (at === place) score += Math.max(0, mind.feelings[other.member.id] ?? 0) * (0.25 + (1 - n.social) * 0.6);
      }
      // Some variety, especially for the curious.
      if (npc.plan.kind === 'stay' && npc.plan.place === place) score -= 0.4;
      score += Math.random() * (0.2 + t.curiosity * 0.4);
      return score - Math.hypot(place.x - npc.x, place.z - npc.z) * 0.0015;
    };

    const decide = (npc: Npc) => {
      // Leaving somewhere: was it a good time? They'll remember.
      if (npc.visit) {
        const reward = (npc.mind.mood - npc.visit.mood) * 2.5 + (npc.visit.talked ? 0.35 : 0) + (npc.mind.likes[npc.visit.kind] ?? 0) * 0.3;
        learnPlace(npc.mind, npc.visit.kind, Math.max(-1, Math.min(1, reward)));
        npc.visit = null;
      }
      if (Math.random() < npc.mind.traits.curiosity * 0.12) {
        const spot = city.wanderSpot(npc.x, npc.z, Math.random);
        npc.path = city.route(npc.x, npc.z, spot.x, spot.y);
        npc.plan = { kind: 'wander', until: clock + 40 };
        return;
      }
      let best: Place | null = null;
      let bestScore = -Infinity;
      for (const place of city.places) {
        const score = appeal(npc, place);
        if (score > bestScore) {
          bestScore = score;
          best = place;
        }
      }
      if (!best) return;
      const spot = spotAt(best);
      npc.path = city.route(npc.x, npc.z, spot.x, spot.y);
      npc.plan = { kind: 'go', place: best, spot };
    };

    const arrive = (npc: Npc, place: Place) => {
      const working = place.kind === npc.member.job;
      npc.plan = { kind: 'stay', place, until: clock + (working ? 90 + Math.random() * 90 : 40 + Math.random() * 60) };
      npc.visit = { kind: place.kind, mood: npc.mind.mood, talked: false };
      if (Math.random() < 0.15) remember(npc.mind, clock, `Spent some time at ${place.label}.`, 1, [], 'saw');
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
      if (npc.convo?.lines || npc.chatting || clock - npc.lastShout < 6) return;
      const lines = SHOUTS[emotion];
      if (!lines) return;
      npc.lastShout = clock;
      npc.label.say(pick(lines), 2.4, clock);
    };

    /** Everyone within `radius` of a spot, with how far away they are. */
    const near = (x: number, z: number, radius: number) =>
      npcs.map((npc) => ({ npc, d: Math.hypot(npc.x - x, npc.z - z) })).filter((n) => n.d < radius);

    /** How someone takes a wild bit of driving: the bold and fun-loving enjoy it, the timid and kind don't. */
    const thrill = (npc: Npc) => {
      const t = npc.mind.traits;
      return (t.courage + t.energy + t.humour) / 3 - 0.5 + (npc.mind.feelings.driver ?? 0) * 0.4;
    };

    // ---------- the police ----------

    let lastTicket = -Infinity;
    const offence = (what: string, x: number, z: number, reach = 30) => {
      const officer = byId.get(OFFICER);
      if (!officer || !active || officer.chatting || clock - lastTicket < 40) return;
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
      if (a.convo || b.convo || a.chatting || b.chatting || a.reaction || b.reaction) return false;
      // BUDDY is always with GUY, so he talks to him now and then, and listens in on GUY's conversations.
      const officerPair = a.member.id === OFFICER || b.member.id === OFFICER;
      const partnerPair = officerPair && (a.member.id === PARTNER || b.member.id === PARTNER);
      if (officerPair && !partnerPair) return false;
      const gap = partnerPair ? 200 : 110;
      if (clock - (lastPair.get(pairKey(a, b)) ?? -Infinity) < gap) return false;
      const fondness = ((a.mind.feelings[b.member.id] ?? 0) + (b.mind.feelings[a.member.id] ?? 0)) / 2;
      const lonely = 2 - a.mind.needs.social - b.mind.needs.social;
      return fondness > -0.35 && lonely * 0.5 + fondness + (a.mind.traits.energy + b.mind.traits.energy) * 0.15 > 0.45;
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
      a.label.say('…', 30, clock);
      // The AI writes conversations you're near enough to hear (as often as the settings allow); the built-in brain does the rest.
      const range = options.think === 'lots' ? 160 : 80;
      const gap = options.think === 'lots' ? 5 : 14;
      const close = Math.hypot(a.x - game.player.x, a.z - game.player.z) < range;
      const brain: Brain = aiReady() && close && clock - lastAiTalk > gap ? ai : rules;
      if (brain === ai) lastAiTalk = clock;
      void brain
        .conversation(person(a), person(b), moment())
        .catch(() => rules.conversation(person(a), person(b), moment()))
        .then((result) => {
          if (disposed || a.convo !== convo) return; // switched off, or interrupted while thinking
          for (const m of result.memories) {
            const who = byId.get(m.who);
            const other = convo.people.find((p) => p !== who);
            if (who && other) remember(who.mind, clock, m.text, m.importance, [other.member.id, ...mentions(m.text)], 'talk');
          }
          for (const f of result.feelings) {
            const who = byId.get(f.from);
            if (who) feel(who.mind, f.about, f.change * 0.06);
          }
          for (const npc of convo.people) {
            const other = convo.people.find((p) => p !== npc)!;
            npc.mind.met[other.member.id] = (npc.mind.met[other.member.id] ?? 0) + 1;
          }
          convo.lines = result.lines.length ? result.lines : [{ speaker: a.member.id, text: 'Hi!', emotion: 'happy' }];
          convo.next = clock;
        });
    };

    /** Cast members named in a bit of text (for whom a memory is about). */
    const mentions = (text: string) => {
      const lower = text.toLowerCase();
      const ids = CAST.filter((c) => lower.includes(c.name.toLowerCase())).map((c) => c.id);
      if (lower.includes('driver')) ids.push('driver');
      return ids;
    };

    const endTalk = (convo: Convo) => {
      for (const npc of convo.people) {
        npc.convo = null;
        npc.lookX = null;
        if (!convo.lines) npc.label.hush(); // the "…" while they were thinking
        npc.mind.needs.social = Math.min(1, npc.mind.needs.social + 0.3);
        jolt(npc.mind, 0.04, npc.mind.emotion);
      }
    };

    const playTalk = (convo: Convo) => {
      if (!convo.lines) {
        if (clock - convo.started > 25) endTalk(convo); // gave up waiting
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

    const reflect = (npc: Npc) => {
      npc.reflecting = true;
      npc.lastReflect = clock;
      const brain: Brain = aiReady() ? ai : rules;
      void brain
        .reflect(person(npc), moment())
        .catch(() => rules.reflect(person(npc), moment()))
        .then((result) => {
          npc.reflecting = false;
          if (disposed) return;
          npc.mind.pending = 0;
          for (const insight of result.insights) addInsight(npc.mind, clock, insight);
          if (result.trait) nudge(npc.mind, result.trait.name, result.trait.up, 0.04);
          if (result.habit) addHabit(npc.mind, result.habit);
          if (result.insights[0] && !npc.convo && !npc.chatting) npc.label.say(`💭 ${result.insights[0]}`, 5, clock, true);
        });
    };

    // ---------- talking to you ----------

    const chat = buildChat(game, {
      send: async (npc: Npc, said: string) => {
        const brain: Brain = (key === 'saved' || key === 'environment') && options.think !== 'off' && performance.now() > waitUntil ? ai : rules;
        const result = await brain.chat(person(npc), said, moment()).catch(() => rules.chat(person(npc), said, moment()));
        if (disposed) return result.reply;
        const mind = npc.mind;
        if (result.driverName) mind.driverName = result.driverName;
        feel(mind, 'driver', result.opinion * 0.12 + 0.02);
        remember(mind, clock, result.memory || `The driver stopped to chat with me.`, result.importance, ['driver', ...mentions(result.memory)], 'chat');
        mind.needs.social = Math.min(1, mind.needs.social + 0.1);
        emote(npc, result.emotion, 15);
        npc.label.say(result.reply, 3 + result.reply.split(' ').length * 0.3, clock);
        return result.reply;
      },
      describe: (npc: Npc) => `${npc.member.role} · ${feelingWord(npc.mind.feelings.driver ?? 0)} you`,
      close: (npc: Npc) => {
        npc.chatting = false;
        npc.lookX = null;
      },
    });

    const startChat = () => {
      if (!active) return;
      if (chat.open) return chat.focus();
      const p = game.player;
      let best: Npc | null = null;
      let bestD = 14;
      for (const npc of npcs) {
        const d = Math.hypot(npc.x - p.x, npc.z - p.z);
        if (d < bestD) {
          bestD = d;
          best = npc;
        }
      }
      if (!best) {
        game.toast('Pull up next to someone to talk to them', 1800);
        return;
      }
      if (Math.hypot(p.vx, p.vz) > 7) {
        game.toast('Slow down to talk', 1500);
        return;
      }
      if (best.convo) endTalk(best.convo);
      best.chatting = true;
      best.path = [];
      best.reaction = null;
      chat.show(best);
    };

    // ---------- the journal ----------

    const journal = buildJournal(game, {
      npcs: () => npcs,
      doing,
      placeName: (npc) => placeName(npc.x, npc.z),
      footer: () =>
        `${(key === 'saved' || key === 'environment') && options.think !== 'off' ? `Thinking with ${MODELS[options.model].label}` : 'Thinking with the built-in brain (add an API key in the Packs tab for AI)'} · ` +
        `🚓 ${tickets} ticket${tickets === 1 ? '' : 's'} · AI spend $${totalSpent().toFixed(2)}`,
      find: (npc) => {
        beam.position.set(npc.x, 0, npc.z);
        beam.visible = true;
        beamUntil = clock + 12;
        const d = Math.round(Math.hypot(npc.x - game.player.x, npc.z - game.player.z));
        game.toast(`${npc.member.name} is at ${placeName(npc.x, npc.z)}, ${d} m away`, 2400);
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
        if (npc.plan.kind === 'go') arrive(npc, npc.plan.place);
        else if (npc.plan.kind === 'wander') decide(npc); // got where they were wandering to
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
      const speed = running ? 4.2 : 1.2 + npc.mind.traits.energy * 0.6;
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
      if (speed < 4 || npc.reaction === 'dive' || npc.chatting) return;
      const reach = 1.6 * Math.max(1, game.playerCar().config.size);
      const rx = npc.x - p.x;
      const rz = npc.z - p.z;
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
      }
    };

    const think = (dt: number) => {
      const p = game.player;
      for (const npc of npcs) {
        const m = npc.mind;
        const staying = npc.plan.kind === 'stay' ? npc.plan.place.kind : null;
        tickNeeds(m, dt, {
          social: !!npc.convo || npc.chatting,
          eating: staying === 'diner' || staying === 'coffee',
          resting: staying === 'park' || (staying !== null && staying !== npc.member.job && !npc.path.length),
          fun: staying === 'stunts' || staying === 'music' || staying === 'gym' || npc.reaction === 'cheer',
        });
        if (clock > npc.emotionUntil && m.emotion !== 'calm') m.emotion = m.mood < -0.3 ? 'sad' : 'calm';
        if (npc.plan.kind === 'stay' && clock > npc.plan.until && !npc.convo && !npc.chatting) decide(npc);
        // A lot on their mind: time to think it over.
        if (!npc.reflecting && m.pending >= 28 && clock - npc.lastReflect > 120) reflect(npc);
        // Speeding past the police.
        if (npc.member.id === OFFICER && Math.hypot(p.vx, p.vz) > 32 && Math.hypot(npc.x - p.x, npc.z - p.z) < 22) offence('speeding', npc.x, npc.z);
      }
      // People who bump into each other may stop for a chat.
      for (let i = 0; i < npcs.length; i++) {
        for (let j = i + 1; j < npcs.length; j++) {
          const a = npcs[i];
          const b = npcs[j];
          if (Math.hypot(a.x - b.x, a.z - b.z) < 4 && wantToTalk(a, b)) {
            if (Math.random() < 0.5) startTalk(a, b);
            else startTalk(b, a);
          }
        }
      }
    };

    /** Puts someone's body where they are. Also done while the game is paused, so they never wait at the map's centre behind the menu. */
    const pose = (npc: Npc) => {
      npc.body.root.position.set(npc.x, game.world.heightAt(npc.x, npc.z), npc.z);
      npc.body.root.rotation.y = npc.heading;
    };

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
        pose(npc);
        const look = npc.lookX === null ? null : wrap(Math.atan2(npc.lookX - npc.x, npc.lookZ - npc.z) - npc.heading);
        npc.body.animate(dt, action, speed, look);
        npc.label.setEmotion(npc.mind.emotion);
      }
      if (beam.visible && clock > beamUntil) beam.visible = false;
      if (chat.open && chat.npc && Math.hypot(chat.npc.x - game.player.x, chat.npc.z - game.player.z) > 30) chat.hide();
    };

    const placeLabels = () => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      for (const npc of npcs) {
        npc.label.place(npc.x, npc.body.root.position.y + npc.body.top + 0.12, npc.z, game.camera, w, h, clock, active, options.tags);
      }
    };

    const setActive = (on: boolean) => {
      active = on;
      layer.style.display = on ? '' : 'none';
      city.setVisible(on);
      for (const npc of npcs) npc.body.root.visible = on;
      if (!on) {
        chat.hide();
        journal.hide();
        beam.visible = false;
      }
    };

    const persist = () => {
      const minds: Record<string, Mind> = {};
      for (const npc of npcs) minds[npc.member.id] = npc.mind;
      game.save('city', { clock, minds, tickets } satisfies Saved);
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
        else for (const npc of npcs) pose(npc);
        // A chat can't stay open over the menu or settings.
        if (game.paused() && chat.open) chat.hide();
        placeLabels();
        journal.tick();
        saveTimer += dt;
        if (saveTimer > 15) {
          saveTimer = 0;
          persist();
        }
      },
      settings(panel) {
        settingsPanel(panel, {
          options,
          save: () => game.save('options', options),
          key: () => key,
          secure: () => secure,
          problem: () => lastProblem,
          spent: () => ({ total: totalSpent(), now: spentNow }),
          setKey: async (value: string) => {
            const result = (await game.callMain('setKey', value).catch(() => ({ ok: false, message: 'Only works in the desktop game.' }))) as { ok: boolean; message?: string };
            await checkKey();
            waitUntil = 0;
            return result;
          },
          clearKey: async () => {
            await game.callMain('clearKey').catch(() => {});
            await checkKey();
          },
          resetCity: () => {
            for (const convo of new Set(npcs.map((n) => n.convo).filter((c): c is Convo => !!c))) endTalk(convo);
            chat.hide();
            // The clock starts again, so everything timed by it starts again too.
            clock = 0;
            tickets = 0;
            lastTicket = -Infinity;
            lastAiTalk = -Infinity;
            beamUntil = 0;
            beam.visible = false;
            news.length = 0;
            lastPair.clear();
            reset(undefined);
            setActive(active);
            persist();
          },
        });
      },
      dispose() {
        persist();
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

// ---------- the chat box ----------

interface ChatBox<T> {
  open: boolean;
  npc: T | null;
  show(npc: T): void;
  hide(): void;
  focus(): void;
  dispose(): void;
}

function buildChat(
  game: GameApi,
  hooks: {
    send: (npc: Npc, said: string) => Promise<string>;
    describe: (npc: Npc) => string;
    close: (npc: Npc) => void;
  },
): ChatBox<Npc> {
  const box = el('div', 'npc-chat hidden');
  const title = el('div', 'npc-chat-title');
  const name = el('b');
  const about = el('span');
  title.append(name, about);
  const log = el('div', 'npc-chat-log');
  const form = el('form', 'npc-chat-form');
  const input = el('input', 'text npc-chat-input');
  input.type = 'text';
  input.maxLength = 200;
  input.placeholder = 'Say something…';
  const say = el('button', 'btn primary', 'Say');
  say.type = 'submit';
  form.append(input, say);
  const hint = el('div', 'npc-chat-hint', 'Enter to say it · Esc to drive on');
  const leave = button('×', 'npc-close', () => chat.hide());
  box.append(leave, title, log, form, hint);
  game.overlay.append(box);

  const line = (who: string, text: string, mine: boolean) => {
    const row = el('div', mine ? 'npc-chat-line mine' : 'npc-chat-line');
    row.append(el('b', '', who), document.createTextNode(` ${text}`));
    log.append(row);
    log.scrollTop = log.scrollHeight;
    return row;
  };

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const npc = chat.npc;
    const said = input.value.trim();
    if (!npc || !said || waiting) return;
    input.value = '';
    line('You:', said, true);
    const reply = line(`${npc.member.name}:`, '…', false);
    // Read-only rather than disabled while they think: a disabled box loses
    // the keyboard, and then Esc would reach the game instead of the chat.
    waiting = true;
    input.readOnly = true;
    void hooks
      .send(npc, said)
      .then((text) => {
        reply.lastChild!.textContent = ` ${text}`;
        about.textContent = hooks.describe(npc);
      })
      .finally(() => {
        waiting = false;
        input.readOnly = false;
        if (chat.open) input.focus();
      });
  });
  let waiting = false;
  // Esc leaves the chat (and doesn't open the game's menu), wherever in the box the keyboard is.
  box.addEventListener('keydown', (e) => {
    if (e.code === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      chat.hide();
    }
  });

  const chat: ChatBox<Npc> = {
    open: false,
    npc: null,
    show(npc) {
      if (chat.npc && chat.npc !== npc) hooks.close(chat.npc);
      chat.npc = npc;
      chat.open = true;
      name.textContent = npc.member.name;
      about.textContent = hooks.describe(npc);
      log.replaceChildren();
      const greeting = npc.mind.driverName ? `Oh hey, ${npc.mind.driverName}!` : (npc.mind.feelings.driver ?? 0) < -0.3 ? 'Oh. You.' : 'Oh, hello there!';
      line(`${npc.member.name}:`, greeting, false);
      box.classList.remove('hidden');
      input.focus();
    },
    hide() {
      if (!chat.open) return;
      chat.open = false;
      box.classList.add('hidden');
      input.blur();
      if (chat.npc) hooks.close(chat.npc);
      chat.npc = null;
    },
    focus() {
      input.focus();
    },
    dispose() {
      box.remove();
    },
  };
  return chat;
}

// ---------- the city journal ----------

function buildJournal(
  game: GameApi,
  hooks: {
    npcs: () => Npc[];
    doing: (npc: Npc) => string;
    placeName: (npc: Npc) => string;
    footer: () => string;
    find: (npc: Npc) => void;
  },
) {
  const panel = el('div', 'npc-journal hidden');
  const head = el('div', 'npc-journal-head');
  head.append(el('b', '', '📖 City journal'), button('×', 'npc-close', () => journal.hide()));
  const list = el('div', 'npc-journal-list');
  const page = el('div', 'npc-journal-page');
  const footer = el('div', 'npc-journal-foot');
  const body = el('div', 'npc-journal-body');
  body.append(list, page);
  panel.append(head, body, footer);
  game.overlay.append(panel);
  let selected = 'guy';
  let timer = 0;

  const FACES: Record<Emotion, string> = {
    happy: '😊', excited: '🤩', calm: '🙂', curious: '🤔', amused: '😄', worried: '😟', scared: '😱', annoyed: '😠', sad: '😢', proud: '😎',
  };

  const section = (title: string, ...content: HTMLElement[]) => {
    const s = el('section');
    s.append(el('h4', '', title), ...content);
    page.append(s);
  };
  const ago = (m: Memory, now: number) => {
    const minutes = Math.round(Math.max(0, now - m.t) / 60);
    return minutes < 1 ? 'just now' : minutes < 60 ? `${minutes} min ago` : `${Math.round(minutes / 60)} h ago`;
  };

  const draw = () => {
    const npcs = hooks.npcs();
    list.replaceChildren(
      ...npcs.map((npc) => {
        const b = button(`${FACES[npc.mind.emotion]} ${npc.member.name}`, npc.member.id === selected ? 'npc-person on' : 'npc-person', () => {
          selected = npc.member.id;
          draw();
        });
        return b;
      }),
    );
    const npc = npcs.find((n) => n.member.id === selected) ?? npcs[0];
    if (!npc) return;
    const mind = npc.mind;
    page.replaceChildren();
    const title = el('div', 'npc-journal-title');
    title.append(el('b', '', npc.member.name), el('span', '', ` ${npc.member.role}`), button('Find', 'btn npc-find', () => hooks.find(npc)));
    page.append(title);
    page.append(el('p', 'npc-bio', npc.member.bio));
    page.append(el('p', '', `Now: ${hooks.doing(npc)}, near ${hooks.placeName(npc)}. Feeling ${moodWord(mind)}${mind.emotion !== 'calm' ? `, ${mind.emotion}` : ''}.`));

    const traits = el('div', 'npc-traits');
    for (const t of TRAIT_NAMES) {
      const row = el('div', 'npc-trait');
      const bar = el('i');
      bar.style.width = `${Math.round(mind.traits[t] * 100)}%`;
      const start = npc.member.traits[t];
      const drift = mind.traits[t] - start;
      row.append(el('span', '', t), el('div', 'npc-bar'), el('em', '', Math.abs(drift) >= 0.02 ? (drift > 0 ? '▲' : '▼') : ''));
      row.children[1].append(bar);
      traits.append(row);
    }
    section('Personality', el('p', 'npc-small', traitWords(mind.traits)), traits);

    const you = mind.feelings.driver ?? 0;
    const people = Object.entries(mind.feelings)
      .filter(([id]) => id !== 'driver')
      .sort((a, b) => b[1] - a[1]);
    const feelings = el('ul');
    const item = (text: string) => feelings.append(el('li', '', text));
    item(`${feelingWord(you)} you${mind.driverName ? ` (knows you as ${mind.driverName})` : ''}`);
    for (const [id, v] of [...people.slice(0, 3), ...people.slice(-1)]) item(`${feelingWord(v)} ${castMember(id)?.name ?? id}`);
    section('Feelings', feelings);

    if (mind.insights.length || mind.habits.length) {
      const beliefs = el('ul');
      for (const i of [...mind.insights].reverse()) beliefs.append(el('li', '', `“${i}”`));
      for (const h of mind.habits) beliefs.append(el('li', 'npc-small', `Habit: ${h}`));
      section('Has come to believe', beliefs);
    }

    const memories = el('ul', 'npc-memories');
    const now = clockOf(npcs);
    for (const m of mind.memories.slice(-8).reverse()) {
      const li = el('li', m.importance >= 7 ? 'big' : '');
      li.append(document.createTextNode(m.text), el('small', '', ` ${ago(m, now)}`));
      memories.append(li);
    }
    if (!mind.memories.length) memories.append(el('li', 'npc-small', 'Nothing much yet.'));
    section('Remembers', memories);
    footer.textContent = hooks.footer();
  };
  // The newest memory time stands in for "now" in the journal (it's only for "x min ago").
  const clockOf = (npcs: Npc[]) => Math.max(0, ...npcs.flatMap((n) => n.mind.memories.map((m) => m.t)));

  const journal = {
    open: false,
    toggle() {
      if (journal.open) journal.hide();
      else journal.show();
    },
    show() {
      journal.open = true;
      panel.classList.remove('hidden');
      draw();
    },
    hide() {
      journal.open = false;
      panel.classList.add('hidden');
    },
    /** Redraws now and then while open, as things change. */
    tick() {
      if (!journal.open) return;
      timer++;
      if (timer % 60 === 0) draw();
    },
    dispose() {
      panel.remove();
    },
  };
  return journal;
}

// ---------- the Packs tab ----------

function settingsPanel(
  panel: PanelUI,
  s: {
    options: Options;
    save: () => void;
    key: () => string;
    secure: () => boolean;
    problem: () => string;
    spent: () => { total: number; now: number };
    setKey: (value: string) => Promise<{ ok: boolean; message?: string }>;
    clearKey: () => Promise<void>;
    resetCity: () => void;
  },
) {
  const { options } = s;
  panel.heading('AI');
  const key = s.key();
  panel.hint(
    key === 'saved'
      ? `Using your saved API key${s.secure() ? ', encrypted by your computer' : ''}. It never leaves the app except to talk to Anthropic.`
      : key === 'environment'
        ? 'Using the API key from ANTHROPIC_API_KEY.'
        : key === 'checking'
          ? 'Checking for an API key…'
          : 'No API key: the city runs on a simpler built-in brain. Add an Anthropic API key (console.anthropic.com) for AI conversations. Using the API costs money; the spend is shown below.',
  );
  const input = el('input', 'text npc-key');
  input.type = 'password';
  input.placeholder = key === 'saved' ? 'Saved (paste a new one to replace it)' : 'sk-ant-…';
  input.autocomplete = 'off';
  const message = el('span', 'npc-small');
  const save = button('Save', 'btn', () => {
    save.disabled = true;
    void s.setKey(input.value).then((result) => {
      save.disabled = false;
      input.value = '';
      if (result.ok) panel.refresh();
      else message.textContent = result.message ?? "Couldn't save it.";
    });
  });
  const clear = button('Forget', 'btn', () => void s.clearKey().then(() => panel.refresh()));
  panel.row('API key', input, save, ...(key === 'saved' ? [clear] : []));
  panel.append(message);
  panel.choice(
    'Model',
    (Object.keys(MODELS) as ModelChoice[]).map((m) => ({ value: m, label: MODELS[m].label.replace('Claude ', '') })),
    () => options.model,
    (v) => ((options.model = v), s.save()),
  );
  panel.choice('AI thinking', [
    { value: 'off', label: 'Off' },
    { value: 'normal', label: 'Normal' },
    { value: 'lots', label: 'Lots' },
  ], () => options.think, (v) => ((options.think = v), s.save()));
  panel.hint('Normal uses the AI for conversations near you and when you talk to someone; Lots uses it more often and further away. Opus writes the richest characters; Haiku is far cheaper.');
  const spent = s.spent();
  panel.hint(`AI spend: $${spent.now.toFixed(2)} this session, $${spent.total.toFixed(2)} in all.${s.problem() ? ` Last problem: ${s.problem()}` : ''}`);
  panel.heading('City');
  panel.choice('Name tags', [
    { value: 'on', label: 'On' },
    { value: 'off', label: 'Off' },
  ], () => (options.tags ? 'on' : 'off'), (v) => ((options.tags = v === 'on'), s.save()));
  let sure = false;
  const reset = button('Reset everyone', 'btn danger', () => {
    if (!sure) {
      sure = true;
      reset.textContent = 'Click again: they forget everything';
      return;
    }
    s.resetCity();
    reset.textContent = 'Done: a fresh start';
    reset.disabled = true;
  });
  panel.row('Memories', reset);
  panel.hint('K: talk to the person next to you (pull up slowly). L: the city journal: what everyone is up to, how they feel and what they remember.');
}
