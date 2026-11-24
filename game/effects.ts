// Explosions and what they leave behind: fire, smoke, sparks, flying debris,
// fireballs, shockwave rings, scorch marks and bullet tracers.
//
// Everything is made once at the start and reused (a "pool"), so blowing things
// up never creates new objects for the garbage collector to clean up later.
import * as THREE from 'three';
import { radialGlowTexture } from '../libs/canvasTexture';

/** Settings for one particle: a soft round blob that moves, grows and fades. */
export interface ParticleOptions {
  at: THREE.Vector3;
  velocity?: THREE.Vector3;
  /** Seconds before it disappears. */
  life: number;
  /** Size in metres when it appears and when it disappears. */
  size: [number, number];
  color: THREE.ColorRepresentation;
  /** Colour it fades to (fire turns from yellow to red). Defaults to `color`. */
  endColor?: THREE.ColorRepresentation;
  /** Starting opacity; it always fades to 0. */
  opacity?: number;
  /** Upward pull in m/s² (smoke rises). Negative values fall like sparks. */
  rise?: number;
  /** How quickly it slows down: the fraction of speed lost each second. */
  drag?: number;
}

interface Particle {
  sprite: THREE.Sprite;
  material: THREE.SpriteMaterial;
  velocity: THREE.Vector3;
  age: number;
  life: number;
  size: [number, number];
  startColor: THREE.Color;
  endColor: THREE.Color;
  opacity: number;
  rise: number;
  drag: number;
}

interface Debris {
  mesh: THREE.Mesh;
  velocity: THREE.Vector3;
  spin: THREE.Vector3;
  age: number;
  life: number;
  resting: boolean;
}

/** A mesh that plays a short animation (fireball, ring, tracer, scorch mark). */
interface Flash {
  mesh: THREE.Mesh;
  age: number;
  life: number;
  size: number;
}

const GLOW_COUNT = 300; // fire, sparks, muzzle flashes
const SMOKE_COUNT = 300;
const DEBRIS_COUNT = 150;

export class Effects {
  /** 0..1: how many particles and chunks each effect makes (the Effects setting). */
  amount = 1;
  private glow: Particle[] = [];
  private smoke: Particle[] = [];
  private nextGlow = 0;
  private nextSmoke = 0;
  private debris: Debris[] = [];
  private nextDebris = 0;
  private fireballs: Flash[] = [];
  private rings: Flash[] = [];
  private tracers: (Flash & { from: THREE.Vector3; to: THREE.Vector3 })[] = [];
  private scorchMarks: Flash[] = [];
  private nextScorch = 0;
  private light: THREE.PointLight;

  constructor(scene: THREE.Scene) {
    const blob = radialGlowTexture();
    const makeParticle = (additive: boolean): Particle => {
      const material = new THREE.SpriteMaterial({
        map: blob,
        transparent: true,
        depthWrite: false,
        // Additive blending makes overlapping fire brighter, like real flames.
        blending: additive ? THREE.AdditiveBlending : THREE.NormalBlending,
      });
      const sprite = new THREE.Sprite(material);
      sprite.visible = false;
      scene.add(sprite);
      return { sprite, material, velocity: new THREE.Vector3(), age: 0, life: 0, size: [1, 1], startColor: new THREE.Color(), endColor: new THREE.Color(), opacity: 1, rise: 0, drag: 0 };
    };
    for (let i = 0; i < GLOW_COUNT; i++) this.glow.push(makeParticle(true));
    for (let i = 0; i < SMOKE_COUNT; i++) this.smoke.push(makeParticle(false));

    // Debris: plain boxes, each with its own material so it can have its own colour.
    const box = new THREE.BoxGeometry(1, 1, 1);
    for (let i = 0; i < DEBRIS_COUNT; i++) {
      const mesh = new THREE.Mesh(box, new THREE.MeshStandardMaterial({ roughness: 0.9 }));
      mesh.castShadow = true;
      mesh.visible = false;
      scene.add(mesh);
      this.debris.push({ mesh, velocity: new THREE.Vector3(), spin: new THREE.Vector3(), age: 0, life: 0, resting: true });
    }

    const glowingMaterial = (color: THREE.ColorRepresentation) =>
      new THREE.MeshBasicMaterial({ color, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false, side: THREE.DoubleSide });
    const makeFlash = (geometry: THREE.BufferGeometry, material: THREE.Material): Flash => {
      const mesh = new THREE.Mesh(geometry, material);
      mesh.visible = false;
      scene.add(mesh);
      return { mesh, age: 0, life: 0, size: 1 };
    };
    for (let i = 0; i < 8; i++) this.fireballs.push(makeFlash(new THREE.IcosahedronGeometry(1, 2), glowingMaterial(0xffa040)));
    for (let i = 0; i < 3; i++) this.rings.push(makeFlash(new THREE.RingGeometry(0.8, 1, 48).rotateX(-Math.PI / 2), glowingMaterial(0x9fdcff)));
    // A tracer is a thin rod 1 m long, pointing along +z; it gets stretched and turned.
    const rod = new THREE.BoxGeometry(0.05, 0.05, 1).translate(0, 0, 0.5);
    for (let i = 0; i < 40; i++) {
      this.tracers.push({ ...makeFlash(rod, glowingMaterial(0xffd27a)), from: new THREE.Vector3(), to: new THREE.Vector3() });
    }
    // Scorch marks: dark blobs on the ground. Polygon offset stops them flickering against the road.
    const flat = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
    for (let i = 0; i < 30; i++) {
      const material = new THREE.MeshBasicMaterial({ color: 0x000000, map: blob, transparent: true, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2 });
      this.scorchMarks.push(makeFlash(flat, material));
    }

    // One light that flashes orange during explosions. It always exists (at zero
    // brightness when idle) because adding or removing lights makes every
    // material in the scene recompile, which causes a stutter.
    this.light = new THREE.PointLight(0xffa040, 0, 40, 1.6);
    scene.add(this.light);
  }

