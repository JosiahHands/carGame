// The drivable world: a city grid with buildings, parks, a car park and a stunt park.
import * as THREE from 'three';
import { buildCar, type CarRig } from './carModel';
import { DEFAULT_CONFIG, randomConfig } from './carConfig';
import { concreteTexture, grassTexture, roadTexture, windowTexture } from './textures';
import { gradientTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';

export interface Box {
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
}

export interface Circle {
  x: number;
  z: number;
  r: number;
}

interface Ramp {
  x: number;
  z: number;
  angle: number; // direction the ramp faces (driving up it)
  width: number;
  length: number;
  height: number;
}

interface Cone {
  mesh: THREE.Mesh;
  home: THREE.Vector3;
  vel: THREE.Vector3;
  spin: THREE.Vector3;
  knocked: boolean;
  knockedAt: number;
}

export const HALF_SIZE = 260; // the world spans -260..260 on x and z
const ROAD_LINES = [-240, -160, -80, 0, 80, 160, 240];
const ROAD_WIDTH = 12;
const RISE = 0.8; // fraction of a ramp's length that slopes up

export class World {
  readonly group = new THREE.Group();
  readonly boxes: Box[] = [];
  readonly circles: Circle[] = [];
  readonly spawn = { x: -3, z: -20, heading: 0 };
  private ramps: Ramp[] = [];
  private cones: Cone[] = [];
  private parkedCars: CarRig[] = [];
  private buildingMats: THREE.MeshStandardMaterial[] = [];
  private lampHeadMat: THREE.MeshStandardMaterial;
  private sun: THREE.DirectionalLight;
  private hemi: THREE.HemisphereLight;
  private dayBg: THREE.Texture;
  private nightBg: THREE.Texture;
  private rand = seededRandom(42);
  private clock = 0;

  constructor(private scene: THREE.Scene) {
    scene.add(this.group);

    this.dayBg = gradientTexture('#5fa8e8', '#cfe5f5');
    this.nightBg = gradientTexture('#02040b', '#18233a');
    this.hemi = new THREE.HemisphereLight(0xdfefff, 0x4a5a3a, 1.2);
    this.sun = new THREE.DirectionalLight(0xfff2dd, 2.6);
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(2048, 2048);
    const s = this.sun.shadow.camera;
    s.left = s.bottom = -45;
    s.right = s.top = 45;
    s.near = 1;
    s.far = 200;
    this.sun.shadow.bias = -0.0004;
    this.sun.shadow.normalBias = 0.03;
    scene.add(this.hemi, this.sun, this.sun.target);

    this.lampHeadMat = new THREE.MeshStandardMaterial({ color: 0xffffff, emissive: 0xffd9a0, emissiveIntensity: 0 });

    this.buildGround();
    this.buildRoads();
    this.buildBlocks();
    this.buildEdges();
    this.setNight(false);
  }

  // ---------- building the world ----------

  private buildGround(): void {
    const tex = grassTexture();
    tex.repeat.set(140, 140);
    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(HALF_SIZE * 2 + 400, HALF_SIZE * 2 + 400),
      new THREE.MeshStandardMaterial({ map: tex, roughness: 1 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.receiveShadow = true;
    this.group.add(ground);
  }

  private buildRoads(): void {
    const tex = roadTexture();
    const len = HALF_SIZE * 2 - 16;
    for (const [i, c] of ROAD_LINES.entries()) {
      for (const dir of ['x', 'z'] as const) {
        const t = tex.clone();
        t.repeat.set(1, len / 12);
        t.needsUpdate = true;
        const road = new THREE.Mesh(
          new THREE.PlaneGeometry(ROAD_WIDTH, len),
          new THREE.MeshStandardMaterial({ map: t, roughness: 0.9 }),
        );
        road.rotation.x = -Math.PI / 2;
        if (dir === 'x') {
          road.rotation.z = Math.PI / 2;
          road.position.set(0, 0.02 + i * 0.0005, c);
        } else {
          road.position.set(c, 0.025 + i * 0.0005, 0);
        }
        road.receiveShadow = true;
        this.group.add(road);
      }
    }
    // Plain asphalt squares at crossings, so dashed lines don't overlap.
    const crossMat = new THREE.MeshStandardMaterial({ color: 0x34363b, roughness: 0.9 });
    const crossGeo = new THREE.PlaneGeometry(ROAD_WIDTH, ROAD_WIDTH);
    for (const x of ROAD_LINES) {
      for (const z of ROAD_LINES) {
        const m = new THREE.Mesh(crossGeo, crossMat);
        m.rotation.x = -Math.PI / 2;
        m.position.set(x, 0.035, z);
        m.receiveShadow = true;
        this.group.add(m);
      }
    }
    // Street lamps along the roads.
    const poles: THREE.Matrix4[] = [];
    // Lamps sit 20 m either side of each block centre, well clear of the crossings.
    const along = ROAD_LINES.slice(0, -1).flatMap((c) => [c + 20, c + 60]);
    for (const c of ROAD_LINES) {
      for (const p of along) {
        poles.push(new THREE.Matrix4().makeTranslation(c + ROAD_WIDTH / 2 + 1, 0, p));
        poles.push(new THREE.Matrix4().makeTranslation(p, 0, c - ROAD_WIDTH / 2 - 1));
      }
    }
    const poleGeo = new THREE.CylinderGeometry(0.12, 0.16, 7, 8).translate(0, 3.5, 0);
    const headGeo = new THREE.BoxGeometry(0.7, 0.25, 0.7).translate(0, 7.1, 0);
    const poleMesh = new THREE.InstancedMesh(poleGeo, new THREE.MeshStandardMaterial({ color: 0x3a3d42, metalness: 0.6, roughness: 0.4 }), poles.length);
    const headMesh = new THREE.InstancedMesh(headGeo, this.lampHeadMat, poles.length);
    poles.forEach((m, i) => {
      poleMesh.setMatrixAt(i, m);
      headMesh.setMatrixAt(i, m);
      const p = new THREE.Vector3().setFromMatrixPosition(m);
      this.circles.push({ x: p.x, z: p.z, r: 0.25 });
    });
    poleMesh.castShadow = true;
    this.group.add(poleMesh, headMesh);
  }

  private buildBlocks(): void {
    const centers = ROAD_LINES.slice(0, -1).map((c) => c + 40);
    const pavement = concreteTexture();
    const trees: THREE.Vector3[] = [];
    for (const bx of centers) {
      for (const bz of centers) {
        const inner = 80 - ROAD_WIDTH; // 68 m square between the roads
        const t = pavement.clone();
        t.repeat.set(inner / 4, inner / 4);
        t.needsUpdate = true;
        const slab = new THREE.Mesh(
          new THREE.PlaneGeometry(inner, inner),
          new THREE.MeshStandardMaterial({ map: t, roughness: 0.95 }),
        );
        slab.rotation.x = -Math.PI / 2;
        slab.position.set(bx, 0.015, bz);
        slab.receiveShadow = true;

        let kind: 'stunt' | 'park' | 'lot' | 'city';
        if (bx === 40 && bz === 40) kind = 'stunt';
        else if (bx === -40 && bz === 40) kind = 'park';
        else if (bx === 40 && bz === -40) kind = 'lot';
        else if (bx === -40 && bz === -40) kind = 'park';
        else {
          const r = this.rand();
          kind = r < 0.62 ? 'city' : r < 0.85 ? 'park' : 'lot';
        }

        if (kind === 'park') {
          // Grass park ringed by pavement, full of trees.
          const grass = new THREE.Mesh(
            new THREE.PlaneGeometry(inner - 8, inner - 8),
            new THREE.MeshStandardMaterial({ color: 0x5b8f43, roughness: 1 }),
          );
          grass.rotation.x = -Math.PI / 2;
          grass.position.set(bx, 0.018, bz);
          grass.receiveShadow = true;
          this.group.add(slab, grass);
          for (let i = 0; i < 26; i++) {
            trees.push(new THREE.Vector3(bx + (this.rand() - 0.5) * (inner - 14), 0, bz + (this.rand() - 0.5) * (inner - 14)));
          }
        } else if (kind === 'city') {
          this.group.add(slab);
          for (const ox of [-1, 1]) {
            for (const oz of [-1, 1]) {
              const w = 14 + this.rand() * 14;
              const d = 14 + this.rand() * 14;
              const h = 8 + Math.pow(this.rand(), 1.6) * 60;
              this.addBuilding(bx + ox * 17, bz + oz * 17, w, d, h);
            }
          }
        } else if (kind === 'lot') {
          this.group.add(slab);
          this.buildCarPark(bx, bz);
        } else {
          this.group.add(slab);
          this.buildStuntPark(bx, bz);
        }
      }
    }
    // Trees around the outskirts.
    for (let i = 0; i < 140; i++) {
      const side = Math.floor(this.rand() * 4);
      const along = (this.rand() - 0.5) * (HALF_SIZE * 2 - 10);
      const off = HALF_SIZE - 3 - this.rand() * 6;
      const p = [new THREE.Vector3(along, 0, off), new THREE.Vector3(along, 0, -off), new THREE.Vector3(off, 0, along), new THREE.Vector3(-off, 0, along)][side];
      trees.push(p);
    }
    this.addTrees(trees);
  }

  private addBuilding(x: number, z: number, w: number, d: number, h: number): void {
    if (this.buildingMats.length === 0) {
      const day = windowTexture(false);
      const lit = windowTexture(true);
      for (const c of ['#c9c2b8', '#9fa8b3', '#b98b73', '#7d8a99', '#d9d4c7', '#8c7f73', '#a7b6a5']) {
        this.buildingMats.push(
          new THREE.MeshStandardMaterial({ color: c, map: day, emissiveMap: lit, emissive: 0xffffff, emissiveIntensity: 0, roughness: 0.85 }),
        );
      }
    }
    const geo = new THREE.BoxGeometry(w, h, d);
    // Scale UVs so windows stay the same size on every building; roofs get a plain patch.
    const uv = geo.getAttribute('uv') as THREE.BufferAttribute;
    const faceSize = [[d, h], [d, h], [0, 0], [0, 0], [w, h], [w, h]];
    for (let f = 0; f < 6; f++) {
      for (let v = 0; v < 4; v++) {
        const i = f * 4 + v;
        const [fw, fh] = faceSize[f];
        if (fw === 0) uv.setXY(i, 0.02, 0.02);
        else uv.setXY(i, uv.getX(i) * (fw / 8), uv.getY(i) * (fh / 7));
      }
    }
    const mat = this.buildingMats[Math.floor(this.rand() * this.buildingMats.length)];
    const mesh = new THREE.Mesh(geo, mat);
    mesh.position.set(x, h / 2, z);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    this.group.add(mesh);
    this.boxes.push({ minX: x - w / 2, maxX: x + w / 2, minZ: z - d / 2, maxZ: z + d / 2 });
  }

  private addTrees(points: THREE.Vector3[]): void {
    const trunkGeo = new THREE.CylinderGeometry(0.22, 0.3, 2.4, 7).translate(0, 1.2, 0);
    const leafGeo = new THREE.IcosahedronGeometry(1.8, 0).translate(0, 3.6, 0);
    const trunks = new THREE.InstancedMesh(trunkGeo, new THREE.MeshStandardMaterial({ color: 0x6b4a2f, roughness: 1 }), points.length);
    const leaves = new THREE.InstancedMesh(leafGeo, new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.9, flatShading: true }), points.length);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const color = new THREE.Color();
    points.forEach((p, i) => {
      const s = 0.8 + this.rand() * 0.6;
      q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), this.rand() * Math.PI * 2);
      m.compose(p, q, new THREE.Vector3(s, s, s));
      trunks.setMatrixAt(i, m);
      leaves.setMatrixAt(i, m);
      leaves.setColorAt(i, color.setHSL(0.25 + this.rand() * 0.08, 0.5, 0.28 + this.rand() * 0.12));
      this.circles.push({ x: p.x, z: p.z, r: 0.45 * s });
    });
    trunks.castShadow = leaves.castShadow = true;
    trunks.receiveShadow = leaves.receiveShadow = true;
    this.group.add(trunks, leaves);
  }

  private buildCarPark(bx: number, bz: number): void {
    const lineMat = new THREE.MeshBasicMaterial({ color: 0xf0f0f0 });
    const asphalt = new THREE.Mesh(
      new THREE.PlaneGeometry(60, 60),
      new THREE.MeshStandardMaterial({ color: 0x3a3c40, roughness: 0.95 }),
    );
    asphalt.rotation.x = -Math.PI / 2;
    asphalt.position.set(bx, 0.02, bz);
    asphalt.receiveShadow = true;
    this.group.add(asphalt);
    for (const row of [-1, 1]) {
      for (let i = 0; i < 9; i++) {
        const x = bx - 24 + i * 6;
        const z = bz + row * 18;
        const line = new THREE.Mesh(new THREE.PlaneGeometry(0.15, 9), lineMat);
        line.rotation.x = -Math.PI / 2;
        line.position.set(x - 3, 0.025, z);
        this.group.add(line);
        if (this.rand() < 0.55) {
          const car = buildCar(randomConfig(DEFAULT_CONFIG), false, false);
          car.root.position.set(x, 0, z);
          car.root.rotation.y = row > 0 ? Math.PI : 0;
          this.group.add(car.root);
          this.parkedCars.push(car);
          const hw = car.width / 2 + 0.05;
          const hl = car.length / 2;
          this.boxes.push({ minX: x - hw, maxX: x + hw, minZ: z - hl, maxZ: z + hl });
        }
      }
    }
  }

  private buildStuntPark(bx: number, bz: number): void {
    const rampMat = new THREE.MeshStandardMaterial({ color: 0xe0a03a, roughness: 0.7 });
    const stripeMat = new THREE.MeshStandardMaterial({ color: 0x222222, roughness: 0.7 });
    const ramps: Ramp[] = [
      { x: bx - 18, z: bz - 12, angle: 0, width: 6, length: 10, height: 1.6 },
      { x: bx + 14, z: bz - 6, angle: Math.PI / 2, width: 7, length: 14, height: 2.6 },
      { x: bx - 4, z: bz + 18, angle: Math.PI, width: 5, length: 8, height: 1.2 },
      { x: bx + 20, z: bz + 22, angle: -Math.PI / 2, width: 6, length: 12, height: 2.0 },
    ];
    for (const ramp of ramps) {
      const L = ramp.length;
      const shape = new THREE.Shape();
      shape.moveTo(-L / 2, 0);
      shape.lineTo(-L / 2 + L * RISE, ramp.height);
      shape.lineTo(L / 2, 0);
      shape.closePath();
      const geo = new THREE.ExtrudeGeometry(shape, { depth: ramp.width, bevelEnabled: false });
      geo.translate(0, 0, -ramp.width / 2);
      geo.rotateY(-Math.PI / 2); // profile now runs along +z
      const mesh = new THREE.Mesh(geo, rampMat);
      mesh.position.set(ramp.x, 0, ramp.z);
      mesh.rotation.y = ramp.angle;
      mesh.castShadow = mesh.receiveShadow = true;
      this.group.add(mesh);
      // Hazard stripes along the lip.
      for (const side of [-1, 1]) {
        const s = new THREE.Mesh(new THREE.BoxGeometry(0.3, 0.05, 0.3), stripeMat);
        s.position.set(side * (ramp.width / 2 - 0.3), ramp.height + 0.02, -L / 2 + L * RISE);
        mesh.add(s);
      }
      this.ramps.push(ramp);
    }

    // Cone slalom and a cone box to smash through.
    const coneGeo = new THREE.ConeGeometry(0.28, 0.75, 14).translate(0, 0.375, 0);
    const coneMat = new THREE.MeshStandardMaterial({ color: 0xff6a13, roughness: 0.6 });
    const place = (x: number, z: number) => {
      const mesh = new THREE.Mesh(coneGeo, coneMat);
      mesh.position.set(x, 0, z);
      mesh.castShadow = true;
      this.group.add(mesh);
      this.cones.push({ mesh, home: mesh.position.clone(), vel: new THREE.Vector3(), spin: new THREE.Vector3(), knocked: false, knockedAt: 0 });
    };
    for (let i = 0; i < 10; i++) place(bx - 28 + i * 5.5, bz + 4 + (i % 2) * 2);
    for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) place(bx - 22 + i * 1.6, bz + 26 + j * 1.6);
    for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) place(bx + 2 + i * 1.6, bz - 26 + j * 1.6);
  }

  private buildEdges(): void {
    // Barrier walls around the edge.
    const wallMat = new THREE.MeshStandardMaterial({ color: 0xb8bcc2, roughness: 0.9 });
    const len = HALF_SIZE * 2 + 2;
    for (const [x, z, w, d] of [
      [0, HALF_SIZE + 1, len, 2],
      [0, -HALF_SIZE - 1, len, 2],
      [HALF_SIZE + 1, 0, 2, len],
      [-HALF_SIZE - 1, 0, 2, len],
    ]) {
      const wall = new THREE.Mesh(new THREE.BoxGeometry(w, 1.4, d), wallMat);
      wall.position.set(x, 0.7, z);
      wall.receiveShadow = true;
      this.group.add(wall);
      this.boxes.push({ minX: x - w / 2, maxX: x + w / 2, minZ: z - d / 2, maxZ: z + d / 2 });
    }
    // Hills on the horizon.
    const hillGeo = new THREE.ConeGeometry(1, 1, 7);
    const count = 70;
    const hills = new THREE.InstancedMesh(hillGeo, new THREE.MeshStandardMaterial({ color: 0x55734a, roughness: 1, flatShading: true }), count);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    for (let i = 0; i < count; i++) {
      const a = (i / count) * Math.PI * 2;
      const dist = 340 + this.rand() * 80;
      const r = 50 + this.rand() * 60;
      const h = 30 + this.rand() * 60;
      q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), this.rand() * Math.PI);
      m.compose(new THREE.Vector3(Math.cos(a) * dist, h / 2 - 2, Math.sin(a) * dist), q, new THREE.Vector3(r, h, r));
      hills.setMatrixAt(i, m);
    }
    this.group.add(hills);
  }

  // ---------- runtime ----------

  /** Ground height at a point (non-zero only on ramps). */
  heightAt(x: number, z: number): number {
    let best = 0;
    for (const r of this.ramps) {
      const dx = x - r.x;
      const dz = z - r.z;
      const c = Math.cos(r.angle);
      const s = Math.sin(r.angle);
      // Into the ramp's local frame: u runs up the ramp, v across it.
      const u = dx * s + dz * c;
      const v = dx * c - dz * s;
      if (Math.abs(v) > r.width / 2 || Math.abs(u) > r.length / 2) continue;
      const fromStart = u + r.length / 2;
      const peak = r.length * RISE;
      const h = fromStart <= peak ? (r.height * fromStart) / peak : (r.height * (r.length - fromStart)) / (r.length - peak);
      if (h > best) best = h;
    }
    return best;
  }

  /** Knocks over cones the car touches. Returns how many were hit. */
  hitCones(x: number, z: number, radius: number, vx: number, vz: number): number {
    let hits = 0;
    for (const c of this.cones) {
      if (c.knocked) continue;
      const dx = c.mesh.position.x - x;
      const dz = c.mesh.position.z - z;
      if (dx * dx + dz * dz > (radius + 0.3) ** 2) continue;
      c.knocked = true;
      c.knockedAt = this.clock;
      const speed = Math.hypot(vx, vz);
      c.vel.set(vx * 1.1 + dx * 2, 2 + speed * 0.18, vz * 1.1 + dz * 2);
      c.spin.set((Math.random() - 0.5) * 14, (Math.random() - 0.5) * 6, (Math.random() - 0.5) * 14);
      hits++;
    }
    return hits;
  }

  update(dt: number, carX: number, carZ: number): void {
    this.clock += dt;
    for (const c of this.cones) {
      if (!c.knocked) continue;
      const p = c.mesh.position;
      if (c.vel.lengthSq() > 0.01 || p.y > 0.01) {
        c.vel.y -= 14 * dt;
        p.addScaledVector(c.vel, dt);
        c.mesh.rotation.x += c.spin.x * dt;
        c.mesh.rotation.y += c.spin.y * dt;
        c.mesh.rotation.z += c.spin.z * dt;
        if (p.y < 0) {
          p.y = 0;
          c.vel.y = Math.abs(c.vel.y) * 0.3;
          c.vel.x *= 0.6;
          c.vel.z *= 0.6;
          c.spin.multiplyScalar(0.6);
          if (c.vel.lengthSq() < 0.5) {
            c.vel.set(0, 0, 0);
            // Settle lying on its side.
            c.mesh.rotation.set(Math.PI / 2, c.mesh.rotation.y, 0);
            p.y = 0.25;
          }
        }
      }
      // Put cones back once the player has driven away.
      if (this.clock - c.knockedAt > 12 && Math.hypot(carX - c.home.x, carZ - c.home.z) > 40) {
        c.knocked = false;
        c.vel.set(0, 0, 0);
        c.mesh.position.copy(c.home);
        c.mesh.rotation.set(0, 0, 0);
      }
    }
    // Keep the sun's shadow box centred on the car.
    this.sun.position.set(carX + 40, 80, carZ + 25);
    this.sun.target.position.set(carX, 0, carZ);
  }

  setNight(night: boolean): void {
    this.scene.background = night ? this.nightBg : this.dayBg;
    this.scene.fog = new THREE.Fog(night ? 0x0b1222 : 0xc4dcef, 60, 430);
    this.hemi.intensity = night ? 0.25 : 1.2;
    this.hemi.color.set(night ? 0x6b7fb3 : 0xdfefff);
    this.sun.intensity = night ? 0.35 : 2.6;
    this.sun.color.set(night ? 0x9bb0ff : 0xfff2dd);
    for (const m of this.buildingMats) m.emissiveIntensity = night ? 1.4 : 0;
    this.lampHeadMat.emissiveIntensity = night ? 3 : 0;
    for (const car of this.parkedCars) car.setLights({ night: false, braking: false });
  }
}
