// Builds the 3D car from a CarConfig. Everything is made from simple shapes,
// so there are no model files to load.
import * as THREE from 'three';
import type { BodyShape, CarConfig, PaintFinish, PartColor, Topper, WheelStyle } from './carConfig';
import { carbonTexture, liveryTexture, plateTexture, raceNumberTexture } from './textures';
import { radialGlowTexture } from '../libs/canvasTexture';

export type P = [number, number]; // (z, y) point on the car's side profile

export interface BodySpec {
  width: number;
  /** Top outline from the rear bottom corner over the car to the front bottom corner. */
  top: P[];
  bottomY: number;
  cabin: P[]; // rear-bottom, rear-top, front-top, front-bottom
  cabinWidth: number;
  axleRear: number;
  axleFront: number;
  track: number; // distance from center line to the wheel center
  wheelRadius: number;
  tireWidth: number;
  spoiler: P; // where the spoiler sits
  headlight: P;
  taillight: P;
  bed?: { z0: number; z1: number; y: number };
}

const BODIES: Record<BodyShape, BodySpec> = {
  sports: {
    width: 1.86,
    top: [[-2.2, 0.26], [-2.22, 0.72], [-1.9, 0.86], [-1.25, 0.9], [0.55, 0.86], [1.7, 0.72], [2.2, 0.52], [2.22, 0.28]],
    bottomY: 0.2,
    cabin: [[-1.35, 0.86], [-0.75, 1.24], [0.15, 1.26], [0.75, 0.84]],
    cabinWidth: 1.5,
    axleRear: -1.35,
    axleFront: 1.32,
    track: 0.78,
    wheelRadius: 0.34,
    tireWidth: 0.26,
    spoiler: [-1.95, 0.86],
    headlight: [2.08, 0.6],
    taillight: [-2.22, 0.66],
  },
  muscle: {
    width: 1.95,
    top: [[-2.4, 0.3], [-2.4, 0.9], [-2.0, 0.96], [-1.3, 0.96], [0.7, 0.95], [2.3, 0.92], [2.42, 0.8], [2.44, 0.34]],
    bottomY: 0.24,
    cabin: [[-1.45, 0.94], [-0.95, 1.36], [0.2, 1.38], [0.85, 0.93]],
    cabinWidth: 1.6,
    axleRear: -1.5,
    axleFront: 1.48,
    track: 0.82,
    wheelRadius: 0.36,
    tireWidth: 0.3,
    spoiler: [-2.15, 0.96],
    headlight: [2.4, 0.72],
    taillight: [-2.4, 0.8],
  },
  hatch: {
    width: 1.76,
    top: [[-1.95, 0.3], [-1.97, 0.95], [-1.6, 1.0], [0.5, 0.98], [1.6, 0.85], [1.95, 0.68], [1.97, 0.32]],
    bottomY: 0.2,
    cabin: [[-1.9, 0.97], [-1.75, 1.55], [0.25, 1.58], [1.05, 0.95]],
    cabinWidth: 1.56,
    axleRear: -1.25,
    axleFront: 1.22,
    track: 0.74,
    wheelRadius: 0.32,
    tireWidth: 0.22,
    spoiler: [-1.72, 1.57],
    headlight: [1.9, 0.72],
    taillight: [-1.98, 0.86],
  },
  pickup: {
    width: 2.0,
    top: [[-2.5, 0.55], [-2.5, 1.18], [-0.75, 1.2], [0.9, 1.2], [2.2, 1.15], [2.5, 1.0], [2.52, 0.55]],
    bottomY: 0.42,
    cabin: [[-0.75, 1.18], [-0.6, 1.9], [0.75, 1.92], [1.25, 1.18]],
    cabinWidth: 1.8,
    axleRear: -1.6,
    axleFront: 1.55,
    track: 0.86,
    wheelRadius: 0.44,
    tireWidth: 0.32,
    spoiler: [-2.3, 1.2],
    headlight: [2.46, 0.98],
    taillight: [-2.5, 1.02],
    bed: { z0: -2.4, z1: -0.85, y: 1.205 },
  },
  // A minibus: a low body with a tall glass box on top.
  van: {
    width: 1.95,
    top: [[-2.4, 0.35], [-2.42, 1.12], [1.9, 1.12], [2.35, 0.95], [2.44, 0.8], [2.45, 0.38]],
    bottomY: 0.3,
    cabin: [[-2.35, 1.1], [-2.3, 1.98], [1.0, 2.0], [1.9, 1.1]],
    cabinWidth: 1.88,
    axleRear: -1.55,
    axleFront: 1.6,
    track: 0.84,
    wheelRadius: 0.36,
    tireWidth: 0.26,
    spoiler: [-2.2, 2.0],
    headlight: [2.38, 0.82],
    taillight: [-2.42, 0.9],
  },
  // A low, wide supercar wedge with a steeply raked windscreen.
  wedge: {
    width: 1.98,
    top: [[-2.25, 0.3], [-2.27, 0.78], [-1.6, 0.84], [-0.6, 0.86], [1.0, 0.74], [2.1, 0.48], [2.3, 0.34], [2.32, 0.26]],
    bottomY: 0.18,
    cabin: [[-1.3, 0.84], [-0.7, 1.12], [0.05, 1.14], [1.1, 0.71]],
    cabinWidth: 1.5,
    axleRear: -1.4,
    axleFront: 1.38,
    track: 0.82,
    wheelRadius: 0.35,
    tireWidth: 0.3,
    spoiler: [-2.0, 0.84],
    headlight: [2.15, 0.5],
    taillight: [-2.27, 0.7],
  },
};

/**
 * Stretches a body: longer, wider and taller. Heights grow from the bottom of
 * the body, so the ground clearance stays the same. Wheels aren't stretched.
 */
function stretchBody(spec: BodySpec, long: number, wide: number, tall: number): BodySpec {
  const zy = ([z, y]: P): P => [z * long, spec.bottomY + (y - spec.bottomY) * tall];
  return {
    ...spec,
    width: spec.width * wide,
    cabinWidth: spec.cabinWidth * wide,
    track: spec.track * wide,
    top: spec.top.map(zy),
    cabin: spec.cabin.map(zy),
    axleRear: spec.axleRear * long,
    axleFront: spec.axleFront * long,
    spoiler: zy(spec.spoiler),
    headlight: zy(spec.headlight),
    taillight: zy(spec.taillight),
    bed: spec.bed && { z0: spec.bed.z0 * long, z1: spec.bed.z1 * long, y: spec.bottomY + (spec.bed.y - spec.bottomY) * tall },
  };
}