  /** Scales a count by the Effects setting, always making at least one. */
  private count(n: number): number {
    return Math.max(1, Math.round(n * this.amount));
  }

  /** Adds a glowing particle (fire, sparks). Reuses the oldest one when all are busy. */
  addGlow(options: ParticleOptions): void {
    this.start(this.glow[this.nextGlow], options);
    this.nextGlow = (this.nextGlow + 1) % GLOW_COUNT;
  }

  /** Adds a smoke or dust particle. */
  addSmoke(options: ParticleOptions): void {
    this.start(this.smoke[this.nextSmoke], options);
    this.nextSmoke = (this.nextSmoke + 1) % SMOKE_COUNT;
  }

  private start(p: Particle, o: ParticleOptions): void {
    p.sprite.position.copy(o.at);
    p.velocity.copy(o.velocity ?? new THREE.Vector3());
    p.age = 0;
    p.life = o.life;
    p.size = o.size;
    p.startColor.set(o.color);
    p.endColor.set(o.endColor ?? o.color);
    p.opacity = o.opacity ?? 1;
    p.rise = o.rise ?? 0;
    p.drag = o.drag ?? 0;
    p.sprite.visible = true;
  }

  /** Throws a chunk of debris. */
  addDebris(at: THREE.Vector3, velocity: THREE.Vector3, size: THREE.Vector3, color: THREE.ColorRepresentation, spin = 8): void {
    const d = this.debris[this.nextDebris];
    this.nextDebris = (this.nextDebris + 1) % DEBRIS_COUNT;
    d.mesh.position.copy(at);
    d.mesh.scale.copy(size);
    d.mesh.rotation.set(Math.random() * 6, Math.random() * 6, Math.random() * 6);
    (d.mesh.material as THREE.MeshStandardMaterial).color.set(color);
    d.mesh.visible = true;
    d.velocity.copy(velocity);
    d.spin.set(random(spin), random(spin), random(spin));
    d.age = 0;
    d.life = 8 + Math.random() * 4;
    d.resting = false;
  }

  /** A firework: a ball of coloured sparks that fall and fade. */
  firework(at: THREE.Vector3, color: THREE.ColorRepresentation, size = 1): void {
    for (let i = 0; i < this.count(60); i++) {
      // Even directions over a sphere.
      const y = Math.random() * 2 - 1;
      const r = Math.sqrt(1 - y * y);
      const a = Math.random() * Math.PI * 2;
      const v = new THREE.Vector3(Math.cos(a) * r, y, Math.sin(a) * r).multiplyScalar(14 * size * (0.8 + Math.random() * 0.4));
      this.addGlow({ at: at.clone(), velocity: v, life: 1.2 + Math.random() * 0.5, size: [0.6 * size, 0.1 * size], color, endColor: 0xffffff, rise: -5, drag: 1.2 });
    }
  }

  /** Confetti: little paper squares thrown up around a point. */
  confetti(at: THREE.Vector3, count: number): void {
    const colors = [0xff3b3b, 0xffd23f, 0x3bb273, 0x1b98e0, 0xc62f9e, 0xffffff];
    for (let i = 0; i < this.count(count); i++) {
      const a = Math.random() * Math.PI * 2;
      const s = 2 + Math.random() * 5;
      this.addDebris(
        new THREE.Vector3(at.x + random(1), at.y + 1, at.z + random(1)),
        new THREE.Vector3(Math.cos(a) * s, 6 + Math.random() * 8, Math.sin(a) * s),
        new THREE.Vector3(0.18, 0.02, 0.12),
        colors[i % colors.length],
        14,
      );
    }
  }

