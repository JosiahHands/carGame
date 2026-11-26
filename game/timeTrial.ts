// Time trial (T): a lap through glowing arches, against the clock. Each map has
// its own lap (see Course in world.ts) and its own best lap. The best lap is
// also recorded, so next time a see-through "ghost" car races you round it.
import * as THREE from 'three';
import type { MapName } from './venues';
import type { Course } from './world';

const COUNTDOWN = 3; // seconds
const GHOST_RATE = 20; // ghost positions recorded per second
/** Values stored per ghost position: x, y, z, heading, pitch, roll. */
const GHOST_VALUES = 6;

/** Where the car is and which way it points: what the ghost records. */
export interface CarPose {
  x: number;
  y: number;
  z: number;
  heading: number;
  pitch: number;
  groundRoll: number;
}

/** Saved best lap time for a map, in seconds (0 if there isn't one). */
export function bestLapFor(map: MapName): number {
  try {
    return Number(localStorage.getItem(bestKey(map))) || 0;
  } catch {
    return 0;
  }
}

// The city kept the key it had before there were other maps, so old best laps still count.
const bestKey = (map: MapName) => (map === 'city' ? 'carGame.bestLap' : `carGame.bestLap.${map}`);
const ghostKey = (map: MapName) => `carGame.ghost.${map}`;

export class TimeTrial {
  active = false;
  /** Seconds left before the start; the car can't move until it reaches 0. */
  countdown = 0;
  /** Lap time so far, in seconds. */
  time = 0;
  /** Best lap ever on this map, in seconds (0 if there isn't one yet). */
  best = 0;
  /** Whether the lap that just finished beat the best lap. */
  newBest = false;
  /** Which arch to drive through next: an index into the course's checkpoints. */
  next = 0;
  total = 0;
  start = { x: 0, z: 0, heading: 0 };
  private course: Course | null = null;
  private map: MapName = 'city';
  private scene: THREE.Scene;
  private arches: THREE.Mesh[] = [];
  private archShape: THREE.TorusGeometry | null = null;
  private arrow: THREE.Mesh;
  private nextMaterial = new THREE.MeshStandardMaterial({ color: 0xffd23f, emissive: 0xffb000, emissiveIntensity: 1.6 });
  private finishMaterial = new THREE.MeshStandardMaterial({ color: 0x39ff88, emissive: 0x20d060, emissiveIntensity: 1.6 });
  private laterMaterial = new THREE.MeshStandardMaterial({ color: 0xffffff, emissive: 0x88aaff, emissiveIntensity: 0.5, transparent: true, opacity: 0.45 });
  /** Where the car was last frame (NaN before the first frame of a lap). */
  private lastX = NaN;
  private lastZ = NaN;
  /** This lap so far, and the best lap's recording: GHOST_VALUES numbers per position. */
  private recording: number[] = [];
  private ghost: number[] | null = null;
  private recordTimer = 0;
  /** Where the car was last frame, to work out where it was at each recording tick in between. */
  private lastPose: CarPose | null = null;

  constructor(scene: THREE.Scene) {
    this.scene = scene;
    // A flat arrow floating over the car, pointing at the next arch. It is
    // drawn pointing up the screen, then laid flat so it points along +z.
    const shape = new THREE.Shape();
    shape.moveTo(0, 1);
    shape.lineTo(0.7, 0);
    shape.lineTo(0.25, 0);
    shape.lineTo(0.25, -0.8);
    shape.lineTo(-0.25, -0.8);
    shape.lineTo(-0.25, 0);
    shape.lineTo(-0.7, 0);
    shape.closePath();
    const arrowShape = new THREE.ExtrudeGeometry(shape, { depth: 0.15, bevelEnabled: false }).rotateX(Math.PI / 2);
    this.arrow = new THREE.Mesh(arrowShape, this.nextMaterial);
    this.arrow.visible = false;
    scene.add(this.arrow);
  }

