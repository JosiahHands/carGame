// Arcade car physics: acceleration, a gearbox (automatic or manual), braking,
// steering, grip, handbrake drifts, jumps off ramps and bumping into things.
// Hand-written, no physics engine.
import * as THREE from 'three';
import type { CarConfig } from './carConfig';
import { WALL_THICKNESS } from './raceTrack';
import type { Target, World } from './world';

export interface CarInput {
  throttle: number; // 0..1
  brake: number; // 0..1 (also reverse)
  steer: number; // -1 (right) .. 1 (left)
  handbrake: boolean;
  boost: boolean;
  jump: boolean;
}

export interface StepEvents {
  impact: number; // speed lost hitting something, m/s
  /** What the car hit hardest, if it can be destroyed. */
  hitTarget: Target | null;
  /** Lamps and trees the car drove straight through. */
  smashed: number;
  cones: number;
  landed: number; // vertical speed on landing, m/s
}

const EARTH_GRAVITY = 16; // a bit more than real gravity, so jumps feel snappy
export const MAX_REVERSE = 12;
const MPH = 0.44704; // meters per second
/**
 * Most sideways acceleration the tires can give, in m/s², at normal grip on
 * dry asphalt: about 2.2 g, like a race car. Take a corner faster than that
 * and the car runs wide (understeer), so you brake for the tight ones. More
 * grip and downforce raise it; rain, snow and grass lower it.
 */
export const TIRE_GRIP = 22;
/** With the handbrake on, the car can swing around this many times faster than its grip allows, and slides. */
const DRIFT_TURN = 3;
/** A car this tall (meters) or taller flattens anything up to twice its height. */
const GIANT_HEIGHT = 3;
/**
 * The five forward gears: where each one runs out of revs, as a share of the
 * car's top speed. Every upshift lands at least halfway up the next gear's
 * revs, where the engine still pulls hard.
 */
export const GEAR_TOPS = [0.18, 0.34, 0.52, 0.72, 1.0];

/**
 * Another way to drive the cars, from a pack (the physics pack). It replaces
 * the arcade model's engine, brakes, steering and tires; the game still does
 * crashes, jumps, ramps and slopes. Set CarPhysics.dynamics to use it.
 */
export interface CarDynamics {
  /**
   * One physics step of driving on the ground (or flying). Updates the car's
   * vx, vz, heading, steerAngle, gear, revs, limiter and braking, and returns
   * how fast it's turning (radians per second, positive is left).
   */
  drive(car: CarPhysics, dt: number, input: CarInput, world: World): number;
  /** Most sideways acceleration the tires can give at this speed, in m/s² (the AI plans its corner speeds with it). */
  cornerGrip(car: CarPhysics, speed: number, surfaceGrip: number): number;
  /** The car hit something: (nx, nz) points away from it, at a point `along` meters forward and `across` meters left of the center. */
  impact?(car: CarPhysics, nx: number, nz: number, along: number, across: number, lost: number): void;
  /** The car was put somewhere new, standing still. */
  reset?(car: CarPhysics, fresh?: boolean): void;
  /** After each step: may set the body's lean (roll and bodyPitch) from the model's own suspension. */
  pose?(car: CarPhysics, dt: number): void;
}

export class CarPhysics {
  /** A pack's driving model in place of the arcade one, or null. Shared by every car. */
  static dynamics: CarDynamics | null = null;
  /** Each car's own state for the pack's driving model. */
  dynamicsState: unknown = null;

  x = 0;
  z = 0;
  y = 0;
  heading = 0; // radians; 0 faces +z, positive turns left
  vx = 0;
  vz = 0;
  vy = 0;
  steerAngle = 0;
  airborne = false;
  /** Visual body lean, radians. */
  roll = 0;
  bodyPitch = 0;
  /** Ground slope pitch (ramps / flight), radians, nose-up negative. */
  pitch = 0;
  /** Ground slope sideways (banked turns), radians; positive lifts the left side. */
  groundRoll = 0;
  /** On grass, off the race track, which slows the car down. */
  onGrass = false;
  forwardSpeed = 0;
  lateralSpeed = 0;
  braking = false;
  /** Nitro left in the bottle, 0..1. */
  nitro = 1;
  boosting = false;
  /** Manual gearbox: the player changes gear with shiftUp() and shiftDown(). Otherwise it changes itself. */
  manual = false;
  /** Forward gear, 1 to 5. (Reversing works in any gear: hold the brake.) */
  gear = 1;
  /** Engine revs: 0 at a standstill, 1 at the redline of the current gear. */
  revs = 0;
  /** On the throttle at the redline, so the rev limiter is cutting the power (manual only). */
  limiter = false;