  /** A burst of debris flying out from a point, in the given colours. */
  shatter(center: THREE.Vector3, count: number, colors: THREE.ColorRepresentation[], size: number, speed: number): void {
    for (let i = 0; i < this.count(count); i++) {
      const angle = Math.random() * Math.PI * 2;
      const s = speed * (0.4 + Math.random() * 0.8);
      const k = size * (0.4 + Math.random() * 0.9);
      this.addDebris(
        new THREE.Vector3(center.x + random(size), center.y + Math.random() * size, center.z + random(size)),
        new THREE.Vector3(Math.cos(angle) * s, 2 + Math.random() * 6, Math.sin(angle) * s),
        new THREE.Vector3(k, k * (0.4 + Math.random() * 0.7), k * (0.6 + Math.random() * 0.6)),
        colors[Math.floor(Math.random() * colors.length)],
      );
    }
  }

  /** A fiery explosion. `size` is roughly its radius in metres. */
  explosion(at: THREE.Vector3, size: number): void {
    this.play(this.fireballs, at, size * 0.8, 0.55);
    this.light.color.set(0xffa040);
    this.light.position.set(at.x, at.y + 2, at.z);
    this.light.intensity = Math.max(this.light.intensity, 40 * size);

    // Fire: big blobs thrown outwards that shrink and turn from yellow to red.
    for (let i = 0; i < this.count(16); i++) {
      this.addGlow({
        at,
        velocity: randomDirection().multiplyScalar(size * (3 + Math.random() * 4)).setY(2 + Math.random() * size * 4),
        life: 0.35 + Math.random() * 0.4,
        size: [size * 1.6, size * 0.3],
        color: 0xffd966,
        endColor: 0xff4010,
        drag: 3.5,
        rise: 2,
      });
    }
    // Sparks: small, fast, and they fall.
    for (let i = 0; i < this.count(12); i++) {
      this.addGlow({
        at,
        velocity: randomDirection().multiplyScalar(18 + Math.random() * 18).setY(4 + Math.random() * 12),
        life: 0.6 + Math.random() * 0.6,
        size: [0.3, 0.1],
        color: 0xffe6a0,
        endColor: 0xff6020,
        rise: -14,
        drag: 0.6,
      });
    }
    // Smoke: dark clouds that rise, grow and fade.
    for (let i = 0; i < this.count(14); i++) {
      this.addSmoke({
        at: at.clone().add(randomDirection().multiplyScalar(size)),
        velocity: randomDirection().multiplyScalar(size * 1.5).setY(1.5 + Math.random() * 2.5),
        life: 2.5 + Math.random() * 2.5,
        size: [size * 2, size * 5],
        color: 0x303030,
        endColor: 0x606060,
        opacity: 0.75,
        rise: 0.6,
        drag: 0.8,
      });
    }
    this.scorch(at.x, at.z, size * 2.6);
  }

  /** A flat blue ring rushing outwards, for the shockwave weapon. */
  shockwave(at: THREE.Vector3, radius: number): void {
    this.play(this.rings, new THREE.Vector3(at.x, 0.3, at.z), radius, 0.6);
    this.light.color.set(0x9fdcff);
    this.light.position.set(at.x, 2, at.z);
    this.light.intensity = 120;
    this.dust(at.x, at.z, 1.5, 1.5, 40, 0.5);
  }

  /** Sparks (and a puff of dust on walls and the road) where a bullet hits. */
  bulletHit(at: THREE.Vector3, dusty: boolean): void {
    for (let i = 0; i < this.count(5); i++) {
      this.addGlow({
        at,
        velocity: randomDirection().multiplyScalar(5 + Math.random() * 8).setY(2 + Math.random() * 5),
        life: 0.15 + Math.random() * 0.25,
        size: [0.2, 0.05],
        color: 0xffd98a,
        endColor: 0xff6a1a,
        rise: -12,
      });
    }
    if (dusty) {
      this.addSmoke({ at, velocity: new THREE.Vector3(0, 0.8, 0), life: 1, size: [0.4, 1.6], color: 0x9a948a, opacity: 0.6, drag: 1.5 });
    }
  }

  /** A quick flash at the end of a gun barrel. */
  muzzleFlash(at: THREE.Vector3, direction: THREE.Vector3): void {
    this.addGlow({ at, velocity: direction.clone().multiplyScalar(4), life: 0.07, size: [0.5, 0.15], color: 0xffd98a });
  }

