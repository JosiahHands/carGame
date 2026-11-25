// Real car models. Each body type gets a detailed car body (models/cars.glb:
// six cars by Daniel Zhabotinsky, CC BY 4.0, see models/CREDITS.md) with
// every badge and logo taken off. The body is stretched to the car's garage
// shape and sits round the game's own wheels, so the garage still decides
// the paint (and finish, pattern and stripes), window tint, length, width,
// height, lift, wheels, rims, track and every add-on.
//
// The models load in the background. Until they're ready (or if they can't
// load) cars keep their sculpted bodies (carBody.ts). All cars share one copy
// of each model's shapes and textures; far-away cars use a simpler copy.
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/addons/libs/meshopt_decoder.module.js';
import type { BodyShape } from '../../game/carConfig';
import type { CarRig } from '../../game/carModel';
import { sculptedBody, stripeStrip } from './carBody';
import { seatExhausts } from './exhausts';
import { fastRays } from './fastRays';

/** The models file. Vite copies it next to the game and gives its address (works from the packaged app too). */
const MODELS_URL = new URL('./models/cars.glb', import.meta.url).href;
/** Cars further than this from the camera (in metres) switch to the simpler body. */
const FAR = 45;
const REAL = 'photoRealModel';

/** A model's measurements in its own metres: +z forward, y up, wheels on the ground at y = 0. */
interface Measure {
  axleFront: number;
  axleRear: number;
  wheelRadius: number;
  bodyWidth: number; // below the windows, without the door mirrors
  archRadius: number; // from a wheel centre to the edge of its arch (the tightest of the four)
  min: number[];
  max: number[];
}

interface Model {
  near: THREE.Object3D;
  far: THREE.Object3D;
  measure: Measure;
}

type Library = Partial<Record<BodyShape, Model>>;

let loading: Promise<Library> | null = null;
let library: Library | null = null;
let failed = false;
/** Whether cars should get real bodies (the "Car models" setting). */
let enabled = true;
/** Every car seen, so the setting can switch them all. */
const cars = new Set<CarRig>();

/** Reads a file as bytes. XMLHttpRequest also reads file:// addresses, as the packaged game uses. */
function fetchBytes(url: string): Promise<ArrayBuffer> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('GET', url);
    request.responseType = 'arraybuffer';
    // A file:// read reports status 0 when it works.
    request.onload = () => (request.status === 0 || (request.status >= 200 && request.status < 300) ? resolve(request.response as ArrayBuffer) : reject(new Error(`${url}: ${request.status}`)));
    request.onerror = () => reject(new Error(`Couldn't read ${url}`));
    request.send();
  });
}

/** Turns the loaded file into one model per body type. */
export function toLibrary(scene: THREE.Object3D, anisotropy: number): Library {
  const out: Library = {};
  for (const node of scene.children) {
    const match = /^car-([a-z]+)$/.exec(node.name);
    if (!match) continue;
    const far = scene.getObjectByName(`${node.name}-far`) ?? node;
    for (const part of [node, far]) {
      part.traverse((o) => {
        const mesh = o as THREE.Mesh;
        if (!mesh.isMesh) return;
        mesh.castShadow = true;
        mesh.receiveShadow = true;
        const material = mesh.material as THREE.MeshStandardMaterial;
        for (const map of [material.map, material.normalMap, material.roughnessMap, material.metalnessMap]) if (map) map.anisotropy = anisotropy;
      });
    }
    out[match[1] as BodyShape] = { near: node, far, measure: node.userData as Measure };
  }
  return out;
}

/**
 * A model's rear (every triangle reaching behind its rear axle), made once per
 * model, for the exhaust pipes to find the rear bumper on. It's cut into
 * slices 10 cm wide across the car: a ray along the car only meets one
 * slice, which is much quicker than looking at every triangle of the car.
 * Never drawn.
 */
