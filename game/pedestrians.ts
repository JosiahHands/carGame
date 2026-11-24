// People walking round the city's pavements in free roam. They stroll round
// the blocks, leap out of the way of cars (always in time: nobody gets hurt),
// run from explosions, and cheer big jumps and fast passes.
//
// Everyone is drawn with a few instanced meshes (one for all the heads, one
// for all the bodies...), so a hundred people cost almost nothing to draw.
import * as THREE from 'three';
import { ROAD_LINES, type World } from './world';

const PER_BLOCK = 3;
/** The pavement loop round each block: this far from the block's centre (the road edge is 34 m out). */
const LOOP = 32;
const SIDE = LOOP * 2;
const PERIMETER = SIDE * 4;
const GRAVITY = 18;

const SKIN = [0xf1c27d, 0xe0ac69, 0xc68642, 0x8d5524, 0xffdbac, 0x6b4226];
const SHIRTS = [0xd7263d, 0x1b98e0, 0x3bb273, 0xf46036, 0xffd23f, 0x7b2cbf, 0xffffff, 0x2e2e2e, 0xff70a6, 0x2ec4b6];
const TROUSERS = [0x2b3a55, 0x3a3a3a, 0x6b4f3a, 0x1d3557, 0x8a8f99, 0x4a5d23];

interface Person {
  /** The centre of the block they walk round, and where they are on its loop (metres). */
  bx: number;
  bz: number;
  along: number;
  /** +1 walks one way round, -1 the other. */
  way: number;
  pace: number;
  /** Walk cycle, for swinging arms and legs. */
  step: number;
  /** Knocked off the loop by a dodge: offset from the loop, and how fast it's moving. */
  off: THREE.Vector3;
  vel: THREE.Vector3;
  /** Seconds left of each mood. */
  dodging: number;
  fleeing: number;
  cheering: number;
  /** Which way they dived, for the lean. */
  lean: number;
}

export class Pedestrians {
  private people: Person[] = [];
  private legs: THREE.InstancedMesh;
  private arms: THREE.InstancedMesh;
  private bodies: THREE.InstancedMesh;
  private heads: THREE.InstancedMesh;
  private group = new THREE.Group();
  private clock = 0;
  // Reused every frame.
  private m = new THREE.Matrix4();
  private part = new THREE.Matrix4();
  private q = new THREE.Quaternion();
  private e = new THREE.Euler();

  constructor(scene: THREE.Scene, private max = 120) {
    const mat = new THREE.MeshStandardMaterial({ roughness: 0.8 });
    // Each part's geometry hangs from its joint (hip or shoulder), so it swings from there.
    this.legs = new THREE.InstancedMesh(new THREE.BoxGeometry(0.17, 0.86, 0.19).translate(0, -0.43, 0), mat, max * 2);
    this.arms = new THREE.InstancedMesh(new THREE.BoxGeometry(0.12, 0.62, 0.13).translate(0, -0.31, 0), mat, max * 2);
    this.bodies = new THREE.InstancedMesh(new THREE.BoxGeometry(0.44, 0.62, 0.25).translate(0, 0.31, 0), mat, max);
    this.heads = new THREE.InstancedMesh(new THREE.SphereGeometry(0.14, 10, 8), mat, max);
    for (const mesh of this.meshes) {
      mesh.castShadow = true;
      mesh.frustumCulled = false; // they move all over the map
      mesh.count = 0;
      this.group.add(mesh);
    }
    scene.add(this.group);
  }

  private get meshes(): THREE.InstancedMesh[] {
    return [this.legs, this.arms, this.bodies, this.heads];
  }

