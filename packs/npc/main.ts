// The Living City pack's main-process part: the city's local AI, and the
// city's saved memories.
//
// The AI is a free, open-source language model (Qwen3.5, Apache 2.0) that
// runs on this computer with llama.cpp (through node-llama-cpp): on the
// graphics chip where it can, or the processor. Nothing is sent anywhere. The
// model is downloaded once, into the pack's data folder, and kept there.
//
// Every answer is held to a JSON schema (protocol.ts) while it's written, so
// the model can only write the shape the game expects. Requests wait in a
// queue, one at a time, with the driver's chats first.
//
// The city (everyone's memories, feelings and life stories) is saved in the
// same data folder, which updates leave alone, so the people remember for as
// long as the game is installed.
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import type { Llama, LlamaContext, LlamaContextSequence, LlamaGrammar, LlamaModel } from 'node-llama-cpp';
import type { PackMainHost } from '../../app/electron/main';
import { CAST } from './cast';
import { defaultModel, MODELS, schemaFor, systemPrompt, type AiStatus, type ModelChoice, type ThinkKind, type ThinkRequest, type ThinkResponse } from './protocol';

/** The library, loaded when first needed (it's large, and only this pack uses it). */
type NodeLlamaCpp = typeof import('node-llama-cpp');

interface AiSettings {
  model: ModelChoice;
  /** Use the graphics chip (faster) or only the processor (leaves the graphics chip to the game). */
  gpu: boolean;
  /** Off: the city uses its built-in brain and the model isn't loaded. */
  enabled: boolean;
}

interface Job {
  request: ThinkRequest;
  resolve: (response: ThinkResponse) => void;
  added: number;
}

/** Most answers needed for each kind of request, in tokens. */
const MAX_TOKENS: Record<ThinkKind, number> = { conversation: 800, chat: 320, reflect: 640 };
/** The driver's chats go first, then conversations, then quiet reflection. */
const PRIORITY: Record<ThinkKind, number> = { chat: 0, conversation: 1, reflect: 2 };
/** Requests waiting longer than this are dropped: the moment has passed. */
const STALE_MS = 45_000;
const MAX_QUEUE = 8;
/** The biggest saved city accepted, in bytes. */
const MAX_CITY = 40 * 1024 * 1024;