  // Set from the car's tuning by configure().
  /** m/s² at full throttle from a standstill. */
  accel = 9;
  /** Top speed in m/s. */
  topSpeed = 50;
  gripMultiplier = 1;
  grip = 7; // how quickly sideways sliding dies away
  /** How hard the brakes slow the car, in m/s². */
  brakeDecel = 20;
  maxSteer = 0.5; // radians
  driftGrip = 0.25;
  downforce = 0;
  nitroPower = 1;
  nitroBurn = 0.3; // bottle used per second
  infiniteNitro = false;
  jumpSpeed = 0; // m/s upwards
  airControl = 0.15;
  gravity = EARTH_GRAVITY;
  bounce = 0.35;
  // Set from the car's size by configure().
  wheelBase = 2.6;
  // The car's size in meters (from its model).
  length = 4.4;
  width = 1.9;
  height = 1.3;
  /** Grip of the surface under the car, from the world: 1 on dry asphalt. */
  surfaceGrip = 1;
  /**
   * All-terrain tires (the rally truck in the career): grass slows the car
   * half as much, and rain and snow take away less grip.
   */
  allTerrain = false;
  /**
   * Slipstream, 0..1: how well tucked in behind another car this is. Less
   * air to push through means a higher top speed (up to 8%). Set every step
   * by the race.
   */
  draft = 0;
  /** Seconds of free boost left: a drift mini-turbo or a rocket start. It doesn't use the nitro bottle. */
  turbo = 0;
  turboPower = 1;
  /** The car's settings from the garage, from configure(). */
  config: CarConfig | null = null;
  private lastForward = 0;
  private jumpHeld = false;

  configure(config: CarConfig, dims: { length: number; width: number; height: number; wheelBase: number }): void {
    const t = config.tuning;
    this.config = config;
    // 400 hp gives the original 10 m/s²; more power has diminishing returns.
    this.accel = 10 * (t.horsepower / 400) ** 0.6;
    this.topSpeed = t.topSpeed * MPH;
    this.gripMultiplier = t.grip;
    this.grip = 7 * t.grip;
    this.brakeDecel = 22 * t.brakes;
    this.maxSteer = (t.steering * Math.PI) / 180;
    this.driftGrip = Math.max(0.06, 0.45 - t.drift * 0.04);
    this.downforce = t.downforce;
    this.nitroPower = 1.2 * t.nitroPower;
    this.nitroBurn = 1 / t.nitroSeconds;
    this.infiniteNitro = t.infiniteNitro;
    this.gravity = EARTH_GRAVITY * t.gravity;
    // Launch speed to reach the jump height: v = sqrt(2 g h).
    this.jumpSpeed = Math.sqrt(2 * this.gravity * t.jump);
    this.airControl = t.airControl;
    this.bounce = t.bounce;
    this.wheelBase = dims.wheelBase;
    this.length = dims.length;
    this.width = dims.width;
    this.height = dims.height;
  }

  /**
   * Puts the car somewhere, standing still. `fresh` is a new start (an
   * event's grid), rather than putting the same car back on the road.
   */
  reset(x: number, z: number, heading: number, fresh = false): void {
    this.x = x;
    this.z = z;
    this.y = 0;
    this.heading = heading;
    this.vx = this.vz = this.vy = 0;
    this.steerAngle = this.roll = this.bodyPitch = this.pitch = this.groundRoll = 0;
    this.airborne = false;
    this.nitro = 1;
    this.boosting = false;
    this.turbo = 0;
    this.draft = 0;
    this.gear = 1;
    this.revs = 0;
    this.limiter = false;
    CarPhysics.dynamics?.reset?.(this, fresh);
  }

  /** Changes up a gear (manual gearbox). Returns false in top gear. */
  shiftUp(): boolean {
    if (this.gear >= GEAR_TOPS.length) return false;
    this.gear++;
    return true;
  }

