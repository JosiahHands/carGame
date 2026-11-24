// What the game page and the main process say to each other about thinking:
// the request, the answer, the models, and the instructions and answer shapes
// for the AI. Shared by both sides (no browser or Electron code here).
import { CAST, EMOTIONS, TRAIT_NAMES } from './cast';

export type ThinkKind = 'conversation' | 'chat' | 'reflect';

export interface ThinkRequest {
  kind: ThinkKind;
  /** The cast members involved (the speakers, in order). */
  ids: string[];
  /** Everything about the moment: who they are now, what they remember... (see brain.ts). */
  prompt: string;
  model: ModelChoice;
}

export type ThinkResponse =
  | { ok: true; data: unknown; cost: number; model: string }
  | { ok: false; reason: 'no-key' | 'refused' | 'busy' | 'error'; message: string };

export type ModelChoice = 'opus' | 'sonnet' | 'haiku';

/** The models on offer, with their prices (US$ per million tokens) for the spend counter. */
export const MODELS: Record<ModelChoice, { id: string; label: string; input: number; output: number; cacheRead: number; cacheWrite: number; fallbacks: boolean }> = {
  opus: { id: 'claude-opus-5-5', label: 'Claude Opus 5.5', input: 4, output: 20, cacheRead: 0.4, cacheWrite: 5, fallbacks: true },
  sonnet: { id: 'claude-sonnet-5-5', label: 'Claude Sonnet 5.5', input: 2, output: 10, cacheRead: 0.1, cacheWrite: 2.5, fallbacks: true },
  haiku: { id: 'claude-haiku-5-5', label: 'Claude Haiku 5.5', input: 0.1, output: 0.5, cacheRead: 0.01, cacheWrite: 0.125, fallbacks: false },
};

/** The standing instructions: the same for every request, so they're cached. */
export function systemPrompt(): string {
  const people = CAST.map(
    (c) => `- ${c.name} (id "${c.id}"), ${c.role}. ${c.bio} Voice: ${c.voice}.${c.bond ? ` Always goes everywhere with ${c.bond.toUpperCase()}.` : ''}`,
  ).join('\n');
  return `You bring to life the people of a small, sunny city in a driving game. They are ordinary people living their days: working, getting coffee, meeting friends, gossiping. Fast cars race through their streets, jumping ramps and drifting round corners; the person driving the player's car is "the driver". The people don't know they are in a game: to them this is simply their city and their life.

Each person has a personality that grows over time. You'll be told how they are now (their traits, mood, beliefs they've formed, habits, and how they feel about others) and what they remember. Keep them consistent with all of it, let them change gradually as things happen, and let them surprise you in small, human ways. Old friends talk like old friends; strangers are polite; people hold grudges and soften slowly.

The people of the city:
${people}

Rules for everything you write:
- Spoken lines are short and natural: at most 15 words each, like real people chatting on a street corner.
- Family-friendly: no swearing, no violence beyond cartoon car chaos, nothing cruel.
- No real-world brands, films, shows, songs, celebrities or other real people. Everything is invented and local to this city.
- Never mention being an AI, a game, code or a simulation. Stay inside the people's lives.
- Refer to the player as "the driver" (or by name, if the person has learned it), describing their car if useful.
- Memories are written in the first person by the one remembering, at most 20 words.
- Only use the people listed above.
- Answer only in the requested JSON shape.`;
}

const IMPORTANCE = ['low', 'medium', 'high'];

/** The shape of the answer for each kind of request (structured outputs, so it always parses). */
export function schemaFor(kind: ThinkKind, ids: string[]): Record<string, unknown> {
  const everyone = [...CAST.map((c) => c.id), 'driver'];
  const str = { type: 'string' };
  const oneOf = (values: readonly string[]) => ({ type: 'string', enum: [...values] });
  const object = (properties: Record<string, unknown>) => ({ type: 'object', additionalProperties: false, required: Object.keys(properties), properties });
  switch (kind) {
    case 'conversation':
      return object({
        lines: { type: 'array', items: object({ speaker: oneOf(ids), text: str, emotion: oneOf(EMOTIONS) }) },
        memories: { type: 'array', items: object({ who: oneOf(ids), text: str, importance: oneOf(IMPORTANCE) }) },
        feelings: { type: 'array', items: object({ from: oneOf(ids), about: oneOf(everyone), change: oneOf(['warmer', 'cooler']) }) },
      });
    case 'chat':
      return object({
        reply: str,
        emotion: oneOf(EMOTIONS),
        opinion: oneOf(['warmer', 'same', 'cooler']),
        memory: str,
        importance: oneOf(IMPORTANCE),
        driverName: str,
      });
    case 'reflect':
      return object({
        insights: { type: 'array', items: str },
        trait: oneOf([...TRAIT_NAMES, 'none']),
        direction: oneOf(['up', 'down']),
        habit: str,
      });
  }
}
