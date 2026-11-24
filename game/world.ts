// The drivable world: the city, or one of the race track venues (see
// venues.ts for the list, and how each one looks). The city is a grid of streets
// with buildings, parks, a car park and a stunt park. The race tracks
// themselves (road, kerbs, banking, barriers) come from raceTrack.ts; this file
// puts them on the map and dresses them up with grandstands, a paddock, trees,
// rocks, buildings, floodlights and weather.
//
// Buildings, parked cars, street lamps and trees can be destroyed ("targets")
// on every map. Buildings sink into a pile of rubble you can drive over;
// parked cars blow up into burning wrecks; lamps and trees fall over.
// Everything comes back a while after the player has driven away.
import * as THREE from 'three';
import { buildCar, type CarRig } from './carModel';
import { DEFAULT_CONFIG, randomConfig } from './carConfig';
import type { Effects } from './effects';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import { RaceTrack, drawTrack, makeWall, type TrackStep, type TrackStyle, type Wall } from './raceTrack';
import { concreteTexture, groundTexture, roadTexture, windowTexture } from './textures';
import { OVAL_RADIUS, trackLayout, venue, type MapName, type Theme, type Venue } from './venues';
import { canvasTexture, gradientTexture, radialGlowTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';

/** A time trial lap: arches to drive through in order, and where the car waits for the start. */
export interface Course {
  /** The last arch is the finish line. `heading` and `bank` are worked out from the arches either side if missing. */
  checkpoints: { x: number; y: number; z: number; heading?: number; bank?: number }[];
  start: { x: number; z: number; heading: number };
  /** Radius of each arch; driving within it counts. */
  radius: number;
}

/** The city's lap, through glowing arches round the streets. The last arch is the finish line. */
const CITY_COURSE: Course = {
  checkpoints: [
    { x: 0, z: 120 },
    { x: 40, z: 160 },
    { x: 120, z: 160 },
    { x: 160, z: 100 },
    { x: 160, z: 20 },
    { x: 120, z: -80 },
    { x: 40, z: -80 },
    { x: 0, z: -40 },
    { x: 0, z: 20 },
  ].map((c) => ({ ...c, y: 0 })),
  // 15 m before the finish line, in the right-hand lane.
  start: { x: -3, z: 5, heading: 0 },
  radius: 7.5,
};

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
  /** A rival car (or anything else in `hitBoxes`) that was hit. */
  hitBox: HitBox | null;
}

/**
 * Something weapons can hit besides the world's own targets: the rival cars
 * in a battle. A box standing on (x, y, z), `half` metres out to each side and
 * `height` tall. The event moves it with the car every frame.
 */
export interface HitBox {
  x: number;
  y: number;
  z: number;
  half: number;
  height: number;
  /** Whether it can be hit now (a wrecked car can't). */
  alive: boolean;
  onHit(damage: number, direction: THREE.Vector3): void;
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

export const HALF_SIZE = 260; // the city spans -260..260 on x and z
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
const CELL = 16; // size of the lookup grid's cells for posts and walls, metres
const WEATHER_DROPS = 2500; // raindrops or snowflakes, in a box that follows the car
const WEATHER_BOX = 70; // metres across...
const WEATHER_HEIGHT = 30; // ...and high

export class World {
  readonly group = new THREE.Group();
  readonly boxes: Box[] = [];
  readonly circles: Circle[] = [];
  /** Straight walls at any angle: race track barriers and grandstands. */
  readonly walls: Wall[] = [];
  readonly targets: Target[] = [];
  readonly blocks: Block[] = [];
  readonly ramps: Ramp[] = [];
  /** Rival cars that weapons can hit, set by a battle event. */
  hitBoxes: HitBox[] = [];
  map: MapName = 'city';
  /** Which track at this venue: 0 is the main one, others are generated (see venues.ts). */
  layout = 0;
  venue: Venue = venue('city');
  /** The map spans -half..half on x and z, with a wall all the way round. */
  half = HALF_SIZE;
  /** The race track, on the circuit and speedway maps. */
  track: RaceTrack | null = null;
  spawn = { x: -3, z: -20, heading: 0 };
  course: Course = CITY_COURSE;
  /** Called whenever something is destroyed (for the score and sounds). */
  onDestroyed: (target: Target) => void = () => {};
  /** Called after a new map (or track) has been built. */
  onLoaded: () => void = () => {};
  private buildings: BuildingTarget[] = [];
  private cones: Cone[] = [];
  private parkedCars: CarRig[] = [];
  /** Paved areas off the track (the paddock), where the car isn't slowed by grass. */
  private pavedAreas: { minX: number; maxX: number; minZ: number; maxZ: number }[] = [];
  /** The five start lights over the start line. */
  private startLights: THREE.MeshStandardMaterial[] = [];
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
  /**
   * A lookup grid of the posts (lamps, trees, rocks) and walls in each 16 m
   * square, so a car only checks the few near it. The forest has thousands of
   * trees, and there can be a dozen cars.
   */
  private cells = new Map<number, { circles: Circle[]; walls: Wall[] }>();
  /** Falling rain or snow, if the venue has weather. */
  private weather: THREE.LineSegments | THREE.Points | null = null;
  private weatherSpeed = 0;

  constructor(
    private scene: THREE.Scene,
    private effects: Effects,
    map: MapName = 'city',
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
    this.load(map);
  }

  // ---------- building the world ----------

  /** The look of this venue: sky, ground, trees, weather, grip. */
  get theme(): Theme {
    return this.venue.theme;
  }

  /** Whether it's dark: the player switched to night, or the venue is always at night. */
  get isNight(): boolean {
    return this.night || this.theme.night;
  }

  /**
   * Throws away the current map and builds another one. `layout` picks the
   * track at a race venue: 0 is its main track, and every other number is a
   * different generated track.
   */
  load(map: MapName, layout = 0): void {
    this.clear();
    this.map = map;
    this.layout = map === 'city' ? 0 : layout;
    this.venue = venue(map);
    this.rand = seededRandom(42 + this.layout); // the same map every time
    this.dayBg.dispose();
    this.dayBg = gradientTexture(...this.theme.sky);
    if (map === 'city') this.buildCity();
    else if (map === 'circuit' && this.layout === 0) this.buildCircuit();
    else if (map === 'oval' && this.layout === 0) this.buildOval();
    else this.buildVenueTrack();
    this.buildEdges();
    this.buildWeather();
    this.buildCells();
    this.setNight(this.night);
    this.onLoaded();
  }

