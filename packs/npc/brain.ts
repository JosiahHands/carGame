// How the city's people think. Two brains with the same job:
//  - LocalBrain asks the local AI (an open-source model running on this
//    computer, see main.ts): real conversations, between the people and with
//    you, that build on who they are and everything they remember.
//  - RuleBrain needs nothing: it fills in phrases from what they remember
//    and how they feel. It keeps the city going while the model downloads or
//    loads, when the AI is busy, and when it's switched off.
// Both return the same things, and everything they return is checked and
// trimmed before the city uses it.
import { castMember, EMOTIONS, PLACE_KINDS, TRAIT_NAMES, type CastMember, type Emotion, type PlaceKind, type TraitName } from './cast';
import { feelingWord, friendship, moodWord, recall, traitWords, type Mind } from './mind';
import { ACTIONS, POLICE_ACTIONS, type Action, type ThinkKind, type ThinkResponse } from './protocol';

/** A person as the brain sees them. */
export interface Person {
  member: CastMember;
  mind: Mind;
  /** Where they are ("the coffee cart") and what they're up to. */
  where: string;
  doing: string;
  /** How the driver is talking to them: pulled up beside them, on the phone, or giving them a ride. */
  contact?: 'beside' | 'phone' | 'passenger';
}

/** The moment: time of day, what's been happening, who the driver is. */
export interface Moment {
  now: number;
  /** The time in the city, in words ("Tuesday, late afternoon"). */
  time: string;
  /** The hour of the day, 0..24. */
  hour: number;
  /** The player's car, in words ("a red sports car"). */
  car: string;
  /** Things that happened in the city lately, newest first. */
  news: string[];
}

export interface Line {
  speaker: string;
  text: string;
  emotion: Emotion;
}

/** A plan two people make to meet again. */
export interface MeetPlan {
  place: PlaceKind;
  when: 'soon' | 'later' | 'tomorrow';
}

export interface ConversationResult {
  lines: Line[];
  memories: { who: string; text: string; importance: number }[];
  feelings: { from: string; about: string; change: number }[];
  plan: MeetPlan | null;
  /** Whether the AI wrote it (shown in the journal). */
  ai: boolean;
}

export interface ChatResult {
  reply: string;
  emotion: Emotion;
  /** How their opinion of the driver moved: -1, 0 or +1. */
  opinion: number;
  memory: string;
  importance: number;
  driverName: string;
  ai: boolean;
}

export interface ReflectResult {
  insights: string[];
  trait: { name: TraitName; up: boolean } | null;
  habit: string;
  /** Their life story so far, rewritten ('' to keep the old one). */
  story: string;
  ai: boolean;
}

/** What's going on when someone decides what to do next. */
export interface Situation {
  /** Why they're deciding now ("just finished at the diner", "the driver just smashed a car in front of you"). */
  why: string;
  /** Who's around (names). */
  nearby: string[];
  /** For the police officer: what the driver has done lately, and how bad it is (0 nothing .. 5 terrible). */
  crimes?: { list: string[]; heat: number };
}

/** What someone chose to do. */
export interface Decision {
  thought: string;
  goal: string;
  action: Action;
  place: PlaceKind | 'home' | null;
  /** A cast id, 'driver', or null. */
  person: string | null;
  say: string;
  ai: boolean;
}

/** Someone's plan for the day. */
export interface DayPlanResult {
  goal: string;
  plan: { hour: number; place: PlaceKind | 'home'; what: string }[];
  ai: boolean;
}

export interface Brain {
  conversation(a: Person, b: Person, moment: Moment): Promise<ConversationResult>;
  chat(person: Person, said: string, moment: Moment): Promise<ChatResult>;
  reflect(person: Person, moment: Moment): Promise<ReflectResult>;
  decide(person: Person, moment: Moment, situation: Situation): Promise<Decision>;
  plan(person: Person, moment: Moment): Promise<DayPlanResult>;
}

const IMPORTANCE: Record<string, number> = { low: 3, medium: 5, high: 8 };