const rears = new Map<Model, THREE.BufferGeometry[]>();
const rayMaterial = new THREE.MeshBasicMaterial({ side: THREE.DoubleSide });
const SLICE = 0.1;
function rearOf(model: Model): THREE.Group {
  let slices = rears.get(model);
  if (!slices) {
    const points = new Map<number, number[]>();
    const copy = model.near.clone();
    copy.position.set(0, 0, 0); // as it goes on the car
    copy.updateMatrixWorld(true);
    const a = new THREE.Vector3();
    const b = new THREE.Vector3();
    const c = new THREE.Vector3();
    copy.traverse((o) => {
      const mesh = o as THREE.Mesh;
      if (!mesh.isMesh) return;
      const position = mesh.geometry.attributes.position;
      const index = mesh.geometry.index;
      const count = index ? index.count : position.count;
      for (let i = 0; i < count; i += 3) {
        a.fromBufferAttribute(position, index ? index.getX(i) : i).applyMatrix4(mesh.matrixWorld);
        b.fromBufferAttribute(position, index ? index.getX(i + 1) : i + 1).applyMatrix4(mesh.matrixWorld);
        c.fromBufferAttribute(position, index ? index.getX(i + 2) : i + 2).applyMatrix4(mesh.matrixWorld);
        if (Math.min(a.z, b.z, c.z) >= model.measure.axleRear) continue;
        // Into every slice the triangle reaches across.
        const first = Math.floor(Math.min(a.x, b.x, c.x) / SLICE);
        const last = Math.floor(Math.max(a.x, b.x, c.x) / SLICE);
        for (let k = first; k <= last; k++) {
          let list = points.get(k);
          if (!list) points.set(k, (list = []));
          list.push(a.x, a.y, a.z, b.x, b.y, b.z, c.x, c.y, c.z);
        }
      }
    });
    slices = [...points.values()].map((list) => {
      const geometry = new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(list, 3));
      geometry.computeBoundingBox();
      geometry.computeBoundingSphere();
      return geometry;
    });
    rears.set(model, slices);
  }
  const group = new THREE.Group();
  for (const geometry of slices) group.add(new THREE.Mesh(geometry, rayMaterial));
  return group;
}

/** Starts loading the models (once). */
function load(anisotropy: number): Promise<Library> {
  loading ??= fetchBytes(MODELS_URL)
    .then(
      (bytes) =>
        new Promise<Library>((resolve, reject) => {
          const loader = new GLTFLoader();
          loader.setMeshoptDecoder(MeshoptDecoder);
          loader.parse(bytes, '', (gltf) => resolve(toLibrary(gltf.scene, anisotropy)), reject);
        }),
    )
    .then((loaded) => (library = loaded))
    .catch((error) => {
      // The cars keep their sculpted bodies.
      failed = true;
      console.warn('Photo Real: the car models could not be loaded, so cars keep their sculpted bodies.', error);
      return {};
    });
  return loading;
}

/** Things kept for taking the real body off again. */
interface Undo {
  hidden: THREE.Object3D[];
  added: THREE.Object3D[];
  moved: { object: THREE.Object3D; position: THREE.Vector3 }[];
  map: { material: THREE.MeshStandardMaterial; map: THREE.Texture | null } | null;
  dispose: { dispose(): void }[];
}

/**
 * Gives a car its real body: now if the models have loaded, or as soon as
 * they have. Call after sculptCar (the sculpted body shows until then).
 */
export function realCar(rig: CarRig, anisotropy: number): void {
  if (!cars.has(rig)) {
    cars.add(rig);
    const dispose = rig.dispose.bind(rig);
    rig.dispose = () => {
      cars.delete(rig);
      dispose();
    };
  }
  if (!enabled || failed) return;
  if (library) fitModel(rig, library);
  else void load(anisotropy).then((loaded) => cars.has(rig) && enabled && fitModel(rig, loaded));
}

/** Switches every car between real and sculpted bodies (the "Car models" setting). */
export function setRealCars(on: boolean, anisotropy: number): void {
  enabled = on;
  for (const rig of cars) {
    if (on) realCar(rig, anisotropy);
    else unrealCar(rig);
  }
}

/** The scale and position that fit a model to a car's garage shape (in the car body's coordinates). */
export function fitFor(rig: CarRig, m: Measure): { scale: THREE.Vector3; position: THREE.Vector3 } {
  const { spec } = rig.shape;
  const config = rig.config;
  // Lengthways: the model's axles go on the game's axles, so the wheels sit in the arches.
  const sz = (spec.axleFront - spec.axleRear) / (m.axleFront - m.axleRear);
  // Across: the body as wide as the game's.
  const sx = spec.width / m.bodyWidth;
  // Up: the roof at the game's roof height, but never squashed or stretched
  // much more than the garage's height setting asks for (so it keeps its looks).
  const roof = Math.max(...spec.cabin.map((p) => p[1])) + 0.045;
  const even = ((sx + sz) / 2) * config.height;
  const fitY = (roof - spec.wheelRadius) / (m.max[1] - m.wheelRadius);
  const sy = THREE.MathUtils.clamp(fitY, even * 0.94, even * 1.06);
  // The model's wheel centres go where the stock wheels' centres are; lifting
  // the car moves the wheels, not the body, as in the game...
  let y = spec.wheelRadius - m.wheelRadius * sy;
  // ...but the body rides up if the tyres would poke through the tops of its arches
  // (bigger wheels than the model's, or a squashed body).
  const tyreTop = rig.shape.wheelY + spec.wheelRadius * config.wheelSize + 0.015;
  const archTop = spec.wheelRadius + m.archRadius * sy;
  y += Math.max(0, tyreTop - archTop);
  return {
    scale: new THREE.Vector3(sx, sy, sz),
    position: new THREE.Vector3(0, y, (spec.axleFront + spec.axleRear) / 2),
  };
}