  /**
   * Changes down a gear (manual gearbox). Returns false in first gear, or when
   * going too fast for the lower gear: that would send the engine past the redline.
   */
  shiftDown(): boolean {
    if (this.gear <= 1 || this.forwardSpeed > GEAR_TOPS[this.gear - 2] * this.topSpeed * 1.01) return false;
    this.gear--;
    return true;
  }

  /** A free boost for `seconds` (on top of any already going), as strong as nitro × `power`. */
  giveTurbo(seconds: number, power: number): void {
    if (this.turbo <= 0 || power >= this.turboPower) this.turboPower = power;
    this.turbo = Math.max(this.turbo, seconds);
  }

  /** Shoves the car, for example away from an explosion. */
  push(vx: number, vy: number, vz: number): void {
    this.vx += vx;
    this.vz += vz;
    if (vy > 0) {
      this.vy = Math.max(this.vy, 0) + vy;
      this.y += 0.05;
      this.airborne = true;
    }
  }

  get speed(): number {
    return Math.hypot(this.vx, this.vz);
  }

  /** Most sideways acceleration the tires can give at this speed (m/s²), with downforce and the surface. */
  cornerGrip(speed: number): number {
    if (CarPhysics.dynamics) return CarPhysics.dynamics.cornerGrip(this, speed, this.surfaceGrip);
    return TIRE_GRIP * this.gripMultiplier * this.surfaceGrip * (1 + (this.downforce / 10) * (speed / (100 * MPH)) ** 2);
  }

  /**
   * The fastest the car can take a bend of this curvature (1 / radius) in
   * m/s, on a surface this grippy. Infinity if downforce means it can take it
   * at any speed.
   */
  cornerSpeed(curve: number, surfaceGrip = this.surfaceGrip): number {
    if (CarPhysics.dynamics) {
      // The pack's grip changes with speed in its own way: find the speed where it just holds the bend.
      let lo = 0;
      let hi = 2000;
      if (CarPhysics.dynamics.cornerGrip(this, hi, surfaceGrip) >= hi * hi * Math.abs(curve)) return Infinity;
      for (let i = 0; i < 40; i++) {
        const v = (lo + hi) / 2;
        if (CarPhysics.dynamics.cornerGrip(this, v, surfaceGrip) >= v * v * Math.abs(curve)) lo = v;
        else hi = v;
      }
      return lo;
    }
    // Solve v² × curve = grip × (1 + k v²), where k is the downforce part.
    const g = TIRE_GRIP * this.gripMultiplier * surfaceGrip;
    const k = (this.downforce / 10) / (100 * MPH) ** 2;
    const room = Math.abs(curve) - g * k;
    return room <= 0 ? Infinity : Math.sqrt(g / room);
  }

  /** The steering input (-1..1) that turns along a bend of this curvature at the current speed. */
  steerFor(curve: number): number {
    const lock = this.maxSteer / (1 + Math.abs(this.forwardSpeed) / 28);
    return Math.max(-1, Math.min(1, Math.atan(curve * this.wheelBase) / lock));
  }

  /** 0..1, how close to top speed. */
  get speedFraction(): number {
    return Math.min(1, Math.abs(this.forwardSpeed) / this.topSpeed);
  }