// ---------- the local AI ----------

export interface LocalSettings {
  /** Calls the pack's main-process part. */
  call: (request: unknown) => Promise<unknown>;
  /** Told why a request didn't work out. */
  failed: (reason: string, message: string) => void;
}

export class LocalBrain implements Brain {
  constructor(
    private settings: LocalSettings,
    private fallback: Brain,
  ) {}

  private async ask(kind: ThinkKind, ids: string[], prompt: string): Promise<unknown | null> {
    try {
      const answer = (await this.settings.call({ kind, ids, prompt })) as ThinkResponse;
      if (!answer.ok) {
        this.settings.failed(answer.reason, answer.message);
        return null;
      }
      return answer.data;
    } catch (error) {
      this.settings.failed('error', error instanceof Error ? error.message : String(error));
      return null;
    }
  }

  async conversation(a: Person, b: Person, moment: Moment): Promise<ConversationResult> {
    const A = a.member;
    const B = b.member;
    const together = a.mind.met[B.id] ?? 0;
    const prompt = [
      scene(moment, a.where, false),
      describe(a, moment, [B.id]),
      describe(b, moment, [A.id]),
      `${A.name} and ${B.name} run into each other at ${a.where}. ${together === 0 ? 'They have never really talked before.' : `They have talked ${together} time${together === 1 ? '' : 's'} before.`}`,
      `Write their conversation: 2 to 4 short lines, taking turns, starting with ${A.name}. Make it real: they might catch up, ask about each other's lives, share gossip about other people, talk about the driver, tease, comfort, flirt a little, argue, or make a plan to meet again. ` +
        `Then: one new memory for each of them (first person), any change in how either feels about the other (or about someone they discussed), and a plan if they agreed to meet somewhere (place "none" if not).`,
    ].join('\n\n');
    const data = (await this.ask('conversation', [A.id, B.id], prompt)) as {
      lines?: { speaker: string; text: string; emotion: string }[];
      memories?: { who: string; text: string; importance: string }[];
      feelings?: { from: string; about: string; change: string }[];
      plan?: { place: string; when: string };
    } | null;
    const ids = [A.id, B.id];
    if (!data?.lines?.length) return this.fallback.conversation(a, b, moment);
    const place = data.plan?.place;
    return {
      lines: data.lines
        .filter((l) => ids.includes(l.speaker) && typeof l.text === 'string')
        .slice(0, 5)
        .map((l) => ({ speaker: l.speaker, text: short(l.text), emotion: emotion(l.emotion) })),
      memories: (data.memories ?? []).filter((m) => ids.includes(m.who)).slice(0, 4).map((m) => ({ who: m.who, text: String(m.text).slice(0, 160), importance: IMPORTANCE[m.importance] ?? 4 })),
      feelings: (data.feelings ?? [])
        .filter((f) => ids.includes(f.from) && f.from !== f.about)
        .slice(0, 6)
        .map((f) => ({ from: f.from, about: f.about, change: f.change === 'warmer' ? 1 : -1 })),
      plan:
        place && place !== 'none' && PLACE_KINDS.includes(place as PlaceKind)
          ? { place: place as PlaceKind, when: data.plan?.when === 'tomorrow' ? 'tomorrow' : data.plan?.when === 'later' ? 'later' : 'soon' }
          : null,
      ai: true,
    };
  }