export const BEVEL = 0.05;

export interface WheelRig {
  steer: THREE.Group;
  spin: THREE.Group;
  front: boolean;
  radius: number;
}

/** A car body's shape after its length, width and height settings (see CarRig.shape). */
export interface CarShape {
  spec: BodySpec;
  /** Wheel center height, wheel arch radius, wheel center distance from the middle, tire width. */
  wheelY: number;
  archR: number;
  wheelX: number;
  tireWidth: number;
  /** The body's front and rear ends. */
  frontZ: number;
  rearZ: number;
}

export interface CarRig {
  root: THREE.Group;
  body: THREE.Group;
  wheels: WheelRig[];
  // Sizes in meters, including the overall size setting.
  length: number;
  width: number;
  height: number;
  wheelBase: number;
  /** Driver's eye height above the ground, for the bonnet camera. */
  eyeHeight: number;
  /** The exhaust tips, where nitro flames and boost trails come out. */
  exhausts: THREE.Object3D[];
  /** Where bullets and rockets leave the car, in the car's own coordinates (+z is forward). */
  gunMuzzle: THREE.Vector3;
  rocketMuzzle: THREE.Vector3;
  /**
   * What the car was built from, for packs that redraw it: the settings, and
   * the body's shape in body coordinates (y up from the ground, +z forward).
   * The body's own parts are named: 'shell' (body, cabin glass and roof),
   * 'shell-stripe', 'headlight', 'taillight', and each wheel's 'tire'.
   */
  config: CarConfig;
  shape: CarShape;
  setLights(state: { night: boolean; braking: boolean }): void;
  /** Shows flickering nitro flames out of the exhausts. Call every frame. */
  setNitro(on: boolean): void;
  /** Turns the car into a burnt-out wreck (every surface dark and rough), or back again. */
  setWrecked(wrecked: boolean): void;
  /** Rainbow paint and underglow, and the waving flag. `time` is in seconds. */
  animate(time: number): void;
  dispose(): void;
}

function paintMaterial(color: string, finish: PaintFinish, carbonMap: THREE.Texture): THREE.MeshPhysicalMaterial {
  const m = new THREE.MeshPhysicalMaterial({ color });
  switch (finish) {
    case 'gloss':
      Object.assign(m, { metalness: 0.05, roughness: 0.3, clearcoat: 1, clearcoatRoughness: 0.05 });
      break;
    case 'metallic':
      Object.assign(m, { metalness: 0.75, roughness: 0.32, clearcoat: 1, clearcoatRoughness: 0.08 });
      break;
    case 'matte':
      Object.assign(m, { metalness: 0.0, roughness: 0.85, clearcoat: 0 });
      break;
    case 'pearl':
      Object.assign(m, { metalness: 0.35, roughness: 0.3, clearcoat: 1, clearcoatRoughness: 0.05, iridescence: 1, iridescenceIOR: 1.6 });
      break;
    case 'chrome':
      Object.assign(m, { metalness: 1, roughness: 0.06, clearcoat: 0.6 });
      break;
    case 'carbon':
      // Carbon fiber weave, tinted a little by the paint color.
      Object.assign(m, { map: carbonMap, metalness: 0.2, roughness: 0.35, clearcoat: 1 });
      m.color.set('#ffffff').lerp(new THREE.Color(color), 0.35);
      break;
    case 'glow':
      // The paint lights itself up, so it shines at night.
      Object.assign(m, { metalness: 0.1, roughness: 0.4, emissive: new THREE.Color(color), emissiveIntensity: 0.7 });
      break;
    case 'rainbow':
      // Glossy; animate() changes the color every frame.
      Object.assign(m, { metalness: 0.4, roughness: 0.25, clearcoat: 1, clearcoatRoughness: 0.05 });
      break;
  }
  return m;
}

/**
 * Texture coordinates for an extruded body side: the (z, y) position in
 * meters, as if looking at the car side on. The roof and bonnet get the top
 * row of the pattern. Used to lay paint patterns over the body.
 */
const SIDE_ON_UVS = {
  generateTopUV(_geometry: THREE.ExtrudeGeometry, v: number[], a: number, b: number, c: number) {
    return [a, b, c].map((i) => new THREE.Vector2(v[i * 3], v[i * 3 + 1]));
  },
  generateSideWallUV(_geometry: THREE.ExtrudeGeometry, v: number[], a: number, b: number, c: number, d: number) {
    return [a, b, c, d].map((i) => new THREE.Vector2(v[i * 3], v[i * 3 + 1]));
  },
};

/** Side-profile shape with wheel arches cut into the bottom edge. */
function lowerProfile(spec: BodySpec, archR: number, wheelY: number): THREE.Shape {
  const shape = new THREE.Shape();
  const [first, ...rest] = spec.top;
  shape.moveTo(first[0], first[1]);
  for (const [z, y] of rest) shape.lineTo(z, y);
  const frontZ = spec.top[spec.top.length - 1][0];
  const rearZ = spec.top[0][0];
  shape.lineTo(frontZ - 0.1, spec.bottomY);
  // Walk the bottom edge from front to rear, going up and over each wheel.
  for (const axle of [spec.axleFront, spec.axleRear]) {
    const s = Math.max(-1, Math.min(1, (spec.bottomY - wheelY) / archR));
    const t0 = Math.asin(s);
    shape.lineTo(axle + archR * Math.cos(t0), spec.bottomY);
    const steps = 14;
    for (let i = 0; i <= steps; i++) {
      const t = t0 + ((Math.PI - 2 * t0) * i) / steps;
      shape.lineTo(axle + archR * Math.cos(t), wheelY + archR * Math.sin(t));
    }
    shape.lineTo(axle - archR * Math.cos(t0), spec.bottomY);
  }
  shape.lineTo(rearZ + 0.1, spec.bottomY);
  shape.closePath();
  return shape;
}

/**
 * Extrudes a side profile across the car's width, centered on x = 0.
 * `bevel` rounds the edges; the bevel also grows the outline outward by
 * `bevelSize` (defaults to `bevel`).
 */