  /** Fills the lookup grid. Posts and walls never move, so this is done once per map. */
  private buildCells(): void {
    this.cells.clear();
    const add = (minX: number, maxX: number, minZ: number, maxZ: number, put: (cell: { circles: Circle[]; walls: Wall[] }) => void) => {
      for (let cx = Math.floor(minX / CELL); cx <= Math.floor(maxX / CELL); cx++) {
        for (let cz = Math.floor(minZ / CELL); cz <= Math.floor(maxZ / CELL); cz++) {
          const key = cellKey(cx, cz);
          let cell = this.cells.get(key);
          if (!cell) this.cells.set(key, (cell = { circles: [], walls: [] }));
          put(cell);
        }
      }
    };
    for (const c of this.circles) add(c.x - c.r, c.x + c.r, c.z - c.r, c.z + c.r, (cell) => cell.circles.push(c));
    for (const w of this.walls) add(w.minX, w.maxX, w.minZ, w.maxZ, (cell) => cell.walls.push(w));
  }

  /**
   * The posts and walls that might be within `reach` metres of (x, z). The
   * same one can be listed more than once.
   */
  nearby(x: number, z: number, reach: number): { circles: Circle[]; walls: Wall[] } {
    const circles: Circle[] = [];
    const walls: Wall[] = [];
    for (let cx = Math.floor((x - reach) / CELL); cx <= Math.floor((x + reach) / CELL); cx++) {
      for (let cz = Math.floor((z - reach) / CELL); cz <= Math.floor((z + reach) / CELL); cz++) {
        const cell = this.cells.get(cellKey(cx, cz));
        if (!cell) continue;
        circles.push(...cell.circles);
        walls.push(...cell.walls);
      }
    }
    return { circles, walls };
  }

  /** Removes everything on the map and frees its memory. */
  private clear(): void {
    for (const car of this.parkedCars) car.dispose();
    // The building and lamp materials are shared by every map, so they're kept.
    const keep = new Set<THREE.Material>([...this.buildingMats, this.lampHeadMat]);
    this.group.traverse((o) => {
      if (!(o instanceof THREE.Mesh)) return;
      // Instanced meshes (lamps, trees, fans, fence posts) also hold a buffer of positions and colours.
      if (o instanceof THREE.InstancedMesh) o.dispose();
      o.geometry.dispose();
      for (const m of [o.material].flat() as THREE.Material[]) {
        if (keep.has(m)) continue;
        (m as THREE.MeshStandardMaterial).map?.dispose();
        m.dispose();
      }
    });
    this.group.clear();
    if (this.weather) {
      this.weather.geometry.dispose();
      (this.weather.material as THREE.Material).dispose();
      this.scene.remove(this.weather);
      this.weather = null;
    }
    const lists: unknown[][] = [this.boxes, this.circles, this.walls, this.targets, this.blocks, this.ramps, this.buildings, this.cones, this.parkedCars, this.pavedAreas, this.startLights];
    for (const list of lists) list.length = 0;
    this.track = null;
  }

  private buildCity(): void {
    this.half = HALF_SIZE;
    this.spawn = { x: -3, z: -20, heading: 0 };
    this.course = CITY_COURSE;
    this.buildGround();
    this.buildRoads();
    this.buildBlocks();
  }

  private buildGround(): void {
    const size = this.half * 2 + 400;
    const tex = groundTexture(this.map === 'city' ? 'grass' : this.theme.ground);
    tex.repeat.set(size / 6.5, size / 6.5);
    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(size, size),
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
    const poles: THREE.Vector3[] = [];
    // Lamps sit 20 m either side of each block centre, well clear of the crossings.
    const along = ROAD_LINES.slice(0, -1).flatMap((c) => [c + 20, c + 60]);
    for (const c of ROAD_LINES) {
      for (const p of along) {
        poles.push(new THREE.Vector3(c + ROAD_WIDTH / 2 + 1, 0, p));
        poles.push(new THREE.Vector3(p, 0, c - ROAD_WIDTH / 2 - 1));
      }
    }
    this.addLamps(poles);
  }

