// Where the cast lives: named places on the city's blocks (the coffee cart,
// the bank, the diner, the parks...), and the walking routes between them
// along the pavements, crossing the roads at the corners.
import * as THREE from 'three';
import { ROAD_LINES, type World } from '../../game/world';
import type { PlaceKind } from './cast';

/** The pavement loop round each block: this far from the block's center (as the game's pedestrians walk). */
const LOOP = 32;

export interface Place {
  id: string;
  kind: PlaceKind;
  /** What people call it. */
  label: string;
  /** Where people gather, and the corner of its block that routes go through. */
  x: number;
  z: number;
  /** How far people stand from the middle when they gather round. */
  spread: number;
  block: { x: number; z: number };
}

const LABELS: Record<PlaceKind, string> = {
  coffee: 'the coffee cart',
  bank: 'the bank',
  diner: 'the diner',
  gym: 'the gym',
  music: 'the music corner',
  news: 'the newsstand',
  garage: 'the garage',
  park: 'the park',
  stunts: 'the stunt park',
  bakery: 'the bakery',
  library: 'the library',
  office: 'the office',
  market: 'the market',
};

/** A point on the routes: a pavement corner. */
interface Node {
  x: number;
  z: number;
  next: number[];
}

export class CityMap {
  places: Place[] = [];
  private nodes: Node[] = [];
  private props = new THREE.Group();
  /** The blocks with buildings, where people live. */
  private homes: { x: number; z: number }[] = [];

  constructor(scene: THREE.Scene) {
    scene.add(this.props);
  }

  /** Lays out the places and routes for this city (the same every time, as the city is). */
  build(world: World): void {
    this.clear();
    const centres = ROAD_LINES.slice(0, -1).map((c) => c + 40);
    // Routes: the four corners of every block; along the block's sides, and across each road to the next block.
    const index = new Map<string, number>();
    const key = (x: number, z: number) => `${Math.round(x)},${Math.round(z)}`;
    for (const bx of centres) {
      for (const bz of centres) {
        for (const [dx, dz] of [[-1, -1], [1, -1], [1, 1], [-1, 1]]) {
          index.set(key(bx + dx * LOOP, bz + dz * LOOP), this.nodes.length);
          this.nodes.push({ x: bx + dx * LOOP, z: bz + dz * LOOP, next: [] });
        }
      }
    }
    const link = (a: number | undefined, b: number | undefined) => {
      if (a === undefined || b === undefined) return;
      this.nodes[a].next.push(b);
      this.nodes[b].next.push(a);
    };
    for (const bx of centres) {
      for (const bz of centres) {
        const c = (dx: number, dz: number) => index.get(key(bx + dx * LOOP, bz + dz * LOOP));
        link(c(-1, -1), c(1, -1));
        link(c(1, -1), c(1, 1));
        link(c(1, 1), c(-1, 1));
        link(c(-1, 1), c(-1, -1));
        // Across the road to the block on the right and the one in front.
        const gap = 80 - 2 * LOOP;
        link(c(1, -1), index.get(key(bx + LOOP + gap, bz - LOOP)));
        link(c(1, 1), index.get(key(bx + LOOP + gap, bz + LOOP)));
        link(c(-1, 1), index.get(key(bx - LOOP, bz + LOOP + gap)));
        link(c(1, 1), index.get(key(bx + LOOP, bz + LOOP + gap)));
      }
    }

    // Places: the shops along the pavements of the blocks nearest the middle of the city,
    // the two parks nearest the middle, and the stunt park.
    const near = (b: { x: number; z: number }) => Math.hypot(b.x, b.z);
    const cityBlocks = world.blocks.filter((b) => b.kind === 'city').sort((a, b) => near(a) - near(b));
    const parks = world.blocks.filter((b) => b.kind === 'park').sort((a, b) => near(a) - near(b));
    const lots = world.blocks.filter((b) => b.kind === 'lot').sort((a, b) => near(a) - near(b));
    const stunts = world.blocks.find((b) => b.kind === 'stunt');
    // Each shop on a different side of one of the nearest blocks: (block, side).
    const shops: PlaceKind[] = ['coffee', 'bank', 'diner', 'gym', 'music', 'news', 'bakery', 'library', 'office', 'market'];
    const sides: [number, number][] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
    shops.forEach((kind, i) => {
      const block = cityBlocks[Math.floor(i / 2) % cityBlocks.length];
      if (!block) return;
      const [sx, sz] = sides[(i * 3 + Math.floor(i / 2)) % 4];
      this.places.push({ id: kind, kind, label: LABELS[kind], x: block.x + sx * LOOP, z: block.z + sz * LOOP, spread: 2.6, block });
    });
    const garage = lots[0] ?? cityBlocks[4];
    if (garage) this.places.push({ id: 'garage', kind: 'garage', label: LABELS.garage, x: garage.x + 18, z: garage.z - LOOP + 4, spread: 2.4, block: garage });
    parks.slice(0, 2).forEach((park, i) => {
      this.places.push({ id: `park${i}`, kind: 'park', label: i === 0 ? 'the park' : 'the north park', x: park.x - 6 + i * 12, z: park.z + 4, spread: 5, block: park });
    });
    if (stunts) this.places.push({ id: 'stunts', kind: 'stunts', label: LABELS.stunts, x: stunts.x - LOOP + 3, z: stunts.z, spread: 3, block: stunts });
    this.homes = cityBlocks.map((b) => ({ x: b.x, z: b.z }));
    for (const place of this.places) this.props.add(propFor(place));
  }

