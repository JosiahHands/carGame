// A closed race track, used by every map except the city.
//
// The track is a centre line sampled every few metres. Each sample knows which
// way the road runs, how sharply it turns and how much it is banked. From
// that, the track can answer "how high is the road here?" and "is this on the
// tarmac?", give the barrier walls for the car to bump into, and build its own
// meshes: road, kerbs, start line, barriers and catch fence.
//
// Positions across the road are given as an offset from the centre line, in
// metres, with positive offsets to the left (as seen driving round the track).
//
// Tracks are drawn like a turtle drawing: straights and corners in turn (see
// drawTrack). generateLayout makes new ones from a seed number, so there can be
// as many different tracks as the career and sandbox need.
import * as THREE from 'three';
import { canvasTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';

const SPACING = 4; // metres between samples
const GRID_CELL = 20; // size of the lookup grid's cells, metres

/** One sample of the centre line. */
export interface TrackPoint {
  x: number;
  z: number;
  /** Direction of travel, length 1. */
  dx: number;
  dz: number;
  /** How sharply it turns: 1 / radius in metres, positive turning left. */
  curve: number;
  /** Bank angle in radians; positive raises the left side of the road. */
  bank: number;
  /** Metres from the start line. */
  along: number;
}

/** How a track looks and where its barriers are. */
export interface TrackStyle {
  width: number; // of the road, metres
  /** Bank angle in degrees on the straights and in the tightest turns (0 and 0 for a flat track). */
  bank: [number, number];
  /** Red and white kerbs on the corners. */
  kerbs: boolean;
  /** Grass between the road and the barrier on the outside of the track (metres). */
  outsideRunoff: number;
  /** Grass on the inside; null means no barrier on the inside at all. */
  insideRunoff: number | null;
  /** No barriers at all, just grass and trees (a rally stage). */
  noWalls?: boolean;
  /** A flat paved strip along the inside edge, like a speedway's apron (metres). */
  apron: number;
  wallHeight: number;
  /** A wire catch fence on top of the outside wall, this tall (0 for none). */
  fence: number;
  /** The barrier's paint: two colours in blocks along its length. */
  wallColors: [string, string];
}

/** Walls are WALL_THICKNESS thick: the car can't come within half of that of a wall's line. */
export const WALL_THICKNESS = 0.6;

/** A wall the car can hit: a line on the ground from a to b (along the middle of the wall), solid up to `top`. */
export interface Wall {
  ax: number;
  az: number;
  bx: number;
  bz: number;
  top: number;
  // Bounding box, so the car can skip walls that are nowhere near it.
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
}

export function makeWall(ax: number, az: number, bx: number, bz: number, top: number): Wall {
  return { ax, az, bx, bz, top, minX: Math.min(ax, bx), maxX: Math.max(ax, bx), minZ: Math.min(az, bz), maxZ: Math.max(az, bz) };
}

/** Where something sits on the track: a position, the direction of the road and its bank. */
export interface TrackPose {
  x: number;
  y: number;
  z: number;
  heading: number;
  bank: number;
}

/** The closest point of the centre line to somewhere. */
interface Nearest {
  /** Index of the sample at the start of the closest stretch, and how far along it (0..1). */
  index: number;
  t: number;
  offset: number;
  bank: number;
  along: number;
}

export class RaceTrack {
  readonly points: TrackPoint[] = [];
  readonly length: number;
  /** +1 if the track mostly turns left (so the inside is on the left), -1 if right. */
  readonly inside: number;
  /** Offsets of the barriers on the left and right (null where there is none). */
  readonly leftWall: number | null;
  readonly rightWall: number | null;
  private grid = new Map<string, number[]>();

  /** `path` is a smooth closed loop of points, in driving order, starting at the start line. */
  constructor(
    path: { x: number; z: number }[],
    readonly style: TrackStyle,
  ) {
    // Resample the path every SPACING metres.
    const lengths = [0];
    for (let i = 1; i <= path.length; i++) {
      const a = path[i - 1];
      const b = path[i % path.length];
      lengths.push(lengths[i - 1] + Math.hypot(b.x - a.x, b.z - a.z));
    }
    const total = lengths[path.length];
    const count = Math.round(total / SPACING);
    this.length = total;
    let seg = 0;
    for (let i = 0; i < count; i++) {
      const along = (i / count) * total;
      while (lengths[seg + 1] < along) seg++;
      const a = path[seg];
      const b = path[(seg + 1) % path.length];
      const f = (along - lengths[seg]) / (lengths[seg + 1] - lengths[seg] || 1);
      this.points.push({ x: a.x + (b.x - a.x) * f, z: a.z + (b.z - a.z) * f, dx: 0, dz: 1, curve: 0, bank: 0, along });
    }

    // Direction and curvature at each sample, from its neighbours.
    const n = this.points.length;
    const at = (i: number) => this.points[(i + n) % n];
    for (let i = 0; i < n; i++) {
      const p = at(i);
      const dx = at(i + 1).x - at(i - 1).x;
      const dz = at(i + 1).z - at(i - 1).z;
      const d = Math.hypot(dx, dz) || 1;
      p.dx = dx / d;
      p.dz = dz / d;
    }
    const heading = (p: TrackPoint) => Math.atan2(p.dx, p.dz);
    let turning = 0;
    for (let i = 0; i < n; i++) {
      // Heading grows when turning left, so the change in heading is positive for left turns.
      at(i).curve = wrapAngle(heading(at(i + 1)) - heading(at(i - 1))) / (2 * SPACING);
      turning += at(i).curve;
    }
    this.inside = turning >= 0 ? 1 : -1;

    // Banking: steeper the tighter the turn, always raising the outside, then
    // smoothed along the track so it eases in and out of the turns.
    const [straight, turn] = style.bank.map((deg) => (deg * Math.PI) / 180);
    const raw = this.points.map((p) => -this.inside * (straight + (turn - straight) * Math.min(1, Math.abs(p.curve) * 260)));
    const window = 15; // samples either side: 60 m
    for (let i = 0; i < n; i++) {
      let sum = 0;
      for (let k = -window; k <= window; k++) sum += raw[(i + k + n) % n];
      at(i).bank = sum / (window * 2 + 1);
    }

    const edge = style.width / 2;
    const insideWall = style.insideRunoff === null || style.noWalls ? null : edge + style.insideRunoff;
    const outsideWall = style.noWalls ? null : edge + style.outsideRunoff;
    this.leftWall = this.inside > 0 ? insideWall : outsideWall;
    this.rightWall = this.inside > 0 ? outsideWall : insideWall;

    // A lookup grid: each cell lists the samples near it, so finding the
    // closest point of the track only checks a handful of samples.
    const reach = edge + Math.max(style.outsideRunoff, style.insideRunoff ?? 0, style.apron) + 40;
    const cells = Math.ceil(reach / GRID_CELL);
    this.points.forEach((p, i) => {
      const cx = Math.floor(p.x / GRID_CELL);
      const cz = Math.floor(p.z / GRID_CELL);
      for (let gx = cx - cells; gx <= cx + cells; gx++) {
        for (let gz = cz - cells; gz <= cz + cells; gz++) {
          const key = `${gx},${gz}`;
          const list = this.grid.get(key);
          if (list) list.push(i);
          else this.grid.set(key, [i]);
        }
      }
    });
  }

  /** The closest point of the centre line, or null if (x, z) is far from the track. */
  private nearest(x: number, z: number): Nearest | null {
    const candidates = this.grid.get(`${Math.floor(x / GRID_CELL)},${Math.floor(z / GRID_CELL)}`);
    if (!candidates) return null;
    let best = -1;
    let bestDist = Infinity;
    for (const i of candidates) {
      const p = this.points[i];
      const d = (p.x - x) ** 2 + (p.z - z) ** 2;
      if (d < bestDist) {
        bestDist = d;
        best = i;
      }
    }
    // The closest point is on the stretch before or after that sample.
    const n = this.points.length;
    let result: Nearest | null = null;
    let resultDist = Infinity;
    for (const start of [(best - 1 + n) % n, best]) {
      const a = this.points[start];
      const b = this.points[(start + 1) % n];
      const sx = b.x - a.x;
      const sz = b.z - a.z;
      const t = Math.max(0, Math.min(1, ((x - a.x) * sx + (z - a.z) * sz) / (sx * sx + sz * sz)));
      const px = a.x + sx * t;
      const pz = a.z + sz * t;
      const d = (x - px) ** 2 + (z - pz) ** 2;
      if (d < resultDist) {
        resultDist = d;
        // The left of the direction (dx, dz) is (dz, -dx).
        const dx = a.dx + (b.dx - a.dx) * t;
        const dz = a.dz + (b.dz - a.dz) * t;
        const offset = ((x - px) * dz - (z - pz) * dx) / (Math.hypot(dx, dz) || 1);
        result = { index: start, t, offset, bank: a.bank + (b.bank - a.bank) * t, along: a.along + SPACING * t };
      }
    }
    return result;
  }

  /**
   * Height of the road surface at a given offset across a banked road. The
   * low edge sits on the ground; the surface keeps its top height beyond the
   * high edge (up to the wall) and drops to the ground beyond the low edge.
   */
  private surfaceHeight(bank: number, offset: number): number {
    if (bank === 0) return 0;
    const w = this.style.width;
    return Math.tan(Math.abs(bank)) * Math.max(0, Math.min(w, w / 2 + Math.sign(bank) * offset));
  }

  /** Height of the ground at (x, z): above zero only on banked track, inside the outside wall. */
  heightAt(x: number, z: number): number {
    if (this.style.bank[1] === 0) return 0;
    const near = this.nearest(x, z);
    if (!near) return 0;
    // Past the outside wall (the high side) the bank drops away to the ground.
    const outsideWall = (this.inside > 0 ? this.rightWall : this.leftWall) ?? Infinity;
    if (-this.inside * near.offset > outsideWall) return 0;
    return this.surfaceHeight(near.bank, near.offset);
  }

  /** Whether (x, z) is on the road, its kerbs or other tarmac, rather than on grass. */
  isPaved(x: number, z: number): boolean {
    const near = this.nearest(x, z);
    if (!near) return false;
    const s = this.style;
    const kerb = s.kerbs ? 1.4 : 0.3;
    // How far past each edge of the road is still paved: the apron on the
    // inside, and the strip up to the wall on the outside of a speedway.
    const insidePaved = s.width / 2 + Math.max(s.apron, kerb);
    const outsidePaved = s.width / 2 + (s.fence > 0 ? s.outsideRunoff : kerb);
    const towardsInside = this.inside * near.offset;
    return towardsInside <= insidePaved && -towardsInside <= outsidePaved;
  }

  /**
   * Where (x, z) is along the track (metres from the start line) and how far
   * to the left of the centre line, or null if it's nowhere near the track.
   */
  locate(x: number, z: number): { along: number; offset: number } | null {
    const near = this.nearest(x, z);
    return near ? { along: near.along, offset: near.offset } : null;
  }

  /** The sample at `along` metres from the start line (wrapping round the lap). */
  pointAt(along: number): TrackPoint {
    const n = this.points.length;
    const i = Math.floor((((along % this.length) + this.length) % this.length) / this.length * n) % n;
    return this.points[i];
  }

  /** How far (x, z) is from the middle of the road, or Infinity if it's nowhere near the track. */
  distanceFrom(x: number, z: number): number {
    const near = this.nearest(x, z);
    return near ? Math.abs(near.offset) : Infinity;
  }

  /** A spot on the track `along` metres from the start line and `offset` metres to the left of the centre. */
  pose(along: number, offset = 0): TrackPose {
    const n = this.points.length;
    const f = ((((along % this.length) + this.length) % this.length) / this.length) * n;
    const i = Math.floor(f) % n;
    const t = f - Math.floor(f);
    const a = this.points[i];
    const b = this.points[(i + 1) % n];
    const dx = a.dx + (b.dx - a.dx) * t;
    const dz = a.dz + (b.dz - a.dz) * t;
    const d = Math.hypot(dx, dz) || 1;
    const x = a.x + (b.x - a.x) * t + (dz / d) * offset;
    const z = a.z + (b.z - a.z) * t - (dx / d) * offset;
    const bank = a.bank + (b.bank - a.bank) * t;
    return { x, y: this.surfaceHeight(bank, offset), z, heading: Math.atan2(dx, dz), bank };
  }

  /** The barrier walls for the car to hit, as short straight pieces. */
  walls(): Wall[] {
    const walls: Wall[] = [];
    const n = this.points.length;
    const s = this.style;
    for (const side of [this.leftWall, this.rightWall === null ? null : -this.rightWall]) {
      if (side === null) continue;
      const outside = -this.inside * side > 0;
      const middle = side + Math.sign(side) * (WALL_THICKNESS / 2); // along the middle of the wall
      for (let i = 0; i < n; i++) {
        const a = this.offsetPoint(i, middle);
        const b = this.offsetPoint((i + 1) % n, middle);
        // Tall enough that a car on the banking can't climb over it.
        const top = Math.max(a.y, b.y) + s.wallHeight + (outside ? s.fence : 0);
        walls.push(makeWall(a.x, a.z, b.x, b.z, top));
      }
    }
    return walls;
  }

  /** Sample i moved `offset` metres to the left, with the road height there. */
  private offsetPoint(i: number, offset: number): { x: number; y: number; z: number } {
    const p = this.points[i];
    return { x: p.x + p.dz * offset, y: this.surfaceHeight(p.bank, offset), z: p.z - p.dx * offset };
  }

  // ---------- meshes ----------

  /** Builds the road, kerbs, start line, barriers and fence. */
  buildMeshes(): THREE.Group {
    const group = new THREE.Group();
    const s = this.style;
    const edge = s.width / 2;
    const n = this.points.length;
    const all = Array.from({ length: n + 1 }, (_, i) => i % n); // every sample, back round to the first

    // The road, with white lines along both edges.
    const road = trackTexture();
    group.add(this.ribbon(all, -edge, edge, 0.03, new THREE.MeshStandardMaterial({ map: road, roughness: 0.85 }), 10));

    // Speedway apron: a lighter flat strip along the inside, and a strip of
    // tarmac between the road and the outside wall.
    const paved = new THREE.MeshStandardMaterial({ color: 0x55575c, roughness: 0.9 });
    if (s.apron > 0) group.add(this.ribbon(all, this.inside * edge, this.inside * (edge + s.apron), 0.025, paved));
    if (s.fence > 0) group.add(this.ribbon(all, -this.inside * edge, -this.inside * (edge + s.outsideRunoff), 0.025, paved));

    // Kerbs: red and white strips on both edges wherever the road turns tightly.
    if (s.kerbs) {
      const kerb = new THREE.MeshStandardMaterial({ map: kerbTexture(), roughness: 0.7 });
      const tight = this.points.map((p) => Math.abs(p.curve) > 1 / 160);
      // Run each kerb a little (16 m) before and after the corner.
      const wanted = tight.map((_, i) => [-4, -3, -2, -1, 0, 1, 2, 3, 4].some((k) => tight[(i + k + n) % n]));
      for (const run of runsOf(wanted)) {
        for (const side of [1, -1]) group.add(this.ribbon(run, side * edge, side * (edge + 1.4), 0.04, kerb, 3));
      }
    }

    // Start / finish line: a checkered strip across the road.
    const start = this.pose(0);
    const line = new THREE.Mesh(new THREE.PlaneGeometry(s.width, 2.4), new THREE.MeshStandardMaterial({ map: checkerTexture(), roughness: 0.8 }));
    line.rotation.set(-Math.PI / 2, 0, 0);
    const holder = new THREE.Group();
    holder.position.set(start.x, this.surfaceHeight(start.bank, 0) + 0.045, start.z);
    holder.rotation.set(0, start.heading, start.bank, 'YXZ');
    holder.add(line);
    group.add(holder);

    // Starting grid: a white bar for each car, staggered left and right, behind the line.
    const gridMat = new THREE.MeshBasicMaterial({ color: 0xf0f0f0 });
    for (let k = 0; k < 10; k++) {
      const side = k % 2 === 0 ? 1 : -1;
      const p = this.pose(-12 - k * 9, side * edge * 0.45);
      const bar = new THREE.Mesh(new THREE.PlaneGeometry(edge * 0.6, 0.4), gridMat);
      bar.rotation.set(-Math.PI / 2, 0, 0);
      const g = new THREE.Group();
      g.position.set(p.x, p.y + 0.045, p.z);
      g.rotation.set(0, p.heading, p.bank, 'YXZ');
      g.add(bar);
      group.add(g);
    }

    // Barriers, and the catch fence on top of the outside one.
    const wallMat = new THREE.MeshStandardMaterial({ map: wallTexture(s.wallColors), roughness: 0.8 });
    for (const side of [this.leftWall, this.rightWall === null ? null : -this.rightWall]) {
      if (side === null) continue;
      const outside = -this.inside * side > 0;
      group.add(this.wallMesh(side, wallMat));
      if (outside && s.fence > 0) group.add(this.fenceMesh(side));
    }
    group.traverse((o) => {
      if (o instanceof THREE.Mesh) o.receiveShadow = true;
    });
    return group;
  }

  /**
   * A strip following the road surface between two offsets, over the given
   * samples. Texture V runs along the track, one repeat every `metresPerRepeat`.
   */
  private ribbon(indices: number[], from: number, to: number, lift: number, material: THREE.Material, metresPerRepeat = 10): THREE.Mesh {
    const positions: number[] = [];
    const uvs: number[] = [];
    const index: number[] = [];
    let distance = 0;
    indices.forEach((i, k) => {
      if (k > 0) {
        const prev = this.points[indices[k - 1]];
        distance += Math.hypot(this.points[i].x - prev.x, this.points[i].z - prev.z);
      }
      for (const [offset, u] of [[from, 0], [to, 1]]) {
        const p = this.offsetPoint(i, offset);
        positions.push(p.x, p.y + lift, p.z);
        uvs.push(u, distance / metresPerRepeat);
      }
      if (k > 0) {
        const a = (k - 1) * 2;
        index.push(a, a + 1, a + 2, a + 1, a + 3, a + 2);
      }
    });
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    geo.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
    geo.setIndex(index);
    geo.computeVertexNormals();
    // The winding depends on which side `from` is, so draw both faces.
    const mesh = new THREE.Mesh(geo, material);
    (mesh.material as THREE.Material).side = THREE.DoubleSide;
    return mesh;
  }

  /** A barrier wall 0.6 m thick, from the ground up to the road height plus the wall height. */
  private wallMesh(offset: number, material: THREE.Material): THREE.Mesh {
    const n = this.points.length;
    const thick = Math.sign(offset) * WALL_THICKNESS; // away from the road
    const positions: number[] = [];
    const uvs: number[] = [];
    const index: number[] = [];
    let distance = 0;
    for (let k = 0; k <= n; k++) {
      const i = k % n;
      const inner = this.offsetPoint(i, offset);
      const outer = this.offsetPoint(i, offset + thick);
      if (k > 0) {
        const prev = this.offsetPoint((k - 1) % n, offset);
        distance += Math.hypot(inner.x - prev.x, inner.z - prev.z);
      }
      const top = inner.y + this.style.wallHeight;
      const v = distance / 8;
      // Four corners of the wall's cross-section: inner bottom, inner top, outer top, outer bottom.
      positions.push(inner.x, 0, inner.z, inner.x, top, inner.z, outer.x, top, outer.z, outer.x, 0, outer.z);
      uvs.push(v, 0, v, 1, v, 1, v, 0);
      if (k > 0) {
        const a = (k - 1) * 4;
        const b = k * 4;
        for (const [c0, c1] of [[0, 1], [1, 2], [2, 3]]) index.push(a + c0, b + c0, a + c1, a + c1, b + c0, b + c1);
      }
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    geo.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
    geo.setIndex(index);
    geo.computeVertexNormals();
    const mesh = new THREE.Mesh(geo, material);
    (mesh.material as THREE.Material).side = THREE.DoubleSide;
    mesh.castShadow = true;
    return mesh;
  }

  /** A see-through wire fence on top of the wall at `offset`, with posts. */
  private fenceMesh(offset: number): THREE.Group {
    const group = new THREE.Group();
    const n = this.points.length;
    const positions: number[] = [];
    const uvs: number[] = [];
    const index: number[] = [];
    let distance = 0;
    const posts: THREE.Vector3[] = [];
    for (let k = 0; k <= n; k++) {
      const p = this.offsetPoint(k % n, offset);
      if (k > 0) {
        const prev = this.offsetPoint((k - 1) % n, offset);
        distance += Math.hypot(p.x - prev.x, p.z - prev.z);
      }
      const bottom = p.y + this.style.wallHeight;
      positions.push(p.x, bottom, p.z, p.x, bottom + this.style.fence, p.z);
      uvs.push(distance / 4, 0, distance / 4, this.style.fence / 4);
      if (k > 0) {
        const a = (k - 1) * 2;
        index.push(a, a + 2, a + 1, a + 1, a + 2, a + 3);
      }
      if (k % 3 === 0 && k < n) posts.push(new THREE.Vector3(p.x, bottom, p.z));
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    geo.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
    geo.setIndex(index);
    group.add(new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ map: fenceTexture(), transparent: true, alphaTest: 0.3, side: THREE.DoubleSide, depthWrite: false })));
    const postGeo = new THREE.CylinderGeometry(0.08, 0.08, this.style.fence, 6).translate(0, this.style.fence / 2, 0);
    const postMesh = new THREE.InstancedMesh(postGeo, new THREE.MeshStandardMaterial({ color: 0x8a8f96, metalness: 0.6, roughness: 0.4 }), posts.length);
    posts.forEach((p, i) => postMesh.setMatrixAt(i, new THREE.Matrix4().makeTranslation(p.x, p.y, p.z)));
    group.add(postMesh);
    return group;
  }
}

