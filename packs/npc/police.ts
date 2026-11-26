// BUDDY's police car: a real car, with the same physics as yours, driven
// the way the game's rival drivers drive (aim at a point ahead, brake in time
// for corners), with flashing lights and a siren.
//
// It drives him (and GUY, riding along) around on patrol, and when BUDDY
// decides you've gone too far, it chases you: siren on, ramming, and
// shooting at you with its roof gun the way combat rivals do. You can shoot
// back: it's a hit box for your weapons, and it can be wrecked.
import * as THREE from 'three';
import { DEFAULT_CONFIG, type CarConfig } from '../../game/carConfig';
import { buildCar, type CarRig } from '../../game/carModel';
import { CarPhysics, type CarInput } from '../../game/carPhysics';
import type { EngineAudio } from '../../game/engineAudio';
import type { HitBox, World } from '../../game/world';
import { routeTo, type Waypoint } from './drive';

const POLICE_CONFIG: CarConfig = {
  ...DEFAULT_CONFIG,
  body: 'muscle',
  paintColor: '#f4f6fa',
  accentColor: '#10204a',
  pattern: 'twoTone',
  finish: 'gloss',
  stripes: 'none',
  spoiler: 'none',
  plate: 'POLICE',
  raceNumber: 0,
  roofGuns: true,
  // BUDDY's roof gun fires (he has no rockets).
  armed: { gun: true, rockets: false },
  bullBar: true,
  underglow: 'off',
  boostTrail: 'smoke',
  lightBar: false,
  tuning: { ...DEFAULT_CONFIG.tuning, horsepower: 520, topSpeed: 125 },
};

/** What happened this step, for the city to react to. */
export interface PoliceStep {
  /** Bullets that hit the player this step (damage, 0..). */
  shot: number;
  /** Fired a burst (for the sound and the witnesses). */
  fired: boolean;
  /** Rammed the player: how hard (m/s). */
  rammed: number;
  /** Reached the end of its route. */
  arrived: boolean;
}

export class PoliceCar {
  readonly physics = new CarPhysics();
  readonly rig: CarRig;
  readonly hitBox: HitBox;
  hp = 100;
  /** Seconds left of being a smoking wreck. */
  wrecked = 0;
  route: Waypoint[] = [];
  mode: 'parked' | 'route' | 'chase' = 'parked';
  siren = false;
  private lights: THREE.Mesh[] = [];
  private bar: THREE.Group | null = null;
  private flash = 0;
  private stuck = 0;
  private reversing = 0;
  /** Where the route's current leg starts. */
  private from = { x: 0, z: 0 };
  /** In a chase: when to look for a way to you again, and whether you're in plain sight. */
  private repath = 0;
  private direct = true;
  /** Seconds left of following the roads only (after getting stuck cutting across). */
  private roadOnly = 0;
  private gunWait = 3;
  private burst = 0;
  private sound: { osc: OscillatorNode; gain: GainNode } | null = null;

  constructor(
    private scene: THREE.Scene,
    private world: World,
    private audio: EngineAudio,
    night: boolean,
  ) {
    this.rig = buildCar(POLICE_CONFIG, night, true);
    this.physics.configure(POLICE_CONFIG, this.rig);
    scene.add(this.rig.root);
    // The light bar: red and blue, flashing in turn when the siren's on.
    const bar = new THREE.Group();
    const base = new THREE.Mesh(new THREE.BoxGeometry(1.1, 0.12, 0.3), new THREE.MeshStandardMaterial({ color: 0x15171c, roughness: 0.5 }));
    bar.add(base);
    for (const [x, color] of [
      [-0.3, 0xff2030],
      [0.3, 0x2060ff],
    ] as const) {
      const lamp = new THREE.Mesh(new THREE.BoxGeometry(0.42, 0.14, 0.26), new THREE.MeshStandardMaterial({ color, emissive: color, emissiveIntensity: 0.2, roughness: 0.3 }));
      lamp.position.set(x, 0.1, 0);
      bar.add(lamp);
      this.lights.push(lamp);
    }
    // On the roof: rig.height is in world meters, the body's own units are before the car's size.
    bar.position.set(0, this.rig.height / POLICE_CONFIG.size - this.rig.body.position.y + 0.06, -0.2);
    this.rig.body.add(bar);
    this.bar = bar;
    this.hitBox = {
      x: 0,
      y: 0,
      z: 0,
      half: 1.4,
      height: 1.6,
      alive: false,
      onHit: (damage) => {
        if (this.wrecked > 0) return;
        this.hp -= damage * 0.45;
        this.onHit?.(damage);
      },
    };
  }

