// The garage panel: every customization option grouped into tabs, plus a
// Settings tab for graphics quality and game options.
import {
  BODY_OPTIONS,
  DEFAULT_CONFIG,
  EXHAUST_OPTIONS,
  FINISH_OPTIONS,
  LIGHT_SWATCHES,
  PAINT_SWATCHES,
  PART_COLOR_OPTIONS,
  SPOILER_OPTIONS,
  STRIPE_OPTIONS,
  TUNING_LABELS,
  WHEEL_OPTIONS,
  cleanPlate,
  cloneConfig,
  randomConfig,
  type CarConfig,
  type Option,
  type Tuning,
} from './carConfig';
import { PRESETS, type Settings } from './settings';

type Change = 'look' | 'tuning';

const TABS = ['Body', 'Paint', 'Wheels', 'Parts', 'Lights', 'Tuning', 'Settings'] as const;
export type Tab = (typeof TABS)[number];

const ON_OFF: Option<'on' | 'off'>[] = [
  { value: 'on', label: 'On' },
  { value: 'off', label: 'Off' },
];

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

/** What the garage tells the game about. */
export interface GarageEvents {
  carChanged(change: Change): void;
  settingsChanged(): void;
  rebuildCity(): void;
  drive(): void;
}

export class GarageUI {
  readonly root: HTMLDivElement;
  private body: HTMLDivElement;
  private tabButtons = new Map<Tab, HTMLButtonElement>();
  private tab: Tab = 'Body';

  constructor(
    private config: CarConfig,
    private settings: Settings,
    private events: GarageEvents,
  ) {
    this.root = el('div', 'garage');
    const head = el('header');
    head.append(el('h1', undefined, 'Garage'), el('p', undefined, 'Build your car, then hit Drive.'));
    const nav = el('nav', 'tabs');
    for (const t of TABS) {
      const b = el('button', 'tab', t);
      b.type = 'button';
      b.addEventListener('click', () => this.show(t));
      this.tabButtons.set(t, b);
      nav.append(b);
    }
    this.body = el('div', 'garage-body');
    const foot = el('footer');
    const random = el('button', 'btn', 'Randomize');
    random.addEventListener('click', () => this.replace(randomConfig(this.config)));
    const reset = el('button', 'btn', 'Reset');
    reset.addEventListener('click', () => this.replace(cloneConfig(DEFAULT_CONFIG)));
    const drive = el('button', 'btn primary', 'Drive ▶');
    drive.addEventListener('click', () => this.events.drive());
    foot.append(random, reset, drive);
    this.root.append(head, nav, this.body, foot);
    this.show('Body');
  }

  setVisible(visible: boolean): void {
    this.root.classList.toggle('hidden', !visible);
  }

  private replace(next: CarConfig): void {
    Object.assign(this.config, next, { tuning: { ...next.tuning } });
    this.events.carChanged('look');
    this.events.carChanged('tuning');
    this.show(this.tab);
  }

