// One event: a race against rivals, or a challenge against the clock. It
// places the cars, counts down, runs the rivals, keeps track of everyone's
// progress round the lap, and works out the result and stars at the end.
//
// The kinds of event:
//   race         Most laps first. Stars for 1st, 2nd and 3rd.
//   elimination  Every so often the car in last place is knocked out.
//   duel         One on one against a chapter's boss. Win it, or nothing.
//   timeAttack   Alone, against target times set by a pace car.
//   takedown     Weapons on. Wreck as many rivals as you can; they shoot back.
//   drift        Drift points before the time runs out.
//   speedTrap    Speed cameras on the fastest parts of the lap; add up your speeds.
//   destruction  In the city: do as much damage as you can.
//   stunt        In the city's stunt park: as much time in the air as you can.
//
// The app runs the player's car as usual; this runs everything else and is
// told about drifts, jumps and damage the player scores.
import * as THREE from 'three';
import type { CarConfig } from './carConfig';
import type { CarPhysics } from './carPhysics';
import type { Effects } from './effects';
import type { EngineAudio } from './engineAudio';
import type { RaceTrack } from './raceTrack';
import { Rival, type RacerPosition, type RivalSpec } from './rivals';
import type { SkillKind } from './skills';
import { formatTime } from './timeTrial';
import type { MapName } from './venues';
import type { HitBox, World } from './world';

export type EventKind = 'race' | 'elimination' | 'duel' | 'timeAttack' | 'takedown' | 'drift' | 'speedTrap' | 'destruction' | 'stunt';

/** What each kind of event is called, and the one-line rules. */
export const EVENT_KINDS: Record<EventKind, { label: string; rules: string; icon: string }> = {
  race: { label: 'Race', rules: 'Finish in the top 3. 1st place for 3 stars.', icon: '🏁' },
  elimination: { label: 'Elimination', rules: 'The car in last place is knocked out every few seconds. Be the last one left.', icon: '⏳' },
  duel: { label: 'Boss duel', rules: 'One on one against the boss. Beat them to move on.', icon: '👑' },
  timeAttack: { label: 'Time attack', rules: 'Just you and the clock. Beat the target times.', icon: '⏱️' },
  takedown: { label: 'Takedown', rules: 'Weapons on. Wreck rivals with guns, rockets and rams. They shoot back!', icon: '💥' },
  drift: { label: 'Drift', rules: 'Score drift points before the time runs out. Crashing loses the drift.', icon: '🌀' },
  speedTrap: { label: 'Speed trap', rules: 'Hit the speed cameras as fast as you can. Your speeds are added up.', icon: '📸' },
  destruction: { label: 'Destruction', rules: 'Weapons on. Do as much damage to the city as you can.', icon: '🏚️' },
  stunt: { label: 'Stunt show', rules: 'Use the ramps. Every second in the air counts.', icon: '🚀' },
};

/** Everything about one event. Career levels and quick races both make these. */
export interface EventSpec {
  kind: EventKind;
  title: string;
  map: MapName;
  /** Which track at the venue: 0 for its main track, anything else for a generated one. */
  layout: number;
  laps: number;
  /** Seconds: the time limit, or for an elimination, how often someone is knocked out. */
  timeLimit: number;
  rivals: RivalSpec[];
  /** The car and skill that set the target times and speeds for time attacks and speed traps. */
  pace?: { config: CarConfig; skill: number };
  /**
   * What 1, 2 and 3 stars need. Finishing places for races (3rd, 2nd, 1st),
   * seconds for a time attack, and points (takedowns, drift points, mph,
   * dollars, seconds in the air) for the rest. Time attacks and speed traps
   * work theirs out from the pace car when the event starts.
   */
  targets: [number, number, number];
  night: boolean;
  weapons: boolean;
  noNitro: boolean;
  /** How hard the rivals catch up when behind and ease off when ahead, 0..1. */
  catchUp: number;
  /** How much damage the player's car can take in a takedown (100 is normal). */
  armour: number;
  /** How much damage each rival can take from weapons and rams. */
  rivalArmour: number;
  /** How hard the rivals' bullets hit, per hit. */
  rivalDamage: number;
  /** The boss's name, for duels. */
  boss?: string;
}

export interface EventResult {
  stars: number;
  /** Finishing place in races (1 is first), or 0 for challenges. */
  position: number;
  /** The score as shown: a place, a time, or points. */
  scoreText: string;
  /** Raw score: place, seconds or points. */
  score: number;
  /** Did the player finish (or for challenges, get at least one star)? */
  passed: boolean;
  /** The final order, for races. */
  standings: { name: string; text: string; me: boolean }[];
  /** Best lap, in seconds (0 if no full lap). */
  bestLap: number;
}

/** What to show on the HUD. */
export interface EventHud {
  /** Big numbers along the top: position, lap, time, score. */
  stats: { label: string; value: string }[];
  /** The running order down the side (races only). */
  standings: { name: string; gap: string; me: boolean; out: boolean }[];
  /** A warning in the middle of the screen, or ''. */
  warning: string;
  /** Player's health, 0..1, in events with weapons; null otherwise. */
  health: number | null;
  /** How the score compares to the star targets, for the star meter. */
  stars: number;
}