  clear(): void {
    this.places = [];
    this.nodes = [];
    for (const child of [...this.props.children]) {
      child.removeFromParent();
      child.traverse((o) => {
        const mesh = o as THREE.Mesh;
        if (mesh.isMesh) {
          mesh.geometry.dispose();
          (mesh.material as THREE.Material).dispose();
        }
      });
    }
  }

  setVisible(visible: boolean): void {
    this.props.visible = visible;
  }

  dispose(): void {
    this.clear();
    this.props.removeFromParent();
  }

  place(id: string): Place | undefined {
    return this.places.find((p) => p.id === id);
  }

  placesOf(kind: PlaceKind): Place[] {
    return this.places.filter((p) => p.kind === kind);
  }

  /** The nearest place to a point, and how far it is. */
  nearestPlace(x: number, z: number): { place: Place; distance: number } | null {
    let best: { place: Place; distance: number } | null = null;
    for (const place of this.places) {
      const distance = Math.hypot(place.x - x, place.z - z);
      if (!best || distance < best.distance) best = { place, distance };
    }
    return best;
  }

  /**
   * A walking route from (x, z) to (tx, tz): to the nearest corner, along the
   * pavements and across the roads at the corners, then straight to the end.
   */
  route(x: number, z: number, tx: number, tz: number): THREE.Vector2[] {
    if (this.nodes.length === 0 || Math.hypot(tx - x, tz - z) < 40) return [new THREE.Vector2(tx, tz)];
    const from = this.nearestNode(x, z);
    const to = this.nearestNode(tx, tz);
    // Dijkstra: the graph is small (a few hundred corners).
    const dist = new Array(this.nodes.length).fill(Infinity);
    const prev = new Array(this.nodes.length).fill(-1);
    const done = new Array(this.nodes.length).fill(false);
    dist[from] = 0;
    for (;;) {
      let u = -1;
      for (let i = 0; i < dist.length; i++) if (!done[i] && dist[i] < Infinity && (u < 0 || dist[i] < dist[u])) u = i;
      if (u < 0 || u === to) break;
      done[u] = true;
      for (const v of this.nodes[u].next) {
        const d = dist[u] + Math.hypot(this.nodes[u].x - this.nodes[v].x, this.nodes[u].z - this.nodes[v].z);
        if (d < dist[v]) {
          dist[v] = d;
          prev[v] = u;
        }
      }
    }
    const path: THREE.Vector2[] = [];
    for (let n = to; n >= 0; n = prev[n]) {
      path.unshift(new THREE.Vector2(this.nodes[n].x, this.nodes[n].z));
      if (n === from) break;
    }
    path.push(new THREE.Vector2(tx, tz));
    return path;
  }

