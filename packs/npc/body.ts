// A cast member's body: a jointed figure (head, body, arms and legs that
// swing from the shoulders and hips) dressed as they are in cast.ts, with
// their hair, hat, glasses and the thing they carry. It walks, talks with
// its hands, cheers, waves and dives out of the way of cars.
import * as THREE from 'three';
import type { Look } from './cast';

export type Action = 'stand' | 'walk' | 'run' | 'talk' | 'listen' | 'cheer' | 'wave' | 'dive';

export class Body {
  readonly root = new THREE.Group();
  /** The point a name tag sits over. */
  readonly top: number;
  private hips = new THREE.Group();
  private chest = new THREE.Group();
  private head = new THREE.Group();
  private arms: THREE.Group[] = [];
  private legs: THREE.Group[] = [];
  private materials: THREE.Material[] = [];
  private geometries: THREE.BufferGeometry[] = [];
  private phase = Math.random() * 10;
  private standHeight: number;

  constructor(look: Look) {
    const s = look.height / 1.8; // everything scales with their height
    const w = look.build;
    const mat = (color: number, rough = 0.75, metal = 0) => {
      const m = new THREE.MeshStandardMaterial({ color, roughness: rough, metalness: metal });
      this.materials.push(m);
      return m;
    };
    const skin = mat(look.skin, 0.6);
    const shirt = mat(look.shirt, 0.8);
    const trousers = mat(look.trousers, 0.85);
    const shoes = mat(look.shoes, 0.5);
    const hair = mat(look.hair, 0.9);
    const part = (geometry: THREE.BufferGeometry, material: THREE.Material, parent: THREE.Object3D, x = 0, y = 0, z = 0) => {
      this.geometries.push(geometry);
      const mesh = new THREE.Mesh(geometry, material);
      mesh.position.set(x, y, z);
      mesh.castShadow = true;
      parent.add(mesh);
      return mesh;
    };

    // Legs hang from the hips; each is a trouser leg with a shoe.
    const legLength = 0.86 * s;
    this.standHeight = legLength + 0.06 * s;
    this.hips.position.y = this.standHeight;
    this.root.add(this.hips);
    for (const side of [-1, 1]) {
      const leg = new THREE.Group();
      leg.position.set(side * 0.11 * w, 0, 0);
      part(new THREE.CapsuleGeometry(0.075 * w, legLength - 0.15, 4, 10), trousers, leg, 0, -legLength / 2, 0);
      part(new THREE.BoxGeometry(0.13, 0.08, 0.27), shoes, leg, 0, -legLength - 0.02 * s, 0.05);
      this.hips.add(leg);
      this.legs.push(leg);
    }

    // The body, leaning a little as they walk.
    this.hips.add(this.chest);
    const torsoHeight = 0.58 * s;
    const torso = part(new THREE.CapsuleGeometry(0.19 * w, torsoHeight - 0.3, 6, 14), shirt, this.chest, 0, torsoHeight / 2 + 0.02, 0);
    torso.scale.set(1.05, 1, 0.68);
    // A collar and a belt line, so the shirt reads as a shirt.
    part(new THREE.TorusGeometry(0.075, 0.025, 6, 16), shirt, this.chest, 0, torsoHeight + 0.03, 0).rotation.x = Math.PI / 2;
    part(new THREE.CylinderGeometry(0.19 * w, 0.19 * w, 0.05, 16), trousers, this.chest, 0, 0.03, 0).scale.z = 0.7;

    // The head on a neck.
    this.head.position.y = torsoHeight + 0.1 * s;
    this.chest.add(this.head);
    part(new THREE.CylinderGeometry(0.05, 0.06, 0.1, 10), skin, this.head, 0, 0.02, 0);
    const headR = 0.12 * s;
    const skull = part(new THREE.SphereGeometry(headR, 20, 16), skin, this.head, 0, 0.14 * s, 0);
    skull.scale.set(0.92, 1.05, 1);
    // A nose and eyes, so they have a face that points somewhere.
    part(new THREE.ConeGeometry(0.02, 0.05, 6), skin, this.head, 0, 0.13 * s, headR * 0.98).rotation.x = Math.PI / 2;
    const eyes = mat(0x1a1a1a, 0.3);
    for (const side of [-1, 1]) part(new THREE.SphereGeometry(0.014, 8, 6), eyes, this.head, side * 0.042, 0.16 * s, headR * 0.86);
    this.dressHair(look, hair, part, headR, s);
    if (look.glasses) {
      const frame = mat(0x111111, 0.3, 0.5);
      for (const side of [-1, 1]) part(new THREE.TorusGeometry(0.03, 0.006, 6, 14), frame, this.head, side * 0.045, 0.16 * s, headR * 0.93);
      part(new THREE.BoxGeometry(0.03, 0.006, 0.006), frame, this.head, 0, 0.16 * s, headR * 0.95);
      if (look.hat === 'police') {
        // Sunglasses: dark lenses.
        const lens = mat(0x050505, 0.1, 0.6);
        for (const side of [-1, 1]) part(new THREE.CircleGeometry(0.03, 12), lens, this.head, side * 0.045, 0.16 * s, headR * 0.94);
      }
    }
    this.dressHat(look, mat, part, headR, s);

    // Arms hang from the shoulders, with hands.
    for (const side of [-1, 1]) {
      const arm = new THREE.Group();
      arm.position.set(side * 0.25 * w, torsoHeight - 0.04, 0);
      const armLength = 0.6 * s;
      part(new THREE.CapsuleGeometry(0.055 * w, armLength - 0.14, 4, 10), shirt, arm, 0, -armLength / 2 + 0.02, 0);
      part(new THREE.SphereGeometry(0.055, 10, 8), skin, arm, 0, -armLength + 0.02, 0);
      this.chest.add(arm);
      this.arms.push(arm);
    }
    this.dressExtra(look, mat, part, torsoHeight, s);
    this.top = this.hips.position.y + this.head.position.y + 0.32 * s + (look.hat === 'none' ? 0 : 0.08);
  }