export function activate(host: PackMainHost): void {
  const modelsDir = path.join(host.dataDir, 'models');
  const settingsFile = path.join(host.dataDir, 'ai.json');
  const cityFile = path.join(host.dataDir, 'city.json');
  fs.mkdirSync(modelsDir, { recursive: true });

  const settings: AiSettings = { model: defaultModel(os.totalmem()), gpu: true, enabled: true, ...readJson<Partial<AiSettings>>(settingsFile, {}) };
  if (!Object.hasOwn(MODELS, settings.model)) settings.model = defaultModel(os.totalmem());
  const status: AiStatus = { model: settings.model, state: 'missing', progress: 0, downloaded: [], gpu: '', onGpu: true, message: '', queue: 0 };

  let lib: NodeLlamaCpp | null = null;
  let llama: Llama | null = null;
  let model: LlamaModel | null = null;
  let context: LlamaContext | null = null;
  let sequence: LlamaContextSequence | null = null;
  let loading: Promise<void> | null = null;
  let downloading: { model: ModelChoice; cancel: () => Promise<void> } | null = null;
  const grammars = new Map<string, LlamaGrammar>();
  const queue: Job[] = [];
  let working = false;
  const system = systemPrompt();

  const modelPath = (choice: ModelChoice) => path.join(modelsDir, MODELS[choice].file);
  /** A model counts as downloaded once its download finished (a partial file doesn't). */
  const isDownloaded = (choice: ModelChoice) => fs.existsSync(modelPath(choice)) && fs.existsSync(`${modelPath(choice)}.done`);
  const refresh = () => {
    status.model = settings.model;
    status.onGpu = settings.gpu;
    status.downloaded = (Object.keys(MODELS) as ModelChoice[]).filter(isDownloaded);
    status.queue = queue.length;
    if (!settings.enabled) status.state = 'off';
    else if (downloading?.model === settings.model) status.state = 'downloading';
    else if (sequence) status.state = 'ready';
    else if (loading) status.state = 'loading';
    else if (status.state !== 'error') status.state = isDownloaded(settings.model) ? 'loading' : 'missing';
    return status;
  };
  const saveSettings = () => fs.writeFileSync(settingsFile, JSON.stringify(settings));

  const loadLibrary = async (): Promise<NodeLlamaCpp> => {
    lib ??= await import('node-llama-cpp');
    return lib;
  };

  /** Lets go of the loaded model (switching models, or the AI switched off). */
  const unload = async () => {
    const old = [context, model];
    sequence = null;
    context = null;
    model = null;
    grammars.clear();
    for (const thing of old) await thing?.dispose().catch(() => {});
  };

  /** Loads the chosen model, if it's downloaded and the AI is on. */
  const load = (): Promise<void> => {
    if (loading) return loading;
    loading = (async () => {
      try {
        await unload();
        if (!settings.enabled || !isDownloaded(settings.model)) return;
        const l = await loadLibrary();
        // The ready-built engine only (the game can't build llama.cpp itself),
        // using the graphics chip if there's one it can use.
        llama ??= await l.getLlama({ gpu: 'auto', build: 'never', progressLogs: false }).catch(() => l.getLlama({ gpu: false, build: 'never', progressLogs: false }));
        const onGpu = settings.gpu && llama.gpu !== false;
        status.gpu = !onGpu ? 'processor' : llama.gpu === 'metal' ? 'graphics chip (Metal)' : `graphics chip (${llama.gpu})`;
        const choice = settings.model;
        // "Processor" keeps the whole model off the graphics chip, leaving it to the game.
        const loaded = await llama.loadModel({ modelPath: modelPath(choice), gpuLayers: onGpu ? 'auto' : 0 });
        if (choice !== settings.model || !settings.enabled) {
          await loaded.dispose(); // changed while loading
          return;
        }
        model = loaded;
        context = await model.createContext({ contextSize: 4096 });
        sequence = context.getSequence();
        status.message = '';
        status.state = 'ready';
      } catch (error) {
        status.state = 'error';
        status.message = `Couldn't start the AI: ${error instanceof Error ? error.message : String(error)}`.slice(0, 300);
        console.error('Living City:', error);
        await unload();
      } finally {
        loading = null;
        refresh();
        void work();
      }
    })();
    return loading;
  };

  /** Downloads a model (it resumes if it was interrupted), then loads it if it's the chosen one. */
  const download = async (choice: ModelChoice) => {
    if (downloading || isDownloaded(choice)) return;
    const info = MODELS[choice];
    status.progress = 0;
    status.message = '';
    try {
      const l = await loadLibrary();
      const downloader = await l.createModelDownloader({
        modelUri: info.uri,
        dirPath: modelsDir,
        fileName: info.file,
        showCliProgress: false,
        onProgress: ({ downloadedSize, totalSize }) => {
          status.progress = totalSize > 0 ? downloadedSize / totalSize : 0;
        },
      });
      downloading = { model: choice, cancel: () => downloader.cancel() };
      refresh();
      await downloader.download();
      fs.writeFileSync(`${modelPath(choice)}.done`, new Date().toISOString());
      status.progress = 1;
    } catch (error) {
      if (downloading) {
        status.state = 'error';
        status.message = `The download stopped: ${error instanceof Error ? error.message : String(error)}`.slice(0, 300);
        console.error('Living City:', error);
      }
    } finally {
      downloading = null;
      refresh();
    }
    if (choice === settings.model && isDownloaded(choice)) await load();
  };

  /** The grammar that holds an answer to its schema (made once for each shape). */
  const grammarFor = async (kind: ThinkKind, ids: string[]) => {
    const schema = schemaFor(kind, ids);
    const key = JSON.stringify(schema);
    let grammar = grammars.get(key);
    if (!grammar) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      grammar = await llama!.createGrammarForJsonSchema(schema as any);
      if (grammars.size > 60) grammars.clear();
      grammars.set(key, grammar);
    }
    return grammar;
  };

  /** Answers the queued requests, one at a time. */
  const work = async () => {
    if (working) return;
    working = true;
    try {
      while (queue.length && sequence && lib) {
        queue.sort((a, b) => PRIORITY[a.request.kind] - PRIORITY[b.request.kind] || a.added - b.added);
        const job = queue.shift()!;
        status.queue = queue.length;
        if (Date.now() - job.added > STALE_MS) {
          job.resolve({ ok: false, reason: 'busy', message: 'The moment passed.' });
          continue;
        }
        const started = Date.now();
        const session = new lib.LlamaChatSession({
          contextSequence: sequence,
          systemPrompt: system,
          autoDisposeSequence: false,
          // Qwen3.5 can think before it answers; the city wants quick replies instead.
          chatWrapper: model?.fileInfo.metadata.general.architecture?.startsWith('qwen') ? new lib.QwenChatWrapper({ thoughts: 'discourage' }) : 'auto',
        });
        try {
          const grammar = await grammarFor(job.request.kind, job.request.ids);
          const text = await session.prompt(job.request.prompt, {
            grammar,
            maxTokens: MAX_TOKENS[job.request.kind],
            temperature: job.request.kind === 'reflect' ? 0.6 : 0.85,
            topP: 0.9,
            signal: AbortSignal.timeout(90_000),
          });
          job.resolve({ ok: true, data: JSON.parse(text), ms: Date.now() - started });
        } catch (error) {
          job.resolve({ ok: false, reason: 'error', message: error instanceof Error ? error.message.slice(0, 200) : 'The AI failed.' });
        } finally {
          session.dispose({ disposeSequence: false });
          await sequence?.clearHistory();
        }
      }
    } finally {
      working = false;
      status.queue = queue.length;
    }
  };

  host.handle('status', () => refresh());

  host.handle('configure', async (raw: unknown) => {
    const change = (raw && typeof raw === 'object' ? raw : {}) as Partial<AiSettings>;
    const before = { ...settings };
    if (typeof change.model === 'string' && Object.hasOwn(MODELS, change.model)) settings.model = change.model;
    if (typeof change.gpu === 'boolean') settings.gpu = change.gpu;
    if (typeof change.enabled === 'boolean') settings.enabled = change.enabled;
    saveSettings();
    status.state = 'missing';
    status.message = '';
    if (settings.model !== before.model || settings.gpu !== before.gpu || settings.enabled !== before.enabled) {
      if (!settings.enabled) await unload();
      else void load();
    }
    return refresh();
  });

  host.handle('download', (raw: unknown) => {
    const choice = typeof raw === 'string' && Object.hasOwn(MODELS, raw) ? (raw as ModelChoice) : settings.model;
    void download(choice);
    return refresh();
  });

  host.handle('cancelDownload', async () => {
    const d = downloading;
    downloading = null;
    await d?.cancel().catch(() => {});
    return refresh();
  });

  host.handle('removeModel', async (raw: unknown) => {
    if (typeof raw !== 'string' || !Object.hasOwn(MODELS, raw)) return refresh();
    const choice = raw as ModelChoice;
    if (choice === settings.model) await unload();
    fs.rmSync(`${modelPath(choice)}.done`, { force: true });
    fs.rmSync(modelPath(choice), { force: true });
    status.state = 'missing';
    return refresh();
  });

  host.handle('think', (raw: unknown): Promise<ThinkResponse> | ThinkResponse => {
    const request = checkRequest(raw);
    if (!request) return { ok: false, reason: 'error', message: 'Bad request.' };
    if (!sequence) return { ok: false, reason: 'not-ready', message: 'The AI is not ready.' };
    // A full queue makes room for a more urgent request by dropping the least urgent, newest one.
    if (queue.length >= MAX_QUEUE) {
      const worst = [...queue].sort((a, b) => PRIORITY[b.request.kind] - PRIORITY[a.request.kind] || b.added - a.added)[0];
      if (PRIORITY[worst.request.kind] <= PRIORITY[request.kind]) return { ok: false, reason: 'busy', message: 'Thinking about other things.' };
      queue.splice(queue.indexOf(worst), 1);
      worst.resolve({ ok: false, reason: 'busy', message: 'Thinking about other things.' });
    }
    return new Promise((resolve) => {
      queue.push({ request, resolve, added: Date.now() });
      status.queue = queue.length;
      void work();
    });
  });

  // ---------- the city's memories ----------

  host.handle('loadCity', () => {
    for (const file of [cityFile, `${cityFile}.backup`]) {
      const city = readJson<unknown>(file, null);
      if (city && typeof city === 'object') return city;
    }
    return null;
  });

  let lastBackup = 0;
  host.handle('saveCity', (raw: unknown) => {
    if (!raw || typeof raw !== 'object') return false;
    const text = JSON.stringify(raw);
    if (text.length > MAX_CITY) return false;
    // Keep the previous save as a backup now and then, in case a save is ever cut short.
    if (Date.now() - lastBackup > 10 * 60_000 && fs.existsSync(cityFile)) {
      fs.copyFileSync(cityFile, `${cityFile}.backup`);
      lastBackup = Date.now();
    }
    // Written to a new file then swapped in, so a crash mid-save can't lose the city.
    fs.writeFileSync(`${cityFile}.tmp`, text);
    fs.renameSync(`${cityFile}.tmp`, cityFile);
    return true;
  });

  // Start the AI: load the model if it's here.
  refresh();
  if (settings.enabled && isDownloaded(settings.model)) void load();
}

function readJson<T>(file: string, fallback: T): T {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8')) as T;
  } catch {
    return fallback;
  }
}

/** Only well-formed requests about real cast members get through. */
function checkRequest(raw: unknown): ThinkRequest | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Partial<ThinkRequest>;
  const kinds: ThinkKind[] = ['conversation', 'chat', 'reflect'];
  if (!kinds.includes(r.kind as ThinkKind) || !Array.isArray(r.ids) || typeof r.prompt !== 'string') return null;
  if (r.ids.length < 1 || r.ids.length > 4 || !r.ids.every((id) => typeof id === 'string' && CAST.some((c) => c.id === id))) return null;
  if (r.prompt.length > 12_000) return null;
  return { kind: r.kind as ThinkKind, ids: r.ids, prompt: r.prompt };
}
