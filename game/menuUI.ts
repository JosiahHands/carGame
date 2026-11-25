// The menus: the main menu, the career (levels, cars and upgrades, the daily
// challenge, stats), quick race setup, and the screens shown over the game:
// pause, results, and the end of the career.
import { button, el } from '../libs/dom';
import {
  BOSS_STARS, CHAPTERS, CHASSIS, DIFFICULTIES, LEVELS, UPGRADES, WEAPON_SHOP, WEAPON_UPGRADE_ABOUT,
  awaitingChoice, buyWeapon, careerCar, careerTuning, chapterStars, dailyLevel, lockedReason, makeLevel, nextLevel, noUpgrades, prestige,
  quickRaceSpec, resetCareer, today, totalStars, upgradeCap, upgradeCost, weaponCost, weaponLockedReason,
  type CareerSave, type Level, type Payout, type UpgradeId,
} from './career';
import { MAX_WEAPON_LEVEL } from './weapons';
import { SHOWCASE_BUILDS, showcaseConfig, type CarConfig, type Tuning } from './carConfig';
import { EVENT_KINDS, type EventKind, type EventResult, type EventSpec } from './event';
import { PRIZES, TROPHIES, WRENCHES, ZONE_STARS, driverLevel, spinWheel } from './rewards';
import { formatTime } from './timeTrial';
import { VENUES, venue, type MapName } from './venues';

/** What the menus ask the game to do. */
export interface MenuEvents {
  /** Play a career level (or the daily challenge, which has number 0). */
  playLevel(level: Level): void;
  playQuick(spec: EventSpec, car: CarConfig): void;
  freeRoam(): void;
  /** Free roam in the career car, where challenge stars count. */
  freeRoamCareer(): void;
  garage(): void;
  /** The career may have changed (a car or weapon bought, chosen or upgraded): save it and show the car. */
  carChanged(): void;
  resume(): void;
  restart(): void;
  quit(): void;
  /** After beating level 100: carry on into the sandbox, or reset the career. */
  chooseSandbox(): void;
  chooseReset(): void;
  /** 'mph' or 'kmh', for showing speeds. */
  units(): 'mph' | 'kmh';
}

type CareerTab = 'Levels' | 'Garage' | 'Daily' | 'Trophies' | 'Stats';

const KIND_ORDER: EventKind[] = ['race', 'elimination', 'duel', 'timeAttack', 'takedown', 'drift', 'speedTrap', 'destruction', 'stunt'];

export class MenuUI {
  readonly root = el('div', 'menu hidden');
  private panel = el('div', 'menu-panel');
  /** The career chapter shown on the Levels tab (11 for the sandbox). */
  private chapter = 1;
  private tab: CareerTab = 'Levels';
  private quick = {
    kind: 'race' as EventKind, map: 'circuit' as MapName, layout: 0, laps: 3, rivals: 5, difficulty: 1,
    car: 'garage', night: false, weapons: false,
  };

  constructor(
    public save: CareerSave,
    private looks: CarConfig,
    private events: MenuEvents,
  ) {
    this.root.append(this.panel);
    this.chapter = Math.min(10, Math.ceil(nextLevel(save) / 10));
  }

  get visible(): boolean {
    return !this.root.classList.contains('hidden');
  }

  hide(): void {
    this.root.classList.add('hidden');
  }

  private open(cls: string, ...children: HTMLElement[]): void {
    this.root.classList.remove('hidden');
    this.root.className = `menu ${cls}`;
    this.panel.replaceChildren(...children);
    this.panel.scrollTop = 0;
  }

  // ---------- main menu ----------

  showMain(): void {
    const s = this.save;
    const title = el('div', 'menu-title');
    title.append(el('h1', undefined, 'CAR GAME'), el('p', undefined, 'Race. Drift. Smash. Upgrade. Repeat.'));
    const level = s.mode === 'sandbox' ? `Sandbox level ${s.sandboxLevel}` : `Level ${nextLevel(s)} of ${LEVELS}`;
    const career = this.bigButton('🏆 Career', `${level} · ${totalStars(s)} ★ · 🪙 ${s.coins.toLocaleString()}`, () => this.showCareer());
    career.classList.add('primary');
    const daily = s.daily === today() ? 'Daily challenge done ✓' : '🎁 A new daily challenge is waiting';
    const list = el('div', 'menu-list');
    // Level 100 beaten but no choice made yet (say the game was closed on the results).
    if (awaitingChoice(s)) {
      const choose = this.bigButton('🏆 Career complete!', 'Choose what happens next: Sandbox or Reset', () => this.showCareerComplete());
      choose.classList.add('primary');
      list.append(choose);
      career.classList.remove('primary');
    }
    const zoneStars = Object.values(s.zones).reduce((a, b) => a + b, 0);
    list.append(
      career,
      this.bigButton('🏁 Quick race', 'Any car, any track, any difficulty', () => this.showQuick()),
      this.bigButton('🗺️ Free roam', `Your garage build. Weapons, stunts, time trials · 🔧 ${s.found.length}/${WRENCHES} gold wrenches`, () => this.events.freeRoam()),
      this.bigButton('🚗 Free roam in your career car', `Speed cameras, drift and jump zones · ${zoneStars}/${ZONE_STARS} ★`, () => this.events.freeRoamCareer()),
      this.bigButton('🔧 Garage & settings', 'Customize your car and the game', () => this.events.garage()),
    );
    if (s.spins > 0) {
      const spin = this.bigButton(`🎰 Prize wheel × ${s.spins}`, 'You levelled up! Spin for coins, upgrades and coin rushes', () => this.showWheel());
      spin.classList.add('prize');
      list.prepend(spin);
    }
    const foot = el('p', 'menu-foot', daily);
    this.open('main', title, this.driverCard(), list, foot);
  }