  /** Sets up the arches for a map's lap, and loads its best lap and ghost. */
  setCourse(course: Course, map: MapName): void {
    this.stop();
    this.course = course;
    this.map = map;
    this.total = course.checkpoints.length;
    this.start = course.start;
    this.best = bestLapFor(map);
    try {
      const saved = JSON.parse(localStorage.getItem(ghostKey(map)) ?? 'null');
      this.ghost = Array.isArray(saved) && saved.every((v) => typeof v === 'number') ? saved : null;
    } catch {
      this.ghost = null;
    }

    for (const arch of this.arches) arch.removeFromParent();
    this.archShape?.dispose();
    // Each arch is half a ring standing on the road, turned to face along it:
    // the direction from the arch before it to the arch after it, unless the course says.
    this.archShape = new THREE.TorusGeometry(course.radius, 0.3, 10, 40, Math.PI);
    const points = course.checkpoints;
    this.arches = points.map((here, i) => {
      const before = points[(i - 1 + points.length) % points.length];
      const after = points[(i + 1) % points.length];
      const arch = new THREE.Mesh(this.archShape!, this.laterMaterial);
      arch.position.set(here.x, here.y, here.z);
      arch.rotation.set(0, here.heading ?? Math.atan2(after.x - before.x, after.z - before.z), here.bank ?? 0, 'YXZ');
      arch.visible = false;
      this.scene.add(arch);
      return arch;
    });
  }

  /** Starts a new lap with a countdown. The game puts the car at `start` first. */
  begin(): void {
    this.active = true;
    this.countdown = COUNTDOWN;
    this.time = 0;
    this.next = 0;
    this.lastX = this.lastZ = NaN;
    this.recording = [];
    this.recordTimer = 0;
    this.lastPose = null;
    this.showArches();
  }

  stop(): void {
    this.active = false;
    this.countdown = 0;
    for (const arch of this.arches) arch.visible = false;
    this.arrow.visible = false;
  }

  /** How many of the five start lights are on: one more every 0.6 s of the countdown, then all off for GO. */
  get startLights(): number {
    if (!this.active || this.countdown <= 0) return 0;
    return Math.min(5, Math.floor((COUNTDOWN - this.countdown) / (COUNTDOWN / 5)) + 1);
  }

  /** Whether there is a ghost to race on this map. */
  get hasGhost(): boolean {
    return this.ghost !== null && this.ghost.length >= GHOST_VALUES * 2;
  }

  /**
   * Where the ghost of the best lap is now, or null when it isn't racing
   * (no best lap yet, before the start, or after it finished).
   */
  ghostPose(): CarPose | null {
    const g = this.ghost;
    if (!this.active || this.countdown > 0 || !g) return null;
    const f = this.time * GHOST_RATE;
    const i = Math.floor(f);
    if ((i + 1) * GHOST_VALUES >= g.length) return null;
    const t = f - i;
    const value = (k: number) => g[i * GHOST_VALUES + k] + (g[(i + 1) * GHOST_VALUES + k] - g[i * GHOST_VALUES + k]) * t;
    // Turn the short way round between the two headings.
    const h0 = g[i * GHOST_VALUES + 3];
    const turn = Math.atan2(Math.sin(g[(i + 1) * GHOST_VALUES + 3] - h0), Math.cos(g[(i + 1) * GHOST_VALUES + 3] - h0));
    return { x: value(0), y: value(1), z: value(2), heading: h0 + turn * t, pitch: value(4), groundRoll: value(5) };
  }

  /**
   * Call every frame with the car. Returns what just happened: 'go' when the
   * countdown ends, 'checkpoint' for each arch, 'finish' at the end.
   */
  update(dt: number, car: CarPose): 'go' | 'checkpoint' | 'finish' | null {
    if (!this.active || !this.course) return null;
    let event: 'go' | 'checkpoint' | 'finish' | null = null;
    const points = this.course.checkpoints;

    if (this.countdown > 0) {
      this.countdown -= dt;
      if (this.countdown <= 0) {
        this.countdown = 0;
        event = 'go';
        this.record(car, car, 1);
      }
    } else {
      this.time += dt;
      // Record the lap for the ghost, exactly GHOST_RATE times a second. A
      // tick usually falls between two frames, so record where the car was
      // at that moment, part way from last frame's position to this one.
      const last = this.lastPose ?? car;
      this.recordTimer += dt;
      while (this.recordTimer >= 1 / GHOST_RATE) {
        this.recordTimer -= 1 / GHOST_RATE;
        this.record(last, car, 1 - this.recordTimer / dt);
      }
      // Check the whole path since last frame, not just where the car is now:
      // a very fast car can jump right past an arch between two frames.
      const fromX = Number.isNaN(this.lastX) ? car.x : this.lastX;
      const fromZ = Number.isNaN(this.lastZ) ? car.z : this.lastZ;
      const target = points[this.next];
      if (distanceToPath(target.x, target.z, fromX, fromZ, car.x, car.z) < this.course.radius + 0.5) {
        this.next++;
        if (this.next === points.length) {
          // One last position at the next tick, carrying on past the line at
          // the same speed, so the ghost reaches the finish too.
          // (Capped, in case this frame was unusually short.)
          this.record(last, car, dt > 0 ? Math.min(3, 1 + (1 / GHOST_RATE - this.recordTimer) / dt) : 1);
          this.finish();
          return 'finish';
        }
        event = 'checkpoint';
        this.showArches();
      }
    }
    this.lastX = car.x;
    this.lastZ = car.z;
    this.lastPose = { x: car.x, y: car.y, z: car.z, heading: car.heading, pitch: car.pitch, groundRoll: car.groundRoll };

    // Keep the arrow above the car (bobbing a little), pointing at the next arch.
    const target = points[this.next];
    this.arrow.visible = true;
    this.arrow.position.set(car.x, car.y + 1.9 + Math.sin(performance.now() / 300) * 0.1, car.z);
    this.arrow.rotation.y = Math.atan2(target.x - car.x, target.z - car.z);
    return event;
  }