  async chat(person: Person, said: string, moment: Moment): Promise<ChatResult> {
    const { member, mind } = person;
    const history = mind.chats
      .slice(-6)
      .map((c) => `The driver: "${c.you}"\n${member.name}: "${c.them}"`)
      .join('\n');
    const contact = person.contact ?? 'beside';
    const opening =
      contact === 'phone'
        ? `The driver calls ${member.name} on the phone and says`
        : contact === 'passenger'
          ? `${member.name} is riding in the driver's car, and the driver says`
          : `The driver pulls up next to ${member.name} and says`;
    const prompt = [
      scene(moment, person.where, contact !== 'phone'),
      describe(person, moment, ['driver']),
      `How ${member.name} sees the driver: ${friendship(mind)}${mind.driverName ? `, knows them as ${mind.driverName}` : ''}. They have talked ${mind.met.driver ?? 0} time${mind.met.driver === 1 ? '' : 's'}.`,
      history ? `Their last talks:\n${history}` : 'They have never talked before.',
      `${opening}: "${said.slice(0, 300)}"\n\n` +
        `Write ${member.name}'s reply: at most 2 short sentences, in their own voice, shaped by their mood, their feelings about the driver and what they remember (they may bring up something they remember, ask the driver something back, or invite the driver somewhere). ` +
        'Say how their opinion of the driver changes, and what they will remember about this talk. If the driver said their own name, give it as driverName; otherwise "".',
    ].join('\n\n');
    const data = (await this.ask('chat', [member.id], prompt)) as {
      reply?: string; emotion?: string; opinion?: string; memory?: string; importance?: string; driverName?: string;
    } | null;
    if (!data?.reply) return this.fallback.chat(person, said, moment);
    return {
      reply: String(data.reply).replace(/\s+/g, ' ').trim().slice(0, 220),
      emotion: emotion(data.emotion),
      opinion: data.opinion === 'warmer' ? 1 : data.opinion === 'cooler' ? -1 : 0,
      memory: String(data.memory ?? '').slice(0, 160),
      importance: IMPORTANCE[data.importance ?? ''] ?? 5,
      driverName: String(data.driverName ?? '').replace(/[^\p{L}\p{N} '-]/gu, '').slice(0, 24).trim(),
      ai: true,
    };
  }

  async reflect(person: Person, moment: Moment): Promise<ReflectResult> {
    const { member, mind } = person;
    const memories = recall(mind, moment.now, [], [], 16)
      .map((m) => `- ${m.text}`)
      .join('\n');
    const prompt = [
      describe(person, moment, []),
      `${member.name} has a quiet moment and thinks over lately:\n${memories || '- nothing much'}`,
      'What one or two new beliefs do they form (about themselves, a friend, the city or the driver), each a short sentence in their own words? ' +
        'Does this nudge one personality trait up or down ("none" if not)? Is there a small new habit they pick up ("" for none)? ' +
        `Finally, rewrite their life story so far in the first person, at most 90 words: keep what mattered from their old story, add what matters now (people, the driver, hopes, worries).`,
    ].join('\n\n');
    const data = (await this.ask('reflect', [member.id], prompt)) as { insights?: string[]; trait?: string; direction?: string; habit?: string; story?: string } | null;
    if (!data?.insights) return this.fallback.reflect(person, moment);
    return {
      insights: data.insights.slice(0, 2).map((s) => unquote(String(s)).slice(0, 140)),
      trait: TRAIT_NAMES.includes(data.trait as TraitName) ? { name: data.trait as TraitName, up: data.direction === 'up' } : null,
      habit: String(data.habit ?? '').slice(0, 100),
      story: String(data.story ?? '').slice(0, 700),
      ai: true,
    };
  }

  async decide(person: Person, moment: Moment, situation: Situation): Promise<Decision> {
    const { member, mind } = person;
    const police = member.id === 'buddy';
    // The police officer's own choices first when the driver has been up to something.
    const actions = police ? (situation.crimes ? [...POLICE_ACTIONS, ...ACTIONS] : [...ACTIONS, ...POLICE_ACTIONS]) : [...ACTIONS];
    const step = currentStep(mind, moment);
    const prompt = [
      scene(moment, person.where, false),
      describe(person, moment, []),
      mind.goal ? `${member.name}'s goal: ${mind.goal}` : `${member.name} has no particular goal yet.`,
      mind.plan.length ? `Their plan for today: ${mind.plan.map((p) => `${p.hour}:00 ${p.what} (${p.place})`).join('; ')}.${step ? ` Right now the plan says: ${step.what}.` : ''}` : '',
      situation.nearby.length ? `Nearby: ${situation.nearby.slice(0, 6).join(', ')}.` : 'Nobody they know is nearby.',
      situation.crimes ? `What the driver has done lately: ${situation.crimes.list.slice(0, 5).join('; ') || 'nothing much'}. How serious it is: ${situation.crimes.heat} out of 5.` : '',
      `Now: ${situation.why}`,
      `${member.name} decides freely what to do next: whatever they truly want, from these actions: ${actions.join(', ')}. ` +
        'Give their honest private thought, their goal (or "" to keep it), the action, the place and person it needs (or "none"), and anything they say out loud ("" for nothing).' +
        (police && situation.crimes
          ? ' As the police officer, they choose for themselves what the driver deserves: "chase the driver down" (siren on, after them in the police car), "ticket the driver", or "let it go".'
          : ''),
    ]
      .filter(Boolean)
      .join('\n\n');
    const data = (await this.ask('decide', [member.id], prompt)) as { thought?: string; goal?: string; action?: string; place?: string; person?: string; say?: string } | null;
    if (!data?.action || !actions.includes(data.action as Action)) return this.fallback.decide(person, moment, situation);
    const place = data.place === 'home' ? 'home' : PLACE_KINDS.includes(data.place as PlaceKind) ? (data.place as PlaceKind) : null;
    const who = data.person && data.person !== 'none' && data.person !== member.id && (data.person === 'driver' || castMember(data.person)) ? data.person : null;
    return {
      thought: unquote(String(data.thought ?? '')).slice(0, 200),
      goal: String(data.goal ?? '').slice(0, 140),
      action: data.action as Action,
      place,
      person: who,
      say: short(unquote(String(data.say ?? ''))),
      ai: true,
    };
  }

  async plan(person: Person, moment: Moment): Promise<DayPlanResult> {
    const { member, mind } = person;
    const prompt = [
      describe(person, moment, []),
      mind.goal ? `Their goal so far: ${mind.goal}` : '',
      `It's ${moment.time}. ${member.name} plans their day, freely: what they want from today, given who they are, their job (${member.job ?? 'none'}), their friends, their feelings and their memories. ` +
        'Give their goal (a sentence: what they are working towards these days, kept or changed) and 3 to 6 steps, each from an hour of the day: where, and what they will do there (a few words).',
    ]
      .filter(Boolean)
      .join('\n\n');
    const data = (await this.ask('plan', [member.id], prompt)) as { goal?: string; plan?: { hour: string; place: string; what: string }[] } | null;
    if (!data?.plan?.length) return this.fallback.plan(person, moment);
    return {
      goal: String(data.goal ?? '').slice(0, 140),
      plan: data.plan
        .map((p) => ({ hour: Number(p.hour), place: (p.place === 'home' ? 'home' : p.place) as PlaceKind | 'home', what: String(p.what).slice(0, 60) }))
        .filter((p) => Number.isFinite(p.hour) && (p.place === 'home' || PLACE_KINDS.includes(p.place)))
        .sort((a, b) => a.hour - b.hour),
      ai: true,
    };
  }
}

/** The step of their day plan they're on now, if any. */
export function currentStep(mind: Mind, moment: Moment): Mind['plan'][number] | null {
  const hour = moment.hour;
  let step: Mind['plan'][number] | null = null;
  for (const p of mind.plan) if (p.hour <= hour) step = p;
  return step;
}

/** The time and place, where the driver is, and the latest news. */
function scene(moment: Moment, where: string, driverHere: boolean): string {
  const news = moment.news.length ? `Lately in the city:\n${moment.news.slice(0, 4).map((n) => `- ${n}`).join('\n')}` : '';
  const driver = driverHere ? `The driver has pulled up here in ${moment.car}.` : `The driver (in ${moment.car}) is not here: they are out driving somewhere in the city.`;
  return [`${moment.time}, at ${where}. ${driver}`, news].filter(Boolean).join('\n');
}

/** How someone's needs feel, in words. */
function needsWords(mind: Mind): string {
  const n = mind.needs;
  const words = [n.food < 0.3 ? 'hungry' : '', n.rest < 0.3 ? 'tired' : '', n.social < 0.3 ? 'lonely' : '', n.fun < 0.3 ? 'bored' : ''].filter(Boolean);
  return words.length ? ` and ${words.join(', ')}` : '';
}

/** One person as they are now, with what they remember about these people. */
function describe(person: Person, moment: Moment, about: string[]): string {
  const { member, mind } = person;
  const feelings = Object.entries(mind.feelings)
    .filter(([id, v]) => Math.abs(v) > 0.2 || about.includes(id))
    .sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]))
    .slice(0, 5)
    .map(([id, v]) => `${feelingWord(v)} ${id === 'driver' ? 'the driver' + (mind.driverName ? ` (${mind.driverName})` : '') : (castMember(id)?.name ?? id)}`);
  // The same thing remembered twice is only worth saying once.
  const memories = [...new Set(recall(mind, moment.now, about, [], 12).map((m) => m.text))].slice(0, 6).map((text) => `- ${text}`);
  return [
    `${member.name}, ${member.role}. ${member.bio} Talks: ${member.voice}.`,
    `Personality now: ${traitWords(mind.traits)}.`,
    mind.story ? `Their life so far: ${mind.story}` : '',
    mind.insights.length ? `Believes: ${mind.insights.slice(-3).join(' ')}` : '',
    mind.habits.length ? `Habits: ${mind.habits.join('; ')}.` : '',
    mind.thoughts.length ? `Lately thinking: ${mind.thoughts.slice(-2).join(' ')}` : '',
    feelings.length ? `${member.name} ${feelings.join(', ')}.` : '',
    `Right now: ${person.doing}, feeling ${moodWord(mind)}${mind.emotion !== 'calm' ? `, ${mind.emotion}` : ''}${needsWords(mind)}.`,
    memories.length ? `${member.name} remembers:\n${memories.join('\n')}` : '',
  ]
    .filter(Boolean)
    .join('\n');
}