  /** The driver level, with a bar for the XP to the next one. */
  private driverCard(): HTMLElement {
    const s = this.save;
    const d = driverLevel(s.xp);
    const card = el('div', 'driver-card');
    const bar = el('div', 'bar');
    const fill = el('div', 'fill');
    fill.style.width = `${Math.round((d.into / d.need) * 100)}%`;
    bar.append(fill);
    const extra = [`🏅 ${s.trophies.length}/${TROPHIES.length} trophies`];
    if (s.coinRush > 0) extra.push(`⚡ Coin rush × ${s.coinRush}`);
    card.append(
      el('span', 'driver-level', `Driver level ${d.level}`),
      bar,
      el('span', 'driver-xp', `${d.into.toLocaleString()} / ${d.need.toLocaleString()} XP · ${extra.join(' · ')}`),
    );
    return card;
  }

  // ---------- the prize wheel ----------

  /** The prize wheel: one spin for every driver level gained. */
  showWheel(): void {
    const box = el('div', 'overlay-box wheel-box');
    box.append(el('h2', 'gold', '🎰 Prize wheel'), el('p', 'muted', 'You earn a spin every time your driver level goes up.'));
    const holder = el('div', 'wheel-holder');
    const wheel = el('div', 'wheel');
    const slice = 360 / PRIZES.length;
    wheel.style.background = `conic-gradient(${PRIZES.map((p, i) => `${p.color} ${i * slice}deg ${(i + 1) * slice}deg`).join(', ')})`;
    PRIZES.forEach((p, i) => {
      const label = el('span', 'wheel-label', p.label);
      label.style.transform = `rotate(${i * slice + slice / 2}deg) translateY(-118px) rotate(-90deg)`;
      wheel.append(label);
    });
    holder.append(wheel, el('div', 'wheel-pointer', '▼'));
    const text = el('p', 'wheel-text', `Spins left: ${this.save.spins}`);
    const actions = el('div', 'actions');
    let turned = 0;
    const spin = button('SPIN!', 'btn primary big', () => {
      if (this.save.spins <= 0) return;
      spin.disabled = true;
      const { index, text: won } = spinWheel(this.save);
      // Turn at least five times, then stop with the prize under the pointer (a bit off centre, for realism).
      const centre = index * slice + slice / 2;
      const jitter = (Math.random() - 0.5) * slice * 0.6;
      turned += 360 * 5 + ((360 - centre - (turned % 360) + 360) % 360) + jitter;
      wheel.style.transform = `rotate(${turned}deg)`;
      turned -= jitter;
      text.textContent = 'Spinning…';
      window.setTimeout(() => {
        text.textContent = won;
        text.classList.add('won');
        spin.disabled = this.save.spins <= 0;
        spin.textContent = this.save.spins > 0 ? `SPIN AGAIN (${this.save.spins})` : 'No spins left';
        this.events.carChanged(); // a free upgrade changes the car
      }, 4200);
    });
    actions.append(spin, button('Done', 'btn big', () => this.showMain()));
    box.append(holder, text, actions);
    this.open('overlay', box);
  }

  private bigButton(label: string, sub: string, onClick: () => void): HTMLButtonElement {
    const b = button('', 'menu-big', onClick);
    b.append(el('span', 'big-label', label), el('span', 'big-sub', sub));
    return b;
  }

  // ---------- career ----------