  /** Lamp posts at each point, `size` times as tall as a street lamp (7 m). */
  private addLamps(points: THREE.Vector3[], size = 1): void {
    const poleGeo = new THREE.CylinderGeometry(0.12, 0.16, 7, 8).translate(0, 3.5, 0);
    const headGeo = new THREE.BoxGeometry(0.7, 0.25, 0.7).translate(0, 7.1, 0);
    const poleMesh = new THREE.InstancedMesh(poleGeo, new THREE.MeshStandardMaterial({ color: 0x3a3d42, metalness: 0.6, roughness: 0.4 }), points.length);
    const headMesh = new THREE.InstancedMesh(headGeo, this.lampHeadMat, points.length);
    points.forEach((p, i) => {
      const m = new THREE.Matrix4().compose(p, new THREE.Quaternion(), new THREE.Vector3(size, size, size));
      poleMesh.setMatrixAt(i, m);
      headMesh.setMatrixAt(i, m);
      // Each lamp can be shot or knocked down (3 bullets).
      const circle: Circle = { x: p.x, z: p.z, r: 0.25 * size, height: 7.3 * size, hitRadius: 0.3 * size, solid: true };
      const lamp: PropTarget = {
        kind: 'lamp', x: p.x, z: p.z, hp: 25, maxHp: 25, alive: true, destroyedAt: 0, value: Math.round(1500 * size),
        circle, meshes: [poleMesh, headMesh], index: i, matrix: m, size,
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

  /**
   * Trees in the venue's style: round leafy trees, pines, snowy pines or
   * cactuses. Each one is a trunk and a top, drawn many at once.
   */
  private addTrees(points: THREE.Vector3[]): void {
    const kind = this.map === 'city' ? 'round' : this.theme.trees;
    let trunkGeo: THREE.BufferGeometry = new THREE.CylinderGeometry(0.22, 0.3, 2.4, 7).translate(0, 1.2, 0);
    let leafGeo: THREE.BufferGeometry = new THREE.IcosahedronGeometry(1.8, 0).translate(0, 3.6, 0);
    let trunkColor = 0x6b4a2f;
    // The top's colour: hue, saturation and lightness, each with a random spread.
    let leafHsl = (r: () => number) => [0.25 + r() * 0.08, 0.5, 0.28 + r() * 0.12];
    if (kind === 'pine' || kind === 'snowPine') {
      // Two stacked cones on a short trunk.
      leafGeo = mergeGeometries([new THREE.ConeGeometry(1.9, 3.6, 7).translate(0, 3.2, 0), new THREE.ConeGeometry(1.3, 2.8, 7).translate(0, 5.2, 0)]);
      leafHsl = kind === 'pine' ? (r) => [0.3 + r() * 0.05, 0.45, 0.18 + r() * 0.08] : (r) => [0.55, 0.2 + r() * 0.15, 0.82 + r() * 0.1];
    } else if (kind === 'cactus') {
      // A tall green column, with two arms as its "top".
      trunkGeo = new THREE.CylinderGeometry(0.32, 0.38, 4.2, 8).translate(0, 2.1, 0);
      trunkColor = 0x4f7a3a;
      leafGeo = mergeGeometries([
        new THREE.CylinderGeometry(0.22, 0.22, 1, 7).rotateZ(Math.PI / 2).translate(0.75, 2, 0),
        new THREE.CylinderGeometry(0.22, 0.22, 1.4, 7).translate(1.2, 2.6, 0),
        new THREE.CylinderGeometry(0.2, 0.2, 0.8, 7).rotateZ(Math.PI / 2).translate(-0.6, 2.8, 0),
        new THREE.CylinderGeometry(0.2, 0.2, 1.1, 7).translate(-0.95, 3.3, 0),
      ]);
      leafHsl = (r) => [0.27 + r() * 0.04, 0.4, 0.3 + r() * 0.06];
    }
    const trunks = new THREE.InstancedMesh(trunkGeo, new THREE.MeshStandardMaterial({ color: trunkColor, roughness: 1 }), points.length);
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
      const [h, sat, l] = leafHsl(this.rand);
      leaves.setColorAt(i, color.setHSL(h, sat, l));
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
        if (this.rand() < 0.55) this.addParkedCar(x, z, row > 0 ? Math.PI : 0);
      }
    }
  }

  /** A random parked car that can be rammed or blown up. */
  private addParkedCar(x: number, z: number, heading: number): void {
    const car = buildCar(randomConfig(DEFAULT_CONFIG), false, false);
    car.root.position.set(x, 0, z);
    car.root.rotation.y = heading;
    this.group.add(car.root);
    this.parkedCars.push(car);
    const box: Box = { minX: x, maxX: x, minZ: z, maxZ: z, height: 1.4, solid: true };
    const parked: CarTarget = {
      kind: 'car', x, z, hp: 70, maxHp: 70, alive: true, destroyedAt: 0, value: 12000,
      box, rig: car, homeX: x, homeZ: z, homeHeading: heading,
      velocity: new THREE.Vector3(), spin: new THREE.Vector3(), flying: false, burnUntil: 0,
    };
    this.fitCarBox(parked);
    box.target = parked;
    this.boxes.push(box);
    this.targets.push(parked);
  }

  /**
   * Adds a car that drives around the city (see traffic.ts). It can be rammed
   * and blown up like a parked car, and comes back at its home spot.
   */
  addTrafficCar(x: number, z: number, heading: number): CarTarget {
    this.addParkedCar(x, z, heading);
    const t = this.targets[this.targets.length - 1] as CarTarget;
    t.value = 15000;
    return t;
  }

  /** Moves a car (and its collision box) to a new spot. */
  moveCar(t: CarTarget, x: number, z: number, heading: number): void {
    t.rig.root.position.set(x, 0, z);
    t.rig.root.rotation.set(0, heading, 0);
    this.fitCarBox(t);
  }

  /** Takes a traffic car out of the world. */
  removeCar(t: CarTarget): void {
    const drop = <T>(list: T[], item: T) => {
      const i = list.indexOf(item);
      if (i >= 0) list.splice(i, 1);
    };
    drop(this.targets, t);
    drop(this.boxes, t.box);
    drop(this.parkedCars, t.rig);
    t.rig.root.removeFromParent();
    t.rig.dispose();
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
    for (const ramp of ramps) this.addRamp(ramp, rampMat, stripeMat);

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

  // ---------- the race tracks ----------

  /** Puts a race track on the map, centred, and sets the start, the lap and the map's size. */
  private addTrack(steps: TrackStep[], style: TrackStyle): RaceTrack {
    const path = drawTrack(steps);
    // Centre the track on the middle of the map.
    const xs = path.map((p) => p.x);
    const zs = path.map((p) => p.z);
    const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
    const cz = (Math.min(...zs) + Math.max(...zs)) / 2;
    const track = new RaceTrack(path.map((p) => ({ x: p.x - cx, z: p.z - cz })), style);
    this.track = track;
    const reach = Math.max(...track.points.map((p) => Math.max(Math.abs(p.x), Math.abs(p.z))));
    this.half = Math.ceil(reach + 110); // room round the outside for grandstands and trees
    this.buildGround();
    this.group.add(track.buildMeshes());
    this.walls.push(...track.walls());
    this.addGantry(track);

    // The lap: arches spaced round the track, the last one on the start line.
    const count = Math.max(6, Math.round(track.length / 320));
    const checkpoints = [];
    for (let k = 1; k <= count; k++) {
      const p = track.pose((track.length * k) / count);
      checkpoints.push({ x: p.x, y: p.y, z: p.z, heading: p.heading, bank: p.bank });
    }
    const start = track.pose(-20); // on the grid, 20 m behind the line
    this.course = { checkpoints, start: { x: start.x, z: start.z, heading: start.heading }, radius: style.width / 2 + 1.5 };
    this.spawn = { ...this.course.start };
    return track;
  }

  private buildCircuit(): void {
    const track = this.addTrack(trackLayout(this.venue, 0), this.venue.style!);
    const start = track.pose(0); // the main straight runs along +x through here
    const wall = track.leftWall!; // both barriers are this far from the middle of the road

    // Grandstands along the outside of the main straight, either side of the
    // start gantry, and beside the top straight and the west straight.
    this.addTrackGrandstand(track, -105, 160, 14, '#d7263d');
    this.addTrackGrandstand(track, 115, 170, 14, '#2b6fd6');
    this.addTrackGrandstand(track, 760, 100, 10, '#f2c94c');
    this.addTrackGrandstand(track, 1541, 110, 10, '#39a845');

    // The paddock inside the main straight: pit building, control tower,
    // team buildings and the teams' cars, on a concrete apron.
    const pz = start.z - wall - 8; // just inside the barrier
    this.addPaddock(start.x - 175, start.x + 190, pz - 110, pz);
    this.addBuilding(start.x + 10, pz - 9, 190, 14, 8); // pit garages
    this.addBuilding(start.x - 120, pz - 12, 16, 18, 30); // control tower
    for (let i = 0; i < 5; i++) this.addBuilding(start.x - 100 + i * 52, pz - 62, 30, 18, 10 + this.rand() * 8); // team hospitality
    for (let i = 0; i < 22; i++) this.addParkedCar(start.x - 150 + i * 14, pz - 34, Math.PI / 2);
    for (let i = 0; i < 22; i++) if (this.rand() < 0.7) this.addParkedCar(start.x - 150 + i * 14, pz - 90, -Math.PI / 2);

    // Floodlights round the outside of the track.
    const lights: THREE.Vector3[] = [];
    for (let along = 60; along < track.length; along += 140) {
      const p = track.pose(along, -(wall + 5));
      if (this.isClear(p.x, p.z, 3)) lights.push(new THREE.Vector3(p.x, 0, p.z));
    }
    this.addLamps(lights, 2.2);
    this.scatterTrees(420);
  }

  private buildOval(): void {
    const style = this.venue.style!;
    const track = this.addTrack(trackLayout(this.venue, 0), style);
    const start = track.pose(0); // the front straight runs along +x through here
    const outside = track.rightWall! + 0.6; // back of the outside wall

    // A huge grandstand along the front straight, in three sections.
    for (const [along, color] of [[-215, '#2b6fd6'], [0, '#d7263d'], [215, '#2b6fd6']] as const) this.addTrackGrandstand(track, along, 200, 22, color);

    // The infield: no wall on the inside, so drive straight off the apron
    // into the paddock, the stunt ramps and the trees.
    const infield = start.z - OVAL_RADIUS; // middle of the oval
    const pz = start.z - style.width / 2 - style.apron - 14;
    this.addPaddock(start.x - 260, start.x + 260, pz - 90, pz);
    this.addBuilding(start.x, pz - 9, 300, 14, 9); // pit garages
    this.addBuilding(start.x - 200, pz - 14, 18, 18, 34); // control tower
    for (let i = 0; i < 30; i++) this.addParkedCar(start.x - 230 + i * 16, pz - 34, Math.PI / 2);
    for (let i = 0; i < 6; i++) this.addBuilding(start.x - 200 + i * 80, pz - 70, 40, 18, 8 + this.rand() * 10);

    // Stunt ramps in the middle of the infield, lined up for big jumps.
    const rampMat = new THREE.MeshStandardMaterial({ color: 0xe0a03a, roughness: 0.7 });
    const stripeMat = new THREE.MeshStandardMaterial({ color: 0x222222, roughness: 0.7 });
    for (const [dx, dz, angle, height] of [
      [-250, 10, Math.PI / 2, 3],
      [0, 10, Math.PI / 2, 5],
      [250, 10, Math.PI / 2, 3],
      [-120, 70, -Math.PI / 2, 4],
      [120, 70, -Math.PI / 2, 4],
    ]) {
      this.addRamp({ x: start.x + dx, z: infield + dz, angle, width: 10, length: height * 6, height }, rampMat, stripeMat);
    }

    // Tall floodlights behind the outside wall.
    const lights: THREE.Vector3[] = [];
    for (let along = 0; along < track.length; along += 150) {
      const p = track.pose(along, -(outside + 6));
      if (this.isClear(p.x, p.z, 3)) lights.push(new THREE.Vector3(p.x, 0, p.z));
    }
    this.addLamps(lights, 3);
    this.scatterTrees(380);
  }

  /**
   * Any track that isn't the hand-made circuit or speedway: a generated
   * layout, dressed up in the venue's style. Grandstands and pits go along the
   * main straight, then the scenery: rocks in the canyon, buildings in the
   * neon city, a thick forest for the rally, and trees everywhere else.
   */
  private buildVenueTrack(): void {
    const v = this.venue;
    const style = v.style!;
    const track = this.addTrack(trackLayout(v, this.layout), style);
    const scenery = this.theme.scenery;
    // How far the main straight runs each way from the start line, staying
    // clear of the corners at each end.
    const straight = (direction: number) => {
      let d = 0;
      while (d < 400 && Math.abs(track.pointAt(direction * d).curve) < 1 / 400) d += 4;
      return Math.max(0, d - 15);
    };
    const ahead = straight(1);
    const behind = straight(-1);
    const out = -track.inside; // the outside of the track: +1 is the left
    const wall = Math.max(track.leftWall ?? style.width / 2, track.rightWall ?? style.width / 2);
    const colors = ['#d7263d', '#2b6fd6', '#f2c94c', '#39a845'];

    if (scenery !== 'woods') {
      // Grandstands on the outside of the main straight, either side of the gantry.
      const room = (from: number, to: number) => to - from > 50;
      if (room(-behind, -12)) this.addTrackGrandstand(track, (-behind - 12) / 2, Math.min(160, behind - 16), 12, colors[this.layout % 4]);
      if (room(12, ahead)) this.addTrackGrandstand(track, (ahead + 12) / 2, Math.min(160, ahead - 16), 12, colors[(this.layout + 1) % 4]);
      // The pits on the inside: garages, and the teams' cars, if there's room.
      const pitLength = Math.min(180, ahead + behind - 20);
      const pitAlong = (ahead - behind) / 2;
      // Every track starts heading along +x, so the main straight runs along x.
      const pit = track.pose(pitAlong, track.inside * (wall + 9));
      if (pitLength > 60 && this.isAreaClear(pit.x - pitLength / 2, pit.x + pitLength / 2, pit.z - 6, pit.z + 6, 1)) {
        this.addBuilding(pit.x, pit.z, pitLength, 12, 8);
        for (let i = 0; i < pitLength / 14; i++) {
          const car = track.pose(pitAlong - pitLength / 2 + 7 + i * 14, track.inside * (wall + 24));
          if (this.isClear(car.x, car.z, 2.5)) this.addParkedCar(car.x, car.z, car.heading + Math.PI / 2);
        }
      }
    }

    if (scenery === 'mesas') this.addRocks(26);
    if (scenery === 'city') this.addCityBlocks(110);
    // Floodlights (or street lamps in the city) along the outside of the track.
    const lightGap = scenery === 'city' ? 45 : 130;
    const lights: THREE.Vector3[] = [];
    for (let along = 30; along < track.length; along += lightGap) {
      const p = track.pose(along, out * (wall + 4));
      if (this.isClear(p.x, p.z, 2)) lights.push(new THREE.Vector3(p.x, 0, p.z));
    }
    this.addLamps(lights, scenery === 'city' ? 1.3 : track.length < 1500 ? 1.2 : 2.2);
    // A forest right up to the edge of the rally stage; trees dotted about elsewhere.
    const area = (this.half * 2) ** 2;
    this.scatterTrees(scenery === 'woods' ? Math.round(area / 260) : Math.round(area / (scenery === 'mesas' ? 3000 : 1300)), scenery === 'woods' ? 3 : 4);
  }

  /** Big flat-topped red rocks, like the mesas of a desert canyon. They are solid. */
  private addRocks(count: number): void {
    const rocks: { x: number; z: number; r: number; h: number }[] = [];
    for (let tries = 0; rocks.length < count && tries < count * 40; tries++) {
      const r = 6 + this.rand() * 16;
      const x = (this.rand() * 2 - 1) * (this.half - r - 4);
      const z = (this.rand() * 2 - 1) * (this.half - r - 4);
      if (!this.isClear(x, z, r + 8) || rocks.some((o) => Math.hypot(o.x - x, o.z - z) < o.r + r + 6)) continue;
      rocks.push({ x, z, r, h: 8 + this.rand() * 34 });
    }
    const geo = new THREE.CylinderGeometry(1, 1.25, 1, 7).translate(0, 0.5, 0);
    const mesh = new THREE.InstancedMesh(geo, new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 1, flatShading: true }), rocks.length);
    const color = new THREE.Color();
    rocks.forEach((o, i) => {
      mesh.setMatrixAt(i, new THREE.Matrix4().compose(new THREE.Vector3(o.x, 0, o.z), new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), this.rand() * 6), new THREE.Vector3(o.r, o.h, o.r)));
      mesh.setColorAt(i, color.setHSL(0.04 + this.rand() * 0.03, 0.55, 0.38 + this.rand() * 0.12));
      // The widest part is at the bottom: 1.25 × the radius.
      this.circles.push({ x: o.x, z: o.z, r: o.r * 1.2, height: o.h, hitRadius: o.r * 1.2, solid: true });
    });
    mesh.castShadow = mesh.receiveShadow = true;
    this.group.add(mesh);
  }