  step(dt: number, input: CarInput, world: World): StepEvents {
    const events: StepEvents = { impact: 0, hitTarget: null, smashed: 0, cones: 0, landed: 0 };
    let fx = Math.sin(this.heading);
    let fz = Math.cos(this.heading);
    let vf = this.vx * fx + this.vz * fz;
    let vl = this.vx * fz - this.vz * fx;
    const control = this.airborne ? this.airControl : 1;

    const yawRate = CarPhysics.dynamics ? CarPhysics.dynamics.drive(this, dt, input, world) : this.arcade(dt, input, world, vf, vl, fx, fz, control);

    // Move, checking for crashes along the way. At hundreds of mph the car
    // moves several meters a step, so it moves in smaller pieces then, or it
    // could pass straight through a lamp post.
    const pieces = Math.min(12, Math.max(1, Math.ceil((this.speed * dt) / (Math.min(this.width, this.length) * 0.4))));
    for (let i = 0; i < pieces; i++) {
      this.x += (this.vx * dt) / pieces;
      this.z += (this.vz * dt) / pieces;
      events.impact = Math.max(events.impact, this.collide(world, events));
    }

    // Jump (when the car has a jump height set).
    if (input.jump && !this.jumpHeld && !this.airborne && this.jumpSpeed > 0) {
      this.vy = this.jumpSpeed;
      this.y += 0.05;
    }
    this.jumpHeld = input.jump;

    // Vertical: follow the ground, fly off ramp lips, land with gravity.
    const ground = world.heightAt(this.x, this.z);
    // A car on the ground stays on it where the road falls away gently, like
    // coming down off a banked turn (less than 1 in 5). Steeper drops, like
    // the lip of a ramp, still launch it into the air.
    const drop = this.y - ground;
    if (!this.airborne && this.vy <= 0 && drop > 0 && drop < this.speed * dt * 0.2 + 0.02) this.y = ground;
    if (this.y > ground + 0.02) {
      this.vy -= this.gravity * dt;
      this.y += this.vy * dt;
      if (this.y <= ground) {
        // Report how hard it hit. Don't rely on `airborne` here: that flag
        // switches off up to 15 cm above the ground, before touchdown.
        events.landed = Math.max(0, -this.vy);
        this.y = ground;
        this.vy = 0;
      }
    } else {
      this.vy = (ground - this.y) / dt;
      this.y = ground;
    }
    this.airborne = this.y > ground + 0.15;

    fx = Math.sin(this.heading);
    fz = Math.cos(this.heading);
    if (!this.airborne) {
      const half = this.wheelBase / 2;
      const hFront = world.heightAt(this.x + fx * half, this.z + fz * half);
      const hRear = world.heightAt(this.x - fx * half, this.z - fz * half);
      const target = -Math.atan2(hFront - hRear, this.wheelBase);
      this.pitch += (target - this.pitch) * Math.min(1, dt * 20);
      // Lean with the ground across the car too, like on a banked turn. The left is (fz, -fx).
      const side = this.width / 2;
      const hLeft = world.heightAt(this.x + fz * side, this.z - fx * side);
      const hRight = world.heightAt(this.x - fz * side, this.z + fx * side);
      const rollTarget = Math.atan2(hLeft - hRight, this.width);
      this.groundRoll += (rollTarget - this.groundRoll) * Math.min(1, dt * 20);
    } else {
      const target = -Math.atan2(this.vy, Math.max(4, Math.abs(vf))) * 0.6;
      this.pitch += (target - this.pitch) * Math.min(1, dt * 2);
      this.groundRoll -= this.groundRoll * Math.min(1, dt * 2);
    }

    events.cones = world.hitCones(this.x, this.z, this.y, this.width / 2 + 0.2, this.vx, this.vz);

    // Final speeds and body lean for the visuals.
    vf = this.vx * fx + this.vz * fz;
    vl = this.vx * fz - this.vz * fx;
    const longAccel = (vf - this.lastForward) / dt;
    this.lastForward = vf;
    this.forwardSpeed = vf;
    this.lateralSpeed = vl;
    const latAccel = this.airborne ? 0 : vf * yawRate;
    const rollTarget = Math.max(-0.1, Math.min(0.1, latAccel * 0.008));
    const pitchTarget = this.airborne ? 0 : Math.max(-0.06, Math.min(0.06, -longAccel * 0.003));
    this.roll += (rollTarget - this.roll) * Math.min(1, dt * 6);
    this.bodyPitch += (pitchTarget - this.bodyPitch) * Math.min(1, dt * 6);
    CarPhysics.dynamics?.pose?.(this, dt);
    return events;
  }

