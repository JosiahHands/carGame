// What the game page and the main process say to each other about thinking:
// the open-source models on offer, the requests and answers, the standing
// instructions, and the shape of every answer (as a JSON schema: the model
// can only write JSON that fits it). Shared by both sides (no browser or
// Electron code here).
import { CAST, EMOTIONS, PLACE_KINDS, TRAIT_NAMES } from './cast';

export type ThinkKind = 'conversation' | 'chat' | 'reflect' | 'decide' | 'plan';

/** What anyone can choose to do next (the AI picks one, with a place and a person where it needs them). */
export const ACTIONS = [
  'carry on',
  'go somewhere',
  'find someone',
  'go home',
  'wander',
  'drive somewhere',
  'give someone a lift',
  'follow someone',
  'avoid someone',
  'work',
  'eat',
  'rest',
  'get up to mischief',
  'text the driver',
  'report the driver to the police',
] as const;
/** What the police officer can also choose (about the driver, or a drive with GUY in the police car). */
export const POLICE_ACTIONS = ['chase the driver down', 'ticket the driver', 'let it go', 'go for a drive with GUY'] as const;
export type Action = (typeof ACTIONS)[number] | (typeof POLICE_ACTIONS)[number];

/** Hours a day plan can use. */
export const PLAN_HOURS = ['6', '7', '8', '9', '10', '11', '12', '13', '14', '15', '16', '17', '18', '19', '20', '21', '22', '23'];

export interface ThinkRequest {
  kind: ThinkKind;
  /** The cast members involved (the speakers, in order). */
  ids: string[];
  /** Everything about the moment: who they are now, what they remember... (see brain.ts). */
  prompt: string;
}

export type ThinkResponse =
  | { ok: true; data: unknown; ms: number }
  | { ok: false; reason: 'not-ready' | 'busy' | 'error'; message: string };

/** The local models to choose from: free, open-source (Apache 2.0) models that run on this computer. */
export type ModelChoice = 'light' | 'balanced' | 'rich';

export interface ModelInfo {
  label: string;
  /** Where to download it from (a Hugging Face repository and quantisation, for node-llama-cpp). */
  uri: string;
  /** The file it's saved as. */
  file: string;
  /** Download size, GB. */
  size: number;
  /** Memory the computer should have to run it comfortably next to the game, GB. */
  memory: number;
  about: string;
}

export const MODELS: Record<ModelChoice, ModelInfo> = {
  light: {
    label: 'Qwen3.5 2B (light)',
    uri: 'hf:unsloth/Qwen3.5-2B-GGUF:Q4_K_M',
    file: 'hf_unsloth_Qwen3.5-2B.Q4_K_M.gguf',
    size: 1.3,
    memory: 8,
    about: 'Quick, and fine on most computers. Simpler conversations.',
  },
  balanced: {
    label: 'Qwen3.5 4B (balanced)',
    uri: 'hf:unsloth/Qwen3.5-4B-GGUF:Q4_K_M',
    file: 'hf_unsloth_Qwen3.5-4B.Q4_K_M.gguf',
    size: 2.7,
    memory: 16,
    about: 'Livelier people with better memories. Needs a computer with 16 GB of memory.',
  },
  rich: {
    label: 'Qwen3.5 9B (rich)',
    uri: 'hf:unsloth/Qwen3.5-9B-GGUF:Q4_K_M',
    file: 'hf_unsloth_Qwen3.5-9B.Q4_K_M.gguf',
    size: 5.7,
    memory: 24,
    about: 'The richest characters. Needs a fast computer with 24 GB of memory or more.',
  },
};

/** The best model for a computer with this much memory (bytes). */
export function defaultModel(totalMemory: number): ModelChoice {
  const gb = totalMemory / 1024 ** 3;
  return gb >= MODELS.balanced.memory - 0.5 ? 'balanced' : 'light';
}

/** Where the local AI is up to, for the Packs tab and the journal. */
export interface AiStatus {
  model: ModelChoice;
  state: 'missing' | 'downloading' | 'loading' | 'ready' | 'error' | 'off';
  /** 0..1 while downloading. */
  progress: number;
  /** Which models are downloaded. */
  downloaded: ModelChoice[];
  /** What it runs on, once loaded ("graphics chip (Metal)"), and whether the graphics chip is chosen. */
  gpu: string;
  onGpu: boolean;
  message: string;
  /** Requests waiting. */
  queue: number;
}

