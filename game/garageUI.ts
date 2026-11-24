// The garage panel: every customization option grouped into tabs, ready-made
// and saved builds with share codes, and a Settings tab for graphics and game
// options.
import {
  ANTENNA_OPTIONS,
  BODY_OPTIONS,
  DEFAULT_CONFIG,
  ENGINE_OPTIONS,
  EXHAUST_OPTIONS,
  FINISH_OPTIONS,
  HORN_OPTIONS,
  LIGHT_SWATCHES,
  PAINT_SWATCHES,
  PART_COLOR_OPTIONS,
  PATTERN_OPTIONS,
  RANGES,
  SHOWCASE_BUILDS,
  SPOILER_OPTIONS,
  STRIPE_OPTIONS,
  TOPPER_OPTIONS,
  TRAIL_OPTIONS,
  UNDERGLOW_OPTIONS,
  WHEEL_OPTIONS,
  cleanPlate,
  cloneConfig,
  loadBuilds,
  randomConfig,
  readShareCode,
  saveBuilds,
  shareCode,
  showcaseConfig,
  type CarConfig,
  type Option,
  type Tuning,
} from './carConfig';
import { button, el } from '../libs/dom';
import { PRESETS, type Settings } from './settings';
import type { PanelUI } from './packs';
import { bestLapFor, formatTime } from './timeTrial';
import { VENUES, type MapName } from './venues';

/** 'look' changes need the car rebuilt; 'tuning' changes (driving, sound) don't. */
type Change = 'look' | 'tuning';

const TABS = ['Builds', 'Maps', 'Body', 'Paint', 'Wheels', 'Parts', 'Lights', 'Sound & FX', 'Engine', 'Physics', 'Settings', 'Packs'] as const;
export type Tab = (typeof TABS)[number];

const ON_OFF: Option<'on' | 'off'>[] = [
  { value: 'on', label: 'On' },
  { value: 'off', label: 'Off' },
];

/** 2.5 -> "2.5×", 12 -> "12×". */
function times(v: number): string {
  return `${v < 10 ? Number(v.toFixed(2)) : Math.round(v)}×`;
}

/** What the garage tells the game about. */
export interface GarageEvents {
  carChanged(change: Change): void;
  settingsChanged(): void;
  rebuildCity(): void;
  /** Switch to another map (already saved in the settings). */
  mapChanged(map: MapName): void;
  drive(): void;
  /** Back to the main menu. */
  menu(): void;
  /** Plays the car's horn, to try it out. */
  honk(): void;
  /** A line about the car: its size and speed. */
  describeCar(): string;
  /** Fills in the Packs tab (see game/packsUI.ts). */
  showPacks(panel: PanelUI): void;
}

export class GarageUI {
  readonly root: HTMLDivElement;
  private body: HTMLDivElement;
  private stats: HTMLParagraphElement;
  private tabButtons = new Map<Tab, HTMLButtonElement>();
  private tab: Tab = 'Builds';

  constructor(
    private config: CarConfig,
    private settings: Settings,
    private events: GarageEvents,
  ) {
    this.root = el('div', 'garage');
    const head = el('header');
    this.stats = el('p', 'stats');
    head.append(el('h1', undefined, 'Garage'), el('p', undefined, 'Build anything, then hit Drive. Your paint and parts go on your career cars too.'), this.stats);
    const nav = el('nav', 'tabs');
    for (const t of TABS) {
      const b = button(t, 'tab', () => this.show(t));
      this.tabButtons.set(t, b);
      nav.append(b);
    }
    this.body = el('div', 'garage-body');
    const foot = el('footer');
    foot.append(
      button('◀ Menu', 'btn', () => this.events.menu()),
      button('Randomize', 'btn', () => this.replace(randomConfig(this.config))),
      button('Reset', 'btn', () => this.replace(cloneConfig(DEFAULT_CONFIG))),
      button('Drive ▶', 'btn primary', () => this.events.drive()),
    );
    this.root.append(head, nav, this.body, foot);
    this.show('Builds');
  }

  setVisible(visible: boolean): void {
    this.root.classList.toggle('hidden', !visible);
    if (visible) this.refreshStats();
  }

  refreshStats(): void {
    this.stats.textContent = this.events.describeCar();
  }

