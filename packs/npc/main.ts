// The Living City pack's main-process part: it talks to Claude for the cast.
// The API key never reaches the game page. It's kept encrypted by the
// operating system (Electron's safeStorage) in the pack's own data folder,
// or read from the ANTHROPIC_API_KEY environment variable.
//
// Every request uses structured outputs (the answer always matches the shape
// in protocol.ts), marks the standing instructions for caching (cheaper when
// the API caches them), keeps effort low (quick, short answers), and, on Opus and
// Sonnet, lets the API retry a declined request on a fallback model.
import Anthropic from '@anthropic-ai/sdk';
import { safeStorage } from 'electron';
import * as fs from 'node:fs';
import * as path from 'node:path';
import type { PackMainHost } from '../../app/electron/main';
import { CAST } from './cast';
import { MODELS, schemaFor, systemPrompt, type ModelChoice, type ThinkRequest, type ThinkResponse } from './protocol';

/** At most this many requests a minute, however busy the city gets. */
const PER_MINUTE = 20;

export function activate(host: PackMainHost): void {
  const keyFile = path.join(host.dataDir, 'api-key.bin');
  let client: Anthropic | null = null;
  let source: 'saved' | 'environment' | 'none' = 'none';
  const recent: number[] = []; // when the last requests were made
  let running = 0;
  const system = systemPrompt();

  /** Makes the client from the saved key, or the environment. */
  const connect = async () => {
    client = null;
    source = 'none';
    try {
      if (fs.existsSync(keyFile) && (await safeStorage.isAsyncEncryptionAvailable())) {
        const { result } = await safeStorage.decryptStringAsync(fs.readFileSync(keyFile));
        if (result) {
          client = new Anthropic({ apiKey: result });
          source = 'saved';
          return;
        }
      }
    } catch (error) {
      console.error('Living City: could not read the saved API key:', error);
    }
    if (process.env.ANTHROPIC_API_KEY) {
      client = new Anthropic(); // reads the key from the environment
      source = 'environment';
    }
  };
  const ready = connect();

  host.handle('status', async () => {
    await ready;
    return { key: source, secure: await safeStorage.isAsyncEncryptionAvailable() };
  });

  host.handle('setKey', async (key: unknown) => {
    if (typeof key !== 'string' || !/^[\w-]{20,300}$/.test(key.trim())) return { ok: false, message: "That doesn't look like an API key." };
    if (!(await safeStorage.isAsyncEncryptionAvailable())) {
      return { ok: false, message: 'This computer has no secure storage for the key. Set ANTHROPIC_API_KEY before starting the game instead.' };
    }
    fs.writeFileSync(keyFile, await safeStorage.encryptStringAsync(key.trim()), { mode: 0o600 });
    await connect();
    return { ok: true };
  });

  host.handle('clearKey', async () => {
    fs.rmSync(keyFile, { force: true });
    await connect();
    return { ok: true };
  });

  host.handle('think', async (raw: unknown): Promise<ThinkResponse> => {
    await ready;
    const request = checkRequest(raw);
    if (!request) return { ok: false, reason: 'error', message: 'Bad request.' };
    if (!client) return { ok: false, reason: 'no-key', message: 'No API key.' };
    const now = Date.now();
    while (recent.length && now - recent[0] > 60_000) recent.shift();
    if (recent.length >= PER_MINUTE || running >= 3) return { ok: false, reason: 'busy', message: 'Thinking too fast.' };
    recent.push(now);
    running++;
    const model = MODELS[request.model];
    try {
      const response = await client.beta.messages.create({
        model: model.id,
        max_tokens: 4000,
        // The standing instructions are the same every time, so the API can cache them (once they're long enough to qualify).
        system: [{ type: 'text', text: system, cache_control: { type: 'ephemeral' } }],
        messages: [{ role: 'user', content: request.prompt }],
        output_config: { effort: 'low', format: { type: 'json_schema', schema: schemaFor(request.kind, request.ids) } },
        // On a declined request, the API tries again on the recommended fallback model.
        ...(model.fallbacks ? { betas: ['server-side-fallback-2026-07-01'], fallbacks: 'default' as const } : {}),
      });
      const u = response.usage;
      const cost =
        ((u.input_tokens ?? 0) * model.input +
          (u.output_tokens ?? 0) * model.output +
          (u.cache_read_input_tokens ?? 0) * model.cacheRead +
          (u.cache_creation_input_tokens ?? 0) * model.cacheWrite) /
        1_000_000;
      if (response.stop_reason === 'refusal') return { ok: false, reason: 'refused', message: 'Declined.' };
      const text = response.content.flatMap((block) => (block.type === 'text' ? [block.text] : [])).join('');
      return { ok: true, data: JSON.parse(text), cost, model: response.model };
    } catch (error) {
      if (error instanceof Anthropic.AuthenticationError) return { ok: false, reason: 'no-key', message: 'The API key was not accepted.' };
      if (error instanceof Anthropic.RateLimitError) return { ok: false, reason: 'busy', message: 'Rate limited.' };
      return { ok: false, reason: 'error', message: error instanceof Error ? error.message.slice(0, 200) : 'Request failed.' };
    } finally {
      running--;
    }
  });
}

/** Only well-formed requests about real cast members get through. */
function checkRequest(raw: unknown): ThinkRequest | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Partial<ThinkRequest>;
  const kinds = ['conversation', 'chat', 'reflect'];
  if (!kinds.includes(r.kind as string) || !Array.isArray(r.ids) || typeof r.prompt !== 'string') return null;
  if (typeof r.model !== 'string' || !Object.hasOwn(MODELS, r.model)) return null;
  if (r.ids.length < 1 || r.ids.length > 4 || !r.ids.every((id) => CAST.some((c) => c.id === id))) return null;
  if (r.prompt.length > 12_000) return null;
  return { kind: r.kind as ThinkRequest['kind'], ids: r.ids, prompt: r.prompt, model: r.model as ModelChoice };
}
