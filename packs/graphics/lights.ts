// Real light at night: the street lamps nearest the camera light the road
// around them, and the player's headlights throw a beam ahead of the car.
// (The game's lamps only glow; they don't light anything.)
import * as THREE from 'three';
import type { World } from '../../game/world';
import type { CarPhysics } from '../../game/carPhysics';

const LAMPS = 6; // a fixed number, so turning them on and off never rebuilds the shaders

export class NightLights {
  private lamps: THREE.PointLight[] = [];
  private beam: THREE.SpotLight;
  private group = new THREE.Group();
  private timer = 0;

  constructor(scene: THREE.Scene) {
    for (let i = 0; i < LAMPS; i++) {
      const lamp = new THREE.PointLight(0xffc98a, 0, 26, 1.6);
      this.lamps.push(lamp);
      this.group.add(lamp);
    }
    this.beam = new THREE.SpotLight(0xfff4e0, 0, 75, 0.42, 0.55, 1.3);
    this.group.add(this.beam, this.beam.target);
    scene.add(this.group);
  }

  /** Each frame: lamps near the camera switch on, and the headlights follow the car. */
  frame(dt: number, world: World, car: CarPhysics, camera: THREE.Camera, night: boolean, driving: boolean): void {
    // Picking the nearest lamps a few times a second is plenty.
    this.timer -= dt;
    if (this.timer <= 0) {
      this.timer = 0.25;
      const cx = camera.position.x;
      const cz = camera.position.z;
      const lamps = night
        ? world.targets
            .filter((t) => t.kind === 'lamp' && t.alive)
            .map((t) => ({ t, d: (t.x - cx) ** 2 + (t.z - cz) ** 2 }))
            .sort((a, b) => a.d - b.d)
            .slice(0, LAMPS)
        : [];
      this.lamps.forEach((light, i) => {
        const lamp = lamps[i]?.t;
        if (!lamp || lamp.kind !== 'lamp') {
          light.intensity = 0;
          return;
        }
        light.position.set(lamp.x, 6.4 * lamp.size, lamp.z);
        light.distance = 26 * lamp.size;
        light.intensity = 24 * lamp.size;
      });
    }

    // Headlights: from the front of the car, aimed a little down the road.
    const on = night && driving;
    this.beam.intensity = on ? 90 : 0;
    if (on) {
      const fx = Math.sin(car.heading);
      const fz = Math.cos(car.heading);
      const front = car.length / 2;
      this.beam.position.set(car.x + fx * front, car.y + 0.75, car.z + fz * front);
      this.beam.target.position.set(car.x + fx * (front + 22), car.y, car.z + fz * (front + 22));
    }
  }

  dispose(): void {
    this.group.removeFromParent();
    for (const light of [...this.lamps, this.beam]) light.dispose();
  }
}
