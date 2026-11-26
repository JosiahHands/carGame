// The game's textures, drawn with canvas so there are no image files to ship.
import * as THREE from 'three';
import { canvasTexture } from '../libs/canvasTexture';
import { seededRandom } from '../libs/random';
import type { Pattern } from './carConfig';

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

/** The ground at a venue: grass, desert sand, snow, or dark city dirt. */
export function groundTexture(kind: 'grass' | 'sand' | 'snow' | 'dirt'): THREE.CanvasTexture {
  if (kind === 'grass') return grassTexture();
  // Base color, then lots of small specks of two other shades.
  const looks = {
    sand: ['#d8b47a', 'rgba(170,120,70,0.35)', 'rgba(250,225,180,0.4)'],
    snow: ['#eef3f7', 'rgba(170,190,210,0.35)', 'rgba(255,255,255,0.6)'],
    dirt: ['#2a2b30', 'rgba(60,60,70,0.45)', 'rgba(15,15,20,0.5)'],
  }[kind];
  const rand = seededRandom(kind.length * 13);
  return canvasTexture(256, 256, (ctx) => {
    ctx.fillStyle = looks[0];
    ctx.fillRect(0, 0, 256, 256);
    for (let i = 0; i < 2400; i++) {
      ctx.fillStyle = looks[1 + (i % 2)];
      ctx.fillRect(rand() * 256, rand() * 256, 1 + rand() * 3, 1 + rand() * 2);
    }
  }, true);
}

/** Road running along the texture's V axis, with edge lines and a dashed center line. */
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

/**
 * A paint pattern for the side of the car, as if looking at it side on: the
 * left edge is the back of the car, the right edge the front, the top edge the
 * top of the body. `length` and `height` are the body's size in meters, so
 * checks and dots come out square and round.
 */
export function liveryTexture(pattern: Pattern, base: string, accent: string, length: number, height: number): THREE.CanvasTexture {
  const width = 1024;
  const ppm = width / length; // pixels per meter
  const canvasHeight = Math.max(32, Math.min(1024, Math.round(height * ppm)));
  const rand = seededRandom(9);
  const texture = canvasTexture(width, canvasHeight, (ctx) => {
    const h = canvasHeight;
    ctx.fillStyle = base;
    ctx.fillRect(0, 0, width, h);
    ctx.fillStyle = accent;
    switch (pattern) {
      case 'twoTone':
        // The lower part of the body in the accent color.
        ctx.fillRect(0, h * 0.55, width, h);
        break;
      case 'checker': {
        const square = 0.25 * ppm;
        for (let y = 0; y * square < h; y++) {
          for (let x = 0; x * square < width; x++) if ((x + y) % 2 === 0) ctx.fillRect(x * square, y * square, square, square);
        }
        break;
      }
      case 'dots': {
        const gap = 0.3 * ppm;
        for (let y = 0; y * gap < h + gap; y++) {
          for (let x = 0; x * gap < width + gap; x++) {
            ctx.beginPath();
            ctx.arc(x * gap + (y % 2) * gap * 0.5, y * gap, 0.07 * ppm, 0, Math.PI * 2);
            ctx.fill();
          }
        }
        break;
      }
      case 'camo': {
        // Overlapping blobs in the accent color and a darker shade of the paint.
        const dark = '#' + new THREE.Color(base).multiplyScalar(0.55).getHexString();
        for (let i = 0; i < length * 40; i++) {
          ctx.fillStyle = i % 2 === 0 ? accent : dark;
          ctx.beginPath();
          ctx.ellipse(rand() * width, rand() * h, (0.12 + rand() * 0.25) * ppm, (0.08 + rand() * 0.15) * ppm, rand() * Math.PI, 0, Math.PI * 2);
          ctx.fill();
        }
        break;
      }
      case 'tiger':
        // Pointed stripes reaching in from the top and the bottom, in turn.
        for (let x = 0.15 * ppm; x < width; x += 0.32 * ppm) {
          const fromTop = Math.round(x / (0.32 * ppm)) % 2 === 0;
          const tipX = x + (rand() - 0.5) * 0.2 * ppm;
          const reach = h * (0.45 + rand() * 0.35);
          ctx.beginPath();
          ctx.moveTo(x - 0.06 * ppm, fromTop ? 0 : h);
          ctx.lineTo(tipX, fromTop ? reach : h - reach);
          ctx.lineTo(x + 0.06 * ppm, fromTop ? 0 : h);
          ctx.fill();
        }
        break;
      case 'flames': {
        // Tongues of flame licking back from the front of the car: the accent
        // color with a yellow core.
        const tongue = (y: number, reach: number, thick: number, color: string) => {
          ctx.fillStyle = color;
          ctx.beginPath();
          ctx.moveTo(width, y - thick);
          ctx.bezierCurveTo(width - reach * 0.4, y - thick * 1.4, width - reach * 0.7, y + thick * 0.6, width - reach, y - thick * 0.3);
          ctx.bezierCurveTo(width - reach * 0.7, y + thick * 1.2, width - reach * 0.4, y + thick * 1.4, width, y + thick);
          ctx.fill();
        };
        for (let i = 0; i < 6; i++) {
          const y = h * (0.3 + i * 0.09);
          const reach = width * (0.35 + rand() * 0.3);
          tongue(y, reach, h * 0.09, accent);
          tongue(y, reach * 0.6, h * 0.04, '#ffd23f');
        }
        break;
      }
      case 'none':
        break;
    }
  });
  return texture;
}

/** A white race-number roundel with a black number, for the doors. */
export function raceNumberTexture(n: number): THREE.CanvasTexture {
  return canvasTexture(128, 128, (ctx) => {
    ctx.fillStyle = '#f4f1e6';
    ctx.beginPath();
    ctx.arc(64, 64, 60, 0, Math.PI * 2);
    ctx.fill();
    ctx.lineWidth = 6;
    ctx.strokeStyle = '#1b1b1b';
    ctx.stroke();
    ctx.fillStyle = '#1b1b1b';
    ctx.font = 'bold 72px "Arial Black", Arial, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(String(n), 64, 68, 100);
  });
}
