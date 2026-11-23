// The drivable world: a city grid with buildings, parks, a car park and a stunt park.
// Buildings, parked cars, street lamps and trees can be destroyed, and come back
// a while after the player has driven away.
import * as THREE from 'three';
import { buildCar, type CarRig } from './carModel';
import { DEFAULT_CONFIG, randomConfig } from './carConfig';
import type { Effects } from './effects';
import { concreteTexture, grassTexture, roadTexture, windowTexture } from './textures';
import { gradientTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';

/** A solid box standing on the ground, for collisions. */
export interface Box {
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
  top: number;
  /** Switched off when what it belongs to is destroyed. */
  off?: boolean;
  target?: Target;
}

/** A solid upright post (lamp pole, tree trunk), for collisions. */
export interface Circle {
  x: number;
  z: number;
  r: number;
  top: number;
  /** Radius bullets hit, when bigger than the post (a tree's leaves). */
  hitR?: number;
  off?: boolean;
  target?: Target;
}

interface TargetBase {
  hp: number;
  maxHp: number;
  alive: boolean;
  destroyedAt: number;
  x: number;
  z: number;
  /** Damage score for destroying it, in dollars. */
  value: number;
}

export interface BuildingTarget extends TargetBase {
  kind: 'building';
  mesh: THREE.Mesh;
  box: Box;
  width: number;
  depth: number;
  height: number;
  phase: 'standing' | 'collapsing' | 'rubble' | 'rising';
  t: number;
  /** Which way it leans as it falls. */
  tilt: THREE.Vector2;
  rubble: THREE.Mesh | null;
  /** How tall the rubble pile is right now (it grows while the building falls). */
  pile: number;
}

export interface CarTarget extends TargetBase {
  kind: 'car';
  rig: CarRig;
  box: Box;
  home: THREE.Vector3;
  homeHeading: number;
  vel: THREE.Vector3;
  spin: THREE.Vector3;
  flying: boolean;
  burnUntil: number;
}

export interface PropTarget extends TargetBase {
  kind: 'lamp' | 'tree';
  meshes: THREE.InstancedMesh[];
  index: number;
  matrix: THREE.Matrix4;
  scale: number;
  circle: Circle;
}

export type Target = BuildingTarget | CarTarget | PropTarget;

export interface RayHit {
  distance: number;
  point: THREE.Vector3;
  normal: THREE.Vector3;
  target: Target | null;
  cone: Cone | null;
  ground: boolean;
}

/** Map features, for the minimap. */
export interface Block {
  x: number;
  z: number;
  kind: 'stunt' | 'park' | 'lot' | 'city';
}

export interface Ramp {
  x: number;
  z: number;
  angle: number; // direction the ramp faces (driving up it)
  width: number;
  length: number;
  height: number;
}

export interface Cone {
  mesh: THREE.Mesh;
  home: THREE.Vector3;
  vel: THREE.Vector3;
  spin: THREE.Vector3;
  knocked: boolean;
  /** Knocked over and come to rest; no more physics until it is reset. */
  settled: boolean;
  knockedAt: number;
}

export const HALF_SIZE = 260; // the world spans -260..260 on x and z
export const ROAD_LINES = [-240, -160, -80, 0, 80, 160, 240];
export const ROAD_WIDTH = 12;
const RISE = 0.8; // fraction of a ramp's length that slopes up
const CONE_HEIGHT = 0.75;
const CONE_LYING_Y = 0.25; // height of a cone's axis when it lies on its side
const COLLAPSE_TIME = 2.4; // seconds for a building to fall
const RISE_TIME = 2; // seconds for a building to be rebuilt
const PILE_HEIGHT = 1.6; // rubble left by a building, metres
const RESPAWN_AFTER = 45; // seconds before destroyed things come back...
const RESPAWN_DISTANCE = 60; // ...once the player is at least this far away

export class World {
  readonly group = new THREE.Group();
  readonly boxes: Box[] = [];
  readonly circles: Circle[] = [];
  readonly targets: Target[] = [];
  readonly blocks: Block[] = [];
  readonly ramps: Ramp[] = [];
  readonly spawn = { x: -3, z: -20, heading: 0 };
  /** Called when something is destroyed (for the score and sounds). */
  onDestroyed: (target: Target) => void = () => {};
  private cones: Cone[] = [];
  private parkedCars: CarRig[] = [];
  private buildingMats: THREE.MeshStandardMaterial[] = [];
  private lampHeadMat: THREE.MeshStandardMaterial;
  private sun: THREE.DirectionalLight;
  private hemi: THREE.HemisphereLight;
  private dayBg: THREE.Texture;
  private nightBg: THREE.Texture;
  private rand = seededRandom(42);
  private clock = 0;
  private buildings: BuildingTarget[] = [];
  private rubbleMats = new Map<THREE.Material, THREE.MeshStandardMaterial>();
  private carX = 0;
  private carZ = 0;
  private night = false;
  private viewDistance = 430;

  constructor(
    private scene: THREE.Scene,
    private effects: Effects,
  ) {
    scene.add(this.group);

    this.dayBg = gradientTexture('#5fa8e8', '#cfe5f5');
    this.nightBg = gradientTexture('#02040b', '#18233a');
    this.hemi = new THREE.HemisphereLight(0xdfefff, 0x4a5a3a, 1.2);
    this.sun = new THREE.DirectionalLight(0xfff2dd, 2.6);
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(2048, 2048);
    const s = this.sun.shadow.camera;
    s.left = s.bottom = -45;
    s.right = s.top = 45;
    s.near = 1;
    s.far = 200;
    this.sun.shadow.bias = -0.0004;
    this.sun.shadow.normalBias = 0.03;
    scene.add(this.hemi, this.sun, this.sun.target);

    this.lampHeadMat = new THREE.MeshStandardMaterial({ color: 0xffffff, emissive: 0xffd9a0, emissiveIntensity: 0 });

    this.buildGround();
    this.buildRoads();
    this.buildBlocks();
    this.buildEdges();
    this.setNight(false);
  }

  // ---------- building the world ----------

  private buildGround(): void {
    const tex = grassTexture();
    tex.repeat.set(140, 140);
    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(HALF_SIZE * 2 + 400, HALF_SIZE * 2 + 400),
      new THREE.MeshStandardMaterial({ map: tex, roughness: 1 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.receiveShadow = true;
    this.group.add(ground);
  }

  private buildRoads(): void {
    const tex = roadTexture();
    const len = HALF_SIZE * 2 - 16;
    for (const [i, c] of ROAD_LINES.entries()) {
      for (const dir of ['x', 'z'] as const) {
        const t = tex.clone();
        t.repeat.set(1, len / 12);
        t.needsUpdate = true;
        const road = new THREE.Mesh(
          new THREE.PlaneGeometry(ROAD_WIDTH, len),
          new THREE.MeshStandardMaterial({ map: t, roughness: 0.9 }),
        );
        road.rotation.x = -Math.PI / 2;
        if (dir === 'x') {
          road.rotation.z = Math.PI / 2;
          road.position.set(0, 0.02 + i * 0.0005, c);
        } else {
          road.position.set(c, 0.025 + i * 0.0005, 0);
        }
        road.receiveShadow = true;
        this.group.add(road);
      }
    }
    // Plain asphalt squares at crossings, so dashed lines don't overlap.
    const crossMat = new THREE.MeshStandardMaterial({ color: 0x34363b, roughness: 0.9 });
    const crossGeo = new THREE.PlaneGeometry(ROAD_WIDTH, ROAD_WIDTH);
    for (const x of ROAD_LINES) {
      for (const z of ROAD_LINES) {
        const m = new THREE.Mesh(crossGeo, crossMat);
        m.rotation.x = -Math.PI / 2;
        m.position.set(x, 0.035, z);
        m.receiveShadow = true;
        this.group.add(m);
      }
    }
    // Street lamps along the roads.
    const poles: THREE.Matrix4[] = [];
    // Lamps sit 20 m either side of each block centre, well clear of the crossings.
    const along = ROAD_LINES.slice(0, -1).flatMap((c) => [c + 20, c + 60]);
    for (const c of ROAD_LINES) {
      for (const p of along) {
        poles.push(new THREE.Matrix4().makeTranslation(c + ROAD_WIDTH / 2 + 1, 0, p));
        poles.push(new THREE.Matrix4().makeTranslation(p, 0, c - ROAD_WIDTH / 2 - 1));
      }
    }
    const poleGeo = new THREE.CylinderGeometry(0.12, 0.16, 7, 8).translate(0, 3.5, 0);
    const headGeo = new THREE.BoxGeometry(0.7, 0.25, 0.7).translate(0, 7.1, 0);
    const poleMesh = new THREE.InstancedMesh(poleGeo, new THREE.MeshStandardMaterial({ color: 0x3a3d42, metalness: 0.6, roughness: 0.4 }), poles.length);
    const headMesh = new THREE.InstancedMesh(headGeo, this.lampHeadMat, poles.length);
    poles.forEach((m, i) => {
      poleMesh.setMatrixAt(i, m);
      headMesh.setMatrixAt(i, m);
      const p = new THREE.Vector3().setFromMatrixPosition(m);
      const circle: Circle = { x: p.x, z: p.z, r: 0.25, top: 7.3, hitR: 0.3 };
      this.circles.push(circle);
      const target: PropTarget = {
        kind: 'lamp', hp: 25, maxHp: 25, alive: true, destroyedAt: 0, x: p.x, z: p.z, value: 1500,
        meshes: [poleMesh, headMesh], index: i, matrix: m.clone(), scale: 1, circle,
      };
      circle.target = target;
      this.targets.push(target);
    });
    poleMesh.castShadow = true;
    this.group.add(poleMesh, headMesh);
  }

  private buildBlocks(): void {
    const centers = ROAD_LINES.slice(0, -1).map((c) => c + 40);
    const pavement = concreteTexture();
    const trees: THREE.Vector3[] = [];
    for (const bx of centers) {
      for (const bz of centers) {
        const inner = 80 - ROAD_WIDTH; // 68 m square between the roads
        const t = pavement.clone();
        t.repeat.set(inner / 4, inner / 4);
        t.needsUpdate = true;
        const slab = new THREE.Mesh(
          new THREE.PlaneGeometry(inner, inner),
          new THREE.MeshStandardMaterial({ map: t, roughness: 0.95 }),
        );
        slab.rotation.x = -Math.PI / 2;
        slab.position.set(bx, 0.015, bz);
        slab.receiveShadow = true;

        let kind: Block['kind'];
        if (bx === 40 && bz === 40) kind = 'stunt';
        else if (bx === -40 && bz === 40) kind = 'park';
        else if (bx === 40 && bz === -40) kind = 'lot';
        else if (bx === -40 && bz === -40) kind = 'park';
        else {
          const r = this.rand();
          kind = r < 0.62 ? 'city' : r < 0.85 ? 'park' : 'lot';
        }
        this.blocks.push({ x: bx, z: bz, kind });

        if (kind === 'park') {
          // Grass park ringed by pavement, full of trees.
          const grass = new THREE.Mesh(
            new THREE.PlaneGeometry(inner - 8, inner - 8),
            new THREE.MeshStandardMaterial({ color: 0x5b8f43, roughness: 1 }),
          );
          grass.rotation.x = -Math.PI / 2;
          grass.position.set(bx, 0.018, bz);
          grass.receiveShadow = true;
          this.group.add(slab, grass);
          for (let i = 0; i < 26; i++) {
            trees.push(new THREE.Vector3(bx + (this.rand() - 0.5) * (inner - 14), 0, bz + (this.rand() - 0.5) * (inner - 14)));
          }
        } else if (kind === 'city') {
          this.group.add(slab);
          for (const ox of [-1, 1]) {
            for (const oz of [-1, 1]) {
              const w = 14 + this.rand() * 14;
              const d = 14 + this.rand() * 14;
              const h = 8 + Math.pow(this.rand(), 1.6) * 60;
              this.addBuilding(bx + ox * 17, bz + oz * 17, w, d, h);
            }
          }
        } else if (kind === 'lot') {
          this.group.add(slab);
          this.buildCarPark(bx, bz);
        } else {
          this.group.add(slab);
          this.buildStuntPark(bx, bz);
        }
      }
    }
    // Trees around the outskirts.
    for (let i = 0; i < 140; i++) {
      const side = Math.floor(this.rand() * 4);
      const along = (this.rand() - 0.5) * (HALF_SIZE * 2 - 10);
      const off = HALF_SIZE - 3 - this.rand() * 6;
      const p = [new THREE.Vector3(along, 0, off), new THREE.Vector3(along, 0, -off), new THREE.Vector3(off, 0, along), new THREE.Vector3(-off, 0, along)][side];
      trees.push(p);
    }
    this.addTrees(trees);
  }

  private addBuilding(x: number, z: number, w: number, d: number, h: number): void {
    if (this.buildingMats.length === 0) {
      const day = windowTexture(false);
      const lit = windowTexture(true);
      for (const c of ['#c9c2b8', '#9fa8b3', '#b98b73', '#7d8a99', '#d9d4c7', '#8c7f73', '#a7b6a5']) {
        this.buildingMats.push(
          new THREE.MeshStandardMaterial({ color: c, map: day, emissiveMap: lit, emissive: 0xffffff, emissiveIntensity: 0, roughness: 0.85 }),
        );
      }
    }
    const geo = new THREE.BoxGeometry(w, h, d);
    // Scale UVs so windows stay the same size on every building; roofs get a plain patch.
    const uv = geo.getAttribute('uv') as THREE.BufferAttribute;
    const faceSize = [[d, h], [d, h], [0, 0], [0, 0], [w, h], [w, h]];
    for (let f = 0; f < 6; f++) {
      for (let v = 0; v < 4; v++) {
        const i = f * 4 + v;
        const [fw, fh] = faceSize[f];
        if (fw === 0) uv.setXY(i, 0.02, 0.02);
        else uv.setXY(i, uv.getX(i) * (fw / 8), uv.getY(i) * (fh / 7));
      }
    }
    const mat = this.buildingMats[Math.floor(this.rand() * this.buildingMats.length)];
    const mesh = new THREE.Mesh(geo, mat);
    mesh.position.set(x, h / 2, z);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    this.group.add(mesh);
    const box: Box = { minX: x - w / 2, maxX: x + w / 2, minZ: z - d / 2, maxZ: z + d / 2, top: h };
    this.boxes.push(box);
    const hp = Math.round(120 + h * 6);
    const target: BuildingTarget = {
      kind: 'building', hp, maxHp: hp, alive: true, destroyedAt: 0, x, z,
      value: Math.round((40 + h * 2.5) * 1000),
      mesh, box, width: w, depth: d, height: h, phase: 'standing', t: 0, tilt: new THREE.Vector2(), rubble: null, pile: 0,
    };
    box.target = target;
    this.targets.push(target);
    this.buildings.push(target);
  }

  private addTrees(points: THREE.Vector3[]): void {
    const trunkGeo = new THREE.CylinderGeometry(0.22, 0.3, 2.4, 7).translate(0, 1.2, 0);
    const leafGeo = new THREE.IcosahedronGeometry(1.8, 0).translate(0, 3.6, 0);
    const trunks = new THREE.InstancedMesh(trunkGeo, new THREE.MeshStandardMaterial({ color: 0x6b4a2f, roughness: 1 }), points.length);
    const leaves = new THREE.InstancedMesh(leafGeo, new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.9, flatShading: true }), points.length);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const color = new THREE.Color();
    points.forEach((p, i) => {
      const s = 0.8 + this.rand() * 0.6;
      q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), this.rand() * Math.PI * 2);
      m.compose(p, q, new THREE.Vector3(s, s, s));
      trunks.setMatrixAt(i, m);
      leaves.setMatrixAt(i, m);
      leaves.setColorAt(i, color.setHSL(0.25 + this.rand() * 0.08, 0.5, 0.28 + this.rand() * 0.12));
      const circle: Circle = { x: p.x, z: p.z, r: 0.45 * s, top: 5.4 * s, hitR: 1.4 * s };
      this.circles.push(circle);
      const target: PropTarget = {
        kind: 'tree', hp: 30, maxHp: 30, alive: true, destroyedAt: 0, x: p.x, z: p.z, value: 800,
        meshes: [trunks, leaves], index: i, matrix: m.clone(), scale: s, circle,
      };
      circle.target = target;
      this.targets.push(target);
    });
    trunks.castShadow = leaves.castShadow = true;
    trunks.receiveShadow = leaves.receiveShadow = true;
    this.group.add(trunks, leaves);
  }

  private buildCarPark(bx: number, bz: number): void {
    const lineMat = new THREE.MeshBasicMaterial({ color: 0xf0f0f0 });
    const asphalt = new THREE.Mesh(
      new THREE.PlaneGeometry(60, 60),
      new THREE.MeshStandardMaterial({ color: 0x3a3c40, roughness: 0.95 }),
    );
    asphalt.rotation.x = -Math.PI / 2;
    asphalt.position.set(bx, 0.02, bz);
    asphalt.receiveShadow = true;
    this.group.add(asphalt);
    for (const row of [-1, 1]) {
      for (let i = 0; i < 9; i++) {
        const x = bx - 24 + i * 6;
        const z = bz + row * 18;
        const line = new THREE.Mesh(new THREE.PlaneGeometry(0.15, 9), lineMat);
        line.rotation.x = -Math.PI / 2;
        line.position.set(x - 3, 0.025, z);
        this.group.add(line);
        if (this.rand() < 0.55) {
          const car = buildCar(randomConfig(DEFAULT_CONFIG), false, false);
          car.root.position.set(x, 0, z);
          car.root.rotation.y = row > 0 ? Math.PI : 0;
          this.group.add(car.root);
          this.parkedCars.push(car);
          const hw = car.width / 2 + 0.05;
          const hl = car.length / 2;
          const box: Box = { minX: x - hw, maxX: x + hw, minZ: z - hl, maxZ: z + hl, top: 1.4 };
          this.boxes.push(box);
          const target: CarTarget = {
            kind: 'car', hp: 70, maxHp: 70, alive: true, destroyedAt: 0, x, z, value: 12000,
            rig: car, box, home: new THREE.Vector3(x, 0, z), homeHeading: car.root.rotation.y,
            vel: new THREE.Vector3(), spin: new THREE.Vector3(), flying: false, burnUntil: 0,
          };
          box.target = target;
          this.targets.push(target);
        }
      }
    }
  }

  private buildStuntPark(bx: number, bz: number): void {
    const rampMat = new THREE.MeshStandardMaterial({ color: 0xe0a03a, roughness: 0.7 });
    const stripeMat = new THREE.MeshStandardMaterial({ color: 0x222222, roughness: 0.7 });
    const ramps: Ramp[] = [
      { x: bx - 18, z: bz - 12, angle: 0, width: 6, length: 10, height: 1.6 },
      { x: bx + 14, z: bz - 6, angle: Math.PI / 2, width: 7, length: 14, height: 2.6 },
      { x: bx - 4, z: bz + 18, angle: Math.PI, width: 5, length: 8, height: 1.2 },
      { x: bx + 20, z: bz + 22, angle: -Math.PI / 2, width: 6, length: 12, height: 2.0 },
    ];
    for (const ramp of ramps) {
      const L = ramp.length;
      const shape = new THREE.Shape();
      shape.moveTo(-L / 2, 0);
      shape.lineTo(-L / 2 + L * RISE, ramp.height);
      shape.lineTo(L / 2, 0);
      shape.closePath();
      const geo = new THREE.ExtrudeGeometry(shape, { depth: ramp.width, bevelEnabled: false });
      geo.translate(0, 0, -ramp.width / 2);
      geo.rotateY(-Math.PI / 2); // profile now runs along +z
      const mesh = new THREE.Mesh(geo, rampMat);
      mesh.position.set(ramp.x, 0, ramp.z);
      mesh.rotation.y = ramp.angle;
      mesh.castShadow = mesh.receiveShadow = true;
      this.group.add(mesh);
      // Hazard stripes along the lip.
      for (const side of [-1, 1]) {
        const s = new THREE.Mesh(new THREE.BoxGeometry(0.3, 0.05, 0.3), stripeMat);
        s.position.set(side * (ramp.width / 2 - 0.3), ramp.height + 0.02, -L / 2 + L * RISE);
        mesh.add(s);
      }
      this.ramps.push(ramp);
    }

    // Cone slalom and a cone box to smash through.
    const coneGeo = new THREE.ConeGeometry(0.28, CONE_HEIGHT, 14).translate(0, CONE_HEIGHT / 2, 0);
    const coneMat = new THREE.MeshStandardMaterial({ color: 0xff6a13, roughness: 0.6 });
    const place = (x: number, z: number) => {
      const mesh = new THREE.Mesh(coneGeo, coneMat);
      mesh.position.set(x, 0, z);
      mesh.castShadow = true;
      this.group.add(mesh);
      this.cones.push({ mesh, home: mesh.position.clone(), vel: new THREE.Vector3(), spin: new THREE.Vector3(), knocked: false, settled: false, knockedAt: 0 });
    };
    for (let i = 0; i < 10; i++) place(bx - 28 + i * 5.5, bz + 4 + (i % 2) * 2);
    for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) place(bx - 22 + i * 1.6, bz + 26 + j * 1.6);
    for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) place(bx + 2 + i * 1.6, bz - 26 + j * 1.6);
  }

  private buildEdges(): void {
    // Barrier walls around the edge.
    const wallMat = new THREE.MeshStandardMaterial({ color: 0xb8bcc2, roughness: 0.9 });
    const len = HALF_SIZE * 2 + 2;
    for (const [x, z, w, d] of [
      [0, HALF_SIZE + 1, len, 2],
      [0, -HALF_SIZE - 1, len, 2],
      [HALF_SIZE + 1, 0, 2, len],
      [-HALF_SIZE - 1, 0, 2, len],
    ]) {
      const wall = new THREE.Mesh(new THREE.BoxGeometry(w, 1.4, d), wallMat);
      wall.position.set(x, 0.7, z);
      wall.receiveShadow = true;
      this.group.add(wall);
      this.boxes.push({ minX: x - w / 2, maxX: x + w / 2, minZ: z - d / 2, maxZ: z + d / 2, top: 1.4 });
    }
    // Hills on the horizon.
    const hillGeo = new THREE.ConeGeometry(1, 1, 7);
    const count = 70;
    const hills = new THREE.InstancedMesh(hillGeo, new THREE.MeshStandardMaterial({ color: 0x55734a, roughness: 1, flatShading: true }), count);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    for (let i = 0; i < count; i++) {
      const a = (i / count) * Math.PI * 2;
      const dist = 340 + this.rand() * 80;
      const r = 50 + this.rand() * 60;
      const h = 30 + this.rand() * 60;
      q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), this.rand() * Math.PI);
      m.compose(new THREE.Vector3(Math.cos(a) * dist, h / 2 - 2, Math.sin(a) * dist), q, new THREE.Vector3(r, h, r));
      hills.setMatrixAt(i, m);
    }
    this.group.add(hills);
  }

  // ---------- runtime ----------

  /** Ground height at a point (non-zero only on ramps). */
  heightAt(x: number, z: number): number {
    let best = 0;
    for (const r of this.ramps) {
      const dx = x - r.x;
      const dz = z - r.z;
      const c = Math.cos(r.angle);
      const s = Math.sin(r.angle);
      // Into the ramp's local frame: u runs up the ramp, v across it.
      const u = dx * s + dz * c;
      const v = dx * c - dz * s;
      if (Math.abs(v) > r.width / 2 || Math.abs(u) > r.length / 2) continue;
      const fromStart = u + r.length / 2;
      const peak = r.length * RISE;
      const h = fromStart <= peak ? (r.height * fromStart) / peak : (r.height * (r.length - fromStart)) / (r.length - peak);
      if (h > best) best = h;
    }
    // Rubble from destroyed buildings: a flat-topped mound you can drive over.
    for (const b of this.buildings) {
      if (b.pile <= 0) continue;
      const m = Math.max(Math.abs(x - b.x) / (b.width / 2), Math.abs(z - b.z) / (b.depth / 2));
      if (m >= 1) continue;
      const h = b.pile * pileShape(m);
      if (h > best) best = h;
    }
    return best;
  }

  /**
   * Knocks over cones the car touches. `carY` is the height of the car's base
   * (cones stand at y = 0), so a car flying over a cone misses it.
   * Returns how many were hit.
   */
  hitCones(x: number, z: number, carY: number, radius: number, vx: number, vz: number): number {
    // The underside of every body sits at least 0.2 m above the car's base.
    if (carY + 0.2 > CONE_HEIGHT) return 0;
    let hits = 0;
    for (const c of this.cones) {
      if (c.knocked) continue;
      const dx = c.mesh.position.x - x;
      const dz = c.mesh.position.z - z;
      if (dx * dx + dz * dz > (radius + 0.3) ** 2) continue;
      const speed = Math.hypot(vx, vz);
      this.knockCone(c, vx * 1.1 + dx * 2, 2 + speed * 0.18, vz * 1.1 + dz * 2);
      hits++;
    }
    return hits;
  }

  /** Sends a cone flying with the given velocity. */
  knockCone(c: Cone, vx: number, vy: number, vz: number): void {
    c.knocked = true;
    c.settled = false;
    c.knockedAt = this.clock;
    c.vel.set(vx, vy, vz);
    c.spin.set((Math.random() - 0.5) * 14, (Math.random() - 0.5) * 6, (Math.random() - 0.5) * 14);
  }

  // ---------- weapons and destruction ----------

  /**
   * Finds the first thing a ray from `origin` along `dir` (a unit vector) hits
   * within `maxDist`: a building, car, wall, lamp, tree, cone or the ground.
   */
  raycast(origin: THREE.Vector3, dir: THREE.Vector3, maxDist: number): RayHit | null {
    let best = maxDist;
    let hit: RayHit | null = null;
    const ox = origin.x, oy = origin.y, oz = origin.z;
    const dx = dir.x, dy = dir.y, dz = dir.z;
    const found = (t: number, nx: number, ny: number, nz: number, target: Target | null, cone: Cone | null, ground: boolean) => {
      best = t;
      hit = {
        distance: t,
        point: new THREE.Vector3(ox + dx * t, oy + dy * t, oz + dz * t),
        normal: new THREE.Vector3(nx, ny, nz),
        target, cone, ground,
      };
    };

    if (dy < -1e-6) {
      const t = -oy / dy;
      if (t >= 0 && t < best) found(t, 0, 1, 0, null, null, true);
    }
    for (const b of this.boxes) {
      if (b.off) continue;
      const r = rayBox(ox, oy, oz, dx, dy, dz, b.minX, 0, b.minZ, b.maxX, b.top, b.maxZ, best);
      if (r) found(r.t, r.nx, r.ny, r.nz, b.target ?? null, null, false);
    }
    for (const c of this.circles) {
      if (c.off) continue;
      const t = rayCylinder(ox, oy, oz, dx, dy, dz, c.x, c.z, c.hitR ?? c.r, c.top, best);
      if (t !== null) {
        const px = ox + dx * t - c.x;
        const pz = oz + dz * t - c.z;
        const len = Math.hypot(px, pz) || 1;
        found(t, px / len, 0, pz / len, c.target ?? null, null, false);
      }
    }
    for (const c of this.cones) {
      if (c.knocked) continue;
      const t = rayCylinder(ox, oy, oz, dx, dy, dz, c.mesh.position.x, c.mesh.position.z, 0.3, CONE_HEIGHT, best);
      if (t !== null) found(t, -dx, 0, -dz, null, c, false);
    }
    return hit;
  }

  /** Applies a bullet or rocket hit: damages what was hit or knocks a cone over. */
  applyHit(hit: RayHit, damage: number, dir: THREE.Vector3): void {
    if (hit.target) this.damage(hit.target, damage, dir);
    else if (hit.cone) this.knockCone(hit.cone, dir.x * 8, 4, dir.z * 8);
  }

  /** Damages everything near a point. Returns how many things it destroyed. */
  explode(at: THREE.Vector3, radius: number, damage: number): number {
    let destroyed = 0;
    for (const t of this.targets) {
      if (!t.alive) continue;
      const d = this.distanceTo(t, at);
      if (d > radius) continue;
      const falloff = 1 - (d / radius) * 0.7;
      const dir = new THREE.Vector3(t.x - at.x, 0, t.z - at.z).normalize();
      if (this.damage(t, damage * falloff, dir)) destroyed++;
    }
    for (const c of this.cones) {
      const p = c.mesh.position;
      const dx = p.x - at.x;
      const dz = p.z - at.z;
      const d = Math.hypot(dx, dz);
      if (d > radius * 1.3) continue;
      const push = (1 - d / (radius * 1.3)) * 18 + 3;
      this.knockCone(c, (dx / (d || 1)) * push, 4 + push * 0.5, (dz / (d || 1)) * push);
    }
    return destroyed;
  }

  /** Damages one thing. Returns true if this destroyed it. */
  damage(t: Target, amount: number, dir: THREE.Vector3): boolean {
    if (!t.alive) {
      // Wrecked cars can still be knocked about.
      if (t.kind === 'car' && !t.flying && amount > 30) this.launchCar(t, dir, amount * 0.05);
      return false;
    }
    t.hp -= amount;
    if (t.hp > 0) return false;
    this.destroy(t, dir);
    return true;
  }

  /** A car hitting a lamp or tree fast enough knocks it straight over. */
  ram(t: Target, speed: number, dir: THREE.Vector3): boolean {
    if (!t.alive || (t.kind !== 'lamp' && t.kind !== 'tree') || speed < 11) return false;
    this.destroy(t, dir);
    return true;
  }

  /** Puts everything that was destroyed back at once. */
  rebuildAll(): void {
    for (const t of this.targets) if (!t.alive || (t.kind === 'building' && t.phase !== 'standing')) this.restore(t, false);
    for (const t of this.targets) t.hp = t.maxHp;
    this.effects.clear();
  }

  private distanceTo(t: Target, p: THREE.Vector3): number {
    if (t.kind === 'building' || t.kind === 'car') {
      const b = t.box;
      const dx = Math.max(b.minX - p.x, 0, p.x - b.maxX);
      const dy = Math.max(-p.y, 0, p.y - b.top);
      const dz = Math.max(b.minZ - p.z, 0, p.z - b.maxZ);
      return Math.hypot(dx, dy, dz);
    }
    const c = t.circle;
    const dy = Math.max(-p.y, 0, p.y - c.top);
    return Math.max(0, Math.hypot(p.x - c.x, p.z - c.z) - c.r * 2) + dy * 0.5;
  }

  private destroy(t: Target, dir: THREE.Vector3): void {
    t.alive = false;
    t.hp = 0;
    t.destroyedAt = this.clock;
    const fx = this.effects;
    switch (t.kind) {
      case 'building': {
        t.phase = 'collapsing';
        t.t = 0;
        t.box.off = true;
        // Lean away from whatever hit it.
        t.tilt.set(dir.z * 0.12, -dir.x * 0.12);
        if (!t.rubble) t.rubble = this.makeRubble(t);
        t.rubble.visible = true;
        t.rubble.scale.y = 0.001;
        const color = (t.mesh.material as THREE.MeshStandardMaterial).color.getHex();
        const mid = new THREE.Vector3(t.x, t.height * 0.5, t.z);
        fx.explosion(new THREE.Vector3(t.x, 2, t.z), 2.2);
        fx.shatter(mid, 40, [color, 0x6d6a66, 0x8a8783], 1.6, 9);
        fx.dustAround(t.x, t.z, t.width / 2, t.depth / 2, 60, 1.3);
        break;
      }
      case 'car': {
        t.rig.setWrecked(true);
        t.burnUntil = this.clock + 15;
        this.launchCar(t, dir, 1);
        fx.explosion(new THREE.Vector3(t.rig.root.position.x, 0.8, t.rig.root.position.z), 1.6);
        fx.shatter(new THREE.Vector3(t.rig.root.position.x, 0.8, t.rig.root.position.z), 10, [0x222222, 0x444444, 0x9cc3d9], 0.5, 8);
        break;
      }
      case 'lamp':
      case 'tree': {
        t.circle.off = true;
        this.setInstanceVisible(t, false);
        const lamp = t.kind === 'lamp';
        const height = lamp ? 7 : 2.4 * t.scale;
        // The pole or trunk topples away, in the direction it was hit.
        fx.chunk({
          pos: new THREE.Vector3(t.x + dir.x, height / 2, t.z + dir.z),
          vel: new THREE.Vector3(dir.x * 6, 3, dir.z * 6),
          scale: lamp ? new THREE.Vector3(0.24, 7, 0.24) : new THREE.Vector3(0.5 * t.scale, 2.4 * t.scale, 0.5 * t.scale),
          color: lamp ? 0x3a3d42 : 0x6b4a2f,
          spin: 4,
        });
        if (lamp) {
          fx.shatter(new THREE.Vector3(t.x, 7, t.z), 4, [0xffffff, 0xcccccc], 0.3, 5);
          for (let i = 0; i < 20; i++) {
            fx.spark({
              x: t.x, y: 7, z: t.z,
              vx: (Math.random() - 0.5) * 8, vy: Math.random() * 5, vz: (Math.random() - 0.5) * 8,
              life: 0.4 + Math.random() * 0.5, size: [0.2, 0.05], color: [0.7, 0.85, 1], gravity: 12,
            });
          }
        } else {
          fx.shatter(new THREE.Vector3(t.x, 3.6 * t.scale, t.z), 14, [0x4f8a3a, 0x3f7a2d, 0x5f9a45], 0.9 * t.scale, 5);
        }
        break;
      }
    }
    this.onDestroyed(t);
  }

  private launchCar(t: CarTarget, dir: THREE.Vector3, strength: number): void {
    t.flying = true;
    t.box.off = true;
    t.vel.set(dir.x * 5 * strength + (Math.random() - 0.5) * 2, 8 + 4 * Math.min(1.5, strength), dir.z * 5 * strength + (Math.random() - 0.5) * 2);
    t.spin.set((Math.random() - 0.5) * 6, (Math.random() - 0.5) * 4, (Math.random() - 0.5) * 6);
  }

  private setInstanceVisible(t: PropTarget, visible: boolean): void {
    const m = visible ? t.matrix : new THREE.Matrix4().makeScale(0, 0, 0);
    for (const mesh of t.meshes) {
      mesh.setMatrixAt(t.index, m);
      mesh.instanceMatrix.needsUpdate = true;
    }
  }

  /** Brings a destroyed thing back. `animate` raises buildings from the ground. */
  private restore(t: Target, animate: boolean): void {
    t.alive = true;
    t.hp = t.maxHp;
    switch (t.kind) {
      case 'building':
        t.box.off = false;
        t.mesh.visible = true;
        t.mesh.rotation.set(0, 0, 0);
        if (animate) {
          t.phase = 'rising';
          t.t = 0;
        } else {
          t.phase = 'standing';
          t.mesh.position.set(t.x, t.height / 2, t.z);
          t.pile = 0;
          if (t.rubble) t.rubble.visible = false;
        }
        break;
      case 'car':
        t.rig.setWrecked(false);
        t.flying = false;
        t.burnUntil = 0;
        t.rig.root.position.copy(t.home);
        t.rig.root.rotation.set(0, t.homeHeading, 0);
        this.fitCarBox(t);
        t.box.off = false;
        break;
      default:
        t.circle.off = false;
        this.setInstanceVisible(t, true);
    }
  }

  /** Fits a car's collision box around it after it has moved. */
  private fitCarBox(t: CarTarget): void {
    const p = t.rig.root.position;
    const c = Math.abs(Math.cos(t.rig.root.rotation.y));
    const s = Math.abs(Math.sin(t.rig.root.rotation.y));
    const hw = t.rig.width / 2 + 0.05;
    const hl = t.rig.length / 2;
    const ex = c * hw + s * hl;
    const ez = s * hw + c * hl;
    Object.assign(t.box, { minX: p.x - ex, maxX: p.x + ex, minZ: p.z - ez, maxZ: p.z + ez });
    t.x = p.x;
    t.z = p.z;
  }

  /** A lumpy mound of rubble the size of the building's footprint. */
  private makeRubble(b: BuildingTarget): THREE.Mesh {
    const nx = Math.max(6, Math.round(b.width / 1.5));
    const nz = Math.max(6, Math.round(b.depth / 1.5));
    const geo = new THREE.PlaneGeometry(b.width, b.depth, nx, nz).rotateX(-Math.PI / 2);
    const pos = geo.getAttribute('position') as THREE.BufferAttribute;
    for (let i = 0; i < pos.count; i++) {
      const m = Math.max(Math.abs(pos.getX(i)) / (b.width / 2), Math.abs(pos.getZ(i)) / (b.depth / 2));
      const lump = m < 0.98 ? (Math.random() - 0.3) * 0.35 : 0;
      pos.setY(i, Math.max(0, PILE_HEIGHT * pileShape(m) + lump));
    }
    geo.computeVertexNormals();
    const base = b.mesh.material as THREE.MeshStandardMaterial;
    let mat = this.rubbleMats.get(base);
    if (!mat) {
      mat = new THREE.MeshStandardMaterial({ color: base.color.clone().multiplyScalar(0.7), roughness: 1, flatShading: true });
      this.rubbleMats.set(base, mat);
    }
    const mesh = new THREE.Mesh(geo, mat);
    mesh.position.set(b.x, 0.02, b.z);
    mesh.castShadow = mesh.receiveShadow = true;
    this.group.add(mesh);
    return mesh;
  }

  update(dt: number, carX: number, carZ: number): void {
    this.clock += dt;
    this.carX = carX;
    this.carZ = carZ;
    this.updateTargets(dt);
    for (const c of this.cones) {
      if (!c.knocked) continue;
      const p = c.mesh.position;
      if (!c.settled) {
        c.vel.y -= 14 * dt;
        p.addScaledVector(c.vel, dt);
        c.mesh.rotation.x += c.spin.x * dt;
        c.mesh.rotation.y += c.spin.y * dt;
        c.mesh.rotation.z += c.spin.z * dt;
        if (p.y < 0) {
          p.y = 0;
          c.vel.y = Math.abs(c.vel.y) * 0.3;
          c.vel.x *= 0.6;
          c.vel.z *= 0.6;
          c.spin.multiplyScalar(0.6);
          if (c.vel.lengthSq() < 0.5) {
            c.vel.set(0, 0, 0);
            // Settle lying on its side.
            c.mesh.rotation.set(Math.PI / 2, c.mesh.rotation.y, 0);
            p.y = CONE_LYING_Y;
            c.settled = true;
          }
        }
      }
      // Put cones back once the player has driven away.
      if (this.clock - c.knockedAt > 12 && Math.hypot(carX - c.home.x, carZ - c.home.z) > 40) {
        c.knocked = false;
        c.settled = false;
        c.vel.set(0, 0, 0);
        c.mesh.position.copy(c.home);
        c.mesh.rotation.set(0, 0, 0);
      }
    }
    // Keep the sun's shadow box centred on the car.
    this.sun.position.set(carX + 40, 80, carZ + 25);
    this.sun.target.position.set(carX, 0, carZ);
  }

  private updateTargets(dt: number): void {
    const fx = this.effects;
    for (const t of this.targets) {
      if (t.kind === 'building') {
        if (t.phase === 'collapsing') {
          t.t += dt;
          const k = Math.min(1, t.t / COLLAPSE_TIME);
          const fall = k * k; // speeds up as it goes
          t.mesh.position.set(t.x + (Math.random() - 0.5) * 0.3 * (1 - k), t.height / 2 - t.height * fall, t.z + (Math.random() - 0.5) * 0.3 * (1 - k));
          t.mesh.rotation.set(t.tilt.x * k, 0, t.tilt.y * k);
          t.pile = PILE_HEIGHT * k;
          if (t.rubble) t.rubble.scale.y = Math.max(0.001, k);
          if (Math.random() < dt * 30) fx.dustAround(t.x, t.z, t.width / 2, t.depth / 2, 4, 1.3);
          if (k >= 1) {
            t.phase = 'rubble';
            t.mesh.visible = false;
          }
        } else if (t.phase === 'rising') {
          t.t += dt;
          const k = Math.min(1, t.t / RISE_TIME);
          const up = 1 - (1 - k) ** 3;
          t.mesh.position.set(t.x, t.height / 2 - t.height * (1 - up), t.z);
          t.pile = PILE_HEIGHT * (1 - k);
          if (t.rubble) t.rubble.scale.y = Math.max(0.001, 1 - k);
          if (k >= 1) {
            t.phase = 'standing';
            t.pile = 0;
            if (t.rubble) t.rubble.visible = false;
          }
        } else if (t.alive && t.hp < t.maxHp * 0.6) {
          // Badly damaged buildings burn on the roof.
          fx.burn(new THREE.Vector3(t.x, t.height, t.z), 1.2 * (1 - t.hp / t.maxHp), dt);
        }
      } else if (t.kind === 'car' && !t.alive) {
        const root = t.rig.root;
        if (t.flying) {
          t.vel.y -= 16 * dt;
          root.position.addScaledVector(t.vel, dt);
          root.rotation.x += t.spin.x * dt;
          root.rotation.y += t.spin.y * dt;
          root.rotation.z += t.spin.z * dt;
          if (root.position.y <= 0 && t.vel.y < 0) {
            root.position.y = 0;
            if (t.vel.y < -5) {
              t.vel.set(t.vel.x * 0.5, -t.vel.y * 0.3, t.vel.z * 0.5);
              t.spin.multiplyScalar(0.4);
              fx.dust(root.position, 1.5, 6);
            } else {
              t.flying = false;
              root.rotation.set(0, root.rotation.y, 0);
              // Keep wrecks inside the city walls.
              root.position.x = THREE.MathUtils.clamp(root.position.x, -HALF_SIZE + 3, HALF_SIZE - 3);
              root.position.z = THREE.MathUtils.clamp(root.position.z, -HALF_SIZE + 3, HALF_SIZE - 3);
              this.fitCarBox(t);
              t.box.off = false;
            }
          }
        }
        if (this.clock < t.burnUntil) fx.burn(new THREE.Vector3(root.position.x, root.position.y + 0.8, root.position.z), 0.9, dt);
      }

      // Bring things back once the player has been away for a while.
      if (!t.alive && !(t.kind === 'car' && t.flying) && this.clock - t.destroyedAt > RESPAWN_AFTER) {
        if (Math.hypot(this.carX - t.x, this.carZ - t.z) > RESPAWN_DISTANCE) this.restore(t, true);
      }
    }
  }

  /** Shadow quality: map size and how far around the car shadows reach. */
  setShadows(on: boolean, mapSize: number, extent: number): void {
    this.sun.castShadow = on;
    if (this.sun.shadow.mapSize.x !== mapSize) {
      this.sun.shadow.mapSize.set(mapSize, mapSize);
      this.sun.shadow.map?.dispose();
      this.sun.shadow.map = null;
    }
    const s = this.sun.shadow.camera;
    s.left = s.bottom = -extent;
    s.right = s.top = extent;
    s.updateProjectionMatrix();
  }

  /** How far you can see before everything fades into the fog. */
  setViewDistance(distance: number): void {
    this.viewDistance = distance;
    this.setNight(this.night);
  }

  setNight(night: boolean): void {
    this.night = night;
    this.scene.background = night ? this.nightBg : this.dayBg;
    this.scene.fog = new THREE.Fog(night ? 0x0b1222 : 0xc4dcef, Math.min(60, this.viewDistance * 0.2), this.viewDistance);
    this.hemi.intensity = night ? 0.25 : 1.2;
    this.hemi.color.set(night ? 0x6b7fb3 : 0xdfefff);
    this.sun.intensity = night ? 0.35 : 2.6;
    this.sun.color.set(night ? 0x9bb0ff : 0xfff2dd);
    for (const m of this.buildingMats) m.emissiveIntensity = night ? 1.4 : 0;
    this.lampHeadMat.emissiveIntensity = night ? 3 : 0;
    for (const car of this.parkedCars) car.setLights({ night: false, braking: false });
  }
}

