// Real car bodies. The game's cars are side profiles pushed straight across,
// like cookie cutters. This builds the same car as a sculpted shell instead:
// cross-sections from bumper to bumper, with rounded corners, sides that
// curve in towards the roof and under the sills, wheel arches with dark
// wheel wells, and a glass cabin with pillars, a painted roof and an
// interior. It also adds the details that make a car read as real: rounded
// light clusters, a grille, door mirrors and handles, tyres with sidewalls,
// and brake discs and calipers behind the wheels.
//
// Every number comes from the car's own shape (CarRig.shape), so every
// change in the garage (body, length, width, height, lift, wheel size,
// track, paint, pattern, stripes) still shapes the car.
import * as THREE from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';
import { BEVEL, type CarRig, type CarShape } from '../../game/carModel';

/** Smooth height of the body's top outline (bonnet, roof line of the lower body, boot) at any z. */
function topLine(shape: CarShape): (z: number) => number {
  // The outline's first and last points are the bottoms of the bumpers; the rest is the top.
  const points = shape.spec.top.slice(1, -1).map(([z, y]) => new THREE.Vector2(z, y));
  const curve = new THREE.SplineCurve(points);
  const samples = curve.getPoints(160).sort((a, b) => a.x - b.x);
  return (z: number) => {
    if (z <= samples[0].x) return samples[0].y;
    for (let i = 1; i < samples.length; i++) {
      if (z <= samples[i].x) {
        const a = samples[i - 1];
        const b = samples[i];
        return a.y + ((b.y - a.y) * (z - a.x)) / Math.max(1e-6, b.x - a.x);
      }
    }
    return samples[samples.length - 1].y;
  };
}

/** A point on a cross-section; `liner` marks the inside of a wheel well (painted dark). */
interface Pt {
  x: number;
  y: number;
  liner: boolean;
}

/** Materials for the different parts of a mesh, in the order of their group numbers. */
type Paint = 'paint' | 'liner' | 'glass' | 'trim';

/**
 * Builds a mesh from rings of points (one ring per station along z). Each
 * quad gets a material from `pick(station, segment)`. Rings must all have the
 * same number of points; `closed` joins the last point back to the first.
 */