  /** Told when your weapons hit it. */
  onHit: ((damage: number) => void) | null = null;

  /** Puts the car somewhere, standing still. */
  place(x: number, z: number, heading: number): void {
    this.physics.reset(x, z, heading);
    this.physics.y = this.world.heightAt(x, z);
    this.stuck = 0;
  }

  /** Shown, and hittable by your weapons, or put away. */
  setActive(on: boolean): void {
    this.rig.root.visible = on;
    this.hitBox.alive = on && this.wrecked <= 0;
    const boxes = this.world.hitBoxes;
    const i = boxes.indexOf(this.hitBox);
    if (on && i < 0) boxes.push(this.hitBox);
    if (!on && i >= 0) boxes.splice(i, 1);
    if (!on) this.setSiren(false);
  }

  /** Blown up: a smoking wreck for a while. */
  wreck(): void {
    this.wrecked = 8;
    this.hp = 0;
    this.hitBox.alive = false;
    this.rig.setWrecked(true);
    this.setSiren(false);
    this.physics.push(0, 7, 0);
  }

  get x(): number {
    return this.physics.x;
  }
  get z(): number {
    return this.physics.z;
  }

  /**
   * Drives one step: along its route, or after `player` in a chase.
   * `unseen` lets it hurry when nobody's watching.
   */
  update(dt: number, player: CarPhysics, playerWrecked: boolean): PoliceStep {
    const step: PoliceStep = { shot: 0, fired: false, rammed: 0, arrived: false };
    const p = this.physics;
    const input: CarInput = { throttle: 0, brake: 0, steer: 0, handbrake: false, boost: false, jump: false };
    if (this.wrecked > 0) {
      this.wrecked -= dt;
      this.hold(input);
      p.step(dt, input, this.world);
      this.sync(dt);
      if (this.wrecked <= 0) {
        this.rig.setWrecked(false);
        this.hp = 100;
        this.hitBox.alive = this.rig.root.visible;
      }
      return step;
    }
    let tx: number;
    let tz: number;
    let wanted: number;
    /** Pulled up beside you (you've stopped): waiting, not stuck. */
    let waiting = false;
    if (this.mode === 'chase') {
      // Aim where you're going to be, a moment from now.
      const lead = Math.min(1.2, Math.hypot(player.x - p.x, player.z - p.z) / 40);
      const ax = player.x + player.vx * lead;
      const az = player.z + player.vz * lead;
      const distance = Math.hypot(ax - p.x, az - p.z);
      const yours = Math.hypot(player.vx, player.vz);
      // Straight at you if nothing's in the way; otherwise round the block, along the roads.
      this.repath -= dt;
      if (this.repath <= 0) {
        this.repath = 0.8;
        // Close by, only the line itself has to be clear (not a car's width either side of it).
        this.direct = this.roadOnly <= 0 && distance < 140 && this.clearLine(p.x, p.z, ax, az, distance < 18 ? 0 : 2.2);
        if (!this.direct) this.setRoute(routeTo(p.x, p.z, p.heading, player.x, player.z));
      }
      if (this.direct && yours < 3 && distance < 45) {
        // You've stopped: pull up a few meters from you, and wait.
        tx = player.x;
        tz = player.z;
        wanted = Math.sqrt(2 * 4 * Math.max(0, distance - 8));
        waiting = distance < 9;
      } else if (this.direct || !this.route.length) {
        tx = ax;
        tz = az;
        // Fast when far (but not reckless in town), matching your speed (and a bit, to ram) when close.
        wanted = distance > 30 ? Math.min(p.topSpeed, Math.max(32, yours + 10)) : yours + 6;
      } else {
        const ahead = this.followRoute(1.3, Math.min(12, yours + 4));
        tx = ahead.x;
        tz = ahead.z;
        wanted = Math.max(ahead.speed, 8);
      }
      // Nitro to catch up on a straight.
      input.boost = distance > 70 && Math.abs(this.angleTo(tx, tz)) < 0.3;
    } else if (this.mode === 'route' && this.route.length) {
      const ahead = this.followRoute(1);
      tx = ahead.x;
      tz = ahead.z;
      wanted = Math.min(16, ahead.speed);
      if (ahead.done) {
        this.mode = 'parked';
        step.arrived = true;
      }
    } else {
      this.hold(input);
      p.step(dt, input, this.world);
      this.sync(dt);
      return step;
    }

    // Steer for the arc that reaches the target (pure pursuit, as the rivals do).
    const angle = this.angleTo(tx, tz);
    const distance = Math.hypot(tx - p.x, tz - p.z) || 1;
    const speed = Math.max(0, p.forwardSpeed);
    if (this.reversing > 0) {
      // Backing out of a dead end: reverse with the wheels turned the other way.
      this.reversing -= dt;
      input.brake = 1;
      input.steer = -Math.sign(angle);
    } else if (waiting) {
      this.hold(input);
    } else if (Math.abs(angle) > 2 && speed < 6 && this.mode === 'chase') {
      // You're behind: turn round (a three-point turn, the quick way).
      input.throttle = 0.6;
      input.steer = Math.sign(angle);
      input.handbrake = speed > 3;
    } else {
      input.steer = p.steerFor((2 * Math.sin(angle)) / distance);
      const curve = Math.abs((2 * Math.sin(angle)) / Math.max(8, distance));
      wanted = Math.min(wanted, p.cornerSpeed(curve) * 1.1);
      if (speed < wanted - 0.5) input.throttle = 1;
      else if (speed < wanted + 0.5) input.throttle = 0.3;
      if (speed > wanted + 1.5) input.brake = Math.min(1, (speed - wanted) / 6 + 0.3);
    }
    p.step(dt, input, this.world);
    if (!input.boost) p.nitro = Math.min(1, p.nitro + dt * 0.08);

    // Stuck against something: back out, then keep to the roads for a bit.
    this.roadOnly -= dt;
    this.stuck = !waiting && p.speed < 1.5 && (input.throttle > 0.5 || this.mode === 'chase') ? this.stuck + dt : 0;
    if (this.stuck > 1.5 && this.reversing <= 0) {
      this.reversing = 2;
      this.stuck = 0;
      this.roadOnly = 5;
      this.repath = 0;
    }

    if (this.mode === 'chase' && !playerWrecked) {
      step.rammed = this.ram(player);
      const shot = this.guns(dt, player);
      step.shot = shot.damage;
      step.fired = shot.fired;
    }
    this.sync(dt);
    return step;
  }

