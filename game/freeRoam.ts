// Things to do in free roam, on every map:
//
//  - Gold wrenches: hidden collectibles, 12 in the city and 8 at each venue.
//    Some sit on the road, some in parks, and some hang in the air past the
//    stunt ramps, so you have to jump for them.
//  - Challenges, each with three stars to win:
//      speed cameras (how fast you pass them),
//      drift zones (drift points scored inside the circle), and
//      jump zones (your longest time in the air from inside the circle).
//
// Stars on challenges only count in your career car (a garage build with
// 1,000 mph tuning would make them meaningless). Wrenches count in any car.
import * as THREE from 'three';
import { canvasTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';
import { HALF_SIZE, ROAD_LINES, type World } from './world';

export type ZoneKind = 'speed' | 'drift' | 'jump';

export interface Zone {
  id: string;
  kind: ZoneKind;
  name: string;
  x: number;
  z: number;
  /** How close counts as passing a camera, or the size of a zone's circle. */
  radius: number;
  /** For 1, 2 and 3 stars: mph, drift points, or seconds in the air. */
  targets: [number, number, number];
}

export interface Wrench {
  id: string;
  x: number;
  y: number;
  z: number;
  mesh: THREE.Object3D;
}

/** What happened this frame, for the game to react to. */
export type FreeRoamEvent =
  | { type: 'wrench'; id: string }
  | { type: 'zone'; zone: Zone; score: number; text: string };

const ZONE_LOOK: Record<ZoneKind, { color: number; icon: string; label: string }> = {
  speed: { color: 0xff3b3b, icon: '📸', label: 'SPEED CAMERA' },
  drift: { color: 0xffd23f, icon: '🌀', label: 'DRIFT ZONE' },
  jump: { color: 0x9b8cff, icon: '🚀', label: 'JUMP ZONE' },
};

export class FreeRoam {
  zones: Zone[] = [];
  wrenches: Wrench[] = [];
  private group = new THREE.Group();
  private goldMat = new THREE.MeshStandardMaterial({ color: 0xffc83d, emissive: 0xb37400, emissiveIntensity: 0.6, metalness: 0.9, roughness: 0.25 });
  private beamMat = new THREE.MeshBasicMaterial({ color: 0xffd23f, transparent: true, opacity: 0.18, depthWrite: false, blending: THREE.AdditiveBlending });
  /** The zone the car is in (drift and jump zones), with the score so far. */
  private inside: { zone: Zone; score: number } | null = null;
  /** Drift points the car already had when it entered a drift zone: they don't count there. */
  private carried = 0;
  /** Cameras passed recently: not counted again until the car has left. */
  private cooldown = new Set<string>();
  private clock = 0;

  constructor(scene: THREE.Scene) {
    scene.add(this.group);
  }

  /** Sets up the wrenches (leaving out ones already found) and challenges for the map just loaded. */
  load(world: World, found: string[]): void {
    this.clear();
    const rand = seededRandom(9001 + world.map.length * 31 + world.map.charCodeAt(0));
    const spots = world.track ? this.trackSpots(world, rand) : this.citySpots(world, rand);
    this.zones = spots.zones;
    spots.wrenches.forEach((p, i) => {
      const id = `${world.map}:${i}`;
      if (found.includes(id)) return;
      const mesh = this.wrenchMesh();
      mesh.position.set(p.x, p.y, p.z);
      this.group.add(mesh);
      this.wrenches.push({ id, x: p.x, y: p.y, z: p.z, mesh });
    });
    for (const z of this.zones) this.group.add(this.zoneMesh(world, z));
  }

  /** Wrench and challenge spots on a race track venue. */
  private trackSpots(world: World, rand: () => number): { wrenches: THREE.Vector3[]; zones: Zone[] } {
    const track = world.track!;
    const half = track.style.width / 2;
    const wrenches: THREE.Vector3[] = [];
    for (let i = 0; i < 8; i++) {
      // Spread round the lap, at different places across the road; every
      // other one just off the edge.
      const along = ((i + 0.3 + rand() * 0.4) / 8) * track.length;
      const offset = i % 2 === 0 ? (rand() - 0.5) * track.style.width * 0.7 : (rand() < 0.5 ? -1 : 1) * (half + 2.5);
      const p = track.pose(along, offset);
      wrenches.push(new THREE.Vector3(p.x, world.heightAt(p.x, p.z) + 1.3, p.z));
    }
    // Speed cameras at the end of the two longest straights; a drift zone on the twistiest stretch.
    const n = track.points.length;
    const straights: { end: number; length: number }[] = [];
    let run = 0;
    for (let k = 0; k < n * 2; k++) {
      const i = k % n;
      if (Math.abs(track.points[i].curve) < 1 / 300) run++;
      else {
        if (run > 10 && k >= n) straights.push({ end: (i - 3 + n) % n, length: run });
        run = 0;
      }
    }
    straights.sort((a, b) => b.length - a.length);
    const zones: Zone[] = [];
    const fast = world.map === 'oval';
    straights.slice(0, 2).forEach((s, k) => {
      const p = track.points[s.end];
      // Longer straights have higher targets. The speedway is flat out all the way round.
      const top = fast ? 1 : Math.min(1.25, 0.8 + (s.length * 4) / 1200);
      const base = fast ? [110, 145, 180] : [70, 95, 125];
      zones.push({ id: `${world.map}:speed${k}`, kind: 'speed', name: `${k === 0 ? 'Main' : 'Back'} straight camera`, x: p.x, z: p.z, radius: half + 3, targets: base.map((v) => Math.round((v * top) / 5) * 5) as [number, number, number] });
    });
    let best = 0;
    let bestAt = 0;
    for (let i = 0; i < n; i++) {
      let sum = 0;
      for (let k = 0; k < 40; k++) sum += Math.abs(track.points[(i + k) % n].curve);
      if (sum > best) {
        best = sum;
        bestAt = (i + 20) % n;
      }
    }
    const d = track.points[bestAt];
    zones.push({ id: `${world.map}:drift`, kind: 'drift', name: 'Drift zone', x: d.x, z: d.z, radius: 70, targets: [1500, 4000, 8000] });
    // The speedway's infield has stunt ramps: a jump zone there.
    if (world.ramps.length) {
      const r = world.ramps[Math.floor(world.ramps.length / 2)];
      zones.push({ id: `${world.map}:jump`, kind: 'jump', name: 'Infield jumps', x: r.x, z: r.z, radius: 60, targets: [1.5, 2.5, 3.5] });
    }
    return { wrenches, zones };
  }

  /** Wrench and challenge spots in the city. */
  private citySpots(world: World, rand: () => number): { wrenches: THREE.Vector3[]; zones: Zone[] } {
    const wrenches: THREE.Vector3[] = [];
    const road = () => ROAD_LINES[1 + Math.floor(rand() * (ROAD_LINES.length - 2))];
    // Three in the air past the stunt ramps: jump for them.
    for (const r of world.ramps.slice(0, 3)) {
      const fx = Math.sin(r.angle);
      const fz = Math.cos(r.angle);
      const lip = -r.length / 2 + r.length * 0.8;
      wrenches.push(new THREE.Vector3(r.x + fx * (lip + 7), r.height + 2.6, r.z + fz * (lip + 7)));
    }
    // Two in parks, among the trees (but not in one).
    for (const b of world.blocks.filter((b) => b.kind === 'park').slice(0, 2)) {
      for (let tries = 0; tries < 50; tries++) {
        const p = new THREE.Vector3(b.x + (rand() - 0.5) * 30, 1.3, b.z + (rand() - 0.5) * 30);
        if (world.circles.every((c) => Math.hypot(p.x - c.x, p.z - c.z) > c.r + 2) || tries === 49) {
          wrenches.push(p);
          break;
        }
      }
    }
    // The rest on the streets, near the edges of the map and down side roads.
    while (wrenches.length < 12) {
      const along = (rand() - 0.5) * (HALF_SIZE * 2 - 40);
      const across = road();
      const p = rand() < 0.5 ? new THREE.Vector3(along, 1.3, across) : new THREE.Vector3(across, 1.3, along);
      if (wrenches.every((w) => w.distanceTo(p) > 60)) wrenches.push(p);
    }
    const stunt = world.blocks.find((b) => b.kind === 'stunt');
    const lot = world.blocks.find((b) => b.kind === 'lot');
    const zones: Zone[] = [
      { id: 'city:speed0', kind: 'speed', name: 'Downtown camera', x: ROAD_LINES[3], z: 200, radius: 7, targets: [60, 80, 100] },
      { id: 'city:speed1', kind: 'speed', name: 'Crosstown camera', x: -200, z: ROAD_LINES[3], radius: 7, targets: [60, 80, 100] },
      { id: 'city:drift', kind: 'drift', name: 'Car park drift zone', x: lot?.x ?? 40, z: lot?.z ?? -40, radius: 45, targets: [1500, 4000, 8000] },
    ];
    if (stunt) zones.push({ id: 'city:jump', kind: 'jump', name: 'Stunt park', x: stunt.x, z: stunt.z, radius: 42, targets: [1.2, 2, 3] });
    return { wrenches, zones };
  }

  private wrenchMesh(): THREE.Object3D {
    const g = new THREE.Group();
    const spinner = new THREE.Group();
    const handle = new THREE.Mesh(new THREE.BoxGeometry(0.22, 1.1, 0.12), this.goldMat);
    const head = new THREE.Mesh(new THREE.TorusGeometry(0.28, 0.1, 8, 16, Math.PI * 1.5), this.goldMat);
    head.position.y = 0.68;
    head.rotation.z = -Math.PI * 0.25;
    spinner.add(handle, head);
    spinner.rotation.z = 0.5;
    spinner.name = 'spinner';
    // A faint beam of light, to spot it from far away.
    const beam = new THREE.Mesh(new THREE.CylinderGeometry(0.5, 0.5, 40, 12, 1, true), this.beamMat);
    beam.position.y = 20;
    g.add(spinner, beam);
    return g;
  }

  /** A glowing ring on the ground (zones) or a post and banner (cameras). */
  private zoneMesh(world: World, z: Zone): THREE.Object3D {
    const look = ZONE_LOOK[z.kind];
    const g = new THREE.Group();
    const y = world.heightAt(z.x, z.z);
    const mat = new THREE.MeshBasicMaterial({ color: look.color, transparent: true, opacity: 0.55, side: THREE.DoubleSide, depthWrite: false });
    if (z.kind === 'speed') {
      const ring = new THREE.Mesh(new THREE.RingGeometry(z.radius - 0.4, z.radius, 40).rotateX(-Math.PI / 2), mat);
      ring.position.y = 0.06;
      g.add(ring);
    } else {
      const ring = new THREE.Mesh(new THREE.RingGeometry(z.radius - 1.2, z.radius, 64).rotateX(-Math.PI / 2), mat);
      ring.position.y = 0.06;
      g.add(ring);
    }
    // A floating sign that always faces the camera.
    const tex = canvasTexture(512, 128, (ctx) => {
      ctx.fillStyle = 'rgba(10, 14, 24, 0.8)';
      ctx.beginPath();
      ctx.roundRect(4, 4, 504, 120, 24);
      ctx.fill();
      ctx.fillStyle = `#${look.color.toString(16).padStart(6, '0')}`;
      ctx.font = 'bold 54px system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(`${look.icon} ${look.label}`, 256, 66);
    });
    const sign = new THREE.Sprite(new THREE.SpriteMaterial({ map: tex, depthWrite: false }));
    sign.scale.set(12, 3, 1);
    sign.position.y = 8;
    g.add(sign);
    g.position.set(z.x, y, z.z);
    return g;
  }

  /**
   * Checks the car against the wrenches and challenges. `drift` is the drift
   * score running now (not yet banked), `airTime` the seconds in the air so far.
   */
  update(dt: number, car: { x: number; y: number; z: number; speed: number; airborne: boolean }, drift: number, airTime: number, reach: number): FreeRoamEvent[] {
    this.clock += dt;
    const events: FreeRoamEvent[] = [];
    // Wrenches spin, bob, and are picked up by driving through them.
    for (let i = this.wrenches.length - 1; i >= 0; i--) {
      const w = this.wrenches[i];
      const spinner = w.mesh.getObjectByName('spinner')!;
      spinner.rotation.y += dt * 2.5;
      spinner.position.y = Math.sin(this.clock * 2 + i) * 0.2;
      // Close enough sideways and in height. The ones in the air need a jump.
      if (Math.hypot(car.x - w.x, car.z - w.z) < reach + 1 && Math.abs(car.y + 0.6 - w.y) < 1.2 + reach * 0.3) {
        w.mesh.removeFromParent();
        this.wrenches.splice(i, 1);
        events.push({ type: 'wrench', id: w.id });
      }
    }

    for (const z of this.zones) {
      const distance = Math.hypot(car.x - z.x, car.z - z.z);
      if (z.kind === 'speed') {
        if (distance < z.radius && !this.cooldown.has(z.id)) {
          this.cooldown.add(z.id);
          const mph = Math.round(car.speed * 2.237);
          events.push({ type: 'zone', zone: z, score: mph, text: `${mph} mph` });
        } else if (distance > z.radius + 20) {
          this.cooldown.delete(z.id);
        }
        continue;
      }
      const inside = distance < z.radius;
      if (inside && !this.inside) {
        this.inside = { zone: z, score: 0 };
        this.carried = drift;
      }
      if (this.inside?.zone !== z) continue;
      if (drift < this.carried) this.carried = 0; // that drift ended (or was lost): a new one counts in full
      if (z.kind === 'jump' && car.airborne) this.inside.score = Math.max(this.inside.score, airTime);
      // A jump from inside the zone counts in full, even if it lands outside.
      if (!inside && z.kind === 'jump' && car.airborne && this.inside.score > 0) continue;
      if (!inside) {
        // Left the zone: that's the result. The part of a drift still going
        // that was scored inside the zone counts.
        const score = z.kind === 'drift' ? this.inside.score + Math.max(0, drift - this.carried) : this.inside.score;
        this.inside = null;
        if (score > 0) events.push({ type: 'zone', zone: z, score, text: z.kind === 'drift' ? `${Math.round(score).toLocaleString()} pts` : `${score.toFixed(1)} s air` });
      }
    }
    return events;
  }

  /** A finished drift while inside a drift zone counts towards it. */
  addDrift(points: number): void {
    if (this.inside?.zone.kind !== 'drift') return;
    // Only the part scored after entering the zone.
    this.inside.score += Math.max(0, points - this.carried);
    this.carried = 0;
  }

  /** What to show on the HUD: the zone the car is in (or close to), and its targets. */
  hud(car: { x: number; z: number }, drift: number): string {
    if (this.inside) {
      const z = this.inside.zone;
      const score = z.kind === 'drift' ? `${Math.round(this.inside.score + Math.max(0, drift - this.carried)).toLocaleString()} pts` : `${this.inside.score.toFixed(1)} s`;
      return `${ZONE_LOOK[z.kind].icon} ${z.name}: ${score} · ${this.targetsText(z)}`;
    }
    for (const z of this.zones) {
      if (Math.hypot(car.x - z.x, car.z - z.z) < z.radius + 60) return `${ZONE_LOOK[z.kind].icon} ${z.name} ahead · ${this.targetsText(z)}`;
    }
    return '';
  }

  targetsText(z: Zone): string {
    const unit = z.kind === 'speed' ? ' mph' : z.kind === 'jump' ? ' s' : '';
    return z.targets.map((t, i) => `${'★'.repeat(i + 1)} ${t.toLocaleString()}${unit}`).join('  ');
  }

  /** Stars for a score on a zone. */
  static stars(z: Zone, score: number): number {
    return z.targets.filter((t) => score >= t).length;
  }

  /** Challenge spots for the minimap, and wrenches when close. */
  mapMarkers(car: { x: number; z: number }): { x: number; z: number; color: string; size: number }[] {
    const out = this.zones.map((z) => ({ x: z.x, z: z.z, color: `#${ZONE_LOOK[z.kind].color.toString(16).padStart(6, '0')}`, size: 5 }));
    for (const w of this.wrenches) if (Math.hypot(car.x - w.x, car.z - w.z) < 90) out.push({ x: w.x, z: w.z, color: '#ffc83d', size: 3 });
    return out;
  }

  setVisible(visible: boolean): void {
    this.group.visible = visible;
  }

  clear(): void {
    this.group.traverse((o) => {
      if (o instanceof THREE.Mesh) o.geometry.dispose();
      if (o instanceof THREE.Sprite) o.material.map?.dispose();
    });
    this.group.clear();
    this.zones = [];
    this.wrenches = [];
    this.inside = null;
    this.cooldown.clear();
  }
}
