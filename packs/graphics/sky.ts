// The sky: a physically based atmosphere with drifting clouds by day and a
// starry sky at night, and the light it casts (image-based lighting). The
// same sky is turned into an environment map, so every shiny surface (car
// paint, glass, wet roads) reflects it and everything is lit by it.
import * as THREE from 'three';
import { Sky } from 'three/addons/objects/Sky.js';
import type { Theme } from '../../game/venues';
import { seededRandom } from '../../libs/random';

/** How the sky looks at one venue. */
interface Weather {
  turbidity: number; // haze: 2 is a clear mountain day, 10 is murky
  rayleigh: number; // how blue the sky is
  clouds: number; // 0 clear, 1 overcast
  cloudDensity: number;
  /** How bright the sun is (overcast days are dull), and how much the sky's light makes up for it. */
  sun: number;
  /** How bright the sky is drawn (gray skies are dimmer). */
  exposure: number;
}

/** The sky at a venue, from its theme: clear, hazy desert, or gray with rain or snow. */
export function weatherFor(theme: Theme): Weather {
  if (theme.weather === 'rain') return { turbidity: 10, rayleigh: 0.6, clouds: 0.92, cloudDensity: 0.9, sun: 0.45, exposure: 0.2 };
  if (theme.weather === 'snow') return { turbidity: 8, rayleigh: 1.2, clouds: 0.8, cloudDensity: 0.7, sun: 0.6, exposure: 0.26 };
  if (theme.ground === 'sand') return { turbidity: 6, rayleigh: 0.9, clouds: 0.12, cloudDensity: 0.3, sun: 1.05, exposure: 0.36 };
  return { turbidity: 2, rayleigh: 1.4, clouds: 0.38, cloudDensity: 0.45, sun: 1, exposure: 0.4 };
}

/** Where the sun is at each time of day: height above the horizon, degrees. */
export const SUN_HEIGHT = { morning: 24, noon: 58, golden: 9 } as const;
export type TimeOfDay = keyof typeof SUN_HEIGHT;

export class SkySystem {
  /** The sky dome, drawn behind everything and following the camera. */
  readonly sky: Sky;
  private stars: THREE.Points;
  private pmrem: THREE.PMREMGenerator;
  private envTarget: THREE.WebGLRenderTarget | null = null;
  /** A small scene for making the environment map: the sky, the ground under it, and city glow at night. */
  private envScene = new THREE.Scene();
  private envSky: Sky;
  private envGround: THREE.Mesh;
  private cityGlow: THREE.Group;
  private time = 0;

  constructor(renderer: THREE.WebGLRenderer) {
    this.sky = makeSky();
    this.sky.scale.setScalar(900);
    this.sky.frustumCulled = false;
    this.sky.renderOrder = -1;
    this.envSky = makeSky();
    this.envSky.scale.setScalar(50);
    // Under the horizon the environment sees the ground, which bounces light back up.
    this.envGround = new THREE.Mesh(new THREE.CircleGeometry(40, 32), new THREE.MeshBasicMaterial({ color: 0x3a3a34 }));
    this.envGround.rotation.x = -Math.PI / 2;
    this.envGround.position.y = -2;
    // Warm and colored lights low on the horizon at night, so paint and wet roads catch them.
    this.cityGlow = new THREE.Group();
    for (let i = 0; i < 28; i++) {
      const a = (i / 28) * Math.PI * 2 + Math.sin(i * 7.3) * 0.1;
      const glow = new THREE.Mesh(new THREE.PlaneGeometry(2 + (i % 3), 0.8 + (i % 4) * 0.5), new THREE.MeshBasicMaterial({ side: THREE.DoubleSide }));
      glow.position.set(Math.cos(a) * 30, 0.5 + (i % 5) * 0.6, Math.sin(a) * 30);
      glow.lookAt(0, glow.position.y, 0);
      this.cityGlow.add(glow);
    }
    this.envScene.add(this.envSky, this.envGround, this.cityGlow);
    this.stars = makeStars();
    this.pmrem = new THREE.PMREMGenerator(renderer);
  }