/**
 * Turns a list of straights ([metres]) and corners ([degrees, radius], with
 * positive degrees turning left) into a closed loop of points about 2 m apart,
 * starting at (0, 0) heading along +x. The corners should add up to 360°; any
 * small gap left where the loop should join up is spread along the whole lap.
 */
export function drawTrack(steps: TrackStep[]): { x: number; z: number }[] {
  let x = 0;
  let z = 0;
  let heading = Math.PI / 2; // +x
  const points = [{ x, z }];
  for (const step of steps) {
    if (step.length === 1) {
      const length = step[0];
      const n = Math.max(1, Math.ceil(length / 2));
      for (let i = 1; i <= n; i++) points.push({ x: x + (Math.sin(heading) * length * i) / n, z: z + (Math.cos(heading) * length * i) / n });
      x += Math.sin(heading) * length;
      z += Math.cos(heading) * length;
    } else {
      const [degrees, radius] = step;
      const turn = (degrees * Math.PI) / 180;
      const side = Math.sign(turn); // +1 left, -1 right
      // The corner's centre is `radius` to that side; the left of `heading` is (cos h, -sin h).
      const cx = x + side * radius * Math.cos(heading);
      const cz = z - side * radius * Math.sin(heading);
      const n = Math.max(1, Math.ceil((Math.abs(turn) * radius) / 2));
      for (let i = 1; i <= n; i++) {
        const h = heading + (turn * i) / n;
        points.push({ x: cx - side * radius * Math.cos(h), z: cz + side * radius * Math.sin(h) });
      }
      heading += turn;
      x = points[points.length - 1].x;
      z = points[points.length - 1].z;
    }
  }
  // The last point should be back at the start: share out any gap.
  const gapX = x;
  const gapZ = z;
  points.pop();
  return points.map((p, i) => ({ x: p.x - (gapX * i) / points.length, z: p.z - (gapZ * i) / points.length }));
}