const COUNTDOWN = 3.5;
/** Progress jumps by more than this (meters) in one step mean the car has left the track: cutting across doesn't count. */
const MAX_JUMP = 40;
/**
 * How far off the edge of the road (meters) a car can go and still be on
 * course. Tracks with wide run-off areas or an apron (the speedway) allow
 * up to 2 m past those instead.
 */
const OFF_COURSE = 6;
const RESPAWN_TIME = 5; // seconds a wrecked rival burns before coming back
const MPH = 2.237;

/** Where one car (the player's or a rival's) is in the event. */
interface Racer {
  name: string;
  physics: CarPhysics;
  rival: Rival | null;
  /** Meters round the lap (0..length) the last time it was on the track. */
  along: number;
  offset: number;
  /** Meters driven round the track since the start line; starts negative on the grid. */
  distance: number;
  /** Seconds since GO when it finished, or null. */
  finished: number | null;
  /** Knocked out of an elimination (the order: 1 for the first one out). */
  outOrder: number;
  /** Seconds spent away from the track, or going the wrong way. */
  lost: number;
  wrongWay: number;
  /** Hit points in a takedown, and the hit box for the player's weapons. */
  hp: number;
  hitBox: HitBox | null;
  /** Rivals' guns: seconds to the next shot, and shots left in the burst. */
  gunWait: number;
  burst: number;
  lapStart: number;
}

export class EventRun {
  state: 'countdown' | 'running' | 'done' = 'countdown';
  countdown = COUNTDOWN;
  /** Seconds since GO. */
  clock = 0;
  /** Points, dollars or seconds in the air: the score in challenges. */
  score = 0;
  result: EventResult | null = null;
  /** Messages for the app to show (one-off toasts). */
  readonly notes: string[] = [];
  /** Skills the player pulled off (near misses, overtakes, takedowns), for the app's skill chain. */
  readonly skills: { kind: SkillKind; points: number; label?: string }[] = [];
  /** Lines the announcer says (the app speaks them if the announcer is on). */
  readonly callouts: string[] = [];
  /** The player's place last frame, to spot overtakes. */
  private lastPlace = 0;
  /** When each rival was last near-missed (by clock), so one pass counts once. */
  private nearMissAt = new Map<Racer, number>();
  private finalLapCalled = false;
  /** The player's car is wrecked: seconds until it's put back on the track. */
  playerWrecked = 0;
  /** Points of the drift going on now, not yet banked: they still count if the time runs out mid-drift. */
  unbankedDrift = 0;
  /** Seconds of the jump going on now: they still count in a stunt show if the time runs out mid-air. */
  unbankedAir = 0;
  /** The furthest the player has got (meters round the track): each speed camera counts once per lap. */
  private trapMark = -Infinity;
  readonly track: RaceTrack | null;
  readonly racers: Racer[] = [];
  private me: Racer;
  private outCount = 0;
  private nextOut: number;
  private bestLap = 0;
  /** Speed cameras: meters along the lap, and the meshes. */
  readonly traps: number[] = [];
  private trapGroup = new THREE.Group();
  private takedowns = 0;
  private firstFinish: number | null = null;

  constructor(
    readonly spec: EventSpec,
    scene: THREE.Scene,
    private world: World,
    private effects: Effects,
    private audio: EngineAudio,
    player: CarPhysics,
    night: boolean,
  ) {
    this.track = world.track;
    this.nextOut = spec.timeLimit;
    this.me = this.makeRacer('You', player, null);
    this.racers.push(this.me);
    for (const r of spec.rivals) {
      const rival = new Rival(r, scene, night);
      rival.useNitro = !spec.noNitro;
      this.racers.push(this.makeRacer(r.name, rival.physics, rival));
    }
    this.placeAll();
    if (this.track && (spec.kind === 'timeAttack' || spec.kind === 'speedTrap')) this.setTargetsFromPaceCar();
    if (spec.weapons) {
      world.hitBoxes = this.racers.filter((r) => r.hitBox).map((r) => r.hitBox!);
    }
    scene.add(this.trapGroup);
  }

  private makeRacer(name: string, physics: CarPhysics, rival: Rival | null): Racer {
    const racer: Racer = {
      name, physics, rival, along: 0, offset: 0, distance: 0, finished: null, outOrder: 0, lost: 0, wrongWay: 0,
      hp: 100, hitBox: null, gunWait: 2 + Math.random() * 3, burst: 0, lapStart: 0,
    };
    if (rival && this.spec.weapons) {
      racer.hitBox = {
        x: 0, y: 0, z: 0, half: Math.max(physics.length, physics.width) * 0.4, height: physics.height, alive: true,
        onHit: (damage, direction) => this.hurtRival(racer, damage, direction),
      };
    }
    return racer;
  }

  get isRace(): boolean {
    return this.spec.kind === 'race' || this.spec.kind === 'elimination' || this.spec.kind === 'duel';
  }

  /** Whether the event counts laps (rather than just a clock). */
  private get usesLaps(): boolean {
    return (this.isRace && this.spec.kind !== 'elimination') || this.spec.kind === 'timeAttack' || this.spec.kind === 'speedTrap';
  }

  // ---------- the start ----------