  /** Puts people on the pavements (city only). */
  load(world: World, on: boolean): void {
    this.people = [];
    if (on && world.map === 'city') {
      const centres = ROAD_LINES.slice(0, -1).map((c) => c + 40);
      for (const bx of centres) {
        for (const bz of centres) {
          for (let k = 0; k < PER_BLOCK && this.people.length < this.max; k++) {
            this.people.push({
              bx, bz, along: Math.random() * PERIMETER, way: Math.random() < 0.5 ? 1 : -1, pace: 1 + Math.random() * 0.6,
              step: Math.random() * 10, off: new THREE.Vector3(), vel: new THREE.Vector3(), dodging: 0, fleeing: 0, cheering: 0, lean: 0,
            });
          }
        }
      }
    }
    // Colours for each person: skin on the head, shirt on the body and arms, trousers on the legs.
    const color = new THREE.Color();
    this.people.forEach((_, i) => {
      const pick = (list: number[]) => color.setHex(list[Math.floor(Math.random() * list.length)]);
      this.heads.setColorAt(i, pick(SKIN));
      const shirt = pick(SHIRTS).clone();
      this.bodies.setColorAt(i, shirt);
      this.arms.setColorAt(i * 2, shirt);
      this.arms.setColorAt(i * 2 + 1, shirt);
      const trousers = pick(TROUSERS).clone();
      this.legs.setColorAt(i * 2, trousers);
      this.legs.setColorAt(i * 2 + 1, trousers);
    });
    for (const mesh of this.meshes) {
      mesh.count = mesh === this.legs || mesh === this.arms ? this.people.length * 2 : this.people.length;
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    }
  }

  setVisible(visible: boolean): void {
    this.group.visible = visible;
  }

  /** Everyone near an explosion runs away from it. */
  panic(x: number, z: number, radius: number): void {
    for (const p of this.people) {
      const at = this.loopPoint(p);
      if (Math.hypot(at.x + p.off.x - x, at.z + p.off.z - z) > radius) continue;
      p.fleeing = 3 + Math.random() * 2;
      // Run whichever way round the loop takes them away from it.
      p.way = (at.dx * (at.x - x) + at.dz * (at.z - z)) >= 0 ? 1 : -1;
    }
  }

  /** Everyone near a big jump or a fast pass cheers. */
  cheer(x: number, z: number, radius: number): void {
    for (const p of this.people) {
      if (p.dodging > 0 || p.fleeing > 0) continue;
      const at = this.loopPoint(p);
      if (Math.hypot(at.x - x, at.z - z) < radius) p.cheering = 1.5 + Math.random() * 1.5;
    }
  }

  /**
   * Moves everyone on, and gets them out of the way of the car at (x, z)
   * going (vx, vz), which reaches `reach` metres from its centre.
   */
  update(dt: number, car: { x: number; y: number; z: number; vx: number; vz: number; reach: number }): void {
    if (!this.people.length || !this.group.visible) return;
    this.clock += dt;
    const speed = Math.hypot(car.vx, car.vz);
    this.people.forEach((p, i) => {
      p.dodging = Math.max(0, p.dodging - dt);
      p.fleeing = Math.max(0, p.fleeing - dt);
      p.cheering = Math.max(0, p.cheering - dt);
      const walking = p.dodging <= 0 && p.cheering <= 0;
      const pace = p.fleeing > 0 ? 4.5 : p.pace;
      if (walking) {
        p.along = (p.along + p.way * pace * dt + PERIMETER) % PERIMETER;
        p.step += pace * dt * 4.2;
      }
      const at = this.loopPoint(p);
      let x = at.x + p.off.x;
      let z = at.z + p.off.z;

      // A car coming: work out how close it will pass in the next second, and
      // leap clear in time. Anyone back on their feet and still in the way jumps again.
      const grounded = p.off.y <= 0 && p.vel.y <= 0;
      if (grounded && speed > 3) {
        const dx = x - car.x;
        const dz = z - car.z;
        const near = Math.hypot(dx, dz);
        const soon = Math.min(1, Math.max(0, (dx * car.vx + dz * car.vz) / (speed * speed)));
        const miss = Math.hypot(dx - car.vx * soon, dz - car.vz * soon);
        if ((miss < car.reach + 1.5 && near < speed * 1.2 + car.reach) || near < car.reach + 0.8) {
          // Jump sideways, away from the car's path (and up, the closer it is).
          const sideX = -car.vz / speed;
          const sideZ = car.vx / speed;
          const away = dx * sideX + dz * sideZ >= 0 ? 1 : -1;
          // Far enough to land well clear of the car's side, in one hop.
          const up = near < car.reach + 2 ? 7 : 4;
          const go = Math.max(1.5, car.reach + 1.5 - miss);
          const push = go / ((2 * up) / GRAVITY);
          p.vel.set(sideX * away * push, up, sideZ * away * push);
          p.dodging = 1.6;
          p.lean = away;
        }
      }
      // Flying through the air from a dodge, then walking back to the pavement.
      if (p.off.y > 0 || p.vel.y > 0) {
        p.vel.y -= GRAVITY * dt;
        p.off.addScaledVector(p.vel, dt);
        if (p.off.y <= 0) {
          p.off.y = 0;
          p.vel.set(0, 0, 0);
        }
      } else if (p.dodging <= 0) {
        p.off.multiplyScalar(Math.exp(-dt * 1.5));
      }
      x = at.x + p.off.x;
      z = at.z + p.off.z;
      this.pose(i, p, x, p.off.y, z, Math.atan2(at.dx * p.way, at.dz * p.way));
    });
    for (const mesh of this.meshes) mesh.instanceMatrix.needsUpdate = true;
  }

