// Exhausts that sit on the body. The game puts its exhaust pipes where its
// own plain body is; when this pack draws a new body (sculpted, or a real
// car model) the pipes are moved onto it, so they never float:
//  - pipes out the back go just inside the bottom edge of the rear bumper,
//    a few centimeters sticking out;
//  - tall stacks stand on the body under them.
// The nitro flames and boost trails at each pipe's tip move with it.
import * as THREE from 'three';
import type { CarRig } from '../../game/carModel';

/** Something moved, and where it was (to put it back). */
export interface Moved {
  object: THREE.Object3D;
  position: THREE.Vector3;
}

/** How much of a rear pipe sticks out behind the bumper (meters). */
const STICK_OUT = 0.08;

/** The game's exhaust pipes: open chrome tubes straight on the body (0.3 m out the back, or 1 m tall stacks). */
function pipesOf(rig: CarRig): THREE.Mesh[] {
  return rig.body.children.filter((c): c is THREE.Mesh => {
    const mesh = c as THREE.Mesh;
    if (!mesh.isMesh || mesh.geometry.type !== 'CylinderGeometry') return false;
    const { openEnded, height } = (mesh.geometry as THREE.CylinderGeometry).parameters;
    return openEnded && (height === 0.3 || height === 1);
  });
}

/** The nitro flames: groups of two cones straight on the body, one at each pipe's tip. */
function flamesOf(rig: CarRig): THREE.Object3D[] {
  return rig.body.children.filter(
    (c) => !(c as THREE.Mesh).isMesh && c.children.length === 2 && c.children.every((m) => (m as THREE.Mesh).geometry?.type === 'ConeGeometry'),
  );
}

const ray = new THREE.Raycaster();

/**
 * Moves the car's exhaust pipes onto `surfaces` (the body that's showing now,
 * already on the car). Each move is added to `moved`, so it can be undone.
 * Options: `rear`, just the back of the body, to find the rear bumper on
 * quicker; `stacksOnly`, to leave the pipes out the back where they are.
 */
export function seatExhausts(rig: CarRig, surfaces: THREE.Object3D[], moved: Moved[], options: { rear?: THREE.Object3D[]; stacksOnly?: boolean } = {}): void {
  const { rear = surfaces, stacksOnly = false } = options;
  const pipes = pipesOf(rig).filter((p) => !stacksOnly || (p.geometry as THREE.CylinderGeometry).parameters.height === 1);
  if (pipes.length === 0) return;
  // Rays are worked out in the car body's coordinates and cast in the world's.
  rig.root.updateMatrixWorld(true);
  const toWorld = rig.body.matrixWorld;
  const toBody = toWorld.clone().invert();
  const cast = (from: THREE.Vector3, direction: THREE.Vector3, on = surfaces): THREE.Vector3 | null => {
    ray.set(from.clone().applyMatrix4(toWorld), direction.clone().transformDirection(toWorld));
    const hit = ray.intersectObjects(on, true)[0];
    return hit ? hit.point.applyMatrix4(toBody) : null;
  };
  const forward = new THREE.Vector3(0, 0, 1);
  const down = new THREE.Vector3(0, -1, 0);
  const { spec, archR } = rig.shape;
  // Anything further forward than `limit` is under the car, not the rear bumper.
  const behindWheels = spec.axleRear - archR;
  /** Where the body's rear face is at a height (looking forward from far behind), if it's there. */
  const rearAt = (x: number, y: number, limit: number): number | null => {
    const hit = cast(new THREE.Vector3(x, y, spec.axleRear - 20), forward, rear);
    return hit && hit.z < limit ? hit.z : null;
  };
  /** The bottom edge of the rear bumper above (or just below) a point: every 5 cm going up, then closer in. */
  const bumperBottom = (x: number, y: number, limit: number): number => {
    let low = y - 0.3;
    let high = NaN;
    for (let up = low; up <= y + 1.5; up += 0.05) {
      if (rearAt(x, up, limit) !== null) {
        high = up;
        break;
      }
      low = up;
    }
    if (Number.isNaN(high)) return NaN;
    for (let i = 0; i < 5; i++) {
      const mid = (low + high) / 2;
      if (rearAt(x, mid, limit) !== null) high = mid;
      else low = mid;
    }
    return high;
  };
  const others = [...flamesOf(rig), ...rig.exhausts];

  for (const pipe of pipes) {
    const { height, radiusTop: r } = (pipe.geometry as THREE.CylinderGeometry).parameters;
    const at = pipe.position;
    const to = at.clone();
    if (height < 1) {
      // Out the back: tucked just inside the bottom edge of the rear bumper.
      // (Wheels so big there's no bumper behind them: whatever's behind the axle.)
      let limit = behindWheels;
      let bottom = bumperBottom(at.x, at.y, limit);
      if (Number.isNaN(bottom)) bottom = bumperBottom(at.x, at.y, (limit = spec.axleRear));
      if (Number.isNaN(bottom)) continue; // nothing there at all: leave it be
      to.y = bottom + r + 0.015;
      // The pipe goes into the bumper (at its middle and its top) and sticks out a little.
      const faces = [rearAt(at.x, to.y, limit), rearAt(at.x, to.y + r * 0.9, limit)].filter((z): z is number => z !== null);
      if (faces.length === 0) continue;
      to.z = Math.min(...faces) - STICK_OUT + height / 2;
    } else {
      // A stack: stands on the body under it. Over nothing (past the tail),
      // it moves forward until it's over the body.
      for (let z = at.z; z < at.z + 1.5; z += 0.05) {
        const hit = cast(new THREE.Vector3(at.x, at.y + 10, z), down);
        if (!hit) continue;
        to.z = z === at.z ? z : z + 0.05;
        to.y = (cast(new THREE.Vector3(at.x, at.y + 10, to.z), down) ?? hit).y - 0.02 + height / 2;
        break;
      }
    }
    // The pipe, and its flames and boost trail with it.
    const dy = to.y - at.y;
    const dz = to.z - at.z;
    if (Math.abs(dy) < 1e-4 && Math.abs(dz) < 1e-4) continue;
    for (const object of [pipe, ...others.filter((o) => Math.abs(o.position.x - at.x) < 1e-4)]) {
      moved.push({ object, position: object.position.clone() });
      object.position.y += dy;
      object.position.z += dz;
    }
  }
}

/** Puts back everything `seatExhausts` moved, last move first. */
export function unseatExhausts(moved: Moved[]): void {
  for (let i = moved.length - 1; i >= 0; i--) moved[i].object.position.copy(moved[i].position);
  moved.length = 0;
}