  show(tab: Tab): void {
    this.tab = tab;
    for (const [t, b] of this.tabButtons) b.classList.toggle('active', t === tab);
    this.body.replaceChildren();
    const c = this.config;
    const look = () => this.events.carChanged('look');
    switch (tab) {
      case 'Body':
        this.choice('Body style', BODY_OPTIONS, () => c.body, (v) => ((c.body = v), look()));
        this.slider('Ride height', -1, 1, 0.1, () => c.rideHeight, (v) => ((c.rideHeight = v), look()), (v) => (v === 0 ? 'Stock' : v < 0 ? 'Lowered' : 'Raised'));
        this.text('Number plate', () => c.plate, (v) => ((c.plate = v), look()));
        break;
      case 'Paint':
        this.color('Paint colour', PAINT_SWATCHES, () => c.paintColor, (v) => ((c.paintColor = v), look()));
        this.choice('Finish', FINISH_OPTIONS, () => c.finish, (v) => ((c.finish = v), look()));
        this.choice('Stripes', STRIPE_OPTIONS, () => c.stripes, (v) => ((c.stripes = v), look()));
        this.color('Accent colour (stripes, parts)', PAINT_SWATCHES, () => c.accentColor, (v) => ((c.accentColor = v), look()));
        this.slider('Window tint', 0, 1, 0.05, () => c.windowTint, (v) => ((c.windowTint = v), look()), (v) => `${Math.round(v * 100)}%`);
        break;
      case 'Wheels':
        this.choice('Rims', WHEEL_OPTIONS, () => c.wheelStyle, (v) => ((c.wheelStyle = v), look()));
        this.color('Rim colour', ['#c9ccd1', '#1a1a1a', '#ffffff', '#d4af37', '#b87333', '#d7263d', '#1b98e0', '#3bb273'], () => c.rimColor, (v) => ((c.rimColor = v), look()));
        this.slider('Wheel size', 0.85, 1.25, 0.05, () => c.wheelSize, (v) => ((c.wheelSize = v), look()), (v) => `${Math.round(v * 100)}%`);
        this.slider('Tyre width', 0.8, 1.4, 0.05, () => c.tireWidth, (v) => ((c.tireWidth = v), look()), (v) => `${Math.round(v * 100)}%`);
        break;
      case 'Parts':
        this.choice('Spoiler', SPOILER_OPTIONS, () => c.spoiler, (v) => ((c.spoiler = v), look()));
        this.choice('Spoiler colour', PART_COLOR_OPTIONS, () => c.spoilerColor, (v) => ((c.spoilerColor = v), look()));
        this.choice('Exhaust', EXHAUST_OPTIONS, () => c.exhaust, (v) => ((c.exhaust = v), look()));
        this.choice('Roof weapons', ON_OFF, () => (c.roofGuns ? 'on' : 'off'), (v) => ((c.roofGuns = v === 'on'), look()));
        break;
      case 'Lights':
        this.color('Headlight colour', LIGHT_SWATCHES, () => c.headlightColor, (v) => ((c.headlightColor = v), look()));
        this.choice('Underglow', ON_OFF, () => (c.underglow ? 'on' : 'off'), (v) => ((c.underglow = v === 'on'), look()));
        this.color('Underglow colour', ['#7af0ff', '#c58bff', '#ff4fd8', '#39ff88', '#ff3b3b', '#ffd23f', '#2f6bff', '#ffffff'], () => c.underglowColor, (v) => ((c.underglowColor = v), look()));
        this.color('Nitro flame colour', ['#4fb3ff', '#ff7a1a', '#c58bff', '#39ff88', '#ff3bd0', '#ffffff'], () => c.nitroColor, (v) => ((c.nitroColor = v), look()));
        this.hint('Press N while driving to switch between day and night.');
        break;
      case 'Tuning':
        for (const key of Object.keys(TUNING_LABELS) as (keyof Tuning)[]) {
          this.slider(TUNING_LABELS[key], 1, 10, 1, () => c.tuning[key], (v) => ((c.tuning[key] = v), this.events.carChanged('tuning')), (v) => `${v}/10`);
        }
        this.hint('Drift sets how far the back steps out when you pull the handbrake (Space). Nitro sets how hard Shift pushes you and how long the bottle lasts.');
        break;
      case 'Settings':
        this.showSettings();
        break;
    }
  }