  /** Lines everyone up on the grid, two by two, with the player at the back. */
  private placeAll(): void {
    const track = this.track;
    if (!track) {
      const s = this.world.spawn;
      this.me.physics.reset(s.x, s.z, s.heading, true);
      return;
    }
    const edge = track.style.width / 2;
    // Rivals first (the fastest at the front), then the player.
    const order = [...this.racers.slice(1), this.me];
    order.forEach((r, i) => {
      const along = -10 - Math.floor(i / 2) * 9 - (i % 2) * 4;
      const offset = order.length === 1 ? 0 : (i % 2 === 0 ? 1 : -1) * edge * 0.4;
      this.placeOnTrack(r, along, offset, true);
      r.distance = along;
      r.along = ((along % track.length) + track.length) % track.length;
    });
    if (this.spec.kind === 'speedTrap') {
      this.buildTraps();
      this.trapMark = this.me.distance;
    }
  }

  /** `fresh`: lining up for a new start, rather than putting a car back on the track. */
  private placeOnTrack(r: Racer, along: number, offset: number, fresh = false): void {
    const track = this.track!;
    const p = track.pose(along, offset);
    r.physics.reset(p.x, p.z, p.heading, fresh);
    r.physics.y = this.world.heightAt(p.x, p.z);
    r.offset = offset;
  }

  /** How many of the five start lights are on. */
  get startLights(): number {
    if (this.state !== 'countdown' || !this.track) return 0;
    return Math.min(5, Math.floor(((COUNTDOWN - this.countdown) / COUNTDOWN) * 6));
  }

  /**
   * Drives a steady pace car round the track, unseen, to set the targets:
   * its time for a time attack and its speeds through the cameras for a speed
   * trap. 3 stars needs the pace car's result; 2 and 1 stars a little less.
   */
  private setTargetsFromPaceCar(): void {
    const track = this.track!;
    const pace = this.spec.pace;
    if (!pace) return;
    const car = new Rival({ name: 'Pace car', config: pace.config, skill: pace.skill, steady: true }, null, false);
    car.useNitro = !this.spec.noNitro;
    const racer = this.makeRacer('Pace car', car.physics, car);
    this.placeOnTrack(racer, -10, 0);
    racer.distance = -10;
    racer.along = track.length - 10;
    const goal = this.spec.laps * track.length;
    const step = 1 / 120;
    let time = 0;
    let trapSpeed = 0;
    let furthest = racer.distance;
    while (racer.distance < goal && time < 900) {
      car.drive(step, this.world, track, racer, [], true);
      this.updateProgress(racer, step);
      if (this.spec.kind === 'speedTrap' && racer.distance > furthest) {
        trapSpeed += this.trapsCrossed(furthest, racer.distance) * car.physics.speed * MPH;
        furthest = racer.distance;
      }
      time += step;
    }
    car.dispose();
    // The pace car may have knocked over a tree or lamp; put everything back.
    this.world.rebuildAll(this.me.physics.x, this.me.physics.z, 3);
    if (this.spec.kind === 'timeAttack') {
      const t = Math.ceil(time * 10) / 10;
      this.spec.targets = [Math.ceil(t * 1.14), Math.ceil(t * 1.06), t];
    } else {
      const s = Math.floor(trapSpeed);
      this.spec.targets = [Math.floor(s * 0.86), Math.floor(s * 0.94), s];
    }
  }

  /** Speed cameras at the end of the three longest straights. */
  private buildTraps(): void {
    const track = this.track!;
    const n = track.points.length;
    // Find the straight stretches: runs of samples with a gentle curve.
    const runs: { start: number; length: number }[] = [];
    let start = -1;
    // Start the scan just after a corner, so no run is cut in two by the start line.
    let first = 0;
    while (first < n && Math.abs(track.points[first].curve) < 1 / 300) first++;
    for (let k = 0; k <= n; k++) {
      const i = (first + k) % n;
      const straight = k < n && Math.abs(track.points[i].curve) < 1 / 300;
      if (straight && start < 0) start = k;
      if (!straight && start >= 0) {
        runs.push({ start: first + start, length: k - start });
        start = -1;
      }
    }
    runs.sort((a, b) => b.length - a.length);
    for (const run of runs.slice(0, 3)) {
      const i = (run.start + Math.floor(run.length * 0.8)) % n;
      this.traps.push(track.points[i].along);
    }
    // A gantry over the road at each camera.
    const post = new THREE.MeshStandardMaterial({ color: 0x30343a, metalness: 0.6, roughness: 0.4 });
    const glow = new THREE.MeshStandardMaterial({ color: 0xffd23f, emissive: 0xffb300, emissiveIntensity: 1.4 });
    const half = track.style.width / 2 + 1;
    for (const along of this.traps) {
      const p = track.pose(along);
      const g = new THREE.Group();
      for (const side of [-1, 1]) {
        const pole = new THREE.Mesh(new THREE.BoxGeometry(0.3, 6, 0.3), post);
        pole.position.set(side * half, 3, 0);
        g.add(pole);
      }
      const beam = new THREE.Mesh(new THREE.BoxGeometry(half * 2, 0.5, 0.5), glow);
      beam.position.y = 6;
      g.add(beam);
      g.position.set(p.x, p.y, p.z);
      g.rotation.y = p.heading;
      this.trapGroup.add(g);
    }
  }

