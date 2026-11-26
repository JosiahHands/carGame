// Light beams: at night, every car's headlights shine visible cones of light
// through the air, brighter in rain and snow (the light catches the drops),
// like headlights on a real misty night.
import * as THREE from 'three';
import type { CarRig } from '../../game/carModel';

const BEAMS = 'photoRealBeams';
const LENGTH = 13; // meters
const SPREAD = 2.4; // radius at the far end, meters

/** How strong the beams are: the "Light beams" setting times the weather. Shared by every car. */
export const beamStrength = { value: 0.5 };

const beamShader = {
  vertexShader: /* glsl */ `
    varying float vAlong;
    varying vec3 vNormal;
    varying vec3 vView;
    void main() {
      vAlong = 1.0 - uv.y; // 0 at the lamp, 1 at the far end
      vec4 world = modelMatrix * vec4(position, 1.0);
      vNormal = normalize(mat3(modelMatrix) * normal);
      vView = normalize(cameraPosition - world.xyz);
      gl_Position = projectionMatrix * viewMatrix * world;
    }`,
  fragmentShader: /* glsl */ `
    uniform vec3 color;
    uniform float strength;
    varying float vAlong;
    varying vec3 vNormal;
    varying vec3 vView;
    void main() {
      // Soft at the edges of the cone (seen side on) and fading out along it.
      float facing = abs(dot(normalize(vNormal), normalize(vView)));
      float glow = pow(facing, 1.5) * pow(1.0 - vAlong, 2.0) * smoothstep(0.0, 0.08, vAlong);
      gl_FragColor = vec4(color * glow * strength * 0.35, 1.0);
    }`,
};

/** One cone, its point at the origin, opening towards +z and tipped a little down at the road. */
function coneGeometry(): THREE.BufferGeometry {
  const geometry = new THREE.ConeGeometry(SPREAD, LENGTH, 24, 6, true);
  geometry.rotateX(-Math.PI / 2); // point (top) back at the lamp
  geometry.translate(0, 0, LENGTH / 2);
  geometry.rotateX(0.07);
  return geometry;
}

/** Adds headlight beams to a car. They show only at night, while the car isn't a wreck. */
export function addBeams(rig: CarRig): void {
  if (rig.root.userData[BEAMS]) return;
  const head = rig.body.children.find((c): c is THREE.Mesh => c.name === 'headlight' && (c as THREE.Mesh).isMesh);
  if (!head) return;
  const lamp = head.material as THREE.MeshStandardMaterial;
  const material = new THREE.ShaderMaterial({
    ...beamShader,
    uniforms: {
      color: { value: lamp.emissive.clone() },
      strength: beamStrength,
    },
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
    side: THREE.DoubleSide,
  });
  const geometry = coneGeometry();
  const cones: THREE.Mesh[] = [];
  for (const side of [-1, 1]) {
    const cone = new THREE.Mesh(geometry, material);
    cone.position.set(side * Math.abs(head.position.x), head.position.y, head.position.z + 0.05);
    cone.castShadow = false;
    cone.receiveShadow = false;
    cone.renderOrder = 2;
    rig.body.add(cone);
    cones.push(cone);
  }

  // Shown at night (the game turns the headlights up), hidden on a wreck.
  let night = lamp.emissiveIntensity > 1.5;
  let wrecked = false;
  const show = () => cones.forEach((c) => (c.visible = night && !wrecked));
  show();
  const setLights = rig.setLights.bind(rig);
  const setWrecked = rig.setWrecked.bind(rig);
  const dispose = rig.dispose.bind(rig);
  rig.setLights = (state) => {
    setLights(state);
    night = state.night;
    show();
  };
  rig.setWrecked = (on) => {
    setWrecked(on);
    wrecked = on;
    show();
  };
  rig.dispose = () => {
    geometry.dispose();
    material.dispose();
    dispose();
  };
  rig.root.userData[BEAMS] = { cones, restore: () => Object.assign(rig, { setLights, setWrecked }) };
}

/** Takes the beams off every car in the scene. */
export function removeBeams(scene: THREE.Scene): void {
  const roots: THREE.Object3D[] = [];
  scene.traverse((o) => o.userData[BEAMS] && roots.push(o));
  for (const root of roots) {
    const { cones, restore } = root.userData[BEAMS] as { cones: THREE.Mesh[]; restore: () => void };
    for (const cone of cones) cone.removeFromParent();
    cones[0].geometry.dispose();
    (cones[0].material as THREE.Material).dispose();
    restore();
    delete root.userData[BEAMS];
  }
}
