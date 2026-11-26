// People's cars. Someone who wants to go a long way finds a car parked at
// the curb, gets in, and drives it along the city's roads: on the right,
// slowing for corners and for anything in the way, to the curb nearest where
// they're going. There they park and get out.
//
// The cars are real cars in the world (World.addTrafficCar), so you can ram
// them or blow them up, just like the traffic. They're moved the same way the
// traffic is (World.moveCar); the routes are worked out here, on the city's
// grid of roads.
import * as THREE from 'three';
import { ROAD_LINES, type CarTarget, type World } from '../../game/world';

const LANE = 3; // meters from a road's center line to the middle of its right-hand lane
const CURB = 4.6; // where cars park: by the right-hand curb
const PAVEMENT = 7.5; // where people stand to get in: on the pavement beside the car
const LOW = ROAD_LINES[0];
const HIGH = ROAD_LINES[ROAD_LINES.length - 1];
const CRUISE = 12; // m/s
const TURN = 6; // m/s round a corner

/** A point the car drives through, and how fast it can take it. */
export interface Waypoint {
  x: number;
  z: number;
  /** Top speed through this point (slow for corners, stopped at the end). */
  speed: number;
}

/** Where a car parks near a spot: a road, a side of it, and how far along. */
interface Curb {
  /** 'x': a road running along z at x = line. 'z': a road running along x at z = line. */
  road: 'x' | 'z';
  line: number;
  along: number;
  /** Which side of the road (+1 or -1 on the other axis). */
  side: number;
}

/** The curb nearest a spot, away from the crossings. */
function curbNear(x: number, z: number): Curb {
  let best: Curb = { road: 'x', line: ROAD_LINES[0], along: z, side: 1 };
  let bestD = Infinity;
  for (const line of ROAD_LINES) {
    const dx = Math.abs(x - line);
    if (dx < bestD) {
      bestD = dx;
      best = { road: 'x', line, along: z, side: x >= line ? 1 : -1 };
    }
    const dz = Math.abs(z - line);
    if (dz < bestD) {
      bestD = dz;
      best = { road: 'z', line, along: x, side: z >= line ? 1 : -1 };
    }
  }
  // Keep it on the city's roads and clear of the crossings (12 m either side).
  let along = Math.max(LOW + 14, Math.min(HIGH - 14, best.along));
  const crossing = ROAD_LINES.reduce((a, b) => (Math.abs(b - along) < Math.abs(a - along) ? b : a));
  if (Math.abs(along - crossing) < 14) along = crossing + (along >= crossing ? 14 : -14);
  return { ...best, along };
}

/**
 * The way a car faces when it's parked on a side of a road: cars drive on
 * the right, so on the +x side of a road along z, they face -z.
 */
function curbDirection(k: Curb): { fx: number; fz: number } {
  if (k.road === 'x') return { fx: 0, fz: k.side > 0 ? -1 : 1 };
  return { fx: k.side > 0 ? 1 : -1, fz: 0 };
}

function curbPoint(k: Curb, out: number): { x: number; z: number } {
  return k.road === 'x' ? { x: k.line + k.side * out, z: k.along } : { x: k.along, z: k.line + k.side * out };
}

/** Where a car parks near (x, z), which way it faces, and where you stand to get in. */
export function parkingNear(x: number, z: number): { x: number; z: number; heading: number; door: { x: number; z: number } } {
  const k = curbNear(x, z);
  const p = curbPoint(k, CURB);
  const d = curbDirection(k);
  return { ...p, heading: Math.atan2(d.fx, d.fz), door: curbPoint(k, PAVEMENT) };
}

/**
 * A route along the roads from a car at (x, z) heading `heading` to the curb
 * nearest (tx, tz). The car keeps to the right, turns at the crossings
 * (Manhattan style: along one road, then the other) and pulls in at the end.
 */