  /**
   * The arcade driving model: one step of engine, brakes, steering and grip.
   * Updates the velocity and heading, and returns the turn rate (rad/s).
   */
  private arcade(dt: number, input: CarInput, world: World, vf: number, vl: number, fx: number, fz: number, control: number): number {
    // Steering: smoothed, and less lock at high speed.
    const steerTarget = input.steer * this.maxSteer / (1 + Math.abs(vf) / 28);
    this.steerAngle += (steerTarget - this.steerAngle) * Math.min(1, dt * 10);

    // The gearbox. An automatic picks the gear for the speed, changing up at
    // each gear's redline. Revs grow with speed through each gear.
    if (!this.manual) {
      this.gear = 1;
      while (this.gear < GEAR_TOPS.length && Math.abs(vf) > GEAR_TOPS[this.gear - 1] * this.topSpeed) this.gear++;
    }
    this.revs = vf < 0 ? -vf / MAX_REVERSE : vf / (GEAR_TOPS[this.gear - 1] * this.topSpeed);
    // How hard the engine pulls in this gear. An automatic always uses the
    // right gear, so it always pulls fully. In a manual, a high gear below
    // about half revs bogs down (first gear never does: the clutch slips), and
    // at the redline the rev limiter cuts the power until you change up.
    // Changing up at the redline lands at least 0.53 up the next gear's revs,
    // so it is exactly as quick as the automatic; changing up early is slower.
    let pull = 1;
    if (this.manual) {
      if (this.revs >= 1) pull = 0;
      else if (this.gear > 1) pull = 0.12 + 0.88 * Math.min(1, this.revs / 0.53) ** 2;
    }
    this.limiter = this.manual && this.revs >= 1 && input.throttle > 0;

    // Throttle, brake and reverse.
    this.braking = false;
    if (input.throttle > 0) {
      if (vf < -0.5) {
        vf = Math.min(0, vf + this.brakeDecel * input.throttle * dt * control);
        this.braking = true;
      } else {
        const top = this.topSpeed * (1 + 0.08 * this.draft);
        const headroom = Math.max(0, 1 - (vf / top) ** 2);
        vf += this.accel * input.throttle * headroom * pull * dt * control;
      }
    }
    // Nitro: extra push and a higher top speed while the bottle lasts. A
    // turbo (drift boost, rocket start) does the same for free, for a moment.
    const power = this.useBoost(dt, input, vf);
    if (power > 0) {
      const top = this.topSpeed * (1 + 0.35 * power);
      const headroom = Math.max(0, 1 - (vf / top) ** 2);
      vf += this.accel * 1.6 * power * headroom * dt * (this.airborne ? Math.max(0.4, this.airControl) : 1);
    }
    if (input.brake > 0) {
      if (vf > 0.5) {
        vf = Math.max(0, vf - this.brakeDecel * input.brake * dt * control);
        this.braking = true;
      } else if (vf > -MAX_REVERSE) {
        vf -= this.accel * 0.6 * input.brake * dt * control;
      }
    }
    if (input.handbrake && !this.airborne) {
      const s = Math.sign(vf);
      vf -= s * Math.min(Math.abs(vf), 7 * dt);
      this.braking = true;
    }
    // Air drag and rolling resistance. On the throttle, air drag is already
    // part of the engine's fade-out near top speed above, so it isn't added
    // again; that way the car really reaches the top speed set in the garage.
    if (!this.airborne) {
      const coast = input.throttle === 0 && input.brake === 0 ? 1.2 : 0.3;
      const air = input.throttle > 0 ? 0 : Math.abs(vf) * 0.012;
      const s = Math.sign(vf);
      vf -= s * Math.min(Math.abs(vf), (coast + air) * dt);
    }
    // Grass beside a race track drags the car back: a normal car manages about 75 mph on it.
    this.feelSurface(world);
    if (this.onGrass) {
      const s = Math.sign(vf);
      const drag = (0.5 + Math.abs(vf) * 0.08) * (this.allTerrain ? 0.5 : 1);
      vf -= s * Math.min(Math.abs(vf), drag * dt);
    }

    // Downforce presses the car onto the road harder the faster it goes,
    // adding grip: none at a standstill, double the grip at 10/10 and 100 mph.
    const downforceGrip = 1 + (this.downforce / 10) * (vf / (100 * MPH)) ** 2;

    // Sideways grip: low grip with the handbrake lets the car slide.
    if (!this.airborne) {
      const g = (input.handbrake ? this.grip * this.driftGrip : this.grip) * downforceGrip * this.surfaceGrip;
      vl *= Math.exp(-g * dt);
    }

    // Turning, limited to what the tires can hold: too fast for a corner and
    // the car runs wide. The handbrake lets it swing around further and slide.
    let yawRate = (vf / this.wheelBase) * Math.tan(this.steerAngle) * control;
    if (input.handbrake && Math.abs(vf) > 3) yawRate *= 1.5;
    const maxYaw = (this.cornerGrip(vf) * (input.handbrake ? DRIFT_TURN : 1)) / Math.max(1, Math.abs(vf));
    yawRate = Math.max(-maxYaw, Math.min(maxYaw, yawRate));
    const dYaw = yawRate * dt;
    this.heading += dYaw;

    // Rebuild velocity. Grippy tires carry most of the speed around the turn;
    // a sliding car keeps going the old way, which makes it drift.
    const carry = input.handbrake ? 0.25 : 0.85;
    const oldFx = fx;
    const oldFz = fz;
    let vx = oldFx * vf + oldFz * vl;
    let vz = oldFz * vf - oldFx * vl;
    if (!this.airborne) {
      const a = dYaw * carry;
      const c = Math.cos(a);
      const s = Math.sin(a);
      [vx, vz] = [vx * c + vz * s, -vx * s + vz * c];
    }
    this.vx = vx;
    this.vz = vz;
    return yawRate;
  }

