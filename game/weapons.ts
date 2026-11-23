// The car's weapons: a roof machine gun, a rocket pod and a shockwave blast.
// Bullets are instant ray tests with a visible tracer; rockets fly and explode.
import * as THREE from 'three';
import type { CarRig } from './carModel';
import type { Effects } from './effects';
import type { RayHit, World } from './world';

export type WeaponKind = 'gun' | 'rockets' | 'shockwave';

export const WEAPONS: { kind: WeaponKind; name: string }[] = [
  { kind: 'gun', name: 'Machine gun' },
  { kind: 'rockets', name: 'Rockets' },
  { kind: 'shockwave', name: 'Shockwave' },
];

const GUN_RATE = 14; // shots per second
const GUN_DAMAGE = 9;
const GUN_RANGE = 300;
const GUN_HEAT = 0.045; // heat per shot; overheats at 1
const GUN_COOL = 0.45; // heat lost per second
const BULLET_SPEED = 400;

const ROCKET_DELAY = 0.45;
const ROCKET_SPEED = 85;
const ROCKET_LIFE = 3;
const ROCKET_RADIUS = 8;
const ROCKET_DAMAGE = 170;

const SHOCK_DELAY = 6;
const SHOCK_RADIUS = 16;
const SHOCK_DAMAGE = 150;

/** Things the game reacts to with sound, camera shake and pushing the car. */
export interface WeaponHooks {
  shot(): void;
  launch(): void;
  explosion(at: THREE.Vector3, radius: number): void;
  shockwave(at: THREE.Vector3, radius: number): void;
}

interface Rocket {
  mesh: THREE.Group;
  vel: THREE.Vector3;
  age: number;
  active: boolean;
}

interface PendingHit {
  hit: RayHit;
  dir: THREE.Vector3;
  delay: number;
}

export class Weapons {
  index = 0;
  /** Machine gun heat, 0..1. */
  heat = 0;
  overheated = false;
  private gunTimer = 0;
  private rocketTimer = 0;
  private shockTimer = 0;
  private rocketTube = 0;
  private rockets: Rocket[] = [];
  private pending: PendingHit[] = [];
  private forward = new THREE.Vector3();

  constructor(
    scene: THREE.Scene,
    private world: World,
    private effects: Effects,
    private hooks: WeaponHooks,
  ) {
    const bodyMat = new THREE.MeshStandardMaterial({ color: 0xe8e8e8, metalness: 0.3, roughness: 0.4 });
    const finMat = new THREE.MeshStandardMaterial({ color: 0xd7263d, roughness: 0.5 });
    const flameMat = new THREE.MeshBasicMaterial({ color: 0xffb347, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false });
    const bodyGeo = new THREE.CylinderGeometry(0.07, 0.07, 0.7, 10).rotateX(Math.PI / 2);
    const noseGeo = new THREE.ConeGeometry(0.07, 0.2, 10).rotateX(Math.PI / 2).translate(0, 0, 0.45);
    const finGeo = new THREE.BoxGeometry(0.3, 0.02, 0.15).translate(0, 0, -0.28);
    const flameGeo = new THREE.ConeGeometry(0.08, 0.6, 8).rotateX(-Math.PI / 2).translate(0, 0, -0.62);
    for (let i = 0; i < 12; i++) {
      const mesh = new THREE.Group();
      const fin2 = new THREE.Mesh(finGeo, finMat);
      fin2.rotation.z = Math.PI / 2;
      mesh.add(new THREE.Mesh(bodyGeo, bodyMat), new THREE.Mesh(noseGeo, finMat), new THREE.Mesh(finGeo, finMat), fin2, new THREE.Mesh(flameGeo, flameMat));
      mesh.visible = false;
      scene.add(mesh);
      this.rockets.push({ mesh, vel: new THREE.Vector3(), age: 0, active: false });
    }
  }

  get kind(): WeaponKind {
    return WEAPONS[this.index].kind;
  }

  get name(): string {
    return WEAPONS[this.index].name;
  }

