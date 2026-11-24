// Surfaces: what the ground and the cars are made of.
//  - Ground (roads, grass, sand, snow) gets detail in world space: patches,
//    fine grain and tiny bumps, so it never looks like a flat colour. In the
//    rain it's wet: darker, shiny, with puddles that reflect the sky.
//  - Cars get a soft contact shadow underneath and metal flakes in the paint.
import * as THREE from 'three';
import type { CarRig } from '../../game/carModel';

/** Shared by every ground material, so the weather changes them all at once. */
export const groundUniforms = {
  wetness: { value: 0 },
  detail: { value: 1 },
};

/** The shader code added to ground materials. */
const NOISE = /* glsl */ `
  varying vec3 vGroundPos;
  uniform float wetness;
  uniform float detail;
  float gHash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
  float gNoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(gHash(i), gHash(i + vec2(1, 0)), u.x), mix(gHash(i + vec2(0, 1)), gHash(i + vec2(1, 1)), u.x), u.y);
  }
  float gFbm(vec2 p) {
    float sum = 0.0, amp = 0.5;
    for (int i = 0; i < 4; i++) { sum += gNoise(p) * amp; p *= 2.03; amp *= 0.5; }
    return sum;
  }
`;

const patched = new Map<THREE.Material, { onBeforeCompile: THREE.Material['onBeforeCompile']; key: THREE.Material['customProgramCacheKey'] }>();

/** Adds the ground detail to one material (once). */
function patchGround(material: THREE.MeshStandardMaterial): void {
  if (patched.has(material)) return;
  patched.set(material, { onBeforeCompile: material.onBeforeCompile, key: material.customProgramCacheKey });
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, groundUniforms);
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying vec3 vGroundPos;')
      .replace('#include <project_vertex>', '#include <project_vertex>\nvGroundPos = (modelMatrix * vec4(transformed, 1.0)).xyz;');
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\n' + NOISE)
      // Colour: big patches (wear, dirt, oil) and fine grain; darker when wet.
      .replace(
        '#include <color_fragment>',
        `#include <color_fragment>
        vec2 gp = vGroundPos.xz;
        float patches = gFbm(gp * 0.12);
        float grain = gNoise(gp * 4.0);
        diffuseColor.rgb *= 1.0 + ((patches - 0.5) * 0.3 + (grain - 0.5) * 0.06) * detail;
        float puddle = smoothstep(0.58, 0.68, gFbm(gp * 0.07 + 13.0)) * wetness;
        diffuseColor.rgb *= mix(1.0, 0.62, wetness) * mix(1.0, 0.75, puddle);`,
      )
      // Roughness: worn smooth in places; wet surfaces shine, puddles are mirrors.
      .replace(
        '#include <roughnessmap_fragment>',
        `#include <roughnessmap_fragment>
        roughnessFactor = clamp(roughnessFactor + (grain - 0.5) * 0.08 * detail - (patches - 0.5) * 0.12 * detail, 0.04, 1.0);
        roughnessFactor = mix(roughnessFactor, roughnessFactor * 0.6, wetness);
        roughnessFactor = mix(roughnessFactor, 0.15, puddle);`,
      )
      // Tiny bumps (not in puddles, which are flat), from the slope of the fine noise.
      .replace(
        '#include <normal_fragment_maps>',
        `#include <normal_fragment_maps>
        {
          vec2 q = gp * 6.0;
          float h = gNoise(q);
          vec3 slope = vec3(gNoise(q + vec2(0.15, 0.0)) - h, 0.0, gNoise(q + vec2(0.0, 0.15)) - h);
          vec3 tilt = (viewMatrix * vec4(slope, 0.0)).xyz;
          // Fades with distance, where the bumps would only shimmer.
          float near = 1.0 - smoothstep(15.0, 60.0, length(vGroundPos - cameraPosition));
          normal = normalize(normal - tilt * 0.35 * detail * near * (1.0 - puddle));
        }`,
      );
  };
  // Ground materials share one compiled shader, apart from the game's other materials.
  material.customProgramCacheKey = () => 'graphics-pack-ground';
  material.needsUpdate = true;
}

const box = new THREE.Box3();
const boxSize = new THREE.Vector3();

/**
 * Finds the ground in a freshly built map: big, flat, solid surfaces (roads,
 * grass, the paddock), and gives them detail.
 */
export function detailGround(root: THREE.Object3D): void {
  root.updateMatrixWorld(true);
  root.traverse((object) => {
    const mesh = object as THREE.Mesh;
    if (!mesh.isMesh || (mesh as THREE.InstancedMesh).isInstancedMesh) return;
    const material = mesh.material as THREE.MeshStandardMaterial;
    if (Array.isArray(material) || !material.isMeshStandardMaterial || material.transparent) return;
    box.setFromObject(mesh).getSize(boxSize);
    if (boxSize.y < 0.6 && boxSize.x * boxSize.z > 40) patchGround(material);
  });
}

/** Takes the detail off every ground material. */
export function undoGround(): void {
  for (const [material, saved] of patched) {
    material.onBeforeCompile = saved.onBeforeCompile;
    material.customProgramCacheKey = saved.key;
    material.needsUpdate = true;
  }
  patched.clear();
}

