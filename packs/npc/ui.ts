// The Living City's windows: the chat box (talking to someone), the city
// journal (everyone's lives, your friends, and what the city has been
// saying), and the pack's settings in the Packs tab.
import type { GameApi, PanelUI } from '../../game/packs';
import { button, el } from '../../libs/dom';
import { castMember, TRAIT_NAMES, type Emotion } from './cast';
import type { Npc } from './index';
import { feelingWord, friendship, isFriend, moodWord, traitWords, type Memory, type Mind } from './mind';
import { MODELS, type AiStatus, type ModelChoice } from './protocol';

export interface Options {
  /** Which conversations between people the AI writes: the ones near you, or everyone's (as fast as it can). */
  think: 'nearby' | 'everywhere';
  tags: boolean;
  /** Whether BUDDY may chase you in his police car (otherwise he only writes tickets). */
  chases: boolean;
}

export const DEFAULT_OPTIONS: Options = { think: 'everywhere', tags: true, chases: true };

/** A text message someone sent you. */
export interface Message {
  from: string;
  text: string;
  t: number;
}

/** A conversation the city overheard, for the journal. */
export interface Talk {
  t: number;
  where: string;
  lines: { name: string; text: string }[];
  ai: boolean;
}

const FACES: Record<Emotion, string> = {
  happy: '😊', excited: '🤩', calm: '🙂', curious: '🤔', amused: '😄', worried: '😟', scared: '😱', annoyed: '😠', sad: '😢', proud: '😎',
};

/** How they greet the driver when a chat starts, from how well they know them. */
export function greeting(mind: Mind): string {
  const name = mind.driverName;
  switch (friendship(mind)) {
    case 'enemy':
      return 'Oh. You. What do you want?';
    case 'wary':
      return name ? `${name}. Careful where you park that thing.` : 'Can I help you?';
    case 'stranger':
      return 'Oh, hello there! Do I know you?';
    case 'acquaintance':
      return name ? `Oh hey, ${name}!` : 'Oh hey, the driver again!';
    case 'friend':
      return `${name ?? 'Hey you'}! Good to see you.`;
    case 'close friend':
      return `${name ?? 'There you are'}! I was hoping you'd stop by.`;
    case 'best friend':
      return `${name ?? 'Hey'}! My favorite person in this whole city!`;
  }
}

// ---------- the chat box ----------

export interface ChatBox {
  open: boolean;
  npc: Npc | null;
  show(npc: Npc): void;
  hide(): void;
  focus(): void;
  dispose(): void;
}

