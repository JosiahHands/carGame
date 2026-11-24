// City traffic: cars that drive the streets in free roam. They keep to the
// right-hand lane, take crossings one car at a time, queue behind each other,
// and stop (and honk) when you're in the way.
//
// Each one is a real car in the world (see World.addTrafficCar): ram it or
// blow it up and it wrecks like a parked car. When the world brings it back,
// it carries on from somewhere else.
import * as THREE from 'three';
import { ROAD_LINES, type CarTarget, type World } from './world';

const LANE = 3; // metres from a road's centre line to the middle of a lane
const STOP = 9; // metres before a crossing's centre where cars wait their turn
const BLOCK = ROAD_LINES[1] - ROAD_LINES[0]; // 80 m between crossings
const LOW = ROAD_LINES[0];
const HIGH = ROAD_LINES[ROAD_LINES.length - 1];
const TURN_SPEED = 7; // m/s round a corner
const LOOK_AHEAD = 28; // metres a car looks ahead for something to stop for

/** Directions of travel: +z, +x, -z, -x. Heading = atan2(x, z) of the forward vector. */
type Dir = 0 | 1 | 2 | 3;
const FORWARD: [number, number][] = [[0, 1], [1, 0], [0, -1], [-1, 0]];
/** The right-hand side for each direction (cars drive on the right). */
const RIGHT: [number, number][] = [[-1, 0], [0, 1], [1, 0], [0, -1]];

/** A straight line (no control point) or a curve round a corner (a quadratic Bézier). */
interface Segment {
  ax: number;
  az: number;
  bx: number;
  bz: number;
  cx?: number;
  cz?: number;
  length: number;
}

interface TrafficCar {
  target: CarTarget;
  /** The road it's on: its direction, and the x (or z) of the road's centre line. */
  dir: Dir;
  line: number;
  /** Where along the road the next crossing is (its z, or x). */
  next: number;
  /** On a road towards a crossing, or going through one. */
  onCrossing: boolean;
  seg: Segment;
  at: number;
  speed: number;
  cruise: number;
  /** After a crossing: the direction and road it will be on. */
  exitDir: Dir;
  exitLine: number;
  /** Seconds it has been stopped; and stunned after being hit. */
  waiting: number;
  stunned: number;
  /** It was wrecked, and needs a new start when the world brings it back. */
  wrecked: boolean;
  nextHonk: number;
  nearMissAt: number;
}

/** Where the player's car is, for the traffic to avoid and to spot near misses. */
export interface TrafficPlayer {
  x: number;
  z: number;
  vx: number;
  vz: number;
  heading: number;
  width: number;
  length: number;
}

export class Traffic {
  private cars: TrafficCar[] = [];
  private world: World | null = null;
  /** Who is going through each crossing now: one car at a time. */
  private crossings = new Map<string, TrafficCar>();
  private clock = 0;
  /** Called when a car honks at you (close by, so the app can play it). */
  onHonk: () => void = () => {};

  /** Puts `count` cars on the streets (city only). Removes any from before. */
  load(world: World, count: number, playerX: number, playerZ: number): void {
    this.clear();
    if (world.map !== 'city') return;
    this.world = world;
    for (let i = 0; i < count; i++) {
      const car = this.spawn(playerX, playerZ);
      if (car) this.cars.push(car);
    }
  }

  /** Takes all the traffic out of the world. */
  clear(): void {
    if (this.world) for (const c of this.cars) this.world.removeCar(c.target);
    this.cars = [];
    this.crossings.clear();
    this.world = null;
  }

  get count(): number {
    return this.cars.length;
  }

  /** True if `t` is one of the traffic cars. */
  owns(t: unknown): boolean {
    return this.cars.some((c) => c.target === t);
  }

  /** The player rammed a traffic car: it stops for a moment, and lets you know. */
  hit(t: unknown): void {
    const car = this.cars.find((c) => c.target === t);
    if (!car) return;
    car.speed = 0;
    car.stunned = 2.5;
    if (this.clock > car.nextHonk) {
      car.nextHonk = this.clock + 3;
      this.onHonk();
    }
  }

