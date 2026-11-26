// Computer-driven rival cars. Each one uses the same physics as the player's
// car and drives it with the same controls (throttle, brake, steering,
// nitro), worked out every step:
//
//  - Steering: aim at a point on the racing line a little way ahead. The
//    racing line cuts to the inside of each corner.
//  - Speed: look down the road for corners. Each corner has a top speed the
//    tires can hold; brake in time to be at or under it when getting there.
//  - Traffic: a car just ahead in the way? Pull out to pass it, or follow.
//  - Skill (about 0.75 to 1.05) is how close to the limit a rival drives.
//    Less skilled rivals brake earlier, and now and then make a mistake: they
//    run into a corner too fast and slide wide.
//
// A rival that gets stuck or lost (off in the trees, facing a wall) is put
// back on the track, just like the player pressing R.
import * as THREE from 'three';
import { buildCar, type CarRig } from './carModel';
import type { CarConfig } from './carConfig';
import { CarPhysics, type CarInput } from './carPhysics';
import type { RaceTrack } from './raceTrack';
import type { World } from './world';

/** Who a rival is, and what they drive. */
export interface RivalSpec {
  name: string;
  config: CarConfig;
  /** How close to the limit they drive: 0.75 (gentle) to 1.05 (flat out, no mistakes). */
  skill: number;
  /** Never makes mistakes: for the pace car that sets target times. */
  steady?: boolean;
}

/** Where a car is in the race, for the rivals to see each other (and the player). */
export interface RacerPosition {
  physics: CarPhysics;
  /** Meters from the start line around the lap, 0..track length. */
  along: number;
  /** Meters to the left of the center line. */
  offset: number;
}

/** Rivals who were renamed, old name to new, so saved records follow them (Kiki Kerb became Kiki Curb in 1.2.3). */
export const RENAMED_RIVALS: Record<string, string> = { 'Kiki Kerb': 'Kiki Curb' };

export const RIVAL_NAMES = [
  'Max Power', 'Lola Lightning', 'Rico Revs', 'Dash Delgado', 'Penny Piston', 'Turbo Tim', 'Gina Gearbox', 'Axel Grease',
  'Skid Marx', 'Nitro Nina', 'Bruno Burnout', 'Holly Hairpin', 'Vince Vroom', 'Zoe Zoom', 'Duke Drift', 'Cleo Clutch',
  'Rusty Bolts', 'Speedy Gonzo', 'Ivy Apex', 'Chuck Chicane', 'Mona Mph', 'Felix Flatout', 'Kiki Curb', 'Otto Overtake',
];

/** The racing line on each track: an offset from the center line for every sample. Worked out once per track. */
const racingLines = new WeakMap<RaceTrack, Float32Array>();

/**
 * The racing line: towards the inside of each corner, by more for tighter
 * corners, eased in and out so it starts turning in early and runs out wide
 * afterwards. Kept a car's width inside the edges of the road.
 */
export function racingLine(track: RaceTrack): Float32Array {
  const cached = racingLines.get(track);
  if (cached) return cached;
  const n = track.points.length;
  const room = Math.max(0, track.style.width / 2 - 2);
  const raw = track.points.map((p) => Math.max(-1, Math.min(1, p.curve * 60)) * room);
  // Smooth over 40 m either way (10 samples).
  const line = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    let sum = 0;
    for (let k = -10; k <= 10; k++) sum += raw[(i + k + n) % n];
    line[i] = sum / 21;
  }
  racingLines.set(track, line);
  return line;
}

export class Rival {
  readonly physics = new CarPhysics();
  rig: CarRig;
  readonly name: string;
  readonly skill: number;
  /** This driver's favorite spot across the road, compared to the racing line. */
  private lane: number;
  /** Extra offset while pulling out to pass someone, easing back to 0. */
  private dodge = 0;
  /** Seconds left of a mistake: running into corners too fast. */
  private mistake = 0;
  private nextMistake: number;
  /** Seconds spent stuck (barely moving) or lost (far off the track). */
  private stuck = 0;
  /** Seconds left before a wrecked rival is put back on the track. */
  wrecked = 0;
  /** Health in a battle: bullets and rockets wear it down. */
  hp = 100;
  /**
   * Catch-up from the race: above 1 when far behind the player, below 1 when
   * far ahead. It scales how hard they push in corners and on straights.
   */
  pace = 1;
  private steady: boolean;
  /** Whether this rival is still racing (not knocked out of an elimination). */
  active = true;
  /** Whether nitro is allowed in this event. */
  useNitro = true;

  /** `scene` is null for a car that only runs the physics, never shown (the pace car). */
  constructor(spec: RivalSpec, scene: THREE.Scene | null, night: boolean) {
    this.name = spec.name;
    this.skill = spec.skill;
    this.steady = spec.steady ?? false;
    this.rig = buildCar(spec.config, night, false);
    this.physics.configure(spec.config, this.rig);
    scene?.add(this.rig.root);
    this.lane = (Math.random() - 0.5) * 2;
    this.nextMistake = 6 + Math.random() * 10;
  }

  /** Puts the car on the track, standing still, `along` meters from the start line and `offset` to the left. */
  place(track: RaceTrack, world: World, along: number, offset: number): void {
    const p = track.pose(along, offset);
    this.physics.reset(p.x, p.z, p.heading);
    this.physics.y = world.heightAt(p.x, p.z);
    this.stuck = 0;
  }