/** Sharper textures at a glancing angle (roads into the distance), for every texture under `root`. */
export function sharpenTextures(root: THREE.Object3D, anisotropy: number): void {
  root.traverse((object) => {
    const material = (object as THREE.Mesh).material as THREE.MeshStandardMaterial | undefined;
    if (!material || Array.isArray(material)) return;
    for (const texture of [material.map, material.normalMap, material.roughnessMap]) {
      if (texture && texture.anisotropy < anisotropy) {
        texture.anisotropy = anisotropy;
        texture.needsUpdate = true;
      }
    }
  });
}

// ---------- cars ----------

let shadowTexture: THREE.Texture | null = null;
let flakeTexture: THREE.Texture | null = null;
const shadowGeometry = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
const SHADOW_NAME = 'graphics-pack-contact-shadow';

/** A soft dark oval, darkest in the middle: the shade right under a car. */
function contactShadowTexture(): THREE.Texture {
  if (shadowTexture) return shadowTexture;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 128;
  const ctx = canvas.getContext('2d')!;
  const gradient = ctx.createRadialGradient(64, 64, 4, 64, 64, 64);
  gradient.addColorStop(0, 'rgba(0,0,0,0.9)');
  gradient.addColorStop(0.55, 'rgba(0,0,0,0.55)');
  gradient.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, 128, 128);
  shadowTexture = new THREE.CanvasTexture(canvas);
  shadowTexture.colorSpace = THREE.SRGBColorSpace;
  return shadowTexture;
}

/** Tiny random bumps: the metal flakes that sparkle in metallic paint. */
function flakeNormals(): THREE.Texture {
  if (flakeTexture) return flakeTexture;
  const size = 256;
  const data = new Uint8Array(size * size * 4);
  for (let i = 0; i < size * size; i++) {
    data[i * 4] = 128 + (Math.random() - 0.5) * 200;
    data[i * 4 + 1] = 128 + (Math.random() - 0.5) * 200;
    data[i * 4 + 2] = 255;
    data[i * 4 + 3] = 255;
  }
  flakeTexture = new THREE.DataTexture(data, size, size);
  flakeTexture.wrapS = flakeTexture.wrapT = THREE.RepeatWrapping;
  flakeTexture.repeat.set(24, 24);
  flakeTexture.generateMipmaps = true;
  flakeTexture.minFilter = THREE.LinearMipmapLinearFilter;
  flakeTexture.needsUpdate = true;
  return flakeTexture;
}

/**
 * Dresses a newly built car: a contact shadow under it that stays on the
 * ground (and fades as the car jumps), and flakes in metallic paint.
 */
export function dressCar(rig: CarRig, groundHeight: (x: number, z: number) => number): void {
  const material = new THREE.MeshBasicMaterial({
    map: contactShadowTexture(),
    transparent: true,
    depthWrite: false,
    polygonOffset: true,
    polygonOffsetFactor: -2,
    polygonOffsetUnits: -2,
    fog: true,
  });
  const shadow = new THREE.Mesh(shadowGeometry, material);
  const scale = rig.root.scale.x || 1;
  shadow.scale.set((rig.width * 1.3) / scale, 1, (rig.length * 1.12) / scale);
  shadow.renderOrder = 1;
  shadow.name = SHADOW_NAME;
  // Just before drawing: drop it to the ground under the car, and fade it with height.
  shadow.onBeforeRender = () => {
    const m = shadow.matrixWorld.elements;
    const ground = groundHeight(m[12], m[14]);
    const height = rig.root.position.y - ground;
    material.opacity = Math.max(0, 1 - height / 2.5) * 0.85;
    m[13] = ground + 0.03;
  };
  rig.root.add(shadow);
  const dispose = rig.dispose.bind(rig);
  rig.dispose = () => {
    material.dispose();
    dispose();
  };

  rig.body.traverse((object) => {
    const paint = (object as THREE.Mesh).material as THREE.MeshPhysicalMaterial | undefined;
    if (!paint || Array.isArray(paint) || !paint.isMeshPhysicalMaterial || paint.map || paint.normalMap) return;
    // Metallic paint: flakes under a smooth clear coat.
    if (paint.metalness >= 0.4 && paint.clearcoat > 0 && paint.roughness > 0.1) {
      paint.normalMap = flakeNormals();
      paint.normalScale.set(0.12, 0.12);
      paint.clearcoatRoughness = Math.min(paint.clearcoatRoughness, 0.04);
      paint.needsUpdate = true;
    }
  });
}

/** Takes the contact shadows and flakes off every car. */
export function undressCars(scene: THREE.Scene): void {
  const found: THREE.Mesh[] = [];
  scene.traverse((object) => {
    if (object.name === SHADOW_NAME) found.push(object as THREE.Mesh);
    const paint = (object as THREE.Mesh).material as THREE.MeshPhysicalMaterial | undefined;
    if (paint && !Array.isArray(paint) && flakeTexture && paint.normalMap === flakeTexture) {
      paint.normalMap = null;
      paint.needsUpdate = true;
    }
  });
  for (const shadow of found) {
    shadow.removeFromParent();
    (shadow.material as THREE.Material).dispose();
  }
}
