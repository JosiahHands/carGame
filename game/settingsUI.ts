// The settings panel (Esc): graphics quality and performance, plus gameplay options.
import { applyPreset, matchingPreset, type Quality, type Settings } from './settings';
import { el, row, segmented, slider } from '../libs/formControls';

type OnOff = 'on' | 'off';
const ON_OFF = [
  { value: 'on' as const, label: 'On' },
  { value: 'off' as const, label: 'Off' },
];

export class SettingsUI {
  readonly root: HTMLDivElement;
  private body: HTMLDivElement;
  private fpsEl: HTMLSpanElement;

  constructor(
    private settings: Settings,
    /** Called after every change, with whether it needs a restart. */
    private onChange: (restartNeeded: boolean) => void,
    private onClose: () => void,
    private onRebuildCity: () => void,
  ) {
    this.root = el('div', 'settings panel hidden');
    const head = el('header');
    this.fpsEl = el('span', 'live-fps');
    head.append(el('h1', undefined, 'Settings'), this.fpsEl, el('p', undefined, 'Lower the quality if the game feels slow.'));
    this.body = el('div', 'panel-body');
    const foot = el('footer');
    const rebuild = el('button', 'btn', 'Rebuild the city');
    rebuild.title = 'Put back everything you have destroyed';
    rebuild.addEventListener('click', () => this.onRebuildCity());
    const close = el('button', 'btn primary', 'Back to game');
    close.addEventListener('click', () => this.onClose());
    foot.append(rebuild, close);
    this.root.append(head, this.body, foot);
    this.render();
  }

  get visible(): boolean {
    return !this.root.classList.contains('hidden');
  }

  setVisible(visible: boolean): void {
    this.root.classList.toggle('hidden', !visible);
    if (visible) this.render();
  }

  /** Shows the measured frame rate while the panel is open. */
  setFps(fps: number): void {
    if (this.visible) this.fpsEl.textContent = `${Math.round(fps)} FPS`;
  }

  private render(): void {
    const s = this.settings;
    this.body.replaceChildren();
    const add = (label: string, control: HTMLElement) => this.body.append(row(label, control));
    const section = (title: string) => this.body.append(el('h2', 'section', title));
    const changed = (restart = false) => {
      s.quality = matchingPreset(s);
      this.onChange(restart);
    };
    // Rebuilds the panel so the other controls show what a preset changed.
    const graphics = () => {
      changed();
      this.render();
    };
    const onOff = (get: () => boolean, set: (v: boolean) => void, restart = false) =>
      segmented<OnOff>(ON_OFF, () => (get() ? 'on' : 'off'), (v) => (set(v === 'on'), changed(restart)));

    section('Graphics');
    add(
      'Quality preset',
      segmented<Quality | 'custom'>(
        [
          { value: 'low', label: 'Low' },
          { value: 'medium', label: 'Medium' },
          { value: 'high', label: 'High' },
          { value: 'ultra', label: 'Ultra' },
          { value: 'custom', label: 'Custom' },
        ],
        () => s.quality,
        (v) => {
          if (v !== 'custom') applyPreset(s, v);
          graphics();
        },
      ),
    );
    add('Resolution', slider(0.5, 1, 0.05, () => s.renderScale, (v) => ((s.renderScale = v), graphics()), (v) => `${Math.round(v * 100)}%`));
    add(
      'Shadows',
      segmented(
        [
          { value: 'off', label: 'Off' },
          { value: 'low', label: 'Low' },
          { value: 'high', label: 'High' },
          { value: 'ultra', label: 'Ultra' },
        ],
        () => s.shadows,
        (v) => ((s.shadows = v), graphics()),
      ),
    );
    add('View distance', slider(150, 600, 10, () => s.viewDistance, (v) => ((s.viewDistance = v), graphics()), (v) => `${v} m`));
    add(
      'Effects (explosions, debris)',
      segmented(
        [
          { value: 'low', label: 'Low' },
          { value: 'medium', label: 'Medium' },
          { value: 'high', label: 'High' },
        ],
        () => s.effects,
        (v) => ((s.effects = v), graphics()),
      ),
    );

    section('Performance');
    add(
      'Frame rate limit',
      segmented(
        [
          { value: '0', label: 'None' },
          { value: '30', label: '30' },
          { value: '60', label: '60' },
          { value: '120', label: '120' },
        ],
        () => String(s.fpsLimit),
        (v) => ((s.fpsLimit = Number(v)), changed()),
      ),
    );
    add('Anti-aliasing (after restart)', onOff(() => s.antialias, (v) => (s.antialias = v), true));
    add('Show FPS counter', onOff(() => s.showFps, (v) => (s.showFps = v)));

    section('Game');
    add('Field of view', slider(50, 90, 1, () => s.fov, (v) => ((s.fov = v), changed()), (v) => `${v}°`));
    add('Weapons', onOff(() => s.weapons, (v) => (s.weapons = v)));
    add('Minimap', onOff(() => s.minimap, (v) => (s.minimap = v)));
    this.body.append(
      el('p', 'hint', 'Low draws fewer pixels, turns shadows off and shortens the view: try it on older laptops. The FPS counter shows how smooth the game is running.'),
    );
  }
}
