// Arcade car physics: acceleration, braking, steering, grip, handbrake drifts,
// jumps off ramps and bumping into things. Hand-written, no physics engine.
import * as THREE from 'three';
import type { BodyShape, CarConfig } from './carConfig';
import type { Target, World } from './world';

export interface CarInput {
  throttle: number; // 0..1
  brake: number; // 0..1 (also reverse)
  steer: number; // -1 (right) .. 1 (left)
  handbrake: boolean;
  boost: boolean;
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

const GRAVITY = 16;
const MAX_REVERSE = 12;

// Small handling differences between body types.
const BODY_FEEL: Record<BodyShape, { accel: number; top: number; grip: number }> = {
  sports: { accel: 1.05, top: 1.05, grip: 1.05 },
  muscle: { accel: 1.12, top: 1.0, grip: 0.9 },
  hatch: { accel: 0.95, top: 0.92, grip: 1.08 },
  pickup: { accel: 0.88, top: 0.9, grip: 0.9 },
};

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

  private accel = 9;
  private topSpeed = 50;
  private grip = 7;
  private brakeDecel = 20;
  private maxSteer = 0.5;
  private driftGrip = 0.25;
  private nitroPower = 1;
  private nitroBurn = 0.3; // bottle used per second
  private wheelBase = 2.6;
  private length = 4.4;
  private width = 1.9;
  private lastForward = 0;

  configure(config: CarConfig, dims: { length: number; width: number; wheelBase: number }): void {
    const t = config.tuning;
    const feel = BODY_FEEL[config.body];
    this.accel = (5 + t.power * 0.9) * feel.accel;
    this.topSpeed = (28 + t.topSpeed * 5) * feel.top;
    this.grip = (2.5 + t.grip * 0.9) * feel.grip;
    this.brakeDecel = 12 + t.brakes * 2;
    this.maxSteer = 0.38 + t.steering * 0.03;
    this.driftGrip = Math.max(0.06, 0.45 - t.drift * 0.04);
    // Strong nitro pushes harder but empties the bottle faster.
    this.nitroPower = 0.6 + t.nitro * 0.12;
    this.nitroBurn = 0.18 + t.nitro * 0.025;
    this.wheelBase = dims.wheelBase;
    this.length = dims.length;
    this.width = dims.width;
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
    const control = this.airborne ? 0.15 : 1;

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
      this.nitro = Math.max(0, this.nitro - this.nitroBurn * dt);
      const top = this.topSpeed * (1 + 0.35 * this.nitroPower);
      const headroom = Math.max(0, 1 - (vf / top) ** 2);
      vf += this.accel * 1.6 * this.nitroPower * headroom * dt * (this.airborne ? 0.4 : 1);
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
    // Air drag and rolling resistance.
    if (!this.airborne) {
      const coast = input.throttle === 0 && input.brake === 0 ? 1.2 : 0.3;
      const s = Math.sign(vf);
      vf -= s * Math.min(Math.abs(vf), (coast + Math.abs(vf) * 0.012) * dt);
    }

    // Sideways grip: low grip with the handbrake lets the car slide.
    if (!this.airborne) {
      const g = input.handbrake ? this.grip * this.driftGrip : this.grip;
      vl *= Math.exp(-g * dt);
    }

    // Turning.
    let yawRate = (vf / this.wheelBase) * Math.tan(this.steerAngle) * control;
    if (input.handbrake && Math.abs(vf) > 3) yawRate *= 1.5;
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
    this.x += vx * dt;
    this.z += vz * dt;

    // Vertical: follow the ground, fly off ramp lips, land with gravity.
    const ground = world.heightAt(this.x, this.z);
    if (this.y > ground + 0.02) {
      this.vy -= GRAVITY * dt;
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

    events.impact = this.collide(world, events);
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

  /** Pushes the car out of walls, buildings and posts. Returns speed lost. */
  private collide(world: World, events: StepEvents): number {
    const fx = Math.sin(this.heading);
    const fz = Math.cos(this.heading);
    const r = this.width / 2;
    const offset = this.length / 2 - r;
    let impact = 0;
    const hit = (lost: number, target: Target | undefined) => {
      if (lost > impact) {
        impact = lost;
        events.hitTarget = target ?? null;
      }
    };
    for (const k of [-1, 0, 1]) {
      const cx = this.x + fx * offset * k;
      const cz = this.z + fz * offset * k;
      for (const b of world.boxes) {
        if (b.off) continue;
        // Flying over something low (a parked car) misses it. Walls always stop you.
        if (b.target && this.y > b.top) continue;
        const px = Math.max(b.minX, Math.min(cx, b.maxX));
        const pz = Math.max(b.minZ, Math.min(cz, b.maxZ));
        hit(this.resolve(cx, cz, px, pz, r), b.target);
      }
      for (const c of world.circles) {
        if (c.off) continue;
        const dx = cx - c.x;
        const dz = cz - c.z;
        const reach = r + c.r;
        if (dx * dx + dz * dz >= reach * reach) continue;
        // Fast enough, and lamps and trees just snap.
        if (c.target && world.ram(c.target, this.speed, new THREE.Vector3(this.vx, 0, this.vz).normalize())) {
          this.vx *= 0.9;
          this.vz *= 0.9;
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
    const bounce = 1.35;
    this.vx -= vn * dx * bounce;
    this.vz -= vn * dz * bounce;
    this.vx *= 0.85;
    this.vz *= 0.85;
    return -vn;
  }
}