/** A straight ([metres]) or a corner ([degrees, radius], positive degrees turning left). */
export type TrackStep = [number] | [number, number];

/** What kind of track generateLayout should make. */
export interface LayoutOptions {
  /** Roughly how long a lap should be, in metres. */
  length: number;
  /** The tightest and the widest corner radius, in metres. */
  radius: [number, number];
  /** How many corners: fewest and most. */
  corners: [number, number];
  /** Only left turns, like a speedway. */
  leftOnly?: boolean;
  /** How close two different parts of the track may come, centre to centre, in metres. */
  clearance: number;
}

/**
 * Makes a new closed track from a seed number: the same seed always gives the
 * same track. It starts and ends on a long main straight (for the starting
 * grid), with a random mix of corners and straights in between.
 *
 * Random corners and straights hardly ever join up by themselves, so two of
 * the straights are then lengthened or shortened by just the right amount to
 * close the loop. Layouts where that doesn't work, or where the track would
 * cross or come too close to itself, are thrown away and another is tried.
 */
export function generateLayout(seed: number, o: LayoutOptions): TrackStep[] {
  const rand = seededRandom(seed * 7919 + 13);
  const between = (min: number, max: number) => min + (max - min) * rand();
  for (let attempt = 0; attempt < 600; attempt++) {
    const count = Math.round(between(o.corners[0], o.corners[1]));
    // Corner angles that add up to one full turn (360°).
    let angles: number[];
    if (o.leftOnly) {
      const weights = Array.from({ length: count }, () => between(0.6, 1.4));
      const total = weights.reduce((a, b) => a + b, 0);
      angles = weights.map((w) => (w / total) * 360);
    } else {
      angles = Array.from({ length: count }, () => (rand() < 0.62 ? 1 : -1) * between(30, 150));
      // Share what's missing (or extra) out over the left turns.
      const missing = 360 - angles.reduce((a, b) => a + b, 0);
      const lefts = angles.filter((a) => a > 0);
      if (lefts.length === 0) continue;
      angles = angles.map((a) => (a > 0 ? a + missing / lefts.length : a));
    }
    if (angles.some((a) => Math.abs(a) < 20 || Math.abs(a) > 180)) continue;
    // Tighter corners are more common than wide sweepers.
    const radii = angles.map(() => o.radius[0] + (o.radius[1] - o.radius[0]) * rand() ** 1.6);

    // Share the rest of the lap out as straights between the corners.
    const main = Math.min(380, Math.max(110, o.length * between(0.12, 0.18))); // the start/finish straight, split in two halves
    const arcs = angles.reduce((sum, a, i) => sum + (Math.abs(a) * Math.PI * radii[i]) / 180, 0);
    const spare = Math.max(count * 30, o.length - main - arcs);
    const weights = Array.from({ length: count - 1 }, () => between(0.2, 1.8));
    const weightSum = weights.reduce((a, b) => a + b, 0) || 1;
    const straights = weights.map((w) => Math.max(20, (w / weightSum) * spare));

    // Where the loop ends up: it should be back at the start.
    const steps = (): TrackStep[] => {
      const out: TrackStep[] = [[main / 2]];
      angles.forEach((a, i) => {
        out.push([a, radii[i]]);
        if (i < count - 1) out.push([straights[i]]);
      });
      out.push([main / 2]);
      return out;
    };
    const end = endOf(steps());
    // The direction of each in-between straight (heading +x at the start).
    const directions: [number, number][] = [];
    let heading = Math.PI / 2;
    for (let i = 0; i < count - 1; i++) {
      heading += (angles[i] * Math.PI) / 180;
      directions.push([Math.sin(heading), Math.cos(heading)]);
    }
    // Of all the pairs of straights that could close the gap, use the one that
    // changes them least, so the lap stays close to the length asked for.
    let best: { i: number; j: number; ka: number; kb: number } | null = null;
    const longest = Math.max(120, o.length * 0.3);
    for (let i = 0; i < directions.length; i++) {
      for (let j = i + 1; j < directions.length; j++) {
        const [ax, az] = directions[i];
        const [bx, bz] = directions[j];
        const det = ax * bz - bx * az;
        if (Math.abs(det) < 0.5) continue; // nearly parallel: they can't close a gap sideways
        // Solve ka × a + kb × b = -end.
        const ka = (-end.x * bz + end.z * bx) / det;
        const kb = (-ax * end.z + az * end.x) / det;
        const la = straights[i] + ka;
        const lb = straights[j] + kb;
        if (la < 25 || lb < 25 || la > longest || lb > longest) continue;
        if (!best || Math.abs(ka) + Math.abs(kb) < Math.abs(best.ka) + Math.abs(best.kb)) best = { i, j, ka, kb };
      }
    }
    if (!best) continue;
    straights[best.i] += best.ka;
    straights[best.j] += best.kb;
    const layout = steps();
    if (isClearOfItself(drawTrack(layout), o.clearance)) return layout;
  }
  // Every try failed (this shouldn't happen): a plain rounded rectangle.
  return [[200], [90, o.radius[1]], [150], [90, o.radius[1]], [400], [90, o.radius[1]], [150], [90, o.radius[1]], [200]];
}