function loft(rings: { z: number; points: Pt[] }[], closed: boolean, pick: (k: number, j: number) => Paint, materials: Record<Paint, THREE.Material>): THREE.Mesh {
  const count = rings[0].points.length;
  const positions: number[] = [];
  const uvs: number[] = [];
  for (const ring of rings) {
    for (const p of ring.points) {
      positions.push(p.x, p.y, ring.z);
      // Side-on texture coordinates in metres, as the game's body uses (for paint patterns).
      uvs.push(ring.z, p.y);
    }
  }
  const order: Paint[] = ['paint', 'liner', 'glass', 'trim'];
  const indices: Record<Paint, number[]> = { paint: [], liner: [], glass: [], trim: [] };
  const segments = closed ? count : count - 1;
  for (let k = 0; k < rings.length - 1; k++) {
    for (let j = 0; j < segments; j++) {
      const a = k * count + j;
      const b = k * count + ((j + 1) % count);
      const c = (k + 1) * count + j;
      const d = (k + 1) * count + ((j + 1) % count);
      indices[pick(k, j)].push(a, b, c, b, d, c);
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
  const all: number[] = [];
  const used: THREE.Material[] = [];
  for (const name of order) {
    if (indices[name].length === 0) continue;
    geometry.addGroup(all.length, indices[name].length, used.length);
    all.push(...indices[name]);
    used.push(materials[name]);
  }
  geometry.setIndex(all);
  geometry.computeVertexNormals();
  const mesh = new THREE.Mesh(geometry, used);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}

/** A superellipse quarter: from angle 0 (out to the side) to 90° (straight up or down), `n` sets how square it is. */
function quarter(n: number, steps: number): { c: number; s: number }[] {
  const out: { c: number; s: number }[] = [];
  for (let i = 0; i <= steps; i++) {
    const t = (i / steps) * (Math.PI / 2);
    out.push({ c: Math.cos(t) ** (2 / n), s: Math.sin(t) ** (2 / n) });
  }
  return out;
}

/** Mirrors a right-hand half section (from the bottom middle up to the top middle) into a full closed ring. */
function mirror(half: Pt[]): Pt[] {
  const left = half.slice(1, -1).reverse().map((p) => ({ ...p, x: -p.x }));
  return [...half, ...left];
}

/** The lower body: everything below the windows. */
function buildShell(shape: CarShape, materials: Record<Paint, THREE.Material>): { mesh: THREE.Mesh; top: (z: number) => number } {
  const { spec, wheelY, archR, wheelX, tireWidth, frontZ, rearZ } = shape;
  const line = topLine(shape);
  const top = (z: number) => line(z) + BEVEL;
  const hw = spec.width / 2;
  const yb = spec.bottomY - 0.03;
  // Where each wheel well's inner wall is (just inside the tyre).
  const linerX = Math.min(hw - 0.08, Math.max(0.15, wheelX - tireWidth / 2 - 0.05));
  const cornerR = Math.min(0.26, hw * 0.3); // corners rounded off in plan view
  const noseR = 0.1; // the top edge of the bumpers rounded over

  // Stations along the car: closer together at the ends, plus one at each edge of each arch.
  const zs = new Set<number>();
  const steps = 64;
  for (let i = 0; i <= steps; i++) {
    const t = (1 - Math.cos((i / steps) * Math.PI)) / 2;
    zs.add(rearZ + (frontZ - rearZ) * t);
  }
  const archAt = (z: number) => {
    // The arch's height over this z, or -Infinity outside every arch.
    let best = -Infinity;
    for (const axle of [spec.axleFront, spec.axleRear]) {
      const dz = z - axle;
      if (Math.abs(dz) < archR) best = Math.max(best, wheelY + Math.sqrt(archR * archR - dz * dz));
    }
    return best;
  };
  for (const axle of [spec.axleFront, spec.axleRear]) {
    const s = Math.max(-1, Math.min(1, (yb - wheelY) / archR));
    const edge = archR * Math.cos(Math.asin(s));
    for (const z of [axle - edge - 0.004, axle - edge + 0.004, axle + edge - 0.004, axle + edge + 0.004]) zs.add(z);
    for (let i = 1; i < 8; i++) zs.add(axle - edge + (2 * edge * i) / 8);
  }
  const stations = [...zs].filter((z) => z >= rearZ && z <= frontZ).sort((a, b) => a - b);

  const lower = quarter(7, 7); // bottom middle round to the side (quite square)
  const upper = quarter(5, 9); // side round to the top middle
  const rings: { z: number; points: Pt[] }[] = [];
  const archRow: number[] = []; // which segment is the ledge over the wheel, per station (-1 if none)

  for (const z of stations) {
    const yt = Math.max(yb + 0.2, top(z));
    const yS = yb + (yt - yb) * 0.55; // the shoulder, the widest point
    const d = Math.min(z - rearZ, frontZ - z);
    const plan = d < cornerR ? (hw - cornerR + Math.sqrt(cornerR * cornerR - (cornerR - d) ** 2)) / hw : 1;
    const nose = d < noseR ? (yt - noseR + Math.sqrt(noseR * noseR - (noseR - d) ** 2) - yb) / (yt - yb) : 1;
    const arch = archAt(z);
    const inArch = arch > yb + 0.02;
    const edgeY = inArch ? Math.min(arch, yS - 0.01) : yb + (yS - yb) * 0.3;

    // Right half, from the bottom middle up: under the car, round the sill, up the side...
    const half: Pt[] = [];
    for (let i = lower.length - 1; i >= 0; i--) {
      const { c, s } = lower[i];
      half.push({ x: hw * c, y: yS - (yS - yb) * s, liner: false });
    }
    // ...with the wheel arch's edge as a pair of points (outside skin, then well wall).
    const outerX = hw * Math.pow(Math.max(0, 1 - ((yS - edgeY) / (yS - yb)) ** 7), 1 / 7);
    let at = half.findIndex((p) => p.y > edgeY);
    if (at < 0) at = half.length;
    half.splice(at, 0, { x: inArch ? linerX : outerX, y: edgeY, liner: inArch }, { x: outerX, y: edgeY, liner: false });
    archRow.push(inArch ? at : -1);
    if (inArch) {
      // Under the arch, the body steps in to the well's wall.
      for (let i = 0; i < at; i++) {
        if (half[i].x > linerX) half[i] = { x: linerX, y: half[i].y, liner: true };
        else half[i].liner = i > 0;
      }
    }
    // ...then from the shoulder over to the top middle, curving in (tumblehome).
    for (let i = 1; i < upper.length; i++) {
      const { c, s } = upper[i];
      half.push({ x: hw * c * (1 - 0.06 * s), y: yS + (yt - yS) * s, liner: false });
    }
    // Rounded ends: narrower and lower towards the bumpers.
    const points = mirror(half).map((p) => ({ x: p.x * plan, y: yb + (p.y - yb) * nose, liner: p.liner }));
    rings.push({ z, points });
  }

  // Close the ends with a small bevel and a point.
  const cap = (ring: { z: number; points: Pt[] }, dz: number) => {
    const midY = (Math.min(...ring.points.map((p) => p.y)) + Math.max(...ring.points.map((p) => p.y))) / 2;
    const squeeze = (k: number) => ring.points.map((p) => ({ x: p.x * k, y: midY + (p.y - midY) * k, liner: false }));
    return [
      { z: ring.z + dz * 0.7, points: squeeze(0.96) },
      { z: ring.z + dz, points: squeeze(0) },
    ];
  };
  const [rearBevel, rearTip] = cap(rings[0], -0.008);
  const [frontBevel, frontTip] = cap(rings[rings.length - 1], 0.008);
  const all = [rearTip, rearBevel, ...rings, frontBevel, frontTip];
  const offset = 2; // the two rear cap rings come first

  const mesh = loft(all, true, (k, j) => {
    // Wheel wells are dark: the wall and ledge segments of the arch on either side.
    const count = all[0].points.length;
    for (const kk of [k - offset, k + 1 - offset]) {
      const row = archRow[kk];
      if (row === undefined || row < 0) continue;
      const ring = all[kk + offset].points;
      const right = j < count / 2;
      const p = ring[j];
      const q = ring[(j + 1) % count];
      if (p.liner && q.liner) return 'liner';
      // The ledge: from the well wall's top out to the skin's edge.
      if (right && j === row) return 'liner';
      if (!right && (p.liner || q.liner)) return 'liner';
    }
    return 'paint';
  }, materials);
  return { mesh, top };
}

/** The cabin: glass with pillars and a painted roof, and a dark interior seen through the windows. */
function buildCabin(shape: CarShape, top: (z: number) => number, materials: Record<Paint, THREE.Material>, interior: THREE.Material): { group: THREE.Group; outside: THREE.Mesh } {
  const { spec } = shape;
  const [c0, c1, c2, c3] = spec.cabin;
  const roofAt = (z: number) => {
    const seg = (a: number[], b: number[]) => a[1] + ((b[1] - a[1]) * (z - a[0])) / Math.max(1e-6, b[0] - a[0]);
    const y = z < c1[0] ? seg(c0, c1) : z < c2[0] ? seg(c1, c2) : seg(c2, c3);
    return y + 0.04;
  };
  const bw = spec.cabinWidth / 2;
  const roofLength = c2[0] - c1[0];
  // Pillars between the side windows: one on a coupe, more on a long van.
  const pillars: number[] = [];
  const between = Math.max(1, Math.floor(roofLength / 1.0));
  for (let i = 1; i <= between; i++) pillars.push(c1[0] + (roofLength * i) / (between + 1));

  const zs = new Set<number>();
  const steps = 36;
  for (let i = 0; i <= steps; i++) zs.add(c0[0] + ((c3[0] - c0[0]) * i) / steps);
  for (const z of [c1[0], c2[0], ...pillars.flatMap((p) => [p - 0.05, p + 0.05])]) zs.add(z);
  const stations = [...zs].sort((a, b) => a - b);

  const arc = quarter(4, 12);
  const rings: { z: number; points: Pt[] }[] = [];
  for (const z of stations) {
    const base = top(z) - 0.03;
    const height = Math.max(0.001, roofAt(z) - base);
    // From the right window's bottom up and over to the left's.
    const half: Pt[] = arc.map(({ c, s }) => ({ x: bw * c * (1 - 0.2 * s), y: base + height * s, liner: false }));
    const left = half.slice(0, -1).reverse().map((p) => ({ ...p, x: -p.x }));
    rings.push({ z, points: [...half, ...left] });
  }
  const segments = arc.length - 1; // per side
  const pick = (k: number, j: number): Paint => {
    const z = (stations[k] + stations[k + 1]) / 2;
    // Distance round from the window's bottom edge: 0 at the bottom, `segments` at the top middle.
    const from = j < segments ? j : 2 * segments - 1 - j;
    const zone = z < c1[0] ? 'rear' : z > c2[0] ? 'screen' : 'roof';
    if (from >= 8) return zone === 'roof' ? 'paint' : 'glass'; // the top: roof, or windscreen and rear window
    if (from >= (zone === 'rear' ? 7 : 6)) return 'paint'; // pillars and roof rails (slimmer round the rear window)
    if (from === 0) return 'trim'; // the rubber seal along the bottom of the windows
    if (pillars.some((p) => Math.abs(z - p) < 0.05)) return 'paint';
    return 'glass';
  };
  const group = new THREE.Group();
  const outside = loft(rings, false, pick, materials);
  // The same shape seen from inside, dark: the interior through the glass.
  const inside = new THREE.Mesh(outside.geometry, interior);
  inside.castShadow = false;
  group.add(outside, inside);
  return { group, outside };
}

/** Things kept for putting the car back as it was. */
interface Undo {
  hidden: THREE.Object3D[];
  added: THREE.Object3D[];
  swapped: { mesh: THREE.Mesh; geometry: THREE.BufferGeometry }[];
  dispose: { dispose(): void }[];
}

const UNDO = 'photoRealUndo';

/** Turns a game car into a real-looking one. */
export function sculptCar(rig: CarRig): void {
  if (!rig.shape || rig.root.userData[UNDO]) return;
  const shape = rig.shape;
  const { spec } = shape;
  const undo: Undo = { hidden: [], added: [], swapped: [], dispose: [] };
  const keep = <T extends { dispose(): void }>(x: T) => (undo.dispose.push(x), x);

  // The game's own body parts, and their materials (so paint, patterns and glass tint carry over).
  const parts = rig.body.children.filter((c): c is THREE.Mesh => (c as THREE.Mesh).isMesh);
  const shells = parts.filter((p) => p.name === 'shell');
  const glassPart = shells.find((p) => (p.material as THREE.Material).transparent);
  const lowerPart = shells.find((p) => p !== glassPart && p.geometry.type === 'ExtrudeGeometry');
  const roofPart = shells.find((p) => p.geometry.type === 'BoxGeometry');
  if (!glassPart || !lowerPart || !roofPart) return;
  const bodyPaint = lowerPart.material as THREE.Material;
  const roofPaint = roofPart.material as THREE.Material;
  const glass = glassPart.material as THREE.Material;
  const liner = keep(new THREE.MeshStandardMaterial({ color: 0x0b0b0c, roughness: 0.95 }));
  const trim = keep(new THREE.MeshStandardMaterial({ color: 0x0d0e10, roughness: 0.45, metalness: 0.1 }));
  const interior = keep(new THREE.MeshStandardMaterial({ color: 0x141416, roughness: 0.9, side: THREE.BackSide }));
  const chrome = keep(new THREE.MeshStandardMaterial({ color: 0xe8e8e8, metalness: 1, roughness: 0.12 }));

  const add = (object: THREE.Object3D, parent: THREE.Object3D = rig.body) => {
    parent.add(object);
    undo.added.push(object);
    object.traverse((o) => {
      const mesh = o as THREE.Mesh;
      if (mesh.isMesh) keep(mesh.geometry);
    });
    return object;
  };

  // ---------- body and cabin ----------
  for (const part of shells) {
    part.visible = false;
    undo.hidden.push(part);
  }
  const shell = buildShell(shape, { paint: bodyPaint, liner, glass, trim });
  add(shell.mesh);
  const cabin = buildCabin(shape, shell.top, { paint: roofPaint, liner, glass, trim }, interior);
  add(cabin.group);

  // Racing stripes follow the new shape: thin strips laid over the top.
  const stripes = parts.filter((p) => p.name === 'shell-stripe');
  if (stripes.length > 0) {
    const accent = stripes[0].material as THREE.Material;
    for (const stripe of stripes) {
      stripe.visible = false;
      undo.hidden.push(stripe);
    }
    // Over the bonnet, roof and boot (not the glass).
    const [, roofRear, roofFront] = spec.cabin;
    const surfaces = (z: number) => (z > roofRear[0] + 0.03 && z < roofFront[0] - 0.03 ? [shell.mesh, cabin.outside] : [shell.mesh]);
    for (const side of [-1, 1]) add(stripeStrip(shape, surfaces, side * 0.2, 0.22, accent));
  }

  // ---------- lights, grille, mirrors, handles ----------
  for (const part of parts) {
    if (part.name !== 'headlight' && part.name !== 'taillight') continue;
    const head = part.name === 'headlight';
    undo.swapped.push({ mesh: part, geometry: part.geometry });
    part.geometry = keep(head ? new RoundedBoxGeometry(0.4, 0.12, 0.14, 3, 0.05) : new RoundedBoxGeometry(0.4, 0.09, 0.09, 3, 0.035));
  }
  const [hz, hy] = spec.headlight;
  const grille = new THREE.Mesh(new RoundedBoxGeometry(Math.max(0.3, spec.width - 1.0), 0.14, 0.08, 2, 0.03), trim);
  grille.position.set(0, hy - 0.06, Math.min(shape.frontZ - 0.02, hz + BEVEL - 0.01));
  add(grille);
  const grilleBar = new THREE.Mesh(new RoundedBoxGeometry(Math.max(0.3, spec.width - 1.0) + 0.04, 0.02, 0.09, 1, 0.008), chrome);
  grilleBar.position.copy(grille.position).setY(grille.position.y + 0.075);
  add(grilleBar);

  const [c0, c1, c2, c3] = spec.cabin;
  const mirrorZ = c2[0] + (c3[0] - c2[0]) * 0.55;
  const mirrorY = shell.top(mirrorZ) + 0.08;
  const doorZ = (c1[0] + c2[0]) / 2;
  const handleY = spec.bottomY + (shell.top(doorZ) - spec.bottomY) * 0.62;
  for (const side of [-1, 1]) {
    const mirror = new THREE.Group();
    const housing = new THREE.Mesh(new RoundedBoxGeometry(0.16, 0.1, 0.07, 2, 0.03), roofPaint);
    housing.position.set(side * 0.09, 0, 0);
    const stalk = new THREE.Mesh(new THREE.BoxGeometry(0.08, 0.025, 0.03), trim);
    mirror.add(housing, stalk);
    mirror.position.set(side * (spec.cabinWidth / 2 + 0.03), mirrorY, mirrorZ);
    mirror.traverse((o) => ((o as THREE.Mesh).isMesh && ((o as THREE.Mesh).castShadow = true)));
    add(mirror);
    // Door handles, front and rear doors on a long cabin.
    const handles = c1[0] - c0[0] + (c3[0] - c2[0]) + (c2[0] - c1[0]) > 2.4 ? [doorZ + 0.35, doorZ - 0.45] : [doorZ - 0.1];
    for (const z of handles) {
      const handle = new THREE.Mesh(new RoundedBoxGeometry(0.02, 0.025, 0.12, 1, 0.008), chrome);
      handle.position.set(side * (spec.width / 2 + 0.004), handleY, z);
      add(handle);
    }
  }

  // ---------- wheels ----------
  const disc = keep(new THREE.MeshStandardMaterial({ color: 0x8a8d91, metalness: 0.9, roughness: 0.42 }));
  const caliperPaint = keep(new THREE.MeshStandardMaterial({ color: rig.config.accentColor, roughness: 0.35, metalness: 0.2 }));
  for (const wheel of rig.wheels) {
    const tyre = wheel.spin.getObjectByName('tyre') as THREE.Mesh | undefined;
    if (!tyre || tyre.geometry.type !== 'CylinderGeometry') continue;
    const { radiusTop: radius, height: width } = (tyre.geometry as THREE.CylinderGeometry).parameters;
    undo.swapped.push({ mesh: tyre, geometry: tyre.geometry });
    tyre.geometry = keep(tyreGeometry(radius, width, rig.config.wheelStyle === 'offroad' ? 0.56 : 0.66));
    // Inside the rim: a barrel and a backing plate (so you can't see through the wheel), and a brake disc.
    const wheelGroup = tyre.parent!;
    const rimR = radius * (rig.config.wheelStyle === 'offroad' ? 0.56 : 0.66);
    const barrel = new THREE.Mesh(new THREE.CylinderGeometry(rimR * 0.99, rimR * 0.99, width * 0.9, 28, 1, true), trim);
    barrel.rotation.z = Math.PI / 2;
    const back = new THREE.Mesh(new THREE.CircleGeometry(rimR, 28), liner);
    back.rotation.y = -Math.PI / 2;
    back.position.x = -width * 0.3;
    const brake = new THREE.Mesh(new THREE.CylinderGeometry(rimR * 0.78, rimR * 0.78, 0.028, 28), disc);
    brake.rotation.z = Math.PI / 2;
    brake.position.x = width / 2 - 0.09;
    add(barrel, wheelGroup);
    add(back, wheelGroup);
    add(brake, wheelGroup);
    // The caliper grips the disc and steers with the wheel, but doesn't spin.
    const outward = Math.sign(wheel.steer.position.x) || 1;
    const caliper = new THREE.Mesh(new RoundedBoxGeometry(0.05, rimR * 0.5, rimR * 0.32, 2, 0.015), caliperPaint);
    caliper.position.set(outward * (width / 2 - 0.09), rimR * 0.52, wheel.front ? -rimR * 0.3 : rimR * 0.3);
    add(caliper, wheel.steer);
  }

  rig.root.userData[UNDO] = undo;
  const dispose = rig.dispose.bind(rig);
  rig.dispose = () => {
    for (const x of undo.dispose) x.dispose();
    dispose();
  };
}

/** A tyre with rounded sidewalls and a flat tread, around the x axis like the game's own. */
function tyreGeometry(radius: number, width: number, rimShare: number): THREE.BufferGeometry {
  const w = width / 2;
  const rim = radius * rimShare;
  // The tyre's cross-section, from the inner bead round the tread to the outer bead.
  const profile = [
    [rim * 1.01, -w * 0.92],
    [radius * 0.86, -w],
    [radius * 0.96, -w * 0.95],
    [radius * 0.995, -w * 0.75],
    [radius, -w * 0.5],
    [radius, w * 0.5],
    [radius * 0.995, w * 0.75],
    [radius * 0.96, w * 0.95],
    [radius * 0.86, w],
    [rim * 1.01, w * 0.92],
  ].map(([r, y]) => new THREE.Vector2(r, y));
  const geometry = new THREE.LatheGeometry(profile, 40);
  // Lathe turns round y; the game's tyres turn round y too before being laid on their side.
  geometry.computeVertexNormals();
  return geometry;
}

/** A stripe laid over the top of the body, `width` wide and centred `x` from the middle. */
function stripeStrip(shape: CarShape, surfaces: (z: number) => THREE.Mesh[], x: number, width: number, material: THREE.Material): THREE.Mesh {
  // Drop rays straight down onto the shell at a grid of points, and join them up.
  const raycaster = new THREE.Raycaster();
  const down = new THREE.Vector3(0, -1, 0);
  // Work in the car body's coordinates: the parts have no offsets of their own
  // within it, so their world matrices are set to "none" until they're drawn.
  for (const mesh of new Set([...surfaces(shape.rearZ), ...surfaces((shape.spec.cabin[1][0] + shape.spec.cabin[2][0]) / 2)])) mesh.matrixWorld.identity();
  const across = 5;
  const rows: THREE.Vector3[][] = [];
  const length = shape.frontZ - shape.rearZ;
  const steps = 70;
  for (let i = 0; i <= steps; i++) {
    const z = shape.rearZ + 0.02 + ((length - 0.04) * i) / steps;
    const row: THREE.Vector3[] = [];
    for (let j = 0; j <= across; j++) {
      const px = x - width / 2 + (width * j) / across;
      raycaster.set(new THREE.Vector3(px, 5, z), down);
      const hit = raycaster.intersectObjects(surfaces(z), false)[0];
      if (!hit) break;
      row.push(hit.point.add(new THREE.Vector3(0, 0.004, 0)));
    }
    if (row.length === across + 1) rows.push(row);
  }
  const positions: number[] = [];
  const index: number[] = [];
  for (const row of rows) for (const p of row) positions.push(p.x, p.y, p.z);
  for (let i = 0; i < rows.length - 1; i++) {
    // Skip gaps (where the stripe ran off the body).
    if (Math.abs(rows[i + 1][0].z - rows[i][0].z) > (length / steps) * 1.5) continue;
    for (let j = 0; j < across; j++) {
      const a = i * (across + 1) + j;
      const b = a + 1;
      const c = a + across + 1;
      const d = c + 1;
      index.push(a, c, b, b, c, d);
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(index);
  geometry.computeVertexNormals();
  const mesh = new THREE.Mesh(geometry, material);
  mesh.receiveShadow = true;
  return mesh;
}

/** Puts every sculpted car in the scene back as the game built it. */
export function unsculptCars(scene: THREE.Scene): void {
  const roots: THREE.Object3D[] = [];
  scene.traverse((o) => o.userData[UNDO] && roots.push(o));
  for (const root of roots) {
    const undo = root.userData[UNDO] as Undo;
    for (const o of undo.hidden) o.visible = true;
    for (const o of undo.added) o.removeFromParent();
    for (const { mesh, geometry } of undo.swapped) mesh.geometry = geometry;
    for (const x of undo.dispose) x.dispose();
    undo.dispose.length = 0;
    delete root.userData[UNDO];
  }
}