  /** How many speed cameras a car passed going from `from` to `to` meters. */
  private trapsCrossed(from: number, to: number): number {
    if (!this.track || to <= from) return 0;
    const L = this.track.length;
    let count = 0;
    for (const t of this.traps) {
      // Is there a lap k where from < t + kL <= to?
      const k = Math.ceil((from - t) / L + 1e-9);
      if (t + k * L <= to && t + k * L > from && t + k * L >= 0) count++;
    }
    return count;
  }

  // ---------- each step ----------

  /**
   * Runs one physics step for the rivals (the player's car has just had its
   * own), then car-to-car crashes and progress. Returns how hard the player's
   * car was hit by another car, in m/s.
   */
  step(dt: number): number {
    if (this.state === 'done') {
      // The rivals carry on lapping behind the results.
      if (this.track) for (const r of this.racers) r.rival?.drive(dt, this.world, this.track, r, [], true);
      return 0;
    }
    const racing = this.state === 'running';
    const track = this.track;
    if (track) {
      const positions: RacerPosition[] = this.racers.filter((r) => !r.rival || r.rival.active);
      for (const r of this.racers) {
        if (!r.rival) continue;
        r.rival.pace = this.paceFor(r);
        r.rival.drive(dt, this.world, track, r, positions.filter((o) => o !== r), racing);
        if (r.rival.wrecked > 0) {
          r.rival.wrecked -= dt;
          if (r.rival.wrecked <= 0) this.respawnRival(r);
        }
      }
    }
    const impact = this.collideCars();
    this.updateDrafts(dt);
    if (!racing) return impact;
    if (impact === 0) this.nearMisses();
    for (const r of this.racers) {
      if (track) this.updateProgress(r, dt);
      // A camera only counts the first time past it each lap: only progress
      // beyond the furthest point reached so far can cross one.
      if (r === this.me && this.spec.kind === 'speedTrap' && r.distance > this.trapMark) {
        const crossed = this.trapsCrossed(this.trapMark, r.distance);
        this.trapMark = r.distance;
        if (crossed > 0) {
          const mph = Math.round(r.physics.speed * MPH);
          this.score += mph * crossed;
          this.notes.push(`📸 ${mph} mph`);
          this.audio.chime();
        }
      }
    }
    return impact;
  }

  /**
   * Slipstream: a car tucked in close behind another one, going the same
   * way, has less air to push through. Works for everyone, so packs form
   * and the car behind can slingshot past.
   */
  private updateDrafts(dt: number): void {
    const cars = this.racers.filter((r) => (!r.rival || (r.rival.active && r.rival.wrecked <= 0)));
    for (const a of cars) {
      const p = a.physics;
      let target = 0;
      if (p.speed > 15 && !p.airborne) {
        const fx = Math.sin(p.heading);
        const fz = Math.cos(p.heading);
        for (const b of cars) {
          if (b === a) continue;
          const q = b.physics;
          const dx = q.x - p.x;
          const dz = q.z - p.z;
          const ahead = dx * fx + dz * fz;
          const side = Math.abs(dx * fz - dz * fx);
          const sameWay = Math.sin(q.heading) * fx + Math.cos(q.heading) * fz;
          if (ahead > 3 && ahead < 32 && side < 2.4 && sameWay > 0.9) target = Math.max(target, 1 - (ahead - 3) / 29);
        }
      }
      p.draft += (target - p.draft) * Math.min(1, dt * 4);
    }
  }

  /** Squeezing past a rival at speed without touching: a near miss. */
  private nearMisses(): void {
    const me = this.me.physics;
    for (const r of this.racers) {
      if (!r.rival || !r.rival.active || r.rival.wrecked > 0) continue;
      const q = r.physics;
      const gap = Math.hypot(q.x - me.x, q.z - me.z) - (me.width + q.width) / 2;
      const closing = Math.hypot(me.vx - q.vx, me.vz - q.vz);
      if (gap > 0.15 && gap < 1.4 && me.speed > 18 && closing > 4 && Math.abs(q.y - me.y) < 1.5) {
        const last = this.nearMissAt.get(r) ?? -Infinity;
        if (this.clock - last > 2.5) this.skills.push({ kind: 'nearMiss', points: 100 + me.speed * 3 });
        this.nearMissAt.set(r, this.clock);
      }
    }
  }

  /** The rivals' catch-up: ease off when far ahead of the player, push harder when far behind. */
  private paceFor(r: Racer): number {
    if (!this.isRace || this.spec.catchUp <= 0) return 1;
    const gap = r.distance - this.me.distance; // positive: ahead of the player
    const amount = Math.max(-1, Math.min(1, gap / 300));
    return 1 - amount * (amount > 0 ? 0.1 : 0.05) * this.spec.catchUp;
  }