  /**
   * Fire and smoke rising from something burning. Called every frame, so it
   * uses `dt` (seconds since the last frame) to keep the same rate at any FPS.
   */
  burn(at: THREE.Vector3, strength: number, dt: number): void {
    const perSecond = 20 * strength * this.amount;
    if (Math.random() > perSecond * dt) return;
    const spot = new THREE.Vector3(at.x + random(strength), at.y, at.z + random(strength));
    this.addGlow({
      at: spot,
      velocity: new THREE.Vector3(0, 2 + Math.random() * 2, 0),
      life: 0.5,
      size: [strength, 0.2],
      color: 0xffb340,
      endColor: 0xff3005,
      drag: 1,
    });
    this.addSmoke({
      at: spot.clone().setY(at.y + 0.6),
      velocity: new THREE.Vector3(0.6, 2 + Math.random() * 1.5, 0.3),
      life: 3 + Math.random() * 2,
      size: [strength, 4 * strength],
      color: 0x202020,
      endColor: 0x505052,
      opacity: 0.6,
      drag: 0.4,
    });
  }

  /**
   * Dust billowing out from the edges of a rectangle on the ground: the
   * footprint of a falling building, or around a car that lands hard.
   */
  dust(x: number, z: number, halfWidth: number, halfDepth: number, count: number, scale = 1): void {
    for (let i = 0; i < this.count(count); i++) {
      // Pick one of the four sides and a random point along it,
      // then send the dust straight out from that side.
      const along = Math.random() * 2 - 1;
      const sides = [
        { x: x + along * halfWidth, z: z + halfDepth, outX: 0, outZ: 1 },
        { x: x + along * halfWidth, z: z - halfDepth, outX: 0, outZ: -1 },
        { x: x + halfWidth, z: z + along * halfDepth, outX: 1, outZ: 0 },
        { x: x - halfWidth, z: z + along * halfDepth, outX: -1, outZ: 0 },
      ];
      const side = sides[Math.floor(Math.random() * 4)];
      const speed = (3 + Math.random() * 5) * scale;
      this.addSmoke({
        at: new THREE.Vector3(side.x + side.outX, 0.5 + Math.random() * 2 * scale, side.z + side.outZ),
        velocity: new THREE.Vector3(side.outX * speed + random(2), 0.5 + Math.random() * 2, side.outZ * speed + random(2)),
        life: 3 + Math.random() * 3,
        size: [3 * scale, 9 * scale],
        color: 0x8c857a,
        opacity: 0.8,
        drag: 0.9,
      });
    }
  }

  /** A glowing streak flying from the gun to where the bullet hit. */
  tracer(from: THREE.Vector3, to: THREE.Vector3): void {
    const t = this.tracers.find((x) => !x.mesh.visible) ?? this.tracers[0];
    t.from.copy(from);
    t.to.copy(to);
    t.age = 0;
    t.life = Math.max(0.04, from.distanceTo(to) / 400); // bullets travel at 400 m/s
    t.mesh.visible = true;
  }

  /** A dark burn mark on the ground. */
  scorch(x: number, z: number, size: number): void {
    const s = this.scorchMarks[this.nextScorch];
    this.nextScorch = (this.nextScorch + 1) % this.scorchMarks.length;
    s.mesh.position.set(x, 0.045, z);
    s.mesh.scale.set(size, 1, size);
    s.mesh.rotation.y = Math.random() * Math.PI;
    s.age = 0;
    s.life = 35;
    s.mesh.visible = true;
  }

  /** Starts an animation on a free mesh from a pool (or the first one if all are busy). */
  private play(pool: Flash[], at: THREE.Vector3, size: number, life: number): void {
    const f = pool.find((x) => !x.mesh.visible) ?? pool[0];
    f.mesh.position.copy(at);
    f.size = size;
    f.age = 0;
    f.life = life;
    f.mesh.visible = true;
  }

