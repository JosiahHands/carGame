// Time trial: a lap of the city through glowing checkpoint arches, against the clock.
import * as THREE from 'three';

// The lap, in order. The last arch is also the first: the start/finish line.
const COURSE: [number, number][] = [
  [0, 20],
  [0, 120],
  [40, 160],
  [120, 160],
  [160, 100],
  [160, 20],
  [120, -80],
  [40, -80],
  [0, -40],
];
const PASS_RADIUS = 8;
const COUNTDOWN = 3;
const BEST_KEY = 'carGame.bestLap';

export interface TrialEvent {
  kind: 'go' | 'checkpoint' | 'finish';
  time?: number;
  best?: boolean;
}

interface Gate {
  x: number;
  z: number;
  heading: number;
  mesh: THREE.Mesh;
}

export class TimeTrial {
  active = false;
  countdown = 0;
  time = 0;
  best = 0;
  /** Index into the lap of the next checkpoint; the lap ends at COURSE.length. */
  next = 0;
  readonly start: { x: number; z: number; heading: number };
  private group = new THREE.Group();
  private gates: Gate[] = [];
  private arrow: THREE.Mesh;
  private nextMat: THREE.MeshStandardMaterial;
  private laterMat: THREE.MeshStandardMaterial;
  private finishMat: THREE.MeshStandardMaterial;

  constructor(scene: THREE.Scene) {
    try {
      this.best = Number(localStorage.getItem(BEST_KEY)) || 0;
    } catch {
      // ignore
    }
    this.nextMat = new THREE.MeshStandardMaterial({ color: 0xffd23f, emissive: 0xffb000, emissiveIntensity: 1.6 });
    this.laterMat = new THREE.MeshStandardMaterial({ color: 0xffffff, emissive: 0x88aaff, emissiveIntensity: 0.5, transparent: true, opacity: 0.45 });
    this.finishMat = new THREE.MeshStandardMaterial({ color: 0x39ff88, emissive: 0x20d060, emissiveIntensity: 1.6 });
    const archGeo = new THREE.TorusGeometry(7.5, 0.3, 10, 40, Math.PI);
    COURSE.forEach(([x, z], i) => {
      // Each arch faces along the road: from the previous checkpoint to the next.
      const [px, pz] = COURSE[(i - 1 + COURSE.length) % COURSE.length];
      const [nx, nz] = COURSE[(i + 1) % COURSE.length];
      const heading = Math.atan2(nx - px, nz - pz);
      const mesh = new THREE.Mesh(archGeo, this.laterMat);
      mesh.position.set(x, 0, z);
      mesh.rotation.y = heading;
      mesh.visible = false;
      this.group.add(mesh);
      this.gates.push({ x, z, heading, mesh });
    });
    const first = this.gates[0];
    // Start 15 m before the first arch, in the right-hand lane.
    this.start = {
      x: first.x - Math.sin(first.heading) * 15 - Math.cos(first.heading) * 3,
      z: first.z - Math.cos(first.heading) * 15 + Math.sin(first.heading) * 3,
      heading: first.heading,
    };
    // A floating arrow over the car that points to the next checkpoint.
    const arrowShape = new THREE.Shape();
    arrowShape.moveTo(0, 1);
    arrowShape.lineTo(0.7, 0);
    arrowShape.lineTo(0.25, 0);
    arrowShape.lineTo(0.25, -0.8);
    arrowShape.lineTo(-0.25, -0.8);
    arrowShape.lineTo(-0.25, 0);
    arrowShape.lineTo(-0.7, 0);
    arrowShape.closePath();
    const arrowGeo = new THREE.ExtrudeGeometry(arrowShape, { depth: 0.15, bevelEnabled: false }).rotateX(Math.PI / 2);
    this.arrow = new THREE.Mesh(arrowGeo, this.nextMat);
    this.arrow.visible = false;
    this.group.add(this.arrow);
    scene.add(this.group);
  }

  get checkpoints(): number {
    return COURSE.length;
  }

  /** Map markers for the next checkpoint and the one after. */
  markers(): { x: number; z: number; color: string; size: number }[] {
    if (!this.active) return [];
    const out = [];
    for (let i = this.next; i <= Math.min(this.next + 1, COURSE.length); i++) {
      const g = this.gates[i % COURSE.length];
      out.push({ x: g.x, z: g.z, color: i === this.next ? (i === COURSE.length ? '#39ff88' : '#ffd23f') : 'rgba(255,255,255,0.6)', size: i === this.next ? 5 : 3.5 });
    }
    return out;
  }

  begin(): void {
    this.active = true;
    this.countdown = COUNTDOWN;
    this.time = 0;
    // The car starts just before the first arch, so the first one to reach is the second.
    this.next = 1;
    this.showGates();
  }

  stop(): void {
    this.active = false;
    this.countdown = 0;
    for (const g of this.gates) g.mesh.visible = false;
    this.arrow.visible = false;
  }

  /** Call every frame. Returns what happened (go, checkpoint, finish). */
  update(dt: number, carX: number, carY: number, carZ: number): TrialEvent | null {
    if (!this.active) return null;
    let event: TrialEvent | null = null;
    if (this.countdown > 0) {
      this.countdown -= dt;
      if (this.countdown <= 0) {
        this.countdown = 0;
        event = { kind: 'go' };
      }
    } else {
      this.time += dt;
      const g = this.gates[this.next % COURSE.length];
      if (Math.hypot(carX - g.x, carZ - g.z) < PASS_RADIUS) {
        this.next++;
        if (this.next > COURSE.length) {
          const best = this.best === 0 || this.time < this.best;
          if (best) {
            this.best = this.time;
            try {
              localStorage.setItem(BEST_KEY, String(this.time));
            } catch {
              // ignore
            }
          }
          event = { kind: 'finish', time: this.time, best };
          this.stop();
          return event;
        }
        event = { kind: 'checkpoint' };
        this.showGates();
      }
    }

    // Point the arrow at the next arch.
    const g = this.gates[this.next % COURSE.length];
    this.arrow.visible = true;
    this.arrow.position.set(carX, carY + 1.9 + Math.sin(performance.now() / 300) * 0.1, carZ);
    this.arrow.rotation.y = Math.atan2(g.x - carX, g.z - carZ);
    return event;
  }

  /** Shows the next arch brightly and the one after faintly; hides the rest. */
  private showGates(): void {
    this.gates.forEach((g, i) => {
      const nextIndex = this.next % COURSE.length;
      const afterIndex = (this.next + 1) % COURSE.length;
      const isFinish = this.next === COURSE.length && i === 0;
      g.mesh.visible = i === nextIndex || (i === afterIndex && this.next < COURSE.length);
      g.mesh.material = i === nextIndex ? (isFinish ? this.finishMat : this.nextMat) : this.laterMat;
    });
  }
}

/** 83.456 seconds -> "1:23.45". */
export function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds - m * 60;
  return `${m}:${s.toFixed(2).padStart(5, '0')}`;
}