  /**
   * Drives every car for `dt` seconds. Returns the points for any near misses
   * the player had with traffic (passing fast and very close).
   */
  update(dt: number, player: TrafficPlayer, night: boolean): number[] {
    const world = this.world;
    if (!world) return [];
    this.clock += dt;
    const misses: number[] = [];
    for (const car of this.cars) {
      const t = car.target;
      if (!t.alive) {
        this.release(car);
        car.wrecked = true;
        continue;
      }
      if (car.wrecked) {
        // The world has put it back: start again somewhere out of the way.
        const fresh = this.start(player.x, player.z, car.target);
        if (fresh) Object.assign(car, fresh);
        continue;
      }
      this.drive(car, dt, player, world);
      // Near misses: you shoot past very close, much faster than it's going.
      const miss = this.nearMiss(car, player);
      if (miss > 0) misses.push(miss);
      // Wheels, and brake lights when slowing or stopped.
      for (const w of t.rig.wheels) {
        w.spin.rotation.x += (car.speed / w.radius) * dt;
        // Turning from +z to +x (one step round the directions) is a left turn.
        if (w.front) w.steer.rotation.y = car.onCrossing && car.seg.cx !== undefined ? (car.exitDir === (car.dir + 1) % 4 ? 0.35 : -0.35) : 0;
      }
      t.rig.setLights({ night, braking: car.speed < car.cruise * 0.6 });
    }
    return misses;
  }

  private drive(car: TrafficCar, dt: number, player: TrafficPlayer, world: World): void {
    const t = car.target;
    car.stunned = Math.max(0, car.stunned - dt);
    let want = car.stunned > 0 ? 0 : car.onCrossing && car.seg.cx !== undefined ? TURN_SPEED : car.cruise;

    // Something ahead in the lane (another car, a wreck, or you): slow down to stop behind it.
    const heading = t.rig.root.rotation.y;
    const fx = Math.sin(heading);
    const fz = Math.cos(heading);
    let gap = Infinity;
    let playerAhead = false;
    const check = (x: number, z: number, room: number, isPlayer: boolean) => {
      const dx = x - t.x;
      const dz = z - t.z;
      const ahead = dx * fx + dz * fz;
      if (ahead <= 0 || ahead > LOOK_AHEAD) return;
      if (Math.abs(dx * fz - dz * fx) > room) return;
      if (ahead < gap) {
        gap = ahead;
        playerAhead = isPlayer;
      }
    };
    for (const other of this.cars) if (other !== car) check(other.target.x, other.target.z, 2.4, false);
    check(player.x, player.z, 1.4 + player.width / 2, true);
    if (gap < Infinity) want = Math.min(want, Math.max(0, (gap - 7) * 1.2));

    // Waiting for a crossing that another car is going through.
    if (!car.onCrossing) {
      const left = car.seg.length - car.at;
      if (left < 14 && !this.reserve(car)) want = Math.min(want, Math.max(0, (left - 0.3) * 1.5));
    }

    // Speed up gently, brake hard.
    const change = want - car.speed;
    car.speed = Math.max(0, car.speed + THREE.MathUtils.clamp(change, -12 * dt, 4 * dt));
    car.waiting = car.speed < 0.3 ? car.waiting + dt : 0;

    // Honk at the player for blocking the road.
    if (playerAhead && car.waiting > 1.2 && this.clock > car.nextHonk && gap < 20) {
      car.nextHonk = this.clock + 3 + Math.random() * 3;
      this.onHonk();
    }
    // Stuck for a long time, out of sight: move it somewhere else.
    if (car.waiting > 12 && Math.hypot(player.x - t.x, player.z - t.z) > 80) {
      const fresh = this.start(player.x, player.z, t);
      if (fresh) {
        this.release(car);
        Object.assign(car, fresh);
      }
      return;
    }

    car.at += car.speed * dt;
    while (car.at >= car.seg.length) {
      car.at -= car.seg.length;
      if (!this.advance(car)) {
        car.at = car.seg.length;
        car.speed = 0;
        break;
      }
    }
    const p = pointOn(car.seg, car.at);
    world.moveCar(t, p.x, p.z, p.heading);
  }

