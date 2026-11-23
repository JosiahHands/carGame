// The drivable world: a city grid with buildings, parks, a car park and a stunt park.
//
// Buildings, parked cars, street lamps and trees can be destroyed ("targets").
// Buildings sink into a pile of rubble you can drive over; parked cars blow up
// into burning wrecks; lamps and trees fall over. Everything comes back a while
// after the player has driven away.
import * as THREE from 'three';
import { buildCar, type CarRig } from './carModel';
import { DEFAULT_CONFIG, randomConfig } from './carConfig';
import type { Effects } from './effects';
import { concreteTexture, grassTexture, roadTexture, windowTexture } from './textures';
import { gradientTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';

/** A solid box standing on the ground (building, parked car, wall). */
export interface Box {
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
  height: number;
  /** False once what it belongs to has been destroyed, so cars drive through it. */
  solid: boolean;
  target?: Target;
}

/** A solid upright post (lamp pole or tree trunk). */
export interface Circle {
  x: number;
  z: number;
  r: number;
  height: number;
  /** Radius bullets can hit, which is bigger than the trunk for a tree's leaves. */
  hitRadius: number;
  solid: boolean;
  target?: Target;
}

/** What every destroyable thing has. */
interface TargetBase {
  x: number;
  z: number;
  hp: number;
  maxHp: number;
  alive: boolean;
  /** World clock time when it was destroyed, for bringing it back later. */
  destroyedAt: number;
  /** Damage score for destroying it, in dollars. */
  value: number;
}

export interface BuildingTarget extends TargetBase {
  kind: 'building';
  box: Box;
  mesh: THREE.Mesh;
  width: number;
  depth: number;
  height: number;
  /** Standing, then falling into the ground, then rubble, then rising back up. */
  state: 'standing' | 'falling' | 'rubble' | 'rising';
  /** Seconds since the current fall or rise began. */
  timer: number;
  /** How far it tips over while falling (around x and z). */
  lean: THREE.Vector2;
  rubble: THREE.Mesh | null;
  /** Height of the rubble pile right now; it grows as the building falls. */
  rubbleHeight: number;
}

export interface CarTarget extends TargetBase {
  kind: 'car';
  box: Box;
  rig: CarRig;
  homeX: number;
  homeZ: number;
  homeHeading: number;
  /** While a wreck is flying through the air. */
  flying: boolean;
  velocity: THREE.Vector3;
  spin: THREE.Vector3;
  burnUntil: number;
}

/** A street lamp or tree. These are drawn many at once (instanced), so each one is a slot. */
export interface PropTarget extends TargetBase {
  kind: 'lamp' | 'tree';
  circle: Circle;
  meshes: THREE.InstancedMesh[];
  index: number;
  /** Its position, turn and size in the instanced meshes. */
  matrix: THREE.Matrix4;
  size: number;
}

export type Target = BuildingTarget | CarTarget | PropTarget;

/** What a bullet or rocket hit. */
export interface RayHit {
  distance: number;
  point: THREE.Vector3;
  target: Target | null;
  cone: Cone | null;
  ground: boolean;
}

/** One city block, for the minimap. */
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
const FALL_TIME = 2.4; // seconds for a destroyed building to sink into the ground
const RISE_TIME = 2; // seconds for a building to be rebuilt
const RUBBLE_HEIGHT = 1.6; // metres
const COMES_BACK_AFTER = 45; // seconds before destroyed things come back...
const COMES_BACK_DISTANCE = 60; // ...as long as the player is at least this far away

export class World {
  readonly group = new THREE.Group();
  readonly boxes: Box[] = [];
  readonly circles: Circle[] = [];
  readonly targets: Target[] = [];
  readonly blocks: Block[] = [];
  readonly ramps: Ramp[] = [];
  readonly spawn = { x: -3, z: -20, heading: 0 };
  /** Called whenever something is destroyed (for the score and sounds). */
  onDestroyed: (target: Target) => void = () => {};
  private buildings: BuildingTarget[] = [];
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
      // Each lamp can be shot or knocked down (3 bullets).
      const circle: Circle = { x: p.x, z: p.z, r: 0.25, height: 7.3, hitRadius: 0.3, solid: true };
      const lamp: PropTarget = {
        kind: 'lamp', x: p.x, z: p.z, hp: 25, maxHp: 25, alive: true, destroyedAt: 0, value: 1500,
        circle, meshes: [poleMesh, headMesh], index: i, matrix: m.clone(), size: 1,
      };
      circle.target = lamp;
      this.circles.push(circle);
      this.targets.push(lamp);
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
    const box: Box = { minX: x - w / 2, maxX: x + w / 2, minZ: z - d / 2, maxZ: z + d / 2, height: h, solid: true };
    // Taller buildings are tougher (2 to 4 rockets) and worth more.
    const hp = Math.round(120 + h * 6);
    const building: BuildingTarget = {
      kind: 'building', x, z, hp, maxHp: hp, alive: true, destroyedAt: 0, value: Math.round(40000 + h * 2500),
      box, mesh, width: w, depth: d, height: h,
      state: 'standing', timer: 0, lean: new THREE.Vector2(), rubble: null, rubbleHeight: 0,
    };
    box.target = building;
    this.boxes.push(box);
    this.targets.push(building);
    this.buildings.push(building);
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
      // The car bumps into the trunk, but bullets hit the leaves too.
      const circle: Circle = { x: p.x, z: p.z, r: 0.45 * s, height: 5.4 * s, hitRadius: 1.4 * s, solid: true };
      const tree: PropTarget = {
        kind: 'tree', x: p.x, z: p.z, hp: 30, maxHp: 30, alive: true, destroyedAt: 0, value: 800,
        circle, meshes: [trunks, leaves], index: i, matrix: m.clone(), size: s,
      };
      circle.target = tree;
      this.circles.push(circle);
      this.targets.push(tree);
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
          const box: Box = { minX: x - hw, maxX: x + hw, minZ: z - hl, maxZ: z + hl, height: 1.4, solid: true };
          const parked: CarTarget = {
            kind: 'car', x, z, hp: 70, maxHp: 70, alive: true, destroyedAt: 0, value: 12000,
            box, rig: car, homeX: x, homeZ: z, homeHeading: car.root.rotation.y,
            velocity: new THREE.Vector3(), spin: new THREE.Vector3(), flying: false, burnUntil: 0,
          };
          box.target = parked;
          this.boxes.push(box);
          this.targets.push(parked);
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
      this.boxes.push({ minX: x - w / 2, maxX: x + w / 2, minZ: z - d / 2, maxZ: z + d / 2, height: 1.4, solid: true });
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

  // ---------- driving ----------

  /** Ground height at a point: non-zero on ramps and rubble piles. */
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
    for (const b of this.buildings) {
      if (b.rubbleHeight <= 0) continue;
      const h = b.rubbleHeight * rubbleShape(x - b.x, z - b.z, b.width, b.depth);
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
      this.knockCone(c, new THREE.Vector3(vx * 1.1 + dx * 2, 2 + speed * 0.18, vz * 1.1 + dz * 2));
      hits++;
    }
    return hits;
  }

  /** Sends a cone flying. */
  private knockCone(c: Cone, velocity: THREE.Vector3): void {
    c.knocked = true;
    c.settled = false;
    c.knockedAt = this.clock;
    c.vel.copy(velocity);
    c.spin.set((Math.random() - 0.5) * 14, (Math.random() - 0.5) * 6, (Math.random() - 0.5) * 14);
  }

  /**
   * A car driving into a lamp or tree fast enough (40 km/h) knocks it straight
   * over instead of stopping. Returns true if it did.
   */
  ram(t: Target, speed: number, direction: THREE.Vector3): boolean {
    if (!t.alive || (t.kind !== 'lamp' && t.kind !== 'tree') || speed < 11) return false;
    this.destroy(t, direction);
    return true;
  }

  // ---------- weapons ----------

  /**
   * Finds the first thing a ray hits: a building, car, wall, lamp, tree, cone
   * or the ground. `direction` must have length 1. Returns null if nothing is
   * hit within `maxDistance`.
   *
   * Everything is treated as a box here (a tree is the box around its leaves),
   * which is close enough for bullets.
   */
  raycast(origin: THREE.Vector3, direction: THREE.Vector3, maxDistance: number): RayHit | null {
    const ray = new THREE.Ray(origin, direction);
    const box = new THREE.Box3();
    const point = new THREE.Vector3();
    let best: RayHit | null = null;
    const consider = (target: Target | null, cone: Cone | null, ground: boolean) => {
      const distance = origin.distanceTo(point);
      if (distance <= maxDistance && (!best || distance < best.distance)) {
        best = { distance, point: point.clone(), target, cone, ground };
      }
    };

    if (ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 1, 0), 0), point)) consider(null, null, true);
    for (const b of this.boxes) {
      if (!b.solid) continue;
      box.min.set(b.minX, 0, b.minZ);
      box.max.set(b.maxX, b.height, b.maxZ);
      if (ray.intersectBox(box, point)) consider(b.target ?? null, null, false);
    }
    for (const c of this.circles) {
      if (!c.solid) continue;
      box.min.set(c.x - c.hitRadius, 0, c.z - c.hitRadius);
      box.max.set(c.x + c.hitRadius, c.height, c.z + c.hitRadius);
      if (ray.intersectBox(box, point)) consider(c.target ?? null, null, false);
    }
    for (const c of this.cones) {
      if (c.knocked) continue;
      const p = c.mesh.position;
      box.min.set(p.x - 0.3, 0, p.z - 0.3);
      box.max.set(p.x + 0.3, CONE_HEIGHT, p.z + 0.3);
      if (ray.intersectBox(box, point)) consider(null, c, false);
    }
    return best;
  }

  /** A bullet or rocket hit: damages what was hit, or knocks a cone flying. */
  applyHit(hit: RayHit, damage: number, direction: THREE.Vector3): void {
    if (hit.target) this.damage(hit.target, damage, direction);
    if (hit.cone) this.knockCone(hit.cone, new THREE.Vector3(direction.x * 8, 4, direction.z * 8));
  }

  /**
   * Damages everything within `radius` of a point. Things at the centre take
   * full damage and things at the edge take 30%. Cones nearby go flying.
   */
  explode(at: THREE.Vector3, radius: number, damage: number): void {
    const box = new THREE.Box3();
    for (const t of this.targets) {
      if (!t.alive) continue;
      // Distance to the nearest point of the thing, not its centre, so a
      // rocket hitting the side of a big building still counts as close.
      if (t.kind === 'building' || t.kind === 'car') {
        box.min.set(t.box.minX, 0, t.box.minZ);
        box.max.set(t.box.maxX, t.box.height, t.box.maxZ);
      } else {
        box.min.set(t.x - t.circle.r * 2, 0, t.z - t.circle.r * 2);
        box.max.set(t.x + t.circle.r * 2, t.circle.height, t.z + t.circle.r * 2);
      }
      const distance = box.distanceToPoint(at);
      if (distance > radius) continue;
      const away = new THREE.Vector3(t.x - at.x, 0, t.z - at.z).normalize();
      this.damage(t, damage * (1 - 0.7 * (distance / radius)), away);
    }
    for (const c of this.cones) {
      const away = new THREE.Vector3(c.mesh.position.x - at.x, 0, c.mesh.position.z - at.z);
      const distance = away.length();
      if (distance > radius * 1.3) continue;
      const push = 3 + 18 * (1 - distance / (radius * 1.3));
      away.normalize().multiplyScalar(push);
      this.knockCone(c, away.setY(4 + push * 0.5));
    }
  }

  /** Takes `amount` of health away, destroying the thing when it runs out. */
  damage(t: Target, amount: number, direction: THREE.Vector3): void {
    if (!t.alive) return;
    t.hp -= amount;
    if (t.hp <= 0) this.destroy(t, direction);
  }

  /**
   * Puts everything that was destroyed back at once ("Rebuild the city").
   * Anything the player's car is on or next to (like a rubble pile) is left
   * to come back later as usual, so the car never ends up inside a solid building.
   */
  rebuildAll(carX: number, carZ: number): void {
    for (const t of this.targets) {
      if (!t.alive && this.isUnderCar(t, carX, carZ)) continue;
      if (!t.alive || (t.kind === 'building' && t.state !== 'standing')) this.restore(t, false);
      t.hp = t.maxHp;
    }
    this.effects.clear();
  }

  /** Whether `t`, once rebuilt, would overlap a car at (x, z). */
  private isUnderCar(t: Target, x: number, z: number): boolean {
    const margin = 3; // about half a car's length, plus a little room
    if (t.kind === 'building') {
      const b = t.box;
      return x > b.minX - margin && x < b.maxX + margin && z > b.minZ - margin && z < b.maxZ + margin;
    }
    // Parked cars come back at their parking spot, not where the wreck landed.
    if (t.kind === 'car') return Math.hypot(x - t.homeX, z - t.homeZ) < 2.5 + margin;
    return Math.hypot(x - t.circle.x, z - t.circle.z) < t.circle.r + margin;
  }

  /** Destroys something; `direction` is the way it was hit, so things fall away from you. */
  private destroy(t: Target, direction: THREE.Vector3): void {
    t.alive = false;
    t.hp = 0;
    t.destroyedAt = this.clock;
    const fx = this.effects;

    if (t.kind === 'building') {
      // Start sinking (see update), leaning away from the hit, with a big
      // explosion, flying chunks of wall and a cloud of dust.
      t.state = 'falling';
      t.timer = 0;
      t.box.solid = false;
      t.lean.set(direction.z * 0.12, -direction.x * 0.12);
      t.rubble ??= this.makeRubble(t);
      t.rubble.visible = true;
      t.rubble.scale.y = 0.001;
      const color = (t.mesh.material as THREE.MeshStandardMaterial).color.getHex();
      fx.explosion(new THREE.Vector3(t.x, 2, t.z), 2.2);
      fx.shatter(new THREE.Vector3(t.x, t.height / 2, t.z), 40, [color, 0x6d6a66, 0x8a8783], 1.6, 9);
      fx.dust(t.x, t.z, t.width / 2, t.depth / 2, 60, 1.3);
    } else if (t.kind === 'car') {
      // Blow it up into the air; it lands as a burning wreck (see update).
      const p = t.rig.root.position;
      t.rig.setWrecked(true);
      t.burnUntil = this.clock + 15;
      t.flying = true;
      t.box.solid = false;
      t.velocity.set(direction.x * 5 + (Math.random() - 0.5) * 2, 12, direction.z * 5 + (Math.random() - 0.5) * 2);
      t.spin.set((Math.random() - 0.5) * 6, (Math.random() - 0.5) * 4, (Math.random() - 0.5) * 6);
      fx.explosion(new THREE.Vector3(p.x, 0.8, p.z), 1.6);
      fx.shatter(new THREE.Vector3(p.x, 0.8, p.z), 10, [0x222222, 0x444444, 0x9cc3d9], 0.5, 8);
    } else {
      // Lamp or tree: hide it, and throw a pole or trunk that topples over.
      t.circle.solid = false;
      this.setPropVisible(t, false);
      const lamp = t.kind === 'lamp';
      const height = lamp ? 7 : 2.4 * t.size;
      fx.addDebris(
        new THREE.Vector3(t.x + direction.x, height / 2, t.z + direction.z),
        new THREE.Vector3(direction.x * 6, 3, direction.z * 6),
        lamp ? new THREE.Vector3(0.24, 7, 0.24) : new THREE.Vector3(0.5 * t.size, height, 0.5 * t.size),
        lamp ? 0x3a3d42 : 0x6b4a2f,
        4,
      );
      if (lamp) fx.shatter(new THREE.Vector3(t.x, 7, t.z), 4, [0xffffff, 0xcccccc], 0.3, 5); // the lamp's glass
      else fx.shatter(new THREE.Vector3(t.x, 3.6 * t.size, t.z), 14, [0x4f8a3a, 0x3f7a2d, 0x5f9a45], 0.9 * t.size, 5); // leaves
    }
    this.onDestroyed(t);
  }

  /** Brings a destroyed thing back. Buildings rise out of the ground if `animate` is set. */
  private restore(t: Target, animate: boolean): void {
    t.alive = true;
    t.hp = t.maxHp;
    if (t.kind === 'building') {
      t.box.solid = true;
      t.mesh.visible = true;
      t.mesh.rotation.set(0, 0, 0);
      if (animate) {
        t.state = 'rising';
        t.timer = 0;
      } else {
        t.state = 'standing';
        t.mesh.position.set(t.x, t.height / 2, t.z);
        t.rubbleHeight = 0;
        if (t.rubble) t.rubble.visible = false;
      }
    } else if (t.kind === 'car') {
      t.rig.setWrecked(false);
      t.flying = false;
      t.burnUntil = 0;
      t.rig.root.position.set(t.homeX, 0, t.homeZ);
      t.rig.root.rotation.set(0, t.homeHeading, 0);
      this.fitCarBox(t);
      t.box.solid = true;
    } else {
      t.circle.solid = true;
      this.setPropVisible(t, true);
    }
  }

  /** Shows or hides one lamp or tree. Hiding sets its size to zero. */
  private setPropVisible(t: PropTarget, visible: boolean): void {
    const matrix = visible ? t.matrix : new THREE.Matrix4().makeScale(0, 0, 0);
    for (const mesh of t.meshes) {
      mesh.setMatrixAt(t.index, matrix);
      mesh.instanceMatrix.needsUpdate = true;
    }
  }

  /** Moves a car's collision box to wherever the car (or wreck) now is. */
  private fitCarBox(t: CarTarget): void {
    const p = t.rig.root.position;
    const turn = t.rig.root.rotation.y;
    // The box around a turned car: half its width and length, mixed by how far it has turned.
    const c = Math.abs(Math.cos(turn));
    const s = Math.abs(Math.sin(turn));
    const halfWidth = t.rig.width / 2 + 0.05;
    const halfLength = t.rig.length / 2;
    const extentX = c * halfWidth + s * halfLength;
    const extentZ = s * halfWidth + c * halfLength;
    t.box.minX = p.x - extentX;
    t.box.maxX = p.x + extentX;
    t.box.minZ = p.z - extentZ;
    t.box.maxZ = p.z + extentZ;
    t.x = p.x;
    t.z = p.z;
  }

  /** A lumpy mound of rubble covering the building's footprint, matching rubbleShape. */
  private makeRubble(b: BuildingTarget): THREE.Mesh {
    const geo = new THREE.PlaneGeometry(b.width, b.depth, Math.round(b.width / 1.5), Math.round(b.depth / 1.5)).rotateX(-Math.PI / 2);
    const pos = geo.getAttribute('position') as THREE.BufferAttribute;
    for (let i = 0; i < pos.count; i++) {
      const shape = rubbleShape(pos.getX(i), pos.getZ(i), b.width, b.depth);
      const lumps = shape > 0 ? (Math.random() - 0.3) * 0.35 : 0;
      pos.setY(i, Math.max(0, RUBBLE_HEIGHT * shape + lumps));
    }
    geo.computeVertexNormals();
    const wall = b.mesh.material as THREE.MeshStandardMaterial;
    const material = new THREE.MeshStandardMaterial({ color: wall.color.clone().multiplyScalar(0.7), roughness: 1, flatShading: true });
    const mesh = new THREE.Mesh(geo, material);
    mesh.position.set(b.x, 0.02, b.z);
    mesh.castShadow = mesh.receiveShadow = true;
    this.group.add(mesh);
    return mesh;
  }

  // ---------- every frame ----------

  update(dt: number, carX: number, carZ: number): void {
    this.clock += dt;
    this.updateBuildings(dt);
    this.updateWrecks(dt);
    this.updateCones(dt, carX, carZ);

    // Bring destroyed things back once they've been gone a while and the player is far away.
    for (const t of this.targets) {
      if (t.alive || (t.kind === 'car' && t.flying)) continue;
      const longAgo = this.clock - t.destroyedAt > COMES_BACK_AFTER;
      const farAway = Math.hypot(carX - t.x, carZ - t.z) > COMES_BACK_DISTANCE;
      if (longAgo && farAway) this.restore(t, true);
    }

    // Keep the sun's shadow box centred on the car.
    this.sun.position.set(carX + 40, 80, carZ + 25);
    this.sun.target.position.set(carX, 0, carZ);
  }

  private updateBuildings(dt: number): void {
    for (const b of this.buildings) {
      if (b.state === 'falling') {
        // Sink into the ground, faster and faster, shaking and leaning, while the rubble grows.
        b.timer += dt;
        const progress = Math.min(1, b.timer / FALL_TIME);
        const sunk = progress * progress;
        const shake = 0.3 * (1 - progress);
        b.mesh.position.set(b.x + (Math.random() - 0.5) * shake, b.height / 2 - b.height * sunk, b.z + (Math.random() - 0.5) * shake);
        b.mesh.rotation.set(b.lean.x * progress, 0, b.lean.y * progress);
        b.rubbleHeight = RUBBLE_HEIGHT * progress;
        b.rubble!.scale.y = Math.max(0.001, progress);
        if (Math.random() < dt * 30) this.effects.dust(b.x, b.z, b.width / 2, b.depth / 2, 4, 1.3);
        if (progress >= 1) {
          b.state = 'rubble';
          b.mesh.visible = false;
        }
      } else if (b.state === 'rising') {
        // Rise back out of the ground, slowing down at the end, while the rubble shrinks.
        b.timer += dt;
        const progress = Math.min(1, b.timer / RISE_TIME);
        const risen = 1 - (1 - progress) ** 3;
        b.mesh.position.set(b.x, b.height / 2 - b.height * (1 - risen), b.z);
        b.rubbleHeight = RUBBLE_HEIGHT * (1 - progress);
        if (b.rubble) b.rubble.scale.y = Math.max(0.001, 1 - progress);
        if (progress >= 1) {
          b.state = 'standing';
          b.rubbleHeight = 0;
          if (b.rubble) b.rubble.visible = false;
        }
      } else if (b.alive && b.hp < b.maxHp * 0.6) {
        // Badly damaged buildings catch fire on the roof.
        this.effects.burn(new THREE.Vector3(b.x, b.height, b.z), 1.2 * (1 - b.hp / b.maxHp), dt);
      }
    }
  }

  private updateWrecks(dt: number): void {
    for (const t of this.targets) {
      if (t.kind !== 'car' || t.alive) continue;
      const root = t.rig.root;
      if (t.flying) {
        t.velocity.y -= 16 * dt; // gravity
        root.position.addScaledVector(t.velocity, dt);
        root.rotation.x += t.spin.x * dt;
        root.rotation.y += t.spin.y * dt;
        root.rotation.z += t.spin.z * dt;
        if (root.position.y <= 0 && t.velocity.y < 0) {
          root.position.y = 0;
          if (t.velocity.y < -5) {
            // Landed hard: bounce once.
            t.velocity.set(t.velocity.x * 0.5, -t.velocity.y * 0.3, t.velocity.z * 0.5);
            t.spin.multiplyScalar(0.4);
            this.effects.dust(root.position.x, root.position.z, 1, 2, 6, 0.4);
          } else {
            // Settle on its wheels, inside the city walls, and become solid again.
            t.flying = false;
            root.rotation.set(0, root.rotation.y, 0);
            root.position.x = THREE.MathUtils.clamp(root.position.x, -HALF_SIZE + 3, HALF_SIZE - 3);
            root.position.z = THREE.MathUtils.clamp(root.position.z, -HALF_SIZE + 3, HALF_SIZE - 3);
            this.fitCarBox(t);
            t.box.solid = true;
          }
        }
      }
      if (this.clock < t.burnUntil) this.effects.burn(new THREE.Vector3(root.position.x, root.position.y + 0.8, root.position.z), 0.9, dt);
    }
  }

  private updateCones(dt: number, carX: number, carZ: number): void {
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
  }

  // ---------- graphics settings, day and night ----------

  /** Shadow quality: the shadow map's size, and how many metres around the car get shadows. */
  setShadows(on: boolean, mapSize: number, area: number): void {
    this.sun.castShadow = on;
    if (this.sun.shadow.mapSize.x !== mapSize) {
      // A new size needs a new shadow map; three.js makes it on the next frame.
      this.sun.shadow.mapSize.set(mapSize, mapSize);
      this.sun.shadow.map?.dispose();
      this.sun.shadow.map = null;
    }
    const s = this.sun.shadow.camera;
    s.left = s.bottom = -area;
    s.right = s.top = area;
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

/**
 * The shape of a rubble pile, from 0 at its edge to 1 on top, at a point
 * (dx, dz) from the middle of a width × depth footprint. Flat on top, sloping
 * down over the outer 40% so a car can drive up it.
 */
function rubbleShape(dx: number, dz: number, width: number, depth: number): number {
  const fromMiddle = Math.max(Math.abs(dx) / (width / 2), Math.abs(dz) / (depth / 2)); // 0 in the middle, 1 at the edge
  return Math.max(0, Math.min(1, (1 - fromMiddle) * 2.5));
}