  private dressHair(look: Look, hair: THREE.Material, part: Part, r: number, s: number): void {
    const y = 0.14 * s;
    switch (look.hairStyle) {
      case 'short':
        part(new THREE.SphereGeometry(r * 1.04, 20, 12, 0, Math.PI * 2, 0, Math.PI * 0.42), hair, this.head, 0, y + 0.005, -0.01);
        break;
      case 'long': {
        part(new THREE.SphereGeometry(r * 1.06, 20, 12, 0, Math.PI * 2, 0, Math.PI * 0.5), hair, this.head, 0, y, -0.01);
        const back = part(new THREE.CapsuleGeometry(r * 0.85, 0.18, 4, 12), hair, this.head, 0, y - 0.12, -0.05);
        back.scale.z = 0.55;
        break;
      }
      case 'curly':
        for (let i = 0; i < 9; i++) {
          const a = (i / 9) * Math.PI * 2;
          part(new THREE.SphereGeometry(r * 0.42, 10, 8), hair, this.head, Math.cos(a) * r * 0.7, y + r * 0.62 + (i % 2) * 0.02, Math.sin(a) * r * 0.62 - 0.02);
        }
        break;
      case 'bun':
        part(new THREE.SphereGeometry(r * 1.04, 20, 12, 0, Math.PI * 2, 0, Math.PI * 0.45), hair, this.head, 0, y, -0.01);
        part(new THREE.SphereGeometry(r * 0.42, 12, 10), hair, this.head, 0, y + r * 0.95, -r * 0.45);
        break;
      case 'spiky':
        for (let i = 0; i < 7; i++) {
          const spike = part(new THREE.ConeGeometry(0.035, 0.12, 6), hair, this.head, (i - 3) * 0.035, y + r * 0.95, -0.01 + (i % 2) * 0.03);
          spike.rotation.z = (i - 3) * -0.18;
        }
        part(new THREE.SphereGeometry(r * 1.03, 18, 10, 0, Math.PI * 2, 0, Math.PI * 0.38), hair, this.head, 0, y, -0.01);
        break;
      case 'bald':
        // A fringe around the back and sides.
        part(new THREE.TorusGeometry(r * 0.92, 0.035, 6, 20, Math.PI * 1.2), hair, this.head, 0, y - 0.01, -0.01).rotation.set(Math.PI / 2, 0, -Math.PI * 0.1 + Math.PI);
        break;
    }
  }