  /**
   * Moves on to the next segment: from the road into the crossing (if it's
   * ours), or out of the crossing onto the next road. False if it must wait.
   */
  private advance(car: TrafficCar): boolean {
    if (!car.onCrossing) {
      if (!this.reserve(car)) return false;
      car.onCrossing = true;
      car.seg = crossingSegment(car.dir, car.line, car.next, car.exitDir, car.exitLine);
      return true;
    }
    // Out the other side: onto the next road, up to the next crossing.
    this.release(car);
    const straight = car.exitDir === car.dir;
    const [efx, efz] = FORWARD[car.exitDir];
    const sign = efx + efz; // +1 or -1 along the new road
    const crossedAt = straight ? car.next : car.line; // where the crossing was, along the new road
    car.dir = car.exitDir;
    car.line = car.exitLine;
    car.next = crossedAt + sign * BLOCK;
    car.onCrossing = false;
    const from = lanePoint(car.dir, car.line, crossedAt + sign * STOP);
    const to = lanePoint(car.dir, car.line, car.next - sign * STOP);
    car.seg = line(from, to);
    this.chooseTurn(car);
    return true;
  }

  /** Picks which way to go at the next crossing: mostly straight on, sometimes a turn. */
  private chooseTurn(car: TrafficCar): void {
    const options: { dir: Dir; line: number; weight: number }[] = [];
    const [fx, fz] = FORWARD[car.dir];
    if (inCity(car.next + (fx + fz) * BLOCK)) options.push({ dir: car.dir, line: car.line, weight: 2 });
    for (const turn of [1, 3]) {
      const dir = ((car.dir + turn) % 4) as Dir;
      const [tx, tz] = FORWARD[dir];
      // After the turn the car is on the road through the crossing, heading away from its old road.
      if (inCity(car.line + (tx + tz) * BLOCK)) options.push({ dir, line: car.next, weight: 1 });
    }
    let roll = Math.random() * options.reduce((a, o) => a + o.weight, 0);
    const pick = options.find((o) => (roll -= o.weight) < 0) ?? options[0];
    car.exitDir = pick.dir;
    car.exitLine = pick.line;
  }

  /** Claims the next crossing for this car, if no one else is going through it. */
  private reserve(car: TrafficCar): boolean {
    const key = this.crossingKey(car);
    const holder = this.crossings.get(key);
    if (holder && holder !== car) return false;
    this.crossings.set(key, car);
    return true;
  }

  private release(car: TrafficCar): void {
    const key = this.crossingKey(car);
    if (this.crossings.get(key) === car) this.crossings.delete(key);
  }

  private crossingKey(car: TrafficCar): string {
    return car.dir % 2 === 0 ? `${car.line},${car.next}` : `${car.next},${car.line}`;
  }

  private nearMiss(car: TrafficCar, player: TrafficPlayer): number {
    const t = car.target;
    const speed = Math.hypot(player.vx, player.vz);
    if (speed < 18 || this.clock - car.nearMissAt < 2.5) return 0;
    const heading = t.rig.root.rotation.y;
    const fx = Math.sin(heading);
    const fz = Math.cos(heading);
    const dx = player.x - t.x;
    const dz = player.z - t.z;
    const along = Math.abs(dx * fx + dz * fz);
    const side = Math.abs(dx * fz - dz * fx);
    const gap = side - t.rig.width / 2 - player.width / 2;
    if (along > (t.rig.length + player.length) / 2 || gap < 0.15 || gap > 1.4) return 0;
    const closing = Math.hypot(player.vx - fx * car.speed, player.vz - fz * car.speed);
    if (closing < 4) return 0;
    car.nearMissAt = this.clock;
    return 100 + speed * 3;
  }

  /** A new car on a random street, away from the player. */
  private spawn(playerX: number, playerZ: number): TrafficCar | null {
    const start = this.start(playerX, playerZ, null);
    if (!start) return null;
    const p = pointOn(start.seg, 0);
    return { ...start, target: this.world!.addTrafficCar(p.x, p.z, p.heading) };
  }

