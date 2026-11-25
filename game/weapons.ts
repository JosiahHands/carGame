// The car's three weapons:
//   1. Machine gun: fast, weak, overheats if you hold the trigger too long.
//   2. Rockets: fly forward and explode, wrecking everything nearby.
//   3. Shockwave: a blast all around the car, then a few seconds to recharge.
// Which of them the car has, and how upgraded each one is, is its loadout:
// all three at level 1 in free roam and quick races, what you've bought in
// the career (see career.ts).
import * as THREE from 'three';
import type { CarRig } from './carModel';
import type { Effects } from './effects';
import type { EngineAudio } from './engineAudio';
import type { World } from './world';

export const WEAPON_NAMES = ['Machine gun', 'Rockets', 'Shockwave'];

const GUN_SHOTS_PER_SECOND = 14;
const GUN_DAMAGE = 9;
const GUN_RANGE = 300; // metres
const GUN_HEAT_PER_SHOT = 0.045; // overheats at 1, after about 2 seconds of firing
const GUN_COOLING = 0.45; // heat lost per second

const ROCKET_RELOAD = 0.45; // seconds between rockets
const ROCKET_SPEED = 85; // m/s
const ROCKET_LIFETIME = 3; // seconds before it blows up in mid-air
const ROCKET_RADIUS = 8; // metres
const ROCKET_DAMAGE = 170;

const SHOCKWAVE_RELOAD = 6;
const SHOCKWAVE_RADIUS = 16;
const SHOCKWAVE_DAMAGE = 150;

/** The highest level a weapon can be upgraded to. Level 1 is stock. */
export const MAX_WEAPON_LEVEL = 5;
/** Each level above 1: this much more damage... */
const DAMAGE_PER_LEVEL = 0.15;
/** ...and this much less gun heat per shot, and time to reload a rocket or recharge the shockwave. */
const QUICKER_PER_LEVEL = 0.08;

/** A weapon's level in each slot (machine gun, rockets, shockwave); 0 means the car doesn't have it. */
export type Loadout = [number, number, number];
export const NO_WEAPONS: Loadout = [0, 0, 0];
export const STOCK_WEAPONS: Loadout = [1, 1, 1];

/** An explosion near the player, so the game can shake the camera and push the car. */
export interface Blast {
  at: THREE.Vector3;
  radius: number;
}

interface Rocket {
  mesh: THREE.Group;
  velocity: THREE.Vector3;
  age: number;
  flying: boolean;
}

export class Weapons {
  /** Which weapon is selected: an index into WEAPON_NAMES. */
  selected = 0;
  /** Machine gun heat, 0..1. */
  heat = 0;
  overheated = false;
  private gunWait = 0; // seconds until the gun can fire again
  private rocketWait = 0;
  private shockwaveWait = 0;
  private nextTube = 0; // which of the 4 rocket pod tubes fires next
  private rockets: Rocket[] = [];
  /** What the car has: see setLoadout. */
  private levels: Loadout = [...STOCK_WEAPONS];

  constructor(
    scene: THREE.Scene,
    private world: World,
    private effects: Effects,
    private audio: EngineAudio,
  ) {
    // A small pool of rockets: a white body, a red nose and fins, and a glowing exhaust.
    const white = new THREE.MeshStandardMaterial({ color: 0xe8e8e8, metalness: 0.3, roughness: 0.4 });
    const red = new THREE.MeshStandardMaterial({ color: 0xd7263d, roughness: 0.5 });
    const flame = new THREE.MeshBasicMaterial({ color: 0xffb347, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false });
    for (let i = 0; i < 12; i++) {
      const mesh = new THREE.Group();
      const body = new THREE.Mesh(new THREE.CylinderGeometry(0.07, 0.07, 0.7, 10).rotateX(Math.PI / 2), white);
      const nose = new THREE.Mesh(new THREE.ConeGeometry(0.07, 0.2, 10).rotateX(Math.PI / 2), red);
      nose.position.z = 0.45;
      const fins = new THREE.Mesh(new THREE.BoxGeometry(0.3, 0.02, 0.15), red);
      fins.position.z = -0.28;
      const exhaust = new THREE.Mesh(new THREE.ConeGeometry(0.08, 0.6, 8).rotateX(-Math.PI / 2), flame);
      exhaust.position.z = -0.62;
      mesh.add(body, nose, fins, exhaust);
      mesh.visible = false;
      scene.add(mesh);
      this.rockets.push({ mesh, velocity: new THREE.Vector3(), age: 0, flying: false });
    }
  }

  get name(): string {
    return WEAPON_NAMES[this.selected];
  }

  /** 0..1, for the bar under the weapon name: 1 means ready to fire. */
  get readiness(): number {
    if (this.selected === 0) return 1 - this.heat;
    if (this.selected === 1) return 1 - Math.max(0, this.rocketWait) / this.quicker(ROCKET_RELOAD, 1);
    return 1 - Math.max(0, this.shockwaveWait) / this.quicker(SHOCKWAVE_RELOAD, 2);
  }

