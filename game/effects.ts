// Visual effects: fire, smoke and sparks (particles), flying debris, fireballs,
// shockwave rings, scorch marks, bullet tracers and explosion flashes.
// Everything is pooled, so nothing is created or thrown away while playing.
import * as THREE from 'three';
import { radialGlowTexture } from '../libs/canvasTexture';

const UP = new THREE.Vector3(0, 1, 0);

// ---------- particles ----------

export interface ParticleSpec {
  x: number;
  y: number;
  z: number;
  vx?: number;
  vy?: number;
  vz?: number;
  life: number; // seconds
  size: [number, number]; // metres, at birth and at death
  color: [number, number, number]; // 0..1 linear-ish RGB
  endColor?: [number, number, number];
  alpha?: [number, number];
  gravity?: number; // m/s², negative rises (smoke)
  drag?: number; // fraction of speed lost per second
}

const VERTEX = /* glsl */ `
  attribute float size;
  attribute vec4 tint;
  uniform float scale;
  varying vec4 vTint;
  void main() {
    vTint = tint;
    vec4 mv = modelViewMatrix * vec4(position, 1.0);
    gl_PointSize = size * scale / max(0.1, -mv.z);
    gl_Position = projectionMatrix * mv;
  }
`;

const FRAGMENT = /* glsl */ `
  uniform float softness;
  varying vec4 vTint;
  void main() {
    float d = length(gl_PointCoord - 0.5) * 2.0;
    if (d > 1.0) discard;
    float a = pow(1.0 - d, softness);
    gl_FragColor = vec4(vTint.rgb, vTint.a * a);
    #include <tonemapping_fragment>
    #include <colorspace_fragment>
  }
`;

/** One pool of round, camera-facing particles drawn in a single call. */
class ParticleLayer {
  readonly points: THREE.Points;
  readonly material: THREE.ShaderMaterial;
  private pos: Float32Array;
  private size: Float32Array;
  private tint: Float32Array;
  private vel: Float32Array;
  private age: Float32Array;
  private life: Float32Array;
  private sizes: Float32Array; // start, end
  private colors: Float32Array; // start rgb, end rgb
  private alphas: Float32Array; // start, end
  private gravity: Float32Array;
  private drag: Float32Array;
  private next = 0;
  private live = 0;

  constructor(private capacity: number, additive: boolean) {
    const geo = new THREE.BufferGeometry();
    this.pos = new Float32Array(capacity * 3);
    this.size = new Float32Array(capacity);
    this.tint = new Float32Array(capacity * 4);
    geo.setAttribute('position', new THREE.BufferAttribute(this.pos, 3).setUsage(THREE.DynamicDrawUsage));
    geo.setAttribute('size', new THREE.BufferAttribute(this.size, 1).setUsage(THREE.DynamicDrawUsage));
    geo.setAttribute('tint', new THREE.BufferAttribute(this.tint, 4).setUsage(THREE.DynamicDrawUsage));
    this.vel = new Float32Array(capacity * 3);
    this.age = new Float32Array(capacity).fill(1);
    this.life = new Float32Array(capacity).fill(0);
    this.sizes = new Float32Array(capacity * 2);
    this.colors = new Float32Array(capacity * 6);
    this.alphas = new Float32Array(capacity * 2);
    this.gravity = new Float32Array(capacity);
    this.drag = new Float32Array(capacity);
    this.material = new THREE.ShaderMaterial({
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      uniforms: { scale: { value: 500 }, softness: { value: additive ? 1.6 : 1.2 } },
      transparent: true,
      depthWrite: false,
      blending: additive ? THREE.AdditiveBlending : THREE.NormalBlending,
    });
    this.points = new THREE.Points(geo, this.material);
    this.points.frustumCulled = false;
    this.points.renderOrder = additive ? 3 : 2;
  }

  spawn(p: ParticleSpec): void {
    const i = this.next;
    this.next = (this.next + 1) % this.capacity;
    if (this.age[i] >= this.life[i]) this.live++;
    this.pos.set([p.x, p.y, p.z], i * 3);
    this.vel.set([p.vx ?? 0, p.vy ?? 0, p.vz ?? 0], i * 3);
    this.age[i] = 0;
    this.life[i] = p.life;
    this.sizes.set(p.size, i * 2);
    const end = p.endColor ?? p.color;
    this.colors.set([...p.color, ...end], i * 6);
    this.alphas.set(p.alpha ?? [1, 0], i * 2);
    this.gravity[i] = p.gravity ?? 0;
    this.drag[i] = p.drag ?? 0;
  }