  /** 0..1: how ready the current weapon is (1 = ready to fire). */
  get readiness(): number {
    switch (this.kind) {
      case 'gun':
        return 1 - this.heat;
      case 'rockets':
        return 1 - Math.max(0, this.rocketTimer) / ROCKET_DELAY;
      case 'shockwave':
        return 1 - Math.max(0, this.shockTimer) / SHOCK_DELAY;
    }
  }

  select(index: number): void {
    this.index = (index + WEAPONS.length) % WEAPONS.length;
  }

  /** Removes rockets and bullets in flight (after a reset). */
  clear(): void {
    for (const r of this.rockets) {
      r.active = false;
      r.mesh.visible = false;
    }
    this.pending = [];
  }

  update(dt: number, firing: boolean, car: CarRig, carVelocity: THREE.Vector3): void {
    this.gunTimer -= dt;
    this.rocketTimer -= dt;
    this.shockTimer -= dt;
    this.heat = Math.max(0, this.heat - GUN_COOL * dt * (firing && this.kind === 'gun' && !this.overheated ? 0.3 : 1));
    if (this.overheated && this.heat < 0.35) this.overheated = false;

    car.root.updateMatrixWorld();
    this.forward.set(0, 0, 1).applyQuaternion(car.root.quaternion).normalize();

    if (firing) {
      switch (this.kind) {
        case 'gun':
          while (this.gunTimer <= 0 && !this.overheated) {
            this.gunTimer += 1 / GUN_RATE;
            this.fireBullet(car);
          }
          break;
        case 'rockets':
          if (this.rocketTimer <= 0) {
            this.rocketTimer = ROCKET_DELAY;
            this.fireRocket(car, carVelocity);
          }
          break;
        case 'shockwave':
          if (this.shockTimer <= 0) {
            this.shockTimer = SHOCK_DELAY;
            const at = car.root.position.clone();
            this.effects.shockwave(at, SHOCK_RADIUS);
            this.world.explode(at, SHOCK_RADIUS, SHOCK_DAMAGE);
            this.hooks.shockwave(at, SHOCK_RADIUS);
          }
          break;
      }
    }
    if (this.gunTimer < 0) this.gunTimer = 0;

    // Bullets land when their tracer gets there.
    for (let i = this.pending.length - 1; i >= 0; i--) {
      const p = this.pending[i];
      p.delay -= dt;
      if (p.delay > 0) continue;
      this.pending.splice(i, 1);
      this.world.applyHit(p.hit, GUN_DAMAGE, p.dir);
      this.effects.impact(p.hit.point, p.hit.normal, p.hit.ground || p.hit.target?.kind === 'building');
    }

    this.updateRockets(dt);
  }

  private fireBullet(car: CarRig): void {
    const from = car.root.localToWorld(car.gunMuzzle.clone());
    const dir = this.aim(from, 0.012);
    this.heat += GUN_HEAT;
    if (this.heat >= 1) {
      this.heat = 1;
      this.overheated = true;
    }
    this.effects.muzzleFlash(from, dir);
    this.hooks.shot();
    const hit = this.world.raycast(from, dir, GUN_RANGE);
    const to = hit ? hit.point : from.clone().addScaledVector(dir, GUN_RANGE);
    const flight = from.distanceTo(to) / BULLET_SPEED;
    this.effects.tracer(from, to, flight);
    if (hit) this.pending.push({ hit, dir, delay: flight });
  }

  private fireRocket(car: CarRig, carVelocity: THREE.Vector3): void {
    const r = this.rockets.find((x) => !x.active) ?? this.rockets[0];
    // Alternate between the four tubes of the pod.
    const tube = this.rocketTube++ % 4;
    const local = car.rocketMuzzle.clone().add(new THREE.Vector3(tube % 2 ? 0.075 : -0.075, tube < 2 ? -0.07 : 0.07, 0));
    const from = car.root.localToWorld(local);
    const dir = this.aim(from, 0.004, true);
    r.active = true;
    r.age = 0;
    r.vel.copy(dir).multiplyScalar(ROCKET_SPEED).add(carVelocity);
    r.mesh.position.copy(from);
    r.mesh.lookAt(from.clone().add(r.vel));
    r.mesh.visible = true;
    this.effects.muzzleFlash(from, dir);
    for (let i = 0; i < 6; i++) {
      this.effects.puff({
        x: from.x, y: from.y, z: from.z,
        vx: (Math.random() - 0.5) * 2 - dir.x * 3, vy: Math.random(), vz: (Math.random() - 0.5) * 2 - dir.z * 3,
        life: 1 + Math.random(), size: [0.4, 1.8], color: [0.8, 0.8, 0.8], alpha: [0.5, 0], drag: 1.5,
      });
    }
    this.hooks.launch();
  }