  /** Records the car's pose a share `t` of the way from `from` to `to` (more than 1 carries on past `to`). */
  private record(from: CarPose, to: CarPose, t: number): void {
    const at = (a: number, b: number) => Math.round((a + (b - a) * t) * 100) / 100; // centimeters are plenty, and keep the save small
    // Turn the short way round between the two headings.
    const turn = Math.atan2(Math.sin(to.heading - from.heading), Math.cos(to.heading - from.heading));
    this.recording.push(at(from.x, to.x), at(from.y, to.y), at(from.z, to.z), at(from.heading, from.heading + turn), at(from.pitch, to.pitch), at(from.groundRoll, to.groundRoll));
  }

  /** Map dots: the next arch, a fainter one for the arch after it, and the ghost. */
  mapMarkers(): { x: number; z: number; color: string; size: number }[] {
    if (!this.active || !this.course) return [];
    const points = this.course.checkpoints;
    const markers = [{ x: points[this.next].x, z: points[this.next].z, color: '#ffd23f', size: 5 }];
    if (this.next + 1 < points.length) markers.push({ x: points[this.next + 1].x, z: points[this.next + 1].z, color: 'rgba(255,255,255,0.6)', size: 3.5 });
    const ghost = this.ghostPose();
    if (ghost) markers.push({ x: ghost.x, z: ghost.z, color: '#9fd3ff', size: 3.5 });
    return markers;
  }

  private finish(): void {
    this.newBest = this.best === 0 || this.time < this.best;
    if (this.newBest) {
      this.best = this.time;
      this.ghost = this.recording;
      try {
        localStorage.setItem(bestKey(this.map), String(this.time));
        localStorage.setItem(ghostKey(this.map), JSON.stringify(this.recording));
      } catch {
        // The best lap just won't be remembered.
      }
    }
    this.stop();
  }

  /** Shows the next arch brightly (green for the finish line) and the one after faintly. */
  private showArches(): void {
    const isLast = this.next === this.arches.length - 1;
    for (let i = 0; i < this.arches.length; i++) {
      const arch = this.arches[i];
      arch.visible = i === this.next || i === this.next + 1;
      arch.material = i === this.next ? (isLast ? this.finishMaterial : this.nextMaterial) : this.laterMaterial;
    }
  }
}

/** How close the point (px, pz) comes to the straight line from (ax, az) to (bx, bz). */
function distanceToPath(px: number, pz: number, ax: number, az: number, bx: number, bz: number): number {
  const dx = bx - ax;
  const dz = bz - az;
  const lengthSq = dx * dx + dz * dz;
  // How far along the line the closest point is, from 0 (start) to 1 (end).
  const t = lengthSq > 0 ? Math.max(0, Math.min(1, ((px - ax) * dx + (pz - az) * dz) / lengthSq)) : 0;
  return Math.hypot(px - (ax + dx * t), pz - (az + dz * t));
}

/** 83.456 seconds -> "1:23.46". */
export function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds - minutes * 60;
  return `${minutes}:${rest.toFixed(2).padStart(5, '0')}`;
}