  /** Office blocks packed round a street circuit, wherever there's room. They can be destroyed, like the city's. */
  private addCityBlocks(count: number): void {
    let placed = 0;
    for (let tries = 0; placed < count && tries < count * 30; tries++) {
      const w = 14 + this.rand() * 16;
      const d = 14 + this.rand() * 16;
      const x = (this.rand() * 2 - 1) * (this.half - 20);
      const z = (this.rand() * 2 - 1) * (this.half - 20);
      if (!this.isClear(x, z, Math.hypot(w, d) / 2 + 5)) continue;
      this.addBuilding(x, z, w, d, 12 + Math.pow(this.rand(), 1.5) * 70);
      placed++;
    }
  }

  /** A concrete area the car can drive on at full speed (grass slows it down). */
  private addPaddock(minX: number, maxX: number, minZ: number, maxZ: number): void {
    const tex = concreteTexture();
    tex.repeat.set((maxX - minX) / 4, (maxZ - minZ) / 4);
    const slab = new THREE.Mesh(new THREE.PlaneGeometry(maxX - minX, maxZ - minZ), new THREE.MeshStandardMaterial({ map: tex, roughness: 0.95 }));
    slab.rotation.x = -Math.PI / 2;
    slab.position.set((minX + maxX) / 2, 0.015, (minZ + maxZ) / 2);
    slab.receiveShadow = true;
    this.group.add(slab);
    this.pavedAreas.push({ minX, maxX, minZ, maxZ });
  }

