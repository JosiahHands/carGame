// Arcade car physics: acceleration, braking, steering, grip, handbrake drifts,
// jumps off ramps and bumping into things. Hand-written, no physics engine.
import * as THREE from 'three';
import type { CarConfig } from './carConfig';
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
const MAX_REVERSE = 12;
const MPH = 0.44704; // metres per second
/**
 * Most sideways acceleration the tyres can give, in m/s², at normal grip.
 * Far more than a real car, but it stops cars at hundreds of mph from spinning
 * on the spot; normal driving never reaches it.
 */
const CORNERING_LIMIT = 250;
/** A car this tall (metres) or taller flattens anything up to twice its height. */
const GIANT_HEIGHT = 3;

export class CarPhysics {
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
  forwardSpeed = 0;
  lateralSpeed = 0;
  braking = false;
  /** Nitro left in the bottle, 0..1. */
  nitro = 1;
  boosting = false;

  // Set from the car's tuning by configure().
  private accel = 9; // m/s² at full throttle from a standstill
  private topSpeed = 50; // m/s
  private gripMultiplier = 1;
  private grip = 7; // how quickly sideways sliding dies away
  private brakeDecel = 20;
  private maxSteer = 0.5; // radians
  private driftGrip = 0.25;
  private downforce = 0;
  private nitroPower = 1;
  private nitroBurn = 0.3; // bottle used per second
  private infiniteNitro = false;
  private jumpSpeed = 0; // m/s upwards
  private airControl = 0.15;
  private gravity = EARTH_GRAVITY;
  private bounce = 0.35;
  // Set from the car's size by configure().
  private wheelBase = 2.6;
  private length = 4.4;
  private width = 1.9;
  private height = 1.3;
  private lastForward = 0;
  private jumpHeld = false;

  configure(config: CarConfig, dims: { length: number; width: number; height: number; wheelBase: number }): void {
    const t = config.tuning;
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

  reset(x: number, z: number, heading: number): void {
    this.x = x;
    this.z = z;
    this.y = 0;
    this.heading = heading;
    this.vx = this.vz = this.vy = 0;
    this.steerAngle = this.roll = this.bodyPitch = this.pitch = 0;
    this.airborne = false;
    this.nitro = 1;
    this.boosting = false;
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

  /** 0..1, how close to top speed. Used for the engine sound. */
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

    // Steering: smoothed, and less lock at high speed.
    const steerTarget = input.steer * this.maxSteer / (1 + Math.abs(vf) / 28);
    this.steerAngle += (steerTarget - this.steerAngle) * Math.min(1, dt * 10);

    // Throttle, brake and reverse.
    this.braking = false;
    if (input.throttle > 0) {
      if (vf < -0.5) {
        vf = Math.min(0, vf + this.brakeDecel * input.throttle * dt * control);
        this.braking = true;
      } else {
        const headroom = Math.max(0, 1 - (vf / this.topSpeed) ** 2);
        vf += this.accel * input.throttle * headroom * dt * control;
      }
    }
    // Nitro: extra push and a higher top speed while the bottle lasts.
    this.boosting = input.boost && this.nitro > 0 && vf > -0.5;
    if (this.boosting) {
      if (!this.infiniteNitro) this.nitro = Math.max(0, this.nitro - this.nitroBurn * dt);
      const top = this.topSpeed * (1 + 0.35 * this.nitroPower);
      const headroom = Math.max(0, 1 - (vf / top) ** 2);
      vf += this.accel * 1.6 * this.nitroPower * headroom * dt * (this.airborne ? Math.max(0.4, this.airControl) : 1);
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

    // Downforce presses the car onto the road harder the faster it goes,
    // adding grip: none at a standstill, double the grip at 10/10 and 100 mph.
    const downforceGrip = 1 + (this.downforce / 10) * (vf / (100 * MPH)) ** 2;

    // Sideways grip: low grip with the handbrake lets the car slide.
    if (!this.airborne) {
      const g = (input.handbrake ? this.grip * this.driftGrip : this.grip) * downforceGrip;
      vl *= Math.exp(-g * dt);
    }

    // Turning, limited to what the tyres can hold (only matters at extreme speed).
    let yawRate = (vf / this.wheelBase) * Math.tan(this.steerAngle) * control;
    if (input.handbrake && Math.abs(vf) > 3) yawRate *= 1.5;
    const maxYaw = (CORNERING_LIMIT * this.gripMultiplier * downforceGrip) / Math.max(1, Math.abs(vf));
    yawRate = Math.max(-maxYaw, Math.min(maxYaw, yawRate));
    const dYaw = yawRate * dt;
    this.heading += dYaw;

    // Rebuild velocity. Grippy tyres carry most of the speed round the turn;
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

    // Move, checking for crashes along the way. At hundreds of mph the car
    // moves several metres a step, so it moves in smaller pieces then, or it
    // could pass straight through a lamp post.
    const pieces = Math.min(12, Math.max(1, Math.ceil((this.speed * dt) / (this.width * 0.4))));
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
    } else {
      const target = -Math.atan2(this.vy, Math.max(4, Math.abs(vf))) * 0.6;
      this.pitch += (target - this.pitch) * Math.min(1, dt * 2);
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
    return events;
  }

  /**
   * Pushes the car out of walls, buildings and posts. Returns speed lost.
   * A giant car (see GIANT_HEIGHT) flattens buildings, parked cars, lamps and
   * trees instead; only the city's edge walls still stop it.
   */
  private collide(world: World, events: StepEvents): number {
    const fx = Math.sin(this.heading);
    const fz = Math.cos(this.heading);
    const r = this.width / 2;
    const giant = this.height >= GIANT_HEIGHT;
    const direction = new THREE.Vector3(this.vx, 0, this.vz).normalize();
    let impact = 0;
    const hit = (lost: number, target: Target | undefined) => {
      if (lost > impact) {
        impact = lost;
        events.hitTarget = target ?? null;
      }
    };
    // The car's footprint is a row of circles, as wide as the car, from the
    // front to the back. Long cars get more of them, so there are no gaps.
    const circles = Math.max(3, Math.ceil(this.length / this.width) + 1);
    const offset = Math.max(0, this.length / 2 - r);
    for (let k = 0; k < circles; k++) {
      const along = offset * (-1 + (2 * k) / (circles - 1));
      const cx = this.x + fx * along;
      const cz = this.z + fz * along;
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
      for (const c of world.circles) {
        if (!c.solid) continue;
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
      // Centre is inside the obstacle: push straight back the way we came.
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
    return -vn;
  }
}