  /** The career screen, on a tab; `chapter` picks the chapter shown on the Levels tab (11 for the sandbox). */
  showCareer(tab: CareerTab = this.tab, chapter = this.chapter): void {
    this.tab = tab;
    this.chapter = chapter;
    this.events.carChanged(); // show the career car behind the menu
    const s = this.save;
    const head = el('header', 'career-head');
    const back = button('◀', 'btn back', () => this.showMain());
    const titles = el('div', 'career-titles');
    titles.append(el('h2', undefined, s.mode === 'sandbox' ? 'Career · Sandbox' : 'Career'));
    if (s.completions > 0) titles.append(el('span', 'badge', `Prestige ${s.completions} · coins ×${prestige(s).toFixed(2)}`));
    const wallet = el('div', 'wallet');
    wallet.append(el('span', 'coins', `🪙 ${s.coins.toLocaleString()}`), el('span', 'stars', `★ ${totalStars(s)} / ${LEVELS * 3}`));
    head.append(back, titles, wallet);

    const nav = el('nav', 'tabs');
    for (const t of ['Levels', 'Garage', 'Daily', 'Trophies', 'Stats'] as CareerTab[]) {
      const b = button(t === 'Garage' ? 'Cars & upgrades' : t, 'tab', () => this.showCareer(t));
      b.classList.toggle('active', t === tab);
      nav.append(b);
    }
    const body = el('div', 'career-body');
    if (tab === 'Levels') this.levelsTab(body);
    else if (tab === 'Garage') this.garageTab(body);
    else if (tab === 'Daily') this.dailyTab(body);
    else if (tab === 'Trophies') this.trophiesTab(body);
    else this.statsTab(body);
    this.open('career', head, nav, body);
  }

  private levelsTab(body: HTMLElement): void {
    const s = this.save;
    const chapters = el('div', 'chapters');
    CHAPTERS.forEach((ch, i) => {
      const n = i + 1;
      const open = lockedReason(s, (n - 1) * 10 + 1) === '';
      let stars = 0;
      for (let k = 0; k < 10; k++) stars += s.stars[i * 10 + k];
      const b = button('', `chapter${n === this.chapter ? ' active' : ''}${open ? '' : ' locked'}`, () => {
        this.chapter = n;
        this.showCareer('Levels');
      });
      b.append(el('span', 'ch-num', open ? String(n) : '🔒'), el('span', 'ch-name', ch.name), el('span', 'ch-stars', `${stars}/30 ★`));
      chapters.append(b);
    });
    if (s.mode === 'sandbox') {
      const b = button('', `chapter sandbox${this.chapter === 11 ? ' active' : ''}`, () => {
        this.chapter = 11;
        this.showCareer('Levels');
      });
      b.append(el('span', 'ch-num', '∞'), el('span', 'ch-name', 'Sandbox'), el('span', 'ch-stars', `Lv ${s.sandboxLevel}`));
      chapters.append(b);
    }
    body.append(chapters);

    if (this.chapter === 11) {
      body.append(el('h3', undefined, 'Sandbox: endless levels'));
      body.append(el('p', 'muted', 'Every level is a surprise, and each one is harder than the last: smarter rivals, faster cars, tougher targets. Upgrades go up to level 20 here. How far can you go?'));
      const grid = el('div', 'levels');
      const first = Math.max(LEVELS + 1, s.sandboxLevel - 6);
      for (let n = first; n <= s.sandboxLevel; n++) grid.append(this.levelTile(makeLevel(n), n < s.sandboxLevel ? 3 : 0, n < s.sandboxLevel ? 'Cleared' : 'Next', ''));
      body.append(grid);
      return;
    }
    const ch = CHAPTERS[this.chapter - 1];
    const intro = el('div', 'chapter-intro');
    intro.append(el('h3', undefined, `Chapter ${this.chapter}: ${ch.name}`), el('p', 'muted', ch.about));
    const have = chapterStars(s, this.chapter);
    intro.append(el('p', 'muted', `Boss: ${ch.boss} · needs ${BOSS_STARS} ★ from levels ${(this.chapter - 1) * 10 + 1}-${this.chapter * 10 - 1} (you have ${have})`));
    body.append(intro);
    const grid = el('div', 'levels');
    for (let i = 0; i < 10; i++) {
      const n = (this.chapter - 1) * 10 + i + 1;
      grid.append(this.levelTile(makeLevel(n), s.stars[n - 1], s.best[n - 1], lockedReason(s, n)));
    }
    body.append(grid);
  }

  private levelTile(level: Level, stars: number, best: string, locked: string): HTMLButtonElement {
    const spec = level.spec;
    const kind = EVENT_KINDS[spec.kind];
    const b = button('', `level${locked ? ' locked' : ''}${level.isBoss ? ' boss' : ''}${stars === 3 ? ' perfect' : ''}`, () => this.showLevel(level, locked));
    b.append(
      el('span', 'lv-num', String(level.number)),
      el('span', 'lv-icon', locked ? '🔒' : kind.icon),
      el('span', 'lv-kind', level.isBoss ? spec.boss ?? 'Boss' : kind.label),
      el('span', 'lv-venue', venue(spec.map).label + (spec.night && spec.map !== 'neon' ? ' · night' : '')),
      el('span', 'lv-stars', '★'.repeat(stars) + '☆'.repeat(3 - stars)),
    );
    if (best) b.title = `Best: ${best}`;
    return b;
  }