/** Puts the model on the car, hiding the sculpted body. */
function fitModel(rig: CarRig, lib: Library): void {
  const model = lib[rig.config.body];
  if (!model || rig.root.userData[REAL] || !rig.root.userData['photoRealUndo']) return;
  const sculpted = sculptedBody(rig);
  const parts = rig.body.children.filter((c): c is THREE.Mesh => (c as THREE.Mesh).isMesh);
  const shells = parts.filter((p) => p.name === 'shell');
  const glassPart = shells.find((p) => (p.material as THREE.Material).transparent);
  const paintPart = shells.find((p) => p !== glassPart && p.geometry.type === 'ExtrudeGeometry');
  const head = parts.find((p) => p.name === 'headlight')?.material as THREE.MeshStandardMaterial | undefined;
  const tail = parts.find((p) => p.name === 'taillight')?.material as THREE.MeshStandardMaterial | undefined;
  if (!glassPart || !paintPart || !head || !tail) return;
  const paint = paintPart.material as THREE.MeshStandardMaterial;
  const glass = glassPart.material as THREE.Material;
  // A see-through car (the time trial's ghost) has one see-through material
  // on every part: the model wears it too, all over.
  const seeThrough = paint.transparent ? paint : null;
  const undo: Undo = { hidden: [], added: [], moved: [], map: null, dispose: [] };

  // ---------- the body, fitted to the garage shape ----------
  const { scale, position } = fitFor(rig, model.measure);
  const group = new THREE.Group();
  group.name = 'real-body';
  group.scale.copy(scale);
  group.position.copy(position);

  // Lamps glow with the game's own headlights and tail lights (on at night, brighter when braking).
  const lamps = new Map<THREE.Material, THREE.Material>();
  const lampFor = (source: THREE.MeshStandardMaterial, driver: THREE.MeshStandardMaterial, strength: number) => {
    let lamp = lamps.get(source);
    if (!lamp) {
      const copy = source.clone();
      copy.emissive.copy(driver.emissive);
      copy.emissiveMap = source.map;
      // Always as bright as the game's lamp: read through, never stored.
      Object.defineProperty(copy, 'emissiveIntensity', { get: () => driver.emissiveIntensity * strength, set: () => {}, configurable: true });
      lamp = copy;
      lamps.set(source, lamp);
      undo.dispose.push(lamp);
    }
    return lamp;
  };
  const dress = (source: THREE.Object3D) => {
    const copy = source.clone(); // shares the shapes and textures
    copy.position.set(0, 0, 0);
    copy.traverse((o) => {
      const mesh = o as THREE.Mesh;
      if (!mesh.isMesh) return;
      const name = (mesh.material as THREE.Material).name;
      if (seeThrough) {
        mesh.material = seeThrough;
        mesh.castShadow = false;
      } else if (name === 'paint') mesh.material = paint;
      else if (name === 'glass') mesh.material = glass;
      else if (name === 'lamp-front') mesh.material = lampFor(mesh.material as THREE.MeshStandardMaterial, head, 0.6);
      else if (name === 'lamp-rear') mesh.material = lampFor(mesh.material as THREE.MeshStandardMaterial, tail, 0.8);
    });
    return copy;
  };
  const near = dress(model.near);
  const lod = new THREE.LOD();
  lod.addLevel(near, 0);
  lod.addLevel(dress(model.far), FAR);
  group.add(lod);
  // Not on the car yet, so its parts' world matrices are in the car body's coordinates (for the rays below).
  group.updateMatrixWorld(true);

  // Paint patterns and carbon fibre: the model's paint is laid out side-on in its
  // own metres, so a copy of the pattern is moved and stretched to match.
  if (paint.map) {
    const map = paint.map.clone();
    map.repeat.set(paint.map.repeat.x * scale.z, paint.map.repeat.y * scale.y);
    map.offset.set(paint.map.offset.x + position.z * paint.map.repeat.x, paint.map.offset.y + position.y * paint.map.repeat.y);
    undo.map = { material: paint, map: paint.map };
    undo.dispose.push(map);
    paint.map = map;
  }

  // ---------- hide what the model replaces ----------
  const hide = (o: THREE.Object3D) => {
    if (!o.visible) return;
    o.visible = false;
    undo.hidden.push(o);
  };
  for (const o of sculpted) hide(o);
  for (const p of parts) if (p.name === 'headlight' || p.name === 'taillight') hide(p);

  // ---------- details placed on the new body ----------
  const meshes: THREE.Mesh[] = [];
  near.traverse((o) => (o as THREE.Mesh).isMesh && meshes.push(o as THREE.Mesh));
  const solid = meshes.filter((m) => m.material !== glass);

  // Racing stripes over the bonnet, roof and boot (not over the windows).
  const stripe = parts.find((p) => p.name === 'shell-stripe');
  if (stripe) {
    for (const side of [-1, 1]) {
      const strip = stripeStrip(rig.shape, () => meshes, side * 0.2, 0.22, stripe.material as THREE.Material, true, (hit) => (hit.object as THREE.Mesh).material === paint);
      rig.body.add(strip);
      undo.added.push(strip);
      undo.dispose.push(strip.geometry);
    }
  }

  // Number plates onto the bumpers.
  const ray = new THREE.Raycaster();
  fastRays(solid);
  for (const plate of parts) {
    const geometry = plate.geometry as THREE.PlaneGeometry;
    if (geometry.type !== 'PlaneGeometry' || geometry.parameters.width !== 0.52) continue;
    const front = plate.position.z > 0;
    ray.set(new THREE.Vector3(0, plate.position.y, front ? 20 : -20), new THREE.Vector3(0, 0, front ? -1 : 1));
    const hit = ray.intersectObjects(solid, false)[0];
    if (!hit) continue;
    undo.moved.push({ object: plate, position: plate.position.clone() });
    plate.position.z = hit.point.z + (front ? 0.012 : -0.012);
  }

  rig.body.add(group);
  undo.added.push(group);
  // The exhaust pipes onto the new body: into the rear bumper, or stacks standing on it.
  const rear = rearOf(model);
  group.add(rear);
  seatExhausts(rig, meshes, undo.moved, { rear: [rear] });
  rear.removeFromParent();
  rig.root.userData[REAL] = undo;
  const dispose = rig.dispose.bind(rig);
  rig.dispose = () => {
    for (const x of undo.dispose) x.dispose();
    undo.dispose.length = 0;
    dispose();
  };
}