  /**
   * A random place to start: on a road between two crossings, at least 50 m
   * from the player and clear of the other cars. Moves `target` there if given.
   */
  private start(playerX: number, playerZ: number, target: CarTarget | null): Omit<TrafficCar, 'target'> | null {
    for (let tries = 0; tries < 40; tries++) {
      const dir = Math.floor(Math.random() * 4) as Dir;
      const [fx, fz] = FORWARD[dir];
      const sign = fx + fz;
      const lineAt = ROAD_LINES[Math.floor(Math.random() * ROAD_LINES.length)];
      // The next crossing can't be the first one in this direction.
      const choices = ROAD_LINES.filter((c) => inCity(c - sign * BLOCK));
      const next = choices[Math.floor(Math.random() * choices.length)];
      const along = next - sign * (STOP + 6 + Math.random() * (BLOCK - 2 * STOP - 12));
      const p = lanePoint(dir, lineAt, along);
      if (Math.hypot(p.x - playerX, p.z - playerZ) < 50) continue;
      if (this.cars.some((c) => c.target !== target && Math.hypot(c.target.x - p.x, c.target.z - p.z) < 14)) continue;
      const car: Omit<TrafficCar, 'target'> = {
        dir, line: lineAt, next, onCrossing: false,
        seg: line(p, lanePoint(dir, lineAt, next - sign * STOP)), at: 0,
        speed: 0, cruise: 11 + Math.random() * 6, exitDir: dir, exitLine: lineAt,
        waiting: 0, stunned: 0, wrecked: false, nextHonk: 0, nearMissAt: -10,
      };
      this.chooseTurn(car as TrafficCar);
      if (target) {
        target.homeX = p.x; // if it's wrecked again, it comes back here
        target.homeZ = p.z;
        const pose = pointOn(car.seg, 0);
        this.world!.moveCar(target, pose.x, pose.z, pose.heading);
        target.homeHeading = pose.heading;
      }
      return car;
    }
    return null;
  }
}

function inCity(c: number): boolean {
  return c >= LOW - 1 && c <= HIGH + 1;
}

/** The middle of the right-hand lane, `along` metres along a road. */
function lanePoint(dir: Dir, roadLine: number, along: number): { x: number; z: number } {
  const [fx] = FORWARD[dir];
  const [rx, rz] = RIGHT[dir];
  return fx === 0 ? { x: roadLine + rx * LANE, z: along } : { x: along, z: roadLine + rz * LANE };
}

function line(a: { x: number; z: number }, b: { x: number; z: number }): Segment {
  return { ax: a.x, az: a.z, bx: b.x, bz: b.z, length: Math.hypot(b.x - a.x, b.z - a.z) };
}

/** Through a crossing: straight on, or a curve from one lane into the other road's lane. */
function crossingSegment(dir: Dir, roadLine: number, next: number, exitDir: Dir, exitLine: number): Segment {
  const [fx, fz] = FORWARD[dir];
  const sign = fx + fz;
  const from = lanePoint(dir, roadLine, next - sign * STOP);
  if (exitDir === dir) return line(from, lanePoint(dir, roadLine, next + sign * STOP));
  const [ex, ez] = FORWARD[exitDir];
  const to = lanePoint(exitDir, exitLine, roadLine + (ex + ez) * STOP);
  // The corner of the curve is where the two lanes meet.
  const cx = fx === 0 ? from.x : to.x;
  const cz = fx === 0 ? to.z : from.z;
  let length = 0;
  let last = { x: from.x, z: from.z };
  for (let i = 1; i <= 12; i++) {
    const p = bezier(from.x, from.z, cx, cz, to.x, to.z, i / 12);
    length += Math.hypot(p.x - last.x, p.z - last.z);
    last = p;
  }
  return { ax: from.x, az: from.z, bx: to.x, bz: to.z, cx, cz, length };
}

function bezier(ax: number, az: number, cx: number, cz: number, bx: number, bz: number, t: number): { x: number; z: number } {
  const u = 1 - t;
  return { x: u * u * ax + 2 * u * t * cx + t * t * bx, z: u * u * az + 2 * u * t * cz + t * t * bz };
}

/** Where on a segment, and which way it faces, `at` metres along. */
function pointOn(seg: Segment, at: number): { x: number; z: number; heading: number } {
  const t = seg.length > 0 ? Math.min(1, at / seg.length) : 1;
  if (seg.cx === undefined || seg.cz === undefined) {
    return { x: seg.ax + (seg.bx - seg.ax) * t, z: seg.az + (seg.bz - seg.az) * t, heading: Math.atan2(seg.bx - seg.ax, seg.bz - seg.az) };
  }
  const p = bezier(seg.ax, seg.az, seg.cx, seg.cz, seg.bx, seg.bz, t);
  const u = 1 - t;
  const dx = 2 * u * (seg.cx - seg.ax) + 2 * t * (seg.bx - seg.cx);
  const dz = 2 * u * (seg.cz - seg.az) + 2 * t * (seg.bz - seg.cz);
  return { ...p, heading: Math.atan2(dx, dz) };
}