  /**
   * Moves a car's place round the lap on. Progress only counts on the road
   * (or just off it): a car out on the grass, or on another part of the track
   * (a big jump round the lap), is off course, and progress only carries on
   * once it's back where it left.
   */
  private updateProgress(r: Racer, dt: number): void {
    const track = this.track!;
    const p = r.physics;
    const at = track.locate(p.x, p.z);
    // Where it is across the road, even off course: the rivals use it to tell when they're lost.
    r.offset = at ? at.offset : 999;
    let moved = Infinity;
    if (at) {
      moved = at.along - r.along;
      if (moved > track.length / 2) moved -= track.length;
      if (moved < -track.length / 2) moved += track.length;
    }
    const st = track.style;
    const room = Math.max(OFF_COURSE, st.outsideRunoff + 2, (st.insideRunoff ?? 0) + 2, st.apron + 2);
    const onCourse = at !== null && Math.abs(at.offset) <= st.width / 2 + room;
    if (at && onCourse && Math.abs(moved) < MAX_JUMP) {
      r.along = at.along;
      const before = r.distance;
      r.distance += moved;
      r.lost = 0;
      // A new lap?
      if (r === this.me && Math.floor(before / track.length) < Math.floor(r.distance / track.length) && r.distance > 1) {
        const lapTime = this.clock - r.lapStart;
        r.lapStart = this.clock;
        if (r.distance > track.length) {
          if (!this.bestLap || lapTime < this.bestLap) this.bestLap = lapTime;
          if (this.usesLaps && r.distance < this.spec.laps * track.length) this.notes.push(`Lap ${formatTime(lapTime)}`);
        }
      }
    } else {
      r.lost += dt;
    }
    // Going the wrong way: the car points against the track's direction.
    const point = track.pointAt(r.along);
    const facing = Math.sin(p.heading) * point.dx + Math.cos(p.heading) * point.dz;
    r.wrongWay = facing < -0.3 && p.speed > 3 ? r.wrongWay + dt : 0;
    if (this.usesLaps && r.finished === null && r.distance >= this.spec.laps * track.length) {
      r.finished = this.clock;
      this.firstFinish ??= this.clock;
    }
  }

  /**
   * Car-to-car crashes. Each car is two circles, front and back. Cars that
   * touch are pushed apart and swap some speed, like snooker balls.
   */
  private collideCars(): number {
    let playerImpact = 0;
    const cars = this.racers.filter((r) => !r.rival || r.rival.active);
    const circles = cars.map((r) => {
      const p = r.physics;
      const fx = Math.sin(p.heading);
      const fz = Math.cos(p.heading);
      const reach = Math.max(0, p.length / 2 - p.width / 2);
      return [
        { x: p.x + fx * reach, z: p.z + fz * reach },
        { x: p.x - fx * reach, z: p.z - fz * reach },
      ];
    });
    for (let i = 0; i < cars.length; i++) {
      for (let j = i + 1; j < cars.length; j++) {
        const a = cars[i].physics;
        const b = cars[j].physics;
        if (Math.abs(a.y - b.y) > Math.min(a.height, b.height)) continue; // one is flying over the other
        if (Math.abs(a.x - b.x) > a.length + b.length || Math.abs(a.z - b.z) > a.length + b.length) continue;
        const reach = (a.width + b.width) / 2;
        for (const ca of circles[i]) {
          for (const cb of circles[j]) {
            let nx = ca.x - cb.x;
            let nz = ca.z - cb.z;
            const d = Math.hypot(nx, nz);
            if (d >= reach) continue;
            if (d < 1e-4) {
              nx = 1;
              nz = 0;
            } else {
              nx /= d;
              nz /= d;
            }
            const push = (reach - d) / 2;
            a.x += nx * push;
            a.z += nz * push;
            b.x -= nx * push;
            b.z -= nz * push;
            // Speed towards each other along the line between them.
            const closing = (a.vx - b.vx) * nx + (a.vz - b.vz) * nz;
            if (closing >= 0) continue;
            const kick = (-closing * 1.3) / 2;
            a.vx += nx * kick;
            a.vz += nz * kick;
            b.vx -= nx * kick;
            b.vz -= nz * kick;
            const impact = -closing;
            if (cars[i] === this.me || cars[j] === this.me) {
              playerImpact = Math.max(playerImpact, impact);
              const other = cars[i] === this.me ? cars[j] : cars[i];
              // Ramming hurts in a takedown: the rival much more than you.
              if (this.spec.kind === 'takedown' && impact > 4 && this.state === 'running') {
                this.hurtRival(other, impact * 7, new THREE.Vector3(-nx, 0, -nz));
                this.hurtPlayer(impact * 1.2);
              }
            }
          }
        }
      }
    }
    return playerImpact;
  }

  // ---------- weapons and wrecks ----------

  private hurtRival(r: Racer, damage: number, direction: THREE.Vector3): void {
    const rival = r.rival;
    if (!rival || rival.wrecked > 0 || !rival.active || this.state !== 'running') return;
    r.hp -= (damage * 100) / this.spec.rivalArmour;
    if (r.hp > 0) return;
    // Wrecked! Blown up and thrown into the air; back after a few seconds.
    rival.wrecked = RESPAWN_TIME;
    rival.rig.setWrecked(true);
    if (r.hitBox) r.hitBox.alive = false;
    const p = rival.physics;
    p.push(direction.x * 6, 9, direction.z * 6);
    this.effects.explosion(new THREE.Vector3(p.x, p.y + 1, p.z), 2);
    this.audio.explosion(1.8, Math.hypot(p.x - this.me.physics.x, p.z - this.me.physics.z));
    if (this.spec.kind === 'takedown') {
      this.takedowns++;
      this.score = this.takedowns;
      this.notes.push(`💥 TAKEDOWN! ${r.name}`);
    } else {
      this.notes.push(`💥 ${r.name} wrecked`);
    }
    this.skills.push({ kind: 'takedown', points: 400, label: `Takedown: ${r.name}` });
    if (Math.random() < 0.5) this.callouts.push('Takedown!');
    this.me.physics.nitro = Math.min(1, this.me.physics.nitro + 0.35);
  }