  /** Moves everything on by `dt` seconds. */
  update(dt: number): void {
    for (const p of [...this.glow, ...this.smoke]) {
      if (!p.sprite.visible) continue;
      p.age += dt;
      if (p.age >= p.life) {
        p.sprite.visible = false;
        continue;
      }
      const t = p.age / p.life; // 0 when it appears, 1 when it disappears
      p.velocity.multiplyScalar(Math.max(0, 1 - p.drag * dt));
      p.velocity.y += p.rise * dt;
      p.sprite.position.addScaledVector(p.velocity, dt);
      p.sprite.position.y = Math.max(0.05, p.sprite.position.y);
      p.sprite.scale.setScalar(p.size[0] + (p.size[1] - p.size[0]) * t);
      p.material.color.lerpColors(p.startColor, p.endColor, t);
      p.material.opacity = p.opacity * (1 - t);
    }

    // Debris falls, bounces, comes to rest, then shrinks away.
    for (const d of this.debris) {
      if (!d.mesh.visible) continue;
      d.age += dt;
      if (d.age >= d.life) {
        d.mesh.visible = false;
        continue;
      }
      const mesh = d.mesh;
      if (!d.resting) {
        d.velocity.y -= 16 * dt; // gravity
        mesh.position.addScaledVector(d.velocity, dt);
        mesh.rotation.x += d.spin.x * dt;
        mesh.rotation.y += d.spin.y * dt;
        mesh.rotation.z += d.spin.z * dt;
        const halfHeight = Math.min(mesh.scale.x, mesh.scale.y, mesh.scale.z) / 2;
        if (mesh.position.y < halfHeight) {
          // Hit the ground: bounce, losing most of the speed.
          mesh.position.y = halfHeight;
          d.velocity.set(d.velocity.x * 0.5, Math.abs(d.velocity.y) * 0.3, d.velocity.z * 0.5);
          d.spin.multiplyScalar(0.5);
          if (d.velocity.length() < 1) {
            // Slow enough: lie flat on the ground.
            d.resting = true;
            mesh.rotation.set(0, mesh.rotation.y, 0);
            mesh.position.y = mesh.scale.y / 2;
          }
        }
      }
      // Shrink away during the last second.
      const left = d.life - d.age;
      if (left < 1) mesh.scale.multiplyScalar(Math.max(0, 1 - dt / left));
    }

    // Fireballs swell and fade from orange to red.
    for (const f of this.fireballs) {
      if (!this.advance(f, dt)) continue;
      const t = f.age / f.life;
      f.mesh.scale.setScalar(f.size * (0.4 + Math.sqrt(t) * 1.1));
      const material = f.mesh.material as THREE.MeshBasicMaterial;
      material.opacity = (1 - t) * 0.9;
      material.color.setRGB(1, 0.65 - t * 0.4, 0.25 - t * 0.2);
    }
    // Shockwave rings rush outwards and fade.
    for (const r of this.rings) {
      if (!this.advance(r, dt)) continue;
      const t = r.age / r.life;
      r.mesh.scale.setScalar(1 + r.size * Math.sqrt(t));
      (r.mesh.material as THREE.MeshBasicMaterial).opacity = (1 - t) * 0.9;
    }
    // Scorch marks stay a while, then fade over their last 10 seconds.
    for (const s of this.scorchMarks) {
      if (!this.advance(s, dt)) continue;
      (s.mesh.material as THREE.MeshBasicMaterial).opacity = 0.75 * Math.min(1, (s.life - s.age) / 10);
    }
    // Tracers: a 4 m streak whose front travels from the gun to the hit point.
    for (const t of this.tracers) {
      if (!this.advance(t, dt)) continue;
      const length = t.from.distanceTo(t.to);
      const front = Math.min(length, (t.age / t.life) * length + 2);
      const back = Math.max(0, front - 4);
      const direction = t.to.clone().sub(t.from).normalize();
      t.mesh.position.copy(t.from).addScaledVector(direction, back);
      t.mesh.lookAt(t.mesh.position.clone().add(direction));
      t.mesh.scale.set(1, 1, Math.max(0.01, front - back));
    }

    // The explosion light dies down quickly.
    this.light.intensity *= Math.exp(-dt * 9);
    if (this.light.intensity < 0.5) this.light.intensity = 0;
  }

  /** Ages an animation by `dt`. Hides it and returns false once it has finished. */
  private advance(f: Flash, dt: number): boolean {
    if (!f.mesh.visible) return false;
    f.age += dt;
    if (f.age >= f.life) f.mesh.visible = false;
    return f.mesh.visible;
  }

  /** Removes every effect at once (when the city is rebuilt). */
  clear(): void {
    for (const p of [...this.glow, ...this.smoke]) p.sprite.visible = false;
    for (const d of this.debris) d.mesh.visible = false;
    for (const f of [...this.fireballs, ...this.rings, ...this.tracers, ...this.scorchMarks]) f.mesh.visible = false;
  }
}

/** A random number between -range/2 and +range/2. */
function random(range: number): number {
  return (Math.random() - 0.5) * range;
}

/** A random direction, as a vector of length 1. */
function randomDirection(): THREE.Vector3 {
  return new THREE.Vector3(random(2), random(2), random(2)).normalize();
}