  /**
   * Works out this step's controls and runs the physics. `me` is where this
   * car is; `others` is every other car in the race, the player included.
   */
  drive(dt: number, world: World, track: RaceTrack, me: RacerPosition, others: RacerPosition[], racing: boolean): void {
    const p = this.physics;
    const input: CarInput = { throttle: 0, brake: 0, steer: 0, handbrake: false, boost: false, jump: false };
    if (this.wrecked > 0 || !this.active) {
      // Wrecked or out: roll to a stop.
      input.brake = 1;
      p.step(dt, input, world);
      return;
    }
    if (!racing) {
      p.step(dt, input, world);
      return;
    }
    const speed = Math.max(0, p.forwardSpeed);
    const half = track.style.width / 2;

    // Mistakes: the less skilled, the more often.
    this.nextMistake -= dt;
    if (this.nextMistake <= 0 && !this.steady) {
      this.nextMistake = 5 + Math.random() * 12;
      if (Math.random() < (1.02 - this.skill) * 2.5) this.mistake = 1.2 + Math.random();
    }
    this.mistake = Math.max(0, this.mistake - dt);

    // Traffic: the nearest car just ahead that's in the way.
    let blocker: RacerPosition | null = null;
    let gap = Infinity;
    for (const o of others) {
      let ahead = o.along - me.along;
      if (ahead < -track.length / 2) ahead += track.length;
      if (ahead > track.length / 2) ahead -= track.length;
      if (ahead > 0 && ahead < 10 + speed * 0.6 && Math.abs(o.offset - me.offset) < 3 && ahead < gap) {
        gap = ahead;
        blocker = o;
      }
    }
    const line = racingLine(track);
    const look = 7 + speed * 0.42;
    const targetAlong = me.along + look;
    const lineHere = line[Math.floor((((targetAlong % track.length) + track.length) % track.length) / track.length * line.length) % line.length];
    if (blocker && blocker.physics.speed < speed + 3) {
      // Pass on whichever side has more room.
      const left = half - blocker.offset;
      const right = blocker.offset + half;
      const side = left > right ? 1 : -1;
      this.dodge += (blocker.offset + side * 3.6 - (lineHere + this.lane) - this.dodge) * Math.min(1, dt * 3);
    } else {
      this.dodge -= this.dodge * Math.min(1, dt * 0.8);
    }
    const offset = Math.max(-half + 1.6, Math.min(half - 1.6, lineHere + this.lane + this.dodge));

    // Steer for the arc that reaches the target point (pure pursuit).
    const target = track.pose(targetAlong, offset);
    const dx = target.x - p.x;
    const dz = target.z - p.z;
    const angle = Math.atan2(Math.sin(Math.atan2(dx, dz) - p.heading), Math.cos(Math.atan2(dx, dz) - p.heading));
    const distance = Math.hypot(dx, dz) || 1;
    input.steer = p.steerFor((2 * Math.sin(angle)) / distance);

    // Speed: the slowest a corner coming up allows, allowing for braking to it.
    const limit = this.skill * this.pace * (this.mistake > 0 ? 1.18 : 1);
    const brake = p.brakeDecel * 0.8;
    // Flat out on the straights, or holding back a little when well ahead.
    let wanted = p.topSpeed * (this.pace < 1 ? this.pace : 1.2);
    const scan = Math.min(400, 30 + (speed * speed) / (2 * brake));
    for (let s = 0; s < scan; s += 4) {
      // The racing line is gentler than the center line, so corners can be taken a little faster.
      const corner = p.cornerSpeed(track.pointAt(me.along + s).curve * 0.85) * limit;
      wanted = Math.min(wanted, Math.sqrt(corner * corner + 2 * brake * s));
    }
    if (blocker && gap < 8 && Math.abs(this.dodge) < 1.5) wanted = Math.min(wanted, blocker.physics.speed); // boxed in: follow
    if (speed < wanted - 0.5) input.throttle = 1;
    else if (speed < wanted + 0.5) input.throttle = 0.3;
    if (speed > wanted + 1.5) input.brake = Math.min(1, (speed - wanted) / 6 + 0.3);
    // Nitro on the straights, when there's plenty of road ahead.
    input.boost = this.useNitro && p.nitro > 0.25 && speed > p.topSpeed * 0.6 && wanted > p.topSpeed * 1.15 && this.skill > 0.8;

    p.step(dt, input, world);
    if (!input.boost) p.nitro = Math.min(1, p.nitro + dt * 0.04); // the bottle slowly refills

    // Stuck against something, or lost off the track: put it back.
    const lost = Math.abs(me.offset) > half + 14;
    this.stuck = p.speed < 2 || lost ? this.stuck + dt : 0;
    if (this.stuck > (lost ? 1 : 2.5)) this.place(track, world, me.along, lineHere);
  }

  /** Moves the model to match the physics. */
  sync(dt: number, night: boolean): void {
    const p = this.physics;
    const rig = this.rig;
    rig.root.position.set(p.x, p.y, p.z);
    rig.root.rotation.set(p.pitch, p.heading, p.groundRoll, 'YXZ');
    rig.body.rotation.set(p.bodyPitch, 0, p.roll);
    for (const w of rig.wheels) {
      w.spin.rotation.x += (p.forwardSpeed / w.radius) * dt;
      if (w.front) w.steer.rotation.y = p.steerAngle;
    }
    rig.setLights({ night, braking: p.braking });
    rig.setNitro(p.boosting);
    rig.animate(performance.now() / 1000);
  }

  dispose(): void {
    this.rig.root.removeFromParent();
    this.rig.dispose();
  }
}