  private hurtPlayer(damage: number): void {
    if (this.playerWrecked > 0 || this.state !== 'running') return;
    this.me.hp -= (damage * 100) / this.spec.armour;
    if (this.me.hp > 0) return;
    this.me.hp = 0;
    this.playerWrecked = 3;
    const p = this.me.physics;
    this.effects.explosion(new THREE.Vector3(p.x, p.y + 1, p.z), 2.2);
    this.audio.explosion(2, 0);
    p.push(0, 8, 0);
    this.notes.push('💀 WRECKED! Back in 3 seconds');
  }

  /** A wrecked rival comes back on the track, out of sight of the player if it can. */
  private respawnRival(r: Racer): void {
    const rival = r.rival!;
    rival.rig.setWrecked(false);
    r.hp = 100;
    if (r.hitBox) r.hitBox.alive = true;
    const track = this.track!;
    // In a takedown, bring it back near the player (ahead or behind) so the fight goes on.
    const along = this.spec.kind === 'takedown' ? this.me.along + (Math.random() < 0.5 ? 150 : -110) : r.along;
    const offset = (Math.random() - 0.5) * track.style.width * 0.5;
    rival.place(track, this.world, along, offset);
    if (this.spec.kind === 'takedown') {
      r.along = ((along % track.length) + track.length) % track.length;
      r.distance = this.me.distance + (r.along - this.me.along);
    }
  }

  /** Rivals fire bursts from their machine guns when the player is in front of them. */
  private rivalGuns(dt: number): void {
    const me = this.me.physics;
    for (const r of this.racers) {
      const rival = r.rival;
      if (!rival || rival.wrecked > 0 || !rival.active) continue;
      r.gunWait -= dt;
      if (r.gunWait > 0) continue;
      const p = rival.physics;
      const dx = me.x - p.x;
      const dz = me.z - p.z;
      const distance = Math.hypot(dx, dz);
      const facing = (Math.sin(p.heading) * dx + Math.cos(p.heading) * dz) / (distance || 1);
      if (distance > 90 || facing < 0.97 || this.playerWrecked > 0) {
        r.gunWait = 0.3;
        continue;
      }
      if (r.burst <= 0) r.burst = 8; // a new burst of shots
      r.burst--;
      r.gunWait = r.burst > 0 ? 0.11 : 2.5 + Math.random() * 2.5;
      const from = new THREE.Vector3(p.x + Math.sin(p.heading) * p.length * 0.5, p.y + p.height * 0.8, p.z + Math.cos(p.heading) * p.length * 0.5);
      // Closer and more skilful shooters hit more often.
      const hits = Math.random() < rival.skill * (0.75 - distance / 300);
      const miss = hits ? 0 : 2 + Math.random() * 3;
      const to = new THREE.Vector3(me.x + (Math.random() - 0.5) * miss, me.y + 0.7, me.z + (Math.random() - 0.5) * miss);
      this.effects.muzzleFlash(from, to.clone().sub(from).normalize());
      this.effects.tracer(from, to);
      if (distance < 60) this.audio.gunshot();
      if (hits) {
        this.effects.bulletHit(to, false);
        this.hurtPlayer(this.spec.rivalDamage);
      }
    }
  }

  // ---------- each frame ----------

  /** The countdown, timers, and the end of the event. `dt` is real time. */
  update(dt: number): void {
    if (this.state === 'countdown') {
      const before = Math.ceil(this.countdown);
      this.countdown -= dt;
      if (Math.ceil(this.countdown) < before && this.countdown > 0 && before <= 4) {
        this.audio.chime();
        this.callouts.push(String(Math.ceil(this.countdown)));
      }
      if (this.countdown <= 0) {
        this.state = 'running';
        this.audio.chime(true);
        this.notes.push('GO!');
        this.callouts.push('Go!');
      }
      return;
    }
    if (this.state !== 'running') return;
    this.clock += dt;
    if (this.spec.weapons && this.track) this.rivalGuns(dt);
    if (this.playerWrecked > 0) {
      this.playerWrecked -= dt;
      if (this.playerWrecked <= 0) {
        this.me.hp = 100;
        this.recover();
      }
    }
    for (const r of this.racers) {
      if (r.hitBox) {
        const p = r.physics;
        r.hitBox.x = p.x;
        r.hitBox.y = p.y;
        r.hitBox.z = p.z;
      }
    }
    if (this.spec.kind === 'elimination') this.eliminate();
    this.watchPlaces();
    this.checkFinished();
  }

  /** Overtakes (for the skill chain) and the final-lap call. */
  private watchPlaces(): void {
    const track = this.track;
    if (this.isRace) {
      const place = this.order().indexOf(this.me) + 1;
      if (this.lastPlace && place < this.lastPlace && this.me.outOrder === 0) {
        this.skills.push({ kind: 'overtake', points: 250 * (this.lastPlace - place), label: `Overtake: P${place}` });
        if (place === 1) this.callouts.push('You take the lead!');
      }
      this.lastPlace = place;
    }
    if (track && this.usesLaps && this.spec.laps > 1 && !this.finalLapCalled && this.me.distance > (this.spec.laps - 1) * track.length) {
      this.finalLapCalled = true;
      this.callouts.push('Final lap!');
      this.notes.push('🏁 FINAL LAP');
    }
  }

