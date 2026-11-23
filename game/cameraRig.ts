// Chase, far-chase and bonnet cameras for driving; an orbit camera in the garage.
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

export type CameraMode = 'chase' | 'far' | 'hood';
const MODES: CameraMode[] = ['chase', 'far', 'hood'];

export class CameraRig {
  mode: CameraMode = 'chase';
  eyeHeight = 1.1;
  readonly orbit: OrbitControls;
  private garage = false;
  private lookAt = new THREE.Vector3();
  private desired = new THREE.Vector3();

  constructor(readonly camera: THREE.PerspectiveCamera, dom: HTMLElement) {
    this.orbit = new OrbitControls(camera, dom);
    this.orbit.enableDamping = true;
    this.orbit.enablePan = false;
    this.orbit.minDistance = 4;
    this.orbit.maxDistance = 14;
    this.orbit.maxPolarAngle = Math.PI / 2 - 0.05;
    this.orbit.autoRotate = true;
    this.orbit.autoRotateSpeed = 0.8;
    this.orbit.enabled = false;
  }

  nextMode(): CameraMode {
    this.mode = MODES[(MODES.indexOf(this.mode) + 1) % MODES.length];
    return this.mode;
  }

  setGarage(on: boolean, carPos: THREE.Vector3, heading: number): void {
    this.garage = on;
    this.orbit.enabled = on;
    if (on) {
      this.orbit.target.set(carPos.x, carPos.y + 0.7, carPos.z);
      const a = heading + Math.PI * 0.8;
      this.camera.position.set(carPos.x + Math.sin(a) * 7, carPos.y + 2.2, carPos.z + Math.cos(a) * 7);
      this.orbit.update();
    } else {
      this.snap(carPos, heading);
    }
  }

  /** Jumps straight to the follow position (after a reset). */
  snap(carPos: THREE.Vector3, heading: number): void {
    this.target(carPos, heading);
    this.camera.position.copy(this.desired);
    this.camera.lookAt(this.lookAt);
  }

  private target(carPos: THREE.Vector3, heading: number): void {
    const fx = Math.sin(heading);
    const fz = Math.cos(heading);
    const [dist, height, ahead] = this.mode === 'far' ? [11, 4.2, 4] : this.mode === 'hood' ? [-0.3, this.eyeHeight, 12] : [6.5, 2.4, 3];
    this.desired.set(carPos.x - fx * dist, carPos.y + height, carPos.z - fz * dist);
    this.lookAt.set(carPos.x + fx * ahead, carPos.y + (this.mode === 'hood' ? this.eyeHeight - 0.2 : 1.0), carPos.z + fz * ahead);
  }

  update(dt: number, carPos: THREE.Vector3, heading: number, speed: number): void {
    if (this.garage) {
      this.orbit.update(dt);
      return;
    }
    this.target(carPos, heading);
    if (this.mode === 'hood') {
      this.camera.position.copy(this.desired);
    } else {
      this.camera.position.lerp(this.desired, 1 - Math.exp(-dt * 6));
      // Never dip below the ground.
      this.camera.position.y = Math.max(this.camera.position.y, carPos.y + 0.6);
    }
    this.camera.lookAt(this.lookAt);
    // Widen the view a little at speed for a sense of pace.
    const fov = 62 + Math.min(14, speed * 0.22);
    this.camera.fov += (fov - this.camera.fov) * Math.min(1, dt * 3);
    this.camera.updateProjectionMatrix();
  }
}