  /**
   * Stops and stays stopped. Holding the brake at a standstill would reverse
   * (that's how the game's cars back up), so the brakes are used only while
   * rolling forward; then the handbrake holds it, and a car rolling backwards
   * is stopped with the throttle (which brakes when going backwards).
   */
  private hold(input: CarInput): void {
    const forward = this.physics.forwardSpeed;
    input.throttle = forward < -0.3 ? Math.min(1, -forward / 3) : 0;
    input.brake = forward > 0.5 ? 1 : 0;
    input.handbrake = forward <= 0.5;
  }

  /** The angle from the car's nose to a point, -π..π (positive: to the left). */
  private angleTo(x: number, z: number): number {
    const p = this.physics;
    const a = Math.atan2(x - p.x, z - p.z) - p.heading;
    return Math.atan2(Math.sin(a), Math.cos(a));
  }

  /** Starts following a route (from where the car is now). */
  setRoute(route: Waypoint[]): void {
    this.route = route;
    this.from = { x: this.physics.x, z: this.physics.z };
  }

  /**
   * Follows the route: a point a little way ahead along it to steer for
   * (so corners are taken along the lanes, not cut across the blocks), and
   * how fast it can go to take the corners ahead (`hurry` × their speeds;
   * in a chase, `endSpeed` at the end, where you are).
   */
  private followRoute(hurry: number, endSpeed = 0): { x: number; z: number; speed: number; done: boolean } {
    const p = this.physics;
    // Passed the next point (or close enough): on to the one after.
    while (this.route.length) {
      const b = this.route[0];
      const sx = b.x - this.from.x;
      const sz = b.z - this.from.z;
      const len = Math.hypot(sx, sz) || 1;
      const t = ((p.x - this.from.x) * sx + (p.z - this.from.z) * sz) / (len * len);
      const close = Math.hypot(b.x - p.x, b.z - p.z) < (this.route.length === 1 ? 3 : 2);
      if (t < 1 && !close) break;
      this.from = { x: b.x, z: b.z };
      this.route.shift();
    }
    if (!this.route.length) return { x: this.from.x, z: this.from.z, speed: 0, done: true };
    // The point `look` meters on from the car's place on the route.
    const look = 4 + Math.max(0, p.forwardSpeed) * 0.45;
    const first = this.route[0];
    let ax = this.from.x;
    let az = this.from.z;
    {
      const sx = first.x - ax;
      const sz = first.z - az;
      const len2 = sx * sx + sz * sz || 1;
      const t = Math.max(0, Math.min(1, ((p.x - ax) * sx + (p.z - az) * sz) / len2));
      ax += sx * t;
      az += sz * t;
    }
    let left = look;
    let target = { x: first.x, z: first.z };
    for (const w of this.route) {
      const d = Math.hypot(w.x - ax, w.z - az);
      if (d >= left) {
        target = { x: ax + ((w.x - ax) / d) * left, z: az + ((w.z - az) / d) * left };
        left = 0;
        break;
      }
      left -= d;
      ax = w.x;
      az = w.z;
      target = { x: w.x, z: w.z };
    }
    // Slow enough for each of the next few points, braking at a comfortable 5 m/s².
    let speed = Infinity;
    let reach = Math.hypot(first.x - p.x, first.z - p.z);
    for (let i = 0; i < Math.min(4, this.route.length); i++) {
      const w = this.route[i];
      if (i > 0) reach += Math.hypot(w.x - this.route[i - 1].x, w.z - this.route[i - 1].z);
      const at = i === this.route.length - 1 && this.mode === 'chase' ? endSpeed : w.speed * hurry;
      speed = Math.min(speed, Math.sqrt(at * at + 2 * 5 * Math.max(0, reach - (w.speed === 0 ? 0 : 2))));
    }
    return { ...target, speed: Math.max(speed, this.route.length === 1 ? 2 : 0), done: false };
  }

