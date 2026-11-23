// The game's textures, drawn with canvas so there are no image files to ship.
import * as THREE from 'three';
import { canvasTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';

export function grassTexture(): THREE.CanvasTexture {
  const rand = seededRandom(7);
  return canvasTexture(256, 256, (ctx) => {
    ctx.fillStyle = '#4f7d3a';
    ctx.fillRect(0, 0, 256, 256);
    for (let i = 0; i < 2600; i++) {
      const g = 90 + Math.floor(rand() * 60);
      ctx.fillStyle = `rgba(${g - 40},${g + 20},${g - 60},0.35)`;
      ctx.fillRect(rand() * 256, rand() * 256, 2, 2 + rand() * 3);
    }
  }, true);
}

/** Road running along the texture's V axis, with edge lines and a dashed centre line. */
export function roadTexture(): THREE.CanvasTexture {
  const rand = seededRandom(11);
  return canvasTexture(128, 256, (ctx) => {
    ctx.fillStyle = '#34363b';
    ctx.fillRect(0, 0, 128, 256);
    for (let i = 0; i < 900; i++) {
      const v = 40 + Math.floor(rand() * 30);
      ctx.fillStyle = `rgba(${v},${v},${v + 4},0.5)`;
      ctx.fillRect(rand() * 128, rand() * 256, 1.5, 1.5);
    }
    ctx.fillStyle = '#e8e8e8';
    ctx.fillRect(6, 0, 4, 256);
    ctx.fillRect(118, 0, 4, 256);
    ctx.fillStyle = '#f2c94c';
    ctx.fillRect(61, 0, 6, 128);
  }, true);
}

export function concreteTexture(): THREE.CanvasTexture {
  const rand = seededRandom(5);
  return canvasTexture(128, 128, (ctx) => {
    ctx.fillStyle = '#8d8f93';
    ctx.fillRect(0, 0, 128, 128);
    for (let i = 0; i < 700; i++) {
      const v = 120 + Math.floor(rand() * 40);
      ctx.fillStyle = `rgba(${v},${v},${v},0.4)`;
      ctx.fillRect(rand() * 128, rand() * 128, 2, 2);
    }
    ctx.strokeStyle = 'rgba(60,60,60,0.5)';
    ctx.lineWidth = 2;
    ctx.strokeRect(0, 0, 128, 128);
  }, true);
}

/** Building facade: a grid of windows. The lit version is the night-time glow map. */
export function windowTexture(lit: boolean): THREE.CanvasTexture {
  const rand = seededRandom(lit ? 3 : 2);
  return canvasTexture(128, 128, (ctx) => {
    ctx.fillStyle = lit ? '#000000' : '#ffffff';
    ctx.fillRect(0, 0, 128, 128);
    for (let y = 0; y < 4; y++) {
      for (let x = 0; x < 4; x++) {
        if (lit) {
          const on = rand() < 0.45;
          ctx.fillStyle = on ? `hsl(${40 + rand() * 20}, 90%, ${55 + rand() * 20}%)` : '#000000';
        } else {
          ctx.fillStyle = '#5c6b7a';
        }
        ctx.fillRect(x * 32 + 6, y * 32 + 7, 20, 18);
      }
    }
  }, true);
}

export function plateTexture(text: string): THREE.CanvasTexture {
  return canvasTexture(256, 96, (ctx) => {
    ctx.fillStyle = '#f4f1e6';
    ctx.fillRect(0, 0, 256, 96);
    ctx.strokeStyle = '#1b1b1b';
    ctx.lineWidth = 6;
    ctx.strokeRect(4, 4, 248, 88);
    ctx.fillStyle = '#1b1b1b';
    ctx.font = 'bold 56px "Arial Black", Arial, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(text || ' ', 128, 52, 232);
  });
}

export function carbonTexture(): THREE.CanvasTexture {
  const tex = canvasTexture(64, 64, (ctx) => {
    ctx.fillStyle = '#151515';
    ctx.fillRect(0, 0, 64, 64);
    for (let y = 0; y < 8; y++) {
      for (let x = 0; x < 8; x++) {
        ctx.fillStyle = (x + y) % 2 === 0 ? '#2a2a2a' : '#0e0e0e';
        ctx.fillRect(x * 8, y * 8, 8, 8);
      }
    }
  }, true);
  tex.repeat.set(4, 4);
  return tex;
}