/** The standing instructions: short, as small models follow short rules best. */
export function systemPrompt(): string {
  return `You bring to life the people of a small, sunny city in a driving game. They are ordinary people living ordinary days: working, eating, meeting friends, gossiping, worrying, dreaming. Fast cars race through their streets; the person driving the player's car is "the driver". To them this is simply their city and their life.

They are free. They choose their own goals and change their minds, keep honest private thoughts about anything, like or dislike whoever they want, and do whatever they want among the things they can do: even bend the rules, make mischief, or chase the driver if they are the police.

Rules:
- Stay true to each person: their personality, way of talking, mood, beliefs, feelings and memories. Let them change slowly and surprise a little.
- Spoken lines are short and natural, like real people: at most 20 words each.
- Write in plain English.
- No sexual content and no slurs. No real brands, films, celebrities or real people.
- Never mention being an AI, a game or a simulation.
- Call the player "the driver", or by their name if the person knows it. Nobody knows whether the driver is a man or a woman, so say "they".
- Memories are first person, written by the one remembering, at most 25 words.
- Answer only with JSON in the shape asked for.`;
}

const IMPORTANCE = ['low', 'medium', 'high'];

/** The shape of the answer for each kind of request. */
export function schemaFor(kind: ThinkKind, ids: string[]): Record<string, unknown> {
  const everyone = [...CAST.map((c) => c.id), 'driver'];
  const text = (max: number) => ({ type: 'string', minLength: 2, maxLength: max });
  const oneOf = (values: readonly string[]) => ({ enum: [...values] });
  const object = (properties: Record<string, unknown>) => ({ type: 'object', properties, required: Object.keys(properties), additionalProperties: false });
  switch (kind) {
    case 'conversation':
      return object({
        lines: { type: 'array', minItems: 2, maxItems: 4, items: object({ speaker: oneOf(ids), text: text(140), emotion: oneOf(EMOTIONS) }) },
        memories: { type: 'array', minItems: 1, maxItems: ids.length, items: object({ who: oneOf(ids), text: text(160), importance: oneOf(IMPORTANCE) }) },
        feelings: { type: 'array', maxItems: 4, items: object({ from: oneOf(ids), about: oneOf(everyone), change: oneOf(['warmer', 'cooler']) }) },
        // Plans they make together ("see you at the diner tonight"), which they then keep.
        plan: object({ place: oneOf([...PLACE_KINDS, 'none']), when: oneOf(['soon', 'later', 'tomorrow']) }),
      });
    case 'chat':
      return object({
        reply: text(220),
        emotion: oneOf(EMOTIONS),
        opinion: oneOf(['warmer', 'same', 'cooler']),
        memory: text(160),
        importance: oneOf(IMPORTANCE),
        driverName: { type: 'string', maxLength: 24 },
      });
    case 'reflect':
      return object({
        insights: { type: 'array', minItems: 1, maxItems: 2, items: text(140) },
        trait: oneOf([...TRAIT_NAMES, 'none']),
        direction: oneOf(['up', 'down']),
        habit: { type: 'string', maxLength: 100 },
        story: text(600),
      });
    case 'decide':
      return object({
        // What's on their mind, honestly (no one else hears it).
        thought: text(200),
        // Their goal for now and the days ahead: "" keeps the one they have.
        goal: { type: 'string', maxLength: 140 },
        action: oneOf(ids[0] === 'buddy' ? [...POLICE_ACTIONS, ...ACTIONS] : ACTIONS),
        place: oneOf([...PLACE_KINDS, 'home', 'none']),
        person: oneOf([...everyone, 'none']),
        // Something they say out loud as they do it ("" for nothing).
        say: { type: 'string', maxLength: 120 },
      });
    case 'plan':
      return object({
        goal: text(140),
        plan: { type: 'array', minItems: 3, maxItems: 6, items: object({ hour: oneOf(PLAN_HOURS), place: oneOf([...PLACE_KINDS, 'home']), what: text(80) }) },
      });
  }
}
