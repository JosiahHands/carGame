// Fast rays: laying stripes, plates and exhausts onto a car body takes hundreds
// of rays per car, and a plain ray tests every triangle of a detailed body. A
// bounding volume hierarchy (three-mesh-bvh) sorts the triangles into nested
// boxes, so a ray only tests the few near it. Each shape is sorted once: the
// real models' shapes are shared by every car that wears them.
import type * as THREE from 'three';
import { acceleratedRaycast, computeBoundsTree } from 'three-mesh-bvh';

/** Makes rays against these meshes fast (sorting each shape the first time). */
export function fastRays(meshes: Iterable<THREE.Mesh>): void {
  for (const mesh of meshes) {
    const geometry = mesh.geometry as THREE.BufferGeometry & { boundsTree?: unknown };
    if (!geometry.boundsTree) computeBoundsTree.call(geometry);
    mesh.raycast = acceleratedRaycast;
  }
}