  /** Whether the car has any weapon at all. */
  get armed(): boolean {
    return this.levels.some((level) => level > 0);
  }

  /** The level of each weapon (0: not fitted). */
  get loadout(): Loadout {
    return [...this.levels];
  }

  /**
   * Fits the car with weapons, by level (0 for one it doesn't have). If the
   * selected weapon is gone, the first one it does have is selected.
   */
  setLoadout(levels: Loadout): void {
    this.levels = [...levels];
    if (!this.levels[this.selected]) this.selected = Math.max(0, this.levels.findIndex((level) => level > 0));
  }

  /** Whether the car has weapon `index`. */
  has(index: number): boolean {
    return (this.levels[index] ?? 0) > 0;
  }

  /** Selects weapon `index`, if the car has it. Returns false if it doesn't. */
  select(index: number): boolean {
    const i = (index + WEAPON_NAMES.length) % WEAPON_NAMES.length;
    if (!this.has(i)) return false;
    this.selected = i;
    return true;
  }

  /** Selects the next weapon the car has, after the selected one. */
  selectNext(): void {
    for (let step = 1; step <= WEAPON_NAMES.length; step++) if (this.select(this.selected + step)) return;
  }

  /** Damage from weapon `index`, with its upgrades. */
  private damage(base: number, index: number): number {
    return base * (1 + DAMAGE_PER_LEVEL * Math.max(0, this.levels[index] - 1));
  }

  /** A heat or reload time for weapon `index`, cut by its upgrades. */
  private quicker(base: number, index: number): number {
    return base * (1 - QUICKER_PER_LEVEL * Math.max(0, this.levels[index] - 1));
  }

  /** Removes rockets in flight (when the car is reset). */
  clear(): void {
    for (const r of this.rockets) {
      r.flying = false;
      r.mesh.visible = false;
    }
  }

  /**
   * Fires the selected weapon while `firing` is held, and moves rockets along.
   * Returns the explosions that happened, so the game can react to them.
   */
  update(dt: number, firing: boolean, car: CarRig, carVelocity: THREE.Vector3): Blast[] {
    const blasts: Blast[] = [];
    // Only weapons the car has can fire (rockets already flying still fly).
    if (!this.has(this.selected)) firing = false;
    this.gunWait -= dt;
    this.rocketWait -= dt;
    this.shockwaveWait -= dt;

    // The gun cools down all the time, but slower while it is firing.
    const gunFiring = firing && this.selected === 0 && !this.overheated;
    this.heat = Math.max(0, this.heat - GUN_COOLING * dt * (gunFiring ? 0.3 : 1));
    if (this.overheated && this.heat < 0.35) this.overheated = false;

    if (firing && this.selected === 0) {
      // Fire as many shots as are due this frame (usually 0 or 1).
      while (this.gunWait <= 0 && !this.overheated) {
        this.gunWait += 1 / GUN_SHOTS_PER_SECOND;
        this.fireBullet(car);
      }
    }
    if (this.gunWait < 0) this.gunWait = 0;

    if (firing && this.selected === 1 && this.rocketWait <= 0) {
      this.rocketWait = this.quicker(ROCKET_RELOAD, 1);
      this.fireRocket(car, carVelocity);
    }

    if (firing && this.selected === 2 && this.shockwaveWait <= 0) {
      this.shockwaveWait = this.quicker(SHOCKWAVE_RELOAD, 2);
      const at = car.root.position.clone();
      this.effects.shockwave(at, SHOCKWAVE_RADIUS);
      this.world.explode(at, SHOCKWAVE_RADIUS, this.damage(SHOCKWAVE_DAMAGE, 2));
      this.audio.shockwave();
      blasts.push({ at, radius: 0 }); // the car is at the centre; it shouldn't be thrown
    }

    this.moveRockets(dt, blasts);
    return blasts;
  }

  private fireBullet(car: CarRig): void {
    const from = car.root.localToWorld(car.gunMuzzle.clone());
    const direction = this.aim(car, from, false);
    this.heat += this.quicker(GUN_HEAT_PER_SHOT, 0);
    if (this.heat >= 1) {
      this.heat = 1;
      this.overheated = true;
    }
    this.effects.muzzleFlash(from, direction);
    this.audio.gunshot();

    // Bullets are instant: find what is in the way and damage it now.
    // The tracer is only for show.
    const hit = this.world.raycast(from, direction, GUN_RANGE);
    const to = hit ? hit.point : from.clone().addScaledVector(direction, GUN_RANGE);
    this.effects.tracer(from, to);
    if (hit) {
      this.world.applyHit(hit, this.damage(GUN_DAMAGE, 0), direction);
      this.effects.bulletHit(hit.point, hit.ground || hit.target?.kind === 'building');
    }
  }