  private dressHat(look: Look, mat: Mat, part: Part, r: number, s: number): void {
    const y = 0.14 * s + r * 0.6;
    switch (look.hat) {
      case 'police': {
        const navy = mat(0x141c30, 0.5);
        part(new THREE.CylinderGeometry(r * 1.12, r * 1.0, 0.1, 18), navy, this.head, 0, y + 0.03, 0);
        const peak = part(new THREE.CylinderGeometry(r * 0.9, r * 0.9, 0.015, 18, 1, false, -Math.PI / 2, Math.PI), mat(0x050505, 0.2), this.head, 0, y - 0.02, 0.05);
        peak.scale.set(1, 1, 0.8);
        part(new THREE.BoxGeometry(0.05, 0.04, 0.01), mat(0xd4af37, 0.3, 0.9), this.head, 0, y + 0.04, r * 1.05);
        break;
      }
      case 'beanie':
        part(new THREE.SphereGeometry(r * 1.1, 18, 10, 0, Math.PI * 2, 0, Math.PI * 0.5), mat(0x2b2b2b), this.head, 0, y - 0.04, 0);
        break;
      case 'cap': {
        const cap = mat(0xd7263d, 0.7);
        part(new THREE.SphereGeometry(r * 1.06, 18, 10, 0, Math.PI * 2, 0, Math.PI * 0.45), cap, this.head, 0, y - 0.05, 0);
        part(new THREE.BoxGeometry(0.16, 0.012, 0.12), cap, this.head, 0, y - 0.04, r + 0.03);
        break;
      }
      case 'none':
        break;
    }
  }

  private dressExtra(look: Look, mat: Mat, part: Part, torso: number, s: number): void {
    switch (look.extra) {
      case 'badge':
        part(new THREE.CircleGeometry(0.035, 6), mat(0xd4af37, 0.25, 0.95), this.chest, -0.09, torso * 0.8, 0.135);
        // A belt with a radio.
        part(new THREE.BoxGeometry(0.06, 0.09, 0.04), mat(0x111111, 0.4), this.chest, 0.17, 0.05, 0.08);
        break;
      case 'apron':
        part(new THREE.BoxGeometry(0.3, 0.45, 0.02), mat(0x6b4226, 0.8), this.chest, 0, torso * 0.35, 0.135);
        break;
      case 'tie':
        part(new THREE.BoxGeometry(0.05, 0.3, 0.015), mat(0xb3001b, 0.5), this.chest, 0, torso * 0.62, 0.135);
        break;
      case 'guitar': {
        const guitar = new THREE.Group();
        const wood = mat(0xb5651d, 0.4);
        part(new THREE.CylinderGeometry(0.17, 0.17, 0.08, 18), wood, guitar, 0, 0, 0).rotation.x = Math.PI / 2;
        part(new THREE.BoxGeometry(0.05, 0.5, 0.03), mat(0x3a2a1a, 0.5), guitar, 0, 0.32, 0);
        guitar.position.set(0, torso * 0.45, -0.17);
        guitar.rotation.z = 0.7;
        this.chest.add(guitar);
        break;
      }
      case 'camera':
        part(new THREE.BoxGeometry(0.12, 0.08, 0.07), mat(0x111111, 0.35, 0.3), this.chest, 0.08, torso * 0.55, 0.16);
        part(new THREE.CylinderGeometry(0.03, 0.03, 0.05, 12), mat(0x333333, 0.2, 0.6), this.chest, 0.08, torso * 0.55, 0.21).rotation.x = Math.PI / 2;
        break;
      case 'skateboard':
        part(new THREE.BoxGeometry(0.2, 0.7, 0.02), mat(0xf46036, 0.5), this.arms[1] ?? this.chest, 0.05, -0.45 * s, 0.12);
        break;
      case 'paint':
        part(new THREE.CylinderGeometry(0.05, 0.05, 0.18, 10), mat(0xff70a6, 0.4, 0.3), this.arms[0] ?? this.chest, 0, -0.62 * s, 0.05);
        break;
      case 'wrench':
        part(new THREE.BoxGeometry(0.03, 0.22, 0.015), mat(0xaaaaaa, 0.3, 0.9), this.arms[1] ?? this.chest, 0, -0.66 * s, 0.05);
        break;
      case 'headband':
        part(new THREE.TorusGeometry(0.115 * s, 0.018, 6, 18), mat(0xffffff, 0.6), this.head, 0, 0.18 * s, 0).rotation.x = Math.PI / 2;
        break;
      case 'newspaper':
        part(new THREE.BoxGeometry(0.02, 0.28, 0.2), mat(0xeeeeee, 0.9), this.arms[0] ?? this.chest, 0.04, -0.58 * s, 0.05);
        break;
      case 'spatula':
        part(new THREE.BoxGeometry(0.015, 0.3, 0.06), mat(0x999999, 0.3, 0.8), this.arms[1] ?? this.chest, 0, -0.7 * s, 0.05);
        break;
      case 'none':
        break;
    }
  }