function extrudeProfile(shape: THREE.Shape, width: number, bevel: number, bevelSize = bevel, sideOnUvs = false): THREE.ExtrudeGeometry {
  const depth = Math.max(0.01, width - 2 * bevel);
  const geo = new THREE.ExtrudeGeometry(shape, {
    depth,
    bevelEnabled: bevel > 0,
    bevelThickness: bevel,
    bevelSize,
    bevelSegments: 3,
    curveSegments: 8,
    UVGenerator: sideOnUvs ? SIDE_ON_UVS : undefined,
  });
  geo.translate(0, 0, -depth / 2);
  geo.rotateY(-Math.PI / 2);
  geo.computeVertexNormals();
  return geo;
}

function polyShape(points: P[]): THREE.Shape {
  const shape = new THREE.Shape();
  shape.moveTo(points[0][0], points[0][1]);
  for (const [z, y] of points.slice(1)) shape.lineTo(z, y);
  shape.closePath();
  return shape;
}

/** Height of the top outline at a given z, used to place parts on the body. */
function topAt(spec: BodySpec, z: number): number {
  const pts = spec.top;
  for (let i = 0; i < pts.length - 1; i++) {
    const [z0, y0] = pts[i];
    const [z1, y1] = pts[i + 1];
    if ((z >= z0 && z <= z1) || (z >= z1 && z <= z0)) {
      const t = z1 === z0 ? 0 : (z - z0) / (z1 - z0);
      return y0 + (y1 - y0) * t;
    }
  }
  return pts[0][1];
}

function buildWheel(
  style: WheelStyle,
  radius: number,
  width: number,
  rimMat: THREE.Material,
  rubber: THREE.Material,
  dark: THREE.Material,
): THREE.Group {
  const g = new THREE.Group();
  const tire = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius, width, 28, 1), rubber);
  tire.rotation.z = Math.PI / 2;
  tire.castShadow = true;
  tire.name = 'tyre';
  g.add(tire);

  const face = width / 2 + 0.006;
  const rimR = radius * (style === 'offroad' ? 0.56 : 0.66);
  const disc = (r: number, thick: number, mat: THREE.Material, x: number) => {
    const m = new THREE.Mesh(new THREE.CylinderGeometry(r, r, thick, 24), mat);
    m.rotation.z = Math.PI / 2;
    m.position.x = x;
    g.add(m);
    return m;
  };
  const ring = (r: number, tube: number) => {
    const m = new THREE.Mesh(new THREE.TorusGeometry(r, tube, 8, 28), rimMat);
    m.rotation.y = Math.PI / 2;
    m.position.x = face;
    g.add(m);
  };
  const spokes = (count: number, thickness: number) => {
    for (let i = 0; i < count; i++) {
      const pivot = new THREE.Group();
      pivot.rotation.x = (i / count) * Math.PI * 2;
      const spoke = new THREE.Mesh(new THREE.BoxGeometry(0.03, rimR * 0.95, thickness), rimMat);
      spoke.position.set(face, rimR * 0.48, 0);
      pivot.add(spoke);
      g.add(pivot);
    }
  };
  const bolts = (count: number, r: number, size: number, mat: THREE.Material) => {
    for (let i = 0; i < count; i++) {
      const a = (i / count) * Math.PI * 2;
      const b = new THREE.Mesh(new THREE.CylinderGeometry(size, size, 0.03, 8), mat);
      b.rotation.z = Math.PI / 2;
      b.position.set(face + 0.012, Math.cos(a) * r, Math.sin(a) * r);
      g.add(b);
    }
  };

  switch (style) {
    case 'fiveSpoke':
      ring(rimR, 0.028);
      spokes(5, 0.075);
      disc(rimR * 0.22, 0.05, rimMat, face);
      break;
    case 'multiSpoke':
      ring(rimR, 0.024);
      ring(rimR * 0.55, 0.014);
      spokes(12, 0.028);
      disc(rimR * 0.18, 0.05, rimMat, face);
      break;
    case 'dish':
      disc(rimR, 0.02, rimMat, face - 0.004);
      disc(rimR * 0.25, 0.04, rimMat, face + 0.01);
      bolts(5, rimR * 0.42, 0.018, dark);
      break;
    case 'offroad': {
      disc(rimR, 0.02, rimMat, face - 0.004);
      ring(rimR * 0.98, 0.03);
      bolts(6, rimR * 0.62, 0.05, dark);
      disc(rimR * 0.2, 0.05, rimMat, face + 0.01);
      // Chunky tread blocks around the tire.
      const treads = 16;
      for (let i = 0; i < treads; i++) {
        const pivot = new THREE.Group();
        pivot.rotation.x = (i / treads) * Math.PI * 2;
        const block = new THREE.Mesh(new THREE.BoxGeometry(width * 1.04, 0.05, 0.08), rubber);
        block.position.y = radius;
        pivot.add(block);
        g.add(pivot);
      }
      break;
    }
  }
  return g;
}