  /** Every `timeLimit` seconds, knocks out whoever is last. */
  private eliminate(): void {
    if (this.clock < this.nextOut) return;
    this.nextOut += this.spec.timeLimit;
    const order = this.order().filter((r) => !r.outOrder);
    const last = order[order.length - 1];
    if (!last) return;
    last.outOrder = ++this.outCount;
    if (last === this.me) return; // checkFinished ends it
    this.notes.push(`❌ ${last.name} is out!`);
    this.callouts.push(`${last.name} is out!`);
    this.audio.crash(4);
    const rival = last.rival!;
    rival.active = false;
    rival.rig.root.visible = false;
    if (last.hitBox) last.hitBox.alive = false;
  }

  private checkFinished(): void {
    const s = this.spec;
    const me = this.me;
    if (this.isRace) {
      const out = me.outOrder > 0;
      const won = s.kind === 'elimination' && this.racers.every((r) => r === me || r.outOrder > 0);
      // A rival won long ago and the player still hasn't finished: give up.
      const tooSlow = this.firstFinish !== null && me.finished === null && this.clock - this.firstFinish > 60;
      if (me.finished !== null || out || won || tooSlow) this.finish();
      return;
    }
    if (this.usesLaps) {
      if (me.finished !== null || this.clock > s.laps * 300) this.finish();
      return;
    }
    if (this.clock >= s.timeLimit) this.finish();
  }

  /** The running order: finished cars first, then by distance; knocked-out cars last. */
  order(): Racer[] {
    return [...this.racers].sort((a, b) => {
      if (a.outOrder || b.outOrder) return (b.outOrder || Infinity) - (a.outOrder || Infinity);
      if (a.finished !== null || b.finished !== null) return (a.finished ?? Infinity) - (b.finished ?? Infinity);
      return b.distance - a.distance;
    });
  }

  /** Whether a lower score is better: finishing places and times. */
  get lowerIsBetter(): boolean {
    return this.isRace || this.spec.kind === 'timeAttack';
  }

  /** Works out the stars for a score. */
  starsFor(score: number): number {
    const t = this.spec.targets;
    let stars = 0;
    for (let k = 0; k < 3; k++) if (this.lowerIsBetter ? score <= t[k] && score > 0 : score >= t[k]) stars = k + 1;
    return stars;
  }

  private finish(): void {
    this.state = 'done';
    this.world.hitBoxes = [];
    const s = this.spec;
    const me = this.me;
    let score = this.score;
    let scoreText = '';
    let position = 0;
    if (s.kind === 'drift') score += this.unbankedDrift;
    if (s.kind === 'stunt') score += this.unbankedAir;
    const standings = this.order().map((r) => ({
      name: r.name,
      me: r === me,
      text: r.finished !== null ? formatTime(r.finished) : r.outOrder ? 'Out' : s.kind === 'elimination' ? 'Survivor' : r.rival ? 'Running' : 'Did not finish',
    }));
    // Not finishing a race (timed out after the winner) earns nothing, whatever the place.
    let finished = true;
    if (this.isRace) {
      position = this.order().indexOf(me) + 1;
      if (me.finished === null && !me.outOrder && s.kind !== 'elimination') {
        position = this.racers.length;
        finished = false;
      }
      score = position;
      scoreText = finished ? ordinal(position) : 'Did not finish';
    } else if (s.kind === 'timeAttack') {
      score = me.finished ?? 0;
      scoreText = me.finished !== null ? formatTime(me.finished) : 'Did not finish';
    } else {
      scoreText = this.formatScore(score);
    }
    const stars = finished ? this.starsFor(score) : 0;
    this.result = { stars, position, score, scoreText, passed: stars > 0, standings, bestLap: this.bestLap };
    this.callouts.push(position === 1 ? 'Winner!' : stars === 3 ? 'Perfect!' : stars > 0 ? 'Finished!' : 'Better luck next time.');
  }

  /** A score as shown for this kind of event. */
  formatScore(value: number): string {
    switch (this.spec.kind) {
      case 'race':
      case 'elimination':
      case 'duel':
        return ordinal(value);
      case 'timeAttack':
        return formatTime(value);
      case 'takedown':
        return `${value} takedown${value === 1 ? '' : 's'}`;
      case 'drift':
        return `${Math.round(value).toLocaleString()} pts`;
      case 'speedTrap':
        return `${Math.round(value)} mph`;
      case 'destruction':
        return `$${Math.round(value).toLocaleString()}`;
      case 'stunt':
        return `${value.toFixed(1)} s air`;
    }
  }

  // ---------- told by the app ----------

  /** A finished drift: its points count in a drift event. */
  addDrift(points: number): void {
    if (this.spec.kind === 'drift' && this.state === 'running') this.score += points;
  }

  /** A landing after `seconds` in the air. */
  addAir(seconds: number): void {
    if (this.spec.kind === 'stunt' && this.state === 'running') this.score += seconds;
  }