/** At most 20 words, so a speech bubble stays readable. */
function short(text: string): string {
  const words = String(text).replace(/\s+/g, ' ').trim().split(' ');
  return words.length <= 20 ? words.join(' ') : words.slice(0, 20).join(' ').replace(/[,;:]$/, '') + '…';
}

/** Takes off quote marks wrapped round a whole sentence. */
function unquote(text: string): string {
  return text.trim().replace(/^["“'](.*)["”']$/s, '$1').trim();
}

function emotion(value: unknown): Emotion {
  return EMOTIONS.includes(value as Emotion) ? (value as Emotion) : 'calm';
}

// ---------- the rule brain ----------

const pick = <T,>(list: readonly T[]): T => list[Math.floor(Math.random() * list.length)];

export class RuleBrain implements Brain {
  async conversation(a: Person, b: Person, moment: Moment): Promise<ConversationResult> {
    const A = a.member;
    const B = b.member;
    const fondness = a.mind.feelings[B.id] ?? 0;
    const greet =
      A.bond === B.id || B.bond === A.id
        ? pick([`There's my favourite person! Hey, ${B.name}.`, `${B.name}! Right where I left you.`, `Partner! Ready for whatever today brings?`])
        : fondness > 0.4
          ? pick([`Hey ${B.name}! Good to see you.`, `${B.name}! How's your day going?`, `Look who it is! ${B.name}!`])
          : fondness < -0.2
            ? pick([`Oh. Hello, ${B.name}.`, `${B.name}. Didn't see you there.`])
            : pick([`Afternoon, ${B.name}.`, `Hi there, ${B.name}.`, `Oh, hello ${B.name}.`]);
    // Something to talk about: the driver, some gossip, a belief, or how they're doing.
    const aboutDriver = recall(a.mind, moment.now, ['driver'], [], 3).find((m) => m.about.includes('driver'));
    const gossip = recall(a.mind, moment.now, [], [], 8).find((m) => m.about.some((id) => id !== 'driver' && id !== B.id && id !== A.id) && m.kind !== 'heard');
    const lines: Line[] = [{ speaker: A.id, text: greet, emotion: fondness > 0.3 ? 'happy' : 'calm' }];
    const memories: ConversationResult['memories'] = [];
    const feelings: ConversationResult['feelings'] = [];
    const driverLike = a.mind.feelings.driver ?? 0;
    if (aboutDriver && Math.random() < 0.6) {
      lines.push({
        speaker: A.id,
        text:
          driverLike > 0.2
            ? pick([`Did you see the driver in ${moment.car}? Incredible!`, `That driver in ${moment.car} is something else.`])
            : driverLike < -0.2
              ? pick([`That driver in ${moment.car} nearly gave me a heart attack.`, `Someone should slow that ${moment.car.replace(/^an? /, '')} down.`])
              : pick([`Have you seen the driver in ${moment.car}?`, `What do you make of that driver in ${moment.car}?`]),
        emotion: driverLike > 0.2 ? 'excited' : driverLike < -0.2 ? 'annoyed' : 'curious',
      });
      const agrees = (b.mind.feelings.driver ?? 0) * driverLike >= 0;
      lines.push({ speaker: B.id, text: agrees ? pick(['Tell me about it!', 'I saw that too!', 'You are not wrong.']) : pick(['Really? I kind of like them.', "Oh, they're harmless.", "I don't know, I think they're fun."]), emotion: agrees ? 'amused' : 'curious' });
      memories.push({ who: B.id, text: `${A.name} told me about the driver in ${moment.car}.`, importance: 3 });
      feelings.push({ from: B.id, about: 'driver', change: driverLike > 0 ? 1 : -1 });
    } else if (gossip && Math.random() < 0.6) {
      const who = castMember(gossip.about.find((id) => id !== 'driver' && id !== B.id && id !== A.id) ?? '')?.name ?? 'someone';
      lines.push({ speaker: A.id, text: pick([`Did you hear about ${who}?`, `Between you and me, ${who} has been up to something.`, `So I was talking to ${who}...`]), emotion: 'curious' });
      lines.push({ speaker: B.id, text: pick(['No way! Tell me everything.', 'I always wondered about that.', 'Ha! That sounds about right.']), emotion: 'amused' });
      memories.push({ who: B.id, text: `${A.name} told me: ${gossip.text}`, importance: Math.max(2, Math.round(gossip.importance * 0.7)) });
    } else if (a.mind.insights.length && Math.random() < 0.5) {
      lines.push({ speaker: A.id, text: short(`You know what I've realised? ${a.mind.insights[a.mind.insights.length - 1]}`), emotion: 'proud' });
      lines.push({ speaker: B.id, text: pick(['Huh. I never thought of it like that.', 'That is very deep.', 'You might be onto something.']), emotion: 'curious' });
    } else {
      const needs = a.mind.needs;
      const line =
        needs.food < 0.35 ? "I'm starving. Want to grab something to eat?" : needs.rest < 0.3 ? "I'm running on empty today." : needs.fun < 0.35 ? 'I need some excitement in my life.' : pick(["Beautiful day, isn't it?", 'Same old, same old.', "Busy day! You know how it is.", `Nice spot, ${a.where}.`]);
      lines.push({ speaker: A.id, text: line, emotion: needs.fun < 0.35 ? 'sad' : 'happy' });
      lines.push({ speaker: B.id, text: pick(['I hear you.', "Let's make it a good one.", 'Hang in there!', 'Same here, honestly.']), emotion: 'calm' });
    }
    // Friendly chats bring people closer; prickly people less so.
    if ((a.member.traits.kindness + b.member.traits.kindness) / 2 > 0.45) {
      feelings.push({ from: A.id, about: B.id, change: 1 }, { from: B.id, about: A.id, change: 1 });
    }
    memories.push({ who: A.id, text: `Chatted with ${B.name} at ${a.where}.`, importance: 2 });
    // Friends sometimes agree to meet again, somewhere one of them likes.
    let plan: MeetPlan | null = null;
    const liked = A.likes.filter((kind) => B.likes.includes(kind))[0] ?? A.likes[0];
    if (fondness > 0.35 && liked && Math.random() < 0.3 && lines.length < 4) {
      plan = { place: liked, when: pick(['soon', 'later', 'tomorrow'] as const) };
      lines.push({ speaker: A.id, text: `Meet me at the ${liked} ${plan.when === 'tomorrow' ? 'tomorrow' : 'later'}?`, emotion: 'happy' });
      lines.push({ speaker: B.id, text: pick(["It's a date!", "I'll be there.", 'Wouldn\'t miss it.']), emotion: 'happy' });
    }
    return { lines: lines.slice(0, 5), memories, feelings, plan, ai: false };
  }

  async chat(person: Person, said: string): Promise<ChatResult> {
    const opinion = person.mind.feelings.driver ?? 0;
    const text = said.toLowerCase();
    const named = said.match(/(?:my name is|i'm|i am|call me)\s+([A-Za-z][A-Za-z'-]{1,20})/i)?.[1] ?? '';
    const name = named || person.mind.driverName || '';
    let reply: string;
    if (named) reply = pick([`Nice to meet you, ${named}! I'm ${person.member.name}.`, `${named}, huh? I'll remember that.`]);
    else if (/\b(hi|hello|hey|morning|evening)\b/.test(text)) reply = opinion < -0.3 ? pick(['Oh. It\'s you.', 'You again.']) : pick([`Hello${name ? ` ${name}` : ''}! Lovely day for a drive.`, `Hey there${name ? `, ${name}` : ''}!`]);
    else if (/how are you|how's it going|you ok/.test(text)) reply = `Feeling ${moodWord(person.mind)}, thanks for asking!`;
    else if (/sorry/.test(text)) reply = opinion < 0 ? 'Apology accepted. Drive safe, okay?' : 'No harm done!';
    else if (/car|drive|jump|fast|speed/.test(text)) reply = opinion > 0.2 ? 'Your driving is the best show in town!' : 'Maybe ease off the pedal a little?';
    else if (/job|work/.test(text)) reply = person.member.job ? `I work at the ${person.member.job}. Come by sometime!` : `Work? I'm a ${person.member.role}. It's a calling.`;
    else reply = pick(['Ha! You are a funny one.', "Interesting. I'll have to think about that.", 'You know, I like talking to you.', "That's not something you hear every day!"]);
    const warmer = /sorry|thank|nice|love|great|friend/.test(text);
    return {
      reply: short(reply),
      emotion: opinion < -0.3 ? 'annoyed' : 'happy',
      opinion: warmer ? 1 : 0,
      memory: `The driver${name ? ` (${name})` : ''} stopped to chat with me: "${said.slice(0, 60)}"`,
      importance: 5,
      driverName: named,
      ai: false,
    };
  }

  async reflect(person: Person, moment: Moment): Promise<ReflectResult> {
    const mind = person.mind;
    const insights: string[] = [];
    const driver = mind.feelings.driver ?? 0;
    const driverMemories = mind.memories.filter((m) => m.about.includes('driver')).length;
    if (driverMemories >= 3) insights.push(driver > 0.2 ? `The driver in ${moment.car} makes this city exciting.` : driver < -0.2 ? `The driver in ${moment.car} is a menace.` : `I can't make my mind up about that driver.`);
    const best = Object.entries(mind.feelings).filter(([id]) => id !== 'driver').sort((a, b) => b[1] - a[1])[0];
    if (best && best[1] > 0.5) insights.push(`${castMember(best[0])?.name ?? best[0]} is one of the good ones.`);
    const favourite = Object.entries(mind.likes).sort((a, b) => (b[1] ?? 0) - (a[1] ?? 0))[0];
    const scared = mind.memories.slice(-12).filter((m) => m.importance >= 7 && m.kind === 'saw').length;
    return {
      insights: insights.slice(0, 2),
      trait: scared >= 3 ? { name: 'courage', up: person.member.traits.courage > 0.6 } : mind.mood > 0.4 ? { name: 'humour', up: true } : null,
      habit: favourite && (favourite[1] ?? 0) > 0.5 ? `heads to the ${favourite[0]} whenever there's a spare moment` : '',
      story: '',
      ai: false,
    };
  }

  async decide(person: Person, moment: Moment, situation: Situation): Promise<Decision> {
    const { mind } = person;
    const n = mind.needs;
    const decision = (action: Action, thought: string, extra: Partial<Decision> = {}): Decision => ({ thought, goal: '', action, place: null, person: null, say: '', ai: false, ...extra });
    // The police officer: how bad has the driver been?
    if (situation.crimes) {
      const heat = situation.crimes.heat;
      const soft = (mind.feelings.driver ?? 0) > 0.55;
      if (heat >= (soft ? 4 : 3)) return decision('chase the driver down', 'That driver has gone too far this time.', { say: pick(['Pull over! Now!', "That's it, you're nicked!", 'Stop right there!']) });
      if (heat >= 2) return decision('ticket the driver', 'Time somebody wrote that driver a ticket.');
      return decision('let it go', "Not worth the paperwork. Yet.");
    }
    if (n.food < 0.3) return decision('eat', "I'm starving.", { place: pick(['diner', 'bakery', 'coffee'] as const) });
    if (n.rest < 0.25) return decision(moment.hour >= 20 ? 'go home' : 'rest', 'I can barely keep my eyes open.', { place: moment.hour >= 20 ? 'home' : 'park' });
    if (n.social < 0.3) {
      const friend = Object.entries(mind.feelings).filter(([id, v]) => id !== 'driver' && v > 0.4).sort((a, b) => b[1] - a[1])[0];
      if (friend) return decision('find someone', `I miss ${castMember(friend[0])?.name ?? 'my friend'}.`, { person: friend[0] });
    }
    // Mischief, now and then, for the prickly and the bored.
    if (mind.traits.kindness < 0.4 && n.fun < 0.5 && Math.random() < 0.15) return decision('get up to mischief', 'Nobody is watching. Probably.');
    // Friends of the driver send a message sometimes.
    if ((mind.feelings.driver ?? 0) > 0.45 && Math.random() < 0.08) {
      return decision('text the driver', 'I wonder what the driver is up to.', { say: pick(['Hey! Fancy meeting at the diner later?', 'Saw you fly past earlier. Show-off!', 'Come by and say hi sometime!']) });
    }
    return decision('carry on', pick(['Same as always.', 'Not a bad day.', `I wonder what's going on at ${person.where}.`]));
  }

  async plan(person: Person): Promise<DayPlanResult> {
    const { member, mind } = person;
    const likes = Object.entries(mind.likes).sort((a, b) => (b[1] ?? 0) - (a[1] ?? 0)).map(([k]) => k as PlaceKind);
    const fun = likes.find((k) => k !== member.job) ?? 'park';
    const plan: DayPlanResult['plan'] = [{ hour: 7, place: 'coffee', what: 'grab a coffee' }];
    if (member.job) plan.push({ hour: 9, place: member.job, what: 'work' }, { hour: 12, place: 'diner', what: 'lunch' }, { hour: 13, place: member.job, what: 'back to work' });
    else plan.push({ hour: 10, place: fun, what: 'spend the morning' }, { hour: 12, place: 'bakery', what: 'lunch' });
    plan.push({ hour: 18, place: fun, what: 'unwind' }, { hour: 22, place: 'home', what: 'head home' });
    return { goal: mind.goal || `Make the most of ${member.job ? 'work and ' : ''}the city.`, plan, ai: false };
  }

}