  /**
   * Moves the joints for what they're doing. `amount` is how much (for
   * walking, how fast; for a dive, how far through it).
   */
  animate(dt: number, action: Action, speed: number, lookAt: number | null): void {
    const pace = action === 'run' ? 2.4 : 1;
    this.phase += dt * (action === 'walk' || action === 'run' ? Math.max(1, speed) * 4.6 * pace * 0.5 : 2);
    const t = this.phase;
    let legSwing = 0;
    let armSwing = 0;
    let armRaise = [0, 0];
    let bob = 0;
    let lean = 0;
    switch (action) {
      case 'walk':
      case 'run':
        legSwing = Math.sin(t) * (action === 'run' ? 0.85 : 0.5);
        armSwing = -legSwing * 0.9;
        bob = Math.abs(Math.cos(t)) * (action === 'run' ? 0.06 : 0.03);
        lean = action === 'run' ? 0.2 : 0.04;
        break;
      case 'talk':
        // Talking with their hands.
        armRaise = [0.4 + Math.sin(t * 1.7) * 0.35, 0.25 + Math.sin(t * 2.3 + 1) * 0.3];
        bob = Math.sin(t * 2) * 0.01;
        break;
      case 'listen':
        bob = Math.sin(t) * 0.008;
        armRaise = [0.1, 0.1];
        break;
      case 'cheer':
        armRaise = [2.6 + Math.sin(t * 6) * 0.25, 2.6 + Math.sin(t * 6 + 2) * 0.25];
        bob = Math.abs(Math.sin(t * 3)) * 0.12;
        break;
      case 'wave':
        armRaise = [0, 2.4 + Math.sin(t * 7) * 0.35];
        break;
      case 'dive':
        armRaise = [2.8, 2.8];
        legSwing = 0.5;
        lean = 1.1;
        break;
      case 'stand':
        bob = Math.sin(t * 0.8) * 0.006; // breathing
        break;
    }
    this.legs[0].rotation.x = legSwing;
    this.legs[1].rotation.x = -legSwing;
    this.arms[0].rotation.set(armSwing - armRaise[0] * 0.3, 0, -armRaise[0]);
    this.arms[1].rotation.set(-armSwing - armRaise[1] * 0.3, 0, armRaise[1]);
    this.hips.position.y = this.standHeight + bob;
    this.chest.rotation.x = lean;
    // Turn the head towards something (in radians, relative to the body), within reason.
    const target = lookAt === null ? Math.sin(t * 0.3) * 0.15 : Math.max(-1.1, Math.min(1.1, lookAt));
    this.head.rotation.y += (target - this.head.rotation.y) * Math.min(1, dt * 4);
  }

  dispose(): void {
    this.root.removeFromParent();
    for (const g of this.geometries) g.dispose();
    for (const m of this.materials) m.dispose();
  }
}

type Part = (geometry: THREE.BufferGeometry, material: THREE.Material, parent: THREE.Object3D, x?: number, y?: number, z?: number) => THREE.Mesh;
type Mat = (color: number, rough?: number, metal?: number) => THREE.MeshStandardMaterial;