  /** Whether a spot is clear of the track, its barriers, buildings and grandstands, by `margin` metres. */
  private isClear(x: number, z: number, margin: number): boolean {
    if (Math.abs(x) > this.half - margin || Math.abs(z) > this.half - margin) return false;
    if (this.track) {
      const wall = Math.max(this.track.leftWall ?? 0, this.track.rightWall ?? 0, this.track.style.width / 2 + this.track.style.apron);
      if (this.track.distanceFrom(x, z) < wall + margin) return false;
    }
    if (this.boxes.some((b) => x > b.minX - margin && x < b.maxX + margin && z > b.minZ - margin && z < b.maxZ + margin)) return false;
    if (this.pavedAreas.some((a) => x > a.minX - margin && x < a.maxX + margin && z > a.minZ - margin && z < a.maxZ + margin)) return false;
    if (this.ramps.some((r) => Math.hypot(x - r.x, z - r.z) < r.length / 2 + r.width + margin)) return false;
    return !this.walls.some((w) => {
      const sx = w.bx - w.ax;
      const sz = w.bz - w.az;
      const t = Math.max(0, Math.min(1, ((x - w.ax) * sx + (z - w.az) * sz) / (sx * sx + sz * sz || 1)));
      return Math.hypot(x - (w.ax + sx * t), z - (w.az + sz * t)) < margin;
    });
  }

  /** Whether a whole rectangle is clear (checked every 8 m), by `margin` metres. */
  private isAreaClear(minX: number, maxX: number, minZ: number, maxZ: number, margin: number): boolean {
    for (let x = minX; x <= maxX + 0.1; x += Math.max(1, (maxX - minX) / Math.ceil((maxX - minX) / 8))) {
      for (let z = minZ; z <= maxZ + 0.1; z += Math.max(1, (maxZ - minZ) / Math.ceil((maxZ - minZ) / 8))) {
        if (!this.isClear(x, z, margin)) return false;
      }
    }
    return true;
  }

  /** Trees dotted around the map wherever there's room, at least `margin` metres from anything. */
  private scatterTrees(count: number, margin = 4): void {
    const trees: THREE.Vector3[] = [];
    for (let tries = 0; trees.length < count && tries < count * 20; tries++) {
      const x = (this.rand() * 2 - 1) * (this.half - 4);
      const z = (this.rand() * 2 - 1) * (this.half - 4);
      if (this.isClear(x, z, margin)) trees.push(new THREE.Vector3(x, 0, z));
    }
    this.addTrees(trees);
  }

  /** A grandstand beside a straight bit of track, `along` metres from the start line, on the outside of the track, facing it. */
  private addTrackGrandstand(track: RaceTrack, along: number, length: number, rows: number, roofColor: string): void {
    const out = -track.inside; // which side is the outside: +1 left, -1 right
    const wall = (out > 0 ? track.leftWall : track.rightWall) ?? track.style.width / 2;
    const p = track.pose(along, out * (wall + 3.5));
    // Away from the track is `out` times the road's left, which is (cos h, -sin h).
    const facing = Math.atan2(out * Math.cos(p.heading), -out * Math.sin(p.heading));
    this.addGrandstand(p.x, p.z, facing, length, rows, roofColor);
  }