  /**
   * Whether nothing solid (a building, a wall) stands between two points,
   * keeping `margin` meters clear of it. Buildings that can be destroyed
   * still count while they stand. Parked cars don't: BUDDY sees and shoots
   * over them, and the cruiser shoves past them.
   */
  private clearLine(ax: number, az: number, bx: number, bz: number, margin: number): boolean {
    const steps = Math.ceil(Math.hypot(bx - ax, bz - az) / (margin > 0 ? 4 : 1.5));
    for (let i = 1; i < steps; i++) {
      const x = ax + ((bx - ax) * i) / steps;
      const z = az + ((bz - az) * i) / steps;
      for (const b of this.world.boxes) {
        if (b.solid && b.target?.kind !== 'car' && x > b.minX - margin && x < b.maxX + margin && z > b.minZ - margin && z < b.maxZ + margin) return false;
      }
    }
    return true;
  }

  /** When the view was last checked, and what it showed. */
  private sightWait = 0;
  private sight = false;

  /**
   * Whether BUDDY can see a point from the car: close enough, and no building
   * in between. Checked a few times a second.
   */
  canSee(x: number, z: number, dt: number): boolean {
    this.sightWait -= dt;
    if (this.sightWait <= 0) {
      this.sightWait = 0.25;
      this.sight = Math.hypot(x - this.x, z - this.z) < 120 && this.clearLine(this.x, this.z, x, z, 0);
    }
    return this.sight;
  }

  /** Bumping into you: both cars pushed apart, and how hard it was. */
  private ram(player: CarPhysics): number {
    const a = this.physics;
    const dx = a.x - player.x;
    const dz = a.z - player.z;
    const d = Math.hypot(dx, dz);
    const reach = (a.length + player.length) / 2 - 0.6;
    if (d >= reach || d < 0.01 || Math.abs(a.y - player.y) > 1.5) return 0;
    const nx = dx / d;
    const nz = dz / d;
    const push = (reach - d) / 2;
    a.x += nx * push;
    a.z += nz * push;
    player.x -= nx * push;
    player.z -= nz * push;
    const closing = (a.vx - player.vx) * nx + (a.vz - player.vz) * nz;
    if (closing >= 0) return 0;
    const kick = (-closing * 1.3) / 2;
    a.vx += nx * kick;
    a.vz += nz * kick;
    player.vx -= nx * kick;
    player.vz -= nz * kick;
    return -closing;
  }