  /** Swaps the whole car for another one (a build, a share code, a reset). */
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
    this.body.scrollTop = 0;
    this.refreshStats();
    const c = this.config;
    const t = c.tuning;
    // Each control calls one of these after changing the config.
    const look = () => {
      this.events.carChanged('look');
      this.refreshStats();
    };
    const tuned = () => {
      this.events.carChanged('tuning');
      this.refreshStats();
    };
    // A slider for a number setting, using its range from RANGES.
    const number = (label: string, key: keyof CarConfig, step: number, fmt: (v: number) => string, log = false) => {
      const [min, max] = RANGES[key];
      this.slider(label, min, max, step, () => c[key] as number, (v) => (((c as unknown as Record<string, number>)[key] = v), look()), fmt, log);
    };
    const tuning = (label: string, key: keyof Tuning, step: number, fmt: (v: number) => string, log = false) => {
      const [min, max] = RANGES[key];
      this.slider(label, min, max, step, () => t[key] as number, (v) => (((t as unknown as Record<string, number>)[key] = v), tuned()), fmt, log);
    };
    const onOff = (label: string, get: () => boolean, set: (on: boolean) => void) =>
      this.choice(label, ON_OFF, () => (get() ? 'on' : 'off'), (v) => set(v === 'on'));
    const length = (m: number) => (this.settings.units === 'mph' ? `${Number((m * 3.281).toFixed(m < 3 ? 1 : 0))} ft` : `${Number(m.toFixed(m < 10 ? 1 : 0))} m`);
    const speed = (mph: number) => (this.settings.units === 'mph' ? `${Math.round(mph)} mph` : `${Math.round(mph * 1.609)} km/h`);