  private showSettings(): void {
    const s = this.settings;
    const changed = () => this.events.settingsChanged();
    const onOff = (on: boolean) => (on ? 'on' : 'off');

    // Preset buttons set several settings at once, then redraw the tab to show them.
    const presets = el('div', 'segmented');
    for (const [name, values] of Object.entries(PRESETS)) {
      const b = el('button', undefined, name);
      b.type = 'button';
      b.addEventListener('click', () => {
        Object.assign(s, values);
        changed();
        this.show('Settings');
      });
      presets.append(b);
    }
    this.row('Quality preset').append(presets);
    this.slider('Resolution', 0.5, 1, 0.05, () => s.resolution, (v) => ((s.resolution = v), changed()), (v) => `${Math.round(v * 100)}%`);
    this.choice(
      'Shadows',
      [
        { value: 'off', label: 'Off' },
        { value: 'low', label: 'Low' },
        { value: 'high', label: 'High' },
        { value: 'ultra', label: 'Ultra' },
      ],
      () => s.shadows,
      (v) => ((s.shadows = v), changed()),
    );
    this.slider('View distance', 150, 600, 10, () => s.viewDistance, (v) => ((s.viewDistance = v), changed()), (v) => `${v} m`);
    this.choice(
      'Explosion effects',
      [
        { value: 'low', label: 'Low' },
        { value: 'medium', label: 'Medium' },
        { value: 'high', label: 'High' },
      ],
      () => s.effects,
      (v) => ((s.effects = v), changed()),
    );
    this.choice(
      'Frame rate limit',
      [
        { value: '0', label: 'None' },
        { value: '30', label: '30' },
        { value: '60', label: '60' },
        { value: '120', label: '120' },
      ],
      () => String(s.fpsLimit),
      (v) => ((s.fpsLimit = Number(v)), changed()),
    );
    this.choice('Anti-aliasing (after restart)', ON_OFF, () => onOff(s.antialias), (v) => ((s.antialias = v === 'on'), changed()));
    this.choice('FPS counter', ON_OFF, () => onOff(s.showFps), (v) => ((s.showFps = v === 'on'), changed()));
    this.slider('Field of view', 50, 90, 1, () => s.fov, (v) => ((s.fov = v), changed()), (v) => `${v}°`);
    this.choice('Weapons', ON_OFF, () => onOff(s.weapons), (v) => ((s.weapons = v === 'on'), changed()));
    this.choice('Minimap', ON_OFF, () => onOff(s.minimap), (v) => ((s.minimap = v === 'on'), changed()));

    const rebuild = el('button', 'btn', 'Rebuild the city');
    rebuild.type = 'button';
    rebuild.addEventListener('click', () => this.events.rebuildCity());
    this.row('Destroyed buildings').append(rebuild);
    this.hint('If the game feels slow, try the Low or Medium preset and turn on the FPS counter to see the difference.');
  }

  private row(label: string): HTMLDivElement {
    const row = el('div', 'row');
    row.append(el('label', undefined, label));
    this.body.append(row);
    return row;
  }

  private hint(text: string): void {
    this.body.append(el('p', 'hint', text));
  }

  private choice<T extends string>(label: string, options: Option<T>[], get: () => T, set: (v: T) => void): void {
    const row = this.row(label);
    const group = el('div', 'segmented');
    const buttons: [T, HTMLButtonElement][] = [];
    for (const o of options) {
      const b = el('button', undefined, o.label);
      b.type = 'button';
      b.addEventListener('click', () => {
        set(o.value);
        for (const [v, btn] of buttons) btn.classList.toggle('active', v === get());
      });
      b.classList.toggle('active', o.value === get());
      buttons.push([o.value, b]);
      group.append(b);
    }
    row.append(group);
  }

  private color(label: string, swatches: string[], get: () => string, set: (v: string) => void): void {
    const row = this.row(label);
    const wrap = el('div', 'swatches');
    const picker = el('input');
    picker.type = 'color';
    picker.value = get();
    picker.title = 'Pick any colour';
    const sync = () => {
      for (const b of wrap.querySelectorAll<HTMLButtonElement>('button')) b.classList.toggle('active', b.dataset.color === get());
      picker.value = get();
    };
    for (const s of swatches) {
      const b = el('button', 'swatch');
      b.type = 'button';
      b.style.background = s;
      b.dataset.color = s;
      b.title = s;
      b.addEventListener('click', () => {
        set(s);
        sync();
      });
      wrap.append(b);
    }
    picker.addEventListener('input', () => {
      set(picker.value.toLowerCase());
      sync();
    });
    wrap.append(picker);
    row.append(wrap);
    sync();
  }

  private slider(label: string, min: number, max: number, step: number, get: () => number, set: (v: number) => void, fmt: (v: number) => string): void {
    const row = this.row(label);
    const wrap = el('div', 'slider');
    const input = el('input');
    input.type = 'range';
    input.min = String(min);
    input.max = String(max);
    input.step = String(step);
    input.value = String(get());
    const out = el('span', 'value', fmt(get()));
    input.addEventListener('input', () => {
      const v = Math.round(Number(input.value) / step) * step;
      const clean = Number(v.toFixed(2));
      set(clean);
      out.textContent = fmt(clean);
    });
    wrap.append(input, out);
    row.append(wrap);
  }

  private text(label: string, get: () => string, set: (v: string) => void): void {
    const row = this.row(label);
    const input = el('input', 'text');
    input.type = 'text';
    input.maxLength = 8;
    input.value = get();
    input.spellcheck = false;
    input.addEventListener('input', () => {
      const clean = cleanPlate(input.value);
      if (clean !== input.value) input.value = clean;
      set(clean);
    });
    row.append(input);
  }
}
