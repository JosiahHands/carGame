// The garage panel: every customization option, grouped into tabs.
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
  type Tuning,
} from './carConfig';
import { el, row, segmented, slider, swatches, textInput } from '../libs/formControls';

type Change = 'look' | 'tuning';

const TABS = ['Body', 'Paint', 'Wheels', 'Parts', 'Lights', 'Tuning'] as const;
type Tab = (typeof TABS)[number];

export class GarageUI {
  readonly root: HTMLDivElement;
  private body: HTMLDivElement;
  private tabButtons = new Map<Tab, HTMLButtonElement>();
  private tab: Tab = 'Body';

  constructor(
    private config: CarConfig,
    private onChange: (config: CarConfig, change: Change) => void,
    onDrive: () => void,
    onSettings: () => void,
  ) {
    this.root = el('div', 'garage panel');
    const head = el('header');
    const settings = el('button', 'icon-btn', '⚙');
    settings.type = 'button';
    settings.title = 'Settings (Esc)';
    settings.addEventListener('click', onSettings);
    head.append(el('h1', undefined, 'Garage'), settings, el('p', undefined, 'Build your car, then hit Drive.'));
    const nav = el('nav', 'tabs');
    for (const t of TABS) {
      const b = el('button', 'tab', t);
      b.type = 'button';
      b.addEventListener('click', () => this.show(t));
      this.tabButtons.set(t, b);
      nav.append(b);
    }
    this.body = el('div', 'panel-body');
    const foot = el('footer');
    const random = el('button', 'btn', 'Randomize');
    random.addEventListener('click', () => this.replace(randomConfig(this.config)));
    const reset = el('button', 'btn', 'Reset');
    reset.addEventListener('click', () => this.replace(cloneConfig(DEFAULT_CONFIG)));
    const drive = el('button', 'btn primary', 'Drive ▶');
    drive.addEventListener('click', onDrive);
    foot.append(random, reset, drive);
    this.root.append(head, nav, this.body, foot);
    this.show('Body');
  }

  setVisible(visible: boolean): void {
    this.root.classList.toggle('hidden', !visible);
  }

  private replace(next: CarConfig): void {
    Object.assign(this.config, next, { tuning: { ...next.tuning } });
    this.onChange(this.config, 'look');
    this.onChange(this.config, 'tuning');
    this.show(this.tab);
  }

  private show(tab: Tab): void {
    this.tab = tab;
    for (const [t, b] of this.tabButtons) b.classList.toggle('active', t === tab);
    this.body.replaceChildren();
    const c = this.config;
    const look = () => this.onChange(c, 'look');
    const add = (label: string, control: HTMLElement) => this.body.append(row(label, control));
    const pct = (v: number) => `${Math.round(v * 100)}%`;
    switch (tab) {
      case 'Body':
        add('Body style', segmented(BODY_OPTIONS, () => c.body, (v) => ((c.body = v), look())));
        add('Ride height', slider(-1, 1, 0.1, () => c.rideHeight, (v) => ((c.rideHeight = v), look()), (v) => (v === 0 ? 'Stock' : v < 0 ? 'Lowered' : 'Raised')));
        add('Number plate', textInput(8, () => c.plate, (v) => ((c.plate = v), look()), cleanPlate));
        break;
      case 'Paint':
        add('Paint colour', swatches(PAINT_SWATCHES, () => c.paintColor, (v) => ((c.paintColor = v), look())));
        add('Finish', segmented(FINISH_OPTIONS, () => c.finish, (v) => ((c.finish = v), look())));
        add('Stripes', segmented(STRIPE_OPTIONS, () => c.stripes, (v) => ((c.stripes = v), look())));
        add('Accent colour (stripes, parts)', swatches(PAINT_SWATCHES, () => c.accentColor, (v) => ((c.accentColor = v), look())));
        add('Window tint', slider(0, 1, 0.05, () => c.windowTint, (v) => ((c.windowTint = v), look()), pct));
        break;
      case 'Wheels':
        add('Rims', segmented(WHEEL_OPTIONS, () => c.wheelStyle, (v) => ((c.wheelStyle = v), look())));
        add('Rim colour', swatches(['#c9ccd1', '#1a1a1a', '#ffffff', '#d4af37', '#b87333', '#d7263d', '#1b98e0', '#3bb273'], () => c.rimColor, (v) => ((c.rimColor = v), look())));
        add('Wheel size', slider(0.85, 1.25, 0.05, () => c.wheelSize, (v) => ((c.wheelSize = v), look()), pct));
        add('Tyre width', slider(0.8, 1.4, 0.05, () => c.tireWidth, (v) => ((c.tireWidth = v), look()), pct));
        break;
      case 'Parts':
        add('Spoiler', segmented(SPOILER_OPTIONS, () => c.spoiler, (v) => ((c.spoiler = v), look())));
        add('Spoiler colour', segmented(PART_COLOR_OPTIONS, () => c.spoilerColor, (v) => ((c.spoilerColor = v), look())));
        add('Exhaust', segmented(EXHAUST_OPTIONS, () => c.exhaust, (v) => ((c.exhaust = v), look())));
        add(
          'Roof weapons',
          segmented([{ value: 'on', label: 'Show' }, { value: 'off', label: 'Hide' }], () => (c.roofGuns ? 'on' : 'off'), (v) => ((c.roofGuns = v === 'on'), look())),
        );
        break;
      case 'Lights':
        add('Headlight colour', swatches(LIGHT_SWATCHES, () => c.headlightColor, (v) => ((c.headlightColor = v), look())));
        add('Underglow', segmented([{ value: 'off', label: 'Off' }, { value: 'on', label: 'On' }], () => (c.underglow ? 'on' : 'off'), (v) => ((c.underglow = v === 'on'), look())));
        add('Underglow colour', swatches(['#7af0ff', '#c58bff', '#ff4fd8', '#39ff88', '#ff3b3b', '#ffd23f', '#2f6bff', '#ffffff'], () => c.underglowColor, (v) => ((c.underglowColor = v), look())));
        add('Nitro flame colour', swatches(['#4fb3ff', '#ff7a1a', '#c58bff', '#39ff88', '#ff3bd0', '#ffffff'], () => c.nitroColor, (v) => ((c.nitroColor = v), look())));
        this.hint('Press N while driving to switch between day and night.');
        break;
      case 'Tuning':
        for (const key of Object.keys(TUNING_LABELS) as (keyof Tuning)[]) {
          add(TUNING_LABELS[key], slider(1, 10, 1, () => c.tuning[key], (v) => ((c.tuning[key] = v), this.onChange(c, 'tuning')), (v) => `${v}/10`));
        }
        this.hint('Drift sets how far the back steps out when you pull the handbrake (Space). Nitro sets how hard Shift pushes you and how long the bottle lasts.');
        break;
    }
  }

  private hint(text: string): void {
    this.body.append(el('p', 'hint', text));
  }
}