  update(dt: number): void {
    if (this.live === 0) return;
    let live = 0;
    for (let i = 0; i < this.capacity; i++) {
      if (this.age[i] >= this.life[i]) continue;
      this.age[i] += dt;
      if (this.age[i] >= this.life[i]) {
        this.size[i] = 0;
        this.tint[i * 4 + 3] = 0;
        continue;
      }
      live++;
      const t = this.age[i] / this.life[i];
      const k = Math.max(0, 1 - this.drag[i] * dt);
      const v = i * 3;
      this.vel[v] *= k;
      this.vel[v + 1] = this.vel[v + 1] * k - this.gravity[i] * dt;
      this.vel[v + 2] *= k;
      this.pos[v] += this.vel[v] * dt;
      this.pos[v + 1] = Math.max(0.05, this.pos[v + 1] + this.vel[v + 1] * dt);
      this.pos[v + 2] += this.vel[v + 2] * dt;
      this.size[i] = this.sizes[i * 2] + (this.sizes[i * 2 + 1] - this.sizes[i * 2]) * t;
      const c = i * 6;
      this.tint[i * 4] = this.colors[c] + (this.colors[c + 3] - this.colors[c]) * t;
      this.tint[i * 4 + 1] = this.colors[c + 1] + (this.colors[c + 4] - this.colors[c + 1]) * t;
      this.tint[i * 4 + 2] = this.colors[c + 2] + (this.colors[c + 5] - this.colors[c + 2]) * t;
      this.tint[i * 4 + 3] = this.alphas[i * 2] + (this.alphas[i * 2 + 1] - this.alphas[i * 2]) * t;
    }
    this.live = live;
    const geo = this.points.geometry;
    geo.attributes.position.needsUpdate = true;
    geo.attributes.size.needsUpdate = true;
    geo.attributes.tint.needsUpdate = true;
  }

  clear(): void {
    this.age.fill(1);
    this.life.fill(0);
    this.size.fill(0);
    this.live = 0;
    this.points.geometry.attributes.size.needsUpdate = true;
  }
}

// ---------- debris ----------

interface Chunk {
  pos: THREE.Vector3;
  vel: THREE.Vector3;
  quat: THREE.Quaternion;
  spin: THREE.Vector3;
  scale: THREE.Vector3;
  age: number;
  life: number;
  resting: boolean;
}

export interface DebrisSpec {
  pos: THREE.Vector3;
  vel: THREE.Vector3;
  scale: THREE.Vector3;
  color: THREE.ColorRepresentation;
  life?: number;
  spin?: number;
}

// ---------- the effects system ----------

export class Effects {
  readonly group = new THREE.Group();
  /** 0..1 multiplier for how many particles and chunks each effect makes. */
  amount = 1;
  private glow: ParticleLayer;
  private smoke: ParticleLayer;
  private chunks: Chunk[] = [];
  private debris: THREE.InstancedMesh;
  private nextChunk = 0;
  private fireballs: { mesh: THREE.Mesh; age: number; life: number; size: number }[] = [];
  private rings: { mesh: THREE.Mesh; age: number; life: number; size: number }[] = [];
  private scorch: { mesh: THREE.Mesh; age: number }[] = [];
  private nextScorch = 0;
  private tracers: THREE.InstancedMesh;
  private tracerData: { from: THREE.Vector3; to: THREE.Vector3; age: number; life: number }[] = [];
  private nextTracer = 0;
  private flash: THREE.PointLight;
  private flashLevel = 0;
  private m = new THREE.Matrix4();
  private v = new THREE.Vector3();
  private q = new THREE.Quaternion();
  private color = new THREE.Color();