    switch (tab) {
      case 'Builds':
        this.showBuilds();
        break;
      case 'Maps':
        this.showMaps();
        break;
      case 'Body':
        this.choice('Body style', BODY_OPTIONS, () => c.body, (v) => ((c.body = v), look()));
        this.heading('Size and shape');
        number('Size', 'size', 0.05, times, true);
        number('Length', 'length', 0.05, times);
        number('Width', 'width', 0.05, times);
        number('Height', 'height', 0.05, times);
        this.hint('Big enough (about 10 ft tall or more) and the car flattens buildings, cars, lamps and trees in its way.');
        this.heading('Plates and numbers');
        this.text('Number plate', 8, () => c.plate, (v) => ((c.plate = cleanPlate(v)), look()), cleanPlate);
        number('Race number', 'raceNumber', 1, (v) => (v === 0 ? 'None' : `#${v}`));
        break;
      case 'Paint':
        this.color('Paint colour', PAINT_SWATCHES, () => c.paintColor, (v) => ((c.paintColor = v), look()));
        this.choice('Finish', FINISH_OPTIONS, () => c.finish, (v) => ((c.finish = v), look()));
        this.choice('Pattern', PATTERN_OPTIONS, () => c.pattern, (v) => ((c.pattern = v), look()));
        this.choice('Stripes', STRIPE_OPTIONS, () => c.stripes, (v) => ((c.stripes = v), look()));
        this.color('Accent colour (patterns, stripes, parts)', PAINT_SWATCHES, () => c.accentColor, (v) => ((c.accentColor = v), look()));
        number('Window tint', 'windowTint', 0.05, (v) => `${Math.round(v * 100)}%`);
        break;
      case 'Wheels':
        this.choice('Rims', WHEEL_OPTIONS, () => c.wheelStyle, (v) => ((c.wheelStyle = v), look()));
        this.color('Rim colour', ['#c9ccd1', '#1a1a1a', '#ffffff', '#d4af37', '#b87333', '#d7263d', '#1b98e0', '#3bb273'], () => c.rimColor, (v) => ((c.rimColor = v), look()));
        this.color('Tyre colour', ['#1a1a1a', '#f2f2f2', '#d7263d', '#1b98e0', '#ffd23f', '#ff70a6', '#3bb273', '#7b2cbf'], () => c.tireColor, (v) => ((c.tireColor = v), look()));
        this.heading('Size and stance');
        number('Wheel size', 'wheelSize', 0.05, times);
        number('Tyre width', 'tireWidth', 0.05, times);
        number('Wheel spacing', 'track', 0.05, times);
        number('Camber', 'camber', 1, (v) => (v === 0 ? 'Straight' : `${v}°`));
        number('Suspension lift', 'lift', 0.01, (v) => (v === 0 ? 'Stock' : v < 0 ? 'Slammed' : `+${length(v)}`));
        this.hint('Wheels too big for the arches lift the body up on top of them, monster truck style.');
        break;
      case 'Parts':
        this.choice('Spoiler', SPOILER_OPTIONS, () => c.spoiler, (v) => ((c.spoiler = v), look()));
        this.choice('Spoiler colour', PART_COLOR_OPTIONS, () => c.spoilerColor, (v) => ((c.spoilerColor = v), look()));
        this.choice('Exhaust', EXHAUST_OPTIONS, () => c.exhaust, (v) => ((c.exhaust = v), look()));
        this.choice('Roof topper', TOPPER_OPTIONS, () => c.topper, (v) => ((c.topper = v), look()));
        this.choice('Antenna', ANTENNA_OPTIONS, () => c.antenna, (v) => ((c.antenna = v), look()));
        onOff('Bull bar', () => c.bullBar, (on) => ((c.bullBar = on), look()));
        onOff('Roof light bar', () => c.lightBar, (on) => ((c.lightBar = on), look()));
        onOff('Roof weapons', () => c.roofGuns, (on) => ((c.roofGuns = on), look()));
        break;
      case 'Lights':
        this.color('Headlight colour', LIGHT_SWATCHES, () => c.headlightColor, (v) => ((c.headlightColor = v), look()));
        this.choice('Underglow', UNDERGLOW_OPTIONS, () => c.underglow, (v) => ((c.underglow = v), look()));
        this.color('Underglow colour', ['#7af0ff', '#c58bff', '#ff4fd8', '#39ff88', '#ff3b3b', '#ffd23f', '#2f6bff', '#ffffff'], () => c.underglowColor, (v) => ((c.underglowColor = v), look()));
        this.color('Nitro flame colour', ['#4fb3ff', '#ff7a1a', '#c58bff', '#39ff88', '#ff3bd0', '#ffffff'], () => c.nitroColor, (v) => ((c.nitroColor = v), look()));
        this.hint('Press N while driving to switch between day and night. Glow paint and underglow look best at night.');
        break;
      case 'Sound & FX':
        this.choice('Engine sound', ENGINE_OPTIONS, () => c.engineSound, (v) => ((c.engineSound = v), tuned()));
        this.choice('Horn', HORN_OPTIONS, () => c.horn, (v) => ((c.horn = v), tuned(), this.events.honk()));
        this.choice('Boost trail', TRAIL_OPTIONS, () => c.boostTrail, (v) => ((c.boostTrail = v), tuned()));
        onOff('Tyre smoke', () => c.tireSmoke, (on) => ((c.tireSmoke = on), tuned()));
        this.color('Tyre smoke colour', ['#e8e8e8', '#3a3a3a', '#ff4fd8', '#7af0ff', '#39ff88', '#ffd23f', '#ff3b3b', '#c58bff'], () => c.tireSmokeColor, (v) => ((c.tireSmokeColor = v), tuned()));
        this.hint('Press V to sound the horn while driving. Tyre smoke shows when you drift.');
        break;
      case 'Engine':
        tuning('Horsepower', 'horsepower', 10, (v) => `${Math.round(v).toLocaleString()} hp`, true);
        tuning('Top speed', 'topSpeed', 1, speed, true);
        tuning('Nitro power', 'nitroPower', 0.05, times);
        tuning('Nitro bottle', 'nitroSeconds', 0.5, (v) => `${v} s`);
        onOff('Infinite nitro', () => t.infiniteNitro, (on) => ((t.infiniteNitro = on), tuned()));
        tuning('Downforce', 'downforce', 0.5, (v) => (v === 0 ? 'None' : `${v}/10`));
        this.hint('A high top speed needs lots of horsepower to reach. Downforce adds grip at speed, so a 500 mph car can still turn.');
        break;
      case 'Physics':
        tuning('Grip', 'grip', 0.05, times);
        tuning('Brakes', 'brakes', 0.05, times);
        tuning('Steering lock', 'steering', 1, (v) => `${v}°`);
        tuning('Drift (handbrake)', 'drift', 1, (v) => `${v}/10`);
        tuning('Jump height', 'jump', 0.5, (v) => (v === 0 ? 'Off' : length(v)));
        tuning('Air control', 'airControl', 0.05, (v) => `${Math.round(v * 100)}%`);
        tuning('Gravity', 'gravity', 0.01, (v) => (Math.abs(v - 0.17) < 0.02 ? 'Moon' : Math.abs(v - 1) < 0.02 ? 'Earth' : Math.abs(v - 2.5) < 0.05 ? 'Jupiter' : times(v)), true);
        tuning('Wall bounce', 'bounce', 0.05, (v) => `${Math.round(v * 100)}%`);
        this.hint('Press J to jump (when jump height is above zero). Air control lets you steer and boost in the air.');
        break;
      case 'Packs':
        this.events.showPacks(this.panel());
        break;
      case 'Settings':
        this.showSettings();
        break;
    }
  }

  private showBuilds(): void {
    // Ready-made cars.
    this.heading('Showcase builds');
    const grid = el('div', 'build-grid');
    for (const build of SHOWCASE_BUILDS) {
      const b = button('', 'build', () => this.replace(showcaseConfig(build.changes)));
      b.append(el('strong', undefined, build.name), el('span', undefined, build.about));
      grid.append(b);
    }
    this.body.append(grid);

    // The player's own saved cars.
    this.heading('Your builds');
    const builds = loadBuilds();
    const saveRow = el('div', 'inline');
    const name = el('input', 'text');
    name.type = 'text';
    name.maxLength = 30;
    name.placeholder = 'Name this car';
    const save = () => {
      builds.unshift({ name: name.value.trim() || `Build ${builds.length + 1}`, config: cloneConfig(this.config) });
      saveBuilds(builds.slice(0, 30));
      this.show('Builds');
    };
    name.addEventListener('keydown', (e) => e.key === 'Enter' && save());
    saveRow.append(name, button('Save', 'btn', save));
    this.body.append(saveRow);
    if (builds.length === 0) this.hint('Saved cars show up here.');
    builds.forEach((build, i) => {
      const row = el('div', 'build-row');
      row.append(
        el('span', undefined, build.name),
        button('Load', 'btn', () => this.replace(build.config)),
        button('Delete', 'btn', () => {
          builds.splice(i, 1);
          saveBuilds(builds);
          this.show('Builds');
        }),
      );
      this.body.append(row);
    });

    // Share codes: copy yours, or paste a friend's.
    this.heading('Share code');
    const code = el('input', 'text code');
    code.type = 'text';
    code.readOnly = true;
    code.value = shareCode(this.config);
    code.addEventListener('focus', () => code.select());
    const copyRow = el('div', 'inline');
    const copy = button('Copy', 'btn', () => {
      void navigator.clipboard?.writeText(code.value).then(
        () => (copy.textContent = 'Copied'),
        () => code.select(),
      );
    });
    copyRow.append(code, copy);
    this.body.append(copyRow);
    const pasteRow = el('div', 'inline');
    const paste = el('input', 'text code');
    paste.type = 'text';
    paste.placeholder = 'Paste a code: CAR1-…';
    const message = el('p', 'hint');
    const load = () => {
      const car = readShareCode(paste.value);
      if (car) this.replace(car);
      else message.textContent = "That isn't a car code. They start with CAR1-.";
    };
    paste.addEventListener('keydown', (e) => e.key === 'Enter' && load());
    pasteRow.append(paste, button('Load', 'btn', load));
    this.body.append(pasteRow, message);
  }

  private showSettings(): void {
    const s = this.settings;
    const changed = () => this.events.settingsChanged();
    const onOff = (on: boolean) => (on ? 'on' : 'off');

    // Preset buttons set several settings at once, then redraw the tab to show them.
    const presets = el('div', 'segmented');
    for (const [name, values] of Object.entries(PRESETS)) {
      presets.append(
        button(name, '', () => {
          Object.assign(s, values);
          changed();
          this.show('Settings');
        }),
      );
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
    this.choice(
      'Units',
      [
        { value: 'mph', label: 'mph and feet' },
        { value: 'kmh', label: 'km/h and metres' },
      ],
      () => s.units,
      (v) => ((s.units = v), changed(), this.refreshStats()),
    );
    this.choice(
      'Gearbox',
      [
        { value: 'auto', label: 'Automatic' },
        { value: 'manual', label: 'Manual (X / Z)' },
      ],
      () => s.transmission,
      (v) => ((s.transmission = v), changed()),
    );
    this.choice('Weapons', ON_OFF, () => onOff(s.weapons), (v) => ((s.weapons = v === 'on'), changed()));
    this.choice('Minimap', ON_OFF, () => onOff(s.minimap), (v) => ((s.minimap = v === 'on'), changed()));
    this.choice(
      'City traffic',
      [
        { value: 'off', label: 'Off' },
        { value: 'light', label: 'Light' },
        { value: 'normal', label: 'Normal' },
        { value: 'busy', label: 'Busy' },
      ],
      () => s.traffic,
      (v) => ((s.traffic = v), changed()),
    );
    this.choice('Pedestrians', ON_OFF, () => onOff(s.pedestrians), (v) => ((s.pedestrians = v === 'on'), changed()));
    this.choice('Music', ON_OFF, () => onOff(s.music), (v) => ((s.music = v === 'on'), changed()));
    this.choice('Announcer', ON_OFF, () => onOff(s.announcer), (v) => ((s.announcer = v === 'on'), changed()));
    this.choice('Speed lines', ON_OFF, () => onOff(s.speedLines), (v) => ((s.speedLines = v === 'on'), changed()));
    this.row('Destroyed buildings').append(button('Rebuild everything', 'btn', () => this.events.rebuildCity()));
    this.hint('If the game feels slow, try the Low or Medium preset and turn on the FPS counter to see the difference.');
  }

  /** A card for each map, with its best lap. Picking one switches to it straight away. */
  private showMaps(): void {
    const grid = el('div', 'build-grid maps');
    for (const m of VENUES) {
      const best = bestLapFor(m.value);
      const card = button('', 'build', () => {
        if (this.settings.map === m.value) return;
        this.settings.map = m.value;
        this.events.mapChanged(m.value);
        this.show('Maps');
      });
      card.classList.toggle('active', this.settings.map === m.value);
      card.append(el('strong', undefined, m.label), el('span', undefined, m.about), el('em', undefined, best > 0 ? `Best lap ${formatTime(best)}` : 'No lap yet'));
      grid.append(card);
    }
    this.body.append(grid);
    this.hint('Press T while driving for a timed lap. Beat your best and a ghost of that lap races you next time.');
  }

  /** The garage's controls, for a pack's options on the Packs tab. */
  private panel(): PanelUI {
    return {
      heading: (text) => this.heading(text),
      hint: (text) => this.hint(text),
      choice: (label, options, get, set) => this.choice(label, options, get, set),
      slider: (label, min, max, step, get, set, format) => this.slider(label, min, max, step, get, set, format),
      row: (label, ...content) => this.row(label).append(...content),
      append: (...content) => this.body.append(...content),
      refresh: () => {
        const top = this.body.scrollTop;
        this.show('Packs');
        this.body.scrollTop = top;
      },
    };
  }

  // ---------- controls ----------

  private row(label: string): HTMLDivElement {
    const row = el('div', 'row');
    row.append(el('label', undefined, label));
    this.body.append(row);
    return row;
  }

  private heading(text: string): void {
    this.body.append(el('h3', 'section', text));
  }

  private hint(text: string): void {
    this.body.append(el('p', 'hint', text));
  }

  private choice<T extends string>(label: string, options: Option<T>[], get: () => T, set: (v: T) => void): void {
    const row = this.row(label);
    const group = el('div', 'segmented');
    const buttons: [T, HTMLButtonElement][] = [];
    for (const o of options) {
      const b = button(o.label, '', () => {
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
      const b = button('', 'swatch', () => {
        set(s);
        sync();
      });
      b.style.background = s;
      b.dataset.color = s;
      b.title = s;
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

  /**
   * A slider from `min` to `max`, rounded to `step`. A `log` slider gives each
   * doubling the same room, so 0.25× to 25× or 50 to 10,000 hp is usable at
   * both ends.
   */
  private slider(label: string, min: number, max: number, step: number, get: () => number, set: (v: number) => void, fmt: (v: number) => string, log = false): void {
    const row = this.row(label);
    const wrap = el('div', 'slider');
    const input = el('input');
    input.type = 'range';
    // The slider itself always runs 0..1000; these convert to and from the value.
    input.min = '0';
    input.max = '1000';
    const toSlider = (v: number) => (log ? (1000 * Math.log(v / min)) / Math.log(max / min) : (1000 * (v - min)) / (max - min));
    const fromSlider = (n: number) => {
      const raw = log ? min * (max / min) ** (n / 1000) : min + ((max - min) * n) / 1000;
      return Math.min(max, Math.max(min, Number((Math.round(raw / step) * step).toFixed(3))));
    };
    input.value = String(toSlider(get()));
    const out = el('span', 'value', fmt(get()));
    input.addEventListener('input', () => {
      const v = fromSlider(Number(input.value));
      if (v === get()) return;
      set(v);
      out.textContent = fmt(v);
    });
    wrap.append(input, out);
    row.append(wrap);
  }

  private text(label: string, maxLength: number, get: () => string, set: (v: string) => void, clean: (v: string) => string): void {
    const row = this.row(label);
    const input = el('input', 'text');
    input.type = 'text';
    input.maxLength = maxLength;
    input.value = get();
    input.spellcheck = false;
    input.addEventListener('input', () => {
      const cleaned = clean(input.value);
      if (cleaned !== input.value) input.value = cleaned;
      set(cleaned);
    });
    row.append(input);
  }
}