/** Builds a roof topper, standing on y = 0. */
function buildTopper(kind: Topper, paint: THREE.Material, accent: THREE.Material, add: (geo: THREE.BufferGeometry, mat: THREE.Material) => THREE.Mesh, track: <T extends { dispose(): void }>(x: T) => T): void {
  const mat = (color: number, metal = 0, rough = 0.5) => track(new THREE.MeshStandardMaterial({ color, metalness: metal, roughness: rough }));
  switch (kind) {
    case 'crown': {
      const gold = mat(0xd4af37, 1, 0.25);
      const band = add(new THREE.CylinderGeometry(0.17, 0.15, 0.1, 20, 1, true), gold);
      band.position.y = 0.05;
      (gold as THREE.MeshStandardMaterial).side = THREE.DoubleSide;
      for (let i = 0; i < 5; i++) {
        const a = (i / 5) * Math.PI * 2;
        const spike = add(new THREE.ConeGeometry(0.035, 0.11, 8), gold);
        spike.position.set(Math.sin(a) * 0.16, 0.15, Math.cos(a) * 0.16);
        const gem = add(new THREE.SphereGeometry(0.022, 8, 6), mat(0xd7263d, 0.2, 0.2));
        gem.position.set(Math.sin(a) * 0.175, 0.05, Math.cos(a) * 0.175);
      }
      break;
    }
    case 'topHat': {
      const black = mat(0x111111, 0, 0.7);
      add(new THREE.CylinderGeometry(0.22, 0.22, 0.02, 24), black).position.y = 0.01;
      add(new THREE.CylinderGeometry(0.13, 0.13, 0.3, 20), black).position.y = 0.17;
      add(new THREE.CylinderGeometry(0.133, 0.133, 0.06, 20), accent).position.y = 0.06;
      break;
    }
    case 'cone': {
      const orange = mat(0xff6a00, 0, 0.6);
      add(new THREE.BoxGeometry(0.32, 0.03, 0.32), orange).position.y = 0.015;
      add(new THREE.ConeGeometry(0.14, 0.45, 16), orange).position.y = 0.25;
      add(new THREE.CylinderGeometry(0.075, 0.095, 0.07, 16), mat(0xffffff)).position.y = 0.22;
      break;
    }
    case 'horns': {
      // A Viking helmet: a metal dome with two horns curving up and out.
      add(new THREE.SphereGeometry(0.17, 20, 10, 0, Math.PI * 2, 0, Math.PI / 2), mat(0x9aa0a6, 0.9, 0.3));
      for (const side of [-1, 1]) {
        const horn = add(new THREE.ConeGeometry(0.055, 0.4, 12), mat(0xf2e8d0, 0, 0.5));
        horn.position.set(side * 0.22, 0.2, 0);
        horn.rotation.z = -side * 0.8;
      }
      break;
    }
    case 'sharkFin': {
      const fin = new THREE.Shape();
      fin.moveTo(-0.3, 0);
      fin.lineTo(0.15, 0);
      fin.quadraticCurveTo(-0.05, 0.15, -0.25, 0.45);
      fin.quadraticCurveTo(-0.22, 0.2, -0.3, 0);
      add(extrudeProfile(fin, 0.05, 0.01), paint);
      break;
    }
    case 'halo': {
      const glow = track(new THREE.MeshBasicMaterial({ color: 0xffe680 }));
      const ring = add(new THREE.TorusGeometry(0.22, 0.03, 8, 32), glow);
      ring.position.y = 0.45;
      ring.rotation.x = Math.PI / 2;
      ring.castShadow = false;
      break;
    }
    case 'duck': {
      const yellow = mat(0xffd23f, 0, 0.35);
      const body = add(new THREE.SphereGeometry(0.16, 20, 14), yellow);
      body.scale.set(1, 0.75, 1.25);
      body.position.y = 0.12;
      add(new THREE.SphereGeometry(0.1, 16, 12), yellow).position.set(0, 0.3, 0.1);
      const beak = add(new THREE.ConeGeometry(0.04, 0.1, 10), mat(0xff7a1a));
      beak.rotation.x = Math.PI / 2;
      beak.position.set(0, 0.29, 0.22);
      for (const side of [-1, 1]) add(new THREE.SphereGeometry(0.014, 8, 6), mat(0x111111)).position.set(side * 0.045, 0.33, 0.18);
      break;
    }
    case 'none':
      break;
  }
}

/**
 * Builds a car. `withLights` adds a real headlight beam and underglow light;
 * only the player's car needs them, because every light costs rendering time.
 */
/**
 * Called with every car model as it's built (the player's, rivals', traffic and
 * parked cars), so packs can change how cars look.
 */
export const carBuiltHooks: ((rig: CarRig) => void)[] = [];

/**
 * Every car model built and not yet thrown away, so a pack that starts after
 * cars were built can change them too. Held weakly: a car the game forgets
 * about is let go.
 */
const built = new Set<WeakRef<CarRig>>();

/** The car models that exist now (the player's, rivals', traffic and parked cars). */
export function liveCars(): CarRig[] {
  const cars: CarRig[] = [];
  for (const ref of built) {
    const rig = ref.deref();
    if (rig) cars.push(rig);
    else built.delete(ref);
  }
  return cars;
}