  /**
   * Someone's front door: a spot on the pavement of one of the city's blocks,
   * the same for them every time (from their id).
   */
  home(id: string): { x: number; z: number; label: string } | null {
    if (this.homes.length === 0) return null;
    let hash = 2166136261;
    for (const c of id) hash = Math.imul(hash ^ c.charCodeAt(0), 16777619) >>> 0;
    const block = this.homes[hash % this.homes.length];
    const side = (hash >>> 8) % 4;
    const along = (((hash >>> 12) % 1000) / 1000 - 0.5) * 2 * (LOOP - 6);
    const [x, z] = [[along, -LOOP], [LOOP, along], [along, LOOP], [-LOOP, along]][side];
    return { x: block.x + x, z: block.z + z, label: 'home' };
  }

  /** A random spot on a pavement near (x, z), for wandering. */
  wanderSpot(x: number, z: number, random: () => number): THREE.Vector2 {
    const n = this.nodes[this.nearestNode(x + (random() - 0.5) * 200, z + (random() - 0.5) * 200)];
    return new THREE.Vector2(n?.x ?? x, n?.z ?? z);
  }

  private nearestNode(x: number, z: number): number {
    let best = 0;
    let bestD = Infinity;
    this.nodes.forEach((n, i) => {
      const d = (n.x - x) ** 2 + (n.z - z) ** 2;
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    });
    return best;
  }
}