/** Takes a car's real body off, back to the sculpted one. */
function unrealCar(rig: CarRig): void {
  const undo = rig.root.userData[REAL] as Undo | undefined;
  if (!undo) return;
  for (const o of undo.added) o.removeFromParent();
  for (const o of undo.hidden) o.visible = true;
  for (const { object, position } of undo.moved) object.position.copy(position);
  if (undo.map) undo.map.material.map = undo.map.map;
  for (const x of undo.dispose) x.dispose();
  undo.dispose.length = 0;
  delete rig.root.userData[REAL];
}

/** Takes every real body off and frees the models (the pack is switched off). */
export function unrealCars(): void {
  for (const rig of cars) unrealCar(rig);
  cars.clear();
  if (library) {
    const geometries = new Set<THREE.BufferGeometry>();
    const materials = new Set<THREE.Material>();
    for (const model of Object.values(library)) {
      for (const part of [model.near, model.far]) {
        part.traverse((o) => {
          const mesh = o as THREE.Mesh;
          if (!mesh.isMesh) return;
          geometries.add(mesh.geometry);
          materials.add(mesh.material as THREE.Material);
        });
      }
    }
    for (const g of geometries) g.dispose();
    // The rears made for the exhausts go with their models.
    for (const slices of rears.values()) for (const g of slices) g.dispose();
    rears.clear();
    for (const m of materials) {
      for (const value of Object.values(m)) if ((value as THREE.Texture | null)?.isTexture) (value as THREE.Texture).dispose();
      m.dispose();
    }
  }
  library = null;
  loading = null;
  failed = false;
  enabled = true;
}