export function buildChat(
  game: GameApi,
  hooks: {
    send: (npc: Npc, said: string) => Promise<string>;
    describe: (npc: Npc) => string;
    close: (npc: Npc) => void;
  },
): ChatBox {
  const box = el('div', 'npc-chat hidden');
  const title = el('div', 'npc-chat-title');
  const name = el('b');
  const about = el('span');
  title.append(name, about);
  const log = el('div', 'npc-chat-log');
  const form = el('form', 'npc-chat-form');
  const input = el('input', 'text npc-chat-input');
  input.type = 'text';
  input.maxLength = 300;
  input.placeholder = 'Say something…';
  const say = el('button', 'btn primary', 'Say');
  say.type = 'submit';
  form.append(input, say);
  const hint = el('div', 'npc-chat-hint', 'Enter to say it · Esc to drive on');
  const leave = button('×', 'npc-close', () => chat.hide());
  box.append(leave, title, log, form, hint);
  game.overlay.append(box);

  const line = (who: string, text: string, kind: '' | 'mine' | 'old') => {
    const row = el('div', `npc-chat-line ${kind}`);
    row.append(el('b', '', who), document.createTextNode(` ${text}`));
    log.append(row);
    log.scrollTop = log.scrollHeight;
    return row;
  };

  let waiting = false;
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    const npc = chat.npc;
    const said = input.value.trim();
    if (!npc || !said || waiting) return;
    input.value = '';
    line('You:', said, 'mine');
    const reply = line(`${npc.member.name}:`, '…', '');
    // Read-only rather than disabled while they think: a disabled box loses
    // the keyboard, and then Esc would reach the game instead of the chat.
    waiting = true;
    input.readOnly = true;
    void hooks
      .send(npc, said)
      .then((text) => {
        reply.lastChild!.textContent = ` ${text}`;
        if (chat.npc === npc) about.textContent = hooks.describe(npc);
      })
      .finally(() => {
        waiting = false;
        input.readOnly = false;
        if (chat.open) input.focus();
      });
  });
  // Esc leaves the chat (and doesn't open the game's menu), wherever in the box the keyboard is.
  box.addEventListener('keydown', (e) => {
    if (e.code === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      chat.hide();
    }
  });

  const chat: ChatBox = {
    open: false,
    npc: null,
    show(npc) {
      if (chat.npc && chat.npc !== npc) hooks.close(chat.npc);
      chat.npc = npc;
      chat.open = true;
      name.textContent = npc.member.name;
      about.textContent = hooks.describe(npc);
      log.replaceChildren();
      // The last few things you said to each other, so you can pick up where you left off.
      for (const c of npc.mind.chats.slice(-3)) {
        line('You:', c.you, 'old');
        line(`${npc.member.name}:`, c.them, 'old');
      }
      line(`${npc.member.name}:`, greeting(npc.mind), '');
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

export interface Journal {
  open: boolean;
  toggle(): void;
  show(page?: string): void;
  hide(): void;
  tick(): void;
  dispose(): void;
}

export function buildJournal(
  game: GameApi,
  hooks: {
    npcs: () => Npc[];
    now: () => number;
    time: () => string;
    doing: (npc: Npc) => string;
    placeName: (npc: Npc) => string;
    footer: () => string;
    news: () => string[];
    talks: () => Talk[];
    plans: (id?: string) => string[];
    /** Your record with the police, in a few words ("🚓 3 tickets · 1 chase"). */
    record: () => string;
    messages: () => Message[];
    find: (npc: Npc) => void;
  },
): Journal {
  const panel = el('div', 'npc-journal hidden');
  const head = el('div', 'npc-journal-head');
  const heading = el('b', '', '📖 City journal');
  head.append(heading, button('×', 'npc-close', () => journal.hide()));
  const list = el('div', 'npc-journal-list');
  const page = el('div', 'npc-journal-page');
  const footer = el('div', 'npc-journal-foot');
  const body = el('div', 'npc-journal-body');
  body.append(list, page);
  panel.append(head, body, footer);
  game.overlay.append(panel);
  /** The page showing: 'city', 'you', or someone's id. */
  let selected = 'city';
  let timer = 0;

  const section = (title: string, ...content: HTMLElement[]) => {
    const s = el('section');
    s.append(el('h4', '', title), ...content);
    page.append(s);
  };
  const ago = (t: number) => {
    const minutes = Math.round(Math.max(0, hooks.now() - t) / 60);
    // A city hour is a minute (see index.ts), so these are city hours and days.
    return minutes < 1 ? 'just now' : minutes < 24 ? `${minutes} h ago` : `${Math.round(minutes / 24)} d ago`;
  };
  const bullets = (items: string[], empty: string, className = '') => {
    const ul = el('ul', className);
    for (const item of items) ul.append(el('li', '', item));
    if (!items.length) ul.append(el('li', 'npc-small', empty));
    return ul;
  };
  const go = (id: string) => {
    selected = id;
    draw();
  };

  /** The city: what's been happening, who's been saying what, and the plans people have made. */
  const cityPage = () => {
    const title = el('div', 'npc-journal-title');
    title.append(el('b', '', 'City life'), el('span', '', ` ${hooks.time()}`));
    page.append(title);
    section('Lately', bullets(hooks.news().slice(0, 6), 'A quiet day so far.'));
    const talks = el('div', 'npc-talks');
    for (const talk of hooks.talks().slice(0, 12)) {
      const t = el('div', 'npc-talk');
      t.append(el('small', '', `${talk.where} · ${ago(talk.t)}${talk.ai ? '' : ' · small talk'}`));
      for (const l of talk.lines) {
        const row = el('div');
        row.append(el('b', '', `${l.name}: `), document.createTextNode(l.text));
        t.append(row);
      }
      talks.append(t);
    }
    if (!talks.children.length) talks.append(el('p', 'npc-small', 'Nobody has stopped to chat yet.'));
    section('Overheard', talks);
    section('Plans', bullets(hooks.plans(), 'No plans made yet.'));
  };

  /** You: who counts you as a friend, and your record with the police. */
  const youPage = () => {
    const title = el('div', 'npc-journal-title');
    title.append(el('b', '', 'You'), el('span', '', ` ${hooks.record()}`));
    page.append(title);
    const messages = el('ul', 'npc-messages');
    for (const m of hooks.messages().slice(0, 8)) {
      const li = el('li');
      li.append(el('b', '', `📱 ${m.from}: `), document.createTextNode(m.text), el('small', '', ` ${ago(m.t)}`));
      messages.append(li);
    }
    if (!messages.children.length) messages.append(el('li', 'npc-small', 'No messages yet. Friends text you now and then.'));
    section('Messages', messages);
    const npcs = hooks.npcs();
    const known = npcs.filter((n) => (n.mind.met.driver ?? 0) > 0 || Math.abs(n.mind.feelings.driver ?? 0) > 0.15);
    known.sort((a, b) => (b.mind.feelings.driver ?? 0) - (a.mind.feelings.driver ?? 0));
    const friends = el('div', 'npc-friends');
    for (const npc of known) {
      const row = button(`${isFriend(npc.mind) ? '💙' : FACES[npc.mind.emotion]} ${npc.member.name}`, 'npc-person', () => go(npc.member.id));
      row.append(el('span', 'npc-small', ` ${friendship(npc.mind)} · ${npc.mind.met.driver ?? 0} talk${npc.mind.met.driver === 1 ? '' : 's'}`));
      friends.append(row);
    }
    if (!known.length) friends.append(el('p', 'npc-small', 'Nobody knows you yet. Click on someone (or pull up next to them and press K) to say hello.'));
    section(`People you know (${npcs.filter((n) => isFriend(n.mind)).length} friends)`, friends);
    page.append(el('p', 'npc-small', 'Friendships grow as you talk to people and treat them well, and they remember you for as long as the game is installed.'));
  };

  const personPage = (npc: Npc) => {
    const mind = npc.mind;
    const title = el('div', 'npc-journal-title');
    title.append(el('b', '', npc.member.name), el('span', '', ` ${npc.member.role}`), button('Find', 'btn npc-find', () => hooks.find(npc)));
    page.append(title);
    page.append(el('p', 'npc-bio', npc.member.bio));
    page.append(el('p', '', `Now: ${hooks.doing(npc)}, near ${hooks.placeName(npc)}. Feeling ${moodWord(mind)}${mind.emotion !== 'calm' ? `, ${mind.emotion}` : ''}.`));
    if (mind.goal) page.append(el('p', 'npc-goal', `🎯 ${mind.goal}`));
    // Standing decisions they've made for themselves (kept until they change their mind).
    if (mind.resolutions.length) {
      const made = el('ul');
      for (const r of mind.resolutions) made.append(el('li', 'npc-goal', `🤝 “${r}”`));
      section('Has made up their mind', made);
    }
    if (mind.story) section('Their story so far', el('p', 'npc-story', mind.story));
    if (mind.plan.length || mind.thoughts.length) {
      const today = el('ul');
      for (const step of mind.plan) {
        const where = step.place === 'home' ? 'home' : step.place;
        today.append(el('li', 'npc-small', `${String(step.hour).padStart(2, '0')}:00 · ${step.what} (${where})`));
      }
      for (const t of [...mind.thoughts].reverse().slice(0, 4)) today.append(el('li', 'npc-thought', `💭 ${t}`));
      section('Today, and on their mind', today);
    }

    const you = el('ul');
    you.append(el('li', '', `${friendship(mind)}: ${feelingWord(mind.feelings.driver ?? 0)} you${mind.driverName ? ` (knows you as ${mind.driverName})` : ''}`));
    for (const c of mind.chats.slice(-3).reverse()) {
      const li = el('li', 'npc-small');
      li.textContent = `You: “${c.you}” · ${npc.member.name}: “${c.them}”`;
      you.append(li);
    }
    section('You and them', you);

    const traits = el('div', 'npc-traits');
    for (const t of TRAIT_NAMES) {
      const row = el('div', 'npc-trait');
      const bar = el('i');
      bar.style.width = `${Math.round(mind.traits[t] * 100)}%`;
      const drift = mind.traits[t] - npc.member.traits[t];
      row.append(el('span', '', t), el('div', 'npc-bar'), el('em', '', Math.abs(drift) >= 0.02 ? (drift > 0 ? '▲' : '▼') : ''));
      row.children[1].append(bar);
      traits.append(row);
    }
    section('Personality', el('p', 'npc-small', traitWords(mind.traits)), traits);

    const people = Object.entries(mind.feelings)
      .filter(([id]) => id !== 'driver' && castMember(id))
      .sort((a, b) => b[1] - a[1]);
    const friends = people.filter(([, v]) => v > 0.55).map(([id]) => castMember(id)!.name);
    const rivals = people.filter(([, v]) => v < -0.3).map(([id]) => castMember(id)!.name);
    const circle = el('ul');
    circle.append(el('li', '', friends.length ? `Friends: ${friends.slice(0, 8).join(', ')}` : 'Still making friends.'));
    if (rivals.length) circle.append(el('li', '', `Doesn't get on with: ${rivals.slice(0, 5).join(', ')}`));
    for (const plan of hooks.plans(npc.member.id)) circle.append(el('li', 'npc-small', plan));
    section('Friends and plans', circle);

    if (mind.insights.length || mind.habits.length) {
      const beliefs = el('ul');
      for (const i of [...mind.insights].reverse()) beliefs.append(el('li', '', `“${i}”`));
      for (const h of mind.habits) beliefs.append(el('li', 'npc-small', `Habit: ${h}`));
      section('Has come to believe', beliefs);
    }

    const memories = el('ul', 'npc-memories');
    for (const m of mind.memories.slice(-10).reverse() as Memory[]) {
      const li = el('li', m.importance >= 7 ? 'big' : '');
      li.append(document.createTextNode(m.text), el('small', '', ` ${ago(m.t)}`));
      memories.append(li);
    }
    if (!mind.memories.length) memories.append(el('li', 'npc-small', 'Nothing much yet.'));
    section(`Remembers (${mind.memories.length})`, memories);
  };

  const draw = () => {
    const npcs = hooks.npcs();
    const scroll = list.scrollTop;
    list.replaceChildren(
      button('🗞 City life', selected === 'city' ? 'npc-person on' : 'npc-person', () => go('city')),
      button('⭐ You', selected === 'you' ? 'npc-person on' : 'npc-person', () => go('you')),
      ...npcs.map((npc) =>
        button(
          `${npc.asleep ? '💤' : FACES[npc.mind.emotion]} ${npc.member.name}${isFriend(npc.mind) ? ' 💙' : ''}`,
          npc.member.id === selected ? 'npc-person on' : 'npc-person',
          () => go(npc.member.id),
        ),
      ),
    );
    list.scrollTop = scroll;
    const pageScroll = page.scrollTop;
    page.replaceChildren();
    const npc = npcs.find((n) => n.member.id === selected);
    if (selected === 'you') youPage();
    else if (npc) personPage(npc);
    else cityPage();
    page.scrollTop = pageScroll;
    footer.textContent = hooks.footer();
  };

  const journal: Journal = {
    open: false,
    toggle() {
      if (journal.open) journal.hide();
      else journal.show();
    },
    show(id) {
      if (id) selected = id;
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
      if (timer % 90 === 0) draw();
    },
    dispose() {
      panel.remove();
    },
  };
  return journal;
}

// ---------- the Packs tab ----------

/** The local AI's state in words, for the settings and the journal. */
export function aiWords(status: AiStatus | null): string {
  if (!status) return 'The local AI runs in the desktop game; here the city uses its built-in brain.';
  const model = MODELS[status.model].label;
  switch (status.state) {
    case 'off':
      return 'Local AI off: the city uses its built-in brain.';
    case 'missing':
      return `${model} isn't downloaded yet: the city uses its built-in brain until it is.`;
    case 'downloading':
      return `Downloading ${model}: ${Math.round(status.progress * 100)}% of ${MODELS[status.model].size} GB. The city uses its built-in brain until it's done.`;
    case 'loading':
      return `Starting ${model}…`;
    case 'ready':
      return `Thinking with ${model} on this computer's ${status.gpu}${status.queue ? ` (${status.queue} thought${status.queue === 1 ? '' : 's'} waiting)` : ''}.`;
    case 'error':
      return status.message || 'The local AI hit a problem.';
  }
}

export function settingsPanel(
  panel: PanelUI,
  s: {
    options: Options;
    save: () => void;
    status: () => AiStatus | null;
    configure: (change: { model?: ModelChoice; gpu?: boolean; enabled?: boolean }) => Promise<void>;
    download: (choice: ModelChoice) => void;
    cancel: () => void;
    remove: (choice: ModelChoice) => void;
    resetCity: () => void;
  },
): (status: AiStatus | null) => void {
  const { options } = s;
  const status = s.status();
  panel.heading('Local AI');
  panel.hint(
    'The people are brought to life by a free, open-source AI model (Qwen3.5, Apache 2.0) that runs on this computer: nothing is sent anywhere and nothing costs money. ' +
      'It is downloaded once and kept with the game.',
  );
  const line = el('div', 'npc-ai-status');
  const bar = el('progress', 'npc-progress');
  bar.max = 1;
  panel.append(line, bar);
  /** What the tab was drawn for: it's drawn again when that changes (a download finishing, say). */
  const drawnFor = (st: AiStatus | null) => (st ? `${st.state}|${st.model}|${st.downloaded.join(',')}` : 'none');
  const shown = drawnFor(status);
  const update = (st: AiStatus | null) => {
    line.textContent = aiWords(st);
    bar.value = st?.progress ?? 0;
    bar.style.display = st?.state === 'downloading' ? '' : 'none';
    if (line.isConnected && drawnFor(st) !== shown) panel.refresh();
  };
  update(status);
  if (!status) return update;

  panel.choice(
    'Model',
    (Object.keys(MODELS) as ModelChoice[]).map((m) => ({ value: m, label: `${MODELS[m].label} · ${MODELS[m].size} GB` })),
    () => status.model,
    (v) => void s.configure({ model: v }).then(() => panel.refresh()),
  );
  panel.hint(`${MODELS[status.model].about} It needs about ${MODELS[status.model].memory} GB of memory in the computer.`);
  const model = status.model;
  const have = status.downloaded.includes(model);
  const buttons: HTMLElement[] = [];
  if (status.state === 'downloading') buttons.push(button('Cancel download', 'btn', () => s.cancel()));
  else if (!have) buttons.push(button(`Download (${MODELS[model].size} GB)`, 'btn primary', () => s.download(model)));
  for (const other of status.downloaded) {
    let sure = false;
    const remove = button(`Delete ${MODELS[other].label.split(' (')[0]}`, 'btn', () => {
      if (!sure) {
        sure = true;
        remove.textContent = 'Click again to delete it';
        return;
      }
      s.remove(other);
    });
    buttons.push(remove);
  }
  if (buttons.length) panel.row('Download', ...buttons);
  panel.choice(
    'Runs on',
    [
      { value: 'gpu', label: 'Graphics chip' },
      { value: 'cpu', label: 'Processor' },
    ],
    () => (status.onGpu ? 'gpu' : 'cpu'),
    (v) => void s.configure({ gpu: v === 'gpu' }).then(() => panel.refresh()),
  );
  panel.choice(
    'AI',
    [
      { value: 'on', label: 'On' },
      { value: 'off', label: 'Off' },
    ],
    () => (status.state === 'off' ? 'off' : 'on'),
    (v) => void s.configure({ enabled: v === 'on' }).then(() => panel.refresh()),
  );
  panel.choice(
    'Conversations',
    [
      { value: 'everywhere', label: 'Everyone' },
      { value: 'nearby', label: 'Near you' },
    ],
    () => options.think,
    (v) => ((options.think = v), s.save()),
  );
  panel.hint(
    'Everyone: the AI writes conversations all over the city, as fast as it can (near you first). Near you: only the ones you could overhear, which leaves more of the computer to the game. ' +
      'The graphics chip is much faster; the processor leaves the graphics chip to the game.',
  );

  panel.heading('City');
  panel.choice(
    'Name tags',
    [
      { value: 'on', label: 'On' },
      { value: 'off', label: 'Off' },
    ],
    () => (options.tags ? 'on' : 'off'),
    (v) => ((options.tags = v === 'on'), s.save()),
  );
  panel.choice(
    'Police chases',
    [
      { value: 'on', label: 'On' },
      { value: 'off', label: 'Off' },
    ],
    () => (options.chases ? 'on' : 'off'),
    (v) => ((options.chases = v === 'on'), s.save()),
  );
  panel.hint('On: BUDDY decides for himself when you have gone too far, and chases you in his police car (siren, ramming, roof gun). Lose him to escape, or stop and take the fine. Off: he only writes tickets.');
  let sure = false;
  const reset = button('Reset everyone', 'btn danger', () => {
    if (!sure) {
      sure = true;
      reset.textContent = 'Click again: they forget everything, you included';
      return;
    }
    s.resetCity();
    reset.textContent = 'Done: a fresh start';
    reset.disabled = true;
  });
  panel.row('Memories', reset);
  panel.hint(
    "Everyone's memories, friendships and life stories are saved in the game's data folder, which updates leave alone: they last for as long as the game is installed. " +
      'Click on someone (or pull up next to them and press K) to talk. L opens the city journal.',
  );
  return update;
}
