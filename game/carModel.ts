// Builds the 3D car from a CarConfig. Everything is made from simple shapes,
// so there are no model files to load.
import * as THREE from 'three';
import type { BodyShape, CarConfig, PaintFinish, PartColor, WheelStyle } from './carConfig';
import { carbonTexture, plateTexture } from './textures';
import { radialGlowTexture } from '../libs/canvasTexture';

type P = [number, number]; // (z, y) point on the car's side profile

interface BodySpec {
  width: number;
  /** Top outline from the rear bottom corner over the car to the front bottom corner. */
  top: P[];
  bottomY: number;
  cabin: P[]; // rear-bottom, rear-top, front-top, front-bottom
  cabinWidth: number;
  axleRear: number;
  axleFront: number;
  track: number; // distance from centre line to the wheel centre
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
};

const BEVEL = 0.05;

export interface WheelRig {
  steer: THREE.Group;
  spin: THREE.Group;
  front: boolean;
  radius: number;
}

export interface CarRig {
  root: THREE.Group;
  body: THREE.Group;
  wheels: WheelRig[];
  length: number;
  width: number;
  wheelBase: number;
  /** Driver's eye height above the ground, for the bonnet camera. */
  eyeHeight: number;
  setLights(state: { night: boolean; braking: boolean }): void;
  dispose(): void;
}

function paintMaterial(color: string, finish: PaintFinish): THREE.MeshPhysicalMaterial {
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
  }
  return m;
}

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

