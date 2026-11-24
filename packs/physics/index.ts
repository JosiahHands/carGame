// 🧪 Real Physics: realistic driving for every car (see model.ts), crashes
// that spin the car and dent its bodywork, and a telemetry panel that shows
// the grip at work.
import * as THREE from 'three';
import { CarPhysics } from '../../game/carPhysics';
import type { CarRig } from '../../game/carModel';
import type { GameApi, Pack, PackInstance } from '../../game/packs';
import { RealisticDynamics, stateOf, type Assists } from './model';
import { Telemetry } from './telemetry';

interface Options {
  grip: 'road' | 'sport' | 'race';
  assists: Assists;
  telemetry: boolean;
  dents: boolean;
}

const DEFAULTS: Options = { grip: 'sport', assists: 'some', telemetry: true, dents: true };
const GRIP = { road: 0.95, sport: 1.2, race: 1.45 };

/** A dent waiting to be pressed into the player's car: where it was hit, in the car's own directions. */
interface Dent {
  along: number;
  across: number;
  nx: number;
  nz: number;
  lost: number;
}

const pack: Pack = {
  id: 'physics',
  name: 'Real Physics',
  icon: '🧪',
  about:
    'Realistic driving: four tyres with real grip curves, weight that shifts under braking and cornering, an engine with a torque curve and gears, ' +
    'and different grip on every surface. Glancing crashes spin the car and dent it, and a telemetry panel shows the tyres at work.',
  activate(game: GameApi): PackInstance {
    const options: Options = { ...DEFAULTS, ...game.load<Partial<Options>>('options', {}) };
    const model = new RealisticDynamics({ grip: GRIP[options.grip], assists: options.assists, player: game.player });
    const dents: Dent[] = [];
    // Every car's model, and each car's driving state, start fresh.
    const baseImpact = model.impact.bind(model);
    model.impact = (car, nx, nz, along, across, lost) => {
      baseImpact(car, nx, nz, along, across, lost);
      if (car === game.player && options.dents && lost > 4) dents.push({ along, across, nx, nz, lost });
    };
    CarPhysics.dynamics = model;
    game.player.dynamicsState = null;
    const telemetry = new Telemetry(game.overlay);
    const dentedMeshes = new Map<THREE.Mesh, THREE.BufferGeometry>(); // the original shape of each dented part
    let dentedRig: CarRig | null = null;

    const save = () => {
      model.options.grip = GRIP[options.grip];
      model.options.assists = options.assists;
      game.save('options', options);
    };

    /** Puts every dented part back to its original shape. */
    const repair = () => {
      for (const [mesh, original] of dentedMeshes) {
        mesh.geometry.dispose();
        mesh.geometry = original;
      }
      dentedMeshes.clear();
      dentedRig = null;
    };

    /**
     * Presses a dent into the car's body: the panels near where it was hit are
     * pushed in, most at the centre and fading out towards the edge.
     */
    const pressDent = (rig: CarRig, dent: Dent) => {
      if (dentedRig !== rig) {
        // A new car model: the old one's dents went with it.
        dentedMeshes.clear();
        dentedRig = rig;
      }
      const size = Math.max(rig.length / 4.4, 0.2);
      const radius = (0.55 + Math.min(1, dent.lost / 25) * 0.6) * size;
      const depth = Math.min(0.22, 0.03 + dent.lost * 0.006) * size;
      rig.root.updateMatrixWorld(true);
      const centre = rig.root.localToWorld(new THREE.Vector3(dent.across, rig.height * 0.45, dent.along));
      // The push goes into the car (the crash normal points away from what it hit).
      const push = new THREE.Vector3(dent.nx, -0.15, dent.nz).normalize().multiplyScalar(depth);
      const point = new THREE.Vector3();
      const inverse = new THREE.Matrix4();
      rig.body.traverse((object) => {
        const mesh = object as THREE.Mesh;
        if (!mesh.isMesh || !(mesh.geometry instanceof THREE.BufferGeometry)) return;
        const position = mesh.geometry.getAttribute('position') as THREE.BufferAttribute | undefined;
        if (!position || position.count > 20000) return;
        // A dented part gets its own copy of the shape, so other cars sharing it aren't dented too.
        if (!dentedMeshes.has(mesh)) {
          dentedMeshes.set(mesh, mesh.geometry);
          mesh.geometry = mesh.geometry.clone();
        }
        const pos = mesh.geometry.getAttribute('position') as THREE.BufferAttribute;
        inverse.copy(mesh.matrixWorld).invert();
        // The push in the part's own coordinates (parts can be scaled and turned).
        const from = centre.clone().applyMatrix4(inverse);
        const localPush = centre.clone().add(push).applyMatrix4(inverse).sub(from);
        let changed = false;
        for (let i = 0; i < pos.count; i++) {
          point.fromBufferAttribute(pos, i).applyMatrix4(mesh.matrixWorld);
          const d = point.distanceTo(centre);
          if (d >= radius) continue;
          const falloff = (1 - d / radius) ** 2;
          pos.setXYZ(i, pos.getX(i) + localPush.x * falloff, pos.getY(i) + localPush.y * falloff, pos.getZ(i) + localPush.z * falloff);
          changed = true;
        }
        if (changed) {
          pos.needsUpdate = true;
          mesh.geometry.computeBoundingSphere();
        }
      });
    };

    return {
      frame() {
        const driving = game.mode() === 'drive';
        // Dents are pressed in outside the physics step (it runs many times a frame).
        const rig = game.playerCar();
        if (driving) for (const dent of dents.splice(0)) pressDent(rig, dent);
        else dents.length = 0;
        // The garage always shows the car as new.
        if (!driving && dentedMeshes.size > 0) repair();
        telemetry.setVisible(driving && options.telemetry && !game.paused());
        if (driving && options.telemetry) telemetry.draw(stateOf(game.player), game.player, options.grip);
      },
      settings(panel) {
        panel.choice('Tyre grip', [
          { value: 'road', label: 'Road' },
          { value: 'sport', label: 'Sport' },
          { value: 'race', label: 'Race' },
        ], () => options.grip, (v) => ((options.grip = v), save()));
        panel.choice('Driving aids', [
          { value: 'full', label: 'Full' },
          { value: 'some', label: 'Some' },
          { value: 'off', label: 'Off' },
        ], () => options.assists, (v) => ((options.assists = v), save()));
        panel.hint('Driving aids are traction control, ABS and stability control, plus a wheel that helps catch slides. Rivals always have them.');
        panel.choice('Telemetry panel', [
          { value: 'on', label: 'On' },
          { value: 'off', label: 'Off' },
        ], () => (options.telemetry ? 'on' : 'off'), (v) => ((options.telemetry = v === 'on'), save()));
        panel.choice('Crash dents', [
          { value: 'on', label: 'On' },
          { value: 'off', label: 'Off' },
        ], () => (options.dents ? 'on' : 'off'), (v) => ((options.dents = v === 'on'), save()));
      },
      dispose() {
        CarPhysics.dynamics = null;
        repair();
        telemetry.dispose();
      },
    };
  },
};

export default pack;