  constructor(scene: THREE.Scene) {
    scene.add(this.group);
    this.glow = new ParticleLayer(2000, true);
    this.smoke = new ParticleLayer(1600, false);
    this.group.add(this.smoke.points, this.glow.points);

    // Debris: one instanced box, each chunk scaled and coloured on its own.
    const chunkCount = 360;
    this.debris = new THREE.InstancedMesh(new THREE.BoxGeometry(1, 1, 1), new THREE.MeshStandardMaterial({ roughness: 0.9 }), chunkCount);
    this.debris.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.debris.castShadow = true;
    this.debris.frustumCulled = false;
    for (let i = 0; i < chunkCount; i++) {
      this.chunks.push({
        pos: new THREE.Vector3(),
        vel: new THREE.Vector3(),
        quat: new THREE.Quaternion(),
        spin: new THREE.Vector3(),
        scale: new THREE.Vector3(),
        age: 1,
        life: 0,
        resting: true,
      });
      this.debris.setMatrixAt(i, this.m.makeScale(0, 0, 0));
      this.debris.setColorAt(i, this.color.set(0x777777));
    }
    this.group.add(this.debris);

    const glowMap = radialGlowTexture();
    for (let i = 0; i < 10; i++) {
      const mesh = new THREE.Mesh(
        new THREE.IcosahedronGeometry(1, 2),
        new THREE.MeshBasicMaterial({ color: 0xffa040, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false }),
      );
      mesh.visible = false;
      this.group.add(mesh);
      this.fireballs.push({ mesh, age: 1, life: 0, size: 1 });
    }
    for (let i = 0; i < 4; i++) {
      const mesh = new THREE.Mesh(
        new THREE.RingGeometry(0.8, 1, 48).rotateX(-Math.PI / 2),
        new THREE.MeshBasicMaterial({ color: 0x9fdcff, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false, side: THREE.DoubleSide }),
      );
      mesh.visible = false;
      this.group.add(mesh);
      this.rings.push({ mesh, age: 1, life: 0, size: 1 });
    }
    const scorchGeo = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
    for (let i = 0; i < 40; i++) {
      const mesh = new THREE.Mesh(
        scorchGeo,
        new THREE.MeshBasicMaterial({ color: 0x000000, map: glowMap, transparent: true, opacity: 0, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2 }),
      );
      mesh.visible = false;
      mesh.renderOrder = 1;
      this.group.add(mesh);
      this.scorch.push({ mesh, age: 0 });
    }

    // Tracers: thin glowing rods stretched along each bullet's path.
    const tracerCount = 48;
    this.tracers = new THREE.InstancedMesh(
      new THREE.BoxGeometry(0.05, 0.05, 1).translate(0, 0, 0.5),
      new THREE.MeshBasicMaterial({ color: 0xffd27a, transparent: true, opacity: 0.9, blending: THREE.AdditiveBlending, depthWrite: false }),
      tracerCount,
    );
    this.tracers.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.tracers.frustumCulled = false;
    for (let i = 0; i < tracerCount; i++) {
      this.tracers.setMatrixAt(i, this.m.makeScale(0, 0, 0));
      this.tracerData.push({ from: new THREE.Vector3(), to: new THREE.Vector3(), age: 1, life: 0 });
    }
    this.group.add(this.tracers);

    // One light for explosion flashes. It always exists (at zero brightness when
    // idle): adding and removing lights makes every material recompile.
    this.flash = new THREE.PointLight(0xffa040, 0, 40, 1.6);
    this.group.add(this.flash);
  }