export function routeTo(x: number, z: number, heading: number, tx: number, tz: number): Waypoint[] {
  const fx = Math.round(Math.sin(heading));
  const fz = Math.round(Math.cos(heading));
  const end = curbNear(tx, tz);
  const endDir = curbDirection(end);
  // Center-line points: where the car is now, the crossings it goes through, and the end.
  const snap = (v: number) => ROAD_LINES.reduce((a, b) => (Math.abs(b - v) < Math.abs(a - v) ? b : a));
  const onX = Math.abs(fx) < 0.5; // driving along z, on a road at x = line
  const line = onX ? snap(x) : snap(z);
  const along = onX ? z : x;
  const sign = onX ? Math.sign(fz) || 1 : Math.sign(fx) || 1;
  // The first crossing ahead.
  const ahead = ROAD_LINES.filter((c) => (c - along) * sign > 4).sort((a, b) => (a - b) * sign)[0] ?? along + sign * 20;
  const first = onX ? { x: line, z: ahead } : { x: ahead, z: line };
  // The crossing just before the end curb, driving the way the end curb faces.
  const endSign = end.road === 'x' ? endDir.fz : endDir.fx;
  const behind = ROAD_LINES.filter((c) => (end.along - c) * endSign > 4).sort((a, b) => (b - a) * endSign)[0] ?? end.along - endSign * 20;
  const last = end.road === 'x' ? { x: end.line, z: behind } : { x: behind, z: end.line };
  // Between the two crossings: along x then z, or z then x (whichever doesn't double back at the start).
  const xFirst = { x: last.x, z: first.z };
  const zFirst = { x: first.x, z: last.z };
  const backwards = (p: { x: number; z: number }) => (p.x - first.x) * fx + (p.z - first.z) * fz < -1;
  const corner = backwards(xFirst) ? zFirst : xFirst;
  const center = [{ x: onX ? line : x, z: onX ? z : line }, first, corner, last, { x: end.road === 'x' ? end.line : end.along, z: end.road === 'x' ? end.along : end.line }];
  // Drop points that repeat.
  const pts = center.filter((p, i) => i === 0 || Math.hypot(p.x - center[i - 1].x, p.z - center[i - 1].z) > 0.5);

  // Into the right-hand lane: each leg moves over to its right; corners are where two lanes meet.
  const legDir = (a: { x: number; z: number }, b: { x: number; z: number }) => ({ x: Math.sign(Math.round(b.x - a.x)), z: Math.sign(Math.round(b.z - a.z)) });
  const right = (d: { x: number; z: number }) => ({ x: -d.z, z: d.x }); // heading +z, right is -x
  const out: Waypoint[] = [];
  const legs = pts.slice(1).map((p, i) => ({ a: pts[i], b: p, d: legDir(pts[i], p) }));
  legs.forEach((leg, i) => {
    const r = right(leg.d);
    const next = legs[i + 1];
    if (!next) {
      // The last leg: pull in to the curb at the end.
      const stop = curbPoint(end, CURB);
      out.push({ x: leg.b.x + r.x * LANE - leg.d.x * 10, z: leg.b.z + r.z * LANE - leg.d.z * 10, speed: 6 }, { ...stop, speed: 0 });
      return;
    }
    const nr = right(next.d);
    const same = leg.d.x === next.d.x && leg.d.z === next.d.z;
    const opposite = leg.d.x === -next.d.x && leg.d.z === -next.d.z;
    if (same) return; // straight on through the crossing
    if (opposite) {
      // A U-turn in the crossing.
      out.push({ x: leg.b.x + r.x * LANE + leg.d.x * 3, z: leg.b.z + r.z * LANE + leg.d.z * 3, speed: 3 });
      out.push({ x: leg.b.x + nr.x * LANE + leg.d.x * 3, z: leg.b.z + nr.z * LANE + leg.d.z * 3, speed: 3 });
      return;
    }
    // A turn: where this leg's lane meets the next one's.
    const cx = leg.d.x === 0 ? leg.b.x + r.x * LANE : next.b.x + nr.x * LANE;
    const cz = leg.d.z === 0 ? leg.b.z + r.z * LANE : next.b.z + nr.z * LANE;
    out.push({ x: cx, z: cz, speed: TURN });
  });
  for (const w of out) {
    w.x = Math.max(LOW - 10, Math.min(HIGH + 10, w.x));
    w.z = Math.max(LOW - 10, Math.min(HIGH + 10, w.z));
  }
  return out;
}

/** Something on the road a car should stop for. */
export interface Obstacle {
  x: number;
  z: number;
}

/** A person's car: parked, or being driven along a route. */
export class NpcCar {
  readonly target: CarTarget;
  route: Waypoint[] = [];
  speed = 0;
  heading: number;
  x: number;
  z: number;
  /** Seconds it has been stuck behind something. */
  private waiting = 0;
  /** Parked at the end of its route (or not driven yet). */
  parked = true;
  /** When it was last driven (city clock), to take it away once it's been sitting a while. */
  parkedAt = 0;