/** Where a list of steps ends, starting from (0, 0) heading along +x. */
function endOf(steps: TrackStep[]): { x: number; z: number } {
  let x = 0;
  let z = 0;
  let heading = Math.PI / 2;
  for (const step of steps) {
    if (step.length === 1) {
      x += Math.sin(heading) * step[0];
      z += Math.cos(heading) * step[0];
    } else {
      const [degrees, radius] = step;
      const turn = (degrees * Math.PI) / 180;
      const side = Math.sign(turn);
      // Round the corner's centre, from the old heading to the new one.
      const cx = x + side * radius * Math.cos(heading);
      const cz = z - side * radius * Math.sin(heading);
      heading += turn;
      x = cx - side * radius * Math.cos(heading);
      z = cz + side * radius * Math.sin(heading);
    }
  }
  return { x, z };
}

/**
 * Whether a closed loop of points (about 2 m apart) keeps different parts of
 * itself at least `clearance` metres apart. Points close together along the
 * loop are allowed to be close, of course; only parts more than a long way
 * round the loop from each other are checked.
 */
function isClearOfItself(points: { x: number; z: number }[], clearance: number): boolean {
  const step = 4; // check every 4th point: about 8 m apart
  const pts = points.filter((_, i) => i % step === 0);
  const n = pts.length;
  // Two points this many samples apart along the loop count as different parts of the track.
  const apart = Math.ceil((Math.max(clearance * 2.4, 140) / 2) / step);
  for (let i = 0; i < n; i++) {
    for (let j = i + apart; j < n; j++) {
      if (n - (j - i) < apart) continue; // close together the other way round the loop
      if ((pts[i].x - pts[j].x) ** 2 + (pts[i].z - pts[j].z) ** 2 < clearance * clearance) return false;
    }
  }
  return true;
}