/** Extrudes a side profile across the car's width, centred on x = 0. */
function extrudeProfile(shape: THREE.Shape, width: number, bevel: number): THREE.ExtrudeGeometry {
  const depth = Math.max(0.01, width - 2 * bevel);
  const geo = new THREE.ExtrudeGeometry(shape, {
    depth,
    bevelEnabled: bevel > 0,
    bevelThickness: bevel,
    bevelSize: bevel,
    bevelSegments: 3,
    curveSegments: 8,
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
      // Chunky tread blocks around the tyre.
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

/**
 * Builds a car. `withLights` adds a real headlight beam and underglow light;
 * only the player's car needs them, because every light costs rendering time.
 */
export function buildCar(config: CarConfig, envNight = false, withLights = true): CarRig {
  const spec = BODIES[config.body];
  const disposables: { dispose(): void }[] = [];
  const track = <T extends { dispose(): void }>(x: T): T => {
    disposables.push(x);
    return x;
  };

  // Materials
  const paint = track(paintMaterial(config.paintColor, config.finish));
  const accent = track(paintMaterial(config.accentColor, config.finish === 'chrome' ? 'gloss' : config.finish));
  const blackPlastic = track(new THREE.MeshStandardMaterial({ color: 0x111214, roughness: 0.6 }));
  const carbonMap = track(carbonTexture());
  const carbon = track(new THREE.MeshPhysicalMaterial({ map: carbonMap, roughness: 0.35, clearcoat: 1, metalness: 0.2 }));
  const chrome = track(new THREE.MeshStandardMaterial({ color: 0xdddddd, metalness: 1, roughness: 0.15 }));
  const rubber = track(new THREE.MeshStandardMaterial({ color: 0x1a1a1a, roughness: 0.92 }));
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

  const root = new THREE.Group();
  const body = new THREE.Group();
  root.add(body);

  const R = spec.wheelRadius;
  const r = R * config.wheelSize;
  const rideOffset = config.rideHeight * 0.07;
  // The body sits higher with bigger wheels and a raised ride height.
  body.position.y = r - R + rideOffset;
  const wheelYInBody = R - rideOffset;
  const archR = r + 0.07 + BEVEL;

  const add = (geo: THREE.BufferGeometry, mat: THREE.Material, parent: THREE.Object3D = body) => {
    track(geo);
    const mesh = new THREE.Mesh(geo, mat);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    parent.add(mesh);
    return mesh;
  };

  // Main body and cabin
  const lowerShape = lowerProfile(spec, archR, wheelYInBody);
  add(extrudeProfile(lowerShape, spec.width, BEVEL), paint);
  add(extrudeProfile(polyShape(spec.cabin), spec.cabinWidth, 0.03), glass).castShadow = false;
  const [, roofRear, roofFront] = spec.cabin;
  const roofLen = roofFront[0] - roofRear[0];
  const roofY = Math.max(roofRear[1], roofFront[1]);
  const roof = add(new THREE.BoxGeometry(spec.cabinWidth * 0.97, 0.05, roofLen + 0.06), paint);
  roof.position.set(0, roofY + 0.02, (roofRear[0] + roofFront[0]) / 2);

  const frontZ = Math.max(...spec.top.map((p) => p[0])) + BEVEL;
  const rearZ = Math.min(...spec.top.map((p) => p[0])) - BEVEL;
  const length = frontZ - rearZ;

  if (spec.bed) {
    const bed = add(new THREE.BoxGeometry(spec.width - 0.22, 0.02, spec.bed.z1 - spec.bed.z0), blackPlastic);
    bed.position.set(0, spec.bed.y + BEVEL, (spec.bed.z0 + spec.bed.z1) / 2);
  }

  // Stripes
  if (config.stripes === 'racing') {
    for (const side of [-1, 1]) {
      const stripe = add(extrudeProfile(lowerShape, 0.22, 0), accent);
      stripe.scale.setScalar(1.012);
      stripe.position.x = side * 0.2;
      const roofStripe = add(new THREE.BoxGeometry(0.22, 0.012, roofLen + 0.06), accent);
      roofStripe.position.set(side * 0.2, roofY + 0.05, roof.position.z);
    }
  } else if (config.stripes === 'side') {
    const z0 = spec.axleRear + archR;
    const z1 = spec.axleFront - archR;
    const midZ = (z0 + z1) / 2;
    const y = (spec.bottomY + topAt(spec, midZ)) / 2 + 0.05;
    for (const side of [-1, 1]) {
      const s = add(new THREE.BoxGeometry(0.012, 0.09, z1 - z0), accent);
      s.position.set(side * (spec.width / 2 + 0.004), y, midZ);
      const s2 = add(new THREE.BoxGeometry(0.012, 0.03, z1 - z0), accent);
      s2.position.set(side * (spec.width / 2 + 0.004), y - 0.09, midZ);
    }
  }

  // Spoiler
  if (config.spoiler !== 'none') {
    const mat = partMat[config.spoilerColor];
    const [sz, sy] = spec.spoiler;
    const baseY = sy + BEVEL;
    if (config.spoiler === 'lip') {
      const lip = add(new THREE.BoxGeometry(spec.width * 0.86, 0.05, 0.2), mat);
      lip.position.set(0, baseY + 0.03, sz - 0.05);
      lip.rotation.x = 0.2;
    } else {
      const gt = config.spoiler === 'gt';
      const postH = gt ? 0.38 : 0.22;
      for (const side of [-1, 1]) {
        const post = add(new THREE.BoxGeometry(0.05, postH, 0.12), mat);
        post.position.set(side * spec.width * 0.3, baseY + postH / 2, sz);
      }
      const plankW = spec.width * (gt ? 1.02 : 0.9);
      const plank = add(new THREE.BoxGeometry(plankW, 0.045, gt ? 0.4 : 0.3), mat);
      plank.position.set(0, baseY + postH + 0.02, sz - 0.04);
      plank.rotation.x = -0.12;
      if (gt) {
        for (const side of [-1, 1]) {
          const plate = add(new THREE.BoxGeometry(0.025, 0.2, 0.44), mat);
          plate.position.set(side * (plankW / 2 + 0.012), baseY + postH + 0.02, sz - 0.04);
        }
      }
    }
  }

  // Lights
  const [hz, hy] = spec.headlight;
  for (const side of [-1, 1]) {
    const h = add(new THREE.BoxGeometry(0.34, 0.1, 0.1), headMat);
    h.position.set(side * (spec.width / 2 - 0.3), hy, hz + BEVEL);
    h.castShadow = false;
  }
  const [tz, ty] = spec.taillight;
  for (const side of [-1, 1]) {
    const t = add(new THREE.BoxGeometry(0.34, 0.08, 0.06), tailMat);
    t.position.set(side * (spec.width / 2 - 0.26), ty, tz - BEVEL);
    t.castShadow = false;
  }
  const beam = new THREE.SpotLight(config.headlightColor, 0, 70, 0.55, 0.6, 1.2);
  beam.position.set(0, hy, frontZ);
  beam.target.position.set(0, -1.5, frontZ + 20);
  if (withLights) body.add(beam, beam.target);

  // Number plates
  const plateMap = track(plateTexture(config.plate));
  const plateMat = track(new THREE.MeshStandardMaterial({ map: plateMap, roughness: 0.5 }));
  const plateY = Math.max(spec.bottomY + 0.14, ty - 0.24);
  const rearPlate = add(new THREE.PlaneGeometry(0.52, 0.19), plateMat);
  rearPlate.position.set(0, plateY, rearZ - 0.012);
  rearPlate.rotation.y = Math.PI;
  const frontPlate = add(new THREE.PlaneGeometry(0.52, 0.19), plateMat);
  frontPlate.position.set(0, Math.max(spec.bottomY + 0.14, hy - 0.24), frontZ + 0.012);

  // Exhaust
  const pipeR = config.exhaust === 'quad' ? 0.038 : 0.048;
  const pipeXs =
    config.exhaust === 'single'
      ? [-spec.width * 0.3]
      : config.exhaust === 'dual'
        ? [-spec.width * 0.3, spec.width * 0.3]
        : [-spec.width * 0.36, -spec.width * 0.27, spec.width * 0.27, spec.width * 0.36];
  for (const x of pipeXs) {
    const pipe = add(new THREE.CylinderGeometry(pipeR, pipeR, 0.3, 14, 1, true), chrome);
    pipe.rotation.x = Math.PI / 2;
    pipe.position.set(x, spec.bottomY + 0.06, rearZ + 0.06);
  }

  // Wheels
  const tireW = spec.tireWidth * config.tireWidth;
  const wheels: WheelRig[] = [];
  for (const [z, front] of [[spec.axleFront, true], [spec.axleRear, false]] as const) {
    for (const side of [-1, 1]) {
      const steer = new THREE.Group();
      steer.position.set(side * (spec.track + (tireW - spec.tireWidth) / 2), r, z);
      const spin = new THREE.Group();
      const wheel = buildWheel(config.wheelStyle, r, tireW, rim, rubber, blackPlastic);
      if (side < 0) wheel.rotation.y = Math.PI; // rim faces outward on both sides
      wheel.traverse((o) => {
        if (o instanceof THREE.Mesh) track(o.geometry);
      });
      spin.add(wheel);
      steer.add(spin);
      root.add(steer);
      wheels.push({ steer, spin, front, radius: r });
    }
  }

  // Underglow: a soft coloured light under the car plus a glow decal on the ground.
  const glowLight = new THREE.PointLight(config.underglowColor, 0, 6, 1.5);
  glowLight.position.y = 0.15;
  if (withLights) root.add(glowLight);
  let glowDecal: THREE.Mesh | null = null;
  if (config.underglow) {
    const glowMap = track(radialGlowTexture());
    const glowMat = track(
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

  const rig: CarRig = {
    root,
    body,
    wheels,
    length,
    width: spec.width,
    wheelBase: spec.axleFront - spec.axleRear,
    eyeHeight: roofY - 0.2 + body.position.y,
    setLights({ night, braking }) {
      headMat.emissiveIntensity = night ? 2.2 : 0.6;
      beam.intensity = night ? 60 : 0;
      tailMat.emissiveIntensity = braking ? 2.5 : night ? 1.0 : 0.35;
      glowLight.intensity = config.underglow ? (night ? 6 : 2) : 0;
    },
    dispose() {
      for (const d of disposables) d.dispose();
      root.removeFromParent();
    },
  };
  rig.setLights({ night: envNight, braking: false });
  return rig;
}
