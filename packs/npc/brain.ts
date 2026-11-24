// How the cast think. Two brains with the same job:
//  - AiBrain asks Claude (through the main process, see main.ts): real,
//    surprising conversations that build on everything they remember.
//  - RuleBrain needs nothing: it fills in phrases from what they remember
//    and how they feel. It's used with no API key, when the AI is busy or
//    declines, and for conversations too far away to hear.
// Both return the same things, and everything they return is checked and
// trimmed before the city uses it.
import { castMember, EMOTIONS, TRAIT_NAMES, type CastMember, type Emotion, type TraitName } from './cast';
import { feelingWord, moodWord, recall, traitWords, type Mind } from './mind';
import type { ModelChoice, ThinkKind, ThinkResponse } from './protocol';

/** A cast member as the brain sees them. */
export interface Person {
  member: CastMember;
  mind: Mind;
  /** Where they are ("the coffee cart") and what they're up to. */
  where: string;
  doing: string;
}

/** The moment: time of day, what's been happening, who the driver is. */
export interface Moment {
  now: number;
  night: boolean;
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

export interface ConversationResult {
  lines: Line[];
  memories: { who: string; text: string; importance: number }[];
  feelings: { from: string; about: string; change: number }[];
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
  ai: boolean;
}

export interface Brain {
  conversation(a: Person, b: Person, moment: Moment): Promise<ConversationResult>;
  chat(person: Person, said: string, moment: Moment): Promise<ChatResult>;
  reflect(person: Person, moment: Moment): Promise<ReflectResult>;
}

const IMPORTANCE: Record<string, number> = { low: 3, medium: 5, high: 8 };

// ---------- the AI brain ----------

export interface AiSettings {
  model: ModelChoice;
  /** Calls the pack's main-process part. */
  call: (request: unknown) => Promise<unknown>;
  /** Told the cost of each answer, and why a request failed. */
  spent: (dollars: number) => void;
  failed: (reason: string, message: string) => void;
}

export class AiBrain implements Brain {
  constructor(
    private settings: AiSettings,
    private fallback: Brain,
  ) {}

  private async ask(kind: ThinkKind, ids: string[], prompt: string): Promise<unknown | null> {
    try {
      const answer = (await this.settings.call({ kind, ids, prompt, model: this.settings.model })) as ThinkResponse;
      if (!answer.ok) {
        this.settings.failed(answer.reason, answer.message);
        return null;
      }
      this.settings.spent(answer.cost);
      return answer.data;
    } catch (error) {
      this.settings.failed('error', error instanceof Error ? error.message : String(error));
      return null;
    }
  }

  async conversation(a: Person, b: Person, moment: Moment): Promise<ConversationResult> {
    const prompt = [
      scene(moment, a.where),
      describe(a, moment, [b.member.id]),
      describe(b, moment, [a.member.id]),
      `${a.member.name} and ${b.member.name} run into each other at ${a.where}. Write their conversation: 2 to 4 short lines, taking turns, starting with ${a.member.name}. ` +
        'They might catch up, share gossip or something they saw (maybe the driver), tease each other, make plans, or disagree, whatever fits who they are now and what they remember. ' +
        'Then give one new memory for each of them, and any change in how either feels about the other or anyone they talked about.',
    ].join('\n\n');
    const data = (await this.ask('conversation', [a.member.id, b.member.id], prompt)) as {
      lines?: { speaker: string; text: string; emotion: string }[];
      memories?: { who: string; text: string; importance: string }[];
      feelings?: { from: string; about: string; change: string }[];
    } | null;
    const ids = [a.member.id, b.member.id];
    if (!data?.lines?.length) return this.fallback.conversation(a, b, moment);
    return {
      lines: data.lines
        .filter((l) => ids.includes(l.speaker))
        .slice(0, 5)
        .map((l) => ({ speaker: l.speaker, text: short(l.text), emotion: emotion(l.emotion) })),
      memories: (data.memories ?? []).filter((m) => ids.includes(m.who)).slice(0, 4).map((m) => ({ who: m.who, text: m.text.slice(0, 160), importance: IMPORTANCE[m.importance] ?? 4 })),
      feelings: (data.feelings ?? [])
        .filter((f) => ids.includes(f.from) && f.from !== f.about)
        .slice(0, 6)
        .map((f) => ({ from: f.from, about: f.about, change: f.change === 'warmer' ? 1 : -1 })),
      ai: true,
    };
  }