  constructor(
    private world: World,
    x: number,
    z: number,
    heading: number,
  ) {
    this.target = world.addTrafficCar(x, z, heading);
    this.x = x;
    this.z = z;
    this.heading = heading;
  }

  /** Wrecked (rammed or blown up): it can't be driven. */
  get wrecked(): boolean {
    return !this.target.alive || this.target.flying;
  }

  /** Sets off for the curb nearest (x, z). */
  driveTo(x: number, z: number): void {
    this.route = routeTo(this.x, this.z, this.heading, x, z);
    this.parked = this.route.length === 0;
    this.waiting = 0;
  }

  /**
   * Drives for `dt` seconds. Returns true when it has just arrived and parked.
   * `obstacles` are things to stop for (other cars, the player).
   */
  update(dt: number, obstacles: Obstacle[], night: boolean, unseen: boolean, now: number): boolean {
    const t = this.target;
    if (this.wrecked) return false;
    if (this.parked) {
      this.speed = 0;
      t.rig.setLights({ night, braking: false });
      return false;
    }
    const wp = this.route[0];
    if (!wp) {
      this.parked = true;
      this.parkedAt = now;
      return true;
    }
    const dx = wp.x - this.x;
    const dz = wp.z - this.z;
    const d = Math.hypot(dx, dz);
    // Reached it (or just passed it): on to the next.
    if (d < (wp.speed === 0 ? 0.8 : 2.5)) {
      this.route.shift();
      if (this.route.length === 0) {
        this.parked = true;
        this.parkedAt = now;
        this.speed = 0;
        place(this);
        return true;
      }
      return false;
    }
    // How fast: slow enough to take the next point (and the one after) at its speed.
    let want = CRUISE * (unseen ? 1.6 : 1);
    let reach = d;
    for (const p of this.route.slice(0, 3)) {
      // (Aiming to stop just past the last point, so it's reached rather than stopped short of.)
      want = Math.min(want, Math.sqrt(p.speed * p.speed + 2 * 5 * Math.max(0, reach - 0.3)));
      reach += 40;
    }
    // Something in the way ahead: slow down to stop behind it.
    const fx = Math.sin(this.heading);
    const fz = Math.cos(this.heading);
    let gap = Infinity;
    for (const o of obstacles) {
      const ox = o.x - this.x;
      const oz = o.z - this.z;
      const along = ox * fx + oz * fz;
      if (along <= 0.5 || along > 24) continue;
      if (Math.abs(ox * fz - oz * fx) > 2.2) continue;
      gap = Math.min(gap, along);
    }
    if (gap < Infinity) want = Math.min(want, Math.max(0, (gap - 6) * 1.2));
    this.speed = Math.max(0, this.speed + THREE.MathUtils.clamp(want - this.speed, -10 * dt, 3.5 * dt));
    this.waiting = this.speed < 0.3 ? this.waiting + dt : 0;
    // Stuck for long out of sight: skip ahead to the next point.
    if (this.waiting > 10 && unseen) {
      this.x = wp.x;
      this.z = wp.z;
      this.waiting = 0;
    }
    // Turn towards the point (no tighter than a car can), and roll forward.
    const want_heading = Math.atan2(dx, dz);
    const turn = Math.atan2(Math.sin(want_heading - this.heading), Math.cos(want_heading - this.heading));
    const maxTurn = (Math.max(this.speed, 2) / 4.5) * dt;
    this.heading += THREE.MathUtils.clamp(turn, -maxTurn, maxTurn);
    this.x += Math.sin(this.heading) * this.speed * dt;
    this.z += Math.cos(this.heading) * this.speed * dt;
    place(this);
    for (const w of t.rig.wheels) {
      w.spin.rotation.x += (this.speed / w.radius) * dt;
      if (w.front) w.steer.rotation.y = THREE.MathUtils.clamp(turn * 2, -0.45, 0.45);
    }
    t.rig.setLights({ night, braking: want < this.speed - 0.5 || this.speed < 0.3 });
    return false;

    function place(car: NpcCar) {
      car.world.moveCar(car.target, car.x, car.z, car.heading);
      car.target.x = car.x;
      car.target.z = car.z;
    }
  }

  /** Takes the car out of the world. */
  dispose(): void {
    if (this.world.targets.includes(this.target)) this.world.removeCar(this.target);
  }
}