  private updateRockets(dt: number): void {
    const step = new THREE.Vector3();
    const dir = new THREE.Vector3();
    for (const r of this.rockets) {
      if (!r.active) continue;
      r.age += dt;
      const p = r.mesh.position;
      step.copy(r.vel).multiplyScalar(dt);
      const len = step.length();
      dir.copy(step).divideScalar(len || 1);
      const hit = this.world.raycast(p, dir, len);
      if (hit || r.age > ROCKET_LIFE || p.y < this.world.heightAt(p.x, p.z)) {
        const at = hit ? hit.point.clone().addScaledVector(hit.normal, 0.3) : p.clone();
        if (hit) this.world.applyHit(hit, ROCKET_DAMAGE * 0.5, dir);
        this.explode(at);
        r.active = false;
        r.mesh.visible = false;
        continue;
      }
      p.add(step);
      r.vel.y -= 1.5 * dt; // a slight drop over long distances
      r.mesh.lookAt(p.clone().add(r.vel));
      // Smoke trail and flame.
      const back = p.clone().addScaledVector(dir, -0.7);
      this.effects.puff({
        x: back.x, y: back.y, z: back.z,
        vx: (Math.random() - 0.5) * 0.6, vy: 0.3 + Math.random() * 0.4, vz: (Math.random() - 0.5) * 0.6,
        life: 1.2 + Math.random() * 0.8, size: [0.35, 1.6], color: [0.85, 0.85, 0.85], alpha: [0.55, 0], drag: 1,
      });
      this.effects.spark({ x: back.x, y: back.y, z: back.z, life: 0.08, size: [0.6, 0.2], color: [1, 0.7, 0.3] });
    }
  }

  private explode(at: THREE.Vector3): void {
    this.effects.explosion(at, 2);
    this.world.explode(at, ROCKET_RADIUS, ROCKET_DAMAGE);
    this.hooks.explosion(at, ROCKET_RADIUS);
  }

  /**
   * Which way to shoot. Aims straight ahead, but locks on to a car, lamp or
   * tree that is nearly in line, so low and thin things are easy to hit.
   */
  private aim(from: THREE.Vector3, spread: number, level = false): THREE.Vector3 {
    const fwd = this.forward;
    let best: THREE.Vector3 | null = null;
    let bestScore = Infinity;
    const to = new THREE.Vector3();
    for (const t of this.world.targets) {
      if (!t.alive || t.kind === 'building') continue;
      to.set(t.x - from.x, (t.kind === 'car' ? 0.7 : t.kind === 'tree' ? 2.5 : 4) - from.y, t.z - from.z);
      const dist = to.length();
      if (dist < 3 || dist > 120) continue;
      const flatDot = (to.x * fwd.x + to.z * fwd.z) / (Math.hypot(to.x, to.z) * Math.hypot(fwd.x, fwd.z) || 1);
      if (flatDot < 0.992) continue; // within about 7 degrees
      const score = dist * (2 - flatDot);
      if (score < bestScore) {
        bestScore = score;
        best = to.clone().normalize();
      }
    }
    // Otherwise bullets aim slightly down, to reach the road about 60 m ahead.
    const dir = best ?? fwd.clone().setY(fwd.y - (level ? 0 : from.y / 60)).normalize();
    dir.x += (Math.random() - 0.5) * spread;
    dir.y += (Math.random() - 0.5) * spread;
    dir.z += (Math.random() - 0.5) * spread;
    return dir.normalize();
  }
}