  /** The level's details, with the Start button. */
  showLevel(level: Level, locked = ''): void {
    const spec = level.spec;
    const kind = EVENT_KINDS[spec.kind];
    const head = el('header', 'career-head');
    head.append(button('◀', 'btn back', () => (level.number === 0 ? this.showCareer('Daily') : this.showCareer('Levels'))));
    const titles = el('div', 'career-titles');
    titles.append(el('h2', undefined, `${kind.icon} ${spec.title}`), el('span', 'badge', level.number > 0 ? `Level ${level.number}` : 'Daily challenge'));
    head.append(titles);
    const card = el('div', 'level-card');
    card.append(el('p', 'rules', level.isBoss ? `${spec.boss} is waiting. ${kind.rules}` : kind.rules));
    const facts = el('dl', 'facts');
    const fact = (k: string, v: string) => facts.append(el('dt', undefined, k), el('dd', undefined, v));
    fact('Event', kind.label);
    fact('Where', `${venue(spec.map).label}${spec.map !== 'city' ? (spec.layout === 0 ? ' · main track' : ` · track #${spec.layout}`) : ''}`);
    if (spec.kind === 'elimination') fact('Knockouts', `last place out every ${spec.timeLimit} s`);
    else if (['race', 'duel', 'timeAttack', 'speedTrap'].includes(spec.kind)) fact('Laps', String(spec.laps));
    else fact('Time limit', formatTime(spec.timeLimit));
    if (spec.rivals.length) fact('Rivals', `${spec.rivals.length} · ${spec.rivals[spec.rivals.length - 1].config.body === 'wedge' ? 'supercars' : rivalCarName(spec.rivals[spec.rivals.length - 1].config)}`);
    const twists: string[] = [];
    if (spec.night) twists.push('🌙 Night');
    if (spec.noNitro) twists.push('🚫 No nitro');
    if (spec.weapons && spec.kind === 'race') twists.push('🔫 Combat race: weapons on');
    if (venue(spec.map).theme.weather !== 'clear') twists.push(venue(spec.map).theme.weather === 'rain' ? '🌧️ Rain' : '❄️ Snow');
    if (twists.length) fact('Twists', twists.join(' · '));
    if (spec.weapons) {
      // Only what you've bought: with none yet, it's rams and crashes.
      const mine = WEAPON_SHOP.filter((w) => this.save.arsenal[w.id] > 0).map((w) => `${w.icon} ${w.name} ${this.save.arsenal[w.id]}`);
      fact('Your weapons', mine.length ? mine.join(' · ') : 'None yet: ram them! Buy weapons under Cars & upgrades');
    }
    fact('Stars', this.targetsText(spec));
    // 3 stars, plus the bonuses for a first clear with 3 new stars (see payLevel).
    const best = level.number === 0 ? 1 : 1 + 0.5 + 0.6;
    fact('Reward', `up to 🪙 ${Math.round(level.reward * best * prestige(this.save)).toLocaleString()}`);
    card.append(facts);
    const car = CHASSIS[this.save.car];
    card.append(el('p', 'muted', `Your car: ${car.name}. Change it or upgrade it under Cars & upgrades.`));
    const actions = el('div', 'actions');
    if (locked) actions.append(el('p', 'locked-reason', `🔒 ${locked}`));
    else actions.append(button('Start ▶', 'btn primary big', () => this.events.playLevel(level)));
    actions.append(button('Cars & upgrades', 'btn', () => this.showCareer('Garage')));
    this.open('career', head, card, actions);
  }

  /** What each star needs. Time attack and speed trap targets are only known once the event has started. */
  private targetsText(spec: EventSpec, started = false): string {
    const t = spec.targets;
    switch (spec.kind) {
      case 'race':
      case 'elimination':
        return '★ 3rd · ★★ 2nd · ★★★ 1st';
      case 'duel':
        return '★★★ win';
      case 'timeAttack':
        return started ? `★ ${formatTime(t[0])} · ★★ ${formatTime(t[1])} · ★★★ ${formatTime(t[2])}` : 'Target times are set by a pace car at the start';
      case 'speedTrap':
        return started ? `★ ${t[0]} mph · ★★ ${t[1]} mph · ★★★ ${t[2]} mph` : 'Target speeds are set by a pace car at the start';
      default: {
        const fmt = (v: number) => (spec.kind === 'destruction' ? `$${v.toLocaleString()}` : spec.kind === 'stunt' ? `${v} s` : spec.kind === 'takedown' ? `${v}` : v.toLocaleString());
        return `★ ${fmt(t[0])} · ★★ ${fmt(t[1])} · ★★★ ${fmt(t[2])}`;
      }
    }
  }

  private garageTab(body: HTMLElement): void {
    const s = this.save;
    const current = careerTuning(s.car, s.upgrades);
    body.append(el('h3', undefined, 'Cars'));
    const cars = el('div', 'cars');
    CHASSIS.forEach((c, i) => {
      const owned = s.owned.includes(i);
      const card = el('div', `car-card${i === s.car ? ' active' : ''}${owned ? '' : ' unowned'}`);
      card.append(el('h4', undefined, c.name), el('p', 'perk', c.perk));
      card.append(this.statBars(careerTuning(i, s.upgrades), current));
      if (i === s.car) card.append(el('span', 'tag', 'Driving'));
      else if (owned) {
        card.append(button('Drive this', 'btn', () => {
          s.car = i;
          this.changed();
        }));
      } else {
        const b = button(`Buy 🪙 ${c.price.toLocaleString()}`, 'btn primary', () => {
          if (s.coins < c.price) return;
          s.coins -= c.price;
          s.owned.push(i);
          s.car = i;
          this.changed();
        });
        b.disabled = s.coins < c.price;
        card.append(b);
      }
      cars.append(card);
    });
    body.append(cars);

    const cap = upgradeCap(s);
    body.append(el('h3', undefined, 'Upgrades'), el('p', 'muted', `Upgrades fit every car you own. Up to level ${cap}${cap < 20 ? ' (20 in the sandbox)' : ''}.`));
    const list = el('div', 'upgrades');
    for (const u of UPGRADES) {
      const level = s.upgrades[u.id];
      const row = el('div', 'upgrade');
      const pips = el('div', 'pips');
      for (let k = 0; k < cap; k++) pips.append(el('span', k < level ? 'pip on' : 'pip'));
      const info = el('div', 'up-info');
      info.append(el('strong', undefined, `${u.icon} ${u.name} · ${level}`), el('span', 'muted', u.about), pips);
      row.append(info);
      if (level >= cap) row.append(el('span', 'tag', 'MAX'));
      else {
        const cost = upgradeCost(level);
        const b = button(`🪙 ${cost.toLocaleString()}`, 'btn primary', () => this.buyUpgrade(u.id));
        b.disabled = s.coins < cost;
        row.append(b);
      }
      list.append(row);
    }
    body.append(list);

    // Weapons: bought one at a time as they unlock, then upgraded. Only what
    // you've bought is on your car, in events with weapons and in free roam.
    body.append(el('h3', undefined, 'Weapons'), el('p', 'muted', `Your car carries the weapons you buy, in events with weapons and in free roam. ${WEAPON_UPGRADE_ABOUT}`));
    const arms = el('div', 'upgrades');
    for (const w of WEAPON_SHOP) {
      const level = s.arsenal[w.id];
      const row = el('div', 'upgrade');
      const pips = el('div', 'pips');
      for (let k = 0; k < MAX_WEAPON_LEVEL; k++) pips.append(el('span', k < level ? 'pip on' : 'pip'));
      const info = el('div', 'up-info');
      info.append(el('strong', undefined, `${w.icon} ${w.name}${level ? ` · ${level}` : ''}`), el('span', 'muted', w.about), pips);
      row.append(info);
      const locked = weaponLockedReason(s, w.id);
      if (locked && !level) row.append(el('span', 'tag', `🔒 ${locked}`));
      else if (level >= MAX_WEAPON_LEVEL) row.append(el('span', 'tag', 'MAX'));
      else {
        const cost = weaponCost(w.id, level);
        const b = button(`${level ? '' : 'Buy '}🪙 ${cost.toLocaleString()}`, 'btn primary', () => buyWeapon(s, w.id) && this.changed());
        b.disabled = s.coins < cost;
        row.append(b);
      }
      arms.append(row);
    }
    body.append(arms);
  }

  private buyUpgrade(id: UpgradeId): void {
    const s = this.save;
    const cost = upgradeCost(s.upgrades[id]);
    if (s.coins < cost || s.upgrades[id] >= upgradeCap(s)) return;
    s.coins -= cost;
    s.upgrades[id]++;
    this.changed();
  }

  /** After buying or choosing something: show it (showCareer tells the game, which saves it). */
  private changed(): void {
    this.showCareer('Garage');
  }

  /** Bars for power, top speed, grip and braking, compared to the car in use (a white tick). */
  private statBars(t: Tuning, compare: Tuning): HTMLElement {
    const max = careerTuning(CHASSIS.length - 1, { ...noUpgrades(), engine: 10, gearbox: 10, tyres: 10, brakes: 10, aero: 10 });
    const bars = el('div', 'stat-bars');
    const speed = (mph: number) => (this.events.units() === 'mph' ? `${Math.round(mph)} mph` : `${Math.round(mph * 1.609)} km/h`);
    const rows: [string, number, number, number, string][] = [
      ['Power', t.horsepower, compare.horsepower, max.horsepower, `${Math.round(t.horsepower)} hp`],
      ['Top speed', t.topSpeed, compare.topSpeed, max.topSpeed, speed(t.topSpeed)],
      ['Grip', t.grip + t.downforce * 0.05, compare.grip + compare.downforce * 0.05, max.grip + max.downforce * 0.05, `${t.grip.toFixed(2)}×`],
      ['Brakes', t.brakes, compare.brakes, max.brakes, `${t.brakes.toFixed(2)}×`],
    ];
    for (const [label, value, other, top, text] of rows) {
      const row = el('div', 'stat');
      const bar = el('div', 'stat-bar');
      const fill = el('div', 'stat-fill');
      fill.style.width = `${Math.min(100, (value / top) * 100)}%`;
      const tick = el('div', 'stat-tick');
      tick.style.left = `${Math.min(100, (other / top) * 100)}%`;
      bar.append(fill, tick);
      row.append(el('span', 'stat-label', label), bar, el('span', 'stat-value', text));
      bars.append(row);
    }
    return bars;
  }

  private dailyTab(body: HTMLElement): void {
    const s = this.save;
    const level = dailyLevel(s);
    const done = s.daily === today();
    const card = el('div', 'daily');
    card.append(
      el('h3', undefined, `${EVENT_KINDS[level.spec.kind].icon} Today: ${EVENT_KINDS[level.spec.kind].label} at ${venue(level.spec.map).label}`),
      el('p', 'muted', 'A new challenge every day, pitched at how far you have got. It pays double, and a streak of days in a row pays up to twice as much again.'),
      el('p', undefined, done ? `✓ Done today. Come back tomorrow! Streak: ${s.streak} day${s.streak === 1 ? '' : 's'}.` : `Streak: ${s.streak} day${s.streak === 1 ? '' : 's'}`),
    );
    card.append(button(done ? 'Play again (no reward)' : 'Play ▶', 'btn primary big', () => this.showLevel(level)));
    body.append(card);
  }

  /** Every trophy: the ones won, and how far along the others are. */
  private trophiesTab(body: HTMLElement): void {
    const s = this.save;
    body.append(el('p', 'muted', `${s.trophies.length} of ${TROPHIES.length} won. Each one pays coins.`));
    const grid = el('div', 'trophies');
    // Won ones first, then the closest to being won.
    const sorted = [...TROPHIES].sort((a, b) => {
      const wa = s.trophies.includes(a.id) ? 2 : Math.min(1, a.value(s) / a.need);
      const wb = s.trophies.includes(b.id) ? 2 : Math.min(1, b.value(s) / b.need);
      return wb - wa;
    });
    for (const t of sorted) {
      const won = s.trophies.includes(t.id);
      const tile = el('div', `trophy${won ? ' won' : ''}`);
      const bar = el('div', 'bar');
      const fill = el('div', 'fill');
      const value = Math.min(t.need, Math.floor(t.value(s)));
      fill.style.width = `${Math.round((value / t.need) * 100)}%`;
      bar.append(fill);
      tile.append(
        el('span', 'trophy-icon', t.icon),
        el('span', 'trophy-name', t.name),
        el('span', 'trophy-about', t.about),
        bar,
        el('span', 'trophy-reward', won ? `Won · 🪙 ${t.reward.toLocaleString()}` : `${value.toLocaleString()} / ${t.need.toLocaleString()} · 🪙 ${t.reward.toLocaleString()}`),
      );
      grid.append(tile);
    }
    body.append(grid);
  }

  private statsTab(body: HTMLElement): void {
    const s = this.save;
    const facts = el('dl', 'facts');
    const fact = (k: string, v: string) => facts.append(el('dt', undefined, k), el('dd', undefined, v));
    const stat = (key: string) => Math.round(s.stats[key] ?? 0).toLocaleString();
    fact('Driver level', `${driverLevel(s.xp).level} (${s.xp.toLocaleString()} XP)`);
    fact('Levels cleared', `${s.stars.filter((x) => x > 0).length} / ${LEVELS}`);
    fact('Stars', `${totalStars(s)} / ${LEVELS * 3}`);
    fact('Events', s.events.toLocaleString());
    fact('Wins', s.wins.toLocaleString());
    fact('Coins earned', s.earned.toLocaleString());
    fact('Cars owned', `${s.owned.length} / ${CHASSIS.length}`);
    fact('Careers finished', String(s.completions));
    if (s.sandboxBest) fact('Best sandbox level', String(s.sandboxBest));
    fact('Best skill chain', stat('bestChain'));
    fact('Near misses', stat('nearMiss'));
    fact('Overtakes', stat('overtake'));
    fact('Takedowns', stat('takedown'));
    fact('Perfect starts', stat('perfectStart'));
    fact('Drift boosts', stat('driftBoost'));
    fact('Seconds in a slipstream', stat('draft'));
    fact('Seconds in the air', stat('air'));
    fact('Top speed in an event', `${stat('topSpeed')} mph`);
    fact('Gold wrenches', `${s.found.length} / ${WRENCHES}`);
    fact('Free-roam challenge stars', `${Object.values(s.zones).reduce((a, b) => a + b, 0)} / ${ZONE_STARS}`);
    const nemesis = Object.entries(s.nemesis).sort((a, b) => b[1] - a[1])[0];
    if (nemesis && nemesis[1] >= 2) fact('Nemesis', `${nemesis[0]} (beat you ${nemesis[1]} times)`);
    body.append(facts);
    body.append(el('h3', undefined, 'Start again'), el('p', 'muted', 'Resetting wipes your coins, cars, upgrades, weapons and stars. If you have finished the career, it counts as a completion: coins pay 25% more for each one.'));
    const reset = button('Reset career…', 'btn danger', () => {
      if (!window.confirm('Reset your career? Coins, cars, upgrades, weapons and stars will be lost.')) return;
      this.save = resetCareer(s, s.stars[LEVELS - 1] > 0); // counts as a completion once level 100 is beaten
      this.showCareer('Levels', 1);
    });
    body.append(reset);
  }

  // ---------- quick race ----------

  showQuick(): void {
    const q = this.quick;
    const head = el('header', 'career-head');
    head.append(button('◀', 'btn back', () => this.showMain()));
    const titles = el('div', 'career-titles');
    titles.append(el('h2', undefined, '🏁 Quick race'));
    head.append(titles);
    const form = el('div', 'quick');
    const choice = <T extends string | number>(label: string, options: { value: T; label: string }[], get: () => T, set: (v: T) => void) => {
      const row = el('div', 'q-row');
      row.append(el('label', undefined, label));
      const opts = el('div', 'choices');
      for (const o of options) {
        const b = button(o.label, `choice${get() === o.value ? ' active' : ''}`, () => {
          set(o.value);
          this.showQuick();
        });
        opts.append(b);
      }
      row.append(opts);
      form.append(row);
    };
    choice('Event', KIND_ORDER.map((k) => ({ value: k, label: `${EVENT_KINDS[k].icon} ${EVENT_KINDS[k].label}` })), () => q.kind, (v) => {
      q.kind = v;
      if ((v === 'destruction' || v === 'stunt') !== (q.map === 'city')) q.map = v === 'destruction' || v === 'stunt' ? 'city' : 'circuit';
    });
    form.append(el('p', 'muted', EVENT_KINDS[q.kind].rules));
    const city = q.kind === 'destruction' || q.kind === 'stunt';
    const maps = VENUES.filter((v) => (city ? v.value === 'city' : v.value !== 'city'));
    choice('Venue', maps.map((v) => ({ value: v.value, label: v.label })), () => q.map, (v) => (q.map = v));
    if (!city) {
      const row = el('div', 'q-row');
      row.append(el('label', undefined, 'Track'));
      const opts = el('div', 'choices');
      opts.append(button('Main track', `choice${q.layout === 0 ? ' active' : ''}`, () => {
        q.layout = 0;
        this.showQuick();
      }));
      opts.append(button(q.layout ? `Track #${q.layout} 🎲` : 'New random track 🎲', `choice${q.layout ? ' active' : ''}`, () => {
        q.layout = 1 + Math.floor(Math.random() * 9999);
        this.showQuick();
      }));
      row.append(opts);
      form.append(row);
    }
    if (['race', 'duel', 'timeAttack', 'speedTrap'].includes(q.kind)) {
      choice('Laps', [1, 2, 3, 4, 5, 7, 10].map((n) => ({ value: n, label: String(n) })), () => q.laps, (v) => (q.laps = v));
    }
    if (['race', 'elimination', 'takedown'].includes(q.kind)) {
      choice('Rivals', [1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => ({ value: n, label: String(n) })), () => q.rivals, (v) => (q.rivals = v));
    }
    if (!city && q.kind !== 'drift') {
      choice('Difficulty', DIFFICULTIES.map((d, i) => ({ value: i, label: d.label })), () => q.difficulty, (v) => (q.difficulty = v));
    }
    const cars = this.quickCars();
    choice('Your car', cars.map((c) => ({ value: c.id, label: c.label })), () => q.car, (v) => (q.car = v));
    choice('Time of day', [{ value: 'day', label: '☀️ Day' }, { value: 'night', label: '🌙 Night' }], () => (q.night ? 'night' : 'day'), (v) => (q.night = v === 'night'));
    if (q.kind === 'race' || q.kind === 'duel' || q.kind === 'elimination') {
      choice('Weapons', [{ value: 'off', label: 'Off' }, { value: 'on', label: '🔫 On (combat race)' }], () => (q.weapons ? 'on' : 'off'), (v) => (q.weapons = v === 'on'));
    }
    form.append(el('p', 'muted', 'Rivals drive cars as quick as yours, so any car makes a close race. Difficulty sets how well they drive.'));
    const actions = el('div', 'actions');
    actions.append(button('Start ▶', 'btn primary big', () => {
      const car = (cars.find((c) => c.id === q.car) ?? cars[0]).config();
      const spec = quickRaceSpec({ ...q, car, layout: city ? 0 : q.layout });
      this.events.playQuick(spec, car);
    }));
    this.open('career', head, form, actions);
  }

  /** Cars for a quick race: the garage build, every career car (stock and fully upgraded), and the showcase builds. */
  private quickCars(): { id: string; label: string; config: () => CarConfig }[] {
    const maxed = { ...noUpgrades(), engine: 10, gearbox: 10, tyres: 10, brakes: 10, nitro: 10, aero: 10, armour: 10 };
    return [
      { id: 'garage', label: '🔧 My garage build', config: () => ({ ...this.looks, tuning: { ...this.looks.tuning } }) },
      { id: 'career', label: `🏆 My career car`, config: () => careerCar(this.looks, this.save.car, this.save.upgrades) },
      ...CHASSIS.flatMap((c, i) => [
        { id: `stock${i}`, label: c.name, config: () => careerCar(this.looks, i, noUpgrades()) },
        { id: `max${i}`, label: `${c.name} (maxed)`, config: () => careerCar(this.looks, i, maxed) },
      ]),
      ...SHOWCASE_BUILDS.map((b, i) => ({ id: `show${i}`, label: `⭐ ${b.name}`, config: () => showcaseConfig(b.changes) })),
    ];
  }

  // ---------- over the game ----------

  showPause(title: string): void {
    const box = el('div', 'overlay-box');
    box.append(el('h2', undefined, 'Paused'), el('p', 'muted', title));
    const actions = el('div', 'actions column');
    actions.append(
      button('Resume', 'btn primary big', () => this.events.resume()),
      button('Restart', 'btn big', () => this.events.restart()),
      button('Quit to menu', 'btn big', () => this.events.quit()),
    );
    box.append(actions);
    this.open('overlay', box);
  }

  /**
   * The results: stars, score, the finishing order and coins. `next` is the
   * next career level, if there is one to go on to.
   */
  showResults(spec: EventSpec, result: EventResult, payout: Payout | null, next: Level | null, extras: string[] = []): void {
    const box = el('div', 'overlay-box results');
    const won = result.position === 1 || (result.position === 0 && result.stars === 3);
    const heading = result.passed ? (won ? (spec.boss ? `${spec.boss} defeated!` : 'Victory!') : 'Finished!') : spec.kind === 'elimination' && result.position > 0 ? 'Knocked out!' : 'Not this time';
    box.append(el('h2', won ? 'gold' : undefined, heading), el('p', 'muted', spec.title));
    const stars = el('div', 'big-stars');
    for (let k = 0; k < 3; k++) {
      const star = el('span', k < result.stars ? 'star on' : 'star', '★');
      star.style.animationDelay = `${0.25 + k * 0.3}s`;
      stars.append(star);
    }
    box.append(stars, el('p', 'result-score', result.scoreText));
    if (result.bestLap) box.append(el('p', 'muted', `Best lap ${formatTime(result.bestLap)}`));
    if (spec.kind !== 'race' && spec.kind !== 'elimination' && spec.kind !== 'duel') {
      box.append(el('p', 'muted', `Targets: ${this.targetsText(spec, true)}`));
    }
    if (result.standings.length > 1) {
      const list = el('ol', 'standings-list');
      for (const r of result.standings) list.append(el('li', r.me ? 'me' : undefined, `${r.name} · ${r.text}`));
      box.append(list);
    }
    if (payout) {
      const coins = el('div', 'payout');
      coins.append(el('strong', undefined, `+🪙 ${payout.coins.toLocaleString()}`));
      for (const line of payout.lines) coins.append(el('span', undefined, line));
      coins.append(el('span', 'muted', `Balance: 🪙 ${this.save.coins.toLocaleString()}`));
      box.append(coins);
    }
    if (extras.length) {
      const more = el('div', 'payout extras');
      for (const line of extras) more.append(el('span', undefined, line));
      box.append(more);
    }
    const actions = el('div', 'actions');
    if (next) actions.append(button(`Next: level ${next.number} ▶`, 'btn primary big', () => this.events.playLevel(next)));
    actions.append(button('Retry', next ? 'btn big' : 'btn primary big', () => this.events.restart()));
    if (payout) actions.append(button('Upgrades', 'btn big', () => {
      this.events.quit();
      this.showCareer('Garage');
    }));
    actions.append(button('Menu', 'btn big', () => this.events.quit()));
    box.append(actions);
    this.open('overlay', box);
  }

  /** Shown once level 100 is beaten: carry on into the sandbox, or start again. */
  showCareerComplete(): void {
    const box = el('div', 'overlay-box results');
    box.append(
      el('h2', 'gold', '🏆 CAREER COMPLETE! 🏆'),
      el('p', undefined, `You beat The Phantom and all ${LEVELS} levels. Legend!`),
      el('p', 'muted', 'What now?'),
    );
    const choices = el('div', 'end-choices');
    const sandbox = this.bigButton('∞ Sandbox', 'Keep your car, coins and upgrades. Endless levels that get harder every time. Upgrades go up to 20.', () => this.events.chooseSandbox());
    sandbox.classList.add('primary');
    const reset = this.bigButton('↺ Reset', `Start again from level 1 with nothing, but every coin pays ${Math.round((prestige(this.save) + 0.25 - 1) * 100)}% more.`, () => this.events.chooseReset());
    choices.append(sandbox, reset);
    box.append(choices);
    this.open('overlay', box);
  }
}

/** "Hot Hatch", "Muscle Car"... for the car a rival drives. */
function rivalCarName(config: CarConfig): string {
  const c = CHASSIS.find((x) => x.body === config.body);
  if (!c) return 'cars';
  const name = c.name.toLowerCase();
  return name.endsWith('h') ? `${name}es` : `${name}s`;
}
