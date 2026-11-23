// Generic helpers for drawing textures with a 2D canvas, so a game can ship
// without image files. Nothing in here knows about the car game.
import * as THREE from 'three';

/** Draws onto a new canvas and wraps it as an sRGB texture. */
export function canvasTexture(
  width: number,
  height: number,
  draw: (ctx: CanvasRenderingContext2D) => void,
  repeat = false,
): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('2D canvas is not available');
  draw(ctx);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 8;
  if (repeat) texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
  return texture;
}

/** A vertical gradient, top to bottom. Handy for skies. */
export function gradientTexture(top: string, bottom: string): THREE.CanvasTexture {
  return canvasTexture(4, 256, (ctx) => {
    const grad = ctx.createLinearGradient(0, 0, 0, 256);
    grad.addColorStop(0, top);
    grad.addColorStop(1, bottom);
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, 4, 256);
  });
}

/** A soft white radial blob that fades to transparent. Handy for glows. */
export function radialGlowTexture(size = 128): THREE.CanvasTexture {
  return canvasTexture(size, size, (ctx) => {
    const c = size / 2;
    const grad = ctx.createRadialGradient(c, c, size / 32, c, c, c);
    grad.addColorStop(0, 'rgba(255,255,255,0.9)');
    grad.addColorStop(1, 'rgba(255,255,255,0)');
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, size, size);
  });
}
