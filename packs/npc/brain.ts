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
import type { ThinkKind, ThinkResponse } from './protocol';

/** A person as the brain sees them. */
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
  /** The time in the city, in words ("Tuesday, late afternoon"). */
  time: string;
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

export interface Brain {
  conversation(a: Person, b: Person, moment: Moment): Promise<ConversationResult>;
  chat(person: Person, said: string, moment: Moment): Promise<ChatResult>;
  reflect(person: Person, moment: Moment): Promise<ReflectResult>;
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
    const prompt = [
      scene(moment, person.where, true),
      describe(person, moment, ['driver']),
      `How ${member.name} sees the driver: ${friendship(mind)}${mind.driverName ? `, knows them as ${mind.driverName}` : ''}. They have talked ${mind.met.driver ?? 0} time${mind.met.driver === 1 ? '' : 's'}.`,
      history ? `Their last talks:\n${history}` : 'They have never talked before.',
      `The driver pulls up next to ${member.name} and says: "${said.slice(0, 300)}"\n\n` +
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
}