/** -π..π */
function wrapAngle(a: number): number {
  return Math.atan2(Math.sin(a), Math.cos(a));
}

/**
 * Splits a loop of flags into runs of neighbouring samples that are set,
 * each one including the sample after it so the strips join up. A run can
 * wrap past the end of the loop.
 */
function runsOf(flags: boolean[]): number[][] {
  const n = flags.length;
  if (flags.every(Boolean)) return [Array.from({ length: n + 1 }, (_, i) => i % n)];
  const start = flags.findIndex((f) => !f); // begin somewhere unset, so no run is split in two
  const runs: number[][] = [];
  let run: number[] = [];
  for (let k = 1; k <= n; k++) {
    const i = (start + k) % n;
    if (flags[i]) run.push(i);
    else if (run.length > 0) {
      run.push(i);
      runs.push(run);
      run = [];
    }
  }
  return runs;
}

// ---------- textures ----------

/** Race track tarmac running along V, with a white line down each edge. */
function trackTexture(): THREE.CanvasTexture {
  const rand = seededRandom(21);
  return canvasTexture(256, 256, (ctx) => {
    ctx.fillStyle = '#2f3135';
    ctx.fillRect(0, 0, 256, 256);
    for (let i = 0; i < 2200; i++) {
      const v = 38 + Math.floor(rand() * 34);
      ctx.fillStyle = `rgba(${v},${v},${v + 4},0.5)`;
      ctx.fillRect(rand() * 256, rand() * 256, 1.5, 1.5);
    }
    // Darker rubbered-in racing line down the middle.
    ctx.fillStyle = 'rgba(10,10,12,0.25)';
    ctx.fillRect(70, 0, 116, 256);
    ctx.fillStyle = '#eeeeee';
    ctx.fillRect(4, 0, 6, 256);
    ctx.fillRect(246, 0, 6, 256);
  }, true);
}