/** A little something at each place so it reads as somewhere: a cart, a sign, benches... */
function propFor(place: Place): THREE.Group {
  const g = new THREE.Group();
  const mat = (color: number, rough = 0.6, metal = 0) => new THREE.MeshStandardMaterial({ color, roughness: rough, metalness: metal });
  const box = (w: number, h: number, d: number, color: number, x: number, y: number, z: number) => {
    const m = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat(color));
    m.position.set(x, y, z);
    m.castShadow = true;
    m.receiveShadow = true;
    g.add(m);
    return m;
  };
  // Props stand just off the gathering spot, towards the block (out of the road).
  const inward = new THREE.Vector2(place.block.x - place.x, place.block.z - place.z).normalize();
  switch (place.kind) {
    case 'coffee': {
      box(1.6, 1, 0.8, 0x7a4a2a, 0, 0.5, 0);
      box(1.7, 0.06, 0.9, 0xf5f0e6, 0, 1.03, 0);
      const pole = new THREE.Mesh(new THREE.CylinderGeometry(0.03, 0.03, 1.4), mat(0xdddddd, 0.3, 0.8));
      pole.position.set(0, 1.7, 0);
      g.add(pole);
      const shade = new THREE.Mesh(new THREE.ConeGeometry(1.3, 0.5, 10), mat(0x2f8f5b));
      shade.position.set(0, 2.45, 0);
      shade.castShadow = true;
      g.add(shade);
      break;
    }
    case 'diner': {
      box(3.2, 2.2, 1.8, 0xe8e2d4, 0, 1.1, 0);
      box(3.3, 0.35, 1.9, 0xd7263d, 0, 2.35, 0);
      box(1.6, 0.7, 0.05, 0x9cc3d9, 0, 1.4, -0.92);
      break;
    }
    case 'bank': {
      for (const s of [-1, 1]) {
        const column = new THREE.Mesh(new THREE.CylinderGeometry(0.22, 0.26, 3.4, 12), mat(0xe8e4da, 0.5));
        column.position.set(s * 1.4, 1.7, 0);
        column.castShadow = true;
        g.add(column);
      }
      box(3.6, 0.4, 0.8, 0xd9d3c4, 0, 3.55, 0);
      box(1.6, 0.3, 0.06, 0xc9a227, 0, 3.05, -0.1);
      break;
    }
    case 'gym':
      box(1.8, 0.5, 0.08, 0xff4d6d, 0, 2.6, 0);
      box(0.1, 2.4, 0.1, 0x333333, -0.85, 1.2, 0);
      box(0.1, 2.4, 0.1, 0x333333, 0.85, 1.2, 0);
      break;
    case 'news':
      box(1.4, 1.6, 1, 0x1d3557, 0, 0.8, 0);
      box(1.5, 0.1, 1.1, 0xffd23f, 0, 1.65, 0);
      break;
    case 'music': {
      const amp = box(0.5, 0.6, 0.35, 0x222222, 0, 0.3, 0);
      amp.rotation.y = 0.4;
      box(0.6, 0.02, 0.4, 0x6b4226, 0.7, 0.01, 0.3); // an open guitar case
      break;
    }
    case 'bakery':
      box(2.4, 1, 0.8, 0xf3e1c7, 0, 0.5, 0); // the counter
      box(2.6, 0.3, 1, 0xe76f51, 0, 2.5, 0.1); // a striped awning
      box(0.08, 2.4, 0.08, 0xdddddd, -1.25, 1.2, 0.5);
      box(0.08, 2.4, 0.08, 0xdddddd, 1.25, 1.2, 0.5);
      for (const x of [-0.7, 0, 0.7]) box(0.4, 0.25, 0.4, 0xc68642, x, 1.13, 0); // loaves
      break;
    case 'library':
      box(3, 2.6, 0.3, 0x6d597a, 0, 1.3, 0);
      box(2.2, 0.4, 0.08, 0xf2e8cf, 0, 2.3, -0.2);
      for (const x of [-0.9, -0.3, 0.3, 0.9]) box(0.12, 0.9, 0.6, [0xd62828, 0x003049, 0xf77f00, 0x2a9d8f][Math.round((x + 0.9) / 0.6)], x, 0.45, -0.5);
      break;
    case 'office':
      box(3.2, 3.2, 0.2, 0x8ecae6, 0, 1.6, 0); // a glass front
      box(1.2, 2.2, 0.06, 0x264653, 0, 1.1, -0.12); // the door
      box(2, 0.3, 0.08, 0x264653, 0, 3, -0.12);
      break;
    case 'market':
      for (const x of [-1.3, 1.3]) {
        box(1.8, 0.9, 0.9, 0x8a5a33, x, 0.45, 0); // a stall
        box(1.6, 0.25, 0.7, x < 0 ? 0xe63946 : 0x80b918, x, 1.02, 0); // fruit and veg
        box(2, 0.08, 1.2, 0xf1faee, x, 2.2, 0); // its canopy
        box(0.06, 1.3, 0.06, 0xbbbbbb, x - 0.9, 1.55, 0.5);
        box(0.06, 1.3, 0.06, 0xbbbbbb, x + 0.9, 1.55, 0.5);
      }
      break;
    case 'garage':
      box(4, 3, 0.2, 0x55595f, 0, 1.5, 0);
      box(2.4, 0.5, 0.1, 0xf46036, 0, 3.2, -0.1);
      box(0.8, 0.9, 0.5, 0xd7263d, 1.6, 0.45, -0.8); // a toolbox
      break;
    case 'park':
    case 'stunts':
      for (const s of [-1, 1]) {
        // A bench: seat, back and legs.
        const bench = new THREE.Group();
        const wood = mat(0x8a5a33, 0.7);
        const seat = new THREE.Mesh(new THREE.BoxGeometry(1.6, 0.06, 0.45), wood);
        seat.position.y = 0.45;
        const back = new THREE.Mesh(new THREE.BoxGeometry(1.6, 0.4, 0.05), wood);
        back.position.set(0, 0.7, 0.22);
        bench.add(seat, back);
        for (const lx of [-0.7, 0.7]) {
          const leg = new THREE.Mesh(new THREE.BoxGeometry(0.06, 0.45, 0.4), mat(0x333333, 0.4, 0.6));
          leg.position.set(lx, 0.22, 0);
          bench.add(leg);
        }
        bench.traverse((o) => ((o as THREE.Mesh).isMesh && ((o as THREE.Mesh).castShadow = true)));
        bench.position.set(s * 2.4, 0, 1.6);
        bench.rotation.y = s * 0.5 + Math.PI;
        g.add(bench);
      }
      break;
  }
  g.position.set(place.x + inward.x * (place.kind === 'park' || place.kind === 'stunts' ? 0 : 2.6), 0, place.z + inward.y * (place.kind === 'park' || place.kind === 'stunts' ? 0 : 2.6));
  // Face the pavement.
  g.rotation.y = Math.atan2(-inward.x, -inward.y);
  return g;
}