  async chat(person: Person, said: string, moment: Moment): Promise<ChatResult> {
    const prompt = [
      scene(moment, person.where),
      describe(person, moment, ['driver']),
      `The driver has pulled up next to ${person.member.name} and says: "${said.slice(0, 300)}"\n\n` +
        `Write ${person.member.name}'s reply (at most 15 words, in their voice, shaped by how they feel about the driver and what they remember). ` +
        'Say how their opinion of the driver changes, and what they will remember about this. If the driver said their name, give it as driverName; otherwise driverName is "".',
    ].join('\n\n');
    const data = (await this.ask('chat', [person.member.id], prompt)) as {
      reply?: string; emotion?: string; opinion?: string; memory?: string; importance?: string; driverName?: string;
    } | null;
    if (!data?.reply) return this.fallback.chat(person, said, moment);
    return {
      reply: short(data.reply),
      emotion: emotion(data.emotion),
      opinion: data.opinion === 'warmer' ? 1 : data.opinion === 'cooler' ? -1 : 0,
      memory: (data.memory ?? '').slice(0, 160),
      importance: IMPORTANCE[data.importance ?? ''] ?? 5,
      driverName: (data.driverName ?? '').replace(/[^\p{L}\p{N} '-]/gu, '').slice(0, 24).trim(),
      ai: true,
    };
  }

  async reflect(person: Person, moment: Moment): Promise<ReflectResult> {
    const memories = recall(person.mind, moment.now, [], [], 14)
      .map((m) => `- ${m.text}`)
      .join('\n');
    const prompt = [
      describe(person, moment, []),
      `${person.member.name} takes a quiet moment to think over lately:\n${memories}\n\n` +
        'What one or two new beliefs do they form (about themselves, a friend, the city or the driver), each a short sentence in their own words? ' +
        'Does this experience nudge one personality trait up or down (or "none")? And is there a small new habit they pick up (or "" for none)?',
    ].join('\n\n');
    const data = (await this.ask('reflect', [person.member.id], prompt)) as { insights?: string[]; trait?: string; direction?: string; habit?: string } | null;
    if (!data?.insights) return this.fallback.reflect(person, moment);
    return {
      insights: data.insights.slice(0, 2).map((s) => s.slice(0, 140)),
      trait: TRAIT_NAMES.includes(data.trait as TraitName) ? { name: data.trait as TraitName, up: data.direction === 'up' } : null,
      habit: (data.habit ?? '').slice(0, 100),
      ai: true,
    };
  }
}

/** The time and place, and the latest news. */
function scene(moment: Moment, where: string): string {
  const news = moment.news.length ? `Lately in the city:\n${moment.news.slice(0, 5).map((n) => `- ${n}`).join('\n')}` : 'A quiet spell in the city.';
  return `It's ${moment.night ? 'night' : 'daytime'} at ${where}. The driver is in ${moment.car}.\n${news}`;
}

/** One person as they are now, with what they remember about these people. */
function describe(person: Person, moment: Moment, about: string[]): string {
  const { member, mind } = person;
  const feelings = Object.entries(mind.feelings)
    .filter(([id, v]) => Math.abs(v) > 0.12 || about.includes(id))
    .sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]))
    .slice(0, 6)
    .map(([id, v]) => `${feelingWord(v)} ${id === 'driver' ? 'the driver' + (mind.driverName ? ` (${mind.driverName})` : '') : (castMember(id)?.name ?? id)}`);
  const memories = recall(mind, moment.now, about, [], 6).map((m) => `- ${m.text}`);
  return [
    `${member.name} (${member.role}). Right now: ${person.doing}, feeling ${moodWord(mind)}${mind.emotion !== 'calm' ? ` and ${mind.emotion}` : ''}.`,
    `Personality now: ${traitWords(mind.traits)}.`,
    mind.insights.length ? `Has come to believe: ${mind.insights.join(' ')}` : '',
    mind.habits.length ? `Habits: ${mind.habits.join('; ')}.` : '',
    feelings.length ? `Feelings: ${member.name} ${feelings.join(', ')}.` : '',
    memories.length ? `${member.name} remembers:\n${memories.join('\n')}` : `${member.name} has nothing much on their mind.`,
  ]
    .filter(Boolean)
    .join('\n');
}

/** At most 15 words, as the speech bubbles promise. */
function short(text: string): string {
  const words = text.replace(/\s+/g, ' ').trim().split(' ');
  return words.length <= 15 ? words.join(' ') : words.slice(0, 15).join(' ').replace(/[,;:]$/, '') + '…';
}

function emotion(value: unknown): Emotion {
  return EMOTIONS.includes(value as Emotion) ? (value as Emotion) : 'calm';
}

// ---------- the rule brain ----------

const pick = <T,>(list: T[]): T => list[Math.floor(Math.random() * list.length)];

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
    return { lines: lines.slice(0, 4), memories, feelings, ai: false };
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
      ai: false,
    };
  }
}