export function buildCar(config: CarConfig, envNight = false, withLights = true): CarRig {
  const spec = stretchBody(BODIES[config.body], config.length, config.width, config.height);
  const disposables: { dispose(): void }[] = [];
  const track = <T extends { dispose(): void }>(x: T): T => {
    disposables.push(x);
    return x;
  };

  // The body's outline, before the bevel that rounds its edges.
  const frontZ = Math.max(...spec.top.map((p) => p[0])) + BEVEL;
  const rearZ = Math.min(...spec.top.map((p) => p[0])) - BEVEL;
  const topY = Math.max(...spec.top.map((p) => p[1]));
  const length = frontZ - rearZ;

  // Materials
  const carbonMap = track(carbonTexture());
  const paint = track(paintMaterial(config.paintColor, config.finish, carbonMap));
  const accent = track(paintMaterial(config.accentColor, config.finish === 'chrome' || config.finish === 'rainbow' ? 'gloss' : config.finish, carbonMap));
  const blackPlastic = track(new THREE.MeshStandardMaterial({ color: 0x111214, roughness: 0.6 }));
  const carbon = track(new THREE.MeshPhysicalMaterial({ map: carbonMap, roughness: 0.35, clearcoat: 1, metalness: 0.2 }));
  const chrome = track(new THREE.MeshStandardMaterial({ color: 0xdddddd, metalness: 1, roughness: 0.15 }));
  const rubber = track(new THREE.MeshStandardMaterial({ color: config.tireColor, roughness: 0.92 }));
  const rim = track(new THREE.MeshStandardMaterial({ color: config.rimColor, metalness: 0.85, roughness: 0.25 }));
  const tint = config.windowTint;
  const glass = track(
    new THREE.MeshPhysicalMaterial({
      color: new THREE.Color('#9cc3d9').lerp(new THREE.Color('#06080a'), tint),
      metalness: 0.3,
      roughness: 0.05,
      transparent: true,
      opacity: 0.55 + tint * 0.4,
    }),
  );
  const headMat = track(
    new THREE.MeshStandardMaterial({ color: 0xffffff, emissive: config.headlightColor, emissiveIntensity: 0.6 }),
  );
  const tailMat = track(new THREE.MeshStandardMaterial({ color: 0x550000, emissive: 0xff1a1a, emissiveIntensity: 0.4 }));
  const partMat: Record<PartColor, THREE.Material> = { body: paint, accent, black: blackPlastic, carbon };

  // A paint pattern goes on a copy of the paint, used only for the main body.
  // The pattern has the paint and accent colors drawn in, so the copy is white.
  let bodyPaint = paint;
  if (config.pattern !== 'none') {
    const livery = track(liveryTexture(config.pattern, config.paintColor, config.accentColor, length, topY - spec.bottomY));
    // The body's texture coordinates are meters (see SIDE_ON_UVS); map them onto the texture.
    livery.repeat.set(1 / length, 1 / (topY - spec.bottomY));
    livery.offset.set(-rearZ / length, -spec.bottomY / (topY - spec.bottomY));
    bodyPaint = track(paint.clone());
    bodyPaint.map = livery;
    bodyPaint.color.set(0xffffff);
  }

  const root = new THREE.Group();
  const body = new THREE.Group();
  root.add(body);

  const add = (geo: THREE.BufferGeometry, mat: THREE.Material, parent: THREE.Object3D = body) => {
    track(geo);
    const mesh = new THREE.Mesh(geo, mat);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    parent.add(mesh);
    return mesh;
  };

  // ---------- wheels and suspension height ----------
  // Body coordinates have y = 0 at the ground for the stock car. `wheelY` is
  // the wheel center in body coordinates; lifting the body moves it down.
  const R = spec.wheelRadius;
  const r = R * config.wheelSize;
  let wheelY = R - config.lift;
  // Wheel arches can only be so big before they run off the ends of the body.
  // Wheels too big for them push the body up until it clears the tires, like
  // a monster truck.
  const archMax = Math.min((spec.axleFront - spec.axleRear) / 2, frontZ - spec.axleFront, spec.axleRear - rearZ) - 0.05;
  let archR = r + 0.07 + BEVEL;
  if (archR > archMax) {
    archR = archMax;
    wheelY = Math.min(wheelY, spec.bottomY - r - 0.08);
  }
  body.position.y = r - wheelY;

  // ---------- main body and cabin ----------
  const lowerShape = lowerProfile(spec, archR, wheelY);
  add(extrudeProfile(lowerShape, spec.width, BEVEL, BEVEL, true), bodyPaint).name = 'shell';
  const cabinGlass = add(extrudeProfile(polyShape(spec.cabin), spec.cabinWidth, 0.03), glass);
  cabinGlass.castShadow = false;
  cabinGlass.name = 'shell';
  const [, roofRear, roofFront] = spec.cabin;
  const roofLen = roofFront[0] - roofRear[0];
  const roofY = Math.max(roofRear[1], roofFront[1]);
  const roofMid = (roofRear[0] + roofFront[0]) / 2;
  const roofTop = roofY + 0.045;
  const roof = add(new THREE.BoxGeometry(spec.cabinWidth * 0.97, 0.05, roofLen + 0.06), paint);
  roof.position.set(0, roofY + 0.02, roofMid);
  roof.name = 'shell';

  if (spec.bed) {
    const bed = add(new THREE.BoxGeometry(spec.width - 0.22, 0.02, spec.bed.z1 - spec.bed.z0), blackPlastic);
    bed.position.set(0, spec.bed.y + BEVEL, (spec.bed.z0 + spec.bed.z1) / 2);
  }

  // ---------- stripes and race number ----------
  if (config.stripes === 'racing') {
    for (const side of [-1, 1]) {
      // The body's bevel pushes its surface BEVEL out from the profile, so the
      // stripe uses the same profile pushed slightly further, sitting on top of the paint.
      const stripe = add(extrudeProfile(lowerShape, 0.22, 0.004, BEVEL + 0.006), accent);
      stripe.position.x = side * 0.2;
      stripe.name = 'shell-stripe';
      const roofStripe = add(new THREE.BoxGeometry(0.22, 0.012, roofLen + 0.06), accent);
      roofStripe.position.set(side * 0.2, roofY + 0.05, roof.position.z);
      roofStripe.name = 'shell-stripe';
    }
  } else if (config.stripes === 'side') {
    const z0 = spec.axleRear + archR;
    const z1 = spec.axleFront - archR;
    const midZ = (z0 + z1) / 2;
    const y = (spec.bottomY + topAt(spec, midZ)) / 2 + 0.05;
    for (const side of [-1, 1]) {
      const s = add(new THREE.BoxGeometry(0.012, 0.09, Math.max(0.1, z1 - z0)), accent);
      s.position.set(side * (spec.width / 2 + 0.004), y, midZ);
      const s2 = add(new THREE.BoxGeometry(0.012, 0.03, Math.max(0.1, z1 - z0)), accent);
      s2.position.set(side * (spec.width / 2 + 0.004), y - 0.09, midZ);
    }
  }
  if (config.raceNumber > 0) {
    // A roundel on each door, facing out.
    const numberMat = track(new THREE.MeshStandardMaterial({ map: track(raceNumberTexture(config.raceNumber)), transparent: true, alphaTest: 0.5, roughness: 0.5 }));
    const midZ = (spec.axleRear + spec.axleFront) / 2;
    const size = Math.min(0.5, (topAt(spec, midZ) - spec.bottomY) * 0.8);
    for (const side of [-1, 1]) {
      const roundel = add(new THREE.PlaneGeometry(size, size), numberMat);
      roundel.position.set(side * (spec.width / 2 + 0.012), (spec.bottomY + topAt(spec, midZ)) / 2 + 0.03, midZ);
      roundel.rotation.y = side * (Math.PI / 2);
      roundel.castShadow = false;
    }
  }

  // ---------- spoiler ----------
  if (config.spoiler !== 'none') {
    const mat = partMat[config.spoilerColor];
    const [sz, sy] = spec.spoiler;
    const baseY = sy + BEVEL;
    if (config.spoiler === 'lip') {
      const lip = add(new THREE.BoxGeometry(spec.width * 0.86, 0.05, 0.2), mat);
      lip.position.set(0, baseY + 0.03, sz - 0.05);
      lip.rotation.x = 0.2;
    } else {
      // Wing, GT wing and dragster wing: the same parts, taller and bigger.
      const big = config.spoiler !== 'wing';
      const postH = config.spoiler === 'drag' ? 1.0 : big ? 0.38 : 0.22;
      for (const side of [-1, 1]) {
        const post = add(new THREE.BoxGeometry(0.05, postH, 0.12), mat);
        post.position.set(side * spec.width * 0.3, baseY + postH / 2, sz);
      }
      const plankW = spec.width * (big ? 1.02 : 0.9);
      const plankD = config.spoiler === 'drag' ? 0.6 : big ? 0.4 : 0.3;
      const plank = add(new THREE.BoxGeometry(plankW, 0.045, plankD), mat);
      plank.position.set(0, baseY + postH + 0.02, sz - 0.04);
      plank.rotation.x = -0.12;
      if (big) {
        for (const side of [-1, 1]) {
          const plate = add(new THREE.BoxGeometry(0.025, 0.2, plankD + 0.04), mat);
          plate.position.set(side * (plankW / 2 + 0.012), baseY + postH + 0.02, sz - 0.04);
        }
      }
    }
  }

  // ---------- lights and plates ----------
  const [hz, hy] = spec.headlight;
  for (const side of [-1, 1]) {
    const h = add(new THREE.BoxGeometry(0.34, 0.1, 0.1), headMat);
    h.name = 'headlight';
    h.position.set(side * (spec.width / 2 - 0.3), hy, hz + BEVEL);
    h.castShadow = false;
  }
  const [tz, ty] = spec.taillight;
  for (const side of [-1, 1]) {
    const t = add(new THREE.BoxGeometry(0.34, 0.08, 0.06), tailMat);
    t.name = 'taillight';
    t.position.set(side * (spec.width / 2 - 0.26), ty, tz - BEVEL);
    t.castShadow = false;
  }
  // Light reach doesn't grow with the car's scale on its own, so scale it here.
  const beam = new THREE.SpotLight(config.headlightColor, 0, 70 * config.size, 0.55, 0.6, 1.2);
  beam.position.set(0, hy, frontZ);
  beam.target.position.set(0, -1.5, frontZ + 20);
  if (withLights) body.add(beam, beam.target);

  const plateMap = track(plateTexture(config.plate));
  const plateMat = track(new THREE.MeshStandardMaterial({ map: plateMap, roughness: 0.5 }));
  const plateY = Math.max(spec.bottomY + 0.14, ty - 0.24);
  const rearPlate = add(new THREE.PlaneGeometry(0.52, 0.19), plateMat);
  rearPlate.position.set(0, plateY, rearZ - 0.012);
  rearPlate.rotation.y = Math.PI;
  const frontPlate = add(new THREE.PlaneGeometry(0.52, 0.19), plateMat);
  frontPlate.position.set(0, Math.max(spec.bottomY + 0.14, hy - 0.24), frontZ + 0.012);

  // ---------- exhaust and nitro flames ----------
  // Each exhaust tip: where it is, and whether it points back or (truck stacks) up.
  const tips: { x: number; y: number; z: number; up: boolean }[] = [];
  const pipeR = config.exhaust === 'quad' ? 0.038 : config.exhaust === 'stacks' ? 0.06 : 0.048;
  if (config.exhaust === 'stacks') {
    // Two tall chrome pipes standing just behind the cabin.
    const z = Math.max(rearZ + 0.25, spec.cabin[0][0] - 0.15);
    const baseY = topAt(spec, z) + BEVEL;
    for (const side of [-1, 1]) {
      const x = side * (spec.width / 2 - 0.14);
      const pipe = add(new THREE.CylinderGeometry(pipeR, pipeR, 1.0, 14, 1, true), chrome);
      pipe.position.set(x, baseY + 0.5, z);
      tips.push({ x, y: baseY + 1.02, z, up: true });
    }
  } else {
    const xs =
      config.exhaust === 'single'
        ? [-spec.width * 0.3]
        : config.exhaust === 'dual'
          ? [-spec.width * 0.3, spec.width * 0.3]
          : [-spec.width * 0.36, -spec.width * 0.27, spec.width * 0.27, spec.width * 0.36];
    for (const x of xs) {
      const pipe = add(new THREE.CylinderGeometry(pipeR, pipeR, 0.3, 14, 1, true), chrome);
      pipe.rotation.x = Math.PI / 2;
      pipe.position.set(x, spec.bottomY + 0.06, rearZ + 0.06);
      tips.push({ x, y: spec.bottomY + 0.06, z: rearZ - 0.08, up: false });
    }
  }

  // Nitro flames: two nested cones at each tip, hidden until boosting.
  const flameOuterMat = track(
    new THREE.MeshBasicMaterial({ color: config.nitroColor, transparent: true, opacity: 0.8, blending: THREE.AdditiveBlending, depthWrite: false }),
  );
  const flameCoreMat = track(new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.9, blending: THREE.AdditiveBlending, depthWrite: false }));
  // Wide at the pipe, pointed 1 m behind it (before scaling).
  const flameGeo = track(new THREE.ConeGeometry(1, 1, 12, 1, true).translate(0, 0.5, 0).rotateX(-Math.PI / 2));
  const flames: THREE.Group[] = [];
  for (const tip of tips) {
    const flame = new THREE.Group();
    flame.position.set(tip.x, tip.y, tip.z);
    if (tip.up) flame.rotation.x = Math.PI / 2; // turn the backward-pointing flame to point up
    const outer = new THREE.Mesh(flameGeo, flameOuterMat);
    outer.scale.set(pipeR * 2.2, pipeR * 2.2, 0.9);
    const core = new THREE.Mesh(flameGeo, flameCoreMat);
    core.scale.set(pipeR * 1.1, pipeR * 1.1, 0.45);
    flame.add(outer, core);
    flame.visible = false;
    body.add(flame);
    flames.push(flame);
  }
  // Points just past each tip, for boost trail particles.
  const exhausts = tips.map((tip) => {
    const spot = new THREE.Object3D();
    spot.position.set(tip.x, tip.y + (tip.up ? 0.3 : 0), tip.z - (tip.up ? 0 : 0.3));
    body.add(spot);
    return spot;
  });

  // ---------- roof weapons ----------
  // A twin machine gun on the right and a rocket pod on the left: only the
  // ones the car has (config.armed; none if it isn't said), so guns on the roof always fire.
  const gunMuzzle = new THREE.Vector3(0.32, body.position.y + roofTop + 0.2, roofMid + 0.9);
  const rocketMuzzle = new THREE.Vector3(-0.32, body.position.y + roofTop + 0.22, roofMid + 0.55);
  const roofGun = config.roofGuns && !!config.armed?.gun;
  const roofPod = config.roofGuns && !!config.armed?.rockets;
  const gunMetal = roofGun || roofPod ? track(new THREE.MeshStandardMaterial({ color: 0x2b2e33, metalness: 0.8, roughness: 0.35 })) : null;
  if (gunMetal) {
    const base = add(new THREE.BoxGeometry(1.0, 0.06, 0.9), blackPlastic);
    base.position.set(0, roofTop + 0.03, roofMid);
  }
  if (roofGun && gunMetal) {
    // Machine gun: a body, two barrels and an ammo box.
    const gunBody = add(new THREE.BoxGeometry(0.2, 0.16, 0.5), gunMetal);
    gunBody.position.set(0.32, roofTop + 0.2, roofMid + 0.05);
    const post = add(new THREE.CylinderGeometry(0.05, 0.07, 0.12, 10), gunMetal);
    post.position.set(0.32, roofTop + 0.1, roofMid);
    for (const dx of [-0.045, 0.045]) {
      const barrel = add(new THREE.CylinderGeometry(0.022, 0.022, 0.6, 10), gunMetal);
      barrel.rotation.x = Math.PI / 2;
      barrel.position.set(0.32 + dx, roofTop + 0.2, roofMid + 0.6);
    }
    const ammo = add(new THREE.BoxGeometry(0.14, 0.12, 0.2), partMat.accent);
    ammo.position.set(0.47, roofTop + 0.12, roofMid - 0.05);
  } else {
    // No machine gun on the roof: it fires from just above the bonnet.
    gunMuzzle.set(0.3, body.position.y + spec.headlight[1] + 0.2, frontZ + 0.1);
  }
  if (roofPod && gunMetal) {
    // Rocket pod: a box with four tubes, the tips painted in the accent color.
    const pod = add(new THREE.BoxGeometry(0.34, 0.3, 0.7), gunMetal);
    pod.position.set(-0.32, roofTop + 0.22, roofMid + 0.05);
    for (const dx of [-0.075, 0.075]) {
      for (const dy of [-0.07, 0.07]) {
        const tube = add(new THREE.CylinderGeometry(0.055, 0.055, 0.04, 12), blackPlastic);
        tube.rotation.x = Math.PI / 2;
        tube.position.set(-0.32 + dx, roofTop + 0.22 + dy, roofMid + 0.41);
        const tip = add(new THREE.ConeGeometry(0.045, 0.1, 10), partMat.accent);
        tip.rotation.x = Math.PI / 2;
        tip.position.set(-0.32 + dx, roofTop + 0.22 + dy, roofMid + 0.44);
      }
    }
  } else {
    // No rocket pod on the roof: rockets fire from just above the bonnet.
    rocketMuzzle.set(-0.3, body.position.y + spec.headlight[1] + 0.2, frontZ + 0.1);
  }

  // ---------- topper, antenna, bull bar and light bar ----------
  if (config.topper !== 'none') {
    // In the middle of the roof, between the guns if there are any.
    const topper = new THREE.Group();
    topper.position.set(0, roofTop + (gunMetal ? 0.06 : 0), roofMid - (gunMetal ? 0.15 : 0));
    body.add(topper);
    buildTopper(config.topper, paint, accent, (geo, mat) => add(geo, mat, topper), track);
  }

  let flag: THREE.Mesh | null = null;
  if (config.antenna !== 'none') {
    // On the back corner of the body.
    const z = rearZ + 0.45;
    const antenna = new THREE.Group();
    antenna.position.set(-(spec.width / 2 - 0.15), topAt(spec, z) + BEVEL, z);
    body.add(antenna);
    const put = (geo: THREE.BufferGeometry, mat: THREE.Material, y: number) => {
      const m = add(geo, mat, antenna);
      m.position.y = y;
      return m;
    };
    if (config.antenna === 'whip') {
      put(new THREE.CylinderGeometry(0.006, 0.012, 1.1, 6), blackPlastic, 0.55);
      put(new THREE.SphereGeometry(0.02, 8, 6), blackPlastic, 1.1);
    } else if (config.antenna === 'flag') {
      put(new THREE.CylinderGeometry(0.01, 0.01, 1.0, 8), chrome, 0.5);
      // The flag hangs off the back of the pole; animate() makes it wave.
      const cloth = track(new THREE.MeshStandardMaterial({ color: config.accentColor, side: THREE.DoubleSide, roughness: 0.8 }));
      const pivot = new THREE.Group();
      pivot.position.y = 0.85;
      antenna.add(pivot);
      flag = add(new THREE.PlaneGeometry(0.45, 0.28).translate(-0.225, 0, 0), cloth, pivot);
      flag.rotation.y = -Math.PI / 2;
      flag.castShadow = false;
    } else {
      put(new THREE.CylinderGeometry(0.008, 0.008, 0.6, 6), blackPlastic, 0.3);
      put(new THREE.SphereGeometry(0.09, 16, 12), accent, 0.66);
    }
  }

  if (config.bullBar) {
    // Chrome tubes across the front.
    const z = frontZ + 0.14;
    const top = hy + 0.12;
    for (const side of [-1, 1]) {
      const post = add(new THREE.CylinderGeometry(0.035, 0.035, top - spec.bottomY, 10), chrome);
      post.position.set(side * spec.width * 0.25, (top + spec.bottomY) / 2, z);
    }
    for (const y of [spec.bottomY + 0.12, top]) {
      const bar = add(new THREE.CylinderGeometry(0.035, 0.035, spec.width * 0.75, 10), chrome);
      bar.rotation.z = Math.PI / 2;
      bar.position.set(0, y, z);
    }
  }

  if (config.lightBar) {
    // A row of lamps along the front of the roof.
    const barWidth = Math.min(spec.cabinWidth * 0.9, 1.4);
    const z = roofFront[0] - 0.12;
    const housing = add(new THREE.BoxGeometry(barWidth, 0.08, 0.14), blackPlastic);
    housing.position.set(0, roofTop + 0.05, z);
    for (let i = 0; i < 6; i++) {
      const lamp = add(new THREE.BoxGeometry(barWidth / 7, 0.05, 0.02), headMat);
      lamp.position.set((i - 2.5) * (barWidth / 6.2), roofTop + 0.05, z + 0.075);
      lamp.castShadow = false;
    }
  }

  // ---------- wheels ----------
  const tireW = spec.tireWidth * config.tireWidth;
  // Wider tires grow outwards; the track setting pushes the wheels further out.
  const wheelX = spec.track * config.track + (tireW - spec.tireWidth) / 2;
  const camber = (config.camber * Math.PI) / 180;
  const wheels: WheelRig[] = [];
  for (const [z, front] of [[spec.axleFront, true], [spec.axleRear, false]] as const) {
    for (const side of [-1, 1]) {
      const steer = new THREE.Group();
      steer.position.set(side * wheelX, r, z);
      steer.rotation.z = -side * camber; // negative camber tips the top of the wheel in
      const spin = new THREE.Group();
      const wheel = buildWheel(config.wheelStyle, r, tireW, rim, rubber, blackPlastic);
      if (side < 0) wheel.rotation.y = Math.PI; // rim faces outward on both sides
      wheel.traverse((o) => {
        if (o instanceof THREE.Mesh) track(o.geometry);
      });
      spin.add(wheel);
      steer.add(spin);
      root.add(steer);
      wheels.push({ steer, spin, front, radius: r * config.size });
    }
  }
  // A lifted body gets a shock absorber from each wheel up to the body.
  const bodyBottom = body.position.y + spec.bottomY;
  if (bodyBottom - r > 0.25) {
    for (const z of [spec.axleFront, spec.axleRear]) {
      for (const side of [-1, 1]) {
        const strut = add(new THREE.CylinderGeometry(0.05, 0.05, bodyBottom - r + 0.1, 10), chrome, root);
        strut.position.set(side * Math.max(0.2, wheelX - tireW / 2 - 0.08), (bodyBottom + r) / 2, z);
      }
    }
  }

  // ---------- underglow ----------
  // A soft colored light under the car plus a glow decal on the ground.
  const glowLight = new THREE.PointLight(config.underglowColor, 0, 6 * config.size, 1.5);
  glowLight.position.y = 0.15;
  if (withLights) root.add(glowLight);
  let glowDecal: THREE.Mesh | null = null;
  let glowMat: THREE.MeshBasicMaterial | null = null;
  if (config.underglow !== 'off') {
    const glowMap = track(radialGlowTexture());
    glowMat = track(
      new THREE.MeshBasicMaterial({
        color: config.underglowColor,
        map: glowMap,
        transparent: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      }),
    );
    glowDecal = add(new THREE.PlaneGeometry(spec.width + 1.6, length + 1.4), glowMat, root);
    glowDecal.rotation.x = -Math.PI / 2;
    glowDecal.position.y = 0.03;
    glowDecal.castShadow = false;
    glowDecal.receiveShadow = false;
  }

  // The whole car grows or shrinks with the size setting.
  root.scale.setScalar(config.size);

  // For wrecks: one burnt material, and a note of each part's real material to put back.
  let wrecked = false;
  const burnt = track(new THREE.MeshStandardMaterial({ color: 0x1c1a18, roughness: 1, metalness: 0.2 }));
  const realMaterials = new Map<THREE.Mesh, THREE.Material | THREE.Material[]>();
  let lightsState = { night: envNight, braking: false }; // to restore the lights after a wreck is repaired

  const rig: CarRig = {
    root,
    body,
    config,
    shape: { spec, wheelY, archR, wheelX, tireWidth: tireW, frontZ, rearZ },
    wheels,
    length: length * config.size,
    width: Math.max(spec.width, 2 * (wheelX + tireW / 2)) * config.size,
    height: (body.position.y + roofTop) * config.size,
    wheelBase: (spec.axleFront - spec.axleRear) * config.size,
    eyeHeight: (roofY - 0.2 + body.position.y) * config.size,
    exhausts,
    gunMuzzle,
    rocketMuzzle,
    setLights({ night, braking }) {
      lightsState = { night, braking };
      if (wrecked) return;
      headMat.emissiveIntensity = night ? 2.2 : 0.6;
      beam.intensity = night ? 60 : 0;
      tailMat.emissiveIntensity = braking ? 2.5 : night ? 1.0 : 0.35;
      glowLight.intensity = config.underglow !== 'off' ? (night ? 6 : 2) : 0;
    },
    setNitro(on) {
      for (const flame of flames) {
        flame.visible = on;
        // A random length each frame makes the flames flicker.
        flame.scale.set(1.2, 1.2, 0.75 + Math.random() * 0.5);
      }
    },
    setWrecked(on) {
      if (on === wrecked) return;
      wrecked = on;
      if (on) {
        // Swap every lit surface for burnt metal (glowing parts like the flames stay as they are).
        root.traverse((part) => {
          if (part instanceof THREE.Mesh && !(part.material instanceof THREE.MeshBasicMaterial)) {
            realMaterials.set(part, part.material);
            part.material = burnt;
          }
        });
        beam.intensity = 0;
        glowLight.intensity = 0;
        if (glowDecal) glowDecal.visible = false;
      } else {
        for (const [part, material] of realMaterials) part.material = material;
        realMaterials.clear();
        if (glowDecal) glowDecal.visible = true;
        rig.setLights(lightsState);
      }
    },
    animate(time) {
      if (wrecked) return;
      // Rainbow paint and underglow cycle through every color, about every 12 seconds.
      const hue = (time * 0.08) % 1;
      if (config.finish === 'rainbow') {
        paint.color.setHSL(hue, 0.85, 0.5);
        // With a pattern, tint it instead, so the pattern still shows.
        if (bodyPaint !== paint) bodyPaint.color.setHSL(hue, 0.7, 0.75);
      }
      if (config.underglow === 'rainbow' && glowMat) {
        glowMat.color.setHSL((hue + 0.5) % 1, 1, 0.55);
        glowLight.color.copy(glowMat.color);
      }
      if (flag) flag.rotation.y = -Math.PI / 2 + Math.sin(time * 7) * 0.3;
    },
    dispose() {
      built.delete(ref);
      for (const d of disposables) d.dispose();
      root.removeFromParent();
    },
  };
  const ref = new WeakRef(rig);
  built.add(ref);
  rig.setLights({ night: envNight, braking: false });
  for (const hook of carBuiltHooks) hook(rig);
  return rig;
}