/** Rubble mound profile: flat on top, sloping down over the outer 40%. */
function pileShape(m: number): number {
  return Math.min(1, (1 - m) * 2.5);
}

/** Ray against an axis-aligned box (slab method). Returns distance and surface normal. */
function rayBox(
  ox: number, oy: number, oz: number,
  dx: number, dy: number, dz: number,
  minX: number, minY: number, minZ: number,
  maxX: number, maxY: number, maxZ: number,
  maxT: number,
): { t: number; nx: number; ny: number; nz: number } | null {
  let tMin = 0;
  let tMax = maxT;
  let axis = -1;
  let sign = 0;
  const o = [ox, oy, oz];
  const d = [dx, dy, dz];
  const lo = [minX, minY, minZ];
  const hi = [maxX, maxY, maxZ];
  for (let a = 0; a < 3; a++) {
    if (Math.abs(d[a]) < 1e-9) {
      if (o[a] < lo[a] || o[a] > hi[a]) return null;
      continue;
    }
    let t1 = (lo[a] - o[a]) / d[a];
    let t2 = (hi[a] - o[a]) / d[a];
    let s = -1;
    if (t1 > t2) {
      [t1, t2] = [t2, t1];
      s = 1;
    }
    if (t1 > tMin) {
      tMin = t1;
      axis = a;
      sign = s;
    }
    tMax = Math.min(tMax, t2);
    if (tMin > tMax) return null;
  }
  if (axis < 0) return null; // started inside the box
  return { t: tMin, nx: axis === 0 ? sign : 0, ny: axis === 1 ? sign : 0, nz: axis === 2 ? sign : 0 };
}

/** Ray against an upright cylinder standing on the ground. Returns the distance. */
function rayCylinder(
  ox: number, oy: number, oz: number,
  dx: number, dy: number, dz: number,
  cx: number, cz: number, r: number, top: number,
  maxT: number,
): number | null {
  const a = dx * dx + dz * dz;
  if (a < 1e-9) return null;
  const px = ox - cx;
  const pz = oz - cz;
  const b = 2 * (px * dx + pz * dz);
  const c = px * px + pz * pz - r * r;
  const disc = b * b - 4 * a * c;
  if (disc < 0) return null;
  const t = (-b - Math.sqrt(disc)) / (2 * a);
  if (t < 0 || t > maxT) return null;
  const y = oy + dy * t;
  return y >= 0 && y <= top ? t : null;
}