  /**
   * Sets the sky for a venue and time, and makes a new environment map.
   * `sunDirection` points towards the sun.
   */
  update(theme: Theme, night: boolean, sunDirection: THREE.Vector3, ground: THREE.Color): THREE.Texture {
    const weather = weatherFor(theme);
    for (const sky of [this.sky, this.envSky]) {
      const u = (sky.material as THREE.ShaderMaterial).uniforms;
      u.turbidity.value = weather.turbidity;
      u.rayleigh.value = weather.rayleigh;
      // A small, tight glow round the sun (a big one washes out half the sky).
      u.mieCoefficient.value = 0.003;
      u.mieDirectionalG.value = weather.clouds > 0.7 ? 0.55 : 0.75;
      u.exposure.value = weather.exposure;
      u.cloudCoverage.value = weather.clouds;
      u.cloudDensity.value = weather.cloudDensity;
      u.cloudElevation.value = 0.55;
      u.sunPosition.value.copy(sunDirection).multiplyScalar(450000);
    }
    this.sky.visible = !night;
    this.envSky.visible = !night;
    this.cityGlow.visible = night;
    // Neon city glow is pink and blue; elsewhere it's sodium orange and white.
    const neon = theme.scenery === 'city' && theme.night;
    this.cityGlow.children.forEach((child, i) => {
      const color = neon ? [0xff3fb4, 0x3fd8ff, 0x9a5cff][i % 3] : [0xffb46b, 0xfff0d8, 0xffc98a][i % 3];
      ((child as THREE.Mesh).material as THREE.MeshBasicMaterial).color.setHex(color).multiplyScalar(neon ? 3 : 1.6);
    });
    this.envScene.background = night ? new THREE.Color(0x05070f) : null;
    (this.envGround.material as THREE.MeshBasicMaterial).color.copy(ground).multiplyScalar(night ? 0.04 : 0.25);

    // The new environment map, blurred into the levels for rough and shiny surfaces.
    this.envTarget?.dispose();
    this.envTarget = this.pmrem.fromScene(this.envScene, 0, 0.1, 200);
    return this.envTarget.texture;
  }

  /** Adds the sky and stars to the scene. */
  attach(scene: THREE.Scene): void {
    scene.add(this.sky, this.stars);
  }

  /** Each frame: the sky follows the camera, the clouds drift, and stars come out at night. */
  frame(dt: number, camera: THREE.Camera, night: boolean): void {
    this.time += dt;
    (this.sky.material as THREE.ShaderMaterial).uniforms.time.value = this.time;
    this.sky.position.copy(camera.position);
    this.stars.position.copy(camera.position);
    this.stars.visible = night;
  }

  dispose(): void {
    this.sky.removeFromParent();
    this.stars.removeFromParent();
    for (const object of [this.sky, this.envSky, this.stars, this.envGround, ...this.cityGlow.children]) {
      const mesh = object as THREE.Mesh;
      mesh.geometry.dispose();
      (mesh.material as THREE.Material).dispose();
    }
    this.envTarget?.dispose();
    this.pmrem.dispose();
  }
}

/**
 * The sky addon, a little darker: its sky is as bright as the real thing, and
 * the game's lights are set up for a camera exposed for the ground.
 */
function makeSky(): Sky {
  const sky = new Sky();
  const material = sky.material as THREE.ShaderMaterial;
  material.uniforms.exposure = { value: 0.55 };
  material.fragmentShader = material.fragmentShader
    .replace('uniform float time;', 'uniform float time;\nuniform float exposure;')
    .replace('gl_FragColor = vec4( texColor, 1.0 );', 'gl_FragColor = vec4( texColor * exposure, 1.0 );');
  material.fog = false;
  return sky;
}

/** A dome of stars: a few thousand points, brighter and fewer near the top. */
function makeStars(): THREE.Points {
  const count = 2500;
  const positions = new Float32Array(count * 3);
  const colors = new Float32Array(count * 3);
  const random = seededRandom(7);
  for (let i = 0; i < count; i++) {
    // Evenly over the upper half of a sphere.
    const y = 0.03 + random() * 0.97;
    const a = random() * Math.PI * 2;
    const r = Math.sqrt(1 - y * y);
    positions.set([Math.cos(a) * r * 800, y * 800, Math.sin(a) * r * 800], i * 3);
    const brightness = 0.35 + random() ** 4 * 0.65;
    const warm = random();
    colors.set([brightness * (0.85 + warm * 0.15), brightness * 0.92, brightness * (1.1 - warm * 0.2)], i * 3);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  const stars = new THREE.Points(
    geometry,
    new THREE.PointsMaterial({ size: 1.6, sizeAttenuation: false, vertexColors: true, fog: false, depthWrite: false, transparent: true }),
  );
  stars.frustumCulled = false;
  stars.renderOrder = -1;
  return stars;
}
