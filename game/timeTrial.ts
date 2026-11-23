// Time trial (T): a lap of the city through glowing arches, against the clock.
import * as THREE from 'three';

// The arches in the order you drive through them. The last one is the finish
// line, which is also where the lap starts.
const CHECKPOINTS = [
  { x: 0, z: 120 },
  { x: 40, z: 160 },
  { x: 120, z: 160 },
  { x: 160, z: 100 },
  { x: 160, z: 20 },
  { x: 120, z: -80 },
  { x: 40, z: -80 },
  { x: 0, z: -40 },
  { x: 0, z: 20 },
];
/** Where the car waits for the countdown: 15 m before the finish line, in the right-hand lane. */
const START = { x: -3, z: 5, heading: 0 };
const ARCH_RADIUS = 7.5; // wide enough to span the road
const PASS_DISTANCE = 8; // how close to the middle of an arch counts as driving through it
const COUNTDOWN = 3; // seconds
const BEST_KEY = 'carGame.bestLap';

export class TimeTrial {
  active = false;
  /** Seconds left before the start; the car can't move until it reaches 0. */
  countdown = 0;
  /** Lap time so far, in seconds. */
  time = 0;
  /** Best lap ever, in seconds (0 if there isn't one yet). */
  best = 0;
  /** Whether the lap that just finished beat the best lap. */
  newBest = false;
  /** Which arch to drive through next: an index into CHECKPOINTS. */
  next = 0;
  readonly start = START;
  readonly total = CHECKPOINTS.length;
  private arches: THREE.Mesh[] = [];
  private arrow: THREE.Mesh;
  private nextMaterial = new THREE.MeshStandardMaterial({ color: 0xffd23f, emissive: 0xffb000, emissiveIntensity: 1.6 });
  private finishMaterial = new THREE.MeshStandardMaterial({ color: 0x39ff88, emissive: 0x20d060, emissiveIntensity: 1.6 });
  private laterMaterial = new THREE.MeshStandardMaterial({ color: 0xffffff, emissive: 0x88aaff, emissiveIntensity: 0.5, transparent: true, opacity: 0.45 });

  constructor(scene: THREE.Scene) {
    try {
      this.best = Number(localStorage.getItem(BEST_KEY)) || 0;
    } catch {
      // No saved best lap.
    }

    // Each arch is half a ring standing on the road, turned to face along it:
    // the direction from the arch before it to the arch after it.
    const archShape = new THREE.TorusGeometry(ARCH_RADIUS, 0.3, 10, 40, Math.PI);
    for (let i = 0; i < CHECKPOINTS.length; i++) {
      const here = CHECKPOINTS[i];
      const before = CHECKPOINTS[(i - 1 + CHECKPOINTS.length) % CHECKPOINTS.length];
      const after = CHECKPOINTS[(i + 1) % CHECKPOINTS.length];
      const arch = new THREE.Mesh(archShape, this.laterMaterial);
      arch.position.set(here.x, 0, here.z);
      arch.rotation.y = Math.atan2(after.x - before.x, after.z - before.z);
      arch.visible = false;
      scene.add(arch);
      this.arches.push(arch);
    }

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

  /** Starts a new lap with a countdown. The game puts the car at `start` first. */
  begin(): void {
    this.active = true;
    this.countdown = COUNTDOWN;
    this.time = 0;
    this.next = 0;
    this.showArches();
  }

  stop(): void {
    this.active = false;
    this.countdown = 0;
    for (const arch of this.arches) arch.visible = false;
    this.arrow.visible = false;
  }

  /**
   * Call every frame with the car's position. Returns what just happened:
   * 'go' when the countdown ends, 'checkpoint' for each arch, 'finish' at the end.
   */
  update(dt: number, carX: number, carY: number, carZ: number): 'go' | 'checkpoint' | 'finish' | null {
    if (!this.active) return null;
    let event: 'go' | 'checkpoint' | 'finish' | null = null;

    if (this.countdown > 0) {
      this.countdown -= dt;
      if (this.countdown <= 0) {
        this.countdown = 0;
        event = 'go';
      }
    } else {
      this.time += dt;
      const target = CHECKPOINTS[this.next];
      if (Math.hypot(carX - target.x, carZ - target.z) < PASS_DISTANCE) {
        this.next++;
        if (this.next === CHECKPOINTS.length) {
          this.finish();
          return 'finish';
        }
        event = 'checkpoint';
        this.showArches();
      }
    }

    // Keep the arrow above the car (bobbing a little), pointing at the next arch.
    const target = CHECKPOINTS[this.next];
    this.arrow.visible = true;
    this.arrow.position.set(carX, carY + 1.9 + Math.sin(performance.now() / 300) * 0.1, carZ);
    this.arrow.rotation.y = Math.atan2(target.x - carX, target.z - carZ);
    return event;
  }

  private finish(): void {
    this.newBest = this.best === 0 || this.time < this.best;
    if (this.newBest) {
      this.best = this.time;
      try {
        localStorage.setItem(BEST_KEY, String(this.time));
      } catch {
        // The best lap just won't be remembered.
      }
    }
    this.stop();
  }

  /** Shows the next arch brightly (green for the finish line) and the one after faintly. */
  private showArches(): void {
    const isLast = this.next === CHECKPOINTS.length - 1;
    for (let i = 0; i < this.arches.length; i++) {
      const arch = this.arches[i];
      arch.visible = i === this.next || i === this.next + 1;
      arch.material = i === this.next ? (isLast ? this.finishMaterial : this.nextMaterial) : this.laterMaterial;
    }
  }

  /** Minimap dots: the next arch, and a fainter one for the arch after it. */
  mapMarkers(): { x: number; z: number; color: string; size: number }[] {
    if (!this.active) return [];
    const markers = [{ ...CHECKPOINTS[this.next], color: '#ffd23f', size: 5 }];
    if (this.next + 1 < CHECKPOINTS.length) markers.push({ ...CHECKPOINTS[this.next + 1], color: 'rgba(255,255,255,0.6)', size: 3.5 });
    return markers;
  }
}

/** 83.456 seconds -> "1:23.46". */
export function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds - minutes * 60;
  return `${minutes}:${rest.toFixed(2).padStart(5, '0')}`;
}
