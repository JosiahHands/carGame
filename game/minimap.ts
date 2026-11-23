// A round minimap in the corner that turns with the car (forward is always up).
import { HALF_SIZE, ROAD_LINES, ROAD_WIDTH, type World } from './world';

const SIZE = 180; // CSS pixels
const RANGE = 130; // metres from the centre to the edge

export interface MapMarker {
  x: number;
  z: number;
  color: string;
  size: number;
}

export class Minimap {
  readonly canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private timer = 0;

  constructor() {
    this.canvas = document.createElement('canvas');
    this.canvas.className = 'minimap';
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    this.canvas.width = this.canvas.height = SIZE * dpr;
    this.canvas.style.width = this.canvas.style.height = `${SIZE}px`;
    const ctx = this.canvas.getContext('2d');
    if (!ctx) throw new Error('2D canvas is not available');
    this.ctx = ctx;
    this.ctx.scale(dpr, dpr);
  }

  setVisible(visible: boolean): void {
    this.canvas.classList.toggle('hidden', !visible);
  }

  /** Redraws about 30 times a second. */
  update(dt: number, world: World, x: number, z: number, heading: number, markers: MapMarker[]): void {
    this.timer -= dt;
    if (this.timer > 0) return;
    this.timer = 1 / 30;

    const ctx = this.ctx;
    const k = SIZE / 2 / RANGE; // pixels per metre
    ctx.save();
    ctx.clearRect(0, 0, SIZE, SIZE);
    ctx.beginPath();
    ctx.arc(SIZE / 2, SIZE / 2, SIZE / 2 - 1, 0, Math.PI * 2);
    ctx.clip();
    ctx.fillStyle = '#3d5a35';
    ctx.fillRect(0, 0, SIZE, SIZE);

    // World coordinates to map: forward (+z when heading is 0) is up, and
    // +x is to the left, matching the view from behind the car.
    ctx.translate(SIZE / 2, SIZE / 2);
    ctx.rotate(heading);
    ctx.scale(-k, -k);
    ctx.translate(-x, -z);
    const rect = (cx: number, cz: number, w: number, d: number, color: string) => {
      ctx.fillStyle = color;
      ctx.fillRect(cx - w / 2, cz - d / 2, w, d);
    };

    const inner = 80 - ROAD_WIDTH;
    for (const b of world.blocks) {
      rect(b.x, b.z, inner, inner, b.kind === 'park' ? '#4f7d3f' : b.kind === 'lot' ? '#45474c' : '#8b8f96');
      if (b.kind === 'stunt') rect(b.x, b.z, inner - 6, inner - 6, '#9a9488');
    }
    const len = HALF_SIZE * 2 - 16;
    for (const c of ROAD_LINES) {
      rect(c, 0, ROAD_WIDTH, len, '#2b2d31');
      rect(0, c, len, ROAD_WIDTH, '#2b2d31');
    }
    for (const r of world.ramps) {
      ctx.save();
      ctx.translate(r.x, r.z);
      ctx.rotate(-r.angle);
      rect(0, 0, r.width, r.length, '#e0a03a');
      ctx.restore();
    }
    for (const t of world.targets) {
      if (t.kind === 'building') {
        const color = t.alive ? '#d8d2c6' : t.phase === 'rising' ? '#a8a296' : '#6a5a4a';
        rect(t.x, t.z, t.width, t.depth, color);
      } else if (t.kind === 'car') {
        rect(t.x, t.z, 2.4, 2.4, t.alive ? '#b9c3d6' : '#1a1a1a');
      }
    }
    // Edge walls.
    ctx.strokeStyle = '#b8bcc2';
    ctx.lineWidth = 2 / k;
    ctx.strokeRect(-HALF_SIZE, -HALF_SIZE, HALF_SIZE * 2, HALF_SIZE * 2);

    // Markers, kept on the rim when they are off the map.
    for (const m of markers) {
      let dx = m.x - x;
      let dz = m.z - z;
      const d = Math.hypot(dx, dz);
      const max = RANGE - 8;
      if (d > max) {
        dx *= max / d;
        dz *= max / d;
      }
      ctx.fillStyle = m.color;
      ctx.beginPath();
      ctx.arc(x + dx, z + dz, m.size / k, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();

    // The player: an arrow in the middle, always pointing up.
    ctx.save();
    ctx.translate(SIZE / 2, SIZE / 2);
    ctx.fillStyle = '#ff5a36';
    ctx.strokeStyle = '#ffffff';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(0, -8);
    ctx.lineTo(6, 6);
    ctx.lineTo(0, 3);
    ctx.lineTo(-6, 6);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
    ctx.restore();

    ctx.strokeStyle = 'rgba(255,255,255,0.35)';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(SIZE / 2, SIZE / 2, SIZE / 2 - 1, 0, Math.PI * 2);
    ctx.stroke();
  }
}