  /** Keeps particle sizes right when the window or field of view changes. */
  setView(camera: THREE.PerspectiveCamera, drawingHeight: number): void {
    const scale = drawingHeight / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov) / 2));
    this.glow.material.uniforms.scale.value = scale;
    this.smoke.material.uniforms.scale.value = scale;
  }

  /** How many to make, scaled by the effects quality. Always at least one. */
  private n(count: number): number {
    return Math.max(1, Math.round(count * this.amount));
  }

  spark(p: ParticleSpec): void {
    this.glow.spawn(p);
  }

  puff(p: ParticleSpec): void {
    this.smoke.spawn(p);
  }

  chunk(d: DebrisSpec): void {
    const i = this.nextChunk;
    this.nextChunk = (this.nextChunk + 1) % this.chunks.length;
    const c = this.chunks[i];
    c.pos.copy(d.pos);
    c.vel.copy(d.vel);
    c.scale.copy(d.scale);
    const spin = d.spin ?? 8;
    c.spin.set((Math.random() - 0.5) * spin, (Math.random() - 0.5) * spin, (Math.random() - 0.5) * spin);
    c.quat.setFromEuler(new THREE.Euler(Math.random() * 6, Math.random() * 6, Math.random() * 6));
    c.age = 0;
    c.life = d.life ?? 8 + Math.random() * 4;
    c.resting = false;
    this.debris.setColorAt(i, this.color.set(d.color));
    if (this.debris.instanceColor) this.debris.instanceColor.needsUpdate = true;
  }

  /** A burst of chunks flying out from a point. */
  shatter(center: THREE.Vector3, count: number, colors: THREE.ColorRepresentation[], size: number, speed: number, up = 1): void {
    for (let i = 0; i < this.n(count); i++) {
      const a = Math.random() * Math.PI * 2;
      const s = speed * (0.4 + Math.random() * 0.8);
      const k = size * (0.4 + Math.random() * 0.9);
      this.chunk({
        pos: new THREE.Vector3(center.x + (Math.random() - 0.5) * size, center.y + Math.random() * size, center.z + (Math.random() - 0.5) * size),
        vel: new THREE.Vector3(Math.cos(a) * s, (2 + Math.random() * 6) * up, Math.sin(a) * s),
        scale: new THREE.Vector3(k, k * (0.4 + Math.random() * 0.7), k * (0.6 + Math.random() * 0.6)),
        color: colors[Math.floor(Math.random() * colors.length)],
      });
    }
  }

  /** A big fiery explosion. `size` is roughly its radius in metres. */
  explosion(at: THREE.Vector3, size: number): void {
    const fb = this.fireballs.find((f) => f.age >= f.life) ?? this.fireballs[0];
    fb.mesh.position.copy(at);
    fb.age = 0;
    fb.life = 0.55;
    fb.size = size * 0.8;
    fb.mesh.visible = true;
    this.flash.position.set(at.x, at.y + 2, at.z);
    this.flashLevel = Math.max(this.flashLevel, 40 * size);

    for (let i = 0; i < this.n(60 * size * 0.35); i++) {
      const d = randomDir().multiplyScalar(size * (3 + Math.random() * 6));
      this.glow.spawn({
        x: at.x, y: at.y, z: at.z,
        vx: d.x, vy: Math.abs(d.y) * 1.2 + 2, vz: d.z,
        life: 0.35 + Math.random() * 0.5,
        size: [size * 1.2, size * 0.2],
        color: [1, 0.85, 0.4], endColor: [1, 0.25, 0.05],
        alpha: [1, 0], drag: 3.5, gravity: -2,
      });
    }
    for (let i = 0; i < this.n(30 * size * 0.35); i++) {
      const d = randomDir().multiplyScalar(18 + Math.random() * 20);
      this.glow.spawn({
        x: at.x, y: at.y, z: at.z,
        vx: d.x, vy: Math.abs(d.y) + 4, vz: d.z,
        life: 0.6 + Math.random() * 0.6,
        size: [0.25, 0.08],
        color: [1, 0.9, 0.6], endColor: [1, 0.4, 0.1],
        gravity: 14, drag: 0.6,
      });
    }
    for (let i = 0; i < this.n(28 * size * 0.35); i++) {
      const d = randomDir().multiplyScalar(size * 1.6);
      const g = 0.12 + Math.random() * 0.12;
      this.smoke.spawn({
        x: at.x + d.x * 0.5, y: at.y + Math.abs(d.y) * 0.5, z: at.z + d.z * 0.5,
        vx: d.x, vy: 1.5 + Math.random() * 2.5, vz: d.z,
        life: 2.5 + Math.random() * 2.5,
        size: [size * 1.5, size * 4.5],
        color: [g, g, g], endColor: [g * 1.8, g * 1.8, g * 1.8],
        alpha: [0.75, 0], gravity: -0.6, drag: 0.8,
      });
    }
    this.scorchMark(at.x, at.z, size * 2.6);
  }

  /** A flat expanding ring, for the shockwave weapon. */
  shockwave(at: THREE.Vector3, radius: number): void {
    const ring = this.rings.find((r) => r.age >= r.life) ?? this.rings[0];
    ring.mesh.position.set(at.x, 0.3, at.z);
    ring.age = 0;
    ring.life = 0.6;
    ring.size = radius;
    ring.mesh.visible = true;
    this.flash.color.set(0x9fdcff);
    this.flash.position.set(at.x, 2, at.z);
    this.flashLevel = Math.max(this.flashLevel, 120);
    for (let i = 0; i < this.n(70); i++) {
      const a = (i / this.n(70)) * Math.PI * 2;
      const s = radius * (2 + Math.random());
      this.smoke.spawn({
        x: at.x + Math.cos(a) * 1.5, y: 0.4, z: at.z + Math.sin(a) * 1.5,
        vx: Math.cos(a) * s, vy: 0.5 + Math.random(), vz: Math.sin(a) * s,
        life: 1.2 + Math.random() * 0.8,
        size: [1.4, 3.5],
        color: [0.65, 0.68, 0.72], alpha: [0.55, 0], drag: 2.8,
      });
    }
  }

  /** Little sparks where a bullet hits something. */
  impact(at: THREE.Vector3, normal: THREE.Vector3, dusty: boolean): void {
    for (let i = 0; i < this.n(7); i++) {
      const d = randomDir().addScaledVector(normal, 1.3).normalize().multiplyScalar(5 + Math.random() * 8);
      this.glow.spawn({
        x: at.x, y: at.y, z: at.z, vx: d.x, vy: d.y, vz: d.z,
        life: 0.15 + Math.random() * 0.25, size: [0.18, 0.05],
        color: [1, 0.85, 0.5], endColor: [1, 0.4, 0.1], gravity: 12,
      });
    }
    if (dusty) {
      const g = 0.55 + Math.random() * 0.15;
      this.smoke.spawn({
        x: at.x + normal.x * 0.2, y: at.y + normal.y * 0.2, z: at.z + normal.z * 0.2,
        vx: normal.x * 1.5, vy: 0.6 + normal.y, vz: normal.z * 1.5,
        life: 0.8 + Math.random() * 0.6, size: [0.4, 1.6],
        color: [g, g * 0.95, g * 0.9], alpha: [0.6, 0], drag: 1.5,
      });
    }
  }

  muzzleFlash(at: THREE.Vector3, dir: THREE.Vector3): void {
    for (let i = 0; i < 3; i++) {
      const s = 2 + Math.random() * 4;
      this.glow.spawn({
        x: at.x, y: at.y, z: at.z, vx: dir.x * s, vy: dir.y * s, vz: dir.z * s,
        life: 0.05 + Math.random() * 0.04, size: [0.45, 0.15], color: [1, 0.85, 0.45],
      });
    }
  }

  /** Smoke and fire rising from something burning, called every frame. */
  burn(at: THREE.Vector3, strength: number, dt: number): void {
    const rate = strength * 26 * this.amount;
    let count = Math.floor(rate * dt + Math.random());
    while (count-- > 0) {
      const jitter = () => (Math.random() - 0.5) * strength * 1.2;
      this.glow.spawn({
        x: at.x + jitter(), y: at.y, z: at.z + jitter(),
        vx: 0, vy: 2 + Math.random() * 2, vz: 0,
        life: 0.4 + Math.random() * 0.4, size: [0.9 * strength, 0.2],
        color: [1, 0.7, 0.25], endColor: [1, 0.2, 0.02], drag: 1,
      });
      if (Math.random() < 0.6) {
        const g = 0.08 + Math.random() * 0.1;
        this.smoke.spawn({
          x: at.x + jitter(), y: at.y + 0.6, z: at.z + jitter(),
          vx: 0.6, vy: 2 + Math.random() * 1.5, vz: 0.3,
          life: 3 + Math.random() * 2, size: [1 * strength, 4 * strength],
          color: [g, g, g], endColor: [0.3, 0.3, 0.32], alpha: [0.6, 0], drag: 0.4,
        });
      }
    }
  }

  /** Light grey dust rolling out along the ground. `scale` makes the clouds bigger. */
  dust(at: THREE.Vector3, spread: number, count: number, scale = 1): void {
    for (let i = 0; i < this.n(count); i++) {
      const a = Math.random() * Math.PI * 2;
      const r = Math.random() * spread;
      const g = 0.5 + Math.random() * 0.2;
      this.smoke.spawn({
        x: at.x + Math.cos(a) * r, y: at.y + Math.random() * 2, z: at.z + Math.sin(a) * r,
        vx: Math.cos(a) * (2 + Math.random() * 4) * scale, vy: 0.5 + Math.random() * 2 * scale, vz: Math.sin(a) * (2 + Math.random() * 4) * scale,
        life: 2 + Math.random() * 2.5 * scale, size: [2 * scale, 6 * scale],
        color: [g * 0.85, g * 0.8, g * 0.72], alpha: [0.75, 0], drag: 1,
      });
    }
  }

  /** Dust billowing out from the edges of a rectangle (a falling building's footprint). */
  dustAround(x: number, z: number, halfW: number, halfD: number, count: number, scale = 1): void {
    for (let i = 0; i < this.n(count); i++) {
      // A random point on the outline, and the direction straight out from it.
      const side = Math.floor(Math.random() * 4);
      const u = Math.random() * 2 - 1;
      const [px, pz, nx, nz] = [
        [x + u * halfW, z + halfD, 0, 1],
        [x + u * halfW, z - halfD, 0, -1],
        [x + halfW, z + u * halfD, 1, 0],
        [x - halfW, z + u * halfD, -1, 0],
      ][side];
      const g = 0.55 + Math.random() * 0.2;
      const s = (3 + Math.random() * 5) * scale;
      this.smoke.spawn({
        x: px + nx * 1.5, y: 0.5 + Math.random() * 3, z: pz + nz * 1.5,
        vx: nx * s + (Math.random() - 0.5) * 2, vy: 0.5 + Math.random() * 2.5, vz: nz * s + (Math.random() - 0.5) * 2,
        life: 3 + Math.random() * 3, size: [3 * scale, 9 * scale],
        color: [g * 0.85, g * 0.8, g * 0.72], alpha: [0.8, 0], drag: 0.9,
      });
    }
  }

  /** A glowing streak for a bullet, from the gun to where it hit. */
  tracer(from: THREE.Vector3, to: THREE.Vector3, life: number): void {
    const i = this.nextTracer;
    this.nextTracer = (this.nextTracer + 1) % this.tracerData.length;
    const t = this.tracerData[i];
    t.from.copy(from);
    t.to.copy(to);
    t.age = 0;
    t.life = Math.max(0.03, life);
  }

  scorchMark(x: number, z: number, size: number): void {
    const s = this.scorch[this.nextScorch];
    this.nextScorch = (this.nextScorch + 1) % this.scorch.length;
    s.mesh.position.set(x, 0.045, z);
    s.mesh.scale.set(size, 1, size);
    s.mesh.rotation.y = Math.random() * Math.PI;
    s.mesh.visible = true;
    s.age = 0;
  }

  update(dt: number): void {
    this.glow.update(dt);
    this.smoke.update(dt);

    // Debris: fall, bounce, settle, then shrink away.
    let dirty = false;
    for (let i = 0; i < this.chunks.length; i++) {
      const c = this.chunks[i];
      if (c.age >= c.life) continue;
      dirty = true;
      c.age += dt;
      if (c.age >= c.life) {
        this.debris.setMatrixAt(i, this.m.makeScale(0, 0, 0));
        continue;
      }
      if (!c.resting) {
        c.vel.y -= 16 * dt;
        c.pos.addScaledVector(c.vel, dt);
        this.q.setFromAxisAngle(this.v.copy(c.spin).normalize(), c.spin.length() * dt);
        c.quat.premultiply(this.q);
        const floor = Math.min(c.scale.x, c.scale.y, c.scale.z) / 2;
        if (c.pos.y < floor) {
          c.pos.y = floor;
          c.vel.y = Math.abs(c.vel.y) * 0.3;
          c.vel.x *= 0.55;
          c.vel.z *= 0.55;
          c.spin.multiplyScalar(0.5);
          if (c.vel.lengthSq() < 1) {
            c.resting = true;
            // Lie flat: keep only the turn around the vertical axis.
            const e = new THREE.Euler().setFromQuaternion(c.quat, 'YXZ');
            const flat = Math.abs(c.scale.y) <= Math.max(c.scale.x, c.scale.z) ? 0 : Math.PI / 2;
            c.quat.setFromEuler(new THREE.Euler(flat, e.y, 0, 'YXZ'));
            c.pos.y = flat ? Math.min(c.scale.x, c.scale.z) / 2 : c.scale.y / 2;
          }
        }
      }
      const fade = Math.min(1, (c.life - c.age) / 1.2);
      this.debris.setMatrixAt(i, this.m.compose(c.pos, c.quat, this.v.copy(c.scale).multiplyScalar(fade)));
    }
    if (dirty) this.debris.instanceMatrix.needsUpdate = true;

    for (const f of this.fireballs) {
      if (f.age >= f.life) continue;
      f.age += dt;
      const t = Math.min(1, f.age / f.life);
      f.mesh.visible = t < 1;
      f.mesh.scale.setScalar(f.size * (0.4 + Math.sqrt(t) * 1.1));
      (f.mesh.material as THREE.MeshBasicMaterial).opacity = (1 - t) * 0.9;
      (f.mesh.material as THREE.MeshBasicMaterial).color.setRGB(1, 0.65 - t * 0.4, 0.25 - t * 0.2);
    }
    for (const r of this.rings) {
      if (r.age >= r.life) continue;
      r.age += dt;
      const t = Math.min(1, r.age / r.life);
      r.mesh.visible = t < 1;
      r.mesh.scale.setScalar(1 + r.size * Math.sqrt(t));
      (r.mesh.material as THREE.MeshBasicMaterial).opacity = (1 - t) * 0.9;
    }
    for (const s of this.scorch) {
      if (!s.mesh.visible) continue;
      s.age += dt;
      const opacity = s.age < 25 ? 0.75 : 0.75 * (1 - (s.age - 25) / 10);
      (s.mesh.material as THREE.MeshBasicMaterial).opacity = Math.max(0, opacity);
      if (opacity <= 0) s.mesh.visible = false;
    }

    let tracersDirty = false;
    for (let i = 0; i < this.tracerData.length; i++) {
      const t = this.tracerData[i];
      if (t.age >= t.life) continue;
      tracersDirty = true;
      t.age += dt;
      if (t.age >= t.life) {
        this.tracers.setMatrixAt(i, this.m.makeScale(0, 0, 0));
        continue;
      }
      // A 4 m streak travelling from the muzzle to the hit point.
      const total = t.from.distanceTo(t.to);
      const head = Math.min(total, (t.age / t.life) * total + 2);
      const tail = Math.max(0, head - 4);
      const dir = this.v.subVectors(t.to, t.from).normalize();
      const start = new THREE.Vector3().copy(t.from).addScaledVector(dir, tail);
      this.q.setFromUnitVectors(new THREE.Vector3(0, 0, 1), dir);
      this.tracers.setMatrixAt(i, this.m.compose(start, this.q, new THREE.Vector3(1, 1, Math.max(0.01, head - tail))));
    }
    if (tracersDirty) this.tracers.instanceMatrix.needsUpdate = true;

    this.flashLevel *= Math.exp(-dt * 9);
    if (this.flashLevel < 0.5) {
      this.flashLevel = 0;
      this.flash.color.set(0xffa040);
    }
    this.flash.intensity = this.flashLevel;
  }

  /** Removes every effect at once (used when the city is rebuilt). */
  clear(): void {
    this.glow.clear();
    this.smoke.clear();
    for (let i = 0; i < this.chunks.length; i++) {
      this.chunks[i].age = this.chunks[i].life = 0;
      this.debris.setMatrixAt(i, this.m.makeScale(0, 0, 0));
    }
    this.debris.instanceMatrix.needsUpdate = true;
    for (const s of this.scorch) s.mesh.visible = false;
  }
}

function randomDir(): THREE.Vector3 {
  const v = new THREE.Vector3(Math.random() - 0.5, Math.random() - 0.5, Math.random() - 0.5);
  if (v.lengthSq() < 1e-6) v.copy(UP);
  return v.normalize();
}