  /** Bursts from the roof gun when you're in front of it, like the combat rivals. */
  private guns(dt: number, player: CarPhysics): { damage: number; fired: boolean } {
    this.gunWait -= dt;
    if (this.gunWait > 0 || !this.firing) return { damage: 0, fired: false };
    const p = this.physics;
    const dx = player.x - p.x;
    const dz = player.z - p.z;
    const distance = Math.hypot(dx, dz);
    const facing = (Math.sin(p.heading) * dx + Math.cos(p.heading) * dz) / (distance || 1);
    // Only at you in plain sight: never through a building.
    if (distance > 85 || facing < 0.96 || !this.clearLine(p.x, p.z, player.x, player.z, 0)) {
      this.gunWait = 0.3;
      return { damage: 0, fired: false };
    }
    if (this.burst <= 0) this.burst = 6;
    this.burst--;
    this.gunWait = this.burst > 0 ? 0.12 : 2.2 + Math.random() * 2;
    const from = new THREE.Vector3(p.x + Math.sin(p.heading) * p.length * 0.5, p.y + p.height * 0.9, p.z + Math.cos(p.heading) * p.length * 0.5);
    const hits = Math.random() < 0.7 * (0.75 - distance / 300);
    const miss = hits ? 0 : 2 + Math.random() * 3;
    const to = new THREE.Vector3(player.x + (Math.random() - 0.5) * miss, player.y + 0.7, player.z + (Math.random() - 0.5) * miss);
    this.onShot?.(from, to, hits, distance);
    return { damage: hits ? 3 : 0, fired: true };
  }

  /** Allowed to shoot (only once BUDDY has warned you). */
  firing = false;
  /** Draws a shot (muzzle flash, tracer, sound): set by the city, which has the effects. */
  onShot: ((from: THREE.Vector3, to: THREE.Vector3, hit: boolean, distance: number) => void) | null = null;

  /** Moves the model to match the physics, and flashes the lights. */
  sync(dt: number): void {
    const p = this.physics;
    const rig = this.rig;
    rig.root.position.set(p.x, p.y, p.z);
    rig.root.rotation.set(p.pitch, p.heading, p.groundRoll, 'YXZ');
    rig.body.rotation.set(p.bodyPitch, 0, p.roll);
    for (const w of rig.wheels) {
      w.spin.rotation.x += (p.forwardSpeed / w.radius) * dt;
      if (w.front) w.steer.rotation.y = p.steerAngle;
    }
    rig.setLights({ night: this.world.isNight, braking: p.braking });
    rig.setNitro(p.boosting);
    this.hitBox.x = p.x;
    this.hitBox.y = p.y;
    this.hitBox.z = p.z;
    this.flash += dt;
    const phase = Math.floor(this.flash * 6) % 2;
    this.lights.forEach((lamp, i) => {
      (lamp.material as THREE.MeshStandardMaterial).emissiveIntensity = this.siren ? (phase === i ? 4 : 0.15) : 0.2;
    });
    this.updateSiren();
  }

  /** The siren: a wailing tone, quieter with distance (from the camera, set by the city). */
  setSiren(on: boolean): void {
    this.siren = on;
    if (!on && this.sound) {
      const s = this.sound;
      s.gain.gain.setTargetAtTime(0, s.osc.context.currentTime, 0.1);
      setTimeout(() => s.osc.stop(), 400);
      this.sound = null;
    }
  }

  /** How far the camera is, for the siren's volume. */
  hearing = 100;

  private updateSiren(): void {
    if (!this.siren) return;
    const ctx = this.audio.context;
    const output = this.audio.output;
    if (!ctx || !output) return;
    if (!this.sound) {
      const osc = ctx.createOscillator();
      osc.type = 'sawtooth';
      const filter = ctx.createBiquadFilter();
      filter.type = 'lowpass';
      filter.frequency.value = 1800;
      const gain = ctx.createGain();
      gain.gain.value = 0;
      osc.connect(filter).connect(gain).connect(output);
      osc.start();
      this.sound = { osc, gain };
    }
    // A wail: up and down twice a second... a classic.
    const t = ctx.currentTime;
    const wail = 0.5 + 0.5 * Math.sin(t * Math.PI * 1.1);
    this.sound.osc.frequency.setTargetAtTime(620 + wail * 520, t, 0.03);
    this.sound.gain.gain.setTargetAtTime(Math.max(0, 0.09 * (1 - this.hearing / 220)), t, 0.1);
  }

  dispose(): void {
    this.setSiren(false);
    this.setActive(false);
    this.bar?.traverse((o) => {
      if (o instanceof THREE.Mesh) {
        o.geometry.dispose();
        (o.material as THREE.Material).dispose();
      }
    });
    this.scene.remove(this.rig.root);
    this.rig.dispose();
  }
}