  /**
   * Nitro and turbo: uses up the bottle (or the free turbo) and returns how
   * strong the boost is this step, or 0 when not boosting. Sets `boosting`.
   */
  useBoost(dt: number, input: CarInput, forwardSpeed: number): number {
    const nitroOn = input.boost && this.nitro > 0;
    const turboOn = this.turbo > 0;
    this.turbo = Math.max(0, this.turbo - dt);
    this.boosting = (nitroOn || turboOn) && forwardSpeed > -0.5;
    if (!this.boosting) return 0;
    if (nitroOn && !this.infiniteNitro) this.nitro = Math.max(0, this.nitro - this.nitroBurn * dt);
    return Math.max(nitroOn ? this.nitroPower : 0, turboOn ? this.turboPower : 0);
  }

  /** Looks at the ground under the car: grass beside a track, and how grippy it is. */
  feelSurface(world: World): void {
    this.onGrass = !this.airborne && world.offRoad(this.x, this.z);
    this.surfaceGrip = world.gripAt(this.x, this.z);
    if (this.allTerrain) this.surfaceGrip = 1 - (1 - this.surfaceGrip) * 0.4;
  }

  /**
   * Pushes the car out of walls, buildings and posts. Returns speed lost.
   * A giant car (see GIANT_HEIGHT) flattens buildings, parked cars, lamps and
   * trees instead, and steps over race track barriers less than half its
   * height; only the map's edge walls still stop it.
   */
  private collide(world: World, events: StepEvents): number {
    const fx = Math.sin(this.heading);
    const fz = Math.cos(this.heading);
    // The car's footprint is a grid of circles as wide as its shorter side
    // (usually the width). A normal car gets a single row of them from the
    // front to the back; a car wider than it is long gets a row from side to
    // side instead. Either way no circle sticks out past the body.
    const r = Math.min(this.length, this.width) / 2;
    const alongRoom = this.length / 2 - r; // how far the circle centers spread forwards and back
    const acrossRoom = this.width / 2 - r; // ...and to the sides
    const alongCount = alongRoom > 0.01 ? Math.max(3, Math.ceil(this.length / (2 * r)) + 1) : 1;
    const acrossCount = acrossRoom > 0.01 ? Math.max(3, Math.ceil(this.width / (2 * r)) + 1) : 1;
    const giant = this.height >= GIANT_HEIGHT;
    const direction = new THREE.Vector3(this.vx, 0, this.vz).normalize();
    // Only the posts and walls near the car (the grid lists them by area).
    const near = world.nearby(this.x, this.z, Math.max(this.length, this.width) / 2 + 1);
    let impact = 0;
    const hit = (lost: number, target: Target | undefined) => {
      if (lost > impact) {
        impact = lost;
        events.hitTarget = target ?? null;
      }
    };
    // Spreads `count` circles evenly from -room to +room (just 0 for one circle).
    const spread = (k: number, count: number, room: number) => (count === 1 ? 0 : room * (-1 + (2 * k) / (count - 1)));
    for (let k = 0; k < alongCount * acrossCount; k++) {
      const along = spread(k % alongCount, alongCount, alongRoom);
      const across = spread(Math.floor(k / alongCount), acrossCount, acrossRoom);
      // Forwards is (fx, fz); to the side is (fz, -fx).
      const cx = this.x + fx * along + fz * across;
      const cz = this.z + fz * along - fx * across;
      for (const b of world.boxes) {
        if (!b.solid) continue;
        // Flying over something low (a parked car) misses it. Walls always stop you.
        if (b.target && this.y > b.height) continue;
        const px = Math.max(b.minX, Math.min(cx, b.maxX));
        const pz = Math.max(b.minZ, Math.min(cz, b.maxZ));
        if ((cx - px) ** 2 + (cz - pz) ** 2 >= r * r) continue;
        if (b.target && giant && this.height * 2 >= b.height) {
          world.damage(b.target, 1e6, direction);
          events.smashed++;
          continue;
        }
        hit(this.resolve(cx, cz, px, pz, r), b.target);
      }
      // Race track barriers and grandstands: straight walls at any angle. A
      // giant car steps over any wall less than half its height.
      const reach = r + WALL_THICKNESS / 2;
      for (const w of near.walls) {
        if (this.y > w.top || (giant && w.top - this.y < this.height / 2)) continue;
        if (cx + reach < w.minX || cx - reach > w.maxX || cz + reach < w.minZ || cz - reach > w.maxZ) continue;
        // The closest point on the wall's middle line.
        const sx = w.bx - w.ax;
        const sz = w.bz - w.az;
        const t = Math.max(0, Math.min(1, ((cx - w.ax) * sx + (cz - w.az) * sz) / (sx * sx + sz * sz || 1)));
        const px = w.ax + sx * t;
        const pz = w.az + sz * t;
        if ((cx - px) ** 2 + (cz - pz) ** 2 >= reach * reach) continue;
        hit(this.resolve(cx, cz, px, pz, reach), undefined);
      }
      for (const c of near.circles) {
        if (!c.solid) continue;
        // Flying over a lamp post or tree misses it.
        if (this.y > c.height) continue;
        const dx = cx - c.x;
        const dz = cz - c.z;
        const reach = r + c.r;
        if (dx * dx + dz * dz >= reach * reach) continue;
        // Hit fast enough (or by a giant), lamps and trees snap and the car keeps going.
        if (c.target && world.ram(c.target, giant ? Infinity : this.speed, direction)) {
          if (!giant) {
            this.vx *= 0.9;
            this.vz *= 0.9;
          }
          events.smashed++;
          continue;
        }
        if (giant && c.height < this.height / 2) continue; // steps over posts that aren't targets, like the start gantry's
        const d = Math.hypot(dx, dz) || 0.001;
        // Closest point on the post's edge, so resolve() treats it like a box corner.
        hit(this.resolve(cx, cz, c.x + (dx / d) * c.r, c.z + (dz / d) * c.r, r), c.target);
      }
    }
    return impact;
  }