  /** Something destroyed, worth `dollars`. */
  addDamage(dollars: number): void {
    if (this.spec.kind === 'destruction' && this.state === 'running') this.score += dollars;
  }

  /** R: puts the player's car back on the track where it left it, or at the start in the city. */
  recover(): void {
    const p = this.me.physics;
    if (!this.track) {
      const s = this.world.spawn;
      p.reset(s.x, s.z, s.heading);
      p.y = this.world.heightAt(s.x, s.z);
      return;
    }
    const nitro = p.nitro;
    this.placeOnTrack(this.me, this.me.along, 0);
    p.nitro = nitro;
    this.me.lost = 0;
    this.me.wrongWay = 0;
  }

  // ---------- the HUD ----------

  hud(): EventHud {
    const s = this.spec;
    const me = this.me;
    const stats: EventHud['stats'] = [];
    const track = this.track;
    const lap = track ? Math.min(s.laps, Math.max(1, Math.floor(me.distance / track.length) + 1)) : 0;
    if (this.isRace) {
      const order = this.order();
      stats.push({ label: 'POS', value: `${order.indexOf(me) + 1}/${order.filter((r) => !r.outOrder || r === me).length}` });
      if (s.kind === 'elimination') stats.push({ label: 'NEXT OUT', value: `${Math.max(0, Math.ceil(this.nextOut - this.clock))}s` });
      else stats.push({ label: 'LAP', value: `${lap}/${s.laps}` });
      stats.push({ label: 'TIME', value: formatTime(this.clock) });
    } else if (this.usesLaps) {
      stats.push({ label: 'LAP', value: `${lap}/${s.laps}` });
      stats.push({ label: 'TIME', value: formatTime(this.clock) });
      if (s.kind === 'speedTrap') stats.push({ label: 'SCORE', value: this.formatScore(this.score) });
      else stats.push({ label: '★★★', value: formatTime(s.targets[2]) });
    } else {
      stats.push({ label: 'TIME LEFT', value: formatTime(Math.max(0, s.timeLimit - this.clock)) });
      stats.push({ label: 'SCORE', value: this.formatScore(this.score) });
      const next = s.targets.find((t) => this.score < t);
      stats.push({ label: next === undefined ? 'ALL STARS!' : 'NEXT ★', value: next === undefined ? '★★★' : this.formatScore(next) });
    }

    const standings: EventHud['standings'] = [];
    if (this.isRace) {
      const order = this.order();
      const leader = order[0];
      for (const r of order) {
        const gap = r === leader ? 'Leader' : r.outOrder ? 'OUT' : r.finished !== null ? 'Finished' : `${Math.round(r.distance - leader.distance)} m`;
        standings.push({ name: r.name, gap, me: r === me, out: r.outOrder > 0 });
      }
    }

    let warning = '';
    if (this.state === 'running' && track) {
      if (this.playerWrecked > 0) warning = 'WRECKED';
      else if (me.lost > 1) warning = 'RETURN TO THE TRACK · R to reset';
      else if (me.wrongWay > 1) warning = 'WRONG WAY';
    }
    if (this.state === 'countdown') warning = this.countdown > 3 ? (s.boss ? `👑 ${s.boss}` : s.title.toUpperCase()) : String(Math.ceil(this.countdown));

    // Stars earned so far, as it stands.
    let stars = 0;
    if (this.isRace) stars = this.starsFor(this.order().indexOf(me) + 1);
    else if (s.kind === 'timeAttack') stars = this.starsFor(Math.max(this.clock, 0.01) + (track ? Math.max(0, s.laps * track.length - me.distance) / 60 : 0));
    else stars = this.starsFor(this.score);

    return { stats, standings, warning, health: s.weapons && track ? Math.max(0, me.hp) / 100 : null, stars };
  }

  /** Colored dots for the rivals and cameras on the minimap. */
  mapMarkers(): { x: number; z: number; color: string; size: number }[] {
    const markers = this.racers
      .filter((r) => r.rival && r.rival.active)
      .map((r) => ({ x: r.physics.x, z: r.physics.z, color: r.rival!.wrecked > 0 ? '#555' : this.spec.boss ? '#ffd23f' : '#ff4d4d', size: 4 }));
    if (this.track) for (const t of this.traps) {
      const p = this.track.pose(t);
      markers.push({ x: p.x, z: p.z, color: '#ffb300', size: 5 });
    }
    return markers;
  }

  /** Moves the rivals' models to match their physics. */
  sync(dt: number, night: boolean): void {
    for (const r of this.racers) r.rival?.sync(dt, night);
  }

  dispose(): void {
    for (const r of this.racers) r.rival?.dispose();
    this.world.hitBoxes = [];
    this.trapGroup.traverse((o) => {
      if (o instanceof THREE.Mesh) o.geometry.dispose();
    });
    this.trapGroup.removeFromParent();
  }
}

/** 1 -> "1st", 2 -> "2nd", 3 -> "3rd", 11 -> "11th". */
export function ordinal(n: number): string {
  const tens = n % 100;
  const suffix = tens >= 11 && tens <= 13 ? 'th' : ['th', 'st', 'nd', 'rd'][n % 10] ?? 'th';
  return `${n}${suffix}`;
}