  private fireRocket(car: CarRig, carVelocity: THREE.Vector3): void {
    const rocket = this.rockets.find((r) => !r.flying) ?? this.rockets[0];
    // The pod has four tubes in a square; take them in turn.
    const tube = this.nextTube++ % 4;
    const muzzle = car.rocketMuzzle.clone();
    muzzle.x += tube % 2 === 0 ? -0.075 : 0.075;
    muzzle.y += tube < 2 ? -0.07 : 0.07;
    const from = car.root.localToWorld(muzzle);
    const direction = this.aim(car, from, true);

    rocket.flying = true;
    rocket.age = 0;
    // Rockets keep the car's speed, so they never fall behind a fast car.
    rocket.velocity.copy(direction).multiplyScalar(ROCKET_SPEED).add(carVelocity);
    rocket.mesh.position.copy(from);
    rocket.mesh.lookAt(from.clone().add(rocket.velocity));
    rocket.mesh.visible = true;
    this.effects.muzzleFlash(from, direction);
    this.audio.rocketLaunch();
  }

  private moveRockets(dt: number, blasts: Blast[]): void {
    for (const r of this.rockets) {
      if (!r.flying) continue;
      r.age += dt;
      const position = r.mesh.position;
      const step = r.velocity.clone().multiplyScalar(dt);
      const direction = step.clone().normalize();

      // Did it hit something on the way to where it will be next frame?
      const hit = this.world.raycast(position, direction, step.length());
      const tooOld = r.age > ROCKET_LIFETIME;
      const inGround = position.y < this.world.heightAt(position.x, position.z);
      if (hit || tooOld || inGround) {
        const at = hit ? hit.point : position.clone();
        const damage = this.damage(ROCKET_DAMAGE, 1);
        if (hit) this.world.applyHit(hit, damage / 2, direction); // a direct hit does extra damage
        this.effects.explosion(at, 2);
        this.world.explode(at, ROCKET_RADIUS, damage);
        blasts.push({ at, radius: ROCKET_RADIUS });
        r.flying = false;
        r.mesh.visible = false;
        continue;
      }

      position.add(step);
      r.velocity.y -= 1.5 * dt; // a slight drop over long distances
      r.mesh.lookAt(position.clone().add(r.velocity));
      // A smoke trail and a flicker of flame behind it.
      const tail = position.clone().addScaledVector(direction, -0.7);
      this.effects.addSmoke({ at: tail, velocity: new THREE.Vector3(0, 0.4, 0), life: 1.5, size: [0.35, 1.6], color: 0xd8d8d8, opacity: 0.55, drag: 1 });
      this.effects.addGlow({ at: tail, life: 0.08, size: [0.6, 0.2], color: 0xffb347 });
    }
  }

  /**
   * Which way to shoot from `from`. Straight ahead, unless a parked car, lamp,
   * tree or rival car is almost in line (within about 7°, 10° for rivals):
   * then aim right at it, so
   * small things are easy to hit. Bullets also dip slightly so they hit the
   * road about 60 m ahead instead of flying over everything low.
   */
  private aim(car: CarRig, from: THREE.Vector3, level: boolean): THREE.Vector3 {
    const forward = new THREE.Vector3(0, 0, 1).applyQuaternion(car.root.quaternion);
    const flatForward = new THREE.Vector3(forward.x, 0, forward.z).normalize();
    let best: THREE.Vector3 | null = null;
    let bestDistance = 120; // don't lock on to anything further than this
    for (const t of this.world.targets) {
      if (!t.alive || t.kind === 'building') continue;
      const middleHeight = t.kind === 'car' ? 0.7 : t.kind === 'tree' ? 2.5 : 4;
      const toTarget = new THREE.Vector3(t.x - from.x, middleHeight - from.y, t.z - from.z);
      const distance = toTarget.length();
      const flatToTarget = new THREE.Vector3(toTarget.x, 0, toTarget.z).normalize();
      const inLine = flatToTarget.dot(flatForward) > 0.992; // cos(7°)
      if (inLine && distance > 3 && distance < bestDistance) {
        bestDistance = distance;
        best = toTarget.normalize();
      }
    }
    // Rival cars in a battle, aimed at the middle of their body.
    for (const h of this.world.hitBoxes) {
      if (!h.alive) continue;
      const toTarget = new THREE.Vector3(h.x - from.x, h.y + h.height / 2 - from.y, h.z - from.z);
      const distance = toTarget.length();
      const flatToTarget = new THREE.Vector3(toTarget.x, 0, toTarget.z).normalize();
      if (flatToTarget.dot(flatForward) > 0.985 && distance > 3 && distance < bestDistance) {
        bestDistance = distance;
        best = toTarget.normalize();
      }
    }
    const direction = best ?? forward.setY(forward.y - (level ? 0 : from.y / 60)).normalize();
    // A little random spread, so the gun sprays like a real one.
    const spread = level ? 0.004 : 0.012;
    direction.x += (Math.random() - 0.5) * spread;
    direction.y += (Math.random() - 0.5) * spread;
    direction.z += (Math.random() - 0.5) * spread;
    return direction.normalize();
  }
}