/** Red and white kerb stripes across V. */
function kerbTexture(): THREE.CanvasTexture {
  return canvasTexture(32, 64, (ctx) => {
    ctx.fillStyle = '#d7263d';
    ctx.fillRect(0, 0, 32, 32);
    ctx.fillStyle = '#f4f4f4';
    ctx.fillRect(0, 32, 32, 32);
  }, true);
}

function checkerTexture(): THREE.CanvasTexture {
  const tex = canvasTexture(256, 32, (ctx) => {
    for (let x = 0; x < 16; x++) {
      for (let y = 0; y < 2; y++) {
        ctx.fillStyle = (x + y) % 2 === 0 ? '#111111' : '#f4f4f4';
        ctx.fillRect(x * 16, y * 16, 16, 16);
      }
    }
  });
  return tex;
}

/** Barrier paint: blocks of two colours along U, with a dark base. */
function wallTexture([a, b]: [string, string]): THREE.CanvasTexture {
  return canvasTexture(128, 64, (ctx) => {
    ctx.fillStyle = a;
    ctx.fillRect(0, 0, 64, 64);
    ctx.fillStyle = b;
    ctx.fillRect(64, 0, 64, 64);
    ctx.fillStyle = 'rgba(0,0,0,0.25)';
    ctx.fillRect(0, 56, 128, 8);
  }, true);
}

/** A see-through diamond wire mesh. */
function fenceTexture(): THREE.CanvasTexture {
  return canvasTexture(64, 64, (ctx) => {
    ctx.clearRect(0, 0, 64, 64);
    ctx.strokeStyle = 'rgba(200,205,212,0.95)';
    ctx.lineWidth = 2;
    for (let k = -64; k <= 64; k += 16) {
      ctx.beginPath();
      ctx.moveTo(k, 0);
      ctx.lineTo(k + 64, 64);
      ctx.moveTo(k + 64, 0);
      ctx.lineTo(k, 64);
      ctx.stroke();
    }
    ctx.fillStyle = 'rgba(160,165,170,1)';
    ctx.fillRect(0, 0, 64, 3); // top rail
  }, true);
}