  private resolve(cx: number, cz: number, px: number, pz: number, r: number): number {
    let dx = cx - px;
    let dz = cz - pz;
    const distSq = dx * dx + dz * dz;
    if (distSq >= r * r) return 0;
    let d = Math.sqrt(distSq);
    if (d < 1e-4) {
      // Center is inside the obstacle: push straight back the way we came.
      const s = Math.hypot(this.vx, this.vz) || 1;
      dx = -this.vx / s;
      dz = -this.vz / s;
      d = 0;
    } else {
      dx /= d;
      dz /= d;
    }
    const push = r - d;
    this.x += dx * push;
    this.z += dz * push;
    const vn = this.vx * dx + this.vz * dz;
    if (vn >= 0) return 0;
    // Remove the speed into the wall, plus some more to bounce back off it.
    this.vx -= vn * dx * (1 + this.bounce);
    this.vz -= vn * dz * (1 + this.bounce);
    this.vx *= 0.85;
    this.vz *= 0.85;
    if (CarPhysics.dynamics?.impact) {
      // Where on the car it hit, so the pack can spin the car from a glancing blow.
      const fx = Math.sin(this.heading);
      const fz = Math.cos(this.heading);
      const along = (cx - this.x) * fx + (cz - this.z) * fz;
      const across = (cx - this.x) * fz - (cz - this.z) * fx;
      CarPhysics.dynamics.impact(this, dx, dz, along, across, -vn);
    }
    return -vn;
  }
}