  /**
   * A grandstand full of fans. (x, z) is the middle of its front edge, and
   * `facing` is the direction from the front to the back (0 = +z), so the
   * fans look the other way, at the track.
   */
  private addGrandstand(x: number, z: number, facing: number, length: number, rows: number, roofColor: string): void {
    const stand = new THREE.Group();
    stand.position.set(x, 0, z);
    stand.rotation.y = facing;
    const step = 1; // depth of each row
    const rise = 0.55; // height of each row
    const depth = rows * step;
    const top = rows * rise;

    // The stepped seating, drawn side-on and stretched along its length.
    const shape = new THREE.Shape();
    shape.moveTo(0, 0);
    for (let r = 0; r < rows; r++) {
      shape.lineTo(r * step, (r + 1) * rise);
      shape.lineTo((r + 1) * step, (r + 1) * rise);
    }
    shape.lineTo(depth, 0);
    shape.closePath();
    const seatsGeo = new THREE.ExtrudeGeometry(shape, { depth: length, bevelEnabled: false }).translate(0, 0, -length / 2).rotateY(-Math.PI / 2);
    const concrete = new THREE.MeshStandardMaterial({ color: 0xb9bcc0, roughness: 0.9 });
    const seats = new THREE.Mesh(seatsGeo, concrete);
    seats.castShadow = seats.receiveShadow = true;
    stand.add(seats);

    // Back wall and a roof on posts over the top rows.
    const back = new THREE.Mesh(new THREE.BoxGeometry(length, top + 5, 0.4), concrete);
    back.position.set(0, (top + 5) / 2, depth + 0.2);
    const roof = new THREE.Mesh(new THREE.BoxGeometry(length + 2, 0.4, depth * 0.75), new THREE.MeshStandardMaterial({ color: roofColor, roughness: 0.6 }));
    roof.position.set(0, top + 5, depth - (depth * 0.75) / 2 + 0.4);
    roof.castShadow = true;
    stand.add(back, roof);
    const postGeo = new THREE.BoxGeometry(0.3, 5, 0.3);
    for (let px = -length / 2; px <= length / 2; px += 20) {
      const post = new THREE.Mesh(postGeo, concrete);
      post.position.set(px, top + 2.5, depth * 0.35);
      stand.add(post);
    }

    // The crowd: a body and a head for each fan, in bright shirts, with gaps.
    const fans: THREE.Matrix4[] = [];
    for (let r = 0; r < rows; r++) {
      for (let px = -length / 2 + 0.5; px < length / 2; px += 0.75) {
        if (this.rand() < 0.15) continue;
        fans.push(new THREE.Matrix4().makeTranslation(px + (this.rand() - 0.5) * 0.2, (r + 1) * rise, r * step + 0.45));
      }
    }
    const bodies = new THREE.InstancedMesh(new THREE.BoxGeometry(0.46, 0.7, 0.3).translate(0, 0.35, 0), new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.9 }), fans.length);
    const heads = new THREE.InstancedMesh(new THREE.SphereGeometry(0.14, 6, 4).translate(0, 0.86, 0), new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.8 }), fans.length);
    const shirt = new THREE.Color();
    const skin = new THREE.Color();
    fans.forEach((m, i) => {
      bodies.setMatrixAt(i, m);
      heads.setMatrixAt(i, m);
      bodies.setColorAt(i, shirt.setHSL(this.rand(), 0.75, 0.35 + this.rand() * 0.3));
      heads.setColorAt(i, skin.setHSL(0.07, 0.45, 0.25 + this.rand() * 0.5));
    });
    stand.add(bodies, heads);
    this.group.add(stand);

    // Solid all round, so a car that jumps the barrier crashes into it.
    const corner = (lx: number, lz: number) => new THREE.Vector3(lx, 0, lz).applyAxisAngle(new THREE.Vector3(0, 1, 0), facing).add(stand.position);
    const corners = [corner(-length / 2, 0), corner(length / 2, 0), corner(length / 2, depth + 0.4), corner(-length / 2, depth + 0.4)];
    for (let i = 0; i < 4; i++) {
      const a = corners[i];
      const b = corners[(i + 1) % 4];
      this.walls.push(makeWall(a.x, a.z, b.x, b.z, top + 5));
    }
  }

  /** The gantry over the start line, with its banner and the five start lights. */
  private addGantry(track: RaceTrack): void {
    const p = track.pose(0);
    const gantry = new THREE.Group();
    gantry.position.set(p.x, 0, p.z);
    gantry.rotation.y = p.heading; // local +z is the way the cars go, +x is their left
    const s = track.style;
    const left = (track.leftWall ?? s.width / 2 + s.apron) + 1.5;
    const right = -((track.rightWall ?? s.width / 2 + s.apron) + 1.5);
    const metal = new THREE.MeshStandardMaterial({ color: 0x2b2e33, metalness: 0.6, roughness: 0.4 });
    const height = 9 + Math.max(...[left, right].map((o) => track.pose(0, o).y));
    for (const x of [left, right]) {
      const pillar = new THREE.Mesh(new THREE.BoxGeometry(1, height, 1), metal);
      pillar.position.set(x, height / 2, 0);
      pillar.castShadow = true;
      gantry.add(pillar);
      // The pillars are solid.
      const at = track.pose(0, x);
      this.circles.push({ x: at.x, z: at.z, r: 0.7, height, hitRadius: 0.7, solid: true });
    }
    const span = left - right;
    const beam = new THREE.Mesh(new THREE.BoxGeometry(span + 1, 2.4, 1.2), metal);
    beam.position.set((left + right) / 2, height - 1.2, 0);
    beam.castShadow = true;
    gantry.add(beam);
    // The banner faces the cars coming towards the line, and the back says it too.
    const banner = new THREE.MeshStandardMaterial({ map: bannerTexture(this.theme.banner), roughness: 0.6 });
    for (const side of [-1, 1]) {
      const sign = new THREE.Mesh(new THREE.PlaneGeometry(Math.min(span, 40), 2), banner);
      sign.position.set((left + right) / 2, height - 1.2, side * 0.61);
      sign.rotation.y = side < 0 ? Math.PI : 0;
      gantry.add(sign);
    }
    // Five start lights hanging under the beam, facing the grid.
    for (let i = 0; i < 5; i++) {
      const housing = new THREE.Mesh(new THREE.BoxGeometry(0.9, 1.6, 0.5), metal);
      housing.position.set((i - 2) * 1.4, height - 3.3, -0.3);
      const light = new THREE.MeshStandardMaterial({ color: 0x300000, emissive: 0xff0000, emissiveIntensity: 0 });
      this.startLights.push(light);
      for (const y of [0.35, -0.35]) {
        const bulb = new THREE.Mesh(new THREE.CircleGeometry(0.3, 16), light);
        bulb.position.set(0, y, -0.26);
        bulb.rotation.y = Math.PI;
        housing.add(bulb);
      }
      gantry.add(housing);
    }
    this.group.add(gantry);
  }

  /** A jump ramp: a wedge you drive up, with a steep drop off the back. */
  private addRamp(ramp: Ramp, rampMat: THREE.Material, stripeMat: THREE.Material): void {
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

  private buildEdges(): void {
    // Barrier walls around the edge.
    const wallMat = new THREE.MeshStandardMaterial({ color: 0xb8bcc2, roughness: 0.9 });
    const half = this.half;
    const len = half * 2 + 2;
    for (const [x, z, w, d] of [
      [0, half + 1, len, 2],
      [0, -half - 1, len, 2],
      [half + 1, 0, 2, len],
      [-half - 1, 0, 2, len],
    ]) {
      const wall = new THREE.Mesh(new THREE.BoxGeometry(w, 1.4, d), wallMat);
      wall.position.set(x, 0.7, z);
      wall.receiveShadow = true;
      this.group.add(wall);
      this.boxes.push({ minX: x - w / 2, maxX: x + w / 2, minZ: z - d / 2, maxZ: z + d / 2, height: 1.4, solid: true });
    }
    // Hills on the horizon: flat-topped mesas in the canyon, tall snowy
    // peaks in the mountains, rounded green hills everywhere else.
    const scenery = this.map === 'city' ? 'city' : this.theme.scenery;
    const mesas = scenery === 'mesas';
    const peaks = scenery === 'mountains';
    const hillGeo = mesas ? new THREE.CylinderGeometry(0.8, 1, 1, 7) : new THREE.ConeGeometry(1, 1, 7);
    const count = 70;
    const hillMat = new THREE.MeshStandardMaterial({ color: this.map === 'city' ? 0x55734a : this.theme.hills, roughness: 1, flatShading: true });
    const hills = new THREE.InstancedMesh(hillGeo, hillMat, count);
    // Snow-capped peaks get a second, white cone over the top of each one.
    const caps = peaks ? new THREE.InstancedMesh(hillGeo, new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.8, flatShading: true }), count) : null;
    if (peaks) hillMat.color.set(0x6f7a86);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    for (let i = 0; i < count; i++) {
      const a = (i / count) * Math.PI * 2;
      const dist = half + 80 + this.rand() * 80;
      const r = 50 + this.rand() * 60;
      const h = (peaks ? 110 + this.rand() * 150 : mesas ? 25 + this.rand() * 45 : 30 + this.rand() * 60) * (scenery === 'city' && this.map !== 'city' ? 0.6 : 1);
      q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), this.rand() * Math.PI);
      m.compose(new THREE.Vector3(Math.cos(a) * dist, h / 2 - 2, Math.sin(a) * dist), q, new THREE.Vector3(r, h, r));
      hills.setMatrixAt(i, m);
      // The cap is the top 35% of the same cone.
      if (caps) caps.setMatrixAt(i, m.compose(new THREE.Vector3(Math.cos(a) * dist, h - 2 - (h * 0.35) / 2 + 0.3, Math.sin(a) * dist), q, new THREE.Vector3(r * 0.36, h * 0.35, r * 0.36)));
    }
    this.group.add(hills);
    if (caps) this.group.add(caps);
  }

  /** Rain (short falling streaks) or snow (drifting flakes), if the venue has weather. */
  private buildWeather(): void {
    const kind = this.map === 'city' ? 'clear' : this.theme.weather;
    if (kind === 'clear') return;
    const rain = kind === 'rain';
    // Each raindrop is a short line (two points); each snowflake is one point.
    const perDrop = rain ? 2 : 1;
    const positions = new Float32Array(WEATHER_DROPS * perDrop * 3);
    for (let i = 0; i < WEATHER_DROPS; i++) {
      const x = (this.rand() - 0.5) * WEATHER_BOX;
      const y = this.rand() * WEATHER_HEIGHT;
      const z = (this.rand() - 0.5) * WEATHER_BOX;
      for (let k = 0; k < perDrop; k++) positions.set([x, y + k * 0.7, z], (i * perDrop + k) * 3);
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    this.weather = rain
      ? new THREE.LineSegments(geo, new THREE.LineBasicMaterial({ color: 0xaec4d6, transparent: true, opacity: 0.45, depthWrite: false }))
      : new THREE.Points(geo, new THREE.PointsMaterial({ color: 0xffffff, size: 0.28, map: radialGlowTexture(32), transparent: true, opacity: 0.95, depthWrite: false }));
    this.weatherSpeed = rain ? 22 : 2.2;
    this.weather.frustumCulled = false; // it always surrounds the camera
    this.scene.add(this.weather);
  }

  /** Lets the rain or snow fall, keeping it in a box round (x, z) so it always surrounds the car. */
  private updateWeather(dt: number, x: number, z: number): void {
    if (!this.weather) return;
    const attr = this.weather.geometry.getAttribute('position') as THREE.BufferAttribute;
    const p = attr.array as Float32Array;
    const perDrop = this.weather instanceof THREE.LineSegments ? 2 : 1;
    const fall = this.weatherSpeed * dt;
    const sway = perDrop === 1 ? Math.sin(this.clock * 0.7) * 0.6 * dt : 0; // snow drifts in the wind
    const half = WEATHER_BOX / 2;
    for (let i = 0; i < WEATHER_DROPS; i++) {
      const a = i * perDrop * 3; // the drop's first point
      let dy = -fall;
      if (p[a + 1] - fall < 0) dy += WEATHER_HEIGHT; // fell out of the bottom: back to the top
      // Wrapped round the box, so drops left behind reappear ahead of the car.
      const dx = p[a] - x > half ? -WEATHER_BOX : p[a] - x < -half ? WEATHER_BOX : 0;
      const dz = p[a + 2] - z > half ? -WEATHER_BOX : p[a + 2] - z < -half ? WEATHER_BOX : 0;
      for (let k = 0; k < perDrop; k++) {
        p[a + k * 3] += dx + sway;
        p[a + k * 3 + 1] += dy;
        p[a + k * 3 + 2] += dz;
      }
    }
    attr.needsUpdate = true;
  }

  // ---------- driving ----------

  /** Ground height at a point: non-zero on banked track, ramps and rubble piles. */
  heightAt(x: number, z: number): number {
    let best = this.track ? this.track.heightAt(x, z) : 0;
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
   * How much grip the tyres have at (x, z), compared to dry tarmac: less in
   * the rain and snow, and less again on grass.
   */
  gripAt(x: number, z: number): number {
    const weather = this.map === 'city' ? 1 : this.theme.grip;
    return this.offRoad(x, z) ? weather * 0.75 : weather;
  }

  /** Whether (x, z) is grass that slows the car down: anywhere off the tarmac on the race tracks. */
  offRoad(x: number, z: number): boolean {
    if (!this.track || this.track.isPaved(x, z)) return false;
    return !this.pavedAreas.some((a) => x > a.minX && x < a.maxX && z > a.minZ && z < a.maxZ);
  }

  /** Lights up the first `count` of the five start lights (none on the city map). */
  setStartLights(count: number): void {
    this.startLights.forEach((m, i) => (m.emissiveIntensity = i < count ? 1.6 : 0));
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
    const consider = (target: Target | null, cone: Cone | null, ground: boolean, hitBox: HitBox | null = null) => {
      const distance = origin.distanceTo(point);
      if (distance <= maxDistance && (!best || distance < best.distance)) {
        best = { distance, point: point.clone(), target, cone, ground, hitBox };
      }
    };

    // The ground. A race track's road can be raised above it (the oval's
    // banked turns), so there the ray steps along 1 m at a time until it dips
    // below the road, then narrows down where it crossed.
    const track = this.track;
    if (track) {
      const below = (t: number) => {
        point.copy(origin).addScaledVector(direction, t);
        return point.y <= track.heightAt(point.x, point.z);
      };
      for (let t = 1; t < maxDistance + 1; t++) {
        if (!below(t)) continue;
        let lo = t - 1;
        let hi = t;
        for (let k = 0; k < 10; k++) {
          const mid = (lo + hi) / 2;
          if (below(mid)) hi = mid;
          else lo = mid;
        }
        below(hi);
        consider(null, null, true);
        break;
      }
    } else if (ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 1, 0), 0), point)) {
      consider(null, null, true);
    }
    for (const b of this.boxes) {
      if (!b.solid) continue;
      box.min.set(b.minX, 0, b.minZ);
      box.max.set(b.maxX, b.height, b.maxZ);
      if (ray.intersectBox(box, point)) consider(b.target ?? null, null, false);
    }
    for (const w of this.walls) {
      // Where the ray crosses the wall, seen from above: `t` along the ray, `u` along the wall.
      const sx = w.bx - w.ax;
      const sz = w.bz - w.az;
      const denom = direction.x * sz - direction.z * sx;
      if (Math.abs(denom) < 1e-9) continue;
      const qx = w.ax - origin.x;
      const qz = w.az - origin.z;
      const t = (qx * sz - qz * sx) / denom;
      const u = (qx * direction.z - qz * direction.x) / denom;
      if (t < 0 || u < 0 || u > 1) continue;
      point.copy(origin).addScaledVector(direction, t);
      if (point.y >= 0 && point.y <= w.top) consider(null, null, false);
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
    for (const h of this.hitBoxes) {
      if (!h.alive) continue;
      box.min.set(h.x - h.half, h.y, h.z - h.half);
      box.max.set(h.x + h.half, h.y + h.height, h.z + h.half);
      if (ray.intersectBox(box, point)) consider(null, null, false, h);
    }
    return best;
  }

  /** A bullet or rocket hit: damages what was hit, or knocks a cone flying. */
  applyHit(hit: RayHit, damage: number, direction: THREE.Vector3): void {
    if (hit.target) this.damage(hit.target, damage, direction);
    if (hit.cone) this.knockCone(hit.cone, new THREE.Vector3(direction.x * 8, 4, direction.z * 8));
    if (hit.hitBox) hit.hitBox.onHit(damage, direction);
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
    for (const h of this.hitBoxes) {
      if (!h.alive) continue;
      box.min.set(h.x - h.half, h.y, h.z - h.half);
      box.max.set(h.x + h.half, h.y + h.height, h.z + h.half);
      const distance = box.distanceToPoint(at);
      if (distance > radius) continue;
      h.onHit(damage * (1 - 0.7 * (distance / radius)), new THREE.Vector3(h.x - at.x, 0, h.z - at.z).normalize());
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
   * `carRadius` is half the car's diagonal: how far its footprint reaches from its centre.
   */
  rebuildAll(carX: number, carZ: number, carRadius: number): void {
    for (const t of this.targets) {
      if (!t.alive && this.isUnderCar(t, carX, carZ, carRadius)) continue;
      if (!t.alive || (t.kind === 'building' && t.state !== 'standing')) this.restore(t, false);
      t.hp = t.maxHp;
    }
    this.effects.clear();
  }

  /** Whether `t`, once rebuilt, would overlap a car at (x, z) that reaches `carRadius` from its centre. */
  private isUnderCar(t: Target, x: number, z: number, carRadius: number): boolean {
    const margin = carRadius + 0.5; // the car's reach, plus a little room
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
      const height = (lamp ? 7 : 2.4) * t.size;
      fx.addDebris(
        new THREE.Vector3(t.x + direction.x, height / 2, t.z + direction.z),
        new THREE.Vector3(direction.x * 6, 3, direction.z * 6),
        lamp ? new THREE.Vector3(0.24 * t.size, height, 0.24 * t.size) : new THREE.Vector3(0.5 * t.size, height, 0.5 * t.size),
        lamp ? 0x3a3d42 : 0x6b4a2f,
        4,
      );
      if (lamp) fx.shatter(new THREE.Vector3(t.x, height, t.z), 4, [0xffffff, 0xcccccc], 0.3, 5); // the lamp's glass
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

  /** `carRadius` is half the car's diagonal: how far its footprint reaches from its centre. */
  update(dt: number, carX: number, carZ: number, carRadius: number): void {
    this.clock += dt;
    this.updateBuildings(dt);
    this.updateWrecks(dt);
    this.updateCones(dt, carX, carZ);

    // Bring destroyed things back once they've been gone a while and the player is far away.
    for (const t of this.targets) {
      if (t.alive || (t.kind === 'car' && t.flying)) continue;
      const longAgo = this.clock - t.destroyedAt > COMES_BACK_AFTER;
      // A giant car has to be further away, so the building doesn't come back inside it.
      const farAway = Math.hypot(carX - t.x, carZ - t.z) > COMES_BACK_DISTANCE + carRadius;
      if (longAgo && farAway && !this.isUnderCar(t, carX, carZ, carRadius)) this.restore(t, true);
    }

    this.updateWeather(dt, carX, carZ);
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
            root.position.x = THREE.MathUtils.clamp(root.position.x, -this.half + 3, this.half - 3);
            root.position.z = THREE.MathUtils.clamp(root.position.z, -this.half + 3, this.half - 3);
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

  /** Day or night. Some venues (the neon city) are always at night. */
  setNight(night: boolean): void {
    this.night = night;
    night = this.isNight;
    const theme = this.map === 'city' ? null : this.theme;
    this.scene.background = night ? this.nightBg : this.dayBg;
    // Rain and snow close in the fog.
    const weather = theme && theme.weather !== 'clear' ? 0.6 : 1;
    this.scene.fog = new THREE.Fog(night ? 0x0b1222 : (theme?.fog ?? 0xc4dcef), Math.min(60, this.viewDistance * 0.2) * weather, this.viewDistance * weather);
    this.hemi.intensity = night ? 0.25 : 1.2;
    this.hemi.color.set(night ? 0x6b7fb3 : 0xdfefff);
    this.sun.intensity = night ? 0.35 : 2.6;
    this.sun.color.set(night ? 0x9bb0ff : 0xfff2dd);
    for (const m of this.buildingMats) m.emissiveIntensity = night ? 1.4 : 0;
    this.lampHeadMat.emissiveIntensity = night ? 3 : 0;
    for (const car of this.parkedCars) car.setLights({ night: false, braking: false });
  }
}

/** A number for each grid cell (cells run from -50000 to 50000 each way). */
function cellKey(cx: number, cz: number): number {
  return (cx + 50000) * 100000 + (cz + 50000);
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

/** White text on a dark banner, for the start gantry. */
function bannerTexture(text: string): THREE.CanvasTexture {
  return canvasTexture(1024, 64, (ctx) => {
    ctx.fillStyle = '#15181d';
    ctx.fillRect(0, 0, 1024, 64);
    ctx.fillStyle = '#d7263d';
    ctx.fillRect(0, 0, 1024, 6);
    ctx.fillRect(0, 58, 1024, 6);
    ctx.fillStyle = '#ffffff';
    ctx.font = 'bold 40px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(`${text}  ·  START  ·  FINISH`, 512, 34);
  });
}