  /** Where on the loop round the block, and the direction of the loop there. */
  private loopPoint(p: Person): { x: number; z: number; dx: number; dz: number } {
    const side = Math.floor(p.along / SIDE);
    const t = p.along - side * SIDE - LOOP;
    switch (side) {
      case 0: return { x: p.bx + t, z: p.bz - LOOP, dx: 1, dz: 0 };
      case 1: return { x: p.bx + LOOP, z: p.bz + t, dx: 0, dz: 1 };
      case 2: return { x: p.bx - t, z: p.bz + LOOP, dx: -1, dz: 0 };
      default: return { x: p.bx - LOOP, z: p.bz - t, dx: 0, dz: -1 };
    }
  }

  /** Sets the matrices for one person's head, body, arms and legs. */
  private pose(i: number, p: Person, x: number, y: number, z: number, heading: number): void {
    const m = this.m;
    // Diving sideways while dodging; a little bounce in the step otherwise.
    const dive = p.dodging > 0 ? p.lean * Math.min(1, p.dodging) * 1.1 : 0;
    const bounce = p.dodging > 0 || p.cheering > 0 ? 0 : Math.abs(Math.sin(p.step)) * 0.04;
    this.e.set(0, heading, dive, 'YXZ');
    m.compose(new THREE.Vector3(x, y + bounce, z), this.q.setFromEuler(this.e), new THREE.Vector3(1, 1, 1));
    const swing = p.dodging > 0 ? 0.6 : Math.sin(p.step) * (p.fleeing > 0 ? 0.9 : 0.5);
    const set = (mesh: THREE.InstancedMesh, k: number, px: number, py: number, pz: number, rx: number, rz = 0) => {
      this.e.set(rx, 0, rz);
      this.part.compose(new THREE.Vector3(px, py, pz), this.q.setFromEuler(this.e), new THREE.Vector3(1, 1, 1));
      mesh.setMatrixAt(k, this.part.premultiply(m));
    };
    set(this.legs, i * 2, -0.1, 0.9, 0, swing);
    set(this.legs, i * 2 + 1, 0.1, 0.9, 0, -swing);
    set(this.bodies, i, 0, 0.88, 0, 0);
    // Arms swing opposite the legs; both go up to cheer or when diving and running away.
    const up = p.cheering > 0 ? Math.PI * (0.85 + Math.sin(this.clock * 12 + i) * 0.1) : p.dodging > 0 || p.fleeing > 0 ? Math.PI * 0.8 : 0;
    set(this.arms, i * 2, -0.29, 1.46, 0, up ? 0 : -swing, up ? -up : 0);
    set(this.arms, i * 2 + 1, 0.29, 1.46, 0, up ? 0 : swing, up ? up : 0);
    set(this.heads, i, 0, 1.66, 0, 0);
  }
}
